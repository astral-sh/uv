#![cfg(all(feature = "python", feature = "pypi"))]

use anyhow::Result;
use assert_fs::prelude::*;
use common::{uv_snapshot, TestContext};
use predicates::prelude::predicate;

mod common;

#[test]
fn build() -> Result<()> {
    let context = TestContext::new("3.12");
    let filters = context
        .filters()
        .into_iter()
        .chain([
            (r"exit code: 1", "exit status: 1"),
            (r"bdist\.[^/\\\s]+-[^/\\\s]+", "bdist.linux-x86_64"),
            (r"\\\.", ""),
        ])
        .collect::<Vec<_>>();

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
    project.child("README").touch()?;

    // Build the specified path.
    uv_snapshot!(&filters, context.build().arg("project"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Building source distribution...
    running egg_info
    creating src/project.egg-info
    writing src/project.egg-info/PKG-INFO
    writing dependency_links to src/project.egg-info/dependency_links.txt
    writing requirements to src/project.egg-info/requires.txt
    writing top-level names to src/project.egg-info/top_level.txt
    writing manifest file 'src/project.egg-info/SOURCES.txt'
    reading manifest file 'src/project.egg-info/SOURCES.txt'
    writing manifest file 'src/project.egg-info/SOURCES.txt'
    running sdist
    running egg_info
    writing src/project.egg-info/PKG-INFO
    writing dependency_links to src/project.egg-info/dependency_links.txt
    writing requirements to src/project.egg-info/requires.txt
    writing top-level names to src/project.egg-info/top_level.txt
    reading manifest file 'src/project.egg-info/SOURCES.txt'
    writing manifest file 'src/project.egg-info/SOURCES.txt'
    running check
    creating project-0.1.0
    creating project-0.1.0/src
    creating project-0.1.0/src/project.egg-info
    copying files to project-0.1.0...
    copying README -> project-0.1.0
    copying pyproject.toml -> project-0.1.0
    copying src/__init__.py -> project-0.1.0/src
    copying src/project.egg-info/PKG-INFO -> project-0.1.0/src/project.egg-info
    copying src/project.egg-info/SOURCES.txt -> project-0.1.0/src/project.egg-info
    copying src/project.egg-info/dependency_links.txt -> project-0.1.0/src/project.egg-info
    copying src/project.egg-info/requires.txt -> project-0.1.0/src/project.egg-info
    copying src/project.egg-info/top_level.txt -> project-0.1.0/src/project.egg-info
    copying src/project.egg-info/SOURCES.txt -> project-0.1.0/src/project.egg-info
    Writing project-0.1.0/setup.cfg
    Creating tar archive
    removing 'project-0.1.0' (and everything under it)
    Building wheel from source distribution...
    running egg_info
    writing src/project.egg-info/PKG-INFO
    writing dependency_links to src/project.egg-info/dependency_links.txt
    writing requirements to src/project.egg-info/requires.txt
    writing top-level names to src/project.egg-info/top_level.txt
    reading manifest file 'src/project.egg-info/SOURCES.txt'
    writing manifest file 'src/project.egg-info/SOURCES.txt'
    running bdist_wheel
    running build
    running build_py
    creating build
    creating build/lib
    copying src/__init__.py -> build/lib
    running egg_info
    writing src/project.egg-info/PKG-INFO
    writing dependency_links to src/project.egg-info/dependency_links.txt
    writing requirements to src/project.egg-info/requires.txt
    writing top-level names to src/project.egg-info/top_level.txt
    reading manifest file 'src/project.egg-info/SOURCES.txt'
    writing manifest file 'src/project.egg-info/SOURCES.txt'
    installing to build/bdist.linux-x86_64/wheel
    running install
    running install_lib
    creating build/bdist.linux-x86_64
    creating build/bdist.linux-x86_64/wheel
    copying build/lib/__init__.py -> build/bdist.linux-x86_64/wheel
    running install_egg_info
    Copying src/project.egg-info to build/bdist.linux-x86_64/wheel/project-0.1.0-py3.12.egg-info
    running install_scripts
    creating build/bdist.linux-x86_64/wheel/project-0.1.0.dist-info/WHEEL
    creating '[TEMP_DIR]/project/dist/[TMP]/wheel' to it
    adding '__init__.py'
    adding 'project-0.1.0.dist-info/METADATA'
    adding 'project-0.1.0.dist-info/WHEEL'
    adding 'project-0.1.0.dist-info/top_level.txt'
    adding 'project-0.1.0.dist-info/RECORD'
    removing build/bdist.linux-x86_64/wheel
    Successfully built project/dist/project-0.1.0.tar.gz and project/dist/project-0.1.0-py3-none-any.whl
    "###);

    project
        .child("dist")
        .child("project-0.1.0.tar.gz")
        .assert(predicate::path::is_file());
    project
        .child("dist")
        .child("project-0.1.0-py3-none-any.whl")
        .assert(predicate::path::is_file());

    fs_err::remove_dir_all(project.child("dist"))?;

    // Build the current working directory.
    uv_snapshot!(&filters, context.build().current_dir(project.path()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Building source distribution...
    running egg_info
    writing src/project.egg-info/PKG-INFO
    writing dependency_links to src/project.egg-info/dependency_links.txt
    writing requirements to src/project.egg-info/requires.txt
    writing top-level names to src/project.egg-info/top_level.txt
    reading manifest file 'src/project.egg-info/SOURCES.txt'
    writing manifest file 'src/project.egg-info/SOURCES.txt'
    running sdist
    running egg_info
    writing src/project.egg-info/PKG-INFO
    writing dependency_links to src/project.egg-info/dependency_links.txt
    writing requirements to src/project.egg-info/requires.txt
    writing top-level names to src/project.egg-info/top_level.txt
    reading manifest file 'src/project.egg-info/SOURCES.txt'
    writing manifest file 'src/project.egg-info/SOURCES.txt'
    running check
    creating project-0.1.0
    creating project-0.1.0/src
    creating project-0.1.0/src/project.egg-info
    copying files to project-0.1.0...
    copying README -> project-0.1.0
    copying pyproject.toml -> project-0.1.0
    copying src/__init__.py -> project-0.1.0/src
    copying src/project.egg-info/PKG-INFO -> project-0.1.0/src/project.egg-info
    copying src/project.egg-info/SOURCES.txt -> project-0.1.0/src/project.egg-info
    copying src/project.egg-info/dependency_links.txt -> project-0.1.0/src/project.egg-info
    copying src/project.egg-info/requires.txt -> project-0.1.0/src/project.egg-info
    copying src/project.egg-info/top_level.txt -> project-0.1.0/src/project.egg-info
    copying src/project.egg-info/SOURCES.txt -> project-0.1.0/src/project.egg-info
    Writing project-0.1.0/setup.cfg
    Creating tar archive
    removing 'project-0.1.0' (and everything under it)
    Building wheel from source distribution...
    running egg_info
    writing src/project.egg-info/PKG-INFO
    writing dependency_links to src/project.egg-info/dependency_links.txt
    writing requirements to src/project.egg-info/requires.txt
    writing top-level names to src/project.egg-info/top_level.txt
    reading manifest file 'src/project.egg-info/SOURCES.txt'
    writing manifest file 'src/project.egg-info/SOURCES.txt'
    running bdist_wheel
    running build
    running build_py
    creating build
    creating build/lib
    copying src/__init__.py -> build/lib
    running egg_info
    writing src/project.egg-info/PKG-INFO
    writing dependency_links to src/project.egg-info/dependency_links.txt
    writing requirements to src/project.egg-info/requires.txt
    writing top-level names to src/project.egg-info/top_level.txt
    reading manifest file 'src/project.egg-info/SOURCES.txt'
    writing manifest file 'src/project.egg-info/SOURCES.txt'
    installing to build/bdist.linux-x86_64/wheel
    running install
    running install_lib
    creating build/bdist.linux-x86_64
    creating build/bdist.linux-x86_64/wheel
    copying build/lib/__init__.py -> build/bdist.linux-x86_64/wheel
    running install_egg_info
    Copying src/project.egg-info to build/bdist.linux-x86_64/wheel/project-0.1.0-py3.12.egg-info
    running install_scripts
    creating build/bdist.linux-x86_64/wheel/project-0.1.0.dist-info/WHEEL
    creating '[TEMP_DIR]/project/dist/[TMP]/wheel' to it
    adding '__init__.py'
    adding 'project-0.1.0.dist-info/METADATA'
    adding 'project-0.1.0.dist-info/WHEEL'
    adding 'project-0.1.0.dist-info/top_level.txt'
    adding 'project-0.1.0.dist-info/RECORD'
    removing build/bdist.linux-x86_64/wheel
    Successfully built dist/project-0.1.0.tar.gz and dist/project-0.1.0-py3-none-any.whl
    "###);

    project
        .child("dist")
        .child("project-0.1.0.tar.gz")
        .assert(predicate::path::is_file());
    project
        .child("dist")
        .child("project-0.1.0-py3-none-any.whl")
        .assert(predicate::path::is_file());

    fs_err::remove_dir_all(project.child("dist"))?;

    // Error if there's nothing to build.
    uv_snapshot!(&filters, context.build(), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Building source distribution...
    error: [TEMP_DIR]/ does not appear to be a Python project, as neither `pyproject.toml` nor `setup.py` are present in the directory

    "###);

    // Build to a specified path.
    uv_snapshot!(&filters, context.build().arg("--out-dir").arg("out").current_dir(project.path()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Building source distribution...
    running egg_info
    writing src/project.egg-info/PKG-INFO
    writing dependency_links to src/project.egg-info/dependency_links.txt
    writing requirements to src/project.egg-info/requires.txt
    writing top-level names to src/project.egg-info/top_level.txt
    reading manifest file 'src/project.egg-info/SOURCES.txt'
    writing manifest file 'src/project.egg-info/SOURCES.txt'
    running sdist
    running egg_info
    writing src/project.egg-info/PKG-INFO
    writing dependency_links to src/project.egg-info/dependency_links.txt
    writing requirements to src/project.egg-info/requires.txt
    writing top-level names to src/project.egg-info/top_level.txt
    reading manifest file 'src/project.egg-info/SOURCES.txt'
    writing manifest file 'src/project.egg-info/SOURCES.txt'
    running check
    creating project-0.1.0
    creating project-0.1.0/src
    creating project-0.1.0/src/project.egg-info
    copying files to project-0.1.0...
    copying README -> project-0.1.0
    copying pyproject.toml -> project-0.1.0
    copying src/__init__.py -> project-0.1.0/src
    copying src/project.egg-info/PKG-INFO -> project-0.1.0/src/project.egg-info
    copying src/project.egg-info/SOURCES.txt -> project-0.1.0/src/project.egg-info
    copying src/project.egg-info/dependency_links.txt -> project-0.1.0/src/project.egg-info
    copying src/project.egg-info/requires.txt -> project-0.1.0/src/project.egg-info
    copying src/project.egg-info/top_level.txt -> project-0.1.0/src/project.egg-info
    copying src/project.egg-info/SOURCES.txt -> project-0.1.0/src/project.egg-info
    Writing project-0.1.0/setup.cfg
    Creating tar archive
    removing 'project-0.1.0' (and everything under it)
    Building wheel from source distribution...
    running egg_info
    writing src/project.egg-info/PKG-INFO
    writing dependency_links to src/project.egg-info/dependency_links.txt
    writing requirements to src/project.egg-info/requires.txt
    writing top-level names to src/project.egg-info/top_level.txt
    reading manifest file 'src/project.egg-info/SOURCES.txt'
    writing manifest file 'src/project.egg-info/SOURCES.txt'
    running bdist_wheel
    running build
    running build_py
    creating build
    creating build/lib
    copying src/__init__.py -> build/lib
    running egg_info
    writing src/project.egg-info/PKG-INFO
    writing dependency_links to src/project.egg-info/dependency_links.txt
    writing requirements to src/project.egg-info/requires.txt
    writing top-level names to src/project.egg-info/top_level.txt
    reading manifest file 'src/project.egg-info/SOURCES.txt'
    writing manifest file 'src/project.egg-info/SOURCES.txt'
    installing to build/bdist.linux-x86_64/wheel
    running install
    running install_lib
    creating build/bdist.linux-x86_64
    creating build/bdist.linux-x86_64/wheel
    copying build/lib/__init__.py -> build/bdist.linux-x86_64/wheel
    running install_egg_info
    Copying src/project.egg-info to build/bdist.linux-x86_64/wheel/project-0.1.0-py3.12.egg-info
    running install_scripts
    creating build/bdist.linux-x86_64/wheel/project-0.1.0.dist-info/WHEEL
    creating '[TEMP_DIR]/project/out/[TMP]/wheel' to it
    adding '__init__.py'
    adding 'project-0.1.0.dist-info/METADATA'
    adding 'project-0.1.0.dist-info/WHEEL'
    adding 'project-0.1.0.dist-info/top_level.txt'
    adding 'project-0.1.0.dist-info/RECORD'
    removing build/bdist.linux-x86_64/wheel
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
    let filters = context
        .filters()
        .into_iter()
        .chain([
            (r"exit code: 1", "exit status: 1"),
            (r"bdist\.[^/\\\s]+-[^/\\\s]+", "bdist.linux-x86_64"),
            (r"\\\.", ""),
        ])
        .collect::<Vec<_>>();

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
    project.child("README").touch()?;

    // Build the specified path.
    uv_snapshot!(&filters, context.build().arg("--sdist").current_dir(&project), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Building source distribution...
    running egg_info
    creating src/project.egg-info
    writing src/project.egg-info/PKG-INFO
    writing dependency_links to src/project.egg-info/dependency_links.txt
    writing requirements to src/project.egg-info/requires.txt
    writing top-level names to src/project.egg-info/top_level.txt
    writing manifest file 'src/project.egg-info/SOURCES.txt'
    reading manifest file 'src/project.egg-info/SOURCES.txt'
    writing manifest file 'src/project.egg-info/SOURCES.txt'
    running sdist
    running egg_info
    writing src/project.egg-info/PKG-INFO
    writing dependency_links to src/project.egg-info/dependency_links.txt
    writing requirements to src/project.egg-info/requires.txt
    writing top-level names to src/project.egg-info/top_level.txt
    reading manifest file 'src/project.egg-info/SOURCES.txt'
    writing manifest file 'src/project.egg-info/SOURCES.txt'
    running check
    creating project-0.1.0
    creating project-0.1.0/src
    creating project-0.1.0/src/project.egg-info
    copying files to project-0.1.0...
    copying README -> project-0.1.0
    copying pyproject.toml -> project-0.1.0
    copying src/__init__.py -> project-0.1.0/src
    copying src/project.egg-info/PKG-INFO -> project-0.1.0/src/project.egg-info
    copying src/project.egg-info/SOURCES.txt -> project-0.1.0/src/project.egg-info
    copying src/project.egg-info/dependency_links.txt -> project-0.1.0/src/project.egg-info
    copying src/project.egg-info/requires.txt -> project-0.1.0/src/project.egg-info
    copying src/project.egg-info/top_level.txt -> project-0.1.0/src/project.egg-info
    copying src/project.egg-info/SOURCES.txt -> project-0.1.0/src/project.egg-info
    Writing project-0.1.0/setup.cfg
    Creating tar archive
    removing 'project-0.1.0' (and everything under it)
    Successfully built dist/project-0.1.0.tar.gz
    "###);

    project
        .child("dist")
        .child("project-0.1.0.tar.gz")
        .assert(predicate::path::is_file());
    project
        .child("dist")
        .child("project-0.1.0-py3-none-any.whl")
        .assert(predicate::path::missing());

    Ok(())
}

#[test]
fn wheel() -> Result<()> {
    let context = TestContext::new("3.12");
    let filters = context
        .filters()
        .into_iter()
        .chain([
            (r"exit code: 1", "exit status: 1"),
            (r"bdist\.[^/\\\s]+-[^/\\\s]+", "bdist.linux-x86_64"),
            (r"\\\.", ""),
        ])
        .collect::<Vec<_>>();

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
    project.child("README").touch()?;

    // Build the specified path.
    uv_snapshot!(&filters, context.build().arg("--wheel").current_dir(&project), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Building wheel...
    running egg_info
    creating src/project.egg-info
    writing src/project.egg-info/PKG-INFO
    writing dependency_links to src/project.egg-info/dependency_links.txt
    writing requirements to src/project.egg-info/requires.txt
    writing top-level names to src/project.egg-info/top_level.txt
    writing manifest file 'src/project.egg-info/SOURCES.txt'
    reading manifest file 'src/project.egg-info/SOURCES.txt'
    writing manifest file 'src/project.egg-info/SOURCES.txt'
    running bdist_wheel
    running build
    running build_py
    creating build
    creating build/lib
    copying src/__init__.py -> build/lib
    running egg_info
    writing src/project.egg-info/PKG-INFO
    writing dependency_links to src/project.egg-info/dependency_links.txt
    writing requirements to src/project.egg-info/requires.txt
    writing top-level names to src/project.egg-info/top_level.txt
    reading manifest file 'src/project.egg-info/SOURCES.txt'
    writing manifest file 'src/project.egg-info/SOURCES.txt'
    installing to build/bdist.linux-x86_64/wheel
    running install
    running install_lib
    creating build/bdist.linux-x86_64
    creating build/bdist.linux-x86_64/wheel
    copying build/lib/__init__.py -> build/bdist.linux-x86_64/wheel
    running install_egg_info
    Copying src/project.egg-info to build/bdist.linux-x86_64/wheel/project-0.1.0-py3.12.egg-info
    running install_scripts
    creating build/bdist.linux-x86_64/wheel/project-0.1.0.dist-info/WHEEL
    creating '[TEMP_DIR]/project/dist/[TMP]/wheel' to it
    adding '__init__.py'
    adding 'project-0.1.0.dist-info/METADATA'
    adding 'project-0.1.0.dist-info/WHEEL'
    adding 'project-0.1.0.dist-info/top_level.txt'
    adding 'project-0.1.0.dist-info/RECORD'
    removing build/bdist.linux-x86_64/wheel
    Successfully built dist/project-0.1.0-py3-none-any.whl
    "###);

    project
        .child("dist")
        .child("project-0.1.0.tar.gz")
        .assert(predicate::path::missing());
    project
        .child("dist")
        .child("project-0.1.0-py3-none-any.whl")
        .assert(predicate::path::is_file());

    Ok(())
}

#[test]
fn sdist_wheel() -> Result<()> {
    let context = TestContext::new("3.12");
    let filters = context
        .filters()
        .into_iter()
        .chain([
            (r"exit code: 1", "exit status: 1"),
            (r"bdist\.[^/\\\s]+-[^/\\\s]+", "bdist.linux-x86_64"),
            (r"\\\.", ""),
        ])
        .collect::<Vec<_>>();

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
    project.child("README").touch()?;

    // Build the specified path.
    uv_snapshot!(&filters, context.build().arg("--sdist").arg("--wheel").current_dir(&project), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Building source distribution...
    running egg_info
    creating src/project.egg-info
    writing src/project.egg-info/PKG-INFO
    writing dependency_links to src/project.egg-info/dependency_links.txt
    writing requirements to src/project.egg-info/requires.txt
    writing top-level names to src/project.egg-info/top_level.txt
    writing manifest file 'src/project.egg-info/SOURCES.txt'
    reading manifest file 'src/project.egg-info/SOURCES.txt'
    writing manifest file 'src/project.egg-info/SOURCES.txt'
    running sdist
    running egg_info
    writing src/project.egg-info/PKG-INFO
    writing dependency_links to src/project.egg-info/dependency_links.txt
    writing requirements to src/project.egg-info/requires.txt
    writing top-level names to src/project.egg-info/top_level.txt
    reading manifest file 'src/project.egg-info/SOURCES.txt'
    writing manifest file 'src/project.egg-info/SOURCES.txt'
    running check
    creating project-0.1.0
    creating project-0.1.0/src
    creating project-0.1.0/src/project.egg-info
    copying files to project-0.1.0...
    copying README -> project-0.1.0
    copying pyproject.toml -> project-0.1.0
    copying src/__init__.py -> project-0.1.0/src
    copying src/project.egg-info/PKG-INFO -> project-0.1.0/src/project.egg-info
    copying src/project.egg-info/SOURCES.txt -> project-0.1.0/src/project.egg-info
    copying src/project.egg-info/dependency_links.txt -> project-0.1.0/src/project.egg-info
    copying src/project.egg-info/requires.txt -> project-0.1.0/src/project.egg-info
    copying src/project.egg-info/top_level.txt -> project-0.1.0/src/project.egg-info
    copying src/project.egg-info/SOURCES.txt -> project-0.1.0/src/project.egg-info
    Writing project-0.1.0/setup.cfg
    Creating tar archive
    removing 'project-0.1.0' (and everything under it)
    Building wheel...
    running egg_info
    writing src/project.egg-info/PKG-INFO
    writing dependency_links to src/project.egg-info/dependency_links.txt
    writing requirements to src/project.egg-info/requires.txt
    writing top-level names to src/project.egg-info/top_level.txt
    reading manifest file 'src/project.egg-info/SOURCES.txt'
    writing manifest file 'src/project.egg-info/SOURCES.txt'
    running bdist_wheel
    running build
    running build_py
    creating build
    creating build/lib
    copying src/__init__.py -> build/lib
    running egg_info
    writing src/project.egg-info/PKG-INFO
    writing dependency_links to src/project.egg-info/dependency_links.txt
    writing requirements to src/project.egg-info/requires.txt
    writing top-level names to src/project.egg-info/top_level.txt
    reading manifest file 'src/project.egg-info/SOURCES.txt'
    writing manifest file 'src/project.egg-info/SOURCES.txt'
    installing to build/bdist.linux-x86_64/wheel
    running install
    running install_lib
    creating build/bdist.linux-x86_64
    creating build/bdist.linux-x86_64/wheel
    copying build/lib/__init__.py -> build/bdist.linux-x86_64/wheel
    running install_egg_info
    Copying src/project.egg-info to build/bdist.linux-x86_64/wheel/project-0.1.0-py3.12.egg-info
    running install_scripts
    creating build/bdist.linux-x86_64/wheel/project-0.1.0.dist-info/WHEEL
    creating '[TEMP_DIR]/project/dist/[TMP]/wheel' to it
    adding '__init__.py'
    adding 'project-0.1.0.dist-info/METADATA'
    adding 'project-0.1.0.dist-info/WHEEL'
    adding 'project-0.1.0.dist-info/top_level.txt'
    adding 'project-0.1.0.dist-info/RECORD'
    removing build/bdist.linux-x86_64/wheel
    Successfully built dist/project-0.1.0.tar.gz and dist/project-0.1.0-py3-none-any.whl
    "###);

    project
        .child("dist")
        .child("project-0.1.0.tar.gz")
        .assert(predicate::path::is_file());
    project
        .child("dist")
        .child("project-0.1.0-py3-none-any.whl")
        .assert(predicate::path::is_file());

    Ok(())
}

#[test]
fn wheel_from_sdist() -> Result<()> {
    let context = TestContext::new("3.12");
    let filters = context
        .filters()
        .into_iter()
        .chain([
            (r"exit code: 1", "exit status: 1"),
            (r"bdist\.[^/\\\s]+-[^/\\\s]+", "bdist.linux-x86_64"),
            (r"\\\.", ""),
        ])
        .collect::<Vec<_>>();

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
    project.child("README").touch()?;

    // Build the sdist.
    uv_snapshot!(&filters, context.build().arg("--sdist").current_dir(&project), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Building source distribution...
    running egg_info
    creating src/project.egg-info
    writing src/project.egg-info/PKG-INFO
    writing dependency_links to src/project.egg-info/dependency_links.txt
    writing requirements to src/project.egg-info/requires.txt
    writing top-level names to src/project.egg-info/top_level.txt
    writing manifest file 'src/project.egg-info/SOURCES.txt'
    reading manifest file 'src/project.egg-info/SOURCES.txt'
    writing manifest file 'src/project.egg-info/SOURCES.txt'
    running sdist
    running egg_info
    writing src/project.egg-info/PKG-INFO
    writing dependency_links to src/project.egg-info/dependency_links.txt
    writing requirements to src/project.egg-info/requires.txt
    writing top-level names to src/project.egg-info/top_level.txt
    reading manifest file 'src/project.egg-info/SOURCES.txt'
    writing manifest file 'src/project.egg-info/SOURCES.txt'
    running check
    creating project-0.1.0
    creating project-0.1.0/src
    creating project-0.1.0/src/project.egg-info
    copying files to project-0.1.0...
    copying README -> project-0.1.0
    copying pyproject.toml -> project-0.1.0
    copying src/__init__.py -> project-0.1.0/src
    copying src/project.egg-info/PKG-INFO -> project-0.1.0/src/project.egg-info
    copying src/project.egg-info/SOURCES.txt -> project-0.1.0/src/project.egg-info
    copying src/project.egg-info/dependency_links.txt -> project-0.1.0/src/project.egg-info
    copying src/project.egg-info/requires.txt -> project-0.1.0/src/project.egg-info
    copying src/project.egg-info/top_level.txt -> project-0.1.0/src/project.egg-info
    copying src/project.egg-info/SOURCES.txt -> project-0.1.0/src/project.egg-info
    Writing project-0.1.0/setup.cfg
    Creating tar archive
    removing 'project-0.1.0' (and everything under it)
    Successfully built dist/project-0.1.0.tar.gz
    "###);

    project
        .child("dist")
        .child("project-0.1.0.tar.gz")
        .assert(predicate::path::is_file());
    project
        .child("dist")
        .child("project-0.1.0-py3-none-any.whl")
        .assert(predicate::path::missing());

    // Error if `--wheel` is not specified.
    uv_snapshot!(&filters, context.build().arg("./dist/project-0.1.0.tar.gz").current_dir(&project), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Pass `--wheel` explicitly to build a wheel from a source distribution
    "###);

    // Error if `--sdist` is specified.
    uv_snapshot!(&filters, context.build().arg("./dist/project-0.1.0.tar.gz").arg("--sdist").current_dir(&project), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Building an `--sdist` from a source distribution is not supported
    "###);

    // Build the wheel from the sdist.
    uv_snapshot!(&filters, context.build().arg("./dist/project-0.1.0.tar.gz").arg("--wheel").current_dir(&project), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Building wheel from source distribution...
    running egg_info
    writing src/project.egg-info/PKG-INFO
    writing dependency_links to src/project.egg-info/dependency_links.txt
    writing requirements to src/project.egg-info/requires.txt
    writing top-level names to src/project.egg-info/top_level.txt
    reading manifest file 'src/project.egg-info/SOURCES.txt'
    writing manifest file 'src/project.egg-info/SOURCES.txt'
    running bdist_wheel
    running build
    running build_py
    creating build
    creating build/lib
    copying src/__init__.py -> build/lib
    running egg_info
    writing src/project.egg-info/PKG-INFO
    writing dependency_links to src/project.egg-info/dependency_links.txt
    writing requirements to src/project.egg-info/requires.txt
    writing top-level names to src/project.egg-info/top_level.txt
    reading manifest file 'src/project.egg-info/SOURCES.txt'
    writing manifest file 'src/project.egg-info/SOURCES.txt'
    installing to build/bdist.linux-x86_64/wheel
    running install
    running install_lib
    creating build/bdist.linux-x86_64
    creating build/bdist.linux-x86_64/wheel
    copying build/lib/__init__.py -> build/bdist.linux-x86_64/wheel
    running install_egg_info
    Copying src/project.egg-info to build/bdist.linux-x86_64/wheel/project-0.1.0-py3.12.egg-info
    running install_scripts
    creating build/bdist.linux-x86_64/wheel/project-0.1.0.dist-info/WHEEL
    creating '[TEMP_DIR]/project/dist/[TMP]/wheel' to it
    adding '__init__.py'
    adding 'project-0.1.0.dist-info/METADATA'
    adding 'project-0.1.0.dist-info/WHEEL'
    adding 'project-0.1.0.dist-info/top_level.txt'
    adding 'project-0.1.0.dist-info/RECORD'
    removing build/bdist.linux-x86_64/wheel
    Successfully built dist/project-0.1.0-py3-none-any.whl
    "###);

    project
        .child("dist")
        .child("project-0.1.0.tar.gz")
        .assert(predicate::path::is_file());
    project
        .child("dist")
        .child("project-0.1.0-py3-none-any.whl")
        .assert(predicate::path::is_file());

    // Passing a wheel is an error.
    uv_snapshot!(&filters, context.build().arg("./dist/project-0.1.0-py3-none-any.whl").arg("--wheel").current_dir(&project), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Building wheel from source distribution...
    error: `dist/project-0.1.0-py3-none-any.whl` is not a valid build source. Expected to receive a source directory, or a source distribution ending in one of: `.tar.gz`, `.zip`, `.tar.bz2`, `.tar.lz`, `.tar.lzma`, `.tar.xz`, `.tar.zst`, `.tar`, `.tbz`, `.tgz`, `.tlz`, or `.txz`.
    "###);

    Ok(())
}

#[test]
fn fail() -> Result<()> {
    let context = TestContext::new("3.12");
    let filters = context
        .filters()
        .into_iter()
        .chain([
            (r"exit code: 1", "exit status: 1"),
            (r"bdist\.[^/\\\s]+-[^/\\\s]+", "bdist.linux-x86_64"),
            (r"\\\.", ""),
        ])
        .collect::<Vec<_>>();

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
    project.child("README").touch()?;

    project.child("setup.py").write_str(
        r#"
        from setuptools import setup

        setup(
            name="project",
            version="0.1.0",
            packages=["project"],
            install_requires=["foo==3.7.0"],
        )
        "#,
    )?;

    // Build the specified path.
    uv_snapshot!(&filters, context.build().arg("project"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Building source distribution...
    Traceback (most recent call last):
      File "<string>", line 14, in <module>
      File "[CACHE_DIR]/builds-v0/[TMP]/build_meta.py", line 328, in get_requires_for_build_sdist
        return self._get_build_requires(config_settings, requirements=[])
               ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
      File "[CACHE_DIR]/builds-v0/[TMP]/build_meta.py", line 295, in _get_build_requires
        self.run_setup()
      File "[CACHE_DIR]/builds-v0/[TMP]/build_meta.py", line 311, in run_setup
        exec(code, locals())
      File "<string>", line 2
        from setuptools import setup
    IndentationError: unexpected indent
    error: Build backend failed to determine extra requires with `build_sdist()` with exit status: 1
    "###);

    Ok(())
}

#[test]
fn workspace() -> Result<()> {
    let context = TestContext::new("3.12");
    let filters = context
        .filters()
        .into_iter()
        .chain([
            (r"exit code: 1", "exit status: 1"),
            (r"bdist\.[^/\\\s]+-[^/\\\s]+", "bdist.linux-x86_64"),
            (r"\\\.", ""),
        ])
        .collect::<Vec<_>>();

    let project = context.temp_dir.child("project");

    let pyproject_toml = project.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0"]

        [tool.uv.workspace]
        members = ["packages/*"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    project.child("src").child("__init__.py").touch()?;
    project.child("README").touch()?;

    let member = project.child("packages").child("member");
    fs_err::create_dir_all(member.path())?;

    member.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "member"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    member.child("src").child("__init__.py").touch()?;
    member.child("README").touch()?;

    // Build the member.
    uv_snapshot!(&filters, context.build().arg("--package").arg("member").current_dir(&project), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Building source distribution...
    running egg_info
    creating src/member.egg-info
    writing src/member.egg-info/PKG-INFO
    writing dependency_links to src/member.egg-info/dependency_links.txt
    writing requirements to src/member.egg-info/requires.txt
    writing top-level names to src/member.egg-info/top_level.txt
    writing manifest file 'src/member.egg-info/SOURCES.txt'
    reading manifest file 'src/member.egg-info/SOURCES.txt'
    writing manifest file 'src/member.egg-info/SOURCES.txt'
    running sdist
    running egg_info
    writing src/member.egg-info/PKG-INFO
    writing dependency_links to src/member.egg-info/dependency_links.txt
    writing requirements to src/member.egg-info/requires.txt
    writing top-level names to src/member.egg-info/top_level.txt
    reading manifest file 'src/member.egg-info/SOURCES.txt'
    writing manifest file 'src/member.egg-info/SOURCES.txt'
    running check
    creating member-0.1.0
    creating member-0.1.0/src
    creating member-0.1.0/src/member.egg-info
    copying files to member-0.1.0...
    copying README -> member-0.1.0
    copying pyproject.toml -> member-0.1.0
    copying src/__init__.py -> member-0.1.0/src
    copying src/member.egg-info/PKG-INFO -> member-0.1.0/src/member.egg-info
    copying src/member.egg-info/SOURCES.txt -> member-0.1.0/src/member.egg-info
    copying src/member.egg-info/dependency_links.txt -> member-0.1.0/src/member.egg-info
    copying src/member.egg-info/requires.txt -> member-0.1.0/src/member.egg-info
    copying src/member.egg-info/top_level.txt -> member-0.1.0/src/member.egg-info
    copying src/member.egg-info/SOURCES.txt -> member-0.1.0/src/member.egg-info
    Writing member-0.1.0/setup.cfg
    Creating tar archive
    removing 'member-0.1.0' (and everything under it)
    Building wheel from source distribution...
    running egg_info
    writing src/member.egg-info/PKG-INFO
    writing dependency_links to src/member.egg-info/dependency_links.txt
    writing requirements to src/member.egg-info/requires.txt
    writing top-level names to src/member.egg-info/top_level.txt
    reading manifest file 'src/member.egg-info/SOURCES.txt'
    writing manifest file 'src/member.egg-info/SOURCES.txt'
    running bdist_wheel
    running build
    running build_py
    creating build
    creating build/lib
    copying src/__init__.py -> build/lib
    running egg_info
    writing src/member.egg-info/PKG-INFO
    writing dependency_links to src/member.egg-info/dependency_links.txt
    writing requirements to src/member.egg-info/requires.txt
    writing top-level names to src/member.egg-info/top_level.txt
    reading manifest file 'src/member.egg-info/SOURCES.txt'
    writing manifest file 'src/member.egg-info/SOURCES.txt'
    installing to build/bdist.linux-x86_64/wheel
    running install
    running install_lib
    creating build/bdist.linux-x86_64
    creating build/bdist.linux-x86_64/wheel
    copying build/lib/__init__.py -> build/bdist.linux-x86_64/wheel
    running install_egg_info
    Copying src/member.egg-info to build/bdist.linux-x86_64/wheel/member-0.1.0-py3.12.egg-info
    running install_scripts
    creating build/bdist.linux-x86_64/wheel/member-0.1.0.dist-info/WHEEL
    creating '[TEMP_DIR]/project/packages/member/dist/[TMP]/wheel' to it
    adding '__init__.py'
    adding 'member-0.1.0.dist-info/METADATA'
    adding 'member-0.1.0.dist-info/WHEEL'
    adding 'member-0.1.0.dist-info/top_level.txt'
    adding 'member-0.1.0.dist-info/RECORD'
    removing build/bdist.linux-x86_64/wheel
    Successfully built packages/member/dist/member-0.1.0.tar.gz and packages/member/dist/member-0.1.0-py3-none-any.whl
    "###);

    member
        .child("dist")
        .child("member-0.1.0.tar.gz")
        .assert(predicate::path::is_file());
    member
        .child("dist")
        .child("member-0.1.0-py3-none-any.whl")
        .assert(predicate::path::is_file());

    // If a source is provided, discover the workspace from the source.
    uv_snapshot!(&filters, context.build().arg("./project").arg("--package").arg("member"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Building source distribution...
    running egg_info
    writing src/member.egg-info/PKG-INFO
    writing dependency_links to src/member.egg-info/dependency_links.txt
    writing requirements to src/member.egg-info/requires.txt
    writing top-level names to src/member.egg-info/top_level.txt
    reading manifest file 'src/member.egg-info/SOURCES.txt'
    writing manifest file 'src/member.egg-info/SOURCES.txt'
    running sdist
    running egg_info
    writing src/member.egg-info/PKG-INFO
    writing dependency_links to src/member.egg-info/dependency_links.txt
    writing requirements to src/member.egg-info/requires.txt
    writing top-level names to src/member.egg-info/top_level.txt
    reading manifest file 'src/member.egg-info/SOURCES.txt'
    writing manifest file 'src/member.egg-info/SOURCES.txt'
    running check
    creating member-0.1.0
    creating member-0.1.0/src
    creating member-0.1.0/src/member.egg-info
    copying files to member-0.1.0...
    copying README -> member-0.1.0
    copying pyproject.toml -> member-0.1.0
    copying src/__init__.py -> member-0.1.0/src
    copying src/member.egg-info/PKG-INFO -> member-0.1.0/src/member.egg-info
    copying src/member.egg-info/SOURCES.txt -> member-0.1.0/src/member.egg-info
    copying src/member.egg-info/dependency_links.txt -> member-0.1.0/src/member.egg-info
    copying src/member.egg-info/requires.txt -> member-0.1.0/src/member.egg-info
    copying src/member.egg-info/top_level.txt -> member-0.1.0/src/member.egg-info
    copying src/member.egg-info/SOURCES.txt -> member-0.1.0/src/member.egg-info
    Writing member-0.1.0/setup.cfg
    Creating tar archive
    removing 'member-0.1.0' (and everything under it)
    Building wheel from source distribution...
    running egg_info
    writing src/member.egg-info/PKG-INFO
    writing dependency_links to src/member.egg-info/dependency_links.txt
    writing requirements to src/member.egg-info/requires.txt
    writing top-level names to src/member.egg-info/top_level.txt
    reading manifest file 'src/member.egg-info/SOURCES.txt'
    writing manifest file 'src/member.egg-info/SOURCES.txt'
    running bdist_wheel
    running build
    running build_py
    creating build
    creating build/lib
    copying src/__init__.py -> build/lib
    running egg_info
    writing src/member.egg-info/PKG-INFO
    writing dependency_links to src/member.egg-info/dependency_links.txt
    writing requirements to src/member.egg-info/requires.txt
    writing top-level names to src/member.egg-info/top_level.txt
    reading manifest file 'src/member.egg-info/SOURCES.txt'
    writing manifest file 'src/member.egg-info/SOURCES.txt'
    installing to build/bdist.linux-x86_64/wheel
    running install
    running install_lib
    creating build/bdist.linux-x86_64
    creating build/bdist.linux-x86_64/wheel
    copying build/lib/__init__.py -> build/bdist.linux-x86_64/wheel
    running install_egg_info
    Copying src/member.egg-info to build/bdist.linux-x86_64/wheel/member-0.1.0-py3.12.egg-info
    running install_scripts
    creating build/bdist.linux-x86_64/wheel/member-0.1.0.dist-info/WHEEL
    creating '[TEMP_DIR]/project/packages/member/dist/[TMP]/wheel' to it
    adding '__init__.py'
    adding 'member-0.1.0.dist-info/METADATA'
    adding 'member-0.1.0.dist-info/WHEEL'
    adding 'member-0.1.0.dist-info/top_level.txt'
    adding 'member-0.1.0.dist-info/RECORD'
    removing build/bdist.linux-x86_64/wheel
    Successfully built project/packages/member/dist/member-0.1.0.tar.gz and project/packages/member/dist/member-0.1.0-py3-none-any.whl
    "###);

    // Fail when `--package` is provided without a workspace.
    uv_snapshot!(&filters, context.build().arg("--package").arg("member"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No `pyproject.toml` found in current directory or any parent directory
      Caused by: `--package` was provided, but no workspace was found
    "###);

    // Fail when `--package` is a non-existent member without a workspace.
    uv_snapshot!(&filters, context.build().arg("--package").arg("fail").current_dir(&project), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Package `fail` not found in workspace
    "###);

    Ok(())
}

#[test]
fn build_constraints() -> Result<()> {
    let context = TestContext::new("3.12");
    let filters = context
        .filters()
        .into_iter()
        .chain([
            (r"exit code: 1", "exit status: 1"),
            (r"bdist\.[^/\\\s]+-[^/\\\s]+", "bdist.linux-x86_64"),
            (r"\\\.", ""),
        ])
        .collect::<Vec<_>>();

    let project = context.temp_dir.child("project");

    let constraints = project.child("constraints.txt");
    constraints.write_str("setuptools==0.1.0")?;

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
    project.child("README").touch()?;

    uv_snapshot!(&filters, context.build().arg("--build-constraint").arg("constraints.txt").current_dir(&project), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Building source distribution...
    error: Failed to install requirements from `build-system.requires` (resolve)
      Caused by: No solution found when resolving: setuptools>=42
      Caused by: Because you require setuptools>=42 and setuptools==0.1.0, we can conclude that your requirements are unsatisfiable.
    "###);

    project
        .child("dist")
        .child("project-0.1.0.tar.gz")
        .assert(predicate::path::missing());
    project
        .child("dist")
        .child("project-0.1.0-py3-none-any.whl")
        .assert(predicate::path::missing());

    Ok(())
}

#[test]
fn sha() -> Result<()> {
    let context = TestContext::new("3.8");
    let filters = context
        .filters()
        .into_iter()
        .chain([
            (r"exit code: 1", "exit status: 1"),
            (r"bdist\.[^/\\\s]+-[^/\\\s]+", "bdist.linux-x86_64"),
            (r"\\\.", ""),
        ])
        .collect::<Vec<_>>();

    let project = context.temp_dir.child("project");

    let pyproject_toml = project.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.8"
        dependencies = ["anyio==3.7.0"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    project.child("src").child("__init__.py").touch()?;
    project.child("README").touch()?;

    // Ignore an incorrect hash, if `--require-hashes` is not provided.
    let constraints = project.child("constraints.txt");
    constraints.write_str("setuptools==68.2.2 --hash=sha256:a248cb506794bececcddeddb1678bc722f9cfcacf02f98f7c0af6b9ed893caf2")?;

    uv_snapshot!(&filters, context.build().arg("--build-constraint").arg("constraints.txt").current_dir(&project), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Building source distribution...
    running egg_info
    creating src/project.egg-info
    writing src/project.egg-info/PKG-INFO
    writing dependency_links to src/project.egg-info/dependency_links.txt
    writing requirements to src/project.egg-info/requires.txt
    writing top-level names to src/project.egg-info/top_level.txt
    writing manifest file 'src/project.egg-info/SOURCES.txt'
    reading manifest file 'src/project.egg-info/SOURCES.txt'
    writing manifest file 'src/project.egg-info/SOURCES.txt'
    running sdist
    running egg_info
    writing src/project.egg-info/PKG-INFO
    writing dependency_links to src/project.egg-info/dependency_links.txt
    writing requirements to src/project.egg-info/requires.txt
    writing top-level names to src/project.egg-info/top_level.txt
    reading manifest file 'src/project.egg-info/SOURCES.txt'
    writing manifest file 'src/project.egg-info/SOURCES.txt'
    running check
    creating project-0.1.0
    creating project-0.1.0/src
    creating project-0.1.0/src/project.egg-info
    copying files to project-0.1.0...
    copying README -> project-0.1.0
    copying pyproject.toml -> project-0.1.0
    copying src/__init__.py -> project-0.1.0/src
    copying src/project.egg-info/PKG-INFO -> project-0.1.0/src/project.egg-info
    copying src/project.egg-info/SOURCES.txt -> project-0.1.0/src/project.egg-info
    copying src/project.egg-info/dependency_links.txt -> project-0.1.0/src/project.egg-info
    copying src/project.egg-info/requires.txt -> project-0.1.0/src/project.egg-info
    copying src/project.egg-info/top_level.txt -> project-0.1.0/src/project.egg-info
    Writing project-0.1.0/setup.cfg
    Creating tar archive
    removing 'project-0.1.0' (and everything under it)
    Building wheel from source distribution...
    running egg_info
    writing src/project.egg-info/PKG-INFO
    writing dependency_links to src/project.egg-info/dependency_links.txt
    writing requirements to src/project.egg-info/requires.txt
    writing top-level names to src/project.egg-info/top_level.txt
    reading manifest file 'src/project.egg-info/SOURCES.txt'
    writing manifest file 'src/project.egg-info/SOURCES.txt'
    running bdist_wheel
    running build
    running build_py
    creating build
    creating build/lib
    copying src/__init__.py -> build/lib
    running egg_info
    writing src/project.egg-info/PKG-INFO
    writing dependency_links to src/project.egg-info/dependency_links.txt
    writing requirements to src/project.egg-info/requires.txt
    writing top-level names to src/project.egg-info/top_level.txt
    reading manifest file 'src/project.egg-info/SOURCES.txt'
    writing manifest file 'src/project.egg-info/SOURCES.txt'
    installing to build/bdist.linux-x86_64/wheel
    running install
    running install_lib
    creating build/bdist.linux-x86_64
    creating build/bdist.linux-x86_64/wheel
    copying build/lib/__init__.py -> build/bdist.linux-x86_64/wheel
    running install_egg_info
    Copying src/project.egg-info to build/bdist.linux-x86_64/wheel/project-0.1.0-py3.8.egg-info
    running install_scripts
    creating build/bdist.linux-x86_64/wheel/project-0.1.0.dist-info/WHEEL
    creating '[TEMP_DIR]/project/dist/[TMP]/wheel' to it
    adding '__init__.py'
    adding 'project-0.1.0.dist-info/METADATA'
    adding 'project-0.1.0.dist-info/WHEEL'
    adding 'project-0.1.0.dist-info/top_level.txt'
    adding 'project-0.1.0.dist-info/RECORD'
    removing build/bdist.linux-x86_64/wheel
    Successfully built dist/project-0.1.0.tar.gz and dist/project-0.1.0-py3-none-any.whl
    "###);

    project
        .child("dist")
        .child("project-0.1.0.tar.gz")
        .assert(predicate::path::is_file());
    project
        .child("dist")
        .child("project-0.1.0-py3-none-any.whl")
        .assert(predicate::path::is_file());

    fs_err::remove_dir_all(project.child("dist"))?;

    // Reject an incorrect hash.
    uv_snapshot!(&filters, context.build().arg("--build-constraint").arg("constraints.txt").arg("--require-hashes").current_dir(&project), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Building source distribution...
    error: Failed to install requirements from `build-system.requires` (install)
      Caused by: Failed to prepare distributions
      Caused by: Failed to fetch wheel: setuptools==68.2.2
      Caused by: Hash mismatch for `setuptools==68.2.2`

    Expected:
      sha256:a248cb506794bececcddeddb1678bc722f9cfcacf02f98f7c0af6b9ed893caf2

    Computed:
      sha256:b454a35605876da60632df1a60f736524eb73cc47bbc9f3f1ef1b644de74fd2a
    "###);

    project
        .child("dist")
        .child("project-0.1.0.tar.gz")
        .assert(predicate::path::missing());
    project
        .child("dist")
        .child("project-0.1.0-py3-none-any.whl")
        .assert(predicate::path::missing());

    fs_err::remove_dir_all(project.child("dist"))?;

    // Reject a missing hash.
    let constraints = project.child("constraints.txt");
    constraints.write_str("setuptools==68.2.2")?;

    uv_snapshot!(&filters, context.build().arg("--build-constraint").arg("constraints.txt").arg("--require-hashes").current_dir(&project), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Building source distribution...
    error: Failed to install requirements from `build-system.requires` (resolve)
      Caused by: No solution found when resolving: setuptools>=42
      Caused by: In `--require-hashes` mode, all requirements must be pinned upfront with `==`, but found: `setuptools`
    "###);

    project
        .child("dist")
        .child("project-0.1.0.tar.gz")
        .assert(predicate::path::missing());
    project
        .child("dist")
        .child("project-0.1.0-py3-none-any.whl")
        .assert(predicate::path::missing());

    // Accept a correct hash.
    let constraints = project.child("constraints.txt");
    constraints.write_str("setuptools==68.2.2 --hash=sha256:b454a35605876da60632df1a60f736524eb73cc47bbc9f3f1ef1b644de74fd2a")?;

    uv_snapshot!(&filters, context.build().arg("--build-constraint").arg("constraints.txt").current_dir(&project), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Building source distribution...
    running egg_info
    writing src/project.egg-info/PKG-INFO
    writing dependency_links to src/project.egg-info/dependency_links.txt
    writing requirements to src/project.egg-info/requires.txt
    writing top-level names to src/project.egg-info/top_level.txt
    reading manifest file 'src/project.egg-info/SOURCES.txt'
    writing manifest file 'src/project.egg-info/SOURCES.txt'
    running sdist
    running egg_info
    writing src/project.egg-info/PKG-INFO
    writing dependency_links to src/project.egg-info/dependency_links.txt
    writing requirements to src/project.egg-info/requires.txt
    writing top-level names to src/project.egg-info/top_level.txt
    reading manifest file 'src/project.egg-info/SOURCES.txt'
    writing manifest file 'src/project.egg-info/SOURCES.txt'
    running check
    creating project-0.1.0
    creating project-0.1.0/src
    creating project-0.1.0/src/project.egg-info
    copying files to project-0.1.0...
    copying README -> project-0.1.0
    copying pyproject.toml -> project-0.1.0
    copying src/__init__.py -> project-0.1.0/src
    copying src/project.egg-info/PKG-INFO -> project-0.1.0/src/project.egg-info
    copying src/project.egg-info/SOURCES.txt -> project-0.1.0/src/project.egg-info
    copying src/project.egg-info/dependency_links.txt -> project-0.1.0/src/project.egg-info
    copying src/project.egg-info/requires.txt -> project-0.1.0/src/project.egg-info
    copying src/project.egg-info/top_level.txt -> project-0.1.0/src/project.egg-info
    Writing project-0.1.0/setup.cfg
    Creating tar archive
    removing 'project-0.1.0' (and everything under it)
    Building wheel from source distribution...
    running egg_info
    writing src/project.egg-info/PKG-INFO
    writing dependency_links to src/project.egg-info/dependency_links.txt
    writing requirements to src/project.egg-info/requires.txt
    writing top-level names to src/project.egg-info/top_level.txt
    reading manifest file 'src/project.egg-info/SOURCES.txt'
    writing manifest file 'src/project.egg-info/SOURCES.txt'
    running bdist_wheel
    running build
    running build_py
    creating build
    creating build/lib
    copying src/__init__.py -> build/lib
    running egg_info
    writing src/project.egg-info/PKG-INFO
    writing dependency_links to src/project.egg-info/dependency_links.txt
    writing requirements to src/project.egg-info/requires.txt
    writing top-level names to src/project.egg-info/top_level.txt
    reading manifest file 'src/project.egg-info/SOURCES.txt'
    writing manifest file 'src/project.egg-info/SOURCES.txt'
    installing to build/bdist.linux-x86_64/wheel
    running install
    running install_lib
    creating build/bdist.linux-x86_64
    creating build/bdist.linux-x86_64/wheel
    copying build/lib/__init__.py -> build/bdist.linux-x86_64/wheel
    running install_egg_info
    Copying src/project.egg-info to build/bdist.linux-x86_64/wheel/project-0.1.0-py3.8.egg-info
    running install_scripts
    creating build/bdist.linux-x86_64/wheel/project-0.1.0.dist-info/WHEEL
    creating '[TEMP_DIR]/project/dist/[TMP]/wheel' to it
    adding '__init__.py'
    adding 'project-0.1.0.dist-info/METADATA'
    adding 'project-0.1.0.dist-info/WHEEL'
    adding 'project-0.1.0.dist-info/top_level.txt'
    adding 'project-0.1.0.dist-info/RECORD'
    removing build/bdist.linux-x86_64/wheel
    Successfully built dist/project-0.1.0.tar.gz and dist/project-0.1.0-py3-none-any.whl
    "###);

    project
        .child("dist")
        .child("project-0.1.0.tar.gz")
        .assert(predicate::path::is_file());
    project
        .child("dist")
        .child("project-0.1.0-py3-none-any.whl")
        .assert(predicate::path::is_file());

    Ok(())
}
