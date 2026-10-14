use itertools::{Either, Itertools};
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use regex::Regex;
use rustc_hash::{FxBuildHasher, FxHashSet};
use std::cmp::Reverse;
use std::env::consts::EXE_SUFFIX;
use std::fmt::{self, Debug, Formatter};
use std::{env, io, iter};
use std::{path::Path, path::PathBuf};
use thiserror::Error;
use tracing::{debug, instrument, trace};
use uv_cache::Cache;
use uv_client::BaseClientBuilder;
use uv_fs::Simplified;
use uv_fs::which::is_executable;
use uv_platform::Platform;
use uv_python_types::{
    EnvironmentPreference, PythonPreference, PythonRequest, PythonRequestError, PythonSource,
    VersionRequest,
};
use uv_static::EnvVars;
use uv_warnings::{warn_user_once, warn_user_with_chain};
use which::which_all;

use crate::installation::PythonInstallation;
#[cfg(windows)]
use crate::microsoft_store::find_microsoft_store_pythons;
use crate::virtualenv_discovery::CondaEnvironmentKind;
use crate::virtualenv_discovery::conda_environment_from_env;
use crate::virtualenv_discovery::virtualenv_from_env;
use crate::virtualenv_discovery::virtualenv_from_working_dir;
#[cfg(windows)]
use crate::windows_registry::{WindowsPython, registry_pythons};
use uv_python_interpreter::{
    BrokenLink, Interpreter, InterpreterError, StatusCodeError, UnexpectedResponseError,
    VirtualEnvError, virtualenv_python_executable,
};
use uv_python_managed::ManagedPythonInstallations;
use uv_python_managed::PythonMinorVersionLink;
use uv_python_managed::downloads::ManagedPythonDownloadList;
use uv_python_types::{
    ArchRequest, ImplementationName, PlatformRequest, PythonArchitecture, PythonDownloadMirrors,
    PythonDownloadRequest, python_build_versions_from_env,
};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct DiscoveryPreferences {
    python_preference: PythonPreference,
    environment_preference: EnvironmentPreference,
}

/// The result of a Python installation search.
///
/// Returned by [`find_python_installation`].
type FindPythonResult = Result<PythonInstallation, PythonNotFound>;

/// The result of failed Python installation discovery.
#[derive(Clone, Debug, Error)]
pub struct PythonNotFound {
    pub(super) request: PythonRequest,
    pub(super) python_preference: PythonPreference,
    pub(super) environment_preference: EnvironmentPreference,
}

/// A non-empty group of equally preferred Python executables.
///
/// Minor-version fallback candidates from one `PATH` directory share a group. Preferred executable
/// names and interpreters from other sources form singleton groups.
struct PythonExecutableGroup(Vec<(PythonSource, PathBuf)>);

impl PythonExecutableGroup {
    fn new(executables: Vec<(PythonSource, PathBuf)>) -> Option<Self> {
        (!executables.is_empty()).then_some(Self(executables))
    }

