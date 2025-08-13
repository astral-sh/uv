use std::{ffi::OsString, path::PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Failed to read from zip file")]
    Zip(#[from] zip::result::ZipError),
    #[error("Failed to read from zip file")]
    AsyncZip(#[from] async_zip::error::ZipError),
    #[error("I/O operation failed during extraction")]
    Io(#[from] std::io::Error),
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
}

impl Error {
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
        match self {
            Self::AsyncZip(async_zip::error::ZipError::UpstreamReadError(_)) => true,
            Self::Io(err) => {
                if let Some(inner) = err.get_ref() {
                    inner.downcast_ref::<reqwest::Error>().is_some()
                } else {
                    false
                }
            }
            _ => false,
        }
    }
}
