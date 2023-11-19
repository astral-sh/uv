use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;

use anyhow::Result;
use bytesize::ByteSize;
use fs_err::tokio as fs;
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tracing::debug;
use url::Url;

use distribution_filename::WheelFilename;
use distribution_types::direct_url::{DirectArchiveUrl, DirectGitUrl};
use distribution_types::{BuiltDist, Dist, Identifier, Metadata, RemoteSource, SourceDist};
use platform_tags::Tags;
use puffin_client::RegistryClient;
use puffin_git::{GitSource, GitUrl};
use puffin_traits::BuildContext;
use pypi_types::Metadata21;

use crate::error::Error;
use crate::reporter::Facade;
use crate::{DiskWheel, Download, InMemoryWheel, Reporter, SourceDistDownload, WheelDownload};

// The cache subdirectory in which to store Git repositories.
const GIT_CACHE: &str = "git-v0";

// The cache subdirectory in which to store downloaded wheel archives.
const ARCHIVES_CACHE: &str = "archives-v0";

/// A high-level interface for accessing distribution metadata and source contents.
pub struct Fetcher<'a> {
    cache: &'a Path,
    reporter: Option<Arc<dyn Reporter>>,
}

impl<'a> Fetcher<'a> {
    /// Initialize a [`Fetcher`].
    pub fn new(cache: &'a Path) -> Self {
        Self {
            cache,
            reporter: None,
        }
    }

    /// Set the [`Reporter`] to use for this source distribution fetcher.
    #[must_use]
    pub fn with_reporter(self, reporter: impl Reporter + 'static) -> Self {
        Self {
            reporter: Some(Arc::new(reporter)),
            ..self
        }
    }

    /// Return the [`Metadata21`] for a distribution, if it exists in the cache.
    pub fn find_metadata(&self, dist: &Dist, tags: &Tags) -> Result<Option<Metadata21>, Error> {
        self.find_in_cache(dist, tags)
            .map(|wheel| wheel.read_dist_info())
            .transpose()
    }

    /// Fetch the [`Metadata21`] for a distribution.
    ///
    /// If the given [`Dist`] is a source distribution, the distribution will be downloaded, built,
    /// and cached.
    pub async fn fetch_metadata(
        &self,
        dist: &Dist,
        client: &RegistryClient,
        build_context: &impl BuildContext,
    ) -> Result<Metadata21> {
        match dist {
            // Fetch the metadata directly from the registry.
            Dist::Built(BuiltDist::Registry(wheel)) => {
                let metadata = client
                    .wheel_metadata(wheel.index.clone(), wheel.file.clone())
                    .await?;
                Ok(metadata)
            }
            // Fetch the distribution, then read the metadata (for built distributions), or build
            // the distribution and _then_ read the metadata (for source distributions).
            dist => match self.fetch_dist(dist, client).await? {
                Download::Wheel(wheel) => {
                    let metadata = wheel.read_dist_info()?;
                    Ok(metadata)
                }
                Download::SourceDist(sdist) => {
                    let wheel = self.build_sdist(sdist, build_context).await?;
                    let metadata = wheel.read_dist_info()?;
                    Ok(metadata)
                }
            },
        }
    }

