//! DO NOT EDIT
//!
//! Generated with `./scripts/sync_scenarios.sh`
//! Scenarios from <https://github.com/astral-sh/packse/tree/HEAD/scenarios>
//!
#![cfg(all(feature = "python", feature = "pypi"))]
#![allow(clippy::needless_raw_string_hashes)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::doc_lazy_continuation)]

use anyhow::Result;
use assert_cmd::assert::OutputAssertExt;
use assert_fs::prelude::*;
use insta::assert_snapshot;

use uv_static::EnvVars;

use crate::common::{packse_index_url, uv_snapshot, TestContext};

/// There are two packages, `a` and `b`. We select `a` with `a==2.0.0` first, and then `b`, but `a==2.0.0` conflicts with all new versions of `b`, so we backtrack through versions of `b`.
///
/// We need to detect this conflict and prioritize `b` over `a` instead of backtracking down to the too old version of `b==1.0.0` that doesn't depend on `a` anymore.
///
/// ```text
/// wrong-backtracking-basic
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   ├── requires a
/// │   │   ├── satisfied by a-1.0.0
/// │   │   └── satisfied by a-2.0.0
/// │   └── requires b
/// │       ├── satisfied by b-1.0.0
/// │       ├── satisfied by b-2.0.0
/// │       ├── satisfied by b-2.0.1
/// │       ├── satisfied by b-2.0.2
/// │       ├── satisfied by b-2.0.3
/// │       ├── satisfied by b-2.0.4
/// │       ├── satisfied by b-2.0.5
/// │       ├── satisfied by b-2.0.6
/// │       ├── satisfied by b-2.0.7
/// │       ├── satisfied by b-2.0.8
/// │       └── satisfied by b-2.0.9
/// ├── a
/// │   ├── a-1.0.0
/// │   └── a-2.0.0
/// ├── b
/// │   ├── b-1.0.0
/// │   │   └── requires too-old
/// │   │       └── satisfied by too-old-1.0.0
/// │   ├── b-2.0.0
/// │   │   └── requires a==1.0.0
/// │   │       └── satisfied by a-1.0.0
/// │   ├── b-2.0.1
/// │   │   └── requires a==1.0.0
/// │   │       └── satisfied by a-1.0.0
/// │   ├── b-2.0.2
/// │   │   └── requires a==1.0.0
/// │   │       └── satisfied by a-1.0.0
/// │   ├── b-2.0.3
/// │   │   └── requires a==1.0.0
/// │   │       └── satisfied by a-1.0.0
/// │   ├── b-2.0.4
/// │   │   └── requires a==1.0.0
/// │   │       └── satisfied by a-1.0.0
/// │   ├── b-2.0.5
/// │   │   └── requires a==1.0.0
/// │   │       └── satisfied by a-1.0.0
/// │   ├── b-2.0.6
/// │   │   └── requires a==1.0.0
/// │   │       └── satisfied by a-1.0.0
/// │   ├── b-2.0.7
/// │   │   └── requires a==1.0.0
/// │   │       └── satisfied by a-1.0.0
/// │   ├── b-2.0.8
/// │   │   └── requires a==1.0.0
/// │   │       └── satisfied by a-1.0.0
/// │   └── b-2.0.9
/// │       └── requires a==1.0.0
/// │           └── satisfied by a-1.0.0
/// └── too-old
///     └── too-old-1.0.0
/// ```
#[test]
fn wrong_backtracking_basic() -> Result<()> {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"wrong-backtracking-basic-", "package-"));

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''wrong-backtracking-basic-a''',
          '''wrong-backtracking-basic-b''',
        ]
        requires-python = ">=3.8"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        revision = 1
        requires-python = ">=3.8"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "package-a" },
            { name = "package-b" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "package-a" },
            { name = "package-b" },
        ]

        [[package]]
        name = "package-a"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/wrong_backtracking_basic_a-1.0.0.tar.gz", hash = "sha256:5251a827291d4e5b7ca11c742df3aa26802cc55442e3f5fc307ff3423b8f9295" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/wrong_backtracking_basic_a-1.0.0-py3-none-any.whl", hash = "sha256:d9a7ee79b176cd36c9db03e36bc3325856dd4fb061aefc6159eecad6e8776e88" },
        ]

        [[package]]
        name = "package-b"
        version = "2.0.9"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        dependencies = [
            { name = "package-a" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/wrong_backtracking_basic_b-2.0.9.tar.gz", hash = "sha256:a4e95f3f0f0d82cc5f19de6c638f70300da1b5101f1ba70d8814c7fe7e949e20" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/wrong_backtracking_basic_b-2.0.9-py3-none-any.whl", hash = "sha256:bf96af1a69f8c1d1d9c2687cd5d6f023cda56dd77d3f37f3cdd422e2a410541f" },
        ]
        "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(packse_index_url())
        .assert()
        .success();

    Ok(())
}

/// This test ensures that multiple non-conflicting but also
/// non-overlapping dependency specifications with the same package name
/// are allowed and supported.
///
/// At time of writing, this provokes a fork in the resolver, but it
/// arguably shouldn't since the requirements themselves do not conflict
/// with one another. However, this does impact resolution. Namely, it
/// leaves the `a>=1` fork free to choose `a==2.0.0` since it behaves as if
/// the `a<2` constraint doesn't exist.
///
/// ```text
/// fork-allows-non-conflicting-non-overlapping-dependencies
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   ├── requires a>=1; sys_platform == "linux"
/// │   │   ├── satisfied by a-1.0.0
/// │   │   └── satisfied by a-2.0.0
/// │   └── requires a<2; sys_platform == "darwin"
/// │       └── satisfied by a-1.0.0
/// └── a
///     ├── a-1.0.0
///     └── a-2.0.0
/// ```
#[test]
fn fork_allows_non_conflicting_non_overlapping_dependencies() -> Result<()> {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((
        r"fork-allows-non-conflicting-non-overlapping-dependencies-",
        "package-",
    ));

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''fork-allows-non-conflicting-non-overlapping-dependencies-a>=1; sys_platform == "linux"''',
          '''fork-allows-non-conflicting-non-overlapping-dependencies-a<2; sys_platform == "darwin"''',
        ]
        requires-python = ">=3.8"
        "###
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        revision = 1
        requires-python = ">=3.8"
        resolution-markers = [
            "sys_platform == 'darwin'",
            "sys_platform == 'linux'",
            "sys_platform != 'darwin' and sys_platform != 'linux'",
        ]

        [[package]]
        name = "package-a"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_allows_non_conflicting_non_overlapping_dependencies_a-1.0.0.tar.gz", hash = "sha256:dd40a6bd59fbeefbf9f4936aec3df6fb6017e57d334f85f482ae5dd03ae353b9" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_allows_non_conflicting_non_overlapping_dependencies_a-1.0.0-py3-none-any.whl", hash = "sha256:8111e996c2a4e04c7a7cf91cf6f8338f5195c22ecf2303d899c4ef4e718a8175" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "package-a", marker = "sys_platform == 'darwin' or sys_platform == 'linux'" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "package-a", marker = "sys_platform == 'darwin'", specifier = "<2" },
            { name = "package-a", marker = "sys_platform == 'linux'", specifier = ">=1" },
        ]
        "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(packse_index_url())
        .assert()
        .success();

    Ok(())
}

/// This test ensures that multiple non-conflicting dependency
/// specifications with the same package name are allowed and supported.
///
/// This test exists because the universal resolver forks itself based on
/// duplicate dependency specifications by looking at package name. So at
/// first glance, a case like this could perhaps cause an errant fork.
/// While it's difficult to test for "does not create a fork" (at time of
/// writing, the implementation does not fork), we can at least check that
/// this case is handled correctly without issue. Namely, forking should
/// only occur when there are duplicate dependency specifications with
/// disjoint marker expressions.
///
/// ```text
/// fork-allows-non-conflicting-repeated-dependencies
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   ├── requires a>=1
/// │   │   ├── satisfied by a-1.0.0
/// │   │   └── satisfied by a-2.0.0
/// │   └── requires a<2
/// │       └── satisfied by a-1.0.0
/// └── a
///     ├── a-1.0.0
///     └── a-2.0.0
/// ```
#[test]
fn fork_allows_non_conflicting_repeated_dependencies() -> Result<()> {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((
        r"fork-allows-non-conflicting-repeated-dependencies-",
        "package-",
    ));

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''fork-allows-non-conflicting-repeated-dependencies-a>=1''',
          '''fork-allows-non-conflicting-repeated-dependencies-a<2''',
        ]
        requires-python = ">=3.8"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        revision = 1
        requires-python = ">=3.8"

        [[package]]
        name = "package-a"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_allows_non_conflicting_repeated_dependencies_a-1.0.0.tar.gz", hash = "sha256:45ca30f1f66eaf6790198fad279b6448719092f2128f23b99f2ede0d6dde613b" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_allows_non_conflicting_repeated_dependencies_a-1.0.0-py3-none-any.whl", hash = "sha256:fc3f6d2fab10d1bb4f52bd9a7de69dc97ed1792506706ca78bdc9e95d6641a6b" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "package-a" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "package-a", specifier = "<2" },
            { name = "package-a", specifier = ">=1" },
        ]
        "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(packse_index_url())
        .assert()
        .success();

    Ok(())
}

/// An extremely basic test of universal resolution. In this case, the resolution
/// should contain two distinct versions of `a` depending on `sys_platform`.
///
/// ```text
/// fork-basic
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   ├── requires a>=2; sys_platform == "linux"
/// │   │   └── satisfied by a-2.0.0
/// │   └── requires a<2; sys_platform == "darwin"
/// │       └── satisfied by a-1.0.0
/// └── a
///     ├── a-1.0.0
///     └── a-2.0.0
/// ```
#[test]
fn fork_basic() -> Result<()> {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"fork-basic-", "package-"));

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''fork-basic-a>=2; sys_platform == "linux"''',
          '''fork-basic-a<2; sys_platform == "darwin"''',
        ]
        requires-python = ">=3.8"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        revision = 1
        requires-python = ">=3.8"
        resolution-markers = [
            "sys_platform == 'darwin'",
            "sys_platform == 'linux'",
            "sys_platform != 'darwin' and sys_platform != 'linux'",
        ]

        [[package]]
        name = "package-a"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform == 'darwin'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_basic_a-1.0.0.tar.gz", hash = "sha256:9bd6d9d74d8928854f79ea3ed4cd0d8a4906eeaa40f5f3d63460a1c2d5f6d773" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_basic_a-1.0.0-py3-none-any.whl", hash = "sha256:9d3af617bb44ae1c8daf19f6d4d118ee8aac7eaf0cc5368d0f405137411291a1" },
        ]

        [[package]]
        name = "package-a"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_basic_a-2.0.0.tar.gz", hash = "sha256:c0ce6dfb6d712eb42a4bbe9402a1f823627b9d3773f31d259c49478fc7d8d082" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_basic_a-2.0.0-py3-none-any.whl", hash = "sha256:3876778dc6e5178b0e456b0d988cb8c2542cb943a45497aff3e198cbec3dfcc9" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "package-a", version = "1.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'darwin'" },
            { name = "package-a", version = "2.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'linux'" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "package-a", marker = "sys_platform == 'darwin'", specifier = "<2" },
            { name = "package-a", marker = "sys_platform == 'linux'", specifier = ">=2" },
        ]
        "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(packse_index_url())
        .assert()
        .success();

    Ok(())
}

/// We have a conflict after forking. This scenario exists to test the error message.
///
/// ```text
/// conflict-in-fork
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   ├── requires a>=2; sys_platform == "linux"
/// │   │   └── satisfied by a-2.0.0
/// │   └── requires a<2; sys_platform == "darwin"
/// │       └── satisfied by a-1.0.0
/// ├── a
/// │   ├── a-1.0.0
/// │   │   ├── requires b
/// │   │   │   └── satisfied by b-1.0.0
/// │   │   └── requires c
/// │   │       └── satisfied by c-1.0.0
/// │   └── a-2.0.0
/// ├── b
/// │   └── b-1.0.0
/// │       └── requires d==1
/// │           └── satisfied by d-1.0.0
/// ├── c
/// │   └── c-1.0.0
/// │       └── requires d==2
/// │           └── satisfied by d-2.0.0
/// └── d
///     ├── d-1.0.0
///     └── d-2.0.0
/// ```
#[test]
fn conflict_in_fork() -> Result<()> {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"conflict-in-fork-", "package-"));

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''conflict-in-fork-a>=2; sys_platform == "linux"''',
          '''conflict-in-fork-a<2; sys_platform == "darwin"''',
        ]
        requires-python = ">=3.8"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies for split (sys_platform == 'darwin'):
      ╰─▶ Because only package-b==1.0.0 is available and package-b==1.0.0 depends on package-d==1, we can conclude that all versions of package-b depend on package-d==1.
          And because package-c==1.0.0 depends on package-d==2 and only package-c==1.0.0 is available, we can conclude that all versions of package-b and all versions of package-c are incompatible.
          And because package-a==1.0.0 depends on package-b and package-c, we can conclude that package-a==1.0.0 cannot be used.
          And because only the following versions of package-a{sys_platform == 'darwin'} are available:
              package-a{sys_platform == 'darwin'}==1.0.0
              package-a{sys_platform == 'darwin'}>2
          and your project depends on package-a{sys_platform == 'darwin'}<2, we can conclude that your project's requirements are unsatisfiable.
    "###
    );

    Ok(())
}

/// This test ensures that conflicting dependency specifications lead to an
/// unsatisfiable result.
///
/// In particular, this is a case that should not fork even though there
/// are conflicting requirements because their marker expressions are
/// overlapping. (Well, there aren't any marker expressions here, which
/// means they are both unconditional.)
///
/// ```text
/// fork-conflict-unsatisfiable
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   ├── requires a>=2
/// │   │   ├── satisfied by a-2.0.0
/// │   │   └── satisfied by a-3.0.0
/// │   └── requires a<2
/// │       └── satisfied by a-1.0.0
/// └── a
///     ├── a-1.0.0
///     ├── a-2.0.0
///     └── a-3.0.0
/// ```
#[test]
fn fork_conflict_unsatisfiable() -> Result<()> {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"fork-conflict-unsatisfiable-", "package-"));

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''fork-conflict-unsatisfiable-a>=2''',
          '''fork-conflict-unsatisfiable-a<2''',
        ]
        requires-python = ">=3.8"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because your project depends on package-a>=2 and package-a<2, we can conclude that your project's requirements are unsatisfiable.
    "###
    );

    Ok(())
}

