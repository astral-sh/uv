use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use pep440_rs::Version;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::debug;

use crate::python_platform::PythonPlatform;
use pep508_rs::MarkerEnvironment;
use platform_host::Platform;

/// A Python executable and its associated platform markers.
#[derive(Debug, Clone)]
pub struct InterpreterInfo {
    pub(crate) platform: PythonPlatform,
    pub(crate) markers: MarkerEnvironment,
    pub(crate) base_exec_prefix: PathBuf,
    pub(crate) base_prefix: PathBuf,
}

impl InterpreterInfo {
    pub fn query_cached(
        executable: &Path,
        platform: Platform,
        cache: Option<&Path>,
    ) -> anyhow::Result<Self> {
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
        })
    }
}

impl InterpreterInfo {
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
        &self.markers.python_version.version
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
}

#[derive(Debug, Error)]
pub(crate) enum InterpreterQueryError {
    #[error(transparent)]
    IO(#[from] io::Error),
    #[error("Failed to query python interpreter at {interpreter}")]
    PythonSubcommand {
        interpreter: PathBuf,
        #[source]
        err: io::Error,
    },
}

#[derive(Deserialize, Serialize)]
pub(crate) struct InterpreterQueryResult {
    pub(crate) markers: MarkerEnvironment,
    pub(crate) base_exec_prefix: PathBuf,
    pub(crate) base_prefix: PathBuf,
}

impl InterpreterQueryResult {
    /// Return the resolved [`InterpreterQueryResult`] for the given Python executable.
    pub(crate) fn query(interpreter: &Path) -> Result<Self, InterpreterQueryError> {
        let output = Command::new(interpreter)
            .args(["-c", include_str!("get_interpreter_info.py")])
            .output()
            .map_err(|err| InterpreterQueryError::PythonSubcommand {
                interpreter: interpreter.to_path_buf(),
                err,
            })?;

        // stderr isn't technically a criterion for success, but i don't know of any cases where there
        // should be stderr output and if there is, we want to know
        if !output.status.success() || !output.stderr.is_empty() {
            return Err(InterpreterQueryError::PythonSubcommand {
                interpreter: interpreter.to_path_buf(),
                err: io::Error::new(
                    io::ErrorKind::Other,
                    format!(
                        "Querying python at {} failed with status {}:\n--- stdout:\n{}\n--- stderr:\n{}",
                        interpreter.display(),
                        output.status,
                        String::from_utf8_lossy(&output.stdout).trim(),
                        String::from_utf8_lossy(&output.stderr).trim()
                    ),
                )
            });
        }
        let data = serde_json::from_slice::<Self>(&output.stdout).map_err(|err|
            InterpreterQueryError::PythonSubcommand {
                interpreter: interpreter.to_path_buf(),
                err: io::Error::new(
                    io::ErrorKind::Other,
                    format!(
                        "Querying python at {} did not return the expected data ({}):\n--- stdout:\n{}\n--- stderr:\n{}",
                        interpreter.display(),
                        err,
                        String::from_utf8_lossy(&output.stdout).trim(),
                        String::from_utf8_lossy(&output.stderr).trim()
                    )
                )
            }
        )?;

        Ok(data)
    }

    /// A wrapper around [`markers::query_interpreter_info`] to cache the computed markers.
    ///
    /// Running a Python script is (relatively) expensive, and the markers won't change
    /// unless the Python executable changes, so we use the executable's last modified
    /// time as a cache key.
    pub(crate) fn query_cached(executable: &Path, cache: Option<&Path>) -> anyhow::Result<Self> {
        // Read from the cache.
        let key = if let Some(cache) = cache {
            if let Ok(key) = cache_key(executable) {
                if let Ok(data) = cacache::read_sync(cache, &key) {
                    if let Ok(info) = serde_json::from_slice::<Self>(&data) {
                        debug!("Using cached markers for {}", executable.display());
                        return Ok(info);
                    }
                }
                Some(key)
            } else {
                None
            }
        } else {
            None
        };

        // Otherwise, run the Python script.
        debug!("Detecting markers for {}", executable.display());
        let info = Self::query(executable)?;

        // Write to the cache.
        if let Some(cache) = cache {
            if let Some(key) = key {
                cacache::write_sync(cache, key, serde_json::to_vec(&info)?)?;
            }
        }

        Ok(info)
    }
}

/// Create a cache key for the Python executable, consisting of the executable's
/// last modified time and the executable's path.
fn cache_key(executable: &Path) -> anyhow::Result<String> {
    let modified = executable
        .metadata()?
        .modified()?
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis();
    Ok(format!("puffin:v0:{}:{}", executable.display(), modified))
}
