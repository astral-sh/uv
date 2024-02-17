//! DO NOT EDIT
//!
//! Generated with ./scripts/scenarios/update.py
//! Scenarios from <https://github.com/zanieb/packse/tree/64b4451b832cece378f6e773d326ea09efe8903d/scenarios>
//!
#![cfg(all(feature = "python", feature = "pypi"))]

use std::path::Path;
use std::process::Command;

use assert_cmd::assert::Assert;
use assert_cmd::prelude::*;

use common::{venv_to_interpreter, INSTA_FILTERS};

use crate::common::{get_bin, uv_snapshot, TestContext};

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

/// Create a `pip install` command with options shared across all scenarios.
fn command(context: &TestContext) -> Command {
    let mut command = Command::new(get_bin());
    command
        .arg("pip")
        .arg("install")
        .arg("--extra-index-url")
        .arg("https://test.pypi.org/simple")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .env("VIRTUAL_ENV", context.venv.as_os_str())
        .env("UV_NO_WRAP", "1")
        .current_dir(&context.temp_dir);
    command
}

/// requires-package-does-not-exist
///
/// The user requires any version of package `a` which does not exist.
///
/// ```text
/// 5a1a4a35
/// ├── environment
/// │   └── python3.8
/// └── root
///     └── requires a
///         └── unsatisfied: no versions for package
/// ```
#[test]
fn requires_package_does_not_exist() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"-5a1a4a35", ""));

    uv_snapshot!(filters, command(&context)
        .arg("a-5a1a4a35")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because a was not found in the package registry and you require a, we can conclude that the requirements are unsatisfiable.
    "###);

    assert_not_installed(&context.venv, "a_5a1a4a35", &context.temp_dir);
}

/// requires-exact-version-does-not-exist
///
/// The user requires an exact version of package `a` but only other versions exist
///
/// ```text
/// 7cff23d9
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a==2.0.0
/// │       └── unsatisfied: no matching version
/// └── a
///     └── a-1.0.0
/// ```
#[test]
fn requires_exact_version_does_not_exist() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-7cff23d9", "albatross"));
    filters.push((r"-7cff23d9", ""));

    uv_snapshot!(filters, command(&context)
        .arg("a-7cff23d9==2.0.0")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because there is no version of albatross==2.0.0 and you require albatross==2.0.0, we can conclude that the requirements are unsatisfiable.
    "###);

    assert_not_installed(&context.venv, "a_7cff23d9", &context.temp_dir);
}

/// requires-greater-version-does-not-exist
///
/// The user requires a version of `a` greater than `1.0.0` but only smaller or
/// equal versions exist
///
/// ```text
/// 63569c9e
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
fn requires_greater_version_does_not_exist() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-63569c9e", "albatross"));
    filters.push((r"-63569c9e", ""));

    uv_snapshot!(filters, command(&context)
        .arg("a-63569c9e>1.0.0")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only albatross<=1.0.0 is available and you require albatross>1.0.0, we can conclude that the requirements are unsatisfiable.
    "###);

    assert_not_installed(&context.venv, "a_63569c9e", &context.temp_dir);
}

/// requires-less-version-does-not-exist
///
/// The user requires a version of `a` less than `1.0.0` but only larger versions
/// exist
///
/// ```text
/// 2af6fa02
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
fn requires_less_version_does_not_exist() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-2af6fa02", "albatross"));
    filters.push((r"-2af6fa02", ""));

    uv_snapshot!(filters, command(&context)
        .arg("a-2af6fa02<2.0.0")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only albatross>=2.0.0 is available and you require albatross<2.0.0, we can conclude that the requirements are unsatisfiable.
    "###);

    assert_not_installed(&context.venv, "a_2af6fa02", &context.temp_dir);
}

/// transitive-requires-package-does-not-exist
///
/// The user requires package `a` but `a` requires package `b` which does not exist
///
/// ```text
/// 64b04b2b
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
fn transitive_requires_package_does_not_exist() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-64b04b2b", "albatross"));
    filters.push((r"-64b04b2b", ""));

    uv_snapshot!(filters, command(&context)
        .arg("a-64b04b2b")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because b was not found in the package registry and albatross==1.0.0 depends on b, we can conclude that albatross==1.0.0 cannot be used.
          And because only albatross==1.0.0 is available and you require albatross, we can conclude that the requirements are unsatisfiable.
    "###);

    assert_not_installed(&context.venv, "a_64b04b2b", &context.temp_dir);
}

/// excluded-only-version
///
/// Only one version of the requested package is available, but the user has banned
/// that version.
///
/// ```text
/// 72f0d052
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a!=1.0.0
/// │       └── unsatisfied: no matching version
/// └── a
///     └── a-1.0.0
/// ```
#[test]
fn excluded_only_version() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-72f0d052", "albatross"));
    filters.push((r"-72f0d052", ""));

    uv_snapshot!(filters, command(&context)
        .arg("a-72f0d052!=1.0.0")
        , @r###"
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

    // Only `a==1.0.0` is available but the user excluded it.
    assert_not_installed(&context.venv, "a_72f0d052", &context.temp_dir);
}

/// excluded-only-compatible-version
///
/// Only one version of the requested package `a` is compatible, but the user has
/// banned that version.
///
/// ```text
/// d6ce69da
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
fn excluded_only_compatible_version() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-d6ce69da", "albatross"));
    filters.push((r"b-d6ce69da", "bluebird"));
    filters.push((r"-d6ce69da", ""));

    uv_snapshot!(filters, command(&context)
        .arg("a-d6ce69da!=2.0.0")
                .arg("b-d6ce69da<3.0.0,>=2.0.0")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only the following versions of albatross are available:
              albatross==1.0.0
              albatross==2.0.0
              albatross==3.0.0
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

    // Only `a==1.2.0` is available since `a==1.0.0` and `a==3.0.0` require
    // incompatible versions of `b`. The user has excluded that version of `a` so
    // resolution fails.
    assert_not_installed(&context.venv, "a_d6ce69da", &context.temp_dir);
    assert_not_installed(&context.venv, "b_d6ce69da", &context.temp_dir);
}

