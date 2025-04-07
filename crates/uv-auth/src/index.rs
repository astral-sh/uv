use std::fmt::{self, Display, Formatter};

use rustc_hash::FxHashSet;
use url::Url;

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
            AuthPolicy::Auto => write!(f, "auto"),
            AuthPolicy::Always => write!(f, "always"),
            AuthPolicy::Never => write!(f, "never"),
        }
    }
}

// TODO(john): We are not using `uv_distribution_types::Index` directly
// here because it would cause circular crate dependencies. However, this
// could potentially make sense for a future refactor.
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct Index {
    /// The root endpoint where authentication is applied.
    /// For PEP 503 endpoints, this excludes `/simple`.
    pub root_url: Url,
    pub auth_policy: AuthPolicy,
}

#[derive(Debug, Default, Clone, Eq, PartialEq)]
pub struct Indexes(FxHashSet<Index>);

impl Indexes {
    pub fn new() -> Self {
        Self(FxHashSet::default())
    }

    /// Create a new [`AuthIndexUrls`] from an iterator of [`AuthIndexUrl`]s.
    pub fn from_indexes(urls: impl IntoIterator<Item = Index>) -> Self {
        let mut index_urls = Self::new();
        for url in urls {
            index_urls.0.insert(url);
        }
        index_urls
    }

    /// Get the index URL prefix for a URL if one exists.
    pub fn index_url_for(&self, url: &Url) -> Option<&Url> {
        // TODO(john): There are probably not many URLs to iterate through,
        // but we could use a trie instead of a HashSet here for more
        // efficient search.
        self.0
            .iter()
            .find(|index| is_url_prefix(&index.root_url, url))
            .map(|index| &index.root_url)
    }

    /// Get the [`AuthPolicy`] for a URL.
    pub fn policy_for(&self, url: &Url) -> AuthPolicy {
        // TODO(john): There are probably not many URLs to iterate through,
        // but we could use a trie instead of a HashMap here for more
        // efficient search.
        for index in &self.0 {
            if is_url_prefix(&index.root_url, url) {
                return index.auth_policy;
            }
        }
        AuthPolicy::Auto
    }
}

fn is_url_prefix(base: &Url, url: &Url) -> bool {
    if base.scheme() != url.scheme()
        || base.host_str() != url.host_str()
        || base.port_or_known_default() != url.port_or_known_default()
    {
        return false;
    }

    url.path().starts_with(base.path())
}
