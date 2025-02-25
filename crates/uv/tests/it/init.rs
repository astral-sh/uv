use std::process::Command;

use anyhow::Result;
use assert_cmd::prelude::OutputAssertExt;
use assert_fs::prelude::*;
use indoc::indoc;
use insta::assert_snapshot;
use predicates::prelude::predicate;

use uv_static::EnvVars;

use crate::common::{uv_snapshot, TestContext};

#[test]
fn init() {
    let context = TestContext::new("3.12");

    uv_snapshot!(context.filters(), context.init().arg("foo"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Initialized project `foo` at `[TEMP_DIR]/foo`
    "###);

    let pyproject = context.read("foo/pyproject.toml");
    let _ = context.read("foo/README.md");

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
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 1 package in [TIME]
    "###);

    let python_version = context.read("foo/.python-version");
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            python_version, @"3.12"
        );
    });
}

#[test]
fn init_bare() {
    let context = TestContext::new("3.12");

    uv_snapshot!(context.filters(), context.init().arg("foo").arg("--bare"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Initialized project `foo` at `[TEMP_DIR]/foo`
    "###);

    // No extra files should be created
    context
        .temp_dir
        .child("foo/README.md")
        .assert(predicate::path::missing());
    context
        .temp_dir
        .child("foo/hello.py")
        .assert(predicate::path::missing());
    context
        .temp_dir
        .child("foo/.python-version")
        .assert(predicate::path::missing());
    context
        .temp_dir
        .child("foo/.git")
        .assert(predicate::path::missing());

    let pyproject = context.read("foo/pyproject.toml");
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject, @r###"
        [project]
        name = "foo"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
        "###
        );
    });
}

/// Run `uv init --app` to create an application project
#[test]
fn init_application() -> Result<()> {
    let context = TestContext::new("3.12");

    let child = context.temp_dir.child("foo");
    child.create_dir_all()?;

    let pyproject_toml = child.join("pyproject.toml");
    let main_py = child.join("main.py");

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

    let hello = fs_err::read_to_string(main_py)?;
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

    uv_snapshot!(context.filters(), context.run().current_dir(&child).arg("main.py"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello from foo!

    ----- stderr -----
    warning: `VIRTUAL_ENV=[VENV]/` does not match the project environment path `.venv` and will be ignored; use `--active` to target the active environment instead
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: .venv
    Resolved 1 package in [TIME]
    Audited in [TIME]
    "###);

    Ok(())
}

/// When `main.py` already exists, we don't create it again
#[test]
fn init_application_hello_exists() -> Result<()> {
    let context = TestContext::new("3.12");

    let child = context.temp_dir.child("foo");
    child.create_dir_all()?;

    let pyproject_toml = child.join("pyproject.toml");
    let main_py = child.child("main.py");
    main_py.touch()?;

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

    let hello = fs_err::read_to_string(main_py)?;
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            hello, @""
        );
    });

    Ok(())
}

/// When other Python files already exists, we still create `main.py`
#[test]
fn init_application_other_python_exists() -> Result<()> {
    let context = TestContext::new("3.12");

    let child = context.temp_dir.child("foo");
    child.create_dir_all()?;

    let pyproject_toml = child.join("pyproject.toml");
    let main_py = child.join("main.py");
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

    let hello = fs_err::read_to_string(main_py)?;
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
        foo = "foo:main"

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
        def main() -> None:
            print("Hello from foo!")
        "###
        );
    });

    uv_snapshot!(context.filters(), context.run().current_dir(&child).arg("foo"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello from foo!

    ----- stderr -----
    warning: `VIRTUAL_ENV=[VENV]/` does not match the project environment path `.venv` and will be ignored; use `--active` to target the active environment instead
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: .venv
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
    warning: `VIRTUAL_ENV=[VENV]/` does not match the project environment path `.venv` and will be ignored; use `--active` to target the active environment instead
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: .venv
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==0.1.0 (from file://[TEMP_DIR]/foo)
    "###);

    Ok(())
}

#[test]
fn init_bare_lib() {
    let context = TestContext::new("3.12");

    uv_snapshot!(context.filters(), context.init().arg("foo").arg("--bare").arg("--lib"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Initialized project `foo` at `[TEMP_DIR]/foo`
    "###);

    // No extra files should be created
    context
        .temp_dir
        .child("foo/README.md")
        .assert(predicate::path::missing());
    context
        .temp_dir
        .child("foo/src")
        .assert(predicate::path::missing());

    context
        .temp_dir
        .child("foo/.git")
        .assert(predicate::path::missing());
    context
        .temp_dir
        .child("foo/.python-version")
        .assert(predicate::path::missing());

    let pyproject = context.read("foo/pyproject.toml");
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject, @r###"
        [project]
        name = "foo"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
        "###
        );
    });
}

#[test]
fn init_bare_package() {
    let context = TestContext::new("3.12");

    uv_snapshot!(context.filters(), context.init().arg("foo").arg("--bare").arg("--package"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Initialized project `foo` at `[TEMP_DIR]/foo`
    "###);

    // No extra files should be created
    context
        .temp_dir
        .child("foo/README.md")
        .assert(predicate::path::missing());
    context
        .temp_dir
        .child("foo/src")
        .assert(predicate::path::missing());

    context
        .temp_dir
        .child("foo/.git")
        .assert(predicate::path::missing());
    context
        .temp_dir
        .child("foo/.python-version")
        .assert(predicate::path::missing());

    let pyproject = context.read("foo/pyproject.toml");
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject, @r###"
        [project]
        name = "foo"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
        "###
        );
    });
}

