#![cfg(all(feature = "python", feature = "pypi"))]

use assert_fs::prelude::*;

use common::{uv_snapshot, TestContext};

mod common;

#[test]
fn test_tool_upgrade_name() {
    let context = TestContext::new("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black>=23.1")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + black==24.3.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    Installed 2 executables: black, blackd
    "###);

    // Upgrade `black`. This should be a no-op, since we have the latest version already.
    uv_snapshot!(context.filters(), context.tool_upgrade()
        .arg("black")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool upgrade` is experimental and may change without warning
    Resolved [N] packages in [TIME]
    Audited [N] packages in [TIME]
    Updated 2 executables: black, blackd
    "###);
}

#[test]
fn test_tool_upgrade_all() {
    let context = TestContext::new("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black==23.1`.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black==23.1")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + black==23.1.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    Installed 2 executables: black, blackd
    "###);

    // Install `pytest==8.0`.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("pytest==8.0")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + iniconfig==2.0.0
     + packaging==24.0
     + pluggy==1.4.0
     + pytest==8.0.0
    Installed 2 executables: py.test, pytest
    "###);

    // Upgrade all. This is a no-op, since we have the latest versions already.
    uv_snapshot!(context.filters(), context.tool_upgrade()
        .arg("--all")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool upgrade` is experimental and may change without warning
    Resolved [N] packages in [TIME]
    Audited [N] packages in [TIME]
    Updated 2 executables: black, blackd
    Resolved [N] packages in [TIME]
    Audited [N] packages in [TIME]
    Updated 2 executables: py.test, pytest
    "###);
}

#[test]
fn test_tool_upgrade_non_existing_package() {
    let context = TestContext::new("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Attempt to upgrade `black`.
    uv_snapshot!(context.filters(), context.tool_upgrade()
        .arg("black")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool upgrade` is experimental and may change without warning
    `black` is not installed; run `uv tool install black` to install
    "###);

    // Attempt to upgrade all.
    uv_snapshot!(context.filters(), context.tool_upgrade()
        .arg("--all")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool upgrade` is experimental and may change without warning
    Nothing to upgrade
    "###);
}

#[test]
fn test_tool_upgrade_settings() {
    let context = TestContext::new("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black` with `lowest-direct`.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black>=23")
        .arg("--resolution=lowest-direct")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + black==23.1.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    Installed 2 executables: black, blackd
    "###);

    // Upgrade `black`. This should be a no-op, since the resolution is set to `lowest-direct`.
    uv_snapshot!(context.filters(), context.tool_upgrade()
        .arg("black")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool upgrade` is experimental and may change without warning
    Resolved [N] packages in [TIME]
    Audited [N] packages in [TIME]
    Updated 2 executables: black, blackd
    "###);

    // Upgrade `black`, but override the resolution.
    uv_snapshot!(context.filters(), context.tool_upgrade()
        .arg("black")
        .arg("--resolution=highest")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool upgrade` is experimental and may change without warning
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Uninstalled [N] packages in [TIME]
    Installed [N] packages in [TIME]
     - black==23.1.0
     + black==24.3.0
    Updated 2 executables: black, blackd
    "###);
}

#[test]
fn test_tool_upgrade_constraint() {
    let context = TestContext::new("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `babel` from Test PyPI, to get an outdated version.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("babel")
        .arg("--index-url")
        .arg("https://test.pypi.org/simple/")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + babel==2.6.0
     + pytz==2018.5
    Installed 1 executable: pybabel
    "###);

    // Upgrade `babel`, but apply a constraint.
    uv_snapshot!(context.filters(), context.tool_upgrade()
        .arg("babel")
        .arg("--index-url")
        .arg("https://pypi.org/simple/")
        .arg("--upgrade-package")
        .arg("babel<2.14.0")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool upgrade` is experimental and may change without warning
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Uninstalled [N] packages in [TIME]
    Installed [N] packages in [TIME]
     - babel==2.6.0
     + babel==2.13.1
     - pytz==2018.5
     + setuptools==69.2.0
    Updated 1 executable: pybabel
    "###);

    // Upgrade `babel` without a constraint.
    uv_snapshot!(context.filters(), context.tool_upgrade()
        .arg("babel")
        .arg("--index-url")
        .arg("https://pypi.org/simple/")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool upgrade` is experimental and may change without warning
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Uninstalled [N] packages in [TIME]
    Installed [N] packages in [TIME]
     - babel==2.13.1
     + babel==2.14.0
     - setuptools==69.2.0
    Updated 1 executable: pybabel
    "###);

    // Passing `--upgrade` explicitly should warn.
    uv_snapshot!(context.filters(), context.tool_upgrade()
        .arg("babel")
        .arg("--index-url")
        .arg("https://pypi.org/simple/")
        .arg("--upgrade")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `--upgrade` is enabled by default on `uv tool upgrade`
    warning: `uv tool upgrade` is experimental and may change without warning
    Resolved [N] packages in [TIME]
    Audited [N] packages in [TIME]
    Updated 1 executable: pybabel
    "###);
}
