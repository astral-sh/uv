//! Select locked packages from requests that survive installation-marker simplification.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use petgraph::graph::NodeIndex;
use petgraph::visit::EdgeRef;
use uv_configuration::{
    DependencyGroupsWithDefaults, DependencyModifierScope, DependencyModifiers, Excludes,
    ExtrasSpecificationWithDefaults, Overrides,
};
use uv_distribution_types::{Edge, Requirement};
use uv_normalize::{ExtraName, GroupName, PackageName};
use uv_pep508::{MarkerEnvironment, MarkerTree, marker};
use uv_pypi_types::{ConflictItem, ConflictKind, ConflictSet, Conflicts};
use uv_resolver_types::selection::{
    ActivationGraph, ActivationNode, SelectionError, ValidatedSelection,
};
use uv_resolver_types::{AnnotatedDist, UniversalMarker};

use super::{
    DependencyContext, DependencySelectionContext, Installable, InstallableRootKind, Lock,
    LockError, LockErrorKind, Package, PackageIndex, implicit_constraints_marker,
};

/// A dependency declaration after overrides and exclusions have been applied.
///
/// The marker uses the declaring package's ordinary extra names. Version and source choices
/// remain on the resolved dependency edges; these requests must survive even when every edge
/// selecting an extra is impossible under the declared conflicts.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, serde::Deserialize, serde::Serialize)]
pub(super) struct DependencyRequest {
    name: PackageName,
    #[serde(default, skip_serializing_if = "<[ExtraName]>::is_empty")]
    extras: Box<[ExtraName]>,
    #[serde(default, skip_serializing_if = "<[GroupName]>::is_empty")]
    groups: Box<[GroupName]>,
    #[serde(
        default,
        skip_serializing_if = "marker::ser::is_empty",
        serialize_with = "marker::ser::serialize"
    )]
    marker: MarkerTree,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    group: Option<GroupName>,
}

impl DependencyRequest {
    /// Require a group-scoped request to refer to a recorded dependency section.
    pub(super) fn validate(&self, package: &Package) -> Result<(), LockError> {
        if let Some(group) = &self.group
            && !package.dependency_groups.contains_key(group)
        {
            return Err(LockErrorKind::InvalidSelection {
                reason: format!(
                    "The lockfile records requests for unknown group `{}:{group}`",
                    package.id.name
                ),
            }
            .into());
        }
        Ok(())
    }

    /// Omit declarations that cannot apply to any dependency section recorded in this lock.
    pub(super) fn is_reachable(
        &self,
        extras: &BTreeMap<ExtraName, Vec<super::Dependency>>,
        environment: MarkerTree,
    ) -> bool {
        !DependencyContext::Production
            .requirement_marker(self.marker)
            .and(environment)
            .is_false()
            || self.group.is_none()
                && extras.keys().any(|extra| {
                    !DependencyContext::Extra(extra)
                        .requirement_marker(self.marker)
                        .and(environment)
                        .is_false()
                })
    }
}

impl Package {
    /// Retain request guards before the resolved edges are restricted by conflicts.
    pub(super) fn record_requests(
        &mut self,
        distribution: &AnnotatedDist,
        modifiers: &DependencyModifiers,
    ) -> Result<(), LockError> {
        let Some(metadata) = distribution.metadata() else {
            return Err(LockErrorKind::MissingRequests {
                package: self.id.clone(),
            }
            .into());
        };
        let mut requests = BTreeSet::new();
        for (group, requirements) in std::iter::once((None, &metadata.requires_dist)).chain(
            metadata
                .dependency_groups
                .iter()
                .map(|(group, requirements)| (Some(group), requirements)),
        ) {
            let scope = if group.is_some() {
                DependencyModifierScope::DependencyGroup(&distribution.name, &distribution.version)
            } else {
                DependencyModifierScope::Package(&distribution.name, &distribution.version)
            };
            for requirement in modifiers.apply(scope, requirements) {
                requests.insert(DependencyRequest {
                    name: requirement.name.clone(),
                    extras: requirement.extras.clone(),
                    groups: requirement.groups.clone(),
                    marker: requirement.marker,
                    group: group.cloned(),
                });
            }
        }
        if !self.id.source.is_immutable() {
            for extra in &metadata.provides_extra {
                self.optional_dependencies.entry(extra.clone()).or_default();
            }
        }
        for group in metadata.dependency_groups.keys() {
            self.dependency_groups.entry(group.clone()).or_default();
        }
        self.requests = Some(requests);
        Ok(())
    }
}

