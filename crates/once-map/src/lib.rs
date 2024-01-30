use std::borrow::Borrow;
use std::collections::hash_map::RandomState;

use std::hash::Hash;
use std::sync::{Arc, Mutex};

use rustc_hash::{FxHashMap, FxHashSet};
use tokio::sync::Notify;
use waitmap::{Ref, WaitMap};

/// Run tasks only once and store the results in a parallel hash map.
///
/// We often have jobs `Fn(K) -> V` that we only want to run once and memoize, e.g. network
/// requests for metadata. When multiple tasks start the same query in parallel, e.g. through source
/// dist builds, we want to wait until the other task is done and get a reference to the same
/// result.
pub struct OnceMap<K: Eq + Hash, V> {
    /// Computations that were started, including those that were finished.
    started: Mutex<FxHashSet<K>>,
    wait_map: WaitMap<K, V>,

    data: Mutex<FxHashMap<K, V>>,
    notifiers: Mutex<FxHashMap<K, Arc<Notify>>>,
}

impl<K: Eq + Hash, V: Clone> OnceMap<K, V> {
    /// Register that you want to start a job.
    ///
    /// If this method returns `true`, you need to start a job and call [`OnceMap::done`] eventually
    /// or other tasks will hang. If it returns `false`, this job is already in progress and you
    /// can [`OnceMap::wait`] for the result.
    pub fn register<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq + ToOwned<Owned = K>,
    {
        let mut lock = self.notifiers.lock().unwrap();
        if lock.contains_key(key) {
            false
        } else {
            lock.insert(key.to_owned(), Arc::new(Notify::new()));
            true
        }
    }

    /// Like [`OnceMap::register`], but takes ownership of the key.
    pub fn register_owned(&self, key: K) -> bool {
        let mut lock = self.notifiers.lock().unwrap();
        if lock.contains_key(&key) {
            false
        } else {
            lock.insert(key, Arc::new(Notify::new()));
            true
        }
    }

    /// Submit the result of a job you registered.
    pub fn done(&self, key: K, value: V) {
        // Remove the notifier from the map.
        let mut notifiers = self.notifiers.lock().unwrap();
        let notifier = notifiers.remove(&key).unwrap();

        // Add the result to the map.
        let mut lock = self.data.lock().unwrap();
        lock.insert(key, value);

        // Notify all waiters.
        notifier.notify_waiters();
    }

    /// Wait for the result of a job that is running.
    ///
    /// Will hang if [`OnceMap::done`] isn't called for this key.
    pub async fn wait<Q: ?Sized + Hash + Eq>(
        &self,
        key: &Q,
    ) -> Result<V, Error>
    where
        K: Borrow<Q> + for<'a> From<&'a Q>,
    {
        // Wait for the data to be available.
        {
            let lock = self.notifiers.lock().unwrap();
            if let Some(notifier) = lock.get(key).cloned() {
                drop(lock);
                notifier.notified().await;
            }
        }

        // Get the data.
        let lock = self.data.lock().unwrap();
        lock.get(key).cloned().ok_or(Error::Canceled)
    }

    /// Return the result of a previous job, if any.
    pub fn get<Q: ?Sized + Hash + Eq>(&self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
    {
        let lock = self.data.lock().unwrap();
        lock.get(key).cloned()
    }

    /// Cancel all waiting tasks.
    ///
    /// Warning: waiting on tasks that have been canceled will cause the map to hang.
    pub fn cancel_all(&self) {
        self.wait_map.cancel_all();
    }
}

impl<K: Eq + Hash + Clone, V> Default for OnceMap<K, V> {
    fn default() -> Self {
        Self {
            started: Mutex::new(FxHashSet::default()),
            wait_map: WaitMap::new(),
            data: Mutex::default(),
            notifiers: Mutex::default(),

        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("The operation was canceled")]
    Canceled,
}
