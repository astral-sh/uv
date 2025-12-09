use crate::common::{TestContext, uv_snapshot, venv_bin_path};
use assert_cmd::assert::OutputAssertExt;
use assert_fs::fixture::{FileTouch, FileWriteStr, PathChild};
use fs_err::OpenOptions;
use indoc::{formatdoc, indoc};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::env;
use std::env::current_dir;
use std::io::Write;
use uv_static::EnvVars;
use wiremock::matchers::{basic_auth, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

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
        .arg("../../test/links/ok-1.0.0-py3-none-any.whl"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Publishing 1 file to https://test.pypi.org/legacy/
    Uploading ok-1.0.0-py3-none-any.whl ([SIZE])
    error: Failed to publish `../../test/links/ok-1.0.0-py3-none-any.whl` to https://test.pypi.org/legacy/
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
        .arg("../../test/links/ok-1.0.0-py3-none-any.whl"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Publishing 1 file to https://test.pypi.org/legacy/
    Uploading ok-1.0.0-py3-none-any.whl ([SIZE])
    error: Failed to publish `../../test/links/ok-1.0.0-py3-none-any.whl` to https://test.pypi.org/legacy/
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
        .arg("../../test/links/ok-1.0.0-py3-none-any.whl")
        // Emulate CI
        .env(EnvVars::GITHUB_ACTIONS, "true"), @r###"
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
        .arg("../../test/links/ok-1.0.0-py3-none-any.whl")
        // Emulate CI
        .env(EnvVars::GITHUB_ACTIONS, "true"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Publishing 1 file to https://test.pypi.org/legacy/
    error: Failed to obtain token for trusted publishing
      Caused by: Failed to obtain OIDC token: is the `id-token: write` permission missing?
      Caused by: GitHub Actions detection error
      Caused by: insufficient permissions: missing ACTIONS_ID_TOKEN_REQUEST_URL
    "
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
        .arg("../../test/links/ok-1.0.0-py3-none-any.whl")
        // Emulate CI
        .env(EnvVars::GITHUB_ACTIONS, "true"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Publishing 1 file to https://test.pypi.org/legacy/
    Note: Neither credentials nor keyring are configured, and there was an error fetching the trusted publishing token. If you don't want to use trusted publishing, you can ignore this error, but you need to provide credentials.
    error: Trusted publishing failed
      Caused by: Failed to obtain OIDC token: is the `id-token: write` permission missing?
      Caused by: GitHub Actions detection error
      Caused by: insufficient permissions: missing ACTIONS_ID_TOKEN_REQUEST_URL
    Uploading ok-1.0.0-py3-none-any.whl ([SIZE])
    error: Failed to publish `../../test/links/ok-1.0.0-py3-none-any.whl` to https://test.pypi.org/legacy/
      Caused by: Failed to send POST request
      Caused by: Missing credentials for https://test.pypi.org/legacy/
    "
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
        .arg(context.temp_dir.join("*")), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: Skipping file that looks like a distribution, but is not a valid distribution filename: `[TEMP_DIR]/data.tar.gz`
    warning: Skipping file that looks like a distribution, but is not a valid distribution filename: `[TEMP_DIR]/not-a-wheel.whl`
    warning: Skipping file that looks like a distribution, but is not a valid distribution filename: `[TEMP_DIR]/not-sdist-1-2-3-asdf.zip`
    error: No files found to publish
    "
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
                .join("test")
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
        .arg("../../test/links/ok-1.0.0-py3-none-any.whl")
        .env(EnvVars::PATH, venv_bin_path(&context.venv)), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Publishing 1 file to https://test.pypi.org/legacy/?ok
    Uploading ok-1.0.0-py3-none-any.whl ([SIZE])
    error: Failed to publish `../../test/links/ok-1.0.0-py3-none-any.whl` to https://test.pypi.org/legacy/?ok
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
        .arg("../../test/links/ok-1.0.0-py3-none-any.whl")
        .env(EnvVars::PATH, venv_bin_path(&context.venv)),  @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Publishing 1 file to https://test.pypi.org/legacy/?ok
    warning: Using `--keyring-provider` with a password or token and no check URL has no effect
    Uploading ok-1.0.0-py3-none-any.whl ([SIZE])
    error: Failed to publish `../../test/links/ok-1.0.0-py3-none-any.whl` to https://test.pypi.org/legacy/?ok
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
        .arg("../../test/links/ok-1.0.0-py3-none-any.whl")
        .env(EnvVars::PATH, venv_bin_path(&context.venv)), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Publishing 1 file to https://test.pypi.org/legacy/?ok
    Keyring request for dummy@https://test.pypi.org/legacy/?ok
    Keyring request for dummy@test.pypi.org
    warning: Keyring has no password for URL `https://test.pypi.org/legacy/?ok` and username `dummy`
    Uploading ok-1.0.0-py3-none-any.whl ([SIZE])
    Keyring request for dummy@https://test.pypi.org/legacy/?ok
    Keyring request for dummy@test.pypi.org
    error: Failed to publish `../../test/links/ok-1.0.0-py3-none-any.whl` to https://test.pypi.org/legacy/?ok
      Caused by: Upload failed with status code 403 Forbidden. Server says: 403 Username/Password authentication is no longer supported. Migrate to API Tokens or Trusted Publishers instead. See https://test.pypi.org/help/#apitoken and https://test.pypi.org/help/#trusted-publishers
    "
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
        .arg("../../test/links/ok-1.0.0-py3-none-any.whl")
        .env(EnvVars::KEYRING_TEST_CREDENTIALS, r#"{"https://test.pypi.org/legacy/?ok": {"dummy": "dummy"}}"#)
        .env(EnvVars::PATH, venv_bin_path(&context.venv)), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Publishing 1 file to https://test.pypi.org/legacy/?ok
    Keyring request for dummy@https://test.pypi.org/legacy/?ok
    Uploading ok-1.0.0-py3-none-any.whl ([SIZE])
    error: Failed to publish `../../test/links/ok-1.0.0-py3-none-any.whl` to https://test.pypi.org/legacy/?ok
      Caused by: Upload failed with status code 403 Forbidden. Server says: 403 Username/Password authentication is no longer supported. Migrate to API Tokens or Trusted Publishers instead. See https://test.pypi.org/help/#apitoken and https://test.pypi.org/help/#trusted-publishers
    "
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
        .join("../../test/links/ok-1.0.0-py3-none-any.whl");

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

/// Ensure that we read index credentials from the environment when publishing.
///
/// <https://github.com/astral-sh/uv/issues/11836#issuecomment-3022735011>
#[tokio::test]
async fn read_index_credential_env_vars_for_check_url() {
    let context = TestContext::new("3.12");

    let server = MockServer::start().await;

    context
        .init()
        .arg("--name")
        .arg("astral-test-private")
        .arg(".")
        .assert()
        .success();

    context.build().arg("--wheel").assert().success();

    let mut file = OpenOptions::new()
        .write(true)
        .append(true)
        .create(false)
        .open(context.temp_dir.join("pyproject.toml"))
        .unwrap();
    file.write_all(
        formatdoc! {
            r#"
            [[tool.uv.index]]
            name = "private-index"
            url = "{index_uri}/simple/"
            publish-url = "{index_uri}/upload"
            "#,
            index_uri = server.uri()
        }
        .as_bytes(),
    )
    .unwrap();

    let filename = "astral_test_private-0.1.0-py3-none-any.whl";
    let wheel = context.temp_dir.join("dist").join(filename);
    let sha256 = format!("{:x}", Sha256::digest(fs_err::read(&wheel).unwrap()));

    let simple_index = json! ({
          "files": [
            {
              "filename": filename,
              "hashes": {
                "sha256": sha256
              },
              "url": format!("{}/{}", server.uri(), filename),
            }
        ]
    });
    Mock::given(method("GET"))
        .and(path("/simple/astral-test-private/"))
        .and(basic_auth("username", "secret"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            simple_index.to_string().into_bytes(),
            "application/vnd.pypi.simple.v1+json",
        ))
        .mount(&server)
        .await;

    // Test that we fail without credentials
    uv_snapshot!(context.filters(), context.publish()
        .current_dir(&context.temp_dir)
        .arg(&wheel)
        .arg("--index")
        .arg("private-index")
        .arg("--trusted-publishing")
        .arg("never"),
        @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Publishing 1 file to http://[LOCALHOST]/upload
    Uploading astral_test_private-0.1.0-py3-none-any.whl ([SIZE])
    error: Failed to publish `dist/astral_test_private-0.1.0-py3-none-any.whl` to http://[LOCALHOST]/upload
      Caused by: Failed to send POST request
      Caused by: Missing credentials for http://[LOCALHOST]/upload
    "
    );
    // Test that it works with credentials
    uv_snapshot!(context.filters(), context.publish()
        .current_dir(&context.temp_dir)
        .arg(&wheel)
        .arg("--index")
        .arg("private-index")
        .env(EnvVars::index_username("PRIVATE_INDEX"), "username")
        .env(EnvVars::index_password("PRIVATE_INDEX"), "secret")
        .arg("--trusted-publishing")
        .arg("never"),
        @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Publishing 1 file to http://[LOCALHOST]/upload
    File astral_test_private-0.1.0-py3-none-any.whl already exists, skipping
    "
    );
}

/// Native GitLab CI trusted publishing using `PYPI_ID_TOKEN`
#[tokio::test]
async fn gitlab_trusted_publishing_pypi_id_token() {
    let context = TestContext::new("3.12");

    let server = MockServer::start().await;

    // Audience endpoint (PyPI)
    Mock::given(method("GET"))
        .and(path("/_/oidc/audience"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw("{\"audience\":\"pypi\"}", "application/json"),
        )
        .mount(&server)
        .await;

    // Mint token endpoint returns a short-lived API token
    Mock::given(method("POST"))
        .and(path("/_/oidc/mint-token"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw("{\"token\":\"apitoken\"}", "application/json"),
        )
        .mount(&server)
        .await;

    // Upload endpoint requires the minted token as Basic auth
    Mock::given(method("POST"))
        .and(path("/upload"))
        .and(basic_auth("__token__", "apitoken"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    uv_snapshot!(context.filters(), context.publish()
        .arg("--trusted-publishing")
        .arg("always")
        .arg("--publish-url")
        .arg(format!("{}/upload", server.uri()))
        .arg("../../test/links/ok-1.0.0-py3-none-any.whl")
        .env(EnvVars::GITLAB_CI, "true")
        .env_remove(EnvVars::GITHUB_ACTIONS)
        .env(EnvVars::PYPI_ID_TOKEN, "gitlab-oidc-jwt"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Publishing 1 file to http://[LOCALHOST]/upload
    Uploading ok-1.0.0-py3-none-any.whl ([SIZE])
    "
    );
}

/// Native GitLab CI trusted publishing using `TESTPYPI_ID_TOKEN`
#[tokio::test]
async fn gitlab_trusted_publishing_testpypi_id_token() {
    let context = TestContext::new("3.12");

    let server = MockServer::start().await;

    // Audience endpoint (TestPyPI)
    Mock::given(method("GET"))
        .and(path("/_/oidc/audience"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw("{\"audience\":\"testpypi\"}", "application/json"),
        )
        .mount(&server)
        .await;

    // Mint token endpoint returns a short-lived API token
    Mock::given(method("POST"))
        .and(path("/_/oidc/mint-token"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw("{\"token\":\"apitoken\"}", "application/json"),
        )
        .mount(&server)
        .await;

    // Upload endpoint requires the minted token as Basic auth
    Mock::given(method("POST"))
        .and(path("/upload"))
        .and(basic_auth("__token__", "apitoken"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    uv_snapshot!(context.filters(), context.publish()
        .arg("--trusted-publishing")
        .arg("always")
        .arg("--publish-url")
        .arg(format!("{}/upload", server.uri()))
        .arg("../../test/links/ok-1.0.0-py3-none-any.whl")
        // Emulate GitLab CI with TESTPYPI_ID_TOKEN present
        .env(EnvVars::GITLAB_CI, "true")
        .env_remove(EnvVars::GITHUB_ACTIONS)
        .env(EnvVars::TESTPYPI_ID_TOKEN, "gitlab-oidc-jwt"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Publishing 1 file to http://[LOCALHOST]/upload
    Uploading ok-1.0.0-py3-none-any.whl ([SIZE])
    "
    );
}
