use std::collections::BTreeMap;

use anyhow::Result;
use assert_fs::prelude::*;
use insta::assert_snapshot;
use serde_json::{Map, Value, json};
use uv_distribution_types::PEP_850_SCHEMA;
use uv_static::EnvVars;
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

/// Install the BLAS variant preferred by the static provider, even though its label sorts last.
#[test]
fn select_variant_by_static_provider_order() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let links = context.temp_dir.child("links");
    links.create_dir_all()?;

    let mut metadata = json!({
        "$schema": PEP_850_SCHEMA,
        "default-priorities": { "namespace": ["blas_lapack"] },
        "providers": {
            "blas_lapack": {
                "static-properties": { "library": ["openblas", "mkl", "accelerate"] }
            }
        },
    });
    let mut variants = Map::new();
    for library in ["accelerate", "mkl", "openblas"] {
        let properties = json!({ "blas_lapack": { "library": [library] } });
        metadata["variants"] = json!({ library: &properties });
        variants.insert(library.to_string(), properties);

        let (_, wheel) = WheelBuilder::new(&"ok".parse()?, &"1.0.0".parse()?)
            .files(&[
                ("ok/variant.txt", library),
                ("ok-1.0.0.dist-info/variant.json", &metadata.to_string()),
            ])
            .build();
        links
            .child(format!("ok-1.0.0-py3-none-any-{library}.whl"))
            .write_binary(&wheel)?;
    }
    metadata["variants"] = variants.into();
    links
        .child("ok-1.0.0-variants.json")
        .write_str(&metadata.to_string())?;

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
    assert_snapshot!(context.read(context.site_packages().join("ok/variant.txt")), @"openblas");
    Ok(())
}

/// Fall back to an older release and reject its otherwise preferred incompatible NVIDIA variant.
#[test]
fn backtrack_to_compatible_nvidia_variant() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let links = context.temp_dir.child("links");
    links.create_dir_all()?;
    let mut metadata = json!({
        "$schema": PEP_850_SCHEMA,
        "default-priorities": { "namespace": ["nvidia"] },
        "providers": {
            "nvidia": {
                "requires": ["nvidia-variant-provider>=0.0.1,<1.0.0"],
                "plugin-api": "nvidia_variant_provider.plugin:NvidiaVariantPlugin"
            }
        }
    });
    let mut variants_by_version = BTreeMap::new();
    for (version, label, lower_bound, upper_bound, sm_arch) in [
        // The major-only CUDA bound `12` is supported but ranks below `12.0`.
        ("1.0.0", "gpu88", "12", "12.1", "88_real"),
        ("1.0.0", "gpu90", "12.0", "12.2", "90_real"),
        ("2.0.0", "gpu90", "12.0", "12.2", "90_real"),
    ] {
        let properties = json!({
            "nvidia": {
                "cuda_version_lower_bound": [lower_bound],
                "cuda_version_upper_bound": [upper_bound],
                "sm_arch": [sm_arch]
            }
        });
        metadata["variants"] = json!({ label: &properties });
        variants_by_version
            .entry(version)
            .or_insert_with(Map::new)
            .insert(label.to_string(), properties);
        let (_, wheel) = WheelBuilder::new(&"ok".parse()?, &version.parse()?)
            .files(&[
                ("ok/variant.txt", label),
                (
                    &format!("ok-{version}.dist-info/variant.json"),
                    &metadata.to_string(),
                ),
            ])
            .build();
        links
            .child(format!("ok-{version}-py3-none-any-{label}.whl"))
            .write_binary(&wheel)?;
    }
    for (version, variants) in variants_by_version {
        metadata["variants"] = variants.into();
        links
            .child(format!("ok-{version}-variants.json"))
            .write_str(&metadata.to_string())?;
    }

    uv_snapshot!(context.filters(), context.pip_install()
        .env(EnvVars::NV_VARIANT_PROVIDER_FORCE_CUDA_DRIVER_VERSION, "12")
        .env(EnvVars::NV_VARIANT_PROVIDER_FORCE_SM_ARCH, "8.9")
        .arg("--no-index")
        .arg("--find-links")
        .arg(links.path())
        .arg("--preview-features")
        .arg("wheel-variants")
        .arg("ok"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + ok==1.0.0
    ");
    assert_snapshot!(context.read(context.site_packages().join("ok/variant.txt")), @"gpu88");
    Ok(())
}

/// Skip the NVIDIA provider when a forced CUDA version would require too many variant values.
///
/// This is inherited from the Python implementation of the variant provider.
#[test]
fn skip_excessive_nvidia_cuda_override() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let links = context.temp_dir.child("links");
    links.create_dir_all()?;
    fs_err::copy(
        context
            .workspace_root
            .join("test/links/ok-1.0.0-py3-none-any.whl"),
        links.child("ok-1.0.0-py3-none-any.whl"),
    )?;
    let metadata = json!({
        "$schema": PEP_850_SCHEMA,
        "default-priorities": { "namespace": ["nvidia"] },
        "providers": {
            "nvidia": {
                "requires": ["nvidia-variant-provider==0.0.1"],
                "plugin-api": "nvidia_variant_provider.plugin:NvidiaVariantPlugin"
            }
        },
        "variants": {
            "gpu": { "nvidia": { "cuda_version_lower_bound": ["12.2000"] } }
        }
    })
    .to_string();
    let (_, wheel) = WheelBuilder::new(&"ok".parse()?, &"2.0.0".parse()?)
        .files(&[("ok-2.0.0.dist-info/variant.json", &metadata)])
        .build();
    links
        .child("ok-2.0.0-py3-none-any-gpu.whl")
        .write_binary(&wheel)?;
    links.child("ok-2.0.0-variants.json").write_str(&metadata)?;

    uv_snapshot!(context.filters(), context.pip_install()
        .env(EnvVars::NV_VARIANT_PROVIDER_FORCE_CUDA_DRIVER_VERSION, "12.2000")
        .env(EnvVars::NV_VARIANT_PROVIDER_FORCE_SM_ARCH, "8.9")
        .arg("--no-index")
        .arg("--find-links")
        .arg(links.path())
        .arg("--preview-features")
        .arg("wheel-variants")
        .arg("ok"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + ok==1.0.0
    ");
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
