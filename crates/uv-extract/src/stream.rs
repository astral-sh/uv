use std::path::Path;

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
    let mut reader = reader.compat();
    let mut zip = async_zip::base::read::stream::ZipFileReader::new(&mut reader);

    let mut directories = FxHashSet::default();

    while let Some(mut entry) = zip.next_with_entry().await? {
        // Construct the (expected) path to the file on-disk.
        let path = entry.reader().entry().filename().as_str()?;
        let path = target.as_ref().join(path);
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

            // Construct the (expected) path to the file on-disk.
            let path = entry.filename().as_str()?;
            let path = target.as_ref().join(path);

            if let Some(mode) = entry.unix_permissions() {
                fs_err::set_permissions(&path, Permissions::from_mode(mode))?;
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
    let decompressed_bytes = async_compression::tokio::bufread::GzipDecoder::new(reader);
    let mut archive = tokio_tar::ArchiveBuilder::new(decompressed_bytes)
        .set_preserve_permissions(false)
        .build();
    Ok(archive.unpack(target.as_ref()).await?)
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
