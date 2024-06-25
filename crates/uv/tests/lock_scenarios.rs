//! DO NOT EDIT
//!
//! Generated with `./scripts/sync_scenarios.sh`
//! Scenarios from <https://github.com/astral-sh/packse/tree/0.3.29/scenarios>
//!
#![cfg(all(feature = "python", feature = "pypi"))]
#![allow(clippy::needless_raw_string_hashes)]

use anyhow::Result;
use assert_fs::prelude::*;
use insta::assert_snapshot;

use common::{uv_snapshot, TestContext};

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
    cmd.arg("--index-url")
        .arg("https://astral-sh.github.io/packse/0.3.29/simple-html/");
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
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

        [[distribution]]
        name = "package-a"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.29/files/fork_allows_non_conflicting_non_overlapping_dependencies_a-1.0.0.tar.gz#sha256=dd40a6bd59fbeefbf9f4936aec3df6fb6017e57d334f85f482ae5dd03ae353b9", hash = "sha256:dd40a6bd59fbeefbf9f4936aec3df6fb6017e57d334f85f482ae5dd03ae353b9" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.29/files/fork_allows_non_conflicting_non_overlapping_dependencies_a-1.0.0-py3-none-any.whl#sha256=37c13aa13cca009990929df08bed3d9de26e1d405a5ebd16ec0c3baef6899b23", hash = "sha256:37c13aa13cca009990929df08bed3d9de26e1d405a5ebd16ec0c3baef6899b23" }]

        [[distribution]]
        name = "package-a"
        version = "2.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.29/files/fork_allows_non_conflicting_non_overlapping_dependencies_a-2.0.0.tar.gz#sha256=0b4ca63d060f4daa2269c08b7083f594e096b94e1bcbde53d212c65b52378358", hash = "sha256:0b4ca63d060f4daa2269c08b7083f594e096b94e1bcbde53d212c65b52378358" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.29/files/fork_allows_non_conflicting_non_overlapping_dependencies_a-2.0.0-py3-none-any.whl#sha256=35168196ad80d0f2822191a47e3a805b4ad527280c8b84e7eed77b7fee505497", hash = "sha256:35168196ad80d0f2822191a47e3a805b4ad527280c8b84e7eed77b7fee505497" }]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+."

        [[distribution.dependencies]]
        name = "package-a"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        marker = "sys_platform == 'darwin'"

        [[distribution.dependencies]]
        name = "package-a"
        version = "2.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        marker = "sys_platform == 'linux'"
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
    cmd.arg("--index-url")
        .arg("https://astral-sh.github.io/packse/0.3.29/simple-html/");
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
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
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.29/files/fork_allows_non_conflicting_repeated_dependencies_a-1.0.0.tar.gz#sha256=45ca30f1f66eaf6790198fad279b6448719092f2128f23b99f2ede0d6dde613b", hash = "sha256:45ca30f1f66eaf6790198fad279b6448719092f2128f23b99f2ede0d6dde613b" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.29/files/fork_allows_non_conflicting_repeated_dependencies_a-1.0.0-py3-none-any.whl#sha256=b08be7896afa7402a2650dd9ebf38e32cf65d5bf86956dc23cf793c5f1f21af2", hash = "sha256:b08be7896afa7402a2650dd9ebf38e32cf65d5bf86956dc23cf793c5f1f21af2" }]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+."

        [[distribution.dependencies]]
        name = "package-a"
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
    cmd.arg("--index-url")
        .arg("https://astral-sh.github.io/packse/0.3.29/simple-html/");
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
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

        [[distribution]]
        name = "package-a"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.29/files/fork_basic_a-1.0.0.tar.gz#sha256=9bd6d9d74d8928854f79ea3ed4cd0d8a4906eeaa40f5f3d63460a1c2d5f6d773", hash = "sha256:9bd6d9d74d8928854f79ea3ed4cd0d8a4906eeaa40f5f3d63460a1c2d5f6d773" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.29/files/fork_basic_a-1.0.0-py3-none-any.whl#sha256=1a28e30240634de42f24d34ff9bdac181208ef57215def75baac0de205685d27", hash = "sha256:1a28e30240634de42f24d34ff9bdac181208ef57215def75baac0de205685d27" }]

        [[distribution]]
        name = "package-a"
        version = "2.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.29/files/fork_basic_a-2.0.0.tar.gz#sha256=c0ce6dfb6d712eb42a4bbe9402a1f823627b9d3773f31d259c49478fc7d8d082", hash = "sha256:c0ce6dfb6d712eb42a4bbe9402a1f823627b9d3773f31d259c49478fc7d8d082" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.29/files/fork_basic_a-2.0.0-py3-none-any.whl#sha256=c830122b1b31a6229208e04ced83ae4d1cef8be28b857b56af054bb4bd868a30", hash = "sha256:c830122b1b31a6229208e04ced83ae4d1cef8be28b857b56af054bb4bd868a30" }]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+."

        [[distribution.dependencies]]
        name = "package-a"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        marker = "sys_platform == 'darwin'"

        [[distribution.dependencies]]
        name = "package-a"
        version = "2.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        marker = "sys_platform == 'linux'"
        "###
        );
    });

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
    cmd.arg("--index-url")
        .arg("https://astral-sh.github.io/packse/0.3.29/simple-html/");
    uv_snapshot!(filters, cmd, @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
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
    cmd.arg("--index-url")
        .arg("https://astral-sh.github.io/packse/0.3.29/simple-html/");
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
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

        [[distribution]]
        name = "package-a"
        version = "4.3.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.29/files/fork_filter_sibling_dependencies_a-4.3.0.tar.gz#sha256=5389f0927f61393ba8bd940622329299d769e79b725233604a6bdac0fd088c49", hash = "sha256:5389f0927f61393ba8bd940622329299d769e79b725233604a6bdac0fd088c49" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.29/files/fork_filter_sibling_dependencies_a-4.3.0-py3-none-any.whl#sha256=dabbae1b6f06cf5e94660f5f7d7b15984480ad0c8506fb0e89d86693fadf604a", hash = "sha256:dabbae1b6f06cf5e94660f5f7d7b15984480ad0c8506fb0e89d86693fadf604a" }]

        [[distribution]]
        name = "package-a"
        version = "4.4.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.29/files/fork_filter_sibling_dependencies_a-4.4.0.tar.gz#sha256=7dbb8575aec8f87063954917b6ee628191cd53ca233ec810f6d926b4954e578b", hash = "sha256:7dbb8575aec8f87063954917b6ee628191cd53ca233ec810f6d926b4954e578b" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.29/files/fork_filter_sibling_dependencies_a-4.4.0-py3-none-any.whl#sha256=7fb31ea56cb4bbe1d7035b93d39bdb9f37148e1ced83607b69a8f7279546e5a0", hash = "sha256:7fb31ea56cb4bbe1d7035b93d39bdb9f37148e1ced83607b69a8f7279546e5a0" }]

        [[distribution]]
        name = "package-b"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.29/files/fork_filter_sibling_dependencies_b-1.0.0.tar.gz#sha256=af3f861d6df9a2bbad55bae02acf17384ea2efa1abbf19206ac56cb021814613", hash = "sha256:af3f861d6df9a2bbad55bae02acf17384ea2efa1abbf19206ac56cb021814613" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.29/files/fork_filter_sibling_dependencies_b-1.0.0-py3-none-any.whl#sha256=229e8570b5bec521c62fa46ffc242ecdf49e095d043f7761a8390213a63f8d7f", hash = "sha256:229e8570b5bec521c62fa46ffc242ecdf49e095d043f7761a8390213a63f8d7f" }]

        [[distribution.dependencies]]
        name = "package-d"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"

        [[distribution]]
        name = "package-c"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.29/files/fork_filter_sibling_dependencies_c-1.0.0.tar.gz#sha256=c03742ca6e81c2a5d7d8cb72d1214bf03b2925e63858a19097f17d3e1a750192", hash = "sha256:c03742ca6e81c2a5d7d8cb72d1214bf03b2925e63858a19097f17d3e1a750192" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.29/files/fork_filter_sibling_dependencies_c-1.0.0-py3-none-any.whl#sha256=28b4728d6ef73637bd296038f8b1895de6fffab1a0f5a2673b7408ac6941e401", hash = "sha256:28b4728d6ef73637bd296038f8b1895de6fffab1a0f5a2673b7408ac6941e401" }]

        [[distribution.dependencies]]
        name = "package-d"
        version = "2.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"

        [[distribution]]
        name = "package-d"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.29/files/fork_filter_sibling_dependencies_d-1.0.0.tar.gz#sha256=cc1af60e53faf957fd0542349441ea79a909cd5feb30fb8933c39dc33404e4b2", hash = "sha256:cc1af60e53faf957fd0542349441ea79a909cd5feb30fb8933c39dc33404e4b2" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.29/files/fork_filter_sibling_dependencies_d-1.0.0-py3-none-any.whl#sha256=aae484d9359b53d95f5ae5e6413317c15cc0f715e5053251394fb748d3de992f", hash = "sha256:aae484d9359b53d95f5ae5e6413317c15cc0f715e5053251394fb748d3de992f" }]

        [[distribution]]
        name = "package-d"
        version = "2.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.29/files/fork_filter_sibling_dependencies_d-2.0.0.tar.gz#sha256=68e380efdea5206363f5397e4cd560a64f5f4927396dc0b6f6f36dd3f026281f", hash = "sha256:68e380efdea5206363f5397e4cd560a64f5f4927396dc0b6f6f36dd3f026281f" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.29/files/fork_filter_sibling_dependencies_d-2.0.0-py3-none-any.whl#sha256=d1c58c45df69e10aaf278190025c77d9bb989518f9d5bc2fe57643434f3de1d2", hash = "sha256:d1c58c45df69e10aaf278190025c77d9bb989518f9d5bc2fe57643434f3de1d2" }]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+."

        [[distribution.dependencies]]
        name = "package-a"
        version = "4.3.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        marker = "sys_platform == 'darwin'"

        [[distribution.dependencies]]
        name = "package-a"
        version = "4.4.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        marker = "sys_platform == 'linux'"

        [[distribution.dependencies]]
        name = "package-b"
        marker = "sys_platform == 'linux'"

        [[distribution.dependencies]]
        name = "package-c"
        marker = "sys_platform == 'darwin'"
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
    cmd.arg("--index-url")
        .arg("https://astral-sh.github.io/packse/0.3.29/simple-html/");
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
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
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_accrue_a-1.0.0.tar.gz#sha256=c791e6062a510c63bff857ca6f7a921a7795bfbc588f21a51124e091fb0343d6", hash = "sha256:c791e6062a510c63bff857ca6f7a921a7795bfbc588f21a51124e091fb0343d6" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_accrue_a-1.0.0-py3-none-any.whl#sha256=c1b40f368e7be6c16c1d537481421bf6cd1e18a09a83f42ed35029633d4b3249", hash = "sha256:c1b40f368e7be6c16c1d537481421bf6cd1e18a09a83f42ed35029633d4b3249" }]

        [[distribution.dependencies]]
        name = "package-c"
        marker = "sys_platform == 'linux'"

        [[distribution]]
        name = "package-b"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_accrue_b-1.0.0.tar.gz#sha256=32e7ea1022061783857c3f6fec5051b4b320630fe8a5aec6523cd565db350387", hash = "sha256:32e7ea1022061783857c3f6fec5051b4b320630fe8a5aec6523cd565db350387" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_accrue_b-1.0.0-py3-none-any.whl#sha256=2df49decd1b188800d1cbf5265806d32388e4c792da3074b6f9be2e7b72185f5", hash = "sha256:2df49decd1b188800d1cbf5265806d32388e4c792da3074b6f9be2e7b72185f5" }]

        [[distribution.dependencies]]
        name = "package-c"
        marker = "sys_platform == 'darwin'"

        [[distribution]]
        name = "package-c"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_accrue_c-1.0.0.tar.gz#sha256=a3e09ac3dc8e787a08ebe8d5d6072e09720c76cbbcb76a6645d6f59652742015", hash = "sha256:a3e09ac3dc8e787a08ebe8d5d6072e09720c76cbbcb76a6645d6f59652742015" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_accrue_c-1.0.0-py3-none-any.whl#sha256=01993b60f134b3b80585fe95e3511b9a6194c2387c0215d962dbf65abd5a5fe1", hash = "sha256:01993b60f134b3b80585fe95e3511b9a6194c2387c0215d962dbf65abd5a5fe1" }]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+."

        [[distribution.dependencies]]
        name = "package-a"
        marker = "implementation_name == 'cpython'"

        [[distribution.dependencies]]
        name = "package-b"
        marker = "implementation_name == 'pypy'"
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
    cmd.arg("--index-url")
        .arg("https://astral-sh.github.io/packse/0.3.29/simple-html/");
    uv_snapshot!(filters, cmd, @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
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
    cmd.arg("--index-url")
        .arg("https://astral-sh.github.io/packse/0.3.29/simple-html/");
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
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

        [[distribution]]
        name = "package-a"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_inherit_combined_allowed_a-1.0.0.tar.gz#sha256=c7232306e8597d46c3fe53a3b1472f99b8ff36b3169f335ba0a5b625e193f7d4", hash = "sha256:c7232306e8597d46c3fe53a3b1472f99b8ff36b3169f335ba0a5b625e193f7d4" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_inherit_combined_allowed_a-1.0.0-py3-none-any.whl#sha256=609b50f1a9fdbea0cc5d77f3eaf5143900159d809c56db5839de138c3123d980", hash = "sha256:609b50f1a9fdbea0cc5d77f3eaf5143900159d809c56db5839de138c3123d980" }]

        [[distribution.dependencies]]
        name = "package-b"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        marker = "implementation_name == 'pypy'"

        [[distribution.dependencies]]
        name = "package-b"
        version = "2.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        marker = "implementation_name == 'cpython'"

        [[distribution]]
        name = "package-a"
        version = "2.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_inherit_combined_allowed_a-2.0.0.tar.gz#sha256=0dcb58c8276afbe439e1c94708fb71954fb8869cc65c230ce8f462c911992ceb", hash = "sha256:0dcb58c8276afbe439e1c94708fb71954fb8869cc65c230ce8f462c911992ceb" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_inherit_combined_allowed_a-2.0.0-py3-none-any.whl#sha256=ed8912f08993ddc9131dcc268b28385ddd84635f4a3c6a3fd83f680838353f1f", hash = "sha256:ed8912f08993ddc9131dcc268b28385ddd84635f4a3c6a3fd83f680838353f1f" }]

        [[distribution]]
        name = "package-b"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_inherit_combined_allowed_b-1.0.0.tar.gz#sha256=d6bd196a0a152c1b32e09f08e554d22ae6a6b3b916e39ad4552572afae5f5492", hash = "sha256:d6bd196a0a152c1b32e09f08e554d22ae6a6b3b916e39ad4552572afae5f5492" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_inherit_combined_allowed_b-1.0.0-py3-none-any.whl#sha256=7935455a6f9a9448dd7ee33978bb4d463b16ef9cd021c24462c7b51e8c4ebc86", hash = "sha256:7935455a6f9a9448dd7ee33978bb4d463b16ef9cd021c24462c7b51e8c4ebc86" }]

        [[distribution.dependencies]]
        name = "package-c"
        marker = "implementation_name == 'pypy' or sys_platform == 'linux'"

        [[distribution]]
        name = "package-b"
        version = "2.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_inherit_combined_allowed_b-2.0.0.tar.gz#sha256=4533845ba671575a25ceb32f10f0bc6836949bef37b7da6e7dd37d9be389871c", hash = "sha256:4533845ba671575a25ceb32f10f0bc6836949bef37b7da6e7dd37d9be389871c" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_inherit_combined_allowed_b-2.0.0-py3-none-any.whl#sha256=6ec5a8c2241bafcf537c695257cd0cd66f392607bac2fc523ee1ad91fbbf623f", hash = "sha256:6ec5a8c2241bafcf537c695257cd0cd66f392607bac2fc523ee1ad91fbbf623f" }]

        [[distribution]]
        name = "package-c"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_inherit_combined_allowed_c-1.0.0.tar.gz#sha256=7ce8efca029cfa952e64f55c2d47fe33975c7f77ec689384bda11cbc3b7ef1db", hash = "sha256:7ce8efca029cfa952e64f55c2d47fe33975c7f77ec689384bda11cbc3b7ef1db" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_inherit_combined_allowed_c-1.0.0-py3-none-any.whl#sha256=8513f5ac195dbb03aa5cec408fed74ad6ab7ace4822dbc6a8effec7f5a1bf47b", hash = "sha256:8513f5ac195dbb03aa5cec408fed74ad6ab7ace4822dbc6a8effec7f5a1bf47b" }]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+."

        [[distribution.dependencies]]
        name = "package-a"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        marker = "sys_platform == 'darwin'"

        [[distribution.dependencies]]
        name = "package-a"
        version = "2.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        marker = "sys_platform == 'linux'"
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
    cmd.arg("--index-url")
        .arg("https://astral-sh.github.io/packse/0.3.29/simple-html/");
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
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

        [[distribution]]
        name = "package-a"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_inherit_combined_disallowed_a-1.0.0.tar.gz#sha256=92081d91570582f3a94ed156f203de53baca5b3fdc350aa1c831c7c42723e798", hash = "sha256:92081d91570582f3a94ed156f203de53baca5b3fdc350aa1c831c7c42723e798" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_inherit_combined_disallowed_a-1.0.0-py3-none-any.whl#sha256=5a3da0f8036be92c72212ff734672805ed8b799a1bc46d3bde56f8486180e1cf", hash = "sha256:5a3da0f8036be92c72212ff734672805ed8b799a1bc46d3bde56f8486180e1cf" }]

        [[distribution.dependencies]]
        name = "package-b"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        marker = "implementation_name == 'pypy'"

        [[distribution.dependencies]]
        name = "package-b"
        version = "2.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        marker = "implementation_name == 'cpython'"

        [[distribution]]
        name = "package-a"
        version = "2.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_inherit_combined_disallowed_a-2.0.0.tar.gz#sha256=9d48383b0699f575af15871f6f7a928b835cd5ad4e13f91a675ee5aba722dabc", hash = "sha256:9d48383b0699f575af15871f6f7a928b835cd5ad4e13f91a675ee5aba722dabc" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_inherit_combined_disallowed_a-2.0.0-py3-none-any.whl#sha256=16091f91bfb46e9db76d7e513d521e15baf7cf1ceae86c09e71e31b6734a7834", hash = "sha256:16091f91bfb46e9db76d7e513d521e15baf7cf1ceae86c09e71e31b6734a7834" }]

        [[distribution]]
        name = "package-b"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_inherit_combined_disallowed_b-1.0.0.tar.gz#sha256=d44b87bd8d39240bca55eaae84a245e74197ed0b7897c27af9f168c713cc63bd", hash = "sha256:d44b87bd8d39240bca55eaae84a245e74197ed0b7897c27af9f168c713cc63bd" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_inherit_combined_disallowed_b-1.0.0-py3-none-any.whl#sha256=382d45c91771c962d7d12328bc58248fe0908d258307474beae1c868d9005631", hash = "sha256:382d45c91771c962d7d12328bc58248fe0908d258307474beae1c868d9005631" }]

        [[distribution]]
        name = "package-b"
        version = "2.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_inherit_combined_disallowed_b-2.0.0.tar.gz#sha256=75a48bf2d44a0a0be6ca33820f5076665765be7b43dabf5f94f7fd5247071097", hash = "sha256:75a48bf2d44a0a0be6ca33820f5076665765be7b43dabf5f94f7fd5247071097" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_inherit_combined_disallowed_b-2.0.0-py3-none-any.whl#sha256=cf730e495e15e6eed66857ce2c044f6dd16fe9ba24354f56ac1c6abfeb9cd976", hash = "sha256:cf730e495e15e6eed66857ce2c044f6dd16fe9ba24354f56ac1c6abfeb9cd976" }]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+."

        [[distribution.dependencies]]
        name = "package-a"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        marker = "sys_platform == 'darwin'"

        [[distribution.dependencies]]
        name = "package-a"
        version = "2.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        marker = "sys_platform == 'linux'"
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
    cmd.arg("--index-url")
        .arg("https://astral-sh.github.io/packse/0.3.29/simple-html/");
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
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

        [[distribution]]
        name = "package-a"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_inherit_combined_a-1.0.0.tar.gz#sha256=2ec4c9dbb7078227d996c344b9e0c1b365ed0000de9527b2ba5b616233636f07", hash = "sha256:2ec4c9dbb7078227d996c344b9e0c1b365ed0000de9527b2ba5b616233636f07" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_inherit_combined_a-1.0.0-py3-none-any.whl#sha256=b24ba06cf3717872f4155240294a11d5cbe8c6b4a5b00963d056b491886b4efe", hash = "sha256:b24ba06cf3717872f4155240294a11d5cbe8c6b4a5b00963d056b491886b4efe" }]

        [[distribution.dependencies]]
        name = "package-b"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        marker = "implementation_name == 'pypy'"

        [[distribution.dependencies]]
        name = "package-b"
        version = "2.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        marker = "implementation_name == 'cpython'"

        [[distribution]]
        name = "package-a"
        version = "2.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_inherit_combined_a-2.0.0.tar.gz#sha256=47958d1659220ee7722b0f26e8c1fe41217a2816881ffa929f0ba794a87ceebf", hash = "sha256:47958d1659220ee7722b0f26e8c1fe41217a2816881ffa929f0ba794a87ceebf" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_inherit_combined_a-2.0.0-py3-none-any.whl#sha256=e2c1eb831475f344a019e6e5b90e1f95f51553d45a4dc9d6f8e23593b8e5c8a1", hash = "sha256:e2c1eb831475f344a019e6e5b90e1f95f51553d45a4dc9d6f8e23593b8e5c8a1" }]

        [[distribution]]
        name = "package-b"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_inherit_combined_b-1.0.0.tar.gz#sha256=6992d194cb5a0f0eed9ed6617d3212af4e3ff09274bf7622c8a1008b072128da", hash = "sha256:6992d194cb5a0f0eed9ed6617d3212af4e3ff09274bf7622c8a1008b072128da" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_inherit_combined_b-1.0.0-py3-none-any.whl#sha256=a3a0071d27faf67fb6d4766b8e557d4642c4f2baa043c4ef430311b177bc126c", hash = "sha256:a3a0071d27faf67fb6d4766b8e557d4642c4f2baa043c4ef430311b177bc126c" }]

        [[distribution]]
        name = "package-b"
        version = "2.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_inherit_combined_b-2.0.0.tar.gz#sha256=e340061505d621a340d10ec1dbaf02dfce0c66358ee8190f61f78018f9999989", hash = "sha256:e340061505d621a340d10ec1dbaf02dfce0c66358ee8190f61f78018f9999989" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_inherit_combined_b-2.0.0-py3-none-any.whl#sha256=1723c70dd56b62995764ce0d616052caa9183fcac252a130eeb34fe3f2e9656f", hash = "sha256:1723c70dd56b62995764ce0d616052caa9183fcac252a130eeb34fe3f2e9656f" }]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+."

        [[distribution.dependencies]]
        name = "package-a"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        marker = "sys_platform == 'darwin'"

        [[distribution.dependencies]]
        name = "package-a"
        version = "2.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        marker = "sys_platform == 'linux'"
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
    cmd.arg("--index-url")
        .arg("https://astral-sh.github.io/packse/0.3.29/simple-html/");
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
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
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_inherit_isolated_a-1.0.0.tar.gz#sha256=724ffc24debfa2bc6b5c2457df777c523638ec3586cc953f8509dad581aa6887", hash = "sha256:724ffc24debfa2bc6b5c2457df777c523638ec3586cc953f8509dad581aa6887" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_inherit_isolated_a-1.0.0-py3-none-any.whl#sha256=0d66a59f2f0afc2d077594603239bf616eabe9ffd203fe9d41621782c7fef33a", hash = "sha256:0d66a59f2f0afc2d077594603239bf616eabe9ffd203fe9d41621782c7fef33a" }]

        [[distribution]]
        name = "package-a"
        version = "2.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_inherit_isolated_a-2.0.0.tar.gz#sha256=bc4567da4349a9c09b7fb4733f0b9f6476249240192291cf051c2b1d28b829fd", hash = "sha256:bc4567da4349a9c09b7fb4733f0b9f6476249240192291cf051c2b1d28b829fd" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_inherit_isolated_a-2.0.0-py3-none-any.whl#sha256=dc624746ae061d80e0bb3e25d0427177983f1edd0b519095f7a3f22b13bc54bd", hash = "sha256:dc624746ae061d80e0bb3e25d0427177983f1edd0b519095f7a3f22b13bc54bd" }]

        [[distribution.dependencies]]
        name = "package-b"
        marker = "sys_platform == 'linux'"

        [[distribution]]
        name = "package-b"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_inherit_isolated_b-1.0.0.tar.gz#sha256=96f8c3cabc5795e08a064c89ec76a4bfba8afe3c13d647161b4a1568b4584ced", hash = "sha256:96f8c3cabc5795e08a064c89ec76a4bfba8afe3c13d647161b4a1568b4584ced" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_inherit_isolated_b-1.0.0-py3-none-any.whl#sha256=79e3a8219a49a5d72db2468c856a65212345e9a7e028abf8c179320f49d85be5", hash = "sha256:79e3a8219a49a5d72db2468c856a65212345e9a7e028abf8c179320f49d85be5" }]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+."

        [[distribution.dependencies]]
        name = "package-a"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        marker = "sys_platform == 'darwin'"

        [[distribution.dependencies]]
        name = "package-a"
        version = "2.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        marker = "sys_platform == 'linux'"
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
    cmd.arg("--index-url")
        .arg("https://astral-sh.github.io/packse/0.3.29/simple-html/");
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
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

        [[distribution]]
        name = "package-a"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_inherit_transitive_a-1.0.0.tar.gz#sha256=8bcab85231487b9350471da0c4c22dc3d69dfe4a1198d16b5f81b0235d7112ce", hash = "sha256:8bcab85231487b9350471da0c4c22dc3d69dfe4a1198d16b5f81b0235d7112ce" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_inherit_transitive_a-1.0.0-py3-none-any.whl#sha256=6904ed373d504f278de986c440e39b6105be4680654e38dbe37c5d539a67fd3d", hash = "sha256:6904ed373d504f278de986c440e39b6105be4680654e38dbe37c5d539a67fd3d" }]

        [[distribution.dependencies]]
        name = "package-b"

        [[distribution]]
        name = "package-a"
        version = "2.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_inherit_transitive_a-2.0.0.tar.gz#sha256=4437ac14c340fec0b451cbc9486f5b8e106568634264ecad339a8de565a93be6", hash = "sha256:4437ac14c340fec0b451cbc9486f5b8e106568634264ecad339a8de565a93be6" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_inherit_transitive_a-2.0.0-py3-none-any.whl#sha256=2965412b82b12d0b0ab081c7e05133cb64209241a4f2836359c76a5ed8d7fb99", hash = "sha256:2965412b82b12d0b0ab081c7e05133cb64209241a4f2836359c76a5ed8d7fb99" }]

        [[distribution]]
        name = "package-b"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_inherit_transitive_b-1.0.0.tar.gz#sha256=03b4b0e323c36bd4a1e51a65e1489715da231d44d26e12b54544e3bf9a9f6129", hash = "sha256:03b4b0e323c36bd4a1e51a65e1489715da231d44d26e12b54544e3bf9a9f6129" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_inherit_transitive_b-1.0.0-py3-none-any.whl#sha256=f3975f31f7e3dee42abb1b44f6e7fea0048bd1f204debc2f1decf16b3d597c9f", hash = "sha256:f3975f31f7e3dee42abb1b44f6e7fea0048bd1f204debc2f1decf16b3d597c9f" }]

        [[distribution.dependencies]]
        name = "package-c"

        [[distribution]]
        name = "package-c"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_inherit_transitive_c-1.0.0.tar.gz#sha256=58bb788896b2297f2948f51a27fc48cfe44057c687a3c0c4d686b107975f7f32", hash = "sha256:58bb788896b2297f2948f51a27fc48cfe44057c687a3c0c4d686b107975f7f32" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_inherit_transitive_c-1.0.0-py3-none-any.whl#sha256=a827909e237ab405b250b4cc534dc51e502a7e583a11205b6db8af51058b38c5", hash = "sha256:a827909e237ab405b250b4cc534dc51e502a7e583a11205b6db8af51058b38c5" }]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+."

        [[distribution.dependencies]]
        name = "package-a"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        marker = "sys_platform == 'darwin'"

        [[distribution.dependencies]]
        name = "package-a"
        version = "2.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        marker = "sys_platform == 'linux'"
        "###
        );
    });

    Ok(())
}

