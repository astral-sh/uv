//! DO NOT EDIT
//!
//! Generated with ./scripts/scenarios/update.py
//! Scenarios from <https://github.com/zanieb/packse/tree/8826f9740703779911d0fcd6eba8d56af0eb3adb/scenarios>
//!
#![cfg(all(feature = "python", feature = "pypi"))]

use std::path::Path;
use std::process::Command;

use anyhow::Result;
use assert_cmd::assert::Assert;
use assert_cmd::prelude::*;
use insta_cmd::_macro_support::insta;
use insta_cmd::{assert_cmd_snapshot, get_cargo_bin};

use common::{create_venv, BIN_NAME, INSTA_FILTERS};

mod common;

fn assert_command(venv: &Path, command: &str, temp_dir: &Path) -> Assert {
    Command::new(venv.join("bin").join("python"))
        .arg("-c")
        .arg(command)
        .current_dir(temp_dir)
        .assert()
}

fn assert_installed(venv: &Path, package: &'static str, version: &'static str, temp_dir: &Path) {
    assert_command(
        venv,
        format!("import {package} as package; print(package.__version__, end='')").as_str(),
        temp_dir,
    )
    .success()
    .stdout(version);
}

fn assert_not_installed(venv: &Path, package: &'static str, temp_dir: &Path) {
    assert_command(venv, format!("import {package}").as_str(), temp_dir).failure();
}

/// excluded-only-version
///
/// Only one version of the requested package is available, but the user has banned
/// that version.
///
/// excluded-only-version-7a9ed79c
/// ├── environment
/// │   └── python3.7
/// ├── root
/// │   └── requires a!=1.0.0
/// │       └── unsatisfied: no matching version
/// └── a
///     └── a-1.0.0
#[test]
fn excluded_only_version() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"excluded-only-version-7a9ed79c-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("excluded-only-version-7a9ed79c-a!=1.0.0")
            .arg("--extra-index-url")
            .arg("https://test.pypi.org/simple")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .env("PUFFIN_NO_WRAP", "1")
            .current_dir(&temp_dir), @r###"
        success: false
        exit_code: 1
        ----- stdout -----

        ----- stderr -----
          × No solution found when resolving dependencies:
          ╰─▶ Because there are no versions of a that satisfy any of:
                  a<1.0.0
                  a>1.0.0
              and root depends on one of:
                  a<1.0.0
                  a>1.0.0
              we can conclude that the requirements are unsatisfiable.
        "###);
    });

    // Only `a==1.0.0` is available but the user excluded it.
    assert_not_installed(&venv, "excluded_only_version_7a9ed79c_a", &temp_dir);

    Ok(())
}

/// excluded-only-compatible-version
///
/// Only one version of the requested package `a` is compatible, but the user has
/// banned that version.
///
/// excluded-only-compatible-version-d28c9e3c
/// ├── environment
/// │   └── python3.7
/// ├── root
/// │   ├── requires a!=2.0.0
/// │   │   ├── satisfied by a-1.0.0
/// │   │   └── satisfied by a-3.0.0
/// │   └── requires b>=2.0.0,<3.0.0
/// │       └── satisfied by b-2.0.0
/// ├── a
/// │   ├── a-1.0.0
/// │   │   └── requires b==1.0.0
/// │   │       └── satisfied by b-1.0.0
/// │   ├── a-2.0.0
/// │   │   └── requires b==2.0.0
/// │   │       └── satisfied by b-2.0.0
/// │   └── a-3.0.0
/// │       └── requires b==3.0.0
/// │           └── satisfied by b-3.0.0
/// └── b
///     ├── b-1.0.0
///     ├── b-2.0.0
///     └── b-3.0.0
#[test]
fn excluded_only_compatible_version() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"excluded-only-compatible-version-d28c9e3c-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("excluded-only-compatible-version-d28c9e3c-a!=2.0.0")
            .arg("excluded-only-compatible-version-d28c9e3c-b>=2.0.0,<3.0.0")
            .arg("--extra-index-url")
            .arg("https://test.pypi.org/simple")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .env("PUFFIN_NO_WRAP", "1")
            .current_dir(&temp_dir), @r###"
        success: false
        exit_code: 1
        ----- stdout -----

        ----- stderr -----
          × No solution found when resolving dependencies:
          ╰─▶ Because there are no versions of a that satisfy any of:
                  a<1.0.0
                  a>1.0.0,<2.0.0
                  a>2.0.0,<3.0.0
                  a>3.0.0
              and a==1.0.0 depends on b==1.0.0, we can conclude that a<2.0.0 depends on b==1.0.0.
              And because a==3.0.0 depends on b==3.0.0 we can conclude that any of:
                  a<2.0.0
                  a>2.0.0
              depends on one of:
                  b<=1.0.0
                  b>=3.0.0

              And because root depends on b>=2.0.0,<3.0.0 and root depends on one of:
                  a<2.0.0
                  a>2.0.0
              we can conclude that the requirements are unsatisfiable.
        "###);
    });

    // Only `a==1.2.0` is available since `a==1.0.0` and `a==3.0.0` require
    // incompatible versions of `b`. The user has excluded that version of `a` so
    // resolution fails.
    assert_not_installed(
        &venv,
        "excluded_only_compatible_version_d28c9e3c_a",
        &temp_dir,
    );
    assert_not_installed(
        &venv,
        "excluded_only_compatible_version_d28c9e3c_b",
        &temp_dir,
    );

    Ok(())
}

/// dependency-excludes-range-of-compatible-versions
///
/// There is a range of compatible versions for the requested package `a`, but
/// another dependency `c` excludes that range.
///
/// dependency-excludes-range-of-compatible-versions-2023222f
/// ├── environment
/// │   └── python3.7
/// ├── root
/// │   ├── requires a
/// │   │   ├── satisfied by a-1.0.0
/// │   │   ├── satisfied by a-2.0.0
/// │   │   ├── satisfied by a-2.1.0
/// │   │   ├── satisfied by a-2.2.0
/// │   │   ├── satisfied by a-2.3.0
/// │   │   └── satisfied by a-3.0.0
/// │   ├── requires b>=2.0.0,<3.0.0
/// │   │   └── satisfied by b-2.0.0
/// │   └── requires c
/// │       ├── satisfied by c-1.0.0
/// │       └── satisfied by c-2.0.0
/// ├── a
/// │   ├── a-1.0.0
/// │   │   └── requires b==1.0.0
/// │   │       └── satisfied by b-1.0.0
/// │   ├── a-2.0.0
/// │   │   └── requires b==2.0.0
/// │   │       └── satisfied by b-2.0.0
/// │   ├── a-2.1.0
/// │   │   └── requires b==2.0.0
/// │   │       └── satisfied by b-2.0.0
/// │   ├── a-2.2.0
/// │   │   └── requires b==2.0.0
/// │   │       └── satisfied by b-2.0.0
/// │   ├── a-2.3.0
/// │   │   └── requires b==2.0.0
/// │   │       └── satisfied by b-2.0.0
/// │   └── a-3.0.0
/// │       └── requires b==3.0.0
/// │           └── satisfied by b-3.0.0
/// ├── b
/// │   ├── b-1.0.0
/// │   ├── b-2.0.0
/// │   └── b-3.0.0
/// └── c
///     ├── c-1.0.0
///     │   └── requires a<2.0.0
///     │       └── satisfied by a-1.0.0
///     └── c-2.0.0
///         └── requires a>=3.0.0
///             └── satisfied by a-3.0.0
#[test]
fn dependency_excludes_range_of_compatible_versions() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((
        r"dependency-excludes-range-of-compatible-versions-2023222f-",
        "",
    ));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("dependency-excludes-range-of-compatible-versions-2023222f-a")
            .arg("dependency-excludes-range-of-compatible-versions-2023222f-b>=2.0.0,<3.0.0")
            .arg("dependency-excludes-range-of-compatible-versions-2023222f-c")
            .arg("--extra-index-url")
            .arg("https://test.pypi.org/simple")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .env("PUFFIN_NO_WRAP", "1")
            .current_dir(&temp_dir), @r###"
        success: false
        exit_code: 1
        ----- stdout -----

        ----- stderr -----
          × No solution found when resolving dependencies:
          ╰─▶ Because there are no versions of a that satisfy any of:
                  a<1.0.0
                  a>1.0.0,<2.0.0
                  a>3.0.0
              and a==1.0.0 depends on b==1.0.0, we can conclude that a<2.0.0 depends on b==1.0.0. (1)

              Because there are no versions of c that satisfy any of:
                  c<1.0.0
                  c>1.0.0,<2.0.0
                  c>2.0.0
              and c==1.0.0 depends on a<2.0.0, we can conclude that c<2.0.0 depends on a<2.0.0.
              And because c==2.0.0 depends on a>=3.0.0 we can conclude that c depends on one of:
                  a<2.0.0
                  a>=3.0.0

              And because we know from (1) that a<2.0.0 depends on b==1.0.0, we can conclude that a!=3.0.0, c*, b!=1.0.0 are incompatible.
              And because a==3.0.0 depends on b==3.0.0 we can conclude that c depends on one of:
                  b<=1.0.0
                  b>=3.0.0

              And because root depends on c and root depends on b>=2.0.0,<3.0.0, we can conclude that the requirements are unsatisfiable.
        "###);
    });

    // Only the `2.x` versions of `a` are available since `a==1.0.0` and `a==3.0.0`
    // require incompatible versions of `b`, but all available versions of `c` exclude
    // that range of `a` so resolution fails.
    assert_not_installed(
        &venv,
        "dependency_excludes_range_of_compatible_versions_2023222f_a",
        &temp_dir,
    );
    assert_not_installed(
        &venv,
        "dependency_excludes_range_of_compatible_versions_2023222f_b",
        &temp_dir,
    );
    assert_not_installed(
        &venv,
        "dependency_excludes_range_of_compatible_versions_2023222f_c",
        &temp_dir,
    );

    Ok(())
}

