//! This module implements marker tree operations using Algebraic Decision Diagrams (ADD).
//!
//! An ADD is a tree of decision nodes as well as two terminal nodes, `true` and `false`. Marker
//! variables are represented as decision nodes. The edge from a decision node to it's child
//! represents a particular assignment of a value to that variable. Depending on the type of
//! variable, an edge can be represented by binary values or a disjoint set of ranges, as opposed
//! to a traditional Binary Decision Diagram.
//!
//! For example, the marker `python_version > '3.7' and os_name == 'Linux'` creates the following
//! marker tree:
//!
//! ```text
//! python_version:
//!   (> '3.7')  -> os_name:
//!                   (> 'Linux')  -> FALSE
//!                   (== 'Linux') -> TRUE
//!                   (< 'Linux')  -> FALSE
//!   (<= '3.7') -> FALSE
//! ```
//!
//! Specifically, a marker tree is represented as a Reduced Ordered ADD. An ADD is ordered if
//! different variables appear in the same order on all paths from the root. Additionally, an ADD
//! is reduced if:
//! - Isomoprhic nodes are merged.
//! - Nodes with isomorphic children are eliminated.
//!
//! These two rules provide an important guarantee for marker trees: marker trees are canonical for
//! a given marker function and variable ordering. Because variable ordering is defined at
//! compile-time, this means any equivalent marker trees are normalized upon construction.
//! Importantly, this means that we can identify trivally true marker trees, as well as
//! unsatisfiable marker trees. This provides important information for the resolver when forking.
//!
//! ADDs provide polynomial time operations such as conjunction and negation, which is important as
//! marker trees are combined during universal resolution. Because ADDs solve the SAT problem, constructing
//! an arbitrary ADD can take exponential time in the worst case. In practice, marker trees have a limited
//! number of variables and user-provided marker trees are typically very simple.
//!
//! Additionally, the implementation in this module uses complemented edges, meaning a marker tree
//! and it's complement are represented by the same node internally. This allows cheap constant-time
//! marker tree negation.
use std::cmp::Ordering;
use std::fmt;
use std::ops::Bound;
use std::sync::Mutex;
use std::sync::MutexGuard;

use itertools::Either;
use pep440_rs::{Version, VersionSpecifier};
use pubgrub::Range;
use rustc_hash::FxHashMap;
use std::sync::LazyLock;
use uv_normalize::ExtraName;

use crate::ExtraOperator;
use crate::{MarkerExpression, MarkerOperator, MarkerValueString, MarkerValueVersion};

use super::range::PubGrubSpecifier;

/// The global node interner.
pub(crate) static INTERNER: LazyLock<Interner> = LazyLock::new(Interner::default);

/// An interner for decision nodes.
///
/// Interning decision nodes allows isomoprhic nodes to be automatically merged.
/// It also allows nodes to cheaply compared.
#[derive(Default)]
pub(crate) struct Interner {
    pub shared: InternerShared,
    state: Mutex<InternerState>,
}

/// The shared part of an [`Interner`], which can be accessed without a lock.
#[derive(Default)]
pub(crate) struct InternerShared {
    /// A list of unique [`Node`]s.
    nodes: boxcar::Vec<Node>,
}

/// The mutable [`Interner`] state, stored behind a lock.
#[derive(Default)]
struct InternerState {
    /// A map from a [`Node`] to a unique [`NodeId`], representing an index
    /// into [`InternerShared`].
    unique: FxHashMap<Node, NodeId>,

    /// A cache for operations between two nodes.
    cache: FxHashMap<(NodeId, NodeId), NodeId>,
}

impl InternerShared {
    /// Returns the node for the given [`NodeId`].
    pub(crate) fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id.index()]
    }
}

impl Interner {
    /// Locks the interner state, returning a guard that can be used to perform marker
    /// operations.
    pub(crate) fn lock(&self) -> InternerGuard<'_> {
        InternerGuard {
            state: self.state.lock().unwrap(),
            shared: &self.shared,
        }
    }
}

