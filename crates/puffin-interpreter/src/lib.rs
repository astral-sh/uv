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
    platform: PythonPlatform,
    venv: PathBuf,
    executable: PathBuf,
    markers: MarkerEnvironment,
}

impl PythonExecutable {
    /// Detect the current Python executable from the host environment.
    pub fn from_env(platform: Platform, cache: Option<&Path>) -> Result<Self> {
        let platform = PythonPlatform::from(platform);
        let venv = virtual_env::detect_virtual_env(&platform)?;
        let executable = platform.venv_python(&venv);
        let markers = markers::detect_cached_markers(&executable, cache)?;

        Ok(Self {
            platform,
            venv,
            executable,
            markers,
        })
    }

    pub fn from_venv(platform: Platform, venv: &Path, cache: Option<&Path>) -> Result<Self> {
        let platform = PythonPlatform::from(platform);
        let executable = platform.venv_python(venv);
        let markers = markers::detect_cached_markers(&executable, cache)?;

        Ok(Self {
            platform,
            venv: venv.to_path_buf(),
            executable,
            markers,
        })
    }

    /// Returns the path to the Python virtual environment.
    pub fn platform(&self) -> &Platform {
        &self.platform
    }

    /// Returns the path to the `site-packages` directory inside a virtual environment.
    pub fn site_packages(&self) -> PathBuf {
        self.platform
            .venv_site_packages(self.venv(), self.simple_version())
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
