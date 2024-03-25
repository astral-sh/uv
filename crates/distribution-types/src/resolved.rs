use std::fmt::Display;

use pep508_rs::PackageName;

use crate::{
    Dist, DistributionId, DistributionMetadata, Identifier, InstalledDist, Name, ResourceId,
    VersionOrUrl,
};

/// A distribution that can be used for resolution and installation.
///
/// Either an already-installed distribution or a distribution that can be installed.
#[derive(Debug, Clone)]
pub enum ResolvedDist {
    Installed(InstalledDist),
    Installable(Dist),
}

/// A variant of [`ResolvedDist`] with borrowed inner distributions.
#[derive(Debug, Clone)]
pub enum ResolvedDistRef<'a> {
    Installed(&'a InstalledDist),
    Installable(&'a Dist),
}

impl ResolvedDist {
    /// Return true if the distribution is editable.
    pub fn is_editable(&self) -> bool {
        match self {
            Self::Installable(dist) => dist.is_editable(),
            Self::Installed(dist) => dist.is_editable(),
        }
    }
}

impl ResolvedDistRef<'_> {
    pub fn as_owned(&self) -> ResolvedDist {
        match self {
            Self::Installable(dist) => ResolvedDist::Installable((*dist).clone()),
            Self::Installed(dist) => ResolvedDist::Installed((*dist).clone()),
        }
    }
}

impl Name for ResolvedDistRef<'_> {
    fn name(&self) -> &PackageName {
        match self {
            Self::Installable(dist) => dist.name(),
            Self::Installed(dist) => dist.name(),
        }
    }
}

impl DistributionMetadata for ResolvedDistRef<'_> {
    fn version_or_url(&self) -> VersionOrUrl {
        match self {
            Self::Installed(installed) => VersionOrUrl::Version(installed.version()),
            Self::Installable(dist) => dist.version_or_url(),
        }
    }
}

impl Identifier for ResolvedDistRef<'_> {
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

impl Name for ResolvedDist {
    fn name(&self) -> &PackageName {
        match self {
            Self::Installable(dist) => dist.name(),
            Self::Installed(dist) => dist.name(),
        }
    }
}

impl DistributionMetadata for ResolvedDist {
    fn version_or_url(&self) -> VersionOrUrl {
        match self {
            Self::Installed(installed) => installed.version_or_url(),
            Self::Installable(dist) => dist.version_or_url(),
        }
    }
}

impl Identifier for ResolvedDist {
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

impl From<Dist> for ResolvedDist {
    fn from(value: Dist) -> Self {
        ResolvedDist::Installable(value)
    }
}

impl From<InstalledDist> for ResolvedDist {
    fn from(value: InstalledDist) -> Self {
        ResolvedDist::Installed(value)
    }
}

impl Display for ResolvedDist {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Installed(dist) => dist.fmt(f),
            Self::Installable(dist) => dist.fmt(f),
        }
    }
}
