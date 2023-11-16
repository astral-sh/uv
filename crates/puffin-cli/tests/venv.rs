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
    let tempdir = assert_fs::TempDir::new()?;
    let venv = tempdir.child(".venv");

    insta::with_settings!({
        filters => vec![
            (r"Using Python 3.12 at .+", "Using Python 3.11 at [PATH]"),
            (tempdir.to_str().unwrap(), "/home/ferris/project"),
        ]
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("venv")
            .arg(venv.as_os_str())
            .arg("--python")
            .arg("python3.12")
            .current_dir(&tempdir));
    });

    venv.assert(predicates::path::is_dir());

    Ok(())
}

#[test]
fn create_venv_defaults_to_cwd() -> Result<()> {
    let tempdir = assert_fs::TempDir::new()?;
    let venv = tempdir.child(".venv");

    insta::with_settings!({
        filters => vec![
            (r"Using Python 3.12 at .+", "Using Python 3.11 at [PATH]"),
            (tempdir.to_str().unwrap(), "/home/ferris/project"),
        ]
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("venv")
            .arg("--python")
            .arg("python3.12")
            .current_dir(&tempdir));
    });

    venv.assert(predicates::path::is_dir());

    Ok(())
}
