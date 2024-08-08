use anyhow::{Context, Error, Result};
use install_wheel_rs::{linker::LinkMode, Layout};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use std::convert;
use tokio::sync::oneshot;
use tracing::instrument;

use distribution_types::CachedDist;
use uv_cache::Cache;
use uv_python::PythonEnvironment;

pub struct Installer<'a> {
    venv: &'a PythonEnvironment,
    link_mode: LinkMode,
    cache: Option<&'a Cache>,
    reporter: Option<Box<dyn Reporter>>,
    installer_name: Option<String>,
}

impl<'a> Installer<'a> {
    /// Initialize a new installer.
    pub fn new(venv: &'a PythonEnvironment) -> Self {
        Self {
            venv,
            link_mode: LinkMode::default(),
            cache: None,
            reporter: None,
            installer_name: Some("uv".to_string()),
        }
    }

    /// Set the [`LinkMode`][`install_wheel_rs::linker::LinkMode`] to use for this installer.
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
    pub fn with_reporter(self, reporter: impl Reporter + 'static) -> Self {
        Self {
            reporter: Some(Box::new(reporter)),
            ..self
        }
    }

    /// Set the `installer_name` to something other than `"uv"`.
    #[must_use]
    pub fn with_installer_name(self, installer_name: Option<String>) -> Self {
        Self {
            installer_name,
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
            installer_name,
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
        rayon::spawn(move || {
            let result = install(
                wheels,
                layout,
                installer_name,
                link_mode,
                reporter,
                relocatable,
            );
            tx.send(result).unwrap();
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
            self.venv.interpreter().layout(),
            self.installer_name,
            self.link_mode,
            self.reporter,
            self.venv.relocatable(),
        )
    }
}

/// Install a set of wheels into a Python virtual environment synchronously.
#[instrument(skip_all, fields(num_wheels = %wheels.len()))]
fn install(
    wheels: Vec<CachedDist>,
    layout: Layout,
    installer_name: Option<String>,
    link_mode: LinkMode,
    reporter: Option<Box<dyn Reporter>>,
    relocatable: bool,
) -> Result<Vec<CachedDist>> {
    let locks = install_wheel_rs::linker::Locks::default();
    wheels.par_iter().try_for_each(|wheel| {
        install_wheel_rs::linker::install_wheel(
            &layout,
            relocatable,
            wheel.path(),
            wheel.filename(),
            wheel
                .parsed_url()?
                .as_ref()
                .map(pypi_types::DirectUrl::try_from)
                .transpose()?
                .as_ref(),
            installer_name.as_deref(),
            link_mode,
            &locks,
        )
        .with_context(|| format!("Failed to install: {} ({wheel})", wheel.filename()))?;

        if let Some(reporter) = reporter.as_ref() {
            reporter.on_install_progress(wheel);
        }

        Ok::<(), Error>(())
    })?;

    Ok(wheels)
}

pub trait Reporter: Send + Sync {
    /// Callback to invoke when a dependency is installed.
    fn on_install_progress(&self, wheel: &CachedDist);

    /// Callback to invoke when the resolution is complete.
    fn on_install_complete(&self);
}
