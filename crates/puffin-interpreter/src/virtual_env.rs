use std::env;
use std::path::{Path, PathBuf};

use tracing::debug;

use platform_host::Platform;

use crate::python_platform::PythonPlatform;
use crate::{Error, Interpreter};

/// A Python executable and its associated platform markers.
#[derive(Debug, Clone)]
pub struct Virtualenv {
    root: PathBuf,
    interpreter: Interpreter,
}

impl Virtualenv {
    /// Venv the current Python executable from the host environment.
    pub fn from_env(platform: Platform, cache: Option<&Path>) -> Result<Self, Error> {
        let platform = PythonPlatform::from(platform);
        let Some(venv) = detect_virtual_env(&platform)? else {
            return Err(Error::NotFound);
        };
        let executable = platform.venv_python(&venv);
        let interpreter = Interpreter::query(&executable, platform.0, cache)?;

        Ok(Self {
            root: venv,
            interpreter,
        })
    }

    pub fn from_virtualenv(
        platform: Platform,
        root: &Path,
        cache: Option<&Path>,
    ) -> Result<Self, Error> {
        let platform = PythonPlatform::from(platform);
        let executable = platform.venv_python(root);
        let interpreter = Interpreter::query(&executable, platform.0, cache)?;

        Ok(Self {
            root: root.to_path_buf(),
            interpreter,
        })
    }

    /// Creating a new venv from a python interpreter changes this
    pub fn new_prefix(venv: &Path, interpreter: &Interpreter) -> Self {
        Self {
            root: venv.to_path_buf(),
            interpreter: Interpreter {
                base_prefix: venv.to_path_buf(),
                ..interpreter.clone()
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

    pub fn interpreter(&self) -> &Interpreter {
        &self.interpreter
    }

    /// Returns the path to the `site-packages` directory inside a virtual environment.
    pub fn site_packages(&self) -> PathBuf {
        self.interpreter
            .platform
            .venv_site_packages(&self.root, self.interpreter().simple_version())
    }
}

/// Locate the current virtual environment.
pub(crate) fn detect_virtual_env(target: &PythonPlatform) -> Result<Option<PathBuf>, Error> {
    match (env::var_os("VIRTUAL_ENV"), env::var_os("CONDA_PREFIX")) {
        (Some(dir), None) => {
            debug!(
                "Found a virtualenv through VIRTUAL_ENV at {}",
                Path::new(&dir).display()
            );
            return Ok(Some(PathBuf::from(dir)));
        }
        (None, Some(dir)) => {
            debug!(
                "Found a virtualenv through CONDA_PREFIX at {}",
                Path::new(&dir).display()
            );
            return Ok(Some(PathBuf::from(dir)));
        }
        (Some(_), Some(_)) => {
            return Err(Error::Conflict);
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
                return Err(Error::MissingPyVenvCfg(dot_venv));
            }
            let python = target.venv_python(&dot_venv);
            if !python.is_file() {
                return Err(Error::BrokenVenv(dot_venv, python));
            }
            debug!("Found a virtualenv named .venv at {}", dot_venv.display());
            return Ok(Some(dot_venv));
        }
    }

    Ok(None)
}
