use distribution_types::{
    Dist, DistributionMetadata, Name, ResolutionDiagnostic, ResolvedDist, VersionId,
    VersionOrUrlRef,
};
use indexmap::IndexSet;
use pep440_rs::{Version, VersionSpecifier};
use pep508_rs::{MarkerEnvironment, MarkerTree, MarkerTreeKind, VerbatimUrl};
use petgraph::{
    graph::{Graph, NodeIndex},
    Directed, Direction,
};
use pypi_types::{HashDigest, ParsedUrlError, Requirement, VerbatimParsedUrl, Yanked};
use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};
use std::fmt::{Display, Formatter};
use uv_configuration::{Constraints, Overrides};
use uv_distribution::Metadata;
use uv_git::GitResolver;
use uv_normalize::{ExtraName, GroupName, PackageName};

use crate::graph_ops::marker_reachability;
use crate::pins::FilePins;
use crate::preferences::Preferences;
use crate::redirect::url_to_precise;
use crate::resolution::AnnotatedDist;
use crate::resolution_mode::ResolutionStrategy;
use crate::resolver::{Resolution, ResolutionDependencyEdge, ResolutionPackage};
use crate::{
    InMemoryIndex, MetadataResponse, Options, PythonRequirement, RequiresPython, ResolveError,
    ResolverMarkers, VersionsResponse,
};

pub(crate) type MarkersForDistribution = FxHashMap<(Version, Option<VerbatimUrl>), Vec<MarkerTree>>;

/// A complete resolution graph in which every node represents a pinned package and every edge
/// represents a dependency between two pinned packages.
#[derive(Debug)]
pub struct ResolutionGraph {
    /// The underlying graph.
    pub(crate) petgraph: Graph<ResolutionGraphNode, MarkerTree, Directed>,
    /// The range of supported Python versions.
    pub(crate) requires_python: RequiresPython,
    /// If the resolution had non-identical forks, store the forks in the lockfile so we can
    /// recreate them in subsequent resolutions.
    pub(crate) fork_markers: Vec<MarkerTree>,
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
    /// If there are multiple options for a package, track which fork they belong to so we
    /// can write that to the lockfile and later get the correct preference per fork back.
    pub(crate) package_markers: FxHashMap<PackageName, MarkersForDistribution>,
}

#[derive(Debug, Clone)]
pub(crate) enum ResolutionGraphNode {
    Root,
    Dist(AnnotatedDist),
}

impl ResolutionGraphNode {
    pub(crate) fn marker(&self) -> &MarkerTree {
        match self {
            ResolutionGraphNode::Root => &MarkerTree::TRUE,
            ResolutionGraphNode::Dist(dist) => &dist.marker,
        }
    }
}

impl Display for ResolutionGraphNode {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolutionGraphNode::Root => f.write_str("root"),
            ResolutionGraphNode::Dist(dist) => Display::fmt(dist, f),
        }
    }
}

#[derive(Eq, PartialEq, Hash)]
struct PackageRef<'a> {
    package_name: &'a PackageName,
    version: &'a Version,
    url: Option<&'a VerbatimParsedUrl>,
    extra: Option<&'a ExtraName>,
    group: Option<&'a GroupName>,
}

