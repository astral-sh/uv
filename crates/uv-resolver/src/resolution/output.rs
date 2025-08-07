use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};
use std::sync::Arc;

use indexmap::IndexSet;
use petgraph::{
    Directed, Direction,
    graph::{Graph, NodeIndex},
};
use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};

use uv_configuration::{Constraints, Overrides};
use uv_distribution::Metadata;
use uv_distribution_types::{
    Dist, DistributionMetadata, Edge, IndexUrl, Name, Node, Requirement, RequiresPython,
    ResolutionDiagnostic, ResolvedDist, VersionId, VersionOrUrlRef,
};
use uv_git::GitResolver;
use uv_normalize::{ExtraName, GroupName, PackageName};
use uv_pep440::{Version, VersionSpecifier};
use uv_pep508::{MarkerEnvironment, MarkerTree, MarkerTreeKind};
use uv_pypi_types::{Conflicts, HashDigests, ParsedUrlError, VerbatimParsedUrl, Yanked};

use crate::graph_ops::{marker_reachability, simplify_conflict_markers};
use crate::pins::FilePins;
use crate::preferences::Preferences;
use crate::redirect::url_to_precise;
use crate::resolution::AnnotatedDist;
use crate::resolution_mode::ResolutionStrategy;
use crate::resolver::{Resolution, ResolutionDependencyEdge, ResolutionPackage};
use crate::universal_marker::{ConflictMarker, UniversalMarker};
use crate::{
    InMemoryIndex, MetadataResponse, Options, PythonRequirement, ResolveError, VersionsResponse,
};

/// The output of a successful resolution.
///
/// Includes a complete resolution graph in which every node represents a pinned package and every
/// edge represents a dependency between two pinned packages.
#[derive(Debug)]
pub struct ResolverOutput {
    /// The underlying graph.
    pub(crate) graph: Graph<ResolutionGraphNode, UniversalMarker, Directed>,
    /// The range of supported Python versions.
    pub(crate) requires_python: RequiresPython,
    /// If the resolution had non-identical forks, store the forks in the lockfile so we can
    /// recreate them in subsequent resolutions.
    pub(crate) fork_markers: Vec<UniversalMarker>,
    /// Any diagnostics that were encountered while building the graph.
    pub(crate) diagnostics: Vec<ResolutionDiagnostic>,
    /// The requirements that were used to build the graph.
    pub(crate) requirements: Vec<Requirement>,
    /// The constraints that were used to build the graph.
    pub(crate) constraints: Constraints,
    /// The overrides that were used to build the graph.
    pub(crate) overrides: Overrides,
    /// The options that were used to build the graph.
    pub(crate) options: Options,
}

#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub(crate) enum ResolutionGraphNode {
    Root,
    Dist(AnnotatedDist),
}

impl ResolutionGraphNode {
    pub(crate) fn marker(&self) -> &UniversalMarker {
        match self {
            Self::Root => &UniversalMarker::TRUE,
            Self::Dist(dist) => &dist.marker,
        }
    }

    pub(crate) fn package_extra_names(&self) -> Option<(&PackageName, &ExtraName)> {
        match self {
            Self::Root => None,
            Self::Dist(dist) => {
                let extra = dist.extra.as_ref()?;
                Some((&dist.name, extra))
            }
        }
    }

    pub(crate) fn package_group_names(&self) -> Option<(&PackageName, &GroupName)> {
        match self {
            Self::Root => None,
            Self::Dist(dist) => {
                let group = dist.dev.as_ref()?;
                Some((&dist.name, group))
            }
        }
    }

    pub(crate) fn package_name(&self) -> Option<&PackageName> {
        match self {
            Self::Root => None,
            Self::Dist(dist) => Some(&dist.name),
        }
    }
}

impl Display for ResolutionGraphNode {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Root => f.write_str("root"),
            Self::Dist(dist) => Display::fmt(dist, f),
        }
    }
}

#[derive(Debug, Eq, PartialEq, Hash)]
struct PackageRef<'a> {
    package_name: &'a PackageName,
    version: &'a Version,
    url: Option<&'a VerbatimParsedUrl>,
    index: Option<&'a IndexUrl>,
    extra: Option<&'a ExtraName>,
    group: Option<&'a GroupName>,
}

