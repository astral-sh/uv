//! Interpreter discovery for PEP 723 scripts and environment compatibility.

use std::path::{Path, PathBuf};

use tracing::{debug, trace, warn};
use uv_cache::{Cache, CacheBucket};
use uv_cache_key::{cache_digest, cache_name};
use uv_client::BaseClientBuilder;
use uv_command_support::Printer;
use uv_configuration::{ActiveEnvironment, DependencyGroupsWithDefaults};
use uv_distribution_types::RequiresPython;
use uv_fs::{CWD, LockedFile, LockedFileError, LockedFileMode, Simplified};
use uv_pep440::Version;
use uv_python::{
    ConfigDiscovery, EnvironmentPreference, Interpreter, PythonArchitecture, PythonDownloads,
    PythonEnvironment, PythonInstallation, PythonPreference, PythonRequest, PythonSource,
    PythonVariant, PythonVersionFile, VersionFileDiscoveryOptions, VersionRequest,
};
use uv_scripts::Pep723ItemRef;
use uv_settings::PythonInstallMirrors;
use uv_static::EnvVars;
use uv_warnings::{warn_user, warn_user_once};
use uv_workspace::Workspace;

use crate::{
    PythonContextError, PythonDownloadReporter, PythonRequestSource, PythonRequirementSource,
    find_requires_python, validate_python_requirement,
};

/// Returns an error if the [`Interpreter`] does not satisfy script or workspace `requires-python`.
pub fn validate_script_requires_python(
    interpreter: &Interpreter,
    requires_python: &RequiresPython,
    source: &PythonRequestSource,
) -> Result<(), PythonContextError> {
    if requires_python.contains(interpreter.python_version()) {
        return Ok(());
    }
    match source {
        PythonRequestSource::UserRequest => {
            Err(PythonContextError::RequestedPythonScriptIncompatibility(
                interpreter.python_version().clone(),
                requires_python.clone(),
            ))
        }
        PythonRequestSource::DotPythonVersion(file) => {
            Err(PythonContextError::DotPythonVersionScriptIncompatibility(
                file.file_name().to_string(),
                interpreter.python_version().clone(),
                requires_python.clone(),
            ))
        }
        PythonRequestSource::RequiresPython => {
            Err(PythonContextError::RequiresPythonScriptIncompatibility(
                interpreter.python_version().clone(),
                requires_python.clone(),
            ))
        }
    }
}

/// An interpreter suitable for a PEP 723 script.
#[derive(Debug, Clone)]
#[expect(clippy::large_enum_variant)]
pub enum ScriptInterpreter {
    /// An interpreter to use to create a new script environment.
    Interpreter(Interpreter),
    /// An interpreter from an existing script environment.
    Environment(PythonEnvironment),
}

impl ScriptInterpreter {
    /// Return the expected virtual environment path for the [`Pep723Script`].
    ///
    /// If `--active` is set, the active virtual environment will be preferred.
    ///
    /// See: [`Workspace::environment_selection`].
    pub fn root(script: Pep723ItemRef<'_>, active: ActiveEnvironment, cache: &Cache) -> PathBuf {
        /// Resolve the `VIRTUAL_ENV` variable, if any.
        fn from_virtual_env_variable() -> Option<PathBuf> {
            let value = std::env::var_os(EnvVars::VIRTUAL_ENV)?;

            if value.is_empty() {
                return None;
            }

            let path = PathBuf::from(value);
            if path.is_absolute() {
                return Some(path);
            }

            // Resolve the path relative to current directory.
            Some(CWD.join(path))
        }

        // Determine the stable path to the script environment in the cache.
        let cache_env = {
            let entry = match script {
                // For local scripts, use a hash of the path to the script.
                Pep723ItemRef::Script(script) => {
                    let digest = cache_digest(&script.path);
                    if let Some(file_name) = script
                        .path
                        .file_stem()
                        .and_then(|name| name.to_str())
                        .and_then(|name| cache_name(name, Some(100)))
                    {
                        format!("{file_name}-{digest}")
                    } else {
                        digest
                    }
                }
                // For remote scripts, use a hash of the URL.
                Pep723ItemRef::Remote(.., url) => cache_digest(url),
                // Otherwise, use a hash of the metadata.
                Pep723ItemRef::Stdin(metadata) => cache_digest(&metadata.raw),
            };

            cache
                .shard(CacheBucket::Environments, entry)
                .into_path_buf()
        };

        // If `--active` is set, prefer the active virtual environment.
        if let Some(from_virtual_env) = from_virtual_env_variable() {
            if !uv_fs::is_same_file_allow_missing(&from_virtual_env, &cache_env).unwrap_or(false) {
                match active {
                    ActiveEnvironment::Prefer => {
                        debug!(
                            "Using active virtual environment `{}` instead of script environment `{}`",
                            from_virtual_env.user_display(),
                            cache_env.user_display()
                        );
                        return from_virtual_env;
                    }
                    ActiveEnvironment::Ignore => {}
                    ActiveEnvironment::Warn => {
                        warn_user_once!(
                            "`VIRTUAL_ENV={}` does not match the script environment path `{}` and will be ignored; use `--active` to target the active environment instead",
                            from_virtual_env.user_display(),
                            cache_env.user_display()
                        );
                    }
                }
            }
        } else {
            if active == ActiveEnvironment::Prefer {
                debug!(
                    "Use of the active virtual environment was requested, but `VIRTUAL_ENV` is not set"
                );
            }
        }

        // Otherwise, use the cache root.
        cache_env
    }

