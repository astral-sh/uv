//! DO NOT EDIT
//!
//! Generated with `uv run scripts/scenarios/generate.py`
//! Scenarios from <test/scenarios>
//!
#![cfg(feature = "test-python")]
#![expect(clippy::needless_raw_string_hashes)]
#![expect(clippy::doc_markdown)]
#![expect(clippy::doc_lazy_continuation)]

use anyhow::Result;
use assert_cmd::assert::OutputAssertExt;
use assert_fs::prelude::*;
use insta::assert_snapshot;

use uv_static::EnvVars;

use uv_test::packse::PackseServer;
use uv_test::uv_snapshot;

/// There are two packages, `a` and `b`. We select `a` with `a==2.0.0` first, and then `b`, but `a==2.0.0` conflicts with all new versions of `b`, so we backtrack through versions of `b`.
///
/// We need to detect this conflict and prioritize `b` over `a` instead of backtracking down to the too old version of `b==1.0.0` that doesn't depend on `a` anymore.
///
/// ```text
/// wrong-backtracking-basic
/// ├── environment
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("backtracking/wrong-backtracking-basic.toml");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''a''',
          '''b''',
        ]
        requires-python = ">=3.12"
        "###,
    )?;

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @r###"<snapshot>
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"<snapshot>
            "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(server.index_url())
        .assert()
        .success();

    Ok(())
}

/// There are three packages, `a`, `b` and `b-inner`. Unlike wrong-backtracking-basic, `b` depends on `b-inner` and `a` and `b-inner` conflict, to add a layer of indirection.
///
/// We select `a` with `a==2.0.0` first, then `b`, and then `b-inner`, but `a==2.0.0` conflicts with all new versions of `b-inner`, so we backtrack through versions of `b-inner`.
///
/// We need to detect this conflict and prioritize `b` and `b-inner` over `a` instead of backtracking down to the too old version of `b-inner==1.0.0` that doesn't depend on `a` anymore.
///
/// ```text
/// wrong-backtracking-indirect
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   ├── requires a
/// │   │   ├── satisfied by a-1.0.0
/// │   │   └── satisfied by a-2.0.0
/// │   └── requires b
/// │       └── satisfied by b-1.0.0
/// ├── a
/// │   ├── a-1.0.0
/// │   └── a-2.0.0
/// ├── b
/// │   └── b-1.0.0
/// │       └── requires b-inner
/// │           ├── satisfied by b-inner-1.0.0
/// │           ├── satisfied by b-inner-2.0.0
/// │           ├── satisfied by b-inner-2.0.1
/// │           ├── satisfied by b-inner-2.0.2
/// │           ├── satisfied by b-inner-2.0.3
/// │           ├── satisfied by b-inner-2.0.4
/// │           ├── satisfied by b-inner-2.0.5
/// │           ├── satisfied by b-inner-2.0.6
/// │           ├── satisfied by b-inner-2.0.7
/// │           ├── satisfied by b-inner-2.0.8
/// │           └── satisfied by b-inner-2.0.9
/// ├── b-inner
/// │   ├── b-inner-1.0.0
/// │   │   └── requires too-old
/// │   │       └── satisfied by too-old-1.0.0
/// │   ├── b-inner-2.0.0
/// │   │   └── requires a==1.0.0
/// │   │       └── satisfied by a-1.0.0
/// │   ├── b-inner-2.0.1
/// │   │   └── requires a==1.0.0
/// │   │       └── satisfied by a-1.0.0
/// │   ├── b-inner-2.0.2
/// │   │   └── requires a==1.0.0
/// │   │       └── satisfied by a-1.0.0
/// │   ├── b-inner-2.0.3
/// │   │   └── requires a==1.0.0
/// │   │       └── satisfied by a-1.0.0
/// │   ├── b-inner-2.0.4
/// │   │   └── requires a==1.0.0
/// │   │       └── satisfied by a-1.0.0
/// │   ├── b-inner-2.0.5
/// │   │   └── requires a==1.0.0
/// │   │       └── satisfied by a-1.0.0
/// │   ├── b-inner-2.0.6
/// │   │   └── requires a==1.0.0
/// │   │       └── satisfied by a-1.0.0
/// │   ├── b-inner-2.0.7
/// │   │   └── requires a==1.0.0
/// │   │       └── satisfied by a-1.0.0
/// │   ├── b-inner-2.0.8
/// │   │   └── requires a==1.0.0
/// │   │       └── satisfied by a-1.0.0
/// │   └── b-inner-2.0.9
/// │       └── requires a==1.0.0
/// │           └── satisfied by a-1.0.0
/// └── too-old
///     └── too-old-1.0.0
/// ```
#[test]
fn wrong_backtracking_indirect() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("backtracking/wrong-backtracking-indirect.toml");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''a''',
          '''b''',
        ]
        requires-python = ">=3.12"
        "###,
    )?;

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @r###"<snapshot>
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"<snapshot>
            "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(server.index_url())
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
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/allows-non-conflicting-non-overlapping-dependencies.toml");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''a>=1 ; sys_platform == 'linux'''',
          '''a<2 ; sys_platform == 'darwin'''',
        ]
        requires-python = ">=3.12"
        "###,
    )?;

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @r###"<snapshot>
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"<snapshot>
            "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(server.index_url())
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
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/allows-non-conflicting-repeated-dependencies.toml");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''a>=1''',
          '''a<2''',
        ]
        requires-python = ">=3.12"
        "###,
    )?;

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @r###"<snapshot>
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"<snapshot>
            "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(server.index_url())
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
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/basic.toml");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''a>=2 ; sys_platform == 'linux'''',
          '''a<2 ; sys_platform == 'darwin'''',
        ]
        requires-python = ">=3.12"
        "###,
    )?;

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @r###"<snapshot>
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"<snapshot>
            "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(server.index_url())
        .assert()
        .success();

    Ok(())
}

