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
    case: &str,
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
        "{case}: {filename}",
    );
}

#[test]
fn win_amd64_marker_reachability() {
    for (case, marker, expected) in [
        // An emulated interpreter needs the x64 wheel despite reporting the ARM64 host.
        ("windows_arm", Some(WINDOWS_ARM), false),
        // A marker without an operating system can still describe Windows ARM64.
        ("arm_without_os", Some("platform_machine == 'ARM64'"), false),
        // Environment constraints can use the lowercase ARM64 spelling.
        (
            "arm64_lowercase",
            Some("sys_platform == 'win32' and platform_machine == 'arm64'"),
            false,
        ),
        // The aarch64 alias must also identify Windows ARM64.
        (
            "aarch64_alias",
            Some("sys_platform == 'win32' and platform_machine == 'aarch64'"),
            false,
        ),
        // A Windows AMD64 environment can install a `win_amd64` wheel.
        (
            "windows_amd64",
            Some("sys_platform == 'win32' and platform_machine == 'AMD64'"),
            false,
        ),
        // An unconstrained marker cannot prove that a wheel is unreachable.
        ("unconstrained", None, false),
        // The Windows exception must not retain Windows wheels for Linux-only dependencies.
        (
            "linux_arm",
            Some("sys_platform == 'linux' and platform_machine == 'aarch64'"),
            true,
        ),
        // The Windows exception must not retain Windows wheels for macOS-only dependencies.
        (
            "macos_arm",
            Some("sys_platform == 'darwin' and platform_machine == 'arm64'"),
            true,
        ),
        // An impossible marker does not make a Windows wheel reachable.
        (
            "contradictory_os",
            Some("sys_platform == 'win32' and sys_platform == 'linux'"),
            true,
        ),
    ] {
        assert_reachability(
            case,
            "example-1.0-py3-none-win_amd64.whl",
            marker,
            ">=3.8",
            None,
            expected,
        );
    }
}

#[test]
fn explicit_tags_take_precedence() {
    for (case, filename, platform, expected) in [
        // Concrete x64 tags confirm that the emulated interpreter can install the wheel.
        (
            "explicit_x64_tags",
            "example-1.0-py3-none-win_amd64.whl",
            Some(Platform::new(Os::Windows, Arch::X86_64)),
            false,
        ),
        // Native ARM64 tags reject an x64 wheel before the marker exception applies.
        (
            "explicit_arm64_tags",
            "example-1.0-py3-none-win_amd64.whl",
            Some(Platform::new(Os::Windows, Arch::Aarch64)),
            true,
        ),
        // Concrete Linux tags reject a Windows wheel before the marker exception applies.
        (
            "explicit_linux_tags",
            "example-1.0-py3-none-win_amd64.whl",
            Some(Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 17,
                },
                Arch::X86_64,
            )),
            true,
        ),
        // Unknown platform tags cannot bypass a concrete compatibility check.
        (
            "unknown_only_concrete_tags",
            "example-1.0-py3-none-unrecognized_platform.whl",
            Some(Platform::new(Os::Windows, Arch::X86_64)),
            true,
        ),
    ] {
        assert_reachability(
            case,
            filename,
            Some(WINDOWS_ARM),
            ">=3.8",
            platform,
            expected,
        );
    }
}

#[test]
fn requires_python_takes_precedence() {
    for (case, filename, requires_python, expected) in [
        // The architecture exception must not retain a wheel for an excluded Python version.
        (
            "requires_python_rejection",
            "example-1.0-cp310-cp310-win_amd64.whl",
            ">=3.13",
            true,
        ),
        // An older stable-ABI wheel remains usable by a newer supported Python version.
        (
            "stable_abi_control",
            "example-1.0-cp39-abi3-win_amd64.whl",
            ">=3.13",
            false,
        ),
    ] {
        assert_reachability(
            case,
            filename,
            Some(WINDOWS_ARM),
            requires_python,
            None,
            expected,
        );
    }
}

#[test]
fn mixed_platform_tags() {
    for (case, filename) in [
        // An unknown tag must not obscure a reachable Windows x64 tag.
        (
            "unknown_plus_x64",
            "example-1.0-py3-none-unrecognized_platform.win_amd64.whl",
        ),
        // A wheel remains reachable when it also advertises the same architecture on Linux.
        (
            "compressed_cross_os_x64",
            "example-1.0-py3-none-win_amd64.manylinux_2_17_x86_64.whl",
        ),
        // A wheel can advertise both Windows architectures.
        (
            "compressed_windows_arches",
            "example-1.0-py3-none-win_amd64.win_arm64.whl",
        ),
        // An ARM tag for another operating system must not hide the Windows x64 tag.
        (
            "compressed_cross_os_arches",
            "example-1.0-py3-none-win_amd64.manylinux_2_17_aarch64.whl",
        ),
        // A universal tag remains usable when combined with a specific Windows tag.
        ("compressed_any", "example-1.0-py3-none-win_amd64.any.whl"),
    ] {
        assert_reachability(case, filename, Some(WINDOWS_ARM), ">=3.8", None, false);
    }
}

#[test]
fn unaffected_platform_wheels() {
    for (case, filename, marker, expected) in [
        // A Windows ARM64 environment can install a native `win_arm64` wheel.
        (
            "native_arm64",
            "example-1.0-py3-none-win_arm64.whl",
            Some(WINDOWS_ARM),
            false,
        ),
        // A 32-bit Windows environment can install a `win32` wheel.
        (
            "native_win32",
            "example-1.0-py3-none-win32.whl",
            Some("sys_platform == 'win32' and platform_machine == 'x86'"),
            false,
        ),
        // Universal wheels are reachable on every platform.
        (
            "universal_platform",
            "example-1.0-py3-none-any.whl",
            Some(WINDOWS_ARM),
            false,
        ),
        // Without concrete tags, an unknown platform cannot safely be ruled out.
        (
            "unknown_only",
            "example-1.0-py3-none-unrecognized_platform.whl",
            Some(WINDOWS_ARM),
            false,
        ),
        // Windows emulation support must not make Linux x64 wheels usable on Linux ARM.
        (
            "linux_x64_wheel_on_arm",
            "example-1.0-py3-none-manylinux_2_17_x86_64.whl",
            Some("sys_platform == 'linux' and platform_machine == 'aarch64'"),
            true,
        ),
    ] {
        assert_reachability(case, filename, marker, ">=3.8", None, expected);
    }
}
