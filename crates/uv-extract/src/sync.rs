use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock, mpsc};

use crate::hash::{DirectoryDigest, DirectoryDigestFile, directory_digest};
use crate::vendor::CloneableSeekableReader;
use crate::{CompressionMethod, Error, insecure_no_validate, validate_archive_member_name};
use async_zip::base::read::seek::ZipFileReader;
use async_zip::error::ZipError;
use futures::executor::block_on;
use futures::io::{AllowStdIo, AsyncReadExt, AsyncWriteExt};
use rayon::prelude::*;
use rustc_hash::FxHashSet;
use tracing::warn;
use uv_configuration::initialize_rayon_once;
use uv_warnings::warn_user_once;

const LOCAL_FILE_HEADER_LENGTH: u64 = 30;
const LOCAL_FILE_HEADER_LENGTH_USIZE: usize = 30;
const LOCAL_FILE_HEADER_SIGNATURE: u32 = 0x0403_4b50;
#[cfg(not(test))]
const STORED_HASH_FAST_PATH_THRESHOLD: u64 = 8 * 1024 * 1024;
#[cfg(test)]
const STORED_HASH_FAST_PATH_THRESHOLD: u64 = 1;
const STORED_HASH_BUFFER_SIZE: usize = 16 * 1024 * 1024;
const PARALLEL_HASH_THRESHOLD: u64 = 8 * 1024 * 1024;
const PARALLEL_HASH_BUFFER_SIZE: usize = 16 * 1024 * 1024;
static HASH_THREAD_POOL: OnceLock<Option<rayon::ThreadPool>> = OnceLock::new();

/// Unzip a `.zip` archive into the target directory.
///
/// Returns the list of unpacked files and their sizes.
pub fn unzip(reader: fs_err::File, target: &Path) -> Result<Vec<(PathBuf, u64)>, Error> {
    let (files, _digest) = unzip_and_hash(reader, target)?;
    Ok(files)
}

/// Unzip a `.zip` archive into the target directory while computing a digest of the extracted files.
///
/// Returns the list of unpacked files and their sizes, along with a digest over the canonicalized
/// extracted file paths, executable bits, sizes, and contents.
pub fn unzip_and_hash(
    reader: fs_err::File,
    target: &Path,
) -> Result<(Vec<(PathBuf, u64)>, DirectoryDigest), Error> {
    let (reader, filename) = reader.into_parts();

    // Parse the central directory once, then clone the archive reader per Rayon worker so
    // extraction stays parallel for already-downloaded wheels.
    let archive = block_on(ZipFileReader::new(AllowStdIo::new(
        CloneableSeekableReader::new(reader),
    )))?;
    unzip_archive(&archive, &filename, target)
}

