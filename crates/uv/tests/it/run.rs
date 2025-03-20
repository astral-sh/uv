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

use crate::common::{uv_snapshot, TestContext};

#[test]
fn run_with_python_version() -> Result<()> {
    let context = TestContext::new_with_versions(&["3.12", "3.11", "3.8"]);

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
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#
    })?;
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
    uv_snapshot!(context.filters(), command_with_args, @r###"
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
    "###);

    // This is the same Python, no reinstallation.
    let mut command = context.run();
    let command_with_args = command
        .arg("-p")
        .arg("3.12")
        .arg("python")
        .arg("-B")
        .arg("main.py");
    uv_snapshot!(context.filters(), command_with_args, @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    3.12.[X]
    3.7.0

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Audited 4 packages in [TIME]
    "###);

    // This time, we target Python 3.11 instead.
    let mut command = context.run();
    let command_with_args = command
        .arg("-p")
        .arg("3.11")
        .arg("python")
        .arg("-B")
        .arg("main.py")
        .env_remove(EnvVars::VIRTUAL_ENV);

    uv_snapshot!(context.filters(), command_with_args, @r###"
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
    "###);

    // This time, we target Python 3.8 instead.
    let mut command = context.run();
    let command_with_args = command
        .arg("-p")
        .arg("3.8")
        .arg("python")
        .arg("-B")
        .arg("main.py")
        .env_remove(EnvVars::VIRTUAL_ENV);

    uv_snapshot!(context.filters(), command_with_args, @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.8.[X] interpreter at: [PYTHON-3.8]
    error: The requested interpreter resolved to Python 3.8.[X], which is incompatible with the project's Python requirement: `>=3.11, <4`
    "###);

    Ok(())
}

#[test]
fn run_args() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.8"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#
    })?;

    // We treat arguments before the command as uv arguments
    uv_snapshot!(context.filters(), context.run().arg("--version").arg("python"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    uv [VERSION] ([COMMIT] DATE)

    ----- stderr -----
    "###);

    // We don't treat arguments after the command as uv arguments
    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.[X]

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/)
    "###);

    // Can use `--` to separate uv arguments from the command arguments.
    uv_snapshot!(context.filters(), context.run().arg("--").arg("python").arg("--version"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.[X]

    ----- stderr -----
    Resolved 1 package in [TIME]
    Audited 1 package in [TIME]
    "###);

    Ok(())
}

/// Run without specifying any arguments.
///
/// This should list the available scripts.
#[test]
fn run_no_args() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.8"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#
    })?;

    // Run without specifying any argunments.
    #[cfg(not(windows))]
    uv_snapshot!(context.filters(), context.run(), @r###"
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
    "###);

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
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.8"
        dependencies = ["anyio"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#
    })?;

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
    uv_snapshot!(context.filters(), context.run().arg("main.py"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    "###);

    // Running again should use the existing environment.
    uv_snapshot!(context.filters(), context.run().arg("main.py"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    "###);

    // But neither invocation should create a lockfile.
    assert!(!context.temp_dir.child("main.py.lock").exists());

    // Otherwise, the script requirements should _not_ be available, but the project requirements
    // should.
    let test_non_script = context.temp_dir.child("main.py");
    test_non_script.write_str(indoc! { r"
        import iniconfig
       "
    })?;

    uv_snapshot!(context.filters(), context.run().arg("main.py"), @r###"
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
    "###);

    // But the script should be runnable.
    let test_non_script = context.temp_dir.child("main.py");
    test_non_script.write_str(indoc! { r#"
        import idna

        print("Hello, world!")
       "#
    })?;

    uv_snapshot!(context.filters(), context.run().arg("main.py"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello, world!

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Audited 4 packages in [TIME]
    "###);

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
    uv_snapshot!(context.filters(), context.run().arg("main.py"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello, world!

    ----- stderr -----
    "###);

    // Running a script with `--locked` should warn.
    uv_snapshot!(context.filters(), context.run().arg("--locked").arg("main.py"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello, world!

    ----- stderr -----
    warning: No lockfile found for Python script (ignoring `--locked`); run `uv lock --script` to generate a lockfile
    "###);

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
    uv_snapshot!(context.filters(), context.run().arg("--group").arg("foo").arg("main.py"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving script dependencies:
      ╰─▶ Because there are no versions of add and you require add, we can conclude that your requirements are unsatisfiable.
    "###);

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

    uv_snapshot!(context.filters(), context.run().arg("--no-project").arg("main.py"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving script dependencies:
      ╰─▶ Because there are no versions of add and you require add, we can conclude that your requirements are unsatisfiable.
    "###);

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

    uv_snapshot!(context.filters(), context.run().arg("--no-project").arg("main.py"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: An opening tag (`# /// script`) was found without a closing tag (`# ///`). Ensure that every line between the opening and closing tags (including empty lines) starts with a leading `#`.
    "###);

    Ok(())
}

#[test]
fn run_pep723_script_requires_python() -> Result<()> {
    let context = TestContext::new_with_versions(&["3.8", "3.11"]);

    // If we have a `.python-version` that's incompatible with the script, we should error.
    let python_version = context.temp_dir.child(PYTHON_VERSION_FILENAME);
    python_version.write_str("3.8")?;

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

    uv_snapshot!(context.filters(), context.run().arg("main.py"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    warning: The Python request from `.python-version` resolved to Python 3.8.[X], which is incompatible with the script's Python requirement: `>=3.11`
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    Traceback (most recent call last):
      File "main.py", line 10, in <module>
        x: str | int = "hello"
    TypeError: unsupported operand type(s) for |: 'type' and 'type'
    "###);

    // Delete the `.python-version` file to allow the script to run.
    fs_err::remove_file(&python_version)?;

    uv_snapshot!(context.filters(), context.run().arg("main.py"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    hello

    ----- stderr -----
    Resolved 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    "###);

    Ok(())
}

/// Run a `.pyw` script. The script should be executed with `pythonw.exe`.
#[test]
fn run_pythonw_script() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.8"
        dependencies = ["anyio"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#
    })?;

    let test_script = context.temp_dir.child("main.pyw");
    test_script.write_str(indoc! { r"
        import anyio
       "
    })?;

    uv_snapshot!(context.filters(), context.run().arg("main.pyw"), @r###"
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
    "###);

    Ok(())
}

/// Run a PEP 723-compatible script with `tool.uv` metadata.
#[test]
#[cfg(feature = "git")]
fn run_pep723_script_metadata() -> Result<()> {
    let context = TestContext::new("3.12");

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
    uv_snapshot!(context.filters(), context.run().arg("main.py"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==1.0.1
    "###);

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
    uv_snapshot!(context.filters(), context.run().arg("main.py"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + uv-public-pypackage==0.1.0 (from git+https://github.com/astral-test/uv-public-pypackage@0dacfd662c64cb4ceb16e6cf65a157a8b715b979)
    "###);

    Ok(())
}

/// Run a PEP 723-compatible script with a `[[tool.uv.index]]`.
#[test]
fn run_pep723_script_index() -> Result<()> {
    let context = TestContext::new("3.12");

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

    uv_snapshot!(context.filters(), context.run().arg("main.py"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + idna==2.7
    "###);

    Ok(())
}

/// Run a PEP 723-compatible script with `tool.uv` constraints.
#[test]
fn run_pep723_script_constraints() -> Result<()> {
    let context = TestContext::new("3.12");

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

    uv_snapshot!(context.filters(), context.run().arg("main.py"), @r###"
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
    "###);

    Ok(())
}

/// Run a PEP 723-compatible script with `tool.uv` overrides.
#[test]
fn run_pep723_script_overrides() -> Result<()> {
    let context = TestContext::new("3.12");

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

    uv_snapshot!(context.filters(), context.run().arg("main.py"), @r###"
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
    "###);

    Ok(())
}

/// Run a PEP 723-compatible script with a lockfile.
#[test]
fn run_pep723_script_lock() -> Result<()> {
    let context = TestContext::new("3.12");

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
    uv_snapshot!(context.filters(), context.run().arg("--locked").arg("main.py"), @r###"
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
    "###);

    // Explicitly lock the script.
    uv_snapshot!(context.filters(), context.lock().arg("--script").arg("main.py"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    "###);

    let lock = context.read("main.py.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        revision = 1
        requires-python = ">=3.11"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        requirements = [{ name = "iniconfig" }]

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
        ]
        "###
        );
    });

    // Run the script.
    uv_snapshot!(context.filters(), context.run().arg("main.py"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello, world!

    ----- stderr -----
    Resolved 1 package in [TIME]
    Audited 1 package in [TIME]
    "###);

    // With a lockfile, running with `--locked` should not warn.
    uv_snapshot!(context.filters(), context.run().arg("--locked").arg("main.py"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello, world!

    ----- stderr -----
    Resolved 1 package in [TIME]
    Audited 1 package in [TIME]
    "###);

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
    uv_snapshot!(context.filters(), context.run().arg("--locked").arg("main.py"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided. To update the lockfile, run `uv lock`.
    "###);

    // Re-running the script with `--frozen` should also error, but at runtime.
    uv_snapshot!(context.filters(), context.run().arg("--frozen").arg("main.py"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    Traceback (most recent call last):
      File "[TEMP_DIR]/main.py", line 8, in <module>
        import anyio
    ModuleNotFoundError: No module named 'anyio'
    "###);

    // Re-running the script should update the lockfile.
    uv_snapshot!(context.filters(), context.run().arg("main.py"), @r###"
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
    "###);

    let lock = context.read("main.py.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        revision = 1
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
        sdist = { url = "https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz", hash = "sha256:f75253795a87df48568485fd18cdd2a3fa5c4f7c5be8e5e36637733fce06fed6", size = 159642 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl", hash = "sha256:048e05d0f6caeed70d731f3db756d35dcc1f35747c8c403364a8332c630441b8", size = 85584 },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 },
        ]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 },
        ]
        "###
        );
    });

    Ok(())
}

/// With `managed = false`, we should avoid installing the project itself.
#[test]
fn run_managed_false() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.8"
        dependencies = ["anyio"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv]
        managed = false
        "#
    })?;

    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.[X]

    ----- stderr -----
    "###);

    Ok(())
}

#[test]
fn run_exact() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.8"
        dependencies = ["iniconfig"]
        "#
    })?;

    uv_snapshot!(context.filters(), context.run().arg("python").arg("-c").arg("import iniconfig"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    "###);

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
    uv_snapshot!(context.filters(), context.run().arg("python").arg("-c").arg("import iniconfig; import anyio"), @r###"
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
    "###);

    // But under `--exact`, `iniconfig` should not be available.
    uv_snapshot!(context.filters(), context.run().arg("--exact").arg("python").arg("-c").arg("import iniconfig"), @r###"
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
    "###);

    Ok(())
}

#[test]
fn run_with() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.8"
        dependencies = ["sniffio==1.3.0"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#
    })?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r"
        import sniffio
       "
    })?;

    // Requesting an unsatisfied requirement should install it.
    uv_snapshot!(context.filters(), context.run().arg("--with").arg("iniconfig").arg("main.py"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

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
    "###);

    // Requesting a satisfied requirement should use the base environment.
    uv_snapshot!(context.filters(), context.run().arg("--with").arg("sniffio").arg("main.py"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Audited 2 packages in [TIME]
    "###);

    // Unless the user requests a different version.
    uv_snapshot!(context.filters(), context.run().arg("--with").arg("sniffio<1.3.0").arg("main.py"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Audited 2 packages in [TIME]
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + sniffio==1.2.0
    "###);

    // If we request a dependency that isn't in the base environment, we should still respect any
    // other dependencies. In this case, `sniffio==1.3.0` is not the latest-compatible version, but
    // we should use it anyway.
    uv_snapshot!(context.filters(), context.run().arg("--with").arg("anyio").arg("main.py"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Audited 2 packages in [TIME]
    Resolved 3 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + sniffio==1.3.0
    "###);

    // Even if we run with` --no-sync`.
    uv_snapshot!(context.filters(), context.run().arg("--with").arg("anyio==4.2.0").arg("--no-sync").arg("main.py"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.2.0
     + idna==3.6
     + sniffio==1.3.0
    "###);

    // If the dependencies can't be resolved, we should reference `--with`.
    uv_snapshot!(context.filters(), context.run().arg("--with").arg("add").arg("main.py"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Audited 2 packages in [TIME]
      × No solution found when resolving `--with` dependencies:
      ╰─▶ Because there are no versions of add and you require add, we can conclude that your requirements are unsatisfiable.
    "###);

    Ok(())
}

/// Sync all members in a workspace.
#[test]
fn run_in_workspace() -> Result<()> {
    let context = TestContext::new("3.12");

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

    uv_snapshot!(context.filters(), context.run().arg("main.py"), @r###"
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
    "###);

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r"
        import iniconfig
       "
    })?;

    uv_snapshot!(context.filters(), context.run().arg("main.py"), @r###"
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
    "###);

    uv_snapshot!(context.filters(), context.run().arg("--package").arg("child1").arg("main.py"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 8 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + child1==0.1.0 (from file://[TEMP_DIR]/child1)
     + iniconfig==2.0.0
    "###);

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r"
        import typing_extensions
       "
    })?;

    uv_snapshot!(context.filters(), context.run().arg("main.py"), @r###"
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
    "###);

    uv_snapshot!(context.filters(), context.run().arg("--all-packages").arg("main.py"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 8 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + child2==0.1.0 (from file://[TEMP_DIR]/child2)
     + typing-extensions==4.10.0
    "###);

    Ok(())
}

#[test]
fn run_with_editable() -> Result<()> {
    let context = TestContext::new("3.12");

    let anyio_local = context.temp_dir.child("src").child("anyio_local");
    copy_dir_all(
        context.workspace_root.join("scripts/packages/anyio_local"),
        &anyio_local,
    )?;

    let black_editable = context.temp_dir.child("src").child("black_editable");
    copy_dir_all(
        context
            .workspace_root
            .join("scripts/packages/black_editable"),
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
    uv_snapshot!(context.filters(), context.run().arg("--with-editable").arg("./src/black_editable").arg("main.py"), @r###"
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
    "###);

    // Requesting an editable requirement should install it in a layer, even if it satisfied
    uv_snapshot!(context.filters(), context.run().arg("--with-editable").arg("./src/anyio_local").arg("main.py"), @r###"
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
    "###);

    // Requesting the project itself should use the base environment.
    uv_snapshot!(context.filters(), context.run().arg("--with-editable").arg(".").arg("main.py"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Audited 4 packages in [TIME]
    "###);

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

    uv_snapshot!(context.filters(), context.sync(), @r###"
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
    "###);

    uv_snapshot!(context.filters(), context.run().arg("--with-editable").arg("./src/anyio_local").arg("main.py"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Audited 3 packages in [TIME]
    "###);

    // If invalid, we should reference `--with-editable`.
    uv_snapshot!(context.filters(), context.run().arg("--with-editable").arg("./foo").arg("main.py"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Audited 3 packages in [TIME]
      × Failed to resolve `--with` requirement
      ╰─▶ Distribution not found at: file://[TEMP_DIR]/foo
    "###);

    Ok(())
}

#[test]
fn run_group() -> Result<()> {
    let context = TestContext::new("3.12");

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

    uv_snapshot!(context.filters(), context.run().arg("main.py"), @r###"
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
    "###);

    uv_snapshot!(context.filters(), context.run().arg("--only-group").arg("bar").arg("main.py"), @r###"
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
    "###);

    uv_snapshot!(context.filters(), context.run().arg("--group").arg("foo").arg("main.py"), @r###"
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
    "###);

    uv_snapshot!(context.filters(), context.run().arg("--group").arg("foo").arg("--group").arg("bar").arg("main.py"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    imported `anyio`
    imported `iniconfig`
    imported `typing_extensions`

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Audited 5 packages in [TIME]
    "###);

    uv_snapshot!(context.filters(), context.run().arg("--all-groups").arg("main.py"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    imported `anyio`
    imported `iniconfig`
    imported `typing_extensions`

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Audited 5 packages in [TIME]
    "###);

    uv_snapshot!(context.filters(), context.run().arg("--all-groups").arg("--no-group").arg("bar").arg("main.py"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    imported `anyio`
    imported `iniconfig`
    imported `typing_extensions`

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Audited 4 packages in [TIME]
    "###);

    uv_snapshot!(context.filters(), context.run().arg("--group").arg("foo").arg("--no-project").arg("main.py"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    imported `anyio`
    imported `iniconfig`
    imported `typing_extensions`

    ----- stderr -----
    warning: `--group foo` has no effect when used alongside `--no-project`
    "###);

    uv_snapshot!(context.filters(), context.run().arg("--group").arg("foo").arg("--group").arg("bar").arg("--no-project").arg("main.py"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    imported `anyio`
    imported `iniconfig`
    imported `typing_extensions`

    ----- stderr -----
    warning: `--group` has no effect when used alongside `--no-project`
    ");

    uv_snapshot!(context.filters(), context.run().arg("--group").arg("dev").arg("--no-project").arg("main.py"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    imported `anyio`
    imported `iniconfig`
    imported `typing_extensions`

    ----- stderr -----
    warning: `--group dev` has no effect when used alongside `--no-project`
    "###);

    uv_snapshot!(context.filters(), context.run().arg("--all-groups").arg("--no-project").arg("main.py"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    imported `anyio`
    imported `iniconfig`
    imported `typing_extensions`

    ----- stderr -----
    warning: `--all-groups` has no effect when used alongside `--no-project`
    "###);

    uv_snapshot!(context.filters(), context.run().arg("--dev").arg("--no-project").arg("main.py"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    imported `anyio`
    imported `iniconfig`
    imported `typing_extensions`

    ----- stderr -----
    warning: `--dev` has no effect when used alongside `--no-project`
    "###);

    Ok(())
}

#[test]
fn run_locked() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
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

    // Running with `--locked` should error, if no lockfile is present.
    uv_snapshot!(context.filters(), context.run().arg("--locked").arg("--").arg("python").arg("--version"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Unable to find lockfile at `uv.lock`. To create a lockfile, run `uv lock` or `uv sync`.
    "###);

    // Lock the initial requirements.
    context.lock().assert().success();

    let existing = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            existing, @r###"
        version = 1
        revision = 1
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
        sdist = { url = "https://files.pythonhosted.org/packages/c6/b3/fefbf7e78ab3b805dec67d698dc18dd505af7a18a8dd08868c9b4fa736b5/anyio-3.7.0.tar.gz", hash = "sha256:275d9973793619a5374e1c89a4f4ad3f4b0a5510a2b5b939444bee8f4c4d37ce", size = 142737 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/68/fe/7ce1926952c8a403b35029e194555558514b365ad77d75125f521a2bec62/anyio-3.7.0-py3-none-any.whl", hash = "sha256:eddca883c4175f14df8aedce21054bfca3adb70ffe76a9f607aef9d7fa2ea7f0", size = 80873 },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 },
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
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 },
        ]
        "###);
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
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    // Running with `--locked` should error.
    uv_snapshot!(context.filters(), context.run().arg("--locked").arg("--").arg("python").arg("--version"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided. To update the lockfile, run `uv lock`.
    "###);

    let updated = context.read("uv.lock");

    // And the lockfile should be unchanged.
    assert_eq!(existing, updated);

    // Lock the updated requirements.
    uv_snapshot!(context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Removed anyio v3.7.0
    Removed idna v3.6
    Added iniconfig v2.0.0
    Removed sniffio v1.3.1
    "###);

    // Lock the updated requirements.
    uv_snapshot!(context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    // Running with `--locked` should succeed.
    uv_snapshot!(context.filters(), context.run().arg("--locked").arg("--").arg("python").arg("--version"), @r###"
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
    "###);

    Ok(())
}

#[test]
fn run_frozen() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
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

    // Running with `--frozen` should error, if no lockfile is present.
    uv_snapshot!(context.filters(), context.run().arg("--frozen").arg("--").arg("python").arg("--version"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Unable to find lockfile at `uv.lock`. To create a lockfile, run `uv lock` or `uv sync`.
    "###);

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
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    // Running with `--frozen` should install the stale lockfile.
    uv_snapshot!(context.filters(), context.run().arg("--frozen").arg("--").arg("python").arg("--version"), @r###"
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
    "###);

    Ok(())
}

#[test]
fn run_no_sync() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
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

    // Running with `--no-sync` should succeed error, even if the lockfile isn't present.
    uv_snapshot!(context.filters(), context.run().arg("--no-sync").arg("--").arg("python").arg("--version"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.[X]

    ----- stderr -----
    "###);

    context.lock().assert().success();

    // Running with `--no-sync` should not install any requirements.
    uv_snapshot!(context.filters(), context.run().arg("--no-sync").arg("--").arg("python").arg("--version"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.[X]

    ----- stderr -----
    "###);

    context.sync().assert().success();

    // But it should have access to the installed packages.
    uv_snapshot!(context.filters(), context.run().arg("--no-sync").arg("--").arg("python").arg("-c").arg("import anyio; print(anyio.__name__)"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    anyio

    ----- stderr -----
    "###);

    Ok(())
}

#[test]
fn run_empty_requirements_txt() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.8"
        dependencies = ["anyio", "sniffio==1.3.1"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#
    })?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r"
        import sniffio
       "
    })?;

    let requirements_txt =
        ChildPath::new(context.temp_dir.canonicalize()?.join("requirements.txt"));
    requirements_txt.touch()?;

    // The project environment is synced on the first invocation.
    uv_snapshot!(context.filters(), context.run().arg("--with-requirements").arg(requirements_txt.as_os_str()).arg("main.py"), @r###"
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
    "###);

    // Then reused in subsequent invocations
    uv_snapshot!(context.filters(), context.run().arg("--with-requirements").arg(requirements_txt.as_os_str()).arg("main.py"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Audited 4 packages in [TIME]
    warning: Requirements file `requirements.txt` does not contain any dependencies
    "###);

    Ok(())
}

#[test]
fn run_requirements_txt() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.8"
        dependencies = ["anyio", "sniffio==1.3.1"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#
    })?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r"
        import sniffio
       "
    })?;

    // Requesting an unsatisfied requirement should install it.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("iniconfig")?;

    uv_snapshot!(context.filters(), context.run().arg("--with-requirements").arg(requirements_txt.as_os_str()).arg("main.py"), @r###"
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
    "###);

    // Requesting a satisfied requirement should use the base environment.
    requirements_txt.write_str("sniffio")?;

    uv_snapshot!(context.filters(), context.run().arg("--with-requirements").arg(requirements_txt.as_os_str()).arg("main.py"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Audited 4 packages in [TIME]
    "###);

    // Unless the user requests a different version.
    requirements_txt.write_str("sniffio<1.3.1")?;

    uv_snapshot!(context.filters(), context.run().arg("--with-requirements").arg(requirements_txt.as_os_str()).arg("main.py"), @r###"
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
    "###);

    // Or includes an unsatisfied requirement via `--with`.
    requirements_txt.write_str("sniffio")?;

    uv_snapshot!(context.filters(), context.run()
        .arg("--with-requirements")
        .arg(requirements_txt.as_os_str())
        .arg("--with")
        .arg("iniconfig")
        .arg("main.py"), @r###"
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
    "###);

    // Allow `-` for stdin.
    uv_snapshot!(context.filters(), context.run()
        .arg("--with-requirements")
        .arg("-")
        .arg("--with")
        .arg("iniconfig")
        .arg("main.py")
        .stdin(std::fs::File::open(&requirements_txt)?), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Audited 4 packages in [TIME]
    Resolved 2 packages in [TIME]
    "###);

    // But not in combination with reading the script from stdin
    uv_snapshot!(context.filters(), context.run()
        .arg("--with-requirements")
        .arg("-")
        // The script to run
        .arg("-")
        .stdin(std::fs::File::open(&requirements_txt)?), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Cannot read both requirements file and script from stdin
    "###);

    uv_snapshot!(context.filters(), context.run()
        .arg("--with-requirements")
        .arg("-")
        .arg("--script")
        .arg("-")
        .stdin(std::fs::File::open(&requirements_txt)?), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Cannot read both requirements file and script from stdin
    "###);

    Ok(())
}

/// Ignore and warn when (e.g.) the `--index-url` argument is a provided `requirements.txt`.
#[test]
fn run_requirements_txt_arguments() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.8"
        dependencies = ["typing_extensions"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#
    })?;

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

    uv_snapshot!(context.filters(), context.run().arg("--with-requirements").arg(requirements_txt.as_os_str()).arg("main.py"), @r###"
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
    "###);

    Ok(())
}

/// Ensure that we can import from the root project when layering `--with` requirements.
#[test]
fn run_editable() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.8"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
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
    uv_snapshot!(context.filters(), context.run().arg("--with").arg("iniconfig").arg("main.py"), @r###"
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
    "###);

    Ok(())
}

#[test]
fn run_from_directory() -> Result<()> {
    // Default to 3.11 so that the `.python-version` is meaningful.
    let context = TestContext::new_with_versions(&["3.10", "3.11", "3.12"]);

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
    let error = regex::escape("The system cannot find the path specified. (os error 3)");
    let filters = context
        .filters()
        .into_iter()
        .chain(
            filters
                .iter()
                .map(|(pattern, replacement)| (pattern.as_str(), replacement.as_str())),
        )
        .chain(std::iter::once((
            error.as_str(),
            "No such file or directory (os error 2)",
        )))
        .collect::<Vec<_>>();

    // Use `--project`, which resolves configuration relative to the provided directory, but paths
    // relative to the current working directory.
    uv_snapshot!(filters.clone(), context.run().arg("--project").arg("project").arg("main"), @r###"
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
    "###);

    fs_err::remove_dir_all(context.temp_dir.join("project").join(".venv"))?;
    uv_snapshot!(filters.clone(), context.run().arg("--project").arg("project").arg("./project/main.py"), @r###"
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
    "###);

    // Use `--directory`, which switches to the provided directory entirely.
    fs_err::remove_dir_all(context.temp_dir.join("project").join(".venv"))?;
    uv_snapshot!(filters.clone(), context.run().arg("--directory").arg("project").arg("main"), @r###"
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
    "###);

    fs_err::remove_dir_all(context.temp_dir.join("project").join(".venv"))?;
    uv_snapshot!(filters.clone(), context.run().arg("--directory").arg("project").arg("./main.py"), @r###"
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
    "###);

    fs_err::remove_dir_all(context.temp_dir.join("project").join(".venv"))?;
    uv_snapshot!(filters.clone(), context.run().arg("--directory").arg("project").arg("./project/main.py"), @r###"
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
      Caused by: No such file or directory (os error 2)
    "###);

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
    uv_snapshot!(filters.clone(), context.run().arg("--project").arg("project").arg("main"), @r###"
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
    "###);

    fs_err::remove_dir_all(context.temp_dir.join("project").join(".venv"))?;
    uv_snapshot!(filters.clone(), context.run().arg("--directory").arg("project").arg("main"), @r###"
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
    "###);

    Ok(())
}

/// By default, omit resolver and installer output.
#[test]
fn run_without_output() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.8"
        dependencies = ["anyio", "sniffio==1.3.1"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#
    })?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r"
        import sniffio
       "
    })?;

    // On the first run, we only show the summary line for each environment.
    uv_snapshot!(context.filters(), context.run().env_remove(EnvVars::UV_SHOW_RESOLUTION).arg("--with").arg("iniconfig").arg("main.py"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed 4 packages in [TIME]
    Installed 1 package in [TIME]
    "###);

    // Subsequent runs are quiet.
    uv_snapshot!(context.filters(), context.run().env_remove(EnvVars::UV_SHOW_RESOLUTION).arg("--with").arg("iniconfig").arg("main.py"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    "###);

    Ok(())
}

/// Ensure that we can import from the root project when layering `--with` requirements.
#[test]
fn run_isolated_python_version() -> Result<()> {
    let context = TestContext::new_with_versions(&["3.8", "3.12"]);

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

    uv_snapshot!(context.filters(), context.run().arg("main.py"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    (3, 8)

    ----- stderr -----
    Using CPython 3.8.[X] interpreter at: [PYTHON-3.8]
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
    "###);

    uv_snapshot!(context.filters(), context.run().arg("--isolated").arg("main.py"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    (3, 8)

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Installed 6 packages in [TIME]
     + anyio==4.3.0
     + exceptiongroup==1.2.0
     + foo==1.0.0 (from file://[TEMP_DIR]/)
     + idna==3.6
     + sniffio==1.3.1
     + typing-extensions==4.10.0
    "###);

    // Set the `.python-version` to `3.12`.
    context
        .temp_dir
        .child(PYTHON_VERSION_FILENAME)
        .write_str("3.12")?;

    uv_snapshot!(context.filters(), context.run().arg("--isolated").arg("main.py"), @r###"
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
    "###);

    Ok(())
}

/// Ignore the existing project when executing with `--no-project`.
#[test]
fn run_no_project() -> Result<()> {
    let context = TestContext::new("3.12")
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
    uv_snapshot!(context.filters(), context.run().arg("python").arg("-c").arg("import sys; print(sys.executable)"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [VENV]/[BIN]/python

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.3.0
     + foo==1.0.0 (from file://[TEMP_DIR]/)
     + idna==3.6
     + sniffio==1.3.1
    "###);

    // `run --no-project` should not (but it should still run in the same environment, as it would
    // if there were no project at all).
    uv_snapshot!(context.filters(), context.run().arg("--no-project").arg("python").arg("-c").arg("import sys; print(sys.executable)"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [VENV]/[BIN]/python

    ----- stderr -----
    "###);

    // `run --no-project --isolated` should run in an entirely isolated environment.
    uv_snapshot!(context.filters(), context.run().arg("--no-project").arg("--isolated").arg("python").arg("-c").arg("import sys; print(sys.executable)"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [CACHE_DIR]/builds-v0/[TMP]/python

    ----- stderr -----
    "###);

    // `run --no-project` should not (but it should still run in the same environment, as it would
    // if there were no project at all).
    uv_snapshot!(context.filters(), context.run().arg("--no-project").arg("python").arg("-c").arg("import sys; print(sys.executable)"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [VENV]/[BIN]/python

    ----- stderr -----
    "###);

    // `run --no-project --locked` should fail.
    uv_snapshot!(context.filters(), context.run().arg("--no-project").arg("--locked").arg("python").arg("-c").arg("import sys; print(sys.executable)"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [VENV]/[BIN]/python

    ----- stderr -----
    warning: `--locked` has no effect when used alongside `--no-project`
    "###);

    Ok(())
}

#[test]
fn run_stdin() -> Result<()> {
    let context = TestContext::new("3.12");

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        print("Hello, world!")
       "#
    })?;

    let mut command = context.run();
    let command_with_args = command.stdin(std::fs::File::open(test_script)?).arg("-");
    uv_snapshot!(context.filters(), command_with_args, @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello, world!

    ----- stderr -----
    "###);

    Ok(())
}

#[test]
fn run_package() -> Result<()> {
    let context = TestContext::new("3.12");

    let main_script = context.temp_dir.child("__main__.py");
    main_script.write_str(indoc! { r#"
        print("Hello, world!")
       "#
    })?;

    uv_snapshot!(context.filters(), context.run().arg("."), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello, world!

    ----- stderr -----
    "###);

    Ok(())
}

#[test]
fn run_zipapp() -> Result<()> {
    let context = TestContext::new("3.12");

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
    uv_snapshot!(context.filters(), context.run().arg(zipapp.as_ref()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello, world!

    ----- stderr -----
    "###);

    Ok(())
}

#[test]
fn run_stdin_args() {
    let context = TestContext::new("3.12");

    uv_snapshot!(context.filters(), context.run().arg("python").arg("-c").arg("import sys; print(sys.argv)").arg("foo").arg("bar"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    ['-c', 'foo', 'bar']

    ----- stderr -----
    "###);
}

/// Run a module equivalent to `python -m foo`.
#[test]
fn run_module() {
    let context = TestContext::new("3.12");

    uv_snapshot!(context.filters(), context.run().arg("-m").arg("__hello__"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello world!

    ----- stderr -----
    "#);

    uv_snapshot!(context.filters(), context.run().arg("-m").arg("http.server").arg("-h"), @r#"
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
    "#);
}

#[test]
fn run_module_stdin() {
    let context = TestContext::new("3.12");

    uv_snapshot!(context.filters(), context.run().arg("-m").arg("-"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Cannot run a Python module from stdin
    "###);
}

/// When the `pyproject.toml` file is invalid.
#[test]
fn run_project_toml_error() -> Result<()> {
    let context = TestContext::new("3.12")
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

    // `run` should fail
    uv_snapshot!(context.filters(), context.run().arg("python").arg("-c").arg("import sys; print(sys.executable)"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No `project` table found in: `[TEMP_DIR]/pyproject.toml`
    "###);

    // `run --no-project` should not
    uv_snapshot!(context.filters(), context.run().arg("--no-project").arg("python").arg("-c").arg("import sys; print(sys.executable)"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [VENV]/[BIN]/python

    ----- stderr -----
    "###);

    Ok(())
}

#[test]
fn run_isolated_incompatible_python() -> Result<()> {
    let context = TestContext::new_with_versions(&["3.8", "3.11"]);

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
    python_version.write_str("3.8")?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        import iniconfig

        x: str | int = "hello"
        print(x)
       "#
    })?;

    // We should reject Python 3.8...
    uv_snapshot!(context.filters(), context.run().arg("main.py"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.8.[X] interpreter at: [PYTHON-3.8]
    error: The Python request from `.python-version` resolved to Python 3.8.[X], which is incompatible with the project's Python requirement: `>=3.12`. Use `uv python pin` to update the `.python-version` file to a compatible version.
    "###);

    // ...even if `--isolated` is provided.
    uv_snapshot!(context.filters(), context.run().arg("--isolated").arg("main.py"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: The Python request from `.python-version` resolved to Python 3.8.[X], which is incompatible with the project's Python requirement: `>=3.12`. Use `uv python pin` to update the `.python-version` file to a compatible version.
    "###);

    Ok(())
}

#[test]
fn run_compiled_python_file() -> Result<()> {
    let context = TestContext::new("3.12");

    // Write a non-PEP 723 script.
    let test_non_script = context.temp_dir.child("main.py");
    test_non_script.write_str(indoc! { r#"
        print("Hello, world!")
       "#
    })?;

    // Run a non-PEP 723 script.
    uv_snapshot!(context.filters(), context.run().arg("main.py"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello, world!

    ----- stderr -----
    "###);

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
    uv_snapshot!(context.filters(), context.run().arg(compiled_non_script.path()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello, world!

    ----- stderr -----
    "###);

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

    uv_snapshot!(context.filters(), context.run().arg("script.py"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    "###);

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
    uv_snapshot!(context.filters(), context.run().arg(compiled_script.path()), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Traceback (most recent call last):
      File "[TEMP_DIR]/script.py", line 7, in <module>
        import iniconfig
    ModuleNotFoundError: No module named 'iniconfig'
    "###);

    Ok(())
}

#[test]
fn run_exit_code() -> Result<()> {
    let context = TestContext::new("3.12");

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
    let context = TestContext::new_with_versions(&["3.12", "3.11", "3.8"]);

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project.urls]
        repository = 'https://github.com/octocat/octocat-python'

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#
    })?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r#"
        print("Hello, world!")
       "#
    })?;

    uv_snapshot!(context.filters(), context.run().arg("main.py"), @r###"
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
    "###);

    Ok(())
}

#[test]
#[cfg(target_family = "unix")]
fn run_script_without_build_system() -> Result<()> {
    let context = TestContext::new("3.12");

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
    uv_snapshot!(context.filters(), context.run().arg("entry"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Audited in [TIME]
    error: Failed to spawn: `entry`
      Caused by: No such file or directory (os error 2)
    "###);

    Ok(())
}

#[test]
fn run_script_module_conflict() -> Result<()> {
    let context = TestContext::new("3.12");

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

    uv_snapshot!(context.filters(), context.run().arg("foo"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello from `__init__`

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + foo==0.1.0 (from file://[TEMP_DIR]/)
    "###);

    // Creating `__main__` should not change the behavior, the entrypoint should take precedence
    let main = context.temp_dir.child("src/foo/__main__.py");
    main.write_str(indoc! { r#"
        print("Hello from `__main__`")
       "#
    })?;

    uv_snapshot!(context.filters(), context.run().arg("foo"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello from `__init__`

    ----- stderr -----
    Resolved 1 package in [TIME]
    Audited 1 package in [TIME]
    "###);

    // Even if the working directory is `src`
    uv_snapshot!(context.filters(), context.run().arg("--directory").arg("src").arg("foo"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello from `__init__`

    ----- stderr -----
    Resolved 1 package in [TIME]
    Audited 1 package in [TIME]
    "###);

    // Unless the user opts-in to module running with `-m`
    uv_snapshot!(context.filters(), context.run().arg("-m").arg("foo"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello from `__main__`

    ----- stderr -----
    Resolved 1 package in [TIME]
    Audited 1 package in [TIME]
    "###);

    Ok(())
}

#[test]
fn run_script_explicit() -> Result<()> {
    let context = TestContext::new("3.12");

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

    uv_snapshot!(context.filters(), context.run().arg("--script").arg("script"), @r###"
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
fn run_script_explicit_stdin() -> Result<()> {
    let context = TestContext::new("3.12");

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

    uv_snapshot!(context.filters(), context.run().arg("--script").arg("-").stdin(std::fs::File::open(test_script)?), @r###"
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
fn run_script_explicit_no_file() {
    let context = TestContext::new("3.12");
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
    let context = TestContext::new("3.12");

    fs_err::create_dir(context.temp_dir.child("script"))?;

    uv_snapshot!(context.filters(), context.run().arg("--script").arg("script"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: failed to read from file `script`: Is a directory (os error 21)
    "###);

    Ok(())
}

#[test]
#[cfg(windows)]
fn run_gui_script_explicit_windows() -> Result<()> {
    let context = TestContext::new("3.12");

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
    let context = TestContext::new("3.12");

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
    let context = TestContext::new("3.12");
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

    uv_snapshot!(context.filters(), context.run().arg("--gui-script").arg("script"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using executable: python
    "###);

    Ok(())
}

#[test]
#[cfg(unix)]
fn run_linked_environment_path() -> Result<()> {
    use anyhow::Ok;

    let context = TestContext::new("3.12")
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
        .env(EnvVars::UV_PROJECT_ENVIRONMENT, "target"), @r###"
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
    "###);

    // `sys.prefix` and `sys.executable` should be from the `target` directory
    uv_snapshot!(context.filters(), context.run()
        .env_remove("VIRTUAL_ENV")  // Ignore the test context's active virtual environment
        .env(EnvVars::UV_PROJECT_ENVIRONMENT, "target")
        .arg("python").arg("-c").arg("import sys; print(sys.prefix); print(sys.executable)"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/target
    [TEMP_DIR]/target/[BIN]/python

    ----- stderr -----
    Resolved 8 packages in [TIME]
    Audited 6 packages in [TIME]
    "###);

    // And, similarly, the entrypoint should use `target`
    let black_entrypoint = context.read("target/bin/black");
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            black_entrypoint, @r###"
        #![TEMP_DIR]/target/[BIN]/python
        # -*- coding: utf-8 -*-
        import sys
        from black import patched_main
        if __name__ == "__main__":
            if sys.argv[0].endswith("-script.pyw"):
                sys.argv[0] = sys.argv[0][:-11]
            elif sys.argv[0].endswith(".exe"):
                sys.argv[0] = sys.argv[0][:-4]
            sys.exit(patched_main())
        "###
        );
    });

    Ok(())
}

#[test]
fn run_active_project_environment() -> Result<()> {
    let context = TestContext::new_with_versions(&["3.11", "3.12"])
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
        .env(EnvVars::VIRTUAL_ENV, "foo"), @r###"
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
    "###);

    // Using `--no-active` should silence the warning
    uv_snapshot!(context.filters(), context.run()
        .arg("--no-active")
        .arg("python").arg("--version")
        .env(EnvVars::VIRTUAL_ENV, "foo"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.11.[X]

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Audited 1 package in [TIME]
    "###);

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
        .env(EnvVars::VIRTUAL_ENV, "foo"), @r###"
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
    "###);

    context
        .temp_dir
        .child("foo")
        .assert(predicate::path::is_dir());

    // Requesting a different Python version should invalidate the environment
    uv_snapshot!(context.filters(), context.run()
        .arg("--active")
        .arg("-p").arg("3.12")
        .arg("python").arg("--version")
        .env(EnvVars::VIRTUAL_ENV, "foo"), @r###"
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
    "###);

    Ok(())
}

#[test]
fn run_active_script_environment() -> Result<()> {
    let context = TestContext::new_with_versions(&["3.11", "3.12"])
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

    let filters = context
        .filters()
        .into_iter()
        .chain(vec![(
            r"environments-v1/main-\w+",
            "environments-v1/main-[HASH]",
        )])
        .collect::<Vec<_>>();

    // Running `uv run --script` with `VIRTUAL_ENV` should _not_ warn.
    uv_snapshot!(&filters, context.run()
        .arg("--script")
        .arg("main.py")
        .env(EnvVars::VIRTUAL_ENV, "foo"), @r###"
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

    // Using `--no-active` should also _not_ warn.
    uv_snapshot!(&filters, context.run()
        .arg("--no-active")
        .arg("--script")
        .arg("main.py")
        .env(EnvVars::VIRTUAL_ENV, "foo"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello, world!

    ----- stderr -----
    "###);

    context
        .temp_dir
        .child("foo")
        .assert(predicate::path::missing());

    // Using `--active` should create the environment
    uv_snapshot!(&filters, context.run()
        .arg("--active")
        .arg("--script")
        .arg("main.py")
        .env(EnvVars::VIRTUAL_ENV, "foo"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello, world!

    ----- stderr -----
    Resolved 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    "###);

    context
        .temp_dir
        .child("foo")
        .assert(predicate::path::is_dir());

    // Requesting a different Python version should invalidate the environment
    uv_snapshot!(&filters, context.run()
        .arg("--active")
        .arg("-p").arg("3.12")
        .arg("--script")
        .arg("main.py")
        .env(EnvVars::VIRTUAL_ENV, "foo"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello, world!

    ----- stderr -----
    Resolved 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    "###);

    Ok(())
}

#[test]
#[cfg(not(windows))]
fn run_gui_script_explicit_stdin_unix() -> Result<()> {
    let context = TestContext::new("3.12");

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
fn run_remote_pep723_script() {
    let context = TestContext::new("3.12").with_filtered_python_names();
    let mut filters = context.filters();
    filters.push((
        r"(?m)^Downloaded remote script to:.*\.py$",
        "Downloaded remote script to: [TEMP_PATH].py",
    ));
    uv_snapshot!(filters, context.run().arg("https://raw.githubusercontent.com/astral-sh/uv/df45b9ac2584824309ff29a6a09421055ad730f6/scripts/uv-run-remote-script-test.py").arg("CI"), @r###"
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
    "###);
}

#[cfg(unix)] // A URL could be a valid filepath on Unix but not on Windows
#[test]
fn run_url_like_with_local_file_priority() -> Result<()> {
    let context = TestContext::new("3.12");

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

    uv_snapshot!(context.filters(), context.run().arg(url), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello, world!

    ----- stderr -----
    "###);

    Ok(())
}

#[test]
fn run_stdin_with_pep723() -> Result<()> {
    let context = TestContext::new("3.12");

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

    uv_snapshot!(context.filters(), context.run().stdin(std::fs::File::open(test_script)?).arg("-"), @r###"
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
fn run_with_env() -> Result<()> {
    let context = TestContext::new("3.12");

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

    uv_snapshot!(context.filters(), context.run().arg("test.py"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    None
    None
    None
    None

    ----- stderr -----
    "###);

    uv_snapshot!(context.filters(), context.run().arg("--env-file").arg(".env").arg("test.py"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    palpatine
    leia_organa
    obi_wan_kenobi
    C3PO

    ----- stderr -----
    "###);

    Ok(())
}

#[test]
fn run_with_env_file() -> Result<()> {
    let context = TestContext::new("3.12");

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

    uv_snapshot!(context.filters(), context.run().arg("--env-file").arg(".file").arg("test.py"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    palpatine
    leia_organa
    obi_wan_kenobi
    C3PO

    ----- stderr -----
    "###);

    Ok(())
}

#[test]
fn run_with_multiple_env_files() -> Result<()> {
    let context = TestContext::new("3.12");

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

    uv_snapshot!(context.filters(), context.run().arg("--env-file").arg(".env1").arg("--env-file").arg(".env2").arg("test.py"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    palpatine
    obi_wan_kenobi
    C3PO

    ----- stderr -----
    "###);

    uv_snapshot!(context.filters(), context.run().arg("test.py").env(EnvVars::UV_ENV_FILE, ".env1 .env2"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    palpatine
    obi_wan_kenobi
    C3PO

    ----- stderr -----
    "###);

    Ok(())
}

#[test]
fn run_with_env_omitted() -> Result<()> {
    let context = TestContext::new("3.12");

    context.temp_dir.child("test.py").write_str(indoc! { "
        import os
        print(os.environ.get('THE_EMPIRE_VARIABLE'))
       "
    })?;

    context.temp_dir.child(".env").write_str(indoc! { "
        THE_EMPIRE_VARIABLE=palpatine
       "
    })?;

    uv_snapshot!(context.filters(), context.run().arg("--env-file").arg(".env").arg("--no-env-file").arg("test.py"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    None

    ----- stderr -----
    "###);

    Ok(())
}

#[test]
fn run_with_malformed_env() -> Result<()> {
    let context = TestContext::new("3.12");

    context.temp_dir.child("test.py").write_str(indoc! { "
        import os
        print(os.environ.get('THE_EMPIRE_VARIABLE'))
       "
    })?;

    context.temp_dir.child(".env").write_str(indoc! { "
        THE_^EMPIRE_VARIABLE=darth_vader
       "
    })?;

    uv_snapshot!(context.filters(), context.run().arg("--env-file").arg(".env").arg("test.py"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    None

    ----- stderr -----
    warning: Failed to parse environment file `.env` at position 4: THE_^EMPIRE_VARIABLE=darth_vader
    "###);

    Ok(())
}

#[test]
fn run_with_not_existing_env_file() -> Result<()> {
    let context = TestContext::new("3.12");

    context.temp_dir.child("test.py").write_str(indoc! { "
        import os
        print(os.environ.get('THE_EMPIRE_VARIABLE'))
       "
    })?;

    let mut filters = context.filters();
    filters.push((
        r"(?m)^error: Failed to read environment file `.env.development`: .*$",
        "error: Failed to read environment file `.env.development`: [ERR]",
    ));

    uv_snapshot!(filters, context.run().arg("--env-file").arg(".env.development").arg("test.py"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No environment file found at: `.env.development`
    "###);

    Ok(())
}

#[test]
fn run_with_extra_conflict() -> Result<()> {
    let context = TestContext::new("3.12");

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
        .arg("import iniconfig"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    "###);

    Ok(())
}

#[test]
fn run_with_group_conflict() -> Result<()> {
    let context = TestContext::new("3.12");

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
        .arg("import iniconfig"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    "###);

    Ok(())
}

/// Test that a signal n makes the process exit with code 128+n.
#[cfg(unix)]
#[test]
fn exit_status_signal() -> Result<()> {
    let context = TestContext::new("3.12");

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
    let context = TestContext::new_with_versions(&["3.13"]);

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
        context.run().arg("--with").arg("typing-extensions").arg("python").arg("-c").arg("import typing_extensions; import iniconfig"), @r###"
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
    "###);

    // Re-running shouldn't require reinstalling `typing-extensions`, since the environment is cached.
    uv_snapshot!(
        context.filters(),
        context.run().arg("--with").arg("typing-extensions").arg("python").arg("-c").arg("import typing_extensions; import iniconfig"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Audited 1 package in [TIME]
    Resolved 1 package in [TIME]
    "###);

    // Re-running as a tool shouldn't require reinstalling `typing-extensions`, since the environment is cached.
    uv_snapshot!(
        context.filters(),
        context.tool_run().arg("--with").arg("typing-extensions").arg("python").arg("-c").arg("import typing_extensions; import iniconfig"), @r###"
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
    "###);

    Ok(())
}

/// See: <https://github.com/astral-sh/uv/issues/11117>
#[test]
fn run_without_overlay() -> Result<()> {
    let context = TestContext::new_with_versions(&["3.13"]);

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
        context.run().arg("--with").arg("typing-extensions").arg("python").arg("-c").arg("import typing_extensions; import iniconfig"), @r###"
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
    "###);

    // Import `iniconfig` in the context of a `tool run` command, which should fail.
    uv_snapshot!(
        context.filters(),
        context.tool_run().arg("--with").arg("typing-extensions").arg("python").arg("-c").arg("import typing_extensions; import iniconfig"), @r###"
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
    "###);

    // Re-running in the context of the project should reset the overlay.
    uv_snapshot!(
        context.filters(),
        context.run().arg("--with").arg("typing-extensions").arg("python").arg("-c").arg("import typing_extensions; import iniconfig"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Audited 1 package in [TIME]
    Resolved 1 package in [TIME]
    "###);

    Ok(())
}

/// See: <https://github.com/astral-sh/uv/issues/11220>
#[cfg(unix)]
#[test]
fn detect_infinite_recursion() -> Result<()> {
    use crate::common::get_bin;
    use indoc::formatdoc;
    use std::os::unix::fs::PermissionsExt;

    let context = TestContext::new("3.12");

    let test_script = context.temp_dir.child("main");
    test_script.write_str(&formatdoc! { r#"
        #!{uv} run

        print("Hello, world!")
    "#, uv = get_bin().display() })?;

    fs_err::set_permissions(test_script.path(), PermissionsExt::from_mode(0o0744))?;

    let mut cmd = std::process::Command::new(test_script.as_os_str());
    context.add_shared_env(&mut cmd, false);

    // Set the max recursion depth to a lower amount to speed up testing.
    cmd.env("UV_RUN_MAX_RECURSION_DEPTH", "5");

    uv_snapshot!(context.filters(), cmd, @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: `uv run` was recursively invoked 6 times which exceeds the limit of 5.

    hint: If you are running a script with `uv run` in the shebang, you may need to include the `--script` flag.
    "###);

    Ok(())
}

#[test]
fn run_uv_variable() {
    let context = TestContext::new("3.12");

    // Display the `UV` variable
    uv_snapshot!(
        context.filters(),
        context.run().arg("python").arg("-c").arg("import os; print(os.environ['UV'])"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [UV]

    ----- stderr -----
    "###);
}

/// Test legacy scripts <https://packaging.python.org/en/latest/guides/distributing-packages-using-setuptools/#scripts>.
///
/// This tests for execution and detection of legacy windows scripts with .bat, .cmd, and .ps1 extensions.
#[cfg(windows)]
#[test]
fn run_windows_legacy_scripts() -> Result<()> {
    let context = TestContext::new("3.12");

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
