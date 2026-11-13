use std::borrow::Borrow;
use std::fmt::{self, Debug};
use std::hash::{BuildHasher, Hash, RandomState};
use std::marker::PhantomData;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use papaya::{Compute, Guard, HashMap, Operation, ResizeMode};
use tokio::sync::Notify;

/// The terminal outcome of one attempt. A pending slot has no outcome yet.
#[derive(Debug)]
enum Outcome<V> {
    // Papaya removes entries by reference. Taking the value under exclusive map access
    // releases it immediately, even if the map defers reclaiming the slot.
    Ready(Mutex<Option<V>>),
    Abandoned,
}

#[derive(Debug)]
struct Slot<V> {
    outcome: OnceLock<Outcome<V>>,
    notify: Notify,
}

impl<V> Slot<V> {
    fn pending() -> Self {
        Self {
            outcome: OnceLock::new(),
            notify: Notify::new(),
        }
    }

    fn ready(value: V) -> Self {
        Self {
            outcome: OnceLock::from(Outcome::Ready(Mutex::new(Some(value)))),
            notify: Notify::new(),
        }
    }

    fn publish(&self, outcome: Outcome<V>) {
        // Independently supplied metadata may already have satisfied this request.
        if self.outcome.set(outcome).is_ok() {
            self.notify.notify_waiters();
        }
    }
}

impl<V: Clone> Slot<V> {
    fn get(&self) -> Option<V> {
        match self.outcome.get()? {
            Outcome::Ready(value) => value.lock().unwrap_or_else(PoisonError::into_inner).clone(),
            Outcome::Abandoned => None,
        }
    }

    async fn wait(&self) -> Option<V> {
        // Register before checking the outcome so publication between the check and the await
        // also wakes this waiter.
        let notified = self.notify.notified();
        if self.outcome.get().is_none() {
            notified.await;
        }
        self.get()
    }
}

/// The sole producer for one registration. Dropping it wakes waiters and allows a new attempt.
#[derive(Debug)]
#[must_use = "dropping the completion abandons the request"]
pub struct Completion<V>(Arc<Slot<V>>);

impl<V> Completion<V> {
    /// Publish the result to every waiter on this attempt.
    pub fn finish(self, value: V) {
        self.0.publish(Outcome::Ready(Mutex::new(Some(value))));
    }
}

impl<V> Drop for Completion<V> {
    fn drop(&mut self) {
        self.0.publish(Outcome::Abandoned);
    }
}

/// A map of shared jobs with owned completion handles.
/// Entries can only be removed with exclusive access.
///
/// Registration and completion use shared references, so jobs can run concurrently. Registered
/// handles borrow the map, preventing removal until they are dropped.
///
/// ```compile_fail,E0502
/// use uv_once_map::RegisteredOnceMap;
///
/// let mut map = RegisteredOnceMap::<_, _>::default();
/// map.insert("package", 42);
/// let entry = map.get_registered("package").expect("completed entry");
/// map.remove(&"package"); // Cannot remove entries while a handle borrows the map.
/// assert_eq!(entry.wait_blocking(), Some(42));
/// ```
///
/// Shared ownership does not grant the ability to remove entries:
///
/// ```compile_fail,E0596
/// use std::sync::Arc;
/// use uv_once_map::RegisteredOnceMap;
///
/// let map = Arc::new(RegisteredOnceMap::<_, _>::default());
/// map.insert("package", 42);
/// let mut alias = Arc::clone(&map);
/// alias.remove(&"package"); // Requires exclusive access to the backing map.
/// ```
pub struct RegisteredOnceMap<K, V, S = RandomState>(HashMap<K, Arc<Slot<V>>, S>);

