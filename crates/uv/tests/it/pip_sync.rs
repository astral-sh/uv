use std::env::consts::EXE_SUFFIX;
use std::path::Path;
use std::process::Command;

use anyhow::Result;
use assert_cmd::prelude::*;
use assert_fs::fixture::ChildPath;
use assert_fs::prelude::*;
use fs_err as fs;
use indoc::indoc;
use predicates::Predicate;
use url::Url;

use crate::common::{
    download_to_disk, site_packages_path, uv_snapshot, venv_to_interpreter, TestContext,
};
use uv_fs::{copy_dir_all, Simplified};
use uv_static::EnvVars;

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

#[test]
fn missing_requirements_txt() {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: File not found: `requirements.txt`
    "###);

    requirements_txt.assert(predicates::path::missing());
}

#[test]
fn missing_venv() -> Result<()> {
    let context = TestContext::new("3.12")
        .with_filtered_virtualenv_bin()
        .with_filtered_python_names();

    let requirements = context.temp_dir.child("requirements.txt");
    requirements.write_str("anyio")?;
    fs::remove_dir_all(&context.venv)?;

    uv_snapshot!(context.filters(), context.pip_sync().arg("requirements.txt"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to inspect Python interpreter from active virtual environment at `.venv/[BIN]/python`
      Caused by: Python interpreter not found at `[VENV]/[BIN]/python`
    "###);

    assert!(predicates::path::missing().eval(&context.venv));

    // If not "active", we hint to create one
    uv_snapshot!(context.filters(), context.pip_sync().arg("requirements.txt").env_remove("VIRTUAL_ENV"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No virtual environment found; run `uv venv` to create an environment, or pass `--system` to install into a non-virtual environment
    "###);

    assert!(predicates::path::missing().eval(&context.venv));

    Ok(())
}

#[test]
fn missing_system() -> Result<()> {
    let context = TestContext::new_with_versions(&[]);
    let requirements = context.temp_dir.child("requirements.txt");
    requirements.write_str("anyio")?;

    uv_snapshot!(context.filters(), context.pip_sync().arg("requirements.txt").arg("--system"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No system Python installation found
    "###);

    Ok(())
}

/// Install a package into a virtual environment using the default link semantics. (On macOS,
/// this using `clone` semantics.)
#[test]
fn install() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("MarkupSafe==2.1.3")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + markupsafe==2.1.3
    "###
    );

    // Counterpart for the `compile()` test.
    assert!(!context
        .site_packages()
        .join("markupsafe")
        .join("__pycache__")
        .join("__init__.cpython-312.pyc")
        .exists());

    context
        .assert_command("from markupsafe import Markup")
        .success();

    // Removing the cache shouldn't invalidate the virtual environment.
    fs::remove_dir_all(context.cache_dir.path())?;

    context
        .assert_command("from markupsafe import Markup")
        .success();

    Ok(())
}

/// Install a package into a virtual environment using copy semantics.
#[test]
fn install_copy() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("MarkupSafe==2.1.3")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--link-mode")
        .arg("copy")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + markupsafe==2.1.3
    "###
    );

    context
        .assert_command("from markupsafe import Markup")
        .success();

    // Removing the cache shouldn't invalidate the virtual environment.
    fs::remove_dir_all(context.cache_dir.path())?;

    context
        .assert_command("from markupsafe import Markup")
        .success();

    Ok(())
}

/// Install a package into a virtual environment using hardlink semantics.
#[test]
fn install_hardlink() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("MarkupSafe==2.1.3")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--link-mode")
        .arg("hardlink")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + markupsafe==2.1.3
    "###
    );

    context
        .assert_command("from markupsafe import Markup")
        .success();

    // Removing the cache shouldn't invalidate the virtual environment.
    fs::remove_dir_all(context.cache_dir.path())?;

    context
        .assert_command("from markupsafe import Markup")
        .success();

    Ok(())
}

/// Install a package into a virtual environment using symlink semantics.
#[test]
fn install_symlink() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("MarkupSafe==2.1.3")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--link-mode")
        .arg("symlink")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + markupsafe==2.1.3
    "###
    );

    context
        .assert_command("from markupsafe import Markup")
        .success();

    // Removing the cache _should_ invalidate the virtual environment.
    fs::remove_dir_all(context.cache_dir.path())?;

    context
        .assert_command("from markupsafe import Markup")
        .failure();

    Ok(())
}

/// Reject attempts to use symlink semantics with `--no-cache`.
#[test]
fn install_symlink_no_cache() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("MarkupSafe==2.1.3")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--link-mode")
        .arg("symlink")
        .arg("--no-cache")
        .arg("--strict"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    error: Symlink-based installation is not supported with `--no-cache`. The created environment will be rendered unusable by the removal of the cache.
    "###
    );

    Ok(())
}

/// Install multiple packages into a virtual environment.
#[test]
fn install_many() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("MarkupSafe==2.1.3\ntomli==2.0.1")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + markupsafe==2.1.3
     + tomli==2.0.1
    "###
    );

    context
        .assert_command("from markupsafe import Markup; import tomli")
        .success();

    Ok(())
}

/// Attempt to install an already-installed package into a virtual environment.
#[test]
fn noop() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("MarkupSafe==2.1.3")?;

    context
        .pip_sync()
        .arg("requirements.txt")
        .arg("--strict")
        .assert()
        .success();

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Audited 1 package in [TIME]
    "###
    );

    context
        .assert_command("from markupsafe import Markup")
        .success();

    Ok(())
}

/// Attempt to sync an empty set of requirements.
#[test]
fn pip_sync_empty() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: Requirements file `requirements.txt` does not contain any dependencies
    No requirements found (hint: use `--allow-empty-requirements` to clear the environment)
    "###
    );

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--allow-empty-requirements"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: Requirements file `requirements.txt` does not contain any dependencies
    Resolved in [TIME]
    Audited in [TIME]
    "###
    );

    // Install a package.
    requirements_txt.write_str("iniconfig==2.0.0")?;
    context
        .pip_sync()
        .arg("requirements.txt")
        .assert()
        .success();

    // Now, syncing should remove the package.
    requirements_txt.write_str("")?;
    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--allow-empty-requirements"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: Requirements file `requirements.txt` does not contain any dependencies
    Resolved in [TIME]
    Uninstalled 1 package in [TIME]
     - iniconfig==2.0.0
    "###
    );

    Ok(())
}

/// Install a package into a virtual environment, then install the same package into a different
/// virtual environment.
#[test]
fn link() -> Result<()> {
    // Sync `anyio` into the first virtual environment.
    let context1 = TestContext::new("3.12");

    let requirements_txt = context1.temp_dir.child("requirements.txt");
    requirements_txt.write_str("iniconfig==2.0.0")?;

    context1
        .pip_sync()
        .arg(requirements_txt.path())
        .arg("--strict")
        .assert()
        .success();

    // Create a separate virtual environment, but reuse the same cache.
    let context2 = TestContext::new("3.12");
    let mut cmd = context1.pip_sync();
    cmd.env(EnvVars::VIRTUAL_ENV, context2.venv.as_os_str())
        .current_dir(&context2.temp_dir);

    uv_snapshot!(cmd
        .arg(requirements_txt.path())
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    "###
    );

    check_command(&context2.venv, "import iniconfig", &context2.temp_dir);

    Ok(())
}

/// Install a package into a virtual environment, then sync the virtual environment with a
/// different requirements file.
#[test]
fn add_remove() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("iniconfig==2.0.0")?;

    context
        .pip_sync()
        .arg("requirements.txt")
        .arg("--strict")
        .assert()
        .success();

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("tomli==2.0.1")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - iniconfig==2.0.0
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
    requirements_txt.write_str("iniconfig==2.0.0")?;

    context
        .pip_sync()
        .arg("requirements.txt")
        .arg("--strict")
        .assert()
        .success();

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("iniconfig==2.0.0\ntomli==2.0.1")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + tomli==2.0.1
    "###
    );

    context
        .assert_command("import iniconfig; import tomli")
        .success();

    Ok(())
}

/// Install a package into a virtual environment, then install a second package into the same
/// virtual environment.
#[test]
fn upgrade() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("tomli==2.0.0")?;

    context
        .pip_sync()
        .arg("requirements.txt")
        .arg("--strict")
        .assert()
        .success();

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("tomli==2.0.1")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
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
    requirements_txt.write_str("werkzeug @ https://files.pythonhosted.org/packages/ff/1d/960bb4017c68674a1cb099534840f18d3def3ce44aed12b5ed8b78e0153e/Werkzeug-2.0.0-py3-none-any.whl")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
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
    requirements_txt.write_str("uv-public-pypackage @ git+https://github.com/astral-test/uv-public-pypackage@b270df1a2fb5d012294e9aaf05e7e0bab1e6a389")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + uv-public-pypackage==0.1.0 (from git+https://github.com/astral-test/uv-public-pypackage@b270df1a2fb5d012294e9aaf05e7e0bab1e6a389)
    "###
    );

    context
        .assert_command("import uv_public_pypackage")
        .success();

    Ok(())
}

/// Install a package into a virtual environment from a Git repository.
#[test]
#[cfg(feature = "git")]
fn install_git_tag() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(
        "uv-public-pypackage @ git+https://github.com/astral-test/uv-public-pypackage@test-tag",
    )?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + uv-public-pypackage==0.1.0 (from git+https://github.com/astral-test/uv-public-pypackage@0dacfd662c64cb4ceb16e6cf65a157a8b715b979)
    "###
    );

    context
        .assert_command("import uv_public_pypackage")
        .success();

    Ok(())
}

/// Install two packages from the same Git repository.
#[test]
#[cfg(feature = "git")]
fn install_git_subdirectories() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("example-pkg-a @ git+https://github.com/pypa/sample-namespace-packages.git@df7530eeb8fa0cb7dbb8ecb28363e8e36bfa2f45#subdirectory=pkg_resources/pkg_a\nexample-pkg-b @ git+https://github.com/pypa/sample-namespace-packages.git@df7530eeb8fa0cb7dbb8ecb28363e8e36bfa2f45#subdirectory=pkg_resources/pkg_b")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
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
    let context = TestContext::new("3.12").with_exclude_newer("2025-01-29T00:00:00Z");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("source-distribution==0.0.1")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + source-distribution==0.0.1
    "###
    );

    context
        .assert_command("import source_distribution")
        .success();

    Ok(())
}

/// Install a source distribution into a virtual environment.
#[test]
fn install_sdist_url() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("source-distribution @ https://files.pythonhosted.org/packages/10/1f/57aa4cce1b1abf6b433106676e15f9fa2c92ed2bd4cf77c3b50a9e9ac773/source_distribution-0.0.1.tar.gz")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + source-distribution==0.0.1 (from https://files.pythonhosted.org/packages/10/1f/57aa4cce1b1abf6b433106676e15f9fa2c92ed2bd4cf77c3b50a9e9ac773/source_distribution-0.0.1.tar.gz)
    "###
    );

    context
        .assert_command("import source_distribution")
        .success();

    Ok(())
}

/// Install a package with source archive format `.tar.bz2`.
#[test]
fn install_sdist_archive_type_bz2() -> Result<()> {
    let context = TestContext::new("3.8");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(&format!(
        "bz2 @ {}",
        context
            .workspace_root
            .join("scripts/links/bz2-1.0.0.tar.bz2")
            .display()
    ))?;

    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + bz2==1.0.0 (from file://[WORKSPACE]/scripts/links/bz2-1.0.0.tar.bz2)
    "###
    );

    Ok(())
}

/// Attempt to re-install a package into a virtual environment from a URL. The second install
/// should be a no-op.
#[test]
fn install_url_then_install_url() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("werkzeug @ https://files.pythonhosted.org/packages/ff/1d/960bb4017c68674a1cb099534840f18d3def3ce44aed12b5ed8b78e0153e/Werkzeug-2.0.0-py3-none-any.whl")?;

    context
        .pip_sync()
        .arg("requirements.txt")
        .arg("--strict")
        .assert()
        .success();

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
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
    requirements_txt.write_str("werkzeug @ https://files.pythonhosted.org/packages/ff/1d/960bb4017c68674a1cb099534840f18d3def3ce44aed12b5ed8b78e0153e/Werkzeug-2.0.0-py3-none-any.whl")?;

    context
        .pip_sync()
        .arg("requirements.txt")
        .arg("--strict")
        .assert()
        .success();

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("werkzeug==2.0.0")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
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
    requirements_txt.write_str("werkzeug==2.0.0")?;

    context
        .pip_sync()
        .arg("requirements.txt")
        .arg("--strict")
        .assert()
        .success();

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("werkzeug @ https://files.pythonhosted.org/packages/ff/1d/960bb4017c68674a1cb099534840f18d3def3ce44aed12b5ed8b78e0153e/Werkzeug-2.0.0-py3-none-any.whl")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
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
    requirements_txt.write_str("numpy")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
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
    requirements_txt.write_str("iniconfig==2.0.0")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--no-index")
        .arg("--strict"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because iniconfig was not found in the provided package locations and you require iniconfig==2.0.0, we can conclude that your requirements are unsatisfiable.

          hint: Packages were unavailable because index lookups were disabled and no additional package locations were provided (try: `--find-links <uri>`)
    "###
    );

    context.assert_command("import iniconfig").failure();

    Ok(())
}

