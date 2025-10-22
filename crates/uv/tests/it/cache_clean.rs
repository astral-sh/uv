use anyhow::Result;
use assert_cmd::prelude::*;
use assert_fs::prelude::*;

use uv_cache::Cache;
use uv_static::EnvVars;

use crate::common::{TestContext, uv_snapshot};

/// `cache clean` should remove all packages.
#[test]
fn clean_all() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("typing-extensions\niniconfig")?;

    // Install a requirement, to populate the cache.
    context
        .pip_sync()
        .arg("requirements.txt")
        .assert()
        .success();

    uv_snapshot!(context.with_filtered_counts().filters(), context.clean().arg("--verbose"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    DEBUG uv [VERSION] ([COMMIT] DATE)
    DEBUG Acquired lock for `[CACHE_DIR]/`
    Clearing cache at: [CACHE_DIR]/
    DEBUG Released lock at `[CACHE_DIR]/.lock`
    Removed [N] files ([SIZE])
    ");

    Ok(())
}

#[test]
fn clean_force() -> Result<()> {
    let context = TestContext::new("3.12").with_filtered_counts();

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("typing-extensions\niniconfig")?;

    // Install a requirement, to populate the cache.
    context
        .pip_sync()
        .arg("requirements.txt")
        .assert()
        .success();

    // When unlocked, `--force` should still take a lock
    uv_snapshot!(context.filters(), context.clean().arg("--verbose").arg("--force"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    DEBUG uv [VERSION] ([COMMIT] DATE)
    DEBUG Acquired lock for `[CACHE_DIR]/`
    Clearing cache at: [CACHE_DIR]/
    DEBUG Released lock at `[CACHE_DIR]/.lock`
    Removed [N] files ([SIZE])
    ");

    // Install a requirement, to re-populate the cache.
    context
        .pip_sync()
        .arg("requirements.txt")
        .assert()
        .success();

    // When locked, `--force` should proceed without blocking
    let _cache = uv_cache::Cache::from_path(context.cache_dir.path()).with_exclusive_lock();
    uv_snapshot!(context.filters(), context.clean().arg("--verbose").arg("--force"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    DEBUG uv [VERSION] ([COMMIT] DATE)
    DEBUG Lock is busy for `[CACHE_DIR]/`
    DEBUG Cache is currently in use, proceeding due to `--force`
    Clearing cache at: [CACHE_DIR]/
    Removed [N] files ([SIZE])
    ");

    Ok(())
}

/// `cache clean iniconfig` should remove a single package (`iniconfig`).
#[test]
fn clean_package_pypi() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("anyio\niniconfig")?;

    // Install a requirement, to populate the cache.
    context
        .pip_sync()
        .arg("requirements.txt")
        .assert()
        .success();

    // Assert that the `.rkyv` file is created for `iniconfig`.
    let rkyv = context
        .cache_dir
        .child("simple-v18")
        .child("pypi")
        .child("iniconfig.rkyv");
    assert!(
        rkyv.exists(),
        "Expected the `.rkyv` file to exist for `iniconfig`"
    );

    let filters: Vec<_> = context
        .filters()
        .into_iter()
        .chain([
            // The cache entry does not have a stable key, so we filter it out.
            (
                r"\[CACHE_DIR\](\\|\/)(.+)(\\|\/).*",
                "[CACHE_DIR]/$2/[ENTRY]",
            ),
            // The file count varies by operating system, so we filter it out.
            ("Removed \\d+ files?", "Removed [N] files"),
        ])
        .collect();

    uv_snapshot!(&filters, context.clean().arg("--verbose").arg("iniconfig"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    DEBUG uv [VERSION] ([COMMIT] DATE)
    DEBUG Acquired lock for `[CACHE_DIR]/`
    DEBUG Removing dangling cache entry: [CACHE_DIR]/archive-v0/[ENTRY]
    Removed [N] files ([SIZE])
    DEBUG Released lock at `[CACHE_DIR]/.lock`
    ");

    // Assert that the `.rkyv` file is removed for `iniconfig`.
    assert!(
        !rkyv.exists(),
        "Expected the `.rkyv` file to be removed for `iniconfig`"
    );

    // Running `uv cache prune` should have no effect.
    uv_snapshot!(&filters, context.prune().arg("--verbose"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    DEBUG uv [VERSION] ([COMMIT] DATE)
    DEBUG Acquired lock for `[CACHE_DIR]/`
    Pruning cache at: [CACHE_DIR]/
    No unused entries found
    DEBUG Released lock at `[CACHE_DIR]/.lock`
    ");

    Ok(())
}

/// `cache clean iniconfig` should remove a single package (`iniconfig`).
#[test]
fn clean_package_index() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("anyio\niniconfig")?;

    // Install a requirement, to populate the cache.
    context
        .pip_sync()
        .arg("requirements.txt")
        .arg("--index-url")
        .arg("https://test.pypi.org/simple")
        .assert()
        .success();

    // Assert that the `.rkyv` file is created for `iniconfig`.
    let rkyv = context
        .cache_dir
        .child("simple-v18")
        .child("index")
        .child("e8208120cae3ba69")
        .child("iniconfig.rkyv");
    assert!(
        rkyv.exists(),
        "Expected the `.rkyv` file to exist for `iniconfig`"
    );

    let filters: Vec<_> = context
        .filters()
        .into_iter()
        .chain([
            // The cache entry does not have a stable key, so we filter it out.
            (
                r"\[CACHE_DIR\](\\|\/)(.+)(\\|\/).*",
                "[CACHE_DIR]/$2/[ENTRY]",
            ),
            // The file count varies by operating system, so we filter it out.
            ("Removed \\d+ files?", "Removed [N] files"),
        ])
        .collect();

    uv_snapshot!(&filters, context.clean().arg("--verbose").arg("iniconfig"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    DEBUG uv [VERSION] ([COMMIT] DATE)
    DEBUG Acquired lock for `[CACHE_DIR]/`
    DEBUG Removing dangling cache entry: [CACHE_DIR]/archive-v0/[ENTRY]
    Removed [N] files ([SIZE])
    DEBUG Released lock at `[CACHE_DIR]/.lock`
    ");

    // Assert that the `.rkyv` file is removed for `iniconfig`.
    assert!(
        !rkyv.exists(),
        "Expected the `.rkyv` file to be removed for `iniconfig`"
    );

    Ok(())
}

#[test]
fn cache_timeout() -> () {
    let context = TestContext::new("3.12");

    // Simulate another uv process running and locking the cache, e.g., with a source build.
    let _cache = Cache::from_path(context.cache_dir.path()).with_exclusive_lock();

    uv_snapshot!(context.filters(), context.clean().env(EnvVars::UV_LOCK_TIMEOUT, "1"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Cache is currently in-use, waiting for other uv processes to finish (use `--force` to override)
    error: Timeout ([TIME]) when waiting for lock on `[CACHE_DIR]/` at `[CACHE_DIR]/.lock`, is another uv process running? You can set `UV_LOCK_TIMEOUT` to increase the timeout.
    ");
}
