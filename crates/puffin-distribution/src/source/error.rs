use thiserror::Error;
use tokio::task::JoinError;
use zip::result::ZipError;

use distribution_filename::WheelFilenameError;
use puffin_normalize::PackageName;

/// The caller is responsible for adding the source dist information to the error chain
#[derive(Debug, Error)]
pub enum SourceDistError {
    #[error("Building source distributions is disabled")]
    NoBuild,

    // Network error
    #[error("Failed to parse URL: `{0}`")]
    UrlParse(String, #[source] url::ParseError),
    #[error("Git operation failed")]
    Git(#[source] anyhow::Error),
    #[error(transparent)]
    Request(#[from] reqwest::Error),
    #[error(transparent)]
    Client(#[from] puffin_client::Error),

    // Cache writing error
    #[error("Failed to write to source distribution cache")]
    Io(#[from] std::io::Error),
    #[error("Cache deserialization failed")]
    Decode(#[from] rmp_serde::decode::Error),
    #[error("Cache serialization failed")]
    Encode(#[from] rmp_serde::encode::Error),

    // Build error
    #[error("Failed to build: {0}")]
    Build(String, #[source] anyhow::Error),
    #[error("Built wheel has an invalid filename")]
    WheelFilename(#[from] WheelFilenameError),
    #[error("Package metadata name `{metadata}` does not match given name `{given}`")]
    NameMismatch {
        given: PackageName,
        metadata: PackageName,
    },
    #[error("Failed to parse metadata from built wheel")]
    Metadata(#[from] pypi_types::Error),
    #[error("Failed to read `dist-info` metadata from built wheel")]
    DistInfo(#[from] install_wheel_rs::Error),
    #[error("Failed to read zip archive from built wheel")]
    Zip(#[from] ZipError),
    #[error("Source distribution directory contains neither readable pyproject.toml nor setup.py")]
    DirWithoutEntrypoint,
    #[error("Failed to extract source distribution: {0}")]
    Extract(#[from] puffin_extract::Error),

    /// Should not occur; only seen when another task panicked.
    #[error("The task executor is broken, did some other task panic?")]
    Join(#[from] JoinError),
}