#[test]
fn init_bare_opt_in() {
    let context = TestContext::new("3.12");

    // With `--bare`, you can still opt-in to extras
    // TODO(zanieb): Add option for `--readme`
    uv_snapshot!(context.filters(), context.init().arg("foo").arg("--bare")
        .arg("--description").arg("foo")
        .arg("--pin-python")
        .arg("--vcs").arg("git"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Initialized project `foo` at `[TEMP_DIR]/foo`
    "###);

    context
        .temp_dir
        .child("foo/README.md")
        .assert(predicate::path::missing());
    context
        .temp_dir
        .child("foo/src")
        .assert(predicate::path::missing());
    context
        .temp_dir
        .child("foo/.git")
        .assert(predicate::path::is_dir());
    context
        .temp_dir
        .child("foo/.python-version")
        .assert(predicate::path::is_file());

    let pyproject = context.read("foo/pyproject.toml");
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject, @r###"
        [project]
        name = "foo"
        version = "0.1.0"
        description = "foo"
        requires-python = ">=3.12"
        dependencies = []
        "###
        );
    });
}

// General init --script correctness test
#[test]
fn init_script() -> Result<()> {
    let context = TestContext::new("3.12");

    let child = context.temp_dir.child("foo");
    child.create_dir_all()?;

    let script = child.join("main.py");

    uv_snapshot!(context.filters(), context.init().current_dir(&child).arg("--script").arg("main.py"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Initialized script at `main.py`
    "###);

    let script = fs_err::read_to_string(&script)?;
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            script, @r###"
        # /// script
        # requires-python = ">=3.12"
        # dependencies = []
        # ///


        def main() -> None:
            print("Hello from main.py!")


        if __name__ == "__main__":
            main()
        "###
        );
    });

    uv_snapshot!(context.filters(), context.run().current_dir(&child).arg("python").arg("main.py"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello from main.py!

    ----- stderr -----
    "###);

    Ok(())
}

// Ensure python versions passed as arguments are present in file metadata
#[test]
fn init_script_python_version() -> Result<()> {
    let context = TestContext::new("3.11");

    let child = context.temp_dir.child("foo");
    child.create_dir_all()?;

    let script = child.join("version.py");

    uv_snapshot!(context.filters(), context.init().current_dir(&child).arg("--script").arg("version.py").arg("--python").arg("3.11"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Initialized script at `version.py`
    "###);

    let script = fs_err::read_to_string(&script)?;
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            script, @r###"
        # /// script
        # requires-python = ">=3.11"
        # dependencies = []
        # ///


        def main() -> None:
            print("Hello from version.py!")


        if __name__ == "__main__":
            main()
        "###
        );
    });

    Ok(())
}

// Init script should create parent directories if they don't exist
#[test]
fn init_script_create_directory() -> Result<()> {
    let context = TestContext::new("3.12");

    let child = context.temp_dir.child("foo");
    child.create_dir_all()?;

    let script = child.join("test").join("dir.py");

    uv_snapshot!(context.filters(), context.init().current_dir(&child).arg("--script").arg("test/dir.py"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Initialized script at `test/dir.py`
    "###);

    let script = fs_err::read_to_string(&script)?;
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            script, @r###"
        # /// script
        # requires-python = ">=3.12"
        # dependencies = []
        # ///


        def main() -> None:
            print("Hello from dir.py!")


        if __name__ == "__main__":
            main()
        "###
        );
    });

    Ok(())
}

