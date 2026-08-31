use std::env::consts::EXE_SUFFIX;

use anyhow::Result;
use assert_cmd::assert::OutputAssertExt;
use assert_fs::prelude::*;
use insta::allow_duplicates;
use predicates::prelude::predicate;

use uv_static::EnvVars;

use uv_test::uv_snapshot;

#[test]
fn tool_uninstall() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_exe_suffix()
        .with_tool_dirs();
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`
    context
        .tool_install()
        .arg("black==24.2.0")
        .assert()
        .success();

    // Package names are normalized before looking up the installed tool.
    uv_snapshot!(context.filters(), context.tool_uninstall().arg("BLACK"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Uninstalled 2 executables: black, blackd
    ");

    // After uninstalling the tool, it shouldn't be listed.
    uv_snapshot!(context.filters(), context.tool_list(), @"
    exit_code: 0 (success)
    ----- stderr -----
    No tools installed
    ");

    // After uninstalling the tool, we should be able to reinstall it.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black==24.2.0")
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 6 packages in [TIME]
    Installed 6 packages in [TIME]
     + black==24.2.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    Installed 2 executables: black, blackd
    ");
}

#[test]
fn tool_uninstall_multiple_names() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_exe_suffix()
        .with_tool_dirs();

    // Install `black`
    context
        .tool_install()
        .arg("black==24.2.0")
        .assert()
        .success();

    context.tool_install().arg("ruff==0.3.4").assert().success();

    uv_snapshot!(context.filters(), context.tool_uninstall().arg("black").arg("ruff"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Uninstalled 3 executables: black, blackd, ruff
    ");

    // After uninstalling the tool, it shouldn't be listed.
    uv_snapshot!(context.filters(), context.tool_list(), @"
    exit_code: 0 (success)
    ----- stderr -----
    No tools installed
    ");
}

#[test]
fn tool_uninstall_not_installed() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_exe_suffix()
        .with_tool_dirs();

    uv_snapshot!(context.filters(), context.tool_uninstall().arg("black"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: `black` is not installed
    ");
}

#[test]
fn tool_uninstall_missing_receipt() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_exe_suffix()
        .with_tool_dirs();
    let tool_dir = context.temp_dir.child("tools");

    // Install `black`
    context
        .tool_install()
        .arg("black==24.2.0")
        .assert()
        .success();

    fs_err::remove_file(tool_dir.join("black").join("uv-receipt.toml")).unwrap();

    uv_snapshot!(context.filters(), context.tool_uninstall().arg("black"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Removed dangling environment for `black`
    ");
}

#[test]
fn tool_uninstall_multiple_names_with_missing_receipt() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_exe_suffix()
        .with_tool_dirs();
    let tool_dir = context.temp_dir.child("tools");

    // Install `black`
    context
        .tool_install()
        .arg("black==24.2.0")
        .assert()
        .success();

    context.tool_install().arg("ruff==0.3.4").assert().success();

    fs_err::remove_file(tool_dir.join("black").join("uv-receipt.toml")).unwrap();

    uv_snapshot!(context.filters(), context.tool_uninstall().arg("black").arg("ruff"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Removed dangling environment for `black`
    Uninstalled 1 executable: ruff
    ");

    // After uninstalling both tools, neither should be listed.
    uv_snapshot!(context.filters(), context.tool_list(), @"
    exit_code: 0 (success)
    ----- stderr -----
    No tools installed
    ");
}

#[test]
fn tool_uninstall_all_missing_receipt() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_exe_suffix()
        .with_tool_dirs();
    let tool_dir = context.temp_dir.child("tools");

    // Install `black`
    context
        .tool_install()
        .arg("black==24.2.0")
        .assert()
        .success();

    fs_err::remove_file(tool_dir.join("black").join("uv-receipt.toml")).unwrap();

    uv_snapshot!(context.filters(), context.tool_uninstall().arg("--all"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Removed dangling environment for `black`
    ");
}

#[test]
fn tool_uninstall_invalid_name() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_exe_suffix()
        .with_tool_dirs();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    context
        .tool_install()
        .arg("black==24.2.0")
        .assert()
        .success();

    // A copied receipt must not cause the original tool's executables to be removed.
    let backup = tool_dir.child("black backup");
    backup.create_dir_all()?;
    fs_err::copy(
        tool_dir.child("black").child("uv-receipt.toml"),
        backup.child("uv-receipt.toml"),
    )?;

    uv_snapshot!(context.filters(), context.tool_uninstall().arg("black backup"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Removed dangling tool directory `tools/black backup`
    ");

    backup.assert(predicate::path::missing());
    tool_dir.child("black").assert(predicate::path::is_dir());
    bin_dir
        .child(format!("black{EXE_SUFFIX}"))
        .assert(predicate::path::exists());
    bin_dir
        .child(format!("blackd{EXE_SUFFIX}"))
        .assert(predicate::path::exists());

    Ok(())
}

#[test]
fn tool_uninstall_all_invalid_name() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_exe_suffix()
        .with_tool_dirs();
    let tool_dir = context.temp_dir.child("tools");

    context
        .tool_install()
        .arg("black==24.2.0")
        .assert()
        .success();

    let backup = tool_dir.child("black backup");
    backup.create_dir_all()?;
    fs_err::copy(
        tool_dir.child("black").child("uv-receipt.toml"),
        backup.child("uv-receipt.toml"),
    )?;

    uv_snapshot!(context.filters(), context.tool_uninstall().arg("--all"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Removed dangling tool directory `tools/black backup`
    Uninstalled 2 executables: black, blackd
    ");

    tool_dir.assert(predicate::path::missing());

    Ok(())
}

#[test]
fn tool_uninstall_invalid_name_requires_directory() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_tool_dirs();
    let tool_dir = context.temp_dir.child("tools");
    tool_dir.create_dir_all()?;
    tool_dir.child("tool backup").write_str("keep")?;

    let outside = context.temp_dir.child("outside");
    outside.create_dir_all()?;
    outside.child("keep").write_str("keep")?;

    // Only an exact directory entry can be removed; paths and regular files are not tools.
    for name in [
        ".".to_owned(),
        "..".to_owned(),
        "../outside".to_owned(),
        "nested/../../outside".to_owned(),
        outside.path().display().to_string(),
        "tool backup".to_owned(),
        ".lock".to_owned(),
    ] {
        let escaped_name = regex::escape(&name);
        let mut filters = vec![(escaped_name.as_str(), "[NAME]")];
        filters.extend(context.filters());

        allow_duplicates! {
            uv_snapshot!(filters, context.tool_uninstall().arg(&name), @"
            exit_code: 2 (failure)
            ----- stderr -----
            error: `[NAME]` is not installed
            ");
        }

        outside.child("keep").assert("keep");
        tool_dir.child("tool backup").assert("keep");
        tool_dir.child(".lock").assert(predicate::path::exists());
    }

    Ok(())
}
