use uv_normalize::PackageName;

use crate::cached::CachedDist;
use crate::installed::InstalledDist;
use crate::{InstalledMetadata, InstalledVersion, Name};

/// A distribution which is either installable, is a wheel in our cache or is already installed.
#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum LocalDist {
    Cached(CachedDist),
    Installed(InstalledDist),
}

impl Name for LocalDist {
    fn name(&self) -> &PackageName {
        match self {
            Self::Cached(dist) => dist.name(),
            Self::Installed(dist) => dist.name(),
        }
    }
}

impl InstalledMetadata for LocalDist {
    fn installed_version(&self) -> InstalledVersion {
        match self {
            Self::Cached(dist) => dist.installed_version(),
            Self::Installed(dist) => dist.installed_version(),
        }
    }
}

impl From<CachedDist> for LocalDist {
    fn from(dist: CachedDist) -> Self {
        Self::Cached(dist)
    }
}

impl From<InstalledDist> for LocalDist {
    fn from(dist: InstalledDist) -> Self {
        Self::Installed(dist)
    }
}
