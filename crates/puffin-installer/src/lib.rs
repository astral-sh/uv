use std::path::Path;
use std::str::FromStr;

use anyhow::Result;
use cacache::{Algorithm, Integrity};
use rayon::iter::ParallelBridge;
use rayon::iter::ParallelIterator;
use tokio::task::JoinSet;
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tracing::{debug, info, span, Level};
use url::Url;
use zip::ZipArchive;

use install_wheel_rs::{unpacked, InstallLocation};
use puffin_client::{File, PypiClient};
use puffin_interpreter::PythonExecutable;
use puffin_package::package_name::PackageName;
use wheel_filename::WheelFilename;

use crate::vendor::CloneableSeekableReader;

mod vendor;

static WHEEL_CACHE: &str = "wheels-v0";

/// Install a set of wheels into a Python virtual environment.
pub async fn install(
    wheels: &[File],
    python: &PythonExecutable,
    client: &PypiClient,
    cache: Option<&Path>,
) -> Result<()> {
    // Create the cache subdirectory, if necessary.
    if let Some(cache) = cache {
        tokio::fs::create_dir_all(cache.join(WHEEL_CACHE)).await?;
    }

    if wheels.is_empty() {
        return Ok(());
    }

    // Phase 1: Fetch the wheels in parallel.
    let mut fetches = JoinSet::new();
    let mut downloads = Vec::with_capacity(wheels.len());
    for wheel in wheels {
        // If the unzipped wheel exists in the cache, skip it.
        let key = cache_key(wheel)?;
        if let Some(cache) = cache {
            if cache.join(WHEEL_CACHE).join(&key).exists() {
                debug!("Found wheel in cache: {}", wheel.filename);
                continue;
            }
        }

        debug!("Downloading: {}", wheel.filename);

        fetches.spawn(fetch_wheel(
            wheel.clone(),
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

    let temp_dir = tempfile::tempdir()?;

    // Phase 2: Unpack the wheels into the cache.
    for wheel in downloads {
        let filename = wheel.file.filename.clone();
        let key = cache_key(&wheel.file)?;

        debug!("Unpacking: {}", filename);

        // Unzip the wheel.
        tokio::task::spawn_blocking({
            let target = temp_dir.path().join(&key);
            move || unzip_wheel(wheel, &target)
        })
        .await??;

        // Write the unzipped wheel to the cache (atomically).
        if let Some(cache) = cache {
            debug!("Caching wheel: {}", filename);
            tokio::fs::rename(
                temp_dir.path().join(&key),
                cache.join(WHEEL_CACHE).join(&key),
            )
            .await?;
        }
    }

    let s = if wheels.len() == 1 { "" } else { "s" };
    info!(
        "Linking package{}: {}",
        s,
        wheels
            .iter()
            .map(cache_key)
            .collect::<Result<Vec<_>>>()?
            .join(" ")
    );

    // Phase 3: Install each wheel.
    let location = InstallLocation::Venv {
        venv_base: python.venv().to_path_buf(),
        python_version: python.simple_version(),
    };
    let locked_dir = location.acquire_lock()?;

    for wheel in wheels {
        let key = cache_key(wheel)?;
        let dir = cache.map_or_else(
            || temp_dir.path().join(&key),
            |cache| cache.join(WHEEL_CACHE).join(&key),
        );

        let wheel_filename = WheelFilename::from_str(&wheel.filename)?;

        // TODO(charlie): Should this be async?
        unpacked::install_wheel(&locked_dir, &dir, &wheel_filename)?;
    }

    Ok(())
}

/// Return the cache key for an unzipped wheel. The cache key should be equivalent to the
/// `.dist-info` directory name, i.e., `<name>-<version>.dist-info`, where `name` is the
/// normalized package name.
fn cache_key(wheel: &File) -> Result<String> {
    let filename = WheelFilename::from_str(&wheel.filename)?;
    Ok(format!(
        "{}-{}",
        PackageName::normalize(filename.distribution),
        filename.version
    ))
}

#[derive(Debug, Clone)]
struct FetchedWheel {
    file: File,
    buffer: Vec<u8>,
}

/// Download a wheel to a given path.
async fn fetch_wheel(
    file: File,
    client: PypiClient,
    cache: Option<impl AsRef<Path>>,
) -> Result<FetchedWheel> {
    // Parse the wheel's SRI.
    let sri = Integrity::from_hex(&file.hashes.sha256, Algorithm::Sha256)?;

    // Read from the cache, if possible.
    if let Some(cache) = cache.as_ref() {
        if let Ok(buffer) = cacache::read_hash(&cache, &sri).await {
            debug!("Extracted wheel from cache: {:?}", file.filename);
            return Ok(FetchedWheel { file, buffer });
        }
    }

    let url = Url::parse(&file.url)?;
    let reader = client.stream_external(&url).await?;

    // Read into a buffer.
    let mut buffer = Vec::with_capacity(file.size);
    let mut reader = tokio::io::BufReader::new(reader.compat());
    tokio::io::copy(&mut reader, &mut buffer).await?;

    // Write the buffer to the cache, if possible.
    if let Some(cache) = cache.as_ref() {
        cacache::write_hash(&cache, &buffer).await?;
    }

    Ok(FetchedWheel { file, buffer })
}

/// Write a wheel into the target directory.
fn unzip_wheel(wheel: FetchedWheel, target: &Path) -> Result<()> {
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
