use std::collections::VecDeque;
use std::collections::hash_map::Entry;

use either::Either;
use petgraph::graph::{EdgeIndex, NodeIndex};
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

use uv_resolver_types::universal_marker::encode_conflict_item;

pub use crate::lock::export::metadata::{Metadata, PythonReport};
pub(crate) use crate::lock::export::metadata::{
    MetadataNode, MetadataNodeId, MetadataNodeKind, MetadataScript, MetadataWorkspace,
    MetadataWorkspaceMember,
};
pub use crate::lock::export::pylock_toml::{PylockToml, PylockTomlError, PylockTomlErrorKind};
pub use crate::lock::export::requirements_txt::RequirementsTxtExport;
use crate::lock::{LockErrorKind, PackageIndex};
use crate::{Installable, InstallableRootKind, LockError, Package};

pub mod cyclonedx_json;
mod metadata;
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
        let mut inverse = vec![None; size_guess];

        let mut queue: VecDeque<(PackageIndex, Option<&ExtraName>)> = VecDeque::new();
        let mut seen = FxHashSet::default();
        let mut activated_items = FxHashMap::default();

        let root = graph.add_node(Node::Root);

        // Add the workspace packages and any additional dependency-group roots to the queue.
        for (root_name, root_kind) in target
            .roots()
            .map(|root| (root, InstallableRootKind::Production))
            .chain(
                target
                    .group_root(groups)
                    .map(|root| (root, InstallableRootKind::DependencyGroups)),
            )
        {
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

            if root_kind == InstallableRootKind::Production {
                // Track the activated package in the list of known conflicts.
                activated_items.insert(ConflictItem::from(dist.id.name.clone()), MarkerTree::TRUE);
            }

            if root_kind == InstallableRootKind::Production && groups.prod() {
                let package_index = target.lock().by_id[&dist.id];

                // Add the workspace package to the graph.
                let index = *inverse[package_index.0]
                    .get_or_insert_with(|| graph.add_node(Node::Package(dist)));
                graph.add_edge(
                    root,
                    index,
                    Edge::Prod {
                        marker: MarkerTree::TRUE,
                        dep_extras: Vec::new(),
                    },
                );

                // Push its dependencies on the queue.
                queue.push_back((package_index, None));
                for extra in extras.extra_names(dist.optional_dependencies.keys()) {
                    queue.push_back((package_index, Some(extra)));
                    activated_items.insert(
                        ConflictItem::from((dist.id.name.clone(), extra.clone())),
                        MarkerTree::TRUE,
                    );
                }
            }

            // Add any development dependencies.
            for (group, dep) in dist
                .dependency_groups
                .iter()
                .filter_map(|(group, deps)| {
                    if target.includes_group(Some(&dist.id.name), group, groups) {
                        Some(deps.iter().map(move |dep| (group, dep)))
                    } else {
                        None
                    }
                })
                .flatten()
            {
                // Track the activated group in the list of known conflicts.
                activated_items.insert(
                    ConflictItem::from((dist.id.name.clone(), group.clone())),
                    MarkerTree::TRUE,
                );

                if prune.contains(&dep.package_id.name) {
                    continue;
                }

                let dep_dist = target.lock().package(dep.index);

                // Add the dependency to the graph.
                let dep_index = *inverse[dep.index.0]
                    .get_or_insert_with(|| graph.add_node(Node::Package(dep_dist)));

                // Add an edge from the root. Development dependencies may be installed without
                // installing the workspace package itself (which can never have markers on it
                // anyway), so they're directly connected to the root.
                graph.add_edge(
                    root,
                    dep_index,
                    Edge::Dev {
                        group,
                        marker: dep.simplified_marker.as_simplified_marker_tree(),
                        dep_extras: dep.extra.iter().collect(),
                    },
                );

                // Push its dependencies on the queue.
                if seen.insert((dep.index, None)) {
                    queue.push_back((dep.index, None));
                }
                for extra in &dep.extra {
                    if seen.insert((dep.index, Some(extra))) {
                        queue.push_back((dep.index, Some(extra)));
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
                        if target.includes_group(None, group, groups) {
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
                    // Determine whether this entry is relevant for the requirement by
                    // intersecting and simplifying the markers.
                    let Some(marker) = target.lock().root_requirement_marker(requirement, dist)
                    else {
                        continue;
                    };
                    let package_index = target.lock().by_id[&dist.id];

                    // Add the dependency to the graph and get its index.
                    let dep_index = *inverse[package_index.0]
                        .get_or_insert_with(|| graph.add_node(Node::Package(dist)));

                    // Add an edge from the root.
                    graph.add_edge(
                        root,
                        dep_index,
                        Edge::Prod {
                            marker,
                            dep_extras: requirement.extras.iter().collect(),
                        },
                    );

                    // Push its dependencies on the queue.
                    if seen.insert((package_index, None)) {
                        queue.push_back((package_index, None));
                    }
                    for extra in &requirement.extras {
                        if seen.insert((package_index, Some(extra))) {
                            queue.push_back((package_index, Some(extra)));
                        }
                    }
                }
            }
        }

        // Create all the relevant nodes.
        while let Some((package_index, extra)) = queue.pop_front() {
            let index = inverse[package_index.0].expect("queued package has a graph node");
            let package = target.lock().package(package_index);

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
                let dep_dist = target.lock().package(dep.index);

                // Add the dependency to the graph.
                let dep_index = *inverse[dep.index.0]
                    .get_or_insert_with(|| graph.add_node(Node::Package(dep_dist)));

                let dep_extras = dep.extra.iter().collect::<Vec<_>>();
                graph.add_edge(
                    index,
                    dep_index,
                    if let Some(extra) = extra {
                        Edge::Optional {
                            extra,
                            marker: dep.simplified_marker.as_simplified_marker_tree(),
                            dep_extras,
                        }
                    } else {
                        Edge::Prod {
                            marker: dep.simplified_marker.as_simplified_marker_tree(),
                            dep_extras,
                        }
                    },
                );

                // Push its dependencies on the queue.
                if seen.insert((dep.index, None)) {
                    queue.push_back((dep.index, None));
                }
                for extra in &dep.extra {
                    if seen.insert((dep.index, Some(extra))) {
                        queue.push_back((dep.index, Some(extra)));
                    }
                }
            }
        }

        // Determine the reachability of each node in the graph.
        let mut reachability = conflict_marker_reachability(&graph, &activated_items);

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
    Prod {
        marker: MarkerTree,
        dep_extras: Vec<&'lock ExtraName>,
    },
    Optional {
        extra: &'lock ExtraName,
        marker: MarkerTree,
        dep_extras: Vec<&'lock ExtraName>,
    },
    Dev {
        group: &'lock GroupName,
        marker: MarkerTree,
        dep_extras: Vec<&'lock ExtraName>,
    },
}