/// Selected packages and edges, with environment conditions for each package.
pub(super) struct Selection {
    pub(super) packages: Vec<MarkerTree>,
    pub(super) edges: Vec<SelectedEdge>,
}

pub(super) struct SelectedEdge {
    pub(super) parent: Option<PackageIndex>,
    pub(super) target: PackageIndex,
    pub(super) kind: Edge,
}

/// Whether the consumer needs one installable solution or an inventory of possible dependencies.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum SelectionMode {
    Resolution,
    Inventory,
}

struct PackageNodes<'lock> {
    production: NodeIndex,
    extras: BTreeMap<&'lock ExtraName, NodeIndex>,
    groups: BTreeMap<&'lock GroupName, NodeIndex>,
}

struct SelectionGraph<'lock> {
    lock: &'lock Lock,
    graph: ActivationGraph,
    packages: Vec<PackageNodes<'lock>>,
    choices: Vec<(NodeIndex, SelectedEdge)>,
    environment: Option<&'lock MarkerEnvironment>,
}

impl Selection {
    /// Validate the selected roots and materialize their environment-specific reachability once.
    pub(super) fn from_lock<'lock>(
        target: &(impl Installable<'lock> + ?Sized),
        roots: &[(&Package, InstallableRootKind)],
        include_manifest: bool,
        context: DependencySelectionContext<'_>,
        extras: &ExtrasSpecificationWithDefaults,
        groups: &DependencyGroupsWithDefaults,
        environment: Option<&MarkerEnvironment>,
        prune: &[PackageName],
        mode: SelectionMode,
    ) -> Result<Self, LockError> {
        let lock = target.lock();
        let mut selection = SelectionGraph {
            lock,
            graph: ActivationGraph::default(),
            packages: Vec::with_capacity(lock.packages.len()),
            choices: Vec::new(),
            environment,
        };
        for package in &lock.packages {
            let production = selection.graph.graph.add_node(ActivationNode::default());
            let mut nodes = PackageNodes {
                production,
                extras: BTreeMap::new(),
                groups: BTreeMap::new(),
            };
            for extra in package.optional_dependencies.keys() {
                let node = selection.graph.graph.add_node(ActivationNode {
                    requests: vec![ConflictItem::from((package.id.name.clone(), extra.clone()))],
                    requires_choice: false,
                });
                selection
                    .graph
                    .graph
                    .add_edge(node, production, MarkerTree::TRUE);
                nodes.extras.insert(extra, node);
            }
            for group in package.dependency_groups.keys() {
                let node = selection.graph.graph.add_node(ActivationNode {
                    requests: vec![ConflictItem::from((package.id.name.clone(), group.clone()))],
                    requires_choice: false,
                });
                nodes.groups.insert(group, node);
            }
            selection.packages.push(nodes);
        }

        for (index, package) in lock.packages.iter().enumerate() {
            if prune.contains(&package.id.name) {
                continue;
            }
            let Some(requests) = &package.requests else {
                return Err(LockErrorKind::MissingRequests {
                    package: package.id.clone(),
                }
                .into());
            };
            for request in requests {
                if prune.contains(&request.name) {
                    continue;
                }
                let contexts = if let Some(group) = &request.group {
                    vec![DependencyContext::Group(group)]
                } else {
                    // An unmentioned extra contributes only production requirements, which are
                    // already attached to the base node. Avoid comparing every request to every
                    // extra on packages with many optional dependencies.
                    let mut extras = BTreeSet::new();
                    request.marker.visit_extras(|_, extra| {
                        if let Some((extra, _)) = package.optional_dependencies.get_key_value(extra)
                        {
                            extras.insert(extra);
                        }
                    });
                    std::iter::once(DependencyContext::Production)
                        .chain(extras.into_iter().map(DependencyContext::Extra))
                        .collect()
                };
                for context in contexts {
                    let marker = selection.specialize(context.requirement_marker(request.marker));
                    if marker.is_false() {
                        continue;
                    }
                    let parent = match context {
                        DependencyContext::Production => selection.packages[index].production,
                        DependencyContext::Extra(extra) => selection.packages[index].extras[extra],
                        DependencyContext::Group(group) => selection.packages[index].groups[group],
                    };
                    let node = selection.request(
                        parent,
                        &request.name,
                        &request.extras,
                        &request.groups,
                        marker,
                    );
                    let mut candidates = BTreeMap::new();
                    for dependency in context.dependencies(package) {
                        if dependency.package_id.name == request.name {
                            let guard = candidates
                                .entry(dependency.index.0)
                                .or_insert(MarkerTree::FALSE);
                            *guard =
                                guard
                                    .or(selection
                                        .specialize(dependency.complexified_marker.combined()));
                        }
                    }
                    // Recursive extras are flattened during resolution, so their request has no
                    // installation edge. They still activate the selected package's extra section.
                    if request.name == package.id.name {
                        candidates.insert(index, MarkerTree::TRUE);
                    }
                    for (candidate, guard) in candidates {
                        let kind = match context {
                            DependencyContext::Production => Edge::Prod,
                            DependencyContext::Extra(extra) => Edge::Optional(extra.clone()),
                            DependencyContext::Group(group) => Edge::Dev(group.clone()),
                        };
                        selection.choice(
                            node,
                            Some(PackageIndex(index)),
                            PackageIndex(candidate),
                            &request.extras,
                            &request.groups,
                            guard,
                            kind,
                        );
                    }
                }
            }
        }

        let root = selection.graph.graph.add_node(ActivationNode::default());
        match context {
            DependencySelectionContext::None => {}
            DependencySelectionContext::Production(package) => selection.graph.graph[root]
                .requests
                .push(ConflictItem::from(package.clone())),
            DependencySelectionContext::Group(package, group) => selection.graph.graph[root]
                .requests
                .push(ConflictItem::from((package.clone(), group.clone()))),
        }
        for (package, kind) in roots {
            if prune.contains(&package.id.name) {
                continue;
            }
            let index = lock.by_id[&package.id];
            let nodes = &selection.packages[index.0];
            if *kind == InstallableRootKind::Production && groups.prod() {
                selection.graph.graph[root]
                    .requests
                    .push(ConflictItem::from(package.id.name.clone()));
                selection
                    .graph
                    .graph
                    .add_edge(root, nodes.production, MarkerTree::TRUE);
                for extra in extras.extra_names(package.optional_dependencies.keys()) {
                    selection
                        .graph
                        .graph
                        .add_edge(root, nodes.extras[extra], MarkerTree::TRUE);
                }
            }
            for (group, node) in &nodes.groups {
                if target.includes_group(Some(&package.id.name), group, groups) {
                    selection
                        .graph
                        .graph
                        .add_edge(root, *node, MarkerTree::TRUE);
                }
            }
        }
        if include_manifest {
            let modifiers = DependencyModifiers::new(
                Overrides::from_entries(lock.manifest.overrides.iter().cloned().collect())
                    .map_err(LockErrorKind::InvalidScopedOverride)?,
                Excludes::from_entries(lock.manifest.excludes.iter().cloned()),
            );
            for (group, requirements) in std::iter::once((None, &lock.manifest.requirements)).chain(
                lock.manifest
                    .dependency_groups
                    .iter()
                    .filter(|(group, _)| target.includes_group(None, group, groups))
                    .map(|(group, requirements)| (Some(group), requirements)),
            ) {
                for requirement in modifiers.apply(DependencyModifierScope::Global, requirements) {
                    if prune.contains(&requirement.name) {
                        continue;
                    }
                    selection.root_requirement(root, &requirement, group, target.install_path())?;
                }
            }
        }

        let environment = selection.specialize(implicit_constraints_marker(
            lock.requires_python.to_marker_tree(),
            &lock.supported_environments,
        ));
        let selected =
            match selection
                .graph
                .select([(root, MarkerTree::TRUE)], environment, &lock.conflicts)
            {
                Ok(selected) => selected,
                Err(error @ SelectionError::UnknownConflictItem(_)) => {
                    return Err(selection_error(error));
                }
                Err(
                    error @ (SelectionError::Conflict { .. }
                    | SelectionError::Unsatisfiable(_)
                    | SelectionError::Ambiguous(_)),
                ) => {
                    if mode == SelectionMode::Resolution {
                        return Err(selection_error(error));
                    }
                    // An SBOM can describe incompatible selections. If there is no single solution,
                    // include every possible target while retaining the environment conditions.
                    for marker in selection.graph.graph.edge_weights_mut() {
                        *marker = marker.without_extras();
                    }
                    for node in selection.graph.graph.node_weights_mut() {
                        node.requires_choice = false;
                    }
                    selection
                        .graph
                        .select([(root, MarkerTree::TRUE)], environment, &Conflicts::empty())
                        .map_err(selection_error)?
                }
            };
        if !include_manifest {
            selection.validate_subgraph(&selected, roots, context)?;
        }
        Ok(Self {
            packages: selection
                .packages
                .iter()
                .map(|nodes| selected.marker(nodes.production))
                .collect(),
            edges: selection
                .choices
                .into_iter()
                .filter_map(|(node, edge)| (!selected.marker(node).is_false()).then_some(edge))
                .collect(),
        })
    }
}