// Init script should fail if file is already a PEP 723 script
#[test]
fn init_script_file_conflicts() -> Result<()> {
    let context = TestContext::new("3.12");

    let child = context.temp_dir.child("foo");
    child.create_dir_all()?;

    uv_snapshot!(context.filters(), context.init().current_dir(&child).arg("--script").arg("name_conflict.py"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Initialized script at `name_conflict.py`
    "###);

    uv_snapshot!(context.filters(), context.init().current_dir(&child).arg("--script").arg("name_conflict.py"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: `name_conflict.py` is already a PEP 723 script; use `uv run` to execute it
    "###);

    let contents = "print(\"Hello, world!\")";
    fs_err::write(child.join("existing_script.py"), contents)?;

    uv_snapshot!(context.filters(), context.init().current_dir(&child).arg("--script").arg("existing_script.py"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Initialized script at `existing_script.py`
    "###);

    let existing_script = fs_err::read_to_string(child.join("existing_script.py"))?;

    assert_snapshot!(
        existing_script, @r###"
    # /// script
    # requires-python = ">=3.12"
    # dependencies = []
    # ///

    print("Hello, world!")
    "###
    );

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
fn init_no_readme() {
    let context = TestContext::new("3.12");

    uv_snapshot!(context.filters(), context.init().arg("foo").arg("--no-readme"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Initialized project `foo` at `[TEMP_DIR]/foo`
    "###);

    let pyproject = context.read("foo/pyproject.toml");
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
}

#[test]
fn init_no_pin_python() {
    let context = TestContext::new("3.12");

    uv_snapshot!(context.filters(), context.init().arg("foo").arg("--no-pin-python"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Initialized project `foo` at `[TEMP_DIR]/foo`
    "###);

    let pyproject = context.read("foo/pyproject.toml");
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
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
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
    let main_py = fs_err::read_to_string(dir.join("main.py"))?;

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
            main_py, @r###"
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
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
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
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
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

    let workspace = context.read("pyproject.toml");
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

    let workspace = context.read("pyproject.toml");
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

    let workspace = context.read("pyproject.toml");
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

    // `foo-bar` module is normalized to `foo-bar`.
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

    // `bar_baz` module is normalized to `bar-baz`.
    uv_snapshot!(context.filters(), context.init().current_dir(&context.temp_dir).arg("bar_baz").arg("--app"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Initialized project `bar-baz` at `[TEMP_DIR]/bar_baz`
    "###);

    let child = context.temp_dir.child("bar_baz");
    let pyproject = fs_err::read_to_string(child.join("pyproject.toml"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject, @r###"
        [project]
        name = "bar-baz"
        version = "0.1.0"
        description = "Add your description here"
        readme = "README.md"
        requires-python = ">=3.12"
        dependencies = []
        "###
        );
    });

    // "baz bop" is normalized to "baz-bop".
    uv_snapshot!(context.filters(), context.init().current_dir(&context.temp_dir).arg("baz bop"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Initialized project `baz-bop` at `[TEMP_DIR]/baz bop`
    "###);

    let child = context.temp_dir.child("baz bop");
    let pyproject = fs_err::read_to_string(child.join("pyproject.toml"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject, @r###"
        [project]
        name = "baz-bop"
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

    let workspace = context.read("pyproject.toml");

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
    let workspace = context.read("pyproject.toml");

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

    let workspace = context.read("pyproject.toml");

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
fn init_no_workspace_warning() {
    let context = TestContext::new("3.12");

    uv_snapshot!(context.filters(), context.init().current_dir(&context.temp_dir).arg("--no-workspace").arg("--name").arg("project"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Initialized project `project`
    "###);

    let workspace = context.read("pyproject.toml");

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

    let workspace = context.read("pyproject.toml");
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
            pyproject, @r#"
        [project]
        name = "foo"
        version = "0.1.0"
        description = "Add your description here"
        readme = "README.md"
        requires-python = ">=3.12"
        dependencies = []
        "#
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
            pyproject, @r#"
        [project]
        name = "foo"
        version = "0.1.0"
        description = "Add your description here"
        readme = "README.md"
        requires-python = ">=3.12"
        dependencies = []

        [tool.uv.workspace]
        members = ["bar"]
        "#
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

    let pyproject = context.read("foo/pyproject.toml");
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject, @r#"
        [project]
        name = "foo"
        version = "0.1.0"
        description = "Add your description here"
        readme = "README.md"
        requires-python = ">=3.12"
        dependencies = []
        "#
        );
    });

    let workspace = context.read("pyproject.toml");
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

    let workspace = context.read("pyproject.toml");
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

    let workspace = context.read("pyproject.toml");
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

/// Run `uv init`, inferring the `requires-python` from the `.python-version` file.
#[test]
fn init_requires_python_version_file() -> Result<()> {
    let context = TestContext::new_with_versions(&["3.8", "3.12"]);

    context.temp_dir.child(".python-version").write_str("3.8")?;

    let child = context.temp_dir.join("foo");
    uv_snapshot!(context.filters(), context.init().current_dir(&context.temp_dir).arg(&child), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
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

    Ok(())
}

/// Run `uv init`, inferring the Python version from an existing `.venv`
#[test]
fn init_existing_environment() -> Result<()> {
    let context = TestContext::new_with_versions(&["3.8", "3.12"]);

    let child = context.temp_dir.child("foo");
    child.create_dir_all()?;

    // Create a new virtual environment in the directory
    uv_snapshot!(context.filters(), context.venv().current_dir(&child).arg("--python").arg("3.12"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: .venv
    Activate with: source .venv/[BIN]/activate
    "###);

    uv_snapshot!(context.filters(), context.init().current_dir(&context.temp_dir).arg(child.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
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
        requires-python = ">=3.12"
        dependencies = []
        "###
        );
    });

    Ok(())
}

/// Run `uv init`, it should ignore a the Python version from a parent `.venv`
#[test]
fn init_existing_environment_parent() -> Result<()> {
    let context = TestContext::new_with_versions(&["3.8", "3.12"]);

    // Create a new virtual environment in the parent directory
    uv_snapshot!(context.filters(), context.venv().current_dir(&context.temp_dir).arg("--python").arg("3.12"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: .venv
    Activate with: source .venv/[BIN]/activate
    "###);

    let child = context.temp_dir.child("foo");

    uv_snapshot!(context.filters(), context.init().current_dir(&context.temp_dir).arg(child.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
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

    let workspace = context.read("pyproject.toml");
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

    let workspace = context.read("foo/pyproject.toml");
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

#[test]
fn init_failure_with_invalid_option_named_backend() {
    let context = TestContext::new("3.12");
    uv_snapshot!(context.filters(), context.init().arg("foo").arg("--backend"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: unexpected argument '--backend' found

      tip: a similar argument exists: '--build-backend'

    Usage: uv init [OPTIONS] [PATH]

    For more information, try '--help'.
    "###);
    uv_snapshot!(context.filters(), context.init().arg("foo").arg("--backend").arg("maturin"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: unexpected argument '--backend' found

      tip: a similar argument exists: '--build-backend'

    Usage: uv init [OPTIONS] [PATH]

    For more information, try '--help'.
    "###);
}
#[test]
#[cfg(feature = "git")]
fn init_git() -> Result<()> {
    let context = TestContext::new("3.12");

    let child = context.temp_dir.child("foo");

    uv_snapshot!(context.filters(), context.init().arg(child.as_ref()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Initialized project `foo` at `[TEMP_DIR]/foo`
    "###);

    let gitignore = fs_err::read_to_string(child.join(".gitignore"))?;
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            gitignore, @r###"
        # Python-generated files
        __pycache__/
        *.py[oc]
        build/
        dist/
        wheels/
        *.egg-info

        # Virtual environments
        .venv
        "###
        );
    });

    child.child(".git").assert(predicate::path::is_dir());

    Ok(())
}

#[test]
fn init_vcs_none() {
    let context = TestContext::new("3.12");

    let child = context.temp_dir.child("foo");

    uv_snapshot!(context.filters(), context.init().arg(child.as_ref()).arg("--vcs").arg("none"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Initialized project `foo` at `[TEMP_DIR]/foo`
    "###);

    child.child(".gitignore").assert(predicate::path::missing());
    child.child(".git").assert(predicate::path::missing());
}

/// Run `uv init` from within a Git repository. Do not try to reinitialize one.
#[test]
#[cfg(feature = "git")]
fn init_inside_git_repo() {
    let context = TestContext::new("3.12");

    Command::new("git")
        .arg("init")
        .current_dir(&context.temp_dir)
        .assert()
        .success();

    let child = context.temp_dir.child("foo");

    uv_snapshot!(context.filters(), context.init().arg(child.as_ref()).arg("--vcs").arg("git"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Initialized project `foo` at `[TEMP_DIR]/foo`
    "###);

    child.child(".gitignore").assert(predicate::path::missing());

    let child = context.temp_dir.child("bar");
    uv_snapshot!(context.filters(), context.init().arg(child.as_ref()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Initialized project `bar` at `[TEMP_DIR]/bar`
    "###);

    child.child(".gitignore").assert(predicate::path::missing());
}

#[test]
fn init_git_not_installed() {
    let context = TestContext::new("3.12");

    let child = context.temp_dir.child("foo");

    // Without explicit `--vcs git`, `uv init` succeeds without initializing a Git repository.
    uv_snapshot!(context.filters(), context.init().env(EnvVars::PATH, &*child).arg(child.as_ref()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Initialized project `foo` at `[TEMP_DIR]/foo`
    "###);

    // With explicit `--vcs git`, `uv init` will fail.
    let child = context.temp_dir.child("bar");
    // Set `PATH` to child to make `git` command cannot be found.
    uv_snapshot!(context.filters(), context.init().env(EnvVars::PATH, &*child).arg(child.as_ref()).arg("--vcs").arg("git"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Attempted to initialize a Git repository, but `git` was not found in PATH
    "###);
}

#[test]
fn init_with_author() {
    let context = TestContext::new("3.12");

    // Create a Git repository and set the author.
    Command::new("git")
        .arg("init")
        .current_dir(&context.temp_dir)
        .assert()
        .success();
    Command::new("git")
        .arg("config")
        .arg("--local")
        .arg("user.name")
        .arg("Alice")
        .current_dir(&context.temp_dir)
        .assert()
        .success();
    Command::new("git")
        .arg("config")
        .arg("--local")
        .arg("user.email")
        .arg("alice@example.com")
        .current_dir(&context.temp_dir)
        .assert()
        .success();

    // `authors` is not filled for non-package application by default,
    context.init().arg("foo").assert().success();
    let pyproject = context.read("foo/pyproject.toml");
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject, @r#"
        [project]
        name = "foo"
        version = "0.1.0"
        description = "Add your description here"
        readme = "README.md"
        requires-python = ">=3.12"
        dependencies = []
        "#
        );
    });

    // use `--author-from auto` to explicitly fill it.
    context
        .init()
        .arg("bar")
        .arg("--author-from")
        .arg("auto")
        .assert()
        .success();
    let pyproject = context.read("bar/pyproject.toml");
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject, @r#"
        [project]
        name = "bar"
        version = "0.1.0"
        description = "Add your description here"
        readme = "README.md"
        authors = [
            { name = "Alice", email = "alice@example.com" }
        ]
        requires-python = ">=3.12"
        dependencies = []
        "#
        );
    });

    // Fill `authors` for library by default,
    context.init().arg("baz").arg("--lib").assert().success();
    let pyproject = context.read("baz/pyproject.toml");
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject, @r#"
        [project]
        name = "baz"
        version = "0.1.0"
        description = "Add your description here"
        readme = "README.md"
        authors = [
            { name = "Alice", email = "alice@example.com" }
        ]
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
        "#
        );
    });

    // use `--authors-from none` to prevent it.
    context
        .init()
        .arg("qux")
        .arg("--lib")
        .arg("--author-from")
        .arg("none")
        .assert()
        .success();
    let pyproject = context.read("qux/pyproject.toml");
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject, @r#"
        [project]
        name = "qux"
        version = "0.1.0"
        description = "Add your description here"
        readme = "README.md"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
        "#
        );
    });
}

/// Run `uv init --app --package --build-backend flit` to create a packaged application project
#[test]
fn init_application_package_flit() -> Result<()> {
    let context = TestContext::new("3.12");

    let child = context.temp_dir.child("foo");
    child.create_dir_all()?;

    let pyproject_toml = child.join("pyproject.toml");
    let init_py = child.join("src").join("foo").join("__init__.py");

    uv_snapshot!(context.filters(), context.init().current_dir(&child).arg("--app").arg("--package").arg("--build-backend").arg("flit"), @r###"
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
        foo = "foo:main"

        [build-system]
        requires = ["flit_core>=3.2,<4"]
        build-backend = "flit_core.buildapi"
        "###
        );
    });

    let init = fs_err::read_to_string(init_py)?;
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            init, @r###"
        def main() -> None:
            print("Hello from foo!")
        "###
        );
    });

    uv_snapshot!(context.filters(), context.run().current_dir(&child).env_remove(EnvVars::VIRTUAL_ENV).arg("foo"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello from foo!

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: .venv
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==0.1.0 (from file://[TEMP_DIR]/foo)
    "###);

    Ok(())
}

/// Run `uv init --lib --build-backend flit` to create an library project
#[test]
fn init_library_flit() -> Result<()> {
    let context = TestContext::new("3.12");

    let child = context.temp_dir.child("foo");
    child.create_dir_all()?;

    let pyproject_toml = child.join("pyproject.toml");
    let init_py = child.join("src").join("foo").join("__init__.py");
    let py_typed = child.join("src").join("foo").join("py.typed");

    uv_snapshot!(context.filters(), context.init().current_dir(&child).arg("--lib").arg("--build-backend").arg("flit"), @r###"
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
        requires = ["flit_core>=3.2,<4"]
        build-backend = "flit_core.buildapi"
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

    uv_snapshot!(context.filters(), context.run().current_dir(&child).env_remove(EnvVars::VIRTUAL_ENV).arg("python").arg("-c").arg("import foo; print(foo.hello())"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello from foo!

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: .venv
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==0.1.0 (from file://[TEMP_DIR]/foo)
    "###);

    Ok(())
}

/// Run `uv init --build-backend flit` should be equivalent to `uv init --package --build-backend flit`.
#[test]
fn init_backend_implies_package() {
    let context = TestContext::new("3.12");

    uv_snapshot!(context.filters(), context.init().arg("project").arg("--build-backend").arg("flit"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Initialized project `project` at `[TEMP_DIR]/project`
    "#);

    let pyproject = context.read("project/pyproject.toml");
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        description = "Add your description here"
        readme = "README.md"
        requires-python = ">=3.12"
        dependencies = []

        [project.scripts]
        project = "project:main"

        [build-system]
        requires = ["flit_core>=3.2,<4"]
        build-backend = "flit_core.buildapi"
        "#
        );
    });
}

/// Run `uv init --app --package --build-backend maturin` to create a packaged application project
#[test]
#[cfg(feature = "crates-io")]
fn init_app_build_backend_maturin() -> Result<()> {
    let context = TestContext::new("3.12");

    let child = context.temp_dir.child("foo");
    child.create_dir_all()?;

    let pyproject_toml = child.join("pyproject.toml");
    let init_py = child.join("src").join("foo").join("__init__.py");
    let pyi_file = child.join("src").join("foo").join("_core.pyi");
    let lib_core = child.join("src").join("lib.rs");
    let build_file = child.join("Cargo.toml");

    uv_snapshot!(context.filters(), context.init().current_dir(&child).arg("--app").arg("--package").arg("--build-backend").arg("maturin"), @r###"
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
        foo = "foo:main"

        [tool.maturin]
        module-name = "foo._core"
        python-packages = ["foo"]
        python-source = "src"

        [build-system]
        requires = ["maturin>=1.0,<2.0"]
        build-backend = "maturin"
        "###
        );
    });

    let init = fs_err::read_to_string(init_py)?;
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            init, @r###"
        from foo._core import hello_from_bin


        def main() -> None:
            print(hello_from_bin())
        "###
        );
    });

    let pyi_contents = fs_err::read_to_string(pyi_file)?;
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyi_contents, @r###"
        def hello_from_bin() -> str: ...
        "###
        );
    });

    let lib_core_contents = fs_err::read_to_string(lib_core)?;
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lib_core_contents, @r###"
        use pyo3::prelude::*;

        #[pyfunction]
        fn hello_from_bin() -> String {
            "Hello from foo!".to_string()
        }

        /// A Python module implemented in Rust. The name of this function must match
        /// the `lib.name` setting in the `Cargo.toml`, else Python will not be able to
        /// import the module.
        #[pymodule]
        fn _core(m: &Bound<'_, PyModule>) -> PyResult<()> {
            m.add_function(wrap_pyfunction!(hello_from_bin, m)?)?;
            Ok(())
        }
        "###
        );
    });

    let build_file_contents = fs_err::read_to_string(build_file)?;
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            build_file_contents, @r###"
        [package]
        name = "foo"
        version = "0.1.0"
        edition = "2021"

        [lib]
        name = "_core"
        # "cdylib" is necessary to produce a shared library for Python to import from.
        crate-type = ["cdylib"]

        [dependencies]
        # "extension-module" tells pyo3 we want to build an extension module (skips linking against libpython.so)
        # "abi3-py39" tells pyo3 (and maturin) to build using the stable ABI with minimum Python version 3.9
        pyo3 = { version = "0.22.4", features = ["extension-module", "abi3-py39"] }
        "###
        );
    });

    Ok(())
}

/// Run `uv init --app --package --build-backend scikit` to create a packaged application project
#[test]
fn init_app_build_backend_scikit() -> Result<()> {
    let context = TestContext::new("3.12");

    let child = context.temp_dir.child("foo");
    child.create_dir_all()?;

    let pyproject_toml = child.join("pyproject.toml");
    let init_py = child.join("src").join("foo").join("__init__.py");
    let pyi_file = child.join("src").join("foo").join("_core.pyi");
    let lib_core = child.join("src").join("main.cpp");
    let build_file = child.join("CMakeLists.txt");

    uv_snapshot!(context.filters(), context.init().current_dir(&child).arg("--app").arg("--package").arg("--build-backend").arg("scikit"), @r###"
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
        foo = "foo:main"

        [tool.scikit-build]
        minimum-version = "build-system.requires"
        build-dir = "build/{wheel_tag}"

        [build-system]
        requires = ["scikit-build-core>=0.10", "pybind11"]
        build-backend = "scikit_build_core.build"
        "###
        );
    });

    let init = fs_err::read_to_string(init_py)?;
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            init, @r###"
        from foo._core import hello_from_bin


        def main() -> None:
            print(hello_from_bin())
        "###
        );
    });

    let pyi_contents = fs_err::read_to_string(pyi_file)?;
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyi_contents, @r###"
        def hello_from_bin() -> str: ...
        "###
        );
    });

    let lib_core_contents = fs_err::read_to_string(lib_core)?;
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lib_core_contents, @r###"
        #include <pybind11/pybind11.h>

        std::string hello_from_bin() { return "Hello from foo!"; }

        namespace py = pybind11;

        PYBIND11_MODULE(_core, m) {
          m.doc() = "pybind11 hello module";

          m.def("hello_from_bin", &hello_from_bin, R"pbdoc(
              A function that returns a Hello string.
          )pbdoc");
        }
        "###
        );
    });

    let build_file_contents = fs_err::read_to_string(build_file)?;
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            build_file_contents, @r###"
        cmake_minimum_required(VERSION 3.15)
        project(${SKBUILD_PROJECT_NAME} LANGUAGES CXX)

        set(PYBIND11_FINDPYTHON ON)
        find_package(pybind11 CONFIG REQUIRED)

        pybind11_add_module(_core MODULE src/main.cpp)
        install(TARGETS _core DESTINATION ${SKBUILD_PROJECT_NAME})
        "###
        );
    });

    // We do not test with uv run since it would otherwise require specific CXX build tooling

    Ok(())
}

