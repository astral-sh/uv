//! Tests for rpath handling functionality.

use std::path::PathBuf;

#[cfg(target_os = "macos")]
use fs_err as fs;
#[cfg(target_os = "macos")]
use tempfile::TempDir;

fn test_data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data")
}

#[cfg(target_os = "macos")]
fn copy_dylib(name: &str, temp_dir: &TempDir) -> PathBuf {
    let src = test_data_dir().join(name);
    let dest = temp_dir.path().join(name);
    fs::copy(&src, &dest).unwrap();
    dest
}

#[test]
fn test_parse_rpath_from_binary() {
    let data_dir = test_data_dir();

    // This binary has rpaths.
    let macho = uv_delocate::macho::parse_macho(&data_dir.join("libextfunc_rpath.dylib")).unwrap();

    // Should have at least one rpath.
    assert!(!macho.rpaths.is_empty(), "Binary should have rpaths");
}

#[test]
#[cfg(target_os = "macos")]
fn test_delete_rpath() {
    let temp_dir = TempDir::new().unwrap();
    let dylib = copy_dylib("libextfunc_rpath.dylib", &temp_dir);

    // Parse the binary and get rpaths.
    let macho = uv_delocate::macho::parse_macho(&dylib).unwrap();
    let original_count = macho.rpaths.len();

    if original_count > 0 {
        let rpath_to_delete = macho.rpaths[0].clone();
        uv_delocate::macho::delete_rpath(&dylib, &rpath_to_delete).unwrap();

        // Verify the rpath was deleted.
        let macho = uv_delocate::macho::parse_macho(&dylib).unwrap();
        assert_eq!(macho.rpaths.len(), original_count - 1);
        assert!(!macho.rpaths.contains(&rpath_to_delete));
    }
}