impl SelectionGraph<'_> {
    /// Resolve environment variables before solving activation-dependent guards.
    fn specialize(&self, marker: MarkerTree) -> MarkerTree {
        self.environment.map_or(marker, |environment| {
            marker.simplify_environment(environment)
        })
    }

    /// Record requested items before any target version is chosen.
    fn request(
        &mut self,
        parent: NodeIndex,
        name: &PackageName,
        extras: &[ExtraName],
        groups: &[GroupName],
        marker: MarkerTree,
    ) -> NodeIndex {
        let node = self.graph.graph.add_node(ActivationNode {
            requests: extras
                .iter()
                .map(|extra| ConflictItem::from((name.clone(), extra.clone())))
                .chain(
                    groups
                        .iter()
                        .map(|group| ConflictItem::from((name.clone(), group.clone()))),
                )
                .collect(),
            requires_choice: true,
        });
        self.graph.graph.add_edge(parent, node, marker);
        node
    }

    /// Reject a concrete subgraph whose version choices still require external workspace context.
    fn validate_subgraph(
        &self,
        selected: &ValidatedSelection,
        roots: &[(&Package, InstallableRootKind)],
        context: DependencySelectionContext<'_>,
    ) -> Result<(), LockError> {
        let mut packages = roots
            .iter()
            .map(|(package, _)| &package.id.name)
            .chain(context.package())
            .collect::<BTreeSet<_>>();
        for (package, nodes) in self.lock.packages.iter().zip(&self.packages) {
            if std::iter::once(&nodes.production)
                .chain(nodes.extras.values())
                .chain(nodes.groups.values())
                .any(|node| !selected.marker(*node).is_false())
            {
                packages.insert(&package.id.name);
            }
        }
        let active = self
            .graph
            .graph
            .node_indices()
            .filter(|node| !selected.marker(*node).is_false())
            .flat_map(|node| &self.graph.graph[node].requests)
            .collect::<BTreeSet<_>>();
        for (choice, edge) in &self.choices {
            let Some(parent) = edge.parent else {
                continue;
            };
            for guard in self
                .graph
                .graph
                .edges_directed(*choice, petgraph::Direction::Incoming)
            {
                if selected.marker(guard.source()).is_false() {
                    continue;
                }
                let mut marker = UniversalMarker::from_combined(*guard.weight());
                for item in self.lock.conflicts.iter().flat_map(ConflictSet::iter) {
                    if !packages.contains(item.package()) {
                        continue;
                    }
                    if active.contains(item) {
                        marker.assume_conflict_item(item);
                    } else {
                        marker.assume_not_conflict_item(item);
                    }
                }
                if marker.has_conflict_marker() {
                    return Err(LockErrorKind::DependencyConflictOutsideSubgraph {
                        package: self.lock.package(parent).id.clone(),
                        dependency: self.lock.package(edge.target).id.clone(),
                    }
                    .into());
                }
            }
        }
        Ok(())
    }

    /// Attach one resolved version and its requested sections to a requirement.
    fn choice(
        &mut self,
        request: NodeIndex,
        parent: Option<PackageIndex>,
        target: PackageIndex,
        extras: &[ExtraName],
        groups: &[GroupName],
        marker: MarkerTree,
        kind: Edge,
    ) {
        if marker.is_false() {
            return;
        }
        let nodes = &self.packages[target.0];
        if groups.iter().any(|group| !nodes.groups.contains_key(group)) {
            return;
        }
        let node = self.graph.graph.add_node(ActivationNode::default());
        self.graph.graph.add_edge(request, node, marker);
        if groups.is_empty() {
            self.graph
                .graph
                .add_edge(node, nodes.production, MarkerTree::TRUE);
        }
        for extra in extras {
            if let Some(extra) = nodes.extras.get(extra) {
                self.graph.graph.add_edge(node, *extra, MarkerTree::TRUE);
            }
        }
        for group in groups {
            if let Some(group) = nodes.groups.get(group) {
                self.graph.graph.add_edge(node, *group, MarkerTree::TRUE);
            }
        }
        self.choices.push((
            node,
            SelectedEdge {
                parent,
                target,
                kind,
            },
        ));
    }

    /// Resolve a script or projectless-workspace requirement using its locked source and version.
    fn root_requirement(
        &mut self,
        root: NodeIndex,
        requirement: &Requirement,
        group: Option<&GroupName>,
        path: &Path,
    ) -> Result<(), LockError> {
        let marker = self.specialize(requirement.marker.resolve_extras([]));
        if marker.is_false() {
            return Ok(());
        }
        let request = self.request(
            root,
            &requirement.name,
            &requirement.extras,
            &requirement.groups,
            marker,
        );
        for package in self.lock.packages_for_name(&requirement.name) {
            if !Lock::package_satisfies_requirement(package, requirement, path)? {
                continue;
            }
            let marker = if package.fork_markers.is_empty() {
                MarkerTree::TRUE
            } else {
                package
                    .fork_markers
                    .iter()
                    .fold(MarkerTree::FALSE, |marker, fork| marker.or(fork.combined()))
            };
            let marker = self.specialize(marker);
            self.choice(
                request,
                None,
                self.lock.by_id[&package.id],
                &requirement.extras,
                &requirement.groups,
                marker,
                group.map_or(Edge::Prod, |group| Edge::Dev(group.clone())),
            );
        }
        Ok(())
    }
}

