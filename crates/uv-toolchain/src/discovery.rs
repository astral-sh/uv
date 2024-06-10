use std::borrow::Cow;
use std::collections::HashSet;
use std::fmt::{self, Formatter};
use std::num::ParseIntError;
use std::{env, io};
use std::{path::Path, path::PathBuf, str::FromStr};

use itertools::Itertools;
use same_file::is_same_file;
use thiserror::Error;
use tracing::{debug, instrument, trace};
use which::which;

use uv_cache::Cache;
use uv_configuration::PreviewMode;
use uv_fs::Simplified;
use uv_warnings::warn_user_once;

use crate::implementation::{ImplementationName, LenientImplementationName};
use crate::interpreter::Error as InterpreterError;
use crate::managed::InstalledToolchains;
use crate::py_launcher::py_list_paths;
use crate::toolchain::Toolchain;
use crate::virtualenv::{
    conda_prefix_from_env, virtualenv_from_env, virtualenv_from_working_dir,
    virtualenv_python_executable,
};
use crate::{Interpreter, PythonVersion};

/// A request to find a Python toolchain.
///
/// See [`ToolchainRequest::from_str`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ToolchainRequest {
    /// Use any discovered Python toolchain
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

/// The sources to consider when finding a Python toolchain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolchainSources {
    // Consider all toolchain sources.
    All(PreviewMode),
    // Only consider system toolchain sources
    System(PreviewMode),
    // Only consider virtual environment sources
    VirtualEnv,
    // Only consider a custom set of sources
    Custom(HashSet<ToolchainSource>),
}

/// A Python toolchain version request.
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
    /// Only allow a system Python if passed directly i.e. via [`ToolchainSource::ProvidedPath`] or [`ToolchainSource::ParentInterpreter`]
    #[default]
    Explicit,
    /// Do not allow a system Python
    Disallowed,
    /// Allow a system Python to be used if no virtual environment is active.
    Allowed,
    /// Ignore virtual environments and require a system Python.
    Required,
}

/// The result of an toolchain search.
///
/// Returned by [`find_toolchain`].
type ToolchainResult = Result<Toolchain, ToolchainNotFound>;

/// The result of failed toolchain discovery.
///
/// See [`InterpreterResult`].
#[derive(Clone, Debug, Error)]
pub enum ToolchainNotFound {
    /// No Python installations were found.
    NoPythonInstallation(ToolchainSources, Option<VersionRequest>),
    /// No Python installations with the requested version were found.
    NoMatchingVersion(ToolchainSources, VersionRequest),
    /// No Python installations with the requested implementation name were found.
    NoMatchingImplementation(ToolchainSources, ImplementationName),
    /// No Python installations with the requested implementation name and version were found.
    NoMatchingImplementationVersion(ToolchainSources, ImplementationName, VersionRequest),
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

/// The source of a discovered Python toolchain.
#[derive(Debug, Clone, PartialEq, Eq, Copy, Hash, PartialOrd, Ord)]
pub enum ToolchainSource {
    /// The toolchain path was provided directly
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
    /// The toolchain was found in the uv toolchain directory
    Managed,
    /// The toolchain was found via the invoking interpreter i.e. via `python -m uv ...`
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

    #[error("Interpreter discovery for `{0}` requires `{1}` but it is not selected; the following are selected: {2}")]
    SourceNotSelected(ToolchainRequest, ToolchainSource, ToolchainSources),
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
    sources: &ToolchainSources,
) -> impl Iterator<Item = Result<(ToolchainSource, PathBuf), Error>> + 'a {
    // Note we are careful to ensure the iterator chain is lazy to avoid unnecessary work

