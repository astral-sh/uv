//! Tests for install name modification (macOS only).

#![cfg(target_os = "macos")]

use std::path::PathBuf;

use fs_err as fs;
use tempfile::TempDir;
use uv_delocate::Arch;

fn test_data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data")
}

fn copy_dylib(name: &str, temp_dir: &TempDir) -> PathBuf {
    let src = test_data_dir().join(name);
    let dst = temp_dir.path().join(name);
    fs::copy(&src, &dst).unwrap();
    dst
}

#[test]
fn test_change_install_name() {
    let temp_dir = TempDir::new().unwrap();
    let dylib = copy_dylib("libextfunc_rpath.dylib", &temp_dir);

    // This dylib depends on @rpath/libextfunc2_rpath.dylib.
    // Change it to a shorter @loader_path path (which will fit).
    uv_delocate::macho::change_install_name(
        &dylib,
        "@rpath/libextfunc2_rpath.dylib",
        "@loader_path/ext2.dylib",
    )
    .unwrap();

    // Verify the change.
    let macho = uv_delocate::macho::parse_macho(&dylib).unwrap();
    let dep_names: Vec<&str> = macho.dependencies.iter().map(|d| d.as_str()).collect();
    assert!(dep_names.contains(&"@loader_path/ext2.dylib"));
    assert!(!dep_names.contains(&"@rpath/libextfunc2_rpath.dylib"));
}

#[test]
fn test_change_install_name_longer_via_install_name_tool() {
    let temp_dir = TempDir::new().unwrap();
    let dylib = copy_dylib("libb.dylib", &temp_dir);

    // libb depends on liba.dylib (short name).
    // Change to a longer name - should succeed via install_name_tool fallback.
    uv_delocate::macho::change_install_name(
        &dylib,
        "liba.dylib",
        "@loader_path/long/path/liba.dylib",
    )
    .unwrap();

    // Verify the change.
    let macho = uv_delocate::macho::parse_macho(&dylib).unwrap();
    let dep_names: Vec<&str> = macho.dependencies.iter().map(|d| d.as_str()).collect();
    assert!(dep_names.contains(&"@loader_path/long/path/liba.dylib"));
}

#[test]
fn test_change_install_name_not_found() {
    let temp_dir = TempDir::new().unwrap();
    let dylib = copy_dylib("liba.dylib", &temp_dir);

    // Get original dependencies.
    let original = uv_delocate::macho::parse_macho(&dylib).unwrap();
    let original_deps: Vec<_> = original.dependencies.iter().map(|d| d.clone()).collect();

    // liba doesn't depend on "nonexistent.dylib".
    // install_name_tool silently does nothing if the old name doesn't exist.
    let result = uv_delocate::macho::change_install_name(
        &dylib,
        "nonexistent.dylib",
        "@loader_path/foo.dylib",
    );
    assert!(result.is_ok());

    // Verify dependencies are unchanged.
    let after = uv_delocate::macho::parse_macho(&dylib).unwrap();
    let after_deps: Vec<_> = after.dependencies.iter().map(|d| d.clone()).collect();
    assert_eq!(original_deps, after_deps);
}

#[test]
fn test_change_install_id() {
    let temp_dir = TempDir::new().unwrap();
    let dylib = copy_dylib("libextfunc_rpath.dylib", &temp_dir);

    // Change install ID (original is @rpath/libextfunc_rpath.dylib).
    // Use a shorter name that fits.
    uv_delocate::macho::change_install_id(&dylib, "@loader_path/ext.dylib").unwrap();

    // Verify the change.
    let macho = uv_delocate::macho::parse_macho(&dylib).unwrap();
    assert!(macho.install_name.is_some());
    assert_eq!(
        macho.install_name.as_ref().unwrap(),
        "@loader_path/ext.dylib"
    );
}

#[test]
fn test_change_install_id_longer_via_install_name_tool() {
    let temp_dir = TempDir::new().unwrap();
    let dylib = copy_dylib("liba.dylib", &temp_dir);

    // Original ID is "liba.dylib" (short).
    // Change to a longer name - should succeed via install_name_tool fallback.
    uv_delocate::macho::change_install_id(&dylib, "@loader_path/long/path/liba.dylib").unwrap();

    // Verify the change.
    let macho = uv_delocate::macho::parse_macho(&dylib).unwrap();
    assert!(macho.install_name.is_some());
    assert_eq!(
        macho.install_name.as_ref().unwrap(),
        "@loader_path/long/path/liba.dylib"
    );
}

#[test]
fn test_change_install_id_universal_binary() {
    let temp_dir = TempDir::new().unwrap();
    let dylib = copy_dylib("libextfunc_rpath.dylib", &temp_dir);

    // Change install ID in universal binary - should update both slices.
    // Original is @rpath/libextfunc_rpath.dylib.
    uv_delocate::macho::change_install_id(&dylib, "@loader_path/ext.dylib").unwrap();

    // Verify both architectures see the change.
    let macho = uv_delocate::macho::parse_macho(&dylib).unwrap();
    assert!(macho.install_name.is_some());
    assert_eq!(
        macho.install_name.as_ref().unwrap(),
        "@loader_path/ext.dylib"
    );
    // Should still have both architectures.
    assert!(macho.archs.contains(&Arch::X86_64));
    assert!(macho.archs.contains(&Arch::Arm64));
}
