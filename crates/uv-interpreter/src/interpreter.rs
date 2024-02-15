use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;

use fs_err as fs;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use cache_key::digest;
use pep440_rs::Version;
use pep508_rs::MarkerEnvironment;
use platform_host::Platform;
use platform_tags::{Tags, TagsError};
use uv_cache::{Cache, CacheBucket, CachedByTimestamp, Freshness, Timestamp};
use uv_fs::write_atomic_sync;

use crate::python_platform::PythonPlatform;
use crate::virtual_env::detect_virtual_env;
use crate::{find_requested_python, Error, PythonVersion};

/// A Python executable and its associated platform markers.
#[derive(Debug, Clone)]
pub struct Interpreter {
    pub(crate) platform: PythonPlatform,
    pub(crate) markers: Box<MarkerEnvironment>,
    pub(crate) base_exec_prefix: PathBuf,
    pub(crate) base_prefix: PathBuf,
    pub(crate) stdlib: PathBuf,
    pub(crate) sys_executable: PathBuf,
    tags: OnceCell<Tags>,
}

impl Interpreter {
    /// Detect the interpreter info for the given Python executable.
    pub fn query(executable: &Path, platform: &Platform, cache: &Cache) -> Result<Self, Error> {
        let info = InterpreterQueryResult::query_cached(executable, cache)?;
        debug_assert!(
            info.base_prefix == info.base_exec_prefix,
            "Not a virtualenv (Python: {}, prefix: {})",
            executable.display(),
            info.base_prefix.display()
        );
        debug_assert!(
            info.sys_executable.is_absolute(),
            "`sys.executable` is not an absolute Python; Python installation is broken: {}",
            info.sys_executable.display()
        );

        Ok(Self {
            platform: PythonPlatform(platform.to_owned()),
            markers: Box::new(info.markers),
            base_exec_prefix: info.base_exec_prefix,
            base_prefix: info.base_prefix,
            stdlib: info.stdlib,
            sys_executable: info.sys_executable,
            tags: OnceCell::new(),
        })
    }

    // TODO(konstin): Find a better way mocking the fields
    pub fn artificial(
        platform: Platform,
        markers: MarkerEnvironment,
        base_exec_prefix: PathBuf,
        base_prefix: PathBuf,
        sys_executable: PathBuf,
        stdlib: PathBuf,
    ) -> Self {
        Self {
            platform: PythonPlatform(platform),
            markers: Box::new(markers),
            base_exec_prefix,
            base_prefix,
            stdlib,
            sys_executable,
            tags: OnceCell::new(),
        }
    }

    /// Return a new [`Interpreter`] with the given base prefix.
    #[must_use]
    pub fn with_base_prefix(self, base_prefix: PathBuf) -> Self {
        Self {
            base_prefix,
            ..self
        }
    }

    /// Find the best available Python interpreter to use.
    ///
    /// If no Python version is provided, we will use the first available interpreter.
    ///
    /// If a Python version is provided, we will first try to find an exact match. If
    /// that cannot be found and a patch version was requested, we will look for a match
    /// without comparing the patch version number. If that cannot be found, we fall back to
    /// the first available version.
    ///
    /// See [`Self::find_version`] for details on the precedence of Python lookup locations.
    pub fn find_best(
        python_version: Option<&PythonVersion>,
        platform: &Platform,
        cache: &Cache,
    ) -> Result<Self, Error> {
        // First, check for an exact match (or the first available version if no Python version was provided)
        if let Some(interpreter) = Self::find_version(python_version, platform, cache)? {
            return Ok(interpreter);
        }

        if let Some(python_version) = python_version {
            // If that fails, and a specific patch version was requested try again allowing a
            // different patch version
            if python_version.patch().is_some() {
                if let Some(interpreter) =
                    Self::find_version(Some(&python_version.without_patch()), platform, cache)?
                {
                    return Ok(interpreter);
                }
            }

            // If a Python version was requested but cannot be fulfilled, just take any version
            if let Some(interpreter) = Self::find_version(None, platform, cache)? {
                return Ok(interpreter);
            }
        }

        Err(Error::PythonNotFound)
    }

    /// Find a Python interpreter.
    ///
    /// We check, in order, the following locations:
    ///
    /// - `VIRTUAL_ENV` and `CONDA_PREFIX`
    /// - A `.venv` folder
    /// - If a python version is given: `pythonx.y`
    /// - `python3` (unix) or `python.exe` (windows)
    ///
    /// If `UV_TEST_PYTHON_PATH` is set, we will not check for Python versions in the
    /// global PATH, instead we will search using the provided path. Virtual environments
    /// will still be respected.
    ///
    /// If a version is provided and an interpreter cannot be found with the given version,
    /// we will return [`None`].
    pub fn find_version(
        python_version: Option<&PythonVersion>,
        platform: &Platform,
        cache: &Cache,
    ) -> Result<Option<Self>, Error> {
        let version_matches = |interpreter: &Self| -> bool {
            if let Some(python_version) = python_version {
                // If a patch version was provided, check for an exact match
                python_version.is_satisfied_by(interpreter)
            } else {
                // The version always matches if one was not provided
                true
            }
        };

        // Check if the venv Python matches.
        let python_platform = PythonPlatform::from(platform.to_owned());
        if let Some(venv) = detect_virtual_env(&python_platform)? {
            let executable = python_platform.venv_python(venv);
            let interpreter = Self::query(&executable, &python_platform.0, cache)?;

            if version_matches(&interpreter) {
                return Ok(Some(interpreter));
            }
        };

        // Look for the requested version with by search for `python{major}.{minor}` in `PATH` on
        // Unix and `py --list-paths` on Windows.
        if let Some(python_version) = python_version {
            if let Some(interpreter) =
                find_requested_python(&python_version.string, platform, cache)?
            {
                if version_matches(&interpreter) {
                    return Ok(Some(interpreter));
                }
            }
        }

        // Python discovery failed to find the requested version, maybe the default Python in PATH
        // matches?
        if cfg!(unix) {
            if let Some(executable) = Interpreter::find_executable("python3")? {
                debug!("Resolved python3 to {}", executable.display());
                let interpreter = Interpreter::query(&executable, &python_platform.0, cache)?;
                if version_matches(&interpreter) {
                    return Ok(Some(interpreter));
                }
            }
        } else if cfg!(windows) {
            if let Some(executable) = Interpreter::find_executable("python.exe")? {
                let interpreter = Interpreter::query(&executable, &python_platform.0, cache)?;
                if version_matches(&interpreter) {
                    return Ok(Some(interpreter));
                }
            }
        } else {
            unimplemented!("Only Windows and Unix are supported");
        }

        Ok(None)
    }

