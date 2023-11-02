use std::process::Command;

use anyhow::Result;
use assert_fs::prelude::*;
use insta_cmd::{assert_cmd_snapshot, get_cargo_bin};

use common::BIN_NAME;

mod common;

#[test]
fn no_arguments() -> Result<()> {
    let tempdir = assert_fs::TempDir::new()?;

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .arg("pip-uninstall")
        .current_dir(&tempdir));

    Ok(())
}

#[test]
fn invalid_requirement() -> Result<()> {
    let tempdir = assert_fs::TempDir::new()?;

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .arg("pip-uninstall")
        .arg("flask==1.0.x")
        .current_dir(&tempdir));

    Ok(())
}

#[test]
fn missing_requirements_txt() -> Result<()> {
    let tempdir = assert_fs::TempDir::new()?;

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .arg("pip-uninstall")
        .arg("-r")
        .arg("requirements.txt")
        .current_dir(&tempdir));

    Ok(())
}

#[test]
fn invalid_requirements_txt_requirement() -> Result<()> {
    let tempdir = assert_fs::TempDir::new()?;
    let requirements_txt = tempdir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("flask==1.0.x")?;

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .arg("pip-uninstall")
        .arg("-r")
        .arg("requirements.txt")
        .current_dir(&tempdir));

    Ok(())
}

#[test]
fn missing_pyproject_toml() -> Result<()> {
    let tempdir = assert_fs::TempDir::new()?;

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .arg("pip-uninstall")
        .arg("-r")
        .arg("pyproject.toml")
        .current_dir(&tempdir));

    Ok(())
}

#[test]
fn invalid_pyproject_toml_syntax() -> Result<()> {
    let tempdir = assert_fs::TempDir::new()?;
    let pyproject_toml = tempdir.child("pyproject.toml");
    pyproject_toml.touch()?;
    pyproject_toml.write_str("123 - 456")?;

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .arg("pip-uninstall")
        .arg("-r")
        .arg("pyproject.toml")
        .current_dir(&tempdir));

    Ok(())
}

#[test]
fn invalid_pyproject_toml_schema() -> Result<()> {
    let tempdir = assert_fs::TempDir::new()?;
    let pyproject_toml = tempdir.child("pyproject.toml");
    pyproject_toml.touch()?;
    pyproject_toml.write_str("[project]")?;

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .arg("pip-uninstall")
        .arg("-r")
        .arg("pyproject.toml")
        .current_dir(&tempdir));

    Ok(())
}

#[test]
fn invalid_pyproject_toml_requirement() -> Result<()> {
    let tempdir = assert_fs::TempDir::new()?;
    let pyproject_toml = tempdir.child("pyproject.toml");
    pyproject_toml.touch()?;
    pyproject_toml.write_str(
        r#"[project]
name = "project"
dependencies = ["flask==1.0.x"]
"#,
    )?;

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .arg("pip-uninstall")
        .arg("-r")
        .arg("pyproject.toml")
        .current_dir(&tempdir));

    Ok(())
}
