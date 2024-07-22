#![cfg(all(feature = "python", feature = "pypi"))]

use anyhow::Result;
use assert_fs::prelude::*;
use indoc::indoc;
use insta::assert_snapshot;

use common::{uv_snapshot, TestContext};

mod common;

#[test]
fn init() -> Result<()> {
    let context = TestContext::new("3.12");

    uv_snapshot!(context.filters(), context.init().arg("foo"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv init` is experimental and may change without warning
    Initialized project foo in [TEMP_DIR]/foo
    "###);

    let pyproject = fs_err::read_to_string(context.temp_dir.join("foo/pyproject.toml"))?;
    let init_py = fs_err::read_to_string(context.temp_dir.join("foo/src/foo/__init__.py"))?;
    let _ = fs_err::read_to_string(context.temp_dir.join("foo/README.md")).unwrap();

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject, @r###"
        [project]
        name = "foo"
        version = "0.1.0"
        description = "Add your description here"
        readme = "README.md"
        dependencies = []

        [tool.uv]
        dev-dependencies = []
        "###
        );
    });

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            init_py, @r###"
        def hello() -> str:
            return "Hello from foo!"
        "###
        );
    });

    // Run `uv lock` in the new project.
    uv_snapshot!(context.filters(), context.lock().current_dir(context.temp_dir.join("foo")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning
    Using Python 3.12.[X] interpreter at: [PYTHON-3.12]
    warning: No `requires-python` field found in the workspace. Defaulting to `>=3.12`.
    Resolved 1 package in [TIME]
    "###);

    Ok(())
}

#[test]
fn init_no_readme() -> Result<()> {
    let context = TestContext::new("3.12");

    uv_snapshot!(context.filters(), context.init().arg("foo").arg("--no-readme"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv init` is experimental and may change without warning
    Initialized project foo in [TEMP_DIR]/foo
    "###);

    let pyproject = fs_err::read_to_string(context.temp_dir.join("foo/pyproject.toml"))?;
    let _ = fs_err::read_to_string(context.temp_dir.join("foo/README.md")).unwrap_err();

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject, @r###"
        [project]
        name = "foo"
        version = "0.1.0"
        description = "Add your description here"
        dependencies = []

        [tool.uv]
        dev-dependencies = []
        "###
        );
    });

    Ok(())
}

#[test]
fn current_dir() -> Result<()> {
    let context = TestContext::new("3.12");

    let dir = context.temp_dir.join("foo");
    fs_err::create_dir(&dir)?;

    uv_snapshot!(context.filters(), context.init().current_dir(&dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv init` is experimental and may change without warning
    Initialized project foo
    "###);

    let pyproject = fs_err::read_to_string(dir.join("pyproject.toml"))?;
    let init_py = fs_err::read_to_string(dir.join("src/foo/__init__.py"))?;
    let _ = fs_err::read_to_string(dir.join("README.md")).unwrap();

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject, @r###"
        [project]
        name = "foo"
        version = "0.1.0"
        description = "Add your description here"
        readme = "README.md"
        dependencies = []

        [tool.uv]
        dev-dependencies = []
        "###
        );
    });

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            init_py, @r###"
        def hello() -> str:
            return "Hello from foo!"
        "###
        );
    });

    // Run `uv lock` in the new project.
    uv_snapshot!(context.filters(), context.lock().current_dir(&dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning
    Using Python 3.12.[X] interpreter at: [PYTHON-3.12]
    warning: No `requires-python` field found in the workspace. Defaulting to `>=3.12`.
    Resolved 1 package in [TIME]
    "###);

    Ok(())
}

#[test]
fn init_workspace() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0"]
        "#,
    })?;

    let child = context.temp_dir.join("foo");
    fs_err::create_dir(&child)?;

    uv_snapshot!(context.filters(), context.init().current_dir(&child), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv init` is experimental and may change without warning
    Adding foo as member of workspace [TEMP_DIR]/
    Initialized project foo
    "###);

    let pyproject = fs_err::read_to_string(child.join("pyproject.toml"))?;
    let init_py = fs_err::read_to_string(child.join("src/foo/__init__.py"))?;

    let _ = fs_err::read_to_string(child.join("README.md")).unwrap();

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject, @r###"
        [project]
        name = "foo"
        version = "0.1.0"
        description = "Add your description here"
        readme = "README.md"
        dependencies = []

        [tool.uv]
        dev-dependencies = []
        "###
        );
    });

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            init_py, @r###"
        def hello() -> str:
            return "Hello from foo!"
        "###
        );
    });

    let workspace = fs_err::read_to_string(context.temp_dir.join("pyproject.toml"))?;
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            workspace, @r###"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0"]

        [tool.uv.workspace]
        members = ["foo"]
        "###
        );
    });

    // Run `uv lock` in the workspace.
    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning
    Resolved 5 packages in [TIME]
    "###);

    Ok(())
}

