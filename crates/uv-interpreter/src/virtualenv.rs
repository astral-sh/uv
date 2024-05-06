use std::{
    env, io,
    path::{Path, PathBuf},
};

use fs_err as fs;
use pypi_types::Scheme;
use thiserror::Error;
use tracing::{debug, info};

/// The layout of a virtual environment.
#[derive(Debug)]
pub struct VirtualEnvironment {
    /// The absolute path to the root of the virtualenv, e.g., `/path/to/.venv`.
    pub root: PathBuf,

    /// The path to the Python interpreter inside the virtualenv, e.g., `.venv/bin/python`
    /// (Unix, Python 3.11).
    pub executable: PathBuf,

    /// The [`Scheme`] paths for the virtualenv, as returned by (e.g.) `sysconfig.get_paths()`.
    pub scheme: Scheme,
}

/// A parsed `pyvenv.cfg`
#[derive(Debug, Clone)]
pub struct PyVenvConfiguration {
    /// If the `virtualenv` package was used to create the virtual environment.
    pub(crate) virtualenv: bool,
    /// If the `uv` package was used to create the virtual environment.
    pub(crate) uv: bool,
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("Broken virtualenv `{0}`: `pyvenv.cfg` is missing")]
    MissingPyVenvCfg(PathBuf),
    #[error("Broken virtualenv `{0}`: `pyvenv.cfg` could not be parsed")]
    ParsePyVenvCfg(PathBuf, #[source] io::Error),
}

/// Locate the current virtual environment.
pub(crate) fn detect_virtualenv() -> Result<Option<PathBuf>, Error> {
    let from_env = virtualenv_from_env();
    if from_env.is_some() {
        return Ok(from_env);
    }
    virtualenv_from_working_dir()
}

/// Locate an active virtual environment by inspecting environment variables.
///
/// Supports `VIRTUAL_ENV` and `CONDA_PREFIX`.
pub(crate) fn virtualenv_from_env() -> Option<PathBuf> {
    if let Some(dir) = env::var_os("VIRTUAL_ENV").filter(|value| !value.is_empty()) {
        info!(
            "Found a virtualenv through VIRTUAL_ENV at: {}",
            Path::new(&dir).display()
        );
        return Some(PathBuf::from(dir));
    }

    if let Some(dir) = env::var_os("CONDA_PREFIX").filter(|value| !value.is_empty()) {
        info!(
            "Found a virtualenv through CONDA_PREFIX at: {}",
            Path::new(&dir).display()
        );
        return Some(PathBuf::from(dir));
    }

    None
}

/// Locate a virtual environment by searching the file system.
///
/// Searches for a `.venv` directory in the current or any parent directory. If the current
/// directory is itself a virtual environment (or a subdirectory of a virtual environment), the
/// containing virtual environment is returned.
pub(crate) fn virtualenv_from_working_dir() -> Result<Option<PathBuf>, Error> {
    let current_dir = env::current_dir().expect("Failed to detect current directory");

    for dir in current_dir.ancestors() {
        // If we're _within_ a virtualenv, return it.
        if dir.join("pyvenv.cfg").is_file() {
            debug!("Found a virtualenv at: {}", dir.display());
            return Ok(Some(dir.to_path_buf()));
        }

        // Otherwise, search for a `.venv` directory.
        let dot_venv = dir.join(".venv");
        if dot_venv.is_dir() {
            if !dot_venv.join("pyvenv.cfg").is_file() {
                return Err(Error::MissingPyVenvCfg(dot_venv));
            }
            debug!("Found a virtualenv named .venv at: {}", dot_venv.display());
            return Ok(Some(dot_venv));
        }
    }

    Ok(None)
}

/// Returns the path to the `python` executable inside a virtual environment.
pub(crate) fn virtualenv_python_executable(venv: impl AsRef<Path>) -> PathBuf {
    let venv = venv.as_ref();
    if cfg!(windows) {
        // Search for `python.exe` in the `Scripts` directory.
        let executable = venv.join("Scripts").join("python.exe");
        if executable.exists() {
            return executable;
        }

        // Apparently, Python installed via msys2 on Windows _might_ produce a POSIX-like layout.
        // See: https://github.com/PyO3/maturin/issues/1108
        let executable = venv.join("bin").join("python.exe");
        if executable.exists() {
            return executable;
        }

        // Fallback for Conda environments.
        venv.join("python.exe")
    } else {
        // Search for `python` in the `bin` directory.
        venv.join("bin").join("python")
    }
}

impl PyVenvConfiguration {
    /// Parse a `pyvenv.cfg` file into a [`PyVenvConfiguration`].
    pub fn parse(cfg: impl AsRef<Path>) -> Result<Self, Error> {
        let mut virtualenv = false;
        let mut uv = false;

        // Per https://snarky.ca/how-virtual-environments-work/, the `pyvenv.cfg` file is not a
        // valid INI file, and is instead expected to be parsed by partitioning each line on the
        // first equals sign.
        let content = fs::read_to_string(&cfg)
            .map_err(|err| Error::ParsePyVenvCfg(cfg.as_ref().to_path_buf(), err))?;
        for line in content.lines() {
            let Some((key, _value)) = line.split_once('=') else {
                continue;
            };
            match key.trim() {
                "virtualenv" => {
                    virtualenv = true;
                }
                "uv" => {
                    uv = true;
                }
                _ => {}
            }
        }

        Ok(Self { virtualenv, uv })
    }

    /// Returns true if the virtual environment was created with the `virtualenv` package.
    pub fn is_virtualenv(&self) -> bool {
        self.virtualenv
    }

    /// Returns true if the virtual environment was created with the `uv` package.
    pub fn is_uv(&self) -> bool {
        self.uv
    }
}
