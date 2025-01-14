use uv_distribution_filename::DistExtension;
use uv_normalize::{ExtraName, GroupName, PackageName};
use uv_pep508::MarkerTree;
use uv_pypi_types::{HashDigest, RequirementSource};

use crate::{BuiltDist, Diagnostic, Dist, Name, ResolvedDist, SourceDist};

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

impl Resolution {
    /// Create a new resolution from the given pinned packages.
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
    MissingDev {
        /// The distribution that was requested with a non-existent development dependency group.
        dist: ResolvedDist,
        /// The development dependency group that was requested.
        dev: GroupName,
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
            Self::MissingDev { dist, dev } => {
                format!("The package `{dist}` does not have a development dependency group named `{dev}`")
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
            Self::MissingDev { dist, .. } => name == dist.name(),
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
        hashes: Vec<HashDigest>,
        install: bool,
    },
}

impl Node {
    /// Returns `true` if the node should be installed.
    pub fn install(&self) -> bool {
        match self {
            Self::Root => false,
            Self::Dist { install, .. } => *install,
        }
    }
}

/// An edge in the resolution graph, along with the marker that must be satisfied to traverse it.
#[derive(Debug, Clone)]
pub enum Edge {
    Prod(MarkerTree),
    Optional(ExtraName, MarkerTree),
    Dev(GroupName, MarkerTree),
}

impl Edge {
    /// Return the [`MarkerTree`] for this edge.
    pub fn marker(&self) -> &MarkerTree {
        match self {
            Self::Prod(marker) => marker,
            Self::Optional(_, marker) => marker,
            Self::Dev(_, marker) => marker,
        }
    }
}

impl From<&ResolvedDist> for RequirementSource {
    fn from(resolved_dist: &ResolvedDist) -> Self {
        match resolved_dist {
            ResolvedDist::Installable { dist, version } => match dist {
                Dist::Built(BuiltDist::Registry(wheels)) => RequirementSource::Registry {
                    specifier: uv_pep440::VersionSpecifiers::from(
                        uv_pep440::VersionSpecifier::equals_version(version.clone()),
                    ),
                    index: Some(wheels.best_wheel().index.url().clone()),
                    conflict: None,
                },
                Dist::Built(BuiltDist::DirectUrl(wheel)) => {
                    let mut location = wheel.url.to_url();
                    location.set_fragment(None);
                    RequirementSource::Url {
                        url: wheel.url.clone(),
                        location,
                        subdirectory: None,
                        ext: DistExtension::Wheel,
                    }
                }
                Dist::Built(BuiltDist::Path(wheel)) => RequirementSource::Path {
                    install_path: wheel.install_path.clone(),
                    url: wheel.url.clone(),
                    ext: DistExtension::Wheel,
                },
                Dist::Source(SourceDist::Registry(sdist)) => RequirementSource::Registry {
                    specifier: uv_pep440::VersionSpecifiers::from(
                        uv_pep440::VersionSpecifier::equals_version(sdist.version.clone()),
                    ),
                    index: Some(sdist.index.url().clone()),
                    conflict: None,
                },
                Dist::Source(SourceDist::DirectUrl(sdist)) => {
                    let mut location = sdist.url.to_url();
                    location.set_fragment(None);
                    RequirementSource::Url {
                        url: sdist.url.clone(),
                        location,
                        subdirectory: sdist.subdirectory.clone(),
                        ext: DistExtension::Source(sdist.ext),
                    }
                }
                Dist::Source(SourceDist::Git(sdist)) => RequirementSource::Git {
                    url: sdist.url.clone(),
                    repository: sdist.git.repository().clone(),
                    reference: sdist.git.reference().clone(),
                    precise: sdist.git.precise(),
                    subdirectory: sdist.subdirectory.clone(),
                },
                Dist::Source(SourceDist::Path(sdist)) => RequirementSource::Path {
                    install_path: sdist.install_path.clone(),
                    url: sdist.url.clone(),
                    ext: DistExtension::Source(sdist.ext),
                },
                Dist::Source(SourceDist::Directory(sdist)) => RequirementSource::Directory {
                    install_path: sdist.install_path.clone(),
                    url: sdist.url.clone(),
                    editable: sdist.editable,
                    r#virtual: sdist.r#virtual,
                },
            },
            ResolvedDist::Installed { dist } => RequirementSource::Registry {
                specifier: uv_pep440::VersionSpecifiers::from(
                    uv_pep440::VersionSpecifier::equals_version(dist.version().clone()),
                ),
                index: None,
                conflict: None,
            },
        }
    }
}
