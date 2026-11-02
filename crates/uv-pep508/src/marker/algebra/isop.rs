//! Extract a sum-of-products cover directly from a decision diagram.
//!
//! Boolean decisions use the Minato-Morreale ISOP recurrence. For range-valued decisions,
//! split the lower and upper bounds into matching ranges and extract the part that can omit
//! the decision from all branches. This preserves the function, but does not guarantee a
//! prime, irredundant cover over arbitrary unions of ranges.
//!
//! See <https://people.eecs.berkeley.edu/~alanmi/publications/2003/iwls03_sop.pdf>, section 4.

use std::cmp::Ordering;
use std::ops::Bound;
use std::rc::Rc;

use indexmap::IndexMap;
use indexmap::map::Entry;
use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};
use version_ranges::Ranges;

use super::{Edges, InternerGuard, NodeId, SmallVec};

/// A shared cover whose paths are expanded only when producing the final DNF.
#[derive(Clone)]
pub(crate) enum Cover {
    False,
    True,
    Guard(NodeId),
    Branches(Rc<[(Guard, Self)]>),
}

/// A single-variable condition, either interned or borrowed from an input decision.
#[derive(Clone, Copy)]
pub(crate) enum Guard {
    Node(NodeId),
    Edge { node: NodeId, child: NodeId },
}

