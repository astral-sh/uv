//! Extract a sum-of-products cover directly from a decision diagram.
//!
//! Boolean decisions use the Minato-Morreale ISOP recurrence. For range-valued decisions,
//! split the lower and upper bounds into matching ranges and extract the part that can omit
//! the decision from all branches. This preserves the function, but does not guarantee a
//! prime, irredundant cover over arbitrary unions of ranges.
//!
//! See <https://people.eecs.berkeley.edu/~alanmi/publications/2003/iwls03_sop.pdf>, section 4.
//! Cover subtraction follows the fused area/cover operation described in section 7.2 of
//! <https://people.eecs.berkeley.edu/~alanmi/publications/2001/tech01_zdd_.pdf>.

use std::cmp::Ordering;
use std::ops::Bound;
use std::rc::Rc;

use indexmap::IndexMap;
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

/// Extraction-local memo tables. Every branch cover remains owned by `covers`, so its
/// allocation address is a stable identity for subtraction until extraction finishes.
#[derive(Default)]
struct CoverCache {
    covers: FxHashMap<(NodeId, NodeId), Cover>,
    differences: FxHashMap<(NodeId, *const (Guard, Cover)), NodeId>,
}

impl InternerGuard<'_> {
    /// Extract a cover without materializing the decision diagram's paths.
    pub(crate) fn isop(&mut self, marker: NodeId) -> Cover {
        self.isop_cached(marker, marker, &mut CoverCache::default())
    }

    /// Return a cover of a function `I`, where `lower => I => upper`.
    /// The interval allows recursive calls to use already covered cases as don't-cares.
    fn isop_cached(&mut self, lower: NodeId, upper: NodeId, cache: &mut CoverCache) -> Cover {
        if lower.is_false() {
            return Cover::False;
        }
        if upper.is_true() {
            return Cover::True;
        }
        debug_assert!(
            !upper.is_false() && !lower.is_true(),
            "invalid ISOP interval: {lower:?}..{upper:?}"
        );
        if let Some(result) = cache.covers.get(&(lower, upper)) {
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
            return Cover::Guard(lower);
        }

        let exact = lower == upper;
        let cofactors = self.cover_cofactors(lower, upper);
        // With two cofactors, nonempty specific covers cannot represent the same function:
        // each must cover a point forbidden by the other cofactor's upper bound.
        let merge_ranges = cofactors.len() > 2;
        // A clause can omit this variable only where every branch permits it.
        let common_upper = cofactors
            .iter()
            .fold(NodeId::TRUE, |common, &(_, _, upper)| {
                self.and(common, upper)
            });
        let mut common_lower = NodeId::FALSE;
        let mut branches = Vec::new();
        let mut equivalent = FxHashMap::default();
        for (guard, lower, upper) in cofactors {
            let required = self.and(lower, common_upper.not());
            let cover = self.isop_cached(required, upper, cache);
            let remaining = self.subtract_cover(lower, &cover, cache);
            common_lower = self.or(common_lower, remaining);
            if let Cover::False = cover {
                continue;
            }
            // Equivalent suffixes can share a union of ranges, including inequalities.
            if merge_ranges {
                let function = self.subtract_cover(NodeId::TRUE, &cover, cache).not();
                if let Some(&index) = equivalent.get(&function) {
                    let (previous, _): &mut (Guard, Cover) = &mut branches[index];
                    let left = self.guard_node(*previous);
                    let right = self.guard_node(guard);
                    *previous = Guard::Node(self.or(left, right));
                    continue;
                }
                equivalent.insert(function, branches.len());
            }
            branches.push((guard, cover));
        }
        let common = self.isop_cached(common_lower, common_upper, cache);
        match common {
            Cover::False => {}
            Cover::True | Cover::Guard(_) | Cover::Branches(_) => {
                branches.push((Guard::Node(NodeId::TRUE), common));
            }
        }
        let branches: Rc<[_]> = branches.into();
        if exact {
            let identity = Rc::as_ptr(&branches).cast();
            cache.differences.insert((lower, identity), NodeId::FALSE);
            cache
                .differences
                .insert((NodeId::TRUE, identity), lower.not());
        }
        let result = Cover::Branches(branches);
        cache.covers.insert((lower, upper), result.clone());
        result
    }

    /// Subtract a cover directly, without constructing the diagram of its covered area.
    /// This fuses cover conversion and Boolean difference, as in EXTRA's ISOP procedure.
    fn subtract_cover(&mut self, marker: NodeId, cover: &Cover, cache: &mut CoverCache) -> NodeId {
        if marker.is_false() {
            return marker;
        }
        match cover {
            Cover::False => marker,
            Cover::True => NodeId::FALSE,
            Cover::Guard(guard) => self.and(marker, guard.not()),
            Cover::Branches(branches) => {
                let key = (marker, Rc::as_ptr(branches).cast());
                if let Some(&result) = cache.differences.get(&key) {
                    return result;
                }
                let mut result = self.subtract_branches(marker, cover, branches, cache);
                if !result.is_true()
                    && !result.is_false()
                    && self.shared.node(result).var.is_conflicting_variable()
                {
                    let exclusions = self.exclusions();
                    if self.disjointness(result, exclusions.not()) {
                        result = NodeId::FALSE;
                    }
                }
                cache.differences.insert(key, result);
                result
            }
        }
    }

    /// Match input and cover cofactors, then rebuild only the uncovered decision.
    /// The specific guards partition one variable; the final guard may omit it.
    fn subtract_branches(
        &mut self,
        marker: NodeId,
        cover: &Cover,
        branches: &[(Guard, Cover)],
        cache: &mut CoverCache,
    ) -> NodeId {
        let Some((first, suffix)) = branches.first() else {
            return marker;
        };
        let condition = match first {
            Guard::Node(node) | Guard::Edge { node, .. } => *node,
        };
        if condition.is_true() {
            return self.subtract_cover(marker, suffix, cache);
        }
        let decision = self.shared.node(condition);
        let (variable, children) = if marker.is_true() {
            (decision.var.clone(), None)
        } else {
            let node = self.shared.node(marker);
            match node.var.cmp(&decision.var) {
                Ordering::Less => {
                    // Earlier input decisions recurse through the memoized operation,
                    // so shared cofactors are visited once, not once per guard.
                    let children = node
                        .children
                        .map(marker, |child| self.subtract_cover(child, cover, cache));
                    return self.create_node(node.var.clone(), children);
                }
                Ordering::Equal => (node.var.clone(), Some(&node.children)),
                Ordering::Greater => (decision.var.clone(), None),
            }
        };
        // Remove the common suffix before the specific suffixes, and keep the
        // uncovered cofactors uninterned until all guards have been processed.
        let (specific, common) = match branches.split_last() {
            Some(((Guard::Node(node), common), specific)) if node.is_true() => {
                (specific, Some(common))
            }
            _ => (branches, None),
        };
        let mut remaining = |child| match common {
            Some(common) => self.subtract_cover(child, common, cache),
            None => child,
        };
        let mut children = if let Some(children) = children {
            children.map(marker, remaining)
        } else {
            let remaining = remaining(marker);
            decision.children.map(condition, |_| remaining)
        };
        for (guard, suffix) in specific {
            let (condition, accepted) = match guard {
                Guard::Node(node) => (*node, NodeId::TRUE),
                Guard::Edge { node, child } => (*node, *child),
            };
            let decision = self.shared.node(condition);
            children = children.apply(
                NodeId::TRUE,
                &decision.children,
                condition,
                |child, guard_child| {
                    if guard_child == accepted {
                        self.subtract_cover(child, suffix, cache)
                    } else {
                        child
                    }
                },
            );
        }
        self.create_node(variable, children)
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
                    Some(edges.as_slice())
                } else {
                    None
                };
                let upper_edges = if let Edges::Version { edges } = &upper_node.children
                    && upper_node.var == *variable
                {
                    Some(edges.as_slice())
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
                    Some(edges.as_slice())
                } else {
                    None
                };
                let upper_edges = if let Edges::String { edges } = &upper_node.children
                    && upper_node.var == *variable
                {
                    Some(edges.as_slice())
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

#[cfg(test)]
mod tests {
    use super::CoverCache;
    use crate::MarkerTree;
    use crate::marker::algebra::INTERNER;

    /// Subtraction must handle unrelated input and cover roots, complemented edges,
    /// overlapping suffixes, native range partitions, and known platform exclusions.
    #[test]
    fn cover_difference() {
        let mut markers = vec![MarkerTree::FALSE, MarkerTree::TRUE];
        markers.extend(
            [
                "extra == 'a'",
                "extra != 'a'",
                "extra == 'b'",
                "extra == 'a' or extra == 'b'",
                "extra == 'a' and extra == 'b'",
                "(extra == 'a' and extra == 'b') or (extra != 'a' and extra == 'c')",
                "(extra == 'a' and extra == 'b') or (extra == 'b' and extra == 'c') or (extra == 'a' and extra == 'c')",
                "python_full_version < '3.10'",
                "python_full_version == '3.10' or extra == 'a'",
                "(python_full_version < '3.10' and extra == 'a') or (python_full_version > '3.10' and extra == 'b')",
                "sys_platform == 'linux'",
                "sys_platform == 'win32' or extra == 'a'",
                "sys_platform != 'linux' and sys_platform != 'win32'",
                "os_name == 'nt' or extra == 'b'",
                "platform_release == '24' or extra == 'a'",
                "platform_release != '24' and extra != 'b'",
            ]
            .map(|marker| marker.parse::<MarkerTree>().expect("valid marker")),
        );
        let mut interner = INTERNER.lock();
        let mut cache = CoverCache::default();
        for covered in &markers {
            let cover = interner.isop_cached(covered.0, covered.0, &mut cache);
            for marker in &markers {
                let expected = interner.and(marker.0, covered.0.not());
                let actual = interner.subtract_cover(marker.0, &cover, &mut cache);
                // Equivalent range decisions can retain different adjacent partitions.
                assert!(interner.disjointness(actual, expected.not()));
                assert!(interner.disjointness(expected, actual.not()));
                assert_eq!(actual.is_false(), expected.is_false());
            }
        }
    }
}
