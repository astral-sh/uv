use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::Display;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::str::FromStr;
use std::task::{Context, Poll};
use std::time::{Duration, SystemTime};
use std::{env, io};

use futures::TryStreamExt;
use itertools::Itertools;
use once_cell::sync::OnceCell;
use owo_colors::OwoColorize;
use reqwest_retry::RetryPolicy;
use serde::Deserialize;
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncWriteExt, BufWriter, ReadBuf};
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tokio_util::either::Either;
use tracing::{debug, instrument};
use url::Url;

use uv_client::{BaseClient, WrappedReqwestError, is_extended_transient_error};
use uv_distribution_filename::{ExtensionError, SourceDistExtension};
use uv_extract::hash::Hasher;
use uv_fs::{Simplified, rename_with_retry};
use uv_pypi_types::{HashAlgorithm, HashDigest};
use uv_redacted::DisplaySafeUrl;
use uv_static::EnvVars;

use crate::PythonVariant;
use crate::implementation::{
    Error as ImplementationError, ImplementationName, LenientImplementationName,
};
use crate::installation::PythonInstallationKey;
use crate::libc::LibcDetectionError;
use crate::managed::ManagedPythonInstallation;
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
    #[error("Failed to download {0}")]
    NetworkError(DisplaySafeUrl, #[source] WrappedReqwestError),
    #[error("Failed to download {0}")]
    NetworkMiddlewareError(DisplaySafeUrl, #[source] anyhow::Error),
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
    #[error("Invalid download URL: {0}")]
    InvalidUrlFormat(Url),
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
    #[error("A mirror was provided via `{0}`, but the URL does not match the expected format: {0}")]
    Mirror(&'static str, &'static str),
    #[error("Failed to determine the libc used on the current platform")]
    LibcDetection(#[from] LibcDetectionError),
    #[error("Remote Python downloads JSON is not yet supported, please use a local path")]
    RemoteJSONNotSupported,
    #[error("The JSON of the python downloads is invalid: {0}")]
    InvalidPythonDownloadsJSON(PathBuf, #[source] serde_json::Error),
    #[error("An offline Python installation was requested, but {file} (from {url}) is missing in {}", python_builds_dir.user_display())]
    OfflinePythonMissing {
        file: Box<PythonInstallationKey>,
        url: Box<Url>,
        python_builds_dir: PathBuf,
    },
}

#[derive(Debug, PartialEq, Clone)]
pub struct ManagedPythonDownload {
    key: PythonInstallationKey,
    url: &'static str,
    sha256: Option<&'static str>,
}

#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct PythonDownloadRequest {
    pub(crate) version: Option<VersionRequest>,
    pub(crate) implementation: Option<ImplementationName>,
    pub(crate) arch: Option<ArchRequest>,
    pub(crate) os: Option<Os>,
    pub(crate) libc: Option<Libc>,

    /// Whether to allow pre-releases or not. If not set, defaults to true if [`Self::version`] is
    /// not None, and false otherwise.
    pub(crate) prereleases: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchRequest {
    Explicit(Arch),
    Environment(Arch),
}

impl Display for ArchRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Explicit(arch) | Self::Environment(arch) => write!(f, "{arch}"),
        }
    }
}

impl ArchRequest {
    pub(crate) fn satisfied_by(self, arch: Arch) -> bool {
        match self {
            Self::Explicit(request) => request == arch,
            Self::Environment(env) => env.supports(arch),
        }
    }

    pub fn inner(&self) -> Arch {
        match self {
            Self::Explicit(arch) | Self::Environment(arch) => *arch,
        }
    }
}

impl PythonDownloadRequest {
    pub fn new(
        version: Option<VersionRequest>,
        implementation: Option<ImplementationName>,
        arch: Option<ArchRequest>,
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
        self.arch = Some(ArchRequest::Explicit(arch));
        self
    }

