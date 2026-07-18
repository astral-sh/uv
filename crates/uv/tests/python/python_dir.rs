use anyhow::Result;
use assert_fs::fixture::{FileWriteStr, PathChild};
use indoc::indoc;

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

    uv_snapshot!(context.filters(), context.python_dir()
    .arg("--no-config")
    .env(EnvVars::UV_PYTHON_INSTALL_DIR, python_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/python

    ----- stderr -----
    ");
}

#[test]
fn python_dir_workspace_config() -> Result<()> {
    let context =
        uv_test::test_context!("3.12").with_filter((uv_version::version(), "[UV_VERSION]"));
    let workspace = context.temp_dir.child("workspace");
    let member = workspace.child("packages").child("member");

    workspace.child("pyproject.toml").write_str(indoc! {r#"
        [tool.uv.workspace]
        members = ["packages/*"]
    "#})?;
    workspace
        .child("uv.toml")
        .write_str("required-version = \">=9999\"\n")?;
    member.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "member"
        version = "0.1.0"

        [tool.uv]
        required-version = ">=0"
    "#})?;

    uv_snapshot!(context.filters(), context.python_dir().current_dir(member.path()), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Required uv version `>=9999` does not match the running version `[UV_VERSION]`
    ");

    Ok(())
}