/// Run `uv init --lib --build-backend maturin` to create a packaged application project
#[test]
#[cfg(feature = "crates-io")]
fn init_lib_build_backend_maturin() -> Result<()> {
    let context = TestContext::new("3.12");

    let child = context.temp_dir.child("foo");
    child.create_dir_all()?;

    let pyproject_toml = child.join("pyproject.toml");
    let init_py = child.join("src").join("foo").join("__init__.py");
    let pyi_file = child.join("src").join("foo").join("_core.pyi");
    let lib_core = child.join("src").join("lib.rs");
    let build_file = child.join("Cargo.toml");

    uv_snapshot!(context.filters(), context.init().current_dir(&child).arg("--lib").arg("--build-backend").arg("maturin"), @r###"
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

        [tool.maturin]
        module-name = "foo._core"
        python-packages = ["foo"]
        python-source = "src"

        [build-system]
        requires = ["maturin>=1.0,<2.0"]
        build-backend = "maturin"
        "###
        );
    });

    let init = fs_err::read_to_string(init_py)?;
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            init, @r###"
        from foo._core import hello_from_bin


        def hello() -> str:
            return hello_from_bin()
        "###
        );
    });

    let pyi_contents = fs_err::read_to_string(pyi_file)?;
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyi_contents, @r###"
        def hello_from_bin() -> str: ...
        "###
        );
    });

    let lib_core_contents = fs_err::read_to_string(lib_core)?;
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lib_core_contents, @r###"
        use pyo3::prelude::*;

        #[pyfunction]
        fn hello_from_bin() -> String {
            "Hello from foo!".to_string()
        }

        /// A Python module implemented in Rust. The name of this function must match
        /// the `lib.name` setting in the `Cargo.toml`, else Python will not be able to
        /// import the module.
        #[pymodule]
        fn _core(m: &Bound<'_, PyModule>) -> PyResult<()> {
            m.add_function(wrap_pyfunction!(hello_from_bin, m)?)?;
            Ok(())
        }
        "###
        );
    });

    let build_file_contents = fs_err::read_to_string(build_file)?;
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            build_file_contents, @r###"
        [package]
        name = "foo"
        version = "0.1.0"
        edition = "2021"

        [lib]
        name = "_core"
        # "cdylib" is necessary to produce a shared library for Python to import from.
        crate-type = ["cdylib"]

        [dependencies]
        # "extension-module" tells pyo3 we want to build an extension module (skips linking against libpython.so)
        # "abi3-py39" tells pyo3 (and maturin) to build using the stable ABI with minimum Python version 3.9
        pyo3 = { version = "0.22.4", features = ["extension-module", "abi3-py39"] }
        "###
        );
    });

    Ok(())
}

