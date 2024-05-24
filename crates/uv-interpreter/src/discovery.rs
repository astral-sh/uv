use itertools::Itertools;
use thiserror::Error;
use tracing::{debug, instrument, trace};
use uv_cache::Cache;
use uv_fs::Simplified;
use uv_warnings::warn_user_once;
use which::which;

use crate::implementation::{ImplementationName, LenientImplementationName};
use crate::interpreter::Error as InterpreterError;
use crate::managed::toolchains_for_current_platform;
use crate::py_launcher::py_list_paths;
use crate::virtualenv::{
    conda_prefix_from_env, virtualenv_from_env, virtualenv_from_working_dir,
    virtualenv_python_executable,
};
use crate::{Interpreter, PythonVersion};
use std::borrow::Cow;

use std::collections::HashSet;
use std::fmt::{self, Formatter};
use std::num::ParseIntError;
use std::{env, io};
use std::{path::Path, path::PathBuf, str::FromStr};

/// A request to find a Python interpreter.
///
/// See [`InterpreterRequest::from_str`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum InterpreterRequest {
    /// Use any discovered Python interpreter
    #[default]
    Any,
    /// A Python version without an implementation name e.g. `3.10`
    Version(VersionRequest),
    /// A path to a directory containing a Python installation, e.g. `.venv`
    Directory(PathBuf),
    /// A path to a Python executable e.g. `~/bin/python`
    File(PathBuf),
    /// The name of a Python executable (i.e. for lookup in the PATH) e.g. `foopython3`
    ExecutableName(String),
    /// A Python implementation without a version e.g. `pypy`
    Implementation(ImplementationName),
    /// A Python implementation name and version e.g. `pypy3.8` or `pypy@3.8`
    ImplementationVersion(ImplementationName, VersionRequest),
}

/// The sources to consider when finding a Python interpreter.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum SourceSelector {
    // Consider all interpreter sources.
    #[default]
    All,
    // Only consider system interpreter sources
    System,
    // Only consider virtual environment sources
    VirtualEnv,
    // Only consider a custom set of sources
    Custom(HashSet<InterpreterSource>),
}

/// A Python interpreter version request.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum VersionRequest {
    #[default]
    Any,
    Major(u8),
    MajorMinor(u8, u8),
    MajorMinorPatch(u8, u8, u8),
}

/// The policy for discovery of "system" Python interpreters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SystemPython {
    /// Only allow a system Python if passed directly i.e. via [`InterpreterSource::ProvidedPath`] or [`InterpreterSource::ParentInterpreter`]
    #[default]
    Explicit,
    /// Do not allow a system Python
    Disallowed,
    /// Allow a system Python to be used if no virtual environment is active.
    Allowed,
    /// Ignore virtual environments and require a system Python.
    Required,
}

/// The result of an interpreter search.
///
/// Returned by [`find_interpreter`].
type InterpreterResult = Result<DiscoveredInterpreter, InterpreterNotFound>;

/// The result of failed interpreter discovery.
///
/// See [`InterpreterResult`].
#[derive(Clone, Debug, Error)]
pub enum InterpreterNotFound {
    /// No Python installations were found.
    NoPythonInstallation(SourceSelector, Option<VersionRequest>),
    /// No Python installations with the requested version were found.
    NoMatchingVersion(SourceSelector, VersionRequest),
    /// No Python installations with the requested implementation name were found.
    NoMatchingImplementation(SourceSelector, ImplementationName),
    /// No Python installations with the requested implementation name and version were found.
    NoMatchingImplementationVersion(SourceSelector, ImplementationName, VersionRequest),
    /// The requested file path does not exist.
    FileNotFound(PathBuf),
    /// The requested directory path does not exist.
    DirectoryNotFound(PathBuf),
    /// No Python executables could be found in the requested directory.
    ExecutableNotFoundInDirectory(PathBuf, PathBuf),
    /// The Python executable name could not be found in the search path (i.e. PATH).
    ExecutableNotFoundInSearchPath(String),
    /// A Python executable was found but is not executable.
    FileNotExecutable(PathBuf),
}

/// The result of successful interpreter discovery.
///
/// See [`InterpreterResult`].
#[derive(Clone, Debug)]
pub struct DiscoveredInterpreter {
    pub(crate) source: InterpreterSource,
    pub(crate) interpreter: Interpreter,
}

