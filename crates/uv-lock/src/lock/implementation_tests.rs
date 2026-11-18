use std::str::FromStr;

use uv_distribution_filename::WheelFilename;
use uv_distribution_types::RequiresPython;
use uv_pep440::VersionSpecifiers;
use uv_pep508::MarkerTree;
use uv_resolver_types::{ConflictMarker, UniversalMarker};

use super::is_wheel_unreachable_for_marker;

fn assert_reachability(
    case: &str,
    filename: &str,
    marker: Option<&str>,
    expected_unreachable: bool,
) {
    let filename = WheelFilename::from_str(filename)
        .unwrap_or_else(|err| panic!("valid wheel filename for {case}: {filename}: {err}"));
    let requires_python = RequiresPython::from_specifiers(
        VersionSpecifiers::from_str(">=3.8").expect("valid version specifiers"),
    );
    let marker = marker.map_or(UniversalMarker::TRUE, |marker| {
        UniversalMarker::new(
            MarkerTree::from_str(marker)
                .unwrap_or_else(|err| panic!("valid marker for {case}: {err}")),
            ConflictMarker::TRUE,
        )
    });

    assert_eq!(
        is_wheel_unreachable_for_marker(&filename, &requires_python, &marker, None),
        expected_unreachable,
        "{case}: {filename}",
    );
}