/// dependency-excludes-non-contiguous-range-of-compatible-versions
///
/// There is a non-contiguous range of compatible versions for the requested package
/// `a`, but another dependency `c` excludes the range. This is the same as
/// `dependency-excludes-range-of-compatible-versions` but some of the versions of
/// `a` are incompatible for another reason e.g. dependency on non-existant package
/// `d`.
///
/// dependency-excludes-non-contiguous-range-of-compatible-versions-aece4208
/// ├── environment
/// │   └── python3.7
/// ├── root
/// │   ├── requires a
/// │   │   ├── satisfied by a-1.0.0
/// │   │   ├── satisfied by a-2.0.0
/// │   │   ├── satisfied by a-2.1.0
/// │   │   ├── satisfied by a-2.2.0
/// │   │   ├── satisfied by a-2.3.0
/// │   │   ├── satisfied by a-2.4.0
/// │   │   └── satisfied by a-3.0.0
/// │   ├── requires b>=2.0.0,<3.0.0
/// │   │   └── satisfied by b-2.0.0
/// │   └── requires c
/// │       ├── satisfied by c-1.0.0
/// │       └── satisfied by c-2.0.0
/// ├── a
/// │   ├── a-1.0.0
/// │   │   └── requires b==1.0.0
/// │   │       └── satisfied by b-1.0.0
/// │   ├── a-2.0.0
/// │   │   └── requires b==2.0.0
/// │   │       └── satisfied by b-2.0.0
/// │   ├── a-2.1.0
/// │   │   ├── requires b==2.0.0
/// │   │   │   └── satisfied by b-2.0.0
/// │   │   └── requires d
/// │   │       └── unsatisfied: no versions for package
/// │   ├── a-2.2.0
/// │   │   └── requires b==2.0.0
/// │   │       └── satisfied by b-2.0.0
/// │   ├── a-2.3.0
/// │   │   ├── requires b==2.0.0
/// │   │   │   └── satisfied by b-2.0.0
/// │   │   └── requires d
/// │   │       └── unsatisfied: no versions for package
/// │   ├── a-2.4.0
/// │   │   └── requires b==2.0.0
/// │   │       └── satisfied by b-2.0.0
/// │   └── a-3.0.0
/// │       └── requires b==3.0.0
/// │           └── satisfied by b-3.0.0
/// ├── b
/// │   ├── b-1.0.0
/// │   ├── b-2.0.0
/// │   └── b-3.0.0
/// └── c
///     ├── c-1.0.0
///     │   └── requires a<2.0.0
///     │       └── satisfied by a-1.0.0
///     └── c-2.0.0
///         └── requires a>=3.0.0
///             └── satisfied by a-3.0.0
#[test]
fn dependency_excludes_non_contiguous_range_of_compatible_versions() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((
        r"dependency-excludes-non-contiguous-range-of-compatible-versions-aece4208-",
        "",
    ));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("dependency-excludes-non-contiguous-range-of-compatible-versions-aece4208-a")
            .arg("dependency-excludes-non-contiguous-range-of-compatible-versions-aece4208-b>=2.0.0,<3.0.0")
            .arg("dependency-excludes-non-contiguous-range-of-compatible-versions-aece4208-c")
            .arg("--extra-index-url")
            .arg("https://test.pypi.org/simple")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .env("PUFFIN_NO_WRAP", "1")
            .current_dir(&temp_dir), @r###"
        success: false
        exit_code: 1
        ----- stdout -----

        ----- stderr -----
          × No solution found when resolving dependencies:
          ╰─▶ Because a==1.0.0 depends on b==1.0.0 and there are no versions of a that satisfy any of:
                  a<1.0.0
                  a>1.0.0,<2.0.0
                  a>3.0.0
              we can conclude that a<2.0.0 depends on b==1.0.0.
              And because a==3.0.0 depends on b==3.0.0 we can conclude that any of:
                  a<2.0.0
                  a>=3.0.0
              depends on one of:
                  b<=1.0.0
                  b>=3.0.0
               (1)

              Because there are no versions of c that satisfy any of:
                  c<1.0.0
                  c>1.0.0,<2.0.0
                  c>2.0.0
              and c==1.0.0 depends on a<2.0.0, we can conclude that c<2.0.0 depends on a<2.0.0.
              And because c==2.0.0 depends on a>=3.0.0 we can conclude that c depends on one of:
                  a<2.0.0
                  a>=3.0.0

              And because we know from (1) that any of:
                  a<2.0.0
                  a>=3.0.0
              depends on one of:
                  b<=1.0.0
                  b>=3.0.0
              we can conclude that c depends on one of:
                  b<=1.0.0
                  b>=3.0.0

              And because root depends on b>=2.0.0,<3.0.0 and root depends on c, we can conclude that the requirements are unsatisfiable.
        "###);
    });

    // Only the `2.x` versions of `a` are available since `a==1.0.0` and `a==3.0.0`
    // require incompatible versions of `b`, but all available versions of `c` exclude
    // that range of `a` so resolution fails.
    assert_not_installed(
        &venv,
        "dependency_excludes_non_contiguous_range_of_compatible_versions_aece4208_a",
        &temp_dir,
    );
    assert_not_installed(
        &venv,
        "dependency_excludes_non_contiguous_range_of_compatible_versions_aece4208_b",
        &temp_dir,
    );
    assert_not_installed(
        &venv,
        "dependency_excludes_non_contiguous_range_of_compatible_versions_aece4208_c",
        &temp_dir,
    );

    Ok(())
}

/// requires-package-only-prereleases
///
/// The user requires any version of package `a` which only has prerelease versions
/// available.
///
/// requires-package-only-prereleases-a8b21d15
/// ├── environment
/// │   └── python3.7
/// ├── root
/// │   └── requires a
/// │       └── unsatisfied: no matching version
/// └── a
///     └── a-1.0.0a1
#[test]
fn requires_package_only_prereleases() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"requires-package-only-prereleases-a8b21d15-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-package-only-prereleases-a8b21d15-a")
            .arg("--extra-index-url")
            .arg("https://test.pypi.org/simple")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .env("PUFFIN_NO_WRAP", "1")
            .current_dir(&temp_dir), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Resolved 1 package in [TIME]
        Downloaded 1 package in [TIME]
        Installed 1 package in [TIME]
         + a==1.0.0a1
        "###);
    });

    // Since there are only prerelease versions of `a` available, it should be
    // installed even though the user did not include a prerelease specifier.
    assert_installed(
        &venv,
        "requires_package_only_prereleases_a8b21d15_a",
        "1.0.0a1",
        &temp_dir,
    );

    Ok(())
}

