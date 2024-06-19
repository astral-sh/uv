#![cfg(all(feature = "python", feature = "pypi"))]

use common::{python_path_with_versions, uv_snapshot, TestContext};
use uv_toolchain::platform::{Arch, Os};

mod common;

#[test]
fn toolchain_find() {
    let context: TestContext = TestContext::new_with_versions(&["3.11", "3.12"]);

    // No interpreters on the path
    uv_snapshot!(context.filters(), context.toolchain_find().env("UV_TEST_PYTHON_PATH", ""), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No Python interpreters found in provided path, active virtual environment, or search path
    "###);

    // We find the first interpreter on the path
    uv_snapshot!(context.filters(), context.toolchain_find(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.11]

    ----- stderr -----
    "###);

    // Request Python 3.12
    uv_snapshot!(context.filters(), context.toolchain_find().arg("3.12"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.12]

    ----- stderr -----
    "###);

    // Request Python 3.11
    uv_snapshot!(context.filters(), context.toolchain_find().arg("3.11"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.11]

    ----- stderr -----
    "###);

    // Request CPython
    uv_snapshot!(context.filters(), context.toolchain_find().arg("cpython"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.11]

    ----- stderr -----
    "###);

    // Request CPython 3.12
    uv_snapshot!(context.filters(), context.toolchain_find().arg("cpython@3.12"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.12]

    ----- stderr -----
    "###);

    // Request CPython 3.12 via partial key syntax
    uv_snapshot!(context.filters(), context.toolchain_find().arg("cpython-3.12"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.12]

    ----- stderr -----
    "###);

    // Request CPython 3.12 for the current platform
    let os = Os::from_env();
    let arch = Arch::from_env();

    uv_snapshot!(context.filters(), context.toolchain_find()
    .arg(format!("cpython-3.12-{os}-{arch}"))
    , @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.12]

    ----- stderr -----
    "###);

    // Request PyPy
    uv_snapshot!(context.filters(), context.toolchain_find().arg("pypy"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No interpreter found for PyPy in provided path, active virtual environment, or search path
    "###);

    // Swap the order of the Python versions
    let python_path = python_path_with_versions(&context.temp_dir, &["3.12", "3.11"])
        .expect("Failed to create Python test path");

    uv_snapshot!(context.filters(), context.toolchain_find().env("UV_TEST_PYTHON_PATH", python_path), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.12]

    ----- stderr -----
    "###);

    // Request Python 3.11
    uv_snapshot!(context.filters(), context.toolchain_find().arg("3.11"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-3.11]

    ----- stderr -----
    "###);
}
