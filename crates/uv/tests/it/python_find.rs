use assert_fs::prelude::PathChild;
use assert_fs::{fixture::FileWriteStr, prelude::PathCreateDir};
use indoc::indoc;

use uv_python::platform::{Arch, Os};
use uv_static::EnvVars;

use crate::common::{uv_snapshot, venv_bin_path, TestContext};

#[test]
fn python_find() {
    let mut context: TestContext = TestContext::new_with_versions(&["3.11", "3.12"]);

    // No interpreters on the path
    if cfg!(windows) {
        uv_snapshot!(context.filters(), context.python_find().env(EnvVars::UV_TEST_PYTHON_PATH, ""), @r###"
        success: false
        exit_code: 2
        ----- stdout -----

        ----- stderr -----
        error: No interpreter found in virtual environments, managed installations, search path, or registry
        "###);
    } else {
        uv_snapshot!(context.filters(), context.python_find().env(EnvVars::UV_TEST_PYTHON_PATH, ""), @r###"
        success: false
        exit_code: 2
        ----- stdout -----

        ----- stderr -----
        error: No interpreter found in virtual environments, managed installations, or search path
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
        error: No interpreter found for PyPy in virtual environments, managed installations, search path, or registry
        "###);
    } else {
        uv_snapshot!(context.filters(), context.python_find().arg("pypy"), @r###"
        success: false
        exit_code: 2
        ----- stdout -----

        ----- stderr -----
        error: No interpreter found for PyPy in virtual environments, managed installations, or search path
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

    let child_dir = context.temp_dir.child("child");
    child_dir.create_dir_all().unwrap();

    // We should also find pinned versions in the parent directory
    uv_snapshot!(context.filters(), context.python_find().current_dir(&child_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.12]

    ----- stderr -----
    "###);

    uv_snapshot!(context.filters(), context.python_pin().arg("3.11").current_dir(&child_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Pinned `.python-version` to `3.11`

    ----- stderr -----
    "###);

    // Unless the child directory also has a pin
    uv_snapshot!(context.filters(), context.python_find().current_dir(&child_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.11]

    ----- stderr -----
    "###);
}

#[test]
fn python_find_project() {
    let context: TestContext = TestContext::new_with_versions(&["3.10", "3.11", "3.12"]);

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.11"
        dependencies = ["anyio==3.7.0"]
    "#})
        .unwrap();

    // We should respect the project's required version, not the first on the path
    uv_snapshot!(context.filters(), context.python_find(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.11]

    ----- stderr -----
    "###);

    // Unless explicitly requested
    uv_snapshot!(context.filters(), context.python_find().arg("3.10"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.10]

    ----- stderr -----
    warning: The requested interpreter resolved to Python 3.10.[X], which is incompatible with the project's Python requirement: `>=3.11`
    "###);

    // Or `--no-project` is used
    uv_snapshot!(context.filters(), context.python_find().arg("--no-project"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.10]

    ----- stderr -----
    "###);

    // But a pin should take precedence
    uv_snapshot!(context.filters(), context.python_pin().arg("3.12"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Pinned `.python-version` to `3.12`

    ----- stderr -----
    "###);
    uv_snapshot!(context.filters(), context.python_find(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.12]

    ----- stderr -----
    "###);

    // Create a pin that's incompatible with the project
    uv_snapshot!(context.filters(), context.python_pin().arg("3.10").arg("--no-workspace"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Updated `.python-version` from `3.12` -> `3.10`

    ----- stderr -----
    "###);

    // We should warn on subsequent uses, but respect the pinned version?
    uv_snapshot!(context.filters(), context.python_find(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.10]

    ----- stderr -----
    warning: The Python request from `.python-version` resolved to Python 3.10.[X], which is incompatible with the project's Python requirement: `>=3.11`. Use `uv python pin` to update the `.python-version` file to a compatible version.
    "###);

    // Unless the pin file is outside the project, in which case we should just ignore it
    let child_dir = context.temp_dir.child("child");
    child_dir.create_dir_all().unwrap();

    let pyproject_toml = child_dir.child("pyproject.toml");
    pyproject_toml
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.11"
        dependencies = ["anyio==3.7.0"]
    "#})
        .unwrap();

    uv_snapshot!(context.filters(), context.python_find().current_dir(&child_dir), @r###"
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
    uv_snapshot!(context.filters(), context.python_find().env_remove(EnvVars::VIRTUAL_ENV), @r###"
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
    uv_snapshot!(context.filters(), context.python_find().env(EnvVars::UV_SYSTEM_PYTHON, "1"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.11]

    ----- stderr -----
    "###);

    // Unless, `--no-system` is included
    // TODO(zanieb): Report this as a bug upstream — this should be allowed.
    uv_snapshot!(context.filters(), context.python_find().arg("--no-system").env(EnvVars::UV_SYSTEM_PYTHON, "1"), @r###"
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
    uv_snapshot!(context.filters(), context.python_find().current_dir(&child_dir).env_remove(EnvVars::VIRTUAL_ENV), @r###"
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
    uv_snapshot!(context.filters(), context.python_find().current_dir(&child_dir).env_remove(EnvVars::VIRTUAL_ENV), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/child/.venv/[BIN]/python

    ----- stderr -----
    "###);

    // But if we delete the parent virtual environment
    fs_err::remove_dir_all(context.temp_dir.child(".venv")).unwrap();

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
    uv_snapshot!(context.filters(), context.python_find().env(EnvVars::VIRTUAL_ENV, child_dir.join(".venv").as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/child/.venv/[BIN]/python

    ----- stderr -----
    "###);

    // Or at the front of the PATH
    #[cfg(not(windows))]
    uv_snapshot!(context.filters(), context.python_find().env(EnvVars::UV_TEST_PYTHON_PATH, child_dir.join(".venv").join("bin").as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/child/.venv/[BIN]/python

    ----- stderr -----
    "###);

    // This holds even if there are other directories before it in the path, as long as they do
    // not contain a Python executable
    #[cfg(not(windows))]
    {
        let path = std::env::join_paths(&[
            context.temp_dir.to_path_buf(),
            child_dir.join(".venv").join("bin"),
        ])
        .unwrap();

        uv_snapshot!(context.filters(), context.python_find().env(EnvVars::UV_TEST_PYTHON_PATH, path.as_os_str()), @r###"
        success: true
        exit_code: 0
        ----- stdout -----
        [TEMP_DIR]/child/.venv/[BIN]/python

        ----- stderr -----
        "###);
    }

    // But, if there's an executable _before_ the virtual environment — we prefer that
    #[cfg(not(windows))]
    {
        let path = std::env::join_paths(
            std::env::split_paths(&context.python_path())
                .chain(std::iter::once(child_dir.join(".venv").join("bin"))),
        )
        .unwrap();

        uv_snapshot!(context.filters(), context.python_find().env(EnvVars::UV_TEST_PYTHON_PATH, path.as_os_str()), @r###"
        success: true
        exit_code: 0
        ----- stdout -----
        [PYTHON-3.11]

        ----- stderr -----
        "###);
    }
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
    error: No interpreter found for Python 4.2 in virtual environments, managed installations, or search path
    "###);

    // Request a low version with a range
    uv_snapshot!(context.filters(), context.python_find().arg("<3.0"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No interpreter found for Python <3.0 in virtual environments, managed installations, or search path
    "###);

    // Request free-threaded Python on unsupported version
    uv_snapshot!(context.filters(), context.python_find().arg("3.12t"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Invalid version request: Python <3.13 does not support free-threading but 3.12t was requested.
    "###);
}

#[test]
fn python_find_venv_invalid() {
    let context: TestContext = TestContext::new("3.12")
        // Enable additional filters for Windows compatibility
        .with_filtered_exe_suffix()
        .with_filtered_python_names()
        .with_filtered_virtualenv_bin();

    // We find the virtual environment
    uv_snapshot!(context.filters(), context.python_find().env(EnvVars::VIRTUAL_ENV, context.venv.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [VENV]/[BIN]/python

    ----- stderr -----
    "###);

    // If the binaries are missing from a virtual environment, we fail
    fs_err::remove_dir_all(venv_bin_path(&context.venv)).unwrap();

    uv_snapshot!(context.filters(), context.python_find().env(EnvVars::VIRTUAL_ENV, context.venv.as_os_str()), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to inspect Python interpreter from active virtual environment at `.venv/[BIN]/python`
      Caused by: Python interpreter not found at `[VENV]/[BIN]/python`
    "###);

    // Unless the virtual environment is not active
    uv_snapshot!(context.filters(), context.python_find(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.12]

    ----- stderr -----
    "###);

    // If there's not a `pyvenv.cfg` file, it's also non-fatal, we ignore the environment
    fs_err::remove_file(context.venv.join("pyvenv.cfg")).unwrap();

    uv_snapshot!(context.filters(), context.python_find().env(EnvVars::VIRTUAL_ENV, context.venv.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.12]

    ----- stderr -----
    "###);
}

/// See: <https://github.com/astral-sh/uv/issues/11825>
#[test]
#[cfg(unix)]
fn python_required_python_major_minor() {
    let context: TestContext = TestContext::new_with_versions(&["3.11", "3.12"]);

    // Find the Python 3.11 executable.
    let path = &context.python_versions.first().unwrap().1;

    // Symlink it to `python3.11`.
    fs_err::create_dir_all(context.temp_dir.child("child")).unwrap();
    std::os::unix::fs::symlink(path, context.temp_dir.child("child").join("python3.11")).unwrap();

    // Find `python3.11`, which is `>=3.11.4`.
    uv_snapshot!(context.filters(), context.python_find().arg(">=3.11.4, <3.12").env(EnvVars::UV_TEST_PYTHON_PATH, context.temp_dir.child("child").path()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/child/python3.11

    ----- stderr -----
    "###);

    // Find `python3.11`, which is `>3.11.4`.
    uv_snapshot!(context.filters(), context.python_find().arg(">3.11.4, <3.12").env(EnvVars::UV_TEST_PYTHON_PATH, context.temp_dir.child("child").path()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/child/python3.11

    ----- stderr -----
    "###);

    // Fail to find any matching Python interpreter.
    uv_snapshot!(context.filters(), context.python_find().arg(">3.11.255, <3.12").env(EnvVars::UV_TEST_PYTHON_PATH, context.temp_dir.child("child").path()), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No interpreter found for Python >3.11.[X], <3.12 in virtual environments, managed installations, or search path
    "###);
}
