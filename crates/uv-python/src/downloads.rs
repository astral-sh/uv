use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::fmt::Display;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::str::FromStr;
use std::task::{Context, Poll};
use std::time::{Duration, Instant, SystemTime, SystemTimeError};
use std::{env, io};

use futures::TryStreamExt;
use itertools::Itertools;
use owo_colors::OwoColorize;
use reqwest_retry::RetryError;
use reqwest_retry::policies::ExponentialBackoff;
use serde::Deserialize;
use thiserror::Error;
use tokio::io::{
    AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader, BufWriter, ReadBuf,
};
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tokio_util::either::Either;
use tracing::{debug, instrument};
use url::Url;

use uv_cache::{Cache, CacheBucket, CacheEntry, CacheShard};
use uv_cache_info::Timestamp;
use uv_cache_key::cache_digest;
use uv_client::{
    BaseClient, RetriableError, WrappedReqwestError, fetch_with_url_fallback,
    retryable_on_request_failure,
};
use uv_distribution_filename::{ExtensionError, SourceDistExtension};
use uv_extract::hash::Hasher;
use uv_fs::{Simplified, rename_with_retry, write_atomic};
use uv_platform::{self as platform, Arch, Libc, Os, Platform};
use uv_pypi_types::{HashAlgorithm, HashDigest};
use uv_redacted::{DisplaySafeUrl, DisplaySafeUrlError};
use uv_static::EnvVars;

