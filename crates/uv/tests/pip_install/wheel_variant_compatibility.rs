use anyhow::Result;
use assert_fs::prelude::*;
use serde_json::{Value, json};
use uv_test::find_links::FindLinksServer;
use uv_test::packse::WheelBuilder;
use uv_test::uv_snapshot;

fn null_variant() -> Value {
    json!({
        "$schema": "https://variants-schema.wheelnext.dev/peps/825/v0.1.1.json",
        "default-priorities": { "namespace": ["x86_64"] },
        "variants": { "null": {} },
    })
}

/// Sync keeps an installed variant even when wheel variants are no longer enabled.
#[test]
fn sync_keeps_installed_variant() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let links = context.temp_dir.child("links");
    links.create_dir_all()?;
    let variant = null_variant().to_string();
    let (_, wheel) = WheelBuilder::new(&"ok".parse()?, &"1.0.0".parse()?)
        .files(&[("ok-1.0.0.dist-info/variant.json", &variant)])
        .build();
    links
        .child("ok-1.0.0-py3-none-any-null.whl")
        .write_binary(&wheel)?;
    links.child("ok-1.0.0-variants.json").write_str(&variant)?;

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--no-index")
        .arg("--find-links")
        .arg(links.path())
        .arg("--preview-features")
        .arg("wheel-variants")
        .arg("ok==1.0.0"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + ok==1.0.0
    ");

    context
        .temp_dir
        .child("requirements.txt")
        .write_str("ok==1.0.0")?;
    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.txt")
        .arg("--no-index")
        .arg("--find-links")
        .arg(links.path()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Checked 1 package in [TIME]
    ");
    Ok(())
}

/// A variant-only release cannot be represented as an installable package in `pylock.toml`.
#[test]
fn pylock_rejects_selected_wheel_variant() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let links = context.temp_dir.child("links");
    links.create_dir_all()?;
    let variant = null_variant().to_string();
    let (_, wheel) = WheelBuilder::new(&"ok".parse()?, &"1.0.0".parse()?)
        .files(&[("ok-1.0.0.dist-info/variant.json", &variant)])
        .build();
    links
        .child("ok-1.0.0-py3-none-any-null.whl")
        .write_binary(&wheel)?;
    links.child("ok-1.0.0-variants.json").write_str(&variant)?;
    context
        .temp_dir
        .child("requirements.in")
        .write_str("ok==1.0.0")?;

    uv_snapshot!(context.filters(), context.pip_compile()
        .arg("requirements.in")
        .arg("--no-index")
        .arg("--find-links")
        .arg(links.path())
        .arg("--preview-features")
        .arg("wheel-variants")
        .arg("--no-header")
        .arg("-o")
        .arg("pylock.toml"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    Resolved 1 package in [TIME]
    error: Package `ok` resolved to a wheel variant, which pylock.toml export does not support
    ");
    context
        .temp_dir
        .child("pylock.toml")
        .assert(predicates::path::missing());
    Ok(())
}

/// Streaming metadata must not combine files from two different `.dist-info` directories.
#[test]
fn streamed_variant_metadata_requires_one_dist_info_directory() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let links = context.temp_dir.child("links");
    links.create_dir_all()?;
    let variant = null_variant().to_string();
    let (_, wheel) = WheelBuilder::new(&"ok".parse()?, &"1.0.0".parse()?)
        .files(&[("ok-2.0.0.dist-info/variant.json", &variant)])
        .build();
    links
        .child("ok-1.0.0-py3-none-any-null.whl")
        .write_binary(&wheel)?;
    links.child("ok-1.0.0-variants.json").write_str(&variant)?;
    let server = FindLinksServer::new(links.path());
    context
        .temp_dir
        .child("requirements.in")
        .write_str("ok==1.0.0")?;

    uv_snapshot!(context.filters(), context.pip_compile()
        .arg("requirements.in")
        .arg("--no-index")
        .arg("--find-links")
        .arg(server.url())
        .arg("--preview-features")
        .arg("wheel-variants"), @"
    exit_code: 1 (failure)
    ----- stderr -----
    error: No solution found when resolving dependencies
      cause: Because ok==1.0.0 has an invalid package format and you require ok==1.0.0, we can conclude that your requirements are unsatisfiable.

    hint: The structure of `ok` (v1.0.0) was invalid:
      Multiple .dist-info directories found: ok-1.0.0, ok-2.0.0
    ");
    Ok(())
}
