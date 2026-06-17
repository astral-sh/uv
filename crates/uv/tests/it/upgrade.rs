use anyhow::{Result, anyhow};
use assert_fs::prelude::*;
use insta::allow_duplicates;
use url::Url;

use uv_static::EnvVars;
use uv_test::packse::PackseServer;
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

/// Return snapshot filters for metadata fetched from a [`PackseServer`].
fn packse_filters(context: &TestContext) -> Vec<(&str, &str)> {
    let mut filters = context.filters();
    filters.push((r"(?m)^WARN Range requests not supported[^\n]*\n", ""));
    filters
}

/// Write a project where `foo==1` resolves two versions of `bar` in platform forks.
fn write_fork_upgrade_project(
    context: &TestContext,
    server: &PackseServer,
    bar_requirement: &str,
) -> Result<String> {
    let pyproject_toml = format!(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["{bar_requirement}", "foo==1"]

        [[tool.uv.index]]
        url = "{}"
        default = true
    "#,
        server.index_url()
    );
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(&pyproject_toml)?;
    fs_err::remove_dir_all(&context.venv)?;
    Ok(pyproject_toml)
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
                                   directory for the duration of the operation
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
#[cfg(feature = "test-pypi")]
fn upgrade_selects_normalized_production_dependency() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let pyproject_toml = r#"
        [project]
        name = "example"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["AnyIO>=2,<3,!=2.1 ; python_version >= '3.12'"]

        [tool.uv]
        exclude-newer = "2024-03-25T00:00:00Z"
    "#;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(pyproject_toml)?;
    fs_err::remove_dir_all(&context.venv)?;

    uv_snapshot!(
        context.filters(),
        context.upgrade().arg("anyio"),
        @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 4 packages in [TIME]
    Add anyio v4.3.0
    Updated requirement: `AnyIO>=2,<3,!=2.1 ; python_version >= '3.12'` -> `anyio>=2,!=2.1,<5 ; python_full_version >= '3.12'`
    "
    );

    insta::assert_snapshot!(
        fs_err::read_to_string(context.temp_dir.child("pyproject.toml"))?,
        @r#"
[project]
name = "example"
version = "0.1.0"
requires-python = ">=3.12"
dependencies = ["anyio>=2,!=2.1,<5 ; python_full_version >= '3.12'"]

[tool.uv]
exclude-newer = "2024-03-25T00:00:00Z"
"#
    );
    assert!(!context.temp_dir.child("uv.lock").exists());
    assert!(!context.temp_dir.child(".venv").exists());
    Ok(())
}

#[test]
fn upgrade_ignores_disjoint_fork_version_for_selected_requirement() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/fork-upgrade.toml");
    let pyproject_toml =
        write_fork_upgrade_project(&context, &server, "bar==2 ; sys_platform != 'linux'")?;

    uv_snapshot!(
        packse_filters(&context),
        context
            .upgrade()
            .arg("bar")
            .env_remove(EnvVars::UV_EXCLUDE_NEWER),
        @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 4 packages in [TIME]
    Add bar v1.0.0, v2.0.0
    "
    );

    assert_project_unchanged(&context, &pyproject_toml)
}

#[test]
fn upgrade_preserves_constraint_that_admits_multiple_fork_versions() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/fork-upgrade.toml");
    let pyproject_toml = write_fork_upgrade_project(&context, &server, "bar>=1")?;

    uv_snapshot!(
        packse_filters(&context),
        context
            .upgrade()
            .arg("bar")
            .env_remove(EnvVars::UV_EXCLUDE_NEWER),
        @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 4 packages in [TIME]
    Add bar v1.0.0, v2.0.0
    "
    );

    assert_project_unchanged(&context, &pyproject_toml)
}

