use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{anyhow, Result};
use url::Url;

use pep440_rs::Version;
use puffin_cache::CanonicalUrl;
use puffin_normalize::PackageName;
use pypi_types::File;

pub mod source;

/// A built distribution (wheel), which either exists remotely or locally.
#[derive(Debug, Clone)]
pub enum Distribution {
    Remote(RemoteDistribution),
    Cached(CachedDistribution),
    Installed(InstalledDistribution),
}

impl Distribution {
    /// Return the normalized [`PackageName`] of the distribution.
    pub fn name(&self) -> &PackageName {
        match self {
            Self::Remote(dist) => dist.name(),
            Self::Cached(dist) => dist.name(),
            Self::Installed(dist) => dist.name(),
        }
    }

    /// Return a [`Version`], for registry-based distributions, or a [`Url`], for URL-based
    /// distributions.
    pub fn version_or_url(&self) -> VersionOrUrl {
        match self {
            Self::Remote(dist) => dist.version_or_url(),
            Self::Cached(dist) => dist.version_or_url(),
            Self::Installed(dist) => dist.version_or_url(),
        }
    }
}

impl From<RemoteDistribution> for Distribution {
    fn from(dist: RemoteDistribution) -> Self {
        Self::Remote(dist)
    }
}

impl From<CachedDistribution> for Distribution {
    fn from(dist: CachedDistribution) -> Self {
        Self::Cached(dist)
    }
}

impl From<InstalledDistribution> for Distribution {
    fn from(dist: InstalledDistribution) -> Self {
        Self::Installed(dist)
    }
}

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

/// A built distribution (wheel) that exists as a remote file (e.g., on `PyPI`).
#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum RemoteDistribution {
    /// The distribution exists in a registry, like `PyPI`.
    Registry(PackageName, Version, File),
    /// The distribution exists at an arbitrary URL.
    Url(PackageName, Url),
}

impl RemoteDistribution {
    /// Create a [`RemoteDistribution`] for a registry-based distribution.
    pub fn from_registry(name: PackageName, version: Version, file: File) -> Self {
        Self::Registry(name, version, file)
    }

    /// Create a [`RemoteDistribution`] for a URL-based distribution.
    pub fn from_url(name: PackageName, url: Url) -> Self {
        Self::Url(name, url)
    }

    /// Return the URL of the distribution.
    pub fn url(&self) -> Result<Cow<'_, Url>> {
        match self {
            Self::Registry(_, _, file) => {
                let url = Url::parse(&file.url)?;
                Ok(Cow::Owned(url))
            }
            Self::Url(_, url) => Ok(Cow::Borrowed(url)),
        }
    }

    /// Return the filename of the distribution.
    pub fn filename(&self) -> Result<Cow<'_, str>> {
        match self {
            Self::Registry(_, _, file) => Ok(Cow::Borrowed(&file.filename)),
            Self::Url(_, url) => {
                let filename = url
                    .path_segments()
                    .and_then(std::iter::Iterator::last)
                    .ok_or_else(|| anyhow!("Could not parse filename from URL: {}", url))?;
                Ok(Cow::Owned(filename.to_owned()))
            }
        }
    }

    /// Return the normalized [`PackageName`] of the distribution.
    pub fn name(&self) -> &PackageName {
        match self {
            Self::Registry(name, _, _) => name,
            Self::Url(name, _) => name,
        }
    }

    /// Return a [`Version`], for registry-based distributions, or a [`Url`], for URL-based
    /// distributions.
    pub fn version_or_url(&self) -> VersionOrUrl {
        match self {
            Self::Registry(_, version, _) => VersionOrUrl::Version(version),
            Self::Url(_, url) => VersionOrUrl::Url(url),
        }
    }

    /// Returns a unique identifier for this distribution.
    pub fn id(&self) -> String {
        match self {
            Self::Registry(name, version, _) => {
                // https://packaging.python.org/en/latest/specifications/recording-installed-packages/#the-dist-info-directory
                // `version` is normalized by its `ToString` impl
                format!(
                    "{}-{}",
                    PackageName::from(name).as_dist_info_name(),
                    version
                )
            }
            Self::Url(_name, url) => puffin_cache::digest(&CanonicalUrl::new(url)),
        }
    }

    /// Returns `true` if this distribution is a wheel.
    pub fn is_wheel(&self) -> bool {
        let filename = match self {
            Self::Registry(_name, _version, file) => &file.filename,
            Self::Url(_name, url) => url.path(),
        };
        Path::new(filename)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("whl"))
    }
}

