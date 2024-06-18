use std::env;
use std::path::PathBuf;
use std::process::Command;

use anyhow::Result;
use assert_cmd::assert::OutputAssertExt;

use crate::common::{copy_dir_ignore, get_bin, uv_snapshot, TestContext, EXCLUDE_NEWER};

mod common;

/// A `pip install` command for workspaces.
///
/// The goal of the workspace tests is to resolve local workspace packages correctly. We add some
/// non-workspace dependencies to ensure that transitive non-workspace dependencies are also
/// correctly resolved.
fn install_workspace(context: &TestContext) -> Command {
    let mut command = Command::new(get_bin());
    command
        .arg("pip")
        .arg("install")
        .arg("--preview")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .arg("--exclude-newer")
        .arg(EXCLUDE_NEWER)
        .arg("-e")
        .env("VIRTUAL_ENV", context.venv.as_os_str())
        .env("UV_NO_WRAP", "1")
        .current_dir(&context.temp_dir);

    if cfg!(all(windows, debug_assertions)) {
        // TODO(konstin): Reduce stack usage in debug mode enough that the tests pass with the
        // default windows stack of 1MB
        command.env("UV_STACK_SIZE", (4 * 1024 * 1024).to_string());
    }

    command
}

/// A `uv run` command.
fn run_workspace(context: &TestContext) -> Command {
    let mut command = Command::new(get_bin());
    command
        .arg("run")
        .arg("--preview")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .arg("--python")
        .arg(context.interpreter())
        .arg("--exclude-newer")
        .arg(EXCLUDE_NEWER)
        .env("UV_NO_WRAP", "1");

    if cfg!(all(windows, debug_assertions)) {
        // TODO(konstin): Reduce stack usage in debug mode enough that the tests pass with the
        // default windows stack of 1MB
        command.env("UV_STACK_SIZE", (4 * 1024 * 1024).to_string());
    }
    command
}

/// A `uv lock` command.
fn lock_workspace(context: &TestContext) -> Command {
    let mut command = Command::new(get_bin());
    command
        .arg("lock")
        .arg("--preview")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .arg("--python")
        .arg(context.interpreter())
        .arg("--exclude-newer")
        .arg(EXCLUDE_NEWER)
        .env("UV_NO_WRAP", "1");

    if cfg!(all(windows, debug_assertions)) {
        // TODO(konstin): Reduce stack usage in debug mode enough that the tests pass with the
        // default windows stack of 1MB
        command.env("UV_STACK_SIZE", (4 * 1024 * 1024).to_string());
    }
    command
}

fn workspaces_dir() -> PathBuf {
    env::current_dir()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("scripts")
        .join("workspaces")
}

