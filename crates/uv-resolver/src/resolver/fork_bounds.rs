use petgraph::Graph;
use rustc_hash::FxHashMap;
use uv_normalize::PackageName;
use uv_pep440::UpperBound;
use uv_pep508::MarkerTree;
use uv_resolver_types::graph_ops::marker_reachability;

use crate::preferences::ForkBound;

/// An edge of the completed solution that caps the version of a dependency.
pub(super) struct BoundedDependency<'a> {
    pub(super) parent: Option<&'a PackageName>,
    pub(super) name: &'a PackageName,
    pub(super) marker: MarkerTree,
    pub(super) upper: UpperBound,
}

/// Find where the selected versions were subject to an upper bound. If Intel macOS
/// caps `numba`, that release's `numpy<2.3` requirement can also be local to Intel.
pub(super) fn sibling_bounds(
    dependencies: Vec<BoundedDependency<'_>>,
) -> FxHashMap<&PackageName, Box<[ForkBound]>> {
    if dependencies
        .iter()
        .all(|dependency| dependency.marker.is_true())
    {
        return FxHashMap::default();
    }

    // Only upper-bounded requirements carry the reason for preferring an older version.
    // An unrestricted dependency on NumPy, for example, does not explain its selection.
    let mut graph = Graph::new();
    let root = graph.add_node(());
    let mut nodes = FxHashMap::default();
    for dependency in &dependencies {
        let parent = dependency.parent.map_or(root, |name| {
            *nodes.entry(name).or_insert_with(|| graph.add_node(()))
        });
        let child = *nodes
            .entry(dependency.name)
            .or_insert_with(|| graph.add_node(()));
        graph.add_edge(parent, child, dependency.marker);
    }
    let scopes = marker_reachability(&graph, &[]);
    let mut bounds: FxHashMap<_, Vec<_>> = FxHashMap::default();
    for dependency in dependencies {
        // If a cycle has no incoming bounded dependency, only the edge's own
        // marker can establish where its upper bound applies.
        let parent = dependency.parent.map_or(root, |name| nodes[name]);
        let scope = scopes.get(&parent).copied().unwrap_or(MarkerTree::TRUE);
        bounds.entry(dependency.name).or_default().push(ForkBound {
            marker: scope.and(dependency.marker),
            upper: dependency.upper,
        });
    }
    // Store unconditional bounds only alongside a conditional bound on the same package.
    bounds
        .into_iter()
        .filter(|(_, bounds)| bounds.iter().any(|bound| !bound.marker.is_true()))
        .map(|(name, bounds)| (name, bounds.into_boxed_slice()))
        .collect()
}
