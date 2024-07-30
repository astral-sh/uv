//! DO NOT EDIT
//!
//! Generated with `./scripts/sync_scenarios.sh`
//! Scenarios from <https://github.com/astral-sh/packse/tree/0.3.31/scenarios>
//!
#![cfg(all(feature = "python", feature = "pypi"))]
#![allow(clippy::needless_raw_string_hashes)]

use anyhow::Result;
use assert_fs::prelude::*;
use insta::assert_snapshot;

use common::{packse_index_url, uv_snapshot, TestContext};

mod common;

/// This test ensures that multiple non-conflicting but also non-overlapping
/// dependency specifications with the same package name are allowed and supported.
/// At time of writing, this provokes a fork in the resolver, but it arguably
/// shouldn't since the requirements themselves do not conflict with one another.
/// However, this does impact resolution. Namely, it leaves the `a>=1` fork free to
/// choose `a==2.0.0` since it behaves as if the `a<2` constraint doesn't exist.
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
    cmd.env_remove("UV_EXCLUDE_NEWER");
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning
    Resolved 2 packages in [TIME]
    "###
    );

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.8"
        environment-markers = [
            "sys_platform == 'darwin'",
            "sys_platform == 'linux'",
            "sys_platform != 'darwin' and sys_platform != 'linux'",
        ]

        [[distribution]]
        name = "package-a"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_allows_non_conflicting_non_overlapping_dependencies_a-1.0.0.tar.gz#sha256=dd40a6bd59fbeefbf9f4936aec3df6fb6017e57d334f85f482ae5dd03ae353b9", hash = "sha256:dd40a6bd59fbeefbf9f4936aec3df6fb6017e57d334f85f482ae5dd03ae353b9" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_allows_non_conflicting_non_overlapping_dependencies_a-1.0.0-py3-none-any.whl#sha256=8111e996c2a4e04c7a7cf91cf6f8338f5195c22ecf2303d899c4ef4e718a8175", hash = "sha256:8111e996c2a4e04c7a7cf91cf6f8338f5195c22ecf2303d899c4ef4e718a8175" },
        ]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "package-a", marker = "sys_platform == 'darwin' or sys_platform == 'linux'" },
        ]
        "###
        );
    });

    Ok(())
}

/// This test ensures that multiple non-conflicting dependency specifications with
/// the same package name are allowed and supported.  This test exists because the
/// universal resolver forks itself based on duplicate dependency specifications by
/// looking at package name. So at first glance, a case like this could perhaps
/// cause an errant fork. While it's difficult to test for "does not create a fork"
/// (at time of writing, the implementation does not fork), we can at least check
/// that this case is handled correctly without issue. Namely, forking should only
/// occur when there are duplicate dependency specifications with disjoint marker
/// expressions.
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
    cmd.env_remove("UV_EXCLUDE_NEWER");
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning
    Resolved 2 packages in [TIME]
    "###
    );

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.8"

        [[distribution]]
        name = "package-a"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_allows_non_conflicting_repeated_dependencies_a-1.0.0.tar.gz#sha256=45ca30f1f66eaf6790198fad279b6448719092f2128f23b99f2ede0d6dde613b", hash = "sha256:45ca30f1f66eaf6790198fad279b6448719092f2128f23b99f2ede0d6dde613b" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_allows_non_conflicting_repeated_dependencies_a-1.0.0-py3-none-any.whl#sha256=fc3f6d2fab10d1bb4f52bd9a7de69dc97ed1792506706ca78bdc9e95d6641a6b", hash = "sha256:fc3f6d2fab10d1bb4f52bd9a7de69dc97ed1792506706ca78bdc9e95d6641a6b" },
        ]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "package-a" },
        ]
        "###
        );
    });

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
    cmd.env_remove("UV_EXCLUDE_NEWER");
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning
    Resolved 3 packages in [TIME]
    "###
    );

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.8"
        environment-markers = [
            "sys_platform == 'darwin'",
            "sys_platform == 'linux'",
            "sys_platform != 'darwin' and sys_platform != 'linux'",
        ]

        [[distribution]]
        name = "package-a"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        environment-markers = [
            "sys_platform == 'darwin'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_basic_a-1.0.0.tar.gz#sha256=9bd6d9d74d8928854f79ea3ed4cd0d8a4906eeaa40f5f3d63460a1c2d5f6d773", hash = "sha256:9bd6d9d74d8928854f79ea3ed4cd0d8a4906eeaa40f5f3d63460a1c2d5f6d773" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_basic_a-1.0.0-py3-none-any.whl#sha256=9d3af617bb44ae1c8daf19f6d4d118ee8aac7eaf0cc5368d0f405137411291a1", hash = "sha256:9d3af617bb44ae1c8daf19f6d4d118ee8aac7eaf0cc5368d0f405137411291a1" },
        ]

        [[distribution]]
        name = "package-a"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        environment-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_basic_a-2.0.0.tar.gz#sha256=c0ce6dfb6d712eb42a4bbe9402a1f823627b9d3773f31d259c49478fc7d8d082", hash = "sha256:c0ce6dfb6d712eb42a4bbe9402a1f823627b9d3773f31d259c49478fc7d8d082" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_basic_a-2.0.0-py3-none-any.whl#sha256=3876778dc6e5178b0e456b0d988cb8c2542cb943a45497aff3e198cbec3dfcc9", hash = "sha256:3876778dc6e5178b0e456b0d988cb8c2542cb943a45497aff3e198cbec3dfcc9" },
        ]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "package-a", version = "1.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'darwin'" },
            { name = "package-a", version = "2.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'linux'" },
        ]
        "###
        );
    });

    Ok(())
}

/// We have a conflict after forking. This scenario exists to test the error
/// message.
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
    cmd.env_remove("UV_EXCLUDE_NEWER");
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning
      × No solution found when resolving dependencies for split (sys_platform == 'darwin'):
      ╰─▶ Because only package-b==1.0.0 is available and package-b==1.0.0 depends on package-d==1, we can conclude that all versions of package-b depend on package-d==1.
          And because package-c==1.0.0 depends on package-d==2 and only package-c==1.0.0 is available, we can conclude that all versions of package-b and all versions of package-c are incompatible.
          And because package-a{sys_platform == 'darwin'}==1.0.0 depends on package-b and package-c, we can conclude that package-a{sys_platform == 'darwin'}==1.0.0 cannot be used.
          And because only the following versions of package-a{sys_platform == 'darwin'} are available:
              package-a{sys_platform == 'darwin'}==1.0.0
              package-a{sys_platform == 'darwin'}>=2
          and project==0.1.0 depends on package-a{sys_platform == 'darwin'}<2, we can conclude that project==0.1.0 cannot be used.
          And because only project==0.1.0 is available and you require project, we can conclude that the requirements are unsatisfiable.
    "###
    );

    Ok(())
}

/// This test ensures that conflicting dependency specifications lead to an
/// unsatisfiable result.  In particular, this is a case that should not fork even
/// though there are conflicting requirements because their marker expressions are
/// overlapping. (Well, there aren't any marker expressions here, which means they
/// are both unconditional.)
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
    cmd.env_remove("UV_EXCLUDE_NEWER");
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning
      × No solution found when resolving dependencies:
      ╰─▶ Because project==0.1.0 depends on package-a>=2 and package-a<2, we can conclude that project==0.1.0 cannot be used.
          And because only project==0.1.0 is available and you require project, we can conclude that the requirements are unsatisfiable.
    "###
    );

    Ok(())
}

