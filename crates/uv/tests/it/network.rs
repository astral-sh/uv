use std::env;

use assert_fs::fixture::{FileWriteStr, PathChild};
use http::StatusCode;
use serde_json::json;
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::common::{TestContext, uv_snapshot};

/// Check the simple index error message when the server returns HTTP status 500, a retryable error.
#[tokio::test]
async fn simple_http_500() {
    let context = TestContext::new("3.12");

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(StatusCode::INTERNAL_SERVER_ERROR))
        .mount(&server)
        .await;
    let mock_server_uri = server.uri();

    let filters = vec![(mock_server_uri.as_str(), "[SERVER]")];
    uv_snapshot!(filters, context
        .pip_install()
        .arg("tqdm")
        .arg("--index-url")
        .arg(server.uri()), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to fetch: `[SERVER]/tqdm/`
      Caused by: HTTP status server error (500 Internal Server Error) for url ([SERVER]/tqdm/)
    ");
}

/// Check the find links error message when the server returns HTTP status 500, a retryable error.
#[tokio::test]
async fn find_links_http_500() {
    let context = TestContext::new("3.12");

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(StatusCode::INTERNAL_SERVER_ERROR))
        .mount(&server)
        .await;
    let mock_server_uri = server.uri();

    let filters = vec![(mock_server_uri.as_str(), "[SERVER]")];
    uv_snapshot!(filters, context
        .pip_install()
        .arg("tqdm")
        .arg("--no-index")
        .arg("--find-links")
        .arg(server.uri()), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to read `--find-links` URL: [SERVER]/
      Caused by: Failed to fetch: `[SERVER]/`
      Caused by: HTTP status server error (500 Internal Server Error) for url ([SERVER]/)
    ");
}

/// Check the direct package URL error message when the server returns HTTP status 500, a retryable
/// error.
#[tokio::test]
async fn direct_url_http_500() {
    let context = TestContext::new("3.12");

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(StatusCode::INTERNAL_SERVER_ERROR))
        .mount(&server)
        .await;
    let mock_server_uri = server.uri();

    let tqdm_url = format!(
        "{mock_server_uri}/packages/d0/30/dc54f88dd4a2b5dc8a0279bdd7270e735851848b762aeb1c1184ed1f6b14/tqdm-4.67.1-py3-none-any.whl"
    );
    let filters = vec![(mock_server_uri.as_str(), "[SERVER]")];
    uv_snapshot!(filters, context
        .pip_install()
        .arg(format!("tqdm @ {tqdm_url}")), @r"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to download `tqdm @ [SERVER]/packages/d0/30/dc54f88dd4a2b5dc8a0279bdd7270e735851848b762aeb1c1184ed1f6b14/tqdm-4.67.1-py3-none-any.whl`
      ├─▶ Failed to fetch: `[SERVER]/packages/d0/30/dc54f88dd4a2b5dc8a0279bdd7270e735851848b762aeb1c1184ed1f6b14/tqdm-4.67.1-py3-none-any.whl`
      ╰─▶ HTTP status server error (500 Internal Server Error) for url ([SERVER]/packages/d0/30/dc54f88dd4a2b5dc8a0279bdd7270e735851848b762aeb1c1184ed1f6b14/tqdm-4.67.1-py3-none-any.whl)
    ");
}

/// Check the Python install error message when the server returns HTTP status 500, a retryable
/// error.
#[tokio::test]
async fn python_install_http_500() {
    let context = TestContext::new("3.12")
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs();

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(StatusCode::INTERNAL_SERVER_ERROR))
        .mount(&server)
        .await;
    let mock_server_uri = server.uri();

    let python_downloads_json = context.temp_dir.child("python_downloads.json");
    let interpreter = json!({
        "cpython-3.10.0-darwin-aarch64-none": {
            "arch": {
                "family": "aarch64",
                "variant": null
            },
            "libc": "none",
            "major": 3,
            "minor": 10,
            "name": "cpython",
            "os": "darwin",
            "patch": 0,
            "prerelease": "",
            "sha256": null,
            "url": format!("{mock_server_uri}/astral-sh/python-build-standalone/releases/download/20211017/cpython-3.10.0-aarch64-apple-darwin-pgo%2Blto-20211017T1616.tar.zst"),
            "variant": null
        }
    });
    python_downloads_json
        .write_str(&serde_json::to_string(&interpreter).unwrap())
        .unwrap();

    let filters = vec![(mock_server_uri.as_str(), "[SERVER]")];
    uv_snapshot!(filters, context
        .python_install()
        .arg("cpython-3.10.0-darwin-aarch64-none")
        .arg("--python-downloads-json-url")
        .arg(python_downloads_json.path()), @r"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    error: Failed to install cpython-3.10.0-macos-aarch64-none
      Caused by: Failed to download [SERVER]/astral-sh/python-build-standalone/releases/download/20211017/cpython-3.10.0-aarch64-apple-darwin-pgo%2Blto-20211017T1616.tar.zst
      Caused by: HTTP status server error (500 Internal Server Error) for url ([SERVER]/astral-sh/python-build-standalone/releases/download/20211017/cpython-3.10.0-aarch64-apple-darwin-pgo%2Blto-20211017T1616.tar.zst)
    ");
}