impl<K: Eq + Hash + Debug, V: Debug, S: BuildHasher + Clone> Debug for RegisteredOnceMap<K, V, S> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl<K: Eq + Hash + Clone, V: Clone, S: BuildHasher + Clone> RegisteredOnceMap<K, V, S> {
    fn register_slot<'g>(
        &'g self,
        key: K,
        guard: &'g impl Guard,
    ) -> (&'g Arc<Slot<V>>, Option<Completion<V>>) {
        match self.0.compute(
            key,
            |entry| {
                if let Some((_, slot)) = entry
                    && !matches!(slot.outcome.get(), Some(Outcome::Abandoned))
                {
                    return Operation::Abort(slot);
                }
                Operation::Insert(Arc::new(Slot::pending()))
            },
            guard,
        ) {
            Compute::Aborted(slot) => (slot, None),
            Compute::Inserted(_, slot) | Compute::Updated { new: (_, slot), .. } => {
                (slot, Some(Completion(slot.clone())))
            }
            Compute::Removed(..) => unreachable!("registration only inserts or replaces a slot"),
        }
    }

    /// Claim an unstarted or abandoned job without retaining a waiter.
    pub fn register(&self, key: K) -> Option<Completion<V>> {
        self.register_slot(key, &self.0.guard()).1
    }

    /// Register a waiter and, for a new attempt, return the handle that must travel with the job.
    pub fn register_entry(&self, key: K) -> (RegisteredEntry<'_, K, V>, Option<Completion<V>>) {
        let guard = self.0.guard();
        let (slot, completion) = self.register_slot(key.clone(), &guard);
        (
            RegisteredEntry {
                key,
                slot: slot.clone(),
                _map: PhantomData,
            },
            completion,
        )
    }

    /// Look up a registration without starting a job.
    pub fn get_registered(&self, key: K) -> Option<RegisteredEntry<'_, K, V>> {
        let slot = self.0.pin().get(&key)?.clone();
        Some(RegisteredEntry {
            key,
            slot,
            _map: PhantomData,
        })
    }

    /// Compute a missing result or share another producer's result. If that producer is
    /// cancelled, compete for a new attempt.
    pub async fn get_or_try_init<E>(
        &self,
        mut key: K,
        init: impl AsyncFnOnce() -> Result<V, E>,
    ) -> Result<V, E> {
        loop {
            let (entry, completion) = self.register_entry(key);
            if let Some(completion) = completion {
                let value = init().await?;
                completion.finish(value.clone());
                return Ok(value);
            }
            if let Some(value) = entry.wait().await {
                return Ok(value);
            }
            key = entry.key;
        }
    }

    /// Supply an independently obtained result, including to waiters on an outstanding request.
    pub fn insert(&self, mut key: K, value: V) {
        let mut value = Some(value);
        loop {
            let (entry, _completion) = self.register_entry(key);
            match entry
                .slot
                .outcome
                .get_or_init(|| Outcome::Ready(Mutex::new(value.take())))
            {
                Outcome::Ready(current) => {
                    if let Some(value) = value {
                        *current.lock().unwrap_or_else(PoisonError::into_inner) = Some(value);
                    } else {
                        entry.slot.notify.notify_waiters();
                    }
                    return;
                }
                // The registered producer may have been dropped before we supplied its result.
                Outcome::Abandoned => key = entry.key,
            }
        }
    }

    pub fn get<Q: ?Sized + Hash + Eq>(&self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
    {
        self.0.pin().get(key)?.get()
    }

    /// Remove a job with exclusive access, after registered waiters have been dropped.
    pub fn remove<Q: ?Sized + Hash + Eq>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
    {
        match self.0.pin().remove(key)?.outcome.get()? {
            Outcome::Ready(value) => value.lock().unwrap_or_else(PoisonError::into_inner).take(),
            Outcome::Abandoned => None,
        }
    }
}

impl<K: Eq + Hash, V, S: Default + BuildHasher + Clone> Default for RegisteredOnceMap<K, V, S> {
    fn default() -> Self {
        Self(
            HashMap::builder()
                .hasher(S::default())
                .resize_mode(ResizeMode::Blocking)
                .build(),
        )
    }
}

impl<K: Eq + Hash, V, S: Default + BuildHasher + Clone> FromIterator<(K, V)>
    for RegisteredOnceMap<K, V, S>
{
    fn from_iter<T: IntoIterator<Item = (K, V)>>(iter: T) -> Self {
        Self(
            iter.into_iter()
                .map(|(k, v)| (k, Arc::new(Slot::ready(v))))
                .collect(),
        )
    }
}

/// A waiter bound to one attempt. Borrowing the map prevents removal while a caller retains a pin.
#[derive(Clone)]
pub struct RegisteredEntry<'map, K, V> {
    key: K,
    slot: Arc<Slot<V>>,
    _map: PhantomData<&'map ()>,
}

impl<K: Debug, V> Debug for RegisteredEntry<'_, K, V> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("RegisteredEntry")
            .field(&self.key)
            .finish()
    }
}

impl<K, V: Clone> RegisteredEntry<'_, K, V> {
    pub fn key(&self) -> &K {
        &self.key
    }

    /// Wait for this attempt. Returns `None` if its producer was dropped.
    pub async fn wait(&self) -> Option<V> {
        self.slot.wait().await
    }

    pub fn wait_blocking(&self) -> Option<V> {
        futures::executor::block_on(self.wait())
    }
}
