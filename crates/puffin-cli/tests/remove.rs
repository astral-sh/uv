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
        .arg("remove")
        .arg("flask")
        .current_dir(&temp_dir), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    puffin::remove::workspace_not_found

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
        .arg("remove")
        .arg("flask")
        .current_dir(&temp_dir), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    puffin::remove::parse

      × Failed to remove `flask` from `pyproject.toml`
      ╰─▶ no `[project]` table found in `pyproject.toml`
    "###);

    pyproject_toml.assert(predicates::str::is_empty());

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
        .arg("remove")
        .arg("flask")
        .current_dir(&temp_dir), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    puffin::remove::parse

      × Failed to remove `flask` from `pyproject.toml`
      ╰─▶ no `[project.dependencies]` array found in `pyproject.toml`
    "###);

    pyproject_toml.assert(
        r#"[project]
name = "project"
"#,
    );

    Ok(())
}

#[test]
fn missing_dependency() -> Result<()> {
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
        .arg("remove")
        .arg("requests")
        .current_dir(&temp_dir), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    puffin::remove::parse

      × Failed to remove `requests` from `pyproject.toml`
      ╰─▶ unable to find package: `requests`
    "###);

    pyproject_toml.assert(
        r#"[project]
name = "project"
dependencies = [
    "flask==1.0.0",
]
"#,
    );

    Ok(())
}

#[test]
fn remove_dependency() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let pyproject_toml = temp_dir.child("pyproject.toml");
    pyproject_toml.touch()?;
    pyproject_toml.write_str(
        r#"[project]
name = "project"
dependencies = [
    "flask==1.0.0",
    "requests",
]
"#,
    )?;

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .arg("remove")
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
    "requests",
]
"#,
    );

    Ok(())
}

#[test]
fn empty_array() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let pyproject_toml = temp_dir.child("pyproject.toml");
    pyproject_toml.touch()?;
    pyproject_toml.write_str(
        r#"[project]
name = "project"
dependencies = [
    "requests",
]
"#,
    )?;

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .arg("remove")
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
dependencies = []
"#,
    );

    Ok(())
}

#[test]
fn normalize_name() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let pyproject_toml = temp_dir.child("pyproject.toml");
    pyproject_toml.touch()?;
    pyproject_toml.write_str(
        r#"[project]
name = "project"
dependencies = [
    "flask==1.0.0",
    "requests",
]
"#,
    )?;

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .arg("remove")
        .arg("Flask")
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
    "requests",
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
dependencies = ["flask==1.0.0", "requests"]
"#,
    )?;

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .arg("remove")
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
]
"#,
    );

    Ok(())
}
