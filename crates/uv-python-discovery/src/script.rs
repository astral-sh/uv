//! Interpreter discovery for PEP 723 scripts and environment compatibility.

use std::path::{Path, PathBuf};

use crate::ConfigDiscovery;
use crate::PythonInstallation;
use crate::PythonVersionFile;
use crate::VersionFileDiscoveryOptions;
use tracing::{debug, trace, warn};
use uv_cache::{Cache, CacheBucket};
use uv_cache_key::{cache_digest, cache_name};
use uv_client::BaseClientBuilder;
use uv_command_support::Printer;
use uv_configuration::ActiveEnvironment;
use uv_distribution_types::RequiresPython;
use uv_fs::{CWD, Simplified};
use uv_pep440::Version;
use uv_python_interpreter::{Interpreter, PythonEnvironment, RequestedInterpreter};
use uv_python_types::{
    EnvironmentPreference, PythonArchitecture, PythonDownloads, PythonPreference, PythonRequest,
    PythonSource, PythonVariant, VersionRequest,
};
use uv_scripts::Pep723ItemRef;
use uv_settings::PythonInstallMirrors;
use uv_static::EnvVars;
use uv_warnings::{warn_user, warn_user_once};

use crate::PythonDownloadReporter;
use crate::PythonRequestSource;
use crate::PythonSelectionError;

/// Determine the [`RequiresPython`] requirement for a new PEP 723 script.
pub async fn init_script_python_requirement(
    python: Option<&str>,
    install_mirrors: &PythonInstallMirrors,
    directory: &Path,
    no_pin_python: bool,
    python_preference: PythonPreference,
    python_arch: Option<PythonArchitecture>,
    python_downloads: PythonDownloads,
    config_discovery: ConfigDiscovery,
    client_builder: &BaseClientBuilder<'_>,
    cache: &Cache,
    reporter: &PythonDownloadReporter,
) -> Result<RequiresPython, PythonSelectionError> {
    let python_request = if let Some(request) = python {
        // (1) Explicit request from user
        Some(PythonRequest::parse(request))
    } else if let (false, Some(request)) = (
        no_pin_python,
        PythonVersionFile::discover(
            directory,
            &VersionFileDiscoveryOptions::default().with_config_discovery(config_discovery),
        )
        .await?
        .and_then(PythonVersionFile::into_version),
    ) {
        // (2) Request from `.python-version`
        Some(request)
    } else {
        // (3) No explicit request
        None
    };

    let interpreter = PythonInstallation::find_or_download(
        python_request.as_ref(),
        EnvironmentPreference::Any,
        python_preference,
        python_arch,
        python_downloads,
        client_builder,
        cache,
        Some(reporter),
        install_mirrors.mirrors(),
        install_mirrors.python_downloads_json_url.as_deref(),
    )
    .await?
    .into_interpreter();

    Ok(RequiresPython::greater_than_equal_version(
        &interpreter.python_minor_version(),
    ))
}

