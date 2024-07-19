#![cfg(all(feature = "python", feature = "pypi"))]

use anyhow::Result;
use assert_cmd::assert::OutputAssertExt;
use assert_fs::fixture::PathChild;
use fs_err as fs;

use common::{uv_snapshot, TestContext};

mod common;

#[test]
fn tool_list() {
    let context = TestContext::new("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`
    context
        .tool_install()
        .arg("black==24.2.0")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .assert()
        .success();

    uv_snapshot!(context.filters(), context.tool_list()
    .env("UV_TOOL_DIR", tool_dir.as_os_str())
    .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    black v24.2.0
    - black
    - blackd

    ----- stderr -----
    warning: `uv tool list` is experimental and may change without warning
    "###);
}

#[test]
fn tool_list_paths() {
    let context = TestContext::new("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`
    context
        .tool_install()
        .arg("black==24.2.0")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .assert()
        .success();

    uv_snapshot!(context.filters(), context.tool_list().arg("--show-paths")
    .env("UV_TOOL_DIR", tool_dir.as_os_str())
    .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    black v24.2.0 ([TEMP_DIR]/tools/black)
    - black ([TEMP_DIR]/bin/black)
    - blackd ([TEMP_DIR]/bin/blackd)

    ----- stderr -----
    warning: `uv tool list` is experimental and may change without warning
    "###);
}

#[test]
fn tool_list_empty() {
    let context = TestContext::new("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    uv_snapshot!(context.filters(), context.tool_list()
    .env("UV_TOOL_DIR", tool_dir.as_os_str())
    .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool list` is experimental and may change without warning
    No tools installed
    "###);
}

#[test]
fn tool_list_missing_receipt() {
    let context = TestContext::new("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`
    context
        .tool_install()
        .arg("black==24.2.0")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .assert()
        .success();

    fs_err::remove_file(tool_dir.join("black").join("uv-receipt.toml")).unwrap();

    uv_snapshot!(context.filters(), context.tool_list()
    .env("UV_TOOL_DIR", tool_dir.as_os_str())
    .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool list` is experimental and may change without warning
    warning: Ignoring malformed tool `black` (run `uv tool uninstall black` to remove)
    "###);
}

#[test]
fn tool_list_bad_environment() -> Result<()> {
    let context = TestContext::new("3.12")
        .with_filtered_python_names()
        .with_filtered_virtualenv_bin()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`
    context
        .tool_install()
        .arg("black==24.2.0")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .assert()
        .success();

    // Install `ruff`
    context
        .tool_install()
        .arg("ruff==0.3.4")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .assert()
        .success();

    let venv_path = common::venv_bin_path(tool_dir.path().join("black"));
    // Remove the python interpreter for black
    fs::remove_dir_all(venv_path.clone())?;

    uv_snapshot!(
        context.filters(),
        context
            .tool_list()
            .env("UV_TOOL_DIR", tool_dir.as_os_str())
            .env("XDG_BIN_HOME", bin_dir.as_os_str()),
        @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    ruff v0.3.4
    - ruff

    ----- stderr -----
    warning: `uv tool list` is experimental and may change without warning
    Python interpreter not found at `[TEMP_DIR]/tools/black/[BIN]/python`
    "###
    );

    Ok(())
}
