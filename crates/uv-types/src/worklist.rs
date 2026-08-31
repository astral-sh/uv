use std::collections::VecDeque;
use std::hash::Hash;

use rustc_hash::FxHashSet;

/// A first-in, first-out worklist that visits each value at most once.
///
/// Values are marked as visited when they are pushed, so duplicate values do not accumulate in
/// the queue while an earlier copy is still pending.
#[derive(Debug)]
pub struct Worklist<T> {
    queue: VecDeque<T>,
    seen: FxHashSet<T>,
}

impl<T> Default for Worklist<T> {
    fn default() -> Self {
        Self {
            queue: VecDeque::new(),
            seen: FxHashSet::default(),
        }
    }
}

impl<T> Worklist<T>
where
    T: Clone + Eq + Hash,
{
    /// Push a value onto the worklist if it has not been visited.
    ///
    /// Returns `true` if the value was added.
    pub fn push(&mut self, value: T) -> bool {
        if self.seen.insert(value.clone()) {
            self.queue.push_back(value);
            true
        } else {
            false
        }
    }

    /// Pop the next value from the worklist.
    pub fn pop(&mut self) -> Option<T> {
        self.queue.pop_front()
    }
}

#[cfg(test)]
mod tests {
    use super::Worklist;

    #[test]
    fn visits_each_value_once() {
        let mut worklist = Worklist::default();

        assert!(worklist.push("a"));
        assert!(worklist.push("b"));
        assert!(!worklist.push("a"));

        assert_eq!(worklist.pop(), Some("a"));
        assert!(!worklist.push("a"));
        assert_eq!(worklist.pop(), Some("b"));
        assert_eq!(worklist.pop(), None);
    }
}
