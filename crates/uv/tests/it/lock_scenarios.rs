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

use crate::common::{TestContext, packse_index_url, uv_snapshot};

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
    let context = TestContext::new("3.12");

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
        requires-python = ">=3.12"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r"
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
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/wrong_backtracking_basic_a-1.0.0.tar.gz", hash = "sha256:b4abd2c802ca129d5855225fe456f2a36068c50d0ae545e37a8e08ef0f580b38" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/wrong_backtracking_basic_a-1.0.0-py3-none-any.whl", hash = "sha256:d669cb8614076ad7fc83f46b97abb94d86ada4ad5341d070874e96640ef808ad" },
        ]

        [[package]]
        name = "package-b"
        version = "2.0.9"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        dependencies = [
            { name = "package-a" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/wrong_backtracking_basic_b-2.0.9.tar.gz", hash = "sha256:aec746d9adae60458015ad7c11b1b9c589031928c07d9c13f1dff23d473b2480" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/wrong_backtracking_basic_b-2.0.9-py3-none-any.whl", hash = "sha256:30c0b2450c13c06d70ccb8804e41d3be9dacc911e27f30ac58b7880d8fe8e705" },
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
        .arg(packse_index_url())
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
    let context = TestContext::new("3.12");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"wrong-backtracking-indirect-", "package-"));

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''wrong-backtracking-indirect-a''',
          '''wrong-backtracking-indirect-b''',
        ]
        requires-python = ">=3.12"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r"
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
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/wrong_backtracking_indirect_a-2.0.0.tar.gz", hash = "sha256:8f28371115dab396e098ce46f514d6bdf15a42c81cc75aa78c675db61e1ed67e" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/wrong_backtracking_indirect_a-2.0.0-py3-none-any.whl", hash = "sha256:ea2a2e5008c2ca39195650f532c8ff6c129a91ca92018490fe53f9f0d323414e" },
        ]

        [[package]]
        name = "package-b"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        dependencies = [
            { name = "package-b-inner" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/wrong_backtracking_indirect_b-1.0.0.tar.gz", hash = "sha256:2463de4ba18fe6b1f03b8458724a399c85c98a354a1861ea02e485757f096e3b" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/wrong_backtracking_indirect_b-1.0.0-py3-none-any.whl", hash = "sha256:cf4fc24449def13876f05f0c8cae59e4104ab9694b58ad6e92cc3fe25ecea6b3" },
        ]

        [[package]]
        name = "package-b-inner"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        dependencies = [
            { name = "package-too-old" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/wrong_backtracking_indirect_b_inner-1.0.0.tar.gz", hash = "sha256:e1ddc7be17380b754483067727ad9fa4e40f2f9b837982e9b9124ee9425ad72e" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/wrong_backtracking_indirect_b_inner-1.0.0-py3-none-any.whl", hash = "sha256:c58bcad2e57e160ec81d3e7a057f2c9a1a5fb74a3e3d18d82d79ab0dc5ce85dd" },
        ]

        [[package]]
        name = "package-too-old"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/wrong_backtracking_indirect_too_old-1.0.0.tar.gz", hash = "sha256:de078b8acaad7f58f194407633aac7fda37550f5fbe778ecd837599ed3872a4d" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/wrong_backtracking_indirect_too_old-1.0.0-py3-none-any.whl", hash = "sha256:d6ddc9421418ce70582869cf38c4f0322fc4061be36edc501555a616d560e7ba" },
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
    let context = TestContext::new("3.12");

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
        requires-python = ">=3.12"
        "###
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r"
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
        name = "package-a"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_allows_non_conflicting_non_overlapping_dependencies_a-1.0.0.tar.gz", hash = "sha256:836b578e798d4aaba37ea42ebde338fd422d61d2bc4a93524b9c9cf77a7539d7" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_allows_non_conflicting_non_overlapping_dependencies_a-1.0.0-py3-none-any.whl", hash = "sha256:fce4343aac09c16fee45735fd638efc462aa97496c3e3332b6f8babdbd1e1e4d" },
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
        "#
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
    let context = TestContext::new("3.12");

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
        requires-python = ">=3.12"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r"
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
        name = "package-a"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_allows_non_conflicting_repeated_dependencies_a-1.0.0.tar.gz", hash = "sha256:4666c8498ab4aa641bacb39c2fb379ed87730d4de89bd7797c388a3c748f9f89" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_allows_non_conflicting_repeated_dependencies_a-1.0.0-py3-none-any.whl", hash = "sha256:77f0eb64e8c3bef8dec459d90c0394069eccb40d0c6c978d97bc1c4089d7d626" },
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
        "#
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
    let context = TestContext::new("3.12");

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
        requires-python = ">=3.12"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r"
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
        name = "package-a"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform == 'darwin'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_basic_a-1.0.0.tar.gz", hash = "sha256:a81cba8fd1453d8fdf35ba4b3d8c536d2f9fa4e2ceb6312f497ec608a5262663" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_basic_a-1.0.0-py3-none-any.whl", hash = "sha256:900f299c08b6e0c1ec013259c13ff9279f5672fb395418fd00f937a3830a7edb" },
        ]

        [[package]]
        name = "package-a"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_basic_a-2.0.0.tar.gz", hash = "sha256:3bbd7b86a9b7870ddcfdf343ed8555f414729053b171b3b072c1fef21478feb2" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_basic_a-2.0.0-py3-none-any.whl", hash = "sha256:4fa08d0429882d46c4c5262630b174b44aa633fc9d0c3f6a90e17406d6d7eb5a" },
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
        "#
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
    let context = TestContext::new("3.12");

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
          '''conflict-in-fork-a>=2; sys_platform == "os1"''',
          '''conflict-in-fork-a<2; sys_platform == "os2"''',
        ]
        requires-python = ">=3.12"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies for split (markers: sys_platform == 'os2'):
      ╰─▶ Because only package-b==1.0.0 is available and package-b==1.0.0 depends on package-d==1, we can conclude that all versions of package-b depend on package-d==1.
          And because package-c==1.0.0 depends on package-d==2 and only package-c==1.0.0 is available, we can conclude that all versions of package-b and all versions of package-c are incompatible.
          And because package-a==1.0.0 depends on package-b and package-c, we can conclude that package-a==1.0.0 cannot be used.
          And because only the following versions of package-a{sys_platform == 'os2'} are available:
              package-a{sys_platform == 'os2'}==1.0.0
              package-a{sys_platform == 'os2'}>2
          and your project depends on package-a{sys_platform == 'os2'}<2, we can conclude that your project's requirements are unsatisfiable.

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
    let context = TestContext::new("3.12");

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
        requires-python = ">=3.12"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because your project depends on package-a>=2 and package-a<2, we can conclude that your project's requirements are unsatisfiable.
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
    let context = TestContext::new("3.12");

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
        requires-python = ">=3.12"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r"
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
        name = "package-a"
        version = "4.3.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform == 'darwin'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_filter_sibling_dependencies_a-4.3.0.tar.gz", hash = "sha256:7bd9f28568add1b4f4ae3c75c527376d75cbb34a720b399c36422549fdb5a397" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_filter_sibling_dependencies_a-4.3.0-py3-none-any.whl", hash = "sha256:0cfa959f1188954c7426a84da27f310a7302c8814575ede013b6face9c71dd63" },
        ]

        [[package]]
        name = "package-a"
        version = "4.4.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_filter_sibling_dependencies_a-4.4.0.tar.gz", hash = "sha256:3ff647ba5d00e7efd6f81922fdc037af1b3d924820304b1dce5aa3e2bb0ebc17" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_filter_sibling_dependencies_a-4.4.0-py3-none-any.whl", hash = "sha256:996fd9369bfc2cd4538d5ef8ead9858574a4c692528874160515570a83986d82" },
        ]

        [[package]]
        name = "package-b"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        dependencies = [
            { name = "package-d", version = "1.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'linux'" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_filter_sibling_dependencies_b-1.0.0.tar.gz", hash = "sha256:5cf545d94aae6b0be2476c2bfe9412bea2a01df9dec426235528214faedc3c30" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_filter_sibling_dependencies_b-1.0.0-py3-none-any.whl", hash = "sha256:1e1027c7f771947bfc45d7ba1c45056af51677360f924f206f89deff41c3adba" },
        ]

        [[package]]
        name = "package-c"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        dependencies = [
            { name = "package-d", version = "2.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'darwin'" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_filter_sibling_dependencies_c-1.0.0.tar.gz", hash = "sha256:d1fead7e86d6a81678fa69cb5de6724d0cbfc196bdd6771a6d2029bc0a6dbafe" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_filter_sibling_dependencies_c-1.0.0-py3-none-any.whl", hash = "sha256:add817457a9772f2f81c36517b9a674017332bc634fefe0402810d87230d0609" },
        ]

        [[package]]
        name = "package-d"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_filter_sibling_dependencies_d-1.0.0.tar.gz", hash = "sha256:5bafd1cfed4b7bb07af597f0817c3e3b2dd178ab26e9d00e3ec119440dbe7867" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_filter_sibling_dependencies_d-1.0.0-py3-none-any.whl", hash = "sha256:2c07fc89a83f28ee8b8bfb73d5fd3578555c60c3645b6502ff89ce4a4ee47ff3" },
        ]

        [[package]]
        name = "package-d"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform == 'darwin'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_filter_sibling_dependencies_d-2.0.0.tar.gz", hash = "sha256:f6c80be71f98c3e0cd3d5a2a7d9be19a9651dd5657a29e5b0f4531d59a611a0b" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_filter_sibling_dependencies_d-2.0.0-py3-none-any.whl", hash = "sha256:b11cb3ef68f972b6b1ebf52d34310ea0e5962c7f618a3eb00db3ffc6c5c6d49f" },
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
        "#
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
    let context = TestContext::new("3.12");

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
        requires-python = ">=3.12"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r"
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
        name = "package-bar"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_upgrade_bar-2.0.0.tar.gz", hash = "sha256:ad9667b61721151a0ace7cd482b0486eef8f0e41d1317c4051e4305d9e5b7eba" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_upgrade_bar-2.0.0-py3-none-any.whl", hash = "sha256:be280a030a3094647684fcbf8d7d9ada41274adb3414b56e299c8df29ded92d0" },
        ]

        [[package]]
        name = "package-foo"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        dependencies = [
            { name = "package-bar" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_upgrade_foo-2.0.0.tar.gz", hash = "sha256:4f26ade9a4954b1bc419e19f130d47ac9a8a8c7ad7f446085938f29cfb6a7f30" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_upgrade_foo-2.0.0-py3-none-any.whl", hash = "sha256:af4548f32eb03890ad85d538acae65797fd339e2fef3b52538d629cfb46f191e" },
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
        "#
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
    let context = TestContext::new("3.12");

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
          '''fork-incomplete-markers-a==1; python_version < "3.13"''',
          '''fork-incomplete-markers-a==2; python_version >= "3.14"''',
          '''fork-incomplete-markers-b''',
        ]
        requires-python = ">=3.12"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r"
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
        name = "package-a"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "python_full_version < '3.13'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_incomplete_markers_a-1.0.0.tar.gz", hash = "sha256:47c0f25a9e21c68f14173c556ebc43656d4e3fa75a2802ff5cd8df98deaf965d" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_incomplete_markers_a-1.0.0-py3-none-any.whl", hash = "sha256:3cdba0dffc4e4fc3b01fbf5b8529b9f85d9811917984cd587d717ae84d853893" },
        ]

        [[package]]
        name = "package-a"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "python_full_version >= '3.14'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_incomplete_markers_a-2.0.0.tar.gz", hash = "sha256:e32ea9a40f05ad71fc666a4f3441020d9c50023b75b75c5d865a89437885831f" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_incomplete_markers_a-2.0.0-py3-none-any.whl", hash = "sha256:4b2a4ca84cae18d8c49ad87ec457a60a8fbe1d1160c61d2aeb473cf8c2b8d9e3" },
        ]

        [[package]]
        name = "package-b"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        dependencies = [
            { name = "package-c", marker = "python_full_version == '3.13.*'" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_incomplete_markers_b-1.0.0.tar.gz", hash = "sha256:99a13ea3286cafd8bdd4054fe0a966950574817d1973e30bcc230502b98f0be7" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_incomplete_markers_b-1.0.0-py3-none-any.whl", hash = "sha256:f429325c6d7cef0721bca12825f9f38ca781cd633ab305526ce9e2af6d43d68e" },
        ]

        [[package]]
        name = "package-c"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_incomplete_markers_c-1.0.0.tar.gz", hash = "sha256:e1a20988cf66fda67faf7009b002e807d457c38f32a7802772481c00bb734fb8" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_incomplete_markers_c-1.0.0-py3-none-any.whl", hash = "sha256:9d5e2b822bdae8076d9f5a5fa71307eff4d82e7ce9208dc247b1868cf3afe31a" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "package-a", version = "1.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "python_full_version < '3.13'" },
            { name = "package-a", version = "2.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "python_full_version >= '3.14'" },
            { name = "package-b" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "package-a", marker = "python_full_version < '3.13'", specifier = "==1" },
            { name = "package-a", marker = "python_full_version >= '3.14'", specifier = "==2" },
            { name = "package-b" },
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
    let context = TestContext::new("3.12");

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
        requires-python = ">=3.12"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r"
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
        name = "package-a"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        dependencies = [
            { name = "package-c", marker = "sys_platform == 'linux'" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_accrue_a-1.0.0.tar.gz", hash = "sha256:275ed50999adcfd2bbdf2ce621b7a4f850b3b314bc9e9f490d8e414cff19134d" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_accrue_a-1.0.0-py3-none-any.whl", hash = "sha256:7d83ea2071fee0d995d31ff7c7ff4db7e4ded2d7e5faa8143fce0296cf9a7a11" },
        ]

        [[package]]
        name = "package-b"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        dependencies = [
            { name = "package-c", marker = "sys_platform == 'darwin'" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_accrue_b-1.0.0.tar.gz", hash = "sha256:1944e0bfb15477a4dcace6a94fdb885a5e699c7c11c2ec81ee5f999a965d98ab" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_accrue_b-1.0.0-py3-none-any.whl", hash = "sha256:775f67358cd784b5d0a5880bc013beb34efbace88d0329fabfeb5093b7e7bbfa" },
        ]

        [[package]]
        name = "package-c"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_accrue_c-1.0.0.tar.gz", hash = "sha256:0d915ef7a76eed8e00c350dbe83fcff7aec77dba739e5a671cbfba69abca4d94" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_accrue_c-1.0.0-py3-none-any.whl", hash = "sha256:4a6d7a72793d13a666302e6dd2b9ac684c202f53c854c5eef55700067924a23b" },
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
        "#
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
    let context = TestContext::new("3.12");

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
        requires-python = ">=3.12"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because your project depends on package-a{sys_platform == 'linux'}>=2 and package-a{sys_platform == 'linux'}<2, we can conclude that your project's requirements are unsatisfiable.
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
    let context = TestContext::new("3.12");

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
        requires-python = ">=3.12"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r"
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
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_allowed_a-1.0.0.tar.gz", hash = "sha256:d6593d7102007515a301ed790fecf1b6366d8fe6c3bbc24a31497f91c44eaac6" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_allowed_a-1.0.0-py3-none-any.whl", hash = "sha256:887cde670bb39a789806e7d1a1b562d084ab163b3a3d08e20ec706b0d4453029" },
        ]

        [[package]]
        name = "package-a"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_allowed_a-2.0.0.tar.gz", hash = "sha256:d822588720e6f9627a002d7c2cf97a4a8d74e530265b0b288aef2b957327e4e9" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_allowed_a-2.0.0-py3-none-any.whl", hash = "sha256:88f87116434130aacff501c430f5413a8d3c2e38f18cd89d0504b37716ecc786" },
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
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_allowed_b-1.0.0.tar.gz", hash = "sha256:a1f33681f42aa5b47c7d05b93f013f8b9555b2ec2df49a42589dc06cab391797" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_allowed_b-1.0.0-py3-none-any.whl", hash = "sha256:da22041f6a3ec9d3a9e72fc4a3c7a9bb1b336b25bc48b5e15657866a12d34657" },
        ]

        [[package]]
        name = "package-b"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "implementation_name == 'cpython' and sys_platform == 'darwin'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_allowed_b-2.0.0.tar.gz", hash = "sha256:0509fdaeb0ad18de20daabb084aa27c198db538636d8b1c1fdfe65ed0943f7d2" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_allowed_b-2.0.0-py3-none-any.whl", hash = "sha256:030efe965f6e36cf4a01e3fd133ac427cee33da4d2b4703c3af560005efca454" },
        ]

        [[package]]
        name = "package-c"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_allowed_c-1.0.0.tar.gz", hash = "sha256:4a69f0cdfcd06caa73c2e16f11adfc3eeeac3c44210beb3aaf1dad4436351ceb" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_allowed_c-1.0.0-py3-none-any.whl", hash = "sha256:d936683675b39b426aad63f3047792f0c2609b6a459ab17d2d0b86d592be4c6e" },
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
        "#
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
    let context = TestContext::new("3.12");

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
        requires-python = ">=3.12"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r"
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
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_disallowed_a-1.0.0.tar.gz", hash = "sha256:c5f27540b38e11066006d693119bf351ce8eda80c6ad2b283c7db1b39ec1acbf" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_disallowed_a-1.0.0-py3-none-any.whl", hash = "sha256:7607e3377c7c4f0cb0e968bfe4e3ed1d3b8b19fceb42a48143e5ed138dca1346" },
        ]

        [[package]]
        name = "package-a"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_disallowed_a-2.0.0.tar.gz", hash = "sha256:1a98bb0369a890195060689cbad646c9c5aec9f2ead3b2340fc52cfbb252ead5" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_disallowed_a-2.0.0-py3-none-any.whl", hash = "sha256:f8db9a220f480a2655cc54efe47c603037a91935c95d3da48bc408810b3bc41e" },
        ]

        [[package]]
        name = "package-b"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "implementation_name == 'pypy' and sys_platform == 'darwin'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_disallowed_b-1.0.0.tar.gz", hash = "sha256:15005f7c07b199cb2d526cc45776bef8327a0d6e7a9cc773a318da76abafc046" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_disallowed_b-1.0.0-py3-none-any.whl", hash = "sha256:1ea9b95b6aa8ae09ff6711bc175c854d4caf4c3ad9ab69b1da3d6cec4a6cb7ed" },
        ]

        [[package]]
        name = "package-b"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "implementation_name == 'cpython' and sys_platform == 'darwin'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_disallowed_b-2.0.0.tar.gz", hash = "sha256:5ed91f43bd7013a8d4d1593a5aadc5a9e0d8629d5ee16f1357f89d24f20be2b9" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_disallowed_b-2.0.0-py3-none-any.whl", hash = "sha256:d4f998f9d1a9f8714347565237d2f1502d9699942c3cdd86cdb4ea7c5c7e5796" },
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
        "#
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
    let context = TestContext::new("3.12");

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
        requires-python = ">=3.12"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r"
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
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_a-1.0.0.tar.gz", hash = "sha256:7abc9661d5e25b05f7dd170fcda8f4abe0f6005768efd5f48b20be876e532954" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_a-1.0.0-py3-none-any.whl", hash = "sha256:d904b3e7317e89d18a9497d93cbf322b101ec1a08aeb3ef911666382e91b12d7" },
        ]

        [[package]]
        name = "package-a"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_a-2.0.0.tar.gz", hash = "sha256:413d4b8f065d7c36eb99760218d2a755242b2682d3976a7adfcc1dd2e2414e44" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_a-2.0.0-py3-none-any.whl", hash = "sha256:21d5b138fbb524ce43f65b2d0a9791e73e64ceb3d88f4be42f2a07bd41520412" },
        ]

        [[package]]
        name = "package-b"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "implementation_name == 'pypy' and sys_platform == 'darwin'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_b-1.0.0.tar.gz", hash = "sha256:1a52000f85375ddf93f7094d36d4c613af75b4cfc0638edb5be8ad4f984a74b2" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_b-1.0.0-py3-none-any.whl", hash = "sha256:62bc162290737672b8d017ad0c067298b7ad0ef9ada95ae37c35d0f9a738d8a4" },
        ]

        [[package]]
        name = "package-b"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "implementation_name == 'cpython' and sys_platform == 'darwin'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_b-2.0.0.tar.gz", hash = "sha256:bdeb3b79dc439acb4f5a76c8afd7fb5f5a1e80d89030ba1e0fd3514455505271" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_b-2.0.0-py3-none-any.whl", hash = "sha256:cf0d309bcf6092017ecf29fec37f1d0cc80fe59fee3f0488c52e944a913a3a8b" },
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
        "#
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
    let context = TestContext::new("3.12");

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
        requires-python = ">=3.12"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r"
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
        name = "package-a"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform == 'darwin'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_isolated_a-1.0.0.tar.gz", hash = "sha256:026f954ea486398d169b3bffbcc3619f261de760e30665db52dd4347c4c2051d" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_isolated_a-1.0.0-py3-none-any.whl", hash = "sha256:df72f591a1280e23989ae5b027b2c08b39b1a059458287cd5e3ac1c5f331119d" },
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
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_isolated_a-2.0.0.tar.gz", hash = "sha256:048dfffa5cc67b19eb5b4db562119293e1dd49e8c93395d242677216efbb2408" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_isolated_a-2.0.0-py3-none-any.whl", hash = "sha256:57a7c3e75553c34b1e269efb0c462f83eba34160f2c089b5ddab663eb2a95485" },
        ]

        [[package]]
        name = "package-b"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_isolated_b-1.0.0.tar.gz", hash = "sha256:7e7b0a264412f2f9563e0b0b9a003f0b22fe47d202f804f07e48c0fea29802e4" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_isolated_b-1.0.0-py3-none-any.whl", hash = "sha256:c8ff985d36ad17a8f0dd4b36805d02369cb4f9bdc64d794cfd42cbb1ffd98740" },
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
        "#
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
    let context = TestContext::new("3.12");

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
        requires-python = ">=3.12"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r"
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
        name = "package-a"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform == 'darwin'",
        ]
        dependencies = [
            { name = "package-b", marker = "sys_platform == 'darwin'" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_transitive_a-1.0.0.tar.gz", hash = "sha256:a6f10f68c6f39595be9e35bcb5bc47c8dff406417da2f949180c9acca5885d32" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_transitive_a-1.0.0-py3-none-any.whl", hash = "sha256:fc324cbddab19f021387526d0bfc0b63ba8d2994516a5f9cafabb97e93100c70" },
        ]

        [[package]]
        name = "package-a"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_transitive_a-2.0.0.tar.gz", hash = "sha256:50c12e6503e560beabfc9c6ffd594998d34a34ec27fc395115c239f52d61e582" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_transitive_a-2.0.0-py3-none-any.whl", hash = "sha256:26a501017aced48531ec9ed5148f95ab7322c5ceef9bdbf25bba6d950621e474" },
        ]

        [[package]]
        name = "package-b"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        dependencies = [
            { name = "package-c", marker = "sys_platform == 'darwin'" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_transitive_b-1.0.0.tar.gz", hash = "sha256:0a26e85593cef47acec83439527e58890ed1afb419b73bf6ccc344df4e1c2072" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_transitive_b-1.0.0-py3-none-any.whl", hash = "sha256:bc91cd833ffbc3f3dce3d74a5776a440907e8894ea8b2e106699374531f8dfc4" },
        ]

        [[package]]
        name = "package-c"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_transitive_c-1.0.0.tar.gz", hash = "sha256:4ad54ca5a23cdb1810d467603551ff254a3652b55f54f17ab991ab7205ad427e" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_transitive_c-1.0.0-py3-none-any.whl", hash = "sha256:4cd03a954fb6353f68a76930b79d9ec9aae9bf80ebe23b539434170b4d7819d9" },
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
        "#
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
    let context = TestContext::new("3.12");

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
        requires-python = ">=3.12"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r"
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
        name = "package-a"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform == 'darwin'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_a-1.0.0.tar.gz", hash = "sha256:6f045f31d58183fe437f35b34462e5ec9bf0bcee541f423999426aad7ccfe1b3" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_a-1.0.0-py3-none-any.whl", hash = "sha256:63bfd654bcd092e74a8ccae0f496e3b1062a5e9e2d98741f1548f337582c849b" },
        ]

        [[package]]
        name = "package-a"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_a-2.0.0.tar.gz", hash = "sha256:3222e12bb0136902b05479736e8731146ab3a01ebf0f666a6fe8c812e1181e33" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_a-2.0.0-py3-none-any.whl", hash = "sha256:72e2161338e159f2d6bed4cb519bc2ebf94e9daf2cbe5e0ac26d5fbe537f38af" },
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
        "#
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
    let context = TestContext::new("3.12");

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
        requires-python = ">=3.12"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r"
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
        name = "package-a"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform == 'darwin'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_limited_inherit_a-1.0.0.tar.gz", hash = "sha256:7bb040222692677419ec7074324c46ae68148a4c4feb95f73fab51f88605f4c3" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_limited_inherit_a-1.0.0-py3-none-any.whl", hash = "sha256:2d1c0ba50a12a630bab2870cffc57f27075cd1b6146208ec6ecbd128dc1c697e" },
        ]

        [[package]]
        name = "package-a"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_limited_inherit_a-2.0.0.tar.gz", hash = "sha256:23cb8783034c2c8fb7d9ce4f404241946dddde2c9e845d2ab19ee161d83d4dcb" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_limited_inherit_a-2.0.0-py3-none-any.whl", hash = "sha256:a15b853867a573c4c3ba8798c7cddac85c19c6f029a1926aa80f37519bb6ce7c" },
        ]

        [[package]]
        name = "package-b"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        dependencies = [
            { name = "package-c", marker = "sys_platform == 'linux'" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_limited_inherit_b-1.0.0.tar.gz", hash = "sha256:401047757cda70f63f0fd187361d225f34ebae0ed9dfc6c732c54005c58206f4" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_limited_inherit_b-1.0.0-py3-none-any.whl", hash = "sha256:6533104ddd85f6e10b501e76dd9b324eef8852edd412bfc29aad83a82a2310c3" },
        ]

        [[package]]
        name = "package-c"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_limited_inherit_c-1.0.0.tar.gz", hash = "sha256:3e8b40d054a5cf6d2f68be7750bd83cd305063f99025717df5def825f6236896" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_limited_inherit_c-1.0.0-py3-none-any.whl", hash = "sha256:17d86b6d2a25639cb06591901bb06353622b5c728e71562fd4c20c7a404fa5f6" },
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
        "#
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
    let context = TestContext::new("3.12");

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
        requires-python = ">=3.12"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r"
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
        name = "package-a"
        version = "0.1.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_selection_a-0.1.0.tar.gz", hash = "sha256:f8a3a4de6e08270cbf03e8d1e9d860dd56e57cc57529b8f622a3e6029b68d4e0" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_selection_a-0.1.0-py3-none-any.whl", hash = "sha256:6c1efed1a7f00b594202e9a811a0c2fc80cb54e62073c8d92d39c82fc745ba48" },
        ]

        [[package]]
        name = "package-b"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform == 'darwin'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_selection_b-1.0.0.tar.gz", hash = "sha256:13419956d96ad6b90ba779235ba6900fe084fbae8dcb288ca6749af98e537b58" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_selection_b-1.0.0-py3-none-any.whl", hash = "sha256:932c364daf4c721d3f592d8371b4d6408cf919c3560f114cba60a82fc566b397" },
        ]

        [[package]]
        name = "package-b"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_selection_b-2.0.0.tar.gz", hash = "sha256:2b1c59091de521af15778ccb7aa6a3718dd89730b215adb2a9094652e7f34133" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_selection_b-2.0.0-py3-none-any.whl", hash = "sha256:1d22dd5dc6026d5794bff9e6236968c8ffdfc7a6173926abdfd43b0591d947eb" },
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
        "#
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
    let context = TestContext::new("3.12");

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
        requires-python = ">=3.12"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r"
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
        name = "package-a"
        version = "1.3.1"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        dependencies = [
            { name = "package-c", marker = "implementation_name == 'iron'" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_track_a-1.3.1.tar.gz", hash = "sha256:6386d3023910b48db03db9fb28d75fcb8fa5a72a2ddc39de3fb6e8f15e9e1f8b" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_track_a-1.3.1-py3-none-any.whl", hash = "sha256:369b83e46c9331551548ed3e3a8643aa2b821f72478a4a30c88e88937d5c5e47" },
        ]

        [[package]]
        name = "package-b"
        version = "2.7"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform == 'darwin'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_track_b-2.7.tar.gz", hash = "sha256:8bd81fc189a302ec091eb05dac4f70510daf13fd82e10d8e0443c2d1d232748b" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_track_b-2.7-py3-none-any.whl", hash = "sha256:dd05dff9c80b7a425882b0809238784d44f021fd8b43f23c5d8ee24e1232532f" },
        ]

        [[package]]
        name = "package-b"
        version = "2.8"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_track_b-2.8.tar.gz", hash = "sha256:4908ff48ea9ff17deaa3e56fb78938fa14c2fef9cd7f20ece4e8424d6b0785b3" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_track_b-2.8-py3-none-any.whl", hash = "sha256:c9e3f25dd64091d3169a2ac13a22b2687eaebc482381a0d05dfbab74464ad395" },
        ]

        [[package]]
        name = "package-c"
        version = "1.10"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_track_c-1.10.tar.gz", hash = "sha256:24ac6f9d025c0e4750c7a215150b51397df744f674fb372a23d8ffe6b243a040" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_track_c-1.10-py3-none-any.whl", hash = "sha256:4e7104c302c1b05a2b1eae4abf4e27ba81866ec5af416e54562985e9495a22ad" },
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
        "#
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
    let context = TestContext::new("3.12");

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
        requires-python = ">=3.12"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r"
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
        name = "package-a"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        dependencies = [
            { name = "package-c", marker = "sys_platform == 'linux'" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_non_fork_marker_transitive_a-1.0.0.tar.gz", hash = "sha256:013eeda4d82bc86fad1a9f0b12e7dca4dda21f1588fdba1ac3345b2fa139c8c2" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_non_fork_marker_transitive_a-1.0.0-py3-none-any.whl", hash = "sha256:6150d8b54b7119a143b976472eac7c380cb130dd384ac02cffd7e6ce0cf82942" },
        ]

        [[package]]
        name = "package-b"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        dependencies = [
            { name = "package-c", marker = "sys_platform == 'darwin'" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_non_fork_marker_transitive_b-1.0.0.tar.gz", hash = "sha256:8257a5deb16f69d60e917f8817093640675bee4ab354a088b81b3e5969832e0c" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_non_fork_marker_transitive_b-1.0.0-py3-none-any.whl", hash = "sha256:e38a84fa43b292c50cf762656d4b2f4e57dea7fddf7be073146fb0870e7c0190" },
        ]

        [[package]]
        name = "package-c"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_non_fork_marker_transitive_c-2.0.0.tar.gz", hash = "sha256:44f2bc2e48b3b88bc4c4b64b7ef7df191c55a6fe0ea402d02dfc8132cec37c1f" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_non_fork_marker_transitive_c-2.0.0-py3-none-any.whl", hash = "sha256:9afd9106b0434c27efd84358f5a5a125950bac3a808535b4b2fa1b78ecbb766f" },
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
        "#
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
    let context = TestContext::new("3.12");

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
        requires-python = ">=3.12"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because package-a==1.0.0 depends on package-c<2.0.0 and package-b==1.0.0 depends on package-c>=2.0.0, we can conclude that package-a{sys_platform == 'linux'}==1.0.0 and package-b{sys_platform == 'darwin'}==1.0.0 are incompatible.
          And because your project depends on package-a{sys_platform == 'linux'}==1.0.0 and package-b{sys_platform == 'darwin'}==1.0.0, we can conclude that your project's requirements are unsatisfiable.
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
    let context = TestContext::new("3.12");

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
        requires-python = ">=3.12"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r"
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
    let context = TestContext::new("3.12");

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
          '''fork-overlapping-markers-basic-a>=1.0.0; python_version < "3.13"''',
          '''fork-overlapping-markers-basic-a>=1.1.0; python_version >= "3.13"''',
          '''fork-overlapping-markers-basic-a>=1.2.0; python_version >= "3.14"''',
        ]
        requires-python = ">=3.12"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r"
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
        name = "package-a"
        version = "1.2.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_overlapping_markers_basic_a-1.2.0.tar.gz", hash = "sha256:bc36ec5060fa24617aa2b40a77b62e9a80737aeec255fa3e005933cdfd81ad98" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_overlapping_markers_basic_a-1.2.0-py3-none-any.whl", hash = "sha256:3afae4a2c2a114de372c165e5b874dbb52bc41aa7ebee2b519820e89c8ccbeec" },
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
            { name = "package-a", marker = "python_full_version < '3.13'", specifier = ">=1.0.0" },
            { name = "package-a", marker = "python_full_version >= '3.13'", specifier = ">=1.1.0" },
            { name = "package-a", marker = "python_full_version >= '3.14'", specifier = ">=1.2.0" },
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
    let context = TestContext::new("3.12");

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
        requires-python = ">=3.12"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r"
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
        name = "package-cleaver"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        dependencies = [
            { name = "package-fork-if-not-forked", version = "3.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'linux'" },
            { name = "package-fork-if-not-forked-proxy", marker = "sys_platform != 'linux'" },
            { name = "package-reject-cleaver1", version = "1.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'linux'" },
            { name = "package-reject-cleaver1-proxy" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_bistable_cleaver-1.0.0.tar.gz", hash = "sha256:d2279d73d7ffff655e269c3bcea9674895dd55631d542ed12fab64a4bbd9fc29" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_bistable_cleaver-1.0.0-py3-none-any.whl", hash = "sha256:6ab14dc6b3a9658f2a222b752773beaf43ac5379fba0a88bb3012282936328b0" },
        ]

        [[package]]
        name = "package-fork-if-not-forked"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform != 'linux'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_bistable_fork_if_not_forked-2.0.0.tar.gz", hash = "sha256:a79abb4beae739e1413afe652bcda59e3807253e46907729dab211954be06a2c" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_bistable_fork_if_not_forked-2.0.0-py3-none-any.whl", hash = "sha256:1c580daeffeba5529c44ae1afb90b858ea7c696157c1c72ba59de0f32f61dd26" },
        ]

        [[package]]
        name = "package-fork-if-not-forked"
        version = "3.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_bistable_fork_if_not_forked-3.0.0.tar.gz", hash = "sha256:4d45258ebd32cd819906e69f2e2b890044d6e99b24b8e82ebc88aba6fe607b1a" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_bistable_fork_if_not_forked-3.0.0-py3-none-any.whl", hash = "sha256:b246c161c65bcf1387dbbf5458bb4841076818c2838babd1d5d274b436eda26e" },
        ]

        [[package]]
        name = "package-fork-if-not-forked-proxy"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        dependencies = [
            { name = "package-fork-if-not-forked", version = "2.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform != 'linux'" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_bistable_fork_if_not_forked_proxy-1.0.0.tar.gz", hash = "sha256:80e40c848eed5472237156444ccaf72ddb0176cd095f2b69469fce203981a14e" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_bistable_fork_if_not_forked_proxy-1.0.0-py3-none-any.whl", hash = "sha256:38cf97a4a1a1bd748744ec3859023c9e1173171525cdec8f8a48e8e5adf4422c" },
        ]

        [[package]]
        name = "package-reject-cleaver1"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_bistable_reject_cleaver1-1.0.0.tar.gz", hash = "sha256:42426a00cb3e8c7994faf906ebcf102e0cbf7e68eabd8ada77562489ae185a3d" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_bistable_reject_cleaver1-1.0.0-py3-none-any.whl", hash = "sha256:7468472d90e0004b177841ffbc5997fca3fa3f593607a8c90ffb013aa7681768" },
        ]

        [[package]]
        name = "package-reject-cleaver1"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform != 'linux'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_bistable_reject_cleaver1-2.0.0.tar.gz", hash = "sha256:1f7c625af40a3dcd86ca2765f496a3423cd0ee5162bf136bf7b73d956ae6715f" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_bistable_reject_cleaver1-2.0.0-py3-none-any.whl", hash = "sha256:695334dd35d3cb9045e111a4fda566d982558fef7a74026138941d869b97c014" },
        ]

        [[package]]
        name = "package-reject-cleaver1-proxy"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        dependencies = [
            { name = "package-reject-cleaver1", version = "2.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform != 'linux'" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_bistable_reject_cleaver1_proxy-1.0.0.tar.gz", hash = "sha256:cbdc4afe283a0b8c556015b20fa5818c9be23f769cfea0b1f3c8ce138af064f9" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_bistable_reject_cleaver1_proxy-1.0.0-py3-none-any.whl", hash = "sha256:74b2c5db1d5841444d89d8238b7a89b082327cf7d51fa674349544ad0f67073b" },
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
        "#
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
    let context = TestContext::new("3.12");

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
        requires-python = ">=3.12"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r"
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
    let context = TestContext::new("3.12");

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
        requires-python = ">=3.12"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r"
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
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_tristable_bar-1.0.0.tar.gz", hash = "sha256:cab958c70ac0b91cf9db6d44846ee4aff97cdedaf1eaf96400657177eb8cf7d2" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_tristable_bar-1.0.0-py3-none-any.whl", hash = "sha256:b81a56870b304c579d5ce5c1e4d0ae1ee60f109d50c739ead96ff4775ea1fb6b" },
        ]

        [[package]]
        name = "package-bar"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_tristable_bar-2.0.0.tar.gz", hash = "sha256:73f911717b729830afd19bdd8f8fee3562a8b03e8fc8f3a734becf9b15bcb9a4" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_tristable_bar-2.0.0-py3-none-any.whl", hash = "sha256:25f44f6a64ef2a5b0e315a7ddf6470931c7c9c3acca67b477a6130abddea1523" },
        ]

        [[package]]
        name = "package-c"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_tristable_c-2.0.0.tar.gz", hash = "sha256:ac8b3503e17933efe9be91ce969d2fd6f95d3175d5aa8f4033a1abd2baa5b4de" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_tristable_c-2.0.0-py3-none-any.whl", hash = "sha256:d04193e40e9415b4239a8709518dc780dc6ed4475124c4a9f0c98574dcbf7569" },
        ]

        [[package]]
        name = "package-c"
        version = "3.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform != 'linux'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_tristable_c-3.0.0.tar.gz", hash = "sha256:5932c8648a79247c23756a841f5bc6ee94018cf5ec0228ae0049f9de6035262e" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_tristable_c-3.0.0-py3-none-any.whl", hash = "sha256:4a0d4549221dc761782115280a678c5ffbf1f93b8c78a5a935a197298b6dc625" },
        ]

        [[package]]
        name = "package-cleaver"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        dependencies = [
            { name = "package-bar", version = "1.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform != 'linux'" },
            { name = "package-foo", marker = "sys_platform == 'linux'" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_tristable_cleaver-1.0.0.tar.gz", hash = "sha256:6c1a467374165e60c80b639fc4ca4830dd66796fc1a363f78b2c5b0756838909" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_tristable_cleaver-1.0.0-py3-none-any.whl", hash = "sha256:1291f62c2630beb9e7a07461e7a79cf662934e4ca2fa80da0f6e7e02bb84a7d6" },
        ]

        [[package]]
        name = "package-d"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        dependencies = [
            { name = "package-c", version = "3.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform != 'linux'" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_tristable_d-1.0.0.tar.gz", hash = "sha256:0662b34dc84d1529fd1f75f69c0802a5ac768611a3a11ba331af81e0e67f96a2" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_tristable_d-1.0.0-py3-none-any.whl", hash = "sha256:7917517d7d735efd5b2559223d95c5e776963b25c5cb174e2d64199898aa6adf" },
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
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_tristable_foo-1.0.0.tar.gz", hash = "sha256:3ab8216595ed4d66e27f853cbbfc61142cc06c1a1205c9477581c6b1463a6245" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_tristable_foo-1.0.0-py3-none-any.whl", hash = "sha256:3a42e5594ebefa939a4a366419cd335e9e52942798b6f5ab6e37b9b55e6474b8" },
        ]

        [[package]]
        name = "package-reject-cleaver-1"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        dependencies = [
            { name = "package-unrelated-dep2", version = "1.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'linux'" },
            { name = "package-unrelated-dep2", version = "2.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform != 'linux'" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_tristable_reject_cleaver_1-1.0.0.tar.gz", hash = "sha256:5fc04a8d0cf1eaa98da779661bf9328b04ba771bbb9261e7784b025a40c8a133" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_tristable_reject_cleaver_1-1.0.0-py3-none-any.whl", hash = "sha256:979e44471666eba857933319c56c678ac393c72081b13f67f1f41b47f558ccf9" },
        ]

        [[package]]
        name = "package-unrelated-dep2"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_tristable_unrelated_dep2-1.0.0.tar.gz", hash = "sha256:83fe5a7e81958bd422fb5cfde2c60537b97228762eada43a658e621e64a0780d" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_tristable_unrelated_dep2-1.0.0-py3-none-any.whl", hash = "sha256:461e8e88feafdaad02a4a79b9f1ad874333ee92131ca83030190c48b96dbcfc4" },
        ]

        [[package]]
        name = "package-unrelated-dep2"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform != 'linux'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_tristable_unrelated_dep2-2.0.0.tar.gz", hash = "sha256:0b491eba7b182cb80955e8592d144f20b57bcfeaf7852c568a03fdd6b0dfd752" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_tristable_unrelated_dep2-2.0.0-py3-none-any.whl", hash = "sha256:c3aedfba6ce432d4d79bc2c304f8bc493a562e0fdc963eed9253f4eb8e8bdb8e" },
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
        "#
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
    let context = TestContext::new("3.12");

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
        requires-python = ">=3.12"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r"
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
        name = "package-bar"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform != 'linux'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_bar-1.0.0.tar.gz", hash = "sha256:78ac06984ed655965d557096cd218087641602f523eb74dc241e3237c7d6129b" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_bar-1.0.0-py3-none-any.whl", hash = "sha256:bf0ab2b019e425ef38d7c5632551fefd42aaa8ab96357ab2d3ce0124dfdfe892" },
        ]

        [[package]]
        name = "package-bar"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_bar-2.0.0.tar.gz", hash = "sha256:70831af376e906171eb5905b0a163f92418e1398400b6ad749b04c7ba27102e1" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_bar-2.0.0-py3-none-any.whl", hash = "sha256:fc188f8e81088843447d1863c1098bdeef1d051040b8caaf94b0421fa331d827" },
        ]

        [[package]]
        name = "package-cleaver"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        dependencies = [
            { name = "package-bar", version = "1.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform != 'linux'" },
            { name = "package-foo", marker = "sys_platform == 'linux'" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_cleaver-1.0.0.tar.gz", hash = "sha256:380812f7558b9df90347bf44e47a240e4a5ff5372628c00b3c467e95858b0e47" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_cleaver-1.0.0-py3-none-any.whl", hash = "sha256:1873c290ddafcd254f74bc4d80000b094159212141172ea24f7565dcd3bbcfe2" },
        ]

        [[package]]
        name = "package-foo"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_foo-1.0.0.tar.gz", hash = "sha256:477ad43616170d9bfec354f737d7025c5e92fce2faa0fdbf25d4c3c077a5df54" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_foo-1.0.0-py3-none-any.whl", hash = "sha256:8a40eef897548e3434cab40ccdfe88648dced3bbe4f2ca40404e5dbb57d8025b" },
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
        "#
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
    let context = TestContext::new("3.12");

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
        requires-python = ">=3.12"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r"
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
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_remaining_universe_partitioning_a-1.0.0.tar.gz", hash = "sha256:8265b2f0d92cd4d63f303ca1a81c0920b9bcb9026434388b5c41847772ea2e41" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_remaining_universe_partitioning_a-1.0.0-py3-none-any.whl", hash = "sha256:2964220ab58b53d9e84ecb9f384d7284e914280779e0157ea5f1c1f4375a67b0" },
        ]

        [[package]]
        name = "package-a"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "sys_platform == 'windows'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_remaining_universe_partitioning_a-2.0.0.tar.gz", hash = "sha256:a06312972c1436362b2af1366918160dd775f046a2a6991d7ec6f3ae17e2da4e" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_remaining_universe_partitioning_a-2.0.0-py3-none-any.whl", hash = "sha256:4e4f4e2dd97e69715d430c227bdaa8635a46b4eac5d26b9d48edf6ea21c2ea9c" },
        ]

        [[package]]
        name = "package-b"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "os_name == 'darwin' and sys_platform == 'illumos'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_remaining_universe_partitioning_b-1.0.0.tar.gz", hash = "sha256:a41c05053e9b6c47c7d1e66a7b0ae74542b40008f906c0bca8e65ddbb124c8f1" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_remaining_universe_partitioning_b-1.0.0-py3-none-any.whl", hash = "sha256:60c8698c2721d7abba70175c61d206f47a77729268735740294778b6fd1913e4" },
        ]

        [[package]]
        name = "package-b"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        resolution-markers = [
            "os_name == 'linux' and sys_platform == 'illumos'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_remaining_universe_partitioning_b-2.0.0.tar.gz", hash = "sha256:043c931dd9a88b360ec57b3bc6753b5df22f0e844b7589f1e42afd84c9de9d76" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_remaining_universe_partitioning_b-2.0.0-py3-none-any.whl", hash = "sha256:2bea93dc4ccddac32a3f3ab47df20c9f39a6fb7c34a2916d4df6397442202ba9" },
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
        "#
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
    uv_snapshot!(filters, cmd, @r"
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
        requires-dist = [{ name = "package-a", marker = "python_full_version == '3.9'", specifier = "==1.0.0" }]
        "#
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
    uv_snapshot!(filters, cmd, @r"
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
        requires-dist = [{ name = "package-a", marker = "python_full_version == '3.9'", specifier = "==1.0.0" }]
        "#
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
///         └── requires python>=3.10
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
    uv_snapshot!(filters, cmd, @r"
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
        name = "package-a"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_requires_python_patch_overlap_a-1.0.0.tar.gz", hash = "sha256:98210d2d45238045c16262c38a2211b3239e1ea164dd6061292f6012e5c85d1e" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_requires_python_patch_overlap_a-1.0.0-py3-none-any.whl", hash = "sha256:a6a97b0e0d0b176aa3942b7f58c8898a32e7050ada4349f6105922d3e2db3924" },
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
        "#
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
    uv_snapshot!(filters, cmd, @r"
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
        requires-dist = [{ name = "package-a", marker = "python_full_version == '3.9.*'", specifier = "==1.0.0" }]
        "#
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
///         └── requires python>=3.10
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
    uv_snapshot!(filters, cmd, @r"
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
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/requires_python_wheels_a-1.0.0.tar.gz", hash = "sha256:a47ad4fb0ebd76fcde3bf4aabf5d9393039f76822d990858e23ad17cd23dc67a" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/requires_python_wheels_a-1.0.0-cp310-cp310-any.whl", hash = "sha256:38109d4f29bd910e11cf3ea462fe06e740a346e76d1afa95fda2228845725de5" },
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/requires_python_wheels_a-1.0.0-cp311-cp311-any.whl", hash = "sha256:38109d4f29bd910e11cf3ea462fe06e740a346e76d1afa95fda2228845725de5" },
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
    let context = TestContext::new("3.12");

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
        requires-python = ">=3.12"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r"
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
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/unreachable_package_a-1.0.0.tar.gz", hash = "sha256:4ceb6be97b8f4526768c02f4a270cf270e225361eb42d1e87426b164be82f383" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/unreachable_package_a-1.0.0-py3-none-any.whl", hash = "sha256:a61dae53684bd2dbc1bd96b375b55e0c2f445b47d4c601b061d3fe1138fd805d" },
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
    let context = TestContext::new("3.12");

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
        requires-python = ">=3.12"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r"
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
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/unreachable_wheels_a-1.0.0.tar.gz", hash = "sha256:7af67216432f3484632f08d42817a4425b2f7e6285d066b857b1b63479683173" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/unreachable_wheels_a-1.0.0-cp312-cp312-win_amd64.whl", hash = "sha256:fb0f200722746083492595dd95b5774f8807e632fa290b5615e66e921267cfd2" },
        ]

        [[package]]
        name = "package-b"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/unreachable_wheels_b-1.0.0.tar.gz", hash = "sha256:31943f973b8114889e4acd42add16396be474638db534604e6ca0a5d88e27ee5" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/unreachable_wheels_b-1.0.0-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:44108443dc9bf6bfb387875df0ee4ea793832d91417097586a0b90cc261da199" },
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/unreachable_wheels_b-1.0.0-cp312-cp312-musllinux_1_1_armv7l.whl", hash = "sha256:44108443dc9bf6bfb387875df0ee4ea793832d91417097586a0b90cc261da199" },
        ]

        [[package]]
        name = "package-c"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/unreachable_wheels_c-1.0.0.tar.gz", hash = "sha256:019230145b8dc90c687cb360077ffcd470243795480fd145d25b2a4af2b71858" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/unreachable_wheels_c-1.0.0-cp312-cp312-macosx_14_0_x86_64.whl", hash = "sha256:17f36f8a58561cf654235bbfecb9d4390bacc62d666ebcf4c93d94b750eb425e" },
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
        .arg(packse_index_url())
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
    let context = TestContext::new("3.12");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"marker-variants-have-different-extras-", "package-"));

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''marker-variants-have-different-extras-psycopg[binary]; platform_python_implementation != "PyPy"''',
          '''marker-variants-have-different-extras-psycopg; platform_python_implementation == "PyPy"''',
        ]
        requires-python = ">=3.12"
        "###
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r"
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
        name = "package-psycopg"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        dependencies = [
            { name = "package-tzdata", marker = "sys_platform == 'win32'" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/marker_variants_have_different_extras_psycopg-1.0.0.tar.gz", hash = "sha256:73bfd3a8cd320336ded5c7cb2219544b8b66819c784cb88634fc2d716b42654b" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/marker_variants_have_different_extras_psycopg-1.0.0-py3-none-any.whl", hash = "sha256:29661efd1d691398d6fc0c1cca3618d795ce64ef50f4b152ff9cff471582afe1" },
        ]

        [package.optional-dependencies]
        binary = [
            { name = "package-psycopg-binary", marker = "implementation_name != 'pypy' and platform_python_implementation != 'PyPy'" },
        ]

        [[package]]
        name = "package-psycopg-binary"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/marker_variants_have_different_extras_psycopg_binary-1.0.0.tar.gz", hash = "sha256:988780eb0dc092cd5af36bef12d5c42404ee4dd73e6c29c842cc802a558d355e" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/marker_variants_have_different_extras_psycopg_binary-1.0.0-py3-none-any.whl", hash = "sha256:8f07333ec83cc11023198e50ec91314afbe96e3b905183ec060e60388d18eab4" },
        ]

        [[package]]
        name = "package-tzdata"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/marker_variants_have_different_extras_tzdata-1.0.0.tar.gz", hash = "sha256:05613cdd40338ffb46f0c94efa618d5b64b4879040c520b2934b982772220f47" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/marker_variants_have_different_extras_tzdata-1.0.0-py3-none-any.whl", hash = "sha256:8d59a2ae73c64014e27574eae496a70d5619aca1ddcb04f5a589de97850b13a8" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "package-psycopg" },
            { name = "package-psycopg", extra = ["binary"], marker = "platform_python_implementation != 'PyPy'" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "package-psycopg", marker = "platform_python_implementation == 'PyPy'" },
            { name = "package-psycopg", extras = ["binary"], marker = "platform_python_implementation != 'PyPy'" },
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
        .arg(packse_index_url())
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
    let context = TestContext::new("3.12");

    // In addition to the standard filters, swap out package names for shorter messages
    let mut filters = context.filters();
    filters.push((r"virtual-package-extra-priorities-", "package-"));

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r###"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
          '''virtual-package-extra-priorities-a==1; python_version >= "3.8"''',
          '''virtual-package-extra-priorities-b; python_version >= "3.9"''',
        ]
        requires-python = ">=3.12"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r"
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
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "package-a" },
            { name = "package-b" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "package-a", marker = "python_full_version >= '3.8'", specifier = "==1" },
            { name = "package-b", marker = "python_full_version >= '3.9'" },
        ]

        [[package]]
        name = "package-a"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        dependencies = [
            { name = "package-b" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/virtual_package_extra_priorities_a-1.0.0.tar.gz", hash = "sha256:df3b975e935948c0e3ce80413fabacc7473b3b77fa6143c2d50134ca9821b982" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/virtual_package_extra_priorities_a-1.0.0-py3-none-any.whl", hash = "sha256:7d8ceba929775dd3294275637a76c1556b816705f86d9d913b92df708bbe09bc" },
        ]

        [[package]]
        name = "package-b"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/virtual_package_extra_priorities_b-1.0.0.tar.gz", hash = "sha256:1871d4ccdcae672dbeda7e8c9bd16c28e3256e343470d1ae9f8bd2946ba7ab9f" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/virtual_package_extra_priorities_b-1.0.0-py3-none-any.whl", hash = "sha256:d92888ac830ae8768749436fec5133b9262667c2c743285f941b18bb13724222" },
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
    let context = TestContext::new("3.12");

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
        requires-python = ">=3.12"
        "###,
    )?;

    let mut cmd = context.lock();
    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r"
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
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/specific_architecture_a-1.0.0.tar.gz", hash = "sha256:d2ab13af91c3f80269717ccc9d71dae2b0cf59b0b53b7ce5e1687c3c12530518" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/specific_architecture_a-1.0.0-py3-none-any.whl", hash = "sha256:55436c2630725abac18f06b5f2255ed32c447b8b6e8ef6abfe533259028abb82" },
        ]

        [[package]]
        name = "package-b"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/specific_architecture_b-1.0.0-cp313-cp313-freebsd_13_aarch64.whl", hash = "sha256:e494f9d058ab90fc641c4390bbd81164210daf0d1811672d187636653496f77d" },
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/specific_architecture_b-1.0.0-cp313-cp313-freebsd_13_x86_64.whl", hash = "sha256:e494f9d058ab90fc641c4390bbd81164210daf0d1811672d187636653496f77d" },
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/specific_architecture_b-1.0.0-cp313-cp313-macosx_10_9_x86_64.whl", hash = "sha256:e494f9d058ab90fc641c4390bbd81164210daf0d1811672d187636653496f77d" },
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/specific_architecture_b-1.0.0-cp313-cp313-manylinux2010_x86_64.whl", hash = "sha256:e494f9d058ab90fc641c4390bbd81164210daf0d1811672d187636653496f77d" },
        ]

        [[package]]
        name = "package-c"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/specific_architecture_c-1.0.0-cp313-cp313-freebsd_13_aarch64.whl", hash = "sha256:64ebbefd7b78db539d4884fc7f2d9df24bd253511cb67350ac6b610291cd8405" },
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/specific_architecture_c-1.0.0-cp313-cp313-freebsd_13_x86_64.whl", hash = "sha256:64ebbefd7b78db539d4884fc7f2d9df24bd253511cb67350ac6b610291cd8405" },
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/specific_architecture_c-1.0.0-cp313-cp313-macosx_10_9_arm64.whl", hash = "sha256:64ebbefd7b78db539d4884fc7f2d9df24bd253511cb67350ac6b610291cd8405" },
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/specific_architecture_c-1.0.0-cp313-cp313-manylinux2010_aarch64.whl", hash = "sha256:64ebbefd7b78db539d4884fc7f2d9df24bd253511cb67350ac6b610291cd8405" },
        ]

        [[package]]
        name = "package-d"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/specific_architecture_d-1.0.0-cp313-cp313-freebsd_13_aarch64.whl", hash = "sha256:f49cf7bda1ea7366cd0cae8b9f0d8975f7b29ae3569b2aa70fe2d5eb5176cc64" },
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/specific_architecture_d-1.0.0-cp313-cp313-freebsd_13_x86_64.whl", hash = "sha256:f49cf7bda1ea7366cd0cae8b9f0d8975f7b29ae3569b2aa70fe2d5eb5176cc64" },
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/specific_architecture_d-1.0.0-cp313-cp313-manylinux2010_i686.whl", hash = "sha256:f49cf7bda1ea7366cd0cae8b9f0d8975f7b29ae3569b2aa70fe2d5eb5176cc64" },
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
        .arg(packse_index_url())
        .assert()
        .success();

    Ok(())
}
