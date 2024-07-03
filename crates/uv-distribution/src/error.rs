use std::path::PathBuf;

use tokio::task::JoinError;
use url::Url;
use zip::result::ZipError;

use crate::metadata::MetadataError;
use distribution_filename::WheelFilenameError;
use pep440_rs::Version;
use pypi_types::HashDigest;
use uv_client::WrappedReqwestError;
use uv_fs::Simplified;
use uv_normalize::PackageName;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Building source distributions is disabled")]
    NoBuild,
    #[error("Using pre-built wheels is disabled")]
    NoBinary,

    // Network error
    #[error("Failed to parse URL: {0}")]
    Url(String, #[source] url::ParseError),
    #[error("Expected an absolute path, but received: {}", _0.user_display())]
    RelativePath(PathBuf),
    #[error(transparent)]
    JoinRelativeUrl(#[from] pypi_types::JoinRelativeError),
    #[error("Expected a file URL, but received: {0}")]
    NonFileUrl(Url),
    #[error(transparent)]
    Git(#[from] uv_git::GitResolverError),
    #[error(transparent)]
    Reqwest(#[from] WrappedReqwestError),
    #[error(transparent)]
    Client(#[from] uv_client::Error),

    // Cache writing error
    #[error("Failed to read from the distribution cache")]
    CacheRead(#[source] std::io::Error),
    #[error("Failed to write to the distribution cache")]
    CacheWrite(#[source] std::io::Error),
    #[error("Failed to deserialize cache entry")]
    CacheDecode(#[from] rmp_serde::decode::Error),
    #[error("Failed to serialize cache entry")]
    CacheEncode(#[from] rmp_serde::encode::Error),

    // Build error
    #[error("Failed to build: `{0}`")]
    Build(String, #[source] anyhow::Error),
    #[error("Failed to build editable: `{0}`")]
    BuildEditable(String, #[source] anyhow::Error),
    #[error("Built wheel has an invalid filename")]
    WheelFilename(#[from] WheelFilenameError),
    #[error("Package metadata name `{metadata}` does not match given name `{given}`")]
    NameMismatch {
        given: PackageName,
        metadata: PackageName,
    },
    #[error("Package metadata version `{metadata}` does not match given version `{given}`")]
    VersionMismatch { given: Version, metadata: Version },
    #[error("Failed to parse metadata from built wheel")]
    Metadata(#[from] pypi_types::MetadataError),
    #[error("Failed to read `dist-info` metadata from built wheel")]
    DistInfo(#[from] install_wheel_rs::Error),
    #[error("Failed to read zip archive from built wheel")]
    Zip(#[from] ZipError),
    #[error("Source distribution directory contains neither readable `pyproject.toml` nor `setup.py`: `{}`", _0.user_display())]
    DirWithoutEntrypoint(PathBuf),
    #[error("Failed to extract archive")]
    Extract(#[from] uv_extract::Error),
    #[error("The source distribution is missing a `PKG-INFO` file")]
    MissingPkgInfo,
    #[error("Failed to extract static metadata from `PKG-INFO`")]
    PkgInfo(#[source] pypi_types::MetadataError),
    #[error("The source distribution is missing a `pyproject.toml` file")]
    MissingPyprojectToml,
    #[error("Failed to extract static metadata from `pyproject.toml`")]
    PyprojectToml(#[source] pypi_types::MetadataError),
    #[error("Unsupported scheme in URL: {0}")]
    UnsupportedScheme(String),
    #[error(transparent)]
    MetadataLowering(#[from] MetadataError),
    #[error("Distribution not found at: {0}")]
    NotFound(Url),

    /// A generic request middleware error happened while making a request.
    /// Refer to the error message for more details.
    #[error(transparent)]
    ReqwestMiddlewareError(#[from] anyhow::Error),

    /// Should not occur; only seen when another task panicked.
    #[error("The task executor is broken, did some other task panic?")]
    Join(#[from] JoinError),

    /// An I/O error that occurs while exhausting a reader to compute a hash.
    #[error("Failed to hash distribution")]
    HashExhaustion(#[source] std::io::Error),

    #[error("Hash mismatch for `{distribution}`\n\nExpected:\n{expected}\n\nComputed:\n{actual}")]
    MismatchedHashes {
        distribution: String,
        expected: String,
        actual: String,
    },

    #[error(
        "Hash-checking is enabled, but no hashes were provided or computed for: `{distribution}`"
    )]
    MissingHashes { distribution: String },

    #[error("Hash-checking is enabled, but no hashes were computed for: `{distribution}`\n\nExpected:\n{expected}")]
    MissingActualHashes {
        distribution: String,
        expected: String,
    },

    #[error("Hash-checking is enabled, but no hashes were provided for: `{distribution}`\n\nComputed:\n{actual}")]
    MissingExpectedHashes {
        distribution: String,
        actual: String,
    },

    #[error("Hash-checking is not supported for local directories: `{0}`")]
    HashesNotSupportedSourceTree(String),

    #[error("Hash-checking is not supported for Git repositories: `{0}`")]
    HashesNotSupportedGit(String),
}

impl From<reqwest::Error> for Error {
    fn from(error: reqwest::Error) -> Self {
        Self::Reqwest(WrappedReqwestError::from(error))
    }
}

impl From<reqwest_middleware::Error> for Error {
    fn from(error: reqwest_middleware::Error) -> Self {
        match error {
            reqwest_middleware::Error::Middleware(error) => Self::ReqwestMiddlewareError(error),
            reqwest_middleware::Error::Reqwest(error) => {
                Self::Reqwest(WrappedReqwestError::from(error))
            }
        }
    }
}

impl Error {
    /// Construct a hash mismatch error.
    pub fn hash_mismatch(
        distribution: String,
        expected: &[HashDigest],
        actual: &[HashDigest],
    ) -> Error {
        match (expected.is_empty(), actual.is_empty()) {
            (true, true) => Self::MissingHashes { distribution },
            (true, false) => {
                let actual = actual
                    .iter()
                    .map(|hash| format!("  {hash}"))
                    .collect::<Vec<_>>()
                    .join("\n");

                Self::MissingExpectedHashes {
                    distribution,
                    actual,
                }
            }
            (false, true) => {
                let expected = expected
                    .iter()
                    .map(|hash| format!("  {hash}"))
                    .collect::<Vec<_>>()
                    .join("\n");

                Self::MissingActualHashes {
                    distribution,
                    expected,
                }
            }
            (false, false) => {
                let expected = expected
                    .iter()
                    .map(|hash| format!("  {hash}"))
                    .collect::<Vec<_>>()
                    .join("\n");

                let actual = actual
                    .iter()
                    .map(|hash| format!("  {hash}"))
                    .collect::<Vec<_>>()
                    .join("\n");

                Self::MismatchedHashes {
                    distribution,
                    expected,
                    actual,
                }
            }
        }
    }
}
