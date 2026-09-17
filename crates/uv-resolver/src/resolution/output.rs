use std::borrow::Cow;
use std::collections::BTreeSet;
use std::sync::Arc;

use petgraph::{
    Directed, Direction,
    graph::{Graph, NodeIndex},
};
use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};

use uv_configuration::{Constraints, Overrides};
use uv_distribution::Metadata;
use uv_distribution_types::{
    Dist, DistributionId, HashCollection, Identifier, IndexUrl, Name, Requirement, RequiresPython,
    ResolutionDiagnostic, ResolvedDist, parse_url_hashes,
};
use uv_git::GitResolver;
use uv_normalize::PackageName;
use uv_pep440::{Version, VersionSpecifier};
use uv_pypi_types::{Conflicts, HashDigests, ParsedUrl, VerbatimParsedUrl, Yanked};
use uv_types::HashStrategy;

use crate::graph_ops::{marker_reachability, simplify_conflict_markers};
use crate::pins::FilePins;
use crate::preferences::Preferences;
use crate::redirect::url_to_precise;
use crate::resolution::{AnnotatedDist, ResolutionGraphNode, ResolverOutput};
use crate::resolution_mode::ResolutionStrategy;
use crate::resolver::{Resolution, ResolutionDependencyEdge, ResolutionPackage};
use crate::universal_marker::{ConflictMarker, UniversalMarker};
use crate::{InMemoryIndex, MetadataResponse, Options, ResolveError, VersionsResponse};

#[derive(Debug, Eq, PartialEq, Hash)]
struct PackageRef<'a> {
    package: &'a ResolutionPackage,
    version: &'a Version,
}

