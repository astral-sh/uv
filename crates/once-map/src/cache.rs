use std::borrow::Borrow;
use std::fmt::Debug;
use std::hash::Hash;
use std::sync::Arc;

use dashmap::DashMap;
use tokio::sync::Notify;

use crate::Error;

pub enum Value<V> {
    Waiting(Arc<Notify>),
    Filled(Arc<V>),
}

/// Run tasks only once and store the results in a parallel hash map.
///
/// We often have jobs `Fn(K) -> V` that we only want to run once and memoize, e.g. network
/// requests for metadata. When multiple tasks start the same query in parallel, e.g. through source
/// dist builds, we want to wait until the other task is done and get a reference to the same
/// result.
pub struct CacheMap<K: Eq + Hash, V> {
    items: DashMap<K, Value<V>>,
}

impl<K: Debug + Eq + Hash, V> CacheMap<K, V> {
    /// Register that you want to start a job.
    ///
    /// If this method returns `true`, you need to start a job and call [`CacheMap::done`] eventually
    /// or other tasks will hang. If it returns `false`, this job is already in progress and you
    /// can [`CacheMap::wait`] for the result.
    pub fn register<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq + Debug + ToOwned<Owned = K>,
    {
        let entry = self.items.entry(key.to_owned());
        match entry {
            dashmap::mapref::entry::Entry::Occupied(_) => false,
            dashmap::mapref::entry::Entry::Vacant(entry) => {
                entry.insert(Value::Waiting(Arc::new(Notify::new())));
                true
            }
        }
    }

    /// Like [`CacheMap::register`], but takes ownership of the key.
    pub fn register_owned(&self, key: K) -> bool {
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
        if let Some(Value::Waiting(notify)) = self.items.insert(key, Value::Filled(Arc::new(value))) {
            notify.notify_waiters();
        }
    }

    /// Wait for the result of a job that is running.
    ///
    /// Will hang if [`CacheMap::done`] isn't called for this key.
    pub async fn wait<Q: ?Sized + Debug + Hash + Eq>(
        &self,
        key: &Q,
    ) -> Result<Arc<V>, Error>
    where
        K: Borrow<Q> + for<'a> From<&'a Q>,
    {
        let entry = self.items.get(key).expect("key must be registered");
        match entry.value() {
            Value::Filled(value) => Ok(value.clone()),
            Value::Waiting(notify) => {
                let notify = notify.clone();
                drop(entry);
                notify.notified().await;

                let entry = self.items.get(key).expect("key must be registered");
                match entry.value() {
                    Value::Filled(value) => Ok(value.clone()),
                    Value::Waiting(_) => unreachable!(),
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

impl<K: Eq + Hash + Clone, V> Default for CacheMap<K, V> {
    fn default() -> Self {
        Self {
            items: DashMap::new(),
        }
    }
}