/// Attempt to install a package without using a remote index
/// after a previous successful installation.
#[test]
fn install_no_index_cached() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("iniconfig==2.0.0")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    "###
    );

    context.assert_command("import iniconfig").success();

    context.pip_uninstall().arg("iniconfig").assert().success();

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--no-index")
        .arg("--strict"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because iniconfig was not found in the provided package locations and you require iniconfig==2.0.0, we can conclude that your requirements are unsatisfiable.

          hint: Packages were unavailable because index lookups were disabled and no additional package locations were provided (try: `--find-links <uri>`)
    "###
    );

    context.assert_command("import iniconfig").failure();

    Ok(())
}

#[test]
fn warn_on_yanked() -> Result<()> {
    let context = TestContext::new("3.12");

    // This version is yanked.
    let requirements_in = context.temp_dir.child("requirements.txt");
    requirements_in.write_str("colorama==0.4.2")?;

    uv_snapshot!(context.filters(), windows_filters=false, context.pip_sync()
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + colorama==0.4.2
    warning: `colorama==0.4.2` is yanked (reason: "Bad build, missing files, will not install")
    "###
    );

    Ok(())
}

#[test]
fn warn_on_yanked_dry_run() -> Result<()> {
    let context = TestContext::new("3.12");

    // This version is yanked.
    let requirements_in = context.temp_dir.child("requirements.txt");
    requirements_in.write_str("colorama==0.4.2")?;

    uv_snapshot!(context.filters(), windows_filters=false, context.pip_sync()
        .arg("requirements.txt")
        .arg("--dry-run")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Would download 1 package
    Would install 1 package
     + colorama==0.4.2
    warning: `colorama==0.4.2` is yanked (reason: "Bad build, missing files, will not install")
    "###
    );

    Ok(())
}

/// Resolve a local wheel.
#[test]
fn install_local_wheel() -> Result<()> {
    let context = TestContext::new("3.12");

    // Download a wheel.
    let archive = context.temp_dir.child("tomli-2.0.1-py3-none-any.whl");
    download_to_disk("https://files.pythonhosted.org/packages/97/75/10a9ebee3fd790d20926a90a2547f0bf78f371b2f13aa822c759680ca7b9/tomli-2.0.1-py3-none-any.whl", &archive);

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(&format!(
        "tomli @ {}",
        Url::from_file_path(archive.path()).unwrap()
    ))?;

    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + tomli==2.0.1 (from file://[TEMP_DIR]/tomli-2.0.1-py3-none-any.whl)
    "###
    );

    context.assert_command("import tomli").success();

    // Create a new virtual environment.
    context.reset_venv();

    // Reinstall. The wheel should come from the cache, so there shouldn't be a "download".
    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.txt")
        .arg("--strict")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Installed 1 package in [TIME]
     + tomli==2.0.1 (from file://[TEMP_DIR]/tomli-2.0.1-py3-none-any.whl)
    "###
    );

    context.assert_command("import tomli").success();

    // Create a new virtual environment.
    context.reset_venv();

    // "Modify" the wheel.
    // The `filetime` crate works on Windows unlike the std.
    filetime::set_file_mtime(&archive, filetime::FileTime::now()).unwrap();

    // Reinstall. The wheel should be "downloaded" again.
    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.txt")
        .arg("--strict")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + tomli==2.0.1 (from file://[TEMP_DIR]/tomli-2.0.1-py3-none-any.whl)
    "###
    );

    context.assert_command("import tomli").success();

    // "Modify" the wheel.
    filetime::set_file_mtime(&archive, filetime::FileTime::now()).unwrap();

    // Reinstall into the same virtual environment. The wheel should be reinstalled.
    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     ~ tomli==2.0.1 (from file://[TEMP_DIR]/tomli-2.0.1-py3-none-any.whl)
    "###
    );

    // Reinstall into the same virtual environment. The wheel should _not_ be reinstalled.
    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Audited 1 package in [TIME]
    "###
    );

    context.assert_command("import tomli").success();

    // Reinstall without the package name.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(&format!("{}", Url::from_file_path(archive.path()).unwrap()))?;

    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Audited 1 package in [TIME]
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
    let archive = context.temp_dir.child("tomli-3.7.2-py3-none-any.whl");
    download_to_disk("https://files.pythonhosted.org/packages/97/75/10a9ebee3fd790d20926a90a2547f0bf78f371b2f13aa822c759680ca7b9/tomli-2.0.1-py3-none-any.whl", &archive);

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(&format!(
        "tomli @ {}",
        Url::from_file_path(archive.path()).unwrap()
    ))?;

    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
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
    let archive = context.temp_dir.child("foo-2.0.1-py3-none-any.whl");
    download_to_disk(
        "https://files.pythonhosted.org/packages/97/75/10a9ebee3fd790d20926a90a2547f0bf78f371b2f13aa822c759680ca7b9/tomli-2.0.1-py3-none-any.whl",
        &archive,
    );

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(&format!(
        "foo @ {}",
        Url::from_file_path(archive.path()).unwrap()
    ))?;

    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because foo has an invalid package format and you require foo, we can conclude that your requirements are unsatisfiable.

          hint: The structure of `foo` was invalid:
            The .dist-info directory tomli-2.0.1 does not start with the normalized package name: foo
    "###
    );

    Ok(())
}

/// Install a local source distribution.
#[test]
fn install_local_source_distribution() -> Result<()> {
    let context = TestContext::new("3.12");

    // Download a source distribution.
    let archive = context.temp_dir.child("wheel-0.42.0.tar.gz");
    download_to_disk(
        "https://files.pythonhosted.org/packages/b0/b4/bc2baae3970c282fae6c2cb8e0f179923dceb7eaffb0e76170628f9af97b/wheel-0.42.0.tar.gz",
        &archive,
    );

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(&format!(
        "wheel @ {}",
        Url::from_file_path(archive.path()).unwrap()
    ))?;

    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + wheel==0.42.0 (from file://[TEMP_DIR]/wheel-0.42.0.tar.gz)
    "###
    );

    context.assert_command("import wheel").success();

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
    requirements_txt.write_str("build-system-no-backend @ https://files.pythonhosted.org/packages/ec/25/1e531108ca027dc3a3b37d351f4b86d811df4884c6a81cd99e73b8b589f5/build-system-no-backend-0.1.0.tar.gz")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
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
    requirements_txt.write_str("source_distribution @ https://files.pythonhosted.org/packages/10/1f/57aa4cce1b1abf6b433106676e15f9fa2c92ed2bd4cf77c3b50a9e9ac773/source_distribution-0.0.1.tar.gz")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + source-distribution==0.0.1 (from https://files.pythonhosted.org/packages/10/1f/57aa4cce1b1abf6b433106676e15f9fa2c92ed2bd4cf77c3b50a9e9ac773/source_distribution-0.0.1.tar.gz)
    "###
    );

    context
        .assert_command("import source_distribution")
        .success();

    // Re-run the installation in a new virtual environment.
    context.reset_venv();

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--strict")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Installed 1 package in [TIME]
     + source-distribution==0.0.1 (from https://files.pythonhosted.org/packages/10/1f/57aa4cce1b1abf6b433106676e15f9fa2c92ed2bd4cf77c3b50a9e9ac773/source_distribution-0.0.1.tar.gz)
    "###
    );

    context
        .assert_command("import source_distribution")
        .success();

    // Clear the cache, then re-run the installation in a new virtual environment.
    context.reset_venv();

    let filters = std::iter::once(("Removed \\d+ files?", "Removed [N] files"))
        .chain(context.filters())
        .collect::<Vec<_>>();
    uv_snapshot!(
        filters,
        context.clean().arg("source_distribution"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Removed [N] files ([SIZE])
    "###
    );

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--strict")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + source-distribution==0.0.1 (from https://files.pythonhosted.org/packages/10/1f/57aa4cce1b1abf6b433106676e15f9fa2c92ed2bd4cf77c3b50a9e9ac773/source_distribution-0.0.1.tar.gz)
    "###
    );

    context
        .assert_command("import source_distribution")
        .success();

    Ok(())
}

/// Check that we show the right messages on cached, Git source distribution installs.
#[test]
#[cfg(feature = "git")]
fn install_git_source_dist_cached() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("uv-public-pypackage @ git+https://github.com/astral-test/uv-public-pypackage@b270df1a2fb5d012294e9aaf05e7e0bab1e6a389")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + uv-public-pypackage==0.1.0 (from git+https://github.com/astral-test/uv-public-pypackage@b270df1a2fb5d012294e9aaf05e7e0bab1e6a389)
    "###
    );

    context
        .assert_command("import uv_public_pypackage")
        .success();

    // Re-run the installation in a new virtual environment.
    context.reset_venv();

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--strict")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Installed 1 package in [TIME]
     + uv-public-pypackage==0.1.0 (from git+https://github.com/astral-test/uv-public-pypackage@b270df1a2fb5d012294e9aaf05e7e0bab1e6a389)
    "###
    );

    context
        .assert_command("import uv_public_pypackage")
        .success();

    // Clear the cache, then re-run the installation in a new virtual environment.
    context.reset_venv();

    let filters = if cfg!(windows) {
        [("Removed 2 files", "Removed 3 files")]
            .into_iter()
            .chain(context.filters())
            .collect()
    } else {
        context.filters()
    };
    uv_snapshot!(filters, context.clean()
        .arg("werkzeug"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    No cache entries found
    "###
    );

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--strict")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Installed 1 package in [TIME]
     + uv-public-pypackage==0.1.0 (from git+https://github.com/astral-test/uv-public-pypackage@b270df1a2fb5d012294e9aaf05e7e0bab1e6a389)
    "###
    );

    context
        .assert_command("import uv_public_pypackage")
        .success();

    Ok(())
}

/// Check that we show the right messages on cached, registry source distribution installs.
#[test]
fn install_registry_source_dist_cached() -> Result<()> {
    let context = TestContext::new("3.12").with_exclude_newer("2025-01-29T00:00:00Z");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("source_distribution==0.0.1")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + source-distribution==0.0.1
    "###
    );

    context
        .assert_command("import source_distribution")
        .success();

    // Re-run the installation in a new virtual environment.
    context.reset_venv();

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--strict")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Installed 1 package in [TIME]
     + source-distribution==0.0.1
    "###
    );

    context
        .assert_command("import source_distribution")
        .success();

    // Clear the cache, then re-run the installation in a new virtual environment.
    context.reset_venv();

    let filters = std::iter::once(("Removed \\d+ files?", "Removed [N] files"))
        .chain(context.filters())
        .collect::<Vec<_>>();
    uv_snapshot!(filters, context.clean()
        .arg("source_distribution"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Removed [N] files ([SIZE])
    "###
    );

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--strict")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + source-distribution==0.0.1
    "###
    );

    context
        .assert_command("import source_distribution")
        .success();

    Ok(())
}

