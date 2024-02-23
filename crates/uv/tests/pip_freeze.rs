#![cfg(all(feature = "python", feature = "pypi"))]

use std::process::Command;

use anyhow::Result;
use assert_fs::prelude::*;

use crate::common::{get_bin, uv_snapshot, TestContext};

mod common;

/// Create a `pip freeze` command with options shared across scenarios.
fn command(context: &TestContext) -> Command {
    let mut command = Command::new(get_bin());
    command
        .arg("pip")
        .arg("freeze")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .env("VIRTUAL_ENV", context.venv.as_os_str())
        .current_dir(&context.temp_dir);
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
    command
}

/// List multiple installed packages in a virtual environment.
#[test]
fn freeze_many() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("MarkupSafe==2.1.3\ntomli==2.0.1")?;

    // Run `pip sync`.
    uv_snapshot!(sync_command(&context)
        .arg("requirements.txt"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Downloaded 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + markupsafe==2.1.3
     + tomli==2.0.1
    "###
    );

    // Run `pip freeze`.
    uv_snapshot!(command(&context)
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    markupsafe==2.1.3
    tomli==2.0.1

    ----- stderr -----
    "###
    );

    Ok(())
}

/// List a package with multiple installed distributions in a virtual environment.
#[test]
#[cfg(unix)]
fn freeze_duplicate() -> Result<()> {
    use assert_cmd::assert::OutputAssertExt;

    use crate::common::{copy_dir_all, INSTA_FILTERS};

    // Sync a version of `pip` into a virtual environment.
    let context1 = TestContext::new("3.12");
    let requirements_txt = context1.temp_dir.child("requirements.txt");
    requirements_txt.write_str("pip==21.3.1")?;

    // Run `pip sync`.
    Command::new(get_bin())
        .arg("pip")
        .arg("sync")
        .arg(requirements_txt.path())
        .arg("--cache-dir")
        .arg(context1.cache_dir.path())
        .env("VIRTUAL_ENV", context1.venv.as_os_str())
        .assert()
        .success();

    // Sync a different version of `pip` into a virtual environment.
    let context2 = TestContext::new("3.12");
    let requirements_txt = context2.temp_dir.child("requirements.txt");
    requirements_txt.write_str("pip==22.1.1")?;

    // Run `pip sync`.
    Command::new(get_bin())
        .arg("pip")
        .arg("sync")
        .arg(requirements_txt.path())
        .arg("--cache-dir")
        .arg(context2.cache_dir.path())
        .env("VIRTUAL_ENV", context2.venv.as_os_str())
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

    // Run `pip freeze`.
    let filters = INSTA_FILTERS
        .iter()
        .chain(&[
            (
                ".*/lib/python3.12/site-packages/pip-22.1.1.dist-info",
                "/pip-22.1.1.dist-info",
            ),
            (
                ".*/lib/python3.12/site-packages/pip-21.3.1.dist-info",
                "/pip-21.3.1.dist-info",
            ),
        ])
        .copied()
        .collect::<Vec<_>>();

    uv_snapshot!(filters, command(&context1).arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    pip==21.3.1
    pip==22.1.1

    ----- stderr -----
    warning: The package `pip` has multiple installed distributions:
    /pip-21.3.1.dist-info
    /pip-22.1.1.dist-info
    "###
    );

    Ok(())
}
