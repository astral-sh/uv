use crate::common::{uv_snapshot, TestContext};
use assert_cmd::assert::OutputAssertExt;
use assert_fs::fixture::{FileTouch, PathChild};
use std::io::Write;
use std::process::{Command, Stdio};
use std::{env, iter};
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
    warning: `uv publish` is experimental and may change without warning
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
    warning: `uv publish` is experimental and may change without warning
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
    warning: `uv publish` is experimental and may change without warning
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
    warning: `uv publish` is experimental and may change without warning
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
    warning: `uv publish` is experimental and may change without warning
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
    warning: `uv publish` is experimental and may change without warning
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
    warning: `uv publish` is experimental and may change without warning
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

    // No upper bound, since we are recommending `uv tool install keyring` without upper bound to
    // users. The keyring package had a bad release in the past, but so it hits us before it hits
    // users.
    context
        .pip_install()
        .arg("keyring")
        // Contains the plaintext keyring
        .arg("keyrings.alt")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
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
        // Avoid `gi.repository.GLib.GError`
        .env(
            "PYTHON_KEYRING_BACKEND",
            "keyrings.alt.file.PlaintextKeyring",
        ), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: `uv publish` is experimental and may change without warning
    Publishing 1 file to https://test.pypi.org/legacy/?ok
    error: Failed to query check URL
      Caused by: Package `ok` was not found in the registry
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
        // Avoid `gi.repository.GLib.GError`
        .env(
            "PYTHON_KEYRING_BACKEND",
            "keyrings.alt.file.PlaintextKeyring",
        ),  @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: `uv publish` is experimental and may change without warning
    Publishing 1 file to https://test.pypi.org/legacy/?ok
    warning: Using `--keyring-provider` with a password or token and no check url has no effect
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
        // Avoid `gi.repository.GLib.GError`
        .env(
            "PYTHON_KEYRING_BACKEND",
            "keyrings.alt.file.PlaintextKeyring",
        ),  @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: `uv publish` is experimental and may change without warning
    Publishing 1 file to https://test.pypi.org/legacy/?ok
    warning: Keyring has no password for URL `https://test.pypi.org/legacy/?ok` and username `dummy`
    error: Failed to query check URL
      Caused by: Package `ok` was not found in the registry
    "###
    );

    let mut child = Command::new(context.interpreter())
        .arg("-m")
        .arg("keyring")
        .arg("set")
        .arg("https://test.pypi.org/legacy/?ok")
        .arg("dummy")
        // Don't save the dummy to the host's actual keyring!
        .env(
            "PYTHON_KEYRING_BACKEND",
            "keyrings.alt.file.PlaintextKeyring",
        )
        // Configure keyring file location
        .env("XDG_DATA_HOME", context.temp_dir.path())
        .env("LOCALAPPDATA", context.temp_dir.path())
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().expect("Failed to open stdin");
    std::thread::spawn(move || {
        stdin
            .write_all("dummy\n".as_bytes())
            .expect("Failed to write to stdin");
        stdin.flush().expect("Failed to flush stdin");
    });
    let status = child.wait().expect("Failed to wait on child");
    assert!(status.success(), "Setting password in keyring failed");

    // Ok: There is a keyring entry for the user dummy.
    // https://github.com/astral-sh/uv/issues/7963#issuecomment-2453558043
    let path_with_venv = env::join_paths(
        iter::once(context.interpreter().parent().unwrap().to_path_buf())
            .chain(env::split_paths(&env::var_os("PATH").unwrap())),
    )
    .unwrap();
    uv_snapshot!(context.filters(), context.publish()
        .arg("-u")
        .arg("dummy")
        .arg("--keyring-provider")
        .arg("subprocess")
        .arg("--publish-url")
        .arg("https://test.pypi.org/legacy/?ok")
        .arg("../../scripts/links/ok-1.0.0-py3-none-any.whl")
        // Don't save the dummy to the host's actual keyring!
        .env(
            "PYTHON_KEYRING_BACKEND",
            "keyrings.alt.file.PlaintextKeyring",
        )
        // Configure keyring file location
        .env("XDG_DATA_HOME", context.temp_dir.path())
        .env("LOCALAPPDATA", context.temp_dir.path())
        .env("PATH", path_with_venv), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: `uv publish` is experimental and may change without warning
    Publishing 1 file to https://test.pypi.org/legacy/?ok
    Uploading ok-1.0.0-py3-none-any.whl ([SIZE])
    error: Failed to publish `../../scripts/links/ok-1.0.0-py3-none-any.whl` to https://test.pypi.org/legacy/?ok
      Caused by: Upload failed with status code 403 Forbidden. Server says: 403 Username/Password authentication is no longer supported. Migrate to API Tokens or Trusted Publishers instead. See https://test.pypi.org/help/#apitoken and https://test.pypi.org/help/#trusted-publishers
    "###
    );
}
