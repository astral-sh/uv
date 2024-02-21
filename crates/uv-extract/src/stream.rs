use std::path::{Component, Path, PathBuf};
use std::pin::Pin;

use futures::StreamExt;
use rustc_hash::FxHashSet;
use tokio_util::compat::{FuturesAsyncReadCompatExt, TokioAsyncReadCompatExt};

use crate::Error;

/// Unzip a `.zip` archive into the target directory, without requiring `Seek`.
///
/// This is useful for unzipping files as they're being downloaded. If the archive
/// is already fully on disk, consider using `unzip_archive`, which can use multiple
/// threads to work faster in that case.
pub async fn unzip<R: tokio::io::AsyncRead + Unpin>(
    reader: R,
    target: impl AsRef<Path>,
) -> Result<(), Error> {
    let target = target.as_ref();
    let mut reader = reader.compat();
    let mut zip = async_zip::base::read::stream::ZipFileReader::new(&mut reader);

    let mut directories = FxHashSet::default();

    while let Some(mut entry) = zip.next_with_entry().await? {
        // Construct the (expected) path to the file on-disk.
        let path = entry.reader().entry().filename().as_str()?;
        let path = target.join(path);
        let is_dir = entry.reader().entry().dir()?;

        // Either create the directory or write the file to disk.
        if is_dir {
            if directories.insert(path.clone()) {
                fs_err::tokio::create_dir_all(path).await?;
            }
        } else {
            if let Some(parent) = path.parent() {
                if directories.insert(parent.to_path_buf()) {
                    fs_err::tokio::create_dir_all(parent).await?;
                }
            }

            // We don't know the file permissions here, because we haven't seen the central directory yet.
            let file = fs_err::tokio::File::create(path).await?;
            let mut writer =
                if let Ok(size) = usize::try_from(entry.reader().entry().uncompressed_size()) {
                    tokio::io::BufWriter::with_capacity(size, file)
                } else {
                    tokio::io::BufWriter::new(file)
                };
            let mut reader = entry.reader_mut().compat();
            tokio::io::copy(&mut reader, &mut writer).await?;
        }

        // Close current file to get access to the next one. See docs:
        // https://docs.rs/async_zip/0.0.16/async_zip/base/read/stream/
        zip = entry.skip().await?;
    }

    // On Unix, we need to set file permissions, which are stored in the central directory, at the
    // end of the archive. The `ZipFileReader` reads until it sees a central directory signature,
    // which indicates the first entry in the central directory. So we continue reading from there.
    #[cfg(unix)]
    {
        use std::fs::Permissions;
        use std::os::unix::fs::PermissionsExt;

        // To avoid lots of small reads to `reader` when parsing the central directory, wrap it in
        // a buffer.
        let mut buf = futures::io::BufReader::new(reader);
        let mut directory = async_zip::base::read::cd::CentralDirectoryReader::new(&mut buf);
        while let Some(entry) = directory.next().await? {
            if entry.dir()? {
                continue;
            }

            let Some(mode) = entry.unix_permissions() else {
                continue;
            };

            // The executable bit is the only permission we preserve, otherwise we use the OS defaults.
            // https://github.com/pypa/pip/blob/3898741e29b7279e7bffe044ecfbe20f6a438b1e/src/pip/_internal/utils/unpacking.py#L88-L100
            let has_any_executable_bit = mode & 0o111;
            if has_any_executable_bit != 0 {
                // Construct the (expected) path to the file on-disk.
                let path = entry.filename().as_str()?;
                let path = target.join(path);

                let permissions = fs_err::tokio::metadata(&path).await?.permissions();
                fs_err::tokio::set_permissions(
                    &path,
                    Permissions::from_mode(permissions.mode() | 0o111),
                )
                .await?;
            }
        }
    }

    Ok(())
}

