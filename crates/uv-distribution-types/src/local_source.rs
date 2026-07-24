use std::io;
use std::path::{Path, PathBuf};

use uv_fs::try_relative_to_if;
use uv_pep508::VerbatimUrl;

/// A local distribution's install path and original URL.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LocalSourcePath {
    /// The absolute path used to read, build, or install the distribution.
    pub install_path: Box<Path>,
    /// The original URL spelling used when displaying the requirement.
    pub url: VerbatimUrl,
}

impl LocalSourcePath {
    /// Construct a local source that preserves an explicitly absolute path.
    #[must_use]
    pub fn new_preserving_absolute(install_path: Box<Path>, url: VerbatimUrl) -> Self {
        Self { install_path, url }
    }

    /// Express this source relative to `root`, unless its original path is absolute.
    pub fn relative_to(&self, root: &Path) -> io::Result<PathBuf> {
        try_relative_to_if(&self.install_path, root, !self.url.was_given_absolute())
    }
}
