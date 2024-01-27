//! DO NOT EDIT
//!
//! Generated with ./scripts/scenarios/update.py
//! Scenarios from <https://github.com/zanieb/packse/tree/a451c95105db0c2897d0ad1bde0703ba0fc40d3a/scenarios>
//!
#![cfg(all(feature = "python", feature = "pypi"))]

use std::path::Path;
use std::process::Command;

use anyhow::Result;
use assert_cmd::assert::Assert;
use assert_cmd::prelude::*;
use insta_cmd::_macro_support::insta;
use insta_cmd::{assert_cmd_snapshot, get_cargo_bin};

use common::{create_venv, venv_to_interpreter, BIN_NAME, INSTA_FILTERS};

mod common;

fn assert_command(venv: &Path, command: &str, temp_dir: &Path) -> Assert {
    Command::new(venv_to_interpreter(venv))
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
/// ```text
/// 3cb60d4c
/// ├── environment
/// │   └── python3.8
/// └── root
///     └── requires a
///         └── unsatisfied: no versions for package
/// ```
#[test]
fn requires_package_does_not_exist() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"-3cb60d4c", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("a-3cb60d4c")
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

    assert_not_installed(&venv, "a_3cb60d4c", &temp_dir);

    Ok(())
}

/// requires-exact-version-does-not-exist
///
/// The user requires an exact version of package `a` but only other versions exist
///
/// ```text
/// e7132fc5
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a==2.0.0
/// │       └── unsatisfied: no matching version
/// └── a
///     └── a-1.0.0
/// ```
#[test]
fn requires_exact_version_does_not_exist() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-e7132fc5", "albatross"));
    filters.push((r"-e7132fc5", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("a-e7132fc5==2.0.0")
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
          ╰─▶ Because there is no version of albatross==2.0.0 and you require albatross==2.0.0, we can conclude that the requirements are unsatisfiable.
        "###);
    });

    assert_not_installed(&venv, "a_e7132fc5", &temp_dir);

    Ok(())
}

/// requires-greater-version-does-not-exist
///
/// The user requires a version of `a` greater than `1.0.0` but only smaller or
/// equal versions exist
///
/// ```text
/// 0e488e8f
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a>1.0.0
/// │       └── unsatisfied: no matching version
/// └── a
///     ├── a-0.1.0
///     └── a-1.0.0
/// ```
#[test]
fn requires_greater_version_does_not_exist() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-0e488e8f", "albatross"));
    filters.push((r"-0e488e8f", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("a-0e488e8f>1.0.0")
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
          ╰─▶ Because only albatross<=1.0.0 is available and you require albatross>1.0.0, we can conclude that the requirements are unsatisfiable.
        "###);
    });

    assert_not_installed(&venv, "a_0e488e8f", &temp_dir);

    Ok(())
}

/// requires-less-version-does-not-exist
///
/// The user requires a version of `a` less than `1.0.0` but only larger versions
/// exist
///
/// ```text
/// 1a4076bc
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a<2.0.0
/// │       └── unsatisfied: no matching version
/// └── a
///     ├── a-2.0.0
///     ├── a-3.0.0
///     └── a-4.0.0
/// ```
#[test]
fn requires_less_version_does_not_exist() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-1a4076bc", "albatross"));
    filters.push((r"-1a4076bc", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("a-1a4076bc<2.0.0")
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
          ╰─▶ Because only albatross>=2.0.0 is available and you require albatross<2.0.0, we can conclude that the requirements are unsatisfiable.
        "###);
    });

    assert_not_installed(&venv, "a_1a4076bc", &temp_dir);

    Ok(())
}

/// transitive-requires-package-does-not-exist
///
/// The user requires package `a` but `a` requires package `b` which does not exist
///
/// ```text
/// 22a72022
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
///         └── requires b
///             └── unsatisfied: no versions for package
/// ```
#[test]
fn transitive_requires_package_does_not_exist() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-22a72022", "albatross"));
    filters.push((r"-22a72022", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("a-22a72022")
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

    assert_not_installed(&venv, "a_22a72022", &temp_dir);

    Ok(())
}

/// excluded-only-version
///
/// Only one version of the requested package is available, but the user has banned
/// that version.
///
/// ```text
/// 2bc4455f
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a!=1.0.0
/// │       └── unsatisfied: no matching version
/// └── a
///     └── a-1.0.0
/// ```
#[test]
fn excluded_only_version() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-2bc4455f", "albatross"));
    filters.push((r"-2bc4455f", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("a-2bc4455f!=1.0.0")
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
          ╰─▶ Because only albatross==1.0.0 is available and you require one of:
                  albatross<1.0.0
                  albatross>1.0.0
              we can conclude that the requirements are unsatisfiable.
        "###);
    });

    // Only `a==1.0.0` is available but the user excluded it.
    assert_not_installed(&venv, "a_2bc4455f", &temp_dir);

    Ok(())
}

/// excluded-only-compatible-version
///
/// Only one version of the requested package `a` is compatible, but the user has
/// banned that version.
///
/// ```text
/// 5b90a629
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   ├── requires a!=2.0.0
/// │   │   ├── satisfied by a-1.0.0
/// │   │   └── satisfied by a-3.0.0
/// │   └── requires b<3.0.0,>=2.0.0
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
/// ```
#[test]
fn excluded_only_compatible_version() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-5b90a629", "albatross"));
    filters.push((r"b-5b90a629", "bluebird"));
    filters.push((r"-5b90a629", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("a-5b90a629!=2.0.0")
            .arg("b-5b90a629<3.0.0,>=2.0.0")
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
          ╰─▶ Because there are no versions of albatross that satisfy any of:
                  albatross<1.0.0
                  albatross>1.0.0,<2.0.0
                  albatross>2.0.0,<3.0.0
                  albatross>3.0.0
              and albatross==1.0.0 depends on bluebird==1.0.0, we can conclude that albatross<2.0.0 depends on bluebird==1.0.0.
              And because albatross==3.0.0 depends on bluebird==3.0.0, we can conclude that any of:
                  albatross<2.0.0
                  albatross>2.0.0
              depends on one of:
                  bluebird<=1.0.0
                  bluebird>=3.0.0

              And because you require bluebird>=2.0.0,<3.0.0 and you require one of:
                  albatross<2.0.0
                  albatross>2.0.0
              we can conclude that the requirements are unsatisfiable.
        "###);
    });

    // Only `a==1.2.0` is available since `a==1.0.0` and `a==3.0.0` require
    // incompatible versions of `b`. The user has excluded that version of `a` so
    // resolution fails.
    assert_not_installed(&venv, "a_5b90a629", &temp_dir);
    assert_not_installed(&venv, "b_5b90a629", &temp_dir);

    Ok(())
}

