use std::process::Command;

use anyhow::Result;
use assert_cmd::prelude::*;
use assert_fs::prelude::*;
use url::Url;

use common::{uv_snapshot, INSTA_FILTERS};
use uv_fs::Simplified;

use crate::common::{get_bin, venv_to_interpreter, TestContext};

mod common;

/// Create a `pip uninstall` command with options shared across scenarios.
fn uninstall_command(context: &TestContext) -> Command {
    let mut command = Command::new(get_bin());
    command
        .arg("pip")
        .arg("uninstall")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .env("VIRTUAL_ENV", context.venv.as_os_str())
        .current_dir(&context.temp_dir);

    if cfg!(all(windows, debug_assertions)) {
        // TODO(konstin): Reduce stack usage in debug mode enough that the tests pass with the
        // default windows stack of 1MB
        command.env("UV_STACK_SIZE", (2 * 1024 * 1024).to_string());
    }

    command
}

/// Create a `pip sync` command with options shared across scenarios.
fn sync_command(context: &TestContext) -> Command {
    let mut command = Command::new(get_bin());
    command
        .arg("pip")
        .arg("sync")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .env("VIRTUAL_ENV", context.venv.as_os_str())
        .current_dir(&context.temp_dir);

    if cfg!(all(windows, debug_assertions)) {
        // TODO(konstin): Reduce stack usage in debug mode enough that the tests pass with the
        // default windows stack of 1MB
        command.env("UV_STACK_SIZE", (8 * 1024 * 1024).to_string());
    }

    command
}

#[test]
fn no_arguments() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;

    uv_snapshot!(Command::new(get_bin())
        .arg("pip")
        .arg("uninstall")
        .current_dir(&temp_dir), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: the following required arguments were not provided:
      <PACKAGE|--requirement <REQUIREMENT>|--editable <EDITABLE>>

    Usage: uv pip uninstall <PACKAGE|--requirement <REQUIREMENT>|--editable <EDITABLE>>

    For more information, try '--help'.
    "###
    );

    Ok(())
}

#[test]
fn invalid_requirement() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;

    uv_snapshot!(Command::new(get_bin())
        .arg("pip")
        .arg("uninstall")
        .arg("flask==1.0.x")
        .current_dir(&temp_dir), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to parse `flask==1.0.x`
      Caused by: after parsing 1.0, found ".x" after it, which is not part of a valid version
    flask==1.0.x
         ^^^^^^^
    "###);

    Ok(())
}

#[test]
fn missing_requirements_txt() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;

    uv_snapshot!(Command::new(get_bin())
        .arg("pip")
        .arg("uninstall")
        .arg("-r")
        .arg("requirements.txt")
        .current_dir(&temp_dir), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: failed to read from file `requirements.txt`
      Caused by: No such file or directory (os error 2)
    "###
    );

    Ok(())
}

#[test]
fn invalid_requirements_txt_requirement() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let requirements_txt = temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("flask==1.0.x")?;

    uv_snapshot!(Command::new(get_bin())
        .arg("pip")
        .arg("uninstall")
        .arg("-r")
        .arg("requirements.txt")
        .current_dir(&temp_dir), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Couldn't parse requirement in `requirements.txt` at position 0
      Caused by: after parsing 1.0, found ".x" after it, which is not part of a valid version
    flask==1.0.x
         ^^^^^^^
    "###);

    Ok(())
}

#[test]
fn missing_pyproject_toml() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;

    uv_snapshot!(Command::new(get_bin())
        .arg("pip")
        .arg("uninstall")
        .arg("-r")
        .arg("pyproject.toml")
        .current_dir(&temp_dir), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: failed to read from file `pyproject.toml`
      Caused by: No such file or directory (os error 2)
    "###
    );

    Ok(())
}

#[test]
fn invalid_pyproject_toml_syntax() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let pyproject_toml = temp_dir.child("pyproject.toml");
    pyproject_toml.touch()?;
    pyproject_toml.write_str("123 - 456")?;

    uv_snapshot!(Command::new(get_bin())
        .arg("pip")
        .arg("uninstall")
        .arg("-r")
        .arg("pyproject.toml")
        .current_dir(&temp_dir), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to parse `pyproject.toml`
      Caused by: TOML parse error at line 1, column 5
      |
    1 | 123 - 456
      |     ^
    expected `.`, `=`

    "###);

    Ok(())
}

