use itertools::Either;
use std::env;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use same_file::is_same_file;

use uv_cache::Cache;
use uv_fs::{LockedFile, Simplified};

use crate::virtualenv::{detect_virtualenv, virtualenv_python_executable, PyVenvConfiguration};
use crate::{find_default_python, find_requested_python, Error, Interpreter, Target};

/// A Python environment, consisting of a Python [`Interpreter`] and its associated paths.
#[derive(Debug, Clone)]
pub struct PythonEnvironment(Arc<PythonEnvironmentShared>);

#[derive(Debug, Clone)]
struct PythonEnvironmentShared {
    root: PathBuf,
    interpreter: Interpreter,
}

impl PythonEnvironment {
    /// Create a [`PythonEnvironment`] for an existing virtual environment, detected from the
    /// environment variables and filesystem.
    pub fn from_virtualenv(cache: &Cache) -> Result<Self, Error> {
        let Some(venv) = detect_virtualenv()? else {
            return Err(Error::VenvNotFound);
        };
        Self::from_root(&venv, cache)
    }

    /// Create a [`PythonEnvironment`] from the virtual environment at the given root.
    pub fn from_root(root: &Path, cache: &Cache) -> Result<Self, Error> {
        let venv = match fs_err::canonicalize(root) {
            Ok(venv) => venv,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Err(Error::VenvDoesNotExist(root.to_path_buf()));
            }
            Err(err) => return Err(err.into()),
        };
        let executable = virtualenv_python_executable(&venv);
        let interpreter = Interpreter::query(&executable, cache)?;

        debug_assert!(
            interpreter.base_prefix() == interpreter.base_exec_prefix(),
            "Not a virtualenv (Python: {}, prefix: {})",
            executable.display(),
            interpreter.base_prefix().display()
        );

        Ok(Self(Arc::new(PythonEnvironmentShared {
            root: venv,
            interpreter,
        })))
    }

    /// Create a [`PythonEnvironment`] for a Python interpreter specifier (e.g., a path or a binary name).
    pub fn from_requested_python(python: &str, cache: &Cache) -> Result<Self, Error> {
        let Some(interpreter) = find_requested_python(python, cache)? else {
            return Err(Error::RequestedPythonNotFound(python.to_string()));
        };
        Ok(Self(Arc::new(PythonEnvironmentShared {
            root: interpreter.prefix().to_path_buf(),
            interpreter,
        })))
    }

    /// Create a [`PythonEnvironment`] for the default Python interpreter.
    pub fn from_default_python(cache: &Cache) -> Result<Self, Error> {
        let interpreter = find_default_python(cache)?;
        Ok(Self(Arc::new(PythonEnvironmentShared {
            root: interpreter.prefix().to_path_buf(),
            interpreter,
        })))
    }

    /// Create a [`PythonEnvironment`] from an existing [`Interpreter`] and root directory.
    pub fn from_interpreter(interpreter: Interpreter) -> Self {
        Self(Arc::new(PythonEnvironmentShared {
            root: interpreter.prefix().to_path_buf(),
            interpreter,
        }))
    }

    /// Create a [`PythonEnvironment`] from an existing [`Interpreter`] and `--target` directory.
    #[must_use]
    pub fn with_target(self, target: Target) -> Self {
        let inner = Arc::unwrap_or_clone(self.0);
        Self(Arc::new(PythonEnvironmentShared {
            interpreter: inner.interpreter.with_target(target),
            ..inner
        }))
    }

    /// Returns the root (i.e., `prefix`) of the Python interpreter.
    pub fn root(&self) -> &Path {
        &self.0.root
    }

    /// Return the [`Interpreter`] for this virtual environment.
    pub fn interpreter(&self) -> &Interpreter {
        &self.0.interpreter
    }

    /// Return the [`PyVenvConfiguration`] for this virtual environment, as extracted from the
    /// `pyvenv.cfg` file.
    pub fn cfg(&self) -> Result<PyVenvConfiguration, Error> {
        Ok(PyVenvConfiguration::parse(self.0.root.join("pyvenv.cfg"))?)
    }

    /// Returns the location of the Python executable.
    pub fn python_executable(&self) -> &Path {
        self.0.interpreter.sys_executable()
    }

    /// Returns an iterator over the `site-packages` directories inside a virtual environment.
    ///
    /// In most cases, `purelib` and `platlib` will be the same, and so the iterator will contain
    /// a single element; however, in some distributions, they may be different.
    ///
    /// Some distributions also create symbolic links from `purelib` to `platlib`; in such cases, we
    /// still deduplicate the entries, returning a single path.
    pub fn site_packages(&self) -> impl Iterator<Item = &Path> {
        if let Some(target) = self.0.interpreter.target() {
            Either::Left(std::iter::once(target.root()))
        } else {
            let purelib = self.0.interpreter.purelib();
            let platlib = self.0.interpreter.platlib();
            Either::Right(std::iter::once(purelib).chain(
                if purelib == platlib || is_same_file(purelib, platlib).unwrap_or(false) {
                    None
                } else {
                    Some(platlib)
                },
            ))
        }
    }

    /// Returns the path to the `bin` directory inside a virtual environment.
    pub fn scripts(&self) -> &Path {
        self.0.interpreter.scripts()
    }

    /// Grab a file lock for the virtual environment to prevent concurrent writes across processes.
    pub fn lock(&self) -> Result<LockedFile, std::io::Error> {
        if let Some(target) = self.0.interpreter.target() {
            // If we're installing into a `--target`, use a target-specific lock file.
            LockedFile::acquire(
                target.root().join(".lock"),
                target.root().simplified_display(),
            )
        } else if self.0.interpreter.is_virtualenv() {
            // If the environment a virtualenv, use a virtualenv-specific lock file.
            LockedFile::acquire(self.0.root.join(".lock"), self.0.root.simplified_display())
        } else {
            // Otherwise, use a global lock file.
            LockedFile::acquire(
                env::temp_dir().join(format!("uv-{}.lock", cache_key::digest(&self.0.root))),
                self.0.root.simplified_display(),
            )
        }
    }

    /// Return the [`Interpreter`] for this virtual environment.
    pub fn into_interpreter(self) -> Interpreter {
        Arc::unwrap_or_clone(self.0).interpreter
    }
}
