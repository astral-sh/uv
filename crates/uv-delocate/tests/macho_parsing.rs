//! Tests for Mach-O parsing functionality.

use std::collections::HashSet;
use std::path::PathBuf;

use uv_delocate::Arch;

fn test_data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data")
}

#[test]
fn test_is_macho_file() {
    let data_dir = test_data_dir();

    // Should recognize dylibs.
    assert!(uv_delocate::macho::is_macho_file(&data_dir.join("liba.dylib")).unwrap());
    assert!(uv_delocate::macho::is_macho_file(&data_dir.join("libb.dylib")).unwrap());
    assert!(uv_delocate::macho::is_macho_file(&data_dir.join("liba_both.dylib")).unwrap());

    // Should recognize .so files.
    assert!(
        uv_delocate::macho::is_macho_file(
            &data_dir.join("np-1.24.1_arm_random__sfc64.cpython-311-darwin.so")
        )
        .unwrap()
    );

    // Non-existent files should return false.
    assert!(!uv_delocate::macho::is_macho_file(&data_dir.join("nonexistent.dylib")).unwrap());
}

#[test]
fn test_parse_single_arch_x86_64() {
    let data_dir = test_data_dir();
    let macho = uv_delocate::macho::parse_macho(&data_dir.join("liba.dylib")).unwrap();

    // Check architecture.
    assert!(macho.archs.contains(&Arch::X86_64));
    assert_eq!(macho.archs.len(), 1);

    // Check dependencies; should have system libs.
    let dep_names: Vec<&str> = macho.dependencies.iter().map(String::as_str).collect();
    assert!(dep_names.contains(&"/usr/lib/libc++.1.dylib"));
    assert!(dep_names.contains(&"/usr/lib/libSystem.B.dylib"));
}

#[test]
fn test_parse_single_arch_arm64() {
    let data_dir = test_data_dir();
    let macho = uv_delocate::macho::parse_macho(&data_dir.join("libam1.dylib")).unwrap();

    // Check architecture.
    assert!(macho.archs.contains(&Arch::Arm64));
    assert_eq!(macho.archs.len(), 1);
}

#[test]
fn test_parse_universal_binary() {
    let data_dir = test_data_dir();
    let macho = uv_delocate::macho::parse_macho(&data_dir.join("liba_both.dylib")).unwrap();

    // Should have both architectures.
    assert!(macho.archs.contains(&Arch::X86_64));
    assert!(macho.archs.contains(&Arch::Arm64));
    assert_eq!(macho.archs.len(), 2);
}

#[test]
fn test_parse_with_dependencies() {
    let data_dir = test_data_dir();

    // libb depends on liba.
    let macho = uv_delocate::macho::parse_macho(&data_dir.join("libb.dylib")).unwrap();
    let dep_names: Vec<&str> = macho.dependencies.iter().map(String::as_str).collect();
    assert!(dep_names.contains(&"liba.dylib"));

    // libc depends on liba and libb.
    let macho = uv_delocate::macho::parse_macho(&data_dir.join("libc.dylib")).unwrap();
    let dep_names: Vec<&str> = macho.dependencies.iter().map(String::as_str).collect();
    assert!(dep_names.contains(&"liba.dylib"));
    assert!(dep_names.contains(&"libb.dylib"));
}

#[test]
fn test_parse_with_rpath() {
    let data_dir = test_data_dir();
    let macho = uv_delocate::macho::parse_macho(&data_dir.join("libextfunc_rpath.dylib")).unwrap();

    // Should have @rpath dependencies.
    let dep_names: Vec<&str> = macho.dependencies.iter().map(String::as_str).collect();
    assert!(dep_names.contains(&"@rpath/libextfunc2_rpath.dylib"));

    // Should have rpaths.
    assert!(!macho.rpaths.is_empty());
    let rpath_set: HashSet<&str> = macho
        .rpaths
        .iter()
        .map(std::string::String::as_str)
        .collect();
    assert!(rpath_set.contains("@loader_path/"));
    assert!(rpath_set.contains("@executable_path/"));
}

#[test]
fn test_parse_numpy_extension() {
    let data_dir = test_data_dir();

    // x86_64 numpy extension.
    let macho = uv_delocate::macho::parse_macho(
        &data_dir.join("np-1.24.1_x86_64_random__sfc64.cpython-311-darwin.so"),
    )
    .unwrap();
    assert!(macho.archs.contains(&Arch::X86_64));

    // arm64 numpy extension.
    let macho = uv_delocate::macho::parse_macho(
        &data_dir.join("np-1.24.1_arm_random__sfc64.cpython-311-darwin.so"),
    )
    .unwrap();
    assert!(macho.archs.contains(&Arch::Arm64));
}
