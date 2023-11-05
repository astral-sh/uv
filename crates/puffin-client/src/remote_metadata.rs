use std::io;
use std::path::Path;

use async_http_range_reader::AsyncHttpRangeReader;
use async_zip::tokio::read::seek::ZipFileReader;
use fs_err::tokio as fs;
use tokio_util::compat::TokioAsyncReadCompatExt;
use url::Url;

use distribution_filename::WheelFilename;
use install_wheel_rs::find_dist_info_metadata;
use puffin_cache::CanonicalUrl;
use puffin_package::pypi_types::Metadata21;

use crate::Error;

const WHEEL_METADATA_FROM_ZIP_CACHE: &str = "wheel-metadata-v0";

/// Try to read the cached METADATA previously extracted from a remote zip, if it exists
pub(crate) async fn wheel_metadata_get_cached(
    url: &Url,
    cache: Option<&Path>,
) -> Option<Metadata21> {
    // TODO(konstin): Actual good cache layout
    let path = cache?
        .join(WHEEL_METADATA_FROM_ZIP_CACHE)
        .join(puffin_cache::digest(&CanonicalUrl::new(url)));
    if !path.is_file() {
        return None;
    }
    let data = fs::read(path).await.ok()?;
    serde_json::from_slice(&data).ok()
}

/// Write the cached METADATA extracted from a remote zip to the cache
pub(crate) async fn wheel_metadata_write_cache(
    url: &Url,
    cache: Option<&Path>,
    metadata: &Metadata21,
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
    fs::write(path, serde_json::to_vec(metadata)?).await
}

/// Read the `.dist-info/METADATA` file from a async remote zip reader, so we avoid downloading the
/// entire wheel just for the one file.
///
/// This method is copied from <https://github.com/prefix-dev/rip/pull/66> and licensed under
/// BSD-3-Clause, see `LICENSE.BSD-3-Clause`
pub(crate) async fn wheel_metadata_from_remote_zip(
    filename: &WheelFilename,
    reader: &mut AsyncHttpRangeReader,
) -> Result<String, Error> {
    // Make sure we have the back part of the stream.
    // Best guess for the central directory size inside the zip
    const CENTRAL_DIRECTORY_SIZE: u64 = 16384;
    // Because the zip index is at the back
    reader
        .prefetch(reader.len().saturating_sub(CENTRAL_DIRECTORY_SIZE)..reader.len())
        .await;

    // Construct a zip reader to uses the stream.
    let mut reader = ZipFileReader::new(reader.compat())
        .await
        .map_err(|err| Error::Zip(filename.clone(), err))?;

    let ((metadata_idx, metadata_entry), _path) = find_dist_info_metadata(
        filename,
        reader
            .file()
            .entries()
            .iter()
            .enumerate()
            .filter_map(|(idx, e)| Some(((idx, e), e.entry().filename().as_str().ok()?))),
    )
    .map_err(|err| Error::InvalidWheel(filename.clone(), err))?;

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

    // Read the contents of the METADATA file
    let mut contents = String::new();
    reader
        .reader_with_entry(metadata_idx)
        .await
        .map_err(|err| Error::Zip(filename.clone(), err))?
        .read_to_string_checked(&mut contents)
        .await
        .map_err(|err| Error::Zip(filename.clone(), err))?;

    Ok(contents)
}
