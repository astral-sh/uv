use std::borrow::Cow;
use std::env::consts::ARCH;
use std::fmt::{Display, Formatter};
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::sync::OnceLock;

use configparser::ini::Ini;
use fs_err as fs;
use owo_colors::OwoColorize;
use same_file::is_same_file;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, trace, warn};

use uv_cache::{Cache, CacheBucket, CachedByTimestamp, Freshness};
use uv_cache_info::Timestamp;
use uv_cache_key::cache_digest;
use uv_fs::{write_atomic_sync, PythonExt, Simplified};
use uv_install_wheel::Layout;
use uv_pep440::Version;
use uv_pep508::{MarkerEnvironment, StringVersion};
use uv_platform_tags::Platform;
use uv_platform_tags::{Tags, TagsError};
use uv_pypi_types::{ResolverMarkerEnvironment, Scheme};

use crate::implementation::LenientImplementationName;
use crate::platform::{Arch, Libc, Os};
use crate::pointer_size::PointerSize;
use crate::{
    Prefix, PythonInstallationKey, PythonVariant, PythonVersion, Target, VersionRequest,
    VirtualEnvironment,
};

/// A Python executable and its associated platform markers.
#[derive(Debug, Clone)]
pub struct Interpreter {
    platform: Platform,
    markers: Box<MarkerEnvironment>,
    scheme: Scheme,
    virtualenv: Scheme,
    manylinux_compatible: bool,
    sys_prefix: PathBuf,
    sys_base_exec_prefix: PathBuf,
    sys_base_prefix: PathBuf,
    sys_base_executable: Option<PathBuf>,
    sys_executable: PathBuf,
    sys_path: Vec<PathBuf>,
    stdlib: PathBuf,
    standalone: bool,
    tags: OnceLock<Tags>,
    target: Option<Target>,
    prefix: Option<Prefix>,
    pointer_size: PointerSize,
    gil_disabled: bool,
}

impl Interpreter {
    /// Detect the interpreter info for the given Python executable.
    pub fn query(executable: impl AsRef<Path>, cache: &Cache) -> Result<Self, Error> {
        let info = InterpreterInfo::query_cached(executable.as_ref(), cache)?;

        debug_assert!(
            info.sys_executable.is_absolute(),
            "`sys.executable` is not an absolute Python; Python installation is broken: {}",
            info.sys_executable.display()
        );

        Ok(Self {
            platform: info.platform,
            markers: Box::new(info.markers),
            scheme: info.scheme,
            virtualenv: info.virtualenv,
            manylinux_compatible: info.manylinux_compatible,
            sys_prefix: info.sys_prefix,
            sys_base_exec_prefix: info.sys_base_exec_prefix,
            pointer_size: info.pointer_size,
            gil_disabled: info.gil_disabled,
            sys_base_prefix: info.sys_base_prefix,
            sys_base_executable: info.sys_base_executable,
            sys_executable: info.sys_executable,
            sys_path: info.sys_path,
            stdlib: info.stdlib,
            standalone: info.standalone,
            tags: OnceLock::new(),
            target: None,
            prefix: None,
        })
    }

    /// Return a new [`Interpreter`] with the given virtual environment root.
    #[must_use]
    pub fn with_virtualenv(self, virtualenv: VirtualEnvironment) -> Self {
        Self {
            scheme: virtualenv.scheme,
            sys_base_executable: Some(virtualenv.base_executable),
            sys_executable: virtualenv.executable,
            sys_prefix: virtualenv.root,
            target: None,
            prefix: None,
            ..self
        }
    }

    /// Return a new [`Interpreter`] to install into the given `--target` directory.
    pub fn with_target(self, target: Target) -> io::Result<Self> {
        target.init()?;
        Ok(Self {
            target: Some(target),
            ..self
        })
    }

    /// Return a new [`Interpreter`] to install into the given `--prefix` directory.
    pub fn with_prefix(self, prefix: Prefix) -> io::Result<Self> {
        prefix.init(self.virtualenv())?;
        Ok(Self {
            prefix: Some(prefix),
            ..self
        })
    }

    /// Return the base Python executable; that is, the Python executable that should be
    /// considered the "base" for the virtual environment. This is typically the Python executable
    /// from the [`Interpreter`]; however, if the interpreter is a virtual environment itself, then
    /// the base Python executable is the Python executable of the interpreter's base interpreter.
    ///
    /// This routine relies on `sys._base_executable`, falling back to `sys.executable` if unset.
    /// Broadly, this routine should be used when attempting to determine the "base Python
    /// executable" in a way that is consistent with the CPython standard library, such as when
    /// determining the `home` key for a virtual environment.
    pub fn to_base_python(&self) -> Result<PathBuf, io::Error> {
        let base_executable = self.sys_base_executable().unwrap_or(self.sys_executable());
        let base_python = std::path::absolute(base_executable)?;
        Ok(base_python)
    }

