use std::borrow::Cow;
use std::hash::BuildHasherDefault;

use anyhow::Result;
use dashmap::DashMap;
use owo_colors::OwoColorize;
use petgraph::visit::EdgeRef;
use petgraph::Direction;
use pubgrub::range::Range;
use pubgrub::solver::{Kind, State};
use pubgrub::type_aliases::SelectedDependencies;

use rustc_hash::FxHashMap;
use url::Url;

use distribution_types::{Dist, DistributionMetadata, LocalEditable, Name, PackageId, Verbatim};
use once_map::OnceMap;
use pep440_rs::Version;
use pep508_rs::VerbatimUrl;
use pypi_types::{Hashes, Metadata21};
use uv_normalize::{ExtraName, PackageName};

use crate::pins::FilePins;
use crate::pubgrub::{PubGrubDistribution, PubGrubPackage, PubGrubPriority};
use crate::resolver::VersionsResponse;

use crate::ResolveError;

/// A complete resolution graph in which every node represents a pinned package and every edge
/// represents a dependency between two pinned packages.
#[derive(Debug)]
pub struct ResolutionGraph {
    /// The underlying graph.
    petgraph: petgraph::graph::Graph<Dist, Range<Version>, petgraph::Directed>,
    /// The metadata for every distribution in this resolution.
    hashes: FxHashMap<PackageName, Vec<Hashes>>,
    /// The set of editable requirements in this resolution.
    editables: FxHashMap<PackageName, (LocalEditable, Metadata21)>,
    /// Any diagnostics that were encountered while building the graph.
    diagnostics: Vec<Diagnostic>,
}

