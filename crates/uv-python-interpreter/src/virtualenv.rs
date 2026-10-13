use std::borrow::Cow;
use std::str::FromStr;
use std::{
    io,
    path::{Path, PathBuf},
};

use fs_err as fs;
use thiserror::Error;

use uv_pypi_types::Scheme;

use uv_python_types::PythonVersion;

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
    /// The `PYTHONHOME` directory containing the base Python executable.
    pub(super) home: Option<PathBuf>,
    /// The executable selected by uv when creating the environment.
    base_executable: Option<PathBuf>,
    /// An executable recorded by Python's `venv` module or `virtualenv`.
    pub(super) executable: Option<PathBuf>,
    /// Was the virtual environment created with the `virtualenv` package?
    virtualenv: bool,
    /// Was the virtual environment created with the `uv` package?
    uv: bool,
    /// Is the virtual environment relocatable?
    relocatable: bool,
    /// Was the virtual environment populated with seed packages?
    seed: bool,
    /// Should the virtual environment include system site packages?
    include_system_site_packages: bool,
    /// The Python version the virtual environment was created with
    pub(super) version: Option<PythonVersion>,
}

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("Broken virtual environment `{0}`: missing `pyvenv.cfg`")]
    MissingPyVenvCfg(PathBuf),
    #[error("Broken virtual environment `{0}`: could not parse `pyvenv.cfg`")]
    ParsePyVenvCfg(PathBuf, #[source] io::Error),
}

/// Returns the path to the `python` executable inside a virtual environment.
pub fn virtualenv_python_executable(venv: impl AsRef<Path>) -> PathBuf {
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
        // Prefer the unversioned `python` command exposed by virtual environments.
        let default_executable = venv.join("bin").join("python");
        if default_executable.exists() {
            return default_executable;
        }

        // Fall back to `python3` for installations without an unversioned executable.
        let executable = venv.join("bin").join("python3");
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
        let mut home = None;
        let mut base_executable = None;
        let mut executable = None;
        let mut virtualenv = false;
        let mut uv = false;
        let mut relocatable = false;
        let mut seed = false;
        let mut include_system_site_packages = true;
        let mut version = None;

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
                "home" => {
                    home = Some(PathBuf::from(value.trim()));
                }
                "uv-base-executable" => {
                    base_executable = Some(PathBuf::from(value.trim()));
                }
                "executable" | "base-executable" => {
                    executable = Some(PathBuf::from(value.trim()));
                }
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
                "version" | "version_info" => {
                    version = Some(
                        PythonVersion::from_str(value.trim())
                            .map_err(|e| io::Error::new(std::io::ErrorKind::InvalidData, e))?,
                    );
                }
                _ => {}
            }
        }

        Ok(Self {
            home,
            base_executable,
            executable,
            virtualenv,
            uv,
            relocatable,
            seed,
            include_system_site_packages,
            version,
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

    /// Return uv's recorded base executable when it agrees with the environment's `home`.
    pub(crate) fn base_executable(&self) -> Option<&Path> {
        let executable = self.base_executable.as_deref()?;
        let home = self.home.as_deref()?;
        if self.uv && executable.is_absolute() && executable.parent() == Some(home) {
            Some(executable)
        } else {
            None
        }
    }

    /// Returns true if the virtual environment is relocatable.
    pub(crate) fn is_relocatable(&self) -> bool {
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
            if let Some((lhs, _)) = line.split_once('=')
                && lhs.trim() == key
            {
                *line = Cow::Owned(format!("{key} = {value}"));
                found = true;
                break;
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
