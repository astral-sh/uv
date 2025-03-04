use itertools::{Either, Itertools};
use regex::Regex;
use same_file::is_same_file;
use std::env::consts::EXE_SUFFIX;
use std::fmt::{self, Debug, Formatter};
use std::{env, io, iter};
use std::{path::Path, path::PathBuf, str::FromStr};
use thiserror::Error;
use tracing::{debug, instrument, trace};
use which::{which, which_all};

use uv_cache::Cache;
use uv_fs::which::is_executable;
use uv_fs::Simplified;
use uv_pep440::{
    release_specifiers_to_ranges, LowerBound, Prerelease, UpperBound, Version, VersionSpecifier,
    VersionSpecifiers,
};
use uv_static::EnvVars;
use uv_warnings::warn_user_once;

use crate::downloads::PythonDownloadRequest;
use crate::implementation::ImplementationName;
use crate::installation::PythonInstallation;
use crate::interpreter::Error as InterpreterError;
use crate::interpreter::{StatusCodeError, UnexpectedResponseError};
use crate::managed::ManagedPythonInstallations;
#[cfg(windows)]
use crate::microsoft_store::find_microsoft_store_pythons;
use crate::virtualenv::Error as VirtualEnvError;
use crate::virtualenv::{
    conda_environment_from_env, virtualenv_from_env, virtualenv_from_working_dir,
    virtualenv_python_executable, CondaEnvironmentKind,
};
#[cfg(windows)]
use crate::windows_registry::{registry_pythons, WindowsPython};
use crate::{Interpreter, PythonVersion};

/// A request to find a Python installation.
///
/// See [`PythonRequest::from_str`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
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
            "auto" | "automatic" | "true" | "1" => Ok(PythonDownloads::Automatic),
            "manual" => Ok(PythonDownloads::Manual),
            "never" | "false" | "0" => Ok(PythonDownloads::Never),
            _ => Err(format!("Invalid value for `python-download`: '{s}'")),
        }
    }
}

impl From<bool> for PythonDownloads {
    fn from(value: bool) -> Self {
        if value {
            PythonDownloads::Automatic
        } else {
            PythonDownloads::Never
        }
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
    Freethreaded,
}

/// A Python discovery version request.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
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

