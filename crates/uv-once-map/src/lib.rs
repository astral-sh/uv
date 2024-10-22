use std::borrow::Borrow;
use std::hash::{BuildHasher, Hash, RandomState};
use std::pin::pin;
use std::sync::Arc;

use dashmap::DashMap;
use tokio::sync::Notify;

/// Run tasks only once and store the results in a parallel hash map.
///
/// We often have jobs `Fn(K) -> V` that we only want to run once and memoize, e.g. network
/// requests for metadata. When multiple tasks start the same query in parallel, e.g. through source
/// dist builds, we want to wait until the other task is done and get a reference to the same
/// result.
///
/// Note that this always clones the value out of the underlying map. Because
/// of this, it's common to wrap the `V` in an `Arc<V>` to make cloning cheap.
pub struct OnceMap<K, V, S = RandomState> {
    items: DashMap<K, Value<V>, S>,
}

impl<K: Eq + Hash, V: Clone, H: BuildHasher + Clone> OnceMap<K, V, H> {
    /// Create a [`OnceMap`] with the specified hasher.
    pub fn with_hasher(hasher: H) -> OnceMap<K, V, H> {
        OnceMap {
            items: DashMap::with_hasher(hasher),
        }
    }

    /// Create a [`OnceMap`] with the specified capacity and hasher.
    pub fn with_capacity_and_hasher(capacity: usize, hasher: H) -> OnceMap<K, V, H> {
        OnceMap {
            items: DashMap::with_capacity_and_hasher(capacity, hasher),
        }
    }

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
        if let Some(Value::Waiting(notify)) = self.items.insert(key, Value::Filled(value)) {
            notify.notify_waiters();
        }
    }

    /// Wait for the result of a job that is running.
    ///
    /// Will hang if [`OnceMap::done`] isn't called for this key.
    pub async fn wait(&self, key: &K) -> Option<V> {
        let notify = {
            let entry = self.items.get(key)?;
            match entry.value() {
                Value::Filled(value) => return Some(value.clone()),
                Value::Waiting(notify) => notify.clone(),
            }
        };

        // Register the waiter for calls to `notify_waiters`.
        let notification = pin!(notify.notified());

        // Make sure the value wasn't inserted in-between us checking the map and registering the waiter.
        if let Value::Filled(value) = self.items.get(key).expect("map is append-only").value() {
            return Some(value.clone());
        };

        // Wait until the value is inserted.
        notification.await;

        let entry = self.items.get(key).expect("map is append-only");
        match entry.value() {
            Value::Filled(value) => Some(value.clone()),
            Value::Waiting(_) => unreachable!("notify was called"),
        }
    }

    /// Wait for the result of a job that is running, in a blocking context.
    ///
    /// Will hang if [`OnceMap::done`] isn't called for this key.
    pub fn wait_blocking(&self, key: &K) -> Option<V> {
        futures::executor::block_on(self.wait(key))
    }

    /// Return the result of a previous job, if any.
    pub fn get<Q: ?Sized + Hash + Eq>(&self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
    {
        let entry = self.items.get(key)?;
        match entry.value() {
            Value::Filled(value) => Some(value.clone()),
            Value::Waiting(_) => None,
        }
    }

    /// Remove the result of a previous job, if any.
    pub fn remove<Q: ?Sized + Hash + Eq>(&self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
    {
        let entry = self.items.remove(key)?;
        match entry {
            (_, Value::Filled(value)) => Some(value),
            (_, Value::Waiting(_)) => None,
        }
    }
}

impl<K: Eq + Hash + Clone, V, H: Default + BuildHasher + Clone> Default for OnceMap<K, V, H> {
    fn default() -> Self {
        Self {
            items: DashMap::with_hasher(H::default()),
        }
    }
}

impl<K, V, H> FromIterator<(K, V)> for OnceMap<K, V, H>
where
    K: Eq + Hash,
    H: Default + Clone + BuildHasher,
{
    fn from_iter<T: IntoIterator<Item = (K, V)>>(iter: T) -> Self {
        OnceMap {
            items: iter
                .into_iter()
                .map(|(k, v)| (k, Value::Filled(v)))
                .collect(),
        }
    }
}

enum Value<V> {
    Waiting(Arc<Notify>),
    Filled(V),
}