/// dependency-excludes-range-of-compatible-versions
///
/// There is a range of compatible versions for the requested package `a`, but
/// another dependency `c` excludes that range.
///
/// ```text
/// e59e1cd1
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   ├── requires a
/// │   │   ├── satisfied by a-1.0.0
/// │   │   ├── satisfied by a-2.0.0
/// │   │   ├── satisfied by a-2.1.0
/// │   │   ├── satisfied by a-2.2.0
/// │   │   ├── satisfied by a-2.3.0
/// │   │   └── satisfied by a-3.0.0
/// │   ├── requires b<3.0.0,>=2.0.0
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
/// ```
#[test]
fn dependency_excludes_range_of_compatible_versions() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-e59e1cd1", "albatross"));
    filters.push((r"b-e59e1cd1", "bluebird"));
    filters.push((r"c-e59e1cd1", "crow"));
    filters.push((r"-e59e1cd1", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("a-e59e1cd1")
            .arg("b-e59e1cd1<3.0.0,>=2.0.0")
            .arg("c-e59e1cd1")
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
          ╰─▶ Because there are no versions of crow that satisfy any of:
                  crow<1.0.0
                  crow>1.0.0,<2.0.0
                  crow>2.0.0
              and crow==1.0.0 depends on albatross<2.0.0, we can conclude that crow<2.0.0 depends on albatross<2.0.0. (1)

              Because there are no versions of albatross that satisfy any of:
                  albatross<1.0.0
                  albatross>1.0.0,<2.0.0
              and albatross==1.0.0 depends on bluebird==1.0.0, we can conclude that albatross<2.0.0 depends on bluebird==1.0.0.
              And because we know from (1) that crow<2.0.0 depends on albatross<2.0.0, we can conclude that crow<2.0.0 depends on bluebird==1.0.0.
              And because crow==2.0.0 depends on albatross>=3.0.0, we can conclude that all versions of crow, bluebird!=1.0.0, albatross<3.0.0 are incompatible. (2)

              Because only albatross<=3.0.0 is available and albatross==3.0.0 depends on bluebird==3.0.0, we can conclude that albatross>=3.0.0 depends on bluebird==3.0.0.
              And because we know from (2) that all versions of crow, bluebird!=1.0.0, albatross<3.0.0 are incompatible, we can conclude that all versions of crow depend on one of:
                  bluebird==1.0.0
                  bluebird==3.0.0

              And because you require crow and you require bluebird>=2.0.0,<3.0.0, we can conclude that the requirements are unsatisfiable.
        "###);
    });

    // Only the `2.x` versions of `a` are available since `a==1.0.0` and `a==3.0.0`
    // require incompatible versions of `b`, but all available versions of `c` exclude
    // that range of `a` so resolution fails.
    assert_not_installed(&venv, "a_e59e1cd1", &temp_dir);
    assert_not_installed(&venv, "b_e59e1cd1", &temp_dir);
    assert_not_installed(&venv, "c_e59e1cd1", &temp_dir);

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
/// ```text
/// ed41451f
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   ├── requires a
/// │   │   ├── satisfied by a-1.0.0
/// │   │   ├── satisfied by a-2.0.0
/// │   │   ├── satisfied by a-2.1.0
/// │   │   ├── satisfied by a-2.2.0
/// │   │   ├── satisfied by a-2.3.0
/// │   │   ├── satisfied by a-2.4.0
/// │   │   └── satisfied by a-3.0.0
/// │   ├── requires b<3.0.0,>=2.0.0
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
/// ```
#[test]
fn dependency_excludes_non_contiguous_range_of_compatible_versions() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-ed41451f", "albatross"));
    filters.push((r"b-ed41451f", "bluebird"));
    filters.push((r"c-ed41451f", "crow"));
    filters.push((r"-ed41451f", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("a-ed41451f")
            .arg("b-ed41451f<3.0.0,>=2.0.0")
            .arg("c-ed41451f")
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
          ╰─▶ Because there are no versions of crow that satisfy any of:
                  crow<1.0.0
                  crow>1.0.0,<2.0.0
                  crow>2.0.0
              and crow==1.0.0 depends on albatross<2.0.0, we can conclude that crow<2.0.0 depends on albatross<2.0.0. (1)

              Because there are no versions of albatross that satisfy any of:
                  albatross<1.0.0
                  albatross>1.0.0,<2.0.0
              and albatross==1.0.0 depends on bluebird==1.0.0, we can conclude that albatross<2.0.0 depends on bluebird==1.0.0.
              And because we know from (1) that crow<2.0.0 depends on albatross<2.0.0, we can conclude that crow<2.0.0 depends on bluebird==1.0.0.
              And because crow==2.0.0 depends on albatross>=3.0.0, we can conclude that all versions of crow, bluebird!=1.0.0, albatross<3.0.0 are incompatible. (2)

              Because only albatross<=3.0.0 is available and albatross==3.0.0 depends on bluebird==3.0.0, we can conclude that albatross>=3.0.0 depends on bluebird==3.0.0.
              And because we know from (2) that all versions of crow, bluebird!=1.0.0, albatross<3.0.0 are incompatible, we can conclude that all versions of crow depend on one of:
                  bluebird==1.0.0
                  bluebird==3.0.0

              And because you require crow and you require bluebird>=2.0.0,<3.0.0, we can conclude that the requirements are unsatisfiable.
        "###);
    });

    // Only the `2.x` versions of `a` are available since `a==1.0.0` and `a==3.0.0`
    // require incompatible versions of `b`, but all available versions of `c` exclude
    // that range of `a` so resolution fails.
    assert_not_installed(&venv, "a_ed41451f", &temp_dir);
    assert_not_installed(&venv, "b_ed41451f", &temp_dir);
    assert_not_installed(&venv, "c_ed41451f", &temp_dir);

    Ok(())
}

