use crate::common::{uv_snapshot, TestContext};

#[test]
fn python_install() {
    let context: TestContext = TestContext::new_with_versions(&[]).with_filtered_python_keys();

    // Install the latest version
    uv_snapshot!(context.filters(), context.python_install(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Searching for Python installations
    Installed Python 3.13.0 in [TIME]
     + cpython-3.13.0-[PLATFORM]
    "###);

    // Should be a no-op when already installed
    uv_snapshot!(context.filters(), context.python_install(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Searching for Python installations
    Found: cpython-3.13.0-[PLATFORM]
    Python is already available. Use `uv python install <request>` to install a specific version.
    "###);

    // Similarly, when a requested version is already installed
    uv_snapshot!(context.filters(), context.python_install().arg("3.13"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Searching for Python versions matching: Python 3.13
    Found existing installation for Python 3.13: cpython-3.13.0-[PLATFORM]
    "###);

    // Uninstallation requires an argument
    uv_snapshot!(context.filters(), context.python_uninstall(), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: the following required arguments were not provided:
      <TARGETS>...

    Usage: uv python uninstall <TARGETS>...

    For more information, try '--help'.
    "###);

    uv_snapshot!(context.filters(), context.python_uninstall().arg("3.13"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Searching for Python versions matching: Python 3.13
    Uninstalled Python 3.13.0 in [TIME]
     - cpython-3.13.0-[PLATFORM]
    "###);
}

#[test]
fn python_install_freethreaded() {
    let context: TestContext = TestContext::new_with_versions(&[]).with_filtered_python_keys();

    // Install the latest version
    uv_snapshot!(context.filters(), context.python_install().arg("3.13t"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Searching for Python versions matching: Python 3.13t
    Installed Python 3.13.0 in [TIME]
     + cpython-3.13.0+freethreaded-[PLATFORM]
    "###);

    // Should be distinct from 3.13
    uv_snapshot!(context.filters(), context.python_install().arg("3.13"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Searching for Python versions matching: Python 3.13
    Installed Python 3.13.0 in [TIME]
     + cpython-3.13.0-[PLATFORM]
    "###);

    // Should not work with older Python versions
    uv_snapshot!(context.filters(), context.python_install().arg("3.12t"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Searching for Python versions matching: Python 3.12t
    error: No download found for request: cpython-3.12t-[PLATFORM]
    "###);

    uv_snapshot!(context.filters(), context.python_uninstall().arg("--all"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Searching for Python installations
    Uninstalled 2 versions in [TIME]
     - cpython-3.13.0-[PLATFORM]
     - cpython-3.13.0+freethreaded-[PLATFORM]
    "###);
}
