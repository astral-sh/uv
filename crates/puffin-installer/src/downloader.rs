use std::path::Path;

use anyhow::Result;
use cacache::{Algorithm, Integrity};
use rayon::iter::ParallelBridge;
use rayon::iter::ParallelIterator;
use tempfile::TempDir;
use tokio::task::JoinSet;
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tracing::debug;
use url::Url;
use zip::ZipArchive;

use pep440_rs::Version;
use puffin_client::PypiClient;
use puffin_interpreter::PythonExecutable;
use puffin_package::package_name::PackageName;

use crate::cache::WheelCache;
use crate::distribution::{Distribution, RemoteDistribution};
use crate::vendor::CloneableSeekableReader;

pub struct Downloader<'a> {
    python: &'a PythonExecutable,
    client: &'a PypiClient,
    cache: Option<&'a Path>,
    reporter: Option<Box<dyn Reporter>>,
}

impl<'a> Downloader<'a> {
    /// Initialize a new downloader.
    pub fn new(
        python: &'a PythonExecutable,
        client: &'a PypiClient,
        cache: Option<&'a Path>,
    ) -> Self {
        Self {
            python,
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
    pub async fn download(&'a self, wheels: &'a [Distribution]) -> Result<DownloadSet<'a>> {
        // Create the wheel cache subdirectory, if necessary.
        let wheel_cache = self.cache.map(WheelCache::new);
        if let Some(wheel_cache) = wheel_cache.as_ref() {
            wheel_cache.init().await?;
        }

        // Phase 1: Fetch the wheels in parallel.
        let mut fetches = JoinSet::new();
        let mut downloads = Vec::with_capacity(wheels.len());
        for wheel in wheels {
            let Distribution::Remote(remote) = wheel else {
                continue;
            };

            debug!("Downloading wheel: {}", remote.file().filename);

            fetches.spawn(fetch_wheel(
                remote.clone(),
                self.client.clone(),
                self.cache.map(Path::to_path_buf),
            ));
        }

        while let Some(result) = fetches.join_next().await.transpose()? {
            downloads.push(result?);
        }

        let staging = tempfile::tempdir()?;

        // Phase 2: Unpack the wheels into the cache.
        for download in downloads {
            let remote = download.remote.clone();

            debug!("Unpacking wheel: {}", remote.file().filename);

            // Unzip the wheel.
            tokio::task::spawn_blocking({
                let target = staging.path().join(remote.id());
                move || unzip_wheel(download, &target)
            })
            .await??;

            // Write the unzipped wheel to the cache (atomically).
            if let Some(wheel_cache) = wheel_cache.as_ref() {
                debug!("Caching wheel: {}", remote.file().filename);
                tokio::fs::rename(
                    staging.path().join(remote.id()),
                    wheel_cache.entry(&remote.id()),
                )
                .await?;
            }

            if let Some(reporter) = self.reporter.as_ref() {
                reporter.on_download_progress(remote.name(), remote.version());
            }
        }

        if let Some(reporter) = self.reporter.as_ref() {
            reporter.on_download_complete();
        }

        Ok(DownloadSet {
            python: self.python,
            wheel_cache,
            wheels,
            staging,
        })
    }
}

#[derive(Debug)]
pub struct DownloadSet<'a> {
    pub(crate) python: &'a PythonExecutable,
    pub(crate) wheel_cache: Option<WheelCache<'a>>,
    pub(crate) wheels: &'a [Distribution],
    pub(crate) staging: TempDir,
}

#[derive(Debug, Clone)]
struct InMemoryDistribution {
    /// The remote file from which this wheel was downloaded.
    remote: RemoteDistribution,
    /// The contents of the wheel.
    buffer: Vec<u8>,
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

/// Write a wheel into the target directory.
fn unzip_wheel(wheel: InMemoryDistribution, target: &Path) -> Result<()> {
    // Read the wheel into a buffer.
    let reader = std::io::Cursor::new(wheel.buffer);
    let archive = ZipArchive::new(CloneableSeekableReader::new(reader))?;

    // Unzip in parallel.
    (0..archive.len())
        .par_bridge()
        .map(|file_number| {
            let mut archive = archive.clone();
            let mut file = archive.by_index(file_number)?;

            // Determine the path of the file within the wheel.
            let file_path = match file.enclosed_name() {
                Some(path) => path.to_owned(),
                None => return Ok(()),
            };

            // Create necessary parent directories.
            let path = target.join(file_path);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            // Write the file.
            let mut outfile = std::fs::File::create(&path)?;
            std::io::copy(&mut file, &mut outfile)?;

            // Set permissions.
            #[cfg(unix)]
            {
                use std::fs::Permissions;
                use std::os::unix::fs::PermissionsExt;

                if let Some(mode) = file.unix_mode() {
                    std::fs::set_permissions(&path, Permissions::from_mode(mode))?;
                }
            }

            Ok(())
        })
        .collect::<Result<_>>()
}

pub trait Reporter: Send + Sync {
    /// Callback to invoke when a wheel is downloaded.
    fn on_download_progress(&self, name: &PackageName, version: &Version);

    /// Callback to invoke when the download is complete.
    fn on_download_complete(&self);
}
