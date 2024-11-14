use std::collections::VecDeque;

use petgraph::visit::EdgeRef;
use petgraph::Direction;
use pubgrub::{Kind, Range, SelectedDependencies, State};
use rustc_hash::FxHashSet;

use uv_distribution_types::{
    DerivationChain, DerivationStep, DistRef, Edge, Name, Node, Resolution, ResolvedDist,
};
use uv_pep440::Version;

use crate::dependency_provider::UvDependencyProvider;
use crate::pubgrub::PubGrubPackage;

/// A chain of derivation steps from the root package to the current package, to explain why a
/// package is included in the resolution.
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]
pub struct DerivationChainBuilder;

impl DerivationChainBuilder {
    /// Compute a [`DerivationChain`] from a resolution graph.
    ///
    /// This is used to construct a derivation chain upon install failure in the `uv pip` context,
    /// where we don't have a lockfile describing the resolution.
    pub fn from_resolution(
        resolution: &Resolution,
        target: DistRef<'_>,
    ) -> Option<DerivationChain> {
        // Find the target distribution in the resolution graph.
        let target = resolution.graph().node_indices().find(|node| {
            let Node::Dist {
                dist: ResolvedDist::Installable { dist, .. },
                ..
            } = &resolution.graph()[*node]
            else {
                return false;
            };
            target == dist.as_ref()
        })?;

        // Perform a BFS to find the shortest path to the root.
        let mut queue = VecDeque::new();
        queue.push_back((target, None, None, Vec::new()));

        // TODO(charlie): Consider respecting markers here.
        let mut seen = FxHashSet::default();
        while let Some((node, extra, group, mut path)) = queue.pop_front() {
            if !seen.insert(node) {
                continue;
            }
            match &resolution.graph()[node] {
                Node::Root => {
                    path.reverse();
                    path.pop();
                    return Some(DerivationChain::from_iter(path));
                }
                Node::Dist { dist, .. } => {
                    for edge in resolution.graph().edges_directed(node, Direction::Incoming) {
                        let mut path = path.clone();
                        path.push(DerivationStep::new(
                            dist.name().clone(),
                            extra.clone(),
                            group.clone(),
                            dist.version().clone(),
                            Range::empty(),
                        ));
                        let target = edge.source();
                        let extra = match edge.weight() {
                            Edge::Optional(extra, ..) => Some(extra.clone()),
                            _ => None,
                        };
                        let group = match edge.weight() {
                            Edge::Dev(group, ..) => Some(group.clone()),
                            _ => None,
                        };
                        queue.push_back((target, extra, group, path));
                    }
                }
            }
        }

        None
    }

    /// Compute a [`DerivationChain`] from the current PubGrub state.
    ///
    /// This is used to construct a derivation chain upon resolution failure.
    pub(crate) fn from_state(
        package: &PubGrubPackage,
        version: &Version,
        state: &State<UvDependencyProvider>,
    ) -> Option<DerivationChain> {
        /// Find a path from the current package to the root package.
        fn find_path(
            package: &PubGrubPackage,
            version: &Version,
            state: &State<UvDependencyProvider>,
            solution: &SelectedDependencies<UvDependencyProvider>,
            path: &mut Vec<DerivationStep>,
        ) -> bool {
            // Retrieve the incompatibilities for the current package.
            let Some(incompatibilities) = state.incompatibilities.get(package) else {
                return false;
            };
            for index in incompatibilities {
                let incompat = &state.incompatibility_store[*index];

                // Find a dependency from a package to the current package.
                if let Kind::FromDependencyOf(p1, _, p2, v2) = &incompat.kind {
                    if p2 == package && v2.contains(version) {
                        if let Some(version) = solution.get(p1) {
                            if p1.name_no_root() == p2.name_no_root() {
                                // Skip proxied dependencies.
                                if find_path(p1, version, state, solution, path) {
                                    return true;
                                }
                            } else if let Some(name) = p1.name_no_root() {
                                // Add to the current path.
                                path.push(DerivationStep::new(
                                    name.clone(),
                                    p1.extra().cloned(),
                                    p1.dev().cloned(),
                                    version.clone(),
                                    v2.clone(),
                                ));

                                // Recursively search the next package.
                                if find_path(p1, version, state, solution, path) {
                                    return true;
                                }

                                // Backtrack if the path didn't lead to the root.
                                path.pop();
                            } else {
                                // If we've reached the root, return.
                                return true;
                            }
                        }
                    }
                }
            }
            false
        }

        let solution = state.partial_solution.extract_solution();
        let path = {
            let mut path = vec![];
            if !find_path(package, version, state, &solution, &mut path) {
                return None;
            }
            path.reverse();
            path
        };

        Some(path.into_iter().collect())
    }
}
