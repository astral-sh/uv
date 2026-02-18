#[cfg(windows)]
use std::path::PathBuf;

use std::{env, path::Path, process::Command};

use anyhow::Context;
use assert_cmd::assert::OutputAssertExt;
use assert_fs::{
    assert::PathAssert,
    prelude::{FileTouch, FileWriteStr, PathChild, PathCreateDir},
};
use indoc::indoc;
use predicates::prelude::predicate;
use tracing::debug;
use uv_test::uv_snapshot;

use uv_fs::Simplified;
use uv_python::managed::platform_key_from_env;
use uv_static::EnvVars;
use walkdir::WalkDir;

#[test]
fn python_install() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_filtered_latest_python_versions()
        .with_managed_python_dirs()
        .with_empty_python_install_mirror()
        .with_python_download_cache();

    // Install the latest version
    uv_snapshot!(context.filters(), context.python_install(), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.14.[LATEST] in [TIME]
     + cpython-3.14.[LATEST]-[PLATFORM] (python3.14)
    ");

    let bin_python = context
        .bin_dir
        .child(format!("python3.14{}", std::env::consts::EXE_SUFFIX));

    // The executable should be installed in the bin directory
    bin_python.assert(predicate::path::exists());

    // On Unix, it should be a link
    #[cfg(unix)]
    bin_python.assert(predicate::path::is_symlink());

    // The link should be a path to the binary
    if cfg!(unix) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python), @"[TEMP_DIR]/managed/cpython-3.14-[PLATFORM]/bin/python3.14"
            );
        });
    } else if cfg!(windows) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python), @"[TEMP_DIR]/managed/cpython-3.14-[PLATFORM]/python"
            );
        });
    }

    // The executable should "work"
    uv_snapshot!(context.filters(), Command::new(bin_python.as_os_str())
        .arg("-c").arg("import subprocess; print('hello world')"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    hello world

    ----- stderr -----
    ");

    // Should be a no-op when already installed
    uv_snapshot!(context.filters(), context.python_install(), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Python is already installed. Use `uv python install <request>` to install another version.
    ");

    // Similarly, when a requested version is already installed
    uv_snapshot!(context.filters(), context.python_install().arg("3.14"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Python 3.14 is already installed
    ");

    // You can opt-in to a reinstall
    uv_snapshot!(context.filters(), context.python_install().arg("3.14").arg("--reinstall"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.14.[LATEST] in [TIME]
     ~ cpython-3.14.[LATEST]-[PLATFORM] (python3.14)
    ");

    // The executable should still be present in the bin directory
    bin_python.assert(predicate::path::exists());

    // Uninstallation requires an argument
    uv_snapshot!(context.filters(), context.python_uninstall(), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: the following required arguments were not provided:
      <TARGETS>...

    Usage: uv python uninstall --cache-dir [CACHE_DIR] --install-dir <INSTALL_DIR> <TARGETS>...

    For more information, try '--help'.
    ");

    uv_snapshot!(context.filters(), context.python_uninstall().arg("3.14"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Searching for Python versions matching: Python 3.14
    Uninstalled Python 3.14.[LATEST] in [TIME]
     - cpython-3.14.[LATEST]-[PLATFORM] (python3.14)
    ");

    // The executable should be removed
    bin_python.assert(predicate::path::missing());
}

#[test]
fn python_reinstall() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_filtered_latest_python_versions()
        .with_managed_python_dirs()
        .with_python_download_cache();

    // Install a couple versions
    uv_snapshot!(context.filters(), context.python_install().arg("3.12").arg("3.13"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed 2 versions in [TIME]
     + cpython-3.12.[LATEST]-[PLATFORM] (python3.12)
     + cpython-3.13.[LATEST]-[PLATFORM] (python3.13)
    ");

    // Reinstall a single version
    uv_snapshot!(context.filters(), context.python_install().arg("3.13").arg("--reinstall"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.[LATEST] in [TIME]
     ~ cpython-3.13.[LATEST]-[PLATFORM] (python3.13)
    ");

    // Reinstall multiple versions
    uv_snapshot!(context.filters(), context.python_install().arg("--reinstall"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed 2 versions in [TIME]
     ~ cpython-3.12.[LATEST]-[PLATFORM] (python3.12)
     ~ cpython-3.13.[LATEST]-[PLATFORM] (python3.13)
    ");

    // Reinstalling a version that is not installed should also work
    uv_snapshot!(context.filters(), context.python_install().arg("3.11").arg("--reinstall"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.11.[LATEST] in [TIME]
     + cpython-3.11.[LATEST]-[PLATFORM] (python3.11)
    ");
}

#[test]
fn python_reinstall_patch() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs()
        .with_python_download_cache();

    // Install a couple patch versions
    uv_snapshot!(context.filters(), context.python_install().arg("3.12.6").arg("3.12.7"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed 2 versions in [TIME]
     + cpython-3.12.6-[PLATFORM]
     + cpython-3.12.7-[PLATFORM] (python3.12)
    ");

    // Reinstall all "3.12" versions
    // TODO(zanieb): This doesn't work today, because we need this to install the "latest" as there
    // is no workflow for `--upgrade` yet
    uv_snapshot!(context.filters(), context.python_install().arg("3.12").arg("--reinstall"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.12.12 in [TIME]
     + cpython-3.12.12-[PLATFORM] (python3.12)
    ");
}

#[test]
fn python_install_automatic() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_filtered_python_sources()
        .with_managed_python_dirs()
        .with_python_download_cache();

    // With downloads disabled, the automatic install should fail
    uv_snapshot!(context.filters(), context.run()
        .env_remove(EnvVars::VIRTUAL_ENV)
        .arg("--no-python-downloads")
        .arg("python").arg("-c").arg("import sys; print(sys.version_info[:2])"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No interpreter found in [PYTHON SOURCES]

    hint: A managed Python download is available, but Python downloads are set to 'never'
    ");

    // Otherwise, we should fetch the latest Python version
    uv_snapshot!(context.filters(), context.run()
        .env_remove(EnvVars::VIRTUAL_ENV)
        .arg("python").arg("-c").arg("import sys; print(sys.version_info[:2])"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    (3, 14)

    ----- stderr -----
    ");

    // Subsequently, we can use the interpreter even with downloads disabled
    uv_snapshot!(context.filters(), context.run()
        .env_remove(EnvVars::VIRTUAL_ENV)
        .arg("--no-python-downloads")
        .arg("python").arg("-c").arg("import sys; print(sys.version_info[:2])"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    (3, 14)

    ----- stderr -----
    ");

    // We should respect the Python request
    uv_snapshot!(context.filters(), context.run()
    .env_remove(EnvVars::VIRTUAL_ENV)
    .arg("-p").arg("3.12")
    .arg("python").arg("-c").arg("import sys; print(sys.version_info[:2])"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    (3, 12)

    ----- stderr -----
    ");

    // But some requests cannot be mapped to a download
    uv_snapshot!(context.filters(), context.run()
       .env_remove(EnvVars::VIRTUAL_ENV)
       .arg("-p").arg("foobar")
       .arg("python").arg("-c").arg("import sys; print(sys.version_info[:2])"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No interpreter found for executable name `foobar` in [PYTHON SOURCES]
    ");

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
            .env_remove(EnvVars::VIRTUAL_ENV)
            // In tests, we ignore `PATH` during Python discovery so we need to add the context `bin`
            .env(EnvVars::UV_TEST_PYTHON_PATH, context.bin_dir.as_os_str())
            .arg("-p").arg("3.11")
            .arg("python").arg("-c").arg("import sys; print(sys.version_info[:2])"), @"
        success: true
        exit_code: 0
        ----- stdout -----
        (3, 11)

        ----- stderr -----
        ");
    }
}

/// Regression test for a bad cpython runtime
/// <https://github.com/astral-sh/uv/issues/13610>
#[test]
fn regression_cpython() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_filtered_python_sources()
        .with_managed_python_dirs()
        .with_python_download_cache();

    let init = context.temp_dir.child("mre.py");
    init.write_str(indoc! { r#"
        class Foo(str): ...

        a = []
        new_value = Foo("1")
        a += new_value
        "#
    })
    .unwrap();

    // We should respect the Python request
    uv_snapshot!(context.filters(), context.run()
        .env_remove(EnvVars::VIRTUAL_ENV)
        .arg("-p").arg("3.12")
        .arg("mre.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    ");
}

#[test]
fn python_install_force() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_filtered_latest_python_versions()
        .with_managed_python_dirs();

    // Install the latest version
    uv_snapshot!(context.filters(), context.python_install(), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.14.[LATEST] in [TIME]
     + cpython-3.14.[LATEST]-[PLATFORM] (python3.14)
    ");

    let bin_python = context
        .bin_dir
        .child(format!("python3.14{}", std::env::consts::EXE_SUFFIX));

    // You can force replacement of the executables
    uv_snapshot!(context.filters(), context.python_install().arg("--force"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.14.[LATEST] in [TIME]
     + cpython-3.14.[LATEST]-[PLATFORM] (python3.14)
    ");

    // The executable should still be present in the bin directory
    bin_python.assert(predicate::path::exists());

    // If an unmanaged executable is present, `--force` is required
    fs_err::remove_file(bin_python.path()).unwrap();
    bin_python.touch().unwrap();

    uv_snapshot!(context.filters(), context.python_install().arg("3.14"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: Failed to install executable for cpython-3.14.[LATEST]-[PLATFORM]
      Caused by: Executable already exists at `[BIN]/python3.14` but is not managed by uv; use `--force` to replace it
    ");

    uv_snapshot!(context.filters(), context.python_install().arg("--force").arg("3.14"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.14.[LATEST] in [TIME]
     + cpython-3.14.[LATEST]-[PLATFORM] (python3.14)
    ");

    bin_python.assert(predicate::path::exists());
}

#[test]
fn python_install_minor() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_filtered_latest_python_versions()
        .with_managed_python_dirs();

    // Install a minor version
    uv_snapshot!(context.filters(), context.python_install().arg("3.11"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.11.[LATEST] in [TIME]
     + cpython-3.11.[LATEST]-[PLATFORM] (python3.11)
    ");

    let bin_python = context
        .bin_dir
        .child(format!("python3.11{}", std::env::consts::EXE_SUFFIX));

    // The executable should be installed in the bin directory
    bin_python.assert(predicate::path::exists());

    // It should be a link to the minor version
    if cfg!(unix) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python), @"[TEMP_DIR]/managed/cpython-3.11-[PLATFORM]/bin/python3.11"
            );
        });
    } else if cfg!(windows) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python), @"[TEMP_DIR]/managed/cpython-3.11-[PLATFORM]/python"
            );
        });
    }

    uv_snapshot!(context.filters(), context.python_uninstall().arg("3.11"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Searching for Python versions matching: Python 3.11
    Uninstalled Python 3.11.[LATEST] in [TIME]
     - cpython-3.11.[LATEST]-[PLATFORM] (python3.11)
    ");

    // The executable should be removed
    bin_python.assert(predicate::path::missing());
}

#[test]
fn python_install_multiple_patch() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs();

    // Install multiple patch versions
    uv_snapshot!(context.filters(), context.python_install().arg("3.12.8").arg("3.12.6"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed 2 versions in [TIME]
     + cpython-3.12.6-[PLATFORM]
     + cpython-3.12.8-[PLATFORM] (python3.12)
    ");

    let bin_python = context
        .bin_dir
        .child(format!("python3.12{}", std::env::consts::EXE_SUFFIX));

    // The executable should be installed in the bin directory
    bin_python.assert(predicate::path::exists());

    // The link should resolve to the newer patch version
    if cfg!(unix) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python), @"[TEMP_DIR]/managed/cpython-3.12.8-[PLATFORM]/bin/python3.12"
            );
        });
    } else if cfg!(windows) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python), @"[TEMP_DIR]/managed/cpython-3.12.8-[PLATFORM]/python"
            );
        });
    }

    uv_snapshot!(context.filters(), context.python_uninstall().arg("3.12.8"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Searching for Python versions matching: Python 3.12.8
    Uninstalled Python 3.12.8 in [TIME]
     - cpython-3.12.8-[PLATFORM] (python3.12)
    ");

    // TODO(zanieb): This behavior is not implemented yet
    // // The executable should be installed in the bin directory
    // bin_python.assert(predicate::path::exists());

    // // When the version is removed, the link should point to the other patch version
    // if cfg!(unix) {
    //     insta::with_settings!({
    //         filters => context.filters(),
    //     }, {
    //         insta::assert_snapshot!(
    //             canonicalize_link_path(&bin_python), @"[TEMP_DIR]/managed/cpython-3.12.6-[PLATFORM]/bin/python3.12"
    //         );
    //     });
    // } else if cfg!(windows) {
    //     insta::with_settings!({
    //         filters => context.filters(),
    //     }, {
    //         insta::assert_snapshot!(
    //             canonicalize_link_path(&bin_python), @"[TEMP_DIR]/managed/cpython-3.12.6-[PLATFORM]/python"
    //         );
    //     });
    // }
}

#[test]
fn python_install_preview() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_filtered_latest_python_versions()
        .with_managed_python_dirs()
        .with_python_download_cache();

    // Install the latest version
    uv_snapshot!(context.filters(), context.python_install().arg("--preview"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.14.[LATEST] in [TIME]
     + cpython-3.14.[LATEST]-[PLATFORM] (python, python3, python3.14)
    ");

    let bin_python = context
        .bin_dir
        .child(format!("python3.14{}", std::env::consts::EXE_SUFFIX));

    // The executable should be installed in the bin directory
    bin_python.assert(predicate::path::exists());

    // On Unix, it should be a link
    #[cfg(unix)]
    bin_python.assert(predicate::path::is_symlink());

    // The link should be to a path containing a minor version symlink directory
    if cfg!(unix) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python), @"[TEMP_DIR]/managed/cpython-3.14-[PLATFORM]/bin/python3.14"
            );
        });
    } else if cfg!(windows) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python), @"[TEMP_DIR]/managed/cpython-3.14-[PLATFORM]/python"
            );
        });
    }

    // The executable should "work"
    uv_snapshot!(context.filters(), Command::new(bin_python.as_os_str())
        .arg("-c").arg("import subprocess; print('hello world')"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    hello world

    ----- stderr -----
    ");

    // Should be a no-op when already installed
    uv_snapshot!(context.filters(), context.python_install().arg("--preview"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Python is already installed. Use `uv python install <request>` to install another version.
    ");

    // You can opt-in to a reinstall
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("--reinstall"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.14.[LATEST] in [TIME]
     ~ cpython-3.14.[LATEST]-[PLATFORM] (python, python3, python3.14)
    ");

    // The executable should still be present in the bin directory
    bin_python.assert(predicate::path::exists());

    // You can also force replacement of the executables
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("--force"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.14.[LATEST] in [TIME]
     + cpython-3.14.[LATEST]-[PLATFORM] (python, python3, python3.14)
    ");

    // The executable should still be present in the bin directory
    bin_python.assert(predicate::path::exists());

    // If an unmanaged executable is present, `--force` is required
    fs_err::remove_file(bin_python.path()).unwrap();
    bin_python.touch().unwrap();

    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.14"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: Failed to install executable for cpython-3.14.[LATEST]-[PLATFORM]
      Caused by: Executable already exists at `[BIN]/python3.14` but is not managed by uv; use `--force` to replace it
    ");

    // With `--bin`, this should error instead of warn
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("--bin").arg("3.14"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    error: Failed to install executable for cpython-3.14.[LATEST]-[PLATFORM]
      Caused by: Executable already exists at `[BIN]/python3.14` but is not managed by uv; use `--force` to replace it
    ");
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.14").env(EnvVars::UV_PYTHON_INSTALL_BIN, "1"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    error: Failed to install executable for cpython-3.14.[LATEST]-[PLATFORM]
      Caused by: Executable already exists at `[BIN]/python3.14` but is not managed by uv; use `--force` to replace it
    ");

    // With `--no-bin`, this should be silent
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("--no-bin").arg("3.14"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Python 3.14 is already installed
    ");
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.14").env(EnvVars::UV_PYTHON_INSTALL_BIN, "0"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Python 3.14 is already installed
    ");

    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("--force").arg("3.14"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.14.[LATEST] in [TIME]
     + cpython-3.14.[LATEST]-[PLATFORM] (python3.14)
    ");

    bin_python.assert(predicate::path::exists());

    // On Unix, it should be a link
    #[cfg(unix)]
    bin_python.assert(predicate::path::is_symlink());

    // Uninstallation requires an argument
    uv_snapshot!(context.filters(), context.python_uninstall(), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: the following required arguments were not provided:
      <TARGETS>...

    Usage: uv python uninstall --cache-dir [CACHE_DIR] --install-dir <INSTALL_DIR> <TARGETS>...

    For more information, try '--help'.
    ");

    uv_snapshot!(context.filters(), context.python_uninstall().arg("3.14"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Searching for Python versions matching: Python 3.14
    Uninstalled Python 3.14.[LATEST] in [TIME]
     - cpython-3.14.[LATEST]-[PLATFORM] (python, python3, python3.14)
    ");

    // The executable should be removed
    bin_python.assert(predicate::path::missing());

    // Install a minor version
    uv_snapshot!(context.filters(), context.python_install().arg("3.11").arg("--preview"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.11.[LATEST] in [TIME]
     + cpython-3.11.[LATEST]-[PLATFORM] (python3.11)
    ");

    let bin_python = context
        .bin_dir
        .child(format!("python3.11{}", std::env::consts::EXE_SUFFIX));

    // The link should be to a path containing a minor version symlink directory
    if cfg!(unix) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python), @"[TEMP_DIR]/managed/cpython-3.11-[PLATFORM]/bin/python3.11"
            );
        });
    } else if cfg!(windows) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python), @"[TEMP_DIR]/managed/cpython-3.11-[PLATFORM]/python"
            );
        });
    }

    uv_snapshot!(context.filters(), context.python_uninstall().arg("3.11"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Searching for Python versions matching: Python 3.11
    Uninstalled Python 3.11.[LATEST] in [TIME]
     - cpython-3.11.[LATEST]-[PLATFORM] (python3.11)
    ");

    // Install multiple patch versions
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.12.8").arg("3.12.6"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed 2 versions in [TIME]
     + cpython-3.12.6-[PLATFORM]
     + cpython-3.12.8-[PLATFORM] (python3.12)
    ");

    let bin_python = context
        .bin_dir
        .child(format!("python3.12{}", std::env::consts::EXE_SUFFIX));

    // The link should resolve to the newer patch version
    if cfg!(unix) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python), @"[TEMP_DIR]/managed/cpython-3.12.8-[PLATFORM]/bin/python3.12"
            );
        });
    } else if cfg!(windows) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python), @"[TEMP_DIR]/managed/cpython-3.12.8-[PLATFORM]/python"
            );
        });
    }
}

#[test]
fn python_install_preview_no_bin() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_filtered_latest_python_versions()
        .with_managed_python_dirs()
        .with_python_download_cache();

    // Install the latest version
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("--no-bin"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.14.[LATEST] in [TIME]
     + cpython-3.14.[LATEST]-[PLATFORM]
    ");

    let bin_python = context
        .bin_dir
        .child(format!("python3.14{}", std::env::consts::EXE_SUFFIX));

    // The executable should not be installed in the bin directory
    bin_python.assert(predicate::path::missing());

    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("--no-bin").arg("--default"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: the argument '--no-bin' cannot be used with '--default'

    Usage: uv python install --cache-dir [CACHE_DIR] --no-bin --install-dir <INSTALL_DIR> [TARGETS]...

    For more information, try '--help'.
    ");

    let bin_python = context
        .bin_dir
        .child(format!("python{}", std::env::consts::EXE_SUFFIX));

    // The executable should not be installed in the bin directory
    bin_python.assert(predicate::path::missing());
}

#[test]
fn python_install_preview_upgrade() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs()
        .with_python_download_cache();

    let bin_python = context
        .bin_dir
        .child(format!("python3.12{}", std::env::consts::EXE_SUFFIX));

    // Install 3.12.5
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.12.5"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.12.5 in [TIME]
     + cpython-3.12.5-[PLATFORM] (python3.12)
    ");

    // Installing with a patch version should cause the link to be to the patch installation.
    if cfg!(unix) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python), @"[TEMP_DIR]/managed/cpython-3.12.5-[PLATFORM]/bin/python3.12"
            );
        });
    } else if cfg!(windows) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python), @"[TEMP_DIR]/managed/cpython-3.12.5-[PLATFORM]/python"
            );
        });
    }

    // Installing 3.12.4 should not replace the executable, but also shouldn't fail
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.12.4"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.12.4 in [TIME]
     + cpython-3.12.4-[PLATFORM]
    ");

    if cfg!(unix) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python), @"[TEMP_DIR]/managed/cpython-3.12.5-[PLATFORM]/bin/python3.12"
            );
        });
    } else if cfg!(windows) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python), @"[TEMP_DIR]/managed/cpython-3.12.5-[PLATFORM]/python"
            );
        });
    }

    // Using `--reinstall` is not sufficient to replace it either
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.12.4").arg("--reinstall"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.12.4 in [TIME]
     ~ cpython-3.12.4-[PLATFORM]
    ");

    if cfg!(unix) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python), @"[TEMP_DIR]/managed/cpython-3.12.5-[PLATFORM]/bin/python3.12"
            );
        });
    } else if cfg!(windows) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python), @"[TEMP_DIR]/managed/cpython-3.12.5-[PLATFORM]/python"
            );
        });
    }

    // But `--force` is
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.12.4").arg("--force"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.12.4 in [TIME]
     + cpython-3.12.4-[PLATFORM] (python3.12)
    ");

    if cfg!(unix) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python), @"[TEMP_DIR]/managed/cpython-3.12.4-[PLATFORM]/bin/python3.12"
            );
        });
    } else if cfg!(windows) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python), @"[TEMP_DIR]/managed/cpython-3.12.4-[PLATFORM]/python"
            );
        });
    }

    // But installing 3.12.6 should upgrade automatically
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.12.6"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.12.6 in [TIME]
     + cpython-3.12.6-[PLATFORM] (python3.12)
    ");

    if cfg!(unix) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python), @"[TEMP_DIR]/managed/cpython-3.12.6-[PLATFORM]/bin/python3.12"
            );
        });
    } else if cfg!(windows) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python), @"[TEMP_DIR]/managed/cpython-3.12.6-[PLATFORM]/python"
            );
        });
    }
}

