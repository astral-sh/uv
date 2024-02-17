use std::borrow::Cow;

use anyhow::Result;

use pep508_rs::VerbatimUrl;
use uv_normalize::PackageName;

use crate::error::Error;
use crate::{
    BuiltDist, CachedDirectUrlDist, CachedDist, CachedRegistryDist, DirectUrlBuiltDist,
    DirectUrlSourceDist, Dist, DistributionId, GitSourceDist, InstalledDirectUrlDist,
    InstalledDist, InstalledRegistryDist, InstalledVersion, LocalDist, PackageId, PathBuiltDist,
    PathSourceDist, RegistryBuiltDist, RegistrySourceDist, ResourceId, SourceDist, VersionOrUrl,
};

pub trait Name {
    /// Return the normalized [`PackageName`] of the distribution.
    fn name(&self) -> &PackageName;
}

/// Metadata that can be resolved from a requirements specification alone (i.e., prior to building
/// or installing the distribution).
pub trait DistributionMetadata: Name {
    /// Return a [`pep440_rs::Version`], for registry-based distributions, or a [`url::Url`],
    /// for URL-based distributions.
    fn version_or_url(&self) -> VersionOrUrl;

    /// Returns a unique identifier for the package.
    ///
    /// Note that this is not equivalent to a unique identifier for the _distribution_, as multiple
    /// registry-based distributions (e.g., different wheels for the same package and version)
    /// will return the same package ID, but different distribution IDs.
    fn package_id(&self) -> PackageId {
        match self.version_or_url() {
            VersionOrUrl::Version(version) => {
                PackageId::from_registry(self.name().clone(), version.clone())
            }
            VersionOrUrl::Url(url) => PackageId::from_url(url),
        }
    }
}

/// Metadata that can be resolved from a built distribution.
pub trait InstalledMetadata: Name {
    /// Return the resolved version of the installed distribution.
    fn installed_version(&self) -> InstalledVersion;
}

pub trait RemoteSource {
    /// Return an appropriate filename for the distribution.
    fn filename(&self) -> Result<Cow<'_, str>, Error>;

    /// Return the size of the distribution, if known.
    fn size(&self) -> Option<u64>;
}

pub trait Identifier {
    /// Return a unique resource identifier for the distribution, like a SHA-256 hash of the
    /// distribution's contents.
    fn distribution_id(&self) -> DistributionId;

    /// Return a unique resource identifier for the underlying resource backing the distribution.
    ///
    /// This is often equivalent to the distribution ID, but may differ in some cases. For example,
    /// if the same Git repository is used for two different distributions, at two different
    /// subdirectories or two different commits, then those distributions would share a resource ID,
    /// but have different distribution IDs.
    fn resource_id(&self) -> ResourceId;
}

pub trait Verbatim {
    /// Return the verbatim representation of the distribution.
    fn verbatim(&self) -> Cow<'_, str>;
}

impl Verbatim for VerbatimUrl {
    fn verbatim(&self) -> Cow<'_, str> {
        if let Some(given) = self.given() {
            Cow::Borrowed(given)
        } else {
            Cow::Owned(self.to_string())
        }
    }
}

impl<T: DistributionMetadata> Verbatim for T {
    fn verbatim(&self) -> Cow<'_, str> {
        Cow::Owned(format!(
            "{}{}",
            self.name(),
            self.version_or_url().verbatim()
        ))
    }
}

// Implement `Display` for all known types that implement `Metadata`.
impl std::fmt::Display for LocalDist {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.name(), self.installed_version())
    }
}

impl std::fmt::Display for BuiltDist {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.name(), self.version_or_url())
    }
}

impl std::fmt::Display for CachedDist {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.name(), self.installed_version())
    }
}

impl std::fmt::Display for CachedDirectUrlDist {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.name(), self.installed_version())
    }
}

impl std::fmt::Display for CachedRegistryDist {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.name(), self.installed_version())
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
        write!(f, "{}{}", self.name(), self.installed_version())
    }
}

impl std::fmt::Display for InstalledDirectUrlDist {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.name(), self.installed_version())
    }
}

impl std::fmt::Display for InstalledRegistryDist {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.name(), self.installed_version())
    }
}

impl std::fmt::Display for PathBuiltDist {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.name(), self.version_or_url())
    }
}

impl std::fmt::Display for PathSourceDist {
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
