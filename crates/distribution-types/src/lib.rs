//! ## Type hierarchy
//!
//! When we receive the requirements from `pip-sync`, we check which requirements already fulfilled
//! in the users environment ([`InstalledDist`]), whether the matching package is in our wheel cache
//! ([`CachedDist`]) or whether we need to download, (potentially build) and install it ([`Dist`]).
//! These three variants make up [`BuiltDist`].
//!
//! ## `Dist`
//! A [`Dist`] is either a built distribution (a wheel), or a source distribution that exists at
//! some location. We translate every PEP 508 requirement e.g. from `requirements.txt` or from
//! `pyproject.toml`'s `[project] dependencies` into a [`Dist`] by checking each index.
//! * [`BuiltDist`]: A wheel, with its three possible origins:
//!   * [`RegistryBuiltDist`]
//!   * [`DirectUrlBuiltDist`]
//!   * [`PathBuiltDist`]
//! * [`SourceDist`]: A source distribution, with its four possible origins:
//!   * [`RegistrySourceDist`]
//!   * [`DirectUrlSourceDist`]
//!   * [`GitSourceDist`]
//!   * [`PathSourceDist`]
//!
//! ## `CachedDist`
//! A [`CachedDist`] is a built distribution (wheel) that exists in the local cache, with the two
//! possible origins we currently track:
//! * [`CachedRegistryDist`]
//! * [`CachedDirectUrlDist`]
//!
//! TODO(konstin): Track all kinds from [`Dist`].
//!
//! ## `InstalledDist`
//! An [`InstalledDist`] is built distribution (wheel) that is installed in a virtual environment,
//! with the two possible origins we currently track:
//! * [`InstalledRegistryDist`]
//! * [`InstalledDirectUrlDist`]
//!
//! Since we read this information from [`direct_url.json`](https://packaging.python.org/en/latest/specifications/direct-url-data-structure/), it doesn't match the information [`Dist`] exactly.
//!
//! TODO(konstin): Track all kinds from [`Dist`].
use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::Result;
use url::Url;

use distribution_filename::WheelFilename;
use pep440_rs::Version;
use pep508_rs::VerbatimUrl;
use puffin_normalize::PackageName;
use pypi_types::BaseUrl;
use requirements_txt::EditableRequirement;

pub use crate::any::*;
pub use crate::cached::*;
pub use crate::direct_url::*;
pub use crate::editable::*;
pub use crate::error::*;
pub use crate::file::*;
pub use crate::id::*;
pub use crate::index_url::*;
pub use crate::installed::*;
pub use crate::resolution::*;
pub use crate::traits::*;

mod any;
mod cached;
mod direct_url;
mod editable;
mod error;
mod file;
mod id;
mod index_url;
mod installed;
mod resolution;
mod traits;

#[derive(Debug, Clone)]
pub enum VersionOrUrl<'a> {
    /// A PEP 440 version specifier, used to identify a distribution in a registry.
    Version(&'a Version),
    /// A URL, used to identify a distribution at an arbitrary location.
    Url(&'a VerbatimUrl),
}

impl Verbatim for VersionOrUrl<'_> {
    fn verbatim(&self) -> Cow<'_, str> {
        match self {
            VersionOrUrl::Version(version) => Cow::Owned(format!("=={version}")),
            VersionOrUrl::Url(url) => Cow::Owned(format!(" @ {}", url.verbatim())),
        }
    }
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
pub enum InstalledVersion<'a> {
    /// A PEP 440 version specifier, used to identify a distribution in a registry.
    Version(&'a Version),
    /// A URL, used to identify a distribution at an arbitrary location, along with the version
    /// specifier to which it resolved.
    Url(&'a Url, &'a Version),
}

impl std::fmt::Display for InstalledVersion<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InstalledVersion::Version(version) => write!(f, "=={version}"),
            InstalledVersion::Url(url, version) => write!(f, "=={version} (from {url})"),
        }
    }
}

/// Either a built distribution, a wheel, or a source distribution that exists at some location
///
/// The location can be index, url or path (wheel) or index, url, path or git (source distribution)
#[derive(Debug, Clone)]
pub enum Dist {
    Built(BuiltDist),
    Source(SourceDist),
}

/// A wheel, with its three possible origins (index, url, path)
#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum BuiltDist {
    Registry(RegistryBuiltDist),
    DirectUrl(DirectUrlBuiltDist),
    Path(PathBuiltDist),
}

