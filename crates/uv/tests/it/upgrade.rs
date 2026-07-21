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

    Usage: uv upgrade [OPTIONS] [PACKAGES]...

    Arguments:
      [PACKAGES]...  The packages to upgrade

    Options:
          --exclude <EXCLUDE>              Exclude the named package from upgrades
          --all-packages                   Upgrade dependencies in all workspace members
          --package <PACKAGE>              Upgrade dependencies in a specific package in the workspace
          --dry-run                        Perform a dry run, without writing the project manifest
          --output-format <OUTPUT_FORMAT>  Select the output format [default: text] [possible values:
                                           text, json]

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
#[cfg(feature = "test-pypi")]
fn upgrade_dry_run_reports_changed_requirement_without_mutation() -> Result<()> {
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
        context.upgrade().arg("--dry-run").arg("anyio"),
        @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 4 packages in [TIME]
    Add anyio v4.3.0
    Would update requirement: `AnyIO>=2,<3,!=2.1 ; python_version >= '3.12'` -> `anyio>=2,!=2.1,<5 ; python_full_version >= '3.12'`
    "
    );

    assert_project_unchanged(&context, pyproject_toml)
}

#[test]
#[cfg(feature = "test-pypi")]
fn upgrade_dry_run_outputs_json_without_mutation() -> Result<()> {
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
        context
            .upgrade()
            .arg("--dry-run")
            .arg("--output-format")
            .arg("json")
            .arg("anyio"),
        @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "schema": {
        "version": "preview"
      },
      "dry_run": true,
      "packages": [
        "anyio"
      ],
      "declarations": [
        {
          "member": "example",
          "package": "anyio",
          "dependency_type": "production",
          "location": "project.dependencies",
          "original_requirement": "AnyIO>=2,<3,!=2.1 ; python_version >= '3.12'",
          "new_requirement": "anyio>=2,!=2.1,<5 ; python_full_version >= '3.12'",
          "resolved_versions": [
            "4.3.0"
          ],
          "status": "changed"
        }
      ],
      "lock_changes": [
        {
          "action": "add",
          "package": "anyio",
          "previous_versions": [],
          "current_versions": [
            {
              "version": "4.3.0"
            }
          ]
        }
      ]
    }

    ----- stderr -----
    warning: The `--output-format json` option is experimental and the schema may change without warning. Pass `--preview-features json-output` to disable this warning.
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 4 packages in [TIME]
    "#
    );

    assert_project_unchanged(&context, pyproject_toml)
}

#[test]
#[cfg(feature = "test-pypi")]
fn upgrade_quiet_json_reports_unchanged_declaration() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let pyproject_toml = r#"
        [project]
        name = "example"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio>=2"]

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
        context
            .upgrade()
            .arg("--quiet")
            .arg("--output-format")
            .arg("json")
            .arg("anyio"),
        @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "schema": {
        "version": "preview"
      },
      "dry_run": false,
      "packages": [
        "anyio"
      ],
      "declarations": [
        {
          "member": "example",
          "package": "anyio",
          "dependency_type": "production",
          "location": "project.dependencies",
          "original_requirement": "anyio>=2",
          "resolved_versions": [
            "4.3.0"
          ],
          "status": "unchanged"
        }
      ],
      "lock_changes": [
        {
          "action": "add",
          "package": "anyio",
          "previous_versions": [],
          "current_versions": [
            {
              "version": "4.3.0"
            }
          ]
        }
      ]
    }

    ----- stderr -----
    "#
    );

    assert_project_unchanged(&context, pyproject_toml)
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
fn upgrade_skips_inapplicable_marked_dependency() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);
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
    uv_snapshot!(context.filters(), context.upgrade().arg("anyio"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: Skipping dependency `anyio` in `project.dependencies`: `anyio<3 ; python_full_version < '3.12'` (excluded by the project's environments or Python requirement)
    ");

    assert_project_unchanged(&context, pyproject_toml)
}

#[test]
fn upgrade_skips_undefined_extra() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);
    let pyproject_toml = r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio<3 ; extra == 'does-not-exist'"]

        [project.optional-dependencies]
        test = []
    "#;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(pyproject_toml)?;
    uv_snapshot!(context.filters(), context.upgrade().arg("anyio"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: Skipping dependency `anyio` in `project.dependencies`: `anyio<3 ; extra == 'does-not-exist'` (references an extra that the project does not provide)
    ");

    assert_project_unchanged(&context, pyproject_toml)
}

#[test]
fn upgrade_warns_for_skipped_requirement_before_validation_error() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);
    let pyproject_toml = r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "bar @ https://example.com/bar-1.0.0-py3-none-any.whl",
            "bar<2 ; python_version < '3.12'",
        ]
    "#;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(pyproject_toml)?;
    uv_snapshot!(context.filters(), context.upgrade().arg("bar"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: Skipping dependency `bar` in `project.dependencies`: `bar<2 ; python_full_version < '3.12'` (excluded by the project's environments or Python requirement)
    error: Dependency `bar` is a direct URL requirement and cannot be upgraded
    ");

    assert_project_unchanged(&context, pyproject_toml)
}

#[test]
fn upgrade_rejects_conflicting_extra_declarations() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/fork-upgrade.toml");
    let pyproject_toml = format!(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "bar<1 ; extra == 'cpu'",
            "bar<2 ; extra == 'gpu'",
            "foo==1",
        ]

        [project.optional-dependencies]
        cpu = []
        gpu = []

        [tool.uv]
        conflicts = [
            [
                {{ extra = "cpu" }},
                {{ extra = "gpu" }},
            ],
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
            .arg("bar")
            .env_remove(EnvVars::UV_EXCLUDE_NEWER),
        @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: Could not update dependency `bar` in `project.dependencies`: `bar<1 ; extra == 'cpu'` (declared under conflicting extra `cpu`)
    warning: Could not update dependency `bar` in `project.dependencies`: `bar<2 ; extra == 'gpu'` (declared under conflicting extra `gpu`)
    "
    );

    assert_project_unchanged(&context, &pyproject_toml)
}

#[test]
fn upgrade_rejects_conflicting_included_group_declarations() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);
    let pyproject_toml = r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [dependency-groups]
        cpu = [{ include-group = "cpu-core" }]
        gpu = [{ include-group = "gpu-core" }]
        cpu-core = ["bar<1"]
        gpu-core = ["bar<2"]

        [tool.uv]
        conflicts = [
            [
                { group = "cpu" },
                { group = "gpu" },
            ],
        ]
    "#;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(pyproject_toml)?;

    uv_snapshot!(
        context.filters(),
        context.upgrade().arg("bar"),
        @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: Could not update dependency `bar` in `dependency-groups.cpu-core`: `bar<1` (declared under conflicting dependency group `cpu`)
    warning: Could not update dependency `bar` in `dependency-groups.gpu-core`: `bar<2` (declared under conflicting dependency group `gpu`)
    "
    );

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
fn upgrade_reports_missing_dependency_in_current_project() -> Result<()> {
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
        context.upgrade().arg("flask"),
        @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Dependency `flask` was not found in the current project
    "
    );

    assert_project_unchanged(&context, pyproject_toml)
}

#[test]
fn upgrade_updates_matching_direct_declarations_across_locations() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/fork-upgrade.toml");
    let pyproject_toml = format!(
        r#"
        [project]
        name = "example"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "foo==1",
            "bar<2",
        ]

        [project.optional-dependencies]
        test = ["bar<2"]

        [dependency-groups]
        dev = [
            "bar<2",
            {{ include-group = "lint" }},
        ]
        lint = []

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
    Updated requirement: `bar<2` -> `bar<3`
    Updated requirement: `bar<2` -> `bar<3`
    "
    );

    let updated_pyproject_toml = fs_err::read_to_string(context.temp_dir.child("pyproject.toml"))?;
    insta::with_settings!({ filters => packse_filters(&context) }, {
        insta::assert_snapshot!(
            updated_pyproject_toml,
            @r#"

[project]
name = "example"
version = "0.1.0"
requires-python = ">=3.12"
dependencies = [
    "foo==1",
    "bar<3",
]

[project.optional-dependencies]
test = ["bar<3"]

[dependency-groups]
dev = [
    "bar<3",
    { include-group = "lint" },
]
lint = []

[[tool.uv.index]]
url = "http://[LOCALHOST]/simple/"
default = true
"#
        );
    });
    assert!(!context.temp_dir.child("uv.lock").exists());
    assert!(!context.temp_dir.child(".venv").exists());
    Ok(())
}

#[test]
fn upgrade_without_package_selects_direct_declarations_across_locations() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/fork-upgrade.toml");
    let pyproject_toml = format!(
        r#"
        [project]
        name = "example"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "foo==1",
            "bar<2",
        ]

        [project.optional-dependencies]
        test = ["bar<2"]

        [dependency-groups]
        dev = ["bar<2"]

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
        context.upgrade().env_remove(EnvVars::UV_EXCLUDE_NEWER),
        @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 3 packages in [TIME]
    Add foo v2.0.0
    Add bar v2.0.0
    Updated requirement: `foo==1` -> `foo==2.0.0`
    Updated requirement: `bar<2` -> `bar<3`
    Updated requirement: `bar<2` -> `bar<3`
    Updated requirement: `bar<2` -> `bar<3`
    "
    );

    let updated_pyproject_toml = fs_err::read_to_string(context.temp_dir.child("pyproject.toml"))?;
    insta::with_settings!({ filters => packse_filters(&context) }, {
        insta::assert_snapshot!(
            updated_pyproject_toml,
            @r#"

[project]
name = "example"
version = "0.1.0"
requires-python = ">=3.12"
dependencies = [
    "foo==2.0.0",
    "bar<3",
]

[project.optional-dependencies]
test = ["bar<3"]

[dependency-groups]
dev = ["bar<3"]

[[tool.uv.index]]
url = "http://[LOCALHOST]/simple/"
default = true
"#
        );
    });
    assert!(!context.temp_dir.child("uv.lock").exists());
    assert!(!context.temp_dir.child(".venv").exists());
    Ok(())
}

#[test]
fn upgrade_exclude_leaves_optional_and_group_dependencies_as_hard_constraints() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/upgrade-outcomes.toml");
    let pyproject_toml = format!(
        r#"
        [project]
        name = "example"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["baz<2"]

        [project.optional-dependencies]
        test = ["bar<2"]

        [dependency-groups]
        dev = ["bar<2"]

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
            .arg("--exclude")
            .arg("bar")
            .env_remove(EnvVars::UV_EXCLUDE_NEWER),
        @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 3 packages in [TIME]
    Add baz v2.0.0
    Updated requirement: `baz<2` -> `baz<3`
    "
    );

    let updated_pyproject_toml = fs_err::read_to_string(context.temp_dir.child("pyproject.toml"))?;
    insta::with_settings!({ filters => packse_filters(&context) }, {
        insta::assert_snapshot!(
            updated_pyproject_toml,
            @r#"

[project]
name = "example"
version = "0.1.0"
requires-python = ">=3.12"
dependencies = ["baz<3"]

[project.optional-dependencies]
test = ["bar<2"]

[dependency-groups]
dev = ["bar<2"]

[[tool.uv.index]]
url = "http://[LOCALHOST]/simple/"
default = true
"#
        );
    });
    assert!(!context.temp_dir.child("uv.lock").exists());
    assert!(!context.temp_dir.child(".venv").exists());
    Ok(())
}

