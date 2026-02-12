use itertools::{Either, Itertools};
use owo_colors::AnsiColors;
use regex::Regex;
use reqwest_retry::policies::ExponentialBackoff;
use rustc_hash::{FxBuildHasher, FxHashSet};
use same_file::is_same_file;
use std::borrow::Cow;
use std::env::consts::EXE_SUFFIX;
use std::fmt::{self, Debug, Formatter};
use std::{env, io, iter};
use std::{path::Path, path::PathBuf, str::FromStr};
use thiserror::Error;
use tracing::{debug, instrument, trace};
use uv_cache::Cache;
use uv_client::BaseClient;
use uv_fs::Simplified;
use uv_fs::which::is_executable;
use uv_pep440::{
    LowerBound, Prerelease, UpperBound, Version, VersionSpecifier, VersionSpecifiers,
    release_specifiers_to_ranges,
};
use uv_preview::Preview;
use uv_static::EnvVars;
use uv_warnings::anstream;
use uv_warnings::warn_user_once;
use which::{which, which_all};

use crate::downloads::{ManagedPythonDownloadList, PlatformRequest, PythonDownloadRequest};
use crate::implementation::ImplementationName;
use crate::installation::{PythonInstallation, PythonInstallationKey};
use crate::interpreter::Error as InterpreterError;
use crate::interpreter::{StatusCodeError, UnexpectedResponseError};
use crate::managed::{ManagedPythonInstallations, PythonMinorVersionLink};
#[cfg(windows)]
use crate::microsoft_store::find_microsoft_store_pythons;
use crate::python_version::python_build_versions_from_env;
use crate::virtualenv::Error as VirtualEnvError;
use crate::virtualenv::{
    CondaEnvironmentKind, conda_environment_from_env, virtualenv_from_env,
    virtualenv_from_working_dir, virtualenv_python_executable,
};
#[cfg(windows)]
use crate::windows_registry::{WindowsPython, registry_pythons};
use crate::{BrokenSymlink, Interpreter, PythonVersion};

/// A request to find a Python installation.
///
/// See [`PythonRequest::from_str`].
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

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct DiscoveryPreferences {
    python_preference: PythonPreference,
    environment_preference: EnvironmentPreference,
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
    Range(VersionSpecifiers, PythonVariant),
}

/// The result of an Python installation search.
///
/// Returned by [`find_python_installation`].
type FindPythonResult = Result<PythonInstallation, PythonNotFound>;

/// The result of failed Python installation discovery.
///
/// See [`FindPythonResult`].
#[derive(Clone, Debug, Error)]
pub struct PythonNotFound {
    pub request: PythonRequest,
    pub python_preference: PythonPreference,
    pub environment_preference: EnvironmentPreference,
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

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] io::Error),

    /// An error was encountering when retrieving interpreter information.
    #[error("Failed to inspect Python interpreter from {} at `{}` ", _2, _1.user_display())]
    Query(
        #[source] Box<crate::interpreter::Error>,
        PathBuf,
        PythonSource,
    ),

    /// An error was encountered while trying to find a managed Python installation matching the
    /// current platform.
    #[error("Failed to discover managed Python installations")]
    ManagedPython(#[from] crate::managed::Error),

    /// An error was encountered when inspecting a virtual environment.
    #[error(transparent)]
    VirtualEnv(#[from] crate::virtualenv::Error),

    #[cfg(windows)]
    #[error("Failed to query installed Python versions from the Windows registry")]
    RegistryError(#[from] windows::core::Error),

    /// An invalid version request was given
    #[error("Invalid version request: {0}")]
    InvalidVersionRequest(String),

    /// The @latest version request was given
    #[error("Requesting the 'latest' Python version is not yet supported")]
    LatestVersionRequest,

    // TODO(zanieb): Is this error case necessary still? We should probably drop it.
    #[error("Interpreter discovery for `{0}` requires `{1}` but only `{2}` is allowed")]
    SourceNotAllowed(PythonRequest, PythonSource, PythonPreference),

    #[error(transparent)]
    BuildVersion(#[from] crate::python_version::BuildVersionError),
}

/// Lazily iterate over Python executables in mutable virtual environments.
///
/// The following sources are supported:
///
/// - Active virtual environment (via `VIRTUAL_ENV`)
/// - Discovered virtual environment (e.g. `.venv` in a parent directory)
///
/// Notably, "system" environments are excluded. See [`python_executables_from_installed`].
fn python_executables_from_virtual_environments<'a>(
    preview: Preview,
) -> impl Iterator<Item = Result<(PythonSource, PathBuf), Error>> + 'a {
    let from_active_environment = iter::once_with(|| {
        virtualenv_from_env()
            .into_iter()
            .map(virtualenv_python_executable)
            .map(|path| Ok((PythonSource::ActiveEnvironment, path)))
    })
    .flatten();

    // N.B. we prefer the conda environment over discovered virtual environments
    let from_conda_environment = iter::once_with(move || {
        conda_environment_from_env(CondaEnvironmentKind::Child, preview)
            .into_iter()
            .map(virtualenv_python_executable)
            .map(|path| Ok((PythonSource::CondaPrefix, path)))
    })
    .flatten();

    let from_discovered_environment = iter::once_with(|| {
        virtualenv_from_working_dir()
            .map(|path| {
                path.map(virtualenv_python_executable)
                    .map(|path| (PythonSource::DiscoveredEnvironment, path))
                    .into_iter()
            })
            .map_err(Error::from)
    })
    .flatten_ok();

    from_active_environment
        .chain(from_conda_environment)
        .chain(from_discovered_environment)
}

/// Lazily iterate over Python executables installed on the system.
///
/// The following sources are supported:
///
/// - Managed Python installations (e.g. `uv python install`)
/// - The search path (i.e. `PATH`)
/// - The registry (Windows only)
///
/// The ordering and presence of each source is determined by the [`PythonPreference`].
///
/// If a [`VersionRequest`] is provided, we will skip executables that we know do not satisfy the request
/// and (as discussed in [`python_executables_from_search_path`]) additional version-specific executables may
/// be included. However, the caller MUST query the returned executables to ensure they satisfy the request;
/// this function does not guarantee that the executables provide any particular version. See
/// [`find_python_installation`] instead.
///
/// This function does not guarantee that the executables are valid Python interpreters.
/// See [`python_interpreters_from_executables`].
fn python_executables_from_installed<'a>(
    version: &'a VersionRequest,
    implementation: Option<&'a ImplementationName>,
    platform: PlatformRequest,
    preference: PythonPreference,
) -> Box<dyn Iterator<Item = Result<(PythonSource, PathBuf), Error>> + 'a> {
    let from_managed_installations = iter::once_with(move || {
        ManagedPythonInstallations::from_settings(None)
            .map_err(Error::from)
            .and_then(|installed_installations| {
                debug!(
                    "Searching for managed installations at `{}`",
                    installed_installations.root().user_display()
                );
                let installations = installed_installations.find_matching_current_platform()?;

                let build_versions = python_build_versions_from_env()?;

                // Check that the Python version and platform satisfy the request to avoid
                // unnecessary interpreter queries later
                Ok(installations
                    .into_iter()
                    .filter(move |installation| {
                        if !version.matches_version(&installation.version()) {
                            debug!("Skipping managed installation `{installation}`: does not satisfy `{version}`");
                            return false;
                        }
                        if !platform.matches(installation.platform()) {
                            debug!("Skipping managed installation `{installation}`: does not satisfy requested platform `{platform}`");
                            return false;
                        }

                        if let Some(requested_build) = build_versions.get(&installation.implementation()) {
                            let Some(installation_build) = installation.build() else {
                                debug!(
                                    "Skipping managed installation `{installation}`: a build version was requested but is not recorded for this installation"
                                );
                                return false;
                            };
                            if installation_build != requested_build {
                                debug!(
                                    "Skipping managed installation `{installation}`: requested build version `{requested_build}` does not match installation build version `{installation_build}`"
                                );
                                return false;
                            }
                        }

                        true
                    })
                    .inspect(|installation| debug!("Found managed installation `{installation}`"))
                    .map(move |installation| {
                        // If it's not a patch version request, then attempt to read the stable
                        // minor version link.
                        let executable = version
                                .patch()
                                .is_none()
                                .then(|| {
                                    PythonMinorVersionLink::from_installation(
                                        &installation,
                                    )
                                    .filter(PythonMinorVersionLink::exists)
                                    .map(
                                        |minor_version_link| {
                                            minor_version_link.symlink_executable.clone()
                                        },
                                    )
                                })
                                .flatten()
                                .unwrap_or_else(|| installation.executable(false));
                        (PythonSource::Managed, executable)
                    })
                )
            })
    })
    .flatten_ok();

    let from_search_path = iter::once_with(move || {
        python_executables_from_search_path(version, implementation)
            .enumerate()
            .map(|(i, path)| {
                if i == 0 {
                    Ok((PythonSource::SearchPathFirst, path))
                } else {
                    Ok((PythonSource::SearchPath, path))
                }
            })
    })
    .flatten();

    let from_windows_registry = iter::once_with(move || {
        #[cfg(windows)]
        {
            // Skip interpreter probing if we already know the version doesn't match.
            let version_filter = move |entry: &WindowsPython| {
                if let Some(found) = &entry.version {
                    // Some distributions emit the patch version (example: `SysVersion: 3.9`)
                    if found.string.chars().filter(|c| *c == '.').count() == 1 {
                        version.matches_major_minor(found.major(), found.minor())
                    } else {
                        version.matches_version(found)
                    }
                } else {
                    true
                }
            };

            env::var_os(EnvVars::UV_TEST_PYTHON_PATH)
                .is_none()
                .then(|| {
                    registry_pythons()
                        .map(|entries| {
                            entries
                                .into_iter()
                                .filter(version_filter)
                                .map(|entry| (PythonSource::Registry, entry.path))
                                .chain(
                                    find_microsoft_store_pythons()
                                        .filter(version_filter)
                                        .map(|entry| (PythonSource::MicrosoftStore, entry.path)),
                                )
                        })
                        .map_err(Error::from)
                })
                .into_iter()
                .flatten_ok()
        }
        #[cfg(not(windows))]
        {
            Vec::new()
        }
    })
    .flatten();

    match preference {
        PythonPreference::OnlyManaged => {
            // TODO(zanieb): Ideally, we'd create "fake" managed installation directories for tests,
            // but for now... we'll just include the test interpreters which are always on the
            // search path.
            if std::env::var(uv_static::EnvVars::UV_INTERNAL__TEST_PYTHON_MANAGED).is_ok() {
                Box::new(from_managed_installations.chain(from_search_path))
            } else {
                Box::new(from_managed_installations)
            }
        }
        PythonPreference::Managed => Box::new(
            from_managed_installations
                .chain(from_search_path)
                .chain(from_windows_registry),
        ),
        PythonPreference::System => Box::new(
            from_search_path
                .chain(from_windows_registry)
                .chain(from_managed_installations),
        ),
        PythonPreference::OnlySystem => Box::new(from_search_path.chain(from_windows_registry)),
    }
}

