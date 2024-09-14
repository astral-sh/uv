use distribution_filename::{ExtensionError, SourceDistExtension};
use futures::TryStreamExt;
use owo_colors::OwoColorize;
use pypi_types::{HashAlgorithm, HashDigest};
use std::fmt::Display;
use std::io;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::str::FromStr;
use std::task::{Context, Poll};
use thiserror::Error;
use tokio::io::{AsyncRead, ReadBuf};
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tokio_util::either::Either;
use tracing::{debug, instrument};
use url::Url;
use uv_client::WrappedReqwestError;
use uv_extract::hash::Hasher;
use uv_fs::{rename_with_retry, Simplified};

use crate::implementation::{
    Error as ImplementationError, ImplementationName, LenientImplementationName,
};
use crate::installation::PythonInstallationKey;
use crate::libc::LibcDetectionError;
use crate::platform::{self, Arch, Libc, Os};
use crate::{Interpreter, PythonRequest, PythonVersion, VersionRequest};

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    ImplementationError(#[from] ImplementationError),
    #[error("Expected download URL (`{0}`) to end in a supported file extension: {1}")]
    MissingExtension(String, ExtensionError),
    #[error("Invalid Python version: {0}")]
    InvalidPythonVersion(String),
    #[error("Invalid request key (too many parts): {0}")]
    TooManyParts(String),
    #[error(transparent)]
    NetworkError(#[from] WrappedReqwestError),
    #[error(transparent)]
    NetworkMiddlewareError(#[from] anyhow::Error),
    #[error("Failed to extract archive: {0}")]
    ExtractError(String, #[source] uv_extract::Error),
    #[error("Failed to hash installation")]
    HashExhaustion(#[source] io::Error),
    #[error("Hash mismatch for `{installation}`\n\nExpected:\n{expected}\n\nComputed:\n{actual}")]
    HashMismatch {
        installation: String,
        expected: String,
        actual: String,
    },
    #[error("Invalid download URL")]
    InvalidUrl(#[from] url::ParseError),
    #[error("Invalid path in file URL: `{0}`")]
    InvalidFileUrl(String),
    #[error("Failed to create download directory")]
    DownloadDirError(#[source] io::Error),
    #[error("Failed to copy to: {0}", to.user_display())]
    CopyError {
        to: PathBuf,
        #[source]
        err: io::Error,
    },
    #[error("Failed to read managed Python installation directory: {0}", dir.user_display())]
    ReadError {
        dir: PathBuf,
        #[source]
        err: io::Error,
    },
    #[error("Failed to parse request part")]
    InvalidRequestPlatform(#[from] platform::Error),
    #[error("No download found for request: {}", _0.green())]
    NoDownloadFound(PythonDownloadRequest),
    #[error(
        "A mirror was provided via `{0}`, but the URL does not match the expected format: {0}"
    )]
    Mirror(&'static str, &'static str),
    #[error(transparent)]
    LibcDetection(#[from] LibcDetectionError),
}

#[derive(Debug, PartialEq)]
pub struct ManagedPythonDownload {
    key: PythonInstallationKey,
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

    /// Whether to allow pre-releases or not. If not set, defaults to true if [`Self::version`] is
    /// not None, and false otherwise.
    prereleases: Option<bool>,
}

impl PythonDownloadRequest {
    pub fn new(
        version: Option<VersionRequest>,
        implementation: Option<ImplementationName>,
        arch: Option<Arch>,
        os: Option<Os>,
        libc: Option<Libc>,
        prereleases: Option<bool>,
    ) -> Self {
        Self {
            version,
            implementation,
            arch,
            os,
            libc,
            prereleases,
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

    #[must_use]
    pub fn with_prereleases(mut self, prereleases: bool) -> Self {
        self.prereleases = Some(prereleases);
        self
    }

    /// Construct a new [`PythonDownloadRequest`] from a [`PythonRequest`] if possible.
    ///
    /// Returns [`None`] if the request kind is not compatible with a download, e.g., it is
    /// a request for a specific directory or executable name.
    pub fn from_request(request: &PythonRequest) -> Option<Self> {
        match request {
            PythonRequest::Version(version) => Some(Self::default().with_version(version.clone())),
            PythonRequest::Implementation(implementation) => {
                Some(Self::default().with_implementation(*implementation))
            }
            PythonRequest::ImplementationVersion(implementation, version) => Some(
                Self::default()
                    .with_implementation(*implementation)
                    .with_version(version.clone()),
            ),
            PythonRequest::Key(request) => Some(request.clone()),
            PythonRequest::Any => Some(Self::default()),
            // We can't download a managed installation for these request kinds
            PythonRequest::Directory(_)
            | PythonRequest::ExecutableName(_)
            | PythonRequest::File(_) => None,
        }
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
            self.libc = Some(Libc::from_env()?);
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
            Some(Libc::from_env()?),
            None,
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
    pub fn iter_downloads(&self) -> impl Iterator<Item = &'static ManagedPythonDownload> + '_ {
        ManagedPythonDownload::iter_all()
            .filter(move |download| self.satisfied_by_download(download))
    }

    /// Whether this request is satisfied by the key of an existing installation.
    pub fn satisfied_by_key(&self, key: &PythonInstallationKey) -> bool {
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
        if let Some(libc) = &self.libc {
            if key.libc != *libc {
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
        // If we don't allow pre-releases, don't match a key with a pre-release tag
        if !self.allows_prereleases() && !key.prerelease.is_empty() {
            return false;
        }
        true
    }

    /// Whether this request is satisfied by a Python download.
    pub fn satisfied_by_download(&self, download: &ManagedPythonDownload) -> bool {
        self.satisfied_by_key(download.key())
    }

    pub fn allows_prereleases(&self) -> bool {
        self.prereleases.unwrap_or_else(|| self.version.is_some())
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
        Ok(Self::new(version, implementation, arch, os, libc, None))
    }
}

include!("downloads.inc");

pub enum DownloadResult {
    AlreadyAvailable(PathBuf),
    Fetched(PathBuf),
}

impl ManagedPythonDownload {
    /// Return the first [`PythonDownload`] matching a request, if any.
    pub fn from_request(
        request: &PythonDownloadRequest,
    ) -> Result<&'static ManagedPythonDownload, Error> {
        request
            .iter_downloads()
            .next()
            .ok_or(Error::NoDownloadFound(request.clone()))
    }

    /// Iterate over all [`PythonDownload`]'s.
    pub fn iter_all() -> impl Iterator<Item = &'static ManagedPythonDownload> {
        PYTHON_DOWNLOADS
            .iter()
            // TODO(konsti): musl python-build-standalone builds are currently broken (statically
            // linked), so we pretend they don't exist. https://github.com/astral-sh/uv/issues/4242
            .filter(|download| download.key.libc != Libc::Some(target_lexicon::Environment::Musl))
    }

    pub fn url(&self) -> &str {
        self.url
    }

    pub fn key(&self) -> &PythonInstallationKey {
        &self.key
    }

    pub fn os(&self) -> &Os {
        self.key.os()
    }

    pub fn sha256(&self) -> Option<&str> {
        self.sha256
    }

    /// Download and extract
    #[instrument(skip(client, installation_dir, cache_dir, reporter), fields(download = % self.key()))]
    pub async fn fetch(
        &self,
        client: &uv_client::BaseClient,
        installation_dir: &Path,
        cache_dir: &Path,
        reporter: Option<&dyn Reporter>,
    ) -> Result<DownloadResult, Error> {
        let url = self.download_url()?;
        let path = installation_dir.join(self.key().to_string());

        // If it already exists, return it
        if path.is_dir() {
            return Ok(DownloadResult::AlreadyAvailable(path));
        }

        let filename = url.path_segments().unwrap().last().unwrap();
        let ext = SourceDistExtension::from_path(filename)
            .map_err(|err| Error::MissingExtension(url.to_string(), err))?;
        let (reader, size) = read_url(&url, client).await?;

        let progress = reporter
            .as_ref()
            .map(|reporter| (reporter, reporter.on_download_start(&self.key, size)));

        // Download and extract into a temporary directory.
        let temp_dir = tempfile::tempdir_in(cache_dir).map_err(Error::DownloadDirError)?;

        debug!(
            "Downloading {url} to temporary location: {}",
            temp_dir.path().simplified().display()
        );

        let mut hashers = self
            .sha256
            .into_iter()
            .map(|_| Hasher::from(HashAlgorithm::Sha256))
            .collect::<Vec<_>>();
        let mut hasher = uv_extract::hash::HashReader::new(reader, &mut hashers);

        debug!("Extracting {filename}");

        match progress {
            Some((&reporter, progress)) => {
                let mut reader = ProgressReader::new(&mut hasher, progress, reporter);
                uv_extract::stream::archive(&mut reader, ext, temp_dir.path())
                    .await
                    .map_err(|err| Error::ExtractError(filename.to_string(), err))?;
            }
            None => {
                uv_extract::stream::archive(&mut hasher, ext, temp_dir.path())
                    .await
                    .map_err(|err| Error::ExtractError(filename.to_string(), err))?;
            }
        };

        hasher.finish().await.map_err(Error::HashExhaustion)?;

        if let Some((&reporter, progress)) = progress {
            reporter.on_progress(&self.key, progress);
        }

        // Check the hash
        if let Some(expected) = self.sha256 {
            let actual = HashDigest::from(hashers.pop().unwrap()).digest;
            if !actual.eq_ignore_ascii_case(expected) {
                return Err(Error::HashMismatch {
                    installation: self.key.to_string(),
                    expected: expected.to_string(),
                    actual: actual.to_string(),
                });
            }
        }

        // Extract the top-level directory.
        let mut extracted = match uv_extract::strip_component(temp_dir.path()) {
            Ok(top_level) => top_level,
            Err(uv_extract::Error::NonSingularArchive(_)) => temp_dir.into_path(),
            Err(err) => return Err(Error::ExtractError(filename.to_string(), err)),
        };

        // If the distribution is a `full` archive, the Python installation is in the `install` directory.
        if extracted.join("install").is_dir() {
            extracted = extracted.join("install");
        }

        // If the distribution is missing a `python`-to-`pythonX.Y` symlink, add it. PEP 394 permits
        // it, and python-build-standalone releases after `20240726` include it, but releases prior
        // to that date do not.
        #[cfg(unix)]
        {
            match std::os::unix::fs::symlink(
                format!("python{}.{}", self.key.major, self.key.minor),
                extracted.join("bin").join("python"),
            ) {
                Ok(()) => {}
                Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {}
                Err(err) => return Err(err.into()),
            }
        }

        // Persist it to the target
        debug!("Moving {} to {}", extracted.display(), path.user_display());
        rename_with_retry(extracted, &path)
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

    /// Return the [`Url`] to use when downloading the distribution. If a mirror is set via the
    /// appropriate environment variable, use it instead.
    fn download_url(&self) -> Result<Url, Error> {
        match self.key.implementation {
            LenientImplementationName::Known(ImplementationName::CPython) => {
                if let Ok(mirror) = std::env::var("UV_PYTHON_INSTALL_MIRROR") {
                    let Some(suffix) = self.url.strip_prefix(
                        "https://github.com/indygreg/python-build-standalone/releases/download/",
                    ) else {
                        return Err(Error::Mirror("UV_PYTHON_INSTALL_MIRROR", self.url));
                    };
                    return Ok(Url::parse(
                        format!("{}/{}", mirror.trim_end_matches('/'), suffix).as_str(),
                    )?);
                }
            }

            LenientImplementationName::Known(ImplementationName::PyPy) => {
                if let Ok(mirror) = std::env::var("UV_PYPY_INSTALL_MIRROR") {
                    let Some(suffix) = self.url.strip_prefix("https://downloads.python.org/pypy/")
                    else {
                        return Err(Error::Mirror("UV_PYPY_INSTALL_MIRROR", self.url));
                    };
                    return Ok(Url::parse(
                        format!("{}/{}", mirror.trim_end_matches('/'), suffix).as_str(),
                    )?);
                }
            }

            _ => {}
        }

        Ok(Url::parse(self.url)?)
    }
}

impl From<reqwest::Error> for Error {
    fn from(error: reqwest::Error) -> Self {
        Self::NetworkError(WrappedReqwestError::from(error))
    }
}

impl From<reqwest_middleware::Error> for Error {
    fn from(error: reqwest_middleware::Error) -> Self {
        match error {
            reqwest_middleware::Error::Middleware(error) => Self::NetworkMiddlewareError(error),
            reqwest_middleware::Error::Reqwest(error) => {
                Self::NetworkError(WrappedReqwestError::from(error))
            }
        }
    }
}

impl Display for ManagedPythonDownload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.key)
    }
}

pub trait Reporter: Send + Sync {
    fn on_progress(&self, name: &PythonInstallationKey, id: usize);
    fn on_download_start(&self, name: &PythonInstallationKey, size: Option<u64>) -> usize;
    fn on_download_progress(&self, id: usize, inc: u64);
    fn on_download_complete(&self);
}

/// An asynchronous reader that reports progress as bytes are read.
struct ProgressReader<'a, R> {
    reader: R,
    index: usize,
    reporter: &'a dyn Reporter,
}

impl<'a, R> ProgressReader<'a, R> {
    /// Create a new [`ProgressReader`] that wraps another reader.
    fn new(reader: R, index: usize, reporter: &'a dyn Reporter) -> Self {
        Self {
            reader,
            index,
            reporter,
        }
    }
}

impl<R> AsyncRead for ProgressReader<'_, R>
where
    R: AsyncRead + Unpin,
{
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.as_mut().reader)
            .poll_read(cx, buf)
            .map_ok(|()| {
                self.reporter
                    .on_download_progress(self.index, buf.filled().len() as u64);
            })
    }
}

/// Convert a [`Url`] into an [`AsyncRead`] stream.
async fn read_url(
    url: &Url,
    client: &uv_client::BaseClient,
) -> Result<(impl AsyncRead + Unpin, Option<u64>), Error> {
    if url.scheme() == "file" {
        // Loads downloaded distribution from the given `file://` URL.
        let path = url
            .to_file_path()
            .map_err(|()| Error::InvalidFileUrl(url.to_string()))?;

        let size = fs_err::tokio::metadata(&path).await?.len();
        let reader = fs_err::tokio::File::open(&path).await?;

        Ok((Either::Left(reader), Some(size)))
    } else {
        let response = client.client().get(url.clone()).send().await?;

        // Ensure the request was successful.
        response.error_for_status_ref()?;

        let size = response.content_length();
        let stream = response
            .bytes_stream()
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err))
            .into_async_read();

        Ok((Either::Right(stream.compat()), size))
    }
}
