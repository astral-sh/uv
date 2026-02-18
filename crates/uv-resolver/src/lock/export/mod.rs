use std::collections::VecDeque;
use std::collections::hash_map::Entry;

use either::Either;
use petgraph::graph::NodeIndex;
use petgraph::prelude::EdgeRef;
use petgraph::visit::IntoNodeReferences;
use petgraph::{Direction, Graph};
use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};

use uv_configuration::{
    DependencyGroupsWithDefaults, ExtrasSpecificationWithDefaults, InstallOptions,
};
use uv_normalize::{ExtraName, GroupName, PackageName};
use uv_pep508::MarkerTree;
use uv_pypi_types::ConflictItem;

use crate::graph_ops::{Reachable, marker_reachability};
use crate::lock::LockErrorKind;
pub(crate) use crate::lock::export::pylock_toml::PylockTomlPackage;
pub use crate::lock::export::pylock_toml::{PylockToml, PylockTomlErrorKind};
pub use crate::lock::export::requirements_txt::RequirementsTxtExport;
use crate::universal_marker::resolve_conflicts;
use crate::{Installable, LockError, Package};

pub mod cyclonedx_json;
mod pylock_toml;
mod requirements_txt;

/// A flat requirement, with its associated marker.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ExportableRequirement<'lock> {
    /// The [`Package`] associated with the requirement.
    package: &'lock Package,
    /// The marker that must be satisfied to install the package.
    marker: MarkerTree,
    /// The list of packages that depend on this package.
    dependents: Vec<&'lock Package>,
}

/// A set of flattened, exportable requirements, generated from a lockfile.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ExportableRequirements<'lock>(Vec<ExportableRequirement<'lock>>);

