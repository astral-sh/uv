//! Integration tests for wheel delocate functionality.

use std::path::PathBuf;

use tempfile::TempDir;
use uv_delocate::{Arch, DelocateOptions};

fn test_data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data")
}

#[test]
fn test_list_wheel_dependencies_pure_python() {
    let data_dir = test_data_dir();
    let deps =
        uv_delocate::list_wheel_dependencies(&data_dir.join("fakepkg2-1.0-py3-none-any.whl"))
            .unwrap();

    // Pure Python wheel should have no external dependencies.
    assert!(deps.is_empty());
}

#[test]
fn test_delocate_pure_python_wheel() {
    let data_dir = test_data_dir();
    let temp_dir = TempDir::new().unwrap();

    let result = uv_delocate::delocate_wheel(
        &data_dir.join("fakepkg2-1.0-py3-none-any.whl"),
        temp_dir.path(),
        &DelocateOptions::default(),
    )
    .unwrap();

    // Should just copy the wheel unchanged.
    assert!(result.exists());
    assert_eq!(result.file_name().unwrap(), "fakepkg2-1.0-py3-none-any.whl");
}

#[test]
fn test_delocate_wheel_with_require_archs() {
    let data_dir = test_data_dir();
    let temp_dir = TempDir::new().unwrap();

    // This should work because the wheel has universal2 binaries.
    let options = DelocateOptions {
        require_archs: vec![Arch::X86_64, Arch::Arm64],
        ..Default::default()
    };

    let result = uv_delocate::delocate_wheel(
        &data_dir.join("fakepkg1-1.0-cp36-abi3-macosx_10_9_universal2.whl"),
        temp_dir.path(),
        &options,
    );

    // The wheel should be processed (it may or may not have external deps).
    assert!(result.is_ok());
}