/// requires-package-only-prereleases-in-range
///
/// The user requires a version of package `a` which only matches prerelease
/// versions but they did not include a prerelease specifier.
///
/// requires-package-only-prereleases-in-range-b4df71d2
/// ├── environment
/// │   └── python3.7
/// ├── root
/// │   └── requires a>0.1.0
/// │       └── unsatisfied: no matching version
/// └── a
///     ├── a-0.1.0
///     └── a-1.0.0a1
#[test]
fn requires_package_only_prereleases_in_range() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"requires-package-only-prereleases-in-range-b4df71d2-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-package-only-prereleases-in-range-b4df71d2-a>0.1.0")
            .arg("--extra-index-url")
            .arg("https://test.pypi.org/simple")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .env("PUFFIN_NO_WRAP", "1")
            .current_dir(&temp_dir), @r###"
        success: false
        exit_code: 1
        ----- stdout -----

        ----- stderr -----
          × No solution found when resolving dependencies:
          ╰─▶ Because there are no versions of a that satisfy a>0.1.0 and root depends on a>0.1.0, we can conclude that the requirements are unsatisfiable.

              hint: Pre-releases are available for a in the requested range (e.g., 1.0.0a1), but pre-releases weren't enabled (try: `--prerelease=allow`)
        "###);
    });

    // Since there are stable versions of `a` available, prerelease versions should not
    // be selected without explicit opt-in.
    assert_not_installed(
        &venv,
        "requires_package_only_prereleases_in_range_b4df71d2_a",
        &temp_dir,
    );

    Ok(())
}

/// requires-package-only-prereleases-in-range-global-opt-in
///
/// The user requires a version of package `a` which only matches prerelease
/// versions. They did not include a prerelease specifier for the package, but they
/// opted into prereleases globally.
///
/// requires-package-only-prereleases-in-range-global-opt-in-51f94da2
/// ├── environment
/// │   └── python3.7
/// ├── root
/// │   └── requires a>0.1.0
/// │       └── unsatisfied: no matching version
/// └── a
///     ├── a-0.1.0
///     └── a-1.0.0a1
#[test]
fn requires_package_only_prereleases_in_range_global_opt_in() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((
        r"requires-package-only-prereleases-in-range-global-opt-in-51f94da2-",
        "",
    ));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-package-only-prereleases-in-range-global-opt-in-51f94da2-a>0.1.0")
            .arg("--prerelease=allow")
            .arg("--extra-index-url")
            .arg("https://test.pypi.org/simple")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .env("PUFFIN_NO_WRAP", "1")
            .current_dir(&temp_dir), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Resolved 1 package in [TIME]
        Downloaded 1 package in [TIME]
        Installed 1 package in [TIME]
         + a==1.0.0a1
        "###);
    });

    assert_installed(
        &venv,
        "requires_package_only_prereleases_in_range_global_opt_in_51f94da2_a",
        "1.0.0a1",
        &temp_dir,
    );

    Ok(())
}

/// requires-package-prerelease-and-final-any
///
/// The user requires any version of package `a` has a prerelease version available
/// and an older non-prerelease version.
///
/// requires-package-prerelease-and-final-any-eebe53a6
/// ├── environment
/// │   └── python3.7
/// ├── root
/// │   └── requires a
/// │       └── satisfied by a-0.1.0
/// └── a
///     ├── a-0.1.0
///     └── a-1.0.0a1
#[test]
fn requires_package_prerelease_and_final_any() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"requires-package-prerelease-and-final-any-eebe53a6-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-package-prerelease-and-final-any-eebe53a6-a")
            .arg("--extra-index-url")
            .arg("https://test.pypi.org/simple")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .env("PUFFIN_NO_WRAP", "1")
            .current_dir(&temp_dir), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Resolved 1 package in [TIME]
        Downloaded 1 package in [TIME]
        Installed 1 package in [TIME]
         + a==0.1.0
        "###);
    });

    // Since the user did not provide a prerelease specifier, the older stable version
    // should be selected.
    assert_installed(
        &venv,
        "requires_package_prerelease_and_final_any_eebe53a6_a",
        "0.1.0",
        &temp_dir,
    );

    Ok(())
}

/// requires-package-prerelease-specified-only-final-available
///
/// The user requires a version of `a` with a prerelease specifier and only stable
/// releases are available.
///
/// requires-package-prerelease-specified-only-final-available-8c3e26d4
/// ├── environment
/// │   └── python3.7
/// ├── root
/// │   └── requires a>=0.1.0a1
/// │       ├── satisfied by a-0.1.0
/// │       ├── satisfied by a-0.2.0
/// │       └── satisfied by a-0.3.0
/// └── a
///     ├── a-0.1.0
///     ├── a-0.2.0
///     └── a-0.3.0
#[test]
fn requires_package_prerelease_specified_only_final_available() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((
        r"requires-package-prerelease-specified-only-final-available-8c3e26d4-",
        "",
    ));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-package-prerelease-specified-only-final-available-8c3e26d4-a>=0.1.0a1")
            .arg("--extra-index-url")
            .arg("https://test.pypi.org/simple")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .env("PUFFIN_NO_WRAP", "1")
            .current_dir(&temp_dir), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Resolved 1 package in [TIME]
        Downloaded 1 package in [TIME]
        Installed 1 package in [TIME]
         + a==0.3.0
        "###);
    });

    // The latest stable version should be selected.
    assert_installed(
        &venv,
        "requires_package_prerelease_specified_only_final_available_8c3e26d4_a",
        "0.3.0",
        &temp_dir,
    );

    Ok(())
}

/// requires-package-prerelease-specified-only-prerelease-available
///
/// The user requires a version of `a` with a prerelease specifier and only
/// prerelease releases are available.
///
/// requires-package-prerelease-specified-only-prerelease-available-b91b9892
/// ├── environment
/// │   └── python3.7
/// ├── root
/// │   └── requires a>=0.1.0a1
/// │       ├── satisfied by a-0.1.0a1
/// │       ├── satisfied by a-0.2.0a1
/// │       └── satisfied by a-0.3.0a1
/// └── a
///     ├── a-0.1.0a1
///     ├── a-0.2.0a1
///     └── a-0.3.0a1
#[test]
fn requires_package_prerelease_specified_only_prerelease_available() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((
        r"requires-package-prerelease-specified-only-prerelease-available-b91b9892-",
        "",
    ));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-package-prerelease-specified-only-prerelease-available-b91b9892-a>=0.1.0a1")
            .arg("--extra-index-url")
            .arg("https://test.pypi.org/simple")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .env("PUFFIN_NO_WRAP", "1")
            .current_dir(&temp_dir), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Resolved 1 package in [TIME]
        Downloaded 1 package in [TIME]
        Installed 1 package in [TIME]
         + a==0.3.0a1
        "###);
    });

    // The latest prerelease version should be selected.
    assert_installed(
        &venv,
        "requires_package_prerelease_specified_only_prerelease_available_b91b9892_a",
        "0.3.0a1",
        &temp_dir,
    );

    Ok(())
}

/// requires-package-prerelease-specified-mixed-available
///
/// The user requires a version of `a` with a prerelease specifier and both
/// prerelease and stable releases are available.
///
/// requires-package-prerelease-specified-mixed-available-48b383b8
/// ├── environment
/// │   └── python3.7
/// ├── root
/// │   └── requires a>=0.1.0a1
/// │       ├── satisfied by a-0.1.0
/// │       ├── satisfied by a-0.2.0a1
/// │       ├── satisfied by a-0.3.0
/// │       └── satisfied by a-1.0.0a1
/// └── a
///     ├── a-0.1.0
///     ├── a-0.2.0a1
///     ├── a-0.3.0
///     └── a-1.0.0a1
#[test]
fn requires_package_prerelease_specified_mixed_available() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((
        r"requires-package-prerelease-specified-mixed-available-48b383b8-",
        "",
    ));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-package-prerelease-specified-mixed-available-48b383b8-a>=0.1.0a1")
            .arg("--extra-index-url")
            .arg("https://test.pypi.org/simple")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .env("PUFFIN_NO_WRAP", "1")
            .current_dir(&temp_dir), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Resolved 1 package in [TIME]
        Downloaded 1 package in [TIME]
        Installed 1 package in [TIME]
         + a==1.0.0a1
        "###);
    });

    // Since the user provided a prerelease specifier, the latest prerelease version
    // should be selected.
    assert_installed(
        &venv,
        "requires_package_prerelease_specified_mixed_available_48b383b8_a",
        "1.0.0a1",
        &temp_dir,
    );

    Ok(())
}

