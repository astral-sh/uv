use std::fmt::{Display, Formatter};

use crate::{
    Dist, DistributionId, DistributionMetadata, Identifier, InstalledDist, Name, ResourceId,
    VersionOrUrlRef,
};
use uv_normalize::PackageName;
use uv_pep440::Version;

/// A distribution that can be requested during resolution.
///
/// Either an already-installed distribution or a distribution that can be installed.
#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum RequestedDist {
    Installed(InstalledDist),
    Installable(Dist),
}

impl RequestedDist {
    /// Returns the version of the distribution, if it is known.
    pub fn version(&self) -> Option<&Version> {
        match self {
            Self::Installed(dist) => Some(dist.version()),
            Self::Installable(dist) => dist.version(),
        }
    }
}

impl Name for RequestedDist {
    fn name(&self) -> &PackageName {
        match self {
            Self::Installable(dist) => dist.name(),
            Self::Installed(dist) => dist.name(),
        }
    }
}

impl DistributionMetadata for RequestedDist {
    fn version_or_url(&self) -> VersionOrUrlRef {
        match self {
            Self::Installed(dist) => dist.version_or_url(),
            Self::Installable(dist) => dist.version_or_url(),
        }
    }
}

impl Identifier for RequestedDist {
    fn distribution_id(&self) -> DistributionId {
        match self {
            Self::Installed(dist) => dist.distribution_id(),
            Self::Installable(dist) => dist.distribution_id(),
        }
    }

    fn resource_id(&self) -> ResourceId {
        match self {
            Self::Installed(dist) => dist.resource_id(),
            Self::Installable(dist) => dist.resource_id(),
        }
    }
}

impl Display for RequestedDist {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Installed(dist) => dist.fmt(f),
            Self::Installable(dist) => dist.fmt(f),
        }
    }
}
