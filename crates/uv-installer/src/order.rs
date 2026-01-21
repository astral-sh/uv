//! Sort packages in dependency order for deterministic installation.
//!
//! When packages have conflicting files, the installation order determines which
//! package's files "win". Installing in dependency order (dependencies first)
//! ensures that if a package depends on another, the dependent package's files
//! take precedence, matching pip's behavior.
//!
//! This module handles circular dependencies by using Tarjan's algorithm to find
//! strongly connected components (SCCs). Packages in cycles are grouped together
//! and sorted deterministically by name within their SCC.

use petgraph::algo::tarjan_scc;
use petgraph::graph::NodeIndex;
use rustc_hash::FxHashMap;

use uv_distribution_types::{CachedDist, Name, Node, Resolution};
use uv_normalize::PackageName;

/// Sort cached distributions in dependency order based on the resolution graph.
///
/// Returns the distributions sorted so that dependencies are installed before
/// their dependents. This ensures deterministic behavior when packages have
/// conflicting files - the dependent package's files will overwrite those
/// from its dependencies.
///
/// Circular dependencies are handled by grouping them into strongly connected
/// components (SCCs) and sorting packages within each SCC alphabetically by name
/// for deterministic ordering.
pub fn sort_by_dependency_order(
    wheels: Vec<CachedDist>,
    resolution: &Resolution,
) -> Vec<CachedDist> {
    if wheels.len() <= 1 {
        return wheels;
    }

    let graph = resolution.graph();

    // Build a mapping from package name to node index
    let mut name_to_node: FxHashMap<&PackageName, NodeIndex> = FxHashMap::default();
    for node_index in graph.node_indices() {
        if let Node::Dist { dist, install, .. } = &graph[node_index] {
            if *install {
                name_to_node.insert(dist.name(), node_index);
            }
        }
    }

    // Use Tarjan's algorithm to find strongly connected components.
    // This handles cycles by grouping cyclically-dependent packages together.
    // The SCCs are returned in reverse topological order (dependencies come later).
    let sccs = tarjan_scc(graph);

    // Assign positions based on SCC order.
    // SCCs are returned in reverse topological order, so we reverse to get
    // dependencies first. Within each SCC, sort by package name for determinism.
    let mut name_to_position: FxHashMap<&PackageName, (usize, &PackageName)> = FxHashMap::default();
    for (scc_index, scc) in sccs.iter().rev().enumerate() {
        for node_index in scc {
            if let Node::Dist { dist, install, .. } = &graph[*node_index] {
                if *install {
                    // Use (scc_index, name) as the sort key.
                    // This ensures packages in earlier SCCs come first,
                    // and within the same SCC, packages are sorted by name.
                    name_to_position.insert(dist.name(), (scc_index, dist.name()));
                }
            }
        }
    }

    // Sort the wheels based on their position
    let mut sorted_wheels = wheels;
    sorted_wheels.sort_by(|a, b| {
        let key_a = name_to_position
            .get(a.name())
            .copied()
            .unwrap_or((usize::MAX, a.name()));
        let key_b = name_to_position
            .get(b.name())
            .copied()
            .unwrap_or((usize::MAX, b.name()));
        key_a.cmp(&key_b)
    });

    sorted_wheels
}

#[cfg(test)]
mod tests {
    use super::*;

    // Tests would go here, but they require complex setup of Resolution graphs
    // Integration tests in crates/uv/tests/it/ are more appropriate
}
