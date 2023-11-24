use puffin_normalize::PackageName;

use crate::cached::CachedDist;
use crate::installed::InstalledDist;
use crate::traits::Metadata;
use crate::{Dist, VersionOrUrl};

/// A distribution which is either installable, is a wheel in our cache or is already installed.
#[derive(Debug, Clone)]
pub enum AnyDist {
    Remote(Dist),
    Cached(CachedDist),
    Installed(InstalledDist),
}

impl Metadata for AnyDist {
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

impl From<Dist> for AnyDist {
    fn from(dist: Dist) -> Self {
        Self::Remote(dist)
    }
}

impl From<CachedDist> for AnyDist {
    fn from(dist: CachedDist) -> Self {
        Self::Cached(dist)
    }
}

impl From<InstalledDist> for AnyDist {
    fn from(dist: InstalledDist) -> Self {
        Self::Installed(dist)
    }
}