/// Create a new [`ResolverOutput`] from the resolved PubGrub state.
pub(crate) fn from_state(
    resolutions: &[Resolution],
    project: Option<&PackageName>,
    workspace_members: &BTreeSet<PackageName>,
    requirements: Vec<Requirement>,
    constraints: Constraints,
    overrides: Overrides,
    preferences: &Preferences,
    hasher: &HashStrategy,
    index: &InMemoryIndex,
    git: &GitResolver,
    requires_python: RequiresPython,
    conflicts: &Conflicts,
    resolution_strategy: &ResolutionStrategy,
    options: Options,
) -> Result<ResolverOutput, ResolveError> {
    let size_guess = resolutions[0].nodes.len();
    let mut graph: Graph<ResolutionGraphNode, UniversalMarker, Directed> =
        Graph::with_capacity(size_guess, size_guess);
    let mut inverse: FxHashMap<PackageRef, NodeIndex<u32>> =
        FxHashMap::with_capacity_and_hasher(size_guess, FxBuildHasher);
    let mut diagnostics = Vec::new();

    // Add the root node.
    let root_index = graph.add_node(ResolutionGraphNode::Root);

    let mut seen = FxHashSet::default();
    for resolution in resolutions {
        // Add every package to the graph.
        for (package, version) in &resolution.nodes {
            if !seen.insert((package, version)) {
                // Insert each node only once.
                continue;
            }
            add_version(
                &mut graph,
                &mut inverse,
                &mut diagnostics,
                preferences,
                hasher,
                &resolution.pins,
                index,
                git,
                package,
                version,
                project == Some(&package.name) || workspace_members.contains(&package.name),
            )?;
        }
    }

    let mut seen = FxHashSet::default();
    for resolution in resolutions {
        let marker = resolution.env.try_universal_markers().unwrap_or_default();

        // Add every edge to the graph, propagating the marker for the current fork, if
        // necessary.
        for edge in &resolution.edges {
            if !seen.insert((edge, marker)) {
                // Insert each node only once.
                continue;
            }

            add_edge(&mut graph, &mut inverse, root_index, edge, marker);
        }
    }

    let fork_markers: Vec<UniversalMarker> = if let [resolution] = resolutions {
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
        report_missing_lower_bounds(&graph, &mut diagnostics, &constraints, &overrides);
    }

    let output = ResolverOutput {
        graph,
        requires_python,
        fork_markers,
        diagnostics,
        requirements,
        constraints,
        overrides,
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
                 https://github.com/astral-sh/uv/issues/new",
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
    inverse: &mut FxHashMap<PackageRef<'_>, NodeIndex>,
    root_index: NodeIndex,
    edge: &ResolutionDependencyEdge,
    marker: UniversalMarker,
) {
    let from_index = edge.from.as_ref().map_or(root_index, |from| {
        inverse[&PackageRef {
            package: &from.package,
            version: &from.version,
        }]
    });
    let to_index = inverse[&PackageRef {
        package: &edge.to.package,
        version: &edge.to.version,
    }];

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

fn add_version<'a>(
    graph: &mut Graph<ResolutionGraphNode, UniversalMarker>,
    inverse: &mut FxHashMap<PackageRef<'a>, NodeIndex>,
    diagnostics: &mut Vec<ResolutionDiagnostic>,
    preferences: &Preferences,
    hasher: &HashStrategy,
    pins: &FilePins,
    in_memory: &InMemoryIndex,
    git: &GitResolver,
    package: &'a ResolutionPackage,
    version: &'a Version,
    is_workspace_member: bool,
) -> Result<(), ResolveError> {
    let ResolutionPackage {
        name,
        facet,
        url,
        index,
    } = &package;
    // Map the package to a distribution.
    let (dist, hashes, metadata) = parse_dist(
        name,
        index.as_ref(),
        url.as_ref(),
        version,
        pins,
        diagnostics,
        preferences,
        hasher,
        in_memory,
        git,
    )?;

    // We normally write dependency paths relative to the lockfile. For the current project and
    // workspace members, preserve the user's choice of relative or absolute paths instead.
    // Metadata from `tool.uv.dependency-metadata` already preserves that choice.
    // Only change this copy, not shared metadata.
    let metadata = if is_workspace_member {
        metadata.map(|metadata| metadata.with_force_relative(false))
    } else {
        metadata
    };

    if let Some(metadata) = metadata.as_ref() {
        // Validate the extra.
        if let Some(extra) = facet.extra() {
            if !metadata.provides_extra.contains(extra) {
                diagnostics.push(ResolutionDiagnostic::MissingExtra {
                    dist: dist.clone(),
                    extra: extra.clone(),
                });
            }
        }

        // Validate the development dependency group.
        if let Some(dev) = facet.group() {
            if !metadata.dependency_groups.contains_key(dev) {
                diagnostics.push(ResolutionDiagnostic::MissingGroup {
                    dist: dist.clone(),
                    group: dev.clone(),
                });
            }
        }
    }

    // Add the distribution to the graph.
    let node = graph.add_node(ResolutionGraphNode::Dist(AnnotatedDist {
        dist,
        name: name.clone(),
        version: version.clone(),
        facet: facet.clone(),
        hashes,
        metadata,
        marker: UniversalMarker::TRUE,
    }));
    inverse.insert(PackageRef { package, version }, node);
    Ok(())
}

fn parse_dist(
    name: &PackageName,
    index: Option<&IndexUrl>,
    url: Option<&VerbatimParsedUrl>,
    version: &Version,
    pins: &FilePins,
    diagnostics: &mut Vec<ResolutionDiagnostic>,
    preferences: &Preferences,
    hasher: &HashStrategy,
    in_memory: &InMemoryIndex,
    git: &GitResolver,
) -> Result<(ResolvedDist, HashDigests, Option<Metadata>), ResolveError> {
    Ok(if let Some(url) = url {
        // Create the locked distribution and recover the metadata using the original URL that
        // was requested during resolution.
        let dist = Dist::from_url(name.clone(), url_to_precise(url.clone(), git))?;
        let metadata_id = Dist::from_url(name.clone(), url.clone())?.distribution_id();

        // Extract the hashes.
        let hashes = get_hashes(
            name,
            index,
            Some(url),
            &metadata_id,
            version,
            preferences,
            hasher,
            in_memory,
        );

        // Extract the metadata.
        let metadata = {
            let response = in_memory
                .distributions()
                .get(&metadata_id)
                .unwrap_or_else(|| {
                    panic!("Every URL distribution should have metadata: {metadata_id:?}")
                });

            let MetadataResponse::Found(archive) = &*response else {
                panic!("Every URL distribution should have metadata: {metadata_id:?}")
            };

            archive.metadata.clone()
        };

        (
            ResolvedDist::Installable {
                dist: Arc::new(dist),
                version: Some(version.clone()),
            },
            hashes,
            Some(metadata),
        )
    } else {
        let (dist, metadata_id) = pins
            .dist_and_id(name, version)
            .expect("Every package should be pinned");
        let dist = dist.clone();
        let hashes_id = dist.distribution_id();

        // Track yanks for any registry distributions.
        match dist.yanked() {
            None | Some(Yanked::Bool(false)) => {}
            Some(Yanked::Bool(true)) => {
                diagnostics.push(ResolutionDiagnostic::YankedVersion {
                    dist: dist.clone(),
                    reason: None,
                });
            }
            Some(Yanked::Reason(reason)) => {
                diagnostics.push(ResolutionDiagnostic::YankedVersion {
                    dist: dist.clone(),
                    reason: Some(reason.to_string()),
                });
            }
        }

        // Extract the hashes.
        let hashes = get_hashes(
            name,
            index,
            None,
            &hashes_id,
            version,
            preferences,
            hasher,
            in_memory,
        );

        // Extract the metadata.
        let metadata = {
            in_memory
                .distributions()
                .get(metadata_id)
                .and_then(|response| {
                    if let MetadataResponse::Found(archive) = &*response {
                        Some(archive.metadata.clone())
                    } else {
                        None
                    }
                })
        };

        (dist, hashes, metadata)
    })
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

    // 2. Reuse a direct URL's declared hash when collecting hashes without validation.
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

    // 3. Look for hashes computed for the specific wheel or source distribution.
    if let Some(metadata_response) = in_memory.distributions().get(metadata_id) {
        if let MetadataResponse::Found(ref archive) = *metadata_response {
            let mut digests = archive.hashes.clone();
            digests.sort_unstable();
            if !digests.is_empty() {
                return digests;
            }
        }
    }

    // 4. Look for hashes from the registry, which are served at the package level.
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
    overrides: &Overrides,
) {
    for node_index in graph.node_indices() {
        let ResolutionGraphNode::Dist(dist) = graph.node_weight(node_index).unwrap() else {
            // Ignore the root package.
            continue;
        };
        if !has_lower_bound(node_index, dist.name(), graph, constraints, overrides) {
            diagnostics.push(ResolutionDiagnostic::MissingLowerBound {
                package_name: dist.name().clone(),
            });
        }
    }
}

/// Whether the given package has a lower version bound by another package.
fn has_lower_bound(
    node_index: NodeIndex,
    package_name: &PackageName,
    graph: &Graph<ResolutionGraphNode, UniversalMarker>,
    constraints: &Constraints,
    overrides: &Overrides,
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

        let Some(metadata) = neighbor_dist.metadata.as_ref() else {
            // We can't check for lower bounds if we lack metadata.
            return true;
        };

        // Get all individual specifier for the current package and check if any has a lower
        // bound.
        for requirement in overrides
            .apply_for(
                neighbor_dist.name(),
                &neighbor_dist.version,
                metadata.requires_dist.iter(),
            )
            .chain(overrides.apply(metadata.dependency_groups.values().flatten()))
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
