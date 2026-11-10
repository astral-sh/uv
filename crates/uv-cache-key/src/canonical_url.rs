use std::borrow::Cow;
use std::fmt::{Debug, Formatter};
use std::hash::{Hash, Hasher};

use uv_redacted::{SafeUrl, SafeUrlError};

use crate::cache_key::{CacheKey, CacheKeyHasher};

/// A wrapper around [`SafeUrl`] which represents a "canonical" version of an original URL.
///
/// This is **only** for internal purposes. All fetching should happen within the context of the
/// original URL, since canonicalization can remove credentials and change paths.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct CanonicalUrl(SafeUrl);

impl CanonicalUrl {
    /// Canonicalize a URL, applying Git source rules when the scheme starts with `git+`.
    pub fn new(url: SafeUrl) -> Self {
        if url.scheme().starts_with("git+") {
            Self::for_git_source(url)
        } else {
            Self::canonicalize(url, None)
        }
    }

    /// Apply Git source normalization even when the URL has no `git+` prefix.
    ///
    /// The key includes the requested revision, query, and fragment, including subdirectory
    /// and LFS options. GitHub path normalization lowercases `@Release` to `@release`
    /// in the key. Ref lookup still uses `Release`, and the wheel key also includes the
    /// resolved SHA.
    pub fn for_git_source(url: SafeUrl) -> Self {
        Self::canonicalize(url, Some(GitUrlKind::Source))
    }

    fn canonicalize(mut url: SafeUrl, git: Option<GitUrlKind>) -> Self {
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

        if let Some(kind) = git {
            normalize_git_path(&mut url, kind);
        } else if matches!(url.scheme(), "http" | "https") {
            // Preserve HTTP path escapes because servers can route encoded and decoded paths differently.
            return Self(url);
        }

        // Decode any percent-encoded characters in the path.
        if memchr::memchr(b'%', url.path().as_bytes()).is_some() {
            // Unwrap safety: We checked `url.cannot_be_a_base()`.
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
}

#[derive(Clone, Copy)]
enum GitUrlKind {
    Source,
    Upstream,
}

/// Apply Git path rules before decoding escapes so `%40` cannot delimit a revision.
fn normalize_git_path(url: &mut SafeUrl, kind: GitUrlKind) {
    // GitHub repository paths are case-insensitive. The scheme is already lowercase and
    // hierarchical paths are percent-encoded.
    if url.host_str() == Some("github.com") {
        let path = url.path().to_ascii_lowercase();
        url.set_path(&path);
    }

    // Repositories can generally be accessed with or without a `.git` suffix.
    if let Some((prefix, suffix)) = url.path().rsplit_once('@') {
        // In a parsed upstream, `@` belongs to the transport path.
        if let GitUrlKind::Source = kind
            && has_git_extension(prefix)
        {
            let prefix = &prefix[..prefix.len() - 4];
            let path = format!("{prefix}@{suffix}");
            url.set_path(&path);
        }
    } else if has_git_extension(url.path()) {
        let last = {
            // The URL can be a base and a path with an extension has a last segment.
            let last = url.path_segments().unwrap().next_back().unwrap();
            last[..last.len() - 4].to_owned()
        };
        url.path_segments_mut().unwrap().pop().push(&last);
    }
}

fn has_git_extension(path: &str) -> bool {
    std::path::Path::new(path)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("git"))
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

impl From<CanonicalUrl> for SafeUrl {
    fn from(value: CanonicalUrl) -> Self {
        value.0
    }
}

impl std::fmt::Display for CanonicalUrl {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
    }
}

/// The comparison and cache identity of a Git upstream.
///
/// Construct this from a Git transport URL with the revision already parsed out. It represents
/// the repository independently of the revision, subdirectory and Git LFS configuration.
/// Fetches use the original transport URL.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Hash)]
pub struct GitUpstreamUrl(CanonicalUrl);

impl GitUpstreamUrl {
    pub fn new(url: SafeUrl) -> Self {
        let CanonicalUrl(mut url) = CanonicalUrl::canonicalize(url, Some(GitUrlKind::Upstream));
        url.set_fragment(None);
        url.set_query(None);
        Self(CanonicalUrl(url))
    }

    pub fn parse(url: &str) -> Result<Self, SafeUrlError> {
        Ok(Self::new(SafeUrl::parse(url)?))
    }