    /// Determine the base Python executable; that is, the Python executable that should be
    /// considered the "base" for the virtual environment. This is typically the Python executable
    /// from the [`Interpreter`]; however, if the interpreter is a virtual environment itself, then
    /// the base Python executable is the Python executable of the interpreter's base interpreter.
    ///
    /// This routine mimics the CPython `getpath.py` logic in order to make a more robust assessment
    /// of the appropriate base Python executable. Broadly, this routine should be used when
    /// attempting to determine the "true" base executable for a Python interpreter by resolving
    /// symlinks until a valid Python installation is found. In particular, we tend to use this
    /// routine for our own managed (or standalone) Python installations.
    pub fn find_base_python(&self) -> Result<PathBuf, io::Error> {
        let base_executable = self.sys_base_executable().unwrap_or(self.sys_executable());
        // In `python-build-standalone`, a symlinked interpreter will return its own executable path
        // as `sys._base_executable`. Using the symlinked path as the base Python executable can be
        // incorrect, since it could cause `home` to point to something that is _not_ a Python
        // installation. Specifically, if the interpreter _itself_ is symlinked to an arbitrary
        // location, we need to fully resolve it to the actual Python executable; however, if the
        // entire standalone interpreter is symlinked, then we can use the symlinked path.
        //
        // We emulate CPython's `getpath.py` to ensure that the base executable results in a valid
        // Python prefix when converted into the `home` key for `pyvenv.cfg`.
        let base_python = match find_base_python(
            base_executable,
            self.python_major(),
            self.python_minor(),
            self.variant().suffix(),
        ) {
            Ok(path) => path,
            Err(err) => {
                warn!("Failed to find base Python executable: {err}");
                uv_fs::canonicalize_executable(base_executable)?
            }
        };
        Ok(base_python)
    }

    /// Returns the path to the Python virtual environment.
    #[inline]
    pub fn platform(&self) -> &Platform {
        &self.platform
    }

    /// Returns the [`MarkerEnvironment`] for this Python executable.
    #[inline]
    pub const fn markers(&self) -> &MarkerEnvironment {
        &self.markers
    }

    /// Return the [`ResolverMarkerEnvironment`] for this Python executable.
    pub fn resolver_marker_environment(&self) -> ResolverMarkerEnvironment {
        ResolverMarkerEnvironment::from(self.markers().clone())
    }

    /// Returns the [`PythonInstallationKey`] for this interpreter.
    pub fn key(&self) -> PythonInstallationKey {
        PythonInstallationKey::new(
            LenientImplementationName::from(self.implementation_name()),
            self.python_major(),
            self.python_minor(),
            self.python_patch(),
            self.python_version().pre(),
            self.os(),
            self.arch(),
            self.libc(),
            self.variant(),
        )
    }

    pub fn variant(&self) -> PythonVariant {
        if self.gil_disabled() {
            PythonVariant::Freethreaded
        } else {
            PythonVariant::default()
        }
    }

    /// Return the [`Arch`] reported by the interpreter platform tags.
    pub fn arch(&self) -> Arch {
        Arch::from(&self.platform().arch())
    }

    /// Return the [`Libc`] reported by the interpreter platform tags.
    pub fn libc(&self) -> Libc {
        Libc::from(self.platform().os())
    }

    /// Return the [`Os`] reported by the interpreter platform tags.
    pub fn os(&self) -> Os {
        Os::from(self.platform().os())
    }

    /// Returns the [`Tags`] for this Python executable.
    pub fn tags(&self) -> Result<&Tags, TagsError> {
        if self.tags.get().is_none() {
            let tags = Tags::from_env(
                self.platform(),
                self.python_tuple(),
                self.implementation_name(),
                self.implementation_tuple(),
                self.manylinux_compatible,
                self.gil_disabled,
            )?;
            self.tags.set(tags).expect("tags should not be set");
        }
        Ok(self.tags.get().expect("tags should be set"))
    }

    /// Returns `true` if the environment is a PEP 405-compliant virtual environment.
    ///
    /// See: <https://github.com/pypa/pip/blob/0ad4c94be74cc24874c6feb5bb3c2152c398a18e/src/pip/_internal/utils/virtualenv.py#L14>
    pub fn is_virtualenv(&self) -> bool {
        // Maybe this should return `false` if it's a target?
        self.sys_prefix != self.sys_base_prefix
    }

    /// Returns `true` if the environment is a `--target` environment.
    pub fn is_target(&self) -> bool {
        self.target.is_some()
    }

    /// Returns `true` if the environment is a `--prefix` environment.
    pub fn is_prefix(&self) -> bool {
        self.prefix.is_some()
    }

