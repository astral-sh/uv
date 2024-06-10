#![cfg(all(feature = "python", feature = "pypi"))]

use anyhow::Result;
use assert_fs::prelude::*;
use indoc::indoc;

use crate::common::get_toolchain;
use common::{uv_snapshot, TestContext};

mod common;

/// Run with different python versions, which also depend on different dependency versions.
#[test]
fn run_with_python_version() -> Result<()> {
    let context = TestContext::new("3.12");

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
    Resolved 10 packages in [TIME]
    Downloaded 4 packages in [TIME]
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
        .arg(get_toolchain("3.12"))
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
    Resolved 10 packages in [TIME]
    Audited 4 packages in [TIME]
    "###);

    // This time, we target Python 3.11 instead.
    let mut command = context.run();
    let command_with_args = command
        .arg("--preview")
        .arg("-p")
        .arg(get_toolchain("3.11"))
        .arg("python")
        .arg("-B")
        .arg("main.py");
    let mut filters = context.filters();
    filters.push((
        r"Using Python 3.11.\d+ interpreter at: .*",
        "Using Python 3.11.[X] interpreter at: [PYTHON]",
    ));
    filters.push((r"3.11.\d+", "3.11.[X]"));
    uv_snapshot!(filters, command_with_args, @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    3.11.[X]
    3.6.0

    ----- stderr -----
    Removing virtual environment at: [VENV]/
    Using Python 3.11.[X] interpreter at: [PYTHON]
    Creating virtualenv at: [VENV]/
    Resolved 10 packages in [TIME]
    Downloaded 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==3.6.0
     + foo==1.0.0 (from file://[TEMP_DIR]/)
     + idna==3.6
     + sniffio==1.3.1
    "###);

    Ok(())
}
