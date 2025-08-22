use std::collections::HashMap;
use std::sync::{Arc, LazyLock, RwLock};
use tracing::trace;
use uv_auth::{Credentials, KeyringProvider};
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
    ///
    /// If a native keyring provider is available, the credentials will also be
    /// persisted to the system keyring for future use.
    ///
    /// Returns the previously stored credentials for this URL, if any.
    pub async fn insert(
        &self,
        url: RepositoryUrl,
        credentials: Credentials,
        keyring_provider: Option<&KeyringProvider>,
    ) -> Option<Arc<Credentials>> {
        if let Some(keyring_provider) = keyring_provider {
            keyring_provider.store_if_native(&url, &credentials).await;
        }
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
pub async fn store_credentials_from_url(
    url: &DisplaySafeUrl,
    keyring_provider: Option<&KeyringProvider>,
) -> bool {
    if let Some(credentials) = Credentials::from_url(url) {
        trace!("Caching credentials for {url}");
        GIT_STORE
            .insert(RepositoryUrl::new(url), credentials, keyring_provider)
            .await;
        true
    } else {
        false
    }
}
