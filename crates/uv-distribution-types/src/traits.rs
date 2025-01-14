use std::borrow::Cow;

use uv_normalize::PackageName;
use uv_pep508::VerbatimUrl;

use crate::error::Error;
use crate::{
    BuiltDist, CachedDirectUrlDist, CachedDist, CachedRegistryDist, DirectUrlBuiltDist,
    DirectUrlSourceDist, DirectorySourceDist, Dist, DistributionId, GitSourceDist,
    InstalledDirectUrlDist, InstalledDist, InstalledEggInfoDirectory, InstalledEggInfoFile,
    InstalledLegacyEditable, InstalledRegistryDist, InstalledVersion, LocalDist, PackageId,
    PathBuiltDist, PathSourceDist, RegistryBuiltWheel, RegistrySourceDist, ResourceId, SourceDist,
    VersionId, VersionOrUrlRef,
};

pub trait Name {
    /// Return the normalized [`PackageName`] of the distribution.
    fn name(&self) -> &PackageName;
}

/// Metadata that can be resolved from a requirements specification alone (i.e., prior to building
/// or installing the distribution).
pub trait DistributionMetadata: Name {
    /// Return a [`uv_pep440::Version`], for registry-based distributions, or a [`url::Url`],
    /// for URL-based distributions.
    fn version_or_url(&self) -> VersionOrUrlRef;

    /// Returns a unique identifier for the package at the given version (e.g., `black==23.10.0`).
    ///
    /// Note that this is not equivalent to a unique identifier for the _distribution_, as multiple
    /// registry-based distributions (e.g., different wheels for the same package and version)
    /// will return the same version ID, but different distribution IDs.
    fn version_id(&self) -> VersionId {
        match self.version_or_url() {
            VersionOrUrlRef::Version(version) => {
                VersionId::from_registry(self.name().clone(), version.clone())
            }
            VersionOrUrlRef::Url(url) => VersionId::from_url(url),
        }
    }

    /// Returns a unique identifier for a package. A package can either be identified by a name
    /// (e.g., `black`) or a URL (e.g., `git+https://github.com/psf/black`).
    ///
    /// Note that this is not equivalent to a unique identifier for the _distribution_, as multiple
    /// registry-based distributions (e.g., different wheels for the same package and version)
    /// will return the same version ID, but different distribution IDs.
    fn package_id(&self) -> PackageId {
        match self.version_or_url() {
            VersionOrUrlRef::Version(_) => PackageId::from_registry(self.name().clone()),
            VersionOrUrlRef::Url(url) => PackageId::from_url(url),
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
    ///
    /// A distribution is a specific archive of a package at a specific version. For a given package
    /// version, there may be multiple distributions, e.g., source distribution, along with
    /// multiple binary distributions (wheels) for different platforms. As a concrete example,
    /// `black-23.10.0-py3-none-any.whl` would represent a (binary) distribution of the `black` package
    /// at version `23.10.0`.
    ///
    /// The distribution ID is used to uniquely identify a distribution. Ideally, the distribution
    /// ID should be a hash of the distribution's contents, though in practice, it's only required
    /// that the ID is unique within a single invocation of the resolver (and so, e.g., a hash of
    /// the URL would also be sufficient).
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

impl std::fmt::Display for InstalledEggInfoFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.name(), self.installed_version())
    }
}

impl std::fmt::Display for InstalledEggInfoDirectory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.name(), self.installed_version())
    }
}

impl std::fmt::Display for InstalledLegacyEditable {
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

impl std::fmt::Display for DirectorySourceDist {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.name(), self.version_or_url())
    }
}

impl std::fmt::Display for RegistryBuiltWheel {
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
