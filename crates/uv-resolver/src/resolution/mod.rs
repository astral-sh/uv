use std::fmt::Display;

use distribution_types::{DistributionMetadata, Name, ResolvedDist, VersionOrUrlRef};
use pypi_types::{HashDigest, Metadata23};
use uv_normalize::{ExtraName, PackageName};

pub use crate::resolution::display::{AnnotationStyle, DisplayResolutionGraph};
pub use crate::resolution::graph::{Diagnostic, ResolutionGraph};

mod display;
mod graph;

/// A pinned package with its resolved distribution and metadata. The [`ResolvedDist`] refers to a
/// specific distribution (e.g., a specific wheel), while the [`Metadata23`] refers to the metadata
/// for the package-version pair.
#[derive(Debug)]
pub(crate) struct AnnotatedDist {
    pub(crate) dist: ResolvedDist,
    pub(crate) extras: Vec<ExtraName>,
    pub(crate) hashes: Vec<HashDigest>,
    pub(crate) metadata: Metadata23,
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
