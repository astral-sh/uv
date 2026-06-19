use anyhow::Result;
use assert_fs::fixture::{FileWriteStr, PathChild};
use uv_static::EnvVars;

use uv_test::uv_snapshot;

/// Install with relative `exclude-newer` values from the `uv pip` CLI.
///
/// Uses idna which has releases at:
/// - 3.6: 2023-11-25
/// - 3.7: 2024-04-11
#[test]
fn pip_install_exclude_newer_relative() {
    let context = uv_test::test_context!("3.12");
    let current_timestamp = "2024-05-01T00:00:00Z";

    // 3 weeks before 2024-05-01 is 2024-04-10, which is before idna 3.7.
    uv_snapshot!(context.filters(), context
        .pip_install()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, current_timestamp)
        .arg("--exclude-newer")
        .arg("3 weeks")
        .arg("idna"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + idna==3.6
    ");

    // A package-specific span can relax the global cutoff for that package.
    uv_snapshot!(context.filters(), context
        .pip_install()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, current_timestamp)
        .arg("--exclude-newer")
        .arg("3 weeks")
        .arg("--exclude-newer-package")
        .arg("idna=2 weeks")
        .arg("--upgrade")
        .arg("idna"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - idna==3.6
     + idna==3.7
    ");
}

/// Install with relative `exclude-newer` values from `[tool.uv.pip]`.
///
/// Uses idna which has releases at:
/// - 3.6: 2023-11-25
/// - 3.7: 2024-04-11
#[test]
fn pip_install_exclude_newer_relative_config() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let current_timestamp = "2024-05-01T00:00:00Z";
    let pyproject_toml = context.temp_dir.child("pyproject.toml");

    pyproject_toml.write_str(
        r#"
        [tool.uv.pip]
        exclude-newer = "3 weeks"
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .pip_install()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, current_timestamp)
        .arg("idna"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + idna==3.6
    ");

    pyproject_toml.write_str(
        r#"
        [tool.uv.pip]
        exclude-newer = "3 weeks"
        exclude-newer-package = { idna = "2 weeks" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .pip_install()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, current_timestamp)
        .arg("--upgrade")
        .arg("idna"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - idna==3.6
     + idna==3.7
    ");

    Ok(())
}
