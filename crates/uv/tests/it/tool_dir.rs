use assert_fs::fixture::PathChild;

use uv_static::EnvVars;

use crate::common::{uv_snapshot, TestContext};

#[test]
fn tool_dir() {
    let context = TestContext::new("3.12");
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    uv_snapshot!(context.filters(), context.tool_dir()
    .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
    .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/tools

    ----- stderr -----
    "###);
}

#[test]
fn tool_dir_bin() {
    let context = TestContext::new("3.12");
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    uv_snapshot!(context.filters(), context.tool_dir().arg("--bin")
    .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
    .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/bin

    ----- stderr -----
    "###);
}
