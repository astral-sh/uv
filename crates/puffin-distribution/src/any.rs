use puffin_normalize::PackageName;

use crate::cached::CachedDistribution;
use crate::installed::InstalledDistribution;
use crate::traits::BaseDistribution;
use crate::{Distribution, VersionOrUrl};

/// A distribution which either exists remotely or locally.
#[derive(Debug, Clone)]
pub enum AnyDistribution {
    Remote(Distribution),
    Cached(CachedDistribution),
    Installed(InstalledDistribution),
}

impl BaseDistribution for AnyDistribution {
    fn name(&self) -> &PackageName {
        match self {
            Self::Remote(dist) => dist.name(),
            Self::Cached(dist) => dist.name(),
            Self::Installed(dist) => dist.name(),
        }
    }

    fn version_or_url(&self) -> VersionOrUrl {
        match self {
            Self::Remote(dist) => dist.version_or_url(),
            Self::Cached(dist) => dist.version_or_url(),
            Self::Installed(dist) => dist.version_or_url(),
        }
    }
}

impl From<Distribution> for AnyDistribution {
    fn from(dist: Distribution) -> Self {
        Self::Remote(dist)
    }
}

impl From<CachedDistribution> for AnyDistribution {
    fn from(dist: CachedDistribution) -> Self {
        Self::Cached(dist)
    }
}

impl From<InstalledDistribution> for AnyDistribution {
    fn from(dist: InstalledDistribution) -> Self {
        Self::Installed(dist)
    }
}
