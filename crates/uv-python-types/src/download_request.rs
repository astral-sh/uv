use std::fmt::Display;
use std::str::FromStr;

use uv_platform::{self as platform, Arch, Libc, Os, Platform};

use crate::BuildVersionError;
use crate::implementation::{
    Error as ImplementationError, ImplementationName, LenientImplementationName,
};
use crate::python_build_version_from_env;
use crate::{PythonInstallationKey, PythonRequest, VersionRequest};

/// A failure while parsing or completing a managed Python download request.
#[derive(Debug, thiserror::Error)]
pub enum PythonDownloadRequestError {
    #[error(transparent)]
    ImplementationError(#[from] ImplementationError),
    #[error("Invalid Python version: {0}")]
    InvalidPythonVersion(String),
    #[error("Invalid request key (empty request)")]
    EmptyRequest,
    #[error("Invalid request key (too many parts): {0}")]
    TooManyParts(String),
    #[error("Failed to parse request part")]
    InvalidRequestPlatform(#[from] platform::Error),
    #[error("Failed to determine the libc used on the current platform")]
    LibcDetection(#[from] platform::LibcDetectionError),
    #[error(transparent)]
    BuildVersion(#[from] BuildVersionError),
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Hash)]
pub struct PythonDownloadRequest {
    pub version: Option<VersionRequest>,
    pub implementation: Option<ImplementationName>,
    pub arch: Option<ArchRequest>,
    pub os: Option<Os>,
    pub libc: Option<Libc>,
    pub build: Option<String>,

    /// Whether to allow pre-releases or not. If not set, defaults to true if [`Self::version`] is
    /// not None, and false otherwise.
    pub prereleases: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArchRequest {
    /// Require an exact architecture.
    Explicit(Arch),
    /// Allow architectures supported by the detected host architecture.
    Environment(Arch),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PlatformRequest {
    os: Option<Os>,
    arch: Option<ArchRequest>,
    libc: Option<Libc>,
}

impl PlatformRequest {
    /// Require an exact match for the given architecture if this request does not specify one.
    #[must_use]
    pub fn with_default_arch(mut self, arch: Option<Arch>) -> Self {
        if self.arch.is_none() {
            self.arch = arch.map(ArchRequest::Explicit);
        }
        self
    }

    /// Check if this platform request is satisfied by a platform.
    pub fn matches(&self, platform: &Platform) -> bool {
        if let Some(os) = self.os
            && !platform.os.supports(os)
        {
            return false;
        }

        if let Some(arch) = self.arch
            && !arch.satisfied_by(platform)
        {
            return false;
        }

        if let Some(libc) = self.libc
            && platform.libc != libc
        {
            return false;
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
    fn satisfied_by(self, platform: &Platform) -> bool {
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

    /// Require an exact match for the given architecture if this request does not specify one.
    #[must_use]
    pub fn with_default_arch(mut self, arch: Option<Arch>) -> Self {
        if self.arch.is_none() {
            self.arch = arch.map(ArchRequest::Explicit);
        }
        self
    }

    #[must_use]
    pub fn with_any_arch(mut self) -> Self {
        self.arch = None;
        self
    }

    #[must_use]
    fn with_os(mut self, os: Os) -> Self {
        self.os = Some(os);
        self
    }

    #[must_use]
    fn with_libc(mut self, libc: Libc) -> Self {
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
    pub fn fill_platform(mut self) -> Result<Self, PythonDownloadRequestError> {
        let platform = Platform::from_env().map_err(|err| match err {
            platform::Error::LibcDetectionError(err) => {
                PythonDownloadRequestError::LibcDetection(err)
            }
            err => PythonDownloadRequestError::InvalidRequestPlatform(err),
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
    fn fill_build_from_env(mut self) -> Result<Self, PythonDownloadRequestError> {
        if self.build.is_some() {
            return Ok(self);
        }
        let Some(implementation) = self.implementation else {
            return Ok(self);
        };

        self.build = python_build_version_from_env(implementation)?;
        Ok(self)
    }

    pub fn fill(mut self) -> Result<Self, PythonDownloadRequestError> {
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
    fn unset_defaults_for_host(self, host: &Platform) -> Self {
        self.unset_non_platform_defaults()
            .unset_platform_defaults(host)
    }

    fn unset_platform_defaults(mut self, host: &Platform) -> Self {
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

        if let Some(implementation) = &self.implementation
            && key.implementation != LenientImplementationName::from(*implementation)
        {
            return false;
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
            if let Some(variant) = version.variant()
                && variant != key.variant
            {
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
    pub(crate) fn allows_debug(&self) -> bool {
        self.version.as_ref().is_some_and(VersionRequest::is_debug)
    }

    /// Whether this download request opts-in to alternative Python implementations.
    pub(crate) fn allows_alternative_implementations(&self) -> bool {
        self.implementation
            .is_some_and(|implementation| !matches!(implementation, ImplementationName::CPython))
            || self.os.is_some_and(|os| os.is_emscripten())
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
    type Err = PythonDownloadRequestError;

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
            fn next(&self) -> Self {
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
            error: Option<PythonDownloadRequestError>,
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

            fn record_err(&mut self, err: PythonDownloadRequestError) {
                // For now, we only record the first error encountered. We could record all of the
                // errors for a given part, then pick the most appropriate one later.
                self.error.get_or_insert(err);
            }
        }

        if s.is_empty() {
            return Err(PythonDownloadRequestError::EmptyRequest);
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
                    match VersionRequest::from_str(part).map_err(|_| {
                        PythonDownloadRequestError::InvalidPythonVersion(part.to_string())
                    }) {
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
                        return Err(PythonDownloadRequestError::TooManyParts(s.to_string()));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PythonVariant;
    use std::assert_matches;
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

        assert_matches!(result, Err(PythonDownloadRequestError::TooManyParts(_)));
    }

    /// We don't allow an empty request.
    #[test]
    fn test_python_download_request_from_str_empty() {
        let result = PythonDownloadRequest::from_str("");

        assert_matches!(result, Err(PythonDownloadRequestError::EmptyRequest));
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

        assert_matches!(
            result,
            Err(PythonDownloadRequestError::ImplementationError(_))
        );
    }

    /// Parse a request with segments in an invalid order.
    #[test]
    fn test_python_download_request_from_str_out_of_order() {
        let result = PythonDownloadRequest::from_str("3.12-cpython");

        assert_matches!(
            result,
            Err(PythonDownloadRequestError::InvalidRequestPlatform(_))
        );
    }

    /// Parse a request with too many "any" segments.
    #[test]
    fn test_python_download_request_from_str_too_many_any() {
        let result = PythonDownloadRequest::from_str("any-any-any-any-any-any");

        assert_matches!(result, Err(PythonDownloadRequestError::TooManyParts(_)));
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
}
