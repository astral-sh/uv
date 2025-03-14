//! DO NOT EDIT
//!
//! Generated with `./scripts/sync_scenarios.sh`
//! Scenarios from <https://github.com/astral-sh/packse/tree/HEAD/scenarios>
//!
#![cfg(all(feature = "python", feature = "pypi", unix))]

use std::path::Path;
use std::process::Command;

use assert_cmd::assert::Assert;
use assert_cmd::prelude::*;

use uv_static::EnvVars;

use crate::common::{
    build_vendor_links_url, get_bin, packse_index_url, uv_snapshot, venv_to_interpreter,
    TestContext,
};

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
        .arg("--index-url")
        .arg(packse_index_url())
        .arg("--find-links")
        .arg(build_vendor_links_url());
    context.add_shared_options(&mut command, true);
    command.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    command
}

/// The user requires an exact version of package `a` but only other versions exist
///
/// ```text
/// requires-exact-version-does-not-exist
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

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"requires-exact-version-does-not-exist-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("requires-exact-version-does-not-exist-a==2.0.0")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because there is no version of package-a==2.0.0 and you require package-a==2.0.0, we can conclude that your requirements are unsatisfiable.
    "###);

    assert_not_installed(
        &context.venv,
        "requires_exact_version_does_not_exist_a",
        &context.temp_dir,
    );
}

/// The user requires a version of `a` greater than `1.0.0` but only smaller or equal versions exist
///
/// ```text
/// requires-greater-version-does-not-exist
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

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"requires-greater-version-does-not-exist-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("requires-greater-version-does-not-exist-a>1.0.0")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only package-a<=1.0.0 is available and you require package-a>1.0.0, we can conclude that your requirements are unsatisfiable.
    "###);

    assert_not_installed(
        &context.venv,
        "requires_greater_version_does_not_exist_a",
        &context.temp_dir,
    );
}

/// The user requires a version of `a` less than `1.0.0` but only larger versions exist
///
/// ```text
/// requires-less-version-does-not-exist
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

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"requires-less-version-does-not-exist-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("requires-less-version-does-not-exist-a<2.0.0")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only package-a>=2.0.0 is available and you require package-a<2.0.0, we can conclude that your requirements are unsatisfiable.
    "###);

    assert_not_installed(
        &context.venv,
        "requires_less_version_does_not_exist_a",
        &context.temp_dir,
    );
}

/// The user requires any version of package `a` which does not exist.
///
/// ```text
/// requires-package-does-not-exist
/// ├── environment
/// │   └── python3.8
/// └── root
///     └── requires a
///         └── unsatisfied: no versions for package
/// ```
#[test]
fn requires_package_does_not_exist() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"requires-package-does-not-exist-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("requires-package-does-not-exist-a")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because package-a was not found in the package registry and you require package-a, we can conclude that your requirements are unsatisfiable.
    "###);

    assert_not_installed(
        &context.venv,
        "requires_package_does_not_exist_a",
        &context.temp_dir,
    );
}

/// The user requires package `a` but `a` requires package `b` which does not exist
///
/// ```text
/// transitive-requires-package-does-not-exist
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

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"transitive-requires-package-does-not-exist-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("transitive-requires-package-does-not-exist-a")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because package-b was not found in the package registry and package-a==1.0.0 depends on package-b, we can conclude that package-a==1.0.0 cannot be used.
          And because only package-a==1.0.0 is available and you require package-a, we can conclude that your requirements are unsatisfiable.
    "###);

    assert_not_installed(
        &context.venv,
        "transitive_requires_package_does_not_exist_a",
        &context.temp_dir,
    );
}

/// There is a non-contiguous range of compatible versions for the requested package `a`, but another dependency `c` excludes the range. This is the same as `dependency-excludes-range-of-compatible-versions` but some of the versions of `a` are incompatible for another reason e.g. dependency on non-existent package `d`.
///
/// ```text
/// dependency-excludes-non-contiguous-range-of-compatible-versions
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

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((
        r"dependency-excludes-non-contiguous-range-of-compatible-versions-",
        "package-",
    ));

    uv_snapshot!(filters, command(&context)
        .arg("dependency-excludes-non-contiguous-range-of-compatible-versions-a")
                .arg("dependency-excludes-non-contiguous-range-of-compatible-versions-b<3.0.0,>=2.0.0")
                .arg("dependency-excludes-non-contiguous-range-of-compatible-versions-c")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because package-a==1.0.0 depends on package-b==1.0.0 and only the following versions of package-a are available:
              package-a==1.0.0
              package-a>2.0.0
          we can conclude that package-a<2.0.0 depends on package-b==1.0.0.
          And because only package-a<=3.0.0 is available, we can conclude that package-a<2.0.0 depends on package-b==1.0.0. (1)

          Because only the following versions of package-c are available:
              package-c==1.0.0
              package-c==2.0.0
          and package-c==1.0.0 depends on package-a<2.0.0, we can conclude that package-c<2.0.0 depends on package-a<2.0.0.
          And because package-c==2.0.0 depends on package-a>=3.0.0, we can conclude that all versions of package-c depend on one of:
              package-a<2.0.0
              package-a>=3.0.0

          And because we know from (1) that package-a<2.0.0 depends on package-b==1.0.0, we can conclude that package-a!=3.0.0, package-b!=1.0.0, all versions of package-c are incompatible.
          And because package-a==3.0.0 depends on package-b==3.0.0, we can conclude that all versions of package-c depend on one of:
              package-b<=1.0.0
              package-b>=3.0.0

          And because you require package-b>=2.0.0,<3.0.0 and package-c, we can conclude that your requirements are unsatisfiable.
    "###);

    // Only the `2.x` versions of `a` are available since `a==1.0.0` and `a==3.0.0` require incompatible versions of `b`, but all available versions of `c` exclude that range of `a` so resolution fails.
    assert_not_installed(
        &context.venv,
        "dependency_excludes_non_contiguous_range_of_compatible_versions_a",
        &context.temp_dir,
    );
    assert_not_installed(
        &context.venv,
        "dependency_excludes_non_contiguous_range_of_compatible_versions_b",
        &context.temp_dir,
    );
    assert_not_installed(
        &context.venv,
        "dependency_excludes_non_contiguous_range_of_compatible_versions_c",
        &context.temp_dir,
    );
}

/// There is a range of compatible versions for the requested package `a`, but another dependency `c` excludes that range.
///
/// ```text
/// dependency-excludes-range-of-compatible-versions
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

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((
        r"dependency-excludes-range-of-compatible-versions-",
        "package-",
    ));

    uv_snapshot!(filters, command(&context)
        .arg("dependency-excludes-range-of-compatible-versions-a")
                .arg("dependency-excludes-range-of-compatible-versions-b<3.0.0,>=2.0.0")
                .arg("dependency-excludes-range-of-compatible-versions-c")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because package-a==1.0.0 depends on package-b==1.0.0 and only the following versions of package-a are available:
              package-a==1.0.0
              package-a>2.0.0
          we can conclude that package-a<2.0.0 depends on package-b==1.0.0.
          And because only package-a<=3.0.0 is available, we can conclude that package-a<2.0.0 depends on package-b==1.0.0. (1)

          Because only the following versions of package-c are available:
              package-c==1.0.0
              package-c==2.0.0
          and package-c==1.0.0 depends on package-a<2.0.0, we can conclude that package-c<2.0.0 depends on package-a<2.0.0.
          And because package-c==2.0.0 depends on package-a>=3.0.0, we can conclude that all versions of package-c depend on one of:
              package-a<2.0.0
              package-a>=3.0.0

          And because we know from (1) that package-a<2.0.0 depends on package-b==1.0.0, we can conclude that package-a!=3.0.0, package-b!=1.0.0, all versions of package-c are incompatible.
          And because package-a==3.0.0 depends on package-b==3.0.0, we can conclude that all versions of package-c depend on one of:
              package-b<=1.0.0
              package-b>=3.0.0

          And because you require package-b>=2.0.0,<3.0.0 and package-c, we can conclude that your requirements are unsatisfiable.
    "###);

    // Only the `2.x` versions of `a` are available since `a==1.0.0` and `a==3.0.0` require incompatible versions of `b`, but all available versions of `c` exclude that range of `a` so resolution fails.
    assert_not_installed(
        &context.venv,
        "dependency_excludes_range_of_compatible_versions_a",
        &context.temp_dir,
    );
    assert_not_installed(
        &context.venv,
        "dependency_excludes_range_of_compatible_versions_b",
        &context.temp_dir,
    );
    assert_not_installed(
        &context.venv,
        "dependency_excludes_range_of_compatible_versions_c",
        &context.temp_dir,
    );
}

/// Only one version of the requested package `a` is compatible, but the user has banned that version.
///
/// ```text
/// excluded-only-compatible-version
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

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"excluded-only-compatible-version-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("excluded-only-compatible-version-a!=2.0.0")
                .arg("excluded-only-compatible-version-b<3.0.0,>=2.0.0")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only the following versions of package-a are available:
              package-a==1.0.0
              package-a==2.0.0
              package-a==3.0.0
          and package-a==1.0.0 depends on package-b==1.0.0, we can conclude that package-a<2.0.0 depends on package-b==1.0.0.
          And because package-a==3.0.0 depends on package-b==3.0.0, we can conclude that all of:
              package-a<2.0.0
              package-a>2.0.0
          depend on one of:
              package-b==1.0.0
              package-b==3.0.0

          And because you require one of:
              package-a<2.0.0
              package-a>2.0.0
          and package-b>=2.0.0,<3.0.0, we can conclude that your requirements are unsatisfiable.
    "###);

    // Only `a==1.2.0` is available since `a==1.0.0` and `a==3.0.0` require incompatible versions of `b`. The user has excluded that version of `a` so resolution fails.
    assert_not_installed(
        &context.venv,
        "excluded_only_compatible_version_a",
        &context.temp_dir,
    );
    assert_not_installed(
        &context.venv,
        "excluded_only_compatible_version_b",
        &context.temp_dir,
    );
}

