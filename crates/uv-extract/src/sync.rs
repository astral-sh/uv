use std::path::Path;
use std::sync::{LazyLock, Mutex};

use rayon::prelude::*;
use rustc_hash::FxHashSet;
use tracing::warn;
use uv_configuration::RAYON_INITIALIZE;
use uv_warnings::warn_user_once;
use zip::ZipArchive;

use crate::hash::Blake3Digest;
use crate::vendor::CloneableSeekableReader;
use crate::{CompressionMethod, Error, insecure_no_validate, validate_archive_member_name};

/// Unzip a `.zip` archive into the target directory, returning the blake3 hash of the archive.
///
/// The blake3 hash is computed using multi-threaded, memory-mapped I/O on the source file, while
/// extraction proceeds in parallel using rayon.
pub fn unzip(path: &Path, target: &Path) -> Result<Blake3Digest, Error> {
    // Compute the blake3 hash using multi-threaded, memory-mapped I/O.
    let blake3_digest = blake3_hash(path)?;

    // Open the file for extraction.
    let file = fs_err::File::open(path).map_err(Error::Io)?;
    let reader = std::io::BufReader::new(file);
    let archive = ZipArchive::new(CloneableSeekableReader::new(reader))?;
    let directories = Mutex::new(FxHashSet::default());
    let skip_validation = insecure_no_validate();

    // Initialize the rayon threadpool with the user settings.
    LazyLock::force(&RAYON_INITIALIZE);

    // Extract all entries in parallel. Each rayon task clones the archive to get an independent
    // seek position via `CloneableSeekableReader`.
    (0..archive.len())
        .into_par_iter()
        .map(|file_number| {
            let mut archive = archive.clone();
            let mut file = archive.by_index(file_number)?;

            let compression = CompressionMethod::from(file.compression());
            if !compression.is_well_known() {
                warn_user_once!(
                    "One or more file entries in '{filename}' use the '{compression}' compression method, which is not widely supported. A future version of uv will reject ZIP archives containing entries compressed with this method. Entries must be compressed with the '{stored}', '{deflate}', or '{zstd}' compression methods.",
                    filename = path.display(),
                    stored = CompressionMethod::Stored,
                    deflate = CompressionMethod::Deflated,
                    zstd = CompressionMethod::Zstd,
                );
            }

            if let Err(err) = validate_archive_member_name(file.name()) {
                if !skip_validation {
                    return Err(err);
                }
            }

            // Determine the path of the file within the wheel.
            let Some(enclosed_name) = file.enclosed_name() else {
                warn!("Skipping unsafe file name: {}", file.name());
                return Ok(());
            };

            // Create necessary parent directories.
            let entry_path = target.join(enclosed_name);
            if file.is_dir() {
                let mut directories = directories.lock().unwrap();
                if directories.insert(entry_path.clone()) {
                    fs_err::create_dir_all(entry_path).map_err(Error::Io)?;
                }
                return Ok(());
            }

            if let Some(parent) = entry_path.parent() {
                let mut directories = directories.lock().unwrap();
                if directories.insert(parent.to_path_buf()) {
                    fs_err::create_dir_all(parent).map_err(Error::Io)?;
                }
            }

            // Copy the file contents.
            let outfile = fs_err::File::create(&entry_path).map_err(Error::Io)?;
            let size = file.size();
            if size > 0 {
                let mut writer = if let Ok(size) = usize::try_from(size) {
                    std::io::BufWriter::with_capacity(std::cmp::min(size, 1024 * 1024), outfile)
                } else {
                    std::io::BufWriter::new(outfile)
                };
                std::io::copy(&mut file, &mut writer).map_err(Error::io_or_compression)?;
            }

            // Preserve executable permissions on Unix.
            #[cfg(unix)]
            {
                use std::fs::Permissions;
                use std::os::unix::fs::PermissionsExt;

                if let Some(mode) = file.unix_mode() {
                    let has_any_executable_bit = mode & 0o111;
                    if has_any_executable_bit != 0 {
                        let permissions =
                            fs_err::metadata(&entry_path).map_err(Error::Io)?.permissions();
                        if permissions.mode() & 0o111 != 0o111 {
                            fs_err::set_permissions(
                                &entry_path,
                                Permissions::from_mode(permissions.mode() | 0o111),
                            )
                            .map_err(Error::Io)?;
                        }
                    }
                }
            }

            Ok(())
        })
        .collect::<Result<Vec<_>, Error>>()?;

    Ok(blake3_digest)
}

/// Compute the blake3 hash of a file using multi-threaded, memory-mapped I/O.
#[expect(unsafe_code)]
fn blake3_hash(path: &Path) -> Result<Blake3Digest, Error> {
    let file = fs_err::File::open(path).map_err(Error::Io)?;
    let metadata = file.metadata().map_err(Error::Io)?;

    // For small files, read directly instead of mmap.
    if metadata.len() < 128 * 1024 {
        let data = fs_err::read(path).map_err(Error::Io)?;
        let hash = blake3::hash(&data);
        return Ok(Blake3Digest::new(hash.to_hex().to_string()));
    }

    // For larger files, use memory-mapped multi-threaded hashing.
    let mut hasher = blake3::Hasher::new();

    // SAFETY: The file is opened read-only and we hold the file handle for the duration of
    // the hash computation. The file is a wheel that was just built locally, so no concurrent
    // modification is expected.
    let mmap = unsafe { memmap2::Mmap::map(&file) }.map_err(Error::Io)?;
    hasher.update_rayon(&mmap);

    let hash = hasher.finalize();
    Ok(Blake3Digest::new(hash.to_hex().to_string()))
}