#[test]
fn test_albatross_in_examples_bird_feeder() {
    let context = TestContext::new("3.12");
    let current_dir = workspaces_dir()
        .join("albatross-in-example")
        .join("examples")
        .join("bird-feeder");

    uv_snapshot!(context.filters(), install_workspace(&context).arg(&current_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.3.0
     + bird-feeder==1.0.0 (from file://[WORKSPACE]/scripts/workspaces/albatross-in-example/examples/bird-feeder)
     + idna==3.6
     + sniffio==1.3.1
    "###
    );

    context.assert_file(current_dir.join("check_installed_bird_feeder.py"));
    uv_snapshot!(context.filters(), install_workspace(&context).arg(&current_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    "###
    );

    context.assert_file(current_dir.join("check_installed_bird_feeder.py"));
}

#[test]
fn test_albatross_in_examples() {
    let context = TestContext::new("3.12");
    let current_dir = workspaces_dir().join("albatross-in-example");

    uv_snapshot!(context.filters(), install_workspace(&context).arg(&current_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + albatross==0.1.0 (from file://[WORKSPACE]/scripts/workspaces/albatross-in-example)
     + tqdm==4.66.2
    "###
    );

    context.assert_file(current_dir.join("check_installed_albatross.py"));
    uv_snapshot!(context.filters(), install_workspace(&context).arg(&current_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    "###
    );

    context.assert_file(current_dir.join("check_installed_albatross.py"));
}

#[test]
fn test_albatross_just_project() {
    let context = TestContext::new("3.12");
    let current_dir = workspaces_dir().join("albatross-just-project");

    uv_snapshot!(context.filters(), install_workspace(&context).arg(&current_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + albatross==0.1.0 (from file://[WORKSPACE]/scripts/workspaces/albatross-just-project)
     + tqdm==4.66.2
    "###
    );

    context.assert_file(current_dir.join("check_installed_albatross.py"));
    uv_snapshot!(context.filters(), install_workspace(&context).arg(&current_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    "###
    );

    context.assert_file(current_dir.join("check_installed_albatross.py"));
}

#[test]
fn test_albatross_project_in_excluded() {
    let context = TestContext::new("3.12");
    let current_dir = workspaces_dir()
        .join("albatross-project-in-excluded")
        .join("excluded")
        .join("bird-feeder");

    uv_snapshot!(context.filters(), install_workspace(&context).arg(&current_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.3.0
     + bird-feeder==1.0.0 (from file://[WORKSPACE]/scripts/workspaces/albatross-project-in-excluded/excluded/bird-feeder)
     + idna==3.6
     + sniffio==1.3.1
    "###
    );

    context.assert_file(current_dir.join("check_installed_bird_feeder.py"));
    uv_snapshot!(context.filters(), install_workspace(&context).arg(&current_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    "###
    );

    context.assert_file(current_dir.join("check_installed_bird_feeder.py"));
}

#[test]
fn test_albatross_root_workspace() {
    let context = TestContext::new("3.12");
    let current_dir = workspaces_dir().join("albatross-root-workspace");

    uv_snapshot!(context.filters(), install_workspace(&context).arg(&current_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
    Prepared 7 packages in [TIME]
    Installed 7 packages in [TIME]
     + albatross==0.1.0 (from file://[WORKSPACE]/scripts/workspaces/albatross-root-workspace)
     + anyio==4.3.0
     + bird-feeder==1.0.0 (from file://[WORKSPACE]/scripts/workspaces/albatross-root-workspace/packages/bird-feeder)
     + idna==3.6
     + seeds==1.0.0 (from file://[WORKSPACE]/scripts/workspaces/albatross-root-workspace/packages/seeds)
     + sniffio==1.3.1
     + tqdm==4.66.2
    "###
    );

    context.assert_file(current_dir.join("check_installed_albatross.py"));
    uv_snapshot!(context.filters(), install_workspace(&context).arg(&current_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    "###
    );

    context.assert_file(current_dir.join("check_installed_albatross.py"));
}

#[test]
fn test_albatross_root_workspace_bird_feeder() {
    let context = TestContext::new("3.12");
    let current_dir = workspaces_dir()
        .join("albatross-root-workspace")
        .join("packages")
        .join("bird-feeder");

    uv_snapshot!(context.filters(), install_workspace(&context).arg(&current_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Prepared 5 packages in [TIME]
    Installed 5 packages in [TIME]
     + anyio==4.3.0
     + bird-feeder==1.0.0 (from file://[WORKSPACE]/scripts/workspaces/albatross-root-workspace/packages/bird-feeder)
     + idna==3.6
     + seeds==1.0.0 (from file://[WORKSPACE]/scripts/workspaces/albatross-root-workspace/packages/seeds)
     + sniffio==1.3.1
    "###
    );

    context.assert_file(current_dir.join("check_installed_bird_feeder.py"));
    uv_snapshot!(context.filters(), install_workspace(&context).arg(&current_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    "###
    );

    context.assert_file(current_dir.join("check_installed_bird_feeder.py"));
}

#[test]
fn test_albatross_root_workspace_albatross() {
    let context = TestContext::new("3.12");
    let current_dir = workspaces_dir()
        .join("albatross-root-workspace")
        .join("packages")
        .join("bird-feeder");

    uv_snapshot!(context.filters(), install_workspace(&context).arg(&current_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Prepared 5 packages in [TIME]
    Installed 5 packages in [TIME]
     + anyio==4.3.0
     + bird-feeder==1.0.0 (from file://[WORKSPACE]/scripts/workspaces/albatross-root-workspace/packages/bird-feeder)
     + idna==3.6
     + seeds==1.0.0 (from file://[WORKSPACE]/scripts/workspaces/albatross-root-workspace/packages/seeds)
     + sniffio==1.3.1
    "###
    );

    context.assert_file(current_dir.join("check_installed_albatross.py"));
    uv_snapshot!(context.filters(), install_workspace(&context).arg(&current_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    "###
    );

    context.assert_file(current_dir.join("check_installed_albatross.py"));
}

#[test]
fn test_albatross_virtual_workspace() {
    let context = TestContext::new("3.12");
    let current_dir = workspaces_dir()
        .join("albatross-virtual-workspace")
        .join("packages")
        .join("bird-feeder");

    uv_snapshot!(context.filters(), install_workspace(&context).arg(&current_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Prepared 5 packages in [TIME]
    Installed 5 packages in [TIME]
     + anyio==4.3.0
     + bird-feeder==1.0.0 (from file://[WORKSPACE]/scripts/workspaces/albatross-virtual-workspace/packages/bird-feeder)
     + idna==3.6
     + seeds==1.0.0 (from file://[WORKSPACE]/scripts/workspaces/albatross-virtual-workspace/packages/seeds)
     + sniffio==1.3.1
    "###
    );

    context.assert_file(current_dir.join("check_installed_bird_feeder.py"));
    uv_snapshot!(context.filters(), install_workspace(&context).arg(&current_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    "###
    );

    context.assert_file(current_dir.join("check_installed_bird_feeder.py"));
}

/// Check that `uv run --package` works in a virtual workspace.
#[test]
fn test_uv_run_with_package_virtual_workspace() -> Result<()> {
    let context = TestContext::new("3.12");
    let work_dir = context.temp_dir.join("albatross-virtual-workspace");

    copy_dir_ignore(
        workspaces_dir().join("albatross-virtual-workspace"),
        &work_dir,
    )?;

    let mut filters = context.filters();
    filters.push((
        r"Using Python 3.12.\[X\] interpreter at: .*",
        "Using Python 3.12.[X] interpreter at: [PYTHON]",
    ));

    uv_snapshot!(filters, run_workspace(&context)
        .arg("--package")
        .arg("bird-feeder")
        .arg("packages/bird-feeder/check_installed_bird_feeder.py")
        .current_dir(&work_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Success

    ----- stderr -----
    Using Python 3.12.[X] interpreter at: [PYTHON]
    Creating virtualenv at: .venv
    Resolved 8 packages in [TIME]
    Prepared 5 packages in [TIME]
    Installed 5 packages in [TIME]
     + anyio==4.3.0
     + bird-feeder==1.0.0 (from file://[TEMP_DIR]/albatross-virtual-workspace/packages/bird-feeder)
     + idna==3.6
     + seeds==1.0.0 (from file://[TEMP_DIR]/albatross-virtual-workspace/packages/seeds)
     + sniffio==1.3.1
    "###
    );

    uv_snapshot!(context.filters(), universal_windows_filters=true, run_workspace(&context)
            .arg("--package")
            .arg("albatross")
            .arg("packages/albatross/check_installed_albatross.py")
            .current_dir(&work_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Success

    ----- stderr -----
    Resolved 8 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + albatross==0.1.0 (from file://[TEMP_DIR]/albatross-virtual-workspace/packages/albatross)
     + tqdm==4.66.2
    "###
    );

    Ok(())
}

/// Check that `uv run --package` works in a root workspace.
#[test]
fn test_uv_run_with_package_root_workspace() -> Result<()> {
    let context = TestContext::new("3.12");
    let work_dir = context.temp_dir.join("albatross-root-workspace");

    copy_dir_ignore(workspaces_dir().join("albatross-root-workspace"), &work_dir)?;

    let mut filters = context.filters();
    filters.push((
        r"Using Python 3.12.\[X\] interpreter at: .*",
        "Using Python 3.12.[X] interpreter at: [PYTHON]",
    ));

    uv_snapshot!(filters, run_workspace(&context)
        .arg("--package")
        .arg("bird-feeder")
        .arg("packages/bird-feeder/check_installed_bird_feeder.py")
        .current_dir(&work_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Success

    ----- stderr -----
    Using Python 3.12.[X] interpreter at: [PYTHON]
    Creating virtualenv at: .venv
    Resolved 8 packages in [TIME]
    Prepared 5 packages in [TIME]
    Installed 5 packages in [TIME]
     + anyio==4.3.0
     + bird-feeder==1.0.0 (from file://[TEMP_DIR]/albatross-root-workspace/packages/bird-feeder)
     + idna==3.6
     + seeds==1.0.0 (from file://[TEMP_DIR]/albatross-root-workspace/packages/seeds)
     + sniffio==1.3.1
    "###
    );

    uv_snapshot!(context.filters(), universal_windows_filters=true, run_workspace(&context)
            .arg("--package")
            .arg("albatross")
            .arg("check_installed_albatross.py")
            .current_dir(&work_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Success

    ----- stderr -----
    Resolved 8 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + albatross==0.1.0 (from file://[TEMP_DIR]/albatross-root-workspace)
     + tqdm==4.66.2
    "###
    );

    Ok(())
}

/// Check that the resolution is the same no matter where in the workspace we are.
fn workspace_lock_idempotence(workspace: &str, subdirectories: &[&str]) -> Result<()> {
    let mut shared_lock = None;

    for dir in subdirectories {
        let context = TestContext::new("3.12");
        let work_dir = context.temp_dir.join(workspace);

        copy_dir_ignore(workspaces_dir().join(workspace), &work_dir)?;

        lock_workspace(&context)
            .current_dir(&work_dir.join(dir))
            .assert()
            .success();

        let lock = fs_err::read_to_string(work_dir.join("uv.lock"))?;
        // Check the lockfile is the same for all resolutions.
        if let Some(shared_lock) = &shared_lock {
            assert_eq!(shared_lock, &lock);
        } else {
            shared_lock = Some(lock);
        }
    }
    Ok(())
}

/// Check that the resolution is the same no matter where in the workspace we are.
#[test]
fn workspace_lock_idempotence_root_workspace() -> Result<()> {
    workspace_lock_idempotence(
        "albatross-root-workspace",
        &[".", "packages/bird-feeder", "packages/seeds"],
    )?;
    Ok(())
}

/// Check that the resolution is the same no matter where in the workspace we are, and that locking
/// works even if there is no root project.
#[test]
fn workspace_lock_idempotence_virtual_workspace() -> Result<()> {
    workspace_lock_idempotence(
        "albatross-virtual-workspace",
        &[
            ".",
            "packages/albatross",
            "packages/bird-feeder",
            "packages/seeds",
        ],
    )?;
    Ok(())
}
