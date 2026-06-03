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
use tracing::warn;
use uv_configuration::initialize_rayon_once;
use uv_warnings::warn_user_once;

use super::{DirectoryDigest, DirectoryDigestFile, directory_digest, empty_directory_paths};

const LOCAL_FILE_HEADER_LENGTH: u64 = 30;
const LOCAL_FILE_HEADER_LENGTH_USIZE: usize = 30;
const LOCAL_FILE_HEADER_SIGNATURE: u32 = 0x0403_4b50;
#[cfg(not(test))]
const STORED_HASH_FAST_PATH_THRESHOLD: u64 = 8 * 1024 * 1024;
#[cfg(test)]
const STORED_HASH_FAST_PATH_THRESHOLD: u64 = 1;
const STORED_HASH_BUFFER_SIZE: usize = 16 * 1024 * 1024;
const PARALLEL_HASH_THRESHOLD: u64 = 8 * 1024 * 1024;
const PARALLEL_HASH_BUFFER_POOL_THRESHOLD: u64 = 64 * 1024 * 1024;
const PARALLEL_HASH_BUFFER_SIZE: usize = 16 * 1024 * 1024;
const PARALLEL_HASH_BUFFER_COUNT: usize = 2;
static HASH_THREAD_POOL: OnceLock<Option<rayon::ThreadPool>> = OnceLock::new();

/// A successfully extracted file, or an explicit directory that can affect the digest.
enum ExtractedEntry {
    File {
        file: (PathBuf, u64),
        hash_file: DirectoryDigestFile,
    },
    Directory(PathBuf),
}

/// Unzip a `.zip` archive into the target directory while computing a digest of the extracted files.
///
/// Returns the list of unpacked files and their sizes, along with a digest over the canonicalized
/// extracted file paths, executable bits, sizes, contents, and empty leaf directories.
pub(crate) fn unzip_and_hash(
    reader: fs_err::File,
    target: &Path,
) -> Result<(Vec<(PathBuf, u64)>, DirectoryDigest), Error> {
    let (reader, filename) = reader.into_parts();

    // Parse the central directory once, then clone the archive reader per Rayon worker so
    // extraction stays parallel for already-downloaded wheels.
    let archive = block_on(ZipFileReader::new(AllowStdIo::new(
        CloneableSeekableReader::new(reader),
    )))?;
    validate_unique_output_paths(archive.file().entries())?;

    let directories = Mutex::new(FxHashSet::default());
    let skip_validation = insecure_no_validate();
    // Initialize the threadpool with the user settings.
    initialize_rayon_once();
    let extracted = (0..archive.file().entries().len())
        .into_par_iter()
        .map(|file_number| {
            let mut archive = archive.clone();
            extract_entry(
                &mut archive,
                file_number,
                target,
                &directories,
                skip_validation,
                &filename,
            )
        })
        // Filter out skipped dangerous paths, then collect files and directory candidates.
        .filter_map(Result::transpose)
        .collect::<Result<Vec<_>, Error>>()?;

    let mut files = Vec::with_capacity(extracted.len());
    let mut hash_files = Vec::with_capacity(extracted.len());
    let mut digest_directories = FxHashSet::default();
    for extracted in extracted {
        match extracted {
            ExtractedEntry::File { file, hash_file } => {
                files.push(file);
                hash_files.push(hash_file);
            }
            ExtractedEntry::Directory(path) => {
                digest_directories.insert(path);
            }
        }
    }
    let hash_directories = empty_directory_paths(
        digest_directories.iter().map(PathBuf::as_path),
        files.iter().map(|(path, _)| path.as_path()),
    );

    Ok((files, directory_digest(hash_files, hash_directories)))
}

