use std::borrow::Borrow;
use std::hash::Hash;
use std::sync::Arc;

use dashmap::DashMap;
use tokio::sync::Notify;

/// Run tasks only once and store the results in a parallel hash map.
///
/// We often have jobs `Fn(K) -> V` that we only want to run once and memoize, e.g. network
/// requests for metadata. When multiple tasks start the same query in parallel, e.g. through source
/// dist builds, we want to wait until the other task is done and get a reference to the same
/// result.
pub struct OnceMap<K: Eq + Hash, V> {
    items: DashMap<K, Value<V>>,
}

impl<K: Eq + Hash, V> OnceMap<K, V> {
    /// Register that you want to start a job.
    ///
    /// If this method returns `true`, you need to start a job and call [`OnceMap::done`] eventually
    /// or other tasks will hang. If it returns `false`, this job is already in progress and you
    /// can [`OnceMap::wait`] for the result.
    pub fn register(&self, key: K) -> bool {
        let entry = self.items.entry(key);
        match entry {
            dashmap::mapref::entry::Entry::Occupied(_) => false,
            dashmap::mapref::entry::Entry::Vacant(entry) => {
                entry.insert(Value::Waiting(Arc::new(Notify::new())));
                true
            }
        }
    }

    /// Submit the result of a job you registered.
    pub fn done(&self, key: K, value: V) {
        if let Some(Value::Waiting(notify)) = self.items.insert(key, Value::Filled(Arc::new(value)))
        {
            notify.notify_waiters();
        }
    }

    /// Wait for the result of a job that is running.
    ///
    /// Will hang if [`OnceMap::done`] isn't called for this key.
    pub async fn wait(&self, key: &K) -> Option<Arc<V>> {
        let entry = self.items.get(key)?;
        match entry.value() {
            Value::Filled(value) => Some(value.clone()),
            Value::Waiting(notify) => {
                let notify = notify.clone();
                drop(entry);
                notify.notified().await;

                let entry = self.items.get(key).expect("map is append-only");
                match entry.value() {
                    Value::Filled(value) => Some(value.clone()),
                    Value::Waiting(_) => unreachable!("notify was called"),
                }
            }
        }
    }

    /// Return the result of a previous job, if any.
    pub fn get<Q: ?Sized + Hash + Eq>(&self, key: &Q) -> Option<Arc<V>>
    where
        K: Borrow<Q>,
    {
        let entry = self.items.get(key)?;
        match entry.value() {
            Value::Filled(value) => Some(value.clone()),
            Value::Waiting(_) => None,
        }
    }
}

impl<K: Eq + Hash + Clone, V> Default for OnceMap<K, V> {
    fn default() -> Self {
        Self {
            items: DashMap::new(),
        }
    }
}

enum Value<V> {
    Waiting(Arc<Notify>),
    Filled(Arc<V>),
}