/// Only one version of the requested package is available, but the user has banned that version.
///
/// ```text
/// excluded-only-version
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

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"excluded-only-version-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("excluded-only-version-a!=1.0.0")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only package-a==1.0.0 is available and you require one of:
              package-a<1.0.0
              package-a>1.0.0
          we can conclude that your requirements are unsatisfiable.
    "###);

    // Only `a==1.0.0` is available but the user excluded it.
    assert_not_installed(&context.venv, "excluded_only_version_a", &context.temp_dir);
}

/// Multiple optional dependencies are requested for the package via an 'all' extra.
///
/// ```text
/// all-extras-required
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

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"all-extras-required-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("all-extras-required-a[all]")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + package-a==1.0.0
     + package-b==1.0.0
     + package-c==1.0.0
    "###);

    assert_installed(
        &context.venv,
        "all_extras_required_a",
        "1.0.0",
        &context.temp_dir,
    );
    assert_installed(
        &context.venv,
        "all_extras_required_b",
        "1.0.0",
        &context.temp_dir,
    );
    assert_installed(
        &context.venv,
        "all_extras_required_c",
        "1.0.0",
        &context.temp_dir,
    );
}

/// Optional dependencies are requested for the package, the extra is only available on an older version.
///
/// ```text
/// extra-does-not-exist-backtrack
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a[extra]
/// │       ├── satisfied by a-2.0.0
/// │       ├── satisfied by a-3.0.0
/// │       ├── satisfied by a-1.0.0
/// │       └── satisfied by a-1.0.0[extra]
/// ├── a
/// │   ├── a-2.0.0
/// │   ├── a-3.0.0
/// │   ├── a-1.0.0
/// │   └── a-1.0.0[extra]
/// │       └── requires b==1.0.0
/// │           └── satisfied by b-1.0.0
/// └── b
///     └── b-1.0.0
/// ```
#[test]
fn extra_does_not_exist_backtrack() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"extra-does-not-exist-backtrack-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("extra-does-not-exist-backtrack-a[extra]")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + package-a==3.0.0
    warning: The package `package-a==3.0.0` does not have an extra named `extra`
    "###);

    // The resolver should not backtrack to `a==1.0.0` because missing extras are allowed during resolution. `b` should not be installed.
    assert_installed(
        &context.venv,
        "extra_does_not_exist_backtrack_a",
        "3.0.0",
        &context.temp_dir,
    );
}

/// One of two incompatible optional dependencies are requested for the package.
///
/// ```text
/// extra-incompatible-with-extra-not-requested
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

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"extra-incompatible-with-extra-not-requested-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("extra-incompatible-with-extra-not-requested-a[extra_c]")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + package-a==1.0.0
     + package-b==2.0.0
    "###);

    // Because the user does not request both extras, it is okay that one is incompatible with the other.
    assert_installed(
        &context.venv,
        "extra_incompatible_with_extra_not_requested_a",
        "1.0.0",
        &context.temp_dir,
    );
    assert_installed(
        &context.venv,
        "extra_incompatible_with_extra_not_requested_b",
        "2.0.0",
        &context.temp_dir,
    );
}

/// Multiple optional dependencies are requested for the package, but they have conflicting requirements with each other.
///
/// ```text
/// extra-incompatible-with-extra
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

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"extra-incompatible-with-extra-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("extra-incompatible-with-extra-a[extra_b,extra_c]")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only package-a[extra-b]==1.0.0 is available and package-a[extra-b]==1.0.0 depends on package-b==1.0.0, we can conclude that all versions of package-a[extra-b] depend on package-b==1.0.0.
          And because package-a[extra-c]==1.0.0 depends on package-b==2.0.0 and only package-a[extra-c]==1.0.0 is available, we can conclude that all versions of package-a[extra-b] and all versions of package-a[extra-c] are incompatible.
          And because you require package-a[extra-b] and package-a[extra-c], we can conclude that your requirements are unsatisfiable.
    "###);

    // Because both `extra_b` and `extra_c` are requested and they require incompatible versions of `b`, `a` cannot be installed.
    assert_not_installed(
        &context.venv,
        "extra_incompatible_with_extra_a",
        &context.temp_dir,
    );
}

/// Optional dependencies are requested for the package, but the extra is not compatible with other requested versions.
///
/// ```text
/// extra-incompatible-with-root
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

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"extra-incompatible-with-root-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("extra-incompatible-with-root-a[extra]")
                .arg("extra-incompatible-with-root-b==2.0.0")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only package-a[extra]==1.0.0 is available and package-a[extra]==1.0.0 depends on package-b==1.0.0, we can conclude that all versions of package-a[extra] depend on package-b==1.0.0.
          And because you require package-a[extra] and package-b==2.0.0, we can conclude that your requirements are unsatisfiable.
    "###);

    // Because the user requested `b==2.0.0` but the requested extra requires `b==1.0.0`, the dependencies cannot be satisfied.
    assert_not_installed(
        &context.venv,
        "extra_incompatible_with_root_a",
        &context.temp_dir,
    );
    assert_not_installed(
        &context.venv,
        "extra_incompatible_with_root_b",
        &context.temp_dir,
    );
}

/// Optional dependencies are requested for the package.
///
/// ```text
/// extra-required
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

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"extra-required-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("extra-required-a[extra]")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + package-a==1.0.0
     + package-b==1.0.0
    "###);

    assert_installed(
        &context.venv,
        "extra_required_a",
        "1.0.0",
        &context.temp_dir,
    );
    assert_installed(
        &context.venv,
        "extra_required_b",
        "1.0.0",
        &context.temp_dir,
    );
}

/// Optional dependencies are requested for the package, but the extra does not exist.
///
/// ```text
/// missing-extra
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

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"missing-extra-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("missing-extra-a[extra]")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + package-a==1.0.0
    warning: The package `package-a==1.0.0` does not have an extra named `extra`
    "###);

    // Missing extras are ignored during resolution.
    assert_installed(&context.venv, "missing_extra_a", "1.0.0", &context.temp_dir);
}

/// Multiple optional dependencies are requested for the package.
///
/// ```text
/// multiple-extras-required
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

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"multiple-extras-required-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("multiple-extras-required-a[extra_b,extra_c]")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + package-a==1.0.0
     + package-b==1.0.0
     + package-c==1.0.0
    "###);

    assert_installed(
        &context.venv,
        "multiple_extras_required_a",
        "1.0.0",
        &context.temp_dir,
    );
    assert_installed(
        &context.venv,
        "multiple_extras_required_b",
        "1.0.0",
        &context.temp_dir,
    );
    assert_installed(
        &context.venv,
        "multiple_extras_required_c",
        "1.0.0",
        &context.temp_dir,
    );
}

/// The user requires two incompatible, existing versions of package `a`
///
/// ```text
/// direct-incompatible-versions
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

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"direct-incompatible-versions-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("direct-incompatible-versions-a==1.0.0")
                .arg("direct-incompatible-versions-a==2.0.0")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because you require package-a==1.0.0 and package-a==2.0.0, we can conclude that your requirements are unsatisfiable.
    "###);

    assert_not_installed(
        &context.venv,
        "direct_incompatible_versions_a",
        &context.temp_dir,
    );
    assert_not_installed(
        &context.venv,
        "direct_incompatible_versions_a",
        &context.temp_dir,
    );
}

/// The user requires `a`, which requires two incompatible, existing versions of package `b`
///
/// ```text
/// transitive-incompatible-versions
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a==1.0.0
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
///         ├── requires b==2.0.0
///             └── unsatisfied: no versions for package
///         └── requires b==1.0.0
///             └── unsatisfied: no versions for package
/// ```
#[test]
fn transitive_incompatible_versions() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"transitive-incompatible-versions-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("transitive-incompatible-versions-a==1.0.0")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because package-a==1.0.0 depends on package-b==1.0.0 and package-b==2.0.0, we can conclude that package-a==1.0.0 cannot be used.
          And because you require package-a==1.0.0, we can conclude that your requirements are unsatisfiable.
    "###);

    assert_not_installed(
        &context.venv,
        "transitive_incompatible_versions_a",
        &context.temp_dir,
    );
}

/// The user requires packages `a` and `b` but `a` requires a different version of `b`
///
/// ```text
/// transitive-incompatible-with-root-version
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

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"transitive-incompatible-with-root-version-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("transitive-incompatible-with-root-version-a")
                .arg("transitive-incompatible-with-root-version-b==1.0.0")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only package-a==1.0.0 is available and package-a==1.0.0 depends on package-b==2.0.0, we can conclude that all versions of package-a depend on package-b==2.0.0.
          And because you require package-a and package-b==1.0.0, we can conclude that your requirements are unsatisfiable.
    "###);

    assert_not_installed(
        &context.venv,
        "transitive_incompatible_with_root_version_a",
        &context.temp_dir,
    );
    assert_not_installed(
        &context.venv,
        "transitive_incompatible_with_root_version_b",
        &context.temp_dir,
    );
}

/// The user requires package `a` and `b`; `a` and `b` require different versions of `c`
///
/// ```text
/// transitive-incompatible-with-transitive
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

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"transitive-incompatible-with-transitive-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("transitive-incompatible-with-transitive-a")
                .arg("transitive-incompatible-with-transitive-b")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only package-a==1.0.0 is available and package-a==1.0.0 depends on package-c==1.0.0, we can conclude that all versions of package-a depend on package-c==1.0.0.
          And because package-b==1.0.0 depends on package-c==2.0.0 and only package-b==1.0.0 is available, we can conclude that all versions of package-a and all versions of package-b are incompatible.
          And because you require package-a and package-b, we can conclude that your requirements are unsatisfiable.
    "###);

    assert_not_installed(
        &context.venv,
        "transitive_incompatible_with_transitive_a",
        &context.temp_dir,
    );
    assert_not_installed(
        &context.venv,
        "transitive_incompatible_with_transitive_b",
        &context.temp_dir,
    );
}

