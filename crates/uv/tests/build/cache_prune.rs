use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result};
use assert_cmd::prelude::*;
use assert_fs::prelude::*;
use filetime::FileTime;
use indoc::indoc;
use insta::allow_duplicates;

use uv_cache::{ArchiveId, Cache};
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
    DEBUG Searching for user configuration in: `[UV_USER_CONFIG_DIR]/uv.toml`
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
    DEBUG Searching for user configuration in: `[UV_USER_CONFIG_DIR]/uv.toml`
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

    let python_cache = context.cache_dir.child("python-v0");
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
    DEBUG Searching for user configuration in: `[UV_USER_CONFIG_DIR]/uv.toml`
    DEBUG uv [VERSION] ([COMMIT] DATE)
    Pruning cache at: [CACHE_DIR]/
    DEBUG Removing cached environment: [CACHE_DIR]/environments-v2/[ENTRY]
    DEBUG Removing dangling cache archive: [CACHE_DIR]/archive-v0/[ENTRY]
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
    let wheels = context.cache_dir.child("wheels-v6");
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
    DEBUG Searching for user configuration in: `[UV_USER_CONFIG_DIR]/uv.toml`
    DEBUG uv [VERSION] ([COMMIT] DATE)
    Pruning cache at: [CACHE_DIR]/
    DEBUG Removing dangling cache archive: [CACHE_DIR]/archive-v0/[ENTRY]
    Removed 44 files ([SIZE])
    ");

    Ok(())
}

