use core::fmt;
use std::borrow::Cow;
use std::cmp::{Ordering, Reverse};
use std::ffi::OsStr;
use std::io::{self, Write};
#[cfg(windows)]
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use fs_err as fs;
use itertools::Itertools;
use thiserror::Error;
use tracing::{debug, warn};
#[cfg(windows)]
use windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

use uv_fs::{
    LockedFile, LockedFileError, LockedFileMode, Simplified, replace_symlink, symlink_or_copy_file,
};
use uv_platform::{Error as PlatformError, Os};
use uv_platform::{LibcDetectionError, Platform};
use uv_state::{StateBucket, StateStore};
use uv_static::EnvVars;
use uv_trampoline_builder::{Launcher, LauncherKind};

use crate::discovery::VersionRequest;
use crate::downloads::{Error as DownloadError, ManagedPythonDownload};
use crate::implementation::{
    Error as ImplementationError, ImplementationName, LenientImplementationName,
};
use crate::installation::{self, PythonInstallationKey};
use crate::interpreter::Interpreter;
use crate::python_version::PythonVersion;
use crate::{
    PythonInstallationMinorVersionKey, PythonRequest, PythonVariant, macos_dylib, sysconfig,
};

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    LockedFile(#[from] LockedFileError),
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
    #[error("Missing expected Python executable at {}", _0.user_display())]
    MissingExecutable(PathBuf),
    #[error("Missing expected target directory for Python minor version link at {}", _0.user_display())]
    MissingPythonMinorVersionLinkTargetDirectory(PathBuf),
    #[error("Failed to create canonical Python executable")]
    CanonicalizeExecutable(#[source] io::Error),
    #[error("Failed to create Python executable link")]
    LinkExecutable(#[source] io::Error),
    #[error("Failed to create Python minor version link directory")]
    PythonMinorVersionLinkDirectory(#[source] io::Error),
    #[error("Failed to create directory for Python executable link")]
    ExecutableDirectory(#[source] io::Error),
    #[error("Failed to read Python installation directory")]
    ReadError(#[source] io::Error),
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
    #[error("Failed to determine the libc used on the current platform")]
    LibcDetection(#[from] LibcDetectionError),
    #[error(transparent)]
    MacOsDylib(#[from] macos_dylib::Error),
}

/// Compare two build version strings.
///
/// Build versions are typically YYYYMMDD date strings. Comparison is done numerically
/// if both values parse as integers, otherwise falls back to lexicographic comparison.
pub fn compare_build_versions(a: &str, b: &str) -> Ordering {
    match (a.parse::<u64>(), b.parse::<u64>()) {
        (Ok(a_num), Ok(b_num)) => a_num.cmp(&b_num),
        _ => a.cmp(b),
    }
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
        Ok(LockedFile::acquire(
            self.root.join(".lock"),
            LockedFileMode::Exclusive,
            self.root.user_display(),
        )
        .await?)
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
        } else if let Some(install_dir) =
            std::env::var_os(EnvVars::UV_PYTHON_INSTALL_DIR).filter(|s| !s.is_empty())
        {
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
    ) -> Result<impl DoubleEndedIterator<Item = ManagedPythonInstallation> + use<>, Error> {
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
                    .map_err(Error::ReadError)?;
                directories
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => vec![],
            Err(err) => {
                return Err(Error::ReadError(err));
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
    ) -> Result<impl DoubleEndedIterator<Item = ManagedPythonInstallation> + use<>, Error> {
        let platform = Platform::from_env()?;

        let iter = Self::from_settings(None)?
            .find_all()?
            .filter(move |installation| {
                if !platform.supports(installation.platform()) {
                    debug!("Skipping managed installation `{installation}`: not supported by current platform `{platform}`");
                    return false;
                }
                true
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
        &'a self,
        version: &'a PythonVersion,
    ) -> Result<impl DoubleEndedIterator<Item = ManagedPythonInstallation> + 'a, Error> {
        let request = VersionRequest::from(version);
        Ok(self
            .find_matching_current_platform()?
            .filter(move |installation| request.matches_installation_key(installation.key())))
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
    url: Option<Cow<'static, str>>,
    /// The SHA256 of the Python archive at the URL.
    ///
    /// Empty when self was constructed from a path.
    sha256: Option<Cow<'static, str>>,
    /// The build version of the Python installation.
    ///
    /// Empty when self was constructed from a path without a BUILD file.
    build: Option<Cow<'static, str>>,
}

impl ManagedPythonInstallation {
    pub fn new(path: PathBuf, download: &ManagedPythonDownload) -> Self {
        Self {
            path,
            key: download.key().clone(),
            url: Some(download.url().clone()),
            sha256: download.sha256().cloned(),
            build: download.build().map(Cow::Borrowed),
        }
    }

    pub(crate) fn from_path(path: impl AsRef<Path>) -> Result<Self, Error> {
        let path = path.as_ref();

        let key = PythonInstallationKey::from_str(
            path.file_name()
                .ok_or(Error::NameError("name is empty".to_string()))?
                .to_str()
                .ok_or(Error::NameError("not a valid string".to_string()))?,
        )?;

        let path = std::path::absolute(path)
            .map_err(|err| Error::AbsolutePath(path.to_path_buf(), err))?;

        // Try to read the BUILD file if it exists
        let build = match fs::read_to_string(path.join("BUILD")) {
            Ok(content) => Some(Cow::Owned(content.trim().to_string())),
            Err(err) if err.kind() == io::ErrorKind::NotFound => None,
            Err(err) => return Err(err.into()),
        };

        Ok(Self {
            path,
            key,
            url: None,
            sha256: None,
            build,
        })
    }

    /// Try to create a [`ManagedPythonInstallation`] from an [`Interpreter`].
    ///
    /// Returns `None` if the interpreter is not a managed installation.
    pub fn try_from_interpreter(interpreter: &Interpreter) -> Option<Self> {
        let managed_root = ManagedPythonInstallations::from_settings(None).ok()?;

        // Canonicalize both paths to handle Windows path format differences
        // (e.g., \\?\ prefix, different casing, junction vs actual path).
        // Fall back to the original path if canonicalization fails (e.g., target doesn't exist).
        let sys_base_prefix = dunce::canonicalize(interpreter.sys_base_prefix())
            .unwrap_or_else(|_| interpreter.sys_base_prefix().to_path_buf());
        let root = dunce::canonicalize(managed_root.root())
            .unwrap_or_else(|_| managed_root.root().to_path_buf());

        // Verify the interpreter's base prefix is within the managed root
        let suffix = sys_base_prefix.strip_prefix(&root).ok()?;

        let first_component = suffix.components().next()?;
        let name = first_component.as_os_str().to_str()?;

        // Verify it's a valid installation key
        PythonInstallationKey::from_str(name).ok()?;

        // Construct the installation from the path within the managed root
        let path = managed_root.root().join(name);
        Self::from_path(path).ok()
    }

    /// The path to this managed installation's Python executable.
    ///
    /// If the installation has multiple executables i.e., `python`, `python3`, etc., this will
    /// return the _canonical_ executable name which the other names link to. On Unix, this is
    /// `python{major}.{minor}{variant}` and on Windows, this is `python{exe}`.
    ///
    /// If windowed is true, `pythonw.exe` is selected over `python.exe` on windows, with no changes
    /// on non-windows.
    pub fn executable(&self, windowed: bool) -> PathBuf {
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
            // Pyodide and GraalPy do not have a version suffix.
            ImplementationName::Pyodide => String::new(),
            ImplementationName::GraalPy => String::new(),
        };

        // On Windows, the executable is just `python.exe` even for alternative variants
        // GraalPy always uses `graalpy.exe` as the main executable
        let variant = if self.implementation() == ImplementationName::GraalPy {
            ""
        } else if cfg!(unix) {
            self.key.variant.executable_suffix()
        } else if cfg!(windows) && windowed {
            // Use windowed Python that doesn't open a terminal.
            "w"
        } else {
            ""
        };

        let name = format!(
            "{implementation}{version}{variant}{exe}",
            implementation = self.implementation().executable_name(),
            exe = std::env::consts::EXE_SUFFIX
        );

        let executable = executable_path_from_base(
            self.python_dir().as_path(),
            &name,
            &LenientImplementationName::from(self.implementation()),
            *self.key.os(),
        );

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

    pub fn implementation(&self) -> ImplementationName {
        match self.key.implementation().into_owned() {
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

    pub fn platform(&self) -> &Platform {
        self.key.platform()
    }

    /// The build version of this installation, if available.
    pub fn build(&self) -> Option<&str> {
        self.build.as_deref()
    }

    pub fn minor_version_key(&self) -> &PythonInstallationMinorVersionKey {
        PythonInstallationMinorVersionKey::ref_cast(&self.key)
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
                *implementation == self.implementation()
            }
            PythonRequest::ImplementationVersion(implementation, version) => {
                *implementation == self.implementation() && version.matches_version(&self.version())
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
                    return Err(Error::MissingExecutable(python.clone()));
                }
                Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {}
                Err(err) => {
                    return Err(Error::CanonicalizeExecutable(err));
                }
            }
        }

        Ok(())
    }

    /// Ensure the environment contains the symlink directory (or junction on Windows)
    /// pointing to the patch directory for this minor version.
    pub fn ensure_minor_version_link(&self) -> Result<(), Error> {
        if let Some(minor_version_link) = PythonMinorVersionLink::from_installation(self) {
            minor_version_link.create_directory()?;
        }
        Ok(())
    }

    /// Ensure the environment is marked as externally managed with the
    /// standard `EXTERNALLY-MANAGED` file.
    pub fn ensure_externally_managed(&self) -> Result<(), Error> {
        if self.key.os().is_emscripten() {
            // Emscripten's stdlib is a zip file so we can't put an
            // EXTERNALLY-MANAGED inside.
            return Ok(());
        }
        // Construct the path to the `stdlib` directory.
        let stdlib = if self.key.os().is_windows() {
            self.python_dir().join("Lib")
        } else {
            let lib_suffix = self.key.variant.lib_suffix();
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
            if self.key.os().is_emscripten() {
                // Emscripten's stdlib is a zip file so we can't update the
                // sysconfig directly
                return Ok(());
            }
            if self.implementation() == ImplementationName::CPython {
                sysconfig::update_sysconfig(
                    self.path(),
                    self.key.major,
                    self.key.minor,
                    self.key.variant.lib_suffix(),
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
            if self.key().os().is_like_darwin() {
                if self.implementation() == ImplementationName::CPython {
                    let dylib_path = self.python_dir().join("lib").join(format!(
                        "{}python{}{}{}",
                        std::env::consts::DLL_PREFIX,
                        self.key.version().python_version(),
                        self.key.variant().executable_suffix(),
                        std::env::consts::DLL_SUFFIX
                    ));
                    macos_dylib::patch_dylib_install_name(dylib_path)?;
                }
            }
        }
        Ok(())
    }

    /// Ensure the build version is written to a BUILD file in the installation directory.
    pub fn ensure_build_file(&self) -> Result<(), Error> {
        if let Some(ref build) = self.build {
            let build_file = self.path.join("BUILD");
            fs::write(&build_file, build.as_ref())?;
        }
        Ok(())
    }

    /// Returns `true` if the path is a link to this installation's binary, e.g., as created by
    /// [`create_bin_link`].
    pub fn is_bin_link(&self, path: &Path) -> bool {
        if cfg!(unix) {
            same_file::is_same_file(path, self.executable(false)).unwrap_or_default()
        } else if cfg!(windows) {
            let Some(launcher) = Launcher::try_from_path(path).unwrap_or_default() else {
                return false;
            };
            if !matches!(launcher.kind, LauncherKind::Python) {
                return false;
            }
            // We canonicalize the target path of the launcher in case it includes a minor version
            // junction directory. If canonicalization fails, we check against the launcher path
            // directly.
            dunce::canonicalize(&launcher.python_path).unwrap_or(launcher.python_path)
                == self.executable(false)
        } else {
            unreachable!("Only Windows and Unix are supported")
        }
    }

    /// Returns `true` if self is a suitable upgrade of other.
    pub fn is_upgrade_of(&self, other: &Self) -> bool {
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
        // If the patch versions are the same, we're handling a pre-release upgrade
        // or a build version upgrade
        if self.key.patch == other.key.patch {
            return match (self.key.prerelease, other.key.prerelease) {
                // Require a newer pre-release, if present on both
                (Some(self_pre), Some(other_pre)) => self_pre > other_pre,
                // Allow upgrade from pre-release to stable
                (None, Some(_)) => true,
                // Do not upgrade from stable to pre-release
                (Some(_), None) => false,
                // For matching stable versions (same patch, no prerelease), check build version
                (None, None) => match (self.build.as_deref(), other.build.as_deref()) {
                    // Download has build, installation doesn't -> upgrade (legacy)
                    (Some(_), None) => true,
                    // Both have build, compare them
                    (Some(self_build), Some(other_build)) => {
                        compare_build_versions(self_build, other_build) == Ordering::Greater
                    }
                    // Download doesn't have build -> no upgrade
                    (None, _) => false,
                },
            };
        }
        // Require a newer patch version
        if self.key.patch < other.key.patch {
            return false;
        }
        true
    }

    pub fn url(&self) -> Option<&str> {
        self.url.as_deref()
    }

    pub fn sha256(&self) -> Option<&str> {
        self.sha256.as_deref()
    }
}

/// A representation of a minor version symlink directory (or junction on Windows)
/// linking to the home directory of a Python installation.
#[derive(Clone, Debug)]
pub struct PythonMinorVersionLink {
    /// The symlink directory (or junction on Windows).
    pub symlink_directory: PathBuf,
    /// The full path to the executable including the symlink directory
    /// (or junction on Windows).
    pub symlink_executable: PathBuf,
    /// The target directory for the symlink. This is the home directory for
    /// a Python installation.
    pub target_directory: PathBuf,
}

impl PythonMinorVersionLink {
    /// Attempt to derive a path from an executable path that substitutes a minor
    /// version symlink directory (or junction on Windows) for the patch version
    /// directory.
    ///
    /// The implementation is expected to be CPython and, on Unix, the base Python is
    /// expected to be in `<home>/bin/` on Unix. If either condition isn't true,
    /// return [`None`].
    ///
    /// # Examples
    ///
    /// ## Unix
    /// For a Python 3.10.8 installation in `/path/to/uv/python/cpython-3.10.8-macos-aarch64-none/bin/python3.10`,
    /// the symlink directory would be `/path/to/uv/python/cpython-3.10-macos-aarch64-none` and the executable path including the
    /// symlink directory would be `/path/to/uv/python/cpython-3.10-macos-aarch64-none/bin/python3.10`.
    ///
    /// ## Windows
    /// For a Python 3.10.8 installation in `C:\path\to\uv\python\cpython-3.10.8-windows-x86_64-none\python.exe`,
    /// the junction would be `C:\path\to\uv\python\cpython-3.10-windows-x86_64-none` and the executable path including the
    /// junction would be `C:\path\to\uv\python\cpython-3.10-windows-x86_64-none\python.exe`.
    fn from_executable(executable: &Path, key: &PythonInstallationKey) -> Option<Self> {
        let implementation = key.implementation();
        if !matches!(
            implementation.as_ref(),
            LenientImplementationName::Known(ImplementationName::CPython)
        ) {
            // We don't currently support transparent upgrades for PyPy or GraalPy.
            return None;
        }
        let executable_name = executable
            .file_name()
            .expect("Executable file name should exist");
        let symlink_directory_name = PythonInstallationMinorVersionKey::ref_cast(key).to_string();
        let parent = executable
            .parent()
            .expect("Executable should have parent directory");

        // The home directory of the Python installation
        let target_directory = if cfg!(unix) {
            if parent
                .components()
                .next_back()
                .is_some_and(|c| c.as_os_str() == "bin")
            {
                parent.parent()?.to_path_buf()
            } else {
                return None;
            }
        } else if cfg!(windows) {
            parent.to_path_buf()
        } else {
            unimplemented!("Only Windows and Unix systems are supported.")
        };
        let symlink_directory = target_directory.with_file_name(symlink_directory_name);
        // If this would create a circular link, return `None`.
        if target_directory == symlink_directory {
            return None;
        }
        // The full executable path including the symlink directory (or junction).
        let symlink_executable = executable_path_from_base(
            symlink_directory.as_path(),
            &executable_name.to_string_lossy(),
            &implementation,
            *key.os(),
        );
        let minor_version_link = Self {
            symlink_directory,
            symlink_executable,
            target_directory,
        };
        Some(minor_version_link)
    }

    pub fn from_installation(installation: &ManagedPythonInstallation) -> Option<Self> {
        Self::from_executable(installation.executable(false).as_path(), installation.key())
    }

    pub fn create_directory(&self) -> Result<(), Error> {
        match replace_symlink(
            self.target_directory.as_path(),
            self.symlink_directory.as_path(),
        ) {
            Ok(()) => {
                debug!(
                    "Created link {} -> {}",
                    &self.symlink_directory.user_display(),
                    &self.target_directory.user_display(),
                );
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                return Err(Error::MissingPythonMinorVersionLinkTargetDirectory(
                    self.target_directory.clone(),
                ));
            }
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {}
            Err(err) => {
                return Err(Error::PythonMinorVersionLinkDirectory(err));
            }
        }
        Ok(())
    }

    /// Check if the minor version link exists and points to the expected target directory.
    ///
    /// This verifies both that the symlink/junction exists AND that it points to the
    /// `target_directory` specified in this struct. This is important because the link
    /// may exist but point to a different installation (e.g., after an upgrade), in which
    /// case we should not use the link for the current installation.
    pub fn exists(&self) -> bool {
        #[cfg(unix)]
        {
            self.symlink_directory
                .symlink_metadata()
                .is_ok_and(|metadata| metadata.file_type().is_symlink())
                && self
                    .read_target()
                    .is_some_and(|target| target == self.target_directory)
        }
        #[cfg(windows)]
        {
            self.symlink_directory
                .symlink_metadata()
                .is_ok_and(|metadata| {
                    // Check that this is a reparse point, which indicates this
                    // is a symlink or junction.
                    (metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0) != 0
                })
                && self
                    .read_target()
                    .is_some_and(|target| target == self.target_directory)
        }
    }

    /// Read the target of the minor version link.
    ///
    /// On Unix, this reads the symlink target. On Windows, this reads the junction target
    /// using the `junction` crate which properly handles the `\??\` prefix that Windows
    /// uses internally for junction targets.
    pub fn read_target(&self) -> Option<PathBuf> {
        #[cfg(unix)]
        {
            self.symlink_directory.read_link().ok()
        }
        #[cfg(windows)]
        {
            junction::get_target(&self.symlink_directory).ok()
        }
    }
}

/// Derive the full path to an executable from the given base path and executable
/// name. On Unix, this is, e.g., `<base>/bin/python3.10`. On Windows, this is,
/// e.g., `<base>\python.exe`.
fn executable_path_from_base(
    base: &Path,
    executable_name: &str,
    implementation: &LenientImplementationName,
    os: Os,
) -> PathBuf {
    if matches!(
        implementation,
        &LenientImplementationName::Known(ImplementationName::GraalPy)
    ) {
        // GraalPy is always in `bin/` regardless of the os
        base.join("bin").join(executable_name)
    } else if os.is_emscripten()
        || matches!(
            implementation,
            &LenientImplementationName::Known(ImplementationName::Pyodide)
        )
    {
        // Emscripten's canonical executable is in the base directory
        base.join(executable_name)
    } else if os.is_windows() {
        // On Windows, the executable is in the base directory
        base.join(executable_name)
    } else {
        // On Unix, the executable is in `bin/`
        base.join("bin").join(executable_name)
    }
}

/// Create a link to a managed Python executable.
///
/// If the file already exists at the link path, an error will be returned.
pub fn create_link_to_executable(link: &Path, executable: &Path) -> Result<(), Error> {
    let link_parent = link.parent().ok_or(Error::NoExecutableDirectory)?;
    fs_err::create_dir_all(link_parent).map_err(Error::ExecutableDirectory)?;

    if cfg!(unix) {
        // Note this will never copy on Unix — we use it here to allow compilation on Windows
        match symlink_or_copy_file(executable, link) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                Err(Error::MissingExecutable(executable.to_path_buf()))
            }
            Err(err) => Err(Error::LinkExecutable(err)),
        }
    } else if cfg!(windows) {
        use uv_trampoline_builder::windows_python_launcher;

        // TODO(zanieb): Install GUI launchers as well
        let launcher = windows_python_launcher(executable, false)?;

        // OK to use `std::fs` here, `fs_err` does not support `File::create_new` and we attach
        // error context anyway
        #[expect(clippy::disallowed_types)]
        {
            std::fs::File::create_new(link)
                .and_then(|mut file| file.write_all(launcher.as_ref()))
                .map_err(Error::LinkExecutable)
        }
    } else {
        unimplemented!("Only Windows and Unix are supported.")
    }
}

/// Create or replace a link to a managed Python executable.
///
/// If a file already exists at the link path, it will be atomically replaced.
///
/// See [`create_link_to_executable`] for a variant that errors if the link already exists.
pub fn replace_link_to_executable(link: &Path, executable: &Path) -> Result<(), Error> {
    let link_parent = link.parent().ok_or(Error::NoExecutableDirectory)?;
    fs_err::create_dir_all(link_parent).map_err(Error::ExecutableDirectory)?;

    if cfg!(unix) {
        replace_symlink(executable, link).map_err(Error::LinkExecutable)
    } else if cfg!(windows) {
        use uv_trampoline_builder::windows_python_launcher;

        let launcher = windows_python_launcher(executable, false)?;

        uv_fs::write_atomic_sync(link, &*launcher).map_err(Error::LinkExecutable)
    } else {
        unimplemented!("Only Windows and Unix are supported.")
    }
}

// TODO(zanieb): Only used in tests now.
/// Generate a platform portion of a key from the environment.
pub fn platform_key_from_env() -> Result<String, Error> {
    Ok(Platform::from_env()?.to_string().to_lowercase())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::implementation::LenientImplementationName;
    use crate::installation::PythonInstallationKey;
    use crate::{ImplementationName, PythonVariant};
    use std::path::PathBuf;
    use std::str::FromStr;
    use uv_pep440::{Prerelease, PrereleaseKind};
    use uv_platform::Platform;

    fn create_test_installation(
        implementation: ImplementationName,
        major: u8,
        minor: u8,
        patch: u8,
        prerelease: Option<Prerelease>,
        variant: PythonVariant,
        build: Option<&str>,
    ) -> ManagedPythonInstallation {
        let platform = Platform::from_str("linux-x86_64-gnu").unwrap();
        let key = PythonInstallationKey::new(
            LenientImplementationName::Known(implementation),
            major,
            minor,
            patch,
            prerelease,
            platform,
            variant,
        );
        ManagedPythonInstallation {
            path: PathBuf::from("/test/path"),
            key,
            url: None,
            sha256: None,
            build: build.map(|s| Cow::Owned(s.to_owned())),
        }
    }

    #[test]
    fn test_is_upgrade_of_same_version() {
        let installation = create_test_installation(
            ImplementationName::CPython,
            3,
            10,
            8,
            None,
            PythonVariant::Default,
            None,
        );

        // Same patch version should not be an upgrade
        assert!(!installation.is_upgrade_of(&installation));
    }

    #[test]
    fn test_is_upgrade_of_patch_version() {
        let older = create_test_installation(
            ImplementationName::CPython,
            3,
            10,
            8,
            None,
            PythonVariant::Default,
            None,
        );
        let newer = create_test_installation(
            ImplementationName::CPython,
            3,
            10,
            9,
            None,
            PythonVariant::Default,
            None,
        );

        // Newer patch version should be an upgrade
        assert!(newer.is_upgrade_of(&older));
        // Older patch version should not be an upgrade
        assert!(!older.is_upgrade_of(&newer));
    }

    #[test]
    fn test_is_upgrade_of_different_minor_version() {
        let py310 = create_test_installation(
            ImplementationName::CPython,
            3,
            10,
            8,
            None,
            PythonVariant::Default,
            None,
        );
        let py311 = create_test_installation(
            ImplementationName::CPython,
            3,
            11,
            0,
            None,
            PythonVariant::Default,
            None,
        );

        // Different minor versions should not be upgrades
        assert!(!py311.is_upgrade_of(&py310));
        assert!(!py310.is_upgrade_of(&py311));
    }

    #[test]
    fn test_is_upgrade_of_different_implementation() {
        let cpython = create_test_installation(
            ImplementationName::CPython,
            3,
            10,
            8,
            None,
            PythonVariant::Default,
            None,
        );
        let pypy = create_test_installation(
            ImplementationName::PyPy,
            3,
            10,
            9,
            None,
            PythonVariant::Default,
            None,
        );

        // Different implementations should not be upgrades
        assert!(!pypy.is_upgrade_of(&cpython));
        assert!(!cpython.is_upgrade_of(&pypy));
    }

    #[test]
    fn test_is_upgrade_of_different_variant() {
        let default = create_test_installation(
            ImplementationName::CPython,
            3,
            10,
            8,
            None,
            PythonVariant::Default,
            None,
        );
        let freethreaded = create_test_installation(
            ImplementationName::CPython,
            3,
            10,
            9,
            None,
            PythonVariant::Freethreaded,
            None,
        );

        // Different variants should not be upgrades
        assert!(!freethreaded.is_upgrade_of(&default));
        assert!(!default.is_upgrade_of(&freethreaded));
    }

    #[test]
    fn test_is_upgrade_of_prerelease() {
        let stable = create_test_installation(
            ImplementationName::CPython,
            3,
            10,
            8,
            None,
            PythonVariant::Default,
            None,
        );
        let prerelease = create_test_installation(
            ImplementationName::CPython,
            3,
            10,
            8,
            Some(Prerelease {
                kind: PrereleaseKind::Alpha,
                number: 1,
            }),
            PythonVariant::Default,
            None,
        );

        // A stable version is an upgrade from prerelease
        assert!(stable.is_upgrade_of(&prerelease));

        // Prerelease are not upgrades of stable versions
        assert!(!prerelease.is_upgrade_of(&stable));
    }

    #[test]
    fn test_is_upgrade_of_prerelease_to_prerelease() {
        let alpha1 = create_test_installation(
            ImplementationName::CPython,
            3,
            10,
            8,
            Some(Prerelease {
                kind: PrereleaseKind::Alpha,
                number: 1,
            }),
            PythonVariant::Default,
            None,
        );
        let alpha2 = create_test_installation(
            ImplementationName::CPython,
            3,
            10,
            8,
            Some(Prerelease {
                kind: PrereleaseKind::Alpha,
                number: 2,
            }),
            PythonVariant::Default,
            None,
        );

        // Later prerelease should be an upgrade
        assert!(alpha2.is_upgrade_of(&alpha1));
        // Earlier prerelease should not be an upgrade
        assert!(!alpha1.is_upgrade_of(&alpha2));
    }

    #[test]
    fn test_is_upgrade_of_prerelease_same_patch() {
        let prerelease = create_test_installation(
            ImplementationName::CPython,
            3,
            10,
            8,
            Some(Prerelease {
                kind: PrereleaseKind::Alpha,
                number: 1,
            }),
            PythonVariant::Default,
            None,
        );

        // Same prerelease should not be an upgrade
        assert!(!prerelease.is_upgrade_of(&prerelease));
    }

    #[test]
    fn test_is_upgrade_of_build_version() {
        let older_build = create_test_installation(
            ImplementationName::CPython,
            3,
            10,
            8,
            None,
            PythonVariant::Default,
            Some("20240101"),
        );
        let newer_build = create_test_installation(
            ImplementationName::CPython,
            3,
            10,
            8,
            None,
            PythonVariant::Default,
            Some("20240201"),
        );

        // Newer build version should be an upgrade
        assert!(newer_build.is_upgrade_of(&older_build));
        // Older build version should not be an upgrade
        assert!(!older_build.is_upgrade_of(&newer_build));
    }

    #[test]
    fn test_is_upgrade_of_build_version_same() {
        let installation = create_test_installation(
            ImplementationName::CPython,
            3,
            10,
            8,
            None,
            PythonVariant::Default,
            Some("20240101"),
        );

        // Same build version should not be an upgrade
        assert!(!installation.is_upgrade_of(&installation));
    }

    #[test]
    fn test_is_upgrade_of_build_with_legacy_installation() {
        let legacy = create_test_installation(
            ImplementationName::CPython,
            3,
            10,
            8,
            None,
            PythonVariant::Default,
            None,
        );
        let with_build = create_test_installation(
            ImplementationName::CPython,
            3,
            10,
            8,
            None,
            PythonVariant::Default,
            Some("20240101"),
        );

        // Installation with build should upgrade legacy installation without build
        assert!(with_build.is_upgrade_of(&legacy));
        // Legacy installation should not upgrade installation with build
        assert!(!legacy.is_upgrade_of(&with_build));
    }

    #[test]
    fn test_is_upgrade_of_patch_takes_precedence_over_build() {
        let older_patch_newer_build = create_test_installation(
            ImplementationName::CPython,
            3,
            10,
            8,
            None,
            PythonVariant::Default,
            Some("20240201"),
        );
        let newer_patch_older_build = create_test_installation(
            ImplementationName::CPython,
            3,
            10,
            9,
            None,
            PythonVariant::Default,
            Some("20240101"),
        );

        // Newer patch version should be an upgrade regardless of build
        assert!(newer_patch_older_build.is_upgrade_of(&older_patch_newer_build));
        // Older patch version should not be an upgrade even with newer build
        assert!(!older_patch_newer_build.is_upgrade_of(&newer_patch_older_build));
    }

    #[test]
    fn test_find_version_matching() {
        use crate::PythonVersion;

        let platform = Platform::from_env().unwrap();
        let temp_dir = tempfile::tempdir().unwrap();

        // Create mock installation directories
        fs::create_dir(temp_dir.path().join(format!("cpython-3.10.0-{platform}"))).unwrap();

        temp_env::with_var(
            uv_static::EnvVars::UV_PYTHON_INSTALL_DIR,
            Some(temp_dir.path()),
            || {
                let installations = ManagedPythonInstallations::from_settings(None).unwrap();

                // Version 3.1 should NOT match 3.10
                let v3_1 = PythonVersion::from_str("3.1").unwrap();
                let matched: Vec<_> = installations.find_version(&v3_1).unwrap().collect();
                assert_eq!(matched.len(), 0);

                // Check that 3.10 matches
                let v3_10 = PythonVersion::from_str("3.10").unwrap();
                let matched: Vec<_> = installations.find_version(&v3_10).unwrap().collect();
                assert_eq!(matched.len(), 1);
            },
        );
    }
}
