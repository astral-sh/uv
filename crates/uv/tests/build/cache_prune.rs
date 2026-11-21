use anyhow::Result;
use assert_cmd::prelude::*;
use assert_fs::prelude::*;
use indoc::indoc;

use uv_cache::CacheBucket;
use uv_static::EnvVars;

use uv_test::uv_snapshot;

/// `cache prune` should be a no-op if there's nothing out-of-date in the cache.
#[test]
fn prune_no_op() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("anyio")?;

    // Install a requirement, to populate the cache.
    context
        .pip_sync()
        .arg("requirements.txt")
        .assert()
        .success();

    uv_snapshot!(context.filters(), context.prune().arg("--verbose"), @"
    exit_code: 0 (success)
    ----- stderr -----
    DEBUG Searching for user configuration in: [UV_USER_CONFIG_DIR]/uv.toml
    DEBUG uv [VERSION] ([COMMIT] DATE)
    Pruning cache at: [CACHE_DIR]/
    No unused entries found
    ");

    Ok(())
}

/// Cache pruning should not count storage retained by another hardlink.
#[cfg(unix)]
#[test]
fn prune_hardlinked_file() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    // Keep both hardlinks on the selected filesystem.
    let retained = context.cache_dir.path().with_file_name("retained.bin");
    fs_err::write(&retained, vec![42; 1024 * 1024])?;
    fs_err::OpenOptions::new()
        .write(true)
        .open(&retained)?
        .sync_all()?;

    let stale = context.cache_dir.child("stale-v0");
    stale.create_dir_all()?;
    fs_err::hard_link(&retained, stale.child("hardlinked.bin"))?;

    // Counting the externally retained hardlink would incorrectly report 1.0MiB.
    uv_snapshot!(context.filters(), context.prune(), @"
    exit_code: 0 (success)
    ----- stderr -----
    Pruning cache at: [CACHE_DIR]/
    Removed 1 file (0B)
    ");

    stale.create_dir_all()?;
    fs_err::hard_link(&retained, stale.child("hardlinked.bin"))?;

    uv_snapshot!(context.filters(), context.prune().arg("--preview-features").arg("cache-physical-space"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Pruning cache at: [CACHE_DIR]/
    Removed 1 file (0B)
    ");

    assert!(retained.is_file());

    Ok(())
}

/// `cache prune` should fall back to logical space on unsupported filesystems.
#[cfg(unix)]
#[test]
fn prune_physical_space_unsupported_fs() -> Result<()> {
    let Some(context) = uv_test::test_context!("3.12").with_cache_on_alt_fs()? else {
        return Ok(());
    };

    let stale = context.cache_dir.child("stale-v0");
    stale.create_dir_all()?;
    stale
        .child("cached.bin")
        .write_binary(&vec![42; 1024 * 1024])?;

    uv_snapshot!(context.filters(), context.prune().arg("--preview-features").arg("cache-physical-space"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Pruning cache at: [ALT_FS]/[CACHE_DIR]/
    Removed 1 file (1.0MiB)
    ");

    Ok(())
}

/// `cache prune` should remove any stale top-level directories from the cache.
#[test]
fn prune_stale_directory() -> Result<()> {
    let context = uv_test::test_context!("3.12");

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

    uv_snapshot!(context.filters(), context.prune().arg("--verbose"), @"
    exit_code: 0 (success)
    ----- stderr -----
    DEBUG Searching for user configuration in: [UV_USER_CONFIG_DIR]/uv.toml
    DEBUG uv [VERSION] ([COMMIT] DATE)
    Pruning cache at: [CACHE_DIR]/
    DEBUG Removing dangling cache bucket: [CACHE_DIR]/simple-v4
    Removed 1 directory (0B)
    ");

    Ok(())
}

/// `cache prune` should preserve cached Python downloads.
#[test]
fn prune_python_downloads() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let python_cache = context.cache_bucket(CacheBucket::Python);
    python_cache.create_dir_all()?;
    let download = python_cache.child("python.tar.gz");
    download.write_binary(b"cached Python download")?;

    uv_snapshot!(context.filters(), context.prune(), @"
    exit_code: 0 (success)
    ----- stderr -----
    Pruning cache at: [CACHE_DIR]/
    No unused entries found
    ");

    assert!(download.is_file());

    Ok(())
}

/// `cache prune` should remove all cached environments from the cache.
#[test]
fn prune_cached_env() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_sizes_and_units()
        // The cache entry does not have a stable key, so we filter it out.
        .with_filter((
            r"\[CACHE_DIR\](\\|\/)(.*?)(\\|\/).*",
            "[CACHE_DIR]/$2/[ENTRY]",
        ));
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("pytest@8.0.0")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    exit_code: 0 (success)
    ----- stdout -----
    pytest 8.0.0

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + iniconfig==2.0.0
     + packaging==24.0
     + pluggy==1.4.0
     + pytest==8.0.0
    ");

    uv_snapshot!(context.filters(), context.prune().arg("--verbose"), @"
    exit_code: 0 (success)
    ----- stderr -----
    DEBUG Searching for user configuration in: [UV_USER_CONFIG_DIR]/uv.toml
    DEBUG uv [VERSION] ([COMMIT] DATE)
    Pruning cache at: [CACHE_DIR]/
    DEBUG Removing cached environment: [CACHE_DIR]/environments-[VERSION]/[ENTRY]
    DEBUG Removing dangling cache archive: [CACHE_DIR]/archive-[VERSION]/[ENTRY]
    Removed [N] files ([SIZE])
    ");
}

/// `cache prune` should remove any stale symlink from the cache.
#[test]
fn prune_stale_symlink() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_filtered_sizes_and_units();

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("anyio")?;

    // Install a requirement, to populate the cache.
    context
        .pip_sync()
        .arg("requirements.txt")
        .assert()
        .success();

    // Remove the wheels directory, causing the symlink to become stale.
    let wheels = context.cache_bucket(CacheBucket::Wheels);
    fs_err::remove_dir_all(wheels)?;

    let filters: Vec<_> = context
        .filters()
        .into_iter()
        .chain([
            // The cache entry does not have a stable key, so we filter it out
            (
                r"\[CACHE_DIR\](\\|\/)(.*?)(\\|\/).*",
                "[CACHE_DIR]/$2/[ENTRY]",
            ),
        ])
        .collect();

    uv_snapshot!(filters, context.prune().arg("--verbose"), @"
    exit_code: 0 (success)
    ----- stderr -----
    DEBUG Searching for user configuration in: [UV_USER_CONFIG_DIR]/uv.toml
    DEBUG uv [VERSION] ([COMMIT] DATE)
    Pruning cache at: [CACHE_DIR]/
    DEBUG Removing dangling cache archive: [CACHE_DIR]/archive-[VERSION]/[ENTRY]
    Removed 44 files ([SIZE])
    ");

    Ok(())
}

#[tokio::test]
async fn prune_force() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_filtered_counts();

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(indoc! {"
        typing-extensions
        iniconfig
    "})?;

    // Install a requirement, to populate the cache.
    context
        .pip_sync()
        .arg("requirements.txt")
        .assert()
        .success();

    // When unlocked, `--force` should still take a lock
    uv_snapshot!(context.filters(), context.prune().arg("--verbose").arg("--force"), @"
    exit_code: 0 (success)
    ----- stderr -----
    DEBUG Searching for user configuration in: [UV_USER_CONFIG_DIR]/uv.toml
    DEBUG uv [VERSION] ([COMMIT] DATE)
    Pruning cache at: [CACHE_DIR]/
    No unused entries found
    ");

    // Add a stale directory to the cache.
    let stale = context.cache_dir.child("stale");
    stale.create_dir_all()?;

    // When locked, `--force` should proceed without blocking
    let _cache = uv_cache::Cache::from_path(context.cache_dir.path())
        .with_exclusive_lock()
        .await;
    uv_snapshot!(context.filters(), context.prune().arg("--verbose").arg("--force"), @"
    exit_code: 0 (success)
    ----- stderr -----
    DEBUG Searching for user configuration in: [UV_USER_CONFIG_DIR]/uv.toml
    DEBUG uv [VERSION] ([COMMIT] DATE)
    DEBUG Lock is busy for `[CACHE_DIR]/`
    DEBUG Cache is currently in use, proceeding due to `--force`
    Pruning cache at: [CACHE_DIR]/
    DEBUG Removing dangling cache bucket: [CACHE_DIR]/stale
    Removed 1 directory (0B)
    ");

    Ok(())
}

/// `cache prune --ci` should be a no-op if the cache does not contain any buckets.
#[test]
fn prune_ci_empty_cache() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    context.cache_dir.create_dir_all()?;

    uv_snapshot!(context.filters(), context.prune().arg("--ci"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Pruning cache at: [CACHE_DIR]/
    No unused entries found
    ");

    Ok(())
}

/// `cache prune --ci` should remove all unzipped archives.
#[test]
fn prune_unzipped() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_exclude_newer("2025-01-01T00:00Z")
        .with_filtered_file_counts()
        .with_filtered_sizes_and_units();

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(indoc! { r"
        source-distribution==0.0.1
        iniconfig
    " })?;

    // Install a requirement, to populate the cache.
    uv_snapshot!(context.filters(), context.pip_install().arg("-r").arg("requirements.txt").arg("--reinstall"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + iniconfig==2.0.0
     + source-distribution==0.0.1
    ");

    uv_snapshot!(context.filters(), context.prune().arg("--ci"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Pruning cache at: [CACHE_DIR]/
    Removed [N] files ([SIZE])
    ");

    context.venv().arg("--clear").assert().success();

    // Reinstalling the source distribution should not require re-downloading the source
    // distribution.
    requirements_txt.write_str(indoc! { r"
        source-distribution==0.0.1
    " })?;
    uv_snapshot!(context.filters(), context.pip_install().arg("-r").arg("requirements.txt").arg("--offline"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + source-distribution==0.0.1
    ");

    // But reinstalling the other package should require a download, since we pruned the wheel.
    requirements_txt.write_str(indoc! { r"
        iniconfig
    " })?;
    uv_snapshot!(context.filters(), context.pip_install().arg("-r").arg("requirements.txt").arg("--offline"), @"
    exit_code: 1 (failure)
    ----- stderr -----
    error: No solution found when resolving dependencies
      cause: Because all versions of iniconfig need to be downloaded from a registry and you require iniconfig, we can conclude that your requirements are unsatisfiable.

    hint: Packages were unavailable because the network was disabled. When the network is disabled, registry packages may only be read from the cache.
    ");

    Ok(())
}

/// `cache prune` should remove any stale source distribution revisions.
#[test]
fn prune_stale_revision() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_file_counts()
        .with_filtered_sizes_and_units()
        // The cache entry does not have a stable key, so we filter it out.
        .with_filter((
            r"\[CACHE_DIR\](\\|\/)(.*?)(\\|\/).*",
            "[CACHE_DIR]/$2/[ENTRY]",
        ));

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"
        "#,
    )?;

    context
        .temp_dir
        .child("src")
        .child("project")
        .child("__init__.py")
        .touch()?;
    context.temp_dir.child("README").touch()?;

    // Install the same package twice, with `--reinstall`.
    uv_snapshot!(context.filters(), context
        .pip_install()
        .arg(".")
        .arg("--reinstall"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + project==0.1.0 (from file://[TEMP_DIR]/)
    ");

    uv_snapshot!(context.filters(), context
        .pip_install()
        .arg(".")
        .arg("--reinstall"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     ~ project==0.1.0 (from file://[TEMP_DIR]/)
    ");

    // Pruning should remove the unused revision.
    uv_snapshot!(context.filters(), context.prune().arg("--verbose"), @"
    exit_code: 0 (success)
    ----- stderr -----
    DEBUG Found workspace root: [TEMP_DIR]/
    DEBUG Adding root workspace member: [TEMP_DIR]/
    DEBUG Skipping `pyproject.toml` in `[TEMP_DIR]/` (no `[tool]` section)
    DEBUG Searching for user configuration in: [UV_USER_CONFIG_DIR]/uv.toml
    DEBUG uv [VERSION] ([COMMIT] DATE)
    Pruning cache at: [CACHE_DIR]/
    DEBUG Removing dangling source revision: [CACHE_DIR]/sdists-[VERSION]/[ENTRY]
    DEBUG Removing dangling cache archive: [CACHE_DIR]/archive-[VERSION]/[ENTRY]
    Removed [N] files ([SIZE])
    ");

    // Uninstall and reinstall the package. We should use the cached version.
    uv_snapshot!(context.filters(), context
        .pip_uninstall()
        .arg("."), @"
    exit_code: 0 (success)
    ----- stderr -----
    Uninstalled 1 package in [TIME]
     - project==0.1.0 (from file://[TEMP_DIR]/)
    ");

    uv_snapshot!(context.filters(), context
        .pip_install()
        .arg("."), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + project==0.1.0 (from file://[TEMP_DIR]/)
    ");

    Ok(())
}

/// Content-addressed cache entries should remain reachable across equivalent stale revisions.
#[test]
fn prune_stale_revision_content_addressed_cache() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_file_counts()
        .with_filtered_sizes_and_units()
        // The cache entry does not have a stable key, so we filter it out.
        .with_filter((
            r"\[CACHE_DIR\](\\|\/)(.*?)(\\|\/).*",
            "[CACHE_DIR]/$2/[ENTRY]",
        ));

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"
        "#,
    )?;

    context
        .temp_dir
        .child("src")
        .child("project")
        .child("__init__.py")
        .touch()?;
    context.temp_dir.child("README").touch()?;

    // Install the same package twice, with `--reinstall`.
    uv_snapshot!(context.filters(), context
        .pip_install()
        .env(EnvVars::UV_PREVIEW_FEATURES, "content-addressed-cache")
        .arg(".")
        .arg("--reinstall"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + project==0.1.0 (from file://[TEMP_DIR]/)
    ");

    uv_snapshot!(context.filters(), context
        .pip_install()
        .env(EnvVars::UV_PREVIEW_FEATURES, "content-addressed-cache")
        .arg(".")
        .arg("--reinstall"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     ~ project==0.1.0 (from file://[TEMP_DIR]/)
    ");

    // Pruning should remove the unused revision but retain the shared archive.
    uv_snapshot!(context.filters(), context.prune().arg("--verbose"), @"
    exit_code: 0 (success)
    ----- stderr -----
    DEBUG Found workspace root: [TEMP_DIR]/
    DEBUG Adding root workspace member: [TEMP_DIR]/
    DEBUG Skipping `pyproject.toml` in `[TEMP_DIR]/` (no `[tool]` section)
    DEBUG Searching for user configuration in: [UV_USER_CONFIG_DIR]/uv.toml
    DEBUG uv [VERSION] ([COMMIT] DATE)
    Pruning cache at: [CACHE_DIR]/
    DEBUG Removing dangling source revision: [CACHE_DIR]/sdists-[VERSION]/[ENTRY]
    Removed [N] files ([SIZE])
    ");

    // Uninstall and reinstall the package. We should use the cached version.
    uv_snapshot!(context.filters(), context
        .pip_uninstall()
        .arg("."), @"
    exit_code: 0 (success)
    ----- stderr -----
    Uninstalled 1 package in [TIME]
     - project==0.1.0 (from file://[TEMP_DIR]/)
    ");

    uv_snapshot!(context.filters(), context
        .pip_install()
        .env(EnvVars::UV_PREVIEW_FEATURES, "content-addressed-cache")
        .arg("."), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + project==0.1.0 (from file://[TEMP_DIR]/)
    ");

    Ok(())
}

/// `cache prune` should remove any temporary build environments left in the cache.
#[test]
fn prune_temporary_build_environment() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_sizes_and_units()
        .with_filter((
            r"\[CACHE_DIR\](\\|\/)(.*?)(\\|\/).*",
            "[CACHE_DIR]/$2/[ENTRY]",
        ));

    // Populate the cache with a temporary build environment.
    let builds = context
        .cache_bucket(CacheBucket::Builds)
        .child(".tmp123456");
    builds.create_dir_all()?;
    builds.child("pyvenv.cfg").write_str("home = /usr/bin")?;

    uv_snapshot!(context.filters(), context.prune().arg("--verbose"), @"
    exit_code: 0 (success)
    ----- stderr -----
    DEBUG Searching for user configuration in: [UV_USER_CONFIG_DIR]/uv.toml
    DEBUG uv [VERSION] ([COMMIT] DATE)
    Pruning cache at: [CACHE_DIR]/
    DEBUG Removing temporary build environment: [CACHE_DIR]/builds-[VERSION]/[ENTRY]
    Removed [N] files ([SIZE])
    ");

    assert!(!builds.exists());

    Ok(())
}

/// `cache prune --unused` should retain the cache entries that a recorded environment references.
#[test]
fn prune_unused_keeps_installed_entries() {
    let context = uv_test::test_context!("3.12");

    context.venv().arg("--clear").assert().success();
    context.pip_install().arg("iniconfig").assert().success();

    uv_snapshot!(context.filters(), context.prune().arg("--unused"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Pruning cache at: [CACHE_DIR]/
    No unused entries found
    ");
}

/// `cache prune --unused` should remove the cache entries that no recorded environment references.
#[test]
fn prune_unused_removes_unreferenced_entries() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_file_counts()
        .with_filtered_sizes_and_units();

    context.venv().arg("--clear").assert().success();
    context.pip_install().arg("iniconfig").assert().success();

    // Reusing the environment records the packages that are now installed, which no longer include
    // `iniconfig`.
    context.venv().arg("--clear").assert().success();
    context.pip_install().arg("anyio").assert().success();

    uv_snapshot!(context.filters(), context.prune().arg("--unused"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Pruning cache at: [CACHE_DIR]/
    Removed [N] files ([SIZE])
    ");
}

/// `cache prune --unused --dry-run` should report the entries without removing them.
#[test]
fn prune_unused_dry_run() {
    let context = uv_test::test_context!("3.12")
        .with_filter((
            "Would remove \\d+ files?".to_string(),
            "Would remove [N] files".to_string(),
        ))
        .with_filtered_file_counts()
        .with_filtered_sizes_and_units();

    context.venv().arg("--clear").assert().success();
    context.pip_install().arg("iniconfig").assert().success();
    context.venv().arg("--clear").assert().success();
    context.pip_install().arg("anyio").assert().success();

    uv_snapshot!(context.filters(), context.prune().arg("--unused").arg("--dry-run"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Checking for unused cache entries in: [CACHE_DIR]/
    Would remove [N] files ([SIZE])
    ");

    // The entry is still cached, so pruning it for real reports the same removal.
    uv_snapshot!(context.filters(), context.prune().arg("--unused"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Pruning cache at: [CACHE_DIR]/
    Removed [N] files ([SIZE])
    ");
}

/// `cache prune --unused` should skip the removal when the cache holds no environment records.
#[test]
fn prune_unused_requires_an_environment() {
    let context = uv_test::test_context!("3.12");

    context.venv().arg("--clear").assert().success();
    context
        .pip_install()
        .arg("iniconfig")
        .env(EnvVars::UV_CACHE_TRACKING, "false")
        .assert()
        .success();

    assert!(
        !context
            .cache_dir
            .child("environment-refs-v0")
            .path()
            .exists()
    );

    uv_snapshot!(context.filters(), context.prune().arg("--unused"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Pruning cache at: [CACHE_DIR]/
    warning: Skipping removal of unused cache entries: no recorded environments; run `uv sync` in a project, or pass `--environment` to include an environment
    No unused entries found
    ");
}

/// `cache prune --unused --environment` should consider an environment that the cache hasn't
/// recorded.
#[test]
fn prune_unused_environment() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    // Disabling recording in the project's configuration leaves the cache without any records.
    context
        .temp_dir
        .child("uv.toml")
        .write_str("cache-tracking = false\n")?;

    context.venv().arg("--clear").assert().success();
    context.pip_install().arg("iniconfig").assert().success();

    uv_snapshot!(context.filters(), context.prune().arg("--unused"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Pruning cache at: [CACHE_DIR]/
    warning: Skipping removal of unused cache entries: no recorded environments; run `uv sync` in a project, or pass `--environment` to include an environment
    No unused entries found
    ");

    uv_snapshot!(context.filters(), context
        .prune()
        .arg("--unused")
        .arg("--environment")
        .arg(".venv"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Pruning cache at: [CACHE_DIR]/
    No unused entries found
    ");

    Ok(())
}

/// `--dry-run` and `--environment` only apply to `--unused`, so they cannot be passed alone.
#[test]
fn prune_unused_requires_unused() {
    let context = uv_test::test_context!("3.12");

    uv_snapshot!(context.filters(), context.prune().arg("--dry-run"), @r"
    exit_code: 2 (failure)
    ----- stderr -----
    error: the following required arguments were not provided:
      --unused

    Usage: uv cache prune --unused --cache-dir [CACHE_DIR] --dry-run

    For more information, try '--help'.
    ");

    uv_snapshot!(context.filters(), context.prune().arg("--environment").arg(".venv"), @r"
    exit_code: 2 (failure)
    ----- stderr -----
    error: the following required arguments were not provided:
      --unused

    Usage: uv cache prune --unused --cache-dir [CACHE_DIR] --environment <PATH>

    For more information, try '--help'.
    ");
}
