use std::path::Path;

use anyhow::Result;
use cacache::{Algorithm, Integrity};
use rayon::iter::ParallelBridge;
use rayon::iter::ParallelIterator;
use tokio::task::JoinSet;
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tracing::{debug, info};
use url::Url;
use zip::ZipArchive;

use puffin_client::PypiClient;
use puffin_interpreter::PythonExecutable;

use crate::cache::WheelCache;
use crate::distribution::{Distribution, RemoteDistribution};
use crate::vendor::CloneableSeekableReader;

/// Install a set of wheels into a Python virtual environment.
pub async fn install(
    wheels: &[Distribution],
    python: &PythonExecutable,
    client: &PypiClient,
    cache: Option<&Path>,
) -> Result<()> {
    if wheels.is_empty() {
        return Ok(());
    }

    // Create the wheel cache subdirectory, if necessary.
    let wheel_cache = cache.map(WheelCache::new);
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

        debug!("Downloading: {}", remote.file().filename);

        fetches.spawn(fetch_wheel(
            remote.clone(),
            client.clone(),
            cache.map(Path::to_path_buf),
        ));
    }

    if !fetches.is_empty() {
        let s = if fetches.len() == 1 { "" } else { "s" };
        info!("Downloading {} wheel{}", fetches.len(), s);
    }

    while let Some(result) = fetches.join_next().await.transpose()? {
        downloads.push(result?);
    }

    if !downloads.is_empty() {
        let s = if downloads.len() == 1 { "" } else { "s" };
        debug!("Unpacking {} wheel{}", downloads.len(), s);
    }

    let staging = tempfile::tempdir()?;

    // Phase 2: Unpack the wheels into the cache.
    for download in downloads {
        let filename = download.remote.file().filename.clone();
        let id = download.remote.id();

        debug!("Unpacking: {}", filename);

        // Unzip the wheel.
        tokio::task::spawn_blocking({
            let target = staging.path().join(&id);
            move || unzip_wheel(download, &target)
        })
        .await??;

        // Write the unzipped wheel to the cache (atomically).
        if let Some(wheel_cache) = wheel_cache.as_ref() {
            debug!("Caching wheel: {}", filename);
            tokio::fs::rename(staging.path().join(&id), wheel_cache.entry(&id)).await?;
        }
    }

    let s = if wheels.len() == 1 { "" } else { "s" };
    info!(
        "Linking package{}: {}",
        s,
        wheels
            .iter()
            .map(Distribution::id)
            .collect::<Vec<_>>()
            .join(" ")
    );

    // Phase 3: Install each wheel.
    let location = install_wheel_rs::InstallLocation::new(
        python.venv().to_path_buf(),
        python.simple_version(),
    );
    let locked_dir = location.acquire_lock()?;

    for wheel in wheels {
        match wheel {
            Distribution::Remote(remote) => {
                let id = remote.id();
                let dir = wheel_cache.as_ref().map_or_else(
                    || staging.path().join(&id),
                    |wheel_cache| wheel_cache.entry(&id),
                );
                install_wheel_rs::unpacked::install_wheel(&locked_dir, &dir)?;
            }
            Distribution::Local(local) => {
                install_wheel_rs::unpacked::install_wheel(&locked_dir, local.path())?;
            }
        }
    }

    Ok(())
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