#[test]
fn python_install_freethreaded() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_latest_python_versions()
        .with_managed_python_dirs()
        .with_python_download_cache()
        .with_filtered_python_install_bin()
        .with_filtered_python_names()
        .with_filtered_exe_suffix();

    // Install the latest version
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.13t"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.[LATEST] in [TIME]
     + cpython-3.13.[LATEST]+freethreaded-[PLATFORM] (python3.13t)
    ");

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
        .arg("-c").arg("import subprocess; print('hello world')"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    hello world

    ----- stderr -----
    ");

    // We should find it with opt-in
    uv_snapshot!(context.filters(), context.python_find().arg("3.13t"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13+freethreaded-[PLATFORM]/[INSTALL-BIN]/[PYTHON]

    ----- stderr -----
    ");

    // We should be able to select it with `+freethreaded`
    uv_snapshot!(context.filters(), context.python_find().arg("3.13+freethreaded"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13+freethreaded-[PLATFORM]/[INSTALL-BIN]/[PYTHON]

    ----- stderr -----
    ");

    // Create a virtual environment with the freethreaded Python
    uv_snapshot!(context.filters(), context.venv().arg("--python").arg("3.13t"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.13.[LATEST]+freethreaded
    Creating virtual environment at: .venv
    Activate with: source .venv/[BIN]/activate
    ");

    // `python`, `python3`, `python3.13`, and `python3.13t` should all be present
    let scripts = context
        .venv
        .join(if cfg!(windows) { "Scripts" } else { "bin" });
    assert!(
        scripts
            .join(format!("python{}", std::env::consts::EXE_SUFFIX))
            .exists()
    );

    #[cfg(windows)]
    assert!(
        scripts
            .join(format!("pythonw{}", std::env::consts::EXE_SUFFIX))
            .exists()
    );

    #[cfg(unix)]
    assert!(
        scripts
            .join(format!("python3{}", std::env::consts::EXE_SUFFIX))
            .exists()
    );

    #[cfg(unix)]
    assert!(
        scripts
            .join(format!("python3.13{}", std::env::consts::EXE_SUFFIX))
            .exists()
    );

    assert!(
        scripts
            .join(format!("python3.13t{}", std::env::consts::EXE_SUFFIX))
            .exists()
    );

    #[cfg(windows)]
    assert!(
        scripts
            .join(format!("pythonw3.13t{}", std::env::consts::EXE_SUFFIX))
            .exists()
    );

    // Remove the virtual environment
    fs_err::remove_dir_all(&context.venv).unwrap();

    // Should be distinct from 3.13
    uv_snapshot!(context.filters(), context.python_install().arg("3.13"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.[LATEST] in [TIME]
     + cpython-3.13.[LATEST]-[PLATFORM] (python3.13)
    ");

    // Should not work with older Python versions
    uv_snapshot!(context.filters(), context.python_install().arg("3.12t"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No download found for request: cpython-3.12+freethreaded-[PLATFORM]
    ");

    uv_snapshot!(context.filters(), context.python_uninstall().arg("--all"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Searching for Python installations
    Uninstalled 2 versions in [TIME]
     - cpython-3.13.[LATEST]+freethreaded-[PLATFORM] (python3.13t)
     - cpython-3.13.[LATEST]-[PLATFORM] (python3.13)
    ");
}

#[test]
fn python_upgrade_not_allowed() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs();

    // Request a patch upgrade
    uv_snapshot!(context.filters(), context.python_upgrade().arg("--preview").arg("3.13.0"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    error: `uv python upgrade` only accepts minor versions, got: 3.13.0
    ");

    // Request a pre-release upgrade
    uv_snapshot!(context.filters(), context.python_upgrade().arg("--preview").arg("3.14rc3"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    error: `uv python upgrade` only accepts minor versions, got: 3.14rc3
    ");
}

// We only support debug builds on Unix
#[cfg(unix)]
#[test]
fn python_install_debug() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_filtered_latest_python_versions()
        .with_managed_python_dirs();

    // Install the latest version
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.13+debug"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.[LATEST] in [TIME]
     + cpython-3.13.[LATEST]+debug-[PLATFORM] (python3.13d)
    ");

    let bin_python = context
        .bin_dir
        .child(format!("python3.13d{}", std::env::consts::EXE_SUFFIX));

    // The executable should be installed in the bin directory
    bin_python.assert(predicate::path::exists());

    // On Unix, it should be a link
    #[cfg(unix)]
    bin_python.assert(predicate::path::is_symlink());

    // The executable should "work"
    uv_snapshot!(context.filters(), Command::new(bin_python.as_os_str())
        .arg("-c").arg("import subprocess; print('hello world')"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    hello world

    ----- stderr -----
    ");

    // We should find it with opt-in
    uv_snapshot!(context.filters(), context.python_find().arg("3.13d"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13+debug-[PLATFORM]/bin/python3.13d

    ----- stderr -----
    ");

    // We should find it without opt-in
    uv_snapshot!(context.filters(), context.python_find().arg("3.13"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13+debug-[PLATFORM]/bin/python3.13d

    ----- stderr -----
    ");

    // Should be distinct from 3.13
    uv_snapshot!(context.filters(), context.python_install().arg("3.13"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.[LATEST] in [TIME]
     + cpython-3.13.[LATEST]-[PLATFORM] (python3.13)
    ");

    // Now we should prefer the non-debug version without opt-in
    uv_snapshot!(context.filters(), context.python_find().arg("3.13"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13-[PLATFORM]/bin/python3.13

    ----- stderr -----
    ");

    // But still select it with opt-in
    uv_snapshot!(context.filters(), context.python_find().arg("3.13d"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13+debug-[PLATFORM]/bin/python3.13d

    ----- stderr -----
    ");

    // We should allow selection with `+debug`
    uv_snapshot!(context.filters(), context.python_find().arg("3.13+debug"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13+debug-[PLATFORM]/bin/python3.13d

    ----- stderr -----
    ");

    // Should work with older Python versions too
    uv_snapshot!(context.filters(), context.python_install().arg("3.12d"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.12.[LATEST] in [TIME]
     + cpython-3.12.[LATEST]+debug-[PLATFORM] (python3.12d)
    ");

    uv_snapshot!(context.filters(), context.python_uninstall().arg("--all"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Searching for Python installations
    Uninstalled 3 versions in [TIME]
     - cpython-3.12.[LATEST]+debug-[PLATFORM] (python3.12d)
     - cpython-3.13.[LATEST]+debug-[PLATFORM] (python3.13d)
     - cpython-3.13.[LATEST]-[PLATFORM] (python3.13)
    ");
}

// We only support debug builds on Unix
#[cfg(unix)]
#[test]
fn python_install_debug_freethreaded() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_filtered_latest_python_versions()
        .with_managed_python_dirs()
        .with_python_download_cache();

    // Install the latest version
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.13td"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.[LATEST] in [TIME]
     + cpython-3.13.[LATEST]+freethreaded+debug-[PLATFORM] (python3.13td)
    ");

    let bin_python = context
        .bin_dir
        .child(format!("python3.13td{}", std::env::consts::EXE_SUFFIX));

    // The executable should be installed in the bin directory
    bin_python.assert(predicate::path::exists());

    // On Unix, it should be a link
    #[cfg(unix)]
    bin_python.assert(predicate::path::is_symlink());

    // The executable should "work"
    uv_snapshot!(context.filters(), Command::new(bin_python.as_os_str())
        .arg("-c").arg("import subprocess; print('hello world')"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    hello world

    ----- stderr -----
    ");

    // We should find it with opt-in
    uv_snapshot!(context.filters(), context.python_find().arg("3.13td"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13+freethreaded+debug-[PLATFORM]/bin/python3.13td

    ----- stderr -----
    ");

    // We should not find it without opt-in
    uv_snapshot!(context.filters(), context.python_find().arg("3.13"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No interpreter found for Python 3.13 in virtual environments, managed installations, or search path
    ");

    // We should allow selection with `+freethread+debug`
    // TODO(zanieb): We don't support this yet
    uv_snapshot!(context.filters(), context.python_find().arg("3.13+freethreaded+debug"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13+freethreaded+debug-[PLATFORM]/bin/python3.13td

    ----- stderr -----
    ");

    // Should be distinct from 3.13
    uv_snapshot!(context.filters(), context.python_install().arg("3.13"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.[LATEST] in [TIME]
     + cpython-3.13.[LATEST]-[PLATFORM] (python3.13)
    ");

    // Should be distinct from 3.13t
    uv_snapshot!(context.filters(), context.python_install().arg("3.13t"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.[LATEST] in [TIME]
     + cpython-3.13.[LATEST]+freethreaded-[PLATFORM] (python3.13t)
    ");

    // Should be distinct from 3.13d
    uv_snapshot!(context.filters(), context.python_install().arg("3.13d"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.[LATEST] in [TIME]
     + cpython-3.13.[LATEST]+debug-[PLATFORM] (python3.13d)
    ");

    // Now we should prefer the non-debug version without opt-in
    uv_snapshot!(context.filters(), context.python_find().arg("3.13"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13-[PLATFORM]/bin/python3.13

    ----- stderr -----
    ");

    uv_snapshot!(context.filters(), context.python_find().arg("3.13t"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13+freethreaded-[PLATFORM]/bin/python3.13t

    ----- stderr -----
    ");

    // But still select it with opt-in
    uv_snapshot!(context.filters(), context.python_find().arg("3.13td"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13+freethreaded+debug-[PLATFORM]/bin/python3.13td

    ----- stderr -----
    ");

    uv_snapshot!(context.filters(), context.python_uninstall().arg("--all"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Searching for Python installations
    Uninstalled 4 versions in [TIME]
     - cpython-3.13.[LATEST]+freethreaded+debug-[PLATFORM] (python3.13td)
     - cpython-3.13.[LATEST]+freethreaded-[PLATFORM] (python3.13t)
     - cpython-3.13.[LATEST]+debug-[PLATFORM] (python3.13d)
     - cpython-3.13.[LATEST]-[PLATFORM] (python3.13)
    ");
}

#[test]
fn python_install_invalid_request() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs()
        .with_python_download_cache();

    // Request something that is not a Python version
    uv_snapshot!(context.filters(), context.python_install().arg("foobar"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: `foobar` is not a valid Python download request; see `uv help python` for supported formats and `uv python list --only-downloads` for available versions
    ");

    // Request a version we don't have a download for
    uv_snapshot!(context.filters(), context.python_install().arg("3.8.0"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No download found for request: cpython-3.8.0-[PLATFORM]
    ");

    // Request a version we don't have a download for mixed with one we do
    uv_snapshot!(context.filters(), context.python_install().arg("3.8.0").arg("3.12"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No download found for request: cpython-3.8.0-[PLATFORM]
    ");
}

#[test]
fn python_install_default() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_filtered_latest_python_versions()
        .with_managed_python_dirs()
        .with_python_download_cache();

    let bin_python_minor_14 = context
        .bin_dir
        .child(format!("python3.14{}", std::env::consts::EXE_SUFFIX));

    let bin_python_major = context
        .bin_dir
        .child(format!("python3{}", std::env::consts::EXE_SUFFIX));

    let bin_python_default = context
        .bin_dir
        .child(format!("python{}", std::env::consts::EXE_SUFFIX));

    // Install a specific version
    uv_snapshot!(context.filters(), context.python_install().arg("3.14"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.14.[LATEST] in [TIME]
     + cpython-3.14.[LATEST]-[PLATFORM] (python3.14)
    ");

    // Only the minor versioned executable should be installed
    bin_python_minor_14.assert(predicate::path::exists());
    bin_python_major.assert(predicate::path::missing());
    bin_python_default.assert(predicate::path::missing());

    // Install again, with `--default`
    uv_snapshot!(context.filters(), context.python_install().arg("--default").arg("3.14"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: The `--default` option is experimental and may change without warning. Pass `--preview-features python-install-default` to disable this warning
    Installed Python 3.14.[LATEST] in [TIME]
     + cpython-3.14.[LATEST]-[PLATFORM] (python, python3)
    ");

    // Now all the executables should be installed
    bin_python_minor_14.assert(predicate::path::exists());
    bin_python_major.assert(predicate::path::exists());
    bin_python_default.assert(predicate::path::exists());

    // Uninstall
    uv_snapshot!(context.filters(), context.python_uninstall().arg("--all"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Searching for Python installations
    Uninstalled Python 3.14.[LATEST] in [TIME]
     - cpython-3.14.[LATEST]-[PLATFORM] (python, python3, python3.14)
    ");

    // The executables should be removed
    bin_python_minor_14.assert(predicate::path::missing());
    bin_python_major.assert(predicate::path::missing());
    bin_python_default.assert(predicate::path::missing());

    // Install the latest version, i.e., a "default install"
    uv_snapshot!(context.filters(), context.python_install().arg("--default"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: The `--default` option is experimental and may change without warning. Pass `--preview-features python-install-default` to disable this warning
    Installed Python 3.14.[LATEST] in [TIME]
     + cpython-3.14.[LATEST]-[PLATFORM] (python, python3, python3.14)
    ");

    // Since it's a default install, we should include all of the executables
    bin_python_minor_14.assert(predicate::path::exists());
    bin_python_major.assert(predicate::path::exists());
    bin_python_default.assert(predicate::path::exists());

    // And 3.14 should be the default
    if cfg!(unix) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_major), @"[TEMP_DIR]/managed/cpython-3.14-[PLATFORM]/bin/python3.14"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_minor_14), @"[TEMP_DIR]/managed/cpython-3.14-[PLATFORM]/bin/python3.14"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_default), @"[TEMP_DIR]/managed/cpython-3.14-[PLATFORM]/bin/python3.14"
            );
        });
    } else if cfg!(windows) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_major), @"[TEMP_DIR]/managed/cpython-3.14-[PLATFORM]/python"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_minor_14), @"[TEMP_DIR]/managed/cpython-3.14-[PLATFORM]/python"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_default), @"[TEMP_DIR]/managed/cpython-3.14-[PLATFORM]/python"
            );
        });
    }

    // Uninstall again
    uv_snapshot!(context.filters(), context.python_uninstall().arg("3.14"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Searching for Python versions matching: Python 3.14
    Uninstalled Python 3.14.[LATEST] in [TIME]
     - cpython-3.14.[LATEST]-[PLATFORM] (python, python3, python3.14)
    ");

    // We should remove all the executables
    bin_python_minor_14.assert(predicate::path::missing());
    bin_python_major.assert(predicate::path::missing());
    bin_python_default.assert(predicate::path::missing());

    // Install multiple versions, with the `--default` flag
    uv_snapshot!(context.filters(), context.python_install().arg("3.12").arg("3.14").arg("--default"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: The `--default` option is experimental and may change without warning. Pass `--preview-features python-install-default` to disable this warning
    error: The `--default` flag cannot be used with multiple targets
    ");

    // Install 3.12 as a new default
    uv_snapshot!(context.filters(), context.python_install().arg("3.12").arg("--default"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: The `--default` option is experimental and may change without warning. Pass `--preview-features python-install-default` to disable this warning
    Installed Python 3.12.[LATEST] in [TIME]
     + cpython-3.12.[LATEST]-[PLATFORM] (python, python3, python3.12)
    ");

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
                read_link(&bin_python_major), @"[TEMP_DIR]/managed/cpython-3.12-[PLATFORM]/bin/python3.12"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_minor_12), @"[TEMP_DIR]/managed/cpython-3.12-[PLATFORM]/bin/python3.12"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_default), @"[TEMP_DIR]/managed/cpython-3.12-[PLATFORM]/bin/python3.12"
            );
        });
    } else {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_major), @"[TEMP_DIR]/managed/cpython-3.12-[PLATFORM]/python"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_minor_12), @"[TEMP_DIR]/managed/cpython-3.12-[PLATFORM]/python"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_default), @"[TEMP_DIR]/managed/cpython-3.12-[PLATFORM]/python"
            );
        });
    }
}

#[test]
fn python_install_default_preview() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_filtered_latest_python_versions()
        .with_managed_python_dirs()
        .with_python_download_cache();

    let bin_python_minor_14 = context
        .bin_dir
        .child(format!("python3.14{}", std::env::consts::EXE_SUFFIX));

    let bin_python_major = context
        .bin_dir
        .child(format!("python3{}", std::env::consts::EXE_SUFFIX));

    let bin_python_default = context
        .bin_dir
        .child(format!("python{}", std::env::consts::EXE_SUFFIX));

    // Install a specific version
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.14"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.14.[LATEST] in [TIME]
     + cpython-3.14.[LATEST]-[PLATFORM] (python3.14)
    ");

    // Only the minor versioned executable should be installed
    bin_python_minor_14.assert(predicate::path::exists());
    bin_python_major.assert(predicate::path::missing());
    bin_python_default.assert(predicate::path::missing());

    // Install again, with `--default`
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("--default").arg("3.14"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.14.[LATEST] in [TIME]
     + cpython-3.14.[LATEST]-[PLATFORM] (python, python3)
    ");

    // Now all the executables should be installed
    bin_python_minor_14.assert(predicate::path::exists());
    bin_python_major.assert(predicate::path::exists());
    bin_python_default.assert(predicate::path::exists());

    // Uninstall
    uv_snapshot!(context.filters(), context.python_uninstall().arg("--all"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Searching for Python installations
    Uninstalled Python 3.14.[LATEST] in [TIME]
     - cpython-3.14.[LATEST]-[PLATFORM] (python, python3, python3.14)
    ");

    // The executables should be removed
    bin_python_minor_14.assert(predicate::path::missing());
    bin_python_major.assert(predicate::path::missing());
    bin_python_default.assert(predicate::path::missing());

    // Install the latest version, i.e., a "default install"
    uv_snapshot!(context.filters(), context.python_install().arg("--default").arg("--preview"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.14.[LATEST] in [TIME]
     + cpython-3.14.[LATEST]-[PLATFORM] (python, python3, python3.14)
    ");

    // Since it's a default install, we should include all of the executables
    bin_python_minor_14.assert(predicate::path::exists());
    bin_python_major.assert(predicate::path::exists());
    bin_python_default.assert(predicate::path::exists());

    // And 3.14 should be the default
    if cfg!(unix) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_major), @"[TEMP_DIR]/managed/cpython-3.14-[PLATFORM]/bin/python3.14"
            );
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python_major), @"[TEMP_DIR]/managed/cpython-3.14.[LATEST]-[PLATFORM]/bin/python3.14"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_minor_14), @"[TEMP_DIR]/managed/cpython-3.14-[PLATFORM]/bin/python3.14"
            );
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python_minor_14), @"[TEMP_DIR]/managed/cpython-3.14.[LATEST]-[PLATFORM]/bin/python3.14"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_default), @"[TEMP_DIR]/managed/cpython-3.14-[PLATFORM]/bin/python3.14"
            );
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python_default), @"[TEMP_DIR]/managed/cpython-3.14.[LATEST]-[PLATFORM]/bin/python3.14"
            );
        });
    } else if cfg!(windows) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_major), @"[TEMP_DIR]/managed/cpython-3.14-[PLATFORM]/python"
            );
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python_major), @"[TEMP_DIR]/managed/cpython-3.14.[LATEST]-[PLATFORM]/python"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_minor_14), @"[TEMP_DIR]/managed/cpython-3.14-[PLATFORM]/python"
            );
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python_minor_14), @"[TEMP_DIR]/managed/cpython-3.14.[LATEST]-[PLATFORM]/python"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_default), @"[TEMP_DIR]/managed/cpython-3.14-[PLATFORM]/python"
            );
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python_default), @"[TEMP_DIR]/managed/cpython-3.14.[LATEST]-[PLATFORM]/python"
            );
        });
    }

    // Uninstall again
    uv_snapshot!(context.filters(), context.python_uninstall().arg("3.14"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Searching for Python versions matching: Python 3.14
    Uninstalled Python 3.14.[LATEST] in [TIME]
     - cpython-3.14.[LATEST]-[PLATFORM] (python, python3, python3.14)
    ");

    // We should remove all the executables
    bin_python_minor_14.assert(predicate::path::missing());
    bin_python_major.assert(predicate::path::missing());
    bin_python_default.assert(predicate::path::missing());

    // Install multiple versions, with the `--default` flag
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.12").arg("3.14").arg("--default"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: The `--default` flag cannot be used with multiple targets
    ");

    // Install 3.12 as a new default
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.12").arg("--default"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.12.[LATEST] in [TIME]
     + cpython-3.12.[LATEST]-[PLATFORM] (python, python3, python3.12)
    ");

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
                read_link(&bin_python_major), @"[TEMP_DIR]/managed/cpython-3.12-[PLATFORM]/bin/python3.12"
            );
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python_major), @"[TEMP_DIR]/managed/cpython-3.12.[LATEST]-[PLATFORM]/bin/python3.12"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_minor_12), @"[TEMP_DIR]/managed/cpython-3.12-[PLATFORM]/bin/python3.12"
            );
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python_minor_12), @"[TEMP_DIR]/managed/cpython-3.12.[LATEST]-[PLATFORM]/bin/python3.12"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_default), @"[TEMP_DIR]/managed/cpython-3.12-[PLATFORM]/bin/python3.12"
            );
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python_default), @"[TEMP_DIR]/managed/cpython-3.12.[LATEST]-[PLATFORM]/bin/python3.12"
            );
        });
    } else {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_major), @"[TEMP_DIR]/managed/cpython-3.12-[PLATFORM]/python"
            );
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python_major), @"[TEMP_DIR]/managed/cpython-3.12.[LATEST]-[PLATFORM]/python"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_minor_12), @"[TEMP_DIR]/managed/cpython-3.12-[PLATFORM]/python"
            );
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python_minor_12), @"[TEMP_DIR]/managed/cpython-3.12.[LATEST]-[PLATFORM]/python"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_default), @"[TEMP_DIR]/managed/cpython-3.12-[PLATFORM]/python"
            );
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python_default), @"[TEMP_DIR]/managed/cpython-3.12.[LATEST]-[PLATFORM]/python"
            );
        });
    }

    // Change the default to 3.14
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.14").arg("--default"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.14.[LATEST] in [TIME]
     + cpython-3.14.[LATEST]-[PLATFORM] (python, python3, python3.14)
    ");

    // All the executables should exist
    bin_python_minor_14.assert(predicate::path::exists());
    bin_python_minor_12.assert(predicate::path::exists());
    bin_python_major.assert(predicate::path::exists());
    bin_python_default.assert(predicate::path::exists());

    // And 3.14 should be the default now
    if cfg!(unix) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python_major), @"[TEMP_DIR]/managed/cpython-3.14.[LATEST]-[PLATFORM]/bin/python3.14"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python_minor_14), @"[TEMP_DIR]/managed/cpython-3.14.[LATEST]-[PLATFORM]/bin/python3.14"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python_minor_12), @"[TEMP_DIR]/managed/cpython-3.12.[LATEST]-[PLATFORM]/bin/python3.12"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python_default), @"[TEMP_DIR]/managed/cpython-3.14.[LATEST]-[PLATFORM]/bin/python3.14"
            );
        });
    } else if cfg!(windows) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python_major), @"[TEMP_DIR]/managed/cpython-3.14.[LATEST]-[PLATFORM]/python"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python_minor_14), @"[TEMP_DIR]/managed/cpython-3.14.[LATEST]-[PLATFORM]/python"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python_minor_12), @"[TEMP_DIR]/managed/cpython-3.12.[LATEST]-[PLATFORM]/python"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python_default), @"[TEMP_DIR]/managed/cpython-3.14.[LATEST]-[PLATFORM]/python"
            );
        });
    }
}

