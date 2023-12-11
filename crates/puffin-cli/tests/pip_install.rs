#![cfg(all(feature = "python", feature = "pypi"))]

use std::path::Path;
use std::process::Command;

use anyhow::Result;
use assert_cmd::prelude::*;
use assert_fs::prelude::*;
use insta_cmd::_macro_support::insta;
use insta_cmd::{assert_cmd_snapshot, get_cargo_bin};

use common::{create_venv_py312, BIN_NAME, INSTA_FILTERS};

mod common;

// Exclude any packages uploaded after this date.
static EXCLUDE_NEWER: &str = "2023-11-18T12:00:00Z";

fn check_command(venv: &Path, command: &str, temp_dir: &Path) {
    Command::new(venv.join("bin").join("python"))
        .arg("-c")
        .arg(command)
        .current_dir(temp_dir)
        .assert()
        .success();
}

#[test]
fn missing_requirements_txt() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let requirements_txt = temp_dir.child("requirements.txt");

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .arg("pip-install")
        .arg("-r")
        .arg("requirements.txt")
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .current_dir(&temp_dir), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: failed to open file `requirements.txt`
      Caused by: No such file or directory (os error 2)
    "###);

    requirements_txt.assert(predicates::path::missing());

    Ok(())
}

/// Install a package from the command line into a virtual environment.
#[test]
fn install_package() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv_py312(&temp_dir, &cache_dir);

    // Install Flask.
    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("Flask")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .arg("--exclude-newer")
            .arg(EXCLUDE_NEWER)
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Resolved 7 packages in [TIME]
        Resolved 7 packages in [TIME]
        Downloaded 7 packages in [TIME]
        Installed 7 packages in [TIME]
         + blinker==1.7.0
         + click==8.1.7
         + flask==3.0.0
         + itsdangerous==2.1.2
         + jinja2==3.1.2
         + markupsafe==2.1.3
         + werkzeug==3.0.1
        "###);
    });

    check_command(&venv, "import flask", &temp_dir);

    Ok(())
}

/// Install a package from a `requirements.txt` into a virtual environment.
#[test]
fn install_requirements_txt() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv_py312(&temp_dir, &cache_dir);

    // Install Flask.
    let requirements_txt = temp_dir.child("requirements.txt");
    requirements_txt.write_str("Flask")?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("-r")
            .arg("requirements.txt")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .arg("--exclude-newer")
            .arg(EXCLUDE_NEWER)
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Resolved 7 packages in [TIME]
        Resolved 7 packages in [TIME]
        Downloaded 7 packages in [TIME]
        Installed 7 packages in [TIME]
         + blinker==1.7.0
         + click==8.1.7
         + flask==3.0.0
         + itsdangerous==2.1.2
         + jinja2==3.1.2
         + markupsafe==2.1.3
         + werkzeug==3.0.1
        "###);
    });

    check_command(&venv, "import flask", &temp_dir);

    // Install Jinja2 (which should already be installed, but shouldn't remove other packages).
    let requirements_txt = temp_dir.child("requirements.txt");
    requirements_txt.write_str("Jinja2")?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("-r")
            .arg("requirements.txt")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .arg("--exclude-newer")
            .arg(EXCLUDE_NEWER)
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Resolved 2 packages in [TIME]
        Audited 2 packages in [TIME]
        "###);
    });

    check_command(&venv, "import flask", &temp_dir);

    Ok(())
}
