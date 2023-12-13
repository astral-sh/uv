use std::hash::BuildHasherDefault;

use anyhow::Result;
use colored::Colorize;
use itertools::Itertools;
use petgraph::visit::EdgeRef;
use petgraph::Direction;
use pubgrub::range::Range;
use pubgrub::solver::{Kind, State};
use pubgrub::type_aliases::SelectedDependencies;
use rustc_hash::FxHashMap;
use url::Url;

use distribution_types::{BuiltDist, Dist, Metadata, SourceDist};
use pep440_rs::{Version, VersionSpecifier, VersionSpecifiers};
use pep508_rs::{Requirement, VersionOrUrl};
use puffin_normalize::{ExtraName, PackageName};
use puffin_traits::OnceMap;
use pypi_types::Metadata21;

use crate::pins::FilePins;
use crate::pubgrub::{PubGrubDistribution, PubGrubPackage, PubGrubPriority, PubGrubVersion};
use crate::ResolveError;

/// A set of packages pinned at specific versions.
#[derive(Debug, Default)]
pub struct ResolutionManifest(FxHashMap<PackageName, Dist>);

impl ResolutionManifest {
    /// Create a new resolution from the given pinned packages.
    pub(crate) fn new(packages: FxHashMap<PackageName, Dist>) -> Self {
        Self(packages)
    }

    /// Return the distribution for the given package name, if it exists.
    pub fn get(&self, package_name: &PackageName) -> Option<&Dist> {
        self.0.get(package_name)
    }

    /// Iterate over the [`PackageName`] entities in this resolution.
    pub fn packages(&self) -> impl Iterator<Item = &PackageName> {
        self.0.keys()
    }

    /// Iterate over the [`Dist`] entities in this resolution.
    pub fn into_distributions(self) -> impl Iterator<Item = Dist> {
        self.0.into_values()
    }

    /// Return the number of distributions in this resolution.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Return `true` if there are no pinned packages in this resolution.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Return the set of [`Requirement`]s that this resolution represents.
    pub fn requirements(&self) -> Vec<Requirement> {
        self.0
            .values()
            .sorted_by_key(|package| package.name())
            .map(as_requirement)
            .collect()
    }
}

/// A complete resolution graph in which every node represents a pinned package and every edge
/// represents a dependency between two pinned packages.
#[derive(Debug)]
pub struct ResolutionGraph {
    /// The underlying graph.
    petgraph: petgraph::graph::Graph<Dist, Range<PubGrubVersion>, petgraph::Directed>,
    /// Any diagnostics that were encountered while building the graph.
    diagnostics: Vec<Diagnostic>,
}