/// Run `uv init --lib --build-backend scikit` to create a packaged application project
#[test]
fn init_lib_build_backend_scikit() -> Result<()> {
    let context = TestContext::new("3.12");

    let child = context.temp_dir.child("foo");
    child.create_dir_all()?;

    let pyproject_toml = child.join("pyproject.toml");
    let init_py = child.join("src").join("foo").join("__init__.py");
    let pyi_file = child.join("src").join("foo").join("_core.pyi");
    let lib_core = child.join("src").join("main.cpp");
    let build_file = child.join("CMakeLists.txt");

    uv_snapshot!(context.filters(), context.init().current_dir(&child).arg("--lib").arg("--build-backend").arg("scikit"), @r###"
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

        [tool.scikit-build]
        minimum-version = "build-system.requires"
        build-dir = "build/{wheel_tag}"

        [build-system]
        requires = ["scikit-build-core>=0.10", "pybind11"]
        build-backend = "scikit_build_core.build"
        "###
        );
    });

    let init = fs_err::read_to_string(init_py)?;
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            init, @r###"
        from foo._core import hello_from_bin


        def hello() -> str:
            return hello_from_bin()
        "###
        );
    });

    let pyi_contents = fs_err::read_to_string(pyi_file)?;
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyi_contents, @r###"
        def hello_from_bin() -> str: ...
        "###
        );
    });

    let lib_core_contents = fs_err::read_to_string(lib_core)?;
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lib_core_contents, @r###"
        #include <pybind11/pybind11.h>

        std::string hello_from_bin() { return "Hello from foo!"; }

        namespace py = pybind11;

        PYBIND11_MODULE(_core, m) {
          m.doc() = "pybind11 hello module";

          m.def("hello_from_bin", &hello_from_bin, R"pbdoc(
              A function that returns a Hello string.
          )pbdoc");
        }
        "###
        );
    });

    let build_file_contents = fs_err::read_to_string(build_file)?;
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            build_file_contents, @r###"
        cmake_minimum_required(VERSION 3.15)
        project(${SKBUILD_PROJECT_NAME} LANGUAGES CXX)

        set(PYBIND11_FINDPYTHON ON)
        find_package(pybind11 CONFIG REQUIRED)

        pybind11_add_module(_core MODULE src/main.cpp)
        install(TARGETS _core DESTINATION ${SKBUILD_PROJECT_NAME})
        "###
        );
    });

    // We do not test with uv run since it would otherwise require specific CXX build tooling

    Ok(())
}

