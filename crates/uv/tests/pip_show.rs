use std::env::current_dir;
use std::process::Command;

use anyhow::Result;
use assert_cmd::prelude::*;
use assert_fs::fixture::PathChild;
use assert_fs::fixture::{FileTouch, FileWriteStr};
use indoc::indoc;

use common::uv_snapshot;

use crate::common::{get_bin, TestContext, EXCLUDE_NEWER};

mod common;

/// Create a `pip install` command with options shared across scenarios.
fn install_command(context: &TestContext) -> Command {
    let mut command = Command::new(get_bin());
    command
        .arg("pip")
        .arg("install")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .arg("--exclude-newer")
        .arg(EXCLUDE_NEWER)
        .env("VIRTUAL_ENV", context.venv.as_os_str())
        .current_dir(&context.temp_dir);

    if cfg!(all(windows, debug_assertions)) {
        // TODO(konstin): Reduce stack usage in debug mode enough that the tests pass with the
        // default windows stack of 1MB
        command.env("UV_STACK_SIZE", (2 * 1024 * 1024).to_string());
    }

    command
}

#[test]
fn show_empty() {
    let context = TestContext::new("3.12");

    uv_snapshot!(Command::new(get_bin())
        .arg("pip")
        .arg("show")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .env("VIRTUAL_ENV", context.venv.as_os_str())
        .current_dir(&context.temp_dir), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    warning: Please provide a package name or names.
    "###
    );
}

#[test]
fn show_found_single_package() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("MarkupSafe==2.1.3")?;

    uv_snapshot!(install_command(&context)
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + markupsafe==2.1.3
    "###
    );

    context.assert_command("import markupsafe").success();
    let filters = [(
        r"Location:.*site-packages",
        "Location: [WORKSPACE_DIR]/site-packages",
    )]
    .to_vec();

    uv_snapshot!(filters, Command::new(get_bin())
        .arg("pip")
        .arg("show")
        .arg("markupsafe")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .env("VIRTUAL_ENV", context.venv.as_os_str())
        .current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Name: markupsafe
    Version: 2.1.3
    Location: [WORKSPACE_DIR]/site-packages

    ----- stderr -----
    "###
    );

    Ok(())
}

#[test]
fn show_found_multiple_packages() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str(indoc! {r"
        MarkupSafe==2.1.3
        pip==21.3.1
    "
    })?;

    uv_snapshot!(install_command(&context)
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Downloaded 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + markupsafe==2.1.3
     + pip==21.3.1
    "###
    );

    context.assert_command("import markupsafe").success();

    // In addition to the standard filters, remove the temporary directory from the snapshot.
    let filters = [(
        r"Location:.*site-packages",
        "Location: [WORKSPACE_DIR]/site-packages",
    )]
    .to_vec();

    uv_snapshot!(filters, Command::new(get_bin())
        .arg("pip")
        .arg("show")
        .arg("markupsafe")
        .arg("pip")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .env("VIRTUAL_ENV", context.venv.as_os_str())
        .current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Name: markupsafe
    Version: 2.1.3
    Location: [WORKSPACE_DIR]/site-packages
    ---
    Name: pip
    Version: 21.3.1
    Location: [WORKSPACE_DIR]/site-packages

    ----- stderr -----
    "###
    );

    Ok(())
}

#[test]
fn show_found_one_out_of_two() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str(indoc! {r"
        MarkupSafe==2.1.3
        pip==21.3.1
    "
    })?;

    uv_snapshot!(install_command(&context)
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Downloaded 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + markupsafe==2.1.3
     + pip==21.3.1
    "###
    );

    context.assert_command("import markupsafe").success();

    // In addition to the standard filters, remove the temporary directory from the snapshot.
    let filters = [(
        r"Location:.*site-packages",
        "Location: [WORKSPACE_DIR]/site-packages",
    )]
    .to_vec();

    uv_snapshot!(filters, Command::new(get_bin())
        .arg("pip")
        .arg("show")
        .arg("markupsafe")
        .arg("flask")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .env("VIRTUAL_ENV", context.venv.as_os_str())
        .current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Name: markupsafe
    Version: 2.1.3
    Location: [WORKSPACE_DIR]/site-packages

    ----- stderr -----
    warning: Package(s) not found for: flask
    "###
    );

    Ok(())
}

#[test]
fn show_found_one_out_of_two_quiet() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str(indoc! {r"
        MarkupSafe==2.1.3
        pip==21.3.1
    "
    })?;

    uv_snapshot!(install_command(&context)
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Downloaded 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + markupsafe==2.1.3
     + pip==21.3.1
    "###
    );

    context.assert_command("import markupsafe").success();

    // Flask isn't installed, but markupsafe is, so the command should succeed.
    uv_snapshot!(Command::new(get_bin())
        .arg("pip")
        .arg("show")
        .arg("markupsafe")
        .arg("flask")
        .arg("--quiet")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .env("VIRTUAL_ENV", context.venv.as_os_str())
        .current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    "###
    );

    Ok(())
}

#[test]
fn show_empty_quiet() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str(indoc! {r"
        MarkupSafe==2.1.3
        pip==21.3.1
    "
    })?;

    uv_snapshot!(install_command(&context)
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Downloaded 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + markupsafe==2.1.3
     + pip==21.3.1
    "###
    );

    context.assert_command("import markupsafe").success();

    // Flask isn't installed, so the command should fail.
    uv_snapshot!(Command::new(get_bin())
        .arg("pip")
        .arg("show")
        .arg("flask")
        .arg("--quiet")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .env("VIRTUAL_ENV", context.venv.as_os_str())
        .current_dir(&context.temp_dir), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    "###
    );

    Ok(())
}

#[test]
fn show_editable() -> Result<()> {
    let context = TestContext::new("3.12");

    // Install the editable package.
    install_command(&context)
        .arg("-e")
        .arg("../../scripts/editable-installs/poetry_editable")
        .current_dir(current_dir()?)
        .env(
            "CARGO_TARGET_DIR",
            "../../../target/target_install_editable",
        )
        .assert()
        .success();

    // In addition to the standard filters, remove the temporary directory from the snapshot.
    let filters = [(
        r"Location:.*site-packages",
        "Location: [WORKSPACE_DIR]/site-packages",
    )]
    .to_vec();

    uv_snapshot!(filters, Command::new(get_bin())
        .arg("pip")
        .arg("show")
        .arg("poetry-editable")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .env("VIRTUAL_ENV", context.venv.as_os_str())
        .current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Name: poetry-editable
    Version: 0.1.0
    Location: [WORKSPACE_DIR]/site-packages

    ----- stderr -----
    "###
    );

    Ok(())
}
