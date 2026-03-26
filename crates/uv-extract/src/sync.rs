use std::path::Path;
use std::sync::{LazyLock, Mutex};

use crate::hash::Blake3Digest;
use crate::vendor::CloneableSeekableReader;
use crate::{CompressionMethod, Error, insecure_no_validate, validate_archive_member_name};
use rayon::prelude::*;
use rustc_hash::FxHashSet;
use tracing::warn;
use uv_configuration::RAYON_INITIALIZE;
use uv_warnings::warn_user_once;
use zip::ZipArchive;

/// Unzip a `.zip` archive into the target directory, returning the blake3 hash of the archive.
///
/// The blake3 hash and extraction are computed concurrently via [`rayon::join`]. The blake3 hash
/// uses multi-threaded, memory-mapped I/O, while extraction proceeds in parallel over zip entries.
/// Since both share the rayon thread pool and the mmap populates the page cache, the extraction's
/// reads are effectively free.
pub fn unzip(path: &Path, target: &Path) -> Result<Blake3Digest, Error> {
    // Initialize the rayon thread pool before spawning work.
    LazyLock::force(&RAYON_INITIALIZE);

    // Open the file for extraction.
    let file = fs_err::File::open(path).map_err(Error::Io)?;
    let reader = std::io::BufReader::new(file);
    let archive = ZipArchive::new(CloneableSeekableReader::new(reader))?;
    let directories = Mutex::new(FxHashSet::default());
    let skip_validation = insecure_no_validate();

    // Run blake3 hashing and zip extraction concurrently on the rayon pool.
    let (blake3_result, extract_result) = rayon::join(
        || blake3_hash(path),
        || {
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

            if let Err(e) = validate_archive_member_name(file.name()) {
                if !skip_validation {
                    return Err(e);
                }
            }

            // Determine the path of the file within the wheel.
            let Some(enclosed_name) = file.enclosed_name() else {
                warn!("Skipping unsafe file name: {}", file.name());
                return Ok(());
            };

            // Create necessary parent directories.
            let path = target.join(enclosed_name);
            if file.is_dir() {
                let mut directories = directories.lock().unwrap();
                if directories.insert(path.clone()) {
                    fs_err::create_dir_all(path).map_err(Error::Io)?;
                }
                return Ok(());
            }

            if let Some(parent) = path.parent() {
                let mut directories = directories.lock().unwrap();
                if directories.insert(parent.to_path_buf()) {
                    fs_err::create_dir_all(parent).map_err(Error::Io)?;
                }
            }

            // Copy the file contents.
            let outfile = fs_err::File::create(&path).map_err(Error::Io)?;
            let size = file.size();
            if size > 0 {
                let mut writer = if let Ok(size) = usize::try_from(size) {
                    std::io::BufWriter::with_capacity(std::cmp::min(size, 1024 * 1024), outfile)
                } else {
                    std::io::BufWriter::new(outfile)
                };
                std::io::copy(&mut file, &mut writer).map_err(Error::io_or_compression)?;
            }

            // See `uv_extract::stream::unzip`. For simplicity, this is identical with the code there except for being
            // sync.
            #[cfg(unix)]
            {
                use std::fs::Permissions;
                use std::os::unix::fs::PermissionsExt;

                if let Some(mode) = file.unix_mode() {
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

            Ok(())
        })
        .collect::<Result<(), Error>>()
        },
    );

    extract_result?;
    blake3_result
}

/// Compute the blake3 hash of a file using multi-threaded, memory-mapped I/O.
///
/// The caller must ensure the rayon thread pool is initialized before calling this function.
fn blake3_hash(path: &Path) -> Result<Blake3Digest, Error> {
    let mut hasher = blake3::Hasher::new();
    hasher.update_mmap_rayon(path).map_err(Error::Io)?;
    let hash = hasher.finalize();
    Ok(Blake3Digest::new(hash.to_hex().to_string()))
}
