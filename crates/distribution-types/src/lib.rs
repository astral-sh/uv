//! ## Type hierarchy
//!
//! When we receive the requirements from `pip sync`, we check which requirements already fulfilled
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
//! ## `InstalledDist`
//! An [`InstalledDist`] is built distribution (wheel) that is installed in a virtual environment,
//! with the two possible origins we currently track:
//! * [`InstalledRegistryDist`]
//! * [`InstalledDirectUrlDist`]
//!
//! Since we read this information from [`direct_url.json`](https://packaging.python.org/en/latest/specifications/direct-url-data-structure/), it doesn't match the information [`Dist`] exactly.
use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::Result;
use url::Url;

use distribution_filename::{DistFilename, SourceDistFilename, WheelFilename};
use pep440_rs::Version;
use pep508_rs::{Scheme, VerbatimUrl};
use uv_normalize::PackageName;

pub use crate::any::*;
pub use crate::buildable::*;
pub use crate::cached::*;
pub use crate::editable::*;
pub use crate::error::*;
pub use crate::file::*;
pub use crate::hash::*;
pub use crate::id::*;
pub use crate::index_url::*;
pub use crate::installed::*;
pub use crate::parsed_url::*;
pub use crate::prioritized_distribution::*;
pub use crate::resolution::*;
pub use crate::resolved::*;
pub use crate::traits::*;
pub use crate::uv_requirement::*;

mod any;
mod buildable;
mod cached;
mod editable;
mod error;
mod file;
mod hash;
mod id;
mod index_url;
mod installed;
mod parsed_url;
mod prioritized_distribution;
mod resolution;
mod resolved;
mod traits;
mod uv_requirement;

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

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
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

