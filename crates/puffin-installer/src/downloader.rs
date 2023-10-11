use std::path::Path;

use anyhow::Result;
use cacache::{Algorithm, Integrity};
use tokio::task::JoinSet;
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tracing::debug;
use url::Url;

use pep440_rs::Version;
use puffin_client::PypiClient;
use puffin_package::package_name::PackageName;

use crate::cache::WheelCache;
use crate::distribution::RemoteDistribution;

pub struct Downloader<'a> {
    client: &'a PypiClient,
    cache: Option<&'a Path>,
    reporter: Option<Box<dyn Reporter>>,
}

impl<'a> Downloader<'a> {
    /// Initialize a new downloader.
    pub fn new(client: &'a PypiClient, cache: Option<&'a Path>) -> Self {
        Self {
            client,
            cache,
            reporter: None,
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

    /// Install a set of wheels into a Python virtual environment.
    pub async fn download(
        &'a self,
        wheels: &'a [RemoteDistribution],
        target: &'a Path,
    ) -> Result<Vec<InMemoryDistribution>> {
        // Create the wheel cache subdirectory, if necessary.
        let wheel_cache = WheelCache::new(target);
        wheel_cache.init().await?;

        // Phase 1: Fetch the wheels in parallel.
        let mut fetches = JoinSet::new();
        let mut downloads = Vec::with_capacity(wheels.len());
        for remote in wheels {
            debug!("Downloading wheel: {}", remote.file().filename);

            fetches.spawn(fetch_wheel(
                remote.clone(),
                self.client.clone(),
                self.cache.map(Path::to_path_buf),
            ));
        }

        while let Some(result) = fetches.join_next().await.transpose()? {
            let result = result?;

            if let Some(reporter) = self.reporter.as_ref() {
                reporter.on_download_progress(result.remote.name(), result.remote.version());
            }

            downloads.push(result);
        }

        if let Some(reporter) = self.reporter.as_ref() {
            reporter.on_download_complete();
        }

        Ok(downloads)
    }
}

#[derive(Debug, Clone)]
pub struct InMemoryDistribution {
    /// The remote file from which this wheel was downloaded.
    pub(crate) remote: RemoteDistribution,
    /// The contents of the wheel.
    pub(crate) buffer: Vec<u8>,
}

/// Download a wheel to a given path.
async fn fetch_wheel(
    remote: RemoteDistribution,
    client: PypiClient,
    cache: Option<impl AsRef<Path>>,
) -> Result<InMemoryDistribution> {
    // Parse the wheel's SRI.
    let sri = Integrity::from_hex(&remote.file().hashes.sha256, Algorithm::Sha256)?;

    // Read from the cache, if possible.
    if let Some(cache) = cache.as_ref() {
        if let Ok(buffer) = cacache::read_hash(&cache, &sri).await {
            debug!("Extracted wheel from cache: {:?}", remote.file().filename);
            return Ok(InMemoryDistribution { remote, buffer });
        }
    }

    let url = Url::parse(&remote.file().url)?;
    let reader = client.stream_external(&url).await?;

    // Read into a buffer.
    let mut buffer = Vec::with_capacity(remote.file().size);
    let mut reader = tokio::io::BufReader::new(reader.compat());
    tokio::io::copy(&mut reader, &mut buffer).await?;

    // Write the buffer to the cache, if possible.
    if let Some(cache) = cache.as_ref() {
        cacache::write_hash(&cache, &buffer).await?;
    }

    Ok(InMemoryDistribution { remote, buffer })
}

pub trait Reporter: Send + Sync {
    /// Callback to invoke when a wheel is downloaded.
    fn on_download_progress(&self, name: &PackageName, version: &Version);

    /// Callback to invoke when the operation is complete.
    fn on_download_complete(&self);
}
