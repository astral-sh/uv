//! DO NOT EDIT
//!
//! Generated with ./scripts/scenarios/update.py
//! Scenarios from <https://github.com/zanieb/packse/tree/c35c57f5b4ab3381658661edbd0cd955680f9cda/scenarios>
//!
#![cfg(all(feature = "python", feature = "pypi"))]

use std::env;
use std::process::Command;

use anyhow::Result;
use assert_cmd::assert::OutputAssertExt;
use assert_fs::fixture::{FileWriteStr, PathChild};
use predicates::prelude::predicate;

use common::{create_bin_with_executables, get_bin, puffin_snapshot, TestContext, INSTA_FILTERS};

mod common;

/// Provision python binaries and return a `pip compile` command with options shared across all scenarios.
fn command(context: &TestContext, python_versions: &[&str]) -> Command {
    let bin = create_bin_with_executables(&context.temp_dir, python_versions)
        .expect("Failed to create bin dir");
    let mut command = Command::new(get_bin());
    command
        .arg("pip")
        .arg("compile")
        .arg("requirements.in")
        .arg("--extra-index-url")
        .arg("https://test.pypi.org/simple")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .env("VIRTUAL_ENV", context.venv.as_os_str())
        .env("PUFFIN_NO_WRAP", "1")
        .env("PUFFIN_TEST_PYTHON_PATH", bin)
        .current_dir(&context.temp_dir);
    command
}

/// requires-incompatible-python-version-compatible-override
///
/// The user requires a package which requires a Python version greater than the
/// current version, but they use an alternative Python version for package
/// resolution.
///
/// ```text
/// 3f4ac9b2
/// ├── environment
/// │   └── python3.9
/// ├── root
/// │   └── requires a==1.0.0
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
///         └── requires python>=3.10 (incompatible with environment)
/// ```
#[test]
fn requires_incompatible_python_version_compatible_override() -> Result<()> {
    let context = TestContext::new("3.9");
    let python_versions = &[];

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-3f4ac9b2", "albatross"));
    filters.push((r"-3f4ac9b2", ""));

    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str("a-3f4ac9b2==1.0.0")?;

    let output = puffin_snapshot!(filters, command(&context, python_versions)
        .arg("--python-version=3.11")
        , @r###"
                 success: true
                 exit_code: 0
                 ----- stdout -----
                 # This file was autogenerated by uv v[VERSION] via the following command:
                 #    puffin pip compile requirements.in --extra-index-url https://test.pypi.org/simple --cache-dir [CACHE_DIR] --python-version=3.11
                 albatross==1.0.0

                 ----- stderr -----
                 warning: The requested Python version 3.11 is not available; 3.9.18 will be used to build dependencies instead.
                 Resolved 1 package in [TIME]
                 "###
    );

    output
        .assert()
        .success()
        .stdout(predicate::str::contains("a-3f4ac9b2==1.0.0"));

    Ok(())
}

/// requires-compatible-python-version-incompatible-override
///
/// The user requires a package which requires a compatible Python version, but they
/// request an incompatible Python version for package resolution.
///
/// ```text
/// fd6db412
/// ├── environment
/// │   └── python3.11
/// ├── root
/// │   └── requires a==1.0.0
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
///         └── requires python>=3.10
/// ```
#[test]
fn requires_compatible_python_version_incompatible_override() -> Result<()> {
    let context = TestContext::new("3.11");
    let python_versions = &[];

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-fd6db412", "albatross"));
    filters.push((r"-fd6db412", ""));

    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str("a-fd6db412==1.0.0")?;

    let output = puffin_snapshot!(filters, command(&context, python_versions)
        .arg("--python-version=3.9")
        , @r###"
                 success: false
                 exit_code: 1
                 ----- stdout -----

                 ----- stderr -----
                 warning: The requested Python version 3.9 is not available; 3.11.7 will be used to build dependencies instead.
                   × No solution found when resolving dependencies:
                   ╰─▶ Because the requested Python version (3.9) does not satisfy Python>=3.10 and albatross==1.0.0 depends on Python>=3.10, we can conclude that albatross==1.0.0 cannot be used.
                       And because you require albatross==1.0.0, we can conclude that the requirements are unsatisfiable.
                 "###
    );

    output.assert().failure();

    Ok(())
}

