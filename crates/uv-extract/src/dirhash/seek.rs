//! Directory hashing while extracting seekable ZIP archives.

use std::path::{Path, PathBuf};
use std::pin::pin;
use std::sync::Mutex;

use crate::vendor::CloneableSeekableReader;
use crate::{Error, insecure_no_validate};
use async_zip::StoredZipEntry;
use async_zip::base::read::seek::ZipFileReader;
use async_zip::error::ZipError;
use futures::executor::block_on;
use futures::io::{AllowStdIo, AsyncReadExt, AsyncWriteExt};
use rayon::prelude::*;
use rustc_hash::FxHashSet;
use tokio_util::compat::{FuturesAsyncReadCompatExt, FuturesAsyncWriteCompatExt};
use tracing::warn;
use uv_configuration::initialize_rayon_once;

use super::{
    DirhashTree, HashedFile, UnhashedFile, UnzipOutput, blake3_copy, directory_tree_from_extracted,
};
use crate::archive_path::SanitizedArchivePath;

/// A successfully extracted file, or an explicit directory that can affect the digest.
enum ExtractedEntry {
    File {
        path: SanitizedArchivePath,
        size: u64,
        digest: Option<blake3::Hash>,
        executable: bool,
    },
    Directory(SanitizedArchivePath),
}

/// Unzip a `.zip` archive into the target directory.
pub(crate) fn unzip(reader: fs_err::File, target: &Path) -> Result<Vec<UnhashedFile>, Error> {
    let UnzipOutput::Unhashed(files) = unzip_inner(reader, target, false)? else {
        return Err(Error::Io(std::io::Error::other(
            "seekable ZIP hash tree was unexpectedly computed",
        )));
    };
    Ok(files)
}

/// Unzip a `.zip` archive into the target directory while computing a hash tree of the extracted
/// files.
///
/// Returns the list of unpacked files and their sizes, along with a hash tree containing the
/// canonicalized extracted file paths, contents, and empty directories.
pub(crate) fn unzip_and_hash(
    reader: fs_err::File,
    target: &Path,
) -> Result<(Vec<HashedFile>, DirhashTree), Error> {
    let UnzipOutput::Hashed { files, tree } = unzip_inner(reader, target, true)? else {
        return Err(Error::Io(std::io::Error::other(
            "seekable ZIP hash tree was not computed",
        )));
    };
    Ok((files, tree))
}

fn unzip_inner(
    reader: fs_err::File,
    target: &Path,
    hash_contents: bool,
) -> Result<UnzipOutput, Error> {
    let (reader, _) = reader.into_parts();

    // Parse the central directory once, then clone the archive reader per Rayon worker so
    // extraction stays parallel for already-downloaded wheels. AllowStdIo adapts synchronous
    // file I/O to async_zip; extraction itself runs on blocking and Rayon threads.
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
            hash_contents,
        )
    };

    if !hash_contents {
        let files = (0..archive.file().entries().len())
            .into_par_iter()
            .map(extract)
            .filter_map(|result| match result {
                Ok(Some(ExtractedEntry::File { path, size, .. })) => {
                    Some(Ok(UnhashedFile::new(path.into_path_buf(), size)))
                }
                Ok(Some(ExtractedEntry::Directory(_)) | None) => None,
                Err(err) => Some(Err(err)),
            })
            .collect::<Result<_, Error>>()?;
        return Ok(UnzipOutput::Unhashed(files));
    }

    let extracted = (0..archive.file().entries().len())
        .into_par_iter()
        .map(extract)
        // Filter out skipped dangerous paths, then collect files and directory candidates.
        .filter_map(Result::transpose)
        .collect::<Result<Vec<_>, Error>>()?;

    let mut hashed_files = Vec::with_capacity(extracted.len());
    let mut digest_directories = FxHashSet::default();
    for extracted in extracted {
        match extracted {
            ExtractedEntry::File {
                path,
                size,
                digest,
                executable,
            } => {
                if let Some(digest) = digest {
                    hashed_files.push(HashedFile::new(path, size, digest, executable));
                }
            }
            ExtractedEntry::Directory(path) => {
                digest_directories.insert(path);
            }
        }
    }
    let tree = directory_tree_from_extracted(&hashed_files, &digest_directories)?;
    Ok(UnzipOutput::Hashed {
        files: hashed_files,
        tree,
    })
}