/// Lazily iterate over all discoverable Python executables.
///
/// Note that Python executables may be excluded by the given [`EnvironmentPreference`],
/// [`PythonPreference`], and [`PlatformRequest`]. However, these filters are only applied for
/// performance. We cannot guarantee that the all requests or preferences are satisfied until we
/// query the interpreter.
///
/// See [`python_executables_from_installed`] and [`python_executables_from_virtual_environments`]
/// for more information on discovery.
fn python_executables<'a>(
    version: &'a VersionRequest,
    implementation: Option<&'a ImplementationName>,
    platform: PlatformRequest,
    environments: EnvironmentPreference,
    preference: PythonPreference,
    preview: Preview,
) -> Box<dyn Iterator<Item = Result<(PythonSource, PathBuf), Error>> + 'a> {
    // Always read from `UV_INTERNAL__PARENT_INTERPRETER` — it could be a system interpreter
    let from_parent_interpreter = iter::once_with(|| {
        env::var_os(EnvVars::UV_INTERNAL__PARENT_INTERPRETER)
            .into_iter()
            .map(|path| Ok((PythonSource::ParentInterpreter, PathBuf::from(path))))
    })
    .flatten();

    // Check if the base conda environment is active
    let from_base_conda_environment = iter::once_with(move || {
        conda_environment_from_env(CondaEnvironmentKind::Base, preview)
            .into_iter()
            .map(virtualenv_python_executable)
            .map(|path| Ok((PythonSource::BaseCondaPrefix, path)))
    })
    .flatten();

    let from_virtual_environments = python_executables_from_virtual_environments(preview);
    let from_installed =
        python_executables_from_installed(version, implementation, platform, preference);

    // Limit the search to the relevant environment preference; this avoids unnecessary work like
    // traversal of the file system. Subsequent filtering should be done by the caller with
    // `source_satisfies_environment_preference` and `interpreter_satisfies_environment_preference`.
    match environments {
        EnvironmentPreference::OnlyVirtual => {
            Box::new(from_parent_interpreter.chain(from_virtual_environments))
        }
        EnvironmentPreference::ExplicitSystem | EnvironmentPreference::Any => Box::new(
            from_parent_interpreter
                .chain(from_virtual_environments)
                .chain(from_base_conda_environment)
                .chain(from_installed),
        ),
        EnvironmentPreference::OnlySystem => Box::new(
            from_parent_interpreter
                .chain(from_base_conda_environment)
                .chain(from_installed),
        ),
    }
}

/// Lazily iterate over Python executables in the `PATH`.
///
/// The [`VersionRequest`] and [`ImplementationName`] are used to determine the possible
/// Python interpreter names, e.g. if looking for Python 3.9 we will look for `python3.9`
/// or if looking for `PyPy` we will look for `pypy` in addition to the default names.
///
/// Executables are returned in the search path order, then by specificity of the name, e.g.
/// `python3.9` is preferred over `python3` and `pypy3.9` is preferred over `python3.9`.
///
/// If a `version` is not provided, we will only look for default executable names e.g.
/// `python3` and `python` — `python3.9` and similar will not be included.
fn python_executables_from_search_path<'a>(
    version: &'a VersionRequest,
    implementation: Option<&'a ImplementationName>,
) -> impl Iterator<Item = PathBuf> + 'a {
    // `UV_TEST_PYTHON_PATH` can be used to override `PATH` to limit Python executable availability in the test suite
    let search_path = env::var_os(EnvVars::UV_TEST_PYTHON_PATH)
        .unwrap_or(env::var_os(EnvVars::PATH).unwrap_or_default());

    let possible_names: Vec<_> = version
        .executable_names(implementation)
        .into_iter()
        .map(|name| name.to_string())
        .collect();

    trace!(
        "Searching PATH for executables: {}",
        possible_names.join(", ")
    );

    // Split and iterate over the paths instead of using `which_all` so we can
    // check multiple names per directory while respecting the search path order and python names
    // precedence.
    let search_dirs: Vec<_> = env::split_paths(&search_path).collect();
    let mut seen_dirs = FxHashSet::with_capacity_and_hasher(search_dirs.len(), FxBuildHasher);
    search_dirs
        .into_iter()
        .filter(|dir| dir.is_dir())
        .flat_map(move |dir| {
            // Clone the directory for second closure
            let dir_clone = dir.clone();
            trace!(
                "Checking `PATH` directory for interpreters: {}",
                dir.display()
            );
            same_file::Handle::from_path(&dir)
                // Skip directories we've already seen, to avoid inspecting interpreters multiple
                // times when directories are repeated or symlinked in the `PATH`
                .map(|handle| seen_dirs.insert(handle))
                .inspect(|fresh_dir| {
                    if !fresh_dir {
                        trace!("Skipping already seen directory: {}", dir.display());
                    }
                })
                // If we cannot determine if the directory is unique, we'll assume it is
                .unwrap_or(true)
                .then(|| {
                    possible_names
                        .clone()
                        .into_iter()
                        .flat_map(move |name| {
                            // Since we're just working with a single directory at a time, we collect to simplify ownership
                            which::which_in_global(&*name, Some(&dir))
                                .into_iter()
                                .flatten()
                                // We have to collect since `which` requires that the regex outlives its
                                // parameters, and the dir is local while we return the iterator.
                                .collect::<Vec<_>>()
                        })
                        .chain(find_all_minor(implementation, version, &dir_clone))
                        .filter(|path| !is_windows_store_shim(path))
                        .inspect(|path| {
                            trace!("Found possible Python executable: {}", path.display());
                        })
                        .chain(
                            // TODO(zanieb): Consider moving `python.bat` into `possible_names` to avoid a chain
                            cfg!(windows)
                                .then(move || {
                                    which::which_in_global("python.bat", Some(&dir_clone))
                                        .into_iter()
                                        .flatten()
                                        .collect::<Vec<_>>()
                                })
                                .into_iter()
                                .flatten(),
                        )
                })
                .into_iter()
                .flatten()
        })
}

/// Find all acceptable `python3.x` minor versions.
///
/// For example, let's say `python` and `python3` are Python 3.10. When a user requests `>= 3.11`,
/// we still need to find a `python3.12` in PATH.
fn find_all_minor(
    implementation: Option<&ImplementationName>,
    version_request: &VersionRequest,
    dir: &Path,
) -> impl Iterator<Item = PathBuf> + use<> {
    match version_request {
        &VersionRequest::Any
        | VersionRequest::Default
        | VersionRequest::Major(_, _)
        | VersionRequest::Range(_, _) => {
            let regex = if let Some(implementation) = implementation {
                Regex::new(&format!(
                    r"^({}|python3)\.(?<minor>\d\d?)t?{}$",
                    regex::escape(&implementation.to_string()),
                    regex::escape(EXE_SUFFIX)
                ))
                .unwrap()
            } else {
                Regex::new(&format!(
                    r"^python3\.(?<minor>\d\d?)t?{}$",
                    regex::escape(EXE_SUFFIX)
                ))
                .unwrap()
            };
            let all_minors = fs_err::read_dir(dir)
                .into_iter()
                .flatten()
                .flatten()
                .map(|entry| entry.path())
                .filter(move |path| {
                    let Some(filename) = path.file_name() else {
                        return false;
                    };
                    let Some(filename) = filename.to_str() else {
                        return false;
                    };
                    let Some(captures) = regex.captures(filename) else {
                        return false;
                    };

                    // Filter out interpreter we already know have a too low minor version.
                    let minor = captures["minor"].parse().ok();
                    if let Some(minor) = minor {
                        // Optimization: Skip generally unsupported Python versions without querying.
                        if minor < 7 {
                            return false;
                        }
                        // Optimization 2: Skip excluded Python (minor) versions without querying.
                        if !version_request.matches_major_minor(3, minor) {
                            return false;
                        }
                    }
                    true
                })
                .filter(|path| is_executable(path))
                .collect::<Vec<_>>();
            Either::Left(all_minors.into_iter())
        }
        VersionRequest::MajorMinor(_, _, _)
        | VersionRequest::MajorMinorPatch(_, _, _, _)
        | VersionRequest::MajorMinorPrerelease(_, _, _, _) => Either::Right(iter::empty()),
    }
}