/// requires-incompatible-python-version-compatible-override-no-wheels
///
/// The user requires a package which requires a incompatible Python version, but
/// they request a compatible Python version for package resolution. There are only
/// source distributions available for the package.
///
/// ```text
/// 3521037f
/// ├── environment
/// │   └── python3.9
/// ├── root
/// │   └── requires a==1.0.0
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
///         └── requires python>=3.10 (incompatible with environment)
/// ```
#[test]
fn requires_incompatible_python_version_compatible_override_no_wheels() -> Result<()> {
    let context = TestContext::new("3.9");
    let python_versions = &[];

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-3521037f", "albatross"));
    filters.push((r"-3521037f", ""));

    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str("a-3521037f==1.0.0")?;

    // Since there are no wheels for the package and it is not compatible with the
    // local installation, we cannot build the source distribution to determine its
    // dependencies.
    let output = puffin_snapshot!(filters, command(&context, python_versions)
        .arg("--python-version=3.11")
        , @r###"
                 success: false
                 exit_code: 1
                 ----- stdout -----

                 ----- stderr -----
                 warning: The requested Python version 3.11 is not available; 3.9.18 will be used to build dependencies instead.
                   × No solution found when resolving dependencies:
                   ╰─▶ Because the current Python version (3.9.18) does not satisfy Python>=3.10 and albatross==1.0.0 depends on Python>=3.10, we can conclude that albatross==1.0.0 cannot be used.
                       And because you require albatross==1.0.0, we can conclude that the requirements are unsatisfiable.
                 "###
    );

    output.assert().failure();

    Ok(())
}

/// requires-incompatible-python-version-compatible-override-no-wheels-available-system
///
/// The user requires a package which requires a incompatible Python version, but
/// they request a compatible Python version for package resolution. There are only
/// source distributions available for the package. The user has a compatible Python
/// version installed elsewhere on their system.
///
/// ```text
/// c68bcf5c
/// ├── environment
/// │   ├── python3.11
/// │   └── python3.9 (active)
/// ├── root
/// │   └── requires a==1.0.0
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
///         └── requires python>=3.10 (incompatible with environment)
/// ```
#[test]
fn requires_incompatible_python_version_compatible_override_no_wheels_available_system(
) -> Result<()> {
    let context = TestContext::new("3.9");
    let python_versions = &["3.11"];

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-c68bcf5c", "albatross"));
    filters.push((r"-c68bcf5c", ""));

    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str("a-c68bcf5c==1.0.0")?;

    // Since there is a compatible Python version available on the system, it should be
    // used to build the source distributions.
    let output = puffin_snapshot!(filters, command(&context, python_versions)
        .arg("--python-version=3.11")
        , @r###"
                 success: true
                 exit_code: 0
                 ----- stdout -----
                 # This file was autogenerated by uv v[VERSION] via the following command:
                 #    puffin pip compile requirements.in --extra-index-url https://test.pypi.org/simple --cache-dir [CACHE_DIR] --python-version=3.11
                 albatross==1.0.0

                 ----- stderr -----
                 Resolved 1 package in [TIME]
                 "###
    );

    output
        .assert()
        .success()
        .stdout(predicate::str::contains("a-c68bcf5c==1.0.0"));

    Ok(())
}

/// requires-incompatible-python-version-compatible-override-no-compatible-wheels
///
/// The user requires a package which requires a incompatible Python version, but
/// they request a compatible Python version for package resolution. There is a
/// wheel available for the package, but it does not have a compatible tag.
///
/// ```text
/// d7b25a2d
/// ├── environment
/// │   └── python3.9
/// ├── root
/// │   └── requires a==1.0.0
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
///         └── requires python>=3.10 (incompatible with environment)
/// ```
#[test]
fn requires_incompatible_python_version_compatible_override_no_compatible_wheels() -> Result<()> {
    let context = TestContext::new("3.9");
    let python_versions = &[];

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-d7b25a2d", "albatross"));
    filters.push((r"-d7b25a2d", ""));

    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str("a-d7b25a2d==1.0.0")?;

    // Since there are no compatible wheels for the package and it is not compatible
    // with the local installation, we cannot build the source distribution to
    // determine its dependencies.
    let output = puffin_snapshot!(filters, command(&context, python_versions)
        .arg("--python-version=3.11")
        , @r###"
                 success: false
                 exit_code: 1
                 ----- stdout -----

                 ----- stderr -----
                 warning: The requested Python version 3.11 is not available; 3.9.18 will be used to build dependencies instead.
                   × No solution found when resolving dependencies:
                   ╰─▶ Because the current Python version (3.9.18) does not satisfy Python>=3.10 and albatross==1.0.0 depends on Python>=3.10, we can conclude that albatross==1.0.0 cannot be used.
                       And because you require albatross==1.0.0, we can conclude that the requirements are unsatisfiable.
                 "###
    );

    output.assert().failure();

    Ok(())
}

