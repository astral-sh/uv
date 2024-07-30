#![cfg(all(feature = "python", feature = "pypi"))]

use anyhow::Result;
use assert_cmd::assert::OutputAssertExt;
use assert_fs::{fixture::ChildPath, prelude::*};
use indoc::indoc;

use uv_python::PYTHON_VERSION_FILENAME;

use common::{uv_snapshot, TestContext};

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
    let command_with_args = command
        .arg("--preview")
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
        .arg("--preview")
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
        .arg("--preview")
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
    Prepared 3 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==3.6.0
     + foo==1.0.0 (from file://[TEMP_DIR]/)
     + idna==3.6
     + sniffio==1.3.1
    "###);

    // This time, we target Python 3.8 instead.
    let mut command = context.run();
    let command_with_args = command
        .arg("--preview")
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
    error: The requested Python interpreter (3.8.[X]) is incompatible with the project Python requirement: `>=3.11, <4`
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
    warning: `uv run` is experimental and may change without warning
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
    warning: `uv run` is experimental and may change without warning
    Resolved 1 package in [TIME]
    Audited 1 package in [TIME]
    "###);

    Ok(())
}

/// Run a PEP 723-compatible script. The script should take precedence over the workspace
/// dependencies.
#[test]
fn run_script() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.8"
        dependencies = ["anyio"]
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
    uv_snapshot!(context.filters(), context.run().arg("--preview").arg("main.py"), @r###"
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
    uv_snapshot!(context.filters(), context.run().arg("--preview").arg("main.py"), @r###"
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

    uv_snapshot!(context.filters(), context.run().arg("--preview").arg("main.py"), @r###"
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

    uv_snapshot!(context.filters(), context.run().arg("--preview").arg("main.py"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello, world!

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Audited 4 packages in [TIME]
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
    warning: `uv run` is experimental and may change without warning
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
    warning: `uv run` is experimental and may change without warning
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
    warning: `uv run` is experimental and may change without warning
    Resolved 6 packages in [TIME]
    Audited 4 packages in [TIME]
    "###);

    // Unless the user requests a different version.
    uv_snapshot!(context.filters(), context.run().arg("--with").arg("sniffio<1.3.1").arg("main.py"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv run` is experimental and may change without warning
    Resolved 6 packages in [TIME]
    Audited 4 packages in [TIME]
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + sniffio==1.3.0
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
        "#,
    )?;

    // Running with `--locked` should error, if no lockfile is present.
    uv_snapshot!(context.filters(), context.run().arg("--locked").arg("--").arg("python").arg("--version"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: `uv run` is experimental and may change without warning
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
        "#,
    )?;

    // Running with `--locked` should error.
    uv_snapshot!(context.filters(), context.run().arg("--locked").arg("--").arg("python").arg("--version"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: `uv run` is experimental and may change without warning
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
    warning: `uv run` is experimental and may change without warning
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
        "#,
    )?;

    // Running with `--frozen` should error, if no lockfile is present.
    uv_snapshot!(context.filters(), context.run().arg("--frozen").arg("--").arg("python").arg("--version"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: `uv run` is experimental and may change without warning
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
        "#,
    )?;

    // Running with `--frozen` should install the stale lockfile.
    uv_snapshot!(context.filters(), context.run().arg("--frozen").arg("--").arg("python").arg("--version"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.[X]

    ----- stderr -----
    warning: `uv run` is experimental and may change without warning
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
fn run_empty_requirements_txt() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.8"
        dependencies = ["anyio", "sniffio==1.3.1"]
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
    warning: `uv run` is experimental and may change without warning
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
    warning: `uv run` is experimental and may change without warning
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
    warning: `uv run` is experimental and may change without warning
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
    warning: `uv run` is experimental and may change without warning
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
    warning: `uv run` is experimental and may change without warning
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
    warning: `uv run` is experimental and may change without warning
    Resolved 6 packages in [TIME]
    Audited 4 packages in [TIME]
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
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
    warning: `uv run` is experimental and may change without warning
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
    warning: `uv run` is experimental and may change without warning
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
        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"

        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.8"
        dependencies = []
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
    warning: `uv run` is experimental and may change without warning
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
    let command_with_args = command
        .arg("--preview")
        .arg("--directory")
        .arg("project")
        .arg("main");

    uv_snapshot!(context.filters(), command_with_args, @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    3.12.[X]

    ----- stderr -----
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
        "#
    })?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r"
        import sniffio
       "
    })?;

    uv_snapshot!(context.filters(), context.run().env_remove("UV_SHOW_RESOLUTION").arg("--with").arg("iniconfig").arg("main.py"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv run` is experimental and may change without warning
    "###);

    Ok(())
}
