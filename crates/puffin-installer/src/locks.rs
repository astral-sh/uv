use fxhash::FxHashMap;
use puffin_cache::RepositoryUrl;
use std::sync::Arc;
use tokio::sync::Mutex;
use url::Url;

/// A set of locks used to prevent concurrent access to the same resource.
#[derive(Debug, Default)]
pub(crate) struct Locks(Mutex<FxHashMap<String, Arc<Mutex<()>>>>);

impl Locks {
    /// Acquire a lock on the given resource.
    pub(crate) async fn acquire(&self, url: &Url) -> Arc<Mutex<()>> {
        let mut map = self.0.lock().await;
        map.entry(puffin_cache::digest(&RepositoryUrl::new(url)))
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }
}