    #[must_use]
    pub fn with_any_arch(mut self) -> Self {
        self.arch = None;
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
            PythonRequest::Any => Some(Self {
                prereleases: Some(true), // Explicitly allow pre-releases for PythonRequest::Any
                ..Self::default()
            }),
            PythonRequest::Default => Some(Self::default()),
            // We can't download a managed installation for these request kinds
            PythonRequest::Directory(_)
            | PythonRequest::ExecutableName(_)
            | PythonRequest::File(_) => None,
        }
    }

    /// Fill empty entries with default values.
    ///
    /// Platform information is pulled from the environment.
    pub fn fill_platform(mut self) -> Result<Self, Error> {
        if self.arch.is_none() {
            self.arch = Some(ArchRequest::Environment(Arch::from_env()));
        }
        if self.os.is_none() {
            self.os = Some(Os::from_env());
        }
        if self.libc.is_none() {
            self.libc = Some(Libc::from_env()?);
        }
        Ok(self)
    }

    pub fn fill(mut self) -> Result<Self, Error> {
        if self.implementation.is_none() {
            self.implementation = Some(ImplementationName::CPython);
        }
        self = self.fill_platform()?;
        Ok(self)
    }

    pub fn implementation(&self) -> Option<&ImplementationName> {
        self.implementation.as_ref()
    }

    pub fn version(&self) -> Option<&VersionRequest> {
        self.version.as_ref()
    }

    pub fn arch(&self) -> Option<&ArchRequest> {
        self.arch.as_ref()
    }

    pub fn os(&self) -> Option<&Os> {
        self.os.as_ref()
    }

    pub fn libc(&self) -> Option<&Libc> {
        self.libc.as_ref()
    }

    /// Iterate over all [`PythonDownload`]'s that match this request.
    pub fn iter_downloads<'a>(
        &'a self,
        python_downloads_json_url: Option<&'a str>,
    ) -> Result<impl Iterator<Item = &'static ManagedPythonDownload> + use<'a>, Error> {
        Ok(ManagedPythonDownload::iter_all(python_downloads_json_url)?
            .filter(move |download| self.satisfied_by_download(download)))
    }

    /// Whether this request is satisfied by an installation key.
    pub fn satisfied_by_key(&self, key: &PythonInstallationKey) -> bool {
        if let Some(os) = &self.os {
            if key.os != *os {
                return false;
            }
        }

        if let Some(arch) = &self.arch {
            if !arch.satisfied_by(key.arch) {
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
        // If we don't allow pre-releases, don't match a key with a pre-release tag
        if !self.allows_prereleases() && key.prerelease.is_some() {
            return false;
        }
        if let Some(version) = &self.version {
            if !version.matches_major_minor_patch_prerelease(
                key.major,
                key.minor,
                key.patch,
                key.prerelease,
            ) {
                return false;
            }
            if let Some(variant) = version.variant() {
                if variant != key.variant {
                    return false;
                }
            }
        }
        true
    }

    /// Whether this request is satisfied by a Python download.
    pub fn satisfied_by_download(&self, download: &ManagedPythonDownload) -> bool {
        self.satisfied_by_key(download.key())
    }

    /// Whether this download request opts-in to pre-release Python versions.
    pub fn allows_prereleases(&self) -> bool {
        self.prereleases.unwrap_or_else(|| {
            self.version
                .as_ref()
                .is_some_and(VersionRequest::allows_prereleases)
        })
    }

    /// Whether this download request opts-in to alternative Python implementations.
    pub fn allows_alternative_implementations(&self) -> bool {
        self.implementation.is_some()
    }

    pub fn satisfied_by_interpreter(&self, interpreter: &Interpreter) -> bool {
        let executable = interpreter.sys_executable().display();
        if let Some(version) = self.version() {
            if !version.matches_interpreter(interpreter) {
                let interpreter_version = interpreter.python_version();
                debug!(
                    "Skipping interpreter at `{executable}`: version `{interpreter_version}` does not match request `{version}`"
                );
                return false;
            }
        }
        if let Some(os) = self.os() {
            let interpreter_os = Os::from(interpreter.platform().os());
            if &interpreter_os != os {
                debug!(
                    "Skipping interpreter at `{executable}`: operating system `{interpreter_os}` does not match request `{os}`"
                );
                return false;
            }
        }
        if let Some(arch) = self.arch() {
            let interpreter_arch = Arch::from(&interpreter.platform().arch());
            if !arch.satisfied_by(interpreter_arch) {
                debug!(
                    "Skipping interpreter at `{executable}`: architecture `{interpreter_arch}` does not match request `{arch}`"
                );
                return false;
            }
        }
        if let Some(implementation) = self.implementation() {
            let interpreter_implementation = interpreter.implementation_name();
            if LenientImplementationName::from(interpreter_implementation)
                != LenientImplementationName::from(*implementation)
            {
                debug!(
                    "Skipping interpreter at `{executable}`: implementation `{interpreter_implementation}` does not match request `{implementation}`"
                );
                return false;
            }
        }
        if let Some(libc) = self.libc() {
            let interpreter_libc = Libc::from(interpreter.platform().os());
            if &interpreter_libc != libc {
                debug!(
                    "Skipping interpreter at `{executable}`: libc `{interpreter_libc}` does not match request `{libc}`"
                );
                return false;
            }
        }
        true
    }
}

impl From<&ManagedPythonInstallation> for PythonDownloadRequest {
    fn from(installation: &ManagedPythonInstallation) -> Self {
        let key = installation.key();
        Self::new(
            Some(VersionRequest::from(&key.version())),
            match &key.implementation {
                LenientImplementationName::Known(implementation) => Some(*implementation),
                LenientImplementationName::Unknown(name) => unreachable!(
                    "Managed Python installations are expected to always have known implementation names, found {name}"
                ),
            },
            Some(ArchRequest::Explicit(key.arch)),
            Some(key.os),
            Some(key.libc),
            Some(key.prerelease.is_some()),
        )
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

        let mut position = 0;
        loop {
            // Consume each part
            let Some(part) = parts.next() else { break };
            position += 1;

            if part.eq_ignore_ascii_case("any") {
                continue;
            }

            match position {
                1 => implementation = Some(ImplementationName::from_str(part)?),
                2 => {
                    version = Some(
                        VersionRequest::from_str(part)
                            .map_err(|_| Error::InvalidPythonVersion(part.to_string()))?,
                    );
                }
                3 => os = Some(Os::from_str(part)?),
                4 => arch = Some(ArchRequest::Explicit(Arch::from_str(part)?)),
                5 => libc = Some(Libc::from_str(part)?),
                _ => return Err(Error::TooManyParts(s.to_string())),
            }
        }
        Ok(Self::new(version, implementation, arch, os, libc, None))
    }
}

const BUILTIN_PYTHON_DOWNLOADS_JSON: &str = include_str!("download-metadata-minified.json");
static PYTHON_DOWNLOADS: OnceCell<std::borrow::Cow<'static, [ManagedPythonDownload]>> =
    OnceCell::new();

#[derive(Debug, Deserialize, Clone)]
struct JsonPythonDownload {
    name: String,
    arch: JsonArch,
    os: String,
    libc: String,
    major: u8,
    minor: u8,
    patch: u8,
    prerelease: Option<String>,
    url: String,
    sha256: Option<String>,
    variant: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
struct JsonArch {
    family: String,
    variant: Option<String>,
}

#[derive(Debug, Clone)]
pub enum DownloadResult {
    AlreadyAvailable(PathBuf),
    Fetched(PathBuf),
}

impl ManagedPythonDownload {
    /// Return the first [`ManagedPythonDownload`] matching a request, if any.
    ///
    /// If there is no stable version matching the request, a compatible pre-release version will
    /// be searched for — even if a pre-release was not explicitly requested.
    pub fn from_request(
        request: &PythonDownloadRequest,
        python_downloads_json_url: Option<&str>,
    ) -> Result<&'static ManagedPythonDownload, Error> {
        if let Some(download) = request.iter_downloads(python_downloads_json_url)?.next() {
            return Ok(download);
        }

        if !request.allows_prereleases() {
            if let Some(download) = request
                .clone()
                .with_prereleases(true)
                .iter_downloads(python_downloads_json_url)?
                .next()
            {
                return Ok(download);
            }
        }

        Err(Error::NoDownloadFound(request.clone()))
    }
    //noinspection RsUnresolvedPath - RustRover can't see through the `include!`

    /// Iterate over all [`ManagedPythonDownload`]s.
    ///
    /// Note: The list is generated on the first call to this function.
    /// so `python_downloads_json_url` is only used in the first call to this function.
    pub fn iter_all(
        python_downloads_json_url: Option<&str>,
    ) -> Result<impl Iterator<Item = &'static ManagedPythonDownload>, Error> {
        let downloads = PYTHON_DOWNLOADS.get_or_try_init(|| {
            let json_downloads: HashMap<String, JsonPythonDownload> = if let Some(json_source) =
                python_downloads_json_url
            {
                // Windows paths are also valid URLs
                let json_source = if let Ok(url) = Url::parse(json_source) {
                    if let Ok(path) = url.to_file_path() {
                        Cow::Owned(path)
                    } else if matches!(url.scheme(), "http" | "https") {
                        return Err(Error::RemoteJSONNotSupported);
                    } else {
                        Cow::Borrowed(Path::new(json_source))
                    }
                } else {
                    Cow::Borrowed(Path::new(json_source))
                };

                let file = fs_err::File::open(json_source.as_ref())?;

                serde_json::from_reader(file)
                    .map_err(|e| Error::InvalidPythonDownloadsJSON(json_source.to_path_buf(), e))?
            } else {
                serde_json::from_str(BUILTIN_PYTHON_DOWNLOADS_JSON).map_err(|e| {
                    Error::InvalidPythonDownloadsJSON(PathBuf::from("EMBEDDED IN THE BINARY"), e)
                })?
            };

            let result = parse_json_downloads(json_downloads);
            Ok(Cow::Owned(result))
        })?;

        Ok(downloads.iter())
    }

    pub fn url(&self) -> &'static str {
        self.url
    }

    pub fn key(&self) -> &PythonInstallationKey {
        &self.key
    }

    pub fn os(&self) -> &Os {
        self.key.os()
    }

    pub fn sha256(&self) -> Option<&'static str> {
        self.sha256
    }

    /// Download and extract a Python distribution, retrying on failure.
    #[instrument(skip(client, installation_dir, scratch_dir, reporter), fields(download = % self.key()))]
    pub async fn fetch_with_retry(
        &self,
        client: &BaseClient,
        installation_dir: &Path,
        scratch_dir: &Path,
        reinstall: bool,
        python_install_mirror: Option<&str>,
        pypy_install_mirror: Option<&str>,
        reporter: Option<&dyn Reporter>,
    ) -> Result<DownloadResult, Error> {
        let mut n_past_retries = 0;
        let start_time = SystemTime::now();
        let retry_policy = client.retry_policy();
        loop {
            let result = self
                .fetch(
                    client,
                    installation_dir,
                    scratch_dir,
                    reinstall,
                    python_install_mirror,
                    pypy_install_mirror,
                    reporter,
                )
                .await;
            if result
                .as_ref()
                .err()
                .is_some_and(|err| is_extended_transient_error(err))
            {
                let retry_decision = retry_policy.should_retry(start_time, n_past_retries);
                if let reqwest_retry::RetryDecision::Retry { execute_after } = retry_decision {
                    debug!(
                        "Transient failure while handling response for {}; retrying...",
                        self.key()
                    );
                    let duration = execute_after
                        .duration_since(SystemTime::now())
                        .unwrap_or_else(|_| Duration::default());
                    tokio::time::sleep(duration).await;
                    n_past_retries += 1;
                    continue;
                }
            }
            return result;
        }
    }

    /// Download and extract a Python distribution.
    #[instrument(skip(client, installation_dir, scratch_dir, reporter), fields(download = % self.key()))]
    pub async fn fetch(
        &self,
        client: &BaseClient,
        installation_dir: &Path,
        scratch_dir: &Path,
        reinstall: bool,
        python_install_mirror: Option<&str>,
        pypy_install_mirror: Option<&str>,
        reporter: Option<&dyn Reporter>,
    ) -> Result<DownloadResult, Error> {
        let url = self.download_url(python_install_mirror, pypy_install_mirror)?;
        let path = installation_dir.join(self.key().to_string());

        // If it is not a reinstall and the dir already exists, return it.
        if !reinstall && path.is_dir() {
            return Ok(DownloadResult::AlreadyAvailable(path));
        }

        // We improve filesystem compatibility by using neither the URL-encoded `%2B` nor the `+` it
        // decodes to.
        let filename = url
            .path_segments()
            .ok_or_else(|| Error::InvalidUrlFormat(url.clone()))?
            .next_back()
            .ok_or_else(|| Error::InvalidUrlFormat(url.clone()))?
            .replace("%2B", "-");
        debug_assert!(
            filename
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.'),
            "Unexpected char in filename: {filename}"
        );
        let ext = SourceDistExtension::from_path(&filename)
            .map_err(|err| Error::MissingExtension(url.to_string(), err))?;

        let temp_dir = tempfile::tempdir_in(scratch_dir).map_err(Error::DownloadDirError)?;

        if let Some(python_builds_dir) = env::var_os(EnvVars::UV_PYTHON_CACHE_DIR) {
            let python_builds_dir = PathBuf::from(python_builds_dir);
            fs_err::create_dir_all(&python_builds_dir)?;
            let hash_prefix = match self.sha256 {
                Some(sha) => {
                    // Shorten the hash to avoid too-long-filename errors
                    &sha[..9]
                }
                None => "none",
            };
            let target_cache_file = python_builds_dir.join(format!("{hash_prefix}-{filename}"));

            // Download the archive to the cache, or return a reader if we have it in cache.
            // TODO(konsti): We should "tee" the write so we can do the download-to-cache and unpacking
            // in one step.
            let (reader, size): (Box<dyn AsyncRead + Unpin>, Option<u64>) =
                match fs_err::tokio::File::open(&target_cache_file).await {
                    Ok(file) => {
                        debug!(
                            "Extracting existing `{}`",
                            target_cache_file.simplified_display()
                        );
                        let size = file.metadata().await?.len();
                        let reader = Box::new(tokio::io::BufReader::new(file));
                        (reader, Some(size))
                    }
                    Err(err) if err.kind() == io::ErrorKind::NotFound => {
                        // Point the user to which file is missing where and where to download it
                        if client.connectivity().is_offline() {
                            return Err(Error::OfflinePythonMissing {
                                file: Box::new(self.key().clone()),
                                url: Box::new(url),
                                python_builds_dir,
                            });
                        }

                        self.download_archive(
                            &url,
                            client,
                            reporter,
                            &python_builds_dir,
                            &target_cache_file,
                        )
                        .await?;

                        debug!("Extracting `{}`", target_cache_file.simplified_display());
                        let file = fs_err::tokio::File::open(&target_cache_file).await?;
                        let size = file.metadata().await?.len();
                        let reader = Box::new(tokio::io::BufReader::new(file));
                        (reader, Some(size))
                    }
                    Err(err) => return Err(err.into()),
                };

            // Extract the downloaded archive into a temporary directory.
            self.extract_reader(
                reader,
                temp_dir.path(),
                &filename,
                ext,
                size,
                reporter,
                Direction::Extract,
            )
            .await?;
        } else {
            // Avoid overlong log lines
            debug!("Downloading {url}");
            debug!(
                "Extracting {filename} to temporary location: {}",
                temp_dir.path().simplified_display()
            );

            let (reader, size) = read_url(&url, client).await?;
            self.extract_reader(
                reader,
                temp_dir.path(),
                &filename,
                ext,
                size,
                reporter,
                Direction::Download,
            )
            .await?;
        }

        // Extract the top-level directory.
        let mut extracted = match uv_extract::strip_component(temp_dir.path()) {
            Ok(top_level) => top_level,
            Err(uv_extract::Error::NonSingularArchive(_)) => temp_dir.keep(),
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
            match fs_err::os::unix::fs::symlink(
                format!("python{}.{}", self.key.major, self.key.minor),
                extracted.join("bin").join("python"),
            ) {
                Ok(()) => {}
                Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {}
                Err(err) => return Err(err.into()),
            }
        }

        // Remove the target if it already exists.
        if path.is_dir() {
            debug!("Removing existing directory: {}", path.user_display());
            fs_err::tokio::remove_dir_all(&path).await?;
        }

        // Persist it to the target.
        debug!("Moving {} to {}", extracted.display(), path.user_display());
        rename_with_retry(extracted, &path)
            .await
            .map_err(|err| Error::CopyError {
                to: path.clone(),
                err,
            })?;

        Ok(DownloadResult::Fetched(path))
    }

    /// Download the managed Python archive into the cache directory.
    async fn download_archive(
        &self,
        url: &Url,
        client: &BaseClient,
        reporter: Option<&dyn Reporter>,
        python_builds_dir: &Path,
        target_cache_file: &Path,
    ) -> Result<(), Error> {
        debug!(
            "Downloading {} to `{}`",
            url,
            target_cache_file.simplified_display()
        );

        let (mut reader, size) = read_url(url, client).await?;
        let temp_dir = tempfile::tempdir_in(python_builds_dir)?;
        let temp_file = temp_dir.path().join("download");

        // Download to a temporary file. We verify the hash when unpacking the file.
        {
            let mut archive_writer = BufWriter::new(fs_err::tokio::File::create(&temp_file).await?);

            // Download with or without progress bar.
            if let Some(reporter) = reporter {
                let key = reporter.on_request_start(Direction::Download, &self.key, size);
                tokio::io::copy(
                    &mut ProgressReader::new(reader, key, reporter),
                    &mut archive_writer,
                )
                .await?;
                reporter.on_request_complete(Direction::Download, key);
            } else {
                tokio::io::copy(&mut reader, &mut archive_writer).await?;
            }

            archive_writer.flush().await?;
        }
        // Move the completed file into place, invalidating the `File` instance.
        fs_err::rename(&temp_file, target_cache_file)?;
        Ok(())
    }

    /// Extract a Python interpreter archive into a (temporary) directory, either from a file or
    /// from a download stream.
    async fn extract_reader(
        &self,
        reader: impl AsyncRead + Unpin,
        target: &Path,
        filename: &String,
        ext: SourceDistExtension,
        size: Option<u64>,
        reporter: Option<&dyn Reporter>,
        direction: Direction,
    ) -> Result<(), Error> {
        let mut hashers = self
            .sha256
            .into_iter()
            .map(|_| Hasher::from(HashAlgorithm::Sha256))
            .collect::<Vec<_>>();
        let mut hasher = uv_extract::hash::HashReader::new(reader, &mut hashers);

        if let Some(reporter) = reporter {
            let progress_key = reporter.on_request_start(direction, &self.key, size);
            let mut reader = ProgressReader::new(&mut hasher, progress_key, reporter);
            uv_extract::stream::archive(&mut reader, ext, target)
                .await
                .map_err(|err| Error::ExtractError(filename.to_string(), err))?;
            reporter.on_request_complete(direction, progress_key);
        } else {
            uv_extract::stream::archive(&mut hasher, ext, target)
                .await
                .map_err(|err| Error::ExtractError(filename.to_string(), err))?;
        }
        hasher.finish().await.map_err(Error::HashExhaustion)?;

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

        Ok(())
    }

    pub fn python_version(&self) -> PythonVersion {
        self.key.version()
    }

    /// Return the [`Url`] to use when downloading the distribution. If a mirror is set via the
    /// appropriate environment variable, use it instead.
    fn download_url(
        &self,
        python_install_mirror: Option<&str>,
        pypy_install_mirror: Option<&str>,
    ) -> Result<Url, Error> {
        match self.key.implementation {
            LenientImplementationName::Known(ImplementationName::CPython) => {
                if let Some(mirror) = python_install_mirror {
                    let Some(suffix) = self.url.strip_prefix(
                        "https://github.com/astral-sh/python-build-standalone/releases/download/",
                    ) else {
                        return Err(Error::Mirror(EnvVars::UV_PYTHON_INSTALL_MIRROR, self.url));
                    };
                    return Ok(Url::parse(
                        format!("{}/{}", mirror.trim_end_matches('/'), suffix).as_str(),
                    )?);
                }
            }

            LenientImplementationName::Known(ImplementationName::PyPy) => {
                if let Some(mirror) = pypy_install_mirror {
                    let Some(suffix) = self.url.strip_prefix("https://downloads.python.org/pypy/")
                    else {
                        return Err(Error::Mirror(EnvVars::UV_PYPY_INSTALL_MIRROR, self.url));
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

fn parse_json_downloads(
    json_downloads: HashMap<String, JsonPythonDownload>,
) -> Vec<ManagedPythonDownload> {
    json_downloads
        .into_iter()
        .filter_map(|(key, entry)| {
            let implementation = match entry.name.as_str() {
                "cpython" => LenientImplementationName::Known(ImplementationName::CPython),
                "pypy" => LenientImplementationName::Known(ImplementationName::PyPy),
                "graalpy" => LenientImplementationName::Known(ImplementationName::GraalPy),
                _ => LenientImplementationName::Unknown(entry.name.clone()),
            };

            let arch_str = match entry.arch.family.as_str() {
                "armv5tel" => "armv5te".to_string(),
                // The `gc` variant of riscv64 is the common base instruction set and
                // is the target in `python-build-standalone`
                // See https://github.com/astral-sh/python-build-standalone/issues/504
                "riscv64" => "riscv64gc".to_string(),
                value => value.to_string(),
            };

            let arch_str = if let Some(variant) = entry.arch.variant {
                format!("{arch_str}_{variant}")
            } else {
                arch_str
            };

            let arch = match Arch::from_str(&arch_str) {
                Ok(arch) => arch,
                Err(e) => {
                    debug!("Skipping entry {key}: Invalid arch '{arch_str}' - {e}");
                    return None;
                }
            };

            let os = match Os::from_str(&entry.os) {
                Ok(os) => os,
                Err(e) => {
                    debug!("Skipping entry {}: Invalid OS '{}' - {}", key, entry.os, e);
                    return None;
                }
            };

            let libc = match Libc::from_str(&entry.libc) {
                Ok(libc) => libc,
                Err(e) => {
                    debug!(
                        "Skipping entry {}: Invalid libc '{}' - {}",
                        key, entry.libc, e
                    );
                    return None;
                }
            };

            let variant = match entry
                .variant
                .as_deref()
                .map(PythonVariant::from_str)
                .transpose()
            {
                Ok(Some(variant)) => variant,
                Ok(None) => PythonVariant::default(),
                Err(()) => {
                    debug!(
                        "Skipping entry {key}: Unknown python variant - {}",
                        entry.variant.unwrap_or_default()
                    );
                    return None;
                }
            };

            let version_str = format!(
                "{}.{}.{}{}",
                entry.major,
                entry.minor,
                entry.patch,
                entry.prerelease.as_deref().unwrap_or_default()
            );

            let version = match PythonVersion::from_str(&version_str) {
                Ok(version) => version,
                Err(e) => {
                    debug!("Skipping entry {key}: Invalid version '{version_str}' - {e}");
                    return None;
                }
            };

            let url = Box::leak(entry.url.into_boxed_str()) as &'static str;
            let sha256 = entry
                .sha256
                .map(|s| Box::leak(s.into_boxed_str()) as &'static str);

            Some(ManagedPythonDownload {
                key: PythonInstallationKey::new_from_version(
                    implementation,
                    &version,
                    os,
                    arch,
                    libc,
                    variant,
                ),
                url,
                sha256,
            })
        })
        .sorted_by(|a, b| Ord::cmp(&b.key, &a.key))
        .collect()
}

impl Error {
    pub(crate) fn from_reqwest(url: DisplaySafeUrl, err: reqwest::Error) -> Self {
        Self::NetworkError(url, WrappedReqwestError::from(err))
    }

    pub(crate) fn from_reqwest_middleware(
        url: DisplaySafeUrl,
        err: reqwest_middleware::Error,
    ) -> Self {
        match err {
            reqwest_middleware::Error::Middleware(error) => {
                Self::NetworkMiddlewareError(url, error)
            }
            reqwest_middleware::Error::Reqwest(error) => {
                Self::NetworkError(url, WrappedReqwestError::from(error))
            }
        }
    }
}

impl Display for ManagedPythonDownload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.key)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Download,
    Extract,
}

impl Direction {
    fn as_str(&self) -> &str {
        match self {
            Direction::Download => "download",
            Direction::Extract => "extract",
        }
    }
}

impl Display for Direction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

pub trait Reporter: Send + Sync {
    fn on_request_start(
        &self,
        direction: Direction,
        name: &PythonInstallationKey,
        size: Option<u64>,
    ) -> usize;
    fn on_request_progress(&self, id: usize, inc: u64);
    fn on_request_complete(&self, direction: Direction, id: usize);
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
                    .on_request_progress(self.index, buf.filled().len() as u64);
            })
    }
}

/// Convert a [`Url`] into an [`AsyncRead`] stream.
async fn read_url(
    url: &Url,
    client: &BaseClient,
) -> Result<(impl AsyncRead + Unpin, Option<u64>), Error> {
    let url = DisplaySafeUrl::from(url.clone());
    if url.scheme() == "file" {
        // Loads downloaded distribution from the given `file://` URL.
        let path = url
            .to_file_path()
            .map_err(|()| Error::InvalidFileUrl(url.to_string()))?;

        let size = fs_err::tokio::metadata(&path).await?.len();
        let reader = fs_err::tokio::File::open(&path).await?;

        Ok((Either::Left(reader), Some(size)))
    } else {
        let response = client
            .for_host(&url)
            .get(Url::from(url.clone()))
            .send()
            .await
            .map_err(|err| Error::from_reqwest_middleware(url.clone(), err))?;

        // Ensure the request was successful.
        response
            .error_for_status_ref()
            .map_err(|err| Error::from_reqwest(url, err))?;

        let size = response.content_length();
        let stream = response
            .bytes_stream()
            .map_err(io::Error::other)
            .into_async_read();

        Ok((Either::Right(stream.compat()), size))
    }
}