/// This tests that sibling dependencies of a package that provokes a
/// fork are correctly filtered out of forks where they are otherwise
/// impossible.
///
/// In this case, a previous version of the universal resolver would
/// include both `b` and `c` in *both* of the forks produced by the
/// conflicting dependency specifications on `a`. This in turn led to
/// transitive dependency specifications on both `d==1.0.0` and `d==2.0.0`.
/// Since the universal resolver only forks based on local conditions, this
/// led to a failed resolution.
///
/// The correct thing to do here is to ensure that `b` is only part of the
/// `a==4.4.0` fork and `c` is only par of the `a==4.3.0` fork.
///
/// ```text
/// fork-filter-sibling-dependencies
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   ├── requires a==4.4.0; sys_platform == "linux"
/// │   │   └── satisfied by a-4.4.0
/// │   ├── requires a==4.3.0; sys_platform == "darwin"
/// │   │   └── satisfied by a-4.3.0
/// │   ├── requires b==1.0.0; sys_platform == "linux"
/// │   │   └── satisfied by b-1.0.0
/// │   └── requires c==1.0.0; sys_platform == "darwin"
/// │       └── satisfied by c-1.0.0
/// ├── a
/// │   ├── a-4.3.0
/// │   └── a-4.4.0
/// ├── b
/// │   └── b-1.0.0
/// │       └── requires d==1.0.0
/// │           └── satisfied by d-1.0.0
/// ├── c
/// │   └── c-1.0.0
/// │       └── requires d==2.0.0
/// │           └── satisfied by d-2.0.0
/// └── d
///     ├── d-1.0.0
///     └── d-2.0.0
/// ```
#[test]
fn fork_filter_sibling_dependencies() -> Result<()> {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"fork-filter-sibling-dependencies-", "package-"));

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''fork-filter-sibling-dependencies-a==4.4.0; sys_platform == "linux"''',
          '''fork-filter-sibling-dependencies-a==4.3.0; sys_platform == "darwin"''',
          '''fork-filter-sibling-dependencies-b==1.0.0; sys_platform == "linux"''',
          '''fork-filter-sibling-dependencies-c==1.0.0; sys_platform == "darwin"''',
        ]
        requires-python = ">=3.8"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        revision = 1
        requires-python = ">=3.8"
        resolution-markers = [
            "sys_platform == 'linux'",
            "sys_platform == 'darwin'",
            "sys_platform != 'darwin' and sys_platform != 'linux'",
        ]

        [[package]]
        name = "package-a"
        version = "4.3.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform == 'darwin'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_filter_sibling_dependencies_a-4.3.0.tar.gz", hash = "sha256:5389f0927f61393ba8bd940622329299d769e79b725233604a6bdac0fd088c49" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_filter_sibling_dependencies_a-4.3.0-py3-none-any.whl", hash = "sha256:932c128393cd499617d1a5b457b11887d51039284b18e06add4c384ab661148c" },
        ]

        [[package]]
        name = "package-a"
        version = "4.4.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_filter_sibling_dependencies_a-4.4.0.tar.gz", hash = "sha256:7dbb8575aec8f87063954917b6ee628191cd53ca233ec810f6d926b4954e578b" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_filter_sibling_dependencies_a-4.4.0-py3-none-any.whl", hash = "sha256:26989734e8fa720896dbbf900adc64551bf3f0026fb62c3c22b47dc23edd4a4c" },
        ]

        [[package]]
        name = "package-b"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        dependencies = [
            { name = "package-d", version = "1.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'linux'" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_filter_sibling_dependencies_b-1.0.0.tar.gz", hash = "sha256:af3f861d6df9a2bbad55bae02acf17384ea2efa1abbf19206ac56cb021814613" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_filter_sibling_dependencies_b-1.0.0-py3-none-any.whl", hash = "sha256:bc72ef97f57a77fc7be9dc400be26ae5c344aabddbe39407c05a62e07554cdbf" },
        ]

        [[package]]
        name = "package-c"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        dependencies = [
            { name = "package-d", version = "2.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'darwin'" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_filter_sibling_dependencies_c-1.0.0.tar.gz", hash = "sha256:c03742ca6e81c2a5d7d8cb72d1214bf03b2925e63858a19097f17d3e1a750192" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_filter_sibling_dependencies_c-1.0.0-py3-none-any.whl", hash = "sha256:71fc9aec5527839358209ccb927186dd0529e9814a725d86aa17e7a033c59cd4" },
        ]

        [[package]]
        name = "package-d"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_filter_sibling_dependencies_d-1.0.0.tar.gz", hash = "sha256:cc1af60e53faf957fd0542349441ea79a909cd5feb30fb8933c39dc33404e4b2" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_filter_sibling_dependencies_d-1.0.0-py3-none-any.whl", hash = "sha256:de669ada03e9f8625e3ac4af637c88de04066a72675c16c3d1757e0e9d5db7a8" },
        ]

        [[package]]
        name = "package-d"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform == 'darwin'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_filter_sibling_dependencies_d-2.0.0.tar.gz", hash = "sha256:68e380efdea5206363f5397e4cd560a64f5f4927396dc0b6f6f36dd3f026281f" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_filter_sibling_dependencies_d-2.0.0-py3-none-any.whl", hash = "sha256:87c133dcc987137d62c011a41af31e8372ca971393d93f808dffc32e136363c7" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "package-a", version = "4.3.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'darwin'" },
            { name = "package-a", version = "4.4.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'linux'" },
            { name = "package-b", marker = "sys_platform == 'linux'" },
            { name = "package-c", marker = "sys_platform == 'darwin'" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "package-a", marker = "sys_platform == 'darwin'", specifier = "==4.3.0" },
            { name = "package-a", marker = "sys_platform == 'linux'", specifier = "==4.4.0" },
            { name = "package-b", marker = "sys_platform == 'linux'", specifier = "==1.0.0" },
            { name = "package-c", marker = "sys_platform == 'darwin'", specifier = "==1.0.0" },
        ]
        "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(packse_index_url())
        .assert()
        .success();

    Ok(())
}

/// This test checks that we discard fork markers when using `--upgrade`.
///
/// ```text
/// fork-upgrade
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires foo
/// │       ├── satisfied by foo-1.0.0
/// │       └── satisfied by foo-2.0.0
/// ├── bar
/// │   ├── bar-1.0.0
/// │   └── bar-2.0.0
/// └── foo
///     ├── foo-1.0.0
///     │   ├── requires bar==1; sys_platform == "linux"
///     │   │   └── satisfied by bar-1.0.0
///     │   └── requires bar==2; sys_platform != "linux"
///     │       └── satisfied by bar-2.0.0
///     └── foo-2.0.0
///         └── requires bar==2
///             └── satisfied by bar-2.0.0
/// ```
#[test]
fn fork_upgrade() -> Result<()> {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"fork-upgrade-", "package-"));

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''fork-upgrade-foo''',
        ]
        requires-python = ">=3.8"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        revision = 1
        requires-python = ">=3.8"

        [[package]]
        name = "package-bar"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_upgrade_bar-2.0.0.tar.gz", hash = "sha256:2e7b5370d7be19b5af56092a8364a2718a7b8516142a12a95656b82d1b9c8cbc" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_upgrade_bar-2.0.0-py3-none-any.whl", hash = "sha256:d8ce562bf363e849fbf4add170a519b5412ab63e378fb4b7ea290183c77616fc" },
        ]

        [[package]]
        name = "package-foo"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        dependencies = [
            { name = "package-bar" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_upgrade_foo-2.0.0.tar.gz", hash = "sha256:77296a92069aa604c7fe1d538cf1698dcdc35b4bc3a4a7b16503d520d871e67e" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_upgrade_foo-2.0.0-py3-none-any.whl", hash = "sha256:f066d0608d24ebdb2c23959810188e2f25947a3b492f1d4402ff203287efaf8a" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "package-foo" },
        ]

        [package.metadata]
        requires-dist = [{ name = "package-foo" }]
        "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(packse_index_url())
        .assert()
        .success();

    Ok(())
}

/// The root cause the resolver to fork over `a`, but the markers on the variant
/// of `a` don't cover the entire marker space, they are missing Python 3.10.
/// Later, we have a dependency this very hole, which we still need to select,
/// instead of having two forks around but without Python 3.10 and omitting
/// `c` from the solution.
///
/// ```text
/// fork-incomplete-markers
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   ├── requires a==1; python_version < "3.10"
/// │   │   └── satisfied by a-1.0.0
/// │   ├── requires a==2; python_version >= "3.11"
/// │   │   └── satisfied by a-2.0.0
/// │   └── requires b
/// │       └── satisfied by b-1.0.0
/// ├── a
/// │   ├── a-1.0.0
/// │   └── a-2.0.0
/// ├── b
/// │   └── b-1.0.0
/// │       └── requires c; python_version == "3.10"
/// │           └── satisfied by c-1.0.0
/// └── c
///     └── c-1.0.0
/// ```
#[test]
fn fork_incomplete_markers() -> Result<()> {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"fork-incomplete-markers-", "package-"));

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''fork-incomplete-markers-a==1; python_version < "3.10"''',
          '''fork-incomplete-markers-a==2; python_version >= "3.11"''',
          '''fork-incomplete-markers-b''',
        ]
        requires-python = ">=3.8"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        revision = 1
        requires-python = ">=3.8"
        resolution-markers = [
            "python_full_version >= '3.11'",
            "python_full_version == '3.10.*'",
            "python_full_version < '3.10'",
        ]

        [[package]]
        name = "package-a"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "python_full_version < '3.10'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_incomplete_markers_a-1.0.0.tar.gz", hash = "sha256:dd56de2e560b3f95c529c44cbdae55d9b1ada826ddd3e19d3ea45438224ad603" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_incomplete_markers_a-1.0.0-py3-none-any.whl", hash = "sha256:779bb805058fc59858e8b9260cd1a40f13f1640631fdea89d9d243691a4f39ca" },
        ]

        [[package]]
        name = "package-a"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "python_full_version >= '3.11'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_incomplete_markers_a-2.0.0.tar.gz", hash = "sha256:580f1454a172036c89f5cfbefe52f175b011806a61f243eb476526bcc186e0be" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_incomplete_markers_a-2.0.0-py3-none-any.whl", hash = "sha256:58a4b7dcf929aabf1ed434d9ff8d715929dc3dec02b92cf2b364d5a2206f1f6a" },
        ]

        [[package]]
        name = "package-b"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        dependencies = [
            { name = "package-c", marker = "python_full_version == '3.10.*'" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_incomplete_markers_b-1.0.0.tar.gz", hash = "sha256:c4deba44768923473d077bdc0e177033fcb6e6fd406d56830d7ee6f4ffad68c1" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_incomplete_markers_b-1.0.0-py3-none-any.whl", hash = "sha256:5c2a5f446580787ed7b3673431b112474237ddeaf1c81325bb30b86e7ee76adb" },
        ]

        [[package]]
        name = "package-c"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_incomplete_markers_c-1.0.0.tar.gz", hash = "sha256:ecc02ea1cc8d3b561c8dcb9d2ba1abcdae2dd32de608bf8e8ed2878118426022" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_incomplete_markers_c-1.0.0-py3-none-any.whl", hash = "sha256:03fa287aa4cb78457211cb3df7459b99ba1ee2259aae24bc745eaab45e7eaaee" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "package-a", version = "1.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "python_full_version < '3.10'" },
            { name = "package-a", version = "2.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "python_full_version >= '3.11'" },
            { name = "package-b" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "package-a", marker = "python_full_version < '3.10'", specifier = "==1" },
            { name = "package-a", marker = "python_full_version >= '3.11'", specifier = "==2" },
            { name = "package-b" },
        ]
        "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(packse_index_url())
        .assert()
        .success();

    Ok(())
}

/// This is actually a non-forking test case that tests the tracking of marker
/// expressions in general. In this case, the dependency on `c` should have its
/// marker expressions automatically combined. In this case, it's `linux OR
/// darwin`, even though `linux OR darwin` doesn't actually appear verbatim as a
/// marker expression for any dependency on `c`.
///
/// ```text
/// fork-marker-accrue
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   ├── requires a==1.0.0; implementation_name == "cpython"
/// │   │   └── satisfied by a-1.0.0
/// │   └── requires b==1.0.0; implementation_name == "pypy"
/// │       └── satisfied by b-1.0.0
/// ├── a
/// │   └── a-1.0.0
/// │       └── requires c==1.0.0; sys_platform == "linux"
/// │           └── satisfied by c-1.0.0
/// ├── b
/// │   └── b-1.0.0
/// │       └── requires c==1.0.0; sys_platform == "darwin"
/// │           └── satisfied by c-1.0.0
/// └── c
///     └── c-1.0.0
/// ```
#[test]
fn fork_marker_accrue() -> Result<()> {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"fork-marker-accrue-", "package-"));

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''fork-marker-accrue-a==1.0.0; implementation_name == "cpython"''',
          '''fork-marker-accrue-b==1.0.0; implementation_name == "pypy"''',
        ]
        requires-python = ">=3.8"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        revision = 1
        requires-python = ">=3.8"

        [[package]]
        name = "package-a"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        dependencies = [
            { name = "package-c", marker = "sys_platform == 'linux'" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_accrue_a-1.0.0.tar.gz", hash = "sha256:c791e6062a510c63bff857ca6f7a921a7795bfbc588f21a51124e091fb0343d6" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_accrue_a-1.0.0-py3-none-any.whl", hash = "sha256:cba9cb55cca41833a15c9f8eb75045236cf80cad5d663f7fb7ecae18dad79538" },
        ]

        [[package]]
        name = "package-b"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        dependencies = [
            { name = "package-c", marker = "sys_platform == 'darwin'" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_accrue_b-1.0.0.tar.gz", hash = "sha256:32e7ea1022061783857c3f6fec5051b4b320630fe8a5aec6523cd565db350387" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_accrue_b-1.0.0-py3-none-any.whl", hash = "sha256:c5202800c26be15ecaf5560e09ad1df710778bb9debd3c267be1c76f44fbc0c9" },
        ]

        [[package]]
        name = "package-c"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_accrue_c-1.0.0.tar.gz", hash = "sha256:a3e09ac3dc8e787a08ebe8d5d6072e09720c76cbbcb76a6645d6f59652742015" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_accrue_c-1.0.0-py3-none-any.whl", hash = "sha256:b0c8719d38c91b2a8548bd065b1d2153fbe031b37775ed244e76fe5bdfbb502e" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "package-a", marker = "implementation_name == 'cpython'" },
            { name = "package-b", marker = "implementation_name == 'pypy'" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "package-a", marker = "implementation_name == 'cpython'", specifier = "==1.0.0" },
            { name = "package-b", marker = "implementation_name == 'pypy'", specifier = "==1.0.0" },
        ]
        "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(packse_index_url())
        .assert()
        .success();

    Ok(())
}

