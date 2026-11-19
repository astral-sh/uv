use std::borrow::Borrow;
use std::fmt::{self, Debug};
use std::hash::{BuildHasher, Hash, RandomState};

use crate::OnceMap;

/// A [`OnceMap`] with registered entry handles.
/// Entries can only be removed with exclusive access.
///
/// Registration and completion use shared references, so jobs can run concurrently. Registered
/// handles borrow the map, preventing removal until they are dropped.
///
/// ```compile_fail,E0502
/// use uv_once_map::RegisteredOnceMap;
///
/// let mut map = RegisteredOnceMap::<_, _>::default();
/// map.done("package", 42);
/// let entry = map.get_registered("package").expect("completed entry");
/// map.remove(&"package"); // Cannot remove entries while a handle borrows the map.
/// assert_eq!(entry.wait_blocking(), 42);
/// ```
///
/// Shared ownership does not grant the ability to remove entries:
///
/// ```compile_fail,E0596
/// use std::sync::Arc;
/// use uv_once_map::RegisteredOnceMap;
///
/// let map = Arc::new(RegisteredOnceMap::<_, _>::default());
/// map.done("package", 42);
/// let mut alias = Arc::clone(&map);
/// alias.remove(&"package"); // Requires exclusive access to the backing map.
/// ```
pub struct RegisteredOnceMap<K, V, S = RandomState>(OnceMap<K, V, S>);

impl<K: Eq + Hash + Debug, V: Debug, S: BuildHasher + Clone> Debug for RegisteredOnceMap<K, V, S> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl<K: Eq + Hash + Clone, V: Clone, S: BuildHasher + Clone> RegisteredOnceMap<K, V, S> {
    /// Register a job without retaining a handle. A `true` result requires the caller to start
    /// the job and eventually call [`Self::done`].
    pub fn register(&self, key: K) -> bool {
        self.0.register(key)
    }

    /// Register a job and retain its identity. [`Registration::New`] requires the caller to
    /// start the job and eventually call [`Self::done`].
    pub fn register_entry(&self, key: K) -> Registration<RegisteredEntry<'_, K, V, S>> {
        let registered = self.0.register(key.clone());
        let entry = RegisteredEntry { map: &self.0, key };
        if registered {
            Registration::New(entry)
        } else {
            Registration::Existing(entry)
        }
    }

    /// Look up an existing registration without inserting or waiting on an absent entry.
    pub fn get_registered(&self, key: K) -> Option<RegisteredEntry<'_, K, V, S>> {
        self.0
            .items
            .pin()
            .contains_key(&key)
            .then_some(RegisteredEntry { map: &self.0, key })
    }

    /// Register a new job, returning `None`, or wait for an existing job's result.
    pub async fn register_or_wait(&self, key: &K) -> Option<V> {
        self.0.register_or_wait(key).await
    }

    /// Submit the result of a job, or populate an entry before registration.
    pub fn done(&self, key: K, value: V) {
        self.0.done(key, value);
    }

    /// Return the result of a completed job, if any.
    pub fn get<Q: ?Sized + Hash + Eq>(&self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
    {
        self.0.get(key)
    }

    /// Remove a job with exclusive access, after all registered handles have been dropped.
    pub fn remove<Q: ?Sized + Hash + Eq>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
    {
        self.0.remove(key)
    }
}

impl<K: Eq + Hash + Clone, V, S: Default + BuildHasher + Clone> Default
    for RegisteredOnceMap<K, V, S>
{
    fn default() -> Self {
        Self(OnceMap::default())
    }
}

impl<K: Eq + Hash, V, S: Default + BuildHasher + Clone> FromIterator<(K, V)>
    for RegisteredOnceMap<K, V, S>
{
    fn from_iter<T: IntoIterator<Item = (K, V)>>(iter: T) -> Self {
        Self(OnceMap::from_iter(iter))
    }
}

/// Whether the caller must start a job or can share its existing registration.
#[derive(Debug)]
pub enum Registration<T> {
    New(T),
    Existing(T),
}

/// A registered job in a map that cannot remove entries while borrowed.
#[derive(Clone)]
pub struct RegisteredEntry<'a, K, V, S = RandomState> {
    map: &'a OnceMap<K, V, S>,
    key: K,
}

impl<K: Debug, V, S> Debug for RegisteredEntry<'_, K, V, S> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("RegisteredEntry")
            .field(&self.key)
            .finish()
    }
}

impl<K: Eq + Hash + Clone, V: Clone, S: BuildHasher + Clone> RegisteredEntry<'_, K, V, S> {
    pub fn key(&self) -> &K {
        &self.key
    }

    /// Wait for the registered job. The producer must eventually call [`RegisteredOnceMap::done`].
    pub async fn wait(&self) -> V {
        // Only RegisteredOnceMap constructs handles, and removal requires an exclusive borrow.
        self.map
            .wait_registered(&self.key)
            .await
            .expect("registered entries cannot be removed while borrowed")
    }

    /// Wait for the registered job in a blocking context.
    pub fn wait_blocking(&self) -> V {
        futures::executor::block_on(self.wait())
    }
}