/// extra-required
///
/// Optional dependencies are requested for the package.
///
/// ```text
/// c9be513b
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a[extra]
/// │       ├── satisfied by a-1.0.0
/// │       └── satisfied by a-1.0.0[extra]
/// ├── a
/// │   ├── a-1.0.0
/// │   └── a-1.0.0[extra]
/// │       └── requires b
/// │           └── satisfied by b-1.0.0
/// └── b
///     └── b-1.0.0
/// ```
#[test]
fn extra_required() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-c9be513b", "albatross"));
    filters.push((r"b-c9be513b", "bluebird"));
    filters.push((r"-c9be513b", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("a-c9be513b[extra]")
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
         + albatross==1.0.0
         + bluebird==1.0.0
        "###);
    });

    assert_installed(&venv, "a_c9be513b", "1.0.0", &temp_dir);
    assert_installed(&venv, "b_c9be513b", "1.0.0", &temp_dir);

    Ok(())
}

/// missing-extra
///
/// Optional dependencies are requested for the package, but the extra does not
/// exist.
///
/// ```text
/// 79fd9a92
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a[extra]
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
/// ```
#[test]
fn missing_extra() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-79fd9a92", "albatross"));
    filters.push((r"-79fd9a92", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("a-79fd9a92[extra]")
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
         + albatross==1.0.0
        "###);
    });

    // Missing extras are ignored during resolution.
    assert_installed(&venv, "a_79fd9a92", "1.0.0", &temp_dir);

    Ok(())
}

/// multiple-extras-required
///
/// Multiple optional dependencies are requested for the package.
///
/// ```text
/// 14317535
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a[extra_b,extra_c]
/// │       ├── satisfied by a-1.0.0
/// │       ├── satisfied by a-1.0.0[extra_b]
/// │       └── satisfied by a-1.0.0[extra_c]
/// ├── a
/// │   ├── a-1.0.0
/// │   ├── a-1.0.0[extra_b]
/// │   │   └── requires b
/// │   │       └── satisfied by b-1.0.0
/// │   └── a-1.0.0[extra_c]
/// │       └── requires c
/// │           └── satisfied by c-1.0.0
/// ├── b
/// │   └── b-1.0.0
/// └── c
///     └── c-1.0.0
/// ```
#[test]
fn multiple_extras_required() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-14317535", "albatross"));
    filters.push((r"b-14317535", "bluebird"));
    filters.push((r"c-14317535", "crow"));
    filters.push((r"-14317535", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("a-14317535[extra_b,extra_c]")
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
         + albatross==1.0.0
         + bluebird==1.0.0
         + crow==1.0.0
        "###);
    });

    assert_installed(&venv, "a_14317535", "1.0.0", &temp_dir);
    assert_installed(&venv, "b_14317535", "1.0.0", &temp_dir);
    assert_installed(&venv, "c_14317535", "1.0.0", &temp_dir);

    Ok(())
}

/// extra-incompatible-with-extra
///
/// Multiple optional dependencies are requested for the package, but they have
/// conflicting requirements with each other.
///
/// ```text
/// afa38951
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a[extra_b,extra_c]
/// │       ├── satisfied by a-1.0.0
/// │       ├── satisfied by a-1.0.0[extra_b]
/// │       └── satisfied by a-1.0.0[extra_c]
/// ├── a
/// │   ├── a-1.0.0
/// │   ├── a-1.0.0[extra_b]
/// │   │   └── requires b==1.0.0
/// │   │       └── satisfied by b-1.0.0
/// │   └── a-1.0.0[extra_c]
/// │       └── requires b==2.0.0
/// │           └── satisfied by b-2.0.0
/// └── b
///     ├── b-1.0.0
///     └── b-2.0.0
/// ```
#[test]
fn extra_incompatible_with_extra() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-afa38951", "albatross"));
    filters.push((r"b-afa38951", "bluebird"));
    filters.push((r"-afa38951", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("a-afa38951[extra_b,extra_c]")
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
          ╰─▶ Because only albatross[extra-b]==1.0.0 is available and albatross[extra-b]==1.0.0 depends on bluebird==1.0.0, we can conclude that all versions of albatross[extra-b] depend on bluebird==1.0.0.
              And because albatross[extra-c]==1.0.0 depends on bluebird==2.0.0 and only albatross[extra-c]==1.0.0 is available, we can conclude that all versions of albatross[extra-c] and all versions of albatross[extra-b] are incompatible.
              And because you require albatross[extra-b] and you require albatross[extra-c], we can conclude that the requirements are unsatisfiable.
        "###);
    });

    // Because both `extra_b` and `extra_c` are requested and they require incompatible
    // versions of `b`, `a` cannot be installed.
    assert_not_installed(&venv, "a_afa38951", &temp_dir);

    Ok(())
}

/// extra-incompatible-with-extra-not-requested
///
/// One of two incompatible optional dependencies are requested for the package.
///
/// ```text
/// 5c305dd9
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a[extra_c]
/// │       ├── satisfied by a-1.0.0
/// │       ├── satisfied by a-1.0.0[extra_b]
/// │       └── satisfied by a-1.0.0[extra_c]
/// ├── a
/// │   ├── a-1.0.0
/// │   ├── a-1.0.0[extra_b]
/// │   │   └── requires b==1.0.0
/// │   │       └── satisfied by b-1.0.0
/// │   └── a-1.0.0[extra_c]
/// │       └── requires b==2.0.0
/// │           └── satisfied by b-2.0.0
/// └── b
///     ├── b-1.0.0
///     └── b-2.0.0
/// ```
#[test]
fn extra_incompatible_with_extra_not_requested() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-5c305dd9", "albatross"));
    filters.push((r"b-5c305dd9", "bluebird"));
    filters.push((r"-5c305dd9", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("a-5c305dd9[extra_c]")
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
         + albatross==1.0.0
         + bluebird==2.0.0
        "###);
    });

    // Because the user does not request both extras, it is okay that one is
    // incompatible with the other.
    assert_installed(&venv, "a_5c305dd9", "1.0.0", &temp_dir);
    assert_installed(&venv, "b_5c305dd9", "2.0.0", &temp_dir);

    Ok(())
}