    /// Returns `Some` if the environment is externally managed, optionally including an error
    /// message from the `EXTERNALLY-MANAGED` file.
    ///
    /// See: <https://packaging.python.org/en/latest/specifications/externally-managed-environments/>
    pub fn is_externally_managed(&self) -> Option<ExternallyManaged> {
        // Per the spec, a virtual environment is never externally managed.
        if self.is_virtualenv() {
            return None;
        }

        // If we're installing into a target or prefix directory, it's never externally managed.
        if self.is_target() || self.is_prefix() {
            return None;
        }

        let Ok(contents) = fs::read_to_string(self.stdlib.join("EXTERNALLY-MANAGED")) else {
            return None;
        };

        let mut ini = Ini::new_cs();
        ini.set_multiline(true);

        let Ok(mut sections) = ini.read(contents) else {
            // If a file exists but is not a valid INI file, we assume the environment is
            // externally managed.
            return Some(ExternallyManaged::default());
        };

        let Some(section) = sections.get_mut("externally-managed") else {
            // If the file exists but does not contain an "externally-managed" section, we assume
            // the environment is externally managed.
            return Some(ExternallyManaged::default());
        };

        let Some(error) = section.remove("Error") else {
            // If the file exists but does not contain an "Error" key, we assume the environment is
            // externally managed.
            return Some(ExternallyManaged::default());
        };

        Some(ExternallyManaged { error })
    }

    /// Returns the `python_full_version` marker corresponding to this Python version.
    #[inline]
    pub fn python_full_version(&self) -> &StringVersion {
        self.markers.python_full_version()
    }

    /// Returns the full Python version.
    #[inline]
    pub fn python_version(&self) -> &Version {
        &self.markers.python_full_version().version
    }

    /// Returns the full minor Python version.
    #[inline]
    pub fn python_minor_version(&self) -> Version {
        Version::new(self.python_version().release().iter().take(2).copied())
    }

    /// Return the major version of this Python version.
    pub fn python_major(&self) -> u8 {
        let major = self.markers.python_full_version().version.release()[0];
        u8::try_from(major).expect("invalid major version")
    }

    /// Return the minor version of this Python version.
    pub fn python_minor(&self) -> u8 {
        let minor = self.markers.python_full_version().version.release()[1];
        u8::try_from(minor).expect("invalid minor version")
    }

    /// Return the patch version of this Python version.
    pub fn python_patch(&self) -> u8 {
        let minor = self.markers.python_full_version().version.release()[2];
        u8::try_from(minor).expect("invalid patch version")
    }

    /// Returns the Python version as a simple tuple.
    pub fn python_tuple(&self) -> (u8, u8) {
        (self.python_major(), self.python_minor())
    }

    /// Return the major version of the implementation (e.g., `CPython` or `PyPy`).
    pub fn implementation_major(&self) -> u8 {
        let major = self.markers.implementation_version().version.release()[0];
        u8::try_from(major).expect("invalid major version")
    }

    /// Return the minor version of the implementation (e.g., `CPython` or `PyPy`).
    pub fn implementation_minor(&self) -> u8 {
        let minor = self.markers.implementation_version().version.release()[1];
        u8::try_from(minor).expect("invalid minor version")
    }

    /// Returns the implementation version as a simple tuple.
    pub fn implementation_tuple(&self) -> (u8, u8) {
        (self.implementation_major(), self.implementation_minor())
    }

    /// Returns the implementation name (e.g., `CPython` or `PyPy`).
    pub fn implementation_name(&self) -> &str {
        self.markers.implementation_name()
    }

    /// Return the `sys.base_exec_prefix` path for this Python interpreter.
    pub fn sys_base_exec_prefix(&self) -> &Path {
        &self.sys_base_exec_prefix
    }

    /// Return the `sys.base_prefix` path for this Python interpreter.
    pub fn sys_base_prefix(&self) -> &Path {
        &self.sys_base_prefix
    }

    /// Return the `sys.prefix` path for this Python interpreter.
    pub fn sys_prefix(&self) -> &Path {
        &self.sys_prefix
    }

    /// Return the `sys._base_executable` path for this Python interpreter. Some platforms do not
    /// have this attribute, so it may be `None`.
    pub fn sys_base_executable(&self) -> Option<&Path> {
        self.sys_base_executable.as_deref()
    }

    /// Return the `sys.executable` path for this Python interpreter.
    pub fn sys_executable(&self) -> &Path {
        &self.sys_executable
    }

    /// Return the `sys.path` for this Python interpreter.
    pub fn sys_path(&self) -> &Vec<PathBuf> {
        &self.sys_path
    }

    /// Return the `stdlib` path for this Python interpreter, as returned by `sysconfig.get_paths()`.
    pub fn stdlib(&self) -> &Path {
        &self.stdlib
    }

    /// Return the `purelib` path for this Python interpreter, as returned by `sysconfig.get_paths()`.
    pub fn purelib(&self) -> &Path {
        &self.scheme.purelib
    }

