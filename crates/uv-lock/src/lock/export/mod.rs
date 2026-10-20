use std::collections::VecDeque;
use std::mem;

use either::Either;
use petgraph::algo::kosaraju_scc;
use petgraph::graph::{EdgeIndex, NodeIndex};
use petgraph::prelude::EdgeRef;
use petgraph::visit::IntoNodeReferences;
use petgraph::{Direction, Graph};
use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};

use uv_configuration::{
    DependencyGroupsWithDefaults, ExtrasSpecificationWithDefaults, InstallOptions,
};
use uv_normalize::{ExtraName, PackageName};
use uv_pep508::{ExtraOperator, MarkerExpression, MarkerTree, MarkerValueExtra};
use uv_pypi_types::{ConflictItem, ConflictSet, Conflicts};

use uv_resolver_types::universal_marker::encode_conflict_item;

pub use crate::lock::export::metadata::{Metadata, PythonReport};
pub(crate) use crate::lock::export::metadata::{
    MetadataNode, MetadataNodeId, MetadataNodeKind, MetadataScript, MetadataWorkspace,
    MetadataWorkspaceMember,
};
pub use crate::lock::export::pylock_toml::{PylockToml, PylockTomlError, PylockTomlErrorKind};
pub use crate::lock::export::requirements_txt::RequirementsTxtExport;
use crate::lock::{LockErrorKind, PackageIndex, Source};
use crate::{Installable, InstallableRootKind, LockError, Package};

pub mod cyclonedx_json;
mod metadata;
mod pylock_toml;
mod requirements_txt;