impl Edge<'_> {
    /// Return the [`MarkerTree`] for this edge.
    fn marker(&self) -> &MarkerTree {
        match self {
            Self::Prod { marker, .. } => marker,
            Self::Optional { marker, .. } => marker,
            Self::Dev { marker, .. } => marker,
        }
    }

    /// Return the dependency extras activated by traversing this edge.
    fn dep_extras(&self) -> &[&ExtraName] {
        match self {
            Self::Prod { dep_extras, .. } => dep_extras,
            Self::Optional { dep_extras, .. } => dep_extras,
            Self::Dev { dep_extras, .. } => dep_extras,
        }
    }
}

/// Determine package reachability while retaining only the activation facts read by markers.
///
/// Optional dependencies read a package-local extra node. Explicit marker expressions read the
/// package's conflict context, which can also receive facts along paths that did not request that
/// optional dependency. Keeping these inputs separate avoids carrying unrelated extras through
/// every transitive dependency while preserving the conditions under which each extra was requested.
fn conflict_marker_reachability<'lock>(
    graph: &Graph<Node<'lock>, Edge<'lock>>,
    known_conflicts: &FxHashMap<ConflictItem, MarkerTree>,
) -> FxHashMap<NodeIndex, MarkerTree> {
    let mut referenced_names = FxHashSet::default();
    let mut referenced_scoped = FxHashSet::default();
    for edge in graph.edge_references() {
        edge.weight().marker().visit_extras(|_, extra| {
            referenced_names.insert(extra.clone());
            if let Node::Package(package) = graph[edge.source()] {
                referenced_scoped
                    .insert(ConflictItem::from((package.name().clone(), extra.clone())));
            }
        });
    }

    // Intern only facts that an explicit marker can observe. An ordinary extra affects its own
    // optional dependencies, but does not need a fact in every descendant's conflict context.
    let mut facts = FxHashMap::default();
    let mut encoded = FxHashMap::default();
    let mut add_fact = |item: ConflictItem| {
        let name = encode_conflict_item(&item);
        if referenced_names.contains(&name) || referenced_scoped.contains(&item) {
            let next = facts.len();
            let index = *facts.entry(item).or_insert(next);
            encoded.insert(name, index);
        }
    };
    for item in known_conflicts.keys() {
        add_fact(item.clone());
    }
    for edge in graph.edge_references() {
        if let Node::Package(package) = graph[edge.target()] {
            for extra in edge.weight().dep_extras() {
                add_fact(ConflictItem::from((
                    package.name().clone(),
                    (*extra).clone(),
                )));
            }
        }
        if let Edge::Dev { group, .. } = edge.weight()
            && let Node::Package(package) = graph[edge.source()]
        {
            add_fact(ConflictItem::from((
                package.name().clone(),
                (*group).clone(),
            )));
        }
    }

    let mut solver = ExportReachability {
        graph,
        edges: Vec::with_capacity(graph.edge_count()),
        reachability: vec![None; graph.node_count()],
        contexts: vec![FxHashMap::default(); graph.node_count()],
        dependents: vec![Vec::new(); graph.node_count()],
        seeds: vec![Vec::new(); graph.node_count()],
        outgoing: vec![Vec::new(); graph.node_count()],
        queue: VecDeque::new(),
        queued: FxHashSet::default(),
    };
    let mut extras = FxHashMap::default();
    for edge in graph.edge_references() {
        let source = match edge.weight() {
            Edge::Optional { extra, .. } => *extras
                .entry((edge.source(), *extra))
                .or_insert_with(|| solver.add_node()),
            Edge::Prod { .. } | Edge::Dev { .. } => edge.source().index(),
        };
        let mut targets = vec![edge.target().index()];
        let mut activated = Vec::new();
        for extra in edge.weight().dep_extras() {
            targets.push(
                *extras
                    .entry((edge.target(), *extra))
                    .or_insert_with(|| solver.add_node()),
            );
            if let Node::Package(package) = graph[edge.target()]
                && let Some(&fact) = facts.get(&ConflictItem::from((
                    package.name().clone(),
                    (*extra).clone(),
                )))
            {
                activated.push(fact);
            }
        }
        if let Edge::Dev { group, .. } = edge.weight()
            && let Node::Package(package) = graph[edge.source()]
            && let Some(&fact) = facts.get(&ConflictItem::from((
                package.name().clone(),
                (*group).clone(),
            )))
        {
            activated.push(fact);
        }
        activated.sort_unstable();
        activated.dedup();

        let mut bindings = FxHashMap::default();
        edge.weight().marker().visit_extras(|_, extra| {
            let scoped = match graph[edge.source()] {
                Node::Package(package) => facts
                    .get(&ConflictItem::from((package.name().clone(), extra.clone())))
                    .copied(),
                Node::Root => None,
            };
            bindings.insert(
                extra.clone(),
                MarkerBinding {
                    scoped,
                    encoded: encoded.get(extra).copied(),
                },
            );
        });
        solver.edges.push(ExportEdge {
            source,
            targets,
            activated,
            bindings,
            inputs: Vec::new(),
            condition: None,
        });
        solver.dependents[source].push(edge.id());
        solver.outgoing[edge.source().index()].push(edge.id());
    }

    for ((package_index, extra), extra_index) in extras {
        if let Node::Package(package) = graph[package_index]
            && let Some(&marker) =
                known_conflicts.get(&ConflictItem::from((package.name().clone(), extra.clone())))
        {
            solver.seeds[package_index.index()].push((extra_index, marker));
        }
    }
    for root in graph.node_indices().filter(|&index| {
        graph
            .edges_directed(index, Direction::Incoming)
            .next()
            .is_none()
    }) {
        for (item, &marker) in known_conflicts {
            if let Some(&fact) = facts.get(item) {
                solver.contexts[root.index()].insert(fact, marker);
            }
        }
        solver.add_reachability(root.index(), MarkerTree::TRUE);
    }
    solver.resolve_marker_bindings();
    solver.solve();
    graph
        .node_indices()
        .map(|index| {
            (
                index,
                solver.reachability[index.index()].unwrap_or(MarkerTree::FALSE),
            )
        })
        .collect()
}

