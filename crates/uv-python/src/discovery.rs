use itertools::{Either, Itertools};
use regex::Regex;
use same_file::is_same_file;
use std::borrow::Cow;
use std::env::consts::EXE_SUFFIX;
use std::fmt::{self, Formatter};
use std::{env, io, iter};
use std::{path::Path, path::PathBuf, str::FromStr};
use thiserror::Error;
use tracing::{debug, instrument, trace};
use which::{which, which_all};

use pep440_rs::{Version, VersionSpecifiers};
use uv_cache::Cache;
use uv_configuration::PreviewMode;
use uv_fs::Simplified;
use uv_warnings::warn_user_once;

use crate::downloads::PythonDownloadRequest;
use crate::implementation::ImplementationName;
use crate::installation::PythonInstallation;
use crate::interpreter::Error as InterpreterError;
use crate::managed::ManagedPythonInstallations;
use crate::py_launcher::{self, py_list_paths};
use crate::virtualenv::{
    conda_prefix_from_env, virtualenv_from_env, virtualenv_from_working_dir,
    virtualenv_python_executable,
};
use crate::which::is_executable;
use crate::{Interpreter, PythonVersion};

/// A request to find a Python installation.
///
/// See [`PythonRequest::from_str`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum PythonRequest {
    /// Use any discovered Python installation
    #[default]
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
    /// Prefer installed Python installations, only download managed Python installations if no system Python installation is found.
    ///
    /// Installed managed Python installations are still preferred over system Python installations.
    #[default]
    Installed,
    /// Prefer managed Python installations over system Python installations, even if fetching is required.
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
pub enum PythonFetch {
    /// Automatically fetch managed Python installations when needed.
    #[default]
    Automatic,
    /// Do not automatically fetch managed Python installations; require explicit installation.
    Manual,
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

/// A Python discovery version request.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum VersionRequest {
    #[default]
    Any,
    Major(u8),
    MajorMinor(u8, u8),
    MajorMinorPatch(u8, u8, u8),
    Range(VersionSpecifiers),
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
    /// An environment was discovered e.g. via `.venv`
    DiscoveredEnvironment,
    /// An executable was found in the search path i.e. `PATH`
    SearchPath,
    /// An executable was found via the `py` launcher
    PyLauncher,
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
    #[error(transparent)]
    Query(#[from] crate::interpreter::Error),

    /// An error was encountered when interacting with a managed Python installation.
    #[error(transparent)]
    ManagedPython(#[from] crate::managed::Error),

    /// An error was encountered when inspecting a virtual environment.
    #[error(transparent)]
    VirtualEnv(#[from] crate::virtualenv::Error),

    /// An error was encountered when using the `py` launcher on Windows.
    #[error(transparent)]
    PyLauncher(#[from] crate::py_launcher::Error),

    /// An invalid version request was given
    #[error("Invalid version request: {0}")]
    InvalidVersionRequest(String),

    // TODO(zanieb): Is this error case necessary still? We should probably drop it.
    #[error("Interpreter discovery for `{0}` requires `{1}` but only {2} is allowed")]
    SourceNotAllowed(PythonRequest, PythonSource, PythonPreference),
}

/// Lazily iterate over Python executables in mutable environments.
///
/// The following sources are supported:
///
/// - Spawning interpreter (via `UV_INTERNAL__PARENT_INTERPRETER`)
/// - Active virtual environment (via `VIRTUAL_ENV`)
/// - Active conda environment (via `CONDA_PREFIX`)
/// - Discovered virtual environment (e.g. `.venv` in a parent directory)
///
/// Notably, "system" environments are excluded. See [`python_executables_from_installed`].
fn python_executables_from_environments<'a>(
) -> impl Iterator<Item = Result<(PythonSource, PathBuf), Error>> + 'a {
    let from_parent_interpreter = std::iter::once_with(|| {
        std::env::var_os("UV_INTERNAL__PARENT_INTERPRETER")
            .into_iter()
            .map(|path| Ok((PythonSource::ParentInterpreter, PathBuf::from(path))))
    })
    .flatten();

    let from_virtual_environment = std::iter::once_with(|| {
        virtualenv_from_env()
            .into_iter()
            .map(virtualenv_python_executable)
            .map(|path| Ok((PythonSource::ActiveEnvironment, path)))
    })
    .flatten();

    let from_conda_environment = std::iter::once_with(|| {
        conda_prefix_from_env()
            .into_iter()
            .map(virtualenv_python_executable)
            .map(|path| Ok((PythonSource::CondaPrefix, path)))
    })
    .flatten();

    let from_discovered_environment = std::iter::once_with(|| {
        virtualenv_from_working_dir()
            .map(|path| {
                path.map(virtualenv_python_executable)
                    .map(|path| (PythonSource::DiscoveredEnvironment, path))
                    .into_iter()
            })
            .map_err(Error::from)
    })
    .flatten_ok();

    from_parent_interpreter
        .chain(from_virtual_environment)
        .chain(from_conda_environment)
        .chain(from_discovered_environment)
}

/// Lazily iterate over Python executables installed on the system.
///
/// The following sources are supported:
///
/// - Managed Python installations (e.g. `uv python install`)
/// - The search path (i.e. `PATH`)
/// - The `py` launcher (Windows only)
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
    version: Option<&'a VersionRequest>,
    implementation: Option<&'a ImplementationName>,
    preference: PythonPreference,
) -> Box<dyn Iterator<Item = Result<(PythonSource, PathBuf), Error>> + 'a> {
    let from_managed_installations = std::iter::once_with(move || {
        ManagedPythonInstallations::from_settings()
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
                        version.is_none()
                            || version.is_some_and(|version| {
                                version.matches_version(&installation.version())
                            })
                    })
                    .inspect(|installation| debug!("Found managed installation `{installation}`"))
                    .map(|installation| (PythonSource::Managed, installation.executable())))
            })
    })
    .flatten_ok();

    let from_search_path = std::iter::once_with(move || {
        python_executables_from_search_path(version, implementation)
            .map(|path| Ok((PythonSource::SearchPath, path)))
    })
    .flatten();

    // TODO(konstin): Implement <https://peps.python.org/pep-0514/> to read python installations from the registry instead.
    let from_py_launcher = std::iter::once_with(move || {
        (cfg!(windows) && env::var_os("UV_TEST_PYTHON_PATH").is_none())
            .then(|| {
                py_list_paths()
                    .map(|entries|
                // We can avoid querying the interpreter using versions from the py launcher output unless a patch is requested
                entries.into_iter().filter(move |entry|
                    version.is_none() || version.is_some_and(|version|
                        version.has_patch() || version.matches_major_minor(entry.major, entry.minor)
                    )
                )
                .map(|entry| (PythonSource::PyLauncher, entry.executable_path)))
                    .map_err(Error::from)
            })
            .into_iter()
            .flatten_ok()
    })
    .flatten();

    match preference {
        PythonPreference::OnlyManaged => Box::new(from_managed_installations),
        PythonPreference::Managed | PythonPreference::Installed => Box::new(
            from_managed_installations
                .chain(from_search_path)
                .chain(from_py_launcher),
        ),
        PythonPreference::System => Box::new(
            from_search_path
                .chain(from_py_launcher)
                .chain(from_managed_installations),
        ),
        PythonPreference::OnlySystem => Box::new(from_search_path.chain(from_py_launcher)),
    }
}

