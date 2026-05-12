use std::borrow::Cow;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::OnceLock;

use fs_err as fs;
use thiserror::Error;
use tracing::warn;
use url::Url;

use uv_cache_info::CacheInfo;
use uv_distribution_filename::{EggInfoFilename, ExpandedTags};
use uv_fs::Simplified;
use uv_install_wheel::WheelFile;
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_pypi_types::{DirectUrl, MetadataError};
use uv_redacted::DisplaySafeUrl;

use crate::{
    BuildInfo, DistributionMetadata, InstalledMetadata, InstalledVersion, Name, VersionOrUrlRef,
};

#[derive(Error, Debug)]
pub enum InstalledDistError {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    UrlParse(#[from] url::ParseError),

    #[error(transparent)]
    Json(#[from] serde_json::Error),

    #[error(transparent)]
    EggInfoParse(#[from] uv_distribution_filename::EggInfoFilenameError),

    #[error(transparent)]
    VersionParse(#[from] uv_pep440::VersionParseError),

    #[error(transparent)]
    PackageNameParse(#[from] uv_normalize::InvalidNameError),

    #[error(transparent)]
    WheelFileParse(#[from] uv_install_wheel::Error),

    #[error(transparent)]
    ExpandedTagParse(#[from] uv_distribution_filename::ExpandedTagError),

    #[error("Invalid .egg-link path: `{}`", _0.user_display())]
    InvalidEggLinkPath(PathBuf),

    #[error("Invalid .egg-link target: `{}`", _0.user_display())]
    InvalidEggLinkTarget(PathBuf),

    #[error("Failed to parse METADATA file: `{}`", path.user_display())]
    MetadataParse {
        path: PathBuf,
        #[source]
        err: Box<MetadataError>,
    },

    #[error("Failed to parse `PKG-INFO` file: `{}`", path.user_display())]
    PkgInfoParse {
        path: PathBuf,
        #[source]
        err: Box<MetadataError>,
    },
}

#[derive(Debug, Clone)]
pub struct InstalledDist {
    pub kind: InstalledDistKind,
    // Cache data that must be read from the `.dist-info` directory. These are safe to cache as
    // the `InstalledDist` is immutable after creation.
    metadata_cache: OnceLock<uv_pypi_types::ResolutionMetadata>,
    tags_cache: OnceLock<Option<ExpandedTags>>,
}

impl From<InstalledDistKind> for InstalledDist {
    fn from(kind: InstalledDistKind) -> Self {
        Self {
            kind,
            metadata_cache: OnceLock::new(),
            tags_cache: OnceLock::new(),
        }
    }
}

impl std::hash::Hash for InstalledDist {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.kind.hash(state);
    }
}

impl PartialEq for InstalledDist {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind
    }
}

impl Eq for InstalledDist {}

/// A built distribution (wheel) that is installed in a virtual environment.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum InstalledDistKind {
    /// The distribution was derived from a registry, like `PyPI`.
    Registry(InstalledRegistryDist),
    /// The distribution was derived from an arbitrary URL.
    Url(InstalledDirectUrlDist),
    /// The distribution was derived from pre-existing `.egg-info` file (as installed by distutils).
    EggInfoFile(InstalledEggInfoFile),
    /// The distribution was derived from pre-existing `.egg-info` directory.
    EggInfoDirectory(InstalledEggInfoDirectory),
    /// The distribution was derived from an `.egg-link` pointer.
    LegacyEditable(InstalledLegacyEditable),
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct InstalledRegistryDist {
    pub name: PackageName,
    pub version: Version,
    pub path: Box<Path>,
    pub cache_info: Option<CacheInfo>,
    pub build_info: Option<BuildInfo>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct InstalledDirectUrlDist {
    pub name: PackageName,
    pub version: Version,
    pub direct_url: Box<DirectUrl>,
    pub url: DisplaySafeUrl,
    pub editable: bool,
    pub path: Box<Path>,
    pub cache_info: Option<CacheInfo>,
    pub build_info: Option<BuildInfo>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct InstalledEggInfoFile {
    pub name: PackageName,
    pub version: Version,
    pub path: Box<Path>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct InstalledEggInfoDirectory {
    pub name: PackageName,
    pub version: Version,
    pub path: Box<Path>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct InstalledLegacyEditable {
    pub name: PackageName,
    pub version: Version,
    pub egg_link: Box<Path>,
    pub target: Box<Path>,
    pub target_url: DisplaySafeUrl,
    pub egg_info: Box<Path>,
}

impl InstalledDist {
    /// Try to parse a distribution from a `.dist-info` directory name (like `django-5.0a1.dist-info`).
    ///
    /// See: <https://packaging.python.org/en/latest/specifications/recording-installed-packages/#recording-installed-packages>
    pub fn try_from_path(path: &Path) -> Result<Option<Self>, InstalledDistError> {
        // Ex) `cffi-1.16.0.dist-info`
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
            let version = Version::from_str(version)?;
            let cache_info = Self::read_cache_info(path)?;
            let build_info = Self::read_build_info(path)?;

            return if let Some(direct_url) = Self::read_direct_url(path)? {
                match DisplaySafeUrl::try_from(&direct_url) {
                    Ok(url) => Ok(Some(Self::from(InstalledDistKind::Url(
                        InstalledDirectUrlDist {
                            name,
                            version,
                            editable: matches!(&direct_url, DirectUrl::LocalDirectory { dir_info, .. } if dir_info.editable == Some(true)),
                            direct_url: Box::new(direct_url),
                            url,
                            path: path.to_path_buf().into_boxed_path(),
                            cache_info,
                            build_info,
                        },
                    )))),
                    Err(err) => {
                        warn!("Failed to parse direct URL: {err}");
                        Ok(Some(Self::from(InstalledDistKind::Registry(
                            InstalledRegistryDist {
                                name,
                                version,
                                path: path.to_path_buf().into_boxed_path(),
                                cache_info,
                                build_info,
                            },
                        ))))
                    }
                }
            } else {
                Ok(Some(Self::from(InstalledDistKind::Registry(
                    InstalledRegistryDist {
                        name,
                        version,
                        path: path.to_path_buf().into_boxed_path(),
                        cache_info,
                        build_info,
                    },
                ))))
            };
        }

        // Ex) `zstandard-0.22.0-py3.12.egg-info` or `vtk-9.2.6.egg-info`
        if path.extension().is_some_and(|ext| ext == "egg-info") {
            let metadata = match fs_err::metadata(path) {
                Ok(metadata) => metadata,
                Err(err) => {
                    warn!("Invalid `.egg-info` path: {err}");
                    return Ok(None);
                }
            };

            let Some(file_stem) = path.file_stem() else {
                return Ok(None);
            };
            let Some(file_stem) = file_stem.to_str() else {
                return Ok(None);
            };
            let file_name = EggInfoFilename::parse(file_stem)?;

            if let Some(version) = file_name.version {
                if metadata.is_dir() {
                    return Ok(Some(Self::from(InstalledDistKind::EggInfoDirectory(
                        InstalledEggInfoDirectory {
                            name: file_name.name,
                            version,
                            path: path.to_path_buf().into_boxed_path(),
                        },
                    ))));
                }

                if metadata.is_file() {
                    return Ok(Some(Self::from(InstalledDistKind::EggInfoFile(
                        InstalledEggInfoFile {
                            name: file_name.name,
                            version,
                            path: path.to_path_buf().into_boxed_path(),
                        },
                    ))));
                }
            }

            if metadata.is_dir() {
                let Some(egg_metadata) = read_metadata(&path.join("PKG-INFO")) else {
                    return Ok(None);
                };
                return Ok(Some(Self::from(InstalledDistKind::EggInfoDirectory(
                    InstalledEggInfoDirectory {
                        name: file_name.name,
                        version: Version::from_str(&egg_metadata.version)?,
                        path: path.to_path_buf().into_boxed_path(),
                    },
                ))));
            }

            if metadata.is_file() {
                let Some(egg_metadata) = read_metadata(path) else {
                    return Ok(None);
                };
                return Ok(Some(Self::from(InstalledDistKind::EggInfoDirectory(
                    InstalledEggInfoDirectory {
                        name: file_name.name,
                        version: Version::from_str(&egg_metadata.version)?,
                        path: path.to_path_buf().into_boxed_path(),
                    },
                ))));
            }
        }

        // Ex) `zstandard.egg-link`
        if path.extension().is_some_and(|ext| ext == "egg-link") {
            let Some(file_stem) = path.file_stem() else {
                return Ok(None);
            };
            let Some(file_stem) = file_stem.to_str() else {
                return Ok(None);
            };

            // https://setuptools.pypa.io/en/latest/deprecated/python_eggs.html#egg-links
            // https://github.com/pypa/pip/blob/946f95d17431f645da8e2e0bf4054a72db5be766/src/pip/_internal/metadata/importlib/_envs.py#L86-L108
            let contents = fs::read_to_string(path)?;
            let Some(target) = contents.lines().find_map(|line| {
                let line = line.trim();
                if line.is_empty() {
                    None
                } else {
                    Some(PathBuf::from(line))
                }
            }) else {
                warn!("Invalid `.egg-link` file: {path:?}");
                return Ok(None);
            };

            // Match pip, but note setuptools only puts absolute paths in `.egg-link` files.
            let target = path
                .parent()
                .ok_or_else(|| InstalledDistError::InvalidEggLinkPath(path.to_path_buf()))?
                .join(target);

            // Normalisation comes from `pkg_resources.to_filename`.
            let egg_info = target.join(file_stem.replace('-', "_") + ".egg-info");
            let url = DisplaySafeUrl::from_file_path(&target)
                .map_err(|()| InstalledDistError::InvalidEggLinkTarget(path.to_path_buf()))?;

            // Mildly unfortunate that we must read metadata to get the version.
            let Some(egg_metadata) = read_metadata(&egg_info.join("PKG-INFO")) else {
                return Ok(None);
            };

            return Ok(Some(Self::from(InstalledDistKind::LegacyEditable(
                InstalledLegacyEditable {
                    name: egg_metadata.name,
                    version: Version::from_str(&egg_metadata.version)?,
                    egg_link: path.to_path_buf().into_boxed_path(),
                    target: target.into_boxed_path(),
                    target_url: url,
                    egg_info: egg_info.into_boxed_path(),
                },
            ))));
        }

        Ok(None)
    }

    /// Return the [`Path`] at which the distribution is stored on-disk.
    pub fn install_path(&self) -> &Path {
        match &self.kind {
            InstalledDistKind::Registry(dist) => &dist.path,
            InstalledDistKind::Url(dist) => &dist.path,
            InstalledDistKind::EggInfoDirectory(dist) => &dist.path,
            InstalledDistKind::EggInfoFile(dist) => &dist.path,
            InstalledDistKind::LegacyEditable(dist) => &dist.egg_info,
        }
    }

    /// Return the [`Version`] of the distribution.
    pub fn version(&self) -> &Version {
        match &self.kind {
            InstalledDistKind::Registry(dist) => &dist.version,
            InstalledDistKind::Url(dist) => &dist.version,
            InstalledDistKind::EggInfoDirectory(dist) => &dist.version,
            InstalledDistKind::EggInfoFile(dist) => &dist.version,
            InstalledDistKind::LegacyEditable(dist) => &dist.version,
        }
    }

    /// Return the [`CacheInfo`] of the distribution, if any.
    pub fn cache_info(&self) -> Option<&CacheInfo> {
        match &self.kind {
            InstalledDistKind::Registry(dist) => dist.cache_info.as_ref(),
            InstalledDistKind::Url(dist) => dist.cache_info.as_ref(),
            InstalledDistKind::EggInfoDirectory(..) => None,
            InstalledDistKind::EggInfoFile(..) => None,
            InstalledDistKind::LegacyEditable(..) => None,
        }
    }

    /// Return the [`BuildInfo`] of the distribution, if any.
    pub fn build_info(&self) -> Option<&BuildInfo> {
        match &self.kind {
            InstalledDistKind::Registry(dist) => dist.build_info.as_ref(),
            InstalledDistKind::Url(dist) => dist.build_info.as_ref(),
            InstalledDistKind::EggInfoDirectory(..) => None,
            InstalledDistKind::EggInfoFile(..) => None,
            InstalledDistKind::LegacyEditable(..) => None,
        }
    }

    /// Read the `direct_url.json` file from a `.dist-info` directory.
    pub fn read_direct_url(path: &Path) -> Result<Option<DirectUrl>, InstalledDistError> {
        let path = path.join("direct_url.json");
        let file = match fs_err::File::open(&path) {
            Ok(file) => file,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(err.into()),
        };
        let direct_url =
            serde_json::from_reader::<BufReader<fs_err::File>, DirectUrl>(BufReader::new(file))?;
        Ok(Some(direct_url))
    }

    /// Read the `uv_cache.json` file from a `.dist-info` directory.
    pub fn read_cache_info(path: &Path) -> Result<Option<CacheInfo>, InstalledDistError> {
        let path = path.join("uv_cache.json");
        let file = match fs_err::File::open(&path) {
            Ok(file) => file,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(err.into()),
        };
        let cache_info =
            serde_json::from_reader::<BufReader<fs_err::File>, CacheInfo>(BufReader::new(file))?;
        Ok(Some(cache_info))
    }

    /// Read the `uv_build.json` file from a `.dist-info` directory.
    pub fn read_build_info(path: &Path) -> Result<Option<BuildInfo>, InstalledDistError> {
        let path = path.join("uv_build.json");
        let file = match fs_err::File::open(&path) {
            Ok(file) => file,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(err.into()),
        };
        let build_info =
            serde_json::from_reader::<BufReader<fs_err::File>, BuildInfo>(BufReader::new(file))?;
        Ok(Some(build_info))
    }

    /// Read the `METADATA` file from a `.dist-info` directory.
    pub fn read_metadata(&self) -> Result<&uv_pypi_types::ResolutionMetadata, InstalledDistError> {
        if let Some(metadata) = self.metadata_cache.get() {
            return Ok(metadata);
        }

        let metadata = match &self.kind {
            InstalledDistKind::Registry(_) | InstalledDistKind::Url(_) => {
                let path = self.install_path().join("METADATA");
                let contents = fs::read(&path)?;
                // TODO(zanieb): Update this to use thiserror so we can unpack parse errors downstream
                uv_pypi_types::ResolutionMetadata::parse_metadata(&contents).map_err(|err| {
                    InstalledDistError::MetadataParse {
                        path: path.clone(),
                        err: Box::new(err),
                    }
                })?
            }
            InstalledDistKind::EggInfoFile(_)
            | InstalledDistKind::EggInfoDirectory(_)
            | InstalledDistKind::LegacyEditable(_) => {
                let path = match &self.kind {
                    InstalledDistKind::EggInfoFile(dist) => Cow::Borrowed(&*dist.path),
                    InstalledDistKind::EggInfoDirectory(dist) => {
                        Cow::Owned(dist.path.join("PKG-INFO"))
                    }
                    InstalledDistKind::LegacyEditable(dist) => {
                        Cow::Owned(dist.egg_info.join("PKG-INFO"))
                    }
                    _ => unreachable!(),
                };
                let contents = fs::read(path.as_ref())?;
                uv_pypi_types::ResolutionMetadata::parse_metadata(&contents).map_err(|err| {
                    InstalledDistError::PkgInfoParse {
                        path: path.to_path_buf(),
                        err: Box::new(err),
                    }
                })?
            }
        };

        let _ = self.metadata_cache.set(metadata);
        Ok(self.metadata_cache.get().expect("metadata should be set"))
    }

    /// Return the `INSTALLER` of the distribution.
    pub fn read_installer(&self) -> Result<Option<String>, InstalledDistError> {
        let path = self.install_path().join("INSTALLER");
        match fs::read_to_string(path) {
            Ok(installer) => Ok(Some(installer.trim().to_owned())),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err.into()),
        }
    }

    /// Return the supported wheel tags for the distribution from the `WHEEL` file, if available.
    pub fn read_tags(&self) -> Result<Option<&ExpandedTags>, InstalledDistError> {
        if let Some(tags) = self.tags_cache.get() {
            return Ok(tags.as_ref());
        }

        let path = match &self.kind {
            InstalledDistKind::Registry(dist) => &dist.path,
            InstalledDistKind::Url(dist) => &dist.path,
            InstalledDistKind::EggInfoFile(_) => return Ok(None),
            InstalledDistKind::EggInfoDirectory(_) => return Ok(None),
            InstalledDistKind::LegacyEditable(_) => return Ok(None),
        };

        // Read the `WHEEL` file.
        let contents = fs_err::read_to_string(path.join("WHEEL"))?;
        let wheel_file = WheelFile::parse(&contents)?;

        // Parse the tags.
        let tags = if let Some(tags) = wheel_file.tags() {
            Some(ExpandedTags::parse(tags.iter().map(String::as_str))?)
        } else {
            None
        };

        let _ = self.tags_cache.set(tags);
        Ok(self.tags_cache.get().expect("tags should be set").as_ref())
    }

    /// Return true if the distribution is editable.
    pub fn is_editable(&self) -> bool {
        matches!(
            &self.kind,
            InstalledDistKind::LegacyEditable(_)
                | InstalledDistKind::Url(InstalledDirectUrlDist { editable: true, .. })
        )
    }

    /// Return the [`Url`] of the distribution, if it is editable.
    pub fn as_editable(&self) -> Option<&Url> {
        match &self.kind {
            InstalledDistKind::Registry(_) => None,
            InstalledDistKind::Url(dist) => dist.editable.then_some(&dist.url),
            InstalledDistKind::EggInfoFile(_) => None,
            InstalledDistKind::EggInfoDirectory(_) => None,
            InstalledDistKind::LegacyEditable(dist) => Some(&dist.target_url),
        }
    }

    /// Return true if the distribution refers to a local file or directory.
    pub fn is_local(&self) -> bool {
        match &self.kind {
            InstalledDistKind::Registry(_) => false,
            InstalledDistKind::Url(dist) => {
                matches!(&*dist.direct_url, DirectUrl::LocalDirectory { .. })
            }
            InstalledDistKind::EggInfoFile(_) => false,
            InstalledDistKind::EggInfoDirectory(_) => false,
            InstalledDistKind::LegacyEditable(_) => true,
        }
    }
}

impl DistributionMetadata for InstalledDist {
    fn version_or_url(&self) -> VersionOrUrlRef<'_> {
        VersionOrUrlRef::Version(self.version())
    }
}

impl Name for InstalledRegistryDist {
    fn name(&self) -> &PackageName {
        &self.name
    }
}

impl Name for InstalledDirectUrlDist {
    fn name(&self) -> &PackageName {
        &self.name
    }
}

impl Name for InstalledEggInfoFile {
    fn name(&self) -> &PackageName {
        &self.name
    }
}

impl Name for InstalledEggInfoDirectory {
    fn name(&self) -> &PackageName {
        &self.name
    }
}

impl Name for InstalledLegacyEditable {
    fn name(&self) -> &PackageName {
        &self.name
    }
}

impl Name for InstalledDist {
    fn name(&self) -> &PackageName {
        match &self.kind {
            InstalledDistKind::Registry(dist) => dist.name(),
            InstalledDistKind::Url(dist) => dist.name(),
            InstalledDistKind::EggInfoDirectory(dist) => dist.name(),
            InstalledDistKind::EggInfoFile(dist) => dist.name(),
            InstalledDistKind::LegacyEditable(dist) => dist.name(),
        }
    }
}

impl InstalledMetadata for InstalledRegistryDist {
    fn installed_version(&self) -> InstalledVersion<'_> {
        InstalledVersion::Version(&self.version)
    }
}

impl InstalledMetadata for InstalledDirectUrlDist {
    fn installed_version(&self) -> InstalledVersion<'_> {
        InstalledVersion::Url(&self.url, &self.version)
    }
}

impl InstalledMetadata for InstalledEggInfoFile {
    fn installed_version(&self) -> InstalledVersion<'_> {
        InstalledVersion::Version(&self.version)
    }
}

impl InstalledMetadata for InstalledEggInfoDirectory {
    fn installed_version(&self) -> InstalledVersion<'_> {
        InstalledVersion::Version(&self.version)
    }
}

impl InstalledMetadata for InstalledLegacyEditable {
    fn installed_version(&self) -> InstalledVersion<'_> {
        InstalledVersion::Version(&self.version)
    }
}

impl InstalledMetadata for InstalledDist {
    fn installed_version(&self) -> InstalledVersion<'_> {
        match &self.kind {
            InstalledDistKind::Registry(dist) => dist.installed_version(),
            InstalledDistKind::Url(dist) => dist.installed_version(),
            InstalledDistKind::EggInfoFile(dist) => dist.installed_version(),
            InstalledDistKind::EggInfoDirectory(dist) => dist.installed_version(),
            InstalledDistKind::LegacyEditable(dist) => dist.installed_version(),
        }
    }
}

fn read_metadata(path: &Path) -> Option<uv_pypi_types::Metadata10> {
    let content = match fs::read(path) {
        Ok(content) => content,
        Err(err) => {
            warn!("Failed to read metadata for {path:?}: {err}");
            return None;
        }
    };
    let metadata = match uv_pypi_types::Metadata10::parse_pkg_info(&content) {
        Ok(metadata) => metadata,
        Err(err) => {
            warn!("Failed to parse metadata for {path:?}: {err}");
            return None;
        }
    };

    Some(metadata)
}
