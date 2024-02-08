#![cfg(feature = "python")]

use std::process::Command;

use anyhow::Result;
use assert_fs::prelude::*;

use puffin_fs::NormalizedDisplay;

use crate::common::{create_bin_with_executables, get_bin, puffin_snapshot};

mod common;

#[test]
fn create_venv() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let bin = create_bin_with_executables(&temp_dir, &["3.12"]).expect("Failed to create bin dir");
    let venv = temp_dir.child(".venv");

    let filter_venv = regex::escape(&venv.normalized_display().to_string());
    let filters = &[
        (
            r"Using Python 3\.\d+\.\d+ interpreter at .+",
            "Using Python [VERSION] interpreter at [PATH]",
        ),
        (&filter_venv, "/home/ferris/project/.venv"),
    ];
    puffin_snapshot!(filters, Command::new(get_bin())
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--python")
        .arg("3.12")
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .env("PUFFIN_TEST_PYTHON_PATH", bin)
        .current_dir(&temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using Python [VERSION] interpreter at [PATH]
    Creating virtualenv at: /home/ferris/project/.venv
    "###
    );

    venv.assert(predicates::path::is_dir());

    Ok(())
}

#[test]
fn create_venv_defaults_to_cwd() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let bin = create_bin_with_executables(&temp_dir, &["3.12"]).expect("Failed to create bin dir");
    let venv = temp_dir.child(".venv");

    let filter_venv = regex::escape(&venv.normalized_display().to_string());
    let filters = &[
        (
            r"Using Python 3\.\d+\.\d+ interpreter at .+",
            "Using Python [VERSION] interpreter at [PATH]",
        ),
        (&filter_venv, "/home/ferris/project/.venv"),
    ];
    puffin_snapshot!(filters, Command::new(get_bin())
        .arg("venv")
        .arg("--python")
        .arg("3.12")
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .env("PUFFIN_TEST_PYTHON_PATH", bin)
        .current_dir(&temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using Python [VERSION] interpreter at [PATH]
    Creating virtualenv at: .venv
    "###
    );

    venv.assert(predicates::path::is_dir());

    Ok(())
}

#[test]
fn seed() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let bin = create_bin_with_executables(&temp_dir, &["3.12"]).expect("Failed to create bin dir");
    let venv = temp_dir.child(".venv");

    let filter_venv = regex::escape(&venv.normalized_display().to_string());
    let filters = &[
        (
            r"Using Python 3\.\d+\.\d+ interpreter at .+",
            "Using Python [VERSION] interpreter at [PATH]",
        ),
        (&filter_venv, "/home/ferris/project/.venv"),
    ];
    puffin_snapshot!(filters, Command::new(get_bin())
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--seed")
        .arg("--python")
        .arg("3.12")
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .env("PUFFIN_TEST_PYTHON_PATH", bin)
        .current_dir(&temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using Python [VERSION] interpreter at [PATH]
    Creating virtualenv at: /home/ferris/project/.venv
     + setuptools==69.0.3
     + pip==24.0
     + wheel==0.42.0
    "###
    );

    venv.assert(predicates::path::is_dir());

    Ok(())
}

#[test]
fn create_venv_unknown_python_minor() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let bin = create_bin_with_executables(&temp_dir, &["3.12"]).expect("Failed to create bin dir");
    let venv = temp_dir.child(".venv");

    let mut command = Command::new(get_bin());
    command
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--python")
        .arg("3.15")
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .env("PUFFIN_TEST_PYTHON_PATH", bin)
        .current_dir(&temp_dir);
    if cfg!(windows) {
        puffin_snapshot!(&mut command, @r###"
        success: false
        exit_code: 1
        ----- stdout -----

        ----- stderr -----
          × No Python 3.15 found through `py --list-paths`. Is Python 3.15 installed?
        "###
        );
    } else {
        puffin_snapshot!(&mut command, @r###"
        success: false
        exit_code: 1
        ----- stdout -----

        ----- stderr -----
          × No Python 3.15 In `PATH`. Is Python 3.15 installed?
        "###
        );
    }

    venv.assert(predicates::path::missing());

    Ok(())
}

#[test]
#[cfg(unix)] // TODO(konstin): Support patch versions on Windows
fn create_venv_unknown_python_patch() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let bin = create_bin_with_executables(&temp_dir, &["3.12"]).expect("Failed to create bin dir");
    let venv = temp_dir.child(".venv");

    let filter_venv = regex::escape(&venv.normalized_display().to_string());
    let filters = &[
        (
            r"Using Python 3\.\d+\.\d+ interpreter at .+",
            "Using Python [VERSION] interpreter at [PATH]",
        ),
        (&filter_venv, "/home/ferris/project/.venv"),
    ];
    puffin_snapshot!(filters, Command::new(get_bin())
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--python")
        .arg("3.8.0")
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .env("PUFFIN_TEST_PYTHON_PATH", bin)
        .current_dir(&temp_dir), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No Python 3.8.0 In `PATH`. Is Python 3.8.0 installed?
    "###
    );

    venv.assert(predicates::path::missing());

    Ok(())
}

#[test]
#[ignore] // TODO(konstin): Switch patch version strategy
#[cfg(unix)] // TODO(konstin): Support patch versions on Windows
fn create_venv_python_patch() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let bin =
        create_bin_with_executables(&temp_dir, &["3.12.1"]).expect("Failed to create bin dir");
    let venv = temp_dir.child(".venv");

    let filter_venv = regex::escape(&venv.normalized_display().to_string());
    let filters = &[
        (r"interpreter at .+", "interpreter at [PATH]"),
        (&filter_venv, "/home/ferris/project/.venv"),
    ];
    puffin_snapshot!(filters, Command::new(get_bin())
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--python")
        .arg("3.12.1")
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .env("PUFFIN_TEST_PYTHON_PATH", bin)
        .current_dir(&temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using Python 3.12.1 interpreter at [PATH]
    Creating virtualenv at: /home/ferris/project/.venv
    "###
    );

    venv.assert(predicates::path::is_dir());

    Ok(())
}
