use std::process::Command;

use anyhow::Result;
use assert_cmd::prelude::*;
use assert_fs::fixture::ChildPath;
use assert_fs::prelude::*;

use common::uv_snapshot;

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
        .env("UV_NO_WRAP", "1")
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
        .env("UV_NO_WRAP", "1")
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
      <PACKAGE|--requirement <REQUIREMENT>>

    Usage: uv pip uninstall <PACKAGE|--requirement <REQUIREMENT>>

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
    error: Failed to parse: `flask==1.0.x`
      Caused by: after parsing '1.0', found '.x', which is not part of a valid version
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
    error: File not found: `requirements.txt`
    "###
    );

    Ok(())
}

#[test]
fn invalid_requirements_txt_requirement() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let requirements_txt = temp_dir.child("requirements.txt");
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
      Caused by: after parsing '1.0', found '.x', which is not part of a valid version
    flask==1.0.x
         ^^^^^^^
    "###);

    Ok(())
}

#[test]
fn uninstall() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
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
    let dist_info = context.site_packages().join("MarkupSafe-2.1.3.dist-info");
    fs_err::remove_file(dist_info.join("RECORD"))?;

    uv_snapshot!(context.filters(), uninstall_command(&context)
        .arg("MarkupSafe"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Cannot uninstall package; `RECORD` file not found at: [SITE_PACKAGES]/MarkupSafe-2.1.3.dist-info/RECORD
    "###
    );

    Ok(())
}