/// extra-incompatible-with-root
///
/// Optional dependencies are requested for the package, but the extra is not
/// compatible with other requested versions.
///
/// ```text
/// 743dac5a
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   ├── requires a[extra]
/// │   │   ├── satisfied by a-1.0.0
/// │   │   └── satisfied by a-1.0.0[extra]
/// │   └── requires b==2.0.0
/// │       └── satisfied by b-2.0.0
/// ├── a
/// │   ├── a-1.0.0
/// │   └── a-1.0.0[extra]
/// │       └── requires b==1.0.0
/// │           └── satisfied by b-1.0.0
/// └── b
///     ├── b-1.0.0
///     └── b-2.0.0
/// ```
#[test]
fn extra_incompatible_with_root() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-743dac5a", "albatross"));
    filters.push((r"b-743dac5a", "bluebird"));
    filters.push((r"-743dac5a", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("a-743dac5a[extra]")
            .arg("b-743dac5a==2.0.0")
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
          ╰─▶ Because only albatross[extra]==1.0.0 is available and albatross[extra]==1.0.0 depends on bluebird==1.0.0, we can conclude that all versions of albatross[extra] depend on bluebird==1.0.0.
              And because you require bluebird==2.0.0 and you require albatross[extra], we can conclude that the requirements are unsatisfiable.
        "###);
    });

    // Because the user requested `b==2.0.0` but the requested extra requires
    // `b==1.0.0`, the dependencies cannot be satisfied.
    assert_not_installed(&venv, "a_743dac5a", &temp_dir);
    assert_not_installed(&venv, "b_743dac5a", &temp_dir);

    Ok(())
}

/// extra-does-not-exist-backtrack
///
/// Optional dependencies are requested for the package, the extra is only available
/// on an older version.
///
/// ```text
/// a35e4442
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a[extra]
/// │       ├── satisfied by a-1.0.0
/// │       ├── satisfied by a-1.0.0[extra]
/// │       ├── satisfied by a-2.0.0
/// │       └── satisfied by a-3.0.0
/// ├── a
/// │   ├── a-1.0.0
/// │   ├── a-1.0.0[extra]
/// │   │   └── requires b==1.0.0
/// │   │       └── satisfied by b-1.0.0
/// │   ├── a-2.0.0
/// │   └── a-3.0.0
/// └── b
///     └── b-1.0.0
/// ```
#[test]
fn extra_does_not_exist_backtrack() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-a35e4442", "albatross"));
    filters.push((r"b-a35e4442", "bluebird"));
    filters.push((r"-a35e4442", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("a-a35e4442[extra]")
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
         + albatross==3.0.0
        "###);
    });

    // The resolver should not backtrack to `a==1.0.0` because missing extras are
    // allowed during resolution. `b` should not be installed.
    assert_installed(&venv, "a_a35e4442", "3.0.0", &temp_dir);

    Ok(())
}

/// direct-incompatible-versions
///
/// The user requires two incompatible, existing versions of package `a`
///
/// ```text
/// f75c56e2
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   ├── requires a==1.0.0
/// │   │   └── satisfied by a-1.0.0
/// │   └── requires a==2.0.0
/// │       └── satisfied by a-2.0.0
/// └── a
///     ├── a-1.0.0
///     └── a-2.0.0
/// ```
#[test]
fn direct_incompatible_versions() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-f75c56e2", "albatross"));
    filters.push((r"-f75c56e2", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("a-f75c56e2==1.0.0")
            .arg("a-f75c56e2==2.0.0")
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
          ╰─▶ your requirements cannot be used because there are conflicting versions for `albatross`: `albatross==1.0.0` does not intersect with `albatross==2.0.0`
        "###);
    });

    assert_not_installed(&venv, "a_f75c56e2", &temp_dir);
    assert_not_installed(&venv, "a_f75c56e2", &temp_dir);

    Ok(())
}

/// transitive-incompatible-with-root-version
///
/// The user requires packages `a` and `b` but `a` requires a different version of
/// `b`
///
/// ```text
/// 812a0fda
/// ├── environment
/// │   └── python3.8
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
/// ```
#[test]
fn transitive_incompatible_with_root_version() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-812a0fda", "albatross"));
    filters.push((r"b-812a0fda", "bluebird"));
    filters.push((r"-812a0fda", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("a-812a0fda")
            .arg("b-812a0fda==1.0.0")
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
          ╰─▶ Because albatross==1.0.0 depends on bluebird==2.0.0 and only albatross==1.0.0 is available, we can conclude that all versions of albatross depend on bluebird==2.0.0.
              And because you require bluebird==1.0.0 and you require albatross, we can conclude that the requirements are unsatisfiable.
        "###);
    });

    assert_not_installed(&venv, "a_812a0fda", &temp_dir);
    assert_not_installed(&venv, "b_812a0fda", &temp_dir);

    Ok(())
}

/// transitive-incompatible-with-transitive
///
/// The user requires package `a` and `b`; `a` and `b` require different versions of
/// `c`
///
/// ```text
/// 74531fe0
/// ├── environment
/// │   └── python3.8
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
/// ```
#[test]
fn transitive_incompatible_with_transitive() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-74531fe0", "albatross"));
    filters.push((r"b-74531fe0", "bluebird"));
    filters.push((r"c-74531fe0", "crow"));
    filters.push((r"-74531fe0", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("a-74531fe0")
            .arg("b-74531fe0")
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
          ╰─▶ Because only albatross==1.0.0 is available and albatross==1.0.0 depends on crow==1.0.0, we can conclude that all versions of albatross depend on crow==1.0.0.
              And because bluebird==1.0.0 depends on crow==2.0.0 and only bluebird==1.0.0 is available, we can conclude that all versions of albatross and all versions of bluebird are incompatible.
              And because you require albatross and you require bluebird, we can conclude that the requirements are unsatisfiable.
        "###);
    });

    assert_not_installed(&venv, "a_74531fe0", &temp_dir);
    assert_not_installed(&venv, "b_74531fe0", &temp_dir);

    Ok(())
}

/// package-only-prereleases
///
/// The user requires any version of package `a` which only has prerelease versions
/// available.
///
/// ```text
/// e2dbc237
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a
/// │       └── unsatisfied: no matching version
/// └── a
///     └── a-1.0.0a1
/// ```
#[test]
fn package_only_prereleases() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-e2dbc237", "albatross"));
    filters.push((r"-e2dbc237", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("a-e2dbc237")
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
         + albatross==1.0.0a1
        "###);
    });

    // Since there are only prerelease versions of `a` available, it should be
    // installed even though the user did not include a prerelease specifier.
    assert_installed(&venv, "a_e2dbc237", "1.0.0a1", &temp_dir);

    Ok(())
}

