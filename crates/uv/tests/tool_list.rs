#![cfg(all(feature = "python", feature = "pypi"))]

use anyhow::Result;
use assert_cmd::assert::OutputAssertExt;
use assert_fs::fixture::PathChild;
use common::{uv_snapshot, TestContext};
use fs_err as fs;
use insta::assert_snapshot;

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
    Provide a command to invoke with `uvx <command>` or `uvx --from <package> <command>`.

    The following tools are already installed:

    black v24.2.0
    - black
    - blackd

    See `uvx --help` for more information.

    ----- stderr -----
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
    Provide a command to invoke with `uvx <command>` or `uvx --from <package> <command>`.

    The following tools are already installed:

    black v24.2.0 ([TEMP_DIR]/tools/black)
    - black ([TEMP_DIR]/bin/black)
    - blackd ([TEMP_DIR]/bin/blackd)

    See `uvx --help` for more information.

    ----- stderr -----
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
    No tools installed.

    See `uv tool install --help` for more information.
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
    Provide a command to invoke with `uvx <command>` or `uvx --from <package> <command>`.

    The following tools are already installed:


    See `uvx --help` for more information.

    ----- stderr -----
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
    Provide a command to invoke with `uvx <command>` or `uvx --from <package> <command>`.

    The following tools are already installed:

    ruff v0.3.4
    - ruff

    See `uvx --help` for more information.

    ----- stderr -----
    Invalid environment at `tools/black`: missing Python executable at `tools/black/[BIN]/python`
    "###
    );

    Ok(())
}

#[test]
fn tool_list_deprecated() -> Result<()> {
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

    // Ensure that we have a modern tool receipt.
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r###"
        [tool]
        requirements = [{ name = "black", specifier = "==24.2.0" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "###);
    });

    // Replace with a legacy receipt.
    fs::write(
        tool_dir.join("black").join("uv-receipt.toml"),
        r#"
        [tool]
        requirements = ["black==24.2.0"]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd" },
        ]
        "#,
    )?;

    // Ensure that we can still list the tool.
    uv_snapshot!(context.filters(), context.tool_list()
    .env("UV_TOOL_DIR", tool_dir.as_os_str())
    .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Provide a command to invoke with `uvx <command>` or `uvx --from <package> <command>`.

    The following tools are already installed:

    black v24.2.0
    - black
    - blackd

    See `uvx --help` for more information.

    ----- stderr -----
    "###);

    // Replace with an invalid receipt.
    fs::write(
        tool_dir.join("black").join("uv-receipt.toml"),
        r#"
        [tool]
        requirements = ["black<>24.2.0"]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd" },
        ]
        "#,
    )?;

    // Ensure that listing fails.
    uv_snapshot!(context.filters(), context.tool_list()
    .env("UV_TOOL_DIR", tool_dir.as_os_str())
    .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Provide a command to invoke with `uvx <command>` or `uvx --from <package> <command>`.

    The following tools are already installed:


    See `uvx --help` for more information.

    ----- stderr -----
    warning: Ignoring malformed tool `black` (run `uv tool uninstall black` to remove)
    "###);

    Ok(())
}

#[test]
fn tool_list_show_version_specifiers() {
    let context = TestContext::new("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`
    context
        .tool_install()
        .arg("black<24.3.0")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .assert()
        .success();

    uv_snapshot!(context.filters(), context.tool_list().arg("--show-version-specifiers")
    .env("UV_TOOL_DIR", tool_dir.as_os_str())
    .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Provide a command to invoke with `uvx <command>` or `uvx --from <package> <command>`.

    The following tools are already installed:

    black v24.2.0 [required: <24.3.0]
    - black
    - blackd

    See `uvx --help` for more information.

    ----- stderr -----
    "###);

    // with paths
    uv_snapshot!(context.filters(), context.tool_list().arg("--show-version-specifiers").arg("--show-paths")
    .env("UV_TOOL_DIR", tool_dir.as_os_str())
    .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Provide a command to invoke with `uvx <command>` or `uvx --from <package> <command>`.

    The following tools are already installed:

    black v24.2.0 [required: <24.3.0] ([TEMP_DIR]/tools/black)
    - black ([TEMP_DIR]/bin/black)
    - blackd ([TEMP_DIR]/bin/blackd)

    See `uvx --help` for more information.

    ----- stderr -----
    "###);
}
