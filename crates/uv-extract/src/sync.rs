use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use crate::vendor::CloneableSeekableReader;
use crate::{CompressionMethod, Error, insecure_no_validate, validate_archive_member_name};
use rayon::prelude::*;
use rustc_hash::FxHashSet;
use tracing::warn;
use uv_configuration::RAYON_INITIALIZE;
use uv_warnings::warn_user_once;
use zip::ZipArchive;

/// Unzip a `.zip` archive into the target directory.
pub fn unzip(reader: fs_err::File, target: &Path) -> Result<(), Error> {
    let (reader, filename) = reader.into_parts();

    // Unzip in parallel.
    let reader = std::io::BufReader::new(reader);
    let archive = ZipArchive::new(CloneableSeekableReader::new(reader))?;
    let directories = Mutex::new(FxHashSet::default());
    let skip_validation = insecure_no_validate();
    // Initialize the threadpool with the user settings.
    LazyLock::force(&RAYON_INITIALIZE);
    (0..archive.len())
        .into_par_iter()
        .map(|file_number| {
            let mut archive = archive.clone();
            let mut file = archive.by_index(file_number)?;

            let compression = CompressionMethod::from(file.compression());
            if !compression.is_well_known() {
                warn_user_once!(
                    "One or more file entries in '{filename}' use the '{compression}' compression method, which is not widely supported. A future version of uv will reject ZIP archives containing entries compressed with this method. Entries must be compressed with the '{stored}', '{deflate}', or '{zstd}' compression methods.",
                    filename = filename.display(),
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