/// Check that we show the right messages on cached, local source distribution installs.
#[test]
fn install_path_source_dist_cached() -> Result<()> {
    let context = TestContext::new("3.12");

    // Download a source distribution.
    let archive = context.temp_dir.child("source_distribution-0.0.1.tar.gz");
    download_to_disk(
        "https://files.pythonhosted.org/packages/10/1f/57aa4cce1b1abf6b433106676e15f9fa2c92ed2bd4cf77c3b50a9e9ac773/source_distribution-0.0.1.tar.gz",
        &archive,
    );

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(&format!(
        "source-distribution @ {}",
        Url::from_file_path(archive.path()).unwrap()
    ))?;

    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + source-distribution==0.0.1 (from file://[TEMP_DIR]/source_distribution-0.0.1.tar.gz)
    "###
    );

    context
        .assert_command("import source_distribution")
        .success();

    // Re-run the installation in a new virtual environment.
    context.reset_venv();

    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.txt")
        .arg("--strict")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Installed 1 package in [TIME]
     + source-distribution==0.0.1 (from file://[TEMP_DIR]/source_distribution-0.0.1.tar.gz)
    "###
    );

    context
        .assert_command("import source_distribution")
        .success();

    // Clear the cache, then re-run the installation in a new virtual environment.
    context.reset_venv();

    let filters = std::iter::once(("Removed \\d+ files?", "Removed [N] files"))
        .chain(context.filters())
        .collect::<Vec<_>>();
    uv_snapshot!(
        filters,
        context.clean().arg("source-distribution"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Removed [N] files ([SIZE])
    "###
    );

    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.txt")
        .arg("--strict")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + source-distribution==0.0.1 (from file://[TEMP_DIR]/source_distribution-0.0.1.tar.gz)
    "###
    );

    context
        .assert_command("import source_distribution")
        .success();

    Ok(())
}

/// Check that we show the right messages on cached, local source distribution installs.
#[test]
fn install_path_built_dist_cached() -> Result<()> {
    let context = TestContext::new("3.12");

    // Download a wheel.
    let archive = context.temp_dir.child("tomli-2.0.1-py3-none-any.whl");
    download_to_disk(
        "https://files.pythonhosted.org/packages/97/75/10a9ebee3fd790d20926a90a2547f0bf78f371b2f13aa822c759680ca7b9/tomli-2.0.1-py3-none-any.whl",
        &archive,
    );

    let requirements_txt = context.temp_dir.child("requirements.txt");
    let url = Url::from_file_path(archive.path()).unwrap();
    requirements_txt.write_str(&format!("tomli @ {url}"))?;

    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + tomli==2.0.1 (from file://[TEMP_DIR]/tomli-2.0.1-py3-none-any.whl)
    "###
    );

    context.assert_command("import tomli").success();

    // Re-run the installation in a new virtual environment.
    context.reset_venv();

    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.txt")
        .arg("--strict")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Installed 1 package in [TIME]
     + tomli==2.0.1 (from file://[TEMP_DIR]/tomli-2.0.1-py3-none-any.whl)
    "###
    );

    context.assert_command("import tomli").success();

    // Clear the cache, then re-run the installation in a new virtual environment.
    context.reset_venv();

    let filters = std::iter::once(("Removed \\d+ files?", "Removed [N] files"))
        .chain(context.filters())
        .collect::<Vec<_>>();
    uv_snapshot!(
        filters,
        context.clean().arg("tomli"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Removed [N] files ([SIZE])
    "###
    );

    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.txt")
        .arg("--strict")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + tomli==2.0.1 (from file://[TEMP_DIR]/tomli-2.0.1-py3-none-any.whl)
    "###
    );

    context.assert_command("import tomli").success();

    Ok(())
}

/// Check that we show the right messages on cached, direct URL built distribution installs.
#[test]
fn install_url_built_dist_cached() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("tqdm @ https://files.pythonhosted.org/packages/00/e5/f12a80907d0884e6dff9c16d0c0114d81b8cd07dc3ae54c5e962cc83037e/tqdm-4.66.1-py3-none-any.whl")?;

    let context_filters = if cfg!(windows) {
        [("warning: The package `tqdm` requires `colorama ; sys_platform == 'win32'`, but it's not installed\n", "")]
            .into_iter()
            .chain(context.filters())
            .collect()
    } else {
        context.filters()
    };
    uv_snapshot!(context_filters, context.pip_sync()
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + tqdm==4.66.1 (from https://files.pythonhosted.org/packages/00/e5/f12a80907d0884e6dff9c16d0c0114d81b8cd07dc3ae54c5e962cc83037e/tqdm-4.66.1-py3-none-any.whl)
    "###
    );

    context.assert_command("import tqdm").success();

    // Re-run the installation in a new virtual environment.
    context.reset_venv();

    uv_snapshot!(context_filters, context.pip_sync()
        .arg("requirements.txt")
        .arg("--strict")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Installed 1 package in [TIME]
     + tqdm==4.66.1 (from https://files.pythonhosted.org/packages/00/e5/f12a80907d0884e6dff9c16d0c0114d81b8cd07dc3ae54c5e962cc83037e/tqdm-4.66.1-py3-none-any.whl)
    "###
    );

    context.assert_command("import tqdm").success();

    // Clear the cache, then re-run the installation in a new virtual environment.
    context.reset_venv();

    let filters = std::iter::once(("Removed \\d+ files?", "Removed [N] files"))
        .chain(context_filters.clone())
        .collect::<Vec<_>>();
    uv_snapshot!(
        filters,
        context.clean().arg("tqdm"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Removed [N] files ([SIZE])
    "###
    );

    uv_snapshot!(context_filters, context.pip_sync()
        .arg("requirements.txt")
        .arg("--strict")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + tqdm==4.66.1 (from https://files.pythonhosted.org/packages/00/e5/f12a80907d0884e6dff9c16d0c0114d81b8cd07dc3ae54c5e962cc83037e/tqdm-4.66.1-py3-none-any.whl)
    "###
    );

    context.assert_command("import tqdm").success();

    Ok(())
}

/// Verify that fail with an appropriate error when a package is repeated.
#[test]
fn duplicate_package_overlap() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("MarkupSafe==2.1.3\nMarkupSafe==2.1.2")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because you require markupsafe==2.1.3 and markupsafe==2.1.2, we can conclude that your requirements are unsatisfiable.
    "###
    );

    Ok(())
}

/// Verify that allow duplicate packages when they are disjoint.
#[test]
fn duplicate_package_disjoint() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("MarkupSafe==2.1.3\nMarkupSafe==2.1.2 ; python_version < '3.6'")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
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
    requirements_txt.write_str("MarkupSafe==2.1.3\ntomli==2.0.1")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + markupsafe==2.1.3
     + tomli==2.0.1
    "###
    );

    context.assert_command("import markupsafe").success();
    context.assert_command("import tomli").success();

    // Re-run the installation with `--reinstall`.
    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--reinstall")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Uninstalled 2 packages in [TIME]
    Installed 2 packages in [TIME]
     ~ markupsafe==2.1.3
     ~ tomli==2.0.1
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
    requirements_txt.write_str("MarkupSafe==2.1.3\ntomli==2.0.1")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + markupsafe==2.1.3
     + tomli==2.0.1
    "###
    );

    context.assert_command("import markupsafe").success();
    context.assert_command("import tomli").success();

    // Re-run the installation with `--reinstall`.
    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--reinstall-package")
        .arg("tomli")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     ~ tomli==2.0.1
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
    requirements_txt.write_str("uv-public-pypackage @ git+https://github.com/astral-test/uv-public-pypackage@b270df1a2fb5d012294e9aaf05e7e0bab1e6a389")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + uv-public-pypackage==0.1.0 (from git+https://github.com/astral-test/uv-public-pypackage@b270df1a2fb5d012294e9aaf05e7e0bab1e6a389)
    "###
    );

    context
        .assert_command("import uv_public_pypackage")
        .success();

    // Re-run the installation with `--reinstall`.
    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--reinstall-package")
        .arg("uv-public-pypackage")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     ~ uv-public-pypackage==0.1.0 (from git+https://github.com/astral-test/uv-public-pypackage@b270df1a2fb5d012294e9aaf05e7e0bab1e6a389)
    "###
    );

    context
        .assert_command("import uv_public_pypackage")
        .success();

    Ok(())
}

/// Verify that we can force refresh of cached data.
#[test]
fn refresh() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("MarkupSafe==2.1.3\ntomli==2.0.1")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + markupsafe==2.1.3
     + tomli==2.0.1
    "###
    );

    context.assert_command("import markupsafe").success();
    context.assert_command("import tomli").success();

    // Re-run the installation into with `--refresh`. Ensure that we resolve and download the
    // latest versions of the packages.
    context.reset_venv();

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--refresh")
        .arg("--strict")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + markupsafe==2.1.3
     + tomli==2.0.1
    "###
    );

    context.assert_command("import markupsafe").success();
    context.assert_command("import tomli").success();

    Ok(())
}