/// A source distribution, with its possible origins (index, url, path, git)
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
    pub filename: WheelFilename,
    pub file: Box<File>,
    pub index: IndexUrl,
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
    pub filename: SourceDistFilename,
    pub file: Box<File>,
    pub index: IndexUrl,
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
    /// The url without `git+` prefix.
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
    pub fn from_registry(filename: DistFilename, file: File, index: IndexUrl) -> Self {
        match filename {
            DistFilename::WheelFilename(filename) => {
                Self::Built(BuiltDist::Registry(RegistryBuiltDist {
                    filename,
                    file: Box::new(file),
                    index,
                }))
            }
            DistFilename::SourceDistFilename(filename) => {
                Self::Source(SourceDist::Registry(RegistrySourceDist {
                    filename,
                    file: Box::new(file),
                    index,
                }))
            }
        }
    }

    pub fn from_https_url(name: PackageName, url: VerbatimUrl) -> Result<Self, Error> {
        if Path::new(url.path())
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("whl"))
        {
            // Validate that the name in the wheel matches that of the requirement.
            let filename = WheelFilename::from_str(&url.filename()?)?;
            if filename.name != name {
                return Err(Error::PackageNameMismatch(
                    name,
                    filename.name,
                    url.verbatim().to_string(),
                ));
            }

            Ok(Self::Built(BuiltDist::DirectUrl(DirectUrlBuiltDist {
                filename,
                url,
            })))
        } else {
            Ok(Self::Source(SourceDist::DirectUrl(DirectUrlSourceDist {
                name,
                url,
            })))
        }
    }

    pub fn from_file_url(
        name: PackageName,
        url: VerbatimUrl,
        editable: bool,
    ) -> Result<Dist, Error> {
        // Store the canonicalized path, which also serves to validate that it exists.
        let path = match url
            .to_file_path()
            .map_err(|()| Error::UrlFilename(url.to_url()))?
            .canonicalize()
        {
            Ok(path) => path,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Err(Error::NotFound(url.to_url()));
            }
            Err(err) => return Err(err.into()),
        };

        if path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("whl"))
        {
            // Validate that the name in the wheel matches that of the requirement.
            let filename = WheelFilename::from_str(&url.filename()?)?;
            if filename.name != name {
                return Err(Error::PackageNameMismatch(
                    name,
                    filename.name,
                    url.verbatim().to_string(),
                ));
            }

            if editable {
                return Err(Error::EditableFile(url));
            }

            Ok(Self::Built(BuiltDist::Path(PathBuiltDist {
                filename,
                url,
                path,
            })))
        } else {
            Ok(Self::Source(SourceDist::Path(PathSourceDist {
                name,
                url,
                path,
                editable,
            })))
        }
    }

    pub fn from_git_url(name: PackageName, url: VerbatimUrl) -> Result<Dist, Error> {
        Ok(Self::Source(SourceDist::Git(GitSourceDist { name, url })))
    }

    // TODO(konsti): We should carry the parsed url through the codebase.
    /// Create a [`Dist`] for a URL-based distribution.
    pub fn from_url(name: PackageName, url: VerbatimUrl) -> Result<Self, Error> {
        match Scheme::parse(url.scheme()) {
            Some(Scheme::Http | Scheme::Https) => Self::from_https_url(name, url),
            Some(Scheme::File) => Self::from_file_url(name, url, false),
            Some(Scheme::GitSsh | Scheme::GitHttps) => Self::from_git_url(name, url),
            Some(Scheme::GitGit | Scheme::GitHttp) => Err(Error::UnsupportedScheme(
                url.scheme().to_owned(),
                url.verbatim().to_string(),
                "insecure Git protocol".to_string(),
            )),
            Some(Scheme::GitFile) => Err(Error::UnsupportedScheme(
                url.scheme().to_owned(),
                url.verbatim().to_string(),
                "local Git protocol".to_string(),
            )),
            Some(
                Scheme::BzrHttp
                | Scheme::BzrHttps
                | Scheme::BzrSsh
                | Scheme::BzrSftp
                | Scheme::BzrFtp
                | Scheme::BzrLp
                | Scheme::BzrFile,
            ) => Err(Error::UnsupportedScheme(
                url.scheme().to_owned(),
                url.verbatim().to_string(),
                "Bazaar is not supported".to_string(),
            )),
            Some(
                Scheme::HgFile
                | Scheme::HgHttp
                | Scheme::HgHttps
                | Scheme::HgSsh
                | Scheme::HgStaticHttp,
            ) => Err(Error::UnsupportedScheme(
                url.scheme().to_owned(),
                url.verbatim().to_string(),
                "Mercurial is not supported".to_string(),
            )),
            Some(
                Scheme::SvnSsh
                | Scheme::SvnHttp
                | Scheme::SvnHttps
                | Scheme::SvnSvn
                | Scheme::SvnFile,
            ) => Err(Error::UnsupportedScheme(
                url.scheme().to_owned(),
                url.verbatim().to_string(),
                "Subversion is not supported".to_string(),
            )),
            None => Err(Error::UnsupportedScheme(
                url.scheme().to_owned(),
                url.verbatim().to_string(),
                "unknown scheme".to_string(),
            )),
        }
    }

    /// Create a [`Dist`] for a local editable distribution.
    pub fn from_editable(name: PackageName, editable: LocalEditable) -> Result<Self, Error> {
        let LocalEditable { url, path, .. } = editable;
        Ok(Self::Source(SourceDist::Path(PathSourceDist {
            name,
            url,
            path,
            editable: true,
        })))
    }

    /// Return true if the distribution is editable.
    pub fn is_editable(&self) -> bool {
        match self {
            Self::Source(dist) => dist.is_editable(),
            Self::Built(_) => false,
        }
    }

    /// Returns the [`IndexUrl`], if the distribution is from a registry.
    pub fn index(&self) -> Option<&IndexUrl> {
        match self {
            Self::Built(dist) => dist.index(),
            Self::Source(dist) => dist.index(),
        }
    }

    /// Returns the [`File`] instance, if this dist is from a registry with simple json api support
    pub fn file(&self) -> Option<&File> {
        match self {
            Self::Built(built) => built.file(),
            Self::Source(source) => source.file(),
        }
    }

    pub fn version(&self) -> Option<&Version> {
        match self {
            Self::Built(wheel) => Some(wheel.version()),
            Self::Source(source_dist) => source_dist.version(),
        }
    }
}

impl BuiltDist {
    /// Returns the [`IndexUrl`], if the distribution is from a registry.
    pub fn index(&self) -> Option<&IndexUrl> {
        match self {
            Self::Registry(registry) => Some(&registry.index),
            Self::DirectUrl(_) => None,
            Self::Path(_) => None,
        }
    }

    /// Returns the [`File`] instance, if this distribution is from a registry.
    pub fn file(&self) -> Option<&File> {
        match self {
            Self::Registry(registry) => Some(&registry.file),
            Self::DirectUrl(_) | Self::Path(_) => None,
        }
    }

    pub fn version(&self) -> &Version {
        match self {
            Self::Registry(wheel) => &wheel.filename.version,
            Self::DirectUrl(wheel) => &wheel.filename.version,
            Self::Path(wheel) => &wheel.filename.version,
        }
    }
}

