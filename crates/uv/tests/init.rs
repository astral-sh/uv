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
    Initialized project `foo` at `[TEMP_DIR]/foo`
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
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
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
    Initialized project `foo` at `[TEMP_DIR]/foo`
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
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
        "###
        );
    });

    Ok(())
}

#[test]
fn init_current_dir() -> Result<()> {
    let context = TestContext::new("3.12");

    let dir = context.temp_dir.join("foo");
    fs_err::create_dir(&dir)?;

    uv_snapshot!(context.filters(), context.init().current_dir(&dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv init` is experimental and may change without warning
    Initialized project `foo`
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
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
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
    Resolved 1 package in [TIME]
    "###);

    Ok(())
}

#[test]
fn init_dot_args() -> Result<()> {
    let context = TestContext::new("3.12");

    let dir = context.temp_dir.join("foo");
    fs_err::create_dir(&dir)?;

    uv_snapshot!(context.filters(), context.init().current_dir(&dir).arg("."), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv init` is experimental and may change without warning
    Initialized project `foo` at `[TEMP_DIR]/foo`
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
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
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
    Adding `foo` as member of workspace `[TEMP_DIR]/`
    Initialized project `foo`
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
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
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

    uv_snapshot!(context.filters(), context.init().current_dir(&context.temp_dir).arg("foo"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv init` is experimental and may change without warning
    Adding `foo` as member of workspace `[TEMP_DIR]/`
    Initialized project `foo` at `[TEMP_DIR]/foo`
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
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
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

    // Run `uv init <path>` outside the workspace.
    uv_snapshot!(context.filters(), context.init().current_dir(&context.home_dir).arg(&child), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv init` is experimental and may change without warning
    Adding `foo` as member of workspace `[TEMP_DIR]/`
    Initialized project `foo` at `[TEMP_DIR]/foo`
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
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
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
fn init_invalid_names() -> Result<()> {
    let context = TestContext::new("3.12");

    // `foo-bar` normalized to `foo_bar`.
    uv_snapshot!(context.filters(), context.init().current_dir(&context.temp_dir).arg("foo-bar"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv init` is experimental and may change without warning
    Initialized project `foo-bar` at `[TEMP_DIR]/foo-bar`
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
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
        "###
        );
    });

    // "bar baz" is not allowed.
    uv_snapshot!(context.filters(), context.init().current_dir(&context.temp_dir).arg("bar baz"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: `uv init` is experimental and may change without warning
    error: Not a valid package or extra name: "bar baz". Names must start and end with a letter or digit and may only contain -, _, ., and alphanumeric characters.
    "###);

    Ok(())
}

#[test]
fn init_isolated() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        "#,
    })?;

    let child = context.temp_dir.join("foo");
    fs_err::create_dir(&child)?;

    uv_snapshot!(context.filters(), context.init().current_dir(&child).arg("--isolated"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: The `--isolated` flag is deprecated and has no effect. Instead, use `--no-config` to prevent uv from discovering configuration files or `--no-workspace` to prevent uv from adding the initialized project to the containing workspace.
    warning: `uv init` is experimental and may change without warning
    Adding `foo` as member of workspace `[TEMP_DIR]/`
    Initialized project `foo`
    "###);

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

        [tool.uv.workspace]
        members = ["foo"]
        "###
        );
    });

    Ok(())
}

#[test]
fn init_no_workspace() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        "#,
    })?;

    let child = context.temp_dir.join("foo");
    fs_err::create_dir(&child)?;

    uv_snapshot!(context.filters(), context.init().current_dir(&child).arg("--no-workspace"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv init` is experimental and may change without warning
    Initialized project `foo`
    "###);

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
        "###
        );
    });

    Ok(())
}

#[test]
fn init_project_inside_project() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        "#,
    })?;

    // Create a child from the workspace root.
    let child = context.temp_dir.join("foo");
    uv_snapshot!(context.filters(), context.init().current_dir(&context.temp_dir).arg(&child), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv init` is experimental and may change without warning
    Adding `foo` as member of workspace `[TEMP_DIR]/`
    Initialized project `foo` at `[TEMP_DIR]/foo`
    "###);

    // Create a grandchild from the child directory.
    uv_snapshot!(context.filters(), context.init().current_dir(&child).arg("bar"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv init` is experimental and may change without warning
    Adding `bar` as member of workspace `[TEMP_DIR]/`
    Initialized project `bar` at `[TEMP_DIR]/foo/bar`
    "###);

    let workspace = fs_err::read_to_string(pyproject_toml)?;
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            workspace, @r###"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [tool.uv.workspace]
        members = ["foo", "foo/bar"]
        "###
        );
    });

    let pyproject = fs_err::read_to_string(child.join("pyproject.toml"))?;
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
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
        "###
        );
    });

    Ok(())
}

