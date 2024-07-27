#![cfg(all(feature = "python", feature = "pypi"))]

use anyhow::Result;
use assert_cmd::prelude::*;
use assert_fs::prelude::*;
use common::uv_snapshot;
use indoc::indoc;

use crate::common::TestContext;

mod common;

/// `cache prune` should be a no-op if there's nothing out-of-date in the cache.
#[test]
fn prune_no_op() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("anyio")?;

    // Install a requirement, to populate the cache.
    context
        .pip_sync()
        .arg("requirements.txt")
        .assert()
        .success();

    uv_snapshot!(context.filters(), context.prune().arg("--verbose"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    DEBUG uv [VERSION] ([COMMIT] DATE)
    Pruning cache at: [CACHE_DIR]/
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
    context
        .pip_sync()
        .arg("requirements.txt")
        .assert()
        .success();

    // Add a stale directory to the cache.
    let simple = context.cache_dir.child("simple-v4");
    simple.create_dir_all()?;

    uv_snapshot!(context.filters(), context.prune().arg("--verbose"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    DEBUG uv [VERSION] ([COMMIT] DATE)
    Pruning cache at: [CACHE_DIR]/
    DEBUG Removing dangling cache entry: [CACHE_DIR]/simple-v4
    Removed 1 directory
    "###);

    Ok(())
}

/// `cache prune` should remove all cached environments from the cache.
#[test]
fn prune_cached_env() {
    let context = TestContext::new("3.12").with_filtered_counts();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("pytest@8.0.0")
        .arg("--version")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    pytest 8.0.0

    ----- stderr -----
    warning: `uv tool run` is experimental and may change without warning
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + iniconfig==2.0.0
     + packaging==24.0
     + pluggy==1.4.0
     + pytest==8.0.0
    "###);

    let filters: Vec<_> = context
        .filters()
        .into_iter()
        .chain([
            // The cache entry does not have a stable key, so we filter it out
            (
                r"\[CACHE_DIR\](\\|\/)(.+)(\\|\/).*",
                "[CACHE_DIR]/$2/[ENTRY]",
            ),
        ])
        .collect();

    uv_snapshot!(filters, context.prune().arg("--verbose"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    DEBUG uv [VERSION] ([COMMIT] DATE)
    Pruning cache at: [CACHE_DIR]/
    DEBUG Removing dangling cache entry: [CACHE_DIR]/environments-v1/[ENTRY]
    DEBUG Removing dangling cache entry: [CACHE_DIR]/archive-v0/[ENTRY]
    Removed [N] files ([SIZE])
    "###);
}

/// `cache prune` should remove any stale symlink from the cache.
#[test]
fn prune_stale_symlink() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("anyio")?;

    // Install a requirement, to populate the cache.
    context
        .pip_sync()
        .arg("requirements.txt")
        .assert()
        .success();

    // Remove the wheels directory, causing the symlink to become stale.
    let wheels = context.cache_dir.child("wheels-v1");
    fs_err::remove_dir_all(wheels)?;

    let filters: Vec<_> = context
        .filters()
        .into_iter()
        .chain([
            // The cache entry does not have a stable key, so we filter it out
            (
                r"\[CACHE_DIR\](\\|\/)(.+)(\\|\/).*",
                "[CACHE_DIR]/$2/[ENTRY]",
            ),
        ])
        .collect();

    uv_snapshot!(filters, context.prune().arg("--verbose"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    DEBUG uv [VERSION] ([COMMIT] DATE)
    Pruning cache at: [CACHE_DIR]/
    DEBUG Removing dangling cache entry: [CACHE_DIR]/archive-v0/[ENTRY]
    Removed 44 files ([SIZE])
    "###);

    Ok(())
}

/// `cache prune --ci` should remove all unzipped archives.
#[test]
fn prune_unzipped() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(indoc! { r"
        source-distribution==0.0.1
        iniconfig
    " })?;

    // Install a requirement, to populate the cache.
    uv_snapshot!(context.filters(), context.pip_sync().env_remove("UV_EXCLUDE_NEWER").arg("requirements.txt").arg("--reinstall"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + iniconfig==2.0.0
     + source-distribution==0.0.1
    "###);

    uv_snapshot!(context.filters(), context.prune().arg("--ci"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Pruning cache at: [CACHE_DIR]/
    Removed 163 files ([SIZE])
    "###);

    // Reinstalling the source distribution should not require re-downloading the source
    // distribution.
    requirements_txt.write_str(indoc! { r"
        source-distribution==0.0.1
    " })?;
    uv_snapshot!(context.filters(), context.pip_sync().env_remove("UV_EXCLUDE_NEWER").arg("requirements.txt").arg("--reinstall").arg("--offline"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 2 packages in [TIME]
    Installed 1 package in [TIME]
     - iniconfig==2.0.0
     - source-distribution==0.0.1
     + source-distribution==0.0.1
    "###);

    requirements_txt.write_str(indoc! { r"
        iniconfig
    " })?;
    uv_snapshot!(context.filters(), context.pip_sync().env_remove("UV_EXCLUDE_NEWER").arg("requirements.txt").arg("--reinstall").arg("--offline"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only the following versions of iniconfig are available:
              iniconfig<=0.1
              iniconfig>=1.0.0
          and iniconfig==0.1 network connectivity is disabled, but the metadata wasn't found in the cache, we can conclude that iniconfig<1.0.0 cannot be used.
          And because iniconfig==1.0.0 network connectivity is disabled, but the metadata wasn't found in the cache and iniconfig==1.0.1 network connectivity is disabled, but the metadata wasn't found in the cache, we can conclude that iniconfig<1.1.0 cannot be used.
          And because iniconfig==1.1.0 network connectivity is disabled, but the metadata wasn't found in the cache and iniconfig==1.1.1 network connectivity is disabled, but the metadata wasn't found in the cache, we can conclude that iniconfig<2.0.0 cannot be used.
          And because iniconfig==2.0.0 network connectivity is disabled, but the metadata wasn't found in the cache and you require iniconfig, we can conclude that the requirements are unsatisfiable.

          hint: Pre-releases are available for iniconfig in the requested range (e.g., 0.2.dev0), but pre-releases weren't enabled (try: `--prerelease=allow`)

          hint: Packages were unavailable because the network was disabled
    "###);

    Ok(())
}
