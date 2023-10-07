use std::path::Path;
use std::str::FromStr;

use anyhow::Result;
use cacache::{Algorithm, Integrity};
use tokio::task::JoinSet;
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tracing::debug;
use url::Url;

use install_wheel_rs::{install_wheel, InstallLocation};
use puffin_client::{File, PypiClient};
use puffin_interpreter::PythonExecutable;
use wheel_filename::WheelFilename;

/// Install a set of wheels into a Python virtual environment.
pub async fn install(
    wheels: &[File],
    python: &PythonExecutable,
    client: &PypiClient,
    cache: Option<&Path>,
) -> Result<()> {
    // Fetch the wheels in parallel.
    let mut fetches = JoinSet::new();
    let mut results = Vec::with_capacity(wheels.len());
    for wheel in wheels {
        fetches.spawn(fetch_wheel(
            wheel.clone(),
            client.clone(),
            cache.map(Path::to_path_buf),
        ));
    }
    while let Some(result) = fetches.join_next().await.transpose()? {
        results.push(result?);
    }

    // Install each wheel.
    let location = InstallLocation::Venv {
        venv_base: python.venv().to_path_buf(),
        python_version: python.simple_version(),
    };
    let locked_dir = location.acquire_lock()?;
    for wheel in results {
        let reader = std::io::Cursor::new(wheel.buffer);
        let filename = WheelFilename::from_str(&wheel.file.filename)?;

        // TODO(charlie): Should this be async?
        install_wheel(
            &locked_dir,
            reader,
            &filename,
            false,
            false,
            &[],
            "",
            python.executable(),
        )?;
    }

    Ok(())
}

#[derive(Debug)]
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