#[tokio::test]
async fn prune_force() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_filtered_counts();

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("typing-extensions\niniconfig")?;

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
    DEBUG Searching for user configuration in: `[UV_USER_CONFIG_DIR]/uv.toml`
    DEBUG uv [VERSION] ([COMMIT] DATE)
    Pruning cache at: [CACHE_DIR]/
    No unused entries found
    ");

    // Add a stale directory to the cache.
    let simple = context.cache_dir.child("simple-v4");
    simple.create_dir_all()?;

    // When locked, `--force` should proceed without blocking
    let _cache = uv_cache::Cache::from_path(context.cache_dir.path())
        .with_exclusive_lock()
        .await;
    uv_snapshot!(context.filters(), context.prune().arg("--verbose").arg("--force"), @"
    exit_code: 0 (success)
    ----- stderr -----
    DEBUG Searching for user configuration in: `[UV_USER_CONFIG_DIR]/uv.toml`
    DEBUG uv [VERSION] ([COMMIT] DATE)
    DEBUG Lock is busy for `[CACHE_DIR]/`
    DEBUG Cache is currently in use, proceeding due to `--force`
    Pruning cache at: [CACHE_DIR]/
    DEBUG Removing dangling cache bucket: [CACHE_DIR]/simple-v4
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
      × No solution found when resolving dependencies:
      ╰─▶ Because all versions of iniconfig need to be downloaded from a registry and you require iniconfig, we can conclude that your requirements are unsatisfiable.

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
    DEBUG Found workspace root: `[TEMP_DIR]/`
    DEBUG Adding root workspace member: `[TEMP_DIR]/`
    DEBUG Skipping `pyproject.toml` in `[TEMP_DIR]/` (no `[tool]` section)
    DEBUG Searching for user configuration in: `[UV_USER_CONFIG_DIR]/uv.toml`
    DEBUG uv [VERSION] ([COMMIT] DATE)
    Pruning cache at: [CACHE_DIR]/
    DEBUG Removing dangling source revision: [CACHE_DIR]/sdists-v9/[ENTRY]
    DEBUG Removing dangling cache archive: [CACHE_DIR]/archive-v0/[ENTRY]
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
    DEBUG Found workspace root: `[TEMP_DIR]/`
    DEBUG Adding root workspace member: `[TEMP_DIR]/`
    DEBUG Skipping `pyproject.toml` in `[TEMP_DIR]/` (no `[tool]` section)
    DEBUG Searching for user configuration in: `[UV_USER_CONFIG_DIR]/uv.toml`
    DEBUG uv [VERSION] ([COMMIT] DATE)
    Pruning cache at: [CACHE_DIR]/
    DEBUG Removing dangling source revision: [CACHE_DIR]/sdists-v9/[ENTRY]
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

/// Age pruning removes unused wheels and their pointers, preserving source build inputs.
#[tokio::test]
async fn prune_unused_wheels() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_filter((
        r"\[CACHE_DIR\](\\|/)archive-v0(\\|/)[^\r\n]+",
        "[CACHE_DIR]/archive-v0/[ARCHIVE]",
    ));
    let cache = Cache::from_path(context.cache_dir.path()).init().await?;
    let unused_entry = context
        .cache_dir
        .child("wheels-v6/pypi/demo/1.0.0-py3-none-any");
    let (unused_archive, unused_marker) = persist_archive(&cache, &unused_entry).await?;
    let (fresh_archive, fresh_marker) = persist_archive(
        &cache,
        &context
            .cache_dir
            .child("wheels-v6/pypi/demo/2.0.0-py3-none-any"),
    )
    .await?;
    let (recent_archive, recent_marker) = persist_archive(
        &cache,
        &context
            .cache_dir
            .child("wheels-v6/pypi/demo/3.0.0-py3-none-any"),
    )
    .await?;
    let source = context
        .cache_dir
        .child("sdists-v9/pypi/demo/1.0.0/revision");
    let built_entry = source.child("demo-1.0.0-py3-none-any");
    let (built_archive, built_marker) = persist_archive(&cache, &built_entry).await?;
    drop(cache);

    let http_pointer = unused_entry.with_file_name("1.0.0-py3-none-any.http");
    let local_pointer = unused_entry.with_file_name("1.0.0-py3-none-any.rev");
    fs_err::write(&http_pointer, "HTTP archive pointer")?;
    fs_err::write(&local_pointer, "local archive pointer")?;
    source
        .child("demo-1.0.0-py3-none-any.whl")
        .write_str("compressed wheel")?;
    source.child("metadata.msgpack").write_str("metadata")?;
    source.child("src/pyproject.toml").write_str("source")?;

    let old = FileTime::from_unix_time(1_700_000_000, 0);
    filetime::set_file_mtime(&unused_marker, old)?;
    filetime::set_file_mtime(&built_marker, old)?;
    let future = FileTime::from_unix_time(FileTime::now().unix_seconds() + 86_400, 0);
    filetime::set_file_mtime(&fresh_marker, future)?;
    // A daily timestamp update can understate actual use by up to a day.
    filetime::set_file_mtime(
        &recent_marker,
        FileTime::from_system_time(SystemTime::now() - Duration::from_hours(30 * 24 + 12)),
    )?;

    uv_snapshot!(context.filters(), context.prune().args(["--max-age", "30", "--dry-run"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    Would remove 2 unused wheels
      [CACHE_DIR]/archive-v0/[ARCHIVE]
      [CACHE_DIR]/archive-v0/[ARCHIVE]
    ");

    assert!(unused_archive.is_dir());
    assert!(built_archive.is_dir());
    assert!(unused_entry.exists());
    assert!(http_pointer.is_file());
    assert!(local_pointer.is_file());
    assert_eq!(
        FileTime::from_last_modification_time(&fs_err::metadata(&unused_marker)?),
        old
    );

    uv_snapshot!(context.filters(), context.prune().args(["--max-age", "30"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    Removed 2 unused wheels
    ");

    assert!(!unused_archive.exists());
    assert!(!built_archive.exists());
    assert!(fs_err::symlink_metadata(&unused_entry).is_err());
    assert!(fs_err::symlink_metadata(&built_entry).is_err());
    assert!(!http_pointer.exists());
    assert!(!local_pointer.exists());
    assert!(!unused_marker.exists());
    assert!(!built_marker.exists());
    assert!(fresh_archive.is_dir());
    assert!(fresh_marker.is_file());
    assert_eq!(
        FileTime::from_last_modification_time(&fs_err::metadata(&fresh_marker)?),
        future
    );
    assert!(recent_archive.is_dir());
    assert!(source.child("demo-1.0.0-py3-none-any.whl").is_file());
    assert!(source.child("metadata.msgpack").is_file());
    assert!(source.child("src/pyproject.toml").is_file());

    Ok(())
}

/// Existing cache entries receive a grace period, without mutating the cache during a dry run.
#[tokio::test]
async fn prune_unused_missing_markers() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let cache = Cache::from_path(context.cache_dir.path()).init().await?;
    let (archive, marker) = persist_archive(
        &cache,
        &context
            .cache_dir
            .child("wheels-v6/pypi/demo/1.0.0-py3-none-any"),
    )
    .await?;
    drop(cache);
    fs_err::remove_file(&marker)?;
    filetime::set_file_mtime(&archive, FileTime::from_unix_time(1_700_000_000, 0))?;

    uv_snapshot!(context.filters(), context.prune().args(["--max-age", "30", "--dry-run"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    Would remove 0 unused wheels
    ");
    assert!(!marker.exists());
    assert!(archive.is_dir());

    uv_snapshot!(context.filters(), context.prune().args(["--max-age", "30"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    Removed 0 unused wheels
    ");
    assert!(marker.is_file());
    assert!(archive.is_dir());

    filetime::set_file_mtime(&marker, FileTime::from_unix_time(1_700_000_000, 0))?;
    uv_snapshot!(context.filters(), context.prune().args(["--max-age", "30"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    Removed 1 unused wheel
    ");
    assert!(!archive.exists());

    Ok(())
}

/// Reusing a wheel refreshes its usage record, without keeping every cached version alive.
#[test]
fn prune_unused_reinstall() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let links = context.workspace_root.join("test/links");
    for requirement in ["ok==1.0.0", "ok==2.0.0"] {
        context
            .pip_install()
            .arg(requirement)
            .args(["--offline", "--no-index", "--find-links"])
            .arg(&links)
            .assert()
            .success();
    }

    let archives = fs_err::read_dir(context.cache_dir.child("archive-v0").path())?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    let unused_archive = archives
        .iter()
        .find(|path| path.join("ok-1.0.0.dist-info").is_dir())
        .context("missing cached ok==1.0.0")?;
    let selected_archive = archives
        .iter()
        .find(|path| path.join("ok-2.0.0.dist-info").is_dir())
        .context("missing cached ok==2.0.0")?;
    let usage = context.cache_dir.child("usage-v0/archive-v0");
    let unused_marker = usage.child(unused_archive.file_name().context("missing archive name")?);
    let selected_marker = usage.child(
        selected_archive
            .file_name()
            .context("missing archive name")?,
    );
    let old = FileTime::from_unix_time(1_700_000_000, 0);
    filetime::set_file_mtime(&unused_marker, old)?;
    filetime::set_file_mtime(&selected_marker, old)?;
    let metadata = selected_archive.join("ok-2.0.0.dist-info/METADATA");
    let metadata_modified = fs_err::metadata(&metadata)?.modified()?;

    context.pip_uninstall().arg("ok").assert().success();
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("ok==2.0.0")
        .args(["--offline", "--no-index", "--find-links"])
        .arg(&links), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Installed 1 package in [TIME]
     + ok==2.0.0
    ");

    assert_eq!(
        FileTime::from_last_modification_time(&fs_err::metadata(&unused_marker)?),
        old
    );
    assert!(FileTime::from_last_modification_time(&fs_err::metadata(&selected_marker)?) > old);
    assert_eq!(fs_err::metadata(&metadata)?.modified()?, metadata_modified);

    // Repeated uses within a day do not cause another timestamp write.
    let recent = FileTime::from_unix_time(FileTime::now().unix_seconds() - 3600, 0);
    filetime::set_file_mtime(&selected_marker, recent)?;
    context.pip_uninstall().arg("ok").assert().success();
    context
        .pip_install()
        .arg("ok==2.0.0")
        .args(["--offline", "--no-index", "--find-links"])
        .arg(&links)
        .assert()
        .success();
    assert_eq!(
        FileTime::from_last_modification_time(&fs_err::metadata(&selected_marker)?),
        recent
    );

    uv_snapshot!(context.filters(), context.prune().args(["--max-age", "30"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    Removed 1 unused wheel
    ");
    assert!(!unused_archive.exists());
    assert!(selected_archive.is_dir());

    // The local wheel can be unpacked again after its stale archive and pointer are removed.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("ok==1.0.0")
        .args(["--offline", "--no-index", "--find-links"])
        .arg(&links), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - ok==2.0.0
     + ok==1.0.0
    ");

    Ok(())
}

/// Cached environments retain both their backing archive and wheels installed with symlinks.
#[cfg(unix)]
#[tokio::test]
async fn prune_unused_cached_environments() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let cache = Cache::from_path(context.cache_dir.path()).init().await?;
    let (needed_archive, needed_marker) = persist_archive(
        &cache,
        &context
            .cache_dir
            .child("wheels-v6/pypi/demo/1.0.0-py3-none-any"),
    )
    .await?;
    let (unused_archive, unused_marker) = persist_archive(
        &cache,
        &context
            .cache_dir
            .child("wheels-v6/pypi/demo/2.0.0-py3-none-any"),
    )
    .await?;
    let environment = context.cache_dir.child("environments-v2/tool");
    let (environment_archive, environment_marker) = persist_archive(&cache, &environment).await?;
    fs_err::os::unix::fs::symlink(
        needed_archive.join("payload.py"),
        environment_archive.join("installed.py"),
    )?;
    drop(cache);
    for marker in [&needed_marker, &unused_marker, &environment_marker] {
        filetime::set_file_mtime(marker, FileTime::from_unix_time(1_700_000_000, 0))?;
    }

    uv_snapshot!(context.filters(), context.prune().args(["--max-age", "30"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    Removed 1 unused wheel
    ");

    assert!(!unused_archive.exists());
    assert!(needed_archive.is_dir());
    assert!(environment.exists());
    assert!(environment_archive.is_dir());
    assert_eq!(
        fs_err::read_to_string(environment.join("installed.py"))?,
        "payload"
    );

    Ok(())
}

/// Unknown cache generations may contain references that this version cannot safely interpret.
#[tokio::test]
async fn prune_unused_unknown_buckets() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let cache = Cache::from_path(context.cache_dir.path()).init().await?;
    let (archive, marker) = persist_archive(
        &cache,
        &context
            .cache_dir
            .child("wheels-v6/pypi/demo/1.0.0-py3-none-any"),
    )
    .await?;
    drop(cache);
    filetime::set_file_mtime(&marker, FileTime::from_unix_time(1_700_000_000, 0))?;

    for bucket in [
        "wheels-v999",
        "sdists-v999",
        "archive-v999",
        "environments-v999",
    ] {
        let unknown = context.cache_dir.child(bucket);
        unknown.child("payload").write_str("unknown cache data")?;

        allow_duplicates! {
            uv_snapshot!(context.filters(), context.prune().args(["--max-age", "30"]), @"
            exit_code: 0 (success)
            ----- stderr -----
            Removed 0 unused wheels
            ");
        }
        assert!(archive.is_dir());
        assert!(unknown.child("payload").is_file());
        fs_err::remove_dir_all(&unknown)?;
    }

    uv_snapshot!(context.filters(), context.prune().args(["--max-age", "30"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    Removed 1 unused wheel
    ");
    assert!(!archive.exists());

    Ok(())
}

/// Usage directories that link outside the cache must not be used to expire or rewrite entries.
#[cfg(unix)]
#[tokio::test]
async fn prune_unused_linked_usage_directories() -> Result<()> {
    for usage_path in ["usage-v0", "usage-v0/archive-v0"] {
        let context = uv_test::test_context!("3.12");
        let cache = Cache::from_path(context.cache_dir.path()).init().await?;
        let entry = context
            .cache_dir
            .child("wheels-v6/pypi/demo/1.0.0-py3-none-any");
        let (archive, marker) = persist_archive(&cache, &entry).await?;
        drop(cache);

        let usage = context.cache_dir.child(usage_path);
        let outside = context.temp_dir.child("outside-usage");
        let outside_marker = outside.join(marker.strip_prefix(&usage)?);
        fs_err::rename(&usage, &outside)?;
        fs_err::os::unix::fs::symlink(&outside, &usage)?;
        fs_err::write(&outside_marker, "outside marker data")?;
        let old = FileTime::from_unix_time(1_700_000_000, 0);
        filetime::set_file_mtime(&outside_marker, old)?;

        allow_duplicates! {
            uv_snapshot!(context.filters(), context.prune().args(["--max-age", "30", "--dry-run"]), @"
            exit_code: 0 (success)
            ----- stderr -----
            Would remove 0 unused wheels
            ");

            uv_snapshot!(context.filters(), context.prune().args(["--max-age", "30"]), @"
            exit_code: 0 (success)
            ----- stderr -----
            Removed 0 unused wheels
            ");
        }

        assert!(archive.is_dir());
        assert!(entry.exists());
        assert!(fs_err::symlink_metadata(&usage)?.is_symlink());
        assert_eq!(
            fs_err::read_to_string(&outside_marker)?,
            "outside marker data"
        );
        assert_eq!(
            FileTime::from_last_modification_time(&fs_err::metadata(&outside_marker)?),
            old
        );
    }

    Ok(())
}

/// Age pruning cannot remove a wheel while another command holds a shared cache lock.
#[tokio::test]
async fn prune_unused_locked_cache() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let cache = Cache::from_path(context.cache_dir.path()).init().await?;
    let (archive, marker) = persist_archive(
        &cache,
        &context
            .cache_dir
            .child("wheels-v6/pypi/demo/1.0.0-py3-none-any"),
    )
    .await?;
    drop(cache);
    filetime::set_file_mtime(&marker, FileTime::from_unix_time(1_700_000_000, 0))?;
    let cache = Cache::from_path(context.cache_dir.path()).init().await?;

    uv_snapshot!(context.filters(), context.prune()
        .args(["--max-age", "30"])
        .env(EnvVars::UV_LOCK_TIMEOUT, "1"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    Cache is currently in-use, waiting for other uv processes to finish
    error: Timeout ([TIME]) when waiting for lock on `[CACHE_DIR]/` at `[CACHE_DIR]/.lock`, is another uv process running? You can set `UV_LOCK_TIMEOUT` to increase the timeout.
    ");
    assert!(archive.is_dir());
    drop(cache);

    uv_snapshot!(context.filters(), context.prune().args(["--max-age", "30"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    Removed 1 unused wheel
    ");
    assert!(!archive.exists());

    Ok(())
}

/// Shared archive IDs refresh one usage record and remove every reference when the payload expires.
#[tokio::test]
async fn prune_unused_shared_archive() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_filter((
        r"\[CACHE_DIR\](\\|/)archive-v0(\\|/)[^\r\n]+",
        "[CACHE_DIR]/archive-v0/[ARCHIVE]",
    ));
    let first_entry = context
        .cache_dir
        .child("wheels-v6/pypi/demo/1.0.0-py3-none-any");
    let second_entry = context
        .cache_dir
        .child("wheels-v6/index/other/demo/1.0.0-py3-none-any");
    let id = ArchiveId::default();
    let archive = context.cache_dir.child("archive-v0").join(&id);
    let marker = context.cache_dir.child("usage-v0/archive-v0").join(&id);
    let old = FileTime::from_unix_time(1_700_000_000, 0);
    let mut pointers = Vec::new();

    for reference in [&first_entry, &second_entry] {
        let cache = Cache::from_path(context.cache_dir.path()).init().await?;
        let directory = tempfile::tempdir_in(cache.root())?;
        fs_err::write(directory.path().join("payload.py"), "payload")?;
        cache
            .persist_with_id(directory, reference, id.clone())
            .await?;
        drop(cache);

        // The second publication reuses the archive and must refresh its aged marker.
        assert!(FileTime::from_last_modification_time(&fs_err::metadata(&marker)?) > old);
        filetime::set_file_mtime(&marker, old)?;

        for extension in ["http", "rev"] {
            let pointer = reference.with_file_name(format!("1.0.0-py3-none-any.{extension}"));
            fs_err::write(&pointer, "archive pointer")?;
            pointers.push(pointer);
        }
    }

    uv_snapshot!(context.filters(), context.prune().args(["--max-age", "30", "--dry-run"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    Would remove 1 unused wheel
      [CACHE_DIR]/archive-v0/[ARCHIVE]
    ");

    assert!(archive.is_dir());
    assert!(first_entry.exists());
    assert!(second_entry.exists());
    assert!(pointers.iter().all(|pointer| pointer.is_file()));
    assert_eq!(
        FileTime::from_last_modification_time(&fs_err::metadata(&marker)?),
        old
    );

    uv_snapshot!(context.filters(), context.prune().args(["--max-age", "30"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    Removed 1 unused wheel
    ");

    assert!(!archive.exists());
    assert!(!marker.exists());
    assert!(fs_err::symlink_metadata(&first_entry).is_err());
    assert!(fs_err::symlink_metadata(&second_entry).is_err());
    assert!(pointers.iter().all(|pointer| !pointer.exists()));

    Ok(())
}

/// Persist a synthetic archive through the same layout as an unpacked wheel or environment.
async fn persist_archive(cache: &Cache, reference: &Path) -> Result<(PathBuf, PathBuf)> {
    let directory = tempfile::tempdir_in(cache.root())?;
    fs_err::write(directory.path().join("payload.py"), "payload")?;
    let id = cache.persist(directory.keep(), reference).await?;
    Ok((
        cache.archive(&id),
        cache.root().join("usage-v0/archive-v0").join(id),
    ))
}
