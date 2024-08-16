use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use cache_key::RepositoryUrl;
use uv_auth::Credentials;

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
