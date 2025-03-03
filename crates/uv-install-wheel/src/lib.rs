//! Takes a wheel and installs it into a venv.

use std::io;
use std::path::PathBuf;

use thiserror::Error;

use uv_fs::Simplified;
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_pypi_types::Scheme;

pub use install::install_wheel;
pub use linker::{LinkMode, Locks};
pub use uninstall::{uninstall_egg, uninstall_legacy_editable, uninstall_wheel, Uninstall};
pub use wheel::{parse_wheel_file, read_record_file, LibKind};

mod install;
mod linker;
mod record;
mod script;
mod uninstall;
mod wheel;

/// The layout of the target environment into which a wheel can be installed.
#[derive(Debug, Clone)]
pub struct Layout {
    /// The Python interpreter, as returned by `sys.executable`.
    pub sys_executable: PathBuf,
    /// The Python version, as returned by `sys.version_info`.
    pub python_version: (u8, u8),
    /// The `os.name` value for the current platform.
    pub os_name: String,
    /// The [`Scheme`] paths for the interpreter.
    pub scheme: Scheme,
}

/// Note: The caller is responsible for adding the path of the wheel we're installing.
#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] io::Error),
    /// Custom error type to add a path to error reading a file from a zip
    #[error("Failed to reflink {} to {}", from.user_display(), to.user_display())]
    Reflink {
        from: PathBuf,
        to: PathBuf,
        #[source]
        err: io::Error,
    },
    /// The wheel is broken
    #[error("The wheel is invalid: {0}")]
    InvalidWheel(String),
    /// Doesn't follow file name schema
    #[error("Failed to move data files")]
    WalkDir(#[from] walkdir::Error),
    #[error("RECORD file doesn't match wheel contents: {0}")]
    RecordFile(String),
    #[error("RECORD file is invalid")]
    RecordCsv(#[from] csv::Error),
    #[error("Broken virtual environment: {0}")]
    BrokenVenv(String),
    #[error(
        "Unable to create Windows launcher for: {0} (only x86_64, x86, and arm64 are supported)"
    )]
    UnsupportedWindowsArch(&'static str),
    #[error("Unable to create Windows launcher on non-Windows platform")]
    NotWindows,
    #[error("Invalid `direct_url.json`")]
    DirectUrlJson(#[from] serde_json::Error),
    #[error("Cannot uninstall package; `RECORD` file not found at: {}", _0.user_display())]
    MissingRecord(PathBuf),
    #[error("Cannot uninstall package; `top_level.txt` file not found at: {}", _0.user_display())]
    MissingTopLevel(PathBuf),
    #[error("Invalid package version")]
    InvalidVersion(#[from] uv_pep440::VersionParseError),
    #[error("Wheel package name does not match filename: {0} != {1}")]
    MismatchedName(PackageName, PackageName),
    #[error("Wheel version does not match filename: {0} != {1}")]
    MismatchedVersion(Version, Version),
    #[error("Invalid egg-link")]
    InvalidEggLink(PathBuf),
    #[error(transparent)]
    LauncherError(#[from] uv_trampoline_builder::Error),
}
