#![cfg(feature = "python")]

use std::process::Command;

use anyhow::Result;
use assert_cmd::prelude::*;
use assert_fs::prelude::*;

use uv_fs::Simplified;

use crate::common::{
    create_bin_with_executables, get_bin, uv_snapshot, TestContext, EXCLUDE_NEWER,
};

mod common;

#[test]
fn create_venv() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let bin = create_bin_with_executables(&temp_dir, &["3.12"]).expect("Failed to create bin dir");
    let venv = temp_dir.child(".venv");

    // Create a virtual environment at `.venv`.
    let filter_venv = regex::escape(&venv.simplified_display().to_string());
    let filter_prompt = r"Activate with: (?:.*)\\Scripts\\activate";
    let filters = &[
        (
            r"Using Python 3\.\d+\.\d+ interpreter at: .+",
            "Using Python [VERSION] interpreter at: [PATH]",
        ),
        (&filter_venv, "/home/ferris/project/.venv"),
        (
            filter_prompt,
            "Activate with: source /home/ferris/project/.venv/bin/activate",
        ),
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
    Using Python [VERSION] interpreter at: [PATH]
    Creating virtualenv at: /home/ferris/project/.venv
    Activate with: source /home/ferris/project/.venv/bin/activate
    "###
    );

    venv.assert(predicates::path::is_dir());

    // Create a virtual environment at the same location, which should replace it.
    let filter_venv = regex::escape(&venv.simplified_display().to_string());
    let filter_prompt = r"Activate with: (?:.*)\\Scripts\\activate";
    let filters = &[
        (
            r"Using Python 3\.\d+\.\d+ interpreter at: .+",
            "Using Python [VERSION] interpreter at: [PATH]",
        ),
        (&filter_venv, "/home/ferris/project/.venv"),
        (
            filter_prompt,
            "Activate with: source /home/ferris/project/.venv/bin/activate",
        ),
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
    Using Python [VERSION] interpreter at: [PATH]
    Creating virtualenv at: /home/ferris/project/.venv
    Activate with: source /home/ferris/project/.venv/bin/activate
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

    let filter_venv = regex::escape(&venv.simplified_display().to_string());
    let filter_prompt = r"Activate with: (?:.*)\\Scripts\\activate";
    let filters = &[
        (
            r"Using Python 3\.\d+\.\d+ interpreter at: .+",
            "Using Python [VERSION] interpreter at: [PATH]",
        ),
        (&filter_venv, "/home/ferris/project/.venv"),
        (filter_prompt, "Activate with: source .venv/bin/activate"),
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
    Using Python [VERSION] interpreter at: [PATH]
    Creating virtualenv at: .venv
    Activate with: source .venv/bin/activate
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

    let filter_venv = regex::escape(&venv.simplified_display().to_string());
    let filter_prompt = r"Activate with: (?:.*)\\Scripts\\activate";
    let filters = &[
        (
            r"Using Python 3\.\d+\.\d+ interpreter at: .+",
            "Using Python [VERSION] interpreter at: [PATH]",
        ),
        (&filter_venv, "/home/ferris/project/.venv"),
        (
            filter_prompt,
            "Activate with: source /home/ferris/project/.venv/bin/activate",
        ),
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
    Using Python [VERSION] interpreter at: [PATH]
    Creating virtualenv at: /home/ferris/project/.venv
     + pip==23.3.1
    Activate with: source /home/ferris/project/.venv/bin/activate
    "###
    );

    venv.assert(predicates::path::is_dir());

    Ok(())
}

#[test]
fn seed_older_python_version() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let bin = create_bin_with_executables(&temp_dir, &["3.10"]).expect("Failed to create bin dir");
    let venv = temp_dir.child(".venv");

    let filter_venv = regex::escape(&venv.simplified_display().to_string());
    let filter_prompt = r"Activate with: (?:.*)\\Scripts\\activate";
    let filters = &[
        (
            r"Using Python 3\.\d+\.\d+ interpreter at: .+",
            "Using Python [VERSION] interpreter at: [PATH]",
        ),
        (&filter_venv, "/home/ferris/project/.venv"),
        (
            filter_prompt,
            "Activate with: source /home/ferris/project/.venv/bin/activate",
        ),
    ];
    uv_snapshot!(filters, Command::new(get_bin())
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--seed")
        .arg("--python")
        .arg("3.10")
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
    Using Python [VERSION] interpreter at: [PATH]
    Creating virtualenv at: /home/ferris/project/.venv
     + pip==23.3.1
     + setuptools==68.2.2
     + wheel==0.41.3
    Activate with: source /home/ferris/project/.venv/bin/activate
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
          × No Python 3.15 found through `py --list-paths` or in `PATH`. Is Python 3.15 installed?
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
fn create_venv_unknown_python_patch() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let bin = create_bin_with_executables(&temp_dir, &["3.12"]).expect("Failed to create bin dir");
    let venv = temp_dir.child(".venv");

    let filter_venv = regex::escape(&venv.simplified_display().to_string());
    let filters = &[
        (
            r"Using Python 3\.\d+\.\d+ interpreter at: .+",
            "Using Python [VERSION] interpreter at: [PATH]",
        ),
        (
            r"No Python 3\.8\.0 found through `py --list-paths` or in `PATH`\. Is Python 3\.8\.0 installed\?",
            "No Python 3.8.0 In `PATH`. Is Python 3.8.0 installed?",
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
fn create_venv_python_patch() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let bin =
        create_bin_with_executables(&temp_dir, &["3.12.1"]).expect("Failed to create bin dir");
    let venv = temp_dir.child(".venv");

    let filter_venv = regex::escape(&venv.simplified_display().to_string());
    let filter_prompt = r"Activate with: (?:.*)\\Scripts\\activate";
    let filters = &[
        (r"interpreter at: .+", "interpreter at: [PATH]"),
        (&filter_venv, "/home/ferris/project/.venv"),
        (
            filter_prompt,
            "Activate with: source /home/ferris/project/.venv/bin/activate",
        ),
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
    Using Python 3.12.1 interpreter at: [PATH]
    Creating virtualenv at: /home/ferris/project/.venv
    Activate with: source /home/ferris/project/.venv/bin/activate
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

    let filter_venv = regex::escape(&venv.simplified_display().to_string());
    let filters = &[
        (
            r"Using Python 3\.\d+\.\d+ interpreter at: .+",
            "Using Python [VERSION] interpreter at: [PATH]",
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
    Using Python [VERSION] interpreter at: [PATH]
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

    let filter_venv = regex::escape(&venv.simplified_display().to_string());
    let filter_prompt = r"Activate with: (?:.*)\\Scripts\\activate";
    let filters = &[
        (
            r"Using Python 3\.\d+\.\d+ interpreter at: .+",
            "Using Python [VERSION] interpreter at: [PATH]",
        ),
        (&filter_venv, "/home/ferris/project/.venv"),
        (
            filter_prompt,
            "Activate with: source /home/ferris/project/.venv/bin/activate",
        ),
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
    Using Python [VERSION] interpreter at: [PATH]
    Creating virtualenv at: /home/ferris/project/.venv
    Activate with: source /home/ferris/project/.venv/bin/activate
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

    let filter_venv = regex::escape(&venv.simplified_display().to_string());
    let filters = &[
        (
            r"Using Python 3\.\d+\.\d+ interpreter at: .+",
            "Using Python [VERSION] interpreter at: [PATH]",
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
    Using Python [VERSION] interpreter at: [PATH]
    Creating virtualenv at: /home/ferris/project/.venv
    uv::venv::creation

      × Failed to create virtualenv
      ╰─▶ The directory `/home/ferris/project/.venv` exists, but it's not a virtualenv
    "###
    );

    Ok(())
}

#[test]
#[cfg(windows)]
fn windows_shims() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let bin =
        create_bin_with_executables(&temp_dir, &["3.8", "3.9"]).expect("Failed to create bin dir");
    let venv = temp_dir.child(".venv");
    let shim_path = temp_dir.child("shim");

    let py38 = std::env::split_paths(&bin)
        .last()
        .expect("create_bin_with_executables to set up the python versions");
    // We want 3.8 and the first version should be 3.9.
    // Picking the last is necessary to prove that shims work because the python version selects
    // the python version from the first path segment by default, so we take the last to prove it's not
    // returning that version.
    assert!(py38.to_str().unwrap().contains("3.8"));

    // Write the shim script that forwards the arguments to the python3.8 installation.
    fs_err::create_dir(&shim_path)?;
    fs_err::write(
        shim_path.child("python.bat"),
        format!("@echo off\r\n{}/python.exe %*", py38.display()),
    )?;

    // Create a virtual environment at `.venv`, passing the redundant `--clear` flag.
    let filter_venv = regex::escape(&venv.simplified_display().to_string());
    let filter_prompt = r"Activate with: (?:.*)\\Scripts\\activate";
    let filters = &[
        (
            r"Using Python 3\.8.\d+ interpreter at: .+",
            "Using Python 3.8.x interpreter at: [PATH]",
        ),
        (&filter_venv, "/home/ferris/project/.venv"),
        (
            &filter_prompt,
            "Activate with: source /home/ferris/project/.venv/bin/activate",
        ),
    ];
    uv_snapshot!(filters, Command::new(get_bin())
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--clear")
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .arg("--exclude-newer")
        .arg(EXCLUDE_NEWER)
        .env("UV_TEST_PYTHON_PATH", format!("{};{}", shim_path.display(), bin.simplified_display()))
        .current_dir(&temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: virtualenv's `--clear` has no effect (uv always clears the virtual environment).
    Using Python 3.8.x interpreter at: [PATH]
    Creating virtualenv at: /home/ferris/project/.venv
    Activate with: source /home/ferris/project/.venv/bin/activate
    "###
    );

    venv.assert(predicates::path::is_dir());

    Ok(())
}

#[test]
fn virtualenv_compatibility() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let bin = create_bin_with_executables(&temp_dir, &["3.12"]).expect("Failed to create bin dir");
    let venv = temp_dir.child(".venv");

    // Create a virtual environment at `.venv`, passing the redundant `--clear` flag.
    let filter_venv = regex::escape(&venv.simplified_display().to_string());
    let filter_prompt = r"Activate with: (?:.*)\\Scripts\\activate";
    let filters = &[
        (
            r"Using Python 3\.\d+\.\d+ interpreter at: .+",
            "Using Python [VERSION] interpreter at: [PATH]",
        ),
        (&filter_venv, "/home/ferris/project/.venv"),
        (
            filter_prompt,
            "Activate with: source /home/ferris/project/.venv/bin/activate",
        ),
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
        .env("UV_TEST_PYTHON_PATH", bin)
        .current_dir(&temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: virtualenv's `--clear` has no effect (uv always clears the virtual environment).
    Using Python [VERSION] interpreter at: [PATH]
    Creating virtualenv at: /home/ferris/project/.venv
    Activate with: source /home/ferris/project/.venv/bin/activate
    "###
    );

    venv.assert(predicates::path::is_dir());

    Ok(())
}

#[test]
fn verify_pyvenv_cfg() {
    let context = TestContext::new("3.12");
    let venv = context.temp_dir.child(".venv");
    let pyvenv_cfg = venv.child("pyvenv.cfg");

    venv.assert(predicates::path::is_dir());

    // Check pyvenv.cfg exists
    pyvenv_cfg.assert(predicates::path::is_file());

    // Check if "uv = version" is present in the file
    let version = env!("CARGO_PKG_VERSION").to_string();
    let search_string = format!("uv = {version}");
    pyvenv_cfg.assert(predicates::str::contains(search_string));
}

/// Ensure that a nested virtual environment uses the same `home` directory as the parent.
#[test]
fn verify_nested_pyvenv_cfg() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let bin = create_bin_with_executables(&temp_dir, &["3.12"]).expect("Failed to create bin dir");
    let venv = temp_dir.child(".venv");

    // Create a virtual environment at `.venv`.
    Command::new(get_bin())
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--python")
        .arg("3.12")
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .arg("--exclude-newer")
        .arg(EXCLUDE_NEWER)
        .env("UV_TEST_PYTHON_PATH", bin.clone())
        .current_dir(&temp_dir)
        .assert()
        .success();

    let pyvenv_cfg = venv.child("pyvenv.cfg");

    // Check pyvenv.cfg exists
    pyvenv_cfg.assert(predicates::path::is_file());

    // Extract the "home" line from the pyvenv.cfg file.
    let contents = fs_err::read_to_string(pyvenv_cfg.path())?;
    let venv_home = contents
        .lines()
        .find(|line| line.starts_with("home"))
        .expect("home line not found");

    // Now, create a virtual environment from within the virtual environment.
    let subvenv = temp_dir.child(".subvenv");
    Command::new(get_bin())
        .arg("venv")
        .arg(subvenv.as_os_str())
        .arg("--python")
        .arg("3.12")
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .arg("--exclude-newer")
        .arg(EXCLUDE_NEWER)
        .env("VIRTUAL_ENV", venv.as_os_str())
        .env("UV_TEST_PYTHON_PATH", bin.clone())
        .current_dir(&temp_dir)
        .assert()
        .success();

    let sub_pyvenv_cfg = subvenv.child("pyvenv.cfg");

    // Extract the "home" line from the pyvenv.cfg file.
    let contents = fs_err::read_to_string(sub_pyvenv_cfg.path())?;
    let sub_venv_home = contents
        .lines()
        .find(|line| line.starts_with("home"))
        .expect("home line not found");

    // Check that both directories point to the same home.
    assert_eq!(sub_venv_home, venv_home);

    Ok(())
}
