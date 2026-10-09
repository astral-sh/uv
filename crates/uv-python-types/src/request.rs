use std::borrow::Cow;
use std::env::consts::EXE_SUFFIX;
use std::fmt::{self, Debug, Formatter};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use uv_fs::Simplified;
use uv_pep440::{
    LowerBound, Prerelease, UpperBound, Version, VersionSpecifier, VersionSpecifiers,
    release_specifiers_to_ranges,
};
use uv_platform::Arch;

use crate::{ImplementationName, PythonDownloadRequest, PythonInstallationKey, PythonVersion};

/// A request to find a Python installation.
///
/// See [`PythonRequest::parse`].
#[derive(Debug, Clone, Eq, Default)]
pub enum PythonRequest {
    /// An appropriate default Python installation
    ///
    /// This may skip some Python installations, such as pre-release versions or alternative
    /// implementations.
    #[default]
    Default,
    /// Any Python installation
    Any,
    /// A Python version without an implementation name e.g. `3.10` or `>=3.12,<3.13`
    Version(VersionRequest),
    /// A path to a directory containing a Python installation, e.g. `.venv`
    Directory(PathBuf),
    /// A path to a Python executable e.g. `~/bin/python`
    File(PathBuf),
    /// The name of a Python executable (i.e. for lookup in the PATH) e.g. `foopython3`
    ExecutableName(String),
    /// A Python implementation without a version e.g. `pypy` or `pp`
    Implementation(ImplementationName),
    /// A Python implementation name and version e.g. `pypy3.8` or `pypy@3.8` or `pp38`
    ImplementationVersion(ImplementationName, VersionRequest),
    /// A request for a specific Python installation key e.g. `cpython-3.12-x86_64-linux-gnu`
    /// Generally these refer to managed Python downloads.
    Key(PythonDownloadRequest),
}

impl PartialEq for PythonRequest {
    fn eq(&self, other: &Self) -> bool {
        self.to_canonical_string() == other.to_canonical_string()
    }
}

impl std::hash::Hash for PythonRequest {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.to_canonical_string().hash(state);
    }
}

impl<'a> serde::Deserialize<'a> for PythonRequest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'a>,
    {
        let s = <Cow<'_, str>>::deserialize(deserializer)?;
        Ok(Self::parse(&s))
    }
}

impl serde::Serialize for PythonRequest {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let s = self.to_canonical_string();
        serializer.serialize_str(&s)
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum PythonPreference {
    /// Only use managed Python installations; never use system Python installations.
    OnlyManaged,
    #[default]
    /// Prefer managed Python installations over system Python installations.
    ///
    /// System Python installations are still preferred over downloading managed Python versions.
    /// Use `only-managed` to always fetch a managed Python version.
    Managed,
    /// Prefer system Python installations over managed Python installations.
    ///
    /// If a system Python installation cannot be found, a managed Python installation can be used.
    System,
    /// Only use system Python installations; never use managed Python installations.
    OnlySystem,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum PythonDownloads {
    /// Automatically download managed Python installations when needed.
    #[default]
    #[serde(alias = "auto")]
    Automatic,
    /// Do not automatically download managed Python installations; require explicit installation.
    Manual,
    /// Do not ever allow Python downloads.
    Never,
}

impl FromStr for PythonDownloads {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "auto" | "automatic" | "true" | "1" => Ok(Self::Automatic),
            "manual" => Ok(Self::Manual),
            "never" | "false" | "0" => Ok(Self::Never),
            _ => Err(format!("Invalid value for `python-download`: '{s}'")),
        }
    }
}

impl From<bool> for PythonDownloads {
    fn from(value: bool) -> Self {
        if value { Self::Automatic } else { Self::Never }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EnvironmentPreference {
    /// Only use virtual environments, never allow a system environment.
    #[default]
    OnlyVirtual,
    /// Prefer virtual environments and allow a system environment if explicitly requested.
    ExplicitSystem,
    /// Only use a system environment, ignore virtual environments.
    OnlySystem,
    /// Allow any environment.
    Any,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PythonVariant {
    #[default]
    Default,
    Debug,
    Freethreaded,
    FreethreadedDebug,
    Gil,
    GilDebug,
}

/// A Python discovery version request.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum VersionRequest {
    /// Allow an appropriate default Python version.
    #[default]
    Default,
    /// Allow any Python version.
    Any,
    Major(u8, PythonVariant),
    MajorMinor(u8, u8, PythonVariant),
    MajorMinorPatch(u8, u8, u8, PythonVariant),
    MajorMinorPrerelease(u8, u8, Prerelease, PythonVariant),
    MajorMinorPatchPrerelease(u8, u8, u8, Prerelease, PythonVariant),
    Range(VersionSpecifiers, PythonVariant),
}

/// A location for discovery of a Python installation or interpreter.
#[derive(Debug, Clone, PartialEq, Eq, Copy, Hash, PartialOrd, Ord)]
pub enum PythonSource {
    /// The path was provided directly
    ProvidedPath,
    /// An environment was active e.g. via `VIRTUAL_ENV`
    ActiveEnvironment,
    /// A conda environment was active e.g. via `CONDA_PREFIX`
    CondaPrefix,
    /// A base conda environment was active e.g. via `CONDA_PREFIX`
    BaseCondaPrefix,
    /// An environment was discovered e.g. via `.venv`
    DiscoveredEnvironment,
    /// An executable was found in the search path i.e. `PATH`
    SearchPath,
    /// The first executable found in the search path i.e. `PATH`
    SearchPathFirst,
    /// An executable was found in the Windows registry via PEP 514
    Registry,
    /// An executable was found in the known Microsoft Store locations
    MicrosoftStore,
    /// The Python installation was found in the uv managed Python directory
    Managed,
    /// The Python installation was found via the invoking interpreter i.e. via `python -m uv ...`
    ParentInterpreter,
}

/// A failure while parsing a Python version request.
#[derive(Debug, thiserror::Error)]
pub enum PythonRequestError {
    #[error("Invalid version request: {0}")]
    InvalidVersionRequest(String),
    #[error("Requesting the 'latest' Python version is not yet supported")]
    LatestVersionRequest,
}

impl PythonVariant {
    /// Return the executable suffix for the variant, e.g., `t` for `python3.13t`.
    ///
    /// Returns an empty string for the default Python variant.
    pub fn executable_suffix(self) -> &'static str {
        match self {
            Self::Default => "",
            Self::Debug => "d",
            Self::Freethreaded => "t",
            Self::FreethreadedDebug => "td",
            Self::Gil => "",
            Self::GilDebug => "d",
        }
    }

    /// Return the suffix for display purposes, e.g., `+gil`.
    pub fn display_suffix(self) -> &'static str {
        match self {
            Self::Default => "",
            Self::Debug => "+debug",
            Self::Freethreaded => "+freethreaded",
            Self::FreethreadedDebug => "+freethreaded+debug",
            Self::Gil => "+gil",
            Self::GilDebug => "+gil+debug",
        }
    }

    /// Return the lib suffix for the variant, e.g., `t` for `python3.13t` but an empty string for
    /// `python3.13d` or `python3.13`.
    pub fn lib_suffix(self) -> &'static str {
        match self {
            Self::Default | Self::Debug | Self::Gil | Self::GilDebug => "",
            Self::Freethreaded | Self::FreethreadedDebug => "t",
        }
    }

    fn is_freethreaded(self) -> bool {
        match self {
            Self::Default | Self::Debug | Self::Gil | Self::GilDebug => false,
            Self::Freethreaded | Self::FreethreadedDebug => true,
        }
    }

    pub fn is_debug(self) -> bool {
        match self {
            Self::Default | Self::Freethreaded | Self::Gil => false,
            Self::Debug | Self::FreethreadedDebug | Self::GilDebug => true,
        }
    }
}
impl PythonRequest {
    /// Create a request from Python version specifiers.
    pub fn from_specifiers(specifiers: &VersionSpecifiers) -> Option<Self> {
        let specifiers = specifiers.clone();
        if specifiers.is_empty() {
            return None;
        }

        Some(Self::Version(VersionRequest::from_specifiers(
            specifiers,
            PythonVariant::Default,
        )))
    }

    /// Create a request from a string.
    ///
    /// This cannot fail, which means weird inputs will be parsed as [`PythonRequest::File`] or
    /// [`PythonRequest::ExecutableName`].
    ///
    /// This is intended for parsing the argument to the `--python` flag. See also
    /// [`try_from_tool_name`][Self::try_from_tool_name] below.
    pub fn parse(value: &str) -> Self {
        Self::parse_with_working_directory(value, Path::new(""))
    }

    /// Parse a request, resolving relative paths against `working_directory`.
    pub fn parse_with_working_directory(value: &str, working_directory: &Path) -> Self {
        let lowercase_value = &value.to_ascii_lowercase();

        // Literals, e.g. `any` or `default`
        if lowercase_value == "any" {
            return Self::Any;
        }
        if lowercase_value == "default" {
            return Self::Default;
        }

        // the prefix of e.g. `python312` and the empty prefix of bare versions, e.g. `312`
        let abstract_version_prefixes = ["python", ""];
        let all_implementation_names = ImplementationName::iter_all().flat_map(|implementation| {
            std::iter::once(implementation.long_name()).chain(implementation.short_name())
        });
        // Abstract versions like `python@312`, `python312`, or `312`, plus implementations and
        // implementation versions like `pypy`, `pypy@312` or `pypy312`.
        if let Ok(Some(request)) = Self::parse_versions_and_implementations(
            abstract_version_prefixes,
            all_implementation_names,
            lowercase_value,
        ) {
            return request;
        }

        let value_as_path = working_directory.join(value);
        // e.g. /path/to/.venv
        if value_as_path.is_dir() {
            return Self::Directory(value_as_path);
        }
        // e.g. /path/to/python
        if value_as_path.is_file() {
            return Self::File(value_as_path);
        }

        // e.g. path/to/python on Windows, where path/to/python.exe is the true path
        #[cfg(windows)]
        if value_as_path.extension().is_none() {
            let value_as_path = value_as_path.with_extension(EXE_SUFFIX);
            if value_as_path.is_file() {
                return Self::File(value_as_path);
            }
        }

        // e.g. .\path\to\python3.exe or ./path/to/python3
        // If it contains a path separator, we'll treat it as a full path even if it does not exist
        if value.contains(std::path::MAIN_SEPARATOR) {
            return Self::File(value_as_path);
        }
        // e.g. ./path/to/python3.exe
        // On Windows, Unix path separators are often valid
        if cfg!(windows) && value.contains('/') {
            return Self::File(value_as_path);
        }
        if let Ok(request) = PythonDownloadRequest::from_str(value) {
            return Self::Key(request);
        }
        // Finally, we'll treat it as the name of an executable (i.e. in the search PATH)
        // e.g. foo.exe
        Self::ExecutableName(value.to_string())
    }