/// requires-package-multiple-prereleases-kinds
///
/// The user requires `a` which has multiple prereleases available with different
/// labels.
///
/// requires-package-multiple-prereleases-kinds-91b38a0e
/// ├── environment
/// │   └── python3.7
/// ├── root
/// │   └── requires a>=1.0.0a1
/// │       ├── satisfied by a-1.0.0a1
/// │       ├── satisfied by a-1.0.0b1
/// │       └── satisfied by a-1.0.0rc1
/// └── a
///     ├── a-1.0.0a1
///     ├── a-1.0.0b1
///     └── a-1.0.0rc1
#[test]
fn requires_package_multiple_prereleases_kinds() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"requires-package-multiple-prereleases-kinds-91b38a0e-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-package-multiple-prereleases-kinds-91b38a0e-a>=1.0.0a1")
            .arg("--extra-index-url")
            .arg("https://test.pypi.org/simple")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .env("PUFFIN_NO_WRAP", "1")
            .current_dir(&temp_dir), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Resolved 1 package in [TIME]
        Downloaded 1 package in [TIME]
        Installed 1 package in [TIME]
         + a==1.0.0rc1
        "###);
    });

    // Release candidates should be the highest precedence prerelease kind.
    assert_installed(
        &venv,
        "requires_package_multiple_prereleases_kinds_91b38a0e_a",
        "1.0.0rc1",
        &temp_dir,
    );

    Ok(())
}

/// requires-package-multiple-prereleases-numbers
///
/// The user requires `a` which has multiple alphas available.
///
/// requires-package-multiple-prereleases-numbers-4cf7acef
/// ├── environment
/// │   └── python3.7
/// ├── root
/// │   └── requires a>=1.0.0a1
/// │       ├── satisfied by a-1.0.0a1
/// │       ├── satisfied by a-1.0.0a2
/// │       └── satisfied by a-1.0.0a3
/// └── a
///     ├── a-1.0.0a1
///     ├── a-1.0.0a2
///     └── a-1.0.0a3
#[test]
fn requires_package_multiple_prereleases_numbers() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((
        r"requires-package-multiple-prereleases-numbers-4cf7acef-",
        "",
    ));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-package-multiple-prereleases-numbers-4cf7acef-a>=1.0.0a1")
            .arg("--extra-index-url")
            .arg("https://test.pypi.org/simple")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .env("PUFFIN_NO_WRAP", "1")
            .current_dir(&temp_dir), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Resolved 1 package in [TIME]
        Downloaded 1 package in [TIME]
        Installed 1 package in [TIME]
         + a==1.0.0a3
        "###);
    });

    // The latest alpha version should be selected.
    assert_installed(
        &venv,
        "requires_package_multiple_prereleases_numbers_4cf7acef_a",
        "1.0.0a3",
        &temp_dir,
    );

    Ok(())
}

/// requires-transitive-package-only-prereleases
///
/// The user requires any version of package `a` which requires `b` which only has
/// prerelease versions available.
///
/// requires-transitive-package-only-prereleases-6e20b294
/// ├── environment
/// │   └── python3.7
/// ├── root
/// │   └── requires a
/// │       └── satisfied by a-0.1.0
/// ├── a
/// │   └── a-0.1.0
/// │       └── requires b
/// │           └── unsatisfied: no matching version
/// └── b
///     └── b-1.0.0a1
#[test]
fn requires_transitive_package_only_prereleases() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((
        r"requires-transitive-package-only-prereleases-6e20b294-",
        "",
    ));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-transitive-package-only-prereleases-6e20b294-a")
            .arg("--extra-index-url")
            .arg("https://test.pypi.org/simple")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .env("PUFFIN_NO_WRAP", "1")
            .current_dir(&temp_dir), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Resolved 2 packages in [TIME]
        Downloaded 2 packages in [TIME]
        Installed 2 packages in [TIME]
         + a==0.1.0
         + b==1.0.0a1
        "###);
    });

    // Since there are only prerelease versions of `b` available, it should be selected
    // even though the user did not opt-in to prereleases.
    assert_installed(
        &venv,
        "requires_transitive_package_only_prereleases_6e20b294_a",
        "0.1.0",
        &temp_dir,
    );
    assert_installed(
        &venv,
        "requires_transitive_package_only_prereleases_6e20b294_b",
        "1.0.0a1",
        &temp_dir,
    );

    Ok(())
}

/// requires-transitive-package-only-prereleases-in-range
///
/// The user requires package `a` which has a dependency on a package which only
/// matches prerelease versions but they did not include a prerelease specifier.
///
/// requires-transitive-package-only-prereleases-in-range-848f2c77
/// ├── environment
/// │   └── python3.7
/// ├── root
/// │   └── requires a
/// │       └── satisfied by a-0.1.0
/// ├── a
/// │   └── a-0.1.0
/// │       └── requires b>0.1
/// │           └── unsatisfied: no matching version
/// └── b
///     ├── b-0.1.0
///     └── b-1.0.0a1
#[test]
fn requires_transitive_package_only_prereleases_in_range() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((
        r"requires-transitive-package-only-prereleases-in-range-848f2c77-",
        "",
    ));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-transitive-package-only-prereleases-in-range-848f2c77-a")
            .arg("--extra-index-url")
            .arg("https://test.pypi.org/simple")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .env("PUFFIN_NO_WRAP", "1")
            .current_dir(&temp_dir), @r###"
        success: false
        exit_code: 1
        ----- stdout -----

        ----- stderr -----
          × No solution found when resolving dependencies:
          ╰─▶ Because there are no versions of b that satisfy b>0.1 and a==0.1.0 depends on b>0.1, we can conclude that a==0.1.0 is forbidden.
              And because there are no versions of a that satisfy any of:
                  a<0.1.0
                  a>0.1.0
              and root depends on a, we can conclude that the requirements are unsatisfiable.

              hint: Pre-releases are available for b in the requested range (e.g., 1.0.0a1), but pre-releases weren't enabled (try: `--prerelease=allow`)
        "###);
    });

    // Since there are stable versions of `b` available, the prerelease version should
    // not be selected without explicit opt-in. The available version is excluded by
    // the range requested by the user.
    assert_not_installed(
        &venv,
        "requires_transitive_package_only_prereleases_in_range_848f2c77_a",
        &temp_dir,
    );

    Ok(())
}

/// requires-transitive-package-only-prereleases-in-range-opt-in
///
/// The user requires package `a` which has a dependency on a package which only
/// matches prerelease versions; the user has opted into allowing prereleases in `b`
/// explicitly.
///
/// requires-transitive-package-only-prereleases-in-range-opt-in-1d2fc5a9
/// ├── environment
/// │   └── python3.7
/// ├── root
/// │   ├── requires a
/// │   │   └── satisfied by a-0.1.0
/// │   └── requires b>0.0.0a1
/// │       └── satisfied by b-0.1.0
/// ├── a
/// │   └── a-0.1.0
/// │       └── requires b>0.1
/// │           └── unsatisfied: no matching version
/// └── b
///     ├── b-0.1.0
///     └── b-1.0.0a1
#[test]
fn requires_transitive_package_only_prereleases_in_range_opt_in() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((
        r"requires-transitive-package-only-prereleases-in-range-opt-in-1d2fc5a9-",
        "",
    ));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-transitive-package-only-prereleases-in-range-opt-in-1d2fc5a9-a")
            .arg("requires-transitive-package-only-prereleases-in-range-opt-in-1d2fc5a9-b>0.0.0a1")
            .arg("--extra-index-url")
            .arg("https://test.pypi.org/simple")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .env("PUFFIN_NO_WRAP", "1")
            .current_dir(&temp_dir), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Resolved 2 packages in [TIME]
        Downloaded 2 packages in [TIME]
        Installed 2 packages in [TIME]
         + a==0.1.0
         + b==1.0.0a1
        "###);
    });

    // Since the user included a dependency on `b` with a prerelease specifier, a
    // prerelease version can be selected.
    assert_installed(
        &venv,
        "requires_transitive_package_only_prereleases_in_range_opt_in_1d2fc5a9_a",
        "0.1.0",
        &temp_dir,
    );
    assert_installed(
        &venv,
        "requires_transitive_package_only_prereleases_in_range_opt_in_1d2fc5a9_b",
        "1.0.0a1",
        &temp_dir,
    );

    Ok(())
}