    // (1) The parent interpreter
    sources.contains(ToolchainSource::ParentInterpreter).then(||
        std::env::var_os("UV_INTERNAL__PARENT_INTERPRETER")
        .into_iter()
        .map(|path| Ok((ToolchainSource::ParentInterpreter, PathBuf::from(path))))
    ).into_iter().flatten()
    // (2) An active virtual environment
    .chain(
        sources.contains(ToolchainSource::ActiveEnvironment).then(||
            virtualenv_from_env()
            .into_iter()
            .map(virtualenv_python_executable)
            .map(|path| Ok((ToolchainSource::ActiveEnvironment, path)))
        ).into_iter().flatten()
    )
    // (3) An active conda environment
    .chain(
        sources.contains(ToolchainSource::CondaPrefix).then(||
            conda_prefix_from_env()
            .into_iter()
            .map(virtualenv_python_executable)
            .map(|path| Ok((ToolchainSource::CondaPrefix, path)))
        ).into_iter().flatten()
    )
    // (4) A discovered environment
    .chain(
        sources.contains(ToolchainSource::DiscoveredEnvironment).then(||
            std::iter::once(
                virtualenv_from_working_dir()
                .map(|path|
                    path
                    .map(virtualenv_python_executable)
                    .map(|path| (ToolchainSource::DiscoveredEnvironment, path))
                    .into_iter()
                )
                .map_err(Error::from)
            ).flatten_ok()
        ).into_iter().flatten()
    )
    // (5) Managed toolchains
    .chain(
        sources.contains(ToolchainSource::Managed).then(move ||
            std::iter::once(
                InstalledToolchains::from_settings().map_err(Error::from).and_then(|installed_toolchains| {
                    debug!("Searching for managed toolchains at `{}`", installed_toolchains.root().user_display());
                    let toolchains = installed_toolchains.find_matching_current_platform()?;
                    // Check that the toolchain version satisfies the request to avoid unnecessary interpreter queries later
                    Ok(
                        toolchains.into_iter().filter(move |toolchain|
                            version.is_none() || version.is_some_and(|version|
                                version.matches_version(toolchain.python_version())
                            )
                        )
                        .inspect(|toolchain| debug!("Found managed toolchain `{toolchain}`"))
                        .map(|toolchain| (ToolchainSource::Managed, toolchain.executable()))
                    )
                })
            ).flatten_ok()
        ).into_iter().flatten()
    )
    // (6) The search path
    .chain(
        sources.contains(ToolchainSource::SearchPath).then(move ||
            python_executables_from_search_path(version, implementation)
            .map(|path| Ok((ToolchainSource::SearchPath, path))),
        ).into_iter().flatten()
    )
    // (7) The `py` launcher (windows only)
    // TODO(konstin): Implement <https://peps.python.org/pep-0514/> to read python installations from the registry instead.
    .chain(
        (sources.contains(ToolchainSource::PyLauncher) && cfg!(windows)).then(||
            std::iter::once(
                py_list_paths()
                .map(|entries|
                    // We can avoid querying the interpreter using versions from the py launcher output unless a patch is requested
                    entries.into_iter().filter(move |entry|
                        version.is_none() || version.is_some_and(|version|
                            version.has_patch() || version.matches_major_minor(entry.major, entry.minor)
                        )
                    )
                    .map(|entry| (ToolchainSource::PyLauncher, entry.executable_path))
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
/// See [`python_executables`] for more information on discovery.
fn python_interpreters<'a>(
    version: Option<&'a VersionRequest>,
    implementation: Option<&'a ImplementationName>,
    system: SystemPython,
    sources: &ToolchainSources,
    cache: &'a Cache,
) -> impl Iterator<Item = Result<(ToolchainSource, Interpreter), Error>> + 'a {
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
                .inspect_err(|err| debug!("{err}")),
            Err(err) => Err(err),
        })
        .filter(move |result| match result {
            // Filter the returned interpreters to conform to the system request
            Ok((source, interpreter)) => match (
                system,
                // Conda environments are not conformant virtual environments but we should not treat them as system interpreters
                interpreter.is_virtualenv() || matches!(source, ToolchainSource::CondaPrefix),
            ) {
                (SystemPython::Allowed, _) => true,
                (SystemPython::Explicit, false) => {
                    if matches!(
                        source,
                        ToolchainSource::ProvidedPath | ToolchainSource::ParentInterpreter
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
/// Returns false when an error could be due to a faulty toolchain and we should continue searching for a working one.
fn should_stop_discovery(err: &Error) -> bool {
    match err {
        // When querying the toolchain interpreter fails, we will only raise errors that demonstrate that something is broken
        // If the toolchain interpreter returned a bad response, we'll continue searching for one that works
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

/// Find a toolchain that satisfies the given request.
///
/// If an error is encountered while locating or inspecting a candidate toolchain,
/// the error will raised instead of attempting further candidates.
pub(crate) fn find_toolchain(
    request: &ToolchainRequest,
    system: SystemPython,
    sources: &ToolchainSources,
    cache: &Cache,
) -> Result<ToolchainResult, Error> {
    let result = match request {
        ToolchainRequest::File(path) => {
            debug!("Checking for Python interpreter at {request}");
            if !sources.contains(ToolchainSource::ProvidedPath) {
                return Err(Error::SourceNotSelected(
                    request.clone(),
                    ToolchainSource::ProvidedPath,
                    sources.clone(),
                ));
            }
            if !path.try_exists()? {
                return Ok(ToolchainResult::Err(ToolchainNotFound::FileNotFound(
                    path.clone(),
                )));
            }
            Toolchain {
                source: ToolchainSource::ProvidedPath,
                interpreter: Interpreter::query(path, cache)?,
            }
        }
        ToolchainRequest::Directory(path) => {
            debug!("Checking for Python interpreter in {request}");
            if !sources.contains(ToolchainSource::ProvidedPath) {
                return Err(Error::SourceNotSelected(
                    request.clone(),
                    ToolchainSource::ProvidedPath,
                    sources.clone(),
                ));
            }
            if !path.try_exists()? {
                return Ok(ToolchainResult::Err(ToolchainNotFound::FileNotFound(
                    path.clone(),
                )));
            }
            let executable = virtualenv_python_executable(path);
            if !executable.try_exists()? {
                return Ok(ToolchainResult::Err(
                    ToolchainNotFound::ExecutableNotFoundInDirectory(path.clone(), executable),
                ));
            }
            Toolchain {
                source: ToolchainSource::ProvidedPath,
                interpreter: Interpreter::query(executable, cache)?,
            }
        }
        ToolchainRequest::ExecutableName(name) => {
            debug!("Searching for Python interpreter with {request}");
            if !sources.contains(ToolchainSource::SearchPath) {
                return Err(Error::SourceNotSelected(
                    request.clone(),
                    ToolchainSource::SearchPath,
                    sources.clone(),
                ));
            }
            let Some(executable) = which(name).ok() else {
                return Ok(ToolchainResult::Err(
                    ToolchainNotFound::ExecutableNotFoundInSearchPath(name.clone()),
                ));
            };
            Toolchain {
                source: ToolchainSource::SearchPath,
                interpreter: Interpreter::query(executable, cache)?,
            }
        }
        ToolchainRequest::Implementation(implementation) => {
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
                return Ok(ToolchainResult::Err(
                    ToolchainNotFound::NoMatchingImplementation(sources.clone(), *implementation),
                ));
            };
            Toolchain {
                source,
                interpreter,
            }
        }
        ToolchainRequest::ImplementationVersion(implementation, version) => {
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
                return Ok(ToolchainResult::Err(
                    ToolchainNotFound::NoMatchingImplementationVersion(
                        sources.clone(),
                        *implementation,
                        *version,
                    ),
                ));
            };
            Toolchain {
                source,
                interpreter,
            }
        }
        ToolchainRequest::Any => {
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
                return Ok(ToolchainResult::Err(
                    ToolchainNotFound::NoPythonInstallation(sources.clone(), None),
                ));
            };
            Toolchain {
                source,
                interpreter,
            }
        }
        ToolchainRequest::Version(version) => {
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
                    ToolchainNotFound::NoPythonInstallation(sources.clone(), Some(*version))
                } else {
                    ToolchainNotFound::NoMatchingVersion(sources.clone(), *version)
                };
                return Ok(ToolchainResult::Err(err));
            };
            Toolchain {
                source,
                interpreter,
            }
        }
    };

    Ok(ToolchainResult::Ok(result))
}

/// Find the default Python toolchain on the system.
///
/// Virtual environments are not included in discovery.
///
/// See [`find_toolchain`] for more details on toolchain discovery.
pub(crate) fn find_default_toolchain(
    preview: PreviewMode,
    cache: &Cache,
) -> Result<ToolchainResult, Error> {
    let request = ToolchainRequest::default();
    let sources = ToolchainSources::System(preview);

    let result = find_toolchain(&request, SystemPython::Required, &sources, cache)?;
    if let Ok(ref toolchain) = result {
        warn_on_unsupported_python(toolchain.interpreter());
    }

    Ok(result)
}

/// Find the best-matching Python toolchain.
///
/// If no Python version is provided, we will use the first available toolchain.
///
/// If a Python version is provided, we will first try to find an exact match. If
/// that cannot be found and a patch version was requested, we will look for a match
/// without comparing the patch version number. If that cannot be found, we fall back to
/// the first available version.
///
/// See [`find_toolchain`] for more details on toolchain discovery.
#[instrument(skip_all, fields(request))]
pub fn find_best_toolchain(
    request: &ToolchainRequest,
    system: SystemPython,
    preview: PreviewMode,
    cache: &Cache,
) -> Result<ToolchainResult, Error> {
    debug!("Starting toolchain discovery for {}", request);

    // Determine if we should be allowed to look outside of virtual environments.
    let sources = ToolchainSources::from_settings(system, preview);

    // First, check for an exact match (or the first available version if no Python versfion was provided)
    debug!("Looking for exact match for request {request}");
    let result = find_toolchain(request, system, &sources, cache)?;
    if let Ok(ref toolchain) = result {
        warn_on_unsupported_python(toolchain.interpreter());
        return Ok(result);
    }

    // If that fails, and a specific patch version was requested try again allowing a
    // different patch version
    if let Some(request) = match request {
        ToolchainRequest::Version(version) => {
            if version.has_patch() {
                Some(ToolchainRequest::Version((*version).without_patch()))
            } else {
                None
            }
        }
        ToolchainRequest::ImplementationVersion(implementation, version) => Some(
            ToolchainRequest::ImplementationVersion(*implementation, (*version).without_patch()),
        ),
        _ => None,
    } {
        debug!("Looking for relaxed patch version {request}");
        let result = find_toolchain(&request, system, &sources, cache)?;
        if let Ok(ref toolchain) = result {
            warn_on_unsupported_python(toolchain.interpreter());
            return Ok(result);
        }
    }

    // If a Python version was requested but cannot be fulfilled, just take any version
    debug!("Looking for Python toolchain with any version");
    let request = ToolchainRequest::Any;
    Ok(find_toolchain(
        // TODO(zanieb): Add a dedicated `Default` variant to `ToolchainRequest`
        &request, system, &sources, cache,
    )?
    .map_err(|err| {
        // Use a more general error in this case since we looked for multiple versions
        if matches!(err, ToolchainNotFound::NoMatchingVersion(..)) {
            ToolchainNotFound::NoPythonInstallation(sources.clone(), None)
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

impl ToolchainRequest {
    /// Create a request from a string.
    ///
    /// This cannot fail, which means weird inputs will be parsed as [`ToolchainRequest::File`] or [`ToolchainRequest::ExecutableName`].
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

    /// Check if a given interpreter satisfies the interpreter request.
    pub fn satisfied(&self, interpreter: &Interpreter, cache: &Cache) -> bool {
        /// Returns `true` if the two paths refer to the same interpreter executable.
        fn is_same_executable(path1: &Path, path2: &Path) -> bool {
            path1 == path2 || is_same_file(path1, path2).unwrap_or(false)
        }

        match self {
            ToolchainRequest::Any => true,
            ToolchainRequest::Version(version_request) => {
                version_request.matches_interpreter(interpreter)
            }
            ToolchainRequest::Directory(directory) => {
                // `sys.prefix` points to the venv root.
                is_same_executable(directory, interpreter.sys_prefix())
            }
            ToolchainRequest::File(file) => {
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
            ToolchainRequest::ExecutableName(name) => {
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
            ToolchainRequest::Implementation(implementation) => {
                interpreter.implementation_name() == implementation.as_str()
            }
            ToolchainRequest::ImplementationVersion(implementation, version) => {
                version.matches_interpreter(interpreter)
                    && interpreter.implementation_name() == implementation.as_str()
            }
        }
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

    pub(crate) fn matches_version(self, version: &PythonVersion) -> bool {
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

    pub(crate) fn matches_major_minor_patch(self, major: u8, minor: u8, patch: u8) -> bool {
        match self {
            Self::Any => true,
            Self::Major(self_major) => self_major == major,
            Self::MajorMinor(self_major, self_minor) => (self_major, self_minor) == (major, minor),
            Self::MajorMinorPatch(self_major, self_minor, self_patch) => {
                (self_major, self_minor, self_patch) == (major, minor, patch)
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

impl ToolchainSources {
    /// Create a new [`SourceSelector::Some`] from an iterator.
    pub(crate) fn from_sources(iter: impl IntoIterator<Item = ToolchainSource>) -> Self {
        let inner = HashSet::from_iter(iter);
        assert!(!inner.is_empty(), "Source selectors cannot be empty");
        Self::Custom(inner)
    }

    /// Return true if this selector includes the given [`ToolchainSource`].
    fn contains(&self, source: ToolchainSource) -> bool {
        match self {
            Self::All(preview) => {
                // Always return `true` except for `ManagedToolchain` which requires preview mode
                source != ToolchainSource::Managed || preview.is_enabled()
            }
            Self::System(preview) => {
                [
                    ToolchainSource::ProvidedPath,
                    ToolchainSource::SearchPath,
                    #[cfg(windows)]
                    ToolchainSource::PyLauncher,
                    ToolchainSource::ParentInterpreter,
                ]
                .contains(&source)
                    // Allow `ManagedToolchain` in preview
                    || (source == ToolchainSource::Managed
                        && preview.is_enabled())
            }
            Self::VirtualEnv => [
                ToolchainSource::DiscoveredEnvironment,
                ToolchainSource::ActiveEnvironment,
                ToolchainSource::CondaPrefix,
            ]
            .contains(&source),
            Self::Custom(sources) => sources.contains(&source),
        }
    }

    /// Return a [`SourceSelector`] based the settings.
    pub fn from_settings(system: SystemPython, preview: PreviewMode) -> Self {
        if env::var_os("UV_FORCE_MANAGED_PYTHON").is_some() {
            debug!("Only considering managed toolchains due to `UV_FORCE_MANAGED_PYTHON`");
            Self::from_sources([ToolchainSource::Managed])
        } else if env::var_os("UV_TEST_PYTHON_PATH").is_some() {
            debug!(
                "Only considering search path, provided path, and active environments due to `UV_TEST_PYTHON_PATH`"
            );
            Self::from_sources([
                ToolchainSource::ActiveEnvironment,
                ToolchainSource::SearchPath,
                ToolchainSource::ProvidedPath,
            ])
        } else {
            match system {
                SystemPython::Allowed | SystemPython::Explicit => Self::All(preview),
                SystemPython::Required => Self::System(preview),
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

impl fmt::Display for ToolchainRequest {
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

impl fmt::Display for ToolchainSource {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::ProvidedPath => f.write_str("provided path"),
            Self::ActiveEnvironment => f.write_str("active virtual environment"),
            Self::CondaPrefix => f.write_str("conda prefix"),
            Self::DiscoveredEnvironment => f.write_str("virtual environment"),
            Self::SearchPath => f.write_str("search path"),
            Self::PyLauncher => f.write_str("`py` launcher output"),
            Self::Managed => f.write_str("managed toolchains"),
            Self::ParentInterpreter => f.write_str("parent interpreter"),
        }
    }
}

impl fmt::Display for ToolchainNotFound {
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

impl fmt::Display for ToolchainSources {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::All(_) => f.write_str("all sources"),
            Self::VirtualEnv => f.write_str("virtual environments"),
            Self::System(preview) => {
                if cfg!(windows) {
                    if preview.is_disabled() {
                        write!(
                            f,
                            "{} or {}",
                            ToolchainSource::SearchPath,
                            ToolchainSource::PyLauncher
                        )
                    } else {
                        write!(
                            f,
                            "{}, {}, or {}",
                            ToolchainSource::SearchPath,
                            ToolchainSource::PyLauncher,
                            ToolchainSource::Managed
                        )
                    }
                } else {
                    if preview.is_disabled() {
                        write!(f, "{}", ToolchainSource::SearchPath)
                    } else {
                        write!(
                            f,
                            "{} or {}",
                            ToolchainSource::SearchPath,
                            ToolchainSource::Managed
                        )
                    }
                }
            }
            Self::Custom(sources) => {
                let sources: Vec<_> = sources
                    .iter()
                    .sorted()
                    .map(ToolchainSource::to_string)
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

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, str::FromStr};

    use assert_fs::{prelude::*, TempDir};
    use test_log::test;

    use crate::{
        discovery::{ToolchainRequest, VersionRequest},
        implementation::ImplementationName,
    };

    #[test]
    fn interpreter_request_from_str() {
        assert_eq!(
            ToolchainRequest::parse("3.12"),
            ToolchainRequest::Version(VersionRequest::from_str("3.12").unwrap())
        );
        assert_eq!(
            ToolchainRequest::parse("foo"),
            ToolchainRequest::ExecutableName("foo".to_string())
        );
        assert_eq!(
            ToolchainRequest::parse("cpython"),
            ToolchainRequest::Implementation(ImplementationName::CPython)
        );
        assert_eq!(
            ToolchainRequest::parse("cpython3.12.2"),
            ToolchainRequest::ImplementationVersion(
                ImplementationName::CPython,
                VersionRequest::from_str("3.12.2").unwrap()
            )
        );
        assert_eq!(
            ToolchainRequest::parse("pypy"),
            ToolchainRequest::Implementation(ImplementationName::PyPy)
        );
        assert_eq!(
            ToolchainRequest::parse("pypy3.10"),
            ToolchainRequest::ImplementationVersion(
                ImplementationName::PyPy,
                VersionRequest::from_str("3.10").unwrap()
            )
        );
        assert_eq!(
            ToolchainRequest::parse("pypy@3.10"),
            ToolchainRequest::ImplementationVersion(
                ImplementationName::PyPy,
                VersionRequest::from_str("3.10").unwrap()
            )
        );
        assert_eq!(
            ToolchainRequest::parse("pypy310"),
            ToolchainRequest::ExecutableName("pypy310".to_string())
        );

        let tempdir = TempDir::new().unwrap();
        assert_eq!(
            ToolchainRequest::parse(tempdir.path().to_str().unwrap()),
            ToolchainRequest::Directory(tempdir.path().to_path_buf()),
            "An existing directory is treated as a directory"
        );
        assert_eq!(
            ToolchainRequest::parse(tempdir.child("foo").path().to_str().unwrap()),
            ToolchainRequest::File(tempdir.child("foo").path().to_path_buf()),
            "A path that does not exist is treated as a file"
        );
        tempdir.child("bar").touch().unwrap();
        assert_eq!(
            ToolchainRequest::parse(tempdir.child("bar").path().to_str().unwrap()),
            ToolchainRequest::File(tempdir.child("bar").path().to_path_buf()),
            "An existing file is treated as a file"
        );
        assert_eq!(
            ToolchainRequest::parse("./foo"),
            ToolchainRequest::File(PathBuf::from_str("./foo").unwrap()),
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