#[test]
fn invalid_pyproject_toml_schema() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let pyproject_toml = temp_dir.child("pyproject.toml");
    pyproject_toml.touch()?;
    pyproject_toml.write_str("[project]")?;

    uv_snapshot!(Command::new(get_bin())
        .arg("pip")
        .arg("uninstall")
        .arg("-r")
        .arg("pyproject.toml")
        .current_dir(&temp_dir), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to parse `pyproject.toml`
      Caused by: TOML parse error at line 1, column 1
      |
    1 | [project]
      | ^^^^^^^^^
    missing field `name`

    "###);

    Ok(())
}

#[test]
fn invalid_pyproject_toml_requirement() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let pyproject_toml = temp_dir.child("pyproject.toml");
    pyproject_toml.touch()?;
    pyproject_toml.write_str(
        r#"[project]
name = "project"
dependencies = ["flask==1.0.x"]
"#,
    )?;

    uv_snapshot!(Command::new(get_bin())
        .arg("pip")
        .arg("uninstall")
        .arg("-r")
        .arg("pyproject.toml")
        .current_dir(&temp_dir), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to parse `pyproject.toml`
      Caused by: TOML parse error at line 3, column 16
      |
    3 | dependencies = ["flask==1.0.x"]
      |                ^^^^^^^^^^^^^^^^
    after parsing 1.0, found ".x" after it, which is not part of a valid version
    flask==1.0.x
         ^^^^^^^

    "###);

    Ok(())
}

#[test]
fn uninstall() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("MarkupSafe==2.1.3")?;

    sync_command(&context)
        .arg("requirements.txt")
        .assert()
        .success();

    Command::new(venv_to_interpreter(&context.venv))
        .arg("-c")
        .arg("import markupsafe")
        .current_dir(&context.temp_dir)
        .assert()
        .success();

    uv_snapshot!(uninstall_command(&context)
        .arg("MarkupSafe"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Uninstalled 1 package in [TIME]
     - markupsafe==2.1.3
    "###
    );

    Command::new(venv_to_interpreter(&context.venv))
        .arg("-c")
        .arg("import markupsafe")
        .current_dir(&context.temp_dir)
        .assert()
        .failure();

    Ok(())
}

#[test]
fn missing_record() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("MarkupSafe==2.1.3")?;

    sync_command(&context)
        .arg("requirements.txt")
        .assert()
        .success();

    Command::new(venv_to_interpreter(&context.venv))
        .arg("-c")
        .arg("import markupsafe")
        .current_dir(&context.temp_dir)
        .assert()
        .success();

    // Delete the RECORD file.
    let dist_info = fs_err::canonicalize(if cfg!(unix) {
        context
            .venv
            .join("lib")
            .join("python3.12")
            .join("site-packages")
            .join("MarkupSafe-2.1.3.dist-info")
    } else if cfg!(windows) {
        context
            .venv
            .join("Lib")
            .join("site-packages")
            .join("MarkupSafe-2.1.3.dist-info")
    } else {
        unimplemented!("Only Windows and Unix are supported")
    })
    .unwrap();
    fs_err::remove_file(dist_info.join("RECORD"))?;

    let dist_info_str = regex::escape(&format!(
        "RECORD file not found at: {}",
        dist_info.simplified_display()
    ));
    let filters: Vec<_> = [(
        dist_info_str.as_str(),
        "RECORD file not found at: [DIST_INFO]",
    )]
    .into_iter()
    .chain(INSTA_FILTERS.to_vec())
    .collect();

    uv_snapshot!(filters, uninstall_command(&context)
        .arg("MarkupSafe"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Cannot uninstall package; RECORD file not found at: [DIST_INFO]/RECORD
    "###
    );

    Ok(())
}

