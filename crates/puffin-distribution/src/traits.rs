use anyhow::Result;
use puffin_cache::CanonicalUrl;
use puffin_normalize::PackageName;

use crate::{
    AnyDist, BuiltDist, CachedDirectUrlDist, CachedDist, CachedRegistryDist, DirectUrlBuiltDist,
    DirectUrlSourceDist, Dist, GitSourceDist, InstalledDirectUrlDist, InstalledDist,
    InstalledRegistryDist, RegistryBuiltDist, RegistrySourceDist, SourceDist, VersionOrUrl,
};

pub trait BaseDist {
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

pub trait RemoteDist {
    /// Return an appropriate filename for the distribution.
    fn filename(&self) -> Result<&str>;

    /// Return the size of the distribution, if known.
    fn size(&self) -> Option<usize>;
}

pub trait DistIdentifier {
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
impl std::fmt::Display for AnyDist {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.name(), self.version_or_url())
    }
}

impl std::fmt::Display for BuiltDist {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.name(), self.version_or_url())
    }
}

impl std::fmt::Display for CachedDist {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.name(), self.version_or_url())
    }
}

impl std::fmt::Display for CachedDirectUrlDist {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.name(), self.version_or_url())
    }
}

impl std::fmt::Display for CachedRegistryDist {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.name(), self.version_or_url())
    }
}

impl std::fmt::Display for DirectUrlBuiltDist {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.name(), self.version_or_url())
    }
}

impl std::fmt::Display for DirectUrlSourceDist {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.name(), self.version_or_url())
    }
}

impl std::fmt::Display for Dist {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.name(), self.version_or_url())
    }
}

impl std::fmt::Display for GitSourceDist {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.name(), self.version_or_url())
    }
}

impl std::fmt::Display for InstalledDist {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.name(), self.version_or_url())
    }
}

impl std::fmt::Display for InstalledDirectUrlDist {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.name(), self.version_or_url())
    }
}

impl std::fmt::Display for InstalledRegistryDist {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.name(), self.version_or_url())
    }
}

impl std::fmt::Display for RegistryBuiltDist {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.name(), self.version_or_url())
    }
}

impl std::fmt::Display for RegistrySourceDist {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.name(), self.version_or_url())
    }
}

impl std::fmt::Display for SourceDist {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.name(), self.version_or_url())
    }
}
