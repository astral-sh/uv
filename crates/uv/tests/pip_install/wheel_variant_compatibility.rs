use std::collections::BTreeMap;
#[cfg(feature = "test-git")]
use std::process::Command;
use std::str::FromStr;

use anyhow::Result;
#[cfg(feature = "test-git")]
use assert_cmd::assert::OutputAssertExt;
use assert_fs::prelude::*;
use indoc::indoc;
use insta::{assert_json_snapshot, assert_snapshot};
use serde_json::{Map, Value, json};
use uv_distribution_filename::VariantLabel;
use uv_distribution_types::PEP_850_SCHEMA;
use uv_normalize::PackageName;
use uv_pep440::Version;
#[cfg(feature = "test-git")]
use uv_redacted::SafeUrl;
use uv_static::EnvVars;
use uv_test::find_links::FindLinksServer;
use uv_test::packse::{PackseServer, WheelBuilder};
use uv_test::uv_snapshot;

fn null_variant() -> Value {
    json!({
        "$schema": "https://variants-schema.wheelnext.dev/peps/825/v0.1.1.json",
        "default-priorities": { "namespace": ["x86_64"] },
        "variants": { "null": {} },
    })
}

fn gpu_variant() -> Value {
    json!({
        "$schema": PEP_850_SCHEMA,
        "default-priorities": { "namespace": ["nvidia"] },
        "providers": {
            "nvidia": {
                "requires": ["nvidia-variant-provider==0.0.1"],
                "plugin-api": "nvidia_variant_provider.plugin:NvidiaVariantPlugin"
            }
        },
        "variants": {
            "gpu": {
                "nvidia": {
                    "cuda_version_lower_bound": ["12"],
                    "sm_arch": ["89_real"]
                }
            }
        }
    })
}

