use std::io;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use async_zip::base::read::cd::Entry;
use async_zip::error::ZipError;
use futures::executor::block_on;
use futures::io::AllowStdIo;
use futures::{AsyncReadExt, StreamExt};
use rustc_hash::{FxHashMap, FxHashSet};
use tar_codec::extract::{ExtractPolicy, LinkPolicy, SymlinkPolicy};
use tar_codec::{
    Archive, DecodeError, DecodePolicy, ExtractError, Member, PaxDecodePolicy,
    PaxVendorExtensionPolicy, TarArchive,
};
use tempfile::TempDir;
use tokio::io::{AsyncReadExt as TokioAsyncReadExt, AsyncWriteExt};
use tokio_util::compat::{
    FuturesAsyncReadCompatExt, FuturesAsyncWriteCompatExt, TokioAsyncReadCompatExt,
};
use tracing::{debug, warn};

use uv_distribution_filename::{LegacySourceDistExtension, SourceDistExtension};
use uv_preview::PreviewFeature;

use crate::archive_path::SanitizedArchivePath;
use crate::dirhash::{
    DirhashTree, HashedFile, UnhashedFile, UnzipOutput, blake3_copy_with_buffer,
    directory_tree_from_extracted,
};
use crate::{Error, insecure_no_validate};

