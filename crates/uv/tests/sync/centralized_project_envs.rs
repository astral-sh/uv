use anyhow::Result;
use assert_cmd::prelude::*;
use assert_fs::prelude::*;
use insta::assert_snapshot;
use serde_json::json;
use std::process::Command;

use uv_fs::Simplified;
use uv_static::EnvVars;
#[cfg(unix)]
use uv_test::ReadOnlyDirectoryGuard;
use uv_test::{TestContext, uv_snapshot};

fn write_project(
    context: &TestContext,
    requires_python: &str,
    dependencies: &[&str],
) -> Result<()> {
    let pyproject_toml = toml::to_string(&json!({
        "project": {
            "name": "project",
            "version": "0.1.0",
            "requires-python": requires_python,
            "dependencies": dependencies,
        }
    }))?;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(&pyproject_toml)?;
    Ok(())
}

#[test]
fn sync_centralized_env() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&["3.12"])
        .with_filtered_centralized_environment_hashes();
    write_project(&context, ">=3.12", &["iniconfig"])?;

    // Creates the environment in the centralized store.
    uv_snapshot!(context.filters(), context.sync()
        .arg("--preview-features")
        .arg("centralized-project-envs"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment `project-cp3.12.[X]-[HASH]`
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    "#);

    let link = context.temp_dir.child(".venv");
    let target = fs_err::read_link(link.path())?;
    // The project link points into the cache.
    insta::with_settings!({ filters => context.filters() }, {
        assert_snapshot!(target.portable_display(), @"[CACHE_DIR]/environments-v2/project-cp3.12.[X]-[HASH]");
    });

    fs_err::remove_dir_all(link.path())?;

    // Reuses the cache entry without recreating `.venv`.
    uv_snapshot!(context.filters(), context.sync()
        .arg("--dry-run")
        .arg("--preview-features")
        .arg("centralized-project-envs"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Would use project environment at: [CACHE_DIR]/environments-v2/project-cp3.12.[X]-[HASH]
    Resolved 2 packages in [TIME]
    Found up-to-date lockfile at: uv.lock
    Checked 1 package in [TIME]
    Would make no changes
    "#);
    // Only the cached environment remains.
    assert!(!link.exists());
    assert!(target.is_dir());
    Ok(())
}

#[test]
fn sync_centralized_env_switch_python() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&["3.11", "3.12"]);
    write_project(&context, ">=3.11", &[])?;

    context
        .sync()
        .arg("--preview-features")
        .arg("centralized-project-envs")
        .arg("--python")
        .arg("3.12")
        .assert()
        .success();
    let link_312 = fs_err::read_link(context.temp_dir.child(".venv").path())?;
    insta::with_settings!({ filters => context.filters() }, {
        assert_snapshot!(link_312.portable_display(), @"[CACHE_DIR]/environments-v2/project-cp3.12.[X]-[HASH]");
    });

    context
        .sync()
        .arg("--preview-features")
        .arg("centralized-project-envs")
        .arg("--python")
        .arg("3.11")
        .assert()
        .success();
    let link_311 = fs_err::read_link(context.temp_dir.child(".venv").path())?;
    insta::with_settings!({ filters => context.filters() }, {
        assert_snapshot!(link_311.portable_display(), @"[CACHE_DIR]/environments-v2/project-cp3.11.[X]-[HASH]");
    });

    // The original environment is reused, not recreated.
    uv_snapshot!(context.filters(), context.sync()
        .arg("--preview-features")
        .arg("centralized-project-envs")
        .arg("--python")
        .arg("3.12"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Checked in [TIME]
    "#);
    Ok(())
}

#[test]
#[cfg(feature = "test-python-managed")]
fn sync_centralized_env_distinguishes_python_patch() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[])
        .with_managed_python_dirs()
        .with_python_download_cache();
    context
        .python_install()
        .arg("3.12.9")
        .arg("3.12.11")
        .assert()
        .success();
    write_project(&context, ">=3.12", &[])?;

    context
        .sync()
        .arg("--preview-features")
        .arg("centralized-project-envs")
        .arg("--python")
        .arg("3.12.9")
        .assert()
        .success();
    let first = fs_err::read_link(context.temp_dir.child(".venv").path())?;
    insta::with_settings!({ filters => context.filters() }, {
        assert_snapshot!(first.portable_display(), @"[CACHE_DIR]/environments-v2/project-cp3.12.9-[HASH]");
    });

    context
        .sync()
        .arg("--preview-features")
        .arg("centralized-project-envs")
        .arg("--python")
        .arg("3.12.11")
        .assert()
        .success();
    let second = fs_err::read_link(context.temp_dir.child(".venv").path())?;
    insta::with_settings!({ filters => context.filters() }, {
        assert_snapshot!(second.portable_display(), @"[CACHE_DIR]/environments-v2/project-cp3.12.11-[HASH]");
    });
    Ok(())
}

#[test]
#[cfg(feature = "test-python-managed")]
fn sync_centralized_env_survives_python_patch_upgrade() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[])
        .with_managed_python_dirs()
        .with_python_download_cache();
    context.python_install().arg("3.12.9").assert().success();
    write_project(&context, ">=3.12", &[])?;

    // Create an upgradeable environment.
    context
        .sync()
        .arg("--preview-features")
        .arg("centralized-project-envs")
        .arg("--python")
        .arg("3.12")
        .assert()
        .success();
    let first = fs_err::read_link(context.temp_dir.child(".venv").path())?;
    insta::with_settings!({ filters => context.filters() }, {
        assert_snapshot!(first.portable_display(), @"[CACHE_DIR]/environments-v2/project-cp3.12-[HASH]");
    });

    // The transparent upgrade retargets the environment to a different interpreter.
    let python = if cfg!(windows) {
        first.join("Scripts/python.exe")
    } else {
        first.join("bin/python")
    };
    context.python_install().arg("3.12.11").assert().success();
    uv_snapshot!(context.filters(), Command::new(&python).arg("--version"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.11

    ----- stderr -----
    "#);

    // Should reuse the same environment without re-creating after the transparent upgrade.
    uv_snapshot!(context.filters(), context.sync()
        .arg("--preview-features")
        .arg("centralized-project-envs")
        .arg("--python")
        .arg("3.12"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Checked in [TIME]
    "#);

    let second = fs_err::read_link(context.temp_dir.child(".venv").path())?;
    assert_eq!(first, second);
    Ok(())
}

#[test]
fn sync_centralized_env_avoids_project_name_collisions() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&["3.12"]);
    let project_a = context.temp_dir.child("project-a");
    let project_b = context.temp_dir.child("project-b");
    for project in [&project_a, &project_b] {
        project.create_dir_all()?;
        project.child("pyproject.toml").write_str(
            r#"
            [project]
            name = "project"
            version = "0.1.0"
            requires-python = ">=3.12"
            dependencies = []
            "#,
        )?;
    }

    for project in [&project_a, &project_b] {
        context
            .sync()
            .arg("--preview-features")
            .arg("centralized-project-envs")
            .arg("--project")
            .arg(project.path())
            .assert()
            .success();
    }

    let target_a = fs_err::read_link(project_a.child(".venv").path())?;
    let target_b = fs_err::read_link(project_b.child(".venv").path())?;
    // Projects with the same name use different environments.
    assert_ne!(target_a, target_b);
    Ok(())
}

#[test]
fn sync_centralized_env_respects_explicit_environments() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&["3.12"]);
    write_project(&context, ">=3.12", &[])?;

    context
        .sync()
        .arg("--preview-features")
        .arg("centralized-project-envs")
        .env(EnvVars::UV_PROJECT_ENVIRONMENT, "override")
        .assert()
        .success();
    // `UV_PROJECT_ENVIRONMENT` bypasses centralized environments.
    assert!(context.temp_dir.child("override").is_dir());
    assert!(!context.temp_dir.child(".venv").exists());

    let active = context.temp_dir.child("active");
    context.venv().arg(active.path()).assert().success();
    context
        .sync()
        .arg("--preview-features")
        .arg("centralized-project-envs")
        .arg("--active")
        .env_remove(EnvVars::UV_PROJECT_ENVIRONMENT)
        .env(EnvVars::VIRTUAL_ENV, active.path())
        .assert()
        .success();
    // `--active` uses `VIRTUAL_ENV` directly.
    assert!(active.is_dir());
    assert!(!context.temp_dir.child(".venv").exists());
    Ok(())
}

