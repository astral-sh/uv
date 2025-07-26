use std::path::PathBuf;

use thiserror::Error;

/// Errors that can occur during binary download and installation.
#[derive(Debug, Error)]
pub enum Error {
    /// Failed to download binary.
    #[error("Failed to download {tool} {version} from {url}")]
    Download {
        tool: String,
        version: String,
        url: String,
        #[source]
        source: reqwest_middleware::Error,
    },

    /// Failed to parse download URL.
    #[error("Failed to parse download URL: {url}")]
    UrlParse {
        url: String,
        #[source]
        source: url::ParseError,
    },

    /// Unsupported platform for binary download.
    #[error("Unsupported platform for {tool}: {platform}")]
    UnsupportedPlatform { tool: String, platform: String },

    /// Failed to extract archive.
    #[error("Failed to extract {tool} archive")]
    Extract {
        tool: String,
        #[source]
        source: anyhow::Error,
    },

    /// Binary not found in extracted archive.
    #[error("Binary not found in {tool} archive at expected location: {expected}")]
    BinaryNotFound { tool: String, expected: PathBuf },

    /// Task join error.
    #[error("Task join error")]
    Join(#[from] tokio::task::JoinError),

    /// I/O error during installation.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// Platform detection error.
    #[error("Failed to detect platform")]
    Platform(#[from] uv_platform::Error),

    /// Generic errors.
    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
}