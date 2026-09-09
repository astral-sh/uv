//! PEP 825 selection using local wheels and explicit target properties.

use std::collections::BTreeMap;

use anyhow::Result;
use assert_fs::prelude::*;
use indoc::indoc;
use serde_json::{Value, json};

use uv_test::packse::generate_wheel_with_files;
use uv_test::{TestContext, uv_snapshot};
use uv_variants::variants_json::VARIANT_SCHEMA;

pub(super) fn write_wheel(
    context: &TestContext,
    name: &str,
    label: Option<&str>,
    properties: Value,
    dependencies: &[&str],
    build: Option<u8>,
) -> Result<()> {
    let mut metadata = json!({
        "$schema": VARIANT_SCHEMA,
        "default-priorities": {"namespace": ["gpu", "cpu"]},
        "variants": {},
    });
    metadata["variants"][label.unwrap_or("null")] = properties;
    let name = name.parse()?;
    let version = "1.0.0".parse()?;
    let dependencies = dependencies
        .iter()
        .map(|requirement| requirement.parse())
        .collect::<Result<Vec<_>, _>>()?;
    let metadata_path = format!("{name}-{version}.dist-info/variant.json");
    let marker_path = format!("{name}/selected.txt");
    let metadata_json = serde_json::to_string(&metadata)?;
    let selected = format!(
        "{} build {}",
        label.unwrap_or("nonvariant"),
        build.unwrap_or(0)
    );
    let mut files = vec![(marker_path.as_str(), selected.as_str())];
    if label.is_some() {
        files.push((metadata_path.as_str(), metadata_json.as_str()));
    }
    let (_, wheel) = generate_wheel_with_files(
        &name,
        &version,
        &dependencies,
        &BTreeMap::new(),
        None,
        "py3-none-any",
        &files,
    );
    let build = build.map(|build| format!("-{build}")).unwrap_or_default();
    let label = label.map(|label| format!("-{label}")).unwrap_or_default();
    context
        .temp_dir
        .child(format!("{name}-{version}{build}-py3-none-any{label}.whl"))
        .write_binary(&wheel)?;
    Ok(())
}

