#![allow(clippy::disallowed_types)]

use anyhow::Result;
use assert_cmd::assert::OutputAssertExt;
use assert_fs::{fixture::ChildPath, prelude::*};
use indoc::indoc;
use insta::assert_snapshot;
use predicates::{prelude::predicate, str::contains};
use std::path::Path;
use uv_fs::copy_dir_all;
use uv_python::PYTHON_VERSION_FILENAME;
use uv_static::EnvVars;

use uv_test::{TestContext, uv_snapshot};

#[test]
fn run_with_python_version() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&["3.12", "3.11", "3.9"]);

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.11, <4"
        dependencies = [
          "anyio==3.6.0 ; python_version == '3.11'",
          "anyio==3.7.0 ; python_version == '3.12'",
        ]

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"
        "#
    })?;
    context
        .temp_dir
        .child("src")
        .child("foo")
        .child("__init__.py")
        .touch()?;
    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        import importlib.metadata
        import platform

        print(platform.python_version())
        print(importlib.metadata.version("anyio"))
       "#
    })?;

    // Our tests change files in <1s, so we must disable CPython bytecode caching with `-B` or we'll
    // get stale files, see https://github.com/python/cpython/issues/75953.
    let mut command = context.run();
    let command_with_args = command.arg("python").arg("-B").arg("main.py");
    uv_snapshot!(context.filters(), command_with_args, @"
    success: true
    exit_code: 0
    ----- stdout -----
    3.12.[X]
    3.7.0

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: .venv
    Resolved 5 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==3.7.0
     + foo==1.0.0 (from file://[TEMP_DIR]/)
     + idna==3.6
     + sniffio==1.3.1
    ");

    // This is the same Python, no reinstallation.
    let mut command = context.run();
    let command_with_args = command
        .arg("-p")
        .arg("3.12")
        .arg("python")
        .arg("-B")
        .arg("main.py");
    uv_snapshot!(context.filters(), command_with_args, @"
    success: true
    exit_code: 0
    ----- stdout -----
    3.12.[X]
    3.7.0

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Audited 4 packages in [TIME]
    ");

    // This time, we target Python 3.11 instead.
    let mut command = context.run();
    let command_with_args = command
        .arg("-p")
        .arg("3.11")
        .arg("python")
        .arg("-B")
        .arg("main.py")
        .env_remove(EnvVars::VIRTUAL_ENV);

    uv_snapshot!(context.filters(), command_with_args, @"
    success: true
    exit_code: 0
    ----- stdout -----
    3.11.[X]
    3.6.0

    ----- stderr -----
    Using CPython 3.11.[X] interpreter at: [PYTHON-3.11]
    Removed virtual environment at: .venv
    Creating virtual environment at: .venv
    Resolved 5 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 4 packages in [TIME]
     + anyio==3.6.0
     + foo==1.0.0 (from file://[TEMP_DIR]/)
     + idna==3.6
     + sniffio==1.3.1
    ");

    // This time, we target Python 3.9 instead.
    let mut command = context.run();
    let command_with_args = command
        .arg("-p")
        .arg("3.9")
        .arg("python")
        .arg("-B")
        .arg("main.py")
        .env_remove(EnvVars::VIRTUAL_ENV);

    uv_snapshot!(context.filters(), command_with_args, @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.9.[X] interpreter at: [PYTHON-3.9]
    error: The requested interpreter resolved to Python 3.9.[X], which is incompatible with the project's Python requirement: `>=3.11, <4` (from `project.requires-python`)
    ");

    Ok(())
}

#[test]
fn run_args() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let context = context
        .with_filter((r"Usage: uv(\.exe)? run \[OPTIONS\] (?s).*", "[UV RUN HELP]"))
        .with_filter((r"usage: .*(\n|.*)*", "usage: [PYTHON HELP]"));

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.8"
        dependencies = []

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"
        "#
    })?;
    context
        .temp_dir
        .child("src")
        .child("foo")
        .child("__init__.py")
        .touch()?;

    // We treat arguments before the command as uv arguments
    uv_snapshot!(context.filters(), context.run().arg("--help").arg("python"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Run a command or script

    [UV RUN HELP]
    ");

    // We don't treat arguments after the command as uv arguments
    uv_snapshot!(context.filters(), context.run().arg("python").arg("--help"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    usage: [PYTHON HELP]
    ");

    // Can use `--` to separate uv arguments from the command arguments.
    uv_snapshot!(context.filters(), context.run().arg("--").arg("python").arg("--version"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.[X]

    ----- stderr -----
    Resolved 1 package in [TIME]
    Audited 1 package in [TIME]
    ");

    Ok(())
}

/// Run without specifying any arguments.
///
/// This should list the available scripts.
#[test]
fn run_no_args() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.8"
        dependencies = []

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"
        "#
    })?;
    context
        .temp_dir
        .child("src")
        .child("foo")
        .child("__init__.py")
        .touch()?;

    // Run without specifying any arguments.
    #[cfg(not(windows))]
    uv_snapshot!(context.filters(), context.run(), @"
    success: false
    exit_code: 2
    ----- stdout -----
    Provide a command or script to invoke with `uv run <command>` or `uv run <script>.py`.

    The following commands are available in the environment:

    - python
    - python3
    - python3.12

    See `uv run --help` for more information.

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/)
    ");

    #[cfg(windows)]
    uv_snapshot!(context.filters(), context.run(), @r###"
    success: false
    exit_code: 2
    ----- stdout -----
    Provide a command or script to invoke with `uv run <command>` or `uv run <script>.py`.

    The following commands are available in the environment:

    - pydoc.bat
    - python
    - pythonw

    See `uv run --help` for more information.

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/)
    "###);

    Ok(())
}

/// Run a PEP 723-compatible script. The script should take precedence over the workspace
/// dependencies.
#[test]
fn run_pep723_script() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.8"
        dependencies = ["anyio"]

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"
        "#
    })?;
    context
        .temp_dir
        .child("src")
        .child("foo")
        .child("__init__.py")
        .touch()?;

    // If the script contains a PEP 723 tag, we should install its requirements.
    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        # /// script
        # requires-python = ">=3.11"
        # dependencies = [
        #   "iniconfig",
        # ]
        # ///

        import iniconfig
       "#
    })?;

    // Running the script should install the requirements.
    uv_snapshot!(context.filters(), context.run().arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    // Running again should use the existing environment.
    uv_snapshot!(context.filters(), context.run().arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    ");

    // But neither invocation should create a lockfile.
    assert!(!context.temp_dir.child("main.py.lock").exists());

    // Otherwise, the script requirements should _not_ be available, but the project requirements
    // should.
    let test_non_script = context.temp_dir.child("main.py");
    test_non_script.write_str(indoc! { r"
        import iniconfig
       "
    })?;

    uv_snapshot!(context.filters(), context.run().arg("main.py"), @r#"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.3.0
     + foo==1.0.0 (from file://[TEMP_DIR]/)
     + idna==3.6
     + sniffio==1.3.1
    Traceback (most recent call last):
      File "[TEMP_DIR]/main.py", line 1, in <module>
        import iniconfig
    ModuleNotFoundError: No module named 'iniconfig'
    "#);

    // But the script should be runnable.
    let test_non_script = context.temp_dir.child("main.py");
    test_non_script.write_str(indoc! { r#"
        import idna

        print("Hello, world!")
       "#
    })?;

    uv_snapshot!(context.filters(), context.run().arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello, world!

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Audited 4 packages in [TIME]
    ");

    // If the script contains a PEP 723 tag, it can omit the dependencies field.
    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        # /// script
        # requires-python = ">=3.11"
        # ///

        print("Hello, world!")
       "#
    })?;

    // Running the script should install the requirements.
    uv_snapshot!(context.filters(), context.run().arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello, world!

    ----- stderr -----
    ");

    // Running a script with `--locked` should warn.
    uv_snapshot!(context.filters(), context.run().arg("--locked").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello, world!

    ----- stderr -----
    warning: No lockfile found for Python script (ignoring `--locked`); run `uv lock --script` to generate a lockfile
    ");

    // If the script can't be resolved, we should reference the script.
    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        # /// script
        # requires-python = ">=3.11"
        # dependencies = [
        #   "add",
        # ]
        # ///
       "#
    })?;

    // Running a script with `--group` should warn.
    uv_snapshot!(context.filters(), context.run().arg("--group").arg("foo").arg("main.py"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving script dependencies:
      ╰─▶ Because there are no versions of add and you require add, we can conclude that your requirements are unsatisfiable.
    ");

    // If the script can't be resolved, we should reference the script.
    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        # /// script
        # requires-python = ">=3.11"
        # dependencies = [
        #   "add",
        # ]
        # ///
       "#
    })?;

    uv_snapshot!(context.filters(), context.run().arg("--no-project").arg("main.py"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving script dependencies:
      ╰─▶ Because there are no versions of add and you require add, we can conclude that your requirements are unsatisfiable.
    ");

    // If the script contains an unclosed PEP 723 tag, we should error.
    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        # /// script
        # requires-python = ">=3.11"
        # dependencies = [
        #   "iniconfig",
        # ]

        # ///

        import iniconfig
       "#
    })?;

    uv_snapshot!(context.filters(), context.run().arg("--no-project").arg("main.py"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: An opening tag (`# /// script`) was found without a closing tag (`# ///`). Ensure that every line between the opening and closing tags (including empty lines) starts with a leading `#`.
    ");

    Ok(())
}

#[test]
fn run_pep723_script_requires_python() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&["3.9", "3.11"]);

    // If we have a `.python-version` that's incompatible with the script, we should error.
    let python_version = context.temp_dir.child(PYTHON_VERSION_FILENAME);
    python_version.write_str("3.9")?;

    // If the script contains a PEP 723 tag, we should install its requirements.
    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        # /// script
        # requires-python = ">=3.11"
        # dependencies = [
        #   "iniconfig",
        # ]
        # ///

        import iniconfig

        x: str | int = "hello"
        print(x)
       "#
    })?;

    uv_snapshot!(context.filters(), context.run().arg("main.py"), @r#"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    warning: The Python request from `.python-version` resolved to Python 3.9.[X], which is incompatible with the script's Python requirement: `>=3.11`
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    Traceback (most recent call last):
      File "[TEMP_DIR]/main.py", line 10, in <module>
        x: str | int = "hello"
    TypeError: unsupported operand type(s) for |: 'type' and 'type'
    "#);

    // Delete the `.python-version` file to allow the script to run.
    fs_err::remove_file(&python_version)?;

    uv_snapshot!(context.filters(), context.run().arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    hello

    ----- stderr -----
    Resolved 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    Ok(())
}

/// Run a `.pyw` script. The script should be executed with `pythonw.exe`.
#[test]
fn run_pythonw_script() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.8"
        dependencies = ["anyio"]

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"
        "#
    })?;
    context
        .temp_dir
        .child("src")
        .child("foo")
        .child("__init__.py")
        .touch()?;

    let test_script = context.temp_dir.child("main.pyw");
    test_script.write_str(indoc! { r"
        import anyio
       "
    })?;

    uv_snapshot!(context.filters(), context.run().arg("main.pyw"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.3.0
     + foo==1.0.0 (from file://[TEMP_DIR]/)
     + idna==3.6
     + sniffio==1.3.1
    ");

    Ok(())
}

/// Run a PEP 723-compatible script with `tool.uv` metadata.
#[test]
#[cfg(feature = "test-git")]
fn run_pep723_script_metadata() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    // If the script contains a PEP 723 tag, we should install its requirements.
    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        # /// script
        # requires-python = ">=3.11"
        # dependencies = [
        #   "iniconfig>1",
        # ]
        #
        # [tool.uv]
        # resolution = "lowest-direct"
        # ///

        import iniconfig
       "#
    })?;

    // Running the script should fail without network access.
    uv_snapshot!(context.filters(), context.run().arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==1.0.1
    ");

    // Respect `tool.uv.sources`.
    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        # /// script
        # requires-python = ">=3.11"
        # dependencies = [
        #   "uv-public-pypackage",
        # ]
        #
        # [tool.uv.sources]
        # uv-public-pypackage = { git = "https://github.com/astral-test/uv-public-pypackage", rev = "0dacfd662c64cb4ceb16e6cf65a157a8b715b979" }
        # ///

        import uv_public_pypackage
       "#
    })?;

    // The script should succeed with the specified source.
    uv_snapshot!(context.filters(), context.run().arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + uv-public-pypackage==0.1.0 (from git+https://github.com/astral-test/uv-public-pypackage@0dacfd662c64cb4ceb16e6cf65a157a8b715b979)
    ");

    Ok(())
}

/// Run a PEP 723-compatible script with a `[[tool.uv.index]]`.
#[test]
fn run_pep723_script_index() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        # /// script
        # requires-python = ">=3.11"
        # dependencies = [
        #   "idna>=2",
        # ]
        #
        # [[tool.uv.index]]
        # name = "test"
        # url = "https://test.pypi.org/simple"
        # explicit = true
        #
        # [tool.uv.sources]
        # idna = { index = "test" }
        # ///

        import idna
       "#
    })?;

    uv_snapshot!(context.filters(), context.run().arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + idna==2.7
    ");

    Ok(())
}

/// Run a PEP 723-compatible script with `tool.uv` constraints.
#[test]
fn run_pep723_script_constraints() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        # /// script
        # requires-python = ">=3.11"
        # dependencies = [
        #   "anyio>=3",
        # ]
        #
        # [tool.uv]
        # constraint-dependencies = ["idna<=3"]
        # ///

        import anyio
       "#
    })?;

    uv_snapshot!(context.filters(), context.run().arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.3.0
     + idna==3.0
     + sniffio==1.3.1
    ");

    Ok(())
}

/// Run a PEP 723-compatible script with `tool.uv` overrides.
#[test]
fn run_pep723_script_overrides() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        # /// script
        # requires-python = ">=3.11"
        # dependencies = [
        #   "anyio>=3",
        # ]
        #
        # [tool.uv]
        # override-dependencies = ["idna<=2"]
        # ///

        import anyio
       "#
    })?;

    uv_snapshot!(context.filters(), context.run().arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.3.0
     + idna==2.0
     + sniffio==1.3.1
    ");

    Ok(())
}

