use std::fmt::Display;
use std::path::{Component, Path, PathBuf};

use async_zip::base::read::cd::Entry;
use async_zip::error::ZipError;
use futures::AsyncReadExt;
use rustc_hash::{FxHashMap, FxHashSet};
use tar_codec::extract::{ExtractPolicy, LinkPolicy, SymlinkPolicy};
use tar_codec::{
    Archive, DecodeError, DecodePolicy, ExtractError, Member, PaxDecodePolicy, TarArchive,
};
use tokio_util::compat::{FuturesAsyncReadCompatExt, TokioAsyncReadCompatExt};
use tracing::{debug, warn};

use uv_distribution_filename::SourceDistExtension;
use uv_warnings::warn_user_once;

use crate::{CompressionMethod, Error, insecure_no_validate, validate_archive_member_name};

const DEFAULT_BUF_SIZE: usize = 128 * 1024;

/// Ensure the file path is safe to use as a [`Path`].
///
/// See: <https://docs.rs/zip/latest/zip/read/struct.ZipFile.html#method.enclosed_name>
pub(crate) fn enclosed_name(file_name: &str) -> Option<PathBuf> {
    if file_name.contains('\0') {
        return None;
    }
    let path = PathBuf::from(file_name);
    let mut depth = 0usize;
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => return None,
            Component::ParentDir => depth = depth.checked_sub(1)?,
            Component::Normal(_) => depth += 1,
            Component::CurDir => (),
        }
    }
    Some(path)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LocalHeaderEntry {
    /// The relative path of the entry, as computed from the local file header.
    relpath: PathBuf,
    /// The computed CRC32 checksum of the entry.
    crc32: u32,
    /// The computed compressed size of the entry.
    compressed_size: u64,
    /// The computed uncompressed size of the entry.
    uncompressed_size: u64,
    /// Whether the entry has a data descriptor.
    data_descriptor: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ComputedEntry {
    /// The computed CRC32 checksum of the entry.
    crc32: u32,
    /// The computed uncompressed size of the entry.
    uncompressed_size: u64,
    /// The computed compressed size of the entry.
    compressed_size: u64,
}

/// Unpack a `.zip` archive into the target directory, without requiring `Seek`.
///
/// This is useful for unzipping files as they're being downloaded. If the archive
/// is already fully on disk, consider using `unzip_archive`, which can use multiple
/// threads to work faster in that case.
///
/// `source_hint` is used for warning messages, to identify the source of the ZIP archive
/// beneath the reader. It might be a URL, a file path, or something else.
///
/// Returns the list of unpacked files and their sizes.
pub async fn unzip<D: Display, R: tokio::io::AsyncRead + Unpin>(
    source_hint: D,
    reader: R,
    target: impl AsRef<Path>,
) -> Result<Vec<(PathBuf, u64)>, Error> {
    // Determine whether ZIP validation is disabled.
    let skip_validation = insecure_no_validate();

    let target = target.as_ref();
    let mut reader = futures::io::BufReader::with_capacity(DEFAULT_BUF_SIZE, reader.compat());
    let mut zip = async_zip::base::read::stream::ZipFileReader::new(&mut reader);

    let mut directories = FxHashSet::default();
    let mut local_headers = FxHashMap::default();
    let mut files = Vec::new();
    let mut offset = 0;

    while let Some(mut entry) = zip.next_with_entry().await? {
        let zip_entry = entry.reader().entry();

        // Check for unexpected compression methods.
        // A future version of uv will reject instead of warning about these.
        let compression = CompressionMethod::from(zip_entry.compression());
        if !compression.is_well_known() {
            warn_user_once!(
                "One or more file entries in '{source_hint}' use the '{compression}' compression method, which is not widely supported. A future version of uv will reject ZIP archives containing entries compressed with this method. Entries must be compressed with the '{stored}', '{deflate}', or '{zstd}' compression methods.",
                stored = CompressionMethod::Stored,
                deflate = CompressionMethod::Deflated,
                zstd = CompressionMethod::Zstd,
            );
        }

        // Construct the (expected) path to the file on-disk.
        let path = match zip_entry.filename().as_str() {
            Ok(path) => path,
            Err(ZipError::StringNotUtf8) => return Err(Error::LocalHeaderNotUtf8 { offset }),
            Err(err) => return Err(err.into()),
        };

        // Apply sanity checks to the file names in local headers.
        if let Err(e) = validate_archive_member_name(path) {
            if !skip_validation {
                return Err(e);
            }
        }

        // Sanitize the file name to prevent directory traversal attacks.
        let Some(relpath) = enclosed_name(path) else {
            warn!("Skipping unsafe file name: {path}");

            // Close current file prior to proceeding, as per:
            // https://docs.rs/async_zip/0.0.16/async_zip/base/read/stream/
            (.., zip) = entry.skip().await?;

            // Store the current offset.
            offset = zip.offset();

            continue;
        };

        let file_offset = zip_entry.file_offset();
        let expected_compressed_size = zip_entry.compressed_size();
        let expected_uncompressed_size = zip_entry.uncompressed_size();
        let expected_data_descriptor = zip_entry.data_descriptor();

        // Either create the directory or write the file to disk.
        let path = target.join(&relpath);
        let is_dir = zip_entry.dir()?;
        let computed = if is_dir {
            if directories.insert(path.clone()) {
                fs_err::tokio::create_dir_all(path)
                    .await
                    .map_err(Error::Io)?;
            }

            // If this is a directory, we expect the CRC32 to be 0.
            if zip_entry.crc32() != 0 {
                if !skip_validation {
                    return Err(Error::BadCrc32 {
                        path: relpath.clone(),
                        computed: 0,
                        expected: zip_entry.crc32(),
                    });
                }
            }

            // If this is a directory, we expect the uncompressed size to be 0.
            if zip_entry.uncompressed_size() != 0 {
                if !skip_validation {
                    return Err(Error::BadUncompressedSize {
                        path: relpath.clone(),
                        computed: 0,
                        expected: zip_entry.uncompressed_size(),
                    });
                }
            }

            ComputedEntry {
                crc32: 0,
                uncompressed_size: 0,
                compressed_size: 0,
            }
        } else {
            if let Some(parent) = path.parent() {
                if directories.insert(parent.to_path_buf()) {
                    fs_err::tokio::create_dir_all(parent)
                        .await
                        .map_err(Error::Io)?;
                }
            }

            // We don't know the file permissions here, because we haven't seen the central directory yet.
            let (actual_uncompressed_size, reader) = match fs_err::tokio::File::create_new(&path)
                .await
            {
                Ok(file) => {
                    // Write the file to disk.
                    let size = zip_entry.uncompressed_size();
                    let mut writer = if let Ok(size) = usize::try_from(size) {
                        tokio::io::BufWriter::with_capacity(std::cmp::min(size, 1024 * 1024), file)
                    } else {
                        tokio::io::BufWriter::new(file)
                    };
                    let mut reader = entry.reader_mut().compat();
                    let bytes_read = tokio::io::copy(&mut reader, &mut writer)
                        .await
                        .map_err(Error::io_or_zip)?;
                    let reader = reader.into_inner();

                    (bytes_read, reader)
                }
                Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                    debug!(
                        "Found duplicate local file header for: {}",
                        relpath.display()
                    );

                    // Read the existing file into memory.
                    let existing_contents = fs_err::tokio::read(&path).await.map_err(Error::Io)?;

                    // Read the entry into memory.
                    let mut expected_contents = Vec::with_capacity(existing_contents.len());
                    let entry_reader = entry.reader_mut();
                    let bytes_read = entry_reader
                        .read_to_end(&mut expected_contents)
                        .await
                        .map_err(Error::io_or_zip)?;

                    // Verify that the existing file contents match the expected contents.
                    if existing_contents != expected_contents {
                        if !skip_validation {
                            return Err(Error::DuplicateLocalFileHeader {
                                path: relpath.clone(),
                            });
                        }
                    }

                    (bytes_read as u64, entry_reader)
                }
                Err(err) => return Err(Error::Io(err)),
            };

            // Validate the uncompressed size.
            if actual_uncompressed_size != expected_uncompressed_size {
                if !(expected_compressed_size == 0 && expected_data_descriptor) {
                    if !skip_validation {
                        return Err(Error::BadUncompressedSize {
                            path: relpath.clone(),
                            computed: actual_uncompressed_size,
                            expected: expected_uncompressed_size,
                        });
                    }
                }
            }

            // Validate the compressed size.
            let actual_compressed_size = reader.bytes_read();
            if actual_compressed_size != expected_compressed_size {
                if !(expected_compressed_size == 0 && expected_data_descriptor) {
                    if !skip_validation {
                        return Err(Error::BadCompressedSize {
                            path: relpath.clone(),
                            computed: actual_compressed_size,
                            expected: expected_compressed_size,
                        });
                    }
                }
            }

            // Validate the CRC of any file we unpack
            // (It would be nice if async_zip made it harder to Not do this...)
            let actual_crc32 = reader.compute_hash();
            let expected_crc32 = reader.entry().crc32();
            if actual_crc32 != expected_crc32 {
                if !(expected_crc32 == 0 && expected_data_descriptor) {
                    if !skip_validation {
                        return Err(Error::BadCrc32 {
                            path: relpath.clone(),
                            computed: actual_crc32,
                            expected: expected_crc32,
                        });
                    }
                }
            }

            // Collect file paths (excluding directories).
            files.push((relpath.clone(), actual_uncompressed_size));

            ComputedEntry {
                crc32: actual_crc32,
                uncompressed_size: actual_uncompressed_size,
                compressed_size: actual_compressed_size,
            }
        };

        // Close current file prior to proceeding, as per:
        // https://docs.rs/async_zip/0.0.16/async_zip/base/read/stream/
        let (descriptor, next) = entry.skip().await?;

        // Verify that the data descriptor field is consistent with the presence (or absence) of a
        // data descriptor in the local file header.
        if expected_data_descriptor && descriptor.is_none() {
            if !skip_validation {
                return Err(Error::MissingDataDescriptor {
                    path: relpath.clone(),
                });
            }
        }
        if !expected_data_descriptor && descriptor.is_some() {
            if !skip_validation {
                return Err(Error::UnexpectedDataDescriptor {
                    path: relpath.clone(),
                });
            }
        }

        // If we have a data descriptor, validate it.
        if let Some(descriptor) = descriptor {
            if descriptor.crc != computed.crc32 {
                if !skip_validation {
                    return Err(Error::BadCrc32 {
                        path: relpath.clone(),
                        computed: computed.crc32,
                        expected: descriptor.crc,
                    });
                }
            }
            if descriptor.uncompressed_size != computed.uncompressed_size {
                if !skip_validation {
                    return Err(Error::BadUncompressedSize {
                        path: relpath.clone(),
                        computed: computed.uncompressed_size,
                        expected: descriptor.uncompressed_size,
                    });
                }
            }
            if descriptor.compressed_size != computed.compressed_size {
                if !skip_validation {
                    return Err(Error::BadCompressedSize {
                        path: relpath.clone(),
                        computed: computed.compressed_size,
                        expected: descriptor.compressed_size,
                    });
                }
            }
        }

        // Store the offset, for validation, and error if we see a duplicate file.
        match local_headers.entry(file_offset) {
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(LocalHeaderEntry {
                    relpath,
                    crc32: computed.crc32,
                    uncompressed_size: computed.uncompressed_size,
                    compressed_size: expected_compressed_size,
                    data_descriptor: expected_data_descriptor,
                });
            }
            std::collections::hash_map::Entry::Occupied(..) => {
                if !skip_validation {
                    return Err(Error::DuplicateLocalFileHeader {
                        path: relpath.clone(),
                    });
                }
            }
        }

        // Advance the reader to the next entry.
        zip = next;

        // Store the current offset.
        offset = zip.offset();
    }

    // Record the actual number of entries in the central directory.
    let mut num_entries = 0;

    // Track the file modes on Unix, to ensure that they're consistent across duplicates.
    #[cfg(unix)]
    let mut modes =
        FxHashMap::with_capacity_and_hasher(local_headers.len(), rustc_hash::FxBuildHasher);

    let mut directory = async_zip::base::read::cd::CentralDirectoryReader::new(&mut reader, offset);
    loop {
        match directory.next().await? {
            Entry::CentralDirectoryEntry(entry) => {
                // Count the number of entries in the central directory.
                num_entries += 1;

                // Construct the (expected) path to the file on-disk.
                let path = match entry.filename().as_str() {
                    Ok(path) => path,
                    Err(ZipError::StringNotUtf8) => {
                        return Err(Error::CentralDirectoryEntryNotUtf8 {
                            index: num_entries - 1,
                        });
                    }
                    Err(err) => return Err(err.into()),
                };

                // Apply sanity checks to the file names in CD headers.
                if let Err(e) = validate_archive_member_name(path) {
                    if !skip_validation {
                        return Err(e);
                    }
                }

                // Sanitize the file name to prevent directory traversal attacks.
                let Some(relpath) = enclosed_name(path) else {
                    continue;
                };

                // Validate that various fields are consistent between the local file header and the
                // central directory entry.
                match local_headers.remove(&entry.file_offset()) {
                    Some(local_header) => {
                        if local_header.relpath != relpath {
                            if !skip_validation {
                                return Err(Error::ConflictingPaths {
                                    offset: entry.file_offset(),
                                    local_path: local_header.relpath.clone(),
                                    central_directory_path: relpath.clone(),
                                });
                            }
                        }
                        if local_header.crc32 != entry.crc32() {
                            if !skip_validation {
                                return Err(Error::ConflictingChecksums {
                                    path: relpath.clone(),
                                    offset: entry.file_offset(),
                                    local_crc32: local_header.crc32,
                                    central_directory_crc32: entry.crc32(),
                                });
                            }
                        }
                        if local_header.uncompressed_size != entry.uncompressed_size() {
                            if !skip_validation {
                                return Err(Error::ConflictingUncompressedSizes {
                                    path: relpath.clone(),
                                    offset: entry.file_offset(),
                                    local_uncompressed_size: local_header.uncompressed_size,
                                    central_directory_uncompressed_size: entry.uncompressed_size(),
                                });
                            }
                        }
                        if local_header.compressed_size != entry.compressed_size() {
                            if !local_header.data_descriptor {
                                if !skip_validation {
                                    return Err(Error::ConflictingCompressedSizes {
                                        path: relpath.clone(),
                                        offset: entry.file_offset(),
                                        local_compressed_size: local_header.compressed_size,
                                        central_directory_compressed_size: entry.compressed_size(),
                                    });
                                }
                            }
                        }
                    }
                    None => {
                        if !skip_validation {
                            return Err(Error::MissingLocalFileHeader {
                                path: relpath.clone(),
                                offset: entry.file_offset(),
                            });
                        }
                    }
                }

                // On Unix, we need to set file permissions, which are stored in the central directory, at the
                // end of the archive. The `ZipFileReader` reads until it sees a central directory signature,
                // which indicates the first entry in the central directory. So we continue reading from there.
                #[cfg(unix)]
                {
                    use std::fs::Permissions;
                    use std::os::unix::fs::PermissionsExt;

                    if entry.dir()? {
                        continue;
                    }

                    let Some(mode) = entry.unix_permissions() else {
                        continue;
                    };

                    // If the file is included multiple times, ensure that the mode is consistent.
                    match modes.entry(relpath.clone()) {
                        std::collections::hash_map::Entry::Vacant(entry) => {
                            entry.insert(mode);
                        }
                        std::collections::hash_map::Entry::Occupied(entry) => {
                            if mode != *entry.get() {
                                if !skip_validation {
                                    return Err(Error::DuplicateExecutableFileHeader {
                                        path: relpath.clone(),
                                    });
                                }
                            }
                        }
                    }

                    // The executable bit is the only permission we preserve, otherwise we use the OS defaults.
                    // https://github.com/pypa/pip/blob/3898741e29b7279e7bffe044ecfbe20f6a438b1e/src/pip/_internal/utils/unpacking.py#L88-L100
                    let has_any_executable_bit = mode & 0o111;
                    if has_any_executable_bit != 0 {
                        let path = target.join(relpath);
                        let permissions = fs_err::tokio::metadata(&path)
                            .await
                            .map_err(Error::Io)?
                            .permissions();
                        if permissions.mode() & 0o111 != 0o111 {
                            fs_err::tokio::set_permissions(
                                &path,
                                Permissions::from_mode(permissions.mode() | 0o111),
                            )
                            .await
                            .map_err(Error::Io)?;
                        }
                    }
                }
            }
            Entry::EndOfCentralDirectoryRecord {
                record,
                comment,
                extensible,
            } => {
                // Reject ZIP64 end-of-central-directory records with extensible data, as the safety
                // tradeoffs don't outweigh the usefulness. We don't ever expect to encounter wheels
                // that leverage this feature anyway.
                if extensible {
                    if !skip_validation {
                        return Err(Error::ExtensibleData);
                    }
                }

                // Sanitize the comment by rejecting bytes `01` to `08`. If the comment contains an
                // embedded ZIP file, it _must_ contain one of these bytes, which are otherwise
                // very rare (non-printing) characters.
                if comment.as_bytes().iter().any(|&b| (1..=8).contains(&b)) {
                    if !skip_validation {
                        return Err(Error::ZipInZip);
                    }
                }

                // Validate that the reported number of entries match what we experienced while
                // reading the local file headers.
                if record.num_entries() != num_entries {
                    if !skip_validation {
                        return Err(Error::ConflictingNumberOfEntries {
                            expected: num_entries,
                            actual: record.num_entries(),
                        });
                    }
                }

                break;
            }
        }
    }

    // If we didn't see the file in the central directory, it means it was not present in the
    // archive.
    if !skip_validation {
        if let Some((key, value)) = local_headers.iter().next() {
            return Err(Error::MissingCentralDirectoryEntry {
                offset: *key,
                path: value.relpath.clone(),
            });
        }
    }

    // Determine whether the reader is exhausted, but allow trailing null bytes, which some zip
    // implementations incorrectly include.
    if !skip_validation {
        let mut has_trailing_bytes = false;
        let mut buf = [0u8; 256];
        loop {
            let n = reader.read(&mut buf).await.map_err(Error::Io)?;
            if n == 0 {
                if has_trailing_bytes {
                    warn!("Ignoring trailing null bytes in ZIP archive");
                }
                break;
            }
            for &b in &buf[..n] {
                if b == 0 {
                    has_trailing_bytes = true;
                } else {
                    return Err(Error::TrailingContents);
                }
            }
        }
    }

    Ok(files)
}