/// Recheck a local wheel's hardware requirements when compiling after a driver change.
#[test]
fn install_local_variant_wheel() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let variant = gpu_variant().to_string();
    let (filename, wheel) = WheelBuilder::new(&"ok".parse()?, &"1.0.0".parse()?)
        .variant(&"gpu".parse()?, &variant)
        .build();
    let wheel_path = context.temp_dir.child(&filename);
    wheel_path.write_binary(&wheel)?;

    uv_snapshot!(context.filters(), context.pip_install()
        .env(EnvVars::NV_VARIANT_PROVIDER_FORCE_CUDA_DRIVER_VERSION, "12")
        .env(EnvVars::NV_VARIANT_PROVIDER_FORCE_SM_ARCH, "8.9")
        .arg("--preview-features")
        .arg("wheel-variants")
        .arg(wheel_path.path()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + ok==1.0.0 (from file://[TEMP_DIR]/ok-1.0.0-py3-none-any-gpu.whl)
    ");

    context
        .temp_dir
        .child("requirements.in")
        .write_str(&format!("./{filename}\n"))?;
    uv_snapshot!(context.filters(), context.pip_compile()
        .env(EnvVars::NV_VARIANT_PROVIDER_FORCE_CUDA_DRIVER_VERSION, "11")
        .env(EnvVars::NV_VARIANT_PROVIDER_FORCE_SM_ARCH, "8.8")
        .arg("--preview-features")
        .arg("wheel-variants")
        .arg("requirements.in"), @"
    exit_code: 1 (failure)
    ----- stderr -----
    error: No solution found when resolving dependencies
      cause: Because only ok==1.0.0 is available and ok==1.0.0 has only unsupported wheel variants, we can conclude that all versions of ok cannot be used.
             And because you require ok, we can conclude that your requirements are unsatisfiable.
    ");
    Ok(())
}

/// A refreshed local variant wheel must have compatible platform tags.
#[test]
fn refresh_incompatible_variant_wheel() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let variant = null_variant().to_string();
    let (filename, wheel) = WheelBuilder::new(&"ok".parse()?, &"1.0.0".parse()?)
        .tag("py3-none-win_amd64")
        .variant(&"null".parse()?, &variant)
        .build();
    let wheel_path = context.temp_dir.child(filename);
    wheel_path.write_binary(&wheel)?;

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--preview-features")
        .arg("wheel-variants")
        .arg("--python-platform")
        .arg("linux")
        .arg("--refresh")
        .arg(wheel_path.path()), @"
    exit_code: 1 (failure)
    ----- stderr -----
    error: No solution found when resolving dependencies
      cause: Because only ok==1.0.0 is available and ok==1.0.0 has no wheels with a matching platform tag (e.g., `manylinux_2_28_x86_64`), we can conclude that all versions of ok cannot be used.
             And because you require ok, we can conclude that your requirements are unsatisfiable.
    ");
    Ok(())
}

/// Resolve a direct URL wheel against the current driver, including when its metadata is cached.
#[test]
fn install_direct_url_variant_wheel() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let variant = gpu_variant().to_string();
    let (filename, wheel) = WheelBuilder::new(&"ok".parse()?, &"1.0.0".parse()?)
        .variant(&"gpu".parse()?, &variant)
        .build();
    context.temp_dir.child(&filename).write_binary(&wheel)?;
    let server = FindLinksServer::new(context.temp_dir.path());
    let url = format!("{}/{filename}", server.url());

    uv_snapshot!(context.filters(), context.pip_install()
        .env(EnvVars::NV_VARIANT_PROVIDER_FORCE_CUDA_DRIVER_VERSION, "11")
        .env(EnvVars::NV_VARIANT_PROVIDER_FORCE_SM_ARCH, "8.8")
        .arg("--dry-run")
        .arg("--preview-features")
        .arg("wheel-variants")
        .arg(&url), @"
    exit_code: 1 (failure)
    ----- stderr -----
    error: No solution found when resolving dependencies
      cause: Because only ok==1.0.0 is available and ok==1.0.0 has only unsupported wheel variants, we can conclude that all versions of ok cannot be used.
             And because you require ok, we can conclude that your requirements are unsatisfiable.
    ");

    // A dependency metadata override still needs the cached wheel's variant requirements.
    let config = context.temp_dir.child("override.toml");
    config.write_str(indoc! {r#"
        [[dependency-metadata]]
        name = "ok"
        version = "1.0.0"
    "#})?;
    uv_snapshot!(context.filters(), context.pip_install()
        .env(EnvVars::NV_VARIANT_PROVIDER_FORCE_CUDA_DRIVER_VERSION, "11")
        .env(EnvVars::NV_VARIANT_PROVIDER_FORCE_SM_ARCH, "8.8")
        .arg("--dry-run")
        .arg("--config-file")
        .arg(config.path())
        .arg("--offline")
        .arg("--preview-features")
        .arg("wheel-variants")
        .arg(&url), @"
    exit_code: 1 (failure)
    ----- stderr -----
    error: No solution found when resolving dependencies
      cause: Because only ok==1.0.0 is available and ok==1.0.0 has only unsupported wheel variants, we can conclude that all versions of ok cannot be used.
             And because you require ok, we can conclude that your requirements are unsatisfiable.
    ");

    uv_snapshot!(context.filters(), context.pip_install()
        .env(EnvVars::NV_VARIANT_PROVIDER_FORCE_CUDA_DRIVER_VERSION, "12")
        .env(EnvVars::NV_VARIANT_PROVIDER_FORCE_SM_ARCH, "8.9")
        .arg("--preview-features")
        .arg("wheel-variants")
        .arg(&url), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + ok==1.0.0 (from http://[LOCALHOST]/ok-1.0.0-py3-none-any-gpu.whl)
    ");
    Ok(())
}

/// A core metadata override still leaves the wheel's embedded variant metadata to validate.
#[test]
fn direct_url_wheel_with_invalid_variant_metadata() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let config = context.temp_dir.child("override.toml");
    config.write_str(indoc! {r#"
        [[dependency-metadata]]
        name = "ok"
        version = "1.0.0"
    "#})?;
    let (filename, wheel) = WheelBuilder::new(&"ok".parse()?, &"1.0.0".parse()?)
        .variant(&"null".parse()?, "{")
        .build();
    context.temp_dir.child(&filename).write_binary(&wheel)?;
    let server = FindLinksServer::new(context.temp_dir.path());
    let url = format!("{}/{filename}", server.url());

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--dry-run")
        .arg("--config-file")
        .arg(config.path())
        .arg("--preview-features")
        .arg("wheel-variants")
        .arg(format!("ok @ {url}")), @"
    exit_code: 1 (failure)
    ----- stderr -----
    error: No solution found when resolving dependencies
      cause: Because only ok==1.0.0 is available and ok==1.0.0 has invalid metadata, we can conclude that all versions of ok cannot be used.
             And because you require ok, we can conclude that your requirements are unsatisfiable.

    hint: Metadata for `ok` (v1.0.0) could not be parsed:
      Failed to parse variant metadata `variant.json`: EOF while parsing an object at line 1 column 1
    ");
    Ok(())
}

/// Dry runs reject an incompatible Git wheel whether or not the wheel is cached.
#[test]
#[cfg(feature = "test-git")]
fn install_git_path_variant_wheel() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_filter((r"@[0-9a-f]{40}", "@[COMMIT]"));
    let repository = context.temp_dir.child("repository");
    repository.create_dir_all()?;
    let variant = gpu_variant().to_string();
    let (filename, wheel) = WheelBuilder::new(&"ok".parse()?, &"1.0.0".parse()?)
        .variant(&"gpu".parse()?, &variant)
        .build();
    repository.child(&filename).write_binary(&wheel)?;
    Command::new("git")
        .arg("init")
        .arg(repository.path())
        .assert()
        .success();
    Command::new("git")
        .current_dir(repository.path())
        .args(["add", &filename])
        .assert()
        .success();
    Command::new("git")
        .current_dir(repository.path())
        .args([
            "-c",
            "user.name=ferris",
            "-c",
            "user.email=ferris@example.com",
            "commit",
            "-m",
            "Add wheel",
        ])
        .assert()
        .success();
    let output = Command::new("git")
        .current_dir(repository.path())
        .args(["rev-parse", "HEAD"])
        .output()?;
    assert!(output.status.success());
    let commit = String::from_utf8(output.stdout)?;
    let commit = commit.trim();
    let repository_url = SafeUrl::from_directory_path(repository.path()).unwrap();
    let requirement = format!("ok @ git+{repository_url}@{commit}#path={filename}");

    uv_snapshot!(context.filters(), context.pip_install()
        .env(EnvVars::NV_VARIANT_PROVIDER_FORCE_CUDA_DRIVER_VERSION, "11")
        .env(EnvVars::NV_VARIANT_PROVIDER_FORCE_SM_ARCH, "8.8")
        .arg("--preview-features")
        .arg("wheel-variants")
        .arg("--dry-run")
        .arg(&requirement), @"
    exit_code: 1 (failure)
    ----- stderr -----
    error: No solution found when resolving dependencies
      cause: Because only ok==1.0.0 is available and ok==1.0.0 has only unsupported wheel variants, we can conclude that all versions of ok cannot be used.
             And because you require ok, we can conclude that your requirements are unsatisfiable.
    ");

    uv_snapshot!(context.filters(), context.pip_install()
        .env(EnvVars::NV_VARIANT_PROVIDER_FORCE_CUDA_DRIVER_VERSION, "12")
        .env(EnvVars::NV_VARIANT_PROVIDER_FORCE_SM_ARCH, "8.9")
        .arg("--preview-features")
        .arg("wheel-variants")
        .arg(&requirement), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + ok==1.0.0 (from git+file://[TEMP_DIR]/repository/@[COMMIT]#path=ok-1.0.0-py3-none-any-gpu.whl)
    ");

    context.reset_venv();
    uv_snapshot!(context.filters(), context.pip_install()
        .env(EnvVars::NV_VARIANT_PROVIDER_FORCE_CUDA_DRIVER_VERSION, "11")
        .env(EnvVars::NV_VARIANT_PROVIDER_FORCE_SM_ARCH, "8.8")
        .arg("--preview-features")
        .arg("wheel-variants")
        .arg("--dry-run")
        .arg(&requirement), @"
    exit_code: 1 (failure)
    ----- stderr -----
    error: No solution found when resolving dependencies
      cause: Because only ok==1.0.0 is available and ok==1.0.0 has only unsupported wheel variants, we can conclude that all versions of ok cannot be used.
             And because you require ok, we can conclude that your requirements are unsatisfiable.
    ");
    Ok(())
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

/// Install the BLAS variant preferred by the static provider, even though its label sorts last
/// and the preferred library is listed second.
#[test]
fn select_variant_by_static_provider_order() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::from_toml(indoc! {r#"
        name = "static-provider-order"
        root = {}

        [expected]
        satisfiable = true

        [packages.ok.versions."1.0.0"]
        wheel = false
        sdist = false

        [packages.ok.versions."1.0.0".variants]
        namespaces = ["blas_lapack"]

        [packages.ok.versions."1.0.0".variants.providers.blas_lapack]
        static-properties.library = ["openblas", "mkl", "accelerate"]

        [packages.ok.versions."1.0.0".variants.wheels]
        accelerate.properties = ["blas_lapack :: library :: accelerate"]
        mkl.properties = ["blas_lapack :: library :: mkl"]
        openblas_or_accelerate.properties = [
            "blas_lapack :: library :: accelerate",
            "blas_lapack :: library :: openblas",
        ]
    "#})?;

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--index-url")
        .arg(server.index_url())
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
    let installed = context.read(
        context
            .site_packages()
            .join("ok-1.0.0.dist-info/variant.json"),
    );
    assert_eq!(
        serde_json::from_str::<Value>(&installed)?["variants"],
        json!({
            "openblas_or_accelerate": { "blas_lapack": { "library": ["accelerate", "openblas"] } }
        })
    );
    Ok(())
}

/// Select the TPU wheel when TPU presence is forced on the host.
#[test]
fn select_tpu_variant() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::from_toml(indoc! {r#"
        name = "tpu-provider"
        root = {}

        [expected]
        satisfiable = true

        [packages.ok.versions."1.0.0"]
        wheel = false
        sdist = false

        [packages.ok.versions."1.0.0".variants]
        namespaces = ["tpu"]

        [packages.ok.versions."1.0.0".variants.providers.tpu]
        requires = ["tpu-variant-provider==0.1.0"]
        plugin-api = "tpu_variant_provider.plugin:TpuVariantPlugin"

        [packages.ok.versions."1.0.0".variants.wheels]
        cpu.properties = ["tpu :: present :: false"]
        tpu.properties = ["tpu :: present :: true"]
    "#})?;

    uv_snapshot!(context.filters(), context.pip_install()
        .env(EnvVars::TPU_VARIANT_PROVIDER_FORCE_PRESENT, "YES")
        .arg("--index-url")
        .arg(server.index_url())
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
    let installed = context.read(
        context
            .site_packages()
            .join("ok-1.0.0.dist-info/variant.json"),
    );
    assert_json_snapshot!(serde_json::from_str::<Value>(&installed)?["variants"], @r#"
    {
      "tpu": {
        "tpu": {
          "present": [
            "true"
          ]
        }
      }
    }
    "#);
    Ok(())
}

/// An explicit AArch64 target installs the default wheel even when an Armv8.4-A wheel exists.
#[test]
fn aarch64_target_uses_platform_defaults() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::from_toml(indoc! {r#"
        name = "aarch64-target-defaults"
        root = {}

        [expected]
        satisfiable = true

        [packages.ok.versions."1.0.0"]
        wheel = false
        sdist = false
        wheel_tags = ["py3-none-linux_aarch64"]

        [packages.ok.versions."1.0.0".variants]
        namespaces = ["aarch64"]

        [packages.ok.versions."1.0.0".variants.providers]
        aarch64.requires = ["aarch64-variant-provider==0.2.0"]

        [packages.ok.versions."1.0.0".variants.wheels]
        arm8.properties = ["aarch64 :: version :: 8a"]
        arm84.properties = ["aarch64 :: version :: 8.4a"]
    "#})?;

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--index-url")
        .arg(server.index_url())
        .arg("--preview-features")
        .arg("wheel-variants")
        .arg("--python-platform")
        .arg("aarch64-unknown-linux-gnu")
        .arg("ok==1.0.0"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + ok==1.0.0
    ");
    let installed = context.read(
        context
            .site_packages()
            .join("ok-1.0.0.dist-info/variant.json"),
    );
    assert_json_snapshot!(serde_json::from_str::<Value>(&installed)?["variants"], @r#"
    {
      "arm8": {
        "aarch64": {
          "version": [
            "8a"
          ]
        }
      }
    }
    "#);
    Ok(())
}

const X86_64_VARIANTS: &str = indoc! {r#"
    name = "x86-64-target-defaults"
    root = {}

    [expected]
    satisfiable = true

    [packages.ok.versions."1.0.0"]
    wheel = false
    sdist = false

    [packages.ok.versions."1.0.0".variants]
    namespaces = ["x86_64"]

    [packages.ok.versions."1.0.0".variants.providers]
    x86_64.requires = ["x86_64-variant-provider==0.2.1"]

    [packages.ok.versions."1.0.0".variants.wheels]
    v1.properties = ["x86_64 :: level :: v1"]
    v2.properties = ["x86_64 :: level :: v2"]
    v3.properties = ["x86_64 :: level :: v3"]
    v4.properties = ["x86_64 :: level :: v4"]
"#};

/// An explicit x86-64 target uses the platform defaults even when higher-level wheels exist.
#[test]
fn x86_64_target_uses_platform_defaults() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::from_toml(X86_64_VARIANTS)?;

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--index-url")
        .arg(server.index_url())
        .arg("--preview-features")
        .arg("wheel-variants")
        .arg("--python-platform")
        .arg("x86_64-unknown-linux-gnu")
        .arg("ok==1.0.0"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + ok==1.0.0
    ");
    let installed = context.read(
        context
            .site_packages()
            .join("ok-1.0.0.dist-info/variant.json"),
    );
    assert_json_snapshot!(serde_json::from_str::<Value>(&installed)?["variants"], @r#"
    {
      "v2": {
        "x86_64": {
          "level": [
            "v2"
          ]
        }
      }
    }
    "#);
    Ok(())
}

/// On macOS and Windows, an x86-64 interpreter without `_ctypes` falls back to the v2 wheel.
#[cfg(any(target_os = "macos", windows))]
#[test]
fn x86_64_without_ctypes_installs_v2() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::from_toml(X86_64_VARIANTS)?;
    fs_err::write(
        context.site_packages().join("sitecustomize.py"),
        indoc! {r#"
            import platform
            import sys
            import sysconfig

            sys.modules["_ctypes"] = None
            # Exercise the x86-64 probe on arm64 runners too.
            if sys.platform == "darwin":
                release, versioninfo, _ = platform.mac_ver()
                platform.mac_ver = lambda: (release, versioninfo, "x86_64")
            else:
                sysconfig.get_platform = lambda: "win-amd64"
        "#},
    )?;

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--no-cache")
        .arg("--index-url")
        .arg(server.index_url())
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
    let installed = context.read(
        context
            .site_packages()
            .join("ok-1.0.0.dist-info/variant.json"),
    );
    assert_json_snapshot!(serde_json::from_str::<Value>(&installed)?["variants"], @r#"
    {
      "v2": {
        "x86_64": {
          "level": [
            "v2"
          ]
        }
      }
    }
    "#);
    Ok(())
}

const ABI_DEPENDENCY_WHEELS: &str = indoc! {r#"
    name = "abi-dependency"
    root = {}

    [expected]
    satisfiable = true

    [packages.extension.versions."1.0.0"]
    wheel = false
    sdist = false

    [packages.extension.versions."1.0.0".variants]
    namespaces = ["abi_dependency"]
    providers.abi_dependency.builtin = "abi_dependency"

    [packages.extension.versions."1.0.0".variants.wheels]
    torch28.properties = ["abi_dependency :: torch :: 2.8"]
    torch28.requires = ["torch==2.8.*"]
    torch29.properties = ["abi_dependency :: torch :: 2.9"]
    torch29.requires = ["torch==2.9.*"]

    [packages.torch.versions."2.8.1"]
    [packages.torch.versions."2.9.1"]
"#};

/// Installing `extension` alone selects the `torch28` wheel. Its `Requires-Dist` pins
/// Torch to `2.8.*`, so uv installs 2.8.1 even though 2.9.1 is available.
#[test]
fn install_selected_wheel_abi_dependency() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::from_toml(ABI_DEPENDENCY_WHEELS)?;

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--index-url")
        .arg(server.index_url())
        .arg("--preview-features")
        .arg("wheel-variants")
        .arg("extension"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + extension==1.0.0
     + torch==2.8.1
    ");
    Ok(())
}

/// An explicit Torch pin rules out the otherwise preferred wheel.
#[test]
fn select_wheel_by_abi_dependency_pin() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::from_toml(ABI_DEPENDENCY_WHEELS)?;

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--index-url")
        .arg(server.index_url())
        .arg("--preview-features")
        .arg("wheel-variants")
        .arg("extension")
        .arg("torch==2.9.1"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + extension==1.0.0
     + torch==2.9.1
    ");
    Ok(())
}

/// The first index's ABI wheels and the second index's null variant all conflict with
/// the requested Torch version. The ordinary wheel on the second index can satisfy it.
#[test]
fn fall_back_to_ordinary_wheel_on_next_index() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let primary = PackseServer::from_toml(ABI_DEPENDENCY_WHEELS)?;
    let fallback = PackseServer::from_toml(indoc! {r#"
        name = "ordinary-fallback"
        root = {}

        [expected]
        satisfiable = true

        [packages.extension.versions."1.0.0"]
        sdist = false
        requires = ["torch", "numpy"]

        [packages.extension.versions."1.0.0".variants]
        namespaces = []
        wheels.null.requires = ["torch==2.9.*"]

        [packages.torch.versions."2.10.1"]
        [packages.numpy.versions."1.0.0"]
    "#})?;

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--index")
        .arg(primary.index_url())
        .arg("--default-index")
        .arg(fallback.index_url())
        .arg("--index-strategy")
        .arg("unsafe-best-match")
        .arg("--preview-features")
        .arg("wheel-variants")
        .arg("extension")
        .arg("torch==2.10.1"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + extension==1.0.0
     + numpy==1.0.0
     + torch==2.10.1
    ");
    Ok(())
}

/// An invalid ABI requirement rules out that wheel while the ordinary wheel remains installable.
#[test]
fn invalid_abi_requirement_uses_ordinary_wheel() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::from_toml(indoc! {r#"
        name = "invalid-abi-requirement"
        root = {}

        [expected]
        satisfiable = true

        [packages.extension.versions."1.0.0"]
        sdist = false

        [packages.extension.versions."1.0.0".variants]
        namespaces = ["abi_dependency"]
        providers.abi_dependency.builtin = "abi_dependency"

        [packages.extension.versions."1.0.0".variants.wheels]
        broken.properties = ["abi_dependency :: _torch :: 2.9"]
    "#})?;

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--index-url")
        .arg(server.index_url())
        .arg("--preview-features")
        .arg("wheel-variants")
        .arg("extension"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + extension==1.0.0
    ");
    let variant = context
        .site_packages()
        .join("extension-1.0.0.dist-info/variant.json");
    assert!(!variant.try_exists()?);
    Ok(())
}

/// The preferred wheel's ABI is incompatible with the only available Torch version.
/// Both wheels declare the same ordinary dependency, so either can supply the metadata.
#[test]
fn retry_abi_wheel_for_resolved_runtime() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::from_toml(indoc! {r#"
        name = "abi-wheel-choice"
        root = {}

        [expected]
        satisfiable = true

        [packages.extension.versions."1.0.0"]
        wheel = false
        sdist = false
        requires = ["torch"]

        [packages.extension.versions."1.0.0".variants]
        namespaces = ["abi_dependency"]
        providers.abi_dependency.builtin = "abi_dependency"

        [packages.extension.versions."1.0.0".variants.wheels]
        torch28.properties = ["abi_dependency :: torch :: 2.8"]
        torch29.properties = ["abi_dependency :: torch :: 2.9"]

        [packages.torch.versions."2.9.1"]
    "#})?;

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--index-url")
        .arg(server.index_url())
        .arg("--preview-features")
        .arg("wheel-variants")
        .arg("extension"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + extension==1.0.0
     + torch==2.9.1
    ");
    Ok(())
}

/// Once resolution drops the release, an incompatible Torch must not require any wheel.
#[test]
fn discarded_abi_release_requires_no_wheel() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::from_toml(indoc! {r#"
        name = "discarded-abi-release"
        root = {}

        [expected]
        satisfiable = true

        [packages.parent.versions."2.0.0"]
        requires = ["extension"]
        [packages.parent.versions."1.0.0"]

        [packages.extension.versions."1.0.0"]
        wheel = false
        sdist = false
        requires = ["torch"]
        [packages.extension.versions."1.0.0".variants]
        namespaces = ["abi_dependency"]
        providers.abi_dependency.builtin = "abi_dependency"
        [packages.extension.versions."1.0.0".variants.wheels]
        torch29.properties = ["abi_dependency :: torch :: 2.9"]

        [packages.torch.versions."2.10.1"]
    "#})?;

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--index-url")
        .arg(server.index_url())
        .arg("--preview-features")
        .arg("wheel-variants")
        .arg("parent")
        .arg("torch"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + parent==1.0.0
     + torch==2.10.1
    ");
    Ok(())
}

/// The index and wheel name different missing runtimes. Resolution should succeed, and
/// preparing the wheel should report the runtime from its embedded ABI metadata.
#[test]
fn wheel_variant_index_cannot_add_dependencies() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let links = context.temp_dir.child("links");
    links.create_dir_all()?;
    let mut variant = json!({
        "$schema": PEP_850_SCHEMA,
        "default-priorities": { "namespace": ["abi_dependency"] },
        "providers": { "abi_dependency": { "builtin": "abi_dependency" } },
        "variants": { "example": { "abi_dependency": { "torch": ["2.9"] } } },
    });
    let (filename, wheel) = WheelBuilder::new(
        &PackageName::from_str("extension")?,
        &Version::new([1, 0, 0]),
    )
    .variant(&VariantLabel::from_static("example"), &variant.to_string())
    .build();
    links.child(filename).write_binary(&wheel)?;
    variant["variants"]["example"] = json!({ "abi_dependency": { "other": ["1"] } });
    links
        .child("extension-1.0.0-variants.json")
        .write_str(&variant.to_string())?;

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--no-index")
        .arg("--find-links")
        .arg(links.path())
        .arg("--preview-features")
        .arg("wheel-variants")
        .arg("extension"), @"
    exit_code: 1 (failure)
    ----- stderr -----
    Resolved 1 package in [TIME]
    error: Failed to download `extension==1.0.0`
      cause: Wheel variant `example` requires unsupported properties: `abi_dependency :: torch :: 2.9`
    ");
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
