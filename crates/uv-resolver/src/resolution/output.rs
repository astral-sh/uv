use std::borrow::Cow;
use std::collections::{BTreeSet, hash_map::Entry};
use std::sync::Arc;

use petgraph::{
    Directed, Direction,
    graph::{Graph, NodeIndex},
};
use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};

use uv_configuration::{Constraints, DependencyModifierScope, DependencyModifiers};
use uv_distribution_types::{
    DistributionId, HashCollection, IndexUrl, Name, Requirement, RequiresPython,
    ResolutionDiagnostic, parse_url_hashes,
};
use uv_normalize::{ExtraName, PackageName};
use uv_pep440::{Version, VersionSpecifier};
use uv_pypi_types::{Conflicts, HashDigests, ParsedUrl, VerbatimParsedUrl, Yanked};
use uv_types::HashStrategy;

use crate::graph_ops::{marker_reachability, simplify_conflict_markers};
use crate::preferences::Preferences;
use crate::resolution::{AnnotatedDist, ResolutionGraphNode, ResolverOutput};
use crate::resolution_mode::ResolutionStrategy;
use crate::resolver::{
    ResolutionDependencyEdge, ResolutionNode, ResolutionPackage, ResolvedFork, SelectedDistribution,
};
use crate::universal_marker::{ConflictMarker, UniversalMarker};
use crate::{InMemoryIndex, MetadataResponse, Options, ResolveError, VersionsResponse};

/// Create a new [`ResolverOutput`] from the resolved PubGrub state.
pub(crate) fn from_state(
    mut resolutions: Vec<ResolvedFork>,
    project: Option<&PackageName>,
    workspace_members: &BTreeSet<PackageName>,
    requirements: Vec<Requirement>,
    constraints: Constraints,
    modifiers: DependencyModifiers,
    preferences: &Preferences,
    hasher: &HashStrategy,
    index: &InMemoryIndex,
    requires_python: RequiresPython,
    conflicts: &Conflicts,
    resolution_strategy: &ResolutionStrategy,
    options: Options,
) -> Result<ResolverOutput, ResolveError> {
    let size_guess = resolutions[0].nodes.len();
    let mut graph: Graph<ResolutionGraphNode, UniversalMarker, Directed> =
        Graph::with_capacity(size_guess, size_guess);
    let mut inverse: FxHashMap<ResolutionNode, NodeIndex<u32>> =
        FxHashMap::with_capacity_and_hasher(size_guess, FxBuildHasher);
    let mut diagnostics = Vec::new();
    let mut extras = FxHashMap::default();

    // Add the root node.
    let root_index = graph.add_node(ResolutionGraphNode::Root);

    for resolution in &mut resolutions {
        // Add every package to the graph.
        for (package, selected) in resolution.nodes.drain(..) {
            let node = ResolutionNode {
                package,
                version: selected.version().clone(),
            };
            let Entry::Vacant(entry) = inverse.entry(node) else {
                // Insert each node only once.
                continue;
            };
            let package = &entry.key().package;
            let node = add_version(
                &mut graph,
                &mut diagnostics,
                &mut extras,
                preferences,
                hasher,
                index,
                package,
                selected,
                project == Some(&package.name) || workspace_members.contains(&package.name),
            );
            entry.insert(node);
        }
    }

    let mut seen = FxHashSet::default();
    for resolution in &resolutions {
        let marker = resolution.env.try_universal_markers().unwrap_or_default();

        // Add every edge to the graph, propagating the marker for the current fork, if
        // necessary.
        for edge in &resolution.edges {
            if !seen.insert((edge, marker)) {
                // Insert each node only once.
                continue;
            }

            add_edge(&mut graph, &inverse, root_index, edge, marker);
        }
    }

    let fork_markers: Vec<UniversalMarker> = if let [resolution] = resolutions.as_slice() {
        // In the case of a singleton marker, we only include it if it's not
        // always true. Otherwise, we keep our `fork_markers` empty as there
        // are no forks.
        resolution
            .env
            .try_universal_markers()
            .into_iter()
            .filter(|marker| !marker.is_true())
            .collect()
    } else {
        resolutions
            .iter()
            .map(|resolution| resolution.env.try_universal_markers().unwrap_or_default())
            .collect()
    };

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

fn add_edge(
    graph: &mut Graph<ResolutionGraphNode, UniversalMarker>,
    inverse: &FxHashMap<ResolutionNode, NodeIndex>,
    root_index: NodeIndex,
    edge: &ResolutionDependencyEdge,
    marker: UniversalMarker,
) {
    let from_index = edge.from.as_ref().map_or(root_index, |from| inverse[from]);
    let to_index = inverse[&edge.to];

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
    extras: &mut FxHashMap<*const MetadataResponse, FxHashSet<ExtraName>>,
    preferences: &Preferences,
    hasher: &HashStrategy,
    in_memory: &InMemoryIndex,
    package: &ResolutionPackage,
    selected: SelectedDistribution,
    is_workspace_member: bool,
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
        url.as_ref(),
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
            if !provides_extra(response, extra, extras) {
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
        is_workspace_member,
        marker: UniversalMarker::TRUE,
    }))
}

