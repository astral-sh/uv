#![cfg(all(feature = "python", feature = "pypi"))]

use assert_fs::prelude::PathChild;
use assert_fs::{fixture::FileWriteStr, prelude::PathCreateDir};
use fs_err::remove_dir_all;
use indoc::indoc;

use common::{uv_snapshot, TestContext};
use uv_python::platform::{Arch, Os};

mod common;

#[test]
fn python_find() {
    let mut context: TestContext = TestContext::new_with_versions(&["3.11", "3.12"]);

    // No interpreters on the path
    if cfg!(windows) {
        uv_snapshot!(context.filters(), context.python_find().env("UV_TEST_PYTHON_PATH", ""), @r###"
        success: false
        exit_code: 2
        ----- stdout -----

        ----- stderr -----
        error: No interpreter found in virtual environments, system path, or `py` launcher
        "###);
    } else {
        uv_snapshot!(context.filters(), context.python_find().env("UV_TEST_PYTHON_PATH", ""), @r###"
        success: false
        exit_code: 2
        ----- stdout -----

        ----- stderr -----
        error: No interpreter found in virtual environments or system path
        "###);
    }

    // We find the first interpreter on the path
    uv_snapshot!(context.filters(), context.python_find(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.11]

    ----- stderr -----
    "###);

    // Request Python 3.12
    uv_snapshot!(context.filters(), context.python_find().arg("3.12"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.12]

    ----- stderr -----
    "###);

    // Request Python 3.11
    uv_snapshot!(context.filters(), context.python_find().arg("3.11"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.11]

    ----- stderr -----
    "###);

    // Request CPython
    uv_snapshot!(context.filters(), context.python_find().arg("cpython"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.11]

    ----- stderr -----
    "###);

    // Request CPython 3.12
    uv_snapshot!(context.filters(), context.python_find().arg("cpython@3.12"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.12]

    ----- stderr -----
    "###);

    // Request CPython 3.12 via partial key syntax
    uv_snapshot!(context.filters(), context.python_find().arg("cpython-3.12"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.12]

    ----- stderr -----
    "###);

    // Request CPython 3.12 for the current platform
    let os = Os::from_env();
    let arch = Arch::from_env();

    uv_snapshot!(context.filters(), context.python_find()
    .arg(format!("cpython-3.12-{os}-{arch}"))
    , @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.12]

    ----- stderr -----
    "###);

    // Request PyPy (which should be missing)
    if cfg!(windows) {
        uv_snapshot!(context.filters(), context.python_find().arg("pypy"), @r###"
        success: false
        exit_code: 2
        ----- stdout -----

        ----- stderr -----
        error: No interpreter found for PyPy in virtual environments, system path, or `py` launcher
        "###);
    } else {
        uv_snapshot!(context.filters(), context.python_find().arg("pypy"), @r###"
        success: false
        exit_code: 2
        ----- stdout -----

        ----- stderr -----
        error: No interpreter found for PyPy in virtual environments or system path
        "###);
    }

    // Swap the order of the Python versions
    context.python_versions.reverse();

    uv_snapshot!(context.filters(), context.python_find(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.12]

    ----- stderr -----
    "###);

    // Request Python 3.11
    uv_snapshot!(context.filters(), context.python_find().arg("3.11"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.11]

    ----- stderr -----
    "###);
}

#[test]
fn python_find_pin() {
    let context: TestContext = TestContext::new_with_versions(&["3.11", "3.12"]);

    // Pin to a version
    uv_snapshot!(context.filters(), context.python_pin().arg("3.12"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Pinned `.python-version` to `3.12`

    ----- stderr -----
    "###);

    // We should find the pinned version, not the first on the path
    uv_snapshot!(context.filters(), context.python_find(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.12]

    ----- stderr -----
    "###);

    // Unless explicitly requested
    uv_snapshot!(context.filters(), context.python_find().arg("3.11"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.11]

    ----- stderr -----
    "###);

    // Or `--no-config` is used
    uv_snapshot!(context.filters(), context.python_find().arg("--no-config"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.11]

    ----- stderr -----
    "###);
}

#[test]
fn python_find_project() {
    let context: TestContext = TestContext::new_with_versions(&["3.11", "3.12"]);

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0"]
    "#})
        .unwrap();

    // We should respect the project's required version, not the first on the path
    uv_snapshot!(context.filters(), context.python_find(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.12]

    ----- stderr -----
    "###);

    // Unless explicitly requested
    uv_snapshot!(context.filters(), context.python_find().arg("3.11"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.11]

    ----- stderr -----
    "###);

    // Or `--no-project` is used
    uv_snapshot!(context.filters(), context.python_find().arg("--no-project"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.11]

    ----- stderr -----
    "###);
}

#[test]
fn python_find_venv() {
    let context: TestContext = TestContext::new_with_versions(&["3.11", "3.12"])
        // Enable additional filters for Windows compatibility
        .with_filtered_exe_suffix()
        .with_filtered_python_names()
        .with_filtered_virtualenv_bin();

    // Create a virtual environment
    uv_snapshot!(context.filters(), context.venv().arg("--python").arg("3.12").arg("-q"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    "###);

    // We should find it first
    // TODO(zanieb): On Windows, this has in a different display path for virtual environments which
    // is super annoying and requires some changes to how we represent working directories in the
    // test context to resolve.
    #[cfg(not(windows))]
    uv_snapshot!(context.filters(), context.python_find(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [VENV]/[BIN]/python

    ----- stderr -----
    "###);

    // Even if the `VIRTUAL_ENV` is not set (the test context includes this by default)
    #[cfg(not(windows))]
    uv_snapshot!(context.filters(), context.python_find().env_remove("VIRTUAL_ENV"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [VENV]/[BIN]/python

    ----- stderr -----
    "###);

    let child_dir = context.temp_dir.child("child");
    child_dir.create_dir_all().unwrap();

    // Unless the system flag is passed
    uv_snapshot!(context.filters(), context.python_find().arg("--system"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.11]

    ----- stderr -----
    "###);

    // Or, `UV_SYSTEM_PYTHON` is set
    uv_snapshot!(context.filters(), context.python_find().env("UV_SYSTEM_PYTHON", "1"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.11]

    ----- stderr -----
    "###);

    // Unless, `--no-system` is included
    // TODO(zanieb): Report this as a bug upstream — this should be allowed.
    uv_snapshot!(context.filters(), context.python_find().arg("--no-system").env("UV_SYSTEM_PYTHON", "1"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: the argument '--no-system' cannot be used with '--system'

    Usage: uv python find --cache-dir [CACHE_DIR] [REQUEST]

    For more information, try '--help'.
    "###);

    // We should find virtual environments from a child directory
    #[cfg(not(windows))]
    uv_snapshot!(context.filters(), context.python_find().current_dir(&child_dir).env_remove("VIRTUAL_ENV"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [VENV]/[BIN]/python

    ----- stderr -----
    "###);

    // A virtual environment in the child directory takes precedence over the parent
    uv_snapshot!(context.filters(), context.venv().arg("--python").arg("3.11").arg("-q").current_dir(&child_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    "###);

    #[cfg(not(windows))]
    uv_snapshot!(context.filters(), context.python_find().current_dir(&child_dir).env_remove("VIRTUAL_ENV"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/child/.venv/[BIN]/python

    ----- stderr -----
    "###);

    // But if we delete the parent virtual environment
    remove_dir_all(context.temp_dir.child(".venv")).unwrap();

    // And query from there... we should not find the child virtual environment
    uv_snapshot!(context.filters(), context.python_find(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.11]

    ----- stderr -----
    "###);

    // Unless, it is requested by path
    #[cfg(not(windows))]
    uv_snapshot!(context.filters(), context.python_find().arg("child/.venv"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/child/.venv/[BIN]/python

    ----- stderr -----
    "###);

    // Or activated via `VIRTUAL_ENV`
    #[cfg(not(windows))]
    uv_snapshot!(context.filters(), context.python_find().env("VIRTUAL_ENV", child_dir.join(".venv").as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/child/.venv/[BIN]/python

    ----- stderr -----
    "###);
}

#[cfg(unix)]
#[test]
fn python_find_unsupported_version() {
    let context: TestContext = TestContext::new_with_versions(&["3.12"]);

    // Request a low version
    uv_snapshot!(context.filters(), context.python_find().arg("3.6"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Invalid version request: Python <3.7 is not supported but 3.6 was requested.
    "###);

    // Request a low version with a patch
    uv_snapshot!(context.filters(), context.python_find().arg("3.6.9"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Invalid version request: Python <3.7 is not supported but 3.6.9 was requested.
    "###);

    // Request a really low version
    uv_snapshot!(context.filters(), context.python_find().arg("2.6"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Invalid version request: Python <3.7 is not supported but 2.6 was requested.
    "###);

    // Request a really low version with a patch
    uv_snapshot!(context.filters(), context.python_find().arg("2.6.8"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Invalid version request: Python <3.7 is not supported but 2.6.8 was requested.
    "###);

    // Request a future version
    uv_snapshot!(context.filters(), context.python_find().arg("4.2"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No interpreter found for Python 4.2 in virtual environments or system path
    "###);

    // Request a low version with a range
    uv_snapshot!(context.filters(), context.python_find().arg("<3.0"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No interpreter found for Python <3.0 in virtual environments or system path
    "###);
}
