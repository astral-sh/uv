use std::collections::BTreeMap;
#[cfg(feature = "test-git")]
use std::process::Command;

use anyhow::Result;
#[cfg(feature = "test-git")]
use anyhow::anyhow;
#[cfg(feature = "test-git")]
use assert_cmd::assert::OutputAssertExt;
use assert_fs::prelude::*;
use indoc::indoc;
use insta::assert_snapshot;
use serde_json::{Map, Value, json};
use uv_distribution_types::PEP_850_SCHEMA;
#[cfg(feature = "test-git")]
use uv_redacted::SafeUrl;
use uv_static::EnvVars;
use uv_test::archive::generate_source_archive;
use uv_test::find_links::FindLinksServer;
use uv_test::packse::{PackseServer, WheelBuilder, scenario::Scenario};
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

/// Install the provider separately from the wheel selected using its properties.
#[test]
fn install_authorized_variant_provider_from_trusted_index() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let scenario = toml::from_str::<Scenario>(indoc! {r#"
        name = "authorized-variant-provider"
        root = {}
        expected.satisfiable = true

        [packages.ok.versions."1.0.0"]
        wheel = false
        sdist = false

        [packages.ok.versions."1.0.0".variants]
        namespaces = ["example"]

        [packages.ok.versions."1.0.0".variants.providers.example]
        requires = ["variant-provider>=1.0.0"]
        plugin-api = "variant_provider.plugin:Provider"

        [packages.ok.versions."1.0.0".variants.wheels]
        accelerated.properties = ["example :: acceleration :: enabled"]
    "#})?;
    let server = PackseServer::from_scenario(&scenario);
    let provider = indoc! {r#"
        from types import SimpleNamespace

        class Provider:
            @classmethod
            def get_supported_configs(cls):
                return [SimpleNamespace(name="acceleration", values=["enabled"], multi_value=False)]
    "#};
    let (filename, wheel) = WheelBuilder::new(&"variant-provider".parse()?, &"1.0.0".parse()?)
        .files(&[("variant_provider/plugin.py", provider)])
        .build();
    let trusted_index = context.temp_dir.child("trusted-index");
    let trusted_package = trusted_index.child("variant-provider");
    trusted_package.create_dir_all()?;
    trusted_package.child(&filename).write_binary(&wheel)?;
    trusted_package
        .child("index.html")
        .write_str(&format!(r#"<a href="{filename}">{filename}</a>"#))?;

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--preview-features")
        .arg("wheel-variants")
        .arg("--variant-provider")
        .arg("example=variant-provider@variant_provider.plugin:Provider")
        .arg("--variant-provider-index")
        .arg(trusted_index.path())
        .arg("--index-url")
        .arg(server.index_url())
        .arg("ok==1.0.0"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + ok==1.0.0
    ");
    assert!(!context.site_packages().join("variant_provider").exists());
    Ok(())
}

/// Local wheels must remain compatible when installed or cached.
#[test]
fn install_local_variant_wheel() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let variant = gpu_variant().to_string();
    let (_, wheel) = WheelBuilder::new(&"ok".parse()?, &"1.0.0".parse()?)
        .files(&[("ok-1.0.0.dist-info/variant.json", &variant)])
        .build();
    let wheel_path = context.temp_dir.child("ok-1.0.0-py3-none-any-gpu.whl");
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
    Installed 1 package in [TIME]
     + ok==1.0.0 (from file://[TEMP_DIR]/ok-1.0.0-py3-none-any-gpu.whl)
    ");

    uv_snapshot!(context.filters(), context.pip_install()
        .env(EnvVars::NV_VARIANT_PROVIDER_FORCE_CUDA_DRIVER_VERSION, "11")
        .env(EnvVars::NV_VARIANT_PROVIDER_FORCE_SM_ARCH, "8.8")
        .arg("--preview-features")
        .arg("wheel-variants")
        .arg(wheel_path.path()), @"
    exit_code: 1 (failure)
    ----- stderr -----
    error: Failed to read `ok @ file://[TEMP_DIR]/ok-1.0.0-py3-none-any-gpu.whl`
      cause: Wheel variant `gpu` requires unsupported properties: `nvidia :: cuda_version_lower_bound :: 12` and `nvidia :: sm_arch :: 89_real`
    ");

    context.reset_venv();
    uv_snapshot!(context.filters(), context.pip_install()
        .env(EnvVars::NV_VARIANT_PROVIDER_FORCE_CUDA_DRIVER_VERSION, "11")
        .env(EnvVars::NV_VARIANT_PROVIDER_FORCE_SM_ARCH, "8.8")
        .arg("--preview-features")
        .arg("wheel-variants")
        .arg(wheel_path.path()), @"
    exit_code: 1 (failure)
    ----- stderr -----
    error: Failed to read `ok @ file://[TEMP_DIR]/ok-1.0.0-py3-none-any-gpu.whl`
      cause: Wheel variant `gpu` requires unsupported properties: `nvidia :: cuda_version_lower_bound :: 12` and `nvidia :: sm_arch :: 89_real`
    ");
    Ok(())
}

/// Refreshing a direct wheel must still check its platform tags before preparing it.
#[test]
fn refresh_incompatible_variant_wheel() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let variant = null_variant().to_string();
    let (_, wheel) = WheelBuilder::new(&"ok".parse()?, &"1.0.0".parse()?)
        .tag("py3-none-win_amd64")
        .files(&[("ok-1.0.0.dist-info/variant.json", &variant)])
        .build();
    let wheel_path = context
        .temp_dir
        .child("ok-1.0.0-py3-none-win_amd64-null.whl");
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

/// Installing a cached URL wheel checks the system's current supported variant properties.
#[test]
fn install_direct_url_variant_wheel() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let variant = gpu_variant().to_string();
    let (_, wheel) = WheelBuilder::new(&"ok".parse()?, &"1.0.0".parse()?)
        .files(&[("ok-1.0.0.dist-info/variant.json", &variant)])
        .build();
    let filename = "ok-1.0.0-py3-none-any-gpu.whl";
    context.temp_dir.child(filename).write_binary(&wheel)?;
    let server = FindLinksServer::new(context.temp_dir.path());
    let url = format!("{}/{filename}", server.url());
    let context = context.with_filter((server.url(), "http://[LOCALHOST]"));

    uv_snapshot!(context.filters(), context.pip_install()
        .env(EnvVars::NV_VARIANT_PROVIDER_FORCE_CUDA_DRIVER_VERSION, "12")
        .env(EnvVars::NV_VARIANT_PROVIDER_FORCE_SM_ARCH, "8.9")
        .arg("--preview-features")
        .arg("wheel-variants")
        .arg(&url), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Installed 1 package in [TIME]
     + ok==1.0.0 (from http://[LOCALHOST]/ok-1.0.0-py3-none-any-gpu.whl)
    ");

    context.reset_venv();
    uv_snapshot!(context.filters(), context.pip_install()
        .env(EnvVars::NV_VARIANT_PROVIDER_FORCE_CUDA_DRIVER_VERSION, "11")
        .env(EnvVars::NV_VARIANT_PROVIDER_FORCE_SM_ARCH, "8.8")
        .arg("--preview-features")
        .arg("wheel-variants")
        .arg(&url), @"
    exit_code: 1 (failure)
    ----- stderr -----
    error: Failed to download `ok @ http://[LOCALHOST]/ok-1.0.0-py3-none-any-gpu.whl`
      cause: Wheel variant `gpu` requires unsupported properties: `nvidia :: cuda_version_lower_bound :: 12` and `nvidia :: sm_arch :: 89_real`
    ");
    Ok(())
}

/// Installing a cached Git wheel checks variant compatibility even at the same pinned commit.
#[test]
#[cfg(feature = "test-git")]
fn install_git_path_variant_wheel() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_filter((r"@[0-9a-f]{40}", "@[COMMIT]"));
    let repository = context.temp_dir.child("repository");
    repository.create_dir_all()?;
    let variant = gpu_variant().to_string();
    let (_, wheel) = WheelBuilder::new(&"ok".parse()?, &"1.0.0".parse()?)
        .files(&[("ok-1.0.0.dist-info/variant.json", &variant)])
        .build();
    let filename = "ok-1.0.0-py3-none-any-gpu.whl";
    repository.child(filename).write_binary(&wheel)?;
    Command::new("git")
        .arg("init")
        .arg(repository.path())
        .assert()
        .success();
    Command::new("git")
        .current_dir(repository.path())
        .args(["add", filename])
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
    let repository_url = SafeUrl::from_directory_path(repository.path())
        .map_err(|()| anyhow!("failed to convert repository path to file URL"))?;
    let requirement = format!("ok @ git+{repository_url}@{commit}#path={filename}");

    uv_snapshot!(context.filters(), context.pip_install()
        .env(EnvVars::NV_VARIANT_PROVIDER_FORCE_CUDA_DRIVER_VERSION, "12")
        .env(EnvVars::NV_VARIANT_PROVIDER_FORCE_SM_ARCH, "8.9")
        .arg("--preview-features")
        .arg("wheel-variants")
        .arg(&requirement), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Installed 1 package in [TIME]
     + ok==1.0.0 (from git+file://[TEMP_DIR]/repository/@[COMMIT]#path=ok-1.0.0-py3-none-any-gpu.whl)
    ");

    context.reset_venv();
    uv_snapshot!(context.filters(), context.pip_install()
        .env(EnvVars::NV_VARIANT_PROVIDER_FORCE_CUDA_DRIVER_VERSION, "11")
        .env(EnvVars::NV_VARIANT_PROVIDER_FORCE_SM_ARCH, "8.8")
        .arg("--preview-features")
        .arg("wheel-variants")
        .arg(&requirement), @"
    exit_code: 1 (failure)
    ----- stderr -----
    error: Failed to download `ok @ git+file://[TEMP_DIR]/repository/@[COMMIT]#path=ok-1.0.0-py3-none-any-gpu.whl`
      cause: Wheel variant `gpu` requires unsupported properties: `nvidia :: cuda_version_lower_bound :: 12` and `nvidia :: sm_arch :: 89_real`
    ");
    Ok(())
}

#[test]
fn local_variant_wheel_requires_variant_metadata() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let (_, wheel) = WheelBuilder::new(&"ok".parse()?, &"1.0.0".parse()?).build();
    let wheel_path = context.temp_dir.child("ok-1.0.0-py3-none-any-null.whl");
    wheel_path.write_binary(&wheel)?;

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--preview-features")
        .arg("wheel-variants")
        .arg(wheel_path.path()), @"
    exit_code: 1 (failure)
    ----- stderr -----
    error: Failed to read `ok @ file://[TEMP_DIR]/ok-1.0.0-py3-none-any-null.whl`
      cause: Failed to read metadata: [TEMP_DIR]/ok-1.0.0-py3-none-any-null.whl
      cause: Missing `variant.json` in the wheel's .dist-info directory
    ");
    Ok(())
}

/// Install and sync replace an installed wheel at the same version as system properties change.
#[test]
fn replace_incompatible_installed_variant() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let scenario = toml::from_str::<Scenario>(indoc! {r#"
        name = "installed-variant-compatibility"
        root = {}
        expected.satisfiable = true

        [packages.ok.versions."1.0.0"]
        sdist = false

        [packages.ok.versions."1.0.0".variants]
        namespaces = ["nvidia"]

        [packages.ok.versions."1.0.0".variants.providers.nvidia]
        requires = ["nvidia-variant-provider==0.0.1"]
        plugin-api = "nvidia_variant_provider.plugin:NvidiaVariantPlugin"

        [packages.ok.versions."1.0.0".variants.wheels]
        gpu12.properties = ["nvidia :: cuda_version_lower_bound :: 12"]
        gpu11.properties = ["nvidia :: cuda_version_lower_bound :: 11"]
    "#})?;
    let server = PackseServer::from_scenario(&scenario);
    let installed_variant = context
        .site_packages()
        .join("ok-1.0.0.dist-info/variant.json");
    context
        .temp_dir
        .child("requirements.txt")
        .write_str("ok==1.0.0")?;

    uv_snapshot!(context.filters(), context.pip_install()
        .env(EnvVars::NV_VARIANT_PROVIDER_FORCE_CUDA_DRIVER_VERSION, "12")
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

    assert_eq!(
        serde_json::from_str::<Value>(&context.read(&installed_variant))?["variants"],
        json!({"gpu12": {"nvidia": {"cuda_version_lower_bound": ["12"]}}})
    );

    uv_snapshot!(context.filters(), context.pip_install()
        .env(EnvVars::NV_VARIANT_PROVIDER_FORCE_CUDA_DRIVER_VERSION, "11")
        .arg("--index-url")
        .arg(server.index_url())
        .arg("--preview-features")
        .arg("wheel-variants")
        .arg("ok==1.0.0"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     ~ ok==1.0.0
    ");

    assert_eq!(
        serde_json::from_str::<Value>(&context.read(&installed_variant))?["variants"],
        json!({"gpu11": {"nvidia": {"cuda_version_lower_bound": ["11"]}}})
    );

    uv_snapshot!(context.filters(), context.pip_sync()
        .env(EnvVars::NV_VARIANT_PROVIDER_FORCE_CUDA_DRIVER_VERSION, "10")
        .arg("requirements.txt")
        .arg("--index-url")
        .arg(server.index_url())
        .arg("--preview-features")
        .arg("wheel-variants"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     ~ ok==1.0.0
    ");
    assert!(!installed_variant.exists());

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--offline")
        .arg("--preview-features")
        .arg("wheel-variants")
        .arg("ok==1.0.0"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Checked 1 package in [TIME]
    ");
    Ok(())
}

/// Dependency markers use only the selected wheel properties supported by the target.
#[test]
fn selected_variant_properties_filter_dependencies() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let scenario = toml::from_str::<Scenario>(indoc! {r#"
        name = "selected-variant-properties"
        root = {}
        expected.satisfiable = true

        [packages.ok.versions."1.0.0"]
        wheel = false
        sdist = false
        requires = [
            "chosen ; 'blas_lapack :: library :: openblas' in variant_properties",
            "unsupported ; 'blas_lapack :: library :: mkl' in variant_properties",
        ]

        [packages.ok.versions."1.0.0".variants]
        namespaces = ["blas_lapack"]

        [packages.ok.versions."1.0.0".variants.providers.blas_lapack]
        static-properties.library = ["openblas"]

        [packages.ok.versions."1.0.0".variants.wheels]
        blas.properties = [
            "blas_lapack :: library :: openblas",
            "blas_lapack :: library :: mkl",
        ]

        [packages.chosen.versions."1.0.0"]
    "#})?;
    let server = PackseServer::from_scenario(&scenario);

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--index-url")
        .arg(server.index_url())
        .arg("--preview-features")
        .arg("wheel-variants")
        .arg("ok==1.0.0"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + chosen==1.0.0
     + ok==1.0.0
    ");

    let recorded = context.read(
        context
            .site_packages()
            .join("ok-1.0.0.dist-info/uv_variant.json"),
    );
    assert_eq!(
        serde_json::from_str::<Value>(&recorded)?,
        json!({"label": "blas", "properties": {"blas_lapack": {"library": ["openblas"]}}}),
    );
    uv_snapshot!(context.pip_check(), @"
    exit_code: 0 (success)
    ----- stderr -----
    Checked 2 packages in [TIME]
    All installed packages are compatible
    ");
    Ok(())
}

/// The only wheel requires a property supplied by the target, absent from the host defaults.
#[test]
fn explicit_variant_target_selects_registry_wheel() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let scenario = toml::from_str::<Scenario>(indoc! {r#"
        name = "explicit-variant-target-selects-registry-wheel"
        root = {}
        expected.satisfiable = true

        [packages.ok.versions."1.0.0"]
        wheel = false
        sdist = false

        [packages.ok.versions."1.0.0".variants]
        namespaces = ["blas_lapack"]

        [packages.ok.versions."1.0.0".variants.providers.blas_lapack]
        static-properties.library = ["openblas"]

        [packages.ok.versions."1.0.0".variants.wheels]
        mkl.properties = ["blas_lapack :: library :: mkl"]
    "#})?;
    let server = PackseServer::from_scenario(&scenario);
    let target = context.temp_dir.child("target.json");
    target.write_str(r#"{"blas_lapack": [["library", ["mkl"]]]}"#)?;

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--index-url")
        .arg(server.index_url())
        .arg("--preview-features")
        .arg("wheel-variants")
        .arg("--variant-target")
        .arg(target.path())
        .arg("ok==1.0.0"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + ok==1.0.0
    ");
    let recorded = context.read(
        context
            .site_packages()
            .join("ok-1.0.0.dist-info/uv_variant.json"),
    );
    assert_eq!(
        serde_json::from_str::<Value>(&recorded)?,
        json!({"label": "mkl", "properties": {"blas_lapack": {"library": ["mkl"]}}}),
    );

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--offline")
        .arg("--preview-features")
        .arg("wheel-variants")
        .arg("--variant-target")
        .arg(target.path())
        .arg("ok==1.0.0"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Checked 1 package in [TIME]
    ");
    Ok(())
}

/// An explicit empty target replaces an installed variant when another package forces resolution.
#[test]
fn explicit_variant_target_replaces_installed_wheel_during_resolution() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let scenario = toml::from_str::<Scenario>(indoc! {r#"
        name = "explicit-variant-target-replaces-installed-wheel"
        root = {}
        expected.satisfiable = true

        [packages.ok.versions."1.0.0"]
        sdist = false

        [packages.ok.versions."1.0.0".variants]
        namespaces = ["blas_lapack"]

        [packages.ok.versions."1.0.0".variants.providers.blas_lapack]
        static-properties.library = ["openblas"]

        [packages.ok.versions."1.0.0".variants.wheels]
        blas.properties = ["blas_lapack :: library :: openblas"]

        [packages.missing.versions."1.0.0"]
    "#})?;
    let server = PackseServer::from_scenario(&scenario);
    let installed_variant = context
        .site_packages()
        .join("ok-1.0.0.dist-info/variant.json");
    let target = context.temp_dir.child("empty-target.json");
    target.write_str("{}")?;

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
    assert!(installed_variant.is_file());

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--index-url")
        .arg(server.index_url())
        .arg("--preview-features")
        .arg("wheel-variants")
        .arg("--variant-target")
        .arg(target.path())
        .arg("ok==1.0.0")
        .arg("missing==1.0.0"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 2 packages in [TIME]
     + missing==1.0.0
     ~ ok==1.0.0
    ");
    assert!(!installed_variant.exists());
    Ok(())
}

/// Project sync installs the wheel selected by the lockfile even if another compatible variant
/// is already installed.
#[test]
fn sync_replaces_compatible_variant_with_selected_wheel() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let scenario = toml::from_str::<Scenario>(indoc! {r#"
        name = "installed-variant-lockfile-selection"
        root = {}
        expected.satisfiable = true

        [packages.ok.versions."1.0.0"]
        sdist = false
        variants.namespaces = ["x86_64"]
        variants.wheels.null.properties = []
    "#})?;
    let server = PackseServer::from_scenario(&scenario);
    let installed_variant = context
        .site_packages()
        .join("ok-1.0.0.dist-info/variant.json");
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
            [project]
            name = "example"
            version = "1.0.0"
            requires-python = ">=3.12"
            dependencies = ["ok==1.0.0"]
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
    assert!(installed_variant.is_file());

    uv_snapshot!(context.filters(), context.sync()
        .arg("--index-url")
        .arg(server.index_url())
        .arg("--preview-features")
        .arg("wheel-variants"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     ~ ok==1.0.0
    ");
    assert!(!installed_variant.exists());
    Ok(())
}

/// A lock that selects an sdist must replace a compatible variant of the same version.
#[test]
fn sync_replaces_compatible_variant_with_selected_sdist() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let links = context.temp_dir.child("links");
    links.create_dir_all()?;
    let name = "ok".parse()?;
    let version = "1.0.0".parse()?;
    let variant = null_variant().to_string();
    let (_, wheel) = WheelBuilder::new(&name, &version)
        .files(&[("ok-1.0.0.dist-info/variant.json", &variant)])
        .build();
    links
        .child("ok-1.0.0-py3-none-any-null.whl")
        .write_binary(&wheel)?;
    links.child("ok-1.0.0-variants.json").write_str(&variant)?;
    links
        .child("ok-1.0.0.tar.gz")
        .write_binary(&generate_source_archive(&name, &version, "", None)?)?;

    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
            [project]
            name = "example"
            version = "1.0.0"
            requires-python = ">=3.12"
            dependencies = ["ok==1.0.0"]

            [tool.uv]
            no-index = true
            find-links = ["links"]
            preview-features = ["wheel-variants"]
        "#})?;

    let installed_variant = context
        .site_packages()
        .join("ok-1.0.0.dist-info/variant.json");

    uv_snapshot!(context.filters(), context.pip_install().arg("ok==1.0.0"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + ok==1.0.0
    ");
    assert!(installed_variant.is_file());

    uv_snapshot!(context.filters(), context.sync(), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     ~ ok==1.0.0
    ");
    assert!(!installed_variant.exists());
    Ok(())
}

/// Disabling wheel variants fails if the installed variant has no compatible replacement.
#[test]
fn incompatible_installed_variant_without_replacement() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let scenario = toml::from_str::<Scenario>(indoc! {r#"
        name = "variant-only"
        root = {}
        expected.satisfiable = true

        [packages.ok.versions."1.0.0"]
        wheel = false
        sdist = false
        variants.namespaces = ["x86_64"]
        variants.wheels.null.properties = []
    "#})?;
    let server = PackseServer::from_scenario(&scenario);

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

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--no-index")
        .arg("ok==1.0.0"), @"
    exit_code: 1 (failure)
    ----- stderr -----
    error: No solution found when resolving dependencies
      cause: Because ok was not found in the provided package locations and ok==1.0.0 has only unsupported wheel variants, we can conclude that ok==1.0.0 cannot be used.
             And because you require ok==1.0.0, we can conclude that your requirements are unsatisfiable.

    hint: Packages were unavailable because index lookups were disabled and no additional package locations were provided (try: `--find-links <uri>`)
    ");
    assert!(
        context
            .site_packages()
            .join("ok-1.0.0.dist-info/variant.json")
            .is_file()
    );
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
    let scenario = toml::from_str::<Scenario>(indoc! {r#"
        name = "static-provider-order"
        root = {}
        expected.satisfiable = true

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
    let server = PackseServer::from_scenario(&scenario);

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
        json!({"openblas_or_accelerate": {"blas_lapack": {"library": ["accelerate", "openblas"]}}})
    );
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
