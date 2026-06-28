use assert_fs::prelude::{FileWriteStr, PathChild};

use uv_static::EnvVars;

use uv_test::uv_snapshot;

#[test]
fn python_dir() {
    let context = uv_test::test_context!("3.12");

    let python_dir = context.temp_dir.child("python");
    uv_snapshot!(context.filters(), context.python_dir()
    .env(EnvVars::UV_PYTHON_INSTALL_DIR, python_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/python

    ----- stderr -----
    ");
}

/// The `python-install-dir` setting in `uv.toml` configures the managed Python directory.
#[test]
fn python_dir_config() -> anyhow::Result<()> {
    let context = uv_test::test_context!("3.12");

    let python_dir = context.temp_dir.child("python");
    context.temp_dir.child("uv.toml").write_str(&format!(
        "python-install-dir = '{}'\n",
        python_dir.path().display()
    ))?;

    uv_snapshot!(context.filters(), context.python_dir(), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/python

    ----- stderr -----
    ");

    Ok(())
}

/// The `UV_PYTHON_INSTALL_DIR` environment variable takes precedence over the `python-install-dir`
/// setting in `uv.toml`.
#[test]
fn python_dir_config_env_precedence() -> anyhow::Result<()> {
    let context = uv_test::test_context!("3.12");

    let config_dir = context.temp_dir.child("config-python");
    let env_dir = context.temp_dir.child("env-python");
    context.temp_dir.child("uv.toml").write_str(&format!(
        "python-install-dir = '{}'\n",
        config_dir.path().display()
    ))?;

    uv_snapshot!(context.filters(), context.python_dir()
    .env(EnvVars::UV_PYTHON_INSTALL_DIR, env_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/env-python

    ----- stderr -----
    ");

    Ok(())
}
