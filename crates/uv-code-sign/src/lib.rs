mod codesign;
mod macho;
mod record;
mod wheel;

pub use codesign::{CodeSignError, SignOptions};

use std::path::Path;

use fs_err as fs;
use thiserror::Error;
use walkdir::WalkDir;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    CodeSign(#[from] CodeSignError),
    #[error("Failed to create temporary directory")]
    TempDir(#[source] std::io::Error),
    #[error("Failed to unpack wheel `{}`", path.display())]
    Unpack {
        path: std::path::PathBuf,
        #[source]
        source: uv_extract::Error,
    },
    #[error("Failed to repack wheel to `{}`", path.display())]
    Repack {
        path: std::path::PathBuf,
        #[source]
        source: wheel::WheelError,
    },
    #[error(transparent)]
    Record(#[from] record::RecordError),
    #[error(transparent)]
    Walk(#[from] walkdir::Error),
    #[error("No `.dist-info/RECORD` found in wheel")]
    MissingRecord,
    #[error("Non-UTF-8 path in wheel")]
    NonUtf8Path,
    #[error(transparent)]
    StripPrefix(#[from] std::path::StripPrefixError),
    #[error("Failed to copy wheel from `{from}` to `{to}`")]
    Copy {
        from: std::path::PathBuf,
        to: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Sign all Mach-O binaries inside a `.whl` file.
///
/// 1. Unpacks the wheel into a temporary directory.
/// 2. Finds all Mach-O binaries (`.so`, `.dylib`, executables) via header inspection.
/// 3. Signs each with `codesign`.
/// 4. Recomputes the `RECORD` file.
/// 5. Repacks into a new wheel at `output`.
pub fn sign_wheel(input: &Path, output: &Path, options: &SignOptions) -> Result<(), Error> {
    let tmp = tempfile::tempdir().map_err(Error::TempDir)?;
    let unpack_dir = tmp.path();

    tracing::info!("unpacking {}", input.display());
    let file = fs::File::open(input).map_err(|source| Error::Unpack {
        path: input.to_path_buf(),
        source: uv_extract::Error::Io(source),
    })?;
    uv_extract::unzip(file, unpack_dir).map_err(|source| Error::Unpack {
        path: input.to_path_buf(),
        source,
    })?;

    // Find and sign Mach-O binaries.
    let mut signed = 0usize;
    for entry in WalkDir::new(unpack_dir) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        if macho::is_macho(entry.path()) {
            codesign::codesign_file(entry.path(), options)?;
            signed += 1;
        }
    }
    if signed == 0 {
        tracing::warn!("no Mach-O binaries found in wheel, copying as-is");
        fs::copy(input, output).map_err(|source| Error::Copy {
            from: input.to_path_buf(),
            to: output.to_path_buf(),
            source,
        })?;
        return Ok(());
    }
    tracing::info!("signed {signed} Mach-O binaries");

    // Find the RECORD file (e.g., "package-1.0.dist-info/RECORD").
    let record_rel = find_record(unpack_dir)?;
    record::write_record(unpack_dir, &record_rel)?;

    tracing::info!("repacking to {}", output.display());
    wheel::repack(unpack_dir, output).map_err(|source| Error::Repack {
        path: output.to_path_buf(),
        source,
    })?;

    Ok(())
}

/// Locate the `*.dist-info/RECORD` file relative to the wheel root.
fn find_record(wheel_dir: &Path) -> Result<String, Error> {
    for entry in WalkDir::new(wheel_dir).min_depth(1).max_depth(2) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = entry.path().strip_prefix(wheel_dir)?;
        let rel_str = rel.to_str().ok_or(Error::NonUtf8Path)?;
        // Normalize to forward slashes for wheel spec compliance.
        let rel_str = rel_str.replace('\\', "/");
        if rel_str.ends_with(".dist-info/RECORD") {
            return Ok(rel_str);
        }
    }
    Err(Error::MissingRecord)
}

#[cfg(test)]
mod tests {
    use super::*;

    use fs_err as fs;

    #[test]
    fn test_find_record_present() {
        let tmp = tempfile::tempdir().unwrap();
        let wheel_dir = tmp.path();
        let dist_info = wheel_dir.join("mypackage-1.0.dist-info");
        fs::create_dir_all(&dist_info).unwrap();
        fs::write(dist_info.join("RECORD"), "").unwrap();
        fs::write(dist_info.join("METADATA"), "Name: mypackage\n").unwrap();

        let result = find_record(wheel_dir).unwrap();
        assert_eq!(result, "mypackage-1.0.dist-info/RECORD");
    }

    #[test]
    fn test_find_record_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let wheel_dir = tmp.path();
        // No dist-info directory at all.
        fs::create_dir_all(wheel_dir.join("pkg")).unwrap();
        fs::write(wheel_dir.join("pkg/__init__.py"), "").unwrap();

        let result = find_record(wheel_dir);
        assert!(
            matches!(result, Err(Error::MissingRecord)),
            "expected MissingRecord, got: {result:?}"
        );
    }

    /// Build a synthetic wheel with a Mach-O binary (copied from the system),
    /// sign it, and verify the output wheel is valid.
    #[cfg(target_os = "macos")]
    #[test]
    fn test_sign_wheel_roundtrip() {
        use std::io::Write;
        use zip::write::FileOptions;
        use zip::{CompressionMethod, ZipArchive, ZipWriter};

        // Read a real Mach-O binary to embed in the test wheel.
        let macho_bytes = fs::read("/usr/bin/true").unwrap();

        // Build a synthetic wheel zip.
        let tmp = tempfile::tempdir().unwrap();
        let input_path = tmp.path().join("pkg-1.0-cp310-cp310-macosx_11_0_arm64.whl");
        {
            let file = fs::File::create(&input_path).unwrap();
            let mut writer = ZipWriter::new(file);
            let options =
                FileOptions::<()>::default().compression_method(CompressionMethod::Deflated);

            writer.start_file("pkg/__init__.py", options).unwrap();
            writer.write_all(b"# init").unwrap();

            writer.start_file("pkg/native.so", options).unwrap();
            writer.write_all(&macho_bytes).unwrap();

            writer
                .start_file("pkg-1.0.dist-info/METADATA", options)
                .unwrap();
            writer.write_all(b"Name: pkg\nVersion: 1.0\n").unwrap();

            writer
                .start_file("pkg-1.0.dist-info/RECORD", options)
                .unwrap();
            writer.write_all(b"").unwrap();

            writer.finish().unwrap();
        }

        let output_path = tmp.path().join("pkg-1.0-signed.whl");
        let opts = SignOptions::default();
        sign_wheel(&input_path, &output_path, &opts).unwrap();

        // Verify the output wheel exists and is a valid zip.
        assert!(output_path.exists());
        let file = fs::File::open(&output_path).unwrap();
        let mut archive = ZipArchive::new(file).unwrap();

        // Collect file names.
        let names: Vec<String> = (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().to_string())
            .collect();
        assert!(names.contains(&"pkg/__init__.py".to_string()));
        assert!(names.contains(&"pkg/native.so".to_string()));
        assert!(names.contains(&"pkg-1.0.dist-info/RECORD".to_string()));

        // Verify the RECORD file has proper entries.
        let mut record_entry = archive.by_name("pkg-1.0.dist-info/RECORD").unwrap();
        let mut record_content = String::new();
        std::io::Read::read_to_string(&mut record_entry, &mut record_content).unwrap();
        assert!(
            record_content.contains("pkg/native.so,sha256="),
            "RECORD should have hash for native.so"
        );
        assert!(
            record_content.contains("pkg-1.0.dist-info/RECORD,,"),
            "RECORD should have self-entry"
        );
    }
}
