use std::path::PathBuf;

use owo_colors::OwoColorize;
use tokio::task::JoinError;
use url::Url;
use zip::result::ZipError;

use crate::metadata::MetadataError;
use uv_client::WrappedReqwestError;
use uv_distribution_filename::WheelFilenameError;
use uv_distribution_types::{InstalledDist, InstalledDistError, IsBuildBackendError};
use uv_fs::Simplified;
use uv_normalize::PackageName;
use uv_pep440::{Version, VersionSpecifiers};
use uv_pypi_types::{HashAlgorithm, HashDigest};
use uv_types::AnyErrorBuild;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Building source distributions is disabled")]
    NoBuild,

    // Network error
    #[error("Expected an absolute path, but received: {}", _0.user_display())]
    RelativePath(PathBuf),
    #[error(transparent)]
    InvalidUrl(#[from] uv_distribution_types::ToUrlError),
    #[error(transparent)]
    JoinRelativeUrl(#[from] uv_pypi_types::JoinRelativeError),
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
    #[error("Failed to walk the distribution cache")]
    CacheWalk(#[source] walkdir::Error),
    #[error(transparent)]
    CacheInfo(#[from] uv_cache_info::CacheInfoError),

    // Build error
    #[error(transparent)]
    Build(AnyErrorBuild),
    #[error("Built wheel has an invalid filename")]
    WheelFilename(#[from] WheelFilenameError),
    #[error("Package metadata name `{metadata}` does not match given name `{given}`")]
    WheelMetadataNameMismatch {
        given: PackageName,
        metadata: PackageName,
    },
    #[error("Package metadata version `{metadata}` does not match given version `{given}`")]
    WheelMetadataVersionMismatch { given: Version, metadata: Version },
    #[error(
        "Package metadata name `{metadata}` does not match `{filename}` from the wheel filename"
    )]
    WheelFilenameNameMismatch {
        filename: PackageName,
        metadata: PackageName,
    },
    #[error(
        "Package metadata version `{metadata}` does not match `{filename}` from the wheel filename"
    )]
    WheelFilenameVersionMismatch {
        filename: Version,
        metadata: Version,
    },
    #[error("Failed to parse metadata from built wheel")]
    Metadata(#[from] uv_pypi_types::MetadataError),
    #[error("Failed to read metadata: `{}`", _0.user_display())]
    WheelMetadata(PathBuf, #[source] Box<uv_metadata::Error>),
    #[error("Failed to read metadata from installed package `{0}`")]
    ReadInstalled(Box<InstalledDist>, #[source] InstalledDistError),
    #[error("Failed to read zip archive from built wheel")]
    Zip(#[from] ZipError),
    #[error("Failed to extract archive")]
    Extract(#[from] uv_extract::Error),
    #[error("The source distribution is missing a `PKG-INFO` file")]
    MissingPkgInfo,
    #[error("The source distribution `{}` has no subdirectory `{}`", _0, _1.display())]
    MissingSubdirectory(Url, PathBuf),
    #[error("Failed to extract static metadata from `PKG-INFO`")]
    PkgInfo(#[source] uv_pypi_types::MetadataError),
    #[error("Failed to extract metadata from `requires.txt`")]
    RequiresTxt(#[source] uv_pypi_types::MetadataError),
    #[error("The source distribution is missing a `pyproject.toml` file")]
    MissingPyprojectToml,
    #[error("Failed to extract static metadata from `pyproject.toml`")]
    PyprojectToml(#[source] uv_pypi_types::MetadataError),
    #[error("Unsupported scheme in URL: {0}")]
    UnsupportedScheme(String),
    #[error(transparent)]
    MetadataLowering(#[from] MetadataError),
    #[error("Distribution not found at: {0}")]
    NotFound(Url),
    #[error("Attempted to re-extract the source distribution for `{}`, but the {} hash didn't match. Run `{}` to clear the cache.", _0, _1, "uv cache clean".green())]
    CacheHeal(String, HashAlgorithm),
    #[error("The source distribution requires Python {0}, but {1} is installed")]
    RequiresPython(VersionSpecifiers, Version),

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

impl IsBuildBackendError for Error {
    fn is_build_backend_error(&self) -> bool {
        match self {
            Self::Build(err) => err.is_build_backend_error(),
            _ => false,
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