/// Lazily iterate over all discoverable Python interpreters.
///
/// Note interpreters may be excluded by the given [`EnvironmentPreference`], [`PythonPreference`],
/// [`VersionRequest`], or [`PlatformRequest`].
///
/// The [`PlatformRequest`] is currently only applied to managed Python installations before querying
/// the interpreter. The caller is responsible for ensuring it is applied otherwise.
///
/// See [`python_executables`] for more information on discovery.
fn python_interpreters<'a>(
    version: &'a VersionRequest,
    implementation: Option<&'a ImplementationName>,
    platform: PlatformRequest,
    environments: EnvironmentPreference,
    preference: PythonPreference,
    cache: &'a Cache,
    preview: Preview,
) -> impl Iterator<Item = Result<(PythonSource, Interpreter), Error>> + 'a {
    let interpreters = python_interpreters_from_executables(
        // Perform filtering on the discovered executables based on their source. This avoids
        // unnecessary interpreter queries, which are generally expensive. We'll filter again
        // with `interpreter_satisfies_environment_preference` after querying.
        python_executables(
            version,
            implementation,
            platform,
            environments,
            preference,
            preview,
        )
        .filter_ok(move |(source, path)| {
            source_satisfies_environment_preference(*source, path, environments)
        }),
        cache,
    )
    .filter_ok(move |(source, interpreter)| {
        interpreter_satisfies_environment_preference(*source, interpreter, environments)
    })
    .filter_ok(move |(source, interpreter)| {
        let request = version.clone().into_request_for_source(*source);
        if request.matches_interpreter(interpreter) {
            true
        } else {
            debug!(
                "Skipping interpreter at `{}` from {source}: does not satisfy request `{request}`",
                interpreter.sys_executable().user_display()
            );
            false
        }
    })
    .filter_ok(move |(source, interpreter)| {
        satisfies_python_preference(*source, interpreter, preference)
    });

    if std::env::var(uv_static::EnvVars::UV_INTERNAL__TEST_PYTHON_MANAGED).is_ok() {
        Either::Left(interpreters.map_ok(|(source, interpreter)| {
            // In test mode, change the source to `Managed` if a version was marked as such via
            // `TestContext::with_versions_as_managed`.
            if interpreter.is_managed() {
                (PythonSource::Managed, interpreter)
            } else {
                (source, interpreter)
            }
        }))
    } else {
        Either::Right(interpreters)
    }
}

/// Lazily convert Python executables into interpreters.
fn python_interpreters_from_executables<'a>(
    executables: impl Iterator<Item = Result<(PythonSource, PathBuf), Error>> + 'a,
    cache: &'a Cache,
) -> impl Iterator<Item = Result<(PythonSource, Interpreter), Error>> + 'a {
    executables.map(|result| match result {
        Ok((source, path)) => Interpreter::query(&path, cache)
            .map(|interpreter| (source, interpreter))
            .inspect(|(source, interpreter)| {
                debug!(
                    "Found `{}` at `{}` ({source})",
                    interpreter.key(),
                    path.display()
                );
            })
            .map_err(|err| Error::Query(Box::new(err), path, source))
            .inspect_err(|err| debug!("{err}")),
        Err(err) => Err(err),
    })
}

/// Whether a [`Interpreter`] matches the [`EnvironmentPreference`].
///
/// This is the correct way to determine if an interpreter matches the preference. In contrast,
/// [`source_satisfies_environment_preference`] only checks if a [`PythonSource`] **could** satisfy
/// preference as a pre-filtering step. We cannot definitively know if a Python interpreter is in
/// a virtual environment until we query it.
fn interpreter_satisfies_environment_preference(
    source: PythonSource,
    interpreter: &Interpreter,
    preference: EnvironmentPreference,
) -> bool {
    match (
        preference,
        // Conda environments are not conformant virtual environments but we treat them as such.
        interpreter.is_virtualenv() || (matches!(source, PythonSource::CondaPrefix)),
    ) {
        (EnvironmentPreference::Any, _) => true,
        (EnvironmentPreference::OnlyVirtual, true) => true,
        (EnvironmentPreference::OnlyVirtual, false) => {
            debug!(
                "Ignoring Python interpreter at `{}`: only virtual environments allowed",
                interpreter.sys_executable().display()
            );
            false
        }
        (EnvironmentPreference::ExplicitSystem, true) => true,
        (EnvironmentPreference::ExplicitSystem, false) => {
            if matches!(
                source,
                PythonSource::ProvidedPath | PythonSource::ParentInterpreter
            ) {
                debug!(
                    "Allowing explicitly requested system Python interpreter at `{}`",
                    interpreter.sys_executable().display()
                );
                true
            } else {
                debug!(
                    "Ignoring Python interpreter at `{}`: system interpreter not explicitly requested",
                    interpreter.sys_executable().display()
                );
                false
            }
        }
        (EnvironmentPreference::OnlySystem, true) => {
            debug!(
                "Ignoring Python interpreter at `{}`: system interpreter required",
                interpreter.sys_executable().display()
            );
            false
        }
        (EnvironmentPreference::OnlySystem, false) => true,
    }
}

/// Returns true if a [`PythonSource`] could satisfy the [`EnvironmentPreference`].
///
/// This is useful as a pre-filtering step. Use of [`interpreter_satisfies_environment_preference`]
/// is required to determine if an [`Interpreter`] satisfies the preference.
///
/// The interpreter path is only used for debug messages.
fn source_satisfies_environment_preference(
    source: PythonSource,
    interpreter_path: &Path,
    preference: EnvironmentPreference,
) -> bool {
    match preference {
        EnvironmentPreference::Any => true,
        EnvironmentPreference::OnlyVirtual => {
            if source.is_maybe_virtualenv() {
                true
            } else {
                debug!(
                    "Ignoring Python interpreter at `{}`: only virtual environments allowed",
                    interpreter_path.display()
                );
                false
            }
        }
        EnvironmentPreference::ExplicitSystem => {
            if source.is_maybe_virtualenv() {
                true
            } else {
                debug!(
                    "Ignoring Python interpreter at `{}`: system interpreter not explicitly requested",
                    interpreter_path.display()
                );
                false
            }
        }
        EnvironmentPreference::OnlySystem => {
            if source.is_maybe_system() {
                true
            } else {
                debug!(
                    "Ignoring Python interpreter at `{}`: system interpreter required",
                    interpreter_path.display()
                );
                false
            }
        }
    }
}

/// Returns true if a Python interpreter matches the [`PythonPreference`].
pub fn satisfies_python_preference(
    source: PythonSource,
    interpreter: &Interpreter,
    preference: PythonPreference,
) -> bool {
    // If the source is "explicit", we will not apply the Python preference, e.g., if the user has
    // activated a virtual environment, we should always allow it. We may want to invalidate the
    // environment in some cases, like in projects, but we can't distinguish between explicit
    // requests for a different Python preference or a persistent preference in a configuration file
    // which would result in overly aggressive invalidation.
    let is_explicit = match source {
        PythonSource::ProvidedPath
        | PythonSource::ParentInterpreter
        | PythonSource::ActiveEnvironment
        | PythonSource::CondaPrefix => true,
        PythonSource::Managed
        | PythonSource::DiscoveredEnvironment
        | PythonSource::SearchPath
        | PythonSource::SearchPathFirst
        | PythonSource::Registry
        | PythonSource::MicrosoftStore
        | PythonSource::BaseCondaPrefix => false,
    };

    match preference {
        PythonPreference::OnlyManaged => {
            // Perform a fast check using the source before querying the interpreter
            if matches!(source, PythonSource::Managed) || interpreter.is_managed() {
                true
            } else {
                if is_explicit {
                    debug!(
                        "Allowing unmanaged Python interpreter at `{}` (in conflict with the `python-preference`) since it is from source: {source}",
                        interpreter.sys_executable().display()
                    );
                    true
                } else {
                    debug!(
                        "Ignoring Python interpreter at `{}`: only managed interpreters allowed",
                        interpreter.sys_executable().display()
                    );
                    false
                }
            }
        }
        // If not "only" a kind, any interpreter is okay
        PythonPreference::Managed | PythonPreference::System => true,
        PythonPreference::OnlySystem => {
            if is_system_interpreter(source, interpreter) {
                true
            } else {
                if is_explicit {
                    debug!(
                        "Allowing managed Python interpreter at `{}` (in conflict with the `python-preference`) since it is from source: {source}",
                        interpreter.sys_executable().display()
                    );
                    true
                } else {
                    debug!(
                        "Ignoring Python interpreter at `{}`: only system interpreters allowed",
                        interpreter.sys_executable().display()
                    );
                    false
                }
            }
        }
    }
}

pub(crate) fn is_system_interpreter(source: PythonSource, interpreter: &Interpreter) -> bool {
    match source {
        // A managed interpreter is never a system interpreter
        PythonSource::Managed => false,
        // We can't be sure if this is a system interpreter without checking
        PythonSource::ProvidedPath
        | PythonSource::ParentInterpreter
        | PythonSource::ActiveEnvironment
        | PythonSource::CondaPrefix
        | PythonSource::DiscoveredEnvironment
        | PythonSource::SearchPath
        | PythonSource::SearchPathFirst
        | PythonSource::Registry
        | PythonSource::BaseCondaPrefix => !interpreter.is_managed(),
        // Managed interpreters should never be found in the store
        PythonSource::MicrosoftStore => true,
    }
}