/// Unpack the given tar archive into the destination directory.
///
/// Returns the list of unpacked files and their sizes.
async fn untar_in<R: tokio::io::AsyncRead + Unpin>(
    reader: R,
    dst: &Path,
) -> Result<Vec<(PathBuf, u64)>, ExtractError<DecodeError>> {
    // Note: we intentionally allow `VENDOR.name` pax records here,
    // and we also allow them to contain non-UTF-8. The latter is technically
    // a violation of the pax spec, but is prevalent in the wild thanks
    // to both GNU tar and libarchive encoding `SCHILY.xattr` records
    // as raw binary.
    let decode_policy = DecodePolicy::default().pax_policy(
        PaxDecodePolicy::default()
            .allow_unknown_pax_vendor_records(true)
            .allow_non_utf8_pax_vendor_values(true),
    );
    let archive = TarArchive::new_with_policy(reader, decode_policy);

    let mut files = Vec::new();
    RecordingArchive::new(archive, &mut files)
        .extract_in(dst, tar_extract_policy())
        .await?;
    Ok(files)
}

/// An archive adapter that records file metadata as members are extracted.
///
/// Keeping this observation inside the lending archive cursor avoids a second filesystem walk and
/// preserves the paths and declared sizes from the archive itself.
struct RecordingArchive<'files, A> {
    archive: A,
    files: &'files mut Vec<(PathBuf, u64)>,
}