/// The source of a discovered Python interpreter.
#[derive(Debug, Clone, PartialEq, Eq, Copy, Hash, PartialOrd, Ord)]
pub enum InterpreterSource {
    /// The interpreter path was provided directly
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
    /// The interpreter was found in the uv toolchain directory
    ManagedToolchain,
    /// The interpreter invoked uv i.e. via `python -m uv ...`
    ParentInterpreter,
    // TODO(zanieb): Add support for fetching the interpreter from a remote source
}

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    IO(#[from] io::Error),

    /// An error was encountering when retrieving interpreter information.
    #[error(transparent)]
    Query(#[from] crate::interpreter::Error),

    /// An error was encountered when interacting with a managed toolchain.
    #[error(transparent)]
    ManagedToolchain(#[from] crate::managed::Error),

    /// An error was encountered when inspecting a virtual environment.
    #[error(transparent)]
    VirtualEnv(#[from] crate::virtualenv::Error),

    /// An error was encountered when using the `py` launcher on Windows.
    #[error(transparent)]
    PyLauncher(#[from] crate::py_launcher::Error),

    #[error("Interpreter discovery for `{0}` requires `{1}` but it is not selected")]
    SourceNotSelected(InterpreterRequest, InterpreterSource),
}

/// Lazily iterate over all discoverable Python executables.
///
/// In order, we look in:
///
/// - The spawning interpreter
/// - The active environment
/// - A discovered environment (e.g. `.venv`)
/// - Installed managed toolchains
/// - The search path (i.e. PATH)
/// - `py` launcher output
///
/// Each location is only queried if the previous location is exhausted.
/// Locations may be omitted using `sources`, sources that are not selected will not be queried.
///
/// If a [`VersionRequest`] is provided, we will skip executables that we know do not satisfy the request
/// and (as discussed in [`python_executables_from_search_path`]) additional version specific executables may
/// be included. However, the caller MUST query the returned executables to ensure they satisfy the request;
/// this function does not guarantee that the executables provide any particular version. See
/// [`find_interpreter`] instead.
fn python_executables<'a>(
    version: Option<&'a VersionRequest>,
    implementation: Option<&'a ImplementationName>,
    sources: &SourceSelector,
) -> impl Iterator<Item = Result<(InterpreterSource, PathBuf), Error>> + 'a {
    // Note we are careful to ensure the iterator chain is lazy to avoid unnecessary work

    // (1) The parent interpreter
    sources.contains(InterpreterSource::ParentInterpreter).then(||
        std::env::var_os("UV_INTERNAL__PARENT_INTERPRETER")
        .into_iter()
        .map(|path| Ok((InterpreterSource::ParentInterpreter, PathBuf::from(path))))
    ).into_iter().flatten()
    // (2) An active virtual environment
    .chain(
        sources.contains(InterpreterSource::ActiveEnvironment).then(||
            virtualenv_from_env()
            .into_iter()
            .map(virtualenv_python_executable)
            .map(|path| Ok((InterpreterSource::ActiveEnvironment, path)))
        ).into_iter().flatten()
    )
    // (3) An active conda environment
    .chain(
        sources.contains(InterpreterSource::CondaPrefix).then(||
            conda_prefix_from_env()
            .into_iter()
            .map(virtualenv_python_executable)
            .map(|path| Ok((InterpreterSource::CondaPrefix, path)))
        ).into_iter().flatten()
    )
    // (4) A discovered environment
    .chain(
        sources.contains(InterpreterSource::DiscoveredEnvironment).then(||
            std::iter::once(
                virtualenv_from_working_dir()
                .map(|path|
                    path
                    .map(virtualenv_python_executable)
                    .map(|path| (InterpreterSource::DiscoveredEnvironment, path))
                    .into_iter()
                )
                .map_err(Error::from)
            ).flatten_ok()
        ).into_iter().flatten()
    )
    // (5) Managed toolchains
    .chain(
        sources.contains(InterpreterSource::ManagedToolchain).then(move ||
            std::iter::once(
                toolchains_for_current_platform()
                .map(|toolchains|
                    // Check that the toolchain version satisfies the request to avoid unnecessary interpreter queries later
                    toolchains.filter(move |toolchain|
                        version.is_none() || version.is_some_and(|version|
                            version.matches_version(toolchain.python_version())
                        )
                    )
                    .map(|toolchain| (InterpreterSource::ManagedToolchain, toolchain.executable()))
                )
                .map_err(Error::from)
            ).flatten_ok()
        ).into_iter().flatten()
    )
    // (6) The search path
    .chain(
        sources.contains(InterpreterSource::SearchPath).then(move ||
            python_executables_from_search_path(version, implementation)
            .map(|path| Ok((InterpreterSource::SearchPath, path))),
        ).into_iter().flatten()
    )
    // (7) The `py` launcher (windows only)
    // TODO(konstin): Implement <https://peps.python.org/pep-0514/> to read python installations from the registry instead.
    .chain(
        (sources.contains(InterpreterSource::PyLauncher) && cfg!(windows)).then(||
            std::iter::once(
                py_list_paths()
                .map(|entries|
                    // We can avoid querying the interpreter using versions from the py launcher output unless a patch is requested
                    entries.into_iter().filter(move |entry|
                        version.is_none() || version.is_some_and(|version|
                            version.has_patch() || version.matches_major_minor(entry.major, entry.minor)
                        )
                    )
                    .map(|entry| (InterpreterSource::PyLauncher, entry.executable_path))
                )
                .map_err(Error::from)
            ).flatten_ok()
        ).into_iter().flatten()
    )
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

    let possible_names: Vec<_> = version
        .unwrap_or(&VersionRequest::Any)
        .possible_names(implementation)
        .collect();

    trace!(
        "Searching PATH for executables: {}",
        possible_names.join(", ")
    );

    // Split and iterate over the paths instead of using `which_all` so we can
    // check multiple names per directory while respecting the search path order
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
                        .collect::<Vec<_>>()
                })
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

/// Lazily iterate over all discoverable Python interpreters.
///
///See [`python_executables`] for more information on discovery.
fn python_interpreters<'a>(
    version: Option<&'a VersionRequest>,
    implementation: Option<&'a ImplementationName>,
    system: SystemPython,
    sources: &SourceSelector,
    cache: &'a Cache,
) -> impl Iterator<Item = Result<(InterpreterSource, Interpreter), Error>> + 'a {
    python_executables(version, implementation, sources)
        .map(|result| match result {
            Ok((source, path)) => Interpreter::query(&path, cache)
                .map(|interpreter| (source, interpreter))
                .inspect(|(source, interpreter)| {
                    debug!(
                        "Found {} {} at `{}` ({source})",
                        LenientImplementationName::from(interpreter.implementation_name()),
                        interpreter.python_full_version(),
                        path.display()
                    );
                })
                .map_err(Error::from)
                .inspect_err(|err| trace!("{err}")),
            Err(err) => Err(err),
        })
        .filter(move |result| match result {
            // Filter the returned interpreters to conform to the system request
            Ok((source, interpreter)) => match (
                system,
                // Conda environments are not conformant virtual environments but we should not treat them as system interpreters
                interpreter.is_virtualenv() || matches!(source, InterpreterSource::CondaPrefix),
            ) {
                (SystemPython::Allowed, _) => true,
                (SystemPython::Explicit, false) => {
                    if matches!(
                        source,
                        InterpreterSource::ProvidedPath | InterpreterSource::ParentInterpreter
                    ) {
                        debug!(
                            "Allowing system Python interpreter at `{}`",
                            interpreter.sys_executable().display()
                        );
                        true
                    } else {
                        debug!(
                            "Ignoring Python interpreter at `{}`: system interpreter not explicit",
                            interpreter.sys_executable().display()
                        );
                        false
                    }
                }
                (SystemPython::Explicit, true) => true,
                (SystemPython::Disallowed, false) => {
                    debug!(
                        "Ignoring Python interpreter at `{}`: system interpreter not allowed",
                        interpreter.sys_executable().display()
                    );
                    false
                }
                (SystemPython::Disallowed, true) => true,
                (SystemPython::Required, true) => {
                    debug!(
                        "Ignoring Python interpreter at `{}`: system interpreter required",
                        interpreter.sys_executable().display()
                    );
                    false
                }
                (SystemPython::Required, false) => true,
            },
            // Do not drop any errors
            Err(_) => true,
        })
}