/// requires-transitive-prerelease-and-stable-dependency
///
/// A transitive dependency has both a prerelease and a stable selector, but can
/// only be satisfied by a prerelease
///
/// requires-transitive-prerelease-and-stable-dependency-31b546ef
/// ├── environment
/// │   └── python3.7
/// ├── root
/// │   ├── requires a
/// │   │   └── satisfied by a-1.0.0
/// │   └── requires b
/// │       └── satisfied by b-1.0.0
/// ├── a
/// │   └── a-1.0.0
/// │       └── requires c==2.0.0b1
/// │           └── satisfied by c-2.0.0b1
/// ├── b
/// │   └── b-1.0.0
/// │       └── requires c>=1.0.0,<=3.0.0
/// │           └── satisfied by c-1.0.0
/// └── c
///     ├── c-1.0.0
///     └── c-2.0.0b1
#[test]
fn requires_transitive_prerelease_and_stable_dependency() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((
        r"requires-transitive-prerelease-and-stable-dependency-31b546ef-",
        "",
    ));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-transitive-prerelease-and-stable-dependency-31b546ef-a")
            .arg("requires-transitive-prerelease-and-stable-dependency-31b546ef-b")
            .arg("--extra-index-url")
            .arg("https://test.pypi.org/simple")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .env("PUFFIN_NO_WRAP", "1")
            .current_dir(&temp_dir), @r###"
        success: false
        exit_code: 1
        ----- stdout -----

        ----- stderr -----
          × No solution found when resolving dependencies:
          ╰─▶ Because there is no version of c==2.0.0b1 and a==1.0.0 depends on c==2.0.0b1, we can conclude that a==1.0.0 is forbidden.
              And because there are no versions of a that satisfy any of:
                  a<1.0.0
                  a>1.0.0
              and root depends on a, we can conclude that the requirements are unsatisfiable.

              hint: c was requested with a pre-release marker (e.g., c==2.0.0b1), but pre-releases weren't enabled (try: `--prerelease=allow`)
        "###);
    });

    // Since the user did not explicitly opt-in to a prerelease, it cannot be selected.
    assert_not_installed(
        &venv,
        "requires_transitive_prerelease_and_stable_dependency_31b546ef_a",
        &temp_dir,
    );
    assert_not_installed(
        &venv,
        "requires_transitive_prerelease_and_stable_dependency_31b546ef_b",
        &temp_dir,
    );

    Ok(())
}

/// requires-transitive-prerelease-and-stable-dependency-opt-in
///
/// A transitive dependency has both a prerelease and a stable selector, but can
/// only be satisfied by a prerelease. The user includes an opt-in to prereleases of
/// the transitive dependency.
///
/// requires-transitive-prerelease-and-stable-dependency-opt-in-dd00a87f
/// ├── environment
/// │   └── python3.7
/// ├── root
/// │   ├── requires a
/// │   │   └── satisfied by a-1.0.0
/// │   ├── requires b
/// │   │   └── satisfied by b-1.0.0
/// │   └── requires c>=0.0.0a1
/// │       ├── satisfied by c-1.0.0
/// │       └── satisfied by c-2.0.0b1
/// ├── a
/// │   └── a-1.0.0
/// │       └── requires c==2.0.0b1
/// │           └── satisfied by c-2.0.0b1
/// ├── b
/// │   └── b-1.0.0
/// │       └── requires c>=1.0.0,<=3.0.0
/// │           └── satisfied by c-1.0.0
/// └── c
///     ├── c-1.0.0
///     └── c-2.0.0b1
#[test]
fn requires_transitive_prerelease_and_stable_dependency_opt_in() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((
        r"requires-transitive-prerelease-and-stable-dependency-opt-in-dd00a87f-",
        "",
    ));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-transitive-prerelease-and-stable-dependency-opt-in-dd00a87f-a")
            .arg("requires-transitive-prerelease-and-stable-dependency-opt-in-dd00a87f-b")
            .arg("requires-transitive-prerelease-and-stable-dependency-opt-in-dd00a87f-c>=0.0.0a1")
            .arg("--extra-index-url")
            .arg("https://test.pypi.org/simple")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .env("PUFFIN_NO_WRAP", "1")
            .current_dir(&temp_dir), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Resolved 3 packages in [TIME]
        Downloaded 3 packages in [TIME]
        Installed 3 packages in [TIME]
         + a==1.0.0
         + b==1.0.0
         + c==2.0.0b1
        "###);
    });

    // Since the user explicitly opted-in to a prerelease for `c`, it can be installed.
    assert_installed(
        &venv,
        "requires_transitive_prerelease_and_stable_dependency_opt_in_dd00a87f_a",
        "1.0.0",
        &temp_dir,
    );
    assert_installed(
        &venv,
        "requires_transitive_prerelease_and_stable_dependency_opt_in_dd00a87f_b",
        "1.0.0",
        &temp_dir,
    );
    assert_installed(
        &venv,
        "requires_transitive_prerelease_and_stable_dependency_opt_in_dd00a87f_c",
        "2.0.0b1",
        &temp_dir,
    );

    Ok(())
}

/// requires-transitive-prerelease-and-stable-dependency-many-versions
///
/// A transitive dependency has both a prerelease and a stable selector, but can
/// only be satisfied by a prerelease. There are many prerelease versions.
///
/// requires-transitive-prerelease-and-stable-dependency-many-versions-3258056f
/// ├── environment
/// │   └── python3.7
/// ├── root
/// │   ├── requires a
/// │   │   └── satisfied by a-1.0.0
/// │   └── requires b
/// │       └── satisfied by b-1.0.0
/// ├── a
/// │   └── a-1.0.0
/// │       └── requires c>=2.0.0b1
/// │           ├── satisfied by c-2.0.0b1
/// │           ├── satisfied by c-2.0.0b2
/// │           ├── satisfied by c-2.0.0b3
/// │           ├── satisfied by c-2.0.0b4
/// │           ├── satisfied by c-2.0.0b5
/// │           ├── satisfied by c-2.0.0b6
/// │           ├── satisfied by c-2.0.0b7
/// │           ├── satisfied by c-2.0.0b8
/// │           └── satisfied by c-2.0.0b9
/// ├── b
/// │   └── b-1.0.0
/// │       └── requires c>=1.0.0,<=3.0.0
/// │           └── satisfied by c-1.0.0
/// └── c
///     ├── c-1.0.0
///     ├── c-2.0.0a1
///     ├── c-2.0.0a2
///     ├── c-2.0.0a3
///     ├── c-2.0.0a4
///     ├── c-2.0.0a5
///     ├── c-2.0.0a6
///     ├── c-2.0.0a7
///     ├── c-2.0.0a8
///     ├── c-2.0.0a9
///     ├── c-2.0.0b1
///     ├── c-2.0.0b2
///     ├── c-2.0.0b3
///     ├── c-2.0.0b4
///     ├── c-2.0.0b5
///     ├── c-2.0.0b6
///     ├── c-2.0.0b7
///     ├── c-2.0.0b8
///     └── c-2.0.0b9
#[test]
fn requires_transitive_prerelease_and_stable_dependency_many_versions() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((
        r"requires-transitive-prerelease-and-stable-dependency-many-versions-3258056f-",
        "",
    ));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-transitive-prerelease-and-stable-dependency-many-versions-3258056f-a")
            .arg("requires-transitive-prerelease-and-stable-dependency-many-versions-3258056f-b")
            .arg("--extra-index-url")
            .arg("https://test.pypi.org/simple")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .env("PUFFIN_NO_WRAP", "1")
            .current_dir(&temp_dir), @r###"
        success: false
        exit_code: 1
        ----- stdout -----

        ----- stderr -----
          × No solution found when resolving dependencies:
          ╰─▶ Because there are no versions of b that satisfy any of:
                  b<1.0.0
                  b>1.0.0
              and b==1.0.0 depends on c, we can conclude that b depends on c.
              And because there are no versions of c that satisfy c>=2.0.0b1 we can conclude that b depends on c<2.0.0b1.
              And because a==1.0.0 depends on c>=2.0.0b1 and there are no versions of a that satisfy any of:
                  a<1.0.0
                  a>1.0.0
              we can conclude that b*, a* are incompatible.
              And because root depends on b and root depends on a, we can conclude that the requirements are unsatisfiable.

              hint: c was requested with a pre-release marker (e.g., c>=2.0.0b1), but pre-releases weren't enabled (try: `--prerelease=allow`)
        "###);
    });

    // Since the user did not explicitly opt-in to a prerelease, it cannot be selected.
    assert_not_installed(
        &venv,
        "requires_transitive_prerelease_and_stable_dependency_many_versions_3258056f_a",
        &temp_dir,
    );
    assert_not_installed(
        &venv,
        "requires_transitive_prerelease_and_stable_dependency_many_versions_3258056f_b",
        &temp_dir,
    );

    Ok(())
}

