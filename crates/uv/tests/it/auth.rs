use anyhow::Result;
use assert_cmd::assert::OutputAssertExt;
use assert_fs::{fixture::PathChild, prelude::FileWriteStr};
use uv_static::EnvVars;

use uv_test::uv_snapshot;

#[tokio::test]
#[cfg(feature = "native-auth")]
async fn add_package_native_auth_realm() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_real_home();
    let proxy = crate::pypi_proxy::start().await;

    // Clear state before the test
    context
        .auth_logout()
        .arg(proxy.uri())
        .arg("--username")
        .arg("public")
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth")
        .status()?;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc::indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.11, <4"
        dependencies = []
        "#
    })?;

    // Try to add a package without credentials.
    uv_snapshot!(context.filters(), context.add().arg("anyio").arg("--default-index").arg(proxy.username_url("public", "/basic-auth/simple"))
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because anyio was not found in the package registry and your project depends on anyio, we can conclude that your project's requirements are unsatisfiable.

          hint: An index URL (http://[LOCALHOST]/basic-auth/simple) could not be queried due to a lack of valid authentication credentials (401 Unauthorized).
      help: If you want to add the package regardless of the failed resolution, provide the `--frozen` flag to skip locking and syncing.
    "
    );

    // Login to the domain
    uv_snapshot!(context.filters(), context.auth_login()
        .arg(proxy.uri())
        .arg("--username")
        .arg("public")
        .arg("--password")
        .arg("heron")
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Stored credentials for public@http://[LOCALHOST]/
    "
    );

    // Try to add the original package without credentials again. This should use credentials
    // storied in the system keyring.
    uv_snapshot!(context.filters(), context.add().arg("anyio").arg("--default-index").arg(proxy.username_url("public", "/basic-auth/simple"))
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
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

    // Logout of the domain
    uv_snapshot!(context.filters(), context.auth_logout()
        .arg(proxy.uri())
        .arg("--username")
        .arg("public")
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Removed credentials for public@http://[LOCALHOST]/
    "
    );

    // Authentication should fail again
    uv_snapshot!(context.filters(), context.add().arg("iniconfig").arg("--default-index").arg(proxy.username_url("public", "/basic-auth/simple"))
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because iniconfig was not found in the package registry and your project depends on iniconfig, we can conclude that your project's requirements are unsatisfiable.

          hint: An index URL (http://[LOCALHOST]/basic-auth/simple) could not be queried due to a lack of valid authentication credentials (401 Unauthorized).
      help: If you want to add the package regardless of the failed resolution, provide the `--frozen` flag to skip locking and syncing.
    "
    );

    Ok(())
}

#[tokio::test]
#[cfg(feature = "native-auth")]
async fn add_package_native_auth() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_real_home();
    let proxy = crate::pypi_proxy::start().await;

    // Clear state before the test
    context
        .auth_logout()
        .arg(proxy.url("/basic-auth/simple"))
        .arg("--username")
        .arg("public")
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth")
        .status()?;

    // Configure `pyproject.toml` with native keyring provider.
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc::indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.11, <4"
        dependencies = []
        "#
    })?;

    // Try to add a package without credentials.
    uv_snapshot!(context.filters(), context.add().arg("anyio").arg("--default-index").arg(proxy.username_url("public", "/basic-auth/simple"))
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because anyio was not found in the package registry and your project depends on anyio, we can conclude that your project's requirements are unsatisfiable.

          hint: An index URL (http://[LOCALHOST]/basic-auth/simple) could not be queried due to a lack of valid authentication credentials (401 Unauthorized).
      help: If you want to add the package regardless of the failed resolution, provide the `--frozen` flag to skip locking and syncing.
    "
    );

    // Login to the index
    uv_snapshot!(context.filters(), context.auth_login()
        .arg(proxy.url("/basic-auth/simple"))
        .arg("--username")
        .arg("public")
        .arg("--password")
        .arg("heron")
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Stored credentials for public@http://[LOCALHOST]/basic-auth
    "
    );

    // Try to add the original package without credentials again. This should use
    // credentials storied in the system keyring.
    uv_snapshot!(context.filters(), context.add().arg("anyio").arg("--default-index").arg(proxy.username_url("public", "/basic-auth/simple"))
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
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
    uv_snapshot!(context.filters(), context.auth_logout()
        .arg(proxy.url("/basic-auth/simple"))
        .arg("--username")
        .arg("public")
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Removed credentials for public@http://[LOCALHOST]/basic-auth
    "
    );

    // Authentication should fail again
    uv_snapshot!(context.filters(), context.add().arg("iniconfig").arg("--default-index").arg(proxy.username_url("public", "/basic-auth/simple"))
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because iniconfig was not found in the package registry and your project depends on iniconfig, we can conclude that your project's requirements are unsatisfiable.

          hint: An index URL (http://[LOCALHOST]/basic-auth/simple) could not be queried due to a lack of valid authentication credentials (401 Unauthorized).
      help: If you want to add the package regardless of the failed resolution, provide the `--frozen` flag to skip locking and syncing.
    "
    );

    Ok(())
}

#[tokio::test]
#[cfg(feature = "native-auth")]
async fn token_native_auth() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]).with_real_home();
    let proxy = crate::pypi_proxy::start().await;

    // Clear state before the test
    context
        .auth_logout()
        .arg(proxy.url("/basic-auth/simple"))
        .arg("--username")
        .arg("public")
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth")
        .status()?;

    // Without persisted credentials
    uv_snapshot!(context.filters(), context.auth_token()
        .arg(proxy.url("/basic-auth/simple"))
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to fetch credentials for http://[LOCALHOST]/basic-auth/simple
    ");

    // Without persisted credentials (with a username in the request)
    uv_snapshot!(context.filters(), context.auth_token()
        .arg(proxy.url("/basic-auth/simple"))
        .arg("--username")
        .arg("public")
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to fetch credentials for public@http://[LOCALHOST]/basic-auth/simple
    ");

    // Login to the index
    uv_snapshot!(context.filters(), context.auth_login()
        .arg(proxy.url("/basic-auth/simple"))
        .arg("--username")
        .arg("public")
        .arg("--password")
        .arg("heron")
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Stored credentials for public@http://[LOCALHOST]/basic-auth
    "
    );

    // Show the credentials
    uv_snapshot!(context.filters(), context.auth_token()
        .arg(proxy.url("/basic-auth/simple"))
        .arg("--username")
        .arg("public")
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    heron

    ----- stderr -----
    ");

    // Without the username
    // TODO(zanieb): Add a hint here if we can?
    uv_snapshot!(context.filters(), context.auth_token()
        .arg(proxy.url("/basic-auth/simple"))
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to fetch credentials for http://[LOCALHOST]/basic-auth/simple
    ");

    // With a mismatched username
    // TODO(zanieb): Add a hint here if we can?
    uv_snapshot!(context.filters(), context.auth_token()
        .arg(proxy.url("/basic-auth/simple"))
        .arg("--username")
        .arg("private")
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to fetch credentials for private@http://[LOCALHOST]/basic-auth/simple
    ");

    // Login to the index with a token
    uv_snapshot!(context.filters(), context.auth_login()
        .arg(proxy.url("/basic-auth/simple"))
        .arg("--token")
        .arg("heron")
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Stored credentials for http://[LOCALHOST]/basic-auth
    "
    );

    // Retrieve the token without a username
    uv_snapshot!(context.filters(), context.auth_token()
        .arg(proxy.url("/basic-auth/simple"))
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    heron

    ----- stderr -----
    ");

    context
        .auth_logout()
        .arg(proxy.url("/basic-auth/simple"))
        .arg("--username")
        .arg("public")
        .status()?;

    // Retrieve token using URL with embedded username (no --username needed)
    uv_snapshot!(context.filters(), context.auth_token()
        .arg(proxy.username_url("public", "/basic-auth/simple"))
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    heron

    ----- stderr -----
    ");

    // Conflict between --username and URL username is rejected
    uv_snapshot!(context.filters(), context.auth_token()
        .arg(proxy.username_url("public", "/basic-auth/simple"))
        .arg("--username")
        .arg("different")
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Cannot specify a username both via the URL and CLI; found `--username different` and `public`
    ");

    Ok(())
}

#[tokio::test]
#[cfg(feature = "native-auth")]
async fn token_native_auth_realm() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]).with_real_home();
    let proxy = crate::pypi_proxy::start().await;

    // Clear state before the test
    context
        .auth_logout()
        .arg(proxy.uri())
        .arg("--username")
        .arg("public")
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth")
        .status()?;
    context
        .auth_logout()
        .arg(proxy.url("/basic-auth/simple"))
        .arg("--username")
        .arg("public")
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth")
        .status()?;
    context
        .auth_logout()
        .arg(proxy.uri())
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth")
        .status()?;

    // Without persisted credentials
    uv_snapshot!(context.filters(), context.auth_token()
        .arg(proxy.uri())
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to fetch credentials for http://[LOCALHOST]/
    ");

    // Without persisted credentials (with a username in the request)
    uv_snapshot!(context.filters(), context.auth_token()
        .arg(proxy.uri())
        .arg("--username")
        .arg("public")
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to fetch credentials for public@http://[LOCALHOST]/
    ");

    // Login to the index
    uv_snapshot!(context.filters(), context.auth_login()
        .arg(proxy.uri())
        .arg("--username")
        .arg("public")
        .arg("--password")
        .arg("heron")
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Stored credentials for public@http://[LOCALHOST]/
    "
    );

    // Show the credentials
    uv_snapshot!(context.filters(), context.auth_token()
        .arg(proxy.uri())
        .arg("--username")
        .arg("public")
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    heron

    ----- stderr -----
    ");

    // Show the credentials for a child URL
    uv_snapshot!(context.filters(), context.auth_token()
        .arg(proxy.url("/basic-auth/simple"))
        .arg("--username")
        .arg("public")
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    heron

    ----- stderr -----
    ");

    // Without the username (defaults to __token__ which wasn't stored)
    uv_snapshot!(context.filters(), context.auth_token()
        .arg(proxy.uri())
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to fetch credentials for http://[LOCALHOST]/
    ");

    // Without the username (defaults to __token__ which wasn't stored)
    uv_snapshot!(context.filters(), context.auth_token()
        .arg(proxy.url("/basic-auth/simple"))
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to fetch credentials for http://[LOCALHOST]/basic-auth/simple
    ");

    // With a mismatched username
    // TODO(zanieb): Add a hint here if we can?
    uv_snapshot!(context.filters(), context.auth_token()
        .arg(proxy.uri())
        .arg("--username")
        .arg("private")
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to fetch credentials for private@http://[LOCALHOST]/
    ");

    // With a mismatched port
    uv_snapshot!(context.filters(), context.auth_token()
        .arg("https://192.0.2.1:1000")
        .arg("--username")
        .arg("public")
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to fetch credentials for public@https://192.0.2.1:1000/
    ");

    // Login to the index with a token
    uv_snapshot!(context.filters(), context.auth_login()
        .arg(proxy.uri())
        .arg("--token")
        .arg("heron")
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Stored credentials for http://[LOCALHOST]/
    "
    );

    // Retrieve the token without a username
    uv_snapshot!(context.filters(), context.auth_token()
        .arg(proxy.uri())
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    heron

    ----- stderr -----
    ");

    context
        .auth_logout()
        .arg(proxy.uri())
        .arg("--username")
        .arg("public")
        .status()?;

    // Retrieve token using URL with embedded username (no --username needed)
    uv_snapshot!(context.filters(), context.auth_token()
        .arg(proxy.username_url("public", "/basic-auth/simple"))
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    heron

    ----- stderr -----
    ");

    Ok(())
}

#[tokio::test]
#[cfg(feature = "native-auth")]
async fn login_native_auth() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]).with_real_home();
    let proxy = crate::pypi_proxy::start().await;

    // Clear state before the test
    context
        .auth_logout()
        .arg(proxy.url("/basic-auth/simple"))
        .arg("--username")
        .arg("public")
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth")
        .status()?;

    // Without a service name
    uv_snapshot!(context.filters(), context.auth_login(), @r"
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
    uv_snapshot!(context.filters(), context.auth_login()
        .arg(proxy.url("/basic-auth/simple"))
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No username provided; did you mean to provide `--username` or `--token`?
    ");

    // Without a password
    uv_snapshot!(context.filters(), context.auth_login()
        .arg(proxy.url("/basic-auth/simple"))
        .arg("--username")
        .arg("public")
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No password provided; did you mean to provide `--password` or `--token`?
    ");

    // Successful
    uv_snapshot!(context.filters(), context.auth_login()
        .arg(proxy.url("/basic-auth/simple"))
        .arg("--username")
        .arg("public")
        .arg("--password")
        .arg("heron")
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Stored credentials for public@http://[LOCALHOST]/basic-auth
    "
    );

    Ok(())
}

#[tokio::test]
#[cfg(feature = "native-auth")]
async fn login_token_native_auth() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]).with_real_home();
    let proxy = crate::pypi_proxy::start().await;

    // Clear state before the test
    context
        .auth_logout()
        .arg(proxy.url("/basic-auth/simple"))
        .arg("--username")
        .arg("__token__")
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth")
        .status()?;

    // Successful with token
    uv_snapshot!(context.filters(), context.auth_login()
        .arg(proxy.url("/basic-auth/simple"))
        .arg("--token")
        .arg("test-token")
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Stored credentials for http://[LOCALHOST]/basic-auth
    "
    );

    Ok(())
}

#[tokio::test]
#[cfg(feature = "native-auth")]
async fn logout_native_auth() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]).with_real_home();
    let proxy = crate::pypi_proxy::start().await;

    // Clear state before the test
    context
        .auth_logout()
        .arg(proxy.url("/basic-auth/simple"))
        .arg("--username")
        .arg("public")
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth")
        .status()?;
    context
        .auth_logout()
        .arg(proxy.url("/basic-auth/simple"))
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth")
        .status()?;

    // Without a service name
    uv_snapshot!(context.filters(), context.auth_logout(), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: the following required arguments were not provided:
      <SERVICE>

    Usage: uv auth logout --cache-dir [CACHE_DIR] <SERVICE>

    For more information, try '--help'.
    ");

    // Logout before logging in
    uv_snapshot!(context.filters(), context.auth_logout()
        .arg(proxy.url("/basic-auth/simple"))
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Unable to remove credentials for http://[LOCALHOST]/basic-auth
      Caused by: No matching entry found in secure storage
    ");

    // Logout before logging in (with a username)
    uv_snapshot!(context.filters(), context.auth_logout()
        .arg(proxy.url("/basic-auth/simple"))
        .arg("--username")
        .arg("public")
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Unable to remove credentials for public@http://[LOCALHOST]/basic-auth
      Caused by: No matching entry found in secure storage
    ");

    // Login with a username
    uv_snapshot!(context.filters(), context.auth_login()
        .arg(proxy.url("/basic-auth/simple"))
        .arg("--username")
        .arg("public")
        .arg("--password")
        .arg("heron")
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Stored credentials for public@http://[LOCALHOST]/basic-auth
    "
    );

    // Logout without a username
    // TODO(zanieb): Add a hint here if we can?
    uv_snapshot!(context.filters(), context.auth_logout()
        .arg(proxy.url("/basic-auth/simple"))
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Unable to remove credentials for http://[LOCALHOST]/basic-auth
      Caused by: No matching entry found in secure storage
    ");

    // Logout with a username
    uv_snapshot!(context.filters(), context.auth_logout()
        .arg(proxy.url("/basic-auth/simple"))
        .arg("--username")
        .arg("public")
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Removed credentials for public@http://[LOCALHOST]/basic-auth
    ");

    // Login again
    context
        .auth_login()
        .arg(proxy.url("/basic-auth/simple"))
        .arg("--username")
        .arg("public")
        .arg("--password")
        .arg("heron")
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth")
        .assert()
        .success();

    // Logout with a username in the URL
    uv_snapshot!(context.filters(), context.auth_logout()
        .arg(proxy.username_url("public", "/basic-auth/simple"))
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Removed credentials for public@http://[LOCALHOST]/basic-auth
    ");

    // Conflict between --username and a URL username is rejected
    uv_snapshot!(context.filters(), context.auth_logout()
        .arg(proxy.username_url("public", "/basic-auth/simple"))
        .arg("--username")
        .arg("foo")
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Cannot specify a username both via the URL and CLI; found `--username foo` and `public`
    ");

    // Conflict between --token and a URL username is rejected
    uv_snapshot!(context.filters(), context.auth_login()
        .arg(proxy.username_url("public", "/basic-auth/simple"))
        .arg("--token")
        .arg("foo")
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: When using `--token`, a username cannot not be provided; found: public
    ");

    Ok(())
}

#[tokio::test]
#[cfg(feature = "native-auth")]
async fn logout_token_native_auth() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]).with_real_home();
    let proxy = crate::pypi_proxy::start().await;

    // Clear state before the test
    context
        .auth_logout()
        .arg(proxy.url("/basic-auth/simple"))
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth")
        .status()?;

    // Login with a token
    uv_snapshot!(context.filters(), context.auth_login()
        .arg(proxy.url("/basic-auth/simple"))
        .arg("--token")
        .arg("test-token")
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Stored credentials for http://[LOCALHOST]/basic-auth
    "
    );

    // Logout without a username
    uv_snapshot!(context.filters(), context.auth_logout()
        .arg(proxy.url("/basic-auth/simple"))
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Removed credentials for http://[LOCALHOST]/basic-auth
    ");

    Ok(())
}

#[test]
#[cfg(feature = "native-auth")]
fn login_native_auth_url() {
    let context = uv_test::test_context_with_versions!(&[]).with_real_home();

    // A domain-only service name gets https:// prepended
    uv_snapshot!(context.auth_login()
        .arg("example.com")
        .arg("--username")
        .arg("test")
        .arg("--password")
        .arg("test")
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Stored credentials for test@https://example.com/
    ");

    // HTTP URLs are not allowed - only HTTPS
    uv_snapshot!(context.auth_login()
        .arg("http://example.com")
        .arg("--username")
        .arg("test")
        .arg("--password")
        .arg("test")
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: invalid value 'http://example.com' for '<SERVICE>': HTTPS is required for non-local hosts

    For more information, try '--help'.
    ");

    // HTTP URLs are fine for localhost
    uv_snapshot!(context.auth_login()
        .arg("http://localhost:1324")
        .arg("--username")
        .arg("test")
        .arg("--password")
        .arg("test")
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Stored credentials for test@http://localhost:1324/
    ");

    uv_snapshot!(context.auth_login()
        .arg("https://example.com")
        .arg("--username")
        .arg("test")
        .arg("--password")
        .arg("test")
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Stored credentials for test@https://example.com/
    ");

    // A domain-only service with a path also gets https:// prepended
    uv_snapshot!(context.auth_login()
        .arg("example.com/simple")
        .arg("--username")
        .arg("test")
        .arg("--password")
        .arg("test")
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Stored credentials for test@https://example.com/
    ");

    // An invalid URL is rejected
    uv_snapshot!(context.auth_login()
        .arg("not a valid url")
        .arg("--username")
        .arg("test")
        .arg("--password")
        .arg("test")
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: invalid value 'not a valid url' for '<SERVICE>': invalid international domain name

    For more information, try '--help'.
    ");

    // URL with embedded credentials works
    uv_snapshot!(context.auth_login()
        .arg("https://test:password@example.com/simple")
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Stored credentials for test@https://example.com/
    ");

    // URL with embedded username and separate password works
    uv_snapshot!(context.auth_login()
        .arg("https://test@example.com/simple")
        .arg("--password")
        .arg("password")
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Stored credentials for test@https://example.com/
    ");

    // Conflict between --username and URL username is rejected
    uv_snapshot!(context.auth_login()
        .arg("https://test@example.com/simple")
        .arg("--username")
        .arg("different")
        .arg("--password")
        .arg("password")
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Cannot specify a username both via the URL and CLI; found `--username different` and `test`
    ");

    // Conflict between --password and URL password is rejected
    uv_snapshot!(context.auth_login()
        .arg("https://test:password@example.com/simple")
        .arg("--password")
        .arg("different")
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Cannot specify a password both via the URL and CLI
    ");

    // Conflict between --token and URL credentials is rejected
    uv_snapshot!(context.auth_login()
        .arg("https://test:password@example.com/simple")
        .arg("--token")
        .arg("some-token")
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: When using `--token`, a username cannot not be provided; found: test
    ");
}

#[tokio::test]
async fn login_text_store() {
    let context = uv_test::test_context_with_versions!(&[]);
    let proxy = crate::pypi_proxy::start().await;

    // Login with a username and password
    uv_snapshot!(context.filters(), context.auth_login()
        .arg(proxy.url("/basic-auth/simple"))
        .arg("--username")
        .arg("public")
        .arg("--password")
        .arg("heron"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Stored credentials for public@http://[LOCALHOST]/basic-auth
    "
    );

    // Login with a token
    uv_snapshot!(context.filters(), context.auth_login()
        .arg("https://example.com/simple")
        .arg("--token")
        .arg("test-token"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Stored credentials for https://example.com/
    "
    );

    // Empty username should fail
    uv_snapshot!(context.filters(), context.auth_login()
        .arg("https://example.com/simple")
        .arg("--username")
        .arg("")
        .arg("--password")
        .arg("testpass"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Username cannot be empty
    "
    );

    // Empty password should fail
    uv_snapshot!(context.filters(), context.auth_login()
        .arg("https://example.com/simple")
        .arg("--username")
        .arg("testuser")
        .arg("--password")
        .arg(""), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Password cannot be empty
    "
    );

    // HTTP should fail
    uv_snapshot!(context.filters(), context.auth_login()
        .arg("http://example.com/simple")
        .arg("--username")
        .arg("testuser")
        .arg("--password")
        .arg("testpass"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: invalid value 'http://example.com/simple' for '<SERVICE>': HTTPS is required for non-local hosts

    For more information, try '--help'.
    ");

    // Other protocol should fail
    uv_snapshot!(context.filters(), context.auth_login()
        .arg("ftp://example.com/simple")
        .arg("--username")
        .arg("testuser")
        .arg("--password")
        .arg("testpass"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: invalid value 'ftp://example.com/simple' for '<SERVICE>': Unsupported scheme: ftp

    For more information, try '--help'.
    ");

    // HTTP should be allowed on localhost
    uv_snapshot!(context.filters(), context.auth_login()
        .arg("http://127.0.0.1/simple")
        .arg("--username")
        .arg("testuser")
        .arg("--password")
        .arg("testpass"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Stored credentials for testuser@http://127.0.0.1/
    ");

    // HTTP should be allowed on localhost
    uv_snapshot!(context.filters(), context.auth_login()
        .arg("http://localhost/simple")
        .arg("--username")
        .arg("testuser")
        .arg("--password")
        .arg("testpass"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Stored credentials for testuser@http://localhost/
    ");
}

#[test]
#[expect(clippy::disallowed_types)]
fn login_password_stdin() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);

    // Create a temporary file with the password
    let password_file = context.temp_dir.child("password.txt");
    password_file.write_str("secret-password")?;

    // Login with password from stdin
    uv_snapshot!(context.auth_login()
        .arg("https://example.com/simple")
        .arg("--username")
        .arg("testuser")
        .arg("--password")
        .arg("-")
        .stdin(std::fs::File::open(password_file)?), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Stored credentials for testuser@https://example.com/
    "
    );

    // Verify the credentials work by retrieving the token
    uv_snapshot!(context.auth_token()
        .arg("https://example.com/simple")
        .arg("--username")
        .arg("testuser"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    secret-password

    ----- stderr -----
    "
    );

    Ok(())
}

#[test]
#[expect(clippy::disallowed_types)]
fn login_token_stdin() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);

    // Create a temporary file with the token
    let token_file = context.temp_dir.child("token.txt");
    token_file.write_str("secret-token")?;

    // Login with token from stdin
    uv_snapshot!(context.auth_login()
        .arg("https://example.com/simple")
        .arg("--token")
        .arg("-")
        .stdin(std::fs::File::open(token_file)?), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Stored credentials for https://example.com/
    "
    );

    // Verify the credentials work by retrieving the token
    uv_snapshot!(context.auth_token()
        .arg("https://example.com/simple"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    secret-token

    ----- stderr -----
    "
    );

    Ok(())
}

#[tokio::test]
async fn token_text_store() {
    let context = uv_test::test_context_with_versions!(&[]);
    let proxy = crate::pypi_proxy::start().await;

    // Login first
    context
        .auth_login()
        .arg(proxy.url("/basic-auth/simple"))
        .arg("--username")
        .arg("public")
        .arg("--password")
        .arg("heron")
        .assert()
        .success();

    // Retrieve the token
    uv_snapshot!(context.filters(), context.auth_token()
        .arg(proxy.url("/basic-auth/simple"))
        .arg("--username")
        .arg("public"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    heron

    ----- stderr -----
    "
    );

    // Login with token
    context
        .auth_login()
        .arg("https://example.com/simple")
        .arg("--token")
        .arg("test-token")
        .assert()
        .success();

    // Retrieve token without username
    uv_snapshot!(context.filters(), context.auth_token()
        .arg("https://example.com/simple"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    test-token

    ----- stderr -----
    "
    );

    // Empty username should fail
    uv_snapshot!(context.filters(), context.auth_token()
        .arg("https://example.com/simple")
        .arg("--username")
        .arg(""), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Username cannot be empty
    "
    );
}

#[tokio::test]
async fn logout_text_store() {
    let context = uv_test::test_context_with_versions!(&[]);
    let proxy = crate::pypi_proxy::start().await;

    // Login first
    context
        .auth_login()
        .arg(proxy.url("/basic-auth/simple"))
        .arg("--username")
        .arg("public")
        .arg("--password")
        .arg("heron")
        .assert()
        .success();

    // Logout
    uv_snapshot!(context.filters(), context.auth_logout()
        .arg(proxy.url("/basic-auth/simple"))
        .arg("--username")
        .arg("public"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Removed credentials for public@http://[LOCALHOST]/basic-auth
    "
    );

    // Login with token then logout
    context
        .auth_login()
        .arg("https://example.com/simple")
        .arg("--token")
        .arg("test-token")
        .assert()
        .success();

    uv_snapshot!(context.filters(), context.auth_logout()
        .arg("https://example.com/simple"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Removed credentials for https://example.com/
    "
    );

    // Empty username should fail
    uv_snapshot!(context.filters(), context.auth_logout()
        .arg("https://example.com/simple")
        .arg("--username")
        .arg(""), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Username cannot be empty
    "
    );
}

#[tokio::test]
async fn auth_disabled_provider_uses_text_store() {
    let context = uv_test::test_context_with_versions!(&[]);
    let proxy = crate::pypi_proxy::start().await;

    // Login with disabled provider should use text store
    uv_snapshot!(context.filters(), context.auth_login()
        .arg(proxy.url("/basic-auth/simple"))
        .arg("--username")
        .arg("public")
        .arg("--password")
        .arg("heron")
        .arg("--keyring-provider")
        .arg("disabled"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Stored credentials for public@http://[LOCALHOST]/basic-auth
    "
    );

    // Token retrieval should work with disabled provider
    uv_snapshot!(context.filters(), context.auth_token()
        .arg(proxy.url("/basic-auth/simple"))
        .arg("--username")
        .arg("public")
        .arg("--keyring-provider")
        .arg("disabled"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    heron

    ----- stderr -----
    "
    );
}

#[test]
fn login_text_store_strips_simple_suffix() {
    let context = uv_test::test_context_with_versions!(&[]);

    // Login with `/simple` suffix - should strip it and store credentials for the root URL
    uv_snapshot!(context.auth_login()
        .arg("https://example.com/simple")
        .arg("--username")
        .arg("testuser")
        .arg("--password")
        .arg("testpass"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Stored credentials for testuser@https://example.com/
    "
    );

    // Login with `/+simple` suffix (devpi format) - should also strip it
    uv_snapshot!(context.auth_login()
        .arg("https://devpi.example.com/root/+simple")
        .arg("--username")
        .arg("devpiuser")
        .arg("--password")
        .arg("devpipass"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Stored credentials for devpiuser@https://devpi.example.com/root
    "
    );

    // Login with `/Simple` (case insensitive) - should strip it
    uv_snapshot!(context.auth_login()
        .arg("https://registry.example.com/Simple")
        .arg("--username")
        .arg("caseuser")
        .arg("--password")
        .arg("casepass"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Stored credentials for caseuser@https://registry.example.com/
    "
    );

    // Login without `/simple` suffix - should store as-is
    uv_snapshot!(context.auth_login()
        .arg("https://custom.example.com/api/v1")
        .arg("--username")
        .arg("apiuser")
        .arg("--password")
        .arg("apipass"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Stored credentials for apiuser@https://custom.example.com/api/v1
    "
    );

    // Login with trailing slash and `/simple` - should strip both
    uv_snapshot!(context.auth_login()
        .arg("https://trailing.example.com/simple/")
        .arg("--username")
        .arg("slashuser")
        .arg("--password")
        .arg("slashpass"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Stored credentials for slashuser@https://trailing.example.com/
    "
    );
}

#[test]
fn logout_text_store_strips_simple_suffix() {
    let context = uv_test::test_context_with_versions!(&[]);

    // Login with `/simple` suffix first
    context
        .auth_login()
        .arg("https://example.com/simple")
        .arg("--username")
        .arg("testuser")
        .arg("--password")
        .arg("testpass")
        .assert()
        .success();

    // Logout using the same URL with `/simple` - should work
    uv_snapshot!(context.auth_logout()
        .arg("https://example.com/simple")
        .arg("--username")
        .arg("testuser"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Removed credentials for testuser@https://example.com/
    "
    );

    // Login with `/+simple` suffix
    context
        .auth_login()
        .arg("https://devpi.example.com/root/+simple")
        .arg("--username")
        .arg("devpiuser")
        .arg("--password")
        .arg("devpipass")
        .assert()
        .success();

    // Logout using URL with `/+simple` - should work
    uv_snapshot!(context.auth_logout()
        .arg("https://devpi.example.com/root/+simple")
        .arg("--username")
        .arg("devpiuser"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Removed credentials for devpiuser@https://devpi.example.com/root
    "
    );
}

#[test]
fn token_text_store_strips_simple_suffix() {
    let context = uv_test::test_context_with_versions!(&[]);

    // Login with `/simple` suffix
    context
        .auth_login()
        .arg("https://example.com/simple")
        .arg("--username")
        .arg("testuser")
        .arg("--password")
        .arg("testpass")
        .assert()
        .success();

    // Retrieve token using URL with `/simple` - should work
    uv_snapshot!(context.auth_token()
        .arg("https://example.com/simple")
        .arg("--username")
        .arg("testuser"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    testpass

    ----- stderr -----
    "
    );

    // Login with token and `/simple` suffix
    context
        .auth_login()
        .arg("https://token.example.com/simple")
        .arg("--token")
        .arg("secret-token")
        .assert()
        .success();

    // Retrieve token using URL with `/simple` - should work
    uv_snapshot!(context.auth_token()
        .arg("https://token.example.com/simple"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    secret-token

    ----- stderr -----
    "
    );
}

#[test]
fn token_text_store_username() {
    let context = uv_test::test_context_with_versions!(&[]);

    // Login with specific username
    context
        .auth_login()
        .arg("https://example.com/simple")
        .arg("--username")
        .arg("testuser")
        .arg("--password")
        .arg("testpass")
        .assert()
        .success();

    // Retrieve token with matching username
    uv_snapshot!(context.auth_token()
        .arg("https://example.com/simple")
        .arg("--username")
        .arg("testuser"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    testpass

    ----- stderr -----
    "
    );

    // Retrieve token without username
    uv_snapshot!(context.auth_token()
        .arg("https://example.com/simple"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to fetch credentials for https://example.com/simple
    "
    );

    // Try to retrieve token with different username - should fail
    uv_snapshot!(context.auth_token()
        .arg("https://example.com/simple")
        .arg("--username")
        .arg("wronguser"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to fetch credentials for wronguser@https://example.com/simple
    "
    );

    // Login with token (no username)
    context
        .auth_login()
        .arg("https://token.example.com/simple")
        .arg("--token")
        .arg("test-token")
        .assert()
        .success();

    // Retrieve token without specifying username - should work
    uv_snapshot!(context.auth_token()
        .arg("https://token.example.com/simple"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    test-token

    ----- stderr -----
    "
    );

    // Login with username at different URL
    context
        .auth_login()
        .arg("https://userexample.com/simple")
        .arg("--username")
        .arg("testuser")
        .arg("--password")
        .arg("testpass")
        .assert()
        .success();

    // Retrieve token without username should fail
    uv_snapshot!(context.auth_token()
        .arg("https://userexample.com/simple"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to fetch credentials for https://userexample.com/simple
    "
    );
}

#[test]
fn logout_text_store_multiple_usernames() {
    let context = uv_test::test_context_with_versions!(&[]);

    // Login with two different usernames for the same service
    context
        .auth_login()
        .arg("https://example.com/simple")
        .arg("--username")
        .arg("user1")
        .arg("--password")
        .arg("pass1")
        .assert()
        .success();

    context
        .auth_login()
        .arg("https://example.com/simple")
        .arg("--username")
        .arg("user2")
        .arg("--password")
        .arg("pass2")
        .assert()
        .success();

    // Logout one specific username
    uv_snapshot!(context.auth_logout()
        .arg("https://example.com/simple")
        .arg("--username")
        .arg("user1"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Removed credentials for user1@https://example.com/
    "
    );

    // Verify the first user is gone but second remains
    uv_snapshot!(context.auth_token()
        .arg("https://example.com/simple")
        .arg("--username")
        .arg("user1"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to fetch credentials for user1@https://example.com/simple
    "
    );

    uv_snapshot!(context.auth_token()
        .arg("https://example.com/simple")
        .arg("--username")
        .arg("user2"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    pass2

    ----- stderr -----
    "
    );

    // Try to logout without specifying username (defaults to `__token__`)
    uv_snapshot!(context.auth_logout()
        .arg("https://example.com/simple"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No matching entry found for https://example.com/
    "
    );
}

#[test]
#[cfg(feature = "native-auth")]
fn native_auth_prefix_match() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]).with_real_home();

    // Clear state before the test
    context
        .auth_logout()
        .arg("https://example.com/api")
        .arg("--username")
        .arg("testuser")
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth")
        .status()?;

    // Login with credentials for a path
    uv_snapshot!(context.auth_login()
        .arg("https://example.com/api")
        .arg("--username")
        .arg("testuser")
        .arg("--password")
        .arg("testpass")
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Stored credentials for testuser@https://example.com/api
    "
    );

    // A request for a child path does not match, the native store does not yet implement prefix
    // matching
    uv_snapshot!(context.auth_token()
        .arg("https://example.com/api/v1")
        .arg("--username")
        .arg("testuser")
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    testpass

    ----- stderr -----
    "
    );

    Ok(())
}

#[test]
#[cfg(feature = "native-auth")]
fn native_auth_host_fallback() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]).with_real_home();

    // Clear state before the test
    context
        .auth_logout()
        .arg("example.com")
        .arg("--username")
        .arg("testuser")
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth")
        .status()?;

    // Login with credentials for the host
    uv_snapshot!(context.auth_login()
        .arg("example.com")
        .arg("--username")
        .arg("testuser")
        .arg("--password")
        .arg("hostpass")
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Stored credentials for testuser@https://example.com/
    "
    );

    // Should fallback to host-level matching
    uv_snapshot!(context.auth_token()
        .arg("https://example.com/any/path")
        .arg("--username")
        .arg("testuser")
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    hostpass

    ----- stderr -----
    "
    );

    // A request to another host should not work
    uv_snapshot!(context.auth_token()
        .arg("https://another-example.com/any/path")
        .arg("--username")
        .arg("testuser")
        .env(EnvVars::UV_PREVIEW_FEATURES, "native-auth"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to fetch credentials for testuser@https://another-example.com/any/path
    "
    );

    Ok(())
}

/// Test credential helper with basic auth credentials
#[test]
fn bazel_helper_basic_auth() {
    let context = uv_test::test_context!("3.12");

    // Store credentials
    uv_snapshot!(context.filters(), context.auth_login()
        .arg("https://test.example.com")
        .arg("--username").arg("testuser")
        .arg("--password").arg("testpass"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Stored credentials for testuser@https://test.example.com/
    ");

    uv_snapshot!(context.filters(), context.auth_helper()
        .arg("--protocol=bazel")
        .arg("get"),
        input=r#"{"uri":"https://test.example.com/path"}"#,
        @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {"headers":{"Authorization":["Basic dGVzdHVzZXI6dGVzdHBhc3M="]}}

    ----- stderr -----
    warning: The `uv auth helper` command is experimental and may change without warning. Pass `--preview-features auth-helper` to disable this warning
    "#
    );
}

/// Test credential helper with token credentials
#[test]
fn bazel_helper_token() {
    let context = uv_test::test_context!("3.12");

    // Store token
    uv_snapshot!(context.filters(), context.auth_login()
        .arg("https://api.example.com")
        .arg("--token").arg("mytoken123"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Stored credentials for https://api.example.com/
    ");

    // Test credential helper - tokens are stored as Basic auth with __token__ username
    uv_snapshot!(context.filters(), context.auth_helper()
        .arg("--protocol=bazel")
        .arg("get"),
        input=r#"{"uri":"https://api.example.com/v1/endpoint"}"#,
        @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {"headers":{"Authorization":["Basic X190b2tlbl9fOm15dG9rZW4xMjM="]}}

    ----- stderr -----
    warning: The `uv auth helper` command is experimental and may change without warning. Pass `--preview-features auth-helper` to disable this warning
    "#
    );
}

/// Test credential helper with no credentials found
#[test]
fn bazel_helper_no_credentials() {
    let context = uv_test::test_context!("3.12");
    uv_snapshot!(context.filters(), context.auth_helper()
        .arg("--protocol=bazel")
        .arg("get"),
        input=r#"{"uri":"https://unknown.example.com/path"}"#,
        @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {"headers":{}}

    ----- stderr -----
    warning: The `uv auth helper` command is experimental and may change without warning. Pass `--preview-features auth-helper` to disable this warning
    "#
    );
}

/// Test credential helper with invalid JSON input
#[test]
fn bazel_helper_invalid_json() {
    let context = uv_test::test_context!("3.12");

    uv_snapshot!(context.filters(), context.auth_helper()
        .arg("--protocol=bazel")
        .arg("get"),
        input="not json",
        @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: The `uv auth helper` command is experimental and may change without warning. Pass `--preview-features auth-helper` to disable this warning
    error: Failed to parse credential request as JSON
      Caused by: expected ident at line 1 column 2
    "
    );
}

/// Test credential helper with invalid URI
#[test]
fn bazel_helper_invalid_uri() {
    let context = uv_test::test_context!("3.12");

    uv_snapshot!(context.filters(), context.auth_helper()
        .arg("--protocol=bazel")
        .arg("get"),
        input=r#"{"uri":"not a url"}"#,
        @r#"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: The `uv auth helper` command is experimental and may change without warning. Pass `--preview-features auth-helper` to disable this warning
    error: Failed to parse credential request as JSON
      Caused by: relative URL without a base: "not a url" at line 1 column 18
    "#
    );
}

/// Test credential helper with username in URI
#[test]
fn bazel_helper_username_in_uri() {
    let context = uv_test::test_context!("3.12");

    // Store credentials with specific username
    uv_snapshot!(context.filters(), context.auth_login()
        .arg("https://test.example.com")
        .arg("--username").arg("specificuser")
        .arg("--password").arg("specificpass"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Stored credentials for specificuser@https://test.example.com/
    ");

    // Test with username in URI
    uv_snapshot!(context.filters(), context.auth_helper()
        .arg("--protocol=bazel")
        .arg("get"),
        input=r#"{"uri":"https://specificuser@test.example.com/path"}"#,
        @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {"headers":{"Authorization":["Basic c3BlY2lmaWN1c2VyOnNwZWNpZmljcGFzcw=="]}}

    ----- stderr -----
    warning: The `uv auth helper` command is experimental and may change without warning. Pass `--preview-features auth-helper` to disable this warning
    "#
    );
}

/// Test credential helper with unknown username in URI
#[test]
fn bazel_helper_unknown_username_in_uri() {
    let context = uv_test::test_context!("3.12");

    // Store credentials with specific username
    uv_snapshot!(context.filters(), context.auth_login()
        .arg("https://test.example.com")
        .arg("--username").arg("specificuser")
        .arg("--password").arg("specificpass"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Stored credentials for specificuser@https://test.example.com/
    ");

    // Test with username in URI
    uv_snapshot!(context.filters(), context.auth_helper()
        .arg("--protocol=bazel")
        .arg("get"),
        input=r#"{"uri":"https://differentuser@test.example.com/path"}"#,
        @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {"headers":{}}

    ----- stderr -----
    warning: The `uv auth helper` command is experimental and may change without warning. Pass `--preview-features auth-helper` to disable this warning
    "#
    );
}

/// Test that `pyx.dev` is recognized as a pyx domain even when `PYX_API_URL` points elsewhere.
///
/// When `PYX_API_URL` is set to a different URL (e.g., localhost for development),
/// `pyx.dev` should still be recognized as a pyx domain and use the OAuth flow.
#[test]
fn login_pyx_dev_with_custom_api_url() {
    let context = uv_test::test_context_with_versions!(&[]);

    // When PYX_API_URL is set to localhost, `pyx.dev` should still be recognized as pyx
    // and reject username/password (because pyx uses OAuth, not basic auth).
    uv_snapshot!(context.auth_login()
        .arg("pyx.dev")
        .arg("--username")
        .arg("testuser")
        .arg("--password")
        .arg("testpass")
        .env(EnvVars::PYX_API_URL, "http://localhost:8000"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Cannot specify a username when logging in to pyx
    "
    );

    // Same for api.pyx.dev
    uv_snapshot!(context.auth_login()
        .arg("api.pyx.dev")
        .arg("--username")
        .arg("testuser")
        .arg("--password")
        .arg("testpass")
        .env(EnvVars::PYX_API_URL, "http://localhost:8000"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Cannot specify a username when logging in to pyx
    "
    );

    // Other subdomains like beta.pyx.dev are NOT recognized as default pyx domains.
    // They fall through to normal credential handling when PYX_API_URL doesn't match.
    uv_snapshot!(context.auth_login()
        .arg("beta.pyx.dev")
        .arg("--username")
        .arg("testuser")
        .arg("--password")
        .arg("testpass")
        .env(EnvVars::PYX_API_URL, "http://localhost:8000"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Stored credentials for testuser@https://beta.pyx.dev/
    "
    );
}

/// Test that logout recognizes `pyx.dev` even with custom `PYX_API_URL`.
#[test]
fn logout_pyx_dev_with_custom_api_url() {
    let context = uv_test::test_context_with_versions!(&[]);

    // Logout for pyx.dev should use the pyx flow (succeeds with no-op message because
    // no credentials exist, but verifies it's recognized as pyx).
    uv_snapshot!(context.auth_logout()
        .arg("pyx.dev")
        .env(EnvVars::PYX_API_URL, "http://localhost:8000"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    No credentials found for http://localhost:8000/
    "
    );
}

/// Test that `uv auth token` recognizes `pyx.dev` even with custom `PYX_API_URL`.
#[test]
fn token_pyx_dev_with_custom_api_url() {
    let context = uv_test::test_context_with_versions!(&[]);

    // Token for pyx.dev should use the pyx flow and reject username.
    uv_snapshot!(context.auth_token()
        .arg("pyx.dev")
        .arg("--username")
        .arg("testuser")
        .env(EnvVars::PYX_API_URL, "http://localhost:8000"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Cannot specify a username when logging in to pyx
    "
    );
}

/// Test behavior when using a pyx.dev subdomain (like staging) without setting `PYX_API_URL`.
///
/// This verifies that subdomains like `astral-sh-staging-api.pyx.dev` are NOT automatically
/// treated as pyx domains - users must set `PYX_API_URL` to use non-default pyx environments.
#[test]
fn token_pyx_staging_without_env_var() {
    let context = uv_test::test_context_with_versions!(&[]);

    // Without PYX_API_URL set, staging pyx URLs are NOT recognized as pyx domains.
    // They fall through to the normal credential store lookup.
    uv_snapshot!(context.auth_token()
        .arg("https://astral-sh-staging-api.pyx.dev"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to fetch credentials for https://astral-sh-staging-api.pyx.dev/
    "
    );
}

/// Test that staging pyx URLs work correctly when `PYX_API_URL` is set.
#[test]
fn login_pyx_staging_with_env_var() {
    let context = uv_test::test_context_with_versions!(&[]);

    // When PYX_API_URL is set to a staging URL, that URL is recognized as pyx
    // and rejects username/password.
    uv_snapshot!(context.auth_login()
        .arg("https://astral-sh-staging-api.pyx.dev")
        .arg("--username")
        .arg("testuser")
        .arg("--password")
        .arg("testpass")
        .env(EnvVars::PYX_API_URL, "https://astral-sh-staging-api.pyx.dev"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Cannot specify a username when logging in to pyx
    "
    );

    // When PYX_API_URL is set to staging, `pyx.dev` is still recognized as pyx
    // (via is_default_pyx_domain) and uses the OAuth flow.
    uv_snapshot!(context.auth_login()
        .arg("pyx.dev")
        .arg("--username")
        .arg("testuser")
        .arg("--password")
        .arg("testpass")
        .env(EnvVars::PYX_API_URL, "https://astral-sh-staging-api.pyx.dev"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Cannot specify a username when logging in to pyx
    "
    );
}