impl SourceDist {
    /// Returns the [`IndexUrl`], if the distribution is from a registry.
    pub fn index(&self) -> Option<&IndexUrl> {
        match self {
            Self::Registry(registry) => Some(&registry.index),
            Self::DirectUrl(_) | Self::Git(_) | Self::Path(_) => None,
        }
    }

    /// Returns the [`File`] instance, if this dist is from a registry with simple json api support
    pub fn file(&self) -> Option<&File> {
        match self {
            Self::Registry(registry) => Some(&registry.file),
            Self::DirectUrl(_) | Self::Git(_) | Self::Path(_) => None,
        }
    }

    pub fn version(&self) -> Option<&Version> {
        match self {
            Self::Registry(source_dist) => Some(&source_dist.filename.version),
            Self::DirectUrl(_) | Self::Git(_) | Self::Path(_) => None,
        }
    }

    #[must_use]
    pub fn with_url(self, url: Url) -> Self {
        match self {
            Self::Git(dist) => Self::Git(GitSourceDist {
                url: VerbatimUrl::unknown(url),
                ..dist
            }),
            dist => dist,
        }
    }

    /// Return true if the distribution is editable.
    pub fn is_editable(&self) -> bool {
        match self {
            Self::Path(PathSourceDist { editable, .. }) => *editable,
            _ => false,
        }
    }

    /// Returns the path to the source distribution, if it's a local distribution.
    pub fn as_path(&self) -> Option<&Path> {
        match self {
            Self::Path(dist) => Some(&dist.path),
            _ => None,
        }
    }
}

impl Name for RegistryBuiltDist {
    fn name(&self) -> &PackageName {
        &self.filename.name
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
        &self.filename.name
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
        VersionOrUrl::Version(&self.filename.version)
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
        VersionOrUrl::Version(&self.filename.version)
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
    fn filename(&self) -> Result<Cow<'_, str>, Error> {
        Ok(Cow::Borrowed(&self.filename))
    }

    fn size(&self) -> Option<u64> {
        self.size
    }
}

impl RemoteSource for Url {
    fn filename(&self) -> Result<Cow<'_, str>, Error> {
        // Identify the last segment of the URL as the filename.
        let filename = self
            .path_segments()
            .and_then(Iterator::last)
            .ok_or_else(|| Error::UrlFilename(self.clone()))?;

        // Decode the filename, which may be percent-encoded.
        let filename = urlencoding::decode(filename)?;

        Ok(filename)
    }

    fn size(&self) -> Option<u64> {
        None
    }
}

impl RemoteSource for RegistryBuiltDist {
    fn filename(&self) -> Result<Cow<'_, str>, Error> {
        self.file.filename()
    }

    fn size(&self) -> Option<u64> {
        self.file.size()
    }
}

impl RemoteSource for RegistrySourceDist {
    fn filename(&self) -> Result<Cow<'_, str>, Error> {
        self.file.filename()
    }

    fn size(&self) -> Option<u64> {
        self.file.size()
    }
}

impl RemoteSource for DirectUrlBuiltDist {
    fn filename(&self) -> Result<Cow<'_, str>, Error> {
        self.url.filename()
    }

    fn size(&self) -> Option<u64> {
        self.url.size()
    }
}

impl RemoteSource for DirectUrlSourceDist {
    fn filename(&self) -> Result<Cow<'_, str>, Error> {
        self.url.filename()
    }

    fn size(&self) -> Option<u64> {
        self.url.size()
    }
}

impl RemoteSource for GitSourceDist {
    fn filename(&self) -> Result<Cow<'_, str>, Error> {
        // The filename is the last segment of the URL, before any `@`.
        match self.url.filename()? {
            Cow::Borrowed(filename) => {
                if let Some((_, filename)) = filename.rsplit_once('@') {
                    Ok(Cow::Borrowed(filename))
                } else {
                    Ok(Cow::Borrowed(filename))
                }
            }
            Cow::Owned(filename) => {
                if let Some((_, filename)) = filename.rsplit_once('@') {
                    Ok(Cow::Owned(filename.to_owned()))
                } else {
                    Ok(Cow::Owned(filename))
                }
            }
        }
    }

    fn size(&self) -> Option<u64> {
        self.url.size()
    }
}

impl RemoteSource for PathBuiltDist {
    fn filename(&self) -> Result<Cow<'_, str>, Error> {
        self.url.filename()
    }

    fn size(&self) -> Option<u64> {
        self.url.size()
    }
}

impl RemoteSource for PathSourceDist {
    fn filename(&self) -> Result<Cow<'_, str>, Error> {
        self.url.filename()
    }

    fn size(&self) -> Option<u64> {
        self.url.size()
    }
}