/// Check if an encountered error is critical and should stop discovery.
///
/// Returns false when an error could be due to a faulty Python installation and we should continue searching for a working one.
impl Error {
    pub fn is_critical(&self) -> bool {
        match self {
            // When querying the Python interpreter fails, we will only raise errors that demonstrate that something is broken
            // If the Python interpreter returned a bad response, we'll continue searching for one that works
            Self::Query(err, _, source) => match &**err {
                InterpreterError::Encode(_)
                | InterpreterError::Io(_)
                | InterpreterError::SpawnFailed { .. } => true,
                InterpreterError::UnexpectedResponse(UnexpectedResponseError { path, .. })
                | InterpreterError::StatusCode(StatusCodeError { path, .. }) => {
                    debug!(
                        "Skipping bad interpreter at {} from {source}: {err}",
                        path.display()
                    );
                    false
                }
                InterpreterError::QueryScript { path, err } => {
                    debug!(
                        "Skipping bad interpreter at {} from {source}: {err}",
                        path.display()
                    );
                    false
                }
                #[cfg(windows)]
                InterpreterError::CorruptWindowsPackage { path, err } => {
                    debug!(
                        "Skipping bad interpreter at {} from {source}: {err}",
                        path.display()
                    );
                    false
                }
                InterpreterError::PermissionDenied { path, err } => {
                    debug!(
                        "Skipping unexecutable interpreter at {} from {source}: {err}",
                        path.display()
                    );
                    false
                }
                InterpreterError::NotFound(path)
                | InterpreterError::BrokenSymlink(BrokenSymlink { path, .. }) => {
                    // If the interpreter is from an active, valid virtual environment, we should
                    // fail because it's broken
                    if matches!(source, PythonSource::ActiveEnvironment)
                        && uv_fs::is_virtualenv_executable(path)
                    {
                        true
                    } else {
                        trace!("Skipping missing interpreter at {}", path.display());
                        false
                    }
                }
            },
            Self::VirtualEnv(VirtualEnvError::MissingPyVenvCfg(path)) => {
                trace!("Skipping broken virtualenv at {}", path.display());
                false
            }
            _ => true,
        }
    }
}

/// Create a [`PythonInstallation`] from a Python interpreter path.
fn python_installation_from_executable(
    path: &PathBuf,
    cache: &Cache,
) -> Result<PythonInstallation, crate::interpreter::Error> {
    Ok(PythonInstallation {
        source: PythonSource::ProvidedPath,
        interpreter: Interpreter::query(path, cache)?,
    })
}

/// Create a [`PythonInstallation`] from a Python installation root directory.
fn python_installation_from_directory(
    path: &PathBuf,
    cache: &Cache,
) -> Result<PythonInstallation, crate::interpreter::Error> {
    let executable = virtualenv_python_executable(path);
    python_installation_from_executable(&executable, cache)
}

/// Lazily iterate over all Python interpreters on the path with the given executable name.
fn python_interpreters_with_executable_name<'a>(
    name: &'a str,
    cache: &'a Cache,
) -> impl Iterator<Item = Result<(PythonSource, Interpreter), Error>> + 'a {
    python_interpreters_from_executables(
        which_all(name)
            .into_iter()
            .flat_map(|inner| inner.map(|path| Ok((PythonSource::SearchPath, path)))),
        cache,
    )
}

/// Iterate over all Python installations that satisfy the given request.
pub fn find_python_installations<'a>(
    request: &'a PythonRequest,
    environments: EnvironmentPreference,
    preference: PythonPreference,
    cache: &'a Cache,
    preview: Preview,
) -> Box<dyn Iterator<Item = Result<FindPythonResult, Error>> + 'a> {
    let sources = DiscoveryPreferences {
        python_preference: preference,
        environment_preference: environments,
    }
    .sources(request);

    match request {
        PythonRequest::File(path) => Box::new(iter::once({
            if preference.allows(PythonSource::ProvidedPath) {
                debug!("Checking for Python interpreter at {request}");
                match python_installation_from_executable(path, cache) {
                    Ok(installation) => Ok(Ok(installation)),
                    Err(InterpreterError::NotFound(_) | InterpreterError::BrokenSymlink(_)) => {
                        Ok(Err(PythonNotFound {
                            request: request.clone(),
                            python_preference: preference,
                            environment_preference: environments,
                        }))
                    }
                    Err(err) => Err(Error::Query(
                        Box::new(err),
                        path.clone(),
                        PythonSource::ProvidedPath,
                    )),
                }
            } else {
                Err(Error::SourceNotAllowed(
                    request.clone(),
                    PythonSource::ProvidedPath,
                    preference,
                ))
            }
        })),
        PythonRequest::Directory(path) => Box::new(iter::once({
            if preference.allows(PythonSource::ProvidedPath) {
                debug!("Checking for Python interpreter in {request}");
                match python_installation_from_directory(path, cache) {
                    Ok(installation) => Ok(Ok(installation)),
                    Err(InterpreterError::NotFound(_) | InterpreterError::BrokenSymlink(_)) => {
                        Ok(Err(PythonNotFound {
                            request: request.clone(),
                            python_preference: preference,
                            environment_preference: environments,
                        }))
                    }
                    Err(err) => Err(Error::Query(
                        Box::new(err),
                        path.clone(),
                        PythonSource::ProvidedPath,
                    )),
                }
            } else {
                Err(Error::SourceNotAllowed(
                    request.clone(),
                    PythonSource::ProvidedPath,
                    preference,
                ))
            }
        })),
        PythonRequest::ExecutableName(name) => {
            if preference.allows(PythonSource::SearchPath) {
                debug!("Searching for Python interpreter with {request}");
                Box::new(
                    python_interpreters_with_executable_name(name, cache)
                        .filter_ok(move |(source, interpreter)| {
                            interpreter_satisfies_environment_preference(
                                *source,
                                interpreter,
                                environments,
                            )
                        })
                        .map_ok(|tuple| Ok(PythonInstallation::from_tuple(tuple))),
                )
            } else {
                Box::new(iter::once(Err(Error::SourceNotAllowed(
                    request.clone(),
                    PythonSource::SearchPath,
                    preference,
                ))))
            }
        }
        PythonRequest::Any => Box::new({
            debug!("Searching for any Python interpreter in {sources}");
            python_interpreters(
                &VersionRequest::Any,
                None,
                PlatformRequest::default(),
                environments,
                preference,
                cache,
                preview,
            )
            .map_ok(|tuple| Ok(PythonInstallation::from_tuple(tuple)))
        }),
        PythonRequest::Default => Box::new({
            debug!("Searching for default Python interpreter in {sources}");
            python_interpreters(
                &VersionRequest::Default,
                None,
                PlatformRequest::default(),
                environments,
                preference,
                cache,
                preview,
            )
            .map_ok(|tuple| Ok(PythonInstallation::from_tuple(tuple)))
        }),
        PythonRequest::Version(version) => {
            if let Err(err) = version.check_supported() {
                return Box::new(iter::once(Err(Error::InvalidVersionRequest(err))));
            }
            Box::new({
                debug!("Searching for {request} in {sources}");
                python_interpreters(
                    version,
                    None,
                    PlatformRequest::default(),
                    environments,
                    preference,
                    cache,
                    preview,
                )
                .map_ok(|tuple| Ok(PythonInstallation::from_tuple(tuple)))
            })
        }
        PythonRequest::Implementation(implementation) => Box::new({
            debug!("Searching for a {request} interpreter in {sources}");
            python_interpreters(
                &VersionRequest::Default,
                Some(implementation),
                PlatformRequest::default(),
                environments,
                preference,
                cache,
                preview,
            )
            .filter_ok(|(_source, interpreter)| implementation.matches_interpreter(interpreter))
            .map_ok(|tuple| Ok(PythonInstallation::from_tuple(tuple)))
        }),
        PythonRequest::ImplementationVersion(implementation, version) => {
            if let Err(err) = version.check_supported() {
                return Box::new(iter::once(Err(Error::InvalidVersionRequest(err))));
            }
            Box::new({
                debug!("Searching for {request} in {sources}");
                python_interpreters(
                    version,
                    Some(implementation),
                    PlatformRequest::default(),
                    environments,
                    preference,
                    cache,
                    preview,
                )
                .filter_ok(|(_source, interpreter)| implementation.matches_interpreter(interpreter))
                .map_ok(|tuple| Ok(PythonInstallation::from_tuple(tuple)))
            })
        }
        PythonRequest::Key(request) => {
            if let Some(version) = request.version() {
                if let Err(err) = version.check_supported() {
                    return Box::new(iter::once(Err(Error::InvalidVersionRequest(err))));
                }
            }

            Box::new({
                debug!("Searching for {request} in {sources}");
                python_interpreters(
                    request.version().unwrap_or(&VersionRequest::Default),
                    request.implementation(),
                    request.platform(),
                    environments,
                    preference,
                    cache,
                    preview,
                )
                .filter_ok(move |(_source, interpreter)| {
                    request.satisfied_by_interpreter(interpreter)
                })
                .map_ok(|tuple| Ok(PythonInstallation::from_tuple(tuple)))
            })
        }
    }
}

