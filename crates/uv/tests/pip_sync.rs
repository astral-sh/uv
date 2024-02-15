#![cfg(all(feature = "python", feature = "pypi"))]

use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::Result;
use assert_cmd::prelude::*;
use assert_fs::prelude::*;
use indoc::indoc;
use url::Url;

use common::{
    create_bin_with_executables, create_venv, uv_snapshot, venv_to_interpreter, INSTA_FILTERS,
};
use uv_fs::Normalized;

use crate::common::{get_bin, TestContext};

mod common;

fn check_command(venv: &Path, command: &str, temp_dir: &Path) {
    Command::new(venv_to_interpreter(venv))
        // Our tests change files in <1s, so we must disable CPython bytecode caching or we'll get stale files
        // https://github.com/python/cpython/issues/75953
        .arg("-B")
        .arg("-c")
        .arg(command)
        .current_dir(temp_dir)
        .assert()
        .success();
}

/// Create a `pip sync` command with options shared across scenarios.
fn command(context: &TestContext) -> Command {
    let mut command = Command::new(get_bin());
    command
        .arg("pip")
        .arg("sync")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .env("VIRTUAL_ENV", context.venv.as_os_str())
        .current_dir(&context.temp_dir);
    command
}

/// Create a `pip uninstall` command with options shared across scenarios.
fn uninstall_command(context: &TestContext) -> Command {
    let mut command = Command::new(get_bin());
    command
        .arg("pip")
        .arg("uninstall")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .env("VIRTUAL_ENV", context.venv.as_os_str())
        .current_dir(&context.temp_dir);
    command
}

#[test]
fn missing_pip() {
    uv_snapshot!(Command::new(get_bin()).arg("sync"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: unrecognized subcommand 'sync'

      tip: a similar subcommand exists: 'uv pip sync'

    Usage: uv [OPTIONS] <COMMAND>

    For more information, try '--help'.
    "###);
}

#[test]
fn missing_requirements_txt() {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");

    uv_snapshot!(command(&context)
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: failed to open file `requirements.txt`
      Caused by: No such file or directory (os error 2)
    "###);

    requirements_txt.assert(predicates::path::missing());
}

#[test]
fn missing_venv() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    uv_snapshot!(Command::new(get_bin())
        .arg("pip")
        .arg("sync")
        .arg("requirements.txt")
        .arg("--strict")
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .env("VIRTUAL_ENV", venv.as_os_str())
        .current_dir(&temp_dir), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: failed to open file `requirements.txt`
      Caused by: No such file or directory (os error 2)
    "###);

    venv.assert(predicates::path::missing());

    Ok(())
}

/// Install a package into a virtual environment using the default link semantics. (On macOS,
/// this using `clone` semantics.)
#[test]
fn install() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("MarkupSafe==2.1.3")?;

    uv_snapshot!(command(&context)
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + markupsafe==2.1.3
    "###
    );

    context.assert_command("import markupsafe").success();

    // Removing the cache shouldn't invalidate the virtual environment.
    fs::remove_dir_all(context.cache_dir.path())?;

    context.assert_command("import markupsafe").success();

    Ok(())
}

/// Install a package into a virtual environment using copy semantics.
#[test]
fn install_copy() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("MarkupSafe==2.1.3")?;

    uv_snapshot!(command(&context)
        .arg("requirements.txt")
        .arg("--link-mode")
        .arg("copy")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + markupsafe==2.1.3
    "###
    );

    context.assert_command("import markupsafe").success();

    // Removing the cache shouldn't invalidate the virtual environment.
    fs::remove_dir_all(context.cache_dir.path())?;

    context.assert_command("import markupsafe").success();

    Ok(())
}

/// Install a package into a virtual environment using hardlink semantics.
#[test]
fn install_hardlink() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("MarkupSafe==2.1.3")?;

    uv_snapshot!(command(&context)
        .arg("requirements.txt")
        .arg("--link-mode")
        .arg("hardlink")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + markupsafe==2.1.3
    "###
    );

    context.assert_command("import markupsafe").success();

    // Removing the cache shouldn't invalidate the virtual environment.
    fs::remove_dir_all(context.cache_dir.path())?;

    context.assert_command("import markupsafe").success();

    Ok(())
}

/// Install multiple packages into a virtual environment.
#[test]
fn install_many() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("MarkupSafe==2.1.3\ntomli==2.0.1")?;

    uv_snapshot!(command(&context)
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Downloaded 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + markupsafe==2.1.3
     + tomli==2.0.1
    "###
    );

    context
        .assert_command("import markupsafe; import tomli")
        .success();

    Ok(())
}

/// Attempt to install an already-installed package into a virtual environment.
#[test]
fn noop() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("MarkupSafe==2.1.3")?;

    command(&context)
        .arg("requirements.txt")
        .arg("--strict")
        .assert()
        .success();

    uv_snapshot!(command(&context)
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    "###
    );

    context.assert_command("import markupsafe").success();

    Ok(())
}

/// Install a package into a virtual environment, then install the same package into a different
/// virtual environment.
#[test]
fn link() -> Result<()> {
    let context = TestContext::new("3.12");
    let venv1 = &context.venv;

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("MarkupSafe==2.1.3")?;

    Command::new(get_bin())
        .arg("pip")
        .arg("sync")
        .arg("requirements.txt")
        .arg("--strict")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .env("VIRTUAL_ENV", venv1.as_os_str())
        .current_dir(&context.temp_dir)
        .assert()
        .success();

    let venv2 = context.temp_dir.child(".venv2");
    let bin = create_bin_with_executables(&context.temp_dir, &["3.12"])
        .expect("Failed to create bin dir");
    Command::new(get_bin())
        .arg("venv")
        .arg(venv2.as_os_str())
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .arg("--python")
        .arg("3.12")
        .env("UV_TEST_PYTHON_PATH", bin)
        .current_dir(&context.temp_dir)
        .assert()
        .success();
    venv2.assert(predicates::path::is_dir());

    uv_snapshot!(Command::new(get_bin())
        .arg("pip")
        .arg("sync")
        .arg("requirements.txt")
        .arg("--strict")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .env("VIRTUAL_ENV", venv2.as_os_str())
        .current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed 1 package in [TIME]
     + markupsafe==2.1.3
    "###
    );

    check_command(&venv2, "import markupsafe", &context.temp_dir);

    Ok(())
}

/// Install a package into a virtual environment, then sync the virtual environment with a
/// different requirements file.
#[test]
fn add_remove() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("MarkupSafe==2.1.3")?;

    command(&context)
        .arg("requirements.txt")
        .arg("--strict")
        .assert()
        .success();

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("tomli==2.0.1")?;

    uv_snapshot!(command(&context)
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - markupsafe==2.1.3
     + tomli==2.0.1
    "###
    );

    context.assert_command("import tomli").success();
    context.assert_command("import markupsafe").failure();

    Ok(())
}

/// Install a package into a virtual environment, then install a second package into the same
/// virtual environment.
#[test]
fn install_sequential() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("MarkupSafe==2.1.3")?;

    command(&context)
        .arg("requirements.txt")
        .arg("--strict")
        .assert()
        .success();

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("MarkupSafe==2.1.3\ntomli==2.0.1")?;

    uv_snapshot!(command(&context)
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + tomli==2.0.1
    "###
    );

    context
        .assert_command("import markupsafe; import tomli")
        .success();

    Ok(())
}

