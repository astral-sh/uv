use std::path::PathBuf;

use thiserror::Error;

use uv_distribution_filename::WheelFilenameError;
use uv_fs::Simplified;

/// Errors that can occur during delocate operations.
#[derive(Debug, Error)]
pub enum DelocateError {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Zip(#[from] zip::result::ZipError),

    #[error(transparent)]
    WalkDir(#[from] walkdir::Error),

    #[error(transparent)]
    Extract(#[from] uv_extract::Error),

    #[error(transparent)]
    Csv(#[from] csv::Error),

    #[error("Failed to parse Mach-O binary: {0}")]
    MachOParse(String),

    #[error("Dependency not found: {name} (required by {})", required_by.user_display())]
    DependencyNotFound { name: String, required_by: PathBuf },

    #[error("Missing required architecture {arch} in {}", path.user_display())]
    MissingArchitecture { arch: String, path: PathBuf },

    #[error("Library name collision: {name} exists at multiple paths: {paths:?}")]
    LibraryCollision { name: String, paths: Vec<PathBuf> },

    #[error("Invalid wheel filename: {filename}")]
    InvalidWheelFilename {
        filename: String,
        #[source]
        err: WheelFilenameError,
    },

    #[error("Invalid wheel path: {}", path.user_display())]
    InvalidWheelPath { path: PathBuf },

    #[error("Missing `.dist-info` directory in wheel")]
    MissingDistInfo,

    #[error("Path `{}` is not within wheel directory `{}`", path.user_display(), wheel_dir.user_display())]
    PathNotInWheel { path: PathBuf, wheel_dir: PathBuf },

    #[error("Unsupported Mach-O format: {0}")]
    UnsupportedFormat(String),

    #[error(
        "Library {} requires macOS {library_version}, but wheel declares {wheel_version}",
        library.user_display()
    )]
    IncompatibleMacOSVersion {
        library: PathBuf,
        library_version: String,
        wheel_version: String,
    },

    #[error("`codesign` failed for {}: {stderr}", path.user_display())]
    CodesignFailed { path: PathBuf, stderr: String },

    #[error("`install_name_tool` failed for {}: {stderr}", path.user_display())]
    InstallNameToolFailed { path: PathBuf, stderr: String },
}
