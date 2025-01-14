use std::{ffi::OsString, path::PathBuf};

use etcetera::BaseStrategy;

use uv_static::EnvVars;

/// Returns an appropriate user-level directory for storing executables.
///
/// This follows, in order:
///
/// - `$OVERRIDE_VARIABLE` (if provided)
/// - `$XDG_BIN_HOME`
/// - `$XDG_DATA_HOME/../bin`
/// - `$HOME/.local/bin`
///
/// On all platforms.
///
/// Returns `None` if a directory cannot be found, i.e., if `$HOME` cannot be resolved. Does not
/// check if the directory exists.
pub fn user_executable_directory(override_variable: Option<&'static str>) -> Option<PathBuf> {
    override_variable
        .and_then(std::env::var_os)
        .and_then(parse_path)
        .or_else(|| std::env::var_os(EnvVars::XDG_BIN_HOME).and_then(parse_path))
        .or_else(|| {
            std::env::var_os(EnvVars::XDG_DATA_HOME)
                .and_then(parse_path)
                .map(|path| path.join("../bin"))
        })
        .or_else(|| {
            let home_dir = etcetera::home_dir().ok();
            home_dir.map(|path| path.join(".local").join("bin"))
        })
}

/// Returns an appropriate user-level directory for storing the cache.
///
/// Corresponds to `$XDG_CACHE_HOME/uv` on Unix.
pub fn user_cache_dir() -> Option<PathBuf> {
    etcetera::base_strategy::choose_base_strategy()
        .ok()
        .map(|dirs| dirs.cache_dir().join("uv"))
}

/// Returns the legacy cache directory path.
///
/// Uses `/Users/user/Library/Application Support/uv` on macOS, in contrast to the new preference
/// for using the XDG directories on all Unix platforms.
pub fn legacy_user_cache_dir() -> Option<PathBuf> {
    etcetera::base_strategy::choose_native_strategy()
        .ok()
        .map(|dirs| dirs.cache_dir().join("uv"))
        .map(|dir| {
            if cfg!(windows) {
                dir.join("cache")
            } else {
                dir
            }
        })
}

/// Returns an appropriate user-level directory for storing application state.
///
/// Corresponds to `$XDG_DATA_HOME/uv` on Unix.
pub fn user_state_dir() -> Option<PathBuf> {
    etcetera::base_strategy::choose_base_strategy()
        .ok()
        .map(|dirs| dirs.data_dir().join("uv"))
}

/// Returns the legacy state directory path.
///
/// Uses `/Users/user/Library/Application Support/uv` on macOS, in contrast to the new preference
/// for using the XDG directories on all Unix platforms.
pub fn legacy_user_state_dir() -> Option<PathBuf> {
    etcetera::base_strategy::choose_native_strategy()
        .ok()
        .map(|dirs| dirs.data_dir().join("uv"))
        .map(|dir| if cfg!(windows) { dir.join("data") } else { dir })
}

/// Return a [`PathBuf`] if the given [`OsString`] is an absolute path.
fn parse_path(path: OsString) -> Option<PathBuf> {
    let path = PathBuf::from(path);
    if path.is_absolute() {
        Some(path)
    } else {
        None
    }
}
