use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::Display;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::str::FromStr;
use std::task::{Context, Poll};
use std::{env, io};

use futures::TryStreamExt;
use itertools::Itertools;
use owo_colors::OwoColorize;
use reqwest_retry::RetryError;
use reqwest_retry::policies::ExponentialBackoff;
use serde::Deserialize;
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt, BufWriter, ReadBuf};
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tokio_util::either::Either;
use tracing::{debug, instrument};
use url::Url;

use uv_client::{BaseClient, RetryState, WrappedReqwestError};
use uv_distribution_filename::{ExtensionError, SourceDistExtension};
use uv_extract::hash::Hasher;
use uv_fs::{Simplified, rename_with_retry};
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
    #[error("Request failed after {retries} {subject}", subject = if *retries > 1 { "retries" } else { "retry" })]
    NetworkErrorWithRetries {
        #[source]
        err: Box<Self>,
        retries: u32,
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
    #[error("An offline Python installation was requested, but {file} (from {url}) is missing in {}", python_builds_dir.user_display())]
    OfflinePythonMissing {
        file: Box<PythonInstallationKey>,
        url: Box<DisplaySafeUrl>,
        python_builds_dir: PathBuf,
    },
    #[error(transparent)]
    BuildVersion(#[from] BuildVersionError),
}

impl Error {
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
}

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
        let platform = Platform::from_env()?;
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

#[derive(Debug, Clone)]
pub enum DownloadResult {
    AlreadyAvailable(PathBuf),
    Fetched(PathBuf),
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

    /// Load available Python distributions from a provided source or the compiled-in list.
    ///
    /// `python_downloads_json_url` can be either `None`, to use the default list (taken from
    /// `crates/uv-python/download-metadata.json`), or `Some` local path
    /// or file://, http://, or https:// URL.
    ///
    /// Returns an error if the provided list could not be opened, if the JSON is invalid, or if it
    /// does not parse into the expected data structure.
    pub async fn new(
        client: &BaseClient,
        python_downloads_json_url: Option<&str>,
    ) -> Result<Self, Error> {
        // Although read_url() handles file:// URLs and converts them to local file reads, here we
        // want to also support parsing bare filenames like "/tmp/py.json", not just
        // "file:///tmp/py.json". Note that "C:\Temp\py.json" should be considered a filename, even
        // though Url::parse would successfully misparse it as a URL with scheme "C".
        enum Source<'a> {
            BuiltIn,
            Path(Cow<'a, Path>),
            Http(DisplaySafeUrl),
        }

        let json_source = if let Some(url_or_path) = python_downloads_json_url {
            if let Ok(url) = DisplaySafeUrl::parse(url_or_path) {
                match url.scheme() {
                    "http" | "https" => Source::Http(url),
                    "file" => Source::Path(Cow::Owned(
                        url.to_file_path().or(Err(Error::InvalidUrlFormat(url)))?,
                    )),
                    _ => Source::Path(Cow::Borrowed(Path::new(url_or_path))),
                }
            } else {
                Source::Path(Cow::Borrowed(Path::new(url_or_path)))
            }
        } else {
            Source::BuiltIn
        };

        let buf: Cow<'_, [u8]> = match json_source {
            Source::BuiltIn => BUILTIN_PYTHON_DOWNLOADS_JSON.into(),
            Source::Path(ref path) => fs_err::read(path.as_ref())?.into(),
            Source::Http(ref url) => fetch_bytes_from_url(client, url)
                .await
                .map_err(|e| Error::FetchingPythonDownloadsJSONError(url.to_string(), Box::new(e)))?
                .into(),
        };
        let json_downloads: HashMap<String, JsonPythonDownload> = serde_json::from_slice(&buf)
            .map_err(
                // As an explicit compatibility mechanism, if there's a top-level "version" key, it
                // means it's a newer format than we know how to deal with.  Before reporting a
                // parse error about the format of JsonPythonDownload, check for that key. We can do
                // this by parsing into a Map<String, IgnoredAny> which allows any valid JSON on the
                // value side. (Because it's zero-sized, Clippy suggests Set<String>, but that won't
                // have the same parsing effect.)
                #[expect(clippy::zero_sized_map_values)]
                |e| {
                    let source = match json_source {
                        Source::BuiltIn => "EMBEDDED IN THE BINARY".to_owned(),
                        Source::Path(path) => path.to_string_lossy().to_string(),
                        Source::Http(url) => url.to_string(),
                    };
                    if let Ok(keys) =
                        serde_json::from_slice::<HashMap<String, serde::de::IgnoredAny>>(&buf)
                        && keys.contains_key("version")
                    {
                        Error::UnsupportedPythonDownloadsJSON(source)
                    } else {
                        Error::InvalidPythonDownloadsJSON(source, e)
                    }
                },
            )?;

        let result = parse_json_downloads(json_downloads);
        Ok(Self { downloads: result })
    }

    /// Load available Python distributions from the compiled-in list only.
    /// for testing purposes.
    pub fn new_only_embedded() -> Result<Self, Error> {
        let json_downloads: HashMap<String, JsonPythonDownload> =
            serde_json::from_slice(BUILTIN_PYTHON_DOWNLOADS_JSON).map_err(|e| {
                Error::InvalidPythonDownloadsJSON("EMBEDDED IN THE BINARY".to_owned(), e)
            })?;
        let result = parse_json_downloads(json_downloads);
        Ok(Self { downloads: result })
    }
}