    /// Return the normalized URL for inspecting the upstream identity.
    pub fn as_url(&self) -> &SafeUrl {
        let CanonicalUrl(url) = &self.0;
        url
    }
}

impl CacheKey for GitUpstreamUrl {
    fn cache_key(&self, state: &mut CacheKeyHasher) {
        self.0.cache_key(state);
    }
}

impl std::fmt::Display for GitUpstreamUrl {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache_digest;

    #[test]
    fn user_credential_does_not_affect_cache_key() -> Result<(), SafeUrlError> {
        let mut hasher = CacheKeyHasher::new();
        CanonicalUrl::new(SafeUrl::parse(
            "https://example.com/pypa/sample-namespace-packages.git@2.0.0",
        )?)
        .cache_key(&mut hasher);
        let hash_without_creds = hasher.finish();

        let mut hasher = CacheKeyHasher::new();
        CanonicalUrl::new(SafeUrl::parse(
            "https://user:foo@example.com/pypa/sample-namespace-packages.git@2.0.0",
        )?)
        .cache_key(&mut hasher);
        let hash_with_creds = hasher.finish();
        assert_eq!(
            hash_without_creds, hash_with_creds,
            "URLs with no user credentials should hash the same as URLs with different user credentials",
        );

        let mut hasher = CacheKeyHasher::new();
        CanonicalUrl::new(SafeUrl::parse(
            "https://user:bar@example.com/pypa/sample-namespace-packages.git@2.0.0",
        )?)
        .cache_key(&mut hasher);
        let hash_with_creds = hasher.finish();
        assert_eq!(
            hash_without_creds, hash_with_creds,
            "URLs with different user credentials should hash the same",
        );

        let mut hasher = CacheKeyHasher::new();
        CanonicalUrl::new(SafeUrl::parse(
            "https://:bar@example.com/pypa/sample-namespace-packages.git@2.0.0",
        )?)
        .cache_key(&mut hasher);
        let hash_with_creds = hasher.finish();
        assert_eq!(
            hash_without_creds, hash_with_creds,
            "URLs with no username, though with a password, should hash the same as URLs with different user credentials",
        );

        let mut hasher = CacheKeyHasher::new();
        CanonicalUrl::new(SafeUrl::parse(
            "https://user:@example.com/pypa/sample-namespace-packages.git@2.0.0",
        )?)
        .cache_key(&mut hasher);
        let hash_with_creds = hasher.finish();
        assert_eq!(
            hash_without_creds, hash_with_creds,
            "URLs with no password, though with a username, should hash the same as URLs with different user credentials",
        );

        Ok(())
    }

    #[test]
    fn canonical_git_source_url() -> Result<(), SafeUrlError> {
        let canonical = |url| SafeUrl::parse(url).map(CanonicalUrl::new);

        assert_eq!(
            canonical("git+https://github.com/pypa/sample-namespace-packages.git")?,
            canonical("git+https://github.com/pypa/sample-namespace-packages")?,
        );
        assert_eq!(
            canonical("git+https://github.com/pypa/sample-namespace-packages.git@2.0.0")?,
            canonical("git+https://github.com/pypa/sample-namespace-packages@2.0.0")?,
        );
        assert_ne!(
            canonical("git+https://github.com/pypa/sample-namespace-packages.git")?,
            canonical("git+https://github.com/pypa/sample-packages.git")?,
        );
        assert_ne!(
            canonical(
                "git+https://github.com/pypa/sample-namespace-packages.git#subdirectory=pkg_resources/pkg_a"
            )?,
            canonical(
                "git+https://github.com/pypa/sample-namespace-packages.git#subdirectory=pkg_resources/pkg_b"
            )?,
        );
        assert_ne!(
            canonical("git+https://github.com/pypa/sample-namespace-packages.git")?,
            canonical("git+https://github.com/pypa/sample-namespace-packages.git#lfs=true")?,
        );
        assert_ne!(
            canonical("git+https://github.com/pypa/sample-namespace-packages.git@2.0.0")?,
            canonical("git+https://github.com/pypa/sample-namespace-packages.git@v1.0.0")?,
        );

        Ok(())
    }