/// Check if an encountered error should stop discovery.
///
/// Returns false when an error could be due to a faulty interpreter and we should continue searching for a working one.
fn should_stop_discovery(err: &Error) -> bool {
    match err {
        // When querying the interpreter fails, we will only raise errors that demonstrate that something is broken
        // If the interpreter returned a bad response, we'll continue searching for one that works
        Error::Query(err) => match err {
            InterpreterError::Encode(_)
            | InterpreterError::Io(_)
            | InterpreterError::SpawnFailed { .. } => true,
            InterpreterError::QueryScript { path, .. }
            | InterpreterError::UnexpectedResponse { path, .. }
            | InterpreterError::StatusCode { path, .. } => {
                trace!("Skipping bad interpreter at {}", path.display());
                false
            }
        },
        _ => true,
    }
}

/// Find an interpreter that satisfies the given request.
///
/// If an error is encountered while locating or inspecting a candidate interpreter,
/// the error will raised instead of attempting further candidates.
pub fn find_interpreter(
    request: &InterpreterRequest,
    system: SystemPython,
    sources: &SourceSelector,
    cache: &Cache,
) -> Result<InterpreterResult, Error> {
    let result = match request {
        InterpreterRequest::File(path) => {
            debug!("Checking for Python interpreter at {request}");
            if !sources.contains(InterpreterSource::ProvidedPath) {
                return Err(Error::SourceNotSelected(
                    request.clone(),
                    InterpreterSource::ProvidedPath,
                ));
            }
            if !path.try_exists()? {
                return Ok(InterpreterResult::Err(InterpreterNotFound::FileNotFound(
                    path.clone(),
                )));
            }
            DiscoveredInterpreter {
                source: InterpreterSource::ProvidedPath,
                interpreter: Interpreter::query(path, cache)?,
            }
        }
        InterpreterRequest::Directory(path) => {
            debug!("Checking for Python interpreter in {request}");
            if !sources.contains(InterpreterSource::ProvidedPath) {
                return Err(Error::SourceNotSelected(
                    request.clone(),
                    InterpreterSource::ProvidedPath,
                ));
            }
            if !path.try_exists()? {
                return Ok(InterpreterResult::Err(InterpreterNotFound::FileNotFound(
                    path.clone(),
                )));
            }
            let executable = virtualenv_python_executable(path);
            if !executable.try_exists()? {
                return Ok(InterpreterResult::Err(
                    InterpreterNotFound::ExecutableNotFoundInDirectory(path.clone(), executable),
                ));
            }
            DiscoveredInterpreter {
                source: InterpreterSource::ProvidedPath,
                interpreter: Interpreter::query(executable, cache)?,
            }
        }
        InterpreterRequest::ExecutableName(name) => {
            debug!("Searching for Python interpreter with {request}");
            if !sources.contains(InterpreterSource::SearchPath) {
                return Err(Error::SourceNotSelected(
                    request.clone(),
                    InterpreterSource::SearchPath,
                ));
            }
            let Some(executable) = which(name).ok() else {
                return Ok(InterpreterResult::Err(
                    InterpreterNotFound::ExecutableNotFoundInSearchPath(name.clone()),
                ));
            };
            DiscoveredInterpreter {
                source: InterpreterSource::SearchPath,
                interpreter: Interpreter::query(executable, cache)?,
            }
        }
        InterpreterRequest::Implementation(implementation) => {
            debug!("Searching for a {request} interpreter in {sources}");
            let Some((source, interpreter)) =
                python_interpreters(None, Some(implementation), system, sources, cache)
                    .find(|result| {
                        match result {
                            // Return the first critical error or matching interpreter
                            Err(err) => should_stop_discovery(err),
                            Ok((_source, interpreter)) => {
                                interpreter.implementation_name() == implementation.as_str()
                            }
                        }
                    })
                    .transpose()?
            else {
                return Ok(InterpreterResult::Err(
                    InterpreterNotFound::NoMatchingImplementation(sources.clone(), *implementation),
                ));
            };
            DiscoveredInterpreter {
                source,
                interpreter,
            }
        }
        InterpreterRequest::ImplementationVersion(implementation, version) => {
            debug!("Searching for {request} in {sources}");
            let Some((source, interpreter)) =
                python_interpreters(Some(version), Some(implementation), system, sources, cache)
                    .find(|result| {
                        match result {
                            // Return the first critical error or matching interpreter
                            Err(err) => should_stop_discovery(err),
                            Ok((_source, interpreter)) => {
                                version.matches_interpreter(interpreter)
                                    && interpreter.implementation_name() == implementation.as_str()
                            }
                        }
                    })
                    .transpose()?
            else {
                // TODO(zanieb): Peek if there are any interpreters with the requested implementation
                //               to improve the error message e.g. using `NoMatchingImplementation` instead
                return Ok(InterpreterResult::Err(
                    InterpreterNotFound::NoMatchingImplementationVersion(
                        sources.clone(),
                        *implementation,
                        *version,
                    ),
                ));
            };
            DiscoveredInterpreter {
                source,
                interpreter,
            }
        }
        InterpreterRequest::Any => {
            debug!("Searching for Python interpreter in {sources}");
            let Some((source, interpreter)) =
                python_interpreters(None, None, system, sources, cache)
                    .find(|result| {
                        match result {
                            // Return the first critical error or interpreter
                            Err(err) => should_stop_discovery(err),
                            Ok(_) => true,
                        }
                    })
                    .transpose()?
            else {
                return Ok(InterpreterResult::Err(
                    InterpreterNotFound::NoPythonInstallation(sources.clone(), None),
                ));
            };
            DiscoveredInterpreter {
                source,
                interpreter,
            }
        }
        InterpreterRequest::Version(version) => {
            debug!("Searching for {request} in {sources}");
            let Some((source, interpreter)) =
                python_interpreters(Some(version), None, system, sources, cache)
                    .find(|result| {
                        match result {
                            // Return the first critical error or matching interpreter
                            Err(err) => should_stop_discovery(err),
                            Ok((_source, interpreter)) => version.matches_interpreter(interpreter),
                        }
                    })
                    .transpose()?
            else {
                let err = if matches!(version, VersionRequest::Any) {
                    InterpreterNotFound::NoPythonInstallation(sources.clone(), Some(*version))
                } else {
                    InterpreterNotFound::NoMatchingVersion(sources.clone(), *version)
                };
                return Ok(InterpreterResult::Err(err));
            };
            DiscoveredInterpreter {
                source,
                interpreter,
            }
        }
    };

    Ok(InterpreterResult::Ok(result))
}

