use std::{ffi::OsString, path::PathBuf};

use crate::validate_archive_member_name;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O operation failed during extraction")]
    Io(#[source] std::io::Error),
    #[error("Invalid zip file structure")]
    AsyncZip(#[source] async_zip::error::ZipError),
    #[error("Invalid tar file")]
    Tar(
        #[source]
        #[from]
        tokio_tar::TarError,
    ),
    #[error("Invalid tar file")]
    TarCodec(
        #[source]
        #[from]
        tar_codec::ExtractError<tar_codec::DecodeError>,
    ),
    #[error(
        "The top-level of the archive must only contain a list directory, but it contains: {0:?}"
    )]
    NonSingularArchive(Vec<OsString>),
    #[error("The top-level of the archive must only contain a list directory, but it's empty")]
    EmptyArchive,
    #[error("ZIP local header filename at offset {offset} does not use UTF-8 encoding")]
    LocalHeaderNotUtf8 { offset: u64 },
    #[error("ZIP central directory entry filename at index {index} does not use UTF-8 encoding")]
    CentralDirectoryEntryNotUtf8 { index: u64 },
    #[error("Bad CRC (got {computed:08x}, expected {expected:08x}) for file: {}", path.display())]
    BadCrc32 {
        path: PathBuf,
        computed: u32,
        expected: u32,
    },
    #[error("Bad uncompressed size (got {computed:08x}, expected {expected:08x}) for file: {}", path.display())]
    BadUncompressedSize {
        path: PathBuf,
        computed: u64,
        expected: u64,
    },
    #[error("Bad compressed size (got {computed:08x}, expected {expected:08x}) for file: {}", path.display())]
    BadCompressedSize {
        path: PathBuf,
        computed: u64,
        expected: u64,
    },
    #[error("ZIP file contains multiple entries with different contents for: {}", path.display())]
    DuplicateLocalFileHeader { path: PathBuf },
    #[error("ZIP file contains a local file header without a corresponding central-directory record entry for: {} ({offset})", path.display())]
    MissingCentralDirectoryEntry { path: PathBuf, offset: u64 },
    #[error("ZIP file contains an end-of-central-directory record entry, but no local file header for: {} ({offset}", path.display())]
    MissingLocalFileHeader { path: PathBuf, offset: u64 },
    #[error("ZIP file uses conflicting paths for the local file header at {} (got {}, expected {})", offset, local_path.display(), central_directory_path.display())]
    ConflictingPaths {
        offset: u64,
        local_path: PathBuf,
        central_directory_path: PathBuf,
    },
    #[error("ZIP file uses conflicting checksums for the local file header and central-directory record (got {local_crc32}, expected {central_directory_crc32}) for: {} ({offset})", path.display())]
    ConflictingChecksums {
        path: PathBuf,
        offset: u64,
        local_crc32: u32,
        central_directory_crc32: u32,
    },
    #[error("ZIP file uses conflicting compressed sizes for the local file header and central-directory record (got {local_compressed_size}, expected {central_directory_compressed_size}) for: {} ({offset})", path.display())]
    ConflictingCompressedSizes {
        path: PathBuf,
        offset: u64,
        local_compressed_size: u64,
        central_directory_compressed_size: u64,
    },
    #[error("ZIP file uses conflicting uncompressed sizes for the local file header and central-directory record (got {local_uncompressed_size}, expected {central_directory_uncompressed_size}) for: {} ({offset})", path.display())]
    ConflictingUncompressedSizes {
        path: PathBuf,
        offset: u64,
        local_uncompressed_size: u64,
        central_directory_uncompressed_size: u64,
    },
    #[error("ZIP file contains trailing contents after the end-of-central-directory record")]
    TrailingContents,
    #[error(
        "ZIP file reports a number of entries in the central directory that conflicts with the actual number of entries (got {actual}, expected {expected})"
    )]
    ConflictingNumberOfEntries { actual: u64, expected: u64 },
    #[error("Data descriptor is missing for file: {}", path.display())]
    MissingDataDescriptor { path: PathBuf },
    #[error("File contains an unexpected data descriptor: {}", path.display())]
    UnexpectedDataDescriptor { path: PathBuf },
    #[error(
        "ZIP file end-of-central-directory record contains a comment that appears to be an embedded ZIP file"
    )]
    ZipInZip,
    #[error("ZIP64 end-of-central-directory record contains unsupported extensible data")]
    ExtensibleData,
    #[error("ZIP file end-of-central-directory record contains multiple entries with the same path, but conflicting modes: {}", path.display())]
    DuplicateExecutableFileHeader { path: PathBuf },
    #[error("Archive contains a file with an empty filename")]
    EmptyFilename,
    #[error("Archive contains unacceptable filename: {filename}")]
    UnacceptableFilename { filename: String },
}

impl From<async_zip::error::ZipError> for Error {
    fn from(err: async_zip::error::ZipError) -> Self {
        let async_zip::error::ZipError::FileNameContainsNul { filename } = err else {
            return Self::AsyncZip(err);
        };

        let filename = String::from_utf8_lossy(&filename);
        validate_archive_member_name(&filename)
            .expect_err("a filename containing an embedded NUL must be rejected")
    }
}

impl Error {
    /// When reading from a ZIP archive, the error can either be an I/O error from the underlying
    /// operating system, or an error with the archive. Both get wrapped into an I/O error through
    /// e.g., `io::copy`. This method extracts ZIP errors to distinguish them from invalid archives.
    pub(crate) fn io_or_zip(err: std::io::Error) -> Self {
        if err.kind() != std::io::ErrorKind::Other {
            return Self::Io(err);
        }

        let err = match err.downcast::<async_zip::error::ZipError>() {
            Ok(zip_err) => return Self::AsyncZip(zip_err),
            Err(err) => err,
        };
        Self::Io(err)
    }

    /// When reading a tar archive, the error can either be an I/O error from the underlying
    /// reader or an error with the archive. Both get wrapped into an I/O error through operations
    /// such as `io::copy`. This method extracts tar errors to distinguish them from I/O errors.
    pub(crate) fn io_or_tar(err: std::io::Error) -> Self {
        if err.kind() != std::io::ErrorKind::Other {
            return Self::Io(err);
        }

        match err.downcast::<tokio_tar::TarError>() {
            Ok(tar_err) => Self::Tar(tar_err),
            Err(err) => Self::Io(err),
        }
    }

    /// Returns `true` if the error is due to the server not supporting HTTP streaming. Most
    /// commonly, this is due to serving ZIP files with features that are incompatible with
    /// streaming, like data descriptors.
    pub fn is_http_streaming_unsupported(&self) -> bool {
        matches!(
            self,
            Self::AsyncZip(async_zip::error::ZipError::FeatureNotSupported(_))
        )
    }

    /// Returns `true` if the error is due to HTTP streaming request failed.
    pub fn is_http_streaming_failed(&self) -> bool {
        fn contains_reqwest_error(error: &(dyn std::error::Error + 'static)) -> bool {
            if error.downcast_ref::<reqwest::Error>().is_some() {
                return true;
            }
            // `std::io::Error::source` forwards to the source of its custom error and can skip the
            // custom error itself, so inspect `get_ref` before following the standard chain.
            if let Some(error) = error.downcast_ref::<std::io::Error>()
                && let Some(inner) = error.get_ref()
                && contains_reqwest_error(inner)
            {
                return true;
            }
            error.source().is_some_and(contains_reqwest_error)
        }

        if matches!(
            self,
            Self::AsyncZip(async_zip::error::ZipError::UpstreamReadError(_))
        ) {
            return true;
        }
        contains_reqwest_error(self)
    }
}