/// package-only-prereleases-in-range
///
/// The user requires a version of package `a` which only matches prerelease
/// versions but they did not include a prerelease specifier.
///
/// ```text
/// 4e199000
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a>0.1.0
/// │       └── unsatisfied: no matching version
/// └── a
///     ├── a-0.1.0
///     └── a-1.0.0a1
/// ```
#[test]
fn package_only_prereleases_in_range() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-4e199000", "albatross"));
    filters.push((r"-4e199000", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("a-4e199000>0.1.0")
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
          ╰─▶ Because only albatross<=0.1.0 is available and you require albatross>0.1.0, we can conclude that the requirements are unsatisfiable.

              hint: Pre-releases are available for albatross in the requested range (e.g., 1.0.0a1), but pre-releases weren't enabled (try: `--prerelease=allow`)
        "###);
    });

    // Since there are stable versions of `a` available, prerelease versions should not
    // be selected without explicit opt-in.
    assert_not_installed(&venv, "a_4e199000", &temp_dir);

    Ok(())
}

/// requires-package-only-prereleases-in-range-global-opt-in
///
/// The user requires a version of package `a` which only matches prerelease
/// versions. They did not include a prerelease specifier for the package, but they
/// opted into prereleases globally.
///
/// ```text
/// af391f9c
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a>0.1.0
/// │       └── unsatisfied: no matching version
/// └── a
///     ├── a-0.1.0
///     └── a-1.0.0a1
/// ```
#[test]
fn requires_package_only_prereleases_in_range_global_opt_in() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-af391f9c", "albatross"));
    filters.push((r"-af391f9c", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("a-af391f9c>0.1.0")
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
         + albatross==1.0.0a1
        "###);
    });

    assert_installed(&venv, "a_af391f9c", "1.0.0a1", &temp_dir);

    Ok(())
}

/// requires-package-prerelease-and-final-any
///
/// The user requires any version of package `a` has a prerelease version available
/// and an older non-prerelease version.
///
/// ```text
/// 81dbab9d
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a
/// │       └── satisfied by a-0.1.0
/// └── a
///     ├── a-0.1.0
///     └── a-1.0.0a1
/// ```
#[test]
fn requires_package_prerelease_and_final_any() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-81dbab9d", "albatross"));
    filters.push((r"-81dbab9d", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("a-81dbab9d")
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
         + albatross==0.1.0
        "###);
    });

    // Since the user did not provide a prerelease specifier, the older stable version
    // should be selected.
    assert_installed(&venv, "a_81dbab9d", "0.1.0", &temp_dir);

    Ok(())
}

/// package-prerelease-specified-only-final-available
///
/// The user requires a version of `a` with a prerelease specifier and only stable
/// releases are available.
///
/// ```text
/// 2ee8f3f9
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a>=0.1.0a1
/// │       ├── satisfied by a-0.1.0
/// │       ├── satisfied by a-0.2.0
/// │       └── satisfied by a-0.3.0
/// └── a
///     ├── a-0.1.0
///     ├── a-0.2.0
///     └── a-0.3.0
/// ```
#[test]
fn package_prerelease_specified_only_final_available() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-2ee8f3f9", "albatross"));
    filters.push((r"-2ee8f3f9", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("a-2ee8f3f9>=0.1.0a1")
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
         + albatross==0.3.0
        "###);
    });

    // The latest stable version should be selected.
    assert_installed(&venv, "a_2ee8f3f9", "0.3.0", &temp_dir);

    Ok(())
}

/// package-prerelease-specified-only-prerelease-available
///
/// The user requires a version of `a` with a prerelease specifier and only
/// prerelease releases are available.
///
/// ```text
/// dd6b1e32
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a>=0.1.0a1
/// │       ├── satisfied by a-0.1.0a1
/// │       ├── satisfied by a-0.2.0a1
/// │       └── satisfied by a-0.3.0a1
/// └── a
///     ├── a-0.1.0a1
///     ├── a-0.2.0a1
///     └── a-0.3.0a1
/// ```
#[test]
fn package_prerelease_specified_only_prerelease_available() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-dd6b1e32", "albatross"));
    filters.push((r"-dd6b1e32", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("a-dd6b1e32>=0.1.0a1")
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
         + albatross==0.3.0a1
        "###);
    });

    // The latest prerelease version should be selected.
    assert_installed(&venv, "a_dd6b1e32", "0.3.0a1", &temp_dir);

    Ok(())
}

/// package-prerelease-specified-mixed-available
///
/// The user requires a version of `a` with a prerelease specifier and both
/// prerelease and stable releases are available.
///
/// ```text
/// 794fce4d
/// ├── environment
/// │   └── python3.8
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
/// ```
#[test]
fn package_prerelease_specified_mixed_available() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-794fce4d", "albatross"));
    filters.push((r"-794fce4d", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("a-794fce4d>=0.1.0a1")
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
         + albatross==1.0.0a1
        "###);
    });

    // Since the user provided a prerelease specifier, the latest prerelease version
    // should be selected.
    assert_installed(&venv, "a_794fce4d", "1.0.0a1", &temp_dir);

    Ok(())
}

/// package-multiple-prereleases-kinds
///
/// The user requires `a` which has multiple prereleases available with different
/// labels.
///
/// ```text
/// 256d4e16
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a>=1.0.0a1
/// │       ├── satisfied by a-1.0.0a1
/// │       ├── satisfied by a-1.0.0b1
/// │       └── satisfied by a-1.0.0rc1
/// └── a
///     ├── a-1.0.0a1
///     ├── a-1.0.0b1
///     └── a-1.0.0rc1
/// ```
#[test]
fn package_multiple_prereleases_kinds() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-256d4e16", "albatross"));
    filters.push((r"-256d4e16", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("a-256d4e16>=1.0.0a1")
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
         + albatross==1.0.0rc1
        "###);
    });

    // Release candidates should be the highest precedence prerelease kind.
    assert_installed(&venv, "a_256d4e16", "1.0.0rc1", &temp_dir);

    Ok(())
}

