use std::collections::VecDeque;
use std::collections::hash_map::Entry;
use std::hash::Hash;

use rustc_hash::FxHashMap;

/// Reachability metadata accumulated by a [`DependencyTraversal`].
///
/// Merging must be monotonic and convergent: implementations should return `true` only when `self`
/// gains new reachability from `other`, and repeated merges over a finite dependency graph must
/// eventually stop changing the accumulated value.
pub trait DependencyReachability: Clone {
    /// Merge another path's reachability into this one, returning whether `self` changed.
    fn merge(&mut self, other: Self) -> bool;
}

#[derive(Debug, Clone)]
struct TraversalState<Reachability> {
    reachability: Reachability,
    queued: bool,
}

/// A breadth-first traversal over package and activated-extra states.
///
/// By default, each `(package, extra)` pair is yielded at most once. Callers that need to track
/// changing reachability state can use [`DependencyTraversal::enqueue_reachable`] and
/// [`DependencyTraversal::walk_reachable`] to revisit states whenever their reachability grows.
/// The base package is represented by `None`; explicitly activated extras are represented by
/// `Some(extra)`.
#[derive(Debug, Clone)]
pub struct DependencyTraversal<Package, Extra, Reachability = ()> {
    queue: VecDeque<(Package, Option<Extra>)>,
    states: FxHashMap<(Package, Option<Extra>), TraversalState<Reachability>>,
}

impl<Package, Extra> DependencyTraversal<Package, Extra>
where
    Package: Clone + Eq + Hash,
    Extra: Clone + Eq + Hash,
{
    /// Enqueue a package state, returning whether it was newly discovered.
    pub fn enqueue(&mut self, package: Package, extra: Option<Extra>) -> bool {
        let key = (package, extra);
        match self.states.entry(key.clone()) {
            Entry::Occupied(_) => false,
            Entry::Vacant(entry) => {
                entry.insert(TraversalState {
                    reachability: (),
                    queued: true,
                });
                self.queue.push_back(key);
                true
            }
        }
    }

    /// Enqueue a package's base state followed by its explicitly activated extras.
    pub fn enqueue_package(
        &mut self,
        package: impl Into<Package>,
        extras: impl IntoIterator<Item = Extra>,
    ) {
        let package = package.into();
        self.enqueue(package.clone(), None);
        for extra in extras {
            self.enqueue(package.clone(), Some(extra));
        }
    }

    /// Visit each discovered state, including states enqueued by the visitor.
    pub fn walk(mut self, mut visit: impl FnMut(Package, Option<Extra>, &mut Self)) {
        while let Some((package, extra)) = self.queue.pop_front() {
            visit(package, extra, &mut self);
        }
    }

    /// Visit each discovered state, including states enqueued by the visitor, stopping when the
    /// visitor returns an error.
    pub fn try_walk<Error>(
        mut self,
        mut visit: impl FnMut(Package, Option<Extra>, &mut Self) -> Result<(), Error>,
    ) -> Result<(), Error> {
        while let Some((package, extra)) = self.queue.pop_front() {
            visit(package, extra, &mut self)?;
        }
        Ok(())
    }
}

impl<Package, Extra, Reachability> DependencyTraversal<Package, Extra, Reachability>
where
    Package: Clone + Eq + Hash,
    Extra: Clone + Eq + Hash,
    Reachability: DependencyReachability,
{
    /// Enqueue a package state with reachability metadata.
    ///
    /// If the state has already been discovered, the existing reachability is merged with the new
    /// reachability. The state is requeued when the merge changes the accumulated reachability.
    /// Returns whether the state was newly discovered or its accumulated reachability changed.
    pub fn enqueue_reachable(
        &mut self,
        package: Package,
        extra: Option<Extra>,
        reachability: Reachability,
    ) -> bool {
        let key = (package, extra);
        match self.states.entry(key.clone()) {
            Entry::Occupied(mut entry) => {
                let state = entry.get_mut();
                let changed = state.reachability.merge(reachability);
                if changed && !state.queued {
                    state.queued = true;
                    self.queue.push_back(key);
                }
                changed
            }
            Entry::Vacant(entry) => {
                entry.insert(TraversalState {
                    reachability,
                    queued: true,
                });
                self.queue.push_back(key);
                true
            }
        }
    }

    /// Enqueue a package's base state followed by its explicitly activated extras.
    pub fn enqueue_reachable_package(
        &mut self,
        package: impl Into<Package>,
        extras: impl IntoIterator<Item = Extra>,
        reachability: &Reachability,
    ) {
        let package = package.into();
        self.enqueue_reachable(package.clone(), None, reachability.clone());
        for extra in extras {
            self.enqueue_reachable(package.clone(), Some(extra), reachability.clone());
        }
    }

    /// Visit each discovered state, including states whose reachability changes during traversal.
    ///
    /// Reachability changes made before a pending visit are coalesced into that visit. Changes made
    /// after a state is visited requeue it with the latest accumulated reachability.
    pub fn walk_reachable(
        mut self,
        mut visit: impl FnMut(Package, Option<Extra>, Reachability, &mut Self),
    ) {
        while let Some((package, extra)) = self.queue.pop_front() {
            let Some(state) = self.states.get_mut(&(package.clone(), extra.clone())) else {
                continue;
            };
            state.queued = false;
            let reachability = state.reachability.clone();
            visit(package, extra, reachability, &mut self);
        }
    }
}

