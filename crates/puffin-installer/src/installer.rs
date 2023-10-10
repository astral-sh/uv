use anyhow::Result;
use tempfile::TempDir;

use pep440_rs::Version;
use puffin_interpreter::PythonExecutable;
use puffin_package::package_name::PackageName;

use crate::cache::WheelCache;
use crate::downloader::DownloadSet;
use crate::Distribution;

pub struct Installer<'a> {
    python: &'a PythonExecutable,
    wheel_cache: Option<WheelCache<'a>>,
    wheels: &'a [Distribution],
    staging: TempDir,
    reporter: Option<Box<dyn Reporter>>,
}

impl<'a> Installer<'a> {
    /// Set the [`Reporter`] to use for this downloader.
    #[must_use]
    pub fn with_reporter(self, reporter: impl Reporter + 'static) -> Self {
        Self {
            reporter: Some(Box::new(reporter)),
            ..self
        }
    }

    /// Install a set of wheels into a Python virtual environment.
    pub fn install(self) -> Result<()> {
        // Install each wheel.
        let location = install_wheel_rs::InstallLocation::new(
            self.python.venv().to_path_buf(),
            self.python.simple_version(),
        );
        let locked_dir = location.acquire_lock()?;

        for wheel in self.wheels {
            match wheel {
                Distribution::Remote(remote) => {
                    let id = remote.id();
                    let dir = self.wheel_cache.as_ref().map_or_else(
                        || self.staging.path().join(&id),
                        |wheel_cache| wheel_cache.entry(&id),
                    );
                    install_wheel_rs::unpacked::install_wheel(&locked_dir, &dir)?;
                }
                Distribution::Local(local) => {
                    let dir = local.path();
                    install_wheel_rs::unpacked::install_wheel(&locked_dir, dir)?;
                }
            }

            if let Some(reporter) = self.reporter.as_ref() {
                reporter.on_install_progress(wheel.name(), wheel.version());
            }
        }

        if let Some(reporter) = self.reporter.as_ref() {
            reporter.on_install_complete();
        }

        Ok(())
    }
}

impl<'a> From<DownloadSet<'a>> for Installer<'a> {
    fn from(set: DownloadSet<'a>) -> Self {
        Self {
            python: set.python,
            wheel_cache: set.wheel_cache,
            wheels: set.wheels,
            staging: set.staging,
            reporter: None,
        }
    }
}

pub trait Reporter: Send + Sync {
    /// Callback to invoke when a dependency is resolved.
    fn on_install_progress(&self, name: &PackageName, version: &Version);

    /// Callback to invoke when the resolution is complete.
    fn on_install_complete(&self);
}
