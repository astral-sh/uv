//! Windows-specific utilities for manipulating the environment.
//!
//! Based on rustup's Windows implementation: <https://github.com/rust-lang/rustup/blob/fede22fea7b160868cece632bd213e6d72f8912f/src/cli/self_update/windows.rs>

#![cfg(windows)]

use std::ffi::OsString;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::slice;

use anyhow::Context;
use winreg::enums::{RegType, HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};
use winreg::{RegKey, RegValue};

/// Append the given [`Path`] to the `PATH` environment variable in the Windows registry.
///
/// Returns `Ok(true)` if the path was successfully appended, and `Ok(false)` if the path was
/// already in `PATH`.
pub fn prepend_path(path: &Path) -> anyhow::Result<bool> {
    // Get the existing `PATH` variable from the registry.
    let windows_path = get_windows_path_var()?;

    // Add the new path to the existing `PATH` variable.
    let windows_path = windows_path.and_then(|windows_path| {
        prepend_to_path(windows_path, OsString::from(path).encode_wide().collect())
    });

    // If the path didn't change, then we don't need to do anything.
    let Some(windows_path) = windows_path else {
        return Ok(false);
    };

    // Set the `PATH` variable in the registry.
    apply_windows_path_var(windows_path)?;

    Ok(true)
}

/// Set the windows `PATH` variable in the registry.
fn apply_windows_path_var(path: Vec<u16>) -> anyhow::Result<()> {
    let root = RegKey::predef(HKEY_CURRENT_USER);
    let environment = root.open_subkey_with_flags("Environment", KEY_READ | KEY_WRITE)?;

    if path.is_empty() {
        environment.delete_value("PATH")?;
    } else {
        let reg_value = RegValue {
            bytes: to_winreg_bytes(path),
            vtype: RegType::REG_EXPAND_SZ,
        };
        environment.set_raw_value("PATH", &reg_value)?;
    }

    Ok(())
}

/// Retrieve the windows `PATH` variable from the registry.
///
/// Returns `Ok(None)` if the `PATH` variable is not a string.
fn get_windows_path_var() -> anyhow::Result<Option<Vec<u16>>> {
    let root = RegKey::predef(HKEY_CURRENT_USER);
    let environment = root
        .open_subkey_with_flags("Environment", KEY_READ | KEY_WRITE)
        .context("Failed to open `Environment` key")?;

    let reg_value = environment.get_raw_value("PATH");
    match reg_value {
        Ok(reg_value) => {
            if let Some(reg_value) = from_winreg_value(&reg_value) {
                Ok(Some(reg_value))
            } else {
                tracing::warn!("`HKEY_CURRENT_USER\\Environment\\PATH` is a non-string");
                Ok(None)
            }
        }
        Err(ref err) if err.kind() == io::ErrorKind::NotFound => Ok(Some(Vec::new())),
        Err(err) => Err(err.into()),
    }
}

/// Prepend a path to the `PATH` variable in the Windows registry.
///
/// Returns `Ok(None)` if the given path is already in `PATH`.
fn prepend_to_path(existing_path: Vec<u16>, path: Vec<u16>) -> Option<Vec<u16>> {
    if existing_path.is_empty() {
        Some(path)
    } else if existing_path.windows(path.len()).any(|p| p == path) {
        None
    } else {
        let mut new_path = path;
        new_path.push(u16::from(b';'));
        new_path.extend(existing_path);
        Some(new_path)
    }
}

/// Convert a vector UCS-2 chars to a null-terminated UCS-2 string in bytes.
fn to_winreg_bytes(mut value: Vec<u16>) -> Vec<u8> {
    value.push(0);
    #[allow(unsafe_code)]
    unsafe {
        slice::from_raw_parts(value.as_ptr().cast::<u8>(), value.len() * 2).to_vec()
    }
}

/// Decode the `HKCU\Environment\PATH` value.
///
/// If the key is not `REG_SZ` or `REG_EXPAND_SZ`, returns `None`.
/// The `winreg` library itself does a lossy unicode conversion.
fn from_winreg_value(val: &RegValue) -> Option<Vec<u16>> {
    match val.vtype {
        RegType::REG_SZ | RegType::REG_EXPAND_SZ => {
            #[allow(unsafe_code)]
            let mut words = unsafe {
                #[allow(clippy::cast_ptr_alignment)]
                slice::from_raw_parts(val.bytes.as_ptr().cast::<u16>(), val.bytes.len() / 2)
                    .to_owned()
            };
            while words.last() == Some(&0) {
                words.pop();
            }
            Some(words)
        }
        _ => None,
    }
}
