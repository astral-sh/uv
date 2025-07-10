//! Windows-specific utilities for manipulating the environment.
//!
//! Based on rustup's Windows implementation: <https://github.com/rust-lang/rustup/commit/bce3ed67d219a2b754857f9e231287794d8c770d>

#![cfg(windows)]

use std::path::Path;

use anyhow::Context;
use tracing::warn;
use windows_registry::{CURRENT_USER, HSTRING};
use windows_result::HRESULT;
use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_INVALID_DATA};

use uv_static::EnvVars;

/// Append the given [`Path`] to the `PATH` environment variable in the Windows registry.
///
/// Returns `Ok(true)` if the path was successfully appended, and `Ok(false)` if the path was
/// already in `PATH`.
pub fn prepend_path(path: &Path) -> anyhow::Result<bool> {
    // Get the existing `PATH` variable from the registry.
    let windows_path = get_windows_path_var()?;

    // Add the new path to the existing `PATH` variable.
    let windows_path =
        windows_path.and_then(|windows_path| prepend_to_path(&windows_path, HSTRING::from(path)));
    // If the path didn't change, then we don't need to do anything.
    let Some(windows_path) = windows_path else {
        return Ok(false);
    };

    // Set the `PATH` variable in the registry.
    apply_windows_path_var(&windows_path)?;

    Ok(true)
}

/// Set the windows `PATH` variable in the registry.
fn apply_windows_path_var(path: &HSTRING) -> anyhow::Result<()> {
    let environment = CURRENT_USER.create("Environment")?;

    if path.is_empty() {
        environment.remove_value(EnvVars::PATH)?;
    } else {
        environment.set_expand_hstring(EnvVars::PATH, path)?;
    }

    Ok(())
}

/// Retrieve the windows `PATH` variable from the registry.
///
/// Returns `Ok(None)` if the `PATH` variable is not a string.
fn get_windows_path_var() -> anyhow::Result<Option<HSTRING>> {
    let environment = CURRENT_USER
        .create("Environment")
        .context("Failed to open `Environment` key")?;

    let reg_value = environment.get_hstring(EnvVars::PATH);
    match reg_value {
        Ok(reg_value) => Ok(Some(reg_value)),
        Err(err) if err.code() == HRESULT::from_win32(ERROR_INVALID_DATA) => {
            warn!("`HKEY_CURRENT_USER\\Environment\\PATH` is a non-string");
            Ok(None)
        }
        Err(err) if err.code() == HRESULT::from_win32(ERROR_FILE_NOT_FOUND) => {
            Ok(Some(HSTRING::new()))
        }
        Err(err) => Err(err.into()),
    }
}

/// Prepend a path to the `PATH` variable in the Windows registry.
///
/// Returns `Ok(None)` if the given path is already in `PATH`.
fn prepend_to_path(existing_path: &HSTRING, path: HSTRING) -> Option<HSTRING> {
    if existing_path.is_empty() {
        Some(path)
    } else if existing_path.windows(path.len()).any(|p| *p == *path) {
        None
    } else {
        let mut new_path = path.to_os_string();
        new_path.push(";");
        new_path.push(existing_path.to_os_string());
        Some(HSTRING::from(new_path))
    }
}
