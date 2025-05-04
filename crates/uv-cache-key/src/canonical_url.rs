use std::borrow::Cow;
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
    pub fn new(url: &Url) -> Self {
        let mut url = url.clone();

        // If the URL cannot be a base, then it's not a valid URL anyway.
        if url.cannot_be_a_base() {
            return Self(url);
        }

        // Strip credentials.
        let _ = url.set_password(None);
        let _ = url.set_username("");

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

        // Decode any percent-encoded characters in the path.
        if memchr::memchr(b'%', url.path().as_bytes()).is_some() {
            let decoded = url
                .path_segments()
                .unwrap()
                .map(|segment| {
                    percent_encoding::percent_decode_str(segment)
                        .decode_utf8()
                        .unwrap_or(Cow::Borrowed(segment))
                        .into_owned()
                })
                .collect::<Vec<_>>();

            let mut path_segments = url.path_segments_mut().unwrap();
            path_segments.clear();
            path_segments.extend(decoded);
        }

        Self(url)
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

impl From<CanonicalUrl> for Url {
    fn from(value: CanonicalUrl) -> Self {
        value.0
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
    pub fn new(url: &Url) -> Self {
        let mut url = CanonicalUrl::new(url).0;

        // If a Git URL ends in a reference (like a branch, tag, or commit), remove it.
        if url.scheme().starts_with("git+") {
            if let Some(prefix) = url
                .path()
                .rsplit_once('@')
                .map(|(prefix, _suffix)| prefix.to_string())
            {
                url.set_path(&prefix);
            }
        }

        // Drop any fragments and query parameters.
        url.set_fragment(None);
        url.set_query(None);

        Self(url)
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

impl std::fmt::Display for RepositoryUrl {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_credential_does_not_affect_cache_key() -> Result<(), url::ParseError> {
        let mut hasher = CacheKeyHasher::new();
        CanonicalUrl::parse("https://example.com/pypa/sample-namespace-packages.git@2.0.0")?
            .cache_key(&mut hasher);
        let hash_without_creds = hasher.finish();

        let mut hasher = CacheKeyHasher::new();
        CanonicalUrl::parse(
            "https://user:foo@example.com/pypa/sample-namespace-packages.git@2.0.0",
        )?
        .cache_key(&mut hasher);
        let hash_with_creds = hasher.finish();
        assert_eq!(
            hash_without_creds, hash_with_creds,
            "URLs with no user credentials should hash the same as URLs with different user credentials",
        );

        let mut hasher = CacheKeyHasher::new();
        CanonicalUrl::parse(
            "https://user:bar@example.com/pypa/sample-namespace-packages.git@2.0.0",
        )?
        .cache_key(&mut hasher);
        let hash_with_creds = hasher.finish();
        assert_eq!(
            hash_without_creds, hash_with_creds,
            "URLs with different user credentials should hash the same",
        );

        let mut hasher = CacheKeyHasher::new();
        CanonicalUrl::parse("https://:bar@example.com/pypa/sample-namespace-packages.git@2.0.0")?
            .cache_key(&mut hasher);
        let hash_with_creds = hasher.finish();
        assert_eq!(
            hash_without_creds, hash_with_creds,
            "URLs with no username, though with a password, should hash the same as URLs with different user credentials",
        );

        let mut hasher = CacheKeyHasher::new();
        CanonicalUrl::parse("https://user:@example.com/pypa/sample-namespace-packages.git@2.0.0")?
            .cache_key(&mut hasher);
        let hash_with_creds = hasher.finish();
        assert_eq!(
            hash_without_creds, hash_with_creds,
            "URLs with no password, though with a username, should hash the same as URLs with different user credentials",
        );

        Ok(())
    }

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

        // Two URLs that cannot be a base should be considered equal.
        assert_eq!(
            CanonicalUrl::parse("git+https:://github.com/pypa/sample-namespace-packages.git")?,
            CanonicalUrl::parse("git+https:://github.com/pypa/sample-namespace-packages.git")?,
        );

        // Two URLs should _not_ be considered equal based on percent-decoding slashes.
        assert_ne!(
            CanonicalUrl::parse("https://github.com/pypa/sample%2Fnamespace%2Fpackages")?,
            CanonicalUrl::parse("https://github.com/pypa/sample/namespace/packages")?,
        );

        // Two URLs should be considered equal regardless of percent-encoding.
        assert_eq!(
            CanonicalUrl::parse("https://github.com/pypa/sample%2Bnamespace%2Bpackages")?,
            CanonicalUrl::parse("https://github.com/pypa/sample+namespace+packages")?,
        );

        // Two URLs should _not_ be considered equal based on percent-decoding slashes.
        assert_ne!(
            CanonicalUrl::parse(
                "file:///home/ferris/my_project%2Fmy_project-0.1.0-py3-none-any.whl"
            )?,
            CanonicalUrl::parse(
                "file:///home/ferris/my_project/my_project-0.1.0-py3-none-any.whl"
            )?,
        );

        // Two URLs should be considered equal regardless of percent-encoding.
        assert_eq!(
            CanonicalUrl::parse(
                "file:///home/ferris/my_project/my_project-0.1.0+foo-py3-none-any.whl"
            )?,
            CanonicalUrl::parse(
                "file:///home/ferris/my_project/my_project-0.1.0%2Bfoo-py3-none-any.whl"
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