/// This tests that sibling dependencies of a package that provokes a fork are
/// correctly filtered out of forks where they are otherwise impossible.  In this
/// case, a previous version of the universal resolver would include both `b` and
/// `c` in *both* of the forks produced by the conflicting dependency specifications
/// on `a`. This in turn led to transitive dependency specifications on both
/// `d==1.0.0` and `d==2.0.0`. Since the universal resolver only forks based on
/// local conditions, this led to a failed resolution.  The correct thing to do here
/// is to ensure that `b` is only part of the `a==4.4.0` fork and `c` is only par of
/// the `a==4.3.0` fork.
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
    cmd.env_remove("UV_EXCLUDE_NEWER");
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning
    Resolved 7 packages in [TIME]
    "###
    );

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.8"
        environment-markers = [
            "sys_platform == 'darwin'",
            "sys_platform == 'linux'",
            "sys_platform != 'darwin' and sys_platform != 'linux'",
        ]

        [[distribution]]
        name = "package-a"
        version = "4.3.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        environment-markers = [
            "sys_platform == 'darwin'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_filter_sibling_dependencies_a-4.3.0.tar.gz#sha256=5389f0927f61393ba8bd940622329299d769e79b725233604a6bdac0fd088c49", hash = "sha256:5389f0927f61393ba8bd940622329299d769e79b725233604a6bdac0fd088c49" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_filter_sibling_dependencies_a-4.3.0-py3-none-any.whl#sha256=932c128393cd499617d1a5b457b11887d51039284b18e06add4c384ab661148c", hash = "sha256:932c128393cd499617d1a5b457b11887d51039284b18e06add4c384ab661148c" },
        ]

        [[distribution]]
        name = "package-a"
        version = "4.4.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        environment-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_filter_sibling_dependencies_a-4.4.0.tar.gz#sha256=7dbb8575aec8f87063954917b6ee628191cd53ca233ec810f6d926b4954e578b", hash = "sha256:7dbb8575aec8f87063954917b6ee628191cd53ca233ec810f6d926b4954e578b" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_filter_sibling_dependencies_a-4.4.0-py3-none-any.whl#sha256=26989734e8fa720896dbbf900adc64551bf3f0026fb62c3c22b47dc23edd4a4c", hash = "sha256:26989734e8fa720896dbbf900adc64551bf3f0026fb62c3c22b47dc23edd4a4c" },
        ]

        [[distribution]]
        name = "package-b"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        dependencies = [
            { name = "package-d", version = "1.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'linux'" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_filter_sibling_dependencies_b-1.0.0.tar.gz#sha256=af3f861d6df9a2bbad55bae02acf17384ea2efa1abbf19206ac56cb021814613", hash = "sha256:af3f861d6df9a2bbad55bae02acf17384ea2efa1abbf19206ac56cb021814613" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_filter_sibling_dependencies_b-1.0.0-py3-none-any.whl#sha256=bc72ef97f57a77fc7be9dc400be26ae5c344aabddbe39407c05a62e07554cdbf", hash = "sha256:bc72ef97f57a77fc7be9dc400be26ae5c344aabddbe39407c05a62e07554cdbf" },
        ]

        [[distribution]]
        name = "package-c"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        dependencies = [
            { name = "package-d", version = "2.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'darwin'" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_filter_sibling_dependencies_c-1.0.0.tar.gz#sha256=c03742ca6e81c2a5d7d8cb72d1214bf03b2925e63858a19097f17d3e1a750192", hash = "sha256:c03742ca6e81c2a5d7d8cb72d1214bf03b2925e63858a19097f17d3e1a750192" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_filter_sibling_dependencies_c-1.0.0-py3-none-any.whl#sha256=71fc9aec5527839358209ccb927186dd0529e9814a725d86aa17e7a033c59cd4", hash = "sha256:71fc9aec5527839358209ccb927186dd0529e9814a725d86aa17e7a033c59cd4" },
        ]

        [[distribution]]
        name = "package-d"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        environment-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_filter_sibling_dependencies_d-1.0.0.tar.gz#sha256=cc1af60e53faf957fd0542349441ea79a909cd5feb30fb8933c39dc33404e4b2", hash = "sha256:cc1af60e53faf957fd0542349441ea79a909cd5feb30fb8933c39dc33404e4b2" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_filter_sibling_dependencies_d-1.0.0-py3-none-any.whl#sha256=de669ada03e9f8625e3ac4af637c88de04066a72675c16c3d1757e0e9d5db7a8", hash = "sha256:de669ada03e9f8625e3ac4af637c88de04066a72675c16c3d1757e0e9d5db7a8" },
        ]

        [[distribution]]
        name = "package-d"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        environment-markers = [
            "sys_platform == 'darwin'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_filter_sibling_dependencies_d-2.0.0.tar.gz#sha256=68e380efdea5206363f5397e4cd560a64f5f4927396dc0b6f6f36dd3f026281f", hash = "sha256:68e380efdea5206363f5397e4cd560a64f5f4927396dc0b6f6f36dd3f026281f" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_filter_sibling_dependencies_d-2.0.0-py3-none-any.whl#sha256=87c133dcc987137d62c011a41af31e8372ca971393d93f808dffc32e136363c7", hash = "sha256:87c133dcc987137d62c011a41af31e8372ca971393d93f808dffc32e136363c7" },
        ]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "package-a", version = "4.3.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'darwin'" },
            { name = "package-a", version = "4.4.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'linux'" },
            { name = "package-b", marker = "sys_platform == 'linux'" },
            { name = "package-c", marker = "sys_platform == 'darwin'" },
        ]
        "###
        );
    });

    Ok(())
}

/// The root cause the resolver to fork over `a`, but the markers on the variant of
/// `a` don't cover the entire marker space, they are missing Python 3.10. Later, we
/// have a dependency this very hole, which we still need to select, instead of
/// having two forks around but without Python 3.10 and omitting `c` from the
/// solution.
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
    cmd.env_remove("UV_EXCLUDE_NEWER");
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning
    Resolved 5 packages in [TIME]
    "###
    );

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.8"
        environment-markers = [
            "python_version < '3.10'",
            "python_version >= '3.11'",
            "python_version < '3.11' and python_version >= '3.10'",
        ]

        [[distribution]]
        name = "package-a"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        environment-markers = [
            "python_version < '3.10'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_incomplete_markers_a-1.0.0.tar.gz#sha256=dd56de2e560b3f95c529c44cbdae55d9b1ada826ddd3e19d3ea45438224ad603", hash = "sha256:dd56de2e560b3f95c529c44cbdae55d9b1ada826ddd3e19d3ea45438224ad603" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_incomplete_markers_a-1.0.0-py3-none-any.whl#sha256=779bb805058fc59858e8b9260cd1a40f13f1640631fdea89d9d243691a4f39ca", hash = "sha256:779bb805058fc59858e8b9260cd1a40f13f1640631fdea89d9d243691a4f39ca" },
        ]

        [[distribution]]
        name = "package-a"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        environment-markers = [
            "python_version >= '3.11'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_incomplete_markers_a-2.0.0.tar.gz#sha256=580f1454a172036c89f5cfbefe52f175b011806a61f243eb476526bcc186e0be", hash = "sha256:580f1454a172036c89f5cfbefe52f175b011806a61f243eb476526bcc186e0be" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_incomplete_markers_a-2.0.0-py3-none-any.whl#sha256=58a4b7dcf929aabf1ed434d9ff8d715929dc3dec02b92cf2b364d5a2206f1f6a", hash = "sha256:58a4b7dcf929aabf1ed434d9ff8d715929dc3dec02b92cf2b364d5a2206f1f6a" },
        ]

        [[distribution]]
        name = "package-b"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        dependencies = [
            { name = "package-c", marker = "python_version == '3.10'" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_incomplete_markers_b-1.0.0.tar.gz#sha256=c4deba44768923473d077bdc0e177033fcb6e6fd406d56830d7ee6f4ffad68c1", hash = "sha256:c4deba44768923473d077bdc0e177033fcb6e6fd406d56830d7ee6f4ffad68c1" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_incomplete_markers_b-1.0.0-py3-none-any.whl#sha256=5c2a5f446580787ed7b3673431b112474237ddeaf1c81325bb30b86e7ee76adb", hash = "sha256:5c2a5f446580787ed7b3673431b112474237ddeaf1c81325bb30b86e7ee76adb" },
        ]

        [[distribution]]
        name = "package-c"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_incomplete_markers_c-1.0.0.tar.gz#sha256=ecc02ea1cc8d3b561c8dcb9d2ba1abcdae2dd32de608bf8e8ed2878118426022", hash = "sha256:ecc02ea1cc8d3b561c8dcb9d2ba1abcdae2dd32de608bf8e8ed2878118426022" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_incomplete_markers_c-1.0.0-py3-none-any.whl#sha256=03fa287aa4cb78457211cb3df7459b99ba1ee2259aae24bc745eaab45e7eaaee", hash = "sha256:03fa287aa4cb78457211cb3df7459b99ba1ee2259aae24bc745eaab45e7eaaee" },
        ]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "package-a", version = "1.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "python_version < '3.10'" },
            { name = "package-a", version = "2.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "python_version >= '3.11'" },
            { name = "package-b", marker = "python_version < '3.10' or python_version >= '3.11' or (python_version < '3.11' and python_version >= '3.10')" },
        ]
        "###
        );
    });

    Ok(())
}