    /// Discover an existing script environment without selecting or downloading an interpreter.
    pub fn discover_existing(
        script: Pep723ItemRef<'_>,
        active: ActiveEnvironment,
        cache: &Cache,
    ) -> Option<PythonEnvironment> {
        let root = Self::root(script, active, cache);
        match PythonEnvironment::from_root(&root, cache) {
            Ok(environment) => Some(environment),
            Err(uv_python::Error::MissingEnvironment(_)) => None,
            Err(err) => {
                warn!("Ignoring existing script environment: {err}");
                None
            }
        }
    }

    /// Discover the interpreter to use for the current [`Pep723Item`].
    pub async fn discover(
        script: Pep723ItemRef<'_>,
        python_request: Option<PythonRequest>,
        client_builder: &BaseClientBuilder<'_>,
        python_preference: PythonPreference,
        python_arch: Option<PythonArchitecture>,
        python_downloads: PythonDownloads,
        install_mirrors: &PythonInstallMirrors,
        keep_incompatible: bool,
        config_discovery: ConfigDiscovery,
        active: ActiveEnvironment,
        cache: &Cache,
        printer: Printer,
    ) -> Result<Self, PythonContextError> {
        // For now, we assume that scripts are never evaluated in the context of a workspace.
        let workspace = None;

        let ScriptPython {
            source,
            python_request,
            requires_python,
        } = ScriptPython::from_request(python_request, workspace, script, config_discovery).await?;

        if let Some(environment) = Self::discover_existing(script, active, cache) {
            match check_environment_compatibility(
                &environment,
                EnvironmentKind::Script,
                python_request.as_ref(),
                python_preference,
                python_arch,
                requires_python
                    .as_ref()
                    .map(|(requires_python, _)| requires_python),
                cache,
            ) {
                Ok(()) => return Ok(Self::Environment(environment)),
                Err(err) if keep_incompatible => {
                    warn_user!(
                        "Using incompatible environment (`{}`) due to `--no-sync` ({err})",
                        environment.root().user_display().cyan(),
                    );
                    return Ok(Self::Environment(environment));
                }
                Err(err) => {
                    debug!("{err}");
                }
            }
        }

        let reporter = PythonDownloadReporter::single(printer);

        let interpreter = PythonInstallation::find_or_download(
            python_request.as_ref(),
            EnvironmentPreference::Any,
            python_preference,
            python_arch,
            python_downloads,
            client_builder,
            cache,
            Some(&reporter),
            install_mirrors.python_install_mirror.as_deref(),
            install_mirrors.pypy_install_mirror.as_deref(),
            install_mirrors.python_downloads_json_url.as_deref(),
        )
        .await?
        .into_interpreter();

        if let Err(err) = match requires_python {
            Some((requires_python, RequiresPythonSource::Project)) => {
                let sources = workspace
                    .and_then(|workspace| {
                        workspace
                            .requires_python(&DependencyGroupsWithDefaults::none())
                            .ok()
                    })
                    .unwrap_or_default();
                validate_python_requirement(
                    &interpreter,
                    &requires_python,
                    &source,
                    &PythonRequirementSource::Workspace {
                        sources,
                        multiple_members: workspace
                            .is_some_and(|workspace| workspace.packages().len() > 1),
                    },
                )
            }
            Some((requires_python, RequiresPythonSource::Script)) => {
                validate_script_requires_python(&interpreter, &requires_python, &source)
            }
            None => Ok(()),
        } {
            warn_user!("{err}");
        }

        Ok(Self::Interpreter(interpreter))
    }

