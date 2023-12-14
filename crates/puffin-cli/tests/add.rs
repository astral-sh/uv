use std::process::Command;

use anyhow::Result;
use assert_fs::prelude::*;
use insta_cmd::{assert_cmd_snapshot, get_cargo_bin};

use common::BIN_NAME;

mod common;

#[test]
fn missing_pyproject_toml() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let pyproject_toml = temp_dir.child("pyproject.toml");

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .arg("add")
        .arg("flask")
        .current_dir(&temp_dir), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    puffin::add::workspace_not_found

      × Could not find a `pyproject.toml` file in the current directory or any of
      │ its parents
    "###);

    pyproject_toml.assert(predicates::path::missing());

    Ok(())
}

#[test]
fn missing_project_table() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let pyproject_toml = temp_dir.child("pyproject.toml");
    pyproject_toml.touch()?;

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .arg("add")
        .arg("flask")
        .current_dir(&temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    "###);

    pyproject_toml.assert(
        r#"[project]
dependencies = [
    "flask",
]
"#,
    );

    Ok(())
}

#[test]
fn missing_dependencies_array() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let pyproject_toml = temp_dir.child("pyproject.toml");
    pyproject_toml.touch()?;
    pyproject_toml.write_str(
        r#"[project]
name = "project"
"#,
    )?;

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .arg("add")
        .arg("flask")
        .current_dir(&temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    "###);

    pyproject_toml.assert(
        r#"[project]
name = "project"
dependencies = [
    "flask",
]
"#,
    );

    Ok(())
}

#[test]
fn replace_dependency() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let pyproject_toml = temp_dir.child("pyproject.toml");
    pyproject_toml.touch()?;
    pyproject_toml.write_str(
        r#"[project]
name = "project"
dependencies = [
    "flask==1.0.0",
]
"#,
    )?;

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .arg("add")
        .arg("flask==2.0.0")
        .current_dir(&temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    "###);

    pyproject_toml.assert(
        r#"[project]
name = "project"
dependencies = [
    "flask==2.0.0",
]
"#,
    );

    Ok(())
}

#[test]
fn reformat_array() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let pyproject_toml = temp_dir.child("pyproject.toml");
    pyproject_toml.touch()?;
    pyproject_toml.write_str(
        r#"[project]
name = "project"
dependencies = ["flask==1.0.0"]
"#,
    )?;

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .arg("add")
        .arg("requests")
        .current_dir(&temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    "###);

    pyproject_toml.assert(
        r#"[project]
name = "project"
dependencies = [
    "flask==1.0.0",
    "requests",
]
"#,
    );

    Ok(())
}
