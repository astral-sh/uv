use std::path::{Path, PathBuf};
use tracing::debug;

use uv_fs::Simplified;
use uv_warnings::warn_user;

use crate::{Options, PyProjectToml};

/// Represents a project workspace that contains a set of options and a root path.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct Workspace {
    pub options: Options,
    pub root: PathBuf,
}

impl Workspace {
    /// Find the [`Workspace`] for the given path.
    ///
    /// The search starts at the given path and goes up the directory tree until a workspace is
    /// found.
    pub fn find(path: impl AsRef<Path>) -> Result<Option<Self>, WorkspaceError> {
        for ancestor in path.as_ref().ancestors() {
            match find_in_directory(ancestor) {
                Ok(Some(options)) => {
                    return Ok(Some(Self {
                        options,
                        root: ancestor.to_path_buf(),
                    }))
                }
                Ok(None) => {
                    // Continue traversing the directory tree.
                }
                Err(err @ WorkspaceError::PyprojectToml(..)) => {
                    // If we see an invalid `pyproject.toml`, warn but continue.
                    warn_user!("{err}");
                }
                Err(err) => {
                    // Otherwise, warn and stop.
                    return Err(err);
                }
            }
        }
        Ok(None)
    }

    /// Load a [`Workspace`] from a `pyproject.toml` or `uv.toml` file.
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, WorkspaceError> {
        Ok(Self {
            options: read_file(path.as_ref())?,
            root: path.as_ref().parent().unwrap().to_path_buf(),
        })
    }
}

/// Read a `uv.toml` or `pyproject.toml` file in the given directory.
fn find_in_directory(dir: &Path) -> Result<Option<Options>, WorkspaceError> {
    // Read a `uv.toml` file in the current directory.
    let path = dir.join("uv.toml");
    match fs_err::read_to_string(&path) {
        Ok(content) => {
            let options: Options = toml::from_str(&content)
                .map_err(|err| WorkspaceError::UvToml(path.user_display().to_string(), err))?;

            debug!("Found workspace configuration at `{}`", path.display());
            return Ok(Some(options));
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => return Err(err.into()),
    }

    // Read a `pyproject.toml` file in the current directory.
    let path = dir.join("pyproject.toml");
    match fs_err::read_to_string(&path) {
        Ok(content) => {
            // Parse, but skip any `pyproject.toml` that doesn't have a `[tool.uv]` section.
            let pyproject: PyProjectToml = toml::from_str(&content).map_err(|err| {
                WorkspaceError::PyprojectToml(path.user_display().to_string(), err)
            })?;
            let Some(tool) = pyproject.tool else {
                debug!(
                    "Skipping `pyproject.toml` in `{}` (no `[tool]` section)",
                    dir.display()
                );
                return Ok(None);
            };
            let Some(options) = tool.uv else {
                debug!(
                    "Skipping `pyproject.toml` in `{}` (no `[tool.uv]` section)",
                    dir.display()
                );
                return Ok(None);
            };

            debug!("Found workspace configuration at `{}`", path.display());
            return Ok(Some(options));
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => return Err(err.into()),
    }

    Ok(None)
}

/// Load [`Options`] from a `pyproject.toml` or `ruff.toml` file.
fn read_file(path: &Path) -> Result<Options, WorkspaceError> {
    let content = fs_err::read_to_string(path)?;
    if path.ends_with("pyproject.toml") {
        let pyproject: PyProjectToml = toml::from_str(&content)
            .map_err(|err| WorkspaceError::PyprojectToml(path.user_display().to_string(), err))?;
        Ok(pyproject.tool.and_then(|tool| tool.uv).unwrap_or_default())
    } else {
        let options: Options = toml::from_str(&content)
            .map_err(|err| WorkspaceError::UvToml(path.user_display().to_string(), err))?;
        Ok(options)
    }
}

#[derive(thiserror::Error, Debug)]
pub enum WorkspaceError {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error("Failed to parse `{0}`")]
    PyprojectToml(String, #[source] toml::de::Error),

    #[error("Failed to parse `{0}`")]
    UvToml(String, #[source] toml::de::Error),
}