#[test]
fn sync_centralized_env_respects_active_default_environment() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&["3.12"]);
    write_project(&context, ">=3.12", &[])?;
    let environment = context.temp_dir.child(".venv");
    context.venv().arg(environment.path()).assert().success();

    uv_snapshot!(context.filters(), context.sync()
        .arg("--preview-features")
        .arg("centralized-project-envs")
        .arg("--active")
        .env(EnvVars::VIRTUAL_ENV, environment.path()), @r#"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Checked in [TIME]
    "#);

    assert!(!context.cache_dir.child("environments-v2").exists());
    Ok(())
}

#[test]
fn sync_centralized_env_virtual_workspace() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&["3.12"]);
    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [tool.uv.workspace]
        members = ["member"]
        "#,
    )?;
    let member = context.temp_dir.child("member");
    member.create_dir_all()?;
    member.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "member"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
        "#,
    )?;

    context
        .sync()
        .arg("--preview-features")
        .arg("centralized-project-envs")
        .assert()
        .success();

    let target = fs_err::read_link(context.temp_dir.child(".venv").path())?;
    // The workspace root owns the centralized environment.
    insta::with_settings!({ filters => context.filters() }, {
        assert_snapshot!(target.portable_display(), @"[CACHE_DIR]/environments-v2/temp-cp3.12.[X]-[HASH]");
    });
    // The workspace member does not get its own environment.
    assert!(!member.child(".venv").exists());
    Ok(())
}

