use std::fmt::Display;
use std::path::{Component, Path, PathBuf};
use std::pin::Pin;

use async_zip::base::read::cd::Entry;
use async_zip::error::ZipError;
use futures::{AsyncReadExt, StreamExt};
use rustc_hash::{FxHashMap, FxHashSet};
use tokio_util::compat::{FuturesAsyncReadCompatExt, TokioAsyncReadCompatExt};
use tracing::{debug, warn};

use uv_distribution_filename::SourceDistExtension;
use uv_warnings::warn_user_once;

use crate::{CompressionMethod, Error, insecure_no_validate, validate_archive_member_name};

const DEFAULT_BUF_SIZE: usize = 128 * 1024;

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
pub async fn unzip<D: Display, R: tokio::io::AsyncRead + Unpin>(
    source_hint: D,
    reader: R,
    target: impl AsRef<Path>,
) -> Result<(), Error> {
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

    // Determine whether ZIP validation is disabled.
    let skip_validation = insecure_no_validate();

    let target = target.as_ref();
    let mut reader = futures::io::BufReader::with_capacity(DEFAULT_BUF_SIZE, reader.compat());
    let mut zip = async_zip::base::read::stream::ZipFileReader::new(&mut reader);

    let mut directories = FxHashSet::default();
    let mut local_headers = FxHashMap::default();
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
                        .map_err(Error::io_or_compression)?;
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
                        .map_err(Error::io_or_compression)?;

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

    Ok(())
}

