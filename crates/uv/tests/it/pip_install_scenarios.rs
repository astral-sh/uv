//! DO NOT EDIT
//!
//! Generated with `uv run scripts/scenarios/generate.py`
//! Scenarios from <test/scenarios>
//!
#![cfg(all(feature = "test-python", unix))]

use std::process::Command;

use uv_static::EnvVars;

use uv_test::packse::PackseServer;
use uv_test::{TestContext, uv_snapshot};

/// Create a `pip install` command with options shared across all scenarios.
fn command(context: &TestContext, server: &PackseServer) -> Command {
    let mut command = context.pip_install();
    command.arg("--index-url").arg(server.index_url());
    command.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    command
}

/// There are two packages, `a` and `b`. All versions of `b` require a specific
/// version of `a`, but that version requires a package `c` that does not exist. The resolver
/// must backtrack through all versions of `b` and eventually fail because no solution exists.
///
/// ```text
/// backtrack-to-missing-package
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   ├── requires a
/// │   │   ├── satisfied by a-2.0.0
/// │   │   └── satisfied by a-1.0.0
/// │   └── requires b
/// │       ├── satisfied by b-1.0.0
/// │       ├── satisfied by b-2.0.0
/// │       └── satisfied by b-3.0.0
/// ├── a
/// │   ├── a-2.0.0
/// │   └── a-1.0.0
/// │       └── requires c
/// │           └── unsatisfied: no versions for package
/// └── b
///     ├── b-1.0.0
///     │   └── requires a==1.0.0
///     │       └── satisfied by a-1.0.0
///     ├── b-2.0.0
///     │   └── requires a==1.0.0
///     │       └── satisfied by a-1.0.0
///     └── b-3.0.0
///         └── requires a==1.0.0
///             └── satisfied by a-1.0.0
/// ```
#[test]
fn backtrack_to_missing_package() {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("backtracking/backtrack-to-missing-package.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a")
                .arg("b")
        , @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because c was not found in the package registry and a==1.0.0 depends on c, we can conclude that a==1.0.0 cannot be used.
          And because all versions of b depend on a==1.0.0 and you require b, we can conclude that your requirements are unsatisfiable.
    ");

    context.assert_not_installed("a");
    context.assert_not_installed("b");
}

/// There are two packages, `a` and `b`. The latest version of `b` requires
/// a specific version of `a`. The older version of `b` requires a package `c` that does not
/// exist. The resolver should backtrack on `a` (not `b`) to find a solution without needing
/// to try `b==1.0.0` which would fail due to the missing package.
///
/// ```text
/// backtrack-with-missing-package
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   ├── requires a
/// │   │   ├── satisfied by a-1.0.0
/// │   │   └── satisfied by a-2.0.0
/// │   └── requires b
/// │       ├── satisfied by b-1.0.0
/// │       └── satisfied by b-2.0.0
/// ├── a
/// │   ├── a-1.0.0
/// │   └── a-2.0.0
/// └── b
///     ├── b-1.0.0
///     │   └── requires c
///     │       └── unsatisfied: no versions for package
///     └── b-2.0.0
///         └── requires a==1.0.0
///             └── satisfied by a-1.0.0
/// ```
#[test]
fn backtrack_with_missing_package() {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("backtracking/backtrack-with-missing-package.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a")
                .arg("b")
        , @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + a==1.0.0
     + b==2.0.0
    ");

    context.assert_installed("a", "1.0.0");
    context.assert_installed("b", "2.0.0");
}

/// The user requires an exact version of package `a` but only other versions exist
///
/// ```text
/// requires-exact-version-does-not-exist
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   └── requires a==2.0.0
/// │       └── unsatisfied: no matching version
/// └── a
///     └── a-1.0.0
/// ```
#[test]
fn requires_exact_version_does_not_exist() {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("does_not_exist/requires-exact-version-does-not-exist.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a==2.0.0")
        , @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because there is no version of a==2.0.0 and you require a==2.0.0, we can conclude that your requirements are unsatisfiable.
    ");

    context.assert_not_installed("a");
}

/// The user requires a version of `a` greater than `1.0.0` but only smaller or equal versions exist
///
/// ```text
/// requires-greater-version-does-not-exist
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   └── requires a>1.0.0
/// │       └── unsatisfied: no matching version
/// └── a
///     ├── a-0.1.0
///     └── a-1.0.0
/// ```
#[test]
fn requires_greater_version_does_not_exist() {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("does_not_exist/requires-greater-version-does-not-exist.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a>1.0.0")
        , @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only a<=1.0.0 is available and you require a>1.0.0, we can conclude that your requirements are unsatisfiable.
    ");

    context.assert_not_installed("a");
}

/// The user requires a version of `a` less than `1.0.0` but only larger versions exist
///
/// ```text
/// requires-less-version-does-not-exist
/// ├── environment
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("does_not_exist/requires-less-version-does-not-exist.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a<2.0.0")
        , @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only a>=2.0.0 is available and you require a<2.0.0, we can conclude that your requirements are unsatisfiable.
    ");

    context.assert_not_installed("a");
}

/// The user requires any version of package `a` which does not exist.
///
/// ```text
/// requires-package-does-not-exist
/// ├── environment
/// │   └── python3.12
/// └── root
///     └── requires a
///         └── unsatisfied: no versions for package
/// ```
#[test]
fn requires_package_does_not_exist() {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("does_not_exist/requires-package-does-not-exist.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a")
        , @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because a was not found in the package registry and you require a, we can conclude that your requirements are unsatisfiable.
    ");

    context.assert_not_installed("a");
}

/// The user requires package `a` but `a` requires package `b` which does not exist
///
/// ```text
/// transitive-requires-package-does-not-exist
/// ├── environment
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server =
        PackseServer::new("does_not_exist/transitive-requires-package-does-not-exist.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a")
        , @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because b was not found in the package registry and a==1.0.0 depends on b, we can conclude that a==1.0.0 cannot be used.
          And because only a==1.0.0 is available and you require a, we can conclude that your requirements are unsatisfiable.
    ");

    context.assert_not_installed("a");
}

/// There is a non-contiguous range of compatible versions for the requested package `a`, but another dependency `c` excludes the range. This is the same as `dependency-excludes-range-of-compatible-versions` but some of the versions of `a` are incompatible for another reason e.g. dependency on non-existent package `d`.
///
/// ```text
/// dependency-excludes-non-contiguous-range-of-compatible-versions
/// ├── environment
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new(
        "excluded/dependency-excludes-non-contiguous-range-of-compatible-versions.toml",
    );

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a")
                .arg("b>=2.0.0,<3.0.0")
                .arg("c")
        , @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because a==1.0.0 depends on b==1.0.0 and only the following versions of a are available:
              a==1.0.0
              a>=2.0.0
          we can conclude that a<2.0.0 depends on b==1.0.0.
          And because only a<=3.0.0 is available, we can conclude that a<2.0.0 depends on b==1.0.0. (1)

          Because only the following versions of c are available:
              c==1.0.0
              c==2.0.0
          and c==1.0.0 depends on a<2.0.0, we can conclude that c<2.0.0 depends on a<2.0.0.
          And because c==2.0.0 depends on a>=3.0.0, we can conclude that all versions of c depend on one of:
              a<2.0.0
              a>=3.0.0

          And because we know from (1) that a<2.0.0 depends on b==1.0.0, we can conclude that a!=3.0.0, b!=1.0.0, all versions of c are incompatible.
          And because a==3.0.0 depends on b==3.0.0, we can conclude that all versions of c depend on one of:
              b<=1.0.0
              b>=3.0.0

          And because you require b>=2.0.0,<3.0.0 and c, we can conclude that your requirements are unsatisfiable.
    ");

    // Only the `2.x` versions of `a` are available since `a==1.0.0` and `a==3.0.0` require incompatible versions of `b`, but all available versions of `c` exclude that range of `a` so resolution fails.
    context.assert_not_installed("a");
    context.assert_not_installed("b");
    context.assert_not_installed("c");
}