/// A lock of [`InternerState`].
pub(crate) struct InternerGuard<'a> {
    state: MutexGuard<'a, InternerState>,
    shared: &'a InternerShared,
}

impl InternerGuard<'_> {
    /// Creates a decision node with the given variable and children.
    fn create_node(&mut self, var: Variable, children: Edges) -> NodeId {
        let mut node = Node { var, children };
        let mut first = node.children.nodes().next().unwrap();

        // With a complemented edge representation, there are two ways to represent the same node:
        // complementing the root and all children edges results in the same node. To ensure markers
        // are canonical, the first child edge is never complemented.
        let mut flipped = false;
        if first.is_complement() {
            node = node.not();
            first = first.not();
            flipped = true;
        }

        // Reduction: If all children refer to the same node, we eliminate the parent node
        // and just return the child.
        if node.children.nodes().all(|node| node == first) {
            if flipped {
                return first.not();
            } else {
                return first;
            }
        }

        // Insert the node.
        let id = self
            .state
            .unique
            .entry(node.clone())
            .or_insert_with(|| NodeId::new(self.shared.nodes.push(node), false));

        if flipped {
            id.not()
        } else {
            *id
        }
    }

    /// Returns a decision node representing a singular marker expression.
    pub(crate) fn terminal(&mut self, expr: MarkerExpression) -> NodeId {
        let (var, children) = match expr {
            // A variable representing the output of a version key. Edges correspond
            // to disjoint version ranges.
            MarkerExpression::Version { key, specifier } => {
                (Variable::Version(key), Edges::from_specifier(specifier))
            }
            // The `in` and `contains` operators are a bit different than other operators.
            // In particular, they do not represent a particular value for the corresponding
            // variable, and can overlap. For example, `'nux' in os_name` and `os_name == 'Linux'`
            // can both be `true` in the same marker environment, and so cannot be represented by
            // the same variable. Because of this, we represent `in` and `contains`, as well as
            // their negations, as distinct variables, unrelated to the range of a given key.
            MarkerExpression::String {
                key,
                operator: MarkerOperator::In,
                value,
            } => (Variable::In { key, value }, Edges::from_bool(true)),
            MarkerExpression::String {
                key,
                operator: MarkerOperator::NotIn,
                value,
            } => (Variable::In { key, value }, Edges::from_bool(false)),
            MarkerExpression::String {
                key,
                operator: MarkerOperator::Contains,
                value,
            } => (Variable::Contains { key, value }, Edges::from_bool(true)),
            MarkerExpression::String {
                key,
                operator: MarkerOperator::NotContains,
                value,
            } => (Variable::Contains { key, value }, Edges::from_bool(false)),
            // A variable representing the output of a string key. Edges correspond
            // to disjoint string ranges.
            MarkerExpression::String {
                key,
                operator,
                value,
            } => (Variable::String(key), Edges::from_string(operator, value)),
            // A variable representing the existence or absence of a particular extra.
            MarkerExpression::Extra {
                name,
                operator: ExtraOperator::Equal,
            } => (Variable::Extra(name), Edges::from_bool(true)),
            MarkerExpression::Extra {
                name,
                operator: ExtraOperator::NotEqual,
            } => (Variable::Extra(name), Edges::from_bool(false)),
        };

        self.create_node(var, children)
    }

    // Returns a decision node representing the disjunction of two nodes.
    pub(crate) fn or(&mut self, x: NodeId, y: NodeId) -> NodeId {
        // We take advantage of cheap negation here and implement OR in terms
        // of it's DeMorgan complement.
        self.and(x.not(), y.not()).not()
    }

    // Returns a decision node representing the conjunction of two nodes.
    pub(crate) fn and(&mut self, xi: NodeId, yi: NodeId) -> NodeId {
        if xi == NodeId::TRUE {
            return yi;
        }
        if yi == NodeId::TRUE {
            return xi;
        }
        if xi == yi {
            return xi;
        }
        if xi == NodeId::FALSE || yi == NodeId::FALSE {
            return NodeId::FALSE;
        }

        // X and Y are not equal but refer to the same node.
        // Thus one is complement but not the other (X and not X).
        if xi.index() == yi.index() {
            return NodeId::FALSE;
        }

        // The operation was memoized.
        if let Some(result) = self.state.cache.get(&(xi, yi)) {
            return *result;
        }

        let (x, y) = (self.shared.node(xi), self.shared.node(yi));

        // Perform Shannon Expansion of the higher order variable.
        //
        // X is higher order than Y, apply Y to every child of X.
        let (func, children) = if x.var < y.var {
            let children = x.children.map(xi, |node| self.and(node, yi));
            (x.var.clone(), children)
        // Y is higher order than X, apply X to every child of Y.
        } else if y.var < x.var {
            let children = y.children.map(yi, |node| self.and(node, xi));
            (y.var.clone(), children)
        // X and Y represent the same variable, combine the children.
        } else {
            let children = x.children.apply(xi, &y.children, yi, |x, y| self.and(x, y));
            (x.var.clone(), children)
        };

        // Create the output node.
        let node = self.create_node(func, children);

        // Memoize the result of this operation.
        //
        // ADDs often contain duplicated subgraphs in distinct branches due to the restricted
        // variable ordering. Memoizing allows ADD operations to remain polynomial time.
        self.state.cache.insert((xi, yi), node);

        node
    }

    // Restrict the output of a given boolean variable in the tree.
    //
    // This allows a tree to be simplified if a variable is known to be `true`.
    pub(crate) fn restrict(&mut self, i: NodeId, f: &impl Fn(&Variable) -> Option<bool>) -> NodeId {
        if matches!(i, NodeId::TRUE | NodeId::FALSE) {
            return i;
        }

        let node = self.shared.node(i);
        if let Edges::Boolean { high, low } = node.children {
            if let Some(value) = f(&node.var) {
                // Restrict this variable to the given output by merging it
                // with the relevant child.
                let node = if value { high } else { low };
                return node.negate(i);
            }
        }

        // Restrict all nodes recursively.
        let children = node.children.map(i, |node| self.restrict(node, f));
        self.create_node(node.var.clone(), children)
    }

    // Restrict the output of a given version variable in the tree.
    //
    // This allows the tree to be simplified if a variable is known to be restricted to a
    // particular range of outputs.
    pub(crate) fn restrict_versions(
        &mut self,
        i: NodeId,
        f: &impl Fn(&Variable) -> Option<Range<Version>>,
    ) -> NodeId {
        if matches!(i, NodeId::TRUE | NodeId::FALSE) {
            return i;
        }

        let node = self.shared.node(i);
        if let Edges::Version { edges: ref map } = node.children {
            if let Some(allowed) = f(&node.var) {
                // Restrict the output of this variable to the given range.
                let mut simplified = SmallVec::new();
                for (range, node) in map {
                    let restricted = range.intersection(&allowed);
                    if restricted.is_empty() {
                        continue;
                    }

                    simplified.push((restricted.clone(), *node));
                }

                return self
                    .create_node(node.var.clone(), Edges::Version { edges: simplified })
                    .negate(i);
            }
        }

        // Restrict all nodes recursively.
        let children = node.children.map(i, |node| self.restrict_versions(node, f));
        self.create_node(node.var.clone(), children)
    }
}

