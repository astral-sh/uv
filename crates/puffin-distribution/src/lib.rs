use std::path::Path;

use anyhow::{Context, Result};
use url::Url;

use pep440_rs::Version;
use puffin_normalize::PackageName;
use pypi_types::File;

pub use crate::any::*;
pub use crate::cached::*;
pub use crate::installed::*;
pub use crate::traits::*;

mod any;
mod cached;
pub mod direct_url;
mod installed;
mod traits;

#[derive(Debug, Clone)]
pub enum VersionOrUrl<'a> {
    /// A PEP 440 version specifier, used to identify a distribution in a registry.
    Version(&'a Version),
    /// A URL, used to identify a distribution at an arbitrary location.
    Url(&'a Url),
}

impl std::fmt::Display for VersionOrUrl<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VersionOrUrl::Version(version) => write!(f, "=={version}"),
            VersionOrUrl::Url(url) => write!(f, " @ {url}"),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Dist {
    Built(BuiltDist),
    Source(SourceDist),
}

#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum BuiltDist {
    Registry(RegistryBuiltDist),
    DirectUrl(DirectUrlBuiltDist),
}

#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum SourceDist {
    Registry(RegistrySourceDist),
    DirectUrl(DirectUrlSourceDist),
    Git(GitSourceDist),
}

/// A built distribution (wheel) that exists in a registry, like `PyPI`.
#[derive(Debug, Clone)]
pub struct RegistryBuiltDist {
    pub name: PackageName,
    pub version: Version,
    pub file: File,
}

/// A built distribution (wheel) that exists at an arbitrary URL.
#[derive(Debug, Clone)]
pub struct DirectUrlBuiltDist {
    pub name: PackageName,
    pub url: Url,
}

/// A source distribution that exists in a registry, like `PyPI`.
#[derive(Debug, Clone)]
pub struct RegistrySourceDist {
    pub name: PackageName,
    pub version: Version,
    pub file: File,
}

/// A source distribution that exists at an arbitrary URL.
#[derive(Debug, Clone)]
pub struct DirectUrlSourceDist {
    pub name: PackageName,
    pub url: Url,
}

/// A source distribution that exists in a Git repository.
#[derive(Debug, Clone)]
pub struct GitSourceDist {
    pub name: PackageName,
    pub url: Url,
}

impl Dist {
    /// Create a [`Dist`] for a registry-based distribution.
    pub fn from_registry(name: PackageName, version: Version, file: File) -> Self {
        if Path::new(&file.filename)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("whl"))
        {
            Self::Built(BuiltDist::Registry(RegistryBuiltDist {
                name,
                version,
                file,
            }))
        } else {
            Self::Source(SourceDist::Registry(RegistrySourceDist {
                name,
                version,
                file,
            }))
        }
    }

    /// Create a [`Dist`] for a URL-based distribution.
    pub fn from_url(name: PackageName, url: Url) -> Self {
        if url.scheme().starts_with("git+") {
            Self::Source(SourceDist::Git(GitSourceDist { name, url }))
        } else if Path::new(url.path())
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("whl"))
        {
            Self::Built(BuiltDist::DirectUrl(DirectUrlBuiltDist { name, url }))
        } else {
            Self::Source(SourceDist::DirectUrl(DirectUrlSourceDist { name, url }))
        }
    }
}

impl BaseDist for RegistryBuiltDist {
    fn name(&self) -> &PackageName {
        &self.name
    }

    fn version_or_url(&self) -> VersionOrUrl {
        VersionOrUrl::Version(&self.version)
    }
}

impl BaseDist for DirectUrlBuiltDist {
    fn name(&self) -> &PackageName {
        &self.name
    }

    fn version_or_url(&self) -> VersionOrUrl {
        VersionOrUrl::Url(&self.url)
    }
}

impl BaseDist for RegistrySourceDist {
    fn name(&self) -> &PackageName {
        &self.name
    }

    fn version_or_url(&self) -> VersionOrUrl {
        VersionOrUrl::Version(&self.version)
    }
}