/// requires-incompatible-python-version-compatible-override-other-wheel
///
/// The user requires a package which requires a incompatible Python version, but
/// they request a compatible Python version for package resolution. There are only
/// source distributions available for the compatible version of the package, but
/// there is an incompatible version with a wheel available.
///
/// ```text
/// a9179f0c
/// ├── environment
/// │   └── python3.9
/// ├── root
/// │   └── requires a
/// │       ├── satisfied by a-1.0.0
/// │       └── satisfied by a-2.0.0
/// └── a
///     ├── a-1.0.0
///     │   └── requires python>=3.10 (incompatible with environment)
///     └── a-2.0.0
///         └── requires python>=3.12 (incompatible with environment)
/// ```
#[test]
fn requires_incompatible_python_version_compatible_override_other_wheel() -> Result<()> {
    let context = TestContext::new("3.9");
    let python_versions = &[];

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-a9179f0c", "albatross"));
    filters.push((r"-a9179f0c", ""));

    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str("a-a9179f0c")?;

    // Since there are no wheels for the version of the package compatible with the
    // target and it is not compatible with the local installation, we cannot build the
    // source distribution to determine its dependencies. The other version has wheels
    // available, but is not compatible with the target version and cannot be used.
    let output = puffin_snapshot!(filters, command(&context, python_versions)
        .arg("--python-version=3.11")
        , @r###"
                 success: false
                 exit_code: 1
                 ----- stdout -----

                 ----- stderr -----
                 warning: The requested Python version 3.11 is not available; 3.9.18 will be used to build dependencies instead.
                   × No solution found when resolving dependencies:
                   ╰─▶ Because the current Python version (3.9.18) does not satisfy Python>=3.10 and albatross==1.0.0 depends on Python>=3.10, we can conclude that albatross==1.0.0 cannot be used.
                       And because only the following versions of albatross are available:
                           albatross==1.0.0
                           albatross==2.0.0
                       we can conclude that albatross<2.0.0 cannot be used. (1)

                       Because the requested Python version (3.11) does not satisfy Python>=3.12 and albatross==2.0.0 depends on Python>=3.12, we can conclude that albatross==2.0.0 cannot be used.
                       And because we know from (1) that albatross<2.0.0 cannot be used, we can conclude that all versions of albatross cannot be used.
                       And because you require albatross, we can conclude that the requirements are unsatisfiable.
                 "###
    );

    output.assert().failure();

    Ok(())
}

/// requires-python-patch-version-override-no-patch
///
/// The user requires a package which requires a Python version with a patch version
/// and the user provides a target version without a patch version.
///
/// ```text
/// e1884826
/// ├── environment
/// │   └── python3.8.18
/// ├── root
/// │   └── requires a==1.0.0
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
///         └── requires python>=3.8.4
/// ```
#[test]
fn requires_python_patch_version_override_no_patch() -> Result<()> {
    let context = TestContext::new("3.8.18");
    let python_versions = &[];

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-e1884826", "albatross"));
    filters.push((r"-e1884826", ""));

    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str("a-e1884826==1.0.0")?;

    // Since the resolver is asked to solve with 3.8, the minimum compatible Python
    // requirement is treated as 3.8.0.
    let output = puffin_snapshot!(filters, command(&context, python_versions)
        .arg("--python-version=3.8")
        , @r###"
                 success: false
                 exit_code: 1
                 ----- stdout -----

                 ----- stderr -----
                   × No solution found when resolving dependencies:
                   ╰─▶ Because the requested Python version (3.8) does not satisfy Python>=3.8.4 and albatross==1.0.0 depends on Python>=3.8.4, we can conclude that albatross==1.0.0 cannot be used.
                       And because you require albatross==1.0.0, we can conclude that the requirements are unsatisfiable.
                 "###
    );

    output.assert().failure();

    Ok(())
}

/// requires-python-patch-version-override-patch-compatible
///
/// The user requires a package which requires a Python version with a patch version
/// and the user provides a target version with a compatible patch version.
///
/// ```text
/// 91b4bcfc
/// ├── environment
/// │   └── python3.8.18
/// ├── root
/// │   └── requires a==1.0.0
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
///         └── requires python>=3.8.0
/// ```
#[test]
fn requires_python_patch_version_override_patch_compatible() -> Result<()> {
    let context = TestContext::new("3.8.18");
    let python_versions = &[];

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-91b4bcfc", "albatross"));
    filters.push((r"-91b4bcfc", ""));

    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str("a-91b4bcfc==1.0.0")?;

    let output = puffin_snapshot!(filters, command(&context, python_versions)
        .arg("--python-version=3.8.0")
        , @r###"
                 success: true
                 exit_code: 0
                 ----- stdout -----
                 # This file was autogenerated by uv v[VERSION] via the following command:
                 #    puffin pip compile requirements.in --extra-index-url https://test.pypi.org/simple --cache-dir [CACHE_DIR] --python-version=3.8.0
                 albatross==1.0.0

                 ----- stderr -----
                 warning: The requested Python version 3.8.0 is not available; 3.8.18 will be used to build dependencies instead.
                 Resolved 1 package in [TIME]
                 "###
    );

    output
        .assert()
        .success()
        .stdout(predicate::str::contains("a-91b4bcfc==1.0.0"));

    Ok(())
}