/// package-multiple-prereleases-numbers
///
/// The user requires `a` which has multiple alphas available.
///
/// ```text
/// ac82d3fb
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a>=1.0.0a1
/// │       ├── satisfied by a-1.0.0a1
/// │       ├── satisfied by a-1.0.0a2
/// │       └── satisfied by a-1.0.0a3
/// └── a
///     ├── a-1.0.0a1
///     ├── a-1.0.0a2
///     └── a-1.0.0a3
/// ```
#[test]
fn package_multiple_prereleases_numbers() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-ac82d3fb", "albatross"));
    filters.push((r"-ac82d3fb", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("a-ac82d3fb>=1.0.0a1")
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
         + albatross==1.0.0a3
        "###);
    });

    // The latest alpha version should be selected.
    assert_installed(&venv, "a_ac82d3fb", "1.0.0a3", &temp_dir);

    Ok(())
}

/// transitive-package-only-prereleases
///
/// The user requires any version of package `a` which requires `b` which only has
/// prerelease versions available.
///
/// ```text
/// 9da8532e
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a
/// │       └── satisfied by a-0.1.0
/// ├── a
/// │   └── a-0.1.0
/// │       └── requires b
/// │           └── unsatisfied: no matching version
/// └── b
///     └── b-1.0.0a1
/// ```
#[test]
fn transitive_package_only_prereleases() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-9da8532e", "albatross"));
    filters.push((r"b-9da8532e", "bluebird"));
    filters.push((r"-9da8532e", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("a-9da8532e")
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
         + albatross==0.1.0
         + bluebird==1.0.0a1
        "###);
    });

    // Since there are only prerelease versions of `b` available, it should be selected
    // even though the user did not opt-in to prereleases.
    assert_installed(&venv, "a_9da8532e", "0.1.0", &temp_dir);
    assert_installed(&venv, "b_9da8532e", "1.0.0a1", &temp_dir);

    Ok(())
}

/// transitive-package-only-prereleases-in-range
///
/// The user requires package `a` which has a dependency on a package which only
/// matches prerelease versions but they did not include a prerelease specifier.
///
/// ```text
/// c41a54fc
/// ├── environment
/// │   └── python3.8
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
/// ```
#[test]
fn transitive_package_only_prereleases_in_range() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-c41a54fc", "albatross"));
    filters.push((r"b-c41a54fc", "bluebird"));
    filters.push((r"-c41a54fc", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("a-c41a54fc")
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
          ╰─▶ Because only bluebird<=0.1 is available and albatross==0.1.0 depends on bluebird>0.1, we can conclude that albatross==0.1.0 cannot be used.
              And because only albatross==0.1.0 is available and you require albatross, we can conclude that the requirements are unsatisfiable.

              hint: Pre-releases are available for bluebird in the requested range (e.g., 1.0.0a1), but pre-releases weren't enabled (try: `--prerelease=allow`)
        "###);
    });

    // Since there are stable versions of `b` available, the prerelease version should
    // not be selected without explicit opt-in. The available version is excluded by
    // the range requested by the user.
    assert_not_installed(&venv, "a_c41a54fc", &temp_dir);

    Ok(())
}

/// transitive-package-only-prereleases-in-range-opt-in
///
/// The user requires package `a` which has a dependency on a package which only
/// matches prerelease versions; the user has opted into allowing prereleases in `b`
/// explicitly.
///
/// ```text
/// 96f98f65
/// ├── environment
/// │   └── python3.8
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
/// ```
#[test]
fn transitive_package_only_prereleases_in_range_opt_in() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-96f98f65", "albatross"));
    filters.push((r"b-96f98f65", "bluebird"));
    filters.push((r"-96f98f65", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("a-96f98f65")
            .arg("b-96f98f65>0.0.0a1")
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
         + albatross==0.1.0
         + bluebird==1.0.0a1
        "###);
    });

    // Since the user included a dependency on `b` with a prerelease specifier, a
    // prerelease version can be selected.
    assert_installed(&venv, "a_96f98f65", "0.1.0", &temp_dir);
    assert_installed(&venv, "b_96f98f65", "1.0.0a1", &temp_dir);

    Ok(())
}

/// transitive-prerelease-and-stable-dependency
///
/// A transitive dependency has both a prerelease and a stable selector, but can
/// only be satisfied by a prerelease
///
/// ```text
/// 3d5eb91f
/// ├── environment
/// │   └── python3.8
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
/// │       └── requires c<=3.0.0,>=1.0.0
/// │           └── satisfied by c-1.0.0
/// └── c
///     ├── c-1.0.0
///     └── c-2.0.0b1
/// ```
#[test]
fn transitive_prerelease_and_stable_dependency() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-3d5eb91f", "albatross"));
    filters.push((r"b-3d5eb91f", "bluebird"));
    filters.push((r"c-3d5eb91f", "crow"));
    filters.push((r"-3d5eb91f", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("a-3d5eb91f")
            .arg("b-3d5eb91f")
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
          ╰─▶ Because there is no version of crow==2.0.0b1 and albatross==1.0.0 depends on crow==2.0.0b1, we can conclude that albatross==1.0.0 cannot be used.
              And because only albatross==1.0.0 is available and you require albatross, we can conclude that the requirements are unsatisfiable.

              hint: crow was requested with a pre-release marker (e.g., crow==2.0.0b1), but pre-releases weren't enabled (try: `--prerelease=allow`)
        "###);
    });

    // Since the user did not explicitly opt-in to a prerelease, it cannot be selected.
    assert_not_installed(&venv, "a_3d5eb91f", &temp_dir);
    assert_not_installed(&venv, "b_3d5eb91f", &temp_dir);

    Ok(())
}

/// transitive-prerelease-and-stable-dependency-opt-in
///
/// A transitive dependency has both a prerelease and a stable selector, but can
/// only be satisfied by a prerelease. The user includes an opt-in to prereleases of
/// the transitive dependency.
///
/// ```text
/// 6192ec57
/// ├── environment
/// │   └── python3.8
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
/// │       └── requires c<=3.0.0,>=1.0.0
/// │           └── satisfied by c-1.0.0
/// └── c
///     ├── c-1.0.0
///     └── c-2.0.0b1
/// ```
#[test]
fn transitive_prerelease_and_stable_dependency_opt_in() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-6192ec57", "albatross"));
    filters.push((r"b-6192ec57", "bluebird"));
    filters.push((r"c-6192ec57", "crow"));
    filters.push((r"-6192ec57", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("a-6192ec57")
            .arg("b-6192ec57")
            .arg("c-6192ec57>=0.0.0a1")
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
         + albatross==1.0.0
         + bluebird==1.0.0
         + crow==2.0.0b1
        "###);
    });

    // Since the user explicitly opted-in to a prerelease for `c`, it can be installed.
    assert_installed(&venv, "a_6192ec57", "1.0.0", &temp_dir);
    assert_installed(&venv, "b_6192ec57", "1.0.0", &temp_dir);
    assert_installed(&venv, "c_6192ec57", "2.0.0b1", &temp_dir);

    Ok(())
}