#[test]
fn upgrade_updates_multiple_marked_production_dependencies() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/fork-upgrade.toml");
    let pyproject_toml = format!(
        r#"
        [project]
        name = "example"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            # No upper bound to upgrade.
            "bar>=1",
            "bar<1 ; sys_platform == 'linux'",
            "bar<2 ; sys_platform != 'linux'",
            "bar<2 ; sys_platform != 'linux'",
            "foo==1",
        ]

        [tool.uv]
        environments = ["sys_platform != 'win32'"]

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
    Updated requirement: `bar<1 ; sys_platform == 'linux'` -> `bar<2 ; sys_platform == 'linux'`
    Updated requirement: `bar<2 ; sys_platform != 'linux'` -> `bar<3 ; sys_platform != 'linux'`
    "
    );

    let updated_pyproject_toml = fs_err::read_to_string(context.temp_dir.child("pyproject.toml"))?;
    insta::with_settings!({ filters => packse_filters(&context) }, {
        insta::assert_snapshot!(
            updated_pyproject_toml,
            @r#"

        [project]
        name = "example"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            # No upper bound to upgrade.
            "bar>=1",
            "bar<2 ; sys_platform == 'linux'",
            "bar<3 ; sys_platform != 'linux'",
            "bar<3 ; sys_platform != 'linux'",
            "foo==1",
        ]

        [tool.uv]
        environments = ["sys_platform != 'win32'"]

        [[tool.uv.index]]
        url = "http://[LOCALHOST]/simple/"
        default = true
        "#
        );
    });
    assert!(!context.temp_dir.child("uv.lock").exists());
    assert!(!context.temp_dir.child(".venv").exists());
    Ok(())
}

#[test]
fn upgrade_updates_multiple_named_packages_together() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/fork-upgrade.toml");
    let pyproject_toml = format!(
        r#"
        [project]
        name = "example"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "foo==1",
            "bar<2",
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
            .arg("foo")
            .arg("bar")
            .env_remove(EnvVars::UV_EXCLUDE_NEWER),
        @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 3 packages in [TIME]
    Add foo v2.0.0
    Add bar v2.0.0
    Updated requirement: `foo==1` -> `foo==2.0.0`
    Updated requirement: `bar<2` -> `bar<3`
    "
    );

    let updated_pyproject_toml = fs_err::read_to_string(context.temp_dir.child("pyproject.toml"))?;
    insta::with_settings!({ filters => packse_filters(&context) }, {
        insta::assert_snapshot!(
            updated_pyproject_toml,
            @r#"
        [project]
        name = "example"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "foo==2.0.0",
            "bar<3",
        ]

        [[tool.uv.index]]
        url = "http://[LOCALHOST]/simple/"
        default = true
        "#
        );
    });
    assert!(!context.temp_dir.child("uv.lock").exists());
    assert!(!context.temp_dir.child(".venv").exists());
    Ok(())
}

#[test]
fn upgrade_without_package_selects_all_production_dependencies() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/fork-upgrade.toml");
    let pyproject_toml = format!(
        r#"
        [project]
        name = "example"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "foo==1",
            "bar<2",
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
        context.upgrade().env_remove(EnvVars::UV_EXCLUDE_NEWER),
        @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 3 packages in [TIME]
    Add foo v2.0.0
    Add bar v2.0.0
    Updated requirement: `foo==1` -> `foo==2.0.0`
    Updated requirement: `bar<2` -> `bar<3`
    "
    );

    let updated_pyproject_toml = fs_err::read_to_string(context.temp_dir.child("pyproject.toml"))?;
    insta::with_settings!({ filters => packse_filters(&context) }, {
        insta::assert_snapshot!(
            updated_pyproject_toml,
            @r#"
        [project]
        name = "example"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "foo==2.0.0",
            "bar<3",
        ]

        [[tool.uv.index]]
        url = "http://[LOCALHOST]/simple/"
        default = true
        "#
        );
    });
    assert!(!context.temp_dir.child("uv.lock").exists());
    assert!(!context.temp_dir.child(".venv").exists());
    Ok(())
}

#[test]
fn upgrade_without_package_skips_optional_self_reference() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/fork-upgrade.toml");
    let pyproject_toml = format!(
        r#"
        [project]
        name = "example"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["foo==1"]

        [project.optional-dependencies]
        cli = []
        all = ["example[cli]"]

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
        context.upgrade().env_remove(EnvVars::UV_EXCLUDE_NEWER),
        @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 3 packages in [TIME]
    Add foo v2.0.0
    Updated requirement: `foo==1` -> `foo==2.0.0`
    "
    );

    let updated_pyproject_toml = fs_err::read_to_string(context.temp_dir.child("pyproject.toml"))?;
    insta::with_settings!({ filters => packse_filters(&context) }, {
        insta::assert_snapshot!(
            updated_pyproject_toml,
            @r#"

[project]
name = "example"
version = "0.1.0"
requires-python = ">=3.12"
dependencies = ["foo==2.0.0"]

[project.optional-dependencies]
cli = []
all = ["example[cli]"]

[[tool.uv.index]]
url = "http://[LOCALHOST]/simple/"
default = true
"#
        );
    });
    assert!(!context.temp_dir.child("uv.lock").exists());
    assert!(!context.temp_dir.child(".venv").exists());
    Ok(())
}

#[test]
fn upgrade_without_package_rejects_direct_url_requirement() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);
    let pyproject_toml = r#"
        [project]
        name = "example"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "idna<3",
            "requests @ https://example.com/requests-2.32.0-py3-none-any.whl",
        ]
    "#;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(pyproject_toml)?;

    uv_snapshot!(
        context.filters(),
        context.upgrade(),
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
fn upgrade_without_package_skips_inapplicable_direct_url_and_updates_registry() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/fork-upgrade.toml");
    let pyproject_toml = format!(
        r#"
        [project]
        name = "example"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "foo @ https://example.com/foo-1.0.0-py3-none-any.whl ; python_version < '3.12'",
            "bar<2",
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
        context.upgrade().env_remove(EnvVars::UV_EXCLUDE_NEWER),
        @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: Skipping dependency `foo` in `project.dependencies`: `foo @ https://example.com/foo-1.0.0-py3-none-any.whl ; python_full_version < '3.12'` (excluded by the project's environments or Python requirement)
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 2 packages in [TIME]
    Add bar v2.0.0
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
fn upgrade_without_package_rejects_non_registry_source() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);
    let pyproject_toml = r#"
        [project]
        name = "example"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["idna<3", "requests>=2"]

        [tool.uv.sources]
        requests = { path = "vendor/requests" }
    "#;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(pyproject_toml)?;

    uv_snapshot!(context.filters(), context.upgrade(), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Dependency `requests` uses a non-registry source in `tool.uv.sources` and cannot be upgraded
    ");

    assert_project_unchanged(&context, pyproject_toml)
}

#[test]
fn upgrade_reports_selection_errors_before_interpreter_failure() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);
    let pyproject_toml = r#"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
            "bar<2 ; python_version < '3.12'",
            "requests @ https://example.com/requests-2.32.0-py3-none-any.whl",
            "project[foo]>=0.1",
            "source>=1",
        ]

        [tool.uv.sources]
        source = { path = "vendor/source" }
    "#;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(pyproject_toml)?;

    uv_snapshot!(context.filters(), context.upgrade().args(["bar", "missing"]), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Dependency `missing` was not found in the current project
    ");

    uv_snapshot!(context.filters(), context.upgrade().args(["bar", "requests"]), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Dependency `requests` is a direct URL requirement and cannot be upgraded
    ");

    uv_snapshot!(context.filters(), context.upgrade().args(["bar", "project"]), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Dependency `project` refers to the current project and cannot be upgraded
    ");

    uv_snapshot!(context.filters(), context.upgrade().args(["bar", "source"]), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Dependency `source` uses a non-registry source in `tool.uv.sources` and cannot be upgraded
    ");

    assert_project_unchanged(&context, pyproject_toml)
}

#[test]
fn upgrade_redacts_malformed_direct_url_dependency() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);
    let pyproject_toml = r#"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = [
            "bar @ https://user:super-secret@files.example.com/bar-1.0.0-py3-none-any.whl?X-Amz-Signature=signing-secret ; python_version <",
        ]
    "#;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(pyproject_toml)?;

    uv_snapshot!(context.filters(), context.upgrade().arg("bar"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to parse dependency from `project.dependencies` in `[TEMP_DIR]/pyproject.toml`: Expected marker value, found end of dependency specification
    ");

    assert_project_unchanged(&context, pyproject_toml)
}

#[test]
fn upgrade_exclude_leaves_dependency_as_hard_constraint() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/fork-upgrade.toml");
    let pyproject_toml = format!(
        r#"
        [project]
        name = "example"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "foo==1",
            "bar<2",
        ]

        [tool.uv]
        environments = ["sys_platform == 'linux'"]

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
            .arg("--exclude")
            .arg("bar")
            .env_remove(EnvVars::UV_EXCLUDE_NEWER),
        @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 3 packages in [TIME]
    Add foo v1.0.0
    "
    );

    let updated_pyproject_toml = fs_err::read_to_string(context.temp_dir.child("pyproject.toml"))?;
    insta::with_settings!({ filters => packse_filters(&context) }, {
        insta::assert_snapshot!(
            updated_pyproject_toml,
            @r#"
        [project]
        name = "example"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "foo==1",
            "bar<2",
        ]

        [tool.uv]
        environments = ["sys_platform == 'linux'"]

        [[tool.uv.index]]
        url = "http://[LOCALHOST]/simple/"
        default = true
        "#
        );
    });
    assert!(!context.temp_dir.child("uv.lock").exists());
    assert!(!context.temp_dir.child(".venv").exists());
    Ok(())
}

#[test]
fn upgrade_updates_safe_declarations_and_warns_for_blocked_declarations() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/upgrade-outcomes.toml");
    let pyproject_toml = format!(
        r#"
        [project]
        name = "example"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "foo==1",
            "bar>=1 ; extra == 'cpu'",
            "baz<2",
        ]

        [project.optional-dependencies]
        cpu = []
        gpu = []

        [tool.uv]
        conflicts = [
            [
                {{ extra = "cpu" }},
                {{ extra = "gpu" }},
            ],
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
            .arg("bar")
            .arg("baz")
            .env_remove(EnvVars::UV_EXCLUDE_NEWER),
        @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 5 packages in [TIME]
    Add baz v2.0.0
    warning: Could not update dependency `bar` in `project.dependencies`: `bar>=1 ; extra == 'cpu'` (declared under conflicting extra `cpu`)
    Updated requirement: `baz<2` -> `baz<3`
    "
    );

    let updated_pyproject_toml = fs_err::read_to_string(context.temp_dir.child("pyproject.toml"))?;
    insta::with_settings!({ filters => packse_filters(&context) }, {
        insta::assert_snapshot!(
            updated_pyproject_toml,
            @r#"
        [project]
        name = "example"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "foo==1",
            "bar>=1 ; extra == 'cpu'",
            "baz<3",
        ]

        [project.optional-dependencies]
        cpu = []
        gpu = []

        [tool.uv]
        conflicts = [
            [
                { extra = "cpu" },
                { extra = "gpu" },
            ],
        ]

        [[tool.uv.index]]
        url = "http://[LOCALHOST]/simple/"
        default = true
        "#
        );
    });
    assert!(!context.temp_dir.child("uv.lock").exists());
    assert!(!context.temp_dir.child(".venv").exists());
    Ok(())
}

