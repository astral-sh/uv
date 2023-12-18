use std::hash::BuildHasherDefault;

use anyhow::Result;
use colored::Colorize;
use petgraph::visit::EdgeRef;
use petgraph::Direction;
use pubgrub::range::Range;
use pubgrub::solver::{Kind, State};
use pubgrub::type_aliases::SelectedDependencies;
use rustc_hash::FxHashMap;
use url::Url;

use distribution_types::{Dist, DistributionMetadata, LocalEditable, Name, PackageId, Verbatim};
use pep440_rs::Version;
use pep508_rs::{Requirement, VerbatimUrl};
use puffin_normalize::{ExtraName, PackageName};
use puffin_traits::OnceMap;
use pypi_types::Metadata21;

use crate::pins::FilePins;
use crate::pubgrub::{PubGrubDistribution, PubGrubPackage, PubGrubPriority, PubGrubVersion};
use crate::ResolveError;

/// A complete resolution graph in which every node represents a pinned package and every edge
/// represents a dependency between two pinned packages.
#[derive(Debug)]
pub struct ResolutionGraph {
    /// The underlying graph.
    petgraph: petgraph::graph::Graph<Dist, Range<PubGrubVersion>, petgraph::Directed>,
    /// The set of editable requirements in this resolution.
    editables: FxHashMap<PackageName, (LocalEditable, Metadata21)>,
    /// Any diagnostics that were encountered while building the graph.
    diagnostics: Vec<Diagnostic>,
}

impl ResolutionGraph {
    /// Create a new graph from the resolved `PubGrub` state.
    pub(crate) fn from_state(
        selection: &SelectedDependencies<PubGrubPackage, PubGrubVersion>,
        pins: &FilePins,
        distributions: &OnceMap<PackageId, Metadata21>,
        redirects: &OnceMap<Url, Url>,
        state: &State<PubGrubPackage, Range<PubGrubVersion>, PubGrubPriority>,
        editables: FxHashMap<PackageName, (LocalEditable, Metadata21)>,
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
                    let pinned_package = if let Some((editable, _)) = editables.get(package_name) {
                        Dist::from_editable(package_name.clone(), editable.clone())?
                    } else {
                        let url = redirects.get(url).map_or_else(
                            || url.clone(),
                            |url| VerbatimUrl::unknown(url.value().clone()),
                        );
                        Dist::from_url(package_name.clone(), url)?
                    };

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
                        let url = redirects.get(url).map_or_else(
                            || url.clone(),
                            |url| VerbatimUrl::unknown(url.value().clone()),
                        );
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

    /// Return the set of [`Requirement`]s that this graph represents.
    pub fn requirements(&self) -> Vec<Requirement> {
        self.petgraph
            .node_indices()
            .map(|node| &self.petgraph[node])
            .cloned()
            .map(Requirement::from)
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
            if let Some((editable, _)) = self.editables.get(package.name()) {
                writeln!(f, "-e {}", editable.verbatim())?;
            } else {
                writeln!(f, "{}", package.verbatim())?;
            }

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

impl From<ResolutionGraph> for distribution_types::Resolution {
    fn from(graph: ResolutionGraph) -> Self {
        Self::new(
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
