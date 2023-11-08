use puffin_cache::CanonicalUrl;
use puffin_normalize::PackageName;

use crate::{AnyDistribution, BuiltDistribution, CachedDistribution, CachedDirectUrlDistribution, CachedRegistryDistribution, DirectUrlBuiltDistribution, DirectUrlSourceDistribution, Distribution, GitSourceDistribution, InstalledDistribution, RegistryBuiltDistribution, RegistrySourceDistribution, SourceDistribution, VersionOrUrl};

pub trait DistributionIdentifier {
    /// Return the normalized [`PackageName`] of the distribution.
    fn name(&self) -> &PackageName;

    /// Return a [`Version`], for registry-based distributions, or a [`Url`], for URL-based
    /// distributions.
    fn version_or_url(&self) -> VersionOrUrl;

    /// Returns a unique identifier for this distribution.
    fn id(&self) -> String {
        match self.version_or_url() {
            VersionOrUrl::Version(version) => {
                // https://packaging.python.org/en/latest/specifications/recording-installed-packages/#the-dist-info-directory
                // `version` is normalized by its `ToString` impl
                format!("{}-{}", self.name().as_dist_info_name(), version)
            }
            VersionOrUrl::Url(url) => puffin_cache::digest(&CanonicalUrl::new(url)),
        }
    }
}


// Implement `Display` for all known types that implement `DistributionIdentifier`.
impl std::fmt::Display for AnyDistribution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.name(), self.version_or_url())
    }
}

impl std::fmt::Display for BuiltDistribution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.name(), self.version_or_url())
    }
}

impl std::fmt::Display for CachedDistribution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.name(), self.version_or_url())
    }
}

impl std::fmt::Display for CachedDirectUrlDistribution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.name(), self.version_or_url())
    }
}

impl std::fmt::Display for CachedRegistryDistribution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.name(), self.version_or_url())
    }
}

impl std::fmt::Display for DirectUrlBuiltDistribution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.name(), self.version_or_url())
    }
}

impl std::fmt::Display for DirectUrlSourceDistribution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.name(), self.version_or_url())
    }
}

impl std::fmt::Display for Distribution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.name(), self.version_or_url())
    }
}

impl std::fmt::Display for GitSourceDistribution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.name(), self.version_or_url())
    }
}

impl std::fmt::Display for InstalledDistribution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.name(), self.version_or_url())
    }
}

impl std::fmt::Display for RegistryBuiltDistribution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.name(), self.version_or_url())
    }
}

impl std::fmt::Display for RegistrySourceDistribution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.name(), self.version_or_url())
    }
}

impl std::fmt::Display for SourceDistribution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.name(), self.version_or_url())
    }
}