async fn fetch_bytes_from_url(client: &BaseClient, url: &DisplaySafeUrl) -> Result<Vec<u8>, Error> {
    let (mut reader, size) = read_url(url, client).await?;
    let capacity = size.and_then(|s| s.try_into().ok()).unwrap_or(1_048_576);
    let mut buf = Vec::with_capacity(capacity);
    reader.read_to_end(&mut buf).await?;
    Ok(buf)
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
        let mut retry_state = RetryState::start(
            *retry_policy,
            self.download_url(python_install_mirror, pypy_install_mirror)?,
        );

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
            match result {
                Ok(download_result) => return Ok(download_result),
                Err(err) => {
                    if let Some(backoff) = retry_state.should_retry(&err, err.retries()) {
                        retry_state.sleep_backoff(backoff).await;
                        continue;
                    }
                    return if retry_state.total_retries() > 0 {
                        Err(Error::NetworkErrorWithRetries {
                            err: Box::new(err),
                            retries: retry_state.total_retries(),
                        })
                    } else {
                        Err(err)
                    };
                }
            };
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

    /// Return the [`Url`] to use when downloading the distribution. If a mirror is set via the
    /// appropriate environment variable, use it instead.
    pub fn download_url(
        &self,
        python_install_mirror: Option<&str>,
        pypy_install_mirror: Option<&str>,
    ) -> Result<DisplaySafeUrl, Error> {
        match self.key.implementation {
            LenientImplementationName::Known(ImplementationName::CPython) => {
                if let Some(mirror) = python_install_mirror {
                    let Some(suffix) = self.url.strip_prefix(
                        "https://github.com/astral-sh/python-build-standalone/releases/download/",
                    ) else {
                        return Err(Error::Mirror(
                            EnvVars::UV_PYTHON_INSTALL_MIRROR,
                            self.url.to_string(),
                        ));
                    };
                    return Ok(DisplaySafeUrl::parse(
                        format!("{}/{}", mirror.trim_end_matches('/'), suffix).as_str(),
                    )?);
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
                    return Ok(DisplaySafeUrl::parse(
                        format!("{}/{}", mirror.trim_end_matches('/'), suffix).as_str(),
                    )?);
                }
            }

            _ => {}
        }

        Ok(DisplaySafeUrl::parse(&self.url)?)
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

impl Error {
    pub(crate) fn from_reqwest(
        url: DisplaySafeUrl,
        err: reqwest::Error,
        retries: Option<u32>,
    ) -> Self {
        let err = Self::NetworkError(url, WrappedReqwestError::from(err));
        if let Some(retries) = retries {
            Self::NetworkErrorWithRetries {
                err: Box::new(err),
                retries,
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
            .map_err(|err| Error::from_reqwest(url.clone(), err, retry_count))?;

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

        let client = uv_client::BaseClientBuilder::default().build();
        let download_list = ManagedPythonDownloadList::new(&client, None).await.unwrap();

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

        let client = uv_client::BaseClientBuilder::default().build();
        let download_list = ManagedPythonDownloadList::new(&client, None).await.unwrap();

        // Should find no matching downloads
        let downloads: Vec<_> = download_list
            .iter_all()
            .filter(|d| request.satisfied_by_download(d))
            .collect();

        assert_eq!(downloads.len(), 0);
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
}
