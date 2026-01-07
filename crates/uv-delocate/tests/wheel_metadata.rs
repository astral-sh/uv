//! Tests for wheel metadata functionality.

use std::str::FromStr;

use uv_distribution_filename::WheelFilename;
use uv_platform_tags::{BinaryFormat, PlatformTag};

#[test]
fn test_wheel_filename_with_platform() {
    let filename = WheelFilename::from_str("foo-1.0-cp311-cp311-macosx_10_9_x86_64.whl").unwrap();

    // Test generating new filename with updated platform.
    let new_name = uv_delocate::wheel::filename_with_platform(
        &filename,
        &[PlatformTag::Macos {
            major: 11,
            minor: 0,
            binary_format: BinaryFormat::X86_64,
        }],
    );
    assert_eq!(new_name, "foo-1.0-cp311-cp311-macosx_11_0_x86_64.whl");
}
