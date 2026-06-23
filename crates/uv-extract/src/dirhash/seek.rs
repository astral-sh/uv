//! Directory hashing while extracting seekable ZIP archives.

use std::io::SeekFrom;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock, mpsc};

use crate::vendor::CloneableSeekableReader;
use crate::{CompressionMethod, Error, insecure_no_validate, validate_archive_member_name};
use async_zip::StoredZipEntry;
use async_zip::base::read::seek::ZipFileReader;
use async_zip::error::ZipError;
use futures::executor::block_on;
use futures::io::{AllowStdIo, AsyncRead, AsyncReadExt, AsyncSeek, AsyncSeekExt, AsyncWriteExt};
use rayon::prelude::*;
use rustc_hash::FxHashSet;
use tokio::sync::{Semaphore, SemaphorePermit};
use tracing::warn;
use uv_configuration::initialize_rayon_once;
use uv_warnings::warn_user_once;

use super::{
    DirectoryDigest, ExtractedFile, directory_digest_from_extracted, empty_directory_paths,
};
use crate::archive_path::SanitizedArchivePath;

const LOCAL_FILE_HEADER_LENGTH: u64 = 30;
const LOCAL_FILE_HEADER_LENGTH_USIZE: usize = 30;
const LOCAL_FILE_HEADER_SIGNATURE: u32 = 0x0403_4b50;
#[cfg(not(test))]
const STORED_HASH_FAST_PATH_THRESHOLD: u64 = 8 * 1024 * 1024;
#[cfg(test)]
const STORED_HASH_FAST_PATH_THRESHOLD: u64 = 1;
const PARALLEL_HASH_THRESHOLD: u64 = 8 * 1024 * 1024;
const PARALLEL_HASH_BUFFER_POOL_THRESHOLD: u64 = 64 * 1024 * 1024;
const HASH_BUFFER_SIZE: usize = 16 * 1024 * 1024;
const PARALLEL_HASH_BUFFER_COUNT: usize = 2;
const ALLOCATING_HASH_BUFFER_COUNT: usize = 3;
const HASH_BUFFER_PERMIT_COUNT: usize = 4;
const _: () = {
    assert!(ALLOCATING_HASH_BUFFER_COUNT <= HASH_BUFFER_PERMIT_COUNT);
    assert!(PARALLEL_HASH_BUFFER_COUNT <= HASH_BUFFER_PERMIT_COUNT);
};
static HASH_BUFFER_LIMITER: Semaphore = Semaphore::const_new(HASH_BUFFER_PERMIT_COUNT);
static HASH_THREAD_POOL: OnceLock<Option<rayon::ThreadPool>> = OnceLock::new();

/// Reserve 16 MiB hash-buffer slots from the process-wide 64 MiB budget.
fn acquire_hash_buffers(count: usize) -> Result<SemaphorePermit<'static>, Error> {
    let count = u32::try_from(count)
        .map_err(|_| Error::Io(std::io::Error::other("invalid hash buffer count")))?;
    block_on(HASH_BUFFER_LIMITER.acquire_many(count))
        .map_err(|_| Error::Io(std::io::Error::other("hash buffer limiter closed")))
}

/// A successfully extracted file, or an explicit directory that can affect the digest.
enum ExtractedEntry {
    File {
        path: SanitizedArchivePath,
        size: u64,
        executable: bool,
        digest: Option<blake3::Hash>,
    },
    Directory(SanitizedArchivePath),
}

struct UnzipOutput {
    files: Vec<(PathBuf, u64)>,
    extracted_files: Vec<ExtractedFile>,
    digest: Option<DirectoryDigest>,
}

/// Unzip a `.zip` archive into the target directory.
pub(crate) fn unzip(reader: fs_err::File, target: &Path) -> Result<Vec<(PathBuf, u64)>, Error> {
    Ok(unzip_inner(reader, target, false)?.files)
}