/// There is a range of compatible versions for the requested package `a`, but another dependency `c` excludes that range.
///
/// ```text
/// dependency-excludes-range-of-compatible-versions
/// ├── environment
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server =
        PackseServer::new("excluded/dependency-excludes-range-of-compatible-versions.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a")
                .arg("b>=2.0.0,<3.0.0")
                .arg("c")
        , @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because a==1.0.0 depends on b==1.0.0 and only the following versions of a are available:
              a==1.0.0
              a>=2.0.0
          we can conclude that a<2.0.0 depends on b==1.0.0.
          And because only a<=3.0.0 is available, we can conclude that a<2.0.0 depends on b==1.0.0. (1)

          Because only the following versions of c are available:
              c==1.0.0
              c==2.0.0
          and c==1.0.0 depends on a<2.0.0, we can conclude that c<2.0.0 depends on a<2.0.0.
          And because c==2.0.0 depends on a>=3.0.0, we can conclude that all versions of c depend on one of:
              a<2.0.0
              a>=3.0.0

          And because we know from (1) that a<2.0.0 depends on b==1.0.0, we can conclude that a!=3.0.0, b!=1.0.0, all versions of c are incompatible.
          And because a==3.0.0 depends on b==3.0.0, we can conclude that all versions of c depend on one of:
              b<=1.0.0
              b>=3.0.0

          And because you require b>=2.0.0,<3.0.0 and c, we can conclude that your requirements are unsatisfiable.
    ");

    // Only the `2.x` versions of `a` are available since `a==1.0.0` and `a==3.0.0` require incompatible versions of `b`, but all available versions of `c` exclude that range of `a` so resolution fails.
    context.assert_not_installed("a");
    context.assert_not_installed("b");
    context.assert_not_installed("c");
}

/// Only one version of the requested package `a` is compatible, but the user has banned that version.
///
/// ```text
/// excluded-only-compatible-version
/// ├── environment
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("excluded/excluded-only-compatible-version.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a!=2.0.0")
                .arg("b>=2.0.0,<3.0.0")
        , @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only the following versions of a are available:
              a==1.0.0
              a==2.0.0
              a==3.0.0
          and a==1.0.0 depends on b==1.0.0, we can conclude that a<2.0.0 depends on b==1.0.0.
          And because a==3.0.0 depends on b==3.0.0, we can conclude that all of:
              a<2.0.0
              a>2.0.0
          depend on one of:
              b==1.0.0
              b==3.0.0

          And because you require one of:
              a<2.0.0
              a>2.0.0
          and b>=2.0.0,<3.0.0, we can conclude that your requirements are unsatisfiable.
    ");

    // Only `a==1.2.0` is available since `a==1.0.0` and `a==3.0.0` require incompatible versions of `b`. The user has excluded that version of `a` so resolution fails.
    context.assert_not_installed("a");
    context.assert_not_installed("b");
}

/// Only one version of the requested package is available, but the user has banned that version.
///
/// ```text
/// excluded-only-version
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   └── requires a!=1.0.0
/// │       └── unsatisfied: no matching version
/// └── a
///     └── a-1.0.0
/// ```
#[test]
fn excluded_only_version() {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("excluded/excluded-only-version.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a!=1.0.0")
        , @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only a==1.0.0 is available and you require one of:
              a<1.0.0
              a>1.0.0
          we can conclude that your requirements are unsatisfiable.
    ");

    // Only `a==1.0.0` is available but the user excluded it.
    context.assert_not_installed("a");
}

/// Multiple optional dependencies are requested for the package via an 'all' extra.
///
/// ```text
/// all-extras-required
/// ├── environment
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("extras/all-extras-required.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a[all]")
        , @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + a==1.0.0
     + b==1.0.0
     + c==1.0.0
    ");

    context.assert_installed("a", "1.0.0");
    context.assert_installed("b", "1.0.0");
    context.assert_installed("c", "1.0.0");
}

/// Optional dependencies are requested for the package, the extra is only available on an older version.
///
/// ```text
/// extra-does-not-exist-backtrack
/// ├── environment
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("extras/extra-does-not-exist-backtrack.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a[extra]")
        , @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + a==3.0.0
    warning: The package `a==3.0.0` does not have an extra named `extra`
    ");

    // The resolver should not backtrack to `a==1.0.0` because missing extras are allowed during resolution. `b` should not be installed.
    context.assert_installed("a", "3.0.0");
}

/// One of two incompatible optional dependencies are requested for the package.
///
/// ```text
/// extra-incompatible-with-extra-not-requested
/// ├── environment
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("extras/extra-incompatible-with-extra-not-requested.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a[extra_c]")
        , @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + a==1.0.0
     + b==2.0.0
    ");

    // Because the user does not request both extras, it is okay that one is incompatible with the other.
    context.assert_installed("a", "1.0.0");
    context.assert_installed("b", "2.0.0");
}

/// Multiple optional dependencies are requested for the package, but they have conflicting requirements with each other.
///
/// ```text
/// extra-incompatible-with-extra
/// ├── environment
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("extras/extra-incompatible-with-extra.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a[extra_b,extra_c]")
        , @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only a[extra-b]==1.0.0 is available and a[extra-b]==1.0.0 depends on b==1.0.0, we can conclude that all versions of a[extra-b] depend on b==1.0.0.
          And because a[extra-c]==1.0.0 depends on b==2.0.0 and only a[extra-c]==1.0.0 is available, we can conclude that all versions of a[extra-b] and all versions of a[extra-c] are incompatible.
          And because you require a[extra-b] and a[extra-c], we can conclude that your requirements are unsatisfiable.
    ");

    // Because both `extra_b` and `extra_c` are requested and they require incompatible versions of `b`, `a` cannot be installed.
    context.assert_not_installed("a");
}

/// Optional dependencies are requested for the package, but the extra is not compatible with other requested versions.
///
/// ```text
/// extra-incompatible-with-root
/// ├── environment
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("extras/extra-incompatible-with-root.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a[extra]")
                .arg("b==2.0.0")
        , @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only a[extra]==1.0.0 is available and a[extra]==1.0.0 depends on b==1.0.0, we can conclude that all versions of a[extra] depend on b==1.0.0.
          And because you require a[extra] and b==2.0.0, we can conclude that your requirements are unsatisfiable.
    ");

    // Because the user requested `b==2.0.0` but the requested extra requires `b==1.0.0`, the dependencies cannot be satisfied.
    context.assert_not_installed("a");
    context.assert_not_installed("b");
}

/// Optional dependencies are requested for the package.
///
/// ```text
/// extra-required
/// ├── environment
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("extras/extra-required.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a[extra]")
        , @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + a==1.0.0
     + b==1.0.0
    ");

    context.assert_installed("a", "1.0.0");
    context.assert_installed("b", "1.0.0");
}

/// Optional dependencies are requested for the package, but the extra does not exist.
///
/// ```text
/// missing-extra
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   └── requires a[extra]
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
/// ```
#[test]
fn missing_extra() {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("extras/missing-extra.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a[extra]")
        , @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + a==1.0.0
    warning: The package `a==1.0.0` does not have an extra named `extra`
    ");

    // Missing extras are ignored during resolution.
    context.assert_installed("a", "1.0.0");
}

/// Multiple optional dependencies are requested for the package.
///
/// ```text
/// multiple-extras-required
/// ├── environment
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("extras/multiple-extras-required.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a[extra_b,extra_c]")
        , @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + a==1.0.0
     + b==1.0.0
     + c==1.0.0
    ");

    context.assert_installed("a", "1.0.0");
    context.assert_installed("b", "1.0.0");
    context.assert_installed("c", "1.0.0");
}

/// The user requires two incompatible, existing versions of package `a`
///
/// ```text
/// direct-incompatible-versions
/// ├── environment
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("incompatible_versions/direct-incompatible-versions.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a==1.0.0")
                .arg("a==2.0.0")
        , @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because you require a==1.0.0 and a==2.0.0, we can conclude that your requirements are unsatisfiable.
    ");

    context.assert_not_installed("a");
    context.assert_not_installed("a");
}