/// Verify that we can force refresh of selective packages.
#[test]
fn refresh_package() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("MarkupSafe==2.1.3\ntomli==2.0.1")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + markupsafe==2.1.3
     + tomli==2.0.1
    "###
    );

    context.assert_command("import markupsafe").success();
    context.assert_command("import tomli").success();

    // Re-run the installation into with `--refresh`. Ensure that we resolve and download the
    // latest versions of the packages.
    context.reset_venv();

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--refresh-package")
        .arg("tomli")
        .arg("--strict")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
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
fn sync_editable() -> Result<()> {
    let context = TestContext::new("3.12");
    let poetry_editable = context.temp_dir.child("poetry_editable");

    // Copy into the temporary directory so we can mutate it.
    copy_dir_all(
        context
            .workspace_root
            .join("scripts/packages/poetry_editable"),
        &poetry_editable,
    )?;

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(&indoc::formatdoc! {r"
        anyio==3.7.0
        -e file://{poetry_editable}
        ",
        poetry_editable = poetry_editable.display()
    })?;

    // Install the editable package.
    uv_snapshot!(context.filters(), context.pip_sync()
        .arg(requirements_txt.path()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + anyio==3.7.0
     + poetry-editable==0.1.0 (from file://[TEMP_DIR]/poetry_editable)
    "###
    );

    // Re-install the editable package. This is a no-op.
    uv_snapshot!(context.filters(), context.pip_sync()
        .arg(requirements_txt.path()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Audited 2 packages in [TIME]
    "###
    );

    // Reinstall the editable package. This won't trigger a rebuild, but it will trigger an install.
    uv_snapshot!(context.filters(), context.pip_sync()
        .arg(requirements_txt.path())
        .arg("--reinstall-package")
        .arg("poetry-editable"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     ~ poetry-editable==0.1.0 (from file://[TEMP_DIR]/poetry_editable)
    "###
    );

    let python_source_file = poetry_editable.path().join("poetry_editable/__init__.py");
    let check_installed = indoc::indoc! {r#"
        from poetry_editable import a

        assert a() == "a", a()
   "#};
    context.assert_command(check_installed).success();

    // Edit the sources and make sure the changes are respected without syncing again.
    let python_version_1 = indoc::indoc! {r"
        version = 1
   "};
    fs_err::write(&python_source_file, python_version_1)?;

    let check_installed = indoc::indoc! {r"
        from poetry_editable import version

        assert version == 1, version
   "};
    context.assert_command(check_installed).success();

    let python_version_2 = indoc::indoc! {r"
        version = 2
   "};
    fs_err::write(&python_source_file, python_version_2)?;

    let check_installed = indoc::indoc! {r"
        from poetry_editable import version

        assert version == 2, version
   "};
    context.assert_command(check_installed).success();

    // Reinstall the editable package. This won't trigger a rebuild or reinstall, since we only
    // detect changes to metadata files (like `pyproject.toml`).
    uv_snapshot!(context.filters(), context.pip_sync()
        .arg(requirements_txt.path()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Audited 2 packages in [TIME]
    "###
    );

    // Modify the `pyproject.toml` file.
    let pyproject_toml = poetry_editable.path().join("pyproject.toml");
    let pyproject_toml_contents = fs_err::read_to_string(&pyproject_toml)?;
    fs_err::write(
        &pyproject_toml,
        pyproject_toml_contents.replace("0.1.0", "0.1.1"),
    )?;

    // Reinstall the editable package. This will trigger a rebuild and reinstall.
    uv_snapshot!(context.filters(), context.pip_sync()
        .arg(requirements_txt.path()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - poetry-editable==0.1.0 (from file://[TEMP_DIR]/poetry_editable)
     + poetry-editable==0.1.1 (from file://[TEMP_DIR]/poetry_editable)
    "###
    );

    // Modify the `pyproject.toml` file.
    let pyproject_toml = poetry_editable.path().join("pyproject.toml");
    let pyproject_toml_contents = fs_err::read_to_string(&pyproject_toml)?;
    fs_err::write(
        &pyproject_toml,
        pyproject_toml_contents.replace("0.1.0", "0.1.1"),
    )?;

    // Reinstall the editable package. This will trigger a rebuild and reinstall.
    uv_snapshot!(context.filters(), context.pip_sync()
        .arg(requirements_txt.path()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     ~ poetry-editable==0.1.1 (from file://[TEMP_DIR]/poetry_editable)
    "###
    );

    Ok(())
}

#[test]
fn sync_editable_and_registry() -> Result<()> {
    let context = TestContext::new("3.12");

    // Copy the black test editable into the "current" directory
    copy_dir_all(
        context
            .workspace_root
            .join("scripts/packages/black_editable"),
        context.temp_dir.join("black_editable"),
    )?;

    // Install the registry-based version of Black.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(indoc::indoc! {r"
        black==24.1.0
        "
    })?;

    uv_snapshot!(context.filters(), context.pip_sync()
        .arg(requirements_txt.path())
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + black==24.1.0
    warning: The package `black` requires `click>=8.0.0`, but it's not installed
    warning: The package `black` requires `mypy-extensions>=0.4.3`, but it's not installed
    warning: The package `black` requires `packaging>=22.0`, but it's not installed
    warning: The package `black` requires `pathspec>=0.9.0`, but it's not installed
    warning: The package `black` requires `platformdirs>=2`, but it's not installed
    "###
    );

    // Install the editable version of Black. This should remove the registry-based version.
    // Use the `file:` syntax for extra coverage.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(indoc::indoc! {r"
        -e file:./black_editable
        "
    })?;

    uv_snapshot!(context.filters(), context.pip_sync()
        .arg(requirements_txt.path()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - black==24.1.0
     + black==0.1.0 (from file://[TEMP_DIR]/black_editable)
    "###
    );

    // Re-install the registry-based version of Black. This should be a no-op, since we have a
    // version of Black installed (the editable version) that satisfies the requirements.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(indoc::indoc! {r"
        black
        "
    })?;

    uv_snapshot!(context.filters(), context.pip_sync()
        .arg(requirements_txt.path()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Audited 1 package in [TIME]
    "###
    );

    // Re-install Black at a specific version. This should replace the editable version.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(indoc::indoc! {r"
        black==23.10.0
        "
    })?;

    uv_snapshot!(context.filters(), context.pip_sync()
        .arg(requirements_txt.path())
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - black==0.1.0 (from file://[TEMP_DIR]/black_editable)
     + black==23.10.0
    warning: The package `black` requires `click>=8.0.0`, but it's not installed
    warning: The package `black` requires `mypy-extensions>=0.4.3`, but it's not installed
    warning: The package `black` requires `packaging>=22.0`, but it's not installed
    warning: The package `black` requires `pathspec>=0.9.0`, but it's not installed
    warning: The package `black` requires `platformdirs>=2`, but it's not installed
    "###
    );

    Ok(())
}

#[test]
fn sync_editable_and_local() -> Result<()> {
    let context = TestContext::new("3.12");

    // Copy the black test editable into the "current" directory
    copy_dir_all(
        context
            .workspace_root
            .join("scripts/packages/black_editable"),
        context.temp_dir.join("black_editable"),
    )?;

    // Install the editable version of Black.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(indoc::indoc! {r"
        -e file:./black_editable
        "
    })?;

    uv_snapshot!(context.filters(), context.pip_sync()
        .arg(requirements_txt.path()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + black==0.1.0 (from file://[TEMP_DIR]/black_editable)
    "###
    );

    // Install the non-editable version of Black. This should replace the editable version.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(indoc::indoc! {r"
        black @ file:./black_editable
        "
    })?;

    uv_snapshot!(context.filters(), context.pip_sync()
        .arg(requirements_txt.path()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     ~ black==0.1.0 (from file://[TEMP_DIR]/black_editable)
    "###
    );

    // Reinstall the editable version of Black. This should replace the non-editable version.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(indoc::indoc! {r"
        -e file:./black_editable
        "
    })?;

    uv_snapshot!(context.filters(), context.pip_sync()
        .arg(requirements_txt.path()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     ~ black==0.1.0 (from file://[TEMP_DIR]/black_editable)
    "###
    );

    Ok(())
}

#[test]
fn incompatible_wheel() -> Result<()> {
    let context = TestContext::new("3.12");
    let wheel = context.temp_dir.child("foo-1.2.3-py3-none-any.whl");
    wheel.touch()?;

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(&format!("foo @ {}", wheel.path().simplified_display()))?;

    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because foo has an invalid package format and you require foo, we can conclude that your requirements are unsatisfiable.

          hint: The structure of `foo` was invalid:
            Failed to read from zip file
    "###
    );

    Ok(())
}

/// Install a project without a `pyproject.toml`, using the PEP 517 build backend.
#[test]
fn sync_legacy_sdist_pep_517() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str("flake8 @ https://files.pythonhosted.org/packages/66/53/3ad4a3b74d609b3b9008a10075c40e7c8909eae60af53623c3888f7a529a/flake8-6.0.0.tar.gz")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.in"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
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

    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.txt")
        .arg("--find-links")
        .arg(context.workspace_root.join("scripts/links/")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + markupsafe==2.1.3
     + numpy==1.26.3
     + tqdm==1000.0.0
     + werkzeug==3.0.1 (from https://files.pythonhosted.org/packages/c3/fc/254c3e9b5feb89ff5b9076a23218dafbc99c96ac5941e900b71206e6313b/werkzeug-3.0.1-py3-none-any.whl)
    "###
    );

    Ok(())
}

/// Sync using `--find-links` with `--no-index`, which should accept the local wheel.
#[test]
fn find_links_no_index_match() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(indoc! {r"
        tqdm==1000.0.0
    "})?;

    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.txt")
        .arg("--no-index")
        .arg("--find-links")
        .arg(context.workspace_root.join("scripts/links/")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + tqdm==1000.0.0
    "###
    );

    Ok(())
}

/// Sync using `--find-links` with `--offline`, which should accept the local wheel.
#[test]
fn find_links_offline_match() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(indoc! {r"
        tqdm==1000.0.0
    "})?;

    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.txt")
        .arg("--offline")
        .arg("--find-links")
        .arg(context.workspace_root.join("scripts/links/")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + tqdm==1000.0.0
    "###
    );

    Ok(())
}

/// Sync using `--find-links` with `--offline`, which should fail to find `numpy`.
#[test]
fn find_links_offline_no_match() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(indoc! {r"
        numpy
        tqdm==1000.0.0
    "})?;

    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.txt")
        .arg("--offline")
        .arg("--find-links")
        .arg(context.workspace_root.join("scripts/links/")), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because numpy was not found in the cache and you require numpy, we can conclude that your requirements are unsatisfiable.

          hint: Packages were unavailable because the network was disabled. When the network is disabled, registry packages may only be read from the cache.
    "###
    );

    Ok(())
}

/// Sync using `--find-links` with a local directory. Ensure that cached wheels are reused.
#[test]
fn find_links_wheel_cache() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(indoc! {r"
        tqdm==1000.0.0
    "})?;

    // Install `tqdm`.
    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.txt")
        .arg("--find-links")
        .arg(context.workspace_root.join("scripts/links/")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + tqdm==1000.0.0
    "###
    );

    // Reinstall `tqdm` with `--reinstall`. Ensure that the wheel is reused.
    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.txt")
        .arg("--reinstall")
        .arg("--find-links")
        .arg(context.workspace_root.join("scripts/links/")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     ~ tqdm==1000.0.0
    "###
    );

    Ok(())
}

/// Sync using `--find-links` with a local directory. Ensure that cached source distributions are
/// reused.
#[test]
fn find_links_source_cache() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(indoc! {r"
        tqdm==999.0.0
    "})?;

    // Install `tqdm`.
    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.txt")
        .arg("--find-links")
        .arg(context.workspace_root.join("scripts/links/")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + tqdm==999.0.0
    "###
    );

    // Reinstall `tqdm` with `--reinstall`. Ensure that the wheel is reused.
    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.txt")
        .arg("--reinstall")
        .arg("--find-links")
        .arg(context.workspace_root.join("scripts/links/")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     ~ tqdm==999.0.0
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
    uv_snapshot!(context.pip_sync()
        .arg("requirements.in")
        .arg("--offline"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because black was not found in the cache and you require black==23.10.1, we can conclude that your requirements are unsatisfiable.

          hint: Packages were unavailable because the network was disabled. When the network is disabled, registry packages may only be read from the cache.
    "###
    );

    // Populate the cache.
    uv_snapshot!(context.pip_sync()
        .arg("requirements.in"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + black==23.10.1
    "###
    );

    // Install with `--offline` with a populated cache.
    context.reset_venv();

    uv_snapshot!(context.pip_sync()
        .arg("requirements.in")
        .arg("--offline")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Installed 1 package in [TIME]
     + black==23.10.1
    "###
    );

    Ok(())
}

/// Include a `constraints.txt` file with a compatible constraint.
#[test]
fn compatible_constraint() -> Result<()> {
    let context = TestContext::new("3.12");
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("anyio==3.7.0")?;

    let constraints_txt = context.temp_dir.child("constraints.txt");
    constraints_txt.write_str("anyio==3.7.0")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--constraint")
        .arg("constraints.txt"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + anyio==3.7.0
    "###
    );

    Ok(())
}

/// Include a `constraints.txt` file with an incompatible constraint.
#[test]
fn incompatible_constraint() -> Result<()> {
    let context = TestContext::new("3.12");
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("anyio==3.7.0")?;

    let constraints_txt = context.temp_dir.child("constraints.txt");
    constraints_txt.write_str("anyio==3.6.0")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--constraint")
        .arg("constraints.txt"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because you require anyio==3.7.0 and anyio==3.6.0, we can conclude that your requirements are unsatisfiable.
    "###
    );

    Ok(())
}

/// Include a `constraints.txt` file with an irrelevant constraint.
#[test]
fn irrelevant_constraint() -> Result<()> {
    let context = TestContext::new("3.12");
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("anyio==3.7.0")?;

    let constraints_txt = context.temp_dir.child("constraints.txt");
    constraints_txt.write_str("black==23.10.1")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--constraint")
        .arg("constraints.txt"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + anyio==3.7.0
    "###
    );

    Ok(())
}

/// Sync with a repeated `anyio` requirement.
#[test]
fn repeat_requirement_identical() -> Result<()> {
    let context = TestContext::new("3.12");
    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str("anyio\nanyio")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.in"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + anyio==4.3.0
    "###);

    Ok(())
}

/// Sync with a repeated `anyio` requirement, with compatible versions.
#[test]
fn repeat_requirement_compatible() -> Result<()> {
    let context = TestContext::new("3.12");
    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str("anyio\nanyio==4.0.0")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.in"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + anyio==4.0.0
    "###);

    Ok(())
}

/// Sync with a repeated, but conflicting `anyio` requirement.
#[test]
fn repeat_requirement_incompatible() -> Result<()> {
    let context = TestContext::new("3.12");
    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str("anyio<4.0.0\nanyio==4.0.0")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.in"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because you require anyio<4.0.0 and anyio==4.0.0, we can conclude that your requirements are unsatisfiable.
    "###);

    Ok(())
}

/// Don't preserve the mtime from .tar.gz files, it may be the unix epoch (1970-01-01), while Python's zip
/// implementation can't handle files with an mtime older than 1980.
/// See also <https://github.com/alexcrichton/tar-rs/issues/349>.
#[test]
fn tar_dont_preserve_mtime() -> Result<()> {
    let context = TestContext::new("3.12");
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("tomli @ https://files.pythonhosted.org/packages/c0/3f/d7af728f075fb08564c5949a9c95e44352e23dee646869fa104a3b2060a3/tomli-2.0.1.tar.gz")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + tomli==2.0.1 (from https://files.pythonhosted.org/packages/c0/3f/d7af728f075fb08564c5949a9c95e44352e23dee646869fa104a3b2060a3/tomli-2.0.1.tar.gz)
    "###);

    Ok(())
}

/// Avoid creating a file with 000 permissions
#[test]
fn set_read_permissions() -> Result<()> {
    let context = TestContext::new("3.12");
    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str("databricks==0.2")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.in"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + databricks==0.2
    "###);

    Ok(())
}

/// Test special case to generate versioned pip launchers.
/// <https://github.com/pypa/pip/blob/3898741e29b7279e7bffe044ecfbe20f6a438b1e/src/pip/_internal/operations/install/wheel.py#L283>
/// <https://github.com/astral-sh/uv/issues/1593>
#[test]
fn pip_entrypoints() -> Result<()> {
    let context = TestContext::new("3.12");

    for pip_requirement in [
        // Test compatibility with launchers in 24.0
        // https://inspector.pypi.io/project/pip/24.0/packages/8a/6a/19e9fe04fca059ccf770861c7d5721ab4c2aebc539889e97c7977528a53b/pip-24.0-py3-none-any.whl/pip-24.0.dist-info/entry_points.txt
        "pip==24.0",
        // Test compatibility with launcher changes from https://github.com/pypa/pip/pull/12536 released in 24.1b1
        // See https://github.com/astral-sh/uv/pull/1982
        "pip==24.1b1",
    ] {
        let requirements_txt = context.temp_dir.child("requirements.txt");
        requirements_txt.write_str(pip_requirement)?;

        context
            .pip_sync()
            .arg("requirements.txt")
            .arg("--strict")
            .output()
            .expect("Failed to install pip");

        let bin_dir = context.venv.join(if cfg!(unix) {
            "bin"
        } else if cfg!(windows) {
            "Scripts"
        } else {
            unimplemented!("Only Windows and Unix are supported")
        });
        ChildPath::new(bin_dir.join(format!("pip3.10{EXE_SUFFIX}")))
            .assert(predicates::path::missing());
        ChildPath::new(bin_dir.join(format!("pip3.12{EXE_SUFFIX}")))
            .assert(predicates::path::exists());
    }

    Ok(())
}

#[test]
fn invalidate_on_change() -> Result<()> {
    let context = TestContext::new("3.12");

    // Create an editable package.
    let editable_dir = context.temp_dir.child("editable");
    editable_dir.create_dir_all()?;
    let pyproject_toml = editable_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"[project]
name = "example"
version = "0.0.0"
dependencies = [
  "anyio==4.0.0"
]
requires-python = ">=3.8"
"#,
    )?;

    // Write to a requirements file.
    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str(&format!("-e {}", editable_dir.path().display()))?;

    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.in"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + example==0.0.0 (from file://[TEMP_DIR]/editable)
    "###
    );

    // Installing again should be a no-op.
    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.in"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Audited 1 package in [TIME]
    "###
    );

    // Modify the editable package.
    pyproject_toml.write_str(
        r#"[project]
name = "example"
version = "0.0.0"
dependencies = [
  "anyio==3.7.1"
]
requires-python = ">=3.8"
"#,
    )?;

    // Re-installing should update the package.
    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.in"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     ~ example==0.0.0 (from file://[TEMP_DIR]/editable)
    "###
    );

    Ok(())
}

/// Install with bytecode compilation.
#[test]
fn compile() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("MarkupSafe==2.1.3")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--compile")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
    Bytecode compiled 3 files in [TIME]
     + markupsafe==2.1.3
    "###
    );

    assert!(context
        .site_packages()
        .join("markupsafe")
        .join("__pycache__")
        .join("__init__.cpython-312.pyc")
        .exists());

    context.assert_command("import markupsafe").success();

    Ok(())
}

/// Re-install with bytecode compilation.
#[test]
fn recompile() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("MarkupSafe==2.1.3")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + markupsafe==2.1.3
    "###
    );

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--compile")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Bytecode compiled 3 files in [TIME]
    "###
    );

    assert!(context
        .site_packages()
        .join("markupsafe")
        .join("__pycache__")
        .join("__init__.cpython-312.pyc")
        .exists());

    context.assert_command("import markupsafe").success();

    Ok(())
}

/// Raise an error when an editable's `Requires-Python` constraint is not met.
#[test]
fn requires_python_editable() -> Result<()> {
    let context = TestContext::new("3.12");

    // Create an editable package with a `Requires-Python` constraint that is not met.
    let editable_dir = context.temp_dir.child("editable");
    editable_dir.create_dir_all()?;
    let pyproject_toml = editable_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"[project]
name = "example"
version = "0.0.0"
dependencies = [
  "anyio==4.0.0"
]
requires-python = ">=3.13"
"#,
    )?;

    // Write to a requirements file.
    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str(&format!("-e {}", editable_dir.path().display()))?;

    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.in"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because the current Python version (3.12.[X]) does not satisfy Python>=3.13 and example==0.0.0 depends on Python>=3.13, we can conclude that example==0.0.0 cannot be used.
          And because only example==0.0.0 is available and you require example, we can conclude that your requirements are unsatisfiable.
    "###
    );

    Ok(())
}

/// Raise an error when a direct URL dependency's `Requires-Python` constraint is not met.
#[test]
fn requires_python_direct_url() -> Result<()> {
    let context = TestContext::new("3.12");

    // Create an editable package with a `Requires-Python` constraint that is not met.
    let editable_dir = context.temp_dir.child("editable");
    editable_dir.create_dir_all()?;
    let pyproject_toml = editable_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"[project]
name = "example"
version = "0.0.0"
dependencies = [
  "anyio==4.0.0"
]
requires-python = ">=3.13"
"#,
    )?;

    // Write to a requirements file.
    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str(&format!("example @ {}", editable_dir.path().display()))?;

    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.in"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because the current Python version (3.12.[X]) does not satisfy Python>=3.13 and example==0.0.0 depends on Python>=3.13, we can conclude that example==0.0.0 cannot be used.
          And because only example==0.0.0 is available and you require example, we can conclude that your requirements are unsatisfiable.
    "###
    );

    Ok(())
}

/// Use an unknown hash algorithm with `--require-hashes`.
#[test]
fn require_hashes_unknown_algorithm() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(
        "anyio==4.0.0 --hash=foo:cfdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f",
    )?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--require-hashes"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Unsupported hash algorithm (expected one of: `md5`, `sha256`, `sha384`, or `sha512`) on: `foo`
    "###
    );

    Ok(())
}

/// Omit the hash with `--require-hashes`.
#[test]
fn require_hashes_missing_hash() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("anyio==4.0.0")?;

    // Install without error when `--require-hashes` is omitted.
    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + anyio==4.0.0
    "###
    );

    // Error when `--require-hashes` is provided.
    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--require-hashes"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: In `--require-hashes` mode, all requirements must have a hash, but none were provided for: anyio==4.0.0
    "###
    );

    Ok(())
}

/// Omit the version with `--require-hashes`.
#[test]
fn require_hashes_missing_version() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(
        "anyio --hash=sha256:cfdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f",
    )?;

    // Install without error when `--require-hashes` is omitted.
    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + anyio==4.3.0
    "###
    );

    // Error when `--require-hashes` is provided.
    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--require-hashes"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: In `--require-hashes` mode, all requirements must have their versions pinned with `==`, but found: anyio
    "###
    );

    Ok(())
}

