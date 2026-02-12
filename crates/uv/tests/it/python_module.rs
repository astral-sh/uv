use assert_cmd::assert::OutputAssertExt;
use assert_fs::prelude::{FileTouch, FileWriteStr, PathChild, PathCreateDir};
use indoc::{formatdoc, indoc};

use uv_fs::Simplified;
use uv_static::EnvVars;

use uv_test::{site_packages_path, uv_snapshot};

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

// Override sys.base_prefix with a path that's guaranteed not to contain
// uv, as otherwise the tests may pick up an already installed uv
// when testing against the system Python install. See #15368.
const TEST_SCRIPT: &str = "
import sys
import uv

sys.base_prefix = '/dev/null'
print(uv.find_uv_bin())
";

#[test]
fn find_uv_bin_venv() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_python_names()
        .with_filtered_virtualenv_bin()
        .with_filtered_exe_suffix()
        .with_filter(user_scheme_bin_filter())
        // Target installs always use "bin" on all platforms. On Windows,
        // `with_filtered_virtualenv_bin` only filters "Scripts", not "bin"
        .with_filter((r"[\\/]bin".to_string(), "/[BIN]".to_string()));

    // Install in a virtual environment
    uv_snapshot!(context.filters(), context.pip_install()
        .arg(context.workspace_root.join("test/packages/fake-uv")), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + uv==0.1.0 (from file://[WORKSPACE]/test/packages/fake-uv)
    "
    );

    // We should find the binary in the virtual environment
    uv_snapshot!(context.filters(), context.python_command()
        .arg("-c")
        .arg(TEST_SCRIPT), @"
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
    let context = uv_test::test_context!("3.12")
        .with_filtered_python_names()
        .with_filtered_virtualenv_bin()
        .with_filtered_exe_suffix()
        .with_filter(user_scheme_bin_filter())
        // Target installs always use "bin" on all platforms. On Windows,
        // `with_filtered_virtualenv_bin` only filters "Scripts", not "bin"
        .with_filter((r"[\\/]bin".to_string(), "/[BIN]".to_string()));

    // Install in a target directory
    uv_snapshot!(context.filters(), context.pip_install()
        .arg(context.workspace_root.join("test/packages/fake-uv"))
        .arg("--target")
        .arg("target"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: .venv/[BIN]/[PYTHON]
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + uv==0.1.0 (from file://[WORKSPACE]/test/packages/fake-uv)
    "
    );

    // We should find the binary in the target directory
    uv_snapshot!(context.filters(), context.python_command()
        .arg("-c")
        .arg(TEST_SCRIPT)
        .env(EnvVars::PYTHONPATH, context.temp_dir.child("target").path()), @"
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
    let context = uv_test::test_context!("3.12")
        .with_filtered_python_names()
        .with_filtered_virtualenv_bin()
        .with_filtered_exe_suffix()
        .with_filter(user_scheme_bin_filter())
        // Target installs always use "bin" on all platforms. On Windows,
        // `with_filtered_virtualenv_bin` only filters "Scripts", not "bin"
        .with_filter((r"[\\/]bin".to_string(), "/[BIN]".to_string()));

    // Install in a prefix directory
    let prefix = context.temp_dir.child("prefix");

    uv_snapshot!(context.filters(), context.pip_install()
        .arg(context.workspace_root.join("test/packages/fake-uv"))
        .arg("--prefix")
        .arg(prefix.path()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: .venv/[BIN]/[PYTHON]
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + uv==0.1.0 (from file://[WORKSPACE]/test/packages/fake-uv)
    "
    );

    // We should find the binary in the prefix directory
    uv_snapshot!(context.filters(), context.python_command()
        .arg("-c")
        .arg(TEST_SCRIPT)
        .env(
            EnvVars::PYTHONPATH,
            site_packages_path(&context.temp_dir.join("prefix"), "python3.12"),
        ), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/prefix/[BIN]/uv

    ----- stderr -----
    "
    );
}

#[test]
fn find_uv_bin_base_prefix() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_python_names()
        .with_filtered_virtualenv_bin()
        .with_filtered_exe_suffix()
        .with_filter(user_scheme_bin_filter())
        // Target installs always use "bin" on all platforms. On Windows,
        // `with_filtered_virtualenv_bin` only filters "Scripts", not "bin"
        .with_filter((r"[\\/]bin".to_string(), "/[BIN]".to_string()));

    // Test base prefix fallback by mutating sys.base_prefix
    // First, create a "base" environment with fake-uv installed
    let base_venv = context.temp_dir.child("base-venv");
    context.venv().arg(base_venv.path()).assert().success();

    // Install fake-uv in the "base" venv
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--python")
        .arg(base_venv.path())
        .arg(context.workspace_root.join("test/packages/fake-uv")), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using Python 3.12.[X] environment at: base-venv
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + uv==0.1.0 (from file://[WORKSPACE]/test/packages/fake-uv)
    "
    );

    context.venv().arg("--clear").assert().success();

    // Mutate `base_prefix` to simulate lookup in a system Python installation
    uv_snapshot!(context.filters(), context.python_command()
        .arg("-c")
        .arg(format!(r#"import sys, uv; sys.base_prefix = "{}"; print(uv.find_uv_bin())"#, base_venv.path().portable_display()))
        .env(EnvVars::PYTHONPATH, site_packages_path(base_venv.path(), "python3.12")), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [TEMP_DIR]/base-venv/[BIN]/uv

    ----- stderr -----
    "
    );
}

#[test]
fn find_uv_bin_in_ephemeral_environment() -> anyhow::Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_python_names()
        .with_filtered_virtualenv_bin()
        .with_filtered_exe_suffix()
        .with_filter(user_scheme_bin_filter())
        // Target installs always use "bin" on all platforms. On Windows,
        // `with_filtered_virtualenv_bin` only filters "Scripts", not "bin"
        .with_filter((r"[\\/]bin".to_string(), "/[BIN]".to_string()));

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
        .arg(context.workspace_root.join("test/packages/fake-uv"))
        .arg("python")
        .arg("-c")
        .arg(TEST_SCRIPT), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [CACHE_DIR]/archive-v0/[HASH]/[BIN]/uv

    ----- stderr -----
    Resolved 1 package in [TIME]
    Audited in [TIME]
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + uv==0.1.0 (from file://[WORKSPACE]/test/packages/fake-uv)
    "
    );

    Ok(())
}

#[test]
fn find_uv_bin_in_parent_of_ephemeral_environment() -> anyhow::Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_python_names()
        .with_filtered_virtualenv_bin()
        .with_filtered_exe_suffix()
        .with_filter(user_scheme_bin_filter())
        // Target installs always use "bin" on all platforms. On Windows,
        // `with_filtered_virtualenv_bin` only filters "Scripts", not "bin"
        .with_filter((r"[\\/]bin".to_string(), "/[BIN]".to_string()));

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
        context.workspace_root.join("test/packages/fake-uv").portable_display()
    })?;

    // When running in an ephemeral environment, we should find the binary in the project
    // environment
    uv_snapshot!(context.filters(), context.run()
        .arg("--with")
        .arg("anyio")
        .arg("python")
        .arg("-c")
        .arg(TEST_SCRIPT),
     @"
    success: true
    exit_code: 0
    ----- stdout -----
    [VENV]/[BIN]/uv

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + uv==0.1.0 (from file://[WORKSPACE]/test/packages/fake-uv)
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + sniffio==1.3.1
    "
    );

    Ok(())
}

#[test]
fn find_uv_bin_user_bin() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_python_names()
        .with_filtered_virtualenv_bin()
        .with_filtered_exe_suffix()
        .with_filter(user_scheme_bin_filter())
        // Target installs always use "bin" on all platforms. On Windows,
        // `with_filtered_virtualenv_bin` only filters "Scripts", not "bin"
        .with_filter((r"[\\/]bin".to_string(), "/[BIN]".to_string()));

    // Add uv to `~/.local/bin`
    let bin = if cfg!(unix) {
        context.home_dir.child(".local").child("bin")
    } else {
        context
            .user_config_dir
            .child("Python")
            .child("Python312")
            .child("Scripts")
    };
    bin.create_dir_all().unwrap();
    bin.child(format!("uv{}", std::env::consts::EXE_SUFFIX))
        .touch()
        .unwrap();

    // Install in a virtual environment
    uv_snapshot!(context.filters(), context.pip_install()
        .arg(context.workspace_root.join("test/packages/fake-uv")), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + uv==0.1.0 (from file://[WORKSPACE]/test/packages/fake-uv)
    "
    );

    // We should find the binary in the virtual environment first
    uv_snapshot!(context.filters(), context.python_command()
        .arg("-c")
        .arg(TEST_SCRIPT), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [VENV]/[BIN]/uv

    ----- stderr -----
    "
    );

    // Remove the virtual environment one for some reason
    fs_err::remove_file(if cfg!(unix) {
        context.venv.child("bin").child("uv")
    } else {
        context.venv.child("Scripts").child("uv.exe")
    })
    .unwrap();

    // We should find the binary in the bin now
    uv_snapshot!(context.filters(), context.python_command()
        .arg("-c")
        .arg(TEST_SCRIPT), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [USER_SCHEME]/[BIN]/uv

    ----- stderr -----
    "
    );
}

#[test]
fn find_uv_bin_error_message() {
    let mut context = uv_test::test_context!("3.12")
        .with_filtered_python_names()
        .with_filtered_virtualenv_bin()
        .with_filtered_exe_suffix()
        .with_filter(user_scheme_bin_filter())
        // Target installs always use "bin" on all platforms. On Windows,
        // `with_filtered_virtualenv_bin` only filters "Scripts", not "bin"
        .with_filter((r"[\\/]bin".to_string(), "/[BIN]".to_string()));

    // Add filters for Python bin directories using with_filtered_path
    // This inserts at the beginning, so these filters are applied first
    let python_info: Vec<_> = context.python_versions.clone();
    for (version, executable) in &python_info {
        let bin_dir = if cfg!(windows) {
            // On Windows, the Python executable is in the root, not the bin directory
            executable
                .canonicalize()
                .unwrap()
                .parent()
                .unwrap()
                .join("Scripts")
        } else {
            executable
                .canonicalize()
                .unwrap()
                .parent()
                .unwrap()
                .to_path_buf()
        };
        context = context.with_filtered_path(&bin_dir, &format!("PYTHON-BIN-{version}"));
    }

    // Install in a virtual environment
    uv_snapshot!(context.filters(), context.pip_install()
        .arg(context.workspace_root.join("test/packages/fake-uv")), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + uv==0.1.0 (from file://[WORKSPACE]/test/packages/fake-uv)
    "
    );

    // Remove the virtual environment executable for some reason
    fs_err::remove_file(if cfg!(unix) {
        context.venv.child("bin").child("uv")
    } else {
        context.venv.child("Scripts").child("uv.exe")
    })
    .unwrap();

    uv_snapshot!(context.filters(), context.python_command()
        .arg("-c")
        .arg(TEST_SCRIPT), @r#"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Traceback (most recent call last):
      File "<string>", line 6, in <module>
      File "[SITE_PACKAGES]/uv/_find_uv.py", line 50, in find_uv_bin
        raise UvNotFound(
    uv._find_uv.UvNotFound: Could not find the uv binary in any of the following locations:
     - [VENV]/[BIN]
     - /dev/null/[BIN]
     - [SITE_PACKAGES]/[BIN]
     - [USER_SCHEME]/[BIN]
    "#
    );
}

#[cfg(feature = "test-python-eol")]
#[test]
fn find_uv_bin_py38() {
    let context = uv_test::test_context!("3.8")
        .with_filtered_python_names()
        .with_filtered_virtualenv_bin()
        .with_filtered_exe_suffix()
        .with_filter(user_scheme_bin_filter())
        // Target installs always use "bin" on all platforms. On Windows,
        // `with_filtered_virtualenv_bin` only filters "Scripts", not "bin"
        .with_filter((r"[\\/]bin".to_string(), "/[BIN]".to_string()));

    // Install in a virtual environment
    uv_snapshot!(context.filters(), context.pip_install()
        .arg(context.workspace_root.join("test/packages/fake-uv")), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + uv==0.1.0 (from file://[WORKSPACE]/test/packages/fake-uv)
    "
    );

    // We should find the binary in the virtual environment
    uv_snapshot!(context.filters(), context.python_command()
        .arg("-c")
        .arg(TEST_SCRIPT), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [VENV]/[BIN]/uv

    ----- stderr -----
    "
    );
}

#[test]
fn find_uv_bin_py39() {
    let context = uv_test::test_context!("3.9")
        .with_filtered_python_names()
        .with_filtered_virtualenv_bin()
        .with_filtered_exe_suffix()
        .with_filter(user_scheme_bin_filter())
        // Target installs always use "bin" on all platforms. On Windows,
        // `with_filtered_virtualenv_bin` only filters "Scripts", not "bin"
        .with_filter((r"[\\/]bin".to_string(), "/[BIN]".to_string()));

    // Install in a virtual environment
    uv_snapshot!(context.filters(), context.pip_install()
        .arg(context.workspace_root.join("test/packages/fake-uv")), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + uv==0.1.0 (from file://[WORKSPACE]/test/packages/fake-uv)
    "
    );

    // We should find the binary in the virtual environment
    uv_snapshot!(context.filters(), context.python_command()
        .arg("-c")
        .arg(TEST_SCRIPT), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [VENV]/[BIN]/uv

    ----- stderr -----
    "
    );
}

#[test]
fn find_uv_bin_py310() {
    let context = uv_test::test_context!("3.10")
        .with_filtered_python_names()
        .with_filtered_virtualenv_bin()
        .with_filtered_exe_suffix()
        .with_filter(user_scheme_bin_filter())
        // Target installs always use "bin" on all platforms. On Windows,
        // `with_filtered_virtualenv_bin` only filters "Scripts", not "bin"
        .with_filter((r"[\\/]bin".to_string(), "/[BIN]".to_string()));

    // Install in a virtual environment
    uv_snapshot!(context.filters(), context.pip_install()
        .arg(context.workspace_root.join("test/packages/fake-uv")), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + uv==0.1.0 (from file://[WORKSPACE]/test/packages/fake-uv)
    "
    );

    // We should find the binary in the virtual environment
    uv_snapshot!(context.filters(), context.python_command()
        .arg("-c")
        .arg(TEST_SCRIPT), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [VENV]/[BIN]/uv

    ----- stderr -----
    "
    );
}

#[test]
fn find_uv_bin_py311() {
    let context = uv_test::test_context!("3.11")
        .with_filtered_python_names()
        .with_filtered_virtualenv_bin()
        .with_filtered_exe_suffix()
        .with_filter(user_scheme_bin_filter())
        // Target installs always use "bin" on all platforms. On Windows,
        // `with_filtered_virtualenv_bin` only filters "Scripts", not "bin"
        .with_filter((r"[\\/]bin".to_string(), "/[BIN]".to_string()));

    // Install in a virtual environment
    uv_snapshot!(context.filters(), context.pip_install()
        .arg(context.workspace_root.join("test/packages/fake-uv")), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + uv==0.1.0 (from file://[WORKSPACE]/test/packages/fake-uv)
    "
    );

    // We should find the binary in the virtual environment
    uv_snapshot!(context.filters(), context.python_command()
        .arg("-c")
        .arg(TEST_SCRIPT), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [VENV]/[BIN]/uv

    ----- stderr -----
    "
    );
}

#[test]
fn find_uv_bin_py312() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_python_names()
        .with_filtered_virtualenv_bin()
        .with_filtered_exe_suffix()
        .with_filter(user_scheme_bin_filter())
        // Target installs always use "bin" on all platforms. On Windows,
        // `with_filtered_virtualenv_bin` only filters "Scripts", not "bin"
        .with_filter((r"[\\/]bin".to_string(), "/[BIN]".to_string()));

    // Install in a virtual environment
    uv_snapshot!(context.filters(), context.pip_install()
        .arg(context.workspace_root.join("test/packages/fake-uv")), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + uv==0.1.0 (from file://[WORKSPACE]/test/packages/fake-uv)
    "
    );

    // We should find the binary in the virtual environment
    uv_snapshot!(context.filters(), context.python_command()
        .arg("-c")
        .arg(TEST_SCRIPT), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [VENV]/[BIN]/uv

    ----- stderr -----
    "
    );
}

#[test]
fn find_uv_bin_py313() {
    let context = uv_test::test_context!("3.13")
        .with_filtered_python_names()
        .with_filtered_virtualenv_bin()
        .with_filtered_exe_suffix()
        .with_filter(user_scheme_bin_filter())
        // Target installs always use "bin" on all platforms. On Windows,
        // `with_filtered_virtualenv_bin` only filters "Scripts", not "bin"
        .with_filter((r"[\\/]bin".to_string(), "/[BIN]".to_string()));

    // Install in a virtual environment
    uv_snapshot!(context.filters(), context.pip_install()
        .arg(context.workspace_root.join("test/packages/fake-uv")), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + uv==0.1.0 (from file://[WORKSPACE]/test/packages/fake-uv)
    "
    );

    // We should find the binary in the virtual environment
    uv_snapshot!(context.filters(), context.python_command()
        .arg("-c")
        .arg(TEST_SCRIPT), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [VENV]/[BIN]/uv

    ----- stderr -----
    "
    );
}

#[test]
fn find_uv_bin_py314() {
    let context = uv_test::test_context!("3.14")
        .with_filtered_python_names()
        .with_filtered_virtualenv_bin()
        .with_filtered_exe_suffix()
        .with_filter(user_scheme_bin_filter())
        // Target installs always use "bin" on all platforms. On Windows,
        // `with_filtered_virtualenv_bin` only filters "Scripts", not "bin"
        .with_filter((r"[\\/]bin".to_string(), "/[BIN]".to_string()));

    // Install in a virtual environment
    uv_snapshot!(context.filters(), context.pip_install()
        .arg(context.workspace_root.join("test/packages/fake-uv")), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + uv==0.1.0 (from file://[WORKSPACE]/test/packages/fake-uv)
    "
    );

    // We should find the binary in the virtual environment
    uv_snapshot!(context.filters(), context.python_command()
        .arg("-c")
        .arg(TEST_SCRIPT), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [VENV]/[BIN]/uv

    ----- stderr -----
    "
    );
}
