use std::{path::Path, process::Command};

use crate::common::{uv_snapshot, TestContext};
use assert_fs::{
    assert::PathAssert,
    prelude::{FileTouch, PathChild, PathCreateDir},
};
use predicates::prelude::predicate;
use uv_fs::Simplified;
use uv_static::EnvVars;

#[test]
fn python_install() {
    let context: TestContext = TestContext::new_with_versions(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs();

    // Install the latest version
    uv_snapshot!(context.filters(), context.python_install(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.2 in [TIME]
     + cpython-3.13.2-[PLATFORM]
    "###);

    let bin_python = context
        .bin_dir
        .child(format!("python3.13{}", std::env::consts::EXE_SUFFIX));

    // The executable should not be installed in the bin directory (requires preview)
    bin_python.assert(predicate::path::missing());

    // Should be a no-op when already installed
    uv_snapshot!(context.filters(), context.python_install(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Python is already installed. Use `uv python install <request>` to install another version.
    "###);

    // Similarly, when a requested version is already installed
    uv_snapshot!(context.filters(), context.python_install().arg("3.13"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    "###);

    // You can opt-in to a reinstall
    uv_snapshot!(context.filters(), context.python_install().arg("3.13").arg("--reinstall"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.2 in [TIME]
     ~ cpython-3.13.2-[PLATFORM]
    "###);

    // Uninstallation requires an argument
    uv_snapshot!(context.filters(), context.python_uninstall(), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: the following required arguments were not provided:
      <TARGETS>...

    Usage: uv python uninstall --install-dir <INSTALL_DIR> <TARGETS>...

    For more information, try '--help'.
    "###);

    uv_snapshot!(context.filters(), context.python_uninstall().arg("3.13"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Searching for Python versions matching: Python 3.13
    Uninstalled Python 3.13.2 in [TIME]
     - cpython-3.13.2-[PLATFORM]
    "###);
}

#[test]
fn python_reinstall() {
    let context: TestContext = TestContext::new_with_versions(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs();

    // Install a couple versions
    uv_snapshot!(context.filters(), context.python_install().arg("3.12").arg("3.13"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed 2 versions in [TIME]
     + cpython-3.12.9-[PLATFORM]
     + cpython-3.13.2-[PLATFORM]
    ");

    // Reinstall a single version
    uv_snapshot!(context.filters(), context.python_install().arg("3.13").arg("--reinstall"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.2 in [TIME]
     ~ cpython-3.13.2-[PLATFORM]
    ");

    // Reinstall multiple versions
    uv_snapshot!(context.filters(), context.python_install().arg("--reinstall"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed 2 versions in [TIME]
     ~ cpython-3.12.9-[PLATFORM]
     ~ cpython-3.13.2-[PLATFORM]
    ");

    // Reinstalling a version that is not installed should also work
    uv_snapshot!(context.filters(), context.python_install().arg("3.11").arg("--reinstall"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.11.11 in [TIME]
     + cpython-3.11.11-[PLATFORM]
    ");
}

#[test]
fn python_reinstall_patch() {
    let context: TestContext = TestContext::new_with_versions(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs();

    // Install a couple patch versions
    uv_snapshot!(context.filters(), context.python_install().arg("3.12.6").arg("3.12.7"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed 2 versions in [TIME]
     + cpython-3.12.6-[PLATFORM]
     + cpython-3.12.7-[PLATFORM]
    ");

    // Reinstall all "3.12" versions
    // TODO(zanieb): This doesn't work today, because we need this to install the "latest" as there
    // is no workflow for `--upgrade` yet
    uv_snapshot!(context.filters(), context.python_install().arg("3.12").arg("--reinstall"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.12.9 in [TIME]
     + cpython-3.12.9-[PLATFORM]
    ");
}

#[test]
fn python_install_automatic() {
    let context: TestContext = TestContext::new_with_versions(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_filtered_python_sources()
        .with_managed_python_dirs();

    // With downloads disabled, the automatic install should fail
    uv_snapshot!(context.filters(), context.run()
        .env_remove("VIRTUAL_ENV")
        .arg("--no-python-downloads")
        .arg("python").arg("-c").arg("import sys; print(sys.version_info[:2])"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No interpreter found in [PYTHON SOURCES]
    "###);

    // Otherwise, we should fetch the latest Python version
    uv_snapshot!(context.filters(), context.run()
        .env_remove("VIRTUAL_ENV")
        .arg("python").arg("-c").arg("import sys; print(sys.version_info[:2])"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    (3, 13)

    ----- stderr -----
    "###);

    // Subsequently, we can use the interpreter even with downloads disabled
    uv_snapshot!(context.filters(), context.run()
        .env_remove("VIRTUAL_ENV")
        .arg("--no-python-downloads")
        .arg("python").arg("-c").arg("import sys; print(sys.version_info[:2])"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    (3, 13)

    ----- stderr -----
    "###);

    // We should respect the Python request
    uv_snapshot!(context.filters(), context.run()
    .env_remove("VIRTUAL_ENV")
    .arg("-p").arg("3.12")
    .arg("python").arg("-c").arg("import sys; print(sys.version_info[:2])"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    (3, 12)

    ----- stderr -----
    "###);

    // But some requests cannot be mapped to a download
    uv_snapshot!(context.filters(), context.run()
       .env_remove("VIRTUAL_ENV")
       .arg("-p").arg("foobar")
       .arg("python").arg("-c").arg("import sys; print(sys.version_info[:2])"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No interpreter found for executable name `foobar` in [PYTHON SOURCES]
    "###);

    // Create a "broken" Python executable in the test context `bin`
    // (the snapshot is different on Windows so we just test on Unix)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let contents = r"#!/bin/sh
        echo 'error: intentionally broken python executable' >&2
        exit 1";
        let python = context
            .bin_dir
            .join(format!("python3{}", std::env::consts::EXE_SUFFIX));
        fs_err::write(&python, contents).unwrap();

        let mut perms = fs_err::metadata(&python).unwrap().permissions();
        perms.set_mode(0o755);
        fs_err::set_permissions(&python, perms).unwrap();

        // We should ignore the broken executable and download a version still
        uv_snapshot!(context.filters(), context.run()
            .env_remove("VIRTUAL_ENV")
            // In tests, we ignore `PATH` during Python discovery so we need to add the context `bin`
            .env("UV_TEST_PYTHON_PATH", context.bin_dir.as_os_str())
            .arg("-p").arg("3.11")
            .arg("python").arg("-c").arg("import sys; print(sys.version_info[:2])"), @r###"
        success: true
        exit_code: 0
        ----- stdout -----
        (3, 11)

        ----- stderr -----
        "###);
    }
}

#[test]
fn python_install_preview() {
    let context: TestContext = TestContext::new_with_versions(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs();

    // Install the latest version
    uv_snapshot!(context.filters(), context.python_install().arg("--preview"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.2 in [TIME]
     + cpython-3.13.2-[PLATFORM] (python, python3, python3.13)
    "###);

    let bin_python = context
        .bin_dir
        .child(format!("python3.13{}", std::env::consts::EXE_SUFFIX));

    // The executable should be installed in the bin directory
    bin_python.assert(predicate::path::exists());

    // On Unix, it should be a link
    #[cfg(unix)]
    bin_python.assert(predicate::path::is_symlink());

    // The executable should "work"
    uv_snapshot!(context.filters(), Command::new(bin_python.as_os_str())
        .arg("-c").arg("import subprocess; print('hello world')"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    hello world

    ----- stderr -----
    "###);

    // Should be a no-op when already installed
    uv_snapshot!(context.filters(), context.python_install().arg("--preview"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Python is already installed. Use `uv python install <request>` to install another version.
    "###);

    // You can opt-in to a reinstall
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("--reinstall"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.2 in [TIME]
     ~ cpython-3.13.2-[PLATFORM] (python, python3, python3.13)
    "###);

    // The executable should still be present in the bin directory
    bin_python.assert(predicate::path::exists());

    // You can also force replacement of the executables
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("--force"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.2 in [TIME]
     + cpython-3.13.2-[PLATFORM] (python, python3, python3.13)
    "###);

    // The executable should still be present in the bin directory
    bin_python.assert(predicate::path::exists());

    // If an unmanaged executable is present, `--force` is required
    fs_err::remove_file(bin_python.path()).unwrap();
    bin_python.touch().unwrap();

    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.13"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    error: Failed to install cpython-3.13.2-[PLATFORM]
      Caused by: Executable already exists at `[BIN]/python3.13` but is not managed by uv; use `--force` to replace it
    "###);

    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("--force").arg("3.13"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.2 in [TIME]
     + cpython-3.13.2-[PLATFORM] (python3.13)
    "###);

    bin_python.assert(predicate::path::exists());

    // On Unix, it should be a link
    #[cfg(unix)]
    bin_python.assert(predicate::path::is_symlink());

    // Uninstallation requires an argument
    uv_snapshot!(context.filters(), context.python_uninstall(), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: the following required arguments were not provided:
      <TARGETS>...

    Usage: uv python uninstall --install-dir <INSTALL_DIR> <TARGETS>...

    For more information, try '--help'.
    "###);

    uv_snapshot!(context.filters(), context.python_uninstall().arg("3.13"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Searching for Python versions matching: Python 3.13
    Uninstalled Python 3.13.2 in [TIME]
     - cpython-3.13.2-[PLATFORM] (python, python3, python3.13)
    "###);

    // The executable should be removed
    bin_python.assert(predicate::path::missing());

    // Install multiple patch versions
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.12.8").arg("3.12.6"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed 2 versions in [TIME]
     + cpython-3.12.6-[PLATFORM]
     + cpython-3.12.8-[PLATFORM] (python3.12)
    "###);

    let bin_python = context
        .bin_dir
        .child(format!("python3.12{}", std::env::consts::EXE_SUFFIX));

    // The link should be for the newer patch version
    if cfg!(unix) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link_path(&bin_python), @"[TEMP_DIR]/managed/cpython-3.12.8-[PLATFORM]/bin/python3.12"
            );
        });
    } else {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link_path(&bin_python), @"[TEMP_DIR]/managed/cpython-3.12.8-[PLATFORM]/python"
            );
        });
    }
}

#[test]
fn python_install_preview_upgrade() {
    let context = TestContext::new_with_versions(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs();

    let bin_python = context
        .bin_dir
        .child(format!("python3.12{}", std::env::consts::EXE_SUFFIX));

    // Install 3.12.5
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.12.5"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.12.5 in [TIME]
     + cpython-3.12.5-[PLATFORM] (python3.12)
    "###);

    // Installing 3.12.4 should not replace the executable, but also shouldn't fail
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.12.4"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.12.4 in [TIME]
     + cpython-3.12.4-[PLATFORM]
    "###);

    if cfg!(unix) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link_path(&bin_python), @"[TEMP_DIR]/managed/cpython-3.12.5-[PLATFORM]/bin/python3.12"
            );
        });
    } else {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link_path(&bin_python), @"[TEMP_DIR]/managed/cpython-3.12.5-[PLATFORM]/python"
            );
        });
    }

    // Using `--reinstall` is not sufficient to replace it either
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.12.4").arg("--reinstall"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.12.4 in [TIME]
     ~ cpython-3.12.4-[PLATFORM]
    "###);

    if cfg!(unix) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link_path(&bin_python), @"[TEMP_DIR]/managed/cpython-3.12.5-[PLATFORM]/bin/python3.12"
            );
        });
    } else {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link_path(&bin_python), @"[TEMP_DIR]/managed/cpython-3.12.5-[PLATFORM]/python"
            );
        });
    }

    // But `--force` is
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.12.4").arg("--force"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.12.4 in [TIME]
     + cpython-3.12.4-[PLATFORM] (python3.12)
    "###);

    if cfg!(unix) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link_path(&bin_python), @"[TEMP_DIR]/managed/cpython-3.12.4-[PLATFORM]/bin/python3.12"
            );
        });
    } else {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link_path(&bin_python), @"[TEMP_DIR]/managed/cpython-3.12.4-[PLATFORM]/python"
            );
        });
    }

    // But installing 3.12.6 should upgrade automatically
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.12.6"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.12.6 in [TIME]
     + cpython-3.12.6-[PLATFORM] (python3.12)
    "###);

    if cfg!(unix) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link_path(&bin_python), @"[TEMP_DIR]/managed/cpython-3.12.6-[PLATFORM]/bin/python3.12"
            );
        });
    } else {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link_path(&bin_python), @"[TEMP_DIR]/managed/cpython-3.12.6-[PLATFORM]/python"
            );
        });
    }
}

#[test]
fn python_install_freethreaded() {
    let context: TestContext = TestContext::new_with_versions(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs();

    // Install the latest version
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.13t"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.2 in [TIME]
     + cpython-3.13.2+freethreaded-[PLATFORM] (python3.13t)
    "###);

    let bin_python = context
        .bin_dir
        .child(format!("python3.13t{}", std::env::consts::EXE_SUFFIX));

    // The executable should be installed in the bin directory
    bin_python.assert(predicate::path::exists());

    // On Unix, it should be a link
    #[cfg(unix)]
    bin_python.assert(predicate::path::is_symlink());

    // The executable should "work"
    uv_snapshot!(context.filters(), Command::new(bin_python.as_os_str())
        .arg("-c").arg("import subprocess; print('hello world')"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    hello world

    ----- stderr -----
    "###);

    // Should be distinct from 3.13
    uv_snapshot!(context.filters(), context.python_install().arg("3.13"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.2 in [TIME]
     + cpython-3.13.2-[PLATFORM]
    "###);

    // Should not work with older Python versions
    uv_snapshot!(context.filters(), context.python_install().arg("3.12t"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No download found for request: cpython-3.12t-[PLATFORM]
    "###);

    uv_snapshot!(context.filters(), context.python_uninstall().arg("--all"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Searching for Python installations
    Uninstalled 2 versions in [TIME]
     - cpython-3.13.2-[PLATFORM]
     - cpython-3.13.2+freethreaded-[PLATFORM] (python3.13t)
    "###);
}

#[test]
fn python_install_invalid_request() {
    let context: TestContext = TestContext::new_with_versions(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs();

    // Request something that is not a Python version
    uv_snapshot!(context.filters(), context.python_install().arg("foobar"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: `foobar` is not a valid Python download request; see `uv help python` for supported formats and `uv python list --only-downloads` for available versions
    "###);

    // Request a version we don't have a download for
    uv_snapshot!(context.filters(), context.python_install().arg("3.8.0"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No download found for request: cpython-3.8.0-[PLATFORM]
    "###);

    // Request a version we don't have a download for mixed with one we do
    uv_snapshot!(context.filters(), context.python_install().arg("3.8.0").arg("3.12"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No download found for request: cpython-3.8.0-[PLATFORM]
    "###);
}

#[test]
fn python_install_default() {
    let context: TestContext = TestContext::new_with_versions(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs();

    let bin_python_minor_13 = context
        .bin_dir
        .child(format!("python3.13{}", std::env::consts::EXE_SUFFIX));

    let bin_python_major = context
        .bin_dir
        .child(format!("python3{}", std::env::consts::EXE_SUFFIX));

    let bin_python_default = context
        .bin_dir
        .child(format!("python{}", std::env::consts::EXE_SUFFIX));

    // `--preview` is required for `--default`
    uv_snapshot!(context.filters(), context.python_install().arg("--default"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    The `--default` flag is only available in preview mode; add the `--preview` flag to use `--default`
    "###);

    // Install a specific version
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.13"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.2 in [TIME]
     + cpython-3.13.2-[PLATFORM] (python3.13)
    "###);

    // Only the minor versioned executable should be installed
    bin_python_minor_13.assert(predicate::path::exists());
    bin_python_major.assert(predicate::path::missing());
    bin_python_default.assert(predicate::path::missing());

    // Install again, with `--default`
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("--default").arg("3.13"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.2 in [TIME]
     + cpython-3.13.2-[PLATFORM] (python, python3)
    "###);

    // Now all the executables should be installed
    bin_python_minor_13.assert(predicate::path::exists());
    bin_python_major.assert(predicate::path::exists());
    bin_python_default.assert(predicate::path::exists());

    // Uninstall
    uv_snapshot!(context.filters(), context.python_uninstall().arg("--all"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Searching for Python installations
    Uninstalled Python 3.13.2 in [TIME]
     - cpython-3.13.2-[PLATFORM] (python, python3, python3.13)
    "###);

    // The executables should be removed
    bin_python_minor_13.assert(predicate::path::missing());
    bin_python_major.assert(predicate::path::missing());
    bin_python_default.assert(predicate::path::missing());

    // Install the latest version, i.e., a "default install"
    uv_snapshot!(context.filters(), context.python_install().arg("--preview"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.2 in [TIME]
     + cpython-3.13.2-[PLATFORM] (python, python3, python3.13)
    "###);

    // Since it's a default install, we should include all of the executables
    bin_python_minor_13.assert(predicate::path::exists());
    bin_python_major.assert(predicate::path::exists());
    bin_python_default.assert(predicate::path::exists());

    // Uninstall again
    uv_snapshot!(context.filters(), context.python_uninstall().arg("3.13"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Searching for Python versions matching: Python 3.13
    Uninstalled Python 3.13.2 in [TIME]
     - cpython-3.13.2-[PLATFORM] (python, python3, python3.13)
    "###);

    // We should remove all the executables
    bin_python_minor_13.assert(predicate::path::missing());
    bin_python_major.assert(predicate::path::missing());
    bin_python_default.assert(predicate::path::missing());

    // Install multiple versions, with the `--default` flag
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.12").arg("3.13").arg("--default"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: The `--default` flag cannot be used with multiple targets
    "###);

    // Install 3.12 as a new default
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.12").arg("--default"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.12.9 in [TIME]
     + cpython-3.12.9-[PLATFORM] (python, python3, python3.12)
    "###);

    let bin_python_minor_12 = context
        .bin_dir
        .child(format!("python3.12{}", std::env::consts::EXE_SUFFIX));

    // All the executables should exist
    bin_python_minor_12.assert(predicate::path::exists());
    bin_python_major.assert(predicate::path::exists());
    bin_python_default.assert(predicate::path::exists());

    // And 3.12 should be the default
    if cfg!(unix) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link_path(&bin_python_major), @"[TEMP_DIR]/managed/cpython-3.12.9-[PLATFORM]/bin/python3.12"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link_path(&bin_python_minor_12), @"[TEMP_DIR]/managed/cpython-3.12.9-[PLATFORM]/bin/python3.12"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link_path(&bin_python_default), @"[TEMP_DIR]/managed/cpython-3.12.9-[PLATFORM]/bin/python3.12"
            );
        });
    } else {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link_path(&bin_python_major), @"[TEMP_DIR]/managed/cpython-3.12.9-[PLATFORM]/python"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link_path(&bin_python_minor_12), @"[TEMP_DIR]/managed/cpython-3.12.9-[PLATFORM]/python"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link_path(&bin_python_default), @"[TEMP_DIR]/managed/cpython-3.12.9-[PLATFORM]/python"
            );
        });
    }

    // Change the default to 3.13
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.13").arg("--default"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.2 in [TIME]
     + cpython-3.13.2-[PLATFORM] (python, python3, python3.13)
    "###);

    // All the executables should exist
    bin_python_minor_13.assert(predicate::path::exists());
    bin_python_minor_12.assert(predicate::path::exists());
    bin_python_major.assert(predicate::path::exists());
    bin_python_default.assert(predicate::path::exists());

    // And 3.13 should be the default now
    if cfg!(unix) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link_path(&bin_python_major), @"[TEMP_DIR]/managed/cpython-3.13.2-[PLATFORM]/bin/python3.13"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link_path(&bin_python_minor_13), @"[TEMP_DIR]/managed/cpython-3.13.2-[PLATFORM]/bin/python3.13"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link_path(&bin_python_minor_12), @"[TEMP_DIR]/managed/cpython-3.12.9-[PLATFORM]/bin/python3.12"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link_path(&bin_python_default), @"[TEMP_DIR]/managed/cpython-3.13.2-[PLATFORM]/bin/python3.13"
            );
        });
    } else {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link_path(&bin_python_major), @"[TEMP_DIR]/managed/cpython-3.13.2-[PLATFORM]/python"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link_path(&bin_python_minor_13), @"[TEMP_DIR]/managed/cpython-3.13.2-[PLATFORM]/python"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link_path(&bin_python_minor_12), @"[TEMP_DIR]/managed/cpython-3.12.9-[PLATFORM]/python"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link_path(&bin_python_default), @"[TEMP_DIR]/managed/cpython-3.13.2-[PLATFORM]/python"
            );
        });
    }
}

fn read_link_path(path: &Path) -> String {
    if cfg!(unix) {
        path.read_link()
            .unwrap_or_else(|_| panic!("{} should be readable", path.display()))
            .simplified_display()
            .to_string()
    } else if cfg!(windows) {
        let launcher = uv_trampoline_builder::Launcher::try_from_path(path)
            .ok()
            .unwrap_or_else(|| panic!("{} should be readable", path.display()))
            .unwrap_or_else(|| panic!("{} should be a valid launcher", path.display()));
        let path = launcher.python_path.simplified_display().to_string();
        path
    } else {
        unreachable!()
    }
}

#[test]
fn python_install_unknown() {
    let context: TestContext = TestContext::new_with_versions(&[]).with_managed_python_dirs();

    // An unknown request
    uv_snapshot!(context.filters(), context.python_install().arg("foobar"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: `foobar` is not a valid Python download request; see `uv help python` for supported formats and `uv python list --only-downloads` for available versions
    "###);

    context.temp_dir.child("foo").create_dir_all().unwrap();

    // A directory
    uv_snapshot!(context.filters(), context.python_install().arg("./foo"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: `./foo` is not a valid Python download request; see `uv help python` for supported formats and `uv python list --only-downloads` for available versions
    "###);
}

#[cfg(unix)]
#[test]
fn python_install_preview_broken_link() {
    use assert_fs::prelude::PathCreateDir;
    use fs_err::os::unix::fs::symlink;

    let context: TestContext = TestContext::new_with_versions(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs();

    let bin_python = context.bin_dir.child("python3.13");

    // Create a broken symlink
    context.bin_dir.create_dir_all().unwrap();
    symlink(context.temp_dir.join("does-not-exist"), &bin_python).unwrap();

    // Install
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.13"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.2 in [TIME]
     + cpython-3.13.2-[PLATFORM] (python3.13)
    "###);

    // We should replace the broken symlink
    insta::with_settings!({
        filters => context.filters(),
    }, {
        insta::assert_snapshot!(
            read_link_path(&bin_python), @"[TEMP_DIR]/managed/cpython-3.13.2-[PLATFORM]/bin/python3.13"
        );
    });
}

#[test]
fn python_install_default_from_env() {
    let context: TestContext = TestContext::new_with_versions(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs();

    // Install the version specified by the `UV_PYTHON` environment variable by default
    uv_snapshot!(context.filters(), context.python_install().env(EnvVars::UV_PYTHON, "3.12"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.12.9 in [TIME]
     + cpython-3.12.9-[PLATFORM]
    ");

    // But prefer explicit requests
    uv_snapshot!(context.filters(), context.python_install().arg("3.11").env(EnvVars::UV_PYTHON, "3.12"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.11.11 in [TIME]
     + cpython-3.11.11-[PLATFORM]
    "###);

    // We should ignore `UV_PYTHON` here and complain there is not a target
    uv_snapshot!(context.filters(), context.python_uninstall().env(EnvVars::UV_PYTHON, "3.12"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: the following required arguments were not provided:
      <TARGETS>...

    Usage: uv python uninstall --install-dir <INSTALL_DIR> <TARGETS>...

    For more information, try '--help'.
    "###);

    // We should ignore `UV_PYTHON` here and respect `--all`
    uv_snapshot!(context.filters(), context.python_uninstall().arg("--all").env(EnvVars::UV_PYTHON, "3.11"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Searching for Python installations
    Uninstalled 2 versions in [TIME]
     - cpython-3.11.11-[PLATFORM]
     - cpython-3.12.9-[PLATFORM]
    "###);

    // Uninstall with no targets should error
    uv_snapshot!(context.filters(), context.python_uninstall(), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: the following required arguments were not provided:
      <TARGETS>...

    Usage: uv python uninstall --install-dir <INSTALL_DIR> <TARGETS>...

    For more information, try '--help'.
    "###);

    // Uninstall with conflicting options should error
    uv_snapshot!(context.filters(), context.python_uninstall().arg("--all").arg("3.12"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: the argument '--all' cannot be used with '<TARGETS>...'

    Usage: uv python uninstall --all --install-dir <INSTALL_DIR> <TARGETS>...

    For more information, try '--help'.
    "###);
}

#[cfg(target_os = "macos")]
#[test]
fn python_install_patch_dylib() {
    use assert_cmd::assert::OutputAssertExt;
    use uv_python::managed::platform_key_from_env;

    let context: TestContext = TestContext::new_with_versions(&[])
        .with_filtered_python_keys()
        .with_managed_python_dirs();

    // Install the latest version
    context
        .python_install()
        .arg("--preview")
        .arg("3.13.1")
        .assert()
        .success();

    let dylib = context
        .temp_dir
        .child("managed")
        .child(format!(
            "cpython-3.13.1-{}",
            platform_key_from_env().unwrap()
        ))
        .child("lib")
        .child(format!(
            "{}python3.13{}",
            std::env::consts::DLL_PREFIX,
            std::env::consts::DLL_SUFFIX
        ));

    let mut cmd = std::process::Command::new("otool");
    cmd.arg("-D").arg(dylib.as_ref());

    uv_snapshot!(context.filters(), cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13.1-[PLATFORM]/lib/libpython3.13.dylib:
    [TEMP_DIR]/managed/cpython-3.13.1-[PLATFORM]/lib/libpython3.13.dylib

    ----- stderr -----
    "###);
}