/// Unzip a `.zip` archive into the target directory while computing a digest of the extracted files.
///
/// Returns the list of unpacked files and their sizes, along with a digest over the canonicalized
/// extracted file paths, executable bits, sizes, contents, and empty leaf directories.
pub(crate) fn unzip_and_hash(
    reader: fs_err::File,
    target: &Path,
) -> Result<(Vec<ExtractedFile>, DirectoryDigest), Error> {
    let output = unzip_inner(reader, target, true)?;
    let Some(digest) = output.digest else {
        return Err(Error::Io(std::io::Error::other(
            "seekable ZIP digest was not computed",
        )));
    };
    Ok((output.extracted_files, digest))
}

fn unzip_inner(
    reader: fs_err::File,
    target: &Path,
    hash_contents: bool,
) -> Result<UnzipOutput, Error> {
    let (reader, filename) = reader.into_parts();

    // Parse the central directory once, then clone the archive reader per Rayon worker so
    // extraction stays parallel for already-downloaded wheels.
    let archive = block_on(ZipFileReader::new(AllowStdIo::new(
        CloneableSeekableReader::new(reader),
    )))?;
    if hash_contents {
        validate_unique_output_paths(archive.file().entries())?;
    }

    let directories = Mutex::new(FxHashSet::default());
    let skip_validation = insecure_no_validate();
    // Initialize the threadpool with the user settings.
    initialize_rayon_once();
    let extract = |file_number| {
        let mut archive = archive.clone();
        extract_entry(
            &mut archive,
            file_number,
            target,
            &directories,
            skip_validation,
            &filename,
            hash_contents,
        )
    };

    if !hash_contents {
        let files = (0..archive.file().entries().len())
            .into_par_iter()
            .map(extract)
            .filter_map(|result| match result {
                Ok(Some(ExtractedEntry::File { path, size, .. })) => {
                    Some(Ok((path.into_path_buf(), size)))
                }
                Ok(Some(ExtractedEntry::Directory(_)) | None) => None,
                Err(err) => Some(Err(err)),
            })
            .collect::<Result<_, Error>>()?;
        return Ok(UnzipOutput {
            files,
            extracted_files: Vec::new(),
            digest: None,
        });
    }

    let extracted = (0..archive.file().entries().len())
        .into_par_iter()
        .map(extract)
        // Filter out skipped dangerous paths, then collect files and directory candidates.
        .filter_map(Result::transpose)
        .collect::<Result<Vec<_>, Error>>()?;

    let mut extracted_files = Vec::with_capacity(extracted.len());
    let mut digest_directories = FxHashSet::default();
    for extracted in extracted {
        match extracted {
            ExtractedEntry::File {
                path,
                size,
                executable,
                digest,
            } => {
                if let Some(digest) = digest {
                    extracted_files.push(ExtractedFile::new(path, size, executable, digest));
                }
            }
            ExtractedEntry::Directory(path) => {
                digest_directories.insert(path);
            }
        }
    }
    let hash_directories = empty_directory_paths(
        &digest_directories,
        extracted_files.iter().map(ExtractedFile::sanitized_path),
    );
    let digest = directory_digest_from_extracted(&extracted_files, hash_directories);
    Ok(UnzipOutput {
        files: Vec::new(),
        extracted_files,
        digest: Some(digest),
    })
}

/// Reject entries that would write to the same sanitized output path.
///
/// This preflight runs before parallel extraction so duplicate entries cannot race to determine
/// which contents are persisted or hashed.
fn validate_unique_output_paths(entries: &[StoredZipEntry]) -> Result<(), Error> {
    let mut paths = FxHashSet::default();
    for (file_number, entry) in entries.iter().enumerate() {
        let file_name = entry_file_name(entry, file_number)?;
        let Some(path) = SanitizedArchivePath::from_archive_member(file_name) else {
            continue;
        };
        if !paths.insert(path.clone()) {
            return Err(Error::DuplicateOutputPath {
                path: path.into_path_buf(),
            });
        }
    }
    Ok(())
}

