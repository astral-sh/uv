use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{anyhow, Context, Result};
use fs_err as fs;
use tracing::warn;
use url::Url;

use distribution_filename::EggInfoFilename;
use pep440_rs::Version;
use pypi_types::DirectUrl;
use uv_fs::Simplified;
use uv_normalize::PackageName;

use crate::{DistributionMetadata, InstalledMetadata, InstalledVersion, Name, VersionOrUrlRef};

/// A built distribution (wheel) that is installed in a virtual environment.
#[derive(Debug, Clone, Hash)]
pub enum InstalledDist {
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

#[derive(Debug, Clone, Hash)]
pub struct InstalledRegistryDist {
    pub name: PackageName,
    pub version: Version,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Hash)]
pub struct InstalledDirectUrlDist {
    pub name: PackageName,
    pub version: Version,
    pub direct_url: Box<DirectUrl>,
    pub url: Url,
    pub editable: bool,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Hash)]
pub struct InstalledEggInfoFile {
    pub name: PackageName,
    pub version: Version,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Hash)]
pub struct InstalledEggInfoDirectory {
    pub name: PackageName,
    pub version: Version,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Hash)]
pub struct InstalledLegacyEditable {
    pub name: PackageName,
    pub version: Version,
    pub egg_link: PathBuf,
    pub target: PathBuf,
    pub target_url: Url,
    pub egg_info: PathBuf,
}

impl InstalledDist {
    /// Try to parse a distribution from a `.dist-info` directory name (like `django-5.0a1.dist-info`).
    ///
    /// See: <https://packaging.python.org/en/latest/specifications/recording-installed-packages/#recording-installed-packages>
    pub fn try_from_path(path: &Path) -> Result<Option<Self>> {
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
            let version = Version::from_str(version).map_err(|err| anyhow!(err))?;
            return if let Some(direct_url) = Self::direct_url(path)? {
                match Url::try_from(&direct_url) {
                    Ok(url) => Ok(Some(Self::Url(InstalledDirectUrlDist {
                        name,
                        version,
                        editable: matches!(&direct_url, DirectUrl::LocalDirectory { dir_info, .. } if dir_info.editable == Some(true)),
                        direct_url: Box::new(direct_url),
                        url,
                        path: path.to_path_buf(),
                    }))),
                    Err(err) => {
                        warn!("Failed to parse direct URL: {err}");
                        Ok(Some(Self::Registry(InstalledRegistryDist {
                            name,
                            version,
                            path: path.to_path_buf(),
                        })))
                    }
                }
            } else {
                Ok(Some(Self::Registry(InstalledRegistryDist {
                    name,
                    version,
                    path: path.to_path_buf(),
                })))
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

            if metadata.is_dir() {
                return Ok(Some(Self::EggInfoDirectory(InstalledEggInfoDirectory {
                    name: file_name.name,
                    version: file_name.version,
                    path: path.to_path_buf(),
                })));
            }

            if metadata.is_file() {
                return Ok(Some(Self::EggInfoFile(InstalledEggInfoFile {
                    name: file_name.name,
                    version: file_name.version,
                    path: path.to_path_buf(),
                })));
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
                .ok_or_else(|| anyhow!("Invalid `.egg-link` path: {}", path.user_display()))?
                .join(target);

            // Normalisation comes from `pkg_resources.to_filename`.
            let egg_info = target.join(file_stem.replace('-', "_") + ".egg-info");
            let url = Url::from_file_path(&target)
                .map_err(|()| anyhow!("Invalid `.egg-link` target: {}", target.user_display()))?;

            // Mildly unfortunate that we must read metadata to get the version.
            let content = match fs::read(egg_info.join("PKG-INFO")) {
                Ok(content) => content,
                Err(err) => {
                    warn!("Failed to read metadata for {path:?}: {err}");
                    return Ok(None);
                }
            };
            let metadata = match pypi_types::Metadata10::parse_pkg_info(&content) {
                Ok(metadata) => metadata,
                Err(err) => {
                    warn!("Failed to parse metadata for {path:?}: {err}");
                    return Ok(None);
                }
            };

            return Ok(Some(Self::LegacyEditable(InstalledLegacyEditable {
                name: metadata.name,
                version: Version::from_str(&metadata.version)?,
                egg_link: path.to_path_buf(),
                target,
                target_url: url,
                egg_info,
            })));
        }

        Ok(None)
    }

    /// Return the [`Path`] at which the distribution is stored on-disk.
    pub fn path(&self) -> &Path {
        match self {
            Self::Registry(dist) => &dist.path,
            Self::Url(dist) => &dist.path,
            Self::EggInfoDirectory(dist) => &dist.path,
            Self::EggInfoFile(dist) => &dist.path,
            Self::LegacyEditable(dist) => &dist.egg_info,
        }
    }

    /// Return the [`Version`] of the distribution.
    pub fn version(&self) -> &Version {
        match self {
            Self::Registry(dist) => &dist.version,
            Self::Url(dist) => &dist.version,
            Self::EggInfoDirectory(dist) => &dist.version,
            Self::EggInfoFile(dist) => &dist.version,
            Self::LegacyEditable(dist) => &dist.version,
        }
    }

    /// Read the `direct_url.json` file from a `.dist-info` directory.
    pub fn direct_url(path: &Path) -> Result<Option<DirectUrl>> {
        let path = path.join("direct_url.json");
        let Ok(file) = fs_err::File::open(path) else {
            return Ok(None);
        };
        let direct_url = serde_json::from_reader::<fs_err::File, DirectUrl>(file)?;
        Ok(Some(direct_url))
    }

    /// Read the `METADATA` file from a `.dist-info` directory.
    pub fn metadata(&self) -> Result<pypi_types::Metadata23> {
        match self {
            Self::Registry(_) | Self::Url(_) => {
                let path = self.path().join("METADATA");
                let contents = fs::read(&path)?;
                // TODO(zanieb): Update this to use thiserror so we can unpack parse errors downstream
                pypi_types::Metadata23::parse_metadata(&contents).with_context(|| {
                    format!(
                        "Failed to parse `METADATA` file at: {}",
                        path.user_display()
                    )
                })
            }
            Self::EggInfoFile(_) | Self::EggInfoDirectory(_) | Self::LegacyEditable(_) => {
                let path = match self {
                    Self::EggInfoFile(dist) => Cow::Borrowed(&dist.path),
                    Self::EggInfoDirectory(dist) => Cow::Owned(dist.path.join("PKG-INFO")),
                    Self::LegacyEditable(dist) => Cow::Owned(dist.egg_info.join("PKG-INFO")),
                    _ => unreachable!(),
                };
                let contents = fs::read(path.as_ref())?;
                pypi_types::Metadata23::parse_metadata(&contents).with_context(|| {
                    format!(
                        "Failed to parse `PKG-INFO` file at: {}",
                        path.user_display()
                    )
                })
            }
        }
    }

    /// Return the `INSTALLER` of the distribution.
    pub fn installer(&self) -> Result<Option<String>> {
        let path = self.path().join("INSTALLER");
        match fs::read_to_string(path) {
            Ok(installer) => Ok(Some(installer)),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err.into()),
        }
    }