/// Find a Python installation that satisfies the given request.
///
/// If an error is encountered while locating or inspecting a candidate installation,
/// the error will raised instead of attempting further candidates.
pub(crate) fn find_python_installation(
    request: &PythonRequest,
    environments: EnvironmentPreference,
    preference: PythonPreference,
    cache: &Cache,
    preview: Preview,
) -> Result<FindPythonResult, Error> {
    let installations =
        find_python_installations(request, environments, preference, cache, preview);
    let mut first_prerelease = None;
    let mut first_debug = None;
    let mut first_managed = None;
    let mut first_error = None;
    for result in installations {
        // Iterate until the first critical error or happy result
        if !result.as_ref().err().is_none_or(Error::is_critical) {
            // Track the first non-critical error
            if first_error.is_none() {
                if let Err(err) = result {
                    first_error = Some(err);
                }
            }
            continue;
        }

        // If it's an error, we're done.
        let Ok(Ok(ref installation)) = result else {
            return result;
        };

        // Check if we need to skip the interpreter because it is "not allowed", e.g., if it is a
        // pre-release version or an alternative implementation, using it requires opt-in.

        // If the interpreter has a default executable name, e.g. `python`, and was found on the
        // search path, we consider this opt-in to use it.
        let has_default_executable_name = installation.interpreter.has_default_executable_name()
            && matches!(
                installation.source,
                PythonSource::SearchPath | PythonSource::SearchPathFirst
            );

        // If it's a pre-release and pre-releases aren't allowed, skip it — but store it for later
        // since we'll use a pre-release if no other versions are available.
        if installation.python_version().pre().is_some()
            && !request.allows_prereleases()
            && !installation.source.allows_prereleases()
            && !has_default_executable_name
        {
            debug!("Skipping pre-release installation {}", installation.key());
            if first_prerelease.is_none() {
                first_prerelease = Some(installation.clone());
            }
            continue;
        }

        // If it's a debug build and debug builds aren't allowed, skip it — but store it for later
        // since we'll use a debug build if no other versions are available.
        if installation.key().variant().is_debug()
            && !request.allows_debug()
            && !installation.source.allows_debug()
            && !has_default_executable_name
        {
            debug!("Skipping debug installation {}", installation.key());
            if first_debug.is_none() {
                first_debug = Some(installation.clone());
            }
            continue;
        }

        // If it's an alternative implementation and alternative implementations aren't allowed,
        // skip it. Note we avoid querying these interpreters at all if they're on the search path
        // and are not requested, but other sources such as the managed installations can include
        // them.
        if installation.is_alternative_implementation()
            && !request.allows_alternative_implementations()
            && !installation.source.allows_alternative_implementations()
            && !has_default_executable_name
        {
            debug!("Skipping alternative implementation {}", installation.key());
            continue;
        }

        // If it's a managed Python installation, and system interpreters are preferred, skip it
        // for now.
        if matches!(preference, PythonPreference::System)
            && !is_system_interpreter(installation.source, installation.interpreter())
        {
            debug!(
                "Skipping managed installation {}: system installation preferred",
                installation.key()
            );
            if first_managed.is_none() {
                first_managed = Some(installation.clone());
            }
            continue;
        }

        // If we didn't skip it, this is the installation to use
        return result;
    }

    // If we only found managed installations, and the preference allows them, we should return
    // the first one.
    if let Some(installation) = first_managed {
        debug!(
            "Allowing managed installation {}: no system installations",
            installation.key()
        );
        return Ok(Ok(installation));
    }

    // If we only found debug installations, they're implicitly allowed and we should return the
    // first one.
    if let Some(installation) = first_debug {
        debug!(
            "Allowing debug installation {}: no non-debug installations",
            installation.key()
        );
        return Ok(Ok(installation));
    }

    // If we only found pre-releases, they're implicitly allowed and we should return the first one.
    if let Some(installation) = first_prerelease {
        debug!(
            "Allowing pre-release installation {}: no stable installations",
            installation.key()
        );
        return Ok(Ok(installation));
    }

    // If we found a Python, but it was unusable for some reason, report that instead of saying we
    // couldn't find any Python interpreters.
    if let Some(err) = first_error {
        return Err(err);
    }

    Ok(Err(PythonNotFound {
        request: request.clone(),
        environment_preference: environments,
        python_preference: preference,
    }))
}

/// Find the best-matching Python installation.
///
/// If no Python version is provided, we will use the first available installation.
///
/// If a Python version is provided, we will first try to find an exact match. If
/// that cannot be found and a patch version was requested, we will look for a match
/// without comparing the patch version number. If that cannot be found, we fall back to
/// the first available version.
///
/// At all points, if the specified version cannot be found, we will attempt to
/// download it if downloads are enabled.
///
/// See [`find_python_installation`] for more details on installation discovery.
#[instrument(skip_all, fields(request))]
pub(crate) async fn find_best_python_installation(
    request: &PythonRequest,
    environments: EnvironmentPreference,
    preference: PythonPreference,
    downloads_enabled: bool,
    download_list: &ManagedPythonDownloadList,
    client: &BaseClient,
    retry_policy: &ExponentialBackoff,
    cache: &Cache,
    reporter: Option<&dyn crate::downloads::Reporter>,
    python_install_mirror: Option<&str>,
    pypy_install_mirror: Option<&str>,
    preview: Preview,
) -> Result<PythonInstallation, crate::Error> {
    debug!("Starting Python discovery for {request}");
    let original_request = request;

    let mut previous_fetch_failed = false;

    let request_without_patch = match request {
        PythonRequest::Version(version) => {
            if version.has_patch() {
                Some(PythonRequest::Version(version.clone().without_patch()))
            } else {
                None
            }
        }
        PythonRequest::ImplementationVersion(implementation, version) => Some(
            PythonRequest::ImplementationVersion(*implementation, version.clone().without_patch()),
        ),
        _ => None,
    };

    for (attempt, request) in iter::once(original_request)
        .chain(request_without_patch.iter())
        .chain(iter::once(&PythonRequest::Default))
        .enumerate()
    {
        debug!(
            "Looking for {request}{}",
            if request != original_request {
                format!(" attempt {attempt} (fallback after failing to find: {original_request})")
            } else {
                String::new()
            }
        );
        let result = find_python_installation(request, environments, preference, cache, preview);
        let error = match result {
            Ok(Ok(installation)) => {
                warn_on_unsupported_python(installation.interpreter());
                return Ok(installation);
            }
            // Continue if we can't find a matching Python and ignore non-critical discovery errors
            Ok(Err(error)) => error.into(),
            Err(error) if !error.is_critical() => error.into(),
            Err(error) => return Err(error.into()),
        };

        // Attempt to download the version if downloads are enabled
        if downloads_enabled
            && !previous_fetch_failed
            && let Some(download_request) = PythonDownloadRequest::from_request(request)
        {
            let download = download_request
                .clone()
                .fill()
                .map(|request| download_list.find(&request));

            let result = match download {
                Ok(Ok(download)) => PythonInstallation::fetch(
                    download,
                    client,
                    retry_policy,
                    cache,
                    reporter,
                    python_install_mirror,
                    pypy_install_mirror,
                )
                .await
                .map(Some),
                Ok(Err(crate::downloads::Error::NoDownloadFound(_))) => Ok(None),
                Ok(Err(error)) => Err(error.into()),
                Err(error) => Err(error.into()),
            };
            if let Ok(Some(installation)) = result {
                return Ok(installation);
            }
            // Emit a warning instead of failing since we may find a suitable
            // interpreter on the system after relaxing the request further.
            // Additionally, uv did not previously attempt downloads in this
            // code path and we want to minimize the fatal cases for
            // backwards compatibility.
            // Errors encountered here are either network errors or quirky
            // configuration problems.
            if let Err(error) = result {
                // This is a hack to get `write_error_chain` to format things the way we want.
                #[derive(Debug, thiserror::Error)]
                #[error(
                    "A managed Python download is available for {0}, but an error occurred when attempting to download it."
                )]
                struct WrappedError<'a>(&'a PythonRequest, #[source] crate::Error);

                // If the request was for the default or any version, propagate
                // the error as nothing else we are about to do will help the
                // situation.
                if matches!(request, PythonRequest::Default | PythonRequest::Any) {
                    return Err(error);
                }

                let mut error_chain = String::new();
                // Writing to a string can't fail with errors (panics on allocation failure)
                uv_warnings::write_error_chain(
                    &WrappedError(request, error),
                    &mut error_chain,
                    "warning",
                    AnsiColors::Yellow,
                )
                .unwrap();
                anstream::eprint!("{}", error_chain);
                previous_fetch_failed = true;
            }
        }

        // If this was a request for the Default or Any version, this means that
        // either that's what we were called with, or we're on the last
        // iteration.
        //
        // The most recent find error therefore becomes a fatal one.
        if matches!(request, PythonRequest::Default | PythonRequest::Any) {
            return Err(match error {
                crate::Error::MissingPython(err, _) => PythonNotFound {
                    // Use a more general error in this case since we looked for multiple versions
                    request: original_request.clone(),
                    python_preference: err.python_preference,
                    environment_preference: err.environment_preference,
                }
                .into(),
                other => other,
            });
        }
    }

    unreachable!("The loop should have terminated when it reached PythonRequest::Default");
}

/// Display a warning if the Python version of the [`Interpreter`] is unsupported by uv.
fn warn_on_unsupported_python(interpreter: &Interpreter) {
    // Warn on usage with an unsupported Python version
    if interpreter.python_tuple() < (3, 8) {
        warn_user_once!(
            "uv is only compatible with Python >=3.8, found Python {}",
            interpreter.python_version()
        );
    }
}