/// A local version should be included in inclusive ordered comparisons.
///
/// ```text
/// local-greater-than-or-equal
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a>=1.2.3
/// │       ├── satisfied by a-1.2.3+bar
/// │       └── satisfied by a-1.2.3+foo
/// └── a
///     ├── a-1.2.3+bar
///     └── a-1.2.3+foo
/// ```
#[test]
fn local_greater_than_or_equal() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"local-greater-than-or-equal-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("local-greater-than-or-equal-a>=1.2.3")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + package-a==1.2.3+foo
    "###);

    // The version '1.2.3+foo' satisfies the constraint '>=1.2.3'.
    assert_installed(
        &context.venv,
        "local_greater_than_or_equal_a",
        "1.2.3+foo",
        &context.temp_dir,
    );
}

/// A local version should be excluded in exclusive ordered comparisons.
///
/// ```text
/// local-greater-than
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a>1.2.3
/// │       └── unsatisfied: no matching version
/// └── a
///     └── a-1.2.3+foo
/// ```
#[test]
fn local_greater_than() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"local-greater-than-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("local-greater-than-a>1.2.3")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only package-a==1.2.3+foo is available and you require package-a>1.2.3, we can conclude that your requirements are unsatisfiable.
    "###);

    assert_not_installed(&context.venv, "local_greater_than_a", &context.temp_dir);
}

/// A local version should be included in inclusive ordered comparisons.
///
/// ```text
/// local-less-than-or-equal
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a<=1.2.3
/// │       ├── satisfied by a-1.2.3+bar
/// │       └── satisfied by a-1.2.3+foo
/// └── a
///     ├── a-1.2.3+bar
///     └── a-1.2.3+foo
/// ```
#[test]
fn local_less_than_or_equal() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"local-less-than-or-equal-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("local-less-than-or-equal-a<=1.2.3")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + package-a==1.2.3+foo
    "###);

    // The version '1.2.3+foo' satisfies the constraint '<=1.2.3'.
    assert_installed(
        &context.venv,
        "local_less_than_or_equal_a",
        "1.2.3+foo",
        &context.temp_dir,
    );
}

/// A local version should be excluded in exclusive ordered comparisons.
///
/// ```text
/// local-less-than
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a<1.2.3
/// │       └── unsatisfied: no matching version
/// └── a
///     └── a-1.2.3+foo
/// ```
#[test]
fn local_less_than() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"local-less-than-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("local-less-than-a<1.2.3")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only package-a==1.2.3+foo is available and you require package-a<1.2.3, we can conclude that your requirements are unsatisfiable.
    "###);

    assert_not_installed(&context.venv, "local_less_than_a", &context.temp_dir);
}

/// Tests that we can select an older version with a local segment when newer versions are incompatible.
///
/// ```text
/// local-not-latest
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a>=1
/// │       ├── satisfied by a-1.2.3
/// │       ├── satisfied by a-1.2.2+foo
/// │       └── satisfied by a-1.2.1+foo
/// └── a
///     ├── a-1.2.3
///     ├── a-1.2.2+foo
///     └── a-1.2.1+foo
/// ```
#[test]
fn local_not_latest() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"local-not-latest-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("local-not-latest-a>=1")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + package-a==1.2.1+foo
    "###);

    assert_installed(
        &context.venv,
        "local_not_latest_a",
        "1.2.1+foo",
        &context.temp_dir,
    );
}

/// If there is a 1.2.3 version with an sdist published and no compatible wheels, then the sdist will be used.
///
/// ```text
/// local-not-used-with-sdist
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a==1.2.3
/// │       ├── satisfied by a-1.2.3
/// │       └── satisfied by a-1.2.3+foo
/// └── a
///     ├── a-1.2.3
///     └── a-1.2.3+foo
/// ```
#[test]
fn local_not_used_with_sdist() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"local-not-used-with-sdist-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("local-not-used-with-sdist-a==1.2.3")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + package-a==1.2.3+foo
    "###);

    // The version '1.2.3' with an sdist satisfies the constraint '==1.2.3'.
    assert_installed(
        &context.venv,
        "local_not_used_with_sdist_a",
        "1.2.3+foo",
        &context.temp_dir,
    );
}

/// A simple version constraint should not exclude published versions with local segments.
///
/// ```text
/// local-simple
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a==1.2.3
/// │       ├── satisfied by a-1.2.3+bar
/// │       └── satisfied by a-1.2.3+foo
/// └── a
///     ├── a-1.2.3+bar
///     └── a-1.2.3+foo
/// ```
#[test]
fn local_simple() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"local-simple-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("local-simple-a==1.2.3")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + package-a==1.2.3+foo
    "###);

    // The version '1.2.3+foo' satisfies the constraint '==1.2.3'.
    assert_installed(
        &context.venv,
        "local_simple_a",
        "1.2.3+foo",
        &context.temp_dir,
    );
}

/// A dependency depends on a conflicting local version of a direct dependency, but we can backtrack to a compatible version.
///
/// ```text
/// local-transitive-backtrack
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   ├── requires a
/// │   │   ├── satisfied by a-1.0.0
/// │   │   └── satisfied by a-2.0.0
/// │   └── requires b==2.0.0+foo
/// │       └── satisfied by b-2.0.0+foo
/// ├── a
/// │   ├── a-1.0.0
/// │   │   └── requires b==2.0.0
/// │   │       ├── satisfied by b-2.0.0+bar
/// │   │       └── satisfied by b-2.0.0+foo
/// │   └── a-2.0.0
/// │       └── requires b==2.0.0+bar
/// │           └── satisfied by b-2.0.0+bar
/// └── b
///     ├── b-2.0.0+bar
///     └── b-2.0.0+foo
/// ```
#[test]
fn local_transitive_backtrack() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"local-transitive-backtrack-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("local-transitive-backtrack-a")
                .arg("local-transitive-backtrack-b==2.0.0+foo")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + package-a==1.0.0
     + package-b==2.0.0+foo
    "###);

    // Backtracking to '1.0.0' gives us compatible local versions of b.
    assert_installed(
        &context.venv,
        "local_transitive_backtrack_a",
        "1.0.0",
        &context.temp_dir,
    );
    assert_installed(
        &context.venv,
        "local_transitive_backtrack_b",
        "2.0.0+foo",
        &context.temp_dir,
    );
}

/// A dependency depends on a conflicting local version of a direct dependency.
///
/// ```text
/// local-transitive-conflicting
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   ├── requires a
/// │   │   └── satisfied by a-1.0.0
/// │   └── requires b==2.0.0+foo
/// │       └── satisfied by b-2.0.0+foo
/// ├── a
/// │   └── a-1.0.0
/// │       └── requires b==2.0.0+bar
/// │           └── satisfied by b-2.0.0+bar
/// └── b
///     ├── b-2.0.0+bar
///     └── b-2.0.0+foo
/// ```
#[test]
fn local_transitive_conflicting() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"local-transitive-conflicting-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("local-transitive-conflicting-a")
                .arg("local-transitive-conflicting-b==2.0.0+foo")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only package-a==1.0.0 is available and package-a==1.0.0 depends on package-b==2.0.0+bar, we can conclude that all versions of package-a depend on package-b==2.0.0+bar.
          And because you require package-a and package-b==2.0.0+foo, we can conclude that your requirements are unsatisfiable.
    "###);

    assert_not_installed(
        &context.venv,
        "local_transitive_conflicting_a",
        &context.temp_dir,
    );
    assert_not_installed(
        &context.venv,
        "local_transitive_conflicting_b",
        &context.temp_dir,
    );
}

/// A transitive dependency has both a non-local and local version published, but the non-local version is unusable.
///
/// ```text
/// local-transitive-confounding
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a
/// │       └── satisfied by a-1.0.0
/// ├── a
/// │   └── a-1.0.0
/// │       └── requires b==2.0.0
/// │           ├── satisfied by b-2.0.0
/// │           ├── satisfied by b-2.0.0+bar
/// │           └── satisfied by b-2.0.0+foo
/// └── b
///     ├── b-2.0.0
///     ├── b-2.0.0+bar
///     └── b-2.0.0+foo
/// ```
#[test]
fn local_transitive_confounding() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"local-transitive-confounding-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("local-transitive-confounding-a")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + package-a==1.0.0
     + package-b==2.0.0+foo
    "###);

    // The version '2.0.0+foo' satisfies the constraint '==2.0.0'.
    assert_installed(
        &context.venv,
        "local_transitive_confounding_a",
        "1.0.0",
        &context.temp_dir,
    );
    assert_installed(
        &context.venv,
        "local_transitive_confounding_b",
        "2.0.0+foo",
        &context.temp_dir,
    );
}

/// A transitive constraint on a local version should match an inclusive ordered operator.
///
/// ```text
/// local-transitive-greater-than-or-equal
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   ├── requires a
/// │   │   └── satisfied by a-1.0.0
/// │   └── requires b==2.0.0+foo
/// │       └── satisfied by b-2.0.0+foo
/// ├── a
/// │   └── a-1.0.0
/// │       └── requires b>=2.0.0
/// │           ├── satisfied by b-2.0.0+bar
/// │           └── satisfied by b-2.0.0+foo
/// └── b
///     ├── b-2.0.0+bar
///     └── b-2.0.0+foo
/// ```
#[test]
fn local_transitive_greater_than_or_equal() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"local-transitive-greater-than-or-equal-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("local-transitive-greater-than-or-equal-a")
                .arg("local-transitive-greater-than-or-equal-b==2.0.0+foo")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + package-a==1.0.0
     + package-b==2.0.0+foo
    "###);

    // The version '2.0.0+foo' satisfies both >=2.0.0 and ==2.0.0+foo.
    assert_installed(
        &context.venv,
        "local_transitive_greater_than_or_equal_a",
        "1.0.0",
        &context.temp_dir,
    );
    assert_installed(
        &context.venv,
        "local_transitive_greater_than_or_equal_b",
        "2.0.0+foo",
        &context.temp_dir,
    );
}

