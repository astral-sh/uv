use std::time::Instant;

/// A metrics counter storing only latest `N` records.
pub struct MetricsCounter<const N: usize> {
    /// Slots to store metrics.
    slots: [(usize, Instant); N],
    /// The slot of the oldest record.
    /// Also the next slot to store the new record.
    index: usize,
}

impl<const N: usize> MetricsCounter<N> {
    /// Creates a new counter with an initial value.
    pub fn new(init: usize, init_at: Instant) -> Self {
        assert!(N > 0, "number of slots must be greater than zero");
        Self {
            slots: [(init, init_at); N],
            index: 0,
        }
    }

    /// Adds record to the counter.
    pub fn add(&mut self, data: usize, added_at: Instant) {
        self.slots[self.index] = (data, added_at);
        self.index = (self.index + 1) % N;
    }

    /// Calculates per-second average rate of all slots.
    pub fn rate(&self) -> f32 {
        let latest = self.slots[self.index.checked_sub(1).unwrap_or(N - 1)];
        let oldest = self.slots[self.index];
        let duration = (latest.1 - oldest.1).as_secs_f32();
        let avg = (latest.0 - oldest.0) as f32 / duration;
        if f32::is_nan(avg) {
            0f32
        } else {
            avg
        }
    }
}

#[cfg(test)]
mod tests {
    use super::MetricsCounter;
    use std::time::{Duration, Instant};

    #[test]
    fn counter() {
        let now = Instant::now();
        let mut counter = MetricsCounter::<3>::new(0, now);
        assert_eq!(counter.rate(), 0f32);
        counter.add(1, now + Duration::from_secs(1));
        assert_eq!(counter.rate(), 1f32);
        counter.add(4, now + Duration::from_secs(2));
        assert_eq!(counter.rate(), 2f32);
        counter.add(7, now + Duration::from_secs(3));
        assert_eq!(counter.rate(), 3f32);
        counter.add(12, now + Duration::from_secs(4));
        assert_eq!(counter.rate(), 4f32);
    }

    #[test]
    #[should_panic(expected = "number of slots must be greater than zero")]
    fn counter_zero_slot() {
        let _counter = MetricsCounter::<0>::new(0, Instant::now());
    }
}
