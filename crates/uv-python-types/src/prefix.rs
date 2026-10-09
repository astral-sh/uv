use std::path::{Path, PathBuf};

/// A `--prefix` directory into which packages can be installed, separate from a virtual environment
/// or system Python interpreter.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Prefix(PathBuf);

impl Prefix {
    /// Return the path to the `--prefix` directory.
    pub fn root(&self) -> &Path {
        &self.0
    }
}

impl From<PathBuf> for Prefix {
    fn from(path: PathBuf) -> Self {
        Self(path)
    }
}
