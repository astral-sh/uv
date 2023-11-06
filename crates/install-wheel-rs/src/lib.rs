//! Takes a wheel and installs it into a venv..

use std::io;

use distribution_filename::WheelFilename;
use platform_info::PlatformInfoError;
use thiserror::Error;
use zip::result::ZipError;

pub use direct_url::DirectUrl;
pub use install_location::{normalize_name, InstallLocation, LockedDir};
use platform_host::{Arch, Os};
pub use record::RecordEntry;
pub use script::Script;
pub use uninstall::{uninstall_wheel, Uninstall};
pub use wheel::{
    find_dist_info, get_script_launcher, install_wheel, parse_key_value_file, read_record_file,
    relative_to, SHEBANG_PYTHON,
};

mod direct_url;
mod install_location;
pub mod linker;
#[cfg(feature = "python_bindings")]
mod python_bindings;
mod record;
mod script;
mod uninstall;
mod wheel;

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    IO(#[from] io::Error),
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
    InvalidWheelFileName(#[from] distribution_filename::WheelFilenameError),
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
    #[error("Invalid direct_url.json")]
    DirectUrlJson(#[from] serde_json::Error),
}

impl Error {
    pub(crate) fn from_zip_error(file: String, value: ZipError) -> Self {
        match value {
            ZipError::Io(io_error) => Self::IO(io_error),
            _ => Self::Zip(file, value),
        }
    }
}

/// The metadata name may be uppercase, while the wheel and dist info names are lowercase, or
/// the metadata name and the dist info name are lowercase, while the wheel name is uppercase.
/// Either way, we just search the wheel for the name
pub fn find_dist_info_metadata<'a, T: Copy>(
    filename: &WheelFilename,
    files: impl Iterator<Item = (T, &'a str)>,
) -> Result<(T, &'a str), String> {
    let dist_info_matcher = format!(
        "{}-{}",
        filename.distribution.as_dist_info_name(),
        filename.version
    );
    let metadatas: Vec<_> = files
        .filter_map(|(payload, path)| {
            let (dir, file) = path.split_once('/')?;
            let dir = dir.strip_suffix(".dist-info")?;
            if dir.to_lowercase() == dist_info_matcher && file == "METADATA" {
                Some((payload, path))
            } else {
                None
            }
        })
        .collect();
    let (payload, path) = match metadatas[..] {
        [] => {
            return Err("no .dist-info directory".to_string());
        }
        [(payload, path)] => (payload, path),
        _ => {
            return Err(format!(
                "multiple .dist-info directories: {}",
                metadatas
                    .into_iter()
                    .map(|(_, path)| path.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    };
    Ok((payload, path))
}
