#![cfg(all(feature = "python", feature = "pypi"))]

use assert_fs::fixture::PathChild;

use common::{uv_snapshot, TestContext};

mod common;

#[test]
fn tool_dir() {
    let context = TestContext::new("3.12");
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    uv_snapshot!(context.filters(), context.tool_dir()
    .env("UV_TOOL_DIR", tool_dir.as_os_str())
    .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/tools

    ----- stderr -----
    warning: `uv tool dir` is experimental and may change without warning
    "###);
}

#[test]
fn tool_dir_bin() {
    let context = TestContext::new("3.12");
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    uv_snapshot!(context.filters(), context.tool_dir().arg("--bin")
    .env("UV_TOOL_DIR", tool_dir.as_os_str())
    .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/bin

    ----- stderr -----
    warning: `uv tool dir` is experimental and may change without warning
    "###);
}
