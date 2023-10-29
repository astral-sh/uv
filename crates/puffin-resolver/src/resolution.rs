use std::hash::BuildHasherDefault;

use colored::Colorize;
use fxhash::FxHashMap;
use petgraph::visit::EdgeRef;
use pubgrub::range::Range;
use pubgrub::solver::{Kind, State};
use pubgrub::type_aliases::SelectedDependencies;

use pep440_rs::{Version, VersionSpecifier, VersionSpecifiers};
use pep508_rs::{Requirement, VersionOrUrl};
use puffin_client::File;
use puffin_package::package_name::PackageName;

use crate::pubgrub::package::PubGrubPackage;
use crate::pubgrub::priority::PubGrubPriority;
use crate::pubgrub::version::PubGrubVersion;

/// A package pinned at a specific version.
#[derive(Debug)]
pub struct PinnedPackage {
    name: PackageName,
    version: Version,
    file: File,
}

impl PinnedPackage {
    /// Initialize a new pinned package.
    pub fn new(name: PackageName, version: Version, file: File) -> Self {
        Self {
            name,
            version,
            file,
        }
    }

    /// Return the name of the pinned package.
    pub fn name(&self) -> &PackageName {
        &self.name
    }

    /// Return the version of the pinned package.
    pub fn version(&self) -> &Version {
        &self.version
    }

    /// Return the file of the pinned package.
    pub fn file(&self) -> &File {
        &self.file
    }
}

/// A set of packages pinned at specific versions.
#[derive(Debug, Default)]
pub struct Resolution(FxHashMap<PackageName, PinnedPackage>);

impl Resolution {
    /// Create a new resolution from the given pinned packages.
    pub(crate) fn new(packages: FxHashMap<PackageName, PinnedPackage>) -> Self {
        Self(packages)
    }

    /// Iterate over the pinned packages in this resolution.
    pub fn iter(&self) -> impl Iterator<Item = (&PackageName, &PinnedPackage)> {
        self.0.iter()
    }

    /// Iterate over the wheels in this resolution.
    pub fn into_files(self) -> impl Iterator<Item = File> {
        self.0.into_values().map(|package| package.file)
    }

    /// Return the pinned package for the given package name, if it exists.
    pub fn get(&self, package_name: &PackageName) -> Option<&PinnedPackage> {
        self.0.get(package_name)
    }

    /// Return the number of pinned packages in this resolution.
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
pub struct Graph(petgraph::graph::Graph<PinnedPackage, (), petgraph::Directed>);

impl Graph {
    /// Create a new graph from the resolved `PubGrub` state.
    pub fn from_state(
        selection: &SelectedDependencies<PubGrubPackage, PubGrubVersion>,
        pins: &FxHashMap<PackageName, FxHashMap<Version, File>>,
        state: &State<PubGrubPackage, Range<PubGrubVersion>, PubGrubPriority>,
    ) -> Self {
        // TODO(charlie): petgraph is a really heavy and unnecessary dependency here. We should
        // write our own graph, given that our requirements are so simple.
        let mut graph = petgraph::graph::Graph::with_capacity(selection.len(), selection.len());

        // Add every package to the graph.
        let mut inverse =
            FxHashMap::with_capacity_and_hasher(selection.len(), BuildHasherDefault::default());
        for (package, version) in selection {
            let PubGrubPackage::Package(package_name, None) = package else {
                continue;
            };
            let version = Version::from(version.clone());
            let file = pins
                .get(package_name)
                .and_then(|versions| versions.get(&version))
                .unwrap()
                .clone();
            let pinned_package = PinnedPackage::new(package_name.clone(), version, file);
            let index = graph.add_node(pinned_package);

            inverse.insert(package_name, index);
        }

        // Add every edge to the graph.
        for (package, version) in selection {
            for id in &state.incompatibilities[package] {
                if let Kind::FromDependencyOf(self_package, self_version, dependency_package, _) =
                    &state.incompatibility_store[*id].kind
                {
                    let PubGrubPackage::Package(self_package, None) = self_package else {
                        continue;
                    };
                    let PubGrubPackage::Package(dependency_package, None) = dependency_package
                    else {
                        continue;
                    };
                    if self_version.contains(version) {
                        let self_index = &inverse[self_package];
                        let dependency_index = &inverse[dependency_package];
                        graph.add_edge(*dependency_index, *self_index, ());
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
            .map(|node| Requirement {
                name: self.0[node].name.to_string(),
                extras: None,
                version_or_url: Some(VersionOrUrl::VersionSpecifier(VersionSpecifiers::from(
                    VersionSpecifier::equals_version(self.0[node].version.clone()),
                ))),
                marker: None,
            })
            .collect()
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
            writeln!(f, "{}=={}", package.name(), package.version())?;

            let mut edges = self
                .0
                .edges(index)
                .map(|edge| &self.0[edge.target()])
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