/// Default buffer size for streaming archive reads and file copies.
const DEFAULT_BUF_SIZE: usize = 128 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
struct LocalHeaderEntry {
    /// The relative path of the entry, as computed from the local file header.
    relpath: SanitizedArchivePath,
    /// Whether the local file header identifies the entry as a directory.
    is_dir: bool,
    /// The computed CRC32 checksum of the entry.
    crc32: u32,
    /// The computed compressed size of the entry.
    compressed_size: u64,
    /// The computed uncompressed size of the entry.
    uncompressed_size: u64,
    /// Whether the entry has a data descriptor.
    data_descriptor: bool,
    /// The digest of the extracted file contents.
    digest: Option<blake3::Hash>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ComputedEntry {
    /// The computed CRC32 checksum of the entry.
    crc32: u32,
    /// The computed uncompressed size of the entry.
    uncompressed_size: u64,
    /// The computed compressed size of the entry.
    compressed_size: u64,
    /// The digest of the extracted file contents.
    digest: Option<blake3::Hash>,
}

/// Unpack a `.zip` archive into the target directory, without requiring `Seek`.
///
/// This is useful for unzipping files as they're being downloaded. If the archive
/// is already fully on disk, consider using [`crate::unzip`], which can use multiple
/// threads to work faster in that case.
///
/// Returns the temporary directory and the list of unpacked files and their sizes.
/// Filesystem operations run in a blocking task, fed through a bounded buffer. Dropping the future
/// closes the pipe; the worker owns cleanup, which can be interrupted by process shutdown.
///
/// Extraction can leave unread bytes when ZIP validation is disabled. Callers must drain the
/// reader before finalizing download hashes.
pub async fn unzip<R: tokio::io::AsyncRead + Unpin>(
    reader: R,
    target: TempDir,
) -> Result<(TempDir, Vec<UnhashedFile>), Error> {
    let (target, UnzipOutput::Unhashed(files)) =
        unzip_streaming_inner(reader, target, false).await?
    else {
        return Err(Error::Io(std::io::Error::other(
            "streaming ZIP hash tree was unexpectedly computed",
        )));
    };
    Ok((target, files))
}

/// Extract a streaming ZIP archive and compute a hash tree of the extracted files.
///
/// The tree includes regular-file paths, contents, and empty directories. ZIP entries are never
/// followed as symlinks; non-directory entries are materialized and hashed as regular files.
///
/// See [`unzip`] for details on buffering, cleanup, and finalizing download hashes.
pub async fn unzip_and_hash<R: tokio::io::AsyncRead + Unpin>(
    reader: R,
    target: TempDir,
) -> Result<(TempDir, Vec<HashedFile>, DirhashTree), Error> {
    let (target, UnzipOutput::Hashed { files, tree }) =
        unzip_streaming_inner(reader, target, true).await?
    else {
        return Err(Error::Io(std::io::Error::other(
            "streaming ZIP hash tree was not computed",
        )));
    };
    Ok((target, files, tree))
}

/// Feed a borrowed archive reader to an extraction worker that owns the temporary directory.
async fn unzip_streaming_inner<R: tokio::io::AsyncRead + Unpin>(
    mut reader: R,
    target: TempDir,
    hash_contents: bool,
) -> Result<(TempDir, UnzipOutput), Error> {
    const PIPE_BUFFER_SIZE: usize = 2 * DEFAULT_BUF_SIZE;

    // Allow the download to get ahead while the worker decompresses and writes files.
    let (sender, receiver) = tokio::io::duplex(PIPE_BUFFER_SIZE);
    let mut extraction = tokio::task::spawn_blocking(move || {
        let extracted = block_on(Box::pin(unzip_inner(
            receiver,
            target.path(),
            hash_contents,
        )))?;
        Ok::<_, Error>((target, extracted))
    });
    let download = async {
        // Own the write end so EOF, errors and cancellation all close the pipe.
        let mut sender = sender;
        let mut buffer = vec![0; DEFAULT_BUF_SIZE];
        loop {
            let read = reader.read(&mut buffer).await?;
            if read == 0 {
                break;
            }
            if let Err(err) = sender.write_all(&buffer[..read]).await {
                if err.kind() == io::ErrorKind::BrokenPipe {
                    // The worker either rejected the archive or finished early because ZIP
                    // validation is disabled. The caller drains the download in the latter case.
                    break;
                }
                return Err(err);
            }
        }
        Ok::<_, io::Error>(())
    };
    let extraction = tokio::select! {
        // Prefer a download error over the resulting truncated-ZIP error if both are ready.
        biased;
        download = download => {
            download.map_err(Error::Io)?;
            extraction.await
        }
        // Stop reading even if the server stalls after sending an invalid ZIP entry.
        extraction = &mut extraction => extraction,
    };
    extraction.map_err(|err| Error::Io(io::Error::other(err)))?
}

async fn unzip_inner<R: tokio::io::AsyncRead + Unpin>(
    reader: R,
    target: &Path,
    hash_contents: bool,
) -> Result<UnzipOutput, Error> {
    // Determine whether ZIP validation is disabled.
    let skip_validation = insecure_no_validate();

    let mut reader = futures::io::BufReader::with_capacity(DEFAULT_BUF_SIZE, reader.compat());
    let mut zip = async_zip::base::read::stream::ZipFileReader::new(&mut reader);

    let mut directories = FxHashSet::default();
    let mut local_headers = FxHashMap::default();
    let mut output_paths = FxHashSet::default();
    let mut files = Vec::new();
    let mut hashed_files = Vec::new();
    let mut digest_directories = FxHashSet::default();
    // Reuse the copy buffer across files, whether or not their contents are hashed.
    let mut copy_buffer = Vec::new();
    let mut offset = 0;

    while let Some(mut entry) = zip.next_with_entry().await? {
        let zip_entry = entry.reader().entry();

        // Construct the (expected) path to the file on-disk.
        let path = match zip_entry.filename().as_str() {
            Ok(path) => path,
            Err(ZipError::StringNotUtf8) => return Err(Error::LocalHeaderNotUtf8 { offset }),
            Err(err) => return Err(err.into()),
        };

        // Validate and sanitize the file name to prevent directory traversal attacks.
        let relpath = match SanitizedArchivePath::from_archive_member(path) {
            Ok(path) => path,
            Err(_) if skip_validation => None,
            Err(err) => return Err(err),
        };
        let Some(relpath) = relpath else {
            warn!("Skipping unsafe file name: {path}");

            // Close current file prior to proceeding, as per:
            // https://docs.rs/async_zip/0.0.16/async_zip/base/read/stream/
            (.., zip) = entry.skip().await?;

            // Store the current offset.
            offset = zip.offset();

            continue;
        };
        if hash_contents && !output_paths.insert(relpath.clone()) {
            return Err(Error::DuplicateOutputPath {
                path: relpath.into_path_buf(),
            });
        }

        let file_offset = zip_entry.file_offset();
        let expected_compressed_size = zip_entry.compressed_size();
        let expected_uncompressed_size = zip_entry.uncompressed_size();
        let expected_data_descriptor = zip_entry.data_descriptor();

        // Either create the directory or write the file to disk.
        let path = target.join(relpath.as_path());
        let is_dir = zip_entry.dir()?;
        let computed = if is_dir {
            if directories.insert(path.clone()) {
                fs_err::create_dir_all(&path).map_err(Error::Io)?;
            }

            // If this is a directory, we expect the CRC32 to be 0.
            if zip_entry.crc32() != 0 {
                if !skip_validation {
                    return Err(Error::BadCrc32 {
                        path: relpath.to_path_buf(),
                        computed: 0,
                        expected: zip_entry.crc32(),
                    });
                }
            }

            // If this is a directory, we expect the uncompressed size to be 0.
            if zip_entry.uncompressed_size() != 0 {
                if !skip_validation {
                    return Err(Error::BadUncompressedSize {
                        path: relpath.to_path_buf(),
                        computed: 0,
                        expected: zip_entry.uncompressed_size(),
                    });
                }
            }

            ComputedEntry {
                crc32: 0,
                uncompressed_size: 0,
                compressed_size: 0,
                digest: None,
            }
        } else {
            if let Some(parent) = path.parent() {
                if directories.insert(parent.to_path_buf()) {
                    fs_err::create_dir_all(parent).map_err(Error::Io)?;
                }
            }

            // We don't know the file permissions here, because we haven't seen the central directory yet.
            let (actual_uncompressed_size, digest) = match fs_err::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(file) => {
                    // Write the file to disk.
                    let file = AllowStdIo::new(file).compat_write();
                    let size = zip_entry.uncompressed_size();
                    let mut writer = if let Ok(size) = usize::try_from(size) {
                        tokio::io::BufWriter::with_capacity(std::cmp::min(size, 1024 * 1024), file)
                    } else {
                        tokio::io::BufWriter::new(file)
                    };
                    let mut reader = entry.reader_mut().compat();
                    if hash_contents {
                        let (bytes_read, digest) =
                            blake3_copy_with_buffer(&mut reader, &mut writer, &mut copy_buffer)
                                .await
                                .map_err(Error::io_or_zip)?;
                        (bytes_read, Some(digest))
                    } else {
                        let mut bytes_read = 0;
                        copy_buffer.resize(DEFAULT_BUF_SIZE, 0);
                        loop {
                            let read = tokio::io::AsyncReadExt::read(&mut reader, &mut copy_buffer)
                                .await
                                .map_err(Error::io_or_zip)?;
                            if read == 0 {
                                break;
                            }
                            tokio::io::AsyncWriteExt::write_all(&mut writer, &copy_buffer[..read])
                                .await
                                .map_err(Error::Io)?;
                            bytes_read += read as u64;
                        }
                        tokio::io::AsyncWriteExt::flush(&mut writer)
                            .await
                            .map_err(Error::Io)?;
                        (bytes_read, None)
                    }
                }
                Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                    debug!(
                        "Found duplicate local file header for: {}",
                        relpath.as_path().display()
                    );

                    // Read the existing file into memory.
                    let existing_contents = fs_err::read(&path).map_err(Error::Io)?;

                    // Read the entry into memory.
                    let mut expected_contents = Vec::with_capacity(existing_contents.len());
                    let bytes_read = entry
                        .reader_mut()
                        .read_to_end(&mut expected_contents)
                        .await
                        .map_err(Error::io_or_zip)?;

                    // Verify that the existing file contents match the expected contents.
                    if existing_contents != expected_contents {
                        if !skip_validation {
                            return Err(Error::DuplicateLocalFileHeader {
                                path: relpath.to_path_buf(),
                            });
                        }
                    }

                    let digest = hash_contents.then(|| blake3::hash(&expected_contents));
                    (bytes_read as u64, digest)
                }
                Err(err) => return Err(Error::Io(err)),
            };

            // Validate the uncompressed size.
            if actual_uncompressed_size != expected_uncompressed_size {
                if !(expected_compressed_size == 0 && expected_data_descriptor) {
                    if !skip_validation {
                        return Err(Error::BadUncompressedSize {
                            path: relpath.to_path_buf(),
                            computed: actual_uncompressed_size,
                            expected: expected_uncompressed_size,
                        });
                    }
                }
            }

            // Validate the compressed size.
            let reader = entry.reader_mut();
            let actual_compressed_size = reader.bytes_read();
            if actual_compressed_size != expected_compressed_size {
                if !(expected_compressed_size == 0 && expected_data_descriptor) {
                    if !skip_validation {
                        return Err(Error::BadCompressedSize {
                            path: relpath.to_path_buf(),
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
                            path: relpath.to_path_buf(),
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
                digest,
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
                    path: relpath.to_path_buf(),
                });
            }
        }
        if !expected_data_descriptor && descriptor.is_some() {
            if !skip_validation {
                return Err(Error::UnexpectedDataDescriptor {
                    path: relpath.to_path_buf(),
                });
            }
        }

        // If we have a data descriptor, validate it.
        if let Some(descriptor) = descriptor {
            if descriptor.crc != computed.crc32 {
                if !skip_validation {
                    return Err(Error::BadCrc32 {
                        path: relpath.to_path_buf(),
                        computed: computed.crc32,
                        expected: descriptor.crc,
                    });
                }
            }
            if descriptor.uncompressed_size != computed.uncompressed_size {
                if !skip_validation {
                    return Err(Error::BadUncompressedSize {
                        path: relpath.to_path_buf(),
                        computed: computed.uncompressed_size,
                        expected: descriptor.uncompressed_size,
                    });
                }
            }
            if descriptor.compressed_size != computed.compressed_size {
                if !skip_validation {
                    return Err(Error::BadCompressedSize {
                        path: relpath.to_path_buf(),
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
                    is_dir,
                    crc32: computed.crc32,
                    uncompressed_size: computed.uncompressed_size,
                    compressed_size: expected_compressed_size,
                    data_descriptor: expected_data_descriptor,
                    digest: computed.digest,
                });
            }
            std::collections::hash_map::Entry::Occupied(..) => {
                if !skip_validation {
                    return Err(Error::DuplicateLocalFileHeader {
                        path: relpath.to_path_buf(),
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

                // Validate and sanitize the file name to prevent directory traversal attacks.
                let relpath = match SanitizedArchivePath::from_archive_member(path) {
                    Ok(path) => path,
                    Err(_) if skip_validation => None,
                    Err(err) => return Err(err),
                };
                let Some(relpath) = relpath else {
                    continue;
                };
                let is_dir = entry.dir()?;

                // Validate that various fields are consistent between the local file header and the
                // central directory entry.
                match local_headers.remove(&entry.file_offset()) {
                    Some(local_header) => {
                        if local_header.relpath != relpath {
                            if !skip_validation {
                                return Err(Error::ConflictingPaths {
                                    offset: entry.file_offset(),
                                    local_path: local_header.relpath.to_path_buf(),
                                    central_directory_path: relpath.to_path_buf(),
                                });
                            }
                        }
                        if local_header.is_dir != is_dir {
                            if !skip_validation {
                                return Err(Error::ConflictingEntryTypes {
                                    path: relpath.to_path_buf(),
                                    offset: entry.file_offset(),
                                });
                            }
                        }
                        if local_header.crc32 != entry.crc32() {
                            if !skip_validation {
                                return Err(Error::ConflictingChecksums {
                                    path: relpath.to_path_buf(),
                                    offset: entry.file_offset(),
                                    local_crc32: local_header.crc32,
                                    central_directory_crc32: entry.crc32(),
                                });
                            }
                        }
                        if local_header.uncompressed_size != entry.uncompressed_size() {
                            if !skip_validation {
                                return Err(Error::ConflictingUncompressedSizes {
                                    path: relpath.to_path_buf(),
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
                                        path: relpath.to_path_buf(),
                                        offset: entry.file_offset(),
                                        local_compressed_size: local_header.compressed_size,
                                        central_directory_compressed_size: entry.compressed_size(),
                                    });
                                }
                            }
                        }
                        if is_dir {
                            if hash_contents {
                                digest_directories.insert(relpath.clone());
                            }
                        } else if let Some(digest) = local_header.digest {
                            hashed_files.push(HashedFile::new(
                                relpath.clone(),
                                local_header.uncompressed_size,
                                digest,
                                entry
                                    .unix_permissions()
                                    .is_some_and(|mode| mode & 0o111 != 0),
                            ));
                        } else {
                            files.push(UnhashedFile::new(
                                relpath.to_path_buf(),
                                local_header.uncompressed_size,
                            ));
                        }
                    }
                    None => {
                        if !skip_validation {
                            return Err(Error::MissingLocalFileHeader {
                                path: relpath.to_path_buf(),
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

                    if is_dir {
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
                                        path: relpath.to_path_buf(),
                                    });
                                }
                            }
                        }
                    }

                    // The executable bit is the only permission we preserve, otherwise we use the OS defaults.
                    // https://github.com/pypa/pip/blob/3898741e29b7279e7bffe044ecfbe20f6a438b1e/src/pip/_internal/utils/unpacking.py#L88-L100
                    let has_any_executable_bit = mode & 0o111;
                    if has_any_executable_bit != 0 {
                        let path = target.join(relpath.as_path());
                        let permissions = fs_err::metadata(&path).map_err(Error::Io)?.permissions();
                        if permissions.mode() & 0o111 != 0o111 {
                            fs_err::set_permissions(
                                &path,
                                Permissions::from_mode(permissions.mode() | 0o111),
                            )
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
                path: value.relpath.to_path_buf(),
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

    if hash_contents {
        let tree = directory_tree_from_extracted(&hashed_files, &digest_directories)?;
        Ok(UnzipOutput::Hashed {
            files: hashed_files,
            tree,
        })
    } else {
        Ok(UnzipOutput::Unhashed(files))
    }
}

/// Unpack the given tar archive into the destination directory.
///
/// Returns the list of unpacked files and their sizes.
async fn untar_in_tar_codec<R: tokio::io::AsyncRead + Unpin>(
    reader: R,
    dst: &Path,
) -> Result<Vec<UnhashedFile>, ExtractError<DecodeError>> {
    let decode_policy = DecodePolicy::default().pax_policy(
        PaxDecodePolicy::default()
            // NOTE: We intentionally allow (ignore) `SCHILY.*` and `LIBARCHIVE.*`
            // pax extensions here, but continue to forbid others.
            // The rationale here is that we know these vendor namespaces don't affect framing in
            // any way, whereas others (like GNU sparse extensions) can.
            .vendor_extension_policy(PaxVendorExtensionPolicy::ignore(["SCHILY", "LIBARCHIVE"]))
            // NOTE: We allow pax records to contain non-UTF-8 values.
            // This is a violation of the pax spec, but is prevalent in the
            // wild thanks to both GNU tar and libarchive encoding `SCHILY.xattr`
            // as raw binary.
            .allow_non_utf8_pax_vendor_values(true),
    );
    let archive = TarArchive::new(reader).with_policy(decode_policy);

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
    files: &'files mut Vec<UnhashedFile>,
}

impl<'files, A> RecordingArchive<'files, A> {
    fn new(archive: A, files: &'files mut Vec<UnhashedFile>) -> Self {
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
            files.push(UnhashedFile::new(PathBuf::from(&metadata.path), *size));
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

/// Unpack the given tar archive into the destination directory with `astral-tokio-tar`.
///
/// This is equivalent to `archive.unpack_in(dst)`, but it also preserves the executable bit.
///
/// Returns the list of unpacked files and their sizes.
async fn untar_in_tokio_tar(
    mut archive: tokio_tar::Archive<&'_ mut (dyn tokio::io::AsyncRead + Unpin)>,
    dst: &Path,
) -> std::io::Result<Vec<UnhashedFile>> {
    // Like `tokio-tar`, canonicalize the destination prior to unpacking.
    let dst = fs_err::tokio::canonicalize(dst).await?;

    // Memoize filesystem calls to canonicalize paths.
    let mut memo = FxHashSet::default();

    let mut files = Vec::new();

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

        let entry_type = file.header().entry_type();

        // Unpack the file into the destination directory.
        let unpacked_at = file.unpack_in_raw(&dst, &mut memo).await?;

        // Collect file paths (excluding directories) that were unpacked successfully.
        if unpacked_at.is_some() && (entry_type.is_file() || entry_type.is_hard_link()) {
            let relpath = file.path()?.into_owned();
            let size = file.effective_size();
            files.push(UnhashedFile::new(relpath, size));
        }

        // Preserve the executable bit.
        #[cfg(unix)]
        {
            use std::fs::Permissions;
            use std::os::unix::fs::PermissionsExt;

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

    Ok(files)
}

/// Select the tar implementation and unpack the archive into the destination directory.
async fn untar_in<R: tokio::io::AsyncRead + Unpin>(
    mut reader: R,
    dst: &Path,
) -> Result<Vec<UnhashedFile>, Error> {
    if uv_preview::is_enabled(PreviewFeature::TarCodec) {
        untar_in_tar_codec(reader, dst).await.map_err(Error::from)
    } else {
        let archive =
            tokio_tar::ArchiveBuilder::new(&mut reader as &mut (dyn tokio::io::AsyncRead + Unpin))
                .set_preserve_mtime(false)
                .set_preserve_permissions(false)
                .set_allow_external_symlinks(false)
                .build();
        untar_in_tokio_tar(archive, dst)
            .await
            .map_err(Error::io_or_tar)
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
) -> Result<Vec<UnhashedFile>, Error> {
    let reader = tokio::io::BufReader::with_capacity(DEFAULT_BUF_SIZE, reader);
    let decompressed_bytes = async_compression::tokio::bufread::GzipDecoder::new(reader);
    untar_in(decompressed_bytes, target.as_ref()).await
}

/// Unpack a `.tar.zst` archive into the target directory, without requiring `Seek`.
///
/// This is useful for unpacking files as they're being downloaded.
///
/// Returns the list of unpacked files and their sizes.
async fn untar_zst<R: tokio::io::AsyncRead + Unpin>(
    reader: R,
    target: impl AsRef<Path>,
) -> Result<Vec<UnhashedFile>, Error> {
    let reader = tokio::io::BufReader::with_capacity(DEFAULT_BUF_SIZE, reader);
    let decompressed_bytes = async_compression::tokio::bufread::ZstdDecoder::new(reader);
    untar_in(decompressed_bytes, target.as_ref()).await
}

/// Unpack a `.tar` archive into the target directory, without requiring `Seek`.
///
/// This is useful for unpacking files as they're being downloaded.
///
/// Returns the list of unpacked files and their sizes.
async fn untar<R: tokio::io::AsyncRead + Unpin>(
    reader: R,
    target: impl AsRef<Path>,
) -> Result<Vec<UnhashedFile>, Error> {
    let reader = tokio::io::BufReader::with_capacity(DEFAULT_BUF_SIZE, reader);
    untar_in(reader, target.as_ref()).await
}

/// Unpack a `.zip`, `.tar.gz`, or `.tar.zst` archive into the target directory,
/// without requiring `Seek`.
///
/// Returns the temporary directory and the list of unpacked files and their sizes.
/// ZIP extraction transfers ownership of the directory to a blocking worker; see [`unzip`].
pub async fn archive<R: tokio::io::AsyncRead + Unpin>(
    reader: R,
    ext: SourceDistExtension,
    target: TempDir,
) -> Result<(TempDir, Vec<UnhashedFile>), Error> {
    let files = match ext {
        SourceDistExtension::Legacy(LegacySourceDistExtension::Zip) => {
            return unzip(reader, target).await;
        }
        SourceDistExtension::Legacy(LegacySourceDistExtension::Tar) => {
            untar(reader, target.path()).await
        }
        SourceDistExtension::Legacy(LegacySourceDistExtension::Tgz)
        | SourceDistExtension::TarGz => untar_gz(reader, target.path()).await,
        SourceDistExtension::Legacy(LegacySourceDistExtension::TarZst) => {
            untar_zst(reader, target.path()).await
        }
        SourceDistExtension::Legacy(_) => Err(Error::UnsupportedCompression),
    }?;
    Ok((target, files))
}