/// The conflict facts that can supply an extra expression's value.
#[derive(Debug)]
struct MarkerBinding {
    scoped: Option<usize>,
    encoded: Option<usize>,
}

/// A dependency's reachability inputs and the facts it activates on its target.
#[derive(Debug)]
struct ExportEdge {
    source: usize,
    targets: Vec<usize>,
    activated: Vec<usize>,
    bindings: FxHashMap<ExtraName, MarkerBinding>,
    inputs: Vec<usize>,
    condition: Option<MarkerTree>,
}

/// An input whose consumers need to be revisited.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ExportChange {
    Reachability(usize),
    Conflict(NodeIndex, usize),
    Edge(EdgeIndex),
}

/// A sparse worklist over package reachability, local extras, and observable conflict facts.
struct ExportReachability<'graph, 'lock> {
    graph: &'graph Graph<Node<'lock>, Edge<'lock>>,
    edges: Vec<ExportEdge>,
    // An unvisited node differs from a false path: the latter can still introduce a false-valued
    // encoded fact that takes precedence over a scoped extra with the same spelling.
    reachability: Vec<Option<MarkerTree>>,
    contexts: Vec<FxHashMap<usize, MarkerTree>>,
    dependents: Vec<Vec<EdgeIndex>>,
    seeds: Vec<Vec<(usize, MarkerTree)>>,
    outgoing: Vec<Vec<EdgeIndex>>,
    queue: VecDeque<ExportChange>,
    queued: FxHashSet<ExportChange>,
}

