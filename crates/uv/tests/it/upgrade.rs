use anyhow::Result;
use assert_fs::prelude::*;
use insta::allow_duplicates;

use uv_test::{TestContext, uv_snapshot};

fn assert_project_unchanged(context: &TestContext, expected: &str) -> Result<()> {
    assert_eq!(
        fs_err::read_to_string(context.temp_dir.child("pyproject.toml"))?,
        expected
    );
    assert!(!context.temp_dir.child("uv.lock").exists());
    assert!(!context.temp_dir.child(".venv").exists());
    Ok(())
}

#[test]
fn upgrade_help() {
    let context = uv_test::test_context_with_versions!(&[]);

    uv_snapshot!(
        context.filters(),
        context.upgrade().arg("--help"),
        @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    Upgrade a dependency in the project

    Usage: uv upgrade [OPTIONS] <PACKAGE>

    Arguments:
      <PACKAGE>  The package to upgrade

    Cache options:
      -n, --no-cache               Avoid reading from or writing to the cache, instead using a temporary
                                   directory for the duration of the operation [env: UV_NO_CACHE=]
          --cache-dir [CACHE_DIR]  Path to the cache directory [env: UV_CACHE_DIR=]

    Python options:
          --managed-python       Require use of uv-managed Python versions [env: UV_MANAGED_PYTHON=]
          --no-managed-python    Disable use of uv-managed Python versions [env: UV_NO_MANAGED_PYTHON=]
          --no-python-downloads  Disable automatic downloads of Python. [env:
                                 "UV_PYTHON_DOWNLOADS=never"]

    Global options:
      -q, --quiet...
              Use quiet output
      -v, --verbose...
              Use verbose output
          --color <COLOR_CHOICE>
              Control the use of color in output [possible values: auto, always, never]
          --system-certs
              Whether to load TLS certificates from the platform's native certificate store [env:
              UV_SYSTEM_CERTS=]
          --offline
              Disable network access [env: UV_OFFLINE=]
          --allow-insecure-host <ALLOW_INSECURE_HOST>
              Allow insecure connections to a host [env: UV_INSECURE_HOST=]
          --no-progress
              Hide all progress outputs [env: UV_NO_PROGRESS=]
          --directory <DIRECTORY>
              Change to the given directory prior to running the command [env: UV_WORKING_DIR=]
          --project <PROJECT>
              Discover a project in the given directory [env: UV_PROJECT=]
          --config-file <CONFIG_FILE>
              The path to a `uv.toml` file to use for configuration [env: UV_CONFIG_FILE=]
          --no-config
              Avoid discovering configuration files (`pyproject.toml`, `uv.toml`) [env: UV_NO_CONFIG=]
      -h, --help
              Display the concise help for this command

    ----- stderr -----
    "#
    );
}

#[test]
fn upgrade_requires_production_dependency() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);
    let pyproject_toml = r#"
        [project]
        name = "example"
        version = "0.1.0"
        dependencies = ["anyio"]

        [project.optional-dependencies]
        test = ["requests"]

        [dependency-groups]
        dev = ["httpx"]
    "#;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(pyproject_toml)?;

    uv_snapshot!(
        context.filters(),
        context.upgrade().arg("requests"),
        @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Dependency `requests` was not found in `project.dependencies`
    "
    );

    uv_snapshot!(
        context.filters(),
        context.upgrade().arg("httpx"),
        @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Dependency `httpx` was not found in `project.dependencies`
    "
    );

    assert_project_unchanged(&context, pyproject_toml)
}

#[test]
fn upgrade_rejects_duplicate_production_dependencies() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);
    let pyproject_toml = r#"
        [project]
        name = "example"
        version = "0.1.0"
        dependencies = [
            "Requests>=2",
            "requests<3",
        ]
    "#;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(pyproject_toml)?;

    uv_snapshot!(
        context.filters(),
        context.upgrade().arg("requests"),
        @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Dependency `requests` is declared multiple times in `project.dependencies`
    "
    );

    assert_project_unchanged(&context, pyproject_toml)
}

#[test]
fn upgrade_rejects_direct_url_requirement() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);
    let pyproject_toml = r#"
        [project]
        name = "example"
        version = "0.1.0"
        dependencies = [
            "requests @ https://example.com/requests-2.32.0-py3-none-any.whl",
        ]
    "#;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(pyproject_toml)?;

    uv_snapshot!(
        context.filters(),
        context.upgrade().arg("requests"),
        @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Dependency `requests` is a direct URL requirement and cannot be upgraded
    "
    );

    assert_project_unchanged(&context, pyproject_toml)
}

