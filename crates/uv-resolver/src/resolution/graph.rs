use std::hash::BuildHasherDefault;
use std::sync::Arc;

use pubgrub::range::Range;
use pubgrub::solver::{Kind, State};
use pubgrub::type_aliases::SelectedDependencies;
use rustc_hash::{FxHashMap, FxHashSet};

use distribution_types::{
    Dist, DistributionMetadata, Name, ParsedUrlError, Requirement, ResolvedDist, VersionId,
    VersionOrUrlRef,
};
use pep440_rs::{Version, VersionSpecifier};
use pep508_rs::MarkerEnvironment;
use uv_normalize::{ExtraName, PackageName};

use crate::dependency_provider::UvDependencyProvider;
use crate::editables::Editables;
use crate::pins::FilePins;
use crate::preferences::Preferences;
use crate::pubgrub::{PubGrubDistribution, PubGrubPackage};
use crate::redirect::url_to_precise;
use crate::resolution::AnnotatedDist;
use crate::resolver::FxOnceMap;
use crate::{
    lock, InMemoryIndex, Lock, LockError, Manifest, MetadataResponse, ResolveError,
    VersionsResponse,
};

/// A complete resolution graph in which every node represents a pinned package and every edge
/// represents a dependency between two pinned packages.
#[derive(Debug)]
pub struct ResolutionGraph {
    /// The underlying graph.
    pub(crate) petgraph: petgraph::graph::Graph<AnnotatedDist, Range<Version>, petgraph::Directed>,
    /// The set of editable requirements in this resolution.
    pub(crate) editables: Editables,
    /// Any diagnostics that were encountered while building the graph.
    pub(crate) diagnostics: Vec<Diagnostic>,
}

