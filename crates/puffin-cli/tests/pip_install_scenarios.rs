//! DO NOT EDIT
//!
//! Generated with ./scripts/scenarios/update.py
//! Scenarios from <https://github.com/zanieb/packse/tree/a9d2f659117693b89cba8a487200fd01444468af/scenarios>
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

/// requires-package-does-not-exist
///
/// The user requires any version of package `a` which does not exist.
///
/// s57cd4136
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
    filters.push((r"s57cd4136-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("s57cd4136-a")
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

    assert_not_installed(&venv, "s57cd4136_a", &temp_dir);

    Ok(())
}

/// requires-exact-version-does-not-exist
///
/// The user requires an exact version of package `a` but only other versions exist
///
/// seaa03067
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
    filters.push((r"seaa03067-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("seaa03067-a==2.0.0")
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

    assert_not_installed(&venv, "seaa03067_a", &temp_dir);

    Ok(())
}

/// requires-greater-version-does-not-exist
///
/// The user requires a version of `a` greater than `1.0.0` but only smaller or
/// equal versions exist
///
/// s6e8e01df
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
    filters.push((r"s6e8e01df-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("s6e8e01df-a>1.0.0")
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

    assert_not_installed(&venv, "s6e8e01df_a", &temp_dir);

    Ok(())
}

/// requires-less-version-does-not-exist
///
/// The user requires a version of `a` less than `1.0.0` but only larger versions
/// exist
///
/// se45cec3c
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
    filters.push((r"se45cec3c-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("se45cec3c-a<2.0.0")
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

    assert_not_installed(&venv, "se45cec3c_a", &temp_dir);

    Ok(())
}

/// transitive-requires-package-does-not-exist
///
/// The user requires package `a` but `a` requires package `b` which does not exist
///
/// saca2796a
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
    filters.push((r"saca2796a-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("saca2796a-a")
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

    assert_not_installed(&venv, "saca2796a_a", &temp_dir);

    Ok(())
}

/// excluded-only-version
///
/// Only one version of the requested package is available, but the user has banned
/// that version.
///
/// s7a9ed79c
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
    filters.push((r"s7a9ed79c-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("s7a9ed79c-a!=1.0.0")
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
    assert_not_installed(&venv, "s7a9ed79c_a", &temp_dir);

    Ok(())
}

/// excluded-only-compatible-version
///
/// Only one version of the requested package `a` is compatible, but the user has
/// banned that version.
///
/// sd28c9e3c
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
    filters.push((r"sd28c9e3c-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("sd28c9e3c-a!=2.0.0")
            .arg("sd28c9e3c-b>=2.0.0,<3.0.0")
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
    assert_not_installed(&venv, "sd28c9e3c_a", &temp_dir);
    assert_not_installed(&venv, "sd28c9e3c_b", &temp_dir);

    Ok(())
}

/// dependency-excludes-range-of-compatible-versions
///
/// There is a range of compatible versions for the requested package `a`, but
/// another dependency `c` excludes that range.
///
/// s2023222f
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
    filters.push((r"s2023222f-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("s2023222f-a")
            .arg("s2023222f-b>=2.0.0,<3.0.0")
            .arg("s2023222f-c")
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
              And because c==2.0.0 depends on a>=3.0.0 we can conclude that all versions of c depends on one of:
                  a<2.0.0
                  a>=3.0.0

              And because we know from (1) that any of:
                  a<2.0.0
                  a>=3.0.0
              depends on one of:
                  b<=1.0.0
                  b>=3.0.0
              we can conclude that all versions of c depends on one of:
                  b<=1.0.0
                  b>=3.0.0

              And because root depends on b>=2.0.0,<3.0.0 and root depends on c, we can conclude that the requirements are unsatisfiable.
        "###);
    });

    // Only the `2.x` versions of `a` are available since `a==1.0.0` and `a==3.0.0`
    // require incompatible versions of `b`, but all available versions of `c` exclude
    // that range of `a` so resolution fails.
    assert_not_installed(&venv, "s2023222f_a", &temp_dir);
    assert_not_installed(&venv, "s2023222f_b", &temp_dir);
    assert_not_installed(&venv, "s2023222f_c", &temp_dir);

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
/// saece4208
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
    filters.push((r"saece4208-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("saece4208-a")
            .arg("saece4208-b>=2.0.0,<3.0.0")
            .arg("saece4208-c")
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
                  c<1.0.0
                  c>1.0.0,<2.0.0
                  c>2.0.0
              and c==1.0.0 depends on a<2.0.0, we can conclude that c<2.0.0 depends on a<2.0.0. (1)

              Because a==1.0.0 depends on b==1.0.0 and there are no versions of a that satisfy any of:
                  a<1.0.0
                  a>1.0.0,<2.0.0
              we can conclude that a<2.0.0 depends on b==1.0.0.
              And because we know from (1) that c<2.0.0 depends on a<2.0.0, we can conclude that c<2.0.0 depends on b==1.0.0.
              And because c==2.0.0 depends on a>=3.0.0 we can conclude that !( a>=3.0.0 ), all versions of c, b!=1.0.0 are incompatible. (2)

              Because a==3.0.0 depends on b==3.0.0 and there are no versions of a that satisfy a>3.0.0, we can conclude that a>=3.0.0 depends on b==3.0.0.
              And because we know from (2) that !( a>=3.0.0 ), all versions of c, b!=1.0.0 are incompatible, we can conclude that all versions of c depends on one of:
                  b<=1.0.0
                  b>=3.0.0

              And because root depends on c and root depends on b>=2.0.0,<3.0.0, we can conclude that the requirements are unsatisfiable.
        "###);
    });

    // Only the `2.x` versions of `a` are available since `a==1.0.0` and `a==3.0.0`
    // require incompatible versions of `b`, but all available versions of `c` exclude
    // that range of `a` so resolution fails.
    assert_not_installed(&venv, "saece4208_a", &temp_dir);
    assert_not_installed(&venv, "saece4208_b", &temp_dir);
    assert_not_installed(&venv, "saece4208_c", &temp_dir);

    Ok(())
}

/// direct-incompatible-versions
///
/// The user requires two incompatible, existing versions of package `a`
///
/// s80d82ee8
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
fn direct_incompatible_versions() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"s80d82ee8-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("s80d82ee8-a==1.0.0")
            .arg("s80d82ee8-a==2.0.0")
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

    assert_not_installed(&venv, "s80d82ee8_a", &temp_dir);
    assert_not_installed(&venv, "s80d82ee8_a", &temp_dir);

    Ok(())
}

/// transitive-incompatible-with-root-version
///
/// The user requires packages `a` and `b` but `a` requires a different version of
/// `b`
///
/// sa967e815
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
fn transitive_incompatible_with_root_version() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"sa967e815-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("sa967e815-a")
            .arg("sa967e815-b==1.0.0")
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
          ╰─▶ Because a==1.0.0 depends on b==2.0.0 and there are no versions of a that satisfy any of:
                  a<1.0.0
                  a>1.0.0
              we can conclude that all versions of a depends on b==2.0.0.
              And because root depends on b==1.0.0 and root depends on a, we can conclude that the requirements are unsatisfiable.
        "###);
    });

    assert_not_installed(&venv, "sa967e815_a", &temp_dir);
    assert_not_installed(&venv, "sa967e815_b", &temp_dir);

    Ok(())
}

/// transitive-incompatible-with-transitive
///
/// The user requires package `a` and `b`; `a` and `b` require different versions of
/// `c`
///
/// s6866d8dc
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
fn transitive_incompatible_with_transitive() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"s6866d8dc-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("s6866d8dc-a")
            .arg("s6866d8dc-b")
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
              and a==1.0.0 depends on c==1.0.0, we can conclude that all versions of a depends on c==1.0.0.
              And because b==1.0.0 depends on c==2.0.0 and there are no versions of b that satisfy any of:
                  b<1.0.0
                  b>1.0.0
              we can conclude that all versions of a and all versions of b are incompatible.
              And because root depends on a and root depends on b, we can conclude that the requirements are unsatisfiable.
        "###);
    });

    assert_not_installed(&venv, "s6866d8dc_a", &temp_dir);
    assert_not_installed(&venv, "s6866d8dc_b", &temp_dir);

    Ok(())
}

