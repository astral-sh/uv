use anyhow::Result;
use assert_cmd::assert::OutputAssertExt;
use assert_fs::prelude::*;
use indoc::formatdoc;
use url::Url;

use crate::common::{uv_snapshot, TestContext};

#[test]
fn project_with_no_license() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
    "#,
    )?;

    uv_snapshot!(context.filters(), context.license().arg("--universal"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    project: 0.1.0, Unknown License

    ----- stderr -----
    Resolved 1 package in [TIME]
    "###
    );

    // `uv tree` should update the lockfile
    let lock = context.read("uv.lock");
    assert!(!lock.is_empty());

    Ok(())
}

#[test]
fn project_with_trove_license() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        classifiers = [
            "License :: Other/Proprietary License"
        ]
    "#,
    )?;

    uv_snapshot!(context.filters(), context.license().arg("--universal"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    project: 0.1.0, Other/Proprietary License

    ----- stderr -----
    Resolved 1 package in [TIME]
    "###
    );

    // `uv tree` should update the lockfile
    let lock = context.read("uv.lock");
    assert!(!lock.is_empty());

    Ok(())
}

#[test]
fn project_with_trove_osi_license() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        classifiers = [
            "License :: OSI Approved"
        ]
    "#,
    )?;

    uv_snapshot!(context.filters(), context.license().arg("--universal"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    project: 0.1.0, OSI Approved

    ----- stderr -----
    Resolved 1 package in [TIME]
    "###
    );

    // `uv tree` should update the lockfile
    let lock = context.read("uv.lock");
    assert!(!lock.is_empty());

    Ok(())
}

#[test]
fn nested_dependencies() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "scikit-learn==1.4.1.post1"
        ]
        classifiers = [
            "License :: OSI Approved :: MIT License"
        ]
    "#,
    )?;

    uv_snapshot!(context.filters(), context.license().arg("--universal"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    project: 0.1.0, MIT License
    scikit-learn: 1.4.1.post1, BSD License
    joblib: 1.3.2, BSD License
    numpy: 1.26.4, BSD License
    scipy: 1.12.0, BSD License
    threadpoolctl: 3.4.0, BSD License

    ----- stderr -----
    Resolved 6 packages in [TIME]
    "###
    );

    // `uv tree` should update the lockfile
    let lock = context.read("uv.lock");
    assert!(!lock.is_empty());

    Ok(())
}
