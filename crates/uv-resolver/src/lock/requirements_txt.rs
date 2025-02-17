use std::borrow::Cow;
use std::collections::hash_map::Entry;
use std::collections::{BTreeSet, VecDeque};
use std::fmt::Formatter;
use std::path::{Component, Path, PathBuf};

use either::Either;
use itertools::Itertools;
use petgraph::graph::NodeIndex;
use petgraph::prelude::EdgeRef;
use petgraph::visit::IntoNodeReferences;
use petgraph::{Direction, Graph};
use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};
use url::Url;

use uv_configuration::{DevGroupsManifest, EditableMode, ExtrasSpecification, InstallOptions};
use uv_distribution_filename::{DistExtension, SourceDistExtension};
use uv_fs::Simplified;
use uv_git_types::GitReference;
use uv_normalize::{ExtraName, GroupName, PackageName};
use uv_pep508::{ExtraOperator, MarkerExpression, MarkerTree};
use uv_pypi_types::{ConflictItem, ParsedArchiveUrl, ParsedGitUrl};

use crate::graph_ops::{marker_reachability, simplify_conflict_markers, Reachable};
use crate::lock::{LockErrorKind, Package, PackageId, Source};
use crate::{ConflictMarker, Installable, LockError, UniversalMarker};

/// An export of a [`Lock`] that renders in `requirements.txt` format.
#[derive(Debug)]
pub struct RequirementsTxtExport<'lock> {
    nodes: Vec<Requirement<'lock>>,
    hashes: bool,
    editable: EditableMode,
}

