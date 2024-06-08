use std::fmt::Display;
use std::io;
use std::num::ParseIntError;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use crate::implementation::{Error as ImplementationError, ImplementationName};
use crate::platform::{Arch, Error as PlatformError, Libc, Os};
use crate::{PythonVersion, ToolchainRequest, VersionRequest};
use thiserror::Error;
use uv_client::BetterReqwestError;

use futures::TryStreamExt;

use tokio_util::compat::FuturesAsyncReadCompatExt;
use tracing::debug;
use url::Url;
use uv_fs::Simplified;

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    IO(#[from] io::Error),
    #[error(transparent)]
    PlatformError(#[from] PlatformError),
    #[error(transparent)]
    ImplementationError(#[from] ImplementationError),
    #[error("Invalid python version: {0}")]
    InvalidPythonVersion(ParseIntError),
    #[error("Download failed")]
    NetworkError(#[from] BetterReqwestError),
    #[error("Download failed")]
    NetworkMiddlewareError(#[source] anyhow::Error),
    #[error("Failed to extract archive: {0}")]
    ExtractError(String, #[source] uv_extract::Error),
    #[error("Invalid download url")]
    InvalidUrl(#[from] url::ParseError),
    #[error("Failed to create download directory")]
    DownloadDirError(#[source] io::Error),
    #[error("Failed to copy to: {0}", to.user_display())]
    CopyError {
        to: PathBuf,
        #[source]
        err: io::Error,
    },
    #[error("Failed to read toolchain directory: {0}", dir.user_display())]
    ReadError {
        dir: PathBuf,
        #[source]
        err: io::Error,
    },
    #[error("Failed to parse toolchain directory name: {0}")]
    NameError(String),
    #[error("Cannot download toolchain for request: {0}")]
    InvalidRequestKind(ToolchainRequest),
    // TODO(zanieb): Implement display for `PythonDownloadRequest`
    #[error("No download found for request: {0:?}")]
    NoDownloadFound(PythonDownloadRequest),
}

#[derive(Debug, PartialEq)]
pub struct PythonDownload {
    key: &'static str,
    implementation: ImplementationName,
    arch: Arch,
    os: Os,
    libc: Libc,
    major: u8,
    minor: u8,
    patch: u8,
    url: &'static str,
    sha256: Option<&'static str>,
}

#[derive(Debug, Clone, Default)]
pub struct PythonDownloadRequest {
    version: Option<VersionRequest>,
    implementation: Option<ImplementationName>,
    arch: Option<Arch>,
    os: Option<Os>,
    libc: Option<Libc>,
}

impl PythonDownloadRequest {
    pub fn new(
        version: Option<VersionRequest>,
        implementation: Option<ImplementationName>,
        arch: Option<Arch>,
        os: Option<Os>,
        libc: Option<Libc>,
    ) -> Self {
        Self {
            version,
            implementation,
            arch,
            os,
            libc,
        }
    }

    #[must_use]
    pub fn with_implementation(mut self, implementation: ImplementationName) -> Self {
        self.implementation = Some(implementation);
        self
    }

    #[must_use]
    pub fn with_version(mut self, version: VersionRequest) -> Self {
        self.version = Some(version);
        self
    }

    #[must_use]
    pub fn with_arch(mut self, arch: Arch) -> Self {
        self.arch = Some(arch);
        self
    }

    #[must_use]
    pub fn with_os(mut self, os: Os) -> Self {
        self.os = Some(os);
        self
    }

    #[must_use]
    pub fn with_libc(mut self, libc: Libc) -> Self {
        self.libc = Some(libc);
        self
    }

    /// Construct a new [`PythonDownloadRequest`] from a [`ToolchainRequest`].
    pub fn from_request(request: ToolchainRequest) -> Result<Self, Error> {
        let result = Self::default();
        let result = match request {
            ToolchainRequest::Version(version) => result.with_version(version),
            ToolchainRequest::Implementation(implementation) => {
                result.with_implementation(implementation)
            }
            ToolchainRequest::ImplementationVersion(implementation, version) => result
                .with_implementation(implementation)
                .with_version(version),
            ToolchainRequest::Any => result,
            // We can't download a toolchain for these request kinds
            ToolchainRequest::Directory(_)
            | ToolchainRequest::ExecutableName(_)
            | ToolchainRequest::File(_) => {
                return Err(Error::InvalidRequestKind(request));
            }
        };
        Ok(result)
    }

    /// Fill empty entries with default values.
    ///
    /// Platform information is pulled from the environment.
    pub fn fill(mut self) -> Result<Self, Error> {
        if self.implementation.is_none() {
            self.implementation = Some(ImplementationName::CPython);
        }
        if self.arch.is_none() {
            self.arch = Some(Arch::from_env()?);
        }
        if self.os.is_none() {
            self.os = Some(Os::from_env()?);
        }
        if self.libc.is_none() {
            self.libc = Some(Libc::from_env());
        }
        Ok(self)
    }

    /// Construct a new [`PythonDownloadRequest`] with platform information from the environment.
    pub fn from_env() -> Result<Self, Error> {
        Ok(Self::new(
            None,
            None,
            Some(Arch::from_env()?),
            Some(Os::from_env()?),
            Some(Libc::from_env()),
        ))
    }

    /// Iterate over all [`PythonDownload`]'s that match this request.
    pub fn iter_downloads(&self) -> impl Iterator<Item = &'static PythonDownload> + '_ {
        PythonDownload::iter_all().filter(move |download| {
            if let Some(arch) = &self.arch {
                if download.arch != *arch {
                    return false;
                }
            }
            if let Some(os) = &self.os {
                if download.os != *os {
                    return false;
                }
            }
            if let Some(implementation) = &self.implementation {
                if download.implementation != *implementation {
                    return false;
                }
            }
            if let Some(version) = &self.version {
                if !version.matches_major_minor_patch(
                    download.major,
                    download.minor,
                    download.patch,
                ) {
                    return false;
                }
            }
            true
        })
    }
}

impl Display for PythonDownloadRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut parts = Vec::new();
        if let Some(version) = self.version {
            parts.push(version.to_string());
        }
        if let Some(implementation) = self.implementation {
            parts.push(implementation.to_string());
        }
        if let Some(os) = &self.os {
            parts.push(os.to_string());
        }
        if let Some(arch) = self.arch {
            parts.push(arch.to_string());
        }
        if let Some(libc) = self.libc {
            parts.push(libc.to_string());
        }
        write!(f, "{}", parts.join("-"))
    }
}

impl FromStr for PythonDownloadRequest {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // TODO(zanieb): Implement parsing of additional request parts
        let version = VersionRequest::from_str(s).map_err(Error::InvalidPythonVersion)?;
        Ok(Self::new(Some(version), None, None, None, None))
    }
}

include!("downloads.inc");

pub enum DownloadResult {
    AlreadyAvailable(PathBuf),
    Fetched(PathBuf),
}

impl PythonDownload {
    /// Return the [`PythonDownload`] corresponding to the key, if it exists.
    pub fn from_key(key: &str) -> Option<&PythonDownload> {
        PYTHON_DOWNLOADS.iter().find(|&value| value.key == key)
    }

    /// Return the first [`PythonDownload`] matching a request, if any.
    pub fn from_request(request: &PythonDownloadRequest) -> Result<&'static PythonDownload, Error> {
        request
            .iter_downloads()
            .next()
            .ok_or(Error::NoDownloadFound(request.clone()))
    }

    /// Iterate over all [`PythonDownload`]'s.
    pub fn iter_all() -> impl Iterator<Item = &'static PythonDownload> {
        PYTHON_DOWNLOADS.iter()
    }

    pub fn url(&self) -> &str {
        self.url
    }

    pub fn key(&self) -> &str {
        self.key
    }

    pub fn sha256(&self) -> Option<&str> {
        self.sha256
    }

    /// Download and extract
    pub async fn fetch(
        &self,
        client: &uv_client::BaseClient,
        parent_path: &Path,
    ) -> Result<DownloadResult, Error> {
        let url = Url::parse(self.url)?;
        let path = parent_path.join(self.key).clone();

        // If it already exists, return it
        if path.is_dir() {
            return Ok(DownloadResult::AlreadyAvailable(path));
        }

        let filename = url.path_segments().unwrap().last().unwrap();
        let response = client.get(url.clone()).send().await?;

        // Ensure the request was successful.
        response.error_for_status_ref()?;

        // Download and extract into a temporary directory.
        let temp_dir = tempfile::tempdir_in(parent_path).map_err(Error::DownloadDirError)?;

        debug!(
            "Downloading {url} to temporary location {}",
            temp_dir.path().display()
        );
        let reader = response
            .bytes_stream()
            .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err))
            .into_async_read();

        debug!("Extracting {filename}");
        uv_extract::stream::archive(reader.compat(), filename, temp_dir.path())
            .await
            .map_err(|err| Error::ExtractError(filename.to_string(), err))?;

        // Extract the top-level directory.
        let extracted = match uv_extract::strip_component(temp_dir.path()) {
            Ok(top_level) => top_level,
            Err(uv_extract::Error::NonSingularArchive(_)) => temp_dir.into_path(),
            Err(err) => return Err(Error::ExtractError(filename.to_string(), err)),
        };

        // Persist it to the target
        debug!("Moving {} to {}", extracted.display(), path.user_display());
        fs_err::tokio::rename(extracted, &path)
            .await
            .map_err(|err| Error::CopyError {
                to: path.clone(),
                err,
            })?;

        Ok(DownloadResult::Fetched(path))
    }

    pub fn python_version(&self) -> PythonVersion {
        PythonVersion::from_str(&format!("{}.{}.{}", self.major, self.minor, self.patch))
            .expect("Python downloads should always have valid versions")
    }
}

impl From<reqwest::Error> for Error {
    fn from(error: reqwest::Error) -> Self {
        Self::NetworkError(BetterReqwestError::from(error))
    }
}

impl From<reqwest_middleware::Error> for Error {
    fn from(error: reqwest_middleware::Error) -> Self {
        match error {
            reqwest_middleware::Error::Middleware(error) => Self::NetworkMiddlewareError(error),
            reqwest_middleware::Error::Reqwest(error) => {
                Self::NetworkError(BetterReqwestError::from(error))
            }
        }
    }
}

impl Display for PythonDownload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.key)
    }
}