/// The user requires `a`, which requires two incompatible, existing versions of package `b`
///
/// ```text
/// transitive-incompatible-versions
/// ├── environment
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("incompatible_versions/transitive-incompatible-versions.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a==1.0.0")
        , @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because a==1.0.0 depends on b==2.0.0 and b==1.0.0, we can conclude that a==1.0.0 cannot be used.
          And because you require a==1.0.0, we can conclude that your requirements are unsatisfiable.
    ");

    context.assert_not_installed("a");
}

/// The user requires packages `a` and `b` but `a` requires a different version of `b`
///
/// ```text
/// transitive-incompatible-with-root-version
/// ├── environment
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server =
        PackseServer::new("incompatible_versions/transitive-incompatible-with-root-version.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a")
                .arg("b==1.0.0")
        , @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only a==1.0.0 is available and a==1.0.0 depends on b==2.0.0, we can conclude that all versions of a depend on b==2.0.0.
          And because you require a and b==1.0.0, we can conclude that your requirements are unsatisfiable.
    ");

    context.assert_not_installed("a");
    context.assert_not_installed("b");
}

/// The user requires package `a` and `b`; `a` and `b` require different versions of `c`
///
/// ```text
/// transitive-incompatible-with-transitive
/// ├── environment
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server =
        PackseServer::new("incompatible_versions/transitive-incompatible-with-transitive.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a")
                .arg("b")
        , @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only a==1.0.0 is available and a==1.0.0 depends on c==1.0.0, we can conclude that all versions of a depend on c==1.0.0.
          And because b==1.0.0 depends on c==2.0.0 and only b==1.0.0 is available, we can conclude that all versions of a and all versions of b are incompatible.
          And because you require a and b, we can conclude that your requirements are unsatisfiable.
    ");

    context.assert_not_installed("a");
    context.assert_not_installed("b");
}

/// A local version should be included in inclusive ordered comparisons.
///
/// ```text
/// local-greater-than-or-equal
/// ├── environment
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("local/local-greater-than-or-equal.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a>=1.2.3")
        , @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + a==1.2.3+foo
    ");

    // The version '1.2.3+foo' satisfies the constraint '>=1.2.3'.
    context.assert_installed("a", "1.2.3+foo");
}

/// A local version should be excluded in exclusive ordered comparisons.
///
/// ```text
/// local-greater-than
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   └── requires a>1.2.3
/// │       └── unsatisfied: no matching version
/// └── a
///     └── a-1.2.3+foo
/// ```
#[test]
fn local_greater_than() {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("local/local-greater-than.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a>1.2.3")
        , @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only a==1.2.3+foo is available and you require a>1.2.3, we can conclude that your requirements are unsatisfiable.
    ");

    context.assert_not_installed("a");
}

/// A local version should be included in inclusive ordered comparisons.
///
/// ```text
/// local-less-than-or-equal
/// ├── environment
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("local/local-less-than-or-equal.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a<=1.2.3")
        , @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + a==1.2.3+foo
    ");

    // The version '1.2.3+foo' satisfies the constraint '<=1.2.3'.
    context.assert_installed("a", "1.2.3+foo");
}

/// A local version should be excluded in exclusive ordered comparisons.
///
/// ```text
/// local-less-than
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   └── requires a<1.2.3
/// │       └── unsatisfied: no matching version
/// └── a
///     └── a-1.2.3+foo
/// ```
#[test]
fn local_less_than() {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("local/local-less-than.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a<1.2.3")
        , @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only a==1.2.3+foo is available and you require a<1.2.3, we can conclude that your requirements are unsatisfiable.
    ");

    context.assert_not_installed("a");
}

/// Tests that we can select an older version with a local segment when newer versions are incompatible.
///
/// ```text
/// local-not-latest
/// ├── environment
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("local/local-not-latest.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a>=1")
        , @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + a==1.2.1+foo
    ");

    context.assert_installed("a", "1.2.1+foo");
}

/// If there is a 1.2.3 version with an sdist published and no compatible wheels, then the sdist will be used.
///
/// ```text
/// local-not-used-with-sdist
/// ├── environment
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("local/local-not-used-with-sdist.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a==1.2.3")
        , @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + a==1.2.3+foo
    ");

    // The version '1.2.3' with an sdist satisfies the constraint '==1.2.3'.
    context.assert_installed("a", "1.2.3+foo");
}

/// A simple version constraint should not exclude published versions with local segments.
///
/// ```text
/// local-simple
/// ├── environment
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("local/local-simple.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a==1.2.3")
        , @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + a==1.2.3+foo
    ");

    // The version '1.2.3+foo' satisfies the constraint '==1.2.3'.
    context.assert_installed("a", "1.2.3+foo");
}

/// A dependency depends on a conflicting local version of a direct dependency, but we can backtrack to a compatible version.
///
/// ```text
/// local-transitive-backtrack
/// ├── environment
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("local/local-transitive-backtrack.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a")
                .arg("b==2.0.0+foo")
        , @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + a==1.0.0
     + b==2.0.0+foo
    ");

    // Backtracking to '1.0.0' gives us compatible local versions of b.
    context.assert_installed("a", "1.0.0");
    context.assert_installed("b", "2.0.0+foo");
}

/// A dependency depends on a conflicting local version of a direct dependency.
///
/// ```text
/// local-transitive-conflicting
/// ├── environment
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("local/local-transitive-conflicting.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a")
                .arg("b==2.0.0+foo")
        , @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only a==1.0.0 is available and a==1.0.0 depends on b==2.0.0+bar, we can conclude that all versions of a depend on b==2.0.0+bar.
          And because you require a and b==2.0.0+foo, we can conclude that your requirements are unsatisfiable.
    ");

    context.assert_not_installed("a");
    context.assert_not_installed("b");
}

/// A transitive dependency has both a non-local and local version published, but the non-local version is unusable.
///
/// ```text
/// local-transitive-confounding
/// ├── environment
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("local/local-transitive-confounding.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a")
        , @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + a==1.0.0
     + b==2.0.0+foo
    ");

    // The version '2.0.0+foo' satisfies the constraint '==2.0.0'.
    context.assert_installed("a", "1.0.0");
    context.assert_installed("b", "2.0.0+foo");
}

/// A transitive constraint on a local version should match an inclusive ordered operator.
///
/// ```text
/// local-transitive-greater-than-or-equal
/// ├── environment
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("local/local-transitive-greater-than-or-equal.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a")
                .arg("b==2.0.0+foo")
        , @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + a==1.0.0
     + b==2.0.0+foo
    ");

    // The version '2.0.0+foo' satisfies both >=2.0.0 and ==2.0.0+foo.
    context.assert_installed("a", "1.0.0");
    context.assert_installed("b", "2.0.0+foo");
}

/// A transitive constraint on a local version should not match an exclusive ordered operator.
///
/// ```text
/// local-transitive-greater-than
/// ├── environment
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("local/local-transitive-greater-than.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a")
                .arg("b==2.0.0+foo")
        , @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only a==1.0.0 is available and a==1.0.0 depends on b>2.0.0, we can conclude that all versions of a depend on b>2.0.0.
          And because you require a and b==2.0.0+foo, we can conclude that your requirements are unsatisfiable.
    ");

    context.assert_not_installed("a");
    context.assert_not_installed("b");
}

/// A transitive constraint on a local version should match an inclusive ordered operator.
///
/// ```text
/// local-transitive-less-than-or-equal
/// ├── environment
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("local/local-transitive-less-than-or-equal.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a")
                .arg("b==2.0.0+foo")
        , @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + a==1.0.0
     + b==2.0.0+foo
    ");

    // The version '2.0.0+foo' satisfies both <=2.0.0 and ==2.0.0+foo.
    context.assert_installed("a", "1.0.0");
    context.assert_installed("b", "2.0.0+foo");
}

/// A transitive constraint on a local version should not match an exclusive ordered operator.
///
/// ```text
/// local-transitive-less-than
/// ├── environment
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("local/local-transitive-less-than.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a")
                .arg("b==2.0.0+foo")
        , @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only a==1.0.0 is available and a==1.0.0 depends on b<2.0.0, we can conclude that all versions of a depend on b<2.0.0.
          And because you require a and b==2.0.0+foo, we can conclude that your requirements are unsatisfiable.
    ");

    context.assert_not_installed("a");
    context.assert_not_installed("b");
}

