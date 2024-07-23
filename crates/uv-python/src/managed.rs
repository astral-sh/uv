use core::fmt;
use fs_err as fs;
use itertools::Itertools;
use std::cmp::Reverse;
use std::ffi::OsStr;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use thiserror::Error;
use tracing::warn;

use uv_state::{StateBucket, StateStore};

use crate::downloads::Error as DownloadError;
use crate::implementation::{
    Error as ImplementationError, ImplementationName, LenientImplementationName,
};
use crate::installation::{self, PythonInstallationKey};
use crate::platform::Error as PlatformError;
use crate::platform::{Arch, Libc, Os};
use crate::python_version::PythonVersion;
use crate::PythonRequest;
use uv_fs::{LockedFile, Simplified};

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Download(#[from] DownloadError),
    #[error(transparent)]
    PlatformError(#[from] PlatformError),
    #[error(transparent)]
    ImplementationError(#[from] ImplementationError),
    #[error("Invalid python version: {0}")]
    InvalidPythonVersion(String),
    #[error(transparent)]
    ExtractError(#[from] uv_extract::Error),
    #[error("Failed to copy to: {0}", to.user_display())]
    CopyError {
        to: PathBuf,
        #[source]
        err: io::Error,
    },
    #[error("Failed to read Python installation directory: {0}", dir.user_display())]
    ReadError {
        dir: PathBuf,
        #[source]
        err: io::Error,
    },
    #[error("Failed to read managed Python directory name: {0}")]
    NameError(String),
    #[error(transparent)]
    NameParseError(#[from] installation::PythonInstallationKeyError),
}
/// A collection of uv-managed Python installations installed on the current system.
#[derive(Debug, Clone)]
pub struct ManagedPythonInstallations {
    /// The path to the top-level directory of the installed Python versions.
    root: PathBuf,
}

impl ManagedPythonInstallations {
    /// A directory for Python installations at `root`.
    fn from_path(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Lock the toolchains directory.
    pub fn acquire_lock(&self) -> Result<LockedFile, Error> {
        Ok(LockedFile::acquire(
            self.root.join(".lock"),
            self.root.user_display(),
        )?)
    }

    /// Prefer, in order:
    /// 1. The specific Python directory specified by the user, i.e., `UV_PYTHON_INSTALL_DIR`
    /// 2. A directory in the system-appropriate user-level data directory, e.g., `~/.local/uv/python`
    /// 3. A directory in the local data directory, e.g., `./.uv/python`
    pub fn from_settings() -> Result<Self, Error> {
        if let Some(install_dir) = std::env::var_os("UV_PYTHON_INSTALL_DIR") {
            Ok(Self::from_path(install_dir))
        } else {
            Ok(Self::from_path(
                StateStore::from_settings(None)?.bucket(StateBucket::ManagedPython),
            ))
        }
    }

    /// Create a temporary Python installation directory.
    pub fn temp() -> Result<Self, Error> {
        Ok(Self::from_path(
            StateStore::temp()?.bucket(StateBucket::ManagedPython),
        ))
    }

    /// Initialize the Python installation directory.
    ///
    /// Ensures the directory is created.
    pub fn init(self) -> Result<Self, Error> {
        let root = &self.root;

        // Support `toolchains` -> `python` migration transparently.
        if !root.exists()
            && root
                .parent()
                .is_some_and(|parent| parent.join("toolchains").exists())
        {
            let deprecated = root.parent().unwrap().join("toolchains");
            // Move the deprecated directory to the new location.
            fs::rename(&deprecated, root)?;
            // Create a link or junction to at the old location
            uv_fs::replace_symlink(root, &deprecated)?;
        } else {
            fs::create_dir_all(root)?;
        }

        // Create the directory, if it doesn't exist.
        fs::create_dir_all(root)?;

        // Add a .gitignore.
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(root.join(".gitignore"))
        {
            Ok(mut file) => file.write_all(b"*")?,
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => (),
            Err(err) => return Err(err.into()),
        }

        Ok(self)
    }

    /// Iterate over each Python installation in this directory.
    ///
    /// Pythons are sorted by [`PythonInstallationKey`], for the same implementation name, the newest versions come first.
    /// This ensures a consistent ordering across all platforms.
    pub fn find_all(
        &self,
    ) -> Result<impl DoubleEndedIterator<Item = ManagedPythonInstallation>, Error> {
        let dirs = match fs_err::read_dir(&self.root) {
            Ok(installation_dirs) => {
                // Collect sorted directory paths; `read_dir` is not stable across platforms
                let directories: Vec<_> = installation_dirs
                    .filter_map(|read_dir| match read_dir {
                        Ok(entry) => match entry.file_type() {
                            Ok(file_type) => file_type.is_dir().then_some(Ok(entry.path())),
                            Err(err) => Some(Err(err)),
                        },
                        Err(err) => Some(Err(err)),
                    })
                    .collect::<Result<_, io::Error>>()
                    .map_err(|err| Error::ReadError {
                        dir: self.root.clone(),
                        err,
                    })?;
                directories
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => vec![],
            Err(err) => {
                return Err(Error::ReadError {
                    dir: self.root.clone(),
                    err,
                })
            }
        };
        Ok(dirs
            .into_iter()
            .filter_map(|path| {
                ManagedPythonInstallation::new(path)
                    .inspect_err(|err| {
                        warn!("Ignoring malformed managed Python entry:\n    {err}");
                    })
                    .ok()
            })
            .sorted_unstable_by_key(|installation| Reverse(installation.key().clone())))
    }

    /// Iterate over Python installations that support the current platform.
    pub fn find_matching_current_platform(
        &self,
    ) -> Result<impl DoubleEndedIterator<Item = ManagedPythonInstallation>, Error> {
        let platform_key = platform_key_from_env();

        let iter = ManagedPythonInstallations::from_settings()?
            .find_all()?
            .filter(move |installation| {
                installation
                    .path
                    .file_name()
                    .map(OsStr::to_string_lossy)
                    .is_some_and(|filename| filename.ends_with(&platform_key))
            });

        Ok(iter)
    }

    /// Iterate over managed Python installations that satisfy the requested version on this platform.
    ///
    /// ## Errors
    ///
    /// - The platform metadata cannot be read
    /// - A directory for the installation cannot be read
    pub fn find_version<'a>(
        &self,
        version: &'a PythonVersion,
    ) -> Result<impl DoubleEndedIterator<Item = ManagedPythonInstallation> + 'a, Error> {
        Ok(self
            .find_matching_current_platform()?
            .filter(move |installation| {
                installation
                    .path
                    .file_name()
                    .map(OsStr::to_string_lossy)
                    .is_some_and(|filename| filename.starts_with(&format!("cpython-{version}")))
            }))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

static EXTERNALLY_MANAGED: &str = "[externally-managed]
Error=This Python installation is managed by uv and should not be modified.
";

/// A uv-managed Python installation on the current system.
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct ManagedPythonInstallation {
    /// The path to the top-level directory of the installed Python.
    path: PathBuf,
    /// An install key for the Python version.
    key: PythonInstallationKey,
}

impl ManagedPythonInstallation {
    pub fn new(path: PathBuf) -> Result<Self, Error> {
        let key = PythonInstallationKey::from_str(
            path.file_name()
                .ok_or(Error::NameError("name is empty".to_string()))?
                .to_str()
                .ok_or(Error::NameError("not a valid string".to_string()))?,
        )?;

        Ok(Self { path, key })
    }

    /// The path to this toolchain's Python executable.
    pub fn executable(&self) -> PathBuf {
        if cfg!(windows) {
            self.python_dir().join("python.exe")
        } else if cfg!(unix) {
            self.python_dir().join("bin").join("python3")
        } else {
            unimplemented!("Only Windows and Unix systems are supported.")
        }
    }

    fn python_dir(&self) -> PathBuf {
        let install = self.path.join("install");
        if install.is_dir() {
            install
        } else {
            self.path.clone()
        }
    }

    /// The [`PythonVersion`] of the toolchain.
    pub fn version(&self) -> PythonVersion {
        self.key.version()
    }

    pub fn implementation(&self) -> &ImplementationName {
        match self.key.implementation() {
            LenientImplementationName::Known(implementation) => implementation,
            LenientImplementationName::Unknown(_) => {
                panic!("Managed Python installations should have a known implementation")
            }
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn key(&self) -> &PythonInstallationKey {
        &self.key
    }

    pub fn satisfies(&self, request: &PythonRequest) -> bool {
        match request {
            PythonRequest::File(path) => self.executable() == *path,
            PythonRequest::Any => true,
            PythonRequest::Directory(path) => self.path() == *path,
            PythonRequest::ExecutableName(name) => self
                .executable()
                .file_name()
                .is_some_and(|filename| filename.to_string_lossy() == *name),
            PythonRequest::Implementation(implementation) => {
                implementation == self.implementation()
            }
            PythonRequest::ImplementationVersion(implementation, version) => {
                implementation == self.implementation() && version.matches_version(&self.version())
            }
            PythonRequest::Version(version) => version.matches_version(&self.version()),
            PythonRequest::Key(request) => request.satisfied_by_key(self.key()),
        }
    }

    /// Ensure the environment is marked as externally managed with the
    /// standard `EXTERNALLY-MANAGED` file.
    pub fn ensure_externally_managed(&self) -> Result<(), Error> {
        // Construct the path to the `stdlib` directory.
        let stdlib = if matches!(self.key.os, Os(target_lexicon::OperatingSystem::Windows)) {
            self.python_dir().join("Lib")
        } else {
            let python = if matches!(
                self.key.implementation,
                LenientImplementationName::Known(ImplementationName::PyPy)
            ) {
                format!("pypy{}", self.key.version().python_version())
            } else {
                format!("python{}", self.key.version().python_version())
            };
            self.python_dir().join("lib").join(python)
        };

        let file = stdlib.join("EXTERNALLY-MANAGED");
        fs_err::write(file, EXTERNALLY_MANAGED)?;

        Ok(())
    }
}

/// Generate a platform portion of a key from the environment.
fn platform_key_from_env() -> String {
    let os = Os::from_env();
    let arch = Arch::from_env();
    let libc = Libc::from_env();
    format!("{os}-{arch}-{libc}").to_lowercase()
}

impl fmt::Display for ManagedPythonInstallation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            self.path
                .file_name()
                .unwrap_or(self.path.as_os_str())
                .to_string_lossy()
        )
    }
}
