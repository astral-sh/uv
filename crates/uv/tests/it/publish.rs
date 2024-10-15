use crate::common::{uv_snapshot, TestContext};
use assert_fs::fixture::{FileTouch, PathChild};

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
      Caused by: Permission denied (status code 403 Forbidden): 403 Username/Password authentication is no longer supported. Migrate to API Tokens or Trusted Publishers instead. See https://test.pypi.org/help/#apitoken and https://test.pypi.org/help/#trusted-publishers
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
      Caused by: Permission denied (status code 403 Forbidden): 403 Invalid or non-existent authentication information. See https://test.pypi.org/help/#invalid-auth for more information.
    "###
    );
}

/// Check that we warn about unnormalized filenames.
#[test]
fn invalid_validation_error() {
    let context = TestContext::new("3.12");

    let dist = context.temp_dir.child("dist");
    // Unescaped dot
    dist.child("aviary.gsm8k-0.7.6-py3-none-any.whl")
        .touch()
        .unwrap();
    // Unescaped dot
    dist.child("aviary.gsm8k-0.7.6.tar.gz").touch().unwrap();
    // Complex but valid
    dist.child("maturin-1.7.4-py3.py2-none-manylinux_2_12_x86_64.manylinux2010_x86_64.musllinux_1_1_x86_64.whl").touch().unwrap();

    uv_snapshot!(context.filters(), context.publish()
        .arg("--token")
        .arg("dummy")
        .arg("--publish-url")
        .arg("https://test.pypi.org/legacy/")
        .current_dir(&context.temp_dir), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: `uv publish` is experimental and may change without warning
    warning: Invalid filename: Expected `aviary_gsm8k-0.7.6-py3-none-any.whl`, found `aviary.gsm8k-0.7.6-py3-none-any.whl`. This is a problem with the build backend.
    warning: Invalid filename: Expected `aviary_gsm8k-0.7.6.tar.gz`, found `aviary.gsm8k-0.7.6.tar.gz`. This is a problem with the build backend.
    Publishing 3 files https://test.pypi.org/legacy/
    Uploading aviary_gsm8k-0.7.6-py3-none-any.whl ([SIZE])
    error: Failed to publish: `dist/aviary.gsm8k-0.7.6-py3-none-any.whl`
      Caused by: Failed to read metadata
      Caused by: Failed to read from zip file
      Caused by: unable to locate the end of central directory record
    "###
    );
}
