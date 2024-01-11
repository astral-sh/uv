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
    let venv = temp_dir.child(".venv");

    insta::with_settings!({
        filters => vec![
            (r"Using Python 3\.\d+\.\d+ at .+", "Using Python [VERSION] at [PATH]"),
            (temp_dir.to_str().unwrap(), "/home/ferris/project"),
        ]
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("venv")
            .arg(venv.as_os_str())
            .arg("--python")
            .arg("python3.12")
            .current_dir(&temp_dir), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Using Python [VERSION] at [PATH]
        Creating virtual environment at: /home/ferris/project/.venv
        "###);
    });

    venv.assert(predicates::path::is_dir());

    Ok(())
}

#[test]
fn create_venv_defaults_to_cwd() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    insta::with_settings!({
        filters => vec![
            (r"Using Python 3\.\d+\.\d+ at .+", "Using Python [VERSION] at [PATH]"),
            (temp_dir.to_str().unwrap(), "/home/ferris/project"),
        ]
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("venv")
            .arg("--python")
            .arg("python3.12")
            .current_dir(&temp_dir), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Using Python [VERSION] at [PATH]
        Creating virtual environment at: .venv
        "###);
    });

    venv.assert(predicates::path::is_dir());

    Ok(())
}

#[test]
fn seed() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    insta::with_settings!({
        filters => vec![
            (r"Using Python 3\.\d+\.\d+ at .+", "Using Python [VERSION] at [PATH]"),
            (temp_dir.to_str().unwrap(), "/home/ferris/project"),
        ]
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("venv")
            .arg(venv.as_os_str())
            .arg("--seed")
            .arg("--python")
            .arg("python3.12")
            .current_dir(&temp_dir), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Using Python [VERSION] at [PATH]
        Creating virtual environment at: /home/ferris/project/.venv
         + setuptools==69.0.3
         + pip==23.3.2
         + wheel==0.42.0
        "###);
    });

    venv.assert(predicates::path::is_dir());

    Ok(())
}
