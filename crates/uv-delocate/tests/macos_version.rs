//! Tests for macOS version parsing functionality.

use std::path::PathBuf;

use uv_delocate::MacOSVersion;

fn test_data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data")
}

#[test]
fn test_parse_macos_version_from_binary() {
    let data_dir = test_data_dir();

    // Parse a modern ARM64 binary.
    let macho = uv_delocate::macho::parse_macho(
        &data_dir.join("np-1.24.1_arm_random__sfc64.cpython-311-darwin.so"),
    )
    .unwrap();

    // ARM64 binaries require macOS 11.0 or later.
    if let Some(version) = macho.min_macos_version {
        assert!(version.major >= 11, "ARM64 binary should require macOS 11+");
    }
}

#[test]
fn test_parse_macos_version_from_x86_binary() {
    let data_dir = test_data_dir();

    // Parse an x86_64 binary.
    let macho = uv_delocate::macho::parse_macho(
        &data_dir.join("np-1.24.1_x86_64_random__sfc64.cpython-311-darwin.so"),
    )
    .unwrap();

    // x86_64 binaries can target older macOS versions.
    if let Some(version) = macho.min_macos_version {
        // Just verify we can read it.
        assert!(version.major >= 10);
    }
}

#[test]
fn test_macos_version_ordering() {
    let v10_9 = MacOSVersion::new(10, 9);
    let v10_15 = MacOSVersion::new(10, 15);
    let v11_0 = MacOSVersion::new(11, 0);
    let v14_0 = MacOSVersion::new(14, 0);

    assert!(v10_9 < v10_15);
    assert!(v10_15 < v11_0);
    assert!(v11_0 < v14_0);
    assert!(v10_9 < v14_0);
}

#[test]
fn test_macos_version_display() {
    assert_eq!(MacOSVersion::new(10, 9).to_string(), "10.9");
    assert_eq!(MacOSVersion::new(11, 0).to_string(), "11.0");
    assert_eq!(MacOSVersion::new(14, 2).to_string(), "14.2");
}
