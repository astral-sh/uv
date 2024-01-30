#![cfg(feature = "python")]

use std::process::Command;

use anyhow::Result;
use assert_fs::prelude::*;
use insta_cmd::_macro_support::insta;
use insta_cmd::{assert_cmd_snapshot, get_cargo_bin};

use common::BIN_NAME;

mod common;

#[test]
fn create_venv() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    let filter_venv = regex::escape(&venv.display().to_string());
    insta::with_settings!({
        filters => vec![
            (r"Using Python 3\.\d+\.\d+ interpreter at .+", "Using Python [VERSION] interpreter at [PATH]"),
            (&filter_venv, "/home/ferris/project/.venv"),
        ]
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("venv")
            .arg(venv.as_os_str())
            .arg("--python")
            .arg("3.12")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .current_dir(&temp_dir), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Using Python [VERSION] interpreter at [PATH]
        Creating virtualenv at: /home/ferris/project/.venv
        "###);
    });

    venv.assert(predicates::path::is_dir());

    Ok(())
}

#[test]
fn create_venv_defaults_to_cwd() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    let filter_venv = regex::escape(&venv.display().to_string());
    insta::with_settings!({
        filters => vec![
            (r"Using Python 3\.\d+\.\d+ interpreter at .+", "Using Python [VERSION] interpreter at [PATH]"),
            (&filter_venv, "/home/ferris/project/.venv"),
        ]
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("venv")
            .arg("--python")
            .arg("3.12")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .current_dir(&temp_dir), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Using Python [VERSION] interpreter at [PATH]
        Creating virtualenv at: .venv
        "###);
    });

    venv.assert(predicates::path::is_dir());

    Ok(())
}

#[test]
fn seed() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    let filter_venv = regex::escape(&venv.display().to_string());
    insta::with_settings!({
        filters => vec![
            (r"Using Python 3\.\d+\.\d+ interpreter at .+", "Using Python [VERSION] interpreter at [PATH]"),
            (&filter_venv, "/home/ferris/project/.venv"),
        ]
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("venv")
            .arg(venv.as_os_str())
            .arg("--seed")
            .arg("--python")
            .arg("3.12")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .current_dir(&temp_dir), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Using Python [VERSION] interpreter at [PATH]
        Creating virtualenv at: /home/ferris/project/.venv
         + setuptools==69.0.3
         + pip==23.3.2
         + wheel==0.42.0
        "###);
    });

    venv.assert(predicates::path::is_dir());

    Ok(())
}

#[test]
fn create_venv_unknown_python_minor() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    let filter_venv = regex::escape(&venv.display().to_string());
    insta::with_settings!({
        filters => vec![
            (r"Using Python 3\.\d+\.\d+ interpreter at .+", "Using Python [VERSION] interpreter at [PATH]"),
            (&filter_venv, "/home/ferris/project/.venv"),
        ]
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("venv")
            .arg(venv.as_os_str())
            .arg("--python")
            .arg("3.15")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .current_dir(&temp_dir), @r###"
        success: false
        exit_code: 1
        ----- stdout -----

        ----- stderr -----
          × Couldn't find `python3.15` in PATH. Is this Python version installed?
        "###);
    });

    venv.assert(predicates::path::missing());

    Ok(())
}

#[test]
fn create_venv_unknown_python_patch() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    let filter_venv = regex::escape(&venv.display().to_string());
    insta::with_settings!({
        filters => vec![
            (r"Using Python 3\.\d+\.\d+ interpreter at .+", "Using Python [VERSION] interpreter at [PATH]"),
            (&filter_venv, "/home/ferris/project/.venv"),
        ]
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("venv")
            .arg(venv.as_os_str())
            .arg("--python")
            .arg("3.8.0")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .current_dir(&temp_dir), @r###"
        success: false
        exit_code: 1
        ----- stdout -----

        ----- stderr -----
          × Couldn't find `python3.8.0` in PATH. Is this Python version installed?
        "###);
    });

    venv.assert(predicates::path::missing());

    Ok(())
}

#[test]
fn create_venv_python_patch() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    let filter_venv = regex::escape(&venv.display().to_string());
    insta::with_settings!({
        filters => vec![
            (r"interpreter at .+", "interpreter at [PATH]"),
            (&filter_venv, "/home/ferris/project/.venv"),
        ]
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("venv")
            .arg(venv.as_os_str())
            .arg("--python")
            .arg("3.12.1")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .current_dir(&temp_dir), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Using Python 3.12.1 interpreter at [PATH]
        Creating virtualenv at: /home/ferris/project/.venv
        "###);
    });

    venv.assert(predicates::path::is_dir());

    Ok(())
}