    /// Return the `platlib` path for this Python interpreter, as returned by `sysconfig.get_paths()`.
    pub fn platlib(&self) -> &Path {
        &self.scheme.platlib
    }

    /// Return the `scripts` path for this Python interpreter, as returned by `sysconfig.get_paths()`.
    pub fn scripts(&self) -> &Path {
        &self.scheme.scripts
    }

    /// Return the `data` path for this Python interpreter, as returned by `sysconfig.get_paths()`.
    pub fn data(&self) -> &Path {
        &self.scheme.data
    }

    /// Return the `include` path for this Python interpreter, as returned by `sysconfig.get_paths()`.
    pub fn include(&self) -> &Path {
        &self.scheme.include
    }

    /// Return the [`Scheme`] for a virtual environment created by this [`Interpreter`].
    pub fn virtualenv(&self) -> &Scheme {
        &self.virtualenv
    }

    /// Return whether this interpreter is `manylinux` compatible.
    pub fn manylinux_compatible(&self) -> bool {
        self.manylinux_compatible
    }

    /// Return the [`PointerSize`] of the Python interpreter (i.e., 32- vs. 64-bit).
    pub fn pointer_size(&self) -> PointerSize {
        self.pointer_size
    }

    /// Return whether this is a Python 3.13+ freethreading Python, as specified by the sysconfig var
    /// `Py_GIL_DISABLED`.
    ///
    /// freethreading Python is incompatible with earlier native modules, re-introducing
    /// abiflags with a `t` flag. <https://peps.python.org/pep-0703/#build-configuration-changes>
    pub fn gil_disabled(&self) -> bool {
        self.gil_disabled
    }

    /// Return the `--target` directory for this interpreter, if any.
    pub fn target(&self) -> Option<&Target> {
        self.target.as_ref()
    }

    /// Return the `--prefix` directory for this interpreter, if any.
    pub fn prefix(&self) -> Option<&Prefix> {
        self.prefix.as_ref()
    }

    /// Returns `true` if an [`Interpreter`] may be a `python-build-standalone` interpreter.
    ///
    /// This method may return false positives, but it should not return false negatives. In other
    /// words, if this method returns `true`, the interpreter _may_ be from
    /// `python-build-standalone`; if it returns `false`, the interpreter is definitely _not_ from
    /// `python-build-standalone`.
    ///
    /// See: <https://github.com/astral-sh/python-build-standalone/issues/382>
    pub fn is_standalone(&self) -> bool {
        self.standalone
    }

    /// Return the [`Layout`] environment used to install wheels into this interpreter.
    pub fn layout(&self) -> Layout {
        Layout {
            python_version: self.python_tuple(),
            sys_executable: self.sys_executable().to_path_buf(),
            os_name: self.markers.os_name().to_string(),
            scheme: if let Some(target) = self.target.as_ref() {
                target.scheme()
            } else if let Some(prefix) = self.prefix.as_ref() {
                prefix.scheme(&self.virtualenv)
            } else {
                Scheme {
                    purelib: self.purelib().to_path_buf(),
                    platlib: self.platlib().to_path_buf(),
                    scripts: self.scripts().to_path_buf(),
                    data: self.data().to_path_buf(),
                    include: if self.is_virtualenv() {
                        // If the interpreter is a venv, then the `include` directory has a different structure.
                        // See: https://github.com/pypa/pip/blob/0ad4c94be74cc24874c6feb5bb3c2152c398a18e/src/pip/_internal/locations/_sysconfig.py#L172
                        self.sys_prefix.join("include").join("site").join(format!(
                            "python{}.{}",
                            self.python_major(),
                            self.python_minor()
                        ))
                    } else {
                        self.include().to_path_buf()
                    },
                }
            },
        }
    }

    /// Returns an iterator over the `site-packages` directories inside the environment.
    ///
    /// In most cases, `purelib` and `platlib` will be the same, and so the iterator will contain
    /// a single element; however, in some distributions, they may be different.
    ///
    /// Some distributions also create symbolic links from `purelib` to `platlib`; in such cases, we
    /// still deduplicate the entries, returning a single path.
    pub fn site_packages(&self) -> impl Iterator<Item = Cow<Path>> {
        let target = self.target().map(Target::site_packages);

        let prefix = self
            .prefix()
            .map(|prefix| prefix.site_packages(self.virtualenv()));

        let interpreter = if target.is_none() && prefix.is_none() {
            let purelib = self.purelib();
            let platlib = self.platlib();
            Some(std::iter::once(purelib).chain(
                if purelib == platlib || is_same_file(purelib, platlib).unwrap_or(false) {
                    None
                } else {
                    Some(platlib)
                },
            ))
        } else {
            None
        };

        target
            .into_iter()
            .flatten()
            .map(Cow::Borrowed)
            .chain(prefix.into_iter().flatten().map(Cow::Owned))
            .chain(interpreter.into_iter().flatten().map(Cow::Borrowed))
    }