/// package-only-prereleases
///
/// The user requires any version of package `a` which only has prerelease versions
/// available.
///
/// s9a1b3dda
/// ├── environment
/// │   └── python3.7
/// ├── root
/// │   └── requires a
/// │       └── unsatisfied: no matching version
/// └── a
///     └── a-1.0.0a1
#[test]
fn package_only_prereleases() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"s9a1b3dda-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("s9a1b3dda-a")
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
    assert_installed(&venv, "s9a1b3dda_a", "1.0.0a1", &temp_dir);

    Ok(())
}

/// package-only-prereleases-in-range
///
/// The user requires a version of package `a` which only matches prerelease
/// versions but they did not include a prerelease specifier.
///
/// s19673198
/// ├── environment
/// │   └── python3.7
/// ├── root
/// │   └── requires a>0.1.0
/// │       └── unsatisfied: no matching version
/// └── a
///     ├── a-0.1.0
///     └── a-1.0.0a1
#[test]
fn package_only_prereleases_in_range() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"s19673198-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("s19673198-a>0.1.0")
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
    assert_not_installed(&venv, "s19673198_a", &temp_dir);

    Ok(())
}

/// requires-package-only-prereleases-in-range-global-opt-in
///
/// The user requires a version of package `a` which only matches prerelease
/// versions. They did not include a prerelease specifier for the package, but they
/// opted into prereleases globally.
///
/// s51f94da2
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
    filters.push((r"s51f94da2-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("s51f94da2-a>0.1.0")
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

    assert_installed(&venv, "s51f94da2_a", "1.0.0a1", &temp_dir);

    Ok(())
}

/// requires-package-prerelease-and-final-any
///
/// The user requires any version of package `a` has a prerelease version available
/// and an older non-prerelease version.
///
/// seebe53a6
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
    filters.push((r"seebe53a6-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("seebe53a6-a")
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
    assert_installed(&venv, "seebe53a6_a", "0.1.0", &temp_dir);

    Ok(())
}

/// package-prerelease-specified-only-final-available
///
/// The user requires a version of `a` with a prerelease specifier and only stable
/// releases are available.
///
/// s9d4725eb
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
fn package_prerelease_specified_only_final_available() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"s9d4725eb-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("s9d4725eb-a>=0.1.0a1")
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
    assert_installed(&venv, "s9d4725eb_a", "0.3.0", &temp_dir);

    Ok(())
}

/// package-prerelease-specified-only-prerelease-available
///
/// The user requires a version of `a` with a prerelease specifier and only
/// prerelease releases are available.
///
/// s6cc95bc8
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
fn package_prerelease_specified_only_prerelease_available() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"s6cc95bc8-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("s6cc95bc8-a>=0.1.0a1")
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
    assert_installed(&venv, "s6cc95bc8_a", "0.3.0a1", &temp_dir);

    Ok(())
}

