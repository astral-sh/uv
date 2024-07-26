use distribution_types::{
    Dist, DistributionMetadata, Name, ResolutionDiagnostic, ResolvedDist, VersionId,
    VersionOrUrlRef,
};
use indexmap::IndexSet;
use pep440_rs::{Version, VersionSpecifier};
use pep508_rs::{MarkerEnvironment, MarkerTree};
use petgraph::{
    graph::{Graph, NodeIndex},
    Directed,
};
use pypi_types::{HashDigest, ParsedUrlError, Requirement, VerbatimParsedUrl, Yanked};
use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};
use uv_configuration::{Constraints, Overrides};
use uv_distribution::Metadata;
use uv_git::GitResolver;
use uv_normalize::{ExtraName, GroupName, PackageName};

use crate::pins::FilePins;
use crate::preferences::Preferences;
use crate::pubgrub::PubGrubDistribution;
use crate::python_requirement::PythonTarget;
use crate::redirect::url_to_precise;
use crate::resolution::AnnotatedDist;
use crate::resolver::{Resolution, ResolutionDependencyEdge, ResolutionPackage};
use crate::{
    InMemoryIndex, MetadataResponse, Options, PythonRequirement, RequiresPython, ResolveError,
    VersionsResponse,
};

/// A complete resolution graph in which every node represents a pinned package and every edge
/// represents a dependency between two pinned packages.
#[derive(Debug)]
pub struct ResolutionGraph {
    /// The underlying graph.
    pub(crate) petgraph: Graph<ResolutionGraphNode, Option<MarkerTree>, Directed>,
    /// The range of supported Python versions.
    pub(crate) requires_python: Option<RequiresPython>,
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

#[derive(Debug)]
pub(crate) enum ResolutionGraphNode {
    Root,
    Dist(AnnotatedDist),
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
        options: Options,
    ) -> Result<Self, ResolveError> {
        let size_guess = resolutions[0].nodes.len();
        let mut petgraph: Graph<ResolutionGraphNode, Option<MarkerTree>, Directed> =
            Graph::with_capacity(size_guess, size_guess);
        let mut inverse: FxHashMap<PackageRef, NodeIndex<u32>> =
            FxHashMap::with_capacity_and_hasher(size_guess, FxBuildHasher);
        let mut diagnostics = Vec::new();

        // Add the root node.
        let root_index = petgraph.add_node(ResolutionGraphNode::Root);

        let mut seen = FxHashSet::default();
        for resolution in resolutions {
            // Add every package to the graph.
            for (package, version) in &resolution.nodes {
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
            // Add every edge to the graph.
            for edge in &resolution.edges {
                if !seen.insert(edge) {
                    // Insert each node only once.
                    continue;
                }

                Self::add_edge(&mut petgraph, &mut inverse, root_index, edge);
            }
        }

        // Extract the `Requires-Python` range, if provided.
        // TODO(charlie): Infer the supported Python range from the `Requires-Python` of the
        // included packages.
        let requires_python = python
            .target()
            .and_then(PythonTarget::as_requires_python)
            .cloned();

        // Normalize any markers.
        for edge in petgraph.edge_indices() {
            if let Some(marker) = petgraph[edge].take() {
                petgraph[edge] = crate::marker::normalize(
                    marker,
                    requires_python.as_ref().map(RequiresPython::bound),
                );
            }
        }

        Ok(Self {
            petgraph,
            requires_python,
            diagnostics,
            requirements: requirements.to_vec(),
            constraints: constraints.clone(),
            overrides: overrides.clone(),
            options,
        })
    }

    fn add_edge(
        petgraph: &mut Graph<ResolutionGraphNode, Option<MarkerTree>>,
        inverse: &mut FxHashMap<PackageRef<'_>, NodeIndex>,
        root_index: NodeIndex,
        edge: &ResolutionDependencyEdge,
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

        if let Some(marker) = petgraph
            .find_edge(from_index, to_index)
            .and_then(|edge| petgraph.edge_weight_mut(edge))
        {
            // If either the existing marker or new marker is `None`, then the dependency is
            // included unconditionally, and so the combined marker should be `None`.
            if let (Some(marker), Some(ref version_marker)) = (marker.as_mut(), edge.marker.clone())
            {
                marker.or(version_marker.clone());
            } else {
                *marker = None;
            }
        } else {
            petgraph.update_edge(from_index, to_index, edge.marker.clone());
        }
    }

    fn add_version<'a>(
        petgraph: &mut Graph<ResolutionGraphNode, Option<MarkerTree>>,
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
            url,
            version,
            pins,
            diagnostics,
            preferences,
            index,
            git,
        )?;

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

