use std::borrow::Borrow;
use std::collections::hash_map::RandomState;

use std::hash::Hash;
use std::sync::Mutex;

use rustc_hash::FxHashSet;
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
}

impl<K: Eq + Hash, V> OnceMap<K, V> {
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
        let mut lock = self.started.lock().unwrap();
        if lock.contains(key) {
            return false;
        }
        lock.insert(key.to_owned())
    }

    /// Submit the result of a job you registered.
    pub fn done(&self, key: K, value: V) {
        self.wait_map.insert(key, value);
    }

    /// Wait for the result of a job that is running.
    ///
    /// Will hang if [`OnceMap::done`] isn't called for this key.
    pub async fn wait<Q: ?Sized + Hash + Eq>(&self, key: &Q) -> Ref<'_, K, V, RandomState>
    where
        K: Borrow<Q> + for<'a> From<&'a Q>,
    {
        self.wait_map
            .wait(key)
            .await
            .expect("This operation is never cancelled")
    }

    /// Return the result of a previous job, if any.
    pub fn get<Q: ?Sized + Hash + Eq>(&self, key: &Q) -> Option<Ref<'_, K, V, RandomState>>
    where
        K: Borrow<Q>,
    {
        self.wait_map.get(key)
    }
}

impl<K: Eq + Hash + Clone, V> Default for OnceMap<K, V> {
    fn default() -> Self {
        Self {
            started: Mutex::new(FxHashSet::default()),
            wait_map: WaitMap::new(),
        }
    }
}
