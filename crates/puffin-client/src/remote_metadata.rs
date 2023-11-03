use std::io;
use std::path::Path;

use async_http_range_reader::AsyncHttpRangeReader;
use async_zip::error::ZipError;
use async_zip::tokio::read::seek::ZipFileReader;
use fs_err::tokio as fs;
use thiserror::Error;
use tokio_util::compat::TokioAsyncReadCompatExt;
use url::Url;

use distribution_filename::WheelFilename;
use puffin_cache::CanonicalUrl;

const WHEEL_METADATA_FROM_ZIP_CACHE: &str = "wheel-metadata-v0";

#[derive(Debug, Error)]
pub enum WheelMetadataFromRemoteZipError {
    #[error("{0}")]
    InvalidWheel(String),
    #[error(transparent)]
    Zip(#[from] ZipError),
}

/// Try to read the cached METADATA previously extracted from a remote zip, if it exists
pub(crate) async fn wheel_metadata_get_cached(url: &Url, cache: Option<&Path>) -> Option<String> {
    // TODO(konstin): Actual good cache layout
    let cache1 = cache?;
    let path = cache1
        .join(WHEEL_METADATA_FROM_ZIP_CACHE)
        .join(puffin_cache::digest(&CanonicalUrl::new(url)));
    if !path.is_file() {
        return None;
    }
    fs::read_to_string(path).await.ok()
}

/// Write the cached METADATA extracted from a remote zip to the cache
pub(crate) async fn wheel_metadata_write_cache(
    url: &Url,
    cache: Option<&Path>,
    metadata: &str,
) -> io::Result<()> {
    // TODO(konstin): Don't cache the whole text, cache the json version with the description
    // stripped instead.
    let Some(cache) = cache else {
        return Ok(());
    };
    // TODO(konstin): Actual good cache layout
    let dir = cache.join(WHEEL_METADATA_FROM_ZIP_CACHE);
    fs::create_dir_all(&dir).await?;
    let path = dir.join(puffin_cache::digest(&CanonicalUrl::new(url)));
    fs::write(path, metadata).await
}

pub(crate) async fn wheel_metadata_from_remote_zip(
    filename: WheelFilename,
    reader: &mut AsyncHttpRangeReader,
) -> Result<String, WheelMetadataFromRemoteZipError> {
    // Make sure we have the back part of the stream.
    // Best guess for the central directory size inside the zip
    const CENTRAL_DIRECTORY_SIZE: u64 = 16384;
    // Because the zip index is at the back
    reader
        .prefetch(reader.len().saturating_sub(CENTRAL_DIRECTORY_SIZE)..reader.len())
        .await;

    // Construct a zip reader to uses the stream.
    let mut reader = ZipFileReader::new(reader.compat()).await?;

    let dist_info_matcher = format!(
        "{}-{}",
        filename.distribution.as_dist_info_name(),
        filename.version
    )
    .to_lowercase();
    let metadatas: Vec<_> = reader
        .file()
        .entries()
        .iter()
        .enumerate()
        .filter_map(|(idx, e)| {
            let name = e.entry().filename().as_str().ok()?;
            let (dir, file) = name.split_once('/')?;
            let dir = dir.strip_suffix(".dist-info")?;
            if dir.to_lowercase() == dist_info_matcher && file == "METADATA" {
                Some((idx, e))
            } else {
                None
            }
        })
        .collect();
    let (metadata_idx, metadata_entry) = match metadatas[..] {
        [] => {
            return Err(WheelMetadataFromRemoteZipError::InvalidWheel(
                "Missing .dist-info directory".to_string(),
            ));
        }
        [(metadata_idx, metadata_entry)] => (metadata_idx, metadata_entry),
        _ => {
            return Err(WheelMetadataFromRemoteZipError::InvalidWheel(format!(
                "Multiple .dist-info directories: {}",
                metadatas
                    .iter()
                    .map(
                        |(_, entry)| String::from_utf8_lossy(entry.entry().filename().as_bytes())
                            .to_string()
                    )
                    .collect::<Vec<String>>()
                    .join(", ")
            )));
        }
    };

    let offset = metadata_entry.header_offset();
    let size = metadata_entry.entry().compressed_size()
        + 30 // Header size in bytes
        + metadata_entry.entry().filename().as_bytes().len() as u64;

    // The zip archive uses as BufReader which reads in chunks of 8192. To ensure we prefetch
    // enough data we round the size up to the nearest multiple of the buffer size.
    let buffer_size = 8192;
    let size = ((size + buffer_size - 1) / buffer_size) * buffer_size;

    // Fetch the bytes from the zip archive that contain the requested file.
    reader
        .inner_mut()
        .get_mut()
        .prefetch(offset..offset + size)
        .await;

    // Read the contents of the metadata.json file
    let mut contents = String::new();
    reader
        .reader_with_entry(metadata_idx)
        .await?
        .read_to_string_checked(&mut contents)
        .await?;

    Ok(contents)
}