/// This is actually a non-forking test case that tests the tracking of marker
/// expressions in general. In this case, the dependency on `c` should have its
/// marker expressions automatically combined. In this case, it's `linux OR darwin`,
/// even though `linux OR darwin` doesn't actually appear verbatim as a marker
/// expression for any dependency on `c`.
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
    cmd.env_remove("UV_EXCLUDE_NEWER");
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning
    Resolved 4 packages in [TIME]
    "###
    );

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.8"

        [[distribution]]
        name = "package-a"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        dependencies = [
            { name = "package-c", marker = "sys_platform == 'linux'" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_accrue_a-1.0.0.tar.gz#sha256=c791e6062a510c63bff857ca6f7a921a7795bfbc588f21a51124e091fb0343d6", hash = "sha256:c791e6062a510c63bff857ca6f7a921a7795bfbc588f21a51124e091fb0343d6" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_accrue_a-1.0.0-py3-none-any.whl#sha256=cba9cb55cca41833a15c9f8eb75045236cf80cad5d663f7fb7ecae18dad79538", hash = "sha256:cba9cb55cca41833a15c9f8eb75045236cf80cad5d663f7fb7ecae18dad79538" },
        ]

        [[distribution]]
        name = "package-b"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        dependencies = [
            { name = "package-c", marker = "sys_platform == 'darwin'" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_accrue_b-1.0.0.tar.gz#sha256=32e7ea1022061783857c3f6fec5051b4b320630fe8a5aec6523cd565db350387", hash = "sha256:32e7ea1022061783857c3f6fec5051b4b320630fe8a5aec6523cd565db350387" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_accrue_b-1.0.0-py3-none-any.whl#sha256=c5202800c26be15ecaf5560e09ad1df710778bb9debd3c267be1c76f44fbc0c9", hash = "sha256:c5202800c26be15ecaf5560e09ad1df710778bb9debd3c267be1c76f44fbc0c9" },
        ]

        [[distribution]]
        name = "package-c"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_accrue_c-1.0.0.tar.gz#sha256=a3e09ac3dc8e787a08ebe8d5d6072e09720c76cbbcb76a6645d6f59652742015", hash = "sha256:a3e09ac3dc8e787a08ebe8d5d6072e09720c76cbbcb76a6645d6f59652742015" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_accrue_c-1.0.0-py3-none-any.whl#sha256=b0c8719d38c91b2a8548bd065b1d2153fbe031b37775ed244e76fe5bdfbb502e", hash = "sha256:b0c8719d38c91b2a8548bd065b1d2153fbe031b37775ed244e76fe5bdfbb502e" },
        ]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "package-a", marker = "implementation_name == 'cpython'" },
            { name = "package-b", marker = "implementation_name == 'pypy'" },
        ]
        "###
        );
    });

    Ok(())
}

/// A basic test that ensures, at least in this one basic case, that forking in
/// universal resolution happens only when the corresponding marker expressions are
/// completely disjoint. Here, we provide two completely incompatible dependency
/// specifications with equivalent markers. Thus, they are trivially not disjoint,
/// and resolution should fail.  NOTE: This acts a regression test for the initial
/// version of universal resolution that would fork whenever a package was repeated
/// in the list of dependency specifications. So previously, this would produce a
/// resolution with both `1.0.0` and `2.0.0` of `a`. But of course, the correct
/// behavior is to fail resolving.
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
    cmd.env_remove("UV_EXCLUDE_NEWER");
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning
      × No solution found when resolving dependencies:
      ╰─▶ Because project==0.1.0 depends on package-a{sys_platform == 'linux'}>=2 and package-a{sys_platform == 'linux'}<2, we can conclude that project==0.1.0 cannot be used.
          And because only project==0.1.0 is available and you require project, we can conclude that the requirements are unsatisfiable.
    "###
    );

    Ok(())
}

/// This test builds on `fork-marker-inherit-combined`. Namely, we add `or
/// implementation_name == 'pypy'` to the dependency on `c`. While `sys_platform ==
/// 'linux'` cannot be true because of the first fork, the second fork which
/// includes `b==1.0.0` happens precisely when `implementation_name == 'pypy'`. So
/// in this case, `c` should be included.
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
    cmd.env_remove("UV_EXCLUDE_NEWER");
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning
    Resolved 6 packages in [TIME]
    "###
    );

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.8"
        environment-markers = [
            "sys_platform == 'linux'",
            "implementation_name == 'cpython' and sys_platform == 'darwin'",
            "implementation_name == 'pypy' and sys_platform == 'darwin'",
            "implementation_name != 'cpython' and implementation_name != 'pypy' and sys_platform == 'darwin'",
            "sys_platform != 'darwin' and sys_platform != 'linux'",
        ]

        [[distribution]]
        name = "package-a"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        environment-markers = [
            "implementation_name == 'cpython' and sys_platform == 'darwin'",
            "implementation_name == 'pypy' and sys_platform == 'darwin'",
            "implementation_name != 'cpython' and implementation_name != 'pypy' and sys_platform == 'darwin'",
        ]
        dependencies = [
            { name = "package-b", version = "1.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "implementation_name == 'pypy'" },
            { name = "package-b", version = "2.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "implementation_name == 'cpython'" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_allowed_a-1.0.0.tar.gz#sha256=c7232306e8597d46c3fe53a3b1472f99b8ff36b3169f335ba0a5b625e193f7d4", hash = "sha256:c7232306e8597d46c3fe53a3b1472f99b8ff36b3169f335ba0a5b625e193f7d4" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_allowed_a-1.0.0-py3-none-any.whl#sha256=198ae54c02a59734dc009bfcee1148d40f56c605b62f9f1a00467e09ebf2ff07", hash = "sha256:198ae54c02a59734dc009bfcee1148d40f56c605b62f9f1a00467e09ebf2ff07" },
        ]

        [[distribution]]
        name = "package-a"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        environment-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_allowed_a-2.0.0.tar.gz#sha256=0dcb58c8276afbe439e1c94708fb71954fb8869cc65c230ce8f462c911992ceb", hash = "sha256:0dcb58c8276afbe439e1c94708fb71954fb8869cc65c230ce8f462c911992ceb" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_allowed_a-2.0.0-py3-none-any.whl#sha256=61b7d273468584342de4c0185beed5b128797ce95ec9ec4a670fe30f73351cf7", hash = "sha256:61b7d273468584342de4c0185beed5b128797ce95ec9ec4a670fe30f73351cf7" },
        ]

        [[distribution]]
        name = "package-b"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        environment-markers = [
            "implementation_name == 'pypy' and sys_platform == 'darwin'",
        ]
        dependencies = [
            { name = "package-c", marker = "implementation_name == 'pypy' or sys_platform == 'linux'" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_allowed_b-1.0.0.tar.gz#sha256=d6bd196a0a152c1b32e09f08e554d22ae6a6b3b916e39ad4552572afae5f5492", hash = "sha256:d6bd196a0a152c1b32e09f08e554d22ae6a6b3b916e39ad4552572afae5f5492" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_allowed_b-1.0.0-py3-none-any.whl#sha256=e1deba885509945ef087e4f31c7dba3ee436fc8284b360fe207a3c42f2f9e22f", hash = "sha256:e1deba885509945ef087e4f31c7dba3ee436fc8284b360fe207a3c42f2f9e22f" },
        ]

        [[distribution]]
        name = "package-b"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        environment-markers = [
            "implementation_name == 'cpython' and sys_platform == 'darwin'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_allowed_b-2.0.0.tar.gz#sha256=4533845ba671575a25ceb32f10f0bc6836949bef37b7da6e7dd37d9be389871c", hash = "sha256:4533845ba671575a25ceb32f10f0bc6836949bef37b7da6e7dd37d9be389871c" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_allowed_b-2.0.0-py3-none-any.whl#sha256=736d1b59cb46a0b889614bc7557c293de245fe8954e3200e786500ae8e42504b", hash = "sha256:736d1b59cb46a0b889614bc7557c293de245fe8954e3200e786500ae8e42504b" },
        ]

        [[distribution]]
        name = "package-c"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_allowed_c-1.0.0.tar.gz#sha256=7ce8efca029cfa952e64f55c2d47fe33975c7f77ec689384bda11cbc3b7ef1db", hash = "sha256:7ce8efca029cfa952e64f55c2d47fe33975c7f77ec689384bda11cbc3b7ef1db" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_allowed_c-1.0.0-py3-none-any.whl#sha256=6a6b776dedabceb6a6c4f54a5d932076fa3fed1380310491999ca2d31e13b41c", hash = "sha256:6a6b776dedabceb6a6c4f54a5d932076fa3fed1380310491999ca2d31e13b41c" },
        ]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "package-a", version = "1.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'darwin'" },
            { name = "package-a", version = "2.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'linux'" },
        ]
        "###
        );
    });

    Ok(())
}

