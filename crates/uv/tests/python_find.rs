#![cfg(all(feature = "python", feature = "pypi"))]

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
        error: No interpreter found in system path or `py` launcher
        "###);
    } else {
        uv_snapshot!(context.filters(), context.python_find().env("UV_TEST_PYTHON_PATH", ""), @r###"
        success: false
        exit_code: 2
        ----- stdout -----

        ----- stderr -----
        error: No interpreter found in system path
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
        error: No interpreter found for PyPy in system path or `py` launcher
        "###);
    } else {
        uv_snapshot!(context.filters(), context.python_find().arg("pypy"), @r###"
        success: false
        exit_code: 2
        ----- stdout -----

        ----- stderr -----
        error: No interpreter found for PyPy in system path
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
