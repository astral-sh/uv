use std::fmt::{Debug, Formatter};
use std::hash::{Hash, Hasher};
use std::ops::Deref;

use url::Url;

use crate::cache_key::{CacheKey, CacheKeyHasher};

/// A wrapper around `Url` which represents a "canonical" version of an original URL.
///
/// A "canonical" url is only intended for internal comparison purposes. It's to help paper over
/// mistakes such as depending on `github.com/foo/bar` vs. `github.com/foo/bar.git`.
///
/// This is **only** for internal purposes and provides no means to actually read the underlying
/// string value of the `Url` it contains. This is intentional, because all fetching should still
/// happen within the context of the original URL.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct CanonicalUrl(Url);

impl CanonicalUrl {
    pub fn new(url: &Url) -> CanonicalUrl {
        let mut url = url.clone();

        // Strip a trailing slash.
        if url.path().ends_with('/') {
            url.path_segments_mut().unwrap().pop_if_empty();
        }

        // For GitHub URLs specifically, just lower-case everything. GitHub
        // treats both the same, but they hash differently, and we're gonna be
        // hashing them. This wants a more general solution, and also we're
        // almost certainly not using the same case conversion rules that GitHub
        // does. (See issue #84)
        if url.host_str() == Some("github.com") {
            url.set_scheme(url.scheme().to_lowercase().as_str())
                .unwrap();
            let path = url.path().to_lowercase();
            url.set_path(&path);
        }

        // Repos can generally be accessed with or without `.git` extension.
        if let Some((prefix, suffix)) = url.path().rsplit_once('@') {
            // Ex) `git+https://github.com/pypa/sample-namespace-packages.git@2.0.0`
            let needs_chopping = std::path::Path::new(prefix)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("git"));
            if needs_chopping {
                let prefix = &prefix[..prefix.len() - 4];
                url.set_path(&format!("{prefix}@{suffix}"));
            }
        } else {
            // Ex) `git+https://github.com/pypa/sample-namespace-packages.git`
            let needs_chopping = std::path::Path::new(url.path())
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("git"));
            if needs_chopping {
                let last = {
                    let last = url.path_segments().unwrap().next_back().unwrap();
                    last[..last.len() - 4].to_owned()
                };
                url.path_segments_mut().unwrap().pop().push(&last);
            }
        }

        CanonicalUrl(url)
    }

    pub fn parse(url: &str) -> Result<Self, url::ParseError> {
        Ok(Self::new(&Url::parse(url)?))
    }
}

impl CacheKey for CanonicalUrl {
    fn cache_key(&self, state: &mut CacheKeyHasher) {
        // `as_str` gives the serialisation of a url (which has a spec) and so insulates against
        // possible changes in how the URL crate does hashing.
        self.0.as_str().cache_key(state);
    }
}

impl Hash for CanonicalUrl {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // `as_str` gives the serialisation of a url (which has a spec) and so insulates against
        // possible changes in how the URL crate does hashing.
        self.0.as_str().hash(state);
    }
}

impl std::fmt::Display for CanonicalUrl {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
    }
}

/// Like [`CanonicalUrl`], but attempts to represent an underlying source repository, abstracting
/// away details like the specific commit or branch, or the subdirectory to build within the
/// repository.
///
/// For example, `https://github.com/pypa/package.git#subdirectory=pkg_a` and
/// `https://github.com/pypa/package.git#subdirectory=pkg_b` would map to different
/// [`CanonicalUrl`] values, but the same [`RepositoryUrl`], since they map to the same
/// resource.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct RepositoryUrl(Url);

impl RepositoryUrl {
    pub fn new(url: &Url) -> RepositoryUrl {
        let mut url = CanonicalUrl::new(url).0;

        // If a Git URL ends in a reference (like a branch, tag, or commit), remove it.
        if url.scheme().starts_with("git+") {
            if let Some((prefix, _)) = url.as_str().rsplit_once('@') {
                url = prefix.parse().unwrap();
            }
        }

        // Drop any fragments and query parameters.
        url.set_fragment(None);
        url.set_query(None);

        RepositoryUrl(url)
    }

    pub fn parse(url: &str) -> Result<Self, url::ParseError> {
        Ok(Self::new(&Url::parse(url)?))
    }
}

