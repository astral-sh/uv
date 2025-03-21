use uv_static::EnvVars;

use crate::common::{uv_snapshot, TestContext};

#[test]
fn python_list() {
    let mut context: TestContext = TestContext::new_with_versions(&["3.11", "3.12"])
        .with_filtered_python_symlinks()
        .with_filtered_python_keys();

    uv_snapshot!(context.filters(), context.python_list().env(EnvVars::UV_TEST_PYTHON_PATH, ""), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    ");

    // We show all interpreters
    uv_snapshot!(context.filters(), context.python_list(), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.12.[X]-[PLATFORM]     [PYTHON-3.12]
    cpython-3.11.[X]-[PLATFORM]    [PYTHON-3.11]

    ----- stderr -----
    ");

    // Swap the order of the Python versions
    context.python_versions.reverse();

    uv_snapshot!(context.filters(), context.python_list(), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.12.[X]-[PLATFORM]     [PYTHON-3.12]
    cpython-3.11.[X]-[PLATFORM]    [PYTHON-3.11]

    ----- stderr -----
    ");

    // Request Python 3.11
    uv_snapshot!(context.filters(), context.python_list().arg("3.11"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: unexpected argument '3.11' found

    Usage: uv python list [OPTIONS]

    For more information, try '--help'.
    ");
}

#[test]
fn python_list_pin() {
    let context: TestContext = TestContext::new_with_versions(&["3.11", "3.12"])
        .with_filtered_python_symlinks()
        .with_filtered_python_keys();

    // Pin to a version
    uv_snapshot!(context.filters(), context.python_pin().arg("3.12"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Pinned `.python-version` to `3.12`

    ----- stderr -----
    "###);

    // The pin should not affect the listing
    uv_snapshot!(context.filters(), context.python_list(), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.12.[X]-[PLATFORM]     [PYTHON-3.12]
    cpython-3.11.[X]-[PLATFORM]    [PYTHON-3.11]

    ----- stderr -----
    ");

    // So `--no-config` has no effect
    uv_snapshot!(context.filters(), context.python_list().arg("--no-config"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.12.[X]-[PLATFORM]     [PYTHON-3.12]
    cpython-3.11.[X]-[PLATFORM]    [PYTHON-3.11]

    ----- stderr -----
    ");
}

#[test]
fn python_list_venv() {
    let context: TestContext = TestContext::new_with_versions(&["3.11", "3.12"])
        .with_filtered_python_symlinks()
        .with_filtered_python_keys()
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

    // We should not display the virtual environment
    uv_snapshot!(context.filters(), context.python_list(), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.12.[X]-[PLATFORM]     [PYTHON-3.12]
    cpython-3.11.[X]-[PLATFORM]    [PYTHON-3.11]

    ----- stderr -----
    ");

    // Same if the `VIRTUAL_ENV` is not set (the test context includes it by default)
    uv_snapshot!(context.filters(), context.python_list().env_remove(EnvVars::VIRTUAL_ENV), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.12.[X]-[PLATFORM]     [PYTHON-3.12]
    cpython-3.11.[X]-[PLATFORM]    [PYTHON-3.11]

    ----- stderr -----
    ");
}
