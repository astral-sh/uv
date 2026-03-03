use std::convert::Infallible;
use std::io;
use std::time::{Duration, Instant};

use assert_fs::fixture::{ChildPath, FileWriteStr, PathChild};
use bytes::Bytes;
use http::StatusCode;
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, StreamBody};
use hyper::body::Frame;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use serde_json::json;
use tokio_stream::wrappers::ReceiverStream;
use wiremock::matchers::{any, method};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

use uv_static::EnvVars;
use uv_test::{TestContext, uv_snapshot};

/// Creates a CONNECT tunnel proxy that forwards connections to the target.
///
/// Returns the proxy address. The proxy runs in a background thread.
fn start_connect_tunnel_proxy() -> std::net::SocketAddr {
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();

    // Spawn a real OS thread for the proxy server
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut client) = stream else { break };

            // Handle each connection in its own thread
            std::thread::spawn(move || {
                // Read the CONNECT request
                let mut buf = vec![0u8; 4096];
                let mut total_read = 0;
                loop {
                    let n = match client.read(&mut buf[total_read..]) {
                        Ok(0) | Err(_) => return,
                        Ok(n) => n,
                    };
                    total_read += n;
                    if buf[..total_read].windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                }

                let request = String::from_utf8_lossy(&buf[..total_read]);

                // Parse "CONNECT host:port HTTP/1.1\r\n"
                let Some(target_addr) = request
                    .lines()
                    .next()
                    .and_then(|line| line.strip_prefix("CONNECT "))
                    .and_then(|s| s.split_whitespace().next())
                    .map(ToString::to_string)
                else {
                    return;
                };

                // Connect to the target
                let Ok(mut target) = TcpStream::connect(&target_addr) else {
                    return;
                };

                // Send 200 Connection Established
                if client
                    .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                    .is_err()
                {
                    return;
                }

                // Bidirectionally forward data using two threads
                let mut client_read = client.try_clone().unwrap();
                let mut target_write = target.try_clone().unwrap();

                let c2t =
                    std::thread::spawn(move || std::io::copy(&mut client_read, &mut target_write));

                let _ = std::io::copy(&mut target, &mut client);
                let _ = c2t.join();
            });
        }
    });

    addr
}

/// Creates a mock that serves a Simple API index page for iniconfig.
async fn mock_simple_api(server: &MockServer) {
    // Simple API response for iniconfig pointing to the real PyPI wheel.
    // Uses upload-time before EXCLUDE_NEWER (2024-03-25) so the package is available.
    let body = json!({
        "name": "iniconfig",
        "files": [{
            "filename": "iniconfig-2.0.0-py3-none-any.whl",
            "url": "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl",
            "hashes": {
                "sha256": "2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3"
            },
            "requires-python": ">=3.8",
            "upload-time": "2024-01-01T00:00:00Z"
        }]
    });

    // Serve the simple index for iniconfig - use any() matcher since HTTP proxy
    // requests may have the full URL in the path
    Mock::given(any())
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(body.to_string(), "application/vnd.pypi.simple.v1+json"),
        )
        .mount(server)
        .await;
}

fn connection_reset(_request: &wiremock::Request) -> io::Error {
    io::Error::new(io::ErrorKind::ConnectionReset, "Connection reset by peer")
}

/// Returns true if the mock server has received any requests.
async fn has_received_requests(server: &MockServer) -> bool {
    !server.received_requests().await.unwrap().is_empty()
}

/// Answers with a retryable HTTP status 500.
async fn http_error_server() -> (MockServer, String) {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(StatusCode::INTERNAL_SERVER_ERROR))
        .mount(&server)
        .await;

    let mock_server_uri = server.uri();
    (server, mock_server_uri)
}

/// Answers with a retryable connection reset IO error.
async fn io_error_server() -> (MockServer, String) {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with_err(connection_reset)
        .mount(&server)
        .await;

    let mock_server_uri = server.uri();
    (server, mock_server_uri)
}

/// Answers with a retryable HTTP status 500 for 2 times, then with a retryable connection reset
/// IO error.
///
/// Tests different errors paths inside uv, which retries 3 times by default, for a total for 4
/// requests.
async fn mixed_error_server() -> (MockServer, String) {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .respond_with_err(connection_reset)
        .up_to_n_times(2)
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(StatusCode::INTERNAL_SERVER_ERROR))
        .up_to_n_times(2)
        .mount(&server)
        .await;

    let mock_server_uri = server.uri();
    (server, mock_server_uri)
}