impl std::fmt::Display for RemoteDistribution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Registry(name, version, _file) => {
                write!(f, "{name}=={version}")
            }
            Self::Url(name, url) => {
                write!(f, "{name} @ {url}")
            }
        }
    }
}

/// A built distribution (wheel) that exists in a local cache.
#[derive(Debug, Clone)]
pub enum CachedDistribution {
    /// The distribution exists in a registry, like `PyPI`.
    Registry(PackageName, Version, PathBuf),
    /// The distribution exists at an arbitrary URL.
    Url(PackageName, Url, PathBuf),
}

impl CachedDistribution {
    /// Initialize a [`CachedDistribution`] from a [`RemoteDistribution`].
    pub fn from_remote(remote: RemoteDistribution, path: PathBuf) -> Self {
        match remote {
            RemoteDistribution::Registry(name, version, _file) => {
                Self::Registry(name, version, path)
            }
            RemoteDistribution::Url(name, url) => Self::Url(name, url, path),
        }
    }

    /// Try to parse a distribution from a cached directory name (like `django-5.0a1`).
    pub fn try_from_path(path: &Path) -> Result<Option<Self>> {
        let Some(file_name) = path.file_name() else {
            return Ok(None);
        };
        let Some(file_name) = file_name.to_str() else {
            return Ok(None);
        };
        let Some((name, version)) = file_name.split_once('-') else {
            return Ok(None);
        };

        let name = PackageName::from_str(name)?;
        let version = Version::from_str(version).map_err(|err| anyhow!(err))?;
        let path = path.to_path_buf();

        Ok(Some(Self::Registry(name, version, path)))
    }

    /// Return the normalized [`PackageName`] of the distribution.
    pub fn name(&self) -> &PackageName {
        match self {
            Self::Registry(name, _, _) => name,
            Self::Url(name, _, _) => name,
        }
    }

    /// Return the [`Path`] at which the distribution is stored on-disk.
    pub fn path(&self) -> &Path {
        match self {
            Self::Registry(_, _, path) => path,
            Self::Url(_, _, path) => path,
        }
    }

    /// Return a [`Version`], for registry-based distributions, or a [`Url`], for URL-based
    /// distributions.
    pub fn version_or_url(&self) -> VersionOrUrl {
        match self {
            Self::Registry(_, version, _) => VersionOrUrl::Version(version),
            Self::Url(_, url, _) => VersionOrUrl::Url(url),
        }
    }
}

impl std::fmt::Display for CachedDistribution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Registry(name, version, _file) => {
                write!(f, "{name}=={version}")
            }
            Self::Url(name, url, _path) => {
                write!(f, "{name} @ {url}")
            }
        }
    }
}

/// A built distribution (wheel) that exists in a virtual environment.
#[derive(Debug, Clone)]
pub struct InstalledDistribution {
    name: PackageName,
    version: Version,
    path: PathBuf,
}

impl InstalledDistribution {
    /// Initialize a new installed distribution.
    pub fn new(name: PackageName, version: Version, path: PathBuf) -> Self {
        Self {
            name,
            version,
            path,
        }
    }

    /// Try to parse a distribution from a `.dist-info` directory name (like `django-5.0a1.dist-info`).
    ///
    /// See: <https://packaging.python.org/en/latest/specifications/recording-installed-packages/#recording-installed-packages>
    pub fn try_from_path(path: &Path) -> Result<Option<Self>> {
        if path.extension().is_some_and(|ext| ext == "dist-info") {
            let Some(file_stem) = path.file_stem() else {
                return Ok(None);
            };
            let Some(file_stem) = file_stem.to_str() else {
                return Ok(None);
            };
            let Some((name, version)) = file_stem.split_once('-') else {
                return Ok(None);
            };

            let name = PackageName::from_str(name)?;
            let version = Version::from_str(version).map_err(|err| anyhow!(err))?;
            let path = path.to_path_buf();

            return Ok(Some(Self {
                name,
                version,
                path,
            }));
        }

        Ok(None)
    }