/// This test builds on `fork-marker-inherit-combined`. Namely, we add `or
/// implementation_name == 'cpython'` to the dependency on `c`. While `sys_platform
/// == 'linux'` cannot be true because of the first fork, the second fork which
/// includes `b==1.0.0` happens precisely when `implementation_name == 'pypy'`,
/// which is *also* disjoint with `implementation_name == 'cpython'`. Therefore, `c`
/// should not be included here.
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
    cmd.env_remove("UV_EXCLUDE_NEWER");
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning
    Resolved 5 packages in [TIME]
    "###
    );

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.8"
        environment-markers = [
            "sys_platform == 'linux'",
            "implementation_name == 'cpython' and sys_platform == 'darwin'",
            "implementation_name == 'pypy' and sys_platform == 'darwin'",
            "implementation_name != 'cpython' and implementation_name != 'pypy' and sys_platform == 'darwin'",
            "sys_platform != 'darwin' and sys_platform != 'linux'",
        ]

        [[distribution]]
        name = "package-a"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        environment-markers = [
            "implementation_name == 'cpython' and sys_platform == 'darwin'",
            "implementation_name == 'pypy' and sys_platform == 'darwin'",
            "implementation_name != 'cpython' and implementation_name != 'pypy' and sys_platform == 'darwin'",
        ]
        dependencies = [
            { name = "package-b", version = "1.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "implementation_name == 'pypy'" },
            { name = "package-b", version = "2.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "implementation_name == 'cpython'" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_disallowed_a-1.0.0.tar.gz#sha256=92081d91570582f3a94ed156f203de53baca5b3fdc350aa1c831c7c42723e798", hash = "sha256:92081d91570582f3a94ed156f203de53baca5b3fdc350aa1c831c7c42723e798" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_disallowed_a-1.0.0-py3-none-any.whl#sha256=ee2dc68d5b33c0318183431cebf99ccca63d98601b936e5d3eae804c73f2b154", hash = "sha256:ee2dc68d5b33c0318183431cebf99ccca63d98601b936e5d3eae804c73f2b154" },
        ]

        [[distribution]]
        name = "package-a"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        environment-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_disallowed_a-2.0.0.tar.gz#sha256=9d48383b0699f575af15871f6f7a928b835cd5ad4e13f91a675ee5aba722dabc", hash = "sha256:9d48383b0699f575af15871f6f7a928b835cd5ad4e13f91a675ee5aba722dabc" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_disallowed_a-2.0.0-py3-none-any.whl#sha256=099db8d3af6c9dfc10589ab0f1e2e6a74276a167afb39322ddaf657791247456", hash = "sha256:099db8d3af6c9dfc10589ab0f1e2e6a74276a167afb39322ddaf657791247456" },
        ]

        [[distribution]]
        name = "package-b"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        environment-markers = [
            "implementation_name == 'pypy' and sys_platform == 'darwin'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_disallowed_b-1.0.0.tar.gz#sha256=d44b87bd8d39240bca55eaae84a245e74197ed0b7897c27af9f168c713cc63bd", hash = "sha256:d44b87bd8d39240bca55eaae84a245e74197ed0b7897c27af9f168c713cc63bd" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_disallowed_b-1.0.0-py3-none-any.whl#sha256=999b3d0029ea0131272257e2b04c0e673defa6c25be6efc411e04936bce72ef6", hash = "sha256:999b3d0029ea0131272257e2b04c0e673defa6c25be6efc411e04936bce72ef6" },
        ]

        [[distribution]]
        name = "package-b"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        environment-markers = [
            "implementation_name == 'cpython' and sys_platform == 'darwin'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_disallowed_b-2.0.0.tar.gz#sha256=75a48bf2d44a0a0be6ca33820f5076665765be7b43dabf5f94f7fd5247071097", hash = "sha256:75a48bf2d44a0a0be6ca33820f5076665765be7b43dabf5f94f7fd5247071097" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_disallowed_b-2.0.0-py3-none-any.whl#sha256=2c6aedd257d0ed21bb96f6e0baba8314c001d4078d09413cda147fb6badb39a2", hash = "sha256:2c6aedd257d0ed21bb96f6e0baba8314c001d4078d09413cda147fb6badb39a2" },
        ]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "package-a", version = "1.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'darwin'" },
            { name = "package-a", version = "2.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'linux'" },
        ]
        "###
        );
    });

    Ok(())
}

/// In this test, we check that marker expressions which provoke a fork are carried
/// through to subsequent forks. Here, the `a>=2` and `a<2` dependency
/// specifications create a fork, and then the `a<2` fork leads to `a==1.0.0` with
/// dependency specifications on `b>=2` and `b<2` that provoke yet another fork.
/// Finally, in the `b<2` fork, a dependency on `c` is introduced whose marker
/// expression is disjoint with the marker expression that provoked the *first*
/// fork. Therefore, `c` should be entirely excluded from the resolution.
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
    cmd.env_remove("UV_EXCLUDE_NEWER");
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning
    Resolved 5 packages in [TIME]
    "###
    );

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.8"
        environment-markers = [
            "sys_platform == 'linux'",
            "implementation_name == 'cpython' and sys_platform == 'darwin'",
            "implementation_name == 'pypy' and sys_platform == 'darwin'",
            "implementation_name != 'cpython' and implementation_name != 'pypy' and sys_platform == 'darwin'",
            "sys_platform != 'darwin' and sys_platform != 'linux'",
        ]

        [[distribution]]
        name = "package-a"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        environment-markers = [
            "implementation_name == 'cpython' and sys_platform == 'darwin'",
            "implementation_name == 'pypy' and sys_platform == 'darwin'",
            "implementation_name != 'cpython' and implementation_name != 'pypy' and sys_platform == 'darwin'",
        ]
        dependencies = [
            { name = "package-b", version = "1.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "implementation_name == 'pypy'" },
            { name = "package-b", version = "2.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "implementation_name == 'cpython'" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_a-1.0.0.tar.gz#sha256=2ec4c9dbb7078227d996c344b9e0c1b365ed0000de9527b2ba5b616233636f07", hash = "sha256:2ec4c9dbb7078227d996c344b9e0c1b365ed0000de9527b2ba5b616233636f07" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_a-1.0.0-py3-none-any.whl#sha256=1150f6d977824bc0260cfb5fcf34816424ed4602d5df316c291b8df3f723c888", hash = "sha256:1150f6d977824bc0260cfb5fcf34816424ed4602d5df316c291b8df3f723c888" },
        ]

        [[distribution]]
        name = "package-a"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        environment-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_a-2.0.0.tar.gz#sha256=47958d1659220ee7722b0f26e8c1fe41217a2816881ffa929f0ba794a87ceebf", hash = "sha256:47958d1659220ee7722b0f26e8c1fe41217a2816881ffa929f0ba794a87ceebf" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_a-2.0.0-py3-none-any.whl#sha256=67e142d749674a27c872db714d50fda083010789da51291e3c30b4daf0e96b3b", hash = "sha256:67e142d749674a27c872db714d50fda083010789da51291e3c30b4daf0e96b3b" },
        ]

        [[distribution]]
        name = "package-b"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        environment-markers = [
            "implementation_name == 'pypy' and sys_platform == 'darwin'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_b-1.0.0.tar.gz#sha256=6992d194cb5a0f0eed9ed6617d3212af4e3ff09274bf7622c8a1008b072128da", hash = "sha256:6992d194cb5a0f0eed9ed6617d3212af4e3ff09274bf7622c8a1008b072128da" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_b-1.0.0-py3-none-any.whl#sha256=d9b50d8a0968d65af338e27d6b2a58eea59c514e47b820752a2c068b5a8333a7", hash = "sha256:d9b50d8a0968d65af338e27d6b2a58eea59c514e47b820752a2c068b5a8333a7" },
        ]

        [[distribution]]
        name = "package-b"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        environment-markers = [
            "implementation_name == 'cpython' and sys_platform == 'darwin'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_b-2.0.0.tar.gz#sha256=e340061505d621a340d10ec1dbaf02dfce0c66358ee8190f61f78018f9999989", hash = "sha256:e340061505d621a340d10ec1dbaf02dfce0c66358ee8190f61f78018f9999989" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_combined_b-2.0.0-py3-none-any.whl#sha256=ff364fd590d05651579d8bea621b069934470106b9a82ab960fb93dfd88ea038", hash = "sha256:ff364fd590d05651579d8bea621b069934470106b9a82ab960fb93dfd88ea038" },
        ]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "package-a", version = "1.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'darwin'" },
            { name = "package-a", version = "2.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'linux'" },
        ]
        "###
        );
    });

    Ok(())
}