/// dependency-excludes-range-of-compatible-versions
///
/// There is a range of compatible versions for the requested package `a`, but
/// another dependency `c` excludes that range.
///
/// ```text
/// 5824fb81
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
fn dependency_excludes_range_of_compatible_versions() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-5824fb81", "albatross"));
    filters.push((r"b-5824fb81", "bluebird"));
    filters.push((r"c-5824fb81", "crow"));
    filters.push((r"-5824fb81", ""));

    uv_snapshot!(filters, command(&context)
        .arg("a-5824fb81")
                .arg("b-5824fb81<3.0.0,>=2.0.0")
                .arg("c-5824fb81")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only the following versions of albatross are available:
              albatross==1.0.0
              albatross>=2.0.0,<=3.0.0
          and albatross==1.0.0 depends on bluebird==1.0.0, we can conclude that albatross<2.0.0 depends on bluebird==1.0.0. (1)

          Because only the following versions of crow are available:
              crow==1.0.0
              crow==2.0.0
          and crow==1.0.0 depends on albatross<2.0.0, we can conclude that crow<2.0.0 depends on albatross<2.0.0.
          And because crow==2.0.0 depends on albatross>=3.0.0, we can conclude that all versions of crow depend on one of:
              albatross<2.0.0
              albatross>=3.0.0

          And because we know from (1) that albatross<2.0.0 depends on bluebird==1.0.0, we can conclude that albatross!=3.0.0, all versions of crow, bluebird!=1.0.0 are incompatible.
          And because albatross==3.0.0 depends on bluebird==3.0.0, we can conclude that all versions of crow depend on one of:
              bluebird==1.0.0
              bluebird==3.0.0

          And because you require crow and you require bluebird>=2.0.0,<3.0.0, we can conclude that the requirements are unsatisfiable.
    "###);

    // Only the `2.x` versions of `a` are available since `a==1.0.0` and `a==3.0.0`
    // require incompatible versions of `b`, but all available versions of `c` exclude
    // that range of `a` so resolution fails.
    assert_not_installed(&context.venv, "a_5824fb81", &context.temp_dir);
    assert_not_installed(&context.venv, "b_5824fb81", &context.temp_dir);
    assert_not_installed(&context.venv, "c_5824fb81", &context.temp_dir);
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
/// 119f929b
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
fn dependency_excludes_non_contiguous_range_of_compatible_versions() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-119f929b", "albatross"));
    filters.push((r"b-119f929b", "bluebird"));
    filters.push((r"c-119f929b", "crow"));
    filters.push((r"-119f929b", ""));

    uv_snapshot!(filters, command(&context)
        .arg("a-119f929b")
                .arg("b-119f929b<3.0.0,>=2.0.0")
                .arg("c-119f929b")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only the following versions of crow are available:
              crow==1.0.0
              crow==2.0.0
          and crow==1.0.0 depends on albatross<2.0.0, we can conclude that crow<2.0.0 depends on albatross<2.0.0. (1)

          Because only the following versions of albatross are available:
              albatross==1.0.0
              albatross>=2.0.0
          and albatross==1.0.0 depends on bluebird==1.0.0, we can conclude that albatross<2.0.0 depends on bluebird==1.0.0.
          And because we know from (1) that crow<2.0.0 depends on albatross<2.0.0, we can conclude that crow<2.0.0 depends on bluebird==1.0.0.
          And because crow==2.0.0 depends on albatross>=3.0.0, we can conclude that albatross<3.0.0, all versions of crow, bluebird!=1.0.0 are incompatible. (2)

          Because only albatross<=3.0.0 is available and albatross==3.0.0 depends on bluebird==3.0.0, we can conclude that albatross>=3.0.0 depends on bluebird==3.0.0.
          And because we know from (2) that albatross<3.0.0, all versions of crow, bluebird!=1.0.0 are incompatible, we can conclude that all versions of crow depend on one of:
              bluebird<=1.0.0
              bluebird>=3.0.0

          And because you require bluebird>=2.0.0,<3.0.0 and you require crow, we can conclude that the requirements are unsatisfiable.
    "###);

    // Only the `2.x` versions of `a` are available since `a==1.0.0` and `a==3.0.0`
    // require incompatible versions of `b`, but all available versions of `c` exclude
    // that range of `a` so resolution fails.
    assert_not_installed(&context.venv, "a_119f929b", &context.temp_dir);
    assert_not_installed(&context.venv, "b_119f929b", &context.temp_dir);
    assert_not_installed(&context.venv, "c_119f929b", &context.temp_dir);
}

/// extra-required
///
/// Optional dependencies are requested for the package.
///
/// ```text
/// c1e0ed38
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
fn extra_required() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-c1e0ed38", "albatross"));
    filters.push((r"b-c1e0ed38", "bluebird"));
    filters.push((r"-c1e0ed38", ""));

    uv_snapshot!(filters, command(&context)
        .arg("a-c1e0ed38[extra]")
        , @r###"
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

    assert_installed(&context.venv, "a_c1e0ed38", "1.0.0", &context.temp_dir);
    assert_installed(&context.venv, "b_c1e0ed38", "1.0.0", &context.temp_dir);
}

/// missing-extra
///
/// Optional dependencies are requested for the package, but the extra does not
/// exist.
///
/// ```text
/// de25a6db
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a[extra]
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
/// ```
#[test]
fn missing_extra() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-de25a6db", "albatross"));
    filters.push((r"-de25a6db", ""));

    uv_snapshot!(filters, command(&context)
        .arg("a-de25a6db[extra]")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + albatross==1.0.0
    "###);

    // Missing extras are ignored during resolution.
    assert_installed(&context.venv, "a_de25a6db", "1.0.0", &context.temp_dir);
}

/// multiple-extras-required
///
/// Multiple optional dependencies are requested for the package.
///
/// ```text
/// 502cbb59
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
fn multiple_extras_required() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-502cbb59", "albatross"));
    filters.push((r"b-502cbb59", "bluebird"));
    filters.push((r"c-502cbb59", "crow"));
    filters.push((r"-502cbb59", ""));

    uv_snapshot!(filters, command(&context)
        .arg("a-502cbb59[extra_b,extra_c]")
        , @r###"
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

    assert_installed(&context.venv, "a_502cbb59", "1.0.0", &context.temp_dir);
    assert_installed(&context.venv, "b_502cbb59", "1.0.0", &context.temp_dir);
    assert_installed(&context.venv, "c_502cbb59", "1.0.0", &context.temp_dir);
}

/// all-extras-required
///
/// Multiple optional dependencies are requested for the via an 'all' extra.
///
/// ```text
/// 4cf56e90
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a[all]
/// │       ├── satisfied by a-1.0.0
/// │       ├── satisfied by a-1.0.0[all]
/// │       ├── satisfied by a-1.0.0[extra_b]
/// │       └── satisfied by a-1.0.0[extra_c]
/// ├── a
/// │   ├── a-1.0.0
/// │   ├── a-1.0.0[all]
/// │   │   ├── requires a[extra_b]
/// │   │   │   ├── satisfied by a-1.0.0
/// │   │   │   ├── satisfied by a-1.0.0[all]
/// │   │   │   ├── satisfied by a-1.0.0[extra_b]
/// │   │   │   └── satisfied by a-1.0.0[extra_c]
/// │   │   └── requires a[extra_c]
/// │   │       ├── satisfied by a-1.0.0
/// │   │       ├── satisfied by a-1.0.0[all]
/// │   │       ├── satisfied by a-1.0.0[extra_b]
/// │   │       └── satisfied by a-1.0.0[extra_c]
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
fn all_extras_required() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-4cf56e90", "albatross"));
    filters.push((r"b-4cf56e90", "bluebird"));
    filters.push((r"c-4cf56e90", "crow"));
    filters.push((r"-4cf56e90", ""));

    uv_snapshot!(filters, command(&context)
        .arg("a-4cf56e90[all]")
        , @r###"
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

    assert_installed(&context.venv, "a_4cf56e90", "1.0.0", &context.temp_dir);
    assert_installed(&context.venv, "b_4cf56e90", "1.0.0", &context.temp_dir);
    assert_installed(&context.venv, "c_4cf56e90", "1.0.0", &context.temp_dir);
}

/// extra-incompatible-with-extra
///
/// Multiple optional dependencies are requested for the package, but they have
/// conflicting requirements with each other.
///
/// ```text
/// a5547b80
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
fn extra_incompatible_with_extra() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-a5547b80", "albatross"));
    filters.push((r"b-a5547b80", "bluebird"));
    filters.push((r"-a5547b80", ""));

    uv_snapshot!(filters, command(&context)
        .arg("a-a5547b80[extra_b,extra_c]")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only albatross[extra-b]==1.0.0 is available and albatross[extra-b]==1.0.0 depends on bluebird==1.0.0, we can conclude that all versions of albatross[extra-b] depend on bluebird==1.0.0.
          And because albatross[extra-c]==1.0.0 depends on bluebird==2.0.0 and only albatross[extra-c]==1.0.0 is available, we can conclude that all versions of albatross[extra-c] and all versions of albatross[extra-b] are incompatible.
          And because you require albatross[extra-c] and you require albatross[extra-b], we can conclude that the requirements are unsatisfiable.
    "###);

    // Because both `extra_b` and `extra_c` are requested and they require incompatible
    // versions of `b`, `a` cannot be installed.
    assert_not_installed(&context.venv, "a_a5547b80", &context.temp_dir);
}

/// extra-incompatible-with-extra-not-requested
///
/// One of two incompatible optional dependencies are requested for the package.
///
/// ```text
/// 8bb31c23
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
fn extra_incompatible_with_extra_not_requested() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-8bb31c23", "albatross"));
    filters.push((r"b-8bb31c23", "bluebird"));
    filters.push((r"-8bb31c23", ""));

    uv_snapshot!(filters, command(&context)
        .arg("a-8bb31c23[extra_c]")
        , @r###"
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

    // Because the user does not request both extras, it is okay that one is
    // incompatible with the other.
    assert_installed(&context.venv, "a_8bb31c23", "1.0.0", &context.temp_dir);
    assert_installed(&context.venv, "b_8bb31c23", "2.0.0", &context.temp_dir);
}

/// extra-incompatible-with-root
///
/// Optional dependencies are requested for the package, but the extra is not
/// compatible with other requested versions.
///
/// ```text
/// aca6971b
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
fn extra_incompatible_with_root() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-aca6971b", "albatross"));
    filters.push((r"b-aca6971b", "bluebird"));
    filters.push((r"-aca6971b", ""));

    uv_snapshot!(filters, command(&context)
        .arg("a-aca6971b[extra]")
                .arg("b-aca6971b==2.0.0")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only albatross[extra]==1.0.0 is available and albatross[extra]==1.0.0 depends on bluebird==1.0.0, we can conclude that all versions of albatross[extra] depend on bluebird==1.0.0.
          And because you require albatross[extra] and you require bluebird==2.0.0, we can conclude that the requirements are unsatisfiable.
    "###);

    // Because the user requested `b==2.0.0` but the requested extra requires
    // `b==1.0.0`, the dependencies cannot be satisfied.
    assert_not_installed(&context.venv, "a_aca6971b", &context.temp_dir);
    assert_not_installed(&context.venv, "b_aca6971b", &context.temp_dir);
}

/// extra-does-not-exist-backtrack
///
/// Optional dependencies are requested for the package, the extra is only available
/// on an older version.
///
/// ```text
/// c4307e58
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
fn extra_does_not_exist_backtrack() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-c4307e58", "albatross"));
    filters.push((r"b-c4307e58", "bluebird"));
    filters.push((r"-c4307e58", ""));

    uv_snapshot!(filters, command(&context)
        .arg("a-c4307e58[extra]")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + albatross==3.0.0
    "###);

    // The resolver should not backtrack to `a==1.0.0` because missing extras are
    // allowed during resolution. `b` should not be installed.
    assert_installed(&context.venv, "a_c4307e58", "3.0.0", &context.temp_dir);
}

/// direct-incompatible-versions
///
/// The user requires two incompatible, existing versions of package `a`
///
/// ```text
/// c0e7adfa
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
fn direct_incompatible_versions() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-c0e7adfa", "albatross"));
    filters.push((r"-c0e7adfa", ""));

    uv_snapshot!(filters, command(&context)
        .arg("a-c0e7adfa==1.0.0")
                .arg("a-c0e7adfa==2.0.0")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ your requirements cannot be used because there are conflicting versions for `albatross`: `albatross==1.0.0` does not intersect with `albatross==2.0.0`
    "###);

    assert_not_installed(&context.venv, "a_c0e7adfa", &context.temp_dir);
    assert_not_installed(&context.venv, "a_c0e7adfa", &context.temp_dir);
}

/// transitive-incompatible-with-root-version
///
/// The user requires packages `a` and `b` but `a` requires a different version of
/// `b`
///
/// ```text
/// a13da883
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
fn transitive_incompatible_with_root_version() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-a13da883", "albatross"));
    filters.push((r"b-a13da883", "bluebird"));
    filters.push((r"-a13da883", ""));

    uv_snapshot!(filters, command(&context)
        .arg("a-a13da883")
                .arg("b-a13da883==1.0.0")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only albatross==1.0.0 is available and albatross==1.0.0 depends on bluebird==2.0.0, we can conclude that all versions of albatross depend on bluebird==2.0.0.
          And because you require albatross and you require bluebird==1.0.0, we can conclude that the requirements are unsatisfiable.
    "###);

    assert_not_installed(&context.venv, "a_a13da883", &context.temp_dir);
    assert_not_installed(&context.venv, "b_a13da883", &context.temp_dir);
}

/// transitive-incompatible-with-transitive
///
/// The user requires package `a` and `b`; `a` and `b` require different versions of
/// `c`
///
/// ```text
/// ec82e315
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
fn transitive_incompatible_with_transitive() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-ec82e315", "albatross"));
    filters.push((r"b-ec82e315", "bluebird"));
    filters.push((r"c-ec82e315", "crow"));
    filters.push((r"-ec82e315", ""));

    uv_snapshot!(filters, command(&context)
        .arg("a-ec82e315")
                .arg("b-ec82e315")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only bluebird==1.0.0 is available and bluebird==1.0.0 depends on crow==2.0.0, we can conclude that all versions of bluebird depend on crow==2.0.0.
          And because albatross==1.0.0 depends on crow==1.0.0 and only albatross==1.0.0 is available, we can conclude that all versions of bluebird and all versions of albatross are incompatible.
          And because you require bluebird and you require albatross, we can conclude that the requirements are unsatisfiable.
    "###);

    assert_not_installed(&context.venv, "a_ec82e315", &context.temp_dir);
    assert_not_installed(&context.venv, "b_ec82e315", &context.temp_dir);
}

/// package-only-prereleases
///
/// The user requires any version of package `a` which only has prerelease versions
/// available.
///
/// ```text
/// 472fcc7e
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a
/// │       └── unsatisfied: no matching version
/// └── a
///     └── a-1.0.0a1
/// ```
#[test]
fn package_only_prereleases() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-472fcc7e", "albatross"));
    filters.push((r"-472fcc7e", ""));

    uv_snapshot!(filters, command(&context)
        .arg("a-472fcc7e")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + albatross==1.0.0a1
    "###);

    // Since there are only prerelease versions of `a` available, it should be
    // installed even though the user did not include a prerelease specifier.
    assert_installed(&context.venv, "a_472fcc7e", "1.0.0a1", &context.temp_dir);
}

/// package-only-prereleases-in-range
///
/// The user requires a version of package `a` which only matches prerelease
/// versions but they did not include a prerelease specifier.
///
/// ```text
/// 1017748b
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
fn package_only_prereleases_in_range() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-1017748b", "albatross"));
    filters.push((r"-1017748b", ""));

    uv_snapshot!(filters, command(&context)
        .arg("a-1017748b>0.1.0")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only albatross<=0.1.0 is available and you require albatross>0.1.0, we can conclude that the requirements are unsatisfiable.

          hint: Pre-releases are available for albatross in the requested range (e.g., 1.0.0a1), but pre-releases weren't enabled (try: `--prerelease=allow`)
    "###);

    // Since there are stable versions of `a` available, prerelease versions should not
    // be selected without explicit opt-in.
    assert_not_installed(&context.venv, "a_1017748b", &context.temp_dir);
}

/// requires-package-only-prereleases-in-range-global-opt-in
///
/// The user requires a version of package `a` which only matches prerelease
/// versions. They did not include a prerelease specifier for the package, but they
/// opted into prereleases globally.
///
/// ```text
/// 95140069
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
fn requires_package_only_prereleases_in_range_global_opt_in() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-95140069", "albatross"));
    filters.push((r"-95140069", ""));

    uv_snapshot!(filters, command(&context)
        .arg("--prerelease=allow")
        .arg("a-95140069>0.1.0")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + albatross==1.0.0a1
    "###);

    assert_installed(&context.venv, "a_95140069", "1.0.0a1", &context.temp_dir);
}

/// requires-package-prerelease-and-final-any
///
/// The user requires any version of package `a` has a prerelease version available
/// and an older non-prerelease version.
///
/// ```text
/// 909975d8
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
fn requires_package_prerelease_and_final_any() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-909975d8", "albatross"));
    filters.push((r"-909975d8", ""));

    uv_snapshot!(filters, command(&context)
        .arg("a-909975d8")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + albatross==0.1.0
    "###);

    // Since the user did not provide a prerelease specifier, the older stable version
    // should be selected.
    assert_installed(&context.venv, "a_909975d8", "0.1.0", &context.temp_dir);
}

/// package-prerelease-specified-only-final-available
///
/// The user requires a version of `a` with a prerelease specifier and only stable
/// releases are available.
///
/// ```text
/// 6f8bea9f
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
fn package_prerelease_specified_only_final_available() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-6f8bea9f", "albatross"));
    filters.push((r"-6f8bea9f", ""));

    uv_snapshot!(filters, command(&context)
        .arg("a-6f8bea9f>=0.1.0a1")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + albatross==0.3.0
    "###);

    // The latest stable version should be selected.
    assert_installed(&context.venv, "a_6f8bea9f", "0.3.0", &context.temp_dir);
}

/// package-prerelease-specified-only-prerelease-available
///
/// The user requires a version of `a` with a prerelease specifier and only
/// prerelease releases are available.
///
/// ```text
/// 48d4bba0
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
fn package_prerelease_specified_only_prerelease_available() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-48d4bba0", "albatross"));
    filters.push((r"-48d4bba0", ""));

    uv_snapshot!(filters, command(&context)
        .arg("a-48d4bba0>=0.1.0a1")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + albatross==0.3.0a1
    "###);

    // The latest prerelease version should be selected.
    assert_installed(&context.venv, "a_48d4bba0", "0.3.0a1", &context.temp_dir);
}

/// package-prerelease-specified-mixed-available
///
/// The user requires a version of `a` with a prerelease specifier and both
/// prerelease and stable releases are available.
///
/// ```text
/// 2b1193a7
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
fn package_prerelease_specified_mixed_available() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-2b1193a7", "albatross"));
    filters.push((r"-2b1193a7", ""));

    uv_snapshot!(filters, command(&context)
        .arg("a-2b1193a7>=0.1.0a1")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + albatross==1.0.0a1
    "###);

    // Since the user provided a prerelease specifier, the latest prerelease version
    // should be selected.
    assert_installed(&context.venv, "a_2b1193a7", "1.0.0a1", &context.temp_dir);
}

/// package-multiple-prereleases-kinds
///
/// The user requires `a` which has multiple prereleases available with different
/// labels.
///
/// ```text
/// 72919cf7
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
fn package_multiple_prereleases_kinds() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-72919cf7", "albatross"));
    filters.push((r"-72919cf7", ""));

    uv_snapshot!(filters, command(&context)
        .arg("a-72919cf7>=1.0.0a1")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + albatross==1.0.0rc1
    "###);

    // Release candidates should be the highest precedence prerelease kind.
    assert_installed(&context.venv, "a_72919cf7", "1.0.0rc1", &context.temp_dir);
}

/// package-multiple-prereleases-numbers
///
/// The user requires `a` which has multiple alphas available.
///
/// ```text
/// cecdb92d
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
fn package_multiple_prereleases_numbers() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-cecdb92d", "albatross"));
    filters.push((r"-cecdb92d", ""));

    uv_snapshot!(filters, command(&context)
        .arg("a-cecdb92d>=1.0.0a1")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + albatross==1.0.0a3
    "###);

    // The latest alpha version should be selected.
    assert_installed(&context.venv, "a_cecdb92d", "1.0.0a3", &context.temp_dir);
}

/// transitive-package-only-prereleases
///
/// The user requires any version of package `a` which requires `b` which only has
/// prerelease versions available.
///
/// ```text
/// e3c94488
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
fn transitive_package_only_prereleases() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-e3c94488", "albatross"));
    filters.push((r"b-e3c94488", "bluebird"));
    filters.push((r"-e3c94488", ""));

    uv_snapshot!(filters, command(&context)
        .arg("a-e3c94488")
        , @r###"
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

    // Since there are only prerelease versions of `b` available, it should be selected
    // even though the user did not opt-in to prereleases.
    assert_installed(&context.venv, "a_e3c94488", "0.1.0", &context.temp_dir);
    assert_installed(&context.venv, "b_e3c94488", "1.0.0a1", &context.temp_dir);
}

/// transitive-package-only-prereleases-in-range
///
/// The user requires package `a` which has a dependency on a package which only
/// matches prerelease versions but they did not include a prerelease specifier.
///
/// ```text
/// 20238f1b
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
fn transitive_package_only_prereleases_in_range() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-20238f1b", "albatross"));
    filters.push((r"b-20238f1b", "bluebird"));
    filters.push((r"-20238f1b", ""));

    uv_snapshot!(filters, command(&context)
        .arg("a-20238f1b")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only bluebird<=0.1 is available and albatross==0.1.0 depends on bluebird>0.1, we can conclude that albatross==0.1.0 cannot be used.
          And because only albatross==0.1.0 is available and you require albatross, we can conclude that the requirements are unsatisfiable.

          hint: Pre-releases are available for bluebird in the requested range (e.g., 1.0.0a1), but pre-releases weren't enabled (try: `--prerelease=allow`)
    "###);

    // Since there are stable versions of `b` available, the prerelease version should
    // not be selected without explicit opt-in. The available version is excluded by
    // the range requested by the user.
    assert_not_installed(&context.venv, "a_20238f1b", &context.temp_dir);
}

/// transitive-package-only-prereleases-in-range-opt-in
///
/// The user requires package `a` which has a dependency on a package which only
/// matches prerelease versions; the user has opted into allowing prereleases in `b`
/// explicitly.
///
/// ```text
/// d65d5fdf
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
fn transitive_package_only_prereleases_in_range_opt_in() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-d65d5fdf", "albatross"));
    filters.push((r"b-d65d5fdf", "bluebird"));
    filters.push((r"-d65d5fdf", ""));

    uv_snapshot!(filters, command(&context)
        .arg("a-d65d5fdf")
                .arg("b-d65d5fdf>0.0.0a1")
        , @r###"
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

    // Since the user included a dependency on `b` with a prerelease specifier, a
    // prerelease version can be selected.
    assert_installed(&context.venv, "a_d65d5fdf", "0.1.0", &context.temp_dir);
    assert_installed(&context.venv, "b_d65d5fdf", "1.0.0a1", &context.temp_dir);
}

/// transitive-prerelease-and-stable-dependency
///
/// A transitive dependency has both a prerelease and a stable selector, but can
/// only be satisfied by a prerelease
///
/// ```text
/// d62255d0
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
fn transitive_prerelease_and_stable_dependency() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-d62255d0", "albatross"));
    filters.push((r"b-d62255d0", "bluebird"));
    filters.push((r"c-d62255d0", "crow"));
    filters.push((r"-d62255d0", ""));

    uv_snapshot!(filters, command(&context)
        .arg("a-d62255d0")
                .arg("b-d62255d0")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because there is no version of crow==2.0.0b1 and albatross==1.0.0 depends on crow==2.0.0b1, we can conclude that albatross==1.0.0 cannot be used.
          And because only albatross==1.0.0 is available and you require albatross, we can conclude that the requirements are unsatisfiable.

          hint: crow was requested with a pre-release marker (e.g., crow==2.0.0b1), but pre-releases weren't enabled (try: `--prerelease=allow`)
    "###);

    // Since the user did not explicitly opt-in to a prerelease, it cannot be selected.
    assert_not_installed(&context.venv, "a_d62255d0", &context.temp_dir);
    assert_not_installed(&context.venv, "b_d62255d0", &context.temp_dir);
}

/// transitive-prerelease-and-stable-dependency-opt-in
///
/// A transitive dependency has both a prerelease and a stable selector, but can
/// only be satisfied by a prerelease. The user includes an opt-in to prereleases of
/// the transitive dependency.
///
/// ```text
/// 0778b0eb
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
fn transitive_prerelease_and_stable_dependency_opt_in() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-0778b0eb", "albatross"));
    filters.push((r"b-0778b0eb", "bluebird"));
    filters.push((r"c-0778b0eb", "crow"));
    filters.push((r"-0778b0eb", ""));

    uv_snapshot!(filters, command(&context)
        .arg("a-0778b0eb")
                .arg("b-0778b0eb")
                .arg("c-0778b0eb>=0.0.0a1")
        , @r###"
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

    // Since the user explicitly opted-in to a prerelease for `c`, it can be installed.
    assert_installed(&context.venv, "a_0778b0eb", "1.0.0", &context.temp_dir);
    assert_installed(&context.venv, "b_0778b0eb", "1.0.0", &context.temp_dir);
    assert_installed(&context.venv, "c_0778b0eb", "2.0.0b1", &context.temp_dir);
}

/// transitive-prerelease-and-stable-dependency-many-versions
///
/// A transitive dependency has both a prerelease and a stable selector, but can
/// only be satisfied by a prerelease. There are many prerelease versions.
///
/// ```text
/// cc6a6eac
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
fn transitive_prerelease_and_stable_dependency_many_versions() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-cc6a6eac", "albatross"));
    filters.push((r"b-cc6a6eac", "bluebird"));
    filters.push((r"c-cc6a6eac", "crow"));
    filters.push((r"-cc6a6eac", ""));

    uv_snapshot!(filters, command(&context)
        .arg("a-cc6a6eac")
                .arg("b-cc6a6eac")
        , @r###"
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

    // Since the user did not explicitly opt-in to a prerelease, it cannot be selected.
    assert_not_installed(&context.venv, "a_cc6a6eac", &context.temp_dir);
    assert_not_installed(&context.venv, "b_cc6a6eac", &context.temp_dir);
}

/// transitive-prerelease-and-stable-dependency-many-versions-holes
///
/// A transitive dependency has both a prerelease and a stable selector, but can
/// only be satisfied by a prerelease. There are many prerelease versions and some
/// are excluded.
///
/// ```text
/// 041e36bc
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
fn transitive_prerelease_and_stable_dependency_many_versions_holes() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-041e36bc", "albatross"));
    filters.push((r"b-041e36bc", "bluebird"));
    filters.push((r"c-041e36bc", "crow"));
    filters.push((r"-041e36bc", ""));

    uv_snapshot!(filters, command(&context)
        .arg("a-041e36bc")
                .arg("b-041e36bc")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only the following versions of crow are available:
              crow<=1.0.0
              crow>=2.0.0a5,<=2.0.0a7
              crow==2.0.0b1
              crow>=2.0.0b5
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

    // Since the user did not explicitly opt-in to a prerelease, it cannot be selected.
    assert_not_installed(&context.venv, "a_041e36bc", &context.temp_dir);
    assert_not_installed(&context.venv, "b_041e36bc", &context.temp_dir);
}

/// requires-python-version-does-not-exist
///
/// The user requires a package which requires a Python version that does not exist
///
/// ```text
/// 4486c0e5
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
fn requires_python_version_does_not_exist() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-4486c0e5", "albatross"));
    filters.push((r"-4486c0e5", ""));

    uv_snapshot!(filters, command(&context)
        .arg("a-4486c0e5==1.0.0")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because the current Python version (3.8.18) does not satisfy Python>=4.0 and albatross==1.0.0 depends on Python>=4.0, we can conclude that albatross==1.0.0 cannot be used.
          And because you require albatross==1.0.0, we can conclude that the requirements are unsatisfiable.
    "###);

    assert_not_installed(&context.venv, "a_4486c0e5", &context.temp_dir);
}

/// requires-python-version-less-than-current
///
/// The user requires a package which requires a Python version less than the
/// current version
///
/// ```text
/// d4ea58de
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
fn requires_python_version_less_than_current() {
    let context = TestContext::new("3.9");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-d4ea58de", "albatross"));
    filters.push((r"-d4ea58de", ""));

    uv_snapshot!(filters, command(&context)
        .arg("a-d4ea58de==1.0.0")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because the current Python version (3.9.18) does not satisfy Python<=3.8 and albatross==1.0.0 depends on Python<=3.8, we can conclude that albatross==1.0.0 cannot be used.
          And because you require albatross==1.0.0, we can conclude that the requirements are unsatisfiable.
    "###);

    assert_not_installed(&context.venv, "a_d4ea58de", &context.temp_dir);
}

/// requires-python-version-greater-than-current
///
/// The user requires a package which requires a Python version greater than the
/// current version
///
/// ```text
/// 741c8854
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
fn requires_python_version_greater_than_current() {
    let context = TestContext::new("3.9");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-741c8854", "albatross"));
    filters.push((r"-741c8854", ""));

    uv_snapshot!(filters, command(&context)
        .arg("a-741c8854==1.0.0")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because the current Python version (3.9.18) does not satisfy Python>=3.10 and albatross==1.0.0 depends on Python>=3.10, we can conclude that albatross==1.0.0 cannot be used.
          And because you require albatross==1.0.0, we can conclude that the requirements are unsatisfiable.
    "###);

    assert_not_installed(&context.venv, "a_741c8854", &context.temp_dir);
}

/// requires-python-version-greater-than-current-patch
///
/// The user requires a package which requires a Python version with a patch version
/// greater than the current patch version
///
/// ```text
/// 0044ac94
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
fn requires_python_version_greater_than_current_patch() {
    let context = TestContext::new("3.8.12");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-0044ac94", "albatross"));
    filters.push((r"-0044ac94", ""));

    uv_snapshot!(filters, command(&context)
        .arg("a-0044ac94==1.0.0")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because the current Python version (3.8.12) does not satisfy Python>=3.8.14 and albatross==1.0.0 depends on Python>=3.8.14, we can conclude that albatross==1.0.0 cannot be used.
          And because you require albatross==1.0.0, we can conclude that the requirements are unsatisfiable.
    "###);

    assert_not_installed(&context.venv, "a_0044ac94", &context.temp_dir);
}

/// requires-python-version-greater-than-current-many
///
/// The user requires a package which has many versions which all require a Python
/// version greater than the current version
///
/// ```text
/// da5bd150
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
fn requires_python_version_greater_than_current_many() {
    let context = TestContext::new("3.9");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-da5bd150", "albatross"));
    filters.push((r"-da5bd150", ""));

    uv_snapshot!(filters, command(&context)
        .arg("a-da5bd150==1.0.0")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because there is no version of albatross==1.0.0 and you require albatross==1.0.0, we can conclude that the requirements are unsatisfiable.
    "###);

    assert_not_installed(&context.venv, "a_da5bd150", &context.temp_dir);
}

/// requires-python-version-greater-than-current-backtrack
///
/// The user requires a package where recent versions require a Python version
/// greater than the current version, but an older version is compatible.
///
/// ```text
/// 3204bc0a
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
fn requires_python_version_greater_than_current_backtrack() {
    let context = TestContext::new("3.9");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-3204bc0a", "albatross"));
    filters.push((r"-3204bc0a", ""));

    uv_snapshot!(filters, command(&context)
        .arg("a-3204bc0a")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + albatross==1.0.0
    "###);

    assert_installed(&context.venv, "a_3204bc0a", "1.0.0", &context.temp_dir);
}

/// requires-python-version-greater-than-current-excluded
///
/// The user requires a package where recent versions require a Python version
/// greater than the current version, but an excluded older version is compatible.
///
/// ```text
/// 874cae6d
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
fn requires_python_version_greater_than_current_excluded() {
    let context = TestContext::new("3.9");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-874cae6d", "albatross"));
    filters.push((r"-874cae6d", ""));

    uv_snapshot!(filters, command(&context)
        .arg("a-874cae6d>=2.0.0")
        , @r###"
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
          And because albatross==2.0.0 depends on Python>=3.10 and only the following versions of albatross are available:
              albatross<=2.0.0
              albatross==3.0.0
              albatross==4.0.0
          we can conclude that albatross>=2.0.0,<3.0.0 cannot be used. (1)

          Because the current Python version (3.9.18) does not satisfy Python>=3.11,<3.12 and the current Python version (3.9.18) does not satisfy Python>=3.12, we can conclude that Python>=3.11 are incompatible.
          And because albatross==3.0.0 depends on Python>=3.11, we can conclude that albatross==3.0.0 cannot be used.
          And because we know from (1) that albatross>=2.0.0,<3.0.0 cannot be used, we can conclude that albatross>=2.0.0,<4.0.0 cannot be used. (2)

          Because the current Python version (3.9.18) does not satisfy Python>=3.12 and albatross==4.0.0 depends on Python>=3.12, we can conclude that albatross==4.0.0 cannot be used.
          And because we know from (2) that albatross>=2.0.0,<4.0.0 cannot be used, we can conclude that albatross>=2.0.0 cannot be used.
          And because you require albatross>=2.0.0, we can conclude that the requirements are unsatisfiable.
    "###);

    assert_not_installed(&context.venv, "a_874cae6d", &context.temp_dir);
}

/// specific-tag-and-default
///
/// A wheel for a specific platform is available alongside the default.
///
/// ```text
/// 8f7a81f1
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
/// ```
#[test]
fn specific_tag_and_default() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-8f7a81f1", "albatross"));
    filters.push((r"-8f7a81f1", ""));

    uv_snapshot!(filters, command(&context)
        .arg("a-8f7a81f1")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + albatross==1.0.0
    "###);
}

/// only-wheels
///
/// No source distributions are available, only wheels.
///
/// ```text
/// a874f41e
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
/// ```
#[test]
fn only_wheels() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-a874f41e", "albatross"));
    filters.push((r"-a874f41e", ""));

    uv_snapshot!(filters, command(&context)
        .arg("a-a874f41e")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + albatross==1.0.0
    "###);
}

/// no-wheels
///
/// No wheels are available, only source distributions.
///
/// ```text
/// 0278f343
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
/// ```
#[test]
fn no_wheels() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-0278f343", "albatross"));
    filters.push((r"-0278f343", ""));

    uv_snapshot!(filters, command(&context)
        .arg("a-0278f343")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + albatross==1.0.0
    "###);
}

/// no-wheels-with-matching-platform
///
/// No wheels with matching platform tags are available, just source distributions.
///
/// ```text
/// f1a1f15c
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
/// ```
#[test]
fn no_wheels_with_matching_platform() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-f1a1f15c", "albatross"));
    filters.push((r"-f1a1f15c", ""));

    uv_snapshot!(filters, command(&context)
        .arg("a-f1a1f15c")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + albatross==1.0.0
    "###);
}

/// no-sdist-no-wheels-with-matching-platform
///
/// No wheels with matching platform tags are available, nor are any source
/// distributions available
///
/// ```text
/// 94e293e5
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
/// ```
#[test]
fn no_sdist_no_wheels_with_matching_platform() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-94e293e5", "albatross"));
    filters.push((r"-94e293e5", ""));

    uv_snapshot!(filters, command(&context)
        .arg("a-94e293e5")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only albatross==1.0.0 is available and albatross==1.0.0 is unusable because no wheels are available with a matching platform, we can conclude that all versions of albatross cannot be used.
          And because you require albatross, we can conclude that the requirements are unsatisfiable.
    "###);

    assert_not_installed(&context.venv, "a_94e293e5", &context.temp_dir);
}

/// no-sdist-no-wheels-with-matching-python
///
/// No wheels with matching Python tags are available, nor are any source
/// distributions available
///
/// ```text
/// 40fe677d
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
/// ```
#[test]
fn no_sdist_no_wheels_with_matching_python() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-40fe677d", "albatross"));
    filters.push((r"-40fe677d", ""));

    uv_snapshot!(filters, command(&context)
        .arg("a-40fe677d")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only albatross==1.0.0 is available and albatross==1.0.0 is unusable because no wheels are available with a matching Python implementation, we can conclude that all versions of albatross cannot be used.
          And because you require albatross, we can conclude that the requirements are unsatisfiable.
    "###);

    assert_not_installed(&context.venv, "a_40fe677d", &context.temp_dir);
}

/// no-sdist-no-wheels-with-matching-abi
///
/// No wheels with matching ABI tags are available, nor are any source distributions
/// available
///
/// ```text
/// 8727a9b9
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
/// ```
#[test]
fn no_sdist_no_wheels_with_matching_abi() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-8727a9b9", "albatross"));
    filters.push((r"-8727a9b9", ""));

    uv_snapshot!(filters, command(&context)
        .arg("a-8727a9b9")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only albatross==1.0.0 is available and albatross==1.0.0 is unusable because no wheels are available with a matching Python ABI, we can conclude that all versions of albatross cannot be used.
          And because you require albatross, we can conclude that the requirements are unsatisfiable.
    "###);

    assert_not_installed(&context.venv, "a_8727a9b9", &context.temp_dir);
}

/// no-wheels-no-build
///
/// No wheels are available, only source distributions but the user has disabled
/// builds.
///
/// ```text
/// 662cbd94
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
/// ```
#[test]
fn no_wheels_no_build() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-662cbd94", "albatross"));
    filters.push((r"-662cbd94", ""));

    uv_snapshot!(filters, command(&context)
        .arg("--only-binary")
        .arg("a-662cbd94")
        .arg("a-662cbd94")
        , @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to download and build: albatross==1.0.0
      Caused by: Building source distributions is disabled
    "###);

    assert_not_installed(&context.venv, "a_662cbd94", &context.temp_dir);
}

/// only-wheels-no-binary
///
/// No source distributions are available, only wheels but the user has disabled
/// using pre-built binaries.
///
/// ```text
/// dd137625
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
/// ```
#[test]
fn only_wheels_no_binary() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-dd137625", "albatross"));
    filters.push((r"-dd137625", ""));

    uv_snapshot!(filters, command(&context)
        .arg("--no-binary")
        .arg("a-dd137625")
        .arg("a-dd137625")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only albatross==1.0.0 is available and albatross==1.0.0 is unusable because no source distribution is available and using wheels is disabled, we can conclude that all versions of albatross cannot be used.
          And because you require albatross, we can conclude that the requirements are unsatisfiable.
    "###);

    assert_not_installed(&context.venv, "a_dd137625", &context.temp_dir);
}

/// no-build
///
/// Both wheels and source distributions are available, and the user has disabled
/// builds.
///
/// ```text
/// 9ff1e173
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
/// ```
#[test]
fn no_build() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-9ff1e173", "albatross"));
    filters.push((r"-9ff1e173", ""));

    uv_snapshot!(filters, command(&context)
        .arg("--only-binary")
        .arg("a-9ff1e173")
        .arg("a-9ff1e173")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + albatross==1.0.0
    "###);

    // The wheel should be used for install
}

/// no-binary
///
/// Both wheels and source distributions are available, and the user has disabled
/// binaries.
///
/// ```text
/// 10e961b8
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
/// ```
#[test]
fn no_binary() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-10e961b8", "albatross"));
    filters.push((r"-10e961b8", ""));

    uv_snapshot!(filters, command(&context)
        .arg("--no-binary")
        .arg("a-10e961b8")
        .arg("a-10e961b8")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + albatross==1.0.0
    "###);

    // The source distribution should be used for install
}

/// package-only-yanked
///
/// The user requires any version of package `a` which only has yanked versions
/// available.
///
/// ```text
/// e3de7eb4
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a
/// │       └── unsatisfied: no matching version
/// └── a
///     └── a-1.0.0 (yanked)
/// ```
#[test]
fn package_only_yanked() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-e3de7eb4", "albatross"));
    filters.push((r"-e3de7eb4", ""));

    uv_snapshot!(filters, command(&context)
        .arg("a-e3de7eb4")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only albatross==1.0.0 is available and albatross==1.0.0 is unusable because it was yanked, we can conclude that all versions of albatross cannot be used.
          And because you require albatross, we can conclude that the requirements are unsatisfiable.
    "###);

    // Yanked versions should not be installed, even if they are the only one
    // available.
    assert_not_installed(&context.venv, "a_e3de7eb4", &context.temp_dir);
}

/// package-only-yanked-in-range
///
/// The user requires a version of package `a` which only matches yanked versions.
///
/// ```text
/// 84b3720e
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a>0.1.0
/// │       └── unsatisfied: no matching version
/// └── a
///     ├── a-0.1.0
///     └── a-1.0.0 (yanked)
/// ```
#[test]
fn package_only_yanked_in_range() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-84b3720e", "albatross"));
    filters.push((r"-84b3720e", ""));

    uv_snapshot!(filters, command(&context)
        .arg("a-84b3720e>0.1.0")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only the following versions of albatross are available:
              albatross<=0.1.0
              albatross==1.0.0
          and albatross==1.0.0 is unusable because it was yanked, we can conclude that albatross>0.1.0 cannot be used.
          And because you require albatross>0.1.0, we can conclude that the requirements are unsatisfiable.
    "###);

    // Since there are other versions of `a` available, yanked versions should not be
    // selected without explicit opt-in.
    assert_not_installed(&context.venv, "a_84b3720e", &context.temp_dir);
}

/// requires-package-yanked-and-unyanked-any
///
/// The user requires any version of package `a` has a yanked version available and
/// an older unyanked version.
///
/// ```text
/// 93eac6d7
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a
/// │       └── satisfied by a-0.1.0
/// └── a
///     ├── a-0.1.0
///     └── a-1.0.0 (yanked)
/// ```
#[test]
fn requires_package_yanked_and_unyanked_any() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-93eac6d7", "albatross"));
    filters.push((r"-93eac6d7", ""));

    uv_snapshot!(filters, command(&context)
        .arg("a-93eac6d7")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + albatross==0.1.0
    "###);

    // The unyanked version should be selected.
    assert_installed(&context.venv, "a_93eac6d7", "0.1.0", &context.temp_dir);
}

/// package-yanked-specified-mixed-available
///
/// The user requires any version of `a` and both yanked and unyanked releases are
/// available.
///
/// ```text
/// 3325916e
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a>=0.1.0
/// │       ├── satisfied by a-0.1.0
/// │       └── satisfied by a-0.3.0
/// └── a
///     ├── a-0.1.0
///     ├── a-0.2.0 (yanked)
///     ├── a-0.3.0
///     └── a-1.0.0 (yanked)
/// ```
#[test]
fn package_yanked_specified_mixed_available() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-3325916e", "albatross"));
    filters.push((r"-3325916e", ""));

    uv_snapshot!(filters, command(&context)
        .arg("a-3325916e>=0.1.0")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + albatross==0.3.0
    "###);

    // The latest unyanked version should be selected.
    assert_installed(&context.venv, "a_3325916e", "0.3.0", &context.temp_dir);
}

/// transitive-package-only-yanked
///
/// The user requires any version of package `a` which requires `b` which only has
/// yanked versions available.
///
/// ```text
/// 9ec30fe2
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
///     └── b-1.0.0 (yanked)
/// ```
#[test]
fn transitive_package_only_yanked() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-9ec30fe2", "albatross"));
    filters.push((r"b-9ec30fe2", "bluebird"));
    filters.push((r"-9ec30fe2", ""));

    uv_snapshot!(filters, command(&context)
        .arg("a-9ec30fe2")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only bluebird==1.0.0 is available and bluebird==1.0.0 is unusable because it was yanked, we can conclude that all versions of bluebird cannot be used.
          And because albatross==0.1.0 depends on bluebird, we can conclude that albatross==0.1.0 cannot be used.
          And because only albatross==0.1.0 is available and you require albatross, we can conclude that the requirements are unsatisfiable.
    "###);

    // Yanked versions should not be installed, even if they are the only one
    // available.
    assert_not_installed(&context.venv, "a_9ec30fe2", &context.temp_dir);
}

/// transitive-package-only-yanked-in-range
///
/// The user requires package `a` which has a dependency on a package which only
/// matches yanked versions.
///
/// ```text
/// 872d714e
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
///     └── b-1.0.0 (yanked)
/// ```
#[test]
fn transitive_package_only_yanked_in_range() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-872d714e", "albatross"));
    filters.push((r"b-872d714e", "bluebird"));
    filters.push((r"-872d714e", ""));

    uv_snapshot!(filters, command(&context)
        .arg("a-872d714e")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only the following versions of bluebird are available:
              bluebird<=0.1
              bluebird==1.0.0
          and bluebird==1.0.0 is unusable because it was yanked, we can conclude that bluebird>0.1 cannot be used.
          And because albatross==0.1.0 depends on bluebird>0.1, we can conclude that albatross==0.1.0 cannot be used.
          And because only albatross==0.1.0 is available and you require albatross, we can conclude that the requirements are unsatisfiable.
    "###);

    // Yanked versions should not be installed, even if they are the only valid version
    // in a range.
    assert_not_installed(&context.venv, "a_872d714e", &context.temp_dir);
}

/// transitive-package-only-yanked-in-range-opt-in
///
/// The user requires package `a` which has a dependency on a package which only
/// matches yanked versions; the user has opted into allowing the yanked version of
/// `b` explicitly.
///
/// ```text
/// 1bbd5d1b
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   ├── requires a
/// │   │   └── satisfied by a-0.1.0
/// │   └── requires b==1.0.0
/// │       └── unsatisfied: no matching version
/// ├── a
/// │   └── a-0.1.0
/// │       └── requires b>0.1
/// │           └── unsatisfied: no matching version
/// └── b
///     ├── b-0.1.0
///     └── b-1.0.0 (yanked)
/// ```
#[test]
fn transitive_package_only_yanked_in_range_opt_in() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-1bbd5d1b", "albatross"));
    filters.push((r"b-1bbd5d1b", "bluebird"));
    filters.push((r"-1bbd5d1b", ""));

    uv_snapshot!(filters, command(&context)
        .arg("a-1bbd5d1b")
                .arg("b-1bbd5d1b==1.0.0")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    warning: bluebird==1.0.0 is yanked.
    Downloaded 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + albatross==0.1.0
     + bluebird==1.0.0
    "###);

    // Since the user included a dependency on `b` with an exact specifier, the yanked
    // version can be selected.
    assert_installed(&context.venv, "a_1bbd5d1b", "0.1.0", &context.temp_dir);
    assert_installed(&context.venv, "b_1bbd5d1b", "1.0.0", &context.temp_dir);
}

/// transitive-yanked-and-unyanked-dependency
///
/// A transitive dependency has both a yanked and an unyanked version, but can only
/// be satisfied by a yanked version
///
/// ```text
/// eb1ba5f5
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   ├── requires a
/// │   │   └── satisfied by a-1.0.0
/// │   └── requires b
/// │       └── satisfied by b-1.0.0
/// ├── a
/// │   └── a-1.0.0
/// │       └── requires c==2.0.0
/// │           └── unsatisfied: no matching version
/// ├── b
/// │   └── b-1.0.0
/// │       └── requires c<=3.0.0,>=1.0.0
/// │           └── satisfied by c-1.0.0
/// └── c
///     ├── c-1.0.0
///     └── c-2.0.0 (yanked)
/// ```
#[test]
fn transitive_yanked_and_unyanked_dependency() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-eb1ba5f5", "albatross"));
    filters.push((r"b-eb1ba5f5", "bluebird"));
    filters.push((r"c-eb1ba5f5", "crow"));
    filters.push((r"-eb1ba5f5", ""));

    uv_snapshot!(filters, command(&context)
        .arg("a-eb1ba5f5")
                .arg("b-eb1ba5f5")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because crow==2.0.0 is unusable because it was yanked and albatross==1.0.0 depends on crow==2.0.0, we can conclude that albatross==1.0.0 cannot be used.
          And because only albatross==1.0.0 is available and you require albatross, we can conclude that the requirements are unsatisfiable.
    "###);

    // Since the user did not explicitly select the yanked version, it cannot be used.
    assert_not_installed(&context.venv, "a_eb1ba5f5", &context.temp_dir);
    assert_not_installed(&context.venv, "b_eb1ba5f5", &context.temp_dir);
}

/// transitive-yanked-and-unyanked-dependency-opt-in
///
/// A transitive dependency has both a yanked and an unyanked version, but can only
/// be satisfied by a yanked. The user includes an opt-in to the yanked version of
/// the transitive dependency.
///
/// ```text
/// f0760ee9
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   ├── requires a
/// │   │   └── satisfied by a-1.0.0
/// │   ├── requires b
/// │   │   └── satisfied by b-1.0.0
/// │   └── requires c==2.0.0
/// │       └── unsatisfied: no matching version
/// ├── a
/// │   └── a-1.0.0
/// │       └── requires c==2.0.0
/// │           └── unsatisfied: no matching version
/// ├── b
/// │   └── b-1.0.0
/// │       └── requires c<=3.0.0,>=1.0.0
/// │           └── satisfied by c-1.0.0
/// └── c
///     ├── c-1.0.0
///     └── c-2.0.0 (yanked)
/// ```
#[test]
fn transitive_yanked_and_unyanked_dependency_opt_in() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-f0760ee9", "albatross"));
    filters.push((r"b-f0760ee9", "bluebird"));
    filters.push((r"c-f0760ee9", "crow"));
    filters.push((r"-f0760ee9", ""));

    uv_snapshot!(filters, command(&context)
        .arg("a-f0760ee9")
                .arg("b-f0760ee9")
                .arg("c-f0760ee9==2.0.0")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    warning: crow==2.0.0 is yanked.
    Downloaded 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + albatross==1.0.0
     + bluebird==1.0.0
     + crow==2.0.0
    "###);

    // Since the user explicitly selected the yanked version of `c`, it can be
    // installed.
    assert_installed(&context.venv, "a_f0760ee9", "1.0.0", &context.temp_dir);
    assert_installed(&context.venv, "b_f0760ee9", "1.0.0", &context.temp_dir);
    assert_installed(&context.venv, "c_f0760ee9", "2.0.0", &context.temp_dir);
}
