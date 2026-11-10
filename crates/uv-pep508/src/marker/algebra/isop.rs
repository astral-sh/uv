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

use super::{EdgeBuilder, Edges, InternerGuard, NodeId, SmallVec, Variable};

/// A shared cover whose paths are expanded only when producing the final DNF.
#[derive(Clone)]
pub(crate) enum Cover {
    False,
    True,
    Guard(NodeId),
    Atom(Box<crate::MarkerExpression>),
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

        // A printed comparison tests the original spelling on both domains.
        // Within its branch, opaque values outside that spelling range are
        // unreachable. Removing them recovers the simple comparison.
        let node = self.shared.node(lower);
        if lower == upper
            && let Variable::String(key) = node.var
            && key.is_version_or_string()
            && let Edges::String { edges } = &node.children
        {
            if let Some(atom) = self.raw_order_atom(key, lower, edges) {
                return (lower, Cover::Atom(Box::new(atom)));
            }
            let mut ranges = IndexMap::<_, Ranges<_>, FxBuildHasher>::default();
            for (range, child) in edges {
                let child = child.negate(lower);
                if !child.is_false() {
                    ranges
                        .entry(child)
                        .and_modify(|r| *r = r.union(range))
                        .or_insert_with(|| range.clone());
                }
            }
            let mut branches = Vec::new();
            for (child, range) in ranges {
                let (possible, _) = crate::marker::version_or_string::spelling_range(&range);
                let assumption = self.create_node(
                    Variable::VersionString(key),
                    Edges::from_version_or_string_range(&possible),
                );
                let suffix = self.restrict(child, assumption);
                if !suffix.is_false() {
                    let (_, cover) = self.isop_cached(suffix, suffix, cache);
                    branches.push((Guard::Edge { node: lower, child }, cover));
                }
            }
            let result = (lower, Cover::Branches(branches.into()));
            cache.insert((lower, upper), result.clone());
            return result;
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

    /// A single spelling cut can encode one of four ordered atoms. Match the
    /// interned decision against those atoms before expanding its branches.
    fn raw_order_atom(
        &mut self,
        key: crate::marker::lowering::CanonicalMarkerValueString,
        parent: NodeId,
        edges: &[(Ranges<arcstr::ArcStr>, NodeId)],
    ) -> Option<crate::MarkerExpression> {
        use crate::MarkerOperator::{GreaterEqual, GreaterThan, LessEqual, LessThan};
        let [(before, _), (_, _)] = edges else {
            return None;
        };
        let (Bound::Unbounded, Bound::Excluded(literal)) = before.bounding_range()? else {
            return None;
        };
        let operator = [LessThan, LessEqual, GreaterThan, GreaterEqual]
            .into_iter()
            .find(|&op| self.raw_order(key, op, literal.clone()) == parent)?;
        Some(crate::MarkerExpression::String {
            key: key.into(),
            operator,
            value: literal.clone(),
        })
    }

    /// Materialize a borrowed guard only when algebra needs its function.
    fn guard_node(&mut self, guard: Guard) -> NodeId {
        match guard {
            Guard::Node(node) => node,
            Guard::Edge { node, child } => {
                let decision = self.shared.node(node);
                let edges = decision.children.map(node, |target| {
                    if target == child {
                        NodeId::TRUE
                    } else {
                        NodeId::FALSE
                    }
                });
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
            Edges::Version { .. } => self.range_guards(
                variable,
                lower,
                upper,
                |edges| match edges {
                    Edges::Version { edges } => Some(edges),
                    _ => None,
                },
                |range| Edges::Version {
                    edges: Edges::from_range(range),
                },
            ),
            Edges::String { .. } => self.range_guards(
                variable,
                lower,
                upper,
                |edges| match edges {
                    Edges::String { edges } => Some(edges),
                    _ => None,
                },
                |range| Edges::String {
                    edges: Edges::from_range(range),
                },
            ),
            Edges::VersionOrString { .. } => self.range_guards(
                variable,
                lower,
                upper,
                |edges| match edges {
                    Edges::VersionOrString { edges } => Some(edges),
                    _ => None,
                },
                Edges::from_version_or_string_range,
            ),
        }
    }

    /// Partition two range decisions using the same endpoints and create their guards.
    fn range_guards<T: Clone + Ord>(
        &mut self,
        variable: &Variable,
        lower: NodeId,
        upper: NodeId,
        ranges: impl Fn(&Edges) -> Option<&[(Ranges<T>, NodeId)]>,
        from_range: impl Fn(&Ranges<T>) -> EdgeBuilder,
    ) -> SmallVec<(Guard, NodeId, NodeId)> {
        let lower_node = self.shared.node(lower);
        let upper_node = self.shared.node(upper);
        let lower_edges = (lower_node.var == *variable)
            .then(|| ranges(&lower_node.children))
            .flatten();
        let upper_edges = (upper_node.var == *variable)
            .then(|| ranges(&upper_node.children))
            .flatten();
        range_cofactors(lower, lower_edges, upper, upper_edges)
            .into_iter()
            .map(|(range, lower, upper)| {
                let guard = if lower.is_false() {
                    NodeId::FALSE
                } else {
                    self.create_node(variable.clone(), from_range(&range))
                };
                (Guard::Node(guard), lower, upper)
            })
            .collect()
    }
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
