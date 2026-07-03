use anyhow::Result;
use assert_cmd::assert::OutputAssertExt;
use assert_fs::fixture::{FileWriteStr, PathChild};

use uv_test::{copy_dir_ignore, uv_snapshot};

/// Test basic list output for a simple workspace with one member.
#[test]
fn workspace_list_simple() {
    let context = uv_test::test_context!("3.12");

    // Initialize a workspace with one member
    context.init().arg("foo").assert().success();

    let workspace = context.temp_dir.child("foo");

    uv_snapshot!(context.filters(), context.workspace_list().current_dir(&workspace), @"
    success: true
    exit_code: 0
    ----- stdout -----
    foo

    ----- stderr -----
    "
    );

    uv_snapshot!(context.filters(), context.workspace_list().arg("--paths").current_dir(&workspace), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/foo

    ----- stderr -----
    "
    );
}

/// Test list output for a root workspace (workspace with a root package).
#[test]
fn workspace_list_root_workspace() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let workspace = context.temp_dir.child("workspace");

    copy_dir_ignore(
        context
            .workspace_root
            .join("test/workspaces/albatross-root-workspace"),
        &workspace,
    )?;

    uv_snapshot!(context.filters(), context.workspace_list().current_dir(&workspace), @"
    success: true
    exit_code: 0
    ----- stdout -----
    albatross
    bird-feeder
    seeds

    ----- stderr -----
    "
    );

    Ok(())
}

/// Test list output for a virtual workspace (no root package).
#[test]
fn workspace_list_virtual_workspace() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let workspace = context.temp_dir.child("workspace");

    copy_dir_ignore(
        context
            .workspace_root
            .join("test/workspaces/albatross-virtual-workspace"),
        &workspace,
    )?;

    uv_snapshot!(context.filters(), context.workspace_list().current_dir(&workspace), @"
    success: true
    exit_code: 0
    ----- stdout -----
    albatross
    bird-feeder
    seeds

    ----- stderr -----
    "
    );

    Ok(())
}

/// Test list output when run from a workspace member directory.
#[test]
fn workspace_list_from_member() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let workspace = context.temp_dir.child("workspace");

    copy_dir_ignore(
        context
            .workspace_root
            .join("test/workspaces/albatross-root-workspace"),
        &workspace,
    )?;

    let member_dir = workspace.join("packages").join("bird-feeder");

    uv_snapshot!(context.filters(), context.workspace_list().current_dir(&member_dir), @"
    success: true
    exit_code: 0
    ----- stdout -----
    albatross
    bird-feeder
    seeds

    ----- stderr -----
    "
    );

    Ok(())
}

/// Test list output for a workspace with multiple packages.
#[test]
fn workspace_list_multiple_members() {
    let context = uv_test::test_context!("3.12");

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

    uv_snapshot!(context.filters(), context.workspace_list().current_dir(&workspace_root), @"
    success: true
    exit_code: 0
    ----- stdout -----
    pkg-a
    pkg-b
    pkg-c

    ----- stderr -----
    "
    );

    uv_snapshot!(context.filters(), context.workspace_list().arg("--paths").current_dir(&workspace_root), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/pkg-a
    [TEMP_DIR]/pkg-a/pkg-b
    [TEMP_DIR]/pkg-a/pkg-c

    ----- stderr -----
    "
    );
}

/// Test list output for a single project (not a workspace).
#[test]
fn workspace_list_single_project() {
    let context = uv_test::test_context!("3.12");

    context.init().arg("my-project").assert().success();

    let project = context.temp_dir.child("my-project");

    uv_snapshot!(context.filters(), context.workspace_list().current_dir(&project), @"
    success: true
    exit_code: 0
    ----- stdout -----
    my-project

    ----- stderr -----
    "
    );
}