/// A source distribution, with its three possible origins (index, url, path, git)
#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum SourceDist {
    Registry(RegistrySourceDist),
    DirectUrl(DirectUrlSourceDist),
    Git(GitSourceDist),
    Path(PathSourceDist),
}

/// A built distribution (wheel) that exists in a registry, like `PyPI`.
#[derive(Debug, Clone)]
pub struct RegistryBuiltDist {
    pub name: PackageName,
    pub version: Version,
    pub file: File,
    pub index: IndexUrl,
    pub base: BaseUrl,
}

/// A built distribution (wheel) that exists at an arbitrary URL.
#[derive(Debug, Clone)]
pub struct DirectUrlBuiltDist {
    /// We require that wheel urls end in the full wheel filename, e.g.
    /// `https://example.org/packages/flask-3.0.0-py3-none-any.whl`
    pub filename: WheelFilename,
    pub url: VerbatimUrl,
}

/// A built distribution (wheel) that exists in a local directory.
#[derive(Debug, Clone)]
pub struct PathBuiltDist {
    pub filename: WheelFilename,
    pub url: VerbatimUrl,
    pub path: PathBuf,
}

/// A source distribution that exists in a registry, like `PyPI`.
#[derive(Debug, Clone)]
pub struct RegistrySourceDist {
    pub name: PackageName,
    pub version: Version,
    pub file: File,
    pub index: IndexUrl,
    pub base: BaseUrl,
}

/// A source distribution that exists at an arbitrary URL.
#[derive(Debug, Clone)]
pub struct DirectUrlSourceDist {
    /// Unlike [`DirectUrlBuiltDist`], we can't require a full filename with a version here, people
    /// like using e.g. `foo @ https://github.com/org/repo/archive/master.zip`
    pub name: PackageName,
    pub url: VerbatimUrl,
}

/// A source distribution that exists in a Git repository.
#[derive(Debug, Clone)]
pub struct GitSourceDist {
    pub name: PackageName,
    pub url: VerbatimUrl,
}

/// A source distribution that exists in a local directory.
#[derive(Debug, Clone)]
pub struct PathSourceDist {
    pub name: PackageName,
    pub url: VerbatimUrl,
    pub path: PathBuf,
    pub editable: bool,
}

impl Dist {
    /// Create a [`Dist`] for a registry-based distribution.
    pub fn from_registry(
        name: PackageName,
        version: Version,
        file: File,
        index: IndexUrl,
        base: BaseUrl,
    ) -> Self {
        if Path::new(&file.filename)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("whl"))
        {
            Self::Built(BuiltDist::Registry(RegistryBuiltDist {
                name,
                version,
                file,
                index,
                base,
            }))
        } else {
            Self::Source(SourceDist::Registry(RegistrySourceDist {
                name,
                version,
                file,
                index,
                base,
            }))
        }
    }

    /// Create a [`Dist`] for a URL-based distribution.
    pub fn from_url(name: PackageName, url: VerbatimUrl) -> Result<Self, Error> {
        if url.scheme().starts_with("git+") {
            return Ok(Self::Source(SourceDist::Git(GitSourceDist { name, url })));
        }

        if url.scheme().eq_ignore_ascii_case("file") {
            // Store the canonicalized path.
            let path = url
                .to_file_path()
                .map_err(|()| Error::UrlFilename(url.to_url()))?
                .canonicalize()
                .map_err(|err| Error::NotFound(url.to_url(), err))?;
            return if path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("whl"))
            {
                Ok(Self::Built(BuiltDist::Path(PathBuiltDist {
                    filename: WheelFilename::from_str(url.filename()?)?,
                    url,
                    path,
                })))
            } else {
                Ok(Self::Source(SourceDist::Path(PathSourceDist {
                    name,
                    url,
                    path,
                    editable: false,
                })))
            };
        }

        if Path::new(url.path())
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("whl"))
        {
            Ok(Self::Built(BuiltDist::DirectUrl(DirectUrlBuiltDist {
                filename: WheelFilename::from_str(url.filename()?)?,
                url,
            })))
        } else {
            Ok(Self::Source(SourceDist::DirectUrl(DirectUrlSourceDist {
                name,
                url,
            })))
        }
    }

    /// Create a [`Dist`] for a local editable distribution.
    pub fn from_editable(name: PackageName, editable: LocalEditable) -> Result<Self, Error> {
        match editable.requirement {
            EditableRequirement::Path { url, path } => {
                Ok(Self::Source(SourceDist::Path(PathSourceDist {
                    name,
                    url,
                    path,
                    editable: true,
                })))
            }
            EditableRequirement::Url { url, path } => {
                Ok(Self::Source(SourceDist::Path(PathSourceDist {
                    name,
                    path,
                    url,
                    editable: true,
                })))
            }
        }
    }

    /// Returns the [`File`] instance, if this dist is from a registry with simple json api support
    pub fn file(&self) -> Option<&File> {
        match self {
            Dist::Built(built) => built.file(),
            Dist::Source(source) => source.file(),
        }
    }
}