impl CacheKey for RepositoryUrl {
    fn cache_key(&self, state: &mut CacheKeyHasher) {
        // `as_str` gives the serialisation of a url (which has a spec) and so insulates against
        // possible changes in how the URL crate does hashing.
        self.0.as_str().cache_key(state);
    }
}

impl Hash for RepositoryUrl {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // `as_str` gives the serialisation of a url (which has a spec) and so insulates against
        // possible changes in how the URL crate does hashing.
        self.0.as_str().hash(state);
    }
}

impl Deref for RepositoryUrl {
    type Target = Url;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_url() -> Result<(), url::ParseError> {
        // Two URLs should be considered equal regardless of the `.git` suffix.
        assert_eq!(
            CanonicalUrl::parse("git+https://github.com/pypa/sample-namespace-packages.git")?,
            CanonicalUrl::parse("git+https://github.com/pypa/sample-namespace-packages")?,
        );

        // Two URLs should be considered equal regardless of the `.git` suffix.
        assert_eq!(
            CanonicalUrl::parse("git+https://github.com/pypa/sample-namespace-packages.git@2.0.0")?,
            CanonicalUrl::parse("git+https://github.com/pypa/sample-namespace-packages@2.0.0")?,
        );

        // Two URLs should be _not_ considered equal if they point to different repositories.
        assert_ne!(
            CanonicalUrl::parse("git+https://github.com/pypa/sample-namespace-packages.git")?,
            CanonicalUrl::parse("git+https://github.com/pypa/sample-packages.git")?,
        );

        // Two URLs should _not_ be considered equal if they request different subdirectories.
        assert_ne!(
            CanonicalUrl::parse("git+https://github.com/pypa/sample-namespace-packages.git#subdirectory=pkg_resources/pkg_a")?,
            CanonicalUrl::parse("git+https://github.com/pypa/sample-namespace-packages.git#subdirectory=pkg_resources/pkg_b")?,
        );

        // Two URLs should _not_ be considered equal if they request different commit tags.
        assert_ne!(
            CanonicalUrl::parse(
                "git+https://github.com/pypa/sample-namespace-packages.git@v1.0.0"
            )?,
            CanonicalUrl::parse(
                "git+https://github.com/pypa/sample-namespace-packages.git@v2.0.0"
            )?,
        );

        Ok(())
    }

    #[test]
    fn repository_url() -> Result<(), url::ParseError> {
        // Two URLs should be considered equal regardless of the `.git` suffix.
        assert_eq!(
            RepositoryUrl::parse("git+https://github.com/pypa/sample-namespace-packages.git")?,
            RepositoryUrl::parse("git+https://github.com/pypa/sample-namespace-packages")?,
        );

        // Two URLs should be considered equal regardless of the `.git` suffix.
        assert_eq!(
            RepositoryUrl::parse(
                "git+https://github.com/pypa/sample-namespace-packages.git@2.0.0"
            )?,
            RepositoryUrl::parse("git+https://github.com/pypa/sample-namespace-packages@2.0.0")?,
        );

        // Two URLs should be _not_ considered equal if they point to different repositories.
        assert_ne!(
            RepositoryUrl::parse("git+https://github.com/pypa/sample-namespace-packages.git")?,
            RepositoryUrl::parse("git+https://github.com/pypa/sample-packages.git")?,
        );

        // Two URLs should be considered equal if they map to the same repository, even if they
        // request different subdirectories.
        assert_eq!(
            RepositoryUrl::parse("git+https://github.com/pypa/sample-namespace-packages.git#subdirectory=pkg_resources/pkg_a")?,
            RepositoryUrl::parse("git+https://github.com/pypa/sample-namespace-packages.git#subdirectory=pkg_resources/pkg_b")?,
        );

        // Two URLs should be considered equal if they map to the same repository, even if they
        // request different commit tags.
        assert_eq!(
            RepositoryUrl::parse(
                "git+https://github.com/pypa/sample-namespace-packages.git@v1.0.0"
            )?,
            RepositoryUrl::parse(
                "git+https://github.com/pypa/sample-namespace-packages.git@v2.0.0"
            )?,
        );

        Ok(())
    }
}
