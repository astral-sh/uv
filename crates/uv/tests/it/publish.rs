use crate::common::{uv_snapshot, venv_bin_path, TestContext};
use assert_cmd::assert::OutputAssertExt;
use assert_fs::fixture::{FileTouch, FileWriteStr, PathChild};
use indoc::indoc;
use std::env;
use std::env::current_dir;
use uv_static::EnvVars;

#[test]
fn username_password_no_longer_supported() {
    let context = TestContext::new("3.12");

    uv_snapshot!(context.filters(), context.publish()
        .arg("-u")
        .arg("dummy")
        .arg("-p")
        .arg("dummy")
        .arg("--publish-url")
        .arg("https://test.pypi.org/legacy/")
        .arg("../../scripts/links/ok-1.0.0-py3-none-any.whl"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Publishing 1 file to https://test.pypi.org/legacy/
    Uploading ok-1.0.0-py3-none-any.whl ([SIZE])
    error: Failed to publish `../../scripts/links/ok-1.0.0-py3-none-any.whl` to https://test.pypi.org/legacy/
      Caused by: Upload failed with status code 403 Forbidden. Server says: 403 Username/Password authentication is no longer supported. Migrate to API Tokens or Trusted Publishers instead. See https://test.pypi.org/help/#apitoken and https://test.pypi.org/help/#trusted-publishers
    "###
    );
}

#[test]
fn invalid_token() {
    let context = TestContext::new("3.12");

    uv_snapshot!(context.filters(), context.publish()
        .arg("-u")
        .arg("__token__")
        .arg("-p")
        .arg("dummy")
        .arg("--publish-url")
        .arg("https://test.pypi.org/legacy/")
        .arg("../../scripts/links/ok-1.0.0-py3-none-any.whl"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Publishing 1 file to https://test.pypi.org/legacy/
    Uploading ok-1.0.0-py3-none-any.whl ([SIZE])
    error: Failed to publish `../../scripts/links/ok-1.0.0-py3-none-any.whl` to https://test.pypi.org/legacy/
      Caused by: Upload failed with status code 403 Forbidden. Server says: 403 Invalid or non-existent authentication information. See https://test.pypi.org/help/#invalid-auth for more information.
    "###
    );
}

/// Emulate a missing `permission` `id-token: write` situation.
#[test]
fn mixed_credentials() {
    let context = TestContext::new("3.12");

    uv_snapshot!(context.filters(), context.publish()
        .arg("--username")
        .arg("ferris")
        .arg("--password")
        .arg("ZmVycmlz")
        .arg("--publish-url")
        .arg("https://test.pypi.org/legacy/")
        .arg("--trusted-publishing")
        .arg("always")
        .arg("../../scripts/links/ok-1.0.0-py3-none-any.whl")
        // Emulate CI
        .env(EnvVars::GITHUB_ACTIONS, "true")
        // Just to make sure
        .env_remove(EnvVars::ACTIONS_ID_TOKEN_REQUEST_TOKEN), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Publishing 1 file to https://test.pypi.org/legacy/
    error: a username and a password are not allowed when using trusted publishing
    "###
    );
}

/// Emulate a missing `permission` `id-token: write` situation.
#[test]
fn missing_trusted_publishing_permission() {
    let context = TestContext::new("3.12");

    uv_snapshot!(context.filters(), context.publish()
        .arg("--publish-url")
        .arg("https://test.pypi.org/legacy/")
        .arg("--trusted-publishing")
        .arg("always")
        .arg("../../scripts/links/ok-1.0.0-py3-none-any.whl")
        // Emulate CI
        .env(EnvVars::GITHUB_ACTIONS, "true")
        // Just to make sure
        .env_remove(EnvVars::ACTIONS_ID_TOKEN_REQUEST_TOKEN), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Publishing 1 file to https://test.pypi.org/legacy/
    error: Failed to obtain token for trusted publishing
      Caused by: Environment variable ACTIONS_ID_TOKEN_REQUEST_TOKEN not set, is the `id-token: write` permission missing?
    "###
    );
}

/// Check the error when there are no credentials provided on GitHub Actions. Is it an incorrect
/// trusted publishing configuration?
#[test]
fn no_credentials() {
    let context = TestContext::new("3.12");

    uv_snapshot!(context.filters(), context.publish()
        .arg("--publish-url")
        .arg("https://test.pypi.org/legacy/")
        .arg("../../scripts/links/ok-1.0.0-py3-none-any.whl")
        // Emulate CI
        .env(EnvVars::GITHUB_ACTIONS, "true")
        // Just to make sure
        .env_remove(EnvVars::ACTIONS_ID_TOKEN_REQUEST_TOKEN), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Publishing 1 file to https://test.pypi.org/legacy/
    Note: Neither credentials nor keyring are configured, and there was an error fetching the trusted publishing token. If you don't want to use trusted publishing, you can ignore this error, but you need to provide credentials.
    Trusted publishing error: Environment variable ACTIONS_ID_TOKEN_REQUEST_TOKEN not set, is the `id-token: write` permission missing?
    Uploading ok-1.0.0-py3-none-any.whl ([SIZE])
    error: Failed to publish `../../scripts/links/ok-1.0.0-py3-none-any.whl` to https://test.pypi.org/legacy/
      Caused by: Failed to send POST request
      Caused by: Missing credentials for https://test.pypi.org/legacy/
    "###
    );
}

/// Hint people that it's not `--skip-existing` but `--check-url`.
#[test]
fn skip_existing_redirect() {
    let context = TestContext::new("3.12");

    uv_snapshot!(context.filters(), context.publish()
        .arg("--skip-existing")
        .arg("--publish-url")
        .arg("https://test.pypi.org/legacy/"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: `uv publish` does not support `--skip-existing` because there is not a reliable way to identify when an upload fails due to an existing distribution. Instead, use `--check-url` to provide the URL to the simple API for your index. uv will check the index for existing distributions before attempting uploads.
    "###
    );
}

#[test]
fn dubious_filenames() {
    let context = TestContext::new("3.12");

    context.temp_dir.child("not-a-wheel.whl").touch().unwrap();
    context.temp_dir.child("data.tar.gz").touch().unwrap();
    context
        .temp_dir
        .child("not-sdist-1-2-3-asdf.zip")
        .touch()
        .unwrap();

    uv_snapshot!(context.filters(), context.publish()
        .arg("-u")
        .arg("dummy")
        .arg("-p")
        .arg("dummy")
        .arg("--publish-url")
        .arg("https://test.pypi.org/legacy/")
        .arg(context.temp_dir.join("*")), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: Skipping file that looks like a distribution, but is not a valid distribution filename: `[TEMP_DIR]/data.tar.gz`
    warning: Skipping file that looks like a distribution, but is not a valid distribution filename: `[TEMP_DIR]/not-a-wheel.whl`
    warning: Skipping file that looks like a distribution, but is not a valid distribution filename: `[TEMP_DIR]/not-sdist-1-2-3-asdf.zip`
    error: No files found to publish
    "###
    );
}

/// Check that we (don't) use the keyring and warn for missing keyring behaviors correctly.
#[test]
fn check_keyring_behaviours() {
    let context = TestContext::new("3.12");

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

    // Ok: The keyring may be used for the index page.
    uv_snapshot!(context.filters(), context.publish()
        .arg("-u")
        .arg("dummy")
        .arg("-p")
        .arg("dummy")
        .arg("--keyring-provider")
        .arg("subprocess")
        .arg("--check-url")
        .arg("https://test.pypi.org/simple/")
        .arg("--publish-url")
        .arg("https://test.pypi.org/legacy/?ok")
        .arg("../../scripts/links/ok-1.0.0-py3-none-any.whl")
        .env(EnvVars::PATH, venv_bin_path(&context.venv)), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Publishing 1 file to https://test.pypi.org/legacy/?ok
    Uploading ok-1.0.0-py3-none-any.whl ([SIZE])
    error: Failed to publish `../../scripts/links/ok-1.0.0-py3-none-any.whl` to https://test.pypi.org/legacy/?ok
      Caused by: Upload failed with status code 403 Forbidden. Server says: 403 Username/Password authentication is no longer supported. Migrate to API Tokens or Trusted Publishers instead. See https://test.pypi.org/help/#apitoken and https://test.pypi.org/help/#trusted-publishers
    "###
    );

    // Warn: The keyring is unused.
    uv_snapshot!(context.filters(), context.publish()
        .arg("-u")
        .arg("dummy")
        .arg("-p")
        .arg("dummy")
        .arg("--keyring-provider")
        .arg("subprocess")
        .arg("--publish-url")
        .arg("https://test.pypi.org/legacy/?ok")
        .arg("../../scripts/links/ok-1.0.0-py3-none-any.whl")
        .env(EnvVars::PATH, venv_bin_path(&context.venv)),  @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Publishing 1 file to https://test.pypi.org/legacy/?ok
    warning: Using `--keyring-provider` with a password or token and no check URL has no effect
    Uploading ok-1.0.0-py3-none-any.whl ([SIZE])
    error: Failed to publish `../../scripts/links/ok-1.0.0-py3-none-any.whl` to https://test.pypi.org/legacy/?ok
      Caused by: Upload failed with status code 403 Forbidden. Server says: 403 Username/Password authentication is no longer supported. Migrate to API Tokens or Trusted Publishers instead. See https://test.pypi.org/help/#apitoken and https://test.pypi.org/help/#trusted-publishers
    "###
    );

    // Warn: There is no keyring entry for the user dummy.
    // https://github.com/astral-sh/uv/issues/7963#issuecomment-2453558043
    uv_snapshot!(context.filters(), context.publish()
        .arg("-u")
        .arg("dummy")
        .arg("--keyring-provider")
        .arg("subprocess")
        .arg("--check-url")
        .arg("https://test.pypi.org/simple/")
        .arg("--publish-url")
        .arg("https://test.pypi.org/legacy/?ok")
        .arg("../../scripts/links/ok-1.0.0-py3-none-any.whl")
        .env(EnvVars::PATH, venv_bin_path(&context.venv)), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Publishing 1 file to https://test.pypi.org/legacy/?ok
    Request for dummy@https://test.pypi.org/legacy/?ok
    Request for dummy@test.pypi.org
    warning: Keyring has no password for URL `https://test.pypi.org/legacy/?ok` and username `dummy`
    Uploading ok-1.0.0-py3-none-any.whl ([SIZE])
    Request for dummy@https://test.pypi.org/legacy/?ok
    Request for dummy@test.pypi.org
    error: Failed to publish `../../scripts/links/ok-1.0.0-py3-none-any.whl` to https://test.pypi.org/legacy/?ok
      Caused by: Upload failed with status code 403 Forbidden. Server says: 403 Username/Password authentication is no longer supported. Migrate to API Tokens or Trusted Publishers instead. See https://test.pypi.org/help/#apitoken and https://test.pypi.org/help/#trusted-publishers
    "###
    );

    // Ok: There is a keyring entry for the user dummy.
    // https://github.com/astral-sh/uv/issues/7963#issuecomment-2453558043
    uv_snapshot!(context.filters(), context.publish()
        .arg("-u")
        .arg("dummy")
        .arg("--keyring-provider")
        .arg("subprocess")
        .arg("--publish-url")
        .arg("https://test.pypi.org/legacy/?ok")
        .arg("../../scripts/links/ok-1.0.0-py3-none-any.whl")
        .env(EnvVars::KEYRING_TEST_CREDENTIALS, r#"{"https://test.pypi.org/legacy/?ok": {"dummy": "dummy"}}"#)
        .env(EnvVars::PATH, venv_bin_path(&context.venv)), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Publishing 1 file to https://test.pypi.org/legacy/?ok
    Request for dummy@https://test.pypi.org/legacy/?ok
    Uploading ok-1.0.0-py3-none-any.whl ([SIZE])
    error: Failed to publish `../../scripts/links/ok-1.0.0-py3-none-any.whl` to https://test.pypi.org/legacy/?ok
      Caused by: Upload failed with status code 403 Forbidden. Server says: 403 Username/Password authentication is no longer supported. Migrate to API Tokens or Trusted Publishers instead. See https://test.pypi.org/help/#apitoken and https://test.pypi.org/help/#trusted-publishers
    "###
    );
}

#[test]
fn invalid_index() {
    let context = TestContext::new("3.12");

    let pyproject_toml = indoc! {r#"
        [project]
        name = "foo"
        version = "0.1.0"

        [[tool.uv.index]]
        explicit = true
        name = "foo"
        url = "https://example.com"

        [[tool.uv.index]]
        name = "internal"
        url = "https://internal.example.org"
    "#};
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(pyproject_toml)
        .unwrap();

    let ok_wheel = current_dir()
        .unwrap()
        .join("../../scripts/links/ok-1.0.0-py3-none-any.whl");

    // No such index
    uv_snapshot!(context.filters(), context.publish()
        .arg("-u")
        .arg("__token__")
        .arg("-p")
        .arg("dummy")
        .arg("--index")
        .arg("bar")
        .arg(&ok_wheel)
        .current_dir(context.temp_dir.path()), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Index not found: `bar`. Found indexes: `foo`, `internal`
    "###
    );

    // Index does not have a publish URL
    uv_snapshot!(context.filters(), context.publish()
        .arg("-u")
        .arg("__token__")
        .arg("-p")
        .arg("dummy")
        .arg("--index")
        .arg("foo")
        .arg(&ok_wheel)
        .current_dir(context.temp_dir.path()), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Index is missing a publish URL: `foo`
    "###
    );
}