/// A basic test that ensures, at least in this one basic case, that forking in
/// universal resolution happens only when the corresponding marker expressions are
/// completely disjoint. Here, we provide two completely incompatible dependency
/// specifications with equivalent markers. Thus, they are trivially not disjoint,
/// and resolution should fail.
///
/// NOTE: This acts a regression test for the initial version of universal
/// resolution that would fork whenever a package was repeated in the list of
/// dependency specifications. So previously, this would produce a resolution with
/// both `1.0.0` and `2.0.0` of `a`. But of course, the correct behavior is to fail
/// resolving.
///
/// ```text
/// fork-marker-disjoint
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   ├── requires a>=2; sys_platform == "linux"
/// │   │   └── satisfied by a-2.0.0
/// │   └── requires a<2; sys_platform == "linux"
/// │       └── satisfied by a-1.0.0
/// └── a
///     ├── a-1.0.0
///     └── a-2.0.0
/// ```
#[test]
fn fork_marker_disjoint() -> Result<()> {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"fork-marker-disjoint-", "package-"));

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''fork-marker-disjoint-a>=2; sys_platform == "linux"''',
          '''fork-marker-disjoint-a<2; sys_platform == "linux"''',
        ]
        requires-python = ">=3.8"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because your project depends on package-a{sys_platform == 'linux'}>=2 and package-a{sys_platform == 'linux'}<2, we can conclude that your project's requirements are unsatisfiable.
    "###
    );

    Ok(())
}

/// This test builds on `fork-marker-inherit-combined`. Namely, we add
/// `or implementation_name == 'pypy'` to the dependency on `c`. While
/// `sys_platform == 'linux'` cannot be true because of the first fork,
/// the second fork which includes `b==1.0.0` happens precisely when
/// `implementation_name == 'pypy'`. So in this case, `c` should be
/// included.
///
/// ```text
/// fork-marker-inherit-combined-allowed
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   ├── requires a>=2; sys_platform == "linux"
/// │   │   └── satisfied by a-2.0.0
/// │   └── requires a<2; sys_platform == "darwin"
/// │       └── satisfied by a-1.0.0
/// ├── a
/// │   ├── a-1.0.0
/// │   │   ├── requires b>=2; implementation_name == "cpython"
/// │   │   │   └── satisfied by b-2.0.0
/// │   │   └── requires b<2; implementation_name == "pypy"
/// │   │       └── satisfied by b-1.0.0
/// │   └── a-2.0.0
/// ├── b
/// │   ├── b-1.0.0
/// │   │   └── requires c; sys_platform == "linux" or implementation_name == "pypy"
/// │   │       └── satisfied by c-1.0.0
/// │   └── b-2.0.0
/// └── c
///     └── c-1.0.0
/// ```
#[test]
fn fork_marker_inherit_combined_allowed() -> Result<()> {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"fork-marker-inherit-combined-allowed-", "package-"));

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''fork-marker-inherit-combined-allowed-a>=2; sys_platform == "linux"''',
          '''fork-marker-inherit-combined-allowed-a<2; sys_platform == "darwin"''',
        ]
        requires-python = ">=3.8"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        revision = 1
        requires-python = ">=3.8"
        resolution-markers = [
            "implementation_name == 'pypy' and sys_platform == 'darwin'",
            "implementation_name == 'cpython' and sys_platform == 'darwin'",
            "implementation_name != 'cpython' and implementation_name != 'pypy' and sys_platform == 'darwin'",
            "sys_platform == 'linux'",
            "sys_platform != 'darwin' and sys_platform != 'linux'",
        ]

        [[package]]
        name = "package-a"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "implementation_name == 'pypy' and sys_platform == 'darwin'",
            "implementation_name == 'cpython' and sys_platform == 'darwin'",
            "implementation_name != 'cpython' and implementation_name != 'pypy' and sys_platform == 'darwin'",
        ]
        dependencies = [
            { name = "package-b", version = "1.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "implementation_name == 'pypy' and sys_platform == 'darwin'" },
            { name = "package-b", version = "2.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "implementation_name == 'cpython' and sys_platform == 'darwin'" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_allowed_a-1.0.0.tar.gz", hash = "sha256:c7232306e8597d46c3fe53a3b1472f99b8ff36b3169f335ba0a5b625e193f7d4" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_allowed_a-1.0.0-py3-none-any.whl", hash = "sha256:198ae54c02a59734dc009bfcee1148d40f56c605b62f9f1a00467e09ebf2ff07" },
        ]

        [[package]]
        name = "package-a"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_allowed_a-2.0.0.tar.gz", hash = "sha256:0dcb58c8276afbe439e1c94708fb71954fb8869cc65c230ce8f462c911992ceb" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_allowed_a-2.0.0-py3-none-any.whl", hash = "sha256:61b7d273468584342de4c0185beed5b128797ce95ec9ec4a670fe30f73351cf7" },
        ]

        [[package]]
        name = "package-b"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "implementation_name == 'pypy' and sys_platform == 'darwin'",
        ]
        dependencies = [
            { name = "package-c", marker = "implementation_name == 'pypy' and sys_platform == 'darwin'" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_allowed_b-1.0.0.tar.gz", hash = "sha256:d6bd196a0a152c1b32e09f08e554d22ae6a6b3b916e39ad4552572afae5f5492" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_allowed_b-1.0.0-py3-none-any.whl", hash = "sha256:e1deba885509945ef087e4f31c7dba3ee436fc8284b360fe207a3c42f2f9e22f" },
        ]

        [[package]]
        name = "package-b"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "implementation_name == 'cpython' and sys_platform == 'darwin'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_allowed_b-2.0.0.tar.gz", hash = "sha256:4533845ba671575a25ceb32f10f0bc6836949bef37b7da6e7dd37d9be389871c" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_allowed_b-2.0.0-py3-none-any.whl", hash = "sha256:736d1b59cb46a0b889614bc7557c293de245fe8954e3200e786500ae8e42504b" },
        ]

        [[package]]
        name = "package-c"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_allowed_c-1.0.0.tar.gz", hash = "sha256:7ce8efca029cfa952e64f55c2d47fe33975c7f77ec689384bda11cbc3b7ef1db" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_allowed_c-1.0.0-py3-none-any.whl", hash = "sha256:6a6b776dedabceb6a6c4f54a5d932076fa3fed1380310491999ca2d31e13b41c" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "package-a", version = "1.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'darwin'" },
            { name = "package-a", version = "2.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'linux'" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "package-a", marker = "sys_platform == 'darwin'", specifier = "<2" },
            { name = "package-a", marker = "sys_platform == 'linux'", specifier = ">=2" },
        ]
        "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(packse_index_url())
        .assert()
        .success();

    Ok(())
}

/// This test builds on `fork-marker-inherit-combined`. Namely, we add
/// `or implementation_name == 'cpython'` to the dependency on `c`.
/// While `sys_platform == 'linux'` cannot be true because of the first
/// fork, the second fork which includes `b==1.0.0` happens precisely
/// when `implementation_name == 'pypy'`, which is *also* disjoint with
/// `implementation_name == 'cpython'`. Therefore, `c` should not be
/// included here.
///
/// ```text
/// fork-marker-inherit-combined-disallowed
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   ├── requires a>=2; sys_platform == "linux"
/// │   │   └── satisfied by a-2.0.0
/// │   └── requires a<2; sys_platform == "darwin"
/// │       └── satisfied by a-1.0.0
/// ├── a
/// │   ├── a-1.0.0
/// │   │   ├── requires b>=2; implementation_name == "cpython"
/// │   │   │   └── satisfied by b-2.0.0
/// │   │   └── requires b<2; implementation_name == "pypy"
/// │   │       └── satisfied by b-1.0.0
/// │   └── a-2.0.0
/// ├── b
/// │   ├── b-1.0.0
/// │   │   └── requires c; sys_platform == "linux" or implementation_name == "cpython"
/// │   │       └── satisfied by c-1.0.0
/// │   └── b-2.0.0
/// └── c
///     └── c-1.0.0
/// ```
#[test]
fn fork_marker_inherit_combined_disallowed() -> Result<()> {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"fork-marker-inherit-combined-disallowed-", "package-"));

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''fork-marker-inherit-combined-disallowed-a>=2; sys_platform == "linux"''',
          '''fork-marker-inherit-combined-disallowed-a<2; sys_platform == "darwin"''',
        ]
        requires-python = ">=3.8"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        revision = 1
        requires-python = ">=3.8"
        resolution-markers = [
            "implementation_name == 'pypy' and sys_platform == 'darwin'",
            "implementation_name == 'cpython' and sys_platform == 'darwin'",
            "implementation_name != 'cpython' and implementation_name != 'pypy' and sys_platform == 'darwin'",
            "sys_platform == 'linux'",
            "sys_platform != 'darwin' and sys_platform != 'linux'",
        ]

        [[package]]
        name = "package-a"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "implementation_name == 'pypy' and sys_platform == 'darwin'",
            "implementation_name == 'cpython' and sys_platform == 'darwin'",
            "implementation_name != 'cpython' and implementation_name != 'pypy' and sys_platform == 'darwin'",
        ]
        dependencies = [
            { name = "package-b", version = "1.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "implementation_name == 'pypy' and sys_platform == 'darwin'" },
            { name = "package-b", version = "2.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "implementation_name == 'cpython' and sys_platform == 'darwin'" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_disallowed_a-1.0.0.tar.gz", hash = "sha256:92081d91570582f3a94ed156f203de53baca5b3fdc350aa1c831c7c42723e798" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_disallowed_a-1.0.0-py3-none-any.whl", hash = "sha256:ee2dc68d5b33c0318183431cebf99ccca63d98601b936e5d3eae804c73f2b154" },
        ]

        [[package]]
        name = "package-a"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_disallowed_a-2.0.0.tar.gz", hash = "sha256:9d48383b0699f575af15871f6f7a928b835cd5ad4e13f91a675ee5aba722dabc" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_disallowed_a-2.0.0-py3-none-any.whl", hash = "sha256:099db8d3af6c9dfc10589ab0f1e2e6a74276a167afb39322ddaf657791247456" },
        ]

        [[package]]
        name = "package-b"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "implementation_name == 'pypy' and sys_platform == 'darwin'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_disallowed_b-1.0.0.tar.gz", hash = "sha256:d44b87bd8d39240bca55eaae84a245e74197ed0b7897c27af9f168c713cc63bd" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_disallowed_b-1.0.0-py3-none-any.whl", hash = "sha256:999b3d0029ea0131272257e2b04c0e673defa6c25be6efc411e04936bce72ef6" },
        ]

        [[package]]
        name = "package-b"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "implementation_name == 'cpython' and sys_platform == 'darwin'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_disallowed_b-2.0.0.tar.gz", hash = "sha256:75a48bf2d44a0a0be6ca33820f5076665765be7b43dabf5f94f7fd5247071097" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_disallowed_b-2.0.0-py3-none-any.whl", hash = "sha256:2c6aedd257d0ed21bb96f6e0baba8314c001d4078d09413cda147fb6badb39a2" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "package-a", version = "1.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'darwin'" },
            { name = "package-a", version = "2.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'linux'" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "package-a", marker = "sys_platform == 'darwin'", specifier = "<2" },
            { name = "package-a", marker = "sys_platform == 'linux'", specifier = ">=2" },
        ]
        "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(packse_index_url())
        .assert()
        .success();

    Ok(())
}

/// In this test, we check that marker expressions which provoke a fork
/// are carried through to subsequent forks. Here, the `a>=2` and `a<2`
/// dependency specifications create a fork, and then the `a<2` fork leads
/// to `a==1.0.0` with dependency specifications on `b>=2` and `b<2` that
/// provoke yet another fork. Finally, in the `b<2` fork, a dependency on
/// `c` is introduced whose marker expression is disjoint with the marker
/// expression that provoked the *first* fork. Therefore, `c` should be
/// entirely excluded from the resolution.
///
/// ```text
/// fork-marker-inherit-combined
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   ├── requires a>=2; sys_platform == "linux"
/// │   │   └── satisfied by a-2.0.0
/// │   └── requires a<2; sys_platform == "darwin"
/// │       └── satisfied by a-1.0.0
/// ├── a
/// │   ├── a-1.0.0
/// │   │   ├── requires b>=2; implementation_name == "cpython"
/// │   │   │   └── satisfied by b-2.0.0
/// │   │   └── requires b<2; implementation_name == "pypy"
/// │   │       └── satisfied by b-1.0.0
/// │   └── a-2.0.0
/// ├── b
/// │   ├── b-1.0.0
/// │   │   └── requires c; sys_platform == "linux"
/// │   │       └── satisfied by c-1.0.0
/// │   └── b-2.0.0
/// └── c
///     └── c-1.0.0
/// ```
#[test]
fn fork_marker_inherit_combined() -> Result<()> {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"fork-marker-inherit-combined-", "package-"));

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''fork-marker-inherit-combined-a>=2; sys_platform == "linux"''',
          '''fork-marker-inherit-combined-a<2; sys_platform == "darwin"''',
        ]
        requires-python = ">=3.8"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        revision = 1
        requires-python = ">=3.8"
        resolution-markers = [
            "implementation_name == 'pypy' and sys_platform == 'darwin'",
            "implementation_name == 'cpython' and sys_platform == 'darwin'",
            "implementation_name != 'cpython' and implementation_name != 'pypy' and sys_platform == 'darwin'",
            "sys_platform == 'linux'",
            "sys_platform != 'darwin' and sys_platform != 'linux'",
        ]

        [[package]]
        name = "package-a"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "implementation_name == 'pypy' and sys_platform == 'darwin'",
            "implementation_name == 'cpython' and sys_platform == 'darwin'",
            "implementation_name != 'cpython' and implementation_name != 'pypy' and sys_platform == 'darwin'",
        ]
        dependencies = [
            { name = "package-b", version = "1.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "implementation_name == 'pypy' and sys_platform == 'darwin'" },
            { name = "package-b", version = "2.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "implementation_name == 'cpython' and sys_platform == 'darwin'" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_a-1.0.0.tar.gz", hash = "sha256:2ec4c9dbb7078227d996c344b9e0c1b365ed0000de9527b2ba5b616233636f07" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_a-1.0.0-py3-none-any.whl", hash = "sha256:1150f6d977824bc0260cfb5fcf34816424ed4602d5df316c291b8df3f723c888" },
        ]

        [[package]]
        name = "package-a"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_a-2.0.0.tar.gz", hash = "sha256:47958d1659220ee7722b0f26e8c1fe41217a2816881ffa929f0ba794a87ceebf" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_a-2.0.0-py3-none-any.whl", hash = "sha256:67e142d749674a27c872db714d50fda083010789da51291e3c30b4daf0e96b3b" },
        ]

        [[package]]
        name = "package-b"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "implementation_name == 'pypy' and sys_platform == 'darwin'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_b-1.0.0.tar.gz", hash = "sha256:6992d194cb5a0f0eed9ed6617d3212af4e3ff09274bf7622c8a1008b072128da" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_b-1.0.0-py3-none-any.whl", hash = "sha256:d9b50d8a0968d65af338e27d6b2a58eea59c514e47b820752a2c068b5a8333a7" },
        ]

        [[package]]
        name = "package-b"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "implementation_name == 'cpython' and sys_platform == 'darwin'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_b-2.0.0.tar.gz", hash = "sha256:e340061505d621a340d10ec1dbaf02dfce0c66358ee8190f61f78018f9999989" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_b-2.0.0-py3-none-any.whl", hash = "sha256:ff364fd590d05651579d8bea621b069934470106b9a82ab960fb93dfd88ea038" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "package-a", version = "1.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'darwin'" },
            { name = "package-a", version = "2.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'linux'" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "package-a", marker = "sys_platform == 'darwin'", specifier = "<2" },
            { name = "package-a", marker = "sys_platform == 'linux'", specifier = ">=2" },
        ]
        "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(packse_index_url())
        .assert()
        .success();

    Ok(())
}