/// A unique variable for a decision node.
///
/// This `enum` also defines the variable ordering for all ADDs.
/// Variable ordering is an interesting property of ADDs. A bad ordering
/// can lead to exponential explosion of the size of an ADD. However,
/// dynamically computing an optimal ordering is NP-complete.
///
/// We may wish to investigate the effect of this ordering on common marker
/// trees. However, marker trees are typically small, so this may not be high
/// impact.
#[derive(PartialOrd, Ord, PartialEq, Eq, Hash, Clone, Debug)]
pub(crate) enum Variable {
    /// A version marker, such as `python_version`.
    Version(MarkerValueVersion),
    /// A string marker, such as `os_name`.
    String(MarkerValueString),
    /// A variable representing a `<key> in <value>` expression for a particular
    /// string marker and value.
    In {
        key: MarkerValueString,
        value: String,
    },
    /// A variable representing a `<value> in <key>` expression for a particular
    /// string marker and value.
    Contains {
        key: MarkerValueString,
        value: String,
    },
    /// A variable representing the existence or absence of a given extra.
    ///
    /// We keep extras at the leaves of the tree, so when simplifying extras we can
    /// trivally remove the leaves without having to reconstruct the entire tree.
    Extra(ExtraName),
}

/// A decision node in an Algebraic Decision Diagram.
#[derive(PartialEq, Eq, Hash, Clone, Debug)]
pub(crate) struct Node {
    /// The variable this node represents.
    pub(crate) var: Variable,
    /// The children of this node, with edges representing the possible outputs
    /// of this variable.
    pub(crate) children: Edges,
}