#[test]
fn uninstall_editable_by_name() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(&format!(
        "-e {}",
        context
            .workspace_root
            .join("scripts/packages/poetry_editable")
            .as_os_str()
            .to_str()
            .expect("Path is valid unicode")
    ))?;
    sync_command(&context)
        .arg(requirements_txt.path())
        .assert()
        .success();

    Command::new(venv_to_interpreter(&context.venv))
        .arg("-c")
        .arg("import poetry_editable")
        .assert()
        .success();

    // Uninstall the editable by name.
    uv_snapshot!(context.filters(), uninstall_command(&context)
        .arg("poetry-editable"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Uninstalled 1 package in [TIME]
     - poetry-editable==0.1.0 (from file://[WORKSPACE]/scripts/packages/poetry_editable)
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
fn uninstall_by_path() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(
        context
            .workspace_root
            .join("scripts/packages/poetry_editable")
            .as_os_str()
            .to_str()
            .expect("Path is valid unicode"),
    )?;

    sync_command(&context)
        .arg(requirements_txt.path())
        .assert()
        .success();

    Command::new(venv_to_interpreter(&context.venv))
        .arg("-c")
        .arg("import poetry_editable")
        .assert()
        .success();

    // Uninstall the editable by path.
    uv_snapshot!(context.filters(), uninstall_command(&context)
        .arg(context.workspace_root.join("scripts/packages/poetry_editable")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Uninstalled 1 package in [TIME]
     - poetry-editable==0.1.0 (from file://[WORKSPACE]/scripts/packages/poetry_editable)
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
fn uninstall_duplicate_by_path() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(
        context
            .workspace_root
            .join("scripts/packages/poetry_editable")
            .as_os_str()
            .to_str()
            .expect("Path is valid unicode"),
    )?;

    sync_command(&context)
        .arg(requirements_txt.path())
        .assert()
        .success();

    Command::new(venv_to_interpreter(&context.venv))
        .arg("-c")
        .arg("import poetry_editable")
        .assert()
        .success();

    // Uninstall the editable by both path and name.
    uv_snapshot!(context.filters(), uninstall_command(&context)
        .arg("poetry-editable")
        .arg(context.workspace_root.join("scripts/packages/poetry_editable")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Uninstalled 1 package in [TIME]
     - poetry-editable==0.1.0 (from file://[WORKSPACE]/scripts/packages/poetry_editable)
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
fn uninstall_duplicate() -> Result<()> {
    use crate::common::copy_dir_all;

    // Sync a version of `pip` into a virtual environment.
    let context1 = TestContext::new("3.12");
    let requirements_txt = context1.temp_dir.child("requirements.txt");
    requirements_txt.write_str("pip==21.3.1")?;

    // Run `pip sync`.
    sync_command(&context1)
        .arg(requirements_txt.path())
        .assert()
        .success();

    // Sync a different version of `pip` into a virtual environment.
    let context2 = TestContext::new("3.12");
    let requirements_txt = context2.temp_dir.child("requirements.txt");
    requirements_txt.write_str("pip==22.1.1")?;

    // Run `pip sync`.
    sync_command(&context2)
        .arg(requirements_txt.path())
        .assert()
        .success();

    // Copy the virtual environment to a new location.
    copy_dir_all(
        context2.site_packages().join("pip-22.1.1.dist-info"),
        context1.site_packages().join("pip-22.1.1.dist-info"),
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

/// Uninstall a `.egg-info` package in a virtual environment.
#[test]
fn uninstall_egg_info() -> Result<()> {
    let context = TestContext::new("3.12");

    let site_packages = ChildPath::new(context.site_packages());

    // Manually create a `.egg-info` directory.
    site_packages
        .child("zstandard-0.22.0-py3.12.egg-info")
        .create_dir_all()?;
    site_packages
        .child("zstandard-0.22.0-py3.12.egg-info")
        .child("top_level.txt")
        .write_str("zstd")?;
    site_packages
        .child("zstandard-0.22.0-py3.12.egg-info")
        .child("SOURCES.txt")
        .write_str("")?;
    site_packages
        .child("zstandard-0.22.0-py3.12.egg-info")
        .child("PKG-INFO")
        .write_str("")?;
    site_packages
        .child("zstandard-0.22.0-py3.12.egg-info")
        .child("dependency_links.txt")
        .write_str("")?;
    site_packages
        .child("zstandard-0.22.0-py3.12.egg-info")
        .child("entry_points.txt")
        .write_str("")?;

    // Manually create the package directory.
    site_packages.child("zstd").create_dir_all()?;
    site_packages
        .child("zstd")
        .child("__init__.py")
        .write_str("")?;

    // Run `pip uninstall`.
    uv_snapshot!(uninstall_command(&context)
        .arg("zstandard"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Uninstalled 1 package in [TIME]
     - zstandard==0.22.0
    "###);

    Ok(())
}

fn normcase(s: &str) -> String {
    if cfg!(windows) {
        s.replace('/', "\\").to_lowercase()
    } else {
        s.to_owned()
    }
}

/// Uninstall a legacy editable package in a virtual environment.
#[test]
fn uninstall_legacy_editable() -> Result<()> {
    let context = TestContext::new("3.12");

    let site_packages = ChildPath::new(context.site_packages());

    let target = context.temp_dir.child("zstandard_project");
    target.child("zstd").create_dir_all()?;
    target.child("zstd").child("__init__.py").write_str("")?;

    target.child("zstandard.egg-info").create_dir_all()?;
    target
        .child("zstandard.egg-info")
        .child("PKG-INFO")
        .write_str(
            "Metadata-Version: 2.1
Name: zstandard
Version: 0.22.0
",
        )?;

    site_packages
        .child("zstandard.egg-link")
        .write_str(target.path().to_str().unwrap())?;

    site_packages.child("easy-install.pth").write_str(&format!(
        "something\n{}\nanother thing\n",
        normcase(target.path().to_str().unwrap())
    ))?;

    // Run `pip uninstall`.
    uv_snapshot!(uninstall_command(&context)
        .arg("zstandard"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Uninstalled 1 package in [TIME]
     - zstandard==0.22.0
    "###);

    // The entry in `easy-install.pth` should be removed.
    assert_eq!(
        fs_err::read_to_string(site_packages.child("easy-install.pth"))?,
        "something\nanother thing\n",
        "easy-install.pth should not contain the path to the uninstalled package"
    );
    // The `.egg-link` file should be removed.
    assert!(!site_packages.child("zstandard.egg-link").exists());
    // The `.egg-info` directory should still exist.
    assert!(target.child("zstandard.egg-info").exists());

    Ok(())
}
