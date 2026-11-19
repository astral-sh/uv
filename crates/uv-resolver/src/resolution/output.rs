use std::borrow::Cow;
use std::collections::{BTreeSet, hash_map::Entry};

use petgraph::{
    Directed, Direction,
    graph::{Graph, NodeIndex},
};
use rustc_hash::{FxHashMap, FxHashSet};

use uv_configuration::{Constraints, DependencyModifierScope, DependencyModifiers};
use uv_distribution_types::{
    DistributionId, HashCollection, IndexUrl, Name, Requirement, RequiresPython,
    ResolutionDiagnostic, parse_url_hashes,
};
use uv_git::GitResolver;
use uv_normalize::PackageName;
use uv_pep440::{Version, VersionSpecifier};
use uv_pypi_types::{Conflicts, HashDigests, ParsedUrl, VerbatimParsedUrl, Yanked};
use uv_resolver_types::PackageRole;
use uv_types::HashStrategy;

use crate::graph_ops::{marker_reachability, simplify_conflict_markers};
use crate::preferences::Preferences;
use crate::resolution::{AnnotatedDist, ResolutionGraphNode, ResolverOutput};
use crate::resolution_mode::ResolutionStrategy;
use crate::resolver::{
    Resolution, ResolutionDependencyEdge, ResolutionNode, ResolutionPackage, SelectedDistribution,
};
use crate::universal_marker::{ConflictMarker, UniversalMarker};
use crate::{InMemoryIndex, MetadataResponse, Options, ResolveError, VersionsResponse};

/// Merge completed forks without retaining their duplicate graphs or unused artifact pins.
pub(crate) struct ResolutionGraphBuilder<'a> {
    graph: Graph<ResolutionGraphNode, UniversalMarker, Directed>,
    root_index: NodeIndex,
    inverse: FxHashMap<ResolutionNode, NodeIndex>,
    fork_markers: Vec<UniversalMarker>,
    diagnostics: Vec<ResolutionDiagnostic>,
    project: Option<&'a PackageName>,
    workspace_members: &'a BTreeSet<PackageName>,
    preferences: &'a Preferences,
    hasher: &'a HashStrategy,
    index: &'a InMemoryIndex,
    git: &'a GitResolver,
}

impl<'a> ResolutionGraphBuilder<'a> {
    /// Start an output graph containing only the root.
    pub(crate) fn new(
        project: Option<&'a PackageName>,
        workspace_members: &'a BTreeSet<PackageName>,
        preferences: &'a Preferences,
        hasher: &'a HashStrategy,
        index: &'a InMemoryIndex,
        git: &'a GitResolver,
    ) -> Self {
        let mut graph = Graph::new();
        let root_index = graph.add_node(ResolutionGraphNode::Root);
        Self {
            graph,
            root_index,
            inverse: FxHashMap::default(),
            fork_markers: Vec::new(),
            diagnostics: Vec::new(),
            project,
            workspace_members,
            preferences,
            hasher,
            index,
            git,
        }
    }

    /// Return the number of completed forks.
    pub(crate) fn num_forks(&self) -> usize {
        self.fork_markers.len()
    }

    /// Consume a completed fork, retaining artifacts and metadata only for new nodes.
    pub(crate) fn push(&mut self, resolution: Resolution<'_>) -> Result<(), ResolveError> {
        if self.fork_markers.is_empty() {
            self.graph.reserve_nodes(resolution.nodes.len());
            self.graph.reserve_edges(resolution.edges.len());
            self.inverse.reserve(resolution.nodes.len());
        }
        let marker = resolution.env.try_universal_markers().unwrap_or_default();
        self.fork_markers.push(marker);
        let mut node_indices = Vec::with_capacity(resolution.nodes.len());
        // Add every package to the graph.
        for (package, version) in resolution.nodes {
            let node = ResolutionNode { package, version };
            let entry = match self.inverse.entry(node) {
                Entry::Occupied(entry) => {
                    node_indices.push(*entry.get());
                    continue;
                }
                Entry::Vacant(entry) => entry,
            };
            let package = &entry.key().package;
            let selected = SelectedDistribution::new(
                package,
                entry.key().version.clone(),
                &resolution.pins,
                self.index,
                self.git,
            )?;
            let package_role = if self.project == Some(&package.name) {
                PackageRole::Project
            } else if self.workspace_members.contains(&package.name) {
                PackageRole::WorkspaceMember
            } else {
                PackageRole::Dependency
            };
            let node = add_version(
                &mut self.graph,
                &mut self.diagnostics,
                self.preferences,
                self.hasher,
                self.index,
                package,
                selected,
                package_role,
            );
            entry.insert(node);
            node_indices.push(node);
        }
        drop(resolution.pins);

        // Add every edge to the graph, propagating the marker for the current fork, if
        // necessary. Only retain the duplicate-edge lookup while consuming this fork.
        let mut seen = FxHashSet::default();
        for edge in resolution.edges {
            let from_index = edge
                .from
                .map_or(self.root_index, |from| node_indices[from.0]);
            let to_index = node_indices[edge.to.0];
            // Local indices can denote different packages in different forks. Deduplicate only
            // after translating both endpoints into the merged graph.
            if !seen.insert((from_index, to_index, edge.marker)) {
                // Insert each edge only once.
                continue;
            }

            add_edge(&mut self.graph, from_index, to_index, &edge, marker);
        }
        Ok(())
    }

