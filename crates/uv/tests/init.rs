#![cfg(all(feature = "python", feature = "pypi"))]

use anyhow::Result;
use assert_fs::prelude::*;
use indoc::indoc;
use insta::assert_snapshot;

use common::{uv_snapshot, TestContext};

mod common;

/// See [`init_application`] and [`init_library`] for more coverage.
#[test]
fn init() -> Result<()> {
    let context = TestContext::new("3.12");

    uv_snapshot!(context.filters(), context.init().arg("foo"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Initialized project `foo` at `[TEMP_DIR]/foo`
    "###);

    let pyproject = fs_err::read_to_string(context.temp_dir.join("foo/pyproject.toml"))?;
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
        "###
        );
    });

    // Run `uv lock` in the new project.
    uv_snapshot!(context.filters(), context.lock().current_dir(context.temp_dir.join("foo")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using Python 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 1 package in [TIME]
    "###);

    let python_version =
        fs_err::read_to_string(context.temp_dir.join("foo").join(".python-version"))?;
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            python_version, @"3.12"
        );
    });

    Ok(())
}

/// Run `uv init --app` to create an application project
#[test]
fn init_application() -> Result<()> {
    let context = TestContext::new("3.12");

    let child = context.temp_dir.child("foo");
    child.create_dir_all()?;

    let pyproject_toml = child.join("pyproject.toml");
    let hello_py = child.join("hello.py");

    uv_snapshot!(context.filters(), context.init().current_dir(&child).arg("--app"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Initialized project `foo`
    "###);

    let pyproject = fs_err::read_to_string(&pyproject_toml)?;
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
        "###
        );
    });

    let hello = fs_err::read_to_string(hello_py)?;
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            hello, @r###"
        def main():
            print("Hello from foo!")


        if __name__ == "__main__":
            main()
        "###
        );
    });

    uv_snapshot!(context.filters(), context.run().current_dir(&child).arg("hello.py"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello from foo!

    ----- stderr -----
    warning: `VIRTUAL_ENV=[VENV]/` does not match the project environment path `.venv` and will be ignored
    Using Python 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtualenv at: .venv
    Resolved 1 package in [TIME]
    Audited in [TIME]
    "###);

    Ok(())
}

/// When `hello.py` already exists, we don't create it again
#[test]
fn init_application_hello_exists() -> Result<()> {
    let context = TestContext::new("3.12");

    let child = context.temp_dir.child("foo");
    child.create_dir_all()?;

    let pyproject_toml = child.join("pyproject.toml");
    let hello_py = child.child("hello.py");
    hello_py.touch()?;

    uv_snapshot!(context.filters(), context.init().current_dir(&child).arg("--app"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Initialized project `foo`
    "###);

    let pyproject = fs_err::read_to_string(&pyproject_toml)?;
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
        "###
        );
    });

    let hello = fs_err::read_to_string(hello_py)?;
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            hello, @""
        );
    });

    Ok(())
}

/// When other Python files already exists, we still create `hello.py`
#[test]
fn init_application_other_python_exists() -> Result<()> {
    let context = TestContext::new("3.12");

    let child = context.temp_dir.child("foo");
    child.create_dir_all()?;

    let pyproject_toml = child.join("pyproject.toml");
    let hello_py = child.join("hello.py");
    let other_py = child.child("foo.py");
    other_py.touch()?;

    uv_snapshot!(context.filters(), context.init().current_dir(&child).arg("--app"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Initialized project `foo`
    "###);

    let pyproject = fs_err::read_to_string(&pyproject_toml)?;
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
        "###
        );
    });

    let hello = fs_err::read_to_string(hello_py)?;
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            hello, @r###"
        def main():
            print("Hello from foo!")


        if __name__ == "__main__":
            main()
        "###
        );
    });

    Ok(())
}