    /// Return the normalized [`PackageName`] of the distribution.
    pub fn name(&self) -> &PackageName {
        &self.name
    }

    pub fn version(&self) -> &Version {
        &self.version
    }

    /// Return the [`Path`] at which the distribution is stored on-disk.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Return a [`Version`], for registry-based distributions, or a [`Url`], for URL-based
    /// distributions.
    pub fn version_or_url(&self) -> VersionOrUrl {
        VersionOrUrl::Version(&self.version)
    }
}

impl std::fmt::Display for InstalledDistribution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}=={}", self.name(), self.version())
    }
}

/// Unowned reference to a [`RemoteDistribution`].
#[derive(Debug, Clone)]
pub enum RemoteDistributionRef<'a> {
    /// The distribution exists in a registry, like `PyPI`.
    Registry(&'a PackageName, &'a Version, &'a File),
    /// The distribution exists at an arbitrary URL.
    Url(&'a PackageName, &'a Url),
}

impl<'a> RemoteDistributionRef<'a> {
    /// Create a [`RemoteDistribution`] for a registry-based distribution.
    pub fn from_registry(name: &'a PackageName, version: &'a Version, file: &'a File) -> Self {
        Self::Registry(name, version, file)
    }

    /// Create a [`RemoteDistribution`] for a URL-based distribution.
    pub fn from_url(name: &'a PackageName, url: &'a Url) -> Self {
        Self::Url(name, url)
    }

    /// Return the URL of the distribution.
    pub fn url(&self) -> Result<Cow<'_, Url>> {
        match self {
            Self::Registry(_, _, file) => {
                let url = Url::parse(&file.url)?;
                Ok(Cow::Owned(url))
            }
            Self::Url(_, url) => Ok(Cow::Borrowed(url)),
        }
    }

    /// Return the filename of the distribution.
    pub fn filename(&self) -> Result<Cow<'_, str>> {
        match self {
            Self::Registry(_, _, file) => Ok(Cow::Borrowed(&file.filename)),
            Self::Url(_, url) => {
                let filename = url
                    .path_segments()
                    .and_then(std::iter::Iterator::last)
                    .ok_or_else(|| anyhow!("Could not parse filename from URL: {}", url))?;
                Ok(Cow::Owned(filename.to_owned()))
            }
        }
    }

    /// Return the normalized [`PackageName`] of the distribution.
    pub fn name(&self) -> &PackageName {
        match self {
            Self::Registry(name, _, _) => name,
            Self::Url(name, _) => name,
        }
    }

    /// Return a [`Version`], for registry-based distributions, or a [`Url`], for URL-based
    /// distributions.
    pub fn version_or_url(&self) -> VersionOrUrl {
        match self {
            Self::Registry(_, version, _) => VersionOrUrl::Version(version),
            Self::Url(_, url) => VersionOrUrl::Url(url),
        }
    }

    /// Returns a unique identifier for this distribution.
    pub fn id(&self) -> String {
        match self {
            Self::Registry(name, version, _) => {
                // https://packaging.python.org/en/latest/specifications/recording-installed-packages/#the-dist-info-directory
                // `version` is normalized by its `ToString` impl
                format!("{}-{}", PackageName::from(*name), version)
            }
            Self::Url(_name, url) => puffin_cache::digest(&CanonicalUrl::new(url)),
        }
    }
}

impl std::fmt::Display for RemoteDistributionRef<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Registry(name, version, _file) => {
                write!(f, "{name}=={version}")
            }
            Self::Url(name, url) => {
                write!(f, "{name} @ {url}")
            }
        }
    }
}

impl<'a> From<&'a RemoteDistribution> for RemoteDistributionRef<'a> {
    fn from(dist: &'a RemoteDistribution) -> Self {
        match dist {
            RemoteDistribution::Registry(name, version, file) => {
                Self::Registry(name, version, file)
            }
            RemoteDistribution::Url(name, url) => Self::Url(name, url),
        }
    }
}
