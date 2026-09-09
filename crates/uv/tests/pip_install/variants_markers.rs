//! Variant dependency markers are scoped to the wheel that declares them.

use anyhow::Result;
use assert_fs::prelude::*;
use indoc::indoc;
use serde_json::json;

use uv_test::uv_snapshot;

use super::variants::{write_metadata, write_wheel};

#[test]
fn pep825_project_sync_nested_labels() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    write_wheel(
        &context,
        "example",
        Some("null"),
        json!({}),
        &["middle; variant_label == 'null'"],
        None,
    )?;
    write_wheel(
        &context,
        "middle",
        None,
        json!({}),
        &["child; variant_label == ''"],
        None,
    )?;
    write_wheel(&context, "child", None, json!({}), &[], None)?;
    write_metadata(&context, json!({"null": {}}))?;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["example"]
    "#})?;

    uv_snapshot!(context.filters(), context.sync()
        .arg("--preview-features").arg("wheel-variants")
        .arg("--no-index").arg("--find-links").arg(context.temp_dir.path()), @r###"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + child==1.0.0
     + example==1.0.0
     + middle==1.0.0
    "###);
    // Reading the saved lockfile must preserve each package's label context too.
    uv_snapshot!(context.filters(), context.sync()
        .arg("--preview-features").arg("wheel-variants")
        .arg("--frozen"), @r###"
    exit_code: 0 (success)
    ----- stderr -----
    Checked 3 packages in [TIME]
    "###);
    Ok(())
}
