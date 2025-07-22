#[cfg(windows)]
use std::path::PathBuf;

use std::{env, path::Path, process::Command};

use crate::common::{TestContext, uv_snapshot};
use assert_fs::{
    assert::PathAssert,
    prelude::{FileTouch, FileWriteStr, PathChild, PathCreateDir},
};
use indoc::indoc;
use predicates::prelude::predicate;
use tracing::debug;

use uv_fs::Simplified;
use uv_static::EnvVars;

#[test]
fn python_install() {
    let context: TestContext = TestContext::new_with_versions(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs();

    // Install the latest version
    uv_snapshot!(context.filters(), context.python_install(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.5 in [TIME]
     + cpython-3.13.5-[PLATFORM] (python3.13)
    ");

    let bin_python = context
        .bin_dir
        .child(format!("python3.13{}", std::env::consts::EXE_SUFFIX));

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
                read_link(&bin_python), @"[TEMP_DIR]/managed/cpython-3.13.5-[PLATFORM]/bin/python3.13"
            );
        });
    } else if cfg!(windows) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python), @"[TEMP_DIR]/managed/cpython-3.13.5-[PLATFORM]/python"
            );
        });
    }

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
    uv_snapshot!(context.filters(), context.python_install().arg("3.13").arg("--reinstall"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.5 in [TIME]
     ~ cpython-3.13.5-[PLATFORM] (python3.13)
    ");

    // The executable should still be present in the bin directory
    bin_python.assert(predicate::path::exists());

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

    uv_snapshot!(context.filters(), context.python_uninstall().arg("3.13"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Searching for Python versions matching: Python 3.13
    Uninstalled Python 3.13.5 in [TIME]
     - cpython-3.13.5-[PLATFORM] (python3.13)
    ");

    // The executable should be removed
    bin_python.assert(predicate::path::missing());
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
     + cpython-3.12.11-[PLATFORM] (python3.12)
     + cpython-3.13.5-[PLATFORM] (python3.13)
    ");

    // Reinstall a single version
    uv_snapshot!(context.filters(), context.python_install().arg("3.13").arg("--reinstall"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.5 in [TIME]
     ~ cpython-3.13.5-[PLATFORM] (python3.13)
    ");

    // Reinstall multiple versions
    uv_snapshot!(context.filters(), context.python_install().arg("--reinstall"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed 2 versions in [TIME]
     ~ cpython-3.12.11-[PLATFORM] (python3.12)
     ~ cpython-3.13.5-[PLATFORM] (python3.13)
    ");

    // Reinstalling a version that is not installed should also work
    uv_snapshot!(context.filters(), context.python_install().arg("3.11").arg("--reinstall"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.11.13 in [TIME]
     + cpython-3.11.13-[PLATFORM] (python3.11)
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
     + cpython-3.12.7-[PLATFORM] (python3.12)
    ");

    // Reinstall all "3.12" versions
    // TODO(zanieb): This doesn't work today, because we need this to install the "latest" as there
    // is no workflow for `--upgrade` yet
    uv_snapshot!(context.filters(), context.python_install().arg("3.12").arg("--reinstall"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.12.11 in [TIME]
     + cpython-3.12.11-[PLATFORM] (python3.12)
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
        .arg("python").arg("-c").arg("import sys; print(sys.version_info[:2])"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No interpreter found in [PYTHON SOURCES]

    hint: A managed Python download is available, but Python downloads are set to 'never'
    ");

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

/// Regression test for a bad cpython runtime
/// <https://github.com/astral-sh/uv/issues/13610>
#[test]
fn regression_cpython() {
    let context: TestContext = TestContext::new_with_versions(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_filtered_python_sources()
        .with_managed_python_dirs();

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
        .env_remove("VIRTUAL_ENV")
        .arg("-p").arg("3.12")
        .arg("mre.py"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----

    "###);
}

#[test]
fn python_install_force() {
    let context: TestContext = TestContext::new_with_versions(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs();

    // Install the latest version
    uv_snapshot!(context.filters(), context.python_install(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.5 in [TIME]
     + cpython-3.13.5-[PLATFORM] (python3.13)
    ");

    let bin_python = context
        .bin_dir
        .child(format!("python3.13{}", std::env::consts::EXE_SUFFIX));

    // You can force replacement of the executables
    uv_snapshot!(context.filters(), context.python_install().arg("--force"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.5 in [TIME]
     + cpython-3.13.5-[PLATFORM] (python3.13)
    ");

    // The executable should still be present in the bin directory
    bin_python.assert(predicate::path::exists());

    // If an unmanaged executable is present, `--force` is required
    fs_err::remove_file(bin_python.path()).unwrap();
    bin_python.touch().unwrap();

    uv_snapshot!(context.filters(), context.python_install().arg("3.13"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: Failed to install executable for cpython-3.13.5-[PLATFORM]
      Caused by: Executable already exists at `[BIN]/python3.13` but is not managed by uv; use `--force` to replace it
    ");

    uv_snapshot!(context.filters(), context.python_install().arg("--force").arg("3.13"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.5 in [TIME]
     + cpython-3.13.5-[PLATFORM] (python3.13)
    ");

    bin_python.assert(predicate::path::exists());
}

#[test]
fn python_install_minor() {
    let context: TestContext = TestContext::new_with_versions(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs();

    // Install a minor version
    uv_snapshot!(context.filters(), context.python_install().arg("3.11"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.11.13 in [TIME]
     + cpython-3.11.13-[PLATFORM] (python3.11)
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
                read_link(&bin_python), @"[TEMP_DIR]/managed/cpython-3.11.13-[PLATFORM]/bin/python3.11"
            );
        });
    } else if cfg!(windows) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python), @"[TEMP_DIR]/managed/cpython-3.11.13-[PLATFORM]/python"
            );
        });
    }

    uv_snapshot!(context.filters(), context.python_uninstall().arg("3.11"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Searching for Python versions matching: Python 3.11
    Uninstalled Python 3.11.13 in [TIME]
     - cpython-3.11.13-[PLATFORM] (python3.11)
    ");

    // The executable should be removed
    bin_python.assert(predicate::path::missing());
}

#[test]
fn python_install_multiple_patch() {
    let context: TestContext = TestContext::new_with_versions(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs();

    // Install multiple patch versions
    uv_snapshot!(context.filters(), context.python_install().arg("3.12.8").arg("3.12.6"), @r"
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

    uv_snapshot!(context.filters(), context.python_uninstall().arg("3.12.8"), @r"
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
    let context: TestContext = TestContext::new_with_versions(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs();

    // Install the latest version
    uv_snapshot!(context.filters(), context.python_install().arg("--preview"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.5 in [TIME]
     + cpython-3.13.5-[PLATFORM] (python, python3, python3.13)
    ");

    let bin_python = context
        .bin_dir
        .child(format!("python3.13{}", std::env::consts::EXE_SUFFIX));

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
                read_link(&bin_python), @"[TEMP_DIR]/managed/cpython-3.13-[PLATFORM]/bin/python3.13"
            );
        });
    } else if cfg!(windows) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python), @"[TEMP_DIR]/managed/cpython-3.13-[PLATFORM]/python"
            );
        });
    }

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
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("--reinstall"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.5 in [TIME]
     ~ cpython-3.13.5-[PLATFORM] (python, python3, python3.13)
    ");

    // The executable should still be present in the bin directory
    bin_python.assert(predicate::path::exists());

    // You can also force replacement of the executables
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("--force"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.5 in [TIME]
     + cpython-3.13.5-[PLATFORM] (python, python3, python3.13)
    ");

    // The executable should still be present in the bin directory
    bin_python.assert(predicate::path::exists());

    // If an unmanaged executable is present, `--force` is required
    fs_err::remove_file(bin_python.path()).unwrap();
    bin_python.touch().unwrap();

    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.13"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: Failed to install executable for cpython-3.13.5-[PLATFORM]
      Caused by: Executable already exists at `[BIN]/python3.13` but is not managed by uv; use `--force` to replace it
    ");

    // With `--bin`, this should error instead of warn
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("--bin").arg("3.13"), @r"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    error: Failed to install executable for cpython-3.13.5-[PLATFORM]
      Caused by: Executable already exists at `[BIN]/python3.13` but is not managed by uv; use `--force` to replace it
    ");
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.13").env(EnvVars::UV_PYTHON_INSTALL_BIN, "1"), @r"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    error: Failed to install executable for cpython-3.13.5-[PLATFORM]
      Caused by: Executable already exists at `[BIN]/python3.13` but is not managed by uv; use `--force` to replace it
    ");

    // With `--no-bin`, this should be silent
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("--no-bin").arg("3.13"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    ");
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.13").env(EnvVars::UV_PYTHON_INSTALL_BIN, "0"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    ");

    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("--force").arg("3.13"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.5 in [TIME]
     + cpython-3.13.5-[PLATFORM] (python3.13)
    ");

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

    uv_snapshot!(context.filters(), context.python_uninstall().arg("3.13"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Searching for Python versions matching: Python 3.13
    Uninstalled Python 3.13.5 in [TIME]
     - cpython-3.13.5-[PLATFORM] (python, python3, python3.13)
    ");

    // The executable should be removed
    bin_python.assert(predicate::path::missing());

    // Install a minor version
    uv_snapshot!(context.filters(), context.python_install().arg("3.11").arg("--preview"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.11.13 in [TIME]
     + cpython-3.11.13-[PLATFORM] (python3.11)
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

    uv_snapshot!(context.filters(), context.python_uninstall().arg("3.11"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Searching for Python versions matching: Python 3.11
    Uninstalled Python 3.11.13 in [TIME]
     - cpython-3.11.13-[PLATFORM] (python3.11)
    ");

    // Install multiple patch versions
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.12.8").arg("3.12.6"), @r"
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
    let context: TestContext = TestContext::new_with_versions(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs();

    // Install the latest version
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("--no-bin"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.5 in [TIME]
     + cpython-3.13.5-[PLATFORM]
    ");

    let bin_python = context
        .bin_dir
        .child(format!("python3.13{}", std::env::consts::EXE_SUFFIX));

    // The executable should not be installed in the bin directory
    bin_python.assert(predicate::path::missing());

    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("--no-bin").arg("--default"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: the argument '--no-bin' cannot be used with '--default'

    Usage: uv python install --no-bin --install-dir <INSTALL_DIR> [TARGETS]...

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
    let context = TestContext::new_with_versions(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs();

    let bin_python = context
        .bin_dir
        .child(format!("python3.12{}", std::env::consts::EXE_SUFFIX));

    // Install 3.12.5
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.12.5"), @r"
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
    let context: TestContext = TestContext::new_with_versions(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs();

    // Install the latest version
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.13t"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.5 in [TIME]
     + cpython-3.13.5+freethreaded-[PLATFORM] (python3.13t)
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
        .arg("-c").arg("import subprocess; print('hello world')"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    hello world

    ----- stderr -----
    "###);

    // Create a virtual environment with the freethreaded Python
    uv_snapshot!(context.filters(), context.venv().arg("--python").arg("3.13t"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.13.5
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
    uv_snapshot!(context.filters(), context.python_install().arg("3.13"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.5 in [TIME]
     + cpython-3.13.5-[PLATFORM] (python3.13)
    ");

    // Should not work with older Python versions
    uv_snapshot!(context.filters(), context.python_install().arg("3.12t"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No download found for request: cpython-3.12t-[PLATFORM]
    ");

    uv_snapshot!(context.filters(), context.python_uninstall().arg("--all"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Searching for Python installations
    Uninstalled 2 versions in [TIME]
     - cpython-3.13.5+freethreaded-[PLATFORM] (python3.13t)
     - cpython-3.13.5-[PLATFORM] (python3.13)
    ");
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

    // Install a specific version
    uv_snapshot!(context.filters(), context.python_install().arg("3.13"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.5 in [TIME]
     + cpython-3.13.5-[PLATFORM] (python3.13)
    ");

    // Only the minor versioned executable should be installed
    bin_python_minor_13.assert(predicate::path::exists());
    bin_python_major.assert(predicate::path::missing());
    bin_python_default.assert(predicate::path::missing());

    // Install again, with `--default`
    uv_snapshot!(context.filters(), context.python_install().arg("--default").arg("3.13"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: The `--default` option is experimental and may change without warning. Pass `--preview` to disable this warning
    Installed Python 3.13.5 in [TIME]
     + cpython-3.13.5-[PLATFORM] (python, python3)
    ");

    // Now all the executables should be installed
    bin_python_minor_13.assert(predicate::path::exists());
    bin_python_major.assert(predicate::path::exists());
    bin_python_default.assert(predicate::path::exists());

    // Uninstall
    uv_snapshot!(context.filters(), context.python_uninstall().arg("--all"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Searching for Python installations
    Uninstalled Python 3.13.5 in [TIME]
     - cpython-3.13.5-[PLATFORM] (python, python3, python3.13)
    ");

    // The executables should be removed
    bin_python_minor_13.assert(predicate::path::missing());
    bin_python_major.assert(predicate::path::missing());
    bin_python_default.assert(predicate::path::missing());

    // Install the latest version, i.e., a "default install"
    uv_snapshot!(context.filters(), context.python_install().arg("--default"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: The `--default` option is experimental and may change without warning. Pass `--preview` to disable this warning
    Installed Python 3.13.5 in [TIME]
     + cpython-3.13.5-[PLATFORM] (python, python3, python3.13)
    ");

    // Since it's a default install, we should include all of the executables
    bin_python_minor_13.assert(predicate::path::exists());
    bin_python_major.assert(predicate::path::exists());
    bin_python_default.assert(predicate::path::exists());

    // And 3.13 should be the default
    if cfg!(unix) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_major), @"[TEMP_DIR]/managed/cpython-3.13.5-[PLATFORM]/bin/python3.13"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_minor_13), @"[TEMP_DIR]/managed/cpython-3.13.5-[PLATFORM]/bin/python3.13"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_default), @"[TEMP_DIR]/managed/cpython-3.13.5-[PLATFORM]/bin/python3.13"
            );
        });
    } else if cfg!(windows) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_major), @"[TEMP_DIR]/managed/cpython-3.13.5-[PLATFORM]/python"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_minor_13), @"[TEMP_DIR]/managed/cpython-3.13.5-[PLATFORM]/python"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_default), @"[TEMP_DIR]/managed/cpython-3.13.5-[PLATFORM]/python"
            );
        });
    }

    // Uninstall again
    uv_snapshot!(context.filters(), context.python_uninstall().arg("3.13"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Searching for Python versions matching: Python 3.13
    Uninstalled Python 3.13.5 in [TIME]
     - cpython-3.13.5-[PLATFORM] (python, python3, python3.13)
    ");

    // We should remove all the executables
    bin_python_minor_13.assert(predicate::path::missing());
    bin_python_major.assert(predicate::path::missing());
    bin_python_default.assert(predicate::path::missing());

    // Install multiple versions, with the `--default` flag
    uv_snapshot!(context.filters(), context.python_install().arg("3.12").arg("3.13").arg("--default"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: The `--default` option is experimental and may change without warning. Pass `--preview` to disable this warning
    error: The `--default` flag cannot be used with multiple targets
    ");

    // Install 3.12 as a new default
    uv_snapshot!(context.filters(), context.python_install().arg("3.12").arg("--default"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: The `--default` option is experimental and may change without warning. Pass `--preview` to disable this warning
    Installed Python 3.12.11 in [TIME]
     + cpython-3.12.11-[PLATFORM] (python, python3, python3.12)
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
                read_link(&bin_python_major), @"[TEMP_DIR]/managed/cpython-3.12.11-[PLATFORM]/bin/python3.12"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_minor_12), @"[TEMP_DIR]/managed/cpython-3.12.11-[PLATFORM]/bin/python3.12"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_default), @"[TEMP_DIR]/managed/cpython-3.12.11-[PLATFORM]/bin/python3.12"
            );
        });
    } else {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_major), @"[TEMP_DIR]/managed/cpython-3.12.11-[PLATFORM]/python"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_minor_12), @"[TEMP_DIR]/managed/cpython-3.12.11-[PLATFORM]/python"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_default), @"[TEMP_DIR]/managed/cpython-3.12.11-[PLATFORM]/python"
            );
        });
    }
}

#[test]
fn python_install_default_preview() {
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

    // Install a specific version
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.13"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.5 in [TIME]
     + cpython-3.13.5-[PLATFORM] (python3.13)
    ");

    // Only the minor versioned executable should be installed
    bin_python_minor_13.assert(predicate::path::exists());
    bin_python_major.assert(predicate::path::missing());
    bin_python_default.assert(predicate::path::missing());

    // Install again, with `--default`
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("--default").arg("3.13"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.5 in [TIME]
     + cpython-3.13.5-[PLATFORM] (python, python3)
    ");

    // Now all the executables should be installed
    bin_python_minor_13.assert(predicate::path::exists());
    bin_python_major.assert(predicate::path::exists());
    bin_python_default.assert(predicate::path::exists());

    // Uninstall
    uv_snapshot!(context.filters(), context.python_uninstall().arg("--all"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Searching for Python installations
    Uninstalled Python 3.13.5 in [TIME]
     - cpython-3.13.5-[PLATFORM] (python, python3, python3.13)
    ");

    // The executables should be removed
    bin_python_minor_13.assert(predicate::path::missing());
    bin_python_major.assert(predicate::path::missing());
    bin_python_default.assert(predicate::path::missing());

    // Install the latest version, i.e., a "default install"
    uv_snapshot!(context.filters(), context.python_install().arg("--default").arg("--preview"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.5 in [TIME]
     + cpython-3.13.5-[PLATFORM] (python, python3, python3.13)
    ");

    // Since it's a default install, we should include all of the executables
    bin_python_minor_13.assert(predicate::path::exists());
    bin_python_major.assert(predicate::path::exists());
    bin_python_default.assert(predicate::path::exists());

    // And 3.13 should be the default
    if cfg!(unix) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_major), @"[TEMP_DIR]/managed/cpython-3.13-[PLATFORM]/bin/python3.13"
            );
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python_major), @"[TEMP_DIR]/managed/cpython-3.13.5-[PLATFORM]/bin/python3.13"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_minor_13), @"[TEMP_DIR]/managed/cpython-3.13-[PLATFORM]/bin/python3.13"
            );
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python_minor_13), @"[TEMP_DIR]/managed/cpython-3.13.5-[PLATFORM]/bin/python3.13"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_default), @"[TEMP_DIR]/managed/cpython-3.13-[PLATFORM]/bin/python3.13"
            );
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python_default), @"[TEMP_DIR]/managed/cpython-3.13.5-[PLATFORM]/bin/python3.13"
            );
        });
    } else if cfg!(windows) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_major), @"[TEMP_DIR]/managed/cpython-3.13-[PLATFORM]/python"
            );
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python_major), @"[TEMP_DIR]/managed/cpython-3.13.5-[PLATFORM]/python"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_minor_13), @"[TEMP_DIR]/managed/cpython-3.13-[PLATFORM]/python"
            );
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python_minor_13), @"[TEMP_DIR]/managed/cpython-3.13.5-[PLATFORM]/python"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_default), @"[TEMP_DIR]/managed/cpython-3.13-[PLATFORM]/python"
            );
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python_default), @"[TEMP_DIR]/managed/cpython-3.13.5-[PLATFORM]/python"
            );
        });
    }

    // Uninstall again
    uv_snapshot!(context.filters(), context.python_uninstall().arg("3.13"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Searching for Python versions matching: Python 3.13
    Uninstalled Python 3.13.5 in [TIME]
     - cpython-3.13.5-[PLATFORM] (python, python3, python3.13)
    ");

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
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.12").arg("--default"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.12.11 in [TIME]
     + cpython-3.12.11-[PLATFORM] (python, python3, python3.12)
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
                canonicalize_link_path(&bin_python_major), @"[TEMP_DIR]/managed/cpython-3.12.11-[PLATFORM]/bin/python3.12"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_minor_12), @"[TEMP_DIR]/managed/cpython-3.12-[PLATFORM]/bin/python3.12"
            );
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python_minor_12), @"[TEMP_DIR]/managed/cpython-3.12.11-[PLATFORM]/bin/python3.12"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_default), @"[TEMP_DIR]/managed/cpython-3.12-[PLATFORM]/bin/python3.12"
            );
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python_default), @"[TEMP_DIR]/managed/cpython-3.12.11-[PLATFORM]/bin/python3.12"
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
                canonicalize_link_path(&bin_python_major), @"[TEMP_DIR]/managed/cpython-3.12.11-[PLATFORM]/python"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_minor_12), @"[TEMP_DIR]/managed/cpython-3.12-[PLATFORM]/python"
            );
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python_minor_12), @"[TEMP_DIR]/managed/cpython-3.12.11-[PLATFORM]/python"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_default), @"[TEMP_DIR]/managed/cpython-3.12-[PLATFORM]/python"
            );
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python_default), @"[TEMP_DIR]/managed/cpython-3.12.11-[PLATFORM]/python"
            );
        });
    }

    // Change the default to 3.13
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.13").arg("--default"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.5 in [TIME]
     + cpython-3.13.5-[PLATFORM] (python, python3, python3.13)
    ");

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
                canonicalize_link_path(&bin_python_major), @"[TEMP_DIR]/managed/cpython-3.13.5-[PLATFORM]/bin/python3.13"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python_minor_13), @"[TEMP_DIR]/managed/cpython-3.13.5-[PLATFORM]/bin/python3.13"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python_minor_12), @"[TEMP_DIR]/managed/cpython-3.12.11-[PLATFORM]/bin/python3.12"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python_default), @"[TEMP_DIR]/managed/cpython-3.13.5-[PLATFORM]/bin/python3.13"
            );
        });
    } else if cfg!(windows) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python_major), @"[TEMP_DIR]/managed/cpython-3.13.5-[PLATFORM]/python"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python_minor_13), @"[TEMP_DIR]/managed/cpython-3.13.5-[PLATFORM]/python"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python_minor_12), @"[TEMP_DIR]/managed/cpython-3.12.11-[PLATFORM]/python"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python_default), @"[TEMP_DIR]/managed/cpython-3.13.5-[PLATFORM]/python"
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
fn python_install_broken_link() {
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
    uv_snapshot!(context.filters(), context.python_install().arg("3.13"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.5 in [TIME]
     + cpython-3.13.5-[PLATFORM] (python3.13)
    ");

    // We should replace the broken symlink
    insta::with_settings!({
        filters => context.filters(),
    }, {
        insta::assert_snapshot!(
            canonicalize_link_path(&bin_python), @"[TEMP_DIR]/managed/cpython-3.13.5-[PLATFORM]/bin/python3.13"
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
    Installed Python 3.12.11 in [TIME]
     + cpython-3.12.11-[PLATFORM] (python3.12)
    ");

    // But prefer explicit requests
    uv_snapshot!(context.filters(), context.python_install().arg("3.11").env(EnvVars::UV_PYTHON, "3.12"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.11.13 in [TIME]
     + cpython-3.11.13-[PLATFORM] (python3.11)
    ");

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
    uv_snapshot!(context.filters(), context.python_uninstall().arg("--all").env(EnvVars::UV_PYTHON, "3.11"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Searching for Python installations
    Uninstalled 2 versions in [TIME]
     - cpython-3.11.13-[PLATFORM] (python3.11)
     - cpython-3.12.11-[PLATFORM] (python3.12)
    ");

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

#[test]
fn python_install_314() {
    let context: TestContext = TestContext::new_with_versions(&[])
        .with_filtered_python_keys()
        .with_managed_python_dirs()
        .with_filtered_exe_suffix();

    // Install 3.14
    // For now, this provides test coverage of pre-release handling
    uv_snapshot!(context.filters(), context.python_install().arg("3.14"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.14.0b4 in [TIME]
     + cpython-3.14.0b4-[PLATFORM] (python3.14)
    ");

    // Install a specific pre-release
    uv_snapshot!(context.filters(), context.python_install().arg("3.14.0a4"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.14.0a4 in [TIME]
     + cpython-3.14.0a4-[PLATFORM]
    ");

    // Add name filtering for the `find` tests, we avoid it in `install` tests because it clobbers
    // the version suffixes which matter in the install logs
    let filters = context
        .filters()
        .iter()
        .map(|(a, b)| ((*a).to_string(), (*b).to_string()))
        .collect::<Vec<_>>();
    let context = context
        .with_filtered_python_install_bin()
        .with_filtered_python_names();

    // We should be able to find this version without opt-in, because there is no stable release
    // installed
    uv_snapshot!(context.filters(), context.python_find().arg("3.14"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.14.0b4-[PLATFORM]/[INSTALL-BIN]/[PYTHON]

    ----- stderr -----
    ");

    // This also applies to `>=` requests, even though pre-releases aren't technically in the range
    uv_snapshot!(context.filters(), context.python_find().arg(">=3.14"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.14.0b4-[PLATFORM]/[INSTALL-BIN]/[PYTHON]

    ----- stderr -----
    ");

    uv_snapshot!(context.filters(), context.python_find().arg("3"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.14.0b4-[PLATFORM]/[INSTALL-BIN]/[PYTHON]

    ----- stderr -----
    ");

    // If we install a stable version, that should be preferred though
    uv_snapshot!(filters, context.python_install().arg("3.13"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.5 in [TIME]
     + cpython-3.13.5-[PLATFORM] (python3.13)
    ");

    uv_snapshot!(context.filters(), context.python_find().arg("3"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13.5-[PLATFORM]/[INSTALL-BIN]/[PYTHON]

    ----- stderr -----
    ");
}

/// Test caching Python archives with `UV_PYTHON_CACHE_DIR`.
#[test]
fn python_install_cached() {
    // It does not make sense to run this test when the developer selected faster test runs
    // by setting the env var.
    if env::var_os("UV_PYTHON_CACHE_DIR").is_some() {
        debug!("Skipping test because UV_PYTHON_CACHE_DIR is set");
        return;
    }

    let context: TestContext = TestContext::new_with_versions(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs();

    let python_cache = context.temp_dir.child("python-cache");

    // Install the latest version
    uv_snapshot!(context.filters(), context
        .python_install()
        .env(EnvVars::UV_PYTHON_CACHE_DIR, python_cache.as_ref()), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.5 in [TIME]
     + cpython-3.13.5-[PLATFORM] (python3.13)
    ");

    let bin_python = context
        .bin_dir
        .child(format!("python3.13{}", std::env::consts::EXE_SUFFIX));

    // The executable should be installed in the bin directory
    bin_python.assert(predicate::path::exists());

    // Should be a no-op when already installed
    uv_snapshot!(context.filters(), context
        .python_install()
        .env(EnvVars::UV_PYTHON_CACHE_DIR, python_cache.as_ref()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Python is already installed. Use `uv python install <request>` to install another version.
    "###);

    uv_snapshot!(context.filters(), context.python_uninstall().arg("3.13"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Searching for Python versions matching: Python 3.13
    Uninstalled Python 3.13.5 in [TIME]
     - cpython-3.13.5-[PLATFORM] (python3.13)
    ");

    // The cached archive can be installed offline
    uv_snapshot!(context.filters(), context
        .python_install()
        .arg("--offline")
        .env(EnvVars::UV_PYTHON_CACHE_DIR, python_cache.as_ref()), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.5 in [TIME]
     + cpython-3.13.5-[PLATFORM] (python3.13)
    ");

    // 3.12 isn't cached, so it can't be installed
    let mut filters = context.filters();
    filters.push((
        "cpython-3.12.*.tar.gz",
        "cpython-3.12.[PATCH]-[DATE]-[PLATFORM].tar.gz",
    ));
    uv_snapshot!(filters, context
        .python_install()
        .arg("3.12")
        .arg("--offline")
        .env(EnvVars::UV_PYTHON_CACHE_DIR, python_cache.as_ref()), @r"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    error: Failed to install cpython-3.12.11-[PLATFORM]
      Caused by: An offline Python installation was requested, but cpython-3.12.[PATCH]-[DATE]-[PLATFORM].tar.gz) is missing in python-cache
    ");
}

#[cfg(target_os = "macos")]
#[test]
fn python_install_emulated_macos() {
    let context: TestContext = TestContext::new_with_versions(&[])
        .with_filtered_exe_suffix()
        .with_managed_python_dirs();

    // Before installation, `uv python list` should not show the x86_64 download
    uv_snapshot!(context.filters(), context.python_list().arg("3.13"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.13.5-macos-aarch64-none    <download available>

    ----- stderr -----
    ");

    // Install an x86_64 version (assuming an aarch64 host)
    uv_snapshot!(context.filters(), context.python_install().arg("cpython-3.13-macos-x86_64"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.5 in [TIME]
     + cpython-3.13.5-macos-x86_64-none (python3.13)
    ");

    // It should be discoverable with `uv python find`
    uv_snapshot!(context.filters(), context.python_find().arg("3.13"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13.5-macos-x86_64-none/bin/python3.13

    ----- stderr -----
    ");

    // And included in `uv python list`
    uv_snapshot!(context.filters(), context.python_list().arg("3.13"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.13.5-macos-aarch64-none    <download available>
    cpython-3.13.5-macos-x86_64-none     managed/cpython-3.13.5-macos-x86_64-none/bin/python3.13

    ----- stderr -----
    ");

    uv_snapshot!(context.filters(), context.python_install().arg("cpython-3.13-macos-aarch64"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.5 in [TIME]
     + cpython-3.13.5-macos-aarch64-none
    ");

    // Once we've installed the native version, it should be preferred over x86_64
    uv_snapshot!(context.filters(), context.python_find().arg("3.13"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13.5-macos-aarch64-none/bin/python3.13

    ----- stderr -----
    ");
}

// A virtual environment should track the latest patch version installed.
#[test]
fn install_transparent_patch_upgrade_uv_venv() {
    let context = TestContext::new_with_versions(&["3.13"])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs()
        .with_filtered_python_install_bin();

    // Install a lower patch version.
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.12.9"), @r"
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
        .arg(context.venv.as_os_str()), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.9
    Creating virtual environment at: .venv
    Activate with: source .venv/[BIN]/activate
    "
    );

    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.9

    ----- stderr -----
    "
    );

    // Install a higher patch version.
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.12.11"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.12.11 in [TIME]
     + cpython-3.12.11-[PLATFORM] (python3.12)
    "
    );

    // Virtual environment should reflect higher version.
    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.11

    ----- stderr -----
    "
    );

    // Install a lower patch version.
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.12.8"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.12.8 in [TIME]
     + cpython-3.12.8-[PLATFORM]
    "
    );

    // Virtual environment should reflect highest version.
    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version"), @r"
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
    let context = TestContext::new_with_versions(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs()
        .with_filtered_python_install_bin();

    // Install 3.12 patches in ascending order list
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.12.9").arg("3.12.11"), @r"
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
        .arg(context.venv.as_os_str()), @r"
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
    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version"), @r"
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
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.10.17").arg("3.10.16"), @r"
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
        .arg(context.venv.as_os_str()), @r"
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
    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version"), @r"
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
    let context = TestContext::new_with_versions(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs()
        .with_filtered_python_install_bin();

    // Install patches in ascending order list
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.12.11").arg("3.12.9").arg("3.12.8"), @r"
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
        .arg(context.venv.as_os_str()), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.11
    Creating virtual environment at: .venv
    Activate with: source .venv/[BIN]/activate
    "
    );

    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.11

    ----- stderr -----
    "
    );

    // Uninstall the highest patch version
    uv_snapshot!(context.filters(), context.python_uninstall().arg("--preview").arg("3.12.11"), @r"
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
    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version"), @r"
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
    let context = TestContext::new_with_versions(&["3.13"])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs()
        .with_filtered_python_install_bin();

    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.12.9"), @r"
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
        .arg(context.venv.as_os_str()), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.9
    Creating virtual environment at: .venv
    Activate with: source .venv/[BIN]/activate
    "
    );

    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.9

    ----- stderr -----
    "
    );

    // Install a higher patch version.
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.12.11"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.12.11 in [TIME]
     + cpython-3.12.11-[PLATFORM] (python3.12)
    "
    );

    // The virtual environment Python version is transparently upgraded.
    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version"), @r"
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
    let context = TestContext::new_with_versions(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs()
        .with_filtered_python_install_bin();

    let bin_dir = context.temp_dir.child("bin");

    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.12.9"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.12.9 in [TIME]
     + cpython-3.12.9-[PLATFORM] (python3.12)
    "
    );

    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.9

    ----- stderr -----
    "
    );

    // Create a virtual environment using venv module.
    uv_snapshot!(context.filters(), context.run().arg("python").arg("-m").arg("venv").arg(context.venv.as_os_str()).arg("--without-pip")
        .env(EnvVars::PATH, bin_dir.as_os_str()), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    ");

    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.9

    ----- stderr -----
    "
    );

    // Install a higher patch version
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.12.11"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.12.11 in [TIME]
     + cpython-3.12.11-[PLATFORM] (python3.12)
    "
    );

    // Virtual environment should reflect highest patch version.
    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version"), @r"
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
    let context = TestContext::new_with_versions(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs()
        .with_filtered_python_install_bin();

    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.12.11"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.12.11 in [TIME]
     + cpython-3.12.11-[PLATFORM] (python3.12)
    "
    );

    uv_snapshot!(context.filters(), context.venv().arg("-p").arg("3.12")
        .arg(context.venv.as_os_str()), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.11
    Creating virtual environment at: .venv
    Activate with: source .venv/[BIN]/activate
    "
    );

    uv_snapshot!(context.filters(), context.init().arg("-p").arg("3.12.9").arg("proj"), @r"
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
        .arg("-p").arg("3.12.9"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.9
    Creating virtual environment at: .venv
    Activate with: source .venv/[BIN]/activate
    ");

    // Original virtual environment should still point to higher patch
    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version"), @r"
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
    let context = TestContext::new_with_versions(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs()
        .with_filtered_virtualenv_bin();

    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.10.17"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.10.17 in [TIME]
     + cpython-3.10.17-[PLATFORM] (python3.10)
    "
    );

    uv_snapshot!(context.filters(), context.venv().arg("-p").arg("3.10"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.10.17
    Creating virtual environment at: .venv
    Activate with: source .venv/[BIN]/activate
    "
    );

    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.10.17

    ----- stderr -----
    "
    );

    uv_snapshot!(context.filters(), context.python_uninstall().arg("--preview").arg("3.10.17"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Searching for Python versions matching: Python 3.10.17
    Uninstalled Python 3.10.17 in [TIME]
     - cpython-3.10.17-[PLATFORM] (python3.10)
    "
    );

    let mut filters = context.filters();
    filters.push(("python3", "python"));

    #[cfg(unix)]
    uv_snapshot!(filters, context.run().arg("python").arg("--version"), @r"
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
    uv_snapshot!(filters, context.run().arg("python").arg("--version"), @r#"
    success: false
    exit_code: 103
    ----- stdout -----

    ----- stderr -----
    No Python at '"[TEMP_DIR]/managed/cpython-3.10-[PLATFORM]/python'
    "#
    );
}
