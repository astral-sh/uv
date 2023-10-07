use std::path::Path;
use std::str::FromStr;

use anyhow::Result;
use cacache::{Algorithm, Integrity};
use tokio::sync::Mutex;
use tokio::task::JoinSet;
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tracing::debug;
use url::Url;
use zip::ZipArchive;

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
        let reader = std::io::Cursor::new(&wheel.buffer);

        // Read the zip.
        let mut archive = ZipArchive::new(reader)?;

        for file_number in 0..archive.len() {
            let file = archive.by_index(file_number)?;

            // Write the file to the content-addressed cache.
            if file.name().ends_with('/') {
                tokio::fs::create_dir_all(file.name()).await?;
            } else {
                if let Some(parent) = out_path.parent() {
                    directory_creator.create_dir_all(parent)?;
                }
                let out_file = File::create(&out_path).with_context(|| "Failed to create file")?;
                // Progress bar strategy. The overall progress across the entire zip file must be
                // denoted in terms of *compressed* bytes, since at the outset we don't know the uncompressed
                // size of each file. Yet, within a given file, we update progress based on the bytes
                // of uncompressed data written, once per 1MB, because that's the information that we happen
                // to have available. So, calculate how many compressed bytes relate to 1MB of uncompressed
                // data, and the remainder.
                let uncompressed_size = file.size();
                let compressed_size = file.compressed_size();
                let mut progress_updater = ProgressUpdater::new(
                    |external_progress| {
                        progress_reporter.bytes_extracted(external_progress);
                    },
                    compressed_size,
                    uncompressed_size,
                    1024 * 1024,
                );
                let mut out_file = progress_streams::ProgressWriter::new(out_file, |bytes_written| {
                    progress_updater.progress(bytes_written as u64)
                });
                // Using a BufWriter here doesn't improve performance even on a VM with
                // spinny disks.
                std::io::copy(&mut file, &mut out_file).with_context(|| "Failed to write directory")?;
                progress_updater.finish();
            }

        }


        let reader = std::io::Cursor::new(&wheel.buffer);
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
