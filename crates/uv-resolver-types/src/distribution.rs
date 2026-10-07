use std::fmt::Display;

use uv_distribution::Metadata;
use uv_distribution_types::{
    BuiltDist, Dist, DistributionMetadata, IndexUrl, Name, ResolvedDist, SourceDist,
    VersionOrUrlRef,
};
use uv_normalize::{ExtraName, GroupName, PackageName};
use uv_pep440::Version;
use uv_pypi_types::HashDigests;

use crate::UniversalMarker;

/// The part of a package represented by a resolver graph node.
#[derive(Debug, Clone, Default, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub enum PackageNodeKind {
    #[default]
    Base,
    Group(GroupName),
    Extra(ExtraName),
}

impl PackageNodeKind {
    pub fn extra(&self) -> Option<&ExtraName> {
        match self {
            Self::Extra(extra) => Some(extra),
            Self::Base | Self::Group(_) => None,
        }
    }

    pub fn group(&self) -> Option<&GroupName> {
        match self {
            Self::Group(group) => Some(group),
            Self::Base | Self::Extra(_) => None,
        }
    }

    pub fn is_base(&self) -> bool {
        match self {
            Self::Base => true,
            Self::Group(_) | Self::Extra(_) => false,
        }
    }
}

/// A pinned package with its resolved distribution and metadata. The [`ResolvedDist`] refers to a
/// specific distribution (e.g., a specific wheel), while the [`Metadata23`] refers to the metadata
/// for the package-version pair.
#[derive(Debug, Clone)]
pub struct AnnotatedDist {
    pub dist: ResolvedDist,
    pub name: PackageName,
    pub version: Version,
    pub kind: PackageNodeKind,
    pub hashes: HashDigests,
    pub metadata: Option<Metadata>,
    /// The "full" marker for this distribution. It precisely describes all
    /// marker environments for which this distribution _can_ be installed.
    /// That is, when doing a traversal over all of the distributions in a
    /// resolution, this marker corresponds to the disjunction of all paths to
    /// this distribution in the resolution graph.
    pub marker: UniversalMarker,
}

impl AnnotatedDist {
    /// Returns `true` if the [`AnnotatedDist`] is a base package (i.e., not an extra or a
    /// dependency group).
    pub(crate) fn is_base(&self) -> bool {
        self.kind.is_base()
    }

    /// Returns the [`IndexUrl`] of the distribution, if it is from a registry.
    pub fn index(&self) -> Option<&IndexUrl> {
        match &self.dist {
            ResolvedDist::Installed { .. } => None,
            ResolvedDist::Installable { dist, .. } => match dist.as_ref() {
                Dist::Built(dist) => match dist {
                    BuiltDist::Registry(dist) => Some(&dist.best_wheel().index),
                    BuiltDist::DirectUrl(_) => None,
                    BuiltDist::Path(_) => None,
                    BuiltDist::GitPath(_) => None,
                },
                Dist::Source(dist) => match dist {
                    SourceDist::Registry(dist) => Some(&dist.index),
                    SourceDist::DirectUrl(_) => None,
                    SourceDist::Path(_) => None,
                    SourceDist::Directory(_) => None,
                    SourceDist::GitPath(_) => None,
                    SourceDist::GitDirectory(_) => None,
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
    fn version_or_url(&self) -> VersionOrUrlRef<'_> {
        self.dist.version_or_url()
    }
}

impl Display for AnnotatedDist {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.dist, f)
    }
}
