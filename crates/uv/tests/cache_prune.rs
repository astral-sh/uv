#![cfg(all(feature = "python", feature = "pypi"))]

use std::process::Command;

use anyhow::Result;
use assert_cmd::prelude::*;
use assert_fs::prelude::*;

use common::uv_snapshot;

use crate::common::{get_bin, TestContext, INSTA_FILTERS};

mod common;

/// Create a `cache prune` command with options shared across scenarios.
fn prune_command(context: &TestContext) -> Command {
    let mut command = Command::new(get_bin());
    command
        .arg("cache")
        .arg("prune")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .env("VIRTUAL_ENV", context.venv.as_os_str())
        .current_dir(&context.temp_dir);

    if cfg!(all(windows, debug_assertions)) {
        // TODO(konstin): Reduce stack usage in debug mode enough that the tests pass with the
        // default windows stack of 1MB
        command.env("UV_STACK_SIZE", (8 * 1024 * 1024).to_string());
    }

    command
}

/// Create a `pip sync` command with options shared across scenarios.
fn sync_command(context: &TestContext) -> Command {
    let mut command = Command::new(get_bin());
    command
        .arg("pip")
        .arg("sync")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .env("VIRTUAL_ENV", context.venv.as_os_str())
        .current_dir(&context.temp_dir);

    if cfg!(all(windows, debug_assertions)) {
        // TODO(konstin): Reduce stack usage in debug mode enough that the tests pass with the
        // default windows stack of 1MB
        command.env("UV_STACK_SIZE", (8 * 1024 * 1024).to_string());
    }

    command
}

/// `cache prune` should be a no-op if there's nothing out-of-date in the cache.
#[test]
fn prune_no_op() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("anyio")?;

    // Install a requirement, to populate the cache.
    sync_command(&context)
        .arg("requirements.txt")
        .assert()
        .success();

    let filters = [(r"Pruning cache at: .*", "Pruning cache at: [CACHE_DIR]")]
        .into_iter()
        .chain(INSTA_FILTERS.to_vec())
        .collect::<Vec<_>>();

    uv_snapshot!(filters, prune_command(&context).arg("--verbose"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Pruning cache at: [CACHE_DIR]
    No unused entries found
    "###);

    Ok(())
}

/// `cache prune` should remove any stale top-level directories from the cache.
#[test]
fn prune_stale_directory() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("anyio")?;

    // Install a requirement, to populate the cache.
    sync_command(&context)
        .arg("requirements.txt")
        .assert()
        .success();

    // Add a stale directory to the cache.
    let simple = context.cache_dir.child("simple-v4");
    simple.create_dir_all()?;

    let filters = [
        (r"Pruning cache at: .*", "Pruning cache at: [CACHE_DIR]"),
        (
            r"Removing dangling cache entry: .*[\\|/]simple-v4",
            "Pruning cache at: [CACHE_DIR]/simple-v4",
        ),
    ]
    .into_iter()
    .chain(INSTA_FILTERS.to_vec())
    .collect::<Vec<_>>();

    uv_snapshot!(filters, prune_command(&context).arg("--verbose"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Pruning cache at: [CACHE_DIR]
    DEBUG Pruning cache at: [CACHE_DIR]/simple-v4
    Removed 1 directory
    "###);

    Ok(())
}

/// `cache prune` should remove any stale symlink from the cache.
#[test]
fn prune_stale_symlink() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("anyio")?;

    // Install a requirement, to populate the cache.
    sync_command(&context)
        .arg("requirements.txt")
        .assert()
        .success();

    // Remove the wheels directory, causing the symlink to become stale.
    let wheels = context.cache_dir.child("wheels-v0");
    fs_err::remove_dir_all(wheels)?;

    let filters = [
        (r"Pruning cache at: .*", "Pruning cache at: [CACHE_DIR]"),
        (
            r"Removing dangling cache entry: .*[\\|/]archive-v0[\\|/].*",
            "Pruning cache at: [CACHE_DIR]/archive-v0/anyio",
        ),
    ]
    .into_iter()
    .chain(INSTA_FILTERS.to_vec())
    .collect::<Vec<_>>();

    uv_snapshot!(filters, prune_command(&context).arg("--verbose"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Pruning cache at: [CACHE_DIR]
    DEBUG Pruning cache at: [CACHE_DIR]/archive-v0/anyio
    Removed 44 files ([SIZE])
    "###);

    Ok(())
}
