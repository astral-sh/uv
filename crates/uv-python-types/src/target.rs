use std::path::{Path, PathBuf};

/// A `--target` directory into which packages can be installed, separate from a virtual environment
/// or system Python interpreter.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Target(PathBuf);

impl Target {
    /// Return the path to the `--target` directory.
    pub fn root(&self) -> &Path {
        &self.0
    }
}

impl From<PathBuf> for Target {
    fn from(path: PathBuf) -> Self {
        Self(path)
    }
}