impl<'files, A> RecordingArchive<'files, A> {
    fn new(archive: A, files: &'files mut Vec<(PathBuf, u64)>) -> Self {
        Self { archive, files }
    }
}

impl<A: Archive> Archive for RecordingArchive<'_, A> {
    type Error = A::Error;
    type Payload<'archive>
        = A::Payload<'archive>
    where
        Self: 'archive;

    async fn next_member(&mut self) -> Result<Option<Member<Self::Payload<'_>>>, Self::Error> {
        let Self { archive, files } = self;
        let member = archive.next_member().await?;
        #[cfg(windows)]
        if let Some(Member::SymbolicLink { metadata, .. }) = &member {
            warn!("Skipping symlink in tar archive: {}", metadata.path);
        }
        if let Some(Member::File { metadata, size, .. }) = &member {
            files.push((PathBuf::from(&metadata.path), *size));
        }
        Ok(member)
    }
}

fn tar_extract_policy() -> ExtractPolicy {
    // Keep tar-codec's defaults, including name validation, hardlink rejection, and rejection of
    // pre-existing link targets. uv extracts archives into new temporary directories.
    if cfg!(windows) {
        ExtractPolicy::default()
            .link_policy(LinkPolicy::default().symlink_policy(SymlinkPolicy::Skip))
    } else {
        ExtractPolicy::default()
    }
}