/// Run `uv init` from within a workspace with an explicit root.
#[test]
fn init_explicit_workspace() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [tool.uv.workspace]
        members = []
        "#,
    })?;

    let child = context.temp_dir.join("foo");
    uv_snapshot!(context.filters(), context.init().current_dir(&context.temp_dir).arg(&child), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv init` is experimental and may change without warning
    Adding `foo` as member of workspace `[TEMP_DIR]/`
    Initialized project `foo` at `[TEMP_DIR]/foo`
    "###);

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

        [tool.uv.workspace]
        members = ["foo"]
        "###
        );
    });

    Ok(())
}

/// Run `uv init` from within a virtual workspace.
#[test]
fn init_virtual_workspace() -> Result<()> {
    let context = TestContext::new("3.12");

    let child = context.temp_dir.child("foo");
    child.create_dir_all()?;

    let pyproject_toml = child.join("pyproject.toml");

    uv_snapshot!(context.filters(), context.init().current_dir(&child).arg("--virtual"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv init` is experimental and may change without warning
    Initialized workspace `foo`
    "###);

    let pyproject = fs_err::read_to_string(&pyproject_toml)?;
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject, @r###"
        [tool.uv.workspace]
        members = []
        "###
        );
    });

    uv_snapshot!(context.filters(), context.init().current_dir(&child).arg("bar"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv init` is experimental and may change without warning
    Adding `bar` as member of workspace `[TEMP_DIR]/foo`
    Initialized project `bar` at `[TEMP_DIR]/foo/bar`
    "###);

    let pyproject = fs_err::read_to_string(pyproject_toml)?;
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject, @r###"
        [tool.uv.workspace]
        members = ["bar"]
        "###
        );
    });

    Ok(())
}

/// Run `uv init --virtual` from within a workspace.
#[test]
fn init_nested_virtual_workspace() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {
        r"
        [tool.uv.workspace]
        members = []
        ",
    })?;

    uv_snapshot!(context.filters(), context.init().current_dir(&context.temp_dir).arg("--virtual").arg("foo"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv init` is experimental and may change without warning
    warning: Nested workspaces are not supported, but outer workspace (`[TEMP_DIR]/`) includes `[TEMP_DIR]/foo`
    Initialized workspace `foo` at `[TEMP_DIR]/foo`
    "###);

    let pyproject = fs_err::read_to_string(context.temp_dir.join("foo").join("pyproject.toml"))?;
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject, @r###"
        [tool.uv.workspace]
        members = []
        "###
        );
    });

    let workspace = fs_err::read_to_string(context.temp_dir.join("pyproject.toml"))?;
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            workspace, @r###"
        [tool.uv.workspace]
        members = []
        "###
        );
    });

    Ok(())
}

/// Run `uv init` from within a workspace. The path is already included via `members`.
#[test]
fn init_matches_members() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {
        r"
        [tool.uv.workspace]
        members = ['packages/*']
        ",
    })?;

    // Create the parent directory (`packages`) and the child directory (`foo`), to ensure that
    // the empty child directory does _not_ trigger a workspace discovery error despite being a
    // valid member.
    let packages = context.temp_dir.join("packages");
    fs_err::create_dir_all(packages.join("foo"))?;

    uv_snapshot!(context.filters(), context.init().current_dir(context.temp_dir.join("packages")).arg("foo"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv init` is experimental and may change without warning
    Project `foo` is already a member of workspace `[TEMP_DIR]/`
    Initialized project `foo` at `[TEMP_DIR]/packages/foo`
    "###);

    let workspace = fs_err::read_to_string(context.temp_dir.join("pyproject.toml"))?;
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            workspace, @r###"
        [tool.uv.workspace]
        members = ['packages/*']
        "###
        );
    });

    Ok(())
}

/// Run `uv init` from within a workspace. The path is excluded via `exclude`.
#[test]
fn init_matches_exclude() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {
        r"
        [tool.uv.workspace]
        exclude = ['packages/foo']
        members = ['packages/*']
        ",
    })?;

    let packages = context.temp_dir.join("packages");
    fs_err::create_dir_all(packages)?;

    uv_snapshot!(context.filters(), context.init().current_dir(context.temp_dir.join("packages")).arg("foo"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv init` is experimental and may change without warning
    Project `foo` is excluded by workspace `[TEMP_DIR]/`
    Initialized project `foo` at `[TEMP_DIR]/packages/foo`
    "###);

    let workspace = fs_err::read_to_string(context.temp_dir.join("pyproject.toml"))?;
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            workspace, @r###"
        [tool.uv.workspace]
        exclude = ['packages/foo']
        members = ['packages/*']
        "###
        );
    });

    Ok(())
}

