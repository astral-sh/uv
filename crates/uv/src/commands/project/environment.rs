use tracing::debug;

use cache_key::{cache_digest, hash_digest};
use distribution_types::Resolution;
use uv_cache::{Cache, CacheBucket};
use uv_client::Connectivity;
use uv_configuration::{Concurrency, PreviewMode};
use uv_fs::{LockedFile, Simplified};
use uv_python::{Interpreter, PythonEnvironment};
use uv_requirements::RequirementsSpecification;

use crate::commands::project::{resolve_environment, sync_environment};
use crate::commands::SharedState;
use crate::printer::Printer;
use crate::settings::ResolverInstallerSettings;

/// A [`PythonEnvironment`] stored in the cache.
#[derive(Debug)]
pub(crate) struct CachedEnvironment(PythonEnvironment);

impl From<CachedEnvironment> for PythonEnvironment {
    fn from(environment: CachedEnvironment) -> Self {
        environment.0
    }
}

impl CachedEnvironment {
    /// Get or create an [`CachedEnvironment`] based on a given set of requirements and a base
    /// interpreter.
    pub(crate) async fn get_or_create(
        spec: RequirementsSpecification,
        interpreter: Interpreter,
        settings: &ResolverInstallerSettings,
        state: &SharedState,
        preview: PreviewMode,
        connectivity: Connectivity,
        concurrency: Concurrency,
        native_tls: bool,
        cache: &Cache,
        printer: Printer,
    ) -> anyhow::Result<Self> {
        // When caching, always use the base interpreter, rather than that of the virtual
        // environment.
        let interpreter = if let Some(interpreter) = interpreter.to_base_interpreter(cache)? {
            debug!(
                "Caching via base interpreter: `{}`",
                interpreter.sys_executable().display()
            );
            interpreter
        } else {
            debug!(
                "Caching via interpreter: `{}`",
                interpreter.sys_executable().display()
            );
            interpreter
        };

        // Resolve the requirements with the interpreter.
        let graph = resolve_environment(
            &interpreter,
            spec,
            settings.as_ref().into(),
            state,
            preview,
            connectivity,
            concurrency,
            native_tls,
            cache,
            printer,
        )
        .await?;
        let resolution = Resolution::from(graph);

        // Hash the resolution by hashing the generated lockfile.
        // TODO(charlie): If the resolution contains any mutable metadata (like a path or URL
        // dependency), skip this step.
        let resolution_hash = {
            let distributions = resolution.distributions().collect::<Vec<_>>();
            hash_digest(&distributions)
        };

        // Hash the interpreter based on its path.
        // TODO(charlie): Come up with a robust hash for the interpreter.
        let interpreter_hash = cache_digest(&interpreter.sys_executable());

        // Search in the content-addressed cache.
        let cache_entry = cache.entry(CacheBucket::Environments, interpreter_hash, resolution_hash);

        // Lock at the interpreter level, to avoid concurrent modification across processes.
        fs_err::tokio::create_dir_all(cache_entry.dir()).await?;
        let _lock = LockedFile::acquire(
            cache_entry.dir().join(".lock"),
            cache_entry.dir().user_display(),
        )?;

        let ok = cache_entry.path().join(".ok");

        if settings.reinstall.is_none() {
            // If the receipt exists, return the environment.
            if ok.is_file() {
                debug!(
                    "Reusing cached environment at: `{}`",
                    cache_entry.path().display()
                );
                return Ok(Self(PythonEnvironment::from_root(
                    cache_entry.path(),
                    cache,
                )?));
            }
        } else {
            // If the receipt exists, remove it.
            match fs_err::tokio::remove_file(&ok).await {
                Ok(()) => {
                    debug!(
                        "Removed receipt for environment at: `{}`",
                        cache_entry.path().display()
                    );
                }
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => return Err(err.into()),
            }
        }

        debug!(
            "Creating cached environment at: `{}`",
            cache_entry.path().display()
        );

        let venv = uv_virtualenv::create_venv(
            cache_entry.path(),
            interpreter,
            uv_virtualenv::Prompt::None,
            false,
            false,
            false,
        )?;

        let venv = sync_environment(
            venv,
            &resolution,
            settings.as_ref().into(),
            state,
            preview,
            connectivity,
            concurrency,
            native_tls,
            cache,
            printer,
        )
        .await?;

        // Create the receipt, to indicate to future readers that the environment is complete.
        fs_err::tokio::File::create(ok).await?;

        Ok(Self(venv))
    }

    /// Convert the [`CachedEnvironment`] into an [`Interpreter`].
    pub(crate) fn into_interpreter(self) -> Interpreter {
        self.0.into_interpreter()
    }
}