    /// Apply global reachability and conflict simplification after every fork has completed.
    pub(crate) fn finish(
        self,
        requirements: Vec<Requirement>,
        constraints: Constraints,
        modifiers: DependencyModifiers,
        requires_python: RequiresPython,
        conflicts: &Conflicts,
        resolution_strategy: &ResolutionStrategy,
        options: Options,
    ) -> Result<ResolverOutput, ResolveError> {
        let Self {
            mut graph,
            mut fork_markers,
            mut diagnostics,
            inverse,
            ..
        } = self;
        // The construction-only node lookup can be large; release it before propagating markers.
        drop(inverse);
        assert!(!fork_markers.is_empty(), "At least one fork must resolve");
        // A single unconditional resolution has no fork markers.
        if fork_markers.len() == 1 && fork_markers[0].is_true() {
            fork_markers.clear();
        }
        // Compute and apply the marker reachability.
        let mut reachability = marker_reachability(&graph, &fork_markers);

        // Apply the reachability to the graph and imbibe world
        // knowledge about conflicts.
        let conflict_marker = ConflictMarker::from_conflicts(conflicts);
        for index in graph.node_indices() {
            if let ResolutionGraphNode::Dist(dist) = &mut graph[index] {
                dist.marker = reachability.remove(&index).unwrap_or_default();
                dist.marker.imbibe(conflict_marker);
            }
        }
        for weight in graph.edge_weights_mut() {
            weight.imbibe(conflict_marker);
        }

        simplify_conflict_markers(conflicts, &mut graph);

        // Discard any unreachable nodes.
        graph.retain_nodes(|graph, node| !graph[node].marker().is_false());

        if matches!(resolution_strategy, ResolutionStrategy::Lowest) {
            report_missing_lower_bounds(&graph, &mut diagnostics, &constraints, &modifiers);
        }

        let output = ResolverOutput {
            graph,
            requires_python,
            fork_markers,
            diagnostics,
            requirements,
            constraints,
            modifiers,
            options,
        };

        // We only do conflicting distribution detection when no
        // conflicting groups have been specified. The reason here
        // is that when there are conflicting groups, then from the
        // perspective of marker expressions only, it may look like
        // one can install different versions of the same package for
        // the same marker environment. However, the thing preventing
        // this is that the only way this should be possible is if
        // one tries to install two or more conflicting extras at
        // the same time. At which point, uv will report an error,
        // thereby sidestepping the possibility of installing different
        // versions of the same package into the same virtualenv. ---AG
        //
        // FIXME: When `UniversalMarker` supports extras/groups, we can
        // re-enable this.
        if conflicts.is_empty() {
            #[allow(unused_mut, reason = "Used in debug_assertions below")]
            let mut conflicting = output.find_conflicting_distributions();
            if !conflicting.is_empty() {
                tracing::warn!(
                    "found {} conflicting distributions in resolution, \
                 please report this as a bug at \
                 `https://github.com/astral-sh/uv/issues/new`",
                    conflicting.len()
                );
            }
            // When testing, we materialize any conflicting distributions as an
            // error to ensure any relevant tests fail. Otherwise, we just leave
            // it at the warning message above. The reason for not returning an
            // error "in production" is that an incorrect resolution may only be
            // incorrect in certain marker environments, but fine in most others.
            // Returning an error in that case would make `uv` unusable whenever
            // the bug occurs, but letting it through means `uv` *could* still be
            // usable.
            #[cfg(debug_assertions)]
            if let Some(err) = conflicting.pop() {
                return Err(ResolveError::ConflictingDistribution(err));
            }
        }
        Ok(output)
    }
}