/// On Windows we might encounter the Windows Store proxy shim (enabled in:
/// Settings/Apps/Advanced app settings/App execution aliases). When Python is _not_ installed
/// via the Windows Store, but the proxy shim is enabled, then executing `python.exe` or
/// `python3.exe` will redirect to the Windows Store installer.
///
/// We need to detect that these `python.exe` and `python3.exe` files are _not_ Python
/// executables.
///
/// This method is taken from Rye:
///
/// > This is a pretty dumb way.  We know how to parse this reparse point, but Microsoft
/// > does not want us to do this as the format is unstable.  So this is a best effort way.
/// > we just hope that the reparse point has the python redirector in it, when it's not
/// > pointing to a valid Python.
///
/// See: <https://github.com/astral-sh/rye/blob/b0e9eccf05fe4ff0ae7b0250a248c54f2d780b4d/rye/src/cli/shim.rs#L108>
#[cfg(windows)]
pub(crate) fn is_windows_store_shim(path: &Path) -> bool {
    use std::os::windows::fs::MetadataExt;
    use std::os::windows::prelude::OsStrExt;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS,
        FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_MODE, MAXIMUM_REPARSE_DATA_BUFFER_SIZE,
        OPEN_EXISTING,
    };
    use windows::Win32::System::IO::DeviceIoControl;
    use windows::Win32::System::Ioctl::FSCTL_GET_REPARSE_POINT;
    use windows::core::PCWSTR;

    // The path must be absolute.
    if !path.is_absolute() {
        return false;
    }

    // The path must point to something like:
    //   `C:\Users\crmar\AppData\Local\Microsoft\WindowsApps\python3.exe`
    let mut components = path.components().rev();

    // Ex) `python.exe`, `python3.exe`, `python3.12.exe`, etc.
    if !components
        .next()
        .and_then(|component| component.as_os_str().to_str())
        .is_some_and(|component| {
            component.starts_with("python")
                && std::path::Path::new(component)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
        })
    {
        return false;
    }

    // Ex) `WindowsApps`
    if components
        .next()
        .is_none_or(|component| component.as_os_str() != "WindowsApps")
    {
        return false;
    }

    // Ex) `Microsoft`
    if components
        .next()
        .is_none_or(|component| component.as_os_str() != "Microsoft")
    {
        return false;
    }

    // The file is only relevant if it's a reparse point.
    let Ok(md) = fs_err::symlink_metadata(path) else {
        return false;
    };
    if md.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 == 0 {
        return false;
    }

    let mut path_encoded = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();

    // SAFETY: The path is null-terminated.
    #[allow(unsafe_code)]
    let reparse_handle = unsafe {
        CreateFileW(
            PCWSTR(path_encoded.as_mut_ptr()),
            0,
            FILE_SHARE_MODE(0),
            None,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            None,
        )
    };

    let Ok(reparse_handle) = reparse_handle else {
        return false;
    };

    let mut buf = [0u16; MAXIMUM_REPARSE_DATA_BUFFER_SIZE as usize];
    let mut bytes_returned = 0;

    // SAFETY: The buffer is large enough to hold the reparse point.
    #[allow(unsafe_code, clippy::cast_possible_truncation)]
    let success = unsafe {
        DeviceIoControl(
            reparse_handle,
            FSCTL_GET_REPARSE_POINT,
            None,
            0,
            Some(buf.as_mut_ptr().cast()),
            buf.len() as u32 * 2,
            Some(&raw mut bytes_returned),
            None,
        )
        .is_ok()
    };

    // SAFETY: The handle is valid.
    #[allow(unsafe_code)]
    unsafe {
        let _ = CloseHandle(reparse_handle);
    }

    // If the operation failed, assume it's not a reparse point.
    if !success {
        return false;
    }

    let reparse_point = String::from_utf16_lossy(&buf[..bytes_returned as usize]);
    reparse_point.contains("\\AppInstallerPythonRedirector.exe")
}

/// On Unix, we do not need to deal with Windows store shims.
///
/// See the Windows implementation for details.
#[cfg(not(windows))]
fn is_windows_store_shim(_path: &Path) -> bool {
    false
}

impl PythonVariant {
    fn matches_interpreter(self, interpreter: &Interpreter) -> bool {
        match self {
            Self::Default => {
                // TODO(zanieb): Right now, we allow debug interpreters to be selected by default for
                // backwards compatibility, but we may want to change this in the future.
                if (interpreter.python_major(), interpreter.python_minor()) >= (3, 14) {
                    // For Python 3.14+, the free-threaded build is not considered experimental
                    // and can satisfy the default variant without opt-in
                    true
                } else {
                    // In Python 3.13 and earlier, the free-threaded build is considered
                    // experimental and requires explicit opt-in
                    !interpreter.gil_disabled()
                }
            }
            Self::Debug => interpreter.debug_enabled(),
            Self::Freethreaded => interpreter.gil_disabled(),
            Self::FreethreadedDebug => interpreter.gil_disabled() && interpreter.debug_enabled(),
            Self::Gil => !interpreter.gil_disabled(),
            Self::GilDebug => !interpreter.gil_disabled() && interpreter.debug_enabled(),
        }
    }

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

