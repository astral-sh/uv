use std::hash::BuildHasherDefault;

use petgraph::{
    graph::{Graph, NodeIndex},
    Directed,
};
use rustc_hash::{FxHashMap, FxHashSet};

use distribution_types::{
    Dist, DistributionMetadata, Name, ResolutionDiagnostic, VersionId, VersionOrUrlRef,
};
use pep440_rs::{Version, VersionSpecifier};
use pep508_rs::{MarkerEnvironment, MarkerTree};
use pypi_types::{ParsedUrlError, Requirement, Yanked};
use uv_git::GitResolver;
use uv_normalize::{ExtraName, PackageName};

use crate::preferences::Preferences;
use crate::pubgrub::{PubGrubDistribution, PubGrubPackageInner};
use crate::redirect::url_to_precise;
use crate::resolution::AnnotatedDist;
use crate::resolver::Resolution;
use crate::{
    lock, InMemoryIndex, Lock, LockError, Manifest, MetadataResponse, ResolveError,
    VersionsResponse,
};

/// A complete resolution graph in which every node represents a pinned package and every edge
/// represents a dependency between two pinned packages.
#[derive(Debug)]
pub struct ResolutionGraph {
    /// The underlying graph.
    pub(crate) petgraph: Graph<AnnotatedDist, Version, Directed>,
    /// Any diagnostics that were encountered while building the graph.
    pub(crate) diagnostics: Vec<ResolutionDiagnostic>,
}

