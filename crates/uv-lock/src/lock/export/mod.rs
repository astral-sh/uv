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
use uv_normalize::{ExtraName, GroupName, PackageName};
use uv_pep508::{ExtraOperator, MarkerExpression, MarkerTree, MarkerValueExtra};
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

        let include_package = |package: &Package| {
            install_options.include_package(
                package.as_install_target(),
                target.project_name(),
                target.lock().members(),
            )
        };

        // Determine the reachability of each node in the graph.
        let mut reachability =
            conflict_marker_reachability(&graph, &activated_items, include_package)?;

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

/// Compute package reachability before resolving global conflict activations.
///
/// Package and optional-extra reachability form a monotone graph problem while conflict
/// variables remain symbolic. Combining complete paths first preserves correlations between
/// alternative forks. Each referenced conflict item then has one activation equation, shared
/// by every branch of the dependency graph.
fn conflict_marker_reachability<'lock>(
    graph: &Graph<Node<'lock>, Edge<'lock>>,
    known_conflicts: &FxHashMap<ConflictItem, MarkerTree>,
    include_package: impl Fn(&Package) -> bool,
) -> Result<FxHashMap<NodeIndex, MarkerTree>, LockError> {
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

    // Ordinary extras only need package-local reachability nodes. Introduce a global
    // variable only when an explicit marker can observe its activation.
    let mut facts = FxHashMap::default();
    let mut encoded = FxHashMap::default();
    let mut fact_names = Vec::new();
    let mut add_fact = |item: ConflictItem| {
        let name = encode_conflict_item(&item);
        if referenced_names.contains(&name) || referenced_scoped.contains(&item) {
            let index = *facts.entry(item).or_insert_with(|| {
                let index = fact_names.len();
                fact_names.push(name.clone());
                index
            });
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

    let mut solver = SymbolicExportReachability {
        edges: Vec::with_capacity(graph.edge_count()),
        reachability: vec![MarkerTree::FALSE; graph.node_count()],
        activations: vec![MarkerTree::FALSE; facts.len()],
        dependents: vec![Vec::new(); graph.node_count()],
        seeds: vec![Vec::new(); graph.node_count()],
        queue: VecDeque::new(),
        queued: vec![false; graph.node_count()],
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
            targets,
            activated,
            bindings,
            marker: *edge.weight().marker(),
        });
        solver.dependents[source].push(edge.id());
    }

    for ((package_index, extra), extra_index) in extras {
        if let Node::Package(package) = graph[package_index]
            && let Some(&marker) =
                known_conflicts.get(&ConflictItem::from((package.name().clone(), extra.clone())))
        {
            solver.seeds[package_index.index()].push((extra_index, marker));
        }
    }
    let roots = graph
        .node_indices()
        .filter(|&index| {
            graph
                .edges_directed(index, Direction::Incoming)
                .next()
                .is_none()
        })
        .collect::<Vec<_>>();
    let mut known_facts = FxHashSet::default();
    for (item, &marker) in known_conflicts {
        if let Some(&fact) = facts.get(item) {
            solver.activations[fact] = marker;
            known_facts.insert(fact);
        }
    }
    for &root in &roots {
        solver.add_reachability(root.index(), MarkerTree::TRUE);
    }
    resolve_marker_bindings(graph, &mut solver.edges, &fact_names, &known_facts, &roots);
    solver.solve();

    let bounds = ExportActivationBounds::new(&solver.activations, &fact_names, &encoded).solve();
    let mut resolved = FxHashMap::default();
    let mut reachability = FxHashMap::default();
    for index in graph.node_indices() {
        let marker = solver.reachability[index.index()];
        let (lower, upper) = *resolved.entry(marker).or_insert_with(|| {
            marker.resolve_extras_with_bounds(
                marker_inputs(marker, &encoded)
                    .into_iter()
                    .map(|fact| (fact_names[fact].clone(), bounds[fact])),
            )
        });
        if lower != upper
            && let Node::Package(package) = graph[index]
            && include_package(package)
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

/// The conflict facts that can supply an extra expression's value.
#[derive(Debug)]
struct MarkerBinding {
    scoped: Option<usize>,
    encoded: Option<usize>,
}

/// A dependency's package/extra targets and the conflict facts that it requests.
#[derive(Debug)]
struct ExportEdge {
    targets: Vec<usize>,
    activated: Vec<usize>,
    bindings: FxHashMap<ExtraName, MarkerBinding>,
    marker: MarkerTree,
}

/// Bind raw and encoded extra names before evaluating their activation conditions.
///
/// An encoded fact takes precedence over a scoped literal of the same spelling when
/// that fact can reach the marker's context. Presence is structural, including false
/// paths; its value is subsequently resolved from the global activation equation.
fn resolve_marker_bindings(
    graph: &Graph<Node<'_>, Edge<'_>>,
    edges: &mut [ExportEdge],
    fact_names: &[ExtraName],
    known_facts: &FxHashSet<usize>,
    roots: &[NodeIndex],
) {
    let mut aliases: FxHashMap<usize, Vec<(EdgeIndex, ExtraName)>> = FxHashMap::default();
    for (index, edge) in edges.iter().enumerate() {
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
        let mut pending = if known_facts.contains(&fact) {
            roots.to_vec()
        } else {
            Vec::new()
        };
        pending.extend(graph.edge_references().filter_map(|edge| {
            edges[edge.id().index()]
                .activated
                .binary_search(&fact)
                .is_ok()
                .then_some(edge.target())
        }));
        let mut present = vec![false; graph.node_count()];
        while let Some(index) = pending.pop() {
            if present[index.index()] {
                continue;
            }
            present[index.index()] = true;
            pending.extend(graph.neighbors_directed(index, Direction::Outgoing));
        }
        for (index, name) in bindings {
            let (parent, _) = graph
                .edge_endpoints(index)
                .expect("binding belongs to an export edge");
            let edge = &mut edges[index.index()];
            if let Some(binding) = edge.bindings.get_mut(&name) {
                if present[parent.index()] || edge.activated.binary_search(&fact).is_ok() {
                    binding.scoped = None;
                } else {
                    binding.encoded = None;
                }
            }
        }
    }
    for edge in edges {
        edge.marker =
            edge.marker
                .resolve_extras(mem::take(&mut edge.bindings).into_iter().map(
                    |(name, binding)| {
                        let marker =
                            binding
                                .encoded
                                .or(binding.scoped)
                                .map_or(MarkerTree::FALSE, |fact| {
                                    if edge.activated.binary_search(&fact).is_ok() {
                                        MarkerTree::TRUE
                                    } else {
                                        MarkerTree::expression(MarkerExpression::Extra {
                                            operator: ExtraOperator::Equal,
                                            name: MarkerValueExtra::Extra(fact_names[fact].clone()),
                                        })
                                    }
                                });
                        (name, marker)
                    },
                ));
    }
}

/// Least package and optional-extra reachability with conflict variables held symbolic.
struct SymbolicExportReachability {
    edges: Vec<ExportEdge>,
    reachability: Vec<MarkerTree>,
    activations: Vec<MarkerTree>,
    dependents: Vec<Vec<EdgeIndex>>,
    seeds: Vec<Vec<(usize, MarkerTree)>>,
    queue: VecDeque<usize>,
    queued: Vec<bool>,
}

impl SymbolicExportReachability {
    /// Add an optional-extra node that can be reached only by an actual request.
    fn add_node(&mut self) -> usize {
        let index = self.reachability.len();
        self.reachability.push(MarkerTree::FALSE);
        self.dependents.push(Vec::new());
        self.seeds.push(Vec::new());
        self.queued.push(false);
        index
    }

    /// Merge complete symbolic paths and schedule only changed sources.
    fn add_reachability(&mut self, index: usize, marker: MarkerTree) {
        let combined = self.reachability[index].or(marker);
        if combined != self.reachability[index] {
            self.reachability[index] = combined;
            if !mem::replace(&mut self.queued[index], true) {
                self.queue.push_back(index);
            }
        }
    }

    /// Close the positive reachability graph and collect each global activation equation.
    fn solve(&mut self) {
        while let Some(source) = self.queue.pop_front() {
            self.queued[source] = false;
            let marker = self.reachability[source];
            for seed in 0..self.seeds[source].len() {
                let (extra, condition) = self.seeds[source][seed];
                self.add_reachability(extra, marker.and(condition));
            }
            for dependent in 0..self.dependents[source].len() {
                let edge = self.dependents[source][dependent].index();
                let condition = marker.and(self.edges[edge].marker);
                for target in 0..self.edges[edge].targets.len() {
                    self.add_reachability(self.edges[edge].targets[target], condition);
                }
                for &fact in &self.edges[edge].activated {
                    self.activations[fact] = self.activations[fact].or(condition);
                }
            }
        }
    }
}

/// Return the distinct canonical conflict variables read by a marker.
fn marker_inputs(marker: MarkerTree, encoded: &FxHashMap<ExtraName, usize>) -> Vec<usize> {
    let mut inputs = Vec::new();
    marker.visit_extras(|_, name| inputs.push(encoded[name]));
    inputs.sort_unstable();
    inputs.dedup();
    inputs
}

/// Which side of an activation interval is being computed with the other side fixed.
#[derive(Clone, Copy)]
enum ExportBound {
    Lower,
    Upper,
}

/// Solve global activation equations in dependency order, iterating only recursive components.
struct ExportActivationBounds<'a> {
    conditions: &'a [MarkerTree],
    names: &'a [ExtraName],
    inputs: Vec<Vec<usize>>,
    dependents: Vec<Vec<usize>>,
    owners: Vec<usize>,
    queued: Vec<bool>,
    bounds: Vec<(MarkerTree, MarkerTree)>,
}

impl<'a> ExportActivationBounds<'a> {
    /// Index each equation by the conflict facts whose values can change it.
    fn new(
        conditions: &'a [MarkerTree],
        names: &'a [ExtraName],
        encoded: &FxHashMap<ExtraName, usize>,
    ) -> Self {
        let inputs = conditions
            .iter()
            .map(|&marker| marker_inputs(marker, encoded))
            .collect::<Vec<_>>();
        let mut dependents = vec![Vec::new(); conditions.len()];
        for (fact, inputs) in inputs.iter().enumerate() {
            for &input in inputs {
                dependents[input].push(fact);
            }
        }
        Self {
            conditions,
            names,
            inputs,
            dependents,
            owners: vec![0; conditions.len()],
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
        for (owner, component) in components.iter().enumerate() {
            for fact in component {
                self.owners[fact.index()] = owner;
            }
        }
        for (owner, component) in components.iter().enumerate() {
            let component = component
                .iter()
                .map(|fact| fact.index())
                .collect::<Vec<_>>();
            if let [fact] = component.as_slice()
                && self.inputs[*fact].binary_search(fact).is_err()
            {
                self.bounds[*fact] = self.evaluate(*fact);
                continue;
            }

            // Negative inputs use the fixed opposite bound, so each inner worklist grows
            // monotonically. Restart upper passes from the new lower bound to discard
            // support from paths whose negative conditions have closed.
            self.solve_bound(&component, owner, ExportBound::Upper);
            loop {
                let previous_upper = component
                    .iter()
                    .map(|&fact| self.bounds[fact].1)
                    .collect::<Vec<_>>();
                for &fact in &component {
                    self.bounds[fact].0 = MarkerTree::FALSE;
                }
                self.solve_bound(&component, owner, ExportBound::Lower);
                for &fact in &component {
                    self.bounds[fact].1 = self.bounds[fact].0;
                }
                self.solve_bound(&component, owner, ExportBound::Upper);
                if component
                    .iter()
                    .zip(previous_upper)
                    .all(|(&fact, previous)| self.bounds[fact].1 == previous)
                {
                    break;
                }
            }
        }
        self.bounds
    }

    /// Evaluate a complete activation equation over the current global intervals.
    fn evaluate(&self, fact: usize) -> (MarkerTree, MarkerTree) {
        self.conditions[fact].resolve_extras_with_bounds(
            self.inputs[fact]
                .iter()
                .map(|&input| (self.names[input].clone(), self.bounds[input])),
        )
    }

    /// Compute one least fixed point without revisiting unrelated components.
    fn solve_bound(&mut self, component: &[usize], owner: usize, bound: ExportBound) {
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
                if self.owners[dependent] == owner
                    && !mem::replace(&mut self.queued[dependent], true)
                {
                    queue.push_back(dependent);
                }
            }
        }
    }
}