impl ResolutionGraph {
    /// Create a new graph from the resolved `PubGrub` state.
    pub(crate) fn from_state(
        selection: &SelectedDependencies<PubGrubPackage, PubGrubVersion>,
        pins: &FilePins,
        distributions: &OnceMap<String, Metadata21>,
        redirects: &OnceMap<Url, Url>,
        state: &State<PubGrubPackage, Range<PubGrubVersion>, PubGrubPriority>,
    ) -> Result<Self, ResolveError> {
        // TODO(charlie): petgraph is a really heavy and unnecessary dependency here. We should
        // write our own graph, given that our requirements are so simple.
        let mut petgraph = petgraph::graph::Graph::with_capacity(selection.len(), selection.len());
        let mut diagnostics = Vec::new();

        // Add every package to the graph.
        let mut inverse =
            FxHashMap::with_capacity_and_hasher(selection.len(), BuildHasherDefault::default());
        for (package, version) in selection {
            match package {
                PubGrubPackage::Package(package_name, None, None) => {
                    let version = Version::from(version.clone());
                    let (index, file) = pins
                        .get(package_name, &version)
                        .expect("Every package should be pinned")
                        .clone();
                    let pinned_package =
                        Dist::from_registry(package_name.clone(), version, file, index);

                    let index = petgraph.add_node(pinned_package);
                    inverse.insert(package_name, index);
                }
                PubGrubPackage::Package(package_name, None, Some(url)) => {
                    let url = redirects
                        .get(url)
                        .map_or_else(|| url.clone(), |url| url.value().clone());
                    let pinned_package = Dist::from_url(package_name.clone(), url)?;

                    let index = petgraph.add_node(pinned_package);
                    inverse.insert(package_name, index);
                }
                PubGrubPackage::Package(package_name, Some(extra), None) => {
                    // Validate that the `extra` exists.
                    let dist = PubGrubDistribution::from_registry(package_name, version);
                    let entry = distributions
                        .get(&dist.package_id())
                        .expect("Every package should have metadata");
                    let metadata = entry.value();

                    if !metadata.provides_extras.contains(extra) {
                        let version = Version::from(version.clone());
                        let (index, file) = pins
                            .get(package_name, &version)
                            .expect("Every package should be pinned")
                            .clone();
                        let pinned_package =
                            Dist::from_registry(package_name.clone(), version, file, index);

                        diagnostics.push(Diagnostic::MissingExtra {
                            dist: pinned_package,
                            extra: extra.clone(),
                        });
                    }
                }
                PubGrubPackage::Package(package_name, Some(extra), Some(url)) => {
                    // Validate that the `extra` exists.
                    let dist = PubGrubDistribution::from_url(package_name, url);
                    let entry = distributions
                        .get(&dist.package_id())
                        .expect("Every package should have metadata");
                    let metadata = entry.value();

                    if !metadata.provides_extras.contains(extra) {
                        let url = redirects
                            .get(url)
                            .map_or_else(|| url.clone(), |url| url.value().clone());
                        let pinned_package = Dist::from_url(package_name.clone(), url)?;

                        diagnostics.push(Diagnostic::MissingExtra {
                            dist: pinned_package,
                            extra: extra.clone(),
                        });
                    }
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
                    let PubGrubPackage::Package(self_package, None, _) = self_package else {
                        continue;
                    };
                    let PubGrubPackage::Package(dependency_package, None, _) = dependency_package
                    else {
                        continue;
                    };

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

    /// Return the set of [`Requirement`]s that this graph represents.
    pub fn requirements(&self) -> Vec<Requirement> {
        // Collect and sort all packages.
        let mut nodes = self
            .petgraph
            .node_indices()
            .map(|node| (node, &self.petgraph[node]))
            .collect::<Vec<_>>();
        nodes.sort_unstable_by_key(|(_, package)| package.name());
        self.petgraph
            .node_indices()
            .map(|node| as_requirement(&self.petgraph[node]))
            .collect()
    }

    /// Return the [`Diagnostic`]s that were encountered while building the graph.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Return the underlying graph.
    pub fn petgraph(
        &self,
    ) -> &petgraph::graph::Graph<Dist, Range<PubGrubVersion>, petgraph::Directed> {
        &self.petgraph
    }
}

/// Write the graph in the `{name}=={version}` format of requirements.txt that pip uses.
impl std::fmt::Display for ResolutionGraph {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Collect and sort all packages.
        let mut nodes = self
            .petgraph
            .node_indices()
            .map(|node| (node, &self.petgraph[node]))
            .collect::<Vec<_>>();
        nodes.sort_unstable_by_key(|(_, package)| package.name());

        // Print out the dependency graph.
        for (index, package) in nodes {
            writeln!(f, "{package}")?;

            let mut edges = self
                .petgraph
                .edges_directed(index, Direction::Incoming)
                .map(|edge| &self.petgraph[edge.source()])
                .collect::<Vec<_>>();
            edges.sort_unstable_by_key(|package| package.name());

            match edges.len() {
                0 => {}
                1 => {
                    for dependency in edges {
                        writeln!(f, "{}", format!("    # via {}", dependency.name()).green())?;
                    }
                }
                _ => {
                    writeln!(f, "{}", "    # via".green())?;
                    for dependency in edges {
                        writeln!(f, "{}", format!("    #   {}", dependency.name()).green())?;
                    }
                }
            }
        }

        Ok(())
    }
}

impl From<ResolutionGraph> for ResolutionManifest {
    fn from(graph: ResolutionGraph) -> Self {
        Self(
            graph
                .petgraph
                .node_indices()
                .map(|node| {
                    (
                        graph.petgraph[node].name().clone(),
                        graph.petgraph[node].clone(),
                    )
                })
                .collect(),
        )
    }
}

/// Create a [`Requirement`] from a [`Dist`].
fn as_requirement(dist: &Dist) -> Requirement {
    match dist {
        Dist::Built(BuiltDist::Registry(wheel)) => Requirement {
            name: wheel.name.clone(),
            extras: None,
            version_or_url: Some(VersionOrUrl::VersionSpecifier(VersionSpecifiers::from(
                VersionSpecifier::equals_version(wheel.version.clone()),
            ))),
            marker: None,
        },
        Dist::Built(BuiltDist::DirectUrl(wheel)) => Requirement {
            name: wheel.filename.name.clone(),
            extras: None,
            version_or_url: Some(VersionOrUrl::Url(wheel.url.clone())),
            marker: None,
        },
        Dist::Built(BuiltDist::Path(wheel)) => Requirement {
            name: wheel.filename.name.clone(),
            extras: None,
            version_or_url: Some(VersionOrUrl::Url(wheel.url.clone())),
            marker: None,
        },
        Dist::Source(SourceDist::Registry(sdist)) => Requirement {
            name: sdist.name.clone(),
            extras: None,
            version_or_url: Some(VersionOrUrl::VersionSpecifier(VersionSpecifiers::from(
                VersionSpecifier::equals_version(sdist.version.clone()),
            ))),
            marker: None,
        },
        Dist::Source(SourceDist::DirectUrl(sdist)) => Requirement {
            name: sdist.name.clone(),
            extras: None,
            version_or_url: Some(VersionOrUrl::Url(sdist.url.clone())),
            marker: None,
        },
        Dist::Source(SourceDist::Git(sdist)) => Requirement {
            name: sdist.name.clone(),
            extras: None,
            version_or_url: Some(VersionOrUrl::Url(sdist.url.clone())),
            marker: None,
        },
        Dist::Source(SourceDist::Path(sdist)) => Requirement {
            name: sdist.name.clone(),
            extras: None,
            version_or_url: Some(VersionOrUrl::Url(sdist.url.clone())),
            marker: None,
        },
    }
}

#[derive(Debug)]
pub enum Diagnostic {
    MissingExtra {
        /// The distribution that was requested with an non-existent extra. For example,
        /// `black==23.10.0`.
        dist: Dist,
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