/// Returns an error if the [`Interpreter`] does not satisfy the script's `requires-python`.
fn validate_script_requires_python(
    interpreter: &Interpreter,
    requires_python: &RequiresPython,
    source: &PythonRequestSource,
) -> Result<(), PythonSelectionError> {
    if requires_python.contains(interpreter.python_version()) {
        return Ok(());
    }
    match source {
        PythonRequestSource::UserRequest => {
            Err(PythonSelectionError::RequestedPythonScriptIncompatibility(
                interpreter.python_version().clone(),
                requires_python.clone(),
            ))
        }
        PythonRequestSource::DotPythonVersion(file) => {
            Err(PythonSelectionError::DotPythonVersionScriptIncompatibility(
                file.file_name().to_string(),
                interpreter.python_version().clone(),
                requires_python.clone(),
            ))
        }
        PythonRequestSource::RequiresPython => {
            Err(PythonSelectionError::RequiresPythonScriptIncompatibility(
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
    Interpreter(RequestedInterpreter),
    /// An interpreter from an existing script environment.
    Environment(PythonEnvironment),
}

impl ScriptInterpreter {
    /// Return the expected virtual environment path for the [`Pep723ItemRef`].
    ///
    /// If `--active` is set, the active virtual environment will be preferred.
    ///
    /// See: [`uv_workspace::Workspace::environment_selection`].
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
            Err(uv_python_interpreter::PythonEnvironmentError::MissingEnvironment(_)) => None,
            Err(err) => {
                warn!("Ignoring existing script environment: {err}");
                None
            }
        }
    }

    /// Discover the interpreter to use for the current [`Pep723ItemRef`].
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
    ) -> Result<Self, PythonSelectionError> {
        let ScriptPython {
            source,
            python_request,
            requires_python,
        } = ScriptPython::from_request(python_request, script, config_discovery).await?;

        if let Some(environment) = Self::discover_existing(script, active, cache) {
            match check_environment_compatibility(
                &environment,
                EnvironmentKind::Script,
                python_request.as_ref(),
                python_preference,
                python_arch,
                requires_python.as_ref(),
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
            install_mirrors.mirrors(),
            install_mirrors.python_downloads_json_url.as_deref(),
        )
        .await?
        .into_interpreter();

        if let Some(requires_python) = requires_python
            && let Err(err) =
                validate_script_requires_python(&interpreter, &requires_python, &source)
        {
            warn_user!("{err}");
        }

        Ok(Self::Interpreter(RequestedInterpreter::new(
            interpreter,
            python_request.unwrap_or_default(),
        )))
    }

    /// Consume the script selection and return its [`Interpreter`].
    pub fn into_interpreter(self) -> Interpreter {
        match self {
            Self::Interpreter(requested) => requested.into_interpreter(),
            Self::Environment(venv) => venv.into_interpreter(),
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
        if environment.interpreter().matches_request(&request, cache) {
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

    if PythonInstallation::new(
        PythonSource::DiscoveredEnvironment,
        environment.interpreter().clone(),
    )
    .satisfies_preference(&python_preference)
    {
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

/// The resolved Python request and requirement for a [`Pep723ItemRef`].
#[derive(Debug, Clone)]
struct ScriptPython {
    /// The source of the Python request.
    source: PythonRequestSource,
    /// The resolved Python request, computed by considering (1) any explicit request from the user
    /// via `--python`, (2) any implicit request from the user via `.python-version`, (3) any
    /// `Requires-Python` specifier in the script metadata.
    python_request: Option<PythonRequest>,
    /// The resolved Python requirement for the script.
    requires_python: Option<RequiresPython>,
}

impl ScriptPython {
    /// Determine the [`ScriptPython`] for the current [`Pep723ItemRef`].
    async fn from_request(
        python_request: Option<PythonRequest>,
        script: Pep723ItemRef<'_>,
        config_discovery: ConfigDiscovery,
    ) -> Result<Self, PythonSelectionError> {
        let requires_python = script
            .metadata()
            .requires_python
            .as_ref()
            .map(|specifiers| RequiresPython::from_specifiers(specifiers.clone()));

        let project_dir = script.path().and_then(Path::parent).unwrap_or(&**CWD);

        let (source, python_request) = if let Some(request) = python_request {
            // (1) Explicit request from user
            (PythonRequestSource::UserRequest, Some(request))
        } else if let Some(file) = PythonVersionFile::discover(
            project_dir,
            &VersionFileDiscoveryOptions::default().with_config_discovery(config_discovery),
        )
        .await?
        .filter(|file| {
            // Ignore version files that are incompatible with the script's `requires-python`
            match (file.version(), requires_python.as_ref()) {
                (Some(request), Some(requires_python)) => {
                    request.intersects_specifiers(requires_python.specifiers())
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
            // (4) No Python request or requirement.
            (PythonRequestSource::RequiresPython, None)
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
