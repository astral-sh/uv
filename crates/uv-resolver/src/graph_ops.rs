use pep508_rs::MarkerTree;
use petgraph::algo::greedy_feedback_arc_set;
use petgraph::visit::{EdgeRef, Topo};
use petgraph::{Directed, Direction, Graph};

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
