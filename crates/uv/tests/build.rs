#![cfg(all(feature = "python", feature = "pypi"))]

use anyhow::Result;
use assert_fs::prelude::*;
use common::{uv_snapshot, TestContext};
use predicates::prelude::predicate;

mod common;

#[test]
fn build() -> Result<()> {
    let context = TestContext::new("3.12");

    let project = context.temp_dir.child("project");

    let pyproject_toml = project.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    project.child("src").child("__init__.py").touch()?;

    // Build the specified path.
    uv_snapshot!(context.filters(), context.build().arg("project"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Successfully built project/dist/project-0.1.0.tar.gz and project/dist/project-0.1.0-py3-none-any.whl
    "###);

    // Build the current working directory.
    uv_snapshot!(context.filters(), context.build().current_dir(project.path()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Successfully built dist/project-0.1.0.tar.gz and dist/project-0.1.0-py3-none-any.whl
    "###);

    // Error if there's nothing to build.
    uv_snapshot!(context.filters(), context.build(), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: [TEMP_DIR]/ does not appear to be a Python project, as neither `pyproject.toml` nor `setup.py` are present in the directory
    "###);

    // Build to a specified path.
    uv_snapshot!(context.filters(), context.build().arg("--out-dir").arg("out").current_dir(project.path()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Successfully built out/project-0.1.0.tar.gz and out/project-0.1.0-py3-none-any.whl
    "###);

    project
        .child("out")
        .child("project-0.1.0.tar.gz")
        .assert(predicate::path::is_file());
    project
        .child("out")
        .child("project-0.1.0-py3-none-any.whl")
        .assert(predicate::path::is_file());

    Ok(())
}

#[test]
fn sdist() -> Result<()> {
    let context = TestContext::new("3.12");

    let project = context.temp_dir.child("project");

    let pyproject_toml = project.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    project.child("src").child("__init__.py").touch()?;

    // Build the specified path.
    uv_snapshot!(context.filters(), context.build().arg("--sdist").current_dir(&project), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Successfully built dist/project-0.1.0.tar.gz
    "###);

    Ok(())
}

#[test]
fn wheel() -> Result<()> {
    let context = TestContext::new("3.12");

    let project = context.temp_dir.child("project");

    let pyproject_toml = project.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    project.child("src").child("__init__.py").touch()?;

    // Build the specified path.
    uv_snapshot!(context.filters(), context.build().arg("--wheel").current_dir(&project), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Successfully built dist/project-0.1.0-py3-none-any.whl
    "###);

    Ok(())
}

#[test]
fn sdist_wheel() -> Result<()> {
    let context = TestContext::new("3.12");

    let project = context.temp_dir.child("project");

    let pyproject_toml = project.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    project.child("src").child("__init__.py").touch()?;

    // Build the specified path.
    uv_snapshot!(context.filters(), context.build().arg("--sdist").arg("--wheel").current_dir(&project), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Successfully built dist/project-0.1.0.tar.gz and dist/project-0.1.0-py3-none-any.whl
    "###);

    Ok(())
}