    /// Check if the interpreter matches the given Python version.
    ///
    /// If a patch version is present, we will require an exact match.
    /// Otherwise, just the major and minor version numbers need to match.
    pub fn satisfies(&self, version: &PythonVersion) -> bool {
        if version.patch().is_some() {
            version.version() == self.python_version()
        } else {
            (version.major(), version.minor()) == self.python_tuple()
        }
    }

    /// Whether or not this Python interpreter is from a default Python executable name, like
    /// `python`, `python3`, or `python.exe`.
    pub(crate) fn has_default_executable_name(&self) -> bool {
        let Some(file_name) = self.sys_executable().file_name() else {
            return false;
        };
        let Some(name) = file_name.to_str() else {
            return false;
        };
        VersionRequest::Default
            .executable_names(None)
            .into_iter()
            .any(|default_name| name == default_name.to_string())
    }
}

/// The `EXTERNALLY-MANAGED` file in a Python installation.
///
/// See: <https://packaging.python.org/en/latest/specifications/externally-managed-environments/>
#[derive(Debug, Default, Clone)]
pub struct ExternallyManaged {
    error: Option<String>,
}

impl ExternallyManaged {
    /// Return the `EXTERNALLY-MANAGED` error message, if any.
    pub fn into_error(self) -> Option<String> {
        self.error
    }
}

#[derive(Debug, Error)]
pub struct UnexpectedResponseError {
    #[source]
    pub(super) err: serde_json::Error,
    pub(super) stdout: String,
    pub(super) stderr: String,
    pub(super) path: PathBuf,
}

impl Display for UnexpectedResponseError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Querying Python at `{}` returned an invalid response: {}",
            self.path.display(),
            self.err
        )?;

        let mut non_empty = false;

        if !self.stdout.trim().is_empty() {
            write!(f, "\n\n{}\n{}", "[stdout]".red(), self.stdout)?;
            non_empty = true;
        }

        if !self.stderr.trim().is_empty() {
            write!(f, "\n\n{}\n{}", "[stderr]".red(), self.stderr)?;
            non_empty = true;
        }

        if non_empty {
            writeln!(f)?;
        }

        Ok(())
    }
}

#[derive(Debug, Error)]
pub struct StatusCodeError {
    pub(super) code: ExitStatus,
    pub(super) stdout: String,
    pub(super) stderr: String,
    pub(super) path: PathBuf,
}

impl Display for StatusCodeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Querying Python at `{}` failed with exit status {}",
            self.path.display(),
            self.code
        )?;

        let mut non_empty = false;

        if !self.stdout.trim().is_empty() {
            write!(f, "\n\n{}\n{}", "[stdout]".red(), self.stdout)?;
            non_empty = true;
        }

        if !self.stderr.trim().is_empty() {
            write!(f, "\n\n{}\n{}", "[stderr]".red(), self.stderr)?;
            non_empty = true;
        }

        if non_empty {
            writeln!(f)?;
        }

        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("Failed to query Python interpreter")]
    Io(#[from] io::Error),
    #[error("Python interpreter not found at `{0}`")]
    NotFound(PathBuf),
    #[error("Failed to query Python interpreter at `{path}`")]
    SpawnFailed {
        path: PathBuf,
        #[source]
        err: io::Error,
    },
    #[error("{0}")]
    UnexpectedResponse(UnexpectedResponseError),
    #[error("{0}")]
    StatusCode(StatusCodeError),
    #[error("Can't use Python at `{path}`")]
    QueryScript {
        #[source]
        err: InterpreterInfoError,
        path: PathBuf,
    },
    #[error("Failed to write to cache")]
    Encode(#[from] rmp_serde::encode::Error),
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "result", rename_all = "lowercase")]
enum InterpreterInfoResult {
    Error(InterpreterInfoError),
    Success(Box<InterpreterInfo>),
}

#[derive(Debug, Error, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InterpreterInfoError {
    #[error("Could not detect a glibc or a musl libc (while running on Linux)")]
    LibcNotFound,
    #[error("Broken Python installation, `platform.mac_ver()` returned an empty value, please reinstall Python")]
    BrokenMacVer,
    #[error("Unknown operating system: `{operating_system}`")]
    UnknownOperatingSystem { operating_system: String },
    #[error("Python {python_version} is not supported. Please use Python 3.8 or newer.")]
    UnsupportedPythonVersion { python_version: String },
    #[error("Python executable does not support `-I` flag. Please use Python 3.8 or newer.")]
    UnsupportedPython,
    #[error("Python installation is missing `distutils`, which is required for packaging on older Python versions. Your system may package it separately, e.g., as `python{python_major}-distutils` or `python{python_major}.{python_minor}-distutils`.")]
    MissingRequiredDistutils {
        python_major: usize,
        python_minor: usize,
    },
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct InterpreterInfo {
    platform: Platform,
    markers: MarkerEnvironment,
    scheme: Scheme,
    virtualenv: Scheme,
    manylinux_compatible: bool,
    sys_prefix: PathBuf,
    sys_base_exec_prefix: PathBuf,
    sys_base_prefix: PathBuf,
    sys_base_executable: Option<PathBuf>,
    sys_executable: PathBuf,
    sys_path: Vec<PathBuf>,
    stdlib: PathBuf,
    standalone: bool,
    pointer_size: PointerSize,
    gil_disabled: bool,
}