/// We have a conflict after forking. This scenario exists to test the error message.
///
/// ```text
/// conflict-in-fork
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   ├── requires a>=2; sys_platform == "os1"
/// │   │   └── satisfied by a-2.0.0
/// │   └── requires a<2; sys_platform == "os2"
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/conflict-in-fork.toml");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''a>=2 ; sys_platform == 'os1'''',
          '''a<2 ; sys_platform == 'os2'''',
        ]
        requires-python = ">=3.12"
        "###,
    )?;

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @r###"<snapshot>
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
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/conflict-unsatisfiable.toml");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''a>=2''',
          '''a<2''',
        ]
        requires-python = ">=3.12"
        "###,
    )?;

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @r###"<snapshot>
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
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/filter-sibling-dependencies.toml");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''a==4.4.0 ; sys_platform == 'linux'''',
          '''a==4.3.0 ; sys_platform == 'darwin'''',
          '''b==1.0.0 ; sys_platform == 'linux'''',
          '''c==1.0.0 ; sys_platform == 'darwin'''',
        ]
        requires-python = ">=3.12"
        "###,
    )?;

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @r###"<snapshot>
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"<snapshot>
            "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(server.index_url())
        .assert()
        .success();

    Ok(())
}

/// This test checks that we discard fork markers when using `--upgrade`.
///
/// ```text
/// fork-upgrade
/// ├── environment
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/fork-upgrade.toml");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''foo''',
        ]
        requires-python = ">=3.12"
        "###,
    )?;

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @r###"<snapshot>
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"<snapshot>
            "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(server.index_url())
        .assert()
        .success();

    Ok(())
}

