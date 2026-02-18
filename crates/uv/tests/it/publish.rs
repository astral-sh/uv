use assert_cmd::assert::OutputAssertExt;
use assert_fs::fixture::{FileTouch, FileWriteStr, PathChild};
use fs_err::OpenOptions;
use indoc::{formatdoc, indoc};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::env::current_dir;
use std::io::Write;
use std::path::{Path, PathBuf};
use uv_static::EnvVars;
use uv_test::{uv_snapshot, venv_bin_path};
use wiremock::matchers::{basic_auth, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn dummy_wheel() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("test/links/ok-1.0.0-py3-none-any.whl")
}

#[test]
fn username_password_no_longer_supported() {
    let context = uv_test::test_context!("3.12");

    uv_snapshot!(context.filters(), context.publish()
        .arg("-u")
        .arg("dummy")
        .arg("-p")
        .arg("dummy")
        .arg("--publish-url")
        .arg("https://test.pypi.org/legacy/")
        .arg(dummy_wheel()), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Publishing 1 file to https://test.pypi.org/legacy/
    Uploading ok-1.0.0-py3-none-any.whl ([SIZE])
    error: Failed to publish `[WORKSPACE]/test/links/ok-1.0.0-py3-none-any.whl` to https://test.pypi.org/legacy/
      Caused by: Server returned status code 403 Forbidden. Server says: 403 Username/Password authentication is no longer supported. Migrate to API Tokens or Trusted Publishers instead. See https://test.pypi.org/help/#apitoken and https://test.pypi.org/help/#trusted-publishers
    "
    );
}

#[test]
fn invalid_token() {
    let context = uv_test::test_context!("3.12");

    uv_snapshot!(context.filters(), context.publish()
        .arg("-u")
        .arg("__token__")
        .arg("-p")
        .arg("dummy")
        .arg("--publish-url")
        .arg("https://test.pypi.org/legacy/")
        .arg(dummy_wheel()), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Publishing 1 file to https://test.pypi.org/legacy/
    Uploading ok-1.0.0-py3-none-any.whl ([SIZE])
    error: Failed to publish `[WORKSPACE]/test/links/ok-1.0.0-py3-none-any.whl` to https://test.pypi.org/legacy/
      Caused by: Server returned status code 403 Forbidden. Server says: 403 Invalid or non-existent authentication information. See https://test.pypi.org/help/#invalid-auth for more information.
    "
    );
}

