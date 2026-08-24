use std::path::{Path, PathBuf};

use uv_cache_key::{CanonicalUrl, cache_digest};
use uv_distribution_types::IndexUrl;
use uv_pypi_types::HashDigest;
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
                .join(revision_digest(url.url())),
            Self::Url(url) => WheelCacheKind::Url.root().join(revision_digest(url)),
            Self::Path(url) => WheelCacheKind::Path.root().join(revision_digest(url)),
            Self::Editable(url) => WheelCacheKind::Editable.root().join(revision_digest(url)),
            Self::Git(url, sha) => WheelCacheKind::Git
                .root()
                .join(revision_digest(url))
                .join(sha),
        }
    }

    /// A subdirectory for wheels and metadata belonging to a specific package and URL revision.
    pub fn wheel_dir(&self, package_name: impl AsRef<Path>) -> PathBuf {
        self.root().join(package_name)
    }

    /// A shared cache location for a URL wheel with a computed hash.
    ///
    /// Unlike HTTP responses, downloaded archives can be shared across hash declarations by
    /// checking their computed digests. Keep unknown and source-selecting fragments in the key.
    pub fn url_hash_dir(
        url: &DisplaySafeUrl,
        package_name: impl AsRef<Path>,
        hash: &HashDigest,
    ) -> PathBuf {
        WheelCacheKind::Url
            .root()
            .join(cache_digest(&CanonicalUrl::new(url.clone())))
            .join(package_name)
            .join("hashes")
            .join(cache_digest(&hash.to_string()))
    }
}

/// Identify a URL revision, retaining hash declarations that cannot be checked against metadata.
fn revision_digest(url: &DisplaySafeUrl) -> String {
    // Metadata and source builds are not interchangeable across expected hashes. Keep a separate
    // revision for each declaration, preserving the existing on-disk keys.
    let mut revision = DisplaySafeUrl::from(CanonicalUrl::new(url.clone()));
    revision.set_fragment(url.fragment());
    cache_digest(&revision.as_str())
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
    use uv_cache_key::cache_digest;
    use uv_pypi_types::{HashAlgorithm, HashDigest};
    use uv_redacted::{DisplaySafeUrl, DisplaySafeUrlError};

    use super::{WheelCache, revision_digest};

    #[test]
    fn archive_and_metadata_url_identity() -> Result<(), DisplaySafeUrlError> {
        let plain = DisplaySafeUrl::parse("https://example.org/pkg.whl")?;
        let hashed = DisplaySafeUrl::parse("https://example.org/pkg.whl#sha256=abc")?;
        assert_ne!(
            WheelCache::Url(&plain).wheel_dir("pkg"),
            WheelCache::Url(&hashed).wheel_dir("pkg"),
        );
        assert_eq!(revision_digest(&hashed), cache_digest(&hashed.as_str()));
        let hash = HashDigest {
            algorithm: HashAlgorithm::Sha256,
            digest: "abc".into(),
        };
        assert_eq!(
            WheelCache::url_hash_dir(&plain, "pkg", &hash),
            WheelCache::url_hash_dir(&hashed, "pkg", &hash),
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