/// This is like `fork-marker-inherit`, but where both `a>=2` and `a<2` have a
/// conditional dependency on `b`. For `a>=2`, the conditional dependency on `b` has
/// overlap with the `a>=2` marker expression, and thus, `b` should be included
/// *only* in the dependencies for `a==2.0.0`. As with `fork-marker-inherit`, the
/// `a<2` path should exclude `b==1.0.0` since their marker expressions are
/// disjoint.
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
    cmd.env_remove("UV_EXCLUDE_NEWER");
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning
    Resolved 4 packages in [TIME]
    "###
    );

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.8"
        environment-markers = [
            "sys_platform == 'darwin'",
            "sys_platform == 'linux'",
            "sys_platform != 'darwin' and sys_platform != 'linux'",
        ]

        [[distribution]]
        name = "package-a"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        environment-markers = [
            "sys_platform == 'darwin'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_isolated_a-1.0.0.tar.gz#sha256=724ffc24debfa2bc6b5c2457df777c523638ec3586cc953f8509dad581aa6887", hash = "sha256:724ffc24debfa2bc6b5c2457df777c523638ec3586cc953f8509dad581aa6887" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_isolated_a-1.0.0-py3-none-any.whl#sha256=6823b88bf6debf2ec6195d82943c2812235a642438f007c0a3c95d745a5b95ba", hash = "sha256:6823b88bf6debf2ec6195d82943c2812235a642438f007c0a3c95d745a5b95ba" },
        ]

        [[distribution]]
        name = "package-a"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        environment-markers = [
            "sys_platform == 'linux'",
        ]
        dependencies = [
            { name = "package-b", marker = "sys_platform == 'linux'" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_isolated_a-2.0.0.tar.gz#sha256=bc4567da4349a9c09b7fb4733f0b9f6476249240192291cf051c2b1d28b829fd", hash = "sha256:bc4567da4349a9c09b7fb4733f0b9f6476249240192291cf051c2b1d28b829fd" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_isolated_a-2.0.0-py3-none-any.whl#sha256=16986b43ef61e3f639b61fc9c22ede133864606d3d72716161a59acd64793c02", hash = "sha256:16986b43ef61e3f639b61fc9c22ede133864606d3d72716161a59acd64793c02" },
        ]

        [[distribution]]
        name = "package-b"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_isolated_b-1.0.0.tar.gz#sha256=96f8c3cabc5795e08a064c89ec76a4bfba8afe3c13d647161b4a1568b4584ced", hash = "sha256:96f8c3cabc5795e08a064c89ec76a4bfba8afe3c13d647161b4a1568b4584ced" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_isolated_b-1.0.0-py3-none-any.whl#sha256=c8affc2f13f9bcd08b3d1601a21a1781ea14d52a8cddc708b29428c9c3d53ea5", hash = "sha256:c8affc2f13f9bcd08b3d1601a21a1781ea14d52a8cddc708b29428c9c3d53ea5" },
        ]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "package-a", version = "1.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'darwin'" },
            { name = "package-a", version = "2.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'linux'" },
        ]
        "###
        );
    });

    Ok(())
}

/// This is like `fork-marker-inherit`, but tests that the marker expressions that
/// provoke a fork are carried transitively through the dependency graph. In this
/// case, `a<2 -> b -> c -> d`, but where the last dependency on `d` requires a
/// marker expression that is disjoint with the initial `a<2` dependency. Therefore,
/// it ought to be completely excluded from the resolution.
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
    cmd.env_remove("UV_EXCLUDE_NEWER");
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning
    Resolved 5 packages in [TIME]
    "###
    );

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.8"
        environment-markers = [
            "sys_platform == 'darwin'",
            "sys_platform == 'linux'",
            "sys_platform != 'darwin' and sys_platform != 'linux'",
        ]

        [[distribution]]
        name = "package-a"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        environment-markers = [
            "sys_platform == 'darwin'",
        ]
        dependencies = [
            { name = "package-b", marker = "sys_platform == 'darwin'" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_transitive_a-1.0.0.tar.gz#sha256=8bcab85231487b9350471da0c4c22dc3d69dfe4a1198d16b5f81b0235d7112ce", hash = "sha256:8bcab85231487b9350471da0c4c22dc3d69dfe4a1198d16b5f81b0235d7112ce" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_transitive_a-1.0.0-py3-none-any.whl#sha256=84d650ff1a909198ba82cbe0f697e836d8a570fb71faa6ad4a30c4df332dfde6", hash = "sha256:84d650ff1a909198ba82cbe0f697e836d8a570fb71faa6ad4a30c4df332dfde6" },
        ]

        [[distribution]]
        name = "package-a"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        environment-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_transitive_a-2.0.0.tar.gz#sha256=4437ac14c340fec0b451cbc9486f5b8e106568634264ecad339a8de565a93be6", hash = "sha256:4437ac14c340fec0b451cbc9486f5b8e106568634264ecad339a8de565a93be6" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_transitive_a-2.0.0-py3-none-any.whl#sha256=420c4c6b02d22c33f7f8ae9f290acc5b4c372fc2e49c881d259237a31c76dc0b", hash = "sha256:420c4c6b02d22c33f7f8ae9f290acc5b4c372fc2e49c881d259237a31c76dc0b" },
        ]

        [[distribution]]
        name = "package-b"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        dependencies = [
            { name = "package-c", marker = "sys_platform == 'darwin'" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_transitive_b-1.0.0.tar.gz#sha256=03b4b0e323c36bd4a1e51a65e1489715da231d44d26e12b54544e3bf9a9f6129", hash = "sha256:03b4b0e323c36bd4a1e51a65e1489715da231d44d26e12b54544e3bf9a9f6129" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_transitive_b-1.0.0-py3-none-any.whl#sha256=c9738afccc13d7d5bd7be85abf5dc77f88c43c577fb2f90dfa2abf1ffa0c8db6", hash = "sha256:c9738afccc13d7d5bd7be85abf5dc77f88c43c577fb2f90dfa2abf1ffa0c8db6" },
        ]

        [[distribution]]
        name = "package-c"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_transitive_c-1.0.0.tar.gz#sha256=58bb788896b2297f2948f51a27fc48cfe44057c687a3c0c4d686b107975f7f32", hash = "sha256:58bb788896b2297f2948f51a27fc48cfe44057c687a3c0c4d686b107975f7f32" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_transitive_c-1.0.0-py3-none-any.whl#sha256=ad2cbb0582ec6f4dc9549d1726d2aae66cd1fdf0e355acc70cd720cf65ae4d86", hash = "sha256:ad2cbb0582ec6f4dc9549d1726d2aae66cd1fdf0e355acc70cd720cf65ae4d86" },
        ]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "package-a", version = "1.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'darwin'" },
            { name = "package-a", version = "2.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'linux'" },
        ]
        "###
        );
    });

    Ok(())
}

