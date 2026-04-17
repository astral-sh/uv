use anyhow::Result;
use assert_fs::prelude::*;
use indoc::indoc;
use insta::assert_snapshot;

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

    ----- stderr -----
    warning: `uv check` is experimental and may change without warning. Pass `--preview-features check` to disable this warning.
    ");

    Ok(())
}
