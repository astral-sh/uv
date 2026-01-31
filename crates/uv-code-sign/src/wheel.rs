use std::io::{Read, Write};
use std::path::Path;

use fs_err as fs;
use thiserror::Error;
use walkdir::WalkDir;
use zip::write::FileOptions;
use zip::{CompressionMethod, ZipWriter};

#[derive(Debug, Error)]
pub enum WheelError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Walk(#[from] walkdir::Error),
    #[error(transparent)]
    StripPrefix(#[from] std::path::StripPrefixError),
    #[error("Non-UTF-8 path in wheel")]
    NonUtf8Path,
    #[error("Failed to extract zip entry")]
    Extract(#[source] std::io::Error),
    #[error("Failed to write zip entry")]
    ZipWrite(#[source] zip::result::ZipError),
}

/// Repack a directory into a wheel zip at `output_path`.
///
/// Entries are sorted for deterministic output.
pub fn repack(source_dir: &Path, output_path: &Path) -> Result<(), WheelError> {
    let out_file = fs::File::create(output_path)?;
    let mut writer = ZipWriter::new(out_file);

    let options = FileOptions::<()>::default()
        .compression_method(CompressionMethod::Deflated)
        .system(zip::System::Unix);

    let mut entries: Vec<_> = Vec::new();
    for entry in WalkDir::new(source_dir).sort_by_file_name() {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let abs = entry.path().to_path_buf();
        let rel = abs
            .strip_prefix(source_dir)?
            .to_str()
            .ok_or(WheelError::NonUtf8Path)?
            // Zip entry names must use forward slashes.
            .replace('\\', "/");
        entries.push((rel, abs));
    }

    for (rel, abs) in &entries {
        // Preserve the executable bit, matching the uv build backend convention:
        // 0o755 (rwxr-xr-x) for executables, 0o644 (rw-r--r--) for regular files.
        #[cfg(unix)]
        let entry_options = {
            use std::os::unix::fs::PermissionsExt;
            let executable = fs::metadata(abs)?.permissions().mode() & 0o111 != 0;
            let permissions = if executable { 0o755 } else { 0o644 };
            options.unix_permissions(permissions)
        };
        #[cfg(not(unix))]
        let entry_options = options;

        writer
            .start_file(rel, entry_options)
            .map_err(WheelError::ZipWrite)?;
        let mut f = fs::File::open(abs)?;
        let mut buf = Vec::new();
        f.read_to_end(&mut buf).map_err(WheelError::Extract)?;
        writer.write_all(&buf).map_err(WheelError::Extract)?;
    }

    writer.finish().map_err(WheelError::ZipWrite)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::BTreeMap;
    use std::io::Write;
    use zip::ZipArchive;

    /// Build a minimal wheel zip in memory with the given file entries.
    fn build_test_wheel(entries: &BTreeMap<&str, &[u8]>) -> Vec<u8> {
        let buf = std::io::Cursor::new(Vec::new());
        let mut writer = ZipWriter::new(buf);
        let options = FileOptions::<()>::default().compression_method(CompressionMethod::Stored);
        for (name, data) in entries {
            writer.start_file(*name, options).unwrap();
            writer.write_all(data).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    /// Helper to unpack a wheel file using `uv_extract::unzip`.
    fn unpack(wheel_path: &Path, dest: &Path) {
        let file = fs::File::open(wheel_path).unwrap();
        uv_extract::unzip(file, dest).unwrap();
    }

    #[test]
    fn test_unpack_creates_files() {
        let mut entries = BTreeMap::new();
        entries.insert("pkg/__init__.py", b"hello" as &[u8]);
        entries.insert("pkg-1.0.dist-info/METADATA", b"Name: pkg\n" as &[u8]);
        let wheel_bytes = build_test_wheel(&entries);

        let tmp = tempfile::tempdir().unwrap();
        let wheel_path = tmp.path().join("test.whl");
        fs::write(&wheel_path, &wheel_bytes).unwrap();

        let dest = tmp.path().join("unpacked");
        fs::create_dir_all(&dest).unwrap();
        unpack(&wheel_path, &dest);

        assert_eq!(
            fs::read_to_string(dest.join("pkg/__init__.py")).unwrap(),
            "hello"
        );
        assert_eq!(
            fs::read_to_string(dest.join("pkg-1.0.dist-info/METADATA")).unwrap(),
            "Name: pkg\n"
        );
    }

    #[test]
    fn test_unpack_repack_roundtrip() {
        let mut entries = BTreeMap::new();
        entries.insert("pkg/__init__.py", b"content_a" as &[u8]);
        entries.insert("pkg/mod.py", b"content_b" as &[u8]);
        entries.insert("pkg-1.0.dist-info/RECORD", b"" as &[u8]);
        let wheel_bytes = build_test_wheel(&entries);

        let tmp = tempfile::tempdir().unwrap();
        let wheel_path = tmp.path().join("input.whl");
        fs::write(&wheel_path, &wheel_bytes).unwrap();

        // Unpack.
        let unpack_dir = tmp.path().join("unpacked");
        fs::create_dir_all(&unpack_dir).unwrap();
        unpack(&wheel_path, &unpack_dir);

        // Repack.
        let output_path = tmp.path().join("output.whl");
        repack(&unpack_dir, &output_path).unwrap();

        // Verify the repacked wheel contains the same files and content.
        let file = fs::File::open(&output_path).unwrap();
        let mut archive = ZipArchive::new(file).unwrap();
        let mut repacked: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i).unwrap();
            let name = entry.name().to_string();
            let mut data = Vec::new();
            entry.read_to_end(&mut data).unwrap();
            repacked.insert(name, data);
        }

        assert_eq!(repacked.len(), entries.len());
        for (name, expected) in &entries {
            let actual = repacked
                .get(*name)
                .unwrap_or_else(|| panic!("missing entry: {name}"));
            assert_eq!(actual, expected, "content mismatch for {name}");
        }
    }

    #[test]
    fn test_unpack_skips_path_traversal() {
        // Create a zip with a suspicious entry name (path traversal).
        let buf = std::io::Cursor::new(Vec::new());
        let mut writer = ZipWriter::new(buf);
        let options = FileOptions::<()>::default().compression_method(CompressionMethod::Stored);
        writer.start_file("../escape.txt", options).unwrap();
        writer.write_all(b"bad").unwrap();
        // Also add a normal file.
        writer.start_file("normal.txt", options).unwrap();
        writer.write_all(b"good").unwrap();
        let wheel_bytes = writer.finish().unwrap().into_inner();

        let tmp = tempfile::tempdir().unwrap();
        let wheel_path = tmp.path().join("evil.whl");
        fs::write(&wheel_path, &wheel_bytes).unwrap();

        let dest = tmp.path().join("unpacked");
        fs::create_dir_all(&dest).unwrap();
        unpack(&wheel_path, &dest);

        // The traversal entry should be skipped (enclosed_name returns None).
        assert!(!dest.join("../escape.txt").exists());
        assert!(!tmp.path().join("escape.txt").exists());
        // The normal file should be extracted.
        assert_eq!(fs::read_to_string(dest.join("normal.txt")).unwrap(), "good");
    }
}