    /// Download a distribution.
    pub async fn fetch_dist(&self, dist: &Dist, client: &RegistryClient) -> Result<Download> {
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
                        dist: dist.clone(),
                        buffer,
                    })))
                } else {
                    let size =
                        small_size.map_or("unknown size".to_string(), |size| size.to_string());
                    debug!("Fetching disk-based wheel from registry: {dist} ({size})");

                    // Create a directory for the wheel.
                    let wheel_dir = self.cache.join(ARCHIVES_CACHE).join(wheel.package_id());
                    fs::create_dir_all(&wheel_dir).await?;

                    // Download the wheel to a temporary file.
                    let wheel_filename = &wheel.file.filename;
                    let wheel_file = wheel_dir.join(wheel_filename);
                    let mut writer = tokio::fs::File::create(&wheel_file).await?;
                    tokio::io::copy(&mut reader.compat(), &mut writer).await?;

                    Ok(Download::Wheel(WheelDownload::Disk(DiskWheel {
                        dist: dist.clone(),
                        path: wheel_file,
                        temp_dir: None,
                    })))
                }
            }

            Dist::Built(BuiltDist::DirectUrl(wheel)) => {
                debug!("Fetching disk-based wheel from URL: {}", &wheel.url);

                // Create a directory for the wheel.
                let wheel_dir = self.cache.join(ARCHIVES_CACHE).join(wheel.package_id());
                fs::create_dir_all(&wheel_dir).await?;

                // Fetch the wheel.
                let reader = client.stream_external(&wheel.url).await?;

                // Download the wheel to the directory.
                let wheel_filename = wheel.filename()?;
                let wheel_file = wheel_dir.join(wheel_filename);
                let mut writer = tokio::fs::File::create(&wheel_file).await?;
                tokio::io::copy(&mut reader.compat(), &mut writer).await?;

                Ok(Download::Wheel(WheelDownload::Disk(DiskWheel {
                    dist: dist.clone(),
                    path: wheel_file,
                    temp_dir: None,
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
                let temp_dir = tempfile::tempdir_in(self.cache)?;
                let sdist_filename = sdist.filename()?;
                let sdist_file = temp_dir.path().join(sdist_filename);
                let mut writer = tokio::fs::File::create(&sdist_file).await?;
                tokio::io::copy(&mut reader.compat(), &mut writer).await?;

                Ok(Download::SourceDist(SourceDistDownload {
                    dist: dist.clone(),
                    sdist_file,
                    subdirectory: None,
                    temp_dir: Some(temp_dir),
                }))
            }

            Dist::Source(SourceDist::DirectUrl(sdist)) => {
                debug!("Fetching source distribution from URL: {}", sdist.url);

                let DirectArchiveUrl { url, subdirectory } = DirectArchiveUrl::from(&sdist.url);

                let reader = client.stream_external(&url).await?;
                let mut reader = tokio::io::BufReader::new(reader.compat());

                // Download the source distribution.
                let temp_dir = tempfile::tempdir_in(self.cache)?;
                let sdist_filename = sdist.filename()?;
                let sdist_file = temp_dir.path().join(sdist_filename);
                let mut writer = tokio::fs::File::create(&sdist_file).await?;
                tokio::io::copy(&mut reader, &mut writer).await?;

                Ok(Download::SourceDist(SourceDistDownload {
                    dist: dist.clone(),
                    sdist_file,
                    subdirectory,
                    temp_dir: Some(temp_dir),
                }))
            }

            Dist::Source(SourceDist::Git(sdist)) => {
                debug!("Fetching source distribution from Git: {}", sdist.url);

                let DirectGitUrl { url, subdirectory } = DirectGitUrl::try_from(&sdist.url)?;

                let git_dir = self.cache.join(GIT_CACHE);
                let source = GitSource::new(url, git_dir);
                let sdist_file = tokio::task::spawn_blocking(move || source.fetch())
                    .await??
                    .into();

                Ok(Download::SourceDist(SourceDistDownload {
                    dist: dist.clone(),
                    sdist_file,
                    subdirectory,
                    temp_dir: None,
                }))
            }
        }
    }

    /// Build a downloaded source distribution.
    pub async fn build_sdist(
        &self,
        dist: SourceDistDownload,
        build_context: &impl BuildContext,
    ) -> Result<WheelDownload> {
        let task = self
            .reporter
            .as_ref()
            .map(|reporter| reporter.on_build_start(&dist.dist));

        // Create a directory for the wheel.
        let wheel_dir = self
            .cache
            .join(ARCHIVES_CACHE)
            .join(dist.remote().package_id());
        fs::create_dir_all(&wheel_dir).await?;

        // Build the wheel.
        // TODO(charlie): If this is a Git dependency, we should do another checkout. If the same
        // repository is used by multiple dependencies, at multiple commits, the local checkout may now
        // point to the wrong commit.
        let disk_filename = build_context
            .build_source(
                &dist.sdist_file,
                dist.subdirectory.as_deref(),
                &wheel_dir,
                &dist.dist.to_string(),
            )
            .await?;
        let wheel_filename = wheel_dir.join(disk_filename);

        if let Some(task) = task {
            if let Some(reporter) = self.reporter.as_ref() {
                reporter.on_build_complete(&dist.dist, task);
            }
        }

        Ok(WheelDownload::Disk(DiskWheel {
            dist: dist.dist,
            path: wheel_filename,
            temp_dir: None,
        }))
    }

    /// Find a built wheel in the cache.
    fn find_in_cache(&self, dist: &Dist, tags: &Tags) -> Option<DiskWheel> {
        let wheel_dir = self.cache.join(ARCHIVES_CACHE).join(dist.distribution_id());
        let read_dir = fs_err::read_dir(wheel_dir).ok()?;
        for entry in read_dir {
            let Ok(entry) = entry else {
                continue;
            };
            let Ok(filename) =
                WheelFilename::from_str(entry.file_name().to_string_lossy().as_ref())
            else {
                continue;
            };
            if filename.is_compatible(tags) {
                return Some(DiskWheel {
                    dist: dist.clone(),
                    path: entry.path(),
                    temp_dir: None,
                });
            }
        }
        None
    }

    /// Given a remote source distribution, return a precise variant, if possible.
    ///
    /// For example, given a Git dependency with a reference to a branch or tag, return a URL
    /// with a precise reference to the current commit of that branch or tag.
    ///
    /// This method takes into account various normalizations that are independent from the Git
    /// layer. For example: removing `#subdirectory=pkg_dir`-like fragments, and removing `git+`
    /// prefix kinds.
    pub async fn precise(&self, dist: &Dist) -> Result<Option<Url>> {
        let Dist::Source(SourceDist::Git(sdist)) = dist else {
            return Ok(None);
        };

        let DirectGitUrl { url, subdirectory } = DirectGitUrl::try_from(&sdist.url)?;

        // If the commit already contains a complete SHA, short-circuit.
        if url.precise().is_some() {
            return Ok(None);
        }

        // Fetch the precise SHA of the Git reference (which could be a branch, a tag, a partial
        // commit, etc.).
        let git_dir = self.cache.join(GIT_CACHE);
        let source = if let Some(reporter) = self.reporter.clone() {
            GitSource::new(url, git_dir).with_reporter(Facade::from(reporter))
        } else {
            GitSource::new(url, git_dir)
        };
        let precise = tokio::task::spawn_blocking(move || source.fetch()).await??;
        let url = GitUrl::from(precise);

        // Re-encode as a URL.
        Ok(Some(DirectGitUrl { url, subdirectory }.into()))
    }
}
