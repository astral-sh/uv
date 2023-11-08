use anyhow::Result;
use puffin_cache::CanonicalUrl;
use puffin_normalize::PackageName;

use crate::{
    AnyDistribution, BuiltDistribution, CachedDirectUrlDistribution, CachedDistribution,
    CachedRegistryDistribution, DirectUrlBuiltDistribution, DirectUrlSourceDistribution,
    Distribution, GitSourceDistribution, InstalledDirectUrlDistribution, InstalledDistribution,
    InstalledRegistryDistribution, RegistryBuiltDistribution, RegistrySourceDistribution,
    SourceDistribution, VersionOrUrl,
};

pub trait BaseDistribution {
    /// Return the normalized [`PackageName`] of the distribution.
    fn name(&self) -> &PackageName;

    /// Return a [`Version`], for registry-based distributions, or a [`Url`], for URL-based
    /// distributions.
    fn version_or_url(&self) -> VersionOrUrl;

    /// Returns a unique identifier for the package.
    ///
    /// Note that this is not equivalent to a unique identifier for the _distribution_, as multiple
    /// registry-based distributions (e.g., different wheels for the same package and version)
    /// will return the same package ID, but different distribution IDs.
    fn package_id(&self) -> String {
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

pub trait RemoteDistribution {
    /// Return an appropriate filename for the distribution.
    fn filename(&self) -> Result<&str>;

    /// Return the size of the distribution, if known.
    fn size(&self) -> Option<usize>;
}

pub trait DistributionIdentifier {
    /// Return a unique resource identifier for the distribution, like a SHA-256 hash of the
    /// distribution's contents.
    fn distribution_id(&self) -> String;

    /// Return a unique resource identifier for the underlying resource backing the distribution.
    ///
    /// This is often equivalent to the distribution ID, but may differ in some cases. For example,
    /// if the same Git repository is used for two different distributions, at two different
    /// subdirectories or two different commits, then those distributions would share a resource ID,
    /// but have different distribution IDs.
    fn resource_id(&self) -> String;
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

impl std::fmt::Display for InstalledDirectUrlDistribution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.name(), self.version_or_url())
    }
}

impl std::fmt::Display for InstalledRegistryDistribution {
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
