use anyhow::Result;
use assert_fs::{fixture::PathChild, prelude::FileWriteStr};

use crate::common::{TestContext, uv_snapshot};

#[test]
fn add_package_native_keyring() -> Result<()> {
    let context = TestContext::new("3.12").with_real_home();

    // Clear state before the test
    let _ = context
        .auth_logout()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--username")
        .arg("public")
        .status();

    // Configure `pyproject.toml` with native keyring provider.
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc::indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.11, <4"
        dependencies = []

        [tool.uv]
        keyring-provider = "native"
        "#
    })?;

    // Try to add a package without credentials.
    uv_snapshot!(context.add().arg("anyio").arg("--default-index").arg("https://public@pypi-proxy.fly.dev/basic-auth/simple"), @r"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because anyio was not found in the package registry and your project depends on anyio, we can conclude that your project's requirements are unsatisfiable.

          hint: An index URL (https://pypi-proxy.fly.dev/basic-auth/simple) could not be queried due to a lack of valid authentication credentials (401 Unauthorized).
      help: If you want to add the package regardless of the failed resolution, provide the `--frozen` flag to skip locking and syncing.
    "
    );

    // Login to the index
    uv_snapshot!(context.auth_login()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--username")
        .arg("public")
        .arg("--password")
        .arg("heron"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Logged in to https://pypi-proxy.fly.dev/basic-auth/simple
    "
    );

    // Try to add the original package without credentials again. This should use
    // credentials storied in the system keyring.
    uv_snapshot!(context.add().arg("anyio").arg("--default-index").arg("https://public@pypi-proxy.fly.dev/basic-auth/simple"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + sniffio==1.3.1
    "
    );

    // Logout of the index
    uv_snapshot!(context.auth_logout()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--username")
        .arg("public"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Logged out of https://pypi-proxy.fly.dev/basic-auth/simple
    "
    );

    // Authentication should fail again
    uv_snapshot!(context.add().arg("iniconfig").arg("--default-index").arg("https://public@pypi-proxy.fly.dev/basic-auth/simple"), @r"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because iniconfig was not found in the package registry and your project depends on iniconfig, we can conclude that your project's requirements are unsatisfiable.

          hint: An index URL (https://pypi-proxy.fly.dev/basic-auth/simple) could not be queried due to a lack of valid authentication credentials (401 Unauthorized).
      help: If you want to add the package regardless of the failed resolution, provide the `--frozen` flag to skip locking and syncing.
    "
    );

    Ok(())
}

#[test]
fn show_native_keyring() {
    let context = TestContext::new_with_versions(&[]).with_real_home();

    // Clear state before the test
    let _ = context
        .auth_logout()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--username")
        .arg("public")
        .status();

    // Without a service name
    uv_snapshot!(context.auth_show(), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: the following required arguments were not provided:
      <SERVICE>

    Usage: uv auth show --cache-dir [CACHE_DIR] <SERVICE>

    For more information, try '--help'.
    ");

    // Without a keyring provider...
    uv_snapshot!(context.auth_show()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Cannot show credentials with `keyring-provider = disabled`, use `keyring-provider = native` instead
    ");

    // With explicit native keyring provider
    uv_snapshot!(context.auth_show()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--keyring-provider")
        .arg("native"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Cannot show credentials with `keyring-provider = disabled`, use `keyring-provider = native` instead
    ");

    // With username and native keyring provider
    uv_snapshot!(context.auth_show()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--username")
        .arg("public")
        .arg("--keyring-provider")
        .arg("native"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Cannot show credentials with `keyring-provider = disabled`, use `keyring-provider = native` instead
    ");

    // Login to the index
    uv_snapshot!(context.auth_login()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--username")
        .arg("public")
        .arg("--password")
        .arg("heron")
        .arg("--keyring-provider")
        .arg("native"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Logged in to https://pypi-proxy.fly.dev/basic-auth/simple
    "
    );
}

#[test]
fn login_native_keyring() {
    let context = TestContext::new_with_versions(&[]).with_real_home();

    // Clear state before the test
    let _ = context
        .auth_logout()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--username")
        .arg("public")
        .status();

    // Without a service name
    uv_snapshot!(context.auth_login(), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: the following required arguments were not provided:
      <SERVICE>

    Usage: uv auth login --cache-dir [CACHE_DIR] <SERVICE>

    For more information, try '--help'.
    ");

    // Without a keyring provider
    uv_snapshot!(context.auth_login()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: `uv auth login` requires either a `--token` or a username, e.g., `uv auth login https://example.com user`
    ");

    // Without a username or token
    uv_snapshot!(context.auth_login()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--keyring-provider")
        .arg("native"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: `uv auth login` requires either a `--token` or a username, e.g., `uv auth login https://example.com user`
    ");

    // Without a password
    uv_snapshot!(context.auth_login()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--username")
        .arg("public")
        .arg("--keyring-provider")
        .arg("native"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: `uv auth login` requires `--password` when not in a terminal.
    ");

    // Successful
    uv_snapshot!(context.auth_login()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--username")
        .arg("public")
        .arg("--password")
        .arg("heron")
        .arg("--keyring-provider")
        .arg("native"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Logged in to https://pypi-proxy.fly.dev/basic-auth/simple
    "
    );
}

#[test]
fn logout_native_keyring() {
    let context = TestContext::new_with_versions(&[]).with_real_home();

    // Clear state before the test
    let _ = context
        .auth_logout()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--username")
        .arg("public")
        .status();

    // Without a service name
    uv_snapshot!(context.auth_logout(), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: the following required arguments were not provided:
      <SERVICE>

    Usage: uv auth logout --cache-dir [CACHE_DIR] <SERVICE>

    For more information, try '--help'.
    ");

    // Without a keyring provider...
    uv_snapshot!(context.auth_logout()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Unable to remove credentials for https://pypi-proxy.fly.dev/basic-auth/simple
      Caused by: No matching entry found in secure storage
    ");

    // With explicit native keyring provider
    uv_snapshot!(context.auth_logout()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--keyring-provider")
        .arg("native"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Unable to remove credentials for https://pypi-proxy.fly.dev/basic-auth/simple
      Caused by: No matching entry found in secure storage
    ");

    // With username and native keyring provider
    uv_snapshot!(context.auth_logout()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--username")
        .arg("public")
        .arg("--keyring-provider")
        .arg("native"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Unable to remove credentials for https://pypi-proxy.fly.dev/basic-auth/simple
      Caused by: No matching entry found in secure storage
    ");

    // First login to create credentials for testing
    uv_snapshot!(context.auth_login()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--username")
        .arg("public")
        .arg("--password")
        .arg("heron")
        .arg("--keyring-provider")
        .arg("native"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Logged in to https://pypi-proxy.fly.dev/basic-auth/simple
    "
    );

    // Successful logout with credentials present
    uv_snapshot!(context.auth_logout()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--username")
        .arg("public")
        .arg("--keyring-provider")
        .arg("native"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Logged out of https://pypi-proxy.fly.dev/basic-auth/simple
    ");
}