/// requires-transitive-prerelease-and-stable-dependency-many-versions-holes
///
/// A transitive dependency has both a prerelease and a stable selector, but can
/// only be satisfied by a prerelease. There are many prerelease versions and some
/// are excluded.
///
/// requires-transitive-prerelease-and-stable-dependency-many-versions-holes-293fcbc7
/// ├── environment
/// │   └── python3.7
/// ├── root
/// │   ├── requires a
/// │   │   └── satisfied by a-1.0.0
/// │   └── requires b
/// │       └── satisfied by b-1.0.0
/// ├── a
/// │   └── a-1.0.0
/// │       └── requires c>1.0.0,!=2.0.0a5,!=2.0.0a6,!=2.0.0a7,!=2.0.0b1,<2.0.0b5
/// │           └── unsatisfied: no matching version
/// ├── b
/// │   └── b-1.0.0
/// │       └── requires c>=1.0.0,<=3.0.0
/// │           └── satisfied by c-1.0.0
/// └── c
///     ├── c-1.0.0
///     ├── c-2.0.0a1
///     ├── c-2.0.0a2
///     ├── c-2.0.0a3
///     ├── c-2.0.0a4
///     ├── c-2.0.0a5
///     ├── c-2.0.0a6
///     ├── c-2.0.0a7
///     ├── c-2.0.0a8
///     ├── c-2.0.0a9
///     ├── c-2.0.0b1
///     ├── c-2.0.0b2
///     ├── c-2.0.0b3
///     ├── c-2.0.0b4
///     ├── c-2.0.0b5
///     ├── c-2.0.0b6
///     ├── c-2.0.0b7
///     ├── c-2.0.0b8
///     └── c-2.0.0b9
#[test]
fn requires_transitive_prerelease_and_stable_dependency_many_versions_holes() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((
        r"requires-transitive-prerelease-and-stable-dependency-many-versions-holes-293fcbc7-",
        "",
    ));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-transitive-prerelease-and-stable-dependency-many-versions-holes-293fcbc7-a")
            .arg("requires-transitive-prerelease-and-stable-dependency-many-versions-holes-293fcbc7-b")
            .arg("--extra-index-url")
            .arg("https://test.pypi.org/simple")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .env("PUFFIN_NO_WRAP", "1")
            .current_dir(&temp_dir), @r###"
        success: false
        exit_code: 1
        ----- stdout -----

        ----- stderr -----
          × No solution found when resolving dependencies:
          ╰─▶ Because there are no versions of c that satisfy any of:
                  c>1.0.0,<2.0.0a5
                  c>2.0.0a7,<2.0.0b1
                  c>2.0.0b1,<2.0.0b5
              and a==1.0.0 depends on one of:
                  c>1.0.0,<2.0.0a5
                  c>2.0.0a7,<2.0.0b1
                  c>2.0.0b1,<2.0.0b5
              we can conclude that a==1.0.0 is forbidden.
              And because there are no versions of a that satisfy any of:
                  a<1.0.0
                  a>1.0.0
              and root depends on a, we can conclude that the requirements are unsatisfiable.

              hint: c was requested with a pre-release marker (e.g., any of:
                  c>1.0.0,<2.0.0a5
                  c>2.0.0a7,<2.0.0b1
                  c>2.0.0b1,<2.0.0b5
              ), but pre-releases weren't enabled (try: `--prerelease=allow`)
        "###);
    });

    // Since the user did not explicitly opt-in to a prerelease, it cannot be selected.
    assert_not_installed(
        &venv,
        "requires_transitive_prerelease_and_stable_dependency_many_versions_holes_293fcbc7_a",
        &temp_dir,
    );
    assert_not_installed(
        &venv,
        "requires_transitive_prerelease_and_stable_dependency_many_versions_holes_293fcbc7_b",
        &temp_dir,
    );

    Ok(())
}

/// requires-package-does-not-exist
///
/// The user requires any version of package `a` which does not exist.
///
/// requires-package-does-not-exist-57cd4136
/// ├── environment
/// │   └── python3.7
/// └── root
///     └── requires a
///         └── unsatisfied: no versions for package
#[test]
fn requires_package_does_not_exist() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"requires-package-does-not-exist-57cd4136-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-package-does-not-exist-57cd4136-a")
            .arg("--extra-index-url")
            .arg("https://test.pypi.org/simple")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .env("PUFFIN_NO_WRAP", "1")
            .current_dir(&temp_dir), @r###"
        success: false
        exit_code: 2
        ----- stdout -----

        ----- stderr -----
        error: Package `a` was not found in the registry.
        "###);
    });

    assert_not_installed(
        &venv,
        "requires_package_does_not_exist_57cd4136_a",
        &temp_dir,
    );

    Ok(())
}

/// requires-exact-version-does-not-exist
///
/// The user requires an exact version of package `a` but only other versions exist
///
/// requires-exact-version-does-not-exist-eaa03067
/// ├── environment
/// │   └── python3.7
/// ├── root
/// │   └── requires a==2.0.0
/// │       └── unsatisfied: no matching version
/// └── a
///     └── a-1.0.0
#[test]
fn requires_exact_version_does_not_exist() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"requires-exact-version-does-not-exist-eaa03067-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-exact-version-does-not-exist-eaa03067-a==2.0.0")
            .arg("--extra-index-url")
            .arg("https://test.pypi.org/simple")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .env("PUFFIN_NO_WRAP", "1")
            .current_dir(&temp_dir), @r###"
        success: false
        exit_code: 1
        ----- stdout -----

        ----- stderr -----
          × No solution found when resolving dependencies:
          ╰─▶ Because there is no version of a==2.0.0 and root depends on a==2.0.0, we can conclude that the requirements are unsatisfiable.
        "###);
    });

    assert_not_installed(
        &venv,
        "requires_exact_version_does_not_exist_eaa03067_a",
        &temp_dir,
    );

    Ok(())
}

/// requires-greater-version-does-not-exist
///
/// The user requires a version of `a` greater than `1.0.0` but only smaller or
/// equal versions exist
///
/// requires-greater-version-does-not-exist-6e8e01df
/// ├── environment
/// │   └── python3.7
/// ├── root
/// │   └── requires a>1.0.0
/// │       └── unsatisfied: no matching version
/// └── a
///     ├── a-0.1.0
///     └── a-1.0.0
#[test]
fn requires_greater_version_does_not_exist() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"requires-greater-version-does-not-exist-6e8e01df-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-greater-version-does-not-exist-6e8e01df-a>1.0.0")
            .arg("--extra-index-url")
            .arg("https://test.pypi.org/simple")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .env("PUFFIN_NO_WRAP", "1")
            .current_dir(&temp_dir), @r###"
        success: false
        exit_code: 1
        ----- stdout -----

        ----- stderr -----
          × No solution found when resolving dependencies:
          ╰─▶ Because there are no versions of a that satisfy a>1.0.0 and root depends on a>1.0.0, we can conclude that the requirements are unsatisfiable.
        "###);
    });

    assert_not_installed(
        &venv,
        "requires_greater_version_does_not_exist_6e8e01df_a",
        &temp_dir,
    );

    Ok(())
}

/// requires-less-version-does-not-exist
///
/// The user requires a version of `a` less than `1.0.0` but only larger versions
/// exist
///
/// requires-less-version-does-not-exist-e45cec3c
/// ├── environment
/// │   └── python3.7
/// ├── root
/// │   └── requires a<2.0.0
/// │       └── unsatisfied: no matching version
/// └── a
///     ├── a-2.0.0
///     ├── a-3.0.0
///     └── a-4.0.0
#[test]
fn requires_less_version_does_not_exist() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"requires-less-version-does-not-exist-e45cec3c-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-less-version-does-not-exist-e45cec3c-a<2.0.0")
            .arg("--extra-index-url")
            .arg("https://test.pypi.org/simple")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .env("PUFFIN_NO_WRAP", "1")
            .current_dir(&temp_dir), @r###"
        success: false
        exit_code: 1
        ----- stdout -----

        ----- stderr -----
          × No solution found when resolving dependencies:
          ╰─▶ Because there are no versions of a that satisfy a<2.0.0 and root depends on a<2.0.0, we can conclude that the requirements are unsatisfiable.
        "###);
    });

    assert_not_installed(
        &venv,
        "requires_less_version_does_not_exist_e45cec3c_a",
        &temp_dir,
    );

    Ok(())
}

