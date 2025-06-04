use uv_python::platform::{Arch, Os};
use uv_static::EnvVars;

use crate::common::{TestContext, uv_snapshot};

#[test]
fn python_list() {
    let mut context: TestContext = TestContext::new_with_versions(&["3.11", "3.12"])
        .with_filtered_python_symlinks()
        .with_filtered_python_keys()
        .with_collapsed_whitespace();

    uv_snapshot!(context.filters(), context.python_list().env(EnvVars::UV_TEST_PYTHON_PATH, ""), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    ");

    // We show all interpreters
    uv_snapshot!(context.filters(), context.python_list(), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.12.[X]-[PLATFORM] [PYTHON-3.12]
    cpython-3.11.[X]-[PLATFORM] [PYTHON-3.11]

    ----- stderr -----
    ");

    // Request Python 3.12
    uv_snapshot!(context.filters(), context.python_list().arg("3.12"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.12.[X]-[PLATFORM] [PYTHON-3.12]

    ----- stderr -----
    ");

    // Request Python 3.11
    uv_snapshot!(context.filters(), context.python_list().arg("3.11"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.11.[X]-[PLATFORM] [PYTHON-3.11]

    ----- stderr -----
    ");

    // Request CPython
    uv_snapshot!(context.filters(), context.python_list().arg("cpython"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.12.[X]-[PLATFORM] [PYTHON-3.12]
    cpython-3.11.[X]-[PLATFORM] [PYTHON-3.11]

    ----- stderr -----
    ");

    // Request CPython 3.12
    uv_snapshot!(context.filters(), context.python_list().arg("cpython@3.12"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.12.[X]-[PLATFORM] [PYTHON-3.12]

    ----- stderr -----
    ");

    // Request CPython 3.12 via partial key syntax
    uv_snapshot!(context.filters(), context.python_list().arg("cpython-3.12"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.12.[X]-[PLATFORM] [PYTHON-3.12]

    ----- stderr -----
    ");

    // Request CPython 3.12 for the current platform
    let os = Os::from_env();
    let arch = Arch::from_env();

    uv_snapshot!(context.filters(), context.python_list().arg(format!("cpython-3.12-{os}-{arch}")), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.12.[X]-[PLATFORM] [PYTHON-3.12]

    ----- stderr -----
    ");

    // Request PyPy (which should be missing)
    uv_snapshot!(context.filters(), context.python_list().arg("pypy"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    ");

    // Swap the order of the Python versions
    context.python_versions.reverse();

    uv_snapshot!(context.filters(), context.python_list(), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.12.[X]-[PLATFORM] [PYTHON-3.12]
    cpython-3.11.[X]-[PLATFORM] [PYTHON-3.11]

    ----- stderr -----
    ");

    // Request Python 3.11
    uv_snapshot!(context.filters(), context.python_list().arg("3.11"), @r"
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
    uv_snapshot!(context.filters(), context.python_pin().arg("3.12"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Pinned `.python-version` to `3.12`

    ----- stderr -----
    "###);

    // The pin should not affect the listing
    uv_snapshot!(context.filters(), context.python_list(), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.12.[X]-[PLATFORM] [PYTHON-3.12]
    cpython-3.11.[X]-[PLATFORM] [PYTHON-3.11]

    ----- stderr -----
    ");

    // So `--no-config` has no effect
    uv_snapshot!(context.filters(), context.python_list().arg("--no-config"), @r"
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
    uv_snapshot!(context.filters(), context.venv().arg("--python").arg("3.12").arg("-q"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    "###);

    // We should not display the virtual environment
    uv_snapshot!(context.filters(), context.python_list(), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.12.[X]-[PLATFORM] [PYTHON-3.12]
    cpython-3.11.[X]-[PLATFORM] [PYTHON-3.11]

    ----- stderr -----
    ");

    // Same if the `VIRTUAL_ENV` is not set (the test context includes it by default)
    uv_snapshot!(context.filters(), context.python_list().env_remove(EnvVars::VIRTUAL_ENV), @r"
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
    uv_snapshot!(context.filters(), context.python_list().arg("3.6"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Invalid version request: Python <3.7 is not supported but 3.6 was requested.
    ");

    // Request a low version with a patch
    uv_snapshot!(context.filters(), context.python_list().arg("3.6.9"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Invalid version request: Python <3.7 is not supported but 3.6.9 was requested.
    ");

    // Request a really low version
    uv_snapshot!(context.filters(), context.python_list().arg("2.6"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Invalid version request: Python <3.7 is not supported but 2.6 was requested.
    ");

    // Request a really low version with a patch
    uv_snapshot!(context.filters(), context.python_list().arg("2.6.8"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Invalid version request: Python <3.7 is not supported but 2.6.8 was requested.
    ");

    // Request a future version
    uv_snapshot!(context.filters(), context.python_list().arg("4.2"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    ");

    // Request a low version with a range
    uv_snapshot!(context.filters(), context.python_list().arg("<3.0"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    ");

    // Request free-threaded Python on unsupported version
    uv_snapshot!(context.filters(), context.python_list().arg("3.12t"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Invalid version request: Python <3.13 does not support free-threading but 3.12t was requested.
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

    uv_snapshot!(context.filters(), context.python_list().env(EnvVars::UV_TEST_PYTHON_PATH, &path), @r"
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

        uv_snapshot!(context.filters(), context.python_list().env(EnvVars::UV_TEST_PYTHON_PATH, &path), @r"
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

        uv_snapshot!(context.filters(), context.python_list().env(EnvVars::UV_TEST_PYTHON_PATH, &path), @r"
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
    uv_snapshot!(context.filters(), context.python_list().arg("3.10").env_remove("UV_PYTHON_DOWNLOADS"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.10.17-[PLATFORM]    <download available>
    pypy-3.10.16-[PLATFORM]       <download available>
    graalpy-3.10.0-[PLATFORM]     <download available>

    ----- stderr -----
    ");

    // Show patch versions
    uv_snapshot!(context.filters(), context.python_list().arg("3.10").arg("--all-versions").env_remove("UV_PYTHON_DOWNLOADS"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
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
        .with_filtered_python_names()
        .with_filtered_python_install_bin()
        .with_managed_python_dirs();

    // We do not test showing all interpreters — as it differs per platform
    // Instead, we choose a Python version where our available distributions are stable

    // First, the download is shown as available
    uv_snapshot!(context.filters(), context.python_list().arg("3.10").env_remove("UV_PYTHON_DOWNLOADS"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.10.17-[PLATFORM]    <download available>
    pypy-3.10.16-[PLATFORM]       <download available>
    graalpy-3.10.0-[PLATFORM]     <download available>

    ----- stderr -----
    ");

    // TODO(zanieb): It'd be nice to test `--show-urls` here too but we need special filtering for
    // the URL

    // But not if `--only-installed` is used
    uv_snapshot!(context.filters(), context.python_list().arg("3.10").arg("--only-installed").env_remove("UV_PYTHON_DOWNLOADS"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    ");

    // Install a Python version
    context.python_install().arg("3.10").assert().success();

    // Then, it should be listed as installed instead of available
    uv_snapshot!(context.filters(), context.python_list().arg("3.10").env_remove("UV_PYTHON_DOWNLOADS"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.10.17-[PLATFORM]    managed/cpython-3.10.17-[PLATFORM]/[INSTALL-BIN]/python
    pypy-3.10.16-[PLATFORM]       <download available>
    graalpy-3.10.0-[PLATFORM]     <download available>

    ----- stderr -----
    ");

    // But, the display should be reverted if `--only-downloads` is used
    uv_snapshot!(context.filters(), context.python_list().arg("3.10").arg("--only-downloads").env_remove("UV_PYTHON_DOWNLOADS"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.10.17-[PLATFORM]    <download available>
    pypy-3.10.16-[PLATFORM]       <download available>
    graalpy-3.10.0-[PLATFORM]     <download available>

    ----- stderr -----
    ");

    // And should not be shown if `--no-managed-python` is used
    uv_snapshot!(context.filters(), context.python_list().arg("3.10").arg("--no-managed-python").env_remove("UV_PYTHON_DOWNLOADS"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    ");
}
