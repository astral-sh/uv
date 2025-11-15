use std::collections::HashMap;
use std::sync::{Arc, LazyLock, RwLock};
use tracing::trace;
use uv_auth::Credentials;
use uv_cache_key::RepositoryUrl;
use uv_redacted::DisplaySafeUrl;

/// Global authentication cache for a uv invocation.
///
/// This is used to share Git credentials within a single process.
pub static GIT_STORE: LazyLock<GitStore> = LazyLock::new(GitStore::default);

/// A store for Git credentials.
#[derive(Debug, Default)]
pub struct GitStore(RwLock<HashMap<RepositoryUrl, Arc<Credentials>>>);

impl GitStore {
    /// Insert [`Credentials`] for the given URL into the store.
    pub fn insert(&self, url: RepositoryUrl, credentials: Credentials) -> Option<Arc<Credentials>> {
        self.0.write().unwrap().insert(url, Arc::new(credentials))
    }

    /// Get the [`Credentials`] for the given URL, if they exist.
    pub fn get(&self, url: &RepositoryUrl) -> Option<Arc<Credentials>> {
        self.0.read().unwrap().get(url).cloned()
    }
}

/// Populate the global authentication store with credentials on a Git URL, if there are any.
///
/// Returns `true` if the store was updated.
pub fn store_credentials_from_url(url: &DisplaySafeUrl) -> bool {
    if let Some(credentials) = Credentials::from_url(url) {
        trace!("Caching credentials for {url}");
        // If the store already contains credentials for this repository, don't override them
        // with credentials embedded in the URL. This respects higher-precedence credential
        // sources such as environment variables and keyrings.
        let repo = RepositoryUrl::new(url);
        if GIT_STORE.get(&repo).is_some() {
            trace!("Skipping caching Git credentials for {url}: credentials already present");
            return false;
        }
        GIT_STORE.insert(repo, credentials);
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_store_credentials_from_url_does_not_override_git_store() {
        use uv_auth::Credentials;

        let base = DisplaySafeUrl::parse("https://github.com/astral-sh/uv").unwrap();
        let repo = RepositoryUrl::new(&base);
        // Seed the store with env-var-like creds
        let env_creds =
            Credentials::basic(Some("envuser".to_string()), Some("envpass".to_string()));
        GIT_STORE.insert(repo.clone(), env_creds.clone());

        // URL with embedded credentials that would have overridden
        let mut url_with_creds = base.clone();
        let _ = url_with_creds.set_username("urluser");
        let _ = url_with_creds.set_password(Some("urlpass"));

        assert!(!store_credentials_from_url(&url_with_creds));

        let cached = GIT_STORE.get(&repo).unwrap();
        assert_eq!(cached.username(), env_creds.username());
        assert_eq!(cached.password(), env_creds.password());
    }
}