/// Unpack the given tar archive into the destination directory.
///
/// This is equivalent to `archive.unpack_in(dst)`, but it also preserves the executable bit.
async fn untar_in(
    mut archive: tokio_tar::Archive<&'_ mut (dyn tokio::io::AsyncRead + Unpin)>,
    dst: &Path,
) -> std::io::Result<()> {
    // Like `tokio-tar`, canonicalize the destination prior to unpacking.
    let dst = fs_err::tokio::canonicalize(dst).await?;

    // Memoize filesystem calls to canonicalize paths.
    let mut memo = FxHashSet::default();

    let mut entries = archive.entries()?;
    let mut pinned = Pin::new(&mut entries);
    while let Some(entry) = pinned.next().await {
        // Unpack the file into the destination directory.
        let mut file = entry?;

        // On Windows, skip symlink entries, as they're not supported. pip recursively copies the
        // symlink target instead.
        if cfg!(windows) && file.header().entry_type().is_symlink() {
            warn!(
                "Skipping symlink in tar archive: {}",
                file.path()?.display()
            );
            continue;
        }

        // Unpack the file into the destination directory.
        #[cfg_attr(not(unix), allow(unused_variables))]
        let unpacked_at = file.unpack_in_raw(&dst, &mut memo).await?;

        // Preserve the executable bit.
        #[cfg(unix)]
        {
            use std::fs::Permissions;
            use std::os::unix::fs::PermissionsExt;

            let entry_type = file.header().entry_type();
            if entry_type.is_file() || entry_type.is_hard_link() {
                let mode = file.header().mode()?;
                let has_any_executable_bit = mode & 0o111;
                if has_any_executable_bit != 0 {
                    if let Some(path) = unpacked_at.as_deref() {
                        let permissions = fs_err::tokio::metadata(&path).await?.permissions();
                        if permissions.mode() & 0o111 != 0o111 {
                            fs_err::tokio::set_permissions(
                                &path,
                                Permissions::from_mode(permissions.mode() | 0o111),
                            )
                            .await?;
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

/// Unpack a `.tar.gz` archive into the target directory, without requiring `Seek`.
///
/// This is useful for unpacking files as they're being downloaded.
pub async fn untar_gz<R: tokio::io::AsyncRead + Unpin>(
    reader: R,
    target: impl AsRef<Path>,
) -> Result<(), Error> {
    let reader = tokio::io::BufReader::with_capacity(DEFAULT_BUF_SIZE, reader);
    let mut decompressed_bytes = async_compression::tokio::bufread::GzipDecoder::new(reader);

    let archive = tokio_tar::ArchiveBuilder::new(
        &mut decompressed_bytes as &mut (dyn tokio::io::AsyncRead + Unpin),
    )
    .set_preserve_mtime(false)
    .set_preserve_permissions(false)
    .set_allow_external_symlinks(false)
    .build();
    untar_in(archive, target.as_ref())
        .await
        .map_err(Error::io_or_compression)
}

/// Unpack a `.tar.bz2` archive into the target directory, without requiring `Seek`.
///
/// This is useful for unpacking files as they're being downloaded.
pub async fn untar_bz2<R: tokio::io::AsyncRead + Unpin>(
    reader: R,
    target: impl AsRef<Path>,
) -> Result<(), Error> {
    let reader = tokio::io::BufReader::with_capacity(DEFAULT_BUF_SIZE, reader);
    let mut decompressed_bytes = async_compression::tokio::bufread::BzDecoder::new(reader);

    let archive = tokio_tar::ArchiveBuilder::new(
        &mut decompressed_bytes as &mut (dyn tokio::io::AsyncRead + Unpin),
    )
    .set_preserve_mtime(false)
    .set_preserve_permissions(false)
    .set_allow_external_symlinks(false)
    .build();
    untar_in(archive, target.as_ref())
        .await
        .map_err(Error::io_or_compression)
}

/// Unpack a `.tar.zst` archive into the target directory, without requiring `Seek`.
///
/// This is useful for unpacking files as they're being downloaded.
pub async fn untar_zst<R: tokio::io::AsyncRead + Unpin>(
    reader: R,
    target: impl AsRef<Path>,
) -> Result<(), Error> {
    let reader = tokio::io::BufReader::with_capacity(DEFAULT_BUF_SIZE, reader);
    let mut decompressed_bytes = async_compression::tokio::bufread::ZstdDecoder::new(reader);

    let archive = tokio_tar::ArchiveBuilder::new(
        &mut decompressed_bytes as &mut (dyn tokio::io::AsyncRead + Unpin),
    )
    .set_preserve_mtime(false)
    .set_preserve_permissions(false)
    .set_allow_external_symlinks(false)
    .build();
    untar_in(archive, target.as_ref())
        .await
        .map_err(Error::io_or_compression)
}

/// Unpack a `.tar.zst` archive from a file on disk into the target directory.
pub fn untar_zst_file<R: std::io::Read>(reader: R, target: impl AsRef<Path>) -> Result<(), Error> {
    let reader = std::io::BufReader::with_capacity(DEFAULT_BUF_SIZE, reader);
    let decompressed = zstd::Decoder::new(reader).map_err(Error::Io)?;
    let mut archive = tar::Archive::new(decompressed);
    archive.set_preserve_mtime(false);
    archive.unpack(target).map_err(Error::io_or_compression)?;
    Ok(())
}

/// Unpack a `.tar.xz` archive into the target directory, without requiring `Seek`.
///
/// This is useful for unpacking files as they're being downloaded.
pub async fn untar_xz<R: tokio::io::AsyncRead + Unpin>(
    reader: R,
    target: impl AsRef<Path>,
) -> Result<(), Error> {
    let reader = tokio::io::BufReader::with_capacity(DEFAULT_BUF_SIZE, reader);
    let mut decompressed_bytes = async_compression::tokio::bufread::XzDecoder::new(reader);

    let archive = tokio_tar::ArchiveBuilder::new(
        &mut decompressed_bytes as &mut (dyn tokio::io::AsyncRead + Unpin),
    )
    .set_preserve_mtime(false)
    .set_preserve_permissions(false)
    .set_allow_external_symlinks(false)
    .build();
    untar_in(archive, target.as_ref())
        .await
        .map_err(Error::io_or_compression)?;
    Ok(())
}

/// Unpack a `.tar` archive into the target directory, without requiring `Seek`.
///
/// This is useful for unpacking files as they're being downloaded.
pub async fn untar<R: tokio::io::AsyncRead + Unpin>(
    reader: R,
    target: impl AsRef<Path>,
) -> Result<(), Error> {
    let mut reader = tokio::io::BufReader::with_capacity(DEFAULT_BUF_SIZE, reader);

    let archive =
        tokio_tar::ArchiveBuilder::new(&mut reader as &mut (dyn tokio::io::AsyncRead + Unpin))
            .set_preserve_mtime(false)
            .set_preserve_permissions(false)
            .set_allow_external_symlinks(false)
            .build();
    untar_in(archive, target.as_ref())
        .await
        .map_err(Error::io_or_compression)?;
    Ok(())
}

/// Unpack a `.zip`, `.tar.gz`, `.tar.bz2`, `.tar.zst`, or `.tar.xz` archive into the target directory,
/// without requiring `Seek`.
///
/// `source_hint` is used for warning messages, to identify the source of the archive
/// beneath the reader. It might be a URL, a file path, or something else.
pub async fn archive<D: Display, R: tokio::io::AsyncRead + Unpin>(
    source_hint: D,
    reader: R,
    ext: SourceDistExtension,
    target: impl AsRef<Path>,
) -> Result<(), Error> {
    match ext {
        SourceDistExtension::Zip => {
            unzip(source_hint, reader, target).await?;
        }
        SourceDistExtension::Tar => {
            untar(reader, target).await?;
        }
        SourceDistExtension::Tgz | SourceDistExtension::TarGz => {
            untar_gz(reader, target).await?;
        }
        SourceDistExtension::Tbz | SourceDistExtension::TarBz2 => {
            untar_bz2(reader, target).await?;
        }
        SourceDistExtension::Txz
        | SourceDistExtension::TarXz
        | SourceDistExtension::Tlz
        | SourceDistExtension::TarLz
        | SourceDistExtension::TarLzma => {
            untar_xz(reader, target).await?;
        }
        SourceDistExtension::TarZst => {
            untar_zst(reader, target).await?;
        }
    }
    Ok(())
}
