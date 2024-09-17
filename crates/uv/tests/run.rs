#![cfg(all(feature = "python", feature = "pypi"))]
#![allow(clippy::disallowed_types)]

use anyhow::Result;
use assert_cmd::assert::OutputAssertExt;
use assert_fs::{fixture::ChildPath, prelude::*};
use indoc::indoc;

use uv_python::PYTHON_VERSION_FILENAME;

use common::{copy_dir_all, uv_snapshot, TestContext};

mod common;

/// Run with different python versions, which also depend on different dependency versions.
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
    Using Python 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtualenv at: .venv
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
        .env_remove("VIRTUAL_ENV");

    uv_snapshot!(context.filters(), command_with_args, @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    3.11.[X]
    3.6.0

    ----- stderr -----
    Using Python 3.11.[X] interpreter at: [PYTHON-3.11]
    Removed virtual environment at: .venv
    Creating virtualenv at: .venv
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
        .env_remove("VIRTUAL_ENV");

    uv_snapshot!(context.filters(), command_with_args, @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Using Python 3.8.[X] interpreter at: [PYTHON-3.8]
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
    Reading inline script metadata from: main.py
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
    Reading inline script metadata from: main.py
    Resolved 1 package in [TIME]
    "###);

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
    Reading inline script metadata from: main.py
    "###);

    // Running a script with `--locked` should warn.
    uv_snapshot!(context.filters(), context.run().arg("--locked").arg("main.py"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello, world!

    ----- stderr -----
    Reading inline script metadata from: main.py
    warning: `--locked` is a no-op for Python scripts with inline metadata, which always run in isolation
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
    Reading inline script metadata from: main.py
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
    Reading inline script metadata from: main.py
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
    Reading inline script metadata from: main.py
    Resolved 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    "###);

    Ok(())
}

/// Run a `.pyw` script. The script should be executed with `pythonw.exe`.
#[test]
#[cfg(windows)]
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
    Reading inline script metadata from: main.py
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
    Reading inline script metadata from: main.py
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + uv-public-pypackage==0.1.0 (from git+https://github.com/astral-test/uv-public-pypackage@0dacfd662c64cb4ceb16e6cf65a157a8b715b979)
    "###);

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
fn run_with() -> Result<()> {
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
    uv_snapshot!(context.filters(), context.run().arg("--with").arg("iniconfig").arg("main.py"), @r###"
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
    uv_snapshot!(context.filters(), context.run().arg("--with").arg("sniffio").arg("main.py"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Audited 4 packages in [TIME]
    "###);

    // Unless the user requests a different version.
    uv_snapshot!(context.filters(), context.run().arg("--with").arg("sniffio<1.3.1").arg("main.py"), @r###"
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

    // If the dependencies can't be resolved, we should reference `--with`.
    uv_snapshot!(context.filters(), context.run().arg("--with").arg("add").arg("main.py"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Audited 4 packages in [TIME]
      × No solution found when resolving `--with` dependencies:
      ╰─▶ Because there are no versions of add and you require add, we can conclude that your requirements are unsatisfiable.
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
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#
    })?;

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
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

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
      × Invalid `--with` requirement
      ╰─▶ Distribution not found at: file://[TEMP_DIR]/foo
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

    let existing = fs_err::read_to_string(context.temp_dir.child("uv.lock"))?;

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

    let updated = fs_err::read_to_string(context.temp_dir.child("uv.lock"))?;

    // And the lockfile should be unchanged.
    assert_eq!(existing, updated);

    // Lock the updated requirements.
    context.lock().assert().success();

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
    warning: Requirements file requirements.txt does not contain any dependencies
    "###);

    // Then reused in subsequent invocations
    uv_snapshot!(context.filters(), context.run().arg("--with-requirements").arg(requirements_txt.as_os_str()).arg("main.py"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Audited 4 packages in [TIME]
    warning: Requirements file requirements.txt does not contain any dependencies
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

    // But reject `-` as a requirements file.
    uv_snapshot!(context.filters(), context.run()
        .arg("--with-requirements")
        .arg("-")
        .arg("--with")
        .arg("iniconfig")
        .arg("main.py"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Reading requirements from stdin is not supported in `uv run`
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
    // default 3.11 so that the .python-version is meaningful
    let context = TestContext::new_with_versions(&["3.11", "3.12"]);

    let project_dir = context.temp_dir.child("project");
    project_dir.create_dir_all()?;
    project_dir
        .child(PYTHON_VERSION_FILENAME)
        .write_str("3.12")?;

    let pyproject_toml = project_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.11, <4"
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

    let mut command = context.run();
    let command_with_args = command.arg("--directory").arg("project").arg("main");

    uv_snapshot!(context.filters(), command_with_args, @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    3.12.[X]

    ----- stderr -----
    warning: `VIRTUAL_ENV=[VENV]/` does not match the project environment path `.venv` and will be ignored
    Using Python 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtualenv at: .venv
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
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
    uv_snapshot!(context.filters(), context.run().env_remove("UV_SHOW_RESOLUTION").arg("--with").arg("iniconfig").arg("main.py"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed 4 packages in [TIME]
    Installed 1 package in [TIME]
    "###);

    // Subsequent runs are quiet.
    uv_snapshot!(context.filters(), context.run().env_remove("UV_SHOW_RESOLUTION").arg("--with").arg("iniconfig").arg("main.py"), @r###"
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
    Using Python 3.8.[X] interpreter at: [PYTHON-3.8]
    Creating virtualenv at: .venv
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
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
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
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
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
    Using Python 3.8.[X] interpreter at: [PYTHON-3.8]
    error: The Python request from `.python-version` resolved to Python 3.8.[X], which is incompatible with the project's Python requirement: `>=3.12`
    "###);

    // ...even if `--isolated` is provided.
    uv_snapshot!(context.filters(), context.run().arg("--isolated").arg("main.py"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: The Python request from `.python-version` resolved to Python 3.8.[X], which is incompatible with the project's Python requirement: `>=3.12`
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
    Reading inline script metadata from: script.py
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
      Caused by: `pyproject.toml` is using the `[project]` table, but the required `project.name` field is not set
      Caused by: TOML parse error at line 1, column 2
      |
    1 | [project.urls]
      |  ^^^^^^^
    missing field `name`

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
