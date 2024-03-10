#![cfg(all(feature = "python", feature = "pypi"))]

use std::process::Command;

use anyhow::Result;
use assert_cmd::prelude::*;
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

/// Create a `pip install` command with options shared across scenarios.
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
        command.env("UV_STACK_SIZE", (2 * 1024 * 1024).to_string());
    }

    command
}

/// List multiple installed packages in a virtual environment.
#[test]
fn freeze_many() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("MarkupSafe==2.1.3\ntomli==2.0.1")?;

    // Run `pip sync`.
    sync_command(&context)
        .arg(requirements_txt.path())
        .assert()
        .success();

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
    use crate::common::{copy_dir_all, INSTA_FILTERS};

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

/// List a direct URL package in a virtual environment.
#[test]
fn freeze_url() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("anyio\niniconfig @ https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl")?;

    // Run `pip sync`.
    sync_command(&context)
        .arg(requirements_txt.path())
        .assert()
        .success();

    // Run `pip freeze`.
    uv_snapshot!(command(&context)
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    anyio==4.3.0
    iniconfig @ https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl

    ----- stderr -----
    warning: The package `anyio` requires `idna >=2.8`, but it's not installed.
    warning: The package `anyio` requires `sniffio >=1.1`, but it's not installed.
    "###
    );

    Ok(())
}