/// Use a non-`==` operator with `--require-hashes`.
#[test]
fn require_hashes_invalid_operator() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(
        "anyio>4.0.0 --hash=sha256:cfdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f",
    )?;

    // Install without error when `--require-hashes` is omitted.
    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + anyio==4.3.0
    "###
    );

    // Error when `--require-hashes` is provided.
    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--require-hashes"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: In `--require-hashes` mode, all requirements must have their versions pinned with `==`, but found: anyio>4.0.0
    "###
    );

    Ok(())
}

/// Include the hash for _just_ the wheel with `--no-binary`.
#[test]
fn require_hashes_wheel_no_binary() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str("anyio==4.0.0 --hash=sha256:cfdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--no-binary")
        .arg(":all:")
        .arg("--require-hashes"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
      × Failed to download and build `anyio==4.0.0`
      ╰─▶ Hash mismatch for `anyio==4.0.0`

          Expected:
            sha256:cfdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f

          Computed:
            sha256:f7ed51751b2c2add651e5747c891b47e26d2a21be5d32d9311dfe9692f3e5d7a
    "###
    );

    Ok(())
}

/// Include the hash for _just_ the wheel with `--only-binary`.
#[test]
fn require_hashes_wheel_only_binary() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str("anyio==4.0.0 --hash=sha256:cfdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--only-binary")
        .arg(":all:")
        .arg("--require-hashes"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + anyio==4.0.0
    "###
    );

    Ok(())
}

/// Include the hash for _just_ the source distribution with `--no-binary`.
#[test]
fn require_hashes_source_no_binary() -> Result<()> {
    let context = TestContext::new("3.12").with_exclude_newer("2025-01-29T00:00:00Z");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str("source-distribution==0.0.1 --hash=sha256:1f83ed7498336c7f2ab9b002cf22583d91115ebc624053dc4eb3a45694490106")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--no-binary")
        .arg(":all:")
        .arg("--require-hashes"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + source-distribution==0.0.1
    "###
    );

    Ok(())
}

/// Include the hash for _just_ the source distribution, with `--binary-only`.
#[test]
fn require_hashes_source_only_binary() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str("anyio==4.0.0 --hash=sha256:f7ed51751b2c2add651e5747c891b47e26d2a21be5d32d9311dfe9692f3e5d7a")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--only-binary")
        .arg(":all:")
        .arg("--require-hashes"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
      × Failed to download `anyio==4.0.0`
      ╰─▶ Hash mismatch for `anyio==4.0.0`

          Expected:
            sha256:f7ed51751b2c2add651e5747c891b47e26d2a21be5d32d9311dfe9692f3e5d7a

          Computed:
            sha256:cfdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f
    "###
    );

    Ok(())
}

/// Include the correct hash algorithm, but the wrong digest.
#[test]
fn require_hashes_wrong_digest() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str("anyio==4.0.0 --hash=sha256:afdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--require-hashes"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
      × Failed to download `anyio==4.0.0`
      ╰─▶ Hash mismatch for `anyio==4.0.0`

          Expected:
            sha256:afdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f

          Computed:
            sha256:cfdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f
    "###
    );

    Ok(())
}

/// Include the correct hash, but the wrong algorithm.
#[test]
fn require_hashes_wrong_algorithm() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str("anyio==4.0.0 --hash=sha512:cfdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--require-hashes"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
      × Failed to download `anyio==4.0.0`
      ╰─▶ Hash mismatch for `anyio==4.0.0`

          Expected:
            sha512:cfdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f

          Computed:
            sha512:f30761c1e8725b49c498273b90dba4b05c0fd157811994c806183062cb6647e773364ce45f0e1ff0b10e32fe6d0232ea5ad39476ccf37109d6b49603a09c11c2
    "###
    );

    Ok(())
}

/// Include the hash for a source distribution specified as a direct URL dependency.
#[test]
fn require_hashes_source_url() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str("source-distribution @ https://files.pythonhosted.org/packages/10/1f/57aa4cce1b1abf6b433106676e15f9fa2c92ed2bd4cf77c3b50a9e9ac773/source_distribution-0.0.1.tar.gz --hash=sha256:1f83ed7498336c7f2ab9b002cf22583d91115ebc624053dc4eb3a45694490106")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--require-hashes"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + source-distribution==0.0.1 (from https://files.pythonhosted.org/packages/10/1f/57aa4cce1b1abf6b433106676e15f9fa2c92ed2bd4cf77c3b50a9e9ac773/source_distribution-0.0.1.tar.gz)
    "###
    );

    // Reinstall with the right hash, and verify that it's reused.
    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--reinstall")
        .arg("--require-hashes"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     ~ source-distribution==0.0.1 (from https://files.pythonhosted.org/packages/10/1f/57aa4cce1b1abf6b433106676e15f9fa2c92ed2bd4cf77c3b50a9e9ac773/source_distribution-0.0.1.tar.gz)
    "###
    );

    // Reinstall with the wrong hash, and verify that it's rejected despite being cached.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str("source-distribution @ https://files.pythonhosted.org/packages/10/1f/57aa4cce1b1abf6b433106676e15f9fa2c92ed2bd4cf77c3b50a9e9ac773/source_distribution-0.0.1.tar.gz --hash=sha256:a7ed51751b2c2add651e5747c891b47e26d2a21be5d32d9311dfe9692f3e5d7a")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--reinstall")
        .arg("--require-hashes"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to download and build `source-distribution @ https://files.pythonhosted.org/packages/10/1f/57aa4cce1b1abf6b433106676e15f9fa2c92ed2bd4cf77c3b50a9e9ac773/source_distribution-0.0.1.tar.gz`
      ╰─▶ Hash mismatch for `source-distribution @ https://files.pythonhosted.org/packages/10/1f/57aa4cce1b1abf6b433106676e15f9fa2c92ed2bd4cf77c3b50a9e9ac773/source_distribution-0.0.1.tar.gz`

          Expected:
            sha256:a7ed51751b2c2add651e5747c891b47e26d2a21be5d32d9311dfe9692f3e5d7a

          Computed:
            sha256:1f83ed7498336c7f2ab9b002cf22583d91115ebc624053dc4eb3a45694490106
    "###
    );

    Ok(())
}

/// Include the _wrong_ hash for a source distribution specified as a direct URL dependency.
#[test]
fn require_hashes_source_url_mismatch() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str("source-distribution @ https://files.pythonhosted.org/packages/10/1f/57aa4cce1b1abf6b433106676e15f9fa2c92ed2bd4cf77c3b50a9e9ac773/source_distribution-0.0.1.tar.gz --hash=sha256:a7ed51751b2c2add651e5747c891b47e26d2a21be5d32d9311dfe9692f3e5d7a")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--require-hashes"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to download and build `source-distribution @ https://files.pythonhosted.org/packages/10/1f/57aa4cce1b1abf6b433106676e15f9fa2c92ed2bd4cf77c3b50a9e9ac773/source_distribution-0.0.1.tar.gz`
      ╰─▶ Hash mismatch for `source-distribution @ https://files.pythonhosted.org/packages/10/1f/57aa4cce1b1abf6b433106676e15f9fa2c92ed2bd4cf77c3b50a9e9ac773/source_distribution-0.0.1.tar.gz`

          Expected:
            sha256:a7ed51751b2c2add651e5747c891b47e26d2a21be5d32d9311dfe9692f3e5d7a

          Computed:
            sha256:1f83ed7498336c7f2ab9b002cf22583d91115ebc624053dc4eb3a45694490106
    "###
    );

    Ok(())
}

/// Include the hash for a built distribution specified as a direct URL dependency.
#[test]
fn require_hashes_wheel_url() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str("anyio @ https://files.pythonhosted.org/packages/36/55/ad4de788d84a630656ece71059665e01ca793c04294c463fd84132f40fe6/anyio-4.0.0-py3-none-any.whl --hash=sha256:cfdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--require-hashes"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + anyio==4.0.0 (from https://files.pythonhosted.org/packages/36/55/ad4de788d84a630656ece71059665e01ca793c04294c463fd84132f40fe6/anyio-4.0.0-py3-none-any.whl)
    "###
    );

    // Reinstall with the right hash, and verify that it's reused.
    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--reinstall")
        .arg("--require-hashes"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     ~ anyio==4.0.0 (from https://files.pythonhosted.org/packages/36/55/ad4de788d84a630656ece71059665e01ca793c04294c463fd84132f40fe6/anyio-4.0.0-py3-none-any.whl)
    "###
    );

    // Reinstall with the wrong hash, and verify that it's rejected despite being cached.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str("anyio @ https://files.pythonhosted.org/packages/36/55/ad4de788d84a630656ece71059665e01ca793c04294c463fd84132f40fe6/anyio-4.0.0-py3-none-any.whl --hash=sha256:afdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--reinstall")
        .arg("--require-hashes"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
      × Failed to download `anyio @ https://files.pythonhosted.org/packages/36/55/ad4de788d84a630656ece71059665e01ca793c04294c463fd84132f40fe6/anyio-4.0.0-py3-none-any.whl`
      ╰─▶ Hash mismatch for `anyio @ https://files.pythonhosted.org/packages/36/55/ad4de788d84a630656ece71059665e01ca793c04294c463fd84132f40fe6/anyio-4.0.0-py3-none-any.whl`

          Expected:
            sha256:afdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f

          Computed:
            sha256:cfdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f
    "###
    );

    // Sync a new dependency and include the wrong hash for anyio. Verify that we reuse anyio
    // despite the wrong hash, like pip, since we don't validate hashes for already-installed
    // distributions.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str("anyio==4.0.0 --hash=sha256:afdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f\niniconfig==2.0.0 --hash=sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--require-hashes"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    "###
    );

    Ok(())
}

