use std::path::{Path, PathBuf};

use anyhow::Result;
use pep508_rs::MarkerEnvironment;

use crate::platform::Platform;

mod markers;
mod platform;
mod virtual_env;

/// A Python executable and its associated platform markers.
#[derive(Debug)]
pub struct PythonExecutable {
    executable: PathBuf,
    markers: MarkerEnvironment,
}

impl PythonExecutable {
    /// Detect the current Python executable from the host environment.
    pub fn from_env() -> Result<Self> {
        let target = Platform::from_host();
        let venv = virtual_env::detect_virtual_env(&target)?;
        let executable = target.get_venv_python(venv);
        let markers = markers::detect_markers(&executable)?;

        Ok(Self {
            executable,
            markers,
        })
    }

    pub fn executable(&self) -> &Path {
        self.executable.as_path()
    }

    pub fn markers(&self) -> &MarkerEnvironment {
        &self.markers
    }
}
