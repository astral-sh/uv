use std::borrow::Cow;
use std::env;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use owo_colors::OwoColorize;
use tracing::debug;

use uv_cache::Cache;
use uv_cache_key::cache_digest;
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

#[derive(Clone, Debug, Error)]
pub struct InvalidEnvironment {
    path: PathBuf,
    pub kind: InvalidEnvironmentKind,
}
#[derive(Debug, Clone)]
pub enum InvalidEnvironmentKind {
    NotDirectory,
    Empty,
    MissingExecutable(PathBuf),
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
        #[derive(Debug, Copy, Clone)]
        enum SearchType {
            /// Only virtual environments were searched.
            Virtual,
            /// Only system installations were searched.
            System,
            /// Both virtual and system installations were searched.
            VirtualOrSystem,
        }

        impl fmt::Display for SearchType {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                match self {
                    Self::Virtual => write!(f, "virtual environment"),
                    Self::System => write!(f, "system Python installation"),
                    Self::VirtualOrSystem => {
                        write!(f, "virtual environment or system Python installation")
                    }
                }
            }
        }

        let search_type = match self.preference {
            EnvironmentPreference::Any => SearchType::VirtualOrSystem,
            EnvironmentPreference::ExplicitSystem => {
                if self.request.is_explicit_system() {
                    SearchType::VirtualOrSystem
                } else {
                    SearchType::Virtual
                }
            }
            EnvironmentPreference::OnlySystem => SearchType::System,
            EnvironmentPreference::OnlyVirtual => SearchType::Virtual,
        };

        if matches!(self.request, PythonRequest::Default | PythonRequest::Any) {
            write!(f, "No {search_type} found")?;
        } else {
            write!(f, "No {search_type} found for {}", self.request)?;
        }

        match search_type {
            // This error message assumes that the relevant API accepts the `--system` flag. This
            // is true of the callsites today, since the project APIs never surface this error.
            SearchType::Virtual => write!(f, "; run `{}` to create an environment, or pass `{}` to install into a non-virtual environment", "uv venv".green(), "--system".green())?,
            SearchType::VirtualOrSystem => write!(f, "; run `{}` to create an environment", "uv venv".green())?,
            SearchType::System => {}
        }

        Ok(())
    }
}

impl fmt::Display for InvalidEnvironment {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Invalid environment at `{}`: {}",
            self.path.user_display(),
            self.kind
        )
    }
}

impl fmt::Display for InvalidEnvironmentKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::NotDirectory => write!(f, "expected directory but found a file"),
            Self::MissingExecutable(path) => {
                write!(f, "missing Python executable at `{}`", path.user_display())
            }
            Self::Empty => write!(f, "directory is empty"),
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
    ///
    /// N.B. This function also works for system Python environments and users depend on this.
    pub fn from_root(root: impl AsRef<Path>, cache: &Cache) -> Result<Self, Error> {
        debug!(
            "Checking for Python environment at `{}`",
            root.as_ref().user_display()
        );
        match root.as_ref().try_exists() {
            Ok(true) => {}
            Ok(false) => {
                return Err(Error::MissingEnvironment(EnvironmentNotFound {
                    preference: EnvironmentPreference::Any,
                    request: PythonRequest::Directory(root.as_ref().to_owned()),
                }));
            }
            Err(err) => return Err(Error::Discovery(err.into())),
        };

        if root.as_ref().is_file() {
            return Err(InvalidEnvironment {
                path: root.as_ref().to_path_buf(),
                kind: InvalidEnvironmentKind::NotDirectory,
            }
            .into());
        }

        if root
            .as_ref()
            .read_dir()
            .is_ok_and(|mut dir| dir.next().is_none())
        {
            return Err(InvalidEnvironment {
                path: root.as_ref().to_path_buf(),
                kind: InvalidEnvironmentKind::Empty,
            }
            .into());
        }

        // Note we do not canonicalize the root path or the executable path, this is important
        // because the path the interpreter is invoked at can determine the value of
        // `sys.executable`.
        let executable = virtualenv_python_executable(&root);

        // If we can't find an executable, exit before querying to provide a better error.
        if !(executable.is_symlink() || executable.is_file()) {
            return Err(InvalidEnvironment {
                path: root.as_ref().to_path_buf(),
                kind: InvalidEnvironmentKind::MissingExecutable(executable.clone()),
            }
            .into());
        };

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

    /// Set a key-value pair in the `pyvenv.cfg` file.
    pub fn set_pyvenv_cfg(&self, key: &str, value: &str) -> Result<(), Error> {
        let content = fs_err::read_to_string(self.0.root.join("pyvenv.cfg"))?;
        fs_err::write(
            self.0.root.join("pyvenv.cfg"),
            PyVenvConfiguration::set(&content, key, value),
        )?;
        Ok(())
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
    pub async fn lock(&self) -> Result<LockedFile, std::io::Error> {
        if let Some(target) = self.0.interpreter.target() {
            // If we're installing into a `--target`, use a target-specific lockfile.
            LockedFile::acquire(target.root().join(".lock"), target.root().user_display()).await
        } else if let Some(prefix) = self.0.interpreter.prefix() {
            // Likewise, if we're installing into a `--prefix`, use a prefix-specific lockfile.
            LockedFile::acquire(prefix.root().join(".lock"), prefix.root().user_display()).await
        } else if self.0.interpreter.is_virtualenv() {
            // If the environment a virtualenv, use a virtualenv-specific lockfile.
            LockedFile::acquire(self.0.root.join(".lock"), self.0.root.user_display()).await
        } else {
            // Otherwise, use a global lockfile.
            LockedFile::acquire(
                env::temp_dir().join(format!("uv-{}.lock", cache_digest(&self.0.root))),
                self.0.root.user_display(),
            )
            .await
        }
    }

    /// Return the [`Interpreter`] for this environment.
    ///
    /// See also [`PythonEnvironment::interpreter`].
    pub fn into_interpreter(self) -> Interpreter {
        Arc::unwrap_or_clone(self.0).interpreter
    }

    /// Returns `true` if the [`PythonEnvironment`] uses the same underlying [`Interpreter`].
    pub fn uses(&self, interpreter: &Interpreter) -> bool {
        // TODO(zanieb): Consider using `sysconfig.get_path("stdlib")` instead, which
        // should be generally robust.
        if cfg!(windows) {
            // On Windows, we can't canonicalize an interpreter based on its executable path
            // because the executables are separate shim files (not links). Instead, we
            // compare the `sys.base_prefix`.
            let old_base_prefix = self.interpreter().sys_base_prefix();
            let selected_base_prefix = interpreter.sys_base_prefix();
            old_base_prefix == selected_base_prefix
        } else {
            // On Unix, we can see if the canonicalized executable is the same file.
            self.interpreter().sys_executable() == interpreter.sys_executable()
                || same_file::is_same_file(
                    self.interpreter().sys_executable(),
                    interpreter.sys_executable(),
                )
                .unwrap_or(false)
        }
    }
}