async fn time_out_response(
    _req: hyper::Request<hyper::body::Incoming>,
) -> Result<hyper::Response<BoxBody<Bytes, Infallible>>, Infallible> {
    let (tx, rx) = tokio::sync::mpsc::channel(1);
    tokio::spawn(async move {
        let _ = tx.send(Ok(Frame::data(Bytes::new()))).await;
        tokio::time::sleep(Duration::from_secs(60)).await;
    });
    let body = StreamBody::new(ReceiverStream::new(rx)).boxed();
    Ok(hyper::Response::builder()
        .header("Content-Type", "text/html")
        .body(body)
        .unwrap())
}

/// Returns the server URL and a drop guard that shuts down the server.
///
/// The server runs in a thread with its own tokio runtime, so it
/// won't be starved by the subprocess blocking the test thread. Dropping the
/// guard shuts down the runtime and all tasks running in it.
fn read_timeout_server() -> (String, impl Drop) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let server = format!("http://{}", listener.local_addr().unwrap());

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async move {
            let listener = tokio::net::TcpListener::from_std(listener).unwrap();
            tokio::select! {
                _ = async {
                    loop {
                        let (stream, _) = listener.accept().await.unwrap();
                        let io = TokioIo::new(stream);

                        tokio::spawn(async move {
                           let _ = hyper_util::server::conn::auto::Builder::new(
                                hyper_util::rt::TokioExecutor::new(),
                            )
                            .serve_connection(io, service_fn(time_out_response))
                            .await;
                        });
                    }
                } => {}
                _ = shutdown_rx => {}
            }
        });
    });

    (server, shutdown_tx)
}