use crate::PythonVariant;
use crate::implementation::{
    Error as ImplementationError, ImplementationName, LenientImplementationName,
};
use crate::installation::PythonInstallationKey;
use crate::managed::ManagedPythonInstallation;
use crate::python_version::{BuildVersionError, python_build_version_from_env};
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
    #[error("Invalid request key (empty request)")]
    EmptyRequest,
    #[error("Invalid request key (too many parts): {0}")]
    TooManyParts(String),
    #[error("Failed to download {0}")]
    NetworkError(DisplaySafeUrl, #[source] WrappedReqwestError),
    #[error(
        "Request failed after {retries} {subject} in {duration:.1}s",
        subject = if *retries > 1 { "retries" } else { "retry" },
        duration = duration.as_secs_f32()
    )]
    NetworkErrorWithRetries {
        #[source]
        err: Box<Self>,
        retries: u32,
        duration: Duration,
    },
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
    InvalidUrl(#[from] DisplaySafeUrlError),
    #[error("Invalid download URL: {0}")]
    InvalidUrlFormat(DisplaySafeUrl),
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
    Mirror(&'static str, String),
    #[error("Failed to determine the libc used on the current platform")]
    LibcDetection(#[from] platform::LibcDetectionError),
    #[error("Unable to parse the JSON Python download list at {0}")]
    InvalidPythonDownloadsJSON(String, #[source] serde_json::Error),
    #[error("This version of uv is too old to support the JSON Python download list at {0}")]
    UnsupportedPythonDownloadsJSON(String),
    #[error("Error while fetching remote python downloads json from '{0}'")]
    FetchingPythonDownloadsJSONError(String, #[source] Box<Self>),
    #[error("Unable to parse NDJSON line at {0}")]
    InvalidPythonDownloadsNdjsonLine(String, #[source] serde_json::Error),
    #[error("Error while fetching remote python downloads NDJSON from '{0}'")]
    FetchingPythonDownloadsNdjsonError(String, #[source] Box<Self>),
    #[error("An offline Python installation was requested, but {file} (from {url}) is missing in {}", python_builds_dir.user_display())]
    OfflinePythonMissing {
        file: Box<PythonInstallationKey>,
        url: Box<DisplaySafeUrl>,
        python_builds_dir: PathBuf,
    },
    #[error(transparent)]
    BuildVersion(#[from] BuildVersionError),
    #[error("No download URL found for Python")]
    NoPythonDownloadUrlFound,
    #[error(transparent)]
    SystemTime(#[from] SystemTimeError),
}

impl RetriableError for Error {
    // Return the number of retries that were made to complete this request before this error was
    // returned.
    //
    // Note that e.g. 3 retries equates to 4 attempts.
    fn retries(&self) -> u32 {
        // Unfortunately different variants of `Error` track retry counts in different ways. We
        // could consider unifying the variants we handle here in `Error::from_reqwest_middleware`
        // instead, but both approaches will be fragile as new variants get added over time.
        if let Self::NetworkErrorWithRetries { retries, .. } = self {
            return *retries;
        }
        if let Self::NetworkMiddlewareError(_, anyhow_error) = self
            && let Some(RetryError::WithRetries { retries, .. }) =
                anyhow_error.downcast_ref::<RetryError>()
        {
            return *retries;
        }
        0
    }

    /// Returns `true` if trying an alternative URL makes sense after this error.
    ///
    /// HTTP-level failures (4xx, 5xx) and connection-level failures return `true`.
    /// Hash mismatches, extraction failures, and similar post-download errors return `false`
    /// because switching to a different host would not fix them.
    fn should_try_next_url(&self) -> bool {
        match self {
            // There are two primary reasons to try an alternative URL:
            // - HTTP/DNS/TCP/etc errors due to a mirror being blocked at various layers
            // - HTTP 404s from the mirror, which may mean the next URL still works
            // So we catch all network-level errors here.
            Self::NetworkError(..)
            | Self::NetworkMiddlewareError(..)
            | Self::NetworkErrorWithRetries { .. } => true,
            // `Io` uses `#[error(transparent)]`, so `source()` delegates to the inner error's
            // own source rather than returning the `io::Error` itself. We must unwrap it
            // explicitly so that `retryable_on_request_failure` can inspect the io error kind.
            Self::Io(err) => retryable_on_request_failure(err).is_some(),
            _ => false,
        }
    }

    fn into_retried(self, retries: u32, duration: Duration) -> Self {
        Self::NetworkErrorWithRetries {
            err: Box::new(self),
            retries,
            duration,
        }
    }
}

/// The URL prefix used by `python-build-standalone` releases on GitHub.
const CPYTHON_DOWNLOADS_URL_PREFIX: &str =
    "https://github.com/astral-sh/python-build-standalone/releases/download/";

/// The default Astral mirror for `python-build-standalone` releases.
///
/// This mirror is tried first for CPython downloads when no user-configured mirror is set.
/// If the mirror fails, uv falls back to the canonical GitHub URL.
const CPYTHON_DOWNLOAD_DEFAULT_MIRROR: &str =
    "https://releases.astral.sh/github/python-build-standalone/releases/download/";

#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub struct ManagedPythonDownload {
    key: PythonInstallationKey,
    url: Cow<'static, str>,
    sha256: Option<Cow<'static, str>>,
    build: Option<&'static str>,
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Hash)]
pub struct PythonDownloadRequest {
    pub(crate) version: Option<VersionRequest>,
    pub(crate) implementation: Option<ImplementationName>,
    pub(crate) arch: Option<ArchRequest>,
    pub(crate) os: Option<Os>,
    pub(crate) libc: Option<Libc>,
    pub(crate) build: Option<String>,

    /// Whether to allow pre-releases or not. If not set, defaults to true if [`Self::version`] is
    /// not None, and false otherwise.
    pub(crate) prereleases: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArchRequest {
    Explicit(Arch),
    Environment(Arch),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PlatformRequest {
    pub(crate) os: Option<Os>,
    pub(crate) arch: Option<ArchRequest>,
    pub(crate) libc: Option<Libc>,
}

impl PlatformRequest {
    /// Check if this platform request is satisfied by a platform.
    pub fn matches(&self, platform: &Platform) -> bool {
        if let Some(os) = self.os {
            if !platform.os.supports(os) {
                return false;
            }
        }

        if let Some(arch) = self.arch {
            if !arch.satisfied_by(platform) {
                return false;
            }
        }

        if let Some(libc) = self.libc {
            if platform.libc != libc {
                return false;
            }
        }

        true
    }
}

impl Display for PlatformRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut parts = Vec::new();
        if let Some(os) = &self.os {
            parts.push(os.to_string());
        }
        if let Some(arch) = &self.arch {
            parts.push(arch.to_string());
        }
        if let Some(libc) = &self.libc {
            parts.push(libc.to_string());
        }
        write!(f, "{}", parts.join("-"))
    }
}

impl Display for ArchRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Explicit(arch) | Self::Environment(arch) => write!(f, "{arch}"),
        }
    }
}

impl ArchRequest {
    pub(crate) fn satisfied_by(self, platform: &Platform) -> bool {
        match self {
            Self::Explicit(request) => request == platform.arch,
            Self::Environment(env) => {
                // Check if the environment's platform can run the target platform
                let env_platform = Platform::new(platform.os, env, platform.libc);
                env_platform.supports(platform)
            }
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
            build: None,
            prereleases,
        }
    }

    #[must_use]
    pub fn with_implementation(mut self, implementation: ImplementationName) -> Self {
        match implementation {
            // Pyodide is actually CPython with an Emscripten OS, we paper over that for usability
            ImplementationName::Pyodide => {
                self = self.with_os(Os::new(target_lexicon::OperatingSystem::Emscripten));
                self = self.with_arch(Arch::new(target_lexicon::Architecture::Wasm32, None));
                self = self.with_libc(Libc::Some(target_lexicon::Environment::Musl));
            }
            _ => {
                self.implementation = Some(implementation);
            }
        }
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

    #[must_use]
    pub fn with_build(mut self, build: String) -> Self {
        self.build = Some(build);
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
        let platform = Platform::from_env().map_err(|err| match err {
            platform::Error::LibcDetectionError(err) => Error::LibcDetection(err),
            err => Error::InvalidRequestPlatform(err),
        })?;
        if self.arch.is_none() {
            self.arch = Some(ArchRequest::Environment(platform.arch));
        }
        if self.os.is_none() {
            self.os = Some(platform.os);
        }
        if self.libc.is_none() {
            self.libc = Some(platform.libc);
        }
        Ok(self)
    }

    /// Fill the build field from the environment variable relevant for the [`ImplementationName`].
    pub fn fill_build_from_env(mut self) -> Result<Self, Error> {
        if self.build.is_some() {
            return Ok(self);
        }
        let Some(implementation) = self.implementation else {
            return Ok(self);
        };

        self.build = python_build_version_from_env(implementation)?;
        Ok(self)
    }

    pub fn fill(mut self) -> Result<Self, Error> {
        if self.implementation.is_none() {
            self.implementation = Some(ImplementationName::CPython);
        }
        self = self.fill_platform()?;
        self = self.fill_build_from_env()?;
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

    pub fn take_version(&mut self) -> Option<VersionRequest> {
        self.version.take()
    }

    /// Remove default implementation and platform details so the request only contains
    /// explicitly user-specified segments.
    #[must_use]
    pub fn unset_defaults(self) -> Self {
        let request = self.unset_non_platform_defaults();

        if let Ok(host) = Platform::from_env() {
            request.unset_platform_defaults(&host)
        } else {
            request
        }
    }

    fn unset_non_platform_defaults(mut self) -> Self {
        self.implementation = self
            .implementation
            .filter(|implementation_name| *implementation_name != ImplementationName::default());

        self.version = self
            .version
            .filter(|version| !matches!(version, VersionRequest::Any | VersionRequest::Default));

        // Drop implicit architecture derived from environment so only user overrides remain.
        self.arch = self
            .arch
            .filter(|arch| !matches!(arch, ArchRequest::Environment(_)));

        self
    }

    #[cfg(test)]
    pub(crate) fn unset_defaults_for_host(self, host: &Platform) -> Self {
        self.unset_non_platform_defaults()
            .unset_platform_defaults(host)
    }

    pub(crate) fn unset_platform_defaults(mut self, host: &Platform) -> Self {
        self.os = self.os.filter(|os| *os != host.os);

        self.libc = self.libc.filter(|libc| *libc != host.libc);

        self.arch = self
            .arch
            .filter(|arch| !matches!(arch, ArchRequest::Explicit(explicit_arch) if *explicit_arch == host.arch));

        self
    }

    /// Drop patch and prerelease information so the request can be re-used for upgrades.
    #[must_use]
    pub fn without_patch(mut self) -> Self {
        self.version = self.version.take().map(VersionRequest::only_minor);
        self.prereleases = None;
        self.build = None;
        self
    }

    /// Return a compact string representation suitable for user-facing display.
    ///
    /// The resulting string only includes explicitly-set pieces of the request and returns
    /// [`None`] when no segments are explicitly set.
    pub fn simplified_display(self) -> Option<String> {
        let parts = [
            self.implementation
                .map(|implementation| implementation.to_string()),
            self.version.map(|version| version.to_string()),
            self.os.map(|os| os.to_string()),
            self.arch.map(|arch| arch.to_string()),
            self.libc.map(|libc| libc.to_string()),
        ];

        let joined = parts.into_iter().flatten().collect::<Vec<_>>().join("-");

        if joined.is_empty() {
            None
        } else {
            Some(joined)
        }
    }

    /// Whether this request is satisfied by an installation key.
    pub fn satisfied_by_key(&self, key: &PythonInstallationKey) -> bool {
        // Check platform requirements
        let request = PlatformRequest {
            os: self.os,
            arch: self.arch,
            libc: self.libc,
        };
        if !request.matches(key.platform()) {
            return false;
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
        // First check the key
        if !self.satisfied_by_key(download.key()) {
            return false;
        }

        // Then check the build if specified
        if let Some(ref requested_build) = self.build {
            let Some(download_build) = download.build() else {
                debug!(
                    "Skipping download `{}`: a build version was requested but is not available for this download",
                    download
                );
                return false;
            };

            if download_build != requested_build {
                debug!(
                    "Skipping download `{}`: requested build version `{}` does not match download build version `{}`",
                    download, requested_build, download_build
                );
                return false;
            }
        }

        true
    }

    /// Whether this download request opts-in to pre-release Python versions.
    pub fn allows_prereleases(&self) -> bool {
        self.prereleases.unwrap_or_else(|| {
            self.version
                .as_ref()
                .is_some_and(VersionRequest::allows_prereleases)
        })
    }

    /// Whether this download request opts-in to a debug Python version.
    pub fn allows_debug(&self) -> bool {
        self.version.as_ref().is_some_and(VersionRequest::is_debug)
    }

    /// Whether this download request opts-in to alternative Python implementations.
    pub fn allows_alternative_implementations(&self) -> bool {
        self.implementation
            .is_some_and(|implementation| !matches!(implementation, ImplementationName::CPython))
            || self.os.is_some_and(|os| os.is_emscripten())
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
        let platform = self.platform();
        let interpreter_platform = Platform::from(interpreter.platform());
        if !platform.matches(&interpreter_platform) {
            debug!(
                "Skipping interpreter at `{executable}`: platform `{interpreter_platform}` does not match request `{platform}`",
            );
            return false;
        }
        if let Some(implementation) = self.implementation() {
            if !implementation.matches_interpreter(interpreter) {
                debug!(
                    "Skipping interpreter at `{executable}`: implementation `{}` does not match request `{implementation}`",
                    interpreter.implementation_name(),
                );
                return false;
            }
        }
        true
    }

    /// Extract the platform components of this request.
    pub fn platform(&self) -> PlatformRequest {
        PlatformRequest {
            os: self.os,
            arch: self.arch,
            libc: self.libc,
        }
    }
}

impl TryFrom<&PythonInstallationKey> for PythonDownloadRequest {
    type Error = LenientImplementationName;

    fn try_from(key: &PythonInstallationKey) -> Result<Self, Self::Error> {
        let implementation = match key.implementation().into_owned() {
            LenientImplementationName::Known(name) => name,
            unknown @ LenientImplementationName::Unknown(_) => return Err(unknown),
        };

        Ok(Self::new(
            Some(VersionRequest::MajorMinor(
                key.major(),
                key.minor(),
                *key.variant(),
            )),
            Some(implementation),
            Some(ArchRequest::Explicit(*key.arch())),
            Some(*key.os()),
            Some(*key.libc()),
            Some(key.prerelease().is_some()),
        ))
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
            Some(ArchRequest::Explicit(*key.arch())),
            Some(*key.os()),
            Some(*key.libc()),
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
        #[derive(Debug, Clone)]
        enum Position {
            Start,
            Implementation,
            Version,
            Os,
            Arch,
            Libc,
            End,
        }

        impl Position {
            pub(crate) fn next(&self) -> Self {
                match self {
                    Self::Start => Self::Implementation,
                    Self::Implementation => Self::Version,
                    Self::Version => Self::Os,
                    Self::Os => Self::Arch,
                    Self::Arch => Self::Libc,
                    Self::Libc => Self::End,
                    Self::End => Self::End,
                }
            }
        }

        #[derive(Debug)]
        struct State<'a, P: Iterator<Item = &'a str>> {
            parts: P,
            part: Option<&'a str>,
            position: Position,
            error: Option<Error>,
            count: usize,
        }

        impl<'a, P: Iterator<Item = &'a str>> State<'a, P> {
            fn new(parts: P) -> Self {
                Self {
                    parts,
                    part: None,
                    position: Position::Start,
                    error: None,
                    count: 0,
                }
            }

            fn next_part(&mut self) {
                self.next_position();
                self.part = self.parts.next();
                self.count += 1;
                self.error.take();
            }

            fn next_position(&mut self) {
                self.position = self.position.next();
            }

            fn record_err(&mut self, err: Error) {
                // For now, we only record the first error encountered. We could record all of the
                // errors for a given part, then pick the most appropriate one later.
                self.error.get_or_insert(err);
            }
        }

        if s.is_empty() {
            return Err(Error::EmptyRequest);
        }

        let mut parts = s.split('-');

        let mut implementation = None;
        let mut version = None;
        let mut os = None;
        let mut arch = None;
        let mut libc = None;

        let mut state = State::new(parts.by_ref());
        state.next_part();

        while let Some(part) = state.part {
            match state.position {
                Position::Start => unreachable!("We start before the loop"),
                Position::Implementation => {
                    if part.eq_ignore_ascii_case("any") {
                        state.next_part();
                        continue;
                    }
                    match ImplementationName::from_str(part) {
                        Ok(val) => {
                            implementation = Some(val);
                            state.next_part();
                        }
                        Err(err) => {
                            state.next_position();
                            state.record_err(err.into());
                        }
                    }
                }
                Position::Version => {
                    if part.eq_ignore_ascii_case("any") {
                        state.next_part();
                        continue;
                    }
                    match VersionRequest::from_str(part)
                        .map_err(|_| Error::InvalidPythonVersion(part.to_string()))
                    {
                        // Err(err) if !first_part => return Err(err),
                        Ok(val) => {
                            version = Some(val);
                            state.next_part();
                        }
                        Err(err) => {
                            state.next_position();
                            state.record_err(err);
                        }
                    }
                }
                Position::Os => {
                    if part.eq_ignore_ascii_case("any") {
                        state.next_part();
                        continue;
                    }
                    match Os::from_str(part) {
                        Ok(val) => {
                            os = Some(val);
                            state.next_part();
                        }
                        Err(err) => {
                            state.next_position();
                            state.record_err(err.into());
                        }
                    }
                }
                Position::Arch => {
                    if part.eq_ignore_ascii_case("any") {
                        state.next_part();
                        continue;
                    }
                    match Arch::from_str(part) {
                        Ok(val) => {
                            arch = Some(ArchRequest::Explicit(val));
                            state.next_part();
                        }
                        Err(err) => {
                            state.next_position();
                            state.record_err(err.into());
                        }
                    }
                }
                Position::Libc => {
                    if part.eq_ignore_ascii_case("any") {
                        state.next_part();
                        continue;
                    }
                    match Libc::from_str(part) {
                        Ok(val) => {
                            libc = Some(val);
                            state.next_part();
                        }
                        Err(err) => {
                            state.next_position();
                            state.record_err(err.into());
                        }
                    }
                }
                Position::End => {
                    if state.count > 5 {
                        return Err(Error::TooManyParts(s.to_string()));
                    }

                    // Throw the first error for the current part
                    //
                    // TODO(zanieb): It's plausible another error variant is a better match but it
                    // sounds hard to explain how? We could peek at the next item in the parts, and
                    // see if that informs the type of this one, or we could use some sort of
                    // similarity or common error matching, but this sounds harder.
                    if let Some(err) = state.error {
                        return Err(err);
                    }
                    state.next_part();
                }
            }
        }

        Ok(Self::new(version, implementation, arch, os, libc, None))
    }
}

const BUILTIN_PYTHON_DOWNLOADS_JSON: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/download-metadata-minified.json"));

/// Default URL for runtime Python download metadata.
const REMOTE_PYTHON_DOWNLOAD_METADATA_URL: &str = "https://raw.githubusercontent.com/astral-sh/versions/refs/heads/main/v1/python-build-standalone.ndjson";

const VERSIONS_CACHE_FILENAME: &str = "python-build-standalone.ndjson";
const VERSIONS_CACHE_META_FILENAME: &str = "python-build-standalone.meta.json";
const VERSIONS_CACHE_FRESHNESS: Duration = Duration::from_secs(10 * 60);
// 2025-03-11, the first CPython release date whose musl builds are dynamically linked.
const CPYTHON_MUSL_STATIC_RELEASE_END: u64 = 2025 * 10_000 + 3 * 100 + 11;
const NDJSON_FLAVOR_PREFERENCES: &[&str] = &[
    "install_only_stripped",
    "install_only",
    "shared-pgo",
    "shared-noopt",
    "static-noopt",
];
const NDJSON_KNOWN_FLAVORS: &[&str] = &["full", "install_only", "install_only_stripped"];

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct VersionsCacheMeta {
    content_length: u64,
    etag: Option<String>,
    checked_at: Timestamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DownloadListFormat {
    Json,
    Ndjson,
}

#[derive(Debug, Clone)]
struct DownloadListSource<'a> {
    location: DownloadListLocation<'a>,
    format: DownloadListFormat,
    implicit: bool,
}

#[derive(Debug, Clone)]
enum DownloadListLocation<'a> {
    Path(Cow<'a, Path>),
    Http(DisplaySafeUrl),
}

pub struct ManagedPythonDownloadList {
    downloads: Vec<ManagedPythonDownload>,
}

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
    build: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
struct JsonArch {
    family: String,
    variant: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
struct NdjsonPythonVersionInfo {
    version: String,
    artifacts: Vec<NdjsonPythonArtifact>,
}

#[derive(Debug, Deserialize, Clone)]
struct NdjsonPythonArtifact {
    platform: String,
    variant: String,
    url: String,
    sha256: Option<String>,
}

#[derive(Debug, Clone)]
pub enum DownloadResult {
    AlreadyAvailable(PathBuf),
    Fetched(PathBuf),
}

fn detect_download_list_format(url_or_path: &str) -> DownloadListFormat {
    let path = Url::parse(url_or_path)
        .ok()
        .filter(|url| matches!(url.scheme(), "http" | "https" | "file"))
        .map(|url| url.path().to_owned());
    let path = path.as_deref().unwrap_or(url_or_path);

    if path.ends_with(".ndjson") {
        DownloadListFormat::Ndjson
    } else {
        DownloadListFormat::Json
    }
}

fn resolve_download_list_source(
    python_downloads_json_url: Option<&str>,
) -> Result<DownloadListSource<'_>, Error> {
    let implicit = python_downloads_json_url.is_none();
    let source = if let Some(source) = python_downloads_json_url {
        Cow::Borrowed(source)
    } else if let Some(source) = env::var_os(EnvVars::UV_INTERNAL__TEST_PYTHON_DOWNLOADS_JSON_URL)
        .filter(|value| !value.is_empty())
        .map(|value| Cow::Owned(value.to_string_lossy().into_owned()))
    {
        source
    } else {
        return Ok(DownloadListSource {
            location: DownloadListLocation::Http(
                DisplaySafeUrl::parse(REMOTE_PYTHON_DOWNLOAD_METADATA_URL)
                    .expect("default remote Python download metadata URL should be valid"),
            ),
            format: DownloadListFormat::Ndjson,
            implicit,
        });
    };

    let format = detect_download_list_format(&source);
    let location = if let Ok(url) = DisplaySafeUrl::parse(&source) {
        match url.scheme() {
            "http" | "https" => DownloadListLocation::Http(url),
            "file" => DownloadListLocation::Path(Cow::Owned(
                url.to_file_path().or(Err(Error::InvalidUrlFormat(url)))?,
            )),
            _ => DownloadListLocation::Path(Cow::Owned(PathBuf::from(source.as_ref()))),
        }
    } else {
        DownloadListLocation::Path(Cow::Owned(PathBuf::from(source.as_ref())))
    };

    Ok(DownloadListSource {
        location,
        format,
        implicit,
    })
}

impl DownloadListSource<'_> {
    fn merge_downloads(
        &self,
        downloads: Vec<ManagedPythonDownload>,
        filter: Option<&PythonDownloadRequest>,
        limit: Option<usize>,
    ) -> Result<Vec<ManagedPythonDownload>, Error> {
        if self.implicit {
            merge_with_embedded_non_cpython(downloads, filter, limit)
        } else {
            Ok(filter_downloads(downloads, filter, limit))
        }
    }

    fn find_in_implicit_embedded_non_cpython(
        &self,
        request: &PythonDownloadRequest,
    ) -> Result<Option<ManagedPythonDownload>, Error> {
        if self.implicit {
            find_in_embedded_non_cpython(request)
        } else {
            Ok(None)
        }
    }

    fn on_implicit_ndjson_parse_error<T>(
        &self,
        err: Error,
        fallback: impl FnOnce() -> Result<T, Error>,
    ) -> Result<T, Error> {
        if self.implicit {
            debug!(
                "Falling back to embedded Python downloads metadata after NDJSON parse failure: {err}"
            );
            fallback()
        } else {
            Err(err)
        }
    }
}

fn versions_cache_shard_key(url: &DisplaySafeUrl) -> String {
    if url.as_str() == REMOTE_PYTHON_DOWNLOAD_METADATA_URL {
        "versions/default".to_string()
    } else {
        let unredacted_url = url.as_str();
        format!("versions/url/{}", cache_digest(&unredacted_url))
    }
}

fn versions_cache_shard(cache: &Cache, url: &DisplaySafeUrl) -> CacheShard {
    cache.shard(CacheBucket::Python, versions_cache_shard_key(url))
}

fn versions_cache_entries(shard: &CacheShard) -> (CacheEntry, CacheEntry) {
    (
        shard.entry(VERSIONS_CACHE_FILENAME),
        shard.entry(VERSIONS_CACHE_META_FILENAME),
    )
}

async fn read_versions_cache(
    content_entry: &CacheEntry,
    meta_entry: &CacheEntry,
) -> Option<(Vec<u8>, VersionsCacheMeta)> {
    let meta_bytes = fs_err::tokio::read(meta_entry.path()).await.ok()?;
    let meta: VersionsCacheMeta = serde_json::from_slice(&meta_bytes).ok()?;
    let content = fs_err::tokio::read(content_entry.path()).await.ok()?;
    if content.len() as u64 != meta.content_length {
        debug!(
            "Cached Python downloads metadata length mismatch: expected {}, got {}",
            meta.content_length,
            content.len()
        );
        return None;
    }
    Some((content, meta))
}

fn versions_cache_is_fresh(meta: &VersionsCacheMeta) -> bool {
    let Some(revalidate_after) = SystemTime::now().checked_sub(VERSIONS_CACHE_FRESHNESS) else {
        return false;
    };
    meta.checked_at >= Timestamp::from(revalidate_after)
}

async fn write_versions_cache_meta(
    meta_entry: &CacheEntry,
    meta: &VersionsCacheMeta,
) -> Result<(), Error> {
    fs_err::tokio::create_dir_all(meta_entry.dir()).await?;
    let meta_bytes = serde_json::to_vec(meta)
        .map_err(|err| io::Error::other(format!("Failed to serialize cache metadata: {err}")))?;
    write_atomic(meta_entry.path(), &meta_bytes).await?;
    Ok(())
}

async fn write_versions_cache(
    content_entry: &CacheEntry,
    meta_entry: &CacheEntry,
    content: &[u8],
    meta: &VersionsCacheMeta,
) -> Result<(), Error> {
    fs_err::tokio::create_dir_all(content_entry.dir()).await?;
    write_atomic(content_entry.path(), content).await?;
    write_versions_cache_meta(meta_entry, meta).await?;
    Ok(())
}

fn validate_ndjson_bytes(source: &str, buf: &[u8]) -> Result<(), Error> {
    parse_ndjson_bytes_with(source, buf, |_| ControlFlow::<()>::Continue(()))?;
    Ok(())
}

fn ndjson_cache_content_is_valid(source: &str, content: &[u8]) -> bool {
    match validate_ndjson_bytes(source, content) {
        Ok(()) => true,
        Err(err) => {
            debug!(
                "Skipping Python downloads metadata cache write because NDJSON did not parse: {err}"
            );
            false
        }
    }
}

async fn write_versions_cache_if_valid(
    content_entry: &CacheEntry,
    meta_entry: &CacheEntry,
    source: &str,
    content: &[u8],
    meta: &VersionsCacheMeta,
) {
    if ndjson_cache_content_is_valid(source, content)
        && let Err(err) = write_versions_cache(content_entry, meta_entry, content, meta).await
    {
        debug!("Failed to write cached Python downloads metadata: {err}");
    }
}

async fn prepend_versions_cache_content(
    content_entry: &CacheEntry,
    new_content: &[u8],
) -> Result<Vec<u8>, Error> {
    let existing = fs_err::tokio::read(content_entry.path()).await?;
    let mut combined = Vec::with_capacity(new_content.len() + existing.len());
    combined.extend_from_slice(new_content);
    combined.extend_from_slice(&existing);
    Ok(combined)
}

async fn fetch_ndjson_cached(
    client: &BaseClient,
    url: &DisplaySafeUrl,
    cache: Option<&Cache>,
) -> Result<Vec<u8>, Error> {
    let Some(cache) = cache else {
        return fetch_bytes_from_url(client, url).await;
    };

    let shard = versions_cache_shard(cache, url);
    let _lock = shard
        .lock()
        .await
        .map_err(|err| io::Error::other(format!("Failed to lock Python downloads cache: {err}")))?;
    let (content_entry, meta_entry) = versions_cache_entries(&shard);
    let cached = read_versions_cache(&content_entry, &meta_entry).await;
    let source = url.to_string();

    if client.connectivity().is_offline() {
        if let Some((content, _)) = cached {
            debug!("Using cached Python downloads metadata in offline mode");
            return Ok(content);
        }
        return fetch_bytes_from_url(client, url).await;
    }

    if let Some((content, meta)) = &cached
        && versions_cache_is_fresh(meta)
    {
        debug!("Using fresh cached Python downloads metadata without revalidation");
        return Ok(content.clone());
    }

    let head_result = client
        .for_host(url)
        .head(Url::from(url.clone()))
        .send()
        .await;
    let head_response = match head_result {
        Ok(response) => match response.error_for_status() {
            Ok(response) => Some(response),
            Err(err) => {
                debug!("Failed to validate Python downloads metadata with HEAD request: {err}");
                None
            }
        },
        Err(err) => {
            debug!("Failed to send HEAD request for Python downloads metadata: {err}");
            None
        }
    };

    let Some(head_response) = head_response else {
        return match fetch_bytes_from_url(client, url).await {
            Ok(content) => {
                let meta = VersionsCacheMeta {
                    content_length: content.len() as u64,
                    etag: None,
                    checked_at: Timestamp::now(),
                };
                write_versions_cache_if_valid(
                    &content_entry,
                    &meta_entry,
                    &source,
                    &content,
                    &meta,
                )
                .await;
                Ok(content)
            }
            Err(err) => {
                if let Some((content, _)) = cached {
                    debug!("Using stale cached Python downloads metadata after HEAD failure");
                    Ok(content)
                } else {
                    Err(err)
                }
            }
        };
    };

    let current_length = head_response
        .headers()
        .get(reqwest::header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());
    let current_etag = head_response
        .headers()
        .get(reqwest::header::ETAG)
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned);

    if let Some((cached_content, cached_meta)) = &cached {
        if current_etag.is_some() && current_etag == cached_meta.etag {
            debug!("Using cached Python downloads metadata with matching ETag");
            let meta = VersionsCacheMeta {
                checked_at: Timestamp::now(),
                ..cached_meta.clone()
            };
            if let Err(err) = write_versions_cache_meta(&meta_entry, &meta).await {
                debug!("Failed to refresh Python downloads cache metadata: {err}");
            }
            return Ok(cached_content.clone());
        }

        if current_etag.is_none() && current_length == Some(cached_meta.content_length) {
            debug!("Using cached Python downloads metadata with unchanged content length");
            let meta = VersionsCacheMeta {
                checked_at: Timestamp::now(),
                ..cached_meta.clone()
            };
            if let Err(err) = write_versions_cache_meta(&meta_entry, &meta).await {
                debug!("Failed to refresh Python downloads cache metadata: {err}");
            }
            return Ok(cached_content.clone());
        }

        if let Some(current_length) = current_length
            && current_length > cached_meta.content_length
        {
            let delta_size = current_length - cached_meta.content_length;
            if delta_size > 0 {
                let range_header = format!("bytes=0-{}", delta_size - 1);
                match client
                    .for_host(url)
                    .get(Url::from(url.clone()))
                    .header(reqwest::header::RANGE, &range_header)
                    .send()
                    .await
                {
                    Ok(response) if response.status() == reqwest::StatusCode::PARTIAL_CONTENT => {
                        let delta_bytes = response.bytes().await.map_err(|err| {
                            Error::from_reqwest(url.clone(), err, None, Instant::now())
                        })?;
                        let meta = VersionsCacheMeta {
                            content_length: current_length,
                            etag: current_etag.clone(),
                            checked_at: Timestamp::now(),
                        };
                        match prepend_versions_cache_content(&content_entry, &delta_bytes).await {
                            Ok(combined) => {
                                write_versions_cache_if_valid(
                                    &content_entry,
                                    &meta_entry,
                                    &source,
                                    &combined,
                                    &meta,
                                )
                                .await;
                                return Ok(combined);
                            }
                            Err(err) => {
                                debug!(
                                    "Failed to update cached Python downloads metadata with delta: {err}"
                                );
                            }
                        }
                    }
                    Ok(_) => {
                        debug!("Python downloads metadata server did not honor range request");
                    }
                    Err(err) => {
                        debug!("Failed to fetch Python downloads metadata delta: {err}");
                    }
                }
            }
        }
    }

    match fetch_bytes_from_url(client, url).await {
        Ok(content) => {
            let meta = VersionsCacheMeta {
                content_length: content.len() as u64,
                etag: current_etag,
                checked_at: Timestamp::now(),
            };
            write_versions_cache_if_valid(&content_entry, &meta_entry, &source, &content, &meta)
                .await;
            Ok(content)
        }
        Err(err) => {
            if let Some((content, _)) = cached {
                debug!("Using stale cached Python downloads metadata after fetch failure");
                Ok(content)
            } else {
                Err(err)
            }
        }
    }
}

/// A wrapper type to display a `ManagedPythonDownload` with its build information.
pub struct ManagedPythonDownloadWithBuild<'a>(&'a ManagedPythonDownload);

impl Display for ManagedPythonDownloadWithBuild<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(build) = self.0.build {
            write!(f, "{}+{}", self.0.key, build)
        } else {
            write!(f, "{}", self.0.key)
        }
    }
}

