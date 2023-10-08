use crate::bare::create_bare_venv;
use camino::{Utf8Path, Utf8PathBuf};
use dirs::cache_dir;
use interpreter::InterpreterInfo;
use std::io;
use tempfile::PersistError;
use thiserror::Error;

pub use interpreter::{get_interpreter_info, parse_python_cli};

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
    MinReq(#[from] minreq::Error),
    #[cfg(feature = "install")]
    #[error("Failed to install {package}")]
    InstallWheel {
        package: String,
        #[source]
        err: install_wheel_rs::Error,
    },
}

pub(crate) fn crate_cache_dir() -> io::Result<Utf8PathBuf> {
    Ok(cache_dir()
        .and_then(|path| Utf8PathBuf::from_path_buf(path).ok())
        .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "Couldn't detect cache dir"))?
        .join(env!("CARGO_PKG_NAME")))
}

/// Create a virtualenv and if not bare, install `wheel`, `pip` and `setuptools`.
pub fn create_venv(
    location: &Utf8Path,
    base_python: &Utf8Path,
    info: &InterpreterInfo,
    bare: bool,
) -> Result<(), Error> {
    let paths = create_bare_venv(location, base_python, info)?;

    if !bare {
        #[cfg(feature = "install")]
        {
            packages::install_base_packages(location, info, &paths)?;
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

    Ok(())
}