/// package-prerelease-specified-mixed-available
///
/// The user requires a version of `a` with a prerelease specifier and both
/// prerelease and stable releases are available.
///
/// sc97845e2
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
fn package_prerelease_specified_mixed_available() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"sc97845e2-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("sc97845e2-a>=0.1.0a1")
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
    assert_installed(&venv, "sc97845e2_a", "1.0.0a1", &temp_dir);

    Ok(())
}

/// package-multiple-prereleases-kinds
///
/// The user requires `a` which has multiple prereleases available with different
/// labels.
///
/// se290bf29
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
fn package_multiple_prereleases_kinds() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"se290bf29-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("se290bf29-a>=1.0.0a1")
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
    assert_installed(&venv, "se290bf29_a", "1.0.0rc1", &temp_dir);

    Ok(())
}

/// package-multiple-prereleases-numbers
///
/// The user requires `a` which has multiple alphas available.
///
/// sf5948c28
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
fn package_multiple_prereleases_numbers() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"sf5948c28-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("sf5948c28-a>=1.0.0a1")
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
    assert_installed(&venv, "sf5948c28_a", "1.0.0a3", &temp_dir);

    Ok(())
}

/// transitive-package-only-prereleases
///
/// The user requires any version of package `a` which requires `b` which only has
/// prerelease versions available.
///
/// s44ebef16
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
fn transitive_package_only_prereleases() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"s44ebef16-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("s44ebef16-a")
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
    assert_installed(&venv, "s44ebef16_a", "0.1.0", &temp_dir);
    assert_installed(&venv, "s44ebef16_b", "1.0.0a1", &temp_dir);

    Ok(())
}