#[test]
fn init_workspace_relative_sub_package() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0"]
        "#,
    })?;

    let child = context.temp_dir.join("foo");
    fs_err::create_dir(&child)?;

    uv_snapshot!(context.filters(), context.init().current_dir(&context.temp_dir).arg("foo"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv init` is experimental and may change without warning
    Adding foo as member of workspace [TEMP_DIR]/
    Initialized project foo in [TEMP_DIR]/foo
    "###);

    let pyproject = fs_err::read_to_string(child.join("pyproject.toml"))?;
    let init_py = fs_err::read_to_string(child.join("src/foo/__init__.py"))?;

    let _ = fs_err::read_to_string(child.join("README.md")).unwrap();

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject, @r###"
        [project]
        name = "foo"
        version = "0.1.0"
        description = "Add your description here"
        readme = "README.md"
        dependencies = []

        [tool.uv]
        dev-dependencies = []
        "###
        );
    });

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            init_py, @r###"
        def hello() -> str:
            return "Hello from foo!"
        "###
        );
    });

    let workspace = fs_err::read_to_string(context.temp_dir.join("pyproject.toml"))?;
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            workspace, @r###"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0"]

        [tool.uv.workspace]
        members = ["foo"]
        "###
        );
    });

    // Run `uv lock` in the workspace.
    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning
    Resolved 5 packages in [TIME]
    "###);

    Ok(())
}

#[test]
fn init_workspace_outside() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0"]
        "#,
    })?;

    let child = context.temp_dir.join("foo");
    fs_err::create_dir(&child)?;

    // Run `uv init <path>` outside the workspace.
    uv_snapshot!(context.filters(), context.init().current_dir(&context.home_dir).arg(&child), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv init` is experimental and may change without warning
    Adding foo as member of workspace [TEMP_DIR]/
    Initialized project foo in [TEMP_DIR]/foo
    "###);

    let pyproject = fs_err::read_to_string(child.join("pyproject.toml"))?;
    let init_py = fs_err::read_to_string(child.join("src/foo/__init__.py"))?;

    let _ = fs_err::read_to_string(child.join("README.md")).unwrap();

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject, @r###"
        [project]
        name = "foo"
        version = "0.1.0"
        description = "Add your description here"
        readme = "README.md"
        dependencies = []

        [tool.uv]
        dev-dependencies = []
        "###
        );
    });

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            init_py, @r###"
        def hello() -> str:
            return "Hello from foo!"
        "###
        );
    });

    let workspace = fs_err::read_to_string(context.temp_dir.join("pyproject.toml"))?;
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            workspace, @r###"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0"]

        [tool.uv.workspace]
        members = ["foo"]
        "###
        );
    });

    // Run `uv lock` in the workspace.
    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning
    Resolved 5 packages in [TIME]
    "###);

    Ok(())
}

#[test]
fn init_invalid_name() -> Result<()> {
    let context = TestContext::new("3.12");

    uv_snapshot!(context.filters(), context.init().current_dir(&context.temp_dir).arg("foo-bar"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv init` is experimental and may change without warning
    Initialized project foo-bar in [TEMP_DIR]/foo-bar
    "###);

    let child = context.temp_dir.child("foo-bar");
    let pyproject = fs_err::read_to_string(child.join("pyproject.toml"))?;
    let _ = fs_err::read_to_string(child.join("src/foo_bar/__init__.py"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject, @r###"
        [project]
        name = "foo-bar"
        version = "0.1.0"
        description = "Add your description here"
        readme = "README.md"
        dependencies = []

        [tool.uv]
        dev-dependencies = []
        "###
        );
    });

    Ok(())
}
