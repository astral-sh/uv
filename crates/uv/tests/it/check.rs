use anyhow::Result;
use assert_cmd::prelude::*;
use assert_fs::prelude::*;
use indoc::indoc;

use uv_test::uv_snapshot;

#[test]
fn check_project() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    let main_py = context.temp_dir.child("main.py");
    main_py.write_str(indoc! {r#"
        x: int = 1
    "#})?;

    uv_snapshot!(context.filters(), context.check(), @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    warning: `uv check` is experimental and may change without warning. Pass `--preview-features check` to disable this warning.
    ");

    Ok(())
}

#[test]
fn check_missing_pyproject_toml() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);

    let main_py = context.temp_dir.child("main.py");
    main_py.write_str(indoc! {r#"
        x: int = 1
    "#})?;

    uv_snapshot!(context.filters(), context.check(), @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    warning: `uv check` is experimental and may change without warning. Pass `--preview-features check` to disable this warning.
    ");

    Ok(())
}

#[test]
fn check_no_project() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);

    let main_py = context.temp_dir.child("main.py");
    main_py.write_str(indoc! {r#"
        x: int = 1
    "#})?;

    uv_snapshot!(context.filters(), context.check().arg("--no-project"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    warning: `uv check` is experimental and may change without warning. Pass `--preview-features check` to disable this warning.
    ");

    Ok(())
}

#[test]
fn check_with_declared_dependency() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]
    "#})?;

    // Sync to install the dependency into the virtual environment.
    context.sync().assert().success();

    let main_py = context.temp_dir.child("main.py");
    main_py.write_str(indoc! {r#"
        import iniconfig
    "#})?;

    // ty should resolve the import via the synced virtual environment.
    uv_snapshot!(context.filters(), context.check(), @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    warning: `uv check` is experimental and may change without warning. Pass `--preview-features check` to disable this warning.
    ");

    Ok(())
}

#[test]
fn check_with_undeclared_dependency() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    // Sync to create the virtual environment (no dependencies installed).
    context.sync().assert().success();

    let main_py = context.temp_dir.child("main.py");
    main_py.write_str(indoc! {r#"
        import iniconfig
    "#})?;

    // ty should report a diagnostic for the unresolvable import.
    uv_snapshot!(context.filters(), context.check(), @"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-import]: Cannot resolve imported module `iniconfig`
     --> main.py:1:8
      |
    1 | import iniconfig
      |        ^^^^^^^^^
      |
    info: Searched in the following paths during module resolution:
    info:   1. [TEMP_DIR]/ (first-party code)
    info:   2. vendored://stdlib (stdlib typeshed stubs vendored by ty)
    info:   3. [SITE_PACKAGES]/ (site-packages)
    info:   4. [VENV]/lib64/python3.12/site-packages (site-packages)
    info: make sure your Python environment is properly configured: https://docs.astral.sh/ty/modules/#python-environment
    info: rule `unresolved-import` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    warning: `uv check` is experimental and may change without warning. Pass `--preview-features check` to disable this warning.
    ");

    Ok(())
}