/// Reject entries that would write to the same output path.
fn validate_unique_output_paths(entries: &[StoredZipEntry]) -> Result<(), Error> {
    let mut paths = FxHashSet::default();
    for (file_number, entry) in entries.iter().enumerate() {
        let file_name = entry_file_name(entry, file_number)?;
        let Some(path) = crate::stream::enclosed_name(file_name) else {
            continue;
        };
        if !paths.insert(path.clone()) {
            return Err(Error::DuplicateOutputPath { path });
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

    let Some(enclosed_name) = crate::stream::enclosed_name(file_name) else {
        warn!("Skipping unsafe file name: {file_name}");
        return Ok(None);
    };

    let path = target.join(&enclosed_name);
    if entry.dir()? {
        create_directory_once(directories, &path)?;
        validate_directory_entry(&entry, &enclosed_name, skip_validation)?;
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
    enclosed_name: PathBuf,
    path: &Path,
    compression: &CompressionMethod,
    skip_validation: bool,
) -> Result<ExtractedEntry, Error>
where
    R: std::io::BufRead + std::io::Seek + Clone + Send + Sync + Unpin,
    AllowStdIo<R>: Clone,
{
    let outfile = fs_err::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(Error::Io)?;
    let size = entry.uncompressed_size();
    let unix_permissions = entry.unix_permissions();
    let executable = unix_permissions.is_some_and(|mode| mode & 0o111 != 0);
    let writer = buffered_file_writer(outfile, size);

    let (copied, computed_crc32, digest) =
        copy_entry_with_digest(archive, entry, file_number, writer, compression, size)?;
    validate_file_entry(
        &enclosed_name,
        copied,
        size,
        computed_crc32,
        entry.crc32(),
        skip_validation,
    )?;
    preserve_executable_bit(path, unix_permissions)?;

    let hash_file = DirectoryDigestFile::new(&enclosed_name, size, executable, digest);
    Ok(ExtractedEntry::File {
        file: (enclosed_name, size),
        hash_file,
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

#[cfg(not(unix))]
/// Preserve executable permissions according to pip's wheel extraction behavior.
fn preserve_executable_bit(_path: &Path, _unix_permissions: Option<u16>) -> Result<(), Error> {
    Ok(())
}

/// Return an error for a poisoned directory memoization lock.
fn directory_lock_error() -> Error {
    Error::Io(std::io::Error::other("directory set lock poisoned"))
}

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
        return copy_entry_with_allocating_hash_thread(archive, file_number, writer);
    }
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
                let mut buffer = vec![0; PARALLEL_HASH_BUFFER_SIZE];
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
            buffer_sender
                .send(vec![0; PARALLEL_HASH_BUFFER_SIZE])
                .map_err(|_| {
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
        let mut buffer = vec![0; STORED_HASH_BUFFER_SIZE];
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::super::DirectoryDigest;
    use super::unzip_and_hash;

    struct ZipEntry<'a> {
        path: &'a str,
        contents: &'a [u8],
        mode: u32,
    }

    #[test]
    fn directory_digest_is_stable_across_archive_metadata_and_order()
    -> Result<(), Box<dyn std::error::Error>> {
        let first_entries = [
            ZipEntry {
                path: "example/__init__.py",
                contents: b"VALUE = 1\n",
                mode: 0o100_644,
            },
            ZipEntry {
                path: "example-1.0.0.dist-info/METADATA",
                contents: b"Name: example\nVersion: 1.0.0\n",
                mode: 0o100_644,
            },
        ];
        let second_entries = [
            ZipEntry {
                path: "example-1.0.0.dist-info/METADATA",
                contents: b"Name: example\nVersion: 1.0.0\n",
                mode: 0o100_644,
            },
            ZipEntry {
                path: "example/__init__.py",
                contents: b"VALUE = 1\n",
                mode: 0o100_644,
            },
        ];

        let first_archive = zip_archive(&first_entries, b"first archive comment");
        let second_archive = zip_archive(&second_entries, b"second archive comment");
        assert_ne!(first_archive, second_archive);

        let temp_dir = tempfile::tempdir()?;
        let first_archive_path = temp_dir.path().join("first.whl");
        let second_archive_path = temp_dir.path().join("second.whl");
        fs_err::write(&first_archive_path, first_archive)?;
        fs_err::write(&second_archive_path, second_archive)?;

        let first_extract = temp_dir.path().join("first");
        let second_extract = temp_dir.path().join("second");
        fs_err::create_dir_all(&first_extract)?;
        fs_err::create_dir_all(&second_extract)?;

        let (first_files, first_digest) =
            unzip_and_hash(fs_err::File::open(&first_archive_path)?, &first_extract)?;
        let (second_files, second_digest) =
            unzip_and_hash(fs_err::File::open(&second_archive_path)?, &second_extract)?;

        assert_eq!(first_digest, second_digest);
        assert_eq!(first_files.len(), 2);
        assert_eq!(second_files.len(), 2);
        assert_eq!(
            fs_err::read(first_extract.join("example/__init__.py"))?,
            b"VALUE = 1\n"
        );
        assert_eq!(
            fs_err::read(second_extract.join("example/__init__.py"))?,
            b"VALUE = 1\n"
        );

        let stream_extract = temp_dir.path().join("stream");
        fs_err::create_dir_all(&stream_extract)?;
        let (stream_file_count, stream_digest) =
            stream_unzip_and_hash(&first_archive_path, &stream_extract)?;

        assert_eq!(first_digest, stream_digest);
        assert_eq!(stream_file_count, 2);
        assert_eq!(
            fs_err::read(stream_extract.join("example/__init__.py"))?,
            b"VALUE = 1\n"
        );

        Ok(())
    }

    #[test]
    fn parent_directory_entries_are_skipped() -> Result<(), Box<dyn std::error::Error>> {
        let parent_entries = [ZipEntry {
            path: "a/../b",
            contents: b"contents",
            mode: 0o100_644,
        }];
        let nested_entries = [ZipEntry {
            path: "a/b",
            contents: b"contents",
            mode: 0o100_644,
        }];

        let temp_dir = tempfile::tempdir()?;
        let parent_archive_path = temp_dir.path().join("parent.whl");
        let nested_archive_path = temp_dir.path().join("nested.whl");
        fs_err::write(
            &parent_archive_path,
            zip_archive(&parent_entries, b"parent archive comment"),
        )?;
        fs_err::write(
            &nested_archive_path,
            zip_archive(&nested_entries, b"nested archive comment"),
        )?;

        let parent_extract = temp_dir.path().join("parent");
        let nested_extract = temp_dir.path().join("nested");
        fs_err::create_dir_all(&parent_extract)?;
        fs_err::create_dir_all(&nested_extract)?;

        let (parent_files, parent_digest) =
            unzip_and_hash(fs_err::File::open(&parent_archive_path)?, &parent_extract)?;
        let (_nested_files, nested_digest) =
            unzip_and_hash(fs_err::File::open(&nested_archive_path)?, &nested_extract)?;

        assert!(parent_files.is_empty());
        assert_ne!(parent_digest, nested_digest);
        assert!(!parent_extract.join("a").exists());
        assert!(!parent_extract.join("b").exists());

        let stream_extract = temp_dir.path().join("stream-parent");
        fs_err::create_dir_all(&stream_extract)?;
        let (stream_file_count, stream_digest) =
            stream_unzip_and_hash(&parent_archive_path, &stream_extract)?;

        assert_eq!(stream_file_count, 0);
        assert_eq!(parent_digest, stream_digest);
        assert!(!stream_extract.join("a").exists());
        assert!(!stream_extract.join("b").exists());

        Ok(())
    }

    #[test]
    fn seekable_rejects_duplicate_output_paths() -> Result<(), Box<dyn std::error::Error>> {
        let first_entries = [
            ZipEntry {
                path: "example/data.txt",
                contents: b"first",
                mode: 0o100_644,
            },
            ZipEntry {
                path: "example/data.txt",
                contents: b"second",
                mode: 0o100_644,
            },
        ];
        let reversed_entries = [
            ZipEntry {
                path: "example/data.txt",
                contents: b"second",
                mode: 0o100_644,
            },
            ZipEntry {
                path: "example/data.txt",
                contents: b"first",
                mode: 0o100_644,
            },
        ];
        let aliased_entries = [
            ZipEntry {
                path: "example/data.txt",
                contents: b"first",
                mode: 0o100_644,
            },
            ZipEntry {
                path: "./example/data.txt",
                contents: b"second",
                mode: 0o100_644,
            },
        ];

        let temp_dir = tempfile::tempdir()?;
        for (name, entries) in [
            ("first", first_entries.as_slice()),
            ("reversed", reversed_entries.as_slice()),
            ("aliased", aliased_entries.as_slice()),
        ] {
            let archive_path = temp_dir.path().join(format!("{name}.whl"));
            fs_err::write(&archive_path, zip_archive(entries, b"archive comment"))?;
            let extract = temp_dir.path().join(name);
            fs_err::create_dir_all(&extract)?;

            let result = unzip_and_hash(fs_err::File::open(&archive_path)?, &extract);

            assert!(matches!(
                result,
                Err(crate::Error::DuplicateOutputPath { path })
                    if path == Path::new("example/data.txt")
            ));
            assert!(!extract.join("example/data.txt").exists());
        }

        Ok(())
    }

    #[test]
    fn seekable_rejects_existing_output_paths() -> Result<(), Box<dyn std::error::Error>> {
        let entries = [ZipEntry {
            path: "example/data.txt",
            contents: b"replacement",
            mode: 0o100_644,
        }];

        let temp_dir = tempfile::tempdir()?;
        let archive_path = temp_dir.path().join("archive.whl");
        fs_err::write(&archive_path, zip_archive(&entries, b"archive comment"))?;
        let extract = temp_dir.path().join("extract");
        fs_err::create_dir_all(extract.join("example"))?;
        fs_err::write(extract.join("example/data.txt"), b"existing")?;

        let result = unzip_and_hash(fs_err::File::open(&archive_path)?, &extract);

        assert!(result.is_err());
        assert_eq!(fs_err::read(extract.join("example/data.txt"))?, b"existing");

        Ok(())
    }

    #[test]
    fn directory_digest_includes_empty_leaf_directories() -> Result<(), Box<dyn std::error::Error>>
    {
        let base_entries = [
            ZipEntry {
                path: "example/__init__.py",
                contents: b"VALUE = 1\n",
                mode: 0o100_644,
            },
            ZipEntry {
                path: "example-1.0.0.dist-info/METADATA",
                contents: b"Name: example\nVersion: 1.0.0\n",
                mode: 0o100_644,
            },
        ];
        let explicit_parent_entries = [
            ZipEntry {
                path: "example/",
                contents: b"",
                mode: 0o040_755,
            },
            ZipEntry {
                path: "example/__init__.py",
                contents: b"VALUE = 1\n",
                mode: 0o100_644,
            },
            ZipEntry {
                path: "example-1.0.0.dist-info/METADATA",
                contents: b"Name: example\nVersion: 1.0.0\n",
                mode: 0o100_644,
            },
        ];
        let empty_directory_entries = [
            ZipEntry {
                path: "example/__init__.py",
                contents: b"VALUE = 1\n",
                mode: 0o100_644,
            },
            ZipEntry {
                path: "example/empty-data/",
                contents: b"",
                mode: 0o040_755,
            },
            ZipEntry {
                path: "example-1.0.0.dist-info/METADATA",
                contents: b"Name: example\nVersion: 1.0.0\n",
                mode: 0o100_644,
            },
        ];

        let temp_dir = tempfile::tempdir()?;
        let base_archive_path = temp_dir.path().join("base.whl");
        let explicit_parent_archive_path = temp_dir.path().join("explicit-parent.whl");
        let empty_directory_archive_path = temp_dir.path().join("empty-directory.whl");
        fs_err::write(
            &base_archive_path,
            zip_archive(&base_entries, b"base archive comment"),
        )?;
        fs_err::write(
            &explicit_parent_archive_path,
            zip_archive(&explicit_parent_entries, b"explicit parent archive comment"),
        )?;
        fs_err::write(
            &empty_directory_archive_path,
            zip_archive(&empty_directory_entries, b"empty directory archive comment"),
        )?;

        let base_extract = temp_dir.path().join("base");
        let explicit_parent_extract = temp_dir.path().join("explicit-parent");
        let empty_directory_extract = temp_dir.path().join("empty-directory");
        fs_err::create_dir_all(&base_extract)?;
        fs_err::create_dir_all(&explicit_parent_extract)?;
        fs_err::create_dir_all(&empty_directory_extract)?;

        let (_base_files, base_digest) =
            unzip_and_hash(fs_err::File::open(&base_archive_path)?, &base_extract)?;
        let (_explicit_parent_files, explicit_parent_digest) = unzip_and_hash(
            fs_err::File::open(&explicit_parent_archive_path)?,
            &explicit_parent_extract,
        )?;
        let (empty_directory_files, empty_directory_digest) = unzip_and_hash(
            fs_err::File::open(&empty_directory_archive_path)?,
            &empty_directory_extract,
        )?;

        assert_eq!(base_digest, explicit_parent_digest);
        assert_ne!(base_digest, empty_directory_digest);
        assert_eq!(empty_directory_files.len(), 2);
        assert!(empty_directory_extract.join("example/empty-data").is_dir());

        let stream_extract = temp_dir.path().join("stream-empty-directory");
        fs_err::create_dir_all(&stream_extract)?;
        let (stream_file_count, stream_digest) =
            stream_unzip_and_hash(&empty_directory_archive_path, &stream_extract)?;

        assert_eq!(empty_directory_digest, stream_digest);
        assert_eq!(stream_file_count, 2);
        assert!(stream_extract.join("example/empty-data").is_dir());

        Ok(())
    }

    #[test]
    fn directory_digest_hashes_zip_symlinks_as_regular_files()
    -> Result<(), Box<dyn std::error::Error>> {
        let symlink_entries = [ZipEntry {
            path: "example/link",
            contents: b"target.txt",
            mode: 0o120_777,
        }];
        let regular_file_entries = [ZipEntry {
            path: "example/link",
            contents: b"target.txt",
            mode: 0o100_777,
        }];

        let temp_dir = tempfile::tempdir()?;
        let symlink_archive_path = temp_dir.path().join("symlink.whl");
        let regular_file_archive_path = temp_dir.path().join("regular.whl");
        fs_err::write(
            &symlink_archive_path,
            zip_archive(&symlink_entries, b"symlink archive comment"),
        )?;
        fs_err::write(
            &regular_file_archive_path,
            zip_archive(&regular_file_entries, b"regular file archive comment"),
        )?;

        let symlink_extract = temp_dir.path().join("symlink");
        let regular_file_extract = temp_dir.path().join("regular");
        fs_err::create_dir_all(&symlink_extract)?;
        fs_err::create_dir_all(&regular_file_extract)?;

        let (_symlink_files, symlink_digest) =
            unzip_and_hash(fs_err::File::open(&symlink_archive_path)?, &symlink_extract)?;
        let (_regular_file_files, regular_file_digest) = unzip_and_hash(
            fs_err::File::open(&regular_file_archive_path)?,
            &regular_file_extract,
        )?;

        assert_eq!(symlink_digest, regular_file_digest);
        assert!(fs_err::symlink_metadata(symlink_extract.join("example/link"))?.is_file());
        assert_eq!(
            fs_err::read(symlink_extract.join("example/link"))?,
            b"target.txt"
        );

        let stream_extract = temp_dir.path().join("stream-symlink");
        fs_err::create_dir_all(&stream_extract)?;
        let (stream_file_count, stream_digest) =
            stream_unzip_and_hash(&symlink_archive_path, &stream_extract)?;

        assert_eq!(stream_file_count, 1);
        assert_eq!(symlink_digest, stream_digest);
        assert!(fs_err::symlink_metadata(stream_extract.join("example/link"))?.is_file());

        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn stored_entry_digest_uses_opened_archive_handle() -> Result<(), Box<dyn std::error::Error>> {
        let original_entries = [ZipEntry {
            path: "example/data.txt",
            contents: b"original-data",
            mode: 0o100_644,
        }];
        let replacement_entries = [ZipEntry {
            path: "example/data.txt",
            contents: b"replaced-data",
            mode: 0o100_644,
        }];
        let original_archive = zip_archive(&original_entries, b"original archive comment");
        let replacement_archive = zip_archive(&replacement_entries, b"replacement archive comment");

        let temp_dir = tempfile::tempdir()?;
        let expected_archive_path = temp_dir.path().join("expected.whl");
        fs_err::write(&expected_archive_path, &original_archive)?;
        let expected_extract = temp_dir.path().join("expected");
        fs_err::create_dir_all(&expected_extract)?;
        let (_expected_files, expected_digest) = unzip_and_hash(
            fs_err::File::open(&expected_archive_path)?,
            &expected_extract,
        )?;

        let archive_path = temp_dir.path().join("replaced.whl");
        fs_err::write(&archive_path, &original_archive)?;
        let opened_archive = fs_err::File::open(&archive_path)?;
        fs_err::remove_file(&archive_path)?;
        fs_err::write(&archive_path, replacement_archive)?;

        let extract = temp_dir.path().join("extract");
        fs_err::create_dir_all(&extract)?;
        let (_files, digest) = unzip_and_hash(opened_archive, &extract)?;

        assert_eq!(digest, expected_digest);
        assert_eq!(
            fs_err::read(extract.join("example/data.txt"))?,
            b"original-data"
        );

        Ok(())
    }

    #[test]
    fn stored_entry_digest_handles_local_extra_fields() -> Result<(), Box<dyn std::error::Error>> {
        let entries = [ZipEntry {
            path: "example/data.txt",
            contents: b"stored-data-with-local-extra-field",
            mode: 0o100_644,
        }];
        let archive = zip_archive_with_local_extra(
            &entries,
            &[
                0xef, 0xbe, // Header ID.
                0x04, 0x00, // Data size.
                b'u', b'v', b'x', b'\0',
            ],
            b"local extra field archive comment",
        );

        let temp_dir = tempfile::tempdir()?;
        let archive_path = temp_dir.path().join("local-extra.whl");
        fs_err::write(&archive_path, archive)?;
        let extract = temp_dir.path().join("extract");
        fs_err::create_dir_all(&extract)?;

        let (files, _digest) = unzip_and_hash(fs_err::File::open(&archive_path)?, &extract)?;

        assert_eq!(files.len(), 1);
        assert_eq!(
            fs_err::read(extract.join("example/data.txt"))?,
            b"stored-data-with-local-extra-field"
        );

        Ok(())
    }

    #[test]
    fn seekable_rejects_directory_entries_with_payload() -> Result<(), Box<dyn std::error::Error>> {
        let entries = [ZipEntry {
            path: "example/not-empty/",
            contents: b"payload",
            mode: 0o040_755,
        }];

        let temp_dir = tempfile::tempdir()?;
        let archive_path = temp_dir.path().join("directory-payload.whl");
        fs_err::write(
            &archive_path,
            zip_archive(&entries, b"directory payload archive comment"),
        )?;
        let extract = temp_dir.path().join("extract");
        fs_err::create_dir_all(&extract)?;

        let result = unzip_and_hash(fs_err::File::open(&archive_path)?, &extract);

        assert!(matches!(
            result,
            Err(crate::Error::BadCrc32 {
                computed: 0,
                expected,
                ..
            }) if expected == crc32(b"payload")
        ));

        Ok(())
    }

    fn stream_unzip_and_hash(
        archive: &Path,
        target: &Path,
    ) -> Result<(usize, DirectoryDigest), Box<dyn std::error::Error>> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        runtime.block_on(async {
            let file = fs_err::tokio::File::open(archive).await?;
            let (files, digest) =
                crate::stream::unzip_and_hash(archive.display(), file, target).await?;
            Ok::<_, Box<dyn std::error::Error>>((files.len(), digest))
        })
    }

    fn zip_archive(entries: &[ZipEntry<'_>], comment: &[u8]) -> Vec<u8> {
        zip_archive_with_local_extra(entries, b"", comment)
    }

    /// Build a stored ZIP archive whose local file headers include `local_extra`.
    fn zip_archive_with_local_extra(
        entries: &[ZipEntry<'_>],
        local_extra: &[u8],
        comment: &[u8],
    ) -> Vec<u8> {
        let mut archive = Vec::new();
        let mut central_directory_entries = Vec::new();

        for entry in entries {
            let local_header_offset =
                u32::try_from(archive.len()).expect("test archive offset fits in u32");
            let crc32 = crc32(entry.contents);
            write_local_file_header(&mut archive, entry, crc32, local_extra);
            archive.extend_from_slice(entry.path.as_bytes());
            archive.extend_from_slice(local_extra);
            archive.extend_from_slice(entry.contents);
            central_directory_entries.push((entry, crc32, local_header_offset));
        }

        let central_directory_offset =
            u32::try_from(archive.len()).expect("test archive offset fits in u32");
        for (entry, crc32, local_header_offset) in central_directory_entries {
            write_central_directory_header(&mut archive, entry, crc32, local_header_offset);
            archive.extend_from_slice(entry.path.as_bytes());
        }
        let central_directory_size = u32::try_from(
            archive.len() - usize::try_from(central_directory_offset).expect("offset fits usize"),
        )
        .expect("test central directory size fits in u32");

        write_end_of_central_directory(
            &mut archive,
            entries.len(),
            central_directory_size,
            central_directory_offset,
            comment,
        );
        archive
    }

    fn write_local_file_header(
        archive: &mut Vec<u8>,
        entry: &ZipEntry<'_>,
        crc32: u32,
        local_extra: &[u8],
    ) {
        write_u32(archive, 0x0403_4b50);
        write_u16(archive, 20);
        write_u16(archive, 0);
        write_u16(archive, 0);
        write_u16(archive, 0);
        write_u16(archive, 0);
        write_u32(archive, crc32);
        write_u32(
            archive,
            u32::try_from(entry.contents.len()).expect("test file size fits in u32"),
        );
        write_u32(
            archive,
            u32::try_from(entry.contents.len()).expect("test file size fits in u32"),
        );
        write_u16(
            archive,
            u16::try_from(entry.path.len()).expect("test path length fits in u16"),
        );
        write_u16(
            archive,
            u16::try_from(local_extra.len()).expect("test extra field length fits in u16"),
        );
    }

    fn write_central_directory_header(
        archive: &mut Vec<u8>,
        entry: &ZipEntry<'_>,
        crc32: u32,
        local_header_offset: u32,
    ) {
        write_u32(archive, 0x0201_4b50);
        write_u16(archive, (0x03 << 8) | 0x14);
        write_u16(archive, 20);
        write_u16(archive, 0);
        write_u16(archive, 0);
        write_u16(archive, 0);
        write_u16(archive, 0);
        write_u32(archive, crc32);
        write_u32(
            archive,
            u32::try_from(entry.contents.len()).expect("test file size fits in u32"),
        );
        write_u32(
            archive,
            u32::try_from(entry.contents.len()).expect("test file size fits in u32"),
        );
        write_u16(
            archive,
            u16::try_from(entry.path.len()).expect("test path length fits in u16"),
        );
        write_u16(archive, 0);
        write_u16(archive, 0);
        write_u16(archive, 0);
        write_u16(archive, 0);
        write_u32(archive, entry.mode << 16);
        write_u32(archive, local_header_offset);
    }

    fn write_end_of_central_directory(
        archive: &mut Vec<u8>,
        entry_count: usize,
        central_directory_size: u32,
        central_directory_offset: u32,
        comment: &[u8],
    ) {
        let entry_count = u16::try_from(entry_count).expect("test entry count fits in u16");
        write_u32(archive, 0x0605_4b50);
        write_u16(archive, 0);
        write_u16(archive, 0);
        write_u16(archive, entry_count);
        write_u16(archive, entry_count);
        write_u32(archive, central_directory_size);
        write_u32(archive, central_directory_offset);
        write_u16(
            archive,
            u16::try_from(comment.len()).expect("test comment length fits in u16"),
        );
        archive.extend_from_slice(comment);
    }

    fn write_u16(archive: &mut Vec<u8>, value: u16) {
        archive.extend_from_slice(&value.to_le_bytes());
    }

    fn write_u32(archive: &mut Vec<u8>, value: u32) {
        archive.extend_from_slice(&value.to_le_bytes());
    }

    fn crc32(bytes: &[u8]) -> u32 {
        let mut crc = 0xffff_ffff;
        for byte in bytes {
            crc ^= u32::from(*byte);
            for _ in 0..8 {
                if crc & 1 == 1 {
                    crc = (crc >> 1) ^ 0xedb8_8320;
                } else {
                    crc >>= 1;
                }
            }
        }
        !crc
    }
}
