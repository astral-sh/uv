use tracing::debug;

use uv_cache::{Cache, CacheBucket};
use uv_cache_key::{cache_digest, hash_digest};
use uv_configuration::{Concurrency, PreviewMode};
use uv_distribution_types::{Name, Resolution};
use uv_python::{Interpreter, PythonEnvironment};

use crate::commands::pip::loggers::{InstallLogger, ResolveLogger};
use crate::commands::pip::operations::Modifications;
use crate::commands::project::{
    resolve_environment, sync_environment, EnvironmentSpecification, PlatformState, ProjectError,
};
use crate::printer::Printer;
use crate::settings::{NetworkSettings, ResolverInstallerSettings};

/// A [`PythonEnvironment`] stored in the cache.
#[derive(Debug)]
pub(crate) struct CachedEnvironment(PythonEnvironment);

impl From<CachedEnvironment> for PythonEnvironment {
    fn from(environment: CachedEnvironment) -> Self {
        environment.0
    }
}

impl CachedEnvironment {
    /// Get or create an [`CachedEnvironment`] based on a given set of requirements.
    pub(crate) async fn from_spec(
        spec: EnvironmentSpecification<'_>,
        interpreter: &Interpreter,
        settings: &ResolverInstallerSettings,
        network_settings: &NetworkSettings,
        state: &PlatformState,
        resolve: Box<dyn ResolveLogger>,
        install: Box<dyn InstallLogger>,
        installer_metadata: bool,
        concurrency: Concurrency,
        cache: &Cache,
        printer: Printer,
        preview: PreviewMode,
    ) -> Result<Self, ProjectError> {
        let interpreter = Self::base_interpreter(interpreter, cache)?;

        // Resolve the requirements with the interpreter.
        let resolution = Resolution::from(
            resolve_environment(
                spec,
                &interpreter,
                &settings.resolver,
                network_settings,
                state,
                resolve,
                concurrency,
                cache,
                printer,
                preview,
            )
            .await?,
        );

        // Hash the resolution by hashing the generated lockfile.
        // TODO(charlie): If the resolution contains any mutable metadata (like a path or URL
        // dependency), skip this step.
        let resolution_hash = {
            let mut distributions = resolution.distributions().collect::<Vec<_>>();
            distributions.sort_unstable_by_key(|dist| dist.name());
            hash_digest(&distributions)
        };

        // Hash the interpreter based on its path.
        // TODO(charlie): Come up with a robust hash for the interpreter.
        let interpreter_hash = cache_digest(&interpreter.sys_executable());

        // Search in the content-addressed cache.
        let cache_entry = cache.entry(CacheBucket::Environments, interpreter_hash, resolution_hash);

        if cache.refresh().is_none() {
            if let Ok(root) = cache.resolve_link(cache_entry.path()) {
                if let Ok(environment) = PythonEnvironment::from_root(root, cache) {
                    return Ok(Self(environment));
                }
            }
        }

        // Create the environment in the cache, then relocate it to its content-addressed location.
        let temp_dir = cache.venv_dir()?;
        let venv = uv_virtualenv::create_venv(
            temp_dir.path(),
            interpreter,
            uv_virtualenv::Prompt::None,
            false,
            false,
            true,
            false,
        )?;

        sync_environment(
            venv,
            &resolution,
            Modifications::Exact,
            settings.into(),
            network_settings,
            state,
            install,
            installer_metadata,
            concurrency,
            cache,
            printer,
            preview,
        )
        .await?;

        // Now that the environment is complete, sync it to its content-addressed location.
        let id = cache
            .persist(temp_dir.into_path(), cache_entry.path())
            .await?;
        let root = cache.archive(&id);

        Ok(Self(PythonEnvironment::from_root(root, cache)?))
    }

    /// Set the ephemeral overlay for a Python environment.
    #[allow(clippy::result_large_err)]
    pub(crate) fn set_overlay(&self, contents: impl AsRef<[u8]>) -> Result<(), ProjectError> {
        let site_packages = self
            .0
            .site_packages()
            .next()
            .ok_or(ProjectError::NoSitePackages)?;
        let overlay_path = site_packages.join("_uv_ephemeral_overlay.pth");
        fs_err::write(overlay_path, contents)?;
        Ok(())
    }

    /// Clear the ephemeral overlay for a Python environment, if it exists.
    #[allow(clippy::result_large_err)]
    pub(crate) fn clear_overlay(&self) -> Result<(), ProjectError> {
        let site_packages = self
            .0
            .site_packages()
            .next()
            .ok_or(ProjectError::NoSitePackages)?;
        let overlay_path = site_packages.join("_uv_ephemeral_overlay.pth");
        match fs_err::remove_file(overlay_path) {
            Ok(()) => (),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => (),
            Err(err) => return Err(ProjectError::OverlayRemoval(err)),
        }
        Ok(())
    }

    /// Enable system site packages for a Python environment.
    #[allow(clippy::result_large_err)]
    pub(crate) fn set_system_site_packages(&self) -> Result<(), ProjectError> {
        self.0
            .set_pyvenv_cfg("include-system-site-packages", "true")?;
        Ok(())
    }

    /// Disable system site packages for a Python environment.
    #[allow(clippy::result_large_err)]
    pub(crate) fn clear_system_site_packages(&self) -> Result<(), ProjectError> {
        self.0
            .set_pyvenv_cfg("include-system-site-packages", "false")?;
        Ok(())
    }

    /// Return the [`Interpreter`] to use for the cached environment, based on a given
    /// [`Interpreter`].
    ///
    /// When caching, always use the base interpreter, rather than that of the virtual
    /// environment.
    fn base_interpreter(
        interpreter: &Interpreter,
        cache: &Cache,
    ) -> Result<Interpreter, uv_python::Error> {
        let base_python = if cfg!(unix) {
            interpreter.find_base_python()?
        } else {
            interpreter.to_base_python()?
        };
        if base_python == interpreter.sys_executable() {
            debug!(
                "Caching via base interpreter: `{}`",
                interpreter.sys_executable().display()
            );
            Ok(interpreter.clone())
        } else {
            let base_interpreter = Interpreter::query(base_python, cache)?;
            debug!(
                "Caching via base interpreter: `{}`",
                base_interpreter.sys_executable().display()
            );
            Ok(base_interpreter)
        }
    }
}
