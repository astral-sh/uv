use crate::{BuiltDist, Dist, DistRef, Edge, Name, Node, Resolution, ResolvedDist, SourceDist};
use petgraph::prelude::EdgeRef;
use petgraph::Direction;
use rustc_hash::FxHashSet;
use std::collections::VecDeque;
use std::fmt::{Debug, Display, Formatter};
use std::ops::Deref;
use uv_normalize::{ExtraName, GroupName, PackageName};
use uv_pep440::Version;
use version_ranges::Ranges;

/// Workaround for inspecting errors while avoiding cyclical crate dependencies.
///
/// The `uv-resolver`, `uv-installer` and `uv-build-frontend` error types all reference each other:
/// Resolution and installation may need to build packages, while the build frontend needs to
/// resolve and install for the PEP 517 build environment (See also: `BuildContext`). We use an
/// opaque `dyn` error type, with this trait allowing to inspect it.
pub trait IsBuildBackendError: std::error::Error + Send + Sync + 'static {
    /// Returns whether the build backend failed to build the package, so it's not a uv error.
    fn is_build_backend_error(&self) -> bool;
}

/// `anyhow::Error`-like wrapper type to make `IsBuildBackendError` work as `thiserror` `#[source]`.
///
/// The `uv-resolver`, `uv-installer` and `uv-build-frontend` error types all reference each other:
/// Resolution and installation may need to build packages, while the build frontend needs to
/// resolve and install for the PEP 517 build environment (See also: `BuildContext`). We use an
/// opaque `dyn` error type with [`IsBuildBackendError`] making the error inspectable.
///
/// `thiserror` does not recognize `Box<dyn IsBuildFrontendError + Send + Sync + 'static>` as
/// error source by itself, it complains about the internal `AsDynError` not being implemented.
/// This struct is an otherwise transparent wrapper that thiserror recognizes.
pub struct AnyErrorBuild(Box<dyn IsBuildBackendError>);

impl Debug for AnyErrorBuild {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(&self.0, f)
    }
}

impl Display for AnyErrorBuild {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl std::error::Error for AnyErrorBuild {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.0.source()
    }

    #[allow(deprecated)]
    fn description(&self) -> &str {
        self.0.description()
    }

    #[allow(deprecated)]
    fn cause(&self) -> Option<&dyn std::error::Error> {
        self.0.cause()
    }
}

impl<T: IsBuildBackendError> From<T> for AnyErrorBuild {
    fn from(err: T) -> Self {
        Self(Box::new(err))
    }
}

impl Deref for AnyErrorBuild {
    type Target = dyn IsBuildBackendError;

    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}

/// The operation(s) that failed when reporting an error with a distribution.
#[derive(Debug)]
pub enum DistErrorKind {
    Download,
    DownloadAndBuild,
    Build,
    BuildBackend,
    Read,
}

impl DistErrorKind {
    pub fn from_dist_and_err(dist: &Dist, err: &impl IsBuildBackendError) -> Self {
        if err.is_build_backend_error() {
            DistErrorKind::BuildBackend
        } else {
            match dist {
                Dist::Built(BuiltDist::Path(_)) => DistErrorKind::Read,
                Dist::Source(SourceDist::Path(_) | SourceDist::Directory(_)) => {
                    DistErrorKind::Build
                }
                Dist::Built(_) => DistErrorKind::Download,
                Dist::Source(source_dist) => {
                    if source_dist.is_local() {
                        DistErrorKind::Build
                    } else {
                        DistErrorKind::DownloadAndBuild
                    }
                }
            }
        }
    }
}

impl Display for DistErrorKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            DistErrorKind::Download => f.write_str("Failed to download"),
            DistErrorKind::DownloadAndBuild => f.write_str("Failed to download and build"),
            DistErrorKind::Build => f.write_str("Failed to build"),
            DistErrorKind::BuildBackend => f.write_str("Failed to build"),
            DistErrorKind::Read => f.write_str("Failed to read"),
        }
    }
}

/// A chain of derivation steps from the root package to the current package, to explain why a
/// package is included in the resolution.
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]
pub struct DerivationChain(Vec<DerivationStep>);

impl FromIterator<DerivationStep> for DerivationChain {
    fn from_iter<T: IntoIterator<Item = DerivationStep>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl DerivationChain {
    /// Compute a [`DerivationChain`] from a resolution graph.
    ///
    /// This is used to construct a derivation chain upon install failure in the `uv pip` context,
    /// where we don't have a lockfile describing the resolution.
    pub fn from_resolution(
        resolution: &Resolution,
        target: DistRef<'_>,
    ) -> Option<DerivationChain> {
        // Find the target distribution in the resolution graph.
        let target = resolution.graph().node_indices().find(|node| {
            let Node::Dist {
                dist: ResolvedDist::Installable { dist, .. },
                ..
            } = &resolution.graph()[*node]
            else {
                return false;
            };
            target == dist.as_ref()
        })?;

        // Perform a BFS to find the shortest path to the root.
        let mut queue = VecDeque::new();
        queue.push_back((target, None, None, Vec::new()));

        // TODO(charlie): Consider respecting markers here.
        let mut seen = FxHashSet::default();
        while let Some((node, extra, group, mut path)) = queue.pop_front() {
            if !seen.insert(node) {
                continue;
            }
            match &resolution.graph()[node] {
                Node::Root => {
                    path.reverse();
                    path.pop();
                    return Some(DerivationChain::from_iter(path));
                }
                Node::Dist { dist, .. } => {
                    for edge in resolution.graph().edges_directed(node, Direction::Incoming) {
                        let mut path = path.clone();
                        path.push(DerivationStep::new(
                            dist.name().clone(),
                            extra.clone(),
                            group.clone(),
                            dist.version().clone(),
                            Ranges::empty(),
                        ));
                        let target = edge.source();
                        let extra = match edge.weight() {
                            Edge::Optional(extra, ..) => Some(extra.clone()),
                            _ => None,
                        };
                        let group = match edge.weight() {
                            Edge::Dev(group, ..) => Some(group.clone()),
                            _ => None,
                        };
                        queue.push_back((target, extra, group, path));
                    }
                }
            }
        }

        None
    }

    /// Returns the length of the derivation chain.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns `true` if the derivation chain is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns an iterator over the steps in the derivation chain.
    pub fn iter(&self) -> std::slice::Iter<DerivationStep> {
        self.0.iter()
    }
}

impl<'chain> IntoIterator for &'chain DerivationChain {
    type Item = &'chain DerivationStep;
    type IntoIter = std::slice::Iter<'chain, DerivationStep>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl IntoIterator for DerivationChain {
    type Item = DerivationStep;
    type IntoIter = std::vec::IntoIter<DerivationStep>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

/// A step in a derivation chain.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DerivationStep {
    /// The name of the package.
    pub name: PackageName,
    /// The enabled extra of the package, if any.
    pub extra: Option<ExtraName>,
    /// The enabled dependency group of the package, if any.
    pub group: Option<GroupName>,
    /// The version of the package.
    pub version: Version,
    /// The constraints applied to the subsequent package in the chain.
    pub range: Ranges<Version>,
}

impl DerivationStep {
    /// Create a [`DerivationStep`] from a package name and version.
    pub fn new(
        name: PackageName,
        extra: Option<ExtraName>,
        group: Option<GroupName>,
        version: Version,
        range: Ranges<Version>,
    ) -> Self {
        Self {
            name,
            extra,
            group,
            version,
            range,
        }
    }
}
