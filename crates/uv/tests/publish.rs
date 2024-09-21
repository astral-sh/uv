#![cfg(feature = "pypi")]

use common::{uv_snapshot, TestContext};

mod common;

#[test]
fn username_password_no_longer_supported() {
    let context = TestContext::new("3.12");

    uv_snapshot!(context.filters(), context.publish()
        .arg("-u")
        .arg("dummy")
        .arg("-p")
        .arg("dummy")
        .arg("../../scripts/links/ok-1.0.0-py3-none-any.whl"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: `uv.sources` is experimental and may change without warning
    Publishing 1 file
    Uploading ok-1.0.0-py3-none-any.whl ([SIZE])
    error: Failed to publish `../../scripts/links/ok-1.0.0-py3-none-any.whl` to `https://upload.pypi.org/legacy/`
      Caused by: Incorrect credentials (status code 403 Forbidden): 403 Username/Password authentication is no longer supported. Migrate to API Tokens or Trusted Publishers instead. See https://pypi.org/help/#apitoken and https://pypi.org/help/#trusted-publishers
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
        .arg("../../scripts/links/ok-1.0.0-py3-none-any.whl"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: `uv.sources` is experimental and may change without warning
    Publishing 1 file
    Uploading ok-1.0.0-py3-none-any.whl ([SIZE])
    error: Failed to publish `../../scripts/links/ok-1.0.0-py3-none-any.whl` to `https://upload.pypi.org/legacy/`
      Caused by: Incorrect credentials (status code 403 Forbidden): 403 Invalid or non-existent authentication information. See https://pypi.org/help/#invalid-auth for more information.
    "###
    );
}