impl Node {
    /// Return the complement of this node, flipping all children IDs.
    fn not(self) -> Node {
        Node {
            var: self.var,
            children: self.children.not(),
        }
    }
}

/// An ID representing a unique decision node.
///
/// The lowest bit of the ID is used represent complemented edges.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct NodeId(usize);

impl NodeId {
    // The terminal node representing `true`, or a trivially `true` node.
    pub(crate) const TRUE: NodeId = NodeId(0);

    // The terminal node representing `false`, or an unsatisifable node.
    pub(crate) const FALSE: NodeId = NodeId(1);

    /// Create a new, optionally complemented, [`NodeId`] with the given index.
    fn new(index: usize, complement: bool) -> NodeId {
        NodeId(((index + 1) << 1) | (complement as usize))
    }

    /// Returns the index of this ID, ignoring the complemented edge.
    fn index(self) -> usize {
        (self.0 >> 1) - 1
    }

    /// Returns `true` if this ID represents a complemented edge.
    fn is_complement(self) -> bool {
        (self.0 & 1) == 1
    }

    /// Returns `true` if this node represents an unsatisfiable node.
    pub(crate) fn is_false(self) -> bool {
        self == NodeId::FALSE
    }

    /// Returns `true` if this node represents a trivially `true` node.
    pub(crate) fn is_true(self) -> bool {
        self == NodeId::TRUE
    }

    /// Returns the complement of this node.
    pub(crate) fn not(self) -> NodeId {
        NodeId(self.0 ^ 1)
    }

    /// Returns the complement of this node, if it's parent is complemented.
    ///
    /// This method is useful to restore the complemented state of children nodes
    /// when traversing the tree.
    pub(crate) fn negate(self, parent: NodeId) -> NodeId {
        if parent.is_complement() {
            self.not()
        } else {
            self
        }
    }
}

/// A [`SmallVec`] with enough elements to hold two constant edges, as well as the
/// ranges in-between.
type SmallVec<T> = smallvec::SmallVec<[T; 5]>;

/// The edges of a decision node.
#[derive(PartialEq, Eq, Hash, Clone, Debug)]
pub(crate) enum Edges {
    // The edges of a version variable, representing a disjoint set of ranges that cover
    // the output space.
    Version {
        edges: SmallVec<(Range<Version>, NodeId)>,
    },
    // The edges of a string variable, representing a disjoint set of ranges that cover
    // the output space.
    String {
        edges: SmallVec<(Range<String>, NodeId)>,
    },
    // The edges of a boolean variable, representing `true` or `false` values.
    Boolean {
        high: NodeId,
        low: NodeId,
    },
}

