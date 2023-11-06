use std::path::PathBuf;
use anyhow::{Context, Error, Result};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use url::Url;
use install_wheel_rs::DirectUrl;

use puffin_distribution::CachedDistribution;
use puffin_git::Git;
use puffin_interpreter::Virtualenv;

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
    pub fn install(self, wheels: &[CachedDistribution]) -> Result<()> {
        tokio::task::block_in_place(|| {
            wheels.par_iter().try_for_each(|wheel| {
                let location = install_wheel_rs::InstallLocation::new(
                    self.venv.root(),
                    self.venv.interpreter_info().simple_version(),
                );

                install_wheel_rs::linker::install_wheel(
                    &location,
                    wheel.path(),
                    None,
                    self.link_mode,
                )
                    .with_context(|| format!("Failed to install: {wheel}"))?;

                if let Some(reporter) = self.reporter.as_ref() {
                    reporter.on_install_progress(wheel);
                }

                Ok::<(), Error>(())
            })
        })
    }
}

/// Return the [`DirectUrl`] for a wheel, if applicable.
fn direct_url(wheel: &CachedDistribution) -> Result<Option<DirectUrl>> {
    let CachedDistribution::Url(_, url, _) = wheel else {
        return Ok(None);
    };

    // If the URL points to a subdirectory, extract it, as in:
    //   `https://git.example.com/MyProject.git@v1.0#subdirectory=pkg_dir`
    //   `https://git.example.com/MyProject.git@v1.0#egg=pkg&subdirectory=pkg_dir`
    let subdirectory = url.fragment().and_then(|fragment| {
        fragment.split('&').find_map(|fragment| {
            fragment.strip_prefix("subdirectory=").map(PathBuf::from)
        })
    });

    if let Some(url) = url.as_str().strip_prefix("git+") {
        let url = Url::parse(url)?;
        let git = Git::try_from(url)?;
        Ok(Self::Git(git, subdirectory))
    } else {
        Ok(Self::RemoteUrl(url, subdirectory))
    }
}

pub trait Reporter: Send + Sync {
    /// Callback to invoke when a dependency is resolved.
    fn on_install_progress(&self, wheel: &CachedDistribution);

    /// Callback to invoke when the resolution is complete.
    fn on_install_complete(&self);
}