#[test]
fn sync_centralized_env_dry_run() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&["3.12"]);
    write_project(&context, ">=3.12", &["iniconfig"])?;

    // Reports the persistent centralized environment path.
    uv_snapshot!(context.filters(), context.sync()
        .arg("--dry-run")
        .arg("--preview-features")
        .arg("centralized-project-envs"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Would create project environment at: [CACHE_DIR]/environments-v2/project-cp3.12.[X]-[HASH]
    Resolved 2 packages in [TIME]
    Would create lockfile at: uv.lock
    Would download 1 package
    Would install 1 package
     + iniconfig==2.0.0
    "#);
    // Dry-run creates neither the persistent environment nor its project link.
    assert!(!context.temp_dir.child(".venv").exists());
    assert!(!context.cache_dir.child("environments-v2").exists());
    Ok(())
}

#[test]
fn cache_prune_removes_and_recreates_centralized_environment() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&["3.12"]);
    write_project(&context, ">=3.12", &[])?;
    let cache_dir = "cache";

    context
        .sync()
        .env(EnvVars::UV_CACHE_DIR, cache_dir)
        .arg("--preview-features")
        .arg("centralized-project-envs")
        .assert()
        .success();
    let link = context.temp_dir.child(".venv");
    let target = fs_err::read_link(link.path())?;

    context
        .prune()
        .env(EnvVars::UV_CACHE_DIR, cache_dir)
        .assert()
        .success();
    assert!(!target.exists());
    assert_eq!(target, fs_err::read_link(link.path())?);

    // Without the preview, uv replaces the dangling cache link with a local environment.
    context
        .sync()
        .env(EnvVars::UV_CACHE_DIR, cache_dir)
        .assert()
        .success();
    assert!(link.is_dir());
    assert!(fs_err::read_link(link.path()).is_err());

    context
        .sync()
        .env(EnvVars::UV_CACHE_DIR, cache_dir)
        .arg("--preview-features")
        .arg("centralized-project-envs")
        .assert()
        .success();
    // The recreated environment uses the same cache entry.
    assert_eq!(target, fs_err::read_link(link.path())?);
    // The dangling target is recreated.
    assert!(target.is_dir());
    Ok(())
}

