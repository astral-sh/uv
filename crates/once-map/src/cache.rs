use std::borrow::Borrow;
use std::collections::hash_map::RandomState;
use std::hash::Hash;
use std::sync::{Arc, Mutex};

use rustc_hash::FxHashMap;
use tokio::sync::Notify;
use waitmap::Ref;
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
    items: Mutex<FxHashMap<K, Value<V>>>,
}

impl<K: Eq + Hash, V> CacheMap<K, V> {
    /// Register that you want to start a job.
    ///
    /// If this method returns `true`, you need to start a job and call [`CacheMap::done`] eventually
    /// or other tasks will hang. If it returns `false`, this job is already in progress and you
    /// can [`CacheMap::wait`] for the result.
    pub fn register<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq + ToOwned<Owned = K>,
    {
        let mut lock = self.items.lock().unwrap();
        if lock.contains_key(key) {
             false
        } else {
            lock.insert(key.to_owned(), Value::Waiting(Arc::new(Notify::new())));
            true
        }
    }

    /// Like [`CacheMap::register`], but takes ownership of the key.
    pub fn register_owned(&self, key: K) -> bool {
        let mut lock = self.items.lock().unwrap();
        if lock.contains_key(&key) {
             false
        } else {
            lock.insert(key, Value::Waiting(Arc::new(Notify::new())));
            true
        }
    }

    /// Submit the result of a job you registered.
    pub fn done(&self, key: K, value: V) {
        let mut lock = self.items.lock().unwrap();
        if let Some(Value::Waiting(notify)) = lock.insert(key, Value::Filled(Arc::new(value))) {
            notify.notify_waiters();
        }
    }

    /// Wait for the result of a job that is running.
    ///
    /// Will hang if [`CacheMap::done`] isn't called for this key.
    pub async fn wait<Q: ?Sized + Hash + Eq>(
        &self,
        key: &Q,
    ) -> Result<Arc<V>, Error>
    where
        K: Borrow<Q> + for<'a> From<&'a Q>,
    {
        let mut lock = self.items.lock().unwrap();
        let notify = match lock.get(key) {
            Some(Value::Filled(value)) => return Ok(value.clone()),
            Some(Value::Waiting(notify)) => notify.clone(),
            None => return Err(Error::Canceled),
        };
        drop(lock);

        notify.notified().await;

        let lock = self.items.lock().unwrap();
        let Some(Value::Filled(value)) = lock.get(key) else { return Err(Error::Canceled) };
        Ok(value.clone())
    }

    /// Return the result of a previous job, if any.
    pub fn get<Q: ?Sized + Hash + Eq>(&self, key: &Q) -> Option<Arc<V>>
    where
        K: Borrow<Q>,
    {
        let lock = self.items.lock().unwrap();
        if let Some(Value::Filled(value)) = lock.get(key) {
            Some(value.clone())
        } else {
            None
        }
    }
}

impl<K: Eq + Hash + Clone, V> Default for CacheMap<K, V> {
    fn default() -> Self {
        Self {
            items: Mutex::new(FxHashMap::default()),
        }
    }
}

