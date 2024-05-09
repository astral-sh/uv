use std::path::{Path, PathBuf};

use tracing::debug;
use uv_fs::Simplified;

use uv_requirements::RequirementsSource;

#[derive(Debug, Clone)]
pub(crate) struct Project {
    /// The path to the `pyproject.toml` file.
    path: PathBuf,
}

impl Project {
    /// Find the current project.
    pub(crate) fn find(path: impl AsRef<Path>) -> Option<Self> {
        for ancestor in path.as_ref().ancestors() {
            let pyproject_path = ancestor.join("pyproject.toml");
            if pyproject_path.exists() {
                debug!(
                    "Loading requirements from: {}",
                    pyproject_path.user_display()
                );
                return Some(Self {
                    path: pyproject_path,
                });
            }
        }

        None
    }

    /// Return the requirements for the project.
    pub(crate) fn requirements(&self) -> Vec<RequirementsSource> {
        vec![
            RequirementsSource::from_requirements_file(self.path.clone()),
            RequirementsSource::from_source_tree(self.path.parent().unwrap().to_path_buf()),
        ]
    }
}
