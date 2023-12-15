use anyhow::{Context, Error, Result};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

use distribution_types::CachedDist;
use puffin_interpreter::Virtualenv;

pub struct Installer<'a> {
    venv: &'a Virtualenv,
    link_mode: install_wheel_rs::linker::LinkMode,
    reporter: Box<dyn Reporter>,
}

impl<'a> Installer<'a> {
    /// Initialize a new installer.
    pub fn new(venv: &'a Virtualenv, reporter: impl Reporter + 'static) -> Self {
        Self {
            venv,
            link_mode: install_wheel_rs::linker::LinkMode::default(),
            reporter: Box::new(reporter),
        }
    }

    /// Set the [`LinkMode`][`install_wheel_rs::linker::LinkMode`] to use for this installer.
    #[must_use]
    pub fn with_link_mode(self, link_mode: install_wheel_rs::linker::LinkMode) -> Self {
        Self { link_mode, ..self }
    }

    /// Install a set of wheels into a Python virtual environment.
    pub fn install(self, wheels: &[CachedDist]) -> Result<()> {
        tokio::task::block_in_place(|| {
            wheels.par_iter().try_for_each(|wheel| {
                let location = install_wheel_rs::InstallLocation::new(
                    self.venv.root(),
                    self.venv.interpreter().simple_version(),
                );

                install_wheel_rs::linker::install_wheel(
                    &location,
                    wheel.path(),
                    wheel
                        .direct_url()?
                        .as_ref()
                        .map(pypi_types::DirectUrl::try_from)
                        .transpose()?
                        .as_ref(),
                    self.link_mode,
                )
                .with_context(|| format!("Failed to install: {wheel}"))?;

                self.reporter.on_install_progress(wheel);

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

pub struct DummyReporter;

impl Reporter for DummyReporter {
    fn on_install_progress(&self, _wheel: &CachedDist) {}
    fn on_install_complete(&self) {}
}
