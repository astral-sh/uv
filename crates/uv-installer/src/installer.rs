use std::convert;
use std::sync::{Arc, LazyLock};

use anyhow::{Context, Error, Result};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use tokio::sync::oneshot;
use tracing::{instrument, warn};

use uv_cache::Cache;
use uv_configuration::RAYON_INITIALIZE;
use uv_distribution_types::CachedDist;
use uv_install_wheel::{Layout, LinkMode};
use uv_preview::Preview;
use uv_python::PythonEnvironment;

pub struct Installer<'a> {
    venv: &'a PythonEnvironment,
    link_mode: LinkMode,
    cache: Option<&'a Cache>,
    reporter: Option<Arc<dyn Reporter>>,
    /// The name of the [`Installer`].
    name: Option<String>,
    /// The metadata associated with the [`Installer`].
    metadata: bool,
    /// Preview settings for the installer.
    preview: Preview,
}

impl<'a> Installer<'a> {
    /// Initialize a new installer.
    pub fn new(venv: &'a PythonEnvironment, preview: Preview) -> Self {
        Self {
            venv,
            link_mode: LinkMode::default(),
            cache: None,
            reporter: None,
            name: Some("uv".to_string()),
            metadata: true,
            preview,
        }
    }

    /// Set the [`LinkMode`][`uv_install_wheel::LinkMode`] to use for this installer.
    #[must_use]
    pub fn with_link_mode(self, link_mode: LinkMode) -> Self {
        Self { link_mode, ..self }
    }

    /// Set the [`Cache`] to use for this installer.
    #[must_use]
    pub fn with_cache(self, cache: &'a Cache) -> Self {
        Self {
            cache: Some(cache),
            ..self
        }
    }

    /// Set the [`Reporter`] to use for this installer.
    #[must_use]
    pub fn with_reporter(self, reporter: Arc<dyn Reporter>) -> Self {
        Self {
            reporter: Some(reporter),
            ..self
        }
    }

    /// Set the `installer_name` to something other than `"uv"`.
    #[must_use]
    pub fn with_installer_name(self, installer_name: Option<String>) -> Self {
        Self {
            name: installer_name,
            ..self
        }
    }

    /// Set whether to install uv-specifier files in the dist-info directory.
    #[must_use]
    pub fn with_installer_metadata(self, installer_metadata: bool) -> Self {
        Self {
            metadata: installer_metadata,
            ..self
        }
    }

    /// Install a set of wheels into a Python virtual environment.
    #[instrument(skip_all, fields(num_wheels = %wheels.len()))]
    pub async fn install(self, wheels: Vec<CachedDist>) -> Result<Vec<CachedDist>> {
        let Self {
            venv,
            cache,
            link_mode,
            reporter,
            name: installer_name,
            metadata: installer_metadata,
            preview,
        } = self;

        if cache.is_some_and(Cache::is_temporary) {
            if link_mode.is_symlink() {
                return Err(anyhow::anyhow!(
                    "Symlink-based installation is not supported with `--no-cache`. The created environment will be rendered unusable by the removal of the cache."
                ));
            }
        }

        let (tx, rx) = oneshot::channel();

        let layout = venv.interpreter().layout();
        let relocatable = venv.relocatable();
        // Initialize the threadpool with the user settings.
        LazyLock::force(&RAYON_INITIALIZE);
        rayon::spawn(move || {
            let result = install(
                wheels,
                &layout,
                installer_name.as_deref(),
                link_mode,
                reporter.as_ref(),
                relocatable,
                installer_metadata,
                preview,
            );

            // This may fail if the main task was cancelled.
            let _ = tx.send(result);
        });

        rx.await
            .map_err(|_| anyhow::anyhow!("`install_blocking` task panicked"))
            .and_then(convert::identity)
    }

    /// Install a set of wheels into a Python virtual environment synchronously.
    #[instrument(skip_all, fields(num_wheels = %wheels.len()))]
    pub fn install_blocking(self, wheels: Vec<CachedDist>) -> Result<Vec<CachedDist>> {
        if self.cache.is_some_and(Cache::is_temporary) {
            if self.link_mode.is_symlink() {
                return Err(anyhow::anyhow!(
                    "Symlink-based installation is not supported with `--no-cache`. The created environment will be rendered unusable by the removal of the cache."
                ));
            }
        }

        install(
            wheels,
            &self.venv.interpreter().layout(),
            self.name.as_deref(),
            self.link_mode,
            self.reporter.as_ref(),
            self.venv.relocatable(),
            self.metadata,
            self.preview,
        )
    }
}

/// Install a set of wheels into a Python virtual environment synchronously.
#[instrument(skip_all, fields(num_wheels = %wheels.len()))]
fn install(
    wheels: Vec<CachedDist>,
    layout: &Layout,
    installer_name: Option<&str>,
    link_mode: LinkMode,
    reporter: Option<&Arc<dyn Reporter>>,
    relocatable: bool,
    installer_metadata: bool,
    preview: Preview,
) -> Result<Vec<CachedDist>> {
    // Initialize the threadpool with the user settings.
    LazyLock::force(&RAYON_INITIALIZE);
    let state = uv_install_wheel::InstallState::new(preview);
    wheels.par_iter().try_for_each(|wheel| {
        uv_install_wheel::install_wheel(
            layout,
            relocatable,
            wheel.path(),
            wheel.filename(),
            wheel
                .parsed_url()
                .map(uv_pypi_types::DirectUrl::from)
                .as_ref(),
            if wheel.cache_info().is_empty() {
                None
            } else {
                Some(wheel.cache_info())
            },
            wheel.build_info(),
            installer_name,
            installer_metadata,
            link_mode,
            &state,
        )
        .with_context(|| format!("Failed to install: {} ({wheel})", wheel.filename()))?;

        if let Some(reporter) = reporter.as_ref() {
            reporter.on_install_progress(wheel);
        }

        Ok::<(), Error>(())
    })?;
    if let Err(err) = state.warn_package_conflicts() {
        warn!("Checking for conflicts between packages failed: {err}");
    }

    Ok(wheels)
}

pub trait Reporter: Send + Sync {
    /// Callback to invoke when a dependency is installed.
    fn on_install_progress(&self, wheel: &CachedDist);

    /// Callback to invoke when the resolution is complete.
    fn on_install_complete(&self);
}