impl ExportReachability<'_, '_> {
    /// Resolve encoded-name precedence before evaluating marker conditions.
    ///
    /// A scoped extra can have the same spelling as an encoded conflict item. Encoded facts take
    /// precedence even when false, so their eventual presence must be known before a scoped
    /// fallback can make a dependency reachable. Fact presence is structural: false paths still
    /// carry keys, and each optional node in the graph has a syntactic request or root selection.
    fn resolve_marker_bindings(&mut self) {
        let mut aliases: FxHashMap<usize, Vec<(EdgeIndex, ExtraName)>> = FxHashMap::default();
        for (index, edge) in self.edges.iter().enumerate() {
            for (name, binding) in &edge.bindings {
                if binding.scoped.is_some()
                    && let Some(encoded) = binding.encoded
                {
                    aliases
                        .entry(encoded)
                        .or_default()
                        .push((EdgeIndex::new(index), name.clone()));
                }
            }
        }
        for (fact, bindings) in aliases {
            let mut pending = self
                .contexts
                .iter()
                .enumerate()
                .filter(|(_, context)| context.contains_key(&fact))
                .map(|(index, _)| NodeIndex::new(index))
                .collect::<Vec<_>>();
            pending.extend(self.graph.edge_references().filter_map(|edge| {
                self.edges[edge.id().index()]
                    .activated
                    .binary_search(&fact)
                    .is_ok()
                    .then_some(edge.target())
            }));
            let mut present = vec![false; self.graph.node_count()];
            while let Some(index) = pending.pop() {
                if present[index.index()] {
                    continue;
                }
                present[index.index()] = true;
                pending.extend(self.graph.neighbors_directed(index, Direction::Outgoing));
            }
            for (index, name) in bindings {
                let (parent, _) = self
                    .graph
                    .edge_endpoints(index)
                    .expect("binding belongs to an export edge");
                if present[parent.index()]
                    && let Some(binding) = self.edges[index.index()].bindings.get_mut(&name)
                {
                    binding.scoped = None;
                }
            }
        }
        for edge in &mut self.edges {
            edge.inputs = edge
                .bindings
                .values()
                .flat_map(|binding| binding.scoped.into_iter().chain(binding.encoded))
                .filter(|fact| edge.activated.binary_search(fact).is_err())
                .collect();
            edge.inputs.sort_unstable();
            edge.inputs.dedup();
        }
    }

    /// Add a package-local extra node, initially unreachable.
    fn add_node(&mut self) -> usize {
        let index = self.reachability.len();
        self.reachability.push(None);
        self.dependents.push(Vec::new());
        self.seeds.push(Vec::new());
        index
    }

    /// Schedule each changed input at most once until its consumers have been visited.
    fn enqueue(&mut self, change: ExportChange) {
        if self.queued.insert(change) {
            self.queue.push_back(change);
        }
    }

    /// Merge a path's conditions into package or extra reachability.
    fn add_reachability(&mut self, index: usize, marker: MarkerTree) {
        let combined = self.reachability[index].map_or(marker, |existing| existing.or(marker));
        if self.reachability[index] != Some(combined) {
            self.reachability[index] = Some(combined);
            self.enqueue(ExportChange::Reachability(index));
        }
    }

