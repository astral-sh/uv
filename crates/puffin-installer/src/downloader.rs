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
use puffin_distribution::source::Source;
use puffin_distribution::{RemoteDistribution, RemoteDistributionRef};
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
    pub async fn download(&self, distributions: Vec<RemoteDistribution>) -> Result<Vec<Download>> {
        // Sort the distributions by size.
        let mut distributions = distributions;
        distributions.sort_unstable_by_key(|wheel| match wheel {
            RemoteDistribution::Registry(_package, _version, file) => Reverse(file.size),
            RemoteDistribution::Url(_, _) => Reverse(usize::MIN),
        });

        // Fetch the distributions in parallel.
        let mut fetches = JoinSet::new();
        let mut downloads = Vec::with_capacity(distributions.len());
        for distribution in distributions {
            if self.no_build && !distribution.is_wheel() {
                bail!(
                    "Building source distributions is disabled, not downloading {}",
                    distribution
                );
            }

            fetches.spawn(fetch_distribution(
                distribution.clone(),
                self.client.clone(),
                self.cache.to_path_buf(),
                self.locks.clone(),
            ));
        }

        while let Some(result) = fetches.join_next().await.transpose()? {
            let result = result?;

            if let Some(reporter) = self.reporter.as_ref() {
                reporter.on_download_progress(result.remote());
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
async fn fetch_distribution(
    distribution: RemoteDistribution,
    client: RegistryClient,
    cache: PathBuf,
    locks: Arc<Locks>,
) -> Result<Download> {
    let url = distribution.url()?;
    let lock = locks.acquire(&url).await;
    let _guard = lock.lock().await;

    if distribution.is_wheel() {
        match &distribution {
            RemoteDistribution::Registry(.., file) => {
                // Fetch the wheel.
                let url = Url::parse(&file.url)?;
                let reader = client.stream_external(&url).await?;

                // If the file is greater than 5MB, write it to disk; otherwise, keep it in memory.
                let file_size = ByteSize::b(file.size as u64);
                if file_size >= ByteSize::mb(5) {
                    debug!("Fetching disk-based wheel from registry: {distribution} ({file_size})");

                    // Download the wheel to a temporary file.
                    let temp_dir = tempfile::tempdir_in(cache)?.into_path();
                    let wheel_filename = distribution.filename()?;
                    let wheel_file = temp_dir.join(wheel_filename.as_ref());
                    let mut writer = tokio::fs::File::create(&wheel_file).await?;
                    tokio::io::copy(&mut reader.compat(), &mut writer).await?;

                    Ok(Download::Wheel(Wheel::Disk(DiskWheel {
                        remote: distribution,
                        path: wheel_file,
                    })))
                } else {
                    debug!("Fetching in-memory wheel from registry: {distribution} ({file_size})");

                    // Read into a buffer.
                    let mut buffer = Vec::with_capacity(file.size);
                    let mut reader = tokio::io::BufReader::new(reader.compat());
                    tokio::io::copy(&mut reader, &mut buffer).await?;

                    Ok(Download::Wheel(Wheel::InMemory(InMemoryWheel {
                        remote: distribution,
                        buffer,
                    })))
                }
            }
            RemoteDistribution::Url(.., url) => {
                debug!("Fetching disk-based wheel from URL: {url}");

                // Fetch the wheel.
                let reader = client.stream_external(url).await?;

                // Download the wheel to a temporary file.
                let temp_dir = tempfile::tempdir_in(cache)?.into_path();
                let wheel_filename = distribution.filename()?;
                let wheel_file = temp_dir.join(wheel_filename.as_ref());
                let mut writer = tokio::fs::File::create(&wheel_file).await?;
                tokio::io::copy(&mut reader.compat(), &mut writer).await?;

                Ok(Download::Wheel(Wheel::Disk(DiskWheel {
                    remote: distribution,
                    path: wheel_file,
                })))
            }
        }
    } else {
        let distribution_ref = RemoteDistributionRef::from(&distribution);
        let source = Source::try_from(&distribution_ref)?;
        let (sdist_file, subdirectory) = match source {
            Source::RegistryUrl(url) => {
                debug!("Fetching source distribution from registry: {url}");

                let reader = client.stream_external(&url).await?;
                let mut reader = tokio::io::BufReader::new(reader.compat());

                // Download the source distribution.
                let temp_dir = tempfile::tempdir_in(cache)?.into_path();
                let sdist_filename = distribution.filename()?;
                let sdist_file = temp_dir.join(sdist_filename.as_ref());
                let mut writer = tokio::fs::File::create(&sdist_file).await?;
                tokio::io::copy(&mut reader, &mut writer).await?;

                // Registry dependencies can't specify a subdirectory.
                let subdirectory = None;

                (sdist_file, subdirectory)
            }
            Source::RemoteUrl(url, subdirectory) => {
                debug!("Fetching source distribution from URL: {url}");

                let reader = client.stream_external(url).await?;
                let mut reader = tokio::io::BufReader::new(reader.compat());

                // Download the source distribution.
                let temp_dir = tempfile::tempdir_in(cache)?.into_path();
                let sdist_filename = distribution.filename()?;
                let sdist_file = temp_dir.join(sdist_filename.as_ref());
                let mut writer = tokio::fs::File::create(&sdist_file).await?;
                tokio::io::copy(&mut reader, &mut writer).await?;

                (sdist_file, subdirectory)
            }
            Source::Git(git, subdirectory) => {
                debug!("Fetching source distribution from Git: {git}");

                let git_dir = cache.join(GIT_CACHE);
                let source = GitSource::new(git, git_dir);
                let sdist_file = tokio::task::spawn_blocking(move || source.fetch())
                    .await??
                    .into();

                (sdist_file, subdirectory)
            }
        };

        Ok(Download::SourceDistribution(SourceDistribution {
            remote: distribution,
            sdist_file,
            subdirectory,
        }))
    }
}

pub trait Reporter: Send + Sync {
    /// Callback to invoke when a wheel is downloaded.
    fn on_download_progress(&self, wheel: &RemoteDistribution);

    /// Callback to invoke when the operation is complete.
    fn on_download_complete(&self);
}

/// A downloaded wheel that's stored in-memory.
#[derive(Debug)]
pub struct InMemoryWheel {
    /// The remote file from which this wheel was downloaded.
    pub(crate) remote: RemoteDistribution,
    /// The contents of the wheel.
    pub(crate) buffer: Vec<u8>,
}

/// A downloaded wheel that's stored on-disk.
#[derive(Debug)]
pub struct DiskWheel {
    /// The remote file from which this wheel was downloaded.
    pub(crate) remote: RemoteDistribution,
    /// The path to the downloaded wheel.
    pub(crate) path: PathBuf,
}

/// A downloaded wheel.
#[derive(Debug)]
pub enum Wheel {
    InMemory(InMemoryWheel),
    Disk(DiskWheel),
}

impl Wheel {
    /// Return the [`RemoteDistribution`] from which this wheel was downloaded.
    pub fn remote(&self) -> &RemoteDistribution {
        match self {
            Wheel::InMemory(wheel) => &wheel.remote,
            Wheel::Disk(wheel) => &wheel.remote,
        }
    }
}

impl std::fmt::Display for Wheel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.remote())
    }
}

/// A downloaded source distribution.
#[derive(Debug, Clone)]
pub struct SourceDistribution {
    /// The remote file from which this wheel was downloaded.
    pub(crate) remote: RemoteDistribution,
    /// The path to the downloaded archive or directory.
    pub(crate) sdist_file: PathBuf,
    /// The subdirectory within the archive or directory.
    pub(crate) subdirectory: Option<PathBuf>,
}

impl std::fmt::Display for SourceDistribution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.remote)
    }
}

/// A downloaded distribution, either a wheel or a source distribution.
#[derive(Debug)]
pub enum Download {
    Wheel(Wheel),
    SourceDistribution(SourceDistribution),
}

impl Download {
    /// Return the [`RemoteDistribution`] from which this distribution was downloaded.
    pub fn remote(&self) -> &RemoteDistribution {
        match self {
            Download::Wheel(distribution) => distribution.remote(),
            Download::SourceDistribution(distribution) => &distribution.remote,
        }
    }
}

impl std::fmt::Display for Download {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.remote())
    }
}