/// This tests that markers which provoked a fork in the universal resolver are used
/// to ignore dependencies which cannot possibly be installed by a resolution
/// produced by that fork.  In this example, the `a<2` dependency is only active on
/// Darwin platforms. But the `a==1.0.0` distribution has a dependency on on `b`
/// that is only active on Linux, where as `a==2.0.0` does not. Therefore, when the
/// fork provoked by the `a<2` dependency considers `b`, it should ignore it because
/// it isn't possible for `sys_platform == 'linux'` and `sys_platform == 'darwin'`
/// to be simultaneously true.
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
    cmd.arg("--index-url")
        .arg("https://astral-sh.github.io/packse/0.3.29/simple-html/");
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
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

        [[distribution]]
        name = "package-a"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_inherit_a-1.0.0.tar.gz#sha256=177511ec69a2f04de39867d43f167a33194ae983e8f86a1cc9b51f59fc379d4b", hash = "sha256:177511ec69a2f04de39867d43f167a33194ae983e8f86a1cc9b51f59fc379d4b" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_inherit_a-1.0.0-py3-none-any.whl#sha256=a7b95b768a7181d9713510b22bb69c5c4f634950e50d13ae44126d800e76abd9", hash = "sha256:a7b95b768a7181d9713510b22bb69c5c4f634950e50d13ae44126d800e76abd9" }]

        [[distribution]]
        name = "package-a"
        version = "2.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_inherit_a-2.0.0.tar.gz#sha256=43e24ce6fcbfbbff1db5eb20b583c20c2aa0888138bfafeab205c4ccc6e7e0a4", hash = "sha256:43e24ce6fcbfbbff1db5eb20b583c20c2aa0888138bfafeab205c4ccc6e7e0a4" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_inherit_a-2.0.0-py3-none-any.whl#sha256=d11786be689f457db5be1729abbb80861259050dbb54e3bb8cfe2b89562db6ec", hash = "sha256:d11786be689f457db5be1729abbb80861259050dbb54e3bb8cfe2b89562db6ec" }]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+."

        [[distribution.dependencies]]
        name = "package-a"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        marker = "sys_platform == 'darwin'"

        [[distribution.dependencies]]
        name = "package-a"
        version = "2.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        marker = "sys_platform == 'linux'"
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
    cmd.arg("--index-url")
        .arg("https://astral-sh.github.io/packse/0.3.29/simple-html/");
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
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

        [[distribution]]
        name = "package-a"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_limited_inherit_a-1.0.0.tar.gz#sha256=ab1fde8d0acb9a2fe99b7a005939962b1c26c6d876e4a55e81fb9d1a1e5e9f76", hash = "sha256:ab1fde8d0acb9a2fe99b7a005939962b1c26c6d876e4a55e81fb9d1a1e5e9f76" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_limited_inherit_a-1.0.0-py3-none-any.whl#sha256=cb79be6fc2fe9c69f19d5452b711d838facf916caddd24dd40b78d34da3930de", hash = "sha256:cb79be6fc2fe9c69f19d5452b711d838facf916caddd24dd40b78d34da3930de" }]

        [[distribution]]
        name = "package-a"
        version = "2.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_limited_inherit_a-2.0.0.tar.gz#sha256=009fdb8872cf52324c1bcdebef31feaba3c262fd76d150a753152aeee3d55b10", hash = "sha256:009fdb8872cf52324c1bcdebef31feaba3c262fd76d150a753152aeee3d55b10" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_limited_inherit_a-2.0.0-py3-none-any.whl#sha256=222092e35ef8ac65f5974f3ea38056ff85a5ce981d84ede5a799a04b5ae50661", hash = "sha256:222092e35ef8ac65f5974f3ea38056ff85a5ce981d84ede5a799a04b5ae50661" }]

        [[distribution]]
        name = "package-b"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_limited_inherit_b-1.0.0.tar.gz#sha256=4c04e090df03e308ecd38a9b8db9813a09fb20a747a89f86c497702c3e5a9001", hash = "sha256:4c04e090df03e308ecd38a9b8db9813a09fb20a747a89f86c497702c3e5a9001" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_limited_inherit_b-1.0.0-py3-none-any.whl#sha256=ee96eb6ec4c7fa40a60feaa557df01550ecbfe1f5e49ec3a9282f243f76a5113", hash = "sha256:ee96eb6ec4c7fa40a60feaa557df01550ecbfe1f5e49ec3a9282f243f76a5113" }]

        [[distribution.dependencies]]
        name = "package-c"
        marker = "sys_platform == 'linux'"

        [[distribution]]
        name = "package-c"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_limited_inherit_c-1.0.0.tar.gz#sha256=8dcb05f5dff09fec52ab507b215ff367fe815848319a17929db997ad3afe88ae", hash = "sha256:8dcb05f5dff09fec52ab507b215ff367fe815848319a17929db997ad3afe88ae" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_limited_inherit_c-1.0.0-py3-none-any.whl#sha256=6870cf7f8eec58dc27a701f9dc0669151e914c013d3e7910953fdfeb387a5e6b", hash = "sha256:6870cf7f8eec58dc27a701f9dc0669151e914c013d3e7910953fdfeb387a5e6b" }]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+."

        [[distribution.dependencies]]
        name = "package-a"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        marker = "sys_platform == 'darwin'"

        [[distribution.dependencies]]
        name = "package-a"
        version = "2.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        marker = "sys_platform == 'linux'"

        [[distribution.dependencies]]
        name = "package-b"
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
    cmd.arg("--index-url")
        .arg("https://astral-sh.github.io/packse/0.3.29/simple-html/");
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
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

        [[distribution]]
        name = "package-a"
        version = "0.1.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_selection_a-0.1.0.tar.gz#sha256=ece83ba864a62d5d747439f79a0bf36aa4c18d15bca96aab855ffc2e94a8eef7", hash = "sha256:ece83ba864a62d5d747439f79a0bf36aa4c18d15bca96aab855ffc2e94a8eef7" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_selection_a-0.1.0-py3-none-any.whl#sha256=8aecc639cc090aa80aa263fb3a9644a7cec9da215133299b8fb381cb7a6bcbb7", hash = "sha256:8aecc639cc090aa80aa263fb3a9644a7cec9da215133299b8fb381cb7a6bcbb7" }]

        [[distribution]]
        name = "package-a"
        version = "0.2.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_selection_a-0.2.0.tar.gz#sha256=42abfb3ce2c13ae008e498d27c80ae39ab19e30fd56e175719b67b1c778ea632", hash = "sha256:42abfb3ce2c13ae008e498d27c80ae39ab19e30fd56e175719b67b1c778ea632" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_selection_a-0.2.0-py3-none-any.whl#sha256=65ff1ce26de8218278abb1ae190fe70d031de79833d85231112208672566b9c4", hash = "sha256:65ff1ce26de8218278abb1ae190fe70d031de79833d85231112208672566b9c4" }]

        [[distribution.dependencies]]
        name = "package-b"
        version = "2.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"

        [[distribution]]
        name = "package-b"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_selection_b-1.0.0.tar.gz#sha256=6f5ea28cadb8b5dfa15d32c9e38818f8f7150fc4f9a58e49aec4e10b23342be4", hash = "sha256:6f5ea28cadb8b5dfa15d32c9e38818f8f7150fc4f9a58e49aec4e10b23342be4" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_selection_b-1.0.0-py3-none-any.whl#sha256=d86ba6d371e152071be1e5bc902a5a54010e94592c8c7e7908870b96ad04d851", hash = "sha256:d86ba6d371e152071be1e5bc902a5a54010e94592c8c7e7908870b96ad04d851" }]

        [[distribution]]
        name = "package-b"
        version = "2.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_selection_b-2.0.0.tar.gz#sha256=d32033ecdf37d605e4b3b3e88df6562bb7ca01c6ed3fb9a55ec078eccc1df9d1", hash = "sha256:d32033ecdf37d605e4b3b3e88df6562bb7ca01c6ed3fb9a55ec078eccc1df9d1" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_selection_b-2.0.0-py3-none-any.whl#sha256=535c038dec0bb33c867ee979fe8863734dd6fb913a94603dcbff42c62790f98b", hash = "sha256:535c038dec0bb33c867ee979fe8863734dd6fb913a94603dcbff42c62790f98b" }]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+."

        [[distribution.dependencies]]
        name = "package-a"
        version = "0.1.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"

        [[distribution.dependencies]]
        name = "package-a"
        version = "0.2.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"

        [[distribution.dependencies]]
        name = "package-b"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        marker = "sys_platform == 'darwin'"

        [[distribution.dependencies]]
        name = "package-b"
        version = "2.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        marker = "sys_platform == 'linux'"
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
    cmd.arg("--index-url")
        .arg("https://astral-sh.github.io/packse/0.3.29/simple-html/");
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
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

        [[distribution]]
        name = "package-a"
        version = "1.3.1"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_track_a-1.3.1.tar.gz#sha256=ffc490c887058825e96a0cc4a270cf56b72f7f28b927c450086603317bb8c120", hash = "sha256:ffc490c887058825e96a0cc4a270cf56b72f7f28b927c450086603317bb8c120" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_track_a-1.3.1-py3-none-any.whl#sha256=79e82592fe6644839cdb6dc73d3d54fc543f0e0f28cce26e221a6c1e30072104", hash = "sha256:79e82592fe6644839cdb6dc73d3d54fc543f0e0f28cce26e221a6c1e30072104" }]

        [[distribution.dependencies]]
        name = "package-c"
        marker = "implementation_name == 'iron'"

        [[distribution]]
        name = "package-a"
        version = "4.3.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_track_a-4.3.0.tar.gz#sha256=ce810c2e0922cff256d3050167c0d2a041955d389d21280fd684ab986dfdb1f5", hash = "sha256:ce810c2e0922cff256d3050167c0d2a041955d389d21280fd684ab986dfdb1f5" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_track_a-4.3.0-py3-none-any.whl#sha256=fb90bca8d00206119df736f59a9c4e18e104a9321b8ea91f19400a119b77ef99", hash = "sha256:fb90bca8d00206119df736f59a9c4e18e104a9321b8ea91f19400a119b77ef99" }]

        [[distribution.dependencies]]
        name = "package-b"
        version = "2.8"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"

        [[distribution]]
        name = "package-b"
        version = "2.7"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_track_b-2.7.tar.gz#sha256=855bf45837a4ba669a5850b14b0253cb138925fdd2b06a2f15c6582b8fabb8a0", hash = "sha256:855bf45837a4ba669a5850b14b0253cb138925fdd2b06a2f15c6582b8fabb8a0" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_track_b-2.7-py3-none-any.whl#sha256=106d0c1c60d67fcf1711029f58f34b770007fed24d087f2fb9cee91226dbdbba", hash = "sha256:106d0c1c60d67fcf1711029f58f34b770007fed24d087f2fb9cee91226dbdbba" }]

        [[distribution]]
        name = "package-b"
        version = "2.8"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_track_b-2.8.tar.gz#sha256=2e14b0ff1fb7f5cf491bd31d876218adee1d6a208ff197dc30363cdf25262e80", hash = "sha256:2e14b0ff1fb7f5cf491bd31d876218adee1d6a208ff197dc30363cdf25262e80" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_track_b-2.8-py3-none-any.whl#sha256=7e2ea2b4f530fa04bec90cf131289ac7eaca1ae38267150587d418d42c814b7c", hash = "sha256:7e2ea2b4f530fa04bec90cf131289ac7eaca1ae38267150587d418d42c814b7c" }]

        [[distribution]]
        name = "package-c"
        version = "1.10"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_track_c-1.10.tar.gz#sha256=c89006d893254790b0fcdd1b33520241c8ff66ab950c6752b745e006bdeff144", hash = "sha256:c89006d893254790b0fcdd1b33520241c8ff66ab950c6752b745e006bdeff144" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.29/files/fork_marker_track_c-1.10-py3-none-any.whl#sha256=6894f06e085884d812ec5e929ed42e7e01a1c7c95fd5ab30781541e4cd94b58d", hash = "sha256:6894f06e085884d812ec5e929ed42e7e01a1c7c95fd5ab30781541e4cd94b58d" }]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+."

        [[distribution.dependencies]]
        name = "package-a"
        version = "1.3.1"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"

        [[distribution.dependencies]]
        name = "package-a"
        version = "4.3.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"

        [[distribution.dependencies]]
        name = "package-b"
        version = "2.7"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        marker = "sys_platform == 'darwin'"

        [[distribution.dependencies]]
        name = "package-b"
        version = "2.8"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        marker = "sys_platform == 'linux'"
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
    cmd.arg("--index-url")
        .arg("https://astral-sh.github.io/packse/0.3.29/simple-html/");
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
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
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.29/files/fork_non_fork_marker_transitive_a-1.0.0.tar.gz#sha256=68cff02c9f4a0b3014fdce524982a3cbf3a2ecaf0291c32c824cadb19f1e7cd0", hash = "sha256:68cff02c9f4a0b3014fdce524982a3cbf3a2ecaf0291c32c824cadb19f1e7cd0" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.29/files/fork_non_fork_marker_transitive_a-1.0.0-py3-none-any.whl#sha256=589fb29588410fe1685650a1151e0f33131c9b295506af6babe16e98dad9da59", hash = "sha256:589fb29588410fe1685650a1151e0f33131c9b295506af6babe16e98dad9da59" }]

        [[distribution.dependencies]]
        name = "package-c"
        marker = "sys_platform == 'linux'"

        [[distribution]]
        name = "package-b"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.29/files/fork_non_fork_marker_transitive_b-1.0.0.tar.gz#sha256=ae7abe9cde79b810f91dff7329b63788a8253250053fe4e82563f0b2d0877182", hash = "sha256:ae7abe9cde79b810f91dff7329b63788a8253250053fe4e82563f0b2d0877182" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.29/files/fork_non_fork_marker_transitive_b-1.0.0-py3-none-any.whl#sha256=545bea70509188de241037b506a1c38dbabb4e52042bb88ca836c04d8103fc48", hash = "sha256:545bea70509188de241037b506a1c38dbabb4e52042bb88ca836c04d8103fc48" }]

        [[distribution.dependencies]]
        name = "package-c"
        marker = "sys_platform == 'darwin'"

        [[distribution]]
        name = "package-c"
        version = "2.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.29/files/fork_non_fork_marker_transitive_c-2.0.0.tar.gz#sha256=ffab9124854f64c8b5059ccaed481547f54abac868ba98aa6a454c0163cdb1c7", hash = "sha256:ffab9124854f64c8b5059ccaed481547f54abac868ba98aa6a454c0163cdb1c7" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.29/files/fork_non_fork_marker_transitive_c-2.0.0-py3-none-any.whl#sha256=80495d1a9075682f6e9dc8d474afd98a3324d32c57c65769b573f281105f3d08", hash = "sha256:80495d1a9075682f6e9dc8d474afd98a3324d32c57c65769b573f281105f3d08" }]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+."

        [[distribution.dependencies]]
        name = "package-a"

        [[distribution.dependencies]]
        name = "package-b"
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
    cmd.arg("--index-url")
        .arg("https://astral-sh.github.io/packse/0.3.29/simple-html/");
    uv_snapshot!(filters, cmd, @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
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
    cmd.arg("--index-url")
        .arg("https://astral-sh.github.io/packse/0.3.29/simple-html/");
    uv_snapshot!(filters, cmd, @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
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
    cmd.arg("--index-url")
        .arg("https://astral-sh.github.io/packse/0.3.29/simple-html/");
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
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
        source = "editable+."
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
    cmd.arg("--index-url")
        .arg("https://astral-sh.github.io/packse/0.3.29/simple-html/");
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
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
        source = "editable+."
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
    cmd.arg("--index-url")
        .arg("https://astral-sh.github.io/packse/0.3.29/simple-html/");
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
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
        source = "registry+https://astral-sh.github.io/packse/0.3.29/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.29/files/fork_requires_python_patch_overlap_a-1.0.0.tar.gz#sha256=ac2820ee4808788674295192d79a709e3259aa4eef5b155e77f719ad4eaa324d", hash = "sha256:ac2820ee4808788674295192d79a709e3259aa4eef5b155e77f719ad4eaa324d" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.29/files/fork_requires_python_patch_overlap_a-1.0.0-py3-none-any.whl#sha256=9c8e127993ded58b011f08453d4103f71f12aa2e8fb61e755061fb56128214e2", hash = "sha256:9c8e127993ded58b011f08453d4103f71f12aa2e8fb61e755061fb56128214e2" }]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+."

        [[distribution.dependencies]]
        name = "package-a"
        marker = "python_version == '3.10'"
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
    cmd.arg("--index-url")
        .arg("https://astral-sh.github.io/packse/0.3.29/simple-html/");
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
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
        source = "editable+."
        "###
        );
    });

    Ok(())
}
