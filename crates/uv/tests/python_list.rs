#![cfg(feature = "python")]

use assert_fs::prelude::PathChild;

use common::{uv_snapshot, TestContext};

mod common;

#[test]
fn python_list() {
    let context: TestContext = TestContext::new_with_versions(&["3.11", "3.12"]);

    let filters: Vec<_> = [("-> .*", "-> [LINK PATH]")]
        .into_iter()
        .chain(context.filters())
        .collect();

    uv_snapshot!(filters, context.python_list(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.12.[X]-macos-aarch64-none    [PYTHON-3.12] -> [LINK PATH]
    cpython-3.11.[X]-macos-aarch64-none    [PYTHON-3.11] -> [LINK PATH]

    ----- stderr -----
    "###);
}

#[test]
fn python_list_no_versions() {
    let context: TestContext = TestContext::new_with_versions(&[]);

    uv_snapshot!(context.filters(), context.python_list().env("UV_TEST_PYTHON_PATH", ""), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    "###);
}

#[cfg(unix)]
#[test]
fn python_list_symlink() {
    let context: TestContext = TestContext::new_with_versions(&["3.11", "3.12"]);

    let filters: Vec<_> = [("-> .*", "-> [LINK PATH]")]
        .into_iter()
        .chain(context.filters())
        .collect();

    let target = &context.python_versions.first().unwrap().1;
    let link = context.temp_dir.child("python");
    fs_err::os::unix::fs::symlink(target, &link).unwrap();

    let mut path = context.python_path();
    path.push(":");
    path.push(link.parent().unwrap().as_os_str());

    uv_snapshot!(filters, context.python_list().env("UV_TEST_PYTHON_PATH", path), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.12.[X]-macos-aarch64-none    [PYTHON-3.12] -> [LINK PATH]
    cpython-3.11.[X]-macos-aarch64-none    python -> [LINK PATH]
    cpython-3.11.[X]-macos-aarch64-none    [PYTHON-3.11] -> [LINK PATH]

    ----- stderr -----
    "###);
}