#[test]
fn upgrade_json_reports_partial_blocked_outcome() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/upgrade-outcomes.toml");
    let pyproject_toml = format!(
        r#"
        [project]
        name = "example"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "foo==1",
            "bar>=1 ; extra == 'cpu'",
            "baz<2",
        ]

        [project.optional-dependencies]
        cpu = []
        gpu = []

        [tool.uv]
        conflicts = [
            [
                {{ extra = "cpu" }},
                {{ extra = "gpu" }},
            ],
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
            .arg("--dry-run")
            .arg("--output-format")
            .arg("json")
            .arg("bar")
            .arg("baz")
            .env_remove(EnvVars::UV_EXCLUDE_NEWER),
        @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "schema": {
        "version": "preview"
      },
      "dry_run": true,
      "packages": [
        "bar",
        "baz"
      ],
      "declarations": [
        {
          "member": "example",
          "package": "bar",
          "dependency_type": "production",
          "location": "project.dependencies",
          "original_requirement": "bar>=1 ; extra == 'cpu'",
          "status": "blocked",
          "reason": {
            "code": "conflict_extra",
            "message": "declared under conflicting extra `cpu`"
          }
        },
        {
          "member": "example",
          "package": "baz",
          "dependency_type": "production",
          "location": "project.dependencies",
          "original_requirement": "baz<2",
          "new_requirement": "baz<3",
          "resolved_versions": [
            "2.0.0"
          ],
          "status": "changed"
        }
      ],
      "lock_changes": [
        {
          "action": "add",
          "package": "baz",
          "previous_versions": [],
          "current_versions": [
            {
              "version": "2.0.0"
            }
          ]
        }
      ]
    }

    ----- stderr -----
    warning: The `--output-format json` option is experimental and the schema may change without warning. Pass `--preview-features json-output` to disable this warning.
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 5 packages in [TIME]
    warning: Could not update dependency `bar` in `project.dependencies`: `bar>=1 ; extra == 'cpu'` (declared under conflicting extra `cpu`)
    "#
    );

    assert_project_unchanged(&context, &pyproject_toml)
}

#[test]
fn upgrade_reports_same_package_mixed_blocked_and_changed_outcomes() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/upgrade-outcomes.toml");
    let pyproject_toml = format!(
        r#"
        [project]
        name = "example"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "bar>=1 ; extra == 'cpu'",
            "bar<2",
        ]

        [project.optional-dependencies]
        cpu = []
        gpu = []

        [tool.uv]
        conflicts = [
            [
                {{ extra = "cpu" }},
                {{ extra = "gpu" }},
            ],
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
            .arg("bar")
            .env_remove(EnvVars::UV_EXCLUDE_NEWER),
        @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 2 packages in [TIME]
    Add bar v2.0.0
    warning: Could not update dependency `bar` in `project.dependencies`: `bar>=1 ; extra == 'cpu'` (declared under conflicting extra `cpu`)
    Updated requirement: `bar<2` -> `bar<3`
    "
    );

    let updated_pyproject_toml = fs_err::read_to_string(context.temp_dir.child("pyproject.toml"))?;
    insta::with_settings!({ filters => packse_filters(&context) }, {
        insta::assert_snapshot!(
            updated_pyproject_toml,
            @r#"

[project]
name = "example"
version = "0.1.0"
requires-python = ">=3.12"
dependencies = [
    "bar>=1 ; extra == 'cpu'",
    "bar<3",
]

[project.optional-dependencies]
cpu = []
gpu = []

[tool.uv]
conflicts = [
    [
        { extra = "cpu" },
        { extra = "gpu" },
    ],
]

[[tool.uv.index]]
url = "http://[LOCALHOST]/simple/"
default = true
"#
        );
    });
    assert!(!context.temp_dir.child("uv.lock").exists());
    assert!(!context.temp_dir.child(".venv").exists());
    Ok(())
}

#[test]
fn upgrade_json_reports_same_package_mixed_blocked_and_changed_outcomes() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/upgrade-outcomes.toml");
    let pyproject_toml = format!(
        r#"
        [project]
        name = "example"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "bar>=1 ; extra == 'cpu'",
            "bar<2",
        ]

        [project.optional-dependencies]
        cpu = []
        gpu = []

        [tool.uv]
        conflicts = [
            [
                {{ extra = "cpu" }},
                {{ extra = "gpu" }},
            ],
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
            .arg("--dry-run")
            .arg("--output-format")
            .arg("json")
            .arg("bar")
            .env_remove(EnvVars::UV_EXCLUDE_NEWER),
        @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "schema": {
        "version": "preview"
      },
      "dry_run": true,
      "packages": [
        "bar"
      ],
      "declarations": [
        {
          "member": "example",
          "package": "bar",
          "dependency_type": "production",
          "location": "project.dependencies",
          "original_requirement": "bar>=1 ; extra == 'cpu'",
          "status": "blocked",
          "reason": {
            "code": "conflict_extra",
            "message": "declared under conflicting extra `cpu`"
          }
        },
        {
          "member": "example",
          "package": "bar",
          "dependency_type": "production",
          "location": "project.dependencies",
          "original_requirement": "bar<2",
          "new_requirement": "bar<3",
          "resolved_versions": [
            "2.0.0"
          ],
          "status": "changed"
        }
      ],
      "lock_changes": [
        {
          "action": "add",
          "package": "bar",
          "previous_versions": [],
          "current_versions": [
            {
              "version": "2.0.0"
            }
          ]
        }
      ]
    }

    ----- stderr -----
    warning: The `--output-format json` option is experimental and the schema may change without warning. Pass `--preview-features json-output` to disable this warning.
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 2 packages in [TIME]
    warning: Could not update dependency `bar` in `project.dependencies`: `bar>=1 ; extra == 'cpu'` (declared under conflicting extra `cpu`)
    "#
    );

    assert_project_unchanged(&context, &pyproject_toml)
}

#[test]
fn upgrade_updates_requirement_constrained_by_conflicting_groups() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/upgrade-outcomes.toml");
    let pyproject_toml = format!(
        r#"
        [project]
        name = "example"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["baz<2"]

        [dependency-groups]
        old = ["baz==1"]
        new = ["baz==2"]

        [tool.uv]
        conflicts = [
            [
                {{ group = "old" }},
                {{ group = "new" }},
            ],
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
            .arg("baz")
            .env_remove(EnvVars::UV_EXCLUDE_NEWER),
        @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 3 packages in [TIME]
    Add baz v1.0.0, v2.0.0
    warning: Could not update dependency `baz` in `dependency-groups.new`: `baz==2` (declared under conflicting dependency group `new`)
    warning: Could not update dependency `baz` in `dependency-groups.old`: `baz==1` (declared under conflicting dependency group `old`)
    Updated requirement: `baz<2` -> `baz<3`
    "
    );

    assert_eq!(
        fs_err::read_to_string(context.temp_dir.child("pyproject.toml"))?,
        pyproject_toml.replace("baz<2", "baz<3")
    );
    assert!(!context.temp_dir.child("uv.lock").exists());
    assert!(!context.temp_dir.child(".venv").exists());
    Ok(())
}

#[test]
fn upgrade_succeeds_when_all_selected_declarations_are_blocked() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/upgrade-outcomes.toml");
    let pyproject_toml = format!(
        r#"
        [project]
        name = "example"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "foo==1",
            "bar==1.*",
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
            .arg("bar")
            .env_remove(EnvVars::UV_EXCLUDE_NEWER),
        @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 4 packages in [TIME]
    warning: Could not update dependency `bar` in `project.dependencies`: `bar==1.*` (Dependency `bar` resolved to versions `1.0.0`, `2.0.0` which cannot be represented by the upgraded requirement; this is not supported yet)
    "
    );

    assert_project_unchanged(&context, &pyproject_toml)?;
    assert!(!context.temp_dir.child("uv.lock").exists());
    assert!(!context.temp_dir.child(".venv").exists());
    Ok(())
}

#[test]
fn upgrade_json_suppresses_lock_changes_when_all_declarations_are_blocked() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/upgrade-outcomes.toml");
    let pyproject_toml = format!(
        r#"
        [project]
        name = "example"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "foo==1",
            "bar==1.*",
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
            .arg("--output-format")
            .arg("json")
            .arg("bar")
            .env_remove(EnvVars::UV_EXCLUDE_NEWER),
        @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "schema": {
        "version": "preview"
      },
      "dry_run": false,
      "packages": [
        "bar"
      ],
      "declarations": [
        {
          "member": "example",
          "package": "bar",
          "dependency_type": "production",
          "location": "project.dependencies",
          "original_requirement": "bar==1.*",
          "resolved_versions": [
            "1.0.0",
            "2.0.0"
          ],
          "status": "blocked",
          "reason": {
            "code": "unrepresentable_requirement",
            "message": "Dependency `bar` resolved to versions `1.0.0`, `2.0.0` which cannot be represented by the upgraded requirement; this is not supported yet"
          }
        }
      ],
      "lock_changes": []
    }

    ----- stderr -----
    warning: The `--output-format json` option is experimental and the schema may change without warning. Pass `--preview-features json-output` to disable this warning.
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 4 packages in [TIME]
    warning: Could not update dependency `bar` in `project.dependencies`: `bar==1.*` (Dependency `bar` resolved to versions `1.0.0`, `2.0.0` which cannot be represented by the upgraded requirement; this is not supported yet)
    "#
    );

    assert_project_unchanged(&context, &pyproject_toml)?;
    assert!(!context.temp_dir.child("uv.lock").exists());
    assert!(!context.temp_dir.child(".venv").exists());
    Ok(())
}

#[test]
fn upgrade_rejects_mixed_updates_after_unrepresentable_blocker() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/upgrade-outcomes.toml");
    let pyproject_toml = format!(
        r#"
        [project]
        name = "example"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "foo==1",
            "bar==1.*",
            "baz<2",
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
            .arg("bar")
            .arg("baz")
            .env_remove(EnvVars::UV_EXCLUDE_NEWER),
        @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 5 packages in [TIME]
    warning: Could not update dependency `bar` in `project.dependencies`: `bar==1.*` (Dependency `bar` resolved to versions `1.0.0`, `2.0.0` which cannot be represented by the upgraded requirement; this is not supported yet)
    error: Could not safely apply dependency updates because one or more selected requirements could not be represented
    "
    );

    assert_project_unchanged(&context, &pyproject_toml)?;
    assert!(!context.temp_dir.child("uv.lock").exists());
    assert!(!context.temp_dir.child(".venv").exists());
    Ok(())
}

#[test]
fn upgrade_preserves_hard_constraint_no_solution_failure() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/upgrade-outcomes.toml");
    let pyproject_toml = format!(
        r#"
        [project]
        name = "example"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "foo==1",
            "bar!=2,<2",
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
            .arg("bar")
            .env_remove(EnvVars::UV_EXCLUDE_NEWER),
        @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
      × No solution found when resolving dependencies for split (markers: sys_platform != 'linux'):
      ╰─▶ Because foo==1.0.0 depends on bar{sys_platform != 'linux'}==2 and your project depends on bar<2, we can conclude that your project and foo==1.0.0 are incompatible.
          And because your project depends on foo==1.0.0, we can conclude that your project's requirements are unsatisfiable.

    hint: The resolution failed for an environment that is not the current one, consider limiting the environments with `tool.uv.environments`.
    "
    );

    assert_project_unchanged(&context, &pyproject_toml)?;
    assert!(!context.temp_dir.child("uv.lock").exists());
    assert!(!context.temp_dir.child(".venv").exists());
    Ok(())
}

