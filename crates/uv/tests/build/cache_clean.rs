#[cfg(target_os = "macos")]
use std::fs::Permissions;
#[cfg(target_os = "macos")]
use std::os::unix::fs::PermissionsExt;

use anyhow::Result;
use assert_cmd::prelude::*;
use assert_fs::prelude::*;

#[cfg(target_os = "linux")]
use std::process::Command;

use uv_cache::Cache;
#[cfg(unix)]
use uv_fs::link::{LinkMode, LinkOptions, link_dir};
use uv_static::EnvVars;

use uv_test::uv_snapshot;

/// `cache clean` should remove all packages.
#[test]
fn clean_all() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_file_counts()
        .with_filtered_sizes_and_units();

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("typing-extensions\niniconfig")?;

    // Install a requirement, to populate the cache.
    context
        .pip_sync()
        .arg("requirements.txt")
        .assert()
        .success();

    uv_snapshot!(context.filters(), context.clean().arg("--verbose"), @"
    exit_code: 0 (success)
    ----- stderr -----
    DEBUG Searching for user configuration in: `[UV_USER_CONFIG_DIR]/uv.toml`
    DEBUG uv [VERSION] ([COMMIT] DATE)
    Clearing cache at: [CACHE_DIR]/
    Removed [N] files ([SIZE])
    ");

    Ok(())
}