/// transitive-package-only-prereleases-in-range
///
/// The user requires package `a` which has a dependency on a package which only
/// matches prerelease versions but they did not include a prerelease specifier.
///
/// s27759187
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
fn transitive_package_only_prereleases_in_range() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"s27759187-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("s27759187-a")
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
          ╰─▶ Because there are no versions of b that satisfy b>0.1 and a==0.1.0 depends on b>0.1, we can conclude that a==0.1.0 cannot be used.
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
    assert_not_installed(&venv, "s27759187_a", &temp_dir);

    Ok(())
}

/// transitive-package-only-prereleases-in-range-opt-in
///
/// The user requires package `a` which has a dependency on a package which only
/// matches prerelease versions; the user has opted into allowing prereleases in `b`
/// explicitly.
///
/// s26efb6c5
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
fn transitive_package_only_prereleases_in_range_opt_in() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"s26efb6c5-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("s26efb6c5-a")
            .arg("s26efb6c5-b>0.0.0a1")
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
    assert_installed(&venv, "s26efb6c5_a", "0.1.0", &temp_dir);
    assert_installed(&venv, "s26efb6c5_b", "1.0.0a1", &temp_dir);

    Ok(())
}

/// transitive-prerelease-and-stable-dependency
///
/// A transitive dependency has both a prerelease and a stable selector, but can
/// only be satisfied by a prerelease
///
/// sc7ad0310
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
fn transitive_prerelease_and_stable_dependency() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"sc7ad0310-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("sc7ad0310-a")
            .arg("sc7ad0310-b")
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
          ╰─▶ Because there is no version of c==2.0.0b1 and a==1.0.0 depends on c==2.0.0b1, we can conclude that a==1.0.0 cannot be used.
              And because there are no versions of a that satisfy any of:
                  a<1.0.0
                  a>1.0.0
              and root depends on a, we can conclude that the requirements are unsatisfiable.

              hint: c was requested with a pre-release marker (e.g., c==2.0.0b1), but pre-releases weren't enabled (try: `--prerelease=allow`)
        "###);
    });

    // Since the user did not explicitly opt-in to a prerelease, it cannot be selected.
    assert_not_installed(&venv, "sc7ad0310_a", &temp_dir);
    assert_not_installed(&venv, "sc7ad0310_b", &temp_dir);

    Ok(())
}

/// transitive-prerelease-and-stable-dependency-opt-in
///
/// A transitive dependency has both a prerelease and a stable selector, but can
/// only be satisfied by a prerelease. The user includes an opt-in to prereleases of
/// the transitive dependency.
///
/// sa05f7cb8
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
fn transitive_prerelease_and_stable_dependency_opt_in() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"sa05f7cb8-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("sa05f7cb8-a")
            .arg("sa05f7cb8-b")
            .arg("sa05f7cb8-c>=0.0.0a1")
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
    assert_installed(&venv, "sa05f7cb8_a", "1.0.0", &temp_dir);
    assert_installed(&venv, "sa05f7cb8_b", "1.0.0", &temp_dir);
    assert_installed(&venv, "sa05f7cb8_c", "2.0.0b1", &temp_dir);

    Ok(())
}

