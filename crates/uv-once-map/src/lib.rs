use std::borrow::Borrow;
use std::fmt::{Debug, Display, Formatter};
use std::hash::{BuildHasher, Hash, RandomState};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use papaya::{HashMap, ResizeMode};
use tokio::sync::Notify;

/// The caller tried to wait for a task that was never registered.
#[derive(Debug)]
pub struct UnregisteredTask<K>(K);

impl<K: Display> Display for UnregisteredTask<K> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Attempted to wait on an unregistered task: {}", self.0)
    }
}

impl<K: Debug + Display> std::error::Error for UnregisteredTask<K> {}

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
    items: HashMap<K, Arc<Value<V>>, S>,
}

impl<K: Eq + Hash + Debug, V: Debug, S: BuildHasher + Clone> Debug for OnceMap<K, V, S> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(&self.items, f)
    }
}

impl<K: Eq + Hash + Clone, V: Clone, H: BuildHasher + Clone> OnceMap<K, V, H> {
    /// Register that you want to start a job.
    ///
    /// If this method returns `true`, you need to start a job and call [`OnceMap::done`] eventually
    /// or other tasks will hang. If it returns `false`, this job is already in progress and you
    /// can [`OnceMap::wait`] for the result.
    pub fn register(&self, key: K) -> bool {
        self.items
            .pin()
            .try_insert_with(key, || Arc::new(Value::new(None)))
            .is_ok()
    }

    /// Register a job and retain its result slot.
    ///
    /// [`Registration::New`] requires the caller to start the job and eventually call
    /// [`OnceMap::done`]. [`Registration::Existing`] shares an already registered job.
    pub fn register_entry(&self, key: K) -> Registration<V> {
        match self
            .items
            .pin()
            .try_insert_with(key, || Arc::new(Value::new(None)))
        {
            Ok(value) => Registration::New(RegisteredEntry(Arc::clone(value))),
            Err(value) => Registration::Existing(RegisteredEntry(Arc::clone(value))),
        }
    }

    /// Return a handle to a registered job, including one that has already completed.
    pub fn entry<Q: ?Sized + Hash + Eq>(&self, key: &Q) -> Option<RegisteredEntry<V>>
    where
        K: Borrow<Q>,
    {
        self.items
            .pin()
            .get(key)
            .map(|value| RegisteredEntry(Arc::clone(value)))
    }

    /// Register that you want to start a job, unless it was already started, then wait for its
    /// result.
    ///
    /// Use this method for once-only operations.
    ///
    /// Returns `None` if the job needs to be started, otherwise returns the result of the job.
    ///
    ///  # Example
    ///
    /// ```rust,ignore
    /// if let Some(response) = cache.register_or_wait(&id).await {
    ///     response
    /// } else {
    ///     let response = fetch(&id).await;
    ///     cache.done(id, response.clone());
    ///     response
    /// }
    /// ```
    pub async fn register_or_wait(&self, key: &K) -> Option<V> {
        let entry = {
            let items = self.items.pin();
            match items.try_insert_with(key.clone(), || Arc::new(Value::new(None))) {
                Ok(_) => return None,
                Err(value) => {
                    if let Some(value) = value.get() {
                        return Some(value);
                    }
                    RegisteredEntry(Arc::clone(value))
                }
            }
        };
        Some(entry.wait().await)
    }

    /// Submit the result of a job you registered.
    pub fn done(&self, key: K, value: V) {
        let items = self.items.pin();
        let entry = items.get_or_insert_with(key, || Arc::new(Value::new(None)));
        *entry.lock() = Some(value);
        entry.notify.notify_waiters();
    }

    /// Wait for the result of a job that is running.
    ///
    /// Will hang if [`OnceMap::done`] isn't called for this key, or if `UnregisteredTask` is a
    /// non-fatal error and [`OnceMap::done`] isn't called for this key.
    pub async fn wait(&self, key: &K) -> Result<V, UnregisteredTask<K>> {
        self.register_or_wait(key)
            .await
            .ok_or_else(|| UnregisteredTask(key.clone()))
    }

    /// Wait for the result of a job that is running, in a blocking context.
    ///
    /// Will hang if [`OnceMap::done`] isn't called for this key, or if `UnregisteredTask` is a
    /// non-fatal error and [`OnceMap::done`] isn't called for this key.
    pub fn wait_blocking(&self, key: &K) -> Result<V, UnregisteredTask<K>> {
        futures::executor::block_on(self.register_or_wait(key))
            .ok_or_else(|| UnregisteredTask(key.clone()))
    }

    /// Return the result of a previous job, if any.
    pub fn get<Q: ?Sized + Hash + Eq>(&self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
    {
        let items = self.items.pin();
        items.get(key)?.get()
    }

    /// Remove the result of a previous job, if any.
    ///
    /// Existing handles retain the removed result. In-flight jobs must complete before removal.
    pub fn remove<Q: ?Sized + Hash + Eq>(&self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
    {
        let items = self.items.pin();
        items.remove(key)?.get()
    }
}

impl<K: Eq + Hash + Clone, V, H: Default + BuildHasher + Clone> Default for OnceMap<K, V, H> {
    fn default() -> Self {
        Self {
            items: HashMap::builder()
                .hasher(H::default())
                .resize_mode(ResizeMode::Blocking)
                .build(),
        }
    }
}

impl<K, V, H> FromIterator<(K, V)> for OnceMap<K, V, H>
where
    K: Eq + Hash,
    H: Default + Clone + BuildHasher,
{
    fn from_iter<T: IntoIterator<Item = (K, V)>>(iter: T) -> Self {
        Self {
            items: iter
                .into_iter()
                .map(|(k, v)| (k, Arc::new(Value::new(Some(v)))))
                .collect(),
        }
    }
}

/// Whether a job needs to be started or was already registered.
#[derive(Debug)]
pub enum Registration<V> {
    /// The caller must start the job and eventually call [`OnceMap::done`].
    New(RegisteredEntry<V>),
    /// The job is already running or has completed.
    Existing(RegisteredEntry<V>),
}

/// A handle to a registered job's result, independent of subsequent map lookups.
///
/// Cloning a handle shares the result slot. Waiting can be repeated and cannot fail due to an
/// unregistered key, but will hang if the job never completes.
#[derive(Clone, Debug)]
pub struct RegisteredEntry<V>(Arc<Value<V>>);

impl<V: Clone> RegisteredEntry<V> {
    /// Wait for the registered job to complete.
    pub async fn wait(&self) -> V {
        if let Some(value) = self.0.get() {
            return value;
        }
        loop {
            // Subscribe before checking the result so completion cannot miss this waiter.
            let notification = self.0.notify.notified();
            if let Some(value) = self.0.get() {
                return value;
            }
            notification.await;
        }
    }

    /// Wait for the registered job to complete in a blocking context.
    pub fn wait_blocking(&self) -> V {
        futures::executor::block_on(self.wait())
    }
}

/// The notification and result share one allocation throughout the job's lifetime.
#[derive(Debug)]
struct Value<V> {
    value: Mutex<Option<V>>,
    notify: Notify,
}

impl<V> Value<V> {
    fn new(value: Option<V>) -> Self {
        Self {
            value: Mutex::new(value),
            notify: Notify::new(),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Option<V>> {
        self.value.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl<V: Clone> Value<V> {
    fn get(&self) -> Option<V> {
        self.lock().clone()
    }
}
