use core::fmt;
use fs_err as fs;
use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use uv_state::{StateBucket, StateStore};

// TODO(zanieb): Separate download and managed error types
pub use crate::downloads::Error;
use crate::platform::{Arch, Libc, Os};
use crate::python_version::PythonVersion;

/// A collection of installed Python toolchains.
#[derive(Debug, Clone)]
pub struct InstalledToolchains {
    /// The path to the top-level directory of the installed toolchains.
    root: PathBuf,
}

impl InstalledToolchains {
    /// A directory for installed toolchains at `root`.
    pub fn from_path(root: impl Into<PathBuf>) -> Result<Self, io::Error> {
        Ok(Self { root: root.into() })
    }

    /// Prefer, in order:
    /// 1. The specific toolchain directory specified by the user, i.e., `UV_TOOLCHAIN_DIR`
    /// 2. A directory in the system-appropriate user-level data directory, e.g., `~/.local/uv/toolchains`
    /// 3. A directory in the local data directory, e.g., `./.uv/toolchains`
    pub fn from_settings() -> Result<Self, io::Error> {
        if let Some(toolchain_dir) = std::env::var_os("UV_TOOLCHAIN_DIR") {
            Self::from_path(toolchain_dir)
        } else {
            Self::from_path(StateStore::from_settings(None)?.bucket(StateBucket::Toolchains))
        }
    }

    /// Create a temporary installed toolchain directory.
    pub fn temp() -> Result<Self, io::Error> {
        Self::from_path(StateStore::temp()?.bucket(StateBucket::Toolchains))
    }

    /// Initialize the installed toolchain directory.
    ///
    /// Ensures the directory is created.
    pub fn init(self) -> Result<Self, io::Error> {
        let root = &self.root;

        // Create the cache directory, if it doesn't exist.
        fs::create_dir_all(root)?;

        // Add a .gitignore.
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(root.join(".gitignore"))
        {
            Ok(mut file) => file.write_all(b"*")?,
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => (),
            Err(err) => return Err(err),
        }

        Ok(self)
    }

    /// Iterate over each installed toolchain in this directory.
    ///
    /// Toolchains are sorted descending by name, such that we get deterministic
    /// ordering across platforms. This also results in newer Python versions coming first,
    /// but should not be relied on — instead the toolchains should be sorted later by
    /// the parsed Python version.
    fn find_all(&self) -> Result<impl DoubleEndedIterator<Item = Toolchain>, Error> {
        let dirs = match fs_err::read_dir(&self.root) {
            Ok(toolchain_dirs) => {
                // Collect sorted directory paths; `read_dir` is not stable across platforms
                let directories: BTreeSet<_> = toolchain_dirs
                    .filter_map(|read_dir| match read_dir {
                        Ok(entry) => match entry.file_type() {
                            Ok(file_type) => file_type.is_dir().then_some(Ok(entry.path())),
                            Err(err) => Some(Err(err)),
                        },
                        Err(err) => Some(Err(err)),
                    })
                    .collect::<Result<_, std::io::Error>>()
                    .map_err(|err| Error::ReadError {
                        dir: self.root.clone(),
                        err,
                    })?;
                directories
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => BTreeSet::default(),
            Err(err) => {
                return Err(Error::ReadError {
                    dir: self.root.clone(),
                    err,
                })
            }
        };
        Ok(dirs
            .into_iter()
            .map(|path| Toolchain::new(path).unwrap())
            .rev())
    }

    /// Iterate over toolchains that support the current platform.
    pub fn find_matching_current_platform(
        &self,
    ) -> Result<impl DoubleEndedIterator<Item = Toolchain>, Error> {
        let platform_key = platform_key_from_env()?;

        let iter = InstalledToolchains::from_settings()?
            .find_all()?
            .filter(move |toolchain| {
                toolchain
                    .path
                    .file_name()
                    .map(OsStr::to_string_lossy)
                    .is_some_and(|filename| filename.ends_with(&platform_key))
            });

        Ok(iter)
    }

    /// Iterate over toolchains that satisfy the given Python version on this platform.
    ///
    /// ## Errors
    ///
    /// - The platform metadata cannot be read
    /// - A directory in the toolchain directory cannot be read
    pub fn find_version<'a>(
        &self,
        version: &'a PythonVersion,
    ) -> Result<impl DoubleEndedIterator<Item = Toolchain> + 'a, Error> {
        Ok(self
            .find_matching_current_platform()?
            .filter(move |toolchain| {
                toolchain
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

/// An installed Python toolchain.
#[derive(Debug, Clone)]
pub struct Toolchain {
    /// The path to the top-level directory of the installed toolchain.
    path: PathBuf,
    python_version: PythonVersion,
}

impl Toolchain {
    pub fn new(path: PathBuf) -> Result<Self, Error> {
        let python_version = PythonVersion::from_str(
            path.file_name()
                .ok_or(Error::NameError("No directory name".to_string()))?
                .to_str()
                .ok_or(Error::NameError("Name not a valid string".to_string()))?
                .split('-')
                .nth(1)
                .ok_or(Error::NameError(
                    "Not enough `-`-separated values".to_string(),
                ))?,
        )
        .map_err(|err| Error::NameError(format!("Name has invalid Python version: {err}")))?;

        Ok(Self {
            path,
            python_version,
        })
    }

    pub fn executable(&self) -> PathBuf {
        if cfg!(windows) {
            self.path.join("install").join("python.exe")
        } else if cfg!(unix) {
            self.path.join("install").join("bin").join("python3")
        } else {
            unimplemented!("Only Windows and Unix systems are supported.")
        }
    }

    pub fn python_version(&self) -> &PythonVersion {
        &self.python_version
    }
}

/// Generate a platform portion of a key from the environment.
fn platform_key_from_env() -> Result<String, Error> {
    let os = Os::from_env()?;
    let arch = Arch::from_env()?;
    let libc = Libc::from_env();
    Ok(format!("{os}-{arch}-{libc}").to_lowercase())
}

impl fmt::Display for Toolchain {
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
