//! Read metadata from wheels and source distributions.
//!
//! This module reads all fields exhaustively. The fields are defined in the [Core metadata
//! specification](https://packaging.python.org/en/latest/specifications/core-metadata/).

use futures::executor::block_on;
use futures::io::AllowStdIo;
use std::io;
use std::path::Path;
use thiserror::Error;
use tokio::io::AsyncReadExt;
use tokio_util::compat::FuturesAsyncReadCompatExt;
use uv_distribution_filename::WheelFilename;
use uv_normalize::InvalidNameError;
use uv_pypi_types::ResolutionMetadata;

pub use dist_info_stem::DistInfoStem;

mod dist_info_stem;

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
    #[error("Bad CRC (got {computed:08x}, expected {expected:08x}) for file: {path}")]
    BadCrc32 {
        path: String,
        computed: u32,
        expected: u32,
    },
    #[error("Failed to read from zip file")]
    AsyncZip(#[source] async_zip::error::ZipError),
    #[error(
        "Archive contains a file with an unsupported compression method; files must be compressed with 'stored', 'DEFLATE', or 'zstd'"
    )]
    UnsupportedCompression,
    // No `#[from]` to enforce manual review of `io::Error` sources.
    #[error(transparent)]
    Io(io::Error),
}

impl From<async_zip::error::ZipError> for Error {
    fn from(err: async_zip::error::ZipError) -> Self {
        match err {
            async_zip::error::ZipError::CompressionNotSupported(_) => Self::UnsupportedCompression,
            o => Self::AsyncZip(o),
        }
    }
}

/// Find the `.dist-info` directory in a zipped wheel.
///
/// Returns the validated [`DistInfoStem`].
///
/// Reference implementation: <https://github.com/pypa/pip/blob/36823099a9cdd83261fdbc8c1d2a24fa2eea72ca/src/pip/_internal/utils/wheel.py#L38>
pub fn find_archive_dist_info<'a, T: Copy>(
    filename: &WheelFilename,
    files: impl Iterator<Item = (T, &'a str)>,
) -> Result<(T, DistInfoStem<'a>), Error> {
    let metadatas: Vec<_> = files
        .filter_map(|(payload, path)| {
            let (dist_info_dir, file) = path.split_once('/')?;
            if file != "METADATA" {
                return None;
            }
            let dist_info_stem = dist_info_dir.strip_suffix(".dist-info")?;
            Some((payload, dist_info_stem))
        })
        .collect();

    // Like `pip`, assert that there is exactly one `.dist-info` directory.
    let (payload, dist_info_stem) = match metadatas[..] {
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

    Ok((payload, DistInfoStem::new(dist_info_stem, &filename.name)?))
}

/// Return the validated [`DistInfoStem`] if the path is a `METADATA` entry.
fn metadata_entry<'a>(
    path: &'a str,
    filename: &WheelFilename,
) -> Result<Option<DistInfoStem<'a>>, Error> {
    let Some((dist_info_dir, file)) = path.split_once('/') else {
        return Ok(None);
    };
    if file != "METADATA" {
        return Ok(None);
    }
    let Some(dist_info_stem) = dist_info_dir.strip_suffix(".dist-info") else {
        return Ok(None);
    };

    DistInfoStem::new(dist_info_stem, &filename.name).map(Some)
}

/// Given an archive, read the `METADATA` from the `.dist-info` directory.
pub fn read_archive_metadata(
    filename: &WheelFilename,
    reader: impl std::io::BufRead + std::io::Seek + Unpin,
) -> Result<Vec<u8>, Error> {
    block_on(async {
        let mut zip_reader =
            async_zip::base::read::seek::ZipFileReader::new(AllowStdIo::new(reader)).await?;

        let (metadata_index, _dist_info_stem) = find_archive_dist_info(
            filename,
            zip_reader
                .file()
                .entries()
                .iter()
                .enumerate()
                .filter_map(|(index, entry)| Some((index, entry.filename().as_str().ok()?))),
        )?;

        let mut buffer = Vec::new();
        zip_reader
            .reader_with_entry(metadata_index)
            .await?
            .read_to_end_checked(&mut buffer)
            .await?;

        Ok(buffer)
    })
}

/// Find the `.dist-info` directory in an unzipped wheel.
///
/// See: <https://github.com/PyO3/python-pkginfo-rs>
fn find_flat_dist_info(
    filename: &WheelFilename,
    path: impl AsRef<Path>,
) -> Result<DistInfoStem<'static>, Error> {
    // Iterate over `path` to find the `.dist-info` directory. It should be at the top-level.
    let Some(dist_info_stem) = fs_err::read_dir(path.as_ref())
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

                let dist_info_stem = path.file_stem()?.to_str()?;
                Some(dist_info_stem.to_string())
            } else {
                None
            }
        })
    else {
        return Err(Error::MissingDistInfo);
    };

    DistInfoStem::new(dist_info_stem, &filename.name)
}

/// Read the wheel `METADATA` metadata from a `.dist-info` directory.
fn read_dist_info_metadata(
    dist_info_stem: &DistInfoStem<'_>,
    wheel: impl AsRef<Path>,
) -> Result<Vec<u8>, Error> {
    let metadata_file = wheel
        .as_ref()
        .join(format!("{dist_info_stem}.dist-info/METADATA"));
    fs_err::read(metadata_file).map_err(Error::Io)
}

