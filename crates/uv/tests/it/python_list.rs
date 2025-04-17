#![allow(dead_code, unused_imports)]

use anyhow::Result;
use assert_fs::prelude::{FileWriteBin, PathChild};
use uv_python::platform::{Arch, Libc, Os};
use uv_static::EnvVars;

use crate::common::{uv_snapshot, TestContext};

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

    // Show only the installed interpreters
    uv_snapshot!(context.filters(), context.python_list().arg("--only-installed"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.12.[X]-[PLATFORM] [PYTHON-3.12]
    cpython-3.11.[X]-[PLATFORM] [PYTHON-3.11]

    ----- stderr -----
    "#);

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

/// A subset versions of the `uv-python/download-metadata.json` file for `uv python list` tests.
const PYTHON_DOWNLOADS_JSON: &[u8] = include_bytes!("python-downloads-metadata.json");

#[test]
#[cfg(unix)] // Windows does not have aarch64 yet, its output is different
fn python_list_downloads() -> Result<()> {
    let context: TestContext = TestContext::new("3.11").with_collapsed_whitespace();

    let downloads_json = context.temp_dir.child("python-downloads.json");
    downloads_json.write_binary(PYTHON_DOWNLOADS_JSON)?;

    let python_list = || {
        let mut cmd = context.python_list();
        cmd.arg("--only-downloads")
            .env(EnvVars::UV_PYTHON_DOWNLOADS, "true")
            .env(EnvVars::UV_PYTHON_DOWNLOADS_JSON_URL, &*downloads_json);
        cmd
    };

    // Regex to match the current os, arch and libc
    let platform = format!(
        "((?:cpython|pypy)-(?:[^-]+))-{}-{}-{}",
        Os::from_env(),
        Arch::from_env(),
        Libc::from_env()?
    );
    let platform_filters = [(platform.as_ref(), "$1-[OS]-[ARCH]-[LIBC]")]
        .into_iter()
        .chain(context.filters())
        .collect::<Vec<_>>();

    // Regex to match current os and libc, but any arch
    let platform_any_arch = format!(
        "((?:cpython|pypy)-(?:[^-]+))-{}-([^-]+)-{}",
        Os::from_env(),
        Libc::from_env()?
    );
    let platform_any_arch_filters = [(platform_any_arch.as_ref(), "$1-[OS]-$2-[LIBC]")]
        .into_iter()
        .chain(context.filters())
        .collect::<Vec<_>>();

    // `--only-downloads` only shows available downloads for the current platform.
    uv_snapshot!(platform_filters, python_list(), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.14.0a6-[OS]-[ARCH]-[LIBC] <download available>
    cpython-3.13.3-[OS]-[ARCH]-[LIBC] <download available>
    cpython-3.13.3+freethreaded-[OS]-[ARCH]-[LIBC] <download available>
    cpython-3.12.10-[OS]-[ARCH]-[LIBC] <download available>
    pypy-3.11.[X]-[OS]-[ARCH]-[LIBC] <download available>
    pypy-3.10.16-[OS]-[ARCH]-[LIBC] <download available>

    ----- stderr -----
    "#);

    // `--all-versions` shows all versions for current platform, including old patch versions.
    uv_snapshot!(platform_filters, python_list().arg("--all-versions"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.14.0a6-[OS]-[ARCH]-[LIBC] <download available>
    cpython-3.13.3-[OS]-[ARCH]-[LIBC] <download available>
    cpython-3.13.3+freethreaded-[OS]-[ARCH]-[LIBC] <download available>
    cpython-3.13.2-[OS]-[ARCH]-[LIBC] <download available>
    cpython-3.12.10-[OS]-[ARCH]-[LIBC] <download available>
    pypy-3.11.[X]-[OS]-[ARCH]-[LIBC] <download available>
    pypy-3.10.16-[OS]-[ARCH]-[LIBC] <download available>

    ----- stderr -----
    "#);

    // `--all-arches` show all architectures for the current platform, with non-latest patch versions hidden.
    uv_snapshot!(platform_any_arch_filters, python_list().arg("--all-arches"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.14.0a6-[OS]-x86_64-[LIBC] <download available>
    cpython-3.14.0a6-[OS]-aarch64-[LIBC] <download available>
    cpython-3.13.3-[OS]-x86_64-[LIBC] <download available>
    cpython-3.13.3+freethreaded-[OS]-x86_64-[LIBC] <download available>
    cpython-3.13.3-[OS]-aarch64-[LIBC] <download available>
    cpython-3.13.3+freethreaded-[OS]-aarch64-[LIBC] <download available>
    cpython-3.12.10-[OS]-x86_64-[LIBC] <download available>
    cpython-3.12.10-[OS]-aarch64-[LIBC] <download available>
    pypy-3.11.[X]-[OS]-x86_64-[LIBC] <download available>
    pypy-3.11.[X]-[OS]-aarch64-[LIBC] <download available>
    pypy-3.10.16-[OS]-x86_64-[LIBC] <download available>
    pypy-3.10.16-[OS]-aarch64-[LIBC] <download available>

    ----- stderr -----
    "#);

    // --all-versions && --all-arches
    uv_snapshot!(platform_any_arch_filters, python_list().arg("--all-versions").arg("--all-arches"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.14.0a6-[OS]-x86_64-[LIBC] <download available>
    cpython-3.14.0a6-[OS]-aarch64-[LIBC] <download available>
    cpython-3.13.3-[OS]-x86_64-[LIBC] <download available>
    cpython-3.13.3+freethreaded-[OS]-x86_64-[LIBC] <download available>
    cpython-3.13.3-[OS]-aarch64-[LIBC] <download available>
    cpython-3.13.3+freethreaded-[OS]-aarch64-[LIBC] <download available>
    cpython-3.13.2-[OS]-x86_64-[LIBC] <download available>
    cpython-3.13.2-[OS]-aarch64-[LIBC] <download available>
    cpython-3.12.10-[OS]-x86_64-[LIBC] <download available>
    cpython-3.12.10-[OS]-aarch64-[LIBC] <download available>
    pypy-3.11.[X]-[OS]-x86_64-[LIBC] <download available>
    pypy-3.11.[X]-[OS]-aarch64-[LIBC] <download available>
    pypy-3.10.16-[OS]-x86_64-[LIBC] <download available>
    pypy-3.10.16-[OS]-aarch64-[LIBC] <download available>

    ----- stderr -----
    "#);

    // `--all-platforms` shows all platforms, its output is independent of the current platform.
    uv_snapshot!(context.filters(), python_list().arg("--all-platforms"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.14.0a6-windows-x86_64-none <download available>
    cpython-3.14.0a6-windows-x86-none <download available>
    cpython-3.14.0a6-macos-x86_64-none <download available>
    cpython-3.14.0a6-macos-aarch64-none <download available>
    cpython-3.14.0a6-linux-x86_64-gnu <download available>
    cpython-3.14.0a6-linux-aarch64-gnu <download available>
    cpython-3.13.3-windows-x86_64-none <download available>
    cpython-3.13.3+freethreaded-windows-x86_64-none <download available>
    cpython-3.13.3-windows-x86-none <download available>
    cpython-3.13.3+freethreaded-windows-x86-none <download available>
    cpython-3.13.3-macos-x86_64-none <download available>
    cpython-3.13.3+freethreaded-macos-x86_64-none <download available>
    cpython-3.13.3-macos-aarch64-none <download available>
    cpython-3.13.3+freethreaded-macos-aarch64-none <download available>
    cpython-3.13.3-linux-x86_64-gnu <download available>
    cpython-3.13.3+freethreaded-linux-x86_64-gnu <download available>
    cpython-3.13.3-linux-aarch64-gnu <download available>
    cpython-3.13.3+freethreaded-linux-aarch64-gnu <download available>
    cpython-3.12.10-windows-x86_64-none <download available>
    cpython-3.12.10-windows-x86-none <download available>
    cpython-3.12.10-macos-x86_64-none <download available>
    cpython-3.12.10-macos-aarch64-none <download available>
    cpython-3.12.10-linux-x86_64-gnu <download available>
    cpython-3.12.10-linux-aarch64-gnu <download available>
    pypy-3.11.[X]-windows-x86_64-none <download available>
    pypy-3.11.[X]-macos-x86_64-none <download available>
    pypy-3.11.[X]-macos-aarch64-none <download available>
    pypy-3.11.[X]-linux-x86_64-gnu <download available>
    pypy-3.11.[X]-linux-aarch64-gnu <download available>
    pypy-3.10.16-windows-x86_64-none <download available>
    pypy-3.10.16-macos-x86_64-none <download available>
    pypy-3.10.16-macos-aarch64-none <download available>
    pypy-3.10.16-linux-x86_64-gnu <download available>
    pypy-3.10.16-linux-aarch64-gnu <download available>

    ----- stderr -----
    "#);

    // --all-platforms && --all-versions
    uv_snapshot!(context.filters(), python_list().arg("--all-platforms").arg("--all-versions"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.14.0a6-windows-x86_64-none <download available>
    cpython-3.14.0a6-windows-x86-none <download available>
    cpython-3.14.0a6-macos-x86_64-none <download available>
    cpython-3.14.0a6-macos-aarch64-none <download available>
    cpython-3.14.0a6-linux-x86_64-gnu <download available>
    cpython-3.14.0a6-linux-aarch64-gnu <download available>
    cpython-3.13.3-windows-x86_64-none <download available>
    cpython-3.13.3+freethreaded-windows-x86_64-none <download available>
    cpython-3.13.3-windows-x86-none <download available>
    cpython-3.13.3+freethreaded-windows-x86-none <download available>
    cpython-3.13.3-macos-x86_64-none <download available>
    cpython-3.13.3+freethreaded-macos-x86_64-none <download available>
    cpython-3.13.3-macos-aarch64-none <download available>
    cpython-3.13.3+freethreaded-macos-aarch64-none <download available>
    cpython-3.13.3-linux-x86_64-gnu <download available>
    cpython-3.13.3+freethreaded-linux-x86_64-gnu <download available>
    cpython-3.13.3-linux-aarch64-gnu <download available>
    cpython-3.13.3+freethreaded-linux-aarch64-gnu <download available>
    cpython-3.13.2-windows-x86_64-none <download available>
    cpython-3.13.2-windows-x86-none <download available>
    cpython-3.13.2-macos-x86_64-none <download available>
    cpython-3.13.2-macos-aarch64-none <download available>
    cpython-3.13.2-linux-x86_64-gnu <download available>
    cpython-3.13.2-linux-aarch64-gnu <download available>
    cpython-3.12.10-windows-x86_64-none <download available>
    cpython-3.12.10-windows-x86-none <download available>
    cpython-3.12.10-macos-x86_64-none <download available>
    cpython-3.12.10-macos-aarch64-none <download available>
    cpython-3.12.10-linux-x86_64-gnu <download available>
    cpython-3.12.10-linux-aarch64-gnu <download available>
    pypy-3.11.[X]-windows-x86_64-none <download available>
    pypy-3.11.[X]-macos-x86_64-none <download available>
    pypy-3.11.[X]-macos-aarch64-none <download available>
    pypy-3.11.[X]-linux-x86_64-gnu <download available>
    pypy-3.11.[X]-linux-aarch64-gnu <download available>
    pypy-3.10.16-windows-x86_64-none <download available>
    pypy-3.10.16-macos-x86_64-none <download available>
    pypy-3.10.16-macos-aarch64-none <download available>
    pypy-3.10.16-linux-x86_64-gnu <download available>
    pypy-3.10.16-linux-aarch64-gnu <download available>

    ----- stderr -----
    "#);

    // --all-platforms && --all-arches
    uv_snapshot!(context.filters(), python_list().arg("--all-platforms").arg("--all-arches"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.14.0a6-windows-x86_64-none <download available>
    cpython-3.14.0a6-windows-x86-none <download available>
    cpython-3.14.0a6-macos-x86_64-none <download available>
    cpython-3.14.0a6-macos-aarch64-none <download available>
    cpython-3.14.0a6-linux-x86_64-gnu <download available>
    cpython-3.14.0a6-linux-aarch64-gnu <download available>
    cpython-3.13.3-windows-x86_64-none <download available>
    cpython-3.13.3+freethreaded-windows-x86_64-none <download available>
    cpython-3.13.3-windows-x86-none <download available>
    cpython-3.13.3+freethreaded-windows-x86-none <download available>
    cpython-3.13.3-macos-x86_64-none <download available>
    cpython-3.13.3+freethreaded-macos-x86_64-none <download available>
    cpython-3.13.3-macos-aarch64-none <download available>
    cpython-3.13.3+freethreaded-macos-aarch64-none <download available>
    cpython-3.13.3-linux-x86_64-gnu <download available>
    cpython-3.13.3+freethreaded-linux-x86_64-gnu <download available>
    cpython-3.13.3-linux-aarch64-gnu <download available>
    cpython-3.13.3+freethreaded-linux-aarch64-gnu <download available>
    cpython-3.12.10-windows-x86_64-none <download available>
    cpython-3.12.10-windows-x86-none <download available>
    cpython-3.12.10-macos-x86_64-none <download available>
    cpython-3.12.10-macos-aarch64-none <download available>
    cpython-3.12.10-linux-x86_64-gnu <download available>
    cpython-3.12.10-linux-aarch64-gnu <download available>
    pypy-3.11.[X]-windows-x86_64-none <download available>
    pypy-3.11.[X]-macos-x86_64-none <download available>
    pypy-3.11.[X]-macos-aarch64-none <download available>
    pypy-3.11.[X]-linux-x86_64-gnu <download available>
    pypy-3.11.[X]-linux-aarch64-gnu <download available>
    pypy-3.10.16-windows-x86_64-none <download available>
    pypy-3.10.16-macos-x86_64-none <download available>
    pypy-3.10.16-macos-aarch64-none <download available>
    pypy-3.10.16-linux-x86_64-gnu <download available>
    pypy-3.10.16-linux-aarch64-gnu <download available>

    ----- stderr -----
    "#);

    // --all-platforms && --all-versions  && --all-arches
    uv_snapshot!(context.filters(), python_list().arg("--all-platforms").arg("--all-versions").arg("--all-arches"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.14.0a6-windows-x86_64-none <download available>
    cpython-3.14.0a6-windows-x86-none <download available>
    cpython-3.14.0a6-macos-x86_64-none <download available>
    cpython-3.14.0a6-macos-aarch64-none <download available>
    cpython-3.14.0a6-linux-x86_64-gnu <download available>
    cpython-3.14.0a6-linux-aarch64-gnu <download available>
    cpython-3.13.3-windows-x86_64-none <download available>
    cpython-3.13.3+freethreaded-windows-x86_64-none <download available>
    cpython-3.13.3-windows-x86-none <download available>
    cpython-3.13.3+freethreaded-windows-x86-none <download available>
    cpython-3.13.3-macos-x86_64-none <download available>
    cpython-3.13.3+freethreaded-macos-x86_64-none <download available>
    cpython-3.13.3-macos-aarch64-none <download available>
    cpython-3.13.3+freethreaded-macos-aarch64-none <download available>
    cpython-3.13.3-linux-x86_64-gnu <download available>
    cpython-3.13.3+freethreaded-linux-x86_64-gnu <download available>
    cpython-3.13.3-linux-aarch64-gnu <download available>
    cpython-3.13.3+freethreaded-linux-aarch64-gnu <download available>
    cpython-3.13.2-windows-x86_64-none <download available>
    cpython-3.13.2-windows-x86-none <download available>
    cpython-3.13.2-macos-x86_64-none <download available>
    cpython-3.13.2-macos-aarch64-none <download available>
    cpython-3.13.2-linux-x86_64-gnu <download available>
    cpython-3.13.2-linux-aarch64-gnu <download available>
    cpython-3.12.10-windows-x86_64-none <download available>
    cpython-3.12.10-windows-x86-none <download available>
    cpython-3.12.10-macos-x86_64-none <download available>
    cpython-3.12.10-macos-aarch64-none <download available>
    cpython-3.12.10-linux-x86_64-gnu <download available>
    cpython-3.12.10-linux-aarch64-gnu <download available>
    pypy-3.11.[X]-windows-x86_64-none <download available>
    pypy-3.11.[X]-macos-x86_64-none <download available>
    pypy-3.11.[X]-macos-aarch64-none <download available>
    pypy-3.11.[X]-linux-x86_64-gnu <download available>
    pypy-3.11.[X]-linux-aarch64-gnu <download available>
    pypy-3.10.16-windows-x86_64-none <download available>
    pypy-3.10.16-macos-x86_64-none <download available>
    pypy-3.10.16-macos-aarch64-none <download available>
    pypy-3.10.16-linux-x86_64-gnu <download available>
    pypy-3.10.16-linux-aarch64-gnu <download available>

    ----- stderr -----
    "#);

    // `--show-urls` also shows the download URLs
    uv_snapshot!(context.filters(), python_list().arg("cpython-3.13.3-linux-aarch64-gnu").arg("--show-urls"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython-3.13.3-linux-aarch64-gnu https://github.com/astral-sh/python-build-standalone/releases/download/20250409/cpython-3.13.3%2B20250409-aarch64-unknown-linux-gnu-install_only_stripped.tar.gz

    ----- stderr -----
    "#);

    // `--output-format=json` outputs in JSON format
    uv_snapshot!(context.filters(), python_list().arg("cpython-3.13.3-linux-aarch64-gnu").arg("--output-format=json"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    [{"key":"cpython-3.13.3-linux-aarch64-gnu","version":"3.13.3","version_parts":{"major":3,"minor":13,"patch":3},"path":null,"symlink":null,"url":"https://github.com/astral-sh/python-build-standalone/releases/download/20250409/cpython-3.13.3%2B20250409-aarch64-unknown-linux-gnu-install_only_stripped.tar.gz","os":"linux","variant":"default","implementation":"cpython","arch":"aarch64","libc":"gnu"}]

    ----- stderr -----
    "#);

    Ok(())
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
