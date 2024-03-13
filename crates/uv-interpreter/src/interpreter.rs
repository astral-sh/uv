use std::path::{Path, PathBuf};
use std::process::Command;

use configparser::ini::Ini;
use fs_err as fs;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use cache_key::digest;
use install_wheel_rs::Layout;
use pep440_rs::Version;
use pep508_rs::{MarkerEnvironment, StringVersion};
use platform_tags::Platform;
use platform_tags::{Tags, TagsError};
use pypi_types::Scheme;
use uv_cache::{Cache, CacheBucket, CachedByTimestamp, Freshness, Timestamp};
use uv_fs::{write_atomic_sync, Simplified};

use crate::Error;
use crate::Virtualenv;

/// A Python executable and its associated platform markers.
#[derive(Debug, Clone)]
pub struct Interpreter {
    platform: Platform,
    markers: Box<MarkerEnvironment>,
    scheme: Scheme,
    virtualenv: Scheme,
    prefix: PathBuf,
    base_exec_prefix: PathBuf,
    base_prefix: PathBuf,
    base_executable: Option<PathBuf>,
    sys_executable: PathBuf,
    stdlib: PathBuf,
    tags: OnceCell<Tags>,
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
            prefix: info.prefix,
            base_exec_prefix: info.base_exec_prefix,
            base_prefix: info.base_prefix,
            base_executable: info.base_executable,
            sys_executable: info.sys_executable,
            stdlib: info.stdlib,
            tags: OnceCell::new(),
        })
    }

    // TODO(konstin): Find a better way mocking the fields
    pub fn artificial(platform: Platform, markers: MarkerEnvironment) -> Self {
        Self {
            platform,
            markers: Box::new(markers),
            scheme: Scheme {
                purelib: PathBuf::from("/dev/null"),
                platlib: PathBuf::from("/dev/null"),
                include: PathBuf::from("/dev/null"),
                scripts: PathBuf::from("/dev/null"),
                data: PathBuf::from("/dev/null"),
            },
            virtualenv: Scheme {
                purelib: PathBuf::from("/dev/null"),
                platlib: PathBuf::from("/dev/null"),
                include: PathBuf::from("/dev/null"),
                scripts: PathBuf::from("/dev/null"),
                data: PathBuf::from("/dev/null"),
            },
            prefix: PathBuf::from("/dev/null"),
            base_exec_prefix: PathBuf::from("/dev/null"),
            base_prefix: PathBuf::from("/dev/null"),
            base_executable: None,
            sys_executable: PathBuf::from("/dev/null"),
            stdlib: PathBuf::from("/dev/null"),
            tags: OnceCell::new(),
        }
    }

    /// Return a new [`Interpreter`] with the given virtual environment root.
    #[must_use]
    pub fn with_virtualenv(self, virtualenv: Virtualenv) -> Self {
        Self {
            scheme: virtualenv.scheme,
            sys_executable: virtualenv.executable,
            prefix: virtualenv.root,
            ..self
        }
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

    /// Returns the [`Tags`] for this Python executable.
    pub fn tags(&self) -> Result<&Tags, TagsError> {
        self.tags.get_or_try_init(|| {
            Tags::from_env(
                self.platform(),
                self.python_tuple(),
                self.implementation_name(),
                self.implementation_tuple(),
            )
        })
    }

    /// Returns `true` if the environment is a PEP 405-compliant virtual environment.
    ///
    /// See: <https://github.com/pypa/pip/blob/0ad4c94be74cc24874c6feb5bb3c2152c398a18e/src/pip/_internal/utils/virtualenv.py#L14>
    pub fn is_virtualenv(&self) -> bool {
        self.prefix != self.base_prefix
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

    /// Returns the Python version.
    #[inline]
    pub const fn python_version(&self) -> &Version {
        &self.markers.python_full_version.version
    }

    /// Returns the `python_full_version` marker corresponding to this Python version.
    #[inline]
    pub const fn python_full_version(&self) -> &StringVersion {
        &self.markers.python_full_version
    }

    /// Return the major version of this Python version.
    pub fn python_major(&self) -> u8 {
        let major = self.markers.python_full_version.version.release()[0];
        u8::try_from(major).expect("invalid major version")
    }

    /// Return the minor version of this Python version.
    pub fn python_minor(&self) -> u8 {
        let minor = self.markers.python_full_version.version.release()[1];
        u8::try_from(minor).expect("invalid minor version")
    }

    /// Return the patch version of this Python version.
    pub fn python_patch(&self) -> u8 {
        let minor = self.markers.python_full_version.version.release()[2];
        u8::try_from(minor).expect("invalid patch version")
    }

    /// Returns the Python version as a simple tuple.
    pub fn python_tuple(&self) -> (u8, u8) {
        (self.python_major(), self.python_minor())
    }

    /// Return the major version of the implementation (e.g., `CPython` or `PyPy`).
    pub fn implementation_major(&self) -> u8 {
        let major = self.markers.implementation_version.version.release()[0];
        u8::try_from(major).expect("invalid major version")
    }

    /// Return the minor version of the implementation (e.g., `CPython` or `PyPy`).
    pub fn implementation_minor(&self) -> u8 {
        let minor = self.markers.implementation_version.version.release()[1];
        u8::try_from(minor).expect("invalid minor version")
    }

    /// Returns the implementation version as a simple tuple.
    pub fn implementation_tuple(&self) -> (u8, u8) {
        (self.implementation_major(), self.implementation_minor())
    }

    /// Returns the implementation name (e.g., `CPython` or `PyPy`).
    pub fn implementation_name(&self) -> &str {
        &self.markers.implementation_name
    }

    /// Return the `sys.base_exec_prefix` path for this Python interpreter.
    pub fn base_exec_prefix(&self) -> &Path {
        &self.base_exec_prefix
    }

    /// Return the `sys.base_prefix` path for this Python interpreter.
    pub fn base_prefix(&self) -> &Path {
        &self.base_prefix
    }

    /// Return the `sys.prefix` path for this Python interpreter.
    pub fn prefix(&self) -> &Path {
        &self.prefix
    }

    /// Return the `sys._base_executable` path for this Python interpreter. Some platforms do not
    /// have this attribute, so it may be `None`.
    pub fn base_executable(&self) -> Option<&Path> {
        self.base_executable.as_deref()
    }

    /// Return the `sys.executable` path for this Python interpreter.
    pub fn sys_executable(&self) -> &Path {
        &self.sys_executable
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

    /// Return the [`Layout`] environment used to install wheels into this interpreter.
    pub fn layout(&self) -> Layout {
        Layout {
            python_version: self.python_tuple(),
            sys_executable: self.sys_executable().to_path_buf(),
            os_name: self.markers.os_name.clone(),
            scheme: Scheme {
                purelib: self.purelib().to_path_buf(),
                platlib: self.platlib().to_path_buf(),
                scripts: self.scripts().to_path_buf(),
                data: self.data().to_path_buf(),
                include: if self.is_virtualenv() {
                    // If the interpreter is a venv, then the `include` directory has a different structure.
                    // See: https://github.com/pypa/pip/blob/0ad4c94be74cc24874c6feb5bb3c2152c398a18e/src/pip/_internal/locations/_sysconfig.py#L172
                    self.prefix.join("include").join("site").join(format!(
                        "python{}.{}",
                        self.python_major(),
                        self.python_minor()
                    ))
                } else {
                    self.include().to_path_buf()
                },
            },
        }
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
    #[error("Unknown operation system: `{operating_system}`")]
    UnknownOperatingSystem { operating_system: String },
    #[error("Python 2 is not supported. Please use Python 3.8 or newer.")]
    UnsupportedPythonVersion,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct InterpreterInfo {
    platform: Platform,
    markers: MarkerEnvironment,
    scheme: Scheme,
    virtualenv: Scheme,
    prefix: PathBuf,
    base_exec_prefix: PathBuf,
    base_prefix: PathBuf,
    base_executable: Option<PathBuf>,
    sys_executable: PathBuf,
    stdlib: PathBuf,
}

impl InterpreterInfo {
    /// Return the resolved [`InterpreterInfo`] for the given Python executable.
    pub(crate) fn query(interpreter: &Path, cache: &Cache) -> Result<Self, Error> {
        let tempdir = tempfile::tempdir_in(cache.root())?;
        Self::setup_python_query_files(tempdir.path())?;

        let output = Command::new(interpreter)
            .arg("-m")
            .arg("python.get_interpreter_info")
            .current_dir(tempdir.path().simplified())
            .output()
            .map_err(|err| Error::PythonSubcommandLaunch {
                interpreter: interpreter.to_path_buf(),
                err,
            })?;

        // stderr isn't technically a criterion for success, but i don't know of any cases where there
        // should be stderr output and if there is, we want to know
        if !output.status.success() || !output.stderr.is_empty() {
            return Err(Error::PythonSubcommandOutput {
                message: format!(
                    "Querying Python at `{}` failed with status {}",
                    interpreter.display(),
                    output.status,
                ),
                exit_code: output.status,
                stdout: String::from_utf8_lossy(&output.stdout).trim().to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            });
        }

        let result: InterpreterInfoResult =
            serde_json::from_slice(&output.stdout).map_err(|err| {
                Error::PythonSubcommandOutput {
                    message: format!(
                        "Querying Python at `{}` did not return the expected data: {err}",
                        interpreter.display(),
                    ),
                    exit_code: output.status,
                    stdout: String::from_utf8_lossy(&output.stdout).trim().to_string(),
                    stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
                }
            })?;

        match result {
            InterpreterInfoResult::Error(err) => Err(Error::QueryScript {
                err,
                interpreter: interpreter.to_path_buf(),
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
        let executable_bytes = executable.as_os_str().as_encoded_bytes();

        let cache_entry = cache.entry(
            CacheBucket::Interpreter,
            "",
            format!("{}.msgpack", digest(&executable_bytes)),
        );

        let modified = Timestamp::from_path(uv_fs::canonicalize_executable(executable)?)?;

        // Read from the cache.
        if cache
            .freshness(&cache_entry, None)
            .is_ok_and(Freshness::is_fresh)
        {
            if let Ok(data) = fs::read(cache_entry.path()) {
                match rmp_serde::from_slice::<CachedByTimestamp<Self>>(&data) {
                    Ok(cached) => {
                        if cached.timestamp == modified {
                            debug!(
                                "Cached interpreter info for Python {}, skipping probing: {}",
                                cached.data.markers.python_full_version,
                                executable.simplified_display()
                            );
                            return Ok(cached.data);
                        }

                        debug!(
                            "Ignoring stale cached markers for: {}",
                            executable.simplified_display()
                        );
                    }
                    Err(err) => {
                        warn!(
                            "Broken cache entry at {}, removing: {err}",
                            cache_entry.path().simplified_display()
                        );
                        let _ = fs_err::remove_file(cache_entry.path());
                    }
                }
            }
        }

        // Otherwise, run the Python script.
        debug!("Probing interpreter info for: {}", executable.display());
        let info = Self::query(executable, cache)?;
        debug!(
            "Found Python {} for: {}",
            info.markers.python_full_version,
            executable.display()
        );

        // If `executable` is a pyenv shim, a bash script that redirects to the activated
        // python executable at another path, we're not allowed to cache the interpreter info.
        if same_file::is_same_file(executable, &info.sys_executable).unwrap_or(false) {
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

#[cfg(unix)]
#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use fs_err as fs;
    use indoc::{formatdoc, indoc};
    use tempfile::tempdir;

    use pep440_rs::Version;
    use uv_cache::Cache;

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
                "base_exec_prefix": "/home/ferris/.pyenv/versions/3.12.0",
                "base_prefix": "/home/ferris/.pyenv/versions/3.12.0",
                "prefix": "/home/ferris/projects/uv/.venv",
                "sys_executable": "/home/ferris/projects/uv/.venv/bin/python",
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
                }
            }
        "##};

        let cache = Cache::temp().unwrap();

        fs::write(
            &mocked_interpreter,
            formatdoc! {r##"
            #!/bin/bash
            echo '{json}'
            "##},
        )
        .unwrap();
        fs::set_permissions(
            &mocked_interpreter,
            std::os::unix::fs::PermissionsExt::from_mode(0o770),
        )
        .unwrap();
        let interpreter = Interpreter::query(&mocked_interpreter, &cache).unwrap();
        assert_eq!(
            interpreter.markers.python_version.version,
            Version::from_str("3.12").unwrap()
        );
        fs::write(
            &mocked_interpreter,
            formatdoc! {r##"
            #!/bin/bash
            echo '{}'
            "##, json.replace("3.12", "3.13")},
        )
        .unwrap();
        let interpreter = Interpreter::query(&mocked_interpreter, &cache).unwrap();
        assert_eq!(
            interpreter.markers.python_version.version,
            Version::from_str("3.13").unwrap()
        );
    }
}