impl<'lock> ExportableRequirements<'lock> {
    /// Generate the set of exportable [`ExportableRequirement`] entries from the given lockfile.
    fn from_lock(
        target: &impl Installable<'lock>,
        prune: &[PackageName],
        extras: &ExtrasSpecificationWithDefaults,
        groups: &DependencyGroupsWithDefaults,
        annotate: bool,
        install_options: &'lock InstallOptions,
    ) -> Result<Self, LockError> {
        let size_guess = target.lock().packages.len();
        let mut graph = Graph::<Node<'lock>, Edge<'lock>>::with_capacity(size_guess, size_guess);
        let mut inverse = FxHashMap::with_capacity_and_hasher(size_guess, FxBuildHasher);

        let mut queue: VecDeque<(&Package, Option<&ExtraName>)> = VecDeque::new();
        let mut seen = FxHashSet::default();
        let mut conflicts = if target.lock().conflicts.is_empty() {
            None
        } else {
            Some(FxHashMap::default())
        };

        let root = graph.add_node(Node::Root);

        // Add the workspace packages to the queue.
        for root_name in target.roots() {
            if prune.contains(root_name) {
                continue;
            }

            let dist = target
                .lock()
                .find_by_name(root_name)
                .map_err(|_| LockErrorKind::MultipleRootPackages {
                    name: root_name.clone(),
                })?
                .ok_or_else(|| LockErrorKind::MissingRootPackage {
                    name: root_name.clone(),
                })?;

            if groups.prod() {
                // Add the workspace package to the graph.
                let index = *inverse
                    .entry(&dist.id)
                    .or_insert_with(|| graph.add_node(Node::Package(dist)));
                graph.add_edge(root, index, Edge::Prod(MarkerTree::TRUE));

                // Push its dependencies on the queue.
                queue.push_back((dist, None));
                for extra in extras.extra_names(dist.optional_dependencies.keys()) {
                    queue.push_back((dist, Some(extra)));

                    // Track the activated extra in the list of known conflicts.
                    if let Some(conflicts) = conflicts.as_mut() {
                        conflicts.insert(
                            ConflictItem::from((dist.id.name.clone(), extra.clone())),
                            MarkerTree::TRUE,
                        );
                    }
                }
            }

            // Add any development dependencies.
            for (group, dep) in dist
                .dependency_groups
                .iter()
                .filter_map(|(group, deps)| {
                    if groups.contains(group) {
                        Some(deps.iter().map(move |dep| (group, dep)))
                    } else {
                        None
                    }
                })
                .flatten()
            {
                // Track the activated group in the list of known conflicts.
                if let Some(conflicts) = conflicts.as_mut() {
                    conflicts.insert(
                        ConflictItem::from((dist.id.name.clone(), group.clone())),
                        MarkerTree::TRUE,
                    );
                }

                if prune.contains(&dep.package_id.name) {
                    continue;
                }

                let dep_dist = target.lock().find_by_id(&dep.package_id);

                // Add the dependency to the graph.
                let dep_index = *inverse
                    .entry(&dep.package_id)
                    .or_insert_with(|| graph.add_node(Node::Package(dep_dist)));

                // Add an edge from the root. Development dependencies may be installed without
                // installing the workspace package itself (which can never have markers on it
                // anyway), so they're directly connected to the root.
                graph.add_edge(
                    root,
                    dep_index,
                    Edge::Dev(group, dep.simplified_marker.as_simplified_marker_tree()),
                );

                // Push its dependencies on the queue.
                if seen.insert((&dep.package_id, None)) {
                    queue.push_back((dep_dist, None));
                }
                for extra in &dep.extra {
                    if seen.insert((&dep.package_id, Some(extra))) {
                        queue.push_back((dep_dist, Some(extra)));
                    }
                }
            }
        }

        // Add requirements that are exclusive to the workspace root (e.g., dependency groups in
        // non-project workspace roots).
        let root_requirements = target
            .lock()
            .requirements()
            .iter()
            .chain(
                target
                    .lock()
                    .dependency_groups()
                    .iter()
                    .filter_map(|(group, deps)| {
                        if groups.contains(group) {
                            Some(deps)
                        } else {
                            None
                        }
                    })
                    .flatten(),
            )
            .filter(|dep| !prune.contains(&dep.name))
            .collect::<Vec<_>>();

        // Index the lockfile by package name, to avoid making multiple passes over the lockfile.
        if !root_requirements.is_empty() {
            let by_name: FxHashMap<_, Vec<_>> = {
                let names = root_requirements
                    .iter()
                    .map(|dep| &dep.name)
                    .collect::<FxHashSet<_>>();
                target.lock().packages().iter().fold(
                    FxHashMap::with_capacity_and_hasher(size_guess, FxBuildHasher),
                    |mut map, package| {
                        if names.contains(&package.id.name) {
                            map.entry(&package.id.name).or_default().push(package);
                        }
                        map
                    },
                )
            };

            for requirement in root_requirements {
                for dist in by_name.get(&requirement.name).into_iter().flatten() {
                    // Determine whether this entry is "relevant" for the requirement, by intersecting
                    // the markers.
                    let marker = if dist.fork_markers.is_empty() {
                        requirement.marker
                    } else {
                        let mut combined = MarkerTree::FALSE;
                        for fork_marker in &dist.fork_markers {
                            combined.or(fork_marker.pep508());
                        }
                        combined.and(requirement.marker);
                        combined
                    };

                    if marker.is_false() {
                        continue;
                    }

                    // Simplify the marker.
                    let marker = target.lock().simplify_environment(marker);

                    // Add the dependency to the graph and get its index.
                    let dep_index = *inverse
                        .entry(&dist.id)
                        .or_insert_with(|| graph.add_node(Node::Package(dist)));

                    // Add an edge from the root.
                    graph.add_edge(root, dep_index, Edge::Prod(marker));

                    // Push its dependencies on the queue.
                    if seen.insert((&dist.id, None)) {
                        queue.push_back((dist, None));
                    }
                    for extra in &requirement.extras {
                        if seen.insert((&dist.id, Some(extra))) {
                            queue.push_back((dist, Some(extra)));
                        }
                    }
                }
            }
        }

        // Create all the relevant nodes.
        while let Some((package, extra)) = queue.pop_front() {
            let index = inverse[&package.id];

            let deps = if let Some(extra) = extra {
                Either::Left(
                    package
                        .optional_dependencies
                        .get(extra)
                        .into_iter()
                        .flatten(),
                )
            } else {
                Either::Right(package.dependencies.iter())
            };

            for dep in deps {
                if prune.contains(&dep.package_id.name) {
                    continue;
                }

                // Evaluate the conflict marker.
                let dep_dist = target.lock().find_by_id(&dep.package_id);

                // Add the dependency to the graph.
                let dep_index = *inverse
                    .entry(&dep.package_id)
                    .or_insert_with(|| graph.add_node(Node::Package(dep_dist)));

                // Add an edge from the dependency.
                graph.add_edge(
                    index,
                    dep_index,
                    if let Some(extra) = extra {
                        Edge::Optional(extra, dep.simplified_marker.as_simplified_marker_tree())
                    } else {
                        Edge::Prod(dep.simplified_marker.as_simplified_marker_tree())
                    },
                );

                // Push its dependencies on the queue.
                if seen.insert((&dep.package_id, None)) {
                    queue.push_back((dep_dist, None));
                }
                for extra in &dep.extra {
                    if seen.insert((&dep.package_id, Some(extra))) {
                        queue.push_back((dep_dist, Some(extra)));
                    }
                }
            }
        }

        // Determine the reachability of each node in the graph.
        let mut reachability = if let Some(conflicts) = conflicts.as_ref() {
            conflict_marker_reachability(&graph, &[], conflicts)
        } else {
            marker_reachability(&graph, &[])
        };

        // Collect all packages.
        let nodes = graph
            .node_references()
            .filter_map(|(index, node)| match node {
                Node::Root => None,
                Node::Package(package) => Some((index, package)),
            })
            .filter(|(_index, package)| {
                install_options.include_package(
                    package.as_install_target(),
                    target.project_name(),
                    target.lock().members(),
                )
            })
            .map(|(index, package)| ExportableRequirement {
                package,
                marker: reachability.remove(&index).unwrap_or_default(),
                dependents: if annotate {
                    let mut dependents = graph
                        .edges_directed(index, Direction::Incoming)
                        .map(|edge| &graph[edge.source()])
                        .filter_map(|node| match node {
                            Node::Package(package) => Some(*package),
                            Node::Root => None,
                        })
                        .collect::<Vec<_>>();
                    dependents.sort_unstable_by_key(|package| package.name());
                    dependents.dedup_by_key(|package| package.name());
                    dependents
                } else {
                    Vec::new()
                },
            })
            .filter(|requirement| !requirement.marker.is_false())
            .collect::<Vec<_>>();

        Ok(Self(nodes))
    }
}

