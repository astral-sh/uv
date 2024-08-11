use uv_normalize::PackageName;

use crate::cached::CachedDist;
use crate::installed::InstalledDist;
use crate::{InstalledMetadata, InstalledVersion, Name};

/// A distribution which is either installable, is a wheel in our cache or is already installed.
#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum LocalDist<'a> {
    Cached(&'a CachedDist),
    Installed(&'a InstalledDist),
}

impl Name for LocalDist<'_> {
    fn name(&self) -> &PackageName {
        match self {
            Self::Cached(dist) => dist.name(),
            Self::Installed(dist) => dist.name(),
        }
    }
}

impl InstalledMetadata for LocalDist<'_> {
    fn installed_version(&self) -> InstalledVersion {
        match self {
            Self::Cached(dist) => dist.installed_version(),
            Self::Installed(dist) => dist.installed_version(),
        }
    }
}

impl<'a> From<&'a CachedDist> for LocalDist<'a> {
    fn from(dist: &'a CachedDist) -> Self {
        Self::Cached(dist)
    }
}

impl<'a> From<&'a InstalledDist> for LocalDist<'a> {
    fn from(dist: &'a InstalledDist) -> Self {
        Self::Installed(dist)
    }
}