impl ResolutionGraph {
    /// Create a new graph from the resolved PubGrub state.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_state(
        index: &InMemoryIndex,
        preferences: &Preferences,
        git: &GitResolver,
        resolution: Resolution,
    ) -> anyhow::Result<Self, ResolveError> {
        // Collect all marker expressions from relevant pubgrub packages.
        let mut markers: FxHashMap<(&PackageName, &Version, &Option<ExtraName>), MarkerTree> =
            FxHashMap::default();
        for (package, versions) in &resolution.packages {
            if let PubGrubPackageInner::Package {
                name,
                marker: Some(marker),
                extra,
                ..
            } = &**package
            {
                for version in versions {
                    markers
                        .entry((name, version, extra))
                        .or_insert_with(|| MarkerTree::Or(vec![]))
                        .or(marker.clone());
                }
            }
        }

        // Add every package to the graph.
        let mut petgraph: Graph<AnnotatedDist, Version, Directed> =
            Graph::with_capacity(resolution.packages.len(), resolution.packages.len());
        let mut inverse: FxHashMap<(&PackageName, &Version, &Option<ExtraName>), NodeIndex<u32>> =
            FxHashMap::with_capacity_and_hasher(
                resolution.packages.len(),
                BuildHasherDefault::default(),
            );
        let mut diagnostics = Vec::new();

        for (package, versions) in &resolution.packages {
            for version in versions {
                match &**package {
                    PubGrubPackageInner::Package {
                        name,
                        extra,
                        marker: None,
                        url: None,
                    } => {
                        // Create the distribution.
                        let dist = resolution
                            .pins
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

                        // Validate the extra.
                        if let Some(extra) = extra {
                            if !metadata.provides_extras.contains(extra) {
                                diagnostics.push(ResolutionDiagnostic::MissingExtra {
                                    dist: dist.clone(),
                                    extra: extra.clone(),
                                });
                            }
                        }
                        // Extract the markers.
                        let marker = markers.get(&(name, version, extra)).cloned();

                        // Add the distribution to the graph.
                        let index = petgraph.add_node(AnnotatedDist {
                            dist,
                            extra: extra.clone(),
                            marker,
                            hashes,
                            metadata,
                        });
                        inverse.insert((name, version, extra), index);
                    }

                    PubGrubPackageInner::Package {
                        name,
                        extra,
                        marker: None,
                        url: Some(url),
                    } => {
                        // Create the distribution.
                        let dist = Dist::from_url(name.clone(), url_to_precise(url.clone(), git))?;

                        // Extract the hashes, preserving those that were already present in the
                        // lockfile if necessary.
                        let hashes = if let Some(digests) = preferences
                            .match_hashes(name, version)
                            .filter(|digests| !digests.is_empty())
                        {
                            digests.to_vec()
                        } else if let Some(metadata_response) =
                            index.distributions().get(&dist.version_id())
                        {
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

                        // Validate the extra.
                        if let Some(extra) = extra {
                            if !metadata.provides_extras.contains(extra) {
                                diagnostics.push(ResolutionDiagnostic::MissingExtra {
                                    dist: dist.clone().into(),
                                    extra: extra.clone(),
                                });
                            }
                        }
                        // Extract the markers.
                        let marker = markers.get(&(name, version, extra)).cloned();

                        // Add the distribution to the graph.
                        let index = petgraph.add_node(AnnotatedDist {
                            dist: dist.into(),
                            extra: extra.clone(),
                            marker,
                            hashes,
                            metadata,
                        });
                        inverse.insert((name, version, extra), index);
                    }

                    _ => {}
                };
            }
        }

        // Add every edge to the graph.
        for (names, version_set) in resolution.dependencies {
            for versions in version_set {
                let from_index =
                    inverse[&(&names.from, &versions.from_version, &versions.from_extra)];
                let to_index = inverse[&(&names.to, &versions.to_version, &versions.to_extra)];
                petgraph.update_edge(from_index, to_index, versions.to_version.clone());
            }
        }

        Ok(Self {
            petgraph,
            diagnostics,
        })
    }

    /// Return the number of distinct packages in the graph.
    pub fn len(&self) -> usize {
        self.petgraph
            .node_indices()
            .map(|index| &self.petgraph[index])
            .filter(|dist| dist.extra.is_none())
            .count()
    }

    /// Return `true` if there are no packages in the graph.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns `true` if the graph contains the given package.
    pub fn contains(&self, name: &PackageName) -> bool {
        self.petgraph
            .node_indices()
            .any(|index| self.petgraph[index].name() == name)
    }

    /// Return the [`ResolutionDiagnostic`]s that were encountered while building the graph.
    pub fn diagnostics(&self) -> &[ResolutionDiagnostic] {
        &self.diagnostics
    }

    /// Return the marker tree specific to this resolution.
    ///
    /// This accepts a manifest, in-memory-index and marker environment. All
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
        manifest: &Manifest,
        index: &InMemoryIndex,
        marker_env: &MarkerEnvironment,
    ) -> anyhow::Result<pep508_rs::MarkerTree, Box<ParsedUrlError>> {
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
        fn add_marker_params_from_tree(marker_tree: &MarkerTree, set: &mut FxHashSet<MarkerParam>) {
            match marker_tree {
                MarkerTree::Expression(
                    MarkerExpression::Version { key, .. }
                    | MarkerExpression::VersionInverted { key, .. },
                ) => {
                    set.insert(MarkerParam::Version(key.clone()));
                }
                MarkerTree::Expression(
                    MarkerExpression::String { key, .. }
                    | MarkerExpression::StringInverted { key, .. },
                ) => {
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

        let mut seen_marker_values = FxHashSet::default();
        for i in self.petgraph.node_indices() {
            let dist = &self.petgraph[i];
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
            let requirements: Vec<_> = archive
                .metadata
                .requires_dist
                .iter()
                .cloned()
                .map(Requirement::from)
                .collect();
            for req in manifest.apply(requirements.iter()) {
                let Some(ref marker_tree) = req.marker else {
                    continue;
                };
                add_marker_params_from_tree(marker_tree, &mut seen_marker_values);
            }
        }

        // Ensure that we consider markers from direct dependencies.
        for direct_req in manifest.apply(manifest.requirements.iter()) {
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

    pub fn lock(&self) -> anyhow::Result<Lock, LockError> {
        let mut locked_dists = vec![];
        for node_index in self.petgraph.node_indices() {
            let dist = &self.petgraph[node_index];
            let mut locked_dist = lock::Distribution::from_annotated_dist(dist)?;
            for neighbor in self.petgraph.neighbors(node_index) {
                let dependency_dist = &self.petgraph[neighbor];
                locked_dist.add_dependency(dependency_dist);
            }
            locked_dists.push(locked_dist);
        }
        let lock = Lock::new(locked_dists)?;
        Ok(lock)
    }
}

impl From<ResolutionGraph> for distribution_types::Resolution {
    fn from(graph: ResolutionGraph) -> Self {
        Self::new(
            graph
                .petgraph
                .node_indices()
                .map(|node| {
                    (
                        graph.petgraph[node].name().clone(),
                        graph.petgraph[node].dist.clone(),
                    )
                })
                .collect(),
            graph.diagnostics,
        )
    }
}
