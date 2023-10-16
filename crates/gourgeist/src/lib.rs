use std::io;
use std::ops::Deref;
use std::path::{Path, PathBuf};

use camino::{Utf8Path, Utf8PathBuf};
use dirs::cache_dir;
use tempfile::PersistError;
use thiserror::Error;

pub use interpreter::{get_interpreter_info, parse_python_cli, InterpreterInfo};

use crate::bare::create_bare_venv;

mod bare;
mod interpreter;
#[cfg(feature = "install")]
mod packages;
#[cfg(not(feature = "install"))]
mod virtualenv_cache;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    IO(#[from] io::Error),
    /// It's effectively an io error with extra info
    #[error(transparent)]
    Persist(#[from] PersistError),
    /// Adds url and target path to the io error
    #[error("Failed to download wheel from {url} to {path}")]
    WheelDownload {
        url: String,
        path: Utf8PathBuf,
        #[source]
        err: io::Error,
    },
    #[error("Failed to determine python interpreter to use")]
    InvalidPythonInterpreter(#[source] Box<dyn std::error::Error + Sync + Send>),
    #[error("Failed to query python interpreter at {interpreter}")]
    PythonSubcommand {
        interpreter: Utf8PathBuf,
        #[source]
        err: io::Error,
    },
    #[cfg(feature = "install")]
    #[error("Failed to contact pypi")]
    Request(#[from] reqwest::Error),
    #[cfg(feature = "install")]
    #[error("Failed to install {package}")]
    InstallWheel {
        package: String,
        #[source]
        err: install_wheel_rs::Error,
    },
    #[error("{0} is not a valid UTF-8 path")]
    NonUTF8Path(PathBuf),
}

/// Provides the paths inside a venv
pub struct Venv(Utf8PathBuf);

impl Deref for Venv {
    type Target = Utf8Path;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Venv {
    pub fn new(location: impl Into<PathBuf>) -> Result<Self, Error> {
        let location = Utf8PathBuf::from_path_buf(location.into()).map_err(Error::NonUTF8Path)?;
        Ok(Self(location))
    }

    /// Returns the location of the python interpreter
    pub fn python_interpreter(&self) -> PathBuf {
        #[cfg(unix)]
        {
            self.0.join("bin").join("python").into_std_path_buf()
        }
        #[cfg(windows)]
        {
            self.0
                .join("Scripts")
                .join("python.exe")
                .into_std_path_buf()
        }
        #[cfg(not(any(unix, windows)))]
        {
            compile_error!("Only windows and unix (linux, mac os, etc.) are supported")
        }
    }
}

pub(crate) fn crate_cache_dir() -> io::Result<Utf8PathBuf> {
    Ok(cache_dir()
        .and_then(|path| Utf8PathBuf::from_path_buf(path).ok())
        .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "Couldn't detect cache dir"))?
        .join(env!("CARGO_PKG_NAME")))
}

/// Create a virtualenv and if not bare, install `wheel`, `pip` and `setuptools`.
pub fn create_venv(
    location: impl Into<PathBuf>,
    base_python: impl AsRef<Path>,
    info: &InterpreterInfo,
    bare: bool,
) -> Result<Venv, Error> {
    let location = Utf8PathBuf::from_path_buf(location.into()).map_err(Error::NonUTF8Path)?;
    let base_python = Utf8Path::from_path(base_python.as_ref())
        .ok_or_else(|| Error::NonUTF8Path(base_python.as_ref().to_path_buf()))?;

    let paths = create_bare_venv(&location, base_python, info)?;

    if !bare {
        #[cfg(feature = "install")]
        {
            packages::install_base_packages(&location, info, &paths)?;
        }
        #[cfg(not(feature = "install"))]
        {
            virtualenv_cache::install_base_packages(
                &paths.bin,
                &paths.interpreter,
                &paths.site_packages,
            )?;
        }
    }

    Ok(Venv(location))
}