/// The root cause the resolver to fork over `a`, but the markers on the variant
/// of `a` don't cover the entire marker space, they are missing Python 3.13.
/// Later, we have a dependency this very hole, which we still need to select,
/// instead of having two forks around but without Python 3.13 and omitting
/// `c` from the solution.
///
/// ```text
/// fork-incomplete-markers
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   ├── requires a==1; python_version < "3.13"
/// │   │   └── satisfied by a-1.0.0
/// │   ├── requires a==2; python_version >= "3.14"
/// │   │   └── satisfied by a-2.0.0
/// │   └── requires b
/// │       └── satisfied by b-1.0.0
/// ├── a
/// │   ├── a-1.0.0
/// │   └── a-2.0.0
/// ├── b
/// │   └── b-1.0.0
/// │       └── requires c; python_version == "3.13"
/// │           └── satisfied by c-1.0.0
/// └── c
///     └── c-1.0.0
/// ```
#[test]
fn fork_incomplete_markers() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/incomplete-markers.toml");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''a==1; python_version < '3.13'''',
          '''a==2; python_version >= '3.14'''',
          '''b''',
        ]
        requires-python = ">=3.12"
        "###,
    )?;

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @r###"<snapshot>
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"<snapshot>
            "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(server.index_url())
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
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/marker-accrue.toml");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''a==1.0.0 ; implementation_name == 'cpython'''',
          '''b==1.0.0 ; implementation_name == 'pypy'''',
        ]
        requires-python = ">=3.12"
        "###,
    )?;

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @r###"<snapshot>
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"<snapshot>
            "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(server.index_url())
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
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/marker-disjoint.toml");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''a>=2 ; sys_platform == 'linux'''',
          '''a<2 ; sys_platform == 'linux'''',
        ]
        requires-python = ">=3.12"
        "###,
    )?;

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @r###"<snapshot>
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
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/marker-inherit-combined-allowed.toml");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''a>=2 ; sys_platform == 'linux'''',
          '''a<2 ; sys_platform == 'darwin'''',
        ]
        requires-python = ">=3.12"
        "###,
    )?;

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @r###"<snapshot>
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"<snapshot>
            "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(server.index_url())
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
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/marker-inherit-combined-disallowed.toml");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''a>=2 ; sys_platform == 'linux'''',
          '''a<2 ; sys_platform == 'darwin'''',
        ]
        requires-python = ">=3.12"
        "###,
    )?;

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @r###"<snapshot>
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"<snapshot>
            "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(server.index_url())
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
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/marker-inherit-combined.toml");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''a>=2 ; sys_platform == 'linux'''',
          '''a<2 ; sys_platform == 'darwin'''',
        ]
        requires-python = ">=3.12"
        "###,
    )?;

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @r###"<snapshot>
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"<snapshot>
            "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(server.index_url())
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
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/marker-inherit-isolated.toml");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''a>=2 ; sys_platform == 'linux'''',
          '''a<2 ; sys_platform == 'darwin'''',
        ]
        requires-python = ">=3.12"
        "###,
    )?;

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @r###"<snapshot>
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"<snapshot>
            "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(server.index_url())
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
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/marker-inherit-transitive.toml");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''a>=2 ; sys_platform == 'linux'''',
          '''a<2 ; sys_platform == 'darwin'''',
        ]
        requires-python = ">=3.12"
        "###,
    )?;

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @r###"<snapshot>
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"<snapshot>
            "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(server.index_url())
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
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/marker-inherit.toml");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''a>=2 ; sys_platform == 'linux'''',
          '''a<2 ; sys_platform == 'darwin'''',
        ]
        requires-python = ">=3.12"
        "###,
    )?;

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @r###"<snapshot>
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"<snapshot>
            "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(server.index_url())
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
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/marker-limited-inherit.toml");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''a>=2 ; sys_platform == 'linux'''',
          '''a<2 ; sys_platform == 'darwin'''',
          '''b''',
        ]
        requires-python = ">=3.12"
        "###,
    )?;

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @r###"<snapshot>
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"<snapshot>
            "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(server.index_url())
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
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/marker-selection.toml");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''a''',
          '''b>=2 ; sys_platform == 'linux'''',
          '''b<2 ; sys_platform == 'darwin'''',
        ]
        requires-python = ">=3.12"
        "###,
    )?;

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @r###"<snapshot>
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"<snapshot>
            "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(server.index_url())
        .assert()
        .success();

    Ok(())
}