/// Extract a single central-directory entry from a seekable ZIP archive.
fn extract_entry<R>(
    archive: &mut ZipFileReader<AllowStdIo<R>>,
    file_number: usize,
    target: &Path,
    directories: &Mutex<FxHashSet<PathBuf>>,
    skip_validation: bool,
    filename: &Path,
    hash_contents: bool,
) -> Result<Option<ExtractedEntry>, Error>
where
    R: std::io::BufRead + std::io::Seek + Clone + Send + Sync + Unpin,
    AllowStdIo<R>: Clone,
{
    let entry = archive.file().entries()[file_number].clone();
    let file_name = entry_file_name(&entry, file_number)?;
    let compression = CompressionMethod::from(entry.compression());
    warn_on_unsupported_compression(filename, &compression);

    if let Err(err) = validate_archive_member_name(file_name) {
        if !skip_validation {
            return Err(err);
        }
    }

    let Some(enclosed_name) = SanitizedArchivePath::from_archive_member(file_name) else {
        warn!("Skipping unsafe file name: {file_name}");
        return Ok(None);
    };

    let path = target.join(enclosed_name.as_path());
    if entry.dir()? {
        create_directory_once(directories, &path)?;
        if hash_contents {
            validate_directory_entry(&entry, enclosed_name.as_path(), skip_validation)?;
        }
        return Ok(Some(ExtractedEntry::Directory(enclosed_name)));
    }

    if let Some(parent) = path.parent() {
        create_directory_once(directories, parent)?;
    }

    extract_file_entry(
        archive,
        &entry,
        file_number,
        enclosed_name,
        &path,
        &compression,
        skip_validation,
        hash_contents,
    )
    .map(Some)
}

/// Return an entry file name from the central directory.
fn entry_file_name(entry: &StoredZipEntry, file_number: usize) -> Result<&str, Error> {
    match entry.filename().as_str() {
        Ok(file_name) => Ok(file_name),
        Err(ZipError::StringNotUtf8) => Err(Error::CentralDirectoryEntryNotUtf8 {
            index: file_number as u64,
        }),
        Err(err) => Err(err.into()),
    }
}

/// Warn for compression methods that uv still accepts but does not recommend.
fn warn_on_unsupported_compression(filename: &Path, compression: &CompressionMethod) {
    if compression.is_well_known() {
        return;
    }

    warn_user_once!(
        "One or more file entries in '{filename}' use the '{compression}' compression method, which is not widely supported. A future version of uv will reject ZIP archives containing entries compressed with this method. Entries must be compressed with the '{stored}', '{deflate}', or '{zstd}' compression methods.",
        filename = filename.display(),
        stored = CompressionMethod::Stored,
        deflate = CompressionMethod::Deflated,
        zstd = CompressionMethod::Zstd,
    );
}

/// Create a directory once across parallel extraction workers.
fn create_directory_once(
    directories: &Mutex<FxHashSet<PathBuf>>,
    path: &Path,
) -> Result<(), Error> {
    let mut directories = directories.lock().map_err(|_| directory_lock_error())?;
    if directories.insert(path.to_path_buf()) {
        fs_err::create_dir_all(path).map_err(Error::Io)?;
    }

    Ok(())
}

/// Validate the metadata for a directory entry.
fn validate_directory_entry(
    entry: &StoredZipEntry,
    path: &Path,
    skip_validation: bool,
) -> Result<(), Error> {
    if skip_validation {
        return Ok(());
    }

    if entry.crc32() != 0 {
        return Err(Error::BadCrc32 {
            path: path.to_path_buf(),
            computed: 0,
            expected: entry.crc32(),
        });
    }

    if entry.uncompressed_size() != 0 {
        return Err(Error::BadUncompressedSize {
            path: path.to_path_buf(),
            computed: 0,
            expected: entry.uncompressed_size(),
        });
    }

    Ok(())
}

