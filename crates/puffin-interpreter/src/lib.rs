use std::path::{Path, PathBuf};

use anyhow::Result;

use pep440_rs::Version;
use pep508_rs::MarkerEnvironment;
use platform_host::Platform;

use crate::python_platform::PythonPlatform;

mod markers;
mod python_platform;
mod virtual_env;

/// A Python executable and its associated platform markers.
#[derive(Debug)]
pub struct PythonExecutable {
    venv: PathBuf,
    executable: PathBuf,
    markers: MarkerEnvironment,
}

impl PythonExecutable {
    /// Detect the current Python executable from the host environment.
    pub fn from_env(platform: &Platform) -> Result<Self> {
        let platform = PythonPlatform::from(platform);
        let venv = virtual_env::detect_virtual_env(&platform)?;
        let executable = platform.venv_python(&venv);
        let markers = markers::detect_markers(&executable)?;

        Ok(Self {
            venv,
            executable,
            markers,
        })
    }

    /// Returns the path to the Python virtual environment.
    pub fn venv(&self) -> &Path {
        self.venv.as_path()
    }

    /// Returns the path to the Python executable.
    pub fn executable(&self) -> &Path {
        self.executable.as_path()
    }

    /// Returns the [`MarkerEnvironment`] for this Python executable.
    pub fn markers(&self) -> &MarkerEnvironment {
        &self.markers
    }

    /// Returns the Python version.
    pub fn version(&self) -> &Version {
        &self.markers.python_version.version
    }

    /// Returns the Python version as a simple tuple.
    pub fn simple_version(&self) -> (u8, u8) {
        (
            u8::try_from(self.version().release[0]).expect("invalid major version"),
            u8::try_from(self.version().release[1]).expect("invalid minor version"),
        )
    }
}