/// This tests that markers which provoked a fork in the universal resolver are used
/// to ignore dependencies which cannot possibly be installed by a resolution
/// produced by that fork.  In this example, the `a<2` dependency is only active on
/// Darwin platforms. But the `a==1.0.0` distribution has a dependency on `b` that
/// is only active on Linux, where as `a==2.0.0` does not. Therefore, when the fork
/// provoked by the `a<2` dependency considers `b`, it should ignore it because it
/// isn't possible for `sys_platform == 'linux'` and `sys_platform == 'darwin'` to
/// be simultaneously true.
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
    cmd.env_remove("UV_EXCLUDE_NEWER");
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning
    Resolved 3 packages in [TIME]
    "###
    );

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.8"
        environment-markers = [
            "sys_platform == 'darwin'",
            "sys_platform == 'linux'",
            "sys_platform != 'darwin' and sys_platform != 'linux'",
        ]

        [[distribution]]
        name = "package-a"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        environment-markers = [
            "sys_platform == 'darwin'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_a-1.0.0.tar.gz#sha256=177511ec69a2f04de39867d43f167a33194ae983e8f86a1cc9b51f59fc379d4b", hash = "sha256:177511ec69a2f04de39867d43f167a33194ae983e8f86a1cc9b51f59fc379d4b" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_a-1.0.0-py3-none-any.whl#sha256=16447932477c5feaa874b4e7510023c6f732578cec07158bc0e872af887a77d6", hash = "sha256:16447932477c5feaa874b4e7510023c6f732578cec07158bc0e872af887a77d6" },
        ]

        [[distribution]]
        name = "package-a"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        environment-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_a-2.0.0.tar.gz#sha256=43e24ce6fcbfbbff1db5eb20b583c20c2aa0888138bfafeab205c4ccc6e7e0a4", hash = "sha256:43e24ce6fcbfbbff1db5eb20b583c20c2aa0888138bfafeab205c4ccc6e7e0a4" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_inherit_a-2.0.0-py3-none-any.whl#sha256=d650b6acf8f68d85e210ceb3e7802fbe84aad2b918b06a72dee534fe5474852b", hash = "sha256:d650b6acf8f68d85e210ceb3e7802fbe84aad2b918b06a72dee534fe5474852b" },
        ]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "package-a", version = "1.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'darwin'" },
            { name = "package-a", version = "2.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'linux'" },
        ]
        "###
        );
    });

    Ok(())
}

/// This is like `fork-marker-inherit`, but it tests that dependency filtering only
/// occurs in the context of a fork.  For example, as in `fork-marker-inherit`, the
/// `c` dependency of `a<2` should be entirely excluded here since it is possible
/// for `sys_platform` to be simultaneously equivalent to Darwin and Linux. However,
/// the unconditional dependency on `b`, which in turn depends on `c` for Linux
/// only, should still incorporate `c` as the dependency is not part of any fork.
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
    cmd.env_remove("UV_EXCLUDE_NEWER");
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning
    Resolved 5 packages in [TIME]
    "###
    );

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.8"
        environment-markers = [
            "sys_platform == 'darwin'",
            "sys_platform == 'linux'",
            "sys_platform != 'darwin' and sys_platform != 'linux'",
        ]

        [[distribution]]
        name = "package-a"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        environment-markers = [
            "sys_platform == 'darwin'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_limited_inherit_a-1.0.0.tar.gz#sha256=ab1fde8d0acb9a2fe99b7a005939962b1c26c6d876e4a55e81fb9d1a1e5e9f76", hash = "sha256:ab1fde8d0acb9a2fe99b7a005939962b1c26c6d876e4a55e81fb9d1a1e5e9f76" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_limited_inherit_a-1.0.0-py3-none-any.whl#sha256=0dcb9659eeb891701535005a2afd7c579f566d3908e96137db74129924ae6a7e", hash = "sha256:0dcb9659eeb891701535005a2afd7c579f566d3908e96137db74129924ae6a7e" },
        ]

        [[distribution]]
        name = "package-a"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        environment-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_limited_inherit_a-2.0.0.tar.gz#sha256=009fdb8872cf52324c1bcdebef31feaba3c262fd76d150a753152aeee3d55b10", hash = "sha256:009fdb8872cf52324c1bcdebef31feaba3c262fd76d150a753152aeee3d55b10" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_limited_inherit_a-2.0.0-py3-none-any.whl#sha256=10957fddbd5611e0db154744a01d588c7105e26fd5f6a8150956ca9542d844c5", hash = "sha256:10957fddbd5611e0db154744a01d588c7105e26fd5f6a8150956ca9542d844c5" },
        ]

        [[distribution]]
        name = "package-b"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        dependencies = [
            { name = "package-c", marker = "sys_platform == 'linux'" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_limited_inherit_b-1.0.0.tar.gz#sha256=4c04e090df03e308ecd38a9b8db9813a09fb20a747a89f86c497702c3e5a9001", hash = "sha256:4c04e090df03e308ecd38a9b8db9813a09fb20a747a89f86c497702c3e5a9001" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_limited_inherit_b-1.0.0-py3-none-any.whl#sha256=17365faaf25dba08be579867f219f914a0ff3298441f8d7b6201625a253333ec", hash = "sha256:17365faaf25dba08be579867f219f914a0ff3298441f8d7b6201625a253333ec" },
        ]

        [[distribution]]
        name = "package-c"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_limited_inherit_c-1.0.0.tar.gz#sha256=8dcb05f5dff09fec52ab507b215ff367fe815848319a17929db997ad3afe88ae", hash = "sha256:8dcb05f5dff09fec52ab507b215ff367fe815848319a17929db997ad3afe88ae" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_limited_inherit_c-1.0.0-py3-none-any.whl#sha256=877a87a4987ad795ddaded3e7266ed7defdd3cfbe07a29500cb6047637db4065", hash = "sha256:877a87a4987ad795ddaded3e7266ed7defdd3cfbe07a29500cb6047637db4065" },
        ]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "package-a", version = "1.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'darwin'" },
            { name = "package-a", version = "2.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'linux'" },
            { name = "package-b", marker = "sys_platform == 'darwin' or sys_platform == 'linux' or (sys_platform != 'darwin' and sys_platform != 'linux')" },
        ]
        "###
        );
    });

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
    cmd.env_remove("UV_EXCLUDE_NEWER");
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning
    Resolved 4 packages in [TIME]
    "###
    );

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.8"
        environment-markers = [
            "sys_platform == 'darwin'",
            "sys_platform == 'linux'",
            "sys_platform != 'darwin' and sys_platform != 'linux'",
        ]

        [[distribution]]
        name = "package-a"
        version = "0.1.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_selection_a-0.1.0.tar.gz#sha256=ece83ba864a62d5d747439f79a0bf36aa4c18d15bca96aab855ffc2e94a8eef7", hash = "sha256:ece83ba864a62d5d747439f79a0bf36aa4c18d15bca96aab855ffc2e94a8eef7" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_selection_a-0.1.0-py3-none-any.whl#sha256=a3b9d6e46cc226d20994cc60653fd59d81d96527749f971a6f59ef8cbcbc7c01", hash = "sha256:a3b9d6e46cc226d20994cc60653fd59d81d96527749f971a6f59ef8cbcbc7c01" },
        ]

        [[distribution]]
        name = "package-b"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        environment-markers = [
            "sys_platform == 'darwin'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_selection_b-1.0.0.tar.gz#sha256=6f5ea28cadb8b5dfa15d32c9e38818f8f7150fc4f9a58e49aec4e10b23342be4", hash = "sha256:6f5ea28cadb8b5dfa15d32c9e38818f8f7150fc4f9a58e49aec4e10b23342be4" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_selection_b-1.0.0-py3-none-any.whl#sha256=5eb8c7fc25dfe94c8a3b71bc09eadb8cd4c7e55b974cee851b848c3856d6a4f9", hash = "sha256:5eb8c7fc25dfe94c8a3b71bc09eadb8cd4c7e55b974cee851b848c3856d6a4f9" },
        ]

        [[distribution]]
        name = "package-b"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        environment-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_selection_b-2.0.0.tar.gz#sha256=d32033ecdf37d605e4b3b3e88df6562bb7ca01c6ed3fb9a55ec078eccc1df9d1", hash = "sha256:d32033ecdf37d605e4b3b3e88df6562bb7ca01c6ed3fb9a55ec078eccc1df9d1" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_selection_b-2.0.0-py3-none-any.whl#sha256=163fbcd238a66243064d41bd383657a63e45155f63bf92668c23af5245307380", hash = "sha256:163fbcd238a66243064d41bd383657a63e45155f63bf92668c23af5245307380" },
        ]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "package-a", marker = "sys_platform == 'darwin' or sys_platform == 'linux' or (sys_platform != 'darwin' and sys_platform != 'linux')" },
            { name = "package-b", version = "1.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'darwin'" },
            { name = "package-b", version = "2.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'linux'" },
        ]
        "###
        );
    });

    Ok(())
}

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
    cmd.env_remove("UV_EXCLUDE_NEWER");
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning
    Resolved 5 packages in [TIME]
    "###
    );

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.8"
        environment-markers = [
            "sys_platform == 'darwin'",
            "sys_platform == 'linux'",
            "sys_platform != 'darwin' and sys_platform != 'linux'",
        ]

        [[distribution]]
        name = "package-a"
        version = "1.3.1"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        dependencies = [
            { name = "package-c", marker = "implementation_name == 'iron'" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_track_a-1.3.1.tar.gz#sha256=ffc490c887058825e96a0cc4a270cf56b72f7f28b927c450086603317bb8c120", hash = "sha256:ffc490c887058825e96a0cc4a270cf56b72f7f28b927c450086603317bb8c120" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_track_a-1.3.1-py3-none-any.whl#sha256=d9dc6a64400a041199df2d37182582ff7cc986bac486da273d814627e9b86648", hash = "sha256:d9dc6a64400a041199df2d37182582ff7cc986bac486da273d814627e9b86648" },
        ]

        [[distribution]]
        name = "package-b"
        version = "2.7"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        environment-markers = [
            "sys_platform == 'darwin'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_track_b-2.7.tar.gz#sha256=855bf45837a4ba669a5850b14b0253cb138925fdd2b06a2f15c6582b8fabb8a0", hash = "sha256:855bf45837a4ba669a5850b14b0253cb138925fdd2b06a2f15c6582b8fabb8a0" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_track_b-2.7-py3-none-any.whl#sha256=544eb2b567d2293c47da724af91fec59c2d3e06675617d29068864ec3a4e390f", hash = "sha256:544eb2b567d2293c47da724af91fec59c2d3e06675617d29068864ec3a4e390f" },
        ]

        [[distribution]]
        name = "package-b"
        version = "2.8"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        environment-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_track_b-2.8.tar.gz#sha256=2e14b0ff1fb7f5cf491bd31d876218adee1d6a208ff197dc30363cdf25262e80", hash = "sha256:2e14b0ff1fb7f5cf491bd31d876218adee1d6a208ff197dc30363cdf25262e80" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_track_b-2.8-py3-none-any.whl#sha256=5aba691ce804ee39b2464c7757f8680786a1468e152ee845ff841c37f8112e21", hash = "sha256:5aba691ce804ee39b2464c7757f8680786a1468e152ee845ff841c37f8112e21" },
        ]

        [[distribution]]
        name = "package-c"
        version = "1.10"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_track_c-1.10.tar.gz#sha256=c89006d893254790b0fcdd1b33520241c8ff66ab950c6752b745e006bdeff144", hash = "sha256:c89006d893254790b0fcdd1b33520241c8ff66ab950c6752b745e006bdeff144" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_marker_track_c-1.10-py3-none-any.whl#sha256=cedcb8fbcdd9fbde4eea76612e57536c8b56507a9d7f7a92e483cb56b18c57a3", hash = "sha256:cedcb8fbcdd9fbde4eea76612e57536c8b56507a9d7f7a92e483cb56b18c57a3" },
        ]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "package-a", marker = "sys_platform == 'darwin' or sys_platform == 'linux' or (sys_platform != 'darwin' and sys_platform != 'linux')" },
            { name = "package-b", version = "2.7", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'darwin'" },
            { name = "package-b", version = "2.8", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'linux'" },
        ]
        "###
        );
    });

    Ok(())
}

