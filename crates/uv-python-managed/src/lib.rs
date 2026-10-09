//! Download, store, inspect, and maintain uv-managed Python installations.

#[cfg(test)]
use uv_static::EnvVars;

pub use managed::{
    Error, ManagedPythonInstallation, ManagedPythonInstallations, PythonExecutable,
    PythonMinorVersionLink, UpgradePolicy, compare_build_versions, create_link_to_executable,
    platform_key_from_env, python_executable_dir, replace_link_to_executable,
};

pub mod downloads;
pub mod macos_dylib;
mod managed;
mod sysconfig;
#[cfg(windows)]
pub mod windows_registry;

#[cfg(not(test))]
fn current_dir() -> Result<std::path::PathBuf, std::io::Error> {
    std::env::current_dir()
}

#[cfg(test)]
fn current_dir() -> Result<std::path::PathBuf, std::io::Error> {
    std::env::var_os(EnvVars::PWD)
        .map(std::path::PathBuf::from)
        .map(Ok)
        .unwrap_or(std::env::current_dir())
}