impl BuiltDist {
    /// Returns the [`File`] instance, if this dist is from a registry with simple json api support
    pub fn file(&self) -> Option<&File> {
        match self {
            BuiltDist::Registry(registry) => Some(&registry.file),
            BuiltDist::DirectUrl(_) | BuiltDist::Path(_) => None,
        }
    }
}

impl SourceDist {
    /// Returns the [`File`] instance, if this dist is from a registry with simple json api support
    pub fn file(&self) -> Option<&File> {
        match self {
            SourceDist::Registry(registry) => Some(&registry.file),
            SourceDist::DirectUrl(_) | SourceDist::Git(_) | SourceDist::Path(_) => None,
        }
    }

    #[must_use]
    pub fn with_url(self, url: Url) -> Self {
        match self {
            SourceDist::DirectUrl(dist) => SourceDist::DirectUrl(DirectUrlSourceDist {
                url: VerbatimUrl::unknown(url),
                ..dist
            }),
            SourceDist::Git(dist) => SourceDist::Git(GitSourceDist {
                url: VerbatimUrl::unknown(url),
                ..dist
            }),
            SourceDist::Path(dist) => SourceDist::Path(PathSourceDist {
                url: VerbatimUrl::unknown(url),
                ..dist
            }),
            dist @ SourceDist::Registry(_) => dist,
        }
    }
}

impl Name for RegistryBuiltDist {
    fn name(&self) -> &PackageName {
        &self.name
    }
}

impl Name for DirectUrlBuiltDist {
    fn name(&self) -> &PackageName {
        &self.filename.name
    }
}

impl Name for PathBuiltDist {
    fn name(&self) -> &PackageName {
        &self.filename.name
    }
}

impl Name for RegistrySourceDist {
    fn name(&self) -> &PackageName {
        &self.name
    }
}

impl Name for DirectUrlSourceDist {
    fn name(&self) -> &PackageName {
        &self.name
    }
}

impl Name for GitSourceDist {
    fn name(&self) -> &PackageName {
        &self.name
    }
}

impl Name for PathSourceDist {
    fn name(&self) -> &PackageName {
        &self.name
    }
}

impl Name for SourceDist {
    fn name(&self) -> &PackageName {
        match self {
            Self::Registry(dist) => dist.name(),
            Self::DirectUrl(dist) => dist.name(),
            Self::Git(dist) => dist.name(),
            Self::Path(dist) => dist.name(),
        }
    }
}

impl Name for BuiltDist {
    fn name(&self) -> &PackageName {
        match self {
            Self::Registry(dist) => dist.name(),
            Self::DirectUrl(dist) => dist.name(),
            Self::Path(dist) => dist.name(),
        }
    }
}

impl Name for Dist {
    fn name(&self) -> &PackageName {
        match self {
            Self::Built(dist) => dist.name(),
            Self::Source(dist) => dist.name(),
        }
    }
}

impl DistributionMetadata for RegistryBuiltDist {
    fn version_or_url(&self) -> VersionOrUrl {
        VersionOrUrl::Version(&self.version)
    }
}

impl DistributionMetadata for DirectUrlBuiltDist {
    fn version_or_url(&self) -> VersionOrUrl {
        VersionOrUrl::Url(&self.url)
    }
}

impl DistributionMetadata for PathBuiltDist {
    fn version_or_url(&self) -> VersionOrUrl {
        VersionOrUrl::Url(&self.url)
    }
}

impl DistributionMetadata for RegistrySourceDist {
    fn version_or_url(&self) -> VersionOrUrl {
        VersionOrUrl::Version(&self.version)
    }
}

impl DistributionMetadata for DirectUrlSourceDist {
    fn version_or_url(&self) -> VersionOrUrl {
        VersionOrUrl::Url(&self.url)
    }
}

