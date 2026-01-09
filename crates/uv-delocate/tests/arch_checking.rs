//! Tests for architecture checking functionality.

use std::path::PathBuf;

use uv_delocate::{Arch, DelocateError};

fn test_data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data")
}

#[test]
fn test_check_archs_single_present() {
    let data_dir = test_data_dir();
    // x86_64 only library.
    assert!(uv_delocate::macho::check_archs(&data_dir.join("liba.dylib"), &[Arch::X86_64]).is_ok());
}

#[test]
fn test_check_archs_single_missing() {
    let data_dir = test_data_dir();
    // x86_64 library doesn't have arm64.
    let result = uv_delocate::macho::check_archs(&data_dir.join("liba.dylib"), &[Arch::Arm64]);
    assert!(matches!(
        result,
        Err(DelocateError::MissingArchitecture { .. })
    ));
}

#[test]
fn test_check_archs_universal() {
    let data_dir = test_data_dir();
    // Universal binary should have both.
    assert!(
        uv_delocate::macho::check_archs(&data_dir.join("liba_both.dylib"), &[Arch::X86_64]).is_ok()
    );
    assert!(
        uv_delocate::macho::check_archs(&data_dir.join("liba_both.dylib"), &[Arch::Arm64]).is_ok()
    );
    assert!(
        uv_delocate::macho::check_archs(
            &data_dir.join("liba_both.dylib"),
            &[Arch::X86_64, Arch::Arm64]
        )
        .is_ok()
    );
}
