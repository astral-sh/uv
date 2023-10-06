use std::path::{Path, PathBuf};

use anyhow::Result;
use pep508_rs::MarkerEnvironment;
use puffin_platform::Platform;

use crate::python_platform::PythonPlatform;

mod markers;
mod python_platform;
mod virtual_env;

/// A Python executable and its associated platform markers.
#[derive(Debug)]
pub struct PythonExecutable {
    executable: PathBuf,
    markers: MarkerEnvironment,
}

impl PythonExecutable {
    /// Detect the current Python executable from the host environment.
    pub fn from_env(platform: &Platform) -> Result<Self> {
        let platform = PythonPlatform::from(platform);
        let venv = virtual_env::detect_virtual_env(&platform)?;
        let executable = platform.venv_python(venv);
        let markers = markers::detect_markers(&executable)?;

        Ok(Self {
            executable,
            markers,
        })
    }

    /// Returns the path to the Python executable.
    pub fn executable(&self) -> &Path {
        self.executable.as_path()
    }

    /// Returns the [`MarkerEnvironment`] for this Python executable.
    pub fn markers(&self) -> &MarkerEnvironment {
        &self.markers
    }

    /// Returns the Python version as a tuple of (major, minor).
    pub fn version(&self) -> (u8, u8) {
        // TODO(charlie): Use `Version`.
        let python_version = &self.markers.python_version;
        (
            u8::try_from(python_version.release[0]).expect("Python major version is too large"),
            u8::try_from(python_version.release[1]).expect("Python minor version is too large"),
        )
    }
}