    /// Find the Python interpreter in `PATH`, respecting `UV_PYTHON_PATH`.
    ///
    /// Returns `Ok(None)` if not found.
    pub fn find_executable<R: AsRef<OsStr> + Into<OsString> + Copy>(
        requested: R,
    ) -> Result<Option<PathBuf>, Error> {
        let result = if let Some(isolated) = std::env::var_os("UV_TEST_PYTHON_PATH") {
            which::which_in(requested, Some(isolated), std::env::current_dir()?)
        } else {
            which::which(requested)
        };

        match result {
            Err(which::Error::CannotFindBinaryPath) => Ok(None),
            Err(err) => Err(Error::WhichError(requested.into(), err)),
            Ok(path) => Ok(Some(path)),
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

    /// Returns the Python version.
    #[inline]
    pub const fn python_version(&self) -> &Version {
        &self.markers.python_full_version.version
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

    pub fn implementation_name(&self) -> &str {
        &self.markers.implementation_name
    }
    pub fn base_exec_prefix(&self) -> &Path {
        &self.base_exec_prefix
    }
    pub fn base_prefix(&self) -> &Path {
        &self.base_prefix
    }

    /// `sysconfig.get_path("stdlib")`
    pub fn stdlib(&self) -> &Path {
        &self.stdlib
    }
    pub fn sys_executable(&self) -> &Path {
        &self.sys_executable
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub(crate) struct InterpreterQueryResult {
    pub(crate) markers: MarkerEnvironment,
    pub(crate) base_exec_prefix: PathBuf,
    pub(crate) base_prefix: PathBuf,
    pub(crate) stdlib: PathBuf,
    pub(crate) sys_executable: PathBuf,
}

impl InterpreterQueryResult {
    /// Return the resolved [`InterpreterQueryResult`] for the given Python executable.
    pub(crate) fn query(interpreter: &Path) -> Result<Self, Error> {
        let output = Command::new(interpreter)
            .args(["-c", include_str!("get_interpreter_info.py")])
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
                stdout: String::from_utf8_lossy(&output.stdout).trim().to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            });
        }
        let data = serde_json::from_slice::<Self>(&output.stdout).map_err(|err| {
            Error::PythonSubcommandOutput {
                message: format!(
                    "Querying Python at `{}` did not return the expected data: {err}",
                    interpreter.display(),
                ),
                stdout: String::from_utf8_lossy(&output.stdout).trim().to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            }
        })?;

        Ok(data)
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

        let modified = Timestamp::from_path(fs_err::canonicalize(executable)?)?;

        // Read from the cache.
        if cache
            .freshness(&cache_entry, None)
            .is_ok_and(Freshness::is_fresh)
        {
            if let Ok(data) = fs::read(cache_entry.path()) {
                match rmp_serde::from_slice::<CachedByTimestamp<Self>>(&data) {
                    Ok(cached) => {
                        if cached.timestamp == modified {
                            debug!("Using cached markers for: {}", executable.display());
                            return Ok(cached.data);
                        }

                        debug!(
                            "Ignoring stale cached markers for: {}",
                            executable.display()
                        );
                    }
                    Err(err) => {
                        warn!(
                            "Broken cache entry at {}, removing: {err}",
                            cache_entry.path().display()
                        );
                        let _ = fs_err::remove_file(cache_entry.path());
                    }
                }
            }
        }

        // Otherwise, run the Python script.
        debug!("Detecting markers for: {}", executable.display());
        let info = Self::query(executable)?;

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
    use platform_host::Platform;
    use uv_cache::Cache;

    use crate::Interpreter;

    #[test]
    fn test_cache_invalidation() {
        let mock_dir = tempdir().unwrap();
        let mocked_interpreter = mock_dir.path().join("python");
        let json = indoc! {r##"
            {
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
                "stdlib": "/usr/lib/python3.12",
                "sys_executable": "/home/ferris/projects/uv/.venv/bin/python"
            }
        "##};

        let cache = Cache::temp().unwrap();
        let platform = Platform::current().unwrap();

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
        let interpreter = Interpreter::query(&mocked_interpreter, &platform, &cache).unwrap();
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
        let interpreter = Interpreter::query(&mocked_interpreter, &platform, &cache).unwrap();
        assert_eq!(
            interpreter.markers.python_version.version,
            Version::from_str("3.13").unwrap()
        );
    }
}
