use std::io;
use std::path::{Path, PathBuf};

use camino::{Utf8Path, Utf8PathBuf};
use dirs::cache_dir;
use tempfile::PersistError;
use thiserror::Error;

pub use interpreter::parse_python_cli;
use platform_host::PlatformError;
use puffin_interpreter::{InterpreterInfo, Venv};

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
    #[error(transparent)]
    Platform(#[from] PlatformError),
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

    Ok(Venv::new_prefix(location.as_std_path(), info))
}
