//! DO NOT EDIT
//!
//! Generated with `cargo dev generate-scenario-tests`
//! Scenarios from <test/scenarios>
//!
#![cfg(all(feature = "test-python", feature = "test-pypi"))]
#![expect(clippy::needless_raw_string_hashes)]
#![expect(clippy::doc_markdown)]

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

    let filters = context.filters();

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [[package]]
        name = "a"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:957f99ff1d65ce0d7883d50f4e67ed8d4b42e76d2c2b5e62384ff0ba538647b5", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-any.whl", hash = "sha256:f936eedc194aa91ca01a4c6c9981136ca6c75ce6df47e3951b12522881dce809", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "2.0.9"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "a" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/b-2.0.9.tar.gz", hash = "sha256:8a0dca91cfb1e865caa23018dc01a32afc0ede285eb653a34e87929401af0152", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-2.0.9-py3-none-any.whl", hash = "sha256:fb91402b66338aaf9408407aa3681dcdd0984b9774ecf46632bc3761198399fa", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "a" },
            { name = "b" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "a" },
            { name = "b" },
        ]
        "#
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

    let filters = context.filters();

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    "
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [[package]]
        name = "a"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/a-2.0.0.tar.gz", hash = "sha256:9610291c2bd57390019f58ca72d0dd4584bb9e7073fa347633ed8bc7267fccfe", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-2.0.0-py3-none-any.whl", hash = "sha256:833374310e0a15880f3be9e6d082f527c9ac70129b2054d733da9b754315361f", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "b-inner" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/b-1.0.0.tar.gz", hash = "sha256:dc8e65ac8e153f517377e576ac880386219fa74cad98ef9dc8ccc7ffaaebb55e", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-1.0.0-py3-none-any.whl", hash = "sha256:e64d7b65e2bc771f36c53ea70d805ebf643322fa2d1761a0dc45b75d0374e2fb", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b-inner"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "too-old" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/b_inner-1.0.0.tar.gz", hash = "sha256:9593374b380761095c60460348d2134a778370ccb51d1bb8a9893c7d51934c8c", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b_inner-1.0.0-py3-none-any.whl", hash = "sha256:dc550a3821df74da8f99f92aeff395eae8f050034fe8403a13043d42dca06a95", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "a" },
            { name = "b" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "a" },
            { name = "b" },
        ]

        [[package]]
        name = "too-old"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/too_old-1.0.0.tar.gz", hash = "sha256:91e0cdc85c04e313e2a5cf8b4ad6459e61594f62d91b04ad658ae44d48b1644a", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/too_old-1.0.0-py3-none-any.whl", hash = "sha256:7efb79d455d0a679335ce5abee7d3bf298cac8c6e0aa19654b7c033d603c93ef", upload-time = "2024-03-24T00:00:00Z" },
        ]
        "#
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
///
/// ```text
/// fork-allows-non-conflicting-non-overlapping-dependencies
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   ├── requires a>=1 ; sys_platform == 'linux'
/// │   │   ├── satisfied by a-1.0.0
/// │   │   └── satisfied by a-2.0.0
/// │   └── requires a<2 ; sys_platform == 'darwin'
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

    let filters = context.filters();

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"
        resolution-markers = [
            "sys_platform == 'darwin'",
            "sys_platform == 'linux'",
            "sys_platform != 'darwin' and sys_platform != 'linux'",
        ]

        [[package]]
        name = "a"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:957f99ff1d65ce0d7883d50f4e67ed8d4b42e76d2c2b5e62384ff0ba538647b5", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-any.whl", hash = "sha256:f936eedc194aa91ca01a4c6c9981136ca6c75ce6df47e3951b12522881dce809", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "a", marker = "sys_platform == 'darwin' or sys_platform == 'linux'" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "a", marker = "sys_platform == 'darwin'", specifier = "<2" },
            { name = "a", marker = "sys_platform == 'linux'", specifier = ">=1" },
        ]
        "#
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

    let filters = context.filters();

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [[package]]
        name = "a"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:957f99ff1d65ce0d7883d50f4e67ed8d4b42e76d2c2b5e62384ff0ba538647b5", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-any.whl", hash = "sha256:f936eedc194aa91ca01a4c6c9981136ca6c75ce6df47e3951b12522881dce809", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "a" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "a", specifier = "<2" },
            { name = "a", specifier = ">=1" },
        ]
        "#
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
///
/// ```text
/// fork-basic
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   ├── requires a>=2 ; sys_platform == 'linux'
/// │   │   └── satisfied by a-2.0.0
/// │   └── requires a<2 ; sys_platform == 'darwin'
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

    let filters = context.filters();

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"
        resolution-markers = [
            "sys_platform == 'darwin'",
            "sys_platform == 'linux'",
            "sys_platform != 'darwin' and sys_platform != 'linux'",
        ]

        [[package]]
        name = "a"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'darwin'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:957f99ff1d65ce0d7883d50f4e67ed8d4b42e76d2c2b5e62384ff0ba538647b5", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-any.whl", hash = "sha256:f936eedc194aa91ca01a4c6c9981136ca6c75ce6df47e3951b12522881dce809", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "a"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-2.0.0.tar.gz", hash = "sha256:9610291c2bd57390019f58ca72d0dd4584bb9e7073fa347633ed8bc7267fccfe", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-2.0.0-py3-none-any.whl", hash = "sha256:833374310e0a15880f3be9e6d082f527c9ac70129b2054d733da9b754315361f", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "a", version = "1.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "sys_platform == 'darwin'" },
            { name = "a", version = "2.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "sys_platform == 'linux'" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "a", marker = "sys_platform == 'darwin'", specifier = "<2" },
            { name = "a", marker = "sys_platform == 'linux'", specifier = ">=2" },
        ]
        "#
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
///
/// ```text
/// conflict-in-fork
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   ├── requires a>=2 ; sys_platform == 'os1'
/// │   │   └── satisfied by a-2.0.0
/// │   └── requires a<2 ; sys_platform == 'os2'
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

    let filters = context.filters();

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies for split (markers: sys_platform == 'os2'):
      ╰─▶ Because only b==1.0.0 is available and b==1.0.0 depends on d==1, we can conclude that all versions of b depend on d==1.
          And because c==1.0.0 depends on d==2 and only c==1.0.0 is available, we can conclude that all versions of b and all versions of c are incompatible.
          And because a==1.0.0 depends on b and c, we can conclude that a==1.0.0 cannot be used.
          And because only the following versions of a{sys_platform == 'os2'} are available:
              a{sys_platform == 'os2'}==1.0.0
              a{sys_platform == 'os2'}>=2
          and your project depends on a{sys_platform == 'os2'}<2, we can conclude that your project's requirements are unsatisfiable.

    hint: The resolution failed for an environment that is not the current one, consider limiting the environments with `tool.uv.environments`.
    "
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

    let filters = context.filters();

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because your project depends on a>=2 and a<2, we can conclude that your project's requirements are unsatisfiable.
    "
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
///
/// ```text
/// fork-filter-sibling-dependencies
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   ├── requires a==4.4.0 ; sys_platform == 'linux'
/// │   │   └── satisfied by a-4.4.0
/// │   ├── requires a==4.3.0 ; sys_platform == 'darwin'
/// │   │   └── satisfied by a-4.3.0
/// │   ├── requires b==1.0.0 ; sys_platform == 'linux'
/// │   │   └── satisfied by b-1.0.0
/// │   └── requires c==1.0.0 ; sys_platform == 'darwin'
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

    let filters = context.filters();

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
    "
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"
        resolution-markers = [
            "sys_platform == 'linux'",
            "sys_platform == 'darwin'",
            "sys_platform != 'darwin' and sys_platform != 'linux'",
        ]

        [[package]]
        name = "a"
        version = "4.3.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'darwin'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-4.3.0.tar.gz", hash = "sha256:c827d6b38cef471d2cbb5c4a0ffd2b1b664fcaceb7468285291b09ae720cce85", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-4.3.0-py3-none-any.whl", hash = "sha256:801f46d2474bf22f2eed823a9a9343480a5ea089a45e949dbb1aad91a32cc14f", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "a"
        version = "4.4.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-4.4.0.tar.gz", hash = "sha256:44ef07c198ff128c43fea8095c6404169d67af81d7a2d5a9d14bf442d46141de", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-4.4.0-py3-none-any.whl", hash = "sha256:3df7c088229de8a0ee9765d9da6f543d5b40155fade904f0c95780fc843c8ed7", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "d", version = "1.0.0", source = { registry = "http://[LOCALHOST]/simple/" } },
        ]
        sdist = { url = "http://[LOCALHOST]/files/b-1.0.0.tar.gz", hash = "sha256:0c4fab34d536effcd612484e0340622ee38fc410901455c7f0a1099e1ac09bc3", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-1.0.0-py3-none-any.whl", hash = "sha256:ee44ff0b8963063959a61fdafa6d9742f5e03efec800a3661e4b081bb6343be1", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "c"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "d", version = "2.0.0", source = { registry = "http://[LOCALHOST]/simple/" } },
        ]
        sdist = { url = "http://[LOCALHOST]/files/c-1.0.0.tar.gz", hash = "sha256:9e5affe114cac181b24f22f6639d9a0dc3f1ec3e39752271f4ccdd23cac7957d", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/c-1.0.0-py3-none-any.whl", hash = "sha256:c243070ddb01de3029aebe2536aed4b43f324ebaef3a156304c9c4d7ae2f6a63", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "d"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/d-1.0.0.tar.gz", hash = "sha256:bbb9d05b6de19e47de8e49fcc69483e76cd868bd556c8173680756d53e6997d4", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/d-1.0.0-py3-none-any.whl", hash = "sha256:362166e5bd895367cc4ba5b7327949b7d417fe30cb3273a76b5db4a280dac05d", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "d"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'darwin'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/d-2.0.0.tar.gz", hash = "sha256:d5570875cb7c4bb8cba56f4e5aec850f5e6f21cb6ee0316b3a4f90be887edb75", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/d-2.0.0-py3-none-any.whl", hash = "sha256:3cf4357f0fdce5ed2a98b8befcef84e8b8150244737481ebc51d3d9ec000bc3b", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "a", version = "4.3.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "sys_platform == 'darwin'" },
            { name = "a", version = "4.4.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "sys_platform == 'linux'" },
            { name = "b", marker = "sys_platform == 'linux'" },
            { name = "c", marker = "sys_platform == 'darwin'" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "a", marker = "sys_platform == 'darwin'", specifier = "==4.3.0" },
            { name = "a", marker = "sys_platform == 'linux'", specifier = "==4.4.0" },
            { name = "b", marker = "sys_platform == 'linux'", specifier = "==1.0.0" },
            { name = "c", marker = "sys_platform == 'darwin'", specifier = "==1.0.0" },
        ]
        "#
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
///     │   ├── requires bar==1 ; sys_platform == 'linux'
///     │   │   └── satisfied by bar-1.0.0
///     │   └── requires bar==2 ; sys_platform != 'linux'
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

    let filters = context.filters();

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [[package]]
        name = "bar"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/bar-2.0.0.tar.gz", hash = "sha256:29e7bc76f76b7e939dcf1f8fe28b077c4631c11ecea673beb86d297dacda11eb", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/bar-2.0.0-py3-none-any.whl", hash = "sha256:563b1af3238a4ad819f2b95b74f940319a2ef30ed7991a2416fa98aa115da87d", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "foo"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "bar" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/foo-2.0.0.tar.gz", hash = "sha256:06645b1e3c66510cee0c5b852731f1144bbfa98700d36d77a3a1def134c32541", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/foo-2.0.0-py3-none-any.whl", hash = "sha256:36953b42725b8f3b6ebde327b8fab1d1e906ce0902c6272855851ea1964bc37a", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "foo" },
        ]

        [package.metadata]
        requires-dist = [{ name = "foo" }]
        "#
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
///
/// ```text
/// fork-incomplete-markers
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   ├── requires a==1 ; python_full_version < '3.13'
/// │   │   └── satisfied by a-1.0.0
/// │   ├── requires a==2 ; python_full_version >= '3.14'
/// │   │   └── satisfied by a-2.0.0
/// │   └── requires b
/// │       └── satisfied by b-1.0.0
/// ├── a
/// │   ├── a-1.0.0
/// │   └── a-2.0.0
/// ├── b
/// │   └── b-1.0.0
/// │       └── requires c ; python_full_version == '3.13.*'
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
          '''a==1 ; python_full_version < '3.13'''',
          '''a==2 ; python_full_version >= '3.14'''',
          '''b''',
        ]
        requires-python = ">=3.12"
        "###,
    )?;

    let filters = context.filters();

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    "
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"
        resolution-markers = [
            "python_full_version >= '3.14'",
            "python_full_version == '3.13.*'",
            "python_full_version < '3.13'",
        ]

        [[package]]
        name = "a"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "python_full_version < '3.13'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:957f99ff1d65ce0d7883d50f4e67ed8d4b42e76d2c2b5e62384ff0ba538647b5", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-any.whl", hash = "sha256:f936eedc194aa91ca01a4c6c9981136ca6c75ce6df47e3951b12522881dce809", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "a"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "python_full_version >= '3.14'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-2.0.0.tar.gz", hash = "sha256:9610291c2bd57390019f58ca72d0dd4584bb9e7073fa347633ed8bc7267fccfe", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-2.0.0-py3-none-any.whl", hash = "sha256:833374310e0a15880f3be9e6d082f527c9ac70129b2054d733da9b754315361f", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "c", marker = "python_full_version == '3.13.*'" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/b-1.0.0.tar.gz", hash = "sha256:675547bdd1ec8b9552c086605f0ca400a2ef057934366281d0b357127e216384", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-1.0.0-py3-none-any.whl", hash = "sha256:b8c3f2688065abd235cc88b32446d4c807cfa3a1f2d676874bccc7e0f63137bd", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "c"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/c-1.0.0.tar.gz", hash = "sha256:699a07ff61aab66fcba4883a94c6d2b61afb7797fa956ae36f2efdf30d9dfbc7", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/c-1.0.0-py3-none-any.whl", hash = "sha256:78c0da7c5681d751d38b2e60c78d1e29d6125d91e68e5aeb22372fa66527ff95", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "a", version = "1.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "python_full_version < '3.13'" },
            { name = "a", version = "2.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "python_full_version >= '3.14'" },
            { name = "b" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "a", marker = "python_full_version < '3.13'", specifier = "==1" },
            { name = "a", marker = "python_full_version >= '3.14'", specifier = "==2" },
            { name = "b" },
        ]
        "#
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
///
/// ```text
/// fork-marker-accrue
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   ├── requires a==1.0.0 ; implementation_name == 'cpython'
/// │   │   └── satisfied by a-1.0.0
/// │   └── requires b==1.0.0 ; implementation_name == 'pypy'
/// │       └── satisfied by b-1.0.0
/// ├── a
/// │   └── a-1.0.0
/// │       └── requires c==1.0.0 ; sys_platform == 'linux'
/// │           └── satisfied by c-1.0.0
/// ├── b
/// │   └── b-1.0.0
/// │       └── requires c==1.0.0 ; sys_platform == 'darwin'
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

    let filters = context.filters();

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [[package]]
        name = "a"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "c", marker = "sys_platform == 'linux'" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:c6a69af4dc542ea244c21aae42b5639705975100871f1066c58be1245f013c95", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-any.whl", hash = "sha256:8ed25f2453465e5f1e91e7b09e21b08234389dc5d6e3f7f27e89e801ba42d807", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "c", marker = "sys_platform == 'darwin'" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/b-1.0.0.tar.gz", hash = "sha256:c855f831e0a345fc9b086051e2a8b02be6f9356bb131f240da96a793bb1d4d1c", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-1.0.0-py3-none-any.whl", hash = "sha256:e21416492e59fd38a6e084b303f29fcde08805f201a4f9e4833c60cd18ebfc4f", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "c"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/c-1.0.0.tar.gz", hash = "sha256:699a07ff61aab66fcba4883a94c6d2b61afb7797fa956ae36f2efdf30d9dfbc7", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/c-1.0.0-py3-none-any.whl", hash = "sha256:78c0da7c5681d751d38b2e60c78d1e29d6125d91e68e5aeb22372fa66527ff95", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "a", marker = "implementation_name == 'cpython'" },
            { name = "b", marker = "implementation_name == 'pypy'" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "a", marker = "implementation_name == 'cpython'", specifier = "==1.0.0" },
            { name = "b", marker = "implementation_name == 'pypy'", specifier = "==1.0.0" },
        ]
        "#
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
///
/// ```text
/// fork-marker-disjoint
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   ├── requires a>=2 ; sys_platform == 'linux'
/// │   │   └── satisfied by a-2.0.0
/// │   └── requires a<2 ; sys_platform == 'linux'
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

    let filters = context.filters();

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because your project depends on a{sys_platform == 'linux'}>=2 and a{sys_platform == 'linux'}<2, we can conclude that your project's requirements are unsatisfiable.
    "
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
///
/// ```text
/// fork-marker-inherit-combined-allowed
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   ├── requires a>=2 ; sys_platform == 'linux'
/// │   │   └── satisfied by a-2.0.0
/// │   └── requires a<2 ; sys_platform == 'darwin'
/// │       └── satisfied by a-1.0.0
/// ├── a
/// │   ├── a-1.0.0
/// │   │   ├── requires b>=2 ; implementation_name == 'cpython'
/// │   │   │   └── satisfied by b-2.0.0
/// │   │   └── requires b<2 ; implementation_name == 'pypy'
/// │   │       └── satisfied by b-1.0.0
/// │   └── a-2.0.0
/// ├── b
/// │   ├── b-1.0.0
/// │   │   └── requires c ; implementation_name == 'pypy' or sys_platform == 'linux'
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

    let filters = context.filters();

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    "
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"
        resolution-markers = [
            "implementation_name == 'pypy' and sys_platform == 'darwin'",
            "implementation_name == 'cpython' and sys_platform == 'darwin'",
            "implementation_name != 'cpython' and implementation_name != 'pypy' and sys_platform == 'darwin'",
            "sys_platform == 'linux'",
            "sys_platform != 'darwin' and sys_platform != 'linux'",
        ]

        [[package]]
        name = "a"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "implementation_name == 'pypy' and sys_platform == 'darwin'",
            "implementation_name == 'cpython' and sys_platform == 'darwin'",
            "implementation_name != 'cpython' and implementation_name != 'pypy' and sys_platform == 'darwin'",
        ]
        dependencies = [
            { name = "b", version = "1.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "implementation_name == 'pypy'" },
            { name = "b", version = "2.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "implementation_name == 'cpython'" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:23d75e1acf1aaf735e83615f8baba2fa0d0e5f9b885706cfd017b9b72301cdab", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-any.whl", hash = "sha256:ebe588eab684413e5969ec398e03f7386a8106c5c88a601dacb2781fe2d0c819", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "a"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-2.0.0.tar.gz", hash = "sha256:9610291c2bd57390019f58ca72d0dd4584bb9e7073fa347633ed8bc7267fccfe", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-2.0.0-py3-none-any.whl", hash = "sha256:833374310e0a15880f3be9e6d082f527c9ac70129b2054d733da9b754315361f", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "implementation_name == 'pypy' and sys_platform == 'darwin'",
        ]
        dependencies = [
            { name = "c" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/b-1.0.0.tar.gz", hash = "sha256:af6eb9a200314b36d0f49af106101b43445321bf148054ce024c92d79e93fa31", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-1.0.0-py3-none-any.whl", hash = "sha256:94d6ef21aaf5389c9ec11da5f313697b2f5a3b35f039735d2bcf0cfb7a6f88d1", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "implementation_name == 'cpython' and sys_platform == 'darwin'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/b-2.0.0.tar.gz", hash = "sha256:256a9af98c362451ef802d6462f06f8d4e26cc52543e8100cba63b584bae9a7c", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-2.0.0-py3-none-any.whl", hash = "sha256:04cc57f7563029528b6d23283933b244b6f52ba1543fad54687c586d6e639fc4", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "c"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/c-1.0.0.tar.gz", hash = "sha256:699a07ff61aab66fcba4883a94c6d2b61afb7797fa956ae36f2efdf30d9dfbc7", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/c-1.0.0-py3-none-any.whl", hash = "sha256:78c0da7c5681d751d38b2e60c78d1e29d6125d91e68e5aeb22372fa66527ff95", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "a", version = "1.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "sys_platform == 'darwin'" },
            { name = "a", version = "2.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "sys_platform == 'linux'" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "a", marker = "sys_platform == 'darwin'", specifier = "<2" },
            { name = "a", marker = "sys_platform == 'linux'", specifier = ">=2" },
        ]
        "#
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
///
/// ```text
/// fork-marker-inherit-combined-disallowed
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   ├── requires a>=2 ; sys_platform == 'linux'
/// │   │   └── satisfied by a-2.0.0
/// │   └── requires a<2 ; sys_platform == 'darwin'
/// │       └── satisfied by a-1.0.0
/// ├── a
/// │   ├── a-1.0.0
/// │   │   ├── requires b>=2 ; implementation_name == 'cpython'
/// │   │   │   └── satisfied by b-2.0.0
/// │   │   └── requires b<2 ; implementation_name == 'pypy'
/// │   │       └── satisfied by b-1.0.0
/// │   └── a-2.0.0
/// ├── b
/// │   ├── b-1.0.0
/// │   │   └── requires c ; implementation_name == 'cpython' or sys_platform == 'linux'
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

    let filters = context.filters();

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    "
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"
        resolution-markers = [
            "implementation_name == 'pypy' and sys_platform == 'darwin'",
            "implementation_name == 'cpython' and sys_platform == 'darwin'",
            "implementation_name != 'cpython' and implementation_name != 'pypy' and sys_platform == 'darwin'",
            "sys_platform == 'linux'",
            "sys_platform != 'darwin' and sys_platform != 'linux'",
        ]

        [[package]]
        name = "a"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "implementation_name == 'pypy' and sys_platform == 'darwin'",
            "implementation_name == 'cpython' and sys_platform == 'darwin'",
            "implementation_name != 'cpython' and implementation_name != 'pypy' and sys_platform == 'darwin'",
        ]
        dependencies = [
            { name = "b", version = "1.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "implementation_name == 'pypy'" },
            { name = "b", version = "2.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "implementation_name == 'cpython'" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:23d75e1acf1aaf735e83615f8baba2fa0d0e5f9b885706cfd017b9b72301cdab", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-any.whl", hash = "sha256:ebe588eab684413e5969ec398e03f7386a8106c5c88a601dacb2781fe2d0c819", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "a"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-2.0.0.tar.gz", hash = "sha256:9610291c2bd57390019f58ca72d0dd4584bb9e7073fa347633ed8bc7267fccfe", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-2.0.0-py3-none-any.whl", hash = "sha256:833374310e0a15880f3be9e6d082f527c9ac70129b2054d733da9b754315361f", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "implementation_name == 'pypy' and sys_platform == 'darwin'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/b-1.0.0.tar.gz", hash = "sha256:c20e9062f4cf30f8e581130ae2e18959f0f294246eb3bdd9f25053bd72a74267", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-1.0.0-py3-none-any.whl", hash = "sha256:b9dea47846d57e4a52afe31d36ad8fc1e7c01505c768be8855222320a9028f3c", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "implementation_name == 'cpython' and sys_platform == 'darwin'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/b-2.0.0.tar.gz", hash = "sha256:256a9af98c362451ef802d6462f06f8d4e26cc52543e8100cba63b584bae9a7c", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-2.0.0-py3-none-any.whl", hash = "sha256:04cc57f7563029528b6d23283933b244b6f52ba1543fad54687c586d6e639fc4", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "a", version = "1.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "sys_platform == 'darwin'" },
            { name = "a", version = "2.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "sys_platform == 'linux'" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "a", marker = "sys_platform == 'darwin'", specifier = "<2" },
            { name = "a", marker = "sys_platform == 'linux'", specifier = ">=2" },
        ]
        "#
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
///
/// ```text
/// fork-marker-inherit-combined
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   ├── requires a>=2 ; sys_platform == 'linux'
/// │   │   └── satisfied by a-2.0.0
/// │   └── requires a<2 ; sys_platform == 'darwin'
/// │       └── satisfied by a-1.0.0
/// ├── a
/// │   ├── a-1.0.0
/// │   │   ├── requires b>=2 ; implementation_name == 'cpython'
/// │   │   │   └── satisfied by b-2.0.0
/// │   │   └── requires b<2 ; implementation_name == 'pypy'
/// │   │       └── satisfied by b-1.0.0
/// │   └── a-2.0.0
/// ├── b
/// │   ├── b-1.0.0
/// │   │   └── requires c ; sys_platform == 'linux'
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

    let filters = context.filters();

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    "
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"
        resolution-markers = [
            "implementation_name == 'pypy' and sys_platform == 'darwin'",
            "implementation_name == 'cpython' and sys_platform == 'darwin'",
            "implementation_name != 'cpython' and implementation_name != 'pypy' and sys_platform == 'darwin'",
            "sys_platform == 'linux'",
            "sys_platform != 'darwin' and sys_platform != 'linux'",
        ]

        [[package]]
        name = "a"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "implementation_name == 'pypy' and sys_platform == 'darwin'",
            "implementation_name == 'cpython' and sys_platform == 'darwin'",
            "implementation_name != 'cpython' and implementation_name != 'pypy' and sys_platform == 'darwin'",
        ]
        dependencies = [
            { name = "b", version = "1.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "implementation_name == 'pypy'" },
            { name = "b", version = "2.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "implementation_name == 'cpython'" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:23d75e1acf1aaf735e83615f8baba2fa0d0e5f9b885706cfd017b9b72301cdab", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-any.whl", hash = "sha256:ebe588eab684413e5969ec398e03f7386a8106c5c88a601dacb2781fe2d0c819", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "a"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-2.0.0.tar.gz", hash = "sha256:9610291c2bd57390019f58ca72d0dd4584bb9e7073fa347633ed8bc7267fccfe", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-2.0.0-py3-none-any.whl", hash = "sha256:833374310e0a15880f3be9e6d082f527c9ac70129b2054d733da9b754315361f", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "implementation_name == 'pypy' and sys_platform == 'darwin'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/b-1.0.0.tar.gz", hash = "sha256:de35bb11b581875ed4be3c930bcc4d98e7e9ac34d5fe678f74a27c2136b633f9", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-1.0.0-py3-none-any.whl", hash = "sha256:42ee7845ea1960c41a676a74c9add4f48915e78f031b0e0a7d3894a692b9b6dd", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "implementation_name == 'cpython' and sys_platform == 'darwin'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/b-2.0.0.tar.gz", hash = "sha256:256a9af98c362451ef802d6462f06f8d4e26cc52543e8100cba63b584bae9a7c", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-2.0.0-py3-none-any.whl", hash = "sha256:04cc57f7563029528b6d23283933b244b6f52ba1543fad54687c586d6e639fc4", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "a", version = "1.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "sys_platform == 'darwin'" },
            { name = "a", version = "2.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "sys_platform == 'linux'" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "a", marker = "sys_platform == 'darwin'", specifier = "<2" },
            { name = "a", marker = "sys_platform == 'linux'", specifier = ">=2" },
        ]
        "#
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
///
/// ```text
/// fork-marker-inherit-isolated
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   ├── requires a>=2 ; sys_platform == 'linux'
/// │   │   └── satisfied by a-2.0.0
/// │   └── requires a<2 ; sys_platform == 'darwin'
/// │       └── satisfied by a-1.0.0
/// ├── a
/// │   ├── a-1.0.0
/// │   │   └── requires b ; sys_platform == 'linux'
/// │   │       └── satisfied by b-1.0.0
/// │   └── a-2.0.0
/// │       └── requires b ; sys_platform == 'linux'
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

    let filters = context.filters();

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"
        resolution-markers = [
            "sys_platform == 'darwin'",
            "sys_platform == 'linux'",
            "sys_platform != 'darwin' and sys_platform != 'linux'",
        ]

        [[package]]
        name = "a"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'darwin'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:bdb790e6d65140f316bfb33a6bc9ab03732245e8b5c6fd8efbcff7744530795d", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-any.whl", hash = "sha256:0f04d2c483396a1af08a8baee4a39f08f4e4a1f9775c3b9bce5245852e864eba", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "a"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        dependencies = [
            { name = "b" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-2.0.0.tar.gz", hash = "sha256:90a1f56c11a242c7437e595d28b6903388568edcb2ab8111657cc36fb7297ece", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-2.0.0-py3-none-any.whl", hash = "sha256:f455166cf613308f098ecf7d0911ca68838ca203b05c62cdb54b3043ad27c2d0", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/b-1.0.0.tar.gz", hash = "sha256:9b42692ac74b4da0eed1ef248b9be6bb0557c49507ba3b38f862b191a06d959c", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-1.0.0-py3-none-any.whl", hash = "sha256:a4c65510001153cab97a29ff219ad86e0d4330653ca89d9d4c84187ccf14c621", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "a", version = "1.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "sys_platform == 'darwin'" },
            { name = "a", version = "2.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "sys_platform == 'linux'" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "a", marker = "sys_platform == 'darwin'", specifier = "<2" },
            { name = "a", marker = "sys_platform == 'linux'", specifier = ">=2" },
        ]
        "#
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
///
/// ```text
/// fork-marker-inherit-transitive
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   ├── requires a>=2 ; sys_platform == 'linux'
/// │   │   └── satisfied by a-2.0.0
/// │   └── requires a<2 ; sys_platform == 'darwin'
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
/// │       └── requires d ; sys_platform == 'linux'
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

    let filters = context.filters();

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    "
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"
        resolution-markers = [
            "sys_platform == 'darwin'",
            "sys_platform == 'linux'",
            "sys_platform != 'darwin' and sys_platform != 'linux'",
        ]

        [[package]]
        name = "a"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'darwin'",
        ]
        dependencies = [
            { name = "b" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:75c52500ad189dbf1bd52b7db63c3a480b381039c554aace83b348c36c39aa25", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-any.whl", hash = "sha256:a0f20c0172d171015f1637827325521d0feaa0b85c49058980f660080d96170c", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "a"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-2.0.0.tar.gz", hash = "sha256:9610291c2bd57390019f58ca72d0dd4584bb9e7073fa347633ed8bc7267fccfe", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-2.0.0-py3-none-any.whl", hash = "sha256:833374310e0a15880f3be9e6d082f527c9ac70129b2054d733da9b754315361f", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "c" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/b-1.0.0.tar.gz", hash = "sha256:3b8dcd0978bd51a2c96c4580a742545bfcf43ff64c664721b68cc96a71c489d4", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-1.0.0-py3-none-any.whl", hash = "sha256:b1299c4860508b15790e950862174413d29090c6e08d069fe70297a6d4db5ee0", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "c"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/c-1.0.0.tar.gz", hash = "sha256:97fd5ea7c4a6535adc8b5d3eccdaa599d33edd9e4eccd922d6684b71171c829a", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/c-1.0.0-py3-none-any.whl", hash = "sha256:a154f7c821c8a9448936b30bcb743e04fb4a187be48bc502e8c465f83a839a0e", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "a", version = "1.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "sys_platform == 'darwin'" },
            { name = "a", version = "2.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "sys_platform == 'linux'" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "a", marker = "sys_platform == 'darwin'", specifier = "<2" },
            { name = "a", marker = "sys_platform == 'linux'", specifier = ">=2" },
        ]
        "#
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
///
/// ```text
/// fork-marker-inherit
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   ├── requires a>=2 ; sys_platform == 'linux'
/// │   │   └── satisfied by a-2.0.0
/// │   └── requires a<2 ; sys_platform == 'darwin'
/// │       └── satisfied by a-1.0.0
/// ├── a
/// │   ├── a-1.0.0
/// │   │   └── requires b ; sys_platform == 'linux'
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

    let filters = context.filters();

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"
        resolution-markers = [
            "sys_platform == 'darwin'",
            "sys_platform == 'linux'",
            "sys_platform != 'darwin' and sys_platform != 'linux'",
        ]

        [[package]]
        name = "a"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'darwin'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:bdb790e6d65140f316bfb33a6bc9ab03732245e8b5c6fd8efbcff7744530795d", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-any.whl", hash = "sha256:0f04d2c483396a1af08a8baee4a39f08f4e4a1f9775c3b9bce5245852e864eba", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "a"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-2.0.0.tar.gz", hash = "sha256:9610291c2bd57390019f58ca72d0dd4584bb9e7073fa347633ed8bc7267fccfe", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-2.0.0-py3-none-any.whl", hash = "sha256:833374310e0a15880f3be9e6d082f527c9ac70129b2054d733da9b754315361f", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "a", version = "1.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "sys_platform == 'darwin'" },
            { name = "a", version = "2.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "sys_platform == 'linux'" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "a", marker = "sys_platform == 'darwin'", specifier = "<2" },
            { name = "a", marker = "sys_platform == 'linux'", specifier = ">=2" },
        ]
        "#
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
///
/// ```text
/// fork-marker-limited-inherit
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   ├── requires a>=2 ; sys_platform == 'linux'
/// │   │   └── satisfied by a-2.0.0
/// │   ├── requires a<2 ; sys_platform == 'darwin'
/// │   │   └── satisfied by a-1.0.0
/// │   └── requires b
/// │       └── satisfied by b-1.0.0
/// ├── a
/// │   ├── a-1.0.0
/// │   │   └── requires c ; sys_platform == 'linux'
/// │   │       └── satisfied by c-1.0.0
/// │   └── a-2.0.0
/// ├── b
/// │   └── b-1.0.0
/// │       └── requires c ; sys_platform == 'linux'
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

    let filters = context.filters();

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    "
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"
        resolution-markers = [
            "sys_platform == 'darwin'",
            "sys_platform == 'linux'",
            "sys_platform != 'darwin' and sys_platform != 'linux'",
        ]

        [[package]]
        name = "a"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'darwin'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:4afce24fb7b6b495a2e3521c84d8703e9e1e31faf88f086a6516db3fbc87f7cc", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-any.whl", hash = "sha256:83b170dfb388aa657648396e796df2890a54c7125464b7087714da4ee7aaab3b", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "a"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-2.0.0.tar.gz", hash = "sha256:9610291c2bd57390019f58ca72d0dd4584bb9e7073fa347633ed8bc7267fccfe", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-2.0.0-py3-none-any.whl", hash = "sha256:833374310e0a15880f3be9e6d082f527c9ac70129b2054d733da9b754315361f", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "c", marker = "sys_platform == 'linux'" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/b-1.0.0.tar.gz", hash = "sha256:de35bb11b581875ed4be3c930bcc4d98e7e9ac34d5fe678f74a27c2136b633f9", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-1.0.0-py3-none-any.whl", hash = "sha256:42ee7845ea1960c41a676a74c9add4f48915e78f031b0e0a7d3894a692b9b6dd", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "c"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/c-1.0.0.tar.gz", hash = "sha256:699a07ff61aab66fcba4883a94c6d2b61afb7797fa956ae36f2efdf30d9dfbc7", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/c-1.0.0-py3-none-any.whl", hash = "sha256:78c0da7c5681d751d38b2e60c78d1e29d6125d91e68e5aeb22372fa66527ff95", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "a", version = "1.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "sys_platform == 'darwin'" },
            { name = "a", version = "2.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "sys_platform == 'linux'" },
            { name = "b" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "a", marker = "sys_platform == 'darwin'", specifier = "<2" },
            { name = "a", marker = "sys_platform == 'linux'", specifier = ">=2" },
            { name = "b" },
        ]
        "#
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
///
/// ```text
/// fork-marker-selection
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   ├── requires a
/// │   │   ├── satisfied by a-0.1.0
/// │   │   └── satisfied by a-0.2.0
/// │   ├── requires b>=2 ; sys_platform == 'linux'
/// │   │   └── satisfied by b-2.0.0
/// │   └── requires b<2 ; sys_platform == 'darwin'
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

    let filters = context.filters();

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"
        resolution-markers = [
            "sys_platform == 'darwin'",
            "sys_platform == 'linux'",
            "sys_platform != 'darwin' and sys_platform != 'linux'",
        ]

        [[package]]
        name = "a"
        version = "0.1.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/a-0.1.0.tar.gz", hash = "sha256:758dc8fff4646aa2c7f2ba2f32bdad3004625ce13ea474f163fe60bcb4d1d7d2", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-0.1.0-py3-none-any.whl", hash = "sha256:40f95c8868f537e1a289e86f8d75e208fa22d1f3b46c42834f526e89630f77c0", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'darwin'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/b-1.0.0.tar.gz", hash = "sha256:9b42692ac74b4da0eed1ef248b9be6bb0557c49507ba3b38f862b191a06d959c", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-1.0.0-py3-none-any.whl", hash = "sha256:a4c65510001153cab97a29ff219ad86e0d4330653ca89d9d4c84187ccf14c621", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/b-2.0.0.tar.gz", hash = "sha256:256a9af98c362451ef802d6462f06f8d4e26cc52543e8100cba63b584bae9a7c", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-2.0.0-py3-none-any.whl", hash = "sha256:04cc57f7563029528b6d23283933b244b6f52ba1543fad54687c586d6e639fc4", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "a" },
            { name = "b", version = "1.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "sys_platform == 'darwin'" },
            { name = "b", version = "2.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "sys_platform == 'linux'" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "a" },
            { name = "b", marker = "sys_platform == 'darwin'", specifier = "<2" },
            { name = "b", marker = "sys_platform == 'linux'", specifier = ">=2" },
        ]
        "#
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

///
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
/// │   ├── requires b>=2.8 ; sys_platform == 'linux'
/// │   │   └── satisfied by b-2.8
/// │   └── requires b<2.8 ; sys_platform == 'darwin'
/// │       └── satisfied by b-2.7
/// ├── a
/// │   ├── a-1.3.1
/// │   │   └── requires c ; implementation_name == 'iron'
/// │   │       └── satisfied by c-1.10
/// │   ├── a-2.0.0
/// │   │   ├── requires b>=2.8
/// │   │   │   └── satisfied by b-2.8
/// │   │   └── requires c ; implementation_name == 'cpython'
/// │   │       └── satisfied by c-1.10
/// │   ├── a-3.1.0
/// │   │   ├── requires b>=2.8
/// │   │   │   └── satisfied by b-2.8
/// │   │   └── requires c ; implementation_name == 'pypy'
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

    let filters = context.filters();

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    "
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"
        resolution-markers = [
            "sys_platform == 'darwin'",
            "sys_platform == 'linux'",
            "sys_platform != 'darwin' and sys_platform != 'linux'",
        ]

        [[package]]
        name = "a"
        version = "1.3.1"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "c", marker = "implementation_name == 'iron'" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-1.3.1.tar.gz", hash = "sha256:b46ebdc4ecc8c6670e8da12889df8c1bd286cbc8eb94f69e4626670e17b760c8", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.3.1-py3-none-any.whl", hash = "sha256:7720c67a8765ed540f4bdac4f8653210787d1ae518eb20ac541344ea8002f81c", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "2.7"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'darwin'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/b-2.7.tar.gz", hash = "sha256:c3e58feccc8d0cb3b8654491f51b4d53bf75edb0c8c5f3fd039570609acc3957", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-2.7-py3-none-any.whl", hash = "sha256:973b02bdc3039f4fa7347e091da60de892a0a18b26effd20a1e17fdcf24a6dcd", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "2.8"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/b-2.8.tar.gz", hash = "sha256:1f6eb422782a5730a466c3a9e0c149653751c64f6c4cdfb44a860d8d4eda2d24", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-2.8-py3-none-any.whl", hash = "sha256:ce8467e32ba82112ebbfbdb23ffd9b537ce7502f09ab70cf65716a1a89eb1683", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "c"
        version = "1.10"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/c-1.10.tar.gz", hash = "sha256:776b6806df1500e84d6b312aaf8d036a9d0d2ed4eb05a5ba52d6420311afd024", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/c-1.10-py3-none-any.whl", hash = "sha256:c830c4360164be9ea0027abb265a7f4f7894d8ac454942d98c09464d21a99b96", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "a" },
            { name = "b", version = "2.7", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "sys_platform == 'darwin'" },
            { name = "b", version = "2.8", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "sys_platform == 'linux'" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "a" },
            { name = "b", marker = "sys_platform == 'darwin'", specifier = "<2.8" },
            { name = "b", marker = "sys_platform == 'linux'", specifier = ">=2.8" },
        ]
        "#
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
/// │       └── requires c>=2.0.0 ; sys_platform == 'linux'
/// │           └── satisfied by c-2.0.0
/// ├── b
/// │   └── b-1.0.0
/// │       └── requires c>=2.0.0 ; sys_platform == 'darwin'
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

    let filters = context.filters();

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [[package]]
        name = "a"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "c", marker = "sys_platform == 'linux'" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:cd24667d7a4725e13a59e180b76b7e932a074cc7a8a20a18d353db979f4b6707", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-any.whl", hash = "sha256:6de66136e60c4e5c1832fdd217af472ad50d9ebd177f4014b9b3f50904f80f5e", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "c", marker = "sys_platform == 'darwin'" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/b-1.0.0.tar.gz", hash = "sha256:d6993f77c784de42e150f111902a2c88a867c555077dacaa6c3a1b71398784a4", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-1.0.0-py3-none-any.whl", hash = "sha256:8cb0c9eaa95e2cf767bf5e6caf4fecfa1b2b9fc09be53c5768372704574ac244", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "c"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/c-2.0.0.tar.gz", hash = "sha256:98b5a57ae857516af05cd6bc5c3f74d31a78cd6559594a51b00b45c4e3891905", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/c-2.0.0-py3-none-any.whl", hash = "sha256:4a585f74490e3c09faafdb7df1ebb51d5e41c67b82ef08b5b5fd2f4c251b4b23", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "a" },
            { name = "b" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "a", specifier = "==1.0.0" },
            { name = "b", specifier = "==1.0.0" },
        ]
        "#
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
///
/// ```text
/// fork-non-local-fork-marker-direct
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   ├── requires a==1.0.0 ; sys_platform == 'linux'
/// │   │   └── satisfied by a-1.0.0
/// │   └── requires b==1.0.0 ; sys_platform == 'darwin'
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

    let filters = context.filters();

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because a==1.0.0 depends on c<2.0.0 and b==1.0.0 depends on c>=2.0.0, we can conclude that b==1.0.0 and a{sys_platform == 'linux'}==1.0.0 are incompatible.
          And because your project depends on a{sys_platform == 'linux'}==1.0.0 and b{sys_platform == 'darwin'}==1.0.0, we can conclude that your project's requirements are unsatisfiable.
    "
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
/// │       └── requires c<2.0.0 ; sys_platform == 'linux'
/// │           └── satisfied by c-1.0.0
/// ├── b
/// │   └── b-1.0.0
/// │       └── requires c>=2.0.0 ; sys_platform == 'darwin'
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

    let filters = context.filters();

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because a==1.0.0 depends on c{sys_platform == 'linux'}<2.0.0 and b==1.0.0 depends on c{sys_platform == 'darwin'}>=2.0.0, we can conclude that a==1.0.0 and b==1.0.0 are incompatible.
          And because your project depends on a==1.0.0 and b==1.0.0, we can conclude that your project's requirements are unsatisfiable.
    "
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
///
/// ```text
/// fork-overlapping-markers-basic
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   ├── requires a>=1.0.0 ; python_full_version < '3.13'
/// │   │   ├── satisfied by a-1.0.0
/// │   │   ├── satisfied by a-1.1.0
/// │   │   └── satisfied by a-1.2.0
/// │   ├── requires a>=1.1.0 ; python_full_version >= '3.13'
/// │   │   ├── satisfied by a-1.1.0
/// │   │   └── satisfied by a-1.2.0
/// │   └── requires a>=1.2.0 ; python_full_version >= '3.14'
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
          '''a>=1.0.0 ; python_full_version < '3.13'''',
          '''a>=1.1.0 ; python_full_version >= '3.13'''',
          '''a>=1.2.0 ; python_full_version >= '3.14'''',
        ]
        requires-python = ">=3.12"
        "###,
    )?;

    let filters = context.filters();

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"
        resolution-markers = [
            "python_full_version >= '3.14'",
            "python_full_version == '3.13.*'",
            "python_full_version < '3.13'",
        ]

        [[package]]
        name = "a"
        version = "1.2.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/a-1.2.0.tar.gz", hash = "sha256:2e50354becbab0cc152f51e0ded5bdf4d7487237d8c1c825a151286117287c62", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.2.0-py3-none-any.whl", hash = "sha256:ac8736ef11e0594522369998752b2780be46e71e034c07d700c7bd0f2cea9863", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "a" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "a", marker = "python_full_version < '3.13'", specifier = ">=1.0.0" },
            { name = "a", marker = "python_full_version >= '3.13'", specifier = ">=1.1.0" },
            { name = "a", marker = "python_full_version >= '3.14'", specifier = ">=1.2.0" },
        ]
        "#
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
///
/// ```text
/// preferences-dependent-forking-bistable
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   └── requires cleaver
/// │       ├── satisfied by cleaver-1.0.0
/// │       └── satisfied by cleaver-2.0.0
/// ├── cleaver
/// │   ├── cleaver-1.0.0
/// │   │   ├── requires fork-if-not-forked!=2 ; sys_platform == 'linux'
/// │   │   │   ├── satisfied by fork-if-not-forked-1.0.0
/// │   │   │   └── satisfied by fork-if-not-forked-3.0.0
/// │   │   ├── requires fork-if-not-forked-proxy ; sys_platform != 'linux'
/// │   │   │   └── satisfied by fork-if-not-forked-proxy-1.0.0
/// │   │   ├── requires reject-cleaver1==1 ; sys_platform == 'linux'
/// │   │   │   └── satisfied by reject-cleaver1-1.0.0
/// │   │   └── requires reject-cleaver1-proxy
/// │   │       └── satisfied by reject-cleaver1-proxy-1.0.0
/// │   └── cleaver-2.0.0
/// │       ├── requires fork-sys-platform==1 ; sys_platform == 'linux'
/// │       │   └── satisfied by fork-sys-platform-1.0.0
/// │       ├── requires fork-sys-platform==2 ; sys_platform != 'linux'
/// │       │   └── satisfied by fork-sys-platform-2.0.0
/// │       ├── requires reject-cleaver2==1 ; os_name == 'posix'
/// │       │   └── satisfied by reject-cleaver2-1.0.0
/// │       └── requires reject-cleaver2-proxy
/// │           └── satisfied by reject-cleaver2-proxy-1.0.0
/// ├── fork-if-not-forked
/// │   ├── fork-if-not-forked-1.0.0
/// │   │   ├── requires fork-os-name==1 ; os_name == 'posix'
/// │   │   │   └── satisfied by fork-os-name-1.0.0
/// │   │   ├── requires fork-os-name==2 ; os_name != 'posix'
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
/// │       └── requires reject-cleaver1==2 ; sys_platform != 'linux'
/// │           └── satisfied by reject-cleaver1-2.0.0
/// ├── reject-cleaver2
/// │   ├── reject-cleaver2-1.0.0
/// │   └── reject-cleaver2-2.0.0
/// └── reject-cleaver2-proxy
///     └── reject-cleaver2-proxy-1.0.0
///         └── requires reject-cleaver2==2 ; os_name != 'posix'
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

    let filters = context.filters();

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 8 packages in [TIME]
    "
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"
        resolution-markers = [
            "sys_platform == 'linux'",
            "sys_platform != 'linux'",
        ]

        [[package]]
        name = "cleaver"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "fork-if-not-forked", version = "3.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "sys_platform == 'linux'" },
            { name = "fork-if-not-forked-proxy", marker = "sys_platform != 'linux'" },
            { name = "reject-cleaver1", version = "1.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "sys_platform == 'linux'" },
            { name = "reject-cleaver1-proxy" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/cleaver-1.0.0.tar.gz", hash = "sha256:e0bb339ac91a41ac2ce20db4866abf934c1be9c1a0b1f83efd701f9cf0a3da3c", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/cleaver-1.0.0-py3-none-any.whl", hash = "sha256:c9c97d652936b7293b36e54194d448350852ffa6fbfad051ba0671db46803d7b", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "fork-if-not-forked"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform != 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/fork_if_not_forked-2.0.0.tar.gz", hash = "sha256:53bb0ea79f0eb0fc38598b1d6bf4f8e15fc6d61342570db99e6756eb01823223", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/fork_if_not_forked-2.0.0-py3-none-any.whl", hash = "sha256:0604f383a6d7cf8fd69c252c85bdb34967743de531b2b0f4084378f00f5b4ccf", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "fork-if-not-forked"
        version = "3.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/fork_if_not_forked-3.0.0.tar.gz", hash = "sha256:15eca5a5864638d4b9a6343b844eb13fec827ad5cd6412df7399e80e60028075", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/fork_if_not_forked-3.0.0-py3-none-any.whl", hash = "sha256:dcfb9267c7ae0868f4f40e29574665b22e336e666c89a542a074355f7bac643e", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "fork-if-not-forked-proxy"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "fork-if-not-forked", version = "2.0.0", source = { registry = "http://[LOCALHOST]/simple/" } },
        ]
        sdist = { url = "http://[LOCALHOST]/files/fork_if_not_forked_proxy-1.0.0.tar.gz", hash = "sha256:cc3c846677ae440eb6f6460e7d9d67b68fdacb21950da4123c604d440f83bdc2", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/fork_if_not_forked_proxy-1.0.0-py3-none-any.whl", hash = "sha256:704030c720c21eee6ad9453ccd4d44d983182a73c057ed89325201c5dcead7fe", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "cleaver" },
        ]

        [package.metadata]
        requires-dist = [{ name = "cleaver" }]

        [[package]]
        name = "reject-cleaver1"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/reject_cleaver1-1.0.0.tar.gz", hash = "sha256:d7897b77030f920c3cda8ff08cc1949d73c24cb479f4dfdaf406c4f9ae9b2f44", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/reject_cleaver1-1.0.0-py3-none-any.whl", hash = "sha256:b3f3e0d98b07e6ec99719106244c07fa85658f0be9aa20f28c957aa8efcbeef8", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "reject-cleaver1"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform != 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/reject_cleaver1-2.0.0.tar.gz", hash = "sha256:4d82244f63049cc6441cbb1e570469e2e997edf28f132e30b1bf38f7ca037581", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/reject_cleaver1-2.0.0-py3-none-any.whl", hash = "sha256:db12d456e29ce239bbfed102b8b613a088adde579403d11ae91b15a08011efd2", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "reject-cleaver1-proxy"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "reject-cleaver1", version = "2.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "sys_platform != 'linux'" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/reject_cleaver1_proxy-1.0.0.tar.gz", hash = "sha256:3ee85cdcbf1fccefe03ff06787c7b38733e194c769deaf6686cc6190cd7a1cfe", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/reject_cleaver1_proxy-1.0.0-py3-none-any.whl", hash = "sha256:8032f56f5eaddb524e079a75ab95b25220f916ac692c377a6977b58e61a3508d", upload-time = "2024-03-24T00:00:00Z" },
        ]
        "#
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
/// │   │   ├── satisfied by cleaver-1.0.0
/// │   │   └── satisfied by cleaver-2.0.0
/// │   └── requires foo
/// │       ├── satisfied by foo-1.0.0
/// │       └── satisfied by foo-2.0.0
/// ├── bar
/// │   ├── bar-1.0.0
/// │   └── bar-2.0.0
/// ├── cleaver
/// │   ├── cleaver-1.0.0
/// │   │   ├── requires bar==1 ; sys_platform != 'linux'
/// │   │   │   └── satisfied by bar-1.0.0
/// │   │   └── requires foo==1 ; sys_platform == 'linux'
/// │   │       └── satisfied by foo-1.0.0
/// │   └── cleaver-2.0.0
/// │       ├── requires reject-cleaver-2
/// │       │   └── satisfied by reject-cleaver-2-1.0.0
/// │       ├── requires unrelated-dep==1 ; sys_platform == 'linux'
/// │       │   └── satisfied by unrelated-dep-1.0.0
/// │       └── requires unrelated-dep==2 ; sys_platform != 'linux'
/// │           └── satisfied by unrelated-dep-2.0.0
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

    let filters = context.filters();

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    "
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
/// │   │   ├── satisfied by cleaver-1.0.0
/// │   │   └── satisfied by cleaver-2.0.0
/// │   └── requires foo
/// │       ├── satisfied by foo-1.0.0
/// │       └── satisfied by foo-2.0.0
/// ├── a
/// │   └── a-1.0.0
/// │       └── requires unrelated-dep3==1 ; os_name == 'posix'
/// │           └── satisfied by unrelated-dep3-1.0.0
/// ├── b
/// │   └── b-1.0.0
/// │       └── requires unrelated-dep3==2 ; os_name != 'posix'
/// │           └── satisfied by unrelated-dep3-2.0.0
/// ├── bar
/// │   ├── bar-1.0.0
/// │   │   ├── requires c!=3 ; sys_platform == 'linux'
/// │   │   │   ├── satisfied by c-1.0.0
/// │   │   │   └── satisfied by c-2.0.0
/// │   │   ├── requires d ; sys_platform != 'linux'
/// │   │   │   └── satisfied by d-1.0.0
/// │   │   └── requires reject-cleaver-1
/// │   │       └── satisfied by reject-cleaver-1-1.0.0
/// │   └── bar-2.0.0
/// ├── c
/// │   ├── c-1.0.0
/// │   │   ├── requires reject-cleaver-1
/// │   │   │   └── satisfied by reject-cleaver-1-1.0.0
/// │   │   ├── requires unrelated-dep2==1 ; os_name == 'posix'
/// │   │   │   └── satisfied by unrelated-dep2-1.0.0
/// │   │   └── requires unrelated-dep2==2 ; os_name != 'posix'
/// │   │       └── satisfied by unrelated-dep2-2.0.0
/// │   ├── c-2.0.0
/// │   └── c-3.0.0
/// ├── cleaver
/// │   ├── cleaver-1.0.0
/// │   │   ├── requires bar==1 ; sys_platform != 'linux'
/// │   │   │   └── satisfied by bar-1.0.0
/// │   │   └── requires foo==1 ; sys_platform == 'linux'
/// │   │       └── satisfied by foo-1.0.0
/// │   └── cleaver-2.0.0
/// │       ├── requires a
/// │       │   └── satisfied by a-1.0.0
/// │       ├── requires b
/// │       │   └── satisfied by b-1.0.0
/// │       ├── requires unrelated-dep==1 ; sys_platform == 'linux'
/// │       │   └── satisfied by unrelated-dep-1.0.0
/// │       └── requires unrelated-dep==2 ; sys_platform != 'linux'
/// │           └── satisfied by unrelated-dep-2.0.0
/// ├── d
/// │   └── d-1.0.0
/// │       └── requires c!=2
/// │           ├── satisfied by c-1.0.0
/// │           └── satisfied by c-3.0.0
/// ├── foo
/// │   ├── foo-1.0.0
/// │   │   ├── requires c!=3 ; sys_platform == 'linux'
/// │   │   │   ├── satisfied by c-1.0.0
/// │   │   │   └── satisfied by c-2.0.0
/// │   │   ├── requires c!=2 ; sys_platform != 'linux'
/// │   │   │   ├── satisfied by c-1.0.0
/// │   │   │   └── satisfied by c-3.0.0
/// │   │   └── requires reject-cleaver-1
/// │   │       └── satisfied by reject-cleaver-1-1.0.0
/// │   └── foo-2.0.0
/// ├── reject-cleaver-1
/// │   └── reject-cleaver-1-1.0.0
/// │       ├── requires unrelated-dep2==1 ; sys_platform == 'linux'
/// │       │   └── satisfied by unrelated-dep2-1.0.0
/// │       └── requires unrelated-dep2==2 ; sys_platform != 'linux'
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

    let filters = context.filters();

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 11 packages in [TIME]
    "
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"
        resolution-markers = [
            "sys_platform == 'linux'",
            "sys_platform != 'linux'",
        ]

        [[package]]
        name = "bar"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform != 'linux'",
        ]
        dependencies = [
            { name = "d" },
            { name = "reject-cleaver-1" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/bar-1.0.0.tar.gz", hash = "sha256:4e9b7cce3ef47387b3b7a08bd81e5bb0ac0a19a62e3a0abf64119c18014f917a", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/bar-1.0.0-py3-none-any.whl", hash = "sha256:3e2cba35e1d80892bb436521134f100c183b8f64677be0ec80ff6ee4c710aa43", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "bar"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/bar-2.0.0.tar.gz", hash = "sha256:29e7bc76f76b7e939dcf1f8fe28b077c4631c11ecea673beb86d297dacda11eb", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/bar-2.0.0-py3-none-any.whl", hash = "sha256:563b1af3238a4ad819f2b95b74f940319a2ef30ed7991a2416fa98aa115da87d", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "c"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/c-2.0.0.tar.gz", hash = "sha256:98b5a57ae857516af05cd6bc5c3f74d31a78cd6559594a51b00b45c4e3891905", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/c-2.0.0-py3-none-any.whl", hash = "sha256:4a585f74490e3c09faafdb7df1ebb51d5e41c67b82ef08b5b5fd2f4c251b4b23", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "c"
        version = "3.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform != 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/c-3.0.0.tar.gz", hash = "sha256:c01237d1b0816abee804906c0b118675aaa1aa1fdbb6e756c350ecc6ed500ebb", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/c-3.0.0-py3-none-any.whl", hash = "sha256:1e031928d6a855d65842904337ad7d984de1a64473b1166d548afbfe5a397341", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "cleaver"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "bar", version = "1.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "sys_platform != 'linux'" },
            { name = "foo", marker = "sys_platform == 'linux'" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/cleaver-1.0.0.tar.gz", hash = "sha256:e48e43a500c95e61d1f1e18d830ec1df0ed3065842738493669f850a2c3da9ad", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/cleaver-1.0.0-py3-none-any.whl", hash = "sha256:f49d93330cfe3f7096636c506aa522a497eae44d08b07f6276caf784ec87f65b", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "d"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "c", version = "3.0.0", source = { registry = "http://[LOCALHOST]/simple/" } },
        ]
        sdist = { url = "http://[LOCALHOST]/files/d-1.0.0.tar.gz", hash = "sha256:41ae55064b4daf92964c6428bb14b433ef5ac82b96ddb2868460ac862b3e80c9", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/d-1.0.0-py3-none-any.whl", hash = "sha256:890292cd440f4d868e46d5e883bd2eee4ced094d39e44a7cd88f7d9f5049f5ef", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "foo"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "c", version = "2.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "sys_platform == 'linux'" },
            { name = "c", version = "3.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "sys_platform != 'linux'" },
            { name = "reject-cleaver-1" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/foo-1.0.0.tar.gz", hash = "sha256:693cd5b07b84a596dc7595d47d3bedebd7540b5455eedb4fa7ddf4196bbcb205", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/foo-1.0.0-py3-none-any.whl", hash = "sha256:a6a8594d7d818843d31479b216b325e9140b6c5b180a55427b03fd85bbd261ad", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "bar", version = "1.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "sys_platform != 'linux'" },
            { name = "bar", version = "2.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "sys_platform == 'linux'" },
            { name = "cleaver" },
            { name = "foo" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "bar" },
            { name = "cleaver" },
            { name = "foo" },
        ]

        [[package]]
        name = "reject-cleaver-1"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "unrelated-dep2", version = "1.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "sys_platform == 'linux'" },
            { name = "unrelated-dep2", version = "2.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "sys_platform != 'linux'" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/reject_cleaver_1-1.0.0.tar.gz", hash = "sha256:c707cb6622bed86ca850cecb72a1e03e2dfef87b7f19018684f0530d5532cdb2", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/reject_cleaver_1-1.0.0-py3-none-any.whl", hash = "sha256:5b4d622444188c423a9476bb5c92358d7322f121d6e764c4b8d8bdc14e9f4017", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "unrelated-dep2"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/unrelated_dep2-1.0.0.tar.gz", hash = "sha256:6154dcfd6aa5dc62404702d8d66e908c7482a6107148323259be64e0268a1885", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/unrelated_dep2-1.0.0-py3-none-any.whl", hash = "sha256:a485b74955fe703c7c525c52ef72da556100f2ea4b49ca97e6fd6f183469fb43", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "unrelated-dep2"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform != 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/unrelated_dep2-2.0.0.tar.gz", hash = "sha256:b58f99b95e60c3b85f930cb1fae95ef3baba23e85eca0db721f82ad4e8c32cf6", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/unrelated_dep2-2.0.0-py3-none-any.whl", hash = "sha256:f666a4b92a10879856a6962ef582a2ab328edd174b8def895d859b10bfea49b6", upload-time = "2024-03-24T00:00:00Z" },
        ]
        "#
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
/// │   │   ├── satisfied by cleaver-1.0.0
/// │   │   └── satisfied by cleaver-2.0.0
/// │   └── requires foo
/// │       ├── satisfied by foo-1.0.0
/// │       └── satisfied by foo-2.0.0
/// ├── bar
/// │   ├── bar-1.0.0
/// │   └── bar-2.0.0
/// ├── cleaver
/// │   ├── cleaver-1.0.0
/// │   │   ├── requires bar==1 ; sys_platform != 'linux'
/// │   │   │   └── satisfied by bar-1.0.0
/// │   │   └── requires foo==1 ; sys_platform == 'linux'
/// │   │       └── satisfied by foo-1.0.0
/// │   └── cleaver-2.0.0
/// │       ├── requires reject-cleaver-2
/// │       │   └── satisfied by reject-cleaver-2-1.0.0
/// │       ├── requires unrelated-dep==1 ; sys_platform == 'linux'
/// │       │   └── satisfied by unrelated-dep-1.0.0
/// │       └── requires unrelated-dep==2 ; sys_platform != 'linux'
/// │           └── satisfied by unrelated-dep-2.0.0
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

    let filters = context.filters();

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    "
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"
        resolution-markers = [
            "sys_platform == 'linux'",
            "sys_platform != 'linux'",
        ]

        [[package]]
        name = "bar"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform != 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/bar-1.0.0.tar.gz", hash = "sha256:bb9cb9098cc77ebe1f2085af0859f2332ab631348e58c687fa344aea81eb4043", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/bar-1.0.0-py3-none-any.whl", hash = "sha256:2fbf0e0a7dd4f48a8b1c2148f73ba0c314699d0bfa0a8ec9b9bcb8105882e9fc", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "bar"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/bar-2.0.0.tar.gz", hash = "sha256:29e7bc76f76b7e939dcf1f8fe28b077c4631c11ecea673beb86d297dacda11eb", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/bar-2.0.0-py3-none-any.whl", hash = "sha256:563b1af3238a4ad819f2b95b74f940319a2ef30ed7991a2416fa98aa115da87d", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "cleaver"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "bar", version = "1.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "sys_platform != 'linux'" },
            { name = "foo", marker = "sys_platform == 'linux'" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/cleaver-1.0.0.tar.gz", hash = "sha256:e48e43a500c95e61d1f1e18d830ec1df0ed3065842738493669f850a2c3da9ad", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/cleaver-1.0.0-py3-none-any.whl", hash = "sha256:f49d93330cfe3f7096636c506aa522a497eae44d08b07f6276caf784ec87f65b", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "foo"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/foo-1.0.0.tar.gz", hash = "sha256:70bd56242b5a5c7f6c04694a8ed2aafdb036726de0fcca1dd1d2f1f467c71ee1", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/foo-1.0.0-py3-none-any.whl", hash = "sha256:df9a39d54a6d71872deb1537d7e331de0ede3b725d630a050fee3efd3fe5145b", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "bar", version = "1.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "sys_platform != 'linux'" },
            { name = "bar", version = "2.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "sys_platform == 'linux'" },
            { name = "cleaver" },
            { name = "foo" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "bar" },
            { name = "cleaver" },
            { name = "foo" },
        ]
        "#
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
///
/// ```text
/// fork-remaining-universe-partitioning
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   ├── requires a>=2 ; sys_platform == 'windows'
/// │   │   └── satisfied by a-2.0.0
/// │   └── requires a<2 ; sys_platform == 'illumos'
/// │       └── satisfied by a-1.0.0
/// ├── a
/// │   ├── a-1.0.0
/// │   │   ├── requires b>=2 ; os_name == 'linux'
/// │   │   │   └── satisfied by b-2.0.0
/// │   │   ├── requires b<2 ; os_name == 'darwin'
/// │   │   │   └── satisfied by b-1.0.0
/// │   │   └── requires z ; sys_platform == 'windows'
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

    let filters = context.filters();

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    "
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"
        resolution-markers = [
            "os_name == 'darwin' and sys_platform == 'illumos'",
            "os_name == 'linux' and sys_platform == 'illumos'",
            "os_name != 'darwin' and os_name != 'linux' and sys_platform == 'illumos'",
            "sys_platform == 'windows'",
            "sys_platform != 'illumos' and sys_platform != 'windows'",
        ]

        [[package]]
        name = "a"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "os_name == 'darwin' and sys_platform == 'illumos'",
            "os_name == 'linux' and sys_platform == 'illumos'",
            "os_name != 'darwin' and os_name != 'linux' and sys_platform == 'illumos'",
        ]
        dependencies = [
            { name = "b", version = "1.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "os_name == 'darwin'" },
            { name = "b", version = "2.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "os_name == 'linux'" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:217554d13af0a280cb3f5d95653c460bfdda5ea14c36a48e64fbaf39c7c6e16c", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-any.whl", hash = "sha256:de627f2f3dc58918c496b5b61036d5a8c19c769fa5c933889bd805366fa2de1d", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "a"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'windows'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-2.0.0.tar.gz", hash = "sha256:9610291c2bd57390019f58ca72d0dd4584bb9e7073fa347633ed8bc7267fccfe", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-2.0.0-py3-none-any.whl", hash = "sha256:833374310e0a15880f3be9e6d082f527c9ac70129b2054d733da9b754315361f", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "os_name == 'darwin' and sys_platform == 'illumos'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/b-1.0.0.tar.gz", hash = "sha256:9b42692ac74b4da0eed1ef248b9be6bb0557c49507ba3b38f862b191a06d959c", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-1.0.0-py3-none-any.whl", hash = "sha256:a4c65510001153cab97a29ff219ad86e0d4330653ca89d9d4c84187ccf14c621", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "os_name == 'linux' and sys_platform == 'illumos'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/b-2.0.0.tar.gz", hash = "sha256:256a9af98c362451ef802d6462f06f8d4e26cc52543e8100cba63b584bae9a7c", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-2.0.0-py3-none-any.whl", hash = "sha256:04cc57f7563029528b6d23283933b244b6f52ba1543fad54687c586d6e639fc4", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "a", version = "1.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "sys_platform == 'illumos'" },
            { name = "a", version = "2.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "sys_platform == 'windows'" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "a", marker = "sys_platform == 'illumos'", specifier = "<2" },
            { name = "a", marker = "sys_platform == 'windows'", specifier = ">=2" },
        ]
        "#
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
///
/// ```text
/// fork-requires-python-full-prerelease
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   └── requires a==1.0.0 ; python_full_version == '3.9'
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
          '''a==1.0.0 ; python_full_version == '3.9'''',
        ]
        requires-python = ">=3.10"
        "###,
    )?;

    let filters = context.filters();

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    "
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.10"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }

        [package.metadata]
        requires-dist = [{ name = "a", marker = "python_full_version == '3.9'", specifier = "==1.0.0" }]
        "#
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
///
/// ```text
/// fork-requires-python-full
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   └── requires a==1.0.0 ; python_full_version == '3.9'
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

    let filters = context.filters();

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    "
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.10"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }

        [package.metadata]
        requires-dist = [{ name = "a", marker = "python_full_version == '3.9'", specifier = "==1.0.0" }]
        "#
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
///
/// ```text
/// fork-requires-python-patch-overlap
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   └── requires a==1.0.0 ; python_full_version == '3.10.*'
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
          '''a==1.0.0 ; python_full_version == '3.10.*'''',
        ]
        requires-python = ">=3.10.1"
        "###,
    )?;

    let filters = context.filters();

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.10.1"

        [[package]]
        name = "a"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:f49e4cc76cd214a2a67efbe254cd317fa72d09cc98cd3d06537083d987284267", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-any.whl", hash = "sha256:277ee4f1bb98d9591ba3a3a28364d1f00a54384c08d8f32100afc01ae76491a5", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "a", marker = "python_full_version < '3.11'" },
        ]

        [package.metadata]
        requires-dist = [{ name = "a", marker = "python_full_version == '3.10.*'", specifier = "==1.0.0" }]
        "#
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
///
/// ```text
/// fork-requires-python
/// ├── environment
/// │   └── python3.12
/// ├── root
/// │   └── requires a==1.0.0 ; python_full_version == '3.9.*'
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
          '''a==1.0.0 ; python_full_version == '3.9.*'''',
        ]
        requires-python = ">=3.10"
        "###,
    )?;

    let filters = context.filters();

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    "
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.10"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }

        [package.metadata]
        requires-dist = [{ name = "a", marker = "python_full_version == '3.9.*'", specifier = "==1.0.0" }]
        "#
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

    let filters = context.filters();

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.10"

        [[package]]
        name = "a"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:f49e4cc76cd214a2a67efbe254cd317fa72d09cc98cd3d06537083d987284267", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-cp310-cp310-any.whl", hash = "sha256:34c6734e2427cc772605ac6710cf4e95c3556fd191b308b65c4d5f056fc95530", upload-time = "2024-03-24T00:00:00Z" },
            { url = "http://[LOCALHOST]/files/a-1.0.0-cp311-cp311-any.whl", hash = "sha256:a3aca28e2bd4f75c53a651a42fd54661d8aec43c1f9a4c703b3a301ad9deb5e2", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "a" },
        ]

        [package.metadata]
        requires-dist = [{ name = "a", specifier = "==1.0.0" }]
        "#
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
/// │   └── requires a==1.0.0 ; sys_platform == 'win32'
/// │       └── satisfied by a-1.0.0
/// ├── a
/// │   └── a-1.0.0
/// │       └── requires b==1.0.0 ; sys_platform == 'linux'
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
          '''a==1.0.0 ; sys_platform == 'win32'''',
        ]
        requires-python = ">=3.12"
        "###,
    )?;

    let filters = context.filters();

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [[package]]
        name = "a"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:9ea08e4e8cc22657585ae7aab665a536fd45dab0f612b3f7cf516aee045058ca", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-any.whl", hash = "sha256:9c02f18ae50a1da3421fa31399fe1497172375a7a6c80e8d1485ed12c3e64ee3", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "a", marker = "sys_platform == 'win32'" },
        ]

        [package.metadata]
        requires-dist = [{ name = "a", marker = "sys_platform == 'win32'", specifier = "==1.0.0" }]
        "#
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
/// │   ├── requires a==1.0.0 ; sys_platform == 'win32'
/// │   │   └── satisfied by a-1.0.0
/// │   ├── requires b==1.0.0 ; sys_platform == 'linux'
/// │   │   └── satisfied by b-1.0.0
/// │   └── requires c==1.0.0 ; sys_platform == 'darwin'
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
          '''a==1.0.0 ; sys_platform == 'win32'''',
          '''b==1.0.0 ; sys_platform == 'linux'''',
          '''c==1.0.0 ; sys_platform == 'darwin'''',
        ]
        requires-python = ">=3.12"
        "###,
    )?;

    let filters = context.filters();

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [[package]]
        name = "a"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:957f99ff1d65ce0d7883d50f4e67ed8d4b42e76d2c2b5e62384ff0ba538647b5", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-cp312-cp312-win_amd64.whl", hash = "sha256:559a9c629536d99c1213064c303dcc6ee8dd2917824c0a0f6549f94d0be02abd", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/b-1.0.0.tar.gz", hash = "sha256:9b42692ac74b4da0eed1ef248b9be6bb0557c49507ba3b38f862b191a06d959c", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-1.0.0-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:4d0fc532c5d6e2f11ca522dfd3684b50b132dd1db78fba3ad510023f4d5830b7", upload-time = "2024-03-24T00:00:00Z" },
            { url = "http://[LOCALHOST]/files/b-1.0.0-cp312-cp312-musllinux_1_1_armv7l.whl", hash = "sha256:8f328487f947ee5119d348f2c73ab9119dd29349b840f6f26e0d351245d6084b", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "c"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/c-1.0.0.tar.gz", hash = "sha256:699a07ff61aab66fcba4883a94c6d2b61afb7797fa956ae36f2efdf30d9dfbc7", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/c-1.0.0-cp312-cp312-macosx_14_0_x86_64.whl", hash = "sha256:a25d2ba9ff1417f63313691a2f686f30b617a8696329023e44b7f7fc96573598", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "a", marker = "sys_platform == 'win32'" },
            { name = "b", marker = "sys_platform == 'linux'" },
            { name = "c", marker = "sys_platform == 'darwin'" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "a", marker = "sys_platform == 'win32'", specifier = "==1.0.0" },
            { name = "b", marker = "sys_platform == 'linux'", specifier = "==1.0.0" },
            { name = "c", marker = "sys_platform == 'darwin'", specifier = "==1.0.0" },
        ]
        "#
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
/// │   ├── requires psycopg[binary] ; platform_python_implementation != 'PyPy'
/// │   │   ├── satisfied by psycopg-1.0.0
/// │   │   └── satisfied by psycopg-1.0.0[binary]
/// │   └── requires psycopg ; platform_python_implementation == 'PyPy'
/// │       ├── satisfied by psycopg-1.0.0
/// │       └── satisfied by psycopg-1.0.0[binary]
/// ├── psycopg
/// │   ├── psycopg-1.0.0
/// │   │   └── requires tzdata ; sys_platform == 'win32'
/// │   │       └── satisfied by tzdata-1.0.0
/// │   └── psycopg-1.0.0[binary]
/// │       └── requires psycopg-binary ; implementation_name != 'pypy'
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

    let filters = context.filters();

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"
        resolution-markers = [
            "platform_python_implementation != 'PyPy'",
            "platform_python_implementation == 'PyPy'",
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "psycopg" },
            { name = "psycopg", extra = ["binary"], marker = "platform_python_implementation != 'PyPy'" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "psycopg", marker = "platform_python_implementation == 'PyPy'" },
            { name = "psycopg", extras = ["binary"], marker = "platform_python_implementation != 'PyPy'" },
        ]

        [[package]]
        name = "psycopg"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "tzdata", marker = "sys_platform == 'win32'" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/psycopg-1.0.0.tar.gz", hash = "sha256:33854afc2bc33353fa645560cc0082cf085dc2a0b94afa97ca499b442f3b1a4e", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/psycopg-1.0.0-py3-none-any.whl", hash = "sha256:e91c6b77d7c6b4262b81b606577e444d27fcdaa22d6603742ced03b39492eda3", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [package.optional-dependencies]
        binary = [
            { name = "psycopg-binary", marker = "implementation_name != 'pypy'" },
        ]

        [[package]]
        name = "psycopg-binary"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/psycopg_binary-1.0.0.tar.gz", hash = "sha256:bc28ec69da2e999fb6475f8f55098375766a57922031775a597998ddc825581a", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/psycopg_binary-1.0.0-py3-none-any.whl", hash = "sha256:35474aba5923a77d2aac9ac1e92bb5e9dc0468b3b2611e120b3947ae956a1482", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "tzdata"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/tzdata-1.0.0.tar.gz", hash = "sha256:32da4d714b0879cde291f6a55811e4ddbfdfd93bd17c0514f2f792f6cad38e38", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/tzdata-1.0.0-py3-none-any.whl", hash = "sha256:9dc909bec179ffb4d319a54ab73cb61e769c912be44743ff4411c5191432573f", upload-time = "2024-03-24T00:00:00Z" },
        ]
        "#
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
/// │   ├── requires a==1 ; python_full_version >= '3.8'
/// │   │   └── satisfied by a-1.0.0
/// │   └── requires b ; python_full_version >= '3.9'
/// │       ├── satisfied by b-1.0.0
/// │       └── satisfied by b-2.0.0
/// ├── a
/// │   ├── a-1.0.0
/// │   │   └── requires b==1 ; python_full_version >= '3.10'
/// │   │       └── satisfied by b-1.0.0
/// │   └── a-2.0.0
/// │       └── requires b==1 ; python_full_version >= '3.10'
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
          '''a==1 ; python_full_version >= '3.8'''',
          '''b ; python_full_version >= '3.9'''',
        ]
        requires-python = ">=3.12"
        "###,
    )?;

    let filters = context.filters();

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [[package]]
        name = "a"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "b" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:ceae689363b89a7feb87b681fc4f66b85ea8227eb50ae60134f69f6af12f8b3e", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-any.whl", hash = "sha256:d3c47a26ec1e08259468942a50736244908a076baf0d5f931f62407814ed96e5", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/b-1.0.0.tar.gz", hash = "sha256:9b42692ac74b4da0eed1ef248b9be6bb0557c49507ba3b38f862b191a06d959c", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-1.0.0-py3-none-any.whl", hash = "sha256:a4c65510001153cab97a29ff219ad86e0d4330653ca89d9d4c84187ccf14c621", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "a" },
            { name = "b" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "a", marker = "python_full_version >= '3.8'", specifier = "==1" },
            { name = "b", marker = "python_full_version >= '3.9'" },
        ]
        "#
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
/// │   └── requires win-only ; sys_platform == 'win32'
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
          '''win-only ; sys_platform == 'win32'''',
        ]
        requires-python = ">=3.12"
        [tool.uv]
        required-environments = [
          '''sys_platform == 'linux'''',
          '''sys_platform == 'win32'''',
        ]
        "###,
    )?;

    let filters = context.filters();

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"
        required-markers = [
            "sys_platform == 'linux'",
            "sys_platform == 'win32'",
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "win-only", marker = "sys_platform == 'win32'" },
        ]

        [package.metadata]
        requires-dist = [{ name = "win-only", marker = "sys_platform == 'win32'" }]

        [[package]]
        name = "win-only"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        wheels = [
            { url = "http://[LOCALHOST]/files/win_only-1.0.0-cp312-abi3-win_amd64.whl", hash = "sha256:b53b82ec335953e0c0d5dc75c59ce658fd9b6c04330126109a7ba104e0d107e3", upload-time = "2024-03-24T00:00:00Z" },
        ]
        "#
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
/// │       ├── requires b ; platform_machine == 'x86_64'
/// │       │   └── satisfied by b-1.0.0
/// │       ├── requires c ; platform_machine == 'aarch64'
/// │       │   └── satisfied by c-1.0.0
/// │       └── requires d ; platform_machine == 'i686'
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

    let filters = context.filters();

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(server.index_url());
    uv_snapshot!(filters, cmd, @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    "
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [[package]]
        name = "a"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "b", marker = "platform_machine == 'x86_64'" },
            { name = "c", marker = "platform_machine == 'aarch64'" },
            { name = "d", marker = "platform_machine == 'i686'" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:d128ac9ff5b61e4db85dc86b943210cad23907d060f95a73914c7479acbbf9d1", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-any.whl", hash = "sha256:ccd718f14fb28f4e947c39f67a39897f08eaf09034d0641ffb6ce41e580dac1a", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-1.0.0-cp313-cp313-freebsd_13_aarch64.whl", hash = "sha256:f631c1c1a0ffc6b2ce37901dca7c5e468d80f9a5107f3e3ec2dd231424e51447", upload-time = "2024-03-24T00:00:00Z" },
            { url = "http://[LOCALHOST]/files/b-1.0.0-cp313-cp313-freebsd_13_x86_64.whl", hash = "sha256:c0a23d73fe066699bb1ed79150f23a989692c419edde2dc07b6f07ad7df04bb6", upload-time = "2024-03-24T00:00:00Z" },
            { url = "http://[LOCALHOST]/files/b-1.0.0-cp313-cp313-macosx_10_9_x86_64.whl", hash = "sha256:afc62d392afde77dc8cb1a76ca661ea28622a990e3655d96b101301526790393", upload-time = "2024-03-24T00:00:00Z" },
            { url = "http://[LOCALHOST]/files/b-1.0.0-cp313-cp313-manylinux2010_x86_64.whl", hash = "sha256:baa416bab7cc0d502bc96bc57672f2fa83dd56b7dc37b701126e16fcc8ce115a", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "c"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        wheels = [
            { url = "http://[LOCALHOST]/files/c-1.0.0-cp313-cp313-freebsd_13_aarch64.whl", hash = "sha256:e34952726fbb61bd86f63a767fa1fcc10a8e27737ef515b597617ee7fcb808ea", upload-time = "2024-03-24T00:00:00Z" },
            { url = "http://[LOCALHOST]/files/c-1.0.0-cp313-cp313-freebsd_13_x86_64.whl", hash = "sha256:c73f855a843c0842e67e01683f9b8e97391ec4de6f4e59e3bc43b7b2218f2fb9", upload-time = "2024-03-24T00:00:00Z" },
            { url = "http://[LOCALHOST]/files/c-1.0.0-cp313-cp313-macosx_10_9_arm64.whl", hash = "sha256:27e36c0dab216a5e265e247d2fb7cab31448beb97cefc22cce301885210b54df", upload-time = "2024-03-24T00:00:00Z" },
            { url = "http://[LOCALHOST]/files/c-1.0.0-cp313-cp313-manylinux2010_aarch64.whl", hash = "sha256:4d267230c94b48fb96c8eabadfd39eeb5efcb5d712f699feba087b1bb7ee18b5", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "d"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        wheels = [
            { url = "http://[LOCALHOST]/files/d-1.0.0-cp313-cp313-freebsd_13_aarch64.whl", hash = "sha256:faa00bef937a8020e48e398b106bf2085b327128af2e0a91a1596dfa07035b0a", upload-time = "2024-03-24T00:00:00Z" },
            { url = "http://[LOCALHOST]/files/d-1.0.0-cp313-cp313-freebsd_13_x86_64.whl", hash = "sha256:0535a2b64b540cf7ab4e124fa4673054113fd4d3957ae6b77b63833729e308c9", upload-time = "2024-03-24T00:00:00Z" },
            { url = "http://[LOCALHOST]/files/d-1.0.0-cp313-cp313-manylinux2010_i686.whl", hash = "sha256:66575b89a885afc891c6c84589a50b2f5112a0d1731c924df1fd376010e659e8", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "a" },
        ]

        [package.metadata]
        requires-dist = [{ name = "a" }]
        "#
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