/// fork-marker-track
///
/// ```text
/// fork-marker-track
/// ├── environment
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/marker-track.toml");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''a''',
          '''b>=2.8 ; sys_platform == 'linux'''',
          '''b<2.8 ; sys_platform == 'darwin'''',
        ]
        requires-python = ">=3.12"
        "###,
    )?;

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @r###"<snapshot>
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"<snapshot>
            "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(server.index_url())
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
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/non-fork-marker-transitive.toml");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''a==1.0.0''',
          '''b==1.0.0''',
        ]
        requires-python = ">=3.12"
        "###,
    )?;

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @r###"<snapshot>
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"<snapshot>
            "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(server.index_url())
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
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/non-local-fork-marker-direct.toml");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''a==1.0.0 ; sys_platform == 'linux'''',
          '''b==1.0.0 ; sys_platform == 'darwin'''',
        ]
        requires-python = ">=3.12"
        "###,
    )?;

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @r###"<snapshot>
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
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/non-local-fork-marker-transitive.toml");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''a==1.0.0''',
          '''b==1.0.0''',
        ]
        requires-python = ">=3.12"
        "###,
    )?;

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @r###"<snapshot>
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
/// `python_version < '3.13'` and the other marker expressions, so this
/// isn't just a scenario with overlapping but non-disjoint markers.
///
/// In particular, this serves as a regression test. uv used to create a
/// lock file with a dependency on `a` with the following markers:
///
///     python_version < '3.13' or python_version >= '3.14'
///
/// But this implies that `a` won't be installed for Python 3.13, which is
/// clearly wrong.
///
/// The issue was that uv was intersecting *all* marker expressions. So
/// that `a>=1.1.0` and `a>=1.2.0` fork was getting `python_version >=
/// '3.13' and python_version >= '3.14'`, which, of course, simplifies
/// to `python_version >= '3.14'`. But this is wrong! It should be
/// `python_version >= '3.13' or python_version >= '3.14'`, which of course
/// simplifies to `python_version >= '3.13'`. And thus, the resulting forks
/// are not just disjoint but complete in this case.
///
/// Since there are no other constraints on `a`, this causes uv to select
/// `1.2.0` unconditionally. (The marker expressions get normalized out
/// entirely.)
///
/// ```text
/// fork-overlapping-markers-basic
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   ├── requires a>=1.0.0; python_version < "3.13"
/// │   │   ├── satisfied by a-1.0.0
/// │   │   ├── satisfied by a-1.1.0
/// │   │   └── satisfied by a-1.2.0
/// │   ├── requires a>=1.1.0; python_version >= "3.13"
/// │   │   ├── satisfied by a-1.1.0
/// │   │   └── satisfied by a-1.2.0
/// │   └── requires a>=1.2.0; python_version >= "3.14"
/// │       └── satisfied by a-1.2.0
/// └── a
///     ├── a-1.0.0
///     ├── a-1.1.0
///     └── a-1.2.0
/// ```
#[test]
fn fork_overlapping_markers_basic() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/overlapping-markers-basic.toml");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''a>=1.0.0 ; python_version < '3.13'''',
          '''a>=1.1.0 ; python_version >= '3.13'''',
          '''a>=1.2.0 ; python_version >= '3.14'''',
        ]
        requires-python = ">=3.12"
        "###,
    )?;

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @r###"<snapshot>
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"<snapshot>
            "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(server.index_url())
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
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/preferences-dependent-forking-bistable.toml");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''cleaver''',
        ]
        requires-python = ">=3.12"
        "###,
    )?;

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @r###"<snapshot>
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"<snapshot>
            "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(server.index_url())
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
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/preferences-dependent-forking-conflicting.toml");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''cleaver''',
          '''foo''',
          '''bar''',
        ]
        requires-python = ">=3.12"
        "###,
    )?;

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @r###"<snapshot>
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
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/preferences-dependent-forking-tristable.toml");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''cleaver''',
          '''foo''',
          '''bar''',
        ]
        requires-python = ">=3.12"
        "###,
    )?;

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @r###"<snapshot>
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"<snapshot>
            "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(server.index_url())
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
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/preferences-dependent-forking.toml");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''cleaver''',
          '''foo''',
          '''bar''',
        ]
        requires-python = ">=3.12"
        "###,
    )?;

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @r###"<snapshot>
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"<snapshot>
            "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(server.index_url())
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
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/remaining-universe-partitioning.toml");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''a>=2 ; sys_platform == 'windows'''',
          '''a<2 ; sys_platform == 'illumos'''',
        ]
        requires-python = ">=3.12"
        "###,
    )?;

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @r###"<snapshot>
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"<snapshot>
            "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(server.index_url())
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
/// ```
#[test]
fn fork_requires_python_full_prerelease() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/requires-python-full-prerelease.toml");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''a==1.0.0 ; python_full_version == '3.9b1'''',
        ]
        requires-python = ">=3.10"
        "###,
    )?;

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @r###"<snapshot>
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"<snapshot>
            "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(server.index_url())
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
/// ```
#[test]
fn fork_requires_python_full() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/requires-python-full.toml");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''a==1.0.0 ; python_full_version == '3.9'''',
        ]
        requires-python = ">=3.10"
        "###,
    )?;

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @r###"<snapshot>
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"<snapshot>
            "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(server.index_url())
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
///         └── requires python>=3.10
/// ```
#[test]
fn fork_requires_python_patch_overlap() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/requires-python-patch-overlap.toml");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''a==1.0.0 ; python_version == '3.10'''',
        ]
        requires-python = ">=3.10.1"
        "###,
    )?;

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @r###"<snapshot>
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"<snapshot>
            "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(server.index_url())
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
/// ```
#[test]
fn fork_requires_python() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/requires-python.toml");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''a==1.0.0 ; python_version == '3.9'''',
        ]
        requires-python = ">=3.10"
        "###,
    )?;

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @r###"<snapshot>
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"<snapshot>
            "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(server.index_url())
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
///         └── requires python>=3.10
/// ```
#[test]
fn requires_python_wheels() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("tag_and_markers/requires-python-wheels.toml");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''a==1.0.0''',
        ]
        requires-python = ">=3.10"
        "###,
    )?;

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @r###"<snapshot>
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"<snapshot>
            "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(server.index_url())
        .assert()
        .success();

    Ok(())
}

/// `c` is not reachable due to the markers, it should be excluded from the lockfile
///
/// ```text
/// unreachable-package
/// ├── environment
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("tag_and_markers/unreachable-package.toml");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''a==1.0.0; sys_platform == 'win32'''',
        ]
        requires-python = ">=3.12"
        "###,
    )?;

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @r###"<snapshot>
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"<snapshot>
            "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(server.index_url())
        .assert()
        .success();

    Ok(())
}