#[test]
fn upgrade_ignores_unrelated_path_package_when_attributing_versions() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/fork-upgrade.toml");
    let pyproject_toml = format!(
        r#"
        [project]
        name = "example"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "bar<2",
            "localpkg",
        ]

        [tool.uv.sources]
        localpkg = {{ path = "localpkg" }}

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
    context
        .temp_dir
        .child("localpkg")
        .child("pyproject.toml")
        .write_str(
            r#"
        [project]
        name = "localpkg"
        version = "0.1.0"
        requires-python = ">=3.12"
    "#,
        )?;
    fs_err::remove_dir_all(&context.venv)?;

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
    Resolved 3 packages in [TIME]
    Add bar v2.0.0
    Updated requirement: `bar<2` -> `bar<3`
    "
    );

    let updated_pyproject_toml = fs_err::read_to_string(context.temp_dir.child("pyproject.toml"))?;
    insta::with_settings!({ filters => packse_filters(&context) }, {
        insta::assert_snapshot!(
            updated_pyproject_toml,
            @r#"
        [project]
        name = "example"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "bar<3",
            "localpkg",
        ]

        [tool.uv.sources]
        localpkg = { path = "localpkg" }

        [[tool.uv.index]]
        url = "http://[LOCALHOST]/simple/"
        default = true
        "#
        );
    });
    assert!(!context.temp_dir.child("uv.lock").exists());
    assert!(!context.temp_dir.child(".venv").exists());
    Ok(())
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
fn upgrade_rejects_direct_url_requirement_in_optional_and_group_declarations() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);

    allow_duplicates! {
        for pyproject_toml in [
            r#"
                [project]
                name = "example"
                version = "0.1.0"
                dependencies = []

                [project.optional-dependencies]
                cpu = ["bar @ https://example.com/bar-1.0.0-py3-none-any.whl"]
            "#,
            r#"
                [project]
                name = "example"
                version = "0.1.0"
                dependencies = []

                [dependency-groups]
                dev = ["bar @ https://example.com/bar-1.0.0-py3-none-any.whl"]
            "#,
        ] {
            context
                .temp_dir
                .child("pyproject.toml")
                .write_str(pyproject_toml)?;

            uv_snapshot!(
                context.filters(),
                context.upgrade().arg("bar"),
                @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Dependency `bar` is a direct URL requirement and cannot be upgraded
    "
            );

            assert_project_unchanged(&context, pyproject_toml)?;
        }

        Ok::<(), anyhow::Error>(())
    }?;

    Ok(())
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
fn upgrade_no_sources_ignores_non_registry_source() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/fork-upgrade.toml");
    let pyproject_toml = format!(
        r#"
        [project]
        name = "example"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["bar<2"]

        [tool.uv]
        no-sources = true

        [tool.uv.sources]
        bar = {{ path = "vendor/bar" }}

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
            .arg("bar")
            .env_remove(EnvVars::UV_EXCLUDE_NEWER),
        @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 2 packages in [TIME]
    Add bar v2.0.0
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
fn upgrade_no_sources_package_ignores_selected_non_registry_source() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/fork-upgrade.toml");
    let pyproject_toml = format!(
        r#"
        [project]
        name = "example"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["bar<2"]

        [tool.uv]
        no-sources-package = ["bar"]

        [tool.uv.sources]
        bar = {{ path = "vendor/bar" }}

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
            .arg("bar")
            .env_remove(EnvVars::UV_EXCLUDE_NEWER),
        @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 2 packages in [TIME]
    Add bar v2.0.0
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
fn upgrade_no_sources_package_preserves_non_covered_source_check() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);
    let pyproject_toml = r#"
        [project]
        name = "example"
        version = "0.1.0"
        dependencies = ["bar<2"]

        [tool.uv]
        no-sources-package = ["baz"]

        [tool.uv.sources]
        bar = { path = "vendor/bar" }
    "#;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(pyproject_toml)?;

    uv_snapshot!(
        context.filters(),
        context.upgrade().arg("bar"),
        @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Dependency `bar` uses a non-registry source in `tool.uv.sources` and cannot be upgraded
    "
    );

    assert_project_unchanged(&context, pyproject_toml)
}

#[test]
fn upgrade_no_sources_preserves_direct_url_requirement_rejection() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);
    let pyproject_toml = r#"
        [project]
        name = "example"
        version = "0.1.0"
        dependencies = [
            "bar @ https://example.com/bar-1.0.0-py3-none-any.whl",
        ]

        [tool.uv]
        no-sources = true
    "#;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(pyproject_toml)?;

    uv_snapshot!(
        context.filters(),
        context.upgrade().arg("bar"),
        @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Dependency `bar` is a direct URL requirement and cannot be upgraded
    "
    );

    assert_project_unchanged(&context, pyproject_toml)
}

#[test]
fn upgrade_skips_non_registry_source_for_undefined_extra() -> Result<()> {
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
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: Skipping dependency `requests` in `project.dependencies`: `requests>=2 ; extra == 'gpu'` (references an extra that the project does not provide)
    "
    );

    assert_project_unchanged(&context, pyproject_toml)
}

#[test]
fn upgrade_uses_optional_and_group_source_origins() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/upgrade-outcomes.toml");
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
        dependencies = []

        [project.optional-dependencies]
        cpu = ["bar<2"]

        [dependency-groups]
        dev = ["baz<2"]

        [tool.uv.sources]
        bar = {{ index = "local", extra = "cpu" }}
        baz = {{ index = "local", group = "dev" }}

        [[tool.uv.index]]
        name = "local"
        url = "{}"
        explicit = true

        [[tool.uv.index]]
        name = "empty"
        url = "{empty_index}"
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
            .arg("bar")
            .arg("baz")
            .env_remove(EnvVars::UV_EXCLUDE_NEWER),
        @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 3 packages in [TIME]
    Add bar v2.0.0
    Add baz v2.0.0
    Updated requirement: `bar<2` -> `bar<3`
    Updated requirement: `baz<2` -> `baz<3`
    "
    );

    let updated_pyproject_toml = fs_err::read_to_string(context.temp_dir.child("pyproject.toml"))?;
    insta::with_settings!({ filters => packse_filters(&context) }, {
        insta::assert_snapshot!(
            updated_pyproject_toml,
            @r#"

[project]
name = "example"
version = "0.1.0"
requires-python = ">=3.12"
dependencies = []

[project.optional-dependencies]
cpu = ["bar<3"]

[dependency-groups]
dev = ["baz<3"]

[tool.uv.sources]
bar = { index = "local", extra = "cpu" }
baz = { index = "local", group = "dev" }

[[tool.uv.index]]
name = "local"
url = "http://[LOCALHOST]/simple/"
explicit = true

[[tool.uv.index]]
name = "empty"
url = "file://[TEMP_DIR]/empty-index/"
default = true
"#
        );
    });
    assert!(!context.temp_dir.child("uv.lock").exists());
    assert!(!context.temp_dir.child(".venv").exists());
    Ok(())
}

#[test]
fn upgrade_rejects_include_scoped_non_registry_source() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);
    let pyproject_toml = r#"
        [project]
        name = "example"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [dependency-groups]
        base = ["bar<2"]
        docs = [{ include-group = "base" }]

        [tool.uv.sources]
        bar = { git = "https://github.com/example/bar", group = "docs" }
    "#;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(pyproject_toml)?;

    uv_snapshot!(
        context.filters(),
        context.upgrade().arg("bar"),
        @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Dependency `bar` uses a non-registry source in `tool.uv.sources` and cannot be upgraded
    "
    );

    assert_project_unchanged(&context, pyproject_toml)
}

#[test]
fn upgrade_rejects_unscoped_non_registry_sources_for_optional_and_group_declarations() -> Result<()>
{
    let context = uv_test::test_context_with_versions!(&[]);

    allow_duplicates! {
        for pyproject_toml in [
            r#"
                [project]
                name = "example"
                version = "0.1.0"
                dependencies = []

                [project.optional-dependencies]
                cpu = ["bar<2"]

                [tool.uv.sources]
                bar = { path = "vendor/bar" }
            "#,
            r#"
                [project]
                name = "example"
                version = "0.1.0"
                dependencies = []

                [dependency-groups]
                dev = ["bar<2"]

                [tool.uv.sources]
                bar = { path = "vendor/bar" }
            "#,
        ] {
            context
                .temp_dir
                .child("pyproject.toml")
                .write_str(pyproject_toml)?;

            uv_snapshot!(
                context.filters(),
                context.upgrade().arg("bar"),
                @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Dependency `bar` uses a non-registry source in `tool.uv.sources` and cannot be upgraded
    "
            );

            assert_project_unchanged(&context, pyproject_toml)?;
        }

        Ok::<(), anyhow::Error>(())
    }?;

    Ok(())
}

#[test]
fn upgrade_rejects_unscoped_git_revision_for_optional_and_group_declarations() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);

    allow_duplicates! {
        for pyproject_toml in [
            r#"
                [project]
                name = "example"
                version = "0.1.0"
                dependencies = []

                [project.optional-dependencies]
                cpu = ["bar<2"]

                [tool.uv.sources]
                bar = { git = "https://github.com/example/bar", rev = "6f205ff422bccd5e4c4fc0b64c5f3e7df5181db6" }
            "#,
            r#"
                [project]
                name = "example"
                version = "0.1.0"
                dependencies = []

                [dependency-groups]
                dev = ["bar<2"]

                [tool.uv.sources]
                bar = { git = "https://github.com/example/bar", rev = "6f205ff422bccd5e4c4fc0b64c5f3e7df5181db6" }
            "#,
        ] {
            context
                .temp_dir
                .child("pyproject.toml")
                .write_str(pyproject_toml)?;

            uv_snapshot!(
                context.filters(),
                context.upgrade().arg("bar"),
                @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Dependency `bar` is pinned to a Git revision and cannot be upgraded commit-to-commit
    "
            );

            assert_project_unchanged(&context, pyproject_toml)?;
        }

        Ok::<(), anyhow::Error>(())
    }?;

    Ok(())
}

#[test]
fn upgrade_rejects_unscoped_include_group_non_registry_source() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);
    let pyproject_toml = r#"
        [project]
        name = "example"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [dependency-groups]
        base = ["bar<2"]
        docs = [{ include-group = "base" }]

        [tool.uv.sources]
        bar = { path = "vendor/bar" }
    "#;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(pyproject_toml)?;

    uv_snapshot!(
        context.filters(),
        context.upgrade().arg("bar"),
        @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Dependency `bar` uses a non-registry source in `tool.uv.sources` and cannot be upgraded
    "
    );

    assert_project_unchanged(&context, pyproject_toml)
}

#[test]
fn upgrade_ignores_disjoint_include_scoped_source() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/fork-upgrade.toml");
    let pyproject_toml = format!(
        r#"
        [project]
        name = "example"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [dependency-groups]
        base = ["bar<2"]
        docs = [{{ include-group = "base" }}]

        [tool.uv.dependency-groups]
        docs = {{ requires-python = "<3.12" }}

        [tool.uv.sources]
        bar = {{ git = "https://github.com/example/bar", group = "docs" }}

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
            .arg("bar")
            .env_remove(EnvVars::UV_EXCLUDE_NEWER),
        @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 2 packages in [TIME]
    Add bar v2.0.0
    Updated requirement: `bar<2` -> `bar<3`
    "
    );

    let updated_pyproject_toml = fs_err::read_to_string(context.temp_dir.child("pyproject.toml"))?;
    insta::with_settings!({ filters => packse_filters(&context) }, {
        insta::assert_snapshot!(
            updated_pyproject_toml,
            @r#"

[project]
name = "example"
version = "0.1.0"
requires-python = ">=3.12"
dependencies = []

[dependency-groups]
base = ["bar<3"]
docs = [{ include-group = "base" }]

[tool.uv.dependency-groups]
docs = { requires-python = "<3.12" }

[tool.uv.sources]
bar = { git = "https://github.com/example/bar", group = "docs" }

[[tool.uv.index]]
url = "http://[LOCALHOST]/simple/"
default = true
"#
        );
    });
    assert!(!context.temp_dir.child("uv.lock").exists());
    assert!(!context.temp_dir.child(".venv").exists());
    Ok(())
}

