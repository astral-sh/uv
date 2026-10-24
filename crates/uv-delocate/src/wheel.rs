//! Python wheel file operations.
//!
//! Provides functionality for unpacking, modifying, and repacking wheel files,
//! including RECORD file updates.

use std::io::Read;
use std::path::Path;

use async_zip::base::write::ZipFileWriter;
use async_zip::{Compression, ZipEntryBuilder};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use fs_err as fs;
use fs_err::File;
use futures::executor::block_on;
use futures::io::AllowStdIo;
use sha2::{Digest, Sha256};
use uv_fs::Simplified;
use uv_install_wheel::{RecordEntry, write_record_file};
use walkdir::WalkDir;

use crate::error::DelocateError;

/// Unpack a wheel to a directory.
pub fn unpack_wheel(wheel_path: &Path, dest_dir: &Path) -> Result<(), DelocateError> {
    let file = File::open(wheel_path)?;
    uv_extract::unzip(file, dest_dir)?;
    Ok(())
}

/// Repack a directory into a wheel file.
pub fn pack_wheel(source_dir: &Path, wheel_path: &Path) -> Result<(), DelocateError> {
    let file = File::create(wheel_path)?;
    let mut zip = ZipFileWriter::new(AllowStdIo::new(file));

    let walkdir = WalkDir::new(source_dir);
    let mut paths: Vec<_> = walkdir
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .collect();

    // Sort for reproducibility.
    paths.sort_by(|a, b| a.path().cmp(b.path()));

    for entry in paths {
        let path = entry.path();
        let relative =
            path.strip_prefix(source_dir)
                .map_err(|_| DelocateError::PathNotInWheel {
                    path: path.to_path_buf(),
                    wheel_dir: source_dir.to_path_buf(),
                })?;

        let relative = relative.portable_display().to_string();
        #[cfg(unix)]
        let mode = {
            use std::os::unix::fs::PermissionsExt;
            if fs::metadata(path)?.permissions().mode() & 0o111 == 0 {
                0o100_644
            } else {
                0o100_755
            }
        };
        #[cfg(not(unix))]
        let mode = 0o100_644;

        let entry =
            ZipEntryBuilder::new(relative.into(), Compression::Deflate).unix_permissions(mode);
        let mut writer = block_on(zip.write_entry_seekable(entry))?;
        block_on(futures::io::copy(
            AllowStdIo::new(File::open(path)?),
            &mut writer,
        ))?;
        block_on(writer.close())?;
    }

    block_on(zip.close())?;
    Ok(())
}

fn hash_file(path: &Path) -> Result<(String, u64), DelocateError> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut size = 0;
    let mut buffer = [0; 8192];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
        size += n as u64;
    }
    let hash = hasher.finalize();
    let hash_str = format!("sha256={}", URL_SAFE_NO_PAD.encode(hash));
    Ok((hash_str, size))
}

/// Update the RECORD file in a wheel directory.
pub(crate) fn update_record(wheel_dir: &Path, dist_info_dir: &str) -> Result<(), DelocateError> {
    let record_path = wheel_dir.join(dist_info_dir).join("RECORD");

    let mut records = Vec::new();

    for entry in WalkDir::new(wheel_dir) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }

        let path = entry.path();
        let relative = path
            .strip_prefix(wheel_dir)
            .map_err(|_| DelocateError::PathNotInWheel {
                path: path.to_path_buf(),
                wheel_dir: wheel_dir.to_path_buf(),
            })?;

        let relative_str = relative.portable_display().to_string();

        // RECORD file itself has no hash.
        if relative_str == format!("{dist_info_dir}/RECORD") {
            records.push(RecordEntry::unhashed(relative_str));
        } else {
            let (hash, size) = hash_file(path)?;
            records.push(RecordEntry::new(relative_str, hash, size));
        }
    }

    write_record_file(&record_path, records)?;

    Ok(())
}

/// Update the WHEEL file with new platform tags.
///
/// This replaces all `Tag:` entries with the new tags derived from the wheel filename.
pub fn update_wheel_file(
    wheel_dir: &Path,
    dist_info_dir: &str,
    new_tags: &[String],
) -> Result<(), DelocateError> {
    use std::io::{BufRead, BufReader, Write};

    let wheel_path = wheel_dir.join(dist_info_dir).join("WHEEL");
    let content = fs::read_to_string(&wheel_path)?;

    let mut output = Vec::new();

    // Copy all non-Tag lines, skipping empty lines.
    for line in BufReader::new(content.as_bytes()).lines() {
        let line = line?;
        if !line.starts_with("Tag:") && !line.is_empty() {
            writeln!(output, "{line}")?;
        }
    }

    // Add new tags.
    for tag in new_tags {
        writeln!(output, "Tag: {tag}")?;
    }

    fs::write(&wheel_path, output)?;

    Ok(())
}
