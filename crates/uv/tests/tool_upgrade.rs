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
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + babel==2.6.0
     + pytz==2018.5
    Installed 1 executable: pybabel
    "###);

    // Upgrade `babel` by installing from PyPI, which should upgrade to the latest version.
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
    Updated babel v2.6.0 -> v2.14.0
     - babel==2.6.0
     + babel==2.14.0
     - pytz==2018.5
    Installed 1 executable: pybabel
    "###);
}

#[test]
fn test_tool_upgrade_multiple_names() {
    let context = TestContext::new("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `python-dotenv` from Test PyPI, to get an outdated version.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("python-dotenv")
        .arg("--index-url")
        .arg("https://test.pypi.org/simple/")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + python-dotenv==0.10.2.post2
    Installed 1 executable: dotenv
    "###);

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
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + babel==2.6.0
     + pytz==2018.5
    Installed 1 executable: pybabel
    "###);

    // Upgrade `babel` and `python-dotenv` from PyPI.
    uv_snapshot!(context.filters(), context.tool_upgrade()
        .arg("babel")
        .arg("python-dotenv")
        .arg("--index-url")
        .arg("https://pypi.org/simple/")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Updated babel v2.6.0 -> v2.14.0
     - babel==2.6.0
     + babel==2.14.0
     - pytz==2018.5
    Installed 1 executable: pybabel
    Updated python-dotenv v0.10.2.post2 -> v1.0.1
     - python-dotenv==0.10.2.post2
     + python-dotenv==1.0.1
    Installed 1 executable: dotenv
    "###);
}

#[test]
fn test_tool_upgrade_all() {
    let context = TestContext::new("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `python-dotenv` from Test PyPI, to get an outdated version.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("python-dotenv")
        .arg("--index-url")
        .arg("https://test.pypi.org/simple/")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + python-dotenv==0.10.2.post2
    Installed 1 executable: dotenv
    "###);

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
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + babel==2.6.0
     + pytz==2018.5
    Installed 1 executable: pybabel
    "###);

    // Upgrade all from PyPI.
    uv_snapshot!(context.filters(), context.tool_upgrade()
        .arg("--all")
        .arg("--index-url")
        .arg("https://pypi.org/simple/")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Updated babel v2.6.0 -> v2.14.0
     - babel==2.6.0
     + babel==2.14.0
     - pytz==2018.5
    Installed 1 executable: pybabel
    Updated python-dotenv v0.10.2.post2 -> v1.0.1
     - python-dotenv==0.10.2.post2
     + python-dotenv==1.0.1
    Installed 1 executable: dotenv
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
    Nothing to upgrade
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
    Updated black v23.1.0 -> v24.3.0
     - black==23.1.0
     + black==24.3.0
    Installed 2 executables: black, blackd
    "###);
}

#[test]
fn test_tool_upgrade_respect_constraints() {
    let context = TestContext::new("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `babel` from Test PyPI, to get an outdated version.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("babel<2.10")
        .arg("--index-url")
        .arg("https://test.pypi.org/simple/")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + babel==2.6.0
     + pytz==2018.5
    Installed 1 executable: pybabel
    "###);

    // Upgrade `babel` from PyPI. It should be updated, but not beyond the constraint.
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
    Updated babel v2.6.0 -> v2.9.1
     - babel==2.6.0
     + babel==2.9.1
     - pytz==2018.5
     + pytz==2024.1
    Installed 1 executable: pybabel
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
    Updated babel v2.6.0 -> v2.13.1
     - babel==2.6.0
     + babel==2.13.1
     - pytz==2018.5
     + setuptools==69.2.0
    Installed 1 executable: pybabel
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
    Updated babel v2.13.1 -> v2.14.0
     - babel==2.13.1
     + babel==2.14.0
     - setuptools==69.2.0
    Installed 1 executable: pybabel
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
    Nothing to upgrade
    "###);
}

/// Upgrade a tool, but only by upgrading one of it's `--with` dependencies, and not the tool
/// itself.
#[test]
fn test_tool_upgrade_with() {
    let context = TestContext::new("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `babel` from Test PyPI, to get an outdated version.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("babel==2.6.0")
        .arg("--index-url")
        .arg("https://test.pypi.org/simple/")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + babel==2.6.0
     + pytz==2018.5
    Installed 1 executable: pybabel
    "###);

    // Upgrade `babel` from PyPI. It shouldn't be updated, but `pytz` should be.
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
    Modified babel environment
     - pytz==2018.5
     + pytz==2024.1
    "###);
}

/// Test that upgrading a tool preserves man pages correctly.
/// This tests the scenario where a tool with man pages is upgraded to the same version
/// (verifying man pages are retained).
#[test]
fn test_tool_upgrade_preserves_manpages() {
    use assert_fs::prelude::*;

    let context = TestContext::new("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");
    let xdg_data_dir = context.temp_dir.child("xdg_data");
    let man_dir = xdg_data_dir.child("man");

    // Install `pycowsay` (has man6 page)
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("pycowsay")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("XDG_DATA_HOME", xdg_data_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + pycowsay==0.0.0.2
    Installed 1 executable: pycowsay
    Installed 1 manpage: man6/pycowsay.6
    "###);

    // Verify man page was installed
    man_dir
        .child("man6")
        .child("pycowsay.6")
        .assert(predicates::path::exists());

    // Upgrade `pycowsay` (should be a no-op since already latest)
    uv_snapshot!(context.filters(), context.tool_upgrade()
        .arg("pycowsay")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("XDG_DATA_HOME", xdg_data_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Nothing to upgrade
    "###);

    // Verify man page still exists after upgrade attempt
    man_dir
        .child("man6")
        .child("pycowsay.6")
        .assert(predicates::path::exists());
}

/// Test upgrading a tool without man pages (baseline for comparison).
#[test]
fn test_tool_upgrade_without_manpages() {
    let context = TestContext::new("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black` (no man pages)
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black==23.1.0")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
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

    // Upgrade `black` to a newer version (note: may be nothing to upgrade if already latest)
    uv_snapshot!(context.filters(), context.tool_upgrade()
        .arg("black")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Nothing to upgrade
    "###);
}

/// Test that upgrading all tools preserves man pages.
#[test]
fn test_tool_upgrade_all_with_manpages() {
    use assert_fs::prelude::*;

    let context = TestContext::new("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");
    let xdg_data_dir = context.temp_dir.child("xdg_data");
    let man_dir = xdg_data_dir.child("man");

    // Install `pycowsay` (has man6 page)
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("pycowsay")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("XDG_DATA_HOME", xdg_data_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + pycowsay==0.0.0.2
    Installed 1 executable: pycowsay
    Installed 1 manpage: man6/pycowsay.6
    "###);

    // Install `black` (no man pages)
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black==23.1.0")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("XDG_DATA_HOME", xdg_data_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
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

    // Verify man page exists before upgrade
    man_dir
        .child("man6")
        .child("pycowsay.6")
        .assert(predicates::path::exists());

    // Upgrade all tools (note: may be nothing to upgrade if already latest)
    uv_snapshot!(context.filters(), context.tool_upgrade()
        .arg("--all")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("XDG_DATA_HOME", xdg_data_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Nothing to upgrade
    "###);

    // Verify man page still exists after upgrading all
    man_dir
        .child("man6")
        .child("pycowsay.6")
        .assert(predicates::path::exists());
}