/// Install a package into a virtual environment, then install a second package into the same
/// virtual environment.
#[test]
fn upgrade() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("tomli==2.0.0")?;

    command(&context)
        .arg("requirements.txt")
        .arg("--strict")
        .assert()
        .success();

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("tomli==2.0.1")?;

    uv_snapshot!(command(&context)
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - tomli==2.0.0
     + tomli==2.0.1
    "###
    );

    context.assert_command("import tomli").success();

    Ok(())
}

/// Install a package into a virtual environment from a URL.
#[test]
fn install_url() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("werkzeug @ https://files.pythonhosted.org/packages/ff/1d/960bb4017c68674a1cb099534840f18d3def3ce44aed12b5ed8b78e0153e/Werkzeug-2.0.0-py3-none-any.whl")?;

    uv_snapshot!(command(&context)
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + werkzeug==2.0.0 (from https://files.pythonhosted.org/packages/ff/1d/960bb4017c68674a1cb099534840f18d3def3ce44aed12b5ed8b78e0153e/Werkzeug-2.0.0-py3-none-any.whl)
    "###
    );

    context.assert_command("import werkzeug").success();

    Ok(())
}

/// Install a package into a virtual environment from a Git repository.
#[test]
#[cfg(feature = "git")]
fn install_git_commit() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("werkzeug @ git+https://github.com/pallets/werkzeug.git@af160e0b6b7ddd81c22f1652c728ff5ac72d5c74")?;

    uv_snapshot!(command(&context)
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + werkzeug==2.0.0 (from git+https://github.com/pallets/werkzeug.git@af160e0b6b7ddd81c22f1652c728ff5ac72d5c74)
    "###
    );

    context.assert_command("import werkzeug").success();

    Ok(())
}

/// Install a package into a virtual environment from a Git repository.
#[test]
#[cfg(feature = "git")]
fn install_git_tag() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("werkzeug @ git+https://github.com/pallets/werkzeug.git@2.0.0")?;

    uv_snapshot!(command(&context)
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + werkzeug==2.0.0 (from git+https://github.com/pallets/werkzeug.git@2.0.0)
    "###
    );

    context.assert_command("import werkzeug").success();

    Ok(())
}

/// Install two packages from the same Git repository.
#[test]
#[cfg(feature = "git")]
fn install_git_subdirectories() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("example-pkg-a @ git+https://github.com/pypa/sample-namespace-packages.git@df7530eeb8fa0cb7dbb8ecb28363e8e36bfa2f45#subdirectory=pkg_resources/pkg_a\nexample-pkg-b @ git+https://github.com/pypa/sample-namespace-packages.git@df7530eeb8fa0cb7dbb8ecb28363e8e36bfa2f45#subdirectory=pkg_resources/pkg_b")?;

    uv_snapshot!(command(&context)
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Downloaded 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + example-pkg-a==1 (from git+https://github.com/pypa/sample-namespace-packages.git@df7530eeb8fa0cb7dbb8ecb28363e8e36bfa2f45#subdirectory=pkg_resources/pkg_a)
     + example-pkg-b==1 (from git+https://github.com/pypa/sample-namespace-packages.git@df7530eeb8fa0cb7dbb8ecb28363e8e36bfa2f45#subdirectory=pkg_resources/pkg_b)
    "###
    );

    context.assert_command("import example_pkg").success();
    context.assert_command("import example_pkg.a").success();
    context.assert_command("import example_pkg.b").success();

    Ok(())
}

/// Install a source distribution into a virtual environment.
#[test]
fn install_sdist() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("Werkzeug==0.9.6")?;

    uv_snapshot!(command(&context)
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + werkzeug==0.9.6
    "###
    );

    context.assert_command("import werkzeug").success();

    Ok(())
}

/// Install a source distribution into a virtual environment.
#[test]
fn install_sdist_url() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("Werkzeug @ https://files.pythonhosted.org/packages/63/69/5702e5eb897d1a144001e21d676676bcb87b88c0862f947509ea95ea54fc/Werkzeug-0.9.6.tar.gz")?;

    uv_snapshot!(command(&context)
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + werkzeug==0.9.6 (from https://files.pythonhosted.org/packages/63/69/5702e5eb897d1a144001e21d676676bcb87b88c0862f947509ea95ea54fc/Werkzeug-0.9.6.tar.gz)
    "###
    );

    context.assert_command("import werkzeug").success();

    Ok(())
}

/// Attempt to re-install a package into a virtual environment from a URL. The second install
/// should be a no-op.
#[test]
fn install_url_then_install_url() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("werkzeug @ https://files.pythonhosted.org/packages/ff/1d/960bb4017c68674a1cb099534840f18d3def3ce44aed12b5ed8b78e0153e/Werkzeug-2.0.0-py3-none-any.whl")?;

    command(&context)
        .arg("requirements.txt")
        .arg("--strict")
        .assert()
        .success();

    uv_snapshot!(command(&context)
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    "###
    );

    context.assert_command("import werkzeug").success();

    Ok(())
}

/// Install a package via a URL, then via a registry version. The second install _should_ remove the
/// URL-based version, but doesn't right now.
#[test]
fn install_url_then_install_version() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("werkzeug @ https://files.pythonhosted.org/packages/ff/1d/960bb4017c68674a1cb099534840f18d3def3ce44aed12b5ed8b78e0153e/Werkzeug-2.0.0-py3-none-any.whl")?;

    command(&context)
        .arg("requirements.txt")
        .arg("--strict")
        .assert()
        .success();

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("werkzeug==2.0.0")?;

    uv_snapshot!(command(&context)
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    "###
    );

    context.assert_command("import werkzeug").success();

    Ok(())
}

/// Install a package via a registry version, then via a direct URL version. The second install
/// should remove the registry-based version.
#[test]
fn install_version_then_install_url() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("werkzeug==2.0.0")?;

    command(&context)
        .arg("requirements.txt")
        .arg("--strict")
        .assert()
        .success();

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("werkzeug @ https://files.pythonhosted.org/packages/ff/1d/960bb4017c68674a1cb099534840f18d3def3ce44aed12b5ed8b78e0153e/Werkzeug-2.0.0-py3-none-any.whl")?;

    uv_snapshot!(command(&context)
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - werkzeug==2.0.0
     + werkzeug==2.0.0 (from https://files.pythonhosted.org/packages/ff/1d/960bb4017c68674a1cb099534840f18d3def3ce44aed12b5ed8b78e0153e/Werkzeug-2.0.0-py3-none-any.whl)
    "###
    );

    context.assert_command("import werkzeug").success();

    Ok(())
}

/// Test that we select the last 3.8 compatible numpy version instead of trying to compile an
/// incompatible sdist <https://github.com/astral-sh/uv/issues/388>
#[test]
fn install_numpy_py38() -> Result<()> {
    let context = TestContext::new("3.8");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("numpy")?;

    uv_snapshot!(command(&context)
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + numpy==1.24.4
    "###
    );

    context.assert_command("import numpy").success();

    Ok(())
}

/// Attempt to install a package without using a remote index.
#[test]
fn install_no_index() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("MarkupSafe==2.1.3")?;

    uv_snapshot!(command(&context)
        .arg("requirements.txt")
        .arg("--no-index")
        .arg("--strict"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: markupsafe isn't available locally, but making network requests to registries was banned.
    "###
    );

    context.assert_command("import markupsafe").failure();

    Ok(())
}

