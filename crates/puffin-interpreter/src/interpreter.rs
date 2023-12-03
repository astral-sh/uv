use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::UNIX_EPOCH;

use fs_err as fs;
use serde::{Deserialize, Serialize};
use tracing::debug;

use pep440_rs::Version;
use pep508_rs::MarkerEnvironment;
use platform_host::Platform;
use puffin_cache::{digest, Cache, CacheBucket};

use crate::python_platform::PythonPlatform;
use crate::Error;

/// A Python executable and its associated platform markers.
#[derive(Debug, Clone)]
pub struct Interpreter {
    pub(crate) platform: PythonPlatform,
    pub(crate) markers: MarkerEnvironment,
    pub(crate) base_exec_prefix: PathBuf,
    pub(crate) base_prefix: PathBuf,
    pub(crate) sys_executable: PathBuf,
}

impl Interpreter {
    /// Detect the interpreter info for the given Python executable.
    pub fn query(executable: &Path, platform: Platform, cache: &Cache) -> Result<Self, Error> {
        let info = InterpreterQueryResult::query_cached(executable, cache)?;
        debug_assert!(
            info.base_prefix == info.base_exec_prefix,
            "Not a venv python: {}, prefix: {}",
            executable.display(),
            info.base_prefix.display()
        );

        Ok(Self {
            platform: PythonPlatform(platform),
            markers: info.markers,
            base_exec_prefix: info.base_exec_prefix,
            base_prefix: info.base_prefix,
            sys_executable: info.sys_executable,
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
        }
    }
}

impl Interpreter {
    /// Returns the path to the Python virtual environment.
    pub fn platform(&self) -> &Platform {
        &self.platform
    }

    /// Returns the [`MarkerEnvironment`] for this Python executable.
    pub fn markers(&self) -> &MarkerEnvironment {
        &self.markers
    }

    /// Returns the Python version.
    pub fn version(&self) -> &Version {
        &self.markers.python_full_version.version
    }

    /// Returns the Python version as a simple tuple.
    pub fn simple_version(&self) -> (u8, u8) {
        (
            u8::try_from(self.version().release[0]).expect("invalid major version"),
            u8::try_from(self.version().release[1]).expect("invalid minor version"),
        )
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

    /// Inject markers of a fake python version
    #[must_use]
    pub fn patch_markers(self, markers: MarkerEnvironment) -> Self {
        Self { markers, ..self }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub(crate) struct InterpreterQueryResult {
    pub(crate) markers: MarkerEnvironment,
    pub(crate) base_exec_prefix: PathBuf,
    pub(crate) base_prefix: PathBuf,
    pub(crate) sys_executable: PathBuf,
}

#[derive(Deserialize, Serialize)]
pub(crate) struct CachedByTimestamp<T> {
    pub(crate) timestamp: u128,
    pub(crate) data: T,
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
                    "Querying python at {} did not return the expected data: {}",
                    interpreter.display(),
                    err,
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
        let cache_dir = cache.bucket(CacheBucket::Interpreter);
        let cache_path = cache_dir.join(format!("{}.json", digest(&executable_bytes)));

        let modified = fs_err::metadata(executable)?
            // Note: This is infallible on windows and unix (i.e., all platforms we support).
            .modified()?
            .duration_since(UNIX_EPOCH)
            .map_err(|err| Error::SystemTime(executable.to_path_buf(), err))?;

        // Read from the cache.
        if let Ok(data) = fs::read(&cache_path) {
            if let Ok(cached) = serde_json::from_slice::<CachedByTimestamp<Self>>(&data) {
                if cached.timestamp == modified.as_millis() {
                    debug!("Using cached markers for: {}", executable.display());
                    return Ok(cached.data);
                }

                debug!(
                    "Ignoring stale cached markers for: {}",
                    executable.display()
                );
            }
        }

        // Otherwise, run the Python script.
        debug!("Detecting markers for: {}", executable.display());
        let info = Self::query(executable)?;

        // If `executable` is a pyenv shim, a bash script that redirects to the activated
        // python executable at another path, we're not allowed to cache the interpreter info
        if executable == info.sys_executable {
            fs::create_dir_all(cache_dir)?;
            // Write to the cache.
            fs::write(
                cache_path,
                serde_json::to_vec(&CachedByTimestamp {
                    timestamp: modified.as_millis(),
                    data: info.clone(),
                })?,
            )?;
        }

        Ok(info)
    }
}