/// A transitive constraint on a local version should not match an exclusive ordered operator.
///
/// ```text
/// local-transitive-greater-than
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   ├── requires a
/// │   │   └── satisfied by a-1.0.0
/// │   └── requires b==2.0.0+foo
/// │       └── satisfied by b-2.0.0+foo
/// ├── a
/// │   └── a-1.0.0
/// │       └── requires b>2.0.0
/// │           └── unsatisfied: no matching version
/// └── b
///     ├── b-2.0.0+bar
///     └── b-2.0.0+foo
/// ```
#[test]
fn local_transitive_greater_than() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"local-transitive-greater-than-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("local-transitive-greater-than-a")
                .arg("local-transitive-greater-than-b==2.0.0+foo")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only package-a==1.0.0 is available and package-a==1.0.0 depends on package-b>2.0.0, we can conclude that all versions of package-a depend on package-b>2.0.0.
          And because you require package-a and package-b==2.0.0+foo, we can conclude that your requirements are unsatisfiable.
    "###);

    assert_not_installed(
        &context.venv,
        "local_transitive_greater_than_a",
        &context.temp_dir,
    );
    assert_not_installed(
        &context.venv,
        "local_transitive_greater_than_b",
        &context.temp_dir,
    );
}

/// A transitive constraint on a local version should match an inclusive ordered operator.
///
/// ```text
/// local-transitive-less-than-or-equal
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   ├── requires a
/// │   │   └── satisfied by a-1.0.0
/// │   └── requires b==2.0.0+foo
/// │       └── satisfied by b-2.0.0+foo
/// ├── a
/// │   └── a-1.0.0
/// │       └── requires b<=2.0.0
/// │           ├── satisfied by b-2.0.0+bar
/// │           └── satisfied by b-2.0.0+foo
/// └── b
///     ├── b-2.0.0+bar
///     └── b-2.0.0+foo
/// ```
#[test]
fn local_transitive_less_than_or_equal() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"local-transitive-less-than-or-equal-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("local-transitive-less-than-or-equal-a")
                .arg("local-transitive-less-than-or-equal-b==2.0.0+foo")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + package-a==1.0.0
     + package-b==2.0.0+foo
    "###);

    // The version '2.0.0+foo' satisfies both <=2.0.0 and ==2.0.0+foo.
    assert_installed(
        &context.venv,
        "local_transitive_less_than_or_equal_a",
        "1.0.0",
        &context.temp_dir,
    );
    assert_installed(
        &context.venv,
        "local_transitive_less_than_or_equal_b",
        "2.0.0+foo",
        &context.temp_dir,
    );
}

/// A transitive constraint on a local version should not match an exclusive ordered operator.
///
/// ```text
/// local-transitive-less-than
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   ├── requires a
/// │   │   └── satisfied by a-1.0.0
/// │   └── requires b==2.0.0+foo
/// │       └── satisfied by b-2.0.0+foo
/// ├── a
/// │   └── a-1.0.0
/// │       └── requires b<2.0.0
/// │           └── unsatisfied: no matching version
/// └── b
///     ├── b-2.0.0+bar
///     └── b-2.0.0+foo
/// ```
#[test]
fn local_transitive_less_than() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"local-transitive-less-than-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("local-transitive-less-than-a")
                .arg("local-transitive-less-than-b==2.0.0+foo")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only package-a==1.0.0 is available and package-a==1.0.0 depends on package-b<2.0.0, we can conclude that all versions of package-a depend on package-b<2.0.0.
          And because you require package-a and package-b==2.0.0+foo, we can conclude that your requirements are unsatisfiable.
    "###);

    assert_not_installed(
        &context.venv,
        "local_transitive_less_than_a",
        &context.temp_dir,
    );
    assert_not_installed(
        &context.venv,
        "local_transitive_less_than_b",
        &context.temp_dir,
    );
}

/// A simple version constraint should not exclude published versions with local segments.
///
/// ```text
/// local-transitive
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   ├── requires a
/// │   │   └── satisfied by a-1.0.0
/// │   └── requires b==2.0.0+foo
/// │       └── satisfied by b-2.0.0+foo
/// ├── a
/// │   └── a-1.0.0
/// │       └── requires b==2.0.0
/// │           ├── satisfied by b-2.0.0+foo
/// │           └── satisfied by b-2.0.0+bar
/// └── b
///     ├── b-2.0.0+foo
///     └── b-2.0.0+bar
/// ```
#[test]
fn local_transitive() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"local-transitive-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("local-transitive-a")
                .arg("local-transitive-b==2.0.0+foo")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + package-a==1.0.0
     + package-b==2.0.0+foo
    "###);

    // The version '2.0.0+foo' satisfies both ==2.0.0 and ==2.0.0+foo.
    assert_installed(
        &context.venv,
        "local_transitive_a",
        "1.0.0",
        &context.temp_dir,
    );
    assert_installed(
        &context.venv,
        "local_transitive_b",
        "2.0.0+foo",
        &context.temp_dir,
    );
}

/// Even if there is a 1.2.3 version published, if it is unavailable for some reason (no sdist and no compatible wheels in this case), a 1.2.3 version with a local segment should be usable instead.
///
/// ```text
/// local-used-without-sdist
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a==1.2.3
/// │       ├── satisfied by a-1.2.3
/// │       └── satisfied by a-1.2.3+foo
/// └── a
///     ├── a-1.2.3
///     └── a-1.2.3+foo
/// ```
#[test]
fn local_used_without_sdist() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"local-used-without-sdist-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("local-used-without-sdist-a==1.2.3")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + package-a==1.2.3+foo
    "###);

    // The version '1.2.3+foo' satisfies the constraint '==1.2.3'.
    assert_installed(
        &context.venv,
        "local_used_without_sdist_a",
        "1.2.3+foo",
        &context.temp_dir,
    );
}

/// An equal version constraint should match a post-release version if the post-release version is available.
///
/// ```text
/// post-equal-available
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a==1.2.3.post0
/// │       └── satisfied by a-1.2.3.post0
/// └── a
///     ├── a-1.2.3
///     └── a-1.2.3.post0
/// ```
#[test]
fn post_equal_available() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"post-equal-available-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("post-equal-available-a==1.2.3.post0")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + package-a==1.2.3.post0
    "###);

    // The version '1.2.3.post0' satisfies the constraint '==1.2.3.post0'.
    assert_installed(
        &context.venv,
        "post_equal_available_a",
        "1.2.3.post0",
        &context.temp_dir,
    );
}

/// An equal version constraint should not match a post-release version if the post-release version is not available.
///
/// ```text
/// post-equal-not-available
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a==1.2.3.post0
/// │       └── unsatisfied: no matching version
/// └── a
///     ├── a-1.2.3
///     └── a-1.2.3.post1
/// ```
#[test]
fn post_equal_not_available() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"post-equal-not-available-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("post-equal-not-available-a==1.2.3.post0")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because there is no version of package-a==1.2.3.post0 and you require package-a==1.2.3.post0, we can conclude that your requirements are unsatisfiable.
    "###);

    assert_not_installed(
        &context.venv,
        "post_equal_not_available_a",
        &context.temp_dir,
    );
}

/// A greater-than-or-equal version constraint should match a post-release version if the constraint is itself a post-release version.
///
/// ```text
/// post-greater-than-or-equal-post
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a>=1.2.3.post0
/// │       ├── satisfied by a-1.2.3.post0
/// │       └── satisfied by a-1.2.3.post1
/// └── a
///     ├── a-1.2.3.post0
///     └── a-1.2.3.post1
/// ```
#[test]
fn post_greater_than_or_equal_post() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"post-greater-than-or-equal-post-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("post-greater-than-or-equal-post-a>=1.2.3.post0")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + package-a==1.2.3.post1
    "###);

    // The version '1.2.3.post1' satisfies the constraint '>=1.2.3.post0'.
    assert_installed(
        &context.venv,
        "post_greater_than_or_equal_post_a",
        "1.2.3.post1",
        &context.temp_dir,
    );
}

/// A greater-than-or-equal version constraint should match a post-release version.
///
/// ```text
/// post-greater-than-or-equal
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a>=1.2.3
/// │       └── satisfied by a-1.2.3.post1
/// └── a
///     └── a-1.2.3.post1
/// ```
#[test]
fn post_greater_than_or_equal() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"post-greater-than-or-equal-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("post-greater-than-or-equal-a>=1.2.3")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + package-a==1.2.3.post1
    "###);

    // The version '1.2.3.post1' satisfies the constraint '>=1.2.3'.
    assert_installed(
        &context.venv,
        "post_greater_than_or_equal_a",
        "1.2.3.post1",
        &context.temp_dir,
    );
}

/// A greater-than version constraint should not match a post-release version if the post-release version is not available.
///
/// ```text
/// post-greater-than-post-not-available
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a>1.2.3.post2
/// │       └── unsatisfied: no matching version
/// └── a
///     ├── a-1.2.3
///     ├── a-1.2.3.post0
///     └── a-1.2.3.post1
/// ```
#[test]
fn post_greater_than_post_not_available() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"post-greater-than-post-not-available-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("post-greater-than-post-not-available-a>1.2.3.post2")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only package-a<=1.2.3.post1 is available and you require package-a>=1.2.3.post3, we can conclude that your requirements are unsatisfiable.
    "###);

    assert_not_installed(
        &context.venv,
        "post_greater_than_post_not_available_a",
        &context.temp_dir,
    );
}

/// A greater-than version constraint should match a post-release version if the constraint is itself a post-release version.
///
/// ```text
/// post-greater-than-post
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a>1.2.3.post0
/// │       └── satisfied by a-1.2.3.post1
/// └── a
///     ├── a-1.2.3.post0
///     └── a-1.2.3.post1
/// ```
#[test]
fn post_greater_than_post() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"post-greater-than-post-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("post-greater-than-post-a>1.2.3.post0")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + package-a==1.2.3.post1
    "###);

    // The version '1.2.3.post1' satisfies the constraint '>1.2.3.post0'.
    assert_installed(
        &context.venv,
        "post_greater_than_post_a",
        "1.2.3.post1",
        &context.temp_dir,
    );
}

