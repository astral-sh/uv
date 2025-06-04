use std::fmt::Display;

use uv_distribution::Metadata;
use uv_distribution_types::{
    BuiltDist, Dist, DistributionMetadata, IndexUrl, Name, ResolvedDist, SourceDist,
    VersionOrUrlRef,
};
use uv_normalize::{ExtraName, GroupName, PackageName};
use uv_pep440::Version;
use uv_pypi_types::HashDigests;

pub use crate::resolution::display::{AnnotationStyle, DisplayResolutionGraph};
pub(crate) use crate::resolution::output::ResolutionGraphNode;
pub use crate::resolution::output::{ConflictingDistributionError, ResolverOutput};
pub(crate) use crate::resolution::requirements_txt::RequirementsTxtDist;
use crate::universal_marker::UniversalMarker;

mod display;
mod output;
mod requirements_txt;

/// A pinned package with its resolved distribution and metadata. The [`ResolvedDist`] refers to a
/// specific distribution (e.g., a specific wheel), while the [`Metadata23`] refers to the metadata
/// for the package-version pair.
#[derive(Debug, Clone)]
pub(crate) struct AnnotatedDist {
    pub(crate) dist: ResolvedDist,
    pub(crate) name: PackageName,
    pub(crate) version: Version,
    pub(crate) extra: Option<ExtraName>,
    pub(crate) dev: Option<GroupName>,
    pub(crate) hashes: HashDigests,
    pub(crate) metadata: Option<Metadata>,
    /// The "full" marker for this distribution. It precisely describes all
    /// marker environments for which this distribution _can_ be installed.
    /// That is, when doing a traversal over all of the distributions in a
    /// resolution, this marker corresponds to the disjunction of all paths to
    /// this distribution in the resolution graph.
    pub(crate) marker: UniversalMarker,
}

impl AnnotatedDist {
    /// Returns `true` if the [`AnnotatedDist`] is a base package (i.e., not an extra or a
    /// dependency group).
    pub(crate) fn is_base(&self) -> bool {
        self.extra.is_none() && self.dev.is_none()
    }

    /// Returns the [`IndexUrl`] of the distribution, if it is from a registry.
    pub(crate) fn index(&self) -> Option<&IndexUrl> {
        match &self.dist {
            ResolvedDist::Installed { .. } => None,
            ResolvedDist::Installable { dist, .. } => match dist.as_ref() {
                Dist::Built(dist) => match dist {
                    BuiltDist::Registry(dist) => Some(&dist.best_wheel().index),
                    BuiltDist::DirectUrl(_) => None,
                    BuiltDist::Path(_) => None,
                },
                Dist::Source(dist) => match dist {
                    SourceDist::Registry(dist) => Some(&dist.index),
                    SourceDist::DirectUrl(_) => None,
                    SourceDist::Git(_) => None,
                    SourceDist::Path(_) => None,
                    SourceDist::Directory(_) => None,
                },
            },
        }
    }
}

impl Name for AnnotatedDist {
    fn name(&self) -> &PackageName {
        self.dist.name()
    }
}

impl DistributionMetadata for AnnotatedDist {
    fn version_or_url(&self) -> VersionOrUrlRef {
        self.dist.version_or_url()
    }
}

impl Display for AnnotatedDist {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.dist, f)
    }
}