impl Edges {
    /// Returns the [`Edges`] for a boolean variable.
    fn from_bool(complemented: bool) -> Edges {
        if complemented {
            Edges::Boolean {
                high: NodeId::TRUE,
                low: NodeId::FALSE,
            }
        } else {
            Edges::Boolean {
                high: NodeId::FALSE,
                low: NodeId::TRUE,
            }
        }
    }

    /// Returns the [`Edges`] for a string expression.
    fn from_string(operator: MarkerOperator, value: String) -> Edges {
        let range: Range<String> = match operator {
            MarkerOperator::Equal => Range::singleton(value),
            MarkerOperator::NotEqual => Range::singleton(value).complement(),
            MarkerOperator::GreaterThan => Range::strictly_higher_than(value),
            MarkerOperator::GreaterEqual => Range::higher_than(value),
            MarkerOperator::LessThan => Range::strictly_lower_than(value),
            MarkerOperator::LessEqual => Range::lower_than(value),
            MarkerOperator::TildeEqual => unreachable!("string comparisons with ~= are ignored"),
            _ => unreachable!("`in` and `contains` are treated as boolean variables"),
        };

        Edges::String {
            edges: Edges::from_range(range),
        }
    }

    /// Returns the [`Edges`] for a version specifier.
    fn from_specifier(specifier: VersionSpecifier) -> Edges {
        // The decision diagram relies on the assumption that the negation of a marker tree
        // is the complement of the marker space. However, pre-release versions violate
        // this assumption. For example, the marker `python_version > '3.9' or python_version <= '3.9'`
        // does not match `python_version == 3.9.0a0`. However, it's negation,
        // `python_version > '3.9' and python_version <= '3.9'` also does not include `3.9.0a0`, and is
        // actually `false`.
        //
        // For this reason we ignore pre-release versions completely when evaluating markers.
        let specifier = PubGrubSpecifier::from_release_specifier(&specifier).unwrap();

        Edges::Version {
            edges: Edges::from_range(specifier.into()),
        }
    }

    /// Returns an [`Edges`] where values in the given range are `true`.
    fn from_range<T>(range: Range<T>) -> SmallVec<(Range<T>, NodeId)>
    where
        T: Ord + Clone,
    {
        let complement = range.complement();

        let mut versions = SmallVec::new();

        // Add the `true` ranges.
        for (start, end) in range.iter() {
            let range = Range::from_range_bounds((start.clone(), end.clone()));
            versions.push((range, NodeId::TRUE));
        }

        // Add the `false` ranges.
        for (start, end) in complement.iter() {
            let range = Range::from_range_bounds((start.clone(), end.clone()));
            versions.push((range, NodeId::FALSE));
        }

        // Sort the ranges.
        //
        // The ranges are disjoint so we don't care about equality.
        versions.sort_by(|(range1, _), (range2, _)| compare_disjoint_range_start(range1, range2));
        versions
    }

    /// Merge two [`Edges`], applying the given function to all disjoint, intersecting edges.
    fn apply(
        &self,
        parent: NodeId,
        map2: &Edges,
        parent2: NodeId,
        mut apply: impl FnMut(NodeId, NodeId) -> NodeId,
    ) -> Edges {
        match (self, map2) {
            // Version or string variables, merge the ranges.
            (Edges::Version { edges: map }, Edges::Version { edges: map2 }) => Edges::Version {
                edges: Edges::apply_ranges(map, parent, map2, parent2, apply),
            },
            (Edges::String { edges: map }, Edges::String { edges: map2 }) => Edges::String {
                edges: Edges::apply_ranges(map, parent, map2, parent2, apply),
            },
            // Boolean variables, simply merge the low and high nodes.
            (
                Edges::Boolean { high, low },
                Edges::Boolean {
                    high: high2,
                    low: low2,
                },
            ) => Edges::Boolean {
                high: apply(high.negate(parent), high2.negate(parent)),
                low: apply(low.negate(parent), low2.negate(parent)),
            },
            _ => unreachable!(),
        }
    }

