//! Read metadata from wheels and source distributions.
//!
//! This module reads all fields exhaustively. The fields are defined in the [Core metadata
//! specification](https://packaging.python.org/en/latest/specifications/core-metadata/).

use std::io;
use std::io::{Read, Seek};
use std::path::Path;
use thiserror::Error;
use tokio::io::AsyncReadExt;
use tokio_util::compat::{FuturesAsyncReadCompatExt, TokioAsyncReadCompatExt};
use uv_distribution_filename::WheelFilename;
use uv_normalize::{DistInfoName, InvalidNameError};
use uv_pypi_types::ResolutionMetadata;
use zip::ZipArchive;

/// The caller is responsible for attaching the path or url we failed to read.
#[derive(Debug, Error)]
pub enum Error {
    #[error("Failed to read `dist-info` metadata from built wheel")]
    DistInfo,
    #[error("No .dist-info directory found")]
    MissingDistInfo,
    #[error("Multiple .dist-info directories found: {0}")]
    MultipleDistInfo(String),
    #[error(
        "The .dist-info directory does not consist of the normalized package name and version: `{0}`"
    )]
    MissingDistInfoSegments(String),
    #[error("The .dist-info directory {0} does not start with the normalized package name: {1}")]
    MissingDistInfoPackageName(String, String),
    #[error("The .dist-info directory name contains invalid characters")]
    InvalidName(#[from] InvalidNameError),
    #[error("The metadata at {0} is invalid")]
    InvalidMetadata(String, Box<uv_pypi_types::MetadataError>),
    #[error("Failed to read from zip file")]
    Zip(#[from] zip::result::ZipError),
    #[error("Failed to read from zip file")]
    AsyncZip(#[from] async_zip::error::ZipError),
    // No `#[from]` to enforce manual review of `io::Error` sources.
    #[error(transparent)]
    Io(io::Error),
}

/// Find the `.dist-info` directory in a zipped wheel.
///
/// Returns the dist info dir prefix without the `.dist-info` extension.
///
/// Reference implementation: <https://github.com/pypa/pip/blob/36823099a9cdd83261fdbc8c1d2a24fa2eea72ca/src/pip/_internal/utils/wheel.py#L38>
pub fn find_archive_dist_info<'a, T: Copy>(
    filename: &WheelFilename,
    files: impl Iterator<Item = (T, &'a str)>,
) -> Result<(T, &'a str), Error> {
    let metadatas: Vec<_> = files
        .filter_map(|(payload, path)| {
            let (dist_info_dir, file) = path.split_once('/')?;
            if file != "METADATA" {
                return None;
            }
            let dist_info_prefix = dist_info_dir.strip_suffix(".dist-info")?;
            Some((payload, dist_info_prefix))
        })
        .collect();

    // Like `pip`, assert that there is exactly one `.dist-info` directory.
    let (payload, dist_info_prefix) = match metadatas[..] {
        [] => {
            return Err(Error::MissingDistInfo);
        }
        [(payload, path)] => (payload, path),
        _ => {
            return Err(Error::MultipleDistInfo(
                metadatas
                    .into_iter()
                    .map(|(_, dist_info_dir)| dist_info_dir.to_string())
                    .collect::<Vec<_>>()
                    .join(", "),
            ));
        }
    };

    // Like `pip`, validate that the `.dist-info` directory is prefixed with the canonical
    // package name.
    let normalized_prefix = DistInfoName::new(dist_info_prefix);
    if !normalized_prefix
        .as_ref()
        .starts_with(filename.name.as_str())
    {
        return Err(Error::MissingDistInfoPackageName(
            dist_info_prefix.to_string(),
            filename.name.to_string(),
        ));
    };

    Ok((payload, dist_info_prefix))
}

/// Returns `true` if the file is a `METADATA` file in a `.dist-info` directory that matches the
/// wheel filename.
pub fn is_metadata_entry(path: &str, filename: &WheelFilename) -> Result<bool, Error> {
    let Some((dist_info_dir, file)) = path.split_once('/') else {
        return Ok(false);
    };
    if file != "METADATA" {
        return Ok(false);
    }
    let Some(dist_info_prefix) = dist_info_dir.strip_suffix(".dist-info") else {
        return Ok(false);
    };

    // Like `pip`, validate that the `.dist-info` directory is prefixed with the canonical
    // package name.
    let normalized_prefix = DistInfoName::new(dist_info_prefix);
    if !normalized_prefix
        .as_ref()
        .starts_with(filename.name.as_str())
    {
        return Err(Error::MissingDistInfoPackageName(
            dist_info_prefix.to_string(),
            filename.name.to_string(),
        ));
    };

    Ok(true)
}

/// Given an archive, read the `METADATA` from the `.dist-info` directory.
pub fn read_archive_metadata(
    filename: &WheelFilename,
    archive: &mut ZipArchive<impl Read + Seek + Sized>,
) -> Result<Vec<u8>, Error> {
    let dist_info_prefix =
        find_archive_dist_info(filename, archive.file_names().map(|name| (name, name)))?.1;

    let mut file = archive.by_name(&format!("{dist_info_prefix}.dist-info/METADATA"))?;

    #[allow(clippy::cast_possible_truncation)]
    let mut buffer = Vec::with_capacity(file.size() as usize);
    file.read_to_end(&mut buffer).map_err(Error::Io)?;

    Ok(buffer)
}

/// Find the `.dist-info` directory in an unzipped wheel.
///
/// See: <https://github.com/PyO3/python-pkginfo-rs>
pub fn find_flat_dist_info(
    filename: &WheelFilename,
    path: impl AsRef<Path>,
) -> Result<String, Error> {
    // Iterate over `path` to find the `.dist-info` directory. It should be at the top-level.
    let Some(dist_info_prefix) = fs_err::read_dir(path.as_ref())
        .map_err(Error::Io)?
        .find_map(|entry| {
            let entry = entry.ok()?;
            let file_type = entry.file_type().ok()?;
            if file_type.is_dir() {
                let path = entry.path();

                let extension = path.extension()?;
                if extension != "dist-info" {
                    return None;
                }

                let dist_info_prefix = path.file_stem()?.to_str()?;
                Some(dist_info_prefix.to_string())
            } else {
                None
            }
        })
    else {
        return Err(Error::MissingDistInfo);
    };

    // Like `pip`, validate that the `.dist-info` directory is prefixed with the canonical
    // package name.
    let normalized_prefix = DistInfoName::new(&dist_info_prefix);
    if !normalized_prefix
        .as_ref()
        .starts_with(filename.name.as_str())
    {
        return Err(Error::MissingDistInfoPackageName(
            dist_info_prefix.to_string(),
            filename.name.to_string(),
        ));
    };

    Ok(dist_info_prefix)
}

/// Read the wheel `METADATA` metadata from a `.dist-info` directory.
pub fn read_dist_info_metadata(
    dist_info_prefix: &str,
    wheel: impl AsRef<Path>,
) -> Result<Vec<u8>, Error> {
    let metadata_file = wheel
        .as_ref()
        .join(format!("{dist_info_prefix}.dist-info/METADATA"));
    fs_err::read(metadata_file).map_err(Error::Io)
}

/// Read a wheel's `METADATA` file from a zip file.
pub async fn read_metadata_async_seek(
    filename: &WheelFilename,
    reader: impl tokio::io::AsyncRead + tokio::io::AsyncSeek + Unpin,
) -> Result<Vec<u8>, Error> {
    let reader = futures::io::BufReader::new(reader.compat());
    let mut zip_reader = async_zip::base::read::seek::ZipFileReader::new(reader).await?;

    let (metadata_idx, _dist_info_prefix) = find_archive_dist_info(
        filename,
        zip_reader
            .file()
            .entries()
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| Some((index, entry.filename().as_str().ok()?))),
    )?;

    // Read the contents of the `METADATA` file.
    let mut contents = Vec::new();
    zip_reader
        .reader_with_entry(metadata_idx)
        .await?
        .read_to_end_checked(&mut contents)
        .await?;

    Ok(contents)
}

/// Like [`read_metadata_async_seek`], but doesn't use seek.
pub async fn read_metadata_async_stream<R: futures::AsyncRead + Unpin>(
    filename: &WheelFilename,
    debug_path: &str,
    reader: R,
) -> Result<ResolutionMetadata, Error> {
    let reader = futures::io::BufReader::with_capacity(128 * 1024, reader);
    let mut zip = async_zip::base::read::stream::ZipFileReader::new(reader);

    while let Some(mut entry) = zip.next_with_entry().await? {
        // Find the `METADATA` entry.
        let path = entry.reader().entry().filename().as_str()?;

        if is_metadata_entry(path, filename)? {
            let mut reader = entry.reader_mut().compat();
            let mut contents = Vec::new();
            reader.read_to_end(&mut contents).await.unwrap();

            let metadata = ResolutionMetadata::parse_metadata(&contents)
                .map_err(|err| Error::InvalidMetadata(debug_path.to_string(), Box::new(err)))?;
            return Ok(metadata);
        }

        // Close current file to get access to the next one. See docs:
        // https://docs.rs/async_zip/0.0.16/async_zip/base/read/stream/
        zip = entry.skip().await?;
    }

    Err(Error::MissingDistInfo)
}

/// Read the [`ResolutionMetadata`] from an unzipped wheel.
pub fn read_flat_wheel_metadata(
    filename: &WheelFilename,
    wheel: impl AsRef<Path>,
) -> Result<ResolutionMetadata, Error> {
    let dist_info_prefix = find_flat_dist_info(filename, &wheel)?;
    let metadata = read_dist_info_metadata(&dist_info_prefix, &wheel)?;
    ResolutionMetadata::parse_metadata(&metadata).map_err(|err| {
        Error::InvalidMetadata(
            format!("{dist_info_prefix}.dist-info/METADATA"),
            Box::new(err),
        )
    })
}

#[cfg(test)]
mod test {
    use super::find_archive_dist_info;
    use std::str::FromStr;
    use uv_distribution_filename::WheelFilename;

    #[test]
    fn test_dot_in_name() {
        let files = [
            "mastodon/Mastodon.py",
            "mastodon/__init__.py",
            "mastodon/streaming.py",
            "Mastodon.py-1.5.1.dist-info/DESCRIPTION.rst",
            "Mastodon.py-1.5.1.dist-info/metadata.json",
            "Mastodon.py-1.5.1.dist-info/top_level.txt",
            "Mastodon.py-1.5.1.dist-info/WHEEL",
            "Mastodon.py-1.5.1.dist-info/METADATA",
            "Mastodon.py-1.5.1.dist-info/RECORD",
        ];
        let filename = WheelFilename::from_str("Mastodon.py-1.5.1-py2.py3-none-any.whl").unwrap();
        let (_, dist_info_prefix) =
            find_archive_dist_info(&filename, files.into_iter().map(|file| (file, file))).unwrap();
        assert_eq!(dist_info_prefix, "Mastodon.py-1.5.1");
    }
}