/// Check that we only include wheels that match the platform markers
///
/// ```text
/// unreachable-wheels
/// ├── environment
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("tag_and_markers/unreachable-wheels.toml");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''a==1.0.0; sys_platform == 'win32'''',
          '''b==1.0.0; sys_platform == 'linux'''',
          '''c==1.0.0; sys_platform == 'darwin'''',
        ]
        requires-python = ">=3.12"
        "###,
    )?;

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @r###"<snapshot>
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"<snapshot>
            "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(server.index_url())
        .assert()
        .success();

    Ok(())
}

/// Check the prioritization for virtual extra and marker packages
///
/// ```text
/// marker-variants-have-different-extras
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   ├── requires psycopg[binary]; platform_python_implementation != "PyPy"
/// │   │   ├── satisfied by psycopg-1.0.0
/// │   │   └── satisfied by psycopg-1.0.0[binary]
/// │   └── requires psycopg; platform_python_implementation == "PyPy"
/// │       ├── satisfied by psycopg-1.0.0
/// │       └── satisfied by psycopg-1.0.0[binary]
/// ├── psycopg
/// │   ├── psycopg-1.0.0
/// │   │   └── requires tzdata; sys_platform == "win32"
/// │   │       └── satisfied by tzdata-1.0.0
/// │   └── psycopg-1.0.0[binary]
/// │       └── requires psycopg-binary; implementation_name != "pypy"
/// │           └── satisfied by psycopg-binary-1.0.0
/// ├── psycopg-binary
/// │   └── psycopg-binary-1.0.0
/// └── tzdata
///     └── tzdata-1.0.0
/// ```
#[test]
fn marker_variants_have_different_extras() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("tag_and_markers/virtual-package-extra-priorities.toml");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''psycopg[binary] ; platform_python_implementation != 'PyPy'''',
          '''psycopg ; platform_python_implementation == 'PyPy'''',
        ]
        requires-python = ">=3.12"
        "###,
    )?;

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @r###"<snapshot>
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"<snapshot>
            "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(server.index_url())
        .assert()
        .success();

    Ok(())
}

