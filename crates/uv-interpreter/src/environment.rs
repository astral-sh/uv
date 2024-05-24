use itertools::Either;
use std::env;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use same_file::is_same_file;

use uv_cache::Cache;
use uv_fs::{LockedFile, Simplified};

use crate::discovery::{InterpreterRequest, SourceSelector, SystemPython};
use crate::virtualenv::{virtualenv_python_executable, PyVenvConfiguration};
use crate::{
    find_default_interpreter, find_interpreter, Error, Interpreter, InterpreterSource, Target,
};

/// A Python environment, consisting of a Python [`Interpreter`] and its associated paths.
#[derive(Debug, Clone)]
pub struct PythonEnvironment(Arc<PythonEnvironmentShared>);

#[derive(Debug, Clone)]
struct PythonEnvironmentShared {
    root: PathBuf,
    interpreter: Interpreter,
}

impl PythonEnvironment {
    /// Create a [`PythonEnvironment`] from a user request.
    pub fn find(python: Option<&str>, system: SystemPython, cache: &Cache) -> Result<Self, Error> {
        // Detect the current Python interpreter.
        if let Some(python) = python {
            Self::from_requested_python(python, system, cache)
        } else if system.is_preferred() {
            Self::from_default_python(cache)
        } else {
            // First check for a parent intepreter
            // We gate this check to avoid an extra log message when it is not set
            if std::env::var_os("UV_INTERNAL__PARENT_INTERPRETER").is_some() {
                match Self::from_parent_interpreter(system, cache) {
                    Ok(env) => return Ok(env),
                    Err(Error::NotFound(_)) => {}
                    Err(err) => return Err(err),
                }
            }

            // Then a virtual environment
            match Self::from_virtualenv(cache) {
                Ok(venv) => Ok(venv),
                Err(Error::NotFound(_)) if system.is_allowed() => Self::from_default_python(cache),
                Err(err) => Err(err),
            }
        }
    }

    /// Create a [`PythonEnvironment`] for an existing virtual environment.
    ///
    /// Allows Conda environments (via `CONDA_PREFIX`) though they are not technically virtual environments.
    pub fn from_virtualenv(cache: &Cache) -> Result<Self, Error> {
        let sources = SourceSelector::VirtualEnv;
        let request = InterpreterRequest::Any;
        let found = find_interpreter(&request, SystemPython::Disallowed, &sources, cache)??;

        debug_assert!(
            found.interpreter().is_virtualenv()
                || matches!(found.source(), InterpreterSource::CondaPrefix),
            "Not a virtualenv (source: {}, prefix: {})",
            found.source(),
            found.interpreter().base_prefix().display()
        );

        Ok(Self(Arc::new(PythonEnvironmentShared {
            root: found.interpreter().prefix().to_path_buf(),
            interpreter: found.into_interpreter(),
        })))
    }

    /// Create a [`PythonEnvironment`] for the parent interpreter i.e. the executable in `python -m uv ...`
    pub fn from_parent_interpreter(system: SystemPython, cache: &Cache) -> Result<Self, Error> {
        let sources = SourceSelector::from_sources([InterpreterSource::ParentInterpreter]);
        let request = InterpreterRequest::Any;
        let found = find_interpreter(&request, system, &sources, cache)??;

        Ok(Self(Arc::new(PythonEnvironmentShared {
            root: found.interpreter().prefix().to_path_buf(),
            interpreter: found.into_interpreter(),
        })))
    }

    /// Create a [`PythonEnvironment`] from the virtual environment at the given root.
    pub fn from_root(root: &Path, cache: &Cache) -> Result<Self, Error> {
        let venv = match fs_err::canonicalize(root) {
            Ok(venv) => venv,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Err(Error::NotFound(
                    crate::InterpreterNotFound::DirectoryNotFound(root.to_path_buf()),
                ));
            }
            Err(err) => return Err(Error::Discovery(err.into())),
        };
        let executable = virtualenv_python_executable(venv);
        let interpreter = Interpreter::query(executable, cache)?;

        Ok(Self(Arc::new(PythonEnvironmentShared {
            root: interpreter.prefix().to_path_buf(),
            interpreter,
        })))
    }

    /// Create a [`PythonEnvironment`] for a Python interpreter specifier (e.g., a path or a binary name).
    pub fn from_requested_python(
        request: &str,
        system: SystemPython,
        cache: &Cache,
    ) -> Result<Self, Error> {
        let sources = SourceSelector::from_settings(system);
        let request = InterpreterRequest::parse(request);
        let interpreter = find_interpreter(&request, system, &sources, cache)??.into_interpreter();
        Ok(Self(Arc::new(PythonEnvironmentShared {
            root: interpreter.prefix().to_path_buf(),
            interpreter,
        })))
    }

    /// Create a [`PythonEnvironment`] for the default Python interpreter.
    pub fn from_default_python(cache: &Cache) -> Result<Self, Error> {
        let interpreter = find_default_interpreter(cache)??.into_interpreter();
        Ok(Self(Arc::new(PythonEnvironmentShared {
            root: interpreter.prefix().to_path_buf(),
            interpreter,
        })))
    }

    /// Create a [`PythonEnvironment`] from an existing [`Interpreter`].
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
    ///
    /// See also [`PythonEnvironment::into_interpreter`].
    pub fn interpreter(&self) -> &Interpreter {
        &self.0.interpreter
    }

    /// Return the [`PyVenvConfiguration`] for this environment, as extracted from the
    /// `pyvenv.cfg` file.
    pub fn cfg(&self) -> Result<PyVenvConfiguration, Error> {
        Ok(PyVenvConfiguration::parse(self.0.root.join("pyvenv.cfg"))?)
    }

    /// Returns the location of the Python executable.
    pub fn python_executable(&self) -> &Path {
        self.0.interpreter.sys_executable()
    }

    /// Returns an iterator over the `site-packages` directories inside the environment.
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

    /// Returns the path to the `bin` directory inside this environment.
    pub fn scripts(&self) -> &Path {
        self.0.interpreter.scripts()
    }

    /// Grab a file lock for the environment to prevent concurrent writes across processes.
    pub fn lock(&self) -> Result<LockedFile, std::io::Error> {
        if let Some(target) = self.0.interpreter.target() {
            // If we're installing into a `--target`, use a target-specific lock file.
            LockedFile::acquire(target.root().join(".lock"), target.root().user_display())
        } else if self.0.interpreter.is_virtualenv() {
            // If the environment a virtualenv, use a virtualenv-specific lock file.
            LockedFile::acquire(self.0.root.join(".lock"), self.0.root.user_display())
        } else {
            // Otherwise, use a global lock file.
            LockedFile::acquire(
                env::temp_dir().join(format!("uv-{}.lock", cache_key::digest(&self.0.root))),
                self.0.root.user_display(),
            )
        }
    }

    /// Return the [`Interpreter`] for this environment.
    ///
    /// See also [`PythonEnvironment::interpreter`].
    pub fn into_interpreter(self) -> Interpreter {
        Arc::unwrap_or_clone(self.0).interpreter
    }
}
