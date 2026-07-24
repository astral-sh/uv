use std::cmp::Ordering;
use std::hash::{Hash, Hasher};
use std::io;
use std::path::{Path, PathBuf};

use uv_fs::try_relative_to_if;
use uv_pep508::VerbatimUrl;

/// A local distribution's install path, original URL, and output path policy.
#[derive(Clone, Debug, Eq)]
pub struct LocalSourcePath {
    /// The absolute path used to read, build, or install the distribution.
    pub install_path: Box<Path>,
    /// The original URL spelling used when displaying the requirement.
    pub url: VerbatimUrl,
    /// Whether a lockfile should preserve an explicitly absolute source.
    pub preserve_absolute: bool,
}

impl LocalSourcePath {
    /// Construct a local source using the original URL's path spelling.
    #[must_use]
    pub fn new(install_path: Box<Path>, url: VerbatimUrl) -> Self {
        let preserve_absolute = url.was_given_absolute();
        Self {
            install_path,
            url,
            preserve_absolute,
        }
    }

    /// Express this source relative to `root`, unless its absolute spelling must be preserved.
    pub fn relative_to(&self, root: &Path) -> io::Result<PathBuf> {
        try_relative_to_if(&self.install_path, root, !self.preserve_absolute)
    }
}

impl PartialEq for LocalSourcePath {
    fn eq(&self, other: &Self) -> bool {
        self.install_path == other.install_path && self.url == other.url
    }
}

impl Hash for LocalSourcePath {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.install_path.hash(state);
        self.url.hash(state);
    }
}

impl Ord for LocalSourcePath {
    fn cmp(&self, other: &Self) -> Ordering {
        self.install_path
            .cmp(&other.install_path)
            .then_with(|| self.url.cmp(&other.url))
    }
}

impl PartialOrd for LocalSourcePath {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::hash_map::DefaultHasher;
    use std::error::Error;
    use std::hash::{Hash, Hasher};
    use uv_pep508::VerbatimUrl;

    use crate::LocalSourcePath;

    #[test]
    fn absolute_path_preservation_does_not_change_source_identity() -> Result<(), Box<dyn Error>> {
        let install_path = std::env::temp_dir().join("uv-local-source");
        let url = VerbatimUrl::from_absolute_path(&install_path)?
            .with_given(install_path.to_string_lossy());
        let absolute = LocalSourcePath::new(install_path.into_boxed_path(), url);
        let mut relative = absolute.clone();
        relative.preserve_absolute = false;

        assert_eq!(absolute, relative);
        assert_eq!(absolute.cmp(&relative), std::cmp::Ordering::Equal);

        let mut absolute_hash = DefaultHasher::new();
        absolute.hash(&mut absolute_hash);
        let mut relative_hash = DefaultHasher::new();
        relative.hash(&mut relative_hash);
        assert_eq!(absolute_hash.finish(), relative_hash.finish());

        Ok(())
    }
}
