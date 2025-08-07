use assert_cmd::assert::OutputAssertExt;
use assert_fs::prelude::{FileWriteStr, PathChild};
use indoc::{formatdoc, indoc};

use uv_fs::Simplified;
use uv_static::EnvVars;

use crate::common::{TestContext, site_packages_path, uv_snapshot};

/// Filter the user scheme, which differs between Windows and Unix.
fn user_scheme_bin_filter() -> (String, String) {
    if cfg!(windows) {
        (
            r"\[USER_CONFIG_DIR\][\\/]Python[\\/]Python\d+".to_string(),
            "[USER_SCHEME]".to_string(),
        )
    } else {
        (r"\[HOME\]/\.local".to_string(), "[USER_SCHEME]".to_string())
    }
}

#[test]
fn find_uv_bin_venv() {
    let context = TestContext::new("3.12")
        .with_filtered_python_names()
        .with_filtered_virtualenv_bin()
        .with_filtered_exe_suffix()
        .with_filter(user_scheme_bin_filter());

    // Install in a virtual environment
    uv_snapshot!(context.filters(), context.pip_install()
        .arg(context.workspace_root.join("scripts/packages/fake-uv")), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + uv==0.1.0 (from file://[WORKSPACE]/scripts/packages/fake-uv)
    "
    );

    // We should find the binary in the virtual environment
    uv_snapshot!(context.filters(), context.python_command()
        .arg("-c")
        .arg("import uv; print(uv.find_uv_bin())"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    [VENV]/[BIN]/uv

    ----- stderr -----
    "
    );
}

#[test]
fn find_uv_bin_target() {
    let context = TestContext::new("3.12")
        .with_filtered_python_names()
        .with_filtered_virtualenv_bin()
        .with_filtered_exe_suffix()
        .with_filter(user_scheme_bin_filter())
        // Target installs always use "bin" on all platforms. On Windows,
        // with_filtered_virtualenv_bin only filters "Scripts", not "bin"
        .with_filter((r"[\\/]bin[\\/]".to_string(), "/[BIN]/".to_string()));

    // Install in a target directory
    uv_snapshot!(context.filters(), context.pip_install()
        .arg(context.workspace_root.join("scripts/packages/fake-uv"))
        .arg("--target")
        .arg("target"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: .venv/[BIN]/[PYTHON]
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + uv==0.1.0 (from file://[WORKSPACE]/scripts/packages/fake-uv)
    "
    );

    // We should find the binary in the target directory
    uv_snapshot!(context.filters(), context.python_command()
        .arg("-c")
        .arg("import uv; print(uv.find_uv_bin())")
        .env(EnvVars::PYTHONPATH, context.temp_dir.child("target").path()), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/target/[BIN]/uv

    ----- stderr -----
    "
    );
}

#[test]
fn find_uv_bin_prefix() {
    let context = TestContext::new("3.12")
        .with_filtered_python_names()
        .with_filtered_virtualenv_bin()
        .with_filtered_exe_suffix()
        .with_filter(user_scheme_bin_filter());

    // Install in a prefix directory
    let prefix = context.temp_dir.child("prefix");

    uv_snapshot!(context.filters(), context.pip_install()
        .arg(context.workspace_root.join("scripts/packages/fake-uv"))
        .arg("--prefix")
        .arg(prefix.path()), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: .venv/[BIN]/[PYTHON]
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + uv==0.1.0 (from file://[WORKSPACE]/scripts/packages/fake-uv)
    "
    );

    // We should find the binary in the prefix directory
    uv_snapshot!(context.filters(), context.python_command()
        .arg("-c")
        .arg("import uv; print(uv.find_uv_bin())")
        .env(
            EnvVars::PYTHONPATH,
            site_packages_path(&context.temp_dir.join("prefix"), "python3.12"),
        ), @r#"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Traceback (most recent call last):
      File "<string>", line 1, in <module>
      File "[TEMP_DIR]/prefix/[PYTHON-LIB]/site-packages/uv/_find_uv.py", line 36, in find_uv_bin
        raise FileNotFoundError(path)
    FileNotFoundError: [USER_SCHEME]/[BIN]/uv
    "#
    );
}

#[test]
fn find_uv_bin_base_prefix() {
    let context = TestContext::new("3.12")
        .with_filtered_python_names()
        .with_filtered_virtualenv_bin()
        .with_filtered_exe_suffix()
        .with_filter(user_scheme_bin_filter());

    // Test base prefix fallback by mutating sys.base_prefix
    // First, create a "base" environment with fake-uv installed
    let base_venv = context.temp_dir.child("base-venv");
    context.venv().arg(base_venv.path()).assert().success();

    // Install fake-uv in the "base" venv
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--python")
        .arg(base_venv.path())
        .arg(context.workspace_root.join("scripts/packages/fake-uv")), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using Python 3.12.[X] environment at: base-venv
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + uv==0.1.0 (from file://[WORKSPACE]/scripts/packages/fake-uv)
    "
    );

    context.venv().assert().success();

    // Mutate `base_prefix` to simulate lookup in a system Python installation
    uv_snapshot!(context.filters(), context.python_command()
        .arg("-c")
        .arg(format!(r#"import sys, uv; sys.base_prefix = "{}"; print(uv.find_uv_bin())"#, base_venv.path().portable_display()))
        .env(EnvVars::PYTHONPATH, site_packages_path(base_venv.path(), "python3.12")), @r#"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Traceback (most recent call last):
      File "<string>", line 1, in <module>
      File "[TEMP_DIR]/base-venv/[PYTHON-LIB]/site-packages/uv/_find_uv.py", line 36, in find_uv_bin
        raise FileNotFoundError(path)
    FileNotFoundError: [USER_SCHEME]/[BIN]/uv
    "#
    );
}

#[test]
fn find_uv_bin_in_ephemeral_environment() -> anyhow::Result<()> {
    let context = TestContext::new("3.12")
        .with_filtered_python_names()
        .with_filtered_virtualenv_bin()
        .with_filtered_exe_suffix()
        .with_filter(user_scheme_bin_filter());

    // Create a minimal pyproject.toml
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "test-project"
        version = "1.0.0"
        requires-python = ">=3.8"
        dependencies = []
        "#
    })?;

    // We should find the binary in an ephemeral `--with` environment
    uv_snapshot!(context.filters(), context.run()
        .arg("--with")
        .arg(context.workspace_root.join("scripts/packages/fake-uv"))
        .arg("python")
        .arg("-c")
        .arg("import uv; print(uv.find_uv_bin())"), @r#"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Audited in [TIME]
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + uv==0.1.0 (from file://[WORKSPACE]/scripts/packages/fake-uv)
    Traceback (most recent call last):
      File "<string>", line 1, in <module>
      File "[CACHE_DIR]/archive-v0/[HASH]/[PYTHON-LIB]/site-packages/uv/_find_uv.py", line 36, in find_uv_bin
        raise FileNotFoundError(path)
    FileNotFoundError: [USER_SCHEME]/[BIN]/uv
    "#
    );

    Ok(())
}

#[test]
fn find_uv_bin_in_parent_of_ephemeral_environment() -> anyhow::Result<()> {
    let context = TestContext::new("3.12")
        .with_filtered_python_names()
        .with_filtered_virtualenv_bin()
        .with_filtered_exe_suffix()
        .with_filter(user_scheme_bin_filter());

    // Add the fake-uv package as a dependency
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(&formatdoc! { r#"
        [project]
        name = "test-project"
        version = "1.0.0"
        requires-python = ">=3.8"
        dependencies = ["uv"]

        [tool.uv.sources]
        uv = {{ path = "{}" }}
        "#,
        context.workspace_root.join("scripts/packages/fake-uv").portable_display()
    })?;

    // When running in an ephemeral environment, we should find the binary in the project
    // environment
    uv_snapshot!(context.filters(), context.run()
        .arg("--with")
        .arg("anyio")
        .arg("python")
        .arg("-c")
        .arg("import uv; print(uv.find_uv_bin())"),
     @r#"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + uv==0.1.0 (from file://[WORKSPACE]/scripts/packages/fake-uv)
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + sniffio==1.3.1
    Traceback (most recent call last):
      File "<string>", line 1, in <module>
      File "[SITE_PACKAGES]/uv/_find_uv.py", line 36, in find_uv_bin
        raise FileNotFoundError(path)
    FileNotFoundError: [USER_SCHEME]/[BIN]/uv
    "#
    );

    Ok(())
}
