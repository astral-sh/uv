//! PEP 514 interactions with the Windows registry.

use crate::managed::ManagedPythonInstallation;
use crate::platform::Arch;
use crate::{PythonInstallationKey, PythonVersion, COMPANY};
use std::cmp::Ordering;
use std::path::PathBuf;
use std::str::FromStr;
use target_lexicon::PointerWidth;
use thiserror::Error;
use tracing::debug;
use windows_registry::{Key, Value, CURRENT_USER, HSTRING, LOCAL_MACHINE};

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
    for root_key in [CURRENT_USER, LOCAL_MACHINE] {
        let Ok(key_python) = root_key.open(r"Software\Python") else {
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
    let company = CURRENT_USER.create(format!("Software\\Python\\{COMPANY}"))?;
    company.set_string("DisplayName", "Astral")?;
    company.set_string("SupportUrl", "https://github.com/astral-sh/uv")?;

    // Ex) Cpython3.13.1
    let python_tag = format!(
        "{}{}",
        installation.key().implementation().pretty(),
        installation.key().version()
    );
    let tag = company.create(&python_tag)?;
    let display_name = format!(
        "{} {} ({}-bit)",
        installation.key().implementation().pretty(),
        installation.key().version(),
        pointer_width
    );
    tag.set_string("DisplayName", &display_name)?;
    tag.set_string("SupportUrl", "https://github.com/astral-sh/uv")?;
    tag.set_string("Version", &installation.key().version().to_string())?;
    tag.set_string("SysVersion", &installation.key().sys_version())?;
    tag.set_string("SysArchitecture", &format!("{pointer_width}bit"))?;
    // Store python build standalone release
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