/// A greater-than version constraint should not match a post-release version.
///
/// ```text
/// post-greater-than
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a>1.2.3
/// │       └── unsatisfied: no matching version
/// └── a
///     └── a-1.2.3.post1
/// ```
#[test]
fn post_greater_than() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"post-greater-than-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("post-greater-than-a>1.2.3")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only package-a==1.2.3.post1 is available and you require package-a>1.2.3, we can conclude that your requirements are unsatisfiable.
    "###);

    assert_not_installed(&context.venv, "post_greater_than_a", &context.temp_dir);
}

/// A less-than-or-equal version constraint should not match a post-release version.
///
/// ```text
/// post-less-than-or-equal
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a<=1.2.3
/// │       └── unsatisfied: no matching version
/// └── a
///     └── a-1.2.3.post1
/// ```
#[test]
fn post_less_than_or_equal() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"post-less-than-or-equal-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("post-less-than-or-equal-a<=1.2.3")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only package-a==1.2.3.post1 is available and you require package-a<=1.2.3, we can conclude that your requirements are unsatisfiable.
    "###);

    assert_not_installed(
        &context.venv,
        "post_less_than_or_equal_a",
        &context.temp_dir,
    );
}

/// A less-than version constraint should not match a post-release version.
///
/// ```text
/// post-less-than
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a<1.2.3
/// │       └── unsatisfied: no matching version
/// └── a
///     └── a-1.2.3.post1
/// ```
#[test]
fn post_less_than() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"post-less-than-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("post-less-than-a<1.2.3")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only package-a==1.2.3.post1 is available and you require package-a<1.2.3, we can conclude that your requirements are unsatisfiable.
    "###);

    assert_not_installed(&context.venv, "post_less_than_a", &context.temp_dir);
}

/// A greater-than version constraint should not match a post-release version with a local version identifier.
///
/// ```text
/// post-local-greater-than-post
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a>1.2.3.post1
/// │       └── unsatisfied: no matching version
/// └── a
///     ├── a-1.2.3.post1
///     └── a-1.2.3.post1+local
/// ```
#[test]
fn post_local_greater_than_post() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"post-local-greater-than-post-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("post-local-greater-than-post-a>1.2.3.post1")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only package-a<=1.2.3.post1+local is available and you require package-a>=1.2.3.post2, we can conclude that your requirements are unsatisfiable.
    "###);

    assert_not_installed(
        &context.venv,
        "post_local_greater_than_post_a",
        &context.temp_dir,
    );
}

/// A greater-than version constraint should not match a post-release version with a local version identifier.
///
/// ```text
/// post-local-greater-than
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a>1.2.3
/// │       └── unsatisfied: no matching version
/// └── a
///     ├── a-1.2.3.post1
///     └── a-1.2.3.post1+local
/// ```
#[test]
fn post_local_greater_than() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"post-local-greater-than-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("post-local-greater-than-a>1.2.3")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only package-a<=1.2.3.post1+local is available and you require package-a>1.2.3, we can conclude that your requirements are unsatisfiable.
    "###);

    assert_not_installed(
        &context.venv,
        "post_local_greater_than_a",
        &context.temp_dir,
    );
}

/// A simple version constraint should not match a post-release version.
///
/// ```text
/// post-simple
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a==1.2.3
/// │       └── unsatisfied: no matching version
/// └── a
///     └── a-1.2.3.post1
/// ```
#[test]
fn post_simple() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"post-simple-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("post-simple-a==1.2.3")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because there is no version of package-a==1.2.3 and you require package-a==1.2.3, we can conclude that your requirements are unsatisfiable.
    "###);

    assert_not_installed(&context.venv, "post_simple_a", &context.temp_dir);
}

/// The user requires `a` which has multiple prereleases available with different labels.
///
/// ```text
/// package-multiple-prereleases-kinds
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

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"package-multiple-prereleases-kinds-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("package-multiple-prereleases-kinds-a>=1.0.0a1")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + package-a==1.0.0rc1
    "###);

    // Release candidates should be the highest precedence prerelease kind.
    assert_installed(
        &context.venv,
        "package_multiple_prereleases_kinds_a",
        "1.0.0rc1",
        &context.temp_dir,
    );
}

/// The user requires `a` which has multiple alphas available.
///
/// ```text
/// package-multiple-prereleases-numbers
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

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"package-multiple-prereleases-numbers-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("package-multiple-prereleases-numbers-a>=1.0.0a1")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + package-a==1.0.0a3
    "###);

    // The latest alpha version should be selected.
    assert_installed(
        &context.venv,
        "package_multiple_prereleases_numbers_a",
        "1.0.0a3",
        &context.temp_dir,
    );
}

/// The user requires a non-prerelease version of `a` which only has prerelease versions available. There are pre-releases on the boundary of their range.
///
/// ```text
/// package-only-prereleases-boundary
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a<0.2.0
/// │       └── unsatisfied: no matching version
/// └── a
///     ├── a-0.1.0a1
///     ├── a-0.2.0a1
///     └── a-0.3.0a1
/// ```
#[test]
fn package_only_prereleases_boundary() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"package-only-prereleases-boundary-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("package-only-prereleases-boundary-a<0.2.0")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + package-a==0.1.0a1
    "###);

    // Since there are only prerelease versions of `a` available, a prerelease is allowed. Since the user did not explicitly request a pre-release, pre-releases at the boundary should not be selected.
    assert_installed(
        &context.venv,
        "package_only_prereleases_boundary_a",
        "0.1.0a1",
        &context.temp_dir,
    );
}

/// The user requires a version of package `a` which only matches prerelease versions but they did not include a prerelease specifier.
///
/// ```text
/// package-only-prereleases-in-range
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

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"package-only-prereleases-in-range-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("package-only-prereleases-in-range-a>0.1.0")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only package-a<0.1.0 is available and you require package-a>0.1.0, we can conclude that your requirements are unsatisfiable.

          hint: Pre-releases are available for `package-a` in the requested range (e.g., 1.0.0a1), but pre-releases weren't enabled (try: `--prerelease=allow`)
    "###);

    // Since there are stable versions of `a` available, prerelease versions should not be selected without explicit opt-in.
    assert_not_installed(
        &context.venv,
        "package_only_prereleases_in_range_a",
        &context.temp_dir,
    );
}

/// The user requires any version of package `a` which only has prerelease versions available.
///
/// ```text
/// package-only-prereleases
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

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"package-only-prereleases-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("package-only-prereleases-a")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + package-a==1.0.0a1
    "###);

    // Since there are only prerelease versions of `a` available, it should be installed even though the user did not include a prerelease specifier.
    assert_installed(
        &context.venv,
        "package_only_prereleases_a",
        "1.0.0a1",
        &context.temp_dir,
    );
}

/// The user requires a version of `a` with a prerelease specifier and both prerelease and stable releases are available.
///
/// ```text
/// package-prerelease-specified-mixed-available
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

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"package-prerelease-specified-mixed-available-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("package-prerelease-specified-mixed-available-a>=0.1.0a1")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + package-a==1.0.0a1
    "###);

    // Since the user provided a prerelease specifier, the latest prerelease version should be selected.
    assert_installed(
        &context.venv,
        "package_prerelease_specified_mixed_available_a",
        "1.0.0a1",
        &context.temp_dir,
    );
}

/// The user requires a version of `a` with a prerelease specifier and only stable releases are available.
///
/// ```text
/// package-prerelease-specified-only-final-available
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

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((
        r"package-prerelease-specified-only-final-available-",
        "package-",
    ));

    uv_snapshot!(filters, command(&context)
        .arg("package-prerelease-specified-only-final-available-a>=0.1.0a1")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + package-a==0.3.0
    "###);

    // The latest stable version should be selected.
    assert_installed(
        &context.venv,
        "package_prerelease_specified_only_final_available_a",
        "0.3.0",
        &context.temp_dir,
    );
}

/// The user requires a version of `a` with a prerelease specifier and only prerelease releases are available.
///
/// ```text
/// package-prerelease-specified-only-prerelease-available
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

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((
        r"package-prerelease-specified-only-prerelease-available-",
        "package-",
    ));

    uv_snapshot!(filters, command(&context)
        .arg("package-prerelease-specified-only-prerelease-available-a>=0.1.0a1")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + package-a==0.3.0a1
    "###);

    // The latest prerelease version should be selected.
    assert_installed(
        &context.venv,
        "package_prerelease_specified_only_prerelease_available_a",
        "0.3.0a1",
        &context.temp_dir,
    );
}

/// The user requires a non-prerelease version of `a` but has enabled pre-releases. There are pre-releases on the boundary of their range.
///
/// ```text
/// package-prereleases-boundary
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a<0.2.0
/// │       └── satisfied by a-0.1.0
/// └── a
///     ├── a-0.1.0
///     ├── a-0.2.0a1
///     └── a-0.3.0
/// ```
#[test]
fn package_prereleases_boundary() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"package-prereleases-boundary-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("--prerelease=allow")
        .arg("package-prereleases-boundary-a<0.2.0")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + package-a==0.1.0
    "###);

    // Since the user did not use a pre-release specifier, pre-releases at the boundary should not be selected even though pre-releases are allowed.
    assert_installed(
        &context.venv,
        "package_prereleases_boundary_a",
        "0.1.0",
        &context.temp_dir,
    );
}

/// The user requires a non-prerelease version of `a` but has enabled pre-releases. There are pre-releases on the boundary of their range.
///
/// ```text
/// package-prereleases-global-boundary
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a<0.2.0
/// │       └── satisfied by a-0.1.0
/// └── a
///     ├── a-0.1.0
///     ├── a-0.2.0a1
///     └── a-0.3.0
/// ```
#[test]
fn package_prereleases_global_boundary() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"package-prereleases-global-boundary-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("--prerelease=allow")
        .arg("package-prereleases-global-boundary-a<0.2.0")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + package-a==0.1.0
    "###);

    // Since the user did not use a pre-release specifier, pre-releases at the boundary should not be selected even though pre-releases are allowed.
    assert_installed(
        &context.venv,
        "package_prereleases_global_boundary_a",
        "0.1.0",
        &context.temp_dir,
    );
}