impl DistributionMetadata for GitSourceDist {
    fn version_or_url(&self) -> VersionOrUrl {
        VersionOrUrl::Url(&self.url)
    }
}

impl DistributionMetadata for PathSourceDist {
    fn version_or_url(&self) -> VersionOrUrl {
        VersionOrUrl::Url(&self.url)
    }
}

impl DistributionMetadata for SourceDist {
    fn version_or_url(&self) -> VersionOrUrl {
        match self {
            Self::Registry(dist) => dist.version_or_url(),
            Self::DirectUrl(dist) => dist.version_or_url(),
            Self::Git(dist) => dist.version_or_url(),
            Self::Path(dist) => dist.version_or_url(),
        }
    }
}

impl DistributionMetadata for BuiltDist {
    fn version_or_url(&self) -> VersionOrUrl {
        match self {
            Self::Registry(dist) => dist.version_or_url(),
            Self::DirectUrl(dist) => dist.version_or_url(),
            Self::Path(dist) => dist.version_or_url(),
        }
    }
}

impl DistributionMetadata for Dist {
    fn version_or_url(&self) -> VersionOrUrl {
        match self {
            Self::Built(dist) => dist.version_or_url(),
            Self::Source(dist) => dist.version_or_url(),
        }
    }
}

impl RemoteSource for File {
    fn filename(&self) -> Result<&str, Error> {
        Ok(&self.filename)
    }

    fn size(&self) -> Option<u64> {
        self.size
    }
}

impl RemoteSource for Url {
    fn filename(&self) -> Result<&str, Error> {
        self.path_segments()
            .and_then(Iterator::last)
            .ok_or_else(|| Error::UrlFilename(self.clone()))
    }

    fn size(&self) -> Option<u64> {
        None
    }
}

impl RemoteSource for RegistryBuiltDist {
    fn filename(&self) -> Result<&str, Error> {
        self.file.filename()
    }

    fn size(&self) -> Option<u64> {
        self.file.size()
    }
}

impl RemoteSource for RegistrySourceDist {
    fn filename(&self) -> Result<&str, Error> {
        self.file.filename()
    }

    fn size(&self) -> Option<u64> {
        self.file.size()
    }
}

impl RemoteSource for DirectUrlBuiltDist {
    fn filename(&self) -> Result<&str, Error> {
        self.url.filename()
    }

    fn size(&self) -> Option<u64> {
        self.url.size()
    }
}

impl RemoteSource for DirectUrlSourceDist {
    fn filename(&self) -> Result<&str, Error> {
        self.url.filename()
    }

    fn size(&self) -> Option<u64> {
        self.url.size()
    }
}

impl RemoteSource for GitSourceDist {
    fn filename(&self) -> Result<&str, Error> {
        self.url.filename().map(|filename| {
            filename
                .rsplit_once('@')
                .map_or(filename, |(_, filename)| filename)
        })
    }

    fn size(&self) -> Option<u64> {
        self.url.size()
    }
}

impl RemoteSource for PathBuiltDist {
    fn filename(&self) -> Result<&str, Error> {
        self.url.filename()
    }

    fn size(&self) -> Option<u64> {
        self.url.size()
    }
}

impl RemoteSource for PathSourceDist {
    fn filename(&self) -> Result<&str, Error> {
        self.url.filename()
    }

    fn size(&self) -> Option<u64> {
        self.url.size()
    }
}

impl RemoteSource for SourceDist {
    fn filename(&self) -> Result<&str, Error> {
        match self {
            Self::Registry(dist) => dist.filename(),
            Self::DirectUrl(dist) => dist.filename(),
            Self::Git(dist) => dist.filename(),
            Self::Path(dist) => dist.filename(),
        }
    }

    fn size(&self) -> Option<u64> {
        match self {
            Self::Registry(dist) => dist.size(),
            Self::DirectUrl(dist) => dist.size(),
            Self::Git(dist) => dist.size(),
            Self::Path(dist) => dist.size(),
        }
    }
}

impl RemoteSource for BuiltDist {
    fn filename(&self) -> Result<&str, Error> {
        match self {
            Self::Registry(dist) => dist.filename(),
            Self::DirectUrl(dist) => dist.filename(),
            Self::Path(dist) => dist.filename(),
        }
    }

    fn size(&self) -> Option<u64> {
        match self {
            Self::Registry(dist) => dist.size(),
            Self::DirectUrl(dist) => dist.size(),
            Self::Path(dist) => dist.size(),
        }
    }
}

