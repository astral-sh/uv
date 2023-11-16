use std::cmp::Reverse;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{bail, Result};
use bytesize::ByteSize;
use tokio::task::JoinSet;
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tracing::debug;
use url::Url;

use puffin_client::RegistryClient;
use puffin_distribution::direct_url::{DirectArchiveUrl, DirectGitUrl};
use puffin_distribution::{BuiltDist, Dist, RemoteSource, SourceDist};
use puffin_git::GitSource;

use crate::locks::Locks;

const GIT_CACHE: &str = "git-v0";

pub struct Downloader<'a> {
    client: &'a RegistryClient,
    cache: &'a Path,
    locks: Arc<Locks>,
    reporter: Option<Box<dyn Reporter>>,
    /// Block building source distributions by not downloading them
    no_build: bool,
}

impl<'a> Downloader<'a> {
    /// Initialize a new distribution downloader.
    pub fn new(client: &'a RegistryClient, cache: &'a Path) -> Self {
        Self {
            client,
            cache,
            locks: Arc::new(Locks::default()),
            reporter: None,
            no_build: false,
        }
    }

    /// Set the [`Reporter`] to use for this downloader.
    #[must_use]
    pub fn with_reporter(self, reporter: impl Reporter + 'static) -> Self {
        Self {
            reporter: Some(Box::new(reporter)),
            ..self
        }
    }

    /// Optionally, block downloading source distributions
    #[must_use]
    pub fn with_no_build(self, no_build: bool) -> Self {
        Self { no_build, ..self }
    }

    /// Download a set of distributions.
    pub async fn download(&self, dists: Vec<Dist>) -> Result<Vec<Download>> {
        // Sort the distributions by size.
        let mut dists = dists;
        dists.sort_unstable_by_key(|distribution| {
            Reverse(distribution.size().unwrap_or(usize::MAX))
        });

        // Fetch the distributions in parallel.
        let mut fetches = JoinSet::new();
        let mut downloads = Vec::with_capacity(dists.len());
        for dist in dists {
            if self.no_build && matches!(dist, Dist::Source(_)) {
                bail!(
                    "Building source distributions is disabled, not downloading {}",
                    dist
                );
            }

            fetches.spawn(fetch(
                dist.clone(),
                self.client.clone(),
                self.cache.to_path_buf(),
                self.locks.clone(),
            ));
        }

        while let Some(result) = fetches.join_next().await.transpose()? {
            let result = result?;

            if let Some(reporter) = self.reporter.as_ref() {
                reporter.on_download_progress(&result);
            }

            downloads.push(result);
        }

        if let Some(reporter) = self.reporter.as_ref() {
            reporter.on_download_complete();
        }

        Ok(downloads)
    }
}

