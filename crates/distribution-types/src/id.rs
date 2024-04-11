use std::fmt::{Display, Formatter};
use std::path::PathBuf;

use cache_key::{CanonicalUrl, RepositoryUrl};
use url::Url;

use pep440_rs::Version;
use pypi_types::HashDigest;
use uv_normalize::PackageName;

/// A unique identifier for a package. A package can either be identified by a name (e.g., `black`)
/// or a URL (e.g., `git+https://github.com/psf/black`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PackageId {
    /// The identifier consists of a package name.
    Name(PackageName),
    /// The identifier consists of a URL.
    Url(CanonicalUrl),
}

impl PackageId {
    /// Create a new [`PackageId`] from a package name and version.
    pub fn from_registry(name: PackageName) -> Self {
        Self::Name(name)
    }

    /// Create a new [`PackageId`] from a URL.
    pub fn from_url(url: &Url) -> Self {
        Self::Url(CanonicalUrl::new(url))
    }
}

impl Display for PackageId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Name(name) => write!(f, "{name}"),
            Self::Url(url) => write!(f, "{url}"),
        }
    }
}

/// A unique identifier for a package at a specific version (e.g., `black==23.10.0`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum VersionId {
    /// The identifier consists of a package name and version.
    NameVersion(PackageName, Version),
    /// The identifier consists of a URL.
    Url(CanonicalUrl),
}

impl VersionId {
    /// Create a new [`VersionId`] from a package name and version.
    pub fn from_registry(name: PackageName, version: Version) -> Self {
        Self::NameVersion(name, version)
    }

    /// Create a new [`VersionId`] from a URL.
    pub fn from_url(url: &Url) -> Self {
        Self::Url(CanonicalUrl::new(url))
    }
}

impl Display for VersionId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NameVersion(name, version) => write!(f, "{name}-{version}"),
            Self::Url(url) => write!(f, "{url}"),
        }
    }
}

/// A unique resource identifier for the distribution, like a SHA-256 hash of the distribution's
/// contents.
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
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DistributionId {
    Url(CanonicalUrl),
    PathBuf(PathBuf),
    Digest(HashDigest),
    AbsoluteUrl(String),
    RelativeUrl(String, String),
}

/// A unique identifier for a resource, like a URL or a Git repository.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ResourceId {
    Url(RepositoryUrl),
    PathBuf(PathBuf),
    Digest(HashDigest),
    AbsoluteUrl(String),
    RelativeUrl(String, String),
}

impl From<&Self> for VersionId {
    /// Required for `WaitMap::wait`.
    fn from(value: &Self) -> Self {
        value.clone()
    }
}

impl From<&Self> for DistributionId {
    /// Required for `WaitMap::wait`.
    fn from(value: &Self) -> Self {
        value.clone()
    }
}

impl From<&Self> for ResourceId {
    /// Required for `WaitMap::wait`.
    fn from(value: &Self) -> Self {
        value.clone()
    }
}
