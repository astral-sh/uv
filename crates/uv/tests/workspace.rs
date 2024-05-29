use std::env;
use std::path::PathBuf;

use crate::common::{get_bin, uv_snapshot, TestContext, EXCLUDE_NEWER};

mod common;

/// A `pip install` command for workspaces.
///
/// The goal of the workspace tests is to resolve local workspace packages correctly. We add some
/// non-workspace dependencies to ensure that transitive non-workspace dependencies are also
/// correctly resolved.
pub fn install_workspace(context: &TestContext) -> std::process::Command {
    let mut command = std::process::Command::new(get_bin());
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
    Downloaded 4 packages in [TIME]
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
    Downloaded 2 packages in [TIME]
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
    Downloaded 2 packages in [TIME]
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
    Downloaded 4 packages in [TIME]
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
    Downloaded 7 packages in [TIME]
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
    Downloaded 5 packages in [TIME]
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
    Downloaded 5 packages in [TIME]
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
    Downloaded 5 packages in [TIME]
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