/// Attempt to install a package without using a remote index
/// after a previous successful installation.
#[test]
fn install_no_index_cached() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("MarkupSafe==2.1.3")?;

    uv_snapshot!(command(&context)
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + markupsafe==2.1.3
    "###
    );

    context.assert_command("import markupsafe").success();

    uninstall_command(&context)
        .arg("markupsafe")
        .assert()
        .success();

    uv_snapshot!(command(&context)
        .arg("requirements.txt")
        .arg("--no-index")
        .arg("--strict"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: markupsafe isn't available locally, but making network requests to registries was banned.
    "###
    );

    context.assert_command("import markupsafe").failure();

    Ok(())
}

#[test]
fn warn_on_yanked_version() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_in = context.temp_dir.child("requirements.txt");
    requirements_in.touch()?;

    // This version is yanked.
    requirements_in.write_str("colorama==0.4.2")?;

    uv_snapshot!(INSTA_FILTERS, windows_filters=false, command(&context)
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    warning: colorama==0.4.2 is yanked (reason: "Bad build, missing files, will not install"). Refresh your lockfile to pin an un-yanked version.
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + colorama==0.4.2
    "###
    );

    Ok(())
}

/// Resolve a local wheel.
#[test]
fn install_local_wheel() -> Result<()> {
    let context = TestContext::new("3.12");

    // Download a wheel.
    let response = reqwest::blocking::get("https://files.pythonhosted.org/packages/97/75/10a9ebee3fd790d20926a90a2547f0bf78f371b2f13aa822c759680ca7b9/tomli-2.0.1-py3-none-any.whl")?;
    let archive = context.temp_dir.child("tomli-2.0.1-py3-none-any.whl");
    let mut archive_file = fs_err::File::create(archive.path())?;
    std::io::copy(&mut response.bytes()?.as_ref(), &mut archive_file)?;

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(&format!(
        "tomli @ {}",
        Url::from_file_path(archive.path()).unwrap()
    ))?;

    // In addition to the standard filters, remove the temporary directory from the snapshot.
    let filters: Vec<_> = [(r"file://.*/", "file://[TEMP_DIR]/")]
        .into_iter()
        .chain(INSTA_FILTERS.to_vec())
        .collect();

    uv_snapshot!(filters, command(&context)
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + tomli==2.0.1 (from file://[TEMP_DIR]/tomli-2.0.1-py3-none-any.whl)
    "###
    );

    context.assert_command("import tomli").success();

    // Create a new virtual environment.
    let venv = create_venv(&context.temp_dir, &context.cache_dir, "3.12");

    // Reinstall. The wheel should come from the cache, so there shouldn't be a "download".
    uv_snapshot!(filters, command(&context)
        .arg("requirements.txt")
        .arg("--strict")
        .env("VIRTUAL_ENV", venv.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed 1 package in [TIME]
     + tomli==2.0.1 (from file://[TEMP_DIR]/tomli-2.0.1-py3-none-any.whl)
    "###
    );

    context.assert_command("import tomli").success();

    // Create a new virtual environment.
    let venv = create_venv(&context.temp_dir, &context.cache_dir, "3.12");

    // "Modify" the wheel.
    // The `filetime` crate works on Windows unlike the std.
    filetime::set_file_mtime(&archive, filetime::FileTime::now()).unwrap();

    // Reinstall. The wheel should be "downloaded" again.
    uv_snapshot!(filters, command(&context)
        .arg("requirements.txt")
        .arg("--strict")
        .env("VIRTUAL_ENV", venv.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + tomli==2.0.1 (from file://[TEMP_DIR]/tomli-2.0.1-py3-none-any.whl)
    "###
    );

    context.assert_command("import tomli").success();

    // "Modify" the wheel.
    filetime::set_file_mtime(&archive, filetime::FileTime::now()).unwrap();

    // Reinstall into the same virtual environment. The wheel should be reinstalled.
    uv_snapshot!(filters, command(&context)
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - tomli==2.0.1 (from file://[TEMP_DIR]/tomli-2.0.1-py3-none-any.whl)
     + tomli==2.0.1 (from file://[TEMP_DIR]/tomli-2.0.1-py3-none-any.whl)
    "###
    );

    context.assert_command("import tomli").success();

    Ok(())
}

/// Install a wheel whose actual version doesn't match the version encoded in the filename.
#[test]
fn mismatched_version() -> Result<()> {
    let context = TestContext::new("3.12");

    // Download a wheel.
    let response = reqwest::blocking::get("https://files.pythonhosted.org/packages/97/75/10a9ebee3fd790d20926a90a2547f0bf78f371b2f13aa822c759680ca7b9/tomli-2.0.1-py3-none-any.whl")?;
    let archive = context.temp_dir.child("tomli-3.7.2-py3-none-any.whl");
    let mut archive_file = fs_err::File::create(archive.path())?;
    std::io::copy(&mut response.bytes()?.as_ref(), &mut archive_file)?;

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(&format!(
        "tomli @ {}",
        Url::from_file_path(archive.path()).unwrap()
    ))?;

    // In addition to the standard filters, remove the temporary directory from the snapshot.
    let filters: Vec<_> = [(r"file://.*/", "file://[TEMP_DIR]/")]
        .into_iter()
        .chain(INSTA_FILTERS.to_vec())
        .collect();

    uv_snapshot!(filters, command(&context)
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    error: Failed to install: tomli-3.7.2-py3-none-any.whl (tomli==3.7.2 (from file://[TEMP_DIR]/tomli-3.7.2-py3-none-any.whl))
      Caused by: Wheel version does not match filename: 2.0.1 != 3.7.2
    "###
    );

    Ok(())
}

/// Install a wheel whose actual name doesn't match the name encoded in the filename.
#[test]
fn mismatched_name() -> Result<()> {
    let context = TestContext::new("3.12");

    // Download a wheel.
    let response = reqwest::blocking::get("https://files.pythonhosted.org/packages/97/75/10a9ebee3fd790d20926a90a2547f0bf78f371b2f13aa822c759680ca7b9/tomli-2.0.1-py3-none-any.whl")?;
    let archive = context.temp_dir.child("foo-2.0.1-py3-none-any.whl");
    let mut archive_file = fs_err::File::create(archive.path())?;
    std::io::copy(&mut response.bytes()?.as_ref(), &mut archive_file)?;

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(&format!(
        "tomli @ {}",
        Url::from_file_path(archive.path()).unwrap()
    ))?;

    // In addition to the standard filters, remove the temporary directory from the snapshot.
    let filters: Vec<_> = [(r"file://.*/", "file://[TEMP_DIR]/")]
        .into_iter()
        .chain(INSTA_FILTERS.to_vec())
        .collect();

    uv_snapshot!(filters, command(&context)
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    error: Failed to install: foo-2.0.1-py3-none-any.whl (foo==2.0.1 (from file://[TEMP_DIR]/foo-2.0.1-py3-none-any.whl))
      Caused by: Wheel package name does not match filename: tomli != foo
    "###
    );

    Ok(())
}

/// Install a local source distribution.
#[test]
fn install_local_source_distribution() -> Result<()> {
    let context = TestContext::new("3.12");

    // Download a source distribution.
    let response = reqwest::blocking::get("https://files.pythonhosted.org/packages/b0/b4/bc2baae3970c282fae6c2cb8e0f179923dceb7eaffb0e76170628f9af97b/wheel-0.42.0.tar.gz")?;
    let archive = context.temp_dir.child("wheel-0.42.0.tar.gz");
    let mut archive_file = fs_err::File::create(archive.path())?;
    std::io::copy(&mut response.bytes()?.as_ref(), &mut archive_file)?;

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(&format!(
        "wheel @ {}",
        Url::from_file_path(archive.path()).unwrap()
    ))?;

    // In addition to the standard filters, remove the temporary directory from the snapshot.
    let filters: Vec<_> = [(r"file://.*/", "file://[TEMP_DIR]/")]
        .into_iter()
        .chain(INSTA_FILTERS.to_vec())
        .collect();

    uv_snapshot!(filters, command(&context)
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + wheel==0.42.0 (from file://[TEMP_DIR]/wheel-0.42.0.tar.gz)
    "###
    );

    context.assert_command("import wheel").success();

    Ok(())
}

/// The `ujson` package includes a `[build-system]`, but no `build-backend`. It lists some explicit
/// build requirements, but _also_ depends on `wheel` and `setuptools`:
/// ```toml
/// [build-system]
/// requires = ["setuptools>=42", "setuptools_scm[toml]>=3.4"]
/// ```
///
/// Like `pip` and `build`, we should use PEP 517 here and respect the `requires`, but use the
/// default build backend.
#[test]
#[cfg(unix)] // https://github.com/astral-sh/uv/issues/1238
fn install_ujson() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("ujson @ https://files.pythonhosted.org/packages/43/1a/b0a027144aa5c8f4ea654f4afdd634578b450807bb70b9f8bad00d6f6d3c/ujson-5.7.0.tar.gz")?;

    uv_snapshot!(command(&context)
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + ujson==5.7.0 (from https://files.pythonhosted.org/packages/43/1a/b0a027144aa5c8f4ea654f4afdd634578b450807bb70b9f8bad00d6f6d3c/ujson-5.7.0.tar.gz)
    "###
    );

    context.assert_command("import ujson").success();

    Ok(())
}

/// This package includes a `[build-system]`, but no `build-backend`.
///
/// It lists some explicit build requirements that are necessary to build the distribution:
/// ```toml
/// [build-system]
/// requires = ["Cython<3", "setuptools", "wheel"]
/// ```
///
/// Like `pip` and `build`, we should use PEP 517 here and respect the `requires`, but use the
/// default build backend.
///
/// The example is based `DTLSSocket==0.1.16`
#[test]
fn install_build_system_no_backend() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("build-system-no-backend @ https://files.pythonhosted.org/packages/ec/25/1e531108ca027dc3a3b37d351f4b86d811df4884c6a81cd99e73b8b589f5/build-system-no-backend-0.1.0.tar.gz")?;

    uv_snapshot!(command(&context)
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + build-system-no-backend==0.1.0 (from https://files.pythonhosted.org/packages/ec/25/1e531108ca027dc3a3b37d351f4b86d811df4884c6a81cd99e73b8b589f5/build-system-no-backend-0.1.0.tar.gz)
    "###
    );

    context
        .assert_command("import build_system_no_backend")
        .success();

    Ok(())
}

/// Check that we show the right messages on cached, direct URL source distribution installs.
#[test]
fn install_url_source_dist_cached() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("tqdm @ https://files.pythonhosted.org/packages/62/06/d5604a70d160f6a6ca5fd2ba25597c24abd5c5ca5f437263d177ac242308/tqdm-4.66.1.tar.gz")?;

    let filters = if cfg!(windows) {
        [("warning: The package `tqdm` requires `colorama ; platform_system == 'Windows'`, but it's not installed.\n", "")]
            .into_iter()
            .chain(INSTA_FILTERS.to_vec())
            .collect()
    } else {
        INSTA_FILTERS.to_vec()
    };
    uv_snapshot!(filters, command(&context)
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + tqdm==4.66.1 (from https://files.pythonhosted.org/packages/62/06/d5604a70d160f6a6ca5fd2ba25597c24abd5c5ca5f437263d177ac242308/tqdm-4.66.1.tar.gz)
    "###
    );

    context.assert_command("import tqdm").success();

    // Re-run the installation in a new virtual environment.
    let parent = assert_fs::TempDir::new()?;
    let venv = create_venv(&parent, &context.cache_dir, "3.12");

    uv_snapshot!(filters, command(&context)
        .arg("requirements.txt")
        .arg("--strict")
        .env("VIRTUAL_ENV", venv.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed 1 package in [TIME]
     + tqdm==4.66.1 (from https://files.pythonhosted.org/packages/62/06/d5604a70d160f6a6ca5fd2ba25597c24abd5c5ca5f437263d177ac242308/tqdm-4.66.1.tar.gz)
    "###
    );

    context.assert_command("import tqdm").success();

    // Clear the cache, then re-run the installation in a new virtual environment.
    let parent = assert_fs::TempDir::new()?;
    let venv = create_venv(&parent, &context.cache_dir, "3.12");

    uv_snapshot!(Command::new(get_bin())
        .arg("clean")
        .arg("tqdm")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .env("VIRTUAL_ENV", venv.as_os_str())
        .current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Removed 126 files for tqdm ([SIZE])
    "###
    );

    uv_snapshot!(filters, command(&context)
        .arg("requirements.txt")
        .arg("--strict")
        .env("VIRTUAL_ENV", venv.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + tqdm==4.66.1 (from https://files.pythonhosted.org/packages/62/06/d5604a70d160f6a6ca5fd2ba25597c24abd5c5ca5f437263d177ac242308/tqdm-4.66.1.tar.gz)
    "###
    );

    context.assert_command("import tqdm").success();

    Ok(())
}

/// Check that we show the right messages on cached, Git source distribution installs.
#[test]
#[cfg(feature = "git")]
fn install_git_source_dist_cached() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("werkzeug @ git+https://github.com/pallets/werkzeug.git@af160e0b6b7ddd81c22f1652c728ff5ac72d5c74")?;

    uv_snapshot!(command(&context)
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + werkzeug==2.0.0 (from git+https://github.com/pallets/werkzeug.git@af160e0b6b7ddd81c22f1652c728ff5ac72d5c74)
    "###
    );

    context.assert_command("import werkzeug").success();

    // Re-run the installation in a new virtual environment.
    let parent = assert_fs::TempDir::new()?;
    let venv = create_venv(&parent, &context.cache_dir, "3.12");

    uv_snapshot!(command(&context)
        .arg("requirements.txt")
        .arg("--strict")
        .env("VIRTUAL_ENV", venv.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed 1 package in [TIME]
     + werkzeug==2.0.0 (from git+https://github.com/pallets/werkzeug.git@af160e0b6b7ddd81c22f1652c728ff5ac72d5c74)
    "###
    );

    check_command(&venv, "import werkzeug", &context.temp_dir);

    // Clear the cache, then re-run the installation in a new virtual environment.
    let parent = assert_fs::TempDir::new()?;
    let venv = create_venv(&parent, &context.cache_dir, "3.12");

    let filters = if cfg!(windows) {
        [("Removed 2 files", "Removed 3 files")]
            .into_iter()
            .chain(INSTA_FILTERS.to_vec())
            .collect()
    } else {
        INSTA_FILTERS.to_vec()
    };
    uv_snapshot!(filters, Command::new(get_bin())
        .arg("clean")
        .arg("werkzeug")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .env("VIRTUAL_ENV", venv.as_os_str())
        .current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Removed 3 files for werkzeug ([SIZE])
    "###
    );

    uv_snapshot!(command(&context)
        .arg("requirements.txt")
        .arg("--strict")
        .env("VIRTUAL_ENV", venv.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + werkzeug==2.0.0 (from git+https://github.com/pallets/werkzeug.git@af160e0b6b7ddd81c22f1652c728ff5ac72d5c74)
    "###
    );

    context.assert_command("import werkzeug").success();

    Ok(())
}

/// Check that we show the right messages on cached, registry source distribution installs.
#[test]
fn install_registry_source_dist_cached() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("future==0.18.3")?;

    uv_snapshot!(command(&context)
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + future==0.18.3
    "###
    );

    context.assert_command("import future").success();

    // Re-run the installation in a new virtual environment.
    let parent = assert_fs::TempDir::new()?;
    let venv = create_venv(&parent, &context.cache_dir, "3.12");

    uv_snapshot!(command(&context)
        .arg("requirements.txt")
        .arg("--strict")
        .env("VIRTUAL_ENV", venv.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed 1 package in [TIME]
     + future==0.18.3
    "###
    );

    context.assert_command("import future").success();

    // Clear the cache, then re-run the installation in a new virtual environment.
    let parent = assert_fs::TempDir::new()?;
    let venv = create_venv(&parent, &context.cache_dir, "3.12");

    let filters = if cfg!(windows) {
        [("Removed 615 files", "Removed 616 files")]
            .into_iter()
            .chain(INSTA_FILTERS.to_vec())
            .collect()
    } else {
        INSTA_FILTERS.to_vec()
    };
    uv_snapshot!(filters, Command::new(get_bin())
        .arg("clean")
        .arg("future")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .env("VIRTUAL_ENV", venv.as_os_str())
        .current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Removed 616 files for future ([SIZE])
    "###
    );

    uv_snapshot!(command(&context)
        .arg("requirements.txt")
        .arg("--strict")
        .env("VIRTUAL_ENV", venv.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + future==0.18.3
    "###
    );

    context.assert_command("import future").success();

    Ok(())
}

/// Check that we show the right messages on cached, local source distribution installs.
#[test]
fn install_path_source_dist_cached() -> Result<()> {
    let context = TestContext::new("3.12");

    // Download a source distribution.
    let response = reqwest::blocking::get("https://files.pythonhosted.org/packages/b0/b4/bc2baae3970c282fae6c2cb8e0f179923dceb7eaffb0e76170628f9af97b/wheel-0.42.0.tar.gz")?;
    let archive = context.temp_dir.child("wheel-0.42.0.tar.gz");
    let mut archive_file = fs_err::File::create(archive.path())?;
    std::io::copy(&mut response.bytes()?.as_ref(), &mut archive_file)?;

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(&format!(
        "wheel @ {}",
        Url::from_file_path(archive.path()).unwrap()
    ))?;

    // In addition to the standard filters, remove the temporary directory from the snapshot.
    let filters: Vec<_> = [(r"file://.*/", "file://[TEMP_DIR]/")]
        .into_iter()
        .chain(INSTA_FILTERS.to_vec())
        .collect();

    uv_snapshot!(filters, command(&context)
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + wheel==0.42.0 (from file://[TEMP_DIR]/wheel-0.42.0.tar.gz)
    "###
    );

    context.assert_command("import wheel").success();

    // Re-run the installation in a new virtual environment.
    let parent = assert_fs::TempDir::new()?;
    let venv = create_venv(&parent, &context.cache_dir, "3.12");

    uv_snapshot!(filters, command(&context)
        .arg("requirements.txt")
        .arg("--strict")
        .env("VIRTUAL_ENV", venv.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed 1 package in [TIME]
     + wheel==0.42.0 (from file://[TEMP_DIR]/wheel-0.42.0.tar.gz)
    "###
    );

    context.assert_command("import wheel").success();

    // Clear the cache, then re-run the installation in a new virtual environment.
    let parent = assert_fs::TempDir::new()?;
    let venv = create_venv(&parent, &context.cache_dir, "3.12");

    let filters2 = if cfg!(windows) {
        [("Removed 3 files", "Removed 4 files")]
            .into_iter()
            .chain(INSTA_FILTERS.to_vec())
            .collect()
    } else {
        INSTA_FILTERS.to_vec()
    };
    uv_snapshot!(filters2, Command::new(get_bin())
        .arg("clean")
        .arg("wheel")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .env("VIRTUAL_ENV", venv.as_os_str())
        .current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Removed 4 files for wheel ([SIZE])
    "###
    );

    uv_snapshot!(filters, command(&context)
        .arg("requirements.txt")
        .arg("--strict")
        .env("VIRTUAL_ENV", venv.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + wheel==0.42.0 (from file://[TEMP_DIR]/wheel-0.42.0.tar.gz)
    "###
    );

    context.assert_command("import wheel").success();

    Ok(())
}

/// Check that we show the right messages on cached, local source distribution installs.
#[test]
fn install_path_built_dist_cached() -> Result<()> {
    let context = TestContext::new("3.12");

    // Download a wheel.
    let response = reqwest::blocking::get("https://files.pythonhosted.org/packages/97/75/10a9ebee3fd790d20926a90a2547f0bf78f371b2f13aa822c759680ca7b9/tomli-2.0.1-py3-none-any.whl")?;
    let archive = context.temp_dir.child("tomli-2.0.1-py3-none-any.whl");
    let mut archive_file = fs_err::File::create(archive.path())?;
    std::io::copy(&mut response.bytes()?.as_ref(), &mut archive_file)?;

    let requirements_txt = context.temp_dir.child("requirements.txt");
    let url = Url::from_file_path(archive.path()).unwrap();
    requirements_txt.write_str(&format!("tomli @ {url}"))?;

    // In addition to the standard filters, remove the temporary directory from the snapshot.
    let url_escaped = regex::escape(url.as_str());
    let filters: Vec<_> = [(
        url_escaped.as_str(),
        "file://[TEMP_DIR]/tomli-2.0.1-py3-none-any.whl",
    )]
    .into_iter()
    .chain(INSTA_FILTERS.to_vec())
    .collect();

    uv_snapshot!(filters, command(&context)
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + tomli==2.0.1 (from file://[TEMP_DIR]/tomli-2.0.1-py3-none-any.whl)
    "###
    );

    context.assert_command("import tomli").success();

    // Re-run the installation in a new virtual environment.
    let parent = assert_fs::TempDir::new()?;
    let venv = create_venv(&context.temp_dir, &context.cache_dir, "3.12");

    uv_snapshot!(filters, command(&context)
        .arg("requirements.txt")
        .arg("--strict")
        .env("VIRTUAL_ENV", venv.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed 1 package in [TIME]
     + tomli==2.0.1 (from file://[TEMP_DIR]/tomli-2.0.1-py3-none-any.whl)
    "###
    );

    check_command(&venv, "import tomli", &parent);

    // Clear the cache, then re-run the installation in a new virtual environment.
    let parent = assert_fs::TempDir::new()?;
    let venv = create_venv(&parent, &context.cache_dir, "3.12");

    let filters2 = if cfg!(windows) {
        [(
            "Removed 1 file for tomli",
            "Removed 1 file for tomli ([SIZE])",
        )]
        .into_iter()
        .chain(INSTA_FILTERS.to_vec())
        .collect()
    } else {
        INSTA_FILTERS.to_vec()
    };
    uv_snapshot!(filters2, Command::new(get_bin())
        .arg("clean")
        .arg("tomli")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .env("VIRTUAL_ENV", venv.as_os_str())
        .current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Removed 1 file for tomli ([SIZE])
    "###
    );

    uv_snapshot!(filters, command(&context)
        .arg("requirements.txt")
        .arg("--strict")
        .env("VIRTUAL_ENV", venv.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + tomli==2.0.1 (from file://[TEMP_DIR]/tomli-2.0.1-py3-none-any.whl)
    "###
    );

    check_command(&venv, "import tomli", &context.temp_dir);

    Ok(())
}

/// Check that we show the right messages on cached, direct URL built distribution installs.
#[test]
fn install_url_built_dist_cached() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("tqdm @ https://files.pythonhosted.org/packages/00/e5/f12a80907d0884e6dff9c16d0c0114d81b8cd07dc3ae54c5e962cc83037e/tqdm-4.66.1-py3-none-any.whl")?;

    let filters = if cfg!(windows) {
        [("warning: The package `tqdm` requires `colorama ; platform_system == 'Windows'`, but it's not installed.\n", "")]
            .into_iter()
            .chain(INSTA_FILTERS.to_vec())
            .collect()
    } else {
        INSTA_FILTERS.to_vec()
    };
    uv_snapshot!(filters, command(&context)
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + tqdm==4.66.1 (from https://files.pythonhosted.org/packages/00/e5/f12a80907d0884e6dff9c16d0c0114d81b8cd07dc3ae54c5e962cc83037e/tqdm-4.66.1-py3-none-any.whl)
    "###
    );

    context.assert_command("import tqdm").success();

    // Re-run the installation in a new virtual environment.
    let parent = assert_fs::TempDir::new()?;
    let venv = create_venv(&parent, &context.cache_dir, "3.12");

    uv_snapshot!(filters, command(&context)
        .arg("requirements.txt")
        .arg("--strict")
        .env("VIRTUAL_ENV", venv.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed 1 package in [TIME]
     + tqdm==4.66.1 (from https://files.pythonhosted.org/packages/00/e5/f12a80907d0884e6dff9c16d0c0114d81b8cd07dc3ae54c5e962cc83037e/tqdm-4.66.1-py3-none-any.whl)
    "###
    );

    check_command(&venv, "import tqdm", &context.temp_dir);

    // Clear the cache, then re-run the installation in a new virtual environment.
    let parent = assert_fs::TempDir::new()?;
    let venv = create_venv(&parent, &context.cache_dir, "3.12");

    uv_snapshot!(Command::new(get_bin())
        .arg("clean")
        .arg("tqdm")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .env("VIRTUAL_ENV", venv.as_os_str())
        .current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Removed 2 files for tqdm ([SIZE])
    "###
    );

    uv_snapshot!(filters, command(&context)
        .arg("requirements.txt")
        .arg("--strict")
        .env("VIRTUAL_ENV", venv.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + tqdm==4.66.1 (from https://files.pythonhosted.org/packages/00/e5/f12a80907d0884e6dff9c16d0c0114d81b8cd07dc3ae54c5e962cc83037e/tqdm-4.66.1-py3-none-any.whl)
    "###
    );

    check_command(&venv, "import tqdm", &context.temp_dir);

    Ok(())
}

/// Verify that fail with an appropriate error when a package is repeated.
#[test]
fn duplicate_package_overlap() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("MarkupSafe==2.1.3\nMarkupSafe==2.1.2")?;

    uv_snapshot!(command(&context)
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to determine installation plan
      Caused by: Detected duplicate package in requirements: markupsafe
    "###
    );

    Ok(())
}

/// Verify that allow duplicate packages when they are disjoint.
#[test]
fn duplicate_package_disjoint() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("MarkupSafe==2.1.3\nMarkupSafe==2.1.2 ; python_version < '3.6'")?;

    uv_snapshot!(command(&context)
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + markupsafe==2.1.3
    "###
    );

    Ok(())
}

/// Verify that we can force reinstall of packages.
#[test]
fn reinstall() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("MarkupSafe==2.1.3\ntomli==2.0.1")?;

    uv_snapshot!(command(&context)
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Downloaded 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + markupsafe==2.1.3
     + tomli==2.0.1
    "###
    );

    context.assert_command("import markupsafe").success();
    context.assert_command("import tomli").success();

    // Re-run the installation with `--reinstall`.
    uv_snapshot!(command(&context)
        .arg("requirements.txt")
        .arg("--reinstall")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Uninstalled 2 packages in [TIME]
    Installed 2 packages in [TIME]
     - markupsafe==2.1.3
     + markupsafe==2.1.3
     - tomli==2.0.1
     + tomli==2.0.1
    "###
    );

    context.assert_command("import markupsafe").success();
    context.assert_command("import tomli").success();

    Ok(())
}

/// Verify that we can force reinstall of selective packages.
#[test]
fn reinstall_package() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("MarkupSafe==2.1.3\ntomli==2.0.1")?;

    uv_snapshot!(command(&context)
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Downloaded 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + markupsafe==2.1.3
     + tomli==2.0.1
    "###
    );

    context.assert_command("import markupsafe").success();
    context.assert_command("import tomli").success();

    // Re-run the installation with `--reinstall`.
    uv_snapshot!(command(&context)
        .arg("requirements.txt")
        .arg("--reinstall-package")
        .arg("tomli")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - tomli==2.0.1
     + tomli==2.0.1
    "###
    );

    context.assert_command("import markupsafe").success();
    context.assert_command("import tomli").success();

    Ok(())
}

/// Verify that we can force reinstall of Git dependencies.
#[test]
#[cfg(feature = "git")]
fn reinstall_git() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("werkzeug @ git+https://github.com/pallets/werkzeug.git@af160e0b6b7ddd81c22f1652c728ff5ac72d5c74")?;

    uv_snapshot!(command(&context)
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + werkzeug==2.0.0 (from git+https://github.com/pallets/werkzeug.git@af160e0b6b7ddd81c22f1652c728ff5ac72d5c74)
    "###
    );

    context.assert_command("import werkzeug").success();

    // Re-run the installation with `--reinstall`.
    uv_snapshot!(command(&context)
        .arg("requirements.txt")
        .arg("--reinstall-package")
        .arg("WerkZeug")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - werkzeug==2.0.0 (from git+https://github.com/pallets/werkzeug.git@af160e0b6b7ddd81c22f1652c728ff5ac72d5c74)
     + werkzeug==2.0.0 (from git+https://github.com/pallets/werkzeug.git@af160e0b6b7ddd81c22f1652c728ff5ac72d5c74)
    "###
    );

    context.assert_command("import werkzeug").success();

    Ok(())
}

/// Verify that we can force refresh of cached data.
#[test]
fn refresh() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("MarkupSafe==2.1.3\ntomli==2.0.1")?;

    uv_snapshot!(command(&context)
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Downloaded 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + markupsafe==2.1.3
     + tomli==2.0.1
    "###
    );

    context.assert_command("import markupsafe").success();
    context.assert_command("import tomli").success();

    // Re-run the installation into with `--refresh`. Ensure that we resolve and download the
    // latest versions of the packages.
    let parent = assert_fs::TempDir::new()?;
    let venv = create_venv(&parent, &context.cache_dir, "3.12");

    uv_snapshot!(command(&context)
        .arg("requirements.txt")
        .arg("--refresh")
        .arg("--strict")
        .env("VIRTUAL_ENV", venv.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Downloaded 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + markupsafe==2.1.3
     + tomli==2.0.1
    "###
    );

    check_command(&venv, "import markupsafe", &context.temp_dir);
    check_command(&venv, "import tomli", &context.temp_dir);

    Ok(())
}

/// Verify that we can force refresh of selective packages.
#[test]
fn refresh_package() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("MarkupSafe==2.1.3\ntomli==2.0.1")?;

    uv_snapshot!(command(&context)
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Downloaded 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + markupsafe==2.1.3
     + tomli==2.0.1
    "###
    );

    context.assert_command("import markupsafe").success();
    context.assert_command("import tomli").success();

    // Re-run the installation into with `--refresh`. Ensure that we resolve and download the
    // latest versions of the packages.
    let parent = assert_fs::TempDir::new()?;
    let venv = create_venv(&parent, &context.cache_dir, "3.12");

    uv_snapshot!(command(&context)
        .arg("requirements.txt")
        .arg("--refresh-package")
        .arg("tomli")
        .arg("--strict")
        .env("VIRTUAL_ENV", venv.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 2 packages in [TIME]
     + markupsafe==2.1.3
     + tomli==2.0.1
    "###
    );

    context.assert_command("import markupsafe").success();
    context.assert_command("import tomli").success();

    Ok(())
}

#[test]
#[cfg(feature = "maturin")]
fn sync_editable() -> Result<()> {
    let context = TestContext::new("3.12");

    let current_dir = std::env::current_dir()?;
    let workspace_url = regex::escape(
        Url::from_directory_path(current_dir.join("..").join("..").canonicalize()?)
            .unwrap()
            .as_str()
            .trim_end_matches(['\\', '/']),
    );

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(&indoc::formatdoc! {r"
        boltons==23.1.1
        -e ../../scripts/editable-installs/maturin_editable
        numpy==1.26.2
            # via poetry-editable
        -e file://{current_dir}/../../scripts/editable-installs/poetry_editable
        ",
        current_dir = current_dir.normalized_display(),
    })?;

    let filter_path = regex::escape(&requirements_txt.normalized_display().to_string());
    let filters = INSTA_FILTERS
        .iter()
        .chain(&[
            (filter_path.as_str(), "requirements.txt"),
            (&workspace_url, "file://[WORKSPACE_DIR]"),
        ])
        .copied()
        .collect::<Vec<_>>();

    // Install the editable packages.
    uv_snapshot!(filters, Command::new(get_bin())
        .arg("pip")
        .arg("sync")
        .arg(requirements_txt.path())
        .arg("--strict")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .env("VIRTUAL_ENV", context.venv.as_os_str())
        .env("CARGO_TARGET_DIR", "../../../target/target_install_editable"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Built 2 editables in [TIME]
    Resolved 2 packages in [TIME]
    Downloaded 2 packages in [TIME]
    Installed 4 packages in [TIME]
     + boltons==23.1.1
     + maturin-editable==0.1.0 (from file://[WORKSPACE_DIR]/scripts/editable-installs/maturin_editable)
     + numpy==1.26.2
     + poetry-editable==0.1.0 (from file://[WORKSPACE_DIR]/scripts/editable-installs/poetry_editable)
    "###
    );

    // Reinstall the editable packages.
    uv_snapshot!(filters, Command::new(get_bin())
        .arg("pip")
        .arg("sync")
        .arg(requirements_txt.path())
        .arg("--reinstall-package")
        .arg("poetry-editable")
        .arg("--strict")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .env("VIRTUAL_ENV", context.venv.as_os_str())
        .env("CARGO_TARGET_DIR", "../../../target/target_install_editable"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Built 1 editable in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - poetry-editable==0.1.0 (from file://[WORKSPACE_DIR]/scripts/editable-installs/poetry_editable)
     + poetry-editable==0.1.0 (from file://[WORKSPACE_DIR]/scripts/editable-installs/poetry_editable)
    "###
    );

    // Make sure we have the right base case.
    let python_source_file =
        "../../scripts/editable-installs/maturin_editable/python/maturin_editable/__init__.py";
    let python_version_1 = indoc::indoc! {r"
        from .maturin_editable import *
        
        version = 1
   "};
    fs_err::write(python_source_file, python_version_1)?;

    let check_installed = indoc::indoc! {r#"
        from maturin_editable import sum_as_string, version

        assert version == 1, version
        assert sum_as_string(1, 2) == "3", sum_as_string(1, 2)
   "#};
    context.assert_command(check_installed).success();

    // Edit the sources.
    let python_version_2 = indoc::indoc! {r"
        from .maturin_editable import *
        
        version = 2
   "};
    fs_err::write(python_source_file, python_version_2)?;

    let check_installed = indoc::indoc! {r#"
        from maturin_editable import sum_as_string, version
        from pathlib import Path

        assert version == 2, version
        assert sum_as_string(1, 2) == "3", sum_as_string(1, 2)
   "#};
    context.assert_command(check_installed).success();

    // Don't create a git diff.
    fs_err::write(python_source_file, python_version_1)?;

    uv_snapshot!(filters, Command::new(get_bin())
        .arg("pip")
        .arg("sync")
        .arg(requirements_txt.path())
        .arg("--strict")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .env("VIRTUAL_ENV", context.venv.as_os_str())
        .env("CARGO_TARGET_DIR", "../../../target/target_install_editable"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 4 packages in [TIME]
    "###
    );

    Ok(())
}

#[test]
fn sync_editable_and_registry() -> Result<()> {
    let context = TestContext::new("3.12");

    let current_dir = std::env::current_dir()?;
    let workspace_url = regex::escape(
        Url::from_directory_path(current_dir.join("..").join("..").canonicalize()?)
            .unwrap()
            .as_str(),
    );

    // Install the registry-based version of Black.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(indoc::indoc! {r"
        black==24.1.0
        "
    })?;

    let filter_path = regex::escape(&requirements_txt.normalized_display().to_string());
    let filters = INSTA_FILTERS
        .iter()
        .chain(&[
            (filter_path.as_str(), "requirements.txt"),
            (workspace_url.as_str(), "file://[WORKSPACE_DIR]/"),
        ])
        .copied()
        .collect::<Vec<_>>();
    uv_snapshot!(filters, Command::new(get_bin())
            .arg("pip")
            .arg("sync")
        .arg(requirements_txt.path())
        .arg("--strict")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .env("VIRTUAL_ENV", context.venv.as_os_str())
        .env("CARGO_TARGET_DIR", "../../../target/target_install_editable"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + black==24.1.0
    warning: The package `black` requires `click >=8.0.0`, but it's not installed.
    warning: The package `black` requires `mypy-extensions >=0.4.3`, but it's not installed.
    warning: The package `black` requires `packaging >=22.0`, but it's not installed.
    warning: The package `black` requires `pathspec >=0.9.0`, but it's not installed.
    warning: The package `black` requires `platformdirs >=2`, but it's not installed.
    "###
    );

    // Install the editable version of Black. This should remove the registry-based version.
    // Use the `file:` syntax for extra coverage.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(indoc::indoc! {r"
        -e file:../../scripts/editable-installs/black_editable
        "
    })?;

    let filter_path = regex::escape(&requirements_txt.normalized_display().to_string());
    let filters = INSTA_FILTERS
        .iter()
        .chain(&[
            (filter_path.as_str(), "requirements.txt"),
            (workspace_url.as_str(), "file://[WORKSPACE_DIR]/"),
        ])
        .copied()
        .collect::<Vec<_>>();
    uv_snapshot!(filters, Command::new(get_bin())
            .arg("pip")
            .arg("sync")
        .arg(requirements_txt.path())
        .arg("--strict")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .env("VIRTUAL_ENV", context.venv.as_os_str())
        .env("CARGO_TARGET_DIR", "../../../target/target_install_editable"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Built 1 editable in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - black==24.1.0
     + black==0.1.0 (from file://[WORKSPACE_DIR]/scripts/editable-installs/black_editable)
    "###
    );

    // Re-install the registry-based version of Black. This should be a no-op, since we have a
    // version of Black installed (the editable version) that satisfies the requirements.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(indoc::indoc! {r"
        black
        "
    })?;

    let filter_path = regex::escape(&requirements_txt.normalized_display().to_string());
    let filters = INSTA_FILTERS
        .iter()
        .chain(&[
            (filter_path.as_str(), "requirements.txt"),
            (workspace_url.as_str(), "file://[WORKSPACE_DIR]/"),
        ])
        .copied()
        .collect::<Vec<_>>();
    uv_snapshot!(filters, Command::new(get_bin())
            .arg("pip")
            .arg("sync")
        .arg(requirements_txt.path())
        .arg("--strict")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .env("VIRTUAL_ENV", context.venv.as_os_str())
        .env("CARGO_TARGET_DIR", "../../../target/target_install_editable"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    "###
    );

    // Re-install Black at a specific version. This should replace the editable version.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(indoc::indoc! {r"
        black==23.10.0
        "
    })?;

    let filter_path = regex::escape(&requirements_txt.normalized_display().to_string());
    let filters = INSTA_FILTERS
        .iter()
        .chain(&[
            (filter_path.as_str(), "requirements.txt"),
            (workspace_url.as_str(), "file://[WORKSPACE_DIR]/"),
        ])
        .copied()
        .collect::<Vec<_>>();
    uv_snapshot!(filters, Command::new(get_bin())
            .arg("pip")
            .arg("sync")
        .arg(requirements_txt.path())
        .arg("--strict")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .env("VIRTUAL_ENV", context.venv.as_os_str())
        .env("CARGO_TARGET_DIR", "../../../target/target_install_editable"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - black==0.1.0 (from file://[WORKSPACE_DIR]/scripts/editable-installs/black_editable)
     + black==23.10.0
    warning: The package `black` requires `click >=8.0.0`, but it's not installed.
    warning: The package `black` requires `mypy-extensions >=0.4.3`, but it's not installed.
    warning: The package `black` requires `packaging >=22.0`, but it's not installed.
    warning: The package `black` requires `pathspec >=0.9.0`, but it's not installed.
    warning: The package `black` requires `platformdirs >=2`, but it's not installed.
    "###
    );

    Ok(())
}

#[test]
fn incompatible_wheel() -> Result<()> {
    let context = TestContext::new("3.12");
    let wheel_dir = assert_fs::TempDir::new()?;

    let wheel = wheel_dir.child("foo-1.2.3-not-compatible-wheel.whl");
    wheel.touch()?;

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(&format!(
        "foo @ {}",
        Url::from_file_path(wheel.path()).unwrap()
    ))?;

    let wheel_dir = regex::escape(
        &wheel_dir
            .path()
            .canonicalize()?
            .normalized_display()
            .to_string(),
    );
    let filters: Vec<_> = [(wheel_dir.as_str(), "[TEMP_DIR]")]
        .into_iter()
        .chain(INSTA_FILTERS.to_vec())
        .collect();

    uv_snapshot!(filters, command(&context)
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to determine installation plan
      Caused by: A path dependency is incompatible with the current platform: [TEMP_DIR]/foo-1.2.3-not-compatible-wheel.whl
    "###
    );

    Ok(())
}

/// Install a project without a `pyproject.toml`, using the PEP 517 build backend (default).
#[test]
fn sync_legacy_sdist_pep_517() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str("flake8 @ https://files.pythonhosted.org/packages/66/53/3ad4a3b74d609b3b9008a10075c40e7c8909eae60af53623c3888f7a529a/flake8-6.0.0.tar.gz")?;

    uv_snapshot!(command(&context)
        .arg("requirements.in"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + flake8==6.0.0 (from https://files.pythonhosted.org/packages/66/53/3ad4a3b74d609b3b9008a10075c40e7c8909eae60af53623c3888f7a529a/flake8-6.0.0.tar.gz)
    "###
    );

    Ok(())
}

/// Install a project without a `pyproject.toml`, using `setuptools` directly.
#[test]
fn sync_legacy_sdist_setuptools() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str("flake8 @ https://files.pythonhosted.org/packages/66/53/3ad4a3b74d609b3b9008a10075c40e7c8909eae60af53623c3888f7a529a/flake8-6.0.0.tar.gz")?;

    uv_snapshot!(command(&context)
        .arg("requirements.in")
        .arg("--legacy-setup-py"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + flake8==6.0.0 (from https://files.pythonhosted.org/packages/66/53/3ad4a3b74d609b3b9008a10075c40e7c8909eae60af53623c3888f7a529a/flake8-6.0.0.tar.gz)
    "###
    );

    Ok(())
}

/// Sync using `--find-links` with a local directory.
#[test]
fn find_links() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(indoc! {r"
        markupsafe==2.1.3
        numpy==1.26.3
        tqdm==1000.0.0
        werkzeug @ https://files.pythonhosted.org/packages/c3/fc/254c3e9b5feb89ff5b9076a23218dafbc99c96ac5941e900b71206e6313b/werkzeug-3.0.1-py3-none-any.whl
    "})?;

    let project_root = fs_err::canonicalize(std::env::current_dir()?.join("../.."))?;
    let project_root_string = regex::escape(&project_root.normalized_display().to_string());
    let filters: Vec<_> = [(project_root_string.as_str(), "[PROJECT_ROOT]")]
        .into_iter()
        .chain(INSTA_FILTERS.to_vec())
        .collect();

    uv_snapshot!(filters, command(&context)
        .arg("requirements.txt")
        .arg("--find-links")
        .arg(project_root.join("scripts/wheels/")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Downloaded 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + markupsafe==2.1.3
     + numpy==1.26.3
     + tqdm==1000.0.0
     + werkzeug==3.0.1 (from https://files.pythonhosted.org/packages/c3/fc/254c3e9b5feb89ff5b9076a23218dafbc99c96ac5941e900b71206e6313b/werkzeug-3.0.1-py3-none-any.whl)
    "###
    );

    Ok(())
}

/// Install without network access via the `--offline` flag.
#[test]
fn offline() -> Result<()> {
    let context = TestContext::new("3.12");
    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str("black==23.10.1")?;

    // Install with `--offline` with an empty cache.
    uv_snapshot!(command(&context)
        .arg("requirements.in")
        .arg("--offline"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Network connectivity is disabled, but the requested data wasn't found in the cache for: `black`
    "###
    );

    // Populate the cache.
    uv_snapshot!(command(&context)
        .arg("requirements.in"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + black==23.10.1
    "###
    );

    // Install with `--offline` with a populated cache.
    let venv = create_venv(&context.temp_dir, &context.cache_dir, "3.12");

    uv_snapshot!(command(&context)
        .arg("requirements.in")
        .arg("--offline")
        .env("VIRTUAL_ENV", venv.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed 1 package in [TIME]
     + black==23.10.1
    "###
    );

    Ok(())
}