/// transitive-prerelease-and-stable-dependency-many-versions
///
/// A transitive dependency has both a prerelease and a stable selector, but can
/// only be satisfied by a prerelease. There are many prerelease versions.
///
/// ```text
/// bcd0f988
/// ├── environment
/// │   └── python3.8
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
/// │       └── requires c<=3.0.0,>=1.0.0
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
/// ```
#[test]
fn transitive_prerelease_and_stable_dependency_many_versions() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-bcd0f988", "albatross"));
    filters.push((r"b-bcd0f988", "bluebird"));
    filters.push((r"c-bcd0f988", "crow"));
    filters.push((r"-bcd0f988", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("a-bcd0f988")
            .arg("b-bcd0f988")
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
          ╰─▶ Because only bluebird==1.0.0 is available and bluebird==1.0.0 depends on crow, we can conclude that all versions of bluebird depend on crow.
              And because only crow<2.0.0b1 is available, we can conclude that all versions of bluebird depend on crow<2.0.0b1.
              And because albatross==1.0.0 depends on crow>=2.0.0b1 and only albatross==1.0.0 is available, we can conclude that all versions of bluebird and all versions of albatross are incompatible.
              And because you require bluebird and you require albatross, we can conclude that the requirements are unsatisfiable.

              hint: crow was requested with a pre-release marker (e.g., crow>=2.0.0b1), but pre-releases weren't enabled (try: `--prerelease=allow`)
        "###);
    });

    // Since the user did not explicitly opt-in to a prerelease, it cannot be selected.
    assert_not_installed(&venv, "a_bcd0f988", &temp_dir);
    assert_not_installed(&venv, "b_bcd0f988", &temp_dir);

    Ok(())
}

/// transitive-prerelease-and-stable-dependency-many-versions-holes
///
/// A transitive dependency has both a prerelease and a stable selector, but can
/// only be satisfied by a prerelease. There are many prerelease versions and some
/// are excluded.
///
/// ```text
/// 5cee052e
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   ├── requires a
/// │   │   └── satisfied by a-1.0.0
/// │   └── requires b
/// │       └── satisfied by b-1.0.0
/// ├── a
/// │   └── a-1.0.0
/// │       └── requires c!=2.0.0a5,!=2.0.0a6,!=2.0.0a7,!=2.0.0b1,<2.0.0b5,>1.0.0
/// │           └── unsatisfied: no matching version
/// ├── b
/// │   └── b-1.0.0
/// │       └── requires c<=3.0.0,>=1.0.0
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
/// ```
#[test]
fn transitive_prerelease_and_stable_dependency_many_versions_holes() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-5cee052e", "albatross"));
    filters.push((r"b-5cee052e", "bluebird"));
    filters.push((r"c-5cee052e", "crow"));
    filters.push((r"-5cee052e", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("a-5cee052e")
            .arg("b-5cee052e")
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
          ╰─▶ Because there are no versions of crow that satisfy any of:
                  crow>1.0.0,<2.0.0a5
                  crow>2.0.0a7,<2.0.0b1
                  crow>2.0.0b1,<2.0.0b5
              and albatross==1.0.0 depends on one of:
                  crow>1.0.0,<2.0.0a5
                  crow>2.0.0a7,<2.0.0b1
                  crow>2.0.0b1,<2.0.0b5
              we can conclude that albatross==1.0.0 cannot be used.
              And because only albatross==1.0.0 is available and you require albatross, we can conclude that the requirements are unsatisfiable.

              hint: crow was requested with a pre-release marker (e.g., any of:
                  crow>1.0.0,<2.0.0a5
                  crow>2.0.0a7,<2.0.0b1
                  crow>2.0.0b1,<2.0.0b5
              ), but pre-releases weren't enabled (try: `--prerelease=allow`)
        "###);
    });

    // Since the user did not explicitly opt-in to a prerelease, it cannot be selected.
    assert_not_installed(&venv, "a_5cee052e", &temp_dir);
    assert_not_installed(&venv, "b_5cee052e", &temp_dir);

    Ok(())
}

/// requires-python-version-does-not-exist
///
/// The user requires a package which requires a Python version that does not exist
///
/// ```text
/// d004e577
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a==1.0.0
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
///         └── requires python>=4.0 (incompatible with environment)
/// ```
#[test]
fn requires_python_version_does_not_exist() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-d004e577", "albatross"));
    filters.push((r"-d004e577", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("a-d004e577==1.0.0")
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
          ╰─▶ Because the current Python version (3.8.18) does not satisfy Python>=4.0 and albatross==1.0.0 depends on Python>=4.0, we can conclude that albatross==1.0.0 cannot be used.
              And because you require albatross==1.0.0, we can conclude that the requirements are unsatisfiable.
        "###);
    });

    assert_not_installed(&venv, "a_d004e577", &temp_dir);

    Ok(())
}

/// requires-python-version-less-than-current
///
/// The user requires a package which requires a Python version less than the
/// current version
///
/// ```text
/// 5e520203
/// ├── environment
/// │   └── python3.9
/// ├── root
/// │   └── requires a==1.0.0
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
///         └── requires python<=3.8 (incompatible with environment)
/// ```
#[test]
fn requires_python_version_less_than_current() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "3.9");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-5e520203", "albatross"));
    filters.push((r"-5e520203", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("a-5e520203==1.0.0")
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
          ╰─▶ Because the current Python version (3.9.18) does not satisfy Python<=3.8 and albatross==1.0.0 depends on Python<=3.8, we can conclude that albatross==1.0.0 cannot be used.
              And because you require albatross==1.0.0, we can conclude that the requirements are unsatisfiable.
        "###);
    });

    assert_not_installed(&venv, "a_5e520203", &temp_dir);

    Ok(())
}