#[test]
fn upgrade_rejects_non_registry_sources_for_optional_and_group_declarations() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);

    allow_duplicates! {
        for pyproject_toml in [
            r#"
                [project]
                name = "example"
                version = "0.1.0"
                dependencies = []

                [project.optional-dependencies]
                cpu = ["bar<2"]

                [tool.uv.sources]
                bar = { git = "https://github.com/example/bar", extra = "cpu" }
            "#,
            r#"
                [project]
                name = "example"
                version = "0.1.0"
                dependencies = []

                [dependency-groups]
                dev = ["bar<2"]

                [tool.uv.sources]
                bar = { git = "https://github.com/example/bar", group = "dev" }
            "#,
        ] {
            context
                .temp_dir
                .child("pyproject.toml")
                .write_str(pyproject_toml)?;

            uv_snapshot!(
                context.filters(),
                context.upgrade().arg("bar"),
                @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Dependency `bar` uses a non-registry source in `tool.uv.sources` and cannot be upgraded
    "
            );

            assert_project_unchanged(&context, pyproject_toml)?;
        }

        Ok::<(), anyhow::Error>(())
    }?;

    Ok(())
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
fn upgrade_ignores_inapplicable_non_registry_source_without_requires_python() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/upgrade-outcomes.toml");
    let pyproject_toml = format!(
        r#"
        [project]
        name = "example"
        version = "0.1.0"
        dependencies = ["baz<2"]

        [tool.uv.sources]
        baz = {{ path = "vendor/baz", marker = "python_version < '3.12'" }}

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
    context
        .temp_dir
        .child("vendor/baz/pyproject.toml")
        .write_str(
            r#"
        [project]
        name = "baz"
        version = "0.1.0"
        "#,
        )?;
    fs_err::remove_dir_all(&context.venv)?;

    uv_snapshot!(
        packse_filters(&context),
        context
            .upgrade()
            .arg("baz")
            .env_remove(EnvVars::UV_EXCLUDE_NEWER),
        @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    warning: No `requires-python` value found in the workspace. Defaulting to `>=3.12`.
    Resolved 2 packages in [TIME]
    Add baz v2.0.0
    Updated requirement: `baz<2` -> `baz<3`
    "
    );

    assert_eq!(
        fs_err::read_to_string(context.temp_dir.child("pyproject.toml"))?,
        pyproject_toml.replace("baz<2", "baz<3")
    );
    assert!(!context.temp_dir.child("uv.lock").exists());
    assert!(!context.temp_dir.child(".venv").exists());
    Ok(())
}

#[test]
fn upgrade_skips_excluded_declarations_and_updates_applicable_requirement() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/fork-upgrade.toml");
    let pyproject_toml = format!(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "bar<2",
            "bar>=1 ; extra == 'does-not-exist' or sys_platform != 'win32'",
            "bar @ https://user:secret@example.com/bar-1.0.0-py3-none-any.whl?token=secret ; python_version < '3.12' or sys_platform == 'win32'",
            "bar<2 ; extra == 'does-not-exist'",
            "foo==2",
        ]

        [tool.uv]
        environments = ["sys_platform != 'win32'"]

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
            .arg("bar")
            .env_remove(EnvVars::UV_EXCLUDE_NEWER),
        @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: Skipping dependency `bar` in `project.dependencies`: `bar @ https://user:****@example.com/bar-1.0.0-py3-none-any.whl ; python_full_version < '3.12' or sys_platform == 'win32'` (excluded by the project's environments or Python requirement)
    warning: Skipping dependency `bar` in `project.dependencies`: `bar<2 ; extra == 'does-not-exist'` (references an extra that the project does not provide)
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 3 packages in [TIME]
    Add bar v2.0.0
    Updated requirement: `bar<2` -> `bar<3`
    "
    );

    assert_eq!(
        fs_err::read_to_string(context.temp_dir.child("pyproject.toml"))?,
        pyproject_toml.replacen("bar<2\",", "bar<3\",", 1)
    );
    assert!(!context.temp_dir.child("uv.lock").exists());
    assert!(!context.temp_dir.child(".venv").exists());
    Ok(())
}