/// Extract a regular file entry and return its digest metadata.
fn extract_file_entry<R>(
    archive: &mut ZipFileReader<AllowStdIo<R>>,
    entry: &StoredZipEntry,
    file_number: usize,
    enclosed_name: SanitizedArchivePath,
    path: &Path,
    compression: &CompressionMethod,
    skip_validation: bool,
    hash_contents: bool,
) -> Result<ExtractedEntry, Error>
where
    R: std::io::BufRead + std::io::Seek + Clone + Send + Sync + Unpin,
    AllowStdIo<R>: Clone,
{
    let outfile = if hash_contents {
        fs_err::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
    } else {
        fs_err::File::create(path)
    }
    .map_err(Error::Io)?;
    let size = entry.uncompressed_size();
    let unix_permissions = entry.unix_permissions();
    let executable = unix_permissions.is_some_and(|mode| mode & 0o111 != 0);
    let writer = buffered_file_writer(outfile, size);

    let (copied, computed_crc32, digest) = if hash_contents {
        let (copied, computed_crc32, digest) =
            copy_entry_with_digest(archive, entry, file_number, writer, compression, size)?;
        (copied, computed_crc32, Some(digest))
    } else {
        block_on(copy_entry(archive, file_number, writer, false))?
    };
    validate_file_entry(
        enclosed_name.as_path(),
        copied,
        size,
        computed_crc32,
        entry.crc32(),
        skip_validation,
    )?;
    #[cfg(unix)]
    preserve_executable_bit(path, unix_permissions)?;

    Ok(ExtractedEntry::File {
        path: enclosed_name,
        size,
        executable,
        digest,
    })
}

/// Build a buffered writer sized for the expected entry contents.
fn buffered_file_writer(file: fs_err::File, size: u64) -> std::io::BufWriter<fs_err::File> {
    if let Ok(size) = usize::try_from(size) {
        std::io::BufWriter::with_capacity(std::cmp::min(size, 1024 * 1024), file)
    } else {
        std::io::BufWriter::new(file)
    }
}

/// Copy an entry to disk while computing its content digest.
///
/// Small entries are hashed while copying. Large compressed entries move hashing to a dedicated
/// thread, while large stored entries hash their raw archive bytes in parallel with extraction.
fn copy_entry_with_digest<R>(
    archive: &mut ZipFileReader<AllowStdIo<R>>,
    entry: &StoredZipEntry,
    file_number: usize,
    writer: std::io::BufWriter<fs_err::File>,
    compression: &CompressionMethod,
    size: u64,
) -> Result<(u64, u32, blake3::Hash), Error>
where
    R: std::io::BufRead + std::io::Seek + Clone + Send + Sync + Unpin,
    AllowStdIo<R>: Clone,
{
    let use_stored_hash_fast_path = matches!(compression, &CompressionMethod::Stored)
        && size >= STORED_HASH_FAST_PATH_THRESHOLD
        && entry.compressed_size() == size;

    if use_stored_hash_fast_path {
        let _permit = acquire_hash_buffers(1)?;
        let header_offset = entry.header_offset();
        let mut hash_archive = archive.clone();
        let (copied, stored_digest) = std::thread::scope(|scope| {
            let stored_digest =
                scope.spawn(move || hash_stored_entry(&mut hash_archive, header_offset, size));
            let copied = block_on(copy_entry(archive, file_number, writer, false));
            (copied, stored_digest.join())
        });
        let (copied, computed_crc32, digest) = copied?;
        debug_assert!(digest.is_none());
        let stored_digest = stored_digest.map_err(|_| thread_panic_error())??;
        return Ok((copied, computed_crc32, stored_digest));
    }

    if size >= PARALLEL_HASH_THRESHOLD {
        return copy_entry_with_hash_thread(archive, file_number, writer, size);
    }

    let (copied, computed_crc32, digest) =
        block_on(copy_entry(archive, file_number, writer, true))?;
    let Some(digest) = digest else {
        return Err(Error::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "missing digest for ZIP entry",
        )));
    };
    Ok((copied, computed_crc32, digest))
}

