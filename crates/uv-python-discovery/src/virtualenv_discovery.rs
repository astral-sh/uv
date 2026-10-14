use std::path::{Path, PathBuf};
use std::{env, io};

use fs_err as fs;
use uv_python_interpreter::VirtualEnvError as Error;
use uv_static::EnvVars;

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
    /// The base environment is typically stored in a location matching the `_CONDA_ROOT` path.
    ///
    /// Additionally, when the base environment is active, `CONDA_DEFAULT_ENV` will be set to a
    /// name, e.g., `base`, which does not match the `CONDA_PREFIX`, e.g., `/usr/local` instead of
    /// `/usr/local/conda/envs/<name>`. Note the name `CONDA_DEFAULT_ENV` is misleading, it's the
    /// active environment name, not a constant base environment name.
    fn from_prefix_path(path: &Path) -> Self {
        // Pixi never creates true "base" envs and names project envs "default", confusing our
        // heuristics, so treat Pixi prefixes as child envs outright.
        if is_pixi_environment(path) {
            return Self::Child;
        }

        // If `_CONDA_ROOT` is set and matches `CONDA_PREFIX`, it's the base environment.
        if let Ok(conda_root) = env::var(EnvVars::CONDA_ROOT) {
            if path == Path::new(&conda_root) {
                return Self::Base;
            }
        }

        // Next, we'll use a heuristic based on `CONDA_DEFAULT_ENV`
        let Ok(current_env) = env::var(EnvVars::CONDA_DEFAULT_ENV) else {
            return Self::Child;
        };

        // If the `CONDA_PREFIX` equals the `CONDA_DEFAULT_ENV`, we're in an unnamed environment
        // which is typical for environments created with `conda create -p /path/to/env`.
        if path == Path::new(&current_env) {
            return Self::Child;
        }

        // Use path-based logic for environment names, including `base` and `root`.
        let Some(name) = path.file_name() else {
            return Self::Child;
        };

        // If the environment is in a directory matching the name of the environment, it's not
        // usually a base environment.
        if name.to_str().is_some_and(|name| name == current_env) {
            Self::Child
        } else {
            Self::Base
        }
    }
}

/// Detect whether the current `CONDA_PREFIX` belongs to a Pixi-managed environment.
fn is_pixi_environment(path: &Path) -> bool {
    path.join("conda-meta").join("pixi").is_file()
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
    }

    Some(path)
}

/// Locate a virtual environment by searching the file system.
///
/// Searches for a `.venv` directory or symlink in the current or any parent directory. If the
/// current directory is itself a virtual environment (or a subdirectory of a virtual environment),
/// the containing virtual environment is returned.
pub(crate) fn virtualenv_from_working_dir() -> Result<Option<PathBuf>, Error> {
    let current_dir = crate::current_dir()?;

    for dir in current_dir.ancestors() {
        // If we're _within_ a virtualenv, return it.
        if uv_fs::is_virtualenv_base(dir) {
            return Ok(Some(dir.to_path_buf()));
        }

        // Otherwise, search for a `.venv` directory.
        let dot_venv = dir.join(".venv");
        let metadata = match fs::symlink_metadata(&dot_venv) {
            Ok(metadata) => metadata,
            Err(err) if err.kind() == io::ErrorKind::NotFound => continue,
            Err(err) => return Err(err.into()),
        };
        if metadata.is_dir() || metadata.file_type().is_symlink() {
            if !uv_fs::is_virtualenv_base(&dot_venv) {
                return Err(Error::MissingPyVenvCfg(dot_venv));
            }
            return Ok(Some(dot_venv));
        }
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;
    use temp_env::with_vars;
    use tempfile::tempdir;
    #[test]
    fn pixi_environment_is_treated_as_child() {
        let tempdir = tempdir().unwrap();
        let prefix = tempdir.path();
        let conda_meta = prefix.join("conda-meta");

        fs::create_dir_all(&conda_meta).unwrap();
        fs::write(conda_meta.join("pixi"), []).unwrap();

        let vars = [
            (EnvVars::CONDA_ROOT, None),
            (EnvVars::CONDA_PREFIX, Some(prefix.as_os_str())),
            (EnvVars::CONDA_DEFAULT_ENV, Some(OsStr::new("example"))),
        ];

        with_vars(vars, || {
            assert_eq!(
                CondaEnvironmentKind::from_prefix_path(prefix),
                CondaEnvironmentKind::Child
            );
        });
    }
}