#[test]
fn upgrade_skips_excluded_optional_declaration() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/fork-upgrade.toml");
    let pyproject_toml = format!(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["bar<2"]

        [project.optional-dependencies]
        legacy = ["bar @ https://example.com/bar-1.0.0-py3-none-any.whl ; python_version < '3.12'"]

        [tool.uv.sources]
        bar = {{ url = "https://example.com/bar-1.0.0-py3-none-any.whl", extra = "legacy" }}

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

    uv_snapshot!(packse_filters(&context), context.upgrade().arg("bar").env_remove(EnvVars::UV_EXCLUDE_NEWER), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: Skipping dependency `bar` in `project.optional-dependencies.legacy`: `bar @ https://example.com/bar-1.0.0-py3-none-any.whl ; python_full_version < '3.12'` (excluded by the project's environments or Python requirement)
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 2 packages in [TIME]
    Add bar v2.0.0
    Updated requirement: `bar<2` -> `bar<3`
    ");

    assert_eq!(
        fs_err::read_to_string(context.temp_dir.child("pyproject.toml"))?,
        pyproject_toml.replace("bar<2", "bar<3")
    );
    assert!(!context.temp_dir.child("uv.lock").exists());
    assert!(!context.temp_dir.child(".venv").exists());
    Ok(())
}

#[test]
fn upgrade_skips_excluded_group_declaration() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/fork-upgrade.toml");
    let pyproject_toml = format!(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [dependency-groups]
        legacy = ["bar @ https://example.com/bar-1.0.0-py3-none-any.whl"]
        docs = [{{ include-group = "legacy" }}]
        test = ["bar<2"]

        [tool.uv.dependency-groups]
        legacy = {{ requires-python = "<3.12" }}
        docs = {{ requires-python = "<3.12" }}

        [tool.uv.sources]
        bar = {{ url = "https://example.com/bar-1.0.0-py3-none-any.whl", group = "docs" }}

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

    uv_snapshot!(packse_filters(&context), context.upgrade().arg("bar").env_remove(EnvVars::UV_EXCLUDE_NEWER), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: Skipping dependency `bar` in `dependency-groups.legacy`: `bar @ https://example.com/bar-1.0.0-py3-none-any.whl` (excluded by the project's environments or Python requirement)
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 2 packages in [TIME]
    Add bar v2.0.0
    Updated requirement: `bar<2` -> `bar<3`
    ");

    assert_eq!(
        fs_err::read_to_string(context.temp_dir.child("pyproject.toml"))?,
        pyproject_toml.replace("bar<2", "bar<3")
    );
    assert!(!context.temp_dir.child("uv.lock").exists());
    assert!(!context.temp_dir.child(".venv").exists());
    Ok(())
}

#[test]
fn upgrade_skips_inapplicable_optional_without_requires_python() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&["3.12"]);
    let pyproject_toml = r#"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = []

        [project.optional-dependencies]
        legacy = ["bar<2 ; python_version < '3.12'"]
    "#;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(pyproject_toml)?;

    uv_snapshot!(context.filters(), context.upgrade().arg("bar"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    warning: Skipping dependency `bar` in `project.optional-dependencies.legacy`: `bar<2 ; python_full_version < '3.12'` (excluded by the project's environments or Python requirement)
    ");

    assert_project_unchanged(&context, pyproject_toml)
}

#[test]
fn upgrade_skips_inapplicable_group_without_requires_python() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&["3.12"]);
    let pyproject_toml = r#"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = []

        [dependency-groups]
        legacy = ["bar<2"]

        [tool.uv.dependency-groups]
        legacy = { requires-python = "<3.12" }
    "#;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(pyproject_toml)?;

    uv_snapshot!(context.filters(), context.upgrade().arg("bar"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    warning: Skipping dependency `bar` in `dependency-groups.legacy`: `bar<2` (excluded by the project's environments or Python requirement)
    ");

    assert_project_unchanged(&context, pyproject_toml)
}

#[test]
fn upgrade_redacts_malformed_direct_url_optional_and_group_dependencies() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);
    let optional_pyproject_toml = r#"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = []

        [project.optional-dependencies]
        legacy = ["bar @ https://user:super-secret@files.example.com/bar-1.0.0-py3-none-any.whl?X-Amz-Signature=signing-secret ; python_version <"]
    "#;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(optional_pyproject_toml)?;

    uv_snapshot!(context.filters(), context.upgrade().arg("bar"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to parse dependency from `project.optional-dependencies.legacy` in `[TEMP_DIR]/pyproject.toml`: Expected marker value, found end of dependency specification
    ");

    assert_project_unchanged(&context, optional_pyproject_toml)?;

    let group_pyproject_toml = r#"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = []

        [dependency-groups]
        legacy = ["bar @ https://user:super-secret@files.example.com/bar-1.0.0-py3-none-any.whl?X-Amz-Signature=signing-secret ; python_version <"]
    "#;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(group_pyproject_toml)?;

    uv_snapshot!(context.filters(), context.upgrade().arg("bar"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Project `project` has malformed dependency groups
      Caused by: Failed to parse entry in group `legacy`: Expected marker value, found end of dependency specification
    ");

    assert_project_unchanged(&context, group_pyproject_toml)
}

#[test]
fn upgrade_json_preview_feature_suppresses_warning() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/fork-upgrade.toml");
    let pyproject_toml = format!(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["bar<2"]

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
            .arg("--preview-features")
            .arg("json-output")
            .arg("--dry-run")
            .arg("--output-format")
            .arg("json")
            .arg("bar")
            .env_remove(EnvVars::UV_EXCLUDE_NEWER),
        @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "schema": {
        "version": "preview"
      },
      "dry_run": true,
      "packages": [
        "bar"
      ],
      "declarations": [
        {
          "member": "project",
          "package": "bar",
          "dependency_type": "production",
          "location": "project.dependencies",
          "original_requirement": "bar<2",
          "new_requirement": "bar<3",
          "resolved_versions": [
            "2.0.0"
          ],
          "status": "changed"
        }
      ],
      "lock_changes": [
        {
          "action": "add",
          "package": "bar",
          "previous_versions": [],
          "current_versions": [
            {
              "version": "2.0.0"
            }
          ]
        }
      ]
    }

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 2 packages in [TIME]
    "#
    );

    assert_project_unchanged(&context, &pyproject_toml)
}

#[test]
fn upgrade_json_reports_existing_lockfile_changes() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/upgrade-outcomes.toml");
    let pyproject_toml = format!(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["bar==1"]

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
        context.lock().env_remove(EnvVars::UV_EXCLUDE_NEWER),
        @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 2 packages in [TIME]
    "
    );
    let lock = fs_err::read(context.temp_dir.child("uv.lock"))?;

    uv_snapshot!(
        packse_filters(&context),
        context
            .upgrade()
            .arg("--output-format")
            .arg("json")
            .arg("bar")
            .env_remove(EnvVars::UV_EXCLUDE_NEWER),
        @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "schema": {
        "version": "preview"
      },
      "dry_run": false,
      "packages": [
        "bar"
      ],
      "declarations": [
        {
          "member": "project",
          "package": "bar",
          "dependency_type": "production",
          "location": "project.dependencies",
          "original_requirement": "bar==1",
          "new_requirement": "bar==2.0.0",
          "resolved_versions": [
            "2.0.0"
          ],
          "status": "changed"
        }
      ],
      "lock_changes": [
        {
          "action": "update",
          "package": "bar",
          "previous_versions": [
            {
              "version": "1.0.0"
            }
          ],
          "current_versions": [
            {
              "version": "2.0.0"
            }
          ]
        }
      ]
    }

    ----- stderr -----
    warning: The `--output-format json` option is experimental and the schema may change without warning. Pass `--preview-features json-output` to disable this warning.
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 2 packages in [TIME]
    "#
    );

    assert_eq!(
        fs_err::read_to_string(context.temp_dir.child("pyproject.toml"))?,
        pyproject_toml.replace("bar==1", "bar==2.0.0")
    );
    assert_eq!(fs_err::read(context.temp_dir.child("uv.lock"))?, lock);
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
fn upgrade_preserves_workspace_root_dependency_groups() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/fork-upgrade.toml");
    let workspace_pyproject_toml = format!(
        r#"
        [tool.uv.workspace]
        members = ["project"]

        [dependency-groups]
        root = ["bar<2"]

        [[tool.uv.index]]
        url = "{}"
        default = true
    "#,
        server.index_url()
    );
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(&workspace_pyproject_toml)?;

    let project = context.temp_dir.child("project");
    project.create_dir_all()?;
    let project_pyproject_toml = r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "foo==1",
            "bar<2",
        ]
    "#;
    project
        .child("pyproject.toml")
        .write_str(project_pyproject_toml)?;
    fs_err::remove_dir_all(&context.venv)?;

    uv_snapshot!(
        packse_filters(&context),
        context
            .upgrade()
            .current_dir(&project)
            .arg("bar")
            .env_remove(EnvVars::UV_EXCLUDE_NEWER),
        @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
      × No solution found when resolving dependencies for split (markers: sys_platform != 'linux'):
      ╰─▶ Because foo==1.0.0 depends on bar{sys_platform != 'linux'}==2 and your project depends on foo==1, we can conclude that your project depends on bar==2.
          And because your project requires bar<2, we can conclude that your project's requirements and your project are incompatible.

    hint: The resolution failed for an environment that is not the current one, consider limiting the environments with `tool.uv.environments`.
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
    assert!(!project.child("uv.lock").exists());
    assert!(!project.child(".venv").exists());
    Ok(())
}

#[test]
fn upgrade_updates_current_workspace_member_by_default() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/fork-upgrade.toml");
    let workspace_pyproject_toml = format!(
        r#"
        [tool.uv.workspace]
        members = ["member-a", "member-b"]

        [[tool.uv.index]]
        url = "{}"
        default = true
    "#,
        server.index_url()
    );
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(&workspace_pyproject_toml)?;

    let member_a = context.temp_dir.child("member-a");
    member_a.create_dir_all()?;
    let member_a_pyproject_toml = r#"
        [project]
        name = "member-a"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["foo==1"]
    "#;
    member_a
        .child("pyproject.toml")
        .write_str(member_a_pyproject_toml)?;

    let member_b = context.temp_dir.child("member-b");
    member_b.create_dir_all()?;
    let member_b_pyproject_toml = r#"
        [project]
        name = "member-b"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["foo>=1"]
    "#;
    member_b
        .child("pyproject.toml")
        .write_str(member_b_pyproject_toml)?;
    fs_err::remove_dir_all(&context.venv)?;

    uv_snapshot!(
        packse_filters(&context),
        context
            .upgrade()
            .current_dir(&member_a)
            .arg("foo")
            .env_remove(EnvVars::UV_EXCLUDE_NEWER),
        @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 4 packages in [TIME]
    Add foo v2.0.0
    Updated requirement: `foo==1` -> `foo==2.0.0`
    "
    );

    assert_eq!(
        fs_err::read_to_string(context.temp_dir.child("pyproject.toml"))?,
        workspace_pyproject_toml
    );
    assert_eq!(
        fs_err::read_to_string(member_a.child("pyproject.toml"))?,
        member_a_pyproject_toml.replace("foo==1", "foo==2.0.0")
    );
    assert_eq!(
        fs_err::read_to_string(member_b.child("pyproject.toml"))?,
        member_b_pyproject_toml
    );
    assert!(!context.temp_dir.child("uv.lock").exists());
    assert!(!context.temp_dir.child(".venv").exists());
    assert!(!member_a.child("uv.lock").exists());
    assert!(!member_a.child(".venv").exists());
    assert!(!member_b.child("uv.lock").exists());
    assert!(!member_b.child(".venv").exists());
    Ok(())
}

#[test]
fn upgrade_package_selects_member_from_virtual_workspace_root() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/fork-upgrade.toml");
    let workspace_pyproject_toml = format!(
        r#"
        [tool.uv.workspace]
        members = ["member-a", "member-b"]

        [[tool.uv.index]]
        url = "{}"
        default = true
    "#,
        server.index_url()
    );
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(&workspace_pyproject_toml)?;

    let member_a = context.temp_dir.child("member-a");
    member_a.create_dir_all()?;
    let member_a_pyproject_toml = r#"
        [project]
        name = "member-a"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["foo==1"]
    "#;
    member_a
        .child("pyproject.toml")
        .write_str(member_a_pyproject_toml)?;

    let member_b = context.temp_dir.child("member-b");
    member_b.create_dir_all()?;
    let member_b_pyproject_toml = r#"
        [project]
        name = "member-b"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#;
    member_b
        .child("pyproject.toml")
        .write_str(member_b_pyproject_toml)?;
    fs_err::remove_dir_all(&context.venv)?;

    uv_snapshot!(
        packse_filters(&context),
        context
            .upgrade()
            .arg("--package")
            .arg("member-a")
            .arg("foo")
            .env_remove(EnvVars::UV_EXCLUDE_NEWER),
        @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 4 packages in [TIME]
    Add foo v2.0.0
    Updated requirement: `foo==1` -> `foo==2.0.0`
    "
    );

    assert_eq!(
        fs_err::read_to_string(context.temp_dir.child("pyproject.toml"))?,
        workspace_pyproject_toml
    );
    assert_eq!(
        fs_err::read_to_string(member_a.child("pyproject.toml"))?,
        member_a_pyproject_toml.replace("foo==1", "foo==2.0.0")
    );
    assert_eq!(
        fs_err::read_to_string(member_b.child("pyproject.toml"))?,
        member_b_pyproject_toml
    );
    assert!(!context.temp_dir.child("uv.lock").exists());
    assert!(!context.temp_dir.child(".venv").exists());
    Ok(())
}

#[test]
fn upgrade_package_skips_inapplicable_member_without_requires_python() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&["3.12"]);
    let workspace_pyproject_toml = r#"
        [tool.uv.workspace]
        members = ["member-a"]
    "#;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(workspace_pyproject_toml)?;

    let member_a = context.temp_dir.child("member-a");
    member_a.create_dir_all()?;
    let member_a_pyproject_toml = r#"
        [project]
        name = "member-a"
        version = "0.1.0"
        dependencies = ["bar<2 ; python_version < '3.12'"]
    "#;
    member_a
        .child("pyproject.toml")
        .write_str(member_a_pyproject_toml)?;

    uv_snapshot!(
        context.filters(),
        context
            .upgrade()
            .arg("--package")
            .arg("member-a")
            .arg("bar"),
        @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    warning: Skipping dependency `bar` in `project.dependencies`: `bar<2 ; python_full_version < '3.12'` (excluded by the project's environments or Python requirement)
    "
    );

    assert_project_unchanged(&context, workspace_pyproject_toml)?;
    assert_eq!(
        fs_err::read_to_string(member_a.child("pyproject.toml"))?,
        member_a_pyproject_toml
    );
    Ok(())
}

#[test]
fn upgrade_all_packages_updates_matching_declarations_across_members() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/fork-upgrade.toml");
    let workspace_pyproject_toml = format!(
        r#"
        [tool.uv.workspace]
        members = ["member-a", "member-b"]

        [[tool.uv.index]]
        url = "{}"
        default = true
    "#,
        server.index_url()
    );
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(&workspace_pyproject_toml)?;

    let member_a = context.temp_dir.child("member-a");
    member_a.create_dir_all()?;
    let member_a_pyproject_toml = r#"
        [project]
        name = "member-a"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["foo==1"]
    "#;
    member_a
        .child("pyproject.toml")
        .write_str(member_a_pyproject_toml)?;

    let member_b = context.temp_dir.child("member-b");
    member_b.create_dir_all()?;
    let member_b_pyproject_toml = r#"
        [project]
        name = "member-b"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["foo==1"]
    "#;
    member_b
        .child("pyproject.toml")
        .write_str(member_b_pyproject_toml)?;
    fs_err::remove_dir_all(&context.venv)?;

    uv_snapshot!(
        packse_filters(&context),
        context
            .upgrade()
            .arg("--all-packages")
            .arg("foo")
            .env_remove(EnvVars::UV_EXCLUDE_NEWER),
        @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 4 packages in [TIME]
    Add foo v2.0.0
    Updated requirement in package `member-a`: `foo==1` -> `foo==2.0.0`
    Updated requirement in package `member-b`: `foo==1` -> `foo==2.0.0`
    "
    );

    assert_eq!(
        fs_err::read_to_string(member_a.child("pyproject.toml"))?,
        member_a_pyproject_toml.replace("foo==1", "foo==2.0.0")
    );
    assert_eq!(
        fs_err::read_to_string(member_b.child("pyproject.toml"))?,
        member_b_pyproject_toml.replace("foo==1", "foo==2.0.0")
    );
    assert!(!context.temp_dir.child("uv.lock").exists());
    assert!(!context.temp_dir.child(".venv").exists());
    Ok(())
}

#[test]
fn upgrade_all_packages_without_package_skips_member_self_references() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/fork-upgrade.toml");
    let workspace_pyproject_toml = format!(
        r#"
        [tool.uv.workspace]
        members = ["member-a", "member-b"]

        [[tool.uv.index]]
        url = "{}"
        default = true
    "#,
        server.index_url()
    );
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(&workspace_pyproject_toml)?;

    let member_a = context.temp_dir.child("member-a");
    member_a.create_dir_all()?;
    let member_a_pyproject_toml = r#"
        [project]
        name = "member-a"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["foo==1"]

        [project.optional-dependencies]
        cli = []
        all = ["member-a[cli]"]
    "#;
    member_a
        .child("pyproject.toml")
        .write_str(member_a_pyproject_toml)?;

    let member_b = context.temp_dir.child("member-b");
    member_b.create_dir_all()?;
    let member_b_pyproject_toml = r#"
        [project]
        name = "member-b"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["bar<2"]

        [project.optional-dependencies]
        cli = []
        all = ["member-b[cli]"]
    "#;
    member_b
        .child("pyproject.toml")
        .write_str(member_b_pyproject_toml)?;
    fs_err::remove_dir_all(&context.venv)?;

    uv_snapshot!(
        packse_filters(&context),
        context
            .upgrade()
            .arg("--all-packages")
            .env_remove(EnvVars::UV_EXCLUDE_NEWER),
        @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 4 packages in [TIME]
    Add foo v2.0.0
    Add bar v2.0.0
    Updated requirement in package `member-a`: `foo==1` -> `foo==2.0.0`
    Updated requirement in package `member-b`: `bar<2` -> `bar<3`
    "
    );

    assert_eq!(
        fs_err::read_to_string(context.temp_dir.child("pyproject.toml"))?,
        workspace_pyproject_toml
    );
    assert_eq!(
        fs_err::read_to_string(member_a.child("pyproject.toml"))?,
        member_a_pyproject_toml.replace("foo==1", "foo==2.0.0")
    );
    assert_eq!(
        fs_err::read_to_string(member_b.child("pyproject.toml"))?,
        member_b_pyproject_toml.replace("bar<2", "bar<3")
    );
    assert!(!context.temp_dir.child("uv.lock").exists());
    assert!(!context.temp_dir.child(".venv").exists());
    assert!(!member_a.child("uv.lock").exists());
    assert!(!member_a.child(".venv").exists());
    assert!(!member_b.child("uv.lock").exists());
    assert!(!member_b.child(".venv").exists());
    Ok(())
}