    /// Try to parse a tool name as a Python version, e.g. `uvx python311`.
    ///
    /// The `PythonRequest::parse` constructor above is intended for the `--python` flag, where the
    /// value is unambiguously a Python version. This alternate constructor is intended for `uvx`
    /// or `uvx --from`, where the executable could be either a Python version or a package name.
    /// There are several differences in behavior:
    ///
    /// - This only supports long names, including e.g. `pypy39` but **not** `pp39` or `39`.
    /// - On Windows only, this allows `pythonw` as an alias for `python`.
    /// - This allows `python` by itself (and on Windows, `pythonw`) as an alias for `default`.
    ///
    /// This can only return `Err` if `@` is used. Otherwise, if no match is found, it returns
    /// `Ok(None)`.
    pub fn try_from_tool_name(value: &str) -> Result<Option<Self>, PythonRequestError> {
        let lowercase_value = &value.to_ascii_lowercase();
        // Omitting the empty string from these lists excludes bare versions like "39".
        let abstract_version_prefixes = if cfg!(windows) {
            &["python", "pythonw"][..]
        } else {
            &["python"][..]
        };
        // e.g. just `python`
        if abstract_version_prefixes.contains(&lowercase_value.as_str()) {
            return Ok(Some(Self::Default));
        }
        Self::parse_versions_and_implementations(
            abstract_version_prefixes.iter().copied(),
            ImplementationName::iter_all().map(ImplementationName::long_name),
            lowercase_value,
        )
    }

    /// Take a value like `"python3.11"`, check whether it matches a set of abstract python
    /// prefixes (e.g. `"python"`, `"pythonw"`, or even `""`) or a set of specific Python
    /// implementations (e.g. `"cpython"` or `"pypy"`, possibly with abbreviations), and if so try
    /// to parse its version.
    ///
    /// This can only return `Err` if `@` is used, see
    /// [`try_split_prefix_and_version`][Self::try_split_prefix_and_version] below. Otherwise, if
    /// no match is found, it returns `Ok(None)`.
    fn parse_versions_and_implementations<'a>(
        // typically "python", possibly also "pythonw" or "" (for bare versions)
        abstract_version_prefixes: impl IntoIterator<Item = &'a str>,
        // expected to be either long names or all names
        implementation_names: impl IntoIterator<Item = &'a str>,
        // the string to parse
        lowercase_value: &str,
    ) -> Result<Option<Self>, PythonRequestError> {
        for prefix in abstract_version_prefixes {
            if let Some(version_request) =
                Self::try_split_prefix_and_version(prefix, lowercase_value)?
            {
                // e.g. `python39` or `python@39`
                // Note that e.g. `python` gets handled elsewhere, if at all. (It's currently
                // allowed in tool executables but not in --python flags.)
                return Ok(Some(Self::Version(version_request)));
            }
        }
        for implementation in implementation_names {
            if lowercase_value == implementation {
                return Ok(Some(Self::Implementation(
                    // e.g. `pypy`
                    // Safety: The name matched the possible names above
                    ImplementationName::from_str(implementation).unwrap(),
                )));
            }
            if let Some(version_request) =
                Self::try_split_prefix_and_version(implementation, lowercase_value)?
            {
                // e.g. `pypy39`
                return Ok(Some(Self::ImplementationVersion(
                    // Safety: The name matched the possible names above
                    ImplementationName::from_str(implementation).unwrap(),
                    version_request,
                )));
            }
        }
        Ok(None)
    }

    /// Take a value like `"python3.11"`, check whether it matches a target prefix (e.g.
    /// `"python"`, `"pypy"`, or even `""`), and if so try to parse its version.
    ///
    /// Failing to match the prefix (e.g. `"notpython3.11"`) or failing to parse a version (e.g.
    /// `"python3notaversion"`) is not an error, and those cases return `Ok(None)`. The `@`
    /// separator is optional, and this function can only return `Err` if `@` is used. There are
    /// two error cases:
    ///
    /// - The value starts with `@` (e.g. `@3.11`).
    /// - The prefix is a match, but the version is invalid (e.g. `python@3.not.a.version`).
    fn try_split_prefix_and_version(
        prefix: &str,
        lowercase_value: &str,
    ) -> Result<Option<VersionRequest>, PythonRequestError> {
        if lowercase_value.starts_with('@') {
            return Err(PythonRequestError::InvalidVersionRequest(
                lowercase_value.to_string(),
            ));
        }
        let Some(rest) = lowercase_value.strip_prefix(prefix) else {
            return Ok(None);
        };
        // Just the prefix by itself (e.g. "python") is handled elsewhere.
        if rest.is_empty() {
            return Ok(None);
        }
        // The @ separator is optional. If it's present, the right half must be a version, and
        // parsing errors are raised to the caller.
        if let Some(after_at) = rest.strip_prefix('@') {
            if after_at == "latest" {
                // Handle `@latest` as a special case. It's still an error for now, but we plan to
                // support it. TODO(zanieb): Add `PythonRequest::Latest`
                return Err(PythonRequestError::LatestVersionRequest);
            }
            return after_at.parse().map(Some);
        }
        // The @ was not present, so if the version fails to parse just return Ok(None). For
        // example, python3stuff.
        Ok(rest.parse().ok())
    }

    /// Check if this request includes a specific patch version.
    pub fn includes_patch(&self) -> bool {
        match self {
            Self::Default => false,
            Self::Any => false,
            Self::Version(version_request) => version_request.patch().is_some(),
            Self::Directory(..) => false,
            Self::File(..) => false,
            Self::ExecutableName(..) => false,
            Self::Implementation(..) => false,
            Self::ImplementationVersion(_, version) => version.patch().is_some(),
            Self::Key(request) => request
                .version
                .as_ref()
                .is_some_and(|request| request.patch().is_some()),
        }
    }

    /// Check if this request includes a specific prerelease version.
    pub fn includes_prerelease(&self) -> bool {
        match self {
            Self::Default => false,
            Self::Any => false,
            Self::Version(version_request) => version_request.prerelease().is_some(),
            Self::Directory(..) => false,
            Self::File(..) => false,
            Self::ExecutableName(..) => false,
            Self::Implementation(..) => false,
            Self::ImplementationVersion(_, version) => version.prerelease().is_some(),
            Self::Key(request) => request
                .version
                .as_ref()
                .is_some_and(|request| request.prerelease().is_some()),
        }
    }

    /// Require an exact architecture for requests that do not select one or name an executable.
    pub fn with_default_arch(&self, arch: Option<Arch>) -> Cow<'_, Self> {
        let Some(arch) = arch else {
            return Cow::Borrowed(self);
        };
        let Some(request) = PythonDownloadRequest::from_request(self) else {
            return Cow::Borrowed(self);
        };
        if request.arch().is_some() {
            return Cow::Borrowed(self);
        }
        Cow::Owned(Self::Key(request.with_arch(arch)))
    }

    /// Whether this request opts-in to a pre-release Python version.
    pub fn allows_prereleases(&self) -> bool {
        match self {
            Self::Default => false,
            Self::Any => true,
            Self::Version(version) => version.allows_prereleases(),
            Self::Directory(_) | Self::File(_) | Self::ExecutableName(_) => true,
            Self::Implementation(_) => false,
            Self::ImplementationVersion(_, _) => true,
            Self::Key(request) => request.allows_prereleases(),
        }
    }

    /// Whether this request opts-in to a debug Python version.
    pub fn allows_debug(&self) -> bool {
        match self {
            Self::Default => false,
            Self::Any => true,
            Self::Version(version) => version.is_debug(),
            Self::Directory(_) | Self::File(_) | Self::ExecutableName(_) => true,
            Self::Implementation(_) => false,
            Self::ImplementationVersion(_, _) => true,
            Self::Key(request) => request.allows_debug(),
        }
    }

    /// Whether this request opts-in to an alternative Python implementation, e.g., PyPy.
    pub fn allows_alternative_implementations(&self) -> bool {
        match self {
            Self::Default => false,
            Self::Any => true,
            Self::Version(_) => false,
            Self::Directory(_) | Self::File(_) | Self::ExecutableName(_) => true,
            Self::Implementation(implementation)
            | Self::ImplementationVersion(implementation, _) => {
                !matches!(implementation, ImplementationName::CPython)
            }
            Self::Key(request) => request.allows_alternative_implementations(),
        }
    }

    pub fn is_explicit_system(&self) -> bool {
        matches!(self, Self::File(_) | Self::Directory(_))
    }

    /// Serialize the request to a canonical representation.
    ///
    /// [`Self::parse`] should always return the same request when given the output of this method.
    pub fn to_canonical_string(&self) -> Cow<'_, str> {
        match self {
            Self::Any => Cow::Borrowed("any"),
            Self::Default => Cow::Borrowed("default"),
            Self::Version(version) => Cow::Owned(version.to_string()),
            Self::Directory(path) | Self::File(path) => path.to_string_lossy(),
            Self::ExecutableName(name) => Cow::Borrowed(name),
            Self::Implementation(implementation) => Cow::Borrowed(implementation.long_name()),
            Self::ImplementationVersion(implementation, version) => {
                Cow::Owned(format!("{implementation}@{version}"))
            }
            Self::Key(request) => Cow::Owned(request.to_string()),
        }
    }

    /// Convert an interpreter request into a concrete PEP 440 `Version` when possible.
    ///
    /// Returns `None` if the request doesn't carry an exact version
    pub fn as_pep440_version(&self) -> Option<Version> {
        match self {
            Self::Version(v) | Self::ImplementationVersion(_, v) => v.as_pep440_version(),
            Self::Key(download_request) => download_request
                .version()
                .and_then(VersionRequest::as_pep440_version),
            Self::Default
            | Self::Any
            | Self::Directory(_)
            | Self::File(_)
            | Self::ExecutableName(_)
            | Self::Implementation(_) => None,
        }
    }

    /// Convert an interpreter request into [`VersionSpecifiers`] representing the range of
    /// compatible versions.
    ///
    /// Returns `None` if the request doesn't carry version constraints (e.g., a path or
    /// executable name).
    fn as_version_specifiers(&self) -> Option<VersionSpecifiers> {
        match self {
            Self::Version(version) | Self::ImplementationVersion(_, version) => {
                version.as_version_specifiers()
            }
            Self::Key(download_request) => download_request
                .version()
                .and_then(VersionRequest::as_version_specifiers),
            Self::Default
            | Self::Any
            | Self::Directory(_)
            | Self::File(_)
            | Self::ExecutableName(_)
            | Self::Implementation(_) => None,
        }
    }

    /// Returns `true` when this request is compatible with the given `requires-python` specifier.
    ///
    /// Requests without version constraints (e.g., paths, executable names) are always considered
    /// compatible. For versioned requests, compatibility means the request's version range has a
    /// non-empty intersection with the `requires-python` range.
    pub fn intersects_specifiers(&self, requires_python: &VersionSpecifiers) -> bool {
        let Some(specifiers) = self.as_version_specifiers() else {
            return true;
        };

        let request_range = release_specifiers_to_ranges(specifiers);
        let requires_python_range = release_specifiers_to_ranges(requires_python.clone());
        !request_range
            .intersection(&requires_python_range)
            .is_empty()
    }
}

impl PythonSource {
    pub fn is_managed(self) -> bool {
        matches!(self, Self::Managed)
    }

    /// Whether a pre-release Python installation from this source can be used without opt-in.
    pub fn allows_prereleases(self) -> bool {
        match self {
            Self::Managed | Self::Registry | Self::MicrosoftStore => false,
            Self::SearchPath
            | Self::SearchPathFirst
            | Self::CondaPrefix
            | Self::BaseCondaPrefix
            | Self::ProvidedPath
            | Self::ParentInterpreter
            | Self::ActiveEnvironment
            | Self::DiscoveredEnvironment => true,
        }
    }

