#[cfg(feature = "keyring-tests")]
use anyhow::Result;
use assert_cmd::assert::OutputAssertExt;
#[cfg(feature = "keyring-tests")]
use assert_fs::{fixture::PathChild, prelude::FileWriteStr};
use uv_static::EnvVars;

use crate::common::venv_bin_path;
use crate::common::{TestContext, uv_snapshot};

#[test]
#[cfg(feature = "keyring-tests")]
fn add_package_native_keyring() -> Result<()> {
    let context = TestContext::new("3.12").with_real_home();

    // Clear state before the test
    context
        .auth_logout()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--username")
        .arg("public")
        .arg("--keyring-provider")
        .arg("native")
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
    warning: The native keyring provider is experimental and may change without warning. Pass `--preview-features native-keyring` to disable this warning.
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
    warning: The native keyring provider is experimental and may change without warning. Pass `--preview-features native-keyring` to disable this warning.
    Stored credentials for public@https://pypi-proxy.fly.dev/basic-auth
    "
    );

    // Try to add the original package without credentials again. This should use
    // credentials storied in the system keyring.
    uv_snapshot!(context.add().arg("anyio").arg("--default-index").arg("https://public@pypi-proxy.fly.dev/basic-auth/simple"), @r"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    warning: The native keyring provider is experimental and may change without warning. Pass `--preview-features native-keyring` to disable this warning.
      × No solution found when resolving dependencies:
      ╰─▶ Because anyio was not found in the package registry and your project depends on anyio, we can conclude that your project's requirements are unsatisfiable.

          hint: An index URL (https://pypi-proxy.fly.dev/basic-auth/simple) could not be queried due to a lack of valid authentication credentials (401 Unauthorized).
      help: If you want to add the package regardless of the failed resolution, provide the `--frozen` flag to skip locking and syncing.
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
    warning: The native keyring provider is experimental and may change without warning. Pass `--preview-features native-keyring` to disable this warning.
    Removed credentials for public@https://pypi-proxy.fly.dev/basic-auth
    "
    );

    // Authentication should fail again
    uv_snapshot!(context.add().arg("iniconfig").arg("--default-index").arg("https://public@pypi-proxy.fly.dev/basic-auth/simple"), @r"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    warning: The native keyring provider is experimental and may change without warning. Pass `--preview-features native-keyring` to disable this warning.
      × No solution found when resolving dependencies:
      ╰─▶ Because iniconfig was not found in the package registry and your project depends on iniconfig, we can conclude that your project's requirements are unsatisfiable.

          hint: An index URL (https://pypi-proxy.fly.dev/basic-auth/simple) could not be queried due to a lack of valid authentication credentials (401 Unauthorized).
      help: If you want to add the package regardless of the failed resolution, provide the `--frozen` flag to skip locking and syncing.
    "
    );

    Ok(())
}

#[test]
#[cfg(feature = "keyring-tests")]
fn token_native_keyring() -> Result<()> {
    let context = TestContext::new_with_versions(&[]).with_real_home();

    // Clear state before the test
    context
        .auth_logout()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--username")
        .arg("public")
        .arg("--keyring-provider")
        .arg("native")
        .status()?;

    // Without persisted credentials
    uv_snapshot!(context.auth_token()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--keyring-provider")
        .arg("native"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: The native keyring provider is experimental and may change without warning. Pass `--preview-features native-keyring` to disable this warning.
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
    warning: The native keyring provider is experimental and may change without warning. Pass `--preview-features native-keyring` to disable this warning.
    error: Failed to fetch credentials for public@https://pypi-proxy.fly.dev/basic-auth/simple
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
    warning: The native keyring provider is experimental and may change without warning. Pass `--preview-features native-keyring` to disable this warning.
    Stored credentials for public@https://pypi-proxy.fly.dev/basic-auth
    "
    );

    // Show the credentials
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
    warning: The native keyring provider is experimental and may change without warning. Pass `--preview-features native-keyring` to disable this warning.
    error: Failed to fetch credentials for public@https://pypi-proxy.fly.dev/basic-auth/simple
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
    warning: The native keyring provider is experimental and may change without warning. Pass `--preview-features native-keyring` to disable this warning.
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
    warning: The native keyring provider is experimental and may change without warning. Pass `--preview-features native-keyring` to disable this warning.
    error: Failed to fetch credentials for private@https://pypi-proxy.fly.dev/basic-auth/simple
    ");

    // Login to the index with a token
    uv_snapshot!(context.auth_login()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--token")
        .arg("heron")
        .arg("--keyring-provider")
        .arg("native"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: The native keyring provider is experimental and may change without warning. Pass `--preview-features native-keyring` to disable this warning.
    Stored credentials for https://pypi-proxy.fly.dev/basic-auth
    "
    );

    // Retrieve the token without a username
    uv_snapshot!(context.auth_token()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--keyring-provider")
        .arg("native"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: The native keyring provider is experimental and may change without warning. Pass `--preview-features native-keyring` to disable this warning.
    error: Failed to fetch credentials for https://pypi-proxy.fly.dev/basic-auth/simple
    ");

    context
        .auth_logout()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--username")
        .arg("public")
        .status()?;

    // Retrieve token using URL with embedded username (no --username needed)
    uv_snapshot!(context.auth_token()
        .arg("https://public@pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--keyring-provider")
        .arg("native"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: The native keyring provider is experimental and may change without warning. Pass `--preview-features native-keyring` to disable this warning.
    error: Failed to fetch credentials for public@https://pypi-proxy.fly.dev/basic-auth/simple
    ");

    // Conflict between --username and URL username is rejected
    uv_snapshot!(context.auth_token()
        .arg("https://public@pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--username")
        .arg("different")
        .arg("--keyring-provider")
        .arg("native"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: The native keyring provider is experimental and may change without warning. Pass `--preview-features native-keyring` to disable this warning.
    error: Cannot specify a username both via the URL and CLI; found `--username different` and `public`
    ");

    Ok(())
}

#[test]
fn token_subprocess_keyring() {
    let context = TestContext::new("3.12");

    // Without a keyring on the PATH
    uv_snapshot!(context.auth_token()
        .arg("https://public@pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--keyring-provider")
        .arg("subprocess"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to fetch credentials for public@https://pypi-proxy.fly.dev/basic-auth/simple
    "
    );

    // Install our keyring plugin
    context
        .pip_install()
        .arg(
            context
                .workspace_root
                .join("scripts")
                .join("packages")
                .join("keyring_test_plugin"),
        )
        .assert()
        .success();

    // Without credentials available
    uv_snapshot!(context.auth_token()
        .arg("https://public@pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--keyring-provider")
        .arg("subprocess")
        .env(EnvVars::PATH, venv_bin_path(&context.venv)), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to fetch credentials for public@https://pypi-proxy.fly.dev/basic-auth/simple
    "
    );

    // Without a username
    // TODO(zanieb): Add a hint here if we can?
    uv_snapshot!(context.auth_token()
        .arg("https://public@pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--keyring-provider")
        .arg("subprocess")
        .env(EnvVars::KEYRING_TEST_CREDENTIALS, r#"{"pypi-proxy.fly.dev": {"public": "heron"}}"#)
        .env(EnvVars::PATH, venv_bin_path(&context.venv)), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to fetch credentials for public@https://pypi-proxy.fly.dev/basic-auth/simple
    "
    );

    // With the correct username
    uv_snapshot!(context.auth_token()
        .arg("https://public@pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--keyring-provider")
        .arg("subprocess")
        .arg("--username")
        .arg("public")
        .env(EnvVars::KEYRING_TEST_CREDENTIALS, r#"{"pypi-proxy.fly.dev": {"public": "heron"}}"#)
        .env(EnvVars::PATH, venv_bin_path(&context.venv)), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Cannot specify a username both via the URL and CLI; found `--username public` and `public`
    "
    );
}

#[test]
#[cfg(feature = "keyring-tests")]
fn login_native_keyring() -> Result<()> {
    let context = TestContext::new_with_versions(&[]).with_real_home();

    // Clear state before the test
    context
        .auth_logout()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--username")
        .arg("public")
        .arg("--keyring-provider")
        .arg("native")
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
    warning: The native keyring provider is experimental and may change without warning. Pass `--preview-features native-keyring` to disable this warning.
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
    warning: The native keyring provider is experimental and may change without warning. Pass `--preview-features native-keyring` to disable this warning.
    error: No password provided; did you mean to provide `--password` or `--token`?
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
    warning: The native keyring provider is experimental and may change without warning. Pass `--preview-features native-keyring` to disable this warning.
    Stored credentials for public@https://pypi-proxy.fly.dev/basic-auth
    "
    );

    Ok(())
}

#[test]
#[cfg(feature = "keyring-tests")]
fn login_token_native_keyring() -> Result<()> {
    let context = TestContext::new_with_versions(&[]).with_real_home();

    // Clear state before the test
    context
        .auth_logout()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--username")
        .arg("__token__")
        .arg("--keyring-provider")
        .arg("native")
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
    warning: The native keyring provider is experimental and may change without warning. Pass `--preview-features native-keyring` to disable this warning.
    Stored credentials for https://pypi-proxy.fly.dev/basic-auth
    "
    );

    Ok(())
}

#[test]
#[cfg(feature = "keyring-tests")]
fn logout_native_keyring() -> Result<()> {
    let context = TestContext::new_with_versions(&[]).with_real_home();

    // Clear state before the test
    context
        .auth_logout()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--username")
        .arg("public")
        .arg("--keyring-provider")
        .arg("native")
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

    // Logout before logging in
    uv_snapshot!(context.auth_logout()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--keyring-provider")
        .arg("native"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: The native keyring provider is experimental and may change without warning. Pass `--preview-features native-keyring` to disable this warning.
    Removed credentials for https://pypi-proxy.fly.dev/basic-auth
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
    warning: The native keyring provider is experimental and may change without warning. Pass `--preview-features native-keyring` to disable this warning.
    error: Unable to remove credentials for public@https://pypi-proxy.fly.dev/basic-auth
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
    warning: The native keyring provider is experimental and may change without warning. Pass `--preview-features native-keyring` to disable this warning.
    Stored credentials for public@https://pypi-proxy.fly.dev/basic-auth
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
    warning: The native keyring provider is experimental and may change without warning. Pass `--preview-features native-keyring` to disable this warning.
    error: Unable to remove credentials for https://pypi-proxy.fly.dev/basic-auth
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
    warning: The native keyring provider is experimental and may change without warning. Pass `--preview-features native-keyring` to disable this warning.
    Removed credentials for public@https://pypi-proxy.fly.dev/basic-auth
    ");

    // Login again
    context
        .auth_login()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--username")
        .arg("public")
        .arg("--password")
        .arg("heron")
        .arg("--keyring-provider")
        .arg("native")
        .assert()
        .success();

    // Logout with a username in the URL
    uv_snapshot!(context.auth_logout()
        .arg("https://public@pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--keyring-provider")
        .arg("native"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: The native keyring provider is experimental and may change without warning. Pass `--preview-features native-keyring` to disable this warning.
    Removed credentials for public@https://pypi-proxy.fly.dev/basic-auth
    ");

    // Conflict between --username and a URL username is rejected
    uv_snapshot!(context.auth_logout()
        .arg("https://public@pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--username")
        .arg("foo")
        .arg("--keyring-provider")
        .arg("native"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: The native keyring provider is experimental and may change without warning. Pass `--preview-features native-keyring` to disable this warning.
    error: Cannot specify a username both via the URL and CLI; found `--username foo` and `public`
    ");

    // Conflict between --token and a URL username is rejected
    uv_snapshot!(context.auth_login()
        .arg("https://public@pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--token")
        .arg("foo")
        .arg("--keyring-provider")
        .arg("native"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: The native keyring provider is experimental and may change without warning. Pass `--preview-features native-keyring` to disable this warning.
    error: When using `--token`, a username cannot not be provided; found: public
    ");

    Ok(())
}

#[test]
#[cfg(feature = "keyring-tests")]
fn logout_token_native_keyring() -> Result<()> {
    let context = TestContext::new_with_versions(&[]).with_real_home();

    // Clear state before the test
    context
        .auth_logout()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--keyring-provider")
        .arg("native")
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
    warning: The native keyring provider is experimental and may change without warning. Pass `--preview-features native-keyring` to disable this warning.
    Stored credentials for https://pypi-proxy.fly.dev/basic-auth
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
    warning: The native keyring provider is experimental and may change without warning. Pass `--preview-features native-keyring` to disable this warning.
    Removed credentials for https://pypi-proxy.fly.dev/basic-auth
    ");

    Ok(())
}

#[test]
#[cfg(feature = "keyring-tests")]
fn login_native_keyring_url() {
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
    warning: The native keyring provider is experimental and may change without warning. Pass `--preview-features native-keyring` to disable this warning.
    Stored credentials for test@https://example.com/
    ");

    // HTTP URLs are not allowed - only HTTPS
    uv_snapshot!(context.auth_login()
        .arg("http://example.com")
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
    error: invalid value 'http://example.com' for '<SERVICE>': only HTTPS is supported

    For more information, try '--help'.
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
    warning: The native keyring provider is experimental and may change without warning. Pass `--preview-features native-keyring` to disable this warning.
    Stored credentials for test@https://example.com/
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
    warning: The native keyring provider is experimental and may change without warning. Pass `--preview-features native-keyring` to disable this warning.
    Stored credentials for test@https://example.com/
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
    error: invalid value 'not a valid url' for '<SERVICE>': failed to parse URL: invalid international domain name

    For more information, try '--help'.
    ");

    // URL with embedded credentials works
    uv_snapshot!(context.auth_login()
        .arg("https://test:password@example.com/simple")
        .arg("--keyring-provider")
        .arg("native"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: The native keyring provider is experimental and may change without warning. Pass `--preview-features native-keyring` to disable this warning.
    Stored credentials for test@https://example.com/
    ");

    // URL with embedded username and separate password works
    uv_snapshot!(context.auth_login()
        .arg("https://test@example.com/simple")
        .arg("--password")
        .arg("password")
        .arg("--keyring-provider")
        .arg("native"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: The native keyring provider is experimental and may change without warning. Pass `--preview-features native-keyring` to disable this warning.
    Stored credentials for test@https://example.com/
    ");

    // Conflict between --username and URL username is rejected
    uv_snapshot!(context.auth_login()
        .arg("https://test@example.com/simple")
        .arg("--username")
        .arg("different")
        .arg("--password")
        .arg("password")
        .arg("--keyring-provider")
        .arg("native"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: The native keyring provider is experimental and may change without warning. Pass `--preview-features native-keyring` to disable this warning.
    error: Cannot specify a username both via the URL and CLI; found `--username different` and `test`
    ");

    // Conflict between --password and URL password is rejected
    uv_snapshot!(context.auth_login()
        .arg("https://test:password@example.com/simple")
        .arg("--password")
        .arg("different")
        .arg("--keyring-provider")
        .arg("native"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: The native keyring provider is experimental and may change without warning. Pass `--preview-features native-keyring` to disable this warning.
    error: Cannot specify a password both via the URL and CLI
    ");

    // Conflict between --token and URL credentials is rejected
    uv_snapshot!(context.auth_login()
        .arg("https://test:password@example.com/simple")
        .arg("--token")
        .arg("some-token")
        .arg("--keyring-provider")
        .arg("native"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: The native keyring provider is experimental and may change without warning. Pass `--preview-features native-keyring` to disable this warning.
    error: When using `--token`, a username cannot not be provided; found: test
    ");
}

#[test]
fn login_text_store() {
    let context = TestContext::new_with_versions(&[]);

    // Successful login without keyring provider (uses text store)
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
    Stored credentials for public@https://pypi-proxy.fly.dev/basic-auth
    "
    );

    // Token-based login
    uv_snapshot!(context.auth_login()
        .arg("https://example.com/simple")
        .arg("--token")
        .arg("test-token"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Stored credentials for https://example.com/
    "
    );
}

#[test]
fn token_text_store() {
    let context = TestContext::new_with_versions(&[]);

    // Login first
    context
        .auth_login()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--username")
        .arg("public")
        .arg("--password")
        .arg("heron")
        .assert()
        .success();

    // Retrieve the token
    uv_snapshot!(context.auth_token()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--username")
        .arg("public"), @r"
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
    uv_snapshot!(context.auth_token()
        .arg("https://example.com/simple"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    test-token

    ----- stderr -----
    "
    );
}

#[test]
fn logout_text_store() {
    let context = TestContext::new_with_versions(&[]);

    // Login first
    context
        .auth_login()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--username")
        .arg("public")
        .arg("--password")
        .arg("heron")
        .assert()
        .success();

    // Logout
    uv_snapshot!(context.auth_logout()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--username")
        .arg("public"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Removed credentials for public@https://pypi-proxy.fly.dev/basic-auth
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

    uv_snapshot!(context.auth_logout()
        .arg("https://example.com/simple"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Removed credentials for https://example.com/
    "
    );
}

#[test]
fn auth_disabled_provider_uses_text_store() {
    let context = TestContext::new_with_versions(&[]);

    // Login with disabled provider should use text store
    uv_snapshot!(context.auth_login()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--username")
        .arg("public")
        .arg("--password")
        .arg("heron")
        .arg("--keyring-provider")
        .arg("disabled"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Stored credentials for public@https://pypi-proxy.fly.dev/basic-auth
    "
    );

    // Token retrieval should work with disabled provider
    uv_snapshot!(context.auth_token()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--username")
        .arg("public")
        .arg("--keyring-provider")
        .arg("disabled"), @r"
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
    let context = TestContext::new_with_versions(&[]);

    // Login with `/simple` suffix - should strip it and store credentials for the root URL
    uv_snapshot!(context.auth_login()
        .arg("https://example.com/simple")
        .arg("--username")
        .arg("testuser")
        .arg("--password")
        .arg("testpass"), @r"
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
        .arg("devpipass"), @r"
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
        .arg("casepass"), @r"
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
        .arg("apipass"), @r"
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
        .arg("slashpass"), @r"
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
    let context = TestContext::new_with_versions(&[]);

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
        .arg("testuser"), @r"
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
        .arg("devpiuser"), @r"
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
    let context = TestContext::new_with_versions(&[]);

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
        .arg("testuser"), @r"
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
        .arg("https://token.example.com/simple"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    secret-token

    ----- stderr -----
    "
    );
}
