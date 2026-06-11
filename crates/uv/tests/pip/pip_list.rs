use anyhow::Result;
use assert_cmd::prelude::*;
use assert_fs::fixture::ChildPath;
use assert_fs::fixture::FileWriteStr;
use assert_fs::fixture::PathChild;
use assert_fs::prelude::*;

use uv_python::PythonVersion;
use uv_python::managed::ManagedPythonInstallations;
use uv_static::EnvVars;
use uv_test::uv_snapshot;

#[test]
fn list_empty_columns() {
    let context = uv_test::test_context!("3.12");

    uv_snapshot!(context.pip_list()
        .arg("--format")
        .arg("columns"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    "
    );
}

#[test]
fn list_empty_freeze() {
    let context = uv_test::test_context!("3.12");

    uv_snapshot!(context.pip_list()
        .arg("--format")
        .arg("freeze"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    "
    );
}

#[test]
fn list_empty_json() {
    let context = uv_test::test_context!("3.12");

    uv_snapshot!(context.pip_list()
        .arg("--format")
        .arg("json"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    []

    ----- stderr -----
    "
    );
}

#[test]
#[cfg(feature = "test-pypi")]
fn list_single_no_editable() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("MarkupSafe==2.1.3")?;

    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + markupsafe==2.1.3
    "
    );

    context.assert_command("import markupsafe").success();

    uv_snapshot!(context.pip_list(), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Package    Version
    ---------- -------
    markupsafe 2.1.3

    ----- stderr -----
    "
    );

    Ok(())
}

#[test]
#[cfg(feature = "test-pypi")]
fn list_outdated_columns() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("anyio==3.0.0")?;

    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==3.0.0
     + idna==3.6
     + sniffio==1.3.1
    "
    );

    uv_snapshot!(context.pip_list().arg("--outdated"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Package Version Latest Type
    ------- ------- ------ -----
    anyio   3.0.0   4.3.0  wheel

    ----- stderr -----
    "
    );

    Ok(())
}

#[test]
#[cfg(feature = "test-pypi")]
fn list_outdated_json() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("anyio==3.0.0")?;

    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==3.0.0
     + idna==3.6
     + sniffio==1.3.1
    "
    );

    uv_snapshot!(context.pip_list().arg("--outdated").arg("--format").arg("json"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    [{"name":"anyio","version":"3.0.0","latest_version":"4.3.0","latest_filetype":"wheel"}]

    ----- stderr -----
    "#
    );

    Ok(())
}

