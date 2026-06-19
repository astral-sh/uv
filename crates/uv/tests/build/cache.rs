use anyhow::Result;
use assert_fs::prelude::*;
use std::process::Command;

#[cfg(unix)]
use uv_fs::create_symlink;
use uv_test::{get_bin, uv_snapshot};

/// When the project directory defaults to a current directory inside the cache directory, we should
/// error before using the cache.
#[test]
fn cache_current_dir_inside_cache() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    uv_snapshot!(context.filters(), context.command()
        .arg("cache")
        .arg("dir")
        .current_dir(context.cache_dir.path()), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: The project directory `.` is inside the cache directory `.`
    ");

    let child = context.cache_dir.child("child");
    child.create_dir_all()?;

    uv_snapshot!(context.filters(), context.command()
        .arg("cache")
        .arg("dir")
        .current_dir(child.path()), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: The project directory `.` is inside the cache directory `[CACHE_DIR]/`
    ");

    Ok(())
}

/// When the project directory is inside a symlinked cache directory, we should error before using
/// the cache.
#[test]
#[cfg(unix)]
fn cache_current_dir_inside_symlinked_cache() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let cache_link = context.temp_dir.child("cache-link");
    create_symlink(context.cache_dir.path(), cache_link.path())?;

    let child = context.cache_dir.child("child");
    child.create_dir_all()?;

    let mut command = Command::new(get_bin!());
    command
        .arg("cache")
        .arg("dir")
        .arg("--cache-dir")
        .arg(cache_link.path());
    context.add_shared_env(&mut command, false);
    command.current_dir(child.path());

    uv_snapshot!(context.filters(), command, @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: The project directory `.` is inside the cache directory `[CACHE_DIR]/`
    ");

    Ok(())
}

/// When a workspace is inside the cache directory, we should error before locking the workspace.
#[test]
fn cache_workspace_inside_cache() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let workspace = context.cache_dir.child("workspace");
    workspace.child("pyproject.toml").write_str(
        r#"
        [tool.uv.workspace]
        members = ["member"]
        "#,
    )?;
    workspace
        .child("member")
        .child("pyproject.toml")
        .write_str(
            r#"
        [project]
        name = "member"
        version = "0.1.0"
        requires-python = ">=3.12"
        "#,
        )?;

    uv_snapshot!(context.filters(), context.lock()
        .arg("--project")
        .arg(workspace.path()), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: The project directory `[CACHE_DIR]/workspace` is inside the cache directory `[CACHE_DIR]/`
    ");

    Ok(())
}

/// When the cache directory is a non-canonical parent of the project directory, we should still
/// detect that the project is inside the cache.
#[test]
fn cache_project_inside_relative_parent_cache() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let project = context.temp_dir.child("project");
    project.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        "#,
    )?;

    let mut command = Command::new(get_bin!());
    command.arg("lock").arg("--cache-dir").arg("..");
    context.add_shared_env(&mut command, false);
    command.current_dir(project.path());

    uv_snapshot!(context.filters(), command, @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: The project directory `.` is inside the cache directory `[TEMP_DIR]/`
    ");

    Ok(())
}

/// When `--no-cache` is enabled, running from a project inside the configured cache directory
/// should not trip the persistent cache guard.
#[test]
fn cache_project_inside_cache_no_cache() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let project = context.cache_dir.child("project");
    project.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock().arg("--no-cache").current_dir(&project), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 1 package in [TIME]
    ");

    Ok(())
}

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
