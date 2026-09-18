use petgraph::algo::tarjan_scc;
use rustc_hash::FxHashMap;

use uv_distribution_filename::DistExtension;
use uv_normalize::{ExtraName, GroupName, PackageName};
use uv_pypi_types::{HashDigest, HashDigests};

use crate::{
    BuiltDist, Diagnostic, Dist, IndexMetadata, Name, RequirementSource, ResolvedDist, SourceDist,
};

/// A set of packages pinned at specific versions.
///
/// This is similar to [`ResolverOutput`], but represents a resolution for a subset of all
/// marker environments. For example, the resolution is guaranteed to contain at most one version
/// for a given package.
#[derive(Debug, Default, Clone)]
pub struct Resolution {
    graph: petgraph::graph::DiGraph<Node, Edge>,
    diagnostics: Vec<ResolutionDiagnostic>,
}

/// A group of distributions that depend on each other, directly or transitively.
#[derive(Debug)]
pub struct DependencyGroup<'a> {
    /// The distributions to install, sorted by package name.
    ///
    /// This can be empty when the group contains only filtered distributions or the root.
    pub distributions: Vec<&'a ResolvedDist>,
    /// Indices of the groups this group depends on, in the result of
    /// [`Resolution::dependency_groups`]. Each index precedes this group.
    pub dependencies: Vec<usize>,
}

impl Resolution {
    /// Create a [`Resolution`] from the given pinned packages.
    pub fn new(graph: petgraph::graph::DiGraph<Node, Edge>) -> Self {
        Self {
            graph,
            diagnostics: Vec::new(),
        }
    }

    /// Return the underlying graph of the resolution.
    pub fn graph(&self) -> &petgraph::graph::DiGraph<Node, Edge> {
        &self.graph
    }

    /// Add [`Diagnostics`] to the resolution.
    #[must_use]
    pub fn with_diagnostics(mut self, diagnostics: Vec<ResolutionDiagnostic>) -> Self {
        self.diagnostics.extend(diagnostics);
        self
    }

    /// Return the hashes for the given package name, if they exist.
    pub fn hashes(&self) -> impl Iterator<Item = (&ResolvedDist, &[HashDigest])> {
        self.graph
            .node_indices()
            .filter_map(move |node| match &self.graph[node] {
                Node::Dist {
                    dist,
                    hashes,
                    install,
                    ..
                } if *install => Some((dist, hashes.as_slice())),
                _ => None,
            })
    }

    /// Iterate over the [`ResolvedDist`] entities in this resolution.
    pub fn distributions(&self) -> impl Iterator<Item = &ResolvedDist> {
        self.graph
            .raw_nodes()
            .iter()
            .filter_map(|node| match &node.weight {
                Node::Dist { dist, install, .. } if *install => Some(dist),
                _ => None,
            })
    }

    /// Group distributions into strongly connected components, with dependencies first.
    ///
    /// Groups can be processed concurrently once their dependencies have completed. Distributions
    /// within a group form a dependency cycle and are returned in a stable order by package name.
    /// Filtered distributions remain in the graph so that dependency paths through them are retained.
    /// Only dependencies recorded in this resolution are included; some resolutions, such as those
    /// constructed from a `pylock.toml`, contain no edges between distributions.
    pub fn dependency_groups(&self) -> Vec<DependencyGroup<'_>> {
        // Resolution edges point from a distribution to its dependencies. Tarjan's reverse
        // topological order therefore puts dependencies before their dependents.
        let components = tarjan_scc(&self.graph);
        let component_by_node = components
            .iter()
            .enumerate()
            .flat_map(|(component_index, component)| {
                component.iter().map(move |node| (*node, component_index))
            })
            .collect::<FxHashMap<_, _>>();