/// Check the simple index error message when the server returns HTTP status 500, a retryable error.
#[tokio::test]
async fn simple_http_500() {
    let context = uv_test::test_context!("3.12");

    let (_server_drop_guard, mock_server_uri) = http_error_server().await;

    let filters = vec![(mock_server_uri.as_str(), "[SERVER]")];
    uv_snapshot!(filters, context
        .pip_install()
        .arg("tqdm")
        .arg("--index-url")
        .arg(&mock_server_uri)
        .env(EnvVars::UV_TEST_NO_HTTP_RETRY_DELAY, "true"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Request failed after 3 retries
      Caused by: Failed to fetch: `[SERVER]/tqdm/`
      Caused by: HTTP status server error (500 Internal Server Error) for url ([SERVER]/tqdm/)
    ");
}

/// Check the simple index error message when the server returns a retryable IO error.
#[tokio::test]
async fn simple_io_err() {
    let context = uv_test::test_context!("3.12");

    let (_server_drop_guard, mock_server_uri) = io_error_server().await;

    let filters = vec![(mock_server_uri.as_str(), "[SERVER]")];
    uv_snapshot!(filters, context
        .pip_install()
        .arg("tqdm")
        .arg("--index-url")
        .arg(&mock_server_uri)
        .env(EnvVars::UV_TEST_NO_HTTP_RETRY_DELAY, "true"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Request failed after 3 retries
      Caused by: Failed to fetch: `[SERVER]/tqdm/`
      Caused by: error sending request for url ([SERVER]/tqdm/)
      Caused by: client error (SendRequest)
      Caused by: connection closed before message completed
    ");
}

/// Check the find links error message when the server returns HTTP status 500, a retryable error.
#[tokio::test]
async fn find_links_http_500() {
    let context = uv_test::test_context!("3.12");

    let (_server_drop_guard, mock_server_uri) = http_error_server().await;

    let filters = vec![(mock_server_uri.as_str(), "[SERVER]")];
    uv_snapshot!(filters, context
        .pip_install()
        .arg("tqdm")
        .arg("--no-index")
        .arg("--find-links")
        .arg(&mock_server_uri)
        .env(EnvVars::UV_TEST_NO_HTTP_RETRY_DELAY, "true"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to read `--find-links` URL: [SERVER]/
      Caused by: Request failed after 3 retries
      Caused by: Failed to fetch: `[SERVER]/`
      Caused by: HTTP status server error (500 Internal Server Error) for url ([SERVER]/)
    ");
}

/// Check the find links error message when the server returns a retryable IO error.
#[tokio::test]
async fn find_links_io_error() {
    let context = uv_test::test_context!("3.12");

    let (_server_drop_guard, mock_server_uri) = io_error_server().await;

    let filters = vec![(mock_server_uri.as_str(), "[SERVER]")];
    uv_snapshot!(filters, context
        .pip_install()
        .arg("tqdm")
        .arg("--no-index")
        .arg("--find-links")
        .arg(&mock_server_uri)
        .env(EnvVars::UV_TEST_NO_HTTP_RETRY_DELAY, "true"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to read `--find-links` URL: [SERVER]/
      Caused by: Request failed after 3 retries
      Caused by: Failed to fetch: `[SERVER]/`
      Caused by: error sending request for url ([SERVER]/)
      Caused by: client error (SendRequest)
      Caused by: connection closed before message completed
    ");
}

/// Check the error message for a find links index page, a non-streaming request, when the server
/// returns different kinds of retryable errors.
#[tokio::test]
async fn find_links_mixed_error() {
    let context = uv_test::test_context!("3.12");

    let (_server_drop_guard, mock_server_uri) = mixed_error_server().await;

    let filters = vec![(mock_server_uri.as_str(), "[SERVER]")];
    uv_snapshot!(filters, context
        .pip_install()
        .arg("tqdm")
        .arg("--no-index")
        .arg("--find-links")
        .arg(&mock_server_uri)
        .env(EnvVars::UV_TEST_NO_HTTP_RETRY_DELAY, "true"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to read `--find-links` URL: [SERVER]/
      Caused by: Request failed after 3 retries
      Caused by: Failed to fetch: `[SERVER]/`
      Caused by: HTTP status server error (500 Internal Server Error) for url ([SERVER]/)
    ");
}

/// Check the direct package URL error message when the server returns HTTP status 500, a retryable
/// error.
#[tokio::test]
async fn direct_url_http_500() {
    let context = uv_test::test_context!("3.12");

    let (_server_drop_guard, mock_server_uri) = http_error_server().await;

    let tqdm_url = format!(
        "{mock_server_uri}/packages/d0/30/dc54f88dd4a2b5dc8a0279bdd7270e735851848b762aeb1c1184ed1f6b14/tqdm-4.67.1-py3-none-any.whl"
    );
    let filters = vec![(mock_server_uri.as_str(), "[SERVER]")];
    uv_snapshot!(filters, context
        .pip_install()
        .arg(format!("tqdm @ {tqdm_url}"))
        .env(EnvVars::UV_TEST_NO_HTTP_RETRY_DELAY, "true"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to download `tqdm @ [SERVER]/packages/d0/30/dc54f88dd4a2b5dc8a0279bdd7270e735851848b762aeb1c1184ed1f6b14/tqdm-4.67.1-py3-none-any.whl`
      ├─▶ Request failed after 3 retries
      ├─▶ Failed to fetch: `[SERVER]/packages/d0/30/dc54f88dd4a2b5dc8a0279bdd7270e735851848b762aeb1c1184ed1f6b14/tqdm-4.67.1-py3-none-any.whl`
      ╰─▶ HTTP status server error (500 Internal Server Error) for url ([SERVER]/packages/d0/30/dc54f88dd4a2b5dc8a0279bdd7270e735851848b762aeb1c1184ed1f6b14/tqdm-4.67.1-py3-none-any.whl)
    ");
}

/// Check the direct package URL error message when the server returns a retryable IO error.
#[tokio::test]
async fn direct_url_io_error() {
    let context = uv_test::test_context!("3.12");

    let (_server_drop_guard, mock_server_uri) = io_error_server().await;

    let tqdm_url = format!(
        "{mock_server_uri}/packages/d0/30/dc54f88dd4a2b5dc8a0279bdd7270e735851848b762aeb1c1184ed1f6b14/tqdm-4.67.1-py3-none-any.whl"
    );
    let filters = vec![(mock_server_uri.as_str(), "[SERVER]")];
    uv_snapshot!(filters, context
        .pip_install()
        .arg(format!("tqdm @ {tqdm_url}"))
        .env(EnvVars::UV_TEST_NO_HTTP_RETRY_DELAY, "true"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to download `tqdm @ [SERVER]/packages/d0/30/dc54f88dd4a2b5dc8a0279bdd7270e735851848b762aeb1c1184ed1f6b14/tqdm-4.67.1-py3-none-any.whl`
      ├─▶ Request failed after 3 retries
      ├─▶ Failed to fetch: `[SERVER]/packages/d0/30/dc54f88dd4a2b5dc8a0279bdd7270e735851848b762aeb1c1184ed1f6b14/tqdm-4.67.1-py3-none-any.whl`
      ├─▶ error sending request for url ([SERVER]/packages/d0/30/dc54f88dd4a2b5dc8a0279bdd7270e735851848b762aeb1c1184ed1f6b14/tqdm-4.67.1-py3-none-any.whl)
      ├─▶ client error (SendRequest)
      ╰─▶ connection closed before message completed
    ");
}

/// Check the error message for direct package URL, a streaming request, when the server returns
/// different kinds of retryable errors.
#[tokio::test]
async fn direct_url_mixed_error() {
    let context = uv_test::test_context!("3.12");

    let (_server_drop_guard, mock_server_uri) = mixed_error_server().await;

    let tqdm_url = format!(
        "{mock_server_uri}/packages/d0/30/dc54f88dd4a2b5dc8a0279bdd7270e735851848b762aeb1c1184ed1f6b14/tqdm-4.67.1-py3-none-any.whl"
    );
    let filters = vec![(mock_server_uri.as_str(), "[SERVER]")];
    uv_snapshot!(filters, context
        .pip_install()
        .arg(format!("tqdm @ {tqdm_url}"))
        .env(EnvVars::UV_TEST_NO_HTTP_RETRY_DELAY, "true"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to download `tqdm @ [SERVER]/packages/d0/30/dc54f88dd4a2b5dc8a0279bdd7270e735851848b762aeb1c1184ed1f6b14/tqdm-4.67.1-py3-none-any.whl`
      ├─▶ Request failed after 3 retries
      ├─▶ Failed to fetch: `[SERVER]/packages/d0/30/dc54f88dd4a2b5dc8a0279bdd7270e735851848b762aeb1c1184ed1f6b14/tqdm-4.67.1-py3-none-any.whl`
      ╰─▶ HTTP status server error (500 Internal Server Error) for url ([SERVER]/packages/d0/30/dc54f88dd4a2b5dc8a0279bdd7270e735851848b762aeb1c1184ed1f6b14/tqdm-4.67.1-py3-none-any.whl)
    ");
}

fn write_python_downloads_json(context: &TestContext, mock_server_uri: &String) -> ChildPath {
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
    python_downloads_json
}

/// Check the Python install error message when the server returns HTTP status 500, a retryable
/// error.
#[tokio::test]
async fn python_install_http_500() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs();

    let (_server_drop_guard, mock_server_uri) = http_error_server().await;

    let python_downloads_json = write_python_downloads_json(&context, &mock_server_uri);

    let filters = vec![(mock_server_uri.as_str(), "[SERVER]")];
    uv_snapshot!(filters, context
        .python_install()
        .arg("cpython-3.10.0-darwin-aarch64-none")
        .arg("--python-downloads-json-url")
        .arg(python_downloads_json.path())
        .env(EnvVars::UV_TEST_NO_HTTP_RETRY_DELAY, "true"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    error: Failed to install cpython-3.10.0-macos-aarch64-none
      Caused by: Request failed after 3 retries
      Caused by: Failed to download [SERVER]/astral-sh/python-build-standalone/releases/download/20211017/cpython-3.10.0-aarch64-apple-darwin-pgo%2Blto-20211017T1616.tar.zst
      Caused by: HTTP status server error (500 Internal Server Error) for url ([SERVER]/astral-sh/python-build-standalone/releases/download/20211017/cpython-3.10.0-aarch64-apple-darwin-pgo%2Blto-20211017T1616.tar.zst)
    ");
}

/// Check the Python install error message when the server returns a retryable IO error.
#[tokio::test]
async fn python_install_io_error() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs();

    let (_server_drop_guard, mock_server_uri) = io_error_server().await;

    let python_downloads_json = write_python_downloads_json(&context, &mock_server_uri);

    let filters = vec![(mock_server_uri.as_str(), "[SERVER]")];
    uv_snapshot!(filters, context
        .python_install()
        .arg("cpython-3.10.0-darwin-aarch64-none")
        .arg("--python-downloads-json-url")
        .arg(python_downloads_json.path())
        .env(EnvVars::UV_TEST_NO_HTTP_RETRY_DELAY, "true"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    error: Failed to install cpython-3.10.0-macos-aarch64-none
      Caused by: Request failed after 3 retries
      Caused by: Failed to download [SERVER]/astral-sh/python-build-standalone/releases/download/20211017/cpython-3.10.0-aarch64-apple-darwin-pgo%2Blto-20211017T1616.tar.zst
      Caused by: error sending request for url ([SERVER]/astral-sh/python-build-standalone/releases/download/20211017/cpython-3.10.0-aarch64-apple-darwin-pgo%2Blto-20211017T1616.tar.zst)
      Caused by: client error (SendRequest)
      Caused by: connection closed before message completed
    ");
}

#[tokio::test]
async fn install_http_retries() {
    let context = uv_test::test_context!("3.12");

    let server = MockServer::start().await;

    // Create a server that always fails, so we can see the number of retries used
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(503))
        .expect(6)
        .mount(&server)
        .await;

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("anyio")
        .arg("--index")
        .arg(server.uri())
        .env(EnvVars::UV_HTTP_RETRIES, "foo"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to parse environment variable `UV_HTTP_RETRIES` with invalid value `foo`: invalid digit found in string
    "
    );

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("anyio")
        .arg("--index")
        .arg(server.uri())
        .env(EnvVars::UV_HTTP_RETRIES, "-1"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to parse environment variable `UV_HTTP_RETRIES` with invalid value `-1`: invalid digit found in string
    "
    );

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("anyio")
        .arg("--index")
        .arg(server.uri())
        .env(EnvVars::UV_HTTP_RETRIES, "999999999999"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to parse environment variable `UV_HTTP_RETRIES` with invalid value `999999999999`: number too large to fit in target type
    "
    );

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("anyio")
        .arg("--index")
        .arg(server.uri())
        .env(EnvVars::UV_HTTP_RETRIES, "5")
        .env(EnvVars::UV_TEST_NO_HTTP_RETRY_DELAY, "true"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Request failed after 5 retries
      Caused by: Failed to fetch: `http://[LOCALHOST]/anyio/`
      Caused by: HTTP status server error (503 Service Unavailable) for url (http://[LOCALHOST]/anyio/)
    "
    );
}

#[tokio::test]
async fn install_http_retry_low_level() {
    let context = uv_test::test_context!("3.12");

    let server = MockServer::start().await;

    // Create a server that fails with a more fundamental error so we trigger
    // earlier error paths
    Mock::given(method("GET"))
        .respond_with_err(|_: &'_ Request| io::Error::new(io::ErrorKind::ConnectionReset, "error"))
        .expect(2)
        .mount(&server)
        .await;

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("anyio")
        .arg("--index")
        .arg(server.uri())
        .env(EnvVars::UV_HTTP_RETRIES, "1")
        .env(EnvVars::UV_TEST_NO_HTTP_RETRY_DELAY, "true"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Request failed after 1 retry
      Caused by: Failed to fetch: `http://[LOCALHOST]/anyio/`
      Caused by: error sending request for url (http://[LOCALHOST]/anyio/)
      Caused by: client error (SendRequest)
      Caused by: connection closed before message completed
    "
    );
}

/// Test problem details with a 403 error containing license compliance information
#[tokio::test]
async fn rfc9457_problem_details_license_violation() {
    let context = uv_test::test_context!("3.12");

    let server = MockServer::start().await;

    let problem_json = r#"{
        "type": "https://example.com/probs/license-violation",
        "title": "License Compliance Issue",
        "status": 403,
        "detail": "This package version has a license that violates organizational policy."
    }"#;

    // Mock HEAD request to return 200 OK
    Mock::given(method("HEAD"))
        .respond_with(ResponseTemplate::new(StatusCode::OK))
        .mount(&server)
        .await;

    // Mock GET request to return 403 with problem details
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(StatusCode::FORBIDDEN)
                .set_body_raw(problem_json, "application/problem+json"),
        )
        .mount(&server)
        .await;

    let mock_server_uri = server.uri();
    let tqdm_url = format!("{mock_server_uri}/packages/tqdm-4.67.1-py3-none-any.whl");

    let filters = vec![(mock_server_uri.as_str(), "[SERVER]")];
    uv_snapshot!(filters, context
        .pip_install()
        .arg(format!("tqdm @ {tqdm_url}")), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to download `tqdm @ [SERVER]/packages/tqdm-4.67.1-py3-none-any.whl`
      ├─▶ Failed to fetch: `[SERVER]/packages/tqdm-4.67.1-py3-none-any.whl`
      ├─▶ Server message: License Compliance Issue, This package version has a license that violates organizational policy.
      ╰─▶ HTTP status client error (403 Forbidden) for url ([SERVER]/packages/tqdm-4.67.1-py3-none-any.whl)
    ");
}

/// Test that invalid proxy URL in uv.toml produces a helpful error message.
#[tokio::test]
async fn proxy_invalid_url_in_uv_toml() {
    let context = uv_test::test_context!("3.12");

    let uv_toml = context.temp_dir.child("uv.toml");
    uv_toml
        .write_str(indoc::indoc! {r#"
            http-proxy = "ftp://proxy.example.com:8080"
        "#})
        .unwrap();

    uv_snapshot!(context.filters(), context
        .pip_install()
        .arg("iniconfig")
        .env_remove(EnvVars::HTTP_PROXY)
        .env_remove(EnvVars::HTTPS_PROXY), @r#"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to parse: `uv.toml`
      Caused by: TOML parse error at line 1, column 14
      |
    1 | http-proxy = "ftp://proxy.example.com:8080"
      |              ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    invalid proxy URL scheme `ftp` in `ftp://proxy.example.com:8080/`: expected http, https, socks5, or socks5h
    "#);
}

/// Test that invalid proxy URL (not a URL) in uv.toml produces a helpful error message.
#[tokio::test]
async fn proxy_invalid_url_not_a_url_in_uv_toml() {
    let context = uv_test::test_context!("3.12");

    let uv_toml = context.temp_dir.child("uv.toml");
    uv_toml
        .write_str(indoc::indoc! {r#"
            http-proxy = "not a valid url"
        "#})
        .unwrap();

    uv_snapshot!(context.filters(), context
        .pip_install()
        .arg("iniconfig")
        .env_remove(EnvVars::HTTP_PROXY)
        .env_remove(EnvVars::HTTPS_PROXY), @r#"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to parse: `uv.toml`
      Caused by: TOML parse error at line 1, column 14
      |
    1 | http-proxy = "not a valid url"
      |              ^^^^^^^^^^^^^^^^^
    invalid proxy URL: invalid international domain name
    "#);
}

/// Test that valid proxy URL in uv.toml routes requests through the proxy.
#[cfg(feature = "test-pypi")]
#[tokio::test]
async fn proxy_valid_url_in_uv_toml() {
    let context = uv_test::test_context!("3.12");

    let target_server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(200))
        .mount(&target_server)
        .await;

    let proxy_server = MockServer::start().await;
    mock_simple_api(&proxy_server).await;

    let target_uri = target_server.uri();
    let proxy_uri = proxy_server.uri();

    let context = context
        .with_filter((target_uri.clone(), "[TARGET]"))
        .with_filter((proxy_uri.clone(), "[PROXY]"));

    let uv_toml = context.temp_dir.child("uv.toml");
    uv_toml
        .write_str(&format!(r#"http-proxy = "{proxy_uri}""#))
        .unwrap();

    uv_snapshot!(context.filters(), context
        .pip_install()
        .arg("iniconfig")
        .arg("--index-url")
        .arg(&target_uri)
        .arg("--config-file")
        .arg(uv_toml.path())
        .env_remove(EnvVars::HTTP_PROXY)
        .env_remove(EnvVars::HTTPS_PROXY)
        .env_remove(EnvVars::ALL_PROXY)
        .env_remove(EnvVars::NO_PROXY), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    assert!(
        has_received_requests(&proxy_server).await,
        "Proxy should have received the request"
    );
    assert!(
        !has_received_requests(&target_server).await,
        "Target should NOT have been called directly when proxy is configured"
    );
}

/// Test that https-proxy in uv.toml routes HTTPS requests through a CONNECT tunnel proxy.
#[cfg(feature = "test-pypi")]
#[test]
fn proxy_https_proxy_in_uv_toml() {
    let context = uv_test::test_context!("3.12");

    let proxy_addr = start_connect_tunnel_proxy();
    let proxy_uri = format!("http://{proxy_addr}");

    let context = context.with_filter((proxy_uri.clone(), "[PROXY]"));

    let uv_toml = context.temp_dir.child("uv.toml");
    uv_toml
        .write_str(&format!(r#"https-proxy = "{proxy_uri}""#))
        .unwrap();

    uv_snapshot!(context.filters(), context
        .pip_install()
        .arg("--config-file")
        .arg(uv_toml.path())
        .arg("iniconfig")
        .env_remove(EnvVars::HTTP_PROXY)
        .env_remove(EnvVars::HTTPS_PROXY)
        .env_remove(EnvVars::ALL_PROXY)
        .env_remove(EnvVars::NO_PROXY), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");
}

/// Test that no-proxy in uv.toml bypasses the proxy for specified hosts.
#[cfg(feature = "test-pypi")]
#[tokio::test]
async fn proxy_no_proxy_in_uv_toml() {
    let context = uv_test::test_context!("3.12");

    let target_server = MockServer::start().await;
    mock_simple_api(&target_server).await;

    let proxy_server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(200))
        .mount(&proxy_server)
        .await;

    let target_uri = target_server.uri();
    let proxy_uri = proxy_server.uri();

    // Note: reqwest's NoProxy matches on host only, not host:port
    let target_url = url::Url::parse(&target_uri).unwrap();
    let target_host = target_url.host_str().unwrap();

    let context = context
        .with_filter((target_uri.clone(), "[TARGET]"))
        .with_filter((proxy_uri.clone(), "[PROXY]"));

    let uv_toml = context.temp_dir.child("uv.toml");
    uv_toml
        .write_str(&format!(
            r#"
http-proxy = "{proxy_uri}"
no-proxy = ["{target_host}"]
"#
        ))
        .unwrap();

    uv_snapshot!(context.filters(), context
        .pip_install()
        .arg("iniconfig")
        .arg("--index-url")
        .arg(&target_uri)
        .arg("--config-file")
        .arg(uv_toml.path())
        .env_remove(EnvVars::HTTP_PROXY)
        .env_remove(EnvVars::HTTPS_PROXY)
        .env_remove(EnvVars::ALL_PROXY)
        .env_remove(EnvVars::NO_PROXY), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    assert!(
        has_received_requests(&target_server).await,
        "Target should have received the request directly when in no-proxy list"
    );
    assert!(
        !has_received_requests(&proxy_server).await,
        "Proxy should NOT have received requests when target is in no-proxy list"
    );
}

/// Test that proxy URLs without a scheme in uv.toml default to http://.
#[cfg(feature = "test-pypi")]
#[tokio::test]
async fn proxy_schemeless_url_in_uv_toml() {
    let context = uv_test::test_context!("3.12");

    let target_server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(200))
        .mount(&target_server)
        .await;

    let proxy_server = MockServer::start().await;
    mock_simple_api(&proxy_server).await;

    let target_uri = target_server.uri();
    let proxy_uri = proxy_server.uri();

    // Strip scheme to test schemeless URL handling
    let proxy_host = proxy_uri
        .strip_prefix("http://")
        .unwrap_or(proxy_uri.as_str());

    let context = context
        .with_filter((target_uri.clone(), "[TARGET]"))
        .with_filter((proxy_uri.clone(), "[PROXY]"))
        .with_filter((proxy_host, "[PROXY_HOST]"));

    let uv_toml = context.temp_dir.child("uv.toml");
    uv_toml
        .write_str(&format!(r#"http-proxy = "{proxy_host}""#))
        .unwrap();

    uv_snapshot!(context.filters(), context
        .pip_install()
        .arg("iniconfig")
        .arg("--index-url")
        .arg(&target_uri)
        .arg("--config-file")
        .arg(uv_toml.path())
        .env_remove(EnvVars::HTTP_PROXY)
        .env_remove(EnvVars::HTTPS_PROXY)
        .env_remove(EnvVars::ALL_PROXY)
        .env_remove(EnvVars::NO_PROXY), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    assert!(
        has_received_requests(&proxy_server).await,
        "Proxy should have received the request even with schemeless URL"
    );
    assert!(
        !has_received_requests(&target_server).await,
        "Target should NOT have been called directly when proxy is configured"
    );
}

#[test]
fn connect_timeout_index() {
    let context = uv_test::test_context!("3.12");

    // Create a server that never responds, causing a timeout for our requests.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let server = listener.local_addr().unwrap().to_string();

    let start = Instant::now();
    uv_snapshot!(context.filters(), context
        .pip_install()
        .arg("tqdm")
        .arg("--index-url")
        .arg(format!("https://{server}"))
        .env(EnvVars::UV_HTTP_CONNECT_TIMEOUT, "1")
        .env(EnvVars::UV_HTTP_RETRIES, "0"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to fetch: `https://[LOCALHOST]/tqdm/`
      Caused by: error sending request for url (https://[LOCALHOST]/tqdm/)
      Caused by: client error (Connect)
      Caused by: operation timed out
    ");

    // Assumption: There's less than 2s overhead for this test and startup.
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(3),
        "Test with 1s connect timeout took too long"
    );
}

#[test]
fn connect_timeout_stream() {
    let context = uv_test::test_context!("3.12");

    // Create a server that never responds, causing a timeout for our requests.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let server = listener.local_addr().unwrap().to_string();

    let start = Instant::now();
    uv_snapshot!(context.filters(), context
        .pip_install()
        .arg(format!("https://{server}/tqdm-0.1-py3-none-any.whl"))
        .env(EnvVars::UV_HTTP_CONNECT_TIMEOUT, "1")
        .env(EnvVars::UV_HTTP_RETRIES, "0"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to download `tqdm @ https://[LOCALHOST]/tqdm-0.1-py3-none-any.whl`
      ├─▶ Failed to fetch: `https://[LOCALHOST]/tqdm-0.1-py3-none-any.whl`
      ├─▶ error sending request for url (https://[LOCALHOST]/tqdm-0.1-py3-none-any.whl)
      ├─▶ client error (Connect)
      ╰─▶ operation timed out
    ");

    // Assumption: There's less than 2s overhead for this test and startup.
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(3),
        "Test with 1s connect timeout took too long"
    );
}

#[tokio::test]
async fn retry_read_timeout_index() {
    let context = uv_test::test_context!("3.12");

    let (server, _guard) = read_timeout_server();

    uv_snapshot!(context.filters(), context
        .pip_install()
        .arg("tqdm")
        .arg("--index-url")
        .arg(server)
        // Speed the test up with the minimum testable values
        .env(EnvVars::UV_HTTP_TIMEOUT, "1")
        .env(EnvVars::UV_HTTP_RETRIES, "1"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Request failed after 1 retry
      Caused by: Failed to fetch: `http://[LOCALHOST]/tqdm/`
      Caused by: error decoding response body
      Caused by: request or response body error
      Caused by: operation timed out
    ");
}

#[tokio::test]
async fn retry_read_timeout_stream() {
    let context = uv_test::test_context!("3.12");

    let (server, _guard) = read_timeout_server();

    uv_snapshot!(context.filters(), context
        .pip_install()
        .arg(format!("{server}/tqdm-0.1-py3-none-any.whl"))
        // Speed the test up with the minimum testable values
        .env(EnvVars::UV_HTTP_TIMEOUT, "1")
        .env(EnvVars::UV_HTTP_RETRIES, "1"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to download `tqdm @ http://[LOCALHOST]/tqdm-0.1-py3-none-any.whl`
      ├─▶ Request failed after 1 retry
      ├─▶ Failed to read metadata: `http://[LOCALHOST]/tqdm-0.1-py3-none-any.whl`
      ├─▶ Failed to read from zip file
      ├─▶ an upstream reader returned an error: Failed to download distribution due to network timeout. Try increasing UV_HTTP_TIMEOUT (current value: [TIME]).
      ╰─▶ Failed to download distribution due to network timeout. Try increasing UV_HTTP_TIMEOUT (current value: [TIME]).
    ");
}
