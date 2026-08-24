use std::path::{Path, PathBuf};

use uv_cache_key::{CanonicalUrl, cache_digest};
use uv_distribution_types::IndexUrl;
use uv_redacted::DisplaySafeUrl;

/// Cache wheels and their metadata, both from remote wheels and built from source distributions.
#[derive(Debug, Clone)]
pub enum WheelCache<'a> {
    /// Either PyPI or an alternative index, which we key by index URL.
    Index(&'a IndexUrl),
    /// A direct URL dependency, which we key by URL.
    Url(&'a DisplaySafeUrl),
    /// A path dependency, which we key by URL.
    Path(&'a DisplaySafeUrl),
    /// An editable dependency, which we key by URL.
    Editable(&'a DisplaySafeUrl),
    /// A Git dependency, which we key by URL (including LFS state), SHA.
    ///
    /// Note that this variant only exists for source distributions; wheels can't be delivered
    /// through Git.
    Git(&'a DisplaySafeUrl, &'a str),
}

impl WheelCache<'_> {
    /// The root directory for a cache bucket.
    pub fn root(&self) -> PathBuf {
        match self {
            Self::Index(IndexUrl::Pypi(_)) => WheelCacheKind::Pypi.root(),
            Self::Index(url) => WheelCacheKind::Index
                .root()
                .join(cache_digest(&CanonicalUrl::new(url.url().clone()))),
            Self::Url(url) => WheelCacheKind::Url
                .root()
                .join(cache_digest(&CanonicalUrl::new((*url).clone()))),
            Self::Path(url) => WheelCacheKind::Path
                .root()
                .join(cache_digest(&CanonicalUrl::new((*url).clone()))),
            Self::Editable(url) => WheelCacheKind::Editable
                .root()
                .join(cache_digest(&CanonicalUrl::new((*url).clone()))),
            Self::Git(url, sha) => WheelCacheKind::Git
                .root()
                .join(cache_digest(&CanonicalUrl::new((*url).clone())))
                .join(sha),
        }
    }

    /// A subdirectory for downloaded wheels belonging to a specific package.
    ///
    /// URL fragments do not identify different wheel bytes. Expected hashes are checked against
    /// the computed hashes in the cached archive, rather than being part of its location.
    pub fn wheel_dir(&self, package_name: impl AsRef<Path>) -> PathBuf {
        match self {
            Self::Url(url) => {
                let mut url = (*url).clone();
                url.set_fragment(None);
                WheelCache::Url(&url).root().join(package_name)
            }
            _ => self.root().join(package_name),
        }
    }

    /// A subdirectory for wheel metadata belonging to a specific package.
    ///
    /// Unlike downloaded archives, metadata cannot be checked against an archive's hash. Retain
    /// the URL fragment so a changed hash cannot reuse metadata for a previous artifact.
    pub fn metadata_dir(&self, package_name: impl AsRef<Path>) -> PathBuf {
        self.root().join(package_name)
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum WheelCacheKind {
    /// A cache of data from PyPI.
    Pypi,
    /// A cache of data from an alternative index.
    Index,
    /// A cache of data from an arbitrary URL.
    Url,
    /// A cache of data from a local path.
    Path,
    /// A cache of data from an editable URL.
    Editable,
    /// A cache of data from a Git repository.
    Git,
}

impl WheelCacheKind {
    fn to_str(self) -> &'static str {
        match self {
            Self::Pypi => "pypi",
            Self::Index => "index",
            Self::Url => "url",
            Self::Path => "path",
            Self::Editable => "editable",
            Self::Git => "git",
        }
    }

    fn root(self) -> PathBuf {
        Path::new(self.to_str()).to_path_buf()
    }
}

impl AsRef<Path> for WheelCacheKind {
    fn as_ref(&self) -> &Path {
        self.to_str().as_ref()
    }
}

#[cfg(test)]
mod tests {
    use uv_redacted::{DisplaySafeUrl, DisplaySafeUrlError};

    use super::WheelCache;

    #[test]
    fn archive_and_metadata_url_identity() -> Result<(), DisplaySafeUrlError> {
        let plain = DisplaySafeUrl::parse("https://example.org/pkg.whl")?;
        let hashed = DisplaySafeUrl::parse("https://example.org/pkg.whl#sha256=abc")?;
        assert_eq!(
            WheelCache::Url(&plain).wheel_dir("pkg"),
            WheelCache::Url(&hashed).wheel_dir("pkg"),
        );
        assert_ne!(
            WheelCache::Url(&plain).metadata_dir("pkg"),
            WheelCache::Url(&hashed).metadata_dir("pkg"),
        );

        let first = DisplaySafeUrl::parse("https://example.org/pkg.tar.gz#subdirectory=first")?;
        let second = DisplaySafeUrl::parse("https://example.org/pkg.tar.gz#subdirectory=second")?;
        assert_ne!(
            WheelCache::Url(&first).root(),
            WheelCache::Url(&second).root()
        );
        Ok(())
    }
}
