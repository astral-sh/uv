use std::borrow::Cow;
use std::env;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use uv_cache::Cache;
use uv_fs::{LockedFile, Simplified};

use crate::discovery::find_python_installation;
use crate::installation::PythonInstallation;
use crate::virtualenv::{virtualenv_python_executable, PyVenvConfiguration};
use crate::{
    EnvironmentPreference, Error, Interpreter, Prefix, PythonNotFound, PythonPreference,
    PythonRequest, Target,
};

/// A Python environment, consisting of a Python [`Interpreter`] and its associated paths.
#[derive(Debug, Clone)]
pub struct PythonEnvironment(Arc<PythonEnvironmentShared>);

#[derive(Debug, Clone)]
struct PythonEnvironmentShared {
    root: PathBuf,
    interpreter: Interpreter,
}

/// The result of failed environment discovery.
///
/// Generally this is cast from [`PythonNotFound`] by [`PythonEnvironment::find`].
#[derive(Clone, Debug, Error)]
pub struct EnvironmentNotFound {
    request: PythonRequest,
    preference: EnvironmentPreference,
}

impl From<PythonNotFound> for EnvironmentNotFound {
    fn from(value: PythonNotFound) -> Self {
        Self {
            request: value.request,
            preference: value.environment_preference,
        }
    }
}

impl fmt::Display for EnvironmentNotFound {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let environment = match self.preference {
            EnvironmentPreference::Any => "virtual or system environment",
            EnvironmentPreference::ExplicitSystem => {
                if self.request.is_explicit_system() {
                    "virtual or system environment"
                } else {
                    // TODO(zanieb): We could add a hint to use the `--system` flag here
                    "virtual environment"
                }
            }
            EnvironmentPreference::OnlySystem => "system environment",
            EnvironmentPreference::OnlyVirtual => "virtual environment",
        };
        match self.request {
            PythonRequest::Any => {
                write!(f, "No {environment} found")
            }
            _ => {
                write!(f, "No {environment} found for {}", self.request)
            }
        }
    }
}

impl PythonEnvironment {
    /// Find a [`PythonEnvironment`] matching the given request and preference.
    ///
    /// If looking for a Python interpreter to create a new environment, use [`PythonInstallation::find`]
    /// instead.
    pub fn find(
        request: &PythonRequest,
        preference: EnvironmentPreference,
        cache: &Cache,
    ) -> Result<Self, Error> {
        let installation = match find_python_installation(
            request,
            preference,
            // Ignore managed installations when looking for environments
            PythonPreference::OnlySystem,
            cache,
        )? {
            Ok(installation) => installation,
            Err(err) => return Err(EnvironmentNotFound::from(err).into()),
        };
        Ok(Self::from_installation(installation))
    }

    /// Create a [`PythonEnvironment`] from the virtual environment at the given root.
    pub fn from_root(root: impl AsRef<Path>, cache: &Cache) -> Result<Self, Error> {
        let venv = match fs_err::canonicalize(root.as_ref()) {
            Ok(venv) => venv,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Err(Error::MissingEnvironment(EnvironmentNotFound {
                    preference: EnvironmentPreference::Any,
                    request: PythonRequest::Directory(root.as_ref().to_owned()),
                }));
            }
            Err(err) => return Err(Error::Discovery(err.into())),
        };
        let executable = virtualenv_python_executable(venv);
        let interpreter = Interpreter::query(executable, cache)?;

        Ok(Self(Arc::new(PythonEnvironmentShared {
            root: interpreter.sys_prefix().to_path_buf(),
            interpreter,
        })))
    }

    /// Create a [`PythonEnvironment`] from an existing [`PythonInstallation`].
    pub fn from_installation(installation: PythonInstallation) -> Self {
        Self::from_interpreter(installation.into_interpreter())
    }

    /// Create a [`PythonEnvironment`] from an existing [`Interpreter`].
    pub fn from_interpreter(interpreter: Interpreter) -> Self {
        Self(Arc::new(PythonEnvironmentShared {
            root: interpreter.sys_prefix().to_path_buf(),
            interpreter,
        }))
    }

    /// Create a [`PythonEnvironment`] from an existing [`Interpreter`] and `--target` directory.
    pub fn with_target(self, target: Target) -> std::io::Result<Self> {
        let inner = Arc::unwrap_or_clone(self.0);
        Ok(Self(Arc::new(PythonEnvironmentShared {
            interpreter: inner.interpreter.with_target(target)?,
            ..inner
        })))
    }

    /// Create a [`PythonEnvironment`] from an existing [`Interpreter`] and `--prefix` directory.
    pub fn with_prefix(self, prefix: Prefix) -> std::io::Result<Self> {
        let inner = Arc::unwrap_or_clone(self.0);
        Ok(Self(Arc::new(PythonEnvironmentShared {
            interpreter: inner.interpreter.with_prefix(prefix)?,
            ..inner
        })))
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

    /// Returns `true` if the environment is "relocatable".
    pub fn relocatable(&self) -> bool {
        self.cfg().is_ok_and(|cfg| cfg.is_relocatable())
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
    pub fn site_packages(&self) -> impl Iterator<Item = Cow<Path>> {
        self.0.interpreter.site_packages()
    }

    /// Returns the path to the `bin` directory inside this environment.
    pub fn scripts(&self) -> &Path {
        self.0.interpreter.scripts()
    }

    /// Grab a file lock for the environment to prevent concurrent writes across processes.
    pub fn lock(&self) -> Result<LockedFile, std::io::Error> {
        if let Some(target) = self.0.interpreter.target() {
            // If we're installing into a `--target`, use a target-specific lockfile.
            LockedFile::acquire(target.root().join(".lock"), target.root().user_display())
        } else if let Some(prefix) = self.0.interpreter.prefix() {
            // Likewise, if we're installing into a `--prefix`, use a prefix-specific lockfile.
            LockedFile::acquire(prefix.root().join(".lock"), prefix.root().user_display())
        } else if self.0.interpreter.is_virtualenv() {
            // If the environment a virtualenv, use a virtualenv-specific lockfile.
            LockedFile::acquire(self.0.root.join(".lock"), self.0.root.user_display())
        } else {
            // Otherwise, use a global lockfile.
            LockedFile::acquire(
                env::temp_dir().join(format!("uv-{}.lock", cache_key::cache_digest(&self.0.root))),
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