/// Find the default Python interpreter on the system.
///
/// Virtual environments are not included in discovery.
///
/// See [`find_interpreter`] for more details on interpreter discovery.
pub fn find_default_interpreter(cache: &Cache) -> Result<InterpreterResult, Error> {
    let request = InterpreterRequest::default();
    let sources = SourceSelector::System;

    let result = find_interpreter(&request, SystemPython::Required, &sources, cache)?;
    if let Ok(ref found) = result {
        warn_on_unsupported_python(found.interpreter());
    }

    Ok(result)
}

/// Find the best-matching Python interpreter.
///
/// If no Python version is provided, we will use the first available interpreter.
///
/// If a Python version is provided, we will first try to find an exact match. If
/// that cannot be found and a patch version was requested, we will look for a match
/// without comparing the patch version number. If that cannot be found, we fall back to
/// the first available version.
///
/// See [`find_interpreter`] for more details on interpreter discovery.
#[instrument(skip_all, fields(request))]
pub fn find_best_interpreter(
    request: &InterpreterRequest,
    system: SystemPython,
    cache: &Cache,
) -> Result<InterpreterResult, Error> {
    debug!("Starting interpreter discovery for {}", request);

    // Determine if we should be allowed to look outside of virtual environments.
    let sources = SourceSelector::from_settings(system);

    // First, check for an exact match (or the first available version if no Python versfion was provided)
    debug!("Looking for exact match for request {request}");
    let result = find_interpreter(request, system, &sources, cache)?;
    if let Ok(ref found) = result {
        warn_on_unsupported_python(found.interpreter());
        return Ok(result);
    }

    // If that fails, and a specific patch version was requested try again allowing a
    // different patch version
    if let Some(request) = match request {
        InterpreterRequest::Version(version) => {
            if version.has_patch() {
                Some(InterpreterRequest::Version((*version).without_patch()))
            } else {
                None
            }
        }
        InterpreterRequest::ImplementationVersion(implementation, version) => Some(
            InterpreterRequest::ImplementationVersion(*implementation, (*version).without_patch()),
        ),
        _ => None,
    } {
        debug!("Looking for relaxed patch version {request}");
        let result = find_interpreter(&request, system, &sources, cache)?;
        if let Ok(ref found) = result {
            warn_on_unsupported_python(found.interpreter());
            return Ok(result);
        }
    }

    // If a Python version was requested but cannot be fulfilled, just take any version
    debug!("Looking for Python interpreter with any version");
    let request = InterpreterRequest::Any;
    Ok(find_interpreter(
        // TODO(zanieb): Add a dedicated `Default` variant to `InterpreterRequest`
        &request, system, &sources, cache,
    )?
    .map_err(|err| {
        // Use a more general error in this case since we looked for multiple versions
        if matches!(err, InterpreterNotFound::NoMatchingVersion(..)) {
            InterpreterNotFound::NoPythonInstallation(sources.clone(), None)
        } else {
            err
        }
    }))
}