    /// Merge two range maps, applying the given function to all disjoint, intersecting ranges.
    fn apply_ranges<T>(
        map: &SmallVec<(Range<T>, NodeId)>,
        parent: NodeId,
        map2: &SmallVec<(Range<T>, NodeId)>,
        parent2: NodeId,
        mut apply: impl FnMut(NodeId, NodeId) -> NodeId,
    ) -> SmallVec<(Range<T>, NodeId)>
    where
        T: Clone + Ord,
    {
        let mut combined = SmallVec::new();
        for (range, node) in map.iter() {
            // Find any intersections.
            //
            // Note that restrict ranges (see `restrict_versions`) makes finding overlapping ranges
            // is a bit more complicated despite the ranges being sorted. We cannot simply zip both
            // sets, as they may contain arbitrary gaps. Instead, we use a quadratic search for
            // simplicity as the set of ranges for a given variable is typically very small.
            for (range2, node2) in map2.iter() {
                let intersection = range2.intersection(&range);
                if intersection.is_empty() {
                    // TODO(ibraheem): take advantage of the sorted ranges to `break` early
                    continue;
                }

                // Merge the intersection.
                let node = apply(node.negate(parent), node2.negate(parent2));
                match combined.last_mut() {
                    // Combine ranges if possible.
                    Some((range, prev)) if *prev == node && can_conjoin(range, &intersection) => {
                        *range = range.union(&intersection);
                    }
                    _ => combined.push((intersection.clone(), node)),
                }
            }
        }

        combined
    }

    // Apply the given function to all direct children of this node.
    fn map(&self, parent: NodeId, mut f: impl FnMut(NodeId) -> NodeId) -> Edges {
        match self {
            Edges::Version { edges: map } => Edges::Version {
                edges: map
                    .iter()
                    .cloned()
                    .map(|(range, node)| (range, f(node.negate(parent))))
                    .collect(),
            },
            Edges::String { edges: map } => Edges::String {
                edges: map
                    .iter()
                    .cloned()
                    .map(|(range, node)| (range, f(node.negate(parent))))
                    .collect(),
            },
            Edges::Boolean { high, low } => Edges::Boolean {
                low: f(low.negate(parent)),
                high: f(high.negate(parent)),
            },
        }
    }

    // Returns an iterator over all direct children of this node.
    fn nodes(&self) -> impl Iterator<Item = NodeId> + '_ {
        match self {
            Edges::Version { edges: map } => {
                Either::Left(Either::Left(map.iter().map(|(_, node)| *node)))
            }
            Edges::String { edges: map } => {
                Either::Left(Either::Right(map.iter().map(|(_, node)| *node)))
            }
            Edges::Boolean { high, low } => Either::Right([*high, *low].into_iter()),
        }
    }

    // Returns the complement of this [`Edges`].
    fn not(self) -> Edges {
        match self {
            Edges::Version { edges: map } => Edges::Version {
                edges: map
                    .into_iter()
                    .map(|(range, node)| (range, node.not()))
                    .collect(),
            },
            Edges::String { edges: map } => Edges::String {
                edges: map
                    .into_iter()
                    .map(|(range, node)| (range, node.not()))
                    .collect(),
            },
            Edges::Boolean { high, low } => Edges::Boolean {
                high: high.not(),
                low: low.not(),
            },
        }
    }
}

/// Compares the start of two ranges that are known to be disjoint.
fn compare_disjoint_range_start<T>(range1: &Range<T>, range2: &Range<T>) -> Ordering
where
    T: Ord,
{
    let (upper1, _) = range1.bounding_range().unwrap();
    let (upper2, _) = range2.bounding_range().unwrap();

    match (upper1, upper2) {
        (Bound::Unbounded, _) => Ordering::Less,
        (_, Bound::Unbounded) => Ordering::Greater,
        (Bound::Included(v1), Bound::Excluded(v2)) if v1 == v2 => Ordering::Less,
        (Bound::Excluded(v1), Bound::Included(v2)) if v1 == v2 => Ordering::Greater,
        // Note that the ranges are disjoint, so their lower bounds cannot be equal.
        (Bound::Included(v1) | Bound::Excluded(v1), Bound::Included(v2) | Bound::Excluded(v2)) => {
            v1.cmp(v2)
        }
    }
}

