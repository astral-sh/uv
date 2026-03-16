#![cfg(all(feature = "python", feature = "pypi"))]

//! Tests for cache compatibility across uv versions.
//!
//! These tests install the latest released uv via `uv tool install`, then exercise cache
//! operations with it and verify the current version can read/write that cache correctly.

use anyhow::Result;
use assert_cmd::prelude::*;
use assert_fs::prelude::*;

use crate::common::{get_bin, TestContext};

mod common;

/// Install the latest released uv via `uv tool install` and return the path to the binary.
fn install_previous_uv(context: &TestContext) -> std::path::PathBuf {
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    let mut command = std::process::Command::new(get_bin());
    command
        .arg("tool")
        .arg("install")
        .arg("uv")
        .env_remove("VIRTUAL_ENV")
        .env("UV_NO_WRAP", "1")
        .env("HOME", context.home_dir.as_os_str())
        .env("UV_PYTHON_INSTALL_DIR", "")
        .env("UV_TEST_PYTHON_PATH", context.python_path())
        .env_remove("UV_EXCLUDE_NEWER")
        .env_remove("UV_CACHE_DIR")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .current_dir(context.temp_dir.path());
    command.assert().success();

    bin_dir.child("uv").to_path_buf()
}

/// Create a command that runs the previously-installed uv binary.
///
/// The previous uv picks up the test cache via `UV_CACHE_DIR`.
fn previous_uv_cmd(context: &TestContext, uv_bin: &std::path::Path) -> std::process::Command {
    let mut command = std::process::Command::new(uv_bin);
    command
        .env_remove("VIRTUAL_ENV")
        .env("UV_NO_WRAP", "1")
        .env("HOME", context.home_dir.as_os_str())
        .env("UV_CACHE_DIR", context.cache_dir.path())
        .current_dir(context.temp_dir.path());

    command
}

fn previous_uv_pip_install(
    context: &TestContext,
    uv_bin: &std::path::Path,
    package: &str,
    extra_args: &[&str],
) -> assert_cmd::assert::Assert {
    let mut command = previous_uv_cmd(context, uv_bin);
    command
        .arg("pip")
        .arg("install")
        .arg(package)
        .arg("--exclude-newer")
        .arg("2024-03-25T00:00:00Z")
        .env("VIRTUAL_ENV", context.venv.as_os_str());
    for arg in extra_args {
        command.arg(arg);
    }
    command.assert().success()
}

fn previous_uv_pip_show(
    context: &TestContext,
    uv_bin: &std::path::Path,
    package: &str,
) -> assert_cmd::assert::Assert {
    previous_uv_cmd(context, uv_bin)
        .arg("pip")
        .arg("show")
        .arg(package)
        .env("VIRTUAL_ENV", context.venv.as_os_str())
        .assert()
        .success()
}

fn previous_uv_cache_clean(
    context: &TestContext,
    uv_bin: &std::path::Path,
) -> assert_cmd::assert::Assert {
    previous_uv_cmd(context, uv_bin)
        .arg("cache")
        .arg("clean")
        .assert()
        .success()
}

/// Full cache compatibility sequence for a package.
///
/// Exercises install, reinstall, refresh, clean, and no-binary operations across
/// the previous and current uv versions sharing the same cache.
fn check_cache_compat(context: &TestContext, uv_bin: &std::path::Path, package: &str) {
    // Install with previous uv to populate the cache.
    previous_uv_pip_install(context, uv_bin, package, &[]);
    previous_uv_pip_show(context, uv_bin, package);

    // Install with current uv (audit, should use cache).
    context.pip_install().arg(package).assert().success();
    context.pip_show().arg(package).assert().success();

    // Reinstall with current uv.
    context
        .pip_install()
        .arg(package)
        .arg("--reinstall")
        .assert()
        .success();
    context.pip_show().arg(package).assert().success();

    // Reinstall with current uv + targeted refresh.
    context
        .pip_install()
        .arg(package)
        .arg("--reinstall-package")
        .arg(package)
        .arg("--refresh-package")
        .arg(package)
        .assert()
        .success();
    context.pip_show().arg(package).assert().success();

    // Reinstall with current uv (post-refresh).
    context
        .pip_install()
        .arg(package)
        .arg("--reinstall")
        .assert()
        .success();
    context.pip_show().arg(package).assert().success();

    // Reinstall with previous uv.
    previous_uv_pip_install(context, uv_bin, package, &["--reinstall"]);
    previous_uv_pip_show(context, uv_bin, package);

    // Clean cache with previous uv.
    previous_uv_cache_clean(context, uv_bin);

    // Install with previous uv using --no-binary (local build).
    previous_uv_pip_install(context, uv_bin, package, &["--no-binary", package]);
    previous_uv_pip_show(context, uv_bin, package);

    // Reinstall with current uv.
    context
        .pip_install()
        .arg(package)
        .arg("--reinstall")
        .assert()
        .success();
    context.pip_show().arg(package).assert().success();
}

#[test]
fn cache_compat_anyio() -> Result<()> {
    let context = TestContext::new("3.12");
    let uv_bin = install_previous_uv(&context);
    check_cache_compat(&context, &uv_bin, "anyio");
    Ok(())
}

#[test]
fn cache_compat_flask() -> Result<()> {
    let context = TestContext::new("3.12");
    let uv_bin = install_previous_uv(&context);
    check_cache_compat(&context, &uv_bin, "flask");
    Ok(())
}