impl<Package, Extra, Reachability> Default for DependencyTraversal<Package, Extra, Reachability> {
    fn default() -> Self {
        Self {
            queue: VecDeque::new(),
            states: FxHashMap::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DependencyReachability, DependencyTraversal};

    #[derive(Debug, Clone, Copy, Eq, PartialEq)]
    struct BitReachability(u8);

    impl DependencyReachability for BitReachability {
        fn merge(&mut self, other: Self) -> bool {
            let previous = *self;
            self.0 |= other.0;
            *self != previous
        }
    }

    #[test]
    fn base_precedes_extras_and_states_are_unique() {
        let mut traversal = DependencyTraversal::default();
        traversal.enqueue_package("package", ["foo", "bar", "foo"]);

        let mut visited = Vec::new();
        traversal.walk(|package, extra, _| visited.push((package, extra)));
        assert_eq!(
            visited,
            vec![
                ("package", None),
                ("package", Some("foo")),
                ("package", Some("bar")),
            ]
        );
    }

    #[test]
    fn reenqueueing_a_cycle_does_not_revisit_states() {
        let mut traversal: DependencyTraversal<&str, &str> = DependencyTraversal::default();
        traversal.enqueue_package("a", []);

        let mut visited = Vec::new();
        traversal.walk(|package, extra, traversal| {
            visited.push((package, extra));
            match package {
                "a" => traversal.enqueue_package("b", []),
                "b" => traversal.enqueue_package("a", []),
                _ => {}
            }
        });

        assert_eq!(visited, vec![("a", None), ("b", None)]);
    }

    #[test]
    fn try_walk_stops_on_error() {
        let mut traversal: DependencyTraversal<&str, &str> = DependencyTraversal::default();
        traversal.enqueue_package("a", []);

        let mut visited = Vec::new();
        let result = traversal.try_walk(|package, extra, traversal| {
            visited.push((package, extra));
            traversal.enqueue_package("b", []);
            Err("stop")
        });

        assert_eq!(result, Err("stop"));
        assert_eq!(visited, vec![("a", None)]);
    }

    #[test]
    fn merged_reachability_revisits_states() {
        let mut traversal: DependencyTraversal<&str, &str, BitReachability> =
            DependencyTraversal::default();
        traversal.enqueue_reachable("a", None, BitReachability(0b001));

        let mut visited = Vec::new();
        traversal.walk_reachable(|package, extra, reachability, traversal| {
            visited.push((package, extra, reachability));
            if reachability == BitReachability(0b001) {
                traversal.enqueue_reachable(package, extra, BitReachability(0b010));
            }
        });

        assert_eq!(
            visited,
            vec![
                ("a", None, BitReachability(0b001)),
                ("a", None, BitReachability(0b011)),
            ]
        );
    }

    #[test]
    fn merged_reachability_updates_a_queued_state() {
        let mut traversal: DependencyTraversal<&str, &str, BitReachability> =
            DependencyTraversal::default();
        traversal.enqueue_reachable("a", None, BitReachability(0b001));
        traversal.enqueue_reachable("a", None, BitReachability(0b010));

        let mut visited = Vec::new();
        traversal.walk_reachable(|package, extra, reachability, _| {
            visited.push((package, extra, reachability));
        });

        assert_eq!(visited, vec![("a", None, BitReachability(0b011))]);
    }

    #[test]
    fn reachable_package_base_precedes_extras() {
        let mut traversal: DependencyTraversal<&str, &str, BitReachability> =
            DependencyTraversal::default();
        traversal.enqueue_reachable_package(
            "package",
            ["foo", "bar", "foo"],
            &BitReachability(0b001),
        );

        let mut visited = Vec::new();
        traversal.walk_reachable(|package, extra, reachability, _| {
            visited.push((package, extra, reachability));
        });

        assert_eq!(
            visited,
            vec![
                ("package", None, BitReachability(0b001)),
                ("package", Some("foo"), BitReachability(0b001)),
                ("package", Some("bar"), BitReachability(0b001)),
            ]
        );
    }

    #[test]
    fn unchanged_reachability_does_not_revisit_a_state() {
        let mut traversal: DependencyTraversal<&str, &str, BitReachability> =
            DependencyTraversal::default();
        traversal.enqueue_reachable("a", None, BitReachability(0b001));

        let mut visited = Vec::new();
        traversal.walk_reachable(|package, extra, reachability, traversal| {
            visited.push((package, extra, reachability));
            traversal.enqueue_reachable(package, extra, reachability);
        });

        assert_eq!(visited, vec![("a", None, BitReachability(0b001))]);
    }

    #[test]
    fn reachability_growth_propagates_through_a_cycle() {
        let mut traversal: DependencyTraversal<&str, &str, BitReachability> =
            DependencyTraversal::default();
        traversal.enqueue_reachable("a", None, BitReachability(0b001));

        let mut visited = Vec::new();
        traversal.walk_reachable(|package, extra, reachability, traversal| {
            visited.push((package, extra, reachability));
            match package {
                "a" => {
                    traversal.enqueue_reachable("b", None, reachability);
                }
                "b" => {
                    traversal.enqueue_reachable("a", None, reachability);
                    if reachability == BitReachability(0b001) {
                        traversal.enqueue_reachable("a", None, BitReachability(0b010));
                    }
                }
                _ => {}
            }
        });

        assert_eq!(
            visited,
            vec![
                ("a", None, BitReachability(0b001)),
                ("b", None, BitReachability(0b001)),
                ("a", None, BitReachability(0b011)),
                ("b", None, BitReachability(0b011)),
            ]
        );
    }
}