    fn filter(mut self, mut predicate: impl FnMut(PythonSource, &Path) -> bool) -> Option<Self> {
        self.0.retain(|(source, path)| predicate(*source, path));
        (!self.0.is_empty()).then_some(self)
    }
}

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] io::Error),

    /// An error was encountering when retrieving interpreter information.
    #[error("Failed to inspect Python interpreter from {} at `{}` ", _2, _1.user_display())]
    Query(
        #[source] Box<uv_python_interpreter::InterpreterError>,
        PathBuf,
        PythonSource,
    ),

    /// An error was encountered while trying to find a managed Python installation matching the
    /// current platform.
    #[error("Failed to discover managed Python installations")]
    ManagedPython(#[from] uv_python_managed::Error),

    /// An error was encountered when inspecting a virtual environment.
    #[error(transparent)]
    VirtualEnv(#[from] uv_python_interpreter::VirtualEnvError),

    #[cfg(windows)]
    #[error("Failed to query installed Python versions from the Windows registry")]
    RegistryError(#[from] windows::core::Error),

    #[error(transparent)]
    InvalidEnvironmentVariable(#[from] uv_static::InvalidEnvironmentVariable),

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
    BuildVersion(#[from] uv_python_types::BuildVersionError),
}

impl From<PythonRequestError> for Error {
    fn from(error: PythonRequestError) -> Self {
        match error {
            PythonRequestError::InvalidVersionRequest(value) => Self::InvalidVersionRequest(value),
            PythonRequestError::LatestVersionRequest => Self::LatestVersionRequest,
        }
    }
}

impl uv_errors::Hinted for Error {
    fn hints(&self) -> uv_errors::Hints<'_> {
        match self {
            Self::Query(err, _, _) => err.hints(),
            _ => uv_errors::Hints::none(),
        }
    }
}

/// Lazily iterate over Python executables in mutable virtual environments.
///
/// The following sources are supported:
///
/// - Active virtual environment (via `VIRTUAL_ENV`)
/// - Discovered virtual environment (e.g. `.venv` in a parent directory)
///
/// Notably, "system" environments are excluded. See [`python_executables_from_installed`].
fn python_executables_from_virtual_environments<'a>()
-> impl Iterator<Item = Result<(PythonSource, PathBuf), Error>> + 'a {
    let from_active_environment = iter::once_with(|| {
        virtualenv_from_env()
            .into_iter()
            .map(virtualenv_python_executable)
            .map(|path| Ok((PythonSource::ActiveEnvironment, path)))
    })
    .flatten();

    // N.B. we prefer the conda environment over discovered virtual environments
    let from_conda_environment = iter::once_with(move || {
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
    platform: PlatformRequest,
    preference: PythonPreference,
) -> Box<dyn Iterator<Item = Result<PythonExecutableGroup, Error>> + 'a> {
    let from_managed_installations = iter::once_with(move || {
        ManagedPythonInstallations::from_settings(None)
            .map_err(Error::from)
            .and_then(|installed_installations| {
                debug!(
                    "Searching for managed installations at `{}`",
                    installed_installations.root().user_display()
                );
                let installations = ManagedPythonInstallations::find_matching_current_platform()?;

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
                                    PythonMinorVersionLink::from_installation(&installation)
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
    .flatten_ok()
    .map_ok(|executable| PythonExecutableGroup(vec![executable]));

    let from_search_path = iter::once_with(move || {
        let mut first = true;
        python_executables_from_search_path(version, implementation).filter_map(move |paths| {
            let executables = paths
                .into_iter()
                .map(|path| {
                    let source = if first {
                        first = false;
                        PythonSource::SearchPathFirst
                    } else {
                        PythonSource::SearchPath
                    };
                    (source, path)
                })
                .collect();
            PythonExecutableGroup::new(executables).map(Ok)
        })
    })
    .flatten();

    #[cfg(windows)]
    let from_windows_registry: Box<
        dyn Iterator<Item = Result<PythonExecutableGroup, Error>> + 'a,
    > = match uv_static::parse_boolish_environment_variable(EnvVars::UV_PYTHON_NO_REGISTRY) {
        Ok(Some(true)) => Box::new(iter::empty()),
        Ok(Some(false) | None) => Box::new(
            iter::once_with(move || {
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
            .flatten_ok()
            .map_ok(|executable| PythonExecutableGroup(vec![executable])),
        ),
        Err(err) => Box::new(iter::once(Err(Error::from(err)))),
    };

    #[cfg(not(windows))]
    let from_windows_registry: Box<
        dyn Iterator<Item = Result<PythonExecutableGroup, Error>> + 'a,
    > = Box::new(iter::empty());

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
) -> Box<dyn Iterator<Item = Result<PythonExecutableGroup, Error>> + 'a> {
    // Always read from `UV_INTERNAL__PARENT_INTERPRETER` — it could be a system interpreter
    let from_parent_interpreter = iter::once_with(|| {
        env::var_os(EnvVars::UV_INTERNAL__PARENT_INTERPRETER)
            .into_iter()
            .map(|path| {
                Ok(PythonExecutableGroup(vec![(
                    PythonSource::ParentInterpreter,
                    PathBuf::from(path),
                )]))
            })
    })
    .flatten();

    // Check if the base conda environment is active
    let from_base_conda_environment = iter::once_with(move || {
        conda_environment_from_env(CondaEnvironmentKind::Base)
            .into_iter()
            .map(virtualenv_python_executable)
            .map(|path| {
                Ok(PythonExecutableGroup(vec![(
                    PythonSource::BaseCondaPrefix,
                    path,
                )]))
            })
    })
    .flatten();

    let from_virtual_environments = python_executables_from_virtual_environments()
        .map_ok(|executable| PythonExecutableGroup(vec![executable]));
    let from_installed =
        python_executables_from_installed(version, implementation, platform, preference);

    // Limit the search to the relevant environment preference; this avoids unnecessary work like
    // traversal of the file system. Subsequent filtering should be done by the caller with
    // `source_satisfies_environment_preference` and `EnvironmentPreference::allows_installation`.
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
/// For a `PATH` directory containing `python`, `python3`, `python3.14`, `python3.15`, and
/// `python3.15t`, an exact `3.15` request produces the following groups:
///
/// ```text
/// [python3.15], [python3], [python]
/// ```
///
/// A `>=3.14,<3.16` request instead produces:
///
/// ```text
/// [python3], [python], [python3.14, python3.15, python3.15t]
/// ```
///
/// Grouping minor-version fallback candidates from the same directory allows their queried
/// installation keys to determine their relative order without overriding search-path precedence.
///
/// If a `version` is not provided, we will only look for default executable names e.g.
/// `python3` and `python` — `python3.9` and similar will not be included.
fn python_executables_from_search_path<'a>(
    version: &'a VersionRequest,
    implementation: Option<&'a ImplementationName>,
) -> impl Iterator<Item = Vec<PathBuf>> + 'a {
    // `UV_PYTHON_SEARCH_PATH` can be used to override `PATH` for Python executable discovery
    let search_path = env::var_os(EnvVars::UV_PYTHON_SEARCH_PATH)
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
                    let minor_version_directory = dir_clone.clone();

                    possible_names
                        .clone()
                        .into_iter()
                        .flat_map(move |name| {
                            // Since we're just working with a single directory at a time, we collect to simplify ownership
                            which::which_in_global(&*name, Some(&dir))
                                .into_iter()
                                .flatten()
                                .filter(|path| !is_windows_store_shim(path))
                                .map(|path| vec![path])
                                // We have to collect since `which` requires that the regex outlives its
                                // parameters, and the dir is local while we return the iterator.
                                .collect::<Vec<_>>()
                        })
                        .chain(
                            iter::once_with(move || {
                                find_all_minor(implementation, version, &minor_version_directory)
                                    .filter(|path| !is_windows_store_shim(path))
                                    .collect::<Vec<_>>()
                            })
                            .filter(|paths| !paths.is_empty()),
                        )
                        .inspect(|paths| {
                            for path in paths {
                                trace!("Found possible Python executable: {}", path.display());
                            }
                        })
                        .chain(
                            // TODO(zanieb): Consider moving `python.bat` into `possible_names` to avoid a chain
                            cfg!(windows)
                                .then(move || {
                                    which::which_in_global("python.bat", Some(&dir_clone))
                                        .into_iter()
                                        .flatten()
                                        .map(|path| vec![path])
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
                        if minor < 6 {
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
        | VersionRequest::MajorMinorPrerelease(_, _, _, _)
        | VersionRequest::MajorMinorPatchPrerelease(_, _, _, _, _) => Either::Right(iter::empty()),
    }
}

/// How to query discovered Python executables.
#[derive(Debug, Clone, Copy)]
enum QueryStrategy {
    /// Lazily query one executable group at a time.
    Sequential,
    /// Query groups and their executables concurrently before yielding results.
    Parallel,
}

/// Iterate over all discoverable Python interpreters.
///
/// Note interpreters may be excluded by the given [`EnvironmentPreference`], [`PythonPreference`],
/// [`VersionRequest`], or [`PlatformRequest`].
///
/// The [`PlatformRequest`] is currently only applied to managed Python installations before querying
/// the interpreter. The caller is responsible for ensuring it is applied otherwise.
///
/// See [`python_executables`] for more information on discovery.
fn python_installations<'a>(
    version: &'a VersionRequest,
    implementation: Option<&'a ImplementationName>,
    platform: PlatformRequest,
    environments: EnvironmentPreference,
    preference: PythonPreference,
    cache: &'a Cache,
    strategy: QueryStrategy,
) -> Box<dyn Iterator<Item = Result<PythonInstallation, Error>> + 'a> {
    Box::new(
        python_installations_from_executables(
            // Perform filtering on the discovered executables based on their source. This avoids
            // unnecessary interpreter queries, which are generally expensive. We'll filter again
            // with `PythonInstallation::satisfies_preferences` after querying.
            python_executables(version, implementation, platform, environments, preference)
                .filter_map(move |result| match result {
                    Ok(group) => group
                        .filter(|source, path| {
                            source_satisfies_environment_preference(source, path, environments)
                        })
                        .map(Ok),
                    Err(error) => Some(Err(error)),
                }),
            cache,
            strategy,
        )
        .filter_ok(move |installation| {
            installation.satisfies_preferences(version, environments, preference)
                && platform.matches(&Platform::from(installation.interpreter.platform()))
        })
        .map_ok(PythonInstallation::maybe_with_test_source),
    )
}

/// Query a single Python executable, returning a [`PythonInstallation`] on success.
fn python_installation_from_executable(
    source: PythonSource,
    path: PathBuf,
    cache: &Cache,
) -> Result<PythonInstallation, Error> {
    Interpreter::query(&path, cache)
        .map(|interpreter| PythonInstallation {
            source,
            interpreter,
        })
        .inspect(|installation| {
            debug!(
                "Found `{}` at `{}` ({source})",
                installation.key(),
                path.display()
            );
        })
        .map_err(|err| Error::Query(Box::new(err), path, source))
        .inspect_err(|err| debug!("{err}"))
}

/// Convert Python executables into installations using the given query strategy.
fn python_installations_from_executables<'a>(
    executables: impl Iterator<Item = Result<PythonExecutableGroup, Error>> + 'a,
    cache: &'a Cache,
    strategy: QueryStrategy,
) -> Box<dyn Iterator<Item = Result<PythonInstallation, Error>> + 'a> {
    match strategy {
        QueryStrategy::Sequential => Box::new(executables.flat_map(move |group| {
            python_installations_from_executable_group(group, cache, strategy)
        })),
        QueryStrategy::Parallel => {
            let items: Vec<Result<PythonExecutableGroup, Error>> = executables.collect();
            let results: Vec<Vec<Result<PythonInstallation, Error>>> = items
                .into_par_iter()
                .map(|group| {
                    python_installations_from_executable_group(group, cache, strategy)
                        .collect::<Vec<_>>()
                })
                .collect();
            Box::new(results.into_iter().flatten())
        }
    }
}

/// Query an executable group, ordering equally preferred installations by their installation keys.
fn python_installations_from_executable_group(
    group: Result<PythonExecutableGroup, Error>,
    cache: &Cache,
    strategy: QueryStrategy,
) -> impl Iterator<Item = Result<PythonInstallation, Error>> + use<> {
    match group {
        Err(error) => Either::Left(iter::once(Err(error))),
        Ok(PythonExecutableGroup(executables)) => {
            let mut installations = match strategy {
                QueryStrategy::Sequential => executables
                    .into_iter()
                    .map(|(source, path)| python_installation_from_executable(source, path, cache))
                    .collect::<Vec<_>>(),
                QueryStrategy::Parallel => executables
                    .into_par_iter()
                    .map(|(source, path)| python_installation_from_executable(source, path, cache))
                    .collect::<Vec<_>>(),
            };

            sort_installations_by_key(&mut installations, PythonInstallation::key);

            Either::Right(installations.into_iter())
        }
    }
}

/// Sort successful installations without moving them across critical query errors.
fn sort_installations_by_key<T, K: Ord>(
    installations: &mut [Result<T, Error>],
    key: impl Fn(&T) -> K,
) {
    // Critical errors preserve discovery order; non-critical errors must not interrupt
    // installation-key ordering and can follow successful queries.
    for candidates in
        installations.split_mut(|result| result.as_ref().is_err_and(Error::is_critical))
    {
        candidates.sort_by_key(|result| Reverse(result.as_ref().ok().map(&key)));
    }
}

/// Returns true if a [`PythonSource`] could satisfy the [`EnvironmentPreference`].
///
/// This is useful as a pre-filtering step. Use of [`PythonInstallation::satisfies_environment_preference`]
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
    pub(crate) fn is_critical(&self) -> bool {
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
                        "Skipping bad interpreter at `{}` from {source}: {err}",
                        path.display()
                    );
                    false
                }
                InterpreterError::QueryScript { path, err } => {
                    debug!(
                        "Skipping bad interpreter at `{}` from {source}: {err}",
                        path.display()
                    );
                    false
                }
                #[cfg(windows)]
                InterpreterError::CorruptWindowsPackage { path, err } => {
                    debug!(
                        "Skipping bad interpreter at `{}` from {source}: {err}",
                        path.display()
                    );
                    false
                }
                InterpreterError::PermissionDenied { path, err } => {
                    debug!(
                        "Skipping unexecutable interpreter at `{}` from {source}: {err}",
                        path.display()
                    );
                    false
                }
                InterpreterError::NotFound(path)
                | InterpreterError::BrokenLink(BrokenLink { path, .. }) => {
                    // If the interpreter is from an active, valid virtual environment, we should
                    // fail because it's broken
                    if matches!(source, PythonSource::ActiveEnvironment)
                        && uv_fs::is_virtualenv_executable(path)
                    {
                        true
                    } else {
                        trace!("Skipping missing interpreter at `{}`", path.display());
                        false
                    }
                }
            },
            Self::VirtualEnv(VirtualEnvError::MissingPyVenvCfg(path)) => {
                trace!("Skipping broken virtualenv at `{}`", path.display());
                false
            }
            _ => true,
        }
    }
}

/// Create a [`PythonInstallation`] from a Python installation root directory.
fn python_installation_from_directory(
    path: &PathBuf,
    cache: &Cache,
) -> Result<PythonInstallation, uv_python_interpreter::InterpreterError> {
    let executable = virtualenv_python_executable(path);
    Ok(PythonInstallation {
        source: PythonSource::ProvidedPath,
        interpreter: Interpreter::query(&executable, cache)?,
    })
}

/// Lazily iterate over all Python executable paths on the path with the given executable name.
fn python_executables_with_name(
    name: &str,
) -> impl Iterator<Item = Result<(PythonSource, PathBuf), Error>> + '_ {
    which_all(name)
        .into_iter()
        .flat_map(|inner| inner.map(|path| Ok((PythonSource::SearchPath, path))))
}

