use assert_fs::fixture::PathChild;

use common::{uv_snapshot, TestContext};

mod common;

#[test]
fn python_dir() {
    let context = TestContext::new("3.12");

    let python_dir = context.temp_dir.child("python");
    uv_snapshot!(context.filters(), context.python_dir()
    .env("UV_PYTHON_INSTALL_DIR", python_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/python

    ----- stderr -----
    "###);
}