/// A simple version constraint should not exclude published versions with local segments.
///
/// ```text
/// local-transitive
/// ├── environment
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("local/local-transitive.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a")
                .arg("b==2.0.0+foo")
        , @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + a==1.0.0
     + b==2.0.0+foo
    ");

    // The version '2.0.0+foo' satisfies both ==2.0.0 and ==2.0.0+foo.
    context.assert_installed("a", "1.0.0");
    context.assert_installed("b", "2.0.0+foo");
}

/// Even if there is a 1.2.3 version published, if it is unavailable for some reason (no sdist and no compatible wheels in this case), a 1.2.3 version with a local segment should be usable instead.
///
/// ```text
/// local-used-without-sdist
/// ├── environment
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("local/local-used-without-sdist.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a==1.2.3")
        , @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + a==1.2.3+foo
    ");

    // The version '1.2.3+foo' satisfies the constraint '==1.2.3'.
    context.assert_installed("a", "1.2.3+foo");
}

/// An equal version constraint should match a post-release version if the post-release version is available.
///
/// ```text
/// post-equal-available
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   └── requires a==1.2.3.post0
/// │       └── satisfied by a-1.2.3.post0
/// └── a
///     ├── a-1.2.3
///     └── a-1.2.3.post0
/// ```
#[test]
fn post_equal_available() {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("post/post-equal-available.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a==1.2.3.post0")
        , @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + a==1.2.3.post0
    ");

    // The version '1.2.3.post0' satisfies the constraint '==1.2.3.post0'.
    context.assert_installed("a", "1.2.3.post0");
}

/// An equal version constraint should not match a post-release version if the post-release version is not available.
///
/// ```text
/// post-equal-not-available
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   └── requires a==1.2.3.post0
/// │       └── unsatisfied: no matching version
/// └── a
///     ├── a-1.2.3
///     └── a-1.2.3.post1
/// ```
#[test]
fn post_equal_not_available() {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("post/post-equal-not-available.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a==1.2.3.post0")
        , @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because there is no version of a==1.2.3.post0 and you require a==1.2.3.post0, we can conclude that your requirements are unsatisfiable.
    ");

    context.assert_not_installed("a");
}

/// A greater-than-or-equal version constraint should match a post-release version if the constraint is itself a post-release version.
///
/// ```text
/// post-greater-than-or-equal-post
/// ├── environment
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("post/post-greater-than-or-equal-post.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a>=1.2.3.post0")
        , @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + a==1.2.3.post1
    ");

    // The version '1.2.3.post1' satisfies the constraint '>=1.2.3.post0'.
    context.assert_installed("a", "1.2.3.post1");
}

/// A greater-than-or-equal version constraint should match a post-release version.
///
/// ```text
/// post-greater-than-or-equal
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   └── requires a>=1.2.3
/// │       └── satisfied by a-1.2.3.post1
/// └── a
///     └── a-1.2.3.post1
/// ```
#[test]
fn post_greater_than_or_equal() {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("post/post-greater-than-or-equal.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a>=1.2.3")
        , @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + a==1.2.3.post1
    ");

    // The version '1.2.3.post1' satisfies the constraint '>=1.2.3'.
    context.assert_installed("a", "1.2.3.post1");
}

/// A greater-than version constraint should not match a post-release version if the post-release version is not available.
///
/// ```text
/// post-greater-than-post-not-available
/// ├── environment
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("post/post-greater-than-post-not-available.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a>1.2.3.post2")
        , @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only a<=1.2.3.post1 is available and you require a>=1.2.3.post3, we can conclude that your requirements are unsatisfiable.
    ");

    context.assert_not_installed("a");
}

/// A greater-than version constraint should match a post-release version if the constraint is itself a post-release version.
///
/// ```text
/// post-greater-than-post
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   └── requires a>1.2.3.post0
/// │       └── satisfied by a-1.2.3.post1
/// └── a
///     ├── a-1.2.3.post0
///     └── a-1.2.3.post1
/// ```
#[test]
fn post_greater_than_post() {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("post/post-greater-than-post.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a>1.2.3.post0")
        , @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + a==1.2.3.post1
    ");

    // The version '1.2.3.post1' satisfies the constraint '>1.2.3.post0'.
    context.assert_installed("a", "1.2.3.post1");
}

/// A greater-than version constraint should not match a post-release version.
///
/// ```text
/// post-greater-than
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   └── requires a>1.2.3
/// │       └── unsatisfied: no matching version
/// └── a
///     └── a-1.2.3.post1
/// ```
#[test]
fn post_greater_than() {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("post/post-greater-than.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a>1.2.3")
        , @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only a==1.2.3.post1 is available and you require a>1.2.3, we can conclude that your requirements are unsatisfiable.
    ");

    context.assert_not_installed("a");
}

/// A less-than-or-equal version constraint should not match a post-release version.
///
/// ```text
/// post-less-than-or-equal
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   └── requires a<=1.2.3
/// │       └── unsatisfied: no matching version
/// └── a
///     └── a-1.2.3.post1
/// ```
#[test]
fn post_less_than_or_equal() {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("post/post-less-than-or-equal.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a<=1.2.3")
        , @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only a==1.2.3.post1 is available and you require a<=1.2.3, we can conclude that your requirements are unsatisfiable.
    ");

    context.assert_not_installed("a");
}

/// A less-than version constraint should not match a post-release version.
///
/// ```text
/// post-less-than
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   └── requires a<1.2.3
/// │       └── unsatisfied: no matching version
/// └── a
///     └── a-1.2.3.post1
/// ```
#[test]
fn post_less_than() {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("post/post-less-than.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a<1.2.3")
        , @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only a==1.2.3.post1 is available and you require a<1.2.3, we can conclude that your requirements are unsatisfiable.
    ");

    context.assert_not_installed("a");
}

/// A greater-than version constraint should not match a post-release version with a local version identifier.
///
/// ```text
/// post-local-greater-than-post
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   └── requires a>1.2.3.post1
/// │       └── unsatisfied: no matching version
/// └── a
///     ├── a-1.2.3.post1
///     └── a-1.2.3.post1+local
/// ```
#[test]
fn post_local_greater_than_post() {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("post/post-local-greater-than-post.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a>1.2.3.post1")
        , @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only a<=1.2.3.post1+local is available and you require a>=1.2.3.post2, we can conclude that your requirements are unsatisfiable.
    ");

    context.assert_not_installed("a");
}

/// A greater-than version constraint should not match a post-release version with a local version identifier.
///
/// ```text
/// post-local-greater-than
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   └── requires a>1.2.3
/// │       └── unsatisfied: no matching version
/// └── a
///     ├── a-1.2.3.post1
///     └── a-1.2.3.post1+local
/// ```
#[test]
fn post_local_greater_than() {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("post/post-local-greater-than.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a>1.2.3")
        , @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only a<=1.2.3.post1+local is available and you require a>1.2.3, we can conclude that your requirements are unsatisfiable.
    ");

    context.assert_not_installed("a");
}

/// A simple version constraint should not match a post-release version.
///
/// ```text
/// post-simple
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   └── requires a==1.2.3
/// │       └── unsatisfied: no matching version
/// └── a
///     └── a-1.2.3.post1
/// ```
#[test]
fn post_simple() {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("post/post-simple.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a==1.2.3")
        , @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because there is no version of a==1.2.3 and you require a==1.2.3, we can conclude that your requirements are unsatisfiable.
    ");

    context.assert_not_installed("a");
}

/// The user requires `a` which has multiple prereleases available with different labels.
///
/// ```text
/// package-multiple-prereleases-kinds
/// ├── environment
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("prereleases/package-multiple-prereleases-kinds.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a>=1.0.0a1")
        , @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + a==1.0.0rc1
    ");

    // Release candidates should be the highest precedence prerelease kind.
    context.assert_installed("a", "1.0.0rc1");
}

