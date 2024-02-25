use std::fmt::{Display, Formatter};

use url::Url;

use pep440_rs::Version;
use uv_normalize::PackageName;

/// A unique identifier for a package (e.g., `black==23.10.0`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PackageId {
    NameVersion(PackageName, Version),
    Url(String),
}

impl PackageId {
    /// Create a new [`PackageId`] from a package name and version.
    pub fn from_registry(name: PackageName, version: Version) -> Self {
        Self::NameVersion(name, version)
    }

    /// Create a new [`PackageId`] from a URL.
    pub fn from_url(url: &Url) -> Self {
        Self::Url(cache_key::digest(&cache_key::CanonicalUrl::new(url)))
    }
}

impl Display for PackageId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NameVersion(name, version) => write!(f, "{name}-{version}"),
            Self::Url(url) => write!(f, "{url}"),
        }
    }
}

/// A unique identifier for a distribution (e.g., `black-23.10.0-py3-none-any.whl`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DistributionId(String);

impl DistributionId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl DistributionId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A unique identifier for a resource, like a URL or a Git repository.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ResourceId(String);

impl ResourceId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl From<&Self> for PackageId {
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