        // Add the distribution to the graph.
        let index = petgraph.add_node(ResolutionGraphNode::Dist(AnnotatedDist {
            dist,
            version: version.clone(),
            extra: extra.clone(),
            dev: dev.clone(),
            hashes,
            metadata,
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
        url: &Option<VerbatimParsedUrl>,
        version: &Version,
        pins: &FilePins,
        diagnostics: &mut Vec<ResolutionDiagnostic>,
        preferences: &Preferences,
        index: &InMemoryIndex,
        git: &GitResolver,
    ) -> Result<(ResolvedDist, Vec<HashDigest>, Metadata), ResolveError> {
        Ok(if let Some(url) = url {
            // Create the distribution.
            let dist = Dist::from_url(name.clone(), url_to_precise(url.clone(), git))?;

            // Extract the hashes, preserving those that were already present in the
            // lockfile if necessary.
            let hashes = if let Some(digests) = preferences
                .match_hashes(name, version)
                .filter(|digests| !digests.is_empty())
            {
                digests.to_vec()
            } else if let Some(metadata_response) = index.distributions().get(&dist.version_id()) {
                if let MetadataResponse::Found(ref archive) = *metadata_response {
                    let mut digests = archive.hashes.clone();
                    digests.sort_unstable();
                    digests
                } else {
                    vec![]
                }
            } else {
                vec![]
            };

            // Extract the metadata.
            let metadata = {
                let dist = PubGrubDistribution::from_url(name, url);

                let response = index
                    .distributions()
                    .get(&dist.version_id())
                    .unwrap_or_else(|| {
                        panic!(
                            "Every package should have metadata: {:?}",
                            dist.version_id()
                        )
                    });

                let MetadataResponse::Found(archive) = &*response else {
                    panic!(
                        "Every package should have metadata: {:?}",
                        dist.version_id()
                    )
                };

                archive.metadata.clone()
            };

            (dist.into(), hashes, metadata)
        } else {
            let dist = pins
                .get(name, version)
                .expect("Every package should be pinned")
                .clone();

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

            // Extract the hashes, preserving those that were already present in the
            // lockfile if necessary.
            let hashes = if let Some(digests) = preferences
                .match_hashes(name, version)
                .filter(|digests| !digests.is_empty())
            {
                digests.to_vec()
            } else if let Some(versions_response) = index.packages().get(name) {
                if let VersionsResponse::Found(ref version_maps) = *versions_response {
                    version_maps
                        .iter()
                        .find_map(|version_map| version_map.hashes(version))
                        .map(|mut digests| {
                            digests.sort_unstable();
                            digests
                        })
                        .unwrap_or_default()
                } else {
                    vec![]
                }
            } else {
                vec![]
            };

            // Extract the metadata.
            let metadata = {
                let dist = PubGrubDistribution::from_registry(name, version);

                let response = index
                    .distributions()
                    .get(&dist.version_id())
                    .unwrap_or_else(|| {
                        panic!(
                            "Every package should have metadata: {:?}",
                            dist.version_id()
                        )
                    });

                let MetadataResponse::Found(archive) = &*response else {
                    panic!(
                        "Every package should have metadata: {:?}",
                        dist.version_id()
                    )
                };

                archive.metadata.clone()
            };

            (dist, hashes, metadata)
        })
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
            match marker_tree {
                MarkerTree::Expression(MarkerExpression::Version { key, .. }) => {
                    set.insert(MarkerParam::Version(key.clone()));
                }
                MarkerTree::Expression(MarkerExpression::String { key, .. }) => {
                    set.insert(MarkerParam::String(key.clone()));
                }
                MarkerTree::And(ref exprs) | MarkerTree::Or(ref exprs) => {
                    for expr in exprs {
                        add_marker_params_from_tree(expr, set);
                    }
                }
                // We specifically don't care about these for the
                // purposes of generating a marker string for a lock
                // file. Quoted strings are marker values given by the
                // user. We don't track those here, since we're only
                // interested in which markers are used.
                MarkerTree::Expression(
                    MarkerExpression::Extra { .. } | MarkerExpression::Arbitrary { .. },
                ) => {}
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
                let Some(ref marker_tree) = req.marker else {
                    continue;
                };
                add_marker_params_from_tree(marker_tree, &mut seen_marker_values);
            }
        }

        // Ensure that we consider markers from direct dependencies.
        for direct_req in self
            .constraints
            .apply(self.overrides.apply(self.requirements.iter()))
        {
            let Some(ref marker_tree) = direct_req.marker else {
                continue;
            };
            add_marker_params_from_tree(marker_tree, &mut seen_marker_values);
        }

        // Generate the final marker expression as a conjunction of
        // strict equality terms.
        let mut conjuncts = vec![];
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
            conjuncts.push(MarkerTree::Expression(expr));
        }
        Ok(MarkerTree::And(conjuncts))
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
