//! Variant dependency markers are scoped to the wheel that declares them.

use std::collections::BTreeMap;

use anyhow::{Result, anyhow};
use assert_fs::prelude::*;
use indoc::{formatdoc, indoc};
use serde_json::json;
use url::Url;

use uv_test::packse::generate_wheel_with_files;
use uv_test::uv_snapshot;

use super::variants::{write_metadata, write_wheel};

#[test]
fn pep825_ordinary_wheel_sync_markers() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    write_wheel(
        &context,
        "example",
        None,
        json!({}),
        &[
            "child; variant_label == 'gpu' or 'gpu' in variant_namespaces",
            "retained; variant_label == '' and 'gpu' not in variant_namespaces",
        ],
        None,
    )?;
    for name in ["child", "retained"] {
        write_wheel(&context, name, None, json!({}), &[], None)?;
    }
    let project = context.temp_dir.child("pyproject.toml");
    project.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["example", "child"]
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
     + retained==1.0.0
    "###);

    // Ordinary wheels have an empty label and property sets. An already installed child
    // must be removed when its only remaining dependency edge has a false variant marker.
    project.write_str(&context.read("pyproject.toml").replace(
        "dependencies = [\"example\", \"child\"]",
        "dependencies = [\"example\"]",
    ))?;
    uv_snapshot!(context.filters(), context.sync()
        .arg("--preview-features").arg("wheel-variants")
        .arg("--no-index").arg("--find-links").arg(context.temp_dir.path()), @r###"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 4 packages in [TIME]
    Uninstalled 1 package in [TIME]
     - child==1.0.0
    "###);
    Ok(())
}

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

        [tool.uv]
        required-environments = ["sys_platform == 'win32'"]
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

    // Each parent has a different label, but the child is still required on Windows.
    fs_err::remove_file(context.temp_dir.child("child-1.0.0-py3-none-any.whl"))?;
    fs_err::remove_file(context.temp_dir.child("uv.lock"))?;
    let (_, wheel) = generate_wheel_with_files(
        &"child".parse()?,
        &"1.0.0".parse()?,
        &[],
        &BTreeMap::new(),
        None,
        "py3-none-manylinux_2_17_x86_64",
        &[],
    );
    context
        .temp_dir
        .child("child-1.0.0-py3-none-manylinux_2_17_x86_64.whl")
        .write_binary(&wheel)?;
    let filters: Vec<_> = context
        .filters()
        .into_iter()
        .chain([(
            r"\nhint: The resolution failed for an environment that is not the current one[^\n]*",
            "",
        )])
        .collect();
    uv_snapshot!(filters, context.lock()
        .arg("--preview-features").arg("wheel-variants")
        .arg("--no-index").arg("--find-links").arg(context.temp_dir.path()), @r###"
    exit_code: 1 (failure)
    ----- stderr -----
    error: No solution found when resolving dependencies for split (markers: sys_platform == 'win32')
      cause: Because child{variant_label == ''}==1.0.0 has no Windows-compatible wheels and only child{variant_label == ''}==1.0.0 is available, we can conclude that all versions of child{variant_label == ''} cannot be used.
             And because all versions of middle depend on child{variant_label == ''}, we can conclude that all versions of middle cannot be used.
             And because all versions of example depend on middle{variant_label == 'null'} and your project depends on example, we can conclude that your project's requirements are unsatisfiable.
    "###);
    Ok(())
}