/// The user requires `a` which has multiple alphas available.
///
/// ```text
/// package-multiple-prereleases-numbers
/// ├── environment
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("prereleases/package-multiple-prereleases-numbers.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a>=1.0.0a1")
        , @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + a==1.0.0a3
    ");

    // The latest alpha version should be selected.
    context.assert_installed("a", "1.0.0a3");
}

/// The user requires a non-prerelease version of `a` which only has prerelease versions available. There are pre-releases on the boundary of their range.
///
/// ```text
/// package-only-prereleases-boundary
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   └── requires a<0.2.0
/// │       └── satisfied by a-0.1.0a1
/// └── a
///     ├── a-0.1.0a1
///     ├── a-0.2.0a1
///     └── a-0.3.0a1
/// ```
#[test]
fn package_only_prereleases_boundary() {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("prereleases/package-only-prereleases-boundary.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a<0.2.0")
        , @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + a==0.1.0a1
    ");

    // Since there are only prerelease versions of `a` available, a prerelease is allowed. Since the user did not explicitly request a pre-release, pre-releases at the boundary should not be selected.
    context.assert_installed("a", "0.1.0a1");
}

/// The user requires a version of package `a` which only matches prerelease versions but they did not include a prerelease specifier.
///
/// ```text
/// package-only-prereleases-in-range
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   └── requires a>0.1.0
/// │       └── satisfied by a-1.0.0a1
/// └── a
///     ├── a-0.1.0
///     └── a-1.0.0a1
/// ```
#[test]
fn package_only_prereleases_in_range() {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("prereleases/package-only-prereleases-in-range.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a>0.1.0")
        , @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only a<=0.1.0 is available and you require a>0.1.0, we can conclude that your requirements are unsatisfiable.

          hint: Pre-releases are available for `a` in the requested range (e.g., 1.0.0a1), but pre-releases weren't enabled (try: `--prerelease=allow`)
    ");

    // Since there are stable versions of `a` available, prerelease versions should not be selected without explicit opt-in.
    context.assert_not_installed("a");
}

/// The user requires any version of package `a` which only has prerelease versions available.
///
/// ```text
/// package-only-prereleases
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   └── requires a
/// │       └── satisfied by a-1.0.0a1
/// └── a
///     └── a-1.0.0a1
/// ```
#[test]
fn package_only_prereleases() {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("prereleases/package-only-prereleases.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a")
        , @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + a==1.0.0a1
    ");

    // Since there are only prerelease versions of `a` available, it should be installed even though the user did not include a prerelease specifier.
    context.assert_installed("a", "1.0.0a1");
}

/// The user requires a version of `a` with a prerelease specifier and both prerelease and stable releases are available.
///
/// ```text
/// package-prerelease-specified-mixed-available
/// ├── environment
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("prereleases/package-prerelease-specified-mixed-available.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a>=0.1.0a1")
        , @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + a==1.0.0a1
    ");

    // Since the user provided a prerelease specifier, the latest prerelease version should be selected.
    context.assert_installed("a", "1.0.0a1");
}

/// The user requires a version of `a` with a prerelease specifier and only stable releases are available.
///
/// ```text
/// package-prerelease-specified-only-final-available
/// ├── environment
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server =
        PackseServer::new("prereleases/package-prerelease-specified-only-final-available.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a>=0.1.0a1")
        , @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + a==0.3.0
    ");

    // The latest stable version should be selected.
    context.assert_installed("a", "0.3.0");
}

/// The user requires a version of `a` with a prerelease specifier and only prerelease releases are available.
///
/// ```text
/// package-prerelease-specified-only-prerelease-available
/// ├── environment
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new(
        "prereleases/package-prerelease-specified-only-prerelease-available.toml",
    );

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a>=0.1.0a1")
        , @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + a==0.3.0a1
    ");

    // The latest prerelease version should be selected.
    context.assert_installed("a", "0.3.0a1");
}

/// The user requires a non-prerelease version of `a` but has enabled pre-releases. There are pre-releases on the boundary of their range.
///
/// ```text
/// package-prereleases-boundary
/// ├── environment
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("prereleases/package-prereleases-boundary.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("--prerelease=allow")
        .arg("a<0.2.0")
        , @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + a==0.1.0
    ");

    // Since the user did not use a pre-release specifier, pre-releases at the boundary should not be selected even though pre-releases are allowed.
    context.assert_installed("a", "0.1.0");
}

/// The user requires a non-prerelease version of `a` but has enabled pre-releases. There are pre-releases on the boundary of their range.
///
/// ```text
/// package-prereleases-global-boundary
/// ├── environment
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("prereleases/package-prereleases-global-boundary.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("--prerelease=allow")
        .arg("a<0.2.0")
        , @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + a==0.1.0
    ");

    // Since the user did not use a pre-release specifier, pre-releases at the boundary should not be selected even though pre-releases are allowed.
    context.assert_installed("a", "0.1.0");
}

/// The user requires a prerelease version of `a`. There are pre-releases on the boundary of their range.
///
/// ```text
/// package-prereleases-specifier-boundary
/// ├── environment
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("prereleases/package-prereleases-specifier-boundary.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a<0.2.0a2")
        , @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + a==0.2.0a1
    ");

    // Since the user used a pre-release specifier, pre-releases at the boundary should be selected.
    context.assert_installed("a", "0.2.0a1");
}

/// The user requires a version of package `a` which only matches prerelease versions. They did not include a prerelease specifier for the package, but they opted into prereleases globally.
///
/// ```text
/// requires-package-only-prereleases-in-range-global-opt-in
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   └── requires a>0.1.0
/// │       └── satisfied by a-1.0.0a1
/// └── a
///     ├── a-0.1.0
///     └── a-1.0.0a1
/// ```
#[test]
fn requires_package_only_prereleases_in_range_global_opt_in() {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new(
        "prereleases/requires-package-only-prereleases-in-range-global-opt-in.toml",
    );

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("--prerelease=allow")
        .arg("a>0.1.0")
        , @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + a==1.0.0a1
    ");

    context.assert_installed("a", "1.0.0a1");
}

/// The user requires any version of package `a` has a prerelease version available and an older non-prerelease version.
///
/// ```text
/// requires-package-prerelease-and-final-any
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   └── requires a
/// │       ├── satisfied by a-0.1.0
/// │       └── satisfied by a-1.0.0a1
/// └── a
///     ├── a-0.1.0
///     └── a-1.0.0a1
/// ```
#[test]
fn requires_package_prerelease_and_final_any() {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("prereleases/requires-package-prerelease-and-final-any.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a")
        , @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + a==0.1.0
    ");

    // Since the user did not provide a prerelease specifier, the older stable version should be selected.
    context.assert_installed("a", "0.1.0");
}

/// The user requires package `a` which has a dependency on a package which only matches prerelease versions; the user has opted into allowing prereleases in `b` explicitly.
///
/// ```text
/// transitive-package-only-prereleases-in-range-opt-in
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   ├── requires a
/// │   │   └── satisfied by a-0.1.0
/// │   └── requires b>0.0.0a1
/// │       ├── satisfied by b-0.1.0
/// │       └── satisfied by b-1.0.0a1
/// ├── a
/// │   └── a-0.1.0
/// │       └── requires b>0.1
/// │           └── satisfied by b-1.0.0a1
/// └── b
///     ├── b-0.1.0
///     └── b-1.0.0a1
/// ```
#[test]
fn transitive_package_only_prereleases_in_range_opt_in() {
    let context = uv_test::test_context!("3.12");
    let server =
        PackseServer::new("prereleases/transitive-package-only-prereleases-in-range-opt-in.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a")
                .arg("b>0.0.0a1")
        , @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + a==0.1.0
     + b==1.0.0a1
    ");

    // Since the user included a dependency on `b` with a prerelease specifier, a prerelease version can be selected.
    context.assert_installed("a", "0.1.0");
    context.assert_installed("b", "1.0.0a1");
}

