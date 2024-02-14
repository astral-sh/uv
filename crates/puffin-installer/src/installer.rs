use anyhow::{Context, Error, Result};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use tracing::instrument;

use distribution_types::CachedDist;
use uv_interpreter::Virtualenv;

pub struct Installer<'a> {
    venv: &'a Virtualenv,
    link_mode: install_wheel_rs::linker::LinkMode,
    reporter: Option<Box<dyn Reporter>>,
}

impl<'a> Installer<'a> {
    /// Initialize a new installer.
    pub fn new(venv: &'a Virtualenv) -> Self {
        Self {
            venv,
            link_mode: install_wheel_rs::linker::LinkMode::default(),
            reporter: None,
        }
    }

    /// Set the [`LinkMode`][`install_wheel_rs::linker::LinkMode`] to use for this installer.
    #[must_use]
    pub fn with_link_mode(self, link_mode: install_wheel_rs::linker::LinkMode) -> Self {
        Self { link_mode, ..self }
    }

    /// Set the [`Reporter`] to use for this installer.
    #[must_use]
    pub fn with_reporter(self, reporter: impl Reporter + 'static) -> Self {
        Self {
            reporter: Some(Box::new(reporter)),
            ..self
        }
    }

    /// Install a set of wheels into a Python virtual environment.
    #[instrument(skip_all, fields(num_wheels = %wheels.len()))]
    pub fn install(self, wheels: &[CachedDist]) -> Result<()> {
        tokio::task::block_in_place(|| {
            wheels.par_iter().try_for_each(|wheel| {
                let location = install_wheel_rs::InstallLocation::new(
                    self.venv.root(),
                    self.venv.interpreter().python_tuple(),
                );

                install_wheel_rs::linker::install_wheel(
                    &location,
                    wheel.path(),
                    wheel.filename(),
                    wheel
                        .direct_url()?
                        .as_ref()
                        .map(pypi_types::DirectUrl::try_from)
                        .transpose()?
                        .as_ref(),
                    Some("uv"),
                    self.link_mode,
                )
                .with_context(|| format!("Failed to install: {} ({wheel})", wheel.filename()))?;

                if let Some(reporter) = self.reporter.as_ref() {
                    reporter.on_install_progress(wheel);
                }

                Ok::<(), Error>(())
            })
        })
    }
}

pub trait Reporter: Send + Sync {
    /// Callback to invoke when a dependency is resolved.
    fn on_install_progress(&self, wheel: &CachedDist);

    /// Callback to invoke when the resolution is complete.
    fn on_install_complete(&self);
}
