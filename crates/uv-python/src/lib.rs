//! Find requested Python interpreters and query interpreters for information.
use thiserror::Error;

#[cfg(test)]
use uv_static::EnvVars;

pub use crate::discovery::{
    find_python_installations, EnvironmentPreference, Error as DiscoveryError, PythonDownloads,
    PythonNotFound, PythonPreference, PythonRequest, PythonSource, PythonVariant, VersionRequest,
};
pub use crate::environment::{InvalidEnvironment, InvalidEnvironmentKind, PythonEnvironment};
pub use crate::implementation::ImplementationName;
pub use crate::installation::{PythonInstallation, PythonInstallationKey};
pub use crate::interpreter::{Error as InterpreterError, Interpreter};
pub use crate::pointer_size::PointerSize;
pub use crate::prefix::Prefix;
pub use crate::python_version::PythonVersion;
pub use crate::target::Target;
pub use crate::version_files::{
    DiscoveryOptions as VersionFileDiscoveryOptions, FilePreference as VersionFilePreference,
    PythonVersionFile, PYTHON_VERSIONS_FILENAME, PYTHON_VERSION_FILENAME,
};
pub use crate::virtualenv::{Error as VirtualEnvError, PyVenvConfiguration, VirtualEnvironment};

mod cpuinfo;
mod discovery;
pub mod downloads;
mod environment;
mod implementation;
mod installation;
mod interpreter;
mod libc;
pub mod managed;
#[cfg(windows)]
mod microsoft_store;
pub mod platform;
mod pointer_size;
mod prefix;
#[cfg(windows)]
mod py_launcher;
mod python_version;
mod target;
mod version_files;
mod virtualenv;

#[cfg(not(test))]
pub(crate) fn current_dir() -> Result<std::path::PathBuf, std::io::Error> {
    std::env::current_dir()
}

#[cfg(test)]
pub(crate) fn current_dir() -> Result<std::path::PathBuf, std::io::Error> {
    std::env::var_os(EnvVars::PWD)
        .map(std::path::PathBuf::from)
        .map(Ok)
        .unwrap_or(std::env::current_dir())
}

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    VirtualEnv(#[from] virtualenv::Error),

    #[error(transparent)]
    Query(#[from] interpreter::Error),

    #[error(transparent)]
    Discovery(#[from] discovery::Error),

    #[error(transparent)]
    ManagedPython(#[from] managed::Error),

    #[error(transparent)]
    Download(#[from] downloads::Error),

    // TODO(zanieb) We might want to ensure this is always wrapped in another type
    #[error(transparent)]
    KeyError(#[from] installation::PythonInstallationKeyError),

    #[error(transparent)]
    MissingPython(#[from] PythonNotFound),

    #[error(transparent)]
    MissingEnvironment(#[from] environment::EnvironmentNotFound),

    #[error(transparent)]
    InvalidEnvironment(#[from] environment::InvalidEnvironment),
}

// The mock interpreters are not valid on Windows so we don't have unit test coverage there
// TODO(zanieb): We should write a mock interpreter script that works on Windows
#[cfg(all(test, unix))]
mod tests;
