use assert_fs::prelude::*;
use insta::assert_snapshot;

use uv_static::EnvVars;

use crate::common::{uv_snapshot, TestContext};

#[test]
fn tool_upgrade_empty() {
    let context = TestContext::new("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    uv_snapshot!(context.filters(), context.tool_upgrade()
        .arg("--all")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Nothing to upgrade
    "###);

    uv_snapshot!(context.filters(), context.tool_upgrade()
        .arg("--all")
        .arg("-p")
        .arg("3.13")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Nothing to upgrade
    "###);

    // Install the latest `babel`.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("babel")
        .arg("--index-url")
        .arg("https://pypi.org/simple/")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + babel==2.14.0
    Installed 1 executable: pybabel
    "###);

    uv_snapshot!(context.filters(), context.tool_upgrade()
        .arg("--all")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Nothing to upgrade
    "###);

    uv_snapshot!(context.filters(), context.tool_upgrade()
        .arg("--all")
        .arg("-p")
        .arg("3.12")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Nothing to upgrade
    "###);
}

#[test]
fn tool_upgrade_name() {
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
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @r###"
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
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @r###"
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
fn tool_upgrade_multiple_names() {
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
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @r###"
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
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @r###"
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
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @r###"
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
fn tool_upgrade_all() {
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
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @r###"
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
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @r###"
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
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @r###"
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
fn tool_upgrade_non_existing_package() {
    let context = TestContext::new("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Attempt to upgrade `black`.
    uv_snapshot!(context.filters(), context.tool_upgrade()
        .arg("black")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    error: Failed to upgrade black
      Caused by: `black` is not installed; run `uv tool install black` to install
    "###);

    // Attempt to upgrade all.
    uv_snapshot!(context.filters(), context.tool_upgrade()
        .arg("--all")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Nothing to upgrade
    "###);
}

#[test]
fn tool_upgrade_not_stop_if_upgrade_fails() -> anyhow::Result<()> {
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
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @r###"
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
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @r###"
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

    // Break the receipt for python-dotenv
    tool_dir
        .child("python-dotenv")
        .child("uv-receipt.toml")
        .write_str("Invalid receipt")?;

    // Upgrade all from PyPI.
    uv_snapshot!(context.filters(), context.tool_upgrade()
        .arg("--all")
        .arg("--index-url")
        .arg("https://pypi.org/simple/")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Updated babel v2.6.0 -> v2.14.0
     - babel==2.6.0
     + babel==2.14.0
     - pytz==2018.5
    Installed 1 executable: pybabel
    error: Failed to upgrade python-dotenv
      Caused by: `python-dotenv` is missing a valid receipt; run `uv tool install --force python-dotenv` to reinstall
    "###);

    Ok(())
}

#[test]
fn tool_upgrade_settings() {
    let context = TestContext::new("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black` with `lowest-direct`.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black>=23")
        .arg("--resolution=lowest-direct")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @r###"
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
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @r###"
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
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @r###"
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
fn tool_upgrade_respect_constraints() {
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
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @r###"
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
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @r###"
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
fn tool_upgrade_constraint() {
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
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @r###"
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

    // Upgrade `babel`, but apply a constraint inline.
    uv_snapshot!(context.filters(), context.tool_upgrade()
        .arg("babel<2.12.0")
        .arg("--index-url")
        .arg("https://pypi.org/simple/")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Updated babel v2.6.0 -> v2.11.0
     - babel==2.6.0
     + babel==2.11.0
     - pytz==2018.5
     + pytz==2024.1
    Installed 1 executable: pybabel
    "###);

    // Upgrade `babel`, but apply a constraint via `--upgrade-package`.
    uv_snapshot!(context.filters(), context.tool_upgrade()
        .arg("babel")
        .arg("--index-url")
        .arg("https://pypi.org/simple/")
        .arg("--upgrade-package")
        .arg("babel<2.14.0")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `--upgrade-package` is enabled by default on `uv tool upgrade`
    Updated babel v2.11.0 -> v2.13.1
     - babel==2.11.0
     + babel==2.13.1
     - pytz==2024.1
     + setuptools==69.2.0
    Installed 1 executable: pybabel
    "###);

    // Upgrade `babel` without a constraint.
    uv_snapshot!(context.filters(), context.tool_upgrade()
        .arg("babel")
        .arg("--index-url")
        .arg("https://pypi.org/simple/")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @r###"
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
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @r###"
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
fn tool_upgrade_with() {
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
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @r###"
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
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Modified babel environment
     - pytz==2018.5
     + pytz==2024.1
    "###);
}

#[test]
fn tool_upgrade_python() {
    let context = TestContext::new_with_versions(&["3.11", "3.12"])
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    uv_snapshot!(context.filters(), context.tool_install()
    .arg("babel==2.6.0")
    .arg("--index-url")
    .arg("https://test.pypi.org/simple/")
    .arg("--python").arg("3.11")
    .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
    .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
    .env(EnvVars::PATH, bin_dir.as_os_str()), @r###"
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

    uv_snapshot!(
        context.filters(),
        context.tool_upgrade().arg("babel")
        .arg("--python").arg("3.12")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + babel==2.6.0
     + pytz==2018.5
    Installed 1 executable: pybabel
    Upgraded tool environment for `babel` to Python 3.12
    "###
    );

    insta::with_settings!({
        filters => context.filters(),
    }, {
        let content = fs_err::read_to_string(tool_dir.join("babel").join("pyvenv.cfg")).unwrap();
        let lines: Vec<&str> = content.split('\n').collect();
        assert_snapshot!(lines[lines.len() - 3], @r###"
        version_info = 3.12.[X]
        "###);
    });
}

#[test]
fn tool_upgrade_python_with_all() {
    let context = TestContext::new_with_versions(&["3.11", "3.12"])
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    uv_snapshot!(context.filters(), context.tool_install()
    .arg("babel==2.6.0")
    .arg("--index-url")
    .arg("https://test.pypi.org/simple/")
    .arg("--python").arg("3.11")
    .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
    .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
    .env(EnvVars::PATH, bin_dir.as_os_str()), @r###"
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

    uv_snapshot!(context.filters(), context.tool_install()
    .arg("python-dotenv")
    .arg("--index-url")
    .arg("https://test.pypi.org/simple/")
    .arg("--python").arg("3.11")
    .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
    .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
    .env(EnvVars::PATH, bin_dir.as_os_str()), @r###"
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

    uv_snapshot!(
        context.filters(),
        context.tool_upgrade().arg("--all")
        .arg("--python").arg("3.12")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + babel==2.6.0
     + pytz==2018.5
    Installed 1 executable: pybabel
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + python-dotenv==0.10.2.post2
    Installed 1 executable: dotenv
    Upgraded tool environments for `babel` and `python-dotenv` to Python 3.12
    "###
    );

    insta::with_settings!({
        filters => context.filters(),
    }, {
        let content = fs_err::read_to_string(tool_dir.join("babel").join("pyvenv.cfg")).unwrap();
        let lines: Vec<&str> = content.split('\n').collect();
        assert_snapshot!(lines[lines.len() - 3], @r###"
        version_info = 3.12.[X]
        "###);
    });

    insta::with_settings!({
        filters => context.filters(),
    }, {
        let content = fs_err::read_to_string(tool_dir.join("python-dotenv").join("pyvenv.cfg")).unwrap();
        let lines: Vec<&str> = content.split('\n').collect();
        assert_snapshot!(lines[lines.len() - 3], @r###"
        version_info = 3.12.[X]
        "###);
    });
}
