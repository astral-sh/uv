use std::path::{Path, PathBuf};
use std::sync::Mutex;

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

/// Unzip a `.zip` archive into the target directory.
///
/// Returns the list of unpacked files and their sizes.
pub fn unzip(reader: fs_err::File, target: &Path) -> Result<Vec<(PathBuf, u64)>, Error> {
    let (reader, filename) = reader.into_parts();

    // Parse the central directory once, then clone the archive reader per Rayon worker so
    // extraction stays parallel for already-downloaded wheels.
    let archive = block_on(ZipFileReader::new(AllowStdIo::new(
        CloneableSeekableReader::new(reader),
    )))?;
    let directories = Mutex::new(FxHashSet::default());
    let skip_validation = insecure_no_validate();
    // Initialize the threadpool with the user settings.
    initialize_rayon_once();
    (0..archive.file().entries().len())
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
                    filename = filename.display(),
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
                let mut directories = directories.lock().map_err(|_| {
                    Error::Io(std::io::Error::other(
                        "ZIP extraction directory tracker mutex was poisoned",
                    ))
                })?;
                if directories.insert(path.clone()) {
                    fs_err::create_dir_all(path).map_err(Error::Io)?;
                }
                return Ok(None);
            }

            if let Some(parent) = path.parent() {
                let mut directories = directories.lock().map_err(|_| {
                    Error::Io(std::io::Error::other(
                        "ZIP extraction directory tracker mutex was poisoned",
                    ))
                })?;
                if directories.insert(parent.to_path_buf()) {
                    fs_err::create_dir_all(parent).map_err(Error::Io)?;
                }
            }

            // Copy the file contents.
            let outfile = fs_err::File::create(&path).map_err(Error::Io)?;
            let size = entry.uncompressed_size();
            let writer = if let Ok(size) = usize::try_from(size) {
                std::io::BufWriter::with_capacity(std::cmp::min(size, 1024 * 1024), outfile)
            } else {
                std::io::BufWriter::new(outfile)
            };
            let (copied, computed_crc32) = block_on(async {
                let mut file = archive.reader_with_entry(file_number).await?;
                let mut writer = AllowStdIo::new(writer);
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
                    writer.write_all(&buffer[..read]).await.map_err(Error::Io)?;
                    copied += read as u64;
                }
                writer.flush().await.map_err(Error::Io)?;
                Ok::<_, Error>((copied, file.compute_hash()))
            })?;

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

                if let Some(mode) = entry.unix_permissions() {
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

            Ok(Some((enclosed_name, size)))
        })
        // Filter out directories and skipped dangerous paths, we only want to collect the files.
        .filter_map(Result::transpose)
        .collect::<Result<_, Error>>()
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