/// Validate the copied size and CRC for a file entry.
fn validate_file_entry(
    path: &Path,
    copied: u64,
    expected_size: u64,
    computed_crc32: u32,
    expected_crc32: u32,
    skip_validation: bool,
) -> Result<(), Error> {
    if skip_validation {
        return Ok(());
    }

    if copied != expected_size {
        return Err(Error::BadUncompressedSize {
            path: path.to_path_buf(),
            computed: copied,
            expected: expected_size,
        });
    }

    if computed_crc32 != expected_crc32 {
        return Err(Error::BadCrc32 {
            path: path.to_path_buf(),
            computed: computed_crc32,
            expected: expected_crc32,
        });
    }

    Ok(())
}

#[cfg(unix)]
/// Preserve executable permissions according to pip's wheel extraction behavior.
fn preserve_executable_bit(path: &Path, unix_permissions: Option<u16>) -> Result<(), Error> {
    use std::fs::Permissions;
    use std::os::unix::fs::PermissionsExt;

    let Some(mode) = unix_permissions else {
        return Ok(());
    };

    // https://github.com/pypa/pip/blob/3898741e29b7279e7bffe044ecfbe20f6a438b1e/src/pip/_internal/utils/unpacking.py#L88-L100
    if mode & 0o111 == 0 {
        return Ok(());
    }

    let permissions = fs_err::metadata(path).map_err(Error::Io)?.permissions();
    if permissions.mode() & 0o111 == 0o111 {
        return Ok(());
    }

    fs_err::set_permissions(path, Permissions::from_mode(permissions.mode() | 0o111))
        .map_err(Error::Io)
}

/// Return an error for a poisoned directory memoization lock.
fn directory_lock_error() -> Error {
    Error::Io(std::io::Error::other("directory set lock poisoned"))
}

/// Copy an entry, optionally hashing the same uncompressed bytes written to disk.
async fn copy_entry<R>(
    archive: &mut ZipFileReader<AllowStdIo<R>>,
    file_number: usize,
    writer: std::io::BufWriter<fs_err::File>,
    hash_contents: bool,
) -> Result<(u64, u32, Option<blake3::Hash>), Error>
where
    R: std::io::BufRead + std::io::Seek + Unpin,
{
    let mut file = archive.reader_with_entry(file_number).await?;
    let mut writer = AllowStdIo::new(writer);
    let mut hasher = hash_contents.then(blake3::Hasher::new);
    let mut copied = 0;
    let mut buffer = vec![0; 128 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .await
            .map_err(Error::io_or_compression)?;
        if read == 0 {
            break;
        }
        if let Some(hasher) = hasher.as_mut() {
            hasher.update(&buffer[..read]);
        }
        writer.write_all(&buffer[..read]).await.map_err(Error::Io)?;
        copied += read as u64;
    }
    writer.flush().await.map_err(Error::Io)?;
    Ok((
        copied,
        file.compute_hash(),
        hasher.map(|hasher| hasher.finalize()),
    ))
}

/// Copy an entry while hashing contents on a separate thread.
fn copy_entry_with_hash_thread<R>(
    archive: &mut ZipFileReader<AllowStdIo<R>>,
    file_number: usize,
    writer: std::io::BufWriter<fs_err::File>,
    size: u64,
) -> Result<(u64, u32, blake3::Hash), Error>
where
    R: std::io::BufRead + std::io::Seek + Unpin,
{
    if size < PARALLEL_HASH_BUFFER_POOL_THRESHOLD {
        let _permit = acquire_hash_buffers(ALLOCATING_HASH_BUFFER_COUNT)?;
        return copy_entry_with_allocating_hash_thread(archive, file_number, writer);
    }
    let _permit = acquire_hash_buffers(PARALLEL_HASH_BUFFER_COUNT)?;
    copy_entry_with_buffer_pool_hash_thread(archive, file_number, writer)
}

