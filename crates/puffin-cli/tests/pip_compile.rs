#![cfg(all(feature = "python", feature = "pypi"))]

use std::process::Command;

use anyhow::Result;
use assert_cmd::prelude::*;
use assert_fs::prelude::*;
use insta_cmd::_macro_support::insta;
use insta_cmd::{assert_cmd_snapshot, get_cargo_bin};

const BIN_NAME: &str = "puffin";

#[test]
fn missing_requirements_in() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let requirements_in = temp_dir.child("requirements.in");

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .arg("pip-compile")
        .arg("requirements.in")
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .current_dir(&temp_dir));

    requirements_in.assert(predicates::path::missing());

    Ok(())
}

#[test]
fn missing_venv() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .arg("pip-compile")
        .arg("requirements.in")
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .env("VIRTUAL_ENV", venv.as_os_str())
        .current_dir(&temp_dir));

    venv.assert(predicates::path::missing());

    Ok(())
}

/// Resolve a specific version of Django from a `requirements.in` file.
#[test]
fn compile_requirements_in() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .current_dir(&temp_dir)
        .assert()
        .success();
    venv.assert(predicates::path::is_dir());

    let requirements_in = temp_dir.child("requirements.in");
    requirements_in.touch()?;
    requirements_in.write_str("django==5.0b1")?;

    insta::with_settings!({
        filters => vec![
            (r"\d+(ms|s)", "[TIME]"),
            (r"#    .* pip-compile", "#    [BIN_PATH] pip-compile"),
            (r"--cache-dir .*", "--cache-dir [CACHE_DIR]"),
        ]
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-compile")
            .arg("requirements.in")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir));
    });

    Ok(())
}

/// Resolve a specific version of Django from a `pyproject.toml` file.
#[test]
fn compile_pyproject_toml() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .current_dir(&temp_dir)
        .assert()
        .success();
    venv.assert(predicates::path::is_dir());

    let pyproject_toml = temp_dir.child("pyproject.toml");
    pyproject_toml.touch()?;
    pyproject_toml.write_str(
        r#"[build-system]
requires = ["setuptools", "wheel"]

[project]
name = "project"
dependencies = [
    "django==5.0b1",
]
"#,
    )?;

    insta::with_settings!({
        filters => vec![
            (r"\d+(ms|s)", "[TIME]"),
            (r"#    .* pip-compile", "#    [BIN_PATH] pip-compile"),
            (r"--cache-dir .*", "--cache-dir [CACHE_DIR]"),
        ]
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-compile")
            .arg("pyproject.toml")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir));
    });

    Ok(())
}

/// Resolve a package from a `requirements.in` file, with a `constraints.txt` file.
#[test]
fn compile_constraints_txt() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .current_dir(&temp_dir)
        .assert()
        .success();
    venv.assert(predicates::path::is_dir());

    let requirements_in = temp_dir.child("requirements.in");
    requirements_in.touch()?;
    requirements_in.write_str("django==5.0b1")?;

    let constraints_txt = temp_dir.child("constraints.txt");
    constraints_txt.touch()?;
    constraints_txt.write_str("sqlparse<0.4.4")?;

    insta::with_settings!({
        filters => vec![
            (r"\d+(ms|s)", "[TIME]"),
            (r"#    .* pip-compile", "#    [BIN_PATH] pip-compile"),
            (r"--cache-dir .*", "--cache-dir [CACHE_DIR]"),
        ]
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-compile")
            .arg("requirements.in")
            .arg("--constraint")
            .arg("constraints.txt")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir));
    });

    Ok(())
}

/// Resolve a package from a `requirements.in` file, with an inline constraint.
#[test]
fn compile_constraints_inline() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .current_dir(&temp_dir)
        .assert()
        .success();
    venv.assert(predicates::path::is_dir());

    let requirements_in = temp_dir.child("requirements.in");
    requirements_in.touch()?;
    requirements_in.write_str("django==5.0b1")?;
    requirements_in.write_str("-c constraints.txt")?;

    let constraints_txt = temp_dir.child("constraints.txt");
    constraints_txt.touch()?;
    constraints_txt.write_str("sqlparse<0.4.4")?;

    insta::with_settings!({
        filters => vec![
            (r"\d+(ms|s)", "[TIME]"),
            (r"#    .* pip-compile", "#    [BIN_PATH] pip-compile"),
            (r"--cache-dir .*", "--cache-dir [CACHE_DIR]"),
        ]
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-compile")
            .arg("requirements.in")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir));
    });

    Ok(())
}