/// Check the prioritization for virtual marker packages
///
/// ```text
/// virtual-package-extra-priorities
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   ├── requires a==1; python_version >= "3.8"
/// │   │   └── satisfied by a-1.0.0
/// │   └── requires b; python_version >= "3.9"
/// │       ├── satisfied by b-1.0.0
/// │       └── satisfied by b-2.0.0
/// ├── a
/// │   ├── a-1.0.0
/// │   │   └── requires b==1; python_version >= "3.10"
/// │   │       └── satisfied by b-1.0.0
/// │   └── a-2.0.0
/// │       └── requires b==1; python_version >= "3.10"
/// │           └── satisfied by b-1.0.0
/// └── b
///     ├── b-1.0.0
///     └── b-2.0.0
/// ```
#[test]
fn virtual_package_extra_priorities() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("tag_and_markers/virtual-package-marker-priorities.toml");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''a==1; python_version >= '3.8'''',
          '''b; python_version >= '3.9'''',
        ]
        requires-python = ">=3.12"
        "###,
    )?;

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @r###"<snapshot>
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"<snapshot>
            "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(server.index_url())
        .assert()
        .success();

    Ok(())
}
/// While both Linux and Windows are required and `win-only` has only a Windows wheel, `win-only` is also used only on Windows.
///
/// ```text
/// requires-python-subset
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   └── requires win-only; sys_platform == "win32"
/// │       └── satisfied by win-only-1.0.0
/// └── win-only
///     └── win-only-1.0.0
/// ```
#[test]
fn requires_python_subset() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("wheels/requires-python-subset.toml");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''win-only; sys_platform == 'win32'''',
        ]
        requires-python = ">=3.12"
        [tool.uv]
        required-environments = [
          '''sys_platform == "linux"''',
          '''sys_platform == "win32"''',
        ]
        "###,
    )?;

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @r###"<snapshot>
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"<snapshot>
            "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(server.index_url())
        .assert()
        .success();

    Ok(())
}

/// When a dependency is only required on a specific platform (like x86_64), omit wheels that target other platforms (like aarch64).
///
/// ```text
/// specific-architecture
/// ├── environment
/// │   └── python3.12
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
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("wheels/specific-architecture.toml");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''a''',
        ]
        requires-python = ">=3.12"
        "###,
    )?;

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @r###"<snapshot>
    "###
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"<snapshot>
            "###
        );
    });

    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).
    context
        .lock()
        .arg("--locked")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index-url")
        .arg(server.index_url())
        .assert()
        .success();

    Ok(())
}