impl InterpreterInfo {
    /// Return the resolved [`InterpreterInfo`] for the given Python executable.
    pub(crate) fn query(interpreter: &Path, cache: &Cache) -> Result<Self, Error> {
        let tempdir = tempfile::tempdir_in(cache.root())?;
        Self::setup_python_query_files(tempdir.path())?;

        // Sanitize the path by (1) running under isolated mode (`-I`) to ignore any site packages
        // modifications, and then (2) adding the path containing our query script to the front of
        // `sys.path` so that we can import it.
        let script = format!(
            r#"import sys; sys.path = ["{}"] + sys.path; from python.get_interpreter_info import main; main()"#,
            tempdir.path().escape_for_python()
        );
        let output = Command::new(interpreter)
            .arg("-I") // Isolated mode.
            .arg("-B") // Don't write bytecode.
            .arg("-c")
            .arg(script)
            .output()
            .map_err(|err| Error::SpawnFailed {
                path: interpreter.to_path_buf(),
                err,
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

            // If the Python version is too old, we may not even be able to invoke the query script
            if stderr.contains("Unknown option: -I") {
                return Err(Error::QueryScript {
                    err: InterpreterInfoError::UnsupportedPython,
                    path: interpreter.to_path_buf(),
                });
            }

            return Err(Error::StatusCode(StatusCodeError {
                code: output.status,
                stderr,
                stdout: String::from_utf8_lossy(&output.stdout).trim().to_string(),
                path: interpreter.to_path_buf(),
            }));
        }

        let result: InterpreterInfoResult =
            serde_json::from_slice(&output.stdout).map_err(|err| {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

                // If the Python version is too old, we may not even be able to invoke the query script
                if stderr.contains("Unknown option: -I") {
                    Error::QueryScript {
                        err: InterpreterInfoError::UnsupportedPython,
                        path: interpreter.to_path_buf(),
                    }
                } else {
                    Error::UnexpectedResponse(UnexpectedResponseError {
                        err,
                        stdout: String::from_utf8_lossy(&output.stdout).trim().to_string(),
                        stderr,
                        path: interpreter.to_path_buf(),
                    })
                }
            })?;

        match result {
            InterpreterInfoResult::Error(err) => Err(Error::QueryScript {
                err,
                path: interpreter.to_path_buf(),
            }),
            InterpreterInfoResult::Success(data) => Ok(*data),
        }
    }

    /// Duplicate the directory structure we have in `../python` into a tempdir, so we can run
    /// the Python probing scripts with `python -m python.get_interpreter_info` from that tempdir.
    fn setup_python_query_files(root: &Path) -> Result<(), Error> {
        let python_dir = root.join("python");
        fs_err::create_dir(&python_dir)?;
        fs_err::write(
            python_dir.join("get_interpreter_info.py"),
            include_str!("../python/get_interpreter_info.py"),
        )?;
        fs_err::write(
            python_dir.join("__init__.py"),
            include_str!("../python/__init__.py"),
        )?;
        let packaging_dir = python_dir.join("packaging");
        fs_err::create_dir(&packaging_dir)?;
        fs_err::write(
            packaging_dir.join("__init__.py"),
            include_str!("../python/packaging/__init__.py"),
        )?;
        fs_err::write(
            packaging_dir.join("_elffile.py"),
            include_str!("../python/packaging/_elffile.py"),
        )?;
        fs_err::write(
            packaging_dir.join("_manylinux.py"),
            include_str!("../python/packaging/_manylinux.py"),
        )?;
        fs_err::write(
            packaging_dir.join("_musllinux.py"),
            include_str!("../python/packaging/_musllinux.py"),
        )?;
        Ok(())
    }

    /// A wrapper around [`markers::query_interpreter_info`] to cache the computed markers.
    ///
    /// Running a Python script is (relatively) expensive, and the markers won't change
    /// unless the Python executable changes, so we use the executable's last modified
    /// time as a cache key.
    pub(crate) fn query_cached(executable: &Path, cache: &Cache) -> Result<Self, Error> {
        let absolute = std::path::absolute(executable)?;

        let cache_entry = cache.entry(
            CacheBucket::Interpreter,
            // Shard interpreter metadata by host architecture, operating system, and version, to
            // invalidate the cache (e.g.) on OS upgrades.
            cache_digest(&(
                ARCH,
                sys_info::os_type().unwrap_or_default(),
                sys_info::os_release().unwrap_or_default(),
            )),
            // We use the absolute path for the cache entry to avoid cache collisions for relative
            // paths. But we don't to query the executable with symbolic links resolved.
            format!("{}.msgpack", cache_digest(&absolute)),
        );

        // We check the timestamp of the canonicalized executable to check if an underlying
        // interpreter has been modified.
        let modified = uv_fs::canonicalize_executable(&absolute)
            .and_then(Timestamp::from_path)
            .map_err(|err| {
                if err.kind() == io::ErrorKind::NotFound {
                    Error::NotFound(executable.to_path_buf())
                } else {
                    err.into()
                }
            })?;

        // Read from the cache.
        if cache
            .freshness(&cache_entry, None, None)
            .is_ok_and(Freshness::is_fresh)
        {
            if let Ok(data) = fs::read(cache_entry.path()) {
                match rmp_serde::from_slice::<CachedByTimestamp<Self>>(&data) {
                    Ok(cached) => {
                        if cached.timestamp == modified {
                            trace!(
                                "Cached interpreter info for Python {}, skipping probing: {}",
                                cached.data.markers.python_full_version(),
                                executable.user_display()
                            );
                            return Ok(cached.data);
                        }

                        trace!(
                            "Ignoring stale interpreter markers for: {}",
                            executable.user_display()
                        );
                    }
                    Err(err) => {
                        warn!(
                            "Broken interpreter cache entry at {}, removing: {err}",
                            cache_entry.path().user_display()
                        );
                        let _ = fs_err::remove_file(cache_entry.path());
                    }
                }
            }
        }

        // Otherwise, run the Python script.
        trace!(
            "Querying interpreter executable at {}",
            executable.display()
        );
        let info = Self::query(executable, cache)?;

        // If `executable` is a pyenv shim, a bash script that redirects to the activated
        // python executable at another path, we're not allowed to cache the interpreter info.
        if is_same_file(executable, &info.sys_executable).unwrap_or(false) {
            fs::create_dir_all(cache_entry.dir())?;
            write_atomic_sync(
                cache_entry.path(),
                rmp_serde::to_vec(&CachedByTimestamp {
                    timestamp: modified,
                    data: info.clone(),
                })?,
            )?;
        }

        Ok(info)
    }
}

/// Find the Python executable that should be considered the "base" for a virtual environment.
///
/// Assumes that the provided executable is that of a standalone Python interpreter.
///
/// The strategy here mimics that of `getpath.py`: we search up the ancestor path to determine
/// whether a given executable will convert into a valid Python prefix; if not, we resolve the
/// symlink and try again.
///
/// This ensures that:
///
/// 1. We avoid using symlinks to arbitrary locations as the base Python executable. For example,
///    if a user symlinks a Python _executable_ to `/Users/user/foo`, we want to avoid using
///    `/Users/user` as `home`, since it's not a Python installation, and so the relevant libraries
///    and headers won't be found when it's used as the executable directory.
///    See: <https://github.com/python/cpython/blob/a03efb533a58fd13fb0cc7f4a5c02c8406a407bd/Modules/getpath.py#L367-L400>
///
/// 2. We use the "first" resolved symlink that _is_ a valid Python prefix, and thereby preserve
///    symlinks. For example, if a user symlinks a Python _installation_ to `/Users/user/foo`, such
///    that `/Users/user/foo/bin/python` is the resulting executable, we want to use `/Users/user/foo`
///    as `home`, rather than resolving to the symlink target. Concretely, this allows users to
///    symlink patch versions (like `cpython-3.12.6-macos-aarch64-none`) to minor version aliases
///    (like `cpython-3.12-macos-aarch64-none`) and preserve those aliases in the resulting virtual
///    environments.
///
/// See: <https://github.com/python/cpython/blob/a03efb533a58fd13fb0cc7f4a5c02c8406a407bd/Modules/getpath.py#L591-L594>
fn find_base_python(
    executable: &Path,
    major: u8,
    minor: u8,
    suffix: &str,
) -> Result<PathBuf, io::Error> {
    /// Returns `true` if `path` is the root directory.
    fn is_root(path: &Path) -> bool {
        let mut components = path.components();
        components.next() == Some(std::path::Component::RootDir) && components.next().is_none()
    }

    /// Determining whether `dir` is a valid Python prefix by searching for a "landmark".
    ///
    /// See: <https://github.com/python/cpython/blob/a03efb533a58fd13fb0cc7f4a5c02c8406a407bd/Modules/getpath.py#L183>
    fn is_prefix(dir: &Path, major: u8, minor: u8, suffix: &str) -> bool {
        if cfg!(windows) {
            dir.join("Lib").join("os.py").is_file()
        } else {
            dir.join("lib")
                .join(format!("python{major}.{minor}{suffix}"))
                .join("os.py")
                .is_file()
        }
    }

    let mut executable = Cow::Borrowed(executable);

    loop {
        debug!(
            "Assessing Python executable as base candidate: {}",
            executable.display()
        );

        // Determine whether this executable will produce a valid `home` for a virtual environment.
        for prefix in executable.ancestors().take_while(|path| !is_root(path)) {
            if is_prefix(prefix, major, minor, suffix) {
                return Ok(executable.into_owned());
            }
        }

        // If not, resolve the symlink.
        let resolved = fs_err::read_link(&executable)?;

        // If the symlink is relative, resolve it relative to the executable.
        let resolved = if resolved.is_relative() {
            if let Some(parent) = executable.parent() {
                parent.join(resolved)
            } else {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    "Symlink has no parent directory",
                ));
            }
        } else {
            resolved
        };

