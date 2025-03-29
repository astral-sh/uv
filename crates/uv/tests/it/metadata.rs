use anyhow::{Ok, Result};
use assert_fs::prelude::*;
use insta::assert_snapshot;

use crate::common::uv_snapshot;
use crate::common::TestContext;

// Print the version
#[test]
fn version_get() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "myproject"
        version = "1.10.31"
        requires-python = ">=3.12"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.metadata_version(), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    myproject 1.10.31

    ----- stderr -----
    ");

    let pyproject = fs_err::read_to_string(&pyproject_toml)?;
    assert_snapshot!(
        pyproject,
    @r#"
    [project]
    name = "myproject"
    version = "1.10.31"
    requires-python = ">=3.12"
    "#
    );
    Ok(())
}

// Print the version (--short)
#[test]
fn version_get_short() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "myproject"
        version = "1.10.31"
        requires-python = ">=3.12"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.metadata_version()
        .arg("--short"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    1.10.31

    ----- stderr -----
    ");

    let pyproject = fs_err::read_to_string(&pyproject_toml)?;
    assert_snapshot!(
        pyproject,
    @r#"
    [project]
    name = "myproject"
    version = "1.10.31"
    requires-python = ">=3.12"
    "#
    );

    Ok(())
}

// Set the version
#[test]
fn version_set_value() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
[project]
name = "myproject"
version = "1.10.31"
requires-python = ">=3.12"
"#,
    )?;

    uv_snapshot!(context.filters(), context.metadata_version()
        .arg("1.1.1"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    myproject 1.10.31 => 1.1.1

    ----- stderr -----
    ");

    let pyproject = fs_err::read_to_string(&pyproject_toml)?;
    assert_snapshot!(
        pyproject,
    @r###"
    [project]
    name = "myproject"
    version = "1.1.1"
    requires-python = ">=3.12"
    "###
    );

    Ok(())
}

// Set the version (--short)
#[test]
fn version_set_value_short() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
[project]
name = "myproject"
version = "1.10.31"
requires-python = ">=3.12"
"#,
    )?;

    uv_snapshot!(context.filters(), context.metadata_version()
        .arg("1.1.1")
        .arg("--short"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    1.1.1

    ----- stderr -----
    ");

    let pyproject = fs_err::read_to_string(&pyproject_toml)?;
    assert_snapshot!(
        pyproject,
    @r###"
    [project]
    name = "myproject"
    version = "1.1.1"
    requires-python = ">=3.12"
    "###
    );

    Ok(())
}

// Bump patch version
#[test]
fn version_bump_patch() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
[project]
name = "myproject"
version = "1.10.31"
requires-python = ">=3.12"
"#,
    )?;

    uv_snapshot!(context.filters(), context.metadata_version()
        .arg("--bump").arg("patch"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    myproject 1.10.31 => 1.10.32

    ----- stderr -----
    ");

    let pyproject = fs_err::read_to_string(&pyproject_toml)?;
    assert_snapshot!(
        pyproject,
    @r#"
    [project]
    name = "myproject"
    version = "1.10.32"
    requires-python = ">=3.12"
    "#
    );
    Ok(())
}

// Bump patch version (--short)
#[test]
fn version_bump_patch_short() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
[project]
name = "myproject"
version = "1.10.31"
requires-python = ">=3.12"
"#,
    )?;

    uv_snapshot!(context.filters(), context.metadata_version()
        .arg("--bump").arg("patch")
        .arg("--short"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    1.10.32

    ----- stderr -----
    ");

    let pyproject = fs_err::read_to_string(&pyproject_toml)?;
    assert_snapshot!(
        pyproject,
    @r#"
    [project]
    name = "myproject"
    version = "1.10.32"
    requires-python = ">=3.12"
    "#
    );
    Ok(())
}

// Bump minor version
#[test]
fn version_bump_minor() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
[project]
name = "myproject"
version = "1.10.31"
requires-python = ">=3.12"
"#,
    )?;

    uv_snapshot!(context.filters(), context.metadata_version()
        .arg("--bump").arg("minor"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    myproject 1.10.31 => 1.11.0

    ----- stderr -----
    ");

    let pyproject = fs_err::read_to_string(&pyproject_toml)?;
    assert_snapshot!(
        pyproject,
    @r#"
    [project]
    name = "myproject"
    version = "1.11.0"
    requires-python = ">=3.12"
    "#
    );
    Ok(())
}

// bump major version
#[test]
fn version_major_version() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
[project]
name = "myproject"
version = "1.10.31"
requires-python = ">=3.12"
"#,
    )?;

    uv_snapshot!(context.filters(), context.metadata_version()
        .arg("--bump").arg("major"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    myproject 1.10.31 => 2.0.0

    ----- stderr -----
    ");

    let pyproject = fs_err::read_to_string(&pyproject_toml)?;
    assert_snapshot!(
        pyproject,
    @r#"
    [project]
    name = "myproject"
    version = "2.0.0"
    requires-python = ">=3.12"
    "#
    );
    Ok(())
}

// Bump patch but the input version is missing a component
#[test]
fn version_patch_uncompleted() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
[project]
name = "myproject"
version = "0.1"
requires-python = ">=3.12"
"#,
    )?;

    uv_snapshot!(context.filters(), context.metadata_version()
        .arg("--bump").arg("patch"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    myproject 0.1 => 0.1.1

    ----- stderr -----
    ");

    let pyproject = fs_err::read_to_string(&pyproject_toml)?;
    assert_snapshot!(
        pyproject,
    @r#"
    [project]
    name = "myproject"
    version = "0.1.1"
    requires-python = ">=3.12"
    "#
    );
    Ok(())
}

// Bump minor but the input version is missing a component
#[test]
fn version_minor_uncompleted() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
[project]
name = "myproject"
version = "0.1"
requires-python = ">=3.12"
"#,
    )?;

    uv_snapshot!(context.filters(), context.metadata_version()
        .arg("--bump").arg("minor"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    myproject 0.1 => 0.2

    ----- stderr -----
    ");

    let pyproject = fs_err::read_to_string(&pyproject_toml)?;
    assert_snapshot!(
        pyproject,
    @r#"
    [project]
    name = "myproject"
    version = "0.2"
    requires-python = ">=3.12"
    "#
    );
    Ok(())
}

// Bump major but the input version is missing a component
#[test]
fn version_major_uncompleted() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
[project]
name = "myproject"
version = "0.1"
requires-python = ">=3.12"
"#,
    )?;

    uv_snapshot!(context.filters(), context.metadata_version()
        .arg("--bump").arg("major"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    myproject 0.1 => 1.0

    ----- stderr -----
    ");

    let pyproject = fs_err::read_to_string(&pyproject_toml)?;
    assert_snapshot!(
        pyproject,
    @r#"
    [project]
    name = "myproject"
    version = "1.0"
    requires-python = ">=3.12"
    "#
    );
    Ok(())
}

// Bump major but the input version is .dev
#[test]
fn version_major_dev() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
[project]
name = "myproject"
version = "1.10.31.dev10"
requires-python = ">=3.12"
"#,
    )?;

    uv_snapshot!(context.filters(), context.metadata_version()
        .arg("--bump").arg("major"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    myproject 1.10.31.dev10 => 2.0.0

    ----- stderr -----
    warning: dev or post versions will be bumped to release versions
    ");

    let pyproject = fs_err::read_to_string(&pyproject_toml)?;
    assert_snapshot!(
        pyproject,
    @r#"
    [project]
    name = "myproject"
    version = "2.0.0"
    requires-python = ">=3.12"
    "#
    );
    Ok(())
}

// Bump major but the input version is .post
#[test]
fn version_major_post() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
[project]
name = "myproject"
version = "1.10.31.post10"
requires-python = ">=3.12"
"#,
    )?;

    uv_snapshot!(context.filters(), context.metadata_version()
        .arg("--bump").arg("major"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    myproject 1.10.31.post10 => 2.0.0

    ----- stderr -----
    warning: dev or post versions will be bumped to release versions
    ");

    let pyproject = fs_err::read_to_string(&pyproject_toml)?;
    assert_snapshot!(
        pyproject,
    @r#"
    [project]
    name = "myproject"
    version = "2.0.0"
    requires-python = ">=3.12"
    "#
    );
    Ok(())
}

// Set version --dry-run
#[test]
fn version_set_dry() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
[project]
name = "myproject"
version = "1.10.31"
requires-python = ">=3.12"
"#,
    )?;

    uv_snapshot!(context.filters(), context.metadata_version()
        .arg("1.2.3")
        .arg("--dry-run"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    myproject 1.10.31 => 1.2.3

    ----- stderr -----
    ");

    let pyproject = fs_err::read_to_string(&pyproject_toml)?;
    assert_snapshot!(
        pyproject,
    @r#"
    [project]
    name = "myproject"
    version = "1.10.31"
    requires-python = ">=3.12"
    "#
    );
    Ok(())
}

// Bump version --dry-run
#[test]
fn version_major_dry() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
[project]
name = "myproject"
version = "1.10.31"
requires-python = ">=3.12"
"#,
    )?;

    uv_snapshot!(context.filters(), context.metadata_version()
        .arg("--bump").arg("major")
        .arg("--dry-run"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    myproject 1.10.31 => 2.0.0

    ----- stderr -----
    ");

    let pyproject = fs_err::read_to_string(&pyproject_toml)?;
    assert_snapshot!(
        pyproject,
    @r#"
    [project]
    name = "myproject"
    version = "1.10.31"
    requires-python = ">=3.12"
    "#
    );
    Ok(())
}

// Set version invalid
#[test]
fn version_set_invalid() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
[project]
name = "myproject"
version = "1.10.31"
requires-python = ">=3.12"
"#,
    )?;

    uv_snapshot!(context.filters(), context.metadata_version()
        .arg("abcd"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: expected version to start with a number, but no leading ASCII digits were found
    ");

    let pyproject = fs_err::read_to_string(&pyproject_toml)?;
    assert_snapshot!(
        pyproject,
    @r#"
    [project]
    name = "myproject"
    version = "1.10.31"
    requires-python = ">=3.12"
    "#
    );
    Ok(())
}

// forget --bump but pass a valid bump name
#[test]
fn version_missing_bump() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
[project]
name = "myproject"
version = "1.10.31"
requires-python = ">=3.12"
"#,
    )?;

    uv_snapshot!(context.filters(), context.metadata_version()
        .arg("minor"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: expected version to start with a number, but no leading ASCII digits were found
    ");

    let pyproject = fs_err::read_to_string(&pyproject_toml)?;
    assert_snapshot!(
        pyproject,
    @r#"
    [project]
    name = "myproject"
    version = "1.10.31"
    requires-python = ">=3.12"
    "#
    );
    Ok(())
}