    /// Whether a debug Python installation from this source can be used without opt-in.
    pub fn allows_debug(self) -> bool {
        match self {
            Self::Managed | Self::Registry | Self::MicrosoftStore => false,
            Self::SearchPath
            | Self::SearchPathFirst
            | Self::CondaPrefix
            | Self::BaseCondaPrefix
            | Self::ProvidedPath
            | Self::ParentInterpreter
            | Self::ActiveEnvironment
            | Self::DiscoveredEnvironment => true,
        }
    }

    /// Whether an alternative Python implementation from this source can be used without opt-in.
    pub fn allows_alternative_implementations(self) -> bool {
        match self {
            Self::Managed
            | Self::Registry
            | Self::SearchPath
            // TODO(zanieb): We may want to allow this at some point, but when adding this variant
            // we want compatibility with existing behavior
            | Self::SearchPathFirst
            | Self::MicrosoftStore => false,
            Self::CondaPrefix
            | Self::BaseCondaPrefix
            | Self::ProvidedPath
            | Self::ParentInterpreter
            | Self::ActiveEnvironment
            | Self::DiscoveredEnvironment => true,
        }
    }

    /// Whether this source **could** be a virtual environment.
    ///
    /// This excludes the [`PythonSource::SearchPath`] although it could be in a virtual
    /// environment; pragmatically, that's not common and saves us from querying a bunch of system
    /// interpreters for no reason. It seems dubious to consider an interpreter in the `PATH` as a
    /// target virtual environment if it's not discovered through our virtual environment-specific
    /// patterns. Instead, we special case the first Python executable found on the `PATH` with
    /// [`PythonSource::SearchPathFirst`], allowing us to check if that's a virtual environment.
    /// This enables targeting the virtual environment with uv by putting its `bin/` on the `PATH`
    /// without setting `VIRTUAL_ENV` — but if there's another interpreter before it we will ignore
    /// it.
    pub fn is_maybe_virtualenv(self) -> bool {
        match self {
            Self::ProvidedPath
            | Self::ActiveEnvironment
            | Self::DiscoveredEnvironment
            | Self::CondaPrefix
            | Self::BaseCondaPrefix
            | Self::ParentInterpreter
            | Self::SearchPathFirst => true,
            Self::Managed | Self::SearchPath | Self::Registry | Self::MicrosoftStore => false,
        }
    }

    /// Whether this source is "explicit", e.g., it was directly provided by the user or is
    /// an active virtual environment.
    pub fn is_explicit(self) -> bool {
        match self {
            Self::ProvidedPath
            | Self::ParentInterpreter
            | Self::ActiveEnvironment
            | Self::CondaPrefix => true,
            Self::Managed
            | Self::DiscoveredEnvironment
            | Self::SearchPath
            | Self::SearchPathFirst
            | Self::Registry
            | Self::MicrosoftStore
            | Self::BaseCondaPrefix => false,
        }
    }

    /// Whether this source **could** be a system interpreter.
    pub fn is_maybe_system(self) -> bool {
        match self {
            Self::CondaPrefix
            | Self::BaseCondaPrefix
            | Self::ParentInterpreter
            | Self::ProvidedPath
            | Self::Managed
            | Self::SearchPath
            | Self::SearchPathFirst
            | Self::Registry
            | Self::MicrosoftStore => true,
            Self::ActiveEnvironment | Self::DiscoveredEnvironment => false,
        }
    }
}

impl PythonPreference {
    pub fn allows_source(self, source: PythonSource) -> bool {
        // If not dealing with a system interpreter source, we don't care about the preference
        if !matches!(
            source,
            PythonSource::Managed | PythonSource::SearchPath | PythonSource::Registry
        ) {
            return true;
        }

        match self {
            Self::OnlyManaged => matches!(source, PythonSource::Managed),
            Self::Managed | Self::System => matches!(
                source,
                PythonSource::Managed | PythonSource::SearchPath | PythonSource::Registry
            ),
            Self::OnlySystem => {
                matches!(source, PythonSource::SearchPath | PythonSource::Registry)
            }
        }
    }

    pub fn allows_managed(self) -> bool {
        match self {
            Self::OnlySystem => false,
            Self::Managed | Self::System | Self::OnlyManaged => true,
        }
    }

    /// Returns a new preference when the `--system` flag is used.
    ///
    /// This will convert [`PythonPreference::Managed`] to [`PythonPreference::System`] when system
    /// is set.
    #[must_use]
    pub fn with_system_flag(self, system: bool) -> Self {
        match self {
            // TODO(zanieb): It's not clear if we want to allow `--system` to override
            // `--managed-python`. We should probably make this `from_system_flag` and refactor
            // handling of the `PythonPreference` to use an `Option` so we can tell if the user
            // provided it?
            Self::OnlyManaged => self,
            Self::Managed => {
                if system {
                    Self::System
                } else {
                    self
                }
            }
            Self::System => self,
            Self::OnlySystem => self,
        }
    }
}

impl PythonDownloads {
    pub fn is_automatic(self) -> bool {
        matches!(self, Self::Automatic)
    }
}

impl EnvironmentPreference {
    pub fn from_system_flag(system: bool, mutable: bool) -> Self {
        match (system, mutable) {
            // When the system flag is provided, ignore virtual environments.
            (true, _) => Self::OnlySystem,
            // For mutable operations, only allow discovery of the system with explicit selection.
            (false, true) => Self::ExplicitSystem,
            // For immutable operations, we allow discovery of the system environment
            (false, false) => Self::Any,
        }
    }
}

#[derive(Debug, Clone, Default, Copy, PartialEq, Eq)]
pub struct ExecutableName {
    implementation: Option<ImplementationName>,
    major: Option<u8>,
    minor: Option<u8>,
    patch: Option<u8>,
    prerelease: Option<Prerelease>,
    variant: PythonVariant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ExecutableNameComparator<'a> {
    name: ExecutableName,
    request: &'a VersionRequest,
    implementation: Option<&'a ImplementationName>,
}

impl Ord for ExecutableNameComparator<'_> {
    /// Note the comparison returns a reverse priority ordering.
    ///
    /// Higher priority items are "Greater" than lower priority items.
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Prefer the default name over a specific implementation, unless an implementation was
        // requested
        let name_ordering = if self.implementation.is_some() {
            std::cmp::Ordering::Greater
        } else {
            std::cmp::Ordering::Less
        };
        if self.name.implementation.is_none() && other.name.implementation.is_some() {
            return name_ordering.reverse();
        }
        if self.name.implementation.is_some() && other.name.implementation.is_none() {
            return name_ordering;
        }
        // Otherwise, use the names in supported order
        let ordering = self.name.implementation.cmp(&other.name.implementation);
        if ordering != std::cmp::Ordering::Equal {
            return ordering;
        }
        let ordering = self.name.major.cmp(&other.name.major);
        let is_default_request =
            matches!(self.request, VersionRequest::Any | VersionRequest::Default);
        if ordering != std::cmp::Ordering::Equal {
            return if is_default_request {
                ordering.reverse()
            } else {
                ordering
            };
        }
        let ordering = self.name.minor.cmp(&other.name.minor);
        if ordering != std::cmp::Ordering::Equal {
            return if is_default_request {
                ordering.reverse()
            } else {
                ordering
            };
        }
        let ordering = self.name.patch.cmp(&other.name.patch);
        if ordering != std::cmp::Ordering::Equal {
            return if is_default_request {
                ordering.reverse()
            } else {
                ordering
            };
        }
        let ordering = self.name.prerelease.cmp(&other.name.prerelease);
        if ordering != std::cmp::Ordering::Equal {
            return if is_default_request {
                ordering.reverse()
            } else {
                ordering
            };
        }
        let ordering = self.name.variant.cmp(&other.name.variant);
        if ordering != std::cmp::Ordering::Equal {
            return if is_default_request {
                ordering.reverse()
            } else {
                ordering
            };
        }
        ordering
    }
}

impl PartialOrd for ExecutableNameComparator<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl ExecutableName {
    #[must_use]
    fn with_implementation(mut self, implementation: ImplementationName) -> Self {
        self.implementation = Some(implementation);
        self
    }

    #[must_use]
    fn with_major(mut self, major: u8) -> Self {
        self.major = Some(major);
        self
    }

    #[must_use]
    fn with_minor(mut self, minor: u8) -> Self {
        self.minor = Some(minor);
        self
    }

    #[must_use]
    fn with_patch(mut self, patch: u8) -> Self {
        self.patch = Some(patch);
        self
    }

    #[must_use]
    fn with_prerelease(mut self, prerelease: Prerelease) -> Self {
        self.prerelease = Some(prerelease);
        self
    }

    #[must_use]
    fn with_variant(mut self, variant: PythonVariant) -> Self {
        self.variant = variant;
        self
    }

    fn into_comparator<'a>(
        self,
        request: &'a VersionRequest,
        implementation: Option<&'a ImplementationName>,
    ) -> ExecutableNameComparator<'a> {
        ExecutableNameComparator {
            name: self,
            request,
            implementation,
        }
    }
}

impl fmt::Display for ExecutableName {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if let Some(implementation) = self.implementation {
            write!(f, "{implementation}")?;
        } else {
            f.write_str("python")?;
        }
        if let Some(major) = self.major {
            write!(f, "{major}")?;
            if let Some(minor) = self.minor {
                write!(f, ".{minor}")?;
                if let Some(patch) = self.patch {
                    write!(f, ".{patch}")?;
                }
            }
        }
        if let Some(prerelease) = &self.prerelease {
            write!(f, "{prerelease}")?;
        }
        f.write_str(self.variant.executable_suffix())?;
        f.write_str(EXE_SUFFIX)?;
        Ok(())
    }
}

impl VersionRequest {
    /// Create a [`VersionRequest`] from [`VersionSpecifiers`].
    ///
    /// If the specifiers consist of a single `==` constraint, the version is parsed as a
    /// concrete version request (e.g., `MajorMinorPatch`) rather than a range.
    pub fn from_specifiers(specifiers: VersionSpecifiers, variant: PythonVariant) -> Self {
        if let [specifier] = specifiers.iter().as_slice()
            && specifier.operator() == &uv_pep440::Operator::Equal
            && let Ok(request) = Self::from_str(&specifier.version().to_string())
        {
            return request;
        }
        Self::Range(specifiers, variant)
    }

    /// Drop any patch or prerelease information from the version request.
    #[must_use]
    pub fn only_minor(self) -> Self {
        match self {
            Self::Any => self,
            Self::Default => self,
            Self::Range(specifiers, variant) => Self::Range(
                specifiers
                    .into_iter()
                    .map(|s| s.only_minor_release())
                    .collect(),
                variant,
            ),
            Self::Major(..) => self,
            Self::MajorMinor(..) => self,
            Self::MajorMinorPatch(major, minor, _, variant)
            | Self::MajorMinorPrerelease(major, minor, _, variant)
            | Self::MajorMinorPatchPrerelease(major, minor, _, _, variant) => {
                Self::MajorMinor(major, minor, variant)
            }
        }
    }

