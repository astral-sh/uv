//! Interpreter discovery and environment selection for PEP 723 scripts.

use std::path::PathBuf;

use tracing::{debug, warn};
use uv_cache::{Cache, CacheBucket};
use uv_cache_key::{cache_digest, cache_name};
use uv_client::BaseClientBuilder;
use uv_command_support::Printer;
use uv_configuration::ActiveEnvironment;
use uv_fs::{CWD, Simplified};
use uv_python::{
    ConfigDiscovery, EnvironmentPreference, Interpreter, PythonArchitecture, PythonDownloads,
    PythonEnvironment, PythonInstallation, PythonPreference, PythonRequest,
};
use uv_python_context::{PythonDownloadReporter, ScriptPythonRequest};
use uv_scripts::Pep723ItemRef;
use uv_settings::PythonInstallMirrors;
use uv_static::EnvVars;
use uv_warnings::{warn_user, warn_user_once};

use crate::EnvironmentError;
use crate::compatibility::{EnvironmentKind, check_environment_compatibility};

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
    /// See: [`uv_workspace::Workspace::environment_selection`].
    pub(crate) fn root(
        script: Pep723ItemRef<'_>,
        active: ActiveEnvironment,
        cache: &Cache,
    ) -> PathBuf {
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
    ) -> Result<Self, EnvironmentError> {
        let script_python =
            ScriptPythonRequest::from_request(python_request, script, config_discovery).await?;
        let python_request = script_python.python_request.as_ref();
        let requires_python = script_python.requires_python();

        if let Some(environment) = Self::discover_existing(script, active, cache) {
            match check_environment_compatibility(
                &environment,
                EnvironmentKind::Script,
                python_request,
                python_preference,
                python_arch,
                requires_python,
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
            python_request,
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

        if let Err(err) = script_python.check(&interpreter) {
            warn_user!("{err}");
        }

        Ok(Self::Interpreter(interpreter))
    }

    /// Consume the [`ScriptInterpreter`] and return the [`Interpreter`].
    pub fn into_interpreter(self) -> Interpreter {
        match self {
            Self::Interpreter(interpreter) => interpreter,
            Self::Environment(venv) => venv.into_interpreter(),
        }
    }
}
