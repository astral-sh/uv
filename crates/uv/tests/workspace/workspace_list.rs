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
    error: No `pyproject.toml` found in current directory or any parent directory
    "
    );
}

#[test]
fn workspace_list_depends_on() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [tool.uv.workspace]
        members = ["packages/*"]
        "#,
    )?;

    let package_a = context.temp_dir.child("packages/package-a");
    package_a.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "package-a"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["package-b"]

        [tool.uv.sources]
        package-b = { workspace = true }
        "#,
    )?;

    let package_b = context.temp_dir.child("packages/package-b");
    package_b.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "package-b"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["package-x"]

        [tool.uv.sources]
        package-x = { workspace = true }
        "#,
    )?;

    let package_c = context.temp_dir.child("packages/package-c");
    package_c.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "package-c"
        version = "0.1.0"
        requires-python = ">=3.12"
        "#,
    )?;

    let package_x = context.temp_dir.child("packages/package-x");
    package_x.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "package-x"
        version = "0.1.0"
        requires-python = ">=3.12"
        "#,
    )?;

    // The query is deliberately lock-backed and does not mutate the project.
    uv_snapshot!(context.filters(), context.workspace_list()
        .arg("--depends-on")
        .arg("package-x"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No `[TEMP_DIR]/uv.lock` found; run `uv lock` to create it
    ");

    context.lock().assert().success();

    // Both direct and transitive dependents are included; the target itself and unrelated members
    // are not.
    uv_snapshot!(context.filters(), context.workspace_list()
        .arg("--depends-on")
        .arg("package-x"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    package-a
    package-b

    ----- stderr -----
    ");

    uv_snapshot!(context.filters(), context.workspace_list()
        .arg("--depends-on")
        .arg("package-x")
        .arg("--paths"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/packages/package-a
    [TEMP_DIR]/packages/package-b

    ----- stderr -----
    ");

    // A known target with no dependents produces an empty successful result.
    uv_snapshot!(context.filters(), context.workspace_list()
        .arg("--depends-on")
        .arg("package-c"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    ");

    // Unknown package names fail instead of silently producing an empty CI matrix.
    uv_snapshot!(context.filters(), context.workspace_list()
        .arg("--depends-on")
        .arg("missing"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Package `missing` was not found in `[TEMP_DIR]/uv.lock`
    ");

    Ok(())
}

#[test]
fn workspace_list_depends_on_preserves_context() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [tool.uv.workspace]
        members = ["packages/*"]
        "#,
    )?;

    let package_p = context.temp_dir.child("packages/package-p");
    package_p.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "package-p"
        version = "0.1.0"
        requires-python = ">=3.12"

        [project.optional-dependencies]
        feature = ["package-x; sys_platform == 'linux'"]

        [tool.uv.sources]
        package-x = { workspace = true }
        "#,
    )?;

    let package_a = context.temp_dir.child("packages/package-a");
    package_a.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "package-a"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["package-p[feature]; sys_platform == 'linux'"]

        [tool.uv.sources]
        package-p = { workspace = true }
        "#,
    )?;

    let package_b = context.temp_dir.child("packages/package-b");
    package_b.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "package-b"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["package-p"]

        [tool.uv.sources]
        package-p = { workspace = true }
        "#,
    )?;

    let package_c = context.temp_dir.child("packages/package-c");
    package_c.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "package-c"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["package-p[feature]; sys_platform == 'win32'"]

        [tool.uv.sources]
        package-p = { workspace = true }
        "#,
    )?;

    let package_x = context.temp_dir.child("packages/package-x");
    package_x.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "package-x"
        version = "0.1.0"
        requires-python = ">=3.12"
        "#,
    )?;

    let package_g = context.temp_dir.child("packages/package-g");
    package_g.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "package-g"
        version = "0.1.0"
        requires-python = ">=3.12"

        [dependency-groups]
        dev = ["package-y"]

        [tool.uv.sources]
        package-y = { workspace = true }
        "#,
    )?;

    let package_h = context.temp_dir.child("packages/package-h");
    package_h.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "package-h"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["package-g"]

        [tool.uv.sources]
        package-g = { workspace = true }
        "#,
    )?;

    let package_y = context.temp_dir.child("packages/package-y");
    package_y.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "package-y"
        version = "0.1.0"
        requires-python = ">=3.12"
        "#,
    )?;

    context.lock().assert().success();

    // Optional dependencies only propagate to consumers that activate the extra under a
    // compatible marker. The package that declares the extra is itself a possible dependent.
    uv_snapshot!(context.filters(), context.workspace_list()
        .arg("--depends-on")
        .arg("package-x"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    package-a
    package-p

    ----- stderr -----
    ");

    // A member's dependency groups do not become dependencies of packages that consume it.
    uv_snapshot!(context.filters(), context.workspace_list()
        .arg("--depends-on")
        .arg("package-y"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    package-g

    ----- stderr -----
    ");

    Ok(())
}