    /// Return possible executable names for the given version request.
    pub fn executable_names(
        &self,
        implementation: Option<&ImplementationName>,
    ) -> Vec<ExecutableName> {
        let prerelease = match self {
            Self::MajorMinorPrerelease(_, _, prerelease, _)
            | Self::MajorMinorPatchPrerelease(_, _, _, prerelease, _) => {
                // Include the prerelease version, e.g., `python3.8a`
                Some(prerelease)
            }
            _ => None,
        };

        // Push a default one
        let mut names = Vec::new();
        names.push(ExecutableName::default());

        // Collect each variant depending on the number of versions
        if let Some(major) = self.major() {
            // e.g. `python3`
            names.push(ExecutableName::default().with_major(major));
            if let Some(minor) = self.minor() {
                // e.g., `python3.12`
                names.push(
                    ExecutableName::default()
                        .with_major(major)
                        .with_minor(minor),
                );
                if let Some(patch) = self.patch() {
                    // e.g, `python3.12.1`
                    names.push(
                        ExecutableName::default()
                            .with_major(major)
                            .with_minor(minor)
                            .with_patch(patch),
                    );
                }
            }
        } else {
            // Include `3` by default, e.g., `python3`
            names.push(ExecutableName::default().with_major(3));
        }

        if let Some(prerelease) = prerelease {
            // Include the prerelease version, e.g., `python3.8a`
            for i in 0..names.len() {
                let name = names[i];
                if name.minor.is_none() {
                    // We don't want to include the pre-release marker here
                    // e.g. `pythonrc1` and `python3rc1` don't make sense
                    continue;
                }
                names.push(name.with_prerelease(*prerelease));
            }
        }

        // Add all the implementation-specific names
        if let Some(implementation) = implementation {
            for i in 0..names.len() {
                let name = names[i].with_implementation(*implementation);
                names.push(name);
            }
        } else {
            // When looking for all implementations, include all possible names
            if matches!(self, Self::Any) {
                for i in 0..names.len() {
                    for implementation in ImplementationName::iter_all() {
                        let name = names[i].with_implementation(implementation);
                        names.push(name);
                    }
                }
            }
        }

        // Include free-threaded variants
        if let Some(variant) = self.variant()
            && variant != PythonVariant::Default
        {
            for i in 0..names.len() {
                let name = names[i].with_variant(variant);
                names.push(name);
            }
        }

        names.sort_unstable_by_key(|name| name.into_comparator(self, implementation));
        names.reverse();

        names
    }

    /// Return the major version segment of the request, if any.
    fn major(&self) -> Option<u8> {
        match self {
            Self::Any | Self::Default | Self::Range(_, _) => None,
            Self::Major(major, _) => Some(*major),
            Self::MajorMinor(major, _, _) => Some(*major),
            Self::MajorMinorPatch(major, _, _, _) => Some(*major),
            Self::MajorMinorPrerelease(major, _, _, _) => Some(*major),
            Self::MajorMinorPatchPrerelease(major, _, _, _, _) => Some(*major),
        }
    }

    /// Return the minor version segment of the request, if any.
    fn minor(&self) -> Option<u8> {
        match self {
            Self::Any | Self::Default | Self::Range(_, _) => None,
            Self::Major(_, _) => None,
            Self::MajorMinor(_, minor, _) => Some(*minor),
            Self::MajorMinorPatch(_, minor, _, _) => Some(*minor),
            Self::MajorMinorPrerelease(_, minor, _, _) => Some(*minor),
            Self::MajorMinorPatchPrerelease(_, minor, _, _, _) => Some(*minor),
        }
    }

    /// Return the patch version segment of the request, if any.
    pub fn patch(&self) -> Option<u8> {
        match self {
            Self::Any | Self::Default | Self::Range(_, _) => None,
            Self::Major(_, _) => None,
            Self::MajorMinor(_, _, _) => None,
            Self::MajorMinorPatch(_, _, patch, _) => Some(*patch),
            Self::MajorMinorPrerelease(_, _, _, _) => None,
            Self::MajorMinorPatchPrerelease(_, _, patch, _, _) => Some(*patch),
        }
    }

    /// Return the pre-release segment of the request, if any.
    fn prerelease(&self) -> Option<&Prerelease> {
        match self {
            Self::Any | Self::Default | Self::Range(_, _) => None,
            Self::Major(_, _) => None,
            Self::MajorMinor(_, _, _) => None,
            Self::MajorMinorPatch(_, _, _, _) => None,
            Self::MajorMinorPrerelease(_, _, prerelease, _) => Some(prerelease),
            Self::MajorMinorPatchPrerelease(_, _, _, prerelease, _) => Some(prerelease),
        }
    }

    /// Check if the request is for a version supported by uv.
    ///
    /// If not, an `Err` is returned with an explanatory message.
    pub fn check_supported(&self) -> Result<(), String> {
        match self {
            Self::Any | Self::Default => (),
            Self::Major(major, _) => {
                if *major < 3 {
                    return Err(format!(
                        "Python <3 is not supported but {major} was requested."
                    ));
                }
            }
            Self::MajorMinor(major, minor, _) => {
                if (*major, *minor) < (3, 6) {
                    return Err(format!(
                        "Python <3.6 is not supported but {major}.{minor} was requested."
                    ));
                }
            }
            Self::MajorMinorPatch(major, minor, patch, _) => {
                if (*major, *minor) < (3, 6) {
                    return Err(format!(
                        "Python <3.6 is not supported but {major}.{minor}.{patch} was requested."
                    ));
                }
            }
            Self::MajorMinorPrerelease(major, minor, prerelease, _) => {
                if (*major, *minor) < (3, 6) {
                    return Err(format!(
                        "Python <3.6 is not supported but {major}.{minor}{prerelease} was requested."
                    ));
                }
            }
            Self::MajorMinorPatchPrerelease(major, minor, patch, prerelease, _) => {
                if (*major, *minor) < (3, 6) {
                    return Err(format!(
                        "Python <3.6 is not supported but {major}.{minor}.{patch}{prerelease} was requested."
                    ));
                }
            }
            // TODO(zanieb): We could do some checking here to see if the range can be satisfied
            Self::Range(_, _) => (),
        }

        if self.is_freethreaded()
            && let Self::MajorMinor(major, minor, _) = self.clone().without_patch()
            && (major, minor) < (3, 13)
        {
            return Err(format!(
                "Python <3.13 does not support free-threading but {self} was requested."
            ));
        }

        Ok(())
    }

    /// Change this request into a request appropriate for the given [`PythonSource`].
    ///
    /// For example, if [`VersionRequest::Default`] is requested, it will be changed to
    /// [`VersionRequest::Any`] for sources that should allow non-default interpreters like
    /// free-threaded variants.
    #[must_use]
    pub fn into_request_for_source(self, source: PythonSource) -> Self {
        match self {
            Self::Default => match source {
                PythonSource::ParentInterpreter
                | PythonSource::CondaPrefix
                | PythonSource::BaseCondaPrefix
                | PythonSource::ProvidedPath
                | PythonSource::DiscoveredEnvironment
                | PythonSource::ActiveEnvironment => Self::Any,
                PythonSource::SearchPath
                | PythonSource::SearchPathFirst
                | PythonSource::Registry
                | PythonSource::MicrosoftStore
                | PythonSource::Managed => Self::Default,
            },
            _ => self,
        }
    }

    /// Check if a version is compatible with the request.
    ///
    /// This only rules out incompatible versions before querying an interpreter; full matching
    /// must also check the interpreter's variant.
    pub fn matches_version(&self, version: &PythonVersion) -> bool {
        match self {
            Self::Any | Self::Default => true,
            Self::Major(major, _) => version.major() == *major,
            Self::MajorMinor(major, minor, _) => {
                (version.major(), version.minor()) == (*major, *minor)
            }
            Self::MajorMinorPatch(major, minor, patch, _) => {
                (version.major(), version.minor(), version.patch())
                    == (*major, *minor, Some(*patch))
            }
            Self::Range(specifiers, _) => {
                // If the specifier contains pre-releases, use the full version for comparison.
                // Otherwise, strip pre-release so that, e.g., `>=3.14` matches `3.14.0rc3`.
                let version = if specifiers
                    .iter()
                    .any(uv_pep440::VersionSpecifier::any_prerelease)
                {
                    Cow::Borrowed(&version.version)
                } else {
                    Cow::Owned(version.version.only_release())
                };
                specifiers.contains(&version)
            }
            Self::MajorMinorPrerelease(major, minor, prerelease, _) => {
                (version.major(), version.minor(), version.pre())
                    == (*major, *minor, Some(*prerelease))
            }
            Self::MajorMinorPatchPrerelease(major, minor, patch, prerelease, _) => {
                (
                    version.major(),
                    version.minor(),
                    version.patch(),
                    version.pre(),
                ) == (*major, *minor, Some(*patch), Some(*prerelease))
            }
        }
    }

    /// Check if major and minor version segments are compatible with the request.
    ///
    /// This only rules out incompatible versions before querying an interpreter; full matching
    /// must also check the interpreter's variant.
    pub fn matches_major_minor(&self, major: u8, minor: u8) -> bool {
        match self {
            Self::Any | Self::Default => true,
            Self::Major(self_major, _) => *self_major == major,
            Self::MajorMinor(self_major, self_minor, _) => {
                (*self_major, *self_minor) == (major, minor)
            }
            Self::MajorMinorPatch(self_major, self_minor, _, _) => {
                (*self_major, *self_minor) == (major, minor)
            }
            Self::Range(specifiers, _) => {
                let range = release_specifiers_to_ranges(specifiers.clone());
                let Some((lower, upper)) = range.bounding_range() else {
                    return true;
                };
                let version = Version::new([u64::from(major), u64::from(minor)]);

                let lower = LowerBound::new(lower.cloned());
                if !lower.major_minor().contains(&version) {
                    return false;
                }

                let upper = UpperBound::new(upper.cloned());
                if !upper.major_minor().contains(&version) {
                    return false;
                }

                true
            }
            Self::MajorMinorPrerelease(self_major, self_minor, _, _) => {
                (*self_major, *self_minor) == (major, minor)
            }
            Self::MajorMinorPatchPrerelease(self_major, self_minor, _, _, _) => {
                (*self_major, *self_minor) == (major, minor)
            }
        }
    }

    /// Check if major, minor, patch, and prerelease version segments are compatible with the
    /// request.
    ///
    /// This only rules out incompatible versions before querying an interpreter; full matching
    /// must also check the interpreter's variant.
    pub(crate) fn matches_major_minor_patch_prerelease(
        &self,
        major: u8,
        minor: u8,
        patch: u8,
        prerelease: Option<Prerelease>,
    ) -> bool {
        match self {
            Self::Any | Self::Default => true,
            Self::Major(self_major, _) => *self_major == major,
            Self::MajorMinor(self_major, self_minor, _) => {
                (*self_major, *self_minor) == (major, minor)
            }
            Self::MajorMinorPatch(self_major, self_minor, self_patch, _) => {
                (*self_major, *self_minor, *self_patch) == (major, minor, patch)
                    // When a patch version is included, we treat it as a request for a stable
                    // release
                    && prerelease.is_none()
            }
            Self::Range(specifiers, _) => specifiers.contains(
                &Version::new([u64::from(major), u64::from(minor), u64::from(patch)])
                    .with_pre(prerelease),
            ),
            Self::MajorMinorPrerelease(self_major, self_minor, self_prerelease, _) => {
                // Pre-releases without a patch in the request match the zero patch version
                (*self_major, *self_minor, 0, Some(*self_prerelease))
                    == (major, minor, patch, prerelease)
            }
            Self::MajorMinorPatchPrerelease(
                self_major,
                self_minor,
                self_patch,
                self_prerelease,
                _,
            ) => {
                (
                    *self_major,
                    *self_minor,
                    *self_patch,
                    Some(*self_prerelease),
                ) == (major, minor, patch, prerelease)
            }
        }
    }

