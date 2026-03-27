use std::fmt;
use std::path::PathBuf;

use owo_colors::OwoColorize;
use tokio::task::JoinError;
use zip::result::ZipError;

use crate::metadata::MetadataError;
use uv_cache::Error as CacheError;
use uv_client::WrappedReqwestError;
use uv_distribution_filename::{WheelFilename, WheelFilenameError};
use uv_distribution_types::{InstalledDist, InstalledDistError, IsBuildBackendError};
use uv_fs::Simplified;
use uv_git::GitError;
use uv_normalize::PackageName;
use uv_pep440::{Version, VersionSpecifiers};
use uv_platform_tags::Platform;
use uv_pypi_types::{HashAlgorithm, HashDigest};
use uv_python::PythonVariant;
use uv_redacted::DisplaySafeUrl;
use uv_types::AnyErrorBuild;

#[derive(Debug, Clone, Copy)]
pub struct PythonVersion {
    pub(crate) version: (u8, u8),
    pub(crate) variant: PythonVariant,
}

impl fmt::Display for PythonVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (major, minor) = self.version;
        write!(f, "{major}.{minor}{}", self.variant.executable_suffix())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Building source distributions is disabled")]
    NoBuild,

    // Network error
    #[error(transparent)]
    InvalidUrl(#[from] uv_distribution_types::ToUrlError),
    #[error("Expected a file URL, but received: {0}")]
    NonFileUrl(DisplaySafeUrl),
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
    #[error("Failed to acquire lock on the distribution cache")]
    CacheLock(#[source] CacheError),
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
    /// This shouldn't happen, it's a bug in the build backend.
    #[error(
        "The built wheel `{}` is not compatible with the current Python {} on {}",
        filename,
        python_version,
        python_platform.pretty(),
    )]
    BuiltWheelIncompatibleHostPlatform {
        filename: WheelFilename,
        python_platform: Platform,
        python_version: PythonVersion,
    },
    /// This may happen when trying to cross-install native dependencies without their build backend
    /// being aware that the target is a cross-install.
    #[error(
        "The built wheel `{}` is not compatible with the target Python {} on {}. Consider using `--no-build` to disable building wheels.",
        filename,
        python_version,
        python_platform.pretty(),
    )]
    BuiltWheelIncompatibleTargetPlatform {
        filename: WheelFilename,
        python_platform: Platform,
        python_version: PythonVersion,
    },
    #[error("Failed to parse metadata from built wheel")]
    Metadata(#[from] uv_pypi_types::MetadataError),
    #[error("Failed to read metadata: `{}`", _0.user_display())]
    WheelMetadata(PathBuf, #[source] Box<uv_metadata::Error>),
    #[error("Failed to read metadata from installed package `{0}`")]
    ReadInstalled(Box<InstalledDist>, #[source] InstalledDistError),
    #[error("Failed to read zip archive from built wheel")]
    Zip(#[from] ZipError),
    #[error("Failed to extract archive: {0}")]
    Extract(String, #[source] uv_extract::Error),
    #[error("The source distribution is missing a `PKG-INFO` file")]
    MissingPkgInfo,
    #[error("The source distribution `{}` has no subdirectory `{}`", _0, _1.display())]
    MissingSubdirectory(DisplaySafeUrl, PathBuf),
    #[error("The source distribution `{0}` is missing Git LFS artifacts.")]
    MissingGitLfsArtifacts(DisplaySafeUrl, #[source] GitError),
    #[error("Failed to extract static metadata from `PKG-INFO`")]
    PkgInfo(#[source] uv_pypi_types::MetadataError),
    #[error("The source distribution is missing a `pyproject.toml` file")]
    MissingPyprojectToml,
    #[error("Failed to extract static metadata from `pyproject.toml`")]
    PyprojectToml(#[source] uv_pypi_types::MetadataError),
    #[error(transparent)]
    MetadataLowering(#[from] MetadataError),
    #[error("Distribution not found at: {0}")]
    NotFound(DisplaySafeUrl),
    #[error("Attempted to re-extract the source distribution for `{}`, but the {} hash didn't match. Run `{}` to clear the cache.", _0, _1, "uv cache clean".green())]
    CacheHeal(String, HashAlgorithm),
    #[error("The source distribution requires Python {0}, but {1} is installed")]
    RequiresPython(VersionSpecifiers, Version),
    #[error("Failed to identify base Python interpreter")]
    BaseInterpreter(#[source] std::io::Error),

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

    #[error(
        "Hash-checking is enabled, but no hashes were computed for: `{distribution}`\n\nExpected:\n{expected}"
    )]
    MissingActualHashes {
        distribution: String,
        expected: String,
    },

    #[error(
        "Hash-checking is enabled, but no hashes were provided for: `{distribution}`\n\nComputed:\n{actual}"
    )]
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
    ) -> Self {
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

#[cfg(test)]
mod tests {
    use super::{Error, PythonVersion};
    use std::str::FromStr;
    use uv_distribution_filename::WheelFilename;
    use uv_platform_tags::{Arch, Os, Platform};
    use uv_python::PythonVariant;

    #[test]
    fn built_wheel_error_formats_freethreaded_python() {
        let err = Error::BuiltWheelIncompatibleHostPlatform {
            filename: WheelFilename::from_str(
                "cryptography-47.0.0.dev1-cp315-abi3t-macosx_11_0_arm64.whl",
            )
            .unwrap(),
            python_platform: Platform::new(
                Os::Macos {
                    major: 11,
                    minor: 0,
                },
                Arch::Aarch64,
            ),
            python_version: PythonVersion {
                version: (3, 15),
                variant: PythonVariant::Freethreaded,
            },
        };

        assert_eq!(
            err.to_string(),
            "The built wheel `cryptography-47.0.0.dev1-cp315-abi3t-macosx_11_0_arm64.whl` is not compatible with the current Python 3.15t on macOS aarch64"
        );
    }

    #[test]
    fn built_wheel_error_formats_target_python() {
        let err = Error::BuiltWheelIncompatibleTargetPlatform {
            filename: WheelFilename::from_str("py313-0.1.0-py313-none-any.whl").unwrap(),
            python_platform: Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 28,
                },
                Arch::X86_64,
            ),
            python_version: PythonVersion {
                version: (3, 12),
                variant: PythonVariant::Default,
            },
        };

        assert_eq!(
            err.to_string(),
            "The built wheel `py313-0.1.0-py313-none-any.whl` is not compatible with the target Python 3.12 on Linux x86_64. Consider using `--no-build` to disable building wheels."
        );
    }
}
