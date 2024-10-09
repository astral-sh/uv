use anyhow::Result;
use assert_fs::fixture::ChildPath;
use assert_fs::fixture::FileWriteStr;
use assert_fs::fixture::PathChild;
use assert_fs::prelude::*;

use common::uv_snapshot;

use crate::common::TestContext;

mod common;

#[test]
fn list_empty_columns() {
    let context = TestContext::new("3.12");

    uv_snapshot!(context.pip_list()
        .arg("--format")
        .arg("columns"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    "###
    );
}

#[test]
fn list_empty_freeze() {
    let context = TestContext::new("3.12");

    uv_snapshot!(context.pip_list()
        .arg("--format")
        .arg("freeze"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    "###
    );
}

#[test]
fn list_empty_json() {
    let context = TestContext::new("3.12");

    uv_snapshot!(context.pip_list()
        .arg("--format")
        .arg("json"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    []

    ----- stderr -----
    "###
    );
}

#[test]
fn list_single_no_editable() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("MarkupSafe==2.1.3")?;

    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + markupsafe==2.1.3
    "###
    );

    context.assert_command("import markupsafe").success();

    uv_snapshot!(context.pip_list(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Package    Version
    ---------- -------
    markupsafe 2.1.3

    ----- stderr -----
    "###
    );

    Ok(())
}

#[test]
fn list_editable() {
    let context = TestContext::new("3.12");

    // Install the editable package.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-e")
        .arg(context.workspace_root.join("scripts/packages/poetry_editable")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + poetry-editable==0.1.0 (from file://[WORKSPACE]/scripts/packages/poetry_editable)
     + sniffio==1.3.1
    "###
    );

    let filters = context
        .filters()
        .into_iter()
        .chain(vec![(r"\-\-\-\-\-\-+.*", "[UNDERLINE]"), ("  +", " ")])
        .collect::<Vec<_>>();

    uv_snapshot!(filters, context.pip_list(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Package Version Editable project location
    [UNDERLINE]
    anyio 4.3.0
    idna 3.6
    poetry-editable 0.1.0 [WORKSPACE]/scripts/packages/poetry_editable
    sniffio 1.3.1

    ----- stderr -----
    "###
    );
}

#[test]
fn list_editable_only() {
    let context = TestContext::new("3.12");

    // Install the editable package.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-e")
        .arg(context.workspace_root.join("scripts/packages/poetry_editable")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + poetry-editable==0.1.0 (from file://[WORKSPACE]/scripts/packages/poetry_editable)
     + sniffio==1.3.1
    "###
    );

    let filters = context
        .filters()
        .into_iter()
        .chain(vec![(r"\-\-\-\-\-\-+.*", "[UNDERLINE]"), ("  +", " ")])
        .collect::<Vec<_>>();

    uv_snapshot!(filters, context.pip_list()
        .arg("--editable"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Package Version Editable project location
    [UNDERLINE]
    poetry-editable 0.1.0 [WORKSPACE]/scripts/packages/poetry_editable

    ----- stderr -----
    "###
    );

    uv_snapshot!(filters, context.pip_list()
        .arg("--exclude-editable"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Package Version
    [UNDERLINE]
    anyio 4.3.0
    idna 3.6
    sniffio 1.3.1

    ----- stderr -----
    "###
    );

    uv_snapshot!(filters, context.pip_list()
        .arg("--editable")
        .arg("--exclude-editable"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: the argument '--editable' cannot be used with '--exclude-editable'

    Usage: uv pip list --cache-dir [CACHE_DIR] --editable

    For more information, try '--help'.
    "###
    );
}

#[test]
fn list_exclude() {
    let context = TestContext::new("3.12");

    // Install the editable package.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-e")
        .arg(context.workspace_root.join("scripts/packages/poetry_editable")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + poetry-editable==0.1.0 (from file://[WORKSPACE]/scripts/packages/poetry_editable)
     + sniffio==1.3.1
    "###
    );

    let filters = context
        .filters()
        .into_iter()
        .chain(vec![(r"\-\-\-\-\-\-+.*", "[UNDERLINE]"), ("  +", " ")])
        .collect::<Vec<_>>();

    uv_snapshot!(filters, context.pip_list()
    .arg("--exclude")
    .arg("numpy"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Package Version Editable project location
    [UNDERLINE]
    anyio 4.3.0
    idna 3.6
    poetry-editable 0.1.0 [WORKSPACE]/scripts/packages/poetry_editable
    sniffio 1.3.1

    ----- stderr -----
    "###
    );

    uv_snapshot!(filters, context.pip_list()
    .arg("--exclude")
    .arg("poetry-editable"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Package Version
    [UNDERLINE]
    anyio 4.3.0
    idna 3.6
    sniffio 1.3.1

    ----- stderr -----
    "###
    );

    uv_snapshot!(filters, context.pip_list()
    .arg("--exclude")
    .arg("numpy")
    .arg("--exclude")
    .arg("poetry-editable"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Package Version
    [UNDERLINE]
    anyio 4.3.0
    idna 3.6
    sniffio 1.3.1

    ----- stderr -----
    "###
    );
}

#[test]
#[cfg(not(windows))]
fn list_format_json() {
    let context = TestContext::new("3.12");

    // Install the editable package.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-e")
        .arg(context.workspace_root.join("scripts/packages/poetry_editable")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + poetry-editable==0.1.0 (from file://[WORKSPACE]/scripts/packages/poetry_editable)
     + sniffio==1.3.1
    "###
    );

    let filters: Vec<_> = context
        .filters()
        .into_iter()
        .chain(vec![(r"\-\-\-\-\-\-+.*", "[UNDERLINE]"), ("  +", " ")])
        .collect();

    uv_snapshot!(filters, context.pip_list()
    .arg("--format=json"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [{"name":"anyio","version":"4.3.0"},{"name":"idna","version":"3.6"},{"name":"poetry-editable","version":"0.1.0","editable_project_location":"[WORKSPACE]/scripts/packages/poetry_editable"},{"name":"sniffio","version":"1.3.1"}]

    ----- stderr -----
    "###
    );

    uv_snapshot!(filters, context.pip_list()
    .arg("--format=json")
    .arg("--editable"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [{"name":"poetry-editable","version":"0.1.0","editable_project_location":"[WORKSPACE]/scripts/packages/poetry_editable"}]

    ----- stderr -----
    "###
    );

    uv_snapshot!(filters, context.pip_list()
    .arg("--format=json")
    .arg("--exclude-editable"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [{"name":"anyio","version":"4.3.0"},{"name":"idna","version":"3.6"},{"name":"sniffio","version":"1.3.1"}]

    ----- stderr -----
    "###
    );
}

#[test]
fn list_format_freeze() {
    let context = TestContext::new("3.12");

    // Install the editable package.
    uv_snapshot!(context.filters(), context
        .pip_install()
        .arg("-e")
        .arg(context.workspace_root.join("scripts/packages/poetry_editable")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + poetry-editable==0.1.0 (from file://[WORKSPACE]/scripts/packages/poetry_editable)
     + sniffio==1.3.1
    "###
    );

    let filters = context
        .filters()
        .into_iter()
        .chain(vec![(r"\-\-\-\-\-\-+.*", "[UNDERLINE]"), ("  +", " ")])
        .collect::<Vec<_>>();

    uv_snapshot!(filters, context.pip_list()
    .arg("--format=freeze"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    anyio==4.3.0
    idna==3.6
    poetry-editable==0.1.0
    sniffio==1.3.1

    ----- stderr -----
    "###
    );

    uv_snapshot!(filters, context.pip_list()
    .arg("--format=freeze")
    .arg("--editable"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    poetry-editable==0.1.0

    ----- stderr -----
    "###
    );

    uv_snapshot!(filters, context.pip_list()
    .arg("--format=freeze")
    .arg("--exclude-editable"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    anyio==4.3.0
    idna==3.6
    sniffio==1.3.1

    ----- stderr -----
    "###
    );
}

#[test]
fn list_legacy_editable() -> Result<()> {
    let context = TestContext::new("3.12");

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
        .arg("--editable"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Package Version Editable project location
    [UNDERLINE]
    zstandard 0.22.0 [TEMP_DIR]/zstandard_project

    ----- stderr -----
    "###
    );

    Ok(())
}

#[test]
fn list_legacy_editable_invalid_version() -> Result<()> {
    let context = TestContext::new("3.12");

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
        .arg("--editable"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to read metadata from: `[SITE_PACKAGES]/paramiko.egg-link`
     Caused by: after parsing `0.1-b`, found `ulbasaur`, which is not part of a valid version
    "###
    );

    Ok(())
}