#[test]
fn uninstall_editable_by_name() -> Result<()> {
    let context = TestContext::new("3.12");

    let current_dir = std::env::current_dir()?;
    let workspace_dir = regex::escape(
        Url::from_directory_path(current_dir.join("..").join("..").canonicalize()?)
            .unwrap()
            .as_str(),
    );

    let filters: Vec<_> = [(workspace_dir.as_str(), "file://[WORKSPACE_DIR]/")]
        .into_iter()
        .chain(INSTA_FILTERS.to_vec())
        .collect();

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("-e ../../scripts/editable-installs/poetry_editable")?;

    sync_command(&context)
        .arg(requirements_txt.path())
        .current_dir(&current_dir)
        .assert()
        .success();

    Command::new(venv_to_interpreter(&context.venv))
        .arg("-c")
        .arg("import poetry_editable")
        .assert()
        .success();

    // Uninstall the editable by name.
    uv_snapshot!(filters, uninstall_command(&context)
        .arg("poetry-editable"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Uninstalled 1 package in [TIME]
     - poetry-editable==0.1.0 (from file://[WORKSPACE_DIR]/scripts/editable-installs/poetry_editable)
    "###
    );

    Command::new(venv_to_interpreter(&context.venv))
        .arg("-c")
        .arg("import poetry_editable")
        .assert()
        .failure();

    Ok(())
}

#[test]
fn uninstall_editable_by_path() -> Result<()> {
    let context = TestContext::new("3.12");

    let current_dir = std::env::current_dir()?;
    let workspace_dir = regex::escape(
        Url::from_directory_path(current_dir.join("..").join("..").canonicalize()?)
            .unwrap()
            .as_str(),
    );

    let filters: Vec<_> = [(workspace_dir.as_str(), "file://[WORKSPACE_DIR]/")]
        .into_iter()
        .chain(INSTA_FILTERS.to_vec())
        .collect();

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("-e ../../scripts/editable-installs/poetry_editable")?;

    sync_command(&context)
        .arg(requirements_txt.path())
        .current_dir(&current_dir)
        .assert()
        .success();

    Command::new(venv_to_interpreter(&context.venv))
        .arg("-c")
        .arg("import poetry_editable")
        .assert()
        .success();

    // Uninstall the editable by path.
    uv_snapshot!(filters, uninstall_command(&context)
        .arg("-e")
        .arg("../../scripts/editable-installs/poetry_editable")
        .current_dir(&current_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Uninstalled 1 package in [TIME]
     - poetry-editable==0.1.0 (from file://[WORKSPACE_DIR]/scripts/editable-installs/poetry_editable)
    "###
    );

    Command::new(venv_to_interpreter(&context.venv))
        .arg("-c")
        .arg("import poetry_editable")
        .assert()
        .failure();

    Ok(())
}

#[test]
fn uninstall_duplicate_editable() -> Result<()> {
    let context = TestContext::new("3.12");

    let current_dir = std::env::current_dir()?;
    let workspace_dir = regex::escape(
        Url::from_directory_path(current_dir.join("..").join("..").canonicalize()?)
            .unwrap()
            .as_str(),
    );

    let filters: Vec<_> = [(workspace_dir.as_str(), "file://[WORKSPACE_DIR]/")]
        .into_iter()
        .chain(INSTA_FILTERS.to_vec())
        .collect();

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("-e ../../scripts/editable-installs/poetry_editable")?;

    sync_command(&context)
        .arg(requirements_txt.path())
        .current_dir(&current_dir)
        .assert()
        .success();

    Command::new(venv_to_interpreter(&context.venv))
        .arg("-c")
        .arg("import poetry_editable")
        .assert()
        .success();

    // Uninstall the editable by both path and name.
    uv_snapshot!(filters, uninstall_command(&context)
        .arg("poetry-editable")
        .arg("-e")
        .arg("../../scripts/editable-installs/poetry_editable")
        .current_dir(&current_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Uninstalled 1 package in [TIME]
     - poetry-editable==0.1.0 (from file://[WORKSPACE_DIR]/scripts/editable-installs/poetry_editable)
    "###
    );

    Command::new(venv_to_interpreter(&context.venv))
        .arg("-c")
        .arg("import poetry_editable")
        .assert()
        .failure();

    Ok(())
}

/// Uninstall a duplicate package in a virtual environment.
#[test]
#[cfg(unix)]
fn uninstall_duplicate() -> Result<()> {
    use crate::common::copy_dir_all;

    // Sync a version of `pip` into a virtual environment.
    let context1 = TestContext::new("3.12");
    let requirements_txt = context1.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("pip==21.3.1")?;

    // Run `pip sync`.
    sync_command(&context1)
        .arg(requirements_txt.path())
        .assert()
        .success();

    // Sync a different version of `pip` into a virtual environment.
    let context2 = TestContext::new("3.12");
    let requirements_txt = context2.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("pip==22.1.1")?;

    // Run `pip sync`.
    sync_command(&context2)
        .arg(requirements_txt.path())
        .assert()
        .success();

    // Copy the virtual environment to a new location.
    copy_dir_all(
        context2
            .venv
            .join("lib/python3.12/site-packages/pip-22.1.1.dist-info"),
        context1
            .venv
            .join("lib/python3.12/site-packages/pip-22.1.1.dist-info"),
    )?;

    // Run `pip uninstall`.
    uv_snapshot!(uninstall_command(&context1)
        .arg("pip"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Uninstalled 2 packages in [TIME]
     - pip==21.3.1
     - pip==22.1.1
    "###
    );

    Ok(())
}