/// Include the _wrong_ hash for a built distribution specified as a direct URL dependency.
#[test]
fn require_hashes_wheel_url_mismatch() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str("anyio @ https://files.pythonhosted.org/packages/36/55/ad4de788d84a630656ece71059665e01ca793c04294c463fd84132f40fe6/anyio-4.0.0-py3-none-any.whl --hash=sha256:afdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--require-hashes"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
      × Failed to download `anyio @ https://files.pythonhosted.org/packages/36/55/ad4de788d84a630656ece71059665e01ca793c04294c463fd84132f40fe6/anyio-4.0.0-py3-none-any.whl`
      ╰─▶ Hash mismatch for `anyio @ https://files.pythonhosted.org/packages/36/55/ad4de788d84a630656ece71059665e01ca793c04294c463fd84132f40fe6/anyio-4.0.0-py3-none-any.whl`

          Expected:
            sha256:afdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f

          Computed:
            sha256:cfdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f
    "###
    );

    Ok(())
}

/// Reject Git dependencies when `--require-hashes` is provided.
#[test]
#[cfg(feature = "git")]
fn require_hashes_git() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str("anyio @ git+https://github.com/agronholm/anyio@4a23745badf5bf5ef7928f1e346e9986bd696d82 --hash=sha256:f7ed51751b2c2add651e5747c891b47e26d2a21be5d32d9311dfe9692f3e5d7a")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--require-hashes"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to download and build `anyio @ git+https://github.com/agronholm/anyio@4a23745badf5bf5ef7928f1e346e9986bd696d82`
      ╰─▶ Hash-checking is not supported for Git repositories: `anyio @ git+https://github.com/agronholm/anyio@4a23745badf5bf5ef7928f1e346e9986bd696d82`
    "###
    );

    Ok(())
}

/// Reject local directory dependencies when `--require-hashes` is provided.
#[test]
fn require_hashes_source_tree() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(&format!(
        "black @ {} --hash=sha256:f7ed51751b2c2add651e5747c891b47e26d2a21be5d32d9311dfe9692f3e5d7a",
        context
            .workspace_root
            .join("scripts/packages/black_editable")
            .display()
    ))?;

    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.txt")
        .arg("--require-hashes"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to build `black @ file://[WORKSPACE]/scripts/packages/black_editable`
      ╰─▶ Hash-checking is not supported for local directories: `black @ file://[WORKSPACE]/scripts/packages/black_editable`
    "###
    );

    Ok(())
}

/// Include the hash for _just_ the wheel with `--only-binary`.
#[test]
fn require_hashes_re_download() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("anyio==4.0.0")?;

    // Install without `--require-hashes`.
    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + anyio==4.0.0
    "###
    );

    // Reinstall with `--require-hashes`, and the wrong hash.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str("anyio==4.0.0 --hash=sha256:afdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--reinstall")
        .arg("--require-hashes"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
      × Failed to download `anyio==4.0.0`
      ╰─▶ Hash mismatch for `anyio==4.0.0`

          Expected:
            sha256:afdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f

          Computed:
            sha256:cfdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f
    "###
    );

    // Reinstall with `--require-hashes`, and the right hash.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str("anyio==4.0.0 --hash=sha256:cfdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--reinstall")
        .arg("--require-hashes"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     ~ anyio==4.0.0
    "###
    );

    Ok(())
}

/// Include the hash for a built distribution specified as a local path dependency.
#[test]
fn require_hashes_wheel_path() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(&format!(
        "tqdm @ {} --hash=sha256:a34996d4bd5abb2336e14ff0a2d22b92cfd0f0ed344e6883041ce01953276a13",
        context
            .workspace_root
            .join("scripts/links/tqdm-1000.0.0-py3-none-any.whl")
            .display()
    ))?;

    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.txt")
        .arg("--require-hashes"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + tqdm==1000.0.0 (from file://[WORKSPACE]/scripts/links/tqdm-1000.0.0-py3-none-any.whl)
    "###
    );

    Ok(())
}

/// Include the _wrong_ hash for a built distribution specified as a local path dependency.
#[test]
fn require_hashes_wheel_path_mismatch() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(&format!(
        "tqdm @ {} --hash=sha256:cfdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f",
        context
            .workspace_root
            .join("scripts/links/tqdm-1000.0.0-py3-none-any.whl")
            .display()
    ))?;

    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.txt")
        .arg("--require-hashes"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
      × Failed to read `tqdm @ file://[WORKSPACE]/scripts/links/tqdm-1000.0.0-py3-none-any.whl`
      ╰─▶ Hash mismatch for `tqdm @ file://[WORKSPACE]/scripts/links/tqdm-1000.0.0-py3-none-any.whl`

          Expected:
            sha256:cfdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f

          Computed:
            sha256:a34996d4bd5abb2336e14ff0a2d22b92cfd0f0ed344e6883041ce01953276a13
    "###
    );

    Ok(())
}

/// Include the hash for a source distribution specified as a local path dependency.
#[test]
fn require_hashes_source_path() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(&format!(
        "tqdm @ {} --hash=sha256:89fa05cffa7f457658373b85de302d24d0c205ceda2819a8739e324b75e9430b",
        context
            .workspace_root
            .join("scripts/links/tqdm-999.0.0.tar.gz")
            .display()
    ))?;

    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.txt")
        .arg("--require-hashes"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + tqdm==999.0.0 (from file://[WORKSPACE]/scripts/links/tqdm-999.0.0.tar.gz)
    "###
    );

    Ok(())
}

/// Include the _wrong_ hash for a source distribution specified as a local path dependency.
#[test]
fn require_hashes_source_path_mismatch() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(&format!(
        "tqdm @ {} --hash=sha256:cfdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f",
        context
            .workspace_root
            .join("scripts/links/tqdm-999.0.0.tar.gz")
            .display()
    ))?;

    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.txt")
        .arg("--require-hashes"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to build `tqdm @ file://[WORKSPACE]/scripts/links/tqdm-999.0.0.tar.gz`
      ╰─▶ Hash mismatch for `tqdm @ file://[WORKSPACE]/scripts/links/tqdm-999.0.0.tar.gz`

          Expected:
            sha256:cfdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f

          Computed:
            sha256:89fa05cffa7f457658373b85de302d24d0c205ceda2819a8739e324b75e9430b
    "###
    );

    Ok(())
}

/// We allow `--require-hashes` for direct URL dependencies.
#[test]
fn require_hashes_unnamed() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str(indoc::indoc! {r"
            https://files.pythonhosted.org/packages/36/55/ad4de788d84a630656ece71059665e01ca793c04294c463fd84132f40fe6/anyio-4.0.0-py3-none-any.whl --hash=sha256:cfdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f
        "} )?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--require-hashes"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + anyio==4.0.0 (from https://files.pythonhosted.org/packages/36/55/ad4de788d84a630656ece71059665e01ca793c04294c463fd84132f40fe6/anyio-4.0.0-py3-none-any.whl)
    "###
    );

    Ok(())
}

/// We disallow `--require-hashes` for editables.
#[test]
fn require_hashes_editable() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(&indoc::formatdoc! {r"
        -e file://{workspace_root}/scripts/packages/black_editable[d]
        ",
        workspace_root = context.workspace_root.simplified_display(),
    })?;

    // Install the editable packages.
    uv_snapshot!(context.filters(), context.pip_sync()
        .arg(requirements_txt.path())
        .arg("--require-hashes"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: In `--require-hashes` mode, all requirements must have a hash, but none were provided for: file://[WORKSPACE]/scripts/packages/black_editable[d]
    "###
    );

    Ok(())
}

/// If a dependency is repeated, the hash should be required for both instances.
#[test]
fn require_hashes_repeated_dependency() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str("anyio==4.0.0 --hash=sha256:f7ed51751b2c2add651e5747c891b47e26d2a21be5d32d9311dfe9692f3e5d7a\nanyio")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--require-hashes"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: In `--require-hashes` mode, all requirements must have their versions pinned with `==`, but found: anyio
    "###
    );

    // Reverse the order.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str("anyio\nanyio==4.0.0 --hash=sha256:f7ed51751b2c2add651e5747c891b47e26d2a21be5d32d9311dfe9692f3e5d7a")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--require-hashes"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: In `--require-hashes` mode, all requirements must have their versions pinned with `==`, but found: anyio
    "###
    );

    Ok(())
}

/// If a dependency is repeated, use the last hash provided. pip seems to use the _first_ hash.
#[test]
fn require_hashes_repeated_hash() -> Result<()> {
    let context = TestContext::new("3.12");

    // Use the same hash in both cases.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str(indoc::indoc! { r"
            anyio @ https://files.pythonhosted.org/packages/36/55/ad4de788d84a630656ece71059665e01ca793c04294c463fd84132f40fe6/anyio-4.0.0-py3-none-any.whl --hash=sha256:cfdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f
            anyio @ https://files.pythonhosted.org/packages/36/55/ad4de788d84a630656ece71059665e01ca793c04294c463fd84132f40fe6/anyio-4.0.0-py3-none-any.whl --hash=sha256:cfdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f
    " })?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--require-hashes"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + anyio==4.0.0 (from https://files.pythonhosted.org/packages/36/55/ad4de788d84a630656ece71059665e01ca793c04294c463fd84132f40fe6/anyio-4.0.0-py3-none-any.whl)
    "###
    );

    // Use a different hash, but both are correct.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str(indoc::indoc! { r"
            anyio @ https://files.pythonhosted.org/packages/36/55/ad4de788d84a630656ece71059665e01ca793c04294c463fd84132f40fe6/anyio-4.0.0-py3-none-any.whl --hash=sha256:cfdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f
            anyio @ https://files.pythonhosted.org/packages/36/55/ad4de788d84a630656ece71059665e01ca793c04294c463fd84132f40fe6/anyio-4.0.0-py3-none-any.whl --hash=sha512:f30761c1e8725b49c498273b90dba4b05c0fd157811994c806183062cb6647e773364ce45f0e1ff0b10e32fe6d0232ea5ad39476ccf37109d6b49603a09c11c2
    " })?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--require-hashes")
        .arg("--reinstall"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     ~ anyio==4.0.0 (from https://files.pythonhosted.org/packages/36/55/ad4de788d84a630656ece71059665e01ca793c04294c463fd84132f40fe6/anyio-4.0.0-py3-none-any.whl)
    "###
    );

    // Use a different hash. The first hash is wrong, but that's fine, since we use the last hash.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str(indoc::indoc! { r"
            anyio @ https://files.pythonhosted.org/packages/36/55/ad4de788d84a630656ece71059665e01ca793c04294c463fd84132f40fe6/anyio-4.0.0-py3-none-any.whl --hash=sha256:a7ed51751b2c2add651e5747c891b47e26d2a21be5d32d9311dfe9692f3e5d7a
            anyio @ https://files.pythonhosted.org/packages/36/55/ad4de788d84a630656ece71059665e01ca793c04294c463fd84132f40fe6/anyio-4.0.0-py3-none-any.whl --hash=md5:420d85e19168705cdf0223621b18831a
    " })?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--require-hashes")
        .arg("--reinstall"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     ~ anyio==4.0.0 (from https://files.pythonhosted.org/packages/36/55/ad4de788d84a630656ece71059665e01ca793c04294c463fd84132f40fe6/anyio-4.0.0-py3-none-any.whl)
    "###
    );

    // Use a different hash. The second hash is wrong. This should fail, since we use the last hash.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str(indoc::indoc! { r"
            anyio @ https://files.pythonhosted.org/packages/36/55/ad4de788d84a630656ece71059665e01ca793c04294c463fd84132f40fe6/anyio-4.0.0-py3-none-any.whl --hash=sha256:f7ed51751b2c2add651e5747c891b47e26d2a21be5d32d9311dfe9692f3e5d7a
            anyio @ https://files.pythonhosted.org/packages/36/55/ad4de788d84a630656ece71059665e01ca793c04294c463fd84132f40fe6/anyio-4.0.0-py3-none-any.whl --hash=md5:520d85e19168705cdf0223621b18831a
    " })?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--require-hashes")
        .arg("--reinstall"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
      × Failed to download `anyio @ https://files.pythonhosted.org/packages/36/55/ad4de788d84a630656ece71059665e01ca793c04294c463fd84132f40fe6/anyio-4.0.0-py3-none-any.whl`
      ╰─▶ Hash mismatch for `anyio @ https://files.pythonhosted.org/packages/36/55/ad4de788d84a630656ece71059665e01ca793c04294c463fd84132f40fe6/anyio-4.0.0-py3-none-any.whl`

          Expected:
            md5:520d85e19168705cdf0223621b18831a

          Computed:
            md5:420d85e19168705cdf0223621b18831a
    "###
    );

    Ok(())
}

/// If a dependency is repeated, the hash should be required for both instances.
#[test]
fn require_hashes_at_least_one() -> Result<()> {
    let context = TestContext::new("3.12");

    // Request `anyio` with a `sha256` hash.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str("anyio==4.0.0 --hash=sha256:f7ed51751b2c2add651e5747c891b47e26d2a21be5d32d9311dfe9692f3e5d7a")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--require-hashes"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + anyio==4.0.0
    "###
    );

    // Reinstall, requesting both `sha256` and `sha512`. We should reinstall from the cache, since
    // at least one hash matches.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str("anyio==4.0.0 --hash=sha256:f7ed51751b2c2add651e5747c891b47e26d2a21be5d32d9311dfe9692f3e5d7a --hash=md5:420d85e19168705cdf0223621b18831a")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--reinstall")
        .arg("--require-hashes"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     ~ anyio==4.0.0
    "###
    );

    // This should be true even if the second hash is wrong.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str("anyio==4.0.0 --hash=sha256:f7ed51751b2c2add651e5747c891b47e26d2a21be5d32d9311dfe9692f3e5d7a --hash=md5:1234")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--reinstall")
        .arg("--require-hashes"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     ~ anyio==4.0.0
    "###
    );

    Ok(())
}