/// Run `uv init --app --package` to create a packaged application project
#[test]
fn init_application_package() -> Result<()> {
    let context = TestContext::new("3.12");

    let child = context.temp_dir.child("foo");
    child.create_dir_all()?;

    let pyproject_toml = child.join("pyproject.toml");
    let init_py = child.join("src").join("foo").join("__init__.py");

    uv_snapshot!(context.filters(), context.init().current_dir(&child).arg("--app").arg("--package"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Initialized project `foo`
    "###);

    let pyproject = fs_err::read_to_string(&pyproject_toml)?;
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

        [project.scripts]
        hello = "foo:hello"

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
        "###
        );
    });

    let init = fs_err::read_to_string(init_py)?;
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            init, @r###"
        def hello() -> None:
            print("Hello from foo!")
        "###
        );
    });

    uv_snapshot!(context.filters(), context.run().current_dir(&child).arg("hello"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello from foo!

    ----- stderr -----
    warning: `VIRTUAL_ENV=[VENV]/` does not match the project environment path `.venv` and will be ignored
    Using Python 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtualenv at: .venv
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==0.1.0 (from file://[TEMP_DIR]/foo)
    "###);

    Ok(())
}

/// Run `uv init --lib` to create an library project
#[test]
fn init_library() -> Result<()> {
    let context = TestContext::new("3.12");

    let child = context.temp_dir.child("foo");
    child.create_dir_all()?;

    let pyproject_toml = child.join("pyproject.toml");
    let init_py = child.join("src").join("foo").join("__init__.py");
    let py_typed = child.join("src").join("foo").join("py.typed");

    uv_snapshot!(context.filters(), context.init().current_dir(&child).arg("--lib"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Initialized project `foo`
    "###);

    let pyproject = fs_err::read_to_string(&pyproject_toml)?;
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

    let init = fs_err::read_to_string(init_py)?;
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            init, @r###"
        def hello() -> str:
            return "Hello from foo!"
        "###
        );
    });

    let py_typed = fs_err::read_to_string(py_typed)?;
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            py_typed, @""
        );
    });

    uv_snapshot!(context.filters(), context.run().current_dir(&child).arg("python").arg("-c").arg("import foo; print(foo.hello())"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello from foo!

    ----- stderr -----
    warning: `VIRTUAL_ENV=[VENV]/` does not match the project environment path `.venv` and will be ignored
    Using Python 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtualenv at: .venv
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==0.1.0 (from file://[TEMP_DIR]/foo)
    "###);

    Ok(())
}

/// Run `uv init --lib` with an existing py.typed file
#[test]
fn init_py_typed_exists() -> Result<()> {
    let context = TestContext::new("3.12");

    let child = context.temp_dir.child("foo");
    child.create_dir_all()?;

    let foo = child.child("src").child("foo");
    foo.create_dir_all()?;

    let py_typed = foo.join("py.typed");
    fs_err::write(&py_typed, "partial")?;

    uv_snapshot!(context.filters(), context.init().current_dir(&child).arg("--lib"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Initialized project `foo`
    "###);

    let py_typed = fs_err::read_to_string(py_typed)?;
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            py_typed, @"partial"
        );
    });
    Ok(())
}

/// Using `uv init --lib --no-package` isn't allowed
#[test]
fn init_library_no_package() -> Result<()> {
    let context = TestContext::new("3.12");

    let child = context.temp_dir.child("foo");
    child.create_dir_all()?;

    uv_snapshot!(context.filters(), context.init().current_dir(&child).arg("--lib").arg("--no-package"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: the argument '--lib' cannot be used with '--no-package'

    Usage: uv init --cache-dir [CACHE_DIR] --lib [PATH]

    For more information, try '--help'.
    "###);

    Ok(())
}

/// Ensure that `uv init` initializes the cache.
#[test]
fn init_cache() -> Result<()> {
    let context = TestContext::new("3.12");

    fs_err::remove_dir_all(&context.cache_dir)?;

    uv_snapshot!(context.filters(), context.init().arg("foo"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Initialized project `foo` at `[TEMP_DIR]/foo`
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
        "###
        );
    });

    Ok(())
}

#[test]
fn init_no_pin_python() -> Result<()> {
    let context = TestContext::new("3.12");

    uv_snapshot!(context.filters(), context.init().arg("foo").arg("--no-pin-python"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Initialized project `foo` at `[TEMP_DIR]/foo`
    "###);

    let pyproject = fs_err::read_to_string(context.temp_dir.join("foo/pyproject.toml"))?;
    let _ = fs_err::read_to_string(context.temp_dir.join("foo/.python-version")).unwrap_err();

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
        "###
        );
    });
    Ok(())
}

#[test]
fn init_library_current_dir() -> Result<()> {
    let context = TestContext::new("3.12");

    let dir = context.temp_dir.join("foo");
    fs_err::create_dir(&dir)?;

    uv_snapshot!(context.filters(), context.init().arg("--lib").current_dir(&dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
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
    Using Python 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 1 package in [TIME]
    "###);

    Ok(())
}

#[test]
fn init_application_current_dir() -> Result<()> {
    let context = TestContext::new("3.12");

    let dir = context.temp_dir.join("foo");
    fs_err::create_dir(&dir)?;

    uv_snapshot!(context.filters(), context.init().arg("--app").current_dir(&dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Initialized project `foo`
    "###);

    let pyproject = fs_err::read_to_string(dir.join("pyproject.toml"))?;
    let hello_py = fs_err::read_to_string(dir.join("hello.py"))?;

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
        "###
        );
    });

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            hello_py, @r###"
        def main():
            print("Hello from foo!")


        if __name__ == "__main__":
            main()
        "###
        );
    });

    // Run `uv lock` in the new project.
    uv_snapshot!(context.filters(), context.lock().current_dir(&dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
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

    uv_snapshot!(context.filters(), context.init().current_dir(&dir).arg(".").arg("--lib"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
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

    uv_snapshot!(context.filters(), context.init().arg("--lib").current_dir(&child), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
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

    uv_snapshot!(context.filters(), context.init().arg("--lib").current_dir(&context.temp_dir).arg("foo"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
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
    uv_snapshot!(context.filters(), context.init().arg("--lib").current_dir(&context.home_dir).arg(&child), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
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
    Resolved 5 packages in [TIME]
    "###);

    Ok(())
}

#[test]
fn init_normalized_names() -> Result<()> {
    let context = TestContext::new("3.12");

    // `foo-bar` module is normalized to `foo_bar`.
    uv_snapshot!(context.filters(), context.init().current_dir(&context.temp_dir).arg("foo-bar").arg("--lib"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
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

    // `foo-bar` module is normalized to `foo_bar`.
    uv_snapshot!(context.filters(), context.init().current_dir(&context.temp_dir).arg("foo-bar").arg("--app"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Project is already initialized in `[TEMP_DIR]/foo-bar` (`pyproject.toml` file exists)
    "###);

    let child = context.temp_dir.child("foo-bar");
    let pyproject = fs_err::read_to_string(child.join("pyproject.toml"))?;

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

    // Initialize with `--no-workspace`.
    let child = context.temp_dir.join("foo");
    fs_err::create_dir(&child)?;

    uv_snapshot!(context.filters(), context.init().current_dir(&child).arg("--no-workspace"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Initialized project `foo`
    "###);

    // Ensure that the workspace was not modified.
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

    // Write out an invalid `pyproject.toml` in the parent, to ensure that `--no-workspace` is
    // robust to errors in discovery.
    pyproject_toml.write_str(indoc! {
        r"",
    })?;

    let child = context.temp_dir.join("bar");
    fs_err::create_dir(&child)?;

    uv_snapshot!(context.filters(), context.init().current_dir(&child).arg("--no-workspace"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Initialized project `bar`
    "###);

    let workspace = fs_err::read_to_string(context.temp_dir.join("pyproject.toml"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            workspace, @""
        );
    });

    Ok(())
}

/// Warn if the user provides `--no-workspace` outside of a workspace.
#[test]
fn init_no_workspace_warning() -> Result<()> {
    let context = TestContext::new("3.12");

    uv_snapshot!(context.filters(), context.init().current_dir(&context.temp_dir).arg("--no-workspace").arg("--name").arg("project"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Initialized project `project`
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
        description = "Add your description here"
        readme = "README.md"
        requires-python = ">=3.12"
        dependencies = []
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
    Adding `foo` as member of workspace `[TEMP_DIR]/`
    Initialized project `foo` at `[TEMP_DIR]/foo`
    "###);

    // Create a grandchild from the child directory.
    uv_snapshot!(context.filters(), context.init().current_dir(&child).arg("bar"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
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

/// Run `uv init --virtual` to create a virtual project.
#[test]
fn init_virtual_project() -> Result<()> {
    let context = TestContext::new("3.12");

    let child = context.temp_dir.child("foo");
    child.create_dir_all()?;

    let pyproject_toml = child.join("pyproject.toml");

    uv_snapshot!(context.filters(), context.init().current_dir(&child).arg("--virtual"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Initialized project `foo`
    "###);

    let pyproject = fs_err::read_to_string(&pyproject_toml)?;
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

        [project.scripts]
        hello = "foo:hello"

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
        "###
        );
    });

    uv_snapshot!(context.filters(), context.init().current_dir(&child).arg("bar"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Adding `bar` as member of workspace `[TEMP_DIR]/foo`
    Initialized project `bar` at `[TEMP_DIR]/foo/bar`
    "###);

    let pyproject = fs_err::read_to_string(pyproject_toml)?;
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

        [project.scripts]
        hello = "foo:hello"

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"

        [tool.uv.workspace]
        members = ["bar"]
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

    // Create a virtual workspace.
    let pyproject_toml = child.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {
        r"
        [tool.uv.workspace]
        members = []
        ",
    })?;

    uv_snapshot!(context.filters(), context.init().current_dir(&child).arg("bar"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
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
    Adding `foo` as member of workspace `[TEMP_DIR]/`
    Initialized project `foo` at `[TEMP_DIR]/foo`
    "###);

    let pyproject = fs_err::read_to_string(context.temp_dir.join("foo").join("pyproject.toml"))?;
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

        [project.scripts]
        hello = "foo:hello"

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
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
        members = ["foo"]
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
        "###
        );
    });

    let python_version = fs_err::read_to_string(child.join(".python-version"))?;
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            python_version, @"3.12"
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
        "###
        );
    });

    let python_version = fs_err::read_to_string(child.join(".python-version"))?;
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            python_version, @"3.8"
        );
    });

    Ok(())
}

/// Run `uv init`, inferring the `requires-python` from the `--python` flag, and preserving the
/// specifiers verbatim.
#[test]
fn init_requires_python_specifiers() -> Result<()> {
    let context = TestContext::new_with_versions(&["3.8", "3.12"]);

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
        "###
        );
    });

    let python_version = fs_err::read_to_string(child.join(".python-version"))?;
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            python_version, @"3.8"
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
    error: Not a valid package or extra name: ".foo". Names must start and end with a letter or digit and may only contain -, _, ., and alphanumeric characters.
    "###);
}

/// Run `uv init` with an invalid `pyproject.toml` in a parent directory.
#[test]
fn init_failure() -> Result<()> {
    let context = TestContext::new("3.12");

    // Create an empty `pyproject.toml`.
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.touch()?;

    uv_snapshot!(context.filters(), context.init().arg("foo"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to discover parent workspace; use `uv init --no-workspace` to ignore
      Caused by: No `project` table found in: `[TEMP_DIR]/pyproject.toml`
    "###);

    uv_snapshot!(context.filters(), context.init().arg("foo").arg("--no-workspace"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Initialized project `foo` at `[TEMP_DIR]/foo`
    "###);

    let workspace = fs_err::read_to_string(context.temp_dir.join("foo").join("pyproject.toml"))?;
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            workspace, @r###"
        [project]
        name = "foo"
        version = "0.1.0"
        description = "Add your description here"
        readme = "README.md"
        requires-python = ">=3.12"
        dependencies = []
        "###
        );
    });

    Ok(())
}