impl InternerGuard<'_> {
    /// Extract a cover without materializing the decision diagram's paths.
    pub(crate) fn isop(&mut self, marker: NodeId) -> Cover {
        self.isop_cached(marker, marker, &mut FxHashMap::default())
            .1
    }

    /// Return a cover and its function `I`, where `lower => I => upper`.
    /// The interval allows recursive calls to use already covered cases as don't-cares.
    fn isop_cached(
        &mut self,
        lower: NodeId,
        upper: NodeId,
        cache: &mut FxHashMap<(NodeId, NodeId), (NodeId, Cover)>,
    ) -> (NodeId, Cover) {
        if lower.is_false() {
            return (NodeId::FALSE, Cover::False);
        }
        if upper.is_true() {
            return (NodeId::TRUE, Cover::True);
        }
        debug_assert!(
            !upper.is_false() && !lower.is_true(),
            "invalid ISOP interval: {lower:?}..{upper:?}"
        );
        if let Some(result) = cache.get(&(lower, upper)) {
            return result.clone();
        }
        // A single decision is already a cover; its range renderer merges equal branches.
        if lower == upper
            && self
                .shared
                .node(lower)
                .children
                .nodes()
                .all(|child| child.is_true() || child.is_false())
        {
            return (lower, Cover::Guard(lower));
        }

        let exact = lower == upper;
        let cofactors = self.cover_cofactors(lower, upper);
        // A clause can omit this variable only where every branch permits it.
        let common_upper = cofactors
            .iter()
            .fold(NodeId::TRUE, |common, &(_, _, upper)| {
                self.and(common, upper)
            });
        let mut common_lower = NodeId::FALSE;
        let mut function = NodeId::FALSE;
        let mut branches = IndexMap::<_, _, FxBuildHasher>::default();
        for (guard, lower, upper) in cofactors {
            let required = self.and(lower, common_upper.not());
            let (specific, cover) = self.isop_cached(required, upper, cache);
            let remaining = self.and(lower, specific.not());
            common_lower = self.or(common_lower, remaining);
            if specific.is_false() {
                continue;
            }
            if !exact {
                let condition = self.guard_node(guard);
                let term = self.and(condition, specific);
                function = self.or(function, term);
            }
            // Equivalent suffixes can share a union of ranges, including inequalities.
            match branches.entry(specific) {
                Entry::Vacant(entry) => {
                    entry.insert((guard, cover));
                }
                Entry::Occupied(mut entry) => {
                    let left = self.guard_node(entry.get().0);
                    let right = self.guard_node(guard);
                    entry.get_mut().0 = Guard::Node(self.or(left, right));
                }
            }
        }
        let (common, cover) = self.isop_cached(common_lower, common_upper, cache);
        // Equal bounds already determine the function; only its cover needs rebuilding.
        function = if exact {
            lower
        } else {
            self.or(function, common)
        };
        if !common.is_false() {
            branches.insert(common, (Guard::Node(NodeId::TRUE), cover));
        }
        let result = (function, Cover::Branches(branches.into_values().collect()));
        cache.insert((lower, upper), result.clone());
        result
    }

    /// Materialize a borrowed guard only when algebra needs its function.
    fn guard_node(&mut self, guard: Guard) -> NodeId {
        match guard {
            Guard::Node(node) => node,
            Guard::Edge { node, child } => {
                let decision = self.shared.node(node);
                let edges = match &decision.children {
                    Edges::Boolean { high, .. } => Edges::from_bool(high.negate(node) == child),
                    Edges::Version { edges } => Edges::Version {
                        edges: guard_edges(edges, node, child),
                    },
                    Edges::String { edges } => Edges::String {
                        edges: guard_edges(edges, node, child),
                    },
                };
                self.create_node(decision.var.clone(), edges)
            }
        }
    }

    /// Partition the first variable into guards and matching lower and upper cofactors.
    /// Every cofactor omits this variable, so recursive extraction always makes progress.
    fn cover_cofactors(
        &mut self,
        lower: NodeId,
        upper: NodeId,
    ) -> SmallVec<(Guard, NodeId, NodeId)> {
        // Identical bounds already share a partition. Keep references to its edges rather
        // than constructing a decision node for every condition.
        if lower == upper {
            let node = self.shared.node(lower);
            if let Edges::Boolean { low, high } = node.children {
                return [low, high]
                    .into_iter()
                    .map(|child| child.negate(lower))
                    .map(|child| (Guard::Edge { node: lower, child }, child, child))
                    .collect();
            }
            let mut seen = FxHashSet::default();
            return node
                .children
                .nodes()
                .map(|child| child.negate(lower))
                .filter(|child| seen.insert(*child))
                .map(|child| (Guard::Edge { node: lower, child }, child, child))
                .collect();
        }
        let lower_node = self.shared.node(lower);
        let upper_node = self.shared.node(upper);
        let variable = (&lower_node.var).min(&upper_node.var);
        let first = if lower_node.var == *variable {
            lower_node
        } else {
            upper_node
        };
        match &first.children {
            Edges::Boolean { .. } => {
                let (lower_low, lower_high) = if let Edges::Boolean { high, low } =
                    lower_node.children
                    && lower_node.var == *variable
                {
                    (low.negate(lower), high.negate(lower))
                } else {
                    (lower, lower)
                };
                let (upper_low, upper_high) = if let Edges::Boolean { high, low } =
                    upper_node.children
                    && upper_node.var == *variable
                {
                    (low.negate(upper), high.negate(upper))
                } else {
                    (upper, upper)
                };
                let guard = self.create_node(variable.clone(), Edges::from_bool(true));
                smallvec::smallvec![
                    (Guard::Node(guard.not()), lower_low, upper_low),
                    (Guard::Node(guard), lower_high, upper_high),
                ]
            }
            Edges::Version { .. } => {
                let lower_edges = if let Edges::Version { edges } = &lower_node.children
                    && lower_node.var == *variable
                {
                    Some(edges.as_ref())
                } else {
                    None
                };
                let upper_edges = if let Edges::Version { edges } = &upper_node.children
                    && upper_node.var == *variable
                {
                    Some(edges.as_ref())
                } else {
                    None
                };
                range_cofactors(lower, lower_edges, upper, upper_edges)
                    .into_iter()
                    .map(|(range, lower, upper)| {
                        if lower.is_false() {
                            return (Guard::Node(NodeId::FALSE), lower, upper);
                        }
                        let guard = self.create_node(
                            variable.clone(),
                            Edges::Version {
                                edges: Edges::from_range(&range),
                            },
                        );
                        (Guard::Node(guard), lower, upper)
                    })
                    .collect()
            }
            Edges::String { .. } => {
                let lower_edges = if let Edges::String { edges } = &lower_node.children
                    && lower_node.var == *variable
                {
                    Some(edges.as_ref())
                } else {
                    None
                };
                let upper_edges = if let Edges::String { edges } = &upper_node.children
                    && upper_node.var == *variable
                {
                    Some(edges.as_ref())
                } else {
                    None
                };
                range_cofactors(lower, lower_edges, upper, upper_edges)
                    .into_iter()
                    .map(|(range, lower, upper)| {
                        if lower.is_false() {
                            return (Guard::Node(NodeId::FALSE), lower, upper);
                        }
                        let guard = self.create_node(
                            variable.clone(),
                            Edges::String {
                                edges: Edges::from_range(&range),
                            },
                        );
                        (Guard::Node(guard), lower, upper)
                    })
                    .collect()
            }
        }
    }
}

