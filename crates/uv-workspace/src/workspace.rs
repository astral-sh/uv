use std::path::{Path, PathBuf};

use uv_fs::Simplified;
use uv_warnings::warn_user;

use crate::{Options, PyProjectToml};

/// Represents a project workspace that contains a set of options and a root path.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct Workspace {
    options: Options,
    root: PathBuf,
}

impl Workspace {
    /// Find the [`Workspace`] for the given path.
    ///
    /// The search starts at the given path and goes up the directory tree until a workspace is
    /// found.
    pub fn find(path: impl AsRef<Path>) -> Result<Option<Self>, WorkspaceError> {
        for ancestor in path.as_ref().ancestors() {
            match read_options(ancestor) {
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
}

/// Read a `uv.toml` or `pyproject.toml` file in the given directory.
fn read_options(dir: &Path) -> Result<Option<Options>, WorkspaceError> {
    // Read a `uv.toml` file in the current directory.
    let path = dir.join("uv.toml");
    match fs_err::read_to_string(&path) {
        Ok(content) => {
            let options: Options = toml::from_str(&content)
                .map_err(|err| WorkspaceError::UvToml(path.user_display().to_string(), err))?;
            return Ok(Some(options));
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => return Err(err.into()),
    }

    // Read a `pyproject.toml` file in the current directory.
    let path = path.join("pyproject.toml");
    match fs_err::read_to_string(&path) {
        Ok(content) => {
            // Parse, but skip any `pyproject.toml` that doesn't have a `[tool.uv]` section.
            let pyproject: PyProjectToml = toml::from_str(&content).map_err(|err| {
                WorkspaceError::PyprojectToml(path.user_display().to_string(), err)
            })?;
            let Some(tool) = pyproject.tool else {
                return Ok(None);
            };
            let Some(options) = tool.uv else {
                return Ok(None);
            };
            return Ok(Some(options));
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => return Err(err.into()),
    }

    Ok(None)
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