/// Run `uv init`, inheriting the `requires-python` from the workspace.
#[test]
fn init_requires_python_workspace() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.10"

        [tool.uv.workspace]
        members = []
        "#,
    })?;

    let child = context.temp_dir.join("foo");
    uv_snapshot!(context.filters(), context.init().current_dir(&context.temp_dir).arg(&child), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv init` is experimental and may change without warning
    Adding `foo` as member of workspace `[TEMP_DIR]/`
    Initialized project `foo` at `[TEMP_DIR]/foo`
    "###);

    let pyproject_toml = fs_err::read_to_string(child.join("pyproject.toml"))?;
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r###"
        [project]
        name = "foo"
        version = "0.1.0"
        description = "Add your description here"
        readme = "README.md"
        requires-python = ">=3.10"
        dependencies = []

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
        "###
        );
    });

    Ok(())
}

/// Run `uv init`, inferring the `requires-python` from the `--python` flag.
#[test]
fn init_requires_python_version() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [tool.uv.workspace]
        members = []
        "#,
    })?;

    let child = context.temp_dir.join("foo");
    uv_snapshot!(context.filters(), context.init().current_dir(&context.temp_dir).arg(&child).arg("--python").arg("3.8"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv init` is experimental and may change without warning
    Adding `foo` as member of workspace `[TEMP_DIR]/`
    Initialized project `foo` at `[TEMP_DIR]/foo`
    "###);

    let pyproject_toml = fs_err::read_to_string(child.join("pyproject.toml"))?;
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r###"
        [project]
        name = "foo"
        version = "0.1.0"
        description = "Add your description here"
        readme = "README.md"
        requires-python = ">=3.8"
        dependencies = []

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
        "###
        );
    });

    Ok(())
}

/// Run `uv init`, inferring the `requires-python` from the `--python` flag, and preserving the
/// specifiers verbatim.
#[test]
fn init_requires_python_specifiers() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [tool.uv.workspace]
        members = []
        "#,
    })?;

    let child = context.temp_dir.join("foo");
    uv_snapshot!(context.filters(), context.init().current_dir(&context.temp_dir).arg(&child).arg("--python").arg("==3.8.*"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv init` is experimental and may change without warning
    Adding `foo` as member of workspace `[TEMP_DIR]/`
    Initialized project `foo` at `[TEMP_DIR]/foo`
    "###);

    let pyproject_toml = fs_err::read_to_string(child.join("pyproject.toml"))?;
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r###"
        [project]
        name = "foo"
        version = "0.1.0"
        description = "Add your description here"
        readme = "README.md"
        requires-python = "==3.8.*"
        dependencies = []

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
        "###
        );
    });

    Ok(())
}

/// Run `uv init` from within an unmanaged project.
#[test]
fn init_unmanaged() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {
        r"
        [tool.uv]
        managed = false
        ",
    })?;

    uv_snapshot!(context.filters(), context.init().arg("foo"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv init` is experimental and may change without warning
    Initialized project `foo` at `[TEMP_DIR]/foo`
    "###);

    let workspace = fs_err::read_to_string(context.temp_dir.join("pyproject.toml"))?;
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            workspace, @r###"
        [tool.uv]
        managed = false
        "###
        );
    });

    Ok(())
}

#[test]
fn init_hidden() {
    let context = TestContext::new("3.12");

    uv_snapshot!(context.filters(), context.init().arg(".foo"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: `uv init` is experimental and may change without warning
    error: Not a valid package or extra name: ".foo". Names must start and end with a letter or digit and may only contain -, _, ., and alphanumeric characters.
    "###);
}