#[test]
fn upgrade_preserves_inapplicable_marked_dependency() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let pyproject_toml = r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio<3 ; python_version < '3.12'"]
    "#;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(pyproject_toml)?;
    fs_err::remove_dir_all(&context.venv)?;

    uv_snapshot!(context.filters(), context.upgrade().arg("anyio"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 1 package in [TIME]
    No version change for anyio
    ");

    assert_project_unchanged(&context, pyproject_toml)
}

#[test]
fn upgrade_expands_constraint_for_multiple_fork_versions() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/fork-upgrade.toml");
    let pyproject_toml = write_fork_upgrade_project(&context, &server, "bar<2")?;

    uv_snapshot!(
        packse_filters(&context),
        context
            .upgrade()
            .arg("bar")
            .env_remove(EnvVars::UV_EXCLUDE_NEWER),
        @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 4 packages in [TIME]
    Add bar v1.0.0, v2.0.0
    Updated requirement: `bar<2` -> `bar<3`
    "
    );

    assert_eq!(
        fs_err::read_to_string(context.temp_dir.child("pyproject.toml"))?,
        pyproject_toml.replace("bar<2", "bar<3")
    );
    assert!(!context.temp_dir.child("uv.lock").exists());
    assert!(!context.temp_dir.child(".venv").exists());
    Ok(())
}

#[test]
fn upgrade_expands_compatible_constraint_for_multiple_fork_versions() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/filter-sibling-dependencies.toml");
    let pyproject_toml = format!(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["a~=3.0"]

        [tool.uv]
        constraint-dependencies = [
            "a==4.3.0 ; sys_platform == 'darwin'",
            "a==4.4.0 ; sys_platform != 'darwin'",
        ]

        [[tool.uv.index]]
        url = "{}"
        default = true
    "#,
        server.index_url()
    );
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(&pyproject_toml)?;
    fs_err::remove_dir_all(&context.venv)?;

    uv_snapshot!(
        packse_filters(&context),
        context
            .upgrade()
            .arg("a")
            .env_remove(EnvVars::UV_EXCLUDE_NEWER),
        @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 3 packages in [TIME]
    Add a v4.3.0, v4.4.0
    Updated requirement: `a~=3.0` -> `a~=4.3`
    "
    );

    assert_eq!(
        fs_err::read_to_string(context.temp_dir.child("pyproject.toml"))?,
        pyproject_toml.replace("a~=3.0", "a~=4.3")
    );
    assert!(!context.temp_dir.child("uv.lock").exists());
    assert!(!context.temp_dir.child(".venv").exists());
    Ok(())
}

