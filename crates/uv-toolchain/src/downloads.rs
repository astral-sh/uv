use std::fmt::Display;
use std::io;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use crate::implementation::{
    Error as ImplementationError, ImplementationName, LenientImplementationName,
};
use crate::platform::{self, Arch, Libc, Os};
use crate::toolchain::ToolchainKey;
use crate::{Interpreter, PythonVersion, ToolchainRequest, VersionRequest};
use thiserror::Error;
use uv_client::BetterReqwestError;

use futures::TryStreamExt;

use tokio_util::compat::FuturesAsyncReadCompatExt;
use tracing::{debug, instrument};
use url::Url;
use uv_fs::Simplified;

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    IO(#[from] io::Error),
    #[error(transparent)]
    ImplementationError(#[from] ImplementationError),
    #[error("Invalid python version: {0}")]
    InvalidPythonVersion(String),
    #[error("Invalid request key, too many parts: {0}")]
    TooManyParts(String),
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
    #[error("Failed to parse request part")]
    InvalidRequestPlatform(#[from] platform::Error),
    #[error("Cannot download toolchain for request: {0}")]
    InvalidRequestKind(ToolchainRequest),
    // TODO(zanieb): Implement display for `PythonDownloadRequest`
    #[error("No download found for request: {0:?}")]
    NoDownloadFound(PythonDownloadRequest),
}

#[derive(Debug, PartialEq)]
pub struct PythonDownload {
    key: ToolchainKey,
    url: &'static str,
    sha256: Option<&'static str>,
}

#[derive(Debug, Clone, Default, Eq, PartialEq)]
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
        let result = match request {
            ToolchainRequest::Version(version) => Self::default().with_version(version),
            ToolchainRequest::Implementation(implementation) => {
                Self::default().with_implementation(implementation)
            }
            ToolchainRequest::ImplementationVersion(implementation, version) => Self::default()
                .with_implementation(implementation)
                .with_version(version),
            ToolchainRequest::Key(request) => request,
            ToolchainRequest::Any => Self::default(),
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
            self.arch = Some(Arch::from_env());
        }
        if self.os.is_none() {
            self.os = Some(Os::from_env());
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
            Some(Arch::from_env()),
            Some(Os::from_env()),
            Some(Libc::from_env()),
        ))
    }

    pub fn implementation(&self) -> Option<&ImplementationName> {
        self.implementation.as_ref()
    }

    pub fn version(&self) -> Option<&VersionRequest> {
        self.version.as_ref()
    }

    pub fn arch(&self) -> Option<&Arch> {
        self.arch.as_ref()
    }

    pub fn os(&self) -> Option<&Os> {
        self.os.as_ref()
    }

    pub fn libc(&self) -> Option<&Libc> {
        self.libc.as_ref()
    }

    /// Iterate over all [`PythonDownload`]'s that match this request.
    pub fn iter_downloads(&self) -> impl Iterator<Item = &'static PythonDownload> + '_ {
        PythonDownload::iter_all().filter(move |download| self.satisfied_by_download(download))
    }

    pub fn satisfied_by_key(&self, key: &ToolchainKey) -> bool {
        if let Some(arch) = &self.arch {
            if key.arch != *arch {
                return false;
            }
        }
        if let Some(os) = &self.os {
            if key.os != *os {
                return false;
            }
        }
        if let Some(implementation) = &self.implementation {
            if key.implementation != LenientImplementationName::from(*implementation) {
                return false;
            }
        }
        if let Some(version) = &self.version {
            if !version.matches_major_minor_patch(key.major, key.minor, key.patch) {
                return false;
            }
        }
        true
    }

    pub fn satisfied_by_download(&self, download: &PythonDownload) -> bool {
        self.satisfied_by_key(download.key())
    }

    pub fn satisfied_by_interpreter(&self, interpreter: &Interpreter) -> bool {
        if let Some(version) = self.version() {
            if !version.matches_interpreter(interpreter) {
                return false;
            }
        }
        if let Some(os) = self.os() {
            if &Os::from(interpreter.platform().os()) != os {
                return false;
            }
        }
        if let Some(arch) = self.arch() {
            if &Arch::from(&interpreter.platform().arch()) != arch {
                return false;
            }
        }
        if let Some(implementation) = self.implementation() {
            if LenientImplementationName::from(interpreter.implementation_name())
                != LenientImplementationName::from(*implementation)
            {
                return false;
            }
        }
        if let Some(libc) = self.libc() {
            if &Libc::from(interpreter.platform().os()) != libc {
                return false;
            }
        }
        true
    }
}

impl Display for PythonDownloadRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut parts = Vec::new();
        if let Some(implementation) = self.implementation {
            parts.push(implementation.to_string());
        } else {
            parts.push("any".to_string());
        }
        if let Some(version) = &self.version {
            parts.push(version.to_string());
        } else {
            parts.push("any".to_string());
        }
        if let Some(os) = &self.os {
            parts.push(os.to_string());
        } else {
            parts.push("any".to_string());
        }
        if let Some(arch) = self.arch {
            parts.push(arch.to_string());
        } else {
            parts.push("any".to_string());
        }
        if let Some(libc) = self.libc {
            parts.push(libc.to_string());
        } else {
            parts.push("any".to_string());
        }
        write!(f, "{}", parts.join("-"))
    }
}

impl FromStr for PythonDownloadRequest {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut parts = s.split('-');
        let mut version = None;
        let mut implementation = None;
        let mut os = None;
        let mut arch = None;
        let mut libc = None;

        loop {
            // Consume each part
            let Some(part) = parts.next() else { break };

            if implementation.is_none() {
                implementation = Some(ImplementationName::from_str(part)?);
                continue;
            }

            if version.is_none() {
                version = Some(
                    VersionRequest::from_str(part)
                        .map_err(|_| Error::InvalidPythonVersion(part.to_string()))?,
                );
                continue;
            }

            if os.is_none() {
                os = Some(Os::from_str(part)?);
                continue;
            }

            if arch.is_none() {
                arch = Some(Arch::from_str(part)?);
                continue;
            }

            if libc.is_none() {
                libc = Some(Libc::from_str(part)?);
                continue;
            }

            return Err(Error::TooManyParts(s.to_string()));
        }
        Ok(Self::new(version, implementation, arch, os, libc))
    }
}

include!("downloads.inc");

pub enum DownloadResult {
    AlreadyAvailable(PathBuf),
    Fetched(PathBuf),
}

impl PythonDownload {
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

    pub fn key(&self) -> &ToolchainKey {
        &self.key
    }

    pub fn os(&self) -> &Os {
        self.key.os()
    }

    pub fn sha256(&self) -> Option<&str> {
        self.sha256
    }

    /// Download and extract
    #[instrument(skip(client, parent_path), fields(download = %self.key()))]
    pub async fn fetch(
        &self,
        client: &uv_client::BaseClient,
        parent_path: &Path,
    ) -> Result<DownloadResult, Error> {
        let url = Url::parse(self.url)?;
        let path = parent_path.join(self.key().to_string()).clone();

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
        self.key.version()
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
        write!(f, "{}", self.key)
    }
}
