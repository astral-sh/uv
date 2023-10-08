//! Takes a wheel and installs it, either in a venv or for monotrail.

use std::io;
use std::io::{Read, Seek};

use platform_info::PlatformInfoError;
use thiserror::Error;
use zip::result::ZipError;
use zip::ZipArchive;

pub use install_location::{normalize_name, InstallLocation, LockedDir};
use platform_host::{Arch, Os};
pub use record::RecordEntry;
pub use script::Script;
pub use wheel::{
    get_script_launcher, install_wheel, parse_key_value_file, read_record_file, relative_to,
    SHEBANG_PYTHON,
};

mod install_location;
#[cfg(feature = "python_bindings")]
mod python_bindings;
mod record;
mod script;
pub mod unpacked;
mod wheel;

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
    #[error(transparent)]
    InvalidWheelFileName(#[from] wheel_filename::Error),
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

pub fn do_thing(reader: impl Read + Seek) -> Result<(), Error> {
    let x = tempfile::tempdir()?;
    let mut archive =
        ZipArchive::new(reader).map_err(|err| Error::from_zip_error("(index)".to_string(), err))?;

    archive.extract(x.path()).unwrap();

    Ok(())
}