impl ManagedPythonDownloadList {
    /// Iterate over all [`ManagedPythonDownload`]s.
    fn iter_all(&self) -> impl Iterator<Item = &ManagedPythonDownload> {
        self.downloads.iter()
    }

    /// Iterate over all [`ManagedPythonDownload`]s that match the request.
    pub fn iter_matching(
        &self,
        request: &PythonDownloadRequest,
    ) -> impl Iterator<Item = &ManagedPythonDownload> {
        self.iter_all()
            .filter(move |download| request.satisfied_by_download(download))
    }

    /// Return the first [`ManagedPythonDownload`] matching a request, if any.
    ///
    /// If there is no stable version matching the request, a compatible pre-release version will
    /// be searched for — even if a pre-release was not explicitly requested.
    pub fn find(&self, request: &PythonDownloadRequest) -> Result<&ManagedPythonDownload, Error> {
        if let Some(download) = self.iter_matching(request).next() {
            return Ok(download);
        }

        if !request.allows_prereleases() {
            if let Some(download) = self
                .iter_matching(&request.clone().with_prereleases(true))
                .next()
            {
                return Ok(download);
            }
        }

        Err(Error::NoDownloadFound(request.clone()))
    }

    /// Load available Python distributions from a provided source.
    ///
    /// If no explicit source is provided, uv fetches metadata from the default remote NDJSON
    /// endpoint and falls back to the embedded metadata if the fetch or parse fails.
    pub async fn new(
        client: &BaseClient,
        python_downloads_json_url: Option<&str>,
        cache: Option<&Cache>,
    ) -> Result<Self, Error> {
        let source = resolve_download_list_source(python_downloads_json_url)?;

        let downloads = match (&source.location, source.format) {
            (DownloadListLocation::Path(path), DownloadListFormat::Json) => source
                .merge_downloads(
                    parse_json_download_bytes(
                        &path.to_string_lossy(),
                        &fs_err::read(path.as_ref())?,
                    )?,
                    None,
                    None,
                )?,
            (DownloadListLocation::Path(path), DownloadListFormat::Ndjson) => source
                .merge_downloads(
                    parse_ndjson_bytes(&path.to_string_lossy(), &fs_err::read(path.as_ref())?)?,
                    None,
                    None,
                )?,
            (DownloadListLocation::Http(url), DownloadListFormat::Json) => {
                match fetch_bytes_from_url(client, url).await {
                    Ok(buf) => source.merge_downloads(
                        parse_json_download_bytes(&url.to_string(), &buf)?,
                        None,
                        None,
                    )?,
                    Err(err) => {
                        if source.implicit {
                            debug!(
                                "Falling back to embedded Python downloads metadata after JSON fetch failure: {err}"
                            );
                            embedded_downloads()?
                        } else {
                            return Err(Error::FetchingPythonDownloadsJSONError(
                                url.to_string(),
                                Box::new(err),
                            ));
                        }
                    }
                }
            }
            (DownloadListLocation::Http(url), DownloadListFormat::Ndjson) => {
                match fetch_ndjson_cached(client, url, cache).await {
                    Ok(buf) => match parse_ndjson_bytes(&url.to_string(), &buf) {
                        Ok(downloads) => source.merge_downloads(downloads, None, None)?,
                        Err(err) => {
                            source.on_implicit_ndjson_parse_error(err, embedded_downloads)?
                        }
                    },
                    Err(err) => {
                        if source.implicit {
                            debug!(
                                "Falling back to embedded Python downloads metadata after NDJSON fetch failure: {err}"
                            );
                            embedded_downloads()?
                        } else {
                            return Err(Error::FetchingPythonDownloadsNdjsonError(
                                url.to_string(),
                                Box::new(err),
                            ));
                        }
                    }
                }
            }
        };

        Ok(Self { downloads })
    }