/// Cache cleanup should count hardlinked storage only when its final link is removed.
#[cfg(unix)]
#[test]
fn clean_all_hardlinked_file() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_filtered_counts();

    // Remove unrelated cache entries so the retained hardlink is the only cached data.
    context.clean().assert().success();
    context.cache_dir.create_dir_all()?;

    // Keep the retained hardlink beside the cache so both entries share a filesystem.
    let retained = context.cache_dir.path().with_file_name("retained.bin");
    fs_err::write(&retained, vec![42; 1024 * 1024])?;
    fs_err::OpenOptions::new()
        .write(true)
        .open(&retained)?
        .sync_all()?;

    let cached = context.cache_dir.child("hardlinked.bin");
    fs_err::hard_link(&retained, &cached)?;

    // Counting the externally retained hardlink would incorrectly report 1.0MiB.
    uv_snapshot!(context.filters(), context.clean(), @"
    exit_code: 0 (success)
    ----- stderr -----
    Clearing cache at: [CACHE_DIR]/
    Removed [N] files (0B)
    ");

    context.cache_dir.create_dir_all()?;
    fs_err::hard_link(&retained, &cached)?;

    uv_snapshot!(context.filters(), context.clean().arg("--preview-features").arg("cache-physical-space"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Clearing cache at: [CACHE_DIR]/
    Removed [N] files (0B)
    ");

    assert!(retained.is_file());

    context.cache_dir.create_dir_all()?;
    cached.write_binary(&vec![42; 1024 * 1024])?;
    fs_err::OpenOptions::new()
        .write(true)
        .open(cached.path())?
        .sync_all()?;
    fs_err::hard_link(&cached, context.cache_dir.child("second-hardlink.bin"))?;

    // Counting each hardlink separately would incorrectly report 2.0MiB.
    uv_snapshot!(context.filters(), context.clean().arg("--preview-features").arg("cache-physical-space"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Clearing cache at: [CACHE_DIR]/
    Removed [N] files (1.0MiB)
    ");

    Ok(())
}

/// `cache clean` should fall back to logical space on unsupported filesystems.
#[cfg(unix)]
#[test]
fn clean_all_physical_space_unsupported_fs() -> Result<()> {
    let Some(context) = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_cache_on_alt_fs()?
    else {
        return Ok(());
    };

    context
        .cache_dir
        .child("cached.bin")
        .write_binary(&vec![42; 1024 * 1024])?;

    uv_snapshot!(context.filters(), context.clean().arg("--preview-features").arg("cache-physical-space"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Clearing cache at: [ALT_FS]/[CACHE_DIR]/
    Removed [N] files (1.0MiB)
    ");

    Ok(())
}

/// `cache clean` should report physical space for copy-on-write clones in preview mode.
#[cfg(unix)]
#[test]
fn clean_all_cloned_file() -> Result<()> {
    let Some(context) = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_cache_on_cow_fs()?
    else {
        return Ok(());
    };
    let retained = context.cache_dir.path().with_file_name("retained");
    fs_err::create_dir_all(&retained)?;
    let original = retained.join("original.bin");
    fs_err::write(&original, vec![42; 1024 * 1024])?;

    // Remove unrelated cache entries so the cloned file is the only allocated data being cleaned.
    context.clean().assert().success();
    context.cache_dir.create_dir_all()?;

    let cached = context.cache_dir.child("cloned");
    let link_mode = link_dir(&retained, &cached, &LinkOptions::new(LinkMode::Clone))?;
    assert_eq!(
        link_mode,
        LinkMode::Clone,
        "the configured copy-on-write filesystem did not clone the cached file"
    );

    uv_snapshot!(context.filters(), context.clean().arg("--preview"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Clearing cache at: [COW_FS]/[CACHE_DIR]/
    Removed [N] files (0B)
    ");

    assert!(original.is_file());

    Ok(())
}

/// Clones shared only within the cache should be counted once when their final reference is removed.
#[cfg(unix)]
#[test]
fn clean_all_cached_clones() -> Result<()> {
    let Some(context) = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_cache_on_cow_fs()?
    else {
        return Ok(());
    };
    let original = context.cache_dir.child("original");
    original.create_dir_all()?;
    original
        .child("original.bin")
        .write_binary(&vec![42; 1024 * 1024])?;

    let cloned = context.cache_dir.child("cloned");
    let link_mode = link_dir(&original, &cloned, &LinkOptions::new(LinkMode::Clone))?;
    assert_eq!(
        link_mode,
        LinkMode::Clone,
        "the configured copy-on-write filesystem did not clone the cached file"
    );

    uv_snapshot!(context.filters(), context.clean().arg("--preview-features").arg("cache-physical-space"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Clearing cache at: [COW_FS]/[CACHE_DIR]/
    Removed [N] files (1.0MiB)
    ");

    Ok(())
}

/// Unknown compressed extents should not discard measurements for unrelated cache entries.
#[cfg(target_os = "linux")]
#[test]
fn clean_all_compressed_file() -> Result<()> {
    let Some(context) = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_cache_on_cow_fs()?
    else {
        return Ok(());
    };
    let measured = context.cache_dir.child("measured.bin");
    measured.write_binary(&vec![42; 1024 * 1024])?;
    fs_err::OpenOptions::new()
        .write(true)
        .open(measured.path())?
        .sync_all()?;

    let compressed = context.cache_dir.child("compressed.bin");
    fs_err::File::create(compressed.path())?;
    Command::new("btrfs")
        .args(["property", "set"])
        .arg(compressed.path())
        .args(["compression", "zstd"])
        .assert()
        .success();
    compressed.write_binary(&vec![42; 1024 * 1024])?;
    fs_err::OpenOptions::new()
        .write(true)
        .open(compressed.path())?
        .sync_all()?;

    uv_snapshot!(context.filters(), context.clean().arg("--preview-features").arg("cache-physical-space"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Clearing cache at: [COW_FS]/[CACHE_DIR]/
    Removed [N] files (at least 1.0MiB)
    ");

    Ok(())
}

/// `cache clear` should behave as an alias of `cache clean`.
#[test]
fn clear_all_alias() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_file_counts()
        .with_filtered_sizes_and_units();

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("typing-extensions\niniconfig")?;

    // Install a requirement, to populate the cache.
    context
        .pip_sync()
        .arg("requirements.txt")
        .assert()
        .success();

    let mut command = context.command();
    command.arg("cache").arg("clear").arg("--verbose");

    uv_snapshot!(context.filters(), command, @"
    exit_code: 0 (success)
    ----- stderr -----
    DEBUG Searching for user configuration in: `[UV_USER_CONFIG_DIR]/uv.toml`
    DEBUG uv [VERSION] ([COMMIT] DATE)
    Clearing cache at: [CACHE_DIR]/
    Removed [N] files ([SIZE])
    ");

    Ok(())
}

#[tokio::test]
async fn clean_force() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_sizes_and_units();

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("typing-extensions\niniconfig")?;

    // Install a requirement, to populate the cache.
    context
        .pip_sync()
        .arg("requirements.txt")
        .assert()
        .success();

    // When unlocked, `--force` should still take a lock
    uv_snapshot!(context.filters(), context.clean().arg("--verbose").arg("--force"), @"
    exit_code: 0 (success)
    ----- stderr -----
    DEBUG Searching for user configuration in: `[UV_USER_CONFIG_DIR]/uv.toml`
    DEBUG uv [VERSION] ([COMMIT] DATE)
    Clearing cache at: [CACHE_DIR]/
    Removed [N] files ([SIZE])
    ");

    // Install a requirement, to re-populate the cache.
    context
        .pip_sync()
        .arg("requirements.txt")
        .assert()
        .success();

    // When locked, `--force` should proceed without blocking
    let _cache = uv_cache::Cache::from_path(context.cache_dir.path())
        .with_exclusive_lock()
        .await;
    uv_snapshot!(context.filters(), context.clean().arg("--verbose").arg("--force"), @"
    exit_code: 0 (success)
    ----- stderr -----
    DEBUG Searching for user configuration in: `[UV_USER_CONFIG_DIR]/uv.toml`
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
    let context = uv_test::test_context!("3.12")
        .with_filtered_file_counts()
        .with_filtered_sizes_and_units()
        // The cache entry does not have a stable key, so we filter it out.
        .with_filter((
            r"\[CACHE_DIR\](\\|\/)(.+)(\\|\/).*",
            "[CACHE_DIR]/$2/[ENTRY]",
        ));

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
        .child("simple-v24")
        .child("pypi")
        .child("iniconfig.rkyv");
    assert!(
        rkyv.exists(),
        "Expected the `.rkyv` file to exist for `iniconfig`"
    );

    uv_snapshot!(context.filters(), context.clean().arg("--verbose").arg("iniconfig"), @"
    exit_code: 0 (success)
    ----- stderr -----
    DEBUG Searching for user configuration in: `[UV_USER_CONFIG_DIR]/uv.toml`
    DEBUG uv [VERSION] ([COMMIT] DATE)
    DEBUG Removing dangling cache entry: [CACHE_DIR]/archive-v0/[ENTRY]
    Removed [N] files ([SIZE])
    ");

    // Assert that the `.rkyv` file is removed for `iniconfig`.
    assert!(
        !rkyv.exists(),
        "Expected the `.rkyv` file to be removed for `iniconfig`"
    );

    // Running `uv cache prune` should have no effect.
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

/// `cache clean iniconfig` should remove a single package (`iniconfig`).
#[test]
fn clean_package_index() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_file_counts()
        .with_filtered_sizes_and_units()
        // The cache entry does not have a stable key, so we filter it out.
        .with_filter((
            r"\[CACHE_DIR\](\\|\/)(.+)(\\|\/).*",
            "[CACHE_DIR]/$2/[ENTRY]",
        ));

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
        .child("simple-v24")
        .child("index")
        .child("e8208120cae3ba69")
        .child("iniconfig.rkyv");
    assert!(
        rkyv.exists(),
        "Expected the `.rkyv` file to exist for `iniconfig`"
    );

    uv_snapshot!(context.filters(), context.clean().arg("--verbose").arg("iniconfig"), @"
    exit_code: 0 (success)
    ----- stderr -----
    DEBUG Searching for user configuration in: `[UV_USER_CONFIG_DIR]/uv.toml`
    DEBUG uv [VERSION] ([COMMIT] DATE)
    DEBUG Removing dangling cache entry: [CACHE_DIR]/archive-v0/[ENTRY]
    Removed [N] files ([SIZE])
    ");

    // Assert that the `.rkyv` file is removed for `iniconfig`.
    assert!(
        !rkyv.exists(),
        "Expected the `.rkyv` file to be removed for `iniconfig`"
    );

    Ok(())
}

#[cfg(unix)]
#[test]
fn clean_package_does_not_follow_symlinks() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_filtered_sizes_and_units();
    let victim_dir = context.temp_dir.child("victim");
    let archive_entry = context.cache_dir.child("archive-v0").child("archive");
    let package_entry = context
        .cache_dir
        .child("wheels-v6")
        .child("pypi")
        .child("demo");

    victim_dir.create_dir_all()?;
    victim_dir.child("payload.txt").write_str("payload")?;
    archive_entry.create_dir_all()?;
    archive_entry.child("payload.txt").write_str("payload")?;
    package_entry.create_dir_all()?;

    // Preserve external targets while still removing unreferenced entries in the archive bucket.
    fs_err::os::unix::fs::symlink(&victim_dir, package_entry.join("escape"))?;
    fs_err::os::unix::fs::symlink(&archive_entry, package_entry.join("archive"))?;

    let files = context.cache_dir.child("files-v0");
    let shard = files.child("shard");
    shard.child("orphan").write_str("orphan")?;
    shard
        .child("nested")
        .child("orphan")
        .write_str("nested orphan")?;
    fs_err::os::unix::fs::symlink(&victim_dir, files.child("escape"))?;
    fs_err::os::unix::fs::symlink(&victim_dir, shard.child("escape"))?;

    // Keep this shard flat so macOS can prune it with bulk metadata reads.
    let flat_shard = files.child("flat");
    flat_shard.child("orphan").write_str("orphan")?;
    let retained = context.cache_dir.path().with_file_name("retained.bin");
    fs_err::write(&retained, "retained")?;
    fs_err::hard_link(&retained, flat_shard.child("retained"))?;
    fs_err::os::unix::fs::symlink(&victim_dir, flat_shard.child("escape"))?;

    uv_snapshot!(context.filters(), context.clean().args(["demo", "other"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    Removed 6 files ([SIZE])
    ");

    assert!(victim_dir.is_dir());
    assert!(victim_dir.child("payload.txt").is_file());
    assert!(fs_err::symlink_metadata(package_entry).is_err());
    assert!(fs_err::symlink_metadata(archive_entry).is_err());
    assert!(!shard.child("orphan").exists());
    assert!(!shard.child("nested").exists());
    assert!(fs_err::symlink_metadata(files.child("escape"))?.is_symlink());
    assert!(fs_err::symlink_metadata(shard.child("escape"))?.is_symlink());
    assert!(!flat_shard.child("orphan").exists());
    assert!(retained.is_file());
    assert!(flat_shard.child("retained").is_file());
    assert!(fs_err::symlink_metadata(flat_shard.child("escape"))?.is_symlink());

    Ok(())
}

/// Empty file-cache shards can be removed without search permission.
#[cfg(target_os = "macos")]
#[test]
fn clean_package_empty_shard_without_search_permission() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let shard = context.cache_dir.child("files-v0").child("shard");
    shard.create_dir_all()?;
    fs_err::set_permissions(&shard, Permissions::from_mode(0o600))?;

    uv_snapshot!(context.filters(), context.clean().arg("demo"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Removed 1 directory (0B)
    ");

    assert!(!shard.exists());

    Ok(())
}

#[tokio::test]
async fn cache_timeout() {
    let context = uv_test::test_context!("3.12");

    // Simulate another uv process running and locking the cache, e.g., with a source build.
    let _cache = Cache::from_path(context.cache_dir.path())
        .with_exclusive_lock()
        .await;

    uv_snapshot!(context.filters(), context.clean().env(EnvVars::UV_LOCK_TIMEOUT, "1"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    Cache is currently in-use, waiting for other uv processes to finish (use `--force` to override)
    error: Timeout ([TIME]) when waiting for lock on `[CACHE_DIR]/` at `[CACHE_DIR]/.lock`, is another uv process running? You can set `UV_LOCK_TIMEOUT` to increase the timeout.
    ");
}

/// `cache clean` should handle file paths normally restricted by Win32 path normalization.
#[cfg(windows)]
#[test]
fn clean_handles_verbatim_paths() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    // Clean slate
    fs_err::remove_dir_all(&context.cache_dir)?;

    // Cached sdist path resembling the uwsgi==2.0.31 build failure.
    let uwsgi_shard = context
        .cache_dir
        .child("sdists-v9")
        .child("pypi")
        .child("uwsgi")
        .child("2.0.31")
        .child("QxDIp0qpjbsWjWURKmegK")
        .child("src")
        .child("core");

    // Attempt to create a file with a trailing dot (we need to make it verbatim to do so)
    uwsgi_shard.create_dir_all()?;
    let invalid_path = uwsgi_shard.child("logging.").to_path_buf();
    let invalid_file = uv_fs::verbatim_path(invalid_path.as_path());
    fs_err::write(&invalid_file, b"")?;

    // Confirm Win32 normalized path causes an os error when attempting to remove
    let remove_err = fs_err::remove_file(&invalid_path).expect_err("expected to fail");
    assert_eq!(remove_err.kind(), std::io::ErrorKind::NotFound);

    // Tests cache clean leverages verbatim conversion
    uv_snapshot!(context.filters(), context.clean().arg("--verbose"), @"
    exit_code: 0 (success)
    ----- stderr -----
    DEBUG Searching for user configuration in: `[UV_USER_CONFIG_DIR]/uv.toml`
    DEBUG uv [VERSION] ([COMMIT] DATE)
    Clearing cache at: [CACHE_DIR]/
    Removed 2 files (0B)
    ");

    Ok(())
}