impl RemoteSource for SourceDist {
    fn filename(&self) -> Result<Cow<'_, str>, Error> {
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
    fn filename(&self) -> Result<Cow<'_, str>, Error> {
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
    fn filename(&self) -> Result<Cow<'_, str>, Error> {
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
        DistributionId::Url(cache_key::CanonicalUrl::new(self))
    }

    fn resource_id(&self) -> ResourceId {
        ResourceId::Url(cache_key::RepositoryUrl::new(self))
    }
}

impl Identifier for File {
    fn distribution_id(&self) -> DistributionId {
        if let Some(hash) = self.hashes.first() {
            DistributionId::Digest(hash.clone())
        } else {
            self.url.distribution_id()
        }
    }

    fn resource_id(&self) -> ResourceId {
        if let Some(hash) = self.hashes.first() {
            ResourceId::Digest(hash.clone())
        } else {
            self.url.resource_id()
        }
    }
}

impl Identifier for Path {
    fn distribution_id(&self) -> DistributionId {
        DistributionId::PathBuf(self.to_path_buf())
    }

    fn resource_id(&self) -> ResourceId {
        ResourceId::PathBuf(self.to_path_buf())
    }
}

impl Identifier for FileLocation {
    fn distribution_id(&self) -> DistributionId {
        match self {
            Self::RelativeUrl(base, url) => {
                DistributionId::RelativeUrl(base.to_string(), url.to_string())
            }
            Self::AbsoluteUrl(url) => DistributionId::AbsoluteUrl(url.to_string()),
            Self::Path(path) => path.distribution_id(),
        }
    }

    fn resource_id(&self) -> ResourceId {
        match self {
            Self::RelativeUrl(base, url) => {
                ResourceId::RelativeUrl(base.to_string(), url.to_string())
            }
            Self::AbsoluteUrl(url) => ResourceId::AbsoluteUrl(url.to_string()),
            Self::Path(path) => path.resource_id(),
        }
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

impl Identifier for InstalledDist {
    fn distribution_id(&self) -> DistributionId {
        self.path().distribution_id()
    }

    fn resource_id(&self) -> ResourceId {
        self.path().resource_id()
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

impl Identifier for DirectSourceUrl<'_> {
    fn distribution_id(&self) -> DistributionId {
        self.url.distribution_id()
    }

    fn resource_id(&self) -> ResourceId {
        self.url.resource_id()
    }
}

impl Identifier for GitSourceUrl<'_> {
    fn distribution_id(&self) -> DistributionId {
        self.url.distribution_id()
    }

    fn resource_id(&self) -> ResourceId {
        self.url.resource_id()
    }
}

impl Identifier for PathSourceUrl<'_> {
    fn distribution_id(&self) -> DistributionId {
        self.url.distribution_id()
    }

    fn resource_id(&self) -> ResourceId {
        self.url.resource_id()
    }
}

impl Identifier for SourceUrl<'_> {
    fn distribution_id(&self) -> DistributionId {
        match self {
            Self::Direct(url) => url.distribution_id(),
            Self::Git(url) => url.distribution_id(),
            Self::Path(url) => url.distribution_id(),
        }
    }

    fn resource_id(&self) -> ResourceId {
        match self {
            Self::Direct(url) => url.resource_id(),
            Self::Git(url) => url.resource_id(),
            Self::Path(url) => url.resource_id(),
        }
    }
}

impl Identifier for BuildableSource<'_> {
    fn distribution_id(&self) -> DistributionId {
        match self {
            BuildableSource::Dist(source) => source.distribution_id(),
            BuildableSource::Url(source) => source.distribution_id(),
        }
    }

    fn resource_id(&self) -> ResourceId {
        match self {
            BuildableSource::Dist(source) => source.resource_id(),
            BuildableSource::Url(source) => source.resource_id(),
        }
    }
}

#[cfg(test)]
mod test {
    use crate::{BuiltDist, Dist, SourceDist};

    /// Ensure that we don't accidentally grow the `Dist` sizes.
    #[test]
    fn dist_size() {
        // At time of writing, Unix is at 240, Windows is at 248.
        assert!(
            std::mem::size_of::<Dist>() <= 248,
            "{}",
            std::mem::size_of::<Dist>()
        );
        assert!(
            std::mem::size_of::<BuiltDist>() <= 248,
            "{}",
            std::mem::size_of::<BuiltDist>()
        );
        // At time of writing, unix is at 168, windows is at 176.
        assert!(
            std::mem::size_of::<SourceDist>() <= 176,
            "{}",
            std::mem::size_of::<SourceDist>()
        );
    }
}