impl BaseDist for DirectUrlSourceDist {
    fn name(&self) -> &PackageName {
        &self.name
    }

    fn version_or_url(&self) -> VersionOrUrl {
        VersionOrUrl::Url(&self.url)
    }
}

impl BaseDist for GitSourceDist {
    fn name(&self) -> &PackageName {
        &self.name
    }

    fn version_or_url(&self) -> VersionOrUrl {
        VersionOrUrl::Url(&self.url)
    }
}

impl BaseDist for SourceDist {
    fn name(&self) -> &PackageName {
        match self {
            Self::Registry(dist) => dist.name(),
            Self::DirectUrl(dist) => dist.name(),
            Self::Git(dist) => dist.name(),
        }
    }

    fn version_or_url(&self) -> VersionOrUrl {
        match self {
            Self::Registry(dist) => dist.version_or_url(),
            Self::DirectUrl(dist) => dist.version_or_url(),
            Self::Git(dist) => dist.version_or_url(),
        }
    }
}

impl BaseDist for BuiltDist {
    fn name(&self) -> &PackageName {
        match self {
            Self::Registry(dist) => dist.name(),
            Self::DirectUrl(dist) => dist.name(),
        }
    }

    fn version_or_url(&self) -> VersionOrUrl {
        match self {
            Self::Registry(dist) => dist.version_or_url(),
            Self::DirectUrl(dist) => dist.version_or_url(),
        }
    }
}

impl BaseDist for Dist {
    fn name(&self) -> &PackageName {
        match self {
            Self::Built(dist) => dist.name(),
            Self::Source(dist) => dist.name(),
        }
    }

    fn version_or_url(&self) -> VersionOrUrl {
        match self {
            Self::Built(dist) => dist.version_or_url(),
            Self::Source(dist) => dist.version_or_url(),
        }
    }
}

impl RemoteDist for RegistryBuiltDist {
    fn filename(&self) -> Result<&str> {
        Ok(&self.file.filename)
    }

    fn size(&self) -> Option<usize> {
        Some(self.file.size)
    }
}

impl RemoteDist for RegistrySourceDist {
    fn filename(&self) -> Result<&str> {
        Ok(&self.file.filename)
    }

    fn size(&self) -> Option<usize> {
        Some(self.file.size)
    }
}

impl RemoteDist for DirectUrlBuiltDist {
    fn filename(&self) -> Result<&str> {
        self.url
            .path_segments()
            .and_then(Iterator::last)
            .map(|filename| {
                filename
                    .rsplit_once('@')
                    .map_or(filename, |(_, filename)| filename)
            })
            .with_context(|| format!("Could not parse filename from URL: {}", self.url))
    }

    fn size(&self) -> Option<usize> {
        None
    }
}

impl RemoteDist for DirectUrlSourceDist {
    fn filename(&self) -> Result<&str> {
        self.url
            .path_segments()
            .and_then(Iterator::last)
            .map(|filename| {
                filename
                    .rsplit_once('@')
                    .map_or(filename, |(_, filename)| filename)
            })
            .with_context(|| format!("Could not parse filename from URL: {}", self.url))
    }

    fn size(&self) -> Option<usize> {
        None
    }
}

impl RemoteDist for GitSourceDist {
    fn filename(&self) -> Result<&str> {
        self.url
            .path_segments()
            .and_then(Iterator::last)
            .map(|filename| {
                filename
                    .rsplit_once('@')
                    .map_or(filename, |(_, filename)| filename)
            })
            .with_context(|| format!("Could not parse filename from URL: {}", self.url))
    }

    fn size(&self) -> Option<usize> {
        None
    }
}

impl RemoteDist for SourceDist {
    fn filename(&self) -> Result<&str> {
        match self {
            Self::Registry(dist) => dist.filename(),
            Self::DirectUrl(dist) => dist.filename(),
            Self::Git(dist) => dist.filename(),
        }
    }