    /// Consume the [`PythonInstallation`] and return the [`Interpreter`].
    pub fn into_interpreter(self) -> Interpreter {
        match self {
            Self::Interpreter(interpreter) => interpreter,
            Self::Environment(venv) => venv.into_interpreter(),
        }
    }

    /// Grab a file lock for the script to prevent concurrent writes across processes.
    pub async fn lock(script: Pep723ItemRef<'_>) -> Result<LockedFile, LockedFileError> {
        match script {
            Pep723ItemRef::Script(script) => {
                LockedFile::acquire(
                    std::env::temp_dir().join(format!("uv-{}.lock", cache_digest(&script.path))),
                    LockedFileMode::Exclusive,
                    script.path.simplified_display(),
                )
                .await
            }
            Pep723ItemRef::Remote(.., url) => {
                LockedFile::acquire(
                    std::env::temp_dir().join(format!("uv-{}.lock", cache_digest(url))),
                    LockedFileMode::Exclusive,
                    url.to_string(),
                )
                .await
            }
            Pep723ItemRef::Stdin(metadata) => {
                LockedFile::acquire(
                    std::env::temp_dir().join(format!("uv-{}.lock", cache_digest(&metadata.raw))),
                    LockedFileMode::Exclusive,
                    "stdin".to_string(),
                )
                .await
            }
        }
    }
}

#[derive(Debug)]
pub enum EnvironmentKind {
    Script,
    Project,
}

impl std::fmt::Display for EnvironmentKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Script => write!(f, "script"),
            Self::Project => write!(f, "project"),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum EnvironmentIncompatibilityError {
    #[error("The {0} environment's Python version does not satisfy the request: `{1}`")]
    PythonRequest(EnvironmentKind, PythonRequest),

    #[error("The {0} environment's Python version does not meet the Python requirement: `{1}`")]
    RequiresPython(EnvironmentKind, RequiresPython),

    #[error(
        "The interpreter in the {0} environment has a different version ({1}) than it was created with ({2})"
    )]
    PyenvVersionConflict(EnvironmentKind, Version, Version),

    #[error("The {0} environment's Python interpreter does not meet the Python preference: `{1}`")]
    PythonPreference(EnvironmentKind, PythonPreference),
}

/// Check whether an environment satisfies the requested Python constraints.
pub fn check_environment_compatibility(
    environment: &PythonEnvironment,
    kind: EnvironmentKind,
    python_request: Option<&PythonRequest>,
    python_preference: PythonPreference,
    python_arch: Option<PythonArchitecture>,
    requires_python: Option<&RequiresPython>,
    cache: &Cache,
) -> Result<(), EnvironmentIncompatibilityError> {
    if let Some((cfg_version, int_version)) = environment.get_pyvenv_version_conflict() {
        return Err(EnvironmentIncompatibilityError::PyenvVersionConflict(
            kind,
            int_version,
            cfg_version,
        ));
    }

    let python_request = python_request
        .or_else(|| python_arch.map(|_| &PythonRequest::Any))
        .map(|request| request.with_default_arch(python_arch.map(PythonArchitecture::into_inner)));
    if let Some(request) = python_request {
        if request.satisfied(environment.interpreter(), cache) {
            debug!("The {kind} environment's Python version satisfies the request: `{request}`");
        } else {
            return Err(EnvironmentIncompatibilityError::PythonRequest(
                kind,
                request.into_owned(),
            ));
        }
    }

    if let Some(requires_python) = requires_python {
        if requires_python.contains(environment.interpreter().python_version()) {
            trace!(
                "The {kind} environment's Python version meets the Python requirement: `{requires_python}`"
            );
        } else {
            return Err(EnvironmentIncompatibilityError::RequiresPython(
                kind,
                requires_python.clone(),
            ));
        }
    }

    if python_preference.allows_installation(&PythonInstallation::new(
        PythonSource::DiscoveredEnvironment,
        environment.interpreter().clone(),
    )) {
        trace!(
            "The virtual environment's Python interpreter meets the Python preference: `{}`",
            python_preference
        );
    } else {
        return Err(EnvironmentIncompatibilityError::PythonPreference(
            kind,
            python_preference,
        ));
    }

    Ok(())
}

