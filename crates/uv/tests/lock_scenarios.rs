//! DO NOT EDIT
//!
//! Generated with `./scripts/sync_scenarios.sh`
//! Scenarios from <https://github.com/astral-sh/packse/tree/0.3.24/scenarios>
//!
#![cfg(all(feature = "python", feature = "pypi"))]
#![allow(clippy::needless_raw_string_hashes)]

use anyhow::Result;
use assert_fs::prelude::*;
use insta::assert_snapshot;

use common::{uv_snapshot, TestContext};

mod common;

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

    let mut cmd = context.lock_without_exclude_newer();
    cmd.arg("--index-url")
        .arg("https://astral-sh.github.io/packse/0.3.24/simple-html/");
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
        source = "registry+https://astral-sh.github.io/packse/0.3.24/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.24/files/fork_basic_a-1.0.0.tar.gz#sha256=9bd6d9d74d8928854f79ea3ed4cd0d8a4906eeaa40f5f3d63460a1c2d5f6d773", hash = "sha256:9bd6d9d74d8928854f79ea3ed4cd0d8a4906eeaa40f5f3d63460a1c2d5f6d773" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.24/files/fork_basic_a-1.0.0-py3-none-any.whl#sha256=1a28e30240634de42f24d34ff9bdac181208ef57215def75baac0de205685d27", hash = "sha256:1a28e30240634de42f24d34ff9bdac181208ef57215def75baac0de205685d27" }]

        [[distribution]]
        name = "package-a"
        version = "2.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.24/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.24/files/fork_basic_a-2.0.0.tar.gz#sha256=c0ce6dfb6d712eb42a4bbe9402a1f823627b9d3773f31d259c49478fc7d8d082", hash = "sha256:c0ce6dfb6d712eb42a4bbe9402a1f823627b9d3773f31d259c49478fc7d8d082" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.24/files/fork_basic_a-2.0.0-py3-none-any.whl#sha256=c830122b1b31a6229208e04ced83ae4d1cef8be28b857b56af054bb4bd868a30", hash = "sha256:c830122b1b31a6229208e04ced83ae4d1cef8be28b857b56af054bb4bd868a30" }]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+."
        sdist = { path = "." }

        [[distribution.dependencies]]
        name = "package-a"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.24/simple-html/"
        marker = "sys_platform == 'darwin'"

        [[distribution.dependencies]]
        name = "package-a"
        version = "2.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.24/simple-html/"
        marker = "sys_platform == 'linux'"
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

    let mut cmd = context.lock_without_exclude_newer();
    cmd.arg("--index-url")
        .arg("https://astral-sh.github.io/packse/0.3.24/simple-html/");
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
        source = "registry+https://astral-sh.github.io/packse/0.3.24/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.24/files/fork_marker_accrue_a-1.0.0.tar.gz#sha256=c791e6062a510c63bff857ca6f7a921a7795bfbc588f21a51124e091fb0343d6", hash = "sha256:c791e6062a510c63bff857ca6f7a921a7795bfbc588f21a51124e091fb0343d6" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.24/files/fork_marker_accrue_a-1.0.0-py3-none-any.whl#sha256=c1b40f368e7be6c16c1d537481421bf6cd1e18a09a83f42ed35029633d4b3249", hash = "sha256:c1b40f368e7be6c16c1d537481421bf6cd1e18a09a83f42ed35029633d4b3249" }]

        [[distribution.dependencies]]
        name = "package-c"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.24/simple-html/"
        marker = "sys_platform == 'linux'"

        [[distribution]]
        name = "package-b"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.24/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.24/files/fork_marker_accrue_b-1.0.0.tar.gz#sha256=32e7ea1022061783857c3f6fec5051b4b320630fe8a5aec6523cd565db350387", hash = "sha256:32e7ea1022061783857c3f6fec5051b4b320630fe8a5aec6523cd565db350387" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.24/files/fork_marker_accrue_b-1.0.0-py3-none-any.whl#sha256=2df49decd1b188800d1cbf5265806d32388e4c792da3074b6f9be2e7b72185f5", hash = "sha256:2df49decd1b188800d1cbf5265806d32388e4c792da3074b6f9be2e7b72185f5" }]

        [[distribution.dependencies]]
        name = "package-c"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.24/simple-html/"
        marker = "sys_platform == 'darwin'"

        [[distribution]]
        name = "package-c"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.24/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.24/files/fork_marker_accrue_c-1.0.0.tar.gz#sha256=a3e09ac3dc8e787a08ebe8d5d6072e09720c76cbbcb76a6645d6f59652742015", hash = "sha256:a3e09ac3dc8e787a08ebe8d5d6072e09720c76cbbcb76a6645d6f59652742015" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.24/files/fork_marker_accrue_c-1.0.0-py3-none-any.whl#sha256=01993b60f134b3b80585fe95e3511b9a6194c2387c0215d962dbf65abd5a5fe1", hash = "sha256:01993b60f134b3b80585fe95e3511b9a6194c2387c0215d962dbf65abd5a5fe1" }]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+."
        sdist = { path = "." }

        [[distribution.dependencies]]
        name = "package-a"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.24/simple-html/"
        marker = "implementation_name == 'cpython'"

        [[distribution.dependencies]]
        name = "package-b"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.24/simple-html/"
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

    let mut cmd = context.lock_without_exclude_newer();
    cmd.arg("--index-url")
        .arg("https://astral-sh.github.io/packse/0.3.24/simple-html/");
    uv_snapshot!(filters, cmd, @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
      × No solution found when resolving dependencies:
      ╰─▶ Because project==0.1.0 depends on package-a{sys_platform == 'linux'}>=2 and package-a{sys_platform == 'linux'}<2, we can conclude that project==0.1.0 cannot be used.
          And because only project==0.1.0 is available and project depends on project, we can conclude that the requirements are unsatisfiable.
    "###
    );

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

    let mut cmd = context.lock_without_exclude_newer();
    cmd.arg("--index-url")
        .arg("https://astral-sh.github.io/packse/0.3.24/simple-html/");
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
        source = "registry+https://astral-sh.github.io/packse/0.3.24/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.24/files/fork_marker_selection_a-0.1.0.tar.gz#sha256=ece83ba864a62d5d747439f79a0bf36aa4c18d15bca96aab855ffc2e94a8eef7", hash = "sha256:ece83ba864a62d5d747439f79a0bf36aa4c18d15bca96aab855ffc2e94a8eef7" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.24/files/fork_marker_selection_a-0.1.0-py3-none-any.whl#sha256=8aecc639cc090aa80aa263fb3a9644a7cec9da215133299b8fb381cb7a6bcbb7", hash = "sha256:8aecc639cc090aa80aa263fb3a9644a7cec9da215133299b8fb381cb7a6bcbb7" }]

        [[distribution]]
        name = "package-a"
        version = "0.2.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.24/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.24/files/fork_marker_selection_a-0.2.0.tar.gz#sha256=42abfb3ce2c13ae008e498d27c80ae39ab19e30fd56e175719b67b1c778ea632", hash = "sha256:42abfb3ce2c13ae008e498d27c80ae39ab19e30fd56e175719b67b1c778ea632" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.24/files/fork_marker_selection_a-0.2.0-py3-none-any.whl#sha256=65ff1ce26de8218278abb1ae190fe70d031de79833d85231112208672566b9c4", hash = "sha256:65ff1ce26de8218278abb1ae190fe70d031de79833d85231112208672566b9c4" }]

        [[distribution.dependencies]]
        name = "package-b"
        version = "2.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.24/simple-html/"

        [[distribution]]
        name = "package-b"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.24/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.24/files/fork_marker_selection_b-1.0.0.tar.gz#sha256=6f5ea28cadb8b5dfa15d32c9e38818f8f7150fc4f9a58e49aec4e10b23342be4", hash = "sha256:6f5ea28cadb8b5dfa15d32c9e38818f8f7150fc4f9a58e49aec4e10b23342be4" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.24/files/fork_marker_selection_b-1.0.0-py3-none-any.whl#sha256=d86ba6d371e152071be1e5bc902a5a54010e94592c8c7e7908870b96ad04d851", hash = "sha256:d86ba6d371e152071be1e5bc902a5a54010e94592c8c7e7908870b96ad04d851" }]

        [[distribution]]
        name = "package-b"
        version = "2.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.24/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.24/files/fork_marker_selection_b-2.0.0.tar.gz#sha256=d32033ecdf37d605e4b3b3e88df6562bb7ca01c6ed3fb9a55ec078eccc1df9d1", hash = "sha256:d32033ecdf37d605e4b3b3e88df6562bb7ca01c6ed3fb9a55ec078eccc1df9d1" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.24/files/fork_marker_selection_b-2.0.0-py3-none-any.whl#sha256=535c038dec0bb33c867ee979fe8863734dd6fb913a94603dcbff42c62790f98b", hash = "sha256:535c038dec0bb33c867ee979fe8863734dd6fb913a94603dcbff42c62790f98b" }]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+."
        sdist = { path = "." }

        [[distribution.dependencies]]
        name = "package-a"
        version = "0.1.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.24/simple-html/"

        [[distribution.dependencies]]
        name = "package-a"
        version = "0.2.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.24/simple-html/"

        [[distribution.dependencies]]
        name = "package-b"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.24/simple-html/"
        marker = "sys_platform == 'darwin'"

        [[distribution.dependencies]]
        name = "package-b"
        version = "2.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.24/simple-html/"
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

    let mut cmd = context.lock_without_exclude_newer();
    cmd.arg("--index-url")
        .arg("https://astral-sh.github.io/packse/0.3.24/simple-html/");
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
        source = "registry+https://astral-sh.github.io/packse/0.3.24/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.24/files/fork_marker_track_a-1.3.1.tar.gz#sha256=ffc490c887058825e96a0cc4a270cf56b72f7f28b927c450086603317bb8c120", hash = "sha256:ffc490c887058825e96a0cc4a270cf56b72f7f28b927c450086603317bb8c120" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.24/files/fork_marker_track_a-1.3.1-py3-none-any.whl#sha256=79e82592fe6644839cdb6dc73d3d54fc543f0e0f28cce26e221a6c1e30072104", hash = "sha256:79e82592fe6644839cdb6dc73d3d54fc543f0e0f28cce26e221a6c1e30072104" }]

        [[distribution.dependencies]]
        name = "package-c"
        version = "1.10"
        source = "registry+https://astral-sh.github.io/packse/0.3.24/simple-html/"
        marker = "implementation_name == 'iron'"

        [[distribution]]
        name = "package-a"
        version = "4.3.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.24/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.24/files/fork_marker_track_a-4.3.0.tar.gz#sha256=ce810c2e0922cff256d3050167c0d2a041955d389d21280fd684ab986dfdb1f5", hash = "sha256:ce810c2e0922cff256d3050167c0d2a041955d389d21280fd684ab986dfdb1f5" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.24/files/fork_marker_track_a-4.3.0-py3-none-any.whl#sha256=fb90bca8d00206119df736f59a9c4e18e104a9321b8ea91f19400a119b77ef99", hash = "sha256:fb90bca8d00206119df736f59a9c4e18e104a9321b8ea91f19400a119b77ef99" }]

        [[distribution.dependencies]]
        name = "package-b"
        version = "2.8"
        source = "registry+https://astral-sh.github.io/packse/0.3.24/simple-html/"

        [[distribution]]
        name = "package-b"
        version = "2.7"
        source = "registry+https://astral-sh.github.io/packse/0.3.24/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.24/files/fork_marker_track_b-2.7.tar.gz#sha256=855bf45837a4ba669a5850b14b0253cb138925fdd2b06a2f15c6582b8fabb8a0", hash = "sha256:855bf45837a4ba669a5850b14b0253cb138925fdd2b06a2f15c6582b8fabb8a0" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.24/files/fork_marker_track_b-2.7-py3-none-any.whl#sha256=106d0c1c60d67fcf1711029f58f34b770007fed24d087f2fb9cee91226dbdbba", hash = "sha256:106d0c1c60d67fcf1711029f58f34b770007fed24d087f2fb9cee91226dbdbba" }]

        [[distribution]]
        name = "package-b"
        version = "2.8"
        source = "registry+https://astral-sh.github.io/packse/0.3.24/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.24/files/fork_marker_track_b-2.8.tar.gz#sha256=2e14b0ff1fb7f5cf491bd31d876218adee1d6a208ff197dc30363cdf25262e80", hash = "sha256:2e14b0ff1fb7f5cf491bd31d876218adee1d6a208ff197dc30363cdf25262e80" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.24/files/fork_marker_track_b-2.8-py3-none-any.whl#sha256=7e2ea2b4f530fa04bec90cf131289ac7eaca1ae38267150587d418d42c814b7c", hash = "sha256:7e2ea2b4f530fa04bec90cf131289ac7eaca1ae38267150587d418d42c814b7c" }]

        [[distribution]]
        name = "package-c"
        version = "1.10"
        source = "registry+https://astral-sh.github.io/packse/0.3.24/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.24/files/fork_marker_track_c-1.10.tar.gz#sha256=c89006d893254790b0fcdd1b33520241c8ff66ab950c6752b745e006bdeff144", hash = "sha256:c89006d893254790b0fcdd1b33520241c8ff66ab950c6752b745e006bdeff144" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.24/files/fork_marker_track_c-1.10-py3-none-any.whl#sha256=6894f06e085884d812ec5e929ed42e7e01a1c7c95fd5ab30781541e4cd94b58d", hash = "sha256:6894f06e085884d812ec5e929ed42e7e01a1c7c95fd5ab30781541e4cd94b58d" }]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+."
        sdist = { path = "." }

        [[distribution.dependencies]]
        name = "package-a"
        version = "1.3.1"
        source = "registry+https://astral-sh.github.io/packse/0.3.24/simple-html/"

        [[distribution.dependencies]]
        name = "package-a"
        version = "4.3.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.24/simple-html/"

        [[distribution.dependencies]]
        name = "package-b"
        version = "2.7"
        source = "registry+https://astral-sh.github.io/packse/0.3.24/simple-html/"
        marker = "sys_platform == 'darwin'"

        [[distribution.dependencies]]
        name = "package-b"
        version = "2.8"
        source = "registry+https://astral-sh.github.io/packse/0.3.24/simple-html/"
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

    let mut cmd = context.lock_without_exclude_newer();
    cmd.arg("--index-url")
        .arg("https://astral-sh.github.io/packse/0.3.24/simple-html/");
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
        source = "registry+https://astral-sh.github.io/packse/0.3.24/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.24/files/fork_non_fork_marker_transitive_a-1.0.0.tar.gz#sha256=68cff02c9f4a0b3014fdce524982a3cbf3a2ecaf0291c32c824cadb19f1e7cd0", hash = "sha256:68cff02c9f4a0b3014fdce524982a3cbf3a2ecaf0291c32c824cadb19f1e7cd0" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.24/files/fork_non_fork_marker_transitive_a-1.0.0-py3-none-any.whl#sha256=589fb29588410fe1685650a1151e0f33131c9b295506af6babe16e98dad9da59", hash = "sha256:589fb29588410fe1685650a1151e0f33131c9b295506af6babe16e98dad9da59" }]

        [[distribution.dependencies]]
        name = "package-c"
        version = "2.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.24/simple-html/"
        marker = "sys_platform == 'linux'"

        [[distribution]]
        name = "package-b"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.24/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.24/files/fork_non_fork_marker_transitive_b-1.0.0.tar.gz#sha256=ae7abe9cde79b810f91dff7329b63788a8253250053fe4e82563f0b2d0877182", hash = "sha256:ae7abe9cde79b810f91dff7329b63788a8253250053fe4e82563f0b2d0877182" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.24/files/fork_non_fork_marker_transitive_b-1.0.0-py3-none-any.whl#sha256=545bea70509188de241037b506a1c38dbabb4e52042bb88ca836c04d8103fc48", hash = "sha256:545bea70509188de241037b506a1c38dbabb4e52042bb88ca836c04d8103fc48" }]

        [[distribution.dependencies]]
        name = "package-c"
        version = "2.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.24/simple-html/"
        marker = "sys_platform == 'darwin'"

        [[distribution]]
        name = "package-c"
        version = "2.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.24/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.24/files/fork_non_fork_marker_transitive_c-2.0.0.tar.gz#sha256=ffab9124854f64c8b5059ccaed481547f54abac868ba98aa6a454c0163cdb1c7", hash = "sha256:ffab9124854f64c8b5059ccaed481547f54abac868ba98aa6a454c0163cdb1c7" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.24/files/fork_non_fork_marker_transitive_c-2.0.0-py3-none-any.whl#sha256=80495d1a9075682f6e9dc8d474afd98a3324d32c57c65769b573f281105f3d08", hash = "sha256:80495d1a9075682f6e9dc8d474afd98a3324d32c57c65769b573f281105f3d08" }]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+."
        sdist = { path = "." }

        [[distribution.dependencies]]
        name = "package-a"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.24/simple-html/"

        [[distribution.dependencies]]
        name = "package-b"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.24/simple-html/"
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

    let mut cmd = context.lock_without_exclude_newer();
    cmd.arg("--index-url")
        .arg("https://astral-sh.github.io/packse/0.3.24/simple-html/");
    uv_snapshot!(filters, cmd, @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
      × No solution found when resolving dependencies:
      ╰─▶ Because package-b{sys_platform == 'darwin'}==1.0.0 depends on package-c>=2.0.0 and package-a{sys_platform == 'linux'}==1.0.0 depends on package-c<2.0.0, we can conclude that package-a{sys_platform == 'linux'}==1.0.0 and package-b{sys_platform == 'darwin'}==1.0.0 are incompatible.
          And because project==0.1.0 depends on package-a{sys_platform == 'linux'}==1.0.0 and package-b{sys_platform == 'darwin'}==1.0.0, we can conclude that project==0.1.0 cannot be used.
          And because only project==0.1.0 is available and project depends on project, we can conclude that the requirements are unsatisfiable.
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

    let mut cmd = context.lock_without_exclude_newer();
    cmd.arg("--index-url")
        .arg("https://astral-sh.github.io/packse/0.3.24/simple-html/");
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
          And because only project==0.1.0 is available and project depends on project, we can conclude that the requirements are unsatisfiable.
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

    let mut cmd = context.lock_without_exclude_newer();
    cmd.arg("--index-url")
        .arg("https://astral-sh.github.io/packse/0.3.24/simple-html/");
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
        sdist = { path = "." }
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

    let mut cmd = context.lock_without_exclude_newer();
    cmd.arg("--index-url")
        .arg("https://astral-sh.github.io/packse/0.3.24/simple-html/");
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
        sdist = { path = "." }
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

    let mut cmd = context.lock_without_exclude_newer();
    cmd.arg("--index-url")
        .arg("https://astral-sh.github.io/packse/0.3.24/simple-html/");
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
        source = "registry+https://astral-sh.github.io/packse/0.3.24/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.24/files/fork_requires_python_patch_overlap_a-1.0.0.tar.gz#sha256=ac2820ee4808788674295192d79a709e3259aa4eef5b155e77f719ad4eaa324d", hash = "sha256:ac2820ee4808788674295192d79a709e3259aa4eef5b155e77f719ad4eaa324d" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.24/files/fork_requires_python_patch_overlap_a-1.0.0-py3-none-any.whl#sha256=9c8e127993ded58b011f08453d4103f71f12aa2e8fb61e755061fb56128214e2", hash = "sha256:9c8e127993ded58b011f08453d4103f71f12aa2e8fb61e755061fb56128214e2" }]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+."
        sdist = { path = "." }

        [[distribution.dependencies]]
        name = "package-a"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.24/simple-html/"
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

    let mut cmd = context.lock_without_exclude_newer();
    cmd.arg("--index-url")
        .arg("https://astral-sh.github.io/packse/0.3.24/simple-html/");
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
        sdist = { path = "." }
        "###
        );
    });

    Ok(())
}