impl ResolutionGraph {
    /// Create a new graph from the resolved PubGrub state.
    pub(crate) fn from_state(
        resolutions: &[Resolution],
        requirements: &[Requirement],
        constraints: &Constraints,
        overrides: &Overrides,
        preferences: &Preferences,
        index: &InMemoryIndex,
        git: &GitResolver,
        python: &PythonRequirement,
        resolution_strategy: &ResolutionStrategy,
        options: Options,
    ) -> Result<Self, ResolveError> {
        let size_guess = resolutions[0].nodes.len();
        let mut petgraph: Graph<ResolutionGraphNode, MarkerTree, Directed> =
            Graph::with_capacity(size_guess, size_guess);
        let mut inverse: FxHashMap<PackageRef, NodeIndex<u32>> =
            FxHashMap::with_capacity_and_hasher(size_guess, FxBuildHasher);
        let mut diagnostics = Vec::new();

        // Add the root node.
        let root_index = petgraph.add_node(ResolutionGraphNode::Root);

        let mut package_markers: FxHashMap<PackageName, MarkersForDistribution> =
            FxHashMap::default();

        let mut seen = FxHashSet::default();
        for resolution in resolutions {
            // Add every package to the graph.
            for (package, version) in &resolution.nodes {
                if package.is_base() {
                    // For packages with diverging versions, store which version comes from which
                    // fork.
                    if let Some(markers) = resolution.markers.fork_markers() {
                        package_markers
                            .entry(package.name.clone())
                            .or_default()
                            .entry((version.clone(), package.url.clone().map(|url| url.verbatim)))
                            .or_default()
                            .push(markers.clone());
                    }
                }

                if !seen.insert((package, version)) {
                    // Insert each node only once.
                    continue;
                }
                Self::add_version(
                    &mut petgraph,
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
            let marker = resolution
                .markers
                .fork_markers()
                .cloned()
                .unwrap_or_default();

            // Add every edge to the graph, propagating the marker for the current fork, if
            // necessary.
            for edge in &resolution.edges {
                if !seen.insert((edge, marker.clone())) {
                    // Insert each node only once.
                    continue;
                }

                Self::add_edge(
                    &mut petgraph,
                    &mut inverse,
                    root_index,
                    edge,
                    marker.clone(),
                );
            }
        }

        // Extract the `Requires-Python` range, if provided.
        let requires_python = python.target().clone();

        let fork_markers = if let [resolution] = resolutions {
            match resolution.markers {
                ResolverMarkers::Universal { .. } | ResolverMarkers::SpecificEnvironment(_) => {
                    vec![]
                }
                ResolverMarkers::Fork(_) => {
                    resolutions
                        .iter()
                        .map(|resolution| {
                            resolution
                                .markers
                                .fork_markers()
                                .expect("A non-forking resolution exists in forking mode")
                                .clone()
                        })
                        // Any unsatisfiable forks were skipped.
                        .filter(|fork| !fork.is_false())
                        .collect()
                }
            }
        } else {
            resolutions
                .iter()
                .map(|resolution| {
                    resolution
                        .markers
                        .fork_markers()
                        .expect("A non-forking resolution exists in forking mode")
                        .clone()
                })
                // Any unsatisfiable forks were skipped.
                .filter(|fork| !fork.is_false())
                .collect()
        };

        // Compute and apply the marker reachability.
        let mut reachability = marker_reachability(&petgraph, &fork_markers);

        // Apply the reachability to the graph.
        for index in petgraph.node_indices() {
            if let ResolutionGraphNode::Dist(dist) = &mut petgraph[index] {
                dist.marker = reachability.remove(&index).unwrap_or_default();
            }
        }

        // Discard any unreachable nodes.
        petgraph.retain_nodes(|graph, node| !graph[node].marker().is_false());

        if matches!(resolution_strategy, ResolutionStrategy::Lowest) {
            report_missing_lower_bounds(&petgraph, &mut diagnostics);
        }

        Ok(Self {
            petgraph,
            requires_python,
            package_markers,
            diagnostics,
            requirements: requirements.to_vec(),
            constraints: constraints.clone(),
            overrides: overrides.clone(),
            options,
            fork_markers,
        })
    }

    fn add_edge(
        petgraph: &mut Graph<ResolutionGraphNode, MarkerTree>,
        inverse: &mut FxHashMap<PackageRef<'_>, NodeIndex>,
        root_index: NodeIndex,
        edge: &ResolutionDependencyEdge,
        marker: MarkerTree,
    ) {
        let from_index = edge.from.as_ref().map_or(root_index, |from| {
            inverse[&PackageRef {
                package_name: from,
                version: &edge.from_version,
                url: edge.from_url.as_ref(),
                extra: edge.from_extra.as_ref(),
                group: edge.from_dev.as_ref(),
            }]
        });
        let to_index = inverse[&PackageRef {
            package_name: &edge.to,
            version: &edge.to_version,
            url: edge.to_url.as_ref(),
            extra: edge.to_extra.as_ref(),
            group: edge.to_dev.as_ref(),
        }];

        let edge_marker = {
            let mut edge_marker = edge.marker.clone();
            edge_marker.and(marker);
            edge_marker
        };

        if let Some(marker) = petgraph
            .find_edge(from_index, to_index)
            .and_then(|edge| petgraph.edge_weight_mut(edge))
        {
            // If either the existing marker or new marker is `true`, then the dependency is
            // included unconditionally, and so the combined marker is `true`.
            marker.or(edge_marker);
        } else {
            petgraph.update_edge(from_index, to_index, edge_marker);
        }
    }

    fn add_version<'a>(
        petgraph: &mut Graph<ResolutionGraphNode, MarkerTree>,
        inverse: &mut FxHashMap<PackageRef<'a>, NodeIndex>,
        diagnostics: &mut Vec<ResolutionDiagnostic>,
        preferences: &Preferences,
        pins: &FilePins,
        index: &InMemoryIndex,
        git: &GitResolver,
        package: &'a ResolutionPackage,
        version: &'a Version,
    ) -> Result<(), ResolveError> {
        let ResolutionPackage {
            name,
            extra,
            dev,
            url,
        } = &package;
        // Map the package to a distribution.
        let (dist, hashes, metadata) = Self::parse_dist(
            name,
            url.as_ref(),
            version,
            pins,
            diagnostics,
            preferences,
            index,
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
                if !metadata.dev_dependencies.contains_key(dev) {
                    diagnostics.push(ResolutionDiagnostic::MissingDev {
                        dist: dist.clone(),
                        dev: dev.clone(),
                    });
                }
            }
        }

        // Add the distribution to the graph.
        let index = petgraph.add_node(ResolutionGraphNode::Dist(AnnotatedDist {
            dist,
            name: name.clone(),
            version: version.clone(),
            extra: extra.clone(),
            dev: dev.clone(),
            hashes,
            metadata,
            marker: MarkerTree::TRUE,
        }));
        inverse.insert(
            PackageRef {
                package_name: name,
                version,
                url: url.as_ref(),
                extra: extra.as_ref(),
                group: dev.as_ref(),
            },
            index,
        );
        Ok(())
    }

