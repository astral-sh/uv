use std::fmt::Display;

use distribution_types::{DistributionMetadata, Name, ResolvedDist, VersionOrUrlRef};
use pep440_rs::Version;
use pypi_types::HashDigest;
use uv_distribution::Metadata;
use uv_normalize::{ExtraName, GroupName, PackageName};

pub use crate::resolution::display::{AnnotationStyle, DisplayResolutionGraph};
pub use crate::resolution::graph::ResolutionGraph;
pub(crate) use crate::resolution::graph::ResolutionGraphNode;
pub(crate) use crate::resolution::requirements_txt::RequirementsTxtDist;

mod display;
mod graph;
mod requirements_txt;

/// A pinned package with its resolved distribution and metadata. The [`ResolvedDist`] refers to a
/// specific distribution (e.g., a specific wheel), while the [`Metadata23`] refers to the metadata
/// for the package-version pair.
#[derive(Debug, Clone)]
pub(crate) struct AnnotatedDist {
    pub(crate) dist: ResolvedDist,
    pub(crate) version: Version,
    pub(crate) extra: Option<ExtraName>,
    pub(crate) dev: Option<GroupName>,
    pub(crate) hashes: Vec<HashDigest>,
    pub(crate) metadata: Metadata,
}

impl AnnotatedDist {
    /// Returns `true` if the [`AnnotatedDist`] is a base package (i.e., not an extra or a
    /// dependency group).
    pub(crate) fn is_base(&self) -> bool {
        self.extra.is_none() && self.dev.is_none()
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