/// transitive-requires-package-does-not-exist
///
/// The user requires package `a` but `a` requires package `b` which does not exist
///
/// transitive-requires-package-does-not-exist-aca2796a
/// ├── environment
/// │   └── python3.7
/// ├── root
/// │   └── requires a
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
///         └── requires b
///             └── unsatisfied: no versions for package
#[test]
fn transitive_requires_package_does_not_exist() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"transitive-requires-package-does-not-exist-aca2796a-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("transitive-requires-package-does-not-exist-aca2796a-a")
            .arg("--extra-index-url")
            .arg("https://test.pypi.org/simple")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .env("PUFFIN_NO_WRAP", "1")
            .current_dir(&temp_dir), @r###"
        success: false
        exit_code: 2
        ----- stdout -----

        ----- stderr -----
        error: Package `b` was not found in the registry.
        "###);
    });

    assert_not_installed(
        &venv,
        "transitive_requires_package_does_not_exist_aca2796a_a",
        &temp_dir,
    );

    Ok(())
}

/// requires-direct-incompatible-versions
///
/// The user requires two incompatible, existing versions of package `a`
///
/// requires-direct-incompatible-versions-063ec9d3
/// ├── environment
/// │   └── python3.7
/// ├── root
/// │   ├── requires a==1.0.0
/// │   │   └── satisfied by a-1.0.0
/// │   └── requires a==2.0.0
/// │       └── satisfied by a-2.0.0
/// └── a
///     ├── a-1.0.0
///     └── a-2.0.0
#[test]
fn requires_direct_incompatible_versions() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"requires-direct-incompatible-versions-063ec9d3-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-direct-incompatible-versions-063ec9d3-a==1.0.0")
            .arg("requires-direct-incompatible-versions-063ec9d3-a==2.0.0")
            .arg("--extra-index-url")
            .arg("https://test.pypi.org/simple")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .env("PUFFIN_NO_WRAP", "1")
            .current_dir(&temp_dir), @r###"
        success: false
        exit_code: 1
        ----- stdout -----

        ----- stderr -----
          × No solution found when resolving dependencies:
          ╰─▶ root dependencies are unusable: Conflicting versions for `a`: `a==1.0.0` does not intersect with `a==2.0.0`
        "###);
    });

    assert_not_installed(
        &venv,
        "requires_direct_incompatible_versions_063ec9d3_a",
        &temp_dir,
    );
    assert_not_installed(
        &venv,
        "requires_direct_incompatible_versions_063ec9d3_a",
        &temp_dir,
    );

    Ok(())
}

/// requires-transitive-incompatible-with-root-version
///
/// The user requires packages `a` and `b` but `a` requires a different version of
/// `b`
///
/// requires-transitive-incompatible-with-root-version-638350f3
/// ├── environment
/// │   └── python3.7
/// ├── root
/// │   ├── requires a
/// │   │   └── satisfied by a-1.0.0
/// │   └── requires b==1.0.0
/// │       └── satisfied by b-1.0.0
/// ├── a
/// │   └── a-1.0.0
/// │       └── requires b==2.0.0
/// │           └── satisfied by b-2.0.0
/// └── b
///     ├── b-1.0.0
///     └── b-2.0.0
#[test]
fn requires_transitive_incompatible_with_root_version() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((
        r"requires-transitive-incompatible-with-root-version-638350f3-",
        "",
    ));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-transitive-incompatible-with-root-version-638350f3-a")
            .arg("requires-transitive-incompatible-with-root-version-638350f3-b==1.0.0")
            .arg("--extra-index-url")
            .arg("https://test.pypi.org/simple")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .env("PUFFIN_NO_WRAP", "1")
            .current_dir(&temp_dir), @r###"
        success: false
        exit_code: 1
        ----- stdout -----

        ----- stderr -----
          × No solution found when resolving dependencies:
          ╰─▶ Because there are no versions of a that satisfy any of:
                  a<1.0.0
                  a>1.0.0
              and a==1.0.0 depends on b==2.0.0, we can conclude that a depends on b==2.0.0.
              And because root depends on a and root depends on b==1.0.0, we can conclude that the requirements are unsatisfiable.
        "###);
    });

    assert_not_installed(
        &venv,
        "requires_transitive_incompatible_with_root_version_638350f3_a",
        &temp_dir,
    );
    assert_not_installed(
        &venv,
        "requires_transitive_incompatible_with_root_version_638350f3_b",
        &temp_dir,
    );

    Ok(())
}

/// requires-transitive-incompatible-with-transitive
///
/// The user requires package `a` and `b`; `a` and `b` require different versions of
/// `c`
///
/// requires-transitive-incompatible-with-transitive-9b595175
/// ├── environment
/// │   └── python3.7
/// ├── root
/// │   ├── requires a
/// │   │   └── satisfied by a-1.0.0
/// │   └── requires b
/// │       └── satisfied by b-1.0.0
/// ├── a
/// │   └── a-1.0.0
/// │       └── requires c==1.0.0
/// │           └── satisfied by c-1.0.0
/// ├── b
/// │   └── b-1.0.0
/// │       └── requires c==2.0.0
/// │           └── satisfied by c-2.0.0
/// └── c
///     ├── c-1.0.0
///     └── c-2.0.0
#[test]
fn requires_transitive_incompatible_with_transitive() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((
        r"requires-transitive-incompatible-with-transitive-9b595175-",
        "",
    ));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-transitive-incompatible-with-transitive-9b595175-a")
            .arg("requires-transitive-incompatible-with-transitive-9b595175-b")
            .arg("--extra-index-url")
            .arg("https://test.pypi.org/simple")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .env("PUFFIN_NO_WRAP", "1")
            .current_dir(&temp_dir), @r###"
        success: false
        exit_code: 1
        ----- stdout -----

        ----- stderr -----
          × No solution found when resolving dependencies:
          ╰─▶ Because there are no versions of b that satisfy any of:
                  b<1.0.0
                  b>1.0.0
              and b==1.0.0 depends on c==2.0.0, we can conclude that b depends on c==2.0.0.
              And because a==1.0.0 depends on c==1.0.0 and there are no versions of a that satisfy any of:
                  a<1.0.0
                  a>1.0.0
              we can conclude that a*, b* are incompatible.
              And because root depends on b and root depends on a, we can conclude that the requirements are unsatisfiable.
        "###);
    });

    assert_not_installed(
        &venv,
        "requires_transitive_incompatible_with_transitive_9b595175_a",
        &temp_dir,
    );
    assert_not_installed(
        &venv,
        "requires_transitive_incompatible_with_transitive_9b595175_b",
        &temp_dir,
    );

    Ok(())
}

/// requires-python-version-does-not-exist
///
/// The user requires a package which requires a Python version that does not exist
///
/// requires-python-version-does-not-exist-0825b69c
/// ├── environment
/// │   └── python3.7
/// ├── root
/// │   └── requires a==1.0.0
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
///         └── requires python>=4.0 (incompatible with environment)
#[test]
fn requires_python_version_does_not_exist() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"requires-python-version-does-not-exist-0825b69c-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-python-version-does-not-exist-0825b69c-a==1.0.0")
            .arg("--extra-index-url")
            .arg("https://test.pypi.org/simple")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .env("PUFFIN_NO_WRAP", "1")
            .current_dir(&temp_dir), @r###"
        success: false
        exit_code: 1
        ----- stdout -----

        ----- stderr -----
          × No solution found when resolving dependencies:
          ╰─▶ Because there are no versions of Python that satisfy Python>=4.0 and a==1.0.0 depends on Python>=4.0, we can conclude that a==1.0.0 is forbidden.
              And because root depends on a==1.0.0 we can conclude that the requirements are unsatisfiable.
        "###);
    });

    assert_not_installed(
        &venv,
        "requires_python_version_does_not_exist_0825b69c_a",
        &temp_dir,
    );

    Ok(())
}

/// requires-python-version-less-than-current
///
/// The user requires a package which requires a Python version less than the
/// current version
///
/// requires-python-version-less-than-current-f9296b84
/// ├── environment
/// │   └── python3.9
/// ├── root
/// │   └── requires a==1.0.0
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
///         └── requires python<=3.8 (incompatible with environment)
#[test]
fn requires_python_version_less_than_current() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.9");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"requires-python-version-less-than-current-f9296b84-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-python-version-less-than-current-f9296b84-a==1.0.0")
            .arg("--extra-index-url")
            .arg("https://test.pypi.org/simple")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .env("PUFFIN_NO_WRAP", "1")
            .current_dir(&temp_dir), @r###"
        success: false
        exit_code: 1
        ----- stdout -----

        ----- stderr -----
          × No solution found when resolving dependencies:
          ╰─▶ Because there are no versions of Python that satisfy Python<=3.8 and a==1.0.0 depends on Python<=3.8, we can conclude that a==1.0.0 is forbidden.
              And because root depends on a==1.0.0 we can conclude that the requirements are unsatisfiable.
        "###);
    });

    assert_not_installed(
        &venv,
        "requires_python_version_less_than_current_f9296b84_a",
        &temp_dir,
    );

    Ok(())
}

