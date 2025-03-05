use std::borrow::Cow;
use std::{
    env, io,
    path::{Path, PathBuf},
};

use fs_err as fs;
use thiserror::Error;

use uv_pypi_types::Scheme;
use uv_static::EnvVars;

/// The layout of a virtual environment.
#[derive(Debug)]
pub struct VirtualEnvironment {
    /// The absolute path to the root of the virtualenv, e.g., `/path/to/.venv`.
    pub root: PathBuf,

    /// The path to the Python interpreter inside the virtualenv, e.g., `.venv/bin/python`
    /// (Unix, Python 3.11).
    pub executable: PathBuf,

    /// The path to the base executable for the environment, within the `home` directory.
    pub base_executable: PathBuf,

    /// The [`Scheme`] paths for the virtualenv, as returned by (e.g.) `sysconfig.get_paths()`.
    pub scheme: Scheme,
}

/// A parsed `pyvenv.cfg`
#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct PyVenvConfiguration {
    /// Was the virtual environment created with the `virtualenv` package?
    pub(crate) virtualenv: bool,
    /// Was the virtual environment created with the `uv` package?
    pub(crate) uv: bool,
    /// Is the virtual environment relocatable?
    pub(crate) relocatable: bool,
    /// Was the virtual environment populated with seed packages?
    pub(crate) seed: bool,
    /// Should the virtual environment include system site packages?
    pub(crate) include_system_site_packages: bool,
}

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("Broken virtual environment `{0}`: `pyvenv.cfg` is missing")]
    MissingPyVenvCfg(PathBuf),
    #[error("Broken virtual environment `{0}`: `pyvenv.cfg` could not be parsed")]
    ParsePyVenvCfg(PathBuf, #[source] io::Error),
}

