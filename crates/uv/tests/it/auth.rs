#[cfg(feature = "keyring-tests")]
use anyhow::Result;
use assert_cmd::assert::OutputAssertExt;
#[cfg(feature = "keyring-tests")]
use assert_fs::{fixture::PathChild, prelude::FileWriteStr};
#[cfg(feature = "keyring-tests")]
use uv_static::EnvVars;

#[cfg(feature = "keyring-tests")]
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
    Stored credentials for public@https://pypi-proxy.fly.dev/basic-auth/simple
    "
    );

    // Try to add the original package without credentials again. This should use
    // credentials storied in the system keyring.
    uv_snapshot!(context.add().arg("anyio").arg("--default-index").arg("https://public@pypi-proxy.fly.dev/basic-auth/simple"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: The native keyring provider is experimental and may change without warning. Pass `--preview-features native-keyring` to disable this warning.
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
    warning: The native keyring provider is experimental and may change without warning. Pass `--preview-features native-keyring` to disable this warning.
    Removed credentials for public@https://pypi-proxy.fly.dev/basic-auth/simple
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

    // With keyring provider - should fail without stored credentials
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
    Stored credentials for public@https://pypi-proxy.fly.dev/basic-auth/simple
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
    warning: The native keyring provider is experimental and may change without warning. Pass `--preview-features native-keyring` to disable this warning.
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
    Stored credentials for https://pypi-proxy.fly.dev/basic-auth/simple
    "
    );

    // Retrieve the token without a username
    uv_snapshot!(context.auth_token()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--keyring-provider")
        .arg("native"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    heron

    ----- stderr -----
    warning: The native keyring provider is experimental and may change without warning. Pass `--preview-features native-keyring` to disable this warning.
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
#[cfg(feature = "keyring-tests")]
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
    Keyring request for public@https://public@pypi-proxy.fly.dev/basic-auth/simple
    Keyring request for public@pypi-proxy.fly.dev
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
    success: true
    exit_code: 0
    ----- stdout -----
    heron

    ----- stderr -----
    Keyring request for public@https://public@pypi-proxy.fly.dev/basic-auth/simple
    Keyring request for public@pypi-proxy.fly.dev
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
    Stored credentials for public@https://pypi-proxy.fly.dev/basic-auth/simple
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
    Stored credentials for https://pypi-proxy.fly.dev/basic-auth/simple
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

    // Logout with keyring provider (should use keyring)
    uv_snapshot!(context.auth_logout()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--keyring-provider")
        .arg("native"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: The native keyring provider is experimental and may change without warning. Pass `--preview-features native-keyring` to disable this warning.
    Removed credentials for https://pypi-proxy.fly.dev/basic-auth/simple
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
    warning: The native keyring provider is experimental and may change without warning. Pass `--preview-features native-keyring` to disable this warning.
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
    warning: The native keyring provider is experimental and may change without warning. Pass `--preview-features native-keyring` to disable this warning.
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
    warning: The native keyring provider is experimental and may change without warning. Pass `--preview-features native-keyring` to disable this warning.
    Stored credentials for public@https://pypi-proxy.fly.dev/basic-auth/simple
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
    warning: The native keyring provider is experimental and may change without warning. Pass `--preview-features native-keyring` to disable this warning.
    Removed credentials for public@https://pypi-proxy.fly.dev/basic-auth/simple
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
    Removed credentials for public@https://pypi-proxy.fly.dev/basic-auth/simple
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
    Stored credentials for https://pypi-proxy.fly.dev/basic-auth/simple
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
    Removed credentials for https://pypi-proxy.fly.dev/basic-auth/simple
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
    Stored credentials for test@https://example.com/simple
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
    Stored credentials for test@https://example.com/simple
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
    Stored credentials for test@https://example.com/simple
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
    Stored credentials for public@https://pypi-proxy.fly.dev/basic-auth/simple
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
    Stored credentials for https://example.com/simple
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
    Removed credentials for public@https://pypi-proxy.fly.dev/basic-auth/simple
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
    Removed credentials for https://example.com/simple
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
    Stored credentials for public@https://pypi-proxy.fly.dev/basic-auth/simple
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