#[cfg(windows)]
fn launcher_path(path: &Path) -> PathBuf {
    let launcher = uv_trampoline_builder::Launcher::try_from_path(path)
        .unwrap_or_else(|_| panic!("{} should be readable", path.display()))
        .unwrap_or_else(|| panic!("{} should be a valid launcher", path.display()));
    launcher.python_path
}

fn canonicalize_link_path(path: &Path) -> String {
    #[cfg(unix)]
    let canonical_path = fs_err::canonicalize(path);

    #[cfg(windows)]
    let canonical_path = dunce::canonicalize(launcher_path(path));

    canonical_path
        .unwrap_or_else(|_| panic!("{} should be readable", path.display()))
        .simplified_display()
        .to_string()
}

fn read_link(path: &Path) -> String {
    #[cfg(unix)]
    let linked_path =
        fs_err::read_link(path).unwrap_or_else(|_| panic!("{} should be readable", path.display()));

    #[cfg(windows)]
    let linked_path = launcher_path(path);

    linked_path.simplified_display().to_string()
}

#[test]
fn python_install_unknown() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_managed_python_dirs()
        .with_python_download_cache();

    // An unknown request
    uv_snapshot!(context.filters(), context.python_install().arg("foobar"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: `foobar` is not a valid Python download request; see `uv help python` for supported formats and `uv python list --only-downloads` for available versions
    ");

    context.temp_dir.child("foo").create_dir_all().unwrap();

    // A directory
    uv_snapshot!(context.filters(), context.python_install().arg("./foo"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: `./foo` is not a valid Python download request; see `uv help python` for supported formats and `uv python list --only-downloads` for available versions
    ");
}

#[cfg(unix)]
#[test]
fn python_install_broken_link() {
    use assert_fs::prelude::PathCreateDir;
    use fs_err::os::unix::fs::symlink;

    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs()
        .with_python_download_cache()
        .with_filtered_latest_python_versions();

    let bin_python = context.bin_dir.child("python3.13");

    // Create a broken symlink
    context.bin_dir.create_dir_all().unwrap();
    symlink(context.temp_dir.join("does-not-exist"), &bin_python).unwrap();

    // Install
    uv_snapshot!(context.filters(), context.python_install().arg("3.13"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.[LATEST] in [TIME]
     + cpython-3.13.[LATEST]-[PLATFORM] (python3.13)
    ");

    // We should replace the broken symlink
    insta::with_settings!({
        filters => context.filters(),
    }, {
        insta::assert_snapshot!(
            canonicalize_link_path(&bin_python), @"[TEMP_DIR]/managed/cpython-3.13.[LATEST]-[PLATFORM]/bin/python3.13"
        );
    });
}

/// Test that --default works with pre-release versions (e.g., 3.15.0a1).
/// This test verifies the fix for issue #16696 where --default didn't create
/// python.exe and python3.exe links for pre-release versions.
#[test]
fn python_install_default_prerelease() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs()
        .with_python_download_cache();

    // Install Python 3.15, which currently only exists as a pre-release (3.15.0a1).
    context
        .python_install()
        .arg("--default")
        .arg("--preview-features")
        .arg("python-install-default")
        .arg("3.15")
        .assert()
        .success();

    let bin_python_minor_15 = context
        .bin_dir
        .child(format!("python3.15{}", std::env::consts::EXE_SUFFIX));

    let bin_python_major = context
        .bin_dir
        .child(format!("python3{}", std::env::consts::EXE_SUFFIX));

    let bin_python_default = context
        .bin_dir
        .child(format!("python{}", std::env::consts::EXE_SUFFIX));

    // Verify that all three executables are created when --default is used with a pre-release version
    bin_python_minor_15.assert(predicate::path::exists());
    bin_python_major.assert(predicate::path::exists());
    bin_python_default.assert(predicate::path::exists());
}

#[test]
fn python_install_default_from_env() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_filtered_latest_python_versions()
        .with_managed_python_dirs()
        .with_python_download_cache();

    // Install the version specified by the `UV_PYTHON` environment variable by default
    uv_snapshot!(context.filters(), context.python_install().env(EnvVars::UV_PYTHON, "3.12"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.12.[LATEST] in [TIME]
     + cpython-3.12.[LATEST]-[PLATFORM] (python3.12)
    ");

    // But prefer explicit requests
    uv_snapshot!(context.filters(), context.python_install().arg("3.11").env(EnvVars::UV_PYTHON, "3.12"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.11.[LATEST] in [TIME]
     + cpython-3.11.[LATEST]-[PLATFORM] (python3.11)
    ");

    // We should ignore `UV_PYTHON` here and complain there is not a target
    uv_snapshot!(context.filters(), context.python_uninstall().env(EnvVars::UV_PYTHON, "3.12"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: the following required arguments were not provided:
      <TARGETS>...

    Usage: uv python uninstall --cache-dir [CACHE_DIR] --install-dir <INSTALL_DIR> <TARGETS>...

    For more information, try '--help'.
    ");

    // We should ignore `UV_PYTHON` here and respect `--all`
    uv_snapshot!(context.filters(), context.python_uninstall().arg("--all").env(EnvVars::UV_PYTHON, "3.11"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Searching for Python installations
    Uninstalled 2 versions in [TIME]
     - cpython-3.11.[LATEST]-[PLATFORM] (python3.11)
     - cpython-3.12.[LATEST]-[PLATFORM] (python3.12)
    ");

    // Uninstall with no targets should error
    uv_snapshot!(context.filters(), context.python_uninstall(), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: the following required arguments were not provided:
      <TARGETS>...

    Usage: uv python uninstall --cache-dir [CACHE_DIR] --install-dir <INSTALL_DIR> <TARGETS>...

    For more information, try '--help'.
    ");

    // Uninstall with conflicting options should error
    uv_snapshot!(context.filters(), context.python_uninstall().arg("--all").arg("3.12"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: the argument '--all' cannot be used with '<TARGETS>...'

    Usage: uv python uninstall --cache-dir [CACHE_DIR] --all --install-dir <INSTALL_DIR> <TARGETS>...

    For more information, try '--help'.
    ");
}

#[cfg(target_os = "macos")]
#[test]
fn python_install_patch_dylib() {
    use assert_cmd::assert::OutputAssertExt;
    use uv_python::managed::platform_key_from_env;

    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_managed_python_dirs()
        .with_python_download_cache();

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

#[test]
fn python_install_prerelease() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_latest_python_versions()
        .with_managed_python_dirs()
        .with_python_download_cache()
        .with_filtered_python_install_bin()
        .with_filtered_exe_suffix();

    // Install 3.15
    // For now, this provides test coverage of pre-release handling
    uv_snapshot!(context.filters(), context.python_install().arg("3.15"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.15.[LATEST] in [TIME]
     + cpython-3.15.[LATEST]-[PLATFORM] (python3.15)
    ");

    // Install a specific pre-release
    uv_snapshot!(context.filters(), context.python_install().arg("3.15.0a2"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.15.0a2 in [TIME]
     + cpython-3.15.0a2-[PLATFORM]
    ");
}

#[test]
fn python_find_prerelease() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_latest_python_versions()
        .with_managed_python_dirs()
        .with_python_download_cache()
        .with_filtered_python_install_bin()
        .with_filtered_python_names()
        .with_filtered_exe_suffix();

    // See [`python_install_prerelease`] coverage of these.
    context.python_install().arg("3.15").assert().success();
    context.python_install().arg("3.15.0a2").assert().success();

    // We should be able to find this version without opt-in, because there is no stable release
    // installed
    uv_snapshot!(context.filters(), context.python_find().arg("3.15"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.15-[PLATFORM]/[INSTALL-BIN]/[PYTHON]

    ----- stderr -----
    ");

    // This also applies to `>=` requests, even though pre-releases aren't technically in the range
    uv_snapshot!(context.filters(), context.python_find().arg(">=3.15"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.15-[PLATFORM]/[INSTALL-BIN]/[PYTHON]

    ----- stderr -----
    ");

    uv_snapshot!(context.filters(), context.python_find().arg("3"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.15-[PLATFORM]/[INSTALL-BIN]/[PYTHON]

    ----- stderr -----
    ");

    // If we install a stable version, that should be preferred though
    uv_snapshot!(context.filters(), context.python_install().arg("3.13"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.[LATEST] in [TIME]
     + cpython-3.13.[LATEST]-[PLATFORM] (python3.13)
    ");

    uv_snapshot!(context.filters(), context.python_find().arg("3"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13-[PLATFORM]/[INSTALL-BIN]/[PYTHON]

    ----- stderr -----
    ");
}

/// A duplicate of [`python_install`] with an isolated `UV_PYTHON_CACHE_DIR`.
///
/// See also, [`python_install_no_cache`].
#[test]
fn python_install_cached() {
    // Skip this test if the developer has set `UV_PYTHON_CACHE_DIR` locally since it's slow
    if env::var_os(EnvVars::UV_PYTHON_CACHE_DIR).is_some() && env::var_os(EnvVars::CI).is_none() {
        debug!("Skipping test because `UV_PYTHON_CACHE_DIR` is set");
        return;
    }

    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs()
        .with_filtered_latest_python_versions();

    let python_cache = context.temp_dir.child("python-cache");

    // Install the latest version
    uv_snapshot!(context.filters(), context
        .python_install()
        .env(EnvVars::UV_PYTHON_CACHE_DIR, python_cache.as_ref()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.14.[LATEST] in [TIME]
     + cpython-3.14.[LATEST]-[PLATFORM] (python3.14)
    ");

    let bin_python = context
        .bin_dir
        .child(format!("python3.14{}", std::env::consts::EXE_SUFFIX));

    // The executable should be installed in the bin directory
    bin_python.assert(predicate::path::exists());

    // Should be a no-op when already installed
    uv_snapshot!(context.filters(), context
        .python_install()
        .env(EnvVars::UV_PYTHON_CACHE_DIR, python_cache.as_ref()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Python is already installed. Use `uv python install <request>` to install another version.
    ");

    uv_snapshot!(context.filters(), context.python_uninstall().arg("3.14"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Searching for Python versions matching: Python 3.14
    Uninstalled Python 3.14.[LATEST] in [TIME]
     - cpython-3.14.[LATEST]-[PLATFORM] (python3.14)
    ");

    // The cached archive can be installed offline
    uv_snapshot!(context.filters(), context
        .python_install()
        .arg("--offline")
        .env(EnvVars::UV_PYTHON_CACHE_DIR, python_cache.as_ref()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.14.[LATEST] in [TIME]
     + cpython-3.14.[LATEST]-[PLATFORM] (python3.14)
    ");

    // 3.12 isn't cached, so it can't be installed
    let context = context.with_filter((
        "cpython-3.12.*.tar.gz",
        "cpython-3.12.[PATCH]-[DATE]-[PLATFORM].tar.gz",
    ));
    uv_snapshot!(context.filters(), context
        .python_install()
        .arg("3.12")
        .arg("--offline")
        .env(EnvVars::UV_PYTHON_CACHE_DIR, python_cache.as_ref()), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    error: Failed to install cpython-3.12.[LATEST]-[PLATFORM]
      Caused by: An offline Python installation was requested, but cpython-3.12.[PATCH]-[DATE]-[PLATFORM].tar.gz) is missing in python-cache
    ");
}

/// Duplicate of [`python_install`] with the cache directory disabled.
#[test]
fn python_install_no_cache() {
    // Skip this test if the developer has set `UV_PYTHON_CACHE_DIR` locally since it's slow
    if env::var_os(EnvVars::UV_PYTHON_CACHE_DIR).is_some() && env::var_os(EnvVars::CI).is_none() {
        debug!("Skipping test because `UV_PYTHON_CACHE_DIR` is set");
        return;
    }

    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs()
        .with_filtered_latest_python_versions();

    // Install the latest version
    uv_snapshot!(context.filters(), context.python_install(), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.14.[LATEST] in [TIME]
     + cpython-3.14.[LATEST]-[PLATFORM] (python3.14)
    ");

    let bin_python = context
        .bin_dir
        .child(format!("python3.14{}", std::env::consts::EXE_SUFFIX));

    // The executable should not present in the bin directory
    bin_python.assert(predicate::path::exists());

    // Should be a no-op when already installed
    uv_snapshot!(context.filters(), context.python_install(), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Python is already installed. Use `uv python install <request>` to install another version.
    ");

    // Similarly, when a requested version is already installed
    uv_snapshot!(context.filters(), context.python_install().arg("3.14"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Python 3.14 is already installed
    ");

    // You can opt-in to a reinstall
    uv_snapshot!(context.filters(), context.python_install().arg("3.14").arg("--reinstall"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.14.[LATEST] in [TIME]
     ~ cpython-3.14.[LATEST]-[PLATFORM] (python3.14)
    ");

    // Uninstallation requires an argument
    uv_snapshot!(context.filters(), context.python_uninstall(), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: the following required arguments were not provided:
      <TARGETS>...

    Usage: uv python uninstall --cache-dir [CACHE_DIR] --install-dir <INSTALL_DIR> <TARGETS>...

    For more information, try '--help'.
    ");

    uv_snapshot!(context.filters(), context.python_uninstall().arg("3.14"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Searching for Python versions matching: Python 3.14
    Uninstalled Python 3.14.[LATEST] in [TIME]
     - cpython-3.14.[LATEST]-[PLATFORM] (python3.14)
    ");

    // 3.12 isn't cached, so it can't be installed
    let context = context
        .with_filter((
            "cpython-3.12.*.tar.gz",
            "cpython-3.12.[PATCH]-[DATE]-[PLATFORM].tar.gz",
        ))
        .with_filter((r"releases/download/\d{8}/", "releases/download/[DATE]/"));
    uv_snapshot!(context.filters(), context
        .python_install()
        .arg("3.12")
        .arg("--offline"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    error: Failed to install cpython-3.12.[LATEST]-[PLATFORM]
      Caused by: Failed to download https://github.com/astral-sh/python-build-standalone/releases/download/[DATE]/cpython-3.12.[PATCH]-[DATE]-[PLATFORM].tar.gz
      Caused by: Network connectivity is disabled, but the requested data wasn't found in the cache for: `https://github.com/astral-sh/python-build-standalone/releases/download/[DATE]/cpython-3.12.[PATCH]-[DATE]-[PLATFORM].tar.gz`
    ");
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn python_install_emulated_macos() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_exe_suffix()
        .with_managed_python_dirs()
        .with_python_download_cache()
        .with_filtered_latest_python_versions();

    let arch_status = Command::new("/usr/bin/arch")
        .arg("-x86_64")
        .arg("true")
        .status();
    if !arch_status.is_ok_and(|x| x.success()) {
        // Rosetta is not available to run the x86_64 interpreter
        // fail the test in CI, otherwise skip it
        #[expect(clippy::manual_assert)]
        if env::var(EnvVars::CI).is_ok() {
            panic!("x86_64 emulation is not available on this CI runner");
        }
        debug!("Skipping test because x86_64 emulation is not available");
        return;
    }

    // Before installation, `uv python list` should not show the x86_64 download
    uv_snapshot!(context.filters(), context.python_list().arg("3.13"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.13.[LATEST]-macos-aarch64-none    <download available>

    ----- stderr -----
    ");

    // Install an x86_64 version (assuming an aarch64 host)
    uv_snapshot!(context.filters(), context.python_install().arg("3.13-x86_64"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.[LATEST] in [TIME]
     + cpython-3.13.[LATEST]-macos-x86_64-none (python3.13)
    ");

    // It should be discoverable with `uv python find`
    uv_snapshot!(context.filters(), context.python_find().arg("3.13").arg("--resolve-links"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13.[LATEST]-macos-x86_64-none/bin/python3.13

    ----- stderr -----
    ");

    // And included in `uv python list`
    uv_snapshot!(context.filters(), context.python_list().arg("3.13"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.13.[LATEST]-macos-aarch64-none    <download available>
    cpython-3.13.[LATEST]-macos-x86_64-none     managed/cpython-3.13-macos-x86_64-none/bin/python3.13

    ----- stderr -----
    ");

    uv_snapshot!(context.filters(), context.python_install().arg("3.13-aarch64"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.[LATEST] in [TIME]
     + cpython-3.13.[LATEST]-macos-aarch64-none
    ");

    // Once we've installed the native version, it should be preferred over x86_64
    uv_snapshot!(context.filters(), context.python_find().arg("3.13").arg("--resolve-links"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13.[LATEST]-macos-aarch64-none/bin/python3.13

    ----- stderr -----
    ");
}

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
#[test]
fn python_install_emulated_windows_x86_on_x64() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_exe_suffix()
        .with_managed_python_dirs()
        .with_python_download_cache()
        .with_filtered_latest_python_versions();

    // Before installation, `uv python list` should not show the x86_32 download
    uv_snapshot!(context.filters(), context.python_list().arg("3.13"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.13.[LATEST]-windows-x86_64-none    <download available>

    ----- stderr -----
    ");

    // Install an x86_32 version (assuming an x64 host)
    uv_snapshot!(context.filters(), context.python_install().arg("3.13-x86"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.[LATEST] in [TIME]
     + cpython-3.13.[LATEST]-windows-x86-none (python3.13)
    ");

    // It should be discoverable with `uv python find`
    uv_snapshot!(context.filters(), context.python_find().arg("3.13"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13-windows-x86-none/python

    ----- stderr -----
    ");

    // And included in `uv python list`
    uv_snapshot!(context.filters(), context.python_list().arg("3.13"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.13.[LATEST]-windows-x86_64-none    <download available>
    cpython-3.13.[LATEST]-windows-x86-none       managed/cpython-3.13-windows-x86-none/python

    ----- stderr -----
    ");

    uv_snapshot!(context.filters(), context.python_install().arg("3.13-x86_64"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.[LATEST] in [TIME]
     + cpython-3.13.[LATEST]-windows-x86_64-none
    ");

    // Once we've installed the native version, it should be preferred over x86_32
    uv_snapshot!(context.filters(), context.python_find().arg("3.13"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13-windows-x86_64-none/python

    ----- stderr -----
    ");
}

// Creating a venv with `--allow-existing` over an existing managed venv should succeed.
//
// Regression test for <https://github.com/astral-sh/uv/issues/17963>.
#[test]
fn install_managed_venv_allow_existing() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_filtered_latest_python_versions()
        .with_managed_python_dirs()
        .with_python_download_cache()
        .with_filtered_python_install_bin();

    // Install a managed Python version.
    uv_snapshot!(context.filters(), context.python_install().arg("3.13"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.[LATEST] in [TIME]
     + cpython-3.13.[LATEST]-[PLATFORM] (python3.13)
    ");

    // Create a virtual environment using the managed installation.
    uv_snapshot!(context.filters(), context.venv().arg("-p").arg("3.13")
        .arg(context.venv.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.13.[LATEST]
    Creating virtual environment at: .venv
    Activate with: source .venv/[BIN]/activate
    ");

    // Create the venv again with `--allow-existing` — this should not fail.
    uv_snapshot!(context.filters(), context.venv().arg("-p").arg("3.13")
        .arg("--allow-existing")
        .arg(context.venv.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.13.[LATEST]
    Creating virtual environment at: .venv
    Activate with: source .venv/[BIN]/activate
    ");
}

// A virtual environment should track the latest patch version installed.
#[test]
fn install_transparent_patch_upgrade_uv_venv() {
    let context = uv_test::test_context_with_versions!(&["3.13"])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs()
        .with_python_download_cache()
        .with_filtered_python_install_bin();

    // Install a lower patch version.
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.12.9"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.12.9 in [TIME]
     + cpython-3.12.9-[PLATFORM] (python3.12)
    "
    );

    // Create a virtual environment.
    uv_snapshot!(context.filters(), context.venv().arg("-p").arg("3.12")
        .arg(context.venv.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.9
    Creating virtual environment at: .venv
    Activate with: source .venv/[BIN]/activate
    "
    );

    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.9

    ----- stderr -----
    "
    );

    // Install a higher patch version.
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.12.11"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.12.11 in [TIME]
     + cpython-3.12.11-[PLATFORM] (python3.12)
    "
    );

    // Virtual environment should reflect higher version.
    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.11

    ----- stderr -----
    "
    );

    // Install a lower patch version.
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.12.8"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.12.8 in [TIME]
     + cpython-3.12.8-[PLATFORM]
    "
    );

    // Virtual environment should reflect highest version.
    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.11

    ----- stderr -----
    "
    );
}

// When installing multiple patches simultaneously, a virtual environment on that
// minor version should point to the highest.
#[test]
fn install_multiple_patches() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs()
        .with_python_download_cache()
        .with_filtered_python_install_bin();

    // Install 3.12 patches in ascending order list
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.12.9").arg("3.12.11"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed 2 versions in [TIME]
     + cpython-3.12.9-[PLATFORM]
     + cpython-3.12.11-[PLATFORM] (python3.12)
    "
    );

    // Create a virtual environment.
    uv_snapshot!(context.filters(), context.venv().arg("-p").arg("3.12")
        .arg(context.venv.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.11
    Creating virtual environment at: .venv
    Activate with: source .venv/[BIN]/activate
    "
    );

    // Virtual environment should be on highest installed patch.
    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.11

    ----- stderr -----
    "
    );

    // Remove the original virtual environment
    fs_err::remove_dir_all(&context.venv).unwrap();

    // Install 3.10 patches in descending order list
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.10.17").arg("3.10.16"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed 2 versions in [TIME]
     + cpython-3.10.16-[PLATFORM]
     + cpython-3.10.17-[PLATFORM] (python3.10)
    "
    );

    // Create a virtual environment on 3.10.
    uv_snapshot!(context.filters(), context.venv().arg("-p").arg("3.10")
        .arg(context.venv.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.10.17
    Creating virtual environment at: .venv
    Activate with: source .venv/[BIN]/activate
    "
    );

    // Virtual environment should be on highest installed patch.
    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.10.17

    ----- stderr -----
    "
    );
}

// After uninstalling the highest patch, a virtual environment should point to the
// next highest.
#[test]
fn uninstall_highest_patch() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs()
        .with_python_download_cache()
        .with_filtered_python_install_bin();

    // Install patches in ascending order list
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.12.11").arg("3.12.9").arg("3.12.8"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed 3 versions in [TIME]
     + cpython-3.12.8-[PLATFORM]
     + cpython-3.12.9-[PLATFORM]
     + cpython-3.12.11-[PLATFORM] (python3.12)
    "
    );

    uv_snapshot!(context.filters(), context.venv().arg("-p").arg("3.12")
        .arg(context.venv.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.11
    Creating virtual environment at: .venv
    Activate with: source .venv/[BIN]/activate
    "
    );

    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.11

    ----- stderr -----
    "
    );

    // Uninstall the highest patch version
    uv_snapshot!(context.filters(), context.python_uninstall().arg("--preview").arg("3.12.11"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Searching for Python versions matching: Python 3.12.11
    Uninstalled Python 3.12.11 in [TIME]
     - cpython-3.12.11-[PLATFORM] (python3.12)
    "
    );

    // Virtual environment should be on highest patch version remaining.
    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.9

    ----- stderr -----
    "
    );
}

// Virtual environments only record minor versions. `uv venv -p 3.x.y` will
// not prevent a virtual environment from tracking the latest patch version
// installed.
#[test]
fn install_no_transparent_upgrade_with_venv_patch_specification() {
    let context = uv_test::test_context_with_versions!(&["3.13"])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs()
        .with_python_download_cache()
        .with_filtered_python_install_bin();

    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.12.9"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.12.9 in [TIME]
     + cpython-3.12.9-[PLATFORM] (python3.12)
    "
    );

    // Create a virtual environment with a patch version
    uv_snapshot!(context.filters(), context.venv().arg("-p").arg("3.12.9")
        .arg(context.venv.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.9
    Creating virtual environment at: .venv
    Activate with: source .venv/[BIN]/activate
    "
    );

    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.9

    ----- stderr -----
    "
    );

    // Install a higher patch version.
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.12.11"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.12.11 in [TIME]
     + cpython-3.12.11-[PLATFORM] (python3.12)
    "
    );

    // The virtual environment Python version is transparently upgraded.
    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.9

    ----- stderr -----
    "
    );
}

// A virtual environment created using the `venv` module should track
// the latest patch version installed.
#[test]
fn install_transparent_patch_upgrade_venv_module() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs()
        .with_python_download_cache()
        .with_filtered_python_install_bin();

    let bin_dir = context.temp_dir.child("bin");

    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.12.9"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.12.9 in [TIME]
     + cpython-3.12.9-[PLATFORM] (python3.12)
    "
    );

    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.9

    ----- stderr -----
    "
    );

    // Create a virtual environment using venv module.
    uv_snapshot!(context.filters(), context.run().arg("python").arg("-m").arg("venv").arg(context.venv.as_os_str()).arg("--without-pip")
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    ");

    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.9

    ----- stderr -----
    "
    );

    // Install a higher patch version
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.12.11"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.12.11 in [TIME]
     + cpython-3.12.11-[PLATFORM] (python3.12)
    "
    );

    // Virtual environment should reflect highest patch version.
    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.11

    ----- stderr -----
    "
    );
}

// Automatically installing a lower patch version when running a command like
// `uv run` should not downgrade virtual environments.
#[test]
fn install_lower_patch_automatically() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs()
        .with_python_download_cache()
        .with_filtered_python_install_bin();

    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.12.11"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.12.11 in [TIME]
     + cpython-3.12.11-[PLATFORM] (python3.12)
    "
    );

    uv_snapshot!(context.filters(), context.venv().arg("-p").arg("3.12")
        .arg(context.venv.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.11
    Creating virtual environment at: .venv
    Activate with: source .venv/[BIN]/activate
    "
    );

    uv_snapshot!(context.filters(), context.init().arg("-p").arg("3.12.9").arg("proj"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Initialized project `proj` at `[TEMP_DIR]/proj`
    "
    );

    // Create a new virtual environment to trigger automatic installation of
    // lower patch version
    uv_snapshot!(context.filters(), context.venv()
        .arg("--directory").arg("proj")
        .arg("-p").arg("3.12.9"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.9
    Creating virtual environment at: .venv
    Activate with: source .venv/[BIN]/activate
    ");

    // Original virtual environment should still point to higher patch
    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.11

    ----- stderr -----
    "
    );
}

#[test]
fn uninstall_last_patch() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs()
        .with_python_download_cache()
        .with_filtered_virtualenv_bin();

    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.10.17"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.10.17 in [TIME]
     + cpython-3.10.17-[PLATFORM] (python3.10)
    "
    );

    uv_snapshot!(context.filters(), context.venv().arg("-p").arg("3.10"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.10.17
    Creating virtual environment at: .venv
    Activate with: source .venv/[BIN]/activate
    "
    );

    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.10.17

    ----- stderr -----
    "
    );

    uv_snapshot!(context.filters(), context.python_uninstall().arg("--preview").arg("3.10.17"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Searching for Python versions matching: Python 3.10.17
    Uninstalled Python 3.10.17 in [TIME]
     - cpython-3.10.17-[PLATFORM] (python3.10)
    "
    );

    let context = context.with_filter(("python3", "python"));

    #[cfg(unix)]
    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to inspect Python interpreter from active virtual environment at `.venv/[BIN]/python`
      Caused by: Broken symlink at `.venv/[BIN]/python`, was the underlying Python interpreter removed?

    hint: Consider recreating the environment (e.g., with `uv venv`)
    "
    );

    #[cfg(windows)]
    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to inspect Python interpreter from active virtual environment at `.venv/[BIN]/python`
      Caused by: Python interpreter not found at `[VENV]/[BIN]/python`
    "
    );
}

#[cfg(unix)] // Pyodide cannot be used on Windows
#[test]
fn python_install_pyodide() {
    use assert_cmd::assert::OutputAssertExt;

    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_exe_suffix()
        .with_managed_python_dirs()
        .with_python_download_cache()
        .with_filtered_latest_python_versions();

    uv_snapshot!(context.filters(), context.python_install().arg("cpython-3.13.2-emscripten-wasm32-musl"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.2 in [TIME]
     + pyodide-3.13.2-emscripten-wasm32-musl (pyodide3.13)
    ");

    let bin_python = context
        .bin_dir
        .child(format!("pyodide3.13{}", std::env::consts::EXE_SUFFIX));

    // The executable should be installed in the bin directory
    bin_python.assert(predicate::path::exists());

    // It should be a link
    bin_python.assert(predicate::path::is_symlink());

    // The link should be a path to the binary
    insta::with_settings!({
        filters => context.filters(),
    }, {
        insta::assert_snapshot!(
            read_link(&bin_python), @"[TEMP_DIR]/managed/pyodide-3.13.2-emscripten-wasm32-musl/python"
        );
    });

    // The executable should "work"
    uv_snapshot!(context.filters(), Command::new(bin_python.as_os_str())
        .arg("-c").arg("import subprocess; print('hello world')"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    hello world

    ----- stderr -----
    ");

    // We should be able to find the Pyodide interpreter
    uv_snapshot!(context.filters(), context.python_find().arg("cpython-3.13.2-emscripten-wasm32-musl"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/managed/pyodide-3.13.2-emscripten-wasm32-musl/python

    ----- stderr -----
    ");

    // We should be able to create a virtual environment with it
    uv_snapshot!(context.filters(), context.venv().arg("--python").arg("cpython-3.13.2-emscripten-wasm32-musl"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.13.2
    Creating virtual environment at: .venv
    Activate with: source .venv/[BIN]/activate
    ");

    // We should be able to run the Python in the virtual environment
    uv_snapshot!(context.filters(), context.python_command().arg("-c").arg("import subprocess; print('hello world')"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    hello world

    ----- stderr -----
    ");

    context.python_uninstall().arg("--all").assert().success();
    fs_err::remove_dir_all(&context.venv).unwrap();

    // Install via `pyodide`
    uv_snapshot!(context.filters(), context.python_install().arg("pyodide"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.2 in [TIME]
     + pyodide-3.13.2-emscripten-wasm32-musl (pyodide3.13)
    ");

    context.python_uninstall().arg("--all").assert().success();

    // Install via `pyodide@<version>`
    uv_snapshot!(context.filters(), context.python_install().arg("pyodide@3.13"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.2 in [TIME]
     + pyodide-3.13.2-emscripten-wasm32-musl (pyodide3.13)
    ");

    // Find via `pyodide``
    uv_snapshot!(context.filters(), context.python_find().arg("pyodide"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/managed/pyodide-3.13.2-emscripten-wasm32-musl/python

    ----- stderr -----
    ");

    // Find without a request should fail
    uv_snapshot!(context.filters(), context.python_find(), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No interpreter found in virtual environments, managed installations, or search path
    ");
    // Find with "cpython" should also fail
    uv_snapshot!(context.filters(), context.python_find().arg("cpython"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No interpreter found for CPython in virtual environments, managed installations, or search path
    ");

    // Install a CPython interpreter
    let context = context.with_filtered_python_keys();
    uv_snapshot!(context.filters(), context.python_install().arg("cpython"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.14.[LATEST] in [TIME]
     + cpython-3.14.[LATEST]-[PLATFORM] (python3.14)
    ");

    // Now, we should prefer that
    uv_snapshot!(context.filters(), context.python_find().arg("any"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.14-[PLATFORM]/bin/python3.14

    ----- stderr -----
    ");

    // Unless we request pyodide
    uv_snapshot!(context.filters(), context.python_find().arg("pyodide"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/managed/pyodide-3.13.2-emscripten-wasm32-musl/python

    ----- stderr -----
    ");
}

#[test]
fn python_install_build_version() {
    use uv_python::managed::platform_key_from_env;

    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_managed_python_dirs()
        .with_python_download_cache()
        .with_filtered_python_sources()
        .with_filtered_python_install_bin()
        .with_filtered_python_names()
        .with_filtered_exe_suffix();

    uv_snapshot!(context.filters(), context.python_install()
        .arg("3.12")
        .env(EnvVars::UV_PYTHON_CPYTHON_BUILD, "20240814"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.12.5 in [TIME]
     + cpython-3.12.5-[PLATFORM] (python3.12)
    ");

    // A BUILD file should be present with the version
    let cpython_dir = context.temp_dir.child("managed").child(format!(
        "cpython-3.12.5-{}",
        platform_key_from_env().unwrap()
    ));
    let build_file_path = cpython_dir.join("BUILD");
    let build_content = fs_err::read_to_string(&build_file_path).unwrap();
    assert_eq!(build_content, "20240814");

    // We should find the build
    uv_snapshot!(context.filters(), context.python_find()
        .arg("3.12")
        .env(EnvVars::UV_PYTHON_CPYTHON_BUILD, "20240814"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.12-[PLATFORM]/[INSTALL-BIN]/[PYTHON]

    ----- stderr -----
    ");

    // If the build number does not match, we should ignore the installation
    uv_snapshot!(context.filters(), context.python_find()
        .arg("3.12")
        .env(EnvVars::UV_PYTHON_CPYTHON_BUILD, "99999999"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No interpreter found for Python 3.12 in [PYTHON SOURCES]
    ");

    // If there's no install for a build number, we should fail
    uv_snapshot!(context.filters(), context.python_install()
        .arg("3.12")
        .env(EnvVars::UV_PYTHON_CPYTHON_BUILD, "99999999"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No download found for request: cpython-3.12-[PLATFORM]
    ");

    // Requesting a specific patch version without a matching build number should fail
    uv_snapshot!(context.filters(), context.python_install()
        .arg("3.12.10")
        .env(EnvVars::UV_PYTHON_CPYTHON_BUILD, "20250814"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No download found for request: cpython-3.12.10-[PLATFORM]
    ");
}

#[test]
fn python_install_build_version_pypy() {
    use uv_python::managed::platform_key_from_env;

    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_python_sources()
        .with_managed_python_dirs()
        .with_python_download_cache()
        .with_filtered_python_install_bin()
        .with_filtered_python_names()
        .with_filtered_exe_suffix();

    uv_snapshot!(context.filters(), context.python_install()
        .arg("pypy3.10")
        .env(EnvVars::UV_PYTHON_PYPY_BUILD, "7.3.19"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.10.16 in [TIME]
     + pypy-3.10.16-[PLATFORM] (pypy3.10)
    ");

    // A BUILD file should be present with the version
    let pypy_dir = context
        .temp_dir
        .child("managed")
        .child(format!("pypy-3.10.16-{}", platform_key_from_env().unwrap()));
    let build_file_path = pypy_dir.join("BUILD");
    let build_content = fs_err::read_to_string(&build_file_path).unwrap();
    assert_eq!(build_content, "7.3.19");

    // We should find the build
    uv_snapshot!(context.filters(), context.python_find()
        .arg("pypy3.10")
        .env(EnvVars::UV_PYTHON_PYPY_BUILD, "7.3.19"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/managed/pypy-3.10.16-[PLATFORM]/[INSTALL-BIN]/[PYPY]

    ----- stderr -----
    ");

    // If the build number does not match, we should ignore the installation
    uv_snapshot!(context.filters(), context.python_find()
        .arg("pypy3.10")
        .env(EnvVars::UV_PYTHON_PYPY_BUILD, "99.99.99"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No interpreter found for PyPy 3.10 in [PYTHON SOURCES]
    ");

    // If there's no install for a build number, we should fail
    uv_snapshot!(context.filters(), context.python_install()
        .arg("pypy3.10")
        .env(EnvVars::UV_PYTHON_PYPY_BUILD, "99.99.99"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No download found for request: pypy-3.10-[PLATFORM]
    ");
}

#[test]
fn python_install_upgrade() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_python_download_cache()
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs()
        .with_filtered_latest_python_versions();

    // Provide `--upgrade` as an `install` option without any versions
    uv_snapshot!(context.filters(), context.python_install().arg("--upgrade"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.14.[LATEST] in [TIME]
     + cpython-3.14.[LATEST]-[PLATFORM] (python3.14)
    ");

    // Provide `--upgrade` as an `install` option without any versions again!
    uv_snapshot!(context.filters(), context.python_install().arg("--upgrade"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    The default Python installation is already on the latest supported patch release. Use `uv python install <request>` to install another version.
    ");

    // Install an earlier patch version
    uv_snapshot!(context.filters(), context.python_install().arg("3.10.17"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.10.17 in [TIME]
     + cpython-3.10.17-[PLATFORM] (python3.10)
    ");

    // Ask for an `--upgrade`
    uv_snapshot!(context.filters(), context.python_install().arg("--upgrade").arg("3.10"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.10.[LATEST] in [TIME]
     + cpython-3.10.[LATEST]-[PLATFORM] (python3.10)
    ");

    // Request a patch version with `--upgrade`
    uv_snapshot!(context.filters(), context.python_install().arg("--upgrade").arg("3.11.4"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    error: `uv python install --upgrade` only accepts minor versions, got: 3.11.4
    ");

    // Request a version that isn't installed yet
    uv_snapshot!(context.filters(), context.python_install().arg("--upgrade").arg("3.11"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.11.[LATEST] in [TIME]
     + cpython-3.11.[LATEST]-[PLATFORM] (python3.11)
    ");

    // Ask for it again
    uv_snapshot!(context.filters(), context.python_install().arg("--upgrade").arg("3.11"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Python 3.11 is already on the latest supported patch release
    ");

    // Install an outdated version
    uv_snapshot!(context.filters(), context.python_install().arg("3.9.5"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.9.5 in [TIME]
     + cpython-3.9.5-[PLATFORM] (python3.9)
    ");

    // We shouldn't update it when not relevant
    uv_snapshot!(context.filters(), context.python_install().arg("--upgrade").arg("3.11"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Python 3.11 is already on the latest supported patch release
    ");

    // Ask for multiple already satisfied versions
    uv_snapshot!(context.filters(), context.python_install().arg("--upgrade").arg("3.10").arg("3.11"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    All requested versions already on latest supported patch release
    ");

    // Mix in an unsatisfied version and a missing one
    uv_snapshot!(context.filters(), context.python_install().arg("--upgrade").arg("3.9").arg("3.10").arg("3.11").arg("3.12"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed 2 versions in [TIME]
     + cpython-3.9.25-[PLATFORM] (python3.9)
     + cpython-3.12.[LATEST]-[PLATFORM] (python3.12)
    ");
}

#[test]
fn python_install_upgrade_version_file() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_python_download_cache()
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs()
        .with_filtered_latest_python_versions();

    // Pin to a minor version
    context.python_pin().arg("3.13").assert().success();

    // Provide `--upgrade` as an `install` option without any versions
    uv_snapshot!(context.filters(), context.python_install().arg("--upgrade"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.[LATEST] in [TIME]
     + cpython-3.13.[LATEST]-[PLATFORM] (python3.13)
    ");

    // Provide `--upgrade` as an `install` option without any versions again!
    uv_snapshot!(context.filters(), context.python_install().arg("--upgrade"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Python 3.13 is already on the latest supported patch release
    ");

    // Pin to a patch version
    context.python_pin().arg("3.12.4").assert().success();

    // Provide `--upgrade` as an `install` option without any versions
    uv_snapshot!(context.filters(), context.python_install().arg("--upgrade"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    error: `uv python install --upgrade` only accepts minor versions, got: 3.12.4

    hint: The version request came from a `.python-version` file; change the patch version in the file to upgrade instead
    ");
}

#[test]
fn python_install_armv7() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_managed_python_dirs()
        .with_python_download_cache()
        .with_filtered_python_sources()
        .with_filtered_python_install_bin()
        .with_filtered_python_names()
        .with_filtered_exe_suffix();

    // Explicitly request a musl build for armv7l
    uv_snapshot!(context.filters(), context.python_install().arg("cpython-3.12.12-linux-armv7-musl"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: uv does not yet provide musl Python distributions on armv7.
    ");

    // Explicitly request a gnuabi build for armv7l
    uv_snapshot!(context.filters(), context.python_install().arg("cpython-3.12.12-linux-armv7-gnueabi"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.12.12 in [TIME]
     + cpython-3.12.12-[PLATFORM] (python3.12)
    ");
}

#[test]
fn python_install_compile_bytecode() -> anyhow::Result<()> {
    fn count_files_by_ext(dir: &Path, extension: &str) -> anyhow::Result<usize> {
        let mut count = 0;
        let walker = WalkDir::new(dir).into_iter();
        for entry in walker {
            let entry = entry?;
            let path = entry.path();
            if entry.metadata()?.is_file() && path.extension().is_some_and(|ext| ext == extension) {
                count += 1;
            }
        }
        Ok(count)
    }

    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_filtered_compiled_file_count()
        .with_managed_python_dirs()
        .with_empty_python_install_mirror()
        .with_python_download_cache()
        .with_filtered_latest_python_versions();

    // Install 3.14 and compile its bytecode
    uv_snapshot!(context.filters(), context.python_install().arg("--compile-bytecode").arg("3.14"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.14.[LATEST] in [TIME]
     + cpython-3.14.[LATEST]-[PLATFORM] (python3.14)
    Bytecode compiled [COUNT] files in [TIME]
    ");

    // Find the stdlib path for cpython 3.14
    let bin_path = context
        .bin_dir
        .child(format!("python3.14{}", std::env::consts::EXE_SUFFIX));

    #[cfg(unix)]
    let stdlib = fs_err::read_link(bin_path)?
        .parent()
        .context("Python binary should be a child of `bin`")?
        .parent()
        .context("`bin` directory should be a child of the installation path")?
        .join("lib")
        .join("python3.14");
    #[cfg(windows)]
    let stdlib = launcher_path(&bin_path)
        .parent()
        .context("Python binary should be a child of the installation path")?
        .join("Lib");

    // And the count should match
    let pyc_count = count_files_by_ext(&stdlib, "pyc")?;
    let py_count = count_files_by_ext(&stdlib, "py")?;
    assert_eq!(pyc_count, py_count);

    // Attempting to install with --compile-bytecode should (currently)
    // unconditionally re-run the bytecode compiler
    uv_snapshot!(context.filters(), context.python_install().arg("--compile-bytecode").arg("3.14"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Python 3.14 is already installed
    Bytecode compiled [COUNT] files in [TIME]
    ");

    // Reinstalling with --compile-bytecode should compile bytecode.
    uv_snapshot!(context.filters(), context.python_install().arg("--reinstall").arg("--compile-bytecode").arg("3.14"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.14.[LATEST] in [TIME]
     ~ cpython-3.14.[LATEST]-[PLATFORM] (python3.14)
    Bytecode compiled [COUNT] files in [TIME]
    ");

    Ok(())
}

#[test]
fn python_install_compile_bytecode_existing() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_filtered_compiled_file_count()
        .with_managed_python_dirs()
        .with_empty_python_install_mirror()
        .with_python_download_cache()
        .with_filtered_latest_python_versions();

    // A fresh install should be able to be compiled later
    uv_snapshot!(context.filters(), context.python_install().arg("3.14"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.14.[LATEST] in [TIME]
     + cpython-3.14.[LATEST]-[PLATFORM] (python3.14)
    ");

    uv_snapshot!(context.filters(), context.python_install().arg("--compile-bytecode").arg("3.14"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Python 3.14 is already installed
    Bytecode compiled [COUNT] files in [TIME]
    ");
}

#[test]
fn python_install_compile_bytecode_upgrade() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_filtered_compiled_file_count()
        .with_managed_python_dirs()
        .with_empty_python_install_mirror()
        .with_python_download_cache()
        .with_filtered_latest_python_versions();

    // An upgrade should also compile bytecode
    uv_snapshot!(context.filters(), context.python_install().arg("3.14.0"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.14.0 in [TIME]
     + cpython-3.14.0-[PLATFORM] (python3.14)
    ");

    uv_snapshot!(context.filters(), context.python_install().arg("--upgrade").arg("--compile-bytecode").arg("3.14"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.14.[LATEST] in [TIME]
     + cpython-3.14.[LATEST]-[PLATFORM] (python3.14)
    Bytecode compiled [COUNT] files in [TIME]
    ");
}

#[test]
fn python_install_upgrade_build_version() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_python_download_cache()
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs();

    // Install Python 3.12
    uv_snapshot!(context.filters(), context.python_install().arg("3.12"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.12.12 in [TIME]
     + cpython-3.12.12-[PLATFORM] (python3.12)
    ");

    // Should be a no-op when already installed at latest version
    uv_snapshot!(context.filters(), context.python_install().arg("--upgrade").arg("3.12"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Python 3.12 is already on the latest supported patch release
    ");

    // Overwrite the BUILD file with an older build version
    let installation_dir = context.temp_dir.child("managed").child(format!(
        "cpython-3.12.12-{}",
        platform_key_from_env().unwrap()
    ));
    let build_file = installation_dir.join("BUILD");
    fs_err::write(&build_file, "19000101").unwrap();

    // Now upgrade should detect the outdated build version and reinstall
    uv_snapshot!(context.filters(), context.python_install().arg("--upgrade").arg("3.12"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.12.12 in [TIME]
     ~ cpython-3.12.12-[PLATFORM]
    ");

    // Should be a no-op again after upgrade
    uv_snapshot!(context.filters(), context.python_install().arg("--upgrade").arg("3.12"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Python 3.12 is already on the latest supported patch release
    ");
}

#[test]
fn python_install_compile_bytecode_multiple() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_filtered_compiled_file_count()
        .with_managed_python_dirs()
        .with_empty_python_install_mirror()
        .with_python_download_cache()
        .with_filtered_latest_python_versions();

    // Should handle installing and compiling multiple versions correctly
    uv_snapshot!(context.filters(), context.python_install().arg("--compile-bytecode").arg("3.14").arg("3.12"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed 2 versions in [TIME]
     + cpython-3.12.[LATEST]-[PLATFORM] (python3.12)
     + cpython-3.14.[LATEST]-[PLATFORM] (python3.14)
    Bytecode compiled [COUNT] files in [TIME]
    ");
}

#[cfg(unix)] // Pyodide cannot be used on Windows
#[test]
fn python_install_compile_bytecode_pyodide() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_filtered_compiled_file_count()
        .with_managed_python_dirs()
        .with_empty_python_install_mirror()
        .with_python_download_cache();

    // Should warn on explicit pyodide installation
    uv_snapshot!(context.filters(), context.python_install().arg("--compile-bytecode").arg("cpython-3.13.2-emscripten-wasm32-musl"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.2 in [TIME]
     + pyodide-3.13.2-emscripten-wasm32-musl (pyodide3.13)
    No compatible versions to bytecode compile (skipped 1)
    ");

    // TODO(tk) There's a bug with python_upgrade when pyodide is installed which leads to
    // `error: No download found for request: pyodide-3.13-emscripten-wasm32-musl`
    //// Recompilation where pyodide isn't explicitly specified shouldn't warn
    //uv_snapshot!(context.filters(), context.python_upgrade().arg("--compile-bytecode"), @r"TODO");
}

#[test]
fn python_install_compile_bytecode_graalpy() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_filtered_compiled_file_count()
        .with_managed_python_dirs()
        .with_empty_python_install_mirror()
        .with_python_download_cache();

    // Should work for graalpy
    uv_snapshot!(context.filters(), context.python_install().arg("--compile-bytecode").arg("graalpy-3.12"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.12.0 in [TIME]
     + graalpy-3.12.0-[PLATFORM] (graalpy3.12)
    Bytecode compiled [COUNT] files in [TIME]
    ");
}

#[test]
fn python_install_compile_bytecode_pypy() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_filtered_compiled_file_count()
        .with_managed_python_dirs()
        .with_empty_python_install_mirror()
        .with_python_download_cache();

    // Should work for pypy
    uv_snapshot!(context.filters(), context.python_install().arg("--compile-bytecode").arg("pypy-3.11"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.11.13 in [TIME]
     + pypy-3.11.13-[PLATFORM] (pypy3.11)
    Bytecode compiled [COUNT] files in [TIME]
    ");
}