#[test]
fn upgrade_rejects_non_registry_sources() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);

    allow_duplicates! {
        for source in [
            r#"{ git = "https://github.com/psf/requests" }"#,
            r#"{ url = "https://example.com/requests-2.32.0-py3-none-any.whl" }"#,
            r#"{ path = "vendor/requests" }"#,
            r"{ workspace = true }",
        ] {
            let pyproject_toml = format!(
                r#"
                [project]
                name = "example"
                version = "0.1.0"
                dependencies = ["requests>=2"]

                [tool.uv.sources]
                requests = {source}
                "#
            );
            context
                .temp_dir
                .child("pyproject.toml")
                .write_str(&pyproject_toml)?;

            uv_snapshot!(
                context.filters(),
                context.upgrade().arg("requests"),
                @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Dependency `requests` uses a non-registry source in `tool.uv.sources` and cannot be upgraded
    "
            );

            assert_project_unchanged(&context, &pyproject_toml)?;
        }

        Ok::<(), anyhow::Error>(())
    }?;

    Ok(())
}

#[test]
fn upgrade_rejects_non_registry_source_for_top_level_extra() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);
    let pyproject_toml = r#"
        [project]
        name = "example"
        version = "0.1.0"
        dependencies = ["requests>=2 ; extra == 'gpu'"]

        [tool.uv.sources]
        requests = { git = "https://github.com/psf/requests", extra = "gpu" }
    "#;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(pyproject_toml)?;

    uv_snapshot!(
        context.filters(),
        context.upgrade().arg("requests"),
        @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Dependency `requests` uses a non-registry source in `tool.uv.sources` and cannot be upgraded
    "
    );

    assert_project_unchanged(&context, pyproject_toml)
}

#[test]
fn upgrade_rejects_workspace_root_non_registry_source() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);
    let workspace_pyproject_toml = r#"
        [tool.uv.workspace]
        members = ["project"]

        [tool.uv.sources]
        requests = { path = "vendor/requests" }
    "#;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(workspace_pyproject_toml)?;

    let project = context.temp_dir.child("project");
    project.create_dir_all()?;
    let project_pyproject_toml = r#"
        [project]
        name = "example"
        version = "0.1.0"
        dependencies = ["requests>=2"]
    "#;
    project
        .child("pyproject.toml")
        .write_str(project_pyproject_toml)?;

    uv_snapshot!(
        context.filters(),
        context
            .upgrade()
            .current_dir(&project)
            .arg("requests"),
        @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Dependency `requests` uses a non-registry source in `tool.uv.sources` and cannot be upgraded
    "
    );

    assert_eq!(
        fs_err::read_to_string(context.temp_dir.child("pyproject.toml"))?,
        workspace_pyproject_toml
    );
    assert_eq!(
        fs_err::read_to_string(project.child("pyproject.toml"))?,
        project_pyproject_toml
    );
    assert!(!context.temp_dir.child("uv.lock").exists());
    assert!(!context.temp_dir.child(".venv").exists());
    Ok(())
}

#[test]
fn upgrade_requires_current_project() {
    let context = uv_test::test_context_with_versions!(&[]);

    uv_snapshot!(
        context.filters(),
        context.upgrade().arg("requests"),
        @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: `uv upgrade` requires a project with a `[project]` table
    "
    );

    assert!(!context.temp_dir.child("uv.lock").exists());
    assert!(!context.temp_dir.child(".venv").exists());
}

#[test]
fn upgrade_rejects_virtual_workspace_root() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);
    let pyproject_toml = r"
        [tool.uv.workspace]
        members = []
    ";
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(pyproject_toml)?;

    uv_snapshot!(
        context.filters(),
        context.upgrade().arg("requests"),
        @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: `uv upgrade` requires a project with a `[project]` table
    "
    );

    assert_project_unchanged(&context, pyproject_toml)
}

#[test]
fn upgrade_rejects_multi_member_workspace() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);
    let pyproject_toml = r#"
        [project]
        name = "example"
        version = "0.1.0"
        dependencies = ["requests>=2"]

        [tool.uv.workspace]
        members = ["member"]
    "#;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(pyproject_toml)?;
    context.temp_dir.child("member").create_dir_all()?;
    context
        .temp_dir
        .child("member")
        .child("pyproject.toml")
        .write_str(
            r#"
            [project]
            name = "member"
            version = "0.1.0"
            "#,
        )?;

    uv_snapshot!(
        context.filters(),
        context.upgrade().arg("requests"),
        @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: `uv upgrade` does not support workspaces with multiple members yet
    "
    );

    assert_project_unchanged(&context, pyproject_toml)
}