pub(super) fn write_metadata(context: &TestContext, variants: Value) -> Result<()> {
    let mut metadata = json!({
        "$schema": VARIANT_SCHEMA,
        "default-priorities": {"namespace": ["gpu", "cpu"]},
        "variants": {},
    });
    metadata["variants"] = variants;
    context
        .temp_dir
        .child("example-1.0.0-variants.json")
        .write_str(&serde_json::to_string(&metadata)?)?;
    context.temp_dir.child("target.toml").write_str(indoc! {r#"
        [metadata]
        version = "0.1"
        created-by = "uv-test"
        [[provider]]
        namespace = "gpu"
        resolved = []
        [provider.properties]
        cuda = ["13.0", "12.0"]
        [[provider]]
        namespace = "cpu"
        resolved = []
        [provider.properties]
        level = ["v4", "v3", "v2"]
    "#})?;
    Ok(())
}

#[test]
fn pep825_selection_and_markers() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let properties = json!({"gpu": {"cuda": ["12.0", "13.0", "14.0"]}, "cpu": {"level": ["v4"]}});
    let dependencies = [
        "supported; 'gpu :: cuda :: 12.0' in variant_properties",
        "unsupported; 'gpu :: cuda :: 14.0' in variant_properties",
        "labelled; variant_label == 'a_long_variant_label_over_16_chars'",
    ];
    write_wheel(
        &context,
        "example",
        Some("a_long_variant_label_over_16_chars"),
        properties.clone(),
        &dependencies,
        Some(1),
    )?;
    write_wheel(
        &context,
        "example",
        Some("a_long_variant_label_over_16_chars"),
        properties.clone(),
        &dependencies,
        Some(2),
    )?;
    write_wheel(
        &context,
        "example",
        Some("z"),
        properties.clone(),
        &[],
        Some(9),
    )?;
    write_wheel(
        &context,
        "example",
        Some("gpu"),
        json!({"gpu": {"cuda": ["13.0"]}}),
        &[],
        None,
    )?;
    write_wheel(&context, "example", Some("null"), json!({}), &[], None)?;
    write_wheel(&context, "example", None, json!({}), &[], None)?;
    write_wheel(
        &context,
        "supported",
        None,
        json!({}),
        &["leaked; 'gpu' in variant_namespaces"],
        None,
    )?;
    write_wheel(&context, "labelled", None, json!({}), &[], None)?;
    write_metadata(
        &context,
        json!({
            "a_long_variant_label_over_16_chars": properties,
            "z": properties,
            "gpu": {"gpu": {"cuda": ["13.0"]}},
            "null": {},
        }),
    )?;
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("example").arg("--no-index").arg("--find-links").arg(context.temp_dir.path())
        .env("UV_VARIANT_LOCK", context.temp_dir.child("target.toml").path()), @r###"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + example==1.0.0
     + labelled==1.0.0
     + supported==1.0.0
    "###);
    uv_snapshot!(context.filters(), context.python_command().arg("-c").arg(indoc! {r#"
        import example
        from pathlib import Path
        print(Path(example.__file__).with_name("selected.txt").read_text())
    "#}), @r###"
    exit_code: 0 (success)
    ----- stdout -----
    a_long_variant_label_over_16_chars build 2
    "###);
    Ok(())
}

#[test]
fn pep825_null_fallback() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    write_wheel(
        &context,
        "example",
        Some("unsupported"),
        json!({"gpu": {"cuda": ["14.0"]}}),
        &[],
        None,
    )?;
    write_wheel(
        &context,
        "example",
        Some("null"),
        json!({}),
        &["labelled; variant_label == 'null'"],
        None,
    )?;
    write_wheel(&context, "example", None, json!({}), &[], None)?;
    write_wheel(&context, "labelled", None, json!({}), &[], None)?;
    write_metadata(
        &context,
        json!({"unsupported": {"gpu": {"cuda": ["14.0"]}}, "null": {}}),
    )?;
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("example").arg("--no-index").arg("--find-links").arg(context.temp_dir.path())
        .env("UV_VARIANT_LOCK", context.temp_dir.child("target.toml").path()), @r###"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + example==1.0.0
     + labelled==1.0.0
    "###);
    Ok(())
}

#[test]
fn pep825_invalid_metadata_fallback() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    write_wheel(&context, "example", Some("null"), json!({}), &[], None)?;
    write_wheel(
        &context,
        "example",
        None,
        json!({}),
        &["labelled; variant_label == '' and variant_label in 'anything'"],
        None,
    )?;
    write_wheel(&context, "labelled", None, json!({}), &[], None)?;
    context
        .temp_dir
        .child("example-1.0.0-variants.json")
        .write_str(r#"{"$schema": "unsupported"}"#)?;
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("example").arg("--no-index").arg("--find-links").arg(context.temp_dir.path()), @r###"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + example==1.0.0
     + labelled==1.0.0
    "###);
    Ok(())
}

#[test]
fn pep825_project_sync() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let properties = json!({"gpu": {"cuda": ["12.0", "13.0", "14.0"]}});
    let dependencies = [
        "supported; 'gpu :: cuda :: 12.0' in variant_properties",
        "unsupported; 'gpu :: cuda :: 14.0' in variant_properties",
    ];
    write_wheel(
        &context,
        "example",
        Some("fast"),
        properties.clone(),
        &dependencies,
        None,
    )?;
    write_wheel(
        &context,
        "example",
        Some("null"),
        json!({}),
        &dependencies,
        None,
    )?;
    write_wheel(&context, "example", None, json!({}), &dependencies, None)?;
    write_wheel(&context, "supported", None, json!({}), &[], None)?;
    write_wheel(&context, "unsupported", None, json!({}), &[], None)?;
    write_metadata(&context, json!({"fast": properties, "null": {}}))?;
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
        .arg("--no-index").arg("--find-links").arg(context.temp_dir.path())
        .env("UV_VARIANT_LOCK", context.temp_dir.child("target.toml").path()), @r###"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + example==1.0.0
     + supported==1.0.0
    "###);
    // A changed host can change the dependency set without changing the selected label.
    context.temp_dir.child("target.toml").write_str(indoc! {r#"
        [metadata]
        version = "0.1"
        created-by = "uv-test"
        [[provider]]
        namespace = "gpu"
        resolved = []
        [provider.properties]
        cuda = ["14.0"]
    "#})?;
    uv_snapshot!(context.filters(), context.sync().arg("--locked")
        .arg("--no-index").arg("--find-links").arg(context.temp_dir.path())
        .env("UV_VARIANT_LOCK", context.temp_dir.child("target.toml").path()), @r###"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - supported==1.0.0
     + unsupported==1.0.0
    "###);
    Ok(())
}