/// The user requires a prerelease version of `a`. There are pre-releases on the boundary of their range.
///
/// ```text
/// package-prereleases-specifier-boundary
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a<0.2.0a2
/// │       ├── satisfied by a-0.1.0
/// │       └── satisfied by a-0.2.0a1
/// └── a
///     ├── a-0.1.0
///     ├── a-0.2.0
///     ├── a-0.2.0a1
///     ├── a-0.2.0a2
///     ├── a-0.2.0a3
///     └── a-0.3.0
/// ```
#[test]
fn package_prereleases_specifier_boundary() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"package-prereleases-specifier-boundary-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("package-prereleases-specifier-boundary-a<0.2.0a2")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + package-a==0.2.0a1
    "###);

    // Since the user used a pre-release specifier, pre-releases at the boundary should be selected.
    assert_installed(
        &context.venv,
        "package_prereleases_specifier_boundary_a",
        "0.2.0a1",
        &context.temp_dir,
    );
}

/// The user requires a version of package `a` which only matches prerelease versions. They did not include a prerelease specifier for the package, but they opted into prereleases globally.
///
/// ```text
/// requires-package-only-prereleases-in-range-global-opt-in
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

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((
        r"requires-package-only-prereleases-in-range-global-opt-in-",
        "package-",
    ));

    uv_snapshot!(filters, command(&context)
        .arg("--prerelease=allow")
        .arg("requires-package-only-prereleases-in-range-global-opt-in-a>0.1.0")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + package-a==1.0.0a1
    "###);

    assert_installed(
        &context.venv,
        "requires_package_only_prereleases_in_range_global_opt_in_a",
        "1.0.0a1",
        &context.temp_dir,
    );
}

/// The user requires any version of package `a` has a prerelease version available and an older non-prerelease version.
///
/// ```text
/// requires-package-prerelease-and-final-any
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

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"requires-package-prerelease-and-final-any-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("requires-package-prerelease-and-final-any-a")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + package-a==0.1.0
    "###);

    // Since the user did not provide a prerelease specifier, the older stable version should be selected.
    assert_installed(
        &context.venv,
        "requires_package_prerelease_and_final_any_a",
        "0.1.0",
        &context.temp_dir,
    );
}

/// The user requires package `a` which has a dependency on a package which only matches prerelease versions; the user has opted into allowing prereleases in `b` explicitly.
///
/// ```text
/// transitive-package-only-prereleases-in-range-opt-in
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   ├── requires a
/// │   │   └── satisfied by a-0.1.0
/// │   └── requires b>0.0.0a1
/// │       ├── satisfied by b-0.1.0
/// │       └── satisfied by b-1.0.0a1
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

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((
        r"transitive-package-only-prereleases-in-range-opt-in-",
        "package-",
    ));

    uv_snapshot!(filters, command(&context)
        .arg("transitive-package-only-prereleases-in-range-opt-in-a")
                .arg("transitive-package-only-prereleases-in-range-opt-in-b>0.0.0a1")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + package-a==0.1.0
     + package-b==1.0.0a1
    "###);

    // Since the user included a dependency on `b` with a prerelease specifier, a prerelease version can be selected.
    assert_installed(
        &context.venv,
        "transitive_package_only_prereleases_in_range_opt_in_a",
        "0.1.0",
        &context.temp_dir,
    );
    assert_installed(
        &context.venv,
        "transitive_package_only_prereleases_in_range_opt_in_b",
        "1.0.0a1",
        &context.temp_dir,
    );
}

/// The user requires package `a` which has a dependency on a package which only matches prerelease versions but they did not include a prerelease specifier.
///
/// ```text
/// transitive-package-only-prereleases-in-range
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

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"transitive-package-only-prereleases-in-range-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("transitive-package-only-prereleases-in-range-a")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only package-b<0.1 is available and package-a==0.1.0 depends on package-b>0.1, we can conclude that package-a==0.1.0 cannot be used.
          And because only package-a==0.1.0 is available and you require package-a, we can conclude that your requirements are unsatisfiable.

          hint: Pre-releases are available for `package-b` in the requested range (e.g., 1.0.0a1), but pre-releases weren't enabled (try: `--prerelease=allow`)
    "###);

    // Since there are stable versions of `b` available, the prerelease version should not be selected without explicit opt-in. The available version is excluded by the range requested by the user.
    assert_not_installed(
        &context.venv,
        "transitive_package_only_prereleases_in_range_a",
        &context.temp_dir,
    );
}

/// The user requires any version of package `a` which requires `b` which only has prerelease versions available.
///
/// ```text
/// transitive-package-only-prereleases
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

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"transitive-package-only-prereleases-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("transitive-package-only-prereleases-a")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + package-a==0.1.0
     + package-b==1.0.0a1
    "###);

    // Since there are only prerelease versions of `b` available, it should be selected even though the user did not opt-in to prereleases.
    assert_installed(
        &context.venv,
        "transitive_package_only_prereleases_a",
        "0.1.0",
        &context.temp_dir,
    );
    assert_installed(
        &context.venv,
        "transitive_package_only_prereleases_b",
        "1.0.0a1",
        &context.temp_dir,
    );
}

/// A transitive dependency has both a prerelease and a stable selector, but can only be satisfied by a prerelease. There are many prerelease versions and some are excluded.
///
/// ```text
/// transitive-prerelease-and-stable-dependency-many-versions-holes
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
/// │           ├── satisfied by c-2.0.0a1
/// │           ├── satisfied by c-2.0.0a2
/// │           ├── satisfied by c-2.0.0a3
/// │           ├── satisfied by c-2.0.0a4
/// │           ├── satisfied by c-2.0.0a8
/// │           ├── satisfied by c-2.0.0a9
/// │           ├── satisfied by c-2.0.0b2
/// │           ├── satisfied by c-2.0.0b3
/// │           └── satisfied by c-2.0.0b4
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

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((
        r"transitive-prerelease-and-stable-dependency-many-versions-holes-",
        "package-",
    ));

    uv_snapshot!(filters, command(&context)
        .arg("transitive-prerelease-and-stable-dependency-many-versions-holes-a")
                .arg("transitive-prerelease-and-stable-dependency-many-versions-holes-b")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only the following versions of package-c are available:
              package-c<1.0.0
              package-c>=2.0.0a5,<=2.0.0a7
              package-c==2.0.0b1
              package-c>=2.0.0b5
          and package-a==1.0.0 depends on one of:
              package-c>1.0.0,<2.0.0a5
              package-c>2.0.0a7,<2.0.0b1
              package-c>2.0.0b1,<2.0.0b5
          we can conclude that package-a==1.0.0 cannot be used.
          And because only package-a==1.0.0 is available and you require package-a, we can conclude that your requirements are unsatisfiable.

          hint: `package-c` was requested with a pre-release marker (e.g., all of:
              package-c>1.0.0,<2.0.0a5
              package-c>2.0.0a7,<2.0.0b1
              package-c>2.0.0b1,<2.0.0b5
          ), but pre-releases weren't enabled (try: `--prerelease=allow`)
    "###);

    // Since the user did not explicitly opt-in to a prerelease, it cannot be selected.
    assert_not_installed(
        &context.venv,
        "transitive_prerelease_and_stable_dependency_many_versions_holes_a",
        &context.temp_dir,
    );
    assert_not_installed(
        &context.venv,
        "transitive_prerelease_and_stable_dependency_many_versions_holes_b",
        &context.temp_dir,
    );
}

/// A transitive dependency has both a prerelease and a stable selector, but can only be satisfied by a prerelease. There are many prerelease versions.
///
/// ```text
/// transitive-prerelease-and-stable-dependency-many-versions
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

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((
        r"transitive-prerelease-and-stable-dependency-many-versions-",
        "package-",
    ));

    uv_snapshot!(filters, command(&context)
        .arg("transitive-prerelease-and-stable-dependency-many-versions-a")
                .arg("transitive-prerelease-and-stable-dependency-many-versions-b")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only package-a==1.0.0 is available and package-a==1.0.0 depends on package-c>=2.0.0b1, we can conclude that all versions of package-a depend on package-c>=2.0.0b1.
          And because only package-c<2.0.0b1 is available, we can conclude that all versions of package-a depend on package-c>3.0.0.
          And because package-b==1.0.0 depends on package-c and only package-b==1.0.0 is available, we can conclude that all versions of package-a and all versions of package-b are incompatible.
          And because you require package-a and package-b, we can conclude that your requirements are unsatisfiable.

          hint: `package-c` was requested with a pre-release marker (e.g., package-c>=2.0.0b1), but pre-releases weren't enabled (try: `--prerelease=allow`)
    "###);

    // Since the user did not explicitly opt-in to a prerelease, it cannot be selected.
    assert_not_installed(
        &context.venv,
        "transitive_prerelease_and_stable_dependency_many_versions_a",
        &context.temp_dir,
    );
    assert_not_installed(
        &context.venv,
        "transitive_prerelease_and_stable_dependency_many_versions_b",
        &context.temp_dir,
    );
}

/// A transitive dependency has both a prerelease and a stable selector, but can only be satisfied by a prerelease. The user includes an opt-in to prereleases of the transitive dependency.
///
/// ```text
/// transitive-prerelease-and-stable-dependency-opt-in
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

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((
        r"transitive-prerelease-and-stable-dependency-opt-in-",
        "package-",
    ));

    uv_snapshot!(filters, command(&context)
        .arg("transitive-prerelease-and-stable-dependency-opt-in-a")
                .arg("transitive-prerelease-and-stable-dependency-opt-in-b")
                .arg("transitive-prerelease-and-stable-dependency-opt-in-c>=0.0.0a1")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + package-a==1.0.0
     + package-b==1.0.0
     + package-c==2.0.0b1
    "###);

    // Since the user explicitly opted-in to a prerelease for `c`, it can be installed.
    assert_installed(
        &context.venv,
        "transitive_prerelease_and_stable_dependency_opt_in_a",
        "1.0.0",
        &context.temp_dir,
    );
    assert_installed(
        &context.venv,
        "transitive_prerelease_and_stable_dependency_opt_in_b",
        "1.0.0",
        &context.temp_dir,
    );
    assert_installed(
        &context.venv,
        "transitive_prerelease_and_stable_dependency_opt_in_c",
        "2.0.0b1",
        &context.temp_dir,
    );
}

