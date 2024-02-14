use std::env;
use std::env::consts::EXE_SUFFIX;
use std::path::{Path, PathBuf};

use tracing::debug;

use platform_host::Platform;
use uv_cache::Cache;
use uv_fs::{LockedFile, Normalized};

use crate::cfg::Configuration;
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
    pub fn from_env(platform: Platform, cache: &Cache) -> Result<Self, Error> {
        let platform = PythonPlatform::from(platform);
        let Some(venv) = detect_virtual_env(&platform)? else {
            return Err(Error::NotFound);
        };
        let venv = fs_err::canonicalize(venv)?;
        let executable = platform.venv_python(&venv);
        let interpreter = Interpreter::query(&executable, &platform.0, cache)?;

        Ok(Self {
            root: venv,
            interpreter,
        })
    }

    /// Creating a new venv from a Python interpreter changes this.
    pub fn from_interpreter(interpreter: Interpreter, venv: &Path) -> Self {
        Self {
            interpreter: interpreter.with_base_prefix(venv.to_path_buf()),
            root: venv.to_path_buf(),
        }
    }

    /// Returns the location of the python interpreter
    pub fn python_executable(&self) -> PathBuf {
        self.bin_dir().join(format!("python{EXE_SUFFIX}"))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Return the [`Interpreter`] for this virtual environment.
    pub fn interpreter(&self) -> &Interpreter {
        &self.interpreter
    }

    /// Return the [`Configuration`] for this virtual environment, as extracted from the
    /// `pyvenv.cfg` file.
    pub fn cfg(&self) -> Result<Configuration, Error> {
        Ok(Configuration::parse(self.root.join("pyvenv.cfg"))?)
    }

    /// Returns the path to the `site-packages` directory inside a virtual environment.
    pub fn site_packages(&self) -> PathBuf {
        self.interpreter
            .platform
            .venv_site_packages(&self.root, self.interpreter().python_tuple())
    }

    pub fn bin_dir(&self) -> PathBuf {
        if cfg!(unix) {
            self.root().join("bin")
        } else if cfg!(windows) {
            self.root().join("Scripts")
        } else {
            unimplemented!("Only Windows and Unix are supported")
        }
    }

    /// Lock the virtual environment to prevent concurrent writes.
    pub fn lock(&self) -> Result<LockedFile, std::io::Error> {
        LockedFile::acquire(self.root.join(".lock"), self.root.normalized_display())
    }
}

/// Locate the current virtual environment.
pub(crate) fn detect_virtual_env(target: &PythonPlatform) -> Result<Option<PathBuf>, Error> {
    match (
        env::var_os("VIRTUAL_ENV").filter(|value| !value.is_empty()),
        env::var_os("CONDA_PREFIX").filter(|value| !value.is_empty()),
    ) {
        (Some(dir), None) => {
            debug!(
                "Found a virtualenv through VIRTUAL_ENV at: {}",
                Path::new(&dir).display()
            );
            return Ok(Some(PathBuf::from(dir)));
        }
        (None, Some(dir)) => {
            debug!(
                "Found a virtualenv through CONDA_PREFIX at: {}",
                Path::new(&dir).display()
            );
            return Ok(Some(PathBuf::from(dir)));
        }
        (Some(venv), Some(conda)) if venv == conda => return Ok(Some(PathBuf::from(venv))),
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
            debug!("Found a virtualenv named .venv at: {}", dot_venv.display());
            return Ok(Some(dot_venv));
        }
    }

    Ok(None)
}