    /// Load available Python distributions with optional filtering and limiting.
    ///
    /// For NDJSON sources, this parses matching downloads eagerly and stops once `limit` matches
    /// have been collected.
    pub async fn new_filtered(
        client: &BaseClient,
        python_downloads_json_url: Option<&str>,
        cache: Option<&Cache>,
        filter: Option<&PythonDownloadRequest>,
        limit: Option<usize>,
    ) -> Result<Self, Error> {
        let source = resolve_download_list_source(python_downloads_json_url)?;

        let downloads = match (&source.location, source.format) {
            (DownloadListLocation::Path(path), DownloadListFormat::Ndjson) => {
                let downloads = parse_ndjson_bytes_filtered(
                    &path.to_string_lossy(),
                    &fs_err::read(path.as_ref())?,
                    |download| {
                        filter
                            .map(|request| request.satisfied_by_download(download))
                            .unwrap_or(true)
                    },
                    limit,
                )?;
                source.merge_downloads(downloads, filter, limit)?
            }
            (DownloadListLocation::Http(url), DownloadListFormat::Ndjson) => {
                let source_url = url.to_string();

                if cache.is_some() {
                    match fetch_ndjson_cached(client, url, cache).await {
                        Ok(buf) => match parse_ndjson_bytes_filtered(
                            &source_url,
                            &buf,
                            |download| {
                                filter
                                    .map(|request| request.satisfied_by_download(download))
                                    .unwrap_or(true)
                            },
                            limit,
                        ) {
                            Ok(downloads) => source.merge_downloads(downloads, filter, limit),
                            Err(err) => source.on_implicit_ndjson_parse_error(err, || {
                                Ok(filter_downloads(embedded_downloads()?, filter, limit))
                            }),
                        },
                        Err(err) => {
                            if source.implicit {
                                debug!(
                                    "Falling back to embedded Python downloads metadata after NDJSON fetch failure: {err}"
                                );
                                Ok(filter_downloads(embedded_downloads()?, filter, limit))
                            } else {
                                return Err(Error::FetchingPythonDownloadsNdjsonError(
                                    source_url,
                                    Box::new(err),
                                ));
                            }
                        }
                    }
                } else {
                    match fetch_ndjson_collect(
                        client,
                        url,
                        |download| {
                            filter
                                .map(|request| request.satisfied_by_download(download))
                                .unwrap_or(true)
                        },
                        limit,
                    )
                    .await
                    {
                        Ok(downloads) => source.merge_downloads(downloads, filter, limit),
                        Err(err) => {
                            if source.implicit {
                                debug!(
                                    "Falling back to embedded Python downloads metadata after NDJSON fetch failure: {err}"
                                );
                                Ok(filter_downloads(embedded_downloads()?, filter, limit))
                            } else {
                                return Err(Error::FetchingPythonDownloadsNdjsonError(
                                    source_url,
                                    Box::new(err),
                                ));
                            }
                        }
                    }
                }?
            }
            _ => filter_downloads(
                Self::new(client, python_downloads_json_url, cache)
                    .await?
                    .downloads,
                filter,
                limit,
            ),
        };

        Ok(Self { downloads })
    }

    /// Find a single download matching the request, using early-exit parsing for NDJSON sources.
    pub async fn find_streaming(
        client: &BaseClient,
        python_downloads_json_url: Option<&str>,
        cache: Option<&Cache>,
        request: &PythonDownloadRequest,
    ) -> Result<Option<ManagedPythonDownload>, Error> {
        let source = resolve_download_list_source(python_downloads_json_url)?;

        if let Some(download) = find_matching_download(client, &source, cache, request).await? {
            return Ok(Some(download));
        }

        if request.allows_prereleases() {
            return Ok(None);
        }

        find_matching_download(
            client,
            &source,
            cache,
            &request.clone().with_prereleases(true),
        )
        .await
    }

    /// Load available Python distributions from the compiled-in list only.
    /// for testing purposes.
    pub fn new_only_embedded() -> Result<Self, Error> {
        Ok(Self {
            downloads: embedded_downloads()?,
        })
    }
}

async fn fetch_bytes_from_url(client: &BaseClient, url: &DisplaySafeUrl) -> Result<Vec<u8>, Error> {
    let (mut reader, size) = read_url(url, client).await?;
    let capacity = size.and_then(|s| s.try_into().ok()).unwrap_or(1_048_576);
    let mut buf = Vec::with_capacity(capacity);
    reader.read_to_end(&mut buf).await?;
    Ok(buf)
}

fn embedded_non_cpython_downloads() -> Result<Vec<ManagedPythonDownload>, Error> {
    Ok(embedded_downloads()?
        .into_iter()
        .filter(|download| {
            !matches!(
                download.key().implementation().as_ref(),
                LenientImplementationName::Known(ImplementationName::CPython)
            )
        })
        .collect())
}

fn merge_with_embedded_non_cpython(
    downloads: Vec<ManagedPythonDownload>,
    filter: Option<&PythonDownloadRequest>,
    limit: Option<usize>,
) -> Result<Vec<ManagedPythonDownload>, Error> {
    let mut merged = BTreeMap::new();

    for download in downloads {
        merged.entry(download.key().clone()).or_insert(download);
    }

    for download in filter_downloads(embedded_non_cpython_downloads()?, filter, None) {
        merged.entry(download.key().clone()).or_insert(download);
    }

    let mut downloads = merged.into_values().collect::<Vec<_>>();
    downloads.sort_by(|a, b| Ord::cmp(&b.key, &a.key));

    if let Some(limit) = limit {
        downloads.truncate(limit);
    }

    Ok(downloads)
}

fn find_in_embedded_non_cpython(
    request: &PythonDownloadRequest,
) -> Result<Option<ManagedPythonDownload>, Error> {
    Ok(embedded_non_cpython_downloads()?
        .into_iter()
        .find(|download| request.satisfied_by_download(download)))
}

fn find_in_embedded_downloads(
    request: &PythonDownloadRequest,
) -> Result<Option<ManagedPythonDownload>, Error> {
    Ok(embedded_downloads()?
        .into_iter()
        .find(|download| request.satisfied_by_download(download)))
}

fn filter_downloads(
    mut downloads: Vec<ManagedPythonDownload>,
    filter: Option<&PythonDownloadRequest>,
    limit: Option<usize>,
) -> Vec<ManagedPythonDownload> {
    if let Some(filter) = filter {
        downloads.retain(|download| filter.satisfied_by_download(download));
    }

    if let Some(limit) = limit {
        downloads.truncate(limit);
    }

    downloads
}