/// Unzip a `.tar.gz` archive into the target directory, without requiring `Seek`.
///
/// This is useful for unpacking files as they're being downloaded.
pub async fn untar<R: tokio::io::AsyncBufRead + Unpin>(
    reader: R,
    target: impl AsRef<Path>,
) -> Result<(), Error> {
    /// Unpack the given tar archive into the destination directory.
    ///
    /// This is equivalent to `archive.unpack_in(dst)`, but it also preserves the executable bit.
    async fn unpack<R: tokio::io::AsyncRead + Unpin, P: AsRef<Path>>(
        archive: &mut tokio_tar::Archive<R>,
        dst: P,
    ) -> std::io::Result<()> {
        let mut entries = archive.entries()?;
        let mut pinned = Pin::new(&mut entries);
        while let Some(entry) = pinned.next().await {
            // Unpack the file into the destination directory.
            let mut file = entry?;
            file.unpack_in(dst.as_ref()).await?;

            // Preserve the executable bit.
            #[cfg(unix)]
            {
                use std::fs::Permissions;
                use std::os::unix::fs::PermissionsExt;

                /// Determine the path at which the given tar entry will be unpacked, when unpacking into `dst`.
                ///
                /// See: <https://github.com/vorot93/tokio-tar/blob/87338a76092330bc6fe60de95d83eae5597332e1/src/entry.rs#L418>
                fn unpacked_at(dst: &Path, entry: &Path) -> Option<PathBuf> {
                    let mut file_dst = dst.to_path_buf();
                    {
                        for part in entry.components() {
                            match part {
                                // Leading '/' characters, root paths, and '.'
                                // components are just ignored and treated as "empty
                                // components"
                                Component::Prefix(..) | Component::RootDir | Component::CurDir => {
                                    continue
                                }

                                // If any part of the filename is '..', then skip over
                                // unpacking the file to prevent directory traversal
                                // security issues.  See, e.g.: CVE-2001-1267,
                                // CVE-2002-0399, CVE-2005-1918, CVE-2007-4131
                                Component::ParentDir => return None,

                                Component::Normal(part) => file_dst.push(part),
                            }
                        }
                    }

                    // Skip cases where only slashes or '.' parts were seen, because
                    // this is effectively an empty filename.
                    if *dst == *file_dst {
                        return None;
                    }

                    // Skip entries without a parent (i.e. outside of FS root)
                    file_dst.parent()?;

                    Some(file_dst)
                }

                let mode = file.header().mode()?;

                let has_any_executable_bit = mode & 0o111;
                if has_any_executable_bit != 0 {
                    if let Some(path) = unpacked_at(dst.as_ref(), &file.path()?) {
                        let permissions = fs_err::tokio::metadata(&path).await?.permissions();
                        fs_err::tokio::set_permissions(
                            &path,
                            Permissions::from_mode(permissions.mode() | 0o111),
                        )
                        .await?;
                    }
                }
            }
        }
        Ok(())
    }

    let decompressed_bytes = async_compression::tokio::bufread::GzipDecoder::new(reader);
    let mut archive = tokio_tar::ArchiveBuilder::new(decompressed_bytes)
        .set_preserve_mtime(false)
        .build();
    Ok(unpack(&mut archive, target.as_ref()).await?)
}

/// Unzip a `.zip` or `.tar.gz` archive into the target directory, without requiring `Seek`.
pub async fn archive<R: tokio::io::AsyncBufRead + Unpin>(
    reader: R,
    source: impl AsRef<Path>,
    target: impl AsRef<Path>,
) -> Result<(), Error> {
    // `.zip`
    if source
        .as_ref()
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"))
    {
        unzip(reader, target).await?;
        return Ok(());
    }

    // `.tar.gz`
    if source
        .as_ref()
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("gz"))
    {
        if source.as_ref().file_stem().is_some_and(|stem| {
            Path::new(stem)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("tar"))
        }) {
            untar(reader, target).await?;
            return Ok(());
        }
    }

    Err(Error::UnsupportedArchive(source.as_ref().to_path_buf()))
}