/// Copy and hash an entry using freshly allocated buffers for each transferred chunk.
fn copy_entry_with_allocating_hash_thread<R>(
    archive: &mut ZipFileReader<AllowStdIo<R>>,
    file_number: usize,
    writer: std::io::BufWriter<fs_err::File>,
) -> Result<(u64, u32, blake3::Hash), Error>
where
    R: std::io::BufRead + std::io::Seek + Unpin,
{
    std::thread::scope(|scope| {
        let (sender, receiver) = mpsc::sync_channel::<Vec<u8>>(1);
        let hash_thread = scope.spawn(move || {
            let mut hasher = blake3::Hasher::new();
            while let Ok(chunk) = receiver.recv() {
                hasher.update(&chunk);
            }
            hasher.finalize()
        });

        let copied = block_on(async {
            let mut file = archive.reader_with_entry(file_number).await?;
            let mut writer = AllowStdIo::new(writer);
            let mut copied = 0;
            loop {
                let mut buffer = vec![0; HASH_BUFFER_SIZE];
                let read = file
                    .read(&mut buffer)
                    .await
                    .map_err(Error::io_or_compression)?;
                if read == 0 {
                    break;
                }
                writer.write_all(&buffer[..read]).await.map_err(Error::Io)?;
                copied += read as u64;
                buffer.truncate(read);
                sender.send(buffer).map_err(|_| {
                    Error::Io(std::io::Error::new(
                        std::io::ErrorKind::BrokenPipe,
                        "failed to send ZIP entry chunk to hash thread",
                    ))
                })?;
            }
            writer.flush().await.map_err(Error::Io)?;
            Ok::<_, Error>((copied, file.compute_hash()))
        });

        drop(sender);
        let digest = hash_thread.join().map_err(|_| thread_panic_error())?;
        copied.map(|(copied, computed_crc32)| (copied, computed_crc32, digest))
    })
}

/// Copy and hash an entry using a small recycled buffer pool to reduce large allocations.
fn copy_entry_with_buffer_pool_hash_thread<R>(
    archive: &mut ZipFileReader<AllowStdIo<R>>,
    file_number: usize,
    writer: std::io::BufWriter<fs_err::File>,
) -> Result<(u64, u32, blake3::Hash), Error>
where
    R: std::io::BufRead + std::io::Seek + Unpin,
{
    std::thread::scope(|scope| {
        struct HashChunk {
            buffer: Vec<u8>,
            read: usize,
        }

        let (sender, receiver) = mpsc::sync_channel::<HashChunk>(PARALLEL_HASH_BUFFER_COUNT);
        let (buffer_sender, buffer_receiver) =
            mpsc::sync_channel::<Vec<u8>>(PARALLEL_HASH_BUFFER_COUNT);
        for _ in 0..PARALLEL_HASH_BUFFER_COUNT {
            buffer_sender.send(vec![0; HASH_BUFFER_SIZE]).map_err(|_| {
                Error::Io(std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    "failed to initialize ZIP entry hash buffer",
                ))
            })?;
        }

        let recycle_sender = buffer_sender.clone();
        drop(buffer_sender);
        let hash_thread = scope.spawn(move || {
            let mut hasher = blake3::Hasher::new();
            while let Ok(chunk) = receiver.recv() {
                hasher.update(&chunk.buffer[..chunk.read]);
                if recycle_sender.send(chunk.buffer).is_err() {
                    break;
                }
            }
            hasher.finalize()
        });

        let copied = block_on(async {
            let mut file = archive.reader_with_entry(file_number).await?;
            let mut writer = AllowStdIo::new(writer);
            let mut copied = 0;
            loop {
                let mut buffer = buffer_receiver.recv().map_err(|_| {
                    Error::Io(std::io::Error::new(
                        std::io::ErrorKind::BrokenPipe,
                        "failed to receive ZIP entry hash buffer",
                    ))
                })?;
                let read = file
                    .read(&mut buffer)
                    .await
                    .map_err(Error::io_or_compression)?;
                if read == 0 {
                    break;
                }
                writer.write_all(&buffer[..read]).await.map_err(Error::Io)?;
                copied += read as u64;
                sender.send(HashChunk { buffer, read }).map_err(|_| {
                    Error::Io(std::io::Error::new(
                        std::io::ErrorKind::BrokenPipe,
                        "failed to send ZIP entry chunk to hash thread",
                    ))
                })?;
            }
            writer.flush().await.map_err(Error::Io)?;
            Ok::<_, Error>((copied, file.compute_hash()))
        });

        drop(sender);
        let digest = hash_thread.join().map_err(|_| thread_panic_error())?;
        copied.map(|(copied, computed_crc32)| (copied, computed_crc32, digest))
    })
}