fn find_matching_or_implicit_embedded(
    source: &DownloadListSource<'_>,
    download: Option<ManagedPythonDownload>,
    request: &PythonDownloadRequest,
) -> Result<Option<ManagedPythonDownload>, Error> {
    match download {
        Some(download) => Ok(Some(download)),
        None => source.find_in_implicit_embedded_non_cpython(request),
    }
}

async fn find_matching_download(
    client: &BaseClient,
    source: &DownloadListSource<'_>,
    cache: Option<&Cache>,
    request: &PythonDownloadRequest,
) -> Result<Option<ManagedPythonDownload>, Error> {
    match (&source.location, source.format) {
        (DownloadListLocation::Path(path), DownloadListFormat::Ndjson) => {
            let download = parse_ndjson_bytes_find(
                &path.to_string_lossy(),
                &fs_err::read(path.as_ref())?,
                |download| request.satisfied_by_download(download),
            )?;
            find_matching_or_implicit_embedded(source, download, request)
        }
        (DownloadListLocation::Http(url), DownloadListFormat::Ndjson) => {
            let source_url = url.to_string();
            if cache.is_some() {
                match fetch_ndjson_cached(client, url, cache).await {
                    Ok(buf) => match parse_ndjson_bytes_find(&source_url, &buf, |download| {
                        request.satisfied_by_download(download)
                    }) {
                        Ok(download) => {
                            find_matching_or_implicit_embedded(source, download, request)
                        }
                        Err(err) => source.on_implicit_ndjson_parse_error(err, || {
                            find_in_embedded_downloads(request)
                        }),
                    },
                    Err(err) => {
                        if source.implicit {
                            debug!(
                                "Falling back to embedded Python downloads metadata after NDJSON fetch failure: {err}"
                            );
                            find_in_embedded_downloads(request)
                        } else {
                            Err(Error::FetchingPythonDownloadsNdjsonError(
                                source_url,
                                Box::new(err),
                            ))
                        }
                    }
                }
            } else {
                match fetch_ndjson_find(client, url, |download| {
                    request.satisfied_by_download(download)
                })
                .await
                {
                    Ok(download) => find_matching_or_implicit_embedded(source, download, request),
                    Err(err) => {
                        if source.implicit {
                            debug!(
                                "Falling back to embedded Python downloads metadata after NDJSON fetch failure: {err}"
                            );
                            find_in_embedded_downloads(request)
                        } else {
                            Err(Error::FetchingPythonDownloadsNdjsonError(
                                source_url,
                                Box::new(err),
                            ))
                        }
                    }
                }
            }
        }
        (DownloadListLocation::Path(path), DownloadListFormat::Json) => {
            let download =
                parse_json_download_bytes(&path.to_string_lossy(), &fs_err::read(path.as_ref())?)?
                    .into_iter()
                    .find(|download| request.satisfied_by_download(download));
            find_matching_or_implicit_embedded(source, download, request)
        }
        (DownloadListLocation::Http(url), DownloadListFormat::Json) => {
            let source_url = url.to_string();
            match fetch_bytes_from_url(client, url).await {
                Ok(buf) => {
                    let download = parse_json_download_bytes(&source_url, &buf)?
                        .into_iter()
                        .find(|download| request.satisfied_by_download(download));
                    find_matching_or_implicit_embedded(source, download, request)
                }
                Err(err) => {
                    if source.implicit {
                        debug!(
                            "Falling back to embedded Python downloads metadata after JSON fetch failure: {err}"
                        );
                        find_in_embedded_downloads(request)
                    } else {
                        Err(Error::FetchingPythonDownloadsJSONError(
                            source_url,
                            Box::new(err),
                        ))
                    }
                }
            }
        }
    }
}

