use std::{path::Path, process::Command};

use assert_fs::{
    assert::PathAssert,
    prelude::{FileTouch, PathChild, PathCreateDir},
};
use predicates::prelude::predicate;
use uv_fs::Simplified;

use crate::common::{uv_snapshot, TestContext};

#[test]
fn python_install() {
    let context: TestContext = TestContext::new_with_versions(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix();

    // Install the latest version
    uv_snapshot!(context.filters(), context.python_install(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.1 in [TIME]
     + cpython-3.13.1-[PLATFORM]
    "###);

    let bin_python = context
        .temp_dir
        .child("bin")
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
    uv_snapshot!(context.filters(), context.python_install().arg("--reinstall"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.1 in [TIME]
     ~ cpython-3.13.1-[PLATFORM]
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
    Uninstalled Python 3.13.1 in [TIME]
     - cpython-3.13.1-[PLATFORM]
    "###);
}

#[test]
fn python_install_preview() {
    let context: TestContext = TestContext::new_with_versions(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix();

    // Install the latest version
    uv_snapshot!(context.filters(), context.python_install().arg("--preview"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.1 in [TIME]
     + cpython-3.13.1-[PLATFORM] (python, python3, python3.13)
    "###);

    let bin_python = context
        .temp_dir
        .child("bin")
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
    Installed Python 3.13.1 in [TIME]
     ~ cpython-3.13.1-[PLATFORM] (python, python3, python3.13)
    "###);

    // The executable should still be present in the bin directory
    bin_python.assert(predicate::path::exists());

    // You can also force replacement of the executables
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("--force"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.1 in [TIME]
     + cpython-3.13.1-[PLATFORM] (python, python3, python3.13)
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
    error: Failed to install cpython-3.13.1-[PLATFORM]
      Caused by: Executable already exists at `[TEMP_DIR]/bin/python3.13` but is not managed by uv; use `--force` to replace it
    "###);

    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("--force").arg("3.13"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.1 in [TIME]
     + cpython-3.13.1-[PLATFORM] (python3.13)
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
    Uninstalled Python 3.13.1 in [TIME]
     - cpython-3.13.1-[PLATFORM] (python, python3, python3.13)
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
        .temp_dir
        .child("bin")
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
        .with_filtered_exe_suffix();

    let bin_python = context
        .temp_dir
        .child("bin")
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
        .with_filtered_exe_suffix();

    // Install the latest version
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.13t"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.1 in [TIME]
     + cpython-3.13.1+freethreaded-[PLATFORM] (python3.13t)
    "###);

    let bin_python = context
        .temp_dir
        .child("bin")
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
    Installed Python 3.13.1 in [TIME]
     + cpython-3.13.1-[PLATFORM]
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
     - cpython-3.13.1-[PLATFORM]
     - cpython-3.13.1+freethreaded-[PLATFORM] (python3.13t)
    "###);
}

#[test]
fn python_install_invalid_request() {
    let context: TestContext = TestContext::new_with_versions(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix();

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
        .with_filtered_exe_suffix();

    let bin_python_minor_13 = context
        .temp_dir
        .child("bin")
        .child(format!("python3.13{}", std::env::consts::EXE_SUFFIX));

    let bin_python_major = context
        .temp_dir
        .child("bin")
        .child(format!("python3{}", std::env::consts::EXE_SUFFIX));

    let bin_python_default = context
        .temp_dir
        .child("bin")
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
    Installed Python 3.13.1 in [TIME]
     + cpython-3.13.1-[PLATFORM] (python3.13)
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
    Installed Python 3.13.1 in [TIME]
     + cpython-3.13.1-[PLATFORM] (python, python3)
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
    Uninstalled Python 3.13.1 in [TIME]
     - cpython-3.13.1-[PLATFORM] (python, python3, python3.13)
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
    Installed Python 3.13.1 in [TIME]
     + cpython-3.13.1-[PLATFORM] (python, python3, python3.13)
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
    Uninstalled Python 3.13.1 in [TIME]
     - cpython-3.13.1-[PLATFORM] (python, python3, python3.13)
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
    Installed Python 3.12.8 in [TIME]
     + cpython-3.12.8-[PLATFORM] (python, python3, python3.12)
    "###);

    let bin_python_minor_12 = context
        .temp_dir
        .child("bin")
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
                read_link_path(&bin_python_major), @"[TEMP_DIR]/managed/cpython-3.12.8-[PLATFORM]/bin/python3.12"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link_path(&bin_python_minor_12), @"[TEMP_DIR]/managed/cpython-3.12.8-[PLATFORM]/bin/python3.12"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link_path(&bin_python_default), @"[TEMP_DIR]/managed/cpython-3.12.8-[PLATFORM]/bin/python3.12"
            );
        });
    } else {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link_path(&bin_python_major), @"[TEMP_DIR]/managed/cpython-3.12.8-[PLATFORM]/python"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link_path(&bin_python_minor_12), @"[TEMP_DIR]/managed/cpython-3.12.8-[PLATFORM]/python"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link_path(&bin_python_default), @"[TEMP_DIR]/managed/cpython-3.12.8-[PLATFORM]/python"
            );
        });
    }

    // Change the default to 3.13
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.13").arg("--default"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.1 in [TIME]
     + cpython-3.13.1-[PLATFORM] (python, python3, python3.13)
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
                read_link_path(&bin_python_major), @"[TEMP_DIR]/managed/cpython-3.13.1-[PLATFORM]/bin/python3.13"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link_path(&bin_python_minor_13), @"[TEMP_DIR]/managed/cpython-3.13.1-[PLATFORM]/bin/python3.13"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link_path(&bin_python_minor_12), @"[TEMP_DIR]/managed/cpython-3.12.8-[PLATFORM]/bin/python3.12"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link_path(&bin_python_default), @"[TEMP_DIR]/managed/cpython-3.13.1-[PLATFORM]/bin/python3.13"
            );
        });
    } else {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link_path(&bin_python_major), @"[TEMP_DIR]/managed/cpython-3.13.1-[PLATFORM]/python"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link_path(&bin_python_minor_13), @"[TEMP_DIR]/managed/cpython-3.13.1-[PLATFORM]/python"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link_path(&bin_python_minor_12), @"[TEMP_DIR]/managed/cpython-3.12.8-[PLATFORM]/python"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link_path(&bin_python_default), @"[TEMP_DIR]/managed/cpython-3.13.1-[PLATFORM]/python"
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
    let context: TestContext = TestContext::new_with_versions(&[]);

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
        .with_filtered_exe_suffix();

    let bin_python = context.temp_dir.child("bin").child("python3.13");

    // Create a broken symlink
    context.temp_dir.child("bin").create_dir_all().unwrap();
    symlink(context.temp_dir.join("does-not-exist"), &bin_python).unwrap();

    // Install
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.13"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.1 in [TIME]
     + cpython-3.13.1-[PLATFORM] (python3.13)
    "###);

    // We should replace the broken symlink
    insta::with_settings!({
        filters => context.filters(),
    }, {
        insta::assert_snapshot!(
            read_link_path(&bin_python), @"[TEMP_DIR]/managed/cpython-3.13.1-[PLATFORM]/bin/python3.13"
        );
    });
}