        components
            .iter()
            .enumerate()
            .map(|(component_index, component)| {
                let mut distributions = component
                    .iter()
                    .filter_map(|node| match &self.graph[*node] {
                        Node::Dist { dist, install, .. } if *install => Some(dist),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                distributions.sort_unstable_by(|left, right| left.name().cmp(right.name()));

                let mut dependencies = component
                    .iter()
                    .flat_map(|node| self.graph.neighbors(*node))
                    .map(|node| component_by_node[&node])
                    .filter(|dependency| *dependency != component_index)
                    .collect::<Vec<_>>();
                dependencies.sort_unstable();
                dependencies.dedup();
                debug_assert!(dependencies.iter().all(|index| *index < component_index));

                DependencyGroup {
                    distributions,
                    dependencies,
                }
            })
            .collect()
    }

    /// Return the number of distributions in this resolution.
    pub fn len(&self) -> usize {
        self.distributions().count()
    }

    /// Return `true` if there are no pinned packages in this resolution.
    pub fn is_empty(&self) -> bool {
        self.distributions().next().is_none()
    }

    /// Return the [`ResolutionDiagnostic`]s that were produced during resolution.
    pub fn diagnostics(&self) -> &[ResolutionDiagnostic] {
        &self.diagnostics
    }

    /// Filter the resolution to only include packages that match the given predicate.
    #[must_use]
    pub fn filter(mut self, predicate: impl Fn(&ResolvedDist) -> bool) -> Self {
        for node in self.graph.node_weights_mut() {
            if let Node::Dist { dist, install, .. } = node {
                if !predicate(dist) {
                    *install = false;
                }
            }
        }
        self
    }

    /// Map over the resolved distributions in this resolution.
    ///
    /// For efficiency, the map function should return `None` if the resolved distribution is
    /// unchanged.
    #[must_use]
    pub fn map(mut self, predicate: impl Fn(&ResolvedDist) -> Option<ResolvedDist>) -> Self {
        for node in self.graph.node_weights_mut() {
            if let Node::Dist { dist, .. } = node {
                if let Some(transformed) = predicate(dist) {
                    *dist = transformed;
                }
            }
        }
        self
    }
}

#[derive(Debug, Clone, Hash)]
pub enum ResolutionDiagnostic {
    MissingExtra {
        /// The distribution that was requested with a non-existent extra. For example,
        /// `black==23.10.0`.
        dist: ResolvedDist,
        /// The extra that was requested. For example, `colorama` in `black[colorama]`.
        extra: ExtraName,
    },
    MissingGroup {
        /// The distribution that was requested with a non-existent development dependency group.
        dist: ResolvedDist,
        /// The development dependency group that was requested.
        group: GroupName,
    },
    YankedVersion {
        /// The package that was requested with a yanked version. For example, `black==23.10.0`.
        dist: ResolvedDist,
        /// The reason that the version was yanked, if any.
        reason: Option<String>,
    },
    MissingLowerBound {
        /// The name of the package that had no lower bound from any other package in the
        /// resolution. For example, `black`.
        package_name: PackageName,
    },
}

impl Diagnostic for ResolutionDiagnostic {
    /// Convert the diagnostic into a user-facing message.
    fn message(&self) -> String {
        match self {
            Self::MissingExtra { dist, extra } => {
                format!("The package `{dist}` does not have an extra named `{extra}`")
            }
            Self::MissingGroup { dist, group } => {
                format!(
                    "The package `{dist}` does not have a development dependency group named `{group}`"
                )
            }
            Self::YankedVersion { dist, reason } => {
                if let Some(reason) = reason {
                    format!("`{dist}` is yanked (reason: \"{reason}\")")
                } else {
                    format!("`{dist}` is yanked")
                }
            }
            Self::MissingLowerBound { package_name: name } => {
                format!(
                    "The transitive dependency `{name}` is unpinned. \
                    Consider setting a lower bound with a constraint when using \
                    `--resolution lowest` to avoid using outdated versions."
                )
            }
        }
    }