    /// Check if a [`PythonInstallationKey`] is compatible with the request.
    ///
    /// This only rules out incompatible versions before querying an interpreter; full matching
    /// must also check the interpreter's variant.
    pub fn matches_installation_key(&self, key: &PythonInstallationKey) -> bool {
        self.matches_major_minor_patch_prerelease(key.major, key.minor, key.patch, key.prerelease())
    }

    /// Whether a patch version segment is present in the request.
    pub fn has_patch(&self) -> bool {
        match self {
            Self::Any | Self::Default => false,
            Self::Major(..) => false,
            Self::MajorMinor(..) => false,
            Self::MajorMinorPatch(..) => true,
            Self::MajorMinorPrerelease(..) => false,
            Self::MajorMinorPatchPrerelease(..) => true,
            Self::Range(_, _) => false,
        }
    }

    /// Return a new [`VersionRequest`] without the patch version if possible.
    ///
    /// If the patch version is not present, the request is returned unchanged.
    #[must_use]
    pub fn without_patch(self) -> Self {
        match self {
            Self::Default => Self::Default,
            Self::Any => Self::Any,
            Self::Major(major, variant) => Self::Major(major, variant),
            Self::MajorMinor(major, minor, variant) => Self::MajorMinor(major, minor, variant),
            Self::MajorMinorPatch(major, minor, _, variant) => {
                Self::MajorMinor(major, minor, variant)
            }
            Self::MajorMinorPrerelease(major, minor, prerelease, variant) => {
                Self::MajorMinorPrerelease(major, minor, prerelease, variant)
            }
            Self::MajorMinorPatchPrerelease(major, minor, _, prerelease, variant) => {
                Self::MajorMinorPrerelease(major, minor, prerelease, variant)
            }
            Self::Range(_, _) => self,
        }
    }

    /// Whether this request should allow selection of pre-release versions.
    pub(crate) fn allows_prereleases(&self) -> bool {
        match self {
            Self::Default => false,
            Self::Any => true,
            Self::Major(..) => false,
            Self::MajorMinor(..) => false,
            Self::MajorMinorPatch(..) => false,
            Self::MajorMinorPrerelease(..) => true,
            Self::MajorMinorPatchPrerelease(..) => true,
            Self::Range(specifiers, _) => specifiers.iter().any(VersionSpecifier::any_prerelease),
        }
    }

    /// Whether this request is for a debug Python variant.
    pub(crate) fn is_debug(&self) -> bool {
        match self {
            Self::Any | Self::Default => false,
            Self::Major(_, variant)
            | Self::MajorMinor(_, _, variant)
            | Self::MajorMinorPatch(_, _, _, variant)
            | Self::MajorMinorPrerelease(_, _, _, variant)
            | Self::MajorMinorPatchPrerelease(_, _, _, _, variant)
            | Self::Range(_, variant) => variant.is_debug(),
        }
    }

    /// Whether this request is for a free-threaded Python variant.
    fn is_freethreaded(&self) -> bool {
        match self {
            Self::Any | Self::Default => false,
            Self::Major(_, variant)
            | Self::MajorMinor(_, _, variant)
            | Self::MajorMinorPatch(_, _, _, variant)
            | Self::MajorMinorPrerelease(_, _, _, variant)
            | Self::MajorMinorPatchPrerelease(_, _, _, _, variant)
            | Self::Range(_, variant) => variant.is_freethreaded(),
        }
    }

    /// Return the [`PythonVariant`] of the request, if any.
    pub(crate) fn variant(&self) -> Option<PythonVariant> {
        match self {
            Self::Any => None,
            Self::Default => Some(PythonVariant::Default),
            Self::Major(_, variant)
            | Self::MajorMinor(_, _, variant)
            | Self::MajorMinorPatch(_, _, _, variant)
            | Self::MajorMinorPrerelease(_, _, _, variant)
            | Self::MajorMinorPatchPrerelease(_, _, _, _, variant)
            | Self::Range(_, variant) => Some(*variant),
        }
    }

    /// Convert this request into a concrete PEP 440 `Version` when possible.
    ///
    /// Returns `None` for non-concrete requests
    fn as_pep440_version(&self) -> Option<Version> {
        match self {
            Self::Default | Self::Any | Self::Range(_, _) => None,
            Self::Major(major, _) => Some(Version::new([u64::from(*major)])),
            Self::MajorMinor(major, minor, _) => {
                Some(Version::new([u64::from(*major), u64::from(*minor)]))
            }
            Self::MajorMinorPatch(major, minor, patch, _) => Some(Version::new([
                u64::from(*major),
                u64::from(*minor),
                u64::from(*patch),
            ])),
            // Pre-releases without a patch use the zero patch version
            Self::MajorMinorPrerelease(major, minor, prerelease, _) => Some(
                Version::new([u64::from(*major), u64::from(*minor), 0]).with_pre(Some(*prerelease)),
            ),
            Self::MajorMinorPatchPrerelease(major, minor, patch, prerelease, _) => Some(
                Version::new([u64::from(*major), u64::from(*minor), u64::from(*patch)])
                    .with_pre(Some(*prerelease)),
            ),
        }
    }

    /// Convert this request into [`VersionSpecifiers`] representing the range of compatible
    /// versions.
    ///
    /// Returns `None` for requests without version constraints (e.g., [`VersionRequest::Default`]
    /// and [`VersionRequest::Any`]).
    fn as_version_specifiers(&self) -> Option<VersionSpecifiers> {
        match self {
            Self::Default | Self::Any => None,
            Self::Major(major, _) => Some(VersionSpecifiers::from(
                VersionSpecifier::equals_star_version(Version::new([u64::from(*major)])),
            )),
            Self::MajorMinor(major, minor, _) => Some(VersionSpecifiers::from(
                VersionSpecifier::equals_star_version(Version::new([
                    u64::from(*major),
                    u64::from(*minor),
                ])),
            )),
            Self::MajorMinorPatch(major, minor, patch, _) => {
                Some(VersionSpecifiers::from(VersionSpecifier::equals_version(
                    Version::new([u64::from(*major), u64::from(*minor), u64::from(*patch)]),
                )))
            }
            Self::MajorMinorPrerelease(major, minor, prerelease, _) => {
                Some(VersionSpecifiers::from(VersionSpecifier::equals_version(
                    Version::new([u64::from(*major), u64::from(*minor), 0])
                        .with_pre(Some(*prerelease)),
                )))
            }
            Self::MajorMinorPatchPrerelease(major, minor, patch, prerelease, _) => {
                Some(VersionSpecifiers::from(VersionSpecifier::equals_version(
                    Version::new([u64::from(*major), u64::from(*minor), u64::from(*patch)])
                        .with_pre(Some(*prerelease)),
                )))
            }
            Self::Range(specifiers, _) => Some(specifiers.clone()),
        }
    }
}

impl FromStr for VersionRequest {
    type Err = PythonRequestError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        /// Extract the variant from the end of a version request string, returning the prefix and
        /// the variant type.
        fn parse_variant(s: &str) -> Result<(&str, PythonVariant), PythonRequestError> {
            // This cannot be a valid version, just error immediately
            if s.chars().all(char::is_alphabetic) {
                return Err(PythonRequestError::InvalidVersionRequest(s.to_string()));
            }

            let Some(mut start) = s.rfind(|c: char| c.is_ascii_digit()) else {
                return Ok((s, PythonVariant::Default));
            };

            // Advance past the first digit
            start += 1;

            // Ensure we're not out of bounds
            if start + 1 > s.len() {
                return Ok((s, PythonVariant::Default));
            }

            let variant = &s[start..];
            let prefix = &s[..start];

            // Strip a leading `+` if present
            let variant = variant.strip_prefix('+').unwrap_or(variant);

            // TODO(zanieb): Special-case error for use of `dt` instead of `td`

            // If there's not a valid variant, fallback to failure in [`Version::from_str`]
            let Ok(variant) = PythonVariant::from_str(variant) else {
                return Ok((s, PythonVariant::Default));
            };

            Ok((prefix, variant))
        }

        let (s, variant) = parse_variant(s)?;
        let Ok(version) = Version::from_str(s) else {
            return parse_version_specifiers_request(s, variant);
        };

        // Split the release component if it uses the wheel tag format (e.g., `38`)
        let version = split_wheel_tag_release_version(version);

        // We dont allow post or dev version here
        if version.post().is_some() || version.dev().is_some() {
            return Err(PythonRequestError::InvalidVersionRequest(s.to_string()));
        }

        // We don't allow local version suffixes unless they're variants, in which case they'd
        // already be stripped.
        if !version.local().is_empty() {
            return Err(PythonRequestError::InvalidVersionRequest(s.to_string()));
        }

        // Cast the release components into u8s since that's what we use in `VersionRequest`
        let Ok(release) = try_into_u8_slice(&version.release()) else {
            return Err(PythonRequestError::InvalidVersionRequest(s.to_string()));
        };

        let prerelease = version.pre();

        match release.as_slice() {
            // e.g. `3
            [major] => {
                // Prereleases are not allowed here, e.g., `3rc1` doesn't make sense
                if prerelease.is_some() {
                    return Err(PythonRequestError::InvalidVersionRequest(s.to_string()));
                }
                Ok(Self::Major(*major, variant))
            }
            // e.g. `3.12` or `312` or `3.13rc1`
            [major, minor] => {
                if let Some(prerelease) = prerelease {
                    return Ok(Self::MajorMinorPrerelease(
                        *major, *minor, prerelease, variant,
                    ));
                }
                Ok(Self::MajorMinor(*major, *minor, variant))
            }
            // e.g. `3.12.1`, `3.13.0rc1`, or `3.14.5rc1`
            [major, minor, patch] => {
                if let Some(prerelease) = prerelease {
                    if *patch == 0 {
                        return Ok(Self::MajorMinorPrerelease(
                            *major, *minor, prerelease, variant,
                        ));
                    }
                    return Ok(Self::MajorMinorPatchPrerelease(
                        *major, *minor, *patch, prerelease, variant,
                    ));
                }
                Ok(Self::MajorMinorPatch(*major, *minor, *patch, variant))
            }
            _ => Err(PythonRequestError::InvalidVersionRequest(s.to_string())),
        }
    }
}

impl FromStr for PythonVariant {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "t" | "freethreaded" => Ok(Self::Freethreaded),
            "d" | "debug" => Ok(Self::Debug),
            "td" | "freethreaded+debug" => Ok(Self::FreethreadedDebug),
            "gil" => Ok(Self::Gil),
            "gil+debug" => Ok(Self::GilDebug),
            "" => Ok(Self::Default),
            _ => Err(()),
        }
    }
}

impl fmt::Display for PythonVariant {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Default => f.write_str("default"),
            Self::Debug => f.write_str("debug"),
            Self::Freethreaded => f.write_str("freethreaded"),
            Self::FreethreadedDebug => f.write_str("freethreaded+debug"),
            Self::Gil => f.write_str("gil"),
            Self::GilDebug => f.write_str("gil+debug"),
        }
    }
}