/// Display a warning if the Python version of the [`Interpreter`] is unsupported by uv.
fn warn_on_unsupported_python(interpreter: &Interpreter) {
    // Warn on usage with an unsupported Python version
    if interpreter.python_tuple() < (3, 8) {
        warn_user_once!(
            "uv is only compatible with Python 3.8+, found Python {}.",
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
    use winapi::um::fileapi::{CreateFileW, OPEN_EXISTING};
    use winapi::um::handleapi::{CloseHandle, INVALID_HANDLE_VALUE};
    use winapi::um::ioapiset::DeviceIoControl;
    use winapi::um::winbase::{FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT};
    use winapi::um::winioctl::FSCTL_GET_REPARSE_POINT;
    use winapi::um::winnt::{FILE_ATTRIBUTE_REPARSE_POINT, MAXIMUM_REPARSE_DATA_BUFFER_SIZE};

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

impl InterpreterRequest {
    /// Create a request from a string.
    ///
    /// This cannot fail, which means weird inputs will be parsed as [`InterpreterRequest::File`] or [`InterpreterRequest::ExecutableName`].
    pub fn parse(value: &str) -> Self {
        // e.g. `3.12.1`
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
        for implementation in ImplementationName::iter() {
            if let Some(remainder) = value
                .to_ascii_lowercase()
                .strip_prefix(implementation.as_str())
            {
                // e.g. `pypy`
                if remainder.is_empty() {
                    return Self::Implementation(*implementation);
                }
                // e.g. `pypy3.12`
                if let Ok(version) = VersionRequest::from_str(remainder) {
                    return Self::ImplementationVersion(*implementation, version);
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
        // Finally, we'll treat it as the name of an executable (i.e. in the search PATH)
        // e.g. foo.exe
        Self::ExecutableName(value.to_string())
    }
}

impl VersionRequest {
    pub(crate) fn default_names(self) -> [Option<Cow<'static, str>>; 4] {
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
            Self::Any => [Some(python3), Some(python), None, None],
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
                let name = implementation.as_str();
                let (python, python3) = if extension.is_empty() {
                    (Cow::Borrowed(name), Cow::Owned(format!("{name}3")))
                } else {
                    (
                        Cow::Owned(format!("{name}{extension}")),
                        Cow::Owned(format!("{name}3{extension}")),
                    )
                };

                match self {
                    Self::Any => [Some(python3), Some(python), None, None],
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
    fn matches_interpreter(self, interpreter: &Interpreter) -> bool {
        match self {
            Self::Any => true,
            Self::Major(major) => interpreter.python_major() == major,
            Self::MajorMinor(major, minor) => {
                (interpreter.python_major(), interpreter.python_minor()) == (major, minor)
            }
            Self::MajorMinorPatch(major, minor, patch) => {
                (
                    interpreter.python_major(),
                    interpreter.python_minor(),
                    interpreter.python_patch(),
                ) == (major, minor, patch)
            }
        }
    }

    fn matches_version(self, version: &PythonVersion) -> bool {
        match self {
            Self::Any => true,
            Self::Major(major) => version.major() == major,
            Self::MajorMinor(major, minor) => (version.major(), version.minor()) == (major, minor),
            Self::MajorMinorPatch(major, minor, patch) => {
                (version.major(), version.minor(), version.patch()) == (major, minor, Some(patch))
            }
        }
    }

    fn matches_major_minor(self, major: u8, minor: u8) -> bool {
        match self {
            Self::Any => true,
            Self::Major(self_major) => self_major == major,
            Self::MajorMinor(self_major, self_minor) => (self_major, self_minor) == (major, minor),
            Self::MajorMinorPatch(self_major, self_minor, _) => {
                (self_major, self_minor) == (major, minor)
            }
        }
    }

    /// Return true if a patch version is present in the request.
    fn has_patch(self) -> bool {
        match self {
            Self::Any => false,
            Self::Major(..) => false,
            Self::MajorMinor(..) => false,
            Self::MajorMinorPatch(..) => true,
        }
    }

    /// Return a new `VersionRequest` without the patch version.
    #[must_use]
    fn without_patch(self) -> Self {
        match self {
            Self::Any => Self::Any,
            Self::Major(major) => Self::Major(major),
            Self::MajorMinor(major, minor) => Self::MajorMinor(major, minor),
            Self::MajorMinorPatch(major, minor, _) => Self::MajorMinor(major, minor),
        }
    }
}

impl FromStr for VersionRequest {
    type Err = ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let versions = s
            .splitn(3, '.')
            .map(str::parse::<u8>)
            .collect::<Result<Vec<_>, _>>()?;

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
        }
    }
}

impl SourceSelector {
    /// Create a new [`SourceSelector::Some`] from an iterator.
    pub(crate) fn from_sources(iter: impl IntoIterator<Item = InterpreterSource>) -> Self {
        let inner = HashSet::from_iter(iter);
        assert!(!inner.is_empty(), "Source selectors cannot be empty");
        Self::Custom(inner)
    }

    /// Return true if this selector includes the given [`InterpreterSource`].
    fn contains(&self, source: InterpreterSource) -> bool {
        match self {
            Self::All => true,
            Self::System => [
                InterpreterSource::ProvidedPath,
                InterpreterSource::SearchPath,
                #[cfg(windows)]
                InterpreterSource::PyLauncher,
                InterpreterSource::ManagedToolchain,
                InterpreterSource::ParentInterpreter,
            ]
            .contains(&source),
            Self::VirtualEnv => [
                InterpreterSource::DiscoveredEnvironment,
                InterpreterSource::ActiveEnvironment,
                InterpreterSource::CondaPrefix,
            ]
            .contains(&source),
            Self::Custom(sources) => sources.contains(&source),
        }
    }

    /// Return a [`SourceSelector`] based the settings.
    pub fn from_settings(system: SystemPython) -> Self {
        if env::var_os("UV_FORCE_MANAGED_PYTHON").is_some() {
            debug!("Only considering managed toolchains due to `UV_FORCE_MANAGED_PYTHON`");
            Self::from_sources([InterpreterSource::ManagedToolchain])
        } else if env::var_os("UV_TEST_PYTHON_PATH").is_some() {
            debug!(
                "Only considering search path and active environments due to `UV_TEST_PYTHON_PATH`"
            );
            Self::from_sources([
                InterpreterSource::ActiveEnvironment,
                InterpreterSource::SearchPath,
            ])
        } else {
            match system {
                SystemPython::Allowed | SystemPython::Explicit => Self::All,
                SystemPython::Required => Self::System,
                SystemPython::Disallowed => Self::VirtualEnv,
            }
        }
    }
}

impl SystemPython {
    /// Returns true if a system Python is allowed.
    pub fn is_allowed(&self) -> bool {
        matches!(self, SystemPython::Allowed | SystemPython::Required)
    }

    /// Returns true if a system Python is preferred.
    pub fn is_preferred(&self) -> bool {
        matches!(self, SystemPython::Required)
    }
}

impl fmt::Display for InterpreterRequest {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Any => write!(f, "any Python"),
            Self::Version(version) => write!(f, "Python {version}"),
            Self::Directory(path) => write!(f, "directory `{}`", path.user_display()),
            Self::File(path) => write!(f, "path `{}`", path.user_display()),
            Self::ExecutableName(name) => write!(f, "executable name `{name}`"),
            Self::Implementation(implementation) => {
                write!(f, "{implementation}")
            }
            Self::ImplementationVersion(implementation, version) => {
                write!(f, "{implementation} {version}")
            }
        }
    }
}

impl fmt::Display for InterpreterSource {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::ProvidedPath => f.write_str("provided path"),
            Self::ActiveEnvironment => f.write_str("active virtual environment"),
            Self::CondaPrefix => f.write_str("conda prefix"),
            Self::DiscoveredEnvironment => f.write_str("virtual environment"),
            Self::SearchPath => f.write_str("search path"),
            Self::PyLauncher => f.write_str("`py` launcher output"),
            Self::ManagedToolchain => f.write_str("managed toolchains"),
            Self::ParentInterpreter => f.write_str("parent interpreter"),
        }
    }
}

impl fmt::Display for InterpreterNotFound {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoPythonInstallation(sources, None | Some(VersionRequest::Any)) => {
                write!(f, "No Python interpreters found in {sources}")
            }
            Self::NoPythonInstallation(sources, Some(version)) => {
                write!(f, "No Python {version} interpreters found in {sources}")
            }
            Self::NoMatchingVersion(sources, VersionRequest::Any) => {
                write!(f, "No Python interpreter found in {sources}")
            }
            Self::NoMatchingVersion(sources, version) => {
                write!(f, "No interpreter found for Python {version} in {sources}")
            }
            Self::NoMatchingImplementation(sources, implementation) => {
                write!(f, "No interpreter found for {implementation} in {sources}")
            }
            Self::NoMatchingImplementationVersion(sources, implementation, version) => {
                write!(
                    f,
                    "No interpreter found for {implementation} {version} in {sources}"
                )
            }
            Self::FileNotFound(path) => write!(
                f,
                "Requested interpreter path `{}` does not exist",
                path.user_display()
            ),
            Self::DirectoryNotFound(path) => write!(
                f,
                "Requested interpreter directory `{}` does not exist",
                path.user_display()
            ),
            Self::ExecutableNotFoundInDirectory(directory, executable) => {
                write!(
                    f,
                    "Interpreter directory `{}` does not contain Python executable at `{}`",
                    directory.user_display(),
                    executable.user_display_from(directory)
                )
            }
            Self::ExecutableNotFoundInSearchPath(name) => {
                write!(f, "Requested Python executable `{name}` not found in PATH")
            }
            Self::FileNotExecutable(path) => {
                write!(
                    f,
                    "Python interpreter at `{}` is not executable",
                    path.user_display()
                )
            }
        }
    }
}

