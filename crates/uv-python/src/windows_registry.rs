//! PEP 514 interactions with the Windows registry.

use crate::managed::ManagedPythonInstallation;
use crate::platform::Arch;
use crate::{PythonInstallationKey, PythonVersion, COMPANY_DISPLAY_NAME, COMPANY_KEY};
use std::cmp::Ordering;
use std::collections::HashSet;
use std::path::PathBuf;
use std::str::FromStr;
use target_lexicon::PointerWidth;
use thiserror::Error;
use tracing::debug;
use uv_warnings::{warn_user, warn_user_once};
use windows_registry::{Key, Value, CURRENT_USER, HSTRING, LOCAL_MACHINE};
use windows_result::HRESULT;
use windows_sys::Win32::Foundation::ERROR_FILE_NOT_FOUND;
use windows_sys::Win32::System::Registry::{KEY_WOW64_32KEY, KEY_WOW64_64KEY};

/// Code returned when the registry key doesn't exist.
const ERROR_NOT_FOUND: HRESULT = HRESULT::from_win32(ERROR_FILE_NOT_FOUND);

/// A Python interpreter found in the Windows registry through PEP 514 or from a known Microsoft
/// Store path.
///
/// There are a lot more (optional) fields defined in PEP 514, but we only care about path and
/// version here, for everything else we probe with a Python script.
#[derive(Debug, Clone)]
pub(crate) struct WindowsPython {
    pub(crate) path: PathBuf,
    pub(crate) version: Option<PythonVersion>,
}

/// Find all Pythons registered in the Windows registry following PEP 514.
pub(crate) fn registry_pythons() -> Result<Vec<WindowsPython>, windows_result::Error> {
    let mut registry_pythons = Vec::new();
    // Prefer `HKEY_CURRENT_USER` over `HKEY_LOCAL_MACHINE`.
    // By default, a 64-bit program does not see a 32-bit global (HKLM) installation of Python in
    // the registry (https://github.com/astral-sh/uv/issues/11217). To work around this, we manually
    // request both 32-bit and 64-bit access. The flags have no effect on 32-bit
    // (https://stackoverflow.com/a/12796797/3549270).
    for (root_key, access_modifier) in [
        (CURRENT_USER, None),
        (LOCAL_MACHINE, Some(KEY_WOW64_64KEY)),
        (LOCAL_MACHINE, Some(KEY_WOW64_32KEY)),
    ] {
        let mut open_options = root_key.options();
        open_options.read();
        if let Some(access_modifier) = access_modifier {
            open_options.access(access_modifier);
        }
        let Ok(key_python) = open_options.open(r"Software\Python") else {
            continue;
        };
        for company in key_python.keys()? {
            // Reserved name according to the PEP.
            if company == "PyLauncher" {
                continue;
            }
            let Ok(company_key) = key_python.open(&company) else {
                // Ignore invalid entries
                continue;
            };
            for tag in company_key.keys()? {
                let tag_key = company_key.open(&tag)?;

                if let Some(registry_python) = read_registry_entry(&company, &tag, &tag_key) {
                    registry_pythons.push(registry_python);
                }
            }
        }
    }

    // The registry has no natural ordering, so we're processing the latest version first.
    registry_pythons.sort_by(|a, b| {
        match (&a.version, &b.version) {
            // Place entries with a version before those without a version.
            (Some(_), None) => Ordering::Greater,
            (None, Some(_)) => Ordering::Less,
            // We want the highest version on top, which is the inverse from the regular order. The
            // path is an arbitrary but stable tie-breaker.
            (Some(version_a), Some(version_b)) => {
                version_a.cmp(version_b).reverse().then(a.path.cmp(&b.path))
            }
            // Sort the entries without a version arbitrarily, but stable (by path).
            (None, None) => a.path.cmp(&b.path),
        }
    });

    Ok(registry_pythons)
}

fn read_registry_entry(company: &str, tag: &str, tag_key: &Key) -> Option<WindowsPython> {
    // `ExecutablePath` is mandatory for executable Pythons.
    let Ok(executable_path) = tag_key
        .open("InstallPath")
        .and_then(|install_path| install_path.get_value("ExecutablePath"))
        .and_then(String::try_from)
    else {
        debug!(
            r"Python interpreter in the registry is not executable: `Software\Python\{}\{}",
            company, tag
        );
        return None;
    };

    // `SysVersion` is optional.
    let version = tag_key
        .get_value("SysVersion")
        .and_then(String::try_from)
        .ok()
        .and_then(|s| match PythonVersion::from_str(&s) {
            Ok(version) => Some(version),
            Err(err) => {
                debug!(
                    "Skipping Python interpreter ({executable_path}) \
                    with invalid registry version {s}: {err}",
                );
                None
            }
        });

    Some(WindowsPython {
        path: PathBuf::from(executable_path),
        version,
    })
}

#[derive(Debug, Error)]
pub enum ManagedPep514Error {
    #[error("Windows has an unknown pointer width for arch: `{_0}`")]
    InvalidPointerSize(Arch),
}

/// Register a managed Python installation in the Windows registry following PEP 514.
pub fn create_registry_entry(
    installation: &ManagedPythonInstallation,
    errors: &mut Vec<(PythonInstallationKey, anyhow::Error)>,
) -> Result<(), ManagedPep514Error> {
    let pointer_width = match installation.key().arch().family().pointer_width() {
        Ok(PointerWidth::U32) => 32,
        Ok(PointerWidth::U64) => 64,
        _ => {
            return Err(ManagedPep514Error::InvalidPointerSize(
                *installation.key().arch(),
            ));
        }
    };

    if let Err(err) = write_registry_entry(installation, pointer_width) {
        errors.push((installation.key().clone(), err.into()));
    }

    Ok(())
}