/// Unpack a `.tar.gz` archive into the target directory, without requiring `Seek`.
///
/// This is useful for unpacking files as they're being downloaded.
///
/// Returns the list of unpacked files and their sizes.
async fn untar_gz<R: tokio::io::AsyncRead + Unpin>(
    reader: R,
    target: impl AsRef<Path>,
) -> Result<Vec<(PathBuf, u64)>, Error> {
    let reader = tokio::io::BufReader::with_capacity(DEFAULT_BUF_SIZE, reader);
    let decompressed_bytes = async_compression::tokio::bufread::GzipDecoder::new(reader);
    untar_in(decompressed_bytes, target.as_ref())
        .await
        .map_err(Error::from)
}

/// Unpack a `.tar.bz2` archive into the target directory, without requiring `Seek`.
///
/// This is useful for unpacking files as they're being downloaded.
///
/// Returns the list of unpacked files and their sizes.
async fn untar_bz2<R: tokio::io::AsyncRead + Unpin>(
    reader: R,
    target: impl AsRef<Path>,
) -> Result<Vec<(PathBuf, u64)>, Error> {
    let reader = tokio::io::BufReader::with_capacity(DEFAULT_BUF_SIZE, reader);
    let decompressed_bytes = async_compression::tokio::bufread::BzDecoder::new(reader);
    untar_in(decompressed_bytes, target.as_ref())
        .await
        .map_err(Error::from)
}