/// Reject entries that would write to the same sanitized output path.
///
/// Duplicate paths can otherwise race to determine which contents are persisted or hashed.
fn validate_unique_output_paths(entries: &[StoredZipEntry]) -> Result<(), Error> {
    let mut paths = FxHashSet::default();
    for (file_number, entry) in entries.iter().enumerate() {
        let file_name = entry_file_name(entry, file_number)?;
        let Ok(Some(path)) = SanitizedArchivePath::from_archive_member(file_name) else {
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
    hash_contents: bool,
) -> Result<Option<ExtractedEntry>, Error>
where
    R: std::io::BufRead + std::io::Seek + Unpin,
{
    let entry = archive.file().entries()[file_number].clone();
    let file_name = entry_file_name(&entry, file_number)?;
    let enclosed_name = match SanitizedArchivePath::from_archive_member(file_name) {
        Ok(path) => path,
        Err(_) if skip_validation => None,
        Err(err) => return Err(err),
    };
    let Some(enclosed_name) = enclosed_name else {
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
    skip_validation: bool,
    hash_contents: bool,
) -> Result<ExtractedEntry, Error>
where
    R: std::io::BufRead + std::io::Seek + Unpin,
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
    let writer = buffered_file_writer(outfile, size);

    // Keep the hashing state out of ordinary extraction, and pin both futures here to avoid
    // moving their large state into `block_on`.
    let (copied, computed_crc32, digest) = if hash_contents {
        let (copied, computed_crc32, digest) =
            block_on(pin!(copy_and_hash_entry(archive, file_number, writer)))?;
        (copied, computed_crc32, Some(digest))
    } else {
        let (copied, computed_crc32) = block_on(pin!(copy_entry(archive, file_number, writer)))?;
        (copied, computed_crc32, None)
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
    preserve_executable_bit(path, entry.unix_permissions())?;

    Ok(ExtractedEntry::File {
        path: enclosed_name,
        size,
        digest,
        executable: entry
            .unix_permissions()
            .is_some_and(|mode| mode & 0o111 != 0),
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

/// Copy an entry without computing a content digest.
async fn copy_entry<R>(
    archive: &mut ZipFileReader<AllowStdIo<R>>,
    file_number: usize,
    writer: std::io::BufWriter<fs_err::File>,
) -> Result<(u64, u32), Error>
where
    R: std::io::BufRead + std::io::Seek + Unpin,
{
    let mut file = archive.reader_with_entry(file_number).await?;
    let mut writer = AllowStdIo::new(writer);

    let mut copied = 0;
    let mut buffer = vec![0; 128 * 1024];
    loop {
        let read = file.read(&mut buffer).await.map_err(Error::io_or_zip)?;
        if read == 0 {
            break;
        }
        writer.write_all(&buffer[..read]).await.map_err(Error::Io)?;
        copied += read as u64;
    }
    writer.flush().await.map_err(Error::Io)?;
    Ok((copied, file.compute_hash()))
}

/// Copy an entry while hashing the same uncompressed bytes written to disk.
async fn copy_and_hash_entry<R>(
    archive: &mut ZipFileReader<AllowStdIo<R>>,
    file_number: usize,
    writer: std::io::BufWriter<fs_err::File>,
) -> Result<(u64, u32, blake3::Hash), Error>
where
    R: std::io::BufRead + std::io::Seek + Unpin,
{
    let mut file = archive.reader_with_entry(file_number).await?;
    let mut writer = AllowStdIo::new(writer);
    let (copied, digest) = blake3_copy((&mut file).compat(), (&mut writer).compat_write())
        .await
        .map_err(Error::io_or_zip)?;
    Ok((copied, file.compute_hash(), digest))
}
