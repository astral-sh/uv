#![cfg(feature = "python")]

use anyhow::Result;
use assert_cmd::prelude::*;
use assert_fs::prelude::*;

use crate::common::{uv_snapshot, TestContext};

mod common;

#[test]
fn create_venv() {
    let context = TestContext::new_with_versions(&["3.12"]);

    // Create a virtual environment at `.venv`.
    uv_snapshot!(context.filters(), context.venv()
        .arg(context.venv.as_os_str())
        .arg("--python")
        .arg("3.12"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using Python 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtualenv at: .venv
    Activate with: source .venv/bin/activate
    "###
    );

    context.venv.assert(predicates::path::is_dir());

    // Create a virtual environment at the same location, which should replace it.
    uv_snapshot!(context.filters(), context.venv()
        .arg(context.venv.as_os_str())
        .arg("--python")
        .arg("3.12"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using Python 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtualenv at: .venv
    Activate with: source .venv/bin/activate
    "###
    );

    context.venv.assert(predicates::path::is_dir());
}

#[test]
fn create_venv_defaults_to_cwd() {
    let context = TestContext::new_with_versions(&["3.12"]);
    uv_snapshot!(context.filters(), context.venv()
        .arg("--python")
        .arg("3.12"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using Python 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtualenv at: .venv
    Activate with: source .venv/bin/activate
    "###
    );

    context.venv.assert(predicates::path::is_dir());
}

#[test]
fn create_venv_ignores_virtual_env_variable() {
    let context = TestContext::new_with_versions(&["3.12"]);
    // We shouldn't care if `VIRTUAL_ENV` is set to an non-existent directory
    // because we ignore virtual environment interpreter sources (we require a system interpreter)
    uv_snapshot!(context.filters(), context.venv()
        .env("VIRTUAL_ENV", context.temp_dir.child("does-not-exist").as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using Python 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtualenv at: .venv
    Activate with: source .venv/bin/activate
    "###
    );
}

#[test]
fn create_venv_reads_request_from_python_version_file() {
    let context = TestContext::new_with_versions(&["3.11", "3.12"]);

    // Without the file, we should use the first on the PATH
    uv_snapshot!(context.filters(), context.venv()
        .arg("--preview"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using Python 3.11.[X] interpreter at: [PYTHON-3.11]
    Creating virtualenv at: .venv
    Activate with: source .venv/bin/activate
    "###
    );

    // With a version file, we should prefer that version
    context
        .temp_dir
        .child(".python-version")
        .write_str("3.12")
        .unwrap();

    uv_snapshot!(context.filters(), context.venv()
        .arg("--preview"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using Python 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtualenv at: .venv
    Activate with: source .venv/bin/activate
    "###
    );

    context.venv.assert(predicates::path::is_dir());
}

#[test]
fn create_venv_reads_request_from_python_versions_file() {
    let context = TestContext::new_with_versions(&["3.11", "3.12"]);

    // Without the file, we should use the first on the PATH
    uv_snapshot!(context.filters(), context.venv()
        .arg("--preview"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using Python 3.11.[X] interpreter at: [PYTHON-3.11]
    Creating virtualenv at: .venv
    Activate with: source .venv/bin/activate
    "###
    );

    // With a versions file, we should prefer the first listed version
    context
        .temp_dir
        .child(".python-versions")
        .write_str("3.12\n3.11")
        .unwrap();

    uv_snapshot!(context.filters(), context.venv()
        .arg("--preview"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using Python 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtualenv at: .venv
    Activate with: source .venv/bin/activate
    "###
    );

    context.venv.assert(predicates::path::is_dir());
}

#[test]
fn create_venv_explicit_request_takes_priority_over_python_version_file() {
    let context = TestContext::new_with_versions(&["3.11", "3.12"]);

    context
        .temp_dir
        .child(".python-version")
        .write_str("3.12")
        .unwrap();

    uv_snapshot!(context.filters(), context.venv()
        .arg("--preview").arg("--python").arg("3.11"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using Python 3.11.[X] interpreter at: [PYTHON-3.11]
    Creating virtualenv at: .venv
    Activate with: source .venv/bin/activate
    "###
    );

    context.venv.assert(predicates::path::is_dir());
}

#[test]
fn seed() {
    let context = TestContext::new_with_versions(&["3.12"]);
    uv_snapshot!(context.filters(), context.venv()
        .arg(context.venv.as_os_str())
        .arg("--seed")
        .arg("--python")
        .arg("3.12"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using Python 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtualenv at: .venv
     + pip==24.0
    Activate with: source .venv/bin/activate
    "###
    );

    context.venv.assert(predicates::path::is_dir());
}

#[test]
fn seed_older_python_version() {
    let context = TestContext::new_with_versions(&["3.10"]);
    uv_snapshot!(context.filters(), context.venv()
        .arg(context.venv.as_os_str())
        .arg("--seed")
        .arg("--python")
        .arg("3.10"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using Python 3.10.[X] interpreter at: [PYTHON-3.10]
    Creating virtualenv at: .venv
     + pip==24.0
     + setuptools==69.2.0
     + wheel==0.43.0
    Activate with: source .venv/bin/activate
    "###
    );

    context.venv.assert(predicates::path::is_dir());
}

#[test]
fn create_venv_unknown_python_minor() {
    let context = TestContext::new_with_versions(&["3.12"]);

    let mut command = context.venv();
    command
        .arg(context.venv.as_os_str())
        // Request a version we know we'll never see
        .arg("--python")
        .arg("3.100")
        // Unset this variable to force what the user would see
        .env_remove("UV_TEST_PYTHON_PATH");

    if cfg!(windows) {
        uv_snapshot!(&mut command, @r###"
        success: false
        exit_code: 1
        ----- stdout -----

        ----- stderr -----
          × No interpreter found for Python 3.100 in system toolchains
        "###
        );
    } else {
        uv_snapshot!(&mut command, @r###"
        success: false
        exit_code: 1
        ----- stdout -----

        ----- stderr -----
          × No interpreter found for Python 3.100 in system toolchains
        "###
        );
    }

    context.venv.assert(predicates::path::missing());
}

#[test]
fn create_venv_unknown_python_patch() {
    let context = TestContext::new_with_versions(&["3.12"]);

    let mut command = context.venv();
    command
        .arg(context.venv.as_os_str())
        // Request a version we know we'll never see
        .arg("--python")
        .arg("3.12.100")
        // Unset this variable to force what the user would see
        .env_remove("UV_TEST_PYTHON_PATH");

    if cfg!(windows) {
        uv_snapshot!(&mut command, @r###"
        success: false
        exit_code: 1
        ----- stdout -----

        ----- stderr -----
          × No interpreter found for Python 3.12.100 in system toolchains
        "###
        );
    } else {
        uv_snapshot!(&mut command, @r###"
        success: false
        exit_code: 1
        ----- stdout -----

        ----- stderr -----
          × No interpreter found for Python 3.12.100 in system toolchains
        "###
        );
    }

    context.venv.assert(predicates::path::missing());
}

#[cfg(feature = "python-patch")]
#[test]
fn create_venv_python_patch() {
    let context = TestContext::new_with_versions(&["3.12.1"]);

    uv_snapshot!(context.filters(), context.venv()
        .arg(context.venv.as_os_str())
        .arg("--python")
        .arg("3.12.1"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using Python 3.12.1 interpreter at: [PYTHON-3.12.1]
    Creating virtualenv at: .venv
    Activate with: source .venv/bin/activate
    "###
    );

    context.venv.assert(predicates::path::is_dir());
}

#[test]
fn file_exists() -> Result<()> {
    let context = TestContext::new_with_versions(&["3.12"]);

    // Create a file at `.venv`. Creating a virtualenv at the same path should fail.
    context.venv.touch()?;

    uv_snapshot!(context.filters(), context.venv()
        .arg(context.venv.as_os_str())
        .arg("--python")
        .arg("3.12"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Using Python 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtualenv at: .venv
    uv::venv::creation

      × Failed to create virtualenv
      ╰─▶ File exists at `.venv`
    "###
    );

    Ok(())
}

#[test]
fn empty_dir_exists() -> Result<()> {
    let context = TestContext::new_with_versions(&["3.12"]);

    // Create an empty directory at `.venv`. Creating a virtualenv at the same path should succeed.
    context.venv.create_dir_all()?;
    uv_snapshot!(context.filters(), context.venv()
        .arg(context.venv.as_os_str())
        .arg("--python")
        .arg("3.12"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using Python 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtualenv at: .venv
    Activate with: source .venv/bin/activate
    "###
    );

    context.venv.assert(predicates::path::is_dir());

    Ok(())
}

#[test]
fn non_empty_dir_exists() -> Result<()> {
    let context = TestContext::new_with_versions(&["3.12"]);

    // Create a non-empty directory at `.venv`. Creating a virtualenv at the same path should fail.
    context.venv.create_dir_all()?;
    context.venv.child("file").touch()?;

    uv_snapshot!(context.filters(), context.venv()
        .arg(context.venv.as_os_str())
        .arg("--python")
        .arg("3.12"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Using Python 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtualenv at: .venv
    uv::venv::creation

      × Failed to create virtualenv
      ╰─▶ The directory `.venv` exists, but it's not a virtualenv
    "###
    );

    Ok(())
}

#[test]
fn non_empty_dir_exists_allow_existing() -> Result<()> {
    let context = TestContext::new_with_versions(&["3.12"]);

    // Create a non-empty directory at `.venv`. Creating a virtualenv at the same path should
    // succeed when `--allow-existing` is specified, but fail when it is not.
    context.venv.create_dir_all()?;
    context.venv.child("file").touch()?;

    uv_snapshot!(context.filters(), context.venv()
        .arg(context.venv.as_os_str())
        .arg("--python")
        .arg("3.12"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Using Python 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtualenv at: .venv
    uv::venv::creation

      × Failed to create virtualenv
      ╰─▶ The directory `.venv` exists, but it's not a virtualenv
    "###
    );

    uv_snapshot!(context.filters(), context.venv()
        .arg(context.venv.as_os_str())
        .arg("--allow-existing")
        .arg("--python")
        .arg("3.12"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using Python 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtualenv at: .venv
    Activate with: source .venv/bin/activate
    "###
    );

    // Running again should _also_ succeed, overwriting existing symlinks and respecting existing
    // directories.
    uv_snapshot!(context.filters(), context.venv()
        .arg(context.venv.as_os_str())
        .arg("--allow-existing")
        .arg("--python")
        .arg("3.12"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using Python 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtualenv at: .venv
    Activate with: source .venv/bin/activate
    "###
    );

    Ok(())
}

#[test]
#[cfg(windows)]
fn windows_shims() -> Result<()> {
    let context = TestContext::new_with_versions(&["3.9", "3.8"]);
    let shim_path = context.temp_dir.child("shim");

    let py38 = context
        .python_versions
        .last()
        .expect("python_path_with_versions to set up the python versions");

    // We want 3.8 and the first version should be 3.9.
    // Picking the last is necessary to prove that shims work because the python version selects
    // the python version from the first path segment by default, so we take the last to prove it's not
    // returning that version.
    assert!(py38.0.to_string().contains("3.8"));

    // Write the shim script that forwards the arguments to the python3.8 installation.
    fs_err::create_dir(&shim_path)?;
    fs_err::write(
        shim_path.child("python.bat"),
        format!(
            "@echo off\r\n{}/python.exe %*",
            py38.1.parent().unwrap().display()
        ),
    )?;

    // Create a virtual environment at `.venv` with the shim
    uv_snapshot!(context.filters(), context.venv()
        .arg(context.venv.as_os_str())
        .env("UV_TEST_PYTHON_PATH", format!("{};{}", shim_path.display(), context.python_path().to_string_lossy())), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using Python 3.8.[X] interpreter at: [PYTHON-3.8]
    Creating virtualenv at: .venv
    Activate with: source .venv/bin/activate
    "###
    );

    context.venv.assert(predicates::path::is_dir());

    Ok(())
}

#[test]
fn virtualenv_compatibility() {
    let context = TestContext::new_with_versions(&["3.12"]);

    // Create a virtual environment at `.venv`, passing the redundant `--clear` flag.
    uv_snapshot!(context.filters(), context.venv()
        .arg(context.venv.as_os_str())
        .arg("--clear")
        .arg("--python")
        .arg("3.12"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: virtualenv's `--clear` has no effect (uv always clears the virtual environment).
    Using Python 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtualenv at: .venv
    Activate with: source .venv/bin/activate
    "###
    );

    context.venv.assert(predicates::path::is_dir());
}

#[test]
fn verify_pyvenv_cfg() {
    let context = TestContext::new("3.12");
    let pyvenv_cfg = context.venv.child("pyvenv.cfg");

    context.venv.assert(predicates::path::is_dir());

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
    let context = TestContext::new_with_versions(&["3.12"]);

    // Create a virtual environment at `.venv`.
    context
        .venv()
        .arg(context.venv.as_os_str())
        .arg("--python")
        .arg("3.12")
        .assert()
        .success();

    let pyvenv_cfg = context.venv.child("pyvenv.cfg");

    // Check pyvenv.cfg exists
    pyvenv_cfg.assert(predicates::path::is_file());

    // Extract the "home" line from the pyvenv.cfg file.
    let contents = fs_err::read_to_string(pyvenv_cfg.path())?;
    let venv_home = contents
        .lines()
        .find(|line| line.starts_with("home"))
        .expect("home line not found");

    // Now, create a virtual environment from within the virtual environment.
    let subvenv = context.temp_dir.child(".subvenv");
    context
        .venv()
        .arg(subvenv.as_os_str())
        .arg("--python")
        .arg("3.12")
        .env("VIRTUAL_ENV", context.venv.as_os_str())
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

/// See <https://github.com/astral-sh/uv/issues/3280>
#[test]
#[cfg(windows)]
fn path_with_trailing_space_gives_proper_error() {
    let context = TestContext::new_with_versions(&["3.12"]);

    // Set a custom cache directory with a trailing space
    let path_with_trailing_slash = format!("{} ", context.cache_dir.path().display());
    uv_snapshot!(context.filters(), std::process::Command::new(crate::common::get_bin())
        .arg("venv")
        .env("UV_CACHE_DIR", path_with_trailing_slash), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: failed to open file `[CACHE_DIR]/ /CACHEDIR.TAG`
      Caused by: The system cannot find the path specified. (os error 3)
    "###
    );
    // Note the extra trailing `/` in the snapshot is due to the filters, not the actual output.
}