/// Keep concrete extra conflicts consistent with the legacy installation diagnostic.
fn selection_error(error: SelectionError) -> LockError {
    match error {
        SelectionError::Conflict { first, second, .. } => {
            let display_item = |item: &ConflictItem| match item.kind() {
                ConflictKind::Project => item.package().to_string(),
                ConflictKind::Extra(extra) => format!("{}[{extra}]", item.package()),
                ConflictKind::Group(group) => format!("{}:{group}", item.package()),
            };
            if let (ConflictKind::Extra(extra1), ConflictKind::Extra(extra2)) =
                (first.kind(), second.kind())
            {
                LockErrorKind::ConflictingExtra {
                    package1: first.package().clone(),
                    extra1: extra1.clone(),
                    package2: second.package().clone(),
                    extra2: extra2.clone(),
                }
                .into()
            } else {
                LockErrorKind::InvalidSelection {
                    reason: format!(
                        "Conflicting requests `{}` and `{}` are enabled simultaneously",
                        display_item(&first),
                        display_item(&second)
                    ),
                }
                .into()
            }
        }
        SelectionError::Unsatisfiable(_) => LockErrorKind::InvalidSelection {
            reason:
                "The selected requirements do not have exactly one compatible locked resolution"
                    .to_owned(),
        }
        .into(),
        SelectionError::Ambiguous(_) => LockErrorKind::InvalidSelection {
            reason:
                "The selected requirements have ambiguous extra or dependency-group activations"
                    .to_owned(),
        }
        .into(),
        SelectionError::UnknownConflictItem(item) => LockErrorKind::InvalidSelection {
            reason: format!("The lockfile refers to an unknown conflict item `{item}`"),
        }
        .into(),
    }
}
