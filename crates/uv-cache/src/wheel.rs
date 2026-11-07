use std::path::{Path, PathBuf};

use uv_cache_key::{CanonicalUrl, cache_digest};
use uv_distribution_types::IndexUrl;
use uv_redacted::SafeUrl;

/// Cache wheels and their metadata, both from remote wheels and built from source distributions.
#[derive(Debug, Clone)]
pub enum WheelCache<'a> {
    /// Either PyPI or an alternative index, which we key by index URL.
    Index(&'a IndexUrl),
    /// A direct URL dependency, which we key by URL.
    Url(&'a SafeUrl),
    /// A path dependency, which we key by URL.
    Path(&'a SafeUrl),
    /// An editable dependency, which we key by URL.
    Editable(&'a SafeUrl),
    /// A Git dependency, keyed by the source URL (including subdirectory and LFS options) and
    /// the resolved SHA.
    ///
    /// Git dependencies provide source distributions. This variant caches the wheels built from them.
    Git(&'a SafeUrl, &'a str),
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
                .join(cache_digest(&CanonicalUrl::for_git_source((*url).clone())))
                .join(sha),
        }
    }

    /// A subdirectory in a bucket for wheels for a specific package.
    pub fn wheel_dir(&self, package_name: impl AsRef<Path>) -> PathBuf {
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
    use insta::assert_snapshot;
    use uv_fs::Simplified;
    use uv_redacted::SafeUrlError;

    use super::*;

    #[test]
    fn direct_wheel_cache_paths_preserve_http_path_spelling() -> Result<(), SafeUrlError> {
        let plain =
            SafeUrl::parse("https://example.com/artifacts@channel/example-1.0-py3-none-any.whl")?;
        let suffixed = SafeUrl::parse(
            "https://example.com/artifacts.git@channel/example-1.0-py3-none-any.whl",
        )?;
        let plain_root = WheelCache::Url(&plain).root();
        let suffixed_root = WheelCache::Url(&suffixed).root();
        assert_ne!(plain_root, suffixed_root);

        let escaped_at = SafeUrl::parse(
            "https://example.com/artifacts.git%40channel/example-1.0-py3-none-any.whl",
        )?;
        let escaped_dot = SafeUrl::parse(
            "https://example.com/artifacts%2Egit@channel/example-1.0-py3-none-any.whl",
        )?;
        assert_ne!(WheelCache::Url(&escaped_at).root(), suffixed_root);
        assert_ne!(WheelCache::Url(&escaped_dot).root(), suffixed_root);
        Ok(())
    }

    #[test]
    fn git_cache_path_uses_legacy_source_digest() -> Result<(), SafeUrlError> {
        let source = SafeUrl::parse(
            "git+https://github.com/PYPA/sample-namespace-packages.git@2.0.0#subdirectory=src&lfs=true",
        )?;
        let root = WheelCache::Git(&source, "c9929c576344ec58").root();
        assert_snapshot!(root.portable_display(), @"git/e1494ed3a4bc2733/c9929c576344ec58");
        Ok(())
    }
}