/// This is like `fork-marker-inherit`, but where both `a>=2` and `a<2`
/// have a conditional dependency on `b`. For `a>=2`, the conditional
/// dependency on `b` has overlap with the `a>=2` marker expression, and
/// thus, `b` should be included *only* in the dependencies for `a==2.0.0`.
/// As with `fork-marker-inherit`, the `a<2` path should exclude `b==1.0.0`
/// since their marker expressions are disjoint.
///
/// ```text
/// fork-marker-inherit-isolated
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   ├── requires a>=2; sys_platform == "linux"
/// │   │   └── satisfied by a-2.0.0
/// │   └── requires a<2; sys_platform == "darwin"
/// │       └── satisfied by a-1.0.0
/// ├── a
/// │   ├── a-1.0.0
/// │   │   └── requires b; sys_platform == "linux"
/// │   │       └── satisfied by b-1.0.0
/// │   └── a-2.0.0
/// │       └── requires b; sys_platform == "linux"
/// │           └── satisfied by b-1.0.0
/// └── b
///     └── b-1.0.0
/// ```
#[test]
fn fork_marker_inherit_isolated() -> Result<()> {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"fork-marker-inherit-isolated-", "package-"));

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''fork-marker-inherit-isolated-a>=2; sys_platform == "linux"''',
          '''fork-marker-inherit-isolated-a<2; sys_platform == "darwin"''',
        ]
        requires-python = ">=3.8"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        revision = 1
        requires-python = ">=3.8"
        resolution-markers = [
            "sys_platform == 'darwin'",
            "sys_platform == 'linux'",
            "sys_platform != 'darwin' and sys_platform != 'linux'",
        ]

        [[package]]
        name = "package-a"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform == 'darwin'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_isolated_a-1.0.0.tar.gz", hash = "sha256:724ffc24debfa2bc6b5c2457df777c523638ec3586cc953f8509dad581aa6887" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_isolated_a-1.0.0-py3-none-any.whl", hash = "sha256:6823b88bf6debf2ec6195d82943c2812235a642438f007c0a3c95d745a5b95ba" },
        ]

        [[package]]
        name = "package-a"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        dependencies = [
            { name = "package-b", marker = "sys_platform == 'linux'" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_isolated_a-2.0.0.tar.gz", hash = "sha256:bc4567da4349a9c09b7fb4733f0b9f6476249240192291cf051c2b1d28b829fd" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_isolated_a-2.0.0-py3-none-any.whl", hash = "sha256:16986b43ef61e3f639b61fc9c22ede133864606d3d72716161a59acd64793c02" },
        ]

        [[package]]
        name = "package-b"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_isolated_b-1.0.0.tar.gz", hash = "sha256:96f8c3cabc5795e08a064c89ec76a4bfba8afe3c13d647161b4a1568b4584ced" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_isolated_b-1.0.0-py3-none-any.whl", hash = "sha256:c8affc2f13f9bcd08b3d1601a21a1781ea14d52a8cddc708b29428c9c3d53ea5" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "package-a", version = "1.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'darwin'" },
            { name = "package-a", version = "2.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'linux'" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "package-a", marker = "sys_platform == 'darwin'", specifier = "<2" },
            { name = "package-a", marker = "sys_platform == 'linux'", specifier = ">=2" },
        ]
        "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(packse_index_url())
        .assert()
        .success();

    Ok(())
}

/// This is like `fork-marker-inherit`, but tests that the marker
/// expressions that provoke a fork are carried transitively through the
/// dependency graph. In this case, `a<2 -> b -> c -> d`, but where the
/// last dependency on `d` requires a marker expression that is disjoint
/// with the initial `a<2` dependency. Therefore, it ought to be completely
/// excluded from the resolution.
///
/// ```text
/// fork-marker-inherit-transitive
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   ├── requires a>=2; sys_platform == "linux"
/// │   │   └── satisfied by a-2.0.0
/// │   └── requires a<2; sys_platform == "darwin"
/// │       └── satisfied by a-1.0.0
/// ├── a
/// │   ├── a-1.0.0
/// │   │   └── requires b
/// │   │       └── satisfied by b-1.0.0
/// │   └── a-2.0.0
/// ├── b
/// │   └── b-1.0.0
/// │       └── requires c
/// │           └── satisfied by c-1.0.0
/// ├── c
/// │   └── c-1.0.0
/// │       └── requires d; sys_platform == "linux"
/// │           └── satisfied by d-1.0.0
/// └── d
///     └── d-1.0.0
/// ```
#[test]
fn fork_marker_inherit_transitive() -> Result<()> {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"fork-marker-inherit-transitive-", "package-"));

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''fork-marker-inherit-transitive-a>=2; sys_platform == "linux"''',
          '''fork-marker-inherit-transitive-a<2; sys_platform == "darwin"''',
        ]
        requires-python = ">=3.8"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        revision = 1
        requires-python = ">=3.8"
        resolution-markers = [
            "sys_platform == 'darwin'",
            "sys_platform == 'linux'",
            "sys_platform != 'darwin' and sys_platform != 'linux'",
        ]

        [[package]]
        name = "package-a"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform == 'darwin'",
        ]
        dependencies = [
            { name = "package-b", marker = "sys_platform == 'darwin'" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_transitive_a-1.0.0.tar.gz", hash = "sha256:8bcab85231487b9350471da0c4c22dc3d69dfe4a1198d16b5f81b0235d7112ce" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_transitive_a-1.0.0-py3-none-any.whl", hash = "sha256:84d650ff1a909198ba82cbe0f697e836d8a570fb71faa6ad4a30c4df332dfde6" },
        ]

        [[package]]
        name = "package-a"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_transitive_a-2.0.0.tar.gz", hash = "sha256:4437ac14c340fec0b451cbc9486f5b8e106568634264ecad339a8de565a93be6" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_transitive_a-2.0.0-py3-none-any.whl", hash = "sha256:420c4c6b02d22c33f7f8ae9f290acc5b4c372fc2e49c881d259237a31c76dc0b" },
        ]

        [[package]]
        name = "package-b"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        dependencies = [
            { name = "package-c", marker = "sys_platform == 'darwin'" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_transitive_b-1.0.0.tar.gz", hash = "sha256:03b4b0e323c36bd4a1e51a65e1489715da231d44d26e12b54544e3bf9a9f6129" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_transitive_b-1.0.0-py3-none-any.whl", hash = "sha256:c9738afccc13d7d5bd7be85abf5dc77f88c43c577fb2f90dfa2abf1ffa0c8db6" },
        ]

        [[package]]
        name = "package-c"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_transitive_c-1.0.0.tar.gz", hash = "sha256:58bb788896b2297f2948f51a27fc48cfe44057c687a3c0c4d686b107975f7f32" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_transitive_c-1.0.0-py3-none-any.whl", hash = "sha256:ad2cbb0582ec6f4dc9549d1726d2aae66cd1fdf0e355acc70cd720cf65ae4d86" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "package-a", version = "1.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'darwin'" },
            { name = "package-a", version = "2.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'linux'" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "package-a", marker = "sys_platform == 'darwin'", specifier = "<2" },
            { name = "package-a", marker = "sys_platform == 'linux'", specifier = ">=2" },
        ]
        "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(packse_index_url())
        .assert()
        .success();

    Ok(())
}

/// This tests that markers which provoked a fork in the universal resolver
/// are used to ignore dependencies which cannot possibly be installed by a
/// resolution produced by that fork.
///
/// In this example, the `a<2` dependency is only active on Darwin
/// platforms. But the `a==1.0.0` distribution has a dependency on `b`
/// that is only active on Linux, where as `a==2.0.0` does not. Therefore,
/// when the fork provoked by the `a<2` dependency considers `b`, it should
/// ignore it because it isn't possible for `sys_platform == 'linux'` and
/// `sys_platform == 'darwin'` to be simultaneously true.
///
/// ```text
/// fork-marker-inherit
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   ├── requires a>=2; sys_platform == "linux"
/// │   │   └── satisfied by a-2.0.0
/// │   └── requires a<2; sys_platform == "darwin"
/// │       └── satisfied by a-1.0.0
/// ├── a
/// │   ├── a-1.0.0
/// │   │   └── requires b; sys_platform == "linux"
/// │   │       └── satisfied by b-1.0.0
/// │   └── a-2.0.0
/// └── b
///     └── b-1.0.0
/// ```
#[test]
fn fork_marker_inherit() -> Result<()> {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"fork-marker-inherit-", "package-"));

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''fork-marker-inherit-a>=2; sys_platform == "linux"''',
          '''fork-marker-inherit-a<2; sys_platform == "darwin"''',
        ]
        requires-python = ">=3.8"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        revision = 1
        requires-python = ">=3.8"
        resolution-markers = [
            "sys_platform == 'darwin'",
            "sys_platform == 'linux'",
            "sys_platform != 'darwin' and sys_platform != 'linux'",
        ]

        [[package]]
        name = "package-a"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform == 'darwin'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_a-1.0.0.tar.gz", hash = "sha256:177511ec69a2f04de39867d43f167a33194ae983e8f86a1cc9b51f59fc379d4b" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_a-1.0.0-py3-none-any.whl", hash = "sha256:16447932477c5feaa874b4e7510023c6f732578cec07158bc0e872af887a77d6" },
        ]

        [[package]]
        name = "package-a"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_a-2.0.0.tar.gz", hash = "sha256:43e24ce6fcbfbbff1db5eb20b583c20c2aa0888138bfafeab205c4ccc6e7e0a4" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_a-2.0.0-py3-none-any.whl", hash = "sha256:d650b6acf8f68d85e210ceb3e7802fbe84aad2b918b06a72dee534fe5474852b" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "package-a", version = "1.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'darwin'" },
            { name = "package-a", version = "2.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'linux'" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "package-a", marker = "sys_platform == 'darwin'", specifier = "<2" },
            { name = "package-a", marker = "sys_platform == 'linux'", specifier = ">=2" },
        ]
        "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(packse_index_url())
        .assert()
        .success();

    Ok(())
}

/// This is like `fork-marker-inherit`, but it tests that dependency
/// filtering only occurs in the context of a fork.
///
/// For example, as in `fork-marker-inherit`, the `c` dependency of
/// `a<2` should be entirely excluded here since it is possible for
/// `sys_platform` to be simultaneously equivalent to Darwin and Linux.
/// However, the unconditional dependency on `b`, which in turn depends on
/// `c` for Linux only, should still incorporate `c` as the dependency is
/// not part of any fork.
///
/// ```text
/// fork-marker-limited-inherit
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   ├── requires a>=2; sys_platform == "linux"
/// │   │   └── satisfied by a-2.0.0
/// │   ├── requires a<2; sys_platform == "darwin"
/// │   │   └── satisfied by a-1.0.0
/// │   └── requires b
/// │       └── satisfied by b-1.0.0
/// ├── a
/// │   ├── a-1.0.0
/// │   │   └── requires c; sys_platform == "linux"
/// │   │       └── satisfied by c-1.0.0
/// │   └── a-2.0.0
/// ├── b
/// │   └── b-1.0.0
/// │       └── requires c; sys_platform == "linux"
/// │           └── satisfied by c-1.0.0
/// └── c
///     └── c-1.0.0
/// ```
#[test]
fn fork_marker_limited_inherit() -> Result<()> {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"fork-marker-limited-inherit-", "package-"));

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''fork-marker-limited-inherit-a>=2; sys_platform == "linux"''',
          '''fork-marker-limited-inherit-a<2; sys_platform == "darwin"''',
          '''fork-marker-limited-inherit-b''',
        ]
        requires-python = ">=3.8"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        revision = 1
        requires-python = ">=3.8"
        resolution-markers = [
            "sys_platform == 'darwin'",
            "sys_platform == 'linux'",
            "sys_platform != 'darwin' and sys_platform != 'linux'",
        ]

        [[package]]
        name = "package-a"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform == 'darwin'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_limited_inherit_a-1.0.0.tar.gz", hash = "sha256:ab1fde8d0acb9a2fe99b7a005939962b1c26c6d876e4a55e81fb9d1a1e5e9f76" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_limited_inherit_a-1.0.0-py3-none-any.whl", hash = "sha256:0dcb9659eeb891701535005a2afd7c579f566d3908e96137db74129924ae6a7e" },
        ]

        [[package]]
        name = "package-a"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_limited_inherit_a-2.0.0.tar.gz", hash = "sha256:009fdb8872cf52324c1bcdebef31feaba3c262fd76d150a753152aeee3d55b10" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_limited_inherit_a-2.0.0-py3-none-any.whl", hash = "sha256:10957fddbd5611e0db154744a01d588c7105e26fd5f6a8150956ca9542d844c5" },
        ]

        [[package]]
        name = "package-b"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        dependencies = [
            { name = "package-c", marker = "sys_platform == 'linux'" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_limited_inherit_b-1.0.0.tar.gz", hash = "sha256:4c04e090df03e308ecd38a9b8db9813a09fb20a747a89f86c497702c3e5a9001" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_limited_inherit_b-1.0.0-py3-none-any.whl", hash = "sha256:17365faaf25dba08be579867f219f914a0ff3298441f8d7b6201625a253333ec" },
        ]

        [[package]]
        name = "package-c"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_limited_inherit_c-1.0.0.tar.gz", hash = "sha256:8dcb05f5dff09fec52ab507b215ff367fe815848319a17929db997ad3afe88ae" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_limited_inherit_c-1.0.0-py3-none-any.whl", hash = "sha256:877a87a4987ad795ddaded3e7266ed7defdd3cfbe07a29500cb6047637db4065" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "package-a", version = "1.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'darwin'" },
            { name = "package-a", version = "2.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'linux'" },
            { name = "package-b" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "package-a", marker = "sys_platform == 'darwin'", specifier = "<2" },
            { name = "package-a", marker = "sys_platform == 'linux'", specifier = ">=2" },
            { name = "package-b" },
        ]
        "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(packse_index_url())
        .assert()
        .success();

    Ok(())
}

/// This tests a case where the resolver forks because of non-overlapping marker
/// expressions on `b`. In the original universal resolver implementation, this
/// resulted in multiple versions of `a` being unconditionally included in the lock
/// file. So this acts as a regression test to ensure that only one version of `a`
/// is selected.
///
/// ```text
/// fork-marker-selection
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   ├── requires a
/// │   │   ├── satisfied by a-0.1.0
/// │   │   └── satisfied by a-0.2.0
/// │   ├── requires b>=2; sys_platform == "linux"
/// │   │   └── satisfied by b-2.0.0
/// │   └── requires b<2; sys_platform == "darwin"
/// │       └── satisfied by b-1.0.0
/// ├── a
/// │   ├── a-0.1.0
/// │   └── a-0.2.0
/// │       └── requires b>=2.0.0
/// │           └── satisfied by b-2.0.0
/// └── b
///     ├── b-1.0.0
///     └── b-2.0.0
/// ```
#[test]
fn fork_marker_selection() -> Result<()> {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"fork-marker-selection-", "package-"));

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''fork-marker-selection-a''',
          '''fork-marker-selection-b>=2; sys_platform == "linux"''',
          '''fork-marker-selection-b<2; sys_platform == "darwin"''',
        ]
        requires-python = ">=3.8"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        revision = 1
        requires-python = ">=3.8"
        resolution-markers = [
            "sys_platform == 'darwin'",
            "sys_platform == 'linux'",
            "sys_platform != 'darwin' and sys_platform != 'linux'",
        ]

        [[package]]
        name = "package-a"
        version = "0.1.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_selection_a-0.1.0.tar.gz", hash = "sha256:ece83ba864a62d5d747439f79a0bf36aa4c18d15bca96aab855ffc2e94a8eef7" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_selection_a-0.1.0-py3-none-any.whl", hash = "sha256:a3b9d6e46cc226d20994cc60653fd59d81d96527749f971a6f59ef8cbcbc7c01" },
        ]

        [[package]]
        name = "package-b"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform == 'darwin'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_selection_b-1.0.0.tar.gz", hash = "sha256:6f5ea28cadb8b5dfa15d32c9e38818f8f7150fc4f9a58e49aec4e10b23342be4" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_selection_b-1.0.0-py3-none-any.whl", hash = "sha256:5eb8c7fc25dfe94c8a3b71bc09eadb8cd4c7e55b974cee851b848c3856d6a4f9" },
        ]

        [[package]]
        name = "package-b"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_selection_b-2.0.0.tar.gz", hash = "sha256:d32033ecdf37d605e4b3b3e88df6562bb7ca01c6ed3fb9a55ec078eccc1df9d1" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_selection_b-2.0.0-py3-none-any.whl", hash = "sha256:163fbcd238a66243064d41bd383657a63e45155f63bf92668c23af5245307380" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "package-a" },
            { name = "package-b", version = "1.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'darwin'" },
            { name = "package-b", version = "2.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'linux'" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "package-a" },
            { name = "package-b", marker = "sys_platform == 'darwin'", specifier = "<2" },
            { name = "package-b", marker = "sys_platform == 'linux'", specifier = ">=2" },
        ]
        "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(packse_index_url())
        .assert()
        .success();

    Ok(())
}

///
///
/// ```text
/// fork-marker-track
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   ├── requires a
/// │   │   ├── satisfied by a-1.3.1
/// │   │   ├── satisfied by a-2.0.0
/// │   │   ├── satisfied by a-3.1.0
/// │   │   └── satisfied by a-4.3.0
/// │   ├── requires b>=2.8; sys_platform == "linux"
/// │   │   └── satisfied by b-2.8
/// │   └── requires b<2.8; sys_platform == "darwin"
/// │       └── satisfied by b-2.7
/// ├── a
/// │   ├── a-1.3.1
/// │   │   └── requires c; implementation_name == "iron"
/// │   │       └── satisfied by c-1.10
/// │   ├── a-2.0.0
/// │   │   ├── requires b>=2.8
/// │   │   │   └── satisfied by b-2.8
/// │   │   └── requires c; implementation_name == "cpython"
/// │   │       └── satisfied by c-1.10
/// │   ├── a-3.1.0
/// │   │   ├── requires b>=2.8
/// │   │   │   └── satisfied by b-2.8
/// │   │   └── requires c; implementation_name == "pypy"
/// │   │       └── satisfied by c-1.10
/// │   └── a-4.3.0
/// │       └── requires b>=2.8
/// │           └── satisfied by b-2.8
/// ├── b
/// │   ├── b-2.7
/// │   └── b-2.8
/// └── c
///     └── c-1.10
/// ```
#[test]
fn fork_marker_track() -> Result<()> {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"fork-marker-track-", "package-"));

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''fork-marker-track-a''',
          '''fork-marker-track-b>=2.8; sys_platform == "linux"''',
          '''fork-marker-track-b<2.8; sys_platform == "darwin"''',
        ]
        requires-python = ">=3.8"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        revision = 1
        requires-python = ">=3.8"
        resolution-markers = [
            "sys_platform == 'darwin'",
            "sys_platform == 'linux'",
            "sys_platform != 'darwin' and sys_platform != 'linux'",
        ]

        [[package]]
        name = "package-a"
        version = "1.3.1"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        dependencies = [
            { name = "package-c", marker = "implementation_name == 'iron'" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_track_a-1.3.1.tar.gz", hash = "sha256:ffc490c887058825e96a0cc4a270cf56b72f7f28b927c450086603317bb8c120" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_track_a-1.3.1-py3-none-any.whl", hash = "sha256:d9dc6a64400a041199df2d37182582ff7cc986bac486da273d814627e9b86648" },
        ]

        [[package]]
        name = "package-b"
        version = "2.7"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform == 'darwin'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_track_b-2.7.tar.gz", hash = "sha256:855bf45837a4ba669a5850b14b0253cb138925fdd2b06a2f15c6582b8fabb8a0" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_track_b-2.7-py3-none-any.whl", hash = "sha256:544eb2b567d2293c47da724af91fec59c2d3e06675617d29068864ec3a4e390f" },
        ]

        [[package]]
        name = "package-b"
        version = "2.8"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_track_b-2.8.tar.gz", hash = "sha256:2e14b0ff1fb7f5cf491bd31d876218adee1d6a208ff197dc30363cdf25262e80" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_track_b-2.8-py3-none-any.whl", hash = "sha256:5aba691ce804ee39b2464c7757f8680786a1468e152ee845ff841c37f8112e21" },
        ]

        [[package]]
        name = "package-c"
        version = "1.10"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_track_c-1.10.tar.gz", hash = "sha256:c89006d893254790b0fcdd1b33520241c8ff66ab950c6752b745e006bdeff144" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_track_c-1.10-py3-none-any.whl", hash = "sha256:cedcb8fbcdd9fbde4eea76612e57536c8b56507a9d7f7a92e483cb56b18c57a3" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "package-a" },
            { name = "package-b", version = "2.7", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'darwin'" },
            { name = "package-b", version = "2.8", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'linux'" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "package-a" },
            { name = "package-b", marker = "sys_platform == 'darwin'", specifier = "<2.8" },
            { name = "package-b", marker = "sys_platform == 'linux'", specifier = ">=2.8" },
        ]
        "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(packse_index_url())
        .assert()
        .success();

    Ok(())
}