#[test]
#[cfg(feature = "test-pypi")]
fn upgrade_updates_requirement_without_updating_lockfile_or_environment() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let initial_pyproject_toml = r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio<=2", "idna"]

        [tool.uv]
        exclude-newer = "2021-01-01T00:00:00Z"
    "#;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(initial_pyproject_toml)?;

    uv_snapshot!(
        context.filters(),
        context.lock().env_remove(EnvVars::UV_EXCLUDE_NEWER),
        @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    ");

    let pyproject_toml =
        initial_pyproject_toml.replace("2021-01-01T00:00:00Z", "2024-03-25T00:00:00Z");
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(&pyproject_toml)?;
    let environment_sentinel = context.venv.child("sentinel");
    environment_sentinel.write_str("present")?;

    let lock = fs_err::read(context.temp_dir.child("uv.lock"))?;

    uv_snapshot!(
        context.filters(),
        context
            .upgrade()
            .arg("anyio")
            .env_remove(EnvVars::UV_EXCLUDE_NEWER),
        @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolving despite existing lockfile due to change of exclude newer timestamp from `2021-01-01T00:00:00Z` to `2024-03-25T00:00:00Z`
    Resolved 4 packages in [TIME]
    Update anyio v2.0.0 -> v4.3.0
    Updated requirement: `anyio<=2` -> `anyio<=4.3.0`
    ");

    assert_eq!(
        fs_err::read_to_string(context.temp_dir.child("pyproject.toml"))?,
        pyproject_toml.replace("anyio<=2", "anyio<=4.3.0")
    );
    assert_eq!(fs_err::read(context.temp_dir.child("uv.lock"))?, lock);
    let lock_contents = context.read("uv.lock");
    assert!(
        lock_contents.contains("name = \"idna\"\nversion = \"2.10\""),
        "{lock_contents}"
    );
    assert!(environment_sentinel.exists());
    Ok(())
}

#[test]
#[cfg(feature = "test-pypi")]
fn upgrade_reports_no_solution_without_mutation() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let pyproject_toml = r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio<=2", "idna==9999"]

        [tool.uv]
        exclude-newer = "2024-03-25T00:00:00Z"
    "#;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(pyproject_toml)?;
    fs_err::remove_dir_all(&context.venv)?;

    uv_snapshot!(context.filters(), context.upgrade().arg("anyio"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
      × No solution found when resolving dependencies:
      ╰─▶ Because there is no version of idna==9999 and your project depends on idna==9999, we can conclude that your project's requirements are unsatisfiable.
    ");

    assert_project_unchanged(&context, pyproject_toml)
}

#[test]
#[cfg(feature = "test-pypi")]
fn upgrade_reports_no_version_change_without_mutation() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let pyproject_toml = r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio"]

        [tool.uv]
        exclude-newer = "2024-03-25T00:00:00Z"
    "#;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(pyproject_toml)?;

    uv_snapshot!(context.filters(), context.lock(), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    ");
    fs_err::remove_dir_all(&context.venv)?;

    let pyproject = fs_err::read(context.temp_dir.child("pyproject.toml"))?;
    let lock = fs_err::read(context.temp_dir.child("uv.lock"))?;

    uv_snapshot!(context.filters(), context.upgrade().arg("anyio"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 4 packages in [TIME]
    No version change for anyio
    ");

    assert_eq!(
        fs_err::read(context.temp_dir.child("pyproject.toml"))?,
        pyproject
    );
    assert_eq!(fs_err::read(context.temp_dir.child("uv.lock"))?, lock);
    assert!(!context.temp_dir.child(".venv").exists());
    Ok(())
}

#[test]
fn upgrade_rejects_dynamic_project_version() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let pyproject_toml = r#"
        [project]
        name = "project"
        dynamic = ["version"]
        requires-python = ">=3.12"
        dependencies = ["anyio<=2"]
    "#;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(pyproject_toml)?;
    fs_err::remove_dir_all(&context.venv)?;

    uv_snapshot!(context.filters(), context.upgrade().arg("anyio"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: `uv upgrade` does not support projects with dynamic versions yet
    ");

    assert_project_unchanged(&context, pyproject_toml)
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
fn upgrade_rejects_duplicate_marked_production_dependencies() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);
    let pyproject_toml = r#"
        [project]
        name = "example"
        version = "0.1.0"
        dependencies = [
            "Requests>=2 ; sys_platform == 'darwin'",
            "requests<3 ; sys_platform != 'darwin'",
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
fn upgrade_rejects_self_dependency() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);
    let pyproject_toml = r#"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = ["project[foo]>=0.1"]
    "#;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(pyproject_toml)?;

    uv_snapshot!(context.filters(), context.upgrade().arg("project"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Dependency `project` refers to the current project and cannot be upgraded
    ");

    assert_project_unchanged(&context, pyproject_toml)
}

#[test]
fn upgrade_rejects_git_revision() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);
    let pyproject_toml = r#"
        [project]
        name = "example"
        version = "0.1.0"
        dependencies = ["requests>=2"]

        [tool.uv.sources]
        requests = { git = "https://github.com/psf/requests", rev = "6f205ff422bccd5e4c4fc0b64c5f3e7df5181db6" }
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
    error: Dependency `requests` is pinned to a Git revision and cannot be upgraded commit-to-commit
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
#[cfg(feature = "test-pypi")]
fn upgrade_allows_registry_source() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let empty_index = context.temp_dir.child("empty-index");
    empty_index.create_dir_all()?;
    let empty_index = Url::from_directory_path(empty_index.path())
        .map_err(|()| anyhow!("Failed to create empty index URL"))?;
    let pyproject_toml = format!(
        r#"
        [project]
        name = "example"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["idna>=2,<3"]

        [tool.uv]
        exclude-newer = "2024-03-25T00:00:00Z"

        [tool.uv.sources]
        idna = {{ index = "pypi" }}

        [[tool.uv.index]]
        name = "pypi"
        url = "https://pypi.org/simple"
        explicit = true

        [[tool.uv.index]]
        name = "empty"
        url = "{empty_index}"
        default = true
    "#
    );
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(&pyproject_toml)?;
    fs_err::remove_dir_all(&context.venv)?;

    uv_snapshot!(
        context.filters(),
        context.upgrade().arg("idna"),
        @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 2 packages in [TIME]
    Add idna v3.6
    Updated requirement: `idna>=2,<3` -> `idna>=2,<4`
    "
    );

    assert_eq!(
        fs_err::read_to_string(context.temp_dir.child("pyproject.toml"))?,
        pyproject_toml.replace("idna>=2,<3", "idna>=2,<4")
    );
    assert!(!context.temp_dir.child("uv.lock").exists());
    assert!(!context.temp_dir.child(".venv").exists());
    Ok(())
}

#[test]
#[cfg(feature = "test-pypi")]
fn upgrade_ignores_inapplicable_non_registry_source() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let pyproject_toml = r#"
        [project]
        name = "example"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio>=2,<3 ; python_version >= '3.12'"]

        [tool.uv]
        exclude-newer = "2024-03-25T00:00:00Z"

        [tool.uv.sources]
        anyio = { git = "https://github.com/agronholm/anyio", marker = "python_version < '3.12'" }
    "#;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(pyproject_toml)?;
    fs_err::remove_dir_all(&context.venv)?;

    uv_snapshot!(
        context.filters(),
        context.upgrade().arg("anyio"),
        @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 4 packages in [TIME]
    Add anyio v4.3.0
    Updated requirement: `anyio>=2,<3 ; python_version >= '3.12'` -> `anyio>=2,<5 ; python_full_version >= '3.12'`
    "
    );

    assert_eq!(
        fs_err::read_to_string(context.temp_dir.child("pyproject.toml"))?,
        pyproject_toml.replace(
            "anyio>=2,<3 ; python_version >= '3.12'",
            "anyio>=2,<5 ; python_full_version >= '3.12'"
        )
    );
    assert!(!context.temp_dir.child("uv.lock").exists());
    assert!(!context.temp_dir.child(".venv").exists());
    Ok(())
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
#[cfg(feature = "test-pypi")]
fn upgrade_updates_nested_workspace_member_only() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let workspace_pyproject_toml = r#"
        [tool.uv.workspace]
        members = ["project"]
    "#;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(workspace_pyproject_toml)?;

    let project = context.temp_dir.child("project");
    project.create_dir_all()?;
    let project_pyproject_toml = r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio<=2"]

        [tool.uv]
        exclude-newer = "2024-03-25T00:00:00Z"
    "#;
    project
        .child("pyproject.toml")
        .write_str(project_pyproject_toml)?;
    fs_err::remove_dir_all(&context.venv)?;

    uv_snapshot!(
        context.filters(),
        context.upgrade().current_dir(&project).arg("anyio"),
        @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 4 packages in [TIME]
    Add anyio v4.3.0
    Updated requirement: `anyio<=2` -> `anyio<=4.3.0`
    "
    );

    assert_eq!(
        fs_err::read_to_string(context.temp_dir.child("pyproject.toml"))?,
        workspace_pyproject_toml
    );
    assert_eq!(
        fs_err::read_to_string(project.child("pyproject.toml"))?,
        project_pyproject_toml.replace("anyio<=2", "anyio<=4.3.0")
    );
    assert!(!context.temp_dir.child("uv.lock").exists());
    assert!(!context.temp_dir.child(".venv").exists());
    assert!(!project.child("uv.lock").exists());
    assert!(!project.child(".venv").exists());
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
