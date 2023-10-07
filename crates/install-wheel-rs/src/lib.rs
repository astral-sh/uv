//! Takes a wheel and installs it, either in a venv or for monotrail
//!
//! ```no_run
//! use std::path::Path;
//! use install_wheel_rs::install_wheel_in_venv;
//!
//! install_wheel_in_venv(
//!     "Django-4.2.6-py3-none-any.whl",
//!     ".venv",
//!     ".venv/bin/python",
//!     (3, 8),
//! ).unwrap();
//! ```

use platform_info::PlatformInfoError;
use std::fs::File;
use std::io;
use std::path::Path;
use std::str::FromStr;
use thiserror::Error;
use zip::result::ZipError;

pub use install_location::{normalize_name, InstallLocation, LockedDir};
pub use wheel::{
    get_script_launcher, install_wheel, parse_key_value_file, read_record_file, relative_to,
    Script, SHEBANG_PYTHON,
};
pub use wheel_tags::{Arch, CompatibleTags, Os, WheelFilename};

mod install_location;
#[cfg(feature = "python_bindings")]
mod python_bindings;
mod wheel;
mod wheel_tags;

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    IO(#[from] io::Error),
    /// This shouldn't actually be possible to occur
    #[error("Failed to serialize direct_url.json ಠ_ಠ")]
    DirectUrlSerdeJson(#[source] serde_json::Error),
    /// Tags/metadata didn't match platform
    #[error("The wheel is incompatible with the current platform {os} {arch}")]
    IncompatibleWheel { os: Os, arch: Arch },
    /// The wheel is broken
    #[error("The wheel is invalid: {0}")]
    InvalidWheel(String),
    /// pyproject.toml or poetry.lock are broken
    #[error("The poetry dependency specification (pyproject.toml or poetry.lock) is broken (try `poetry update`?): {0}")]
    InvalidPoetry(String),
    /// Doesn't follow file name schema
    #[error("The wheel filename \"{0}\" is invalid: {1}")]
    InvalidWheelFileName(String, String),
    #[error("Failed to read the wheel file {0}")]
    Zip(String, #[source] ZipError),
    #[error("Failed to run python subcommand")]
    PythonSubcommand(#[source] io::Error),
    #[error("Failed to move data files")]
    WalkDir(#[from] walkdir::Error),
    #[error("RECORD file doesn't match wheel contents: {0}")]
    RecordFile(String),
    #[error("RECORD file is invalid")]
    RecordCsv(#[from] csv::Error),
    #[error("Broken virtualenv: {0}")]
    BrokenVenv(String),
    #[error("Failed to detect the operating system version: {0}")]
    OsVersionDetection(String),
    #[error("Failed to detect the current platform")]
    PlatformInfo(#[source] PlatformInfoError),
    #[error("Invalid version specification, only none or == is supported")]
    Pep440,
}

impl Error {
    pub(crate) fn from_zip_error(file: String, value: ZipError) -> Self {
        match value {
            ZipError::Io(io_error) => Self::IO(io_error),
            _ => Self::Zip(file, value),
        }
    }
}

/// High level API: Install a wheel in a virtualenv
///
/// The python interpreter is used for compiling to byte code, the python version for computing
/// the site packages path on unix.
///
/// Returns the tag of the wheel
pub fn install_wheel_in_venv(
    wheel: impl AsRef<Path>,
    venv: impl AsRef<Path>,
    interpreter: impl AsRef<Path>,
    major_minor: (u8, u8),
) -> Result<String, Error> {
    let venv_base = venv.as_ref().canonicalize()?;
    let location = InstallLocation::Venv {
        venv_base,
        python_version: major_minor,
    };
    let locked_dir = location.acquire_lock()?;

    let filename = wheel
        .as_ref()
        .file_name()
        .ok_or_else(|| Error::InvalidWheel("Expected a file".to_string()))?
        .to_string_lossy();
    let filename = WheelFilename::from_str(&filename)?;
    let compatible_tags = CompatibleTags::current(location.get_python_version())?;
    filename.compatibility(&compatible_tags)?;

    install_wheel(
        &locked_dir,
        File::open(wheel)?,
        filename,
        false,
        true,
        &[],
        // Only relevant for monotrail style installation
        "",
        interpreter,
    )
}