/// A transitive dependency has both a prerelease and a stable selector, but can only be satisfied by a prerelease
///
/// ```text
/// transitive-prerelease-and-stable-dependency
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

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"transitive-prerelease-and-stable-dependency-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("transitive-prerelease-and-stable-dependency-a")
                .arg("transitive-prerelease-and-stable-dependency-b")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because there is no version of package-c==2.0.0b1 and package-a==1.0.0 depends on package-c==2.0.0b1, we can conclude that package-a==1.0.0 cannot be used.
          And because only package-a==1.0.0 is available and you require package-a, we can conclude that your requirements are unsatisfiable.

          hint: `package-c` was requested with a pre-release marker (e.g., package-c==2.0.0b1), but pre-releases weren't enabled (try: `--prerelease=allow`)
    "###);

    // Since the user did not explicitly opt-in to a prerelease, it cannot be selected.
    assert_not_installed(
        &context.venv,
        "transitive_prerelease_and_stable_dependency_a",
        &context.temp_dir,
    );
    assert_not_installed(
        &context.venv,
        "transitive_prerelease_and_stable_dependency_b",
        &context.temp_dir,
    );
}

/// The user requires a package where recent versions require a Python version greater than the current version, but an older version is compatible.
///
/// ```text
/// python-greater-than-current-backtrack
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
fn python_greater_than_current_backtrack() {
    let context = TestContext::new("3.9");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"python-greater-than-current-backtrack-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("python-greater-than-current-backtrack-a")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + package-a==1.0.0
    "###);

    assert_installed(
        &context.venv,
        "python_greater_than_current_backtrack_a",
        "1.0.0",
        &context.temp_dir,
    );
}

/// The user requires a package where recent versions require a Python version greater than the current version, but an excluded older version is compatible.
///
/// ```text
/// python-greater-than-current-excluded
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
fn python_greater_than_current_excluded() {
    let context = TestContext::new("3.9");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"python-greater-than-current-excluded-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("python-greater-than-current-excluded-a>=2.0.0")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because the current Python version (3.9.[X]) does not satisfy Python>=3.10 and package-a==2.0.0 depends on Python>=3.10, we can conclude that package-a==2.0.0 cannot be used.
          And because only the following versions of package-a are available:
              package-a<=2.0.0
              package-a==3.0.0
              package-a==4.0.0
          we can conclude that package-a>=2.0.0,<3.0.0 cannot be used. (1)

          Because the current Python version (3.9.[X]) does not satisfy Python>=3.11 and package-a==3.0.0 depends on Python>=3.11, we can conclude that package-a==3.0.0 cannot be used.
          And because we know from (1) that package-a>=2.0.0,<3.0.0 cannot be used, we can conclude that package-a>=2.0.0,<4.0.0 cannot be used. (2)

          Because the current Python version (3.9.[X]) does not satisfy Python>=3.12 and package-a==4.0.0 depends on Python>=3.12, we can conclude that package-a==4.0.0 cannot be used.
          And because we know from (2) that package-a>=2.0.0,<4.0.0 cannot be used, we can conclude that package-a>=2.0.0 cannot be used.
          And because you require package-a>=2.0.0, we can conclude that your requirements are unsatisfiable.
    "###);

    assert_not_installed(
        &context.venv,
        "python_greater_than_current_excluded_a",
        &context.temp_dir,
    );
}

/// The user requires a package which has many versions which all require a Python version greater than the current version
///
/// ```text
/// python-greater-than-current-many
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
fn python_greater_than_current_many() {
    let context = TestContext::new("3.9");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"python-greater-than-current-many-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("python-greater-than-current-many-a==1.0.0")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because there is no version of package-a==1.0.0 and you require package-a==1.0.0, we can conclude that your requirements are unsatisfiable.
    "###);

    assert_not_installed(
        &context.venv,
        "python_greater_than_current_many_a",
        &context.temp_dir,
    );
}

/// The user requires a package which requires a Python version with a patch version greater than the current patch version
///
/// ```text
/// python-greater-than-current-patch
/// ├── environment
/// │   └── python3.8.12
/// ├── root
/// │   └── requires a==1.0.0
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
///         └── requires python>=3.8.14 (incompatible with environment)
/// ```
#[cfg(feature = "python-patch")]
#[test]
fn python_greater_than_current_patch() {
    let context = TestContext::new("3.8.12");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"python-greater-than-current-patch-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("python-greater-than-current-patch-a==1.0.0")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because the current Python version (3.8.12) does not satisfy Python>=3.8.14 and package-a==1.0.0 depends on Python>=3.8.14, we can conclude that package-a==1.0.0 cannot be used.
          And because you require package-a==1.0.0, we can conclude that your requirements are unsatisfiable.
    "###);

    assert_not_installed(
        &context.venv,
        "python_greater_than_current_patch_a",
        &context.temp_dir,
    );
}

/// The user requires a package which requires a Python version greater than the current version
///
/// ```text
/// python-greater-than-current
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
fn python_greater_than_current() {
    let context = TestContext::new("3.9");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"python-greater-than-current-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("python-greater-than-current-a==1.0.0")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because the current Python version (3.9.[X]) does not satisfy Python>=3.10 and package-a==1.0.0 depends on Python>=3.10, we can conclude that package-a==1.0.0 cannot be used.
          And because you require package-a==1.0.0, we can conclude that your requirements are unsatisfiable.
    "###);

    assert_not_installed(
        &context.venv,
        "python_greater_than_current_a",
        &context.temp_dir,
    );
}

/// The user requires a package which requires a Python version less than the current version
///
/// ```text
/// python-less-than-current
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
fn python_less_than_current() {
    let context = TestContext::new("3.9");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"python-less-than-current-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("python-less-than-current-a==1.0.0")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + package-a==1.0.0
    "###);

    // We ignore the upper bound on Python requirements
}

/// The user requires a package which requires a Python version that does not exist
///
/// ```text
/// python-version-does-not-exist
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a==1.0.0
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
///         └── requires python>=3.30 (incompatible with environment)
/// ```
#[test]
fn python_version_does_not_exist() {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"python-version-does-not-exist-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("python-version-does-not-exist-a==1.0.0")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because the current Python version (3.8.[X]) does not satisfy Python>=3.30 and package-a==1.0.0 depends on Python>=3.30, we can conclude that package-a==1.0.0 cannot be used.
          And because you require package-a==1.0.0, we can conclude that your requirements are unsatisfiable.
    "###);

    assert_not_installed(
        &context.venv,
        "python_version_does_not_exist_a",
        &context.temp_dir,
    );
}

/// Both wheels and source distributions are available, and the user has disabled binaries.
///
/// ```text
/// no-binary
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

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"no-binary-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("--no-binary")
        .arg("no-binary-a")
        .arg("no-binary-a")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + package-a==1.0.0
    "###);

    // The source distribution should be used for install
}

/// Both wheels and source distributions are available, and the user has disabled builds.
///
/// ```text
/// no-build
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

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"no-build-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("--only-binary")
        .arg("no-build-a")
        .arg("no-build-a")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + package-a==1.0.0
    "###);

    // The wheel should be used for install
}

/// No wheels with matching ABI tags are available, nor are any source distributions available
///
/// ```text
/// no-sdist-no-wheels-with-matching-abi
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

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"no-sdist-no-wheels-with-matching-abi-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("--python-platform=x86_64-manylinux2014")
        .arg("no-sdist-no-wheels-with-matching-abi-a")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only package-a==1.0.0 is available and package-a==1.0.0 has no wheels with a matching Python ABI tag (e.g., `cp38`), we can conclude that all versions of package-a cannot be used.
          And because you require package-a, we can conclude that your requirements are unsatisfiable.

          hint: You require CPython 3.8 (`cp38`), but we only found wheels for `package-a` (v1.0.0) with the following Python ABI tag: `graalpy240_310_native`
    "###);

    assert_not_installed(
        &context.venv,
        "no_sdist_no_wheels_with_matching_abi_a",
        &context.temp_dir,
    );
}

/// No wheels with matching platform tags are available, nor are any source distributions available
///
/// ```text
/// no-sdist-no-wheels-with-matching-platform
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

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"no-sdist-no-wheels-with-matching-platform-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("--python-platform=x86_64-manylinux2014")
        .arg("no-sdist-no-wheels-with-matching-platform-a")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only package-a==1.0.0 is available and package-a==1.0.0 has no wheels with a matching platform tag (e.g., `manylinux_2_17_x86_64`), we can conclude that all versions of package-a cannot be used.
          And because you require package-a, we can conclude that your requirements are unsatisfiable.

          hint: Wheels are available for `package-a` (v1.0.0) on the following platform: `macosx_10_0_ppc64`
    "###);

    assert_not_installed(
        &context.venv,
        "no_sdist_no_wheels_with_matching_platform_a",
        &context.temp_dir,
    );
}

/// No wheels with matching Python tags are available, nor are any source distributions available
///
/// ```text
/// no-sdist-no-wheels-with-matching-python
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

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"no-sdist-no-wheels-with-matching-python-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("--python-platform=x86_64-manylinux2014")
        .arg("no-sdist-no-wheels-with-matching-python-a")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only package-a==1.0.0 is available and package-a==1.0.0 has no wheels with a matching Python implementation tag (e.g., `cp38`), we can conclude that all versions of package-a cannot be used.
          And because you require package-a, we can conclude that your requirements are unsatisfiable.

          hint: You require CPython 3.8 (`cp38`), but we only found wheels for `package-a` (v1.0.0) with the following Python implementation tag: `graalpy310`
    "###);

    assert_not_installed(
        &context.venv,
        "no_sdist_no_wheels_with_matching_python_a",
        &context.temp_dir,
    );
}