fn write_registry_entry(
    installation: &ManagedPythonInstallation,
    pointer_width: i32,
) -> windows_registry::Result<()> {
    // We currently just overwrite all known keys, without removing prior entries first

    // Similar to using the bin directory in HOME on Unix, we only install for the current user
    // on Windows.
    let company = CURRENT_USER.create(format!("Software\\Python\\{COMPANY_KEY}"))?;
    company.set_string("DisplayName", COMPANY_DISPLAY_NAME)?;
    company.set_string("SupportUrl", "https://github.com/astral-sh/uv")?;

    // Ex) CPython3.13.1
    let tag = company.create(registry_python_tag(installation.key()))?;
    let display_name = format!(
        "{} {} ({}-bit)",
        installation.key().implementation().pretty(),
        installation.key().version(),
        pointer_width
    );
    tag.set_string("DisplayName", &display_name)?;
    tag.set_string("SupportUrl", "https://github.com/astral-sh/uv")?;
    tag.set_string("Version", installation.key().version().to_string())?;
    tag.set_string("SysVersion", installation.key().sys_version())?;
    tag.set_string("SysArchitecture", format!("{pointer_width}bit"))?;
    // Store `python-build-standalone` release
    if let Some(url) = installation.url() {
        tag.set_string("DownloadUrl", url)?;
    }
    if let Some(sha256) = installation.sha256() {
        tag.set_string("DownloadSha256", sha256)?;
    }

    let install_path = tag.create("InstallPath")?;
    install_path.set_value(
        "",
        &Value::from(&HSTRING::from(installation.path().as_os_str())),
    )?;
    install_path.set_value(
        "ExecutablePath",
        &Value::from(&HSTRING::from(installation.executable(false).as_os_str())),
    )?;
    install_path.set_value(
        "WindowedExecutablePath",
        &Value::from(&HSTRING::from(installation.executable(true).as_os_str())),
    )?;
    Ok(())
}

fn registry_python_tag(key: &PythonInstallationKey) -> String {
    format!("{}{}", key.implementation().pretty(), key.version())
}

/// Remove requested Python entries from the Windows Registry (PEP 514).
pub fn remove_registry_entry<'a>(
    installations: impl IntoIterator<Item = &'a ManagedPythonInstallation>,
    all: bool,
    errors: &mut Vec<(PythonInstallationKey, anyhow::Error)>,
) {
    let astral_key = format!("Software\\Python\\{COMPANY_KEY}");
    if all {
        debug!("Removing registry key HKCU:\\{}", astral_key);
        if let Err(err) = CURRENT_USER.remove_tree(&astral_key) {
            if err.code() == ERROR_NOT_FOUND {
                debug!("No registry entries to remove, no registry key {astral_key}");
            } else {
                warn_user!("Failed to clear registry entries under {astral_key}: {err}");
            }
        }
        return;
    }

    for installation in installations {
        let python_tag = registry_python_tag(installation.key());
        let python_entry = format!("{astral_key}\\{python_tag}");
        debug!("Removing registry key HKCU:\\{}", python_entry);
        if let Err(err) = CURRENT_USER.remove_tree(&python_entry) {
            if err.code() == ERROR_NOT_FOUND {
                debug!(
                    "No registry entries to remove for {}, no registry key {}",
                    installation.key(),
                    python_entry
                );
            } else {
                errors.push((
                    installation.key().clone(),
                    anyhow::Error::new(err)
                        .context("Failed to clear registry entries under HKCU:\\{python_entry}"),
                ));
            }
        };
    }
}

/// Remove Python entries from the Windows Registry (PEP 514) that are not matching any
/// installation.
pub fn remove_orphan_registry_entries(installations: &[ManagedPythonInstallation]) {
    let keep: HashSet<_> = installations
        .iter()
        .map(|installation| registry_python_tag(installation.key()))
        .collect();
    let astral_key = format!("Software\\Python\\{COMPANY_KEY}");
    let key = match CURRENT_USER.open(&astral_key) {
        Ok(subkeys) => subkeys,
        Err(err) if err.code() == ERROR_NOT_FOUND => {
            return;
        }
        Err(err) => {
            // TODO(konsti): We don't have an installation key here.
            warn_user_once!("Failed to open HKCU:\\{astral_key}: {err}");
            return;
        }
    };
    // Separate assignment since `keys()` creates a borrow.
    let subkeys = match key.keys() {
        Ok(subkeys) => subkeys,
        Err(err) => {
            // TODO(konsti): We don't have an installation key here.
            warn_user_once!("Failed to list subkeys of HKCU:\\{astral_key}: {err}");
            return;
        }
    };
    for subkey in subkeys {
        if keep.contains(&subkey) {
            continue;
        }
        let python_entry = format!("{astral_key}\\{subkey}");
        debug!("Removing orphan registry key HKCU:\\{}", python_entry);
        if let Err(err) = CURRENT_USER.remove_tree(&python_entry) {
            // TODO(konsti): We don't have an installation key here.
            warn_user_once!("Failed to remove orphan registry key HKCU:\\{python_entry}: {err}");
        };
    }
}
