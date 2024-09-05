use pep508_rs::MarkerTree;
use petgraph::algo::greedy_feedback_arc_set;
use petgraph::graph::NodeIndex;
use petgraph::visit::{EdgeRef, Topo};
use petgraph::{Directed, Direction, Graph};
use rustc_hash::FxHashMap;
use std::collections::hash_map::Entry;

/// A trait for a graph node that can be annotated with a [`MarkerTree`].
pub(crate) trait Markers {
    fn set_markers(&mut self, markers: MarkerTree);
}

/// Propagate the [`MarkerTree`] qualifiers across the graph.
///
/// The graph is directed, so if any edge contains a marker, we need to propagate it to all
/// downstream nodes.
pub(crate) fn propagate_markers<T: Markers>(
    mut graph: Graph<T, MarkerTree, Directed>,
) -> Graph<T, MarkerTree, Directed> {
    // Remove any cycles. By absorption, it should be fine to ignore cycles.
    //
    // Imagine a graph: `A -> B -> C -> A`. Assume that `A` has weight `1`, `B` has weight `2`,
    // and `C` has weight `3`. The weights are the marker trees.
    //
    // When propagating, we'd return to `A` when we hit the cycle, to create `1 or (1 and 2 and 3)`,
    // which resolves to `1`.
    //
    // TODO(charlie): The above reasoning could be incorrect. Consider using a graph algorithm that
    // can handle weight propagation with cycles.
    let edges = {
        let mut fas = greedy_feedback_arc_set(&graph)
            .map(|edge| edge.id())
            .collect::<Vec<_>>();
        fas.sort_unstable();
        let mut edges = Vec::with_capacity(fas.len());
        for edge_id in fas.into_iter().rev() {
            edges.push(graph.edge_endpoints(edge_id).unwrap());
            graph.remove_edge(edge_id);
        }
        edges
    };

    let mut topo = Topo::new(&graph);
    while let Some(index) = topo.next(&graph) {
        let marker_tree = {
            // Fold over the edges to combine the marker trees. If any edge is `None`, then
            // the combined marker tree is `None`.
            let mut edges = graph.edges_directed(index, Direction::Incoming);

            edges
                .next()
                .and_then(|edge| graph.edge_weight(edge.id()).cloned())
                .and_then(|initial| {
                    edges.try_fold(initial, |mut acc, edge| {
                        acc.or(graph.edge_weight(edge.id())?.clone());
                        Some(acc)
                    })
                })
                .unwrap_or_default()
        };

        // Propagate the marker tree to all downstream nodes.
        let mut walker = graph
            .neighbors_directed(index, Direction::Outgoing)
            .detach();
        while let Some((outgoing, _)) = walker.next(&graph) {
            if let Some(weight) = graph.edge_weight_mut(outgoing) {
                weight.and(marker_tree.clone());
            }
        }

        let node = &mut graph[index];
        node.set_markers(marker_tree);
    }

    // Re-add the removed edges. We no longer care about the edge _weights_, but we do want the
    // edges to be present, to power the `# via` annotations.
    for (source, target) in edges {
        graph.add_edge(source, target, MarkerTree::TRUE);
    }

    graph
}

/// Determine the markers under which a package is reachable in the dependency tree.
///
/// The algorithm is a variant of Dijkstra's algorithm for not totally ordered distances:
/// Whenever we find a shorter distance to a node (a marker that is not a subset of the existing
/// marker), we re-queue the node and update all its children. This implicitly handles cycles,
/// whenever we re-reach a node through a cycle the marker we have is a more
/// specific marker/longer path, so we don't update the node and don't re-queue it.
pub(crate) fn marker_reachability<T>(
    graph: &Graph<T, MarkerTree>,
    fork_markers: &[MarkerTree],
) -> FxHashMap<NodeIndex, MarkerTree> {
    // Note that we build including the virtual packages due to how we propagate markers through
    // the graph, even though we then only read the markers for base packages.
    let mut reachability = FxHashMap::default();

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
    let root_markers: MarkerTree = if fork_markers.is_empty() {
        MarkerTree::TRUE
    } else {
        fork_markers
            .iter()
            .fold(MarkerTree::FALSE, |mut acc, marker| {
                acc.or(marker.clone());
                acc
            })
    };
    for root_index in &queue {
        reachability.insert(*root_index, root_markers.clone());
    }

    // Propagate all markers through the graph, so that the eventual marker for each node is the
    // union of the markers of each path we can reach the node by.
    while let Some(parent_index) = queue.pop() {
        let marker = reachability[&parent_index].clone();
        for child_edge in graph.edges_directed(parent_index, Direction::Outgoing) {
            // The marker for all paths to the child through the parent.
            let mut child_marker = child_edge.weight().clone();
            child_marker.and(marker.clone());
            match reachability.entry(child_edge.target()) {
                Entry::Occupied(mut existing) => {
                    // If the marker is a subset of the existing marker (A ⊆ B exactly if
                    // A ∪ B = A), updating the child wouldn't change child's marker.
                    child_marker.or(existing.get().clone());
                    if &child_marker != existing.get() {
                        existing.insert(child_marker);
                        queue.push(child_edge.target());
                    }
                }
                Entry::Vacant(vacant) => {
                    vacant.insert(child_marker.clone());
                    queue.push(child_edge.target());
                }
            }
        }
    }

    reachability
}