#[test]
fn list_outdated_find_links() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let links_dir = context.workspace_root.join("test/links");
    let first_links_dir = context.temp_dir.child("first-links");
    first_links_dir.create_dir_all()?;
    fs_err::copy(
        links_dir.join("validation-2.0.0-py3-none-any.whl"),
        first_links_dir
            .child("validation-2.0.0-py3-none-any.whl")
            .path(),
    )?;
    let second_links_dir = context.temp_dir.child("second-links");
    second_links_dir.create_dir_all()?;
    fs_err::copy(
        links_dir.join("validation-3.0.0-py3-none-any.whl"),
        second_links_dir
            .child("validation-3.0.0-py3-none-any.whl")
            .path(),
    )?;

    uv_snapshot!(context.filters(), context.pip_install()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("validation==1.0.0")
        .arg("--find-links")
        .arg(&links_dir)
        .arg("--no-index"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + validation==1.0.0
    "###
    );

    uv_snapshot!(context.filters(), context.pip_list()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--outdated")
        .arg("--find-links")
        .arg(first_links_dir.path())
        .arg("--find-links")
        .arg(second_links_dir.path())
        .arg("--no-index"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Package    Version Latest Type
    ---------- ------- ------ -----
    validation 1.0.0   3.0.0  wheel

    ----- stderr -----
    "###
    );

    Ok(())
}

#[test]
fn list_outdated_freeze() {
    let context = uv_test::test_context!("3.12");

    uv_snapshot!(context.pip_list().arg("--outdated").arg("--format").arg("freeze"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: `--outdated` cannot be used with `--format freeze`
    "
    );
}

#[test]
#[cfg(feature = "test-git")]
fn list_outdated_git() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(indoc::indoc! {r"
        iniconfig==1.0.0
        uv-public-pypackage @ git+https://github.com/astral-test/uv-public-pypackage@0.0.1
    "})?;

    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + iniconfig==1.0.0
     + uv-public-pypackage==0.1.0 (from git+https://github.com/astral-test/uv-public-pypackage@0dacfd662c64cb4ceb16e6cf65a157a8b715b979)
    "
    );

    uv_snapshot!(context.pip_list().arg("--outdated"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Package   Version Latest Type
    --------- ------- ------ -----
    iniconfig 1.0.0   2.0.0  wheel

    ----- stderr -----
    "
    );

    Ok(())
}

#[test]
#[cfg(feature = "test-pypi")]
fn list_outdated_index() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("anyio==3.0.0")?;

    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==3.0.0
     + idna==3.6
     + sniffio==1.3.1
    "
    );

    uv_snapshot!(context.pip_list()
        .arg("--outdated")
        .arg("--index-url")
        .arg("https://test.pypi.org/simple"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Package Version Latest Type
    ------- ------- ------ -----
    anyio   3.0.0   3.5.0  wheel

    ----- stderr -----
    "
    );

    Ok(())
}

#[test]
#[cfg(feature = "test-pypi")]
fn list_editable() {
    let context = uv_test::test_context!("3.12");

    // Install the editable package.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-e")
        .arg(context.workspace_root.join("test/packages/poetry_editable")), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + poetry-editable==0.1.0 (from file://[WORKSPACE]/test/packages/poetry_editable)
     + sniffio==1.3.1
    "
    );

    let filters = context
        .filters()
        .into_iter()
        .chain(vec![(r"\-\-\-\-\-\-+.*", "[UNDERLINE]"), ("  +", " ")])
        .collect::<Vec<_>>();

    uv_snapshot!(filters, context.pip_list(), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Package Version Editable project location
    [UNDERLINE]
    anyio 4.3.0
    idna 3.6
    poetry-editable 0.1.0 [WORKSPACE]/test/packages/poetry_editable
    sniffio 1.3.1

    ----- stderr -----
    "
    );
}

#[test]
#[cfg(feature = "test-pypi")]
fn list_editable_only() {
    let context = uv_test::test_context!("3.12");

    // Install the editable package.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-e")
        .arg(context.workspace_root.join("test/packages/poetry_editable")), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + poetry-editable==0.1.0 (from file://[WORKSPACE]/test/packages/poetry_editable)
     + sniffio==1.3.1
    "
    );

    let filters = context
        .filters()
        .into_iter()
        .chain(vec![(r"\-\-\-\-\-\-+.*", "[UNDERLINE]"), ("  +", " ")])
        .collect::<Vec<_>>();

    uv_snapshot!(filters, context.pip_list()
        .arg("--editable"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Package Version Editable project location
    [UNDERLINE]
    poetry-editable 0.1.0 [WORKSPACE]/test/packages/poetry_editable

    ----- stderr -----
    "
    );

    uv_snapshot!(filters, context.pip_list()
        .arg("--exclude-editable"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Package Version
    [UNDERLINE]
    anyio 4.3.0
    idna 3.6
    sniffio 1.3.1

    ----- stderr -----
    "
    );

    uv_snapshot!(filters, context.pip_list()
        .arg("--editable")
        .arg("--exclude-editable"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: the argument '--editable' cannot be used with '--exclude-editable'

    Usage: uv pip list --cache-dir [CACHE_DIR] --editable --exclude-newer <EXCLUDE_NEWER>

    For more information, try '--help'.
    "
    );
}

#[test]
#[cfg(feature = "test-pypi")]
fn list_exclude() {
    let context = uv_test::test_context!("3.12");

    // Install the editable package.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-e")
        .arg(context.workspace_root.join("test/packages/poetry_editable")), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + poetry-editable==0.1.0 (from file://[WORKSPACE]/test/packages/poetry_editable)
     + sniffio==1.3.1
    "
    );

    let filters = context
        .filters()
        .into_iter()
        .chain(vec![(r"\-\-\-\-\-\-+.*", "[UNDERLINE]"), ("  +", " ")])
        .collect::<Vec<_>>();

    uv_snapshot!(filters, context.pip_list()
    .arg("--exclude")
    .arg("numpy"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Package Version Editable project location
    [UNDERLINE]
    anyio 4.3.0
    idna 3.6
    poetry-editable 0.1.0 [WORKSPACE]/test/packages/poetry_editable
    sniffio 1.3.1

    ----- stderr -----
    "
    );

    uv_snapshot!(filters, context.pip_list()
    .arg("--exclude")
    .arg("poetry-editable"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Package Version
    [UNDERLINE]
    anyio 4.3.0
    idna 3.6
    sniffio 1.3.1

    ----- stderr -----
    "
    );

    uv_snapshot!(filters, context.pip_list()
    .arg("--exclude")
    .arg("numpy")
    .arg("--exclude")
    .arg("poetry-editable"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Package Version
    [UNDERLINE]
    anyio 4.3.0
    idna 3.6
    sniffio 1.3.1

    ----- stderr -----
    "
    );
}

#[test]
#[cfg(feature = "test-pypi")]
#[cfg(not(windows))]
fn list_format_json() {
    let context = uv_test::test_context!("3.12");

    // Install the editable package.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-e")
        .arg(context.workspace_root.join("test/packages/poetry_editable")), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + poetry-editable==0.1.0 (from file://[WORKSPACE]/test/packages/poetry_editable)
     + sniffio==1.3.1
    "
    );

    let filters: Vec<_> = context
        .filters()
        .into_iter()
        .chain(vec![(r"\-\-\-\-\-\-+.*", "[UNDERLINE]"), ("  +", " ")])
        .collect();

    uv_snapshot!(filters, context.pip_list()
    .arg("--format=json"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    [{"name":"anyio","version":"4.3.0"},{"name":"idna","version":"3.6"},{"name":"poetry-editable","version":"0.1.0","editable_project_location":"[WORKSPACE]/test/packages/poetry_editable"},{"name":"sniffio","version":"1.3.1"}]

    ----- stderr -----
    "#
    );

    uv_snapshot!(filters, context.pip_list()
    .arg("--format=json")
    .arg("--editable"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    [{"name":"poetry-editable","version":"0.1.0","editable_project_location":"[WORKSPACE]/test/packages/poetry_editable"}]

    ----- stderr -----
    "#
    );

    uv_snapshot!(filters, context.pip_list()
    .arg("--format=json")
    .arg("--exclude-editable"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    [{"name":"anyio","version":"4.3.0"},{"name":"idna","version":"3.6"},{"name":"sniffio","version":"1.3.1"}]

    ----- stderr -----
    "#
    );
}

#[test]
#[cfg(feature = "test-pypi")]
fn list_format_freeze() {
    let context = uv_test::test_context!("3.12");

    // Install the editable package.
    uv_snapshot!(context.filters(), context
        .pip_install()
        .arg("-e")
        .arg(context.workspace_root.join("test/packages/poetry_editable")), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + poetry-editable==0.1.0 (from file://[WORKSPACE]/test/packages/poetry_editable)
     + sniffio==1.3.1
    "
    );

    let filters = context
        .filters()
        .into_iter()
        .chain(vec![(r"\-\-\-\-\-\-+.*", "[UNDERLINE]"), ("  +", " ")])
        .collect::<Vec<_>>();

    uv_snapshot!(filters, context.pip_list()
    .arg("--format=freeze"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    anyio==4.3.0
    idna==3.6
    poetry-editable==0.1.0
    sniffio==1.3.1

    ----- stderr -----
    "
    );

    uv_snapshot!(filters, context.pip_list()
    .arg("--format=freeze")
    .arg("--editable"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    poetry-editable==0.1.0

    ----- stderr -----
    "
    );

    uv_snapshot!(filters, context.pip_list()
    .arg("--format=freeze")
    .arg("--exclude-editable"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    anyio==4.3.0
    idna==3.6
    sniffio==1.3.1

    ----- stderr -----
    "
    );
}

#[test]
fn list_legacy_editable() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let site_packages = ChildPath::new(context.site_packages());

    let target = context.temp_dir.child("zstandard_project");
    target.child("zstd").create_dir_all()?;
    target.child("zstd").child("__init__.py").write_str("")?;

    target.child("zstandard.egg-info").create_dir_all()?;
    target
        .child("zstandard.egg-info")
        .child("PKG-INFO")
        .write_str(
            "Metadata-Version: 2.1
Name: zstandard
Version: 0.22.0
",
        )?;

    site_packages
        .child("zstandard.egg-link")
        .write_str(target.path().to_str().unwrap())?;

    site_packages.child("easy-install.pth").write_str(&format!(
        "something\n{}\nanother thing\n",
        target.path().to_str().unwrap()
    ))?;

    let filters = context
        .filters()
        .into_iter()
        .chain(vec![(r"\-\-\-\-\-\-+.*", "[UNDERLINE]"), ("  +", " ")])
        .collect::<Vec<_>>();

    uv_snapshot!(filters, context.pip_list()
        .arg("--editable"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Package Version Editable project location
    [UNDERLINE]
    zstandard 0.22.0 [TEMP_DIR]/zstandard_project

    ----- stderr -----
    "
    );

    Ok(())
}

#[test]
fn list_legacy_editable_invalid_version() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let site_packages = ChildPath::new(context.site_packages());

    let target = context.temp_dir.child("paramiko_project");
    target.child("paramiko.egg-info").create_dir_all()?;
    target
        .child("paramiko.egg-info")
        .child("PKG-INFO")
        .write_str(
            "Metadata-Version: 1.0
Name: paramiko
Version: 0.1-bulbasaur
",
        )?;
    site_packages
        .child("paramiko.egg-link")
        .write_str(target.path().to_str().unwrap())?;

    let filters = context
        .filters()
        .into_iter()
        .chain(vec![(r"\-\-\-\-\-\-+.*", "[UNDERLINE]"), ("  +", " ")])
        .collect::<Vec<_>>();

    uv_snapshot!(filters, context.pip_list()
        .arg("--editable"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to read metadata from: `[SITE_PACKAGES]/paramiko.egg-link`
     Caused by: after parsing `0.1-b`, found `ulbasaur`, which is not part of a valid version
    "
    );

    Ok(())
}

#[test]
#[cfg(feature = "test-pypi")]
fn list_ignores_quiet_flag_format_freeze() {
    let context = uv_test::test_context!("3.12");

    // Install the editable package.
    uv_snapshot!(context.filters(), context
        .pip_install()
        .arg("-e")
        .arg(context.workspace_root.join("test/packages/poetry_editable")), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + poetry-editable==0.1.0 (from file://[WORKSPACE]/test/packages/poetry_editable)
     + sniffio==1.3.1
    "
    );

    let filters = context
        .filters()
        .into_iter()
        .chain(vec![(r"\-\-\-\-\-\-+.*", "[UNDERLINE]"), ("  +", " ")])
        .collect::<Vec<_>>();

    uv_snapshot!(filters, context.pip_list()
    .arg("--format=freeze")
    .arg("--quiet"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    anyio==4.3.0
    idna==3.6
    poetry-editable==0.1.0
    sniffio==1.3.1

    ----- stderr -----
    "
    );

    uv_snapshot!(filters, context.pip_list()
    .arg("--format=freeze")
    .arg("--editable")
    .arg("--quiet"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    poetry-editable==0.1.0

    ----- stderr -----
    "
    );

    uv_snapshot!(filters, context.pip_list()
    .arg("--format=freeze")
    .arg("--exclude-editable")
    .arg("--quiet"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    anyio==4.3.0
    idna==3.6
    sniffio==1.3.1

    ----- stderr -----
    "
    );
}

#[test]
#[cfg(feature = "test-pypi")]
fn list_target() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("MarkupSafe==2.1.3\ntomli==2.0.1")?;

    let target = context.temp_dir.child("target");

    // Install packages to a target directory.
    context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--target")
        .arg(target.path())
        .assert()
        .success();

    // List packages in the target directory.
    uv_snapshot!(context.filters(), context.pip_list()
        .arg("--target")
        .arg(target.path()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Package    Version
    ---------- -------
    markupsafe 2.1.3
    tomli      2.0.1

    ----- stderr -----
    "
    );

    // Without --target, the packages should not be visible.
    uv_snapshot!(context.pip_list(), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    "
    );

    Ok(())
}

#[test]
#[cfg(feature = "test-pypi")]
fn list_prefix() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("MarkupSafe==2.1.3\ntomli==2.0.1")?;

    let prefix = context.temp_dir.child("prefix");

    // Install packages to a prefix directory.
    context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--prefix")
        .arg(prefix.path())
        .assert()
        .success();

    // List packages in the prefix directory.
    uv_snapshot!(context.filters(), context.pip_list()
        .arg("--prefix")
        .arg(prefix.path()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Package    Version
    ---------- -------
    markupsafe 2.1.3
    tomli      2.0.1

    ----- stderr -----
    "
    );

    // Without --prefix, the packages should not be visible.
    uv_snapshot!(context.pip_list(), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    "
    );

    Ok(())
}

/// Check support for `include-system-site-packages = true` in `uv pip list`.
///
/// Specifically, check that:
/// * system packages are shown when using the system interpreter
/// * system packages are hidden with `include-system-site-packages = false`, but venv packages are
///   shown
/// * system packages are shown with `include-system-site-packages = true`, and also venv packages
#[test]
#[cfg(feature = "test-pypi")]
fn list_system_site_packages() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_python_keys()
        .with_filtered_python_install_bin()
        .with_filtered_python_names()
        .with_filter((r"(?m)^(pip +)\d+\.\d+\.\d+$", "$1[PIP_VERSION]"));

    let python_version_312: PythonVersion = "3.12".parse().map_err(anyhow::Error::msg)?;
    let base_python = ManagedPythonInstallations::from_settings(None)?
        .find_version(&python_version_312)?
        .next()
        .expect("a managed Python 3.12 interpreter");
    let relative_executable = base_python
        .executable(false)
        .strip_prefix(base_python.path())?
        .to_path_buf();
    let base_python = base_python.path().to_path_buf();

    let custom_python_path = context.temp_dir.join(
        base_python
            .file_name()
            .expect("managed Python 3.12 interpreter has a filename"),
    );
    uv_fs::copy_dir_all(&base_python, &custom_python_path)?;

    let no_system_venv = context.temp_dir.join("no-system-venv");

    let custom_python_3_12 = custom_python_path.join(relative_executable);
    let empty_requirements = context.temp_dir.child("empty-requirements.txt");
    empty_requirements.write_str("")?;

    uv_snapshot!(context.filters(), context
        .venv()
        .arg("--python")
        .arg(&custom_python_3_12)
        .arg(&no_system_venv), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: cpython-3.12.[X]-[PLATFORM]/[INSTALL-BIN]/[PYTHON]
    Creating virtual environment at: no-system-venv
    Activate with: source no-system-venv/[BIN]/activate
    ");

    uv_snapshot!(context.filters(), context
        .pip_install()
        .arg("--system")
        // Don't do this at home
        .arg("--break-system-packages")
        .arg("--python")
        .arg(&custom_python_3_12)
        .arg("anyio"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using Python 3.12.[X] environment at: cpython-3.12.[X]-[PLATFORM]
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + sniffio==1.3.1
    ");

    uv_snapshot!(context.filters(), context
        .pip_install()
        .arg("--python")
        .arg(&no_system_venv)
        .arg("markupsafe"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using Python 3.12.[X] environment at: no-system-venv
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + markupsafe==2.1.5
    ");

    uv_snapshot!(context.filters(), context
        .pip_list()
        .arg("--python")
        .arg(&custom_python_3_12), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    Package Version
    ------- -------
    anyio   4.3.0
    idna    3.6
    pip     [PIP_VERSION]
    sniffio 1.3.1

    ----- stderr -----
    Using Python 3.12.[X] environment at: cpython-3.12.[X]-[PLATFORM]
    ");

    uv_snapshot!(context.filters(), context
        .pip_list()
        .arg("--python")
        .arg(&no_system_venv),
        @r"
    success: true
    exit_code: 0
    ----- stdout -----
    Package    Version
    ---------- -------
    markupsafe 2.1.5

    ----- stderr -----
    Using Python 3.12.[X] environment at: no-system-venv
    ");

    // Recreate the same environment with system site-packages enabled. Reusing the path verifies
    // that interpreter caching accounts for `pyvenv.cfg` changes.
    uv_snapshot!(context.filters(), context
        .venv()
        .arg("--clear")
        .arg("--system-site-packages")
        .arg("--python")
        .arg(&custom_python_3_12)
        .arg(&no_system_venv), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: cpython-3.12.[X]-[PLATFORM]/[INSTALL-BIN]/[PYTHON]
    Creating virtual environment at: no-system-venv
    Activate with: source no-system-venv/[BIN]/activate
    ");

    // Exact reconciliation must not treat inherited packages as extraneous.
    uv_snapshot!(context.filters(), context
        .pip_sync()
        .arg(empty_requirements.path())
        .arg("--allow-empty-requirements")
        .arg("--dry-run")
        .arg("--python")
        .arg(&no_system_venv), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: Requirements file `empty-requirements.txt` does not contain any dependencies
    Using Python 3.12.[X] environment at: no-system-venv
    Resolved in [TIME]
    Checked in [TIME]
    Would make no changes
    ");

    uv_snapshot!(context.filters(), context
        .pip_install()
        .arg("--python")
        .arg(&no_system_venv)
        .arg("markupsafe"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using Python 3.12.[X] environment at: no-system-venv
    Resolved 1 package in [TIME]
    Installed 1 package in [TIME]
     + markupsafe==2.1.5
    ");

    uv_snapshot!(context.filters(), context
        .pip_list()
        .arg("--python")
        .arg(&no_system_venv),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    Package    Version
    ---------- -------
    anyio      4.3.0
    idna       3.6
    markupsafe 2.1.5
    pip        [PIP_VERSION]
    sniffio    1.3.1

    ----- stderr -----
    Using Python 3.12.[X] environment at: no-system-venv
    ");

    // A matching inherited package satisfies an install request without creating a local copy.
    uv_snapshot!(context.filters(), context
        .pip_install()
        .arg("--python")
        .arg(&no_system_venv)
        .arg("anyio==4.3.0"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using Python 3.12.[X] environment at: no-system-venv
    Checked 1 package in [TIME]
    ");

    // A mismatched inherited package is shadowed by a local installation without uninstalling the
    // read-only base copy.
    uv_snapshot!(context.filters(), context
        .pip_install()
        .arg("--python")
        .arg(&no_system_venv)
        .arg("anyio==3.0.0"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using Python 3.12.[X] environment at: no-system-venv
    Resolved 3 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + anyio==3.0.0
    ");

    uv_snapshot!(context.filters(), context
        .pip_list()
        .arg("--python")
        .arg(&no_system_venv), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Package    Version
    ---------- -------
    anyio      3.0.0
    idna       3.6
    markupsafe 2.1.5
    pip        [PIP_VERSION]
    sniffio    1.3.1

    ----- stderr -----
    Using Python 3.12.[X] environment at: no-system-venv
    ");

    // Uninstall removes the local shadow, revealing the inherited package. A second uninstall does
    // not mutate the base interpreter.
    uv_snapshot!(context.filters(), context
        .pip_uninstall()
        .arg("--python")
        .arg(&no_system_venv)
        .arg("anyio"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using Python 3.12.[X] environment at: no-system-venv
    Uninstalled 1 package in [TIME]
     - anyio==3.0.0
    ");

    uv_snapshot!(context.filters(), context
        .pip_uninstall()
        .arg("--python")
        .arg(&no_system_venv)
        .arg("anyio"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using Python 3.12.[X] environment at: no-system-venv
    warning: Skipping anyio as it is not installed
    warning: No packages to uninstall
    ");

    uv_snapshot!(context.filters(), context
        .pip_list()
        .arg("--python")
        .arg(&no_system_venv), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Package    Version
    ---------- -------
    anyio      4.3.0
    idna       3.6
    markupsafe 2.1.5
    pip        [PIP_VERSION]
    sniffio    1.3.1

    ----- stderr -----
    Using Python 3.12.[X] environment at: no-system-venv
    ");

    Ok(())
}