/// Locate an active virtual environment by inspecting environment variables.
///
/// Supports `VIRTUAL_ENV`.
pub(crate) fn virtualenv_from_env() -> Option<PathBuf> {
    if let Some(dir) = env::var_os(EnvVars::VIRTUAL_ENV).filter(|value| !value.is_empty()) {
        return Some(PathBuf::from(dir));
    }

    None
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub(crate) enum CondaEnvironmentKind {
    /// The base Conda environment; treated like a system Python environment.
    Base,
    /// Any other Conda environment; treated like a virtual environment.
    Child,
}

impl CondaEnvironmentKind {
    /// Whether the given `CONDA_PREFIX` path is the base Conda environment.
    ///
    /// When the base environment is used, `CONDA_DEFAULT_ENV` will be set to a name, i.e., `base` or
    /// `root` which does not match the prefix, e.g. `/usr/local` instead of
    /// `/usr/local/conda/envs/<name>`.
    fn from_prefix_path(path: &Path) -> Self {
        // If we cannot read `CONDA_DEFAULT_ENV`, there's no way to know if the base environment
        let Ok(default_env) = env::var(EnvVars::CONDA_DEFAULT_ENV) else {
            return CondaEnvironmentKind::Child;
        };

        // These are the expected names for the base environment
        if default_env != "base" && default_env != "root" {
            return CondaEnvironmentKind::Child;
        }

        let Some(name) = path.file_name() else {
            return CondaEnvironmentKind::Child;
        };

        if name.to_str().is_some_and(|name| name == default_env) {
            CondaEnvironmentKind::Base
        } else {
            CondaEnvironmentKind::Child
        }
    }
}

/// Locate an active conda environment by inspecting environment variables.
///
/// If `base` is true, the active environment must be the base environment or `None` is returned,
/// and vice-versa.
pub(crate) fn conda_environment_from_env(kind: CondaEnvironmentKind) -> Option<PathBuf> {
    let dir = env::var_os(EnvVars::CONDA_PREFIX).filter(|value| !value.is_empty())?;
    let path = PathBuf::from(dir);

    if kind != CondaEnvironmentKind::from_prefix_path(&path) {
        return None;
    };

    Some(path)
}

/// Locate a virtual environment by searching the file system.
///
/// Searches for a `.venv` directory in the current or any parent directory. If the current
/// directory is itself a virtual environment (or a subdirectory of a virtual environment), the
/// containing virtual environment is returned.
pub(crate) fn virtualenv_from_working_dir() -> Result<Option<PathBuf>, Error> {
    let current_dir = crate::current_dir()?;

    for dir in current_dir.ancestors() {
        // If we're _within_ a virtualenv, return it.
        if dir.join("pyvenv.cfg").is_file() {
            return Ok(Some(dir.to_path_buf()));
        }

        // Otherwise, search for a `.venv` directory.
        let dot_venv = dir.join(".venv");
        if dot_venv.is_dir() {
            if !dot_venv.join("pyvenv.cfg").is_file() {
                return Err(Error::MissingPyVenvCfg(dot_venv));
            }
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
        let default_executable = venv.join("Scripts").join("python.exe");
        if default_executable.exists() {
            return default_executable;
        }

        // Apparently, Python installed via msys2 on Windows _might_ produce a POSIX-like layout.
        // See: https://github.com/PyO3/maturin/issues/1108
        let executable = venv.join("bin").join("python.exe");
        if executable.exists() {
            return executable;
        }

        // Fallback for Conda environments.
        let executable = venv.join("python.exe");
        if executable.exists() {
            return executable;
        }

        // If none of these exist, return the standard location
        default_executable
    } else {
        // Check for both `python3` over `python`, preferring the more specific one
        let default_executable = venv.join("bin").join("python3");
        if default_executable.exists() {
            return default_executable;
        }

        let executable = venv.join("bin").join("python");
        if executable.exists() {
            return executable;
        }

        // If none of these exist, return the standard location
        default_executable
    }
}

impl PyVenvConfiguration {
    /// Parse a `pyvenv.cfg` file into a [`PyVenvConfiguration`].
    pub fn parse(cfg: impl AsRef<Path>) -> Result<Self, Error> {
        let mut virtualenv = false;
        let mut uv = false;
        let mut relocatable = false;
        let mut seed = false;
        let mut include_system_site_packages = true;

        // Per https://snarky.ca/how-virtual-environments-work/, the `pyvenv.cfg` file is not a
        // valid INI file, and is instead expected to be parsed by partitioning each line on the
        // first equals sign.
        let content = fs::read_to_string(&cfg)
            .map_err(|err| Error::ParsePyVenvCfg(cfg.as_ref().to_path_buf(), err))?;
        for line in content.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            match key.trim() {
                "virtualenv" => {
                    virtualenv = true;
                }
                "uv" => {
                    uv = true;
                }
                "relocatable" => {
                    relocatable = value.trim().to_lowercase() == "true";
                }
                "seed" => {
                    seed = value.trim().to_lowercase() == "true";
                }
                "include-system-site-packages" => {
                    include_system_site_packages = value.trim().to_lowercase() == "true";
                }
                _ => {}
            }
        }

        Ok(Self {
            virtualenv,
            uv,
            relocatable,
            seed,
            include_system_site_packages,
        })
    }

    /// Returns true if the virtual environment was created with the `virtualenv` package.
    pub fn is_virtualenv(&self) -> bool {
        self.virtualenv
    }

    /// Returns true if the virtual environment was created with the uv package.
    pub fn is_uv(&self) -> bool {
        self.uv
    }

    /// Returns true if the virtual environment is relocatable.
    pub fn is_relocatable(&self) -> bool {
        self.relocatable
    }

    /// Returns true if the virtual environment was populated with seed packages.
    pub fn is_seed(&self) -> bool {
        self.seed
    }

    /// Returns true if the virtual environment should include system site packages.
    pub fn include_system_site_packages(&self) -> bool {
        self.include_system_site_packages
    }

    /// Set the key-value pair in the `pyvenv.cfg` file.
    pub fn set(content: &str, key: &str, value: &str) -> String {
        let mut lines = content.lines().map(Cow::Borrowed).collect::<Vec<_>>();
        let mut found = false;
        for line in &mut lines {
            if let Some((lhs, _)) = line.split_once('=') {
                if lhs.trim() == key {
                    *line = Cow::Owned(format!("{key} = {value}"));
                    found = true;
                    break;
                }
            }
        }
        if !found {
            lines.push(Cow::Owned(format!("{key} = {value}")));
        }
        if lines.is_empty() {
            String::new()
        } else {
            format!("{}\n", lines.join("\n"))
        }
    }
}

#[cfg(test)]
mod tests {
    use indoc::indoc;

    use super::*;

    #[test]
    fn test_set_existing_key() {
        let content = indoc! {"
            home = /path/to/python
            version = 3.8.0
            include-system-site-packages = false
        "};
        let result = PyVenvConfiguration::set(content, "version", "3.9.0");
        assert_eq!(
            result,
            indoc! {"
                home = /path/to/python
                version = 3.9.0
                include-system-site-packages = false
            "}
        );
    }

    #[test]
    fn test_set_new_key() {
        let content = indoc! {"
            home = /path/to/python
            version = 3.8.0
        "};
        let result = PyVenvConfiguration::set(content, "include-system-site-packages", "false");
        assert_eq!(
            result,
            indoc! {"
                home = /path/to/python
                version = 3.8.0
                include-system-site-packages = false
            "}
        );
    }

    #[test]
    fn test_set_key_no_spaces() {
        let content = indoc! {"
            home=/path/to/python
            version=3.8.0
        "};
        let result = PyVenvConfiguration::set(content, "include-system-site-packages", "false");
        assert_eq!(
            result,
            indoc! {"
                home=/path/to/python
                version=3.8.0
                include-system-site-packages = false
            "}
        );
    }

    #[test]
    fn test_set_key_prefix() {
        let content = indoc! {"
            home = /path/to/python
            home_dir = /other/path
        "};
        let result = PyVenvConfiguration::set(content, "home", "new/path");
        assert_eq!(
            result,
            indoc! {"
                home = new/path
                home_dir = /other/path
            "}
        );
    }

    #[test]
    fn test_set_empty_content() {
        let content = "";
        let result = PyVenvConfiguration::set(content, "version", "3.9.0");
        assert_eq!(
            result,
            indoc! {"
                version = 3.9.0
            "}
        );
    }
}