/// No wheels are available, only source distributions but the user has disabled builds.
///
/// ```text
/// no-wheels-no-build
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

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"no-wheels-no-build-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("--only-binary")
        .arg("no-wheels-no-build-a")
        .arg("no-wheels-no-build-a")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only package-a==1.0.0 is available and package-a==1.0.0 has no usable wheels, we can conclude that all versions of package-a cannot be used.
          And because you require package-a, we can conclude that your requirements are unsatisfiable.

          hint: Wheels are required for `package-a` because building from source is disabled for `package-a` (i.e., with `--no-build-package package-a`)
    "###);

    assert_not_installed(&context.venv, "no_wheels_no_build_a", &context.temp_dir);
}

/// No wheels with matching platform tags are available, just source distributions.
///
/// ```text
/// no-wheels-with-matching-platform
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

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"no-wheels-with-matching-platform-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("no-wheels-with-matching-platform-a")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + package-a==1.0.0
    "###);
}

/// No wheels are available, only source distributions.
///
/// ```text
/// no-wheels
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

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"no-wheels-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("no-wheels-a")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + package-a==1.0.0
    "###);
}

/// No source distributions are available, only wheels but the user has disabled using pre-built binaries.
///
/// ```text
/// only-wheels-no-binary
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

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"only-wheels-no-binary-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("--no-binary")
        .arg("only-wheels-no-binary-a")
        .arg("only-wheels-no-binary-a")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only package-a==1.0.0 is available and package-a==1.0.0 has no source distribution, we can conclude that all versions of package-a cannot be used.
          And because you require package-a, we can conclude that your requirements are unsatisfiable.

          hint: A source distribution is required for `package-a` because using pre-built wheels is disabled for `package-a` (i.e., with `--no-binary-package package-a`)
    "###);

    assert_not_installed(&context.venv, "only_wheels_no_binary_a", &context.temp_dir);
}

/// No source distributions are available, only wheels.
///
/// ```text
/// only-wheels
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

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"only-wheels-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("only-wheels-a")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + package-a==1.0.0
    "###);
}

/// A wheel for a specific platform is available alongside the default.
///
/// ```text
/// specific-tag-and-default
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

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"specific-tag-and-default-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("specific-tag-and-default-a")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + package-a==1.0.0
    "###);
}

/// The user requires a version of package `a` which only matches yanked versions.
///
/// ```text
/// package-only-yanked-in-range
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

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"package-only-yanked-in-range-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("package-only-yanked-in-range-a>0.1.0")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only the following versions of package-a are available:
              package-a<0.1.0
              package-a==1.0.0
          and package-a==1.0.0 was yanked (reason: Yanked for testing), we can conclude that package-a>0.1.0 cannot be used.
          And because you require package-a>0.1.0, we can conclude that your requirements are unsatisfiable.
    "###);

    // Since there are other versions of `a` available, yanked versions should not be selected without explicit opt-in.
    assert_not_installed(
        &context.venv,
        "package_only_yanked_in_range_a",
        &context.temp_dir,
    );
}

/// The user requires any version of package `a` which only has yanked versions available.
///
/// ```text
/// package-only-yanked
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

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"package-only-yanked-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("package-only-yanked-a")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only package-a==1.0.0 is available and package-a==1.0.0 was yanked (reason: Yanked for testing), we can conclude that all versions of package-a cannot be used.
          And because you require package-a, we can conclude that your requirements are unsatisfiable.
    "###);

    // Yanked versions should not be installed, even if they are the only one available.
    assert_not_installed(&context.venv, "package_only_yanked_a", &context.temp_dir);
}

/// The user requires any version of `a` and both yanked and unyanked releases are available.
///
/// ```text
/// package-yanked-specified-mixed-available
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

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"package-yanked-specified-mixed-available-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("package-yanked-specified-mixed-available-a>=0.1.0")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + package-a==0.3.0
    "###);

    // The latest unyanked version should be selected.
    assert_installed(
        &context.venv,
        "package_yanked_specified_mixed_available_a",
        "0.3.0",
        &context.temp_dir,
    );
}

/// The user requires any version of package `a` has a yanked version available and an older unyanked version.
///
/// ```text
/// requires-package-yanked-and-unyanked-any
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

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"requires-package-yanked-and-unyanked-any-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("requires-package-yanked-and-unyanked-any-a")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + package-a==0.1.0
    "###);

    // The unyanked version should be selected.
    assert_installed(
        &context.venv,
        "requires_package_yanked_and_unyanked_any_a",
        "0.1.0",
        &context.temp_dir,
    );
}

/// The user requires package `a` which has a dependency on a package which only matches yanked versions; the user has opted into allowing the yanked version of `b` explicitly.
///
/// ```text
/// transitive-package-only-yanked-in-range-opt-in
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

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((
        r"transitive-package-only-yanked-in-range-opt-in-",
        "package-",
    ));

    uv_snapshot!(filters, command(&context)
        .arg("transitive-package-only-yanked-in-range-opt-in-a")
                .arg("transitive-package-only-yanked-in-range-opt-in-b==1.0.0")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + package-a==0.1.0
     + package-b==1.0.0
    warning: `package-b==1.0.0` is yanked (reason: "Yanked for testing")
    "###);

    // Since the user included a dependency on `b` with an exact specifier, the yanked version can be selected.
    assert_installed(
        &context.venv,
        "transitive_package_only_yanked_in_range_opt_in_a",
        "0.1.0",
        &context.temp_dir,
    );
    assert_installed(
        &context.venv,
        "transitive_package_only_yanked_in_range_opt_in_b",
        "1.0.0",
        &context.temp_dir,
    );
}

/// The user requires package `a` which has a dependency on a package which only matches yanked versions.
///
/// ```text
/// transitive-package-only-yanked-in-range
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

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"transitive-package-only-yanked-in-range-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("transitive-package-only-yanked-in-range-a")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only the following versions of package-b are available:
              package-b<0.1
              package-b==1.0.0
          and package-b==1.0.0 was yanked (reason: Yanked for testing), we can conclude that package-b>0.1 cannot be used.
          And because package-a==0.1.0 depends on package-b>0.1, we can conclude that package-a==0.1.0 cannot be used.
          And because only package-a==0.1.0 is available and you require package-a, we can conclude that your requirements are unsatisfiable.
    "###);

    // Yanked versions should not be installed, even if they are the only valid version in a range.
    assert_not_installed(
        &context.venv,
        "transitive_package_only_yanked_in_range_a",
        &context.temp_dir,
    );
}

/// The user requires any version of package `a` which requires `b` which only has yanked versions available.
///
/// ```text
/// transitive-package-only-yanked
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

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"transitive-package-only-yanked-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("transitive-package-only-yanked-a")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only package-b==1.0.0 is available and package-b==1.0.0 was yanked (reason: Yanked for testing), we can conclude that all versions of package-b cannot be used.
          And because package-a==0.1.0 depends on package-b, we can conclude that package-a==0.1.0 cannot be used.
          And because only package-a==0.1.0 is available and you require package-a, we can conclude that your requirements are unsatisfiable.
    "###);

    // Yanked versions should not be installed, even if they are the only one available.
    assert_not_installed(
        &context.venv,
        "transitive_package_only_yanked_a",
        &context.temp_dir,
    );
}

/// A transitive dependency has both a yanked and an unyanked version, but can only be satisfied by a yanked. The user includes an opt-in to the yanked version of the transitive dependency.
///
/// ```text
/// transitive-yanked-and-unyanked-dependency-opt-in
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

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((
        r"transitive-yanked-and-unyanked-dependency-opt-in-",
        "package-",
    ));

    uv_snapshot!(filters, command(&context)
        .arg("transitive-yanked-and-unyanked-dependency-opt-in-a")
                .arg("transitive-yanked-and-unyanked-dependency-opt-in-b")
                .arg("transitive-yanked-and-unyanked-dependency-opt-in-c==2.0.0")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + package-a==1.0.0
     + package-b==1.0.0
     + package-c==2.0.0
    warning: `package-c==2.0.0` is yanked (reason: "Yanked for testing")
    "###);

    // Since the user explicitly selected the yanked version of `c`, it can be installed.
    assert_installed(
        &context.venv,
        "transitive_yanked_and_unyanked_dependency_opt_in_a",
        "1.0.0",
        &context.temp_dir,
    );
    assert_installed(
        &context.venv,
        "transitive_yanked_and_unyanked_dependency_opt_in_b",
        "1.0.0",
        &context.temp_dir,
    );
    assert_installed(
        &context.venv,
        "transitive_yanked_and_unyanked_dependency_opt_in_c",
        "2.0.0",
        &context.temp_dir,
    );
}

/// A transitive dependency has both a yanked and an unyanked version, but can only be satisfied by a yanked version
///
/// ```text
/// transitive-yanked-and-unyanked-dependency
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

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"transitive-yanked-and-unyanked-dependency-", "package-"));

    uv_snapshot!(filters, command(&context)
        .arg("transitive-yanked-and-unyanked-dependency-a")
                .arg("transitive-yanked-and-unyanked-dependency-b")
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because package-c==2.0.0 was yanked (reason: Yanked for testing) and package-a==1.0.0 depends on package-c==2.0.0, we can conclude that package-a==1.0.0 cannot be used.
          And because only package-a==1.0.0 is available and you require package-a, we can conclude that your requirements are unsatisfiable.
    "###);

    // Since the user did not explicitly select the yanked version, it cannot be used.
    assert_not_installed(
        &context.venv,
        "transitive_yanked_and_unyanked_dependency_a",
        &context.temp_dir,
    );
    assert_not_installed(
        &context.venv,
        "transitive_yanked_and_unyanked_dependency_b",
        &context.temp_dir,
    );
}
