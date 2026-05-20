//! DO NOT EDIT
//!
//! Generated with `cargo dev generate-scenarios`
//! Scenarios from <test/scenarios>
//!
#![cfg(feature = "test-python")]
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
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:3d2b4c28a4e112f3a1cef1db4dc5efa33fcbbcc38bc11ccc80321097db86c097", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-any.whl", hash = "sha256:a81d806fcb056fb2ef0e1ebe929778d2d5895a2fbb2c34a7b04ddb1fe0bba1f4", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "2.0.9"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "a" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/b-2.0.9.tar.gz", hash = "sha256:f9d404650ef15d09f718b4f99d911962f653516974fa1e01cf8dba7afa695caa", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-2.0.9-py3-none-any.whl", hash = "sha256:6bed0883c7a7babef9fa2160a3f7ecb8b4ac1909f741f0655bdfd3cad536eed4", upload-time = "2024-03-24T00:00:00Z" },
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
        sdist = { url = "http://[LOCALHOST]/files/a-2.0.0.tar.gz", hash = "sha256:80ec95a66cff82a78a3333e3f5702e4254cf80533f21762933252eec58c9869a", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-2.0.0-py3-none-any.whl", hash = "sha256:274c04a6dc88ddc6c0781ec938dc00953b3ceab3950f97ed6da84e231765f555", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "b-inner" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/b-1.0.0.tar.gz", hash = "sha256:d002926c2038325d9c4287f87bc5c12d7336a32c5bbff9925ac474fa4341149d", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-1.0.0-py3-none-any.whl", hash = "sha256:303df711de5530c57eb9476592f5679c041bca426da9addef7f8385c68423d91", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b-inner"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "too-old" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/b_inner-1.0.0.tar.gz", hash = "sha256:73cb22e45889937a3a26ec58d668e92e488c133a6ecc4515f067532380980631", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b_inner-1.0.0-py3-none-any.whl", hash = "sha256:bc13f146ceaeccea086b31bf6a67c762dca03426730518d60bb2eb03f5c17a36", upload-time = "2024-03-24T00:00:00Z" },
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
        sdist = { url = "http://[LOCALHOST]/files/too_old-1.0.0.tar.gz", hash = "sha256:98267dfd9af634cfcbfa079c8a5cdcbb5169836904a5cb4441b71333467dc682", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/too_old-1.0.0-py3-none-any.whl", hash = "sha256:902360c3800ab509d8971eb023574a8782a3ca6d65fcb96c57871516150babce", upload-time = "2024-03-24T00:00:00Z" },
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
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:3d2b4c28a4e112f3a1cef1db4dc5efa33fcbbcc38bc11ccc80321097db86c097", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-any.whl", hash = "sha256:a81d806fcb056fb2ef0e1ebe929778d2d5895a2fbb2c34a7b04ddb1fe0bba1f4", upload-time = "2024-03-24T00:00:00Z" },
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
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:3d2b4c28a4e112f3a1cef1db4dc5efa33fcbbcc38bc11ccc80321097db86c097", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-any.whl", hash = "sha256:a81d806fcb056fb2ef0e1ebe929778d2d5895a2fbb2c34a7b04ddb1fe0bba1f4", upload-time = "2024-03-24T00:00:00Z" },
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
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:3d2b4c28a4e112f3a1cef1db4dc5efa33fcbbcc38bc11ccc80321097db86c097", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-any.whl", hash = "sha256:a81d806fcb056fb2ef0e1ebe929778d2d5895a2fbb2c34a7b04ddb1fe0bba1f4", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "a"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-2.0.0.tar.gz", hash = "sha256:80ec95a66cff82a78a3333e3f5702e4254cf80533f21762933252eec58c9869a", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-2.0.0-py3-none-any.whl", hash = "sha256:274c04a6dc88ddc6c0781ec938dc00953b3ceab3950f97ed6da84e231765f555", upload-time = "2024-03-24T00:00:00Z" },
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
        sdist = { url = "http://[LOCALHOST]/files/a-4.3.0.tar.gz", hash = "sha256:ae6dc9fc44095c1d8ec669ea9ce6623fb598874d6436e989ccad9fbb8ecf121f", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-4.3.0-py3-none-any.whl", hash = "sha256:5704939c06d6e96a7ae8d4b5df26340f96eb6b27c1afac268c66b6880ac34d75", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "a"
        version = "4.4.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-4.4.0.tar.gz", hash = "sha256:d531c73543a10f88aa8cb084ab856c7f369d442d09ad3ffdeb4a1771590b1d0f", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-4.4.0-py3-none-any.whl", hash = "sha256:82fed8f9850d6717ab8c53cbc20c8c38ae44ba16967809f51d0ebcff5f37f478", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "d", version = "1.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "sys_platform == 'linux'" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/b-1.0.0.tar.gz", hash = "sha256:e86cc7db081c964c8874d032149c4e84b3f23563c517beb7bedfa875a8f9f4ce", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-1.0.0-py3-none-any.whl", hash = "sha256:85296957a3e956dc1b36ea1ffcec3b44124a6eb2c8b03a6b70132a0519a5c88e", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "c"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "d", version = "2.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "sys_platform == 'darwin'" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/c-1.0.0.tar.gz", hash = "sha256:bc02a5bce314ce38dbd74b2132c25979a09f132c32808726ea3c9a793bd125a9", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/c-1.0.0-py3-none-any.whl", hash = "sha256:3a478c7cdde1574cbec33a8a69a00b06a3d5a7d6d885c7a36839c2f7a7165c59", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "d"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/d-1.0.0.tar.gz", hash = "sha256:4f363304bad30565286697b70b1b48e348267d318562a3afb36af66a8a8cad1d", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/d-1.0.0-py3-none-any.whl", hash = "sha256:951763fd7ea103411bb4fbdd21e0d61688641068f8f4f8a410fc2654a436daa6", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "d"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'darwin'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/d-2.0.0.tar.gz", hash = "sha256:710cbab9073b67674e70a5f5225f81fe5496739fb4d14e6b9ecec40109290ee9", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/d-2.0.0-py3-none-any.whl", hash = "sha256:08a97bc49abdc539a2f6a9865ba3bef51e3321b5f2839ad5d7d13a0ab93d6203", upload-time = "2024-03-24T00:00:00Z" },
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
        sdist = { url = "http://[LOCALHOST]/files/bar-2.0.0.tar.gz", hash = "sha256:fa9a4faf506228722784ed740a362bccd96913f4f98a4e10d45ab79d8abb270a", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/bar-2.0.0-py3-none-any.whl", hash = "sha256:784ed1cc368c82163d5a5a862169df650956184e08277ca8193b4dea4d9a45c6", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "foo"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "bar" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/foo-2.0.0.tar.gz", hash = "sha256:447cd218ce6ee5c4cc8ba7bf5431bd114d5336fa6214a91f28c6fc9ade89bf4c", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/foo-2.0.0-py3-none-any.whl", hash = "sha256:53c304f682a612f55ba47e11dd4330a7ecf0ceff65799800ae455172ccd86a66", upload-time = "2024-03-24T00:00:00Z" },
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
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:3d2b4c28a4e112f3a1cef1db4dc5efa33fcbbcc38bc11ccc80321097db86c097", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-any.whl", hash = "sha256:a81d806fcb056fb2ef0e1ebe929778d2d5895a2fbb2c34a7b04ddb1fe0bba1f4", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "a"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "python_full_version >= '3.14'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-2.0.0.tar.gz", hash = "sha256:80ec95a66cff82a78a3333e3f5702e4254cf80533f21762933252eec58c9869a", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-2.0.0-py3-none-any.whl", hash = "sha256:274c04a6dc88ddc6c0781ec938dc00953b3ceab3950f97ed6da84e231765f555", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "c", marker = "python_full_version == '3.13.*'" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/b-1.0.0.tar.gz", hash = "sha256:6f887656a85fbbf549dddf2da49a17c6b55ca2a31a62ca030e82b8fdfd593177", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-1.0.0-py3-none-any.whl", hash = "sha256:ab723a3d54f0204ae695d230a49cdab4d5f14d5b31cc4e80fd2e016956b7dcce", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "c"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/c-1.0.0.tar.gz", hash = "sha256:6e14a2e7cc6be61fa5aa41c0e55beff8b708a3aea257fed948306a0741bb5c47", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/c-1.0.0-py3-none-any.whl", hash = "sha256:94d63505a37723c7f5106520b366c8ea4859382b4a08fa63757d7383c022378c", upload-time = "2024-03-24T00:00:00Z" },
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
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:8fc0cb224becb328b822daf159f2277eae1be228b916d6caa86484d2d83a5235", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-any.whl", hash = "sha256:d16fde797ad07a590f41649a679a55ca444c1492a4c333385bc706adb055c35e", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "c", marker = "sys_platform == 'darwin'" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/b-1.0.0.tar.gz", hash = "sha256:71e4e9b4e8b7ebf5c34d935f8934c4b3e762ece483247af9ff7913b0d986b96e", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-1.0.0-py3-none-any.whl", hash = "sha256:0461fd84b5f62960add5cdb1a40e1079c85a6ccda7ec9701521c189529ae2825", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "c"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/c-1.0.0.tar.gz", hash = "sha256:6e14a2e7cc6be61fa5aa41c0e55beff8b708a3aea257fed948306a0741bb5c47", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/c-1.0.0-py3-none-any.whl", hash = "sha256:94d63505a37723c7f5106520b366c8ea4859382b4a08fa63757d7383c022378c", upload-time = "2024-03-24T00:00:00Z" },
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
            { name = "b", version = "1.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "implementation_name == 'pypy' and sys_platform == 'darwin'" },
            { name = "b", version = "2.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "implementation_name == 'cpython' and sys_platform == 'darwin'" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:7e3d8e97204e1ea461ae625819c340a5a762b70f34421d4f9721bb7b607a0a93", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-any.whl", hash = "sha256:b36da0e97927d6ddaca4daf0feefa0ef53d84a11507d1254c7b32bdeae5f9750", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "a"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-2.0.0.tar.gz", hash = "sha256:80ec95a66cff82a78a3333e3f5702e4254cf80533f21762933252eec58c9869a", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-2.0.0-py3-none-any.whl", hash = "sha256:274c04a6dc88ddc6c0781ec938dc00953b3ceab3950f97ed6da84e231765f555", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "implementation_name == 'pypy' and sys_platform == 'darwin'",
        ]
        dependencies = [
            { name = "c", marker = "implementation_name == 'pypy' and sys_platform == 'darwin'" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/b-1.0.0.tar.gz", hash = "sha256:7bbe7def4bbd23115cf653d246419c773afd3c1b3e6ba94f19acbc1742f51d08", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-1.0.0-py3-none-any.whl", hash = "sha256:1b2ad8dcefd1334c57921e51d8463137d63458174c08386a437cd3e3c5935d67", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "implementation_name == 'cpython' and sys_platform == 'darwin'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/b-2.0.0.tar.gz", hash = "sha256:18fb09ba28eba255186405065e027093a6e952fa71eb565b4c46d619fdb60809", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-2.0.0-py3-none-any.whl", hash = "sha256:3f145dd24c81c207bad5dd019b8c1ce19e275ac00f2caa3ee844a8e392271f04", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "c"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/c-1.0.0.tar.gz", hash = "sha256:6e14a2e7cc6be61fa5aa41c0e55beff8b708a3aea257fed948306a0741bb5c47", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/c-1.0.0-py3-none-any.whl", hash = "sha256:94d63505a37723c7f5106520b366c8ea4859382b4a08fa63757d7383c022378c", upload-time = "2024-03-24T00:00:00Z" },
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
            { name = "b", version = "1.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "implementation_name == 'pypy' and sys_platform == 'darwin'" },
            { name = "b", version = "2.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "implementation_name == 'cpython' and sys_platform == 'darwin'" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:7e3d8e97204e1ea461ae625819c340a5a762b70f34421d4f9721bb7b607a0a93", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-any.whl", hash = "sha256:b36da0e97927d6ddaca4daf0feefa0ef53d84a11507d1254c7b32bdeae5f9750", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "a"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-2.0.0.tar.gz", hash = "sha256:80ec95a66cff82a78a3333e3f5702e4254cf80533f21762933252eec58c9869a", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-2.0.0-py3-none-any.whl", hash = "sha256:274c04a6dc88ddc6c0781ec938dc00953b3ceab3950f97ed6da84e231765f555", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "implementation_name == 'pypy' and sys_platform == 'darwin'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/b-1.0.0.tar.gz", hash = "sha256:f615a5b13329186c0948d63a275af18758e1346ad512f06366b0534e1c4e3ab3", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-1.0.0-py3-none-any.whl", hash = "sha256:018cea1923423e425a912b4e449e73adf8fbfcd31a5fc2da551cb64174593772", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "implementation_name == 'cpython' and sys_platform == 'darwin'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/b-2.0.0.tar.gz", hash = "sha256:18fb09ba28eba255186405065e027093a6e952fa71eb565b4c46d619fdb60809", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-2.0.0-py3-none-any.whl", hash = "sha256:3f145dd24c81c207bad5dd019b8c1ce19e275ac00f2caa3ee844a8e392271f04", upload-time = "2024-03-24T00:00:00Z" },
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
            { name = "b", version = "1.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "implementation_name == 'pypy' and sys_platform == 'darwin'" },
            { name = "b", version = "2.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "implementation_name == 'cpython' and sys_platform == 'darwin'" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:7e3d8e97204e1ea461ae625819c340a5a762b70f34421d4f9721bb7b607a0a93", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-any.whl", hash = "sha256:b36da0e97927d6ddaca4daf0feefa0ef53d84a11507d1254c7b32bdeae5f9750", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "a"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-2.0.0.tar.gz", hash = "sha256:80ec95a66cff82a78a3333e3f5702e4254cf80533f21762933252eec58c9869a", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-2.0.0-py3-none-any.whl", hash = "sha256:274c04a6dc88ddc6c0781ec938dc00953b3ceab3950f97ed6da84e231765f555", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "implementation_name == 'pypy' and sys_platform == 'darwin'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/b-1.0.0.tar.gz", hash = "sha256:675a6c7a1456ba55a2bb89763b3e58b9086a120918d8a9965b616f81f77150fb", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-1.0.0-py3-none-any.whl", hash = "sha256:70ec4955d4c486b0b20a2cec29f98f57340165882cd488a8e0885dce520385b2", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "implementation_name == 'cpython' and sys_platform == 'darwin'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/b-2.0.0.tar.gz", hash = "sha256:18fb09ba28eba255186405065e027093a6e952fa71eb565b4c46d619fdb60809", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-2.0.0-py3-none-any.whl", hash = "sha256:3f145dd24c81c207bad5dd019b8c1ce19e275ac00f2caa3ee844a8e392271f04", upload-time = "2024-03-24T00:00:00Z" },
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
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:36c9054329425d5b328167c29b8977798e496b738e0c773de19896aeff397ba6", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-any.whl", hash = "sha256:1d5dcb237795f1a02789f0a2cad2893739ea097d532725ab6790e43e31f46002", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "a"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        dependencies = [
            { name = "b", marker = "sys_platform == 'linux'" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-2.0.0.tar.gz", hash = "sha256:1f9238da44c4971ded49abeb4dffd96c319bea753e2b61e4d095cc9110896d13", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-2.0.0-py3-none-any.whl", hash = "sha256:ef5c85d4c61c75bb118e02a3e76e60f9c6dab4363f18bda7ab276bdfed1e136e", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/b-1.0.0.tar.gz", hash = "sha256:b532bd9c3ccd69c4d5e915542dc50fb748c91c7a8e204c75387178d68fca113f", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-1.0.0-py3-none-any.whl", hash = "sha256:6406e0adced5e1b9475c3bdc52be87854c25df751b7fa07b20d7b0d7e7c4c4f3", upload-time = "2024-03-24T00:00:00Z" },
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
            { name = "b", marker = "sys_platform == 'darwin'" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:3f8ff2b2832415dfda5a576afabc2f8b0e93e0e7a0ee9064b2f9c0a6488c1320", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-any.whl", hash = "sha256:d5b88474e4925a60ec739039289d091ec33512b4d55676c6d69c4211b17fc24c", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "a"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-2.0.0.tar.gz", hash = "sha256:80ec95a66cff82a78a3333e3f5702e4254cf80533f21762933252eec58c9869a", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-2.0.0-py3-none-any.whl", hash = "sha256:274c04a6dc88ddc6c0781ec938dc00953b3ceab3950f97ed6da84e231765f555", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "c", marker = "sys_platform == 'darwin'" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/b-1.0.0.tar.gz", hash = "sha256:0e68acfea0cd703f2fa3e0a3b12f71228a0a6f5befc5df7f5f907a4cd153a90e", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-1.0.0-py3-none-any.whl", hash = "sha256:dd30a0d1adc839c205d6a193e4ebb41d49a3e69fd9233e21ce9fb50b970d1dd4", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "c"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/c-1.0.0.tar.gz", hash = "sha256:7f4d834ea98e687d4fb313f6b90abcf10a1b574b7273f8157eed433b7371c305", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/c-1.0.0-py3-none-any.whl", hash = "sha256:864b06b06872ca70e65503bb197b3fb9f3134e8052658d6ac0522e32342949aa", upload-time = "2024-03-24T00:00:00Z" },
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
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:36c9054329425d5b328167c29b8977798e496b738e0c773de19896aeff397ba6", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-any.whl", hash = "sha256:1d5dcb237795f1a02789f0a2cad2893739ea097d532725ab6790e43e31f46002", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "a"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-2.0.0.tar.gz", hash = "sha256:80ec95a66cff82a78a3333e3f5702e4254cf80533f21762933252eec58c9869a", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-2.0.0-py3-none-any.whl", hash = "sha256:274c04a6dc88ddc6c0781ec938dc00953b3ceab3950f97ed6da84e231765f555", upload-time = "2024-03-24T00:00:00Z" },
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
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:c59d625a854e3d8e7cca350ff23a960884bf8a558af994598950e60ecaecf1be", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-any.whl", hash = "sha256:88d7a7f26ac3f709c9d7c660879de404a868fa5f384bcffefa04d52684efe8c3", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "a"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-2.0.0.tar.gz", hash = "sha256:80ec95a66cff82a78a3333e3f5702e4254cf80533f21762933252eec58c9869a", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-2.0.0-py3-none-any.whl", hash = "sha256:274c04a6dc88ddc6c0781ec938dc00953b3ceab3950f97ed6da84e231765f555", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "c", marker = "sys_platform == 'linux'" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/b-1.0.0.tar.gz", hash = "sha256:675a6c7a1456ba55a2bb89763b3e58b9086a120918d8a9965b616f81f77150fb", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-1.0.0-py3-none-any.whl", hash = "sha256:70ec4955d4c486b0b20a2cec29f98f57340165882cd488a8e0885dce520385b2", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "c"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/c-1.0.0.tar.gz", hash = "sha256:6e14a2e7cc6be61fa5aa41c0e55beff8b708a3aea257fed948306a0741bb5c47", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/c-1.0.0-py3-none-any.whl", hash = "sha256:94d63505a37723c7f5106520b366c8ea4859382b4a08fa63757d7383c022378c", upload-time = "2024-03-24T00:00:00Z" },
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
        sdist = { url = "http://[LOCALHOST]/files/a-0.1.0.tar.gz", hash = "sha256:7500398834f46e3567b86bca779e305a94e6039d344819c8672c589c92ba9629", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-0.1.0-py3-none-any.whl", hash = "sha256:d679bc6b0eaee4b4b07ad6fabc88891bd19ea3fb0122880157c65e577ef1156f", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'darwin'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/b-1.0.0.tar.gz", hash = "sha256:b532bd9c3ccd69c4d5e915542dc50fb748c91c7a8e204c75387178d68fca113f", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-1.0.0-py3-none-any.whl", hash = "sha256:6406e0adced5e1b9475c3bdc52be87854c25df751b7fa07b20d7b0d7e7c4c4f3", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/b-2.0.0.tar.gz", hash = "sha256:18fb09ba28eba255186405065e027093a6e952fa71eb565b4c46d619fdb60809", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-2.0.0-py3-none-any.whl", hash = "sha256:3f145dd24c81c207bad5dd019b8c1ce19e275ac00f2caa3ee844a8e392271f04", upload-time = "2024-03-24T00:00:00Z" },
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
        sdist = { url = "http://[LOCALHOST]/files/a-1.3.1.tar.gz", hash = "sha256:21bb6af59c842bf5ffc008914ca6e817139e07965d4093ff76cff8956c6965ff", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.3.1-py3-none-any.whl", hash = "sha256:44f6698aa9c5031e41f4228b9cd1d1a22260a32553d9cf72cd44bc8c70c3f50a", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "2.7"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'darwin'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/b-2.7.tar.gz", hash = "sha256:30fe41d5a9282b73cd50d58eceb33cec85d57c78af4a91fe3e202335f949949f", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-2.7-py3-none-any.whl", hash = "sha256:e0481e7675f064e7e6eaae46ecb257fb4c91503d2a6f5677abd03f7a4975ad1e", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "2.8"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/b-2.8.tar.gz", hash = "sha256:673cbbd654751f7880842420431a400e62458486cb428bc7e508cfea4b9c8cd0", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-2.8-py3-none-any.whl", hash = "sha256:0e6db82530e5e4caee7e91a9c575f8e8c7e2cbe63332cb19167d049ba2a033bb", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "c"
        version = "1.10"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/c-1.10.tar.gz", hash = "sha256:4824f76781aef886ff01094b7c50133c4a0218f46e3c19ff0bd069f2661eee80", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/c-1.10-py3-none-any.whl", hash = "sha256:31a94312ae1b6ceb56ef0198203c1eaa675afeaef1b48108c23cd9e9869d50fd", upload-time = "2024-03-24T00:00:00Z" },
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
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:c2e92ae787edb18204782312a98f7dd9d0116ad5d0e61aa046956bdb2ef2e2e3", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-any.whl", hash = "sha256:c37cba72ea0b32c4b66d51a0418ef22e29a66cb7bb84167ba85f4a53731e24c5", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "c", marker = "sys_platform == 'darwin'" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/b-1.0.0.tar.gz", hash = "sha256:8080895028b838440d4e08b9e6b1cc9c727c625e702c9c208de07fc7e06edfa0", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-1.0.0-py3-none-any.whl", hash = "sha256:eec86455f18244fdbcc942aed9b3289f375792c58c80ac4c5edf1d56fdfd12c9", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "c"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/c-2.0.0.tar.gz", hash = "sha256:72db9a21521acaa8ff10d0ce3bb4b68bc6b275bcb77bdb3debd95388f5120021", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/c-2.0.0-py3-none-any.whl", hash = "sha256:cda9567ddeae2a7733f84dd4fca4053c1f306c992cdc10abb561641e43d297f5", upload-time = "2024-03-24T00:00:00Z" },
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
        sdist = { url = "http://[LOCALHOST]/files/a-1.2.0.tar.gz", hash = "sha256:ea1f0436d9d88c51f66e4154f17f7c6778d0dc7674f75cda35dc4b668fb287a7", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.2.0-py3-none-any.whl", hash = "sha256:d321f9707c4774aa50598d896a1368ea0ca104e95b66ac7f4fd67c483b6307e9", upload-time = "2024-03-24T00:00:00Z" },
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
        sdist = { url = "http://[LOCALHOST]/files/cleaver-1.0.0.tar.gz", hash = "sha256:049d1cd0cb93f315070151f69682f97eec46ff0a3da64d87d387cc11830ba541", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/cleaver-1.0.0-py3-none-any.whl", hash = "sha256:28df935adf0109c6f7171045389872fd22b3ed0c745983440527e91e9bfd2a01", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "fork-if-not-forked"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform != 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/fork_if_not_forked-2.0.0.tar.gz", hash = "sha256:0e93f72e7bcbdc71a1a3573b1f79a747e82c9c238505a847e74d314143eedc18", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/fork_if_not_forked-2.0.0-py3-none-any.whl", hash = "sha256:db1e6c6ee309c8478f07c9eff614f260426e2031992dee85851b475abb8c4471", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "fork-if-not-forked"
        version = "3.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/fork_if_not_forked-3.0.0.tar.gz", hash = "sha256:cdbe6609a59da78b2ea6ddb8703131385dc71fd9d13bca82e35ed5d541a7425d", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/fork_if_not_forked-3.0.0-py3-none-any.whl", hash = "sha256:050490a34922a9851d444b1711d4601b2d5daf685401c0cbc0378926a886b3d8", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "fork-if-not-forked-proxy"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "fork-if-not-forked", version = "2.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "sys_platform != 'linux'" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/fork_if_not_forked_proxy-1.0.0.tar.gz", hash = "sha256:1da9ee55735388976535cfd2ae143c8b9e21b9dd5f01ce9521a4a095a5e9e258", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/fork_if_not_forked_proxy-1.0.0-py3-none-any.whl", hash = "sha256:ceb957dc9b8efe8996c802d4fadfe56debcb40ddf5eb6deb7688e46f4cd71958", upload-time = "2024-03-24T00:00:00Z" },
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
        sdist = { url = "http://[LOCALHOST]/files/reject_cleaver1-1.0.0.tar.gz", hash = "sha256:6da0074d5a0a178cb730df50264fe123ac776ebb72b8624a2e04eefe2c694246", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/reject_cleaver1-1.0.0-py3-none-any.whl", hash = "sha256:570e572d14f2b7901e9e354320a1fe56f743c518fd929d9bd2e6541a61591d83", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "reject-cleaver1"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform != 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/reject_cleaver1-2.0.0.tar.gz", hash = "sha256:3b6adb7793ff0c4bf6e14d10e16156bc184046e1e5b738d921974ef6b9f9b58a", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/reject_cleaver1-2.0.0-py3-none-any.whl", hash = "sha256:af667f607eb5f28631a93e18df608ea0b52ae7c6f1904e4b7389fde1ce3d2bc7", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "reject-cleaver1-proxy"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "reject-cleaver1", version = "2.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "sys_platform != 'linux'" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/reject_cleaver1_proxy-1.0.0.tar.gz", hash = "sha256:43034027a9360a2497ff2558c3efc0652ec5a18b30872cf88ab87bd4a3675799", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/reject_cleaver1_proxy-1.0.0-py3-none-any.whl", hash = "sha256:8177082bec11cb14b8862ab7c26ee7a4c155b1656c5af8b5dfcc7f5907c01056", upload-time = "2024-03-24T00:00:00Z" },
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
            { name = "d", marker = "sys_platform != 'linux'" },
            { name = "reject-cleaver-1", marker = "sys_platform != 'linux'" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/bar-1.0.0.tar.gz", hash = "sha256:7433981e897b2fdbefc4ccc713282a03663dbe9b7468658a702af1d01d241e27", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/bar-1.0.0-py3-none-any.whl", hash = "sha256:1088650879f10865a639909a67dbf8880f2841c6d95fb4b7a9778d408b1f123f", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "bar"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/bar-2.0.0.tar.gz", hash = "sha256:fa9a4faf506228722784ed740a362bccd96913f4f98a4e10d45ab79d8abb270a", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/bar-2.0.0-py3-none-any.whl", hash = "sha256:784ed1cc368c82163d5a5a862169df650956184e08277ca8193b4dea4d9a45c6", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "c"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/c-2.0.0.tar.gz", hash = "sha256:72db9a21521acaa8ff10d0ce3bb4b68bc6b275bcb77bdb3debd95388f5120021", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/c-2.0.0-py3-none-any.whl", hash = "sha256:cda9567ddeae2a7733f84dd4fca4053c1f306c992cdc10abb561641e43d297f5", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "c"
        version = "3.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform != 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/c-3.0.0.tar.gz", hash = "sha256:27d544487f21f6ece9d49d672fc8da1664c48a7ef864d8ff91b756183d77b831", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/c-3.0.0-py3-none-any.whl", hash = "sha256:c9735c8221c6ccd689f9d3048d836792db3750c381ac5af40bae1cc8b6b1185f", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "cleaver"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "bar", version = "1.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "sys_platform != 'linux'" },
            { name = "foo", marker = "sys_platform == 'linux'" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/cleaver-1.0.0.tar.gz", hash = "sha256:9156f0d8df99763e90677ca1b4dab67bee5fa33f59d102626dd91505cf332d2a", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/cleaver-1.0.0-py3-none-any.whl", hash = "sha256:6f65490ce9066dcffd63aff8612c89da4385e0b9aa3808ffade1e5a5a8a9d02c", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "d"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "c", version = "3.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "sys_platform != 'linux'" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/d-1.0.0.tar.gz", hash = "sha256:ef58b0f5f8f9c4d7e7864a36f86ad96ca8cee734095b3e6512ae8c395380ee41", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/d-1.0.0-py3-none-any.whl", hash = "sha256:e75980e1835774a1b4b2c4b1b5b873dcedfd7dd6e89cf96d7171709857ba4f2d", upload-time = "2024-03-24T00:00:00Z" },
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
        sdist = { url = "http://[LOCALHOST]/files/foo-1.0.0.tar.gz", hash = "sha256:b9179a545511f4418bdde59a4907297accb585f0113176790170db5665ca3416", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/foo-1.0.0-py3-none-any.whl", hash = "sha256:0937427a4cf00adedce4f8b461ddf2b6ac73e0ef23d7bc27feb80ed5a8fc2adf", upload-time = "2024-03-24T00:00:00Z" },
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
        sdist = { url = "http://[LOCALHOST]/files/reject_cleaver_1-1.0.0.tar.gz", hash = "sha256:a6383fa52eff39c3635fde33f9a4dd03d2fc7cffdd42fd4c148cf997e6fb5246", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/reject_cleaver_1-1.0.0-py3-none-any.whl", hash = "sha256:ac973aed1a156c9bfad209ba5ac09c54c44c2c6b55fe05aefb995df8ec7d8377", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "unrelated-dep2"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/unrelated_dep2-1.0.0.tar.gz", hash = "sha256:43649d9e654b0a121308187b8a4f43fa2b498e08565e2432bd1e0e3f1728acb2", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/unrelated_dep2-1.0.0-py3-none-any.whl", hash = "sha256:ae20850cf6fed20f5fc4676ad2b22b7d7f55aed6e43cb860ce38d05160b2b938", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "unrelated-dep2"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform != 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/unrelated_dep2-2.0.0.tar.gz", hash = "sha256:292e78906a8e8c74931f9a5b54aba9e6957afe87cca68a2a456363569e7aa652", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/unrelated_dep2-2.0.0-py3-none-any.whl", hash = "sha256:d4727f7a2d5b65b565cbe75c6fc8c42ed5c753ee694ac474ff225c7764438ba1", upload-time = "2024-03-24T00:00:00Z" },
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
        sdist = { url = "http://[LOCALHOST]/files/bar-1.0.0.tar.gz", hash = "sha256:d373f4858d602855ef53231a7f24ebff4e67e1fe30a2e810ee2b2a29b9d1a50a", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/bar-1.0.0-py3-none-any.whl", hash = "sha256:eff4a52b23cd871cca3f6a95e9b66583f486e500b0c5eb171fa9a520f063af27", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "bar"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/bar-2.0.0.tar.gz", hash = "sha256:fa9a4faf506228722784ed740a362bccd96913f4f98a4e10d45ab79d8abb270a", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/bar-2.0.0-py3-none-any.whl", hash = "sha256:784ed1cc368c82163d5a5a862169df650956184e08277ca8193b4dea4d9a45c6", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "cleaver"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "bar", version = "1.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "sys_platform != 'linux'" },
            { name = "foo", marker = "sys_platform == 'linux'" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/cleaver-1.0.0.tar.gz", hash = "sha256:9156f0d8df99763e90677ca1b4dab67bee5fa33f59d102626dd91505cf332d2a", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/cleaver-1.0.0-py3-none-any.whl", hash = "sha256:6f65490ce9066dcffd63aff8612c89da4385e0b9aa3808ffade1e5a5a8a9d02c", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "foo"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/foo-1.0.0.tar.gz", hash = "sha256:4d4cf959f969e0a39663aca6064428819f1e1c6be60d9a35164801ce17950ed4", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/foo-1.0.0-py3-none-any.whl", hash = "sha256:a1bdba17d92a0acbf00f18c5bb316b28ce302b7ce953291975aeb864b2996d2b", upload-time = "2024-03-24T00:00:00Z" },
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
            { name = "b", version = "1.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "os_name == 'darwin' and sys_platform == 'illumos'" },
            { name = "b", version = "2.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "os_name == 'linux' and sys_platform == 'illumos'" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:1073194686665f5b1459914275b353a20eafca9d9aa550f4c493aa0aceba119a", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-any.whl", hash = "sha256:05dee76753ed4b976028450f58906cf57764348f59bb5fd40375dbd2738e9dd1", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "a"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'windows'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-2.0.0.tar.gz", hash = "sha256:80ec95a66cff82a78a3333e3f5702e4254cf80533f21762933252eec58c9869a", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-2.0.0-py3-none-any.whl", hash = "sha256:274c04a6dc88ddc6c0781ec938dc00953b3ceab3950f97ed6da84e231765f555", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "os_name == 'darwin' and sys_platform == 'illumos'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/b-1.0.0.tar.gz", hash = "sha256:b532bd9c3ccd69c4d5e915542dc50fb748c91c7a8e204c75387178d68fca113f", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-1.0.0-py3-none-any.whl", hash = "sha256:6406e0adced5e1b9475c3bdc52be87854c25df751b7fa07b20d7b0d7e7c4c4f3", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "os_name == 'linux' and sys_platform == 'illumos'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/b-2.0.0.tar.gz", hash = "sha256:18fb09ba28eba255186405065e027093a6e952fa71eb565b4c46d619fdb60809", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-2.0.0-py3-none-any.whl", hash = "sha256:3f145dd24c81c207bad5dd019b8c1ce19e275ac00f2caa3ee844a8e392271f04", upload-time = "2024-03-24T00:00:00Z" },
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
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:0f32d06c5dab1a669df8d282b93b1d1af33e685ed4f52393d0b216436b5f52dc", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-any.whl", hash = "sha256:f3df0e931504190bab18ffc626bc3bb78b907dd44673fc9a635ddb926ab934d3", upload-time = "2024-03-24T00:00:00Z" },
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
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:0f32d06c5dab1a669df8d282b93b1d1af33e685ed4f52393d0b216436b5f52dc", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-cp310-cp310-any.whl", hash = "sha256:6d83c3a755c2d22b385314232ef9eb7695b379d0cd8a2f950362738b75fdd7f0", upload-time = "2024-03-24T00:00:00Z" },
            { url = "http://[LOCALHOST]/files/a-1.0.0-cp311-cp311-any.whl", hash = "sha256:ddb4ab1480ee225d17a72f49b53cd5b934c84bb7bd719132f8dc7cf5a65b25f1", upload-time = "2024-03-24T00:00:00Z" },
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
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:4e402b215539e074a824ab44c7eaf04b6451bb1ab36d0cb045374828bb8bdcc7", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-any.whl", hash = "sha256:f8e5ac807d803ceba9c49d0089c195ae556a1a171ed940e5afddac7f54725258", upload-time = "2024-03-24T00:00:00Z" },
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
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:3d2b4c28a4e112f3a1cef1db4dc5efa33fcbbcc38bc11ccc80321097db86c097", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-cp312-cp312-win_amd64.whl", hash = "sha256:ad52e945035f3a5a61e843d5c6d6cce0ff81183bfc6b2e1ac5099194e62de04f", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/b-1.0.0.tar.gz", hash = "sha256:b532bd9c3ccd69c4d5e915542dc50fb748c91c7a8e204c75387178d68fca113f", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-1.0.0-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:5dce5329b0892e56a874514273babad5d43ec7b3e765c97e6d85cdaa4893288c", upload-time = "2024-03-24T00:00:00Z" },
            { url = "http://[LOCALHOST]/files/b-1.0.0-cp312-cp312-musllinux_1_1_armv7l.whl", hash = "sha256:bd37cc5c7267d1e98304dccb5556515ace0ca4638d892d56f746e1de33c6a1c4", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "c"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/c-1.0.0.tar.gz", hash = "sha256:6e14a2e7cc6be61fa5aa41c0e55beff8b708a3aea257fed948306a0741bb5c47", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/c-1.0.0-cp312-cp312-macosx_14_0_x86_64.whl", hash = "sha256:03c572d970ec98989986ea8ef0f0c94f25eac6b601e73a1b140fcb12aaaf6112", upload-time = "2024-03-24T00:00:00Z" },
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
        sdist = { url = "http://[LOCALHOST]/files/psycopg-1.0.0.tar.gz", hash = "sha256:c513b2021142cd4f00f9c09d92106d6cfe180f1b649f763ea6bd89b84ca9f1fb", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/psycopg-1.0.0-py3-none-any.whl", hash = "sha256:e95e655367be53bdc7a53203d5fc1941f29f9802b15ae5b49c42aac8679a460b", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [package.optional-dependencies]
        binary = [
            { name = "psycopg-binary", marker = "implementation_name != 'pypy' and platform_python_implementation != 'PyPy'" },
        ]

        [[package]]
        name = "psycopg-binary"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/psycopg_binary-1.0.0.tar.gz", hash = "sha256:64c3ee856436c3f1d564bf46476dfcd049a507b38eb74b14b48de437143d31c8", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/psycopg_binary-1.0.0-py3-none-any.whl", hash = "sha256:3f8683f244fb1484dc5daa9033869294cc5193b60acf61a3b62cf527936e44ad", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "tzdata"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/tzdata-1.0.0.tar.gz", hash = "sha256:f2285c9ed855e3433bcd3aabb0b446753f97bcdf8dffebe4725d70af8a1f2fe5", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/tzdata-1.0.0-py3-none-any.whl", hash = "sha256:c246f39402d95aad9ad98cdca0d3d98118ca544ef86e1ef549a09b58b668f5c5", upload-time = "2024-03-24T00:00:00Z" },
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
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:d01ad61c229cf2c4efe61755254b7419b078c7e712907c2e594b0a31ee1ac17e", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-any.whl", hash = "sha256:e61889037d1d2ada6112dcd284f057289b127f21bd0324011387b4a4472cf3f2", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/b-1.0.0.tar.gz", hash = "sha256:b532bd9c3ccd69c4d5e915542dc50fb748c91c7a8e204c75387178d68fca113f", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-1.0.0-py3-none-any.whl", hash = "sha256:6406e0adced5e1b9475c3bdc52be87854c25df751b7fa07b20d7b0d7e7c4c4f3", upload-time = "2024-03-24T00:00:00Z" },
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
            { url = "http://[LOCALHOST]/files/win_only-1.0.0-cp312-abi3-win_amd64.whl", hash = "sha256:99c3a4c97c505574b2139983dc17a651664a35d30c55617f17f908246e141f84", upload-time = "2024-03-24T00:00:00Z" },
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
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:ea7bef4508a5b327b7ce64ab4ffda42d30d0818af6afab6297db8c32300e67d8", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-any.whl", hash = "sha256:9acd6b4f4353047bb16329c2a4db4e2b7017aea6b47c54079d58bfce027f4317", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-1.0.0-cp313-cp313-freebsd_13_aarch64.whl", hash = "sha256:32b85d961e0b3b6591d66d757dbebb67cca155e42c095efc9fbc858fb21b01d3", upload-time = "2024-03-24T00:00:00Z" },
            { url = "http://[LOCALHOST]/files/b-1.0.0-cp313-cp313-freebsd_13_x86_64.whl", hash = "sha256:c514a05814db51291b504f60bae34808b45576ef6a5cc3e93ce124608d0d04c2", upload-time = "2024-03-24T00:00:00Z" },
            { url = "http://[LOCALHOST]/files/b-1.0.0-cp313-cp313-macosx_10_9_x86_64.whl", hash = "sha256:388897fd591f9baf0b39b05fbf9675fd0d8301e4e11eaa8fce9a847857690c67", upload-time = "2024-03-24T00:00:00Z" },
            { url = "http://[LOCALHOST]/files/b-1.0.0-cp313-cp313-manylinux2010_x86_64.whl", hash = "sha256:fd4e521818ef00e3ad71fd6a74808e55a3497f8432f8294a008c6dd10212bc64", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "c"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        wheels = [
            { url = "http://[LOCALHOST]/files/c-1.0.0-cp313-cp313-freebsd_13_aarch64.whl", hash = "sha256:6f34d008a5c2ca4947fc829665ca1291de1b2744456fde000a3d839e06699a1d", upload-time = "2024-03-24T00:00:00Z" },
            { url = "http://[LOCALHOST]/files/c-1.0.0-cp313-cp313-freebsd_13_x86_64.whl", hash = "sha256:2a3e5181140e4acec6638e5fd1e5f0893cd5e032aa016af467a96fd0c00632df", upload-time = "2024-03-24T00:00:00Z" },
            { url = "http://[LOCALHOST]/files/c-1.0.0-cp313-cp313-macosx_10_9_arm64.whl", hash = "sha256:2f6667c36d6d216bae24f7b48a8d0203d8811040cda936f613ccac4316e2b947", upload-time = "2024-03-24T00:00:00Z" },
            { url = "http://[LOCALHOST]/files/c-1.0.0-cp313-cp313-manylinux2010_aarch64.whl", hash = "sha256:cbd0688bdedd4ce12304e17294ba75b79e9b0aa430f7eac4c9be5e0f11075566", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "d"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        wheels = [
            { url = "http://[LOCALHOST]/files/d-1.0.0-cp313-cp313-freebsd_13_aarch64.whl", hash = "sha256:fcd545e6e5d3877b84e0ee902b4dd868715c9e57335c5fad0673391bd5ce7c5a", upload-time = "2024-03-24T00:00:00Z" },
            { url = "http://[LOCALHOST]/files/d-1.0.0-cp313-cp313-freebsd_13_x86_64.whl", hash = "sha256:ef2731c9184cf2e3cbc8689ad8e0c8a567f075a09f9d458ec445ed50f59f9efe", upload-time = "2024-03-24T00:00:00Z" },
            { url = "http://[LOCALHOST]/files/d-1.0.0-cp313-cp313-manylinux2010_i686.whl", hash = "sha256:2239a0197c315c9aa91a99768c561d443d6265be0a3fd4e0acb825fc0d1f6b55", upload-time = "2024-03-24T00:00:00Z" },
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