/// The user requires package `a` which has a dependency on a package which only matches prerelease versions but they did not include a prerelease specifier.
///
/// ```text
/// transitive-package-only-prereleases-in-range
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   └── requires a
/// │       └── satisfied by a-0.1.0
/// ├── a
/// │   └── a-0.1.0
/// │       └── requires b>0.1
/// │           └── satisfied by b-1.0.0a1
/// └── b
///     ├── b-0.1.0
///     └── b-1.0.0a1
/// ```
#[test]
fn transitive_package_only_prereleases_in_range() {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("prereleases/transitive-package-only-prereleases-in-range.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a")
        , @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only b<=0.1 is available and a==0.1.0 depends on b>0.1, we can conclude that a==0.1.0 cannot be used.
          And because only a==0.1.0 is available and you require a, we can conclude that your requirements are unsatisfiable.

          hint: Pre-releases are available for `b` in the requested range (e.g., 1.0.0a1), but pre-releases weren't enabled (try: `--prerelease=allow`)
    ");

    // Since there are stable versions of `b` available, the prerelease version should not be selected without explicit opt-in. The available version is excluded by the range requested by the user.
    context.assert_not_installed("a");
}

/// The user requires any version of package `a` which requires `b` which only has prerelease versions available.
///
/// ```text
/// transitive-package-only-prereleases
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   └── requires a
/// │       └── satisfied by a-0.1.0
/// ├── a
/// │   └── a-0.1.0
/// │       └── requires b
/// │           └── satisfied by b-1.0.0a1
/// └── b
///     └── b-1.0.0a1
/// ```
#[test]
fn transitive_package_only_prereleases() {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("prereleases/transitive-package-only-prereleases.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a")
        , @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + a==0.1.0
     + b==1.0.0a1
    ");

    // Since there are only prerelease versions of `b` available, it should be selected even though the user did not opt-in to prereleases.
    context.assert_installed("a", "0.1.0");
    context.assert_installed("b", "1.0.0a1");
}

/// A transitive dependency has both a prerelease and a stable selector, but can only be satisfied by a prerelease. There are many prerelease versions and some are excluded.
///
/// ```text
/// transitive-prerelease-and-stable-dependency-many-versions-holes
/// ├── environment
/// │   └── python3.12
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
/// │           ├── satisfied by c-1.0.0
/// │           ├── satisfied by c-2.0.0a1
/// │           ├── satisfied by c-2.0.0a2
/// │           ├── satisfied by c-2.0.0a3
/// │           ├── satisfied by c-2.0.0a4
/// │           ├── satisfied by c-2.0.0a5
/// │           ├── satisfied by c-2.0.0a6
/// │           ├── satisfied by c-2.0.0a7
/// │           ├── satisfied by c-2.0.0a8
/// │           ├── satisfied by c-2.0.0a9
/// │           ├── satisfied by c-2.0.0b1
/// │           ├── satisfied by c-2.0.0b2
/// │           ├── satisfied by c-2.0.0b3
/// │           ├── satisfied by c-2.0.0b4
/// │           ├── satisfied by c-2.0.0b5
/// │           ├── satisfied by c-2.0.0b6
/// │           ├── satisfied by c-2.0.0b7
/// │           ├── satisfied by c-2.0.0b8
/// │           └── satisfied by c-2.0.0b9
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new(
        "prereleases/transitive-prerelease-and-stable-dependency-many-versions-holes.toml",
    );

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a")
                .arg("b")
        , @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only the following versions of c are available:
              c<=1.0.0
              c>=2.0.0a5,<=2.0.0a7
              c==2.0.0b1
              c>=2.0.0b5
          and a==1.0.0 depends on one of:
              c>1.0.0,<2.0.0a5
              c>2.0.0a7,<2.0.0b1
              c>2.0.0b1,<2.0.0b5
          we can conclude that a==1.0.0 cannot be used.
          And because only a==1.0.0 is available and you require a, we can conclude that your requirements are unsatisfiable.

          hint: `c` was requested with a pre-release marker (e.g., all of:
              c>1.0.0,<2.0.0a5
              c>2.0.0a7,<2.0.0b1
              c>2.0.0b1,<2.0.0b5
          ), but pre-releases weren't enabled (try: `--prerelease=allow`)
    ");

    // Since the user did not explicitly opt-in to a prerelease, it cannot be selected.
    context.assert_not_installed("a");
    context.assert_not_installed("b");
}

/// A transitive dependency has both a prerelease and a stable selector, but can only be satisfied by a prerelease. There are many prerelease versions.
///
/// ```text
/// transitive-prerelease-and-stable-dependency-many-versions
/// ├── environment
/// │   └── python3.12
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
/// │           ├── satisfied by c-1.0.0
/// │           ├── satisfied by c-2.0.0a1
/// │           ├── satisfied by c-2.0.0a2
/// │           ├── satisfied by c-2.0.0a3
/// │           ├── satisfied by c-2.0.0a4
/// │           ├── satisfied by c-2.0.0a5
/// │           ├── satisfied by c-2.0.0a6
/// │           ├── satisfied by c-2.0.0a7
/// │           ├── satisfied by c-2.0.0a8
/// │           ├── satisfied by c-2.0.0a9
/// │           ├── satisfied by c-2.0.0b1
/// │           ├── satisfied by c-2.0.0b2
/// │           ├── satisfied by c-2.0.0b3
/// │           ├── satisfied by c-2.0.0b4
/// │           ├── satisfied by c-2.0.0b5
/// │           ├── satisfied by c-2.0.0b6
/// │           ├── satisfied by c-2.0.0b7
/// │           ├── satisfied by c-2.0.0b8
/// │           └── satisfied by c-2.0.0b9
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new(
        "prereleases/transitive-prerelease-and-stable-dependency-many-versions.toml",
    );

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a")
                .arg("b")
        , @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only a==1.0.0 is available and a==1.0.0 depends on c>=2.0.0b1, we can conclude that all versions of a depend on c>=2.0.0b1.
          And because only c<2.0.0b1 is available, we can conclude that all versions of a depend on c>3.0.0.
          And because b==1.0.0 depends on c and only b==1.0.0 is available, we can conclude that all versions of a and all versions of b are incompatible.
          And because you require a and b, we can conclude that your requirements are unsatisfiable.

          hint: `c` was requested with a pre-release marker (e.g., c>=2.0.0b1), but pre-releases weren't enabled (try: `--prerelease=allow`)
    ");

    // Since the user did not explicitly opt-in to a prerelease, it cannot be selected.
    context.assert_not_installed("a");
    context.assert_not_installed("b");
}

/// A transitive dependency has both a prerelease and a stable selector, but can only be satisfied by a prerelease. The user includes an opt-in to prereleases of the transitive dependency.
///
/// ```text
/// transitive-prerelease-and-stable-dependency-opt-in
/// ├── environment
/// │   └── python3.12
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
/// │           ├── satisfied by c-1.0.0
/// │           └── satisfied by c-2.0.0b1
/// └── c
///     ├── c-1.0.0
///     └── c-2.0.0b1
/// ```
#[test]
fn transitive_prerelease_and_stable_dependency_opt_in() {
    let context = uv_test::test_context!("3.12");
    let server =
        PackseServer::new("prereleases/transitive-prerelease-and-stable-dependency-opt-in.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a")
                .arg("b")
                .arg("c>=0.0.0a1")
        , @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + a==1.0.0
     + b==1.0.0
     + c==2.0.0b1
    ");

    // Since the user explicitly opted-in to a prerelease for `c`, it can be installed.
    context.assert_installed("a", "1.0.0");
    context.assert_installed("b", "1.0.0");
    context.assert_installed("c", "2.0.0b1");
}