#[test]
fn implementation_markers_filter_locked_wheels() {
    let cpython_macos_arm = "example-1.0.0-cp312-cp312-macosx_11_0_arm64.whl";
    let cpython_macos_x86 = "example-1.0.0-cp312-cp312-macosx_10_9_x86_64.whl";
    let cpython_linux = "example-1.0.0-cp312-cp312-manylinux_2_17_x86_64.whl";
    let cpython_windows = "example-1.0.0-cp312-cp312-win_amd64.whl";
    let pypy_macos_arm = "example-1.0.0-pp312-pypy312_pp73-macosx_11_0_arm64.whl";
    let pypy_linux = "example-1.0.0-pp312-pypy312_pp73-manylinux_2_17_x86_64.whl";
    let pypy_windows = "example-1.0.0-pp312-pypy312_pp73-win_amd64.whl";
    let generic = "example-1.0.0-py3-none-any.whl";
    let mixed = "example-1.0.0-cp312.pp312-cp312.pypy312_pp73-manylinux_2_17_x86_64.whl";
    let graalpy = "example-1.0.0-graalpy312-graalpy240_312_native-manylinux_2_17_x86_64.whl";
    let pyston = "example-1.0.0-pyston38-pyston_23_x86_64_linux_gnu-manylinux_2_17_x86_64.whl";

    let split = "(implementation_name == 'cpython' and sys_platform == 'linux') or (implementation_name == 'pypy' and sys_platform == 'darwin')";
    let issue = "implementation_name == 'cpython' and sys_platform == 'darwin' and platform_machine == 'arm64'";
    let windows_arm_cpython = "implementation_name == 'cpython' and sys_platform == 'win32' and platform_machine == 'ARM64'";

    for (case, filename, marker, unreachable) in [
        // CPython environments omit PyPy wheels and keep CPython and generic wheels.
        (
            "cpython_drops_pypy",
            pypy_macos_arm,
            Some("implementation_name == 'cpython'"),
            true,
        ),
        (
            "cpython_keeps_cpython",
            cpython_macos_arm,
            Some("implementation_name == 'cpython'"),
            false,
        ),
        (
            "cpython_keeps_generic",
            generic,
            Some("implementation_name == 'cpython'"),
            false,
        ),
        // `platform_python_implementation` is the same interpreter fact.
        (
            "platform_impl_drops_pypy",
            pypy_linux,
            Some("platform_python_implementation == 'CPython'"),
            true,
        ),
        (
            "platform_impl_keeps_cpython",
            cpython_linux,
            Some("platform_python_implementation == 'CPython'"),
            false,
        ),
        (
            "pypy_platform_impl_drops_cpython",
            cpython_linux,
            Some("platform_python_implementation == 'PyPy'"),
            true,
        ),
        (
            "pypy_platform_impl_keeps_pypy",
            pypy_linux,
            Some("platform_python_implementation == 'PyPy'"),
            false,
        ),
        (
            "not_pypy_drops_pypy",
            pypy_linux,
            Some("implementation_name != 'pypy'"),
            true,
        ),
        (
            "not_pypy_keeps_cpython",
            cpython_linux,
            Some("implementation_name != 'pypy'"),
            false,
        ),
        (
            "not_cpython_platform_drops_cpython",
            cpython_linux,
            Some("platform_python_implementation != 'CPython'"),
            true,
        ),
        (
            "not_cpython_platform_keeps_pypy",
            pypy_linux,
            Some("platform_python_implementation != 'CPython'"),
            false,
        ),
        // An unconstrained lock still records every interpreter's wheels.
        ("unconstrained_pypy", pypy_linux, None, false),
        ("unconstrained_generic", generic, None, false),
        // Each environment split applies the interpreter and platform together.
        ("split_drops_pypy_linux", pypy_linux, Some(split), true),
        ("split_keeps_pypy_macos", pypy_macos_arm, Some(split), false),
        (
            "split_keeps_cpython_linux",
            cpython_linux,
            Some(split),
            false,
        ),
        (
            "split_drops_cpython_macos",
            cpython_macos_arm,
            Some(split),
            true,
        ),
        ("split_keeps_generic", generic, Some(split), false),
        // The reported environments also keep the existing platform filtering.
        ("issue_drops_pypy_arm", pypy_macos_arm, Some(issue), true),
        (
            "issue_keeps_cpython_arm",
            cpython_macos_arm,
            Some(issue),
            false,
        ),
        (
            "issue_drops_intel_macos",
            cpython_macos_x86,
            Some(issue),
            true,
        ),
        ("issue_drops_linux", cpython_linux, Some(issue), true),
        // Windows ARM still needs `win_amd64` wheels, but only for a compatible interpreter.
        (
            "windows_arm_keeps_cpython",
            cpython_windows,
            Some(windows_arm_cpython),
            false,
        ),
        (
            "windows_arm_drops_pypy",
            pypy_windows,
            Some(windows_arm_cpython),
            true,
        ),
        (
            "windows_arm_keeps_generic",
            "example-1.0.0-py3-none-win_amd64.whl",
            Some(windows_arm_cpython),
            false,
        ),
        // Other language tags follow the same rule.
        (
            "graalpy_dropped_for_cpython",
            graalpy,
            Some("implementation_name == 'cpython'"),
            true,
        ),
        (
            "graalpy_kept_for_graalpy",
            graalpy,
            Some("implementation_name == 'graalpy'"),
            false,
        ),
        (
            "graalpy_kept_for_platform_impl",
            graalpy,
            Some("platform_python_implementation == 'GraalVM'"),
            false,
        ),
        (
            "cpython_dropped_for_graalvm",
            cpython_linux,
            Some("platform_python_implementation == 'GraalVM'"),
            true,
        ),
        (
            "pyston_dropped_for_cpython_name",
            pyston,
            Some("implementation_name == 'cpython'"),
            true,
        ),
        (
            "pyston_kept_for_pyston_name",
            pyston,
            Some("implementation_name == 'pyston'"),
            false,
        ),
        // Pyston reports `platform_python_implementation == 'CPython'`, so that marker cannot
        // prove a `pyston*` wheel is unreachable.
        (
            "pyston_kept_for_cpython_platform_impl",
            pyston,
            Some("platform_python_implementation == 'CPython'"),
            false,
        ),
        // A wheel tagged for both interpreters survives either environment.
        (
            "mixed_kept_for_cpython",
            mixed,
            Some("implementation_name == 'cpython'"),
            false,
        ),
        (
            "mixed_kept_for_pypy",
            mixed,
            Some("implementation_name == 'pypy'"),
            false,
        ),
        (
            "mixed_dropped_for_graalpy",
            mixed,
            Some("implementation_name == 'graalpy'"),
            true,
        ),
    ] {
        assert_reachability(case, filename, marker, unreachable);
    }
}
