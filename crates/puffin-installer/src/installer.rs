use anyhow::{Error, Result};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

use pep440_rs::Version;
use puffin_interpreter::PythonExecutable;
use puffin_package::package_name::PackageName;

use crate::CachedDistribution;

pub struct Installer<'a> {
    python: &'a PythonExecutable,
    link_mode: install_wheel_rs::linker::LinkMode,
    reporter: Option<Box<dyn Reporter>>,
}

impl<'a> Installer<'a> {
    /// Initialize a new installer.
    pub fn new(python: &'a PythonExecutable) -> Self {
        Self {
            python,
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
    pub fn install(self, wheels: &[CachedDistribution]) -> Result<()> {
        tokio::task::block_in_place(|| {
            wheels.par_iter().try_for_each(|wheel| {
                let location = install_wheel_rs::InstallLocation::new(
                    self.python.venv().to_path_buf(),
                    self.python.simple_version(),
                );

                install_wheel_rs::linker::install_wheel(&location, wheel.path(), self.link_mode)?;

                if let Some(reporter) = self.reporter.as_ref() {
                    reporter.on_install_progress(wheel.name(), wheel.version());
                }

                Ok::<(), Error>(())
            })
        })
    }
}

pub trait Reporter: Send + Sync {
    /// Callback to invoke when a dependency is resolved.
    fn on_install_progress(&self, name: &PackageName, version: &Version);

    /// Callback to invoke when the resolution is complete.
    fn on_install_complete(&self);
}
