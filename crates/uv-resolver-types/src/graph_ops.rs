use std::collections::hash_map::Entry;

use petgraph::graph::NodeIndex;
use petgraph::visit::EdgeRef;
use petgraph::{Direction, Graph};
use rustc_hash::{FxBuildHasher, FxHashMap};

use uv_pep508::MarkerTree;

use crate::universal_marker::UniversalMarker;

/// Determine the markers under which a package is reachable in the dependency tree.
///
/// The algorithm is a variant of Dijkstra's algorithm for not totally ordered distances:
/// Whenever we find a shorter distance to a node (a marker that is not a subset of the existing
/// marker), we re-queue the node and update all its children. This implicitly handles cycles,
/// whenever we re-reach a node through a cycle the marker we have is a more
/// specific marker/longer path, so we don't update the node and don't re-queue it.
pub fn marker_reachability<Marker: Boolean + Copy + PartialEq, Node, Edge: Reachable<Marker>>(
    graph: &Graph<Node, Edge>,
    fork_markers: &[Edge],
) -> FxHashMap<NodeIndex, Marker> {
    // Note that we build including the virtual packages due to how we propagate markers through
    // the graph, even though we then only read the markers for base packages.
    let mut reachability = FxHashMap::with_capacity_and_hasher(graph.node_count(), FxBuildHasher);

    // Collect the root nodes.
    //
    // Besides the actual virtual root node, virtual dev dependencies packages are also root
    // nodes since the edges don't cover dev dependencies.
    let mut queue: Vec<_> = graph
        .node_indices()
        .filter(|node_index| {
            graph
                .edges_directed(*node_index, Direction::Incoming)
                .next()
                .is_none()
        })
        .collect();

    // The root nodes are always applicable, unless the user has restricted resolver
    // environments with `tool.uv.environments`.
    let root_markers = if fork_markers.is_empty() {
        Edge::true_marker()
    } else {
        fork_markers
            .iter()
            .fold(Edge::false_marker(), |mut acc, edge| {
                acc.or(edge.marker());
                acc
            })
    };
    for root_index in &queue {
        reachability.insert(*root_index, root_markers);
    }

    // Propagate all markers through the graph, so that the eventual marker for each node is the
    // union of the markers of each path we can reach the node by.
    while let Some(parent_index) = queue.pop() {
        let marker = reachability[&parent_index];
        for child_edge in graph.edges_directed(parent_index, Direction::Outgoing) {
            // The marker for all paths to the child through the parent.
            let mut child_marker = child_edge.weight().marker();
            child_marker.and(marker);
            match reachability.entry(child_edge.target()) {
                Entry::Occupied(mut existing) => {
                    // If the marker is a subset of the existing marker (A ⊆ B exactly if
                    // A ∪ B = A), updating the child wouldn't change child's marker.
                    child_marker.or(*existing.get());
                    if &child_marker != existing.get() {
                        existing.insert(child_marker);
                        queue.push(child_edge.target());
                    }
                }
                Entry::Vacant(vacant) => {
                    vacant.insert(child_marker);
                    queue.push(child_edge.target());
                }
            }
        }
    }

    reachability
}

pub trait Reachable<T> {
    /// The marker representing the "true" value.
    fn true_marker() -> T;

    /// The marker representing the "false" value.
    fn false_marker() -> T;

    /// The marker attached to the edge.
    fn marker(&self) -> T;
}

impl Reachable<Self> for MarkerTree {
    fn true_marker() -> Self {
        Self::TRUE
    }

    fn false_marker() -> Self {
        Self::FALSE
    }

    fn marker(&self) -> Self {
        *self
    }
}

impl Reachable<Self> for UniversalMarker {
    fn true_marker() -> Self {
        Self::TRUE
    }

    fn false_marker() -> Self {
        Self::FALSE
    }

    fn marker(&self) -> Self {
        *self
    }
}

/// A trait for types that can be used as markers in the dependency graph.
pub trait Boolean {
    /// Perform a logical AND operation with another marker.
    fn and(&mut self, other: Self);

    /// Perform a logical OR operation with another marker.
    fn or(&mut self, other: Self);
}

impl Boolean for UniversalMarker {
    fn and(&mut self, other: Self) {
        self.and(other);
    }

    fn or(&mut self, other: Self) {
        self.or(other);
    }
}

impl Boolean for MarkerTree {
    fn and(&mut self, other: Self) {
        *self = Self::and(*self, other);
    }

    fn or(&mut self, other: Self) {
        *self = Self::or(*self, other);
    }
}
