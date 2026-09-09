//! Inspect and reuse installed variants without executing their providers again.

use std::collections::BTreeMap;

use anyhow::Result;
use assert_fs::prelude::*;
use indoc::indoc;
use serde_json::json;

use uv_test::packse::generate_wheel_with_files;
use uv_test::uv_snapshot;
use uv_variants::variants_json::VARIANT_SCHEMA;

use super::variants::{write_metadata, write_wheel};

/// Inspection uses the supported subset saved during installation, even after the target
/// property file is gone. A wheel's unsupported alternative must not become a dependency.
#[test]
fn pep825_installed_marker_context() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let properties = json!({"gpu": {"cuda": ["13.0", "14.0"]}});
    let dependencies = [
        "supported; 'gpu :: cuda :: 12.0' in variant_properties",
        "supported; 'gpu :: cuda :: 13.0' in variant_properties",
        "missing; 'gpu :: cuda :: 14.0' in variant_properties",
        "missing; variant_label == 'other'",
    ];
    write_wheel(
        &context,
        "example",
        Some("gpu"),
        properties.clone(),
        &dependencies,
        None,
    )?;
    write_wheel(&context, "supported", None, json!({}), &[], None)?;
    write_metadata(&context, json!({"gpu": properties}))?;

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("example").arg("--preview-features").arg("wheel-variants")
        .arg("--no-index").arg("--find-links").arg(context.temp_dir.path())
        .env("UV_VARIANT_LOCK", context.temp_dir.child("target.toml").path()), @r###"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + example==1.0.0
     + supported==1.0.0
    "###);

    fs_err::remove_file(context.temp_dir.child("target.toml").path())?;
    uv_snapshot!(context.filters(), context.pip_check(), @r###"
    exit_code: 0 (success)
    ----- stderr -----
    Checked 2 packages in [TIME]
    All installed packages are compatible
    "###);
    uv_snapshot!(context.filters(), context.pip_show().arg("example"), @r###"
    exit_code: 0 (success)
    ----- stdout -----
    Name: example
    Version: 1.0.0
    Location: [SITE_PACKAGES]/
    Requires: supported
    Required-by:
    "###);
    uv_snapshot!(context.filters(), context.pip_show().arg("supported"), @r###"
    exit_code: 0 (success)
    ----- stdout -----
    Name: supported
    Version: 1.0.0
    Location: [SITE_PACKAGES]/
    Requires:
    Required-by: example
    "###);
    uv_snapshot!(context.filters(), context.pip_tree(), @r###"
    exit_code: 0 (success)
    ----- stdout -----
    example v1.0.0
    └── supported v1.0.0
    "###);
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("example").arg("--preview-features").arg("wheel-variants")
        .arg("--no-index").arg("--find-links").arg(context.temp_dir.path()), @r###"
    exit_code: 0 (success)
    ----- stderr -----
    Checked 1 package in [TIME]
    "###);

    // The supported dependency is still required; the persisted context must not suppress it.
    uv_snapshot!(context.filters(), context.pip_uninstall().arg("supported"), @r###"
    exit_code: 0 (success)
    ----- stderr -----
    Uninstalled 1 package in [TIME]
     - supported==1.0.0
    "###);
    uv_snapshot!(context.filters(), context.pip_check(), @r###"
    exit_code: 1 (failure)
    ----- stderr -----
    Checked 1 package in [TIME]
    Found 1 incompatibility
    The package `example` requires `supported ; 'gpu :: cuda :: 13.0' in variant_properties`, but it's not installed
    "###);

    // An explicit new target overrides the saved subset, even when the wheel is reused.
    write_wheel(&context, "missing", None, json!({}), &[], None)?;
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
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("example").arg("--preview-features").arg("wheel-variants")
        .arg("--no-index").arg("--find-links").arg(context.temp_dir.path())
        .env("UV_VARIANT_LOCK", context.temp_dir.child("target.toml").path()), @r###"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 2 packages in [TIME]
     ~ example==1.0.0
     + missing==1.0.0
    "###);
    // Inspection without a target must see the newly recorded supported subset.
    uv_snapshot!(context.filters(), context.pip_check(), @r###"
    exit_code: 0 (success)
    ----- stderr -----
    Checked 2 packages in [TIME]
    All installed packages are compatible
    "###);

    // An incompatible installed wheel must not prevent selection of another compatible variant.
    let older = json!({"gpu": {"cuda": ["12.0"]}});
    write_wheel(
        &context,
        "example",
        Some("older"),
        older.clone(),
        &dependencies,
        None,
    )?;
    write_metadata(&context, json!({"gpu": properties, "older": older}))?;
    context.temp_dir.child("target.toml").write_str(indoc! {r#"
        [metadata]
        version = "0.1"
        created-by = "uv-test"
        [[provider]]
        namespace = "gpu"
        resolved = []
        [provider.properties]
        cuda = ["12.0"]
    "#})?;
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("example").arg("--preview-features").arg("wheel-variants")
        .arg("--no-index").arg("--find-links").arg(context.temp_dir.path())
        .env("UV_VARIANT_LOCK", context.temp_dir.child("target.toml").path()), @r###"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 2 packages in [TIME]
     ~ example==1.0.0
     + supported==1.0.0
    "###);
    uv_snapshot!(context.filters(), context.python_command().arg("-c").arg(indoc! {r#"
        import example
        from pathlib import Path
        print(Path(example.__file__).with_name("selected.txt").read_text())
    "#}), @r###"
    exit_code: 0 (success)
    ----- stdout -----
    older build 0
    "###);
    Ok(())
}

/// Outdated checks must follow the same preview gate as installation, even though they only
/// inspect filenames and do not resolve or download the advertised newer wheel.
#[test]
fn wheel_variants_preview_outdated() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    write_wheel(&context, "example", None, json!({}), &[], None)?;
    let metadata = serde_json::to_string(&json!({
        "$schema": VARIANT_SCHEMA,
        "default-priorities": {"namespace": ["cpu"]},
        "variants": {"null": {}},
    }))?;
    let (_, wheel) = generate_wheel_with_files(
        &"example".parse()?,
        &"2.0.0".parse()?,
        &[],
        &BTreeMap::new(),
        None,
        "py3-none-any",
        &[("example-2.0.0.dist-info/variant.json", &metadata)],
    );
    context
        .temp_dir
        .child("example-2.0.0-py3-none-any-null.whl")
        .write_binary(&wheel)?;

    uv_snapshot!(context.filters(), context.pip_install().arg("example")
        .arg("--no-index").arg("--find-links").arg(context.temp_dir.path()), @r###"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + example==1.0.0
    "###);
    uv_snapshot!(context.filters(), context.pip_list().arg("--outdated").arg("--format=json")
        .arg("--no-index").arg("--find-links").arg(context.temp_dir.path()), @r###"
    exit_code: 0 (success)
    ----- stdout -----
    []

    ----- stderr -----
    warning: example-1.0.0-py3-none-any.whl is missing an upload date, but user provided: 2024-03-25T00:00:00Z
    "###);
    uv_snapshot!(context.filters(), context.pip_list().arg("--outdated").arg("--format=json")
        .arg("--preview-features").arg("wheel-variants")
        .arg("--no-index").arg("--find-links").arg(context.temp_dir.path()), @r###"
    exit_code: 0 (success)
    ----- stdout -----
    [{"name":"example","version":"1.0.0","latest_version":"2.0.0","latest_filetype":"wheel"}]

    ----- stderr -----
    warning: example-1.0.0-py3-none-any.whl is missing an upload date, but user provided: 2024-03-25T00:00:00Z
    warning: example-2.0.0-py3-none-any-null.whl is missing an upload date, but user provided: 2024-03-25T00:00:00Z
    "###);
    Ok(())
}