/// Unpack a `.tar.zst` archive into the target directory, without requiring `Seek`.
///
/// This is useful for unpacking files as they're being downloaded.
///
/// Returns the list of unpacked files and their sizes.
pub async fn untar_zst<R: tokio::io::AsyncRead + Unpin>(
    reader: R,
    target: impl AsRef<Path>,
) -> Result<Vec<(PathBuf, u64)>, Error> {
    let reader = tokio::io::BufReader::with_capacity(DEFAULT_BUF_SIZE, reader);
    let decompressed_bytes = async_compression::tokio::bufread::ZstdDecoder::new(reader);
    untar_in(decompressed_bytes, target.as_ref())
        .await
        .map_err(Error::from)
}

/// Unpack a `.tar.xz` archive into the target directory, without requiring `Seek`.
///
/// This is useful for unpacking files as they're being downloaded.
///
/// Returns the list of unpacked files and their sizes.
async fn untar_xz<R: tokio::io::AsyncRead + Unpin>(
    reader: R,
    target: impl AsRef<Path>,
) -> Result<Vec<(PathBuf, u64)>, Error> {
    let reader = tokio::io::BufReader::with_capacity(DEFAULT_BUF_SIZE, reader);
    let decompressed_bytes = async_compression::tokio::bufread::XzDecoder::new(reader);
    untar_in(decompressed_bytes, target.as_ref())
        .await
        .map_err(Error::from)
}

/// Unpack a `.tar` archive into the target directory, without requiring `Seek`.
///
/// This is useful for unpacking files as they're being downloaded.
///
/// Returns the list of unpacked files and their sizes.
async fn untar<R: tokio::io::AsyncRead + Unpin>(
    reader: R,
    target: impl AsRef<Path>,
) -> Result<Vec<(PathBuf, u64)>, Error> {
    let reader = tokio::io::BufReader::with_capacity(DEFAULT_BUF_SIZE, reader);
    untar_in(reader, target.as_ref()).await.map_err(Error::from)
}

