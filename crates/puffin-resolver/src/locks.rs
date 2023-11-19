use std::sync::Arc;

use fxhash::FxHashMap;
use tokio::sync::Mutex;

use distribution_types::Identifier;

/// A set of locks used to prevent concurrent access to the same resource.
#[derive(Debug, Default)]
pub(crate) struct Locks(Mutex<FxHashMap<String, Arc<Mutex<()>>>>);

impl Locks {
    /// Acquire a lock on the given resource.
    pub(crate) async fn acquire(&self, distribution: &impl Identifier) -> Arc<Mutex<()>> {
        let mut map = self.0.lock().await;
        map.entry(distribution.resource_id())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }
}