/// The source of a `Requires-Python` specifier.
#[derive(Debug, Clone)]
enum RequiresPythonSource {
    /// From the PEP 723 inline script metadata.
    Script,
    /// From a `pyproject.toml` in a workspace.
    Project,
}

/// The resolved Python request and requirement for a [`Pep723Script`]
#[derive(Debug, Clone)]
struct ScriptPython {
    /// The source of the Python request.
    source: PythonRequestSource,
    /// The resolved Python request, computed by considering (1) any explicit request from the user
    /// via `--python`, (2) any implicit request from the user via `.python-version`, (3) any
    /// `Requires-Python` specifier in the script metadata, and (4) any `Requires-Python` specifier
    /// in the `pyproject.toml`.
    python_request: Option<PythonRequest>,
    /// The resolved Python requirement for the script and its source.
    requires_python: Option<(RequiresPython, RequiresPythonSource)>,
}

impl ScriptPython {
    /// Determine the [`ScriptPython`] for the current [`Pep723Script`].
    async fn from_request(
        python_request: Option<PythonRequest>,
        workspace: Option<&Workspace>,
        script: Pep723ItemRef<'_>,
        config_discovery: ConfigDiscovery,
    ) -> Result<Self, PythonContextError> {
        let script_requires_python = script
            .metadata()
            .requires_python
            .as_ref()
            .map(|specifiers| RequiresPython::from_specifiers(specifiers.clone()));

        let workspace_requires_python = workspace
            .map(|workspace| find_requires_python(workspace, &DependencyGroupsWithDefaults::none()))
            .transpose()?
            .flatten();

        let workspace_root = workspace.map(Workspace::install_path);
        let project_dir = script.path().and_then(Path::parent).unwrap_or(&**CWD);

        let (source, python_request) = if let Some(request) = python_request {
            // (1) Explicit request from user
            (PythonRequestSource::UserRequest, Some(request))
        } else if let Some(file) = PythonVersionFile::discover(
            project_dir,
            &VersionFileDiscoveryOptions::default()
                .with_stop_discovery_at(workspace_root.map(PathBuf::as_ref))
                .with_config_discovery(config_discovery),
        )
        .await?
        .filter(|file| {
            // Ignore version files that are incompatible with the script's `requires-python`
            match (file.version(), script_requires_python.as_ref()) {
                (Some(request), Some(requires_python)) => {
                    request.intersects_requires_python(requires_python)
                }
                _ => true,
            }
        })
        .filter(|file| {
            // Ignore global version files that are incompatible with the workspace `requires-python`
            if !file.is_global() {
                return true;
            }
            match (file.version(), workspace_requires_python.as_ref()) {
                (Some(request), Some(requires_python)) => {
                    request.intersects_requires_python(requires_python)
                }
                _ => true,
            }
        }) {
            // (2) Request from `.python-version`
            (
                PythonRequestSource::DotPythonVersion(file.clone()),
                file.version().cloned(),
            )
        } else if let Some(specifiers) = script.metadata().requires_python.as_ref() {
            // (3) `requires-python` from script metadata
            let request = PythonRequest::Version(VersionRequest::from_specifiers(
                specifiers.clone(),
                PythonVariant::Default,
            ));
            (PythonRequestSource::RequiresPython, Some(request))
        } else {
            // (4) `requires-python` from workspace `pyproject.toml`
            let request = workspace_requires_python
                .as_ref()
                .and_then(PythonRequest::from_requires_python);
            (PythonRequestSource::RequiresPython, request)
        };

        let requires_python = if let Some(requires_python) = script_requires_python {
            Some((requires_python, RequiresPythonSource::Script))
        } else {
            workspace_requires_python
                .map(|requires_python| (requires_python, RequiresPythonSource::Project))
        };

        if let Some(python_request) = python_request.as_ref() {
            debug!(
                "Using Python request `{}` from {source}",
                python_request.to_canonical_string()
            );
        }

        Ok(Self {
            source,
            python_request,
            requires_python,
        })
    }
}
