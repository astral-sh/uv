use uv_platform::{Arch, Os};
use uv_static::EnvVars;

use crate::common::{TestContext, uv_snapshot};
use anyhow::Result;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

#[test]
fn python_list() {
    let mut context: TestContext = TestContext::new_with_versions(&["3.11", "3.12"])
        .with_filtered_python_symlinks()
        .with_filtered_python_keys()
        .with_collapsed_whitespace();

    uv_snapshot!(context.filters(), context.python_list().env(EnvVars::UV_TEST_PYTHON_PATH, ""), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    ");

    // We show all interpreters
    uv_snapshot!(context.filters(), context.python_list(), @"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.12.[X]-[PLATFORM] [PYTHON-3.12]
    cpython-3.11.[X]-[PLATFORM] [PYTHON-3.11]

    ----- stderr -----
    ");

    // Request Python 3.12
    uv_snapshot!(context.filters(), context.python_list().arg("3.12"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.12.[X]-[PLATFORM] [PYTHON-3.12]

    ----- stderr -----
    ");

    // Request Python 3.11
    uv_snapshot!(context.filters(), context.python_list().arg("3.11"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.11.[X]-[PLATFORM] [PYTHON-3.11]

    ----- stderr -----
    ");

    // Request CPython
    uv_snapshot!(context.filters(), context.python_list().arg("cpython"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.12.[X]-[PLATFORM] [PYTHON-3.12]
    cpython-3.11.[X]-[PLATFORM] [PYTHON-3.11]

    ----- stderr -----
    ");

    // Request CPython 3.12
    uv_snapshot!(context.filters(), context.python_list().arg("cpython@3.12"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.12.[X]-[PLATFORM] [PYTHON-3.12]

    ----- stderr -----
    ");

    // Request CPython 3.12 via partial key syntax
    uv_snapshot!(context.filters(), context.python_list().arg("cpython-3.12"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.12.[X]-[PLATFORM] [PYTHON-3.12]

    ----- stderr -----
    ");

    // Request CPython 3.12 for the current platform
    let os = Os::from_env();
    let arch = Arch::from_env();

    uv_snapshot!(context.filters(), context.python_list().arg(format!("cpython-3.12-{os}-{arch}")), @"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.12.[X]-[PLATFORM] [PYTHON-3.12]

    ----- stderr -----
    ");

    // Request PyPy (which should be missing)
    uv_snapshot!(context.filters(), context.python_list().arg("pypy"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    ");

    // Swap the order of the Python versions
    context.python_versions.reverse();

    uv_snapshot!(context.filters(), context.python_list(), @"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.12.[X]-[PLATFORM] [PYTHON-3.12]
    cpython-3.11.[X]-[PLATFORM] [PYTHON-3.11]

    ----- stderr -----
    ");

    // Request Python 3.11
    uv_snapshot!(context.filters(), context.python_list().arg("3.11"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.11.[X]-[PLATFORM] [PYTHON-3.11]

    ----- stderr -----
    ");
}

#[test]
fn python_list_pin() {
    let context: TestContext = TestContext::new_with_versions(&["3.11", "3.12"])
        .with_filtered_python_symlinks()
        .with_filtered_python_keys()
        .with_collapsed_whitespace();

    // Pin to a version
    uv_snapshot!(context.filters(), context.python_pin().arg("3.12"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Pinned `.python-version` to `3.12`

    ----- stderr -----
    ");

    // The pin should not affect the listing
    uv_snapshot!(context.filters(), context.python_list(), @"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.12.[X]-[PLATFORM] [PYTHON-3.12]
    cpython-3.11.[X]-[PLATFORM] [PYTHON-3.11]

    ----- stderr -----
    ");

    // So `--no-config` has no effect
    uv_snapshot!(context.filters(), context.python_list().arg("--no-config"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.12.[X]-[PLATFORM] [PYTHON-3.12]
    cpython-3.11.[X]-[PLATFORM] [PYTHON-3.11]

    ----- stderr -----
    ");
}

#[test]
fn python_list_venv() {
    let context: TestContext = TestContext::new_with_versions(&["3.11", "3.12"])
        .with_filtered_python_symlinks()
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_filtered_python_names()
        .with_filtered_virtualenv_bin()
        .with_collapsed_whitespace();

    // Create a virtual environment
    uv_snapshot!(context.filters(), context.venv().arg("--python").arg("3.12").arg("-q"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    ");

    // We should not display the virtual environment
    uv_snapshot!(context.filters(), context.python_list(), @"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.12.[X]-[PLATFORM] [PYTHON-3.12]
    cpython-3.11.[X]-[PLATFORM] [PYTHON-3.11]

    ----- stderr -----
    ");

    // Same if the `VIRTUAL_ENV` is not set
    uv_snapshot!(context.filters(), context.python_list(), @"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.12.[X]-[PLATFORM] [PYTHON-3.12]
    cpython-3.11.[X]-[PLATFORM] [PYTHON-3.11]

    ----- stderr -----
    ");
}

#[cfg(unix)]
#[test]
fn python_list_unsupported_version() {
    let context: TestContext = TestContext::new_with_versions(&["3.12"])
        .with_filtered_python_symlinks()
        .with_filtered_python_keys();

    // Request a low version
    uv_snapshot!(context.filters(), context.python_list().arg("3.6"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Invalid version request: Python <3.7 is not supported but 3.6 was requested.
    ");

    // Request a low version with a patch
    uv_snapshot!(context.filters(), context.python_list().arg("3.6.9"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Invalid version request: Python <3.7 is not supported but 3.6.9 was requested.
    ");

    // Request a really low version
    uv_snapshot!(context.filters(), context.python_list().arg("2.6"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Invalid version request: Python <3.7 is not supported but 2.6 was requested.
    ");

    // Request a really low version with a patch
    uv_snapshot!(context.filters(), context.python_list().arg("2.6.8"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Invalid version request: Python <3.7 is not supported but 2.6.8 was requested.
    ");

    // Request a future version
    uv_snapshot!(context.filters(), context.python_list().arg("4.2"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    ");

    // Request a low version with a range
    uv_snapshot!(context.filters(), context.python_list().arg("<3.0"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    ");

    // Request free-threaded Python on unsupported version
    uv_snapshot!(context.filters(), context.python_list().arg("3.12t"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Invalid version request: Python <3.13 does not support free-threading but 3.12+freethreaded was requested.
    ");
}

#[test]
fn python_list_duplicate_path_entries() {
    let context: TestContext = TestContext::new_with_versions(&["3.11", "3.12"])
        .with_filtered_python_symlinks()
        .with_filtered_python_keys()
        .with_collapsed_whitespace();

    // Construct a `PATH` with all entries duplicated
    let path = std::env::join_paths(
        std::env::split_paths(&context.python_path())
            .chain(std::env::split_paths(&context.python_path())),
    )
    .unwrap();

    uv_snapshot!(context.filters(), context.python_list().env(EnvVars::UV_TEST_PYTHON_PATH, &path), @"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.12.[X]-[PLATFORM] [PYTHON-3.12]
    cpython-3.11.[X]-[PLATFORM] [PYTHON-3.11]

    ----- stderr -----
    ");

    #[cfg(unix)]
    {
        // Construct a `PATH` with symlinks
        let path = std::env::join_paths(std::env::split_paths(&context.python_path()).chain(
            std::env::split_paths(&context.python_path()).map(|path| {
                let dst = format!("{}-link", path.display());
                fs_err::os::unix::fs::symlink(&path, &dst).unwrap();
                std::path::PathBuf::from(dst)
            }),
        ))
        .unwrap();

        uv_snapshot!(context.filters(), context.python_list().env(EnvVars::UV_TEST_PYTHON_PATH, &path), @"
        success: true
        exit_code: 0
        ----- stdout -----
        cpython-3.12.[X]-[PLATFORM] [PYTHON-3.12]
        cpython-3.11.[X]-[PLATFORM] [PYTHON-3.11]

        ----- stderr -----
        ");

        // Reverse the order so the symlinks are first
        let path = std::env::join_paths(
            {
                let mut paths = std::env::split_paths(&path).collect::<Vec<_>>();
                paths.reverse();
                paths
            }
            .iter(),
        )
        .unwrap();

        uv_snapshot!(context.filters(), context.python_list().env(EnvVars::UV_TEST_PYTHON_PATH, &path), @"
        success: true
        exit_code: 0
        ----- stdout -----
        cpython-3.12.[X]-[PLATFORM] [PYTHON-3.12]-link/python3
        cpython-3.11.[X]-[PLATFORM] [PYTHON-3.11]-link/python3

        ----- stderr -----
        ");
    }
}

#[test]
fn python_list_downloads() {
    let context: TestContext = TestContext::new_with_versions(&[]).with_filtered_python_keys();

    // We do not test showing all interpreters — as it differs per platform
    // Instead, we choose a Python version where our available distributions are stable

    // Test the default display, which requires reverting the test context disabling Python downloads
    uv_snapshot!(context.filters(), context.python_list().arg("3.10").env_remove(EnvVars::UV_PYTHON_DOWNLOADS), @"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.10.19-[PLATFORM]    <download available>
    pypy-3.10.16-[PLATFORM]       <download available>
    graalpy-3.10.0-[PLATFORM]     <download available>

    ----- stderr -----
    ");

    // Show patch versions
    uv_snapshot!(context.filters(), context.python_list().arg("3.10").arg("--all-versions").env_remove(EnvVars::UV_PYTHON_DOWNLOADS), @"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.10.19-[PLATFORM]    <download available>
    cpython-3.10.18-[PLATFORM]    <download available>
    cpython-3.10.17-[PLATFORM]    <download available>
    cpython-3.10.16-[PLATFORM]    <download available>
    cpython-3.10.15-[PLATFORM]    <download available>
    cpython-3.10.14-[PLATFORM]    <download available>
    cpython-3.10.13-[PLATFORM]    <download available>
    cpython-3.10.12-[PLATFORM]    <download available>
    cpython-3.10.11-[PLATFORM]    <download available>
    cpython-3.10.9-[PLATFORM]     <download available>
    cpython-3.10.8-[PLATFORM]     <download available>
    cpython-3.10.7-[PLATFORM]     <download available>
    cpython-3.10.6-[PLATFORM]     <download available>
    cpython-3.10.5-[PLATFORM]     <download available>
    cpython-3.10.4-[PLATFORM]     <download available>
    cpython-3.10.3-[PLATFORM]     <download available>
    cpython-3.10.2-[PLATFORM]     <download available>
    cpython-3.10.0-[PLATFORM]     <download available>
    pypy-3.10.16-[PLATFORM]       <download available>
    pypy-3.10.14-[PLATFORM]       <download available>
    pypy-3.10.13-[PLATFORM]       <download available>
    pypy-3.10.12-[PLATFORM]       <download available>
    graalpy-3.10.0-[PLATFORM]     <download available>

    ----- stderr -----
    ");
}

#[test]
#[cfg(feature = "python-managed")]
fn python_list_downloads_installed() {
    use assert_cmd::assert::OutputAssertExt;

    let context: TestContext = TestContext::new_with_versions(&[])
        .with_filtered_python_keys()
        .with_filtered_python_install_bin()
        .with_filtered_python_names()
        .with_managed_python_dirs();

    // We do not test showing all interpreters — as it differs per platform
    // Instead, we choose a Python version where our available distributions are stable

    // First, the download is shown as available
    uv_snapshot!(context.filters(), context.python_list().arg("3.10").env_remove(EnvVars::UV_PYTHON_DOWNLOADS), @"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.10.19-[PLATFORM]    <download available>
    pypy-3.10.16-[PLATFORM]       <download available>
    graalpy-3.10.0-[PLATFORM]     <download available>

    ----- stderr -----
    ");

    // TODO(zanieb): It'd be nice to test `--show-urls` here too but we need special filtering for
    // the URL

    // But not if `--only-installed` is used
    uv_snapshot!(context.filters(), context.python_list().arg("3.10").arg("--only-installed").env_remove(EnvVars::UV_PYTHON_DOWNLOADS), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    ");

    // Install a Python version
    context.python_install().arg("3.10").assert().success();

    // Then, it should be listed as installed instead of available
    uv_snapshot!(context.filters(), context.python_list().arg("3.10").env_remove(EnvVars::UV_PYTHON_DOWNLOADS), @"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.10.19-[PLATFORM]    managed/cpython-3.10.19-[PLATFORM]/[INSTALL-BIN]/[PYTHON]
    pypy-3.10.16-[PLATFORM]       <download available>
    graalpy-3.10.0-[PLATFORM]     <download available>

    ----- stderr -----
    ");

    // But, the display should be reverted if `--only-downloads` is used
    uv_snapshot!(context.filters(), context.python_list().arg("3.10").arg("--only-downloads").env_remove(EnvVars::UV_PYTHON_DOWNLOADS), @"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.10.19-[PLATFORM]    <download available>
    pypy-3.10.16-[PLATFORM]       <download available>
    graalpy-3.10.0-[PLATFORM]     <download available>

    ----- stderr -----
    ");

    // And should not be shown if `--no-managed-python` is used
    uv_snapshot!(context.filters(), context.python_list().arg("3.10").arg("--no-managed-python").env_remove(EnvVars::UV_PYTHON_DOWNLOADS), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    ");
}

#[tokio::test]
async fn python_list_remote_python_downloads_json_url() -> Result<()> {
    let context: TestContext = TestContext::new_with_versions(&[]);
    let server = MockServer::start().await;

    let remote_json = r#"
    {
        "cpython-3.14.0-darwin-aarch64-none": {
            "name": "cpython",
            "arch": {
                "family": "aarch64",
                "variant": null
            },
            "os": "darwin",
            "libc": "none",
            "major": 3,
            "minor": 14,
            "patch": 0,
            "prerelease": "",
            "url": "https://custom.com/cpython-3.14.0-darwin-aarch64-none.tar.gz",
            "sha256": "c3223d5924a0ed0ef5958a750377c362d0957587f896c0f6c635ae4b39e0f337",
            "variant": null,
            "build": "20251028"
        },
        "cpython-3.13.2+freethreaded-linux-powerpc64le-gnu": {
            "name": "cpython",
            "arch": {
                "family": "powerpc64le",
                "variant": null
            },
            "os": "linux",
            "libc": "gnu",
            "major": 3,
            "minor": 13,
            "patch": 2,
            "prerelease": "",
            "url": "https://custom.com/ccpython-3.13.2+freethreaded-linux-powerpc64le-gnu.tar.gz",
            "sha256": "6ae8fa44cb2edf4ab49cff1820b53c40c10349c0f39e11b8cd76ce7f3e7e1def",
            "variant": "freethreaded",
            "build": "20250317"
        }
    }
    "#;
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(remote_json, "application/json"))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/invalid"))
        .respond_with(ResponseTemplate::new(200).set_body_raw("{", "application/json"))
        .mount(&server)
        .await;

    // Test showing all interpreters from the remote JSON URL
    uv_snapshot!(context
        .python_list()
        .env_remove(EnvVars::UV_PYTHON_DOWNLOADS)
        .arg("--all-versions")
        .arg("--all-platforms")
        .arg("--all-arches")
        .arg("--show-urls")
        .arg("--python-downloads-json-url").arg(server.uri()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.14.0-macos-aarch64-none                    https://custom.com/cpython-3.14.0-darwin-aarch64-none.tar.gz
    cpython-3.13.2+freethreaded-linux-powerpc64le-gnu    https://custom.com/ccpython-3.13.2+freethreaded-linux-powerpc64le-gnu.tar.gz

    ----- stderr -----
    ");

    // test invalid URL path
    uv_snapshot!(context.filters(), context
        .python_list()
        .env_remove(EnvVars::UV_PYTHON_DOWNLOADS)
        .arg("--python-downloads-json-url").arg(format!("{}/404", server.uri())), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Error while fetching remote python downloads json from 'http://[LOCALHOST]/404'
      Caused by: Failed to download http://[LOCALHOST]/404
      Caused by: HTTP status client error (404 Not Found) for url (http://[LOCALHOST]/404)
    ");

    // test invalid json
    uv_snapshot!(context.filters(), context
        .python_list()
        .env_remove(EnvVars::UV_PYTHON_DOWNLOADS)
        .arg("--python-downloads-json-url").arg(format!("{}/invalid", server.uri())), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Unable to parse the JSON Python download list at http://[LOCALHOST]/invalid
      Caused by: EOF while parsing an object at line 1 column 1
    ");

    Ok(())
}

#[test]
fn python_list_with_mirrors() {
    let context: TestContext = TestContext::new_with_versions(&[])
        .with_filtered_python_keys()
        .with_collapsed_whitespace()
        // Add filters to normalize file paths in URLs
        .with_filter((
            r"(https://mirror\.example\.com/).*".to_string(),
            "$1[FILE-PATH]".to_string(),
        ))
        .with_filter((
            r"(https://python-mirror\.example\.com/).*".to_string(),
            "$1[FILE-PATH]".to_string(),
        ))
        .with_filter((
            r"(https://pypy-mirror\.example\.com/).*".to_string(),
            "$1[FILE-PATH]".to_string(),
        ))
        .with_filter((
            r"(https://github\.com/astral-sh/python-build-standalone/releases/download/).*"
                .to_string(),
            "$1[FILE-PATH]".to_string(),
        ))
        .with_filter((
            r"(https://downloads\.python\.org/pypy/).*".to_string(),
            "$1[FILE-PATH]".to_string(),
        ))
        .with_filter((
            r"(https://github\.com/oracle/graalpython/releases/download/).*".to_string(),
            "$1[FILE-PATH]".to_string(),
        ));

    // Test with UV_PYTHON_INSTALL_MIRROR environment variable - verify mirror URL is used
    uv_snapshot!(context.filters(), context.python_list()
        .arg("cpython@3.10.19")
        .arg("--show-urls")
        .env(EnvVars::UV_PYTHON_INSTALL_MIRROR, "https://mirror.example.com")
        .env_remove(EnvVars::UV_PYTHON_DOWNLOADS), @"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.10.19-[PLATFORM] https://mirror.example.com/[FILE-PATH]

    ----- stderr -----
    ");

    // Test with UV_PYPY_INSTALL_MIRROR environment variable - verify PyPy mirror URL is used
    uv_snapshot!(context.filters(), context.python_list()
        .arg("pypy@3.10")
        .arg("--show-urls")
        .env(EnvVars::UV_PYPY_INSTALL_MIRROR, "https://pypy-mirror.example.com")
        .env_remove(EnvVars::UV_PYTHON_DOWNLOADS), @"
    success: true
    exit_code: 0
    ----- stdout -----
    pypy-3.10.16-[PLATFORM] https://pypy-mirror.example.com/[FILE-PATH]

    ----- stderr -----
    ");

    // Test with both mirror environment variables set
    uv_snapshot!(context.filters(), context.python_list()
        .arg("3.10")
        .arg("--show-urls")
        .env(EnvVars::UV_PYTHON_INSTALL_MIRROR, "https://python-mirror.example.com")
        .env(EnvVars::UV_PYPY_INSTALL_MIRROR, "https://pypy-mirror.example.com")
        .env_remove(EnvVars::UV_PYTHON_DOWNLOADS), @"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.10.19-[PLATFORM] https://python-mirror.example.com/[FILE-PATH]
    pypy-3.10.16-[PLATFORM] https://pypy-mirror.example.com/[FILE-PATH]
    graalpy-3.10.0-[PLATFORM] https://github.com/oracle/graalpython/releases/download/[FILE-PATH]

    ----- stderr -----
    ");

    // Test without mirrors - verify default URLs are used
    uv_snapshot!(context.filters(), context.python_list()
        .arg("3.10")
        .arg("--show-urls")
        .env_remove(EnvVars::UV_PYTHON_DOWNLOADS), @"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.10.19-[PLATFORM] https://github.com/astral-sh/python-build-standalone/releases/download/[FILE-PATH]
    pypy-3.10.16-[PLATFORM] https://downloads.python.org/pypy/[FILE-PATH]
    graalpy-3.10.0-[PLATFORM] https://github.com/oracle/graalpython/releases/download/[FILE-PATH]

    ----- stderr -----
    ");
}

/// Test `uv python list` with remote NDJSON metadata using the preview feature.
///
/// This tests the streaming NDJSON functionality with early-exit behavior.
#[tokio::test]
async fn python_list_remote_ndjson_with_preview() -> Result<()> {
    let context: TestContext = TestContext::new_with_versions(&[]);
    let server = MockServer::start().await;

    // NDJSON format: one JSON object per line, each representing a Python version
    // with all its artifacts. Note: versions are ordered newest first to enable
    // early-exit during streaming.
    let ndjson_content = r#"{"version":"3.14.0","artifacts":[{"url":"https://custom.com/cpython-3.14.0-darwin-aarch64.tar.gz","platform":"aarch64-apple-darwin","sha256":"abc123","variant":"install_only"}]}
{"version":"3.13.2","artifacts":[{"url":"https://custom.com/cpython-3.13.2-darwin-aarch64.tar.gz","platform":"aarch64-apple-darwin","sha256":"def456","variant":"install_only"},{"url":"https://custom.com/cpython-3.13.2+freethreaded-darwin-aarch64.tar.gz","platform":"aarch64-apple-darwin","sha256":"ghi789","variant":"freethreaded+pgo+lto"}]}
{"version":"3.12.8","artifacts":[{"url":"https://custom.com/cpython-3.12.8-darwin-aarch64.tar.gz","platform":"aarch64-apple-darwin","sha256":"jkl012","variant":"install_only"}]}
"#;

    Mock::given(method("GET"))
        .and(path("/versions.ndjson"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(ndjson_content, "application/x-ndjson"),
        )
        .mount(&server)
        .await;

    // Test listing all versions from NDJSON with preview feature
    uv_snapshot!(context
        .python_list()
        .env_remove(EnvVars::UV_PYTHON_DOWNLOADS)
        .arg("--all-versions")
        .arg("--all-platforms")
        .arg("--all-arches")
        .arg("--show-urls")
        .arg("--preview-features").arg("remote-python-download-metadata")
        .arg("--python-downloads-json-url").arg(format!("{}/versions.ndjson", server.uri())), @"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.14.0-macos-aarch64-none                 https://custom.com/cpython-3.14.0-darwin-aarch64.tar.gz
    cpython-3.13.2-macos-aarch64-none                 https://custom.com/cpython-3.13.2-darwin-aarch64.tar.gz
    cpython-3.13.2+freethreaded-macos-aarch64-none    https://custom.com/cpython-3.13.2+freethreaded-darwin-aarch64.tar.gz
    cpython-3.12.8-macos-aarch64-none                 https://custom.com/cpython-3.12.8-darwin-aarch64.tar.gz

    ----- stderr -----
    ");

    Ok(())
}

/// Test `uv python list` with streaming limit (without --all-versions).
///
/// When not using --all-versions, the streaming implementation limits the
/// number of downloads fetched for better performance.
#[tokio::test]
async fn python_list_remote_ndjson_with_limit() -> Result<()> {
    let context: TestContext = TestContext::new_with_versions(&[]);
    let server = MockServer::start().await;

    // Create NDJSON with many versions to test the limit behavior
    // The limit is 50 matching downloads, which translates to roughly 50 versions
    // for a single platform
    let mut ndjson_lines = Vec::new();
    for minor in (7..=14).rev() {
        for patch in (0..=5).rev() {
            let version = format!("3.{minor}.{patch}");
            let line = format!(
                r#"{{"version":"{}","artifacts":[{{"url":"https://custom.com/cpython-{}-darwin-aarch64.tar.gz","platform":"aarch64-apple-darwin","sha256":"hash{}{}","variant":"install_only"}}]}}"#,
                version, version, minor, patch
            );
            ndjson_lines.push(line);
        }
    }
    let ndjson_content = ndjson_lines.join("\n") + "\n";

    Mock::given(method("GET"))
        .and(path("/many-versions.ndjson"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(ndjson_content, "application/x-ndjson"),
        )
        .mount(&server)
        .await;

    // Test listing versions with default limit (no --all-versions)
    // This should use streaming with early-exit after the limit is reached
    let output = context
        .python_list()
        .env_remove(EnvVars::UV_PYTHON_DOWNLOADS)
        .arg("--all-platforms")
        .arg("--all-arches")
        .arg("--preview-features")
        .arg("remote-python-download-metadata")
        .arg("--python-downloads-json-url")
        .arg(format!("{}/many-versions.ndjson", server.uri()))
        .output()?;

    // The output should be limited (not all 48 versions)
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line_count = stdout.lines().count();

    // With a limit of 50 downloads, we should see at most 50 versions
    assert!(
        line_count <= 50,
        "Expected at most 50 versions due to limit, got {line_count}"
    );
    assert!(
        line_count > 0,
        "Expected some versions to be listed, got {line_count}"
    );

    Ok(())
}

/// Test `uv python list` caching with delta fetching.
///
/// This test verifies that:
/// 1. First call fetches full content and caches it
/// 2. Second call with same content uses cache (no GET request)
/// 3. Third call with new content fetches only the delta using Range request
#[tokio::test]
async fn python_list_remote_ndjson_caching() -> Result<()> {
    let context: TestContext = TestContext::new_with_versions(&[]);
    let server = MockServer::start().await;

    // Initial content
    let initial_content = r#"{"version":"3.13.2","artifacts":[{"url":"https://custom.com/cpython-3.13.2-darwin-aarch64.tar.gz","platform":"aarch64-apple-darwin","sha256":"def456","variant":"install_only"}]}
{"version":"3.12.8","artifacts":[{"url":"https://custom.com/cpython-3.12.8-darwin-aarch64.tar.gz","platform":"aarch64-apple-darwin","sha256":"jkl012","variant":"install_only"}]}
"#;

    // Mock HEAD request returning content length
    let initial_len = initial_content.len();
    Mock::given(method("HEAD"))
        .and(path("/versions.ndjson"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Length", initial_len.to_string())
                .insert_header("Accept-Ranges", "bytes"),
        )
        .expect(2) // Called twice: first call + second call (cache validation)
        .mount(&server)
        .await;

    // Mock GET request for full content
    Mock::given(method("GET"))
        .and(path("/versions.ndjson"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(initial_content, "application/x-ndjson")
                .insert_header("Content-Length", initial_len.to_string()),
        )
        .expect(1) // Only called once on first fetch
        .mount(&server)
        .await;

    // First call - should fetch full content
    let output1 = context
        .python_list()
        .env_remove(EnvVars::UV_PYTHON_DOWNLOADS)
        .arg("--all-versions")
        .arg("--all-platforms")
        .arg("--all-arches")
        .arg("--show-urls")
        .arg("--preview-features")
        .arg("remote-python-download-metadata")
        .arg("--python-downloads-json-url")
        .arg(format!("{}/versions.ndjson", server.uri()))
        .output()?;

    assert!(output1.status.success(), "First call should succeed");

    // Second call - should use cache (same content length from HEAD)
    let output2 = context
        .python_list()
        .env_remove(EnvVars::UV_PYTHON_DOWNLOADS)
        .arg("--all-versions")
        .arg("--all-platforms")
        .arg("--all-arches")
        .arg("--show-urls")
        .arg("--preview-features")
        .arg("remote-python-download-metadata")
        .arg("--python-downloads-json-url")
        .arg(format!("{}/versions.ndjson", server.uri()))
        .output()?;

    assert!(output2.status.success(), "Second call should succeed");
    assert_eq!(
        String::from_utf8_lossy(&output1.stdout),
        String::from_utf8_lossy(&output2.stdout),
        "Both calls should produce the same output"
    );

    // Verify mock expectations (HEAD called twice, GET called once)
    // The mock server automatically verifies the `expect` counts on drop

    Ok(())
}

/// Test `uv python list` delta fetching with Range requests.
///
/// This test verifies that when new content is prepended to the NDJSON file,
/// only the new bytes are fetched using HTTP Range requests.
#[tokio::test]
async fn python_list_remote_ndjson_delta_fetch() -> Result<()> {
    let context: TestContext = TestContext::new_with_versions(&[]);
    let server = MockServer::start().await;

    // Initial content (will be cached first)
    let initial_content = r#"{"version":"3.12.8","artifacts":[{"url":"https://custom.com/cpython-3.12.8-darwin-aarch64.tar.gz","platform":"aarch64-apple-darwin","sha256":"jkl012","variant":"install_only"}]}
"#;

    // New content prepended (newer version at the start)
    let new_line = r#"{"version":"3.13.2","artifacts":[{"url":"https://custom.com/cpython-3.13.2-darwin-aarch64.tar.gz","platform":"aarch64-apple-darwin","sha256":"def456","variant":"install_only"}]}
"#;
    let updated_content = format!("{new_line}{initial_content}");

    let initial_len = initial_content.len();
    let updated_len = updated_content.len();
    let delta_len = new_line.len();

    // First HEAD request - initial length
    Mock::given(method("HEAD"))
        .and(path("/versions.ndjson"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Length", initial_len.to_string())
                .insert_header("Accept-Ranges", "bytes"),
        )
        .expect(1)
        .mount(&server)
        .await;

    // First GET request - full initial content
    Mock::given(method("GET"))
        .and(path("/versions.ndjson"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(initial_content, "application/x-ndjson")
                .insert_header("Content-Length", initial_len.to_string()),
        )
        .expect(1)
        .mount(&server)
        .await;

    // First call - fetch full content
    let output1 = context
        .python_list()
        .env_remove(EnvVars::UV_PYTHON_DOWNLOADS)
        .arg("--all-versions")
        .arg("--all-platforms")
        .arg("--all-arches")
        .arg("--show-urls")
        .arg("--preview-features")
        .arg("remote-python-download-metadata")
        .arg("--python-downloads-json-url")
        .arg(format!("{}/versions.ndjson", server.uri()))
        .output()?;

    assert!(output1.status.success(), "First call should succeed");

    // Drop the old mocks and set up new ones for updated content
    drop(server);
    let server = MockServer::start().await;

    // Second HEAD request - updated length (larger)
    Mock::given(method("HEAD"))
        .and(path("/versions.ndjson"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Length", updated_len.to_string())
                .insert_header("Accept-Ranges", "bytes"),
        )
        .expect(1)
        .mount(&server)
        .await;

    // Range request for delta bytes (new content at start)
    Mock::given(method("GET"))
        .and(path("/versions.ndjson"))
        .and(wiremock::matchers::header(
            "Range",
            format!("bytes=0-{}", delta_len - 1),
        ))
        .respond_with(
            ResponseTemplate::new(206) // Partial Content
                .set_body_raw(new_line, "application/x-ndjson")
                .insert_header(
                    "Content-Range",
                    format!("bytes 0-{}/{}", delta_len - 1, updated_len),
                ),
        )
        .expect(1)
        .mount(&server)
        .await;

    // Second call - should fetch only delta
    let output2 = context
        .python_list()
        .env_remove(EnvVars::UV_PYTHON_DOWNLOADS)
        .arg("--all-versions")
        .arg("--all-platforms")
        .arg("--all-arches")
        .arg("--show-urls")
        .arg("--preview-features")
        .arg("remote-python-download-metadata")
        .arg("--python-downloads-json-url")
        .arg(format!("{}/versions.ndjson", server.uri()))
        .output()?;

    assert!(output2.status.success(), "Second call should succeed");

    // Second call should show both versions (cached + delta)
    let stdout2 = String::from_utf8_lossy(&output2.stdout);
    assert!(
        stdout2.contains("3.13.2"),
        "Output should contain new version 3.13.2"
    );
    assert!(
        stdout2.contains("3.12.8"),
        "Output should contain cached version 3.12.8"
    );

    Ok(())
}
