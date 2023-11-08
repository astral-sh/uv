use std::str::FromStr;

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
mod installed;
pub mod source;
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
pub enum Distribution {
    Built(BuiltDistribution),
    Source(SourceDistribution),
}

#[derive(Debug, Clone)]
pub enum BuiltDistribution {
    Registry(RegistryBuiltDistribution),
    DirectUrl(DirectUrlBuiltDistribution),
}

#[derive(Debug, Clone)]
pub enum SourceDistribution {
    Registry(RegistrySourceDistribution),
    DirectUrl(DirectUrlSourceDistribution),
    Git(GitSourceDistribution),
}

/// A built distribution (wheel) that exists in a registry, like `PyPI`.
#[derive(Debug, Clone)]
pub struct RegistryBuiltDistribution {
    pub name: PackageName,
    pub version: Version,
    pub file: File,
}

/// A built distribution (wheel) that exists at an arbitrary URL.
#[derive(Debug, Clone)]
pub struct DirectUrlBuiltDistribution {
    pub name: PackageName,
    pub url: Url,
}

/// A source distribution that exists in a registry, like `PyPI`.
#[derive(Debug, Clone)]
pub struct RegistrySourceDistribution {
    pub name: PackageName,
    pub version: Version,
    pub file: File,
}

/// A source distribution that exists at an arbitrary URL.
#[derive(Debug, Clone)]
pub struct DirectUrlSourceDistribution {
    pub name: PackageName,
    pub url: Url,
}

/// A source distribution that exists in a Git repository.
#[derive(Debug, Clone)]
pub struct GitSourceDistribution {
    pub name: PackageName,
    pub url: Url,
}

impl DistributionIdentifier for RegistryBuiltDistribution {
    fn name(&self) -> &PackageName {
        &self.name
    }

    fn version_or_url(&self) -> VersionOrUrl {
        VersionOrUrl::Version(&self.version)
    }
}

impl DistributionIdentifier for DirectUrlBuiltDistribution {
    fn name(&self) -> &PackageName {
        &self.name
    }

    fn version_or_url(&self) -> VersionOrUrl {
        VersionOrUrl::Url(&self.url)
    }
}

impl DistributionIdentifier for RegistrySourceDistribution {
    fn name(&self) -> &PackageName {
        &self.name
    }

    fn version_or_url(&self) -> VersionOrUrl {
        VersionOrUrl::Version(&self.version)
    }
}

impl DistributionIdentifier for DirectUrlSourceDistribution {
    fn name(&self) -> &PackageName {
        &self.name
    }

    fn version_or_url(&self) -> VersionOrUrl {
        VersionOrUrl::Url(&self.url)
    }
}

impl DistributionIdentifier for GitSourceDistribution {
    fn name(&self) -> &PackageName {
        &self.name
    }

    fn version_or_url(&self) -> VersionOrUrl {
        VersionOrUrl::Url(&self.url)
    }
}

impl DistributionIdentifier for SourceDistribution {
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

impl DistributionIdentifier for BuiltDistribution {
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

impl DistributionIdentifier for Distribution {
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

impl Distribution {
    /// Create a [`Distribution`] for a registry-based distribution.
    pub fn from_registry(name: PackageName, version: Version, file: File) -> Self {
        if file.filename.ends_with(".whl") {
            Self::Built(BuiltDistribution::Registry(RegistryBuiltDistribution {
                name,
                version,
                file,
            }))
        } else {
            Self::Source(SourceDistribution::Registry(RegistrySourceDistribution {
                name,
                version,
                file,
            }))
        }
    }

    /// Create a [`Distribution`] for a URL-based distribution.
    pub fn from_url(name: PackageName, url: Url) -> Self {
        if url.scheme().starts_with("git+") {
            Self::Source(SourceDistribution::Git(GitSourceDistribution { name, url }))
        } else if url.path().ends_with(".whl") {
            Self::Built(BuiltDistribution::DirectUrl(DirectUrlBuiltDistribution {
                name,
                url,
            }))
        } else {
            Self::Source(SourceDistribution::DirectUrl(DirectUrlSourceDistribution {
                name,
                url,
            }))
        }
    }
}