/// Test list output with excluded packages.
#[test]
fn workspace_list_with_excluded() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let workspace = context.temp_dir.child("workspace");

    copy_dir_ignore(
        context
            .workspace_root
            .join("test/workspaces/albatross-project-in-excluded"),
        &workspace,
    )?;

    uv_snapshot!(context.filters(), context.workspace_list().current_dir(&workspace), @"
    success: true
    exit_code: 0
    ----- stdout -----
    albatross

    ----- stderr -----
    "
    );

    Ok(())
}

/// Test list error output when not in a project.
#[test]
fn workspace_list_no_project() {
    let context = uv_test::test_context!("3.12");

    uv_snapshot!(context.filters(), context.workspace_list(), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No `pyproject.toml` found in current directory or any parent directory. To create one, run `uv init` or see https://docs.astral.sh/uv/guides/projects/
    "
    );
}

/// Test recursively listing PEP 723 scripts while respecting ignore rules and environment
/// boundaries.
#[test]
fn workspace_list_scripts() -> Result<()> {
    let mut context = uv_test::test_context!("3.12");

    context.init().arg("project").assert().success();
    let project = context.temp_dir.child("project");

    let script = r"# /// script
# dependencies = []
# ///
";

    project.child("script.py").write_str(script)?;
    project.child("scripts/nested.py").write_str(script)?;
    project.child(".github/hidden.py").write_str(script)?;

    // Extensionless scripts are not discovered.
    project.child("tool").write_str(script)?;

    // PEP 723 examples in documentation are not Python scripts.
    project
        .child("docs/example.md")
        .write_str(&format!("# Example\n\n{script}\n{script}"))?;

    // Regular Python modules are not scripts.
    project
        .child("src/project/module.py")
        .write_str("VALUE = 1\n")?;

    project.child(".gitignore").write_str("ignored/\n")?;
    project.child("ignored/script.py").write_str(script)?;
    project
        .child(".ignore")
        .write_str("ignored-by-dot-ignore.py\n")?;
    project
        .child("ignored-by-dot-ignore.py")
        .write_str(script)?;

    project.child(".git/script.py").write_str(script)?;
    project.child(".venv/script.py").write_str(script)?;
    project.child("environment/pyvenv.cfg").write_str("")?;
    project.child("environment/script.py").write_str(script)?;

    uv_snapshot!(context.filters(), context.workspace_list()
        .arg("--scripts")
        .current_dir(&project), @"
    success: true
    exit_code: 0
    ----- stdout -----
    .github/hidden.py
    script.py
    scripts/nested.py

    ----- stderr -----
    warning: The `--scripts` option is experimental and may change without warning. Pass `--preview-features workspace-list-scripts` to disable this warning.
    ");

    uv_snapshot!(context.filters(), context.workspace_list()
        .arg("--scripts")
        .arg("--preview-features")
        .arg("workspace-list-scripts")
        .current_dir(&project), @"
    success: true
    exit_code: 0
    ----- stdout -----
    .github/hidden.py
    script.py
    scripts/nested.py

    ----- stderr -----
    ");

    uv_snapshot!(context.filters(), context.workspace_list()
        .arg("--scripts")
        .arg("--paths")
        .current_dir(&project), @"
    success: true
    exit_code: 0
    ----- stdout -----
    .github/hidden.py
    script.py
    scripts/nested.py

    ----- stderr -----
    warning: The `--scripts` option is experimental and may change without warning. Pass `--preview-features workspace-list-scripts` to disable this warning.
    ");

    // The configured cache is excluded even when it has not been initialized with ignore files.
    let cache = project.child("cache");
    cache.child("script.py").write_str(script)?;
    context.cache_dir = cache;
    uv_snapshot!(context.filters(), context.workspace_list()
        .arg("--scripts")
        .current_dir(&project), @"
    success: true
    exit_code: 0
    ----- stdout -----
    .github/hidden.py
    script.py
    scripts/nested.py

    ----- stderr -----
    warning: The `--scripts` option is experimental and may change without warning. Pass `--preview-features workspace-list-scripts` to disable this warning.
    ");

    Ok(())
}
