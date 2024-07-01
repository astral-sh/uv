#![cfg(all(feature = "python", feature = "pypi"))]

use assert_fs::fixture::PathChild;
use common::{uv_snapshot, TestContext};

mod common;

#[test]
fn toolchain_dir() {
    let context = TestContext::new("3.12");

    let toolchain_dir = context.temp_dir.child("toolchains");
    uv_snapshot!(context.filters(), context.toolchain_dir()
    .env("UV_TOOLCHAIN_DIR", toolchain_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/toolchains

    ----- stderr -----
    warning: `uv toolchain dir` is experimental and may change without warning.
    "###);
}
