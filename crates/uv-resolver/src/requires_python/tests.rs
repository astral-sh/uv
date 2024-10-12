use std::cmp::Ordering;
use std::collections::Bound;
use std::str::FromStr;

use uv_distribution_filename::WheelFilename;
use uv_pep440::{Version, VersionSpecifiers};

use crate::requires_python::{LowerBound, UpperBound};
use crate::RequiresPython;

#[test]
fn requires_python_included() {
    let version_specifiers = VersionSpecifiers::from_str("==3.10.*").unwrap();
    let requires_python = RequiresPython::from_specifiers(&version_specifiers).unwrap();
    let wheel_names = &[
        "bcrypt-4.1.3-cp37-abi3-macosx_10_12_universal2.whl",
        "black-24.4.2-cp310-cp310-win_amd64.whl",
        "black-24.4.2-cp310-none-win_amd64.whl",
        "cbor2-5.6.4-py3-none-any.whl",
        "solace_pubsubplus-1.8.0-py36-none-manylinux_2_12_x86_64.whl",
        "torch-1.10.0-py310-none-macosx_10_9_x86_64.whl",
        "torch-1.10.0-py37-none-macosx_10_9_x86_64.whl",
        "watchfiles-0.22.0-pp310-pypy310_pp73-macosx_11_0_arm64.whl",
    ];
    for wheel_name in wheel_names {
        assert!(
            requires_python.matches_wheel_tag(&WheelFilename::from_str(wheel_name).unwrap()),
            "{wheel_name}"
        );
    }

    let version_specifiers = VersionSpecifiers::from_str(">=3.12.3").unwrap();
    let requires_python = RequiresPython::from_specifiers(&version_specifiers).unwrap();
    let wheel_names = &["dearpygui-1.11.1-cp312-cp312-win_amd64.whl"];
    for wheel_name in wheel_names {
        assert!(
            requires_python.matches_wheel_tag(&WheelFilename::from_str(wheel_name).unwrap()),
            "{wheel_name}"
        );
    }

    let version_specifiers = VersionSpecifiers::from_str("==3.12.6").unwrap();
    let requires_python = RequiresPython::from_specifiers(&version_specifiers).unwrap();
    let wheel_names = &["lxml-5.3.0-cp312-cp312-musllinux_1_2_aarch64.whl"];
    for wheel_name in wheel_names {
        assert!(
            requires_python.matches_wheel_tag(&WheelFilename::from_str(wheel_name).unwrap()),
            "{wheel_name}"
        );
    }

    let version_specifiers = VersionSpecifiers::from_str("==3.12").unwrap();
    let requires_python = RequiresPython::from_specifiers(&version_specifiers).unwrap();
    let wheel_names = &["lxml-5.3.0-cp312-cp312-musllinux_1_2_x86_64.whl"];
    for wheel_name in wheel_names {
        assert!(
            requires_python.matches_wheel_tag(&WheelFilename::from_str(wheel_name).unwrap()),
            "{wheel_name}"
        );
    }
}

#[test]
fn requires_python_dropped() {
    let version_specifiers = VersionSpecifiers::from_str("==3.10.*").unwrap();
    let requires_python = RequiresPython::from_specifiers(&version_specifiers).unwrap();
    let wheel_names = &[
        "PySocks-1.7.1-py27-none-any.whl",
        "black-24.4.2-cp39-cp39-win_amd64.whl",
        "dearpygui-1.11.1-cp312-cp312-win_amd64.whl",
        "psutil-6.0.0-cp27-none-win32.whl",
        "psutil-6.0.0-cp36-cp36m-win32.whl",
        "pydantic_core-2.20.1-pp39-pypy39_pp73-win_amd64.whl",
        "torch-1.10.0-cp311-none-macosx_10_9_x86_64.whl",
        "torch-1.10.0-cp36-none-macosx_10_9_x86_64.whl",
        "torch-1.10.0-py311-none-macosx_10_9_x86_64.whl",
    ];
    for wheel_name in wheel_names {
        assert!(
            !requires_python.matches_wheel_tag(&WheelFilename::from_str(wheel_name).unwrap()),
            "{wheel_name}"
        );
    }

    let version_specifiers = VersionSpecifiers::from_str(">=3.12.3").unwrap();
    let requires_python = RequiresPython::from_specifiers(&version_specifiers).unwrap();
    let wheel_names = &["dearpygui-1.11.1-cp310-cp310-win_amd64.whl"];
    for wheel_name in wheel_names {
        assert!(
            !requires_python.matches_wheel_tag(&WheelFilename::from_str(wheel_name).unwrap()),
            "{wheel_name}"
        );
    }
}

#[test]
fn lower_bound_ordering() {
    let versions = &[
        // No bound
        LowerBound::new(Bound::Unbounded),
        // >=3.8
        LowerBound::new(Bound::Included(Version::new([3, 8]))),
        // >3.8
        LowerBound::new(Bound::Excluded(Version::new([3, 8]))),
        // >=3.8.1
        LowerBound::new(Bound::Included(Version::new([3, 8, 1]))),
        // >3.8.1
        LowerBound::new(Bound::Excluded(Version::new([3, 8, 1]))),
    ];
    for (i, v1) in versions.iter().enumerate() {
        for v2 in &versions[i + 1..] {
            assert_eq!(v1.cmp(v2), Ordering::Less, "less: {v1:?}\ngreater: {v2:?}");
        }
    }
}

#[test]
fn upper_bound_ordering() {
    let versions = &[
        // <3.8
        UpperBound::new(Bound::Excluded(Version::new([3, 8]))),
        // <=3.8
        UpperBound::new(Bound::Included(Version::new([3, 8]))),
        // <3.8.1
        UpperBound::new(Bound::Excluded(Version::new([3, 8, 1]))),
        // <=3.8.1
        UpperBound::new(Bound::Included(Version::new([3, 8, 1]))),
        // No bound
        UpperBound::new(Bound::Unbounded),
    ];
    for (i, v1) in versions.iter().enumerate() {
        for v2 in &versions[i + 1..] {
            assert_eq!(v1.cmp(v2), Ordering::Less, "less: {v1:?}\ngreater: {v2:?}");
        }
    }
}
