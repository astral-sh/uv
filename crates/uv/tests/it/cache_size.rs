use anyhow::Result;
use assert_cmd::assert::OutputAssertExt;
use assert_fs::prelude::*;

use crate::common::{TestContext, uv_snapshot};

/// Test that `cache size` returns 0 for an empty cache directory (raw output).
#[test]
fn cache_size_empty_raw() -> Result<()> {
    let context = TestContext::new("3.12");

    // Clean cache first to ensure truly empty state
    context.clean().assert().success();

    uv_snapshot!(context.filters(), context.cache_size(), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    0

    ----- stderr -----
    ");

    Ok(())
}

/// Test that `cache size` returns raw bytes after installing packages.
#[test]
fn cache_size_with_packages_raw() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("anyio")?;

    // Install a requirement to populate the cache
    context
        .pip_sync()
        .arg("requirements.txt")
        .assert()
        .success();

    // Check cache size is now positive (raw bytes)
    uv_snapshot!(context.with_filtered_cache_size().filters(), context.cache_size(), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    [SIZE]

    ----- stderr -----
    ");

    Ok(())
}

/// Test that `cache size --human` returns "0 B" for empty cache.
#[test]
fn cache_size_empty_human() -> Result<()> {
    let context = TestContext::new("3.12");

    // Clean cache first to ensure truly empty state
    context.clean().assert().success();

    uv_snapshot!(context.filters(), context.cache_size().arg("--human"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    0 B

    ----- stderr -----
    ");

    Ok(())
}

/// Test that `cache size --human` returns human-readable format after installing packages.
#[test]
fn cache_size_with_packages_human() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("anyio")?;

    // Install a requirement to populate the cache
    context
        .pip_sync()
        .arg("requirements.txt")
        .assert()
        .success();

    // Check cache size with --human flag
    uv_snapshot!(context.with_filtered_cache_size().filters(), context.cache_size().arg("--human"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    [SIZE]

    ----- stderr -----
    ");

    Ok(())
}