/// Download a built distribution (wheel) or source distribution (sdist).
async fn fetch(
    dist: Dist,
    client: RegistryClient,
    cache: PathBuf,
    locks: Arc<Locks>,
) -> Result<Download> {
    let lock = locks.acquire(&dist).await;
    let _guard = lock.lock().await;

    match &dist {
        Dist::Built(BuiltDist::Registry(wheel)) => {
            // Fetch the wheel.
            let url = Url::parse(&wheel.file.url)?;
            let reader = client.stream_external(&url).await?;

            // If the file is greater than 5MB, write it to disk; otherwise, keep it in memory.
            let small_size = if let Some(size) = wheel.file.size {
                let byte_size = ByteSize::b(size as u64);
                if byte_size < ByteSize::mb(5) {
                    Some(size)
                } else {
                    None
                }
            } else {
                None
            };
            if let Some(small_size) = small_size {
                debug!(
                    "Fetching in-memory wheel from registry: {dist} ({})",
                    ByteSize::b(small_size as u64)
                );

                // Read into a buffer.
                let mut buffer = Vec::with_capacity(small_size);
                let mut reader = tokio::io::BufReader::new(reader.compat());
                tokio::io::copy(&mut reader, &mut buffer).await?;

                Ok(Download::Wheel(WheelDownload::InMemory(InMemoryWheel {
                    dist,
                    buffer,
                })))
            } else {
                let size = small_size.map_or("unknown size".to_string(), |size| size.to_string());
                debug!("Fetching disk-based wheel from registry: {dist} ({size})");

                // Download the wheel to a temporary file.
                let temp_dir = tempfile::tempdir_in(cache)?.into_path();
                let wheel_filename = &wheel.file.filename;
                let wheel_file = temp_dir.join(wheel_filename);
                let mut writer = tokio::fs::File::create(&wheel_file).await?;
                tokio::io::copy(&mut reader.compat(), &mut writer).await?;

                Ok(Download::Wheel(WheelDownload::Disk(DiskWheel {
                    dist,
                    path: wheel_file,
                })))
            }
        }

        Dist::Built(BuiltDist::DirectUrl(wheel)) => {
            debug!("Fetching disk-based wheel from URL: {}", &wheel.url);

            // Fetch the wheel.
            let reader = client.stream_external(&wheel.url).await?;

            // Download the wheel to a temporary file.
            let temp_dir = tempfile::tempdir_in(cache)?.into_path();
            let wheel_filename = wheel.filename()?;
            let wheel_file = temp_dir.join(wheel_filename);
            let mut writer = tokio::fs::File::create(&wheel_file).await?;
            tokio::io::copy(&mut reader.compat(), &mut writer).await?;

            Ok(Download::Wheel(WheelDownload::Disk(DiskWheel {
                dist,
                path: wheel_file,
            })))
        }

        Dist::Source(SourceDist::Registry(sdist)) => {
            debug!(
                "Fetching source distribution from registry: {}",
                &sdist.file.url
            );

            let url = Url::parse(&sdist.file.url)?;
            let reader = client.stream_external(&url).await?;

            // Download the source distribution.
            let temp_dir = tempfile::tempdir_in(cache)?.into_path();
            let sdist_filename = sdist.filename()?;
            let sdist_file = temp_dir.join(sdist_filename);
            let mut writer = tokio::fs::File::create(&sdist_file).await?;
            tokio::io::copy(&mut reader.compat(), &mut writer).await?;

            Ok(Download::SourceDist(SourceDistDownload {
                dist,
                sdist_file,
                subdirectory: None,
            }))
        }

        Dist::Source(SourceDist::DirectUrl(sdist)) => {
            debug!("Fetching source distribution from URL: {}", sdist.url);

            let DirectArchiveUrl { url, subdirectory } = DirectArchiveUrl::from(&sdist.url);

            let reader = client.stream_external(&url).await?;
            let mut reader = tokio::io::BufReader::new(reader.compat());

            // Download the source distribution.
            let temp_dir = tempfile::tempdir_in(cache)?.into_path();
            let sdist_filename = sdist.filename()?;
            let sdist_file = temp_dir.join(sdist_filename);
            let mut writer = tokio::fs::File::create(&sdist_file).await?;
            tokio::io::copy(&mut reader, &mut writer).await?;

            Ok(Download::SourceDist(SourceDistDownload {
                dist,
                sdist_file,
                subdirectory,
            }))
        }

        Dist::Source(SourceDist::Git(sdist)) => {
            debug!("Fetching source distribution from Git: {}", sdist.url);

            let DirectGitUrl { url, subdirectory } = DirectGitUrl::try_from(&sdist.url)?;

            let git_dir = cache.join(GIT_CACHE);
            let source = GitSource::new(url, git_dir);
            let sdist_file = tokio::task::spawn_blocking(move || source.fetch())
                .await??
                .into();

            Ok(Download::SourceDist(SourceDistDownload {
                dist,
                sdist_file,
                subdirectory,
            }))
        }
    }
}

pub trait Reporter: Send + Sync {
    /// Callback to invoke when a wheel is downloaded.
    fn on_download_progress(&self, download: &Download);

    /// Callback to invoke when the operation is complete.
    fn on_download_complete(&self);
}

/// A downloaded wheel that's stored in-memory.
#[derive(Debug)]
pub struct InMemoryWheel {
    /// The remote distribution from which this wheel was downloaded.
    pub(crate) dist: Dist,
    /// The contents of the wheel.
    pub(crate) buffer: Vec<u8>,
}

/// A downloaded wheel that's stored on-disk.
#[derive(Debug)]
pub struct DiskWheel {
    /// The remote distribution from which this wheel was downloaded.
    pub(crate) dist: Dist,
    /// The path to the downloaded wheel.
    pub(crate) path: PathBuf,
}

/// A downloaded wheel.
#[derive(Debug)]
pub enum WheelDownload {
    InMemory(InMemoryWheel),
    Disk(DiskWheel),
}

impl WheelDownload {
    /// Return the [`Dist`] from which this wheel was downloaded.
    pub fn remote(&self) -> &Dist {
        match self {
            WheelDownload::InMemory(wheel) => &wheel.dist,
            WheelDownload::Disk(wheel) => &wheel.dist,
        }
    }
}

/// A downloaded source distribution.
#[derive(Debug, Clone)]
pub struct SourceDistDownload {
    /// The remote distribution from which this source distribution was downloaded.
    pub(crate) dist: Dist,
    /// The path to the downloaded archive or directory.
    pub(crate) sdist_file: PathBuf,
    /// The subdirectory within the archive or directory.
    pub(crate) subdirectory: Option<PathBuf>,
}

/// A downloaded distribution, either a wheel or a source distribution.
#[derive(Debug)]
pub enum Download {
    Wheel(WheelDownload),
    SourceDist(SourceDistDownload),
}

impl std::fmt::Display for Download {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Download::Wheel(wheel) => write!(f, "{wheel}"),
            Download::SourceDist(sdist) => write!(f, "{sdist}"),
        }
    }
}

impl std::fmt::Display for WheelDownload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WheelDownload::InMemory(wheel) => write!(f, "{}", wheel.dist),
            WheelDownload::Disk(wheel) => write!(f, "{}", wheel.dist),
        }
    }
}

impl std::fmt::Display for SourceDistDownload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.dist)
    }
}
