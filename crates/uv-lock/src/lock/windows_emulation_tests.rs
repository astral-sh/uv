use std::str::FromStr;

use uv_distribution_filename::WheelFilename;
use uv_distribution_types::RequiresPython;
use uv_pep440::VersionSpecifiers;
use uv_pep508::MarkerTree;
use uv_platform_tags::{Arch, Os, Platform, Tags, TagsOptions};
use uv_resolver_types::{ConflictMarker, UniversalMarker};

use super::is_wheel_unreachable_for_marker;

const WINDOWS_ARM: &str =
    "os_name == 'nt' and sys_platform == 'win32' and platform_machine == 'ARM64'";

fn assert_reachability(
    filename: &str,
    marker: Option<&str>,
    requires_python: &str,
    platform: Option<Platform>,
    expected_unreachable: bool,
) {
    let filename = WheelFilename::from_str(filename).expect("valid wheel filename");
    let requires_python = RequiresPython::from_specifiers(
        VersionSpecifiers::from_str(requires_python).expect("valid version specifiers"),
    );
    let marker = marker.map_or(UniversalMarker::TRUE, |marker| {
        UniversalMarker::new(
            MarkerTree::from_str(marker).expect("valid marker"),
            ConflictMarker::TRUE,
        )
    });
    let tags = platform.map(|platform| {
        Tags::from_env(
            platform,
            (3, 13),
            "cpython",
            (3, 13),
            TagsOptions::default(),
        )
        .expect("valid explicit platform tags")
    });

    assert_eq!(
        is_wheel_unreachable_for_marker(&filename, &requires_python, &marker, tags.as_ref()),
        expected_unreachable,
    );
}

macro_rules! reachability_case {
    ($name:ident, $filename:literal, $marker:expr, $expected:literal) => {
        reachability_case!($name, $filename, $marker, ">=3.8", None, $expected);
    };
    ($name:ident, $filename:literal, $marker:expr, $requires_python:literal, $platform:expr, $expected:literal) => {
        #[test]
        fn $name() {
            assert_reachability($filename, $marker, $requires_python, $platform, $expected);
        }
    };
}

reachability_case!(
    windows_arm,
    "example-1.0-py3-none-win_amd64.whl",
    Some(WINDOWS_ARM),
    false
);

reachability_case!(
    arm_without_os,
    "example-1.0-py3-none-win_amd64.whl",
    Some("platform_machine == 'ARM64'"),
    false
);

reachability_case!(
    arm64_lowercase,
    "example-1.0-py3-none-win_amd64.whl",
    Some("sys_platform == 'win32' and platform_machine == 'arm64'"),
    false
);

reachability_case!(
    aarch64_alias,
    "example-1.0-py3-none-win_amd64.whl",
    Some("sys_platform == 'win32' and platform_machine == 'aarch64'"),
    false
);

reachability_case!(
    windows_amd64,
    "example-1.0-py3-none-win_amd64.whl",
    Some("sys_platform == 'win32' and platform_machine == 'AMD64'"),
    false
);

reachability_case!(
    unconstrained,
    "example-1.0-py3-none-win_amd64.whl",
    None,
    false
);

reachability_case!(
    native_arm64,
    "example-1.0-py3-none-win_arm64.whl",
    Some(WINDOWS_ARM),
    false
);

reachability_case!(
    linux_arm,
    "example-1.0-py3-none-win_amd64.whl",
    Some("sys_platform == 'linux' and platform_machine == 'aarch64'"),
    true
);

reachability_case!(
    macos_arm,
    "example-1.0-py3-none-win_amd64.whl",
    Some("sys_platform == 'darwin' and platform_machine == 'arm64'"),
    true
);

reachability_case!(
    linux_x64_wheel_on_arm,
    "example-1.0-py3-none-manylinux_2_17_x86_64.whl",
    Some("sys_platform == 'linux' and platform_machine == 'aarch64'"),
    true
);

reachability_case!(
    contradictory_os,
    "example-1.0-py3-none-win_amd64.whl",
    Some("sys_platform == 'win32' and sys_platform == 'linux'"),
    true
);

reachability_case!(
    explicit_x64_tags,
    "example-1.0-py3-none-win_amd64.whl",
    Some(WINDOWS_ARM),
    ">=3.8",
    Some(Platform::new(Os::Windows, Arch::X86_64)),
    false
);

reachability_case!(
    explicit_arm64_tags,
    "example-1.0-py3-none-win_amd64.whl",
    Some(WINDOWS_ARM),
    ">=3.8",
    Some(Platform::new(Os::Windows, Arch::Aarch64)),
    true
);

reachability_case!(
    explicit_linux_tags,
    "example-1.0-py3-none-win_amd64.whl",
    Some(WINDOWS_ARM),
    ">=3.8",
    Some(Platform::new(
        Os::Manylinux {
            major: 2,
            minor: 17
        },
        Arch::X86_64,
    )),
    true
);

reachability_case!(
    requires_python_rejection,
    "example-1.0-cp310-cp310-win_amd64.whl",
    Some(WINDOWS_ARM),
    ">=3.13",
    None,
    true
);

reachability_case!(
    stable_abi_control,
    "example-1.0-cp39-abi3-win_amd64.whl",
    Some(WINDOWS_ARM),
    ">=3.13",
    None,
    false
);

reachability_case!(
    universal_platform,
    "example-1.0-py3-none-any.whl",
    Some(WINDOWS_ARM),
    false
);

reachability_case!(
    unknown_only,
    "example-1.0-py3-none-unrecognized_platform.whl",
    Some(WINDOWS_ARM),
    false
);

reachability_case!(
    unknown_only_concrete_tags,
    "example-1.0-py3-none-unrecognized_platform.whl",
    Some(WINDOWS_ARM),
    ">=3.8",
    Some(Platform::new(Os::Windows, Arch::X86_64)),
    true
);

reachability_case!(
    unknown_plus_x64,
    "example-1.0-py3-none-unrecognized_platform.win_amd64.whl",
    Some(WINDOWS_ARM),
    false
);

reachability_case!(
    compressed_cross_os_x64,
    "example-1.0-py3-none-win_amd64.manylinux_2_17_x86_64.whl",
    Some(WINDOWS_ARM),
    false
);

reachability_case!(
    compressed_windows_arches,
    "example-1.0-py3-none-win_amd64.win_arm64.whl",
    Some(WINDOWS_ARM),
    false
);

reachability_case!(
    compressed_cross_os_arches,
    "example-1.0-py3-none-win_amd64.manylinux_2_17_aarch64.whl",
    Some(WINDOWS_ARM),
    false
);

reachability_case!(
    compressed_any,
    "example-1.0-py3-none-win_amd64.any.whl",
    Some(WINDOWS_ARM),
    false
);

reachability_case!(
    native_win32,
    "example-1.0-py3-none-win32.whl",
    Some("sys_platform == 'win32' and platform_machine == 'x86'"),
    false
);