/// requires-python-version-greater-than-current
///
/// The user requires a package which requires a Python version greater than the
/// current version
///
/// ```text
/// 62394505
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
fn requires_python_version_greater_than_current() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "3.9");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-62394505", "albatross"));
    filters.push((r"-62394505", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("a-62394505==1.0.0")
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
          ╰─▶ Because the current Python version (3.9.18) does not satisfy Python>=3.10 and albatross==1.0.0 depends on Python>=3.10, we can conclude that albatross==1.0.0 cannot be used.
              And because you require albatross==1.0.0, we can conclude that the requirements are unsatisfiable.
        "###);
    });

    assert_not_installed(&venv, "a_62394505", &temp_dir);

    Ok(())
}

/// requires-python-version-greater-than-current-patch
///
/// The user requires a package which requires a Python version with a patch version
/// greater than the current patch version
///
/// ```text
/// 7b98f5af
/// ├── environment
/// │   └── python3.8.12
/// ├── root
/// │   └── requires a==1.0.0
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
///         └── requires python>=3.8.14 (incompatible with environment)
/// ```
#[test]
fn requires_python_version_greater_than_current_patch() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "3.8.12");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-7b98f5af", "albatross"));
    filters.push((r"-7b98f5af", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("a-7b98f5af==1.0.0")
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
          ╰─▶ Because the current Python version (3.8.12) does not satisfy Python>=3.8.14 and albatross==1.0.0 depends on Python>=3.8.14, we can conclude that albatross==1.0.0 cannot be used.
              And because you require albatross==1.0.0, we can conclude that the requirements are unsatisfiable.
        "###);
    });

    assert_not_installed(&venv, "a_7b98f5af", &temp_dir);

    Ok(())
}

/// requires-python-version-greater-than-current-many
///
/// The user requires a package which has many versions which all require a Python
/// version greater than the current version
///
/// ```text
/// fd1f719c
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
/// ```
#[test]
fn requires_python_version_greater_than_current_many() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "3.9");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-fd1f719c", "albatross"));
    filters.push((r"-fd1f719c", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("a-fd1f719c==1.0.0")
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
          ╰─▶ Because there is no version of albatross==1.0.0 and you require albatross==1.0.0, we can conclude that the requirements are unsatisfiable.
        "###);
    });

    assert_not_installed(&venv, "a_fd1f719c", &temp_dir);

    Ok(())
}

/// requires-python-version-greater-than-current-backtrack
///
/// The user requires a package where recent versions require a Python version
/// greater than the current version, but an older version is compatible.
///
/// ```text
/// b2677f9a
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
/// ```
#[test]
fn requires_python_version_greater_than_current_backtrack() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "3.9");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-b2677f9a", "albatross"));
    filters.push((r"-b2677f9a", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("a-b2677f9a")
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
         + albatross==1.0.0
        "###);
    });

    assert_installed(&venv, "a_b2677f9a", "1.0.0", &temp_dir);

    Ok(())
}

/// requires-python-version-greater-than-current-excluded
///
/// The user requires a package where recent versions require a Python version
/// greater than the current version, but an excluded older version is compatible.
///
/// ```text
/// 34ee1c3e
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
/// ```
#[test]
fn requires_python_version_greater_than_current_excluded() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "3.9");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-34ee1c3e", "albatross"));
    filters.push((r"-34ee1c3e", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("a-34ee1c3e>=2.0.0")
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
          ╰─▶ Because the current Python version (3.9.18) does not satisfy Python>=3.10,<3.11 and the current Python version (3.9.18) does not satisfy Python>=3.12, we can conclude that any of:
                  Python>=3.10,<3.11
                  Python>=3.12
               are incompatible.
              And because the current Python version (3.9.18) does not satisfy Python>=3.11,<3.12, we can conclude that Python>=3.10 are incompatible.
              And because albatross==2.0.0 depends on Python>=3.10 and there are no versions of albatross that satisfy any of:
                  albatross>2.0.0,<3.0.0
                  albatross>3.0.0,<4.0.0
                  albatross>4.0.0
              we can conclude that albatross>=2.0.0,<3.0.0 cannot be used. (1)

              Because the current Python version (3.9.18) does not satisfy Python>=3.11,<3.12 and the current Python version (3.9.18) does not satisfy Python>=3.12, we can conclude that Python>=3.11 are incompatible.
              And because albatross==3.0.0 depends on Python>=3.11, we can conclude that albatross==3.0.0 cannot be used.
              And because we know from (1) that albatross>=2.0.0,<3.0.0 cannot be used, we can conclude that albatross>=2.0.0,<4.0.0 cannot be used. (2)

              Because the current Python version (3.9.18) does not satisfy Python>=3.12 and albatross==4.0.0 depends on Python>=3.12, we can conclude that albatross==4.0.0 cannot be used.
              And because we know from (2) that albatross>=2.0.0,<4.0.0 cannot be used, we can conclude that albatross>=2.0.0 cannot be used.
              And because you require albatross>=2.0.0, we can conclude that the requirements are unsatisfiable.
        "###);
    });

    assert_not_installed(&venv, "a_34ee1c3e", &temp_dir);

    Ok(())
}

/// specific-tag-and-default
///
/// A wheel for a specific platform is available alongside the default.
///
/// ```text
/// ce63fb65
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
/// ```
#[test]
fn specific_tag_and_default() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-ce63fb65", "albatross"));
    filters.push((r"-ce63fb65", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("a-ce63fb65")
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
         + albatross==1.0.0
        "###);
    });

    Ok(())
}

/// only-wheels
///
/// No source distributions are available, only wheels.
///
/// ```text
/// 08df4319
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
/// ```
#[test]
fn only_wheels() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-08df4319", "albatross"));
    filters.push((r"-08df4319", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("a-08df4319")
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
         + albatross==1.0.0
        "###);
    });

    Ok(())
}

/// no-wheels
///
/// No wheels are available, only source distributions.
///
/// ```text
/// 0a9090ba
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
/// ```
#[test]
fn no_wheels() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-0a9090ba", "albatross"));
    filters.push((r"-0a9090ba", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("a-0a9090ba")
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
         + albatross==1.0.0
        "###);
    });

    Ok(())
}

/// no-wheels-with-matching-platform
///
/// No wheels with valid tags are available, just source distributions.
///
/// ```text
/// b595b358
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
/// ```
#[test]
fn no_wheels_with_matching_platform() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-b595b358", "albatross"));
    filters.push((r"-b595b358", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("a-b595b358")
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
         + albatross==1.0.0
        "###);
    });

    Ok(())
}