/// requires-python-version-greater-than-current
///
/// The user requires a package which requires a Python version greater than the
/// current version
///
/// requires-python-version-greater-than-current-a11d5394
/// ├── environment
/// │   └── python3.9
/// ├── root
/// │   └── requires a==1.0.0
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
///         └── requires python>=3.10 (incompatible with environment)
#[test]
fn requires_python_version_greater_than_current() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.9");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((
        r"requires-python-version-greater-than-current-a11d5394-",
        "",
    ));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-python-version-greater-than-current-a11d5394-a==1.0.0")
            .arg("--extra-index-url")
            .arg("https://test.pypi.org/simple")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .env("PUFFIN_NO_WRAP", "1")
            .current_dir(&temp_dir), @r###"
        success: false
        exit_code: 1
        ----- stdout -----

        ----- stderr -----
          × No solution found when resolving dependencies:
          ╰─▶ Because there are no versions of Python that satisfy Python>=3.10 and a==1.0.0 depends on Python>=3.10, we can conclude that a==1.0.0 is forbidden.
              And because root depends on a==1.0.0 we can conclude that the requirements are unsatisfiable.
        "###);
    });

    assert_not_installed(
        &venv,
        "requires_python_version_greater_than_current_a11d5394_a",
        &temp_dir,
    );

    Ok(())
}

/// requires-python-version-greater-than-current-many
///
/// The user requires a package which has many versions which all require a Python
/// version greater than the current version
///
/// requires-python-version-greater-than-current-many-02dc550c
/// ├── environment
/// │   └── python3.9
/// ├── root
/// │   └── requires a==1.0.0
/// │       └── unsatisfied: no matching version
/// └── a
///     ├── a-2.0.0
///     │   └── requires python>=3.10 (incompatible with environment)
///     ├── a-2.1.0
///     │   └── requires python>=3.10 (incompatible with environment)
///     ├── a-2.2.0
///     │   └── requires python>=3.10 (incompatible with environment)
///     ├── a-2.3.0
///     │   └── requires python>=3.10 (incompatible with environment)
///     ├── a-2.4.0
///     │   └── requires python>=3.10 (incompatible with environment)
///     ├── a-2.5.0
///     │   └── requires python>=3.10 (incompatible with environment)
///     ├── a-3.0.0
///     │   └── requires python>=3.11 (incompatible with environment)
///     ├── a-3.1.0
///     │   └── requires python>=3.11 (incompatible with environment)
///     ├── a-3.2.0
///     │   └── requires python>=3.11 (incompatible with environment)
///     ├── a-3.3.0
///     │   └── requires python>=3.11 (incompatible with environment)
///     ├── a-3.4.0
///     │   └── requires python>=3.11 (incompatible with environment)
///     └── a-3.5.0
///         └── requires python>=3.11 (incompatible with environment)
#[test]
fn requires_python_version_greater_than_current_many() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.9");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((
        r"requires-python-version-greater-than-current-many-02dc550c-",
        "",
    ));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-python-version-greater-than-current-many-02dc550c-a==1.0.0")
            .arg("--extra-index-url")
            .arg("https://test.pypi.org/simple")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .env("PUFFIN_NO_WRAP", "1")
            .current_dir(&temp_dir), @r###"
        success: false
        exit_code: 1
        ----- stdout -----

        ----- stderr -----
          × No solution found when resolving dependencies:
          ╰─▶ Because there is no version of a==1.0.0 and root depends on a==1.0.0, we can conclude that the requirements are unsatisfiable.
        "###);
    });

    assert_not_installed(
        &venv,
        "requires_python_version_greater_than_current_many_02dc550c_a",
        &temp_dir,
    );

    Ok(())
}

/// requires-python-version-greater-than-current-backtrack
///
/// The user requires a package where recent versions require a Python version
/// greater than the current version, but an older version is compatible.
///
/// requires-python-version-greater-than-current-backtrack-ef060cef
/// ├── environment
/// │   └── python3.9
/// ├── root
/// │   └── requires a
/// │       ├── satisfied by a-1.0.0
/// │       ├── satisfied by a-2.0.0
/// │       ├── satisfied by a-3.0.0
/// │       └── satisfied by a-4.0.0
/// └── a
///     ├── a-1.0.0
///     ├── a-2.0.0
///     │   └── requires python>=3.10 (incompatible with environment)
///     ├── a-3.0.0
///     │   └── requires python>=3.11 (incompatible with environment)
///     └── a-4.0.0
///         └── requires python>=3.12 (incompatible with environment)
#[test]
fn requires_python_version_greater_than_current_backtrack() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.9");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((
        r"requires-python-version-greater-than-current-backtrack-ef060cef-",
        "",
    ));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-python-version-greater-than-current-backtrack-ef060cef-a")
            .arg("--extra-index-url")
            .arg("https://test.pypi.org/simple")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .env("PUFFIN_NO_WRAP", "1")
            .current_dir(&temp_dir), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Resolved 1 package in [TIME]
        Downloaded 1 package in [TIME]
        Installed 1 package in [TIME]
         + a==1.0.0
        "###);
    });

    assert_installed(
        &venv,
        "requires_python_version_greater_than_current_backtrack_ef060cef_a",
        "1.0.0",
        &temp_dir,
    );

    Ok(())
}

/// requires-python-version-greater-than-current-excluded
///
/// The user requires a package where recent versions require a Python version
/// greater than the current version, but an excluded older version is compatible.
///
/// requires-python-version-greater-than-current-excluded-1bde0c18
/// ├── environment
/// │   └── python3.9
/// ├── root
/// │   └── requires a>=2.0.0
/// │       ├── satisfied by a-2.0.0
/// │       ├── satisfied by a-3.0.0
/// │       └── satisfied by a-4.0.0
/// └── a
///     ├── a-1.0.0
///     ├── a-2.0.0
///     │   └── requires python>=3.10 (incompatible with environment)
///     ├── a-3.0.0
///     │   └── requires python>=3.11 (incompatible with environment)
///     └── a-4.0.0
///         └── requires python>=3.12 (incompatible with environment)
#[test]
fn requires_python_version_greater_than_current_excluded() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.9");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((
        r"requires-python-version-greater-than-current-excluded-1bde0c18-",
        "",
    ));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-python-version-greater-than-current-excluded-1bde0c18-a>=2.0.0")
            .arg("--extra-index-url")
            .arg("https://test.pypi.org/simple")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .env("PUFFIN_NO_WRAP", "1")
            .current_dir(&temp_dir), @r###"
        success: false
        exit_code: 1
        ----- stdout -----

        ----- stderr -----
          × No solution found when resolving dependencies:
          ╰─▶ Because there are no versions of Python that satisfy Python>=3.10,<3.11 and there are no versions of Python that satisfy Python>=3.12, we can conclude that Python>=3.10, <3.11 | >=3.12 are incompatible.
              And because there are no versions of Python that satisfy Python>=3.11,<3.12 we can conclude that Python>=3.10 are incompatible.
              And because a==2.0.0 depends on Python>=3.10 and there are no versions of a that satisfy any of:
                  a>2.0.0,<3.0.0
                  a>3.0.0,<4.0.0
                  a>4.0.0
              we can conclude that a>=2.0.0,<3.0.0 is forbidden. (1)

              Because there are no versions of Python that satisfy Python>=3.11,<3.12 and there are no versions of Python that satisfy Python>=3.12, we can conclude that Python>=3.11 are incompatible.
              And because a==3.0.0 depends on Python>=3.11 we can conclude that a==3.0.0 is forbidden.
              And because we know from (1) that a>=2.0.0,<3.0.0 is forbidden, we can conclude that a>=2.0.0,<4.0.0 is forbidden. (2)

              Because there are no versions of Python that satisfy Python>=3.12 and a==4.0.0 depends on Python>=3.12, we can conclude that a==4.0.0 is forbidden.
              And because we know from (2) that a>=2.0.0,<4.0.0 is forbidden, we can conclude that a>=2.0.0 is forbidden.
              And because root depends on a>=2.0.0 we can conclude that the requirements are unsatisfiable.
        "###);
    });

    assert_not_installed(
        &venv,
        "requires_python_version_greater_than_current_excluded_1bde0c18_a",
        &temp_dir,
    );

    Ok(())
}