    fn parse_dist(
        name: &PackageName,
        url: Option<&VerbatimParsedUrl>,
        version: &Version,
        pins: &FilePins,
        diagnostics: &mut Vec<ResolutionDiagnostic>,
        preferences: &Preferences,
        index: &InMemoryIndex,
        git: &GitResolver,
    ) -> Result<(ResolvedDist, Vec<HashDigest>, Option<Metadata>), ResolveError> {
        Ok(if let Some(url) = url {
            // Create the distribution.
            let dist = Dist::from_url(name.clone(), url_to_precise(url.clone(), git))?;

            let version_id = VersionId::from_url(&url.verbatim);

            // Extract the hashes.
            let hashes =
                Self::get_hashes(name, Some(url), &version_id, version, preferences, index);

            // Extract the metadata.
            let metadata = {
                let response = index.distributions().get(&version_id).unwrap_or_else(|| {
                    panic!("Every URL distribution should have metadata: {version_id:?}")
                });

                let MetadataResponse::Found(archive) = &*response else {
                    panic!("Every URL distribution should have metadata: {version_id:?}")
                };

                archive.metadata.clone()
            };

            (dist.into(), hashes, Some(metadata))
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
                        reason: Some(reason.clone()),
                    });
                }
            }

            // Extract the hashes.
            let hashes = Self::get_hashes(name, None, &version_id, version, preferences, index);

            // Extract the metadata.
            let metadata = {
                index.distributions().get(&version_id).and_then(|response| {
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
        url: Option<&VerbatimParsedUrl>,
        version_id: &VersionId,
        version: &Version,
        preferences: &Preferences,
        index: &InMemoryIndex,
    ) -> Vec<HashDigest> {
        // 1. Look for hashes from the lockfile.
        if let Some(digests) = preferences.match_hashes(name, version) {
            if !digests.is_empty() {
                return digests.to_vec();
            }
        }

        // 2. Look for hashes for the distribution (i.e., the specific wheel or source distribution).
        if let Some(metadata_response) = index.distributions().get(version_id) {
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
            if let Some(versions_response) = index.packages().get(name) {
                if let VersionsResponse::Found(ref version_maps) = *versions_response {
                    if let Some(digests) = version_maps
                        .iter()
                        .find_map(|version_map| version_map.hashes(version))
                        .map(|mut digests| {
                            digests.sort_unstable();
                            digests
                        })
                    {
                        if !digests.is_empty() {
                            return digests;
                        }
                    }
                }
            }
        }

        vec![]
    }

    /// Returns an iterator over the distinct packages in the graph.
    fn dists(&self) -> impl Iterator<Item = &AnnotatedDist> {
        self.petgraph
            .node_indices()
            .filter_map(move |index| match &self.petgraph[index] {
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
        use pep508_rs::{
            MarkerExpression, MarkerOperator, MarkerTree, MarkerValueString, MarkerValueVersion,
        };

        /// A subset of the possible marker values.
        ///
        /// We only track the marker parameters that are referenced in a marker
        /// expression. We'll use references to the parameter later to generate
        /// values based on the current marker environment.
        #[derive(Debug, Eq, Hash, PartialEq)]
        enum MarkerParam {
            Version(MarkerValueVersion),
            String(MarkerValueString),
        }

        /// Add all marker parameters from the given tree to the given set.
        fn add_marker_params_from_tree(marker_tree: &MarkerTree, set: &mut IndexSet<MarkerParam>) {
            match marker_tree.kind() {
                MarkerTreeKind::True => {}
                MarkerTreeKind::False => {}
                MarkerTreeKind::Version(marker) => {
                    set.insert(MarkerParam::Version(marker.key().clone()));
                    for (_, tree) in marker.edges() {
                        add_marker_params_from_tree(&tree, set);
                    }
                }
                MarkerTreeKind::String(marker) => {
                    set.insert(MarkerParam::String(marker.key().clone()));
                    for (_, tree) in marker.children() {
                        add_marker_params_from_tree(&tree, set);
                    }
                }
                MarkerTreeKind::In(marker) => {
                    set.insert(MarkerParam::String(marker.key().clone()));
                    for (_, tree) in marker.children() {
                        add_marker_params_from_tree(&tree, set);
                    }
                }
                MarkerTreeKind::Contains(marker) => {
                    set.insert(MarkerParam::String(marker.key().clone()));
                    for (_, tree) in marker.children() {
                        add_marker_params_from_tree(&tree, set);
                    }
                }
                // We specifically don't care about these for the
                // purposes of generating a marker string for a lock
                // file. Quoted strings are marker values given by the
                // user. We don't track those here, since we're only
                // interested in which markers are used.
                MarkerTreeKind::Extra(marker) => {
                    for (_, tree) in marker.children() {
                        add_marker_params_from_tree(&tree, set);
                    }
                }
            }
        }

        let mut seen_marker_values = IndexSet::default();
        for i in self.petgraph.node_indices() {
            let ResolutionGraphNode::Dist(dist) = &self.petgraph[i] else {
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
                add_marker_params_from_tree(&req.marker, &mut seen_marker_values);
            }
        }

        // Ensure that we consider markers from direct dependencies.
        for direct_req in self
            .constraints
            .apply(self.overrides.apply(self.requirements.iter()))
        {
            add_marker_params_from_tree(&direct_req.marker, &mut seen_marker_values);
        }

        // Generate the final marker expression as a conjunction of
        // strict equality terms.
        let mut conjunction = MarkerTree::TRUE;
        for marker_param in seen_marker_values {
            let expr = match marker_param {
                MarkerParam::Version(value_version) => {
                    let from_env = marker_env.get_version(&value_version);
                    MarkerExpression::Version {
                        key: value_version,
                        specifier: VersionSpecifier::equals_version(from_env.clone()),
                    }
                }
                MarkerParam::String(value_string) => {
                    let from_env = marker_env.get_string(&value_string);
                    MarkerExpression::String {
                        key: value_string,
                        operator: MarkerOperator::Equal,
                        value: from_env.to_string(),
                    }
                }
            };
            conjunction.and(MarkerTree::expression(expr));
        }
        Ok(conjunction)
    }

    /// If there are multiple distributions for the same package name, return the markers of the
    /// fork(s) that contained this distribution, otherwise return `None`.
    pub fn fork_markers(
        &self,
        package_name: &PackageName,
        version: &Version,
        url: Option<&VerbatimUrl>,
    ) -> Option<&Vec<MarkerTree>> {
        let package_markers = &self.package_markers.get(package_name)?;
        if package_markers.len() == 1 {
            None
        } else {
            Some(&package_markers[&(version.clone(), url.cloned())])
        }
    }
}

impl From<ResolutionGraph> for distribution_types::Resolution {
    fn from(graph: ResolutionGraph) -> Self {
        Self::new(
            graph
                .dists()
                .map(|node| (node.name().clone(), node.dist.clone()))
                .collect(),
            graph
                .dists()
                .map(|node| (node.name().clone(), node.hashes.clone()))
                .collect(),
            graph.diagnostics,
        )
    }
}

/// Find any packages that don't have any lower bound on them when in resolution-lowest mode.
fn report_missing_lower_bounds(
    petgraph: &Graph<ResolutionGraphNode, MarkerTree>,
    diagnostics: &mut Vec<ResolutionDiagnostic>,
) {
    for node_index in petgraph.node_indices() {
        let ResolutionGraphNode::Dist(dist) = petgraph.node_weight(node_index).unwrap() else {
            // Ignore the root package.
            continue;
        };
        if dist.dev.is_some() {
            // TODO(konsti): Dev dependencies are modelled incorrectly in the graph. There should
            // be an edge from root to project-with-dev, just like to project-with-extra, but
            // currently there is only an edge from project to to project-with-dev that we then
            // have to drop.
            continue;
        }
        if !has_lower_bound(node_index, dist.name(), petgraph) {
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
    petgraph: &Graph<ResolutionGraphNode, MarkerTree>,
) -> bool {
    for neighbor_index in petgraph.neighbors_directed(node_index, Direction::Incoming) {
        let neighbor_dist = match petgraph.node_weight(neighbor_index).unwrap() {
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
            .chain(metadata.dev_dependencies.values().flatten())
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
