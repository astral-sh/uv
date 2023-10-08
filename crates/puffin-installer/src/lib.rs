mod cloneable_seekable_reader;

use std::collections::HashMap;
use std::io::Cursor;
use std::iter::Zip;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::Result;
use cacache::{Algorithm, Integrity, WriteOpts};
use rayon::iter::ParallelBridge;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;
use tokio::task::JoinSet;
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tracing::debug;
use url::Url;use rayon::iter::ParallelIterator;
use zip::result::ZipError;
use zip::ZipArchive;

use install_wheel_rs::{install_wheel, InstallLocation};
use puffin_client::{File, PypiClient};
use puffin_interpreter::PythonExecutable;
use wheel_filename::WheelFilename;
use crate::cloneable_seekable_reader::{CloneableSeekableReader, HasLength};

struct ZipFileMetadata {
    sri: String,
    mode: Option<u32>,
}

struct ZipIndex {
    files: HashMap<String, ZipFileMetadata>,
}

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

    let x = results.clone();

    if let Some(cache) = cache {
        let start = std::time::Instant::now();

        for wheel in x {
            let cache = cache.to_path_buf();
            tokio::task::spawn_blocking(move || {
                cache_wheel(
                    wheel,
                    cache.to_path_buf(),
                )
            }).await??;
        }

        // // Fetch the wheels in parallel.
        // let mut fetches = JoinSet::new();
        // let mut indexes = Vec::with_capacity(wheels.len());
        // for wheel in x {
        //     fetches.spawn(cache_wheel(
        //         wheel,
        //         cache.to_path_buf(),
        //     ));
        // }
        // while let Some(result) = fetches.join_next().await.transpose()? {
        //     indexes.push(result?);
        // }

        //
        // if file.name().ends_with('/') {
        //     tokio::fs::create_dir_all(file.name()).await?;
        // } else {
        //     if let Some(parent) = out_path.parent() {
        //         directory_creator.create_dir_all(parent)?;
        //     }
        //     let out_file = File::create(&out_path).with_context(|| "Failed to create file")?;
        //     // Progress bar strategy. The overall progress across the entire zip file must be
        //     // denoted in terms of *compressed* bytes, since at the outset we don't know the uncompressed
        //     // size of each file. Yet, within a given file, we update progress based on the bytes
        //     // of uncompressed data written, once per 1MB, because that's the information that we happen
        //     // to have available. So, calculate how many compressed bytes relate to 1MB of uncompressed
        //     // data, and the remainder.
        //     let uncompressed_size = file.size();
        //     let compressed_size = file.compressed_size();
        //     let mut progress_updater = ProgressUpdater::new(
        //         |external_progress| {
        //             progress_reporter.bytes_extracted(external_progress);
        //         },
        //         compressed_size,
        //         uncompressed_size,
        //         1024 * 1024,
        //     );
        //     let mut out_file = progress_streams::ProgressWriter::new(out_file, |bytes_written| {
        //         progress_updater.progress(bytes_written as u64)
        //     });
        //     // Using a BufWriter here doesn't improve performance even on a VM with
        //     // spinny disks.
        //     std::io::copy(&mut file, &mut out_file).with_context(|| "Failed to write directory")?;
        //     progress_updater.finish();
        // }
        println!("Time to unzip: {:?}", start.elapsed());
    }


    let start = std::time::Instant::now();
    // Install each wheel.
    let location = InstallLocation::Venv {
        venv_base: python.venv().to_path_buf(),
        python_version: python.simple_version(),
    };
    let locked_dir = location.acquire_lock()?;

    for wheel in &results {
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

    println!("Time to install: {:?}", start.elapsed());

    Ok(())
}

impl HasLength for std::io::Cursor<Vec<u8>> {
    fn len(&self) -> u64 {
        self.get_ref().len() as u64
    }
}

fn cache_wheel(
    wheel: FetchedWheel,
    cache: PathBuf,
) -> Result<ZipIndex> {
    let mut index = ZipIndex {
        files: HashMap::new(),
    };

    let start = std::time::Instant::now();
    println!("Unzipping wheel");

    let reader = std::io::Cursor::new(wheel.buffer);

    // Read the zip.
    let mut archive = ZipArchive::new(CloneableSeekableReader::new(reader))?;

    // Parallelize the unzip.
    (0..archive.len()).par_bridge().map( |file_number| {
        println!("Unzipping file {}", file_number);
        let mut archive = archive.clone();
        let mut file = archive.by_index(file_number)?;

        // Determine the path of the file within the wheel.
        let file_path = match file.enclosed_name() {
            Some(path) => path.to_owned(),
            None => return Ok::<(), anyhow::Error>(())
        };
        let file_path = file_path.to_string_lossy().to_string();

        // Write the file to the content-addressed cache.
        let mut writer = WriteOpts::new()
            .algorithm(cacache::Algorithm::Xxh3)
            .open_hash_sync::<PathBuf>(cache.clone())?;
        std::io::copy(&mut file, &mut writer)?;
        let sri = writer.commit()?;

        // Record the file's metadata.
        let metadata = ZipFileMetadata {
            sri: sri.to_string(),
            mode: file.unix_mode(),
        };



        Ok::<(), anyhow::Error>(())


    }).collect::<Vec<_>>();

    // for file_number in 0..archive.len() {
    //     let mut file = archive.by_index(file_number)?;
    //
    //     // Determine the path of the file within the wheel.
    //     let file_path = match file.enclosed_name() {
    //         Some(path) => path.to_owned(),
    //         None => continue,
    //     };
    //     let file_path = file_path.to_string_lossy().to_string();
    //
    //     // Write the file to the content-addressed cache.
    //     let mut writer = WriteOpts::new()
    //         .algorithm(cacache::Algorithm::Xxh3)
    //         .open_hash_sync(cache.as_ref())?;
    //     std::io::copy(&mut file, &mut writer)?;
    //     let sri = writer.commit()?;
    //
    //     // Record the file's metadata.
    //     let metadata = ZipFileMetadata {
    //         sri: sri.to_string(),
    //         mode: file.unix_mode(),
    //     };
    //
    //     // Insert into the index.
    //     index.files.insert(file_path, metadata);
    // }

    println!("Done! {:?}", start.elapsed());

    Ok(index)
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
