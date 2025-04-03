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

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct AuthIndex {
    /// The PEP 503 simple endpoint for the index
    pub index_url: Url,
    /// The root endpoint where the auth policy is applied.
    /// For PEP 503 endpoints, this excludes `/simple`.
    pub policy_url: Url,
    pub auth_policy: AuthPolicy,
}

#[derive(Debug, Default, Clone, Eq, PartialEq)]
pub struct AuthIndexes(FxHashSet<AuthIndex>);

impl AuthIndexes {
    pub fn new() -> Self {
        Self(FxHashSet::default())
    }

    /// Create a new [`AuthIndexUrls`] from an iterator of [`AuthIndexUrl`]s.
    pub fn from_auth_indexes(urls: impl IntoIterator<Item = AuthIndex>) -> Self {
        let mut auth_index_urls = Self::new();
        for url in urls {
            auth_index_urls.0.insert(url);
        }
        auth_index_urls
    }

    /// Get the index URL prefix for a URL if one exists.
    pub fn auth_index_url_for(&self, url: &Url) -> Option<&Url> {
        // TODO(john): There are probably not many URLs to iterate through,
        // but we could use a trie instead of a HashSet here for more
        // efficient search.
        self.0
            .iter()
            .find(|auth_index| url.as_str().starts_with(auth_index.index_url.as_str()))
            .map(|auth_index| &auth_index.index_url)
    }

    /// Get the [`AuthPolicy`] for a URL.
    pub fn policy_for(&self, url: &Url) -> AuthPolicy {
        // TODO(john): There are probably not many URLs to iterate through,
        // but we could use a trie instead of a HashMap here for more
        // efficient search.
        for auth_index in &self.0 {
            if url.as_str().starts_with(auth_index.policy_url.as_str()) {
                return auth_index.auth_policy;
            }
        }
        AuthPolicy::Auto
    }
}
