//! Tests for validation of preview settings and preview feature flags.
//!
//! Unknown preview settings must produce a clear error or warning instead of being silently
//! ignored. See <https://github.com/astral-sh/uv/issues/22950>.

use anyhow::Result;
use assert_fs::prelude::*;
use indoc::indoc;
use uv_test::uv_snapshot;

/// An unknown `--preview-features` flag warns instead of being silently ignored.
#[test]
fn unknown_preview_feature_flag_warns() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);

    context.temp_dir.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
    "#})?;

    uv_snapshot!(context.filters(), context.version().arg("--preview-features").arg("nonexistent-flag"), @r#"
    exit_code: 0 (success)
    ----- stdout -----
    project 0.1.0

    ----- stderr -----
    warning: Unknown preview feature: `nonexistent-flag`
    "#);
    Ok(())
}

/// An unknown key in a `[tool.uv.preview]` table warns during settings discovery, instead of
/// being silently ignored.
#[test]
fn unknown_preview_setting_in_pyproject_warns() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);

    context.temp_dir.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [tool.uv.preview]
        nonexistent = true
    "#})?;

    uv_snapshot!(context.filters(), context.version(), @r#"
    exit_code: 0 (success)
    ----- stdout -----
    project 0.1.0

    ----- stderr -----
    warning: Failed to parse `pyproject.toml` during settings discovery:
      TOML parse error at line 6, column 1
        |
      6 | [tool.uv.preview]
        | ^^^^^^^^^^^^^^^^^
      unknown preview feature `nonexistent` in the `preview` table; the `preview` setting must be a boolean; to enable specific preview features, use `preview-features` (e.g. `preview-features = ["json-output"]`)
    "#);
    Ok(())
}

/// An unknown key in a `[preview]` table in a `uv.toml` is a hard error.
#[test]
fn unknown_preview_setting_in_uv_toml_errors() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);

    context.temp_dir.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
    "#})?;
    context.temp_dir.child("uv.toml").write_str(indoc! {r#"
        [preview]
        nonexistent = true
    "#})?;

    uv_snapshot!(context.filters(), context.version(), @r#"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Failed to parse: uv.toml
      cause: TOML parse error at line 1, column 1
               |
             1 | [preview]
               | ^^^^^^^^^
             unknown preview feature `nonexistent` in the `preview` table; the `preview` setting must be a boolean; to enable specific preview features, use `preview-features` (e.g. `preview-features = ["json-output"]`)
    "#);
    Ok(())
}

/// A `preview` list of unknown feature names in a `uv.toml` is a hard error.
#[test]
fn unknown_preview_feature_list_in_uv_toml_errors() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);

    context.temp_dir.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
    "#})?;
    context
        .temp_dir
        .child("uv.toml")
        .write_str(r#"preview = ["nonexistent"]"#)?;

    uv_snapshot!(context.filters(), context.version(), @r#"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Failed to parse: uv.toml
      cause: TOML parse error at line 1, column 11
               |
             1 | preview = ["nonexistent"]
               |           ^^^^^^^^^^^^^^^
             unknown preview feature `nonexistent` in the `preview` list; the `preview` setting must be a boolean; to enable specific preview features, use `preview-features` (e.g. `preview-features = ["json-output"]`)
    "#);
    Ok(())
}
