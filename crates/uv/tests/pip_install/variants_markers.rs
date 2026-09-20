//! Variant dependency markers are scoped to the wheel that declares them.

use anyhow::{Result, anyhow};
use assert_fs::prelude::*;
use indoc::formatdoc;
use serde_json::json;
use url::Url;

use uv_test::uv_snapshot;

use super::variants::write_wheel;

#[test]
fn pep825_direct_wheel_universal_markers() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let missing = Url::from_file_path(
        context
            .temp_dir
            .child("missing-1.0.0-py3-none-any.whl")
            .path(),
    )
    .map_err(|()| anyhow!("expected an absolute wheel path"))?;
    write_wheel(
        &context,
        "fallback",
        Some("null"),
        json!({}),
        &[&format!(
            "missing @ {missing}; variant_label != 'null' or 'gpu' in variant_namespaces or 'gpu :: cuda' in variant_features or 'gpu :: cuda :: 12.0' in variant_properties"
        )],
        None,
    )?;
    write_wheel(
        &context,
        "ordinary",
        None,
        json!({}),
        &[&format!(
            "missing @ {missing}; variant_label != '' or 'gpu' in variant_namespaces"
        )],
        None,
    )?;
    write_wheel(
        &context,
        "accelerated",
        Some("fast"),
        json!({"gpu": {"cuda": ["12.0", "13.0"]}}),
        &[
            &format!(
                "missing @ {missing}; variant_label != 'fast' or variant_label not in 'fast' or 'slow' in variant_label"
            ),
            "first; 'gpu :: cuda :: 12.0' in variant_properties",
            "second; 'gpu :: cuda :: 13.0' in variant_properties",
        ],
        None,
    )?;
    for name in ["first", "second"] {
        write_wheel(&context, name, None, json!({}), &[], None)?;
    }
    let dependencies = [
        ("fallback", "-null"),
        ("ordinary", ""),
        ("accelerated", "-fast"),
    ]
    .into_iter()
    .map(|(name, label)| {
        let wheel = context
            .temp_dir
            .child(format!("{name}-1.0.0-py3-none-any{label}.whl"));
        let url = Url::from_file_path(wheel.path())
            .map_err(|()| anyhow!("expected an absolute wheel path"))?;
        Ok(format!("{name} @ {url}"))
    })
    .collect::<Result<Vec<_>>>()?;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(&formatdoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = {}
        "#,
            serde_json::to_string(&dependencies)?,
        })?;

    // Fixed labels exclude the missing direct dependency, while both target-dependent
    // property alternatives must remain in the universal lockfile.
    uv_snapshot!(context.filters(), context.lock()
        .arg("--preview-features").arg("wheel-variants")
        .arg("--no-index").arg("--find-links").arg(context.temp_dir.path()), @r###"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 6 packages in [TIME]
    "###);
    Ok(())
}
