#![cfg(all(feature = "python", feature = "pypi"))]

use assert_fs::fixture::{FileWriteStr as _, PathChild as _};
use common::{uv_snapshot, TestContext};
use insta::assert_snapshot;
use uv_python::{
    platform::{Arch, Os},
    PYTHON_VERSION_FILENAME,
};

mod common;

#[test]
fn python_pin() {
    let context: TestContext = TestContext::new_with_versions(&["3.11", "3.12"]);

    // Without arguments, we attempt to read the current pin (which does not exist yet)
    uv_snapshot!(context.filters(), context.python_pin(), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No pinned Python version found.
    "###);

    // Given an argument, we pin to that version
    uv_snapshot!(context.filters(), context.python_pin().arg("any"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Pinned to `any`

    ----- stderr -----
    "###);

    let python_version =
        fs_err::read_to_string(context.temp_dir.join(PYTHON_VERSION_FILENAME)).unwrap();
    assert_snapshot!(python_version, @r#"any"#);

    // Without arguments, we read the current pin
    uv_snapshot!(context.filters(), context.python_pin(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    any

    ----- stderr -----
    "###);

    // We should not mutate the file
    let python_version =
        fs_err::read_to_string(context.temp_dir.join(PYTHON_VERSION_FILENAME)).unwrap();
    assert_snapshot!(python_version, @r#"any"#);

    // Request Python 3.12
    uv_snapshot!(context.filters(), context.python_pin().arg("3.12"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Replaced existing pin with `3.12`

    ----- stderr -----
    "###);

    let python_version =
        fs_err::read_to_string(context.temp_dir.join(PYTHON_VERSION_FILENAME)).unwrap();
    assert_snapshot!(python_version, @r###"
    3.12
    "###);

    // Request Python 3.11
    uv_snapshot!(context.filters(), context.python_pin().arg("3.11"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Replaced existing pin with `3.11`

    ----- stderr -----
    "###);

    let python_version =
        fs_err::read_to_string(context.temp_dir.join(PYTHON_VERSION_FILENAME)).unwrap();
    assert_snapshot!(python_version, @r###"
    3.11
    "###);

    // Request CPython
    uv_snapshot!(context.filters(), context.python_pin().arg("cpython"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Replaced existing pin with `cpython`

    ----- stderr -----
    "###);

    let python_version =
        fs_err::read_to_string(context.temp_dir.join(PYTHON_VERSION_FILENAME)).unwrap();
    assert_snapshot!(python_version, @r###"
    cpython
    "###);

    // Request CPython 3.12
    uv_snapshot!(context.filters(), context.python_pin().arg("cpython@3.12"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Replaced existing pin with `cpython@3.12`

    ----- stderr -----
    "###);

    let python_version =
        fs_err::read_to_string(context.temp_dir.join(PYTHON_VERSION_FILENAME)).unwrap();
    assert_snapshot!(python_version, @r###"
    cpython@3.12
    "###);

    // Request CPython 3.12 via non-canonical syntax
    uv_snapshot!(context.filters(), context.python_pin().arg("cp3.12"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Replaced existing pin with `cpython@3.12`

    ----- stderr -----
    "###);

    let python_version =
        fs_err::read_to_string(context.temp_dir.join(PYTHON_VERSION_FILENAME)).unwrap();
    assert_snapshot!(python_version, @r###"
    cpython@3.12
    "###);

    // Request CPython 3.12 via partial key syntax
    uv_snapshot!(context.filters(), context.python_pin().arg("cpython-3.12"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Replaced existing pin with `cpython-3.12-any-any-any`

    ----- stderr -----
    "###);

    let python_version =
        fs_err::read_to_string(context.temp_dir.join(PYTHON_VERSION_FILENAME)).unwrap();
    assert_snapshot!(python_version, @r###"
    cpython-3.12-any-any-any
    "###);

    // Request a specific path
    uv_snapshot!(context.filters(), context.python_pin().arg(&context.python_versions.first().unwrap().1), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Replaced existing pin with `[PYTHON-3.11]`

    ----- stderr -----
    "###);

    let python_version =
        fs_err::read_to_string(context.temp_dir.join(PYTHON_VERSION_FILENAME)).unwrap();
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(python_version, @r###"
        [PYTHON-3.11]
        "###);
    });

    // Request an implementation that is not installed
    // (skip on Windows because the snapshot is different and the behavior is not platform dependent)
    #[cfg(unix)]
    {
        uv_snapshot!(context.filters(), context.python_pin().arg("pypy"), @r###"
        success: true
        exit_code: 0
        ----- stdout -----
        Replaced existing pin with `pypy`

        ----- stderr -----
        warning: No interpreter found for PyPy in system path
        "###);

        let python_version =
            fs_err::read_to_string(context.temp_dir.join(PYTHON_VERSION_FILENAME)).unwrap();
        assert_snapshot!(python_version, @r###"
        pypy
        "###);
    }

    // Request a version that is not installed
    // (skip on Windows because the snapshot is different and the behavior is not platform dependent)
    #[cfg(unix)]
    {
        uv_snapshot!(context.filters(), context.python_pin().arg("3.7"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Replaced existing pin with `3.7`

    ----- stderr -----
    warning: No interpreter found for Python 3.7 in system path
    "###);

        let python_version =
            fs_err::read_to_string(context.temp_dir.join(PYTHON_VERSION_FILENAME)).unwrap();
        assert_snapshot!(python_version, @r###"
    3.7
    "###);
    }
}

/// We do not need a Python interpreter to pin without `--resolved`
/// (skip on Windows because the snapshot is different and the behavior is not platform dependent)
#[cfg(unix)]
#[test]
fn python_pin_no_python() {
    let context: TestContext = TestContext::new_with_versions(&[]);

    uv_snapshot!(context.filters(), context.python_pin().arg("3.12"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Pinned to `3.12`

    ----- stderr -----
    warning: No interpreter found for Python 3.12 in system path
    "###);
}

#[test]
fn python_pin_compatible_with_requires_python() -> anyhow::Result<()> {
    let context: TestContext = TestContext::new_with_versions(&["3.10", "3.11"]);
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.11"
        dependencies = ["iniconfig"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.python_pin().arg("3.10"), @r###"
        success: false
        exit_code: 2
        ----- stdout -----

        ----- stderr -----
        error: The pinned Python version is incompatible with the project's `Requires-Python` of >=3.11.
        "###);

    uv_snapshot!(context.filters(), context.python_pin().arg("3.11"), @r###"
        success: true
        exit_code: 0
        ----- stdout -----
        Pinned to `3.11`

        ----- stderr -----
        "###);

    let python_version =
        fs_err::read_to_string(context.temp_dir.join(PYTHON_VERSION_FILENAME)).unwrap();
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(python_version, @r###"
        3.11
        "###);
    });

    // Updating `requires-python` should affect `uv python pin` compatibilities.
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.python_pin(), @r###"
        success: false
        exit_code: 2
        ----- stdout -----

        ----- stderr -----
        error: The pinned Python version is incompatible with the project's `Requires-Python` of >=3.12.
        "###);

    Ok(())
}

#[test]
fn warning_pinned_python_version_not_installed() -> anyhow::Result<()> {
    let context: TestContext = TestContext::new_with_versions(&["3.10", "3.11"]);
    let python_version_file = context.temp_dir.child(PYTHON_VERSION_FILENAME);
    python_version_file.write_str(r"3.12")?;
    if cfg!(windows) {
        uv_snapshot!(context.filters(), context.python_pin(), @r###"
        success: true
        exit_code: 0
        ----- stdout -----
        3.12

        ----- stderr -----
        warning: No interpreter found for Python 3.12 in system path or `py` launcher
        "###);
    } else {
        uv_snapshot!(context.filters(), context.python_pin(), @r###"
        success: true
        exit_code: 0
        ----- stdout -----
        3.12

        ----- stderr -----
        warning: No interpreter found for Python 3.12 in system path
        "###);
    }

    Ok(())
}

/// We do need a Python interpreter for `--resolved` pins
#[test]
fn python_pin_resolve_no_python() {
    let context: TestContext = TestContext::new_with_versions(&[]);

    if cfg!(windows) {
        uv_snapshot!(context.filters(), context.python_pin().arg("--resolved").arg("3.12"), @r###"
        success: false
        exit_code: 2
        ----- stdout -----

        ----- stderr -----
        error: No interpreter found for Python 3.12 in system path or `py` launcher
        "###);
    } else {
        uv_snapshot!(context.filters(), context.python_pin().arg("--resolved").arg("3.12"), @r###"
        success: false
        exit_code: 2
        ----- stdout -----

        ----- stderr -----
        error: No interpreter found for Python 3.12 in system path
        "###);
    }
}

#[test]
fn python_pin_resolve() {
    let context: TestContext = TestContext::new_with_versions(&["3.11", "3.12"]);

    // We pin the first interpreter on the path
    uv_snapshot!(context.filters(), context.python_pin().arg("--resolved").arg("any"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Pinned to `[PYTHON-3.11]`

    ----- stderr -----
    "###);

    let python_version =
        fs_err::read_to_string(context.temp_dir.join(PYTHON_VERSION_FILENAME)).unwrap();
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(python_version, @r###"
        [PYTHON-3.11]
        "###);
    });

    // Request Python 3.12
    uv_snapshot!(context.filters(), context.python_pin().arg("--resolved").arg("3.12"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Replaced existing pin with `[PYTHON-3.12]`

    ----- stderr -----
    "###);

    let python_version =
        fs_err::read_to_string(context.temp_dir.join(PYTHON_VERSION_FILENAME)).unwrap();
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(python_version, @r###"
        [PYTHON-3.12]
        "###);
    });

    // Request Python 3.11
    uv_snapshot!(context.filters(), context.python_pin().arg("--resolved").arg("3.11"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Replaced existing pin with `[PYTHON-3.11]`

    ----- stderr -----
    "###);

    let python_version =
        fs_err::read_to_string(context.temp_dir.join(PYTHON_VERSION_FILENAME)).unwrap();
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(python_version, @r###"
        [PYTHON-3.11]
        "###);
    });

    // Request CPython
    uv_snapshot!(context.filters(), context.python_pin().arg("--resolved").arg("cpython"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Replaced existing pin with `[PYTHON-3.11]`

    ----- stderr -----
    "###);

    let python_version =
        fs_err::read_to_string(context.temp_dir.join(PYTHON_VERSION_FILENAME)).unwrap();
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(python_version, @r###"
        [PYTHON-3.11]
        "###);
    });

    // Request CPython 3.12
    uv_snapshot!(context.filters(), context.python_pin().arg("--resolved").arg("cpython@3.12"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Replaced existing pin with `[PYTHON-3.12]`

    ----- stderr -----
    "###);

    let python_version =
        fs_err::read_to_string(context.temp_dir.join(PYTHON_VERSION_FILENAME)).unwrap();
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(python_version, @r###"
        [PYTHON-3.12]
        "###);
    });

    // Request CPython 3.12 via partial key syntax
    uv_snapshot!(context.filters(), context.python_pin().arg("--resolved").arg("cpython-3.12"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Replaced existing pin with `[PYTHON-3.12]`

    ----- stderr -----
    "###);

    let python_version =
        fs_err::read_to_string(context.temp_dir.join(PYTHON_VERSION_FILENAME)).unwrap();
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(python_version, @r###"
        [PYTHON-3.12]
        "###);
    });

    // Request CPython 3.12 for the current platform
    let os = Os::from_env();
    let arch = Arch::from_env();

    uv_snapshot!(context.filters(), context.python_pin().arg("--resolved")
    .arg(format!("cpython-3.12-{os}-{arch}"))
    , @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Replaced existing pin with `[PYTHON-3.12]`

    ----- stderr -----
    "###);

    let python_version =
        fs_err::read_to_string(context.temp_dir.join(PYTHON_VERSION_FILENAME)).unwrap();
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(python_version, @r###"
        [PYTHON-3.12]
        "###);
    });

    // Request an implementation that is not installed
    // (skip on Windows because the snapshot is different and the behavior is not platform dependent)
    #[cfg(unix)]
    uv_snapshot!(context.filters(), context.python_pin().arg("--resolved").arg("pypy"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No interpreter found for PyPy in system path
    "###);

    let python_version =
        fs_err::read_to_string(context.temp_dir.join(PYTHON_VERSION_FILENAME)).unwrap();
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(python_version, @r###"
        [PYTHON-3.12]
        "###);
    });

    // Request a version that is not installed
    // (skip on Windows because the snapshot is different and the behavior is not platform dependent)
    #[cfg(unix)]
    uv_snapshot!(context.filters(), context.python_pin().arg("--resolved").arg("3.7"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No interpreter found for Python 3.7 in system path
    "###);

    let python_version =
        fs_err::read_to_string(context.temp_dir.join(PYTHON_VERSION_FILENAME)).unwrap();
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(python_version, @r###"
        [PYTHON-3.12]
        "###);
    });
}