    /// Returns `true` if the [`PackageName`] is involved in this diagnostic.
    fn includes(&self, name: &PackageName) -> bool {
        match self {
            Self::MissingExtra { dist, .. } => name == dist.name(),
            Self::MissingGroup { dist, .. } => name == dist.name(),
            Self::YankedVersion { dist, .. } => name == dist.name(),
            Self::MissingLowerBound { package_name } => name == package_name,
        }
    }
}

/// A node in the resolution, along with whether its been filtered out.
///
/// We retain filtered nodes as we still need to be able to trace dependencies through the graph
/// (e.g., to determine why a package was included in the resolution).
#[derive(Debug, Clone)]
pub enum Node {
    Root,
    Dist {
        dist: ResolvedDist,
        hashes: HashDigests,
        install: bool,
    },
}

/// An edge in the resolution graph.
#[derive(Debug, Clone)]
pub enum Edge {
    Prod,
    Optional(ExtraName),
    Dev(GroupName),
}

impl From<&ResolvedDist> for RequirementSource {
    fn from(resolved_dist: &ResolvedDist) -> Self {
        match resolved_dist {
            ResolvedDist::Installable { dist, .. } => match dist.as_ref() {
                Dist::Built(BuiltDist::Registry(wheels)) => {
                    let wheel = wheels.best_wheel();
                    Self::Registry {
                        specifier: uv_pep440::VersionSpecifiers::from(
                            uv_pep440::VersionSpecifier::equals_version(
                                wheel.filename.version.clone(),
                            ),
                        ),
                        index: Some(IndexMetadata::from(wheel.index.clone())),
                        conflict: None,
                    }
                }
                Dist::Built(BuiltDist::DirectUrl(wheel)) => {
                    let mut location = wheel.url.to_url();
                    location.set_fragment(None);
                    Self::Url {
                        url: wheel.url.clone(),
                        location,
                        subdirectory: None,
                        ext: DistExtension::Wheel,
                    }
                }
                Dist::Built(BuiltDist::Path(wheel)) => Self::Path {
                    install_path: wheel.install_path.clone(),
                    url: wheel.url.clone(),
                    ext: DistExtension::Wheel,
                },
                Dist::Built(BuiltDist::GitPath(wheel)) => Self::GitPath {
                    url: wheel.url.clone(),
                    git: (*wheel.git).clone(),
                    install_path: wheel.install_path.clone(),
                    ext: DistExtension::Wheel,
                },
                Dist::Source(SourceDist::Registry(sdist)) => Self::Registry {
                    specifier: uv_pep440::VersionSpecifiers::from(
                        uv_pep440::VersionSpecifier::equals_version(sdist.version.clone()),
                    ),
                    index: Some(IndexMetadata::from(sdist.index.clone())),
                    conflict: None,
                },
                Dist::Source(SourceDist::DirectUrl(sdist)) => {
                    let mut location = sdist.url.to_url();
                    location.set_fragment(None);
                    Self::Url {
                        url: sdist.url.clone(),
                        location,
                        subdirectory: sdist.subdirectory.clone(),
                        ext: DistExtension::Source(sdist.ext),
                    }
                }
                Dist::Source(SourceDist::GitPath(sdist)) => Self::GitPath {
                    url: sdist.url.clone(),
                    git: (*sdist.git).clone(),
                    install_path: sdist.install_path.clone(),
                    ext: DistExtension::Source(sdist.ext),
                },
                Dist::Source(SourceDist::GitDirectory(sdist)) => Self::GitDirectory {
                    git: (*sdist.git).clone(),
                    url: sdist.url.clone(),
                    subdirectory: sdist.subdirectory.clone(),
                },
                Dist::Source(SourceDist::Path(sdist)) => Self::Path {
                    install_path: sdist.install_path.clone(),
                    url: sdist.url.clone(),
                    ext: DistExtension::Source(sdist.ext),
                },
                Dist::Source(SourceDist::Directory(sdist)) => Self::Directory {
                    install_path: sdist.install_path.clone(),
                    url: sdist.url.clone(),
                    editable: sdist.editable,
                    r#virtual: sdist.r#virtual,
                },
            },
            ResolvedDist::Installed { dist } => Self::Registry {
                specifier: uv_pep440::VersionSpecifiers::from(
                    uv_pep440::VersionSpecifier::equals_version(dist.version().clone()),
                ),
                index: None,
                conflict: None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::str::FromStr;
    use std::sync::Arc;

    use petgraph::graph::DiGraph;

    use uv_normalize::PackageName;
    use uv_pep440::Version;
    use uv_pypi_types::HashDigests;

    use crate::{InstalledDistKind, InstalledRegistryDist, Name, ResolvedDist};

    use super::{Edge, Node, Resolution};

    fn distribution(name: &str, install: bool) -> Node {
        Node::Dist {
            dist: ResolvedDist::Installed {
                dist: Arc::new(
                    InstalledDistKind::Registry(InstalledRegistryDist {
                        name: PackageName::from_str(name).expect("valid package name"),
                        version: Version::from_str("1.0").expect("valid version"),
                        path: PathBuf::new().into_boxed_path(),
                        cache_info: None,
                        build_info: None,
                    })
                    .into(),
                ),
            },
            hashes: HashDigests::empty(),
            install,
        }
    }

    fn groups(resolution: &Resolution) -> Vec<(Vec<&str>, Vec<usize>)> {
        resolution
            .dependency_groups()
            .into_iter()
            .map(|group| {
                (
                    group
                        .distributions
                        .into_iter()
                        .map(|dist| dist.name().as_str())
                        .collect(),
                    group.dependencies,
                )
            })
            .collect()
    }

    #[test]
    fn dependency_groups_retain_filtered_paths() {
        let mut graph = DiGraph::new();
        let root = graph.add_node(Node::Root);
        let app = graph.add_node(distribution("app", true));
        let filtered = graph.add_node(distribution("filtered", false));
        let base = graph.add_node(distribution("base", true));
        graph.add_edge(root, app, Edge::Prod);
        graph.add_edge(app, filtered, Edge::Prod);
        graph.add_edge(filtered, base, Edge::Prod);

        assert_eq!(
            groups(&Resolution::new(graph)),
            [
                (vec!["base"], vec![]),
                (vec![], vec![0]),
                (vec!["app"], vec![1]),
                (vec![], vec![2]),
            ]
        );
    }

    #[test]
    fn dependency_groups_sort_cycles_and_deduplicate_edges() {
        let mut graph = DiGraph::new();
        let zebra = graph.add_node(distribution("zebra", true));
        let alpha = graph.add_node(distribution("alpha", true));
        let base = graph.add_node(distribution("base", true));
        graph.add_edge(zebra, alpha, Edge::Prod);
        graph.add_edge(alpha, zebra, Edge::Prod);
        graph.add_edge(zebra, base, Edge::Prod);
        graph.add_edge(alpha, base, Edge::Prod);
        graph.add_edge(alpha, base, Edge::Prod);

        assert_eq!(
            groups(&Resolution::new(graph)),
            [(vec!["base"], vec![]), (vec!["alpha", "zebra"], vec![0]),]
        );
    }

    #[test]
    fn dependency_groups_leave_unrelated_distributions_independent() {
        let mut graph = DiGraph::new();
        let root = graph.add_node(Node::Root);
        let alpha = graph.add_node(distribution("alpha", true));
        let beta = graph.add_node(distribution("beta", true));
        graph.add_edge(root, alpha, Edge::Prod);
        graph.add_edge(root, beta, Edge::Prod);

        let resolution = Resolution::new(graph);
        let groups = resolution.dependency_groups();
        assert_eq!(groups.len(), 3);
        assert!(
            groups[..2]
                .iter()
                .all(|group| { group.distributions.len() == 1 && group.dependencies.is_empty() })
        );
        assert!(groups[2].distributions.is_empty());
        assert_eq!(groups[2].dependencies, [0, 1]);
    }
}
