//! Tests for code signing after binary modification (macOS only).

#![cfg(target_os = "macos")]

use std::path::PathBuf;
use std::process::Command;

use fs_err as fs;
use tempfile::TempDir;

fn test_data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data")
}

fn copy_dylib(name: &str, temp_dir: &TempDir) -> PathBuf {
    let src = test_data_dir().join(name);
    let dest = temp_dir.path().join(name);
    fs::copy(&src, &dest).unwrap();
    dest
}

#[test]
fn test_codesign_after_modification() {
    let temp_dir = TempDir::new().unwrap();
    let dylib = copy_dylib("liba.dylib", &temp_dir);

    // Modify the install ID.
    uv_delocate::macho::change_install_id(&dylib, "@loader_path/new_id.dylib").unwrap();

    // Verify the binary is still valid (codesign should have been applied).
    let output = Command::new("codesign")
        .args(["--verify", &dylib.to_string_lossy()])
        .output()
        .unwrap();

    // Ad-hoc signed binaries should verify successfully.
    assert!(
        output.status.success(),
        "Binary should be valid after modification: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_codesign_after_rpath_deletion() {
    let temp_dir = TempDir::new().unwrap();
    let dylib = copy_dylib("libextfunc_rpath.dylib", &temp_dir);

    // Parse and check rpaths.
    let macho = uv_delocate::macho::parse_macho(&dylib).unwrap();

    // Delete an rpath if there is one.
    if !macho.rpaths.is_empty() {
        let rpath = &macho.rpaths[0];
        uv_delocate::macho::delete_rpath(&dylib, rpath).unwrap();

        // Verify the binary is still valid.
        let output = Command::new("codesign")
            .args(["--verify", &dylib.to_string_lossy()])
            .output()
            .unwrap();

        assert!(
            output.status.success(),
            "Binary should be valid after rpath deletion"
        );
    }
}
