use rustc_hash::FxHashMap;
use url::Url;

#[derive(Copy, Clone, Debug, Default, Hash, Eq, PartialEq)]
pub enum AuthPolicy {
    /// Try an unauthenticated request. If that fails, try an authenticated request.
    #[default]
    Auto,
    /// Always attempt to authenticate.
    Always,
    /// Never attempt to authenticate.
    Never,
}

#[derive(Debug, Default, Clone, Eq, PartialEq)]
pub struct UrlAuthPolicies(FxHashMap<Url, AuthPolicy>);

impl UrlAuthPolicies {
    pub fn new() -> Self {
        Self(FxHashMap::default())
    }

    /// Create a new [`UrlAuthPolicies`] from a list of URL and [`AuthPolicy`]
    /// tuples.
    pub fn from_tuples(tuples: impl IntoIterator<Item = (Url, AuthPolicy)>) -> Self {
        let mut auth_policies = Self::new();
        for (url, auth_policy) in tuples {
            auth_policies.add_policy(url, auth_policy);
        }
        auth_policies
    }

    /// An [`AuthPolicy`] for a URL.
    pub fn add_policy(&mut self, url: Url, auth_policy: AuthPolicy) {
        self.0.insert(url, auth_policy);
    }

    /// Get the [`AuthPolicy`] for a URL.
    pub fn policy_for(&self, url: &Url) -> AuthPolicy {
        // TODO(john): There are probably not many URLs to iterate through,
        // but we could use a trie instead of a HashMap here for more
        // efficient search.
        for (auth_url, auth_policy) in &self.0 {
            if url.as_str().starts_with(auth_url.as_str()) {
                return *auth_policy;
            }
        }
        AuthPolicy::Auto
    }
}
