use anyhow::Result;
use assert_cmd::prelude::*;
use assert_fs::prelude::*;

use uv_cache::Cache;
use uv_python::managed::ManagedPythonInstallations;
use uv_static::EnvVars;

use uv_test::uv_snapshot;

/// `cache clean` should remove all packages.
#[test]
fn clean_all() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("typing-extensions\niniconfig")?;

    // Install a requirement, to populate the cache.
    context
        .pip_sync()
        .arg("requirements.txt")
        .assert()
        .success();

    uv_snapshot!(context.with_filtered_counts().filters(), context.clean().arg("--verbose"), @"
    exit_code: 0 (success)
    ----- stderr -----
    DEBUG Searching for user configuration in: `[UV_USER_CONFIG_DIR]/uv.toml`
    DEBUG uv [VERSION] ([COMMIT] DATE)
    Clearing cache at: [CACHE_DIR]/
    Removed [N] files ([SIZE])
    ");

    Ok(())
}

/// `cache clear` should behave as an alias of `cache clean`.
#[test]
fn clear_all_alias() -> Result<()> {
    let context = uv_test::test_context!("3.12");

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

    uv_snapshot!(context.with_filtered_counts().filters(), command, @"
    exit_code: 0 (success)
    ----- stderr -----
    DEBUG Searching for user configuration in: `[UV_USER_CONFIG_DIR]/uv.toml`
    DEBUG uv [VERSION] ([COMMIT] DATE)
    Clearing cache at: [CACHE_DIR]/
    Removed [N] files ([SIZE])
    ");

    Ok(())
}

/// A full cache clean should also reclaim interrupted managed Python downloads.
#[test]
fn clean_all_python_temporary_directories() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_managed_python_dirs();

    let managed = context.temp_dir.child("managed");
    let scratch = managed.child(".temp");
    let interrupted = scratch.child(".tmp-interrupted");
    interrupted.create_dir_all()?;
    interrupted.child("download").write_str("partial Python")?;

    let installation = managed.child("cpython-existing");
    installation.create_dir_all()?;
    installation.child("python").write_str("installed Python")?;

    uv_snapshot!(context.filters(), context.clean(), @"
    exit_code: 0 (success)
    ----- stderr -----
    Clearing cache at: [CACHE_DIR]/
    Clearing temporary Python downloads at: managed/.temp
    Removed [N] files ([SIZE])
    ");

    assert!(scratch.is_dir());
    assert!(!interrupted.exists());
    assert!(installation.child("python").is_file());

    Ok(())
}

/// An empty managed Python scratch directory should not produce a cleanup message.
#[test]
fn clean_all_does_not_report_empty_python_temporary_directories() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_managed_python_dirs();

    let scratch = context.temp_dir.child("managed").child(".temp");
    scratch.create_dir_all()?;

    context.cache_dir.create_dir_all()?;
    context.cache_dir.child("cached").write_str("cached")?;

    uv_snapshot!(context.filters(), context.clean(), @"
    exit_code: 0 (success)
    ----- stderr -----
    Clearing cache at: [CACHE_DIR]/
    Removed [N] files ([SIZE])
    ");

    assert!(scratch.is_dir());

    Ok(())
}

/// Managed Python downloads can still be cleaned when the package cache is absent.
#[test]
fn clean_python_temporary_directories_without_cache() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_managed_python_dirs();

    if context.cache_dir.exists() {
        fs_err::remove_dir_all(&context.cache_dir)?;
    }

    let scratch = context.temp_dir.child("managed").child(".temp");
    let interrupted = scratch.child(".tmp-interrupted");
    interrupted.create_dir_all()?;
    interrupted.child("download").write_str("partial Python")?;

    uv_snapshot!(context.filters(), context.clean(), @"
    exit_code: 0 (success)
    ----- stderr -----
    No cache found at: [CACHE_DIR]/
    Clearing temporary Python downloads at: managed/.temp
    Removed 1 file ([SIZE])
    ");

    assert!(scratch.is_dir());
    assert!(!interrupted.exists());

    Ok(())
}

/// Cleaning an individual package must not remove managed Python downloads.
#[test]
fn clean_package_preserves_python_temporary_directories() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_managed_python_dirs();

    let scratch = context.temp_dir.child("managed").child(".temp");
    let interrupted = scratch.child(".tmp-interrupted");
    interrupted.create_dir_all()?;
    interrupted.child("download").write_str("partial Python")?;

    uv_snapshot!(context.filters(), context.clean().arg("missing-package"), @"
    exit_code: 0 (success)
    ----- stderr -----
    No cache entries found
    ");

    assert!(interrupted.child("download").is_file());

    Ok(())
}

/// A cache clean, even with `--force`, must not remove an active managed Python download.
#[tokio::test]
async fn clean_python_temporary_directories_waits_for_installation_lock() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_managed_python_dirs();

    let managed = context.temp_dir.child("managed");
    let scratch = managed.child(".temp");
    let active = scratch.child(".tmp-active");
    active.create_dir_all()?;
    active
        .child("download")
        .write_str("active Python download")?;

    let installations =
        ManagedPythonInstallations::from_settings(Some(managed.to_path_buf()))?.init()?;
    let _lock = installations.lock().await?;

    uv_snapshot!(context.filters(), context.clean().env(EnvVars::UV_LOCK_TIMEOUT, "1"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Timeout ([TIME]) when waiting for lock on `managed` at `managed/.lock`, is another uv process running? You can set `UV_LOCK_TIMEOUT` to increase the timeout.
    ");

    assert!(active.child("download").is_file());

    uv_snapshot!(context.filters(), context.clean().arg("--force").env(EnvVars::UV_LOCK_TIMEOUT, "1"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Timeout ([TIME]) when waiting for lock on `managed` at `managed/.lock`, is another uv process running? You can set `UV_LOCK_TIMEOUT` to increase the timeout.
    ");

    assert!(active.child("download").is_file());

    Ok(())
}

#[tokio::test]
async fn clean_force() -> Result<()> {
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
    let context = uv_test::test_context!("3.12");

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

    uv_snapshot!(&filters, context.clean().arg("--verbose").arg("iniconfig"), @"
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
    uv_snapshot!(&filters, context.prune().arg("--verbose"), @"
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
    let context = uv_test::test_context!("3.12");

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

    uv_snapshot!(&filters, context.clean().arg("--verbose").arg("iniconfig"), @"
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
    let context = uv_test::test_context!("3.12");
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

    uv_snapshot!(context.filters(), context.clean().arg("demo"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Removed 3 files ([SIZE])
    ");

    assert!(victim_dir.is_dir());
    assert!(victim_dir.child("payload.txt").is_file());
    assert!(fs_err::symlink_metadata(package_entry).is_err());
    assert!(fs_err::symlink_metadata(archive_entry).is_err());

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
    Removed 2 files
    ");

    Ok(())
}