fn add_edge(
    graph: &mut Graph<ResolutionGraphNode, UniversalMarker>,
    from_index: NodeIndex,
    to_index: NodeIndex,
    edge: &ResolutionDependencyEdge,
    marker: UniversalMarker,
) {
    let edge_marker = {
        let mut edge_marker = edge.universal_marker();
        edge_marker.and(marker);
        edge_marker
    };

    if let Some(weight) = graph
        .find_edge(from_index, to_index)
        .and_then(|edge| graph.edge_weight_mut(edge))
    {
        // If either the existing marker or new marker is `true`, then the dependency is
        // included unconditionally, and so the combined marker is `true`.
        weight.or(edge_marker);
    } else {
        graph.update_edge(from_index, to_index, edge_marker);
    }
}

fn add_version(
    graph: &mut Graph<ResolutionGraphNode, UniversalMarker>,
    diagnostics: &mut Vec<ResolutionDiagnostic>,
    preferences: &Preferences,
    hasher: &HashStrategy,
    in_memory: &InMemoryIndex,
    package: &ResolutionPackage,
    selected: SelectedDistribution,
    package_role: PackageRole,
) -> NodeIndex {
    let ResolutionPackage {
        name,
        kind,
        url,
        index,
    } = &package;
    let hashes = get_hashes(
        name,
        index.as_ref(),
        url.as_deref(),
        &selected.hashes_id(),
        selected.version(),
        preferences,
        hasher,
        in_memory,
    );
    let (version, dist, metadata) = selected.into_parts();

    // Track yanks for registry distributions.
    match dist.yanked() {
        None | Some(Yanked::Bool(false)) => {}
        Some(Yanked::Bool(true)) => diagnostics.push(ResolutionDiagnostic::YankedVersion {
            dist: dist.clone(),
            reason: None,
        }),
        Some(Yanked::Reason(reason)) => diagnostics.push(ResolutionDiagnostic::YankedVersion {
            dist: dist.clone(),
            reason: Some(reason.to_string()),
        }),
    }

    if let Some(response) = metadata.as_ref()
        && let MetadataResponse::Found(archive) = response.as_ref()
    {
        let metadata = &archive.metadata;
        // Validate the extra.
        if let Some(extra) = kind.extra() {
            if !metadata.provides_extra.contains(extra) {
                diagnostics.push(ResolutionDiagnostic::MissingExtra {
                    dist: dist.clone(),
                    extra: extra.clone(),
                });
            }
        }

        // Validate the development dependency group.
        if let Some(dev) = kind.group() {
            if !metadata.dependency_groups.contains_key(dev) {
                diagnostics.push(ResolutionDiagnostic::MissingGroup {
                    dist: dist.clone(),
                    group: dev.clone(),
                });
            }
        }
    }

    // Add the distribution to the graph.
    graph.add_node(ResolutionGraphNode::Dist(AnnotatedDist {
        dist,
        name: name.clone(),
        version,
        kind: kind.clone(),
        hashes,
        metadata,
        role: package_role,
        marker: UniversalMarker::TRUE,
    }))
}