/// Returns whether the resolved distribution provides the requested extra.
fn provides_extra(
    response: &Arc<MetadataResponse>,
    extra: &ExtraName,
    extras: &mut FxHashMap<*const MetadataResponse, FxHashSet<ExtraName>>,
) -> bool {
    let MetadataResponse::Found(archive) = response.as_ref() else {
        return false;
    };

    if archive.metadata.provides_extra.len() <= 8 {
        return archive.metadata.provides_extra.contains(extra);
    }

    extras
        .entry(Arc::as_ptr(response))
        .or_insert_with(|| archive.metadata.provides_extra.iter().cloned().collect())
        .contains(extra)
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::str::FromStr;
    use std::sync::Arc;

    use rustc_hash::FxHashMap;

    use uv_distribution::{ArchiveMetadata, Metadata};
    use uv_normalize::{ExtraName, PackageName};
    use uv_pep440::Version;
    use uv_pep508::Requirement;

    use crate::MetadataResponse;
    use crate::resolution::output::provides_extra;

    fn metadata(extra_start: usize, extra_count: usize) -> Arc<MetadataResponse> {
        let provides_extra = (extra_start..extra_start + extra_count)
            .map(|extra| ExtraName::from_str(&format!("extra-{extra}")).expect("valid extra"))
            .collect();
        let requires_dist = (0..128)
            .map(|requirement| {
                Requirement::from_str(&format!(
                    "dependency-{requirement}>=1; python_version >= '3.8'"
                ))
                .expect("valid requirement")
                .into()
            })
            .collect();
        Arc::new(MetadataResponse::Found(ArchiveMetadata::from(Metadata {
            name: PackageName::from_str("package").expect("valid name"),
            version: Version::from_str("1.0.0").expect("valid version"),
            requires_dist,
            requires_python: None,
            provides_extra,
            dependency_groups: BTreeMap::default(),
            dynamic: false,
        })))
    }

    #[test]
    fn small_extra_lookup_avoids_cache() {
        let metadata = metadata(0, 8);
        let mut extras = FxHashMap::default();

        assert!(provides_extra(
            &metadata,
            &ExtraName::from_str("extra-7").expect("valid extra"),
            &mut extras,
        ));
        assert!(!provides_extra(
            &metadata,
            &ExtraName::from_str("missing").expect("valid extra"),
            &mut extras,
        ));
        assert!(extras.is_empty());
    }

    #[test]
    fn large_extra_lookup_is_scoped_to_distribution() {
        let first = metadata(0, 128);
        let second = metadata(64, 128);
        let mut extras = FxHashMap::default();

        for extra in 0..128 {
            assert!(provides_extra(
                &first,
                &ExtraName::from_str(&format!("extra-{extra}")).expect("valid extra"),
                &mut extras,
            ));
        }
        assert!(!provides_extra(
            &first,
            &ExtraName::from_str("extra-191").expect("valid extra"),
            &mut extras,
        ));
        assert!(!provides_extra(
            &second,
            &ExtraName::from_str("extra-0").expect("valid extra"),
            &mut extras,
        ));
        assert!(provides_extra(
            &second,
            &ExtraName::from_str("extra-191").expect("valid extra"),
            &mut extras,
        ));
        assert_eq!(extras.len(), 2);
    }
}
