use anyhow::Result;
use assert_fs::{fixture::PathChild, prelude::FileWriteStr};

use crate::common::{TestContext, uv_snapshot};

#[test]
fn add_package_native_keyring() -> Result<()> {
    let context = TestContext::new("3.12").with_real_home();

    // Clear state before the test
    context
        .auth_logout()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--username")
        .arg("public")
        .status()?;

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
    Logged in to public@https://pypi-proxy.fly.dev/basic-auth/simple
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
    Logged out of public@https://pypi-proxy.fly.dev/basic-auth/simple
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
fn show_native_keyring() -> Result<()> {
    let context = TestContext::new_with_versions(&[]).with_real_home();

    // Clear state before the test
    context
        .auth_logout()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--username")
        .arg("public")
        .status()?;

    // Without a service name
    uv_snapshot!(context.auth_token(), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: the following required arguments were not provided:
      <SERVICE>

    Usage: uv auth token --cache-dir [CACHE_DIR] <SERVICE>

    For more information, try '--help'.
    ");

    // Without a keyring provider...
    uv_snapshot!(context.auth_token()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Cannot show credentials with `keyring-provider = disabled`, use `keyring-provider = native` instead
    ");

    // Without persisted credentials
    uv_snapshot!(context.auth_token()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--keyring-provider")
        .arg("native"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to fetch credentials for https://pypi-proxy.fly.dev/basic-auth/simple
    ");

    // Without persisted credentials (with a username in the request)
    uv_snapshot!(context.auth_token()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--username")
        .arg("public")
        .arg("--keyring-provider")
        .arg("native"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to fetch credentials for https://pypi-proxy.fly.dev/basic-auth/simple
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
    Logged in to public@https://pypi-proxy.fly.dev/basic-auth/simple
    "
    );

    // Show the credentials
    uv_snapshot!(context.auth_token()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--username")
        .arg("public")
        .arg("--keyring-provider")
        .arg("native"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    heron

    ----- stderr -----
    ");

    // Without the username
    // TODO(zanieb): Add a hint here if we can?
    uv_snapshot!(context.auth_token()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--keyring-provider")
        .arg("native"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to fetch credentials for https://pypi-proxy.fly.dev/basic-auth/simple
    ");

    // With a mismatched username
    // TODO(zanieb): Add a hint here if we can?
    uv_snapshot!(context.auth_token()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--username")
        .arg("private")
        .arg("--keyring-provider")
        .arg("native"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to fetch credentials for https://pypi-proxy.fly.dev/basic-auth/simple
    ");

    Ok(())
}

#[test]
fn login_native_keyring() -> Result<()> {
    let context = TestContext::new_with_versions(&[]).with_real_home();

    // Clear state before the test
    context
        .auth_logout()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--username")
        .arg("public")
        .status()?;

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

    // Without a username (or token)
    uv_snapshot!(context.auth_login()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--keyring-provider")
        .arg("native"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No username provided; did you mean to provide `--username` or `--token`?
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
    error: No password provided; did you mean to provide `--password` or `--token`?
    ");

    // Without a keyring provider
    uv_snapshot!(context.auth_login()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--token")
        .arg("foo"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Logging in requires setting `keyring-provider = native` for credentials to be retrieved in subsequent commands
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
    Logged in to public@https://pypi-proxy.fly.dev/basic-auth/simple
    "
    );

    Ok(())
}

#[test]
fn login_token_native_keyring() -> Result<()> {
    let context = TestContext::new_with_versions(&[]).with_real_home();

    // Clear state before the test
    context
        .auth_logout()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--username")
        .arg("__token__")
        .status()?;

    // Successful with token
    uv_snapshot!(context.auth_login()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--token")
        .arg("test-token")
        .arg("--keyring-provider")
        .arg("native"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Logged in to https://pypi-proxy.fly.dev/basic-auth/simple
    "
    );

    Ok(())
}

#[test]
fn logout_native_keyring() -> Result<()> {
    let context = TestContext::new_with_versions(&[]).with_real_home();

    // Clear state before the test
    context
        .auth_logout()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--username")
        .arg("public")
        .status()?;

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

    // Logout without a keyring provider
    uv_snapshot!(context.auth_logout()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Logged out of https://pypi-proxy.fly.dev/basic-auth/simple
    ");

    // Logout before logging in (without a username)
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

    // Logout before logging in (with a username)
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
    error: Unable to remove credentials for public@https://pypi-proxy.fly.dev/basic-auth/simple
      Caused by: No matching entry found in secure storage
    ");

    // Login with a username
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
    Logged in to public@https://pypi-proxy.fly.dev/basic-auth/simple
    "
    );

    // Logout without a username
    // TODO(zanieb): Add a hint here if we can?
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

    // Logout with a username
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
    Logged out of public@https://pypi-proxy.fly.dev/basic-auth/simple
    ");

    Ok(())
}

#[test]
fn logout_token_native_keyring() -> Result<()> {
    let context = TestContext::new_with_versions(&[]).with_real_home();

    // Clear state before the test
    context
        .auth_logout()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple")
        .status()?;

    // Login with a token
    uv_snapshot!(context.auth_login()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--token")
        .arg("test-token")
        .arg("--keyring-provider")
        .arg("native"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Logged in to https://pypi-proxy.fly.dev/basic-auth/simple
    "
    );

    // Logout without a username
    uv_snapshot!(context.auth_logout()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--keyring-provider")
        .arg("native"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Logged out of https://pypi-proxy.fly.dev/basic-auth/simple
    ");

    Ok(())
}

#[test]
fn login_url_parsing() {
    let context = TestContext::new_with_versions(&[]).with_real_home();

    // A domain-only service name gets https:// prepended
    uv_snapshot!(context.auth_login()
        .arg("example.com")
        .arg("--username")
        .arg("test")
        .arg("--password")
        .arg("test")
        .arg("--keyring-provider")
        .arg("native"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Logged in to test@https://example.com/
    ");

    // When including a protocol explicitly, it is retained
    uv_snapshot!(context.auth_login()
        .arg("http://example.com")
        .arg("--username")
        .arg("test")
        .arg("--password")
        .arg("test")
        .arg("--keyring-provider")
        .arg("native"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Logged in to test@http://example.com/
    ");

    uv_snapshot!(context.auth_login()
        .arg("https://example.com")
        .arg("--username")
        .arg("test")
        .arg("--password")
        .arg("test")
        .arg("--keyring-provider")
        .arg("native"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Logged in to test@https://example.com/
    ");

    // A domain-only service with a path also gets https:// prepended
    uv_snapshot!(context.auth_login()
        .arg("example.com/simple")
        .arg("--username")
        .arg("test")
        .arg("--password")
        .arg("test")
        .arg("--keyring-provider")
        .arg("native"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Logged in to test@https://example.com/simple
    ");

    // An invalid URL is rejected
    uv_snapshot!(context.auth_login()
        .arg("not a valid url")
        .arg("--username")
        .arg("test")
        .arg("--password")
        .arg("test")
        .arg("--keyring-provider")
        .arg("native"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: invalid value 'not a valid url' for '<SERVICE>': invalid international domain name

    For more information, try '--help'.
    ");
}
