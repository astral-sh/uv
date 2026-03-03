use assert_cmd::assert::OutputAssertExt;
use assert_fs::prelude::{FileTouch, PathChild};
use assert_fs::{fixture::FileWriteStr, prelude::PathCreateDir};
use indoc::indoc;

use uv_platform::{Arch, Os};
use uv_static::EnvVars;

use uv_test::{uv_snapshot, venv_bin_path};

#[test]
fn python_find() {
    let mut context =
        uv_test::test_context_with_versions!(&["3.11", "3.12"]).with_filtered_python_sources();

    // No interpreters on the path
    uv_snapshot!(context.filters(), context.python_find().env(EnvVars::UV_TEST_PYTHON_PATH, ""), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No interpreter found in [PYTHON SOURCES]
    ");

    // We find the first interpreter on the path
    uv_snapshot!(context.filters(), context.python_find(), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.11]

    ----- stderr -----
    ");

    // Request Python 3.12
    uv_snapshot!(context.filters(), context.python_find().arg("3.12"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.12]

    ----- stderr -----
    ");

    // Request Python 3.12
    uv_snapshot!(context.filters(), context.python_find().arg("==3.12.*"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.12]

    ----- stderr -----
    ");

    // Request Python 3.11
    uv_snapshot!(context.filters(), context.python_find().arg("3.11"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.11]

    ----- stderr -----
    ");

    // Request CPython
    uv_snapshot!(context.filters(), context.python_find().arg("cpython"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.11]

    ----- stderr -----
    ");

    // Request CPython 3.12
    uv_snapshot!(context.filters(), context.python_find().arg("cpython@3.12"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.12]

    ----- stderr -----
    ");

    // Request CPython 3.12 via partial key syntax
    uv_snapshot!(context.filters(), context.python_find().arg("cpython-3.12"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.12]

    ----- stderr -----
    ");

    // Request Python 3.12 via partial key syntax with placeholders
    uv_snapshot!(context.filters(), context.python_find().arg("any-3.12-any"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.12]

    ----- stderr -----
    ");

    // Request CPython 3.12 for the current platform
    let os = Os::from_env();
    let arch = Arch::from_env();

    uv_snapshot!(context.filters(), context.python_find()
        .arg(format!("cpython-3.12-{os}-{arch}")), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.12]

    ----- stderr -----
    ");

    // Request PyPy (which should be missing)
    uv_snapshot!(context.filters(), context.python_find().arg("pypy"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No interpreter found for PyPy in [PYTHON SOURCES]
    ");

    // Swap the order of the Python versions
    context.python_versions.reverse();

    uv_snapshot!(context.filters(), context.python_find(), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.12]

    ----- stderr -----
    ");

    // Request Python 3.11
    uv_snapshot!(context.filters(), context.python_find().arg("3.11"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.11]

    ----- stderr -----
    ");
}

#[test]
fn python_find_pin() {
    let context = uv_test::test_context_with_versions!(&["3.11", "3.12"]);

    // Pin to a version
    uv_snapshot!(context.filters(), context.python_pin().arg("3.12"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Pinned `.python-version` to `3.12`

    ----- stderr -----
    ");

    // We should find the pinned version, not the first on the path
    uv_snapshot!(context.filters(), context.python_find(), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.12]

    ----- stderr -----
    ");

    // Unless explicitly requested
    uv_snapshot!(context.filters(), context.python_find().arg("3.11"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.11]

    ----- stderr -----
    ");

    // Or `--no-config` is used
    uv_snapshot!(context.filters(), context.python_find().arg("--no-config"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.11]

    ----- stderr -----
    ");

    let child_dir = context.temp_dir.child("child");
    child_dir.create_dir_all().unwrap();

    // We should also find pinned versions in the parent directory
    uv_snapshot!(context.filters(), context.python_find().current_dir(&child_dir), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.12]

    ----- stderr -----
    ");

    uv_snapshot!(context.filters(), context.python_pin().arg("3.11").current_dir(&child_dir), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Pinned `.python-version` to `3.11`

    ----- stderr -----
    ");

    // Unless the child directory also has a pin
    uv_snapshot!(context.filters(), context.python_find().current_dir(&child_dir), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.11]

    ----- stderr -----
    ");
}

#[test]
fn python_find_pin_arbitrary_name() {
    let context = uv_test::test_context_with_versions!(&["3.11", "3.12"]);

    // Try to pin to an arbitrary name
    uv_snapshot!(context.filters(), context.python_pin().arg("foo"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Requests for arbitrary names (e.g., `foo`) are not supported in version files
    ");

    // Pin to an arbitrary name, bypassing uv
    context
        .temp_dir
        .child(".python-version")
        .write_str("foo")
        .unwrap();

    // The arbitrary name should be ignored
    uv_snapshot!(context.filters(), context.python_find(), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.11]

    ----- stderr -----
    warning: Ignoring unsupported Python request `foo` in version file: [TEMP_DIR]/.python-version
    ");

    // The pin should be updatable
    uv_snapshot!(context.filters(), context.python_pin().arg("3.11"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Pinned `.python-version` to `3.11`

    ----- stderr -----
    warning: Ignoring unsupported Python request `foo` in version file: [TEMP_DIR]/.python-version
    ");

    // Warnings shouldn't appear afterwards...
    uv_snapshot!(context.filters(), context.python_pin().arg("3.12"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Updated `.python-version` from `3.11` -> `3.12`

    ----- stderr -----
    ");

    // Pin in a sub-directory
    context.temp_dir.child("foo").create_dir_all().unwrap();
    context
        .temp_dir
        .child("foo")
        .child(".python-version")
        .write_str("foo")
        .unwrap();

    // The arbitrary name should be ignored, but we won't walk up to the parent `.python-version`
    // file (which contains 3.12); this behavior is a little questionable but we probably want to
    // ignore all empty version files if we want to change this?
    uv_snapshot!(context.filters(), context.python_find().current_dir(context.temp_dir.child("foo").path()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.11]

    ----- stderr -----
    warning: Ignoring unsupported Python request `foo` in version file: [TEMP_DIR]/foo/.python-version
    ");
}

#[test]
fn python_find_project() {
    let context = uv_test::test_context_with_versions!(&["3.10", "3.11", "3.12"]);

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
    uv_snapshot!(context.filters(), context.python_find(), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.11]

    ----- stderr -----
    ");

    // Unless explicitly requested
    uv_snapshot!(context.filters(), context.python_find().arg("3.10"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.10]

    ----- stderr -----
    warning: The requested interpreter resolved to Python 3.10.[X], which is incompatible with the project's Python requirement: `>=3.11` (from `project.requires-python`)
    ");

    // Or `--no-project` is used
    uv_snapshot!(context.filters(), context.python_find().arg("--no-project"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.10]

    ----- stderr -----
    ");

    // But a pin should take precedence
    uv_snapshot!(context.filters(), context.python_pin().arg("3.12"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Pinned `.python-version` to `3.12`

    ----- stderr -----
    ");
    uv_snapshot!(context.filters(), context.python_find(), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.12]

    ----- stderr -----
    ");

    // Create a pin that's incompatible with the project
    uv_snapshot!(context.filters(), context.python_pin().arg("3.10").arg("--no-workspace"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Updated `.python-version` from `3.12` -> `3.10`

    ----- stderr -----
    ");

    // We should warn on subsequent uses, but respect the pinned version?
    uv_snapshot!(context.filters(), context.python_find(), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.10]

    ----- stderr -----
    warning: The Python request from `.python-version` resolved to Python 3.10.[X], which is incompatible with the project's Python requirement: `>=3.11` (from `project.requires-python`)
    Use `uv python pin` to update the `.python-version` file to a compatible version
    ");

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

    uv_snapshot!(context.filters(), context.python_find().current_dir(&child_dir), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.11]

    ----- stderr -----
    ");
}

#[test]
fn virtual_empty() {
    // testing how `uv python find` reacts to a pyproject with no `[project]` and nothing useful to it
    let context = uv_test::test_context_with_versions!(&["3.10", "3.11", "3.12"]);

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml
        .write_str(indoc! {r#"
        [tool.mycooltool]
        wow = "someconfig"
    "#})
        .unwrap();

    // Ask for the python
    uv_snapshot!(context.filters(), context.python_find(), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.10]

    ----- stderr -----
    ");

    // Ask for the python (--no-project)
    uv_snapshot!(context.filters(), context.python_find()
        .arg("--no-project"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.10]

    ----- stderr -----
    ");

    // Ask for specific python (3.11)
    uv_snapshot!(context.filters(), context.python_find().arg("3.11"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.11]

    ----- stderr -----
    ");

    // Create a pin
    uv_snapshot!(context.filters(), context.python_pin().arg("3.12"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Pinned `.python-version` to `3.12`

    ----- stderr -----
    ");

    // Ask for the python
    uv_snapshot!(context.filters(), context.python_find(), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.12]

    ----- stderr -----
    ");

    // Ask for specific python (3.11)
    uv_snapshot!(context.filters(), context.python_find().arg("3.11"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.11]

    ----- stderr -----
    ");

    // Ask for the python (--no-project)
    uv_snapshot!(context.filters(), context.python_find(), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.12]

    ----- stderr -----
    ");
}

#[test]
fn virtual_dependency_group() {
    // testing basic `uv python find` functionality
    // when the pyproject.toml is fully virtual (no `[project]`, but `[dependency-groups]` defined,
    // which really shouldn't matter)
    let context = uv_test::test_context_with_versions!(&["3.10", "3.11", "3.12"]);

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml
        .write_str(indoc! {r#"
        [dependency-groups]
        foo = ["sortedcontainers"]
        bar = ["iniconfig"]
        dev = ["sniffio"]
    "#})
        .unwrap();

    // Ask for the python
    uv_snapshot!(context.filters(), context.python_find(), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.10]

    ----- stderr -----
    ");

    // Ask for the python (--no-project)
    uv_snapshot!(context.filters(), context.python_find()
        .arg("--no-project"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.10]

    ----- stderr -----
    ");

    // Ask for specific python (3.11)
    uv_snapshot!(context.filters(), context.python_find().arg("3.11"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.11]

    ----- stderr -----
    ");

    // Create a pin
    uv_snapshot!(context.filters(), context.python_pin().arg("3.12"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Pinned `.python-version` to `3.12`

    ----- stderr -----
    ");

    // Ask for the python
    uv_snapshot!(context.filters(), context.python_find(), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.12]

    ----- stderr -----
    ");

    // Ask for specific python (3.11)
    uv_snapshot!(context.filters(), context.python_find().arg("3.11"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.11]

    ----- stderr -----
    ");

    // Ask for the python (--no-project)
    uv_snapshot!(context.filters(), context.python_find(), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.12]

    ----- stderr -----
    ");
}

#[test]
fn python_find_venv() {
    let context = uv_test::test_context_with_versions!(&["3.11", "3.12"])
        // Enable additional filters for Windows compatibility
        .with_filtered_exe_suffix()
        .with_filtered_python_names()
        .with_filtered_virtualenv_bin();

    // Create a virtual environment
    uv_snapshot!(context.filters(), context.venv().arg("--python").arg("3.12").arg("-q"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    ");

    // We should find it first
    // TODO(zanieb): On Windows, this has in a different display path for virtual environments which
    // is super annoying and requires some changes to how we represent working directories in the
    // test context to resolve.
    #[cfg(not(windows))]
    uv_snapshot!(context.filters(), context.python_find(), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [VENV]/[BIN]/[PYTHON]

    ----- stderr -----
    ");

    let child_dir = context.temp_dir.child("child");
    child_dir.create_dir_all().unwrap();

    // Unless the system flag is passed
    uv_snapshot!(context.filters(), context.python_find().arg("--system"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.11]

    ----- stderr -----
    ");

    // Or, `UV_SYSTEM_PYTHON` is set
    uv_snapshot!(context.filters(), context.python_find().env(EnvVars::UV_SYSTEM_PYTHON, "1"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.11]

    ----- stderr -----
    ");

    // Unless, `--no-system` is included
    // TODO(zanieb): Report this as a bug upstream — this should be allowed.
    uv_snapshot!(context.filters(), context.python_find().arg("--no-system").env(EnvVars::UV_SYSTEM_PYTHON, "1"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: the argument '--no-system' cannot be used with '--system'

    Usage: uv python find --cache-dir [CACHE_DIR] [REQUEST]

    For more information, try '--help'.
    ");

    // We should find virtual environments from a child directory
    #[cfg(not(windows))]
    uv_snapshot!(context.filters(), context.python_find().current_dir(&child_dir), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [VENV]/[BIN]/[PYTHON]

    ----- stderr -----
    ");

    // A virtual environment in the child directory takes precedence over the parent
    uv_snapshot!(context.filters(), context.venv().arg("--python").arg("3.11").arg("-q").current_dir(&child_dir), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    ");

    #[cfg(not(windows))]
    uv_snapshot!(context.filters(), context.python_find().current_dir(&child_dir), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/child/.venv/[BIN]/[PYTHON]

    ----- stderr -----
    ");

    // But if we delete the parent virtual environment
    fs_err::remove_dir_all(context.temp_dir.child(".venv")).unwrap();

    // And query from there... we should not find the child virtual environment
    uv_snapshot!(context.filters(), context.python_find(), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.11]

    ----- stderr -----
    ");

    // Unless, it is requested by path
    #[cfg(not(windows))]
    uv_snapshot!(context.filters(), context.python_find().arg("child/.venv"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/child/.venv/[BIN]/[PYTHON]

    ----- stderr -----
    ");

    // Or activated via `VIRTUAL_ENV`
    #[cfg(not(windows))]
    uv_snapshot!(context.filters(), context.python_find().env(EnvVars::VIRTUAL_ENV, child_dir.join(".venv").as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/child/.venv/[BIN]/[PYTHON]

    ----- stderr -----
    ");

    // Or at the front of the PATH
    #[cfg(not(windows))]
    uv_snapshot!(context.filters(), context.python_find().env(EnvVars::UV_TEST_PYTHON_PATH, child_dir.join(".venv").join("bin").as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/child/.venv/[BIN]/[PYTHON]

    ----- stderr -----
    ");

    // This holds even if there are other directories before it in the path, as long as they do
    // not contain a Python executable
    #[cfg(not(windows))]
    {
        let path = std::env::join_paths(&[
            context.temp_dir.to_path_buf(),
            child_dir.join(".venv").join("bin"),
        ])
        .unwrap();

        uv_snapshot!(context.filters(), context.python_find().env(EnvVars::UV_TEST_PYTHON_PATH, path.as_os_str()), @"
        success: true
        exit_code: 0
        ----- stdout -----
        [TEMP_DIR]/child/.venv/[BIN]/[PYTHON]

        ----- stderr -----
        ");
    }

    // But, if there's an executable _before_ the virtual environment — we prefer that
    #[cfg(not(windows))]
    {
        let path = std::env::join_paths(
            std::env::split_paths(&context.python_path())
                .chain(std::iter::once(child_dir.join(".venv").join("bin"))),
        )
        .unwrap();

        uv_snapshot!(context.filters(), context.python_find().env(EnvVars::UV_TEST_PYTHON_PATH, path.as_os_str()), @"
        success: true
        exit_code: 0
        ----- stdout -----
        [PYTHON-3.11]

        ----- stderr -----
        ");
    }
}

#[cfg(unix)]
#[test]
fn python_find_unsupported_version() {
    let context = uv_test::test_context_with_versions!(&["3.12"]);

    // Request a low version
    uv_snapshot!(context.filters(), context.python_find().arg("3.6"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Invalid version request: Python <3.7 is not supported but 3.6 was requested.
    ");

    // Request a low version with a patch
    uv_snapshot!(context.filters(), context.python_find().arg("3.6.9"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Invalid version request: Python <3.7 is not supported but 3.6.9 was requested.
    ");

    // Request a really low version
    uv_snapshot!(context.filters(), context.python_find().arg("2.6"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Invalid version request: Python <3.7 is not supported but 2.6 was requested.
    ");

    // Request a really low version with a patch
    uv_snapshot!(context.filters(), context.python_find().arg("2.6.8"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Invalid version request: Python <3.7 is not supported but 2.6.8 was requested.
    ");

    // Request a future version
    uv_snapshot!(context.filters(), context.python_find().arg("4.2"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No interpreter found for Python 4.2 in virtual environments, managed installations, or search path
    ");

    // Request a low version with a range
    uv_snapshot!(context.filters(), context.python_find().arg("<3.0"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No interpreter found for Python <3.0 in virtual environments, managed installations, or search path
    ");

    // Request free-threaded Python on unsupported version
    uv_snapshot!(context.filters(), context.python_find().arg("3.12t"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Invalid version request: Python <3.13 does not support free-threading but 3.12+freethreaded was requested.
    ");
}

#[test]
fn python_find_venv_invalid() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_python_names()
        .with_filtered_virtualenv_bin()
        .with_filtered_exe_suffix();

    // We find the virtual environment
    uv_snapshot!(context.filters(), context.python_find().env(EnvVars::VIRTUAL_ENV, context.venv.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [VENV]/[BIN]/[PYTHON]

    ----- stderr -----
    ");

    // If the binaries are missing from a virtual environment, we fail
    fs_err::remove_dir_all(venv_bin_path(&context.venv)).unwrap();

    uv_snapshot!(context.filters(), context.python_find().env(EnvVars::VIRTUAL_ENV, context.venv.as_os_str()), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to inspect Python interpreter from active virtual environment at `.venv/[BIN]/[PYTHON]`
      Caused by: Python interpreter not found at `[VENV]/[BIN]/[PYTHON]`
    ");

    // Unless the virtual environment is not active
    uv_snapshot!(context.filters(), context.python_find(), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.12]

    ----- stderr -----
    ");

    // If there's not a `pyvenv.cfg` file, it's also non-fatal, we ignore the environment
    fs_err::remove_file(context.venv.join("pyvenv.cfg")).unwrap();

    uv_snapshot!(context.filters(), context.python_find().env(EnvVars::VIRTUAL_ENV, context.venv.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.12]

    ----- stderr -----
    ");
}

#[test]
fn python_find_managed() {
    let context = uv_test::test_context_with_versions!(&["3.11", "3.12"])
        .with_filtered_python_sources()
        .with_versions_as_managed(&["3.12"]);

    // We find the managed interpreter
    uv_snapshot!(context.filters(), context.python_find().arg("--managed-python"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.12]

    ----- stderr -----
    ");

    // Request an interpreter that cannot be satisfied
    uv_snapshot!(context.filters(), context.python_find().arg("--managed-python").arg("3.11"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No interpreter found for Python 3.11 in virtual environments or managed installations
    ");

    let context = uv_test::test_context_with_versions!(&["3.11", "3.12"])
        .with_filtered_python_sources()
        .with_versions_as_managed(&["3.11"]);

    // We find the unmanaged interpreter with managed Python disabled
    uv_snapshot!(context.filters(), context.python_find().arg("--no-managed-python"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.12]

    ----- stderr -----
    ");

    // Request an interpreter that cannot be satisfied
    uv_snapshot!(context.filters(), context.python_find().arg("--no-managed-python").arg("3.11"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No interpreter found for Python 3.11 in [PYTHON SOURCES]
    ");

    // We find the unmanaged interpreter with system Python preferred
    uv_snapshot!(context.filters(), context.python_find().arg("--python-preference").arg("system"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.12]

    ----- stderr -----
    ");

    // But, if no system Python meets the request, we'll use the managed interpreter
    uv_snapshot!(context.filters(), context.python_find().arg("--python-preference").arg("system").arg("3.11"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.11]

    ----- stderr -----
    ");
}

/// See: <https://github.com/astral-sh/uv/issues/11825>
///
/// This test will not succeed on macOS if using a Homebrew provided interpreter. The interpreter
/// reports `sys.executable` as the canonicalized path instead of `[TEMP_DIR]/...`. For this reason,
/// it's marked as requiring our `python-managed` feature — but it does not enforce that these are
/// used in the test context.
#[test]
#[cfg(unix)]
#[cfg(feature = "test-python-managed")]
fn python_required_python_major_minor() {
    let context = uv_test::test_context_with_versions!(&["3.11", "3.12"]);

    // Find the Python 3.11 executable.
    let path = &context.python_versions.first().unwrap().1;

    // Symlink it to `python3.11`.
    fs_err::create_dir_all(context.temp_dir.child("child")).unwrap();
    fs_err::os::unix::fs::symlink(path, context.temp_dir.child("child").join("python3.11"))
        .unwrap();

    // Find `python3.11`, which is `>=3.11.4`.
    uv_snapshot!(context.filters(), context.python_find().arg(">=3.11.4, <3.12").env(EnvVars::UV_TEST_PYTHON_PATH, context.temp_dir.child("child").path()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/child/python3.11

    ----- stderr -----
    ");

    // Find `python3.11`, which is `>3.11.4`.
    uv_snapshot!(context.filters(), context.python_find().arg(">3.11.4, <3.12").env(EnvVars::UV_TEST_PYTHON_PATH, context.temp_dir.child("child").path()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/child/python3.11

    ----- stderr -----
    ");

    // Fail to find any matching Python interpreter.
    uv_snapshot!(context.filters(), context.python_find().arg(">3.11.255, <3.12").env(EnvVars::UV_TEST_PYTHON_PATH, context.temp_dir.child("child").path()), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No interpreter found for Python >3.11.[X], <3.12 in virtual environments, managed installations, or search path
    ");
}

#[test]
fn python_find_script() {
    let context = uv_test::test_context!("3.13")
        .with_filtered_virtualenv_bin()
        .with_filtered_python_names()
        .with_filtered_exe_suffix();

    uv_snapshot!(context.filters(), context.init().arg("--script").arg("foo.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Initialized script at `foo.py`
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--script").arg("foo.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Creating script environment at: [CACHE_DIR]/environments-v2/foo-[HASH]
    Resolved in [TIME]
    Audited in [TIME]
    ");

    uv_snapshot!(context.filters(), context.python_find().arg("--script").arg("foo.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [CACHE_DIR]/environments-v2/foo-[HASH]/[BIN]/[PYTHON]

    ----- stderr -----
    ");
}

#[test]
fn python_find_script_no_environment() {
    let context = uv_test::test_context!("3.13")
        .with_filtered_virtualenv_bin()
        .with_filtered_python_names()
        .with_filtered_exe_suffix();

    let script = context.temp_dir.child("foo.py");

    script
        .write_str(indoc! {r"
            # /// script
            # dependencies = []
            # ///
        "})
        .unwrap();

    uv_snapshot!(context.filters(), context.python_find().arg("--script").arg("foo.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [VENV]/[BIN]/[PYTHON]

    ----- stderr -----
    ");
}

#[test]
fn python_find_script_python_not_found() {
    let context = uv_test::test_context_with_versions!(&[]).with_filtered_python_sources();

    let script = context.temp_dir.child("foo.py");

    script
        .write_str(indoc! {r"
            # /// script
            # dependencies = []
            # ///
        "})
        .unwrap();

    uv_snapshot!(context.filters(), context.python_find().arg("--script").arg("foo.py"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    No interpreter found in [PYTHON SOURCES]

    hint: A managed Python download is available, but Python downloads are set to 'never'
    ");
}

#[test]
fn python_find_script_no_such_version() {
    let context = uv_test::test_context!("3.13")
        .with_filtered_virtualenv_bin()
        .with_filtered_python_names()
        .with_filtered_exe_suffix()
        .with_filtered_python_sources();
    let script = context.temp_dir.child("foo.py");
    script
        .write_str(indoc! {r#"
            # /// script
            # requires-python = ">=3.13"
            # dependencies = []
            # ///
        "#})
        .unwrap();

    uv_snapshot!(context.filters(), context.sync().arg("--script").arg("foo.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Creating script environment at: [CACHE_DIR]/environments-v2/foo-[HASH]
    Resolved in [TIME]
    Audited in [TIME]
    ");

    script
        .write_str(indoc! {r#"
            # /// script
            # requires-python = ">=3.15"
            # dependencies = []
            # ///
        "#})
        .unwrap();

    uv_snapshot!(context.filters(), context.python_find().arg("--script").arg("foo.py"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    No interpreter found for Python >=3.15 in [PYTHON SOURCES]
    ");
}

#[test]
fn python_find_show_version() {
    let context =
        uv_test::test_context_with_versions!(&["3.11", "3.12"]).with_filtered_python_sources();

    // No interpreters found
    uv_snapshot!(context.filters(), context.python_find().env(EnvVars::UV_TEST_PYTHON_PATH, "").arg("--show-version"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No interpreter found in [PYTHON SOURCES]
    ");

    // Show the first version found
    uv_snapshot!(context.filters(), context.python_find().arg("--show-version"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    3.11.[X]

    ----- stderr -----
    ");

    // Request Python 3.12
    uv_snapshot!(context.filters(), context.python_find().arg("--show-version").arg("3.12"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    3.12.[X]

    ----- stderr -----
    ");

    // Request Python 3.11
    uv_snapshot!(context.filters(), context.python_find().arg("--show-version").arg("3.11"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    3.11.[X]

    ----- stderr -----
    ");
}

#[test]
fn python_find_path() {
    let context = uv_test::test_context_with_versions!(&[]).with_filtered_not_executable();

    context.temp_dir.child("foo").create_dir_all().unwrap();
    context.temp_dir.child("bar").touch().unwrap();

    // No interpreter in a directory
    uv_snapshot!(context.filters(), context.python_find().arg(context.temp_dir.child("foo").as_os_str()), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No interpreter found in directory `foo`
    ");

    // No interpreter at a file
    uv_snapshot!(context.filters(), context.python_find().arg(context.temp_dir.child("bar").as_os_str()), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to inspect Python interpreter from provided path at `bar`
      Caused by: Failed to query Python interpreter at `[TEMP_DIR]/bar`
      Caused by: [PERMISSION DENIED]
    ");

    // No interpreter at a file that does not exist
    uv_snapshot!(context.filters(), context.python_find().arg(context.temp_dir.child("foobar").as_os_str()), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No interpreter found at path `foobar`
    ");
}

#[test]
#[cfg(feature = "test-python-managed")]
fn python_find_freethreaded_313() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_python_sources()
        .with_managed_python_dirs()
        .with_python_download_cache()
        .with_filtered_python_install_bin()
        .with_filtered_python_names()
        .with_filtered_exe_suffix();

    context
        .python_install()
        .arg("--preview")
        .arg("3.13t")
        .assert()
        .success();

    // Request Python 3.13 (without opt-in)
    uv_snapshot!(context.filters(), context.python_find().arg("3.13"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No interpreter found for Python 3.13 in [PYTHON SOURCES]
    ");

    // Request Python 3.13t (with explicit opt-in)
    uv_snapshot!(context.filters(), context.python_find().arg("3.13t"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13+freethreaded-[PLATFORM]/[INSTALL-BIN]/[PYTHON]

    ----- stderr -----
    ");
}

#[test]
#[cfg(feature = "test-python-managed")]
fn python_find_freethreaded_314() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_python_sources()
        .with_managed_python_dirs()
        .with_python_download_cache()
        .with_filtered_python_install_bin()
        .with_filtered_python_names()
        .with_filtered_exe_suffix();

    context
        .python_install()
        .arg("--preview")
        .arg("3.14t")
        .assert()
        .success();

    // Request Python 3.14 (without opt-in)
    uv_snapshot!(context.filters(), context.python_find().arg("3.14"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.14+freethreaded-[PLATFORM]/[INSTALL-BIN]/[PYTHON]

    ----- stderr -----
    ");

    // Request Python 3.14t (with explicit opt-in)
    uv_snapshot!(context.filters(), context.python_find().arg("3.14t"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.14+freethreaded-[PLATFORM]/[INSTALL-BIN]/[PYTHON]

    ----- stderr -----
    ");

    // Request Python 3.14+gil
    uv_snapshot!(context.filters(), context.python_find().arg("3.14+gil"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No interpreter found for Python 3.14+gil in [PYTHON SOURCES]
    ");

    // Install the non-freethreaded version
    context
        .python_install()
        .arg("--preview")
        .arg("3.14")
        .assert()
        .success();

    // Request Python 3.14
    uv_snapshot!(context.filters(), context.python_find().arg("3.14"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.14-[PLATFORM]/[INSTALL-BIN]/[PYTHON]

    ----- stderr -----
    ");

    // Request Python 3.14+gil
    uv_snapshot!(context.filters(), context.python_find().arg("3.14+gil"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.14-[PLATFORM]/[INSTALL-BIN]/[PYTHON]

    ----- stderr -----
    ");
}

#[test]
#[cfg(feature = "test-python-managed")]
fn python_find_prerelease_version_specifiers() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_python_sources()
        .with_managed_python_dirs()
        .with_python_download_cache()
        .with_filtered_python_install_bin()
        .with_filtered_python_names()
        .with_filtered_exe_suffix();

    context.python_install().arg("3.14.0rc2").assert().success();
    context.python_install().arg("3.14.0rc3").assert().success();

    // `>=3.14` should allow pre-release versions
    uv_snapshot!(context.filters(), context.python_find().arg(">=3.14").arg("--resolve-links"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.14.0rc3-[PLATFORM]/[INSTALL-BIN]/[PYTHON]

    ----- stderr -----
    warning: You're using a pre-release version of Python (3.14.0rc3) but a stable version is available. Use `uv python upgrade 3.14` to upgrade.
    ");

    // `>3.14rc2` should not match rc2
    uv_snapshot!(context.filters(), context.python_find().arg(">3.14.0rc2").arg("--resolve-links"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.14.0rc3-[PLATFORM]/[INSTALL-BIN]/[PYTHON]

    ----- stderr -----
    ");

    // `>3.14rc3` should not match rc3
    uv_snapshot!(context.filters(), context.python_find().arg(">3.14.0rc3"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No interpreter found for Python >3.14.0rc3 in [PYTHON SOURCES]
    ");

    // `>=3.14.0rc3` should match rc3
    uv_snapshot!(context.filters(), context.python_find().arg(">=3.14.0rc3").arg("--resolve-links"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.14.0rc3-[PLATFORM]/[INSTALL-BIN]/[PYTHON]

    ----- stderr -----
    ");

    // `<3.14.0rc3` should match rc2
    uv_snapshot!(context.filters(), context.python_find().arg("<3.14.0rc3").arg("--resolve-links"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.14.0rc2-[PLATFORM]/[INSTALL-BIN]/[PYTHON]

    ----- stderr -----
    ");

    // `<=3.14.0rc3` should match rc3
    uv_snapshot!(context.filters(), context.python_find().arg("<=3.14.0rc3").arg("--resolve-links"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.14.0rc3-[PLATFORM]/[INSTALL-BIN]/[PYTHON]

    ----- stderr -----
    ");

    // Install the stable version
    context.python_install().arg("3.14.0").assert().success();

    // `>=3.14` should prefer stable
    uv_snapshot!(context.filters(), context.python_find().arg(">=3.14").arg("--resolve-links"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.14.0-[PLATFORM]/[INSTALL-BIN]/[PYTHON]

    ----- stderr -----
    ");

    // `>3.14rc2` should prefer stable
    uv_snapshot!(context.filters(), context.python_find().arg(">3.14.0rc2").arg("--resolve-links"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.14.0-[PLATFORM]/[INSTALL-BIN]/[PYTHON]

    ----- stderr -----
    ");
}

#[test]
#[cfg(feature = "test-python-managed")]
fn python_find_prerelease_with_patch_request() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_python_sources()
        .with_managed_python_dirs()
        .with_python_download_cache()
        .with_filtered_python_install_bin()
        .with_filtered_python_names()
        .with_filtered_exe_suffix();

    // Install 3.14.0rc3
    context.python_install().arg("3.14.0rc3").assert().success();

    // When no `.0` patch version is included, we'll allow selection of a pre-release
    uv_snapshot!(context.filters(), context.python_find().arg("3.14"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.14-[PLATFORM]/[INSTALL-BIN]/[PYTHON]

    ----- stderr -----
    warning: You're using a pre-release version of Python (3.14.0rc3) but a stable version is available. Use `uv python upgrade 3.14` to upgrade.
    ");

    // When `.0` is explicitly included, we will require a stable release
    uv_snapshot!(context.filters(), context.python_find().arg("3.14.0"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No interpreter found for Python 3.14.0 in [PYTHON SOURCES]
    ");

    // Install 3.14.0 stable
    context.python_install().arg("3.14.0").assert().success();

    uv_snapshot!(context.filters(), context.python_find().arg("3.14.0"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.14.0-[PLATFORM]/[INSTALL-BIN]/[PYTHON]

    ----- stderr -----
    ");
}
