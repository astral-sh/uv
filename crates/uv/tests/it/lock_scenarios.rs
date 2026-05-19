//! DO NOT EDIT
//!
//! Generated with `cargo dev generate-scenarios`
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

        [options]
        exclude-newer = "2026-05-12T02:01:30Z"

        [[package]]
        name = "a"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:4c45b53a975754849aa6da0c80ebf1db847a712e06ed36f6eebe0e46a0056fb1", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-any.whl", hash = "sha256:3bb550e2717befbb11afa0d1f3dc9b4f6775a6a805f5bea7b0da6dc980b47520", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "2.0.9"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "a" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/b-2.0.9.tar.gz", hash = "sha256:bfe6bc97704d2d581a5041fa83bdc8ba46391086ad2f77a0b1d22d8687786a33", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-2.0.9-py3-none-any.whl", hash = "sha256:12ad87cac056e0e3211f6c09fc8925181dcf8d7c332d670bf14c7ea6989a746f", upload-time = "2024-03-24T00:00:00Z" },
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

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

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

        [options]
        exclude-newer = "2026-05-12T02:01:30Z"

        [[package]]
        name = "a"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/a-2.0.0.tar.gz", hash = "sha256:220578d86869d9fd54c47cd9b4219c065bf712acd9fc0719b4473d17ba6729d2", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-2.0.0-py3-none-any.whl", hash = "sha256:6a62b0a0a71b6d01beeeb72e7fa7aa30a3a457f5ee0357b40bd66a64463dc3b4", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "b-inner" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/b-1.0.0.tar.gz", hash = "sha256:fa8dd9533d2460c30294b33bf9fe0562678c4ba1483b700ab153d79c0268c9b6", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-1.0.0-py3-none-any.whl", hash = "sha256:b5e09c13671d3f4d41d7d04b110edaf6f6fdd6caed4caa9319248d9cbb6ced91", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b-inner"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "too-old" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/b_inner-1.0.0.tar.gz", hash = "sha256:217239c6cb257b847cb8368421341c3c78309cb64e9d8a8e03473c1e83ea1a7d", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b_inner-1.0.0-py3-none-any.whl", hash = "sha256:b32b4885eefef71892c1f49df45a9b95f67f15d3d1d628968c8b1f2db3eee5a7", upload-time = "2024-03-24T00:00:00Z" },
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
        sdist = { url = "http://[LOCALHOST]/files/too_old-1.0.0.tar.gz", hash = "sha256:8c9f7a48cf7b6b9f5f91eb5e285aaebb2bf3420248aa4ee14dddec9b4a0e45c7", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/too_old-1.0.0-py3-none-any.whl", hash = "sha256:57ffafea045b55b3fe73501c0a5b45db373f058df4168aa580878260ab2a4b52", upload-time = "2024-03-24T00:00:00Z" },
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

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

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

        [options]
        exclude-newer = "2026-05-12T02:01:30Z"

        [[package]]
        name = "a"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:4c45b53a975754849aa6da0c80ebf1db847a712e06ed36f6eebe0e46a0056fb1", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-any.whl", hash = "sha256:3bb550e2717befbb11afa0d1f3dc9b4f6775a6a805f5bea7b0da6dc980b47520", upload-time = "2024-03-24T00:00:00Z" },
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

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

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

        [options]
        exclude-newer = "2026-05-12T02:01:30Z"

        [[package]]
        name = "a"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:4c45b53a975754849aa6da0c80ebf1db847a712e06ed36f6eebe0e46a0056fb1", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-any.whl", hash = "sha256:3bb550e2717befbb11afa0d1f3dc9b4f6775a6a805f5bea7b0da6dc980b47520", upload-time = "2024-03-24T00:00:00Z" },
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

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

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

        [options]
        exclude-newer = "2026-05-12T02:01:30Z"

        [[package]]
        name = "a"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'darwin'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:4c45b53a975754849aa6da0c80ebf1db847a712e06ed36f6eebe0e46a0056fb1", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-any.whl", hash = "sha256:3bb550e2717befbb11afa0d1f3dc9b4f6775a6a805f5bea7b0da6dc980b47520", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "a"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-2.0.0.tar.gz", hash = "sha256:220578d86869d9fd54c47cd9b4219c065bf712acd9fc0719b4473d17ba6729d2", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-2.0.0-py3-none-any.whl", hash = "sha256:6a62b0a0a71b6d01beeeb72e7fa7aa30a3a457f5ee0357b40bd66a64463dc3b4", upload-time = "2024-03-24T00:00:00Z" },
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

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

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

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

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

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

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

        [options]
        exclude-newer = "2026-05-12T02:01:30Z"

        [[package]]
        name = "a"
        version = "4.3.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'darwin'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-4.3.0.tar.gz", hash = "sha256:365ad30e070bf7305b7ca8161ca2f10cf3a95426beebfb95cd387f67eb94c9e1", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-4.3.0-py3-none-any.whl", hash = "sha256:4eae5bcfed5c2ed82a4826af49f35270973046f88c62198f578a578838723900", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "a"
        version = "4.4.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-4.4.0.tar.gz", hash = "sha256:4e53764dd62711d046e892a1c450b286fd58149b3deeffd4369f3115aecd8ef4", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-4.4.0-py3-none-any.whl", hash = "sha256:bddc32211e7564f035b45d1381c63e72be059920a12d693fd47719b06afbae8b", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "d", version = "1.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "sys_platform == 'linux'" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/b-1.0.0.tar.gz", hash = "sha256:93e860cbfbdc3aac05371115e0893559d7c662f45b603d4636f82fe501c982a9", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-1.0.0-py3-none-any.whl", hash = "sha256:93ba2c158db91acf4004b98a4da25642f5f5eaa46bebcb5418ad836fb1ef2226", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "c"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "d", version = "2.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "sys_platform == 'darwin'" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/c-1.0.0.tar.gz", hash = "sha256:22da28fb7e05128da0ef406ba5a2de8c785c46e4b49540169c278acfbbe020b5", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/c-1.0.0-py3-none-any.whl", hash = "sha256:5fa869c2f8c0bb3ceb85f00d42a2f65fcebb67322b6c792b8d3c9c1fdd7dbaff", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "d"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/d-1.0.0.tar.gz", hash = "sha256:5207beaf1587ef55869637ed444e110357aeb807bf82099b248d65c7a821dfbb", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/d-1.0.0-py3-none-any.whl", hash = "sha256:b41fe5d94d1cc63db5dad7569ff9d1cbe0381bb85a3c6aa87fcc440dd4b01d0f", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "d"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'darwin'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/d-2.0.0.tar.gz", hash = "sha256:8317a53240e9456084109ae7c3bbb9ddfa9458e516cf5625489836cc01de43e5", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/d-2.0.0-py3-none-any.whl", hash = "sha256:fdc6021e4bdf8b73707394cf349e0ae4b914664f1c362b44d46a22cfcb495487", upload-time = "2024-03-24T00:00:00Z" },
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

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

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

        [options]
        exclude-newer = "2026-05-12T02:01:30Z"

        [[package]]
        name = "bar"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/bar-2.0.0.tar.gz", hash = "sha256:8d6ec45c5353de7f61472245dc056c74aeadabe1babd35f686b82a3a5e6676ee", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/bar-2.0.0-py3-none-any.whl", hash = "sha256:30058fca0b5a4d570025278d8f2a7c2a05360e355e8b1b1f186fce304ae63696", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "foo"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "bar" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/foo-2.0.0.tar.gz", hash = "sha256:8c86bd216e66c21a3b5cc658bd6620c72c2d689efcf5643490199f8e39d42885", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/foo-2.0.0-py3-none-any.whl", hash = "sha256:02ac0da19d145413246a117538284b5c5bf99d3100e7bfa844618239fb0347c6", upload-time = "2024-03-24T00:00:00Z" },
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

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

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

        [options]
        exclude-newer = "2026-05-12T02:01:30Z"

        [[package]]
        name = "a"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "python_full_version < '3.13'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:4c45b53a975754849aa6da0c80ebf1db847a712e06ed36f6eebe0e46a0056fb1", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-any.whl", hash = "sha256:3bb550e2717befbb11afa0d1f3dc9b4f6775a6a805f5bea7b0da6dc980b47520", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "a"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "python_full_version >= '3.14'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-2.0.0.tar.gz", hash = "sha256:220578d86869d9fd54c47cd9b4219c065bf712acd9fc0719b4473d17ba6729d2", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-2.0.0-py3-none-any.whl", hash = "sha256:6a62b0a0a71b6d01beeeb72e7fa7aa30a3a457f5ee0357b40bd66a64463dc3b4", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "c", marker = "python_full_version == '3.13.*'" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/b-1.0.0.tar.gz", hash = "sha256:dc3c65e13f1a81e3413f788ed9a1029d7965176ce1c75e67b1185991695384cb", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-1.0.0-py3-none-any.whl", hash = "sha256:e7d730fad282cff419b96ca050b0a0abfb49e430ff1494158a642f639a27660e", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "c"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/c-1.0.0.tar.gz", hash = "sha256:c992f835174aa9f6427c57d4f26403cfb15f48d523bb9e5492872c0c2481eb18", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/c-1.0.0-py3-none-any.whl", hash = "sha256:6538c793eb1a787d65d1da4730c21cb517ae7b6d6d770bda939c1932b1e8a01c", upload-time = "2024-03-24T00:00:00Z" },
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

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

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

        [options]
        exclude-newer = "2026-05-12T02:01:30Z"

        [[package]]
        name = "a"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "c", marker = "sys_platform == 'linux'" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:22a204699e525f1a86b99705c1c407083b0d630c92112e9c7ced0bbe81a2c2a1", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-any.whl", hash = "sha256:d8cabfd6f2f2a4940fefc6b872519c06d1699f660722d5ee5856ebadd9dc3786", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "c", marker = "sys_platform == 'darwin'" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/b-1.0.0.tar.gz", hash = "sha256:cc3376128b341ca5c83a2bd11b165901263cde823da3385789e735e79b15588f", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-1.0.0-py3-none-any.whl", hash = "sha256:bdfa46036256430c2cc7a84f57d63abb17a3523e297bad8a2e14d3ef0574c26f", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "c"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/c-1.0.0.tar.gz", hash = "sha256:c992f835174aa9f6427c57d4f26403cfb15f48d523bb9e5492872c0c2481eb18", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/c-1.0.0-py3-none-any.whl", hash = "sha256:6538c793eb1a787d65d1da4730c21cb517ae7b6d6d770bda939c1932b1e8a01c", upload-time = "2024-03-24T00:00:00Z" },
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

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

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

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

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

        [options]
        exclude-newer = "2026-05-12T02:01:30Z"

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
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:e465af2d85eb5f988ba926de54dbc78d85ce8b03b3da8fb2e90eefec19ed1acf", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-any.whl", hash = "sha256:49ed99dc5f047f6000118bac0ed1b56436b1af35e291d15f99e8df3c4b232d63", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "a"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-2.0.0.tar.gz", hash = "sha256:220578d86869d9fd54c47cd9b4219c065bf712acd9fc0719b4473d17ba6729d2", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-2.0.0-py3-none-any.whl", hash = "sha256:6a62b0a0a71b6d01beeeb72e7fa7aa30a3a457f5ee0357b40bd66a64463dc3b4", upload-time = "2024-03-24T00:00:00Z" },
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
        sdist = { url = "http://[LOCALHOST]/files/b-1.0.0.tar.gz", hash = "sha256:ad334835ac83492e1ec3fcab2bacb82002a5345c3c1cd75d044920e75ed9496c", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-1.0.0-py3-none-any.whl", hash = "sha256:06adc6ffa2076f1fa31cedca3d88fa364f738bec0fc1e6101fd3ec46c95d3df0", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "implementation_name == 'cpython' and sys_platform == 'darwin'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/b-2.0.0.tar.gz", hash = "sha256:0736c2aa1d22756aec5aec3a66de4527a9647ece471816db7b984b06e52dcba6", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-2.0.0-py3-none-any.whl", hash = "sha256:d0d9a8026b777021642e5d3ca9fec669eb63d9742a59a3d1872edab105b7cd81", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "c"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/c-1.0.0.tar.gz", hash = "sha256:c992f835174aa9f6427c57d4f26403cfb15f48d523bb9e5492872c0c2481eb18", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/c-1.0.0-py3-none-any.whl", hash = "sha256:6538c793eb1a787d65d1da4730c21cb517ae7b6d6d770bda939c1932b1e8a01c", upload-time = "2024-03-24T00:00:00Z" },
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

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

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

        [options]
        exclude-newer = "2026-05-12T02:01:30Z"

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
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:e465af2d85eb5f988ba926de54dbc78d85ce8b03b3da8fb2e90eefec19ed1acf", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-any.whl", hash = "sha256:49ed99dc5f047f6000118bac0ed1b56436b1af35e291d15f99e8df3c4b232d63", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "a"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-2.0.0.tar.gz", hash = "sha256:220578d86869d9fd54c47cd9b4219c065bf712acd9fc0719b4473d17ba6729d2", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-2.0.0-py3-none-any.whl", hash = "sha256:6a62b0a0a71b6d01beeeb72e7fa7aa30a3a457f5ee0357b40bd66a64463dc3b4", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "implementation_name == 'pypy' and sys_platform == 'darwin'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/b-1.0.0.tar.gz", hash = "sha256:408e835f403da9eb686583524bdab76ade62238ff26f94046a1ffe9161ed83d6", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-1.0.0-py3-none-any.whl", hash = "sha256:7a883705ff22f5905afb169243e759c91bcc41c8153ababd0b75c6fe012ad2d2", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "implementation_name == 'cpython' and sys_platform == 'darwin'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/b-2.0.0.tar.gz", hash = "sha256:0736c2aa1d22756aec5aec3a66de4527a9647ece471816db7b984b06e52dcba6", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-2.0.0-py3-none-any.whl", hash = "sha256:d0d9a8026b777021642e5d3ca9fec669eb63d9742a59a3d1872edab105b7cd81", upload-time = "2024-03-24T00:00:00Z" },
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

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

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

        [options]
        exclude-newer = "2026-05-12T02:01:30Z"

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
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:e465af2d85eb5f988ba926de54dbc78d85ce8b03b3da8fb2e90eefec19ed1acf", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-any.whl", hash = "sha256:49ed99dc5f047f6000118bac0ed1b56436b1af35e291d15f99e8df3c4b232d63", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "a"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-2.0.0.tar.gz", hash = "sha256:220578d86869d9fd54c47cd9b4219c065bf712acd9fc0719b4473d17ba6729d2", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-2.0.0-py3-none-any.whl", hash = "sha256:6a62b0a0a71b6d01beeeb72e7fa7aa30a3a457f5ee0357b40bd66a64463dc3b4", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "implementation_name == 'pypy' and sys_platform == 'darwin'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/b-1.0.0.tar.gz", hash = "sha256:8d3ee692bde773fdf75296e2b9390b1d2a4275af19bcae3c984dff0c142f4ace", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-1.0.0-py3-none-any.whl", hash = "sha256:2669bd01ef1fc7729d9c25c6689df45482af12edd515aaa6f1e71b465b2fa5cf", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "implementation_name == 'cpython' and sys_platform == 'darwin'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/b-2.0.0.tar.gz", hash = "sha256:0736c2aa1d22756aec5aec3a66de4527a9647ece471816db7b984b06e52dcba6", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-2.0.0-py3-none-any.whl", hash = "sha256:d0d9a8026b777021642e5d3ca9fec669eb63d9742a59a3d1872edab105b7cd81", upload-time = "2024-03-24T00:00:00Z" },
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

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

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

        [options]
        exclude-newer = "2026-05-12T02:01:30Z"

        [[package]]
        name = "a"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'darwin'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:387432fe70844699bcf10feadad03facaf0b28731d5227efd3fe90fd7ebb6e3e", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-any.whl", hash = "sha256:527c38843f560be7c57cbd9ea29ba590a7170001b1b2255a9b1f3a6adaf02802", upload-time = "2024-03-24T00:00:00Z" },
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
        sdist = { url = "http://[LOCALHOST]/files/a-2.0.0.tar.gz", hash = "sha256:01ae6cc3b88cb8cfd45df882fa85cb05896f212f69a5a0546f7b790eb163b385", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-2.0.0-py3-none-any.whl", hash = "sha256:b6ebfbb12c5fa225cd89697c703bc8a5a89e90e7cc169d2b13d5396657c125e5", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/b-1.0.0.tar.gz", hash = "sha256:76826fb2fb2840bdd8f0b371f4e83e1281e71aab7cfad5cc946283fc2589e8dc", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-1.0.0-py3-none-any.whl", hash = "sha256:7bfefe7c9de97c4900f6a712427046f21236b96ac6081cae7701009038ea2b72", upload-time = "2024-03-24T00:00:00Z" },
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

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

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

        [options]
        exclude-newer = "2026-05-12T02:01:30Z"

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
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:08af1cb0165e0c951890bd56691955bad7178f21b3dcde6cddbeaba36586daca", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-any.whl", hash = "sha256:075db311ef6c99e6b108056db2b1ced6c9d635b34a458bae137c562fade89585", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "a"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-2.0.0.tar.gz", hash = "sha256:220578d86869d9fd54c47cd9b4219c065bf712acd9fc0719b4473d17ba6729d2", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-2.0.0-py3-none-any.whl", hash = "sha256:6a62b0a0a71b6d01beeeb72e7fa7aa30a3a457f5ee0357b40bd66a64463dc3b4", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "c", marker = "sys_platform == 'darwin'" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/b-1.0.0.tar.gz", hash = "sha256:dc23da007c3a9a3424f5accebb9eed86a09b1816084e37b54793891090276c07", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-1.0.0-py3-none-any.whl", hash = "sha256:c82e8b72e46b0423687649a5fff24a23ff1ef4a0b8de8548f55f0a929b4eda5e", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "c"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/c-1.0.0.tar.gz", hash = "sha256:e4f7802cdbac234e1202cbbf8a4479f50d76ddf9abb036a5a9016cd778ff04f2", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/c-1.0.0-py3-none-any.whl", hash = "sha256:061278709034937dc5f19394b3c2018caa82ff52adc882526a1e8985fa30ece6", upload-time = "2024-03-24T00:00:00Z" },
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

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

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

        [options]
        exclude-newer = "2026-05-12T02:01:30Z"

        [[package]]
        name = "a"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'darwin'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:387432fe70844699bcf10feadad03facaf0b28731d5227efd3fe90fd7ebb6e3e", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-any.whl", hash = "sha256:527c38843f560be7c57cbd9ea29ba590a7170001b1b2255a9b1f3a6adaf02802", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "a"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-2.0.0.tar.gz", hash = "sha256:220578d86869d9fd54c47cd9b4219c065bf712acd9fc0719b4473d17ba6729d2", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-2.0.0-py3-none-any.whl", hash = "sha256:6a62b0a0a71b6d01beeeb72e7fa7aa30a3a457f5ee0357b40bd66a64463dc3b4", upload-time = "2024-03-24T00:00:00Z" },
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

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

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

        [options]
        exclude-newer = "2026-05-12T02:01:30Z"

        [[package]]
        name = "a"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'darwin'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:2a7a59790d2b972c32f4c95a98c584fc172c57193c6e9215c6f8e56a2ff49db8", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-any.whl", hash = "sha256:fd773b571195e61b7ea5c0054a4e63bd6aad50ba4d56f91593d85a90a6b579f0", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "a"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-2.0.0.tar.gz", hash = "sha256:220578d86869d9fd54c47cd9b4219c065bf712acd9fc0719b4473d17ba6729d2", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-2.0.0-py3-none-any.whl", hash = "sha256:6a62b0a0a71b6d01beeeb72e7fa7aa30a3a457f5ee0357b40bd66a64463dc3b4", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "c", marker = "sys_platform == 'linux'" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/b-1.0.0.tar.gz", hash = "sha256:8d3ee692bde773fdf75296e2b9390b1d2a4275af19bcae3c984dff0c142f4ace", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-1.0.0-py3-none-any.whl", hash = "sha256:2669bd01ef1fc7729d9c25c6689df45482af12edd515aaa6f1e71b465b2fa5cf", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "c"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/c-1.0.0.tar.gz", hash = "sha256:c992f835174aa9f6427c57d4f26403cfb15f48d523bb9e5492872c0c2481eb18", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/c-1.0.0-py3-none-any.whl", hash = "sha256:6538c793eb1a787d65d1da4730c21cb517ae7b6d6d770bda939c1932b1e8a01c", upload-time = "2024-03-24T00:00:00Z" },
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

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

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

        [options]
        exclude-newer = "2026-05-12T02:01:30Z"

        [[package]]
        name = "a"
        version = "0.1.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/a-0.1.0.tar.gz", hash = "sha256:1ff22dd78c6a4f907b6b4ec9cd66decc18a8a1154999af5fb9e378a8e7138606", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-0.1.0-py3-none-any.whl", hash = "sha256:fe3c334817464011aa9fcc76d74785d0e27e391138e61769600d2fd839f8fe3e", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'darwin'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/b-1.0.0.tar.gz", hash = "sha256:76826fb2fb2840bdd8f0b371f4e83e1281e71aab7cfad5cc946283fc2589e8dc", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-1.0.0-py3-none-any.whl", hash = "sha256:7bfefe7c9de97c4900f6a712427046f21236b96ac6081cae7701009038ea2b72", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/b-2.0.0.tar.gz", hash = "sha256:0736c2aa1d22756aec5aec3a66de4527a9647ece471816db7b984b06e52dcba6", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-2.0.0-py3-none-any.whl", hash = "sha256:d0d9a8026b777021642e5d3ca9fec669eb63d9742a59a3d1872edab105b7cd81", upload-time = "2024-03-24T00:00:00Z" },
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

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

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

        [options]
        exclude-newer = "2026-05-12T02:01:30Z"

        [[package]]
        name = "a"
        version = "1.3.1"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "c", marker = "implementation_name == 'iron'" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-1.3.1.tar.gz", hash = "sha256:2ae4daa645a3cfbab78e6a8dc04c1af6746689368294d0a90c51cb47bd9dbda3", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.3.1-py3-none-any.whl", hash = "sha256:63dc8b2d02d39ab3e243870b9852eac9660e8d91377a0bb37f61ec6e0f2e2e9b", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "2.7"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'darwin'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/b-2.7.tar.gz", hash = "sha256:7225686685495a8ff4ef9aef0a00a9c3b1aa527d91d3c10589edde23998e936f", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-2.7-py3-none-any.whl", hash = "sha256:18dd3f4b87a2291e6cfd14b7d096a5834b1a7986b60eb8e74ab60945c19b81cd", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "2.8"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/b-2.8.tar.gz", hash = "sha256:f329b4d67cb590b0d031ce7a30834b0b0b2063f7e07d757964eb19807dcc23b2", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-2.8-py3-none-any.whl", hash = "sha256:2bf4db5fd525f4581366a48de53474a63b9b95b0272c67a2f531d0caa20216dd", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "c"
        version = "1.10"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/c-1.10.tar.gz", hash = "sha256:a0e36fd6a93cf1ff1c5f56bfd239d6ce74aa34cdff2d28d67dcc9471b4195c0f", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/c-1.10-py3-none-any.whl", hash = "sha256:ba2d23a8f3a3114db4559d2d592af787dc944508110dbf1abd43a0edbf645a4b", upload-time = "2024-03-24T00:00:00Z" },
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

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

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

        [options]
        exclude-newer = "2026-05-12T02:01:30Z"

        [[package]]
        name = "a"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "c", marker = "sys_platform == 'linux'" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:12e254807f0dc32c599883aac1a2a18ecf349d041924803e8cf2bca26279cbb3", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-any.whl", hash = "sha256:afa1008030c031fc70e50b13f8cc1117023274f05b0531a25f7d1e834ca376d2", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "c", marker = "sys_platform == 'darwin'" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/b-1.0.0.tar.gz", hash = "sha256:d9d543e4b7cc70e2376e26fc555ecb633f160dc3b2d2c96b58eb1565c3c82c09", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-1.0.0-py3-none-any.whl", hash = "sha256:481f0b473749804b60655300acc9b17258ce3c7665dc7c6af8aea383a2ed72ee", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "c"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/c-2.0.0.tar.gz", hash = "sha256:16b76bba2ca5b44d7dc224071b6c24d754a8857a6a37dcb3128654c69eb91687", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/c-2.0.0-py3-none-any.whl", hash = "sha256:e4250088158b745edd41612deab90a2070681364aa5a79ea426e59766974902d", upload-time = "2024-03-24T00:00:00Z" },
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

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

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

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

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

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

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

        [options]
        exclude-newer = "2026-05-12T02:01:30Z"

        [[package]]
        name = "a"
        version = "1.2.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/a-1.2.0.tar.gz", hash = "sha256:ed135b48cf50c71e79d1d8e71438083d366c006d092a57ef9c4dcb678cb30021", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.2.0-py3-none-any.whl", hash = "sha256:7eb11ab3ef5d5ad9ea0a29983e094f23084e40f33bcc6d9c2cc897d17d6972d9", upload-time = "2024-03-24T00:00:00Z" },
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

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

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

        [options]
        exclude-newer = "2026-05-12T02:01:30Z"

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
        sdist = { url = "http://[LOCALHOST]/files/cleaver-1.0.0.tar.gz", hash = "sha256:7e1ca96a05eca2f4faf4f685bccfb5d693e81766aa3c892c923c20dd21d36203", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/cleaver-1.0.0-py3-none-any.whl", hash = "sha256:2f458e1663a8d73503c724c53b4c21aa1c2fa8684ea9df7ab8a58fce295fd3ab", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "fork-if-not-forked"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform != 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/fork_if_not_forked-2.0.0.tar.gz", hash = "sha256:be3c0974c9834f5f704edf7035d399ed65c4638a6cc8e283ecb3b1f8d3faba0b", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/fork_if_not_forked-2.0.0-py3-none-any.whl", hash = "sha256:2e7c62567ae2897f93bdcd9b83eddd95f0119cd7faead9f8135edda14c6ad372", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "fork-if-not-forked"
        version = "3.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/fork_if_not_forked-3.0.0.tar.gz", hash = "sha256:51063336c06410d759dae4fa2818a4bf78d215decb62eecb589691ee02c54be7", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/fork_if_not_forked-3.0.0-py3-none-any.whl", hash = "sha256:0122e6ab47685617c815378db984a67edc9ed09bf432ea49b1dbf70132c2a990", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "fork-if-not-forked-proxy"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "fork-if-not-forked", version = "2.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "sys_platform != 'linux'" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/fork_if_not_forked_proxy-1.0.0.tar.gz", hash = "sha256:b397bc5e48342cb658dc7a204a736fa536d9efa73d34b91b65de36be69afa168", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/fork_if_not_forked_proxy-1.0.0-py3-none-any.whl", hash = "sha256:f80880fedf339affe6c2257b166892d9665c7fc7e704618555fd850e10a85845", upload-time = "2024-03-24T00:00:00Z" },
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
        sdist = { url = "http://[LOCALHOST]/files/reject_cleaver1-1.0.0.tar.gz", hash = "sha256:8e011019adc72ffcaec0ac8facbd045efc0eea6fae9dee1186db1cfd45839c42", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/reject_cleaver1-1.0.0-py3-none-any.whl", hash = "sha256:a47acf9faa8b9f68b2abbc1bdbeda4ea6c41e8873f71237a593e5c5613d13cc6", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "reject-cleaver1"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform != 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/reject_cleaver1-2.0.0.tar.gz", hash = "sha256:243ed11a988d6563020c4cbf23878dc7a7631ce5258192158657102654f0ba03", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/reject_cleaver1-2.0.0-py3-none-any.whl", hash = "sha256:a73278908dbf0cb8cc969152f92aa93e109206280f26ff5eca498c7d27294e79", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "reject-cleaver1-proxy"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "reject-cleaver1", version = "2.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "sys_platform != 'linux'" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/reject_cleaver1_proxy-1.0.0.tar.gz", hash = "sha256:389d40f18b0c5a6f508f6132247522045524a9e364793c58e62b35d8872247f0", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/reject_cleaver1_proxy-1.0.0-py3-none-any.whl", hash = "sha256:12e0dfa551c8ac9e357da725292ed91c94c76dbe4dc9fc3cfc1d14f6de8398b9", upload-time = "2024-03-24T00:00:00Z" },
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

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

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

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

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

        [options]
        exclude-newer = "2026-05-12T02:01:30Z"

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
        sdist = { url = "http://[LOCALHOST]/files/bar-1.0.0.tar.gz", hash = "sha256:9d963b7f0e77107b5bee4314cfc6461a65b2b44dfe748fdb8449882f359950f9", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/bar-1.0.0-py3-none-any.whl", hash = "sha256:8124600da3e1d8019fa3e8664425d3f8b651094f840741d002060eaf3ce62c2d", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "bar"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/bar-2.0.0.tar.gz", hash = "sha256:8d6ec45c5353de7f61472245dc056c74aeadabe1babd35f686b82a3a5e6676ee", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/bar-2.0.0-py3-none-any.whl", hash = "sha256:30058fca0b5a4d570025278d8f2a7c2a05360e355e8b1b1f186fce304ae63696", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "c"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/c-2.0.0.tar.gz", hash = "sha256:16b76bba2ca5b44d7dc224071b6c24d754a8857a6a37dcb3128654c69eb91687", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/c-2.0.0-py3-none-any.whl", hash = "sha256:e4250088158b745edd41612deab90a2070681364aa5a79ea426e59766974902d", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "c"
        version = "3.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform != 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/c-3.0.0.tar.gz", hash = "sha256:997998faf55d711ef51b935b57aef0e99c9e7a240b6aa60595d67b0eb8f64d67", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/c-3.0.0-py3-none-any.whl", hash = "sha256:8deeaf14ff994d5f36cfc4e71db5a7a0eb8abb7e84e3d0ade0fb541cdd22e9c7", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "cleaver"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "bar", version = "1.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "sys_platform != 'linux'" },
            { name = "foo", marker = "sys_platform == 'linux'" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/cleaver-1.0.0.tar.gz", hash = "sha256:c4a61e89adabbe18f3fc81ee0af13de0677869d515ebf53245e6e22ce6284676", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/cleaver-1.0.0-py3-none-any.whl", hash = "sha256:6da7ff3295cdddefba66b34222f977d1242284e35583b01f465ea2b5d201d6ee", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "d"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "c", version = "3.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "sys_platform != 'linux'" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/d-1.0.0.tar.gz", hash = "sha256:dea3f863549ca57ef43822f937dcb016bad41cb29a2e125c7cd111c61e1d34b6", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/d-1.0.0-py3-none-any.whl", hash = "sha256:8cba05b5f73e2c65aa7bd8be9fb3d33f4e015ea5c9ec887085ed7298c49522a2", upload-time = "2024-03-24T00:00:00Z" },
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
        sdist = { url = "http://[LOCALHOST]/files/foo-1.0.0.tar.gz", hash = "sha256:7042e6462a8b9f2f5477528a424a5fe4c996ffa1aeaee956dae2a4fd3cccc919", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/foo-1.0.0-py3-none-any.whl", hash = "sha256:e87e67aac79b608391e81513c24922730b60f1a33845357eaa667a4b96416830", upload-time = "2024-03-24T00:00:00Z" },
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
        sdist = { url = "http://[LOCALHOST]/files/reject_cleaver_1-1.0.0.tar.gz", hash = "sha256:06bab8b7a04cec63b70f564beeb2a0bfcd166b581b95a5fb82d53d8de08c6096", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/reject_cleaver_1-1.0.0-py3-none-any.whl", hash = "sha256:1be085f4480cf7b6363e86ff688005e8d48518c352e2edf103645d03d2a4e82d", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "unrelated-dep2"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/unrelated_dep2-1.0.0.tar.gz", hash = "sha256:f528d951b0e6b4c7c98adee856573cc1262837de0b387f23d2b423385a600a23", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/unrelated_dep2-1.0.0-py3-none-any.whl", hash = "sha256:01845c6a8bca5e9dafa9e5bce923cf907635bfa684524bc54106a60870beb62d", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "unrelated-dep2"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform != 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/unrelated_dep2-2.0.0.tar.gz", hash = "sha256:3c0164bcbb708d6054242b3903dccad29796cef24a0c54b10e5e1296f9420268", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/unrelated_dep2-2.0.0-py3-none-any.whl", hash = "sha256:be978e3c036d1c85a9957def2f5a2b48e0d041e664180a8ab8c5a7a439a645d7", upload-time = "2024-03-24T00:00:00Z" },
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

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

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

        [options]
        exclude-newer = "2026-05-12T02:01:30Z"

        [[package]]
        name = "bar"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform != 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/bar-1.0.0.tar.gz", hash = "sha256:fd5f55be769a435833a85e1f5e7a9608105b13e0cb8c7fc6f2c1d119d8bf2a53", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/bar-1.0.0-py3-none-any.whl", hash = "sha256:abdad4f56ddd665e00054959ba415c5cb3ebca65792c6c14bb6b3480d812677e", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "bar"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/bar-2.0.0.tar.gz", hash = "sha256:8d6ec45c5353de7f61472245dc056c74aeadabe1babd35f686b82a3a5e6676ee", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/bar-2.0.0-py3-none-any.whl", hash = "sha256:30058fca0b5a4d570025278d8f2a7c2a05360e355e8b1b1f186fce304ae63696", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "cleaver"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "bar", version = "1.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "sys_platform != 'linux'" },
            { name = "foo", marker = "sys_platform == 'linux'" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/cleaver-1.0.0.tar.gz", hash = "sha256:c4a61e89adabbe18f3fc81ee0af13de0677869d515ebf53245e6e22ce6284676", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/cleaver-1.0.0-py3-none-any.whl", hash = "sha256:6da7ff3295cdddefba66b34222f977d1242284e35583b01f465ea2b5d201d6ee", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "foo"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/foo-1.0.0.tar.gz", hash = "sha256:f612ceff063b6467399e7b8740a5ef9c618fbb6e7756022ebaf519f949005a72", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/foo-1.0.0-py3-none-any.whl", hash = "sha256:05175a766bcf570f01876193b164fe18806e8e72bebd1e78145c7f3a72a91757", upload-time = "2024-03-24T00:00:00Z" },
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

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

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

        [options]
        exclude-newer = "2026-05-12T02:01:30Z"

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
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:9f54d704b5abd19c4374b8271a2bf77d8a0bae92160732feea8986b99dc96ff8", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-any.whl", hash = "sha256:65a435b18f9744a6f898e3a70e067524cc67e5be63f87ba5da4b7d35151e5c72", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "a"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "sys_platform == 'windows'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-2.0.0.tar.gz", hash = "sha256:220578d86869d9fd54c47cd9b4219c065bf712acd9fc0719b4473d17ba6729d2", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-2.0.0-py3-none-any.whl", hash = "sha256:6a62b0a0a71b6d01beeeb72e7fa7aa30a3a457f5ee0357b40bd66a64463dc3b4", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "os_name == 'darwin' and sys_platform == 'illumos'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/b-1.0.0.tar.gz", hash = "sha256:76826fb2fb2840bdd8f0b371f4e83e1281e71aab7cfad5cc946283fc2589e8dc", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-1.0.0-py3-none-any.whl", hash = "sha256:7bfefe7c9de97c4900f6a712427046f21236b96ac6081cae7701009038ea2b72", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "os_name == 'linux' and sys_platform == 'illumos'",
        ]
        sdist = { url = "http://[LOCALHOST]/files/b-2.0.0.tar.gz", hash = "sha256:0736c2aa1d22756aec5aec3a66de4527a9647ece471816db7b984b06e52dcba6", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-2.0.0-py3-none-any.whl", hash = "sha256:d0d9a8026b777021642e5d3ca9fec669eb63d9742a59a3d1872edab105b7cd81", upload-time = "2024-03-24T00:00:00Z" },
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

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

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

        [options]
        exclude-newer = "2026-05-12T02:01:30Z"

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

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

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

        [options]
        exclude-newer = "2026-05-12T02:01:30Z"

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

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

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

        [options]
        exclude-newer = "2026-05-12T02:01:30Z"

        [[package]]
        name = "a"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:24b2dc1e28c44878ee330bdf348498e82cfd1fbc59a8e16b602d0c30c9edc214", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-any.whl", hash = "sha256:c7433f6de78829d826bb6d78fcf1a756ea7a70117f2ed50f071a5022e8537eb3", upload-time = "2024-03-24T00:00:00Z" },
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

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

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

        [options]
        exclude-newer = "2026-05-12T02:01:30Z"

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

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

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

        [options]
        exclude-newer = "2026-05-12T02:01:30Z"

        [[package]]
        name = "a"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:24b2dc1e28c44878ee330bdf348498e82cfd1fbc59a8e16b602d0c30c9edc214", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-cp310-cp310-any.whl", hash = "sha256:25db0ad8ae40e499fc55390becdb4766c8874dd76a40b94829752e97aeece895", upload-time = "2024-03-24T00:00:00Z" },
            { url = "http://[LOCALHOST]/files/a-1.0.0-cp311-cp311-any.whl", hash = "sha256:8aa7d0ff72e12054f2b3077be5bd8606f1e2d92ed897d0ef1d846d1092a8675f", upload-time = "2024-03-24T00:00:00Z" },
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

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

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

        [options]
        exclude-newer = "2026-05-12T02:01:30Z"

        [[package]]
        name = "a"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:31d127ca76613d25b61a15ec0035a95160e9d879518c865ff2010b779db27590", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-any.whl", hash = "sha256:6c9fc4415658808b58b5f6d2ad7e07c242dc2a53f79b48a8b6834cdcbbb6c20d", upload-time = "2024-03-24T00:00:00Z" },
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

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

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

        [options]
        exclude-newer = "2026-05-12T02:01:30Z"

        [[package]]
        name = "a"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:4c45b53a975754849aa6da0c80ebf1db847a712e06ed36f6eebe0e46a0056fb1", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-cp312-cp312-win_amd64.whl", hash = "sha256:9bcce818092ca0b3f246067a55a79b8719b0481f0bf426f7ced7f9e82c6845cb", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/b-1.0.0.tar.gz", hash = "sha256:76826fb2fb2840bdd8f0b371f4e83e1281e71aab7cfad5cc946283fc2589e8dc", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-1.0.0-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:72edd68e9b19269d97ce4a25f7d1222f33787dc41b021cab357bf5654ca9e114", upload-time = "2024-03-24T00:00:00Z" },
            { url = "http://[LOCALHOST]/files/b-1.0.0-cp312-cp312-musllinux_1_1_armv7l.whl", hash = "sha256:ec7e01ee30a81c11b2c25ec8a9efd334ec0715ddf508bf177a66c36687294853", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "c"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/c-1.0.0.tar.gz", hash = "sha256:c992f835174aa9f6427c57d4f26403cfb15f48d523bb9e5492872c0c2481eb18", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/c-1.0.0-cp312-cp312-macosx_14_0_x86_64.whl", hash = "sha256:976fb70abc38668e73095c6c32ca2c149276299d61659fe881a4e2b9a9af61b8", upload-time = "2024-03-24T00:00:00Z" },
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

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

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

        [options]
        exclude-newer = "2026-05-12T02:01:30Z"

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
        sdist = { url = "http://[LOCALHOST]/files/psycopg-1.0.0.tar.gz", hash = "sha256:c30eb5b32bd6e1129a309fc4a05f07dc8fd713055e80150e3643449ec14e6bd3", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/psycopg-1.0.0-py3-none-any.whl", hash = "sha256:efe60773f06d1ebee2db4c7eb9bb8389da3f729ad0e532ac7ac534b5d2b6c5df", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [package.optional-dependencies]
        binary = [
            { name = "psycopg-binary", marker = "implementation_name != 'pypy' and platform_python_implementation != 'PyPy'" },
        ]

        [[package]]
        name = "psycopg-binary"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/psycopg_binary-1.0.0.tar.gz", hash = "sha256:8a11ba649ef8dafdfe521c129e162718f8c2cc3a58a5c86d5f333668ccddeb98", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/psycopg_binary-1.0.0-py3-none-any.whl", hash = "sha256:1154fe8109fbfca9717971080dbdd38d492b51b1bfb0d8916ad04ee89891b73c", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "tzdata"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/tzdata-1.0.0.tar.gz", hash = "sha256:2493b79f200e36a5acb42cf88d6d4a8e0a513061eec32465fe7f669e40161571", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/tzdata-1.0.0-py3-none-any.whl", hash = "sha256:6b2cdf2b72682be633655b994d1bf54f5ca5e6bc84225cb1cd84c4c295f3e715", upload-time = "2024-03-24T00:00:00Z" },
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

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

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

        [options]
        exclude-newer = "2026-05-12T02:01:30Z"

        [[package]]
        name = "a"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "b" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:45bb15b05a0bbef7470bd3d07a2353111ecd4b02a0342f97471b343f85d7a110", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-any.whl", hash = "sha256:5a9e088c6f905fc94a0d6b749ddb30181d7371b1cfd7b80552fb243db823fc06", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/b-1.0.0.tar.gz", hash = "sha256:76826fb2fb2840bdd8f0b371f4e83e1281e71aab7cfad5cc946283fc2589e8dc", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-1.0.0-py3-none-any.whl", hash = "sha256:7bfefe7c9de97c4900f6a712427046f21236b96ac6081cae7701009038ea2b72", upload-time = "2024-03-24T00:00:00Z" },
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

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

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

        [options]
        exclude-newer = "2026-05-12T02:01:30Z"

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
            { url = "http://[LOCALHOST]/files/win_only-1.0.0-cp312-abi3-win_amd64.whl", hash = "sha256:2e9abfa701593c82513445e9db7be9a74c36fb1686fdf438e95f45c0bc5e40ab", upload-time = "2024-03-24T00:00:00Z" },
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

    let mut filters = context.filters();
    // The "hint" about non-current environments is platform-dependent, so filter it out.
    filters.push((r"\n\s+hint: .*", ""));

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

        [options]
        exclude-newer = "2026-05-12T02:01:30Z"

        [[package]]
        name = "a"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        dependencies = [
            { name = "b", marker = "platform_machine == 'x86_64'" },
            { name = "c", marker = "platform_machine == 'aarch64'" },
            { name = "d", marker = "platform_machine == 'i686'" },
        ]
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:17dd14daa5ab49e46dea62513fb0eb5d0a16086464b527aab736b1fb7d9d1a17", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-any.whl", hash = "sha256:1aaf8f097e407e33211de4d23a13b391e538563a23d0e6e076bb4a40e8b79a9c", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "b"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        wheels = [
            { url = "http://[LOCALHOST]/files/b-1.0.0-cp313-cp313-freebsd_13_aarch64.whl", hash = "sha256:1f8d700bacf3f9c536a3339498ae8b38b049a8e1c3d488297ebbde14da69aee5", upload-time = "2024-03-24T00:00:00Z" },
            { url = "http://[LOCALHOST]/files/b-1.0.0-cp313-cp313-freebsd_13_x86_64.whl", hash = "sha256:1a23f527ed5cf188a28a8ed81c06422c295b9c9c1874c95af81f696e4af8f129", upload-time = "2024-03-24T00:00:00Z" },
            { url = "http://[LOCALHOST]/files/b-1.0.0-cp313-cp313-macosx_10_9_x86_64.whl", hash = "sha256:d9ba5c5956164fc35647c0f0fe250f3695fcef3d812203b37167fac3f50c4f00", upload-time = "2024-03-24T00:00:00Z" },
            { url = "http://[LOCALHOST]/files/b-1.0.0-cp313-cp313-manylinux2010_x86_64.whl", hash = "sha256:ffd5b0655ac095db862072a10e99e73b174a7053c1a190c292efb62d68db9fec", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "c"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        wheels = [
            { url = "http://[LOCALHOST]/files/c-1.0.0-cp313-cp313-freebsd_13_aarch64.whl", hash = "sha256:3e17c66b556c8a4731a66dd2af5a36ed435fd9d19a23f926daf24966f8d7c997", upload-time = "2024-03-24T00:00:00Z" },
            { url = "http://[LOCALHOST]/files/c-1.0.0-cp313-cp313-freebsd_13_x86_64.whl", hash = "sha256:8060ca87f19f1e77f1f9c37117f0a791587f98c044b00349103fea115112400e", upload-time = "2024-03-24T00:00:00Z" },
            { url = "http://[LOCALHOST]/files/c-1.0.0-cp313-cp313-macosx_10_9_arm64.whl", hash = "sha256:0bc5e89d4bd3e63d04e0ec3b6e8f3b71bf00b2006cb9dc445e4472e620553e34", upload-time = "2024-03-24T00:00:00Z" },
            { url = "http://[LOCALHOST]/files/c-1.0.0-cp313-cp313-manylinux2010_aarch64.whl", hash = "sha256:b9cf5cb287ea13e94927dc39c7c14740a120e11042ec925c0ca7caa76c7e3f17", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "d"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        wheels = [
            { url = "http://[LOCALHOST]/files/d-1.0.0-cp313-cp313-freebsd_13_aarch64.whl", hash = "sha256:34aac6a3d45cd4be2467b7ab53846c4d908695ec6d4d197d0ed4a97315713b94", upload-time = "2024-03-24T00:00:00Z" },
            { url = "http://[LOCALHOST]/files/d-1.0.0-cp313-cp313-freebsd_13_x86_64.whl", hash = "sha256:0a3d9d85f9e662a3aa9bc20cc63369eadd1f1cf7ed967c2925272be0b9a122c0", upload-time = "2024-03-24T00:00:00Z" },
            { url = "http://[LOCALHOST]/files/d-1.0.0-cp313-cp313-manylinux2010_i686.whl", hash = "sha256:0bbb3aff5fabc3babad3225ab67c34eb685cdfe8fcc0dd42100df091c2a1dbb6", upload-time = "2024-03-24T00:00:00Z" },
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
