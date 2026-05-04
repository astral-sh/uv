use std::fmt::{self, Display, Formatter};

use rustc_hash::FxHashSet;
use url::Url;
use uv_redacted::DisplaySafeUrl;

/// When to use authentication.
#[derive(
    Copy,
    Clone,
    Debug,
    Default,
    Hash,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    serde::Serialize,
    serde::Deserialize,
)]
#[serde(rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum AuthPolicy {
    /// Authenticate when necessary.
    ///
    /// If credentials are provided, they will be used. Otherwise, an unauthenticated request will
    /// be attempted first. If the request fails, uv will search for credentials. If credentials are
    /// found, an authenticated request will be attempted.
    #[default]
    Auto,
    /// Always authenticate.
    ///
    /// If credentials are not provided, uv will eagerly search for credentials. If credentials
    /// cannot be found, uv will error instead of attempting an unauthenticated request.
    Always,
    /// Never authenticate.
    ///
    /// If credentials are provided, uv will error. uv will not search for credentials.
    Never,
}

impl Display for AuthPolicy {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            Self::Auto => write!(f, "auto"),
            Self::Always => write!(f, "always"),
            Self::Never => write!(f, "never"),
        }
    }
}

// TODO(john): We are not using `uv_distribution_types::Index` directly
// here because it would cause circular crate dependencies. However, this
// could potentially make sense for a future refactor.
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct Index {
    pub url: DisplaySafeUrl,
    /// The root endpoint where authentication is applied.
    /// For PEP 503 endpoints, this excludes `/simple`.
    pub root_url: DisplaySafeUrl,
    pub auth_policy: AuthPolicy,
}

impl Index {
    pub fn is_prefix_for(&self, url: &Url) -> bool {
        if self.root_url.scheme() != url.scheme()
            || self.root_url.host_str() != url.host_str()
            || self.root_url.port_or_known_default() != url.port_or_known_default()
        {
            return false;
        }

        is_path_prefix(self.root_url.path(), url.path())
    }
}

/// Returns `true` if `prefix` is a complete path-segment prefix of `path`.
///
/// This rejects partial segment matches, so `/simple` matches `/simple/anyio` but not
/// `/simpleevil`.
pub(crate) fn is_path_prefix(prefix: &str, path: &str) -> bool {
    if prefix == path {
        return true;
    }

    let Some(suffix) = path.strip_prefix(prefix) else {
        return false;
    };

    prefix.ends_with('/') || suffix.starts_with('/')
}

// TODO(john): Multiple methods in this struct need to iterate over
// all the indexes in the set. There are probably not many URLs to
// iterate through, but we could use a trie instead of a HashSet here
// for more efficient search.
#[derive(Debug, Default, Clone, Eq, PartialEq)]
pub struct Indexes(FxHashSet<Index>);

impl Indexes {
    pub fn new() -> Self {
        Self(FxHashSet::default())
    }

    /// Create a new [`Indexes`] instance from an iterator of [`Index`]s.
    pub fn from_indexes(urls: impl IntoIterator<Item = Index>) -> Self {
        let mut index_urls = Self::new();
        for url in urls {
            index_urls.0.insert(url);
        }
        index_urls
    }

    /// Get the index for a URL if one exists.
    pub fn index_for(&self, url: &Url) -> Option<&Index> {
        self.find_prefix_index(url)
    }

    /// Get the [`AuthPolicy`] for a URL.
    pub fn auth_policy_for(&self, url: &Url) -> AuthPolicy {
        self.find_prefix_index(url)
            .map(|index| index.auth_policy)
            .unwrap_or(AuthPolicy::Auto)
    }

    fn find_prefix_index(&self, url: &Url) -> Option<&Index> {
        self.0.iter().find(|&index| index.is_prefix_for(url))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index(root_url: &str, auth_policy: AuthPolicy) -> Index {
        let root_url = DisplaySafeUrl::parse(root_url).unwrap();
        Index {
            url: root_url.clone(),
            root_url,
            auth_policy,
        }
    }

    #[test]
    fn test_index_path_prefix_requires_segment_boundary() {
        let index = index("https://example.com/simple", AuthPolicy::Always);

        for url in [
            "https://example.com/simple",
            "https://example.com/simple/",
            "https://example.com/simple/anyio",
        ] {
            assert!(
                index.is_prefix_for(&Url::parse(url).unwrap()),
                "Failed to match URL with prefix: {url}"
            );
        }

        for url in [
            "https://example.com/simpleevil",
            "https://example.com/simple-evil",
            "https://example.com/simpl",
        ] {
            assert!(
                !index.is_prefix_for(&Url::parse(url).unwrap()),
                "Should not match URL with partial path segment: {url}"
            );
        }
    }
}
