use std::collections::HashSet;
use std::hash::Hash;

use tokio::sync::Mutex;
use waitmap::WaitMap;

// The `for<'a> From<&'a K>` bound exists to satisfy `WaitMap::wait`
pub struct InFlight<K: Eq + Hash + for<'a> From<&'a K>, V: Clone> {
    in_flight: Mutex<HashSet<K>>,
    waitmap: WaitMap<K, V>,
}

impl<K: Eq + Hash + for<'a> From<&'a K>, V: Clone> InFlight<K, V> {
    pub async fn wait_or_register(&self, key: K) -> Option<V> {
        let mut in_flight_lock = self.in_flight.lock().await;
        if in_flight_lock.contains(&key) {
            let value = self
                .waitmap
                .wait(&key)
                .await
                .expect("This operation is never cancelled")
                .value()
                .clone();
            Some(value)
        } else {
            in_flight_lock.insert(key);
            None
        }
    }

    pub fn submit_done(&self, key: K, value: V) {
        self.waitmap.insert(key, value);
    }
}

impl<K: Eq + Hash + for<'a> From<&'a K>, V: Clone> Default for InFlight<K, V> {
    fn default() -> Self {
        Self {
            in_flight: Mutex::new(HashSet::new()),
            waitmap: WaitMap::new(),
        }
    }
}