#[test]
fn upgrade_without_package_skips_current_workspace_member_self_reference() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/fork-upgrade.toml");
    let workspace_pyproject_toml = format!(
        r#"
        [tool.uv.workspace]
        members = ["member-a", "member-b"]

        [[tool.uv.index]]
        url = "{}"
        default = true
    "#,
        server.index_url()
    );
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(&workspace_pyproject_toml)?;

    let member_a = context.temp_dir.child("member-a");
    member_a.create_dir_all()?;
    let member_a_pyproject_toml = r#"
        [project]
        name = "member-a"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["foo==1"]

        [project.optional-dependencies]
        cli = []
        all = ["member-a[cli]"]
    "#;
    member_a
        .child("pyproject.toml")
        .write_str(member_a_pyproject_toml)?;

    let member_b = context.temp_dir.child("member-b");
    member_b.create_dir_all()?;
    let member_b_pyproject_toml = r#"
        [project]
        name = "member-b"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["bar>=1"]

        [project.optional-dependencies]
        cli = []
        all = ["member-b[cli]"]
    "#;
    member_b
        .child("pyproject.toml")
        .write_str(member_b_pyproject_toml)?;
    fs_err::remove_dir_all(&context.venv)?;

    uv_snapshot!(
        packse_filters(&context),
        context
            .upgrade()
            .current_dir(&member_a)
            .env_remove(EnvVars::UV_EXCLUDE_NEWER),
        @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 4 packages in [TIME]
    Add foo v2.0.0
    Updated requirement: `foo==1` -> `foo==2.0.0`
    "
    );

    assert_eq!(
        fs_err::read_to_string(context.temp_dir.child("pyproject.toml"))?,
        workspace_pyproject_toml
    );
    assert_eq!(
        fs_err::read_to_string(member_a.child("pyproject.toml"))?,
        member_a_pyproject_toml.replace("foo==1", "foo==2.0.0")
    );
    assert_eq!(
        fs_err::read_to_string(member_b.child("pyproject.toml"))?,
        member_b_pyproject_toml
    );
    assert!(!context.temp_dir.child("uv.lock").exists());
    assert!(!context.temp_dir.child(".venv").exists());
    assert!(!member_a.child("uv.lock").exists());
    assert!(!member_a.child(".venv").exists());
    assert!(!member_b.child("uv.lock").exists());
    assert!(!member_b.child(".venv").exists());
    Ok(())
}

#[test]
fn upgrade_workspace_json_reports_member_locations_without_mutation() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/fork-upgrade.toml");
    let workspace_pyproject_toml = format!(
        r#"
        [tool.uv.workspace]
        members = ["member-a", "member-b"]

        [[tool.uv.index]]
        url = "{}"
        default = true
    "#,
        server.index_url()
    );
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(&workspace_pyproject_toml)?;

    let member_a = context.temp_dir.child("member-a");
    member_a.create_dir_all()?;
    let member_a_pyproject_toml = r#"
        [project]
        name = "member-a"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["foo==1"]
    "#;
    member_a
        .child("pyproject.toml")
        .write_str(member_a_pyproject_toml)?;

    let member_b = context.temp_dir.child("member-b");
    member_b.create_dir_all()?;
    let member_b_pyproject_toml = r#"
        [project]
        name = "member-b"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["foo==1"]
    "#;
    member_b
        .child("pyproject.toml")
        .write_str(member_b_pyproject_toml)?;
    fs_err::remove_dir_all(&context.venv)?;

    uv_snapshot!(
        packse_filters(&context),
        context
            .upgrade()
            .arg("--all-packages")
            .arg("--dry-run")
            .arg("--output-format")
            .arg("json")
            .arg("foo")
            .env_remove(EnvVars::UV_EXCLUDE_NEWER),
        @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "schema": {
        "version": "preview"
      },
      "dry_run": true,
      "packages": [
        "foo"
      ],
      "declarations": [
        {
          "member": "member-a",
          "package": "foo",
          "dependency_type": "production",
          "location": "project.dependencies",
          "original_requirement": "foo==1",
          "new_requirement": "foo==2.0.0",
          "resolved_versions": [
            "2.0.0"
          ],
          "status": "changed"
        },
        {
          "member": "member-b",
          "package": "foo",
          "dependency_type": "production",
          "location": "project.dependencies",
          "original_requirement": "foo==1",
          "new_requirement": "foo==2.0.0",
          "resolved_versions": [
            "2.0.0"
          ],
          "status": "changed"
        }
      ],
      "lock_changes": [
        {
          "action": "add",
          "package": "foo",
          "previous_versions": [],
          "current_versions": [
            {
              "version": "2.0.0"
            }
          ]
        }
      ]
    }

    ----- stderr -----
    warning: The `--output-format json` option is experimental and the schema may change without warning. Pass `--preview-features json-output` to disable this warning.
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 4 packages in [TIME]
    "#
    );

    assert_eq!(
        fs_err::read_to_string(context.temp_dir.child("pyproject.toml"))?,
        workspace_pyproject_toml
    );
    assert_eq!(
        fs_err::read_to_string(member_a.child("pyproject.toml"))?,
        member_a_pyproject_toml
    );
    assert_eq!(
        fs_err::read_to_string(member_b.child("pyproject.toml"))?,
        member_b_pyproject_toml
    );
    assert!(!context.temp_dir.child("uv.lock").exists());
    assert!(!context.temp_dir.child(".venv").exists());
    assert!(!member_a.child("uv.lock").exists());
    assert!(!member_a.child(".venv").exists());
    assert!(!member_b.child("uv.lock").exists());
    assert!(!member_b.child(".venv").exists());
    Ok(())
}

#[test]
fn upgrade_all_packages_skips_inapplicable_member_and_updates_other() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/fork-upgrade.toml");
    let workspace_pyproject_toml = format!(
        r#"
        [tool.uv.workspace]
        members = ["member-a", "member-b"]

        [[tool.uv.index]]
        url = "{}"
        default = true
    "#,
        server.index_url()
    );
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(&workspace_pyproject_toml)?;

    let member_a = context.temp_dir.child("member-a");
    member_a.create_dir_all()?;
    let member_a_pyproject_toml = r#"
        [project]
        name = "member-a"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["bar @ https://example.com/bar-1.0.0-py3-none-any.whl ; python_version < '3.12'"]
    "#;
    member_a
        .child("pyproject.toml")
        .write_str(member_a_pyproject_toml)?;

    let member_b = context.temp_dir.child("member-b");
    member_b.create_dir_all()?;
    let member_b_pyproject_toml = r#"
        [project]
        name = "member-b"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["bar<2"]
    "#;
    member_b
        .child("pyproject.toml")
        .write_str(member_b_pyproject_toml)?;
    fs_err::remove_dir_all(&context.venv)?;

    uv_snapshot!(
        packse_filters(&context),
        context
            .upgrade()
            .arg("--all-packages")
            .arg("bar")
            .env_remove(EnvVars::UV_EXCLUDE_NEWER),
        @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: Skipping dependency `bar` in package `member-a` `project.dependencies`: `bar @ https://example.com/bar-1.0.0-py3-none-any.whl ; python_full_version < '3.12'` (excluded by the project's environments or Python requirement)
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 3 packages in [TIME]
    Add bar v2.0.0
    Updated requirement in package `member-b`: `bar<2` -> `bar<3`
    "
    );

    assert_eq!(
        fs_err::read_to_string(member_a.child("pyproject.toml"))?,
        member_a_pyproject_toml
    );
    assert_eq!(
        fs_err::read_to_string(member_b.child("pyproject.toml"))?,
        member_b_pyproject_toml.replace("bar<2", "bar<3")
    );
    assert!(!context.temp_dir.child("uv.lock").exists());
    assert!(!context.temp_dir.child(".venv").exists());
    Ok(())
}

#[test]
fn upgrade_workspace_json_reports_skipped_requirement_without_lock_change() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/fork-upgrade.toml");
    let workspace_pyproject_toml = format!(
        r#"
        [tool.uv.workspace]
        members = ["member-a", "member-b"]

        [[tool.uv.index]]
        url = "{}"
        default = true
    "#,
        server.index_url()
    );
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(&workspace_pyproject_toml)?;

    let member_a = context.temp_dir.child("member-a");
    member_a.create_dir_all()?;
    let member_a_pyproject_toml = r#"
        [project]
        name = "member-a"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["foo @ https://user:secret@example.com/foo-1.0.0-py3-none-any.whl?token=secret ; python_version < '3.12'"]
    "#;
    member_a
        .child("pyproject.toml")
        .write_str(member_a_pyproject_toml)?;

    let member_b = context.temp_dir.child("member-b");
    member_b.create_dir_all()?;
    let member_b_pyproject_toml = r#"
        [project]
        name = "member-b"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["bar<2"]
    "#;
    member_b
        .child("pyproject.toml")
        .write_str(member_b_pyproject_toml)?;
    fs_err::remove_dir_all(&context.venv)?;

    uv_snapshot!(
        packse_filters(&context),
        context
            .upgrade()
            .arg("--all-packages")
            .arg("--dry-run")
            .arg("--output-format")
            .arg("json")
            .arg("foo")
            .arg("bar")
            .env_remove(EnvVars::UV_EXCLUDE_NEWER),
        @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "schema": {
        "version": "preview"
      },
      "dry_run": true,
      "packages": [
        "foo",
        "bar"
      ],
      "declarations": [
        {
          "member": "member-a",
          "package": "foo",
          "dependency_type": "production",
          "location": "project.dependencies",
          "original_requirement": "foo @ https://user:****@example.com/foo-1.0.0-py3-none-any.whl ; python_full_version < '3.12'",
          "status": "skipped",
          "reason": {
            "code": "environment_or_python_requirement",
            "message": "excluded by the project's environments or Python requirement"
          }
        },
        {
          "member": "member-b",
          "package": "bar",
          "dependency_type": "production",
          "location": "project.dependencies",
          "original_requirement": "bar<2",
          "new_requirement": "bar<3",
          "resolved_versions": [
            "2.0.0"
          ],
          "status": "changed"
        }
      ],
      "lock_changes": [
        {
          "action": "add",
          "package": "bar",
          "previous_versions": [],
          "current_versions": [
            {
              "version": "2.0.0"
            }
          ]
        }
      ]
    }

    ----- stderr -----
    warning: The `--output-format json` option is experimental and the schema may change without warning. Pass `--preview-features json-output` to disable this warning.
    warning: Skipping dependency `foo` in package `member-a` `project.dependencies`: `foo @ https://user:****@example.com/foo-1.0.0-py3-none-any.whl ; python_full_version < '3.12'` (excluded by the project's environments or Python requirement)
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 3 packages in [TIME]
    "#
    );

    assert_project_unchanged(&context, &workspace_pyproject_toml)?;
    assert_eq!(
        fs_err::read_to_string(member_a.child("pyproject.toml"))?,
        member_a_pyproject_toml
    );
    assert_eq!(
        fs_err::read_to_string(member_b.child("pyproject.toml"))?,
        member_b_pyproject_toml
    );
    Ok(())
}