/// Whether to include virtual packages in the export.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VirtualPackages {
    Include,
    Exclude,
}

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
    ///
    /// Excluded virtual packages still contribute dependencies.
    fn from_lock(
        target: &impl Installable<'lock>,
        prune: &[PackageName],
        extras: &ExtrasSpecificationWithDefaults,
        groups: &DependencyGroupsWithDefaults,
        annotate: bool,
        install_options: &'lock InstallOptions,
        virtual_packages: VirtualPackages,
    ) -> Result<Self, LockError> {
        let size_guess = target.lock().packages.len();
        let mut graph = Graph::<Node<'lock>, Edge<'lock>>::with_capacity(size_guess, size_guess);
        let mut node_by_package = vec![None; size_guess];

        let mut queue: VecDeque<(PackageIndex, Option<&ExtraName>)> = VecDeque::new();
        let mut seen = FxHashSet::default();
        let mut activated_items = FxHashSet::default();

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

            if root_kind == InstallableRootKind::Production && groups.prod() {
                // Track the activated package in the list of known conflicts.
                activated_items.insert(ConflictItem::from(dist.id.name.clone()));

                let package_index = target.lock().by_id[&dist.id];

                // Add the workspace package to the graph.
                let index = *node_by_package[package_index.0]
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
                    activated_items
                        .insert(ConflictItem::from((dist.id.name.clone(), extra.clone())));
                }
            }

            // Add any development dependencies.
            for (group, dependencies) in dist
                .dependency_groups
                .iter()
                .filter(|(group, _)| target.includes_group(Some(&dist.id.name), group, groups))
            {
                // Empty selected groups still participate in conflicts.
                activated_items.insert(ConflictItem::from((dist.id.name.clone(), group.clone())));

                for dep in dependencies {
                    if prune.contains(&dep.package_id.name) {
                        continue;
                    }

                    let dep_dist = target.lock().package(dep.index);

                    // Add the dependency to the graph.
                    let dep_index = *node_by_package[dep.index.0]
                        .get_or_insert_with(|| graph.add_node(Node::Package(dep_dist)));

                    // Add an edge from the root. Development dependencies may be installed without
                    // installing the workspace package itself (which can never have markers on it
                    // anyway), so they're directly connected to the root.
                    graph.add_edge(
                        root,
                        dep_index,
                        Edge::Prod {
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
                    let dep_index = *node_by_package[package_index.0]
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

        // Traverse each package's regular and requested optional dependencies separately.
        while let Some((package_index, extra)) = queue.pop_front() {
            let index = node_by_package[package_index.0].expect("queued package has a graph node");
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

                let dep_dist = target.lock().package(dep.index);

                // Add the dependency to the graph.
                let dep_index = *node_by_package[dep.index.0]
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

        let include_package = |package: &Package| {
            let include_source = match &package.id.source {
                Source::Virtual(_) => virtual_packages == VirtualPackages::Include,
                Source::Registry(_)
                | Source::Git(..)
                | Source::Direct(..)
                | Source::Path(_)
                | Source::Directory(_)
                | Source::Editable(_) => true,
            };

            include_source
                && install_options.include_package(
                    package.as_install_target(),
                    target.project_name(),
                    target.lock().members(),
                )
        };

        // Determine the reachability of each node in the graph.
        let mut reachability = conflict_marker_reachability(
            &graph,
            root,
            &activated_items,
            target.lock().conflicts(),
            include_package,
        )?;

        // Collect all packages.
        let nodes = graph
            .node_references()
            .filter_map(|(index, node)| match node {
                Node::Root => None,
                Node::Package(package) => Some((index, package)),
            })
            .filter(|(_index, package)| include_package(package))
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
}

impl Edge<'_> {
    /// Return the [`MarkerTree`] for this edge.
    fn marker(&self) -> &MarkerTree {
        match self {
            Self::Prod { marker, .. } => marker,
            Self::Optional { marker, .. } => marker,
        }
    }

    /// Return the dependency extras activated by traversing this edge.
    fn dep_extras(&self) -> &[&ExtraName] {
        match self {
            Self::Prod { dep_extras, .. } => dep_extras,
            Self::Optional { dep_extras, .. } => dep_extras,
        }
    }
}

/// Compute package reachability before resolving global conflict activations.
///
/// Package and optional-extra reachability form a monotone graph problem while conflict
/// variables remain symbolic. Combining complete paths first preserves correlations between
/// alternative forks. Each referenced conflict item then has one activation equation, shared
/// by every branch of the dependency graph.
///
/// For example, if the only path requesting `foo[ssl]` has marker `sys_platform == 'linux'`,
/// that condition replaces references to `foo[ssl]` throughout the graph, including on siblings.
fn conflict_marker_reachability<'lock>(
    graph: &Graph<Node<'lock>, Edge<'lock>>,
    root: NodeIndex,
    known_conflicts: &FxHashSet<ConflictItem>,
    conflicts: &Conflicts,
    include_package: impl Fn(&Package) -> bool,
) -> Result<FxHashMap<NodeIndex, MarkerTree>, LockError> {
    let variables = ConflictVariables::from_graph(graph, known_conflicts, conflicts);

    // Give each requested package extra its own reachability node. For `a -> foo[ssl]`, the
    // edge reaches both `foo` and `foo[ssl]`; the optional dependencies of `foo[ssl]` leave
    // from the extra node, so reaching `foo` alone cannot activate them.
    let mut solver = SymbolicExportReachability {
        edges: Vec::with_capacity(graph.edge_count()),
        nodes: vec![ReachabilityNode::new(); graph.node_count()],
        activations: vec![MarkerTree::FALSE; variables.len()],
        queue: VecDeque::new(),
    };
    let mut extra_nodes = FxHashMap::default();
    for edge in graph.edge_references() {
        let source = match edge.weight() {
            Edge::Optional { extra, .. } => *extra_nodes
                .entry((edge.source(), *extra))
                .or_insert_with(|| solver.add_node()),
            Edge::Prod { .. } => edge.source().index(),
        };
        let mut targets = vec![edge.target().index()];
        for extra in edge.weight().dep_extras() {
            targets.push(
                *extra_nodes
                    .entry((edge.target(), *extra))
                    .or_insert_with(|| solver.add_node()),
            );
        }
        solver.edges.push(ExportEdge {
            targets,
            marker: *edge.weight().marker(),
        });
        solver.nodes[source].outgoing.push(edge.id());
    }

    // Link package-local extra nodes to their global facts, and seed the selected root items.
    for ((package_index, extra), extra_index) in extra_nodes {
        if let Node::Package(package) = graph[package_index] {
            let item = ConflictItem::from((package.name().clone(), extra.clone()));
            solver.nodes[extra_index].extra_fact = variables.by_item.get(&item).copied();
            if known_conflicts.contains(&item) {
                // Selected extras belong to unique, unconditionally reached production roots.
                solver.add_reachability(extra_index, MarkerTree::TRUE);
            }
        }
    }
    for item in known_conflicts {
        if let Some(&fact) = variables.by_item.get(item) {
            solver.activations[fact] = MarkerTree::TRUE;
        }
    }
    solver.add_reachability(root.index(), MarkerTree::TRUE);

    resolve_marker_bindings(graph, root, &mut solver, &variables);
    solver.simplify_unconditional_activations(&variables);
    solver.solve();

    // Recursive activations may remain uncertain, but only emitted packages need
    // determined reachability. Omitted packages still participate in the analysis.
    let bounds = ExportActivationBounds::new(&solver.activations, &variables).solve();
    let mut resolved = FxHashMap::default();
    let mut reachability = FxHashMap::default();
    for index in graph.node_indices() {
        let marker = solver.nodes[index.index()].marker;
        let (lower, upper) = *resolved.entry(marker).or_insert_with(|| {
            marker.resolve_extras_with_bounds(
                variables
                    .inputs(marker)
                    .into_iter()
                    .map(|fact| (variables.names[fact].clone(), bounds[fact])),
            )
        });

        // A difference matters only if it describes a possible environment.
        if let Node::Package(package) = graph[index]
            && include_package(package)
            && !upper.and(lower.negate()).is_false()
        {
            return Err(LockErrorKind::UnresolvedExportConflict {
                package: package.id.clone(),
            }
            .into());
        }
        reachability.insert(index, lower);
    }

    Ok(reachability)
}

/// Global conflict variables, indexed by both their conflict item and encoded marker name.
#[derive(Default)]
struct ConflictVariables {
    by_item: FxHashMap<ConflictItem, usize>,
    by_encoded_name: FxHashMap<ExtraName, usize>,
    names: Vec<ExtraName>,
}

impl ConflictVariables {
    fn from_graph(
        graph: &Graph<Node<'_>, Edge<'_>>,
        known_conflicts: &FxHashSet<ConflictItem>,
        conflicts: &Conflicts,
    ) -> Self {
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

        // Intern the global facts used by marker inputs. Include declared conflicts as well: their
        // encoded names can shadow literal extra names even when no path activates them.
        let conflict_items = conflicts
            .iter()
            .flat_map(ConflictSet::iter)
            .collect::<FxHashSet<_>>();
        let mut variables = Self::default();
        let mut add_fact = |item: ConflictItem| {
            let name = encode_conflict_item(&item);
            if referenced_names.contains(&name)
                || referenced_scoped.contains(&item)
                || conflict_items.contains(&item)
            {
                variables.intern(item, name);
            }
        };

        for item in &conflict_items {
            add_fact((*item).clone());
        }
        for item in known_conflicts {
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
        }

        variables
    }

    fn intern(&mut self, item: ConflictItem, name: ExtraName) {
        let index = *self.by_item.entry(item).or_insert_with(|| {
            let index = self.names.len();
            self.names.push(name.clone());
            index
        });
        self.by_encoded_name.insert(name, index);
    }

    fn len(&self) -> usize {
        self.names.len()
    }

    /// Return the distinct canonical conflict variables read by a marker.
    fn inputs(&self, marker: MarkerTree) -> Vec<usize> {
        let mut inputs = Vec::new();
        marker.visit_extras(|_, name| inputs.push(self.by_encoded_name[name]));
        inputs.sort_unstable();
        inputs.dedup();
        inputs
    }
}

/// A dependency's package/extra targets and its canonical marker.
#[derive(Debug)]
struct ExportEdge {
    targets: Vec<usize>,
    marker: MarkerTree,
}

/// Bind raw and encoded extra names before evaluating their activation conditions.
///
/// For example, on an edge out of `bar`, `extra == 'extra-3-foo-ssl'` could refer to the
/// encoded conflict `foo[ssl]` or the literal extra `bar[extra-3-foo-ssl]`. The encoded fact
/// wins when it is structurally present at the edge, even through a path with a false marker.
/// Its activation value is resolved later.
fn resolve_marker_bindings(
    graph: &Graph<Node<'_>, Edge<'_>>,
    root: NodeIndex,
    solver: &mut SymbolicExportReachability,
    variables: &ConflictVariables,
) {
    // Request lists follow edge order, so membership checks can use binary search.
    let mut requests = vec![Vec::new(); variables.len()];
    for (index, edge) in solver.edges.iter().enumerate() {
        for &target in &edge.targets {
            if let Some(fact) = solver.nodes[target].extra_fact {
                requests[fact].push(EdgeIndex::new(index));
            }
        }
    }

    let scoped_fact = |source: NodeIndex, name: &ExtraName| match graph[source] {
        Node::Package(package) => variables
            .by_item
            .get(&ConflictItem::from((package.name().clone(), name.clone())))
            .copied(),
        Node::Root => None,
    };

    // Find names that could refer to either a local extra or an encoded conflict.
    let mut aliases: FxHashMap<usize, Vec<(EdgeIndex, usize)>> = FxHashMap::default();
    for edge in graph.edge_references() {
        edge.weight().marker().visit_extras(|_, name| {
            if let Some(scoped) = scoped_fact(edge.source(), name)
                && let Some(&encoded) = variables.by_encoded_name.get(name)
            {
                aliases
                    .entry(encoded)
                    .or_default()
                    .push((edge.id(), scoped));
            }
        });
    }

    let mut overrides = FxHashMap::default();
    for (fact, bindings) in aliases {
        // Selected facts are present from the root; requested facts become present at the target.
        let mut pending = if solver.activations[fact].is_true() {
            vec![root]
        } else {
            Vec::new()
        };
        pending.extend(requests[fact].iter().map(|&edge| {
            graph
                .edge_endpoints(edge)
                .expect("request belongs to an export edge")
                .1
        }));

        let mut present = vec![false; graph.node_count()];
        while let Some(index) = pending.pop() {
            if present[index.index()] {
                continue;
            }
            present[index.index()] = true;
            pending.extend(graph.neighbors_directed(index, Direction::Outgoing));
        }

        for (index, scoped) in bindings {
            let (parent, _) = graph
                .edge_endpoints(index)
                .expect("binding belongs to an export edge");
            if !present[parent.index()] && requests[fact].binary_search(&index).is_err() {
                overrides.insert((index, fact), scoped);
            }
        }
    }

    // Replace each extra name with its bound fact. An edge's own request is already active
    // when evaluating that edge's marker.
    for edge in graph.edge_references() {
        let mut replacements = Vec::new();
        edge.weight().marker().visit_extras(|_, name| {
            let fact = variables
                .by_encoded_name
                .get(name)
                .map(|&fact| overrides.get(&(edge.id(), fact)).copied().unwrap_or(fact))
                .or_else(|| scoped_fact(edge.source(), name));
            let marker = fact.map_or(MarkerTree::FALSE, |fact| {
                if requests[fact].binary_search(&edge.id()).is_ok() {
                    MarkerTree::TRUE
                } else {
                    conflict_variable(&variables.names[fact])
                }
            });
            replacements.push((name.clone(), marker));
        });
        solver.edges[edge.id().index()].marker =
            edge.weight().marker().resolve_extras(replacements);
    }
}

/// Least package and optional-extra reachability with conflict variables held symbolic.
///
/// Original graph nodes keep their indices, and optional-extra nodes are appended. Edge indices
/// also match the original graph. `activations` is indexed by global fact rather than node.
struct SymbolicExportReachability {
    edges: Vec<ExportEdge>,
    nodes: Vec<ReachabilityNode>,
    activations: Vec<MarkerTree>,
    queue: VecDeque<usize>,
}

/// Reachability and worklist state for one package or optional-extra node.
#[derive(Clone)]
struct ReachabilityNode {
    marker: MarkerTree,
    outgoing: Vec<EdgeIndex>,
    extra_fact: Option<usize>,
    queued: bool,
}

impl ReachabilityNode {
    /// Create an unreachable node with no outgoing edges or global activation.
    fn new() -> Self {
        Self {
            marker: MarkerTree::FALSE,
            outgoing: Vec::new(),
            extra_fact: None,
            queued: false,
        }
    }
}

impl SymbolicExportReachability {
    /// Add an optional-extra node that can be reached only by an actual request.
    fn add_node(&mut self) -> usize {
        let index = self.nodes.len();
        self.nodes.push(ReachabilityNode::new());
        index
    }

    /// Merge complete symbolic paths and schedule only changed sources.
    fn add_reachability(&mut self, index: usize, marker: MarkerTree) {
        let node = &mut self.nodes[index];
        let combined = node.marker.or(marker);
        if combined != node.marker {
            node.marker = combined;
            if !mem::replace(&mut node.queued, true) {
                self.queue.push_back(index);
            }
        }
    }

    /// Fold activations proved by unconditional request paths before building symbolic paths.
    ///
    /// Only constant-true guards and seeds establish these facts, independently of any
    /// assumed activation. All unproved activations remain symbolic.
    fn simplify_unconditional_activations(&mut self, variables: &ConflictVariables) {
        if self.activations.is_empty() {
            return;
        }

        let mut active = self
            .activations
            .iter()
            .map(|marker| marker.is_true())
            .collect::<Vec<_>>();

        let mut reached = vec![false; self.nodes.len()];
        let mut pending = self.queue.iter().copied().collect::<Vec<_>>();
        while let Some(source) = pending.pop() {
            if mem::replace(&mut reached[source], true) {
                continue;
            }
            if let Some(fact) = self.nodes[source].extra_fact {
                active[fact] = true;
            }
            for &edge in &self.nodes[source].outgoing {
                let edge = &self.edges[edge.index()];
                if !edge.marker.is_true() {
                    continue;
                }
                pending.extend(edge.targets.iter().copied());
            }
        }

        if active.iter().any(|&active| active) {
            for edge in &mut self.edges {
                edge.marker = edge
                    .marker
                    .simplify_extras_with(|name| active[variables.by_encoded_name[name]]);
            }
        }
    }

    /// Close package/extra reachability, then derive each global activation from its local nodes.
    fn solve(&mut self) {
        while let Some(source) = self.queue.pop_front() {
            self.nodes[source].queued = false;
            let marker = self.nodes[source].marker;
            for outgoing in 0..self.nodes[source].outgoing.len() {
                let edge = self.nodes[source].outgoing[outgoing].index();
                let condition = marker.and(self.edges[edge].marker);
                for target in 0..self.edges[edge].targets.len() {
                    self.add_reachability(self.edges[edge].targets[target], condition);
                }
            }
        }

        for node in &self.nodes {
            if let Some(fact) = node.extra_fact {
                self.activations[fact] = self.activations[fact].or(node.marker);
            }
        }
    }
}

/// The canonical Boolean variable for one global conflict item.
fn conflict_variable(name: &ExtraName) -> MarkerTree {
    MarkerTree::expression(MarkerExpression::Extra {
        operator: ExtraOperator::Equal,
        name: MarkerValueExtra::Extra(name.clone()),
    })
}

/// Which side of an activation interval is being computed with the other side fixed.
#[derive(Clone, Copy)]
enum ExportBound {
    Lower,
    Upper,
}

/// Solve global activation equations in dependency order, iterating only recursive components.
///
/// Under founded evaluation, a positive cycle cannot activate itself. Each `(lower, upper)` pair
/// describes environments where an item is definitely or possibly active under that rule. For
/// example, `a = a` has no external support, so both bounds are false. Mutually negative
/// equations such as `a = !b; b = !a` can remain uncertain.
struct ExportActivationBounds<'a> {
    /// Activation condition for each global fact; it may refer to itself or other facts.
    conditions: &'a [MarkerTree],
    variables: &'a ConflictVariables,
    inputs: Vec<Vec<usize>>,
    dependents: Vec<Vec<usize>>,
    component_by_fact: Vec<usize>,
    queued: Vec<bool>,
    bounds: Vec<(MarkerTree, MarkerTree)>,
}

impl<'a> ExportActivationBounds<'a> {
    /// Index each equation by the conflict facts whose values can change it.
    fn new(conditions: &'a [MarkerTree], variables: &'a ConflictVariables) -> Self {
        let inputs = conditions
            .iter()
            .map(|&marker| variables.inputs(marker))
            .collect::<Vec<_>>();
        let mut dependents = vec![Vec::new(); conditions.len()];
        for (fact, inputs) in inputs.iter().enumerate() {
            for &input in inputs {
                dependents[input].push(fact);
            }
        }

        Self {
            conditions,
            variables,
            inputs,
            dependents,
            component_by_fact: vec![0; conditions.len()],
            queued: vec![false; conditions.len()],
            bounds: vec![(MarkerTree::FALSE, MarkerTree::FALSE); conditions.len()],
        }
    }

    /// Compute founded bounds while retaining uncertainty from predecessor components.
    fn solve(mut self) -> Vec<(MarkerTree, MarkerTree)> {
        let mut graph = Graph::<(), ()>::with_capacity(self.conditions.len(), 0);
        for _ in self.conditions {
            graph.add_node(());
        }

        for (fact, inputs) in self.inputs.iter().enumerate() {
            for &input in inputs {
                // SCC postorder then visits an equation's dependencies before its readers.
                graph.add_edge(NodeIndex::new(fact), NodeIndex::new(input), ());
            }
        }

        let components = kosaraju_scc(&graph);
        for (component_index, component) in components.iter().enumerate() {
            for fact in component {
                self.component_by_fact[fact.index()] = component_index;
            }
        }

        for (component_index, component) in components.iter().enumerate() {
            let component = component
                .iter()
                .map(|fact| fact.index())
                .collect::<Vec<_>>();

            self.solve_component(&component, component_index);
        }

        self.bounds
    }

    /// Solve one component using bounds already established for its dependencies.
    fn solve_component(&mut self, component: &[usize], component_index: usize) {
        // A nonrecursive equation can be evaluated once after its inputs have been solved.
        if let [fact] = component
            && self.inputs[*fact].binary_search(fact).is_err()
        {
            self.bounds[*fact] = self.evaluate(*fact);
            return;
        }

        // Negative inputs use the fixed opposite bound, so each inner worklist grows
        // monotonically. Restart upper passes from the new lower bound to discard
        // support from paths whose negative conditions have closed.
        self.solve_bound(component, component_index, ExportBound::Upper);
        loop {
            let previous_upper = component
                .iter()
                .map(|&fact| self.bounds[fact].1)
                .collect::<Vec<_>>();
            for &fact in component {
                self.bounds[fact].0 = MarkerTree::FALSE;
            }
            self.solve_bound(component, component_index, ExportBound::Lower);

            for &fact in component {
                self.bounds[fact].1 = self.bounds[fact].0;
            }
            self.solve_bound(component, component_index, ExportBound::Upper);

            // With an unchanged upper bound, another lower pass would produce the same result.
            if component
                .iter()
                .zip(previous_upper)
                .all(|(&fact, previous)| self.bounds[fact].1 == previous)
            {
                break;
            }
        }
    }

    /// Evaluate a complete activation equation over the current global intervals.
    fn evaluate(&self, fact: usize) -> (MarkerTree, MarkerTree) {
        self.conditions[fact].resolve_extras_with_bounds(
            self.inputs[fact]
                .iter()
                .map(|&input| (self.variables.names[input].clone(), self.bounds[input])),
        )
    }

    /// Compute one least fixed point without revisiting unrelated components.
    fn solve_bound(&mut self, component: &[usize], component_index: usize, bound: ExportBound) {
        let mut queue = component.iter().copied().collect::<VecDeque<_>>();
        for &fact in component {
            self.queued[fact] = true;
        }

        while let Some(fact) = queue.pop_front() {
            self.queued[fact] = false;
            let (lower, upper) = self.evaluate(fact);
            let (current, next) = match bound {
                ExportBound::Lower => (&mut self.bounds[fact].0, lower),
                ExportBound::Upper => (&mut self.bounds[fact].1, upper),
            };
            debug_assert!(current.and(next.negate()).is_false());
            if *current == next {
                continue;
            }
            *current = next;

            for &dependent in &self.dependents[fact] {
                if self.component_by_fact[dependent] == component_index
                    && !mem::replace(&mut self.queued[dependent], true)
                {
                    queue.push_back(dependent);
                }
            }
        }
    }
}