fn unzip_archive<R>(
    archive: &ZipFileReader<AllowStdIo<R>>,
    filename: &Path,
    target: &Path,
) -> Result<(Vec<(PathBuf, u64)>, DirectoryDigest), Error>
where
    R: std::io::BufRead + std::io::Seek + Clone + Send + Sync + Unpin,
    AllowStdIo<R>: Clone,
{
    let directories = Mutex::new(FxHashSet::default());
    let skip_validation = insecure_no_validate();
    let filename_display = filename.display().to_string();
    // Initialize the threadpool with the user settings.
    initialize_rayon_once();
    let archive = (*archive).clone();
    let extracted = (0..archive.file().entries().len())
        .into_par_iter()
        .map(|file_number| {
            let mut archive = archive.clone();
            let entry = archive.file().entries()[file_number].clone();
            let file_name = match entry.filename().as_str() {
                Ok(file_name) => file_name,
                Err(ZipError::StringNotUtf8) => {
                    return Err(Error::CentralDirectoryEntryNotUtf8 {
                        index: file_number as u64,
                    });
                }
                Err(err) => return Err(err.into()),
            };

            let compression = CompressionMethod::from(entry.compression());
            if !compression.is_well_known() {
                warn_user_once!(
                    "One or more file entries in '{filename}' use the '{compression}' compression method, which is not widely supported. A future version of uv will reject ZIP archives containing entries compressed with this method. Entries must be compressed with the '{stored}', '{deflate}', or '{zstd}' compression methods.",
                    filename = filename_display,
                    stored = CompressionMethod::Stored,
                    deflate = CompressionMethod::Deflated,
                    zstd = CompressionMethod::Zstd,
                );
            }

            if let Err(e) = validate_archive_member_name(file_name) {
                if !skip_validation {
                    return Err(e);
                }
            }

            // Determine the path of the file within the wheel.
            let Some(enclosed_name) = crate::stream::enclosed_name(file_name) else {
                warn!("Skipping unsafe file name: {file_name}");
                return Ok(None);
            };

            // Create necessary parent directories.
            let path = target.join(&enclosed_name);
            if entry.dir()? {
                let mut directories = directories.lock().unwrap();
                if directories.insert(path.clone()) {
                    fs_err::create_dir_all(path).map_err(Error::Io)?;
                }
                return Ok(None);
            }

            if let Some(parent) = path.parent() {
                let mut directories = directories.lock().unwrap();
                if directories.insert(parent.to_path_buf()) {
                    fs_err::create_dir_all(parent).map_err(Error::Io)?;
                }
            }

            // Copy the file contents.
            let outfile = fs_err::File::create(&path).map_err(Error::Io)?;
            let size = entry.uncompressed_size();
            let unix_permissions = entry.unix_permissions();
            let executable = unix_permissions.is_some_and(|mode| mode & 0o111 != 0);
            let writer = if let Ok(size) = usize::try_from(size) {
                std::io::BufWriter::with_capacity(std::cmp::min(size, 1024 * 1024), outfile)
            } else {
                std::io::BufWriter::new(outfile)
            };
            let use_stored_hash_fast_path = matches!(compression, CompressionMethod::Stored)
                && size >= STORED_HASH_FAST_PATH_THRESHOLD
                && entry.compressed_size() == size;
            let (copied, computed_crc32, digest) = if use_stored_hash_fast_path {
                let (copied, stored_digest) = std::thread::scope(|scope| {
                    let stored_digest =
                        scope.spawn(|| hash_stored_entry(filename, entry.header_offset(), size));
                    let copied = block_on(copy_entry(&mut archive, file_number, writer, false));
                    (copied, stored_digest.join())
                });
                let (copied, computed_crc32, digest) = copied?;
                debug_assert!(digest.is_none());
                let stored_digest = stored_digest.map_err(|_| thread_panic_error())??;
                (copied, computed_crc32, stored_digest)
            } else if size >= PARALLEL_HASH_THRESHOLD {
                copy_entry_with_hash_thread(&mut archive, file_number, writer)?
            } else {
                let (copied, computed_crc32, digest) = block_on(copy_entry(
                    &mut archive,
                    file_number,
                    writer,
                    true,
                ))?;
                let Some(digest) = digest else {
                    return Err(Error::Io(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "missing digest for ZIP entry",
                    )));
                };
                (copied, computed_crc32, digest)
            };

            if copied != size && !skip_validation {
                return Err(Error::BadUncompressedSize {
                    path: enclosed_name.clone(),
                    computed: copied,
                    expected: size,
                });
            }

            if computed_crc32 != entry.crc32() && !skip_validation {
                return Err(Error::BadCrc32 {
                    path: enclosed_name.clone(),
                    computed: computed_crc32,
                    expected: entry.crc32(),
                });
            }

            // See `uv_extract::stream::unzip`. For simplicity, this is identical with the code there except for being
            // sync.
            #[cfg(unix)]
            {
                use std::fs::Permissions;
                use std::os::unix::fs::PermissionsExt;

                if let Some(mode) = unix_permissions {
                    // https://github.com/pypa/pip/blob/3898741e29b7279e7bffe044ecfbe20f6a438b1e/src/pip/_internal/utils/unpacking.py#L88-L100
                    let has_any_executable_bit = mode & 0o111;
                    if has_any_executable_bit != 0 {
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

            let hash_file = DirectoryDigestFile::new(&enclosed_name, size, executable, digest);
            Ok(Some(((enclosed_name, size), hash_file)))
        })
        // Filter out directories and skipped dangerous paths, we only want to collect the files.
        .filter_map(Result::transpose)
        .collect::<Result<Vec<_>, Error>>()?;

    let mut files = Vec::with_capacity(extracted.len());
    let mut hash_files = Vec::with_capacity(extracted.len());
    for (file, hash_file) in extracted {
        files.push(file);
        hash_files.push(hash_file);
    }

    Ok((files, directory_digest(hash_files)))
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

fn copy_entry_with_hash_thread<R>(
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

fn hash_stored_entry(
    filename: &Path,
    header_offset: u64,
    compressed_size: u64,
) -> Result<blake3::Hash, Error> {
    let mut file = fs_err::File::open(filename).map_err(Error::Io)?;
    let data_offset = stored_entry_data_offset(&mut file, header_offset)?;
    file.seek(SeekFrom::Start(data_offset)).map_err(Error::Io)?;

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
        file.read_exact(&mut buffer[..read_size])
            .map_err(Error::Io)?;
        update_hash_rayon(&mut hasher, &buffer[..read_size]);
        remaining -= read_size as u64;
    }

    Ok(hasher.finalize())
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

fn stored_entry_data_offset(file: &mut fs_err::File, header_offset: u64) -> Result<u64, Error> {
    file.seek(SeekFrom::Start(header_offset))
        .map_err(Error::Io)?;
    let mut header = [0; LOCAL_FILE_HEADER_LENGTH_USIZE];
    file.read_exact(&mut header).map_err(Error::Io)?;

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

/// Extract the top-level directory from an unpacked archive.
///
/// The specification says:
/// > A .tar.gz source distribution (sdist) contains a single top-level directory called
/// > `{name}-{version}` (e.g. foo-1.0), containing the source files of the package.
///
/// This function returns the path to that top-level directory.
pub fn strip_component(source: impl AsRef<Path>) -> Result<PathBuf, Error> {
    // TODO(konstin): Verify the name of the directory.
    let top_level = fs_err::read_dir(source.as_ref())
        .map_err(Error::Io)?
        .collect::<std::io::Result<Vec<fs_err::DirEntry>>>()
        .map_err(Error::Io)?;
    match top_level.as_slice() {
        [root] => Ok(root.path()),
        [] => Err(Error::EmptyArchive),
        _ => Err(Error::NonSingularArchive(
            top_level
                .into_iter()
                .map(|entry| entry.file_name())
                .collect(),
        )),
    }
}

#[cfg(test)]
mod tests {
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
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let (stream_files, stream_digest) = runtime.block_on(async {
            let file = fs_err::tokio::File::open(&first_archive_path).await?;
            let result = crate::stream::unzip_and_hash("first.whl", file, &stream_extract).await?;
            Ok::<_, Box<dyn std::error::Error>>(result)
        })?;

        assert_eq!(first_digest, stream_digest);
        assert_eq!(stream_files.len(), 2);
        assert_eq!(
            fs_err::read(stream_extract.join("example/__init__.py"))?,
            b"VALUE = 1\n"
        );

        Ok(())
    }

    fn zip_archive(entries: &[ZipEntry<'_>], comment: &[u8]) -> Vec<u8> {
        let mut archive = Vec::new();
        let mut central_directory_entries = Vec::new();

        for entry in entries {
            let local_header_offset =
                u32::try_from(archive.len()).expect("test archive offset fits in u32");
            let crc32 = crc32(entry.contents);
            write_local_file_header(&mut archive, entry, crc32);
            archive.extend_from_slice(entry.path.as_bytes());
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

    fn write_local_file_header(archive: &mut Vec<u8>, entry: &ZipEntry<'_>, crc32: u32) {
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
        write_u16(archive, 0);
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