/// Build a reduced single-variable decision for all edges leading to the selected child.
fn guard_edges<T: Clone + Ord>(
    edges: &[(Ranges<T>, NodeId)],
    parent: NodeId,
    child: NodeId,
) -> SmallVec<(Ranges<T>, NodeId)> {
    let range = edges
        .iter()
        .filter(|(_, candidate)| candidate.negate(parent) == child)
        .flat_map(|(range, _)| range.iter())
        .map(|(start, end)| (start.cloned(), end.cloned()))
        .collect();
    Edges::from_range(&range)
}

/// Intersect sorted range partitions, grouping regions with identical cofactors.
fn range_cofactors<T: Clone + Ord>(
    lower: NodeId,
    lower_edges: Option<&[(Ranges<T>, NodeId)]>,
    upper: NodeId,
    upper_edges: Option<&[(Ranges<T>, NodeId)]>,
) -> Vec<(Ranges<T>, NodeId, NodeId)> {
    let lower_constant = [(Ranges::full(), lower)];
    let upper_constant = [(Ranges::full(), upper)];
    let (lower_edges, lower_parent) =
        lower_edges.map_or((&lower_constant[..], NodeId::TRUE), |edges| (edges, lower));
    let (upper_edges, upper_parent) =
        upper_edges.map_or((&upper_constant[..], NodeId::TRUE), |edges| (edges, upper));
    let mut regions = IndexMap::<_, Ranges<T>, FxBuildHasher>::default();
    let mut left = lower_edges.iter().peekable();
    let mut right = upper_edges.iter().peekable();
    while let (Some((lower_range, lower)), Some((upper_range, upper))) = (left.peek(), right.peek())
    {
        let intersection = lower_range.intersection(upper_range);
        if !intersection.is_empty() {
            let cofactors = (lower.negate(lower_parent), upper.negate(upper_parent));
            regions
                .entry(cofactors)
                .and_modify(|range| *range = range.union(&intersection))
                .or_insert(intersection);
        }
        let lower_end = lower_range.bounding_range().map(|(_, end)| end);
        let upper_end = upper_range.bounding_range().map(|(_, end)| end);
        match (lower_end, upper_end) {
            (Some(lower_end), Some(upper_end)) => match compare_ends(lower_end, upper_end) {
                Ordering::Less => {
                    left.next();
                }
                Ordering::Equal => {
                    left.next();
                    right.next();
                }
                Ordering::Greater => {
                    right.next();
                }
            },
            (None, _) => {
                left.next();
            }
            (_, None) => {
                right.next();
            }
        }
    }
    regions
        .into_iter()
        .map(|((lower, upper), range)| (range, lower, upper))
        .collect()
}

/// Compare upper bounds, placing an included endpoint after an excluded endpoint.
fn compare_ends<T: Ord>(left: Bound<&T>, right: Bound<&T>) -> Ordering {
    match (left, right) {
        (Bound::Unbounded, Bound::Unbounded) => Ordering::Equal,
        (Bound::Unbounded, Bound::Included(_) | Bound::Excluded(_)) => Ordering::Greater,
        (Bound::Included(_) | Bound::Excluded(_), Bound::Unbounded) => Ordering::Less,
        (Bound::Included(left), Bound::Included(right))
        | (Bound::Excluded(left), Bound::Excluded(right)) => left.cmp(right),
        (Bound::Included(left), Bound::Excluded(right)) => left.cmp(right).then(Ordering::Greater),
        (Bound::Excluded(left), Bound::Included(right)) => left.cmp(right).then(Ordering::Less),
    }
}
