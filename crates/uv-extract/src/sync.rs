use std::io::{Read, Seek};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::vendor::{CloneableSeekableReader, HasLength};
use crate::{CompressionMethod, Error, insecure_no_validate, validate_archive_member_name};
use async_zip::base::read::seek::ZipFileReader;
use async_zip::error::ZipError;
use futures::executor::block_on;
use futures::io::{AllowStdIo, AsyncReadExt, AsyncWriteExt};
use rayon::prelude::*;
use rustc_hash::{FxHashMap, FxHashSet};
use tracing::warn;
use uv_configuration::initialize_rayon_once;
use uv_warnings::warn_user_once;

#[derive(Debug)]
struct DuplicateFileEntry {
    file_number: usize,
    #[cfg(unix)]
    mode: Option<u16>,
}

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
    let duplicate_file_entries = if skip_validation {
        FxHashSet::default()
    } else {
        duplicate_file_entries_to_skip(&archive)?
    };
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
                let mut directories = directories.lock().unwrap();
                if directories.insert(path.clone()) {
                    fs_err::create_dir_all(path).map_err(Error::Io)?;
                }
                return Ok(None);
            }

            if duplicate_file_entries.contains(&file_number) {
                return Ok(Some((enclosed_name, entry.uncompressed_size())));
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

fn duplicate_file_entries_to_skip<R>(
    archive: &ZipFileReader<AllowStdIo<CloneableSeekableReader<R>>>,
) -> Result<FxHashSet<usize>, Error>
where
    R: Read + Seek + HasLength,
{
    let mut entries_by_path: FxHashMap<PathBuf, Vec<DuplicateFileEntry>> = FxHashMap::default();

    for (file_number, entry) in archive.file().entries().iter().enumerate() {
        let file_name = match entry.filename().as_str() {
            Ok(file_name) => file_name,
            Err(ZipError::StringNotUtf8) => {
                return Err(Error::CentralDirectoryEntryNotUtf8 {
                    index: file_number as u64,
                });
            }
            Err(err) => return Err(err.into()),
        };

        validate_archive_member_name(file_name)?;

        let Some(enclosed_name) = crate::stream::enclosed_name(file_name) else {
            continue;
        };

        if entry.dir()? {
            continue;
        }

        entries_by_path
            .entry(enclosed_name)
            .or_default()
            .push(DuplicateFileEntry {
                file_number,
                #[cfg(unix)]
                mode: entry.unix_permissions(),
            });
    }

    let mut skip = FxHashSet::default();
    for (path, entries) in entries_by_path {
        let Some((first, rest)) = entries.split_first() else {
            continue;
        };
        if rest.is_empty() {
            continue;
        }

        #[cfg(unix)]
        {
            let mut expected_mode = first.mode;
            for entry in rest {
                if let Some(mode) = entry.mode {
                    match expected_mode {
                        Some(expected) if expected != mode => {
                            return Err(Error::DuplicateExecutableFileHeader { path });
                        }
                        Some(_) => {}
                        None => expected_mode = Some(mode),
                    }
                }
            }
        }

        let expected_contents = read_entry_contents(archive, first.file_number, &path)?;
        for entry in rest {
            let contents = read_entry_contents(archive, entry.file_number, &path)?;
            if contents != expected_contents {
                return Err(Error::DuplicateLocalFileHeader { path });
            }
            skip.insert(entry.file_number);
        }
    }

    Ok(skip)
}

fn read_entry_contents<R>(
    archive: &ZipFileReader<AllowStdIo<CloneableSeekableReader<R>>>,
    file_number: usize,
    path: &Path,
) -> Result<Vec<u8>, Error>
where
    R: Read + Seek + HasLength,
{
    let mut archive = archive.clone();
    let entry = archive.file().entries()[file_number].clone();
    let expected_size = entry.uncompressed_size();
    let capacity = usize::try_from(expected_size).unwrap_or_default();

    let (contents, computed_crc32) = block_on(async {
        let mut file = archive.reader_with_entry(file_number).await?;
        let mut contents = Vec::with_capacity(capacity);
        file.read_to_end(&mut contents)
            .await
            .map_err(Error::io_or_compression)?;
        Ok::<_, Error>((contents, file.compute_hash()))
    })?;

    let computed_size = u64::try_from(contents.len()).unwrap_or(u64::MAX);
    if computed_size != expected_size {
        return Err(Error::BadUncompressedSize {
            path: path.to_path_buf(),
            computed: computed_size,
            expected: expected_size,
        });
    }

    if computed_crc32 != entry.crc32() {
        return Err(Error::BadCrc32 {
            path: path.to_path_buf(),
            computed: computed_crc32,
            expected: entry.crc32(),
        });
    }

    Ok(contents)
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
    use std::io::Write;
    use std::path::Path;

    use async_zip::base::write::ZipFileWriter;
    use async_zip::{Compression, ZipEntryBuilder};
    use futures::executor::block_on;

    use crate::Error;

    fn zip_with_entries(entries: &[(&str, &[u8])]) -> tempfile::NamedTempFile {
        let mut writer = ZipFileWriter::new(Vec::new());
        for (path, contents) in entries {
            let entry = ZipEntryBuilder::new((*path).into(), Compression::Stored);
            block_on(writer.write_entry_whole(entry, contents)).unwrap();
        }
        let bytes = block_on(writer.close()).unwrap();

        let mut archive = tempfile::NamedTempFile::new().unwrap();
        archive.write_all(&bytes).unwrap();
        archive.flush().unwrap();
        archive
    }

    #[test]
    fn rejects_duplicate_entries_with_conflicting_contents() {
        let archive = zip_with_entries(&[
            ("package/data.txt", b"first"),
            ("package/data.txt", b"second"),
        ]);
        let target = tempfile::TempDir::new().unwrap();

        let err = super::unzip(fs_err::File::open(archive.path()).unwrap(), target.path())
            .expect_err("conflicting duplicate entries must be rejected");

        match err {
            Error::DuplicateLocalFileHeader { path } => {
                assert_eq!(path, Path::new("package/data.txt"));
            }
            err => panic!("expected duplicate local file header error, got {err:?}"),
        }
    }

    #[test]
    fn allows_duplicate_entries_with_matching_contents() {
        let archive =
            zip_with_entries(&[("package/data.txt", b"same"), ("package/data.txt", b"same")]);
        let target = tempfile::TempDir::new().unwrap();

        let files = super::unzip(fs_err::File::open(archive.path()).unwrap(), target.path())
            .expect("matching duplicate entries should be accepted");

        assert_eq!(
            fs_err::read_to_string(target.path().join("package/data.txt")).unwrap(),
            "same"
        );
        assert_eq!(
            files,
            vec![
                (Path::new("package/data.txt").to_path_buf(), 4),
                (Path::new("package/data.txt").to_path_buf(), 4),
            ]
        );
    }
}
