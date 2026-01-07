//! Python wheel file operations.
//!
//! Provides functionality for unpacking, modifying, and repacking wheel files,
//! including RECORD file updates.

use std::io::{self, Read};
use std::path::Path;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use fs_err as fs;
use fs_err::File;
use serde::Serialize;
use sha2::{Digest, Sha256};
use walkdir::WalkDir;
use zip::ZipWriter;
use zip::write::FileOptions;

use uv_distribution_filename::WheelFilename;
use uv_platform_tags::PlatformTag;

use crate::error::DelocateError;

/// Build a wheel filename with the given platform tags.
pub fn filename_with_platform(filename: &WheelFilename, platform_tags: &[PlatformTag]) -> String {
    let python_tags = filename
        .python_tags()
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(".");
    let abi_tags = filename
        .abi_tags()
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(".");
    let platform_tags_str = platform_tags
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(".");

    if let Some(build_tag) = filename.build_tag() {
        format!(
            "{}-{}-{}-{}-{}-{}.whl",
            filename.name.as_dist_info_name(),
            filename.version,
            build_tag,
            python_tags,
            abi_tags,
            platform_tags_str
        )
    } else {
        format!(
            "{}-{}-{}-{}-{}.whl",
            filename.name.as_dist_info_name(),
            filename.version,
            python_tags,
            abi_tags,
            platform_tags_str
        )
    }
}

/// Unpack a wheel to a directory.
pub fn unpack_wheel(wheel_path: &Path, dest_dir: &Path) -> Result<(), DelocateError> {
    let file = File::open(wheel_path)?;
    uv_extract::unzip(file, dest_dir)?;
    Ok(())
}

/// Repack a directory into a wheel file.
pub fn pack_wheel(source_dir: &Path, wheel_path: &Path) -> Result<(), DelocateError> {
    let file = File::create(wheel_path)?;
    let mut zip = ZipWriter::new(file);

    let options = FileOptions::<()>::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);

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

        let relative_str = relative.to_string_lossy();

        // Determine permissions.
        #[cfg(unix)]
        let options = {
            use std::os::unix::fs::PermissionsExt;
            let metadata = fs::metadata(path)?;
            let mode = metadata.permissions().mode();
            options.unix_permissions(mode)
        };

        zip.start_file(relative_str.as_ref(), options)?;

        let mut f = File::open(path)?;
        io::copy(&mut f, &mut zip)?;
    }

    zip.finish()?;
    Ok(())
}

/// Compute the SHA256 hash of a file in the format used by RECORD files.
fn hash_file(path: &Path) -> Result<(String, u64), DelocateError> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    let mut size = 0u64;

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

/// A single entry in a RECORD file.
///
/// See: <https://packaging.python.org/en/latest/specifications/recording-installed-packages/#the-record-file>
#[derive(Serialize, PartialOrd, PartialEq, Ord, Eq)]
struct RecordEntry {
    path: String,
    hash: Option<String>,
    size: Option<u64>,
}

/// Update the RECORD file in a wheel directory.
pub fn update_record(wheel_dir: &Path, dist_info_dir: &str) -> Result<(), DelocateError> {
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

        let relative_str = relative.to_string_lossy().replace('\\', "/");

        // RECORD file itself has no hash.
        if relative_str == format!("{dist_info_dir}/RECORD") {
            records.push(RecordEntry {
                path: relative_str,
                hash: None,
                size: None,
            });
        } else {
            let (hash, size) = hash_file(path)?;
            records.push(RecordEntry {
                path: relative_str,
                hash: Some(hash),
                size: Some(size),
            });
        }
    }

    // Sort for reproducibility.
    records.sort();

    let mut writer = csv::WriterBuilder::new()
        .has_headers(false)
        .escape(b'"')
        .from_path(&record_path)?;
    for record in records {
        writer.serialize(record)?;
    }

    Ok(())
}

/// Find the .dist-info directory in an unpacked wheel.
pub fn find_dist_info(wheel_dir: &Path) -> Result<String, DelocateError> {
    for entry in fs::read_dir(wheel_dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.ends_with(".dist-info") && entry.file_type()?.is_dir() {
            return Ok(name_str.into_owned());
        }
    }

    Err(DelocateError::MissingDistInfo)
}