#[test]
fn sync_recovers_incomplete_centralized_environment() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&["3.12"]);
    write_project(&context, ">=3.12", &[])?;
    context
        .sync()
        .arg("--preview-features")
        .arg("centralized-project-envs")
        .assert()
        .success();

    let link = context.temp_dir.child(".venv");
    let target = fs_err::read_link(link.path())?;

    // Simulate a mangled environment (e.g., due to interruption).
    uv_fs::remove_virtualenv(link.path())?;
    uv_fs::remove_virtualenv(&target)?;
    fs_err::create_dir(&target)?;
    uv_fs::cachedir::ensure_tag(&target)?;
    fs_err::write(target.join(".gitignore"), "*")?;

    context
        .sync()
        .arg("--preview-features")
        .arg("centralized-project-envs")
        .assert()
        .success();

    assert_eq!(target, fs_err::read_link(link.path())?);
    assert!(target.join("pyvenv.cfg").is_file());
    Ok(())
}

#[test]
fn sync_centralized_env_no_cache_uses_dot_venv() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&["3.12"])
        .with_filtered_centralized_environment_hashes();
    write_project(&context, ">=3.12", &[])?;

    uv_snapshot!(context.filters(), context.sync()
        .arg("--no-cache")
        .arg("--preview-features")
        .arg("centralized-project-envs"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: The `centralized-project-envs` feature has no effect when `--no-cache` is enabled
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: .venv
    Resolved 1 package in [TIME]
    Checked in [TIME]
    "#);

    let environment = context.temp_dir.child(".venv");
    assert!(environment.is_dir());
    assert!(fs_err::read_link(environment.path()).is_err());

    uv_snapshot!(context.filters(), context.sync()
        .arg("--preview-features")
        .arg("centralized-project-envs"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment `project-cp3.12.[X]-[HASH]`
    Resolved 1 package in [TIME]
    Checked in [TIME]
    "#);

    let target = fs_err::read_link(environment.path())?;
    // A later cached invocation replaces the local environment with a centralized one.
    insta::with_settings!({ filters => context.filters() }, {
        assert_snapshot!(target.portable_display(), @"[CACHE_DIR]/environments-v2/project-cp3.12.[X]-[HASH]");
    });

    // With `--no-cache`, uv follows the existing `.venv` link without recreating the environment.
    uv_snapshot!(context.filters(), context.sync()
        .arg("--no-cache")
        .arg("--preview-features")
        .arg("centralized-project-envs"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: The `centralized-project-envs` feature has no effect when `--no-cache` is enabled
    Resolved 1 package in [TIME]
    Checked in [TIME]
    "#);
    assert_eq!(fs_err::read_link(environment.path())?, target);
    Ok(())
}

#[test]
fn sync_centralized_env_replaces_existing_directory_link() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&["3.12"]);
    write_project(&context, ">=3.12", &[])?;
    let environment = context.temp_dir.child(".venv");
    let previous_target = context.temp_dir.child("previous-target");
    previous_target.create_dir_all()?;
    let marker = previous_target.child("marker");
    marker.touch()?;
    uv_fs::create_symlink(previous_target.path(), environment.path())?;

    context
        .sync()
        .arg("--preview-features")
        .arg("centralized-project-envs")
        .assert()
        .success();

    let target = fs_err::read_link(environment.path())?;
    insta::with_settings!({ filters => context.filters() }, {
        assert_snapshot!(target.portable_display(), @"[CACHE_DIR]/environments-v2/project-cp3.12.[X]-[HASH]");
    });
    assert!(marker.is_file());
    Ok(())
}

#[test]
fn sync_centralized_env_with_existing_file() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&["3.12"]);
    write_project(&context, ">=3.12", &[])?;
    let environment = context.temp_dir.child(".venv");
    environment.write_str("user-data")?;

    context
        .sync()
        .arg("--preview-features")
        .arg("centralized-project-envs")
        .assert()
        .success();

    #[cfg(unix)]
    {
        let target = fs_err::read_link(environment.path())?;
        insta::with_settings!({ filters => context.filters() }, {
            assert_snapshot!(target.portable_display(), @"[CACHE_DIR]/environments-v2/project-cp3.12.[X]-[HASH]");
        });
    }

    #[cfg(windows)]
    {
        // TODO(tk): This changes once `.venv` can store an environment path.
        assert_eq!(fs_err::read_to_string(environment.path())?, "user-data");
        assert!(context.cache_dir.child("environments-v2").is_dir());
    }
    Ok(())
}

#[cfg(windows)]
#[test]
fn sync_centralized_env_replaces_existing_empty_directory() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&["3.12"]);
    write_project(&context, ">=3.12", &[])?;
    context.temp_dir.child(".venv").create_dir_all()?;

    context
        .sync()
        .arg("--preview-features")
        .arg("centralized-project-envs")
        .assert()
        .success();

    let target = fs_err::read_link(context.temp_dir.child(".venv").path())?;
    insta::with_settings!({ filters => context.filters() }, {
        assert_snapshot!(target.portable_display(), @"[CACHE_DIR]/environments-v2/project-cp3.12.[X]-[HASH]");
    });
    Ok(())
}

