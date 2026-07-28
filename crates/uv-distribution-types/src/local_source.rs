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
    /// Whether to prefer a relative output path when one can be constructed.
    prefer_relative: bool,
}

impl LocalSourcePath {
    /// Construct a local source that preserves an explicitly absolute path.
    #[must_use]
    pub fn new_preserving_absolute(install_path: Box<Path>, url: VerbatimUrl) -> Self {
        Self {
            prefer_relative: !url.was_given_absolute(),
            install_path,
            url,
        }
    }

    /// Construct a local source that prefers a relative path when possible.
    #[must_use]
    pub fn new_preferring_relative(install_path: Box<Path>, url: VerbatimUrl) -> Self {
        Self {
            install_path,
            url,
            prefer_relative: true,
        }
    }

    /// Return whether a relative output path is preferred when possible.
    #[must_use]
    pub fn prefer_relative(&self) -> bool {
        self.prefer_relative
    }

    /// Express this source relative to `root` when a relative path is preferred.
    pub fn relative_to(&self, root: &Path) -> io::Result<PathBuf> {
        try_relative_to_if(&self.install_path, root, self.prefer_relative)
    }
}

// Path formatting is not part of source identity. Requirements and distributions
// use these implementations for deduplication and lockfile comparison, so their
// equality, hashing, and ordering must not depend on `prefer_relative`.
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
    use std::cmp::Ordering;
    use std::collections::HashSet;
    use std::error::Error;
    use std::path::Path;

    use uv_pep508::VerbatimUrl;

    use crate::LocalSourcePath;

    #[test]
    fn constructors_select_absolute_or_relative_paths() -> Result<(), Box<dyn Error>> {
        let root = std::env::temp_dir();
        let install_path = root.join("uv-local-source");
        let url = VerbatimUrl::from_absolute_path(&install_path)?
            .with_given(install_path.to_string_lossy());
        let absolute = LocalSourcePath::new_preserving_absolute(
            install_path.clone().into_boxed_path(),
            url.clone(),
        );
        let relative =
            LocalSourcePath::new_preferring_relative(install_path.into_boxed_path(), url);

        assert_eq!(absolute.relative_to(&root)?, root.join("uv-local-source"));
        assert_eq!(relative.relative_to(&root)?, Path::new("uv-local-source"));

        Ok(())
    }

    #[test]
    fn path_preference_does_not_change_source_identity() -> Result<(), Box<dyn Error>> {
        let install_path = std::env::temp_dir().join("uv-local-source");
        let url = VerbatimUrl::from_absolute_path(&install_path)?
            .with_given(install_path.to_string_lossy());
        let absolute = LocalSourcePath::new_preserving_absolute(
            install_path.clone().into_boxed_path(),
            url.clone(),
        );
        let relative =
            LocalSourcePath::new_preferring_relative(install_path.into_boxed_path(), url);

        assert_eq!(absolute, relative);
        assert_eq!(absolute.cmp(&relative), Ordering::Equal);
        assert_eq!(HashSet::from([absolute, relative]).len(), 1);

        Ok(())
    }
}