/// Run a PEP 723-compatible script with `tool.uv` build constraints.
#[test]
fn run_pep723_script_build_constraints() -> Result<()> {
    let context = uv_test::test_context!("3.9");

    let test_script = context.temp_dir.child("main.py");

    // Incompatible build constraints.
    test_script.write_str(indoc! { r#"
        # /// script
        # requires-python = ">=3.9"
        # dependencies = [
        #   "anyio>=3",
        #   "requests==1.2"
        # ]
        #
        # [tool.uv]
        # build-constraint-dependencies = ["setuptools==1"]
        # ///

        import anyio
       "#
    })?;

    uv_snapshot!(context.filters(), context.run().arg("main.py"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to download and build `requests==1.2.0`
      ├─▶ Failed to resolve requirements from `setup.py` build
      ├─▶ No solution found when resolving: `setuptools>=40.8.0`
      ╰─▶ Because you require setuptools>=40.8.0 and setuptools==1, we can conclude that your requirements are unsatisfiable.
    ");

    // Compatible build constraints.
    test_script.write_str(indoc! { r#"
        # /// script
        # requires-python = ">=3.9"
        # dependencies = [
        #   "anyio>=3",
        #   "requests==1.2"
        # ]
        #
        # [tool.uv]
        # build-constraint-dependencies = ["setuptools>=40"]
        # ///

        import anyio
       "#
    })?;

    uv_snapshot!(context.filters(), context.run().arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 6 packages in [TIME]
    Installed 6 packages in [TIME]
     + anyio==4.3.0
     + exceptiongroup==1.2.0
     + idna==3.6
     + requests==1.2.0
     + sniffio==1.3.1
     + typing-extensions==4.10.0
    ");

    Ok(())
}

/// Run a PEP 723-compatible script with a lockfile.
#[test]
fn run_pep723_script_lock() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        # /// script
        # requires-python = ">=3.11"
        # dependencies = [
        #   "iniconfig",
        # ]
        # ///

        import iniconfig

        print("Hello, world!")
       "#
    })?;

    // Without a lockfile, running with `--locked` should warn.
    uv_snapshot!(context.filters(), context.run().arg("--locked").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello, world!

    ----- stderr -----
    warning: No lockfile found for Python script (ignoring `--locked`); run `uv lock --script` to generate a lockfile
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    // Explicitly lock the script.
    uv_snapshot!(context.filters(), context.lock().arg("--script").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    ");

    let lock = context.read("main.py.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.11"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        requirements = [{ name = "iniconfig" }]

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646, upload-time = "2023-01-07T11:08:11.254Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892, upload-time = "2023-01-07T11:08:09.864Z" },
        ]
        "#
        );
    });

    // Run the script.
    uv_snapshot!(context.filters(), context.run().arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello, world!

    ----- stderr -----
    Resolved 1 package in [TIME]
    Audited 1 package in [TIME]
    ");

    // With a lockfile, running with `--locked` should not warn.
    uv_snapshot!(context.filters(), context.run().arg("--locked").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello, world!

    ----- stderr -----
    Resolved 1 package in [TIME]
    Audited 1 package in [TIME]
    ");

    // Modify the metadata.
    test_script.write_str(indoc! { r#"
        # /// script
        # requires-python = ">=3.11"
        # dependencies = [
        #   "anyio",
        # ]
        # ///

        import anyio

        print("Hello, world!")
       "#
    })?;

    // Re-running the script with `--locked` should error.
    uv_snapshot!(context.filters(), context.run().arg("--locked").arg("main.py"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided. To update the lockfile, run `uv lock`.
    ");

    // Re-running the script with `--frozen` should also error, but at runtime.
    uv_snapshot!(context.filters(), context.run().arg("--frozen").arg("main.py"), @r#"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    Traceback (most recent call last):
      File "[TEMP_DIR]/main.py", line 8, in <module>
        import anyio
    ModuleNotFoundError: No module named 'anyio'
    "#);

    // Re-running the script should update the lockfile.
    uv_snapshot!(context.filters(), context.run().arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello, world!

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + sniffio==1.3.1
    ");

    let lock = context.read("main.py.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.11"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        requirements = [{ name = "anyio" }]

        [[package]]
        name = "anyio"
        version = "4.3.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "idna" },
            { name = "sniffio" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz", hash = "sha256:f75253795a87df48568485fd18cdd2a3fa5c4f7c5be8e5e36637733fce06fed6", size = 159642, upload-time = "2024-02-19T08:36:28.641Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl", hash = "sha256:048e05d0f6caeed70d731f3db756d35dcc1f35747c8c403364a8332c630441b8", size = 85584, upload-time = "2024-02-19T08:36:26.842Z" },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426, upload-time = "2023-11-25T15:40:54.902Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567, upload-time = "2023-11-25T15:40:52.604Z" },
        ]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372, upload-time = "2024-02-25T23:20:04.057Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235, upload-time = "2024-02-25T23:20:01.196Z" },
        ]
        "#
        );
    });

    Ok(())
}

/// With `managed = false`, we should avoid installing the project itself.
#[test]
fn run_managed_false() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.8"
        dependencies = ["anyio"]

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"

        [tool.uv]
        managed = false
        "#
    })?;

    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.[X]

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn run_exact() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.8"
        dependencies = ["iniconfig"]
        "#
    })?;

    uv_snapshot!(context.filters(), context.run().arg("python").arg("-c").arg("import iniconfig"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    // Remove `iniconfig`.
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.8"
        dependencies = ["anyio"]
        "#
    })?;

    // By default, `uv run` uses inexact semantics, so both `iniconfig` and `anyio` should still be available.
    uv_snapshot!(context.filters(), context.run().arg("python").arg("-c").arg("import iniconfig; import anyio"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + sniffio==1.3.1
    ");

    // But under `--exact`, `iniconfig` should not be available.
    uv_snapshot!(context.filters(), context.run().arg("--exact").arg("python").arg("-c").arg("import iniconfig"), @r#"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Uninstalled 1 package in [TIME]
     - iniconfig==2.0.0
    Traceback (most recent call last):
      File "<string>", line 1, in <module>
    ModuleNotFoundError: No module named 'iniconfig'
    "#);

    Ok(())
}

#[test]
fn run_with() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.8"
        dependencies = ["sniffio==1.3.0"]

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"
        "#
    })?;
    context
        .temp_dir
        .child("src")
        .child("foo")
        .child("__init__.py")
        .touch()?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r"
        import sniffio

        print(sniffio.__version__)
       "
    })?;

    // Requesting an unsatisfied requirement should install it.
    uv_snapshot!(context.filters(), context.run().arg("--with").arg("iniconfig").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    1.3.0

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/)
     + sniffio==1.3.0
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    // Requesting a satisfied requirement should use the base environment.
    uv_snapshot!(context.filters(), context.run().arg("--with").arg("sniffio").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    1.3.0

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Audited 2 packages in [TIME]
    ");

    // Unless the user requests a different version.
    uv_snapshot!(context.filters(), context.run().arg("--with").arg("sniffio<1.3.0").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    1.2.0

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Audited 2 packages in [TIME]
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + sniffio==1.2.0
    ");

    // If we request a dependency that isn't in the base environment, we should still respect any
    // other dependencies. In this case, `sniffio==1.3.0` is not the latest-compatible version, but
    // we should use it anyway.
    uv_snapshot!(context.filters(), context.run().arg("--with").arg("anyio").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    1.3.0

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Audited 2 packages in [TIME]
    Resolved 3 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + sniffio==1.3.0
    ");

    // Even if we run with` --no-sync`.
    uv_snapshot!(context.filters(), context.run().arg("--with").arg("anyio==4.2.0").arg("--no-sync").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    1.3.0

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.2.0
     + idna==3.6
     + sniffio==1.3.0
    ");

    // If the dependencies can't be resolved, we should reference `--with`.
    uv_snapshot!(context.filters(), context.run().arg("--with").arg("add").arg("main.py"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Audited 2 packages in [TIME]
      × No solution found when resolving `--with` dependencies:
      ╰─▶ Because there are no versions of add and you require add, we can conclude that your requirements are unsatisfiable.
    ");

    Ok(())
}

/// Test that an ephemeral environment writes the path of its parent environment to the `extends-environment` key
/// of its `pyvenv.cfg` file. This feature makes it easier for static-analysis tools like ty to resolve which import
/// search paths are available in these ephemeral environments.
#[test]
fn run_with_pyvenv_cfg_file() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_pyvenv_cfg_filters();

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.8"

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"
        "#
    })?;
    context
        .temp_dir
        .child("src")
        .child("foo")
        .child("__init__.py")
        .touch()?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        import os

        with open(f'{os.getenv("VIRTUAL_ENV")}/pyvenv.cfg') as f:
            print(f.read())
       "#
    })?;

    uv_snapshot!(context.filters(), context.run().arg("--with").arg("iniconfig").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    home = [PYTHON_HOME]
    implementation = CPython
    uv = [UV_VERSION]
    version_info = 3.12.[X]
    include-system-site-packages = false
    extends-environment = [PARENT_VENV]


    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/)
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    Ok(())
}

