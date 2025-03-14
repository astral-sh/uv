use anyhow::Result;
use assert_cmd::prelude::*;
use assert_fs::prelude::*;
use indoc::indoc;

use uv_static::EnvVars;

use crate::common::uv_snapshot;
use crate::common::TestContext;

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

    let filters: Vec<_> = context
        .filters()
        .into_iter()
        .chain(std::iter::once((r"Removed \d+ files", "Removed [N] files")))
        .collect();

    uv_snapshot!(&filters, context.prune().arg("--verbose"), @r###"
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

    let filters: Vec<_> = context
        .filters()
        .into_iter()
        .chain(std::iter::once((r"Removed \d+ files", "Removed [N] files")))
        .collect();

    uv_snapshot!(&filters, context.prune().arg("--verbose"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    DEBUG uv [VERSION] ([COMMIT] DATE)
    Pruning cache at: [CACHE_DIR]/
    DEBUG Removing dangling cache bucket: [CACHE_DIR]/simple-v4
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

    let filters: Vec<_> = context
        .filters()
        .into_iter()
        .chain(std::iter::once((r"Removed \d+ files", "Removed [N] files")))
        .collect();

    uv_snapshot!(&filters, context.tool_run()
        .arg("pytest@8.0.0")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
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
    "###);

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

    uv_snapshot!(filters, context.prune().arg("--verbose"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    DEBUG uv [VERSION] ([COMMIT] DATE)
    Pruning cache at: [CACHE_DIR]/
    DEBUG Removing dangling cache environment: [CACHE_DIR]/environments-v2/[ENTRY]
    DEBUG Removing dangling cache archive: [CACHE_DIR]/archive-v0/[ENTRY]
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
    let wheels = context.cache_dir.child("wheels-v5");
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

    uv_snapshot!(filters, context.prune().arg("--verbose"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    DEBUG uv [VERSION] ([COMMIT] DATE)
    Pruning cache at: [CACHE_DIR]/
    DEBUG Removing dangling cache archive: [CACHE_DIR]/archive-v0/[ENTRY]
    Removed 44 files ([SIZE])
    "###);

    Ok(())
}

/// `cache prune --ci` should remove all unzipped archives.
#[test]
fn prune_unzipped() -> Result<()> {
    let context = TestContext::new("3.12").with_exclude_newer("2025-01-01T00:00Z");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(indoc! { r"
        source-distribution==0.0.1
        iniconfig
    " })?;

    let filters: Vec<_> = std::iter::once((r"Removed \d+ files", "Removed [N] files"))
        .chain(context.filters())
        .collect();

    // Install a requirement, to populate the cache.
    uv_snapshot!(&filters, context.pip_install().arg("-r").arg("requirements.txt").arg("--reinstall"), @r###"
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

    uv_snapshot!(&filters, context.prune().arg("--ci"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Pruning cache at: [CACHE_DIR]/
    Removed [N] files ([SIZE])
    "###);

    context.venv().assert().success();

    // Reinstalling the source distribution should not require re-downloading the source
    // distribution.
    requirements_txt.write_str(indoc! { r"
        source-distribution==0.0.1
    " })?;
    uv_snapshot!(&filters, context.pip_install().arg("-r").arg("requirements.txt").arg("--offline"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + source-distribution==0.0.1
    "###);

    // But reinstalling the other package should require a download, since we pruned the wheel.
    requirements_txt.write_str(indoc! { r"
        iniconfig
    " })?;
    uv_snapshot!(&filters, context.pip_install().arg("-r").arg("requirements.txt").arg("--offline"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because all versions of iniconfig need to be downloaded from a registry and you require iniconfig, we can conclude that your requirements are unsatisfiable.

          hint: Pre-releases are available for `iniconfig` in the requested range (e.g., 0.2.dev0), but pre-releases weren't enabled (try: `--prerelease=allow`)

          hint: Packages were unavailable because the network was disabled. When the network is disabled, registry packages may only be read from the cache.
    "###);

    Ok(())
}

/// `cache prune` should remove any stale source distribution revisions.
#[test]
fn prune_stale_revision() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    context.temp_dir.child("src").child("__init__.py").touch()?;
    context.temp_dir.child("README").touch()?;

    let filters: Vec<_> = context
        .filters()
        .into_iter()
        .chain(std::iter::once((r"Removed \d+ files", "Removed [N] files")))
        .collect();

    // Install the same package twice, with `--reinstall`.
    uv_snapshot!(&filters, context
        .pip_install()
        .arg(".")
        .arg("--reinstall"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + project==0.1.0 (from file://[TEMP_DIR]/)
    "###);

    uv_snapshot!(&filters, context
        .pip_install()
        .arg(".")
        .arg("--reinstall"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     ~ project==0.1.0 (from file://[TEMP_DIR]/)
    "###);

    let filters: Vec<_> = filters
        .into_iter()
        .chain([
            // The cache entry does not have a stable key, so we filter it out
            (
                r"\[CACHE_DIR\](\\|\/)(.*?)(\\|\/).*",
                "[CACHE_DIR]/$2/[ENTRY]",
            ),
        ])
        .collect();

    // Pruning should remove the unused revision.
    uv_snapshot!(&filters, context.prune().arg("--verbose"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    DEBUG uv [VERSION] ([COMMIT] DATE)
    Pruning cache at: [CACHE_DIR]/
    DEBUG Removing dangling source revision: [CACHE_DIR]/sdists-v8/[ENTRY]
    DEBUG Removing dangling cache archive: [CACHE_DIR]/archive-v0/[ENTRY]
    Removed [N] files ([SIZE])
    "###);

    // Uninstall and reinstall the package. We should use the cached version.
    uv_snapshot!(&filters, context
        .pip_uninstall()
        .arg("."), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Uninstalled 1 package in [TIME]
     - project==0.1.0 (from file://[TEMP_DIR]/)
    "###);

    uv_snapshot!(&filters, context
        .pip_install()
        .arg("."), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + project==0.1.0 (from file://[TEMP_DIR]/)
    "###);

    Ok(())
}