/// transitive-prerelease-and-stable-dependency-many-versions
///
/// A transitive dependency has both a prerelease and a stable selector, but can
/// only be satisfied by a prerelease. There are many prerelease versions.
///
/// s02ae765c
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
fn transitive_prerelease_and_stable_dependency_many_versions() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"s02ae765c-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("s02ae765c-a")
            .arg("s02ae765c-b")
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
              and b==1.0.0 depends on c, we can conclude that all versions of b depends on c.
              And because there are no versions of c that satisfy c>=2.0.0b1 we can conclude that all versions of b depends on c<2.0.0b1.
              And because a==1.0.0 depends on c>=2.0.0b1 and there are no versions of a that satisfy any of:
                  a<1.0.0
                  a>1.0.0
              we can conclude that all versions of b and all versions of a are incompatible.
              And because root depends on b and root depends on a, we can conclude that the requirements are unsatisfiable.

              hint: c was requested with a pre-release marker (e.g., c>=2.0.0b1), but pre-releases weren't enabled (try: `--prerelease=allow`)
        "###);
    });

    // Since the user did not explicitly opt-in to a prerelease, it cannot be selected.
    assert_not_installed(&venv, "s02ae765c_a", &temp_dir);
    assert_not_installed(&venv, "s02ae765c_b", &temp_dir);

    Ok(())
}

/// transitive-prerelease-and-stable-dependency-many-versions-holes
///
/// A transitive dependency has both a prerelease and a stable selector, but can
/// only be satisfied by a prerelease. There are many prerelease versions and some
/// are excluded.
///
/// sef9ce80f
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
fn transitive_prerelease_and_stable_dependency_many_versions_holes() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"sef9ce80f-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("sef9ce80f-a")
            .arg("sef9ce80f-b")
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
              we can conclude that a==1.0.0 cannot be used.
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
    assert_not_installed(&venv, "sef9ce80f_a", &temp_dir);
    assert_not_installed(&venv, "sef9ce80f_b", &temp_dir);

    Ok(())
}

/// requires-python-version-does-not-exist
///
/// The user requires a package which requires a Python version that does not exist
///
/// s0825b69c
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
    filters.push((r"s0825b69c-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("s0825b69c-a==1.0.0")
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
          ╰─▶ Because there are no versions of Python that satisfy Python>=4.0 and a==1.0.0 depends on Python>=4.0, we can conclude that a==1.0.0 cannot be used.
              And because root depends on a==1.0.0 we can conclude that the requirements are unsatisfiable.
        "###);
    });

    assert_not_installed(&venv, "s0825b69c_a", &temp_dir);

    Ok(())
}

/// requires-python-version-less-than-current
///
/// The user requires a package which requires a Python version less than the
/// current version
///
/// sf9296b84
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
    filters.push((r"sf9296b84-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("sf9296b84-a==1.0.0")
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
          ╰─▶ Because there are no versions of Python that satisfy Python<=3.8 and a==1.0.0 depends on Python<=3.8, we can conclude that a==1.0.0 cannot be used.
              And because root depends on a==1.0.0 we can conclude that the requirements are unsatisfiable.
        "###);
    });

    assert_not_installed(&venv, "sf9296b84_a", &temp_dir);

    Ok(())
}

/// requires-python-version-greater-than-current
///
/// The user requires a package which requires a Python version greater than the
/// current version
///
/// sa11d5394
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
    filters.push((r"sa11d5394-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("sa11d5394-a==1.0.0")
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
          ╰─▶ Because there are no versions of Python that satisfy Python>=3.10 and a==1.0.0 depends on Python>=3.10, we can conclude that a==1.0.0 cannot be used.
              And because root depends on a==1.0.0 we can conclude that the requirements are unsatisfiable.
        "###);
    });

    assert_not_installed(&venv, "sa11d5394_a", &temp_dir);

    Ok(())
}

