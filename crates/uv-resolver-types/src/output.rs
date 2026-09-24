use crate::{AnnotatedDist, Options, UniversalMarker};
use petgraph::{
    Directed,
    graph::{Graph, NodeIndex},
};
use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};
use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};
use uv_configuration::{BuildOptions, Constraints, Overrides};
use uv_distribution_types::{
    BuiltDist, Dist, Edge, Name, Node, Requirement, RequiresPython, ResolutionDiagnostic,
    ResolvedDist, SourceDist,
};
use uv_normalize::{ExtraName, GroupName, PackageName};
use uv_pep440::Version;
use uv_pypi_types::HashDigests;
/// The output of a successful resolution.
///
/// Includes a complete resolution graph in which every node represents a pinned package and every
/// edge represents a dependency between two pinned packages.
#[derive(Debug)]
pub struct ResolverOutput {
    /// The underlying graph.
    pub graph: Graph<ResolutionGraphNode, UniversalMarker, Directed>,
    /// The range of supported Python versions.
    pub requires_python: RequiresPython,
    /// If the resolution had non-identical forks, store the forks in the lockfile so we can
    /// recreate them in subsequent resolutions.
    pub fork_markers: Vec<UniversalMarker>,
    /// Any diagnostics that were encountered while building the graph.
    pub diagnostics: Vec<ResolutionDiagnostic>,
    /// The requirements that were used to build the graph.
    pub requirements: Vec<Requirement>,
    /// The constraints that were used to build the graph.
    pub constraints: Constraints,
    /// The overrides that were used to build the graph.
    pub overrides: Overrides,
    /// The options that were used to build the graph.
    pub options: Options,
}

#[derive(Debug, Clone)]
#[expect(clippy::large_enum_variant)]
pub enum ResolutionGraphNode {
    Root,
    Dist(AnnotatedDist),
}

impl ResolutionGraphNode {
    pub fn marker(&self) -> &UniversalMarker {
        match self {
            Self::Root => &UniversalMarker::TRUE,
            Self::Dist(dist) => &dist.marker,
        }
    }

    pub(crate) fn package_extra_names(&self) -> Option<(&PackageName, &ExtraName)> {
        match self {
            Self::Root => None,
            Self::Dist(dist) => {
                let extra = dist.kind.extra()?;
                Some((&dist.name, extra))
            }
        }
    }

    pub(crate) fn package_group_names(&self) -> Option<(&PackageName, &GroupName)> {
        match self {
            Self::Root => None,
            Self::Dist(dist) => {
                let group = dist.kind.group()?;
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

impl ResolverOutput {
    /// Returns an iterator over the distinct packages in the graph.
    fn dists(&self) -> impl Iterator<Item = &AnnotatedDist> {
        self.graph
            .node_indices()
            .filter_map(move |index| match &self.graph[index] {
                ResolutionGraphNode::Root => None,
                ResolutionGraphNode::Dist(dist) => Some(dist),
            })
    }

    /// Returns an iterator over the base distributions in the graph.
    pub fn base_dists(&self) -> impl Iterator<Item = (NodeIndex, &AnnotatedDist)> {
        self.graph
            .node_indices()
            .filter_map(move |node_index| match &self.graph[node_index] {
                ResolutionGraphNode::Root => None,
                ResolutionGraphNode::Dist(dist) => dist.is_base().then_some((node_index, dist)),
            })
    }

    /// Return the number of distinct packages in the graph.
    pub fn len(&self) -> usize {
        self.base_dists().count()
    }

    /// Return `true` if there are no packages in the graph.
    pub fn is_empty(&self) -> bool {
        self.base_dists().next().is_none()
    }

    /// Retain registry hashes only for artifacts permitted by package-specific build options.
    ///
    /// All available wheel hashes remain eligible when source builds are disabled so the
    /// resulting requirements can still be installed on other supported platforms.
    pub fn retain_allowed_distribution_hashes(&mut self, build_options: &BuildOptions) {
        for node in self.graph.node_weights_mut() {
            let ResolutionGraphNode::Dist(distribution) = node else {
                continue;
            };
            let ResolvedDist::Installable { dist, .. } = &distribution.dist else {
                continue;
            };
            let allowed_hashes = match dist.as_ref() {
                Dist::Built(BuiltDist::Registry(dist))
                    if build_options.no_build_package(&distribution.name) =>
                {
                    dist.wheels
                        .iter()
                        .flat_map(|wheel| wheel.file.hashes.iter())
                        .collect::<FxHashSet<_>>()
                }
                Dist::Source(SourceDist::Registry(source))
                    if build_options.no_binary_package(&distribution.name) =>
                {
                    source.file.hashes.iter().collect::<FxHashSet<_>>()
                }
                _ => continue,
            };
            if allowed_hashes.is_empty() {
                continue;
            }

            let hashes = distribution
                .hashes
                .iter()
                .filter(|hash| allowed_hashes.contains(hash))
                .cloned()
                .collect::<Vec<_>>();
            if !hashes.is_empty() {
                distribution.hashes = HashDigests::from(hashes);
            }
        }
    }

    /// Returns `true` if the graph contains the given package.
    pub fn contains(&self, name: &PackageName) -> bool {
        self.dists().any(|dist| dist.name() == name)
    }

    /// Return the [`ResolutionDiagnostic`]s that were encountered while building the graph.
    pub fn diagnostics(&self) -> &[ResolutionDiagnostic] {
        &self.diagnostics
    }

    /// Returns a sequence of conflicting distribution errors from this
    /// resolution.
    ///
    /// Correct resolutions always return an empty sequence. A non-empty
    /// sequence implies there is a package with two distinct versions in the
    /// same marker environment in this resolution. This in turn implies that
    /// an installation in that marker environment could wind up trying to
    /// install different versions of the same package, which is not allowed.
    pub fn find_conflicting_distributions(&self) -> Vec<ConflictingDistributionError> {
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

                    let edge = if let Some(extra) = source_dist.kind.extra() {
                        Edge::Optional(extra.clone())
                    } else if let Some(group) = source_dist.kind.group() {
                        Edge::Dev(group.clone())
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