impl ResolverOutput {
    /// Create a new [`ResolverOutput`] from the resolved PubGrub state.
    pub(crate) fn from_state(
        resolutions: &[Resolution],
        requirements: &[Requirement],
        constraints: &Constraints,
        overrides: &Overrides,
        preferences: &Preferences,
        index: &InMemoryIndex,
        git: &GitResolver,
        python: &PythonRequirement,
        conflicts: &Conflicts,
        resolution_strategy: &ResolutionStrategy,
        options: Options,
    ) -> Result<Self, ResolveError> {
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
                Self::add_version(
                    &mut graph,
                    &mut inverse,
                    &mut diagnostics,
                    preferences,
                    &resolution.pins,
                    index,
                    git,
                    package,
                    version,
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

                Self::add_edge(&mut graph, &mut inverse, root_index, edge, marker);
            }
        }

        // Extract the `Requires-Python` range, if provided.
        let requires_python = python.target().clone();

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
            report_missing_lower_bounds(&graph, &mut diagnostics, constraints, overrides);
        }

        let output = Self {
            graph,
            requires_python,
            diagnostics,
            requirements: requirements.to_vec(),
            constraints: constraints.clone(),
            overrides: overrides.clone(),
            options,
            fork_markers,
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
                package_name: from,
                version: &edge.from_version,
                url: edge.from_url.as_ref(),
                index: edge.from_index.as_ref(),
                extra: edge.from_extra.as_ref(),
                group: edge.from_dev.as_ref(),
            }]
        });
        let to_index = inverse[&PackageRef {
            package_name: &edge.to,
            version: &edge.to_version,
            url: edge.to_url.as_ref(),
            index: edge.to_index.as_ref(),
            extra: edge.to_extra.as_ref(),
            group: edge.to_dev.as_ref(),
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
        pins: &FilePins,
        in_memory: &InMemoryIndex,
        git: &GitResolver,
        package: &'a ResolutionPackage,
        version: &'a Version,
    ) -> Result<(), ResolveError> {
        let ResolutionPackage {
            name,
            extra,
            dev,
            url,
            index,
        } = &package;
        // Map the package to a distribution.
        let (dist, hashes, metadata) = Self::parse_dist(
            name,
            index.as_ref(),
            url.as_ref(),
            version,
            pins,
            diagnostics,
            preferences,
            in_memory,
            git,
        )?;

        if let Some(metadata) = metadata.as_ref() {
            // Validate the extra.
            if let Some(extra) = extra {
                if !metadata.provides_extras.contains(extra) {
                    diagnostics.push(ResolutionDiagnostic::MissingExtra {
                        dist: dist.clone(),
                        extra: extra.clone(),
                    });
                }
            }

            // Validate the development dependency group.
            if let Some(dev) = dev {
                if !metadata.dependency_groups.contains_key(dev) {
                    diagnostics.push(ResolutionDiagnostic::MissingDev {
                        dist: dist.clone(),
                        dev: dev.clone(),
                    });
                }
            }
        }

        // Add the distribution to the graph.
        let node = graph.add_node(ResolutionGraphNode::Dist(AnnotatedDist {
            dist,
            name: name.clone(),
            version: version.clone(),
            extra: extra.clone(),
            dev: dev.clone(),
            hashes,
            metadata,
            marker: UniversalMarker::TRUE,
        }));
        inverse.insert(
            PackageRef {
                package_name: name,
                version,
                url: url.as_ref(),
                index: index.as_ref(),
                extra: extra.as_ref(),
                group: dev.as_ref(),
            },
            node,
        );
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
        in_memory: &InMemoryIndex,
        git: &GitResolver,
    ) -> Result<(ResolvedDist, HashDigests, Option<Metadata>), ResolveError> {
        Ok(if let Some(url) = url {
            // Create the distribution.
            let dist = Dist::from_url(name.clone(), url_to_precise(url.clone(), git))?;

            let version_id = VersionId::from_url(&url.verbatim);

            // Extract the hashes.
            let hashes = Self::get_hashes(
                name,
                index,
                Some(url),
                &version_id,
                version,
                preferences,
                in_memory,
            );

            // Extract the metadata.
            let metadata = {
                let response = in_memory
                    .distributions()
                    .get(&version_id)
                    .unwrap_or_else(|| {
                        panic!("Every URL distribution should have metadata: {version_id:?}")
                    });

                let MetadataResponse::Found(archive) = &*response else {
                    panic!("Every URL distribution should have metadata: {version_id:?}")
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
            let dist = pins
                .get(name, version)
                .expect("Every package should be pinned")
                .clone();

            let version_id = dist.version_id();

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
            let hashes = Self::get_hashes(
                name,
                index,
                None,
                &version_id,
                version,
                preferences,
                in_memory,
            );

            // Extract the metadata.
            let metadata = {
                in_memory
                    .distributions()
                    .get(&version_id)
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

    /// Identify the hashes for the [`VersionId`], preserving any hashes that were provided by the
    /// lockfile.
    fn get_hashes(
        name: &PackageName,
        index: Option<&IndexUrl>,
        url: Option<&VerbatimParsedUrl>,
        version_id: &VersionId,
        version: &Version,
        preferences: &Preferences,
        in_memory: &InMemoryIndex,
    ) -> HashDigests {
        // 1. Look for hashes from the lockfile.
        if let Some(digests) = preferences.match_hashes(name, version) {
            if !digests.is_empty() {
                return HashDigests::from(digests);
            }
        }

        // 2. Look for hashes for the distribution (i.e., the specific wheel or source distribution).
        if let Some(metadata_response) = in_memory.distributions().get(version_id) {
            if let MetadataResponse::Found(ref archive) = *metadata_response {
                let mut digests = archive.hashes.clone();
                digests.sort_unstable();
                if !digests.is_empty() {
                    return digests;
                }
            }
        }

        // 3. Look for hashes from the registry, which are served at the package level.
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

    /// Returns an iterator over the distinct packages in the graph.
    fn dists(&self) -> impl Iterator<Item = &AnnotatedDist> {
        self.graph
            .node_indices()
            .filter_map(move |index| match &self.graph[index] {
                ResolutionGraphNode::Root => None,
                ResolutionGraphNode::Dist(dist) => Some(dist),
            })
    }

    /// Return the number of distinct packages in the graph.
    pub fn len(&self) -> usize {
        self.dists().filter(|dist| dist.is_base()).count()
    }

    /// Return `true` if there are no packages in the graph.
    pub fn is_empty(&self) -> bool {
        self.dists().any(AnnotatedDist::is_base)
    }

    /// Returns `true` if the graph contains the given package.
    pub fn contains(&self, name: &PackageName) -> bool {
        self.dists().any(|dist| dist.name() == name)
    }

    /// Return the [`ResolutionDiagnostic`]s that were encountered while building the graph.
    pub fn diagnostics(&self) -> &[ResolutionDiagnostic] {
        &self.diagnostics
    }

    /// Return the marker tree specific to this resolution.
    ///
    /// This accepts an in-memory-index and marker environment, all
    /// of which should be the same values given to the resolver that produced
    /// this graph.
    ///
    /// The marker tree returned corresponds to an expression that, when true,
    /// this resolution is guaranteed to be correct. Note though that it's
    /// possible for resolution to be correct even if the returned marker
    /// expression is false.
    ///
    /// For example, if the root package has a dependency `foo; sys_platform ==
    /// "macos"` and resolution was performed on Linux, then the marker tree
    /// returned will contain a `sys_platform == "linux"` expression. This
    /// means that whenever the marker expression evaluates to true (i.e., the
    /// current platform is Linux), then the resolution here is correct. But
    /// it is possible that the resolution is also correct on other platforms
    /// that aren't macOS, such as Windows. (It is unclear at time of writing
    /// whether this is fundamentally impossible to compute, or just impossible
    /// to compute in some cases.)
    pub fn marker_tree(
        &self,
        index: &InMemoryIndex,
        marker_env: &MarkerEnvironment,
    ) -> Result<MarkerTree, Box<ParsedUrlError>> {
        use uv_pep508::{
            CanonicalMarkerValueString, CanonicalMarkerValueVersion, MarkerExpression,
            MarkerOperator, MarkerTree,
        };

        /// A subset of the possible marker values.
        ///
        /// We only track the marker parameters that are referenced in a marker
        /// expression. We'll use references to the parameter later to generate
        /// values based on the current marker environment.
        #[derive(Debug, Eq, Hash, PartialEq)]
        enum MarkerParam {
            Version(CanonicalMarkerValueVersion),
            String(CanonicalMarkerValueString),
        }

        /// Add all marker parameters from the given tree to the given set.
        fn add_marker_params_from_tree(marker_tree: MarkerTree, set: &mut IndexSet<MarkerParam>) {
            match marker_tree.kind() {
                MarkerTreeKind::True => {}
                MarkerTreeKind::False => {}
                MarkerTreeKind::Version(marker) => {
                    set.insert(MarkerParam::Version(marker.key()));
                    for (_, tree) in marker.edges() {
                        add_marker_params_from_tree(tree, set);
                    }
                }
                MarkerTreeKind::String(marker) => {
                    set.insert(MarkerParam::String(marker.key()));
                    for (_, tree) in marker.children() {
                        add_marker_params_from_tree(tree, set);
                    }
                }
                MarkerTreeKind::In(marker) => {
                    set.insert(MarkerParam::String(marker.key()));
                    for (_, tree) in marker.children() {
                        add_marker_params_from_tree(tree, set);
                    }
                }
                MarkerTreeKind::Contains(marker) => {
                    set.insert(MarkerParam::String(marker.key()));
                    for (_, tree) in marker.children() {
                        add_marker_params_from_tree(tree, set);
                    }
                }
                // We specifically don't care about these for the
                // purposes of generating a marker string for a lock
                // file. Quoted strings are marker values given by the
                // user. We don't track those here, since we're only
                // interested in which markers are used.
                MarkerTreeKind::Extra(marker) => {
                    for (_, tree) in marker.children() {
                        add_marker_params_from_tree(tree, set);
                    }
                }
                MarkerTreeKind::List(marker) => {
                    for (_, tree) in marker.children() {
                        add_marker_params_from_tree(tree, set);
                    }
                }
            }
        }

        let mut seen_marker_values = IndexSet::default();
        for i in self.graph.node_indices() {
            let ResolutionGraphNode::Dist(dist) = &self.graph[i] else {
                continue;
            };
            let version_id = match dist.version_or_url() {
                VersionOrUrlRef::Version(version) => {
                    VersionId::from_registry(dist.name().clone(), version.clone())
                }
                VersionOrUrlRef::Url(verbatim_url) => VersionId::from_url(verbatim_url.raw()),
            };
            let res = index
                .distributions()
                .get(&version_id)
                .expect("every package in resolution graph has metadata");
            let MetadataResponse::Found(archive, ..) = &*res else {
                panic!(
                    "Every package should have metadata: {:?}",
                    dist.version_id()
                )
            };
            for req in self
                .constraints
                .apply(self.overrides.apply(archive.metadata.requires_dist.iter()))
            {
                add_marker_params_from_tree(req.marker, &mut seen_marker_values);
            }
        }

        // Ensure that we consider markers from direct dependencies.
        for direct_req in self
            .constraints
            .apply(self.overrides.apply(self.requirements.iter()))
        {
            add_marker_params_from_tree(direct_req.marker, &mut seen_marker_values);
        }

        // Generate the final marker expression as a conjunction of
        // strict equality terms.
        let mut conjunction = MarkerTree::TRUE;
        for marker_param in seen_marker_values {
            let expr = match marker_param {
                MarkerParam::Version(value_version) => {
                    let from_env = marker_env.get_version(value_version);
                    MarkerExpression::Version {
                        key: value_version.into(),
                        specifier: VersionSpecifier::equals_version(from_env.clone()),
                    }
                }
                MarkerParam::String(value_string) => {
                    let from_env = marker_env.get_string(value_string);
                    MarkerExpression::String {
                        key: value_string.into(),
                        operator: MarkerOperator::Equal,
                        value: from_env.into(),
                    }
                }
            };
            conjunction.and(MarkerTree::expression(expr));
        }
        Ok(conjunction)
    }

    /// Returns a sequence of conflicting distribution errors from this
    /// resolution.
    ///
    /// Correct resolutions always return an empty sequence. A non-empty
    /// sequence implies there is a package with two distinct versions in the
    /// same marker environment in this resolution. This in turn implies that
    /// an installation in that marker environment could wind up trying to
    /// install different versions of the same package, which is not allowed.
    fn find_conflicting_distributions(&self) -> Vec<ConflictingDistributionError> {
        let mut name_to_markers: BTreeMap<&PackageName, Vec<(&Version, &UniversalMarker)>> =
            BTreeMap::new();
        for node in self.graph.node_weights() {
            let annotated_dist = match node {
                ResolutionGraphNode::Root => continue,
                ResolutionGraphNode::Dist(annotated_dist) => annotated_dist,
            };
            name_to_markers
                .entry(&annotated_dist.name)
                .or_default()
                .push((&annotated_dist.version, &annotated_dist.marker));
        }
        let mut dupes = vec![];
        for (name, marker_trees) in name_to_markers {
            for (i, (version1, marker1)) in marker_trees.iter().enumerate() {
                for (version2, marker2) in &marker_trees[i + 1..] {
                    if version1 == version2 {
                        continue;
                    }
                    if !marker1.is_disjoint(**marker2) {
                        dupes.push(ConflictingDistributionError {
                            name: name.clone(),
                            version1: (*version1).clone(),
                            version2: (*version2).clone(),
                            marker1: **marker1,
                            marker2: **marker2,
                        });
                    }
                }
            }
        }
        dupes
    }
}

/// An error that occurs for conflicting versions of the same package.
///
/// Specifically, this occurs when two distributions with the same package
/// name are found with distinct versions in at least one possible marker
/// environment. This error reflects an error that could occur when installing
/// the corresponding resolution into that marker environment.
#[derive(Debug)]
pub struct ConflictingDistributionError {
    name: PackageName,
    version1: Version,
    version2: Version,
    marker1: UniversalMarker,
    marker2: UniversalMarker,
}

impl std::error::Error for ConflictingDistributionError {}

impl Display for ConflictingDistributionError {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        let Self {
            ref name,
            ref version1,
            ref version2,
            ref marker1,
            ref marker2,
        } = *self;
        write!(
            f,
            "found conflicting versions for package `{name}`:
             `{marker1:?}` (for version `{version1}`) is not disjoint with \
             `{marker2:?}` (for version `{version2}`)",
        )
    }
}

/// Convert a [`ResolverOutput`] into a [`uv_distribution_types::Resolution`].
///
/// This involves converting [`ResolutionGraphNode`]s into [`Node`]s, which in turn involves
/// dropping any extras and dependency groups from the graph nodes. Instead, each package is
/// collapsed into a single node, with  extras and dependency groups annotating the _edges_, rather
/// than being represented as separate nodes. This is a more natural representation, but a further
/// departure from the PubGrub model.
///
/// For simplicity, this transformation makes the assumption that the resolution only applies to a
/// subset of markers, i.e., it shouldn't be called on universal resolutions, and expects only a
/// single version of each package to be present in the graph.
impl From<ResolverOutput> for uv_distribution_types::Resolution {
    fn from(output: ResolverOutput) -> Self {
        let ResolverOutput {
            graph,
            diagnostics,
            fork_markers,
            ..
        } = output;

        assert!(
            fork_markers.is_empty(),
            "universal resolutions are not supported"
        );

        let mut transformed = Graph::with_capacity(graph.node_count(), graph.edge_count());
        let mut inverse = FxHashMap::with_capacity_and_hasher(graph.node_count(), FxBuildHasher);

        // Create the root node.
        let root = transformed.add_node(Node::Root);

        // Re-add the nodes to the reduced graph.
        for index in graph.node_indices() {
            let ResolutionGraphNode::Dist(dist) = &graph[index] else {
                continue;
            };
            if dist.is_base() {
                inverse.insert(
                    &dist.name,
                    transformed.add_node(Node::Dist {
                        dist: dist.dist.clone(),
                        hashes: dist.hashes.clone(),
                        install: true,
                    }),
                );
            }
        }

        // Re-add the edges to the reduced graph.
        for edge in graph.edge_indices() {
            let (source, target) = graph.edge_endpoints(edge).unwrap();

            match (&graph[source], &graph[target]) {
                (ResolutionGraphNode::Root, ResolutionGraphNode::Dist(target_dist)) => {
                    let target = inverse[&target_dist.name()];
                    transformed.update_edge(root, target, Edge::Prod);
                }
                (
                    ResolutionGraphNode::Dist(source_dist),
                    ResolutionGraphNode::Dist(target_dist),
                ) => {
                    let source = inverse[&source_dist.name()];
                    let target = inverse[&target_dist.name()];

                    let edge = if let Some(extra) = source_dist.extra.as_ref() {
                        Edge::Optional(extra.clone())
                    } else if let Some(dev) = source_dist.dev.as_ref() {
                        Edge::Dev(dev.clone())
                    } else {
                        Edge::Prod
                    };

                    transformed.add_edge(source, target, edge);
                }
                _ => {
                    unreachable!("root should not contain incoming edges");
                }
            }
        }

        Self::new(transformed).with_diagnostics(diagnostics)
    }
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
        for requirement in metadata
            .requires_dist
            .iter()
            // These bounds sources are missing from the graph.
            .chain(metadata.dependency_groups.values().flatten())
            .chain(constraints.requirements())
            .chain(overrides.requirements())
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
