use std::hash::BuildHasherDefault;

use colored::Colorize;
use fxhash::FxHashMap;
use petgraph::visit::EdgeRef;
use petgraph::Direction;
use pubgrub::range::Range;
use pubgrub::solver::{Kind, State};
use pubgrub::type_aliases::SelectedDependencies;
use url::Url;
use waitmap::WaitMap;

use distribution_types::{BuiltDist, Dist, Metadata, SourceDist};
use pep440_rs::{Version, VersionSpecifier, VersionSpecifiers};
use pep508_rs::{Requirement, VersionOrUrl};
use puffin_normalize::PackageName;
use pypi_types::File;

use crate::pubgrub::{PubGrubPackage, PubGrubPriority, PubGrubVersion};

/// A set of packages pinned at specific versions.
#[derive(Debug, Default)]
pub struct Resolution(FxHashMap<PackageName, Dist>);

impl Resolution {
    /// Create a new resolution from the given pinned packages.
    pub(crate) fn new(packages: FxHashMap<PackageName, Dist>) -> Self {
        Self(packages)
    }

    /// Return the distribution for the given package name, if it exists.
    pub fn get(&self, package_name: &PackageName) -> Option<&Dist> {
        self.0.get(package_name)
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
}

/// A complete resolution graph in which every node represents a pinned package and every edge
/// represents a dependency between two pinned packages.
#[derive(Debug)]
pub struct Graph(pub petgraph::graph::Graph<Dist, Range<PubGrubVersion>, petgraph::Directed>);

impl Graph {
    /// Create a new graph from the resolved `PubGrub` state.
    pub fn from_state(
        selection: &SelectedDependencies<PubGrubPackage, PubGrubVersion>,
        pins: &FxHashMap<PackageName, FxHashMap<Version, File>>,
        redirects: &WaitMap<Url, Url>,
        state: &State<PubGrubPackage, Range<PubGrubVersion>, PubGrubPriority>,
    ) -> Self {
        // TODO(charlie): petgraph is a really heavy and unnecessary dependency here. We should
        // write our own graph, given that our requirements are so simple.
        let mut graph = petgraph::graph::Graph::with_capacity(selection.len(), selection.len());

        // Add every package to the graph.
        let mut inverse =
            FxHashMap::with_capacity_and_hasher(selection.len(), BuildHasherDefault::default());
        for (package, version) in selection {
            match package {
                PubGrubPackage::Package(package_name, None, None) => {
                    let version = Version::from(version.clone());
                    let file = pins
                        .get(package_name)
                        .and_then(|versions| versions.get(&version))
                        .unwrap()
                        .clone();
                    let pinned_package = Dist::from_registry(package_name.clone(), version, file);

                    let index = graph.add_node(pinned_package);
                    inverse.insert(package_name, index);
                }
                PubGrubPackage::Package(package_name, None, Some(url)) => {
                    let url = redirects
                        .get(url)
                        .map_or_else(|| url.clone(), |url| url.value().clone());
                    let pinned_package = Dist::from_url(package_name.clone(), url);

                    let index = graph.add_node(pinned_package);
                    inverse.insert(package_name, index);
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
                        graph.update_edge(*self_index, *dependency_index, dependency_range.clone());
                    }
                }
            }
        }

        Self(graph)
    }

    /// Return the number of packages in the graph.
    pub fn len(&self) -> usize {
        self.0.node_count()
    }

    /// Return `true` if there are no packages in the graph.
    pub fn is_empty(&self) -> bool {
        self.0.node_count() == 0
    }

    pub fn requirements(&self) -> Vec<Requirement> {
        // Collect and sort all packages.
        let mut nodes = self
            .0
            .node_indices()
            .map(|node| (node, &self.0[node]))
            .collect::<Vec<_>>();
        nodes.sort_unstable_by_key(|(_, package)| package.name());
        self.0
            .node_indices()
            .map(|node| match &self.0[node] {
                Dist::Built(BuiltDist::Registry(wheel)) => Requirement {
                    name: wheel.name.clone(),
                    extras: None,
                    version_or_url: Some(VersionOrUrl::VersionSpecifier(VersionSpecifiers::from(
                        VersionSpecifier::equals_version(wheel.version.clone()),
                    ))),
                    marker: None,
                },
                Dist::Built(BuiltDist::DirectUrl(wheel)) => Requirement {
                    name: wheel.name.clone(),
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
            })
            .collect()
    }

    /// Return the underlying graph.
    pub fn petgraph(
        &self,
    ) -> &petgraph::graph::Graph<Dist, Range<PubGrubVersion>, petgraph::Directed> {
        &self.0
    }
}

/// Write the graph in the `{name}=={version}` format of requirements.txt that pip uses.
impl std::fmt::Display for Graph {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Collect and sort all packages.
        let mut nodes = self
            .0
            .node_indices()
            .map(|node| (node, &self.0[node]))
            .collect::<Vec<_>>();
        nodes.sort_unstable_by_key(|(_, package)| package.name());

        // Print out the dependency graph.
        for (index, package) in nodes {
            writeln!(f, "{package}")?;

            let mut edges = self
                .0
                .edges_directed(index, Direction::Incoming)
                .map(|edge| &self.0[edge.source()])
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
