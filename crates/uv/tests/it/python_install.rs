use std::process::Command;

use assert_fs::{assert::PathAssert, prelude::PathChild};
use predicates::prelude::predicate;

use crate::common::{uv_snapshot, TestContext};

#[test]
fn python_install() {
    let context: TestContext = TestContext::new_with_versions(&[]).with_filtered_python_keys();

    // Install the latest version
    uv_snapshot!(context.filters(), context.python_install(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Searching for Python installations
    Installed Python 3.13.0 in [TIME]
     + cpython-3.13.0-[PLATFORM]
    warning: `[TEMP_DIR]/bin` is not on your PATH. To use the installed Python executable, run `export PATH="[TEMP_DIR]/bin:$PATH"`.
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
    uv_snapshot!(context.filters(), context.python_install(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Searching for Python installations
    Installed Python 3.13.0 in [TIME]
     + cpython-3.13.0-[PLATFORM]
    warning: `[TEMP_DIR]/bin` is not on your PATH. To use the installed Python executable, run `export PATH="[TEMP_DIR]/bin:$PATH"`.
    "###);

    // Similarly, when a requested version is already installed
    uv_snapshot!(context.filters(), context.python_install().arg("3.13"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Searching for Python versions matching: Python 3.13
    Installed Python 3.13.0 in [TIME]
     + cpython-3.13.0-[PLATFORM]
    warning: `[TEMP_DIR]/bin` is not on your PATH. To use the installed Python executable, run `export PATH="[TEMP_DIR]/bin:$PATH"`.
    "###);

    // The executable should still be present in the bin directory
    bin_python.assert(predicate::path::exists());

    // You can opt-in to a reinstall
    uv_snapshot!(context.filters(), context.python_install().arg("--reinstall"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Searching for Python installations
    Installed Python 3.13.0 in [TIME]
     + cpython-3.13.0-[PLATFORM]
    warning: `[TEMP_DIR]/bin` is not on your PATH. To use the installed Python executable, run `export PATH="[TEMP_DIR]/bin:$PATH"`.
    "###);

    // The executable should still be present in the bin directory
    bin_python.assert(predicate::path::exists());
}

#[test]
fn python_install_freethreaded() {
    let context: TestContext = TestContext::new_with_versions(&[]).with_filtered_python_keys();

    // Install the latest version
    uv_snapshot!(context.filters(), context.python_install().arg("3.13t"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Searching for Python versions matching: Python 3.13t
    Installed Python 3.13.0 in [TIME]
     + cpython-3.13.0+freethreaded-[PLATFORM]
    warning: `[TEMP_DIR]/bin` is not on your PATH. To use the installed Python executable, run `export PATH="[TEMP_DIR]/bin:$PATH"`.
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
    Searching for Python versions matching: Python 3.13
    Installed Python 3.13.0 in [TIME]
     + cpython-3.13.0-[PLATFORM]
    warning: `[TEMP_DIR]/bin` is not on your PATH. To use the installed Python executable, run `export PATH="[TEMP_DIR]/bin:$PATH"`.
    "###);

    // Should not work with older Python versions
    uv_snapshot!(context.filters(), context.python_install().arg("3.12t"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Searching for Python versions matching: Python 3.12t
    error: No download found for request: cpython-3.12t-[PLATFORM]
    "###);
}
