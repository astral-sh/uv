//! DO NOT EDIT
//!
//! Generated with `./scripts/sync_scenarios.sh`
//! Scenarios from <https://github.com/astral-sh/packse/tree/0.3.18/scenarios>
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
        "###,
    )?;

    let mut cmd = context.lock_without_exclude_newer();
    cmd.arg("--index-url")
        .arg("https://astral-sh.github.io/packse/0.3.18/simple-html/");
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
    warning: No `requires-python` field found in `project`. Defaulting to `>=3.8`.
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
        source = "registry+https://astral-sh.github.io/packse/0.3.18/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.18/files/fork_basic_a-1.0.0.tar.gz#sha256=3e45d6136e4a52416f85b7f53f405493db8f9fea33210299e6a68895bf0acf2a", hash = "sha256:3e45d6136e4a52416f85b7f53f405493db8f9fea33210299e6a68895bf0acf2a" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.18/files/fork_basic_a-1.0.0-py3-none-any.whl#sha256=b81a7553af25f15c9d49ed26af9c5b86eb2be107f3dd1bd97d7a4b0e8ca0329e", hash = "sha256:b81a7553af25f15c9d49ed26af9c5b86eb2be107f3dd1bd97d7a4b0e8ca0329e" }]

        [[distribution]]
        name = "package-a"
        version = "2.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.18/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.18/files/fork_basic_a-2.0.0.tar.gz#sha256=ceb7349a6dd7640be952c70dce8ee6a44e3442dfd9b248b96242e37623e1028e", hash = "sha256:ceb7349a6dd7640be952c70dce8ee6a44e3442dfd9b248b96242e37623e1028e" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.18/files/fork_basic_a-2.0.0-py3-none-any.whl#sha256=9cab1de38d28e75ac5fe5c4dda9157555c60dd03ee26e6ad51b01ca18d8a0f01", hash = "sha256:9cab1de38d28e75ac5fe5c4dda9157555c60dd03ee26e6ad51b01ca18d8a0f01" }]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+file://[TEMP_DIR]/"
        sdist = { url = "file://[TEMP_DIR]/" }

        [[distribution.dependencies]]
        name = "package-a"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.18/simple-html/"
        marker = "sys_platform == 'darwin'"

        [[distribution.dependencies]]
        name = "package-a"
        version = "2.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.18/simple-html/"
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
        "###,
    )?;

    let mut cmd = context.lock_without_exclude_newer();
    cmd.arg("--index-url")
        .arg("https://astral-sh.github.io/packse/0.3.18/simple-html/");
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
    warning: No `requires-python` field found in `project`. Defaulting to `>=3.8`.
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
        source = "registry+https://astral-sh.github.io/packse/0.3.18/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.18/files/fork_marker_accrue_a-1.0.0.tar.gz#sha256=9096dbf9c8e8c2da4a1527be515f740f697ee833ec1492953883f36c8931bc37", hash = "sha256:9096dbf9c8e8c2da4a1527be515f740f697ee833ec1492953883f36c8931bc37" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.18/files/fork_marker_accrue_a-1.0.0-py3-none-any.whl#sha256=5fed1607b73cc7a5e9703206c24cc3fa730600a776bf40ae264ad364ad610e0a", hash = "sha256:5fed1607b73cc7a5e9703206c24cc3fa730600a776bf40ae264ad364ad610e0a" }]

        [[distribution.dependencies]]
        name = "package-c"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.18/simple-html/"
        marker = "sys_platform == 'linux'"

        [[distribution]]
        name = "package-b"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.18/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.18/files/fork_marker_accrue_b-1.0.0.tar.gz#sha256=d92d0083d2d5da2f83180c08dfc79a03ec9606c00bc3153566f7b577c0e6b859", hash = "sha256:d92d0083d2d5da2f83180c08dfc79a03ec9606c00bc3153566f7b577c0e6b859" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.18/files/fork_marker_accrue_b-1.0.0-py3-none-any.whl#sha256=e5382e438f417f2de9427296a5960f9f9631ff1fa11c93d6b0b3b9d7fb60760f", hash = "sha256:e5382e438f417f2de9427296a5960f9f9631ff1fa11c93d6b0b3b9d7fb60760f" }]

        [[distribution.dependencies]]
        name = "package-c"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.18/simple-html/"
        marker = "sys_platform == 'darwin'"

        [[distribution]]
        name = "package-c"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.18/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.18/files/fork_marker_accrue_c-1.0.0.tar.gz#sha256=81068ae8b43deb3165cab17eb52aa5f99cda64f51c359b4659918d86995b9cad", hash = "sha256:81068ae8b43deb3165cab17eb52aa5f99cda64f51c359b4659918d86995b9cad" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.18/files/fork_marker_accrue_c-1.0.0-py3-none-any.whl#sha256=f5fe6d35f360ea802b3a7da030e9ed1dce776c30ed028ea7be04fafcb7ac55b6", hash = "sha256:f5fe6d35f360ea802b3a7da030e9ed1dce776c30ed028ea7be04fafcb7ac55b6" }]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+file://[TEMP_DIR]/"
        sdist = { url = "file://[TEMP_DIR]/" }

        [[distribution.dependencies]]
        name = "package-a"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.18/simple-html/"
        marker = "implementation_name == 'cpython'"

        [[distribution.dependencies]]
        name = "package-b"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.18/simple-html/"
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
        "###,
    )?;

    let mut cmd = context.lock_without_exclude_newer();
    cmd.arg("--index-url")
        .arg("https://astral-sh.github.io/packse/0.3.18/simple-html/");
    uv_snapshot!(filters, cmd, @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
    warning: No `requires-python` field found in `project`. Defaulting to `>=3.8`.
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
        "###,
    )?;

    let mut cmd = context.lock_without_exclude_newer();
    cmd.arg("--index-url")
        .arg("https://astral-sh.github.io/packse/0.3.18/simple-html/");
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
    warning: No `requires-python` field found in `project`. Defaulting to `>=3.8`.
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
        source = "registry+https://astral-sh.github.io/packse/0.3.18/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.18/files/fork_marker_selection_a-0.1.0.tar.gz#sha256=03c464276ee75f5a1468da2a4090ee6b5fda0f26f548707c9ffcf06d3cf69282", hash = "sha256:03c464276ee75f5a1468da2a4090ee6b5fda0f26f548707c9ffcf06d3cf69282" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.18/files/fork_marker_selection_a-0.1.0-py3-none-any.whl#sha256=0e45ca7b3616810a583dc9754b52b91c69aeea4070d6fe0806c67081d0e95473", hash = "sha256:0e45ca7b3616810a583dc9754b52b91c69aeea4070d6fe0806c67081d0e95473" }]

        [[distribution]]
        name = "package-a"
        version = "0.2.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.18/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.18/files/fork_marker_selection_a-0.2.0.tar.gz#sha256=ef1d840fe2e86c6eecd4673606076d858b51a3712c1de097b7503fee0c96b97f", hash = "sha256:ef1d840fe2e86c6eecd4673606076d858b51a3712c1de097b7503fee0c96b97f" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.18/files/fork_marker_selection_a-0.2.0-py3-none-any.whl#sha256=78797f388900cece9866aa20917c6a40040dd65f906f8ef034a8cedb4dd75e6c", hash = "sha256:78797f388900cece9866aa20917c6a40040dd65f906f8ef034a8cedb4dd75e6c" }]

        [[distribution.dependencies]]
        name = "package-b"
        version = "2.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.18/simple-html/"

        [[distribution]]
        name = "package-b"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.18/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.18/files/fork_marker_selection_b-1.0.0.tar.gz#sha256=97f1098f4c89457ab2b16982990d487ac6ae2c664f8e22e822a086df71999dc1", hash = "sha256:97f1098f4c89457ab2b16982990d487ac6ae2c664f8e22e822a086df71999dc1" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.18/files/fork_marker_selection_b-1.0.0-py3-none-any.whl#sha256=aba998c3dfa70f4118a4587f636c96f5a2785081b733120cf81b6d762f67b1ca", hash = "sha256:aba998c3dfa70f4118a4587f636c96f5a2785081b733120cf81b6d762f67b1ca" }]

        [[distribution]]
        name = "package-b"
        version = "2.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.18/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.18/files/fork_marker_selection_b-2.0.0.tar.gz#sha256=1f66e4ba827d2913827fa52cc9fd08491b16ab409fa31c40a2fe4e3cde91cb4a", hash = "sha256:1f66e4ba827d2913827fa52cc9fd08491b16ab409fa31c40a2fe4e3cde91cb4a" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.18/files/fork_marker_selection_b-2.0.0-py3-none-any.whl#sha256=ad1b23547813b9ac69b33d3fcf1896cd49a90cd8f957e954dbdd77b628d631cf", hash = "sha256:ad1b23547813b9ac69b33d3fcf1896cd49a90cd8f957e954dbdd77b628d631cf" }]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+file://[TEMP_DIR]/"
        sdist = { url = "file://[TEMP_DIR]/" }

        [[distribution.dependencies]]
        name = "package-a"
        version = "0.1.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.18/simple-html/"

        [[distribution.dependencies]]
        name = "package-a"
        version = "0.2.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.18/simple-html/"

        [[distribution.dependencies]]
        name = "package-b"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.18/simple-html/"
        marker = "sys_platform == 'darwin'"

        [[distribution.dependencies]]
        name = "package-b"
        version = "2.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.18/simple-html/"
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
        "###,
    )?;

    let mut cmd = context.lock_without_exclude_newer();
    cmd.arg("--index-url")
        .arg("https://astral-sh.github.io/packse/0.3.18/simple-html/");
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
    warning: No `requires-python` field found in `project`. Defaulting to `>=3.8`.
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
        source = "registry+https://astral-sh.github.io/packse/0.3.18/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.18/files/fork_marker_track_a-1.3.1.tar.gz#sha256=b88e1c256f2f3b2f3d0cff5398fd6a1a17682f3b5fd736e08d44c313ed48ef37", hash = "sha256:b88e1c256f2f3b2f3d0cff5398fd6a1a17682f3b5fd736e08d44c313ed48ef37" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.18/files/fork_marker_track_a-1.3.1-py3-none-any.whl#sha256=8f2bd8bcd8f3fc2cfe64621d62a3a9404db665830f7a76db60307a80cf8e632f", hash = "sha256:8f2bd8bcd8f3fc2cfe64621d62a3a9404db665830f7a76db60307a80cf8e632f" }]

        [[distribution.dependencies]]
        name = "package-c"
        version = "1.10"
        source = "registry+https://astral-sh.github.io/packse/0.3.18/simple-html/"
        marker = "implementation_name == 'iron'"

        [[distribution]]
        name = "package-a"
        version = "4.3.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.18/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.18/files/fork_marker_track_a-4.3.0.tar.gz#sha256=46a0ab5d6b934f2b8c762893660483036a81ac1f8df9a6555e72a3b4859e1a75", hash = "sha256:46a0ab5d6b934f2b8c762893660483036a81ac1f8df9a6555e72a3b4859e1a75" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.18/files/fork_marker_track_a-4.3.0-py3-none-any.whl#sha256=73ad4b017bae8cb4743be03bc406f65594c92ec5038b0f56a4acb07873bfcaa5", hash = "sha256:73ad4b017bae8cb4743be03bc406f65594c92ec5038b0f56a4acb07873bfcaa5" }]

        [[distribution.dependencies]]
        name = "package-b"
        version = "2.8"
        source = "registry+https://astral-sh.github.io/packse/0.3.18/simple-html/"

        [[distribution]]
        name = "package-b"
        version = "2.7"
        source = "registry+https://astral-sh.github.io/packse/0.3.18/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.18/files/fork_marker_track_b-2.7.tar.gz#sha256=25258fd52c9611c9e101138f9986ada5930f5bea08988d0356645c772a8162dd", hash = "sha256:25258fd52c9611c9e101138f9986ada5930f5bea08988d0356645c772a8162dd" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.18/files/fork_marker_track_b-2.7-py3-none-any.whl#sha256=be56f5850a343cb02dfc22e75eaa1009db675ac2f1275b78ba4089c6ea2f2808", hash = "sha256:be56f5850a343cb02dfc22e75eaa1009db675ac2f1275b78ba4089c6ea2f2808" }]

        [[distribution]]
        name = "package-b"
        version = "2.8"
        source = "registry+https://astral-sh.github.io/packse/0.3.18/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.18/files/fork_marker_track_b-2.8.tar.gz#sha256=7ec0f88f013fa0b75a4c88097799866617de4cae558b18ad0677f7cc65ad6628", hash = "sha256:7ec0f88f013fa0b75a4c88097799866617de4cae558b18ad0677f7cc65ad6628" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.18/files/fork_marker_track_b-2.8-py3-none-any.whl#sha256=d9969066117d846fe3a200df5bafc3b3279cc419f36f7275e6e55b2dbde2d5d1", hash = "sha256:d9969066117d846fe3a200df5bafc3b3279cc419f36f7275e6e55b2dbde2d5d1" }]

        [[distribution]]
        name = "package-c"
        version = "1.10"
        source = "registry+https://astral-sh.github.io/packse/0.3.18/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.18/files/fork_marker_track_c-1.10.tar.gz#sha256=6f4a62bec34fbda0e605dc9acb40af318b1d789816d81cbd0bc7c60595de5930", hash = "sha256:6f4a62bec34fbda0e605dc9acb40af318b1d789816d81cbd0bc7c60595de5930" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.18/files/fork_marker_track_c-1.10-py3-none-any.whl#sha256=19791f8bd3bad9a76be5477e1753dc2a4e797d163bef90fdfd99462c271ed6ff", hash = "sha256:19791f8bd3bad9a76be5477e1753dc2a4e797d163bef90fdfd99462c271ed6ff" }]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+file://[TEMP_DIR]/"
        sdist = { url = "file://[TEMP_DIR]/" }

        [[distribution.dependencies]]
        name = "package-a"
        version = "1.3.1"
        source = "registry+https://astral-sh.github.io/packse/0.3.18/simple-html/"

        [[distribution.dependencies]]
        name = "package-a"
        version = "4.3.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.18/simple-html/"

        [[distribution.dependencies]]
        name = "package-b"
        version = "2.7"
        source = "registry+https://astral-sh.github.io/packse/0.3.18/simple-html/"
        marker = "sys_platform == 'darwin'"

        [[distribution.dependencies]]
        name = "package-b"
        version = "2.8"
        source = "registry+https://astral-sh.github.io/packse/0.3.18/simple-html/"
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
        "###,
    )?;

    let mut cmd = context.lock_without_exclude_newer();
    cmd.arg("--index-url")
        .arg("https://astral-sh.github.io/packse/0.3.18/simple-html/");
    uv_snapshot!(filters, cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
    warning: No `requires-python` field found in `project`. Defaulting to `>=3.8`.
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
        source = "registry+https://astral-sh.github.io/packse/0.3.18/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.18/files/fork_non_fork_marker_transitive_a-1.0.0.tar.gz#sha256=017f775164ac5e33682262bbd44922938737bb8d7258161abb65d8d22f7f0749", hash = "sha256:017f775164ac5e33682262bbd44922938737bb8d7258161abb65d8d22f7f0749" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.18/files/fork_non_fork_marker_transitive_a-1.0.0-py3-none-any.whl#sha256=d0ffdf00cba31099cc02d1419f1d2a0c8add5efe7c916b5e12bc23c8f7fdfb4c", hash = "sha256:d0ffdf00cba31099cc02d1419f1d2a0c8add5efe7c916b5e12bc23c8f7fdfb4c" }]

        [[distribution.dependencies]]
        name = "package-c"
        version = "2.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.18/simple-html/"
        marker = "sys_platform == 'linux'"

        [[distribution]]
        name = "package-b"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.18/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.18/files/fork_non_fork_marker_transitive_b-1.0.0.tar.gz#sha256=f930b038c81f712230deda8d3b7d2a9a9758b71e86313722747e0ecd44d86e4a", hash = "sha256:f930b038c81f712230deda8d3b7d2a9a9758b71e86313722747e0ecd44d86e4a" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.18/files/fork_non_fork_marker_transitive_b-1.0.0-py3-none-any.whl#sha256=d50cf9f9bcff0c90e969d6eba899bbbcb3c09666217c2c9a8011cdef089070a4", hash = "sha256:d50cf9f9bcff0c90e969d6eba899bbbcb3c09666217c2c9a8011cdef089070a4" }]

        [[distribution.dependencies]]
        name = "package-c"
        version = "2.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.18/simple-html/"
        marker = "sys_platform == 'darwin'"

        [[distribution]]
        name = "package-c"
        version = "2.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.18/simple-html/"
        sdist = { url = "https://astral-sh.github.io/packse/0.3.18/files/fork_non_fork_marker_transitive_c-2.0.0.tar.gz#sha256=c989314fe5534401e9b2374e9b0461c9d44c237853d9122bc7d9aee006ee0c34", hash = "sha256:c989314fe5534401e9b2374e9b0461c9d44c237853d9122bc7d9aee006ee0c34" }
        wheels = [{ url = "https://astral-sh.github.io/packse/0.3.18/files/fork_non_fork_marker_transitive_c-2.0.0-py3-none-any.whl#sha256=661def8c77b372df8146049485a75678ecee810518fb7cba024b609920bdef74", hash = "sha256:661def8c77b372df8146049485a75678ecee810518fb7cba024b609920bdef74" }]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+file://[TEMP_DIR]/"
        sdist = { url = "file://[TEMP_DIR]/" }

        [[distribution.dependencies]]
        name = "package-a"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.18/simple-html/"

        [[distribution.dependencies]]
        name = "package-b"
        version = "1.0.0"
        source = "registry+https://astral-sh.github.io/packse/0.3.18/simple-html/"
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
        "###,
    )?;

    let mut cmd = context.lock_without_exclude_newer();
    cmd.arg("--index-url")
        .arg("https://astral-sh.github.io/packse/0.3.18/simple-html/");
    uv_snapshot!(filters, cmd, @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
    warning: No `requires-python` field found in `project`. Defaulting to `>=3.8`.
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
        "###,
    )?;

    let mut cmd = context.lock_without_exclude_newer();
    cmd.arg("--index-url")
        .arg("https://astral-sh.github.io/packse/0.3.18/simple-html/");
    uv_snapshot!(filters, cmd, @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
    warning: No `requires-python` field found in `project`. Defaulting to `>=3.8`.
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