/// Identify the hashes for a concrete distribution, preserving any hashes that were provided
/// by the lockfile.
fn get_hashes(
    name: &PackageName,
    index: Option<&IndexUrl>,
    url: Option<&VerbatimParsedUrl>,
    metadata_id: &DistributionId,
    version: &Version,
    preferences: &Preferences,
    hasher: &HashStrategy,
    in_memory: &InMemoryIndex,
) -> HashDigests {
    // 1. Look for hashes from the lockfile.
    if let Some(digests) = preferences.match_hashes(name, version) {
        if !digests.is_empty() {
            return HashDigests::from(digests);
        }
    }

    // 2. Preserve trusted hashes for this URL or path. Wheel metadata lookup does not always
    // hash the archive, so installation still needs the original hashes to verify its contents.
    if let Some(url) = url {
        let policy = hasher.archive_policy_for_url(&url.verbatim);
        if !policy.digests().is_empty() {
            return HashDigests::from(policy.digests());
        }
    }

    // 3. Reuse a direct URL's declared hash when collecting hashes without validation.
    if let Some(url) = url
        && let ParsedUrl::Archive(_) = &url.parsed_url
        && hasher.collection() != HashCollection::None
        && !hasher
            .archive_policy_for_url(&url.verbatim)
            .requires_validation()
        && let Some(hashes) = parse_url_hashes(&url.verbatim)
    {
        return hashes;
    }

    // 4. Look for hashes computed for the specific wheel or source distribution.
    if let Some(metadata_response) = in_memory.distributions().get(metadata_id) {
        if let MetadataResponse::Found(ref archive) = *metadata_response {
            let mut digests = archive.hashes.clone();
            digests.sort_unstable();
            if !digests.is_empty() {
                return digests;
            }
        }
    }

    // 5. Look for hashes from the registry, which are served at the package level.
    if url.is_none() {
        // Query the implicit and explicit indexes (lazily) for the hashes.
        let implicit_response = in_memory.implicit().get(name);
        let mut explicit_response = None;

        // Search in the implicit indexes.
        let hashes = implicit_response
            .as_ref()
            .and_then(|response| {
                if let VersionsResponse::Found(version_maps) = &**response {
                    Some(version_maps)
                } else {
                    None
                }
            })
            .into_iter()
            .flatten()
            .filter(|version_map| version_map.index() == index)
            .find_map(|version_map| version_map.hashes(version))
            .or_else(|| {
                // Search in the explicit indexes.
                explicit_response = index
                    .and_then(|index| in_memory.explicit().get(&(name.clone(), index.clone())));
                explicit_response
                    .as_ref()
                    .and_then(|response| {
                        if let VersionsResponse::Found(version_maps) = &**response {
                            Some(version_maps)
                        } else {
                            None
                        }
                    })
                    .into_iter()
                    .flatten()
                    .filter(|version_map| version_map.index() == index)
                    .find_map(|version_map| version_map.hashes(version))
            });

        if let Some(hashes) = hashes {
            let mut digests = HashDigests::from(hashes);
            digests.sort_unstable();
            if !digests.is_empty() {
                return digests;
            }
        }
    }

    HashDigests::empty()
}

/// Find any packages that don't have any lower bound on them when in resolution-lowest mode.
fn report_missing_lower_bounds(
    graph: &Graph<ResolutionGraphNode, UniversalMarker>,
    diagnostics: &mut Vec<ResolutionDiagnostic>,
    constraints: &Constraints,
    modifiers: &DependencyModifiers,
) {
    let mut missing_lower_bounds = Vec::new();
    for node_index in graph.node_indices() {
        let ResolutionGraphNode::Dist(dist) = graph.node_weight(node_index).unwrap() else {
            // Ignore the root package.
            continue;
        };
        if !has_lower_bound(node_index, dist.name(), graph, constraints, modifiers) {
            missing_lower_bounds.push(dist.name());
        }
    }
    missing_lower_bounds.sort_unstable();
    diagnostics.extend(missing_lower_bounds.into_iter().map(|package_name| {
        ResolutionDiagnostic::MissingLowerBound {
            package_name: package_name.clone(),
        }
    }));
}

/// Whether the given package has a lower version bound by another package.
fn has_lower_bound(
    node_index: NodeIndex,
    package_name: &PackageName,
    graph: &Graph<ResolutionGraphNode, UniversalMarker>,
    constraints: &Constraints,
    modifiers: &DependencyModifiers,
) -> bool {
    for neighbor_index in graph.neighbors_directed(node_index, Direction::Incoming) {
        let neighbor_dist = match graph.node_weight(neighbor_index).unwrap() {
            ResolutionGraphNode::Root => {
                // We already handled direct dependencies with a missing constraint
                // separately.
                return true;
            }
            ResolutionGraphNode::Dist(neighbor_dist) => neighbor_dist,
        };

        if neighbor_dist.name() == package_name {
            // Only warn for real packages, not for virtual packages such as dev nodes.
            return true;
        }

        let Some(metadata) = neighbor_dist.metadata() else {
            // We can't check for lower bounds if we lack metadata.
            return true;
        };

        // Get all individual specifier for the current package and check if any has a lower
        // bound.
        for requirement in modifiers
            .apply(
                DependencyModifierScope::Package(neighbor_dist.name(), &neighbor_dist.version),
                metadata.requires_dist.iter(),
            )
            .chain(modifiers.apply(
                DependencyModifierScope::Global,
                metadata.dependency_groups.values().flatten(),
            ))
            // Constraints are missing from the graph.
            .chain(constraints.requirements().map(Cow::Borrowed))
        {
            if requirement.name != *package_name {
                continue;
            }
            let Some(specifiers) = requirement.source.version_specifiers() else {
                // URL requirements are a bound.
                return true;
            };
            if specifiers.iter().any(VersionSpecifier::has_lower_bound) {
                return true;
            }
        }
    }
    false
}
