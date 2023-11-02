use std::env;
use std::path::{Path, PathBuf};

use crate::InterpreterInfo;
use anyhow::{bail, Result};
use platform_host::Platform;
use tracing::debug;

use crate::python_platform::PythonPlatform;

/// A Python executable and its associated platform markers.
#[derive(Debug, Clone)]
pub struct Virtualenv {
    root: PathBuf,
    interpreter_info: InterpreterInfo,
}

impl Virtualenv {
    /// Venv the current Python executable from the host environment.
    pub fn from_env(platform: Platform, cache: Option<&Path>) -> Result<Self> {
        let platform = PythonPlatform::from(platform);
        let venv = detect_virtual_env(&platform)?;
        let executable = platform.venv_python(&venv);
        let interpreter_info = InterpreterInfo::query(&executable, platform.0, cache)?;

        Ok(Self {
            root: venv,
            interpreter_info,
        })
    }

    pub fn from_virtualenv(platform: Platform, root: &Path, cache: Option<&Path>) -> Result<Self> {
        let platform = PythonPlatform::from(platform);
        let executable = platform.venv_python(root);
        let interpreter_info = InterpreterInfo::query(&executable, platform.0, cache)?;

        Ok(Self {
            root: root.to_path_buf(),
            interpreter_info,
        })
    }

    /// Creating a new venv from a python interpreter changes this
    pub fn new_prefix(venv: &Path, interpreter_info: &InterpreterInfo) -> Self {
        Self {
            root: venv.to_path_buf(),
            interpreter_info: InterpreterInfo {
                base_prefix: venv.to_path_buf(),
                ..interpreter_info.clone()
            },
        }
    }

    /// Returns the location of the python interpreter
    pub fn python_executable(&self) -> PathBuf {
        #[cfg(unix)]
        {
            self.root.join("bin").join("python")
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

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn interpreter_info(&self) -> &InterpreterInfo {
        &self.interpreter_info
    }

    /// Returns the path to the `site-packages` directory inside a virtual environment.
    pub fn site_packages(&self) -> PathBuf {
        self.interpreter_info
            .platform
            .venv_site_packages(&self.root, self.interpreter_info().simple_version())
    }
}

/// Locate the current virtual environment.
pub(crate) fn detect_virtual_env(target: &PythonPlatform) -> Result<PathBuf> {
    match (env::var_os("VIRTUAL_ENV"), env::var_os("CONDA_PREFIX")) {
        (Some(dir), None) => return Ok(PathBuf::from(dir)),
        (None, Some(dir)) => return Ok(PathBuf::from(dir)),
        (Some(_), Some(_)) => {
            bail!("Both VIRTUAL_ENV and CONDA_PREFIX are set. Please unset one of them.")
        }
        (None, None) => {
            // No environment variables set. Try to find a virtualenv in the current directory.
        }
    };

    // Search for a `.venv` directory in the current or any parent directory.
    let current_dir = env::current_dir().expect("Failed to detect current directory");
    for dir in current_dir.ancestors() {
        let dot_venv = dir.join(".venv");
        if dot_venv.is_dir() {
            if !dot_venv.join("pyvenv.cfg").is_file() {
                bail!(
                    "Expected {} to be a virtual environment, but pyvenv.cfg is missing",
                    dot_venv.display()
                );
            }
            let python = target.venv_python(&dot_venv);
            if !python.is_file() {
                bail!(
                    "Your virtualenv at {} is broken. It contains a pyvenv.cfg but no python at {}",
                    dot_venv.display(),
                    python.display()
                );
            }
            debug!("Found a virtualenv named .venv at {}", dot_venv.display());
            return Ok(dot_venv);
        }
    }

    bail!("Couldn't find a virtualenv or conda environment.")
}
