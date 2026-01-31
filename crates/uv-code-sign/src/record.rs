use std::path::Path;

use base64::Engine;
use base64::prelude::BASE64_URL_SAFE_NO_PAD as base64;
use fs_err as fs;
use sha2::{Digest, Sha256};
use thiserror::Error;
use uv_install_wheel::RecordEntry;
use walkdir::WalkDir;

#[derive(Debug, Error)]
pub enum RecordError {
    #[error(transparent)]
    Walk(#[from] walkdir::Error),
    #[error(transparent)]
    StripPrefix(#[from] std::path::StripPrefixError),
    #[error("Non-UTF-8 path in wheel")]
    NonUtf8Path,
    #[error("Failed to read `{}`", path.display())]
    Read {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("Failed to write RECORD file")]
    Write(#[source] std::io::Error),
    #[error("Failed to write CSV record")]
    Csv(#[source] csv::Error),
}

/// Recompute and write the `RECORD` file for a wheel directory.
///
/// Every file gets a `sha256=<base64url_nopad>` hash and byte size.
/// The RECORD file's own entry is written as `<path>,,` (no hash, no size).
pub fn write_record(wheel_dir: &Path, record_rel_path: &str) -> Result<(), RecordError> {
    let record_path = wheel_dir.join(record_rel_path);
    let f = fs::File::create(&record_path).map_err(RecordError::Write)?;
    let mut csv_writer = csv::WriterBuilder::new()
        .has_headers(false)
        .escape(b'"')
        .from_writer(f);

    let mut entries: Vec<RecordEntry> = Vec::new();

    for entry in WalkDir::new(wheel_dir).sort_by_file_name() {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }

        let abs = entry.path();
        let rel = abs.strip_prefix(wheel_dir)?;
        let rel_str = rel.to_str().ok_or(RecordError::NonUtf8Path)?;
        // Wheel RECORD paths must use forward slashes.
        let rel_str = rel_str.replace('\\', "/");

        // RECORD's own entry has no hash/size.
        if rel_str == record_rel_path {
            continue;
        }

        let contents = fs::read(abs).map_err(|source| RecordError::Read {
            path: abs.to_path_buf(),
            source,
        })?;
        let hash = base64.encode(Sha256::digest(&contents));
        let size = contents.len();

        entries.push(RecordEntry {
            path: rel_str,
            hash: Some(format!("sha256={hash}")),
            size: Some(size as u64),
        });
    }

    // Sort for deterministic output.
    entries.sort();

    for entry in &entries {
        csv_writer.serialize(entry).map_err(RecordError::Csv)?;
    }

    // Add the RECORD's own entry last (no hash, no size).
    csv_writer
        .serialize(RecordEntry {
            path: record_rel_path.to_string(),
            hash: None,
            size: None,
        })
        .map_err(RecordError::Csv)?;

    csv_writer.flush().map_err(RecordError::Write)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use base64::Engine;
    use base64::prelude::BASE64_URL_SAFE_NO_PAD as base64;
    use sha2::{Digest, Sha256};

    /// Create a wheel directory with some files and verify that `write_record`
    /// produces a valid RECORD with correct hashes, sizes, and the self-entry.
    #[test]
    fn test_write_record_basic() {
        let tmp = tempfile::tempdir().unwrap();
        let wheel_dir = tmp.path();

        // Create a dist-info directory and an initial (empty) RECORD file.
        let dist_info = wheel_dir.join("pkg-1.0.dist-info");
        fs::create_dir_all(&dist_info).unwrap();
        fs::write(dist_info.join("RECORD"), "").unwrap();
        fs::write(dist_info.join("METADATA"), "Name: pkg\nVersion: 1.0\n").unwrap();

        // Create a package file.
        let pkg_dir = wheel_dir.join("pkg");
        fs::create_dir_all(&pkg_dir).unwrap();
        let init_content = b"print('hello')";
        fs::write(pkg_dir.join("__init__.py"), init_content).unwrap();

        let record_rel = "pkg-1.0.dist-info/RECORD";
        write_record(wheel_dir, record_rel).unwrap();

        let record_content = fs::read_to_string(dist_info.join("RECORD")).unwrap();
        let lines: Vec<&str> = record_content.lines().collect();

        // Expect 3 entries: METADATA, __init__.py, and the RECORD self-entry.
        assert_eq!(
            lines.len(),
            3,
            "unexpected RECORD content:\n{record_content}"
        );

        // The RECORD self-entry must have no hash or size.
        assert!(
            lines.contains(&"pkg-1.0.dist-info/RECORD,,"),
            "missing RECORD self-entry in:\n{record_content}"
        );

        // Verify hash for __init__.py.
        let expected_hash = base64.encode(Sha256::digest(init_content));
        let expected_line = format!(
            "pkg/__init__.py,sha256={expected_hash},{size}",
            size = init_content.len()
        );
        assert!(
            lines.contains(&expected_line.as_str()),
            "missing or incorrect __init__.py entry.\nExpected: {expected_line}\nGot:\n{record_content}"
        );
    }

    /// Verify that entries are sorted by file name for deterministic output.
    #[test]
    fn test_write_record_sorted() {
        let tmp = tempfile::tempdir().unwrap();
        let wheel_dir = tmp.path();

        let dist_info = wheel_dir.join("pkg-1.0.dist-info");
        fs::create_dir_all(&dist_info).unwrap();
        fs::write(dist_info.join("RECORD"), "").unwrap();

        let pkg_dir = wheel_dir.join("pkg");
        fs::create_dir_all(&pkg_dir).unwrap();
        // Create files in reverse alphabetical order.
        fs::write(pkg_dir.join("z.py"), "z").unwrap();
        fs::write(pkg_dir.join("a.py"), "a").unwrap();
        fs::write(pkg_dir.join("m.py"), "m").unwrap();

        let record_rel = "pkg-1.0.dist-info/RECORD";
        write_record(wheel_dir, record_rel).unwrap();

        let record_content = fs::read_to_string(dist_info.join("RECORD")).unwrap();
        let file_names: Vec<&str> = record_content
            .lines()
            .map(|l| l.split(',').next().unwrap())
            .collect();

        // Entries should be sorted (RECORD self-entry comes last by convention).
        let len = file_names.len();
        let data_entries: Vec<&str> = file_names[..len - 1].to_vec();
        let record_entry = file_names[len - 1];
        let mut sorted_entries = data_entries.clone();
        sorted_entries.sort();
        assert_eq!(record_entry, "pkg-1.0.dist-info/RECORD");
        assert_eq!(
            data_entries, sorted_entries,
            "RECORD entries are not sorted"
        );
    }
}