impl fmt::Display for SourceSelector {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::All => f.write_str("all sources"),
            Self::VirtualEnv => f.write_str("virtual environments"),
            Self::System => {
                // TODO(zanieb): We intentionally omit managed toolchains for now since they are not public
                if cfg!(windows) {
                    write!(
                        f,
                        "{} or {}",
                        InterpreterSource::SearchPath,
                        InterpreterSource::PyLauncher
                    )
                } else {
                    write!(f, "{}", InterpreterSource::SearchPath)
                }
            }
            Self::Custom(sources) => {
                let sources: Vec<_> = sources
                    .iter()
                    .sorted()
                    .map(InterpreterSource::to_string)
                    .collect();
                match sources[..] {
                    [] => unreachable!("Source selectors must contain at least one source"),
                    [ref one] => f.write_str(one),
                    [ref first, ref second] => write!(f, "{first} or {second}"),
                    [ref first @ .., ref last] => write!(f, "{}, or {last}", first.join(", ")),
                }
            }
        }
    }
}

impl DiscoveredInterpreter {
    #[allow(dead_code)]
    pub fn source(&self) -> &InterpreterSource {
        &self.source
    }

    pub fn interpreter(&self) -> &Interpreter {
        &self.interpreter
    }

    pub fn into_interpreter(self) -> Interpreter {
        self.interpreter
    }
}