    fn size(&self) -> Option<usize> {
        match self {
            Self::Registry(dist) => dist.size(),
            Self::DirectUrl(dist) => dist.size(),
            Self::Git(dist) => dist.size(),
        }
    }
}

impl RemoteDist for BuiltDist {
    fn filename(&self) -> Result<&str> {
        match self {
            Self::Registry(dist) => dist.filename(),
            Self::DirectUrl(dist) => dist.filename(),
        }
    }

    fn size(&self) -> Option<usize> {
        match self {
            Self::Registry(dist) => dist.size(),
            Self::DirectUrl(dist) => dist.size(),
        }
    }
}

impl RemoteDist for Dist {
    fn filename(&self) -> Result<&str> {
        match self {
            Self::Built(dist) => dist.filename(),
            Self::Source(dist) => dist.filename(),
        }
    }

    fn size(&self) -> Option<usize> {
        match self {
            Self::Built(dist) => dist.size(),
            Self::Source(dist) => dist.size(),
        }
    }
}

impl DistIdentifier for Url {
    fn distribution_id(&self) -> String {
        puffin_cache::digest(&puffin_cache::CanonicalUrl::new(self))
    }

    fn resource_id(&self) -> String {
        puffin_cache::digest(&puffin_cache::RepositoryUrl::new(self))
    }
}

impl DistIdentifier for File {
    fn distribution_id(&self) -> String {
        self.hashes.sha256.clone()
    }

    fn resource_id(&self) -> String {
        self.hashes.sha256.clone()
    }
}

impl DistIdentifier for RegistryBuiltDist {
    fn distribution_id(&self) -> String {
        self.file.distribution_id()
    }

    fn resource_id(&self) -> String {
        self.file.resource_id()
    }
}

impl DistIdentifier for RegistrySourceDist {
    fn distribution_id(&self) -> String {
        self.file.distribution_id()
    }

    fn resource_id(&self) -> String {
        self.file.resource_id()
    }
}

impl DistIdentifier for DirectUrlBuiltDist {
    fn distribution_id(&self) -> String {
        self.url.distribution_id()
    }

    fn resource_id(&self) -> String {
        self.url.resource_id()
    }
}

impl DistIdentifier for DirectUrlSourceDist {
    fn distribution_id(&self) -> String {
        self.url.distribution_id()
    }

    fn resource_id(&self) -> String {
        self.url.resource_id()
    }
}

impl DistIdentifier for GitSourceDist {
    fn distribution_id(&self) -> String {
        self.url.distribution_id()
    }

    fn resource_id(&self) -> String {
        self.url.resource_id()
    }
}

impl DistIdentifier for SourceDist {
    fn distribution_id(&self) -> String {
        match self {
            Self::Registry(dist) => dist.distribution_id(),
            Self::DirectUrl(dist) => dist.distribution_id(),
            Self::Git(dist) => dist.distribution_id(),
        }
    }

    fn resource_id(&self) -> String {
        match self {
            Self::Registry(dist) => dist.resource_id(),
            Self::DirectUrl(dist) => dist.resource_id(),
            Self::Git(dist) => dist.resource_id(),
        }
    }
}

impl DistIdentifier for BuiltDist {
    fn distribution_id(&self) -> String {
        match self {
            Self::Registry(dist) => dist.distribution_id(),
            Self::DirectUrl(dist) => dist.distribution_id(),
        }
    }

    fn resource_id(&self) -> String {
        match self {
            Self::Registry(dist) => dist.resource_id(),
            Self::DirectUrl(dist) => dist.resource_id(),
        }
    }
}

impl DistIdentifier for Dist {
    fn distribution_id(&self) -> String {
        match self {
            Self::Built(dist) => dist.distribution_id(),
            Self::Source(dist) => dist.distribution_id(),
        }
    }

    fn resource_id(&self) -> String {
        match self {
            Self::Built(dist) => dist.resource_id(),
            Self::Source(dist) => dist.resource_id(),
        }
    }
}