/// This is the same setup as `non-local-fork-marker-transitive`, but the disjoint
/// dependency specifications on `c` use the same constraints and thus depend on the
/// same version of `c`. In this case, there is no conflict.
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
    cmd.env_remove("UV_EXCLUDE_NEWER");
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning
    Resolved 4 packages in [TIME]
    "###
    );

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.8"

        [[distribution]]
        name = "package-a"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        dependencies = [
            { name = "package-c", marker = "sys_platform == 'linux'" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_non_fork_marker_transitive_a-1.0.0.tar.gz#sha256=68cff02c9f4a0b3014fdce524982a3cbf3a2ecaf0291c32c824cadb19f1e7cd0", hash = "sha256:68cff02c9f4a0b3014fdce524982a3cbf3a2ecaf0291c32c824cadb19f1e7cd0" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_non_fork_marker_transitive_a-1.0.0-py3-none-any.whl#sha256=6c49aef823d3544d795c05497ca2dbd5c419cad4454e4d41b8f4860be45bd4bf", hash = "sha256:6c49aef823d3544d795c05497ca2dbd5c419cad4454e4d41b8f4860be45bd4bf" },
        ]

        [[distribution]]
        name = "package-b"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        dependencies = [
            { name = "package-c", marker = "sys_platform == 'darwin'" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_non_fork_marker_transitive_b-1.0.0.tar.gz#sha256=ae7abe9cde79b810f91dff7329b63788a8253250053fe4e82563f0b2d0877182", hash = "sha256:ae7abe9cde79b810f91dff7329b63788a8253250053fe4e82563f0b2d0877182" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_non_fork_marker_transitive_b-1.0.0-py3-none-any.whl#sha256=6f301799cb51d920c7bef0120d5914f8315758ddc9f856b88783efae706dac16", hash = "sha256:6f301799cb51d920c7bef0120d5914f8315758ddc9f856b88783efae706dac16" },
        ]

        [[distribution]]
        name = "package-c"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_non_fork_marker_transitive_c-2.0.0.tar.gz#sha256=ffab9124854f64c8b5059ccaed481547f54abac868ba98aa6a454c0163cdb1c7", hash = "sha256:ffab9124854f64c8b5059ccaed481547f54abac868ba98aa6a454c0163cdb1c7" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_non_fork_marker_transitive_c-2.0.0-py3-none-any.whl#sha256=2b72d6af81967e1c55f30d920d6a7b913fce6ad0a0658ec79972a3d1a054e85f", hash = "sha256:2b72d6af81967e1c55f30d920d6a7b913fce6ad0a0658ec79972a3d1a054e85f" },
        ]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "package-a" },
            { name = "package-b" },
        ]
        "###
        );
    });

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
    cmd.env_remove("UV_EXCLUDE_NEWER");
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning
      × No solution found when resolving dependencies:
      ╰─▶ Because package-b{sys_platform == 'darwin'}==1.0.0 depends on package-c>=2.0.0 and package-a{sys_platform == 'linux'}==1.0.0 depends on package-c<2.0.0, we can conclude that package-a{sys_platform == 'linux'}==1.0.0 and package-b{sys_platform == 'darwin'}==1.0.0 are incompatible.
          And because project==0.1.0 depends on package-a{sys_platform == 'linux'}==1.0.0 and package-b{sys_platform == 'darwin'}==1.0.0, we can conclude that project==0.1.0 cannot be used.
          And because only project==0.1.0 is available and you require project, we can conclude that the requirements are unsatisfiable.
    "###
    );

    Ok(())
}

/// This setup introduces dependencies on two distinct versions of `c`, where each
/// such dependency has a marker expression attached that would normally make them
/// disjoint. In a non-universal resolver, this is no problem. But in a forking
/// resolver that tries to create one universal resolution, this can lead to two
/// distinct versions of `c` in the resolution. This is in and of itself not a
/// problem, since that is an expected scenario for universal resolution. The
/// problem in this case is that because the dependency specifications for `c` occur
/// in two different points (i.e., they are not sibling dependency specifications)
/// in the dependency graph, the forking resolver does not "detect" it, and thus
/// never forks and thus this results in "no resolution."
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
    cmd.env_remove("UV_EXCLUDE_NEWER");
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning
      × No solution found when resolving dependencies:
      ╰─▶ Because package-b==1.0.0 depends on package-c{sys_platform == 'darwin'}>=2.0.0 and only package-c{sys_platform == 'darwin'}<=2.0.0 is available, we can conclude that package-b==1.0.0 depends on package-c{sys_platform == 'darwin'}==2.0.0.
          And because only the following versions of package-c{sys_platform == 'linux'} are available:
              package-c{sys_platform == 'linux'}==1.0.0
              package-c{sys_platform == 'linux'}>=2.0.0
          and package-a==1.0.0 depends on package-c{sys_platform == 'linux'}<2.0.0, we can conclude that package-a==1.0.0 and package-b==1.0.0 are incompatible.
          And because project==0.1.0 depends on package-a==1.0.0 and package-b==1.0.0, we can conclude that project==0.1.0 cannot be used.
          And because only project==0.1.0 is available and you require project, we can conclude that the requirements are unsatisfiable.
    "###
    );

    Ok(())
}