#[test]
fn run_and_sync_link_failure_reporting() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&["3.12"])
        .with_filtered_centralized_environment_hashes()
        .with_filter((
            r"(?m)^(warning: Failed to create link to project environment at `[^`]+`): .*$",
            "$1: [ERR]",
        ));
    write_project(&context, ">=3.12", &["iniconfig"])?;
    let environment = context.temp_dir.child(".venv");
    environment.create_dir_all()?;
    environment.child("keep").touch()?;

    // `uv run` uses the centralized environment without reporting a link update failure.
    uv_snapshot!(context.filters(), context.run()
        .current_dir(&context.home_dir)
        .arg("--project")
        .arg(context.temp_dir.path())
        .arg("--preview-features")
        .arg("centralized-project-envs")
        .arg("python")
        .arg("-c")
        .arg("import iniconfig"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment `project-cp3.12.[X]-[HASH]`
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    "#);

    assert!(environment.child("keep").is_file());

    // `uv sync` reports the same link update failure to the user.
    uv_snapshot!(context.filters(), context.sync()
        .current_dir(&context.home_dir)
        .arg("--project")
        .arg(context.temp_dir.path())
        .arg("--preview-features")
        .arg("centralized-project-envs"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: Failed to create link to project environment at `[VENV]/`: [ERR]
    Resolved 2 packages in [TIME]
    Checked 1 package in [TIME]
    "#);

    assert!(environment.child("keep").is_file());
    Ok(())
}

#[cfg(unix)]
#[test]
fn sync_centralized_env_local_environment_removal_failure_is_not_fatal() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&["3.12"])
        .with_filtered_centralized_environment_hashes();
    write_project(&context, ">=3.12", &[])?;
    let environment = context.temp_dir.child(".venv");
    environment.create_dir_all()?;
    let pyvenv_cfg = environment.child("pyvenv.cfg");
    pyvenv_cfg.touch()?;

    // Prevent uv from removing `pyvenv.cfg` while replacing the local environment.
    let _guard = ReadOnlyDirectoryGuard::new(environment.path())?;
    uv_snapshot!(context.filters(), context.sync()
        .arg("--preview-features")
        .arg("centralized-project-envs"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment `project-cp3.12.[X]-[HASH]`
    warning: Failed to remove existing local virtual environment at `.venv`: failed to remove file `[VENV]/pyvenv.cfg`: Permission denied (os error 13)
    Resolved 1 package in [TIME]
    Checked in [TIME]
    "#);

    assert!(pyvenv_cfg.is_file());
    assert!(context.cache_dir.child("environments-v2").is_dir());
    Ok(())
}

#[cfg(unix)]
#[test]
fn sync_centralized_env_link_creation_failure_preserves_cached_target() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&["3.12"])
        .with_filter((r"\.tmp[a-zA-Z0-9]+", "[TMP]"));
    write_project(&context, ">=3.12", &[])?;
    context
        .sync()
        .arg("--preview-features")
        .arg("centralized-project-envs")
        .assert()
        .success();

    let environment = context.temp_dir.child(".venv");
    // Record the cache target to verify the failed link update leaves it selected.
    let target = fs_err::read_link(environment.path())?;

    let _guard = ReadOnlyDirectoryGuard::new(context.temp_dir.path())?;
    uv_snapshot!(context.filters(), context.sync()
        .arg("--preview-features")
        .arg("centralized-project-envs"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: Failed to create link to project environment at `.venv`: Permission denied (os error 13) at path "[TEMP_DIR]/[TMP]"
    Resolved 1 package in [TIME]
    Checked in [TIME]
    "#);

    assert_eq!(target, fs_err::read_link(environment.path())?);
    assert!(target.join("pyvenv.cfg").is_file());
    Ok(())
}

#[test]
fn sync_replaces_environment_links_without_removing_cached_targets() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&["3.11", "3.12"]);
    write_project(&context, ">=3.11", &[])?;
    context
        .sync()
        .arg("--preview-features")
        .arg("centralized-project-envs")
        .arg("--python")
        .arg("3.12")
        .assert()
        .success();
    // Record the cache target so the final sync can verify it remains reusable.
    let cache_target = fs_err::read_link(context.temp_dir.child(".venv").path())?;

    let override_environment = context.temp_dir.child("override");
    uv_fs::create_symlink(&cache_target, override_environment.path())?;
    context
        .sync()
        .arg("--preview-features")
        .arg("centralized-project-envs")
        .arg("--python")
        .arg("3.11")
        .env(EnvVars::UV_PROJECT_ENVIRONMENT, "override")
        .assert()
        .success();

    // An explicit environment path is local, but replacing it does not remove the cached target.
    assert!(override_environment.is_dir());
    assert!(fs_err::read_link(override_environment.path()).is_err());

    let environment = context.temp_dir.child(".venv");
    let intermediate = context.temp_dir.child("intermediate");
    uv_fs::create_symlink(&cache_target, intermediate.path())?;
    uv_fs::replace_symlink(intermediate.path(), environment.path())?;

    uv_snapshot!(context.filters(), context
        .sync()
        .arg("--python")
        .arg("3.11"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.11.[X] interpreter at: [PYTHON-3.11]
    Removed link to project environment at: .venv
    Creating virtual environment at: .venv
    Resolved 1 package in [TIME]
    Checked in [TIME]
    "#);

    // Without the preview, uv replaces the indirect cache link with a local environment.
    assert!(environment.is_dir());
    assert!(fs_err::read_link(environment.path()).is_err());

    // uv rebuilds the linked environment without replacing the link.
    let target = context.temp_dir.child("environment");
    fs_err::rename(environment.path(), target.path())?;
    uv_fs::create_symlink(target.path(), environment.path())?;
    uv_snapshot!(context.filters(), context.sync()
        .arg("--python")
        .arg("3.12"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Removed virtual environment at: .venv
    Creating virtual environment at: .venv
    Resolved 1 package in [TIME]
    Checked in [TIME]
    "#);

    assert_eq!(fs_err::read_link(environment.path())?, target.path());
    // The link still points to the rebuilt Python 3.12 environment.
    let python = if cfg!(windows) {
        target.join("Scripts/python.exe")
    } else {
        target.join("bin/python")
    };
    uv_snapshot!(context.filters(), Command::new(python).arg("--version"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.[X]

    ----- stderr -----
    "#);

    // uv reuses the cached environment without recreating it.
    uv_snapshot!(context.filters(), context.sync()
        .arg("--preview-features")
        .arg("centralized-project-envs")
        .arg("--python")
        .arg("3.12"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Checked in [TIME]
    "#);
    assert_eq!(fs_err::read_link(environment.path())?, cache_target);
    Ok(())
}
