use anyhow::Result;
use assert_cmd::assert::OutputAssertExt;
use assert_fs::prelude::*;
use indoc::indoc;
use insta::assert_snapshot;

use uv_lock::Lock;
use uv_test::uv_snapshot;

/// Upgrade an existing lockfile without silently accepting it under `--locked`.
#[test]
fn lockfile_v2_upgrade() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [tool.uv]
        default-groups = ["docs"]

        [tool.uv.sources]
        child = { path = "child" }

        [dependency-groups]
        docs = ["child"]
    "#})?;
    context
        .temp_dir
        .child("child/pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "child"
        version = "1.0.0"

        [tool.uv]
        package = false
    "#})?;

    context.lock().arg("--offline").assert().success();
    let original = context.read("uv.lock");
    uv_snapshot!(context.filters(), context.lock().args(["--offline", "--locked", "--preview-features", "lockfile-v2"]), @"
    exit_code: 1 (failure)
    ----- stderr -----
    warning: The `lockfile-v2` feature is highly experimental. The lockfile format may change incompatibly in patch releases.
    Resolved 2 packages in [TIME]
    error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided.

    hint: To update the lockfile, run `uv lock`.
    ");
    assert_eq!(context.read("uv.lock"), original);

    uv_snapshot!(context.filters(), context.lock().args(["--offline", "--preview-features", "lockfile-v2"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: The `lockfile-v2` feature is highly experimental. The lockfile format may change incompatibly in patch releases.
    Resolved 2 packages in [TIME]
    ");
    let upgraded = context.read("uv.lock");
    // The v2 field names must be handled by the canonical parser, without a TOML fallback.
    assert_eq!(Lock::from_canonical_toml(&upgraded)?.to_toml()?, upgraded);
    assert_snapshot!(upgraded, @r#"
    version = 2
    requires-python = ">=3.12"

    [options]
    exclude-newer = "2024-03-25T00:00:00Z"

    [[package]]
    name = "child"
    version = "1.0.0"
    source = { virtual = "child" }

    [[package]]
    name = "project"
    version = "0.1.0"
    source = { virtual = "." }
    default-groups = ["docs"]

    [package.dependency-groups]
    docs = [
        { name = "child" },
    ]

    [package.metadata]

    [package.metadata.dependency-groups]
    docs = [{ name = "child", virtual = "child" }]
    "#);

    uv_snapshot!(context.filters(), context.lock().args(["--offline", "--locked", "--preview-features", "lockfile-v2"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: The `lockfile-v2` feature is highly experimental. The lockfile format may change incompatibly in patch releases.
    Resolved 2 packages in [TIME]
    ");
    assert_eq!(context.read("uv.lock"), upgraded);

    // Frozen reads require the feature too, including when no resolution is needed.
    uv_snapshot!(context.filters(), context.sync().args(["--offline", "--frozen"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: The lockfile at `uv.lock` uses an unsupported schema version (v2, but versions up to v1 are supported). Downgrade to a compatible uv version, or remove the `uv.lock` prior to running `uv lock` or `uv sync`.
    ");
    uv_snapshot!(context.filters(), context.sync().args(["--offline", "--frozen", "--preview-features", "lockfile-v2"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: The `lockfile-v2` feature is highly experimental. The lockfile format may change incompatibly in patch releases.
    Checked in [TIME]
    ");
    assert_eq!(context.read("uv.lock"), upgraded);

    Ok(())
}

/// Read the feature from project configuration when creating a lockfile.
#[test]
fn lockfile_v2_config() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [tool.uv]
        preview-features = ["lockfile-v2"]
    "#})?;

    uv_snapshot!(context.filters(), context.lock().arg("--offline"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    warning: The `lockfile-v2` feature is highly experimental. The lockfile format may change incompatibly in patch releases.
    ");
    assert_snapshot!(context.read("uv.lock"), @r#"
    version = 2
    requires-python = ">=3.12"

    [options]
    exclude-newer = "2024-03-25T00:00:00Z"

    [[package]]
    name = "project"
    version = "0.1.0"
    source = { virtual = "." }
    "#);
    uv_snapshot!(context.filters(), context.lock().args(["--offline", "--locked"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: The `lockfile-v2` feature is highly experimental. The lockfile format may change incompatibly in patch releases.
    Resolved 1 package in [TIME]
    ");

    Ok(())
}

/// Apply the format feature to script lockfiles, including noncanonical TOML reads.
#[test]
fn lockfile_v2_script() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    context.temp_dir.child("script.py").write_str(indoc! {r#"
        # /// script
        # requires-python = ">=3.12"
        # dependencies = []
        # ///
    "#})?;

    uv_snapshot!(context.filters(), context.lock().args(["--script", "script.py", "--offline"]).env("UV_PREVIEW_FEATURES", "lockfile-v2"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved in [TIME]
    warning: The `lockfile-v2` feature is highly experimental. The lockfile format may change incompatibly in patch releases.
    ");
    assert_snapshot!(context.read("script.py.lock"), @r#"
    version = 2
    requires-python = ">=3.12"

    [options]
    exclude-newer = "2024-03-25T00:00:00Z"
    "#);

    context
        .temp_dir
        .child("script.py.lock")
        .write_str(indoc! {r"
        # Hand-edited lockfile using noncanonical TOML.
        version = 2
        requires-python = '>=3.12'

        [options]
        exclude-newer = '2024-03-25T00:00:00Z'
    "})?;
    uv_snapshot!(context.filters(), context.lock().args(["--script", "script.py", "--offline", "--locked", "--preview-features", "lockfile-v2"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: The `lockfile-v2` feature is highly experimental. The lockfile format may change incompatibly in patch releases.
    Resolved in [TIME]
    ");

    Ok(())
}

/// Enabling v2 still rejects unknown schema versions, even when their contents cannot be parsed.
#[test]
fn lockfile_v2_unsupported_version() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
    "#})?;
    context.temp_dir.child("uv.lock").write_str(indoc! {r#"
        version = 3
        requires-python = ">=3.12"
    "#})?;
    uv_snapshot!(context.filters(), context.lock().args(["--frozen", "--preview-features", "lockfile-v2"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    warning: The `lockfile-v2` feature is highly experimental. The lockfile format may change incompatibly in patch releases.
    error: The lockfile at `uv.lock` uses an unsupported schema version (v3, but versions up to v2 are supported). Downgrade to a compatible uv version, or remove the `uv.lock` prior to running `uv lock` or `uv sync`.
    ");

    context.temp_dir.child("uv.lock").write_str(indoc! {r"
        version = 3
        requires-python = false
    "})?;
    uv_snapshot!(context.filters(), context.lock().args(["--frozen", "--preview-features", "lockfile-v2"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    warning: The `lockfile-v2` feature is highly experimental. The lockfile format may change incompatibly in patch releases.
    error: Failed to parse `uv.lock`, which uses an unsupported schema version (v3, but versions up to v2 are supported). Downgrade to a compatible uv version, or remove the `uv.lock` prior to running `uv lock` or `uv sync`.
      cause: TOML parse error at line 2, column 19
               |
             2 | requires-python = false
               |                   ^^^^^
             invalid type: boolean `false`, expected a string
    ");

    Ok(())
}
