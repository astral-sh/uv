use std::path::Path;

use tempfile::NamedTempFile;

/// Write `data` to `path` atomically using a temporary file and atomic rename.
pub async fn write_atomic(path: impl AsRef<Path>, data: impl AsRef<[u8]>) -> std::io::Result<()> {
    let temp_file = NamedTempFile::new_in(
        path.as_ref()
            .parent()
            .expect("Write path must have a parent"),
    )?;
    fs_err::tokio::write(&temp_file, &data).await?;
    temp_file.persist(&path).map_err(|err| {
        std::io::Error::new(
            std::io::ErrorKind::Other,
            format!(
                "Failed to persist temporary file to {}: {}",
                path.as_ref().display(),
                err.error
            ),
        )
    })?;
    Ok(())
}

/// Write `data` to `path` atomically using a temporary file and atomic rename.
pub fn write_atomic_sync(path: impl AsRef<Path>, data: impl AsRef<[u8]>) -> std::io::Result<()> {
    let temp_file = NamedTempFile::new_in(
        path.as_ref()
            .parent()
            .expect("Write path must have a parent"),
    )?;
    fs_err::write(&temp_file, &data)?;
    temp_file.persist(&path).map_err(|err| {
        std::io::Error::new(
            std::io::ErrorKind::Other,
            format!(
                "Failed to persist temporary file to {}: {}",
                path.as_ref().display(),
                err.error
            ),
        )
    })?;
    Ok(())
}

/// Rename `from` to `to` atomically using a temporary file and atomic rename.
///
/// Returns `false` if the `to` path already existed and thus was removed before performing the
/// rename.
pub fn rename_atomic_sync(from: impl AsRef<Path>, to: impl AsRef<Path>) -> std::io::Result<bool> {
    // Remove the destination if it exists.
    let safe = if let Ok(metadata) = fs_err::metadata(&to) {
        if metadata.is_dir() {
            fs_err::remove_dir_all(&to)?;
        } else {
            fs_err::remove_file(&to)?;
        }
        false
    } else {
        true
    };

    // Move the source file to the destination.
    fs_err::rename(from, to)?;

    Ok(safe)
}

/// Copy `from` to `to` atomically using a temporary file and atomic rename.
///
/// Returns `false` if the `to` path already existed and thus was removed before performing the
/// rename.
pub fn copy_atomic_sync(from: impl AsRef<Path>, to: impl AsRef<Path>) -> std::io::Result<bool> {
    // Copy to a temporary file.
    let temp_file =
        NamedTempFile::new_in(to.as_ref().parent().expect("Write path must have a parent"))?;
    fs_err::copy(from, &temp_file)?;

    // Remove the destination if it exists.
    let safe = if let Ok(metadata) = fs_err::metadata(&to) {
        if metadata.is_dir() {
            fs_err::remove_dir_all(&to)?;
        } else {
            fs_err::remove_file(&to)?;
        }
        false
    } else {
        true
    };

    // Move the temporary file to the destination.
    temp_file.persist(&to).map_err(|err| {
        std::io::Error::new(
            std::io::ErrorKind::Other,
            format!(
                "Failed to persist temporary file to {}: {}",
                to.as_ref().display(),
                err.error
            ),
        )
    })?;

    Ok(safe)
}