/// A transitive dependency has both a prerelease and a stable selector, but can only be satisfied by a prerelease
///
/// ```text
/// transitive-prerelease-and-stable-dependency
/// ├── environment
/// │   └── python3.12
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
/// │           ├── satisfied by c-1.0.0
/// │           └── satisfied by c-2.0.0b1
/// └── c
///     ├── c-1.0.0
///     └── c-2.0.0b1
/// ```
#[test]
fn transitive_prerelease_and_stable_dependency() {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("prereleases/transitive-prerelease-and-stable-dependency.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a")
                .arg("b")
        , @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because there is no version of c==2.0.0b1 and a==1.0.0 depends on c==2.0.0b1, we can conclude that a==1.0.0 cannot be used.
          And because only a==1.0.0 is available and you require a, we can conclude that your requirements are unsatisfiable.

          hint: `c` was requested with a pre-release marker (e.g., c==2.0.0b1), but pre-releases weren't enabled (try: `--prerelease=allow`)
    ");

    // Since the user did not explicitly opt-in to a prerelease, it cannot be selected.
    context.assert_not_installed("a");
    context.assert_not_installed("b");
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
    let context = uv_test::test_context!("3.9");
    let server = PackseServer::new("requires_python/python-greater-than-current-backtrack.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a")
        , @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + a==1.0.0
    ");

    context.assert_installed("a", "1.0.0");
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
    let context = uv_test::test_context!("3.9");
    let server = PackseServer::new("requires_python/python-greater-than-current-excluded.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a>=2.0.0")
        , @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because the current Python version (3.9.[X]) does not satisfy Python>=3.10 and a==2.0.0 depends on Python>=3.10, we can conclude that a==2.0.0 cannot be used.
          And because only the following versions of a are available:
              a<=2.0.0
              a==3.0.0
              a==4.0.0
          we can conclude that a>=2.0.0,<3.0.0 cannot be used. (1)

          Because the current Python version (3.9.[X]) does not satisfy Python>=3.11 and a==3.0.0 depends on Python>=3.11, we can conclude that a==3.0.0 cannot be used.
          And because we know from (1) that a>=2.0.0,<3.0.0 cannot be used, we can conclude that a>=2.0.0,<4.0.0 cannot be used. (2)

          Because the current Python version (3.9.[X]) does not satisfy Python>=3.12 and a==4.0.0 depends on Python>=3.12, we can conclude that a==4.0.0 cannot be used.
          And because we know from (2) that a>=2.0.0,<4.0.0 cannot be used, we can conclude that a>=2.0.0 cannot be used.
          And because you require a>=2.0.0, we can conclude that your requirements are unsatisfiable.
    ");

    context.assert_not_installed("a");
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
    let context = uv_test::test_context!("3.9");
    let server = PackseServer::new("requires_python/python-greater-than-current-many.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a==1.0.0")
        , @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because there is no version of a==1.0.0 and you require a==1.0.0, we can conclude that your requirements are unsatisfiable.
    ");

    context.assert_not_installed("a");
}

/// The user requires a package which requires a Python version with a patch version greater than the current patch version
///
/// ```text
/// python-greater-than-current-patch
/// ├── environment
/// │   └── python3.13.0
/// ├── root
/// │   └── requires a==1.0.0
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
///         └── requires python>=3.13.2 (incompatible with environment)
/// ```
#[cfg(feature = "test-python-patch")]
#[test]
fn python_greater_than_current_patch() {
    let context = uv_test::test_context!("3.13.0");
    let server = PackseServer::new("requires_python/python-greater-than-current-patch.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a==1.0.0")
        , @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because the current Python version (3.13) does not satisfy Python>=3.13.2 and a==1.0.0 depends on Python>=3.13.2, we can conclude that a==1.0.0 cannot be used.
          And because you require a==1.0.0, we can conclude that your requirements are unsatisfiable.
    ");

    context.assert_not_installed("a");
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
    let context = uv_test::test_context!("3.9");
    let server = PackseServer::new("requires_python/python-greater-than-current.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a==1.0.0")
        , @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because the current Python version (3.9.[X]) does not satisfy Python>=3.10 and a==1.0.0 depends on Python>=3.10, we can conclude that a==1.0.0 cannot be used.
          And because you require a==1.0.0, we can conclude that your requirements are unsatisfiable.
    ");

    context.assert_not_installed("a");
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
    let context = uv_test::test_context!("3.9");
    let server = PackseServer::new("requires_python/python-less-than-current.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a==1.0.0")
        , @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + a==1.0.0
    ");

    // We ignore the upper bound on Python requirements
}

/// The user requires a package which requires a Python version that does not exist
///
/// ```text
/// python-version-does-not-exist
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   └── requires a==1.0.0
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
///         └── requires python>=3.30 (incompatible with environment)
/// ```
#[test]
fn python_version_does_not_exist() {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("requires_python/python-version-does-not-exist.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a==1.0.0")
        , @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because the current Python version (3.12.[X]) does not satisfy Python>=3.30 and a==1.0.0 depends on Python>=3.30, we can conclude that a==1.0.0 cannot be used.
          And because you require a==1.0.0, we can conclude that your requirements are unsatisfiable.
    ");

    context.assert_not_installed("a");
}