#[test]
fn upgrade_json_reports_all_skipped_requirements() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);
    let pyproject_toml = r#"
        [project]
        name = "example"
        version = "0.1.0"
        dependencies = ["foo>=1 ; extra == 'does-not-exist'"]
    "#;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(pyproject_toml)?;

    uv_snapshot!(
        context.filters(),
        context
            .upgrade()
            .arg("--dry-run")
            .arg("--output-format")
            .arg("json")
            .arg("foo"),
        @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "schema": {
        "version": "preview"
      },
      "dry_run": true,
      "packages": [
        "foo"
      ],
      "declarations": [
        {
          "member": "example",
          "package": "foo",
          "dependency_type": "production",
          "location": "project.dependencies",
          "original_requirement": "foo>=1 ; extra == 'does-not-exist'",
          "status": "skipped",
          "reason": {
            "code": "undefined_extra",
            "message": "references an extra that the project does not provide"
          }
        }
      ],
      "lock_changes": []
    }

    ----- stderr -----
    warning: The `--output-format json` option is experimental and the schema may change without warning. Pass `--preview-features json-output` to disable this warning.
    warning: Skipping dependency `foo` in `project.dependencies`: `foo>=1 ; extra == 'does-not-exist'` (references an extra that the project does not provide)
    "#
    );

    assert_project_unchanged(&context, pyproject_toml)
}

#[test]
fn upgrade_unselected_workspace_member_constraint_remains_hard() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/fork-upgrade.toml");
    let workspace_pyproject_toml = format!(
        r#"
        [tool.uv.workspace]
        members = ["member-a", "member-b"]

        [[tool.uv.index]]
        url = "{}"
        default = true
    "#,
        server.index_url()
    );
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(&workspace_pyproject_toml)?;

    let member_a = context.temp_dir.child("member-a");
    member_a.create_dir_all()?;
    let member_a_pyproject_toml = r#"
        [project]
        name = "member-a"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["foo==1"]
    "#;
    member_a
        .child("pyproject.toml")
        .write_str(member_a_pyproject_toml)?;

    let member_b = context.temp_dir.child("member-b");
    member_b.create_dir_all()?;
    let member_b_pyproject_toml = r#"
        [project]
        name = "member-b"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["bar<2"]
    "#;
    member_b
        .child("pyproject.toml")
        .write_str(member_b_pyproject_toml)?;
    fs_err::remove_dir_all(&context.venv)?;

    uv_snapshot!(
        packse_filters(&context),
        context
            .upgrade()
            .current_dir(&member_a)
            .arg("foo")
            .env_remove(EnvVars::UV_EXCLUDE_NEWER),
        @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
      × No solution found when resolving dependencies for split (markers: sys_platform != 'linux'):
      ╰─▶ Because only the following versions of foo are available:
              foo==1.0.0
              foo==2.0.0
          and foo==1.0.0 depends on bar{sys_platform != 'linux'}==2, we can conclude that foo<2.0.0 depends on bar==2.
          And because foo==2.0.0 depends on bar==2, we can conclude that all versions of foo depend on bar==2.
          And because member-b depends on bar<2 and member-a depends on foo, we can conclude that member-a and member-b are incompatible.
          And because your workspace requires member-a and member-b, we can conclude that your workspace's requirements are unsatisfiable.

    hint: The resolution failed for an environment that is not the current one, consider limiting the environments with `tool.uv.environments`.
    "
    );

    assert_eq!(
        fs_err::read_to_string(member_a.child("pyproject.toml"))?,
        member_a_pyproject_toml
    );
    assert_eq!(
        fs_err::read_to_string(member_b.child("pyproject.toml"))?,
        member_b_pyproject_toml
    );
    assert!(!context.temp_dir.child("uv.lock").exists());
    assert!(!context.temp_dir.child(".venv").exists());
    Ok(())
}

#[test]
fn upgrade_ignores_unselected_member_non_registry_source() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/fork-upgrade.toml");
    let workspace_pyproject_toml = format!(
        r#"
        [tool.uv.workspace]
        members = ["member-a", "member-b"]

        [[tool.uv.index]]
        url = "{}"
        default = true
    "#,
        server.index_url()
    );
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(&workspace_pyproject_toml)?;

    let member_a = context.temp_dir.child("member-a");
    member_a.create_dir_all()?;
    let member_a_pyproject_toml = r#"
        [project]
        name = "member-a"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["bar<2"]
    "#;
    member_a
        .child("pyproject.toml")
        .write_str(member_a_pyproject_toml)?;

    let member_b = context.temp_dir.child("member-b");
    member_b.create_dir_all()?;
    let member_b_pyproject_toml = r#"
        [project]
        name = "member-b"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [project.optional-dependencies]
        test = ["bar<2"]

        [tool.uv.sources]
        bar = { path = "vendor/bar", extra = "test" }
    "#;
    member_b
        .child("pyproject.toml")
        .write_str(member_b_pyproject_toml)?;
    member_b.child("vendor").child("bar").create_dir_all()?;
    member_b
        .child("vendor")
        .child("bar")
        .child("pyproject.toml")
        .write_str(
            r#"
            [project]
            name = "bar"
            version = "1.0.0"
            "#,
        )?;
    fs_err::remove_dir_all(&context.venv)?;

    uv_snapshot!(
        packse_filters(&context),
        context
            .upgrade()
            .arg("--package")
            .arg("member-a")
            .arg("bar")
            .env_remove(EnvVars::UV_EXCLUDE_NEWER),
        @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 3 packages in [TIME]
    Add bar v1.0.0
    "
    );

    assert_eq!(
        fs_err::read_to_string(member_a.child("pyproject.toml"))?,
        member_a_pyproject_toml
    );
    assert_eq!(
        fs_err::read_to_string(member_b.child("pyproject.toml"))?,
        member_b_pyproject_toml
    );
    assert!(!context.temp_dir.child("uv.lock").exists());
    assert!(!context.temp_dir.child(".venv").exists());
    Ok(())
}

#[test]
fn upgrade_rejects_selected_member_non_registry_source() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);
    let workspace_pyproject_toml = r#"
        [tool.uv.workspace]
        members = ["member-a", "member-b"]
    "#;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(workspace_pyproject_toml)?;

    let member_a = context.temp_dir.child("member-a");
    member_a.create_dir_all()?;
    let member_a_pyproject_toml = r#"
        [project]
        name = "member-a"
        version = "0.1.0"
        dependencies = ["requests>=2"]

        [tool.uv.sources]
        requests = { path = "vendor/requests" }
    "#;
    member_a
        .child("pyproject.toml")
        .write_str(member_a_pyproject_toml)?;

    let member_b = context.temp_dir.child("member-b");
    member_b.create_dir_all()?;
    let member_b_pyproject_toml = r#"
        [project]
        name = "member-b"
        version = "0.1.0"
        dependencies = []
    "#;
    member_b
        .child("pyproject.toml")
        .write_str(member_b_pyproject_toml)?;

    uv_snapshot!(
        context.filters(),
        context
            .upgrade()
            .arg("--package")
            .arg("member-a")
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
        fs_err::read_to_string(member_a.child("pyproject.toml"))?,
        member_a_pyproject_toml
    );
    assert_eq!(
        fs_err::read_to_string(member_b.child("pyproject.toml"))?,
        member_b_pyproject_toml
    );
    assert!(!context.temp_dir.child("uv.lock").exists());
    assert!(!context.temp_dir.child(".venv").exists());
    Ok(())
}

#[test]
fn upgrade_stale_unselected_lockfile_entry_is_preference() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/fork-upgrade.toml");
    let workspace_pyproject_toml = format!(
        r#"
        [tool.uv.workspace]
        members = ["member-a", "member-b"]

        [[tool.uv.index]]
        url = "{}"
        default = true
    "#,
        server.index_url()
    );
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(&workspace_pyproject_toml)?;

    let member_a = context.temp_dir.child("member-a");
    member_a.create_dir_all()?;
    let member_a_initial_pyproject_toml = r#"
        [project]
        name = "member-a"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#;
    member_a
        .child("pyproject.toml")
        .write_str(member_a_initial_pyproject_toml)?;

    let member_b = context.temp_dir.child("member-b");
    member_b.create_dir_all()?;
    let member_b_initial_pyproject_toml = r#"
        [project]
        name = "member-b"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["bar<2"]
    "#;
    member_b
        .child("pyproject.toml")
        .write_str(member_b_initial_pyproject_toml)?;
    fs_err::remove_dir_all(&context.venv)?;

    uv_snapshot!(
        packse_filters(&context),
        context.lock().env_remove(EnvVars::UV_EXCLUDE_NEWER),
        @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 3 packages in [TIME]
    "
    );
    let lock = fs_err::read(context.temp_dir.child("uv.lock"))?;

    let member_a_pyproject_toml = member_a_initial_pyproject_toml
        .replace("dependencies = []", r#"dependencies = ["foo==1"]"#);
    member_a
        .child("pyproject.toml")
        .write_str(&member_a_pyproject_toml)?;
    let member_b_pyproject_toml =
        member_b_initial_pyproject_toml.replace(r#"dependencies = ["bar<2"]"#, "dependencies = []");
    member_b
        .child("pyproject.toml")
        .write_str(&member_b_pyproject_toml)?;

    uv_snapshot!(
        packse_filters(&context),
        context
            .upgrade()
            .arg("--package")
            .arg("member-a")
            .arg("foo")
            .env_remove(EnvVars::UV_EXCLUDE_NEWER),
        @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 4 packages in [TIME]
    Add foo v2.0.0
    Updated requirement: `foo==1` -> `foo==2.0.0`
    "
    );

    assert_eq!(
        fs_err::read_to_string(member_a.child("pyproject.toml"))?,
        member_a_pyproject_toml.replace("foo==1", "foo==2.0.0")
    );
    assert_eq!(
        fs_err::read_to_string(member_b.child("pyproject.toml"))?,
        member_b_pyproject_toml
    );
    assert_eq!(fs_err::read(context.temp_dir.child("uv.lock"))?, lock);
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
    error: `uv upgrade` requires a project with a `[project]` table; use `--package` or `--all-packages` to select workspace members
    "
    );

    assert_project_unchanged(&context, pyproject_toml)
}

#[test]
fn upgrade_updates_workspace_root_project_by_default() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("fork/fork-upgrade.toml");
    let pyproject_toml = format!(
        r#"
        [project]
        name = "example"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["foo==1"]

        [tool.uv.workspace]
        members = ["member"]

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

    let member = context.temp_dir.child("member");
    member.create_dir_all()?;
    let member_pyproject_toml = r#"
        [project]
        name = "member"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#;
    member
        .child("pyproject.toml")
        .write_str(member_pyproject_toml)?;
    fs_err::remove_dir_all(&context.venv)?;

    uv_snapshot!(
        packse_filters(&context),
        context
            .upgrade()
            .arg("foo")
            .env_remove(EnvVars::UV_EXCLUDE_NEWER),
        @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 4 packages in [TIME]
    Add foo v2.0.0
    Updated requirement: `foo==1` -> `foo==2.0.0`
    "
    );

    assert_eq!(
        fs_err::read_to_string(context.temp_dir.child("pyproject.toml"))?,
        pyproject_toml.replace("foo==1", "foo==2.0.0")
    );
    assert_eq!(
        fs_err::read_to_string(member.child("pyproject.toml"))?,
        member_pyproject_toml
    );
    assert!(!context.temp_dir.child("uv.lock").exists());
    assert!(!context.temp_dir.child(".venv").exists());
    Ok(())
}

#[test]
fn upgrade_reports_missing_workspace_package() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);
    let pyproject_toml = r#"
        [tool.uv.workspace]
        members = ["member"]
    "#;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(pyproject_toml)?;
    let member = context.temp_dir.child("member");
    member.create_dir_all()?;
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
        context
            .upgrade()
            .arg("--package")
            .arg("missing")
            .arg("requests"),
        @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: The workspace does not have a member missing: [TEMP_DIR]/
    "
    );

    assert_project_unchanged(&context, pyproject_toml)
}