    pub fn is_freethreaded(self) -> bool {
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
    /// Create a request from a string.
    ///
    /// This cannot fail, which means weird inputs will be parsed as [`PythonRequest::File`] or
    /// [`PythonRequest::ExecutableName`].
    ///
    /// This is intended for parsing the argument to the `--python` flag. See also
    /// [`try_from_tool_name`][Self::try_from_tool_name] below.
    pub fn parse(value: &str) -> Self {
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
        let all_implementation_names =
            ImplementationName::long_names().chain(ImplementationName::short_names());
        // Abstract versions like `python@312`, `python312`, or `312`, plus implementations and
        // implementation versions like `pypy`, `pypy@312` or `pypy312`.
        if let Ok(Some(request)) = Self::parse_versions_and_implementations(
            abstract_version_prefixes,
            all_implementation_names,
            lowercase_value,
        ) {
            return request;
        }

        let value_as_path = PathBuf::from(value);
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

        // During unit testing, we cannot change the working directory used by std
        // so we perform a check relative to the mock working directory. Ideally we'd
        // remove this code and use tests at the CLI level so we can change the real
        // directory.
        #[cfg(test)]
        if value_as_path.is_relative() {
            if let Ok(current_dir) = crate::current_dir() {
                let relative = current_dir.join(&value_as_path);
                if relative.is_dir() {
                    return Self::Directory(relative);
                }
                if relative.is_file() {
                    return Self::File(relative);
                }
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
    pub fn try_from_tool_name(value: &str) -> Result<Option<Self>, Error> {
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
            ImplementationName::long_names(),
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
        // expected to be either long_names() or all names
        implementation_names: impl IntoIterator<Item = &'a str>,
        // the string to parse
        lowercase_value: &str,
    ) -> Result<Option<Self>, Error> {
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
    ) -> Result<Option<VersionRequest>, Error> {
        if lowercase_value.starts_with('@') {
            return Err(Error::InvalidVersionRequest(lowercase_value.to_string()));
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
                return Err(Error::LatestVersionRequest);
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

    /// Check if a given interpreter satisfies the interpreter request.
    pub fn satisfied(&self, interpreter: &Interpreter, cache: &Cache) -> bool {
        /// Returns `true` if the two paths refer to the same interpreter executable.
        fn is_same_executable(path1: &Path, path2: &Path) -> bool {
            path1 == path2 || is_same_file(path1, path2).unwrap_or(false)
        }

        match self {
            Self::Default | Self::Any => true,
            Self::Version(version_request) => version_request.matches_interpreter(interpreter),
            Self::Directory(directory) => {
                // `sys.prefix` points to the environment root or `sys.executable` is the same
                is_same_executable(directory, interpreter.sys_prefix())
                    || is_same_executable(
                        virtualenv_python_executable(directory).as_path(),
                        interpreter.sys_executable(),
                    )
            }
            Self::File(file) => {
                // The interpreter satisfies the request both if it is the venv...
                if is_same_executable(interpreter.sys_executable(), file) {
                    return true;
                }
                // ...or if it is the base interpreter the venv was created from.
                if interpreter
                    .sys_base_executable()
                    .is_some_and(|sys_base_executable| {
                        is_same_executable(sys_base_executable, file)
                    })
                {
                    return true;
                }
                // ...or, on Windows, if both interpreters have the same base executable. On
                // Windows, interpreters are copied rather than symlinked, so a virtual environment
                // created from within a virtual environment will _not_ evaluate to the same
                // `sys.executable`, but will have the same `sys._base_executable`.
                if cfg!(windows) {
                    if let Ok(file_interpreter) = Interpreter::query(file, cache) {
                        if let (Some(file_base), Some(interpreter_base)) = (
                            file_interpreter.sys_base_executable(),
                            interpreter.sys_base_executable(),
                        ) {
                            if is_same_executable(file_base, interpreter_base) {
                                return true;
                            }
                        }
                    }
                }
                false
            }
            Self::ExecutableName(name) => {
                // First, see if we have a match in the venv ...
                if interpreter
                    .sys_executable()
                    .file_name()
                    .is_some_and(|filename| filename == name.as_str())
                {
                    return true;
                }
                // ... or the venv's base interpreter (without performing IO), if that fails, ...
                if interpreter
                    .sys_base_executable()
                    .and_then(|executable| executable.file_name())
                    .is_some_and(|file_name| file_name == name.as_str())
                {
                    return true;
                }
                // ... check in `PATH`. The name we find here does not need to be the
                // name we install, so we can find `foopython` here which got installed as `python`.
                if which(name)
                    .ok()
                    .as_ref()
                    .and_then(|executable| executable.file_name())
                    .is_some_and(|file_name| file_name == name.as_str())
                {
                    return true;
                }
                false
            }
            Self::Implementation(implementation) => interpreter
                .implementation_name()
                .eq_ignore_ascii_case(implementation.into()),
            Self::ImplementationVersion(implementation, version) => {
                version.matches_interpreter(interpreter)
                    && interpreter
                        .implementation_name()
                        .eq_ignore_ascii_case(implementation.into())
            }
            Self::Key(request) => request.satisfied_by_interpreter(interpreter),
        }
    }

    /// Whether this request opts-in to a pre-release Python version.
    pub(crate) fn allows_prereleases(&self) -> bool {
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
    pub(crate) fn allows_debug(&self) -> bool {
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
    pub(crate) fn allows_alternative_implementations(&self) -> bool {
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

    pub(crate) fn is_explicit_system(&self) -> bool {
        matches!(self, Self::File(_) | Self::Directory(_))
    }

    /// Serialize the request to a canonical representation.
    ///
    /// [`Self::parse`] should always return the same request when given the output of this method.
    pub fn to_canonical_string(&self) -> String {
        match self {
            Self::Any => "any".to_string(),
            Self::Default => "default".to_string(),
            Self::Version(version) => version.to_string(),
            Self::Directory(path) => path.display().to_string(),
            Self::File(path) => path.display().to_string(),
            Self::ExecutableName(name) => name.clone(),
            Self::Implementation(implementation) => implementation.to_string(),
            Self::ImplementationVersion(implementation, version) => {
                format!("{implementation}@{version}")
            }
            Self::Key(request) => request.to_string(),
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
            _ => None,
        }
    }
}

impl PythonSource {
    pub fn is_managed(self) -> bool {
        matches!(self, Self::Managed)
    }

    /// Whether a pre-release Python installation from this source can be used without opt-in.
    pub(crate) fn allows_prereleases(self) -> bool {
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
    pub(crate) fn allows_debug(self) -> bool {
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
    pub(crate) fn allows_alternative_implementations(self) -> bool {
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
    pub(crate) fn is_maybe_virtualenv(self) -> bool {
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

    /// Whether this source **could** be a system interpreter.
    pub(crate) fn is_maybe_system(self) -> bool {
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
    fn allows(self, source: PythonSource) -> bool {
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

    pub(crate) fn allows_managed(self) -> bool {
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
pub(crate) struct ExecutableName {
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
            | Self::MajorMinorPrerelease(major, minor, _, variant) => {
                Self::MajorMinor(major, minor, variant)
            }
        }
    }

    /// Return possible executable names for the given version request.
    pub(crate) fn executable_names(
        &self,
        implementation: Option<&ImplementationName>,
    ) -> Vec<ExecutableName> {
        let prerelease = if let Self::MajorMinorPrerelease(_, _, prerelease, _) = self {
            // Include the prerelease version, e.g., `python3.8a`
            Some(prerelease)
        } else {
            None
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
        if let Some(variant) = self.variant() {
            if variant != PythonVariant::Default {
                for i in 0..names.len() {
                    let name = names[i].with_variant(variant);
                    names.push(name);
                }
            }
        }

        names.sort_unstable_by_key(|name| name.into_comparator(self, implementation));
        names.reverse();

        names
    }

    /// Return the major version segment of the request, if any.
    pub(crate) fn major(&self) -> Option<u8> {
        match self {
            Self::Any | Self::Default | Self::Range(_, _) => None,
            Self::Major(major, _) => Some(*major),
            Self::MajorMinor(major, _, _) => Some(*major),
            Self::MajorMinorPatch(major, _, _, _) => Some(*major),
            Self::MajorMinorPrerelease(major, _, _, _) => Some(*major),
        }
    }

    /// Return the minor version segment of the request, if any.
    pub(crate) fn minor(&self) -> Option<u8> {
        match self {
            Self::Any | Self::Default | Self::Range(_, _) => None,
            Self::Major(_, _) => None,
            Self::MajorMinor(_, minor, _) => Some(*minor),
            Self::MajorMinorPatch(_, minor, _, _) => Some(*minor),
            Self::MajorMinorPrerelease(_, minor, _, _) => Some(*minor),
        }
    }

    /// Return the patch version segment of the request, if any.
    pub(crate) fn patch(&self) -> Option<u8> {
        match self {
            Self::Any | Self::Default | Self::Range(_, _) => None,
            Self::Major(_, _) => None,
            Self::MajorMinor(_, _, _) => None,
            Self::MajorMinorPatch(_, _, patch, _) => Some(*patch),
            Self::MajorMinorPrerelease(_, _, _, _) => None,
        }
    }

    /// Return the pre-release segment of the request, if any.
    pub(crate) fn prerelease(&self) -> Option<&Prerelease> {
        match self {
            Self::Any | Self::Default | Self::Range(_, _) => None,
            Self::Major(_, _) => None,
            Self::MajorMinor(_, _, _) => None,
            Self::MajorMinorPatch(_, _, _, _) => None,
            Self::MajorMinorPrerelease(_, _, prerelease, _) => Some(prerelease),
        }
    }

    /// Check if the request is for a version supported by uv.
    ///
    /// If not, an `Err` is returned with an explanatory message.
    pub(crate) fn check_supported(&self) -> Result<(), String> {
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
                if (*major, *minor) < (3, 7) {
                    return Err(format!(
                        "Python <3.7 is not supported but {major}.{minor} was requested."
                    ));
                }
            }
            Self::MajorMinorPatch(major, minor, patch, _) => {
                if (*major, *minor) < (3, 7) {
                    return Err(format!(
                        "Python <3.7 is not supported but {major}.{minor}.{patch} was requested."
                    ));
                }
            }
            Self::MajorMinorPrerelease(major, minor, prerelease, _) => {
                if (*major, *minor) < (3, 7) {
                    return Err(format!(
                        "Python <3.7 is not supported but {major}.{minor}{prerelease} was requested."
                    ));
                }
            }
            // TODO(zanieb): We could do some checking here to see if the range can be satisfied
            Self::Range(_, _) => (),
        }

        if self.is_freethreaded() {
            if let Self::MajorMinor(major, minor, _) = self.clone().without_patch() {
                if (major, minor) < (3, 13) {
                    return Err(format!(
                        "Python <3.13 does not support free-threading but {self} was requested."
                    ));
                }
            }
        }

        Ok(())
    }

    /// Change this request into a request appropriate for the given [`PythonSource`].
    ///
    /// For example, if [`VersionRequest::Default`] is requested, it will be changed to
    /// [`VersionRequest::Any`] for sources that should allow non-default interpreters like
    /// free-threaded variants.
    #[must_use]
    pub(crate) fn into_request_for_source(self, source: PythonSource) -> Self {
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

    /// Check if a interpreter matches the request.
    pub(crate) fn matches_interpreter(&self, interpreter: &Interpreter) -> bool {
        match self {
            Self::Any => true,
            // Do not use free-threaded interpreters by default
            Self::Default => PythonVariant::Default.matches_interpreter(interpreter),
            Self::Major(major, variant) => {
                interpreter.python_major() == *major && variant.matches_interpreter(interpreter)
            }
            Self::MajorMinor(major, minor, variant) => {
                (interpreter.python_major(), interpreter.python_minor()) == (*major, *minor)
                    && variant.matches_interpreter(interpreter)
            }
            Self::MajorMinorPatch(major, minor, patch, variant) => {
                (
                    interpreter.python_major(),
                    interpreter.python_minor(),
                    interpreter.python_patch(),
                ) == (*major, *minor, *patch)
                    // When a patch version is included, we treat it as a request for a stable
                    // release
                    && interpreter.python_version().pre().is_none()
                    && variant.matches_interpreter(interpreter)
            }
            Self::Range(specifiers, variant) => {
                // If the specifier contains pre-releases, use the full version for comparison.
                // Otherwise, strip pre-release so that, e.g., `>=3.14` matches `3.14.0rc3`.
                let version = if specifiers
                    .iter()
                    .any(uv_pep440::VersionSpecifier::any_prerelease)
                {
                    Cow::Borrowed(interpreter.python_version())
                } else {
                    Cow::Owned(interpreter.python_version().only_release())
                };
                specifiers.contains(&version) && variant.matches_interpreter(interpreter)
            }
            Self::MajorMinorPrerelease(major, minor, prerelease, variant) => {
                let version = interpreter.python_version();
                let Some(interpreter_prerelease) = version.pre() else {
                    return false;
                };
                (
                    interpreter.python_major(),
                    interpreter.python_minor(),
                    interpreter_prerelease,
                ) == (*major, *minor, *prerelease)
                    && variant.matches_interpreter(interpreter)
            }
        }
    }

    /// Check if a version is compatible with the request.
    ///
    /// WARNING: Use [`VersionRequest::matches_interpreter`] too. This method is only suitable to
    /// avoid querying interpreters if it's clear it cannot fulfill the request.
    pub(crate) fn matches_version(&self, version: &PythonVersion) -> bool {
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
        }
    }

    /// Check if major and minor version segments are compatible with the request.
    ///
    /// WARNING: Use [`VersionRequest::matches_interpreter`] too. This method is only suitable to
    /// avoid querying interpreters if it's clear it cannot fulfill the request.
    fn matches_major_minor(&self, major: u8, minor: u8) -> bool {
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
        }
    }

    /// Check if major, minor, patch, and prerelease version segments are compatible with the
    /// request.
    ///
    /// WARNING: Use [`VersionRequest::matches_interpreter`] too. This method is only suitable to
    /// avoid querying interpreters if it's clear it cannot fulfill the request.
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
                // Pre-releases of Python versions are always for the zero patch version
                (*self_major, *self_minor, 0, Some(*self_prerelease))
                    == (major, minor, patch, prerelease)
            }
        }
    }

    /// Check if a [`PythonInstallationKey`] is compatible with the request.
    ///
    /// WARNING: Use [`VersionRequest::matches_interpreter`] too. This method is only suitable to
    /// avoid querying interpreters if it's clear it cannot fulfill the request.
    pub(crate) fn matches_installation_key(&self, key: &PythonInstallationKey) -> bool {
        self.matches_major_minor_patch_prerelease(key.major, key.minor, key.patch, key.prerelease())
    }

    /// Whether a patch version segment is present in the request.
    fn has_patch(&self) -> bool {
        match self {
            Self::Any | Self::Default => false,
            Self::Major(..) => false,
            Self::MajorMinor(..) => false,
            Self::MajorMinorPatch(..) => true,
            Self::MajorMinorPrerelease(..) => false,
            Self::Range(_, _) => false,
        }
    }

    /// Return a new [`VersionRequest`] without the patch version if possible.
    ///
    /// If the patch version is not present, the request is returned unchanged.
    #[must_use]
    fn without_patch(self) -> Self {
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
            | Self::Range(_, variant) => variant.is_debug(),
        }
    }

    /// Whether this request is for a free-threaded Python variant.
    pub(crate) fn is_freethreaded(&self) -> bool {
        match self {
            Self::Any | Self::Default => false,
            Self::Major(_, variant)
            | Self::MajorMinor(_, _, variant)
            | Self::MajorMinorPatch(_, _, _, variant)
            | Self::MajorMinorPrerelease(_, _, _, variant)
            | Self::Range(_, variant) => variant.is_freethreaded(),
        }
    }

    /// Return a new [`VersionRequest`] with the [`PythonVariant`] if it has one.
    ///
    /// This is useful for converting the string representation to pep440.
    #[must_use]
    pub fn without_python_variant(self) -> Self {
        // TODO(zanieb): Replace this entire function with a utility that casts this to a version
        // without using `VersionRequest::to_string`.
        match self {
            Self::Any | Self::Default => self,
            Self::Major(major, _) => Self::Major(major, PythonVariant::Default),
            Self::MajorMinor(major, minor, _) => {
                Self::MajorMinor(major, minor, PythonVariant::Default)
            }
            Self::MajorMinorPatch(major, minor, patch, _) => {
                Self::MajorMinorPatch(major, minor, patch, PythonVariant::Default)
            }
            Self::MajorMinorPrerelease(major, minor, prerelease, _) => {
                Self::MajorMinorPrerelease(major, minor, prerelease, PythonVariant::Default)
            }
            Self::Range(specifiers, _) => Self::Range(specifiers, PythonVariant::Default),
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
            | Self::Range(_, variant) => Some(*variant),
        }
    }

    /// Convert this request into a concrete PEP 440 `Version` when possible.
    ///
    /// Returns `None` for non-concrete requests
    pub fn as_pep440_version(&self) -> Option<Version> {
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
            // Pre-releases of Python versions are always for the zero patch version
            Self::MajorMinorPrerelease(major, minor, prerelease, _) => Some(
                Version::new([u64::from(*major), u64::from(*minor), 0]).with_pre(Some(*prerelease)),
            ),
        }
    }
}

impl FromStr for VersionRequest {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        /// Extract the variant from the end of a version request string, returning the prefix and
        /// the variant type.
        fn parse_variant(s: &str) -> Result<(&str, PythonVariant), Error> {
            // This cannot be a valid version, just error immediately
            if s.chars().all(char::is_alphabetic) {
                return Err(Error::InvalidVersionRequest(s.to_string()));
            }

            let Some(mut start) = s.rfind(|c: char| c.is_numeric()) else {
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
            return Err(Error::InvalidVersionRequest(s.to_string()));
        }

        // We don't allow local version suffixes unless they're variants, in which case they'd
        // already be stripped.
        if !version.local().is_empty() {
            return Err(Error::InvalidVersionRequest(s.to_string()));
        }

        // Cast the release components into u8s since that's what we use in `VersionRequest`
        let Ok(release) = try_into_u8_slice(&version.release()) else {
            return Err(Error::InvalidVersionRequest(s.to_string()));
        };

        let prerelease = version.pre();

        match release.as_slice() {
            // e.g. `3
            [major] => {
                // Prereleases are not allowed here, e.g., `3rc1` doesn't make sense
                if prerelease.is_some() {
                    return Err(Error::InvalidVersionRequest(s.to_string()));
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
            // e.g. `3.12.1` or `3.13.0rc1`
            [major, minor, patch] => {
                if let Some(prerelease) = prerelease {
                    // Prereleases are only allowed for the first patch version, e.g, 3.12.2rc1
                    // isn't a proper Python release
                    if *patch != 0 {
                        return Err(Error::InvalidVersionRequest(s.to_string()));
                    }
                    return Ok(Self::MajorMinorPrerelease(
                        *major, *minor, prerelease, variant,
                    ));
                }
                Ok(Self::MajorMinorPatch(*major, *minor, *patch, variant))
            }
            _ => Err(Error::InvalidVersionRequest(s.to_string())),
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
) -> Result<VersionRequest, Error> {
    let Ok(specifiers) = VersionSpecifiers::from_str(s) else {
        return Err(Error::InvalidVersionRequest(s.to_string()));
    };
    if specifiers.is_empty() {
        return Err(Error::InvalidVersionRequest(s.to_string()));
    }
    Ok(VersionRequest::Range(specifiers, variant))
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
    fn sources(self) -> &'static [PythonSource] {
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

    /// Return the canonical name.
    // TODO(zanieb): This should be a `Display` impl and we should have a different view for
    // the sources
    pub fn canonical_name(&self) -> &'static str {
        match self {
            Self::OnlyManaged => "only managed",
            Self::Managed => "prefer managed",
            Self::System => "prefer system",
            Self::OnlySystem => "only system",
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

impl DiscoveryPreferences {
    /// Return a string describing the sources that are considered when searching for Python with
    /// the given preferences.
    fn sources(&self, request: &PythonRequest) -> String {
        let python_sources = self
            .python_preference
            .sources()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        match self.environment_preference {
            EnvironmentPreference::Any => disjunction(
                &["virtual environments"]
                    .into_iter()
                    .chain(python_sources.iter().map(String::as_str))
                    .collect::<Vec<_>>(),
            ),
            EnvironmentPreference::ExplicitSystem => {
                if request.is_explicit_system() {
                    disjunction(
                        &["virtual environments"]
                            .into_iter()
                            .chain(python_sources.iter().map(String::as_str))
                            .collect::<Vec<_>>(),
                    )
                } else {
                    disjunction(&["virtual environments"])
                }
            }
            EnvironmentPreference::OnlySystem => disjunction(
                &python_sources
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
            ),
            EnvironmentPreference::OnlyVirtual => disjunction(&["virtual environments"]),
        }
    }
}

impl fmt::Display for PythonNotFound {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let sources = DiscoveryPreferences {
            python_preference: self.python_preference,
            environment_preference: self.environment_preference,
        }
        .sources(&self.request);

        match self.request {
            PythonRequest::Default | PythonRequest::Any => {
                write!(f, "No interpreter found in {sources}")
            }
            PythonRequest::File(_) => {
                write!(f, "No interpreter found at {}", self.request)
            }
            PythonRequest::Directory(_) => {
                write!(f, "No interpreter found in {}", self.request)
            }
            _ => {
                write!(f, "No interpreter found for {} in {sources}", self.request)
            }
        }
    }
}

/// Join a series of items with `or` separators, making use of commas when necessary.
fn disjunction(items: &[&str]) -> String {
    match items.len() {
        0 => String::new(),
        1 => items[0].to_string(),
        2 => format!("{} or {}", items[0], items[1]),
        _ => {
            let last = items.last().unwrap();
            format!(
                "{}, or {}",
                items.iter().take(items.len() - 1).join(", "),
                last
            )
        }
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
    use std::{path::PathBuf, str::FromStr};

    use assert_fs::{TempDir, prelude::*};
    use target_lexicon::{Aarch64Architecture, Architecture};
    use test_log::test;
    use uv_pep440::{Prerelease, PrereleaseKind, Version, VersionSpecifiers};

    use crate::{
        discovery::{PythonRequest, VersionRequest},
        downloads::{ArchRequest, PythonDownloadRequest},
        implementation::ImplementationName,
    };
    use uv_platform::{Arch, Libc, Os};

    use super::{
        DiscoveryPreferences, EnvironmentPreference, Error, PythonPreference, PythonVariant,
    };

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
    fn discovery_sources_prefer_system_orders_search_path_first() {
        let preferences = DiscoveryPreferences {
            python_preference: PythonPreference::System,
            environment_preference: EnvironmentPreference::OnlySystem,
        };
        let sources = preferences.sources(&PythonRequest::Default);

        if cfg!(windows) {
            assert_eq!(sources, "search path, registry, or managed installations");
        } else {
            assert_eq!(sources, "search path or managed installations");
        }
    }

    #[test]
    fn discovery_sources_only_system_matches_platform_order() {
        let preferences = DiscoveryPreferences {
            python_preference: PythonPreference::OnlySystem,
            environment_preference: EnvironmentPreference::OnlySystem,
        };
        let sources = preferences.sources(&PythonRequest::Default);

        if cfg!(windows) {
            assert_eq!(sources, "search path or registry");
        } else {
            assert_eq!(sources, "search path");
        }
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
        assert!(
            matches!(
                VersionRequest::from_str("3rc1"),
                Err(Error::InvalidVersionRequest(_))
            ),
            "Pre-release version requests require a minor version"
        );
        assert!(
            matches!(
                VersionRequest::from_str("3.13.2rc1"),
                Err(Error::InvalidVersionRequest(_))
            ),
            "Pre-release version requests require a patch version of zero"
        );
        assert!(
            matches!(
                VersionRequest::from_str("3.12-dev"),
                Err(Error::InvalidVersionRequest(_))
            ),
            "Development version segments are not allowed"
        );
        assert!(
            matches!(
                VersionRequest::from_str("3.12+local"),
                Err(Error::InvalidVersionRequest(_))
            ),
            "Local version segments are not allowed"
        );
        assert!(
            matches!(
                VersionRequest::from_str("3.12.post0"),
                Err(Error::InvalidVersionRequest(_))
            ),
            "Post version segments are not allowed"
        );
        assert!(
            // Test for overflow
            matches!(
                VersionRequest::from_str("31000"),
                Err(Error::InvalidVersionRequest(_))
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
        assert!(matches!(
            VersionRequest::from_str("3.13tt"),
            Err(Error::InvalidVersionRequest(_))
        ));
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
        assert!(matches!(
            PythonRequest::try_split_prefix_and_version("prefix", "prefix"),
            Ok(None),
        ));
        assert!(matches!(
            PythonRequest::try_split_prefix_and_version("prefix", "prefix3"),
            Ok(Some(_)),
        ));
        assert!(matches!(
            PythonRequest::try_split_prefix_and_version("prefix", "prefix@3"),
            Ok(Some(_)),
        ));
        assert!(matches!(
            PythonRequest::try_split_prefix_and_version("prefix", "prefix3notaversion"),
            Ok(None),
        ));
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
}
