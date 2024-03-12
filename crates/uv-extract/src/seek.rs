use std::path::Path;

use rustc_hash::FxHashSet;
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tokio_util::compat::TokioAsyncReadCompatExt;

use crate::Error;

/// Unzip a `.zip` archive into the target directory, requiring `Seek`.
///
/// This is useful for unzipping files asynchronously that already exist on disk.
pub async fn unzip<R: tokio::io::AsyncRead + tokio::io::AsyncSeek + Unpin>(
    reader: R,
    target: impl AsRef<Path>,
) -> Result<(), Error> {
    let target = target.as_ref();
    let mut reader = reader.compat();
    let mut zip = async_zip::base::read::seek::ZipFileReader::new(&mut reader).await?;

    let mut directories = FxHashSet::default();

    for index in 0..zip.file().entries().len() {
        let reader = zip.reader_with_entry(index).await?;

        // Construct the (expected) path to the file on-disk.
        let path = reader.entry().filename().as_str()?;
        let path = target.join(path);
        let is_dir = reader.entry().dir()?;

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

            // Copy the mode.
            #[cfg(unix)]
            let mode = reader.entry().unix_permissions();

            // Copy the file contents.
            let file = fs_err::tokio::File::create(&path).await?;
            let mut writer = if let Ok(size) = usize::try_from(reader.entry().uncompressed_size()) {
                tokio::io::BufWriter::with_capacity(size, file)
            } else {
                tokio::io::BufWriter::new(file)
            };
            tokio::io::copy(&mut reader.compat(), &mut writer).await?;

            // See `uv_extract::stream::unzip`.
            #[cfg(unix)]
            {
                use std::fs::Permissions;
                use std::os::unix::fs::PermissionsExt;

                let Some(mode) = mode else {
                    continue;
                };

                // The executable bit is the only permission we preserve, otherwise we use the OS defaults.
                // https://github.com/pypa/pip/blob/3898741e29b7279e7bffe044ecfbe20f6a438b1e/src/pip/_internal/utils/unpacking.py#L88-L100
                let has_any_executable_bit = mode & 0o111;
                if has_any_executable_bit != 0 {
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

/// Unzip a `.zip` or `.tar.gz` archive into the target directory, requiring `Seek`.
pub async fn archive<R: tokio::io::AsyncBufRead + tokio::io::AsyncSeek + Unpin>(
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
        && source.as_ref().file_stem().is_some_and(|stem| {
            Path::new(stem)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("tar"))
        })
    {
        crate::stream::untar(reader, target).await?;
        return Ok(());
    }

    Err(Error::UnsupportedArchive(source.as_ref().to_path_buf()))
}