/// Read and parse a wheel's `METADATA` file from a zip stream without seeking.
pub async fn read_metadata_async_stream<R: futures::AsyncRead + Unpin>(
    filename: &WheelFilename,
    debug_path: &str,
    reader: R,
) -> Result<ResolutionMetadata, Error> {
    let reader = futures::io::BufReader::with_capacity(128 * 1024, reader);
    let mut zip = async_zip::base::read::stream::ZipFileReader::new(reader);

    while let Some(mut entry) = zip.next_with_entry().await? {
        // Find the `METADATA` entry.
        let path = entry.reader().entry().filename().as_str()?.to_owned();

        if metadata_entry(&path, filename)?.is_some() {
            let mut reader = entry.reader_mut().compat();
            let mut contents = Vec::new();
            reader.read_to_end(&mut contents).await.map_err(Error::Io)?;

            // Validate the CRC of any file we unpack
            // (It would be nice if async_zip made it harder to Not do this...)
            let reader = reader.into_inner();
            let computed = reader.compute_hash();
            let expected = reader.entry().crc32();
            if computed != expected {
                let error = Error::BadCrc32 {
                    path,
                    computed,
                    expected,
                };
                // There are some cases where we fail to get a proper CRC.
                // This is probably connected to out-of-line data descriptors
                // which are problematic to access in a streaming context.
                // In those cases the CRC seems to reliably be stubbed inline as 0,
                // so we downgrade this to a (hidden-by-default) warning.
                if expected == 0 {
                    tracing::warn!("presumed missing CRC: {error}");
                } else {
                    return Err(error);
                }
            }

            let metadata = ResolutionMetadata::parse_metadata(&contents)
                .map_err(|err| Error::InvalidMetadata(debug_path.to_string(), Box::new(err)))?;
            return Ok(metadata);
        }

        // Close current file to get access to the next one. See docs:
        // https://docs.rs/async_zip/0.0.16/async_zip/base/read/stream/
        (.., zip) = entry.skip().await?;
    }

    Err(Error::MissingDistInfo)
}

/// Read the [`ResolutionMetadata`] from an unzipped wheel.
pub fn read_flat_wheel_metadata(
    filename: &WheelFilename,
    wheel: impl AsRef<Path>,
) -> Result<ResolutionMetadata, Error> {
    let dist_info_stem = find_flat_dist_info(filename, &wheel)?;
    let metadata = read_dist_info_metadata(&dist_info_stem, &wheel)?;
    ResolutionMetadata::parse_metadata(&metadata).map_err(|err| {
        Error::InvalidMetadata(
            format!("{dist_info_stem}.dist-info/METADATA"),
            Box::new(err),
        )
    })
}

#[cfg(test)]
mod test {
    use super::{DistInfoStem, find_archive_dist_info, metadata_entry};
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
        let (_, dist_info_stem) =
            find_archive_dist_info(&filename, files.into_iter().map(|file| (file, file))).unwrap();
        assert_eq!(dist_info_stem.as_str(), "Mastodon.py-1.5.1");
    }

    #[test]
    fn test_dist_info_stem_compatibility() {
        let filename = WheelFilename::from_str("friendly_bard-1.0-py3-none-any.whl")
            .expect("valid wheel filename");
        for name in [
            "friendly-bard-1.0",
            "FrIeNdLy-._.-bArD-1.0",
            "friendly_bard-1.0+local",
            "friendly_bard",
            "friendly_bard_extra-2.0",
        ] {
            let path = format!("{name}.dist-info/METADATA");
            let ((), archive_name) =
                find_archive_dist_info(&filename, [((), path.as_str())].into_iter())
                    .expect("accepted archive directory name");
            let stream_name = metadata_entry(&path, &filename)
                .expect("accepted streaming directory name")
                .expect("metadata entry");
            let owned_name = DistInfoStem::new(name.to_owned(), &filename.name)
                .expect("accepted owned directory name");

            assert_eq!(archive_name.as_str(), name);
            assert_eq!(stream_name.as_str(), name);
            assert_eq!(owned_name.as_str(), name);
        }
    }

    #[test]
    fn test_dist_info_stem_mismatch() {
        let filename = WheelFilename::from_str("friendly_bard-1.0-py3-none-any.whl")
            .expect("valid wheel filename");
        let name = "other_package-1.0";
        let path = format!("{name}.dist-info/METADATA");
        let expected = "The .dist-info directory other_package-1.0 does not start with the normalized package name: friendly-bard";

        assert_eq!(
            find_archive_dist_info(&filename, [((), path.as_str())].into_iter())
                .expect_err("mismatched archive directory name")
                .to_string(),
            expected
        );
        assert_eq!(
            metadata_entry(&path, &filename)
                .expect_err("mismatched streaming directory name")
                .to_string(),
            expected
        );
        assert_eq!(
            DistInfoStem::new(name.to_owned(), &filename.name)
                .expect_err("mismatched owned directory name")
                .to_string(),
            expected
        );

        assert!(
            metadata_entry("other_package-1.0.dist-info/WHEEL", &filename)
                .expect("non-metadata entry")
                .is_none()
        );
        assert_eq!(
            find_archive_dist_info(
                &filename,
                [
                    ((), path.as_str()),
                    ((), "friendly_bard-1.0.dist-info/METADATA"),
                ]
                .into_iter(),
            )
            .expect_err("multiple metadata directories")
            .to_string(),
            "Multiple .dist-info directories found: other_package-1.0, friendly_bard-1.0"
        );
    }
}