impl ResolutionGraph {
    /// Create a new graph from the resolved PubGrub state.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_state(
        selection: &SelectedDependencies<UvDependencyProvider>,
        pins: &FilePins,
        packages: &FxOnceMap<PackageName, Arc<VersionsResponse>>,
        distributions: &FxOnceMap<VersionId, Arc<MetadataResponse>>,
        state: &State<UvDependencyProvider>,
        preferences: &Preferences,
        editables: Editables,
    ) -> anyhow::Result<Self, ResolveError> {
        // Collect and validate the extras.
        let mut extras = FxHashMap::default();
        let mut diagnostics = Vec::new();
        for (package, version) in selection {
            match package {
                PubGrubPackage::Package(package_name, Some(extra), None) => {
                    let dist = PubGrubDistribution::from_registry(package_name, version);

                    let response = distributions.get(&dist.version_id()).unwrap_or_else(|| {
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

                    if archive.metadata.provides_extras.contains(extra) {
                        extras
                            .entry(package_name.clone())
                            .or_insert_with(Vec::new)
                            .push(extra.clone());
                    } else {
                        let dist = pins
                            .get(package_name, version)
                            .unwrap_or_else(|| {
                                panic!("Every package should be pinned: {package_name:?}")
                            })
                            .clone();

                        diagnostics.push(Diagnostic::MissingExtra {
                            dist,
                            extra: extra.clone(),
                        });
                    }
                }
                PubGrubPackage::Package(package_name, Some(extra), Some(url)) => {
                    if let Some(editable) = editables.get(package_name) {
                        if editable.metadata.provides_extras.contains(extra) {
                            extras
                                .entry(package_name.clone())
                                .or_insert_with(Vec::new)
                                .push(extra.clone());
                        } else {
                            let dist =
                                Dist::from_editable(package_name.clone(), editable.built.clone())?;

                            diagnostics.push(Diagnostic::MissingExtra {
                                dist: dist.into(),
                                extra: extra.clone(),
                            });
                        }
                    } else {
                        let dist = PubGrubDistribution::from_url(package_name, url);

                        let response = distributions.get(&dist.version_id()).unwrap_or_else(|| {
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

                        if archive.metadata.provides_extras.contains(extra) {
                            extras
                                .entry(package_name.clone())
                                .or_insert_with(Vec::new)
                                .push(extra.clone());
                        } else {
                            let dist =
                                Dist::from_url(package_name.clone(), url_to_precise(url.clone()))?;

                            diagnostics.push(Diagnostic::MissingExtra {
                                dist: dist.into(),
                                extra: extra.clone(),
                            });
                        }
                    }
                }
                _ => {}
            };
        }

        // Add every package to the graph.
        // TODO(charlie): petgraph is a really heavy and unnecessary dependency here. We should
        // write our own graph, given that our requirements are so simple.
        let mut petgraph = petgraph::graph::Graph::with_capacity(selection.len(), selection.len());
        let mut inverse =
            FxHashMap::with_capacity_and_hasher(selection.len(), BuildHasherDefault::default());

        for (package, version) in selection {
            match package {
                PubGrubPackage::Package(package_name, None, None) => {
                    // Create the distribution.
                    let dist = pins
                        .get(package_name, version)
                        .expect("Every package should be pinned")
                        .clone();

                    // Extract the hashes, preserving those that were already present in the
                    // lockfile if necessary.
                    let hashes = if let Some(digests) = preferences
                        .match_hashes(package_name, version)
                        .filter(|digests| !digests.is_empty())
                    {
                        digests.to_vec()
                    } else if let Some(versions_response) = packages.get(package_name) {
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
                        let dist = PubGrubDistribution::from_registry(package_name, version);

                        let response = distributions.get(&dist.version_id()).unwrap_or_else(|| {
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

                    // Extract the extras.
                    let extras = extras.get(package_name).cloned().unwrap_or_default();

                    // Add the distribution to the graph.
                    let index = petgraph.add_node(AnnotatedDist {
                        dist,
                        extras,
                        hashes,
                        metadata,
                    });
                    inverse.insert(package_name, index);
                }
                PubGrubPackage::Package(package_name, None, Some(url)) => {
                    // Create the distribution.
                    if let Some(editable) = editables.get(package_name) {
                        let dist =
                            Dist::from_editable(package_name.clone(), editable.built.clone())?;

                        // Add the distribution to the graph.
                        let index = petgraph.add_node(AnnotatedDist {
                            dist: dist.into(),
                            extras: editable.built.extras.clone(),
                            hashes: vec![],
                            metadata: editable.metadata.clone(),
                        });
                        inverse.insert(package_name, index);
                    } else {
                        let dist =
                            Dist::from_url(package_name.clone(), url_to_precise(url.clone()))?;

                        // Extract the hashes, preserving those that were already present in the
                        // lockfile if necessary.
                        let hashes = if let Some(digests) = preferences
                            .match_hashes(package_name, version)
                            .filter(|digests| !digests.is_empty())
                        {
                            digests.to_vec()
                        } else if let Some(metadata_response) =
                            distributions.get(&dist.version_id())
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
                            let dist = PubGrubDistribution::from_url(package_name, url);

                            let response =
                                distributions.get(&dist.version_id()).unwrap_or_else(|| {
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

                        // Extract the extras.
                        let extras = extras.get(package_name).cloned().unwrap_or_default();

                        // Add the distribution to the graph.
                        let index = petgraph.add_node(AnnotatedDist {
                            dist: dist.into(),
                            extras,
                            hashes,
                            metadata,
                        });
                        inverse.insert(package_name, index);
                    };
                }
                _ => {}
            };
        }

        // Add every edge to the graph.
        for (package, version) in selection {
            for id in &state.incompatibilities[package] {
                if let Kind::FromDependencyOf(
                    self_package,
                    self_version,
                    dependency_package,
                    dependency_range,
                ) = &state.incompatibility_store[*id].kind
                {
                    // `Kind::FromDependencyOf` will include inverse dependencies. That is, if we're
                    // looking for a package `A`, this list will include incompatibilities of
                    // package `B` _depending on_ `A`. We're only interested in packages that `A`
                    // depends on.
                    if package != self_package {
                        continue;
                    }

                    let PubGrubPackage::Package(self_package, _, _) = self_package else {
                        continue;
                    };
                    let PubGrubPackage::Package(dependency_package, _, _) = dependency_package
                    else {
                        continue;
                    };

                    // For extras, we include a dependency between the extra and the base package.
                    if self_package == dependency_package {
                        continue;
                    }

                    if self_version.contains(version) {
                        let self_index = &inverse[self_package];
                        let dependency_index = &inverse[dependency_package];
                        petgraph.update_edge(
                            *self_index,
                            *dependency_index,
                            dependency_range.clone(),
                        );
                    }
                }
            }
        }

        Ok(Self {
            petgraph,
            editables,
            diagnostics,
        })
    }

    /// Return the number of packages in the graph.
    pub fn len(&self) -> usize {
        self.petgraph.node_count()
    }

    /// Return `true` if there are no packages in the graph.
    pub fn is_empty(&self) -> bool {
        self.petgraph.node_count() == 0
    }

    /// Returns `true` if the graph contains the given package.
    pub fn contains(&self, name: &PackageName) -> bool {
        self.petgraph
            .node_indices()
            .any(|index| self.petgraph[index].name() == name)
    }

    /// Iterate over the [`ResolvedDist`] entities in this resolution.
    pub fn into_distributions(self) -> impl Iterator<Item = ResolvedDist> {
        self.petgraph
            .into_nodes_edges()
            .0
            .into_iter()
            .map(|node| node.weight.dist)
    }

    /// Return the [`Diagnostic`]s that were encountered while building the graph.
    pub fn diagnostics(&self) -> &[Diagnostic] {
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
                .map(Requirement::from_pep508)
                .collect::<anyhow::Result<_, _>>()?;
            for req in manifest.apply(requirements.iter()) {
                let Some(ref marker_tree) = req.marker else {
                    continue;
                };
                add_marker_params_from_tree(marker_tree, &mut seen_marker_values);
            }
        }

        // Ensure that we consider markers from direct dependencies.
        let direct_reqs = manifest.requirements.iter().chain(
            manifest
                .editables
                .iter()
                .flat_map(|editable| &editable.requirements.dependencies),
        );
        for direct_req in manifest.apply(direct_reqs) {
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
            for edge in self.petgraph.neighbors(node_index) {
                let dependency_dist = &self.petgraph[edge];
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
        )
    }
}

#[derive(Debug)]
pub enum Diagnostic {
    MissingExtra {
        /// The distribution that was requested with an non-existent extra. For example,
        /// `black==23.10.0`.
        dist: ResolvedDist,
        /// The extra that was requested. For example, `colorama` in `black[colorama]`.
        extra: ExtraName,
    },
}

impl Diagnostic {
    /// Convert the diagnostic into a user-facing message.
    pub fn message(&self) -> String {
        match self {
            Self::MissingExtra { dist, extra } => {
                format!("The package `{dist}` does not have an extra named `{extra}`.")
            }
        }
    }

    /// Returns `true` if the [`PackageName`] is involved in this diagnostic.
    pub fn includes(&self, name: &PackageName) -> bool {
        match self {
            Self::MissingExtra { dist, .. } => name == dist.name(),
        }
    }
}