/// Both wheels and source distributions are available, and the user has disabled binaries.
///
/// ```text
/// no-binary
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   └── requires a
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
/// ```
#[test]
fn no_binary() {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("wheels/no-binary.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("--no-binary")
        .arg("a")
        .arg("a")
        , @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + a==1.0.0
    ");

    // The source distribution should be used for install
}

/// Both wheels and source distributions are available, and the user has disabled builds.
///
/// ```text
/// no-build
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   └── requires a
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
/// ```
#[test]
fn no_build() {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("wheels/no-build.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("--only-binary")
        .arg("a")
        .arg("a")
        , @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + a==1.0.0
    ");

    // The wheel should be used for install
}

/// No wheels with matching ABI tags are available, nor are any source distributions available
///
/// ```text
/// no-sdist-no-wheels-with-matching-abi
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   └── requires a
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
/// ```
#[test]
fn no_sdist_no_wheels_with_matching_abi() {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("wheels/no-sdist-no-wheels-with-matching-abi.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("--python-platform=x86_64-manylinux2014")
        .arg("a")
        , @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only a==1.0.0 is available and a==1.0.0 has no wheels with a matching Python ABI tag (e.g., `cp312`), we can conclude that all versions of a cannot be used.
          And because you require a, we can conclude that your requirements are unsatisfiable.

          hint: You require CPython 3.12 (`cp312`), but we only found wheels for `a` (v1.0.0) with the following Python ABI tag: `graalpy240_310_native`
    ");

    context.assert_not_installed("a");
}

/// No wheels with matching platform tags are available, nor are any source distributions available
///
/// ```text
/// no-sdist-no-wheels-with-matching-platform
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   └── requires a
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
/// ```
#[test]
fn no_sdist_no_wheels_with_matching_platform() {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("wheels/no-sdist-no-wheels-with-matching-platform.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("--python-platform=x86_64-manylinux2014")
        .arg("a")
        , @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only a==1.0.0 is available and a==1.0.0 has no wheels with a matching platform tag (e.g., `manylinux_2_17_x86_64`), we can conclude that all versions of a cannot be used.
          And because you require a, we can conclude that your requirements are unsatisfiable.

          hint: Wheels are available for `a` (v1.0.0) on the following platform: `macosx_10_0_ppc64`
    ");

    context.assert_not_installed("a");
}

/// No wheels with matching Python tags are available, nor are any source distributions available
///
/// ```text
/// no-sdist-no-wheels-with-matching-python
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   └── requires a
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
/// ```
#[test]
fn no_sdist_no_wheels_with_matching_python() {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("wheels/no-sdist-no-wheels-with-matching-python.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("--python-platform=x86_64-manylinux2014")
        .arg("a")
        , @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only a==1.0.0 is available and a==1.0.0 has no wheels with a matching Python implementation tag (e.g., `cp312`), we can conclude that all versions of a cannot be used.
          And because you require a, we can conclude that your requirements are unsatisfiable.

          hint: You require CPython 3.12 (`cp312`), but we only found wheels for `a` (v1.0.0) with the following Python implementation tag: `graalpy310`
    ");

    context.assert_not_installed("a");
}

/// No wheels are available, only source distributions but the user has disabled builds.
///
/// ```text
/// no-wheels-no-build
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   └── requires a
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
/// ```
#[test]
fn no_wheels_no_build() {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("wheels/no-wheels-no-build.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("--only-binary")
        .arg("a")
        .arg("a")
        , @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only a==1.0.0 is available and a==1.0.0 has no usable wheels, we can conclude that all versions of a cannot be used.
          And because you require a, we can conclude that your requirements are unsatisfiable.

          hint: Wheels are required for `a` because building from source is disabled for `a` (i.e., with `--no-build-package a`)
    ");

    context.assert_not_installed("a");
}

/// No wheels with matching platform tags are available, just source distributions.
///
/// ```text
/// no-wheels-with-matching-platform
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   └── requires a
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
/// ```
#[test]
fn no_wheels_with_matching_platform() {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("wheels/no-wheels-with-matching-platform.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a")
        , @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + a==1.0.0
    ");
}

/// No wheels are available, only source distributions.
///
/// ```text
/// no-wheels
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   └── requires a
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
/// ```
#[test]
fn no_wheels() {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("wheels/no-wheels.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a")
        , @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + a==1.0.0
    ");
}

/// No source distributions are available, only wheels but the user has disabled using pre-built binaries.
///
/// ```text
/// only-wheels-no-binary
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   └── requires a
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
/// ```
#[test]
fn only_wheels_no_binary() {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("wheels/only-wheels-no-binary.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("--no-binary")
        .arg("a")
        .arg("a")
        , @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only a==1.0.0 is available and a==1.0.0 has no source distribution, we can conclude that all versions of a cannot be used.
          And because you require a, we can conclude that your requirements are unsatisfiable.

          hint: A source distribution is required for `a` because using pre-built wheels is disabled for `a` (i.e., with `--no-binary-package a`)
    ");

    context.assert_not_installed("a");
}

/// No source distributions are available, only wheels.
///
/// ```text
/// only-wheels
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   └── requires a
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
/// ```
#[test]
fn only_wheels() {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("wheels/only-wheels.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a")
        , @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + a==1.0.0
    ");
}

/// A wheel for a specific platform is available alongside the default.
///
/// ```text
/// specific-tag-and-default
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   └── requires a
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
/// ```
#[test]
fn specific_tag_and_default() {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("wheels/specific-tag-and-default.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a")
        , @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + a==1.0.0
    ");
}

/// The user requires a version of package `a` which only matches yanked versions.
///
/// ```text
/// package-only-yanked-in-range
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   └── requires a>0.1.0
/// │       └── unsatisfied: no matching version
/// └── a
///     ├── a-0.1.0
///     └── a-1.0.0 (yanked)
/// ```
#[test]
fn package_only_yanked_in_range() {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("yanked/package-only-yanked-in-range.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a>0.1.0")
        , @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only the following versions of a are available:
              a<=0.1.0
              a==1.0.0
          and a==1.0.0 was yanked, we can conclude that a>0.1.0 cannot be used.
          And because you require a>0.1.0, we can conclude that your requirements are unsatisfiable.
    ");

    // Since there are other versions of `a` available, yanked versions should not be selected without explicit opt-in.
    context.assert_not_installed("a");
}

/// The user requires any version of package `a` which only has yanked versions available.
///
/// ```text
/// package-only-yanked
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   └── requires a
/// │       └── unsatisfied: no matching version
/// └── a
///     └── a-1.0.0 (yanked)
/// ```
#[test]
fn package_only_yanked() {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("yanked/package-only-yanked.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a")
        , @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only a==1.0.0 is available and a==1.0.0 was yanked, we can conclude that all versions of a cannot be used.
          And because you require a, we can conclude that your requirements are unsatisfiable.
    ");

    // Yanked versions should not be installed, even if they are the only one available.
    context.assert_not_installed("a");
}

/// The user requires any version of `a` and both yanked and unyanked releases are available.
///
/// ```text
/// package-yanked-specified-mixed-available
/// ├── environment
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("yanked/package-yanked-specified-mixed-available.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a>=0.1.0")
        , @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + a==0.3.0
    ");

    // The latest unyanked version should be selected.
    context.assert_installed("a", "0.3.0");
}

/// The user requires any version of package `a` has a yanked version available and an older unyanked version.
///
/// ```text
/// requires-package-yanked-and-unyanked-any
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   └── requires a
/// │       └── satisfied by a-0.1.0
/// └── a
///     ├── a-0.1.0
///     └── a-1.0.0 (yanked)
/// ```
#[test]
fn requires_package_yanked_and_unyanked_any() {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("yanked/requires-package-yanked-and-unyanked-any.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a")
        , @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + a==0.1.0
    ");

    // The unyanked version should be selected.
    context.assert_installed("a", "0.1.0");
}

/// The user requires package `a` which has a dependency on a package which only matches yanked versions; the user has opted into allowing the yanked version of `b` explicitly.
///
/// ```text
/// transitive-package-only-yanked-in-range-opt-in
/// ├── environment
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("yanked/transitive-package-only-yanked-in-range-opt-in.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a")
                .arg("b==1.0.0")
        , @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + a==0.1.0
     + b==1.0.0
    warning: `b==1.0.0` is yanked
    ");

    // Since the user included a dependency on `b` with an exact specifier, the yanked version can be selected.
    context.assert_installed("a", "0.1.0");
    context.assert_installed("b", "1.0.0");
}

/// The user requires package `a` which has a dependency on a package which only matches yanked versions.
///
/// ```text
/// transitive-package-only-yanked-in-range
/// ├── environment
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("yanked/transitive-package-only-yanked-in-range.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a")
        , @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only the following versions of b are available:
              b<=0.1
              b==1.0.0
          and b==1.0.0 was yanked, we can conclude that b>0.1 cannot be used.
          And because a==0.1.0 depends on b>0.1, we can conclude that a==0.1.0 cannot be used.
          And because only a==0.1.0 is available and you require a, we can conclude that your requirements are unsatisfiable.
    ");

    // Yanked versions should not be installed, even if they are the only valid version in a range.
    context.assert_not_installed("a");
}

/// The user requires any version of package `a` which requires `b` which only has yanked versions available.
///
/// ```text
/// transitive-package-only-yanked
/// ├── environment
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("yanked/transitive-package-only-yanked.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a")
        , @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only b==1.0.0 is available and b==1.0.0 was yanked, we can conclude that all versions of b cannot be used.
          And because a==0.1.0 depends on b, we can conclude that a==0.1.0 cannot be used.
          And because only a==0.1.0 is available and you require a, we can conclude that your requirements are unsatisfiable.
    ");

    // Yanked versions should not be installed, even if they are the only one available.
    context.assert_not_installed("a");
}

/// A transitive dependency has both a yanked and an unyanked version, but can only be satisfied by a yanked. The user includes an opt-in to the yanked version of the transitive dependency.
///
/// ```text
/// transitive-yanked-and-unyanked-dependency-opt-in
/// ├── environment
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("yanked/transitive-yanked-and-unyanked-dependency-opt-in.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a")
                .arg("b")
                .arg("c==2.0.0")
        , @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + a==1.0.0
     + b==1.0.0
     + c==2.0.0
    warning: `c==2.0.0` is yanked
    ");

    // Since the user explicitly selected the yanked version of `c`, it can be installed.
    context.assert_installed("a", "1.0.0");
    context.assert_installed("b", "1.0.0");
    context.assert_installed("c", "2.0.0");
}

/// A transitive dependency has both a yanked and an unyanked version, but can only be satisfied by a yanked version
///
/// ```text
/// transitive-yanked-and-unyanked-dependency
/// ├── environment
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("yanked/transitive-yanked-and-unyanked-dependency.toml");

    uv_snapshot!(context.filters(), command(&context, &server)
        .arg("a")
                .arg("b")
        , @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because c==2.0.0 was yanked and a==1.0.0 depends on c==2.0.0, we can conclude that a==1.0.0 cannot be used.
          And because only a==1.0.0 is available and you require a, we can conclude that your requirements are unsatisfiable.
    ");

    // Since the user did not explicitly select the yanked version, it cannot be used.
    context.assert_not_installed("a");
    context.assert_not_installed("b");
}
