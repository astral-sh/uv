use std::hash::Hash;

use uv_cache_key::CanonicalUrl;
use uv_normalize::PackageName;
use uv_pep440::Version;

use crate::cached::CachedDist;
use crate::installed::InstalledDist;
use crate::{InstalledMetadata, InstalledVersion, Name};

/// A distribution which is either installable, is a wheel in our cache or is already installed.
///
/// Note equality and hash operations are only based on the name and canonical version, not the
/// kind.
#[derive(Debug, Clone, Eq)]
#[allow(clippy::large_enum_variant)]
pub enum LocalDist {
    Cached(CachedDist, CanonicalVersion),
    Installed(InstalledDist, CanonicalVersion),
}

impl LocalDist {
    fn canonical_version(&self) -> &CanonicalVersion {
        match self {
            Self::Cached(_, version) => version,
            Self::Installed(_, version) => version,
        }
    }
}

impl Name for LocalDist {
    fn name(&self) -> &PackageName {
        match self {
            Self::Cached(dist, _) => dist.name(),
            Self::Installed(dist, _) => dist.name(),
        }
    }
}

impl InstalledMetadata for LocalDist {
    fn installed_version(&self) -> InstalledVersion {
        match self {
            Self::Cached(dist, _) => dist.installed_version(),
            Self::Installed(dist, _) => dist.installed_version(),
        }
    }
}

impl From<CachedDist> for LocalDist {
    fn from(dist: CachedDist) -> Self {
        let version = CanonicalVersion::from(dist.installed_version());
        Self::Cached(dist, version)
    }
}

impl From<InstalledDist> for LocalDist {
    fn from(dist: InstalledDist) -> Self {
        let version = CanonicalVersion::from(dist.installed_version());
        Self::Installed(dist, version)
    }
}

impl Hash for LocalDist {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.name().hash(state);
        self.canonical_version().hash(state);
    }
}

impl PartialEq for LocalDist {
    fn eq(&self, other: &Self) -> bool {
        self.name() == other.name() && self.canonical_version() == other.canonical_version()
    }
}

/// Like [`InstalledVersion`], but with [`CanonicalUrl`] to ensure robust URL comparisons.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CanonicalVersion {
    Version(Version),
    Url(CanonicalUrl, Version),
}

impl From<InstalledVersion<'_>> for CanonicalVersion {
    fn from(installed_version: InstalledVersion<'_>) -> Self {
        match installed_version {
            InstalledVersion::Version(version) => Self::Version(version.clone()),
            InstalledVersion::Url(url, version) => {
                Self::Url(CanonicalUrl::new(url), version.clone())
            }
        }
    }
}