/// This is the same setup as `non-local-fork-marker-transitive`, but the disjoint
/// dependency specifications on `c` use the same constraints and thus depend on
/// the same version of `c`. In this case, there is no conflict.
///
/// ```text
/// fork-non-fork-marker-transitive
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   ├── requires a==1.0.0
/// │   │   └── satisfied by a-1.0.0
/// │   └── requires b==1.0.0
/// │       └── satisfied by b-1.0.0
/// ├── a
/// │   └── a-1.0.0
/// │       └── requires c>=2.0.0; sys_platform == "linux"
/// │           └── satisfied by c-2.0.0
/// ├── b
/// │   └── b-1.0.0
/// │       └── requires c>=2.0.0; sys_platform == "darwin"
/// │           └── satisfied by c-2.0.0
/// └── c
///     ├── c-1.0.0
///     └── c-2.0.0
/// ```
#[test]
fn fork_non_fork_marker_transitive() -> Result<()> {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"fork-non-fork-marker-transitive-", "package-"));

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''fork-non-fork-marker-transitive-a==1.0.0''',
          '''fork-non-fork-marker-transitive-b==1.0.0''',
        ]
        requires-python = ">=3.8"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        revision = 1
        requires-python = ">=3.8"

        [[package]]
        name = "package-a"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        dependencies = [
            { name = "package-c", marker = "sys_platform == 'linux'" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_non_fork_marker_transitive_a-1.0.0.tar.gz", hash = "sha256:68cff02c9f4a0b3014fdce524982a3cbf3a2ecaf0291c32c824cadb19f1e7cd0" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_non_fork_marker_transitive_a-1.0.0-py3-none-any.whl", hash = "sha256:6c49aef823d3544d795c05497ca2dbd5c419cad4454e4d41b8f4860be45bd4bf" },
        ]

        [[package]]
        name = "package-b"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        dependencies = [
            { name = "package-c", marker = "sys_platform == 'darwin'" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_non_fork_marker_transitive_b-1.0.0.tar.gz", hash = "sha256:ae7abe9cde79b810f91dff7329b63788a8253250053fe4e82563f0b2d0877182" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_non_fork_marker_transitive_b-1.0.0-py3-none-any.whl", hash = "sha256:6f301799cb51d920c7bef0120d5914f8315758ddc9f856b88783efae706dac16" },
        ]

        [[package]]
        name = "package-c"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_non_fork_marker_transitive_c-2.0.0.tar.gz", hash = "sha256:ffab9124854f64c8b5059ccaed481547f54abac868ba98aa6a454c0163cdb1c7" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_non_fork_marker_transitive_c-2.0.0-py3-none-any.whl", hash = "sha256:2b72d6af81967e1c55f30d920d6a7b913fce6ad0a0658ec79972a3d1a054e85f" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "package-a" },
            { name = "package-b" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "package-a", specifier = "==1.0.0" },
            { name = "package-b", specifier = "==1.0.0" },
        ]
        "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(packse_index_url())
        .assert()
        .success();

    Ok(())
}

/// This is like `non-local-fork-marker-transitive`, but the marker expressions are
/// placed on sibling dependency specifications. However, the actual dependency on
/// `c` is indirect, and thus, there's no fork detected by the universal resolver.
/// This in turn results in an unresolvable conflict on `c`.
///
/// ```text
/// fork-non-local-fork-marker-direct
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   ├── requires a==1.0.0; sys_platform == "linux"
/// │   │   └── satisfied by a-1.0.0
/// │   └── requires b==1.0.0; sys_platform == "darwin"
/// │       └── satisfied by b-1.0.0
/// ├── a
/// │   └── a-1.0.0
/// │       └── requires c<2.0.0
/// │           └── satisfied by c-1.0.0
/// ├── b
/// │   └── b-1.0.0
/// │       └── requires c>=2.0.0
/// │           └── satisfied by c-2.0.0
/// └── c
///     ├── c-1.0.0
///     └── c-2.0.0
/// ```
#[test]
fn fork_non_local_fork_marker_direct() -> Result<()> {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"fork-non-local-fork-marker-direct-", "package-"));

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''fork-non-local-fork-marker-direct-a==1.0.0; sys_platform == "linux"''',
          '''fork-non-local-fork-marker-direct-b==1.0.0; sys_platform == "darwin"''',
        ]
        requires-python = ">=3.8"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because package-a==1.0.0 depends on package-c<2.0.0 and package-b==1.0.0 depends on package-c>=2.0.0, we can conclude that package-a{sys_platform == 'linux'}==1.0.0 and package-b{sys_platform == 'darwin'}==1.0.0 are incompatible.
          And because your project depends on package-a{sys_platform == 'linux'}==1.0.0 and package-b{sys_platform == 'darwin'}==1.0.0, we can conclude that your project's requirements are unsatisfiable.
    "###
    );

    Ok(())
}

/// This setup introduces dependencies on two distinct versions of `c`, where
/// each such dependency has a marker expression attached that would normally
/// make them disjoint. In a non-universal resolver, this is no problem. But in a
/// forking resolver that tries to create one universal resolution, this can lead
/// to two distinct versions of `c` in the resolution. This is in and of itself
/// not a problem, since that is an expected scenario for universal resolution.
/// The problem in this case is that because the dependency specifications for
/// `c` occur in two different points (i.e., they are not sibling dependency
/// specifications) in the dependency graph, the forking resolver does not "detect"
/// it, and thus never forks and thus this results in "no resolution."
///
/// ```text
/// fork-non-local-fork-marker-transitive
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   ├── requires a==1.0.0
/// │   │   └── satisfied by a-1.0.0
/// │   └── requires b==1.0.0
/// │       └── satisfied by b-1.0.0
/// ├── a
/// │   └── a-1.0.0
/// │       └── requires c<2.0.0; sys_platform == "linux"
/// │           └── satisfied by c-1.0.0
/// ├── b
/// │   └── b-1.0.0
/// │       └── requires c>=2.0.0; sys_platform == "darwin"
/// │           └── satisfied by c-2.0.0
/// └── c
///     ├── c-1.0.0
///     └── c-2.0.0
/// ```
#[test]
fn fork_non_local_fork_marker_transitive() -> Result<()> {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"fork-non-local-fork-marker-transitive-", "package-"));

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''fork-non-local-fork-marker-transitive-a==1.0.0''',
          '''fork-non-local-fork-marker-transitive-b==1.0.0''',
        ]
        requires-python = ">=3.8"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because package-a==1.0.0 depends on package-c{sys_platform == 'linux'}<2.0.0 and only the following versions of package-c{sys_platform == 'linux'} are available:
              package-c{sys_platform == 'linux'}==1.0.0
              package-c{sys_platform == 'linux'}>2.0.0
          we can conclude that package-a==1.0.0 depends on package-c{sys_platform == 'linux'}==1.0.0.
          And because only package-c{sys_platform == 'darwin'}<=2.0.0 is available and package-b==1.0.0 depends on package-c{sys_platform == 'darwin'}>=2.0.0, we can conclude that package-a==1.0.0 and package-b==1.0.0 are incompatible.
          And because your project depends on package-a==1.0.0 and package-b==1.0.0, we can conclude that your project's requirements are unsatisfiable.
    "###
    );

    Ok(())
}

/// This scenario tests a very basic case of overlapping markers. Namely,
/// it emulates a common pattern in the ecosystem where marker expressions
/// are used to progressively increase the version constraints of a package
/// as the Python version increases.
///
/// In this case, there is actually a split occurring between
/// `python_version < '3.10'` and the other marker expressions, so this
/// isn't just a scenario with overlapping but non-disjoint markers.
///
/// In particular, this serves as a regression test. uv used to create a
/// lock file with a dependency on `a` with the following markers:
///
///     python_version < '3.10' or python_version >= '3.11'
///
/// But this implies that `a` won't be installed for Python 3.10, which is
/// clearly wrong.
///
/// The issue was that uv was intersecting *all* marker expressions. So
/// that `a>=1.1.0` and `a>=1.2.0` fork was getting `python_version >=
/// '3.10' and python_version >= '3.11'`, which, of course, simplifies
/// to `python_version >= '3.11'`. But this is wrong! It should be
/// `python_version >= '3.10' or python_version >= '3.11'`, which of course
/// simplifies to `python_version >= '3.10'`. And thus, the resulting forks
/// are not just disjoint but complete in this case.
///
/// Since there are no other constraints on `a`, this causes uv to select
/// `1.2.0` unconditionally. (The marker expressions get normalized out
/// entirely.)
///
/// ```text
/// fork-overlapping-markers-basic
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   ├── requires a>=1.0.0; python_version < "3.10"
/// │   │   ├── satisfied by a-1.0.0
/// │   │   ├── satisfied by a-1.1.0
/// │   │   └── satisfied by a-1.2.0
/// │   ├── requires a>=1.1.0; python_version >= "3.10"
/// │   │   ├── satisfied by a-1.1.0
/// │   │   └── satisfied by a-1.2.0
/// │   └── requires a>=1.2.0; python_version >= "3.11"
/// │       └── satisfied by a-1.2.0
/// └── a
///     ├── a-1.0.0
///     ├── a-1.1.0
///     └── a-1.2.0
/// ```
#[test]
fn fork_overlapping_markers_basic() -> Result<()> {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"fork-overlapping-markers-basic-", "package-"));

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''fork-overlapping-markers-basic-a>=1.0.0; python_version < "3.10"''',
          '''fork-overlapping-markers-basic-a>=1.1.0; python_version >= "3.10"''',
          '''fork-overlapping-markers-basic-a>=1.2.0; python_version >= "3.11"''',
        ]
        requires-python = ">=3.8"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        revision = 1
        requires-python = ">=3.8"
        resolution-markers = [
            "python_full_version >= '3.11'",
            "python_full_version == '3.10.*'",
            "python_full_version < '3.10'",
        ]

        [[package]]
        name = "package-a"
        version = "1.2.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_overlapping_markers_basic_a-1.2.0.tar.gz", hash = "sha256:f8c2058d80430d62b15c87fd66040a6c0dd23d32e7f144a932899c0c74bdff2a" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_overlapping_markers_basic_a-1.2.0-py3-none-any.whl", hash = "sha256:04293ed42eb3620c9ddf56e380a8408a30733d5d38f321a35c024d03e7116083" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "package-a" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "package-a", marker = "python_full_version < '3.10'", specifier = ">=1.0.0" },
            { name = "package-a", marker = "python_full_version >= '3.10'", specifier = ">=1.1.0" },
            { name = "package-a", marker = "python_full_version >= '3.11'", specifier = ">=1.2.0" },
        ]
        "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(packse_index_url())
        .assert()
        .success();

    Ok(())
}