    /// Merge a path's conditions into an explicitly referenced conflict fact.
    fn add_conflict(&mut self, index: NodeIndex, fact: usize, marker: MarkerTree) {
        let changed = match self.contexts[index.index()].entry(fact) {
            Entry::Occupied(mut existing) => {
                let combined = existing.get().or(marker);
                if combined == *existing.get() {
                    false
                } else {
                    existing.insert(combined);
                    true
                }
            }
            Entry::Vacant(entry) => {
                entry.insert(marker);
                true
            }
        };
        if changed {
            // Invalidate guards immediately, before another fact can be transported using their
            // cached conditions. The edge event combines changes to all of a guard's inputs.
            for outgoing in 0..self.outgoing[index.index()].len() {
                let edge = self.outgoing[index.index()][outgoing];
                if self.edges[edge.index()].inputs.binary_search(&fact).is_ok() {
                    self.enqueue(ExportChange::Edge(edge));
                }
            }
            self.enqueue(ExportChange::Conflict(index, fact));
        }
    }

    /// Reevaluate a dependency when its source or an explicit marker input changes.
    fn evaluate_edge(&mut self, index: EdgeIndex) {
        let (parent, child) = self
            .graph
            .edge_endpoints(index)
            .expect("queued edge belongs to the export graph");
        let edge = &self.edges[index.index()];
        let Some(source) = self.reachability[edge.source] else {
            return;
        };
        let context = &self.contexts[parent.index()];
        let value = |fact| {
            if edge.activated.binary_search(&fact).is_ok() {
                // A dependency's requested extras are active on that edge itself. The source
                // condition still comes from the parent package or its optional extra, so an
                // optional self-dependency cannot activate its own source.
                Some(MarkerTree::TRUE)
            } else {
                context.get(&fact).copied()
            }
        };
        let marker = self.graph[index]
            .marker()
            .resolve_extras(edge.bindings.iter().map(|(name, binding)| {
                let marker = binding
                    .encoded
                    .and_then(value)
                    .or_else(|| binding.scoped.and_then(value))
                    .unwrap_or(MarkerTree::FALSE);
                (name.clone(), marker)
            }));
        let condition = source.and(marker);

        // Preserve the current condition, including when a negative marker closes an edge. Facts
        // learned while it is closed must be replayed if another path later opens it again.
        // Facts are also deferred while this edge is queued, so replay them even if its condition
        // is unchanged after combining all input updates.
        let mut conflicts = context
            .iter()
            .filter(|(fact, _)| edge.activated.binary_search(fact).is_err())
            .map(|(&fact, &marker)| (fact, marker.and(condition)))
            .collect::<Vec<_>>();
        conflicts.extend(edge.activated.iter().map(|&fact| (fact, condition)));
        self.edges[index.index()].condition = Some(condition);
        for target in 0..self.edges[index.index()].targets.len() {
            self.add_reachability(self.edges[index.index()].targets[target], condition);
        }
        for (fact, marker) in conflicts {
            self.add_conflict(child, fact, marker);
        }
    }

    /// Carry one changed fact through an edge without copying the rest of its parent's context.
    fn propagate_conflict(&mut self, index: EdgeIndex, fact: usize) {
        let edge = &self.edges[index.index()];
        if edge.activated.binary_search(&fact).is_ok() {
            return;
        }
        let Some(condition) = edge.condition else {
            return;
        };
        let (parent, child) = self
            .graph
            .edge_endpoints(index)
            .expect("queued edge belongs to the export graph");
        let marker = self.contexts[parent.index()][&fact].and(condition);
        self.add_conflict(child, fact, marker);
    }

    /// Propagate changed inputs until all dependency contributions are unchanged.
    fn solve(&mut self) {
        while let Some(change) = self.queue.pop_front() {
            self.queued.remove(&change);
            match change {
                ExportChange::Reachability(index) => {
                    let marker =
                        self.reachability[index].expect("queued reachability has a marker");
                    for seed in 0..self.seeds[index].len() {
                        let (extra, condition) = self.seeds[index][seed];
                        self.add_reachability(extra, marker.and(condition));
                    }
                    for dependent in 0..self.dependents[index].len() {
                        self.enqueue(ExportChange::Edge(self.dependents[index][dependent]));
                    }
                }
                ExportChange::Conflict(index, fact) => {
                    for outgoing in 0..self.outgoing[index.index()].len() {
                        let edge_index = self.outgoing[index.index()][outgoing];
                        if !self.queued.contains(&ExportChange::Edge(edge_index)) {
                            self.propagate_conflict(edge_index, fact);
                        }
                    }
                }
                ExportChange::Edge(index) => self.evaluate_edge(index),
            }
        }
    }
}
