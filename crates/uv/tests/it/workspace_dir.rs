use anyhow::Result;
use assert_cmd::assert::OutputAssertExt;
use assert_fs::fixture::PathChild;

use uv_test::{copy_dir_ignore, uv_snapshot};

/// Test basic output for a simple workspace with one member.
#[test]
fn workspace_dir_simple() {
    let context = uv_test::test_context!("3.12");

    // Initialize a workspace with one member
    context.init().arg("foo").assert().success();

    let workspace = context.temp_dir.child("foo");

    uv_snapshot!(context.filters(), context.workspace_dir().current_dir(&workspace), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/foo

    ----- stderr -----

    "
    );
}

/// Workspace dir output when run with `--package`.
#[test]
fn workspace_dir_specific_package() {
    let context = uv_test::test_context!("3.12");
    context.init().arg("foo").assert().success();
    context.init().arg("foo/bar").assert().success();
    let workspace = context.temp_dir.child("foo");

    // root workspace
    uv_snapshot!(context.filters(), context.workspace_dir().current_dir(&workspace), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/foo

    ----- stderr -----

    "
    );

    // with --package bar
    uv_snapshot!(context.filters(), context.workspace_dir().arg("--package").arg("bar").current_dir(&workspace), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/foo/bar

    ----- stderr -----

    "
    );
}

/// Test output when run from a workspace member directory.
#[test]
fn workspace_metadata_from_member() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let workspace = context.temp_dir.child("workspace");

    let albatross_workspace = context
        .workspace_root
        .join("test/workspaces/albatross-root-workspace");

    copy_dir_ignore(albatross_workspace, &workspace)?;

    let member_dir = workspace.join("packages").join("bird-feeder");

    uv_snapshot!(context.filters(), context.workspace_dir().current_dir(&member_dir), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/workspace

    ----- stderr -----

    "
    );

    Ok(())
}

/// Test workspace dir error output for a non-existent package.
#[test]
fn workspace_dir_package_doesnt_exist() {
    let context = uv_test::test_context!("3.12");

    // Initialize a workspace with one member
    context.init().arg("foo").assert().success();

    let workspace = context.temp_dir.child("foo");

    uv_snapshot!(context.filters(), context.workspace_dir().arg("--package").arg("bar").current_dir(&workspace), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Package `bar` not found in workspace.
    "
    );
}

/// Test workspace dir error output when not in a project.
#[test]
fn workspace_metadata_no_project() {
    let context = uv_test::test_context!("3.12");

    uv_snapshot!(context.filters(), context.workspace_dir(), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No `pyproject.toml` found in current directory or any parent directory
    "
    );
}