/// Like `preferences-dependent-forking`, but when we don't fork the resolution
/// fails.  Consider a fresh run without preferences: * We start with cleaver 2 * We
/// fork * We reject cleaver 2 * We find cleaver solution in fork 1 with foo 2 with
/// bar 1 * We find cleaver solution in fork 2 with foo 1 with bar 2 * We write
/// cleaver 1, foo 1, foo 2, bar 1 and bar 2 to the lockfile  In a subsequent run,
/// we read the preference cleaver 1 from the lockfile (the preferences for foo and
/// bar don't matter): * We start with cleaver 1 * We're in universal mode, cleaver
/// requires foo 1, bar 1 * foo 1 requires bar 2, conflict  Design sketch: ```text
/// root -> clear, foo, bar # Cause a fork, then forget that version. cleaver 2 ->
/// unrelated-dep==1; fork==1 cleaver 2 -> unrelated-dep==2; fork==2 cleaver 2 ->
/// reject-cleaver-2 # Allow different versions when forking, but force foo 1, bar 1
/// in universal mode without forking. cleaver 1 -> foo==1; fork==1 cleaver 1 ->
/// bar==1; fork==2 # When we selected foo 1, bar 1 in universal mode for cleaver,
/// this causes a conflict, otherwise we select bar 2. foo 1 -> bar==2 ```
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
    cmd.env_remove("UV_EXCLUDE_NEWER");
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning
    Resolved 6 packages in [TIME]
    "###
    );

    Ok(())
}

/// This test contains a scenario where the solution depends on whether we fork, and
/// whether we fork depends on the preferences.  Consider a fresh run without
/// preferences: * We start with cleaver 2 * We fork * We reject cleaver 2 * We find
/// cleaver solution in fork 1 with foo 2 with bar 1 * We find cleaver solution in
/// fork 2 with foo 1 with bar 2 * We write cleaver 1, foo 1, foo 2, bar 1 and bar 2
/// to the lockfile  In a subsequent run, we read the preference cleaver 1 from the
/// lockfile (the preferences for foo and bar don't matter): * We start with cleaver
/// 1 * We're in universal mode, we resolve foo 1 and bar 1 * We write cleaver 1 and
/// bar 1 to the lockfile  We call a resolution that's different on the second run
/// to the first unstable.  Design sketch: ```text root -> clear, foo, bar # Cause a
/// fork, then forget that version. cleaver 2 -> unrelated-dep==1; fork==1 cleaver 2
/// -> unrelated-dep==2; fork==2 cleaver 2 -> reject-cleaver-2 # Allow different
/// versions when forking, but force foo 1, bar 1 in universal mode without forking.
/// cleaver 1 -> foo==1; fork==1 cleaver 1 -> bar==1; fork==2 ```
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
    cmd.env_remove("UV_EXCLUDE_NEWER");
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning
    Resolved 5 packages in [TIME]
    "###
    );

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.8"
        environment-markers = [
            "sys_platform == 'linux'",
            "sys_platform != 'linux'",
        ]

        [[distribution]]
        name = "package-bar"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_bar-1.0.0.tar.gz#sha256=7eef4e0c910b9e4cadf6c707e60a2151f7dc6407d815112ec93a467d76226f5e", hash = "sha256:7eef4e0c910b9e4cadf6c707e60a2151f7dc6407d815112ec93a467d76226f5e" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_bar-1.0.0-py3-none-any.whl#sha256=3cdaac4b0ba330f902d0628c0b1d6e62692f52255d02718d04f46ade7c8ad6a6", hash = "sha256:3cdaac4b0ba330f902d0628c0b1d6e62692f52255d02718d04f46ade7c8ad6a6" },
        ]

        [[distribution]]
        name = "package-cleaver"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        dependencies = [
            { name = "package-bar", marker = "sys_platform != 'linux'" },
            { name = "package-foo", version = "1.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'linux'" },
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_cleaver-1.0.0.tar.gz#sha256=0347b927fdf7731758ea53e1594309fc6311ca6983f36553bc11654a264062b2", hash = "sha256:0347b927fdf7731758ea53e1594309fc6311ca6983f36553bc11654a264062b2" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_cleaver-1.0.0-py3-none-any.whl#sha256=855467570c9da8e92ce37d0ebd0653cfa50d5d88b9540beca94feaa37a539dc3", hash = "sha256:855467570c9da8e92ce37d0ebd0653cfa50d5d88b9540beca94feaa37a539dc3" },
        ]

        [[distribution]]
        name = "package-foo"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        environment-markers = [
            "sys_platform == 'linux'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_foo-1.0.0.tar.gz#sha256=abf1c0ac825ee5961e683067634916f98c6651a6d4473ff87d8b57c17af8fed2", hash = "sha256:abf1c0ac825ee5961e683067634916f98c6651a6d4473ff87d8b57c17af8fed2" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_foo-1.0.0-py3-none-any.whl#sha256=85348e8df4892b9f297560c16abcf231828f538dc07339ed121197a00a0626a5", hash = "sha256:85348e8df4892b9f297560c16abcf231828f538dc07339ed121197a00a0626a5" },
        ]

        [[distribution]]
        name = "package-foo"
        version = "2.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        environment-markers = [
            "sys_platform != 'linux'",
        ]
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_foo-2.0.0.tar.gz#sha256=ad54d14a4fd931b8ccb6412edef71fe223c36362d0ccfe3fa251c17d4f07e4a9", hash = "sha256:ad54d14a4fd931b8ccb6412edef71fe223c36362d0ccfe3fa251c17d4f07e4a9" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/preferences_dependent_forking_foo-2.0.0-py3-none-any.whl#sha256=bae278cf259c0e031e52b6cbb537d945e0e606d045e980b90d406d0f1e06aae9", hash = "sha256:bae278cf259c0e031e52b6cbb537d945e0e606d045e980b90d406d0f1e06aae9" },
        ]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "package-bar" },
            { name = "package-cleaver" },
            { name = "package-foo", version = "1.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform == 'linux'" },
            { name = "package-foo", version = "2.0.0", source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }, marker = "sys_platform != 'linux'" },
        ]
        "###
        );
    });

    Ok(())
}

/// This tests that a `Requires-Python` specifier will result in the exclusion of
/// dependency specifications that cannot possibly satisfy it.  In particular, this
/// is tested via the `python_full_version` marker with a pre-release version.
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
    cmd.env_remove("UV_EXCLUDE_NEWER");
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning
    Resolved 1 package in [TIME]
    "###
    );

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.10"

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        "###
        );
    });

    Ok(())
}

/// This tests that a `Requires-Python` specifier will result in the exclusion of
/// dependency specifications that cannot possibly satisfy it.  In particular, this
/// is tested via the `python_full_version` marker instead of the more common
/// `python_version` marker.
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
    cmd.env_remove("UV_EXCLUDE_NEWER");
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning
    Resolved 1 package in [TIME]
    "###
    );

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.10"

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        "###
        );
    });

    Ok(())
}

/// This tests that a `Requires-Python` specifier that includes a Python patch
/// version will not result in excluded a dependency specification with a
/// `python_version == '3.10'` marker.  This is a regression test for the universal
/// resolver where it would convert a `Requires-Python: >=3.10.1` specifier into a
/// `python_version >= '3.10.1'` marker expression, which would be considered
/// disjoint with `python_version == '3.10'`. Thus, the dependency `a` below was
/// erroneously excluded. It should be included.
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
    cmd.env_remove("UV_EXCLUDE_NEWER");
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning
    Resolved 2 packages in [TIME]
    "###
    );

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.10.1"

        [[distribution]]
        name = "package-a"
        version = "1.0.0"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_requires_python_patch_overlap_a-1.0.0.tar.gz#sha256=ac2820ee4808788674295192d79a709e3259aa4eef5b155e77f719ad4eaa324d", hash = "sha256:ac2820ee4808788674295192d79a709e3259aa4eef5b155e77f719ad4eaa324d" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/fork_requires_python_patch_overlap_a-1.0.0-py3-none-any.whl#sha256=43a750ba4eaab749d608d70e94d3d51e083cc21f5a52ac99b5967b26486d5ef1", hash = "sha256:43a750ba4eaab749d608d70e94d3d51e083cc21f5a52ac99b5967b26486d5ef1" },
        ]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "package-a", marker = "python_version == '3.10'" },
        ]
        "###
        );
    });

    Ok(())
}

/// This tests that a `Requires-Python` specifier will result in the exclusion of
/// dependency specifications that cannot possibly satisfy it.
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
    cmd.env_remove("UV_EXCLUDE_NEWER");
    cmd.arg("--index-url").arg(packse_index_url());
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning
    Resolved 1 package in [TIME]
    "###
    );

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.10"

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        "###
        );
    });

    Ok(())
}