fn parse_version_specifiers_request(
    s: &str,
    variant: PythonVariant,
) -> Result<VersionRequest, PythonRequestError> {
    let Ok(specifiers) = VersionSpecifiers::from_str(s) else {
        return Err(PythonRequestError::InvalidVersionRequest(s.to_string()));
    };
    if specifiers.is_empty() {
        return Err(PythonRequestError::InvalidVersionRequest(s.to_string()));
    }
    Ok(VersionRequest::from_specifiers(specifiers, variant))
}

impl From<&PythonVersion> for VersionRequest {
    fn from(version: &PythonVersion) -> Self {
        Self::from_str(&version.string)
            .expect("Valid `PythonVersion`s should be valid `VersionRequest`s")
    }
}

impl fmt::Display for VersionRequest {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Any => f.write_str("any"),
            Self::Default => f.write_str("default"),
            Self::Major(major, variant) => write!(f, "{major}{}", variant.display_suffix()),
            Self::MajorMinor(major, minor, variant) => {
                write!(f, "{major}.{minor}{}", variant.display_suffix())
            }
            Self::MajorMinorPatch(major, minor, patch, variant) => {
                write!(f, "{major}.{minor}.{patch}{}", variant.display_suffix())
            }
            Self::MajorMinorPrerelease(major, minor, prerelease, variant) => {
                write!(f, "{major}.{minor}{prerelease}{}", variant.display_suffix())
            }
            Self::MajorMinorPatchPrerelease(major, minor, patch, prerelease, variant) => {
                write!(
                    f,
                    "{major}.{minor}.{patch}{prerelease}{}",
                    variant.display_suffix()
                )
            }
            Self::Range(specifiers, _) => write!(f, "{specifiers}"),
        }
    }
}

impl fmt::Display for PythonRequest {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Default => write!(f, "a default Python"),
            Self::Any => write!(f, "any Python"),
            Self::Version(version) => write!(f, "Python {version}"),
            Self::Directory(path) => write!(f, "directory `{}`", path.user_display()),
            Self::File(path) => write!(f, "path `{}`", path.user_display()),
            Self::ExecutableName(name) => write!(f, "executable name `{name}`"),
            Self::Implementation(implementation) => {
                write!(f, "{}", implementation.pretty())
            }
            Self::ImplementationVersion(implementation, version) => {
                write!(f, "{} {version}", implementation.pretty())
            }
            Self::Key(request) => write!(f, "{request}"),
        }
    }
}

impl fmt::Display for PythonSource {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::ProvidedPath => f.write_str("provided path"),
            Self::ActiveEnvironment => f.write_str("active virtual environment"),
            Self::CondaPrefix | Self::BaseCondaPrefix => f.write_str("conda prefix"),
            Self::DiscoveredEnvironment => f.write_str("virtual environment"),
            Self::SearchPath => f.write_str("search path"),
            Self::SearchPathFirst => f.write_str("first executable in the search path"),
            Self::Registry => f.write_str("registry"),
            Self::MicrosoftStore => f.write_str("Microsoft Store"),
            Self::Managed => f.write_str("managed installations"),
            Self::ParentInterpreter => f.write_str("parent interpreter"),
        }
    }
}

impl PythonPreference {
    /// Return the sources that are considered when searching for a Python interpreter with this
    /// preference.
    pub fn sources(self) -> &'static [PythonSource] {
        match self {
            Self::OnlyManaged => &[PythonSource::Managed],
            Self::Managed => {
                if cfg!(windows) {
                    &[
                        PythonSource::Managed,
                        PythonSource::SearchPath,
                        PythonSource::Registry,
                    ]
                } else {
                    &[PythonSource::Managed, PythonSource::SearchPath]
                }
            }
            Self::System => {
                if cfg!(windows) {
                    &[
                        PythonSource::SearchPath,
                        PythonSource::Registry,
                        PythonSource::Managed,
                    ]
                } else {
                    &[PythonSource::SearchPath, PythonSource::Managed]
                }
            }
            Self::OnlySystem => {
                if cfg!(windows) {
                    &[PythonSource::SearchPath, PythonSource::Registry]
                } else {
                    &[PythonSource::SearchPath]
                }
            }
        }
    }
}

impl fmt::Display for PythonPreference {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::OnlyManaged => "only managed",
            Self::Managed => "prefer managed",
            Self::System => "prefer system",
            Self::OnlySystem => "only system",
        })
    }
}

fn try_into_u8_slice(release: &[u64]) -> Result<Vec<u8>, std::num::TryFromIntError> {
    release
        .iter()
        .map(|x| match u8::try_from(*x) {
            Ok(x) => Ok(x),
            Err(e) => Err(e),
        })
        .collect()
}

