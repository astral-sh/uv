#![cfg(feature = "python")]

use std::process::Command;

use anyhow::Result;
use assert_fs::prelude::*;

use uv_fs::Normalized;

use crate::common::{create_bin_with_executables, get_bin, uv_snapshot, EXCLUDE_NEWER};

mod common;

#[test]
fn create_venv() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let bin = create_bin_with_executables(&temp_dir, &["3.12"]).expect("Failed to create bin dir");
    let venv = temp_dir.child(".venv");

    // Create a virtual environment at `.venv`.
    let filter_venv = regex::escape(&venv.normalized_display().to_string());
    let filters = &[
        (
            r"Using Python 3\.\d+\.\d+ interpreter at .+",
            "Using Python [VERSION] interpreter at [PATH]",
        ),
        (&filter_venv, "/home/ferris/project/.venv"),
    ];
    uv_snapshot!(filters, Command::new(get_bin())
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--python")
        .arg("3.12")
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .arg("--exclude-newer")
        .arg(EXCLUDE_NEWER)
        .env("UV_TEST_PYTHON_PATH", bin.clone())
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

    // Create a virtual environment at the same location, which should replace it.
    let filter_venv = regex::escape(&venv.normalized_display().to_string());
    let filters = &[
        (
            r"Using Python 3\.\d+\.\d+ interpreter at .+",
            "Using Python [VERSION] interpreter at [PATH]",
        ),
        (&filter_venv, "/home/ferris/project/.venv"),
    ];
    uv_snapshot!(filters, Command::new(get_bin())
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--python")
        .arg("3.12")
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .arg("--exclude-newer")
        .arg(EXCLUDE_NEWER)
        .env("UV_NO_WRAP", "1")
        .env("UV_TEST_PYTHON_PATH", bin)
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
    uv_snapshot!(filters, Command::new(get_bin())
        .arg("venv")
        .arg("--python")
        .arg("3.12")
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .arg("--exclude-newer")
        .arg(EXCLUDE_NEWER)
        .env("UV_NO_WRAP", "1")
        .env("UV_TEST_PYTHON_PATH", bin)
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
    uv_snapshot!(filters, Command::new(get_bin())
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--seed")
        .arg("--python")
        .arg("3.12")
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .arg("--exclude-newer")
        .arg(EXCLUDE_NEWER)
        .env("UV_NO_WRAP", "1")
        .env("UV_TEST_PYTHON_PATH", bin)
        .current_dir(&temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using Python [VERSION] interpreter at [PATH]
    Creating virtualenv at: /home/ferris/project/.venv
     + setuptools==68.2.2
     + pip==23.3.1
     + wheel==0.41.3
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
        .arg("--exclude-newer")
        .arg(EXCLUDE_NEWER)
        .env("UV_NO_WRAP", "1")
        .env("UV_TEST_PYTHON_PATH", bin)
        .current_dir(&temp_dir);
    if cfg!(windows) {
        uv_snapshot!(&mut command, @r###"
        success: false
        exit_code: 1
        ----- stdout -----

        ----- stderr -----
          × No Python 3.15 found through `py --list-paths`. Is Python 3.15 installed?
        "###
        );
    } else {
        uv_snapshot!(&mut command, @r###"
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
    uv_snapshot!(filters, Command::new(get_bin())
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--python")
        .arg("3.8.0")
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .arg("--exclude-newer")
        .arg(EXCLUDE_NEWER)
        .env("UV_NO_WRAP", "1")
        .env("UV_TEST_PYTHON_PATH", bin)
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
    uv_snapshot!(filters, Command::new(get_bin())
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--python")
        .arg("3.12.1")
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .arg("--exclude-newer")
        .arg(EXCLUDE_NEWER)
        .env("UV_NO_WRAP", "1")
        .env("UV_TEST_PYTHON_PATH", bin)
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

#[test]
fn file_exists() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let bin = create_bin_with_executables(&temp_dir, &["3.12"]).expect("Failed to create bin dir");
    let venv = temp_dir.child(".venv");

    // Create a file at `.venv`. Creating a virtualenv at the same path should fail.
    venv.touch()?;

    let filter_venv = regex::escape(&venv.normalized_display().to_string());
    let filters = &[
        (
            r"Using Python 3\.\d+\.\d+ interpreter at .+",
            "Using Python [VERSION] interpreter at [PATH]",
        ),
        (&filter_venv, "/home/ferris/project/.venv"),
    ];
    uv_snapshot!(filters, Command::new(get_bin())
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--python")
        .arg("3.12")
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .arg("--exclude-newer")
        .arg(EXCLUDE_NEWER)
        .env("UV_NO_WRAP", "1")
        .env("UV_TEST_PYTHON_PATH", bin)
        .current_dir(&temp_dir), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Using Python [VERSION] interpreter at [PATH]
    Creating virtualenv at: /home/ferris/project/.venv
    uv::venv::creation

      × Failed to create virtualenv
      ╰─▶ File exists at `/home/ferris/project/.venv`
    "###
    );

    Ok(())
}

#[test]
fn empty_dir_exists() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let bin = create_bin_with_executables(&temp_dir, &["3.12"]).expect("Failed to create bin dir");
    let venv = temp_dir.child(".venv");

    // Create an empty directory at `.venv`. Creating a virtualenv at the same path should succeed.
    venv.create_dir_all()?;

    let filter_venv = regex::escape(&venv.normalized_display().to_string());
    let filters = &[
        (
            r"Using Python 3\.\d+\.\d+ interpreter at .+",
            "Using Python [VERSION] interpreter at [PATH]",
        ),
        (&filter_venv, "/home/ferris/project/.venv"),
    ];
    uv_snapshot!(filters, Command::new(get_bin())
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--python")
        .arg("3.12")
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .arg("--exclude-newer")
        .arg(EXCLUDE_NEWER)
        .env("UV_NO_WRAP", "1")
        .env("UV_TEST_PYTHON_PATH", bin)
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
fn non_empty_dir_exists() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let bin = create_bin_with_executables(&temp_dir, &["3.12"]).expect("Failed to create bin dir");
    let venv = temp_dir.child(".venv");

    // Create a non-empty directory at `.venv`. Creating a virtualenv at the same path should fail.
    venv.create_dir_all()?;
    venv.child("file").touch()?;

    let filter_venv = regex::escape(&venv.normalized_display().to_string());
    let filters = &[
        (
            r"Using Python 3\.\d+\.\d+ interpreter at .+",
            "Using Python [VERSION] interpreter at [PATH]",
        ),
        (&filter_venv, "/home/ferris/project/.venv"),
    ];
    uv_snapshot!(filters, Command::new(get_bin())
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--python")
        .arg("3.12")
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .arg("--exclude-newer")
        .arg(EXCLUDE_NEWER)
        .env("UV_NO_WRAP", "1")
        .env("UV_TEST_PYTHON_PATH", bin)
        .current_dir(&temp_dir), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Using Python [VERSION] interpreter at [PATH]
    Creating virtualenv at: /home/ferris/project/.venv
    uv::venv::creation

      × Failed to create virtualenv
      ╰─▶ The directory `/home/ferris/project/.venv` exists, but it's not a virtualenv
    "###
    );

    Ok(())
}

#[test]
fn virtualenv_compatibility() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let bin = create_bin_with_executables(&temp_dir, &["3.12"]).expect("Failed to create bin dir");
    let venv = temp_dir.child(".venv");

    // Create a virtual environment at `.venv`, passing the redundant `--clear` flag.
    let filter_venv = regex::escape(&venv.normalized_display().to_string());
    let filters = &[
        (
            r"Using Python 3\.\d+\.\d+ interpreter at .+",
            "Using Python [VERSION] interpreter at [PATH]",
        ),
        (&filter_venv, "/home/ferris/project/.venv"),
    ];
    uv_snapshot!(filters, Command::new(get_bin())
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--clear")
        .arg("--python")
        .arg("3.12")
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .arg("--exclude-newer")
        .arg(EXCLUDE_NEWER)
        .env("UV_TEST_PYTHON_PATH", bin.clone())
        .current_dir(&temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: virtualenv's `--clear` has no effect (uv always clears the virtual environment).
    Using Python [VERSION] interpreter at [PATH]
    Creating virtualenv at: /home/ferris/project/.venv
    "###
    );

    venv.assert(predicates::path::is_dir());

    Ok(())
}