/// Returns `true` if two disjoint ranges can be conjoined seamlessly without introducing a gap.
fn can_conjoin<T>(range1: &Range<T>, range2: &Range<T>) -> bool
where
    T: Ord + Clone,
{
    let Some((_, end)) = range1.bounding_range() else {
        return false;
    };
    let Some((start, _)) = range2.bounding_range() else {
        return false;
    };

    match (end, start) {
        (Bound::Included(v1), Bound::Excluded(v2)) if v1 == v2 => true,
        (Bound::Excluded(v1), Bound::Included(v2)) if v1 == v2 => true,
        _ => false,
    }
}

impl fmt::Debug for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if *self == NodeId::FALSE {
            return write!(f, "false");
        }

        if *self == NodeId::TRUE {
            return write!(f, "true");
        }

        if self.is_complement() {
            write!(f, "{:?}", INTERNER.shared.node(*self).clone().not())
        } else {
            write!(f, "{:?}", INTERNER.shared.node(*self))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::INTERNER;
    use crate::{marker::algebra::NodeId, MarkerExpression};

    fn expr(s: &str) -> NodeId {
        INTERNER
            .lock()
            .terminal(MarkerExpression::from_str(s).unwrap().unwrap())
    }

    #[test]
    fn basic() {
        let m = || INTERNER.lock();
        let a = expr("extra == 'foo'");
        assert!(!a.is_false());

        let b = expr("os_name == 'foo'");
        let c = m().or(a, b);
        assert!(!c.is_false());
        assert!(!m().and(a, b).is_false());

        let d = m().and(a, a.not());
        assert!(d.is_false());

        let e = m().or(d, b);
        assert!(!e.is_false());

        let f = m().or(c, c.not());
        assert!(!f.is_false());
        assert!(f.is_true());

        let a = expr("extra == 'foo'");
        assert!(!a.is_false());

        let b = expr("extra != 'foo'");
        assert!(m().and(a, b).is_false());
        assert!(m().or(a, b).is_true());

        let a = expr("os_name >= 'bar'");
        assert!(!a.is_false());

        let b = expr("os_name < 'bar'");
        assert!(m().and(a, b).is_false());
        assert!(m().or(a, b).is_true());

        let c = expr("os_name <= 'bar'");
        assert!(!m().and(a, c).is_false());
        assert!(m().or(a, c).is_true());
    }

    #[test]
    fn version() {
        let m = || INTERNER.lock();
        let a = expr("python_version == '3'");
        let b = expr("python_version != '3'");
        let c = expr("python_version >= '3'");
        let d = expr("python_version <= '3'");

        let e = expr("python_version == '2'");
        let z = expr("python_version == '1'");
        assert!(m().and(e, z).is_false());

        assert_eq!(a.not(), b);
        assert_eq!(a, b.not());

        assert!(m().and(a, b).is_false());
        assert!(m().or(a, b).is_true());

        assert_eq!(m().and(a, c), a);
        assert_eq!(m().and(a, d), a);

        assert_eq!(m().and(c, d), a);

        assert!(!m().and(c, d).is_false());
        assert!(m().or(c, d).is_true());
    }

    #[test]
    fn simplify() {
        let m = || INTERNER.lock();
        let x = expr("platform_machine == 'x86_64'");
        let n = expr("platform_machine != 'x86_64'");
        let w = expr("platform_machine == 'Windows'");

        let a = m().and(x, w);
        let b = m().and(n, w);
        assert_eq!(m().or(a, b), w);
    }
}
