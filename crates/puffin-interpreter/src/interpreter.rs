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
use puffin_cache::{Cache, CacheBucket, CachedByTimestamp};
use puffin_fs::write_atomic_sync;

use crate::python_platform::PythonPlatform;
use crate::virtual_env::detect_virtual_env;
use crate::{Error, PythonVersion};

/// A Python executable and its associated platform markers.
#[derive(Debug, Clone)]
pub struct Interpreter {
    pub(crate) platform: PythonPlatform,
    pub(crate) markers: MarkerEnvironment,
    pub(crate) base_exec_prefix: PathBuf,
    pub(crate) base_prefix: PathBuf,
    pub(crate) sys_executable: PathBuf,
    tags: OnceCell<Tags>,
}

impl Interpreter {
    /// Detect the interpreter info for the given Python executable.
    pub fn query(executable: &Path, platform: Platform, cache: &Cache) -> Result<Self, Error> {
        let info = InterpreterQueryResult::query_cached(executable, cache)?;
        debug_assert!(
            info.base_prefix == info.base_exec_prefix,
            "Not a virtual environment (Python: {}, prefix: {})",
            executable.display(),
            info.base_prefix.display()
        );
        debug_assert!(
            info.sys_executable.is_absolute(),
            "`sys.executable` is not an absolute python; Python installation is broken: {}",
            info.sys_executable.display()
        );

        Ok(Self {
            platform: PythonPlatform(platform),
            markers: info.markers,
            base_exec_prefix: info.base_exec_prefix,
            base_prefix: info.base_prefix,
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
    ) -> Self {
        Self {
            platform: PythonPlatform(platform),
            markers,
            base_exec_prefix,
            base_prefix,
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

    /// Detect the python interpreter to use.
    ///
    /// Note that `python_version` is a preference here, not a requirement.
    ///
    /// We check, in order:
    /// * `VIRTUAL_ENV` and `CONDA_PREFIX`
    /// * A `.venv` folder
    /// * If a python version is given: `pythonx.y` (TODO(konstin): `py -x.y` on windows),
    /// * `python3` (unix) or `python.exe` (windows)
    pub fn find(
        python_version: Option<&PythonVersion>,
        platform: Platform,
        cache: &Cache,
    ) -> Result<Self, Error> {
        let platform = PythonPlatform::from(platform);
        if let Some(venv) = detect_virtual_env(&platform)? {
            let executable = platform.venv_python(venv);
            let interpreter = Self::query(&executable, platform.0, cache)?;
            return Ok(interpreter);
        };

        #[cfg(unix)]
        {
            if let Some(python_version) = python_version {
                let requested = format!(
                    "python{}.{}",
                    python_version.major(),
                    python_version.minor()
                );
                if let Ok(executable) = which::which(&requested) {
                    debug!("Resolved {requested} to {}", executable.display());
                    let interpreter = Interpreter::query(&executable, platform.0, cache)?;
                    return Ok(interpreter);
                }
            }

            let executable = which::which("python3")
                .map_err(|err| Error::WhichNotFound("python3".to_string(), err))?;
            debug!("Resolved python3 to {}", executable.display());
            let interpreter = Interpreter::query(&executable, platform.0, cache)?;
            Ok(interpreter)
        }

        #[cfg(windows)]
        {
            if let Some(_python_version) = python_version {
                unimplemented!("Implement me")
            }

            let executable = which::which("python.exe")
                .map_err(|err| Error::WhichNotFound("python.exe".to_string(), err))?;
            let interpreter = Interpreter::query(&executable, platform.0, cache)?;
            Ok(interpreter)
        }

        #[cfg(not(any(unix, windows)))]
        {
            compile_error!("only unix (like mac and linux) and windows are supported")
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
    pub fn sys_executable(&self) -> &Path {
        &self.sys_executable
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub(crate) struct InterpreterQueryResult {
    pub(crate) markers: MarkerEnvironment,
    pub(crate) base_exec_prefix: PathBuf,
    pub(crate) base_prefix: PathBuf,
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
                    "Querying python at {} failed with status {}",
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
                    "Querying python at {} did not return the expected data: {err}",
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

        // `modified()` is infallible on windows and unix (i.e., all platforms we support).
        let modified = fs_err::metadata(executable)?.modified()?;

        // Read from the cache.
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

        // Otherwise, run the Python script.
        debug!("Detecting markers for: {}", executable.display());
        let info = Self::query(executable)?;

        // If `executable` is a pyenv shim, a bash script that redirects to the activated
        // python executable at another path, we're not allowed to cache the interpreter info
        if executable == info.sys_executable {
            fs::create_dir_all(cache_entry.dir())?;
            // Write to the cache.
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

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use fs_err as fs;
    use indoc::{formatdoc, indoc};
    use tempfile::tempdir;

    use pep440_rs::Version;
    use platform_host::Platform;
    use puffin_cache::Cache;

    use crate::Interpreter;

    #[test]
    #[cfg(unix)]
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
                "sys_executable": "/home/ferris/projects/puffin/.venv/bin/python"
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
        let interpreter =
            Interpreter::query(&mocked_interpreter, platform.clone(), &cache).unwrap();
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
        let interpreter = Interpreter::query(&mocked_interpreter, platform, &cache).unwrap();
        assert_eq!(
            interpreter.markers.python_version.version,
            Version::from_str("3.13").unwrap()
        );
    }
}