/// requires-python-version-greater-than-current-many
///
/// The user requires a package which has many versions which all require a Python
/// version greater than the current version
///
/// s02dc550c
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
    filters.push((r"s02dc550c-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("s02dc550c-a==1.0.0")
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

    assert_not_installed(&venv, "s02dc550c_a", &temp_dir);

    Ok(())
}

/// requires-python-version-greater-than-current-backtrack
///
/// The user requires a package where recent versions require a Python version
/// greater than the current version, but an older version is compatible.
///
/// sef060cef
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
    filters.push((r"sef060cef-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("sef060cef-a")
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

    assert_installed(&venv, "sef060cef_a", "1.0.0", &temp_dir);

    Ok(())
}

/// requires-python-version-greater-than-current-excluded
///
/// The user requires a package where recent versions require a Python version
/// greater than the current version, but an excluded older version is compatible.
///
/// s1bde0c18
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
    filters.push((r"s1bde0c18-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("s1bde0c18-a>=2.0.0")
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
          ╰─▶ Because there are no versions of Python that satisfy Python>=3.10,<3.11 and there are no versions of Python that satisfy Python>=3.12, we can conclude that any of:
                  Python>=3.10,<3.11
                  Python>=3.12
               are incompatible.
              And because there are no versions of Python that satisfy Python>=3.11,<3.12 we can conclude that Python>=3.10 are incompatible.
              And because a==2.0.0 depends on Python>=3.10 and there are no versions of a that satisfy any of:
                  a>2.0.0,<3.0.0
                  a>3.0.0,<4.0.0
                  a>4.0.0
              we can conclude that a>=2.0.0,<3.0.0 cannot be used. (1)

              Because there are no versions of Python that satisfy Python>=3.11,<3.12 and there are no versions of Python that satisfy Python>=3.12, we can conclude that Python>=3.11 are incompatible.
              And because a==3.0.0 depends on Python>=3.11 we can conclude that a==3.0.0 cannot be used.
              And because we know from (1) that a>=2.0.0,<3.0.0 cannot be used, we can conclude that a>=2.0.0,<4.0.0 cannot be used. (2)

              Because there are no versions of Python that satisfy Python>=3.12 and a==4.0.0 depends on Python>=3.12, we can conclude that a==4.0.0 cannot be used.
              And because we know from (2) that a>=2.0.0,<4.0.0 cannot be used, we can conclude that a>=2.0.0 cannot be used.
              And because root depends on a>=2.0.0 we can conclude that the requirements are unsatisfiable.
        "###);
    });

    assert_not_installed(&venv, "s1bde0c18_a", &temp_dir);

    Ok(())
}

/// specific-tag-and-default
///
/// A wheel for a specific platform is available alongside the default.
///
/// s74e4a459
/// ├── environment
/// │   └── python3.7
/// ├── root
/// │   └── requires a
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
#[test]
fn specific_tag_and_default() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"s74e4a459-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("s74e4a459-a")
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

    Ok(())
}

/// only-wheels
///
/// No source distributions are available, only wheels.
///
/// s4f019491
/// ├── environment
/// │   └── python3.7
/// ├── root
/// │   └── requires a
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
#[test]
fn only_wheels() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"s4f019491-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("s4f019491-a")
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

    Ok(())
}

/// no-wheels
///
/// No wheels are available, only source distributions.
///
/// s614d801c
/// ├── environment
/// │   └── python3.7
/// ├── root
/// │   └── requires a
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
#[test]
fn no_wheels() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"s614d801c-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("s614d801c-a")
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

    Ok(())
}

/// no-wheels-with-matching-platform
///
/// No wheels with valid tags are available, just source distributions.
///
/// s737bbfd4
/// ├── environment
/// │   └── python3.7
/// ├── root
/// │   └── requires a
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
#[test]
fn no_wheels_with_matching_platform() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"s737bbfd4-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("s737bbfd4-a")
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

    Ok(())
}
