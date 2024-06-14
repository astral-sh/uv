#![cfg(all(feature = "python", feature = "pypi"))]

use common::{python_path_with_versions, uv_snapshot, TestContext};
use uv_toolchain::platform::{Arch, Os};

mod common;

#[test]
fn toolchain_find() {
    let context: TestContext = TestContext::new("3.12");

    // No interpreters on the path
    uv_snapshot!(context.filters(), context.toolchain_find(), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No Python interpreters found in provided path, active virtual environment, or search path
    "###);

    let python_path = python_path_with_versions(&context.temp_dir, &["3.11", "3.12"])
        .expect("Failed to create Python test path");

    // Create some filters for the test interpreters, otherwise they'll be a path on the dev's machine
    // TODO(zanieb): Standardize this when writing more tests
    let python_path_filters = std::env::split_paths(&python_path)
        .zip(["3.11", "3.12"])
        .flat_map(|(path, version)| {
            TestContext::path_patterns(path)
                .into_iter()
                .map(move |pattern| {
                    (
                        format!("{pattern}python.*"),
                        format!("[PYTHON-PATH-{version}]"),
                    )
                })
        })
        .collect::<Vec<_>>();

    let filters = python_path_filters
        .iter()
        .map(|(pattern, replacement)| (pattern.as_str(), replacement.as_str()))
        .chain(context.filters())
        .collect::<Vec<_>>();

    // We find the first interpreter on the path
    uv_snapshot!(filters, context.toolchain_find()
        .env("UV_TEST_PYTHON_PATH", &python_path), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-PATH-3.11]

    ----- stderr -----
    "###);

    // Request Python 3.12
    uv_snapshot!(filters, context.toolchain_find()
        .arg("3.12")
        .env("UV_TEST_PYTHON_PATH", &python_path), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-PATH-3.12]

    ----- stderr -----
    "###);

    // Request Python 3.11
    uv_snapshot!(filters, context.toolchain_find()
        .arg("3.11")
        .env("UV_TEST_PYTHON_PATH", &python_path), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-PATH-3.11]

    ----- stderr -----
    "###);

    // Request CPython
    uv_snapshot!(filters, context.toolchain_find()
        .arg("cpython")
        .env("UV_TEST_PYTHON_PATH", &python_path), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-PATH-3.11]

    ----- stderr -----
    "###);

    // Request CPython 3.12
    uv_snapshot!(filters, context.toolchain_find()
        .arg("cpython@3.12")
        .env("UV_TEST_PYTHON_PATH", &python_path), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-PATH-3.12]

    ----- stderr -----
    "###);

    // Request CPython 3.12 via partial key syntax
    uv_snapshot!(filters, context.toolchain_find()
        .arg("cpython-3.12")
        .env("UV_TEST_PYTHON_PATH", &python_path), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-PATH-3.12]

    ----- stderr -----
    "###);

    // Request CPython 3.12 for the current platform
    let os = Os::from_env();
    let arch = Arch::from_env();

    uv_snapshot!(filters, context.toolchain_find()
    .arg(format!("cpython-3.12-{os}-{arch}"))
    .env("UV_TEST_PYTHON_PATH", &python_path), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-PATH-3.12]

    ----- stderr -----
    "###);

    // Request PyPy
    uv_snapshot!(filters, context.toolchain_find()
        .arg("pypy")
        .env("UV_TEST_PYTHON_PATH", &python_path), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No interpreter found for PyPy in provided path, active virtual environment, or search path
    "###);

    // Swap the order (but don't change the filters to preserve our indices)
    let python_path = python_path_with_versions(&context.temp_dir, &["3.12", "3.11"])
        .expect("Failed to create Python test path");

    uv_snapshot!(filters, context.toolchain_find()
        .env("UV_TEST_PYTHON_PATH", &python_path), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-PATH-3.12]

    ----- stderr -----
    "###);

    // Request Python 3.11
    uv_snapshot!(filters, context.toolchain_find()
        .arg("3.11")
        .env("UV_TEST_PYTHON_PATH", &python_path), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTHON-PATH-3.11]

    ----- stderr -----
    "###);
}