/// Unpack a `.zip`, `.tar.gz`, `.tar.bz2`, `.tar.zst`, or `.tar.xz` archive into the target directory,
/// without requiring `Seek`.
///
/// `source_hint` is used for warning messages, to identify the source of the archive
/// beneath the reader. It might be a URL, a file path, or something else.
///
/// Returns the list of unpacked files and their sizes.
pub async fn archive<D: Display, R: tokio::io::AsyncRead + Unpin>(
    source_hint: D,
    reader: R,
    ext: SourceDistExtension,
    target: impl AsRef<Path>,
) -> Result<Vec<(PathBuf, u64)>, Error> {
    match ext {
        SourceDistExtension::Zip => unzip(source_hint, reader, target).await,
        SourceDistExtension::Tar => untar(reader, target).await,
        SourceDistExtension::Tgz | SourceDistExtension::TarGz => untar_gz(reader, target).await,
        SourceDistExtension::Tbz | SourceDistExtension::TarBz2 => untar_bz2(reader, target).await,
        SourceDistExtension::Txz
        | SourceDistExtension::TarXz
        | SourceDistExtension::Tlz
        | SourceDistExtension::TarLz
        | SourceDistExtension::TarLzma => untar_xz(reader, target).await,
        SourceDistExtension::TarZst => untar_zst(reader, target).await,
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;
    use std::io;
    use std::path::PathBuf;
    use std::pin::Pin;
    use std::task::{Context, Poll};

    use insta::assert_snapshot;
    use tar_codec::{DecodeError, ExtractError, ExtractPolicyViolation};
    use tokio::io::{AsyncRead, ReadBuf};

    use super::{untar, untar_in};

    const BLOCK_SIZE: usize = 512;

    #[tokio::test]
    async fn untar_records_files_and_preserves_executable_intent() {
        let mut archive = Vec::new();
        append_entry(
            &mut archive,
            "pax",
            b'x',
            &pax_record("path", "pkg/tool"),
            "",
            0o644,
        );
        append_entry(&mut archive, "ignored", b'0', b"run", "", 0o755);
        finish_archive(&mut archive);
        let temp = tempfile::tempdir().expect("temporary directory should be created");

        let files = untar_in(archive.as_slice(), temp.path())
            .await
            .expect("tar archive should extract");

        assert_eq!(files, [(PathBuf::from("pkg/tool"), 3)]);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;

            assert_ne!(
                fs_err::metadata(temp.path().join("pkg/tool"))
                    .expect("tool metadata should be readable")
                    .permissions()
                    .mode()
                    & 0o111,
                0
            );
        }
    }

    #[tokio::test]
    async fn untar_rejects_hardlinks() {
        let mut archive = Vec::new();
        append_entry(&mut archive, "pkg/tool", b'0', b"run", "", 0o644);
        append_entry(&mut archive, "pkg/tool-copy", b'1', b"", "pkg/tool", 0o644);
        finish_archive(&mut archive);
        let temp = tempfile::tempdir().expect("temporary directory should be created");

        let error = untar_in(archive.as_slice(), temp.path())
            .await
            .expect_err("hardlink should be rejected");

        assert!(matches!(
            error,
            ExtractError::PolicyViolation {
                violation: ExtractPolicyViolation::HardLink,
                ..
            }
        ));
        assert!(!temp.path().join("pkg/tool-copy").exists());
    }

    #[tokio::test]
    async fn untar_uses_default_name_validation() {
        let mut archive = Vec::new();
        append_entry(
            &mut archive,
            "pkg/name:stream",
            b'0',
            b"contents",
            "",
            0o644,
        );
        finish_archive(&mut archive);
        let temp = tempfile::tempdir().expect("temporary directory should be created");

        let error = untar_in(archive.as_slice(), temp.path())
            .await
            .expect_err("default name validation should reject colons");

        assert!(matches!(
            error,
            ExtractError::PolicyViolation {
                violation: ExtractPolicyViolation::NameRejected {
                    context: "member path",
                    value,
                },
                ..
            } if value == "pkg/name:stream"
        ));
    }

    #[tokio::test]
    async fn untar_reads_gnu_archives() {
        let mut archive = Vec::new();
        append_entry_with_format(
            &mut archive,
            TestFormat::Gnu,
            "pkg/file",
            b'0',
            b"contents",
            "",
            0o644,
        );
        finish_archive(&mut archive);
        let temp = tempfile::tempdir().expect("temporary directory should be created");

        let files = untar_in(archive.as_slice(), temp.path())
            .await
            .expect("GNU archive should extract");

        assert_eq!(files, [(PathBuf::from("pkg/file"), 8)]);
        assert_eq!(
            fs_err::read(temp.path().join("pkg/file")).expect("file should be readable"),
            b"contents"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn untar_preserves_archive_owned_and_missing_symlink_targets() {
        let mut archive = Vec::new();
        append_entry(
            &mut archive,
            "pkg/created-link",
            b'2',
            b"",
            "created",
            0o777,
        );
        append_entry(&mut archive, "pkg/created", b'0', b"created", "", 0o644);
        append_entry(
            &mut archive,
            "pkg/missing-link",
            b'2',
            b"",
            "missing",
            0o777,
        );
        finish_archive(&mut archive);
        let temp = tempfile::tempdir().expect("temporary directory should be created");

        let files = untar_in(archive.as_slice(), temp.path())
            .await
            .expect("safe symlinks should extract");

        assert_eq!(files, [(PathBuf::from("pkg/created"), 7)]);
        for (link, target) in [("created-link", "created"), ("missing-link", "missing")] {
            assert_eq!(
                fs_err::read_link(temp.path().join("pkg").join(link))
                    .expect("symlink should be readable"),
                PathBuf::from(target)
            );
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn untar_rejects_ambient_symlink_targets() {
        let mut archive = Vec::new();
        append_entry(
            &mut archive,
            "pkg/ambient-link",
            b'2',
            b"",
            "ambient",
            0o777,
        );
        finish_archive(&mut archive);
        let temp = tempfile::tempdir().expect("temporary directory should be created");
        fs_err::create_dir_all(temp.path().join("pkg"))
            .expect("ambient directory should be created");
        fs_err::write(temp.path().join("pkg/ambient"), b"ambient")
            .expect("ambient target should be created");

        let error = untar_in(archive.as_slice(), temp.path())
            .await
            .expect_err("ambient symlink target should be rejected");

        assert!(matches!(
            error,
            ExtractError::InvalidLink {
                path,
                target,
                reason: "ambient target is not allowed",
                ..
            } if path == *"pkg/ambient-link" && target == "ambient"
        ));
        assert!(!temp.path().join("pkg/ambient-link").exists());
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn untar_skips_symlinks_on_windows() {
        let mut archive = Vec::new();
        append_entry(&mut archive, "target", b'0', b"contents", "", 0o644);
        append_entry(&mut archive, "link", b'2', b"", "target", 0o777);
        finish_archive(&mut archive);
        let temp = tempfile::tempdir().expect("temporary directory should be created");

        untar_in(archive.as_slice(), temp.path())
            .await
            .expect("archive should extract while skipping its symlink");

        assert_eq!(
            fs_err::read(temp.path().join("target")).expect("target should be readable"),
            b"contents"
        );
        assert!(matches!(
            fs_err::symlink_metadata(temp.path().join("link")),
            Err(error) if error.kind() == io::ErrorKind::NotFound
        ));
    }

    #[tokio::test]
    async fn untar_rejects_malformed_archives() {
        let temp = tempfile::tempdir().expect("temporary directory should be created");
        let error = untar_in(b"not a tar archive".as_slice(), temp.path())
            .await
            .expect_err("malformed archive should be rejected");

        assert!(matches!(
            error,
            ExtractError::Archive(DecodeError::Framing(_))
        ));
    }

    #[tokio::test]
    async fn untar_rejects_member_path_traversal() {
        let mut archive = Vec::new();
        append_entry(&mut archive, "../escape", b'0', b"escape", "", 0o644);
        finish_archive(&mut archive);
        let temp = tempfile::tempdir().expect("temporary directory should be created");
        let destination = temp.path().join("out");

        let error = untar_in(archive.as_slice(), &destination)
            .await
            .expect_err("parent-directory traversal should be rejected");

        assert!(matches!(
            error,
            ExtractError::UnsafePath {
                context: "member path",
                value,
                reason: "contains a parent-directory component",
                ..
            } if value == "../escape"
        ));
        assert!(!temp.path().join("escape").exists());
    }

    #[tokio::test]
    async fn strict_decoder_rejects_excluded_tar_framing() {
        let mut v7 = Vec::new();
        append_entry_with_format(&mut v7, TestFormat::V7, "file", b'0', b"", "", 0o644);
        finish_archive(&mut v7);

        let mut mixed = Vec::new();
        append_entry_with_format(&mut mixed, TestFormat::Pax, "pax", b'0', b"", "", 0o644);
        append_entry_with_format(&mut mixed, TestFormat::Gnu, "gnu", b'0', b"", "", 0o644);
        finish_archive(&mut mixed);

        let mut sparse = Vec::new();
        append_entry_with_format(&mut sparse, TestFormat::Gnu, "sparse", b'S', b"", "", 0o644);
        finish_archive(&mut sparse);

        let mut malformed_termination = Vec::new();
        append_entry(&mut malformed_termination, "file", b'0', b"", "", 0o644);
        malformed_termination.resize(malformed_termination.len() + BLOCK_SIZE, 0);

        let mut failures = String::new();
        for (case, archive) in [
            ("v7", v7),
            ("mixed GNU/pax", mixed),
            ("sparse", sparse),
            ("malformed termination", malformed_termination),
        ] {
            let temp = tempfile::tempdir().expect("temporary directory should be created");
            let error = untar_in(archive.as_slice(), temp.path())
                .await
                .expect_err(case);
            assert!(
                matches!(&error, ExtractError::Archive(DecodeError::Framing(_))),
                "unexpected error for {case}: {error:?}"
            );
            writeln!(failures, "{case}: {error}").expect("writing to a string should succeed");
        }
        assert_snapshot!(failures, @r"
        v7: at byte 0: invalid tar identity: found [0, 0, 0, 0, 0, 0, 0, 0]
        mixed GNU/pax: at byte 512: archive format changed from Pax to Gnu
        sparse: at byte 0: unsupported tar typeflag 83
        malformed termination: at byte 1024: missing two-block end-of-archive marker
        ");
    }

    #[tokio::test]
    async fn strict_decoder_rejects_non_utf8_member_names() {
        let mut header = test_header(TestFormat::Pax, "file", b'0', 0, "", 0o644);
        header[0] = 0xff;
        set_checksum(&mut header);
        let mut archive = header.to_vec();
        finish_archive(&mut archive);
        let temp = tempfile::tempdir().expect("temporary directory should be created");

        let error = untar_in(archive.as_slice(), temp.path())
            .await
            .expect_err("non-UTF-8 member path should be rejected");

        assert!(matches!(
            &error,
            ExtractError::Archive(DecodeError::InvalidUtf8 { field: "path", .. })
        ));
        assert_snapshot!(error, @"at byte 0: path is not valid UTF-8");
    }

    #[tokio::test]
    async fn tar_http_errors_are_detected_through_the_source_chain() {
        let reqwest_error = reqwest::Client::new()
            .get("://")
            .build()
            .expect_err("invalid URL should be rejected");
        let reader = FailingReader(Some(io::Error::other(reqwest_error)));
        let temp = tempfile::tempdir().expect("temporary directory should be created");

        let error = untar(reader, temp.path())
            .await
            .expect_err("reader error should fail extraction");

        assert!(matches!(error, crate::Error::Tar(_)));
        assert!(error.is_http_streaming_failed());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn untar_rejects_external_symlink_targets() {
        let mut archive = Vec::new();
        append_entry(&mut archive, "python", b'2', b"", "/bin/python", 0o777);
        finish_archive(&mut archive);
        let temp = tempfile::tempdir().expect("temporary directory should be created");

        let error = untar_in(archive.as_slice(), temp.path())
            .await
            .expect_err("external symlink target should be rejected");

        assert!(matches!(
            error,
            ExtractError::UnsafePath {
                context: "symbolic-link target",
                value,
                reason: "is absolute",
                ..
            } if value == "/bin/python"
        ));
        assert!(!temp.path().join("python").exists());
    }

    fn append_entry(
        archive: &mut Vec<u8>,
        path: &str,
        typeflag: u8,
        payload: &[u8],
        link_name: &str,
        mode: u32,
    ) {
        append_entry_with_format(
            archive,
            TestFormat::Pax,
            path,
            typeflag,
            payload,
            link_name,
            mode,
        );
    }

    fn append_entry_with_format(
        archive: &mut Vec<u8>,
        format: TestFormat,
        path: &str,
        typeflag: u8,
        payload: &[u8],
        link_name: &str,
        mode: u32,
    ) {
        let header = test_header(
            format,
            path,
            typeflag,
            payload.len() as u64,
            link_name,
            mode,
        );
        archive.extend_from_slice(&header);
        archive.extend_from_slice(payload);
        archive.resize(archive.len().next_multiple_of(BLOCK_SIZE), 0);
    }

    fn test_header(
        format: TestFormat,
        path: &str,
        typeflag: u8,
        size: u64,
        link_name: &str,
        mode: u32,
    ) -> [u8; BLOCK_SIZE] {
        let mut header = [0; BLOCK_SIZE];
        set_text(&mut header[0..100], path);
        header[100..108].copy_from_slice(format!("{mode:07o}\0").as_bytes());
        header[108..116].copy_from_slice(b"0000000\0");
        header[116..124].copy_from_slice(b"0000000\0");
        header[124..136].copy_from_slice(format!("{size:011o}\0").as_bytes());
        header[136..148].copy_from_slice(b"00000000000\0");
        header[156] = typeflag;
        set_text(&mut header[157..257], link_name);
        match format {
            TestFormat::Pax => header[257..265].copy_from_slice(b"ustar\x0000"),
            TestFormat::Gnu => header[257..265].copy_from_slice(b"ustar  \0"),
            TestFormat::V7 => {}
        }
        set_checksum(&mut header);
        header
    }

    fn set_checksum(header: &mut [u8; BLOCK_SIZE]) {
        header[148..156].fill(b' ');
        let checksum: u64 = header.iter().map(|byte| u64::from(*byte)).sum();
        header[148..156].copy_from_slice(format!("{checksum:06o}\0 ").as_bytes());
    }

    fn finish_archive(archive: &mut Vec<u8>) {
        archive.resize(archive.len() + 2 * BLOCK_SIZE, 0);
    }

    fn set_text(field: &mut [u8], value: &str) {
        assert!(value.len() < field.len());
        field[..value.len()].copy_from_slice(value.as_bytes());
    }

    fn pax_record(keyword: &str, value: &str) -> Vec<u8> {
        let mut length = 0;
        loop {
            let record = format!("{length} {keyword}={value}\n");
            if record.len() == length {
                return record.into_bytes();
            }
            length = record.len();
        }
    }

    #[derive(Clone, Copy)]
    enum TestFormat {
        Pax,
        Gnu,
        V7,
    }

    struct FailingReader(Option<io::Error>);

    impl AsyncRead for FailingReader {
        fn poll_read(
            self: Pin<&mut Self>,
            _context: &mut Context<'_>,
            _buffer: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            let error = self
                .get_mut()
                .0
                .take()
                .unwrap_or_else(|| io::Error::other("reader was polled after failing"));
            Poll::Ready(Err(error))
        }
    }
}
