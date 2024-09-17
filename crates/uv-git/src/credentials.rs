use cache_key::RepositoryUrl;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, RwLock};
use tracing::trace;
use url::Url;
use uv_auth::Credentials;

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
pub fn store_credentials_from_url(url: &Url) -> bool {
    if let Some(credentials) = Credentials::from_url(url) {
        trace!("Caching credentials for {url}");
        GIT_STORE.insert(RepositoryUrl::new(url), credentials);
        true
    } else {
        false
    }
}
