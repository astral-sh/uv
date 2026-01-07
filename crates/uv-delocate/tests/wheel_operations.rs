//! Tests for wheel packing and unpacking operations.

use std::path::PathBuf;

use fs_err as fs;
use tempfile::TempDir;

fn test_data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data")
}

#[test]
fn test_unpack_wheel() {
    let data_dir = test_data_dir();
    let temp_dir = TempDir::new().unwrap();

    uv_delocate::wheel::unpack_wheel(
        &data_dir.join("fakepkg1-1.0-cp36-abi3-macosx_10_9_universal2.whl"),
        temp_dir.path(),
    )
    .unwrap();

    // Check that dist-info exists.
    assert!(temp_dir.path().join("fakepkg1-1.0.dist-info").exists());
    assert!(
        temp_dir
            .path()
            .join("fakepkg1-1.0.dist-info/WHEEL")
            .exists()
    );
    assert!(
        temp_dir
            .path()
            .join("fakepkg1-1.0.dist-info/RECORD")
            .exists()
    );

    // Check that package files exist.
    assert!(temp_dir.path().join("fakepkg1").exists());
}

#[test]
fn test_find_dist_info() {
    let data_dir = test_data_dir();
    let temp_dir = TempDir::new().unwrap();

    uv_delocate::wheel::unpack_wheel(
        &data_dir.join("fakepkg1-1.0-cp36-abi3-macosx_10_9_universal2.whl"),
        temp_dir.path(),
    )
    .unwrap();

    let dist_info = uv_delocate::wheel::find_dist_info(temp_dir.path()).unwrap();
    assert_eq!(dist_info, "fakepkg1-1.0.dist-info");
}

#[test]
fn test_unpack_repack_wheel() {
    let data_dir = test_data_dir();
    let temp_dir = TempDir::new().unwrap();
    let unpack_dir = temp_dir.path().join("unpacked");
    let output_wheel = temp_dir.path().join("output.whl");

    fs::create_dir(&unpack_dir).unwrap();

    // Unpack.
    uv_delocate::wheel::unpack_wheel(&data_dir.join("fakepkg2-1.0-py3-none-any.whl"), &unpack_dir)
        .unwrap();

    // Repack.
    uv_delocate::wheel::pack_wheel(&unpack_dir, &output_wheel).unwrap();

    // Verify the output is a valid zip.
    assert!(output_wheel.exists());
    let file = fs::File::open(&output_wheel).unwrap();
    let archive = zip::ZipArchive::new(file).unwrap();
    assert!(!archive.is_empty());
}