/// Lazily iterate over all Python installations on the path with the given executable name.
fn python_installations_with_name<'a>(
    name: &'a str,
    cache: &'a Cache,
    strategy: QueryStrategy,
) -> Box<dyn Iterator<Item = Result<PythonInstallation, Error>> + 'a> {
    python_installations_from_executables(
        python_executables_with_name(name)
            .map_ok(|executable| PythonExecutableGroup(vec![executable])),
        cache,
        strategy,
    )
}

/// Iterate over all Python installations that satisfy the given request.
pub(crate) fn find_python_installations<'a>(
    request: &'a PythonRequest,
    environments: EnvironmentPreference,
    preference: PythonPreference,
    arch: Option<PythonArchitecture>,
    cache: &'a Cache,
) -> Box<dyn Iterator<Item = Result<FindPythonResult, Error>> + 'a> {
    find_python_installations_with_strategy(
        request,
        environments,
        preference,
        arch,
        cache,
        QueryStrategy::Sequential,
    )
}

/// Iterate over all Python installations that satisfy the given request using the given query
/// strategy.
fn find_python_installations_with_strategy<'a>(
    request: &'a PythonRequest,
    environments: EnvironmentPreference,
    preference: PythonPreference,
    arch: Option<PythonArchitecture>,
    cache: &'a Cache,
    strategy: QueryStrategy,
) -> Box<dyn Iterator<Item = Result<FindPythonResult, Error>> + 'a> {
    let arch = arch.map(|arch| {
        PythonDownloadRequest::from_request(request)
            .and_then(|request| request.arch().map(ArchRequest::inner))
            .unwrap_or_else(|| arch.into_inner())
    });
    let platform = PlatformRequest::default().with_default_arch(arch);
    let sources = DiscoveryPreferences {
        python_preference: preference,
        environment_preference: environments,
    }
    .sources(request);

    match request {
        PythonRequest::File(path) => Box::new(iter::once({
            if preference.allows_source(PythonSource::ProvidedPath) {
                debug!("Checking for Python interpreter at {request}");
                match Interpreter::query(path, cache) {
                    Ok(interpreter) => Ok(Ok(PythonInstallation {
                        source: PythonSource::ProvidedPath,
                        interpreter,
                    })),
                    Err(InterpreterError::NotFound(_) | InterpreterError::BrokenLink(_)) => {
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
            if preference.allows_source(PythonSource::ProvidedPath) {
                debug!("Checking for Python interpreter in {request}");
                match python_installation_from_directory(path, cache) {
                    Ok(installation) => Ok(Ok(installation)),
                    Err(InterpreterError::NotFound(_) | InterpreterError::BrokenLink(_)) => {
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
            if preference.allows_source(PythonSource::SearchPath) {
                debug!("Searching for Python interpreter with {request}");
                Box::new(
                    python_installations_with_name(name, cache, strategy)
                        .filter_ok(move |installation| {
                            installation.satisfies_environment_preference(environments)
                        })
                        .map_ok(Ok),
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
            python_installations(
                &VersionRequest::Any,
                None,
                platform,
                environments,
                preference,
                cache,
                strategy,
            )
            .map_ok(Ok)
        }),
        PythonRequest::Default => Box::new({
            debug!("Searching for default Python interpreter in {sources}");
            python_installations(
                &VersionRequest::Default,
                None,
                platform,
                environments,
                preference,
                cache,
                strategy,
            )
            .map_ok(Ok)
        }),
        PythonRequest::Version(version) => {
            if let Err(err) = version.check_supported() {
                return Box::new(iter::once(Err(Error::InvalidVersionRequest(err))));
            }
            Box::new({
                debug!("Searching for {request} in {sources}");
                python_installations(
                    version,
                    None,
                    platform,
                    environments,
                    preference,
                    cache,
                    strategy,
                )
                .map_ok(Ok)
            })
        }
        PythonRequest::Implementation(implementation) => Box::new({
            debug!("Searching for a {request} interpreter in {sources}");
            python_installations(
                &VersionRequest::Default,
                Some(implementation),
                platform,
                environments,
                preference,
                cache,
                strategy,
            )
            .filter_ok(move |installation| {
                installation
                    .interpreter
                    .matches_implementation(*implementation)
            })
            .map_ok(Ok)
        }),
        PythonRequest::ImplementationVersion(implementation, version) => {
            if let Err(err) = version.check_supported() {
                return Box::new(iter::once(Err(Error::InvalidVersionRequest(err))));
            }
            Box::new({
                debug!("Searching for {request} in {sources}");
                python_installations(
                    version,
                    Some(implementation),
                    platform,
                    environments,
                    preference,
                    cache,
                    strategy,
                )
                .filter_ok(move |installation| {
                    installation
                        .interpreter
                        .matches_implementation(*implementation)
                })
                .map_ok(Ok)
            })
        }
        PythonRequest::Key(request) => {
            if let Some(version) = request.version()
                && let Err(err) = version.check_supported()
            {
                return Box::new(iter::once(Err(Error::InvalidVersionRequest(err))));
            }

            Box::new({
                debug!("Searching for {request} in {sources}");
                python_installations(
                    request.version().unwrap_or(&VersionRequest::Default),
                    request.implementation(),
                    request.platform().with_default_arch(arch),
                    environments,
                    preference,
                    cache,
                    strategy,
                )
                .filter_ok(move |installation| {
                    installation.interpreter.matches_download_request(request)
                })
                .map_ok(Ok)
            })
        }
    }
}

/// Find all Python installations that satisfy the given request, querying interpreters
/// concurrently.
///
/// Eagerly collects matching installations. Interpreter query failures produce warnings and are
/// skipped. Other non-critical discovery errors are dropped, while critical errors are propagated in
/// discovery order.
pub fn find_all_python_installations(
    request: &PythonRequest,
    environments: EnvironmentPreference,
    preference: PythonPreference,
    arch: Option<PythonArchitecture>,
    cache: &Cache,
) -> Result<Vec<PythonInstallation>, Error> {
    let results = find_python_installations_with_strategy(
        request,
        environments,
        preference,
        arch,
        cache,
        QueryStrategy::Parallel,
    );
    let mut installations = Vec::new();
    for result in results {
        match result {
            Ok(Ok(installation)) => installations.push(installation),
            Ok(Err(_)) => {}
            Err(err @ Error::Query(..)) => {
                warn_user_with_chain!(&err);
            }
            Err(err) if err.is_critical() => return Err(err),
            Err(_) => {}
        }
    }
    Ok(installations)
}

/// Find a Python installation that satisfies the given request.
///
/// If an error is encountered while locating or inspecting a candidate installation,
/// the error will raised instead of attempting further candidates.
pub(crate) fn find_python_installation(
    request: &PythonRequest,
    environments: EnvironmentPreference,
    preference: PythonPreference,
    arch: Option<PythonArchitecture>,
    cache: &Cache,
) -> Result<FindPythonResult, Error> {
    let installations = find_python_installations(request, environments, preference, arch, cache);
    let mut first_prerelease = None;
    let mut first_debug = None;
    let mut first_managed = None;
    let mut first_error = None;
    for result in installations {
        // Iterate until the first critical error or happy result
        if !result.as_ref().err().is_none_or(Error::is_critical) {
            // Track the first non-critical error
            if first_error.is_none()
                && let Err(err) = result
            {
                first_error = Some(err);
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
        if matches!(preference, PythonPreference::System) && installation.is_managed() {
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
        request: request
            .with_default_arch(arch.map(PythonArchitecture::into_inner))
            .into_owned(),
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
    arch: Option<PythonArchitecture>,
    downloads_enabled: bool,
    client_builder: &BaseClientBuilder<'_>,
    cache: &Cache,
    reporter: Option<&dyn uv_python_managed::downloads::Reporter>,
    mirrors: PythonDownloadMirrors<'_>,
    python_downloads_json_url: Option<&str>,
) -> Result<PythonInstallation, crate::Error> {
    debug!("Starting Python discovery for {request}");
    let original_request = request;

    let mut previous_fetch_failed = false;
    let mut download_state = None;

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
        let result = find_python_installation(request, environments, preference, arch, cache);
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
            let (client, retry_policy, download_list) =
                if let Some(download_state) = &mut download_state {
                    download_state
                } else {
                    let download_list = ManagedPythonDownloadList::new(
                        client_builder,
                        cache,
                        python_downloads_json_url,
                    )
                    .await?;
                    let retry_policy = client_builder.retry_policy();

                    // Python downloads are performing their own retries to catch stream errors, disable
                    // the default retries to avoid the middleware performing uncontrolled retries.
                    let client = client_builder.clone().retries(0).build()?;
                    download_state.insert((client, retry_policy, download_list))
                };

            let download = download_request
                .clone()
                .with_default_arch(arch.map(PythonArchitecture::into_inner))
                .fill()
                .map(|request| download_list.find(&request));

            let result = match download {
                Ok(Ok(download)) => PythonInstallation::fetch(
                    download,
                    client,
                    retry_policy,
                    cache,
                    reporter,
                    mirrors,
                )
                .await
                .map(Some),
                Ok(Err(uv_python_managed::downloads::Error::NoDownloadFound(_))) => Ok(None),
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
                // If the request was for the default or any version, propagate
                // the error as nothing else we are about to do will help the
                // situation.
                if matches!(request, PythonRequest::Default | PythonRequest::Any) {
                    return Err(error);
                }

                warn_user_with_chain!(
                    anyhow::Error::from(error)
                        .context(format!(
                            "A managed Python download is available for {request}, but an error occurred when attempting to download it."
                        ))
                        .as_ref()
                );
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
                    request: original_request
                        .with_default_arch(arch.map(PythonArchitecture::into_inner))
                        .into_owned(),
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
fn is_windows_store_shim(path: &Path) -> bool {
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

#[cfg(test)]
mod tests {
    use std::assert_matches;
    use std::{cell::Cell, io, path::PathBuf};

    use test_log::test;
    use uv_cache::Cache;

    use uv_python_types::PythonRequest;

    use super::{
        DiscoveryPreferences, EnvironmentPreference, Error, InterpreterError,
        PythonExecutableGroup, PythonPreference, PythonSource, QueryStrategy,
        python_installations_from_executables, sort_installations_by_key,
    };

    // Testing this at a higher level would necessitate relying on filesystem ordering.
    #[test]
    fn installation_key_order_only_partitions_critical_errors() {
        let query_error = |error| {
            Error::Query(
                Box::new(error),
                PathBuf::from("python"),
                PythonSource::SearchPath,
            )
        };

        let mut installations = [
            Ok(1_u8),
            Err(query_error(InterpreterError::NotFound(PathBuf::from(
                "missing",
            )))),
            Ok(2),
            Err(query_error(InterpreterError::Io(io::Error::other(
                "critical",
            )))),
            Ok(3),
        ];

        sort_installations_by_key(&mut installations, |key| *key);

        assert_matches!(
            &installations[..],
            [Ok(2), Ok(1), Err(noncritical), Err(critical), Ok(3)]
                if !noncritical.is_critical() && critical.is_critical()
        );
    }

    #[test]
    fn sequential_query_strategy_does_not_prefetch_executable_groups() -> anyhow::Result<()> {
        let cache = Cache::temp()?;
        let pulls = Cell::new(0);
        let executables = (0..2).map(|_| {
            pulls.set(pulls.get() + 1);
            Err::<PythonExecutableGroup, _>(Error::SourceNotAllowed(
                PythonRequest::Default,
                PythonSource::SearchPath,
                PythonPreference::OnlyManaged,
            ))
        });

        let mut installations =
            python_installations_from_executables(executables, &cache, QueryStrategy::Sequential);

        assert_eq!(pulls.get(), 0);
        assert!(installations.next().is_some_and(|result| result.is_err()));
        assert_eq!(pulls.get(), 1);

        Ok(())
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
}