/// This test contains a bistable resolution scenario when not using ahead-of-time
/// splitting of resolution forks: We meet one of two fork points depending on the
/// preferences, creating a resolution whose preferences lead us the other fork
/// point.
///
/// In the first case, we are in cleaver 2 and fork on `sys_platform`, in the
/// second case, we are in foo 1 or bar 1 amd fork over `os_name`.
///
/// First case: We select cleaver 2, fork on `sys_platform`, we reject cleaver 2
/// (missing fork `os_name`), we select cleaver 1 and don't fork on `os_name` in
/// `fork-if-not-forked`, done.
/// Second case: We have preference cleaver 1, fork on `os_name` in
/// `fork-if-not-forked`, we reject cleaver 1, we select cleaver 2, we fork on
/// `sys_platform`, we accept cleaver 2 since we forked on `os_name`, done.
///
/// ```text
/// preferences-dependent-forking-bistable
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires cleaver
/// │       ├── satisfied by cleaver-2.0.0
/// │       └── satisfied by cleaver-1.0.0
/// ├── cleaver
/// │   ├── cleaver-2.0.0
/// │   │   ├── requires fork-sys-platform==1; sys_platform == "linux"
/// │   │   │   └── satisfied by fork-sys-platform-1.0.0
/// │   │   ├── requires fork-sys-platform==2; sys_platform != "linux"
/// │   │   │   └── satisfied by fork-sys-platform-2.0.0
/// │   │   ├── requires reject-cleaver2==1; os_name == "posix"
/// │   │   │   └── satisfied by reject-cleaver2-1.0.0
/// │   │   └── requires reject-cleaver2-proxy
/// │   │       └── satisfied by reject-cleaver2-proxy-1.0.0
/// │   └── cleaver-1.0.0
/// │       ├── requires fork-if-not-forked!=2; sys_platform == "linux"
/// │       │   ├── satisfied by fork-if-not-forked-1.0.0
/// │       │   └── satisfied by fork-if-not-forked-3.0.0
/// │       ├── requires fork-if-not-forked-proxy; sys_platform != "linux"
/// │       │   └── satisfied by fork-if-not-forked-proxy-1.0.0
/// │       ├── requires reject-cleaver1==1; sys_platform == "linux"
/// │       │   └── satisfied by reject-cleaver1-1.0.0
/// │       └── requires reject-cleaver1-proxy
/// │           └── satisfied by reject-cleaver1-proxy-1.0.0
/// ├── fork-if-not-forked
/// │   ├── fork-if-not-forked-1.0.0
/// │   │   ├── requires fork-os-name==1; os_name == "posix"
/// │   │   │   └── satisfied by fork-os-name-1.0.0
/// │   │   ├── requires fork-os-name==2; os_name != "posix"
/// │   │   │   └── satisfied by fork-os-name-2.0.0
/// │   │   └── requires reject-cleaver1-proxy
/// │   │       └── satisfied by reject-cleaver1-proxy-1.0.0
/// │   ├── fork-if-not-forked-2.0.0
/// │   └── fork-if-not-forked-3.0.0
/// ├── fork-if-not-forked-proxy
/// │   └── fork-if-not-forked-proxy-1.0.0
/// │       └── requires fork-if-not-forked!=3
/// │           ├── satisfied by fork-if-not-forked-1.0.0
/// │           └── satisfied by fork-if-not-forked-2.0.0
/// ├── fork-os-name
/// │   ├── fork-os-name-1.0.0
/// │   └── fork-os-name-2.0.0
/// ├── fork-sys-platform
/// │   ├── fork-sys-platform-1.0.0
/// │   └── fork-sys-platform-2.0.0
/// ├── reject-cleaver1
/// │   ├── reject-cleaver1-1.0.0
/// │   └── reject-cleaver1-2.0.0
/// ├── reject-cleaver1-proxy
/// │   └── reject-cleaver1-proxy-1.0.0
/// │       └── requires reject-cleaver1==2; sys_platform != "linux"
/// │           └── satisfied by reject-cleaver1-2.0.0
/// ├── reject-cleaver2
/// │   ├── reject-cleaver2-1.0.0
/// │   └── reject-cleaver2-2.0.0
/// └── reject-cleaver2-proxy
///     └── reject-cleaver2-proxy-1.0.0
///         └── requires reject-cleaver2==2; os_name != "posix"
///             └── satisfied by reject-cleaver2-2.0.0
/// ```
#[test]
fn preferences_dependent_forking_bistable() -> Result<()> {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"preferences-dependent-forking-bistable-", "package-"));

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''preferences-dependent-forking-bistable-cleaver''',
        ]
        requires-python = ">=3.8"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 8 packages in [TIME]
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        revision = 1
        requires-python = ">=3.8"
        resolution-markers = [
            "sys_platform == 'linux'",
            "sys_platform != 'linux'",
        ]

        [[package]]
        name = "package-cleaver"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        dependencies = [
            { name = "package-fork-if-not-forked", version = "3.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'linux'" },
            { name = "package-fork-if-not-forked-proxy", marker = "sys_platform != 'linux'" },
            { name = "package-reject-cleaver1", version = "1.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'linux'" },
            { name = "package-reject-cleaver1-proxy" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_bistable_cleaver-1.0.0.tar.gz", hash = "sha256:64e5ee0c81d6a51fb71ed517fd04cc26c656908ad05073270e67c2f9b92194c5" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_bistable_cleaver-1.0.0-py3-none-any.whl", hash = "sha256:552a061bf303fc4103ff91adb03864391a041f9bdcb9b2f8a552b232efce633b" },
        ]

        [[package]]
        name = "package-fork-if-not-forked"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform != 'linux'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_bistable_fork_if_not_forked-2.0.0.tar.gz", hash = "sha256:1f130c437449e7f0752938bff562addd287b6df96784122885e83563f7624798" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_bistable_fork_if_not_forked-2.0.0-py3-none-any.whl", hash = "sha256:a3e0a53d855ef38b9bbe2c6de67a1dd5eefc65c40e02b5282319cabf59bac740" },
        ]

        [[package]]
        name = "package-fork-if-not-forked"
        version = "3.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_bistable_fork_if_not_forked-3.0.0.tar.gz", hash = "sha256:72aee18148130c3287f2e07f31cd8883f1b35d91d6ef5230961e5fcc57667943" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_bistable_fork_if_not_forked-3.0.0-py3-none-any.whl", hash = "sha256:45343fd8a37969d5ace1fe0d235341573b1dc84eea099d92f479d41a21e206fa" },
        ]

        [[package]]
        name = "package-fork-if-not-forked-proxy"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        dependencies = [
            { name = "package-fork-if-not-forked", version = "2.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform != 'linux'" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_bistable_fork_if_not_forked_proxy-1.0.0.tar.gz", hash = "sha256:0ed00a7c8280348225835fadc76db8ecc6b4a9ee11351a6c432c475f8d1579de" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_bistable_fork_if_not_forked_proxy-1.0.0-py3-none-any.whl", hash = "sha256:0f6bd3726cd7aa245751f08e176caa797a5de986f020b7d0b8767756eea77d26" },
        ]

        [[package]]
        name = "package-reject-cleaver1"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_bistable_reject_cleaver1-1.0.0.tar.gz", hash = "sha256:bf19f244de469bb73c7fb9dc438bca2fac829d865e546327694b2f292192c042" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_bistable_reject_cleaver1-1.0.0-py3-none-any.whl", hash = "sha256:bda045df120e617d369b8be48e7a489c57968ee2b75e181969593fbc2a789519" },
        ]

        [[package]]
        name = "package-reject-cleaver1"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform != 'linux'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_bistable_reject_cleaver1-2.0.0.tar.gz", hash = "sha256:b671f6112e6829557bec5c1aa86e55e79a9883a28117025523a132ff24cd9be3" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_bistable_reject_cleaver1-2.0.0-py3-none-any.whl", hash = "sha256:104923522767e447fb2ff3e2cfc730f5d2d4b2040f89a33d1abeb9863ed169ac" },
        ]

        [[package]]
        name = "package-reject-cleaver1-proxy"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        dependencies = [
            { name = "package-reject-cleaver1", version = "2.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform != 'linux'" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_bistable_reject_cleaver1_proxy-1.0.0.tar.gz", hash = "sha256:6b6eaa229d55de992e36084521d2f62dce35120a866e20354d0e5617e16e00ce" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_bistable_reject_cleaver1_proxy-1.0.0-py3-none-any.whl", hash = "sha256:08ace26d0f4a74275dd38803fd67101eaf2cb400441fc8d479461ced31a947c1" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "package-cleaver" },
        ]

        [package.metadata]
        requires-dist = [{ name = "package-cleaver" }]
        "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(packse_index_url())
        .assert()
        .success();

    Ok(())
}

/// Like `preferences-dependent-forking`, but when we don't fork the resolution fails.
///
/// Consider a fresh run without preferences:
/// * We start with cleaver 2
/// * We fork
/// * We reject cleaver 2
/// * We find cleaver solution in fork 1 with foo 2 with bar 1
/// * We find cleaver solution in fork 2 with foo 1 with bar 2
/// * We write cleaver 1, foo 1, foo 2, bar 1 and bar 2 to the lockfile
///
/// In a subsequent run, we read the preference cleaver 1 from the lockfile (the preferences for foo and bar don't matter):
/// * We start with cleaver 1
/// * We're in universal mode, cleaver requires foo 1, bar 1
/// * foo 1 requires bar 2, conflict
///
/// Design sketch:
/// ```text
/// root -> clear, foo, bar
/// # Cause a fork, then forget that version.
/// cleaver 2 -> unrelated-dep==1; fork==1
/// cleaver 2 -> unrelated-dep==2; fork==2
/// cleaver 2 -> reject-cleaver-2
/// # Allow different versions when forking, but force foo 1, bar 1 in universal mode without forking.
/// cleaver 1 -> foo==1; fork==1
/// cleaver 1 -> bar==1; fork==2
/// # When we selected foo 1, bar 1 in universal mode for cleaver, this causes a conflict, otherwise we select bar 2.
/// foo 1 -> bar==2
/// ```
///
/// ```text
/// preferences-dependent-forking-conflicting
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   ├── requires bar
/// │   │   ├── satisfied by bar-1.0.0
/// │   │   └── satisfied by bar-2.0.0
/// │   ├── requires cleaver
/// │   │   ├── satisfied by cleaver-2.0.0
/// │   │   └── satisfied by cleaver-1.0.0
/// │   └── requires foo
/// │       ├── satisfied by foo-1.0.0
/// │       └── satisfied by foo-2.0.0
/// ├── bar
/// │   ├── bar-1.0.0
/// │   └── bar-2.0.0
/// ├── cleaver
/// │   ├── cleaver-2.0.0
/// │   │   ├── requires reject-cleaver-2
/// │   │   │   └── satisfied by reject-cleaver-2-1.0.0
/// │   │   ├── requires unrelated-dep==1; sys_platform == "linux"
/// │   │   │   └── satisfied by unrelated-dep-1.0.0
/// │   │   └── requires unrelated-dep==2; sys_platform != "linux"
/// │   │       └── satisfied by unrelated-dep-2.0.0
/// │   └── cleaver-1.0.0
/// │       ├── requires bar==1; sys_platform != "linux"
/// │       │   └── satisfied by bar-1.0.0
/// │       └── requires foo==1; sys_platform == "linux"
/// │           └── satisfied by foo-1.0.0
/// ├── foo
/// │   ├── foo-1.0.0
/// │   │   └── requires bar==2
/// │   │       └── satisfied by bar-2.0.0
/// │   └── foo-2.0.0
/// ├── reject-cleaver-2
/// │   └── reject-cleaver-2-1.0.0
/// │       └── requires unrelated-dep==3
/// │           └── satisfied by unrelated-dep-3.0.0
/// └── unrelated-dep
///     ├── unrelated-dep-1.0.0
///     ├── unrelated-dep-2.0.0
///     └── unrelated-dep-3.0.0
/// ```
#[test]
fn preferences_dependent_forking_conflicting() -> Result<()> {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"preferences-dependent-forking-conflicting-", "package-"));

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''preferences-dependent-forking-conflicting-cleaver''',
          '''preferences-dependent-forking-conflicting-foo''',
          '''preferences-dependent-forking-conflicting-bar''',
        ]
        requires-python = ">=3.8"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    "###
    );

    Ok(())
}