/// Lazily iterate over all discoverable Python executables.
///
/// Note that Python executables may be excluded by the given [`EnvironmentPreference`] and [`PythonPreference`].
///
/// See [`python_executables_from_installed`] and [`python_executables_from_environments`]
/// for more information on discovery.
fn python_executables<'a>(
    version: Option<&'a VersionRequest>,
    implementation: Option<&'a ImplementationName>,
    environments: EnvironmentPreference,
    preference: PythonPreference,
) -> Box<dyn Iterator<Item = Result<(PythonSource, PathBuf), Error>> + 'a> {
    let from_environments = python_executables_from_environments();
    let from_installed = python_executables_from_installed(version, implementation, preference);

    match environments {
        EnvironmentPreference::OnlyVirtual => Box::new(from_environments),
        EnvironmentPreference::ExplicitSystem | EnvironmentPreference::Any => {
            Box::new(from_environments.chain(from_installed))
        }
        EnvironmentPreference::OnlySystem => Box::new(from_installed),
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
    version: Option<&'a VersionRequest>,
    implementation: Option<&'a ImplementationName>,
) -> impl Iterator<Item = PathBuf> + 'a {
    // `UV_TEST_PYTHON_PATH` can be used to override `PATH` to limit Python executable availability in the test suite
    let search_path =
        env::var_os("UV_TEST_PYTHON_PATH").unwrap_or(env::var_os("PATH").unwrap_or_default());

    let version_request = version.unwrap_or(&VersionRequest::Any);
    let possible_names: Vec<_> = version_request.possible_names(implementation).collect();

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
                .chain(find_all_minor(implementation, version_request, &dir_clone))
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
        VersionRequest::Any | VersionRequest::Major(_) | VersionRequest::Range(_) => {
            let regex = if let Some(implementation) = implementation {
                Regex::new(&format!(
                    r"^({}|python3)\.(?<minor>\d\d?){}$",
                    regex::escape(&implementation.to_string()),
                    regex::escape(EXE_SUFFIX)
                ))
                .unwrap()
            } else {
                Regex::new(&format!(
                    r"^python3\.(?<minor>\d\d?){}$",
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
        VersionRequest::MajorMinor(_, _) | VersionRequest::MajorMinorPatch(_, _, _) => {
            Either::Right(iter::empty())
        }
    }
}

/// Lazily iterate over all discoverable Python interpreters.
///
/// Note interpreters may be excluded by the given [`EnvironmentPreference`] and [`PythonPreference`].
///
/// See [`python_executables`] for more information on discovery.
fn python_interpreters<'a>(
    version: Option<&'a VersionRequest>,
    implementation: Option<&'a ImplementationName>,
    environments: EnvironmentPreference,
    preference: PythonPreference,
    cache: &'a Cache,
) -> impl Iterator<Item = Result<(PythonSource, Interpreter), Error>> + 'a {
    python_interpreters_from_executables(
        python_executables(version, implementation, environments, preference),
        cache,
    )
    .filter(move |result| result_satisfies_environment_preference(result, environments))
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
            .map_err(Error::from)
            .inspect_err(|err| debug!("{err}")),
        Err(err) => Err(err),
    })
}

