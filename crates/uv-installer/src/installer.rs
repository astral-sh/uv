use anyhow::{Context, Error, Result};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use tracing::instrument;

use distribution_types::CachedDist;
use uv_toolchain::PythonEnvironment;

pub struct Installer<'a> {
    venv: &'a PythonEnvironment,
    link_mode: install_wheel_rs::linker::LinkMode,
    reporter: Option<Box<dyn Reporter>>,
    installer_name: Option<String>,
}

impl<'a> Installer<'a> {
    /// Initialize a new installer.
    pub fn new(venv: &'a PythonEnvironment) -> Self {
        Self {
            venv,
            link_mode: install_wheel_rs::linker::LinkMode::default(),
            reporter: None,
            installer_name: Some("uv".to_string()),
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
    pub fn install(self, wheels: &[CachedDist]) -> Result<()> {
        let layout = self.venv.interpreter().layout();
        tokio::task::block_in_place(|| {
            wheels.par_iter().try_for_each(|wheel| {
                install_wheel_rs::linker::install_wheel(
                    &layout,
                    wheel.path(),
                    wheel.filename(),
                    wheel
                        .parsed_url()?
                        .as_ref()
                        .map(pypi_types::DirectUrl::try_from)
                        .transpose()?
                        .as_ref(),
                    self.installer_name.as_deref(),
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
    /// Callback to invoke when a dependency is installed.
    fn on_install_progress(&self, wheel: &CachedDist);

    /// Callback to invoke when the resolution is complete.
    fn on_install_complete(&self);
}
