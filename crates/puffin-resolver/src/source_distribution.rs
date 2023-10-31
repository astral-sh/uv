use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::Result;
use fs_err::tokio as fs;
use tempfile::tempdir;
use tokio::task::spawn_blocking;
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tracing::debug;
use url::Url;
use zip::ZipArchive;

use distribution_filename::WheelFilename;
use pep440_rs::Version;
use platform_tags::Tags;
use puffin_client::RegistryClient;
use puffin_package::package_name::PackageName;
use puffin_package::pypi_types::{File, Metadata21};
use puffin_traits::BuildContext;

const BUILT_WHEELS_CACHE: &str = "built-wheels-v0";

/// TODO(konstin): Find a better home for me?
///
/// Stores wheels built from source distributions. We need to keep those separate from the regular
/// wheel cache since a wheel with the same name may be uploaded after we made our build and in that
/// case the hashes would clash.
pub struct BuiltSourceDistributionCache(PathBuf);

impl BuiltSourceDistributionCache {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self(path.as_ref().join(BUILT_WHEELS_CACHE))
    }

    pub fn version(&self, name: &PackageName, version: &Version) -> PathBuf {
        self.0.join(name.to_string()).join(version.to_string())
    }

    /// Search for a wheel matching the tags that was built from the given source distribution.  
    pub fn find_wheel(
        &self,
        package_name: &PackageName,
        version: &Version,
        tags: &Tags,
    ) -> Option<PathBuf> {
        let Ok(read_dir) = fs_err::read_dir(self.version(package_name, version)) else {
            return None;
        };

        for entry in read_dir {
            let Ok(entry) = entry else {
                continue;
            };
            let Ok(wheel) = WheelFilename::from_str(entry.file_name().to_string_lossy().as_ref())
            else {
                continue;
            };

            if wheel.is_compatible(tags) {
                return Some(entry.path().clone());
            }
        }
        None
    }
}

pub(crate) async fn download_and_build_sdist(
    file: &File,
    package_name: &PackageName,
    version: &Version,
    client: &RegistryClient,
    build_context: &impl BuildContext,
) -> Result<Metadata21> {
    debug!("Building: {}", &file.filename);
    let url = Url::parse(&file.url)?;
    let reader = client.stream_external(&url).await?;
    let mut reader = tokio::io::BufReader::new(reader.compat());
    let temp_dir = tempdir()?;

    let sdist_dir = temp_dir.path().join("sdist");
    tokio::fs::create_dir(&sdist_dir).await?;
    let sdist_file = sdist_dir.join(&file.filename);
    let mut writer = tokio::fs::File::create(&sdist_file).await?;
    tokio::io::copy(&mut reader, &mut writer).await?;

    let wheel_dir = if let Some(cache) = &build_context.cache() {
        BuiltSourceDistributionCache::new(cache).version(package_name, version)
    } else {
        temp_dir.path().join("wheels")
    };

    fs::create_dir_all(&wheel_dir).await?;

    let disk_filename = build_context
        .build_source_distribution(&sdist_file, &wheel_dir)
        .await?;

    let metadata21 = read_dist_info(wheel_dir.join(disk_filename)).await?;

    debug!("Finished building: {}", &file.filename);
    Ok(metadata21)
}

pub(crate) async fn read_dist_info(wheel: PathBuf) -> Result<Metadata21> {
    let dist_info = spawn_blocking(move || -> Result<String> {
        let mut archive = ZipArchive::new(std::fs::File::open(&wheel)?)?;
        let dist_info_prefix = install_wheel_rs::find_dist_info(
            &WheelFilename::from_str(wheel.file_name().unwrap().to_string_lossy().as_ref())?,
            &mut archive,
        )?;
        let dist_info = std::io::read_to_string(
            archive.by_name(&format!("{dist_info_prefix}.dist-info/METADATA"))?,
        )?;
        Ok(dist_info)
    })
    .await
    .unwrap()?;
    Ok(Metadata21::parse(dist_info.as_bytes())?)
}