/// A node in the graph.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Node<'lock> {
    Root,
    Package(&'lock Package),
}

/// An edge in the resolution graph, along with the marker that must be satisfied to traverse it.
#[derive(Debug, Clone)]
enum Edge<'lock> {
    Prod(MarkerTree),
    Optional(&'lock ExtraName, MarkerTree),
    Dev(&'lock GroupName, MarkerTree),
}

impl Edge<'_> {
    /// Return the [`MarkerTree`] for this edge.
    fn marker(&self) -> &MarkerTree {
        match self {
            Self::Prod(marker) => marker,
            Self::Optional(_, marker) => marker,
            Self::Dev(_, marker) => marker,
        }
    }
}

impl Reachable<MarkerTree> for Edge<'_> {
    fn true_marker() -> MarkerTree {
        MarkerTree::TRUE
    }

    fn false_marker() -> MarkerTree {
        MarkerTree::FALSE
    }

    fn marker(&self) -> MarkerTree {
        *self.marker()
    }
}

/// Determine the markers under which a package is reachable in the dependency tree, taking into
/// account conflicts.
///
/// This method is structurally similar to [`marker_reachability`], but it _also_ attempts to resolve
/// conflict markers. Specifically, in addition to tracking the reachability marker for each node,
/// we also track (for each node) the conditions under which each conflict item is `true`. Then,
/// when evaluating the marker for the node, we inline the conflict marker conditions, thus removing
/// all conflict items from the marker expression.
fn conflict_marker_reachability<'lock>(
    graph: &Graph<Node<'lock>, Edge<'lock>>,
    fork_markers: &[Edge<'lock>],
    known_conflicts: &FxHashMap<ConflictItem, MarkerTree>,
) -> FxHashMap<NodeIndex, MarkerTree> {
    // For each node, track the conditions under which each conflict item is enabled.
    let mut conflict_maps =
        FxHashMap::<NodeIndex, FxHashMap<ConflictItem, MarkerTree>>::with_capacity_and_hasher(
            graph.node_count(),
            FxBuildHasher,
        );

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
        MarkerTree::TRUE
    } else {
        fork_markers
            .iter()
            .fold(MarkerTree::FALSE, |mut acc, edge| {
                acc.or(*edge.marker());
                acc
            })
    };
    for root_index in &queue {
        reachability.insert(*root_index, root_markers);
    }

    // Propagate all markers through the graph, so that the eventual marker for each node is the
    // union of the markers of each path we can reach the node by.
    while let Some(parent_index) = queue.pop() {
        // Resolve any conflicts in the parent marker.
        reachability.entry(parent_index).and_modify(|marker| {
            let conflict_map = conflict_maps.get(&parent_index).unwrap_or(known_conflicts);
            *marker = resolve_conflicts(*marker, conflict_map);
        });

        // When we see an edge like `parent [dotenv]> flask`, we should take the reachability
        // on `parent`, combine it with the marker on the edge, then add `flask[dotenv]` to
        // the inference map on the `flask` node.
        for child_edge in graph.edges_directed(parent_index, Direction::Outgoing) {
            let mut parent_marker = reachability[&parent_index];

            // The marker for all paths to the child through the parent.
            let mut parent_map = conflict_maps
                .get(&parent_index)
                .cloned()
                .unwrap_or_else(|| known_conflicts.clone());

            match child_edge.weight() {
                Edge::Prod(marker) => {
                    // Resolve any conflicts on the edge.
                    let marker = resolve_conflicts(*marker, &parent_map);

                    // Propagate the edge to the known conflicts.
                    for value in parent_map.values_mut() {
                        value.and(marker);
                    }

                    // Propagate the edge to the node itself.
                    parent_marker.and(marker);
                }
                Edge::Optional(extra, marker) => {
                    // Resolve any conflicts on the edge.
                    let marker = resolve_conflicts(*marker, &parent_map);

                    // Propagate the edge to the known conflicts.
                    for value in parent_map.values_mut() {
                        value.and(marker);
                    }

                    // Propagate the edge to the node itself.
                    parent_marker.and(marker);

                    // Add a known conflict item for the extra.
                    if let Node::Package(parent) = graph[parent_index] {
                        let item = ConflictItem::from((parent.name().clone(), (*extra).clone()));
                        parent_map.insert(item, parent_marker);
                    }
                }
                Edge::Dev(group, marker) => {
                    // Resolve any conflicts on the edge.
                    let marker = resolve_conflicts(*marker, &parent_map);

                    // Propagate the edge to the known conflicts.
                    for value in parent_map.values_mut() {
                        value.and(marker);
                    }

                    // Propagate the edge to the node itself.
                    parent_marker.and(marker);

                    // Add a known conflict item for the group.
                    if let Node::Package(parent) = graph[parent_index] {
                        let item = ConflictItem::from((parent.name().clone(), (*group).clone()));
                        parent_map.insert(item, parent_marker);
                    }
                }
            }

            // Combine the inferred conflicts with the existing conflicts on the node.
            match conflict_maps.entry(child_edge.target()) {
                Entry::Occupied(mut existing) => {
                    let child_map = existing.get_mut();
                    for (key, value) in parent_map {
                        let mut after = child_map.get(&key).copied().unwrap_or(MarkerTree::FALSE);
                        after.or(value);
                        child_map.entry(key).or_insert(MarkerTree::FALSE).or(value);
                    }
                }
                Entry::Vacant(vacant) => {
                    vacant.insert(parent_map);
                }
            }

            // Combine the inferred marker with the existing marker on the node.
            match reachability.entry(child_edge.target()) {
                Entry::Occupied(mut existing) => {
                    // If the marker is a subset of the existing marker (A ⊆ B exactly if
                    // A ∪ B = A), updating the child wouldn't change child's marker.
                    parent_marker.or(*existing.get());
                    if parent_marker != *existing.get() {
                        existing.insert(parent_marker);
                        queue.push(child_edge.target());
                    }
                }
                Entry::Vacant(vacant) => {
                    vacant.insert(parent_marker);
                    queue.push(child_edge.target());
                }
            }
        }
    }

    reachability
}