/// This test case is like "preferences-dependent-forking-bistable", but with three
/// states instead of two. The first two locks are in a different state, then we
/// enter the tristable state.
///
/// It's not polished, but it's useful to have something with a higher period
/// than 2 in our test suite.
///
/// ```text
/// preferences-dependent-forking-tristable
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   ├── requires bar
/// │   │   ├── satisfied by bar-1.0.0
/// │   │   └── satisfied by bar-2.0.0
/// │   ├── requires cleaver
/// │   │   ├── satisfied by cleaver-2.0.0
/// │   │   └── satisfied by cleaver-1.0.0
/// │   └── requires foo
/// │       ├── satisfied by foo-1.0.0
/// │       └── satisfied by foo-2.0.0
/// ├── a
/// │   └── a-1.0.0
/// │       └── requires unrelated-dep3==1; os_name == "posix"
/// │           └── satisfied by unrelated-dep3-1.0.0
/// ├── b
/// │   └── b-1.0.0
/// │       └── requires unrelated-dep3==2; os_name != "posix"
/// │           └── satisfied by unrelated-dep3-2.0.0
/// ├── bar
/// │   ├── bar-1.0.0
/// │   │   ├── requires c!=3; sys_platform == "linux"
/// │   │   │   ├── satisfied by c-1.0.0
/// │   │   │   └── satisfied by c-2.0.0
/// │   │   ├── requires d; sys_platform != "linux"
/// │   │   │   └── satisfied by d-1.0.0
/// │   │   └── requires reject-cleaver-1
/// │   │       └── satisfied by reject-cleaver-1-1.0.0
/// │   └── bar-2.0.0
/// ├── c
/// │   ├── c-1.0.0
/// │   │   ├── requires reject-cleaver-1
/// │   │   │   └── satisfied by reject-cleaver-1-1.0.0
/// │   │   ├── requires unrelated-dep2==1; os_name == "posix"
/// │   │   │   └── satisfied by unrelated-dep2-1.0.0
/// │   │   └── requires unrelated-dep2==2; os_name != "posix"
/// │   │       └── satisfied by unrelated-dep2-2.0.0
/// │   ├── c-2.0.0
/// │   └── c-3.0.0
/// ├── cleaver
/// │   ├── cleaver-2.0.0
/// │   │   ├── requires a
/// │   │   │   └── satisfied by a-1.0.0
/// │   │   ├── requires b
/// │   │   │   └── satisfied by b-1.0.0
/// │   │   ├── requires unrelated-dep==1; sys_platform == "linux"
/// │   │   │   └── satisfied by unrelated-dep-1.0.0
/// │   │   └── requires unrelated-dep==2; sys_platform != "linux"
/// │   │       └── satisfied by unrelated-dep-2.0.0
/// │   └── cleaver-1.0.0
/// │       ├── requires bar==1; sys_platform != "linux"
/// │       │   └── satisfied by bar-1.0.0
/// │       └── requires foo==1; sys_platform == "linux"
/// │           └── satisfied by foo-1.0.0
/// ├── d
/// │   └── d-1.0.0
/// │       └── requires c!=2
/// │           ├── satisfied by c-1.0.0
/// │           └── satisfied by c-3.0.0
/// ├── foo
/// │   ├── foo-1.0.0
/// │   │   ├── requires c!=3; sys_platform == "linux"
/// │   │   │   ├── satisfied by c-1.0.0
/// │   │   │   └── satisfied by c-2.0.0
/// │   │   ├── requires c!=2; sys_platform != "linux"
/// │   │   │   ├── satisfied by c-1.0.0
/// │   │   │   └── satisfied by c-3.0.0
/// │   │   └── requires reject-cleaver-1
/// │   │       └── satisfied by reject-cleaver-1-1.0.0
/// │   └── foo-2.0.0
/// ├── reject-cleaver-1
/// │   └── reject-cleaver-1-1.0.0
/// │       ├── requires unrelated-dep2==1; sys_platform == "linux"
/// │       │   └── satisfied by unrelated-dep2-1.0.0
/// │       └── requires unrelated-dep2==2; sys_platform != "linux"
/// │           └── satisfied by unrelated-dep2-2.0.0
/// ├── reject-cleaver-2
/// │   └── reject-cleaver-2-1.0.0
/// │       └── requires unrelated-dep3==3
/// │           └── satisfied by unrelated-dep3-3.0.0
/// ├── unrelated-dep
/// │   ├── unrelated-dep-1.0.0
/// │   ├── unrelated-dep-2.0.0
/// │   └── unrelated-dep-3.0.0
/// ├── unrelated-dep2
/// │   ├── unrelated-dep2-1.0.0
/// │   ├── unrelated-dep2-2.0.0
/// │   └── unrelated-dep2-3.0.0
/// └── unrelated-dep3
///     ├── unrelated-dep3-1.0.0
///     ├── unrelated-dep3-2.0.0
///     └── unrelated-dep3-3.0.0
/// ```
#[test]
fn preferences_dependent_forking_tristable() -> Result<()> {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"preferences-dependent-forking-tristable-", "package-"));

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''preferences-dependent-forking-tristable-cleaver''',
          '''preferences-dependent-forking-tristable-foo''',
          '''preferences-dependent-forking-tristable-bar''',
        ]
        requires-python = ">=3.8"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 11 packages in [TIME]
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        revision = 1
        requires-python = ">=3.8"
        resolution-markers = [
            "sys_platform == 'linux'",
            "sys_platform != 'linux'",
        ]

        [[package]]
        name = "package-bar"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform != 'linux'",
        ]
        dependencies = [
            { name = "package-d", marker = "sys_platform != 'linux'" },
            { name = "package-reject-cleaver-1", marker = "sys_platform != 'linux'" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_tristable_bar-1.0.0.tar.gz", hash = "sha256:5d7142b60729bd25206dde836b8f629c72a29593156dee4c4551ad23b7096e8c" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_tristable_bar-1.0.0-py3-none-any.whl", hash = "sha256:a590cb59852676a12e3537efe2c812c0640a32408a2ea7f6e5611c7190683865" },
        ]

        [[package]]
        name = "package-bar"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_tristable_bar-2.0.0.tar.gz", hash = "sha256:cc856e6aca342176e6ba518a298198258b7be3ee7a6b86319c1d8b731e54991e" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_tristable_bar-2.0.0-py3-none-any.whl", hash = "sha256:80195408d22da78f3d6ac3cc955840b5fcb2a76d774120e2aa007c7e7cbc2b4e" },
        ]

        [[package]]
        name = "package-c"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_tristable_c-2.0.0.tar.gz", hash = "sha256:f0d941b83146d72e05fde266be4a500400683e6c62ae86dab11af78c2d26587b" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_tristable_c-2.0.0-py3-none-any.whl", hash = "sha256:aaaddb9a24c0827169bd66d4b1b1965ceb375bebdb60047e2d66a05d363df2e3" },
        ]

        [[package]]
        name = "package-c"
        version = "3.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform != 'linux'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_tristable_c-3.0.0.tar.gz", hash = "sha256:3531c0ec88cc79cde8106e949c7062854bbd48e3bc60803246372cdc4f4c4864" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_tristable_c-3.0.0-py3-none-any.whl", hash = "sha256:c048df9ab2c29bf914684add607dccca9ed7d035608cb92ef789216a15544e8b" },
        ]

        [[package]]
        name = "package-cleaver"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        dependencies = [
            { name = "package-bar", version = "1.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform != 'linux'" },
            { name = "package-foo", marker = "sys_platform == 'linux'" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_tristable_cleaver-1.0.0.tar.gz", hash = "sha256:49ec5779d0722586652e3ceb4ca2bf053a79dc3fa2d7ccd428a359bcc885a248" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_tristable_cleaver-1.0.0-py3-none-any.whl", hash = "sha256:fb33bd10e4c6a237e7d0488e7ba1c5ee794eb01a1813ff80695bbfc4036f01b7" },
        ]

        [[package]]
        name = "package-d"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        dependencies = [
            { name = "package-c", version = "3.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform != 'linux'" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_tristable_d-1.0.0.tar.gz", hash = "sha256:690b69acb46d0ebfb11a81f401d2ea2e2e6a8ae97f199d345715e9bd40a7ceba" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_tristable_d-1.0.0-py3-none-any.whl", hash = "sha256:f34e37e7164316c9b9ed3022d1ff378b3dcd895db6e339894f53d2b27a5d6ba0" },
        ]

        [[package]]
        name = "package-foo"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        dependencies = [
            { name = "package-c", version = "2.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'linux'" },
            { name = "package-c", version = "3.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform != 'linux'" },
            { name = "package-reject-cleaver-1" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_tristable_foo-1.0.0.tar.gz", hash = "sha256:7c1a2ca51dd2156cf36c3400e38595e11b09442052f4bd1d6b3d53eb5b2acf32" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_tristable_foo-1.0.0-py3-none-any.whl", hash = "sha256:524dfd846c31a55bb6d6a0d0cec80d42c0a87c78aabbe0f1d5426c60493bd41b" },
        ]

        [[package]]
        name = "package-reject-cleaver-1"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        dependencies = [
            { name = "package-unrelated-dep2", version = "1.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'linux'" },
            { name = "package-unrelated-dep2", version = "2.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform != 'linux'" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_tristable_reject_cleaver_1-1.0.0.tar.gz", hash = "sha256:6ef93ca22db3a054559cb34f574ffa3789951f2f82b213c5502d0e9ff746f15e" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_tristable_reject_cleaver_1-1.0.0-py3-none-any.whl", hash = "sha256:b5e5203994245c2b983dd94595281a03ac38c05e14f0a8792d13763f69aa43a8" },
        ]

        [[package]]
        name = "package-unrelated-dep2"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_tristable_unrelated_dep2-1.0.0.tar.gz", hash = "sha256:bbeb0f558aff8c48bac6fdab42ed52f49d68d2b51a7de82ff9357925a6e5023a" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_tristable_unrelated_dep2-1.0.0-py3-none-any.whl", hash = "sha256:b36bc1e6f0140fdbf03575eb6bb0873c298b1d44dd7955412909ba9c2650a250" },
        ]

        [[package]]
        name = "package-unrelated-dep2"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform != 'linux'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_tristable_unrelated_dep2-2.0.0.tar.gz", hash = "sha256:ac23c6208b6340b2542e730e1df770ed4ca65f234de86d2216add6c2b975f95c" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_tristable_unrelated_dep2-2.0.0-py3-none-any.whl", hash = "sha256:5fc6d9c0fee066b33df862f31057c8cc2c0c5662ef9949337407e0131aa46e7f" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "package-bar", version = "1.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform != 'linux'" },
            { name = "package-bar", version = "2.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'linux'" },
            { name = "package-cleaver" },
            { name = "package-foo" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "package-bar" },
            { name = "package-cleaver" },
            { name = "package-foo" },
        ]
        "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(packse_index_url())
        .assert()
        .success();

    Ok(())
}

/// This test contains a scenario where the solution depends on whether we fork, and whether we fork depends on the
/// preferences.
///
/// Consider a fresh run without preferences:
/// * We start with cleaver 2
/// * We fork
/// * We reject cleaver 2
/// * We find cleaver solution in fork 1 with foo 2 with bar 1
/// * We find cleaver solution in fork 2 with foo 1 with bar 2
/// * We write cleaver 1, foo 1, foo 2, bar 1 and bar 2 to the lockfile
///
/// In a subsequent run, we read the preference cleaver 1 from the lockfile (the preferences for foo and bar don't matter):
/// * We start with cleaver 1
/// * We're in universal mode, we resolve foo 1 and bar 1
/// * We write cleaver 1 and bar 1 to the lockfile
///
/// We call a resolution that's different on the second run to the first unstable.
///
/// Design sketch:
/// ```text
/// root -> clear, foo, bar
/// # Cause a fork, then forget that version.
/// cleaver 2 -> unrelated-dep==1; fork==1
/// cleaver 2 -> unrelated-dep==2; fork==2
/// cleaver 2 -> reject-cleaver-2
/// # Allow different versions when forking, but force foo 1, bar 1 in universal mode without forking.
/// cleaver 1 -> foo==1; fork==1
/// cleaver 1 -> bar==1; fork==2
/// ```
///
/// ```text
/// preferences-dependent-forking
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   ├── requires bar
/// │   │   ├── satisfied by bar-1.0.0
/// │   │   └── satisfied by bar-2.0.0
/// │   ├── requires cleaver
/// │   │   ├── satisfied by cleaver-2.0.0
/// │   │   └── satisfied by cleaver-1.0.0
/// │   └── requires foo
/// │       ├── satisfied by foo-1.0.0
/// │       └── satisfied by foo-2.0.0
/// ├── bar
/// │   ├── bar-1.0.0
/// │   └── bar-2.0.0
/// ├── cleaver
/// │   ├── cleaver-2.0.0
/// │   │   ├── requires reject-cleaver-2
/// │   │   │   └── satisfied by reject-cleaver-2-1.0.0
/// │   │   ├── requires unrelated-dep==1; sys_platform == "linux"
/// │   │   │   └── satisfied by unrelated-dep-1.0.0
/// │   │   └── requires unrelated-dep==2; sys_platform != "linux"
/// │   │       └── satisfied by unrelated-dep-2.0.0
/// │   └── cleaver-1.0.0
/// │       ├── requires bar==1; sys_platform != "linux"
/// │       │   └── satisfied by bar-1.0.0
/// │       └── requires foo==1; sys_platform == "linux"
/// │           └── satisfied by foo-1.0.0
/// ├── foo
/// │   ├── foo-1.0.0
/// │   └── foo-2.0.0
/// ├── reject-cleaver-2
/// │   └── reject-cleaver-2-1.0.0
/// │       └── requires unrelated-dep==3
/// │           └── satisfied by unrelated-dep-3.0.0
/// └── unrelated-dep
///     ├── unrelated-dep-1.0.0
///     ├── unrelated-dep-2.0.0
///     └── unrelated-dep-3.0.0
/// ```
#[test]
fn preferences_dependent_forking() -> Result<()> {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"preferences-dependent-forking-", "package-"));

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''preferences-dependent-forking-cleaver''',
          '''preferences-dependent-forking-foo''',
          '''preferences-dependent-forking-bar''',
        ]
        requires-python = ">=3.8"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        revision = 1
        requires-python = ">=3.8"
        resolution-markers = [
            "sys_platform == 'linux'",
            "sys_platform != 'linux'",
        ]

        [[package]]
        name = "package-bar"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform != 'linux'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_bar-1.0.0.tar.gz", hash = "sha256:7eef4e0c910b9e4cadf6c707e60a2151f7dc6407d815112ec93a467d76226f5e" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_bar-1.0.0-py3-none-any.whl", hash = "sha256:3cdaac4b0ba330f902d0628c0b1d6e62692f52255d02718d04f46ade7c8ad6a6" },
        ]

        [[package]]
        name = "package-bar"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_bar-2.0.0.tar.gz", hash = "sha256:f440dbb8c3b848be467c9d3cd4970963fae3144de12454fd48fe9077eb76e9ea" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_bar-2.0.0-py3-none-any.whl", hash = "sha256:24fd0534fec4053f4cac960244943ef13d1bad26bbb5fffe6944a8cf898f26f0" },
        ]

        [[package]]
        name = "package-cleaver"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        dependencies = [
            { name = "package-bar", version = "1.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform != 'linux'" },
            { name = "package-foo", marker = "sys_platform == 'linux'" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_cleaver-1.0.0.tar.gz", hash = "sha256:0347b927fdf7731758ea53e1594309fc6311ca6983f36553bc11654a264062b2" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_cleaver-1.0.0-py3-none-any.whl", hash = "sha256:855467570c9da8e92ce37d0ebd0653cfa50d5d88b9540beca94feaa37a539dc3" },
        ]

        [[package]]
        name = "package-foo"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_foo-1.0.0.tar.gz", hash = "sha256:abf1c0ac825ee5961e683067634916f98c6651a6d4473ff87d8b57c17af8fed2" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_foo-1.0.0-py3-none-any.whl", hash = "sha256:85348e8df4892b9f297560c16abcf231828f538dc07339ed121197a00a0626a5" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "package-bar", version = "1.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform != 'linux'" },
            { name = "package-bar", version = "2.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'linux'" },
            { name = "package-cleaver" },
            { name = "package-foo" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "package-bar" },
            { name = "package-cleaver" },
            { name = "package-foo" },
        ]
        "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(packse_index_url())
        .assert()
        .success();

    Ok(())
}

/// This scenario tries to check that the "remaining universe" handling in
/// the universal resolver is correct. Namely, whenever we create forks
/// from disjoint markers that don't union to the universe, we need to
/// create *another* fork corresponding to the difference between the
/// universe and the union of the forks.
///
/// But when we do this, that remaining universe fork needs to be created
/// like any other fork: it should start copying whatever set of forks
/// existed by the time we got to this point, intersecting the markers with
/// the markers describing the remaining universe and then filtering out
/// any dependencies that are disjoint with the resulting markers.
///
/// This test exercises that logic by ensuring that a package `z` in the
/// remaining universe is excluded based on the combination of markers
/// from a parent fork. That is, if the remaining universe fork does not
/// pick up the markers from the parent forks, then `z` would be included
/// because the remaining universe for _just_ the `b` dependencies of `a`
/// is `os_name != 'linux' and os_name != 'darwin'`, which is satisfied by
/// `z`'s marker of `sys_platform == 'windows'`. However, `a 1.0.0` is only
/// selected in the context of `a < 2 ; sys_platform == 'illumos'`, so `z`
/// should never appear in the resolution.
///
/// ```text
/// fork-remaining-universe-partitioning
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   ├── requires a>=2; sys_platform == "windows"
/// │   │   └── satisfied by a-2.0.0
/// │   └── requires a<2; sys_platform == "illumos"
/// │       └── satisfied by a-1.0.0
/// ├── a
/// │   ├── a-1.0.0
/// │   │   ├── requires b>=2; os_name == "linux"
/// │   │   │   └── satisfied by b-2.0.0
/// │   │   ├── requires b<2; os_name == "darwin"
/// │   │   │   └── satisfied by b-1.0.0
/// │   │   └── requires z; sys_platform == "windows"
/// │   │       └── satisfied by z-1.0.0
/// │   └── a-2.0.0
/// ├── b
/// │   ├── b-1.0.0
/// │   └── b-2.0.0
/// └── z
///     └── z-1.0.0
/// ```
#[test]
fn fork_remaining_universe_partitioning() -> Result<()> {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"fork-remaining-universe-partitioning-", "package-"));

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''fork-remaining-universe-partitioning-a>=2; sys_platform == "windows"''',
          '''fork-remaining-universe-partitioning-a<2; sys_platform == "illumos"''',
        ]
        requires-python = ">=3.8"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        revision = 1
        requires-python = ">=3.8"
        resolution-markers = [
            "os_name == 'darwin' and sys_platform == 'illumos'",
            "os_name == 'linux' and sys_platform == 'illumos'",
            "os_name != 'darwin' and os_name != 'linux' and sys_platform == 'illumos'",
            "sys_platform == 'windows'",
            "sys_platform != 'illumos' and sys_platform != 'windows'",
        ]

        [[package]]
        name = "package-a"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "os_name == 'darwin' and sys_platform == 'illumos'",
            "os_name == 'linux' and sys_platform == 'illumos'",
            "os_name != 'darwin' and os_name != 'linux' and sys_platform == 'illumos'",
        ]
        dependencies = [
            { name = "package-b", version = "1.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "os_name == 'darwin' and sys_platform == 'illumos'" },
            { name = "package-b", version = "2.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "os_name == 'linux' and sys_platform == 'illumos'" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_remaining_universe_partitioning_a-1.0.0.tar.gz", hash = "sha256:d5be0af9a1958ec08ca2827b47bfd507efc26cab03ecf7ddf204e18e8a3a18ae" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_remaining_universe_partitioning_a-1.0.0-py3-none-any.whl", hash = "sha256:d72d45c02de21048507987503d67ff7b579cd58b8f58003fdf7800bc450b2b1d" },
        ]

        [[package]]
        name = "package-a"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform == 'windows'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_remaining_universe_partitioning_a-2.0.0.tar.gz", hash = "sha256:c6166efba9da6cbe32221dd425873c9de605343db1cd8d732c4c1624635944b0" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_remaining_universe_partitioning_a-2.0.0-py3-none-any.whl", hash = "sha256:db8e9cdacc9d755db5ce38bb1fd884c5cb047c3f3e1753e7a9cd46aed13757ae" },
        ]

        [[package]]
        name = "package-b"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "os_name == 'darwin' and sys_platform == 'illumos'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_remaining_universe_partitioning_b-1.0.0.tar.gz", hash = "sha256:83755cf4f9d97909bc295a3fbb10006747c02b2344f3f017cff276fa7922b756" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_remaining_universe_partitioning_b-1.0.0-py3-none-any.whl", hash = "sha256:24ecd35e335149ed5de3ed495aa3715c31385d34cde7f9e0db5d168099e74f51" },
        ]

        [[package]]
        name = "package-b"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "os_name == 'linux' and sys_platform == 'illumos'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_remaining_universe_partitioning_b-2.0.0.tar.gz", hash = "sha256:32cf6efcab24453f11a3bf2c230536b99a41e9611f5e96b2eee589c0d81f2348" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_remaining_universe_partitioning_b-2.0.0-py3-none-any.whl", hash = "sha256:4c90283190759f076d67f0b4683efd061af5ab2ce5007b35c7dd42836ceaebdf" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "package-a", version = "1.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'illumos'" },
            { name = "package-a", version = "2.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'windows'" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "package-a", marker = "sys_platform == 'illumos'", specifier = "<2" },
            { name = "package-a", marker = "sys_platform == 'windows'", specifier = ">=2" },
        ]
        "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(packse_index_url())
        .assert()
        .success();

    Ok(())
}

/// This tests that a `Requires-Python` specifier will result in the
/// exclusion of dependency specifications that cannot possibly satisfy it.
///
/// In particular, this is tested via the `python_full_version` marker with
/// a pre-release version.
///
/// ```text
/// fork-requires-python-full-prerelease
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   └── requires a==1.0.0; python_full_version == "3.9b1"
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
///         └── requires python>=3.8
/// ```
#[test]
fn fork_requires_python_full_prerelease() -> Result<()> {
    let context = TestContext::new("3.12");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"fork-requires-python-full-prerelease-", "package-"));

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''fork-requires-python-full-prerelease-a==1.0.0; python_full_version == "3.9b1"''',
        ]
        requires-python = ">=3.10"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        revision = 1
        requires-python = ">=3.10"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }

        [package.metadata]
        requires-dist = [{ name = "package-a", marker = "python_full_version == '3.9'", specifier = "==1.0.0" }]
        "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(packse_index_url())
        .assert()
        .success();

    Ok(())
}