/// Returns true if a Python interpreter matches the [`EnvironmentPreference`].
fn satisfies_environment_preference(
    source: PythonSource,
    interpreter: &Interpreter,
    preference: EnvironmentPreference,
) -> bool {
    match (
        preference,
        // Conda environments are not conformant virtual environments but we treat them as such
        interpreter.is_virtualenv() || matches!(source, PythonSource::CondaPrefix),
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

/// Utility for applying [`satisfies_environment_preference`] to a result type.
fn result_satisfies_environment_preference(
    result: &Result<(PythonSource, Interpreter), Error>,
    preference: EnvironmentPreference,
) -> bool {
    result.as_ref().ok().map_or(true, |(source, interpreter)| {
        satisfies_environment_preference(*source, interpreter, preference)
    })
}

/// Check if an encountered error is critical and should stop discovery.
///
/// Returns false when an error could be due to a faulty Python installation and we should continue searching for a working one.
impl Error {
    pub fn is_critical(&self) -> bool {
        match self {
            // When querying the Python interpreter fails, we will only raise errors that demonstrate that something is broken
            // If the Python interpreter returned a bad response, we'll continue searching for one that works
            Error::Query(err) => match err {
                InterpreterError::Encode(_)
                | InterpreterError::Io(_)
                | InterpreterError::SpawnFailed { .. } => true,
                InterpreterError::QueryScript { path, .. }
                | InterpreterError::UnexpectedResponse { path, .. }
                | InterpreterError::StatusCode { path, .. } => {
                    debug!("Skipping bad interpreter at {}: {err}", path.display());
                    false
                }
                InterpreterError::NotFound(path) => {
                    trace!("Skipping missing interpreter at {}", path.display());
                    false
                }
            },
            // Ignore `py` if it's not installed
            Error::PyLauncher(py_launcher::Error::NotFound) => {
                debug!("The `py` launcher could not be found to query for Python versions");
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
    Ok(PythonInstallation {
        source: PythonSource::ProvidedPath,
        interpreter: Interpreter::query(executable, cache)?,
    })
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
    match request {
        PythonRequest::File(path) => Box::new(std::iter::once({
            if preference.allows(PythonSource::ProvidedPath) {
                debug!("Checking for Python interpreter at {request}");
                match python_installation_from_executable(path, cache) {
                    Ok(installation) => Ok(FindPythonResult::Ok(installation)),
                    Err(InterpreterError::NotFound(_)) => {
                        Ok(FindPythonResult::Err(PythonNotFound {
                            request: request.clone(),
                            python_preference: preference,
                            environment_preference: environments,
                        }))
                    }
                    Err(err) => Err(err.into()),
                }
            } else {
                Err(Error::SourceNotAllowed(
                    request.clone(),
                    PythonSource::ProvidedPath,
                    preference,
                ))
            }
        })),
        PythonRequest::Directory(path) => Box::new(std::iter::once({
            debug!("Checking for Python interpreter in {request}");
            if preference.allows(PythonSource::ProvidedPath) {
                debug!("Checking for Python interpreter at {request}");
                match python_installation_from_directory(path, cache) {
                    Ok(installation) => Ok(FindPythonResult::Ok(installation)),
                    Err(InterpreterError::NotFound(_)) => {
                        Ok(FindPythonResult::Err(PythonNotFound {
                            request: request.clone(),
                            python_preference: preference,
                            environment_preference: environments,
                        }))
                    }
                    Err(err) => Err(err.into()),
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
            debug!("Searching for Python interpreter with {request}");
            if preference.allows(PythonSource::SearchPath) {
                debug!("Checking for Python interpreter at {request}");
                Box::new(
                    python_interpreters_with_executable_name(name, cache).map(|result| {
                        result
                            .map(PythonInstallation::from_tuple)
                            .map(FindPythonResult::Ok)
                    }),
                )
            } else {
                Box::new(std::iter::once(Err(Error::SourceNotAllowed(
                    request.clone(),
                    PythonSource::SearchPath,
                    preference,
                ))))
            }
        }
        PythonRequest::Any => Box::new({
            debug!("Searching for Python interpreter in {preference}");
            python_interpreters(None, None, environments, preference, cache).map(|result| {
                result
                    .map(PythonInstallation::from_tuple)
                    .map(FindPythonResult::Ok)
            })
        }),
        PythonRequest::Version(version) => Box::new({
            debug!("Searching for {request} in {preference}");
            python_interpreters(Some(version), None, environments, preference, cache)
                .filter(|result| match result {
                    Err(_) => true,
                    Ok((_source, interpreter)) => version.matches_interpreter(interpreter),
                })
                .map(|result| {
                    result
                        .map(PythonInstallation::from_tuple)
                        .map(FindPythonResult::Ok)
                })
        }),
        PythonRequest::Implementation(implementation) => Box::new({
            debug!("Searching for a {request} interpreter in {preference}");
            python_interpreters(None, Some(implementation), environments, preference, cache)
                .filter(|result| match result {
                    Err(_) => true,
                    Ok((_source, interpreter)) => interpreter
                        .implementation_name()
                        .eq_ignore_ascii_case(implementation.into()),
                })
                .map(|result| {
                    result
                        .map(PythonInstallation::from_tuple)
                        .map(FindPythonResult::Ok)
                })
        }),
        PythonRequest::ImplementationVersion(implementation, version) => Box::new({
            debug!("Searching for {request} in {preference}");
            python_interpreters(
                Some(version),
                Some(implementation),
                environments,
                preference,
                cache,
            )
            .filter(|result| match result {
                Err(_) => true,
                Ok((_source, interpreter)) => {
                    version.matches_interpreter(interpreter)
                        && interpreter
                            .implementation_name()
                            .eq_ignore_ascii_case(implementation.into())
                }
            })
            .map(|result| {
                result
                    .map(PythonInstallation::from_tuple)
                    .map(FindPythonResult::Ok)
            })
        }),
        PythonRequest::Key(request) => Box::new({
            debug!("Searching for {request} in {preference}");
            python_interpreters(
                request.version(),
                request.implementation(),
                environments,
                preference,
                cache,
            )
            .filter(|result| match result {
                Err(_) => true,
                Ok((_source, interpreter)) => request.satisfied_by_interpreter(interpreter),
            })
            .map(|result| {
                result
                    .map(PythonInstallation::from_tuple)
                    .map(FindPythonResult::Ok)
            })
        }),
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
    let mut installations = find_python_installations(request, environments, preference, cache);
    if let Some(result) = installations.find(|result| {
        // Return the first critical discovery error or result
        result.as_ref().err().map_or(true, Error::is_critical)
    }) {
        result
    } else {
        Ok(FindPythonResult::Err(PythonNotFound {
            request: request.clone(),
            environment_preference: environments,
            python_preference: preference,
        }))
    }
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

    // First, check for an exact match (or the first available version if no Python versfion was provided)
    debug!("Looking for exact match for request {request}");
    let result = find_python_installation(request, environments, preference, cache)?;
    if let Ok(ref installation) = result {
        warn_on_unsupported_python(installation.interpreter());
        return Ok(result);
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
        let result = find_python_installation(&request, environments, preference, cache)?;
        if let Ok(ref installation) = result {
            warn_on_unsupported_python(installation.interpreter());
            return Ok(result);
        }
    }

    // If a Python version was requested but cannot be fulfilled, just take any version
    debug!("Looking for Python installation with any version");
    let request = PythonRequest::Any;
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

    // Ex) `python.exe` or `python3.exe`
    if !components
        .next()
        .and_then(|component| component.as_os_str().to_str())
        .is_some_and(|component| component == "python.exe" || component == "python3.exe")
    {
        return false;
    }

    // Ex) `WindowsApps`
    if !components
        .next()
        .is_some_and(|component| component.as_os_str() == "WindowsApps")
    {
        return false;
    }

    // Ex) `Microsoft`
    if !components
        .next()
        .is_some_and(|component| component.as_os_str() == "Microsoft")
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
            0,
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

impl PythonRequest {
    /// Create a request from a string.
    ///
    /// This cannot fail, which means weird inputs will be parsed as [`PythonRequest::File`] or [`PythonRequest::ExecutableName`].
    pub fn parse(value: &str) -> Self {
        // e.g. `any`
        if value.eq_ignore_ascii_case("any") {
            return Self::Any;
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
        for implementation in ImplementationName::possible_names() {
            if let Some(remainder) = value
                .to_ascii_lowercase()
                .strip_prefix(Into::<&str>::into(implementation))
            {
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
            PythonRequest::Any => true,
            PythonRequest::Version(version_request) => {
                version_request.matches_interpreter(interpreter)
            }
            PythonRequest::Directory(directory) => {
                // `sys.prefix` points to the venv root.
                is_same_executable(directory, interpreter.sys_prefix())
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

    pub(crate) fn is_explicit_system(&self) -> bool {
        matches!(self, Self::File(_) | Self::Directory(_))
    }

    /// Serialize the request to a canonical representation.
    ///
    /// [`Self::parse`] should always return the same request when given the output of this method.
    pub fn to_canonical_string(&self) -> String {
        match self {
            Self::Any => "any".to_string(),
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
    pub fn is_managed(&self) -> bool {
        matches!(self, Self::Managed)
    }
}

impl PythonPreference {
    fn allows(self, source: PythonSource) -> bool {
        // If not dealing with a system interpreter source, we don't care about the preference
        if !matches!(
            source,
            PythonSource::Managed | PythonSource::SearchPath | PythonSource::PyLauncher
        ) {
            return true;
        }

        match self {
            PythonPreference::OnlyManaged => matches!(source, PythonSource::Managed),
            Self::Managed | Self::System | Self::Installed => matches!(
                source,
                PythonSource::Managed | PythonSource::SearchPath | PythonSource::PyLauncher
            ),
            PythonPreference::OnlySystem => {
                matches!(source, PythonSource::SearchPath | PythonSource::PyLauncher)
            }
        }
    }

    /// Return a default [`PythonPreference`] based on the environment and preview mode.
    pub fn default_from(preview: PreviewMode) -> Self {
        if env::var_os("UV_TEST_PYTHON_PATH").is_some() {
            debug!("Only considering system interpreters due to `UV_TEST_PYTHON_PATH`");
            Self::OnlySystem
        } else if preview.is_enabled() {
            Self::default()
        } else {
            Self::OnlySystem
        }
    }

    pub(crate) fn allows_managed(self) -> bool {
        matches!(self, Self::Managed | Self::OnlyManaged | Self::Installed)
    }
}

impl PythonFetch {
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

impl VersionRequest {
    pub(crate) fn default_names(&self) -> [Option<Cow<'static, str>>; 4] {
        let (python, python3, extension) = if cfg!(windows) {
            (
                Cow::Borrowed("python.exe"),
                Cow::Borrowed("python3.exe"),
                ".exe",
            )
        } else {
            (Cow::Borrowed("python"), Cow::Borrowed("python3"), "")
        };

        match self {
            Self::Any | Self::Range(_) => [Some(python3), Some(python), None, None],
            Self::Major(major) => [
                Some(Cow::Owned(format!("python{major}{extension}"))),
                Some(python),
                None,
                None,
            ],
            Self::MajorMinor(major, minor) => [
                Some(Cow::Owned(format!("python{major}.{minor}{extension}"))),
                Some(Cow::Owned(format!("python{major}{extension}"))),
                Some(python),
                None,
            ],
            Self::MajorMinorPatch(major, minor, patch) => [
                Some(Cow::Owned(format!(
                    "python{major}.{minor}.{patch}{extension}",
                ))),
                Some(Cow::Owned(format!("python{major}.{minor}{extension}"))),
                Some(Cow::Owned(format!("python{major}{extension}"))),
                Some(python),
            ],
        }
    }

    pub(crate) fn possible_names<'a>(
        &'a self,
        implementation: Option<&'a ImplementationName>,
    ) -> impl Iterator<Item = Cow<'static, str>> + 'a {
        implementation
            .into_iter()
            .flat_map(move |implementation| {
                let extension = std::env::consts::EXE_SUFFIX;
                let name: &str = implementation.into();
                let (python, python3) = if extension.is_empty() {
                    (Cow::Borrowed(name), Cow::Owned(format!("{name}3")))
                } else {
                    (
                        Cow::Owned(format!("{name}{extension}")),
                        Cow::Owned(format!("{name}3{extension}")),
                    )
                };

                match self {
                    Self::Any | Self::Range(_) => [Some(python3), Some(python), None, None],
                    Self::Major(major) => [
                        Some(Cow::Owned(format!("{name}{major}{extension}"))),
                        Some(python),
                        None,
                        None,
                    ],
                    Self::MajorMinor(major, minor) => [
                        Some(Cow::Owned(format!("{name}{major}.{minor}{extension}"))),
                        Some(Cow::Owned(format!("{name}{major}{extension}"))),
                        Some(python),
                        None,
                    ],
                    Self::MajorMinorPatch(major, minor, patch) => [
                        Some(Cow::Owned(format!(
                            "{name}{major}.{minor}.{patch}{extension}",
                        ))),
                        Some(Cow::Owned(format!("{name}{major}.{minor}{extension}"))),
                        Some(Cow::Owned(format!("{name}{major}{extension}"))),
                        Some(python),
                    ],
                }
            })
            .chain(self.default_names())
            .flatten()
    }

    /// Check if a interpreter matches the requested Python version.
    pub(crate) fn matches_interpreter(&self, interpreter: &Interpreter) -> bool {
        match self {
            Self::Any => true,
            Self::Major(major) => interpreter.python_major() == *major,
            Self::MajorMinor(major, minor) => {
                (interpreter.python_major(), interpreter.python_minor()) == (*major, *minor)
            }
            Self::MajorMinorPatch(major, minor, patch) => {
                (
                    interpreter.python_major(),
                    interpreter.python_minor(),
                    interpreter.python_patch(),
                ) == (*major, *minor, *patch)
            }
            Self::Range(specifiers) => specifiers.contains(interpreter.python_version()),
        }
    }

    pub(crate) fn matches_version(&self, version: &PythonVersion) -> bool {
        match self {
            Self::Any => true,
            Self::Major(major) => version.major() == *major,
            Self::MajorMinor(major, minor) => {
                (version.major(), version.minor()) == (*major, *minor)
            }
            Self::MajorMinorPatch(major, minor, patch) => {
                (version.major(), version.minor(), version.patch())
                    == (*major, *minor, Some(*patch))
            }
            Self::Range(specifiers) => specifiers.contains(&version.version),
        }
    }

    fn matches_major_minor(&self, major: u8, minor: u8) -> bool {
        match self {
            Self::Any => true,
            Self::Major(self_major) => *self_major == major,
            Self::MajorMinor(self_major, self_minor) => {
                (*self_major, *self_minor) == (major, minor)
            }
            Self::MajorMinorPatch(self_major, self_minor, _) => {
                (*self_major, *self_minor) == (major, minor)
            }
            Self::Range(specifiers) => {
                specifiers.contains(&Version::new([u64::from(major), u64::from(minor)]))
            }
        }
    }

    pub(crate) fn matches_major_minor_patch(&self, major: u8, minor: u8, patch: u8) -> bool {
        match self {
            Self::Any => true,
            Self::Major(self_major) => *self_major == major,
            Self::MajorMinor(self_major, self_minor) => {
                (*self_major, *self_minor) == (major, minor)
            }
            Self::MajorMinorPatch(self_major, self_minor, self_patch) => {
                (*self_major, *self_minor, *self_patch) == (major, minor, patch)
            }
            Self::Range(specifiers) => specifiers.contains(&Version::new([
                u64::from(major),
                u64::from(minor),
                u64::from(patch),
            ])),
        }
    }

    /// Return true if a patch version is present in the request.
    fn has_patch(&self) -> bool {
        match self {
            Self::Any => false,
            Self::Major(..) => false,
            Self::MajorMinor(..) => false,
            Self::MajorMinorPatch(..) => true,
            Self::Range(_) => false,
        }
    }

    /// Return a new [`VersionRequest`] without the patch version if possible.
    ///
    /// If the patch version is not present, it is returned unchanged.
    #[must_use]
    fn without_patch(self) -> Self {
        match self {
            Self::Any => Self::Any,
            Self::Major(major) => Self::Major(major),
            Self::MajorMinor(major, minor) => Self::MajorMinor(major, minor),
            Self::MajorMinorPatch(major, minor, _) => Self::MajorMinor(major, minor),
            Self::Range(_) => self,
        }
    }
}

impl FromStr for VersionRequest {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        fn parse_nosep(s: &str) -> Option<VersionRequest> {
            let mut chars = s.chars();
            let major = chars.next()?.to_digit(10)?.try_into().ok()?;
            if chars.as_str().is_empty() {
                return Some(VersionRequest::Major(major));
            }
            let minor = chars.as_str().parse::<u8>().ok()?;
            Some(VersionRequest::MajorMinor(major, minor))
        }

        // e.g. `3`, `38`, `312`
        if let Some(request) = parse_nosep(s) {
            Ok(request)
        }
        // e.g. `3.12.1`
        else if let Ok(versions) = s
            .splitn(3, '.')
            .map(str::parse::<u8>)
            .collect::<Result<Vec<_>, _>>()
        {
            let selector = match versions.as_slice() {
                // e.g. `3`
                [major] => VersionRequest::Major(*major),
                // e.g. `3.10`
                [major, minor] => VersionRequest::MajorMinor(*major, *minor),
                // e.g. `3.10.4`
                [major, minor, patch] => VersionRequest::MajorMinorPatch(*major, *minor, *patch),
                _ => unreachable!(),
            };

            Ok(selector)

        // e.g. `>=3.12.1,<3.12`
        } else if let Ok(specifiers) = VersionSpecifiers::from_str(s) {
            if specifiers.is_empty() {
                return Err(Error::InvalidVersionRequest(s.to_string()));
            }
            Ok(Self::Range(specifiers))
        } else {
            Err(Error::InvalidVersionRequest(s.to_string()))
        }
    }
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
            Self::Any => f.write_str("default"),
            Self::Major(major) => write!(f, "{major}"),
            Self::MajorMinor(major, minor) => write!(f, "{major}.{minor}"),
            Self::MajorMinorPatch(major, minor, patch) => {
                write!(f, "{major}.{minor}.{patch}")
            }
            Self::Range(specifiers) => write!(f, "{specifiers}"),
        }
    }
}

impl fmt::Display for PythonRequest {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
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
            Self::CondaPrefix => f.write_str("conda prefix"),
            Self::DiscoveredEnvironment => f.write_str("virtual environment"),
            Self::SearchPath => f.write_str("search path"),
            Self::PyLauncher => f.write_str("`py` launcher output"),
            Self::Managed => f.write_str("managed installations"),
            Self::ParentInterpreter => f.write_str("parent interpreter"),
        }
    }
}

impl PythonPreference {
    /// Return the sources that are considered when searching for a Python interpreter.
    fn sources(self) -> &'static [&'static str] {
        match self {
            Self::OnlyManaged => &["managed installations"],
            Self::Managed | Self::Installed | Self::System => {
                if cfg!(windows) {
                    &["managed installations", "system path", "`py` launcher"]
                } else {
                    &["managed installations", "system path"]
                }
            }
            Self::OnlySystem => {
                if cfg!(windows) {
                    &["system path", "`py` launcher"]
                } else {
                    &["system path"]
                }
            }
        }
    }
}

impl fmt::Display for PythonPreference {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", conjunction(self.sources()))
    }
}

impl fmt::Display for PythonNotFound {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let sources = match self.environment_preference {
            EnvironmentPreference::Any => conjunction(
                &["virtual environments"]
                    .into_iter()
                    .chain(self.python_preference.sources().iter().copied())
                    .collect::<Vec<_>>(),
            ),
            EnvironmentPreference::ExplicitSystem => {
                if self.request.is_explicit_system() {
                    conjunction(&["virtual", "system environment"])
                } else {
                    conjunction(&["virtual environment"])
                }
            }
            EnvironmentPreference::OnlySystem => conjunction(self.python_preference.sources()),
            EnvironmentPreference::OnlyVirtual => conjunction(&["virtual environments"]),
        };

        match self.request {
            PythonRequest::Any => {
                write!(f, "No interpreter found in {sources}")
            }
            _ => {
                write!(f, "No interpreter found for {} in {sources}", self.request)
            }
        }
    }
}

/// Join a series of items with `or` separators, making use of commas when necessary.
fn conjunction(items: &[&str]) -> String {
    match items.len() {
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

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, str::FromStr};

    use assert_fs::{prelude::*, TempDir};
    use test_log::test;

    use crate::{
        discovery::{PythonRequest, VersionRequest},
        implementation::ImplementationName,
    };

    use super::Error;

    #[test]
    fn interpreter_request_from_str() {
        assert_eq!(PythonRequest::parse("any"), PythonRequest::Any);
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
            PythonRequest::parse("foo"),
            PythonRequest::ExecutableName("foo".to_string())
        );
        assert_eq!(
            PythonRequest::parse("cpython"),
            PythonRequest::Implementation(ImplementationName::CPython)
        );
        assert_eq!(
            PythonRequest::parse("cpython3.12.2"),
            PythonRequest::ImplementationVersion(
                ImplementationName::CPython,
                VersionRequest::from_str("3.12.2").unwrap()
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
                VersionRequest::from_str("3.10").unwrap()
            )
        );
        assert_eq!(
            PythonRequest::parse("pp310"),
            PythonRequest::ImplementationVersion(
                ImplementationName::PyPy,
                VersionRequest::from_str("3.10").unwrap()
            )
        );
        assert_eq!(
            PythonRequest::parse("graalpy3.10"),
            PythonRequest::ImplementationVersion(
                ImplementationName::GraalPy,
                VersionRequest::from_str("3.10").unwrap()
            )
        );
        assert_eq!(
            PythonRequest::parse("gp310"),
            PythonRequest::ImplementationVersion(
                ImplementationName::GraalPy,
                VersionRequest::from_str("3.10").unwrap()
            )
        );
        assert_eq!(
            PythonRequest::parse("cp38"),
            PythonRequest::ImplementationVersion(
                ImplementationName::CPython,
                VersionRequest::from_str("3.8").unwrap()
            )
        );
        assert_eq!(
            PythonRequest::parse("pypy@3.10"),
            PythonRequest::ImplementationVersion(
                ImplementationName::PyPy,
                VersionRequest::from_str("3.10").unwrap()
            )
        );
        assert_eq!(
            PythonRequest::parse("pypy310"),
            PythonRequest::ImplementationVersion(
                ImplementationName::PyPy,
                VersionRequest::from_str("3.10").unwrap()
            )
        );
        assert_eq!(
            PythonRequest::parse("graalpy@3.10"),
            PythonRequest::ImplementationVersion(
                ImplementationName::GraalPy,
                VersionRequest::from_str("3.10").unwrap()
            )
        );
        assert_eq!(
            PythonRequest::parse("graalpy310"),
            PythonRequest::ImplementationVersion(
                ImplementationName::GraalPy,
                VersionRequest::from_str("3.10").unwrap()
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
    }

    #[test]
    fn interpreter_request_to_canonical_string() {
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
                VersionRequest::from_str("3.12.2").unwrap()
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
                VersionRequest::from_str("3.10").unwrap()
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
                VersionRequest::from_str("3.10").unwrap()
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
            VersionRequest::Major(3)
        );
        assert_eq!(
            VersionRequest::from_str("3.12").unwrap(),
            VersionRequest::MajorMinor(3, 12)
        );
        assert_eq!(
            VersionRequest::from_str("3.12.1").unwrap(),
            VersionRequest::MajorMinorPatch(3, 12, 1)
        );
        assert!(VersionRequest::from_str("1.foo.1").is_err());
        assert_eq!(
            VersionRequest::from_str("3").unwrap(),
            VersionRequest::Major(3)
        );
        assert_eq!(
            VersionRequest::from_str("38").unwrap(),
            VersionRequest::MajorMinor(3, 8)
        );
        assert_eq!(
            VersionRequest::from_str("312").unwrap(),
            VersionRequest::MajorMinor(3, 12)
        );
        assert_eq!(
            VersionRequest::from_str("3100").unwrap(),
            VersionRequest::MajorMinor(3, 100)
        );
        assert!(
            // Test for overflow
            matches!(
                VersionRequest::from_str("31000"),
                Err(Error::InvalidVersionRequest(_))
            )
        );
    }
}
