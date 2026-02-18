#[cfg(unix)]
use anyhow::Result;
#[cfg(unix)]
use assert_fs::prelude::*;
#[cfg(unix)]
use std::process::Command;

#[cfg(unix)]
use uv_test::{get_bin, uv_snapshot};

/// When the cache directory cannot be created (e.g., due to permissions), we should show a
/// chained error message that indicates we failed to initialize the cache.
#[test]
#[cfg(unix)]
fn cache_init_failure() -> Result<()> {
    use uv_test::ReadOnlyDirectoryGuard;

    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]
        "#,
    )?;

    // Create a read-only directory that will serve as the parent of the cache.
    // The guard sets it to read-only and restores original permissions on drop (including panic).
    let cache_parent = context.temp_dir.child("cache_parent");
    fs_err::create_dir(&cache_parent)?;
    let _guard = ReadOnlyDirectoryGuard::new(cache_parent.path())?;

    // Point the cache to a subdirectory within the read-only parent
    let cache_dir = cache_parent.child("cache");

    // Filter both the relative path (in the first line) and absolute path (in the cause)
    let context = context
        .with_filter((r"cache_parent/cache", "[CACHE_DIR]"))
        .with_filter((
            r"failed to create directory `.*`",
            "failed to create directory `[CACHE_DIR]`",
        ));

    // Build the sync command manually to use our custom cache directory.
    // We can't use context.sync() because it adds --cache-dir with the default cache.
    let mut command = Command::new(get_bin!());
    command
        .arg("sync")
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .current_dir(context.temp_dir.path());
    context.add_shared_env(&mut command, false);

    // Running a command should fail with a chained error about cache initialization
    uv_snapshot!(context.filters(), command, @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to initialize cache at `[CACHE_DIR]`
      Caused by: failed to create directory `[CACHE_DIR]`: Permission denied (os error 13)
    ");

    Ok(())
}
