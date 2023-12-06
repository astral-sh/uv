use std::borrow::Borrow;
use std::collections::hash_map::RandomState;
use std::collections::HashSet;
use std::hash::Hash;
use tokio::sync::Mutex;

use waitmap::{Ref, WaitMap};

/// Run tasks only once and store the results in a parallel hash map.
///
/// We often have jobs `Fn(K) -> V` that we only want to run once and memoize, e.g. network
/// requests for metadata. When multiple tasks start the same query in parallel, e.g. through source
/// dist builds, we want to wait until the other task is done and get a reference to the same
/// result.
pub struct OnceMap<K: Eq + Hash, V> {
    /// Computations that were started, including those that were finished.
    started: Mutex<HashSet<K>>,
    waitmap: WaitMap<K, V>,
}

impl<K: Eq + Hash, V> OnceMap<K, V> {
    /// Return the result of an in-flight request or register a new in-flight request.
    ///
    /// You must call [`OnceMap::done`] or other tasks will hang.
    pub async fn wait_or_register<'a>(&self, key: &'a K) -> Option<Ref<'_, K, V, RandomState>>
    where
        K: Clone + From<&'a K> + 'a,
    {
        let mut in_flight_lock = self.started.lock().await;
        if in_flight_lock.contains(key) {
            let reference = self
                .waitmap
                .wait(key)
                .await
                .expect("This operation is never cancelled");
            Some(reference)
        } else {
            in_flight_lock.insert(key.clone());
            None
        }
    }

    /// Register that you want to start a job.
    ///
    /// If this method returns `true`, you need to start a job and call [`OnceMap::done`] eventually
    /// or other tasks will hang. If it returns `false`, this job is already in progress and you
    /// can [`OnceMap::wait`] for the result.
    pub async fn register(&self, key: K) -> bool {
        self.started.lock().await.insert(key)
    }

    /// Submit the result of a job you registered.
    pub fn done(&self, key: K, value: V) {
        self.waitmap.insert(key, value);
    }

    /// Wait for the result of a job that is running.
    ///
    /// Will hang if [`OnceMap::done`] isn't called for this key.
    pub async fn wait<Q: ?Sized + Hash + Eq>(&self, key: &Q) -> Ref<'_, K, V, RandomState>
    where
        K: Borrow<Q> + for<'a> From<&'a Q>,
    {
        self.waitmap
            .wait(key)
            .await
            .expect("This operation is never cancelled")
    }

    /// Return the result of a previous job, if any.
    pub fn get<Q: ?Sized + Hash + Eq>(&self, key: &Q) -> Option<Ref<'_, K, V, RandomState>>
    where
        K: Borrow<Q>,
    {
        self.waitmap.get(key)
    }
}

impl<K: Eq + Hash + Clone, V> Default for OnceMap<K, V> {
    fn default() -> Self {
        Self {
            started: Mutex::new(HashSet::new()),
            waitmap: WaitMap::new(),
        }
    }
}