impl RemoteSource for Dist {
    fn filename(&self) -> Result<&str, Error> {
        match self {
            Self::Built(dist) => dist.filename(),
            Self::Source(dist) => dist.filename(),
        }
    }

    fn size(&self) -> Option<u64> {
        match self {
            Self::Built(dist) => dist.size(),
            Self::Source(dist) => dist.size(),
        }
    }
}

impl Identifier for Url {
    fn distribution_id(&self) -> DistributionId {
        DistributionId::new(cache_key::digest(&cache_key::CanonicalUrl::new(self)))
    }

    fn resource_id(&self) -> ResourceId {
        ResourceId::new(cache_key::digest(&cache_key::RepositoryUrl::new(self)))
    }
}

impl Identifier for File {
    fn distribution_id(&self) -> DistributionId {
        DistributionId::new(self.hashes.sha256.clone())
    }

    fn resource_id(&self) -> ResourceId {
        ResourceId::new(self.hashes.sha256.clone())
    }
}

impl Identifier for Path {
    fn distribution_id(&self) -> DistributionId {
        DistributionId::new(cache_key::digest(&self))
    }

    fn resource_id(&self) -> ResourceId {
        ResourceId::new(cache_key::digest(&self))
    }
}

impl Identifier for RegistryBuiltDist {
    fn distribution_id(&self) -> DistributionId {
        self.file.distribution_id()
    }

    fn resource_id(&self) -> ResourceId {
        self.file.resource_id()
    }
}

impl Identifier for RegistrySourceDist {
    fn distribution_id(&self) -> DistributionId {
        self.file.distribution_id()
    }

    fn resource_id(&self) -> ResourceId {
        self.file.resource_id()
    }
}

impl Identifier for DirectUrlBuiltDist {
    fn distribution_id(&self) -> DistributionId {
        self.url.distribution_id()
    }

    fn resource_id(&self) -> ResourceId {
        self.url.resource_id()
    }
}

impl Identifier for DirectUrlSourceDist {
    fn distribution_id(&self) -> DistributionId {
        self.url.distribution_id()
    }

    fn resource_id(&self) -> ResourceId {
        self.url.resource_id()
    }
}

impl Identifier for PathBuiltDist {
    fn distribution_id(&self) -> DistributionId {
        self.url.distribution_id()
    }

    fn resource_id(&self) -> ResourceId {
        self.url.resource_id()
    }
}

impl Identifier for PathSourceDist {
    fn distribution_id(&self) -> DistributionId {
        self.url.distribution_id()
    }

    fn resource_id(&self) -> ResourceId {
        self.url.resource_id()
    }
}

impl Identifier for GitSourceDist {
    fn distribution_id(&self) -> DistributionId {
        self.url.distribution_id()
    }

    fn resource_id(&self) -> ResourceId {
        self.url.resource_id()
    }
}

impl Identifier for SourceDist {
    fn distribution_id(&self) -> DistributionId {
        match self {
            Self::Registry(dist) => dist.distribution_id(),
            Self::DirectUrl(dist) => dist.distribution_id(),
            Self::Git(dist) => dist.distribution_id(),
            Self::Path(dist) => dist.distribution_id(),
        }
    }

    fn resource_id(&self) -> ResourceId {
        match self {
            Self::Registry(dist) => dist.resource_id(),
            Self::DirectUrl(dist) => dist.resource_id(),
            Self::Git(dist) => dist.resource_id(),
            Self::Path(dist) => dist.resource_id(),
        }
    }
}

impl Identifier for BuiltDist {
    fn distribution_id(&self) -> DistributionId {
        match self {
            Self::Registry(dist) => dist.distribution_id(),
            Self::DirectUrl(dist) => dist.distribution_id(),
            Self::Path(dist) => dist.distribution_id(),
        }
    }

    fn resource_id(&self) -> ResourceId {
        match self {
            Self::Registry(dist) => dist.resource_id(),
            Self::DirectUrl(dist) => dist.resource_id(),
            Self::Path(dist) => dist.resource_id(),
        }
    }
}

impl Identifier for Dist {
    fn distribution_id(&self) -> DistributionId {
        match self {
            Self::Built(dist) => dist.distribution_id(),
            Self::Source(dist) => dist.distribution_id(),
        }
    }

    fn resource_id(&self) -> ResourceId {
        match self {
            Self::Built(dist) => dist.resource_id(),
            Self::Source(dist) => dist.resource_id(),
        }
    }
}