impl ResolutionGraph {
    /// Create a new graph from the resolved `PubGrub` state.
    pub(crate) fn from_state(
        selection: &SelectedDependencies<PubGrubPackage, Version>,
        pins: &FilePins,
        packages: &OnceMap<PackageName, VersionsResponse>,
        distributions: &OnceMap<PackageId, Metadata21>,
        redirects: &DashMap<Url, Url>,
        state: &State<PubGrubPackage, Range<Version>, PubGrubPriority>,
        editables: FxHashMap<PackageName, (LocalEditable, Metadata21)>,
    ) -> Result<Self, ResolveError> {
        // TODO(charlie): petgraph is a really heavy and unnecessary dependency here. We should
        // write our own graph, given that our requirements are so simple.
        let mut petgraph = petgraph::graph::Graph::with_capacity(selection.len(), selection.len());
        let mut hashes =
            FxHashMap::with_capacity_and_hasher(selection.len(), BuildHasherDefault::default());
        let mut diagnostics = Vec::new();

        // Add every package to the graph.
        let mut inverse =
            FxHashMap::with_capacity_and_hasher(selection.len(), BuildHasherDefault::default());
        for (package, version) in selection {
            match package {
                PubGrubPackage::Package(package_name, None, None) => {
                    // Create the distribution.
                    let pinned_package = if let Some((editable, _)) = editables.get(package_name) {
                        Dist::from_editable(package_name.clone(), editable.clone())?
                    } else {
                        pins.get(package_name, version)
                            .expect("Every package should be pinned")
                            .clone()
                    };

                    // Add its hashes to the index.
                    if let Some(versions_response) = packages.get(package_name) {
                        if let VersionsResponse::Found(ref version_map) = *versions_response {
                            hashes.insert(package_name.clone(), {
                                let mut hashes = version_map.hashes(version);
                                hashes.sort_unstable();
                                hashes
                            });
                        }
                    }

                    // Add the distribution to the graph.
                    let index = petgraph.add_node(pinned_package);
                    inverse.insert(package_name, index);
                }
                PubGrubPackage::Package(package_name, None, Some(url)) => {
                    // Create the distribution.
                    let pinned_package = {
                        let url = redirects.get(url).map_or_else(
                            || url.clone(),
                            |url| VerbatimUrl::unknown(url.value().clone()),
                        );
                        Dist::from_url(package_name.clone(), url)?
                    };

                    // Add its hashes to the index.
                    if let Some(versions_response) = packages.get(package_name) {
                        if let VersionsResponse::Found(ref version_map) = *versions_response {
                            hashes.insert(package_name.clone(), {
                                let mut hashes = version_map.hashes(version);
                                hashes.sort_unstable();
                                hashes
                            });
                        }
                    }

                    // Add the distribution to the graph.
                    let index = petgraph.add_node(pinned_package);
                    inverse.insert(package_name, index);
                }
                PubGrubPackage::Package(package_name, Some(extra), None) => {
                    // Validate that the `extra` exists.
                    let dist = PubGrubDistribution::from_registry(package_name, version);

                    if let Some((_, metadata)) = editables.get(package_name) {
                        if !metadata.provides_extras.contains(extra) {
                            let pinned_package = pins
                                .get(package_name, version)
                                .expect("Every package should be pinned")
                                .clone();

                            diagnostics.push(Diagnostic::MissingExtra {
                                dist: pinned_package,
                                extra: extra.clone(),
                            });
                        }
                    } else {
                        let metadata = distributions
                            .get(&dist.package_id())
                            .expect("Every package should have metadata");

                        if !metadata.provides_extras.contains(extra) {
                            let pinned_package = pins
                                .get(package_name, version)
                                .expect("Every package should be pinned")
                                .clone();

                            diagnostics.push(Diagnostic::MissingExtra {
                                dist: pinned_package,
                                extra: extra.clone(),
                            });
                        }
                    }
                }
                PubGrubPackage::Package(package_name, Some(extra), Some(url)) => {
                    // Validate that the `extra` exists.
                    let dist = PubGrubDistribution::from_url(package_name, url);
                    let metadata = distributions
                        .get(&dist.package_id())
                        .expect("Every package should have metadata");

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
            hashes,
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

    /// Return the [`Diagnostic`]s that were encountered while building the graph.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Return the underlying graph.
    pub fn petgraph(&self) -> &petgraph::graph::Graph<Dist, Range<Version>, petgraph::Directed> {
        &self.petgraph
    }
}

/// A [`std::fmt::Display`] implementation for the resolution graph.
#[derive(Debug)]
pub struct DisplayResolutionGraph<'a> {
    /// The underlying graph.
    resolution: &'a ResolutionGraph,
    /// Whether to include hashes in the output.
    show_hashes: bool,
    /// Whether to include annotations in the output, to indicate which dependency or dependencies
    /// requested each package.
    include_annotations: bool,
}

impl<'a> From<&'a ResolutionGraph> for DisplayResolutionGraph<'a> {
    fn from(resolution: &'a ResolutionGraph) -> Self {
        Self::new(resolution, false, true)
    }
}

impl<'a> DisplayResolutionGraph<'a> {
    /// Create a new [`DisplayResolutionGraph`] for the given graph.
    pub fn new(
        underlying: &'a ResolutionGraph,
        show_hashes: bool,
        include_annotations: bool,
    ) -> DisplayResolutionGraph<'a> {
        Self {
            resolution: underlying,
            show_hashes,
            include_annotations,
        }
    }
}

/// Write the graph in the `{name}=={version}` format of requirements.txt that pip uses.
impl std::fmt::Display for DisplayResolutionGraph<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        #[derive(Debug)]
        enum Node<'a> {
            /// A node linked to an editable distribution.
            Editable(&'a PackageName, &'a LocalEditable),
            /// A node linked to a non-editable distribution.
            Distribution(&'a PackageName, &'a Dist),
        }

        #[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
        enum NodeKey<'a> {
            /// A node linked to an editable distribution, sorted by verbatim representation.
            Editable(Cow<'a, str>),
            /// A node linked to a non-editable distribution, sorted by package name.
            Distribution(&'a PackageName),
        }

        impl<'a> Node<'a> {
            /// Return the name of the package.
            fn name(&self) -> &'a PackageName {
                match self {
                    Node::Editable(name, _) => name,
                    Node::Distribution(name, _) => name,
                }
            }

            /// Return a comparable key for the node.
            fn key(&self) -> NodeKey<'a> {
                match self {
                    Node::Editable(_, editable) => NodeKey::Editable(editable.verbatim()),
                    Node::Distribution(name, _) => NodeKey::Distribution(name),
                }
            }
        }

        // Collect all packages.
        let mut nodes = self
            .resolution
            .petgraph
            .node_indices()
            .map(|index| {
                let dist = &self.resolution.petgraph[index];
                let name = dist.name();
                let node = if let Some((editable, _)) = self.resolution.editables.get(name) {
                    Node::Editable(name, editable)
                } else {
                    Node::Distribution(name, dist)
                };
                (index, node)
            })
            .collect::<Vec<_>>();

        // Sort the nodes by name, but with editable packages first.
        nodes.sort_unstable_by_key(|(index, node)| (node.key(), *index));

        // Print out the dependency graph.
        for (index, node) in nodes {
            // Display the node itself.
            match node {
                Node::Distribution(_, dist) => {
                    write!(f, "{}", dist.verbatim())?;
                }
                Node::Editable(_, editable) => {
                    write!(f, "-e {}", editable.verbatim())?;
                }
            }

            // Display the distribution hashes, if any.
            if self.show_hashes {
                if let Some(hashes) = self
                    .resolution
                    .hashes
                    .get(node.name())
                    .filter(|hashes| !hashes.is_empty())
                {
                    for hash in hashes {
                        if let Some(hash) = hash.to_string() {
                            writeln!(f, " \\")?;
                            write!(f, "    --hash={hash}")?;
                        }
                    }
                }
            }
            writeln!(f)?;

            if self.include_annotations {
                // Display all dependencies.
                let mut edges = self
                    .resolution
                    .petgraph
                    .edges_directed(index, Direction::Incoming)
                    .map(|edge| &self.resolution.petgraph[edge.source()])
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
