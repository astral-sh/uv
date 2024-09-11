use anyhow::{Ok, Result};
use assert_fs::prelude::*;

use common::{uv_snapshot, TestContext};
use insta::assert_snapshot;

mod common;

#[test]
fn bump_no_args_prints_project_version() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.515.0"
        requires-python = ">=3.12"
        "#,
    )?;
    uv_snapshot!(context.filters(), context.bump(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Current version: 0.515.0

    ----- stderr -----
    "###);
    Ok(())
}

#[test]
fn bump_to_raw_version_string() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
[project]
name = "project"
version = "0.515.0"
requires-python = ">=3.12"
"#,
    )?;
    uv_snapshot!(context.filters(), context.bump().arg("--raw=1.1.1"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Bumped from 0.515.0  to: 1.1.1

    ----- stderr -----
    "###);
    let pyproject = fs_err::read_to_string(&pyproject_toml)?;
    assert_snapshot!(
        pyproject,
    @r###"
    [project]
    name = "project"
    version = "1.1.1"
    requires-python = ">=3.12"
    "###
    );

    Ok(())
}

#[test]
fn bump_patch_version() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
[project]
name = "project"
version = "0.515.0"
requires-python = ">=3.12"
"#,
    )?;
    uv_snapshot!(context.filters(), context.bump().arg("patch"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Bumped from 0.515.0  to: 0.515.1

    ----- stderr -----
    "###);
    let pyproject = fs_err::read_to_string(&pyproject_toml)?;
    assert_snapshot!(
        pyproject,
    @r###"
    [project]
    name = "project"
    version = "0.515.1"
    requires-python = ">=3.12"
    "###
    );
    Ok(())
}

#[test]
fn bump_minor_version() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
[project]
name = "project"
version = "0.515.3"
requires-python = ">=3.12"
"#,
    )?;
    uv_snapshot!(context.filters(), context.bump().arg("minor"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Bumped from 0.515.3  to: 0.516.0

    ----- stderr -----
    "###);
    let pyproject = fs_err::read_to_string(&pyproject_toml)?;
    assert_snapshot!(
        pyproject,
    @r###"
    [project]
    name = "project"
    version = "0.516.0"
    requires-python = ">=3.12"
    "###
    );
    Ok(())
}

#[test]
fn bump_major_version() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
[project]
name = "project"
version = "0.515.0"
requires-python = ">=3.12"
"#,
    )?;
    uv_snapshot!(context.filters(), context.bump().arg("major"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Bumped from 0.515.0  to: 1.0.0

    ----- stderr -----
    "###);
    let pyproject = fs_err::read_to_string(&pyproject_toml)?;
    assert_snapshot!(
        pyproject,
    @r###"
    [project]
    name = "project"
    version = "1.0.0"
    requires-python = ">=3.12"
    "###
    );
    Ok(())
}