/// Emulate a missing `permission` `id-token: write` situation.
#[test]
fn mixed_credentials() {
    let context = uv_test::test_context!("3.12");

    uv_snapshot!(context.filters(), context.publish()
        .arg("--username")
        .arg("ferris")
        .arg("--password")
        .arg("ZmVycmlz")
        .arg("--publish-url")
        .arg("https://test.pypi.org/legacy/")
        .arg("--trusted-publishing")
        .arg("always")
        .arg(dummy_wheel())
        // Emulate CI
        .env(EnvVars::GITHUB_ACTIONS, "true"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Publishing 1 file to https://test.pypi.org/legacy/
    error: a username and a password are not allowed when using trusted publishing
    "
    );
}

/// Emulate a missing `permission` `id-token: write` situation.
#[test]
fn missing_trusted_publishing_permission() {
    let context = uv_test::test_context!("3.12");

    uv_snapshot!(context.filters(), context.publish()
        .arg("--publish-url")
        .arg("https://test.pypi.org/legacy/")
        .arg("--trusted-publishing")
        .arg("always")
        .arg(dummy_wheel())
        // Emulate CI
        .env(EnvVars::GITHUB_ACTIONS, "true"), @"
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
    let context = uv_test::test_context!("3.12");

    uv_snapshot!(context.filters(), context.publish()
        .arg("--publish-url")
        .arg("https://test.pypi.org/legacy/")
        .arg(dummy_wheel())
        // Emulate CI
        .env(EnvVars::GITHUB_ACTIONS, "true"), @"
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
    error: Failed to publish `[WORKSPACE]/test/links/ok-1.0.0-py3-none-any.whl` to https://test.pypi.org/legacy/
      Caused by: Failed to send POST request
      Caused by: Missing credentials for https://test.pypi.org/legacy/
    "
    );
}

/// Hint people that it's not `--skip-existing` but `--check-url`.
#[test]
fn skip_existing_redirect() {
    let context = uv_test::test_context!("3.12");

    uv_snapshot!(context.filters(), context.publish()
        .arg("--skip-existing")
        .arg("--publish-url")
        .arg("https://test.pypi.org/legacy/"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: `uv publish` does not support `--skip-existing` because there is not a reliable way to identify when an upload fails due to an existing distribution. Instead, use `--check-url` to provide the URL to the simple API for your index. uv will check the index for existing distributions before attempting uploads.
    "
    );
}

#[test]
fn dubious_filenames() {
    let context = uv_test::test_context!("3.12");

    context.temp_dir.child("not-a-wheel.whl").touch().unwrap();
    context.temp_dir.child("data.tar.gz").touch().unwrap();
    context
        .temp_dir
        .child("not-sdist-1-2-3-asdf.zip")
        .touch()
        .unwrap();

    uv_snapshot!(context.filters(), context.publish()
        .current_dir(current_dir().unwrap())
        .arg("-u")
        .arg("dummy")
        .arg("-p")
        .arg("dummy")
        .arg("--publish-url")
        .arg("https://test.pypi.org/legacy/")
        .arg(context.temp_dir.join("*")), @"
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
    let context = uv_test::test_context!("3.12");

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
        .arg(dummy_wheel())
        .env(EnvVars::PATH, venv_bin_path(&context.venv)), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Publishing 1 file to https://test.pypi.org/legacy/?ok
    Uploading ok-1.0.0-py3-none-any.whl ([SIZE])
    error: Failed to publish `[WORKSPACE]/test/links/ok-1.0.0-py3-none-any.whl` to https://test.pypi.org/legacy/?ok
      Caused by: Server returned status code 403 Forbidden. Server says: 403 Username/Password authentication is no longer supported. Migrate to API Tokens or Trusted Publishers instead. See https://test.pypi.org/help/#apitoken and https://test.pypi.org/help/#trusted-publishers
    "
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
        .arg(dummy_wheel())
        .env(EnvVars::PATH, venv_bin_path(&context.venv)),  @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Publishing 1 file to https://test.pypi.org/legacy/?ok
    warning: Using `--keyring-provider` with a password or token and no check URL has no effect
    Uploading ok-1.0.0-py3-none-any.whl ([SIZE])
    error: Failed to publish `[WORKSPACE]/test/links/ok-1.0.0-py3-none-any.whl` to https://test.pypi.org/legacy/?ok
      Caused by: Server returned status code 403 Forbidden. Server says: 403 Username/Password authentication is no longer supported. Migrate to API Tokens or Trusted Publishers instead. See https://test.pypi.org/help/#apitoken and https://test.pypi.org/help/#trusted-publishers
    "
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
        .arg(dummy_wheel())
        .env(EnvVars::PATH, venv_bin_path(&context.venv)), @"
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
    error: Failed to publish `[WORKSPACE]/test/links/ok-1.0.0-py3-none-any.whl` to https://test.pypi.org/legacy/?ok
      Caused by: Server returned status code 403 Forbidden. Server says: 403 Username/Password authentication is no longer supported. Migrate to API Tokens or Trusted Publishers instead. See https://test.pypi.org/help/#apitoken and https://test.pypi.org/help/#trusted-publishers
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
        .arg(dummy_wheel())
        .env(EnvVars::KEYRING_TEST_CREDENTIALS, r#"{"https://test.pypi.org/legacy/?ok": {"dummy": "dummy"}}"#)
        .env(EnvVars::PATH, venv_bin_path(&context.venv)), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Publishing 1 file to https://test.pypi.org/legacy/?ok
    Keyring request for dummy@https://test.pypi.org/legacy/?ok
    Uploading ok-1.0.0-py3-none-any.whl ([SIZE])
    error: Failed to publish `[WORKSPACE]/test/links/ok-1.0.0-py3-none-any.whl` to https://test.pypi.org/legacy/?ok
      Caused by: Server returned status code 403 Forbidden. Server says: 403 Username/Password authentication is no longer supported. Migrate to API Tokens or Trusted Publishers instead. See https://test.pypi.org/help/#apitoken and https://test.pypi.org/help/#trusted-publishers
    "
    );
}

#[test]
fn invalid_index() {
    let context = uv_test::test_context!("3.12");

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
        .current_dir(context.temp_dir.path()), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Index not found: `bar`. Found indexes: `foo`, `internal`
    "
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
        .current_dir(context.temp_dir.path()), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Index is missing a publish URL: `foo`
    "
    );
}

/// Ensure that we read index credentials from the environment when publishing.
///
/// <https://github.com/astral-sh/uv/issues/11836#issuecomment-3022735011>
#[tokio::test]
async fn read_index_credential_env_vars_for_check_url() {
    let context = uv_test::test_context!("3.12");

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
        .arg(&wheel)
        .arg("--index")
        .arg("private-index")
        .arg("--trusted-publishing")
        .arg("never"),
        @"
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
        .arg(&wheel)
        .arg("--index")
        .arg("private-index")
        .env(EnvVars::index_username("PRIVATE_INDEX"), "username")
        .env(EnvVars::index_password("PRIVATE_INDEX"), "secret")
        .arg("--trusted-publishing")
        .arg("never"),
        @"
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
    let context = uv_test::test_context!("3.12");

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
        .arg(dummy_wheel())
        .env(EnvVars::GITLAB_CI, "true")
        .env(EnvVars::PYPI_ID_TOKEN, "gitlab-oidc-jwt"), @"
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
    let context = uv_test::test_context!("3.12");

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
        .arg(dummy_wheel())
        // Emulate GitLab CI with TESTPYPI_ID_TOKEN present
        .env(EnvVars::GITLAB_CI, "true")
        .env(EnvVars::TESTPYPI_ID_TOKEN, "gitlab-oidc-jwt"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Publishing 1 file to http://[LOCALHOST]/upload
    Uploading ok-1.0.0-py3-none-any.whl ([SIZE])
    "
    );
}

/// PyPI returns `application/json` errors with a `code` field.
#[tokio::test]
async fn upload_error_pypi_json() {
    let context = uv_test::test_context!("3.12");
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/upload"))
        .respond_with(ResponseTemplate::new(400).set_body_raw(
            r#"{"message": "Error", "code": "400 Use 'source' as Python version for an sdist.", "title": "Bad Request"}"#,
            "application/json",
        ))
        .mount(&server)
        .await;

    uv_snapshot!(context.filters(), context.publish()
        .arg("-u")
        .arg("dummy")
        .arg("-p")
        .arg("dummy")
        .arg("--publish-url")
        .arg(format!("{}/upload", server.uri()))
        .arg(dummy_wheel()), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Publishing 1 file to http://[LOCALHOST]/upload
    Uploading ok-1.0.0-py3-none-any.whl ([SIZE])
    error: Failed to publish `[WORKSPACE]/test/links/ok-1.0.0-py3-none-any.whl` to http://[LOCALHOST]/upload
      Caused by: Server returned status code 400 Bad Request. Server says: 400 Use 'source' as Python version for an sdist.
    "
    );
}

/// pyx returns `application/problem+json` errors with RFC 9457 Problem Details.
#[tokio::test]
async fn upload_error_problem_details() {
    let context = uv_test::test_context!("3.12");
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/upload"))
        .respond_with(ResponseTemplate::new(400).set_body_raw(
            r#"{"type": "about:blank", "status": 400, "title": "Bad Request", "detail": "Missing required field `name`"}"#,
            "application/problem+json",
        ))
        .mount(&server)
        .await;

    uv_snapshot!(context.filters(), context.publish()
        .arg("-u")
        .arg("dummy")
        .arg("-p")
        .arg("dummy")
        .arg("--publish-url")
        .arg(format!("{}/upload", server.uri()))
        .arg(dummy_wheel()), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Publishing 1 file to http://[LOCALHOST]/upload
    Uploading ok-1.0.0-py3-none-any.whl ([SIZE])
    error: Failed to publish `[WORKSPACE]/test/links/ok-1.0.0-py3-none-any.whl` to http://[LOCALHOST]/upload
      Caused by: Server returned status code 400 Bad Request. Server message: Bad Request, Missing required field `name`
    "
    );
}

/// Test that `--dry-run` checks all files and reports all errors instead of
/// stopping at the first failure.
#[test]
fn dry_run_reports_all_errors() {
    let context = uv_test::test_context!("3.12");

    // Create two fake wheel files that will fail metadata reading.
    let wheel_a = context.temp_dir.child("a-1.0.0-py3-none-any.whl");
    wheel_a.touch().unwrap();
    let wheel_b = context.temp_dir.child("b-1.0.0-py3-none-any.whl");
    wheel_b.touch().unwrap();

    uv_snapshot!(context.filters(), context.publish()
        .arg("--dry-run")
        .arg("--publish-url")
        .arg("https://test.pypi.org/legacy/")
        .arg("--token")
        .arg("dummy")
        .arg(wheel_a.path())
        .arg(wheel_b.path()), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Checking 2 files against https://test.pypi.org/legacy/
    Checking a-1.0.0-py3-none-any.whl ([SIZE])
    error: Failed to publish: `a-1.0.0-py3-none-any.whl`
      Caused by: Failed to read metadata
      Caused by: Failed to read from zip file
      Caused by: unable to locate the end of central directory record
    Checking b-1.0.0-py3-none-any.whl ([SIZE])
    error: Failed to publish: `b-1.0.0-py3-none-any.whl`
      Caused by: Failed to read metadata
      Caused by: Failed to read from zip file
      Caused by: unable to locate the end of central directory record
    Found issues with 2 files
    "
    );
}

/// Warn when a wheel has a non-normalized filename (e.g., leading zeros in version).
#[test]
fn non_normalized_filename_warning() {
    let context = uv_test::test_context!("3.12");

    // Create a wheel file with a non-normalized version (leading zero: 1.01.0 -> 1.1.0).
    let wheel = context.temp_dir.child("ok-1.01.0-py3-none-any.whl");
    wheel.touch().unwrap();

    uv_snapshot!(context.filters(), context.publish()
        .arg("-u")
        .arg("dummy")
        .arg("-p")
        .arg("dummy")
        .arg("--publish-url")
        .arg("https://test.pypi.org/legacy/")
        .arg(wheel.path()), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Publishing 1 file to https://test.pypi.org/legacy/
    warning: `ok-1.01.0-py3-none-any.whl` has a non-normalized filename (expected `ok-1.1.0-py3-none-any.whl`). Pass `--preview-features publish-require-normalized` to skip such files.
    Uploading ok-1.1.0-py3-none-any.whl ([SIZE])
    error: Failed to publish: `ok-1.01.0-py3-none-any.whl`
      Caused by: Failed to read metadata
      Caused by: Failed to read from zip file
      Caused by: unable to locate the end of central directory record
    "
    );
}

/// With the preview flag, skip wheels with non-normalized filenames.
#[test]
fn non_normalized_filename_skip() {
    let context = uv_test::test_context!("3.12");

    // Create a wheel file with a non-normalized version.
    let wheel = context.temp_dir.child("ok-1.01.0-py3-none-any.whl");
    wheel.touch().unwrap();

    uv_snapshot!(context.filters(), context.publish()
        .arg("--preview-features")
        .arg("publish-require-normalized")
        .arg("-u")
        .arg("dummy")
        .arg("-p")
        .arg("dummy")
        .arg("--publish-url")
        .arg("https://test.pypi.org/legacy/")
        .arg(wheel.path()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Publishing 1 file to https://test.pypi.org/legacy/
    warning: `ok-1.01.0-py3-none-any.whl` has a non-normalized filename (expected `ok-1.1.0-py3-none-any.whl`), skipping
    "
    );
}