    /// An error was encountered when interacting with a managed Python installation.
    #[error(transparent)]
    ManagedPython(#[from] crate::managed::Error),

    /// An error was encountered when inspecting a virtual environment.
    #[error(transparent)]
    VirtualEnv(#[from] crate::virtualenv::Error),

    #[cfg(windows)]
    #[error("Failed to query installed Python versions from the Windows registry")]
    RegistryError(#[from] windows_result::Error),

    /// An invalid version request was given
    #[error("Invalid version request: {0}")]
    InvalidVersionRequest(String),

    // TODO(zanieb): Is this error case necessary still? We should probably drop it.
    #[error("Interpreter discovery for `{0}` requires `{1}` but only {2} is allowed")]
    SourceNotAllowed(PythonRequest, PythonSource, PythonPreference),
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
) -> impl Iterator<Item = Result<(PythonSource, PathBuf), Error>> + 'a {
    let from_active_environment = iter::once_with(|| {
        virtualenv_from_env()
            .into_iter()
            .map(virtualenv_python_executable)
            .map(|path| Ok((PythonSource::ActiveEnvironment, path)))
    })
    .flatten();

    // N.B. we prefer the conda environment over discovered virtual environments
    let from_conda_environment = iter::once_with(|| {
        conda_environment_from_env(CondaEnvironmentKind::Child)
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
                // Check that the Python version satisfies the request to avoid unnecessary interpreter queries later
                Ok(installations
                    .into_iter()
                    .filter(move |installation| {
                        if version.matches_version(&installation.version()) {
                            true
                        } else {
                            debug!("Skipping incompatible managed installation `{installation}`");
                            false
                        }
                    })
                    .inspect(|installation| debug!("Found managed installation `{installation}`"))
                    .map(|installation| (PythonSource::Managed, installation.executable(false))))
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
        PythonPreference::OnlyManaged => Box::new(from_managed_installations),
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
/// Note that Python executables may be excluded by the given [`EnvironmentPreference`] and
/// [`PythonPreference`]. However, these filters are only applied for performance. We cannot
/// guarantee that the [`EnvironmentPreference`] is satisfied until we query the interpreter.
///
/// See [`python_executables_from_installed`] and [`python_executables_from_virtual_environments`]
/// for more information on discovery.
fn python_executables<'a>(
    version: &'a VersionRequest,
    implementation: Option<&'a ImplementationName>,
    environments: EnvironmentPreference,
    preference: PythonPreference,
) -> Box<dyn Iterator<Item = Result<(PythonSource, PathBuf), Error>> + 'a> {
    // Always read from `UV_INTERNAL__PARENT_INTERPRETER` — it could be a system interpreter
    let from_parent_interpreter = iter::once_with(|| {
        env::var_os(EnvVars::UV_INTERNAL__PARENT_INTERPRETER)
            .into_iter()
            .map(|path| Ok((PythonSource::ParentInterpreter, PathBuf::from(path))))
    })
    .flatten();

    // Check if the base conda environment is active
    let from_base_conda_environment = iter::once_with(|| {
        conda_environment_from_env(CondaEnvironmentKind::Base)
            .into_iter()
            .map(virtualenv_python_executable)
            .map(|path| Ok((PythonSource::BaseCondaPrefix, path)))
    })
    .flatten();

    let from_virtual_environments = python_executables_from_virtual_environments();
    let from_installed = python_executables_from_installed(version, implementation, preference);

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
                .inspect(|path| trace!("Found possible Python executable: {}", path.display()))
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
}

/// Find all acceptable `python3.x` minor versions.
///
/// For example, let's say `python` and `python3` are Python 3.10. When a user requests `>= 3.11`,
/// we still need to find a `python3.12` in PATH.
fn find_all_minor(
    implementation: Option<&ImplementationName>,
    version_request: &VersionRequest,
    dir: &Path,
) -> impl Iterator<Item = PathBuf> {
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
/// Note interpreters may be excluded by the given [`EnvironmentPreference`] and [`PythonPreference`].
///
/// See [`python_executables`] for more information on discovery.
fn python_interpreters<'a>(
    version: &'a VersionRequest,
    implementation: Option<&'a ImplementationName>,
    environments: EnvironmentPreference,
    preference: PythonPreference,
    cache: &'a Cache,
) -> impl Iterator<Item = Result<(PythonSource, Interpreter), Error>> + 'a {
    python_interpreters_from_executables(
        // Perform filtering on the discovered executables based on their source. This avoids
        // unnecessary interpreter queries, which are generally expensive. We'll filter again
        // with `interpreter_satisfies_environment_preference` after querying.
        python_executables(version, implementation, environments, preference).filter_ok(
            move |(source, path)| {
                source_satisfies_environment_preference(*source, path, environments)
            },
        ),
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

/// Check if an encountered error is critical and should stop discovery.
///
/// Returns false when an error could be due to a faulty Python installation and we should continue searching for a working one.
impl Error {
    pub fn is_critical(&self) -> bool {
        match self {
            // When querying the Python interpreter fails, we will only raise errors that demonstrate that something is broken
            // If the Python interpreter returned a bad response, we'll continue searching for one that works
            Error::Query(err, _, source) => match &**err {
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
                InterpreterError::NotFound(path) => {
                    // If the interpreter is from an active, valid virtual environment, we should
                    // fail because it's broken
                    if let Some(Ok(true)) = matches!(source, PythonSource::ActiveEnvironment)
                        .then(|| {
                            path.parent()
                                .and_then(Path::parent)
                                .map(|path| path.join("pyvenv.cfg").try_exists())
                        })
                        .flatten()
                    {
                        true
                    } else {
                        trace!("Skipping missing interpreter at {}", path.display());
                        false
                    }
                }
            },
            Error::VirtualEnv(VirtualEnvError::MissingPyVenvCfg(path)) => {
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
                    Err(InterpreterError::NotFound(_)) => Ok(Err(PythonNotFound {
                        request: request.clone(),
                        python_preference: preference,
                        environment_preference: environments,
                    })),
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
                    Err(InterpreterError::NotFound(_)) => Ok(Err(PythonNotFound {
                        request: request.clone(),
                        python_preference: preference,
                        environment_preference: environments,
                    })),
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
            python_interpreters(&VersionRequest::Any, None, environments, preference, cache)
                .map_ok(|tuple| Ok(PythonInstallation::from_tuple(tuple)))
        }),
        PythonRequest::Default => Box::new({
            debug!("Searching for default Python interpreter in {sources}");
            python_interpreters(
                &VersionRequest::Default,
                None,
                environments,
                preference,
                cache,
            )
            .map_ok(|tuple| Ok(PythonInstallation::from_tuple(tuple)))
        }),
        PythonRequest::Version(version) => {
            if let Err(err) = version.check_supported() {
                return Box::new(iter::once(Err(Error::InvalidVersionRequest(err))));
            };
            Box::new({
                debug!("Searching for {request} in {sources}");
                python_interpreters(version, None, environments, preference, cache)
                    .map_ok(|tuple| Ok(PythonInstallation::from_tuple(tuple)))
            })
        }
        PythonRequest::Implementation(implementation) => Box::new({
            debug!("Searching for a {request} interpreter in {sources}");
            python_interpreters(
                &VersionRequest::Default,
                Some(implementation),
                environments,
                preference,
                cache,
            )
            .filter_ok(|(_source, interpreter)| {
                interpreter
                    .implementation_name()
                    .eq_ignore_ascii_case(implementation.into())
            })
            .map_ok(|tuple| Ok(PythonInstallation::from_tuple(tuple)))
        }),
        PythonRequest::ImplementationVersion(implementation, version) => {
            if let Err(err) = version.check_supported() {
                return Box::new(iter::once(Err(Error::InvalidVersionRequest(err))));
            };
            Box::new({
                debug!("Searching for {request} in {sources}");
                python_interpreters(
                    version,
                    Some(implementation),
                    environments,
                    preference,
                    cache,
                )
                .filter_ok(|(_source, interpreter)| {
                    interpreter
                        .implementation_name()
                        .eq_ignore_ascii_case(implementation.into())
                })
                .map_ok(|tuple| Ok(PythonInstallation::from_tuple(tuple)))
            })
        }
        PythonRequest::Key(request) => {
            if let Some(version) = request.version() {
                if let Err(err) = version.check_supported() {
                    return Box::new(iter::once(Err(Error::InvalidVersionRequest(err))));
                };
            };
            Box::new({
                debug!("Searching for {request} in {sources}");
                python_interpreters(
                    request.version().unwrap_or(&VersionRequest::Default),
                    request.implementation(),
                    environments,
                    preference,
                    cache,
                )
                .filter_ok(|(_source, interpreter)| request.satisfied_by_interpreter(interpreter))
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
) -> Result<FindPythonResult, Error> {
    let installations = find_python_installations(request, environments, preference, cache);
    let mut first_prerelease = None;
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
            debug!("Skipping pre-release {}", installation.key());
            if first_prerelease.is_none() {
                first_prerelease = Some(installation.clone());
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

        // If we didn't skip it, this is the installation to use
        return result;
    }

    // If we only found pre-releases, they're implicitly allowed and we should return the first one.
    if let Some(installation) = first_prerelease {
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
/// See [`find_python_installation`] for more details on installation discovery.
#[instrument(skip_all, fields(request))]
pub fn find_best_python_installation(
    request: &PythonRequest,
    environments: EnvironmentPreference,
    preference: PythonPreference,
    cache: &Cache,
) -> Result<FindPythonResult, Error> {
    debug!("Starting Python discovery for {}", request);

    // First, check for an exact match (or the first available version if no Python version was provided)
    debug!("Looking for exact match for request {request}");
    let result = find_python_installation(request, environments, preference, cache);
    match result {
        Ok(Ok(installation)) => {
            warn_on_unsupported_python(installation.interpreter());
            return Ok(Ok(installation));
        }
        // Continue if we can't find a matching Python and ignore non-critical discovery errors
        Ok(Err(_)) => {}
        Err(ref err) if !err.is_critical() => {}
        _ => return result,
    }

    // If that fails, and a specific patch version was requested try again allowing a
    // different patch version
    if let Some(request) = match request {
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
    } {
        debug!("Looking for relaxed patch version {request}");
        let result = find_python_installation(&request, environments, preference, cache);
        match result {
            Ok(Ok(installation)) => {
                warn_on_unsupported_python(installation.interpreter());
                return Ok(Ok(installation));
            }
            // Continue if we can't find a matching Python and ignore non-critical discovery errors
            Ok(Err(_)) => {}
            Err(ref err) if !err.is_critical() => {}
            _ => return result,
        }
    }

    // If a Python version was requested but cannot be fulfilled, just take any version
    debug!("Looking for a default Python installation");
    let request = PythonRequest::Default;
    Ok(
        find_python_installation(&request, environments, preference, cache)?.map_err(|err| {
            // Use a more general error in this case since we looked for multiple versions
            PythonNotFound {
                request,
                python_preference: err.python_preference,
                environment_preference: err.environment_preference,
            }
        }),
    )
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
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS,
        FILE_FLAG_OPEN_REPARSE_POINT, MAXIMUM_REPARSE_DATA_BUFFER_SIZE, OPEN_EXISTING,
    };
    use windows_sys::Win32::System::Ioctl::FSCTL_GET_REPARSE_POINT;
    use windows_sys::Win32::System::IO::DeviceIoControl;

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
    if md.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0 {
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
            path_encoded.as_mut_ptr(),
            0,
            0,
            std::ptr::null_mut(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            std::ptr::null_mut(),
        )
    };

    if reparse_handle == INVALID_HANDLE_VALUE {
        return false;
    }

    let mut buf = [0u16; MAXIMUM_REPARSE_DATA_BUFFER_SIZE as usize];
    let mut bytes_returned = 0;

    // SAFETY: The buffer is large enough to hold the reparse point.
    #[allow(unsafe_code, clippy::cast_possible_truncation)]
    let success = unsafe {
        DeviceIoControl(
            reparse_handle,
            FSCTL_GET_REPARSE_POINT,
            std::ptr::null_mut(),
            0,
            buf.as_mut_ptr().cast(),
            buf.len() as u32 * 2,
            &mut bytes_returned,
            std::ptr::null_mut(),
        ) != 0
    };

    // SAFETY: The handle is valid.
    #[allow(unsafe_code)]
    unsafe {
        CloseHandle(reparse_handle);
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
            PythonVariant::Default => !interpreter.gil_disabled(),
            PythonVariant::Freethreaded => interpreter.gil_disabled(),
        }
    }

    /// Return the lib or executable suffix for the variant, e.g., `t` for `python3.13t`.
    ///
    /// Returns an empty string for the default Python variant.
    pub fn suffix(self) -> &'static str {
        match self {
            Self::Default => "",
            Self::Freethreaded => "t",
        }
    }
}
impl PythonRequest {
    /// Create a request from a string.
    ///
    /// This cannot fail, which means weird inputs will be parsed as [`PythonRequest::File`] or [`PythonRequest::ExecutableName`].
    pub fn parse(value: &str) -> Self {
        // Literals, e.g. `any` or `default`
        if value.eq_ignore_ascii_case("any") {
            return Self::Any;
        }
        if value.eq_ignore_ascii_case("default") {
            return Self::Default;
        }

        // e.g. `3.12.1`, `312`, or `>=3.12`
        if let Ok(version) = VersionRequest::from_str(value) {
            return Self::Version(version);
        }
        // e.g. `python3.12.1`
        if let Some(remainder) = value.strip_prefix("python") {
            if let Ok(version) = VersionRequest::from_str(remainder) {
                return Self::Version(version);
            }
        }
        // e.g. `pypy@3.12`
        if let Some((first, second)) = value.split_once('@') {
            if let Ok(implementation) = ImplementationName::from_str(first) {
                if let Ok(version) = VersionRequest::from_str(second) {
                    return Self::ImplementationVersion(implementation, version);
                }
            }
        }
        for implementation in
            ImplementationName::long_names().chain(ImplementationName::short_names())
        {
            if let Some(remainder) = value.to_ascii_lowercase().strip_prefix(implementation) {
                // e.g. `pypy`
                if remainder.is_empty() {
                    return Self::Implementation(
                        // Safety: The name matched the possible names above
                        ImplementationName::from_str(implementation).unwrap(),
                    );
                }
                // e.g. `pypy3.12` or `pp312`
                if let Ok(version) = VersionRequest::from_str(remainder) {
                    return Self::ImplementationVersion(
                        // Safety: The name matched the possible names above
                        ImplementationName::from_str(implementation).unwrap(),
                        version,
                    );
                }
            }
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

        // e.g. path/to/python on Windows, where path/to/python is the true path
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

    /// Check if a given interpreter satisfies the interpreter request.
    pub fn satisfied(&self, interpreter: &Interpreter, cache: &Cache) -> bool {
        /// Returns `true` if the two paths refer to the same interpreter executable.
        fn is_same_executable(path1: &Path, path2: &Path) -> bool {
            path1 == path2 || is_same_file(path1, path2).unwrap_or(false)
        }

        match self {
            PythonRequest::Default | PythonRequest::Any => true,
            PythonRequest::Version(version_request) => {
                version_request.matches_interpreter(interpreter)
            }
            PythonRequest::Directory(directory) => {
                // `sys.prefix` points to the environment root or `sys.executable` is the same
                is_same_executable(directory, interpreter.sys_prefix())
                    || is_same_executable(
                        virtualenv_python_executable(directory).as_path(),
                        interpreter.sys_executable(),
                    )
            }
            PythonRequest::File(file) => {
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
            PythonRequest::ExecutableName(name) => {
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
            PythonRequest::Implementation(implementation) => interpreter
                .implementation_name()
                .eq_ignore_ascii_case(implementation.into()),
            PythonRequest::ImplementationVersion(implementation, version) => {
                version.matches_interpreter(interpreter)
                    && interpreter
                        .implementation_name()
                        .eq_ignore_ascii_case(implementation.into())
            }
            PythonRequest::Key(request) => request.satisfied_by_interpreter(interpreter),
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

    /// Whether this request opts-in to an alternative Python implementation, e.g., PyPy.
    pub(crate) fn allows_alternative_implementations(&self) -> bool {
        match self {
            Self::Default => false,
            Self::Any => true,
            Self::Version(_) => false,
            Self::Directory(_) | Self::File(_) | Self::ExecutableName(_) => true,
            Self::Implementation(_) => true,
            Self::ImplementationVersion(_, _) => true,
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
            PythonPreference::OnlyManaged => matches!(source, PythonSource::Managed),
            Self::Managed | Self::System => matches!(
                source,
                PythonSource::Managed | PythonSource::SearchPath | PythonSource::Registry
            ),
            PythonPreference::OnlySystem => {
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
        f.write_str(self.variant.suffix())?;
        f.write_str(EXE_SUFFIX)?;
        Ok(())
    }
}

impl VersionRequest {
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
        if self.is_freethreaded() {
            for i in 0..names.len() {
                let name = names[i].with_variant(PythonVariant::Freethreaded);
                names.push(name);
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
                    && variant.matches_interpreter(interpreter)
            }
            Self::Range(specifiers, variant) => {
                let version = interpreter.python_version().only_release();
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
            Self::Range(specifiers, _) => specifiers.contains(&version.version),
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
            }
            Self::Range(specifiers, _) => specifiers.contains(
                &Version::new([u64::from(major), u64::from(minor), u64::from(patch)])
                    .with_pre(prerelease),
            ),
            Self::MajorMinorPrerelease(self_major, self_minor, self_prerelease, _) => {
                // Pre-releases of Python versions are always for the zero patch version
                (*self_major, *self_minor, 0) == (major, minor, patch)
                    && prerelease.is_none_or(|pre| *self_prerelease == pre)
            }
        }
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
            Self::Major(..) => true,
            Self::MajorMinor(..) => true,
            Self::MajorMinorPatch(..) => true,
            Self::MajorMinorPrerelease(..) => true,
            Self::Range(specifiers, _) => specifiers.iter().any(VersionSpecifier::any_prerelease),
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
            | Self::Range(_, variant) => variant == &PythonVariant::Freethreaded,
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
}

impl FromStr for VersionRequest {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Check if the version request is for a free-threaded Python version
        let (s, variant) = s
            .strip_suffix('t')
            .map_or((s, PythonVariant::Default), |s| {
                (s, PythonVariant::Freethreaded)
            });

        if variant == PythonVariant::Freethreaded && s.ends_with('t') {
            // More than one trailing "t" is not allowed
            return Err(Error::InvalidVersionRequest(format!("{s}t")));
        }

        let Ok(version) = Version::from_str(s) else {
            return parse_version_specifiers_request(s, variant);
        };

        // Split the release component if it uses the wheel tag format (e.g., `38`)
        let version = split_wheel_tag_release_version(version);

        // We dont allow post or dev version here
        if version.post().is_some() || version.dev().is_some() {
            return Err(Error::InvalidVersionRequest(s.to_string()));
        }

        // Check if the local version includes a variant
        let variant = if version.local().is_empty() {
            variant
        } else {
            // If we already have a variant, do not allow another to be requested
            if variant != PythonVariant::Default {
                return Err(Error::InvalidVersionRequest(s.to_string()));
            }

            let uv_pep440::LocalVersionSlice::Segments([uv_pep440::LocalSegment::String(local)]) =
                version.local()
            else {
                return Err(Error::InvalidVersionRequest(s.to_string()));
            };

            match local.as_str() {
                "freethreaded" => PythonVariant::Freethreaded,
                _ => return Err(Error::InvalidVersionRequest(s.to_string())),
            }
        };

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
            "" => Ok(Self::Default),
            _ => Err(()),
        }
    }
}

impl fmt::Display for PythonVariant {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Default => f.write_str("default"),
            Self::Freethreaded => f.write_str("freethreaded"),
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
            Self::Major(major, PythonVariant::Default) => write!(f, "{major}"),
            Self::Major(major, PythonVariant::Freethreaded) => write!(f, "{major}t"),
            Self::MajorMinor(major, minor, PythonVariant::Default) => write!(f, "{major}.{minor}"),
            Self::MajorMinor(major, minor, PythonVariant::Freethreaded) => {
                write!(f, "{major}.{minor}t")
            }
            Self::MajorMinorPatch(major, minor, patch, PythonVariant::Default) => {
                write!(f, "{major}.{minor}.{patch}")
            }
            Self::MajorMinorPatch(major, minor, patch, PythonVariant::Freethreaded) => {
                write!(f, "{major}.{minor}.{patch}t")
            }
            Self::MajorMinorPrerelease(major, minor, prerelease, PythonVariant::Default) => {
                write!(f, "{major}.{minor}{prerelease}")
            }
            Self::MajorMinorPrerelease(major, minor, prerelease, PythonVariant::Freethreaded) => {
                write!(f, "{major}.{minor}{prerelease}t")
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
            Self::Managed | Self::System => {
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
            Self::OnlySystem => {
                if cfg!(windows) {
                    &[PythonSource::Registry, PythonSource::SearchPath]
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

    use assert_fs::{prelude::*, TempDir};
    use test_log::test;
    use uv_pep440::{Prerelease, PrereleaseKind, VersionSpecifiers};

    use crate::{
        discovery::{PythonRequest, VersionRequest},
        implementation::ImplementationName,
    };

    use super::{Error, PythonVariant};

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
}
