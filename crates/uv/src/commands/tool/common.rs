use cache_key::digest;
use pypi_types::Requirement;
use tracing::debug;
use uv_cache::{Cache, CacheBucket};
use uv_client::Connectivity;
use uv_configuration::{Concurrency, PreviewMode};
use uv_fs::{LockedFile, Simplified};
use uv_python::{Interpreter, PythonEnvironment};
use uv_requirements::RequirementsSpecification;
use uv_resolver::Lock;

use crate::commands::project::{resolve_environment, sync_environment};
use crate::commands::{project, SharedState};
use crate::printer::Printer;
use crate::settings::ResolverInstallerSettings;

/// Resolve any [`UnnamedRequirements`].
pub(super) async fn resolve_requirements(
    requirements: impl Iterator<Item = &str>,
    interpreter: &Interpreter,
    settings: &ResolverInstallerSettings,
    state: &SharedState,
    preview: PreviewMode,
    connectivity: Connectivity,
    concurrency: Concurrency,
    native_tls: bool,
    cache: &Cache,
    printer: Printer,
) -> anyhow::Result<Vec<Requirement>> {
    // Parse the requirements.
    let requirements = {
        let mut parsed = vec![];
        for requirement in requirements {
            parsed.push(RequirementsSpecification::parse_package(requirement)?);
        }
        parsed
    };

    // Resolve the parsed requirements.
    project::resolve_names(
        requirements,
        interpreter,
        settings,
        state,
        preview,
        connectivity,
        concurrency,
        native_tls,
        cache,
        printer,
    )
    .await
}

/// An ephemeral [`PythonEnvironment`] stored in the cache.
#[derive(Debug)]
pub(super) struct EphemeralEnvironment(PythonEnvironment);

impl From<EphemeralEnvironment> for PythonEnvironment {
    fn from(ephemeral: EphemeralEnvironment) -> Self {
        ephemeral.0
    }
}

impl EphemeralEnvironment {
    /// Get or create an [`EphemeralEnvironment`] based on a given set of requirements and a base
    /// interpreter.
    pub(super) async fn get_or_create(
        requirements: Vec<Requirement>,
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
        let spec = RequirementsSpecification::from_requirements(requirements);

        // Resolve the requirements with the interpreter.
        let resolution = resolve_environment(
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

        // Hash the resolution by hashing the generated lockfile.
        // TODO(charlie): If the resolution contains any mutable metadata (like a path or URL
        // dependency), skip this step.
        let lock = Lock::from_resolution_graph(&resolution)?;
        let toml = lock.to_toml()?;
        let resolution_hash = digest(&toml);

        // Hash the interpreter by hashing the sysconfig data.
        // TODO(charlie): Come up with a robust hash for the interpreter.
        let interpreter_hash = digest(&interpreter.sys_executable());

        // Search in the content-addressed cache.
        let cache_entry = cache.entry(CacheBucket::Environments, interpreter_hash, resolution_hash);

        // Lock the interpreter, to avoid concurrent modification across processes.
        fs_err::tokio::create_dir_all(cache_entry.dir()).await?;
        let _lock = LockedFile::acquire(
            cache_entry.dir().join(".lock"),
            cache_entry.dir().user_display(),
        )?;

        // If the receipt exists, return the environment.
        let ok = cache_entry.path().join(".ok");
        if ok.is_file() {
            return Ok(Self(PythonEnvironment::from_root(
                cache_entry.path(),
                cache,
            )?));
        }

        debug!(
            "Creating ephemeral environment at: `{}`",
            cache_entry.path().display()
        );

        let venv = uv_virtualenv::create_venv(
            cache_entry.path(),
            interpreter,
            uv_virtualenv::Prompt::None,
            false,
            false,
        )?;

        // Install the ephemeral requirements.
        // TODO(charlie): Rather than passing all the arguments to `sync_environment`, return a
        // struct that lets us "continue" from `resolve_environment`.
        let venv = sync_environment(
            venv,
            &resolution.into(),
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
}