impl<'lock> RequirementsTxtExport<'lock> {
    pub fn from_lock(
        target: &impl Installable<'lock>,
        prune: &[PackageName],
        extras: &ExtrasSpecification,
        dev: &DevGroupsManifest,
        editable: EditableMode,
        hashes: bool,
        install_options: &'lock InstallOptions,
    ) -> Result<Self, LockError> {
        let size_guess = target.lock().packages.len();
        let mut petgraph = Graph::<Node<'lock>, Edge<'lock>>::with_capacity(size_guess, size_guess);
        let mut inverse = FxHashMap::with_capacity_and_hasher(size_guess, FxBuildHasher);

        let mut queue: VecDeque<(&Package, Option<&ExtraName>)> = VecDeque::new();
        let mut seen = FxHashSet::default();
        let mut activated_extras: Vec<(&PackageName, &ExtraName)> = vec![];
        let mut activated_groups: Vec<(&PackageName, &GroupName)> = vec![];

        let root = petgraph.add_node(Node::Root);

        let mut base_map = FxHashMap::default();

        // Add the workspace packages to the queue.
        for root_name in target.roots() {
            if prune.contains(root_name) {
                continue;
            }

            let dist = target
                .lock()
                .find_by_name(root_name)
                .expect("found too many packages matching root")
                .expect("could not find root");

            if dev.prod() {
                // Add the workspace package to the graph.
                if let Entry::Vacant(entry) = inverse.entry(&dist.id) {
                    entry.insert(petgraph.add_node(Node::Package(dist)));
                }

                // Add an edge from the root.
                let index = inverse[&dist.id];
                petgraph.add_edge(root, index, Edge::Prod(MarkerTree::TRUE));

                // Push its dependencies on the queue.
                queue.push_back((dist, None));
                for extra in extras.extra_names(dist.optional_dependencies.keys()) {
                    queue.push_back((dist, Some(extra)));
                    base_map.insert(ConflictItem::from((dist.id.name.clone(), extra.clone())), MarkerTree::TRUE);
                }
            }

            // Add any development dependencies.
            for (group, dep) in dist
                .dependency_groups
                .iter()
                .filter_map(|(group, deps)| {
                    if dev.contains(group) {
                        Some(deps.iter().map(move |dep| (group, dep)))
                    } else {
                        None
                    }
                })
                .flatten()
            {
                if prune.contains(&dep.package_id.name) {
                    continue;
                }

                activated_groups.push((&dist.id.name, group));

                let dep_dist = target.lock().find_by_id(&dep.package_id);

                // Add the dependency to the graph.
                if let Entry::Vacant(entry) = inverse.entry(&dep.package_id) {
                    entry.insert(petgraph.add_node(Node::Package(dep_dist)));
                }

                // Add an edge from the root. Development dependencies may be installed without
                // installing the workspace package itself (which can never have markers on it
                // anyway), so they're directly connected to the root.
                let dep_index = inverse[&dep.package_id];
                petgraph.add_edge(
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
        // (legacy) non-project workspace roots).
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
                        if dev.contains(group) {
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

                    // Add the dependency to the graph.
                    if let Entry::Vacant(entry) = inverse.entry(&dist.id) {
                        entry.insert(petgraph.add_node(Node::Package(dist)));
                    }

                    // Add an edge from the root.
                    let dep_index = inverse[&dist.id];
                    petgraph.add_edge(root, dep_index, Edge::Prod(marker));

                    // Push its dependencies on the queue.
                    if seen.insert((&dist.id, None)) {
                        queue.push_back((dist, None));
                    }
                    for extra in &requirement.extras {
                        if seen.insert((&dist.id, Some(extra))) {
                            queue.push_back((dist, Some(extra)));
                            activated_extras.push((&dist.id.name, extra));
                        }
                    }
                }
            }
        }

        // // See: [`Installable::to_resolution`].
        // if !target.lock().conflicts().is_empty() {
        //     let mut activated_extras_set: BTreeSet<(&PackageName, &ExtraName)> =
        //         activated_extras.iter().copied().collect();
        //     let mut queue = queue.clone();
        //     let mut seen = seen.clone();
        //     while let Some((package, extra)) = queue.pop_front() {
        //         let deps = if let Some(extra) = extra {
        //             Either::Left(
        //                 package
        //                     .optional_dependencies
        //                     .get(extra)
        //                     .into_iter()
        //                     .flatten(),
        //             )
        //         } else {
        //             Either::Right(package.dependencies.iter())
        //         };
        //         for dep in deps {
        //             let mut additional_activated_extras = vec![];
        //             for extra in &dep.extra {
        //                 let key = (&dep.package_id.name, extra);
        //                 if !activated_extras_set.contains(&key) {
        //                     additional_activated_extras.push(key);
        //                 }
        //             }
        //             let temp_activated_extras = if additional_activated_extras.is_empty() {
        //                 Cow::Borrowed(&activated_extras)
        //             } else {
        //                 let mut owned = activated_extras.clone();
        //                 owned.extend_from_slice(&additional_activated_extras);
        //                 Cow::Owned(owned)
        //             };
        //             if !dep
        //                 .complexified_marker
        //                 .conflict()
        //                 .evaluate(&temp_activated_extras, &activated_groups)
        //             {
        //                 continue;
        //             }
        //             for key in additional_activated_extras {
        //                 activated_extras_set.insert(key);
        //                 activated_extras.push(key);
        //             }
        //             let dep_dist = target.lock().find_by_id(&dep.package_id);
        //
        //             // Push its dependencies on the queue.
        //             if seen.insert((&dep.package_id, None)) {
        //                 queue.push_back((dep_dist, None));
        //             }
        //             for extra in &dep.extra {
        //                 if seen.insert((&dep.package_id, Some(extra))) {
        //                     queue.push_back((dep_dist, Some(extra)));
        //                 }
        //             }
        //         }
        //     }
        //     for set in target.lock().conflicts().iter() {
        //         for ((pkg1, extra1), (pkg2, extra2)) in
        //             activated_extras_set.iter().tuple_combinations()
        //         {
        //             if set.contains(pkg1, *extra1) && set.contains(pkg2, *extra2) {
        //                 return Err(LockErrorKind::ConflictingExtra {
        //                     package1: (*pkg1).clone(),
        //                     extra1: (*extra1).clone(),
        //                     package2: (*pkg2).clone(),
        //                     extra2: (*extra2).clone(),
        //                 }
        //                 .into());
        //             }
        //         }
        //     }
        // }

        // Why is it problematic to collect activated extras?
        //
        // Well, we have on `package[cpu]` and `package[gpu`] that conflict.
        //
        // Maybe the top-level has:
        // ```
        // ["package[cpu] ; sys_platform == 'darwin'", "package[gpu] ; sys_platform == 'linux'"]
        // ```
        //
        // So then we detect that both `package[cpu]` and `package[gpu]` are "activated".
        //
        // What if we then have a dependency that's like...
        // ```
        // torch==2.6.0 ; (python_version >= '3.7' and package[cpu]) or (package[gpu])
        // ```
        //
        // It seems like what we need is... we have to replace the conflict marker with the expression
        // that would cause it to be true or false...
        //
        // We want this to simplify to...
        // ```
        // torch==2.6.0 ; (sys_platform == 'darwin' and python_version >= '3.7') or (sys_platform == 'linux')
        // ```
        //
        // We actually just want to get rid of all the conflict markers, but we don't want them to
        // evaluate to `true`... At minimum, we need the ones that are actual conflicts to evaluate
        // to `false`.

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

                // if !dep
                //     .complexified_marker
                //     .conflict()
                //     .evaluate(&activated_extras, &activated_groups)
                // {
                //     continue;
                // }

                // Evaluate the conflict marker.
                let dep_dist = target.lock().find_by_id(&dep.package_id);

                // Add the dependency to the graph.
                if let Entry::Vacant(entry) = inverse.entry(&dep.package_id) {
                    entry.insert(petgraph.add_node(Node::Package(dep_dist)));
                }

                // Add the edge.
                let dep_index = inverse[&dep.package_id];
                petgraph.add_edge(
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

        let mut reachability = {
            // Ok, so... for each node, we want to a map from conflict item to marker.
            let mut conflict_maps = FxHashMap::<NodeIndex, FxHashMap<ConflictItem, MarkerTree>>::with_capacity_and_hasher(petgraph.node_count(), FxBuildHasher);

            // Perform a BFS.
            // Collect the root nodes.
            //
            // Besides the actual virtual root node, virtual dev dependencies packages are also root
            // nodes since the edges don't cover dev dependencies.
            let mut queue: Vec<_> = petgraph
                .node_indices()
                .filter(|node_index| {
                    petgraph
                        .edges_directed(*node_index, Direction::Incoming)
                        .next()
                        .is_none()
                })
                .collect();

            fn replace(
                marker: MarkerTree,
                conflict_map: &FxHashMap<ConflictItem, MarkerTree>,
            ) -> MarkerTree {
                if marker.is_true() || marker.is_false() {
                    return marker;
                }

                println!("attempting {:?} with {:?}", marker, conflict_map);

                let mut marker = marker.to_dnf();
                let mut transformed = MarkerTree::FALSE;
                for m in marker.iter_mut() {
                    let mut or = MarkerTree::TRUE;
                    for m_i in m.iter_mut() {
                        if let MarkerExpression::Extra { operator, name } = m_i {
                            let name = name.as_extra().unwrap();
                            let mut found = false;
                            for (conflict_item, conflict_marker) in conflict_map {
                                if let Some(extra) = conflict_item.extra() {
                                    let package = conflict_item.package();
                                    let package_len = package.as_str().len();
                                    let encoded = ExtraName::new(format!(
                                        "extra-{package_len}-{package}-{extra}"
                                    ))
                                    .unwrap();
                                    if encoded == *name {
                                        match operator {
                                            ExtraOperator::Equal => {
                                                or.and(conflict_marker.clone());
                                                found = true;
                                                break;
                                            }
                                            ExtraOperator::NotEqual => {
                                                or.and(conflict_marker.negate());
                                                found = true;
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                            if !found {
                                match operator {
                                    ExtraOperator::Equal => {
                                        println!("Couldn't find {:?}; replacing with FALSE", name);
                                        or.and(MarkerTree::FALSE);
                                    }
                                    ExtraOperator::NotEqual => {
                                        println!("Couldn't find {:?}; replacing with TRUE", name);
                                        or.and(MarkerTree::TRUE);
                                    }
                                }
                            }
                        } else {
                            or.and(MarkerTree::expression(m_i.clone()));
                        }
                    }
                    transformed.or(or);
                }

                println!("transformed: {:?}", transformed);
                println!();

                transformed
            }

            let mut reachability = FxHashMap::<NodeIndex, MarkerTree>::with_capacity_and_hasher(petgraph.node_count(), FxBuildHasher);
            for root_index in &queue {
                reachability.insert(*root_index, MarkerTree::TRUE);
            }

            // Propagate all markers through the graph, so that the eventual marker for each node is the
            // union of the markers of each path we can reach the node by.
            //
            // STOPSHIP(charlie): I think this assumes no cycles... Which might be a blocker?
            while let Some(parent_index) = queue.pop() {
                if let Node::Package(package) = &petgraph[parent_index] {
                    println!("parent: {:?}", package.id);
                }

                // Here, we should take the reachability and "fix" it.
                reachability
                    .entry(parent_index)
                    .and_modify(|marker| {
                        let conflict_map = conflict_maps.get(&parent_index).unwrap_or(&base_map);
                        println!("parent marker: {:?}", marker);
                        *marker = replace(*marker, &conflict_map);
                        println!("replaced parent with: {:?}", marker);
                    });

                // When we see an edge like parent [dotenv]> flask, we should take the reachability
                // on `parent`, combine it with the marker on the edge, then add `flask[dotenv]` to
                // the inference map on the `flask` node.
                //
                // What if there are multiple edges? Combine the maps with an OR?
                //
                // Then, at the end, we take the marker on the node, convert to DNF, and replace any
                // conflict markers with the implied edges.
                for child_edge in petgraph.edges_directed(parent_index, Direction::Outgoing) {
                    let mut parent_marker = reachability[&parent_index];

                    if let Node::Package(package) = &petgraph[child_edge.target()] {
                        println!("child: {:?}", package.id);
                    }

                    // The marker for all paths to the child through the parent.
                    let mut parent_map = conflict_maps
                        .get(&parent_index)
                        .cloned()
                        .unwrap_or_else(|| base_map.clone());

                    match child_edge.weight() {
                        Edge::Prod(marker) => {
                            let marker = replace(*marker, &parent_map);
                            for (i, value) in parent_map.iter_mut() {
                                value.and(marker);
                                println!("Set {:?} to {:?}", i, value);
                            }
                            parent_marker.and(marker);
                        }
                        Edge::Optional(extra, marker) => {
                            let marker = replace(*marker, &parent_map);
                            for (i, value) in parent_map.iter_mut() {
                                value.and(marker);
                                println!("Set {:?} to {:?}", i, value);
                            }
                            let parent_name = match petgraph[child_edge.source()] {
                                Node::Package(package) => &package.id.name,
                                Node::Root => panic!("root node has no name"),
                            };
                            let item = ConflictItem::from((parent_name.clone(), (*extra).clone()));

                            let mut x = parent_marker;
                            x.and(marker);
                            println!("Set {:?} to {:?}", item, x);
                            parent_map.insert(item, x);

                            parent_marker.and(marker);
                        }
                        Edge::Dev(group, marker) => {
                            let marker = replace(*marker, &parent_map);
                            for (i, value) in parent_map.iter_mut() {
                                // We need to "replace" on this using the parent map.
                                value.and(marker);
                                println!("Set {:?} to {:?}", i, value);
                            }
                            parent_marker.and(marker);
                        }
                    }

                    match conflict_maps.entry(child_edge.target()) {
                        Entry::Occupied(mut existing) => {
                            let child_map = existing.get_mut();
                            for (key, value) in parent_map {
                                let mut after = child_map.get(&key).cloned().unwrap_or(MarkerTree::FALSE);
                                after.or(value);
                                println!("Set {:?} to {:?}", key, after);
                                child_map.entry(key).or_insert(MarkerTree::FALSE).or(value);
                            }
                        }
                        Entry::Vacant(vacant) => {
                            vacant.insert(parent_map);
                        }
                    }

                    match reachability.entry(child_edge.target()) {
                        Entry::Occupied(mut existing) => {
                            let child_marker = existing.get_mut();
                            child_marker.or(parent_marker);
                            println!("Set child to {:?}", child_marker);
                        }
                        Entry::Vacant(vacant) => {
                            println!("Set child to {:?}", parent_marker);
                            vacant.insert(parent_marker);
                        }
                    }

                    queue.push(child_edge.target());
                }
            }

            for i in petgraph.node_indices() {
                let node = &petgraph[i];
                let marker = reachability[&i];
                let conflict_map = conflict_maps.get(&i).cloned().unwrap_or_default();
                match node {
                    Node::Root => {}
                    Node::Package(p) => {
                        println!("node: {:?}", p.id);
                        println!("marker: {:?}", marker);
                        println!("conflict_map: {:?}", conflict_map);
                        println!();
                    }
                }
            }

            reachability
        };

        // Collect all packages.
        let mut nodes = petgraph
            .node_references()
            .filter_map(|(index, node)| match node {
                Node::Root => None,
                Node::Package(package) => Some((index, package)),
            })
            .filter(|(_index, package)| {
                install_options.include_package(
                    &package.id.name,
                    target.project_name(),
                    target.lock().members(),
                )
            })
            .map(|(index, package)| Requirement {
                package,
                marker: reachability.remove(&index).unwrap_or_default(),
            })
            .filter(|requirement| !requirement.marker.is_false())
            // .map(|Requirement { package, marker }| {
            //     {
            //         // I somehow need to call simplify_conflict_markers here...
            //         println!("package: {:?}", package.name());
            //         println!("marker: {:?}", marker);
            //         let mut marker = UniversalMarker::from_combined(marker);
            //         for (package, extra) in &activated_extras {
            //             marker.assume_conflict_item(&ConflictItem::from((
            //                 (*package).clone(),
            //                 (*extra).clone(),
            //             )));
            //         }
            //         println!("assume_conflict_items: {:?}", marker);
            //         let marker = marker.drop_extras();
            //         println!("drop_extras: {:?}", marker);
            //
            //         // TODO(charlie): Then here, we should mark them all as false? What happens?
            //         // I guess we can try.
            //         Requirement { package, marker }
            //     }
            // })
            .collect::<Vec<_>>();

        // Sort the nodes, such that unnamed URLs (editables) appear at the top.
        nodes.sort_unstable_by(|a, b| {
            RequirementComparator::from(a.package).cmp(&RequirementComparator::from(b.package))
        });

        Ok(Self {
            nodes,
            hashes,
            editable,
        })
    }
}


/// Determine the markers under which a package is reachable in the dependency tree.
///
/// The algorithm is a variant of Dijkstra's algorithm for not totally ordered distances:
/// Whenever we find a shorter distance to a node (a marker that is not a subset of the existing
/// marker), we re-queue the node and update all its children. This implicitly handles cycles,
/// whenever we re-reach a node through a cycle the marker we have is a more
/// specific marker/longer path, so we don't update the node and don't re-queue it.
fn conflict_marker_reachability<'lock>(
    graph: &Graph<Node<'lock>, Edge<'lock>>,
    fork_markers: &[Edge<'lock>],
) -> FxHashMap<NodeIndex, Edge<'lock>> {
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
            .fold(Edge::false_marker(), |mut acc, marker| {
                acc.or(*marker);
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
            let mut child_marker = *child_edge.weight();
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


impl std::fmt::Display for RequirementsTxtExport<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        // Write out each package.
        for Requirement { package, marker } in &self.nodes {
            match &package.id.source {
                Source::Registry(_) => {
                    let version = package
                        .id
                        .version
                        .as_ref()
                        .expect("registry package without version");
                    write!(f, "{}=={}", package.id.name, version)?;
                }
                Source::Git(url, git) => {
                    // Remove the fragment and query from the URL; they're already present in the
                    // `GitSource`.
                    let mut url = url.to_url().map_err(|_| std::fmt::Error)?;
                    url.set_fragment(None);
                    url.set_query(None);

                    // Reconstruct the `GitUrl` from the `GitSource`.
                    let git_url = uv_git_types::GitUrl::from_commit(
                        url,
                        GitReference::from(git.kind.clone()),
                        git.precise,
                    )
                    .expect("Internal Git URLs must have supported schemes");

                    // Reconstruct the PEP 508-compatible URL from the `GitSource`.
                    let url = Url::from(ParsedGitUrl {
                        url: git_url.clone(),
                        subdirectory: git.subdirectory.as_ref().map(PathBuf::from),
                    });

                    write!(f, "{} @ {}", package.id.name, url)?;
                }
                Source::Direct(url, direct) => {
                    let subdirectory = direct.subdirectory.as_ref().map(PathBuf::from);
                    let url = Url::from(ParsedArchiveUrl {
                        url: url.to_url().map_err(|_| std::fmt::Error)?,
                        subdirectory: subdirectory.clone(),
                        ext: DistExtension::Source(SourceDistExtension::TarGz),
                    });
                    write!(f, "{} @ {}", package.id.name, url)?;
                }
                Source::Path(path) | Source::Directory(path) => {
                    if path.is_absolute() {
                        write!(
                            f,
                            "{}",
                            Url::from_file_path(path).map_err(|()| std::fmt::Error)?
                        )?;
                    } else {
                        write!(f, "{}", anchor(path).portable_display())?;
                    }
                }
                Source::Editable(path) => match self.editable {
                    EditableMode::Editable => {
                        write!(f, "-e {}", anchor(path).portable_display())?;
                    }
                    EditableMode::NonEditable => {
                        if path.is_absolute() {
                            write!(
                                f,
                                "{}",
                                Url::from_file_path(path).map_err(|()| std::fmt::Error)?
                            )?;
                        } else {
                            write!(f, "{}", anchor(path).portable_display())?;
                        }
                    }
                },
                Source::Virtual(_) => {
                    continue;
                }
            }

            if let Some(contents) = marker.contents() {
                write!(f, " ; {contents}")?;
            }

            if self.hashes {
                let mut hashes = package.hashes();
                hashes.sort_unstable();
                if !hashes.is_empty() {
                    for hash in &hashes {
                        writeln!(f, " \\")?;
                        write!(f, "    --hash=")?;
                        write!(f, "{hash}")?;
                    }
                }
            }

            writeln!(f)?;
        }

        Ok(())
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

/// A flat requirement, with its associated marker.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Requirement<'lock> {
    package: &'lock Package,
    marker: MarkerTree,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum RequirementComparator<'lock> {
    Editable(&'lock Path),
    Path(&'lock Path),
    Package(&'lock PackageId),
}

impl<'lock> From<&'lock Package> for RequirementComparator<'lock> {
    fn from(value: &'lock Package) -> Self {
        match &value.id.source {
            Source::Path(path) | Source::Directory(path) => Self::Path(path),
            Source::Editable(path) => Self::Editable(path),
            _ => Self::Package(&value.id),
        }
    }
}

/// Modify a relative [`Path`] to anchor it at the current working directory.
///
/// For example, given `foo/bar`, returns `./foo/bar`.
fn anchor(path: &Path) -> Cow<'_, Path> {
    match path.components().next() {
        None => Cow::Owned(PathBuf::from(".")),
        Some(Component::CurDir | Component::ParentDir) => Cow::Borrowed(path),
        _ => Cow::Owned(PathBuf::from("./").join(path)),
    }
}