/// Convert a wheel tag formatted version (e.g., `38`) to multiple components (e.g., `3.8`).
///
/// The major version is always assumed to be a single digit 0-9. The minor version is all
/// the following content.
///
/// If not a wheel tag formatted version, the input is returned unchanged.
fn split_wheel_tag_release_version(version: Version) -> Version {
    let release = version.release();
    if release.len() != 1 {
        return version;
    }

    let release = release[0].to_string();
    let mut chars = release.chars();
    let Some(major) = chars.next().and_then(|c| c.to_digit(10)) else {
        return version;
    };

    let Ok(minor) = chars.as_str().parse::<u32>() else {
        return version;
    };

    version.with_release([u64::from(major), u64::from(minor)])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ArchRequest;
    use assert_fs::{TempDir, prelude::*};
    use std::assert_matches;
    use target_lexicon::{Aarch64Architecture, Architecture};
    use uv_pep440::PrereleaseKind;
    use uv_platform::{Arch, Libc, Os};
    #[test]
    fn interpreter_request_from_str() {
        assert_eq!(PythonRequest::parse("any"), PythonRequest::Any);
        assert_eq!(PythonRequest::parse("default"), PythonRequest::Default);
        assert_eq!(
            PythonRequest::parse("3.12"),
            PythonRequest::Version(VersionRequest::from_str("3.12").unwrap())
        );
        assert_eq!(
            PythonRequest::parse(">=3.12"),
            PythonRequest::Version(VersionRequest::from_str(">=3.12").unwrap())
        );
        assert_eq!(
            PythonRequest::parse(">=3.12,<3.13"),
            PythonRequest::Version(VersionRequest::from_str(">=3.12,<3.13").unwrap())
        );
        assert_eq!(
            PythonRequest::parse(">=3.12,<3.13"),
            PythonRequest::Version(VersionRequest::from_str(">=3.12,<3.13").unwrap())
        );

        assert_eq!(
            PythonRequest::parse("3.13.0a1"),
            PythonRequest::Version(VersionRequest::from_str("3.13.0a1").unwrap())
        );
        assert_eq!(
            PythonRequest::parse("3.13.0b5"),
            PythonRequest::Version(VersionRequest::from_str("3.13.0b5").unwrap())
        );
        assert_eq!(
            PythonRequest::parse("3.13.0rc1"),
            PythonRequest::Version(VersionRequest::from_str("3.13.0rc1").unwrap())
        );
        assert_eq!(
            PythonRequest::parse("3.13.1rc1"),
            PythonRequest::ExecutableName("3.13.1rc1".to_string()),
            "Pre-release version requests require a patch version of zero"
        );
        assert_eq!(
            PythonRequest::parse("3rc1"),
            PythonRequest::ExecutableName("3rc1".to_string()),
            "Pre-release version requests require a minor version"
        );

        assert_eq!(
            PythonRequest::parse("cpython"),
            PythonRequest::Implementation(ImplementationName::CPython)
        );

        assert_eq!(
            PythonRequest::parse("cpython3.12.2"),
            PythonRequest::ImplementationVersion(
                ImplementationName::CPython,
                VersionRequest::from_str("3.12.2").unwrap(),
            )
        );

        assert_eq!(
            PythonRequest::parse("cpython-3.13.2"),
            PythonRequest::Key(PythonDownloadRequest {
                version: Some(VersionRequest::MajorMinorPatch(
                    3,
                    13,
                    2,
                    PythonVariant::Default
                )),
                implementation: Some(ImplementationName::CPython),
                arch: None,
                os: None,
                libc: None,
                build: None,
                prereleases: None
            })
        );
        assert_eq!(
            PythonRequest::parse("cpython-3.13.2-macos-aarch64-none"),
            PythonRequest::Key(PythonDownloadRequest {
                version: Some(VersionRequest::MajorMinorPatch(
                    3,
                    13,
                    2,
                    PythonVariant::Default
                )),
                implementation: Some(ImplementationName::CPython),
                arch: Some(ArchRequest::Explicit(Arch::new(
                    Architecture::Aarch64(Aarch64Architecture::Aarch64),
                    None
                ))),
                os: Some(Os::new(target_lexicon::OperatingSystem::Darwin(None))),
                libc: Some(Libc::None),
                build: None,
                prereleases: None
            })
        );
        assert_eq!(
            PythonRequest::parse("any-3.13.2"),
            PythonRequest::Key(PythonDownloadRequest {
                version: Some(VersionRequest::MajorMinorPatch(
                    3,
                    13,
                    2,
                    PythonVariant::Default
                )),
                implementation: None,
                arch: None,
                os: None,
                libc: None,
                build: None,
                prereleases: None
            })
        );
        assert_eq!(
            PythonRequest::parse("any-3.13.2-any-aarch64"),
            PythonRequest::Key(PythonDownloadRequest {
                version: Some(VersionRequest::MajorMinorPatch(
                    3,
                    13,
                    2,
                    PythonVariant::Default
                )),
                implementation: None,
                arch: Some(ArchRequest::Explicit(Arch::new(
                    Architecture::Aarch64(Aarch64Architecture::Aarch64),
                    None
                ))),
                os: None,
                libc: None,
                build: None,
                prereleases: None
            })
        );

        assert_eq!(
            PythonRequest::parse("pypy"),
            PythonRequest::Implementation(ImplementationName::PyPy)
        );
        assert_eq!(
            PythonRequest::parse("pp"),
            PythonRequest::Implementation(ImplementationName::PyPy)
        );
        assert_eq!(
            PythonRequest::parse("graalpy"),
            PythonRequest::Implementation(ImplementationName::GraalPy)
        );
        assert_eq!(
            PythonRequest::parse("gp"),
            PythonRequest::Implementation(ImplementationName::GraalPy)
        );
        assert_eq!(
            PythonRequest::parse("cp"),
            PythonRequest::Implementation(ImplementationName::CPython)
        );
        assert_eq!(
            PythonRequest::parse("pypy3.10"),
            PythonRequest::ImplementationVersion(
                ImplementationName::PyPy,
                VersionRequest::from_str("3.10").unwrap(),
            )
        );
        assert_eq!(
            PythonRequest::parse("pp310"),
            PythonRequest::ImplementationVersion(
                ImplementationName::PyPy,
                VersionRequest::from_str("3.10").unwrap(),
            )
        );
        assert_eq!(
            PythonRequest::parse("graalpy3.10"),
            PythonRequest::ImplementationVersion(
                ImplementationName::GraalPy,
                VersionRequest::from_str("3.10").unwrap(),
            )
        );
        assert_eq!(
            PythonRequest::parse("gp310"),
            PythonRequest::ImplementationVersion(
                ImplementationName::GraalPy,
                VersionRequest::from_str("3.10").unwrap(),
            )
        );
        assert_eq!(
            PythonRequest::parse("cp38"),
            PythonRequest::ImplementationVersion(
                ImplementationName::CPython,
                VersionRequest::from_str("3.8").unwrap(),
            )
        );
        assert_eq!(
            PythonRequest::parse("pypy@3.10"),
            PythonRequest::ImplementationVersion(
                ImplementationName::PyPy,
                VersionRequest::from_str("3.10").unwrap(),
            )
        );
        assert_eq!(
            PythonRequest::parse("pypy310"),
            PythonRequest::ImplementationVersion(
                ImplementationName::PyPy,
                VersionRequest::from_str("3.10").unwrap(),
            )
        );
        assert_eq!(
            PythonRequest::parse("graalpy@3.10"),
            PythonRequest::ImplementationVersion(
                ImplementationName::GraalPy,
                VersionRequest::from_str("3.10").unwrap(),
            )
        );
        assert_eq!(
            PythonRequest::parse("graalpy310"),
            PythonRequest::ImplementationVersion(
                ImplementationName::GraalPy,
                VersionRequest::from_str("3.10").unwrap(),
            )
        );

        let tempdir = TempDir::new().unwrap();
        assert_eq!(
            PythonRequest::parse(tempdir.path().to_str().unwrap()),
            PythonRequest::Directory(tempdir.path().to_path_buf()),
            "An existing directory is treated as a directory"
        );
        assert_eq!(
            PythonRequest::parse(tempdir.child("foo").path().to_str().unwrap()),
            PythonRequest::File(tempdir.child("foo").path().to_path_buf()),
            "A path that does not exist is treated as a file"
        );
        tempdir.child("bar").touch().unwrap();
        assert_eq!(
            PythonRequest::parse(tempdir.child("bar").path().to_str().unwrap()),
            PythonRequest::File(tempdir.child("bar").path().to_path_buf()),
            "An existing file is treated as a file"
        );
        assert_eq!(
            PythonRequest::parse("./foo"),
            PythonRequest::File(PathBuf::from_str("./foo").unwrap()),
            "A string with a file system separator is treated as a file"
        );
        assert_eq!(
            PythonRequest::parse("3.13t"),
            PythonRequest::Version(VersionRequest::from_str("3.13t").unwrap())
        );
    }

    #[test]
    fn interpreter_request_to_canonical_string() {
        assert_eq!(PythonRequest::Default.to_canonical_string(), "default");
        assert_eq!(PythonRequest::Any.to_canonical_string(), "any");
        assert_eq!(
            PythonRequest::Version(VersionRequest::from_str("3.12").unwrap()).to_canonical_string(),
            "3.12"
        );
        assert_eq!(
            PythonRequest::Version(VersionRequest::from_str(">=3.12").unwrap())
                .to_canonical_string(),
            ">=3.12"
        );
        assert_eq!(
            PythonRequest::Version(VersionRequest::from_str(">=3.12,<3.13").unwrap())
                .to_canonical_string(),
            ">=3.12, <3.13"
        );

        assert_eq!(
            PythonRequest::Version(VersionRequest::from_str("3.13.0a1").unwrap())
                .to_canonical_string(),
            "3.13a1"
        );

        assert_eq!(
            PythonRequest::Version(VersionRequest::from_str("3.13.0b5").unwrap())
                .to_canonical_string(),
            "3.13b5"
        );

        assert_eq!(
            PythonRequest::Version(VersionRequest::from_str("3.13.0rc1").unwrap())
                .to_canonical_string(),
            "3.13rc1"
        );

        assert_eq!(
            PythonRequest::Version(VersionRequest::from_str("313rc4").unwrap())
                .to_canonical_string(),
            "3.13rc4"
        );

        assert_eq!(
            PythonRequest::Version(VersionRequest::from_str("3.14.5rc1").unwrap())
                .to_canonical_string(),
            "3.14.5rc1"
        );

        assert_eq!(
            PythonRequest::ExecutableName("foo".to_string()).to_canonical_string(),
            "foo"
        );
        assert_eq!(
            PythonRequest::Implementation(ImplementationName::CPython).to_canonical_string(),
            "cpython"
        );
        assert_eq!(
            PythonRequest::ImplementationVersion(
                ImplementationName::CPython,
                VersionRequest::from_str("3.12.2").unwrap(),
            )
            .to_canonical_string(),
            "cpython@3.12.2"
        );
        assert_eq!(
            PythonRequest::Implementation(ImplementationName::PyPy).to_canonical_string(),
            "pypy"
        );
        assert_eq!(
            PythonRequest::ImplementationVersion(
                ImplementationName::PyPy,
                VersionRequest::from_str("3.10").unwrap(),
            )
            .to_canonical_string(),
            "pypy@3.10"
        );
        assert_eq!(
            PythonRequest::Implementation(ImplementationName::GraalPy).to_canonical_string(),
            "graalpy"
        );
        assert_eq!(
            PythonRequest::ImplementationVersion(
                ImplementationName::GraalPy,
                VersionRequest::from_str("3.10").unwrap(),
            )
            .to_canonical_string(),
            "graalpy@3.10"
        );

        let tempdir = TempDir::new().unwrap();
        assert_eq!(
            PythonRequest::Directory(tempdir.path().to_path_buf()).to_canonical_string(),
            tempdir.path().to_str().unwrap(),
            "An existing directory is treated as a directory"
        );
        assert_eq!(
            PythonRequest::File(tempdir.child("foo").path().to_path_buf()).to_canonical_string(),
            tempdir.child("foo").path().to_str().unwrap(),
            "A path that does not exist is treated as a file"
        );
        tempdir.child("bar").touch().unwrap();
        assert_eq!(
            PythonRequest::File(tempdir.child("bar").path().to_path_buf()).to_canonical_string(),
            tempdir.child("bar").path().to_str().unwrap(),
            "An existing file is treated as a file"
        );
        assert_eq!(
            PythonRequest::File(PathBuf::from_str("./foo").unwrap()).to_canonical_string(),
            "./foo",
            "A string with a file system separator is treated as a file"
        );
    }

    #[test]
    fn version_request_from_str() {
        assert_eq!(
            VersionRequest::from_str("3").unwrap(),
            VersionRequest::Major(3, PythonVariant::Default)
        );
        assert_eq!(
            VersionRequest::from_str("3.12").unwrap(),
            VersionRequest::MajorMinor(3, 12, PythonVariant::Default)
        );
        assert_eq!(
            VersionRequest::from_str("3.12.1").unwrap(),
            VersionRequest::MajorMinorPatch(3, 12, 1, PythonVariant::Default)
        );
        assert!(VersionRequest::from_str("1.foo.1").is_err());
        assert_eq!(
            VersionRequest::from_str("3").unwrap(),
            VersionRequest::Major(3, PythonVariant::Default)
        );
        assert_eq!(
            VersionRequest::from_str("38").unwrap(),
            VersionRequest::MajorMinor(3, 8, PythonVariant::Default)
        );
        assert_eq!(
            VersionRequest::from_str("312").unwrap(),
            VersionRequest::MajorMinor(3, 12, PythonVariant::Default)
        );
        assert_eq!(
            VersionRequest::from_str("3100").unwrap(),
            VersionRequest::MajorMinor(3, 100, PythonVariant::Default)
        );
        assert_eq!(
            VersionRequest::from_str("3.13a1").unwrap(),
            VersionRequest::MajorMinorPrerelease(
                3,
                13,
                Prerelease {
                    kind: PrereleaseKind::Alpha,
                    number: 1
                },
                PythonVariant::Default
            )
        );
        assert_eq!(
            VersionRequest::from_str("313b1").unwrap(),
            VersionRequest::MajorMinorPrerelease(
                3,
                13,
                Prerelease {
                    kind: PrereleaseKind::Beta,
                    number: 1
                },
                PythonVariant::Default
            )
        );
        assert_eq!(
            VersionRequest::from_str("3.13.0b2").unwrap(),
            VersionRequest::MajorMinorPrerelease(
                3,
                13,
                Prerelease {
                    kind: PrereleaseKind::Beta,
                    number: 2
                },
                PythonVariant::Default
            )
        );
        assert_eq!(
            VersionRequest::from_str("3.13.0rc3").unwrap(),
            VersionRequest::MajorMinorPrerelease(
                3,
                13,
                Prerelease {
                    kind: PrereleaseKind::Rc,
                    number: 3
                },
                PythonVariant::Default
            )
        );
        assert_matches!(
            VersionRequest::from_str("3rc1"),
            Err(PythonRequestError::InvalidVersionRequest(_)),
            "Pre-release version requests require a minor version"
        );
        assert_eq!(
            VersionRequest::from_str("3.14.5rc1").unwrap(),
            VersionRequest::MajorMinorPatchPrerelease(
                3,
                14,
                5,
                Prerelease {
                    kind: PrereleaseKind::Rc,
                    number: 1
                },
                PythonVariant::Default
            ),
            "Pre-release version requests with a non-zero patch are allowed (e.g., `3.14.5rc1`)"
        );
        assert_eq!(
            VersionRequest::from_str("3.13.2rc1").unwrap(),
            VersionRequest::MajorMinorPatchPrerelease(
                3,
                13,
                2,
                Prerelease {
                    kind: PrereleaseKind::Rc,
                    number: 1
                },
                PythonVariant::Default
            )
        );
        assert_matches!(
            VersionRequest::from_str("3.12-dev"),
            Err(PythonRequestError::InvalidVersionRequest(_)),
            "Development version segments are not allowed"
        );
        assert_matches!(
            VersionRequest::from_str("3.12+local"),
            Err(PythonRequestError::InvalidVersionRequest(_)),
            "Local version segments are not allowed"
        );
        assert_matches!(
            VersionRequest::from_str("3.12.post0"),
            Err(PythonRequestError::InvalidVersionRequest(_)),
            "Post version segments are not allowed"
        );
        assert!(
            // Test for overflow
            matches!(
                VersionRequest::from_str("31000"),
                Err(PythonRequestError::InvalidVersionRequest(_))
            )
        );
        assert_eq!(
            VersionRequest::from_str("3t").unwrap(),
            VersionRequest::Major(3, PythonVariant::Freethreaded)
        );
        assert_eq!(
            VersionRequest::from_str("313t").unwrap(),
            VersionRequest::MajorMinor(3, 13, PythonVariant::Freethreaded)
        );
        assert_eq!(
            VersionRequest::from_str("3.13t").unwrap(),
            VersionRequest::MajorMinor(3, 13, PythonVariant::Freethreaded)
        );
        assert_eq!(
            VersionRequest::from_str(">=3.13t").unwrap(),
            VersionRequest::Range(
                VersionSpecifiers::from_str(">=3.13").unwrap(),
                PythonVariant::Freethreaded
            )
        );
        assert_eq!(
            VersionRequest::from_str(">=3.13").unwrap(),
            VersionRequest::Range(
                VersionSpecifiers::from_str(">=3.13").unwrap(),
                PythonVariant::Default
            )
        );
        assert_eq!(
            VersionRequest::from_str(">=3.12,<3.14t").unwrap(),
            VersionRequest::Range(
                VersionSpecifiers::from_str(">=3.12,<3.14").unwrap(),
                PythonVariant::Freethreaded
            )
        );
        assert_matches!(
            VersionRequest::from_str("3.13tt"),
            Err(PythonRequestError::InvalidVersionRequest(_))
        );
        assert_matches!(
            VersionRequest::from_str("3.12²t"),
            Err(PythonRequestError::InvalidVersionRequest(_))
        );

        // `==` specifiers are parsed as concrete version requests via `from_specifiers`
        assert_eq!(
            VersionRequest::from_str("==3.12").unwrap(),
            VersionRequest::MajorMinor(3, 12, PythonVariant::Default)
        );
        assert_eq!(
            VersionRequest::from_str("==3.12.1").unwrap(),
            VersionRequest::MajorMinorPatch(3, 12, 1, PythonVariant::Default)
        );
    }

    #[test]
    fn version_request_from_specifiers() {
        // A single `==` specifier is parsed as a concrete version request
        assert_eq!(
            VersionRequest::from_specifiers(
                VersionSpecifiers::from_str("==3.12").unwrap(),
                PythonVariant::Default
            ),
            VersionRequest::MajorMinor(3, 12, PythonVariant::Default)
        );
        assert_eq!(
            VersionRequest::from_specifiers(
                VersionSpecifiers::from_str("==3.12.1").unwrap(),
                PythonVariant::Default
            ),
            VersionRequest::MajorMinorPatch(3, 12, 1, PythonVariant::Default)
        );

        // Wildcard `==` specifiers remain as ranges
        assert_eq!(
            VersionRequest::from_specifiers(
                VersionSpecifiers::from_str("==3.12.*").unwrap(),
                PythonVariant::Default
            ),
            VersionRequest::Range(
                VersionSpecifiers::from_str("==3.12.*").unwrap(),
                PythonVariant::Default
            )
        );

        // Range specifiers remain as ranges
        assert_eq!(
            VersionRequest::from_specifiers(
                VersionSpecifiers::from_str(">=3.12").unwrap(),
                PythonVariant::Default
            ),
            VersionRequest::Range(
                VersionSpecifiers::from_str(">=3.12").unwrap(),
                PythonVariant::Default
            )
        );

        // Multi-specifier constraints remain as ranges
        assert_eq!(
            VersionRequest::from_specifiers(
                VersionSpecifiers::from_str(">=3.12,<3.14").unwrap(),
                PythonVariant::Default
            ),
            VersionRequest::Range(
                VersionSpecifiers::from_str(">=3.12,<3.14").unwrap(),
                PythonVariant::Default
            )
        );
    }

    #[test]
    fn executable_names_from_request() {
        fn case(request: &str, expected: &[&str]) {
            let (implementation, version) = match PythonRequest::parse(request) {
                PythonRequest::Any => (None, VersionRequest::Any),
                PythonRequest::Default => (None, VersionRequest::Default),
                PythonRequest::Version(version) => (None, version),
                PythonRequest::ImplementationVersion(implementation, version) => {
                    (Some(implementation), version)
                }
                PythonRequest::Implementation(implementation) => {
                    (Some(implementation), VersionRequest::Default)
                }
                result => {
                    panic!("Test cases should request versions or implementations; got {result:?}")
                }
            };

            let result: Vec<_> = version
                .executable_names(implementation.as_ref())
                .into_iter()
                .map(|name| name.to_string())
                .collect();

            let expected: Vec<_> = expected
                .iter()
                .map(|name| format!("{name}{exe}", exe = std::env::consts::EXE_SUFFIX))
                .collect();

            assert_eq!(result, expected, "mismatch for case \"{request}\"");
        }

        case(
            "any",
            &[
                "python", "python3", "cpython", "cpython3", "pypy", "pypy3", "graalpy", "graalpy3",
                "pyodide", "pyodide3",
            ],
        );

        case("default", &["python", "python3"]);

        case("3", &["python3", "python"]);

        case("4", &["python4", "python"]);

        case("3.13", &["python3.13", "python3", "python"]);

        case("pypy", &["pypy", "pypy3", "python", "python3"]);

        case(
            "pypy@3.10",
            &[
                "pypy3.10",
                "pypy3",
                "pypy",
                "python3.10",
                "python3",
                "python",
            ],
        );

        case(
            "3.13t",
            &[
                "python3.13t",
                "python3.13",
                "python3t",
                "python3",
                "pythont",
                "python",
            ],
        );
        case("3t", &["python3t", "python3", "pythont", "python"]);

        case(
            "3.13.2",
            &["python3.13.2", "python3.13", "python3", "python"],
        );

        case(
            "3.13rc2",
            &["python3.13rc2", "python3.13", "python3", "python"],
        );
    }

    #[test]
    fn test_try_split_prefix_and_version() {
        assert_matches!(
            PythonRequest::try_split_prefix_and_version("prefix", "prefix"),
            Ok(None),
        );
        assert_matches!(
            PythonRequest::try_split_prefix_and_version("prefix", "prefix3"),
            Ok(Some(_)),
        );
        assert_matches!(
            PythonRequest::try_split_prefix_and_version("prefix", "prefix@3"),
            Ok(Some(_)),
        );
        assert_matches!(
            PythonRequest::try_split_prefix_and_version("prefix", "prefix3notaversion"),
            Ok(None),
        );
        // Version parsing errors are only raised if @ is present.
        assert!(
            PythonRequest::try_split_prefix_and_version("prefix", "prefix@3notaversion").is_err()
        );
        // @ is not allowed if the prefix is empty.
        assert!(PythonRequest::try_split_prefix_and_version("", "@3").is_err());
    }

    #[test]
    fn version_request_as_pep440_version() {
        // Non-concrete requests return `None`
        assert_eq!(VersionRequest::Default.as_pep440_version(), None);
        assert_eq!(VersionRequest::Any.as_pep440_version(), None);
        assert_eq!(
            VersionRequest::from_str(">=3.10")
                .unwrap()
                .as_pep440_version(),
            None
        );

        // `VersionRequest::Major`
        assert_eq!(
            VersionRequest::Major(3, PythonVariant::Default).as_pep440_version(),
            Some(Version::from_str("3").unwrap())
        );

        // `VersionRequest::MajorMinor`
        assert_eq!(
            VersionRequest::MajorMinor(3, 12, PythonVariant::Default).as_pep440_version(),
            Some(Version::from_str("3.12").unwrap())
        );

        // `VersionRequest::MajorMinorPatch`
        assert_eq!(
            VersionRequest::MajorMinorPatch(3, 12, 5, PythonVariant::Default).as_pep440_version(),
            Some(Version::from_str("3.12.5").unwrap())
        );

        // `VersionRequest::MajorMinorPrerelease`
        assert_eq!(
            VersionRequest::MajorMinorPrerelease(
                3,
                14,
                Prerelease {
                    kind: PrereleaseKind::Alpha,
                    number: 1
                },
                PythonVariant::Default
            )
            .as_pep440_version(),
            Some(Version::from_str("3.14.0a1").unwrap())
        );
        assert_eq!(
            VersionRequest::MajorMinorPrerelease(
                3,
                14,
                Prerelease {
                    kind: PrereleaseKind::Beta,
                    number: 2
                },
                PythonVariant::Default
            )
            .as_pep440_version(),
            Some(Version::from_str("3.14.0b2").unwrap())
        );
        assert_eq!(
            VersionRequest::MajorMinorPrerelease(
                3,
                13,
                Prerelease {
                    kind: PrereleaseKind::Rc,
                    number: 3
                },
                PythonVariant::Default
            )
            .as_pep440_version(),
            Some(Version::from_str("3.13.0rc3").unwrap())
        );

        // Variant is ignored
        assert_eq!(
            VersionRequest::Major(3, PythonVariant::Freethreaded).as_pep440_version(),
            Some(Version::from_str("3").unwrap())
        );
        assert_eq!(
            VersionRequest::MajorMinor(3, 13, PythonVariant::Freethreaded).as_pep440_version(),
            Some(Version::from_str("3.13").unwrap())
        );
    }

    #[test]
    fn python_request_as_pep440_version() {
        // `PythonRequest::Any` and `PythonRequest::Default` return `None`
        assert_eq!(PythonRequest::Any.as_pep440_version(), None);
        assert_eq!(PythonRequest::Default.as_pep440_version(), None);

        // `PythonRequest::Version` delegates to `VersionRequest`
        assert_eq!(
            PythonRequest::Version(VersionRequest::MajorMinor(3, 11, PythonVariant::Default))
                .as_pep440_version(),
            Some(Version::from_str("3.11").unwrap())
        );

        // `PythonRequest::ImplementationVersion` extracts version
        assert_eq!(
            PythonRequest::ImplementationVersion(
                ImplementationName::CPython,
                VersionRequest::MajorMinorPatch(3, 12, 1, PythonVariant::Default),
            )
            .as_pep440_version(),
            Some(Version::from_str("3.12.1").unwrap())
        );

        // `PythonRequest::Implementation` returns `None` (no version)
        assert_eq!(
            PythonRequest::Implementation(ImplementationName::CPython).as_pep440_version(),
            None
        );

        // `PythonRequest::Key` with version
        assert_eq!(
            PythonRequest::parse("cpython-3.13.2").as_pep440_version(),
            Some(Version::from_str("3.13.2").unwrap())
        );

        // `PythonRequest::Key` without version returns `None`
        assert_eq!(
            PythonRequest::parse("cpython-macos-aarch64-none").as_pep440_version(),
            None
        );

        // Range versions return `None`
        assert_eq!(
            PythonRequest::Version(VersionRequest::from_str(">=3.10").unwrap()).as_pep440_version(),
            None
        );
    }

    #[test]
    fn intersects_specifiers_exact() {
        let requires_python = VersionSpecifiers::from_str(">=3.12").unwrap();

        assert!(PythonRequest::parse("3.12").intersects_specifiers(&requires_python));
        assert!(!PythonRequest::parse("3.11").intersects_specifiers(&requires_python));
    }

    #[test]
    fn intersects_specifiers_major() {
        let requires_python = VersionSpecifiers::from_str(">=3.12").unwrap();

        // `3` overlaps with `>=3.12` (e.g., 3.12, 3.13, ... are all Python 3)
        assert!(PythonRequest::parse("3").intersects_specifiers(&requires_python));
        // `2` does not overlap with `>=3.12`
        assert!(!PythonRequest::parse("2").intersects_specifiers(&requires_python));
    }

    #[test]
    fn intersects_specifiers_range() {
        let requires_python = VersionSpecifiers::from_str(">=3.12").unwrap();

        assert!(PythonRequest::parse(">=3.12,<3.13").intersects_specifiers(&requires_python));
        assert!(!PythonRequest::parse(">=3.10,<3.12").intersects_specifiers(&requires_python));
    }

    #[test]
    fn intersects_specifiers_implementation_range() {
        let requires_python = VersionSpecifiers::from_str(">=3.12").unwrap();

        assert!(
            PythonRequest::parse("cpython@>=3.12,<3.13").intersects_specifiers(&requires_python)
        );
        assert!(
            !PythonRequest::parse("cpython@>=3.10,<3.12").intersects_specifiers(&requires_python)
        );
    }

    #[test]
    fn intersects_specifiers_no_version() {
        let requires_python = VersionSpecifiers::from_str(">=3.12").unwrap();

        // Requests without version constraints are always compatible
        assert!(PythonRequest::Any.intersects_specifiers(&requires_python));
        assert!(PythonRequest::Default.intersects_specifiers(&requires_python));
        assert!(
            PythonRequest::Implementation(ImplementationName::CPython)
                .intersects_specifiers(&requires_python)
        );
    }
}