#[test]
fn run_with_overlay_interpreter() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.8"
        dependencies = ["anyio"]

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"

        [project.scripts]
        main = "foo:main"

        [project.gui-scripts]
        main_gui = "foo:main_gui"
        "#
    })?;

    let foo = context.temp_dir.child("src").child("foo");
    foo.create_dir_all()?;
    let init_py = foo.child("__init__.py");
    init_py.write_str(indoc! { r#"
        import sys
        import shutil
        from pathlib import Path

        def show_python():
            print(sys.executable)

        def copy_entrypoint():
            base = Path(sys.executable)
            shutil.copyfile(base.with_name("main").with_suffix(base.suffix), sys.argv[1])

        def copy_gui_entrypoint():
            base = Path(sys.executable)
            shutil.copyfile(base.with_name("main_gui").with_suffix(base.suffix), sys.argv[1])

        def main():
            show_python()
            if len(sys.argv) > 1:
                copy_entrypoint()

        def main_gui():
            show_python()
            if len(sys.argv) > 1:
                copy_gui_entrypoint()
       "#
    })?;

    // The project's entrypoint should be rewritten to use the overlay interpreter.
    uv_snapshot!(context.filters(), context.run().arg("--with").arg("iniconfig").arg("main").arg(context.temp_dir.child("main").as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [CACHE_DIR]/builds-v0/[TMP]/python

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.3.0
     + foo==1.0.0 (from file://[TEMP_DIR]/)
     + idna==3.6
     + sniffio==1.3.1
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    // The project's gui entrypoint should be rewritten to use the overlay interpreter.
    #[cfg(windows)]
    uv_snapshot!(context.filters(), context.run().arg("--with").arg("iniconfig").arg("main_gui").arg(context.temp_dir.child("main_gui").as_os_str()), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    [CACHE_DIR]/builds-v0/[TMP]/pythonw

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Audited 4 packages in [TIME]
    Resolved 1 package in [TIME]
    ");

    #[cfg(unix)]
    insta::with_settings!({
        filters => context.filters(),
    }, {
            assert_snapshot!(
                context.read("main"), @r#"
            #![CACHE_DIR]/builds-v0/[TMP]/python
            # -*- coding: utf-8 -*-
            import sys
            from foo import main
            if __name__ == "__main__":
                if sys.argv[0].endswith("-script.pyw"):
                    sys.argv[0] = sys.argv[0][:-11]
                elif sys.argv[0].endswith(".exe"):
                    sys.argv[0] = sys.argv[0][:-4]
                sys.exit(main())
            "#
            );
        }
    );

    // The package, its dependencies, and the overlay dependencies should be available.
    context
        .run()
        .arg("--with")
        .arg("iniconfig")
        .arg("python")
        .arg("-c")
        .arg("import foo; import anyio; import iniconfig")
        .assert()
        .success();

    // When layering the project on top (via `--with`), the overlay interpreter also should be used.
    uv_snapshot!(context.filters(), context.run().arg("--no-project").arg("--with").arg(".").arg("main"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [CACHE_DIR]/builds-v0/[TMP]/python

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.3.0
     + foo==1.0.0 (from file://[TEMP_DIR]/)
     + idna==3.6
     + sniffio==1.3.1
    ");

    // When layering the project on top (via `--with`), the overlay gui interpreter also should be used.
    #[cfg(windows)]
    uv_snapshot!(context.filters(), context.run().arg("--no-project").arg("--gui-script").arg("--with").arg(".").arg("main_gui"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    [CACHE_DIR]/builds-v0/[TMP]/pythonw

    ----- stderr -----
    Resolved 4 packages in [TIME]
    ");

    // Switch to a relocatable virtual environment.
    context
        .venv()
        .arg("--allow-existing")
        .arg("--relocatable")
        .assert()
        .success();

    // Cleanup previous shutil
    fs_err::remove_file(context.temp_dir.child("main"))?;
    #[cfg(windows)]
    fs_err::remove_file(context.temp_dir.child("main_gui"))?;

    // The project's entrypoint should be rewritten to use the overlay interpreter.
    uv_snapshot!(context.filters(), context.run().arg("--with").arg("iniconfig").arg("main").arg(context.temp_dir.child("main").as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [CACHE_DIR]/builds-v0/[TMP]/python

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Audited 4 packages in [TIME]
    Resolved 1 package in [TIME]
    ");

    // The project's gui entrypoint should be rewritten to use the overlay interpreter.
    #[cfg(windows)]
    uv_snapshot!(context.filters(), context.run().arg("--with").arg("iniconfig").arg("main_gui").arg(context.temp_dir.child("main_gui").as_os_str()), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    [CACHE_DIR]/builds-v0/[TMP]/pythonw

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Audited 4 packages in [TIME]
    Resolved 1 package in [TIME]
    ");

    // The package, its dependencies, and the overlay dependencies should be available.
    context
        .run()
        .arg("--with")
        .arg("iniconfig")
        .arg("python")
        .arg("-c")
        .arg("import foo; import anyio; import iniconfig")
        .assert()
        .success();

    #[cfg(unix)]
    insta::with_settings!({
        filters => context.filters(),
    }, {
            assert_snapshot!(
                context.read("main"), @r#"
            #![CACHE_DIR]/builds-v0/[TMP]/python
            # -*- coding: utf-8 -*-
            import sys
            from foo import main
            if __name__ == "__main__":
                if sys.argv[0].endswith("-script.pyw"):
                    sys.argv[0] = sys.argv[0][:-11]
                elif sys.argv[0].endswith(".exe"):
                    sys.argv[0] = sys.argv[0][:-4]
                sys.exit(main())
            "#
            );
        }
    );

    // When layering the project on top (via `--with`), the overlay interpreter also should be used.
    uv_snapshot!(context.filters(), context.run().arg("--no-project").arg("--with").arg(".").arg("main"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [CACHE_DIR]/builds-v0/[TMP]/python

    ----- stderr -----
    Resolved 4 packages in [TIME]
    ");

    // When layering the project on top (via `--with`), the overlay gui interpreter also should be used.
    #[cfg(windows)]
    uv_snapshot!(context.filters(), context.run().arg("--no-project").arg("--gui-script").arg("--with").arg(".").arg("main_gui"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    [CACHE_DIR]/builds-v0/[TMP]/pythonw

    ----- stderr -----
    Resolved 4 packages in [TIME]
    ");

    Ok(())
}

#[test]
fn run_with_build_constraints() -> Result<()> {
    let context = uv_test::test_context!("3.9");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.9"
        dependencies = ["anyio"]

        [tool.uv]
        build-constraint-dependencies = ["setuptools==1"]
        "#
    })?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r"
        import os
       "
    })?;

    // Installing requests with incompatible build constraints should fail.
    uv_snapshot!(context.filters(), context.run().arg("--with").arg("requests==1.2").arg("main.py"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 5 packages in [TIME]
    Installed 5 packages in [TIME]
     + anyio==4.3.0
     + exceptiongroup==1.2.0
     + idna==3.6
     + sniffio==1.3.1
     + typing-extensions==4.10.0
      × Failed to download and build `requests==1.2.0`
      ├─▶ Failed to resolve requirements from `setup.py` build
      ├─▶ No solution found when resolving: `setuptools>=40.8.0`
      ╰─▶ Because you require setuptools>=40.8.0 and setuptools==1, we can conclude that your requirements are unsatisfiable.
    ");

    // Change the build constraint to be compatible with `requests==1.2`.
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.9"
        dependencies = ["anyio"]

        [tool.uv]
        build-constraint-dependencies = ["setuptools>=42"]
        "#
    })?;

    uv_snapshot!(context.filters(), context.run().arg("--with").arg("requests==1.2").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Audited 5 packages in [TIME]
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + requests==1.2.0
    ");

    Ok(())
}

/// Sync all members in a workspace.
#[test]
fn run_in_workspace() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio>3"]

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"

        [tool.uv.workspace]
        members = ["child1", "child2"]

        [tool.uv.sources]
        child1 = { workspace = true }
        child2 = { workspace = true }
        "#,
    )?;
    context
        .temp_dir
        .child("src")
        .child("project")
        .child("__init__.py")
        .touch()?;

    let child1 = context.temp_dir.child("child1");
    child1.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "child1"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig>1"]

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
        "#,
    )?;
    child1
        .child("src")
        .child("child1")
        .child("__init__.py")
        .touch()?;

    let child2 = context.temp_dir.child("child2");
    child2.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "child2"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["typing-extensions>4"]

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
        "#,
    )?;
    child2
        .child("src")
        .child("child2")
        .child("__init__.py")
        .touch()?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r"
        import anyio
       "
    })?;

    uv_snapshot!(context.filters(), context.run().arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 8 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + project==0.1.0 (from file://[TEMP_DIR]/)
     + sniffio==1.3.1
    ");

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r"
        import iniconfig
       "
    })?;

    uv_snapshot!(context.filters(), context.run().arg("main.py"), @r#"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 8 packages in [TIME]
    Audited 4 packages in [TIME]
    Traceback (most recent call last):
      File "[TEMP_DIR]/main.py", line 1, in <module>
        import iniconfig
    ModuleNotFoundError: No module named 'iniconfig'
    "#);

    uv_snapshot!(context.filters(), context.run().arg("--package").arg("child1").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 8 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + child1==0.1.0 (from file://[TEMP_DIR]/child1)
     + iniconfig==2.0.0
    ");

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r"
        import typing_extensions
       "
    })?;

    uv_snapshot!(context.filters(), context.run().arg("main.py"), @r#"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 8 packages in [TIME]
    Audited 4 packages in [TIME]
    Traceback (most recent call last):
      File "[TEMP_DIR]/main.py", line 1, in <module>
        import typing_extensions
    ModuleNotFoundError: No module named 'typing_extensions'
    "#);

    uv_snapshot!(context.filters(), context.run().arg("--all-packages").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 8 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + child2==0.1.0 (from file://[TEMP_DIR]/child2)
     + typing-extensions==4.10.0
    ");

    Ok(())
}

#[test]
fn run_with_editable() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let anyio_local = context.temp_dir.child("src").child("anyio_local");
    copy_dir_all(
        context.workspace_root.join("test/packages/anyio_local"),
        &anyio_local,
    )?;

    let black_editable = context.temp_dir.child("src").child("black_editable");
    copy_dir_all(
        context.workspace_root.join("test/packages/black_editable"),
        &black_editable,
    )?;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.8"
        dependencies = ["anyio", "sniffio==1.3.1"]

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
        "#
    })?;

    context
        .temp_dir
        .child("src")
        .child("foo")
        .child("__init__.py")
        .touch()?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r"
        import sniffio
       "
    })?;

    // Requesting an editable requirement should install it in a layer.
    uv_snapshot!(context.filters(), context.run().arg("--with-editable").arg("./src/black_editable").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.3.0
     + foo==1.0.0 (from file://[TEMP_DIR]/)
     + idna==3.6
     + sniffio==1.3.1
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + black==0.1.0 (from file://[TEMP_DIR]/src/black_editable)
    ");

    // Requesting an editable requirement should install it in a layer, even if it satisfied
    uv_snapshot!(context.filters(), context.run().arg("--with-editable").arg("./src/anyio_local").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Audited 4 packages in [TIME]
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + anyio==4.3.0+foo (from file://[TEMP_DIR]/src/anyio_local)
    ");

    // Requesting the project itself should use the base environment.
    uv_snapshot!(context.filters(), context.run().arg("--with-editable").arg(".").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Audited 4 packages in [TIME]
    ");

    // Similarly, an already editable requirement does not require a layer
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.8"
        dependencies = ["anyio", "sniffio==1.3.1"]

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"

        [tool.uv.sources]
        anyio = { path = "./src/anyio_local", editable = true }
        "#
    })?;

    uv_snapshot!(context.filters(), context.sync(), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 3 packages in [TIME]
    Installed 2 packages in [TIME]
     - anyio==4.3.0
     + anyio==4.3.0+foo (from file://[TEMP_DIR]/src/anyio_local)
     ~ foo==1.0.0 (from file://[TEMP_DIR]/)
     - idna==3.6
    ");

    uv_snapshot!(context.filters(), context.run().arg("--with-editable").arg("./src/anyio_local").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Audited 3 packages in [TIME]
    ");

    // If invalid, we should reference `--with-editable`.
    uv_snapshot!(context.filters(), context.run().arg("--with-editable").arg("./foo").arg("main.py"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Audited 3 packages in [TIME]
      × Failed to resolve `--with` requirement
      ╰─▶ Distribution not found at: file://[TEMP_DIR]/foo
    ");

    Ok(())
}

#[test]
fn run_group() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["typing-extensions"]

        [dependency-groups]
        foo = ["anyio"]
        bar = ["iniconfig"]
        dev = ["sniffio"]
        "#,
    )?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        try:
            import anyio
            print("imported `anyio`")
        except ImportError:
            print("failed to import `anyio`")

        try:
            import iniconfig
            print("imported `iniconfig`")
        except ImportError:
            print("failed to import `iniconfig`")

        try:
            import typing_extensions
            print("imported `typing_extensions`")
        except ImportError:
            print("failed to import `typing_extensions`")
       "#
    })?;

    context.lock().assert().success();

    uv_snapshot!(context.filters(), context.run().arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    failed to import `anyio`
    failed to import `iniconfig`
    imported `typing_extensions`

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + sniffio==1.3.1
     + typing-extensions==4.10.0
    ");

    uv_snapshot!(context.filters(), context.run().arg("--only-group").arg("bar").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    failed to import `anyio`
    imported `iniconfig`
    imported `typing_extensions`

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    uv_snapshot!(context.filters(), context.run().arg("--group").arg("foo").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    imported `anyio`
    imported `iniconfig`
    imported `typing_extensions`

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
    ");

    uv_snapshot!(context.filters(), context.run().arg("--group").arg("foo").arg("--group").arg("bar").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    imported `anyio`
    imported `iniconfig`
    imported `typing_extensions`

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Audited 5 packages in [TIME]
    ");

    uv_snapshot!(context.filters(), context.run().arg("--all-groups").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    imported `anyio`
    imported `iniconfig`
    imported `typing_extensions`

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Audited 5 packages in [TIME]
    ");

    uv_snapshot!(context.filters(), context.run().arg("--all-groups").arg("--no-group").arg("bar").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    imported `anyio`
    imported `iniconfig`
    imported `typing_extensions`

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Audited 4 packages in [TIME]
    ");

    uv_snapshot!(context.filters(), context.run().arg("--group").arg("foo").arg("--no-project").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    imported `anyio`
    imported `iniconfig`
    imported `typing_extensions`

    ----- stderr -----
    warning: `--group foo` has no effect when used alongside `--no-project`
    ");

    uv_snapshot!(context.filters(), context.run().arg("--group").arg("foo").arg("--group").arg("bar").arg("--no-project").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    imported `anyio`
    imported `iniconfig`
    imported `typing_extensions`

    ----- stderr -----
    warning: `--group` has no effect when used alongside `--no-project`
    ");

    uv_snapshot!(context.filters(), context.run().arg("--group").arg("dev").arg("--no-project").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    imported `anyio`
    imported `iniconfig`
    imported `typing_extensions`

    ----- stderr -----
    warning: `--group dev` has no effect when used alongside `--no-project`
    ");

    uv_snapshot!(context.filters(), context.run().arg("--all-groups").arg("--no-project").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    imported `anyio`
    imported `iniconfig`
    imported `typing_extensions`

    ----- stderr -----
    warning: `--all-groups` has no effect when used alongside `--no-project`
    ");

    uv_snapshot!(context.filters(), context.run().arg("--dev").arg("--no-project").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    imported `anyio`
    imported `iniconfig`
    imported `typing_extensions`

    ----- stderr -----
    warning: `--dev` has no effect when used alongside `--no-project`
    ");

    Ok(())
}

#[test]
fn run_locked() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0"]

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"
        "#,
    )?;
    context
        .temp_dir
        .child("src")
        .child("project")
        .child("__init__.py")
        .touch()?;

    // Running with `--locked` should error, if no lockfile is present.
    uv_snapshot!(context.filters(), context.run().arg("--locked").arg("--").arg("python").arg("--version"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Unable to find lockfile at `uv.lock`, but `--locked` was provided. To create a lockfile, run `uv lock` or `uv sync` without the flag.
    ");

    // Lock the initial requirements.
    context.lock().assert().success();

    let existing = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            existing, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "anyio"
        version = "3.7.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "idna" },
            { name = "sniffio" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/c6/b3/fefbf7e78ab3b805dec67d698dc18dd505af7a18a8dd08868c9b4fa736b5/anyio-3.7.0.tar.gz", hash = "sha256:275d9973793619a5374e1c89a4f4ad3f4b0a5510a2b5b939444bee8f4c4d37ce", size = 142737, upload-time = "2023-05-27T11:12:46.688Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/68/fe/7ce1926952c8a403b35029e194555558514b365ad77d75125f521a2bec62/anyio-3.7.0-py3-none-any.whl", hash = "sha256:eddca883c4175f14df8aedce21054bfca3adb70ffe76a9f607aef9d7fa2ea7f0", size = 80873, upload-time = "2023-05-27T11:12:44.474Z" },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426, upload-time = "2023-11-25T15:40:54.902Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567, upload-time = "2023-11-25T15:40:52.604Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "anyio" },
        ]

        [package.metadata]
        requires-dist = [{ name = "anyio", specifier = "==3.7.0" }]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372, upload-time = "2024-02-25T23:20:04.057Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235, upload-time = "2024-02-25T23:20:01.196Z" },
        ]
        "#);
        }
    );

    // Update the requirements.
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"
        "#,
    )?;

    // Running with `--locked` should error.
    uv_snapshot!(context.filters(), context.run().arg("--locked").arg("--").arg("python").arg("--version"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided. To update the lockfile, run `uv lock`.
    ");

    let updated = context.read("uv.lock");

    // And the lockfile should be unchanged.
    assert_eq!(existing, updated);

    // Lock the updated requirements.
    uv_snapshot!(context.lock(), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Removed anyio v3.7.0
    Removed idna v3.6
    Added iniconfig v2.0.0
    Removed sniffio v1.3.1
    ");

    // Lock the updated requirements.
    uv_snapshot!(context.lock(), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    // Running with `--locked` should succeed.
    uv_snapshot!(context.filters(), context.run().arg("--locked").arg("--").arg("python").arg("--version"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.[X]

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + iniconfig==2.0.0
     + project==0.1.0 (from file://[TEMP_DIR]/)
    ");

    Ok(())
}

#[test]
fn run_frozen() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0"]

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"
        "#,
    )?;
    context
        .temp_dir
        .child("src")
        .child("project")
        .child("__init__.py")
        .touch()?;

    // Running with `--frozen` should error, if no lockfile is present.
    uv_snapshot!(context.filters(), context.run().arg("--frozen").arg("--").arg("python").arg("--version"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Unable to find lockfile at `uv.lock`, but `--frozen` was provided. To create a lockfile, run `uv lock` or `uv sync` without the flag.
    ");

    context.lock().assert().success();

    // Update the requirements.
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"
        "#,
    )?;

    // Running with `--frozen` should install the stale lockfile.
    uv_snapshot!(context.filters(), context.run().arg("--frozen").arg("--").arg("python").arg("--version"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.[X]

    ----- stderr -----
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==3.7.0
     + idna==3.6
     + project==0.1.0 (from file://[TEMP_DIR]/)
     + sniffio==1.3.1
    ");

    Ok(())
}

#[test]
fn run_no_sync() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0"]

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"
        "#,
    )?;
    context
        .temp_dir
        .child("src")
        .child("project")
        .child("__init__.py")
        .touch()?;

    // Running with `--no-sync` should succeed error, even if the lockfile isn't present.
    uv_snapshot!(context.filters(), context.run().arg("--no-sync").arg("--").arg("python").arg("--version"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.[X]

    ----- stderr -----
    ");

    context.lock().assert().success();

    // Running with `--no-sync` should not install any requirements.
    uv_snapshot!(context.filters(), context.run().arg("--no-sync").arg("--").arg("python").arg("--version"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.[X]

    ----- stderr -----
    ");

    context.sync().assert().success();

    // But it should have access to the installed packages.
    uv_snapshot!(context.filters(), context.run().arg("--no-sync").arg("--").arg("python").arg("-c").arg("import anyio; print(anyio.__name__)"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    anyio

    ----- stderr -----
    ");

    Ok(())
}

/// Test that `UV_NO_SYNC=1` environment variable works for `uv run`.
///
/// See: <https://github.com/astral-sh/uv/issues/17390>
#[test]
fn run_no_sync_env_var() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0"]

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"
        "#,
    )?;
    context
        .temp_dir
        .child("src")
        .child("project")
        .child("__init__.py")
        .touch()?;

    // Running with `UV_NO_SYNC=1` should succeed, even if the lockfile isn't present.
    uv_snapshot!(context.filters(), context.run().env(EnvVars::UV_NO_SYNC, "1").arg("--").arg("python").arg("--version"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.[X]

    ----- stderr -----
    ");

    context.lock().assert().success();

    // Running with `UV_NO_SYNC=1` should not install any requirements.
    uv_snapshot!(context.filters(), context.run().env(EnvVars::UV_NO_SYNC, "1").arg("--").arg("python").arg("--version"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.[X]

    ----- stderr -----
    ");

    context.sync().assert().success();

    // But it should have access to the installed packages.
    uv_snapshot!(context.filters(), context.run().env(EnvVars::UV_NO_SYNC, "1").arg("--").arg("python").arg("-c").arg("import anyio; print(anyio.__name__)"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    anyio

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn run_empty_requirements_txt() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.8"
        dependencies = ["anyio", "sniffio==1.3.1"]

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"
        "#
    })?;
    context
        .temp_dir
        .child("src")
        .child("foo")
        .child("__init__.py")
        .touch()?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r"
        import sniffio
       "
    })?;

    let requirements_txt =
        ChildPath::new(context.temp_dir.canonicalize()?.join("requirements.txt"));
    requirements_txt.touch()?;

    // The project environment is synced on the first invocation.
    uv_snapshot!(context.filters(), context.run().arg("--with-requirements").arg(requirements_txt.as_os_str()).arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.3.0
     + foo==1.0.0 (from file://[TEMP_DIR]/)
     + idna==3.6
     + sniffio==1.3.1
    warning: Requirements file `requirements.txt` does not contain any dependencies
    ");

    // Then reused in subsequent invocations
    uv_snapshot!(context.filters(), context.run().arg("--with-requirements").arg(requirements_txt.as_os_str()).arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Audited 4 packages in [TIME]
    warning: Requirements file `requirements.txt` does not contain any dependencies
    ");

    Ok(())
}

#[test]
fn run_requirements_txt() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.8"
        dependencies = ["anyio", "sniffio==1.3.1"]

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"
        "#
    })?;
    context
        .temp_dir
        .child("src")
        .child("foo")
        .child("__init__.py")
        .touch()?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r"
        import sniffio
       "
    })?;

    // Requesting an unsatisfied requirement should install it.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("iniconfig")?;

    uv_snapshot!(context.filters(), context.run().arg("--with-requirements").arg(requirements_txt.as_os_str()).arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.3.0
     + foo==1.0.0 (from file://[TEMP_DIR]/)
     + idna==3.6
     + sniffio==1.3.1
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    // Requesting a satisfied requirement should use the base environment.
    requirements_txt.write_str("sniffio")?;

    uv_snapshot!(context.filters(), context.run().arg("--with-requirements").arg(requirements_txt.as_os_str()).arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Audited 4 packages in [TIME]
    ");

    // Unless the user requests a different version.
    requirements_txt.write_str("sniffio<1.3.1")?;

    uv_snapshot!(context.filters(), context.run().arg("--with-requirements").arg(requirements_txt.as_os_str()).arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Audited 4 packages in [TIME]
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + sniffio==1.3.0
    ");

    // Or includes an unsatisfied requirement via `--with`.
    requirements_txt.write_str("sniffio")?;

    uv_snapshot!(context.filters(), context.run()
        .arg("--with-requirements")
        .arg(requirements_txt.as_os_str())
        .arg("--with")
        .arg("iniconfig")
        .arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Audited 4 packages in [TIME]
    Resolved 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + iniconfig==2.0.0
     + sniffio==1.3.1
    ");

    // Allow `-` for stdin.
    uv_snapshot!(context.filters(), context.run()
        .arg("--with-requirements")
        .arg("-")
        .arg("--with")
        .arg("iniconfig")
        .arg("main.py")
        .stdin(std::fs::File::open(&requirements_txt)?), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Audited 4 packages in [TIME]
    Resolved 2 packages in [TIME]
    ");

    // But not in combination with reading the script from stdin
    uv_snapshot!(context.filters(), context.run()
        .arg("--with-requirements")
        .arg("-")
        // The script to run
        .arg("-")
        .stdin(std::fs::File::open(&requirements_txt)?), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Cannot read both requirements file and script from stdin
    ");

    uv_snapshot!(context.filters(), context.run()
        .arg("--with-requirements")
        .arg("-")
        .arg("--script")
        .arg("-")
        .stdin(std::fs::File::open(&requirements_txt)?), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Cannot read both requirements file and script from stdin
    ");

    Ok(())
}

/// Ignore and warn when (e.g.) the `--index-url` argument is a provided `requirements.txt`.
#[test]
fn run_requirements_txt_arguments() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.8"
        dependencies = ["typing_extensions"]

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"
        "#
    })?;
    context
        .temp_dir
        .child("src")
        .child("foo")
        .child("__init__.py")
        .touch()?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r"
        import typing_extensions
       "
    })?;

    // Requesting an unsatisfied requirement should install it.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(indoc! { r"
        --index-url https://test.pypi.org/simple
        idna
        "
    })?;

    uv_snapshot!(context.filters(), context.run().arg("--with-requirements").arg(requirements_txt.as_os_str()).arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/)
     + typing-extensions==4.10.0
    warning: Ignoring `--index-url` from requirements file: `https://test.pypi.org/simple`. Instead, use the `--index-url` command-line argument, or set `index-url` in a `uv.toml` or `pyproject.toml` file.
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + idna==3.6
    ");

    Ok(())
}

/// Ensure that we can import from the root project when layering `--with` requirements.
#[test]
fn run_editable() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.8"
        dependencies = []

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"
        "#
    })?;

    let src = context.temp_dir.child("src").child("foo");
    src.create_dir_all()?;

    let init = src.child("__init__.py");
    init.touch()?;

    let main = context.temp_dir.child("main.py");
    main.write_str(indoc! { r"
        import foo
        print('Hello, world!')
       "
    })?;

    // We treat arguments before the command as uv arguments
    uv_snapshot!(context.filters(), context.run().arg("--with").arg("iniconfig").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello, world!

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/)
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    Ok(())
}

#[test]
fn run_from_directory() -> Result<()> {
    // Default to 3.11 so that the `.python-version` is meaningful.
    let context = uv_test::test_context_with_versions!(&["3.10", "3.11", "3.12"])
        .with_filtered_missing_file_error();

    let project_dir = context.temp_dir.child("project");
    project_dir
        .child(PYTHON_VERSION_FILENAME)
        .write_str("3.12")?;

    let pyproject_toml = project_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.10"
        dependencies = []

        [project.scripts]
        main = "main:main"

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#
    })?;

    let main_script = project_dir.child("main.py");
    main_script.write_str(indoc! { r"
        import platform

        def main():
            print(platform.python_version())
       "
    })?;

    let filters = TestContext::path_patterns(Path::new("project").join(".venv"))
        .into_iter()
        .map(|pattern| (pattern, "[PROJECT_VENV]/".to_string()))
        .collect::<Vec<_>>();
    let filters = context
        .filters()
        .into_iter()
        .chain(
            filters
                .iter()
                .map(|(pattern, replacement)| (pattern.as_str(), replacement.as_str())),
        )
        .collect::<Vec<_>>();

    // Use `--project`, which resolves configuration relative to the provided directory, but paths
    // relative to the current working directory.
    uv_snapshot!(filters.clone(), context.run().arg("--project").arg("project").arg("main"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    3.12.[X]

    ----- stderr -----
    warning: `VIRTUAL_ENV=.venv` does not match the project environment path `[PROJECT_VENV]/` and will be ignored; use `--active` to target the active environment instead
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: [PROJECT_VENV]/
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/project)
    ");

    fs_err::remove_dir_all(context.temp_dir.join("project").join(".venv"))?;
    uv_snapshot!(filters.clone(), context.run().arg("--project").arg("project").arg("./project/main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `VIRTUAL_ENV=.venv` does not match the project environment path `[PROJECT_VENV]/` and will be ignored; use `--active` to target the active environment instead
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: [PROJECT_VENV]/
    Resolved 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/project)
    ");

    // Use `--directory`, which switches to the provided directory entirely.
    fs_err::remove_dir_all(context.temp_dir.join("project").join(".venv"))?;
    uv_snapshot!(filters.clone(), context.run().arg("--directory").arg("project").arg("main"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    3.12.[X]

    ----- stderr -----
    warning: `VIRTUAL_ENV=[VENV]/` does not match the project environment path `.venv` and will be ignored; use `--active` to target the active environment instead
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: .venv
    Resolved 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/project)
    ");

    fs_err::remove_dir_all(context.temp_dir.join("project").join(".venv"))?;
    uv_snapshot!(filters.clone(), context.run().arg("--directory").arg("project").arg("./main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `VIRTUAL_ENV=[VENV]/` does not match the project environment path `.venv` and will be ignored; use `--active` to target the active environment instead
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: .venv
    Resolved 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/project)
    ");

    fs_err::remove_dir_all(context.temp_dir.join("project").join(".venv"))?;
    uv_snapshot!(filters.clone(), context.run().arg("--directory").arg("project").arg("./project/main.py"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: `VIRTUAL_ENV=[VENV]/` does not match the project environment path `.venv` and will be ignored; use `--active` to target the active environment instead
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: .venv
    Resolved 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/project)
    error: Failed to spawn: `./project/main.py`
      Caused by: [OS ERROR 2]
    ");

    // Even if we write a `.python-version` file in the current directory, we should prefer the
    // one in the project directory in both cases.
    context
        .temp_dir
        .child(PYTHON_VERSION_FILENAME)
        .write_str("3.11")?;

    project_dir
        .child(PYTHON_VERSION_FILENAME)
        .write_str("3.10")?;

    fs_err::remove_dir_all(context.temp_dir.join("project").join(".venv"))?;
    uv_snapshot!(filters.clone(), context.run().arg("--project").arg("project").arg("main"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    3.10.[X]

    ----- stderr -----
    warning: `VIRTUAL_ENV=.venv` does not match the project environment path `[PROJECT_VENV]/` and will be ignored; use `--active` to target the active environment instead
    Using CPython 3.10.[X] interpreter at: [PYTHON-3.10]
    Creating virtual environment at: [PROJECT_VENV]/
    Resolved 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/project)
    ");

    fs_err::remove_dir_all(context.temp_dir.join("project").join(".venv"))?;
    uv_snapshot!(filters.clone(), context.run().arg("--directory").arg("project").arg("main"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    3.10.[X]

    ----- stderr -----
    warning: `VIRTUAL_ENV=[VENV]/` does not match the project environment path `.venv` and will be ignored; use `--active` to target the active environment instead
    Using CPython 3.10.[X] interpreter at: [PYTHON-3.10]
    Creating virtual environment at: .venv
    Resolved 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/project)
    ");

    Ok(())
}

/// By default, omit resolver and installer output.
#[test]
fn run_without_output() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.8"
        dependencies = ["anyio", "sniffio==1.3.1"]

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"
        "#
    })?;
    context
        .temp_dir
        .child("src")
        .child("foo")
        .child("__init__.py")
        .touch()?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r"
        import sniffio
       "
    })?;

    // On the first run, we only show the summary line for each environment.
    uv_snapshot!(context.filters(), context.run().env_remove(EnvVars::UV_SHOW_RESOLUTION).arg("--with").arg("iniconfig").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed 4 packages in [TIME]
    Installed 1 package in [TIME]
    ");

    // Subsequent runs are quiet.
    uv_snapshot!(context.filters(), context.run().env_remove(EnvVars::UV_SHOW_RESOLUTION).arg("--with").arg("iniconfig").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    ");

    Ok(())
}

/// Ensure that we can import from the root project when layering `--with` requirements.
#[test]
fn run_isolated_python_version() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&["3.9", "3.12"]);

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.8"
        dependencies = ["anyio"]

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
        "#
    })?;

    let src = context.temp_dir.child("src").child("foo");
    src.create_dir_all()?;

    let init = src.child("__init__.py");
    init.touch()?;

    let main = context.temp_dir.child("main.py");
    main.write_str(indoc! { r"
        import sys

        print((sys.version_info.major, sys.version_info.minor))
       "
    })?;

    uv_snapshot!(context.filters(), context.run().arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    (3, 9)

    ----- stderr -----
    Using CPython 3.9.[X] interpreter at: [PYTHON-3.9]
    Creating virtual environment at: .venv
    Resolved 6 packages in [TIME]
    Prepared 6 packages in [TIME]
    Installed 6 packages in [TIME]
     + anyio==4.3.0
     + exceptiongroup==1.2.0
     + foo==1.0.0 (from file://[TEMP_DIR]/)
     + idna==3.6
     + sniffio==1.3.1
     + typing-extensions==4.10.0
    ");

    uv_snapshot!(context.filters(), context.run().arg("--isolated").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    (3, 9)

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Installed 6 packages in [TIME]
     + anyio==4.3.0
     + exceptiongroup==1.2.0
     + foo==1.0.0 (from file://[TEMP_DIR]/)
     + idna==3.6
     + sniffio==1.3.1
     + typing-extensions==4.10.0
    ");

    // Set the `.python-version` to `3.12`.
    context
        .temp_dir
        .child(PYTHON_VERSION_FILENAME)
        .write_str("3.12")?;

    uv_snapshot!(context.filters(), context.run().arg("--isolated").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    (3, 12)

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.3.0
     + foo==1.0.0 (from file://[TEMP_DIR]/)
     + idna==3.6
     + sniffio==1.3.1
    ");

    Ok(())
}

/// Ignore the existing project when executing with `--no-project`.
#[test]
fn run_no_project() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_python_names()
        .with_filtered_virtualenv_bin()
        .with_filtered_exe_suffix();

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.8"
        dependencies = ["anyio"]

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
        "#
    })?;

    let src = context.temp_dir.child("src").child("foo");
    src.create_dir_all()?;

    let init = src.child("__init__.py");
    init.touch()?;

    // `run` should run in the context of the project.
    uv_snapshot!(context.filters(), context.run().arg("python").arg("-c").arg("import sys; print(sys.executable)"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [VENV]/[BIN]/[PYTHON]

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.3.0
     + foo==1.0.0 (from file://[TEMP_DIR]/)
     + idna==3.6
     + sniffio==1.3.1
    ");

    // `run --no-project` should not (but it should still run in the same environment, as it would
    // if there were no project at all).
    uv_snapshot!(context.filters(), context.run().arg("--no-project").arg("python").arg("-c").arg("import sys; print(sys.executable)"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [VENV]/[BIN]/[PYTHON]

    ----- stderr -----
    ");

    // `run --no-project --isolated` should run in an entirely isolated environment.
    uv_snapshot!(context.filters(), context.run().arg("--no-project").arg("--isolated").arg("python").arg("-c").arg("import sys; print(sys.executable)"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [CACHE_DIR]/builds-v0/[TMP]/[PYTHON]

    ----- stderr -----
    ");

    // `run --no-project` should not (but it should still run in the same environment, as it would
    // if there were no project at all).
    uv_snapshot!(context.filters(), context.run().arg("--no-project").arg("python").arg("-c").arg("import sys; print(sys.executable)"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [VENV]/[BIN]/[PYTHON]

    ----- stderr -----
    ");

    // `run --no-project --locked` should fail.
    uv_snapshot!(context.filters(), context.run().arg("--no-project").arg("--locked").arg("python").arg("-c").arg("import sys; print(sys.executable)"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [VENV]/[BIN]/[PYTHON]

    ----- stderr -----
    warning: `--locked` has no effect when used alongside `--no-project`
    ");

    Ok(())
}

#[test]
fn run_stdin() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        print("Hello, world!")
       "#
    })?;

    let mut command = context.run();
    let command_with_args = command.stdin(std::fs::File::open(test_script)?).arg("-");
    uv_snapshot!(context.filters(), command_with_args, @"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello, world!

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn run_package() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let main_script = context.temp_dir.child("__main__.py");
    main_script.write_str(indoc! { r#"
        print("Hello, world!")
       "#
    })?;

    uv_snapshot!(context.filters(), context.run().arg("."), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello, world!

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn run_zipapp() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    // Create a zipapp.
    let child = context.temp_dir.child("app");
    child.create_dir_all()?;

    let main_script = child.child("__main__.py");
    main_script.write_str(indoc! { r#"
        print("Hello, world!")
       "#
    })?;

    let zipapp = context.temp_dir.child("app.pyz");
    let status = context
        .run()
        .arg("python")
        .arg("-m")
        .arg("zipapp")
        .arg(child.as_ref())
        .arg("--output")
        .arg(zipapp.as_ref())
        .status()?;
    assert!(status.success());

    // Run the zipapp.
    uv_snapshot!(context.filters(), context.run().arg(zipapp.as_ref()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello, world!

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn run_stdin_args() {
    let context = uv_test::test_context!("3.12");

    uv_snapshot!(context.filters(), context.run().arg("python").arg("-c").arg("import sys; print(sys.argv)").arg("foo").arg("bar"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    ['-c', 'foo', 'bar']

    ----- stderr -----
    ");
}

/// Run a module equivalent to `python -m foo`.
#[test]
fn run_module() {
    let context = uv_test::test_context!("3.12");

    uv_snapshot!(context.filters(), context.run().arg("-m").arg("__hello__"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello world!

    ----- stderr -----
    ");

    uv_snapshot!(context.filters(), context.run().arg("-m").arg("http.server").arg("-h"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    usage: server.py [-h] [--cgi] [-b ADDRESS] [-d DIRECTORY] [-p VERSION] [port]

    positional arguments:
      port                  bind to this port (default: 8000)

    options:
      -h, --help            show this help message and exit
      --cgi                 run as CGI server
      -b ADDRESS, --bind ADDRESS
                            bind to this address (default: all interfaces)
      -d DIRECTORY, --directory DIRECTORY
                            serve this directory (default: current directory)
      -p VERSION, --protocol VERSION
                            conform to this HTTP version (default: HTTP/1.0)

    ----- stderr -----
    ");
}

#[test]
fn run_module_stdin() {
    let context = uv_test::test_context!("3.12");

    uv_snapshot!(context.filters(), context.run().arg("-m").arg("-"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Cannot run a Python module from stdin
    ");
}

/// Test for how run reacts to a pyproject.toml without a `[project]`
#[test]
fn virtual_empty() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_python_names()
        .with_filtered_virtualenv_bin()
        .with_filtered_exe_suffix();

    // Create an empty project
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.touch()?;

    let src = context.temp_dir.child("src").child("foo");
    src.create_dir_all()?;

    let init = src.child("__init__.py");
    init.touch()?;

    // `run` should work fine
    uv_snapshot!(context.filters(), context.run().arg("python").arg("-c").arg("import sys; print(sys.executable)"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [VENV]/[BIN]/[PYTHON]

    ----- stderr -----
    warning: No `requires-python` value found in the workspace. Defaulting to `>=3.12`.
    Resolved in [TIME]
    Audited in [TIME]
    ");

    // `run --no-project` should also work fine
    uv_snapshot!(context.filters(), context.run().arg("--no-project").arg("python").arg("-c").arg("import sys; print(sys.executable)"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [VENV]/[BIN]/[PYTHON]

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn run_isolated_incompatible_python() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&["3.9", "3.11"]);

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
        "#
    })?;

    let python_version = context.temp_dir.child(PYTHON_VERSION_FILENAME);
    python_version.write_str("3.9")?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        import iniconfig

        x: str | int = "hello"
        print(x)
       "#
    })?;

    // We should reject Python 3.9...
    uv_snapshot!(context.filters(), context.run().arg("main.py"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.9.[X] interpreter at: [PYTHON-3.9]
    error: The Python request from `.python-version` resolved to Python 3.9.[X], which is incompatible with the project's Python requirement: `>=3.12` (from `project.requires-python`)
    Use `uv python pin` to update the `.python-version` file to a compatible version
    ");

    // ...even if `--isolated` is provided.
    uv_snapshot!(context.filters(), context.run().arg("--isolated").arg("main.py"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: The Python request from `.python-version` resolved to Python 3.9.[X], which is incompatible with the project's Python requirement: `>=3.12` (from `project.requires-python`)
    Use `uv python pin` to update the `.python-version` file to a compatible version
    ");

    Ok(())
}

#[test]
fn run_isolated_does_not_modify_lock() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = [
            "anyio>=3,<5",
        ]

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"
        "#
    })?;
    context
        .temp_dir
        .child("src")
        .child("foo")
        .child("__init__.py")
        .touch()?;
    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        import importlib.metadata
        print(importlib.metadata.version("anyio"))
       "#
    })?;

    // Run with --isolated
    uv_snapshot!(context.filters(), context.run()
        .arg("--isolated")
        .arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    4.3.0

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.3.0
     + foo==1.0.0 (from file://[TEMP_DIR]/)
     + idna==3.6
     + sniffio==1.3.1
    ");

    // This should not create a lock file
    context
        .temp_dir
        .child("uv.lock")
        .assert(predicate::path::missing());

    // Create initial lock with default resolution
    uv_snapshot!(context.filters(), context.run().arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    4.3.0

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.3.0
     + foo==1.0.0 (from file://[TEMP_DIR]/)
     + idna==3.6
     + sniffio==1.3.1
    ");

    // Read the lock file content
    let pre_uv_lock = context.read("uv.lock");

    // Run with --isolated and --resolution lowest-direct to force different resolution
    // This should use anyio 3.x but not modify the lock file
    uv_snapshot!(context.filters(), context.run()
        .arg("--isolated")
        .arg("--resolution")
        .arg("lowest-direct")
        .arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    3.0.0

    ----- stderr -----
    Ignoring existing lockfile due to change in resolution mode: `highest` vs. `lowest-direct`
    Resolved 4 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 4 packages in [TIME]
     + anyio==3.0.0
     + foo==1.0.0 (from file://[TEMP_DIR]/)
     + idna==3.6
     + sniffio==1.3.1
    ");

    // Verify the lock file hasn't changed
    let post_uv_lock = context.read("uv.lock");
    assert_eq!(
        pre_uv_lock, post_uv_lock,
        "Lock file should not be modified with --isolated"
    );

    Ok(())
}

#[test]
fn run_isolated_with_frozen() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = [
            "anyio>=3,<5",
        ]

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"
        "#
    })?;
    context
        .temp_dir
        .child("src")
        .child("foo")
        .child("__init__.py")
        .touch()?;
    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        import importlib.metadata
        print(importlib.metadata.version("anyio"))
       "#
    })?;

    // Create an initial lockfile with lowest-direct resolution
    uv_snapshot!(context.filters(), context.run()
        .arg("--resolution")
        .arg("lowest-direct")
        .arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    3.0.0

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==3.0.0
     + foo==1.0.0 (from file://[TEMP_DIR]/)
     + idna==3.6
     + sniffio==1.3.1
    ");

    // Run with `--isolated` and `--frozen` to use the existing lock
    // We should not re-resolve to the highest version here
    uv_snapshot!(context.filters(), context.run()
        .arg("--isolated")
        .arg("--frozen")
        .arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    3.0.0

    ----- stderr -----
    Installed 4 packages in [TIME]
     + anyio==3.0.0
     + foo==1.0.0 (from file://[TEMP_DIR]/)
     + idna==3.6
     + sniffio==1.3.1
    ");

    Ok(())
}

#[test]
fn run_compiled_python_file() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    // Write a non-PEP 723 script.
    let test_non_script = context.temp_dir.child("main.py");
    test_non_script.write_str(indoc! { r#"
        print("Hello, world!")
       "#
    })?;

    // Run a non-PEP 723 script.
    uv_snapshot!(context.filters(), context.run().arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello, world!

    ----- stderr -----
    ");

    let compile_output = context
        .run()
        .arg("python")
        .arg("-m")
        .arg("compileall")
        .arg(test_non_script.path())
        .output()?;

    assert!(
        compile_output.status.success(),
        "Failed to compile the python script"
    );

    // Run the compiled non-PEP 723 script.
    let compiled_non_script = context.temp_dir.child("__pycache__/main.cpython-312.pyc");
    uv_snapshot!(context.filters(), context.run().arg(compiled_non_script.path()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello, world!

    ----- stderr -----
    ");

    // If the script contains a PEP 723 tag, we should install its requirements.
    let test_script = context.temp_dir.child("script.py");
    test_script.write_str(indoc! { r#"
        # /// script
        # requires-python = ">=3.11"
        # dependencies = [
        #   "iniconfig",
        # ]
        # ///
        import iniconfig
       "#
    })?;

    uv_snapshot!(context.filters(), context.run().arg("script.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    // Compile the PEP 723 script.
    let compile_output = context
        .run()
        .arg("python")
        .arg("-m")
        .arg("compileall")
        .arg(test_script.path())
        .output()?;

    assert!(
        compile_output.status.success(),
        "Failed to compile the python script"
    );

    // Run the compiled PEP 723 script. This fails, since we can't read the script tag.
    let compiled_script = context.temp_dir.child("__pycache__/script.cpython-312.pyc");
    uv_snapshot!(context.filters(), context.run().arg(compiled_script.path()), @r#"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Traceback (most recent call last):
      File "[TEMP_DIR]/script.py", line 7, in <module>
        import iniconfig
    ModuleNotFoundError: No module named 'iniconfig'
    "#);

    Ok(())
}

#[test]
fn run_exit_code() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let test_script = context.temp_dir.child("script.py");
    test_script.write_str(indoc! { r#"
        # /// script
        # requires-python = ">=3.11"
        # ///

        exit(42)
       "#
    })?;

    context.run().arg("script.py").assert().code(42);

    Ok(())
}

#[test]
fn run_invalid_project_table() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&["3.12"]);

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project.urls]
        repository = 'https://github.com/octocat/octocat-python'

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"
        "#
    })?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        print("Hello, world!")
       "#
    })?;

    uv_snapshot!(context.filters(), context.run().arg("main.py"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to parse: `pyproject.toml`
      Caused by: TOML parse error at line 1, column 2
      |
    1 | [project.urls]
      |  ^^^^^^^
    `pyproject.toml` is using the `[project]` table, but the required `project.name` field is not set
    ");

    Ok(())
}

#[test]
#[cfg(target_family = "unix")]
fn run_script_without_build_system() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [project.scripts]
        entry = "foo:custom_entry"
        "#
    })?;

    let test_script = context.temp_dir.child("src/__init__.py");
    test_script.write_str(indoc! { r#"
        def custom_entry():
            print!("Hello")
       "#
    })?;

    // TODO(lucab): this should match `entry` and warn
    // <https://github.com/astral-sh/uv/issues/7428>
    uv_snapshot!(context.filters(), context.run().arg("entry"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Audited in [TIME]
    error: Failed to spawn: `entry`
      Caused by: No such file or directory (os error 2)
    ");

    Ok(())
}

#[test]
fn run_script_module_conflict() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [project.scripts]
        foo = "foo:app"

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
        "#
    })?;

    let init = context.temp_dir.child("src/foo/__init__.py");
    init.write_str(indoc! { r#"
        def app():
            print("Hello from `__init__`")
       "#
    })?;

    uv_snapshot!(context.filters(), context.run().arg("foo"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello from `__init__`

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==0.1.0 (from file://[TEMP_DIR]/)
    ");

    // Creating `__main__` should not change the behavior, the entrypoint should take precedence
    let main = context.temp_dir.child("src/foo/__main__.py");
    main.write_str(indoc! { r#"
        print("Hello from `__main__`")
       "#
    })?;

    uv_snapshot!(context.filters(), context.run().arg("foo"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello from `__init__`

    ----- stderr -----
    Resolved 1 package in [TIME]
    Audited 1 package in [TIME]
    ");

    // Even if the working directory is `src`
    uv_snapshot!(context.filters(), context.run().arg("--directory").arg("src").arg("foo"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello from `__init__`

    ----- stderr -----
    Resolved 1 package in [TIME]
    Audited 1 package in [TIME]
    ");

    // Unless the user opts-in to module running with `-m`
    uv_snapshot!(context.filters(), context.run().arg("-m").arg("foo"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello from `__main__`

    ----- stderr -----
    Resolved 1 package in [TIME]
    Audited 1 package in [TIME]
    ");

    Ok(())
}

#[test]
fn run_script_explicit() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let test_script = context.temp_dir.child("script");
    test_script.write_str(indoc! { r#"
        # /// script
        # requires-python = ">=3.11"
        # dependencies = [
        #   "iniconfig",
        # ]
        # ///
        import iniconfig
        print("Hello, world!")
       "#
    })?;

    uv_snapshot!(context.filters(), context.run().arg("--script").arg("script"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello, world!

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    Ok(())
}

#[test]
fn run_script_explicit_stdin() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let test_script = context.temp_dir.child("script");
    test_script.write_str(indoc! { r#"
        # /// script
        # requires-python = ">=3.11"
        # dependencies = [
        #   "iniconfig",
        # ]
        # ///
        import iniconfig
        print("Hello, world!")
       "#
    })?;

    uv_snapshot!(context.filters(), context.run().arg("--script").arg("-").stdin(std::fs::File::open(test_script)?), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello, world!

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    Ok(())
}

#[test]
fn run_script_explicit_no_file() {
    let context = uv_test::test_context!("3.12");
    context
        .run()
        .arg("--script")
        .arg("script")
        .assert()
        .stderr(contains("can't open file"))
        .stderr(contains("[Errno 2] No such file or directory"));
}

#[cfg(target_family = "unix")]
#[test]
fn run_script_explicit_directory() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    fs_err::create_dir(context.temp_dir.child("script"))?;

    uv_snapshot!(context.filters(), context.run().arg("--script").arg("script"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: failed to read from file `script`: Is a directory (os error 21)
    ");

    Ok(())
}

#[test]
#[cfg(windows)]
fn run_gui_script_explicit_windows() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let test_script = context.temp_dir.child("script");
    test_script.write_str(indoc! { r#"
        # /// script
        # requires-python = ">=3.11"
        # dependencies = []
        # ///
        import sys
        import os

        executable = os.path.basename(sys.executable).lower()
        if not executable.startswith("pythonw"):
            print(f"Error: Expected pythonw.exe but got: {executable}", file=sys.stderr)
            sys.exit(1)

        print(f"Using executable: {executable}", file=sys.stderr)
    "#})?;

    uv_snapshot!(context.filters(), context.run().arg("--gui-script").arg("script"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using executable: pythonw.exe
    "###);

    Ok(())
}

#[test]
#[cfg(windows)]
fn run_gui_script_explicit_stdin_windows() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let test_script = context.temp_dir.child("script");
    test_script.write_str(indoc! { r#"
        # /// script
        # requires-python = ">=3.11"
        # dependencies = [
        #   "iniconfig",
        # ]
        # ///
        import iniconfig
        print("Hello, world!")
       "#
    })?;

    uv_snapshot!(context.filters(), context.run().arg("--gui-script").arg("-").stdin(std::fs::File::open(test_script)?), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello, world!

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    "###);

    Ok(())
}

#[test]
#[cfg(not(windows))]
fn run_gui_script_explicit_unix() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let test_script = context.temp_dir.child("script");
    test_script.write_str(indoc! { r#"
        # /// script
        # requires-python = ">=3.11"
        # dependencies = []
        # ///
        import sys
        import os

        executable = os.path.basename(sys.executable).lower()
        print(f"Using executable: {executable}", file=sys.stderr)
    "#})?;

    uv_snapshot!(context.filters(), context.run().arg("--gui-script").arg("script"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using executable: python
    ");

    Ok(())
}

#[test]
#[cfg(unix)]
fn run_linked_environment_path() -> Result<()> {
    use anyhow::Ok;

    let context = uv_test::test_context!("3.12")
        .with_filtered_virtualenv_bin()
        .with_filtered_python_names();

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["black"]
        "#,
    )?;

    // Create a link from `target` -> virtual environment
    fs_err::os::unix::fs::symlink(&context.venv, context.temp_dir.child("target"))?;

    // Running `uv sync` should use the environment at `target``
    uv_snapshot!(context.filters(), context.sync()
        .env(EnvVars::UV_PROJECT_ENVIRONMENT, "target"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 8 packages in [TIME]
    Prepared 6 packages in [TIME]
    Installed 6 packages in [TIME]
     + black==24.3.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    ");

    // `sys.prefix` and `sys.executable` should be from the `target` directory
    uv_snapshot!(context.filters(), context.run()
        .env_remove(EnvVars::VIRTUAL_ENV)  // Ignore the test context's active virtual environment
        .env(EnvVars::UV_PROJECT_ENVIRONMENT, "target")
        .arg("python").arg("-c").arg("import sys; print(sys.prefix); print(sys.executable)"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/target
    [TEMP_DIR]/target/[BIN]/[PYTHON]

    ----- stderr -----
    Resolved 8 packages in [TIME]
    Audited 6 packages in [TIME]
    ");

    // And, similarly, the entrypoint should use `target`
    let black_entrypoint = context.read("target/bin/black");
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            black_entrypoint, @r#"
        #![TEMP_DIR]/target/[BIN]/[PYTHON]
        # -*- coding: utf-8 -*-
        import sys
        from black import patched_main
        if __name__ == "__main__":
            if sys.argv[0].endswith("-script.pyw"):
                sys.argv[0] = sys.argv[0][:-11]
            elif sys.argv[0].endswith(".exe"):
                sys.argv[0] = sys.argv[0][:-4]
            sys.exit(patched_main())
        "#
        );
    });

    Ok(())
}

#[test]
fn run_active_project_environment() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&["3.11", "3.12"])
        .with_filtered_virtualenv_bin()
        .with_filtered_python_names();

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

    // Running `uv run` with `VIRTUAL_ENV` should warn
    uv_snapshot!(context.filters(), context.run()
        .arg("python").arg("--version")
        .env(EnvVars::VIRTUAL_ENV, "foo"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.11.[X]

    ----- stderr -----
    warning: `VIRTUAL_ENV=foo` does not match the project environment path `.venv` and will be ignored; use `--active` to target the active environment instead
    Using CPython 3.11.[X] interpreter at: [PYTHON-3.11]
    Creating virtual environment at: .venv
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    // Using `--no-active` should silence the warning
    uv_snapshot!(context.filters(), context.run()
        .arg("--no-active")
        .arg("python").arg("--version")
        .env(EnvVars::VIRTUAL_ENV, "foo"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.11.[X]

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Audited 1 package in [TIME]
    ");

    context
        .temp_dir
        .child(".venv")
        .assert(predicate::path::is_dir());

    context
        .temp_dir
        .child("foo")
        .assert(predicate::path::missing());

    // Using `--active` should create the environment
    uv_snapshot!(context.filters(), context.run()
        .arg("--active")
        .arg("python").arg("--version")
        .env(EnvVars::VIRTUAL_ENV, "foo"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.11.[X]

    ----- stderr -----
    Using CPython 3.11.[X] interpreter at: [PYTHON-3.11]
    Creating virtual environment at: foo
    Resolved 2 packages in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    context
        .temp_dir
        .child("foo")
        .assert(predicate::path::is_dir());

    // Requesting a different Python version should invalidate the environment
    uv_snapshot!(context.filters(), context.run()
        .arg("--active")
        .arg("-p").arg("3.12")
        .arg("python").arg("--version")
        .env(EnvVars::VIRTUAL_ENV, "foo"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.[X]

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Removed virtual environment at: foo
    Creating virtual environment at: foo
    Resolved 2 packages in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    Ok(())
}

#[test]
fn run_active_script_environment() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&["3.11", "3.12"])
        .with_filtered_virtualenv_bin()
        .with_filtered_python_names();

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        # /// script
        # requires-python = ">=3.11"
        # dependencies = [
        #   "iniconfig",
        # ]
        # ///

        import iniconfig

        print("Hello, world!")
       "#
    })?;

    // Running `uv run --script` with `VIRTUAL_ENV` should _not_ warn.
    uv_snapshot!(context.filters(), context.run()
        .arg("--script")
        .arg("main.py")
        .env(EnvVars::VIRTUAL_ENV, "foo"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello, world!

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    // Using `--no-active` should also _not_ warn.
    uv_snapshot!(context.filters(), context.run()
        .arg("--no-active")
        .arg("--script")
        .arg("main.py")
        .env(EnvVars::VIRTUAL_ENV, "foo"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello, world!

    ----- stderr -----
    ");

    context
        .temp_dir
        .child("foo")
        .assert(predicate::path::missing());

    // Using `--active` should create the environment
    uv_snapshot!(context.filters(), context.run()
        .arg("--active")
        .arg("--script")
        .arg("main.py")
        .env(EnvVars::VIRTUAL_ENV, "foo"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello, world!

    ----- stderr -----
    Resolved 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    context
        .temp_dir
        .child("foo")
        .assert(predicate::path::is_dir());

    // Requesting a different Python version should invalidate the environment
    uv_snapshot!(context.filters(), context.run()
        .arg("--active")
        .arg("-p").arg("3.12")
        .arg("--script")
        .arg("main.py")
        .env(EnvVars::VIRTUAL_ENV, "foo"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello, world!

    ----- stderr -----
    Resolved 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    Ok(())
}

#[test]
#[cfg(not(windows))]
fn run_gui_script_explicit_stdin_unix() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let test_script = context.temp_dir.child("script");
    test_script.write_str(indoc! { r#"
        # /// script
        # requires-python = ">=3.11"
        # dependencies = [
        #   "iniconfig",
        # ]
        # ///
        import iniconfig
        print("Hello, world!")
       "#
    })?;

    uv_snapshot!(context.filters(), context.run().arg("--gui-script").arg("-").stdin(std::fs::File::open(test_script)?), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello, world!

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    Ok(())
}

#[test]
fn run_remote_pep723_script() {
    let context = uv_test::test_context!("3.12").with_filtered_python_names();
    let context = context.with_filter((
        r"(?m)^Downloaded remote script to:.*\.py$",
        "Downloaded remote script to: [TEMP_PATH].py",
    ));
    uv_snapshot!(context.filters(), context.run().arg("https://raw.githubusercontent.com/astral-sh/uv/df45b9ac2584824309ff29a6a09421055ad730f6/scripts/uv-run-remote-script-test.py").arg(EnvVars::CI), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello CI, from uv!

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + markdown-it-py==3.0.0
     + mdurl==0.1.2
     + pygments==2.17.2
     + rich==13.7.1
    ");
}

#[cfg(unix)] // A URL could be a valid filepath on Unix but not on Windows
#[test]
fn run_url_like_with_local_file_priority() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let url = "https://example.com/path/to/main.py";
    let local_path: std::path::PathBuf = ["https:", "", "example.com", "path", "to", "main.py"]
        .iter()
        .collect();

    // replace with URL-like filepath
    let test_script = context.temp_dir.child(local_path);
    test_script.write_str(indoc! { r#"
        print("Hello, world!")
       "#
    })?;

    uv_snapshot!(context.filters(), context.run().arg(url), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello, world!

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn run_stdin_with_pep723() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        # /// script
        # requires-python = ">=3.11"
        # dependencies = [
        #   "iniconfig",
        # ]
        # ///
        import iniconfig
        print("Hello, world!")
       "#
    })?;

    uv_snapshot!(context.filters(), context.run().stdin(std::fs::File::open(test_script)?).arg("-"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello, world!

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    Ok(())
}

#[test]
fn run_with_env() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    context.temp_dir.child("test.py").write_str(indoc! { "
        import os
        print(os.environ.get('THE_EMPIRE_VARIABLE'))
        print(os.environ.get('REBEL_1'))
        print(os.environ.get('REBEL_2'))
        print(os.environ.get('REBEL_3'))
       "
    })?;

    context.temp_dir.child(".env").write_str(indoc! { "
        THE_EMPIRE_VARIABLE=palpatine
        REBEL_1=leia_organa
        REBEL_2=obi_wan_kenobi
        REBEL_3=C3PO
       "
    })?;

    uv_snapshot!(context.filters(), context.run().arg("test.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    None
    None
    None
    None

    ----- stderr -----
    ");

    uv_snapshot!(context.filters(), context.run().arg("--env-file").arg(".env").arg("test.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    palpatine
    leia_organa
    obi_wan_kenobi
    C3PO

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn run_with_env_file() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    context.temp_dir.child("test.py").write_str(indoc! { "
        import os
        print(os.environ.get('THE_EMPIRE_VARIABLE'))
        print(os.environ.get('REBEL_1'))
        print(os.environ.get('REBEL_2'))
        print(os.environ.get('REBEL_3'))
       "
    })?;

    context.temp_dir.child(".file").write_str(indoc! { "
        THE_EMPIRE_VARIABLE=palpatine
        REBEL_1=leia_organa
        REBEL_2=obi_wan_kenobi
        REBEL_3=C3PO
       "
    })?;

    uv_snapshot!(context.filters(), context.run().arg("--env-file").arg(".file").arg("test.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    palpatine
    leia_organa
    obi_wan_kenobi
    C3PO

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn run_with_multiple_env_files() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    context.temp_dir.child("test.py").write_str(indoc! { "
        import os
        print(os.environ.get('THE_EMPIRE_VARIABLE'))
        print(os.environ.get('REBEL_1'))
        print(os.environ.get('REBEL_2'))
       "
    })?;

    context.temp_dir.child(".env1").write_str(indoc! { "
        THE_EMPIRE_VARIABLE=palpatine
        REBEL_1=leia_organa
       "
    })?;

    context.temp_dir.child(".env2").write_str(indoc! { "
        THE_EMPIRE_VARIABLE=palpatine
        REBEL_1=obi_wan_kenobi
        REBEL_2=C3PO
       "
    })?;

    uv_snapshot!(context.filters(), context.run().arg("--env-file").arg(".env1").arg("--env-file").arg(".env2").arg("test.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    palpatine
    obi_wan_kenobi
    C3PO

    ----- stderr -----
    ");

    uv_snapshot!(context.filters(), context.run().arg("test.py").env(EnvVars::UV_ENV_FILE, ".env1 .env2"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    palpatine
    obi_wan_kenobi
    C3PO

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn run_with_env_omitted() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    context.temp_dir.child("test.py").write_str(indoc! { "
        import os
        print(os.environ.get('THE_EMPIRE_VARIABLE'))
       "
    })?;

    context.temp_dir.child(".env").write_str(indoc! { "
        THE_EMPIRE_VARIABLE=palpatine
       "
    })?;

    uv_snapshot!(context.filters(), context.run().arg("--env-file").arg(".env").arg("--no-env-file").arg("test.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    None

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn run_with_malformed_env() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    context.temp_dir.child("test.py").write_str(indoc! { "
        import os
        print(os.environ.get('THE_EMPIRE_VARIABLE'))
       "
    })?;

    context.temp_dir.child(".env").write_str(indoc! { "
        THE_^EMPIRE_VARIABLE=darth_vader
       "
    })?;

    uv_snapshot!(context.filters(), context.run().arg("--env-file").arg(".env").arg("test.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    None

    ----- stderr -----
    warning: Failed to parse environment file `.env` at position 4: THE_^EMPIRE_VARIABLE=darth_vader
    ");

    Ok(())
}

#[test]
fn run_with_not_existing_env_file() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    context.temp_dir.child("test.py").write_str(indoc! { "
        import os
        print(os.environ.get('THE_EMPIRE_VARIABLE'))
       "
    })?;

    let context = context.with_filter((
        r"(?m)^error: Failed to read environment file `.env.development`: .*$",
        "error: Failed to read environment file `.env.development`: [ERR]",
    ));

    uv_snapshot!(context.filters(), context.run().arg("--env-file").arg(".env.development").arg("test.py"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No environment file found at: `.env.development`
    ");

    Ok(())
}

#[test]
fn run_with_extra_conflict() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12.0"
        dependencies = []

        [project.optional-dependencies]
        foo = ["iniconfig==2.0.0"]
        bar = ["iniconfig==1.1.1"]

        [tool.uv]
        conflicts = [
          [
            { extra = "foo" },
            { extra = "bar" },
          ],
        ]
        "#
    })?;

    uv_snapshot!(context.filters(), context.run()
        .arg("--extra")
        .arg("foo")
        .arg("python")
        .arg("-c")
        .arg("import iniconfig"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    Ok(())
}

#[test]
fn run_with_group_conflict() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12.0"
        dependencies = []

        [dependency-groups]
        foo = ["iniconfig==2.0.0"]
        bar = ["iniconfig==1.1.1"]

        [tool.uv]
        conflicts = [
          [
            { group = "foo" },
            { group = "bar" },
          ],
        ]
        "#
    })?;

    uv_snapshot!(context.filters(), context.run()
        .arg("--group")
        .arg("foo")
        .arg("python")
        .arg("-c")
        .arg("import iniconfig"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    Ok(())
}

#[test]
fn run_default_groups() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["typing-extensions"]

        [dependency-groups]
        foo = ["anyio"]
        bar = ["iniconfig"]
        dev = ["sniffio"]
        "#,
    )?;

    context.lock().assert().success();

    // Only the main dependencies and `dev` group should be installed.
    uv_snapshot!(context.filters(), context.run().arg("python").arg("-c").arg("import typing_extensions"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + sniffio==1.3.1
     + typing-extensions==4.10.0
    ");

    // If we set a different default group, it should be synced instead.
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["typing-extensions"]

        [dependency-groups]
        foo = ["anyio"]
        bar = ["iniconfig"]
        dev = ["sniffio"]

        [tool.uv]
        default-groups = ["foo"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.run()
        .arg("--exact")
        .arg("python")
        .arg("-c")
        .arg("import typing_extensions"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
    ");

    // `--no-group` should remove from the defaults.
    uv_snapshot!(context.filters(), context.run()
        .arg("--exact")
        .arg("--no-group")
        .arg("foo")
        .arg("python")
        .arg("-c")
        .arg("import typing_extensions"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Uninstalled 3 packages in [TIME]
     - anyio==4.3.0
     - idna==3.6
     - sniffio==1.3.1
    ");

    // Using `--group` should include the defaults
    uv_snapshot!(context.filters(), context.run()
        .arg("--exact")
        .arg("--group")
        .arg("bar")
        .arg("python")
        .arg("-c")
        .arg("import iniconfig"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + iniconfig==2.0.0
     + sniffio==1.3.1
    ");

    // Using `--all-groups` should include the defaults
    uv_snapshot!(context.filters(), context.run()
        .arg("--exact")
        .arg("--all-groups")
        .arg("python")
        .arg("-c")
        .arg("import iniconfig"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Audited 5 packages in [TIME]
    ");

    // Using `--only-group` should exclude the defaults
    uv_snapshot!(context.filters(), context.run()
        .arg("--exact")
        .arg("--only-group")
        .arg("bar")
        .arg("python")
        .arg("-c")
        .arg("import iniconfig"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Uninstalled 4 packages in [TIME]
     - anyio==4.3.0
     - idna==3.6
     - sniffio==1.3.1
     - typing-extensions==4.10.0
    ");

    uv_snapshot!(context.filters(), context.run()
        .arg("--exact")
        .arg("--all-groups")
        .arg("python")
        .arg("-c")
        .arg("import iniconfig"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + sniffio==1.3.1
     + typing-extensions==4.10.0
    ");

    // Using `--no-default-groups` should exclude all groups.
    uv_snapshot!(context.filters(), context.run()
        .arg("--exact")
        .arg("--no-default-groups")
        .arg("python")
        .arg("-c")
        .arg("import typing_extensions"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Uninstalled 4 packages in [TIME]
     - anyio==4.3.0
     - idna==3.6
     - iniconfig==2.0.0
     - sniffio==1.3.1
    ");

    uv_snapshot!(context.filters(), context.run()
        .arg("--all-groups")
        .arg("python")
        .arg("-c")
        .arg("import iniconfig"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + iniconfig==2.0.0
     + sniffio==1.3.1
    ");

    // Using `--no-default-groups` with `--group foo` and `--group bar` should include those
    // groups.
    uv_snapshot!(context.filters(), context.run()
        .arg("--exact")
        .arg("--no-default-groups")
        .arg("--group")
        .arg("foo")
        .arg("--group")
        .arg("bar")
        .arg("python")
        .arg("-c")
        .arg("import typing_extensions"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Audited 5 packages in [TIME]
    ");

    Ok(())
}

#[test]
fn run_groups_requires_python() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&["3.11", "3.12", "3.13"])
        .with_filtered_python_sources();

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.11"
        dependencies = ["typing-extensions"]

        [dependency-groups]
        foo = ["anyio"]
        bar = ["iniconfig"]
        dev = ["sniffio"]

        [tool.uv.dependency-groups]
        foo = {requires-python=">=3.100"}
        bar = {requires-python=">=3.13"}
        dev = {requires-python=">=3.12"}
        "#,
    )?;

    context.lock().assert().success();

    // With --no-default-groups only the main requires-python should be consulted
    uv_snapshot!(context.filters(), context.run()
        .arg("--no-default-groups")
        .arg("python").arg("-c").arg("import typing_extensions"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.11.[X] interpreter at: [PYTHON-3.11]
    Creating virtual environment at: .venv
    Resolved 6 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + typing-extensions==4.10.0
    ");

    // The main requires-python and the default group's requires-python should be consulted
    // (This should trigger a version bump)
    uv_snapshot!(context.filters(), context.run()
        .arg("python").arg("-c").arg("import typing_extensions"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Removed virtual environment at: .venv
    Creating virtual environment at: .venv
    Resolved 6 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 2 packages in [TIME]
     + sniffio==1.3.1
     + typing-extensions==4.10.0
    ");

    // The main requires-python and "dev" and "bar" requires-python should be consulted
    // (This should trigger a version bump)
    uv_snapshot!(context.filters(), context.run()
        .arg("--group").arg("bar")
        .arg("python").arg("-c").arg("import typing_extensions"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.13.[X] interpreter at: [PYTHON-3.13]
    Removed virtual environment at: .venv
    Creating virtual environment at: .venv
    Resolved 6 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 3 packages in [TIME]
     + iniconfig==2.0.0
     + sniffio==1.3.1
     + typing-extensions==4.10.0
    ");

    // TMP: Attempt to catch this flake with verbose output
    // See https://github.com/astral-sh/uv/issues/14160
    let output = context
        .run()
        .arg("-vv")
        .arg("python")
        .arg("-c")
        .arg("import typing_extensions")
        .output()?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("Removed virtual environment"),
        "{}",
        stderr
    );

    // Going back to just "dev" we shouldn't churn the venv needlessly
    uv_snapshot!(context.filters(), context.run()
        .arg("python").arg("-c").arg("import typing_extensions"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Audited 2 packages in [TIME]
    ");

    // Explicitly requesting an in-range python can downgrade
    uv_snapshot!(context.filters(), context.run()
        .arg("-p").arg("3.12")
        .arg("python").arg("-c").arg("import typing_extensions"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Removed virtual environment at: .venv
    Creating virtual environment at: .venv
    Resolved 6 packages in [TIME]
    Installed 2 packages in [TIME]
     + sniffio==1.3.1
     + typing-extensions==4.10.0
    ");

    // Explicitly requesting an out-of-range python fails
    uv_snapshot!(context.filters(), context.run()
        .arg("-p").arg("3.11")
        .arg("python").arg("-c").arg("import typing_extensions"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.11.[X] interpreter at: [PYTHON-3.11]
    error: The requested interpreter resolved to Python 3.11.[X], which is incompatible with the project's Python requirement: `>=3.12` (from `tool.uv.dependency-groups.dev.requires-python`).
    ");

    // Enabling foo we can't find an interpreter
    uv_snapshot!(context.filters(), context.run()
        .arg("--group").arg("foo")
        .arg("python").arg("-c").arg("import typing_extensions"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No interpreter found for Python >=3.100 in [PYTHON SOURCES]
    ");

    Ok(())
}

#[test]
fn run_groups_include_requires_python() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&["3.11", "3.12", "3.13"]);

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.11"
        dependencies = ["typing-extensions"]

        [dependency-groups]
        foo = ["anyio"]
        bar = ["iniconfig"]
        baz = ["iniconfig"]
        dev = ["sniffio", {include-group = "foo"}, {include-group = "baz"}]

        [tool.uv.dependency-groups]
        foo = {requires-python="<3.13"}
        bar = {requires-python=">=3.13"}
        baz = {requires-python=">=3.12"}
        "#,
    )?;

    context.lock().assert().success();

    // With --no-default-groups only the main requires-python should be consulted
    uv_snapshot!(context.filters(), context.run()
        .arg("--no-default-groups")
        .arg("python").arg("-c").arg("import typing_extensions"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.11.[X] interpreter at: [PYTHON-3.11]
    Creating virtual environment at: .venv
    Resolved 6 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + typing-extensions==4.10.0
    ");

    // The main requires-python and the default group's requires-python should be consulted
    // (This should trigger a version bump)
    uv_snapshot!(context.filters(), context.run()
        .arg("python").arg("-c").arg("import typing_extensions"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Removed virtual environment at: .venv
    Creating virtual environment at: .venv
    Resolved 6 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 5 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + iniconfig==2.0.0
     + sniffio==1.3.1
     + typing-extensions==4.10.0
    ");

    // The main requires-python and "dev" and "bar" requires-python should be consulted
    // (This should trigger a conflict)
    uv_snapshot!(context.filters(), context.run()
        .arg("--group").arg("bar")
        .arg("python").arg("-c").arg("import typing_extensions"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Found conflicting Python requirements:
    - project: >=3.11
    - project:bar: >=3.13
    - project:dev: >=3.12, <3.13
    ");

    // Explicitly requesting an out-of-range python fails
    uv_snapshot!(context.filters(), context.run()
        .arg("-p").arg("3.13")
        .arg("python").arg("-c").arg("import typing_extensions"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.13.[X] interpreter at: [PYTHON-3.13]
    error: The requested interpreter resolved to Python 3.13.[X], which is incompatible with the project's Python requirement: `==3.12.*` (from `tool.uv.dependency-groups.dev.requires-python`).
    ");
    Ok(())
}

/// Test that a signal n makes the process exit with code 128+n.
#[cfg(unix)]
#[test]
fn exit_status_signal() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let script = context.temp_dir.child("segfault.py");
    script.write_str(indoc! {r"
        import os
        os.kill(os.getpid(), 11)
    "})?;
    let status = context.run().arg(script.path()).status()?;
    assert_eq!(status.code().expect("a status code"), 139);
    Ok(())
}

#[test]
fn run_repeated() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&["3.13", "3.12"]);

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.11, <4"
        dependencies = ["iniconfig"]
        "#
    })?;

    // Import `iniconfig` in the context of the project.
    uv_snapshot!(
        context.filters(),
        context.run().arg("--with").arg("typing-extensions").arg("python").arg("-c").arg("import typing_extensions; import iniconfig"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.13.[X] interpreter at: [PYTHON-3.13]
    Creating virtual environment at: .venv
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + typing-extensions==4.10.0
    ");

    // Re-running shouldn't require reinstalling `typing-extensions`, since the environment is cached.
    uv_snapshot!(
        context.filters(),
        context.run().arg("--with").arg("typing-extensions").arg("python").arg("-c").arg("import typing_extensions; import iniconfig"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Audited 1 package in [TIME]
    Resolved 1 package in [TIME]
    ");

    // Import `iniconfig` in the context of a `tool run` command, which should fail.
    uv_snapshot!(
        context.filters(),
        context.tool_run().arg("--with").arg("typing-extensions").arg("python").arg("-c").arg("import typing_extensions; import iniconfig"), @r#"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Traceback (most recent call last):
      File "<string>", line 1, in <module>
        import typing_extensions; import iniconfig
                                  ^^^^^^^^^^^^^^^^
    ModuleNotFoundError: No module named 'iniconfig'
    "#);

    Ok(())
}

/// See: <https://github.com/astral-sh/uv/issues/11117>
#[test]
fn run_without_overlay() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&["3.13"]);

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.11, <4"
        dependencies = ["iniconfig"]
        "#
    })?;

    // Import `iniconfig` in the context of the project.
    uv_snapshot!(
        context.filters(),
        context.run().arg("--with").arg("typing-extensions").arg("python").arg("-c").arg("import typing_extensions; import iniconfig"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.13.[X] interpreter at: [PYTHON-3.13]
    Creating virtual environment at: .venv
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + typing-extensions==4.10.0
    ");

    // Import `iniconfig` in the context of a `tool run` command, which should fail.
    uv_snapshot!(
        context.filters(),
        context.tool_run().arg("--with").arg("typing-extensions").arg("python").arg("-c").arg("import typing_extensions; import iniconfig"), @r#"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Traceback (most recent call last):
      File "<string>", line 1, in <module>
        import typing_extensions; import iniconfig
                                  ^^^^^^^^^^^^^^^^
    ModuleNotFoundError: No module named 'iniconfig'
    "#);

    // Re-running in the context of the project should reset the overlay.
    uv_snapshot!(
        context.filters(),
        context.run().arg("--with").arg("typing-extensions").arg("python").arg("-c").arg("import typing_extensions; import iniconfig"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Audited 1 package in [TIME]
    Resolved 1 package in [TIME]
    ");

    Ok(())
}

/// See: <https://github.com/astral-sh/uv/issues/11220>
#[cfg(unix)]
#[test]
fn detect_infinite_recursion() -> Result<()> {
    use indoc::formatdoc;
    use std::os::unix::fs::PermissionsExt;
    use uv_test::get_bin;

    let context = uv_test::test_context!("3.12");

    let test_script = context.temp_dir.child("main");
    test_script.write_str(&formatdoc! { r#"
        #!{uv} run

        print("Hello, world!")
    "#, uv = get_bin!().display() })?;

    fs_err::set_permissions(test_script.path(), PermissionsExt::from_mode(0o0744))?;

    let mut cmd = std::process::Command::new(test_script.as_os_str());
    context.add_shared_env(&mut cmd, false);

    // Set the max recursion depth to a lower amount to speed up testing.
    cmd.env(EnvVars::UV_RUN_MAX_RECURSION_DEPTH, "5");

    uv_snapshot!(context.filters(), cmd, @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: `uv run` was recursively invoked 6 times which exceeds the limit of 5.

    hint: If you are running a script with `uv run` in the shebang, you may need to include the `--script` flag.
    ");

    Ok(())
}

#[test]
fn run_uv_variable() {
    let context = uv_test::test_context!("3.12");

    // Display the `UV` variable
    uv_snapshot!(
        context.filters(),
        context.run().arg("python").arg("-c").arg("import os; print(os.environ['UV'])"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [UV]

    ----- stderr -----
    ");
}

/// Test legacy scripts <https://packaging.python.org/en/latest/guides/distributing-packages-using-setuptools/#scripts>.
///
/// This tests for execution and detection of legacy windows scripts with .bat, .cmd, and .ps1 extensions.
#[cfg(windows)]
#[test]
fn run_windows_legacy_scripts() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");

    // Use `script-files` which enables legacy scripts packaging.
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.8"
        dependencies = []

        [tool.setuptools]
        packages = []
        script-files = [
            "misc/custom_pydoc.bat",
            "misc/custom_pydoc.cmd",
            "misc/custom_pydoc.ps1"
        ]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#
    })?;

    let custom_pydoc_bat = context.temp_dir.child("misc").child("custom_pydoc.bat");
    let custom_pydoc_cmd = context.temp_dir.child("misc").child("custom_pydoc.cmd");
    let custom_pydoc_ps1 = context.temp_dir.child("misc").child("custom_pydoc.ps1");

    custom_pydoc_bat.write_str("python.exe -m pydoc %*")?;
    custom_pydoc_cmd.write_str("python.exe -m pydoc %*")?;
    custom_pydoc_ps1.write_str("python.exe -m pydoc $args")?;

    uv_snapshot!(context.filters(), context.run(), @r###"
    success: false
    exit_code: 2
    ----- stdout -----
    Provide a command or script to invoke with `uv run <command>` or `uv run <script>.py`.

    The following commands are available in the environment:

    - custom_pydoc.bat
    - custom_pydoc.cmd
    - custom_pydoc.ps1
    - pydoc.bat
    - python
    - pythonw

    See `uv run --help` for more information.

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/)
    "###);

    // Test with explicit .bat extension
    uv_snapshot!(context.filters(), context.run().arg("custom_pydoc.bat"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    pydoc - the Python documentation tool

    pydoc <name> ...
        Show text documentation on something.  <name> may be the name of a
        Python keyword, topic, function, module, or package, or a dotted
        reference to a class or function within a module or module in a
        package.  If <name> contains a '\', it is used as the path to a
        Python source file to document. If name is 'keywords', 'topics',
        or 'modules', a listing of these things is displayed.

    pydoc -k <keyword>
        Search for a keyword in the synopsis lines of all available modules.

    pydoc -n <hostname>
        Start an HTTP server with the given hostname (default: localhost).

    pydoc -p <port>
        Start an HTTP server on the given port on the local machine.  Port
        number 0 can be used to get an arbitrary unused port.

    pydoc -b
        Start an HTTP server on an arbitrary unused port and open a web browser
        to interactively browse documentation.  This option can be used in
        combination with -n and/or -p.

    pydoc -w <name> ...
        Write out the HTML documentation for a module to a file in the current
        directory.  If <name> contains a '\', it is treated as a filename; if
        it names a directory, documentation is written for all the contents.


    ----- stderr -----
    Resolved 1 package in [TIME]
    Audited 1 package in [TIME]
    "###);

    // Test with explicit .cmd extension
    uv_snapshot!(context.filters(), context.run().arg("custom_pydoc.cmd"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    pydoc - the Python documentation tool

    pydoc <name> ...
        Show text documentation on something.  <name> may be the name of a
        Python keyword, topic, function, module, or package, or a dotted
        reference to a class or function within a module or module in a
        package.  If <name> contains a '\', it is used as the path to a
        Python source file to document. If name is 'keywords', 'topics',
        or 'modules', a listing of these things is displayed.

    pydoc -k <keyword>
        Search for a keyword in the synopsis lines of all available modules.

    pydoc -n <hostname>
        Start an HTTP server with the given hostname (default: localhost).

    pydoc -p <port>
        Start an HTTP server on the given port on the local machine.  Port
        number 0 can be used to get an arbitrary unused port.

    pydoc -b
        Start an HTTP server on an arbitrary unused port and open a web browser
        to interactively browse documentation.  This option can be used in
        combination with -n and/or -p.

    pydoc -w <name> ...
        Write out the HTML documentation for a module to a file in the current
        directory.  If <name> contains a '\', it is treated as a filename; if
        it names a directory, documentation is written for all the contents.


    ----- stderr -----
    Resolved 1 package in [TIME]
    Audited 1 package in [TIME]
    "###);

    // Test with explicit .ps1 extension
    uv_snapshot!(context.filters(), context.run().arg("custom_pydoc.ps1"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    pydoc - the Python documentation tool

    pydoc <name> ...
        Show text documentation on something.  <name> may be the name of a
        Python keyword, topic, function, module, or package, or a dotted
        reference to a class or function within a module or module in a
        package.  If <name> contains a '\', it is used as the path to a
        Python source file to document. If name is 'keywords', 'topics',
        or 'modules', a listing of these things is displayed.

    pydoc -k <keyword>
        Search for a keyword in the synopsis lines of all available modules.

    pydoc -n <hostname>
        Start an HTTP server with the given hostname (default: localhost).

    pydoc -p <port>
        Start an HTTP server on the given port on the local machine.  Port
        number 0 can be used to get an arbitrary unused port.

    pydoc -b
        Start an HTTP server on an arbitrary unused port and open a web browser
        to interactively browse documentation.  This option can be used in
        combination with -n and/or -p.

    pydoc -w <name> ...
        Write out the HTML documentation for a module to a file in the current
        directory.  If <name> contains a '\', it is treated as a filename; if
        it names a directory, documentation is written for all the contents.


    ----- stderr -----
    Resolved 1 package in [TIME]
    Audited 1 package in [TIME]
    "###);

    // Test without explicit extension (.ps1 should be used) as there's no .exe available.
    uv_snapshot!(context.filters(), context.run().arg("custom_pydoc"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    pydoc - the Python documentation tool

    pydoc <name> ...
        Show text documentation on something.  <name> may be the name of a
        Python keyword, topic, function, module, or package, or a dotted
        reference to a class or function within a module or module in a
        package.  If <name> contains a '\', it is used as the path to a
        Python source file to document. If name is 'keywords', 'topics',
        or 'modules', a listing of these things is displayed.

    pydoc -k <keyword>
        Search for a keyword in the synopsis lines of all available modules.

    pydoc -n <hostname>
        Start an HTTP server with the given hostname (default: localhost).

    pydoc -p <port>
        Start an HTTP server on the given port on the local machine.  Port
        number 0 can be used to get an arbitrary unused port.

    pydoc -b
        Start an HTTP server on an arbitrary unused port and open a web browser
        to interactively browse documentation.  This option can be used in
        combination with -n and/or -p.

    pydoc -w <name> ...
        Write out the HTML documentation for a module to a file in the current
        directory.  If <name> contains a '\', it is treated as a filename; if
        it names a directory, documentation is written for all the contents.


    ----- stderr -----
    Resolved 1 package in [TIME]
    Audited 1 package in [TIME]
    "###);

    Ok(())
}

/// If a `--with` requirement overlaps with a locked script requirement, respect the lockfile as a
/// preference.
///
/// See: <https://github.com/astral-sh/uv/issues/13173>
#[test]
fn run_pep723_script_with_constraints_lock() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        # /// script
        # requires-python = ">=3.11"
        # dependencies = [
        #   "iniconfig<2",
        # ]
        # ///

        import iniconfig

        print("Hello, world!")
       "#
    })?;

    // Explicitly lock the script.
    uv_snapshot!(context.filters(), context.lock().arg("--script").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    ");

    let lock = context.read("main.py.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.11"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        requirements = [{ name = "iniconfig", specifier = "<2" }]

        [[package]]
        name = "iniconfig"
        version = "1.1.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/23/a2/97899f6bd0e873fed3a7e67ae8d3a08b21799430fb4da15cfedf10d6e2c2/iniconfig-1.1.1.tar.gz", hash = "sha256:bc3af051d7d14b2ee5ef9969666def0cd1a000e121eaea580d4a313df4b37f32", size = 8104, upload-time = "2020-10-14T10:20:18.572Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/9b/dd/b3c12c6d707058fa947864b67f0c4e0c39ef8610988d7baea9578f3c48f3/iniconfig-1.1.1-py2.py3-none-any.whl", hash = "sha256:011e24c64b7f47f6ebd835bb12a743f2fbe9a26d4cecaa7f53bc4f35ee9da8b3", size = 4990, upload-time = "2020-10-16T17:37:23.05Z" },
        ]
        "#
        );
    });

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.10"
        dependencies = [
          "iniconfig",
        ]
        "#
    })?;

    uv_snapshot!(context.filters(), context.run().arg("--with").arg(".").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello, world!

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==1.1.1
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 2 packages in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/)
     + iniconfig==1.1.1
    ");

    Ok(())
}

/// If a `--with` requirement overlaps with a non-locked script requirement, respect the environment
/// site-packages as preferences.
///
/// See: <https://github.com/astral-sh/uv/issues/13173>
#[test]
fn run_pep723_script_with_constraints() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        # /// script
        # requires-python = ">=3.11"
        # dependencies = [
        #   "iniconfig<2",
        # ]
        # ///

        import iniconfig

        print("Hello, world!")
       "#
    })?;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.10"
        dependencies = [
          "iniconfig",
        ]
        "#
    })?;

    uv_snapshot!(context.filters(), context.run().arg("--with").arg(".").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello, world!

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==1.1.1
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 2 packages in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/)
     + iniconfig==1.1.1
    ");

    Ok(())
}

#[test]
fn run_no_sync_incompatible_python() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&["3.12", "3.11", "3.9"]);

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = [
          "iniconfig"
        ]
        "#
    })?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        import iniconfig
        print("Hello, world!")
       "#
    })?;

    uv_snapshot!(context.filters(), context.run().arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello, world!

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: .venv
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    uv_snapshot!(context.filters(), context.run().arg("--no-sync").arg("--python").arg("3.9").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello, world!

    ----- stderr -----
    warning: Using incompatible environment (`.venv`) due to `--no-sync` (The project environment's Python version does not satisfy the request: `Python 3.9`)
    ");

    Ok(())
}

#[test]
fn run_python_preference_no_project() {
    let context =
        uv_test::test_context_with_versions!(&["3.12", "3.11"]).with_versions_as_managed(&["3.12"]);

    context.venv().assert().success();

    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.[X]

    ----- stderr -----
    ");

    uv_snapshot!(context.filters(), context.run().arg("--managed-python").arg("python").arg("--version"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.[X]

    ----- stderr -----
    ");

    // `VIRTUAL_ENV` is set here, so we'll ignore the flag
    uv_snapshot!(context.filters(), context.run().arg("--no-managed-python").arg("python").arg("--version"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.[X]

    ----- stderr -----
    ");

    // If we remove the `VIRTUAL_ENV` variable, we should get the unmanaged Python
    uv_snapshot!(context.filters(), context.run().arg("--no-managed-python").arg("python").arg("--version").env_remove(EnvVars::VIRTUAL_ENV), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.11.[X]

    ----- stderr -----
    ");
}

/// Regression test for: <https://github.com/astral-sh/uv/issues/15518>
#[test]
fn isolate_child_environment() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = [
          "iniconfig"
        ]

        [tool.uv.workspace]
        members = ["child"]
        "#
    })?;

    context
        .temp_dir
        .child("child")
        .child("pyproject.toml")
        .write_str(indoc! { r#"
        [project]
        name = "child"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = []
        "#
        })?;

    // Sync the parent package.
    uv_snapshot!(context.filters(), context.sync(), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    // Ensure that the isolated environment can't access `iniconfig` (from the parent package).
    uv_snapshot!(context.filters(), context.run().arg("--package").arg("child").arg("--isolated").arg("python").arg("-c").arg("import iniconfig"), @r#"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Audited in [TIME]
    Traceback (most recent call last):
      File "<string>", line 1, in <module>
    ModuleNotFoundError: No module named 'iniconfig'
    "#);

    // Ensure that the isolated environment can't access `iniconfig` (from the parent package).
    uv_snapshot!(context.filters(), context.run().arg("--package").arg("child").arg("--isolated").arg("--with").arg("typing-extensions").arg("python").arg("-c").arg("import iniconfig"), @r#"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Audited in [TIME]
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + typing-extensions==4.10.0
    Traceback (most recent call last):
      File "<string>", line 1, in <module>
    ModuleNotFoundError: No module named 'iniconfig'
    "#);

    Ok(())
}

#[test]
fn run_only_group_and_extra_conflict() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [project.optional-dependencies]
        test = ["pytest"]

        [dependency-groups]
        dev = ["ruff"]
        "#,
    )?;

    // Using --only-group and --extra together should error.
    uv_snapshot!(context.filters(), context.run().arg("--only-group").arg("dev").arg("--extra").arg("test").arg("python").arg("-c").arg("print('hello')"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: the argument '--only-group <ONLY_GROUP>' cannot be used with '--extra <EXTRA>'

    Usage: uv run --cache-dir [CACHE_DIR] --only-group <ONLY_GROUP> --exclude-newer <EXCLUDE_NEWER>

    For more information, try '--help'.
    ");

    // Using --only-group and --all-extras together should also error.
    uv_snapshot!(context.filters(), context.run().arg("--only-group").arg("dev").arg("--all-extras").arg("python").arg("-c").arg("print('hello')"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: the argument '--only-group <ONLY_GROUP>' cannot be used with '--all-extras'

    Usage: uv run --cache-dir [CACHE_DIR] --only-group <ONLY_GROUP> --exclude-newer <EXCLUDE_NEWER>

    For more information, try '--help'.
    ");

    Ok(())
}

/// Test that `--preview-features target-workspace-discovery` discovers the workspace
/// from the target's directory rather than the current working directory.
#[test]
fn run_target_workspace_discovery() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    // Create a workspace in a subdirectory.
    let workspace = context.temp_dir.child("project");
    workspace.create_dir_all()?;

    workspace.child("pyproject.toml").write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"
        "#
    })?;
    workspace
        .child("src")
        .child("foo")
        .child("__init__.py")
        .touch()?;

    // Create a script in the workspace that imports from the project.
    workspace.child("script.py").write_str(indoc! { r"
        import iniconfig
        print('success')
        "
    })?;

    // Without the preview feature, running from the parent directory fails to find the workspace,
    // so the dependency is not installed.
    uv_snapshot!(context.filters(), context.run().arg("project/script.py").env_remove(EnvVars::VIRTUAL_ENV), @r#"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Traceback (most recent call last):
      File "[TEMP_DIR]/project/script.py", line 1, in <module>
        import iniconfig
    ModuleNotFoundError: No module named 'iniconfig'
    "#);

    // With the preview feature, the workspace is discovered from the target's directory.
    uv_snapshot!(context.filters(), context.run().arg("--preview-features").arg("target-workspace-discovery").arg("project/script.py").env_remove(EnvVars::VIRTUAL_ENV), @"
    success: true
    exit_code: 0
    ----- stdout -----
    success

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: project/.venv
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/project)
     + iniconfig==2.0.0
    ");

    Ok(())
}

/// Test that `--preview-features target-workspace-discovery` works with a bare script
/// filename (no directory component), which would otherwise cause `Path::parent()` to
/// return an empty path.
#[test]
fn run_target_workspace_discovery_bare_script() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    context
        .temp_dir
        .child("script.py")
        .write_str(r"print('success')")?;

    // With the preview feature and a bare filename, the script should run without error.
    uv_snapshot!(context.filters(), context.run()
        .arg("--preview-features")
        .arg("target-workspace-discovery")
        .arg("script.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    success

    ----- stderr -----
    ");

    Ok(())
}
