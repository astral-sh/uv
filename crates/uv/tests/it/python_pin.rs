use std::path::PathBuf;

use crate::common::{uv_snapshot, TestContext};
use anyhow::Result;
use assert_fs::fixture::{FileWriteStr, PathChild, PathCreateDir};
use insta::assert_snapshot;
use uv_python::{
    platform::{Arch, Os},
    PYTHON_VERSIONS_FILENAME, PYTHON_VERSION_FILENAME,
};

#[test]
fn python_pin() {
    let context: TestContext = TestContext::new_with_versions(&["3.11", "3.12"]);

    // Without arguments, we attempt to read the current pin (which does not exist yet)
    uv_snapshot!(context.filters(), context.python_pin(), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No pinned Python version found
    "###);

    // Given an argument, we pin to that version
    uv_snapshot!(context.filters(), context.python_pin().arg("any"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Pinned `.python-version` to `any`

    ----- stderr -----
    "###);

    let python_version = context.read(PYTHON_VERSION_FILENAME);
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
    let python_version = context.read(PYTHON_VERSION_FILENAME);
    assert_snapshot!(python_version, @r#"any"#);

    // Request Python 3.12
    uv_snapshot!(context.filters(), context.python_pin().arg("3.12"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Updated `.python-version` from `any` -> `3.12`

    ----- stderr -----
    "###);

    let python_version = context.read(PYTHON_VERSION_FILENAME);
    assert_snapshot!(python_version, @r###"
    3.12
    "###);

    // Request Python 3.11
    uv_snapshot!(context.filters(), context.python_pin().arg("3.11"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Updated `.python-version` from `3.12` -> `3.11`

    ----- stderr -----
    "###);

    let python_version = context.read(PYTHON_VERSION_FILENAME);
    assert_snapshot!(python_version, @r###"
    3.11
    "###);

    // Request CPython
    uv_snapshot!(context.filters(), context.python_pin().arg("cpython"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Updated `.python-version` from `3.11` -> `cpython`

    ----- stderr -----
    "###);

    let python_version = context.read(PYTHON_VERSION_FILENAME);
    assert_snapshot!(python_version, @r###"
    cpython
    "###);

    // Request CPython 3.12
    uv_snapshot!(context.filters(), context.python_pin().arg("cpython@3.12"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Updated `.python-version` from `cpython` -> `cpython@3.12`

    ----- stderr -----
    "###);

    let python_version = context.read(PYTHON_VERSION_FILENAME);
    assert_snapshot!(python_version, @r###"
    cpython@3.12
    "###);

    // Request CPython 3.12 via non-canonical syntax
    uv_snapshot!(context.filters(), context.python_pin().arg("cp3.12"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Pinned `.python-version` to `cpython@3.12`

    ----- stderr -----
    "###);

    let python_version = context.read(PYTHON_VERSION_FILENAME);
    assert_snapshot!(python_version, @r###"
    cpython@3.12
    "###);

    // Request CPython 3.12 via partial key syntax
    uv_snapshot!(context.filters(), context.python_pin().arg("cpython-3.12"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Updated `.python-version` from `cpython@3.12` -> `cpython-3.12-any-any-any`

    ----- stderr -----
    "###);

    let python_version = context.read(PYTHON_VERSION_FILENAME);
    assert_snapshot!(python_version, @r###"
    cpython-3.12-any-any-any
    "###);

    // Request a specific path
    uv_snapshot!(context.filters(), context.python_pin().arg(&context.python_versions.first().unwrap().1), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Updated `.python-version` from `cpython-3.12-any-any-any` -> `[PYTHON-3.11]`

    ----- stderr -----
    "###);

    let python_version = context.read(PYTHON_VERSION_FILENAME);
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
        Updated `.python-version` from `[PYTHON-3.11]` -> `pypy`

        ----- stderr -----
        warning: No interpreter found for PyPy in managed installations or search path
        "###);

        let python_version = context.read(PYTHON_VERSION_FILENAME);
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
        Updated `.python-version` from `pypy` -> `3.7`

        ----- stderr -----
        warning: No interpreter found for Python 3.7 in managed installations or search path
        "###);

        let python_version = context.read(PYTHON_VERSION_FILENAME);
        assert_snapshot!(python_version, @r###"
    3.7
    "###);
    }
}

// If there is no project-level `.python-version` file, respect the global pin.
#[test]
fn python_pin_global_if_no_local() -> Result<()> {
    let context: TestContext = TestContext::new_with_versions(&["3.11", "3.12"]);
    let uv = context.user_config_dir.child("uv");
    uv.create_dir_all()?;

    // Without arguments, we attempt to read the current pin (which does not exist yet)
    uv_snapshot!(context.filters(), context.python_pin(), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No pinned Python version found
    "###);

    // Given an argument, we globally pin to that version
    uv_snapshot!(context.filters(), context.python_pin().arg("3.11").arg("--global"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    Pinned `[UV_USER_CONFIG_DIR]/.python-version` to `3.11`

    ----- stderr -----
    ");

    // If no local pin, use global.
    uv_snapshot!(context.filters(), context.python_pin(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    3.11

    ----- stderr -----
    "###);

    Ok(())
}

// If there is a project-level `.python-version` file, it takes precedence over
// the global pin.
#[test]
fn python_pin_global_use_local_if_available() -> Result<()> {
    let context: TestContext = TestContext::new_with_versions(&["3.11", "3.12"]);
    let uv = context.user_config_dir.child("uv");
    uv.create_dir_all()?;

    // Given an argument, we globally pin to that version
    uv_snapshot!(context.filters(), context.python_pin().arg("3.12").arg("--global"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    Pinned `[UV_USER_CONFIG_DIR]/.python-version` to `3.12`

    ----- stderr -----
    ");

    // With no local, we get the global pin
    uv_snapshot!(context.filters(), context.python_pin(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    3.12

    ----- stderr -----
    "###);

    let mut global_version_path = PathBuf::from(uv.path());
    global_version_path.push(PYTHON_VERSION_FILENAME);
    let global_python_version = context.read(&global_version_path);
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(global_python_version, @r###"
        3.12
        "###);
    });

    // Request Python 3.11 for local .python-version
    uv_snapshot!(context.filters(), context.python_pin().arg("3.11"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Pinned `.python-version` to `3.11`

    ----- stderr -----
    "###);

    // Local should override global
    uv_snapshot!(context.filters(), context.python_pin(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    3.11

    ----- stderr -----
    "###);

    // We should still be able to check global pin
    uv_snapshot!(context.filters(), context.python_pin().arg("--global"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    3.12

    ----- stderr -----
    "###);

    // Local .python-version exists and has the right version.
    let local_python_version = context.read(PYTHON_VERSION_FILENAME);
    assert_snapshot!(local_python_version, @r###"
    3.11
    "###);

    // Global .python-version still exists and has the right version.
    let global_python_version = context.read(&global_version_path);
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(global_python_version, @r###"
        3.12
        "###);
    });

    Ok(())
}

#[test]
fn python_pin_global_creates_parent_dirs() {
    let context: TestContext = TestContext::new_with_versions(&["3.12"]);
    let uv_global_config_dir = context.user_config_dir.child("uv");

    assert!(
        !uv_global_config_dir.exists(),
        "Global config directory should not exist yet."
    );

    uv_snapshot!(context.filters(), context.python_pin().arg("3.12").arg("--global"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    Pinned `[UV_USER_CONFIG_DIR]/.python-version` to `3.12`

    ----- stderr -----
    ");

    assert!(
        uv_global_config_dir.exists(),
        "Global config directory should be automatically created (if missing) after global pin."
    );
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
    Pinned `.python-version` to `3.12`

    ----- stderr -----
    warning: No interpreter found for Python 3.12 in managed installations or search path
    "###);
}

#[test]
fn python_pin_compatible_with_requires_python() -> Result<()> {
    let context: TestContext =
        TestContext::new_with_versions(&["3.10", "3.11"]).with_filtered_python_sources();
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
    error: The requested Python version `3.10` is incompatible with the project `requires-python` value of `>=3.11`.
    "###);

    // Request a implementation version that is incompatible
    uv_snapshot!(context.filters(), context.python_pin().arg("cpython@3.10"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: The requested Python version `cpython@3.10` is incompatible with the project `requires-python` value of `>=3.11`.
    "###);

    // Request an incompatible version with project discovery turned off
    uv_snapshot!(context.filters(), context.python_pin().arg("cpython@3.10").arg("--no-project"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Pinned `.python-version` to `cpython@3.10`

    ----- stderr -----
    "###);

    // And, as an alias, workspace discovery
    uv_snapshot!(context.filters(), context.python_pin().arg("cpython@3.10").arg("--no-workspace"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Pinned `.python-version` to `cpython@3.10`

    ----- stderr -----
    "###);

    // Request a complex version range that resolves to an incompatible version
    uv_snapshot!(context.filters(), context.python_pin().arg(">3.8,<3.11"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Updated `.python-version` from `cpython@3.10` -> `>3.8, <3.11`

    ----- stderr -----
    warning: The requested Python version `>3.8, <3.11` resolves to `3.10.[X]` which  is incompatible with the project `requires-python` value of `>=3.11`.
    "###);

    // Request a version that is compatible
    uv_snapshot!(context.filters(), context.python_pin().arg("3.11"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Updated `.python-version` from `>3.8, <3.11` -> `3.11`

    ----- stderr -----
    "###);

    // Request a version that is compatible and uses a Python variant
    uv_snapshot!(context.filters(), context.python_pin().arg("3.13t"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Updated `.python-version` from `3.11` -> `3.13t`

    ----- stderr -----
    warning: No interpreter found for Python 3.13t in [PYTHON SOURCES]
    "###);

    // Request a implementation version that is compatible
    uv_snapshot!(context.filters(), context.python_pin().arg("cpython@3.11"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Updated `.python-version` from `3.13t` -> `cpython@3.11`

    ----- stderr -----
    "###);

    let python_version = context.read(PYTHON_VERSION_FILENAME);
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(python_version, @r###"
        cpython@3.11
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
    success: true
    exit_code: 0
    ----- stdout -----
    cpython@3.11

    ----- stderr -----
    warning: The pinned Python version `cpython@3.11` is incompatible with the project `requires-python` value of `>=3.12`.
    "###);

    // Request a implementation that resolves to a compatible version
    uv_snapshot!(context.filters(), context.python_pin().arg("cpython"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Updated `.python-version` from `cpython@3.11` -> `cpython`

    ----- stderr -----
    warning: The requested Python version `cpython` resolves to `3.10.[X]` which  is incompatible with the project `requires-python` value of `>=3.12`.
    "###);

    uv_snapshot!(context.filters(), context.python_pin(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    cpython

    ----- stderr -----
    warning: The pinned Python version `cpython` resolves to `3.10.[X]` which  is incompatible with the project `requires-python` value of `>=3.12`.
    "###);

    // Request a complex version range that resolves to a compatible version
    uv_snapshot!(context.filters(), context.python_pin().arg(">3.8,<3.12"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Updated `.python-version` from `cpython` -> `>3.8, <3.12`

    ----- stderr -----
    warning: The requested Python version `>3.8, <3.12` resolves to `3.10.[X]` which  is incompatible with the project `requires-python` value of `>=3.12`.
    "###);

    uv_snapshot!(context.filters(), context.python_pin(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    >3.8, <3.12

    ----- stderr -----
    warning: The pinned Python version `>3.8, <3.12` resolves to `3.10.[X]` which  is incompatible with the project `requires-python` value of `>=3.12`.
    "###);

    Ok(())
}

#[test]
fn warning_pinned_python_version_not_installed() -> Result<()> {
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

    let python_version_file = context.temp_dir.child(PYTHON_VERSION_FILENAME);
    python_version_file.write_str(r"3.12")?;
    if cfg!(windows) {
        uv_snapshot!(context.filters(), context.python_pin(), @r###"
        success: true
        exit_code: 0
        ----- stdout -----
        3.12

        ----- stderr -----
        warning: Failed to resolve pinned Python version `3.12`: No interpreter found for Python 3.12 in managed installations, search path, or registry
        "###);
    } else {
        uv_snapshot!(context.filters(), context.python_pin(), @r###"
        success: true
        exit_code: 0
        ----- stdout -----
        3.12

        ----- stderr -----
        warning: Failed to resolve pinned Python version `3.12`: No interpreter found for Python 3.12 in managed installations or search path
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
        error: No interpreter found for Python 3.12 in managed installations, search path, or registry
        "###);
    } else {
        uv_snapshot!(context.filters(), context.python_pin().arg("--resolved").arg("3.12"), @r###"
        success: false
        exit_code: 2
        ----- stdout -----

        ----- stderr -----
        error: No interpreter found for Python 3.12 in managed installations or search path
        "###);
    }
}

#[test]
fn python_pin_resolve() {
    let context: TestContext = TestContext::new_with_versions(&["3.12", "3.13"]);

    // We pin the first interpreter on the path
    uv_snapshot!(context.filters(), context.python_pin().arg("--resolved").arg("any"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Pinned `.python-version` to `[PYTHON-3.12]`

    ----- stderr -----
    "###);

    let python_version = context.read(PYTHON_VERSION_FILENAME);
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(python_version, @"[PYTHON-3.12]");
    });

    // Request Python 3.13
    uv_snapshot!(context.filters(), context.python_pin().arg("--resolved").arg("3.13"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Updated `.python-version` from `[PYTHON-3.12]` -> `[PYTHON-3.13]`

    ----- stderr -----
    "###);

    let python_version = context.read(PYTHON_VERSION_FILENAME);
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(python_version, @"[PYTHON-3.13]");
    });

    // Request Python 3.13
    uv_snapshot!(context.filters(), context.python_pin().arg("--resolved").arg("3.13"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Pinned `.python-version` to `[PYTHON-3.13]`

    ----- stderr -----
    "###);

    let python_version = context.read(PYTHON_VERSION_FILENAME);
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(python_version, @"[PYTHON-3.13]");
    });

    // Request CPython
    uv_snapshot!(context.filters(), context.python_pin().arg("--resolved").arg("cpython"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Updated `.python-version` from `[PYTHON-3.13]` -> `[PYTHON-3.12]`

    ----- stderr -----
    "###);

    let python_version = context.read(PYTHON_VERSION_FILENAME);
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(python_version, @"[PYTHON-3.12]");
    });

    // Request CPython 3.13
    uv_snapshot!(context.filters(), context.python_pin().arg("--resolved").arg("cpython@3.13"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Updated `.python-version` from `[PYTHON-3.12]` -> `[PYTHON-3.13]`

    ----- stderr -----
    "###);

    let python_version = context.read(PYTHON_VERSION_FILENAME);
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(python_version, @"[PYTHON-3.13]");
    });

    // Request CPython 3.13 via partial key syntax
    uv_snapshot!(context.filters(), context.python_pin().arg("--resolved").arg("cpython-3.13"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Pinned `.python-version` to `[PYTHON-3.13]`

    ----- stderr -----
    "###);

    let python_version = context.read(PYTHON_VERSION_FILENAME);
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(python_version, @"[PYTHON-3.13]");
    });

    // Request CPython 3.13 for the current platform
    let os = Os::from_env();
    let arch = Arch::from_env();

    uv_snapshot!(context.filters(), context.python_pin().arg("--resolved")
    .arg(format!("cpython-3.13-{os}-{arch}"))
    , @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Pinned `.python-version` to `[PYTHON-3.13]`

    ----- stderr -----
    "###);

    let python_version = context.read(PYTHON_VERSION_FILENAME);
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(python_version, @"[PYTHON-3.13]");
    });

    // Request an implementation that is not installed
    // (skip on Windows because the snapshot is different and the behavior is not platform dependent)
    #[cfg(unix)]
    uv_snapshot!(context.filters(), context.python_pin().arg("--resolved").arg("pypy"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No interpreter found for PyPy in managed installations or search path
    "###);

    let python_version = context.read(PYTHON_VERSION_FILENAME);
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(python_version, @"[PYTHON-3.13]");
    });

    // Request a version that is not installed
    // (skip on Windows because the snapshot is different and the behavior is not platform dependent)
    #[cfg(unix)]
    uv_snapshot!(context.filters(), context.python_pin().arg("--resolved").arg("3.7"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No interpreter found for Python 3.7 in managed installations or search path
    "###);

    let python_version = context.read(PYTHON_VERSION_FILENAME);
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(python_version, @"[PYTHON-3.13]");
    });
}

#[test]
fn python_pin_with_comments() -> Result<()> {
    let context = TestContext::new_with_versions(&[]);

    let content = indoc::indoc! {r"
        3.12

        # 3.11
        3.10
    "};

    let version_file = context.temp_dir.child(PYTHON_VERSION_FILENAME);
    version_file.write_str(content)?;
    uv_snapshot!(context.filters(), context.python_pin(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    3.12
    3.10

    ----- stderr -----
    "###);
    fs_err::remove_file(version_file)?;

    let versions_file = context.temp_dir.child(PYTHON_VERSIONS_FILENAME);
    versions_file.write_str(content)?;
    uv_snapshot!(context.filters(), context.python_pin(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    3.12
    3.10

    ----- stderr -----
    "###);

    Ok(())
}
