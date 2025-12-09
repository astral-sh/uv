use anyhow::Result;
use assert_cmd::assert::OutputAssertExt;
use assert_fs::fixture::PathChild;

use crate::common::{TestContext, copy_dir_ignore, uv_snapshot};

/// Test basic list output for a simple workspace with one member.
#[test]
fn workspace_list_simple() {
    let context = TestContext::new("3.12");

    // Initialize a workspace with one member
    context.init().arg("foo").assert().success();

    let workspace = context.temp_dir.child("foo");

    uv_snapshot!(context.filters(), context.workspace_list().current_dir(&workspace), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    foo

    ----- stderr -----
    warning: The `uv workspace list` command is experimental and may change without warning. Pass `--preview-features workspace-list` to disable this warning.
    "
    );

    uv_snapshot!(context.filters(), context.workspace_list().arg("--paths").current_dir(&workspace), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/foo

    ----- stderr -----
    warning: The `uv workspace list` command is experimental and may change without warning. Pass `--preview-features workspace-list` to disable this warning.
    "
    );
}

/// Test list output for a root workspace (workspace with a root package).
#[test]
fn workspace_list_root_workspace() -> Result<()> {
    let context = TestContext::new("3.12");
    let workspace = context.temp_dir.child("workspace");

    copy_dir_ignore(
        context
            .workspace_root
            .join("test/workspaces/albatross-root-workspace"),
        &workspace,
    )?;

    uv_snapshot!(context.filters(), context.workspace_list().current_dir(&workspace), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    albatross
    bird-feeder
    seeds

    ----- stderr -----
    warning: The `uv workspace list` command is experimental and may change without warning. Pass `--preview-features workspace-list` to disable this warning.
    "
    );

    Ok(())
}

/// Test list output for a virtual workspace (no root package).
#[test]
fn workspace_list_virtual_workspace() -> Result<()> {
    let context = TestContext::new("3.12");
    let workspace = context.temp_dir.child("workspace");

    copy_dir_ignore(
        context
            .workspace_root
            .join("test/workspaces/albatross-virtual-workspace"),
        &workspace,
    )?;

    uv_snapshot!(context.filters(), context.workspace_list().current_dir(&workspace), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    albatross
    bird-feeder
    seeds

    ----- stderr -----
    warning: The `uv workspace list` command is experimental and may change without warning. Pass `--preview-features workspace-list` to disable this warning.
    "
    );

    Ok(())
}

/// Test list output when run from a workspace member directory.
#[test]
fn workspace_list_from_member() -> Result<()> {
    let context = TestContext::new("3.12");
    let workspace = context.temp_dir.child("workspace");

    copy_dir_ignore(
        context
            .workspace_root
            .join("test/workspaces/albatross-root-workspace"),
        &workspace,
    )?;

    let member_dir = workspace.join("packages").join("bird-feeder");

    uv_snapshot!(context.filters(), context.workspace_list().current_dir(&member_dir), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    albatross
    bird-feeder
    seeds

    ----- stderr -----
    warning: The `uv workspace list` command is experimental and may change without warning. Pass `--preview-features workspace-list` to disable this warning.
    "
    );

    Ok(())
}

/// Test list output for a workspace with multiple packages.
#[test]
fn workspace_list_multiple_members() {
    let context = TestContext::new("3.12");

    // Initialize workspace root
    context.init().arg("pkg-a").assert().success();

    let workspace_root = context.temp_dir.child("pkg-a");

    // Add more members
    context
        .init()
        .arg("pkg-b")
        .current_dir(&workspace_root)
        .assert()
        .success();

    context
        .init()
        .arg("pkg-c")
        .current_dir(&workspace_root)
        .assert()
        .success();

    uv_snapshot!(context.filters(), context.workspace_list().current_dir(&workspace_root), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    pkg-a
    pkg-b
    pkg-c

    ----- stderr -----
    warning: The `uv workspace list` command is experimental and may change without warning. Pass `--preview-features workspace-list` to disable this warning.
    "
    );

    uv_snapshot!(context.filters(), context.workspace_list().arg("--paths").current_dir(&workspace_root), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/pkg-a
    [TEMP_DIR]/pkg-a/pkg-b
    [TEMP_DIR]/pkg-a/pkg-c

    ----- stderr -----
    warning: The `uv workspace list` command is experimental and may change without warning. Pass `--preview-features workspace-list` to disable this warning.
    "
    );
}

/// Test list output for a single project (not a workspace).
#[test]
fn workspace_list_single_project() {
    let context = TestContext::new("3.12");

    context.init().arg("my-project").assert().success();

    let project = context.temp_dir.child("my-project");

    uv_snapshot!(context.filters(), context.workspace_list().current_dir(&project), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    my-project

    ----- stderr -----
    warning: The `uv workspace list` command is experimental and may change without warning. Pass `--preview-features workspace-list` to disable this warning.
    "
    );
}

/// Test list output with excluded packages.
#[test]
fn workspace_list_with_excluded() -> Result<()> {
    let context = TestContext::new("3.12");
    let workspace = context.temp_dir.child("workspace");

    copy_dir_ignore(
        context
            .workspace_root
            .join("test/workspaces/albatross-project-in-excluded"),
        &workspace,
    )?;

    uv_snapshot!(context.filters(), context.workspace_list().current_dir(&workspace), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    albatross

    ----- stderr -----
    warning: The `uv workspace list` command is experimental and may change without warning. Pass `--preview-features workspace-list` to disable this warning.
    "
    );

    Ok(())
}

/// Test list error output when not in a project.
#[test]
fn workspace_list_no_project() {
    let context = TestContext::new("3.12");

    uv_snapshot!(context.filters(), context.workspace_list(), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: The `uv workspace list` command is experimental and may change without warning. Pass `--preview-features workspace-list` to disable this warning.
    error: No `pyproject.toml` found in current directory or any parent directory
    "
    );
}
