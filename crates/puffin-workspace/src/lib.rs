use std::path::{Path, PathBuf};

pub use error::WorkspaceError;
pub use verbatim::VerbatimRequirement;
pub use workspace::Workspace;

mod error;
mod toml;
mod verbatim;
mod workspace;

/// Find the closest `pyproject.toml` file to the given path.
pub fn find_pyproject_toml(path: impl AsRef<Path>) -> Option<PathBuf> {
    for directory in path.as_ref().ancestors() {
        let pyproject_toml = directory.join("pyproject.toml");
        if pyproject_toml.is_file() {
            return Some(pyproject_toml);
        }
    }
    None
}