impl ManagedPythonDownload {
    /// Return a display type that includes the build information.
    pub fn to_display_with_build(&self) -> ManagedPythonDownloadWithBuild<'_> {
        ManagedPythonDownloadWithBuild(self)
    }

    pub fn url(&self) -> &Cow<'static, str> {
        &self.url
    }

    pub fn key(&self) -> &PythonInstallationKey {
        &self.key
    }

    pub fn os(&self) -> &Os {
        self.key.os()
    }

    pub fn sha256(&self) -> Option<&Cow<'static, str>> {
        self.sha256.as_ref()
    }

    pub fn build(&self) -> Option<&'static str> {
        self.build
    }

    /// Download and extract a Python distribution, retrying on failure.
    ///
    /// For CPython without a user-configured mirror, the default Astral mirror is tried first.
    /// Each attempt tries all URLs in sequence without backoff between them; backoff is only
    /// applied after all URLs have been exhausted.
    #[instrument(skip(client, installation_dir, scratch_dir, reporter), fields(download = % self.key()))]
    pub async fn fetch_with_retry(
        &self,
        client: &BaseClient,
        retry_policy: &ExponentialBackoff,
        installation_dir: &Path,
        scratch_dir: &Path,
        reinstall: bool,
        python_install_mirror: Option<&str>,
        pypy_install_mirror: Option<&str>,
        reporter: Option<&dyn Reporter>,
    ) -> Result<DownloadResult, Error> {
        let urls = self.download_urls(python_install_mirror, pypy_install_mirror)?;
        if urls.is_empty() {
            return Err(Error::NoPythonDownloadUrlFound);
        }
        fetch_with_url_fallback(&urls, *retry_policy, &format!("`{}`", self.key()), |url| {
            self.fetch_from_url(
                url,
                client,
                installation_dir,
                scratch_dir,
                reinstall,
                reporter,
            )
        })
        .await
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
        let urls = self.download_urls(python_install_mirror, pypy_install_mirror)?;
        let url = urls
            .into_iter()
            .next()
            .ok_or(Error::NoPythonDownloadUrlFound)?;
        self.fetch_from_url(
            url,
            client,
            installation_dir,
            scratch_dir,
            reinstall,
            reporter,
        )
        .await
    }

    /// Download and extract a Python distribution from the given URL.
    async fn fetch_from_url(
        &self,
        url: DisplaySafeUrl,
        client: &BaseClient,
        installation_dir: &Path,
        scratch_dir: &Path,
        reinstall: bool,
        reporter: Option<&dyn Reporter>,
    ) -> Result<DownloadResult, Error> {
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

        if let Some(python_builds_dir) =
            env::var_os(EnvVars::UV_PYTHON_CACHE_DIR).filter(|s| !s.is_empty())
        {
            let python_builds_dir = PathBuf::from(python_builds_dir);
            fs_err::create_dir_all(&python_builds_dir)?;
            let hash_prefix = match self.sha256.as_deref() {
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
                                url: Box::new(url.clone()),
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
            Err(err) => return Err(Error::ExtractError(filename, err)),
        };

        // If the distribution is a `full` archive, the Python installation is in the `install` directory.
        if extracted.join("install").is_dir() {
            extracted = extracted.join("install");
        // If the distribution is a Pyodide archive, the Python installation is in the `pyodide-root/dist` directory.
        } else if self.os().is_emscripten() {
            extracted = extracted.join("pyodide-root").join("dist");
        }

        #[cfg(unix)]
        {
            // Pyodide distributions require all of the supporting files to be alongside the Python
            // executable, so they don't have a `bin` directory. We create it and link
            // `bin/pythonX.Y` to `dist/python`.
            if self.os().is_emscripten() {
                fs_err::create_dir_all(extracted.join("bin"))?;
                fs_err::os::unix::fs::symlink(
                    "../python",
                    extracted
                        .join("bin")
                        .join(format!("python{}.{}", self.key.major, self.key.minor)),
                )?;
            }

            // If the distribution is missing a `python` -> `pythonX.Y` symlink, add it.
            //
            // Pyodide releases never contain this link by default.
            //
            // PEP 394 permits it, and python-build-standalone releases after `20240726` include it,
            // but releases prior to that date do not.
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
        url: &DisplaySafeUrl,
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
        match rename_with_retry(&temp_file, target_cache_file).await {
            Ok(()) => {}
            Err(_) if target_cache_file.is_file() => {}
            Err(err) => return Err(err.into()),
        }
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
        let mut hashers = if self.sha256.is_some() {
            vec![Hasher::from(HashAlgorithm::Sha256)]
        } else {
            vec![]
        };
        let mut hasher = uv_extract::hash::HashReader::new(reader, &mut hashers);

        if let Some(reporter) = reporter {
            let progress_key = reporter.on_request_start(direction, &self.key, size);
            let mut reader = ProgressReader::new(&mut hasher, progress_key, reporter);
            uv_extract::stream::archive(filename, &mut reader, ext, target)
                .await
                .map_err(|err| Error::ExtractError(filename.to_owned(), err))?;
            reporter.on_request_complete(direction, progress_key);
        } else {
            uv_extract::stream::archive(filename, &mut hasher, ext, target)
                .await
                .map_err(|err| Error::ExtractError(filename.to_owned(), err))?;
        }
        hasher.finish().await.map_err(Error::HashExhaustion)?;

        // Check the hash
        if let Some(expected) = self.sha256.as_deref() {
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

    /// Return the primary [`Url`] to use when downloading the distribution.
    ///
    /// This is the first URL from [`Self::download_urls`]. For CPython without a user-configured
    /// mirror, this is the default Astral mirror URL. Use [`Self::download_urls`] to get all
    /// URLs including fallbacks.
    pub fn download_url(
        &self,
        python_install_mirror: Option<&str>,
        pypy_install_mirror: Option<&str>,
    ) -> Result<DisplaySafeUrl, Error> {
        self.download_urls(python_install_mirror, pypy_install_mirror)
            .map(|mut urls| urls.remove(0))
    }

    /// Return the ordered list of [`Url`]s to try when downloading the distribution.
    ///
    /// For CPython without a user-configured mirror, the default Astral mirror is listed first,
    /// followed by the canonical GitHub URL as a fallback.
    ///
    /// For all other cases (user mirror explicitly set, PyPy, GraalPy, Pyodide), a single URL
    /// is returned with no fallback.
    pub fn download_urls(
        &self,
        python_install_mirror: Option<&str>,
        pypy_install_mirror: Option<&str>,
    ) -> Result<Vec<DisplaySafeUrl>, Error> {
        match self.key.implementation {
            LenientImplementationName::Known(ImplementationName::CPython) => {
                if let Some(mirror) = python_install_mirror {
                    // User-configured mirror: use it exclusively, no automatic fallback.
                    let Some(suffix) = self.url.strip_prefix(CPYTHON_DOWNLOADS_URL_PREFIX) else {
                        return Err(Error::Mirror(
                            EnvVars::UV_PYTHON_INSTALL_MIRROR,
                            self.url.to_string(),
                        ));
                    };
                    return Ok(vec![DisplaySafeUrl::parse(
                        format!("{}/{}", mirror.trim_end_matches('/'), suffix).as_str(),
                    )?]);
                }
                // No user mirror: try the default Astral mirror first, fall back to GitHub.
                if let Some(suffix) = self.url.strip_prefix(CPYTHON_DOWNLOADS_URL_PREFIX) {
                    let mirror_url = DisplaySafeUrl::parse(
                        format!(
                            "{}/{}",
                            CPYTHON_DOWNLOAD_DEFAULT_MIRROR.trim_end_matches('/'),
                            suffix
                        )
                        .as_str(),
                    )?;
                    let canonical_url = DisplaySafeUrl::parse(&self.url)?;
                    return Ok(vec![mirror_url, canonical_url]);
                }
            }

            LenientImplementationName::Known(ImplementationName::PyPy) => {
                if let Some(mirror) = pypy_install_mirror {
                    let Some(suffix) = self.url.strip_prefix("https://downloads.python.org/pypy/")
                    else {
                        return Err(Error::Mirror(
                            EnvVars::UV_PYPY_INSTALL_MIRROR,
                            self.url.to_string(),
                        ));
                    };
                    return Ok(vec![DisplaySafeUrl::parse(
                        format!("{}/{}", mirror.trim_end_matches('/'), suffix).as_str(),
                    )?]);
                }
            }

            _ => {}
        }

        Ok(vec![DisplaySafeUrl::parse(&self.url)?])
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

            let url = Cow::Owned(entry.url);
            let sha256 = entry.sha256.map(Cow::Owned);
            let build = entry
                .build
                .map(|s| Box::leak(s.into_boxed_str()) as &'static str);

            Some(ManagedPythonDownload {
                key: PythonInstallationKey::new_from_version(
                    implementation,
                    &version,
                    Platform::new(os, arch, libc),
                    variant,
                ),
                url,
                sha256,
                build,
            })
        })
        .sorted_by(|a, b| Ord::cmp(&b.key, &a.key))
        .collect()
}

fn embedded_downloads() -> Result<Vec<ManagedPythonDownload>, Error> {
    let json_downloads: HashMap<String, JsonPythonDownload> =
        serde_json::from_slice(BUILTIN_PYTHON_DOWNLOADS_JSON).map_err(|err| {
            Error::InvalidPythonDownloadsJSON("EMBEDDED IN THE BINARY".to_owned(), err)
        })?;
    Ok(parse_json_downloads(json_downloads))
}

fn parse_json_download_bytes(
    source: &str,
    buf: &[u8],
) -> Result<Vec<ManagedPythonDownload>, Error> {
    let json_downloads: HashMap<String, JsonPythonDownload> = serde_json::from_slice(buf).map_err(
        #[expect(clippy::zero_sized_map_values)]
        |err| {
            if let Ok(keys) = serde_json::from_slice::<HashMap<String, serde::de::IgnoredAny>>(buf)
                && keys.contains_key("version")
            {
                Error::UnsupportedPythonDownloadsJSON(source.to_owned())
            } else {
                Error::InvalidPythonDownloadsJSON(source.to_owned(), err)
            }
        },
    )?;
    Ok(parse_json_downloads(json_downloads))
}

fn parse_version_with_build(s: &str) -> Result<(PythonVersion, Option<&str>), Error> {
    if let Some((version_str, build)) = s.split_once('+') {
        let version = PythonVersion::from_str(version_str)
            .map_err(|_| Error::InvalidPythonVersion(s.to_string()))?;
        Ok((version, Some(build)))
    } else {
        let version =
            PythonVersion::from_str(s).map_err(|_| Error::InvalidPythonVersion(s.to_string()))?;
        Ok((version, None))
    }
}

/// Parse one NDJSON version record into managed downloads, selecting the best
/// artifact for each platform and [`PythonVariant`].
fn parse_ndjson_version_info(version_info: NdjsonPythonVersionInfo) -> Vec<ManagedPythonDownload> {
    let (version, build) = match parse_version_with_build(&version_info.version) {
        Ok((version, build)) => (version, build),
        Err(err) => {
            debug!(
                "Skipping NDJSON entry: invalid version '{}' - {}",
                version_info.version, err
            );
            return Vec::new();
        }
    };

    let release = build.and_then(|value| value.parse::<u64>().ok());
    let build = build.map(|value| Box::leak(value.to_owned().into_boxed_str()) as &'static str);

    let mut artifacts = version_info.artifacts;
    // Match the built-in metadata generator's deterministic tie-breaker when two artifacts have
    // the same platform, variant, and priority.
    artifacts.sort_by(|a, b| a.url.cmp(&b.url));

    let mut selected = BTreeMap::new();
    for artifact in artifacts {
        let Some((download, priority)) = parse_ndjson_artifact(&version, build, release, artifact)
        else {
            continue;
        };
        let key = (download.key().platform().clone(), *download.key().variant());

        // Collapse duplicate artifacts for the same platform and variant to
        // the preferred flavor/build-option combination.
        if let Some((existing_download, existing_priority)) = selected.get(&key)
            && priority >= *existing_priority
        {
            debug!(
                "Skipping NDJSON artifact {} (priority {:?}): lower priority than {} (priority {:?})",
                download, priority, existing_download, existing_priority
            );
            continue;
        }

        selected.insert(key, (download, priority));
    }

    selected
        .into_values()
        .map(|(download, _priority)| download)
        .collect()
}

fn parse_ndjson_artifact(
    version: &PythonVersion,
    build: Option<&'static str>,
    release: Option<u64>,
    artifact: NdjsonPythonArtifact,
) -> Option<(ManagedPythonDownload, (usize, i8))> {
    let (platform, mut build_options) = parse_ndjson_platform(&artifact.platform)?;
    let (flavor, variant_build_options) = parse_ndjson_artifact_variant(&artifact.variant);
    build_options.extend(variant_build_options);

    if build_options.contains(&"static") {
        debug!(
            "Skipping NDJSON artifact: static unsupported - {}",
            artifact.url
        );
        return None;
    }

    if release.is_some_and(|release| release < CPYTHON_MUSL_STATIC_RELEASE_END)
        && matches!(platform.libc, Libc::Some(target_lexicon::Environment::Musl))
    {
        return None;
    }

    let variant = python_variant_from_ndjson_build_options(&build_options);
    let priority = ndjson_artifact_priority(flavor, &build_options);

    Some((
        ManagedPythonDownload {
            key: PythonInstallationKey::new_from_version(
                LenientImplementationName::Known(ImplementationName::CPython),
                version,
                platform,
                variant,
            ),
            url: Cow::Owned(artifact.url),
            sha256: artifact.sha256.map(Cow::Owned),
            build,
        },
        priority,
    ))
}

fn parse_ndjson_platform(platform: &str) -> Option<(Platform, Vec<&str>)> {
    let mut platform = platform;
    let mut build_options = Vec::new();

    for (suffix, build_option) in [("-debug", "debug"), ("-freethreaded", "freethreaded")] {
        if let Some(stripped) = platform.strip_suffix(suffix) {
            platform = stripped;
            build_options.push(build_option);
        }
    }

    let platform = match Platform::from_cargo_dist_triple(platform) {
        Ok(platform) => platform,
        Err(err) => {
            debug!(
                "Skipping NDJSON artifact: invalid platform '{}' - {}",
                platform, err
            );
            return None;
        }
    };

    Some((platform, build_options))
}

fn parse_ndjson_artifact_variant(variant: &str) -> (&str, Vec<&str>) {
    let mut parts = variant.split('+').collect::<Vec<_>>();
    if parts
        .last()
        .is_some_and(|flavor| NDJSON_KNOWN_FLAVORS.contains(flavor))
        && let Some(flavor) = parts.pop()
    {
        (flavor, parts)
    } else {
        (variant, Vec::new())
    }
}

fn python_variant_from_ndjson_build_options(build_options: &[&str]) -> PythonVariant {
    let debug = build_options.contains(&"debug");
    let freethreaded = build_options.contains(&"freethreaded");

    match (debug, freethreaded) {
        (true, true) => PythonVariant::FreethreadedDebug,
        (true, false) => PythonVariant::Debug,
        (false, true) => PythonVariant::Freethreaded,
        (false, false) => PythonVariant::default(),
    }
}

fn ndjson_artifact_priority(flavor: &str, build_options: &[&str]) -> (usize, i8) {
    let flavor_priority = NDJSON_FLAVOR_PREFERENCES
        .iter()
        .position(|preference| *preference == flavor)
        .unwrap_or(NDJSON_FLAVOR_PREFERENCES.len() + 1);

    let build_option_priority = -i8::from(build_options.contains(&"lto"))
        - i8::from(build_options.contains(&"pgo"))
        - i8::from(!build_options.contains(&"static"));

    (flavor_priority, build_option_priority)
}

fn parse_ndjson_line(source: &str, line: &[u8]) -> Result<NdjsonPythonVersionInfo, Error> {
    let line_str = std::str::from_utf8(line).map_err(|_| {
        Error::InvalidPythonDownloadsNdjsonLine(
            source.to_owned(),
            serde_json::from_str::<()>("invalid utf8").unwrap_err(),
        )
    })?;
    serde_json::from_str(line_str)
        .map_err(|err| Error::InvalidPythonDownloadsNdjsonLine(source.to_owned(), err))
}

fn visit_ndjson_line<T>(
    source: &str,
    line: &[u8],
    visitor: &mut impl FnMut(ManagedPythonDownload) -> ControlFlow<T, ()>,
) -> Result<Option<T>, Error> {
    if line.is_empty() || line.iter().all(u8::is_ascii_whitespace) {
        return Ok(None);
    }

    let version_info = parse_ndjson_line(source, line)?;
    for download in parse_ndjson_version_info(version_info) {
        if let ControlFlow::Break(value) = visitor(download) {
            return Ok(Some(value));
        }
    }

    Ok(None)
}

fn parse_ndjson_bytes_with<T>(
    source: &str,
    buf: &[u8],
    mut visitor: impl FnMut(ManagedPythonDownload) -> ControlFlow<T, ()>,
) -> Result<Option<T>, Error> {
    for line in buf.split(|byte| *byte == b'\n') {
        if let Some(value) = visit_ndjson_line(source, line, &mut visitor)? {
            return Ok(Some(value));
        }
    }

    Ok(None)
}

fn parse_ndjson_bytes(source: &str, buf: &[u8]) -> Result<Vec<ManagedPythonDownload>, Error> {
    let mut downloads = Vec::new();
    parse_ndjson_bytes_with(source, buf, |download| {
        downloads.push(download);
        ControlFlow::<()>::Continue(())
    })?;
    downloads.sort_by(|a, b| Ord::cmp(&b.key, &a.key));
    Ok(downloads)
}

fn parse_ndjson_bytes_filtered(
    source: &str,
    buf: &[u8],
    predicate: impl Fn(&ManagedPythonDownload) -> bool,
    limit: Option<usize>,
) -> Result<Vec<ManagedPythonDownload>, Error> {
    let mut downloads = Vec::new();
    parse_ndjson_bytes_with(source, buf, |download| {
        if predicate(&download) {
            downloads.push(download);
            if limit.is_some_and(|limit| downloads.len() >= limit) {
                return ControlFlow::Break(());
            }
        }
        ControlFlow::Continue(())
    })?;
    downloads.sort_by(|a, b| Ord::cmp(&b.key, &a.key));
    Ok(downloads)
}

fn parse_ndjson_bytes_find(
    source: &str,
    buf: &[u8],
    predicate: impl Fn(&ManagedPythonDownload) -> bool,
) -> Result<Option<ManagedPythonDownload>, Error> {
    parse_ndjson_bytes_with(source, buf, |download| {
        if predicate(&download) {
            ControlFlow::Break(download)
        } else {
            ControlFlow::Continue(())
        }
    })
}

async fn fetch_ndjson_streaming<T>(
    client: &BaseClient,
    url: &DisplaySafeUrl,
    mut visitor: impl FnMut(ManagedPythonDownload) -> ControlFlow<T, ()>,
) -> Result<Option<T>, Error> {
    let source = url.to_string();
    let (reader, _) = read_url(url, client).await?;
    let mut reader = BufReader::new(reader);
    let mut line = Vec::new();

    loop {
        line.clear();
        if reader.read_until(b'\n', &mut line).await? == 0 {
            break;
        }

        if line.last() == Some(&b'\n') {
            line.pop();
        }
        if line.last() == Some(&b'\r') {
            line.pop();
        }

        if let Some(value) = visit_ndjson_line(&source, &line, &mut visitor)? {
            return Ok(Some(value));
        }
    }

    Ok(None)
}

async fn fetch_ndjson_find(
    client: &BaseClient,
    url: &DisplaySafeUrl,
    predicate: impl Fn(&ManagedPythonDownload) -> bool,
) -> Result<Option<ManagedPythonDownload>, Error> {
    fetch_ndjson_streaming(client, url, |download| {
        if predicate(&download) {
            ControlFlow::Break(download)
        } else {
            ControlFlow::Continue(())
        }
    })
    .await
}

async fn fetch_ndjson_collect(
    client: &BaseClient,
    url: &DisplaySafeUrl,
    predicate: impl Fn(&ManagedPythonDownload) -> bool,
    limit: Option<usize>,
) -> Result<Vec<ManagedPythonDownload>, Error> {
    let mut downloads = Vec::new();
    fetch_ndjson_streaming(client, url, |download| {
        if predicate(&download) {
            downloads.push(download);
            if limit.is_some_and(|limit| downloads.len() >= limit) {
                return ControlFlow::Break(());
            }
        }
        ControlFlow::Continue(())
    })
    .await?;
    downloads.sort_by(|a, b| Ord::cmp(&b.key, &a.key));
    Ok(downloads)
}

impl Error {
    pub(crate) fn from_reqwest(
        url: DisplaySafeUrl,
        err: reqwest::Error,
        retries: Option<u32>,
        start: Instant,
    ) -> Self {
        let err = Self::NetworkError(url, WrappedReqwestError::from(err));
        if let Some(retries) = retries {
            Self::NetworkErrorWithRetries {
                err: Box::new(err),
                retries,
                duration: start.elapsed(),
            }
        } else {
            err
        }
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
            Self::Download => "download",
            Self::Extract => "extract",
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
    url: &DisplaySafeUrl,
    client: &BaseClient,
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
        let start = Instant::now();
        let response = client
            .for_host(url)
            .get(Url::from(url.clone()))
            .send()
            .await
            .map_err(|err| Error::from_reqwest_middleware(url.clone(), err))?;

        let retry_count = response
            .extensions()
            .get::<reqwest_retry::RetryCount>()
            .map(|retries| retries.value());

        // Check the status code.
        let response = response
            .error_for_status()
            .map_err(|err| Error::from_reqwest(url.clone(), err, retry_count, start))?;

        let size = response.content_length();
        let stream = response
            .bytes_stream()
            .map_err(io::Error::other)
            .into_async_read();

        Ok((Either::Right(stream.compat()), size))
    }
}

#[cfg(test)]
mod tests {
    use crate::PythonVariant;
    use crate::implementation::LenientImplementationName;
    use crate::installation::PythonInstallationKey;
    use uv_platform::{Arch, Libc, Os, Platform};

    use super::*;

    /// Parse a request with all of its fields.
    #[test]
    fn test_python_download_request_from_str_complete() {
        let request = PythonDownloadRequest::from_str("cpython-3.12.0-linux-x86_64-gnu")
            .expect("Test request should be parsed");

        assert_eq!(request.implementation, Some(ImplementationName::CPython));
        assert_eq!(
            request.version,
            Some(VersionRequest::from_str("3.12.0").unwrap())
        );
        assert_eq!(
            request.os,
            Some(Os::new(target_lexicon::OperatingSystem::Linux))
        );
        assert_eq!(
            request.arch,
            Some(ArchRequest::Explicit(Arch::new(
                target_lexicon::Architecture::X86_64,
                None
            )))
        );
        assert_eq!(
            request.libc,
            Some(Libc::Some(target_lexicon::Environment::Gnu))
        );
    }

    /// Parse a request with `any` in various positions.
    #[test]
    fn test_python_download_request_from_str_with_any() {
        let request = PythonDownloadRequest::from_str("any-3.11-any-x86_64-any")
            .expect("Test request should be parsed");

        assert_eq!(request.implementation, None);
        assert_eq!(
            request.version,
            Some(VersionRequest::from_str("3.11").unwrap())
        );
        assert_eq!(request.os, None);
        assert_eq!(
            request.arch,
            Some(ArchRequest::Explicit(Arch::new(
                target_lexicon::Architecture::X86_64,
                None
            )))
        );
        assert_eq!(request.libc, None);
    }

    /// Parse a request with `any` implied by the omission of segments.
    #[test]
    fn test_python_download_request_from_str_missing_segment() {
        let request =
            PythonDownloadRequest::from_str("pypy-linux").expect("Test request should be parsed");

        assert_eq!(request.implementation, Some(ImplementationName::PyPy));
        assert_eq!(request.version, None);
        assert_eq!(
            request.os,
            Some(Os::new(target_lexicon::OperatingSystem::Linux))
        );
        assert_eq!(request.arch, None);
        assert_eq!(request.libc, None);
    }

    #[test]
    fn test_python_download_request_from_str_version_only() {
        let request =
            PythonDownloadRequest::from_str("3.10.5").expect("Test request should be parsed");

        assert_eq!(request.implementation, None);
        assert_eq!(
            request.version,
            Some(VersionRequest::from_str("3.10.5").unwrap())
        );
        assert_eq!(request.os, None);
        assert_eq!(request.arch, None);
        assert_eq!(request.libc, None);
    }

    #[test]
    fn test_python_download_request_from_str_implementation_only() {
        let request =
            PythonDownloadRequest::from_str("cpython").expect("Test request should be parsed");

        assert_eq!(request.implementation, Some(ImplementationName::CPython));
        assert_eq!(request.version, None);
        assert_eq!(request.os, None);
        assert_eq!(request.arch, None);
        assert_eq!(request.libc, None);
    }

    /// Parse a request with the OS and architecture specified.
    #[test]
    fn test_python_download_request_from_str_os_arch() {
        let request = PythonDownloadRequest::from_str("windows-x86_64")
            .expect("Test request should be parsed");

        assert_eq!(request.implementation, None);
        assert_eq!(request.version, None);
        assert_eq!(
            request.os,
            Some(Os::new(target_lexicon::OperatingSystem::Windows))
        );
        assert_eq!(
            request.arch,
            Some(ArchRequest::Explicit(Arch::new(
                target_lexicon::Architecture::X86_64,
                None
            )))
        );
        assert_eq!(request.libc, None);
    }

    /// Parse a request with a pre-release version.
    #[test]
    fn test_python_download_request_from_str_prerelease() {
        let request = PythonDownloadRequest::from_str("cpython-3.13.0rc1")
            .expect("Test request should be parsed");

        assert_eq!(request.implementation, Some(ImplementationName::CPython));
        assert_eq!(
            request.version,
            Some(VersionRequest::from_str("3.13.0rc1").unwrap())
        );
        assert_eq!(request.os, None);
        assert_eq!(request.arch, None);
        assert_eq!(request.libc, None);
    }

    /// We fail on extra parts in the request.
    #[test]
    fn test_python_download_request_from_str_too_many_parts() {
        let result = PythonDownloadRequest::from_str("cpython-3.12-linux-x86_64-gnu-extra");

        assert!(matches!(result, Err(Error::TooManyParts(_))));
    }

    /// We don't allow an empty request.
    #[test]
    fn test_python_download_request_from_str_empty() {
        let result = PythonDownloadRequest::from_str("");

        assert!(matches!(result, Err(Error::EmptyRequest)), "{result:?}");
    }

    /// Parse a request with all "any" segments.
    #[test]
    fn test_python_download_request_from_str_all_any() {
        let request = PythonDownloadRequest::from_str("any-any-any-any-any")
            .expect("Test request should be parsed");

        assert_eq!(request.implementation, None);
        assert_eq!(request.version, None);
        assert_eq!(request.os, None);
        assert_eq!(request.arch, None);
        assert_eq!(request.libc, None);
    }

    /// Test that "any" is case-insensitive in various positions.
    #[test]
    fn test_python_download_request_from_str_case_insensitive_any() {
        let request = PythonDownloadRequest::from_str("ANY-3.11-Any-x86_64-aNy")
            .expect("Test request should be parsed");

        assert_eq!(request.implementation, None);
        assert_eq!(
            request.version,
            Some(VersionRequest::from_str("3.11").unwrap())
        );
        assert_eq!(request.os, None);
        assert_eq!(
            request.arch,
            Some(ArchRequest::Explicit(Arch::new(
                target_lexicon::Architecture::X86_64,
                None
            )))
        );
        assert_eq!(request.libc, None);
    }

    /// Parse a request with an invalid leading segment.
    #[test]
    fn test_python_download_request_from_str_invalid_leading_segment() {
        let result = PythonDownloadRequest::from_str("foobar-3.14-windows");

        assert!(
            matches!(result, Err(Error::ImplementationError(_))),
            "{result:?}"
        );
    }

    /// Parse a request with segments in an invalid order.
    #[test]
    fn test_python_download_request_from_str_out_of_order() {
        let result = PythonDownloadRequest::from_str("3.12-cpython");

        assert!(
            matches!(result, Err(Error::InvalidRequestPlatform(_))),
            "{result:?}"
        );
    }

    /// Parse a request with too many "any" segments.
    #[test]
    fn test_python_download_request_from_str_too_many_any() {
        let result = PythonDownloadRequest::from_str("any-any-any-any-any-any");

        assert!(matches!(result, Err(Error::TooManyParts(_))));
    }

    /// Test that build filtering works correctly
    #[tokio::test]
    async fn test_python_download_request_build_filtering() {
        let request = PythonDownloadRequest::default()
            .with_version(VersionRequest::from_str("3.12").unwrap())
            .with_implementation(ImplementationName::CPython)
            .with_build("20240814".to_string());

        let download_list = ManagedPythonDownloadList::new_only_embedded().unwrap();

        let downloads: Vec<_> = download_list
            .iter_all()
            .filter(|d| request.satisfied_by_download(d))
            .collect();

        assert!(
            !downloads.is_empty(),
            "Should find at least one matching download"
        );
        for download in downloads {
            assert_eq!(download.build(), Some("20240814"));
        }
    }

    /// Test that an invalid build results in no matches
    #[tokio::test]
    async fn test_python_download_request_invalid_build() {
        // Create a request with a non-existent build
        let request = PythonDownloadRequest::default()
            .with_version(VersionRequest::from_str("3.12").unwrap())
            .with_implementation(ImplementationName::CPython)
            .with_build("99999999".to_string());

        let download_list = ManagedPythonDownloadList::new_only_embedded().unwrap();

        // Should find no matching downloads
        let downloads: Vec<_> = download_list
            .iter_all()
            .filter(|d| request.satisfied_by_download(d))
            .collect();

        assert_eq!(downloads.len(), 0);
    }

    #[test]
    fn parse_ndjson_bytes_filtered_applies_limit() {
        let ndjson = br#"{"version":"3.14.1+20260420","artifacts":[{"url":"https://example.com/cpython-3.14.1-aarch64-apple-darwin.tar.gz","platform":"aarch64-apple-darwin","sha256":"abc123","variant":"install_only"}]}
{"version":"3.13.2","artifacts":[{"url":"https://example.com/cpython-3.13.2-aarch64-apple-darwin.tar.gz","platform":"aarch64-apple-darwin","sha256":"def456","variant":"install_only"}]}
"#;

        let downloads = parse_ndjson_bytes_filtered("test.ndjson", ndjson, |_| true, Some(1))
            .expect("NDJSON should parse");

        assert_eq!(downloads.len(), 1);
        assert_eq!(downloads[0].key().version().to_string(), "3.14.1");
        assert_eq!(downloads[0].build(), Some("20260420"));
    }

    #[test]
    fn parse_ndjson_bytes_find_returns_first_match() {
        let ndjson = br#"{"version":"3.14.1","artifacts":[{"url":"https://example.com/cpython-3.14.1-aarch64-apple-darwin.tar.gz","platform":"aarch64-apple-darwin","sha256":"abc123","variant":"install_only"}]}
{"version":"3.13.2","artifacts":[{"url":"https://example.com/cpython-3.13.2-aarch64-apple-darwin.tar.gz","platform":"aarch64-apple-darwin","sha256":"def456","variant":"install_only"}]}
"#;

        let download = parse_ndjson_bytes_find("test.ndjson", ndjson, |download| {
            download.key().version().to_string() == "3.13.2"
        })
        .expect("NDJSON should parse")
        .expect("matching download should be found");

        assert_eq!(download.key().version().to_string(), "3.13.2");
        assert_eq!(
            download.url().as_ref(),
            "https://example.com/cpython-3.13.2-aarch64-apple-darwin.tar.gz"
        );
    }

    #[test]
    fn parse_ndjson_bytes_matches_generator_artifact_selection() {
        let ndjson = br#"{"version":"3.14.1+20260420","artifacts":[{"url":"https://example.com/cpython-3.14.1-aarch64-apple-darwin-install_only.tar.gz","platform":"aarch64-apple-darwin","sha256":"abc123","variant":"install_only"},{"url":"https://example.com/cpython-3.14.1-aarch64-apple-darwin-install_only_stripped.tar.gz","platform":"aarch64-apple-darwin","sha256":"def456","variant":"install_only_stripped"}]}
{"version":"3.10.0+20211017","artifacts":[{"url":"https://example.com/cpython-3.10.0-x86_64-unknown-linux-gnu-pgo-lto-full.tar.zst","platform":"x86_64-unknown-linux-gnu","sha256":"ghi789","variant":"pgo+lto+full"}]}
"#;

        let downloads = parse_ndjson_bytes("test.ndjson", ndjson).expect("NDJSON should parse");
        let downloads = downloads
            .iter()
            .map(|download| (download.key().to_string(), download.url().as_ref()))
            .collect::<Vec<_>>();

        assert_eq!(
            downloads,
            vec![
                (
                    "cpython-3.14.1-macos-aarch64-none".to_string(),
                    "https://example.com/cpython-3.14.1-aarch64-apple-darwin-install_only_stripped.tar.gz",
                ),
                (
                    "cpython-3.10.0-linux-x86_64-gnu".to_string(),
                    "https://example.com/cpython-3.10.0-x86_64-unknown-linux-gnu-pgo-lto-full.tar.zst",
                ),
            ]
        );
    }

    #[test]
    fn versions_cache_shard_key_hashes_unredacted_url() {
        let url_a = DisplaySafeUrl::parse("https://user:tokenA@example.com/versions.ndjson")
            .expect("URL should parse");
        let url_b = DisplaySafeUrl::parse("https://user:tokenB@example.com/versions.ndjson")
            .expect("URL should parse");

        assert_eq!(url_a.to_string(), url_b.to_string());
        assert_ne!(
            versions_cache_shard_key(&url_a),
            versions_cache_shard_key(&url_b)
        );
    }

    #[test]
    fn upgrade_request_native_defaults() {
        let request = PythonDownloadRequest::default()
            .with_implementation(ImplementationName::CPython)
            .with_version(VersionRequest::MajorMinorPatch(
                3,
                13,
                1,
                PythonVariant::Default,
            ))
            .with_os(Os::from_str("linux").unwrap())
            .with_arch(Arch::from_str("x86_64").unwrap())
            .with_libc(Libc::from_str("gnu").unwrap())
            .with_prereleases(false);

        let host = Platform::new(
            Os::from_str("linux").unwrap(),
            Arch::from_str("x86_64").unwrap(),
            Libc::from_str("gnu").unwrap(),
        );

        assert_eq!(
            request
                .clone()
                .unset_defaults_for_host(&host)
                .without_patch()
                .simplified_display()
                .as_deref(),
            Some("3.13")
        );
    }

    #[test]
    fn upgrade_request_preserves_variant() {
        let request = PythonDownloadRequest::default()
            .with_implementation(ImplementationName::CPython)
            .with_version(VersionRequest::MajorMinorPatch(
                3,
                13,
                0,
                PythonVariant::Freethreaded,
            ))
            .with_os(Os::from_str("linux").unwrap())
            .with_arch(Arch::from_str("x86_64").unwrap())
            .with_libc(Libc::from_str("gnu").unwrap())
            .with_prereleases(false);

        let host = Platform::new(
            Os::from_str("linux").unwrap(),
            Arch::from_str("x86_64").unwrap(),
            Libc::from_str("gnu").unwrap(),
        );

        assert_eq!(
            request
                .clone()
                .unset_defaults_for_host(&host)
                .without_patch()
                .simplified_display()
                .as_deref(),
            Some("3.13+freethreaded")
        );
    }

    #[test]
    fn upgrade_request_preserves_non_default_platform() {
        let request = PythonDownloadRequest::default()
            .with_implementation(ImplementationName::CPython)
            .with_version(VersionRequest::MajorMinorPatch(
                3,
                12,
                4,
                PythonVariant::Default,
            ))
            .with_os(Os::from_str("linux").unwrap())
            .with_arch(Arch::from_str("aarch64").unwrap())
            .with_libc(Libc::from_str("gnu").unwrap())
            .with_prereleases(false);

        let host = Platform::new(
            Os::from_str("linux").unwrap(),
            Arch::from_str("x86_64").unwrap(),
            Libc::from_str("gnu").unwrap(),
        );

        assert_eq!(
            request
                .clone()
                .unset_defaults_for_host(&host)
                .without_patch()
                .simplified_display()
                .as_deref(),
            Some("3.12-aarch64")
        );
    }

    #[test]
    fn upgrade_request_preserves_custom_implementation() {
        let request = PythonDownloadRequest::default()
            .with_implementation(ImplementationName::PyPy)
            .with_version(VersionRequest::MajorMinorPatch(
                3,
                10,
                5,
                PythonVariant::Default,
            ))
            .with_os(Os::from_str("linux").unwrap())
            .with_arch(Arch::from_str("x86_64").unwrap())
            .with_libc(Libc::from_str("gnu").unwrap())
            .with_prereleases(false);

        let host = Platform::new(
            Os::from_str("linux").unwrap(),
            Arch::from_str("x86_64").unwrap(),
            Libc::from_str("gnu").unwrap(),
        );

        assert_eq!(
            request
                .clone()
                .unset_defaults_for_host(&host)
                .without_patch()
                .simplified_display()
                .as_deref(),
            Some("pypy-3.10")
        );
    }

    #[test]
    fn simplified_display_returns_none_when_empty() {
        let request = PythonDownloadRequest::default()
            .fill_platform()
            .expect("should populate defaults");

        let host = Platform::from_env().expect("host platform");

        assert_eq!(
            request.unset_defaults_for_host(&host).simplified_display(),
            None
        );
    }

    #[test]
    fn simplified_display_omits_environment_arch() {
        let mut request = PythonDownloadRequest::default()
            .with_version(VersionRequest::MajorMinor(3, 12, PythonVariant::Default))
            .with_os(Os::from_str("linux").unwrap())
            .with_libc(Libc::from_str("gnu").unwrap());

        request.arch = Some(ArchRequest::Environment(Arch::from_str("x86_64").unwrap()));

        let host = Platform::new(
            Os::from_str("linux").unwrap(),
            Arch::from_str("aarch64").unwrap(),
            Libc::from_str("gnu").unwrap(),
        );

        assert_eq!(
            request
                .unset_defaults_for_host(&host)
                .simplified_display()
                .as_deref(),
            Some("3.12")
        );
    }

    /// Test build display
    #[test]
    fn test_managed_python_download_build_display() {
        // Create a test download with a build
        let key = PythonInstallationKey::new(
            LenientImplementationName::Known(crate::implementation::ImplementationName::CPython),
            3,
            12,
            0,
            None,
            Platform::new(
                Os::from_str("linux").unwrap(),
                Arch::from_str("x86_64").unwrap(),
                Libc::from_str("gnu").unwrap(),
            ),
            crate::PythonVariant::default(),
        );

        let download_with_build = ManagedPythonDownload {
            key,
            url: Cow::Borrowed("https://example.com/python.tar.gz"),
            sha256: Some(Cow::Borrowed("abc123")),
            build: Some("20240101"),
        };

        // Test display with build
        assert_eq!(
            download_with_build.to_display_with_build().to_string(),
            "cpython-3.12.0-linux-x86_64-gnu+20240101"
        );

        // Test download without build
        let download_without_build = ManagedPythonDownload {
            key: download_with_build.key.clone(),
            url: Cow::Borrowed("https://example.com/python.tar.gz"),
            sha256: Some(Cow::Borrowed("abc123")),
            build: None,
        };

        // Test display without build
        assert_eq!(
            download_without_build.to_display_with_build().to_string(),
            "cpython-3.12.0-linux-x86_64-gnu"
        );
    }

    /// A hash mismatch is a post-download integrity failure — retrying a different URL cannot fix
    /// it, so it should not trigger a fallback.
    #[test]
    fn test_should_try_next_url_hash_mismatch() {
        let err = Error::HashMismatch {
            installation: "cpython-3.12.0".to_string(),
            expected: "abc".to_string(),
            actual: "def".to_string(),
        };
        assert!(!err.should_try_next_url());
    }

    /// A local filesystem error during extraction (e.g. permission denied writing to disk) is not
    /// a network failure — a different URL would produce the same outcome.
    #[test]
    fn test_should_try_next_url_extract_error_filesystem() {
        let err = Error::ExtractError(
            "archive.tar.gz".to_string(),
            uv_extract::Error::Io(io::Error::new(io::ErrorKind::PermissionDenied, "")),
        );
        assert!(!err.should_try_next_url());
    }

    /// A generic IO error from a local filesystem operation (e.g. permission denied on cache
    /// directory) should not trigger a fallback to a different URL.
    #[test]
    fn test_should_try_next_url_io_error_filesystem() {
        let err = Error::Io(io::Error::new(io::ErrorKind::PermissionDenied, ""));
        assert!(!err.should_try_next_url());
    }

    /// A network IO error (e.g. connection reset mid-download) surfaces as `Error::Io` from
    /// `download_archive`. It should trigger a fallback because a different mirror may succeed.
    #[test]
    fn test_should_try_next_url_io_error_network() {
        let err = Error::Io(io::Error::new(io::ErrorKind::ConnectionReset, ""));
        assert!(err.should_try_next_url());
    }

    /// A 404 HTTP response from the mirror becomes `Error::NetworkError` — it should trigger a
    /// URL fallback, because a 404 on the mirror does not mean the file is absent from GitHub.
    #[test]
    fn test_should_try_next_url_network_error_404() {
        let url =
            DisplaySafeUrl::from_str("https://releases.astral.sh/python/cpython-3.12.0.tar.gz")
                .unwrap();
        // `NetworkError` wraps a `WrappedReqwestError`; we use a middleware error as a
        // stand-in because `should_try_next_url` only inspects the variant, not the contents.
        let wrapped = WrappedReqwestError::with_problem_details(
            reqwest_middleware::Error::Middleware(anyhow::anyhow!("404 Not Found")),
            None,
        );
        let err = Error::NetworkError(url, wrapped);
        assert!(err.should_try_next_url());
    }
}