/// Using `--find-links`, but the registry doesn't provide us with a hash.
#[test]
fn require_hashes_find_links_no_hash() -> Result<()> {
    let context = TestContext::new("3.12");

    // First, use the correct hash.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str("example-a-961b4c22==1.0.0 --hash=sha256:5d69f0b590514103234f0c3526563856f04d044d8d0ea1073a843ae429b3187e")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--reinstall")
        .arg("--require-hashes")
        .arg("--find-links")
        .arg("https://raw.githubusercontent.com/astral-test/astral-test-hash/main/no-hash/simple-html/example-a-961b4c22/index.html"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + example-a-961b4c22==1.0.0
    "###
    );

    // Second, use an incorrect hash.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("example-a-961b4c22==1.0.0 --hash=sha256:123")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--reinstall")
        .arg("--require-hashes")
        .arg("--find-links")
        .arg("https://raw.githubusercontent.com/astral-test/astral-test-hash/main/no-hash/simple-html/example-a-961b4c22/index.html"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
      × Failed to download `example-a-961b4c22==1.0.0`
      ╰─▶ Hash mismatch for `example-a-961b4c22==1.0.0`

          Expected:
            sha256:123

          Computed:
            sha256:5d69f0b590514103234f0c3526563856f04d044d8d0ea1073a843ae429b3187e
    "###
    );

    // Third, use the hash from the source distribution. This will actually fail, when it _could_
    // succeed, but pip has the same behavior.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str("example-a-961b4c22==1.0.0 --hash=sha256:294e788dbe500fdc39e8b88e82652ab67409a1dc9dd06543d0fe0ae31b713eb3")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--reinstall")
        .arg("--require-hashes")
        .arg("--find-links")
        .arg("https://raw.githubusercontent.com/astral-test/astral-test-hash/main/no-hash/simple-html/example-a-961b4c22/index.html"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
      × Failed to download `example-a-961b4c22==1.0.0`
      ╰─▶ Hash mismatch for `example-a-961b4c22==1.0.0`

          Expected:
            sha256:294e788dbe500fdc39e8b88e82652ab67409a1dc9dd06543d0fe0ae31b713eb3

          Computed:
            sha256:5d69f0b590514103234f0c3526563856f04d044d8d0ea1073a843ae429b3187e
    "###
    );

    // Fourth, use the hash from the source distribution, and disable wheels. This should succeed.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str("example-a-961b4c22==1.0.0 --hash=sha256:294e788dbe500fdc39e8b88e82652ab67409a1dc9dd06543d0fe0ae31b713eb3")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--no-binary")
        .arg(":all:")
        .arg("--reinstall")
        .arg("--require-hashes")
        .arg("--find-links")
        .arg("https://raw.githubusercontent.com/astral-test/astral-test-hash/main/no-hash/simple-html/example-a-961b4c22/index.html"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     ~ example-a-961b4c22==1.0.0
    "###
    );

    Ok(())
}

/// Using `--find-links`, and the registry serves us a correct hash.
#[test]
fn require_hashes_find_links_valid_hash() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str("example-a-961b4c22==1.0.0 --hash=sha256:5d69f0b590514103234f0c3526563856f04d044d8d0ea1073a843ae429b3187e")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--require-hashes")
        .arg("--find-links")
        .arg("https://raw.githubusercontent.com/astral-test/astral-test-hash/main/valid-hash/simple-html/example-a-961b4c22/index.html"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + example-a-961b4c22==1.0.0
    "###
    );

    Ok(())
}

/// Using `--find-links`, and the registry serves us an incorrect hash.
#[test]
fn require_hashes_find_links_invalid_hash() -> Result<()> {
    let context = TestContext::new("3.12");

    // First, request some other hash.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("example-a-961b4c22==1.0.0 --hash=sha256:123")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--reinstall")
        .arg("--require-hashes")
        .arg("--find-links")
        .arg("https://raw.githubusercontent.com/astral-test/astral-test-hash/main/invalid-hash/simple-html/example-a-961b4c22/index.html"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
      × Failed to download `example-a-961b4c22==1.0.0`
      ╰─▶ Hash mismatch for `example-a-961b4c22==1.0.0`

          Expected:
            sha256:123

          Computed:
            sha256:5d69f0b590514103234f0c3526563856f04d044d8d0ea1073a843ae429b3187e
    "###
    );

    // Second, request the invalid hash, that the registry _thinks_ is correct. We should reject it.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str("example-a-961b4c22==1.0.0 --hash=sha256:8838f9d005ff0432b258ba648d9cabb1cbdf06ac29d14f788b02edae544032ea")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--reinstall")
        .arg("--require-hashes")
        .arg("--find-links")
        .arg("https://raw.githubusercontent.com/astral-test/astral-test-hash/main/invalid-hash/simple-html/example-a-961b4c22/index.html"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
      × Failed to download `example-a-961b4c22==1.0.0`
      ╰─▶ Hash mismatch for `example-a-961b4c22==1.0.0`

          Expected:
            sha256:8838f9d005ff0432b258ba648d9cabb1cbdf06ac29d14f788b02edae544032ea

          Computed:
            sha256:5d69f0b590514103234f0c3526563856f04d044d8d0ea1073a843ae429b3187e
    "###
    );

    // Third, request the correct hash, that the registry _thinks_ is correct. We should accept
    // it, since it's already cached under this hash.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str("example-a-961b4c22==1.0.0 --hash=sha256:5d69f0b590514103234f0c3526563856f04d044d8d0ea1073a843ae429b3187e")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--reinstall")
        .arg("--require-hashes")
        .arg("--find-links")
        .arg("https://raw.githubusercontent.com/astral-test/astral-test-hash/main/invalid-hash/simple-html/example-a-961b4c22/index.html"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + example-a-961b4c22==1.0.0
    "###
    );

    // Fourth, request the correct hash, that the registry _thinks_ is correct, but without the
    // cache. We _should_ accept it, but we currently don't.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str("example-a-961b4c22==1.0.0 --hash=sha256:5d69f0b590514103234f0c3526563856f04d044d8d0ea1073a843ae429b3187e")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--refresh")
        .arg("--reinstall")
        .arg("--require-hashes")
        .arg("--find-links")
        .arg("https://raw.githubusercontent.com/astral-test/astral-test-hash/main/invalid-hash/simple-html/example-a-961b4c22/index.html"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     ~ example-a-961b4c22==1.0.0
    "###
    );

    // Finally, request the correct hash, along with the incorrect hash for the source distribution.
    // Resolution will fail, since the incorrect hash matches the registry's hash.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str("example-a-961b4c22==1.0.0 --hash=sha256:5d69f0b590514103234f0c3526563856f04d044d8d0ea1073a843ae429b3187e --hash=sha256:a3cf07a05aac526131a2e8b6e4375ee6c6eaac8add05b88035e960ac6cd999ee")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--refresh")
        .arg("--reinstall")
        .arg("--require-hashes")
        .arg("--find-links")
        .arg("https://raw.githubusercontent.com/astral-test/astral-test-hash/main/invalid-hash/simple-html/example-a-961b4c22/index.html"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
      × Failed to download and build `example-a-961b4c22==1.0.0`
      ╰─▶ Hash mismatch for `example-a-961b4c22==1.0.0`

          Expected:
            sha256:5d69f0b590514103234f0c3526563856f04d044d8d0ea1073a843ae429b3187e
            sha256:a3cf07a05aac526131a2e8b6e4375ee6c6eaac8add05b88035e960ac6cd999ee

          Computed:
            sha256:294e788dbe500fdc39e8b88e82652ab67409a1dc9dd06543d0fe0ae31b713eb3
    "###
    );

    Ok(())
}

/// Using `--index-url`, but the registry doesn't provide us with a hash.
#[test]
fn require_hashes_registry_no_hash() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str("example-a-961b4c22==1.0.0 --hash=sha256:5d69f0b590514103234f0c3526563856f04d044d8d0ea1073a843ae429b3187e")?;

    uv_snapshot!(context.pip_sync()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("requirements.txt")
        .arg("--require-hashes")
        .arg("--index-url")
        .arg("https://astral-test.github.io/astral-test-hash/no-hash/simple-html/"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + example-a-961b4c22==1.0.0
    "###
    );

    Ok(())
}

/// Using `--index-url`, and the registry serves us a correct hash.
#[test]
fn require_hashes_registry_valid_hash() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str("example-a-961b4c22==1.0.0 --hash=sha256:5d69f0b590514103234f0c3526563856f04d044d8d0ea1073a843ae429b3187e")?;

    uv_snapshot!(context.pip_sync()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("requirements.txt")
        .arg("--require-hashes")
        .arg("--find-links")
        .arg("https://astral-test.github.io/astral-test-hash/valid-hash/simple-html/"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because example-a-961b4c22 was not found in the package registry and you require example-a-961b4c22==1.0.0, we can conclude that your requirements are unsatisfiable.
    "###
    );

    Ok(())
}