    /// Return true if the distribution is editable.
    pub fn is_editable(&self) -> bool {
        matches!(
            self,
            Self::LegacyEditable(_) | Self::Url(InstalledDirectUrlDist { editable: true, .. })
        )
    }

    /// Return the [`Url`] of the distribution, if it is editable.
    pub fn as_editable(&self) -> Option<&Url> {
        match self {
            Self::Registry(_) => None,
            Self::Url(dist) => dist.editable.then_some(&dist.url),
            Self::EggInfoFile(_) => None,
            Self::EggInfoDirectory(_) => None,
            Self::LegacyEditable(dist) => Some(&dist.target_url),
        }
    }

    /// Return true if the distribution refers to a local file or directory.
    pub fn is_local(&self) -> bool {
        match self {
            Self::Registry(_) => false,
            Self::Url(dist) => matches!(&*dist.direct_url, DirectUrl::LocalDirectory { .. }),
            Self::EggInfoFile(_) => false,
            Self::EggInfoDirectory(_) => false,
            Self::LegacyEditable(_) => true,
        }
    }
}

impl DistributionMetadata for InstalledDist {
    fn version_or_url(&self) -> VersionOrUrlRef {
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
        match self {
            Self::Registry(dist) => dist.name(),
            Self::Url(dist) => dist.name(),
            Self::EggInfoDirectory(dist) => dist.name(),
            Self::EggInfoFile(dist) => dist.name(),
            Self::LegacyEditable(dist) => dist.name(),
        }
    }
}

impl InstalledMetadata for InstalledRegistryDist {
    fn installed_version(&self) -> InstalledVersion {
        InstalledVersion::Version(&self.version)
    }
}

impl InstalledMetadata for InstalledDirectUrlDist {
    fn installed_version(&self) -> InstalledVersion {
        InstalledVersion::Url(&self.url, &self.version)
    }
}

impl InstalledMetadata for InstalledEggInfoFile {
    fn installed_version(&self) -> InstalledVersion {
        InstalledVersion::Version(&self.version)
    }
}

impl InstalledMetadata for InstalledEggInfoDirectory {
    fn installed_version(&self) -> InstalledVersion {
        InstalledVersion::Version(&self.version)
    }
}

impl InstalledMetadata for InstalledLegacyEditable {
    fn installed_version(&self) -> InstalledVersion {
        InstalledVersion::Version(&self.version)
    }
}

impl InstalledMetadata for InstalledDist {
    fn installed_version(&self) -> InstalledVersion {
        match self {
            Self::Registry(dist) => dist.installed_version(),
            Self::Url(dist) => dist.installed_version(),
            Self::EggInfoFile(dist) => dist.installed_version(),
            Self::EggInfoDirectory(dist) => dist.installed_version(),
            Self::LegacyEditable(dist) => dist.installed_version(),
        }
    }
}