/// This tests that a `Requires-Python` specifier will result in the
/// exclusion of dependency specifications that cannot possibly satisfy it.
///
/// In particular, this is tested via the `python_full_version` marker
/// instead of the more common `python_version` marker.
///
/// ```text
/// fork-requires-python-full
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   └── requires a==1.0.0; python_full_version == "3.9"
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
///         └── requires python>=3.8
/// ```
#[test]
fn fork_requires_python_full() -> Result<()> {
    let context = TestContext::new("3.12");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"fork-requires-python-full-", "package-"));

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''fork-requires-python-full-a==1.0.0; python_full_version == "3.9"''',
        ]
        requires-python = ">=3.10"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        revision = 1
        requires-python = ">=3.10"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }

        [package.metadata]
        requires-dist = [{ name = "package-a", marker = "python_full_version == '3.9'", specifier = "==1.0.0" }]
        "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(packse_index_url())
        .assert()
        .success();

    Ok(())
}

/// This tests that a `Requires-Python` specifier that includes a Python
/// patch version will not result in excluded a dependency specification
/// with a `python_version == '3.10'` marker.
///
/// This is a regression test for the universal resolver where it would
/// convert a `Requires-Python: >=3.10.1` specifier into a
/// `python_version >= '3.10.1'` marker expression, which would be
/// considered disjoint with `python_version == '3.10'`. Thus, the
/// dependency `a` below was erroneously excluded. It should be included.
///
/// ```text
/// fork-requires-python-patch-overlap
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   └── requires a==1.0.0; python_version == "3.10"
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
///         └── requires python>=3.8
/// ```
#[test]
fn fork_requires_python_patch_overlap() -> Result<()> {
    let context = TestContext::new("3.12");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"fork-requires-python-patch-overlap-", "package-"));

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''fork-requires-python-patch-overlap-a==1.0.0; python_version == "3.10"''',
        ]
        requires-python = ">=3.10.1"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        revision = 1
        requires-python = ">=3.10.1"

        [[package]]
        name = "package-a"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_requires_python_patch_overlap_a-1.0.0.tar.gz", hash = "sha256:ac2820ee4808788674295192d79a709e3259aa4eef5b155e77f719ad4eaa324d" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_requires_python_patch_overlap_a-1.0.0-py3-none-any.whl", hash = "sha256:43a750ba4eaab749d608d70e94d3d51e083cc21f5a52ac99b5967b26486d5ef1" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "package-a", marker = "python_full_version < '3.11'" },
        ]

        [package.metadata]
        requires-dist = [{ name = "package-a", marker = "python_full_version == '3.10.*'", specifier = "==1.0.0" }]
        "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(packse_index_url())
        .assert()
        .success();

    Ok(())
}

/// This tests that a `Requires-Python` specifier will result in the
/// exclusion of dependency specifications that cannot possibly satisfy it.
///
/// ```text
/// fork-requires-python
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   └── requires a==1.0.0; python_version == "3.9"
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
///         └── requires python>=3.8
/// ```
#[test]
fn fork_requires_python() -> Result<()> {
    let context = TestContext::new("3.12");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"fork-requires-python-", "package-"));

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''fork-requires-python-a==1.0.0; python_version == "3.9"''',
        ]
        requires-python = ">=3.10"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        revision = 1
        requires-python = ">=3.10"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }

        [package.metadata]
        requires-dist = [{ name = "package-a", marker = "python_full_version == '3.9.*'", specifier = "==1.0.0" }]
        "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(packse_index_url())
        .assert()
        .success();

    Ok(())
}

/// Check that we only include wheels that match the required Python version
///
/// ```text
/// requires-python-wheels
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   └── requires a==1.0.0
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
///         └── requires python>=3.8
/// ```
#[test]
fn requires_python_wheels() -> Result<()> {
    let context = TestContext::new("3.12");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"requires-python-wheels-", "package-"));

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''requires-python-wheels-a==1.0.0''',
        ]
        requires-python = ">=3.10"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        revision = 1
        requires-python = ">=3.10"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "package-a" },
        ]

        [package.metadata]
        requires-dist = [{ name = "package-a", specifier = "==1.0.0" }]

        [[package]]
        name = "package-a"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/requires_python_wheels_a-1.0.0.tar.gz", hash = "sha256:9a11ff73fdc513c4dab0d3e137f4145a00ef0dfc95154360c8f503eed62a03c9" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/requires_python_wheels_a-1.0.0-cp310-cp310-any.whl", hash = "sha256:b979494a0d7dc825b84d6c516ac407143915f6d2840d229ee2a36b3d06deb61d" },
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/requires_python_wheels_a-1.0.0-cp311-cp311-any.whl", hash = "sha256:b979494a0d7dc825b84d6c516ac407143915f6d2840d229ee2a36b3d06deb61d" },
        ]
        "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(packse_index_url())
        .assert()
        .success();

    Ok(())
}

/// `c` is not reachable due to the markers, it should be excluded from the lockfile
///
/// ```text
/// unreachable-package
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a==1.0.0; sys_platform == "win32"
/// │       └── satisfied by a-1.0.0
/// ├── a
/// │   └── a-1.0.0
/// │       └── requires b==1.0.0; sys_platform == "linux"
/// │           └── satisfied by b-1.0.0
/// └── b
///     └── b-1.0.0
/// ```
#[test]
fn unreachable_package() -> Result<()> {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"unreachable-package-", "package-"));

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''unreachable-package-a==1.0.0; sys_platform == "win32"''',
        ]
        requires-python = ">=3.8"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        revision = 1
        requires-python = ">=3.8"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "package-a", marker = "sys_platform == 'win32'" },
        ]

        [package.metadata]
        requires-dist = [{ name = "package-a", marker = "sys_platform == 'win32'", specifier = "==1.0.0" }]

        [[package]]
        name = "package-a"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/unreachable_package_a-1.0.0.tar.gz", hash = "sha256:308f0b6772e99dcb33acee38003b176e3acffbe01c3c511585db9a7d7ec008f7" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/unreachable_package_a-1.0.0-py3-none-any.whl", hash = "sha256:cc472ded9f3b260e6cda0e633fa407a13607e190422cb455f02beebd32d6751f" },
        ]
        "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(packse_index_url())
        .assert()
        .success();

    Ok(())
}

/// Check that we only include wheels that match the platform markers
///
/// ```text
/// unreachable-wheels
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   ├── requires a==1.0.0; sys_platform == "win32"
/// │   │   └── satisfied by a-1.0.0
/// │   ├── requires b==1.0.0; sys_platform == "linux"
/// │   │   └── satisfied by b-1.0.0
/// │   └── requires c==1.0.0; sys_platform == "darwin"
/// │       └── satisfied by c-1.0.0
/// ├── a
/// │   └── a-1.0.0
/// ├── b
/// │   └── b-1.0.0
/// └── c
///     └── c-1.0.0
/// ```
#[test]
fn unreachable_wheels() -> Result<()> {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"unreachable-wheels-", "package-"));

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''unreachable-wheels-a==1.0.0; sys_platform == "win32"''',
          '''unreachable-wheels-b==1.0.0; sys_platform == "linux"''',
          '''unreachable-wheels-c==1.0.0; sys_platform == "darwin"''',
        ]
        requires-python = ">=3.8"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        revision = 1
        requires-python = ">=3.8"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "package-a", marker = "sys_platform == 'win32'" },
            { name = "package-b", marker = "sys_platform == 'linux'" },
            { name = "package-c", marker = "sys_platform == 'darwin'" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "package-a", marker = "sys_platform == 'win32'", specifier = "==1.0.0" },
            { name = "package-b", marker = "sys_platform == 'linux'", specifier = "==1.0.0" },
            { name = "package-c", marker = "sys_platform == 'darwin'", specifier = "==1.0.0" },
        ]

        [[package]]
        name = "package-a"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/unreachable_wheels_a-1.0.0.tar.gz", hash = "sha256:91c6619d1cfa227f3662c0c062b1c0c16efe11e589db2f1836e809e2c6d9961e" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/unreachable_wheels_a-1.0.0-cp312-cp312-win_amd64.whl", hash = "sha256:e9fb30c5eb114114f9031d0ad2238614c2dcce203c5992848305ccda8f38a53e" },
        ]

        [[package]]
        name = "package-b"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/unreachable_wheels_b-1.0.0.tar.gz", hash = "sha256:253ae69b963651cd5ac16601a445e2e179db9eac552e8cfc37aadf73a88931ed" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/unreachable_wheels_b-1.0.0-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:a3de2212ca86f1137324965899ce7f48640ed8db94578f4078d641520b77e13e" },
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/unreachable_wheels_b-1.0.0-cp312-cp312-musllinux_1_1_armv7l.whl", hash = "sha256:a3de2212ca86f1137324965899ce7f48640ed8db94578f4078d641520b77e13e" },
        ]

        [[package]]
        name = "package-c"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/unreachable_wheels_c-1.0.0.tar.gz", hash = "sha256:5c4783e85f0fa57b720fd02b5c7e0ff8bc98121546fe2cce435710efe4a34b28" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/unreachable_wheels_c-1.0.0-cp312-cp312-macosx_14_0_x86_64.whl", hash = "sha256:4b846c5b1646b04828a2bef6c9d180ff7cfd725866013dcec8933de7fb5f9e8d" },
        ]
        "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(packse_index_url())
        .assert()
        .success();

    Ok(())
}

/// When a dependency is only required on a specific platform (like x86_64), omit wheels that target other platforms (like aarch64).
///
/// ```text
/// specific-architecture
/// ├── environment
/// │   └── python3.8
/// ├── root
/// │   └── requires a
/// │       └── satisfied by a-1.0.0
/// ├── a
/// │   └── a-1.0.0
/// │       ├── requires b; platform_machine == "x86_64"
/// │       │   └── satisfied by b-1.0.0
/// │       ├── requires c; platform_machine == "aarch64"
/// │       │   └── satisfied by c-1.0.0
/// │       └── requires d; platform_machine == "i686"
/// │           └── satisfied by d-1.0.0
/// ├── b
/// │   └── b-1.0.0
/// ├── c
/// │   └── c-1.0.0
/// └── d
///     └── d-1.0.0
/// ```
#[test]
fn specific_architecture() -> Result<()> {
    let context = TestContext::new("3.8");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"specific-architecture-", "package-"));

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''specific-architecture-a''',
        ]
        requires-python = ">=3.8"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        revision = 1
        requires-python = ">=3.8"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "package-a" },
        ]

        [package.metadata]
        requires-dist = [{ name = "package-a" }]

        [[package]]
        name = "package-a"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        dependencies = [
            { name = "package-b", marker = "platform_machine == 'x86_64'" },
            { name = "package-c", marker = "platform_machine == 'aarch64'" },
            { name = "package-d", marker = "platform_machine == 'i686'" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/specific_architecture_a-1.0.0.tar.gz", hash = "sha256:3543f7e4bc8aaf16a9705e07df1521f40a77407c7a33a82424b35ef63df8224a" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/specific_architecture_a-1.0.0-py3-none-any.whl", hash = "sha256:cd2f9894093805af0749592e8239d62e7a724476a74c4cb65da30bc6a3900046" },
        ]

        [[package]]
        name = "package-b"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/specific_architecture_b-1.0.0-cp313-cp313-freebsd_13_aarch64.whl", hash = "sha256:4ce70a68440d4aaa31cc1c6174b83b741e9b8f3074ad0f3ef41c572795378999" },
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/specific_architecture_b-1.0.0-cp313-cp313-freebsd_13_x86_64.whl", hash = "sha256:4ce70a68440d4aaa31cc1c6174b83b741e9b8f3074ad0f3ef41c572795378999" },
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/specific_architecture_b-1.0.0-cp313-cp313-macosx_10_9_x86_64.whl", hash = "sha256:4ce70a68440d4aaa31cc1c6174b83b741e9b8f3074ad0f3ef41c572795378999" },
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/specific_architecture_b-1.0.0-cp313-cp313-manylinux2010_x86_64.whl", hash = "sha256:4ce70a68440d4aaa31cc1c6174b83b741e9b8f3074ad0f3ef41c572795378999" },
        ]

        [[package]]
        name = "package-c"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/specific_architecture_c-1.0.0-cp313-cp313-freebsd_13_aarch64.whl", hash = "sha256:b028c88fe496724cea4a7d95eb789a000b7f000067f95c922b09461be2746a3d" },
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/specific_architecture_c-1.0.0-cp313-cp313-freebsd_13_x86_64.whl", hash = "sha256:b028c88fe496724cea4a7d95eb789a000b7f000067f95c922b09461be2746a3d" },
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/specific_architecture_c-1.0.0-cp313-cp313-macosx_10_9_arm64.whl", hash = "sha256:b028c88fe496724cea4a7d95eb789a000b7f000067f95c922b09461be2746a3d" },
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/specific_architecture_c-1.0.0-cp313-cp313-manylinux2010_aarch64.whl", hash = "sha256:b028c88fe496724cea4a7d95eb789a000b7f000067f95c922b09461be2746a3d" },
        ]

        [[package]]
        name = "package-d"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/specific_architecture_d-1.0.0-cp313-cp313-freebsd_13_aarch64.whl", hash = "sha256:842864c1348694fab33199eb05921602c2abfc77844a81085a55db02edd30da4" },
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/specific_architecture_d-1.0.0-cp313-cp313-freebsd_13_x86_64.whl", hash = "sha256:842864c1348694fab33199eb05921602c2abfc77844a81085a55db02edd30da4" },
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/specific_architecture_d-1.0.0-cp313-cp313-manylinux2010_i686.whl", hash = "sha256:842864c1348694fab33199eb05921602c2abfc77844a81085a55db02edd30da4" },
        ]
        "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(packse_index_url())
        .assert()
        .success();

    Ok(())
}