/// Using `--index-url`, and the registry serves us an incorrect hash.
#[test]
fn require_hashes_registry_invalid_hash() -> Result<()> {
    let context = TestContext::new("3.12");

    // First, request some other hash.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("example-a-961b4c22==1.0.0 --hash=sha256:123")?;

    uv_snapshot!(context.pip_sync()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("requirements.txt")
        .arg("--reinstall")
        .arg("--require-hashes")
        .arg("--index-url")
        .arg("https://astral-test.github.io/astral-test-hash/invalid-hash/simple-html/"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
      × Failed to download `example-a-961b4c22==1.0.0`
      ╰─▶ Hash mismatch for `example-a-961b4c22==1.0.0`

          Expected:
            sha256:123

          Computed:
            sha256:5d69f0b590514103234f0c3526563856f04d044d8d0ea1073a843ae429b3187e
    "###
    );

    // Second, request the invalid hash, that the registry _thinks_ is correct. We should reject it.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str("example-a-961b4c22==1.0.0 --hash=sha256:8838f9d005ff0432b258ba648d9cabb1cbdf06ac29d14f788b02edae544032ea")?;

    uv_snapshot!(context.pip_sync()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("requirements.txt")
        .arg("--reinstall")
        .arg("--require-hashes")
        .arg("--index-url")
        .arg("https://astral-test.github.io/astral-test-hash/invalid-hash/simple-html/"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
      × Failed to download `example-a-961b4c22==1.0.0`
      ╰─▶ Hash mismatch for `example-a-961b4c22==1.0.0`

          Expected:
            sha256:8838f9d005ff0432b258ba648d9cabb1cbdf06ac29d14f788b02edae544032ea

          Computed:
            sha256:5d69f0b590514103234f0c3526563856f04d044d8d0ea1073a843ae429b3187e
    "###
    );

    // Third, request the correct hash, that the registry _thinks_ is correct. We should accept
    // it, since it's already cached under this hash.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str("example-a-961b4c22==1.0.0 --hash=sha256:5d69f0b590514103234f0c3526563856f04d044d8d0ea1073a843ae429b3187e")?;

    uv_snapshot!(context.pip_sync()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("requirements.txt")
        .arg("--reinstall")
        .arg("--require-hashes")
        .arg("--index-url")
        .arg("https://astral-test.github.io/astral-test-hash/invalid-hash/simple-html/"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + example-a-961b4c22==1.0.0
    "###
    );

    // Fourth, request the correct hash, that the registry _thinks_ is correct, but without the
    // cache. We _should_ accept it, but we currently don't.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str("example-a-961b4c22==1.0.0 --hash=sha256:5d69f0b590514103234f0c3526563856f04d044d8d0ea1073a843ae429b3187e")?;

    uv_snapshot!(context.pip_sync()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("requirements.txt")
        .arg("--refresh")
        .arg("--reinstall")
        .arg("--require-hashes")
        .arg("--index-url")
        .arg("https://astral-test.github.io/astral-test-hash/invalid-hash/simple-html/"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     ~ example-a-961b4c22==1.0.0
    "###
    );

    // Finally, request the correct hash, along with the incorrect hash for the source distribution.
    // Resolution will fail, since the incorrect hash matches the registry's hash.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str("example-a-961b4c22==1.0.0 --hash=sha256:5d69f0b590514103234f0c3526563856f04d044d8d0ea1073a843ae429b3187e --hash=sha256:a3cf07a05aac526131a2e8b6e4375ee6c6eaac8add05b88035e960ac6cd999ee")?;

    uv_snapshot!(context.pip_sync()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("requirements.txt")
        .arg("--refresh")
        .arg("--reinstall")
        .arg("--require-hashes")
        .arg("--index-url")
        .arg("https://astral-test.github.io/astral-test-hash/invalid-hash/simple-html/"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
      × Failed to download and build `example-a-961b4c22==1.0.0`
      ╰─▶ Hash mismatch for `example-a-961b4c22==1.0.0`

          Expected:
            sha256:5d69f0b590514103234f0c3526563856f04d044d8d0ea1073a843ae429b3187e
            sha256:a3cf07a05aac526131a2e8b6e4375ee6c6eaac8add05b88035e960ac6cd999ee

          Computed:
            sha256:294e788dbe500fdc39e8b88e82652ab67409a1dc9dd06543d0fe0ae31b713eb3
    "###
    );

    Ok(())
}

/// Include the hash in the URL directly.
#[test]
fn require_hashes_url() -> Result<()> {
    let context = TestContext::new("3.12").with_exclude_newer("2025-01-29T00:00:00Z");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str("iniconfig @ https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl#sha256=b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--require-hashes"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0 (from https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl#sha256=b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374)
    "###
    );

    Ok(())
}

/// Include an irrelevant fragment in the URL.
#[test]
fn require_hashes_url_other_fragment() -> Result<()> {
    let context = TestContext::new("3.12").with_exclude_newer("2025-01-29T00:00:00Z");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str("iniconfig @ https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl#foo=bar")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--require-hashes"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: In `--require-hashes` mode, all requirements must have a hash, but none were provided for: iniconfig @ https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl#foo=bar
    "###
    );

    Ok(())
}

/// Include an invalid hash in the URL directly.
#[test]
fn require_hashes_url_invalid() -> Result<()> {
    let context = TestContext::new("3.12").with_exclude_newer("2025-01-29T00:00:00Z");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str("iniconfig @ https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl#sha256=c6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--require-hashes"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
      × Failed to download `iniconfig @ https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl#sha256=c6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374`
      ╰─▶ Hash mismatch for `iniconfig @ https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl#sha256=c6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374`

          Expected:
            sha256:c6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374

          Computed:
            sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374
    "###
    );

    Ok(())
}

/// Ignore the (valid) hash on the fragment if (invalid) hashes are provided directly.
#[test]
fn require_hashes_url_ignore() -> Result<()> {
    let context = TestContext::new("3.12").with_exclude_newer("2025-01-29T00:00:00Z");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str("iniconfig @ https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl#sha256=b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374 --hash sha256:c6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--require-hashes"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
      × Failed to download `iniconfig @ https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl#sha256=b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374`
      ╰─▶ Hash mismatch for `iniconfig @ https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl#sha256=b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374`

          Expected:
            sha256:c6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374

          Computed:
            sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374
    "###
    );

    Ok(())
}

/// Include the hash in the URL directly.
#[test]
fn require_hashes_url_unnamed() -> Result<()> {
    let context = TestContext::new("3.12").with_exclude_newer("2025-01-29T00:00:00Z");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str("https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl#sha256=b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--require-hashes"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0 (from https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl#sha256=b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374)
    "###
    );

    Ok(())
}

/// Sync to a `--target` directory with a built distribution.
#[test]
fn target_built_distribution() -> Result<()> {
    let context = TestContext::new("3.12")
        .with_filtered_python_names()
        .with_filtered_virtualenv_bin()
        .with_filtered_exe_suffix();

    // Install `iniconfig` to the target directory.
    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str("iniconfig==2.0.0")?;

    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.in")
        .arg("--target")
        .arg("target"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: .venv/[BIN]/python
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    "###);

    // Ensure that the package is present in the target directory.
    assert!(context.temp_dir.child("target").child("iniconfig").is_dir());

    // Ensure that we can't import the package.
    context.assert_command("import iniconfig").failure();

    // Ensure that we can import the package by augmenting the `PYTHONPATH`.
    Command::new(venv_to_interpreter(&context.venv))
        .arg("-B")
        .arg("-c")
        .arg("import iniconfig")
        .env(EnvVars::PYTHONPATH, context.temp_dir.child("target").path())
        .current_dir(&context.temp_dir)
        .assert()
        .success();

    // Upgrade it.
    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str("iniconfig==1.1.1")?;

    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.in")
        .arg("--target")
        .arg("target"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: .venv/[BIN]/python
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - iniconfig==2.0.0
     + iniconfig==1.1.1
    "###);

    // Remove it, and replace with `flask`, which includes a binary.
    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str("flask")?;

    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.in")
        .arg("--target")
        .arg("target"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: .venv/[BIN]/python
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     + flask==3.0.2
     - iniconfig==1.1.1
    "###);
    // Ensure that the binary is present in the target directory.
    assert!(context
        .temp_dir
        .child("target")
        .child("bin")
        .child(format!("flask{EXE_SUFFIX}"))
        .is_file());

    Ok(())
}

/// Sync to a `--target` directory with a package that requires building from source.
#[test]
fn target_source_distribution() -> Result<()> {
    let context = TestContext::new("3.12")
        .with_filtered_python_names()
        .with_filtered_virtualenv_bin()
        .with_filtered_exe_suffix();

    // Install `iniconfig` to the target directory.
    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str("iniconfig==2.0.0")?;

    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.in")
        .arg("--no-binary")
        .arg("iniconfig")
        .arg("--target")
        .arg("target"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: .venv/[BIN]/python
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    "###);

    // Ensure that the build requirements are not present in the target directory.
    assert!(!context.temp_dir.child("target").child("hatchling").is_dir());

    // Ensure that the package is present in the target directory.
    assert!(context.temp_dir.child("target").child("iniconfig").is_dir());

    // Ensure that we can't import the package.
    context.assert_command("import iniconfig").failure();

    // Ensure that we can import the package by augmenting the `PYTHONPATH`.
    Command::new(venv_to_interpreter(&context.venv))
        .arg("-B")
        .arg("-c")
        .arg("import iniconfig")
        .env(EnvVars::PYTHONPATH, context.temp_dir.child("target").path())
        .current_dir(&context.temp_dir)
        .assert()
        .success();

    Ok(())
}

/// Sync to a `--target` directory with a package that requires building from source, along with
/// `--no-build-isolation`.
#[test]
fn target_no_build_isolation() -> Result<()> {
    let context = TestContext::new("3.12")
        .with_filtered_python_names()
        .with_filtered_virtualenv_bin()
        .with_filtered_exe_suffix();

    // Install `hatchling` into the current environment.
    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str("flit_core")?;

    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.in"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + flit-core==3.9.0
    "###);

    // Install `iniconfig` to the target directory.
    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str("wheel")?;

    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.in")
        .arg("--no-build-isolation")
        .arg("--no-binary")
        .arg("wheel")
        .arg("--target")
        .arg("target"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: .venv/[BIN]/python
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + wheel==0.43.0
    "###);

    // Ensure that the build requirements are not present in the target directory.
    assert!(!context.temp_dir.child("target").child("flit_core").is_dir());

    // Ensure that the package is present in the target directory.
    assert!(context.temp_dir.child("target").child("wheel").is_dir());

    // Ensure that we can't import the package.
    context.assert_command("import wheel").failure();

    // Ensure that we can import the package by augmenting the `PYTHONPATH`.
    Command::new(venv_to_interpreter(&context.venv))
        .arg("-B")
        .arg("-c")
        .arg("import wheel")
        .env(EnvVars::PYTHONPATH, context.temp_dir.child("target").path())
        .current_dir(&context.temp_dir)
        .assert()
        .success();

    Ok(())
}

/// Sync to a `--target` directory without a virtual environment.
#[test]
fn target_system() -> Result<()> {
    let context = TestContext::new_with_versions(&["3.12"]);

    // Install `iniconfig` to the target directory.
    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str("iniconfig==2.0.0")?;

    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.in")
        .arg("--target")
        .arg("target"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    "###);

    // Ensure that the package is present in the target directory.
    assert!(context.temp_dir.child("target").child("iniconfig").is_dir());

    Ok(())
}

/// Sync to a `--prefix` directory.
#[test]
fn prefix() -> Result<()> {
    let context = TestContext::new("3.12")
        .with_filtered_python_names()
        .with_filtered_virtualenv_bin()
        .with_filtered_exe_suffix();

    // Install `iniconfig` to the target directory.
    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str("iniconfig==2.0.0")?;

    let prefix = context.temp_dir.child("prefix");

    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.in")
        .arg("--prefix")
        .arg(prefix.path()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: .venv/[BIN]/python
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    "###);

    // Ensure that we can't import the package.
    context.assert_command("import iniconfig").failure();

    // Ensure that we can import the package by augmenting the `PYTHONPATH`.
    Command::new(venv_to_interpreter(&context.venv))
        .arg("-B")
        .arg("-c")
        .arg("import iniconfig")
        .env(
            EnvVars::PYTHONPATH,
            site_packages_path(&context.temp_dir.join("prefix"), "python3.12"),
        )
        .current_dir(&context.temp_dir)
        .assert()
        .success();

    // Upgrade it.
    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str("iniconfig==1.1.1")?;

    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.in")
        .arg("--prefix")
        .arg(prefix.path()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: .venv/[BIN]/python
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - iniconfig==2.0.0
     + iniconfig==1.1.1
    "###);

    Ok(())
}

/// Ensure that we install packages with markers on them.
#[test]
fn preserve_markers() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("anyio ; python_version > '3.7'")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + anyio==4.3.0
    "###
    );

    Ok(())
}

/// Include a `build_constraints.txt` file with an incompatible constraint.
#[test]
fn incompatible_build_constraint() -> Result<()> {
    let context = TestContext::new("3.8");
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("requests==1.2")?;

    let constraints_txt = context.temp_dir.child("build_constraints.txt");
    constraints_txt.write_str("setuptools==1")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--build-constraint")
        .arg("build_constraints.txt"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
      × Failed to download and build `requests==1.2.0`
      ├─▶ Failed to resolve requirements from `setup.py` build
      ├─▶ No solution found when resolving: `setuptools>=40.8.0`
      ╰─▶ Because you require setuptools>=40.8.0 and setuptools==1, we can conclude that your requirements are unsatisfiable.
    "###
    );

    Ok(())
}

/// Include a `build_constraints.txt` file with a compatible constraint.
#[test]
fn compatible_build_constraint() -> Result<()> {
    let context = TestContext::new("3.8");
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("requests==1.2")?;

    let constraints_txt = context.temp_dir.child("build_constraints.txt");
    constraints_txt.write_str("setuptools>=40")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt")
        .arg("--build-constraint")
        .arg("build_constraints.txt"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + requests==1.2.0
    "###
    );

    Ok(())
}

#[test]
fn sync_seed() -> Result<()> {
    let context = TestContext::new("3.8");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("requests==1.2")?;

    // Add `pip` to the environment.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("pip"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + pip==24.0
    "###
    );

    // Syncing should remove the seed packages.
    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.txt"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - pip==24.0
     + requests==1.2.0
    "###
    );

    // Re-create the environment with seed packages.
    uv_snapshot!(context.filters(), context.venv()
        .arg("--seed"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.8.[X] interpreter at: [PYTHON-3.8]
    Creating virtual environment with seed packages at: .venv
     + pip==24.0
     + setuptools==69.2.0
     + wheel==0.43.0
    Activate with: source .venv/[BIN]/activate
    "###
    );

    // Syncing should retain the seed packages.
    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.txt"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Installed 1 package in [TIME]
     + requests==1.2.0
    "###
    );

    Ok(())
}

/// Sanitize zip files during extraction.
#[test]
fn sanitize() -> Result<()> {
    let context = TestContext::new("3.12");

    // Install a zip file that includes a path that extends outside the parent.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("payload-package @ https://github.com/astral-sh/sanitize-wheel-test/raw/bc59283d5b4b136a191792e32baa51b477fdf65e/payload_package-0.1.0-py3-none-any.whl")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + payload-package==0.1.0 (from https://github.com/astral-sh/sanitize-wheel-test/raw/bc59283d5b4b136a191792e32baa51b477fdf65e/payload_package-0.1.0-py3-none-any.whl)
    "###
    );

    // There should be no `payload` file in the root.
    if let Some(parent) = context.temp_dir.parent() {
        assert!(!parent.join("payload").exists());
    }

    Ok(())
}

/// Allow semicolons attached to markers, as long as they're preceded by a space.
#[test]
fn semicolon_trailing_space() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements = context.temp_dir.child("requirements.txt");
    requirements.write_str("iniconfig @ https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl; python_version > '3.10'")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0 (from https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl)
    "###
    );

    Ok(())
}

/// Treat a semicolon that's not whitespace-separated as a part of the URL.
#[test]
fn semicolon_no_space() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements = context.temp_dir.child("requirements.txt");
    requirements.write_str("iniconfig @ https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl;python_version > '3.10'")?;

    uv_snapshot!(context.pip_sync()
        .arg("requirements.txt"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Couldn't parse requirement in `requirements.txt` at position 0
      Caused by: Expected direct URL (`https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl;python_version%20%3E%20'3.10'`) to end in a supported file extension: `.whl`, `.tar.gz`, `.zip`, `.tar.bz2`, `.tar.lz`, `.tar.lzma`, `.tar.xz`, `.tar.zst`, `.tar`, `.tbz`, `.tgz`, `.tlz`, or `.txz`
    iniconfig @ https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl;python_version > '3.10'
                ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    "###
    );

    Ok(())
}
