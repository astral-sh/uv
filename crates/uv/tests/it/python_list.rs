use uv_platform::{Arch, Os};
use uv_static::EnvVars;

use anyhow::Result;
use uv_test::uv_snapshot;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

#[test]
fn python_list() {
    let mut context = uv_test::test_context_with_versions!(&["3.11", "3.12"])
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
    let context = uv_test::test_context_with_versions!(&["3.11", "3.12"])
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
    let context = uv_test::test_context_with_versions!(&["3.11", "3.12"])
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
    let context = uv_test::test_context_with_versions!(&["3.12"])
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
    let context = uv_test::test_context_with_versions!(&["3.11", "3.12"])
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
    let context = uv_test::test_context_with_versions!(&[]).with_filtered_python_keys();

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
#[cfg(feature = "test-python-managed")]
fn python_list_downloads_installed() {
    use assert_cmd::assert::OutputAssertExt;

    let context = uv_test::test_context_with_versions!(&[])
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
    cpython-3.10.19-[PLATFORM]    managed/cpython-3.10-[PLATFORM]/[INSTALL-BIN]/[PYTHON]
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
    let context = uv_test::test_context_with_versions!(&[]);
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
    let context = uv_test::test_context_with_versions!(&[])
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
