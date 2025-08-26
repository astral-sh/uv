use anyhow::Result;
use assert_fs::{fixture::PathChild, prelude::FileWriteStr};

use crate::common::{TestContext, uv_snapshot};

#[test]
fn auth_add_package() -> Result<()> {
    let context = TestContext::new("3.12");

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
    warning: Unable to fetch credentials for https://pypi-proxy.fly.dev/basic-auth/simple from system keyring: Platform secure storage failure: A default keychain could not be found.
    warning: Unable to fetch credentials for pypi-proxy.fly.dev from system keyring: Platform secure storage failure: A default keychain could not be found.
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
    warning: Unable to store credentials for public@https://pypi-proxy.fly.dev/basic-auth/simple in the system keyring: Platform secure storage failure: A default keychain could not be found.
    "
    );

    // Try to add the original package without credentials again. This should use
    // credentials storied in the system keyring.
    uv_snapshot!(context.add().arg("anyio").arg("--default-index").arg("https://public@pypi-proxy.fly.dev/basic-auth/simple"), @r"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    warning: Unable to fetch credentials for https://pypi-proxy.fly.dev/basic-auth/simple from system keyring: Platform secure storage failure: A default keychain could not be found.
    warning: Unable to fetch credentials for pypi-proxy.fly.dev from system keyring: Platform secure storage failure: A default keychain could not be found.
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
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Unable to remove credentials for https://pypi-proxy.fly.dev/basic-auth/simple
      Caused by: Platform secure storage failure: A default keychain could not be found.
      Caused by: A default keychain could not be found.
    "
    );

    // Authentication should fail again
    uv_snapshot!(context.add().arg("iniconfig").arg("--default-index").arg("https://public@pypi-proxy.fly.dev/basic-auth/simple"), @r"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    warning: Unable to fetch credentials for https://pypi-proxy.fly.dev/basic-auth/simple from system keyring: Platform secure storage failure: A default keychain could not be found.
    warning: Unable to fetch credentials for pypi-proxy.fly.dev from system keyring: Platform secure storage failure: A default keychain could not be found.
      × No solution found when resolving dependencies:
      ╰─▶ Because iniconfig was not found in the package registry and your project depends on iniconfig, we can conclude that your project's requirements are unsatisfiable.

          hint: An index URL (https://pypi-proxy.fly.dev/basic-auth/simple) could not be queried due to a lack of valid authentication credentials (401 Unauthorized).
      help: If you want to add the package regardless of the failed resolution, provide the `--frozen` flag to skip locking and syncing.
    "
    );

    Ok(())
}

#[test]
fn auth_show() {
    let context = TestContext::new_with_versions(&[]);

    // Without a keyring provider...
    uv_snapshot!(context.auth_show()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Cannot show credentials with `keyring-provider = disabled`
    ");

    uv_snapshot!(context.auth_show()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--keyring-provider")
        .arg("native"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Cannot show credentials with `keyring-provider = disabled`
    ");

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
    error: Cannot show credentials with `keyring-provider = disabled`
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
    warning: Unable to store credentials for public@https://pypi-proxy.fly.dev/basic-auth/simple in the system keyring: Platform secure storage failure: A default keychain could not be found.
    "
    );
}

#[test]
fn auth_login() {
    let context = TestContext::new_with_versions(&[]);

    // Without a keyring provider...
    uv_snapshot!(context.auth_login()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: `uv auth login` requires either a `--token` or a username, e.g., `uv auth login https://example.com user`
    ");

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
    warning: Unable to store credentials for public@https://pypi-proxy.fly.dev/basic-auth/simple in the system keyring: Platform secure storage failure: A default keychain could not be found.
    "
    );
}

#[test]
fn auth_logout() {
    let context = TestContext::new_with_versions(&[]);

    // Without a keyring provider...
    uv_snapshot!(context.auth_logout()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Unable to remove credentials for https://pypi-proxy.fly.dev/basic-auth/simple
      Caused by: Platform secure storage failure: A default keychain could not be found.
      Caused by: A default keychain could not be found.
    ");

    uv_snapshot!(context.auth_logout()
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--keyring-provider")
        .arg("native"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Unable to remove credentials for https://pypi-proxy.fly.dev/basic-auth/simple
      Caused by: Platform secure storage failure: A default keychain could not be found.
      Caused by: A default keychain could not be found.
    ");

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
      Caused by: Platform secure storage failure: A default keychain could not be found.
      Caused by: A default keychain could not be found.
    ");

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
    warning: Unable to store credentials for public@https://pypi-proxy.fly.dev/basic-auth/simple in the system keyring: Platform secure storage failure: A default keychain could not be found.
    "
    );

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
      Caused by: Platform secure storage failure: A default keychain could not be found.
      Caused by: A default keychain could not be found.
    ");
}