/// Hash a stored entry's raw bytes from an already-open archive reader.
///
/// Stored entries have identical compressed and uncompressed contents, so a cloned seekable reader
/// can hash the payload in parallel while the primary reader extracts and validates the entry.
fn hash_stored_entry<R>(
    archive: &mut ZipFileReader<AllowStdIo<R>>,
    header_offset: u64,
    compressed_size: u64,
) -> Result<blake3::Hash, Error>
where
    R: std::io::BufRead + std::io::Seek + Unpin,
{
    block_on(async {
        let reader = archive.inner_mut();
        let data_offset = stored_entry_data_offset(reader, header_offset).await?;
        reader
            .seek(SeekFrom::Start(data_offset))
            .await
            .map_err(Error::Io)?;

        let mut hasher = blake3::Hasher::new();
        let mut remaining = compressed_size;
        let mut buffer = vec![0; HASH_BUFFER_SIZE];
        while remaining > 0 {
            let read_size = usize::try_from(remaining.min(buffer.len() as u64)).map_err(|_| {
                Error::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "stored ZIP entry is too large to hash",
                ))
            })?;
            reader
                .read_exact(&mut buffer[..read_size])
                .await
                .map_err(Error::Io)?;
            update_hash_rayon(&mut hasher, &buffer[..read_size]);
            remaining -= read_size as u64;
        }

        Ok(hasher.finalize())
    })
}

/// Return the byte offset of a stored entry's payload from its local file header.
///
/// The central-directory offset points to the header, so the variable-length file name and extra
/// field must be skipped before hashing the payload.
async fn stored_entry_data_offset<R>(reader: &mut R, header_offset: u64) -> Result<u64, Error>
where
    R: AsyncRead + AsyncSeek + Unpin,
{
    reader
        .seek(SeekFrom::Start(header_offset))
        .await
        .map_err(Error::Io)?;
    let mut header = [0; LOCAL_FILE_HEADER_LENGTH_USIZE];
    reader.read_exact(&mut header).await.map_err(Error::Io)?;

    let signature = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
    if signature != LOCAL_FILE_HEADER_SIGNATURE {
        return Err(Error::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "invalid ZIP local file header signature",
        )));
    }

    let filename_length = u64::from(u16::from_le_bytes([header[26], header[27]]));
    let extra_field_length = u64::from(u16::from_le_bytes([header[28], header[29]]));
    header_offset
        .checked_add(LOCAL_FILE_HEADER_LENGTH)
        .and_then(|offset| offset.checked_add(filename_length))
        .and_then(|offset| offset.checked_add(extra_field_length))
        .ok_or_else(|| {
            Error::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "ZIP local file header is too large",
            ))
        })
}

fn thread_panic_error() -> Error {
    Error::Io(std::io::Error::other("hash thread panicked"))
}

/// Hash a large stored-entry chunk using the bounded hash pool when available.
fn update_hash_rayon(hasher: &mut blake3::Hasher, bytes: &[u8]) {
    match HASH_THREAD_POOL.get_or_init(build_hash_thread_pool) {
        Some(pool) => {
            pool.install(|| {
                hasher.update_rayon(bytes);
            });
        }
        None => {
            hasher.update(bytes);
        }
    }
}

/// Build a small pool for BLAKE3 chunk parallelism without occupying the extraction workers.
fn build_hash_thread_pool() -> Option<rayon::ThreadPool> {
    let threads = std::thread::available_parallelism()
        .map(|threads| threads.get().clamp(1, 4))
        .unwrap_or(1);
    rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .thread_name(|index| format!("uv-extract-hash-{index}"))
        .build()
        .ok()
}
