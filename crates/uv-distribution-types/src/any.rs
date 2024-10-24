use std::hash::Hash;

use uv_normalize::PackageName;

use crate::cached::CachedDist;
use crate::installed::InstalledDist;
use crate::{InstalledMetadata, InstalledVersion, Name};

/// A distribution which is either installable, is a wheel in our cache or is already installed.
///
/// Note equality and hash operations are only based on the name and version, not the kind.
#[derive(Debug, Clone, Eq)]
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

impl Hash for LocalDist {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.name().hash(state);
        self.installed_version().hash(state);
    }
}

impl PartialEq for LocalDist {
    fn eq(&self, other: &Self) -> bool {
        self.name() == other.name() && self.installed_version() == other.installed_version()
    }
}