    #[test]
    fn canonical_url() -> Result<(), SafeUrlError> {
        assert_ne!(
            CanonicalUrl::new(SafeUrl::parse("https://example.com/simple/")?),
            CanonicalUrl::new(SafeUrl::parse("https://example.com/simple.git/")?),
        );

        let opaque = SafeUrl::parse("git+https:://github.com/pypa/sample-namespace-packages.git")?;
        assert_eq!(SafeUrl::from(CanonicalUrl::new(opaque.clone())), opaque);

        // Two URLs should _not_ be considered equal based on percent-decoding slashes.
        assert_ne!(
            CanonicalUrl::new(SafeUrl::parse(
                "https://github.com/pypa/sample%2Fnamespace%2Fpackages"
            )?),
            CanonicalUrl::new(SafeUrl::parse(
                "https://github.com/pypa/sample/namespace/packages"
            )?),
        );

        assert_ne!(
            CanonicalUrl::new(SafeUrl::parse(
                "https://github.com/pypa/sample%2Bnamespace%2Bpackages"
            )?),
            CanonicalUrl::new(SafeUrl::parse(
                "https://github.com/pypa/sample+namespace+packages"
            )?),
        );

        assert_ne!(
            CanonicalUrl::new(SafeUrl::parse("HTTPS://github.com/PYPA/Éxample")?),
            CanonicalUrl::new(SafeUrl::parse("https://github.com/pypa/%C3%89xample")?),
        );
        assert_ne!(
            CanonicalUrl::new(SafeUrl::parse("https://github.com/pypa/Éxample")?),
            CanonicalUrl::new(SafeUrl::parse("https://github.com/pypa/éxample")?),
        );

        // Two URLs should _not_ be considered equal based on percent-decoding slashes.
        assert_ne!(
            CanonicalUrl::new(SafeUrl::parse(
                "file:///home/ferris/my_project%2Fmy_project-0.1.0-py3-none-any.whl"
            )?),
            CanonicalUrl::new(SafeUrl::parse(
                "file:///home/ferris/my_project/my_project-0.1.0-py3-none-any.whl"
            )?),
        );

        // Two URLs should be considered equal regardless of percent-encoding.
        assert_eq!(
            CanonicalUrl::new(SafeUrl::parse(
                "file:///home/ferris/my_project/my_project-0.1.0+foo-py3-none-any.whl"
            )?),
            CanonicalUrl::new(SafeUrl::parse(
                "file:///home/ferris/my_project/my_project-0.1.0%2Bfoo-py3-none-any.whl"
            )?),
        );

        Ok(())
    }

    #[test]
    fn git_upstream_url() -> Result<(), SafeUrlError> {
        let upstream =
            GitUpstreamUrl::parse("https://github.com/PYPA/sample-namespace-packages.git")?;
        assert_eq!(
            upstream,
            GitUpstreamUrl::parse("https://github.com/pypa/sample-namespace-packages")?,
        );
        assert_ne!(
            upstream,
            GitUpstreamUrl::parse("https://github.com/pypa/sample-packages.git")?,
        );
        assert_eq!(
            upstream,
            GitUpstreamUrl::parse(
                "https://user:password@github.com/pypa/sample-namespace-packages.git?query=value#fragment",
            )?,
        );

        Ok(())
    }

    #[test]
    fn git_upstream_preserves_at_signs_in_transport_paths() -> Result<(), SafeUrlError> {
        let plain = GitUpstreamUrl::parse("https://example.com/pkg@tenant")?;
        let suffixed = GitUpstreamUrl::parse("https://example.com/pkg.git@tenant")?;
        assert_ne!(plain, suffixed);
        assert_ne!(
            plain,
            GitUpstreamUrl::parse("https://example.com/pkg@tenant.git")?,
        );
        assert_eq!(
            suffixed,
            GitUpstreamUrl::parse("https://example.com/pkg.git%40tenant")?,
        );

        assert_ne!(cache_digest(&plain), cache_digest(&suffixed));

        let leading = GitUpstreamUrl::parse("https://example.com/@solo")?;
        assert_eq!(
            leading,
            GitUpstreamUrl::parse("https://example.com/%40solo")?,
        );
        assert_eq!(
            GitUpstreamUrl::parse("https://example.com/pkg.git/@solo")?
                .as_url()
                .path(),
            "/pkg.git/@solo",
        );

        Ok(())
    }
}