        // Normalize the resolved path.
        let resolved = uv_fs::normalize_absolute_path(&resolved)?;

        executable = Cow::Owned(resolved);
    }
}

#[cfg(unix)]
#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use fs_err as fs;
    use indoc::{formatdoc, indoc};
    use tempfile::tempdir;

    use uv_cache::Cache;
    use uv_pep440::Version;

    use crate::Interpreter;

    #[test]
    fn test_cache_invalidation() {
        let mock_dir = tempdir().unwrap();
        let mocked_interpreter = mock_dir.path().join("python");
        let json = indoc! {r##"
        {
            "result": "success",
            "platform": {
                "os": {
                    "name": "manylinux",
                    "major": 2,
                    "minor": 38
                },
                "arch": "x86_64"
            },
            "manylinux_compatible": false,
            "standalone": false,
            "markers": {
                "implementation_name": "cpython",
                "implementation_version": "3.12.0",
                "os_name": "posix",
                "platform_machine": "x86_64",
                "platform_python_implementation": "CPython",
                "platform_release": "6.5.0-13-generic",
                "platform_system": "Linux",
                "platform_version": "#13-Ubuntu SMP PREEMPT_DYNAMIC Fri Nov  3 12:16:05 UTC 2023",
                "python_full_version": "3.12.0",
                "python_version": "3.12",
                "sys_platform": "linux"
            },
            "sys_base_exec_prefix": "/home/ferris/.pyenv/versions/3.12.0",
            "sys_base_prefix": "/home/ferris/.pyenv/versions/3.12.0",
            "sys_prefix": "/home/ferris/projects/uv/.venv",
            "sys_executable": "/home/ferris/projects/uv/.venv/bin/python",
            "sys_path": [
                "/home/ferris/.pyenv/versions/3.12.0/lib/python3.12/lib/python3.12",
                "/home/ferris/.pyenv/versions/3.12.0/lib/python3.12/site-packages"
            ],
            "stdlib": "/home/ferris/.pyenv/versions/3.12.0/lib/python3.12",
            "scheme": {
                "data": "/home/ferris/.pyenv/versions/3.12.0",
                "include": "/home/ferris/.pyenv/versions/3.12.0/include",
                "platlib": "/home/ferris/.pyenv/versions/3.12.0/lib/python3.12/site-packages",
                "purelib": "/home/ferris/.pyenv/versions/3.12.0/lib/python3.12/site-packages",
                "scripts": "/home/ferris/.pyenv/versions/3.12.0/bin"
            },
            "virtualenv": {
                "data": "",
                "include": "include",
                "platlib": "lib/python3.12/site-packages",
                "purelib": "lib/python3.12/site-packages",
                "scripts": "bin"
            },
            "pointer_size": "64",
            "gil_disabled": true
        }
    "##};

        let cache = Cache::temp().unwrap().init().unwrap();

        fs::write(
            &mocked_interpreter,
            formatdoc! {r"
        #!/bin/sh
        echo '{json}'
        "},
        )
        .unwrap();

        fs::set_permissions(
            &mocked_interpreter,
            std::os::unix::fs::PermissionsExt::from_mode(0o770),
        )
        .unwrap();
        let interpreter = Interpreter::query(&mocked_interpreter, &cache).unwrap();
        assert_eq!(
            interpreter.markers.python_version().version,
            Version::from_str("3.12").unwrap()
        );
        fs::write(
            &mocked_interpreter,
            formatdoc! {r"
        #!/bin/sh
        echo '{}'
        ", json.replace("3.12", "3.13")},
        )
        .unwrap();
        let interpreter = Interpreter::query(&mocked_interpreter, &cache).unwrap();
        assert_eq!(
            interpreter.markers.python_version().version,
            Version::from_str("3.13").unwrap()
        );
    }
}