/// Run `uv init --app --package --build-backend uv` to create a packaged application project
#[test]
fn init_application_package_uv() -> Result<()> {
    let context = TestContext::new("3.12");

    let child = context.temp_dir.child("foo");
    child.create_dir_all()?;

    let pyproject_toml = child.join("pyproject.toml");
    let init_py = child.join("src").join("foo").join("__init__.py");

    uv_snapshot!(context.filters(), context.init().current_dir(&child).arg("--app").arg("--package").arg("--build-backend").arg("uv"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: The uv build backend is experimental and may change without warning
    Initialized project `foo`
    "###);

    let pyproject = fs_err::read_to_string(&pyproject_toml)?;
    let mut filters = context.filters();
    filters.push((r#"\["uv_build>=.*,<.*"\]"#, r#"["uv_build[SPECIFIERS]"]"#));
    insta::with_settings!({
        filters => filters,
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
        foo = "foo:main"

        [build-system]
        requires = ["uv_build[SPECIFIERS]"]
        build-backend = "uv_build"
        "###
        );
    });

    let init = fs_err::read_to_string(init_py)?;
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            init, @r###"
        def main() -> None:
            print("Hello from foo!")
        "###
        );
    });

    // Use preview to go through the fast path.
    uv_snapshot!(context.filters(), context.run().arg("--preview").arg("foo").current_dir(&child).env_remove(EnvVars::VIRTUAL_ENV), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello from foo!

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: .venv
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==0.1.0 (from file://[TEMP_DIR]/foo)
    "###);

    Ok(())
}

#[test]
fn init_with_description() -> Result<()> {
    let context = TestContext::new("3.12");

    let child = context.temp_dir.join("foo");
    fs_err::create_dir_all(&child)?;

    // Initialize the project with a description
    context
        .init()
        .current_dir(&child)
        .arg("--description")
        .arg("A sample project description")
        .arg("--lib")
        .assert()
        .success();

    // Read the generated pyproject.toml
    let pyproject = fs_err::read_to_string(child.join("pyproject.toml"))?;

    // Verify the description in pyproject.toml
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject, @r#"
        [project]
        name = "foo"
        version = "0.1.0"
        description = "A sample project description"
        readme = "README.md"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
        "#
        );
    });

    Ok(())
}

#[test]
fn init_without_description() -> Result<()> {
    let context = TestContext::new("3.12");

    let child = context.temp_dir.join("bar");
    fs_err::create_dir_all(&child)?;

    // Initialize the project without a description
    context
        .init()
        .current_dir(&child)
        .arg("--lib")
        .assert()
        .success();

    // Read the generated pyproject.toml
    let pyproject = fs_err::read_to_string(child.join("pyproject.toml"))?;

    // Verify the default description in pyproject.toml
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject, @r#"
        [project]
        name = "bar"
        version = "0.1.0"
        description = "Add your description here"
        readme = "README.md"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
        "#
        );
    });

    Ok(())
}
