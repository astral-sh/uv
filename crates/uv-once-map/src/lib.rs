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
    items: HashMap<K, Value<V>, S>,
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
            .try_insert(key, Value::Waiting(Arc::new(Notify::new())))
            .is_ok()
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
        let notify = {
            let items = self.items.pin();
            match items.try_insert_with(key.clone(), || Value::Waiting(Arc::new(Notify::new()))) {
                Ok(_) => return None,
                Err(value) => match value {
                    Value::Filled(_) => return value.get(),
                    Value::Waiting(notify) => notify.clone(),
                },
            }
        };

        // Register the waiter for calls to `notify_waiters`.
        let notification = notify.notified();

        // Make sure the value wasn't inserted in-between us checking the map and registering the waiter.
        if let Some(value) = self.items.pin().get(key).expect("map is append-only").get() {
            return Some(value);
        }

        // Wait until the value is inserted.
        notification.await;

        let items = self.items.pin();
        let value = items.get(key).expect("map is append-only");
        match value {
            Value::Filled(_) => value.get(),
            Value::Waiting(_) => unreachable!("notify was called"),
        }
    }

    /// Submit the result of a job you registered.
    pub fn done(&self, key: K, value: V) {
        if let Some(Value::Waiting(notify)) = self.items.pin().insert(key, Value::filled(value)) {
            notify.notify_waiters();
        }
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
    pub fn remove<Q: ?Sized + Hash + Eq>(&self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
    {
        let items = self.items.pin();
        items.remove(key)?.take()
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
                .map(|(k, v)| (k, Value::filled(v)))
                .collect(),
        }
    }
}

#[derive(Debug)]
enum Value<V> {
    Waiting(Arc<Notify>),
    /// The mutex is a workaround to papaya always returning borrowed instead of owned values.
    Filled(Mutex<Option<V>>),
}

impl<V> Value<V> {
    fn filled(value: V) -> Self {
        Self::Filled(Mutex::new(Some(value)))
    }

    fn lock(value: &Mutex<Option<V>>) -> MutexGuard<'_, Option<V>> {
        value.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn take(&self) -> Option<V> {
        match self {
            Self::Filled(value) => Self::lock(value).take(),
            Self::Waiting(_) => None,
        }
    }
}

impl<V: Clone> Value<V> {
    fn get(&self) -> Option<V> {
        match self {
            Self::Filled(value) => Self::lock(value).clone(),
            Self::Waiting(_) => None,
        }
    }
}
