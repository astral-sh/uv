use std::env;
use std::path::{Path, PathBuf};

use tracing::debug;

use platform_host::Platform;
use uv_cache::Cache;
use uv_fs::{LockedFile, Normalized};

use crate::cfg::PyVenvConfiguration;
use crate::virtualenv_layout::VirtualenvLayout;
use crate::{find_default_python, find_requested_python, Error, Interpreter};

/// A Python environment, consisting of a Python [`Interpreter`] and a root directory.
#[derive(Debug, Clone)]
pub struct PythonEnvironment {
    root: PathBuf,
    interpreter: Interpreter,
}

impl PythonEnvironment {
    /// Create a [`PythonEnvironment`] for an existing virtual environment.
    pub fn from_virtualenv(platform: Platform, cache: &Cache) -> Result<Self, Error> {
        let layout = VirtualenvLayout::from_platform(&platform);
        let Some(venv) = detect_virtual_env(&layout)? else {
            return Err(Error::VenvNotFound);
        };
        let venv = fs_err::canonicalize(venv)?;
        let executable = layout.python_executable(&venv);
        let interpreter = Interpreter::query(&executable, platform, cache)?;

        debug_assert!(
            interpreter.base_prefix() == interpreter.base_exec_prefix(),
            "Not a virtualenv (Python: {}, prefix: {})",
            executable.display(),
            interpreter.base_prefix().display()
        );

        Ok(Self {
            root: venv,
            interpreter,
        })
    }

    /// Create a [`PythonEnvironment`] for a new virtual environment, created with the given interpreter.
    pub fn from_interpreter(interpreter: Interpreter, venv: &Path) -> Self {
        Self {
            interpreter: interpreter.with_venv_root(venv.to_path_buf()),
            root: venv.to_path_buf(),
        }
    }

    /// Create a [`PythonEnvironment`] for a Python interpreter specifier (e.g., a path or a binary name).
    pub fn from_requested_python(
        python: &str,
        platform: &Platform,
        cache: &Cache,
    ) -> Result<Self, Error> {
        let Some(interpreter) = find_requested_python(python, platform, cache)? else {
            return Err(Error::RequestedPythonNotFound(python.to_string()));
        };
        Ok(Self {
            root: interpreter.base_prefix().to_path_buf(),
            interpreter,
        })
    }

    /// Create a [`PythonEnvironment`] for the default Python interpreter.
    pub fn from_default_python(platform: &Platform, cache: &Cache) -> Result<Self, Error> {
        let interpreter = find_default_python(platform, cache)?;
        Ok(Self {
            root: interpreter.base_prefix().to_path_buf(),
            interpreter,
        })
    }

    /// Returns the location of the Python interpreter.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Return the [`Interpreter`] for this virtual environment.
    pub fn interpreter(&self) -> &Interpreter {
        &self.interpreter
    }

    /// Return the [`PyVenvConfiguration`] for this virtual environment, as extracted from the
    /// `pyvenv.cfg` file.
    pub fn cfg(&self) -> Result<PyVenvConfiguration, Error> {
        Ok(PyVenvConfiguration::parse(self.root.join("pyvenv.cfg"))?)
    }

    /// Returns the location of the Python executable.
    pub fn python_executable(&self) -> &Path {
        self.interpreter.sys_executable()
    }

    /// Returns the path to the `site-packages` directory inside a virtual environment.
    pub fn site_packages(&self) -> &Path {
        self.interpreter.purelib()
    }

    /// Returns the path to the `bin` directory inside a virtual environment.
    pub fn scripts(&self) -> &Path {
        self.interpreter.scripts()
    }

    /// Lock the virtual environment to prevent concurrent writes.
    pub fn lock(&self) -> Result<LockedFile, std::io::Error> {
        if self.interpreter.is_virtualenv() {
            // If the environment a virtualenv, use a virtualenv-specific lock file.
            LockedFile::acquire(self.root.join(".lock"), self.root.normalized_display())
        } else {
            // Otherwise, use a global lock file.
            LockedFile::acquire(
                env::temp_dir().join(format!("uv-{}.lock", cache_key::digest(&self.root))),
                self.root.normalized_display(),
            )
        }
    }
}

/// Locate the current virtual environment.
pub(crate) fn detect_virtual_env(layout: &VirtualenvLayout) -> Result<Option<PathBuf>, Error> {
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
            let python = layout.python_executable(&dot_venv);
            if !python.is_file() {
                return Err(Error::BrokenVenv(dot_venv, python));
            }
            debug!("Found a virtualenv named .venv at: {}", dot_venv.display());
            return Ok(Some(dot_venv));
        }
    }

    Ok(None)
}