#[cfg(test)]
mod tests {

    use std::{path::PathBuf, str::FromStr};

    use test_log::test;

    use assert_fs::{prelude::*, TempDir};

    use crate::{
        discovery::{InterpreterRequest, VersionRequest},
        implementation::ImplementationName,
    };

    #[test]
    fn interpreter_request_from_str() {
        assert_eq!(
            InterpreterRequest::parse("3.12"),
            InterpreterRequest::Version(VersionRequest::from_str("3.12").unwrap())
        );
        assert_eq!(
            InterpreterRequest::parse("foo"),
            InterpreterRequest::ExecutableName("foo".to_string())
        );
        assert_eq!(
            InterpreterRequest::parse("cpython"),
            InterpreterRequest::Implementation(ImplementationName::CPython)
        );
        assert_eq!(
            InterpreterRequest::parse("cpython3.12.2"),
            InterpreterRequest::ImplementationVersion(
                ImplementationName::CPython,
                VersionRequest::from_str("3.12.2").unwrap()
            )
        );
        assert_eq!(
            InterpreterRequest::parse("pypy"),
            InterpreterRequest::Implementation(ImplementationName::PyPy)
        );
        assert_eq!(
            InterpreterRequest::parse("pypy3.10"),
            InterpreterRequest::ImplementationVersion(
                ImplementationName::PyPy,
                VersionRequest::from_str("3.10").unwrap()
            )
        );
        assert_eq!(
            InterpreterRequest::parse("pypy@3.10"),
            InterpreterRequest::ImplementationVersion(
                ImplementationName::PyPy,
                VersionRequest::from_str("3.10").unwrap()
            )
        );
        assert_eq!(
            InterpreterRequest::parse("pypy310"),
            InterpreterRequest::ExecutableName("pypy310".to_string())
        );

        let tempdir = TempDir::new().unwrap();
        assert_eq!(
            InterpreterRequest::parse(tempdir.path().to_str().unwrap()),
            InterpreterRequest::Directory(tempdir.path().to_path_buf()),
            "An existing directory is treated as a directory"
        );
        assert_eq!(
            InterpreterRequest::parse(tempdir.child("foo").path().to_str().unwrap()),
            InterpreterRequest::File(tempdir.child("foo").path().to_path_buf()),
            "A path that does not exist is treated as a file"
        );
        tempdir.child("bar").touch().unwrap();
        assert_eq!(
            InterpreterRequest::parse(tempdir.child("bar").path().to_str().unwrap()),
            InterpreterRequest::File(tempdir.child("bar").path().to_path_buf()),
            "An existing file is treated as a file"
        );
        assert_eq!(
            InterpreterRequest::parse("./foo"),
            InterpreterRequest::File(PathBuf::from_str("./foo").unwrap()),
            "A string with a file system separator is treated as a file"
        );
    }

    #[test]
    fn version_request_from_str() {
        assert_eq!(VersionRequest::from_str("3"), Ok(VersionRequest::Major(3)));
        assert_eq!(
            VersionRequest::from_str("3.12"),
            Ok(VersionRequest::MajorMinor(3, 12))
        );
        assert_eq!(
            VersionRequest::from_str("3.12.1"),
            Ok(VersionRequest::MajorMinorPatch(3, 12, 1))
        );
        assert!(VersionRequest::from_str("1.foo.1").is_err());
    }
}
