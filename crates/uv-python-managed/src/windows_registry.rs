//! Register managed Python installations in the Windows registry following PEP 514.

use std::collections::HashSet;

use target_lexicon::PointerWidth;
use thiserror::Error;
use tracing::debug;
use uv_platform::Arch;
use uv_warnings::{warn_user, warn_user_once};
use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_KEY_DELETED};
use windows::core::HRESULT;
use windows_registry::{CURRENT_USER, HSTRING, Value};

use crate::managed::ManagedPythonInstallation;
use uv_python_types::PythonInstallationKey;

const COMPANY_KEY: &str = "Astral";
const COMPANY_DISPLAY_NAME: &str = "Astral Software Inc.";

#[derive(Debug, Error)]
pub enum ManagedPep514Error {
    #[error("Windows has an unknown pointer width for arch: `{_0}`")]
    InvalidPointerSize(Arch),
    #[error("Failed to write registry entry: {0}")]
    WriteError(#[from] windows::core::Error),
    #[error("Failed to clear registry entries under HKCU:\\{key}: {source}")]
    RemoveError {
        key: String,
        #[source]
        source: windows::core::Error,
    },
}

/// Register a managed Python installation in the Windows registry following PEP 514.
pub fn create_registry_entry(
    installation: &ManagedPythonInstallation,
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

    write_registry_entry(installation, pointer_width)?;

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
    // Include the variant's executable suffix (e.g., "t" for freethreaded) in the
    // registry tag so that variant (freethreaded, debug, etc.) installations of the same version
    // get distinct registry entries. This suffix can be empty.
    //
    // See: https://github.com/astral-sh/uv/issues/18795
    let variant_suffix = key.variant().executable_suffix();
    format!(
        "{}{}{}",
        key.implementation().pretty(),
        key.version(),
        variant_suffix,
    )
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
            if err.code() == HRESULT::from(ERROR_FILE_NOT_FOUND)
                || err.code() == HRESULT::from(ERROR_KEY_DELETED)
            {
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
            if err.code() == HRESULT::from(ERROR_FILE_NOT_FOUND)
                || err.code() == HRESULT::from(ERROR_KEY_DELETED)
            {
                debug!(
                    "No registry entries to remove for {}, no registry key {}",
                    installation.key(),
                    python_entry
                );
            } else {
                errors.push((
                    installation.key().clone(),
                    ManagedPep514Error::RemoveError {
                        key: python_entry,
                        source: err,
                    }
                    .into(),
                ));
            }
        }
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
        Err(err)
            if err.code() == HRESULT::from(ERROR_FILE_NOT_FOUND)
                || err.code() == HRESULT::from(ERROR_KEY_DELETED) =>
        {
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
        Err(err)
            if err.code() == HRESULT::from(ERROR_FILE_NOT_FOUND)
                || err.code() == HRESULT::from(ERROR_KEY_DELETED) =>
        {
            return;
        }
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
            if err.code() == HRESULT::from(ERROR_FILE_NOT_FOUND)
                || err.code() == HRESULT::from(ERROR_KEY_DELETED)
            {
                continue;
            }
            // TODO(konsti): We don't have an installation key here.
            warn_user_once!("Failed to remove orphan registry key HKCU:\\{python_entry}: {err}");
        }
    }
}
