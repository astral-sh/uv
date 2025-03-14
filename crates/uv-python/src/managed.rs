use core::fmt;
use std::cmp::Reverse;
use std::ffi::OsStr;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use fs_err as fs;
use itertools::Itertools;
use same_file::is_same_file;
use thiserror::Error;
use tracing::{debug, warn};

use uv_fs::{symlink_or_copy_file, LockedFile, Simplified};
use uv_state::{StateBucket, StateStore};
use uv_static::EnvVars;
use uv_trampoline_builder::{windows_python_launcher, Launcher};

use crate::downloads::{Error as DownloadError, ManagedPythonDownload};
use crate::implementation::{
    Error as ImplementationError, ImplementationName, LenientImplementationName,
};
use crate::installation::{self, PythonInstallationKey};
use crate::libc::LibcDetectionError;
use crate::platform::Error as PlatformError;
use crate::platform::{Arch, Libc, Os};
use crate::python_version::PythonVersion;
use crate::{macos_dylib, sysconfig, PythonRequest, PythonVariant};

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
    #[error(transparent)]
    SysconfigError(#[from] sysconfig::Error),
    #[error("Failed to copy to: {0}", to.user_display())]
    CopyError {
        to: PathBuf,
        #[source]
        err: io::Error,
    },
    #[error("Missing expected Python executable at {}", _0.user_display())]
    MissingExecutable(PathBuf),
    #[error("Failed to create canonical Python executable at {} from {}", to.user_display(), from.user_display())]
    CanonicalizeExecutable {
        from: PathBuf,
        to: PathBuf,
        #[source]
        err: io::Error,
    },
    #[error("Failed to create Python executable link at {} from {}", to.user_display(), from.user_display())]
    LinkExecutable {
        from: PathBuf,
        to: PathBuf,
        #[source]
        err: io::Error,
    },
    #[error("Failed to create directory for Python executable link at {}", to.user_display())]
    ExecutableDirectory {
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
    #[error("Failed to find a directory to install executables into")]
    NoExecutableDirectory,
    #[error(transparent)]
    LauncherError(#[from] uv_trampoline_builder::Error),
    #[error("Failed to read managed Python directory name: {0}")]
    NameError(String),
    #[error("Failed to construct absolute path to managed Python directory: {}", _0.user_display())]
    AbsolutePath(PathBuf, #[source] io::Error),
    #[error(transparent)]
    NameParseError(#[from] installation::PythonInstallationKeyError),
    #[error(transparent)]
    LibcDetection(#[from] LibcDetectionError),
    #[error(transparent)]
    MacOsDylib(#[from] macos_dylib::Error),
}
/// A collection of uv-managed Python installations installed on the current system.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ManagedPythonInstallations {
    /// The path to the top-level directory of the installed Python versions.
    root: PathBuf,
}

impl ManagedPythonInstallations {
    /// A directory for Python installations at `root`.
    fn from_path(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Grab a file lock for the managed Python distribution directory to prevent concurrent access
    /// across processes.
    pub async fn lock(&self) -> Result<LockedFile, Error> {
        Ok(LockedFile::acquire(self.root.join(".lock"), self.root.user_display()).await?)
    }

    /// Prefer, in order:
    ///
    /// 1. The specific Python directory passed via the `install_dir` argument.
    /// 2. The specific Python directory specified with the `UV_PYTHON_INSTALL_DIR` environment variable.
    /// 3. A directory in the system-appropriate user-level data directory, e.g., `~/.local/uv/python`.
    /// 4. A directory in the local data directory, e.g., `./.uv/python`.
    pub fn from_settings(install_dir: Option<PathBuf>) -> Result<Self, Error> {
        if let Some(install_dir) = install_dir {
            Ok(Self::from_path(install_dir))
        } else if let Some(install_dir) = std::env::var_os(EnvVars::UV_PYTHON_INSTALL_DIR) {
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

    /// Return the location of the scratch directory for managed Python installations.
    pub fn scratch(&self) -> PathBuf {
        self.root.join(".temp")
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

        // Create the scratch directory, if it doesn't exist.
        let scratch = self.scratch();
        fs::create_dir_all(&scratch)?;

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
        let scratch = self.scratch();
        Ok(dirs
            .into_iter()
            // Ignore the scratch directory
            .filter(|path| *path != scratch)
            // Ignore any `.` prefixed directories
            .filter(|path| {
                path.file_name()
                    .and_then(OsStr::to_str)
                    .map(|name| !name.starts_with('.'))
                    .unwrap_or(true)
            })
            .filter_map(|path| {
                ManagedPythonInstallation::from_path(path)
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
        let os = Os::from_env();
        let arch = Arch::from_env();
        let libc = Libc::from_env()?;

        let iter = ManagedPythonInstallations::from_settings(None)?
            .find_all()?
            .filter(move |installation| {
                installation.key.os == os
                    && (arch.supports(installation.key.arch)
                        // TODO(zanieb): Allow inequal variants, as `Arch::supports` does not
                        // implement this yet. See https://github.com/astral-sh/uv/pull/9788
                        || arch.family == installation.key.arch.family)
                    && installation.key.libc == libc
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
    /// The URL with the Python archive.
    ///
    /// Empty when self was constructed from a path.
    url: Option<&'static str>,
    /// The SHA256 of the Python archive at the URL.
    ///
    /// Empty when self was constructed from a path.
    sha256: Option<&'static str>,
}

impl ManagedPythonInstallation {
    pub fn new(path: PathBuf, download: &ManagedPythonDownload) -> Self {
        Self {
            path,
            key: download.key().clone(),
            url: Some(download.url()),
            sha256: download.sha256(),
        }
    }

    pub(crate) fn from_path(path: PathBuf) -> Result<Self, Error> {
        let key = PythonInstallationKey::from_str(
            path.file_name()
                .ok_or(Error::NameError("name is empty".to_string()))?
                .to_str()
                .ok_or(Error::NameError("not a valid string".to_string()))?,
        )?;

        let path = std::path::absolute(&path).map_err(|err| Error::AbsolutePath(path, err))?;

        Ok(Self {
            path,
            key,
            url: None,
            sha256: None,
        })
    }

    /// The path to this managed installation's Python executable.
    ///
    /// If the installation has multiple execututables i.e., `python`, `python3`, etc., this will
    /// return the _canonical_ executable name which the other names link to. On Unix, this is
    /// `python{major}.{minor}{variant}` and on Windows, this is `python{exe}`.
    ///
    /// If windowed is true, `pythonw.exe` is selected over `python.exe` on windows, with no changes
    /// on non-windows.
    pub fn executable(&self, windowed: bool) -> PathBuf {
        let implementation = match self.implementation() {
            ImplementationName::CPython => "python",
            ImplementationName::PyPy => "pypy",
            ImplementationName::GraalPy => {
                unreachable!("Managed installations of GraalPy are not supported")
            }
        };

        let version = match self.implementation() {
            ImplementationName::CPython => {
                if cfg!(unix) {
                    format!("{}.{}", self.key.major, self.key.minor)
                } else {
                    String::new()
                }
            }
            // PyPy uses a full version number, even on Windows.
            ImplementationName::PyPy => format!("{}.{}", self.key.major, self.key.minor),
            ImplementationName::GraalPy => {
                unreachable!("Managed installations of GraalPy are not supported")
            }
        };

        // On Windows, the executable is just `python.exe` even for alternative variants
        let variant = if cfg!(unix) {
            self.key.variant.suffix()
        } else if cfg!(windows) && windowed {
            // Use windowed Python that doesn't open a terminal.
            "w"
        } else {
            ""
        };

        let name = format!(
            "{implementation}{version}{variant}{exe}",
            exe = std::env::consts::EXE_SUFFIX
        );

        let executable = if cfg!(windows) {
            self.python_dir().join(name)
        } else if cfg!(unix) {
            self.python_dir().join("bin").join(name)
        } else {
            unimplemented!("Only Windows and Unix systems are supported.")
        };

        // Workaround for python-build-standalone v20241016 which is missing the standard
        // `python.exe` executable in free-threaded distributions on Windows.
        //
        // See https://github.com/astral-sh/uv/issues/8298
        if cfg!(windows)
            && matches!(self.key.variant, PythonVariant::Freethreaded)
            && !executable.exists()
        {
            // This is the alternative executable name for the freethreaded variant
            return self.python_dir().join(format!(
                "python{}.{}t{}",
                self.key.major,
                self.key.minor,
                std::env::consts::EXE_SUFFIX
            ));
        }

        executable
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
            PythonRequest::File(path) => self.executable(false) == *path,
            PythonRequest::Default | PythonRequest::Any => true,
            PythonRequest::Directory(path) => self.path() == *path,
            PythonRequest::ExecutableName(name) => self
                .executable(false)
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

    /// Ensure the environment contains the canonical Python executable names.
    pub fn ensure_canonical_executables(&self) -> Result<(), Error> {
        let python = self.executable(false);

        let canonical_names = &["python"];

        for name in canonical_names {
            let executable =
                python.with_file_name(format!("{name}{exe}", exe = std::env::consts::EXE_SUFFIX));

            // Do not attempt to perform same-file copies — this is fine on Unix but fails on
            // Windows with a permission error instead of 'already exists'
            if executable == python {
                continue;
            }

            match symlink_or_copy_file(&python, &executable) {
                Ok(()) => {
                    debug!(
                        "Created link {} -> {}",
                        executable.user_display(),
                        python.user_display(),
                    );
                }
                Err(err) if err.kind() == io::ErrorKind::NotFound => {
                    return Err(Error::MissingExecutable(python.clone()))
                }
                Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {}
                Err(err) => {
                    return Err(Error::CanonicalizeExecutable {
                        from: executable,
                        to: python,
                        err,
                    })
                }
            };
        }

        Ok(())
    }

    /// Ensure the environment is marked as externally managed with the
    /// standard `EXTERNALLY-MANAGED` file.
    pub fn ensure_externally_managed(&self) -> Result<(), Error> {
        // Construct the path to the `stdlib` directory.
        let stdlib = if matches!(self.key.os, Os(target_lexicon::OperatingSystem::Windows)) {
            self.python_dir().join("Lib")
        } else {
            let lib_suffix = self.key.variant.suffix();
            let python = if matches!(
                self.key.implementation,
                LenientImplementationName::Known(ImplementationName::PyPy)
            ) {
                format!("pypy{}", self.key.version().python_version())
            } else {
                format!("python{}{lib_suffix}", self.key.version().python_version())
            };
            self.python_dir().join("lib").join(python)
        };

        let file = stdlib.join("EXTERNALLY-MANAGED");
        fs_err::write(file, EXTERNALLY_MANAGED)?;

        Ok(())
    }

    /// Ensure that the `sysconfig` data is patched to match the installation path.
    pub fn ensure_sysconfig_patched(&self) -> Result<(), Error> {
        if cfg!(unix) {
            if *self.implementation() == ImplementationName::CPython {
                sysconfig::update_sysconfig(
                    self.path(),
                    self.key.major,
                    self.key.minor,
                    self.key.variant.suffix(),
                )?;
            }
        }
        Ok(())
    }

    /// On macOS, ensure that the `install_name` for the Python dylib is set
    /// correctly, rather than pointing at `/install/lib/libpython{version}.dylib`.
    /// This is necessary to ensure that native extensions written in Rust
    /// link to the correct location for the Python library.
    ///
    /// See <https://github.com/astral-sh/uv/issues/10598> for more information.
    pub fn ensure_dylib_patched(&self) -> Result<(), macos_dylib::Error> {
        if cfg!(target_os = "macos") {
            if self.key().os.is_like_darwin() {
                if *self.implementation() == ImplementationName::CPython {
                    let dylib_path = self.python_dir().join("lib").join(format!(
                        "{}python{}{}{}",
                        std::env::consts::DLL_PREFIX,
                        self.key.version().python_version(),
                        self.key.variant().suffix(),
                        std::env::consts::DLL_SUFFIX
                    ));
                    macos_dylib::patch_dylib_install_name(dylib_path)?;
                }
            }
        }
        Ok(())
    }

    /// Create a link to the managed Python executable.
    ///
    /// If the file already exists at the target path, an error will be returned.
    pub fn create_bin_link(&self, target: &Path) -> Result<(), Error> {
        let python = self.executable(false);

        let bin = target.parent().ok_or(Error::NoExecutableDirectory)?;
        fs_err::create_dir_all(bin).map_err(|err| Error::ExecutableDirectory {
            to: bin.to_path_buf(),
            err,
        })?;

        if cfg!(unix) {
            // Note this will never copy on Unix — we use it here to allow compilation on Windows
            match symlink_or_copy_file(&python, target) {
                Ok(()) => Ok(()),
                Err(err) if err.kind() == io::ErrorKind::NotFound => {
                    Err(Error::MissingExecutable(python.clone()))
                }
                Err(err) => Err(Error::LinkExecutable {
                    from: python,
                    to: target.to_path_buf(),
                    err,
                }),
            }
        } else if cfg!(windows) {
            // TODO(zanieb): Install GUI launchers as well
            let launcher = windows_python_launcher(&python, false)?;

            // OK to use `std::fs` here, `fs_err` does not support `File::create_new` and we attach
            // error context anyway
            #[allow(clippy::disallowed_types)]
            {
                std::fs::File::create_new(target)
                    .and_then(|mut file| file.write_all(launcher.as_ref()))
                    .map_err(|err| Error::LinkExecutable {
                        from: python,
                        to: target.to_path_buf(),
                        err,
                    })
            }
        } else {
            unimplemented!("Only Windows and Unix systems are supported.")
        }
    }

    /// Returns `true` if the path is a link to this installation's binary, e.g., as created by
    /// [`ManagedPythonInstallation::create_bin_link`].
    pub fn is_bin_link(&self, path: &Path) -> bool {
        if cfg!(unix) {
            is_same_file(path, self.executable(false)).unwrap_or_default()
        } else if cfg!(windows) {
            let Some(launcher) = Launcher::try_from_path(path).unwrap_or_default() else {
                return false;
            };
            if !matches!(launcher.kind, uv_trampoline_builder::LauncherKind::Python) {
                return false;
            }
            launcher.python_path == self.executable(false)
        } else {
            unreachable!("Only Windows and Unix are supported")
        }
    }

    /// Returns `true` if self is a suitable upgrade of other.
    pub fn is_upgrade_of(&self, other: &ManagedPythonInstallation) -> bool {
        // Require matching implementation
        if self.key.implementation != other.key.implementation {
            return false;
        }
        // Require a matching variant
        if self.key.variant != other.key.variant {
            return false;
        }
        // Require matching minor version
        if (self.key.major, self.key.minor) != (other.key.major, other.key.minor) {
            return false;
        }
        // Require a newer, or equal patch version (for pre-release upgrades)
        if self.key.patch <= other.key.patch {
            return false;
        }
        if let Some(other_pre) = other.key.prerelease {
            if let Some(self_pre) = self.key.prerelease {
                return self_pre > other_pre;
            }
            // Do not upgrade from non-prerelease to prerelease
            return false;
        }
        // Do not upgrade if the patch versions are the same
        self.key.patch != other.key.patch
    }

    pub fn url(&self) -> Option<&'static str> {
        self.url
    }

    pub fn sha256(&self) -> Option<&'static str> {
        self.sha256
    }
}

// TODO(zanieb): Only used in tests now.
/// Generate a platform portion of a key from the environment.
pub fn platform_key_from_env() -> Result<String, Error> {
    let os = Os::from_env();
    let arch = Arch::from_env();
    let libc = Libc::from_env()?;
    Ok(format!("{os}-{arch}-{libc}").to_lowercase())
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

/// Find the directory to install Python executables into.
pub fn python_executable_dir() -> Result<PathBuf, Error> {
    uv_dirs::user_executable_directory(Some(EnvVars::UV_PYTHON_BIN_DIR))
        .ok_or(Error::NoExecutableDirectory)
}
