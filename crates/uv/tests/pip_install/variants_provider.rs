//! Variant provider subprocess behavior.

use std::collections::BTreeMap;
use std::env;
use std::time::Duration;

use anyhow::Result;
use assert_fs::prelude::*;
use indoc::indoc;
use insta::allow_duplicates;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use url::Url;

use uv_test::packse::generate_wheel_with_files;
use uv_test::{TestContext, uv_snapshot};
use uv_variants::variants_json::VARIANT_SCHEMA;

use super::variants::write_wheel;

fn write_test_packages(context: &TestContext, multiple_providers: bool) -> Result<()> {
    let provider_code = indoc! {r#"
        import json
        import os
        import time
        from pathlib import Path
        from types import SimpleNamespace

        def get_supported_configs():
            output = os.environ.get("TEST_PROVIDER_OUTPUT")
            if output:
                names = ["PYX_API_KEY", "UV_API_KEY", "PYX_AUTH_TOKEN", "UV_AUTH_TOKEN"]
                data = {
                    "credentials": [name for name in names if name in os.environ],
                    "path_contains_test_dir": os.environ["TEST_PROVIDER_PATH"] in os.get_exec_path(),
                    "ordinary_variable": os.environ.get("TEST_PROVIDER_ORDINARY"),
                }
                Path(output).write_text(json.dumps(data))

            lock = os.environ.get("TEST_PROVIDER_LOCK")
            if lock:
                try:
                    fd = os.open(lock, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
                except FileExistsError:
                    raise RuntimeError("providers ran concurrently") from None
                try:
                    os.close(fd)
                    time.sleep(0.5)
                finally:
                    Path(lock).unlink()

            return [SimpleNamespace(name="level", values=["v3"])]
    "#};
    let (filename, wheel) = generate_wheel_with_files(
        &"env-provider".parse()?,
        &"1.0.0".parse()?,
        &[],
        &BTreeMap::new(),
        None,
        "py3-none-any",
        &[
            ("env_provider/runner.py", provider_code),
            (
                "env_provider/cpu.py",
                "from .runner import get_supported_configs\nnamespace = 'cpu'\n",
            ),
            (
                "env_provider/gpu.py",
                "from .runner import get_supported_configs\nnamespace = 'gpu'\n",
            ),
        ],
    );
    context.temp_dir.child(filename).write_binary(&wheel)?;

    let mut metadata = json!({
        "$schema": VARIANT_SCHEMA,
        "default-priorities": {"namespace": ["cpu"]},
        "providers": {
            "cpu": {"requires": ["env-provider==1.0.0"], "plugin-api": "env_provider.cpu"}
        },
        "variants": {"fast": {"cpu": {"level": ["v3"]}}},
    });
    if multiple_providers {
        metadata["default-priorities"]["namespace"] = json!(["cpu", "gpu"]);
        metadata["providers"]["gpu"] = json!({
            "requires": ["env-provider==1.0.0"], "plugin-api": "env_provider.gpu"
        });
        metadata["variants"]["fast"]["gpu"] = json!({"level": ["v3"]});
    }
    let metadata = serde_json::to_string(&metadata)?;
    context
        .temp_dir
        .child("example-1.0.0-variants.json")
        .write_str(&metadata)?;
    let (_, wheel) = generate_wheel_with_files(
        &"example".parse()?,
        &"1.0.0".parse()?,
        &[],
        &BTreeMap::new(),
        None,
        "py3-none-any",
        &[("example-1.0.0.dist-info/variant.json", &metadata)],
    );
    context
        .temp_dir
        .child("example-1.0.0-py3-none-any-fast.whl")
        .write_binary(&wheel)?;
    Ok(())
}

#[test]
fn variants_provider_environment() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    write_test_packages(&context, false)?;
    let test_path = context.temp_dir.child("test-path");
    test_path.create_dir_all()?;
    let system_path = env::var_os("PATH").unwrap_or_default();
    let path = env::join_paths(
        std::iter::once(test_path.path().to_path_buf()).chain(env::split_paths(&system_path)),
    )?;
    // The second run imports the provider from the target environment without isolation.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("env-provider").arg("--no-index").arg("--find-links").arg(context.temp_dir.path()), @r###"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + env-provider==1.0.0
    "###);

    for no_isolation in [false, true] {
        let output = context
            .temp_dir
            .child(format!("provider-output-{no_isolation}.json"));
        let mut command = context.pip_install();
        command
            .arg("example")
            .arg("--preview-features")
            .arg("wheel-variants")
            .arg("--no-index")
            .arg("--find-links")
            .arg(context.temp_dir.path())
            .arg("--reinstall")
            .env("TEST_PROVIDER_OUTPUT", output.path())
            .env("TEST_PROVIDER_PATH", test_path.path())
            .env("TEST_PROVIDER_ORDINARY", "kept")
            .env("PATH", &path)
            .env("PYX_API_KEY", "secret")
            .env("UV_API_KEY", "secret")
            .env("PYX_AUTH_TOKEN", "secret")
            .env("UV_AUTH_TOKEN", "secret");
        if no_isolation {
            command.env("UV_NO_PROVIDER_ISOLATION", "env_provider.cpu");
        }
        if no_isolation {
            uv_snapshot!(context.filters(), command, @r###"
            exit_code: 0 (success)
            ----- stderr -----
            Resolved 1 package in [TIME]
            Prepared 1 package in [TIME]
            Uninstalled 1 package in [TIME]
            Installed 1 package in [TIME]
             ~ example==1.0.0
            "###);
        } else {
            uv_snapshot!(context.filters(), command, @r###"
            exit_code: 0 (success)
            ----- stderr -----
            Resolved 1 package in [TIME]
            Prepared 1 package in [TIME]
            Installed 1 package in [TIME]
             + example==1.0.0
            "###);
        }
        let captured: Value = serde_json::from_slice(&fs_err::read(output.path())?)?;
        assert_eq!(
            captured,
            json!({
                "credentials": [],
                "path_contains_test_dir": true,
                "ordinary_variable": "kept",
            })
        );
    }
    Ok(())
}

/// An index's variant metadata is not authenticated by a wheel hash.
#[test]
fn variants_provider_wheel_hashes() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    write_test_packages(&context, false)?;
    let wheel = context
        .temp_dir
        .child("example-1.0.0-py3-none-any-fast.whl");
    let hash = hex::encode(Sha256::digest(fs_err::read(wheel.path())?));
    let requirements = context.temp_dir.child("requirements.txt");
    requirements.write_str(&format!("example==1.0.0 --hash=sha256:{hash}\n"))?;
    let invalid_requirements = context.temp_dir.child("invalid-requirements.txt");
    invalid_requirements.write_str(&format!(
        "example==1.0.0 --hash=sha256:{}\n",
        "0".repeat(64)
    ))?;
    let target = context.temp_dir.child("target.toml");
    target.write_str(indoc! {r#"
        provider = []
        [metadata]
        version = "0.1"
        created-by = "uv-test"
    "#})?;
    let output = context.temp_dir.child("provider-output.json");

    // A mismatched hash and an accepted hash both leave the sidecar unauthenticated. An
    // incomplete target file must not allow the provider to run in either case.
    allow_duplicates! {
      for (requirements, require_hashes) in [(&invalid_requirements, true), (&requirements, false)] {
        let mut command = context.pip_install();
        command
            .arg("-r")
            .arg(requirements.path())
            .arg("--preview-features")
            .arg("wheel-variants")
            .arg("--no-index")
            .arg("--find-links")
            .arg(context.temp_dir.path())
            .env("UV_VARIANT_LOCK", target.path())
            .env("UV_VARIANT_LOCK_INCOMPLETE", "1")
            .env("TEST_PROVIDER_OUTPUT", output.path())
            .env("TEST_PROVIDER_PATH", context.temp_dir.path());
        if require_hashes {
            command.arg("--require-hashes");
        }
        uv_snapshot!(context.filters(), command, @r###"
        exit_code: 1 (failure)
        ----- stderr -----
        error: Cannot run variant provider `cpu` from an index when verifying wheel hashes; provide its properties with `UV_VARIANT_LOCK`
        "###);
        assert!(!output.path().exists());
      }
    }

    target.write_str(indoc! {r#"
        [metadata]
        version = "0.1"
        created-by = "uv-test"
        [[provider]]
        namespace = "cpu"
        resolved = ["env-provider==1.0.0"]
        plugin-api = "env_provider.cpu"
        [provider.properties]
        level = ["v3"]
    "#})?;
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-r").arg(requirements.path()).arg("--require-hashes")
        .arg("--preview-features").arg("wheel-variants")
        .arg("--no-index").arg("--find-links").arg(context.temp_dir.path())
        .env("UV_VARIANT_LOCK", target.path())
        .env("TEST_PROVIDER_OUTPUT", output.path())
        .env("TEST_PROVIDER_PATH", context.temp_dir.path()), @r###"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + example==1.0.0
    "###);
    assert!(!output.path().exists());
    Ok(())
}

#[test]
fn variants_provider_build_concurrency() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    write_test_packages(&context, true)?;
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("example").arg("--preview-features").arg("wheel-variants")
        .arg("--no-index").arg("--find-links").arg(context.temp_dir.path())
        .env("UV_CONCURRENT_BUILDS", "1")
        .env("TEST_PROVIDER_LOCK", context.temp_dir.child("provider.lock").path()), @r###"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + example==1.0.0
    "###);
    Ok(())
}

/// A provider cannot depend on a wheel that needs that provider during selection.
#[test]
fn variant_provider_dependency_cycle() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let metadata = json!({
        "$schema": VARIANT_SCHEMA,
        "default-priorities": {"namespace": ["cpu"]},
        "variants": {"null": {}},
        "providers": {"cpu": {
            "plugin-api": "provider",
            "requires": ["provider==1.0.0"],
        }},
    });
    for name in ["example", "provider"] {
        write_wheel(&context, name, Some("null"), json!({}), &[], None)?;
        context
            .temp_dir
            .child(format!("{name}-1.0.0-variants.json"))
            .write_str(&serde_json::to_string(&metadata)?)?;
    }
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("example").arg("--preview-features").arg("wheel-variants")
        .arg("--no-index").arg("--find-links").arg(context.temp_dir.path()), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Failed to resolve requirements from `variant.providers.requires`
      cause: No solution found when resolving: `provider==1.0.0`
      cause: Cyclic variant provider dependency detected for `cpu`
    ");
    Ok(())
}

/// A provider can recur through a package whose variant selection is already pending.
#[test]
fn variant_provider_requires_root() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    write_wheel(&context, "example", Some("null"), json!({}), &[], None)?;
    context
        .temp_dir
        .child("example-1.0.0-variants.json")
        .write_str(&serde_json::to_string(&json!({
            "$schema": VARIANT_SCHEMA,
            "default-priorities": {"namespace": ["cpu"]},
            "variants": {"null": {}},
            "providers": {"cpu": {
                "plugin-api": "cpu_provider",
                "requires": ["example==1.0.0"],
            }},
        }))?)?;
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("example").arg("--preview-features").arg("wheel-variants")
        .arg("--no-index").arg("--find-links").arg(context.temp_dir.path()), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Failed to resolve requirements from `variant.providers.requires`
      cause: No solution found when resolving: `example==1.0.0`
      cause: Cyclic variant provider dependency detected for `cpu`
    ");
    Ok(())
}

/// Concurrent provider queries must not wait on each other's unfinished environments.
#[test]
fn variant_provider_mutual_cycle() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let cpu = json!({"plugin-api": "alpha", "requires": ["alpha==1.0.0"]});
    let gpu = json!({"plugin-api": "beta", "requires": ["beta==1.0.0"]});
    for (name, providers) in [
        ("example", json!({"cpu": cpu, "gpu": gpu})),
        ("alpha", json!({"gpu": gpu})),
        ("beta", json!({"cpu": cpu})),
    ] {
        write_wheel(&context, name, Some("null"), json!({}), &[], None)?;
        context
            .temp_dir
            .child(format!("{name}-1.0.0-variants.json"))
            .write_str(&serde_json::to_string(&json!({
                "$schema": VARIANT_SCHEMA,
                "default-priorities": {"namespace": ["cpu", "gpu"]},
                "variants": {"null": {}},
                "providers": providers,
            }))?)?;
    }
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("example").arg("--preview-features").arg("wheel-variants")
        .arg("--no-index").arg("--find-links").arg(context.temp_dir.path())
        .env("UV_CONCURRENT_BUILDS", "1"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Failed to resolve requirements from `variant.providers.requires`
      cause: No solution found when resolving: `beta==1.0.0`
      cause: Failed to resolve requirements from `variant.providers.requires`
      cause: No solution found when resolving: `alpha==1.0.0`
      cause: Cyclic variant provider dependency detected for `gpu`
    ");
    Ok(())
}

/// Create an sdist whose build requirement has a provider that requires the sdist's package.
fn write_source_build_cycle(context: &TestContext) -> Result<()> {
    let source = context.temp_dir.child("source");
    source.create_dir_all()?;
    source.child("pyproject.toml").write_str(indoc! {r#"
        [build-system]
        requires = ["build-dependency==1.0.0"]
        build-backend = "backend"
        backend-path = ["."]
    "#})?;
    source.child("backend.py").write_str(indoc! {r#"
        from pathlib import Path

        def prepare_metadata_for_build_wheel(metadata_directory, config_settings=None):
            info = Path(metadata_directory) / "source_package-1.0.0.dist-info"
            info.mkdir()
            (info / "METADATA").write_text("Metadata-Version: 2.3\nName: source-package\nVersion: 1.0.0\n\n")
            return info.name
    "#})?;
    let output = context
        .python_command()
        .arg("-c")
        .arg(indoc! {r#"
            import sys
            import tarfile

            with tarfile.open(sys.argv[1], "w:gz") as archive:
                archive.add(sys.argv[2], arcname="source_package-1.0.0")
        "#})
        .arg(context.temp_dir.child("source_package-1.0.0.tar.gz").path())
        .arg(source.path())
        .output()?;
    anyhow::ensure!(
        output.status.success(),
        "Failed to create sdist: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let mut metadata = json!({
        "$schema": VARIANT_SCHEMA,
        "default-priorities": {"namespace": ["cpu"]},
        "variants": {"null": {}},
    });
    let wheel_metadata = serde_json::to_string(&metadata)?;
    let (_, wheel) = generate_wheel_with_files(
        &"build-dependency".parse()?,
        &"1.0.0".parse()?,
        &[],
        &BTreeMap::new(),
        None,
        "py3-none-any",
        &[(
            "build_dependency-1.0.0.dist-info/variant.json",
            &wheel_metadata,
        )],
    );
    context
        .temp_dir
        .child("build_dependency-1.0.0-py3-none-any-null.whl")
        .write_binary(&wheel)?;
    metadata["providers"] = json!({
        "cpu": {"plugin-api": "provider", "requires": ["source-package==1.0.0"]},
    });
    context
        .temp_dir
        .child("build_dependency-1.0.0-variants.json")
        .write_str(&serde_json::to_string(&metadata)?)?;
    Ok(())
}

fn compile_source_build_cycle(context: &TestContext, requirement: &str) -> Result<String> {
    context
        .temp_dir
        .child("requirements.in")
        .write_str(requirement)?;
    let mut command = context.pip_compile();
    command
        .arg("requirements.in")
        .arg("--preview-features")
        .arg("wheel-variants")
        .arg("--no-index")
        .arg("--find-links")
        .arg(context.temp_dir.path())
        .arg("--no-header")
        .arg("--no-annotate");
    let output = assert_cmd::Command::from_std(command)
        .timeout(Duration::from_secs(60))
        .output()?;
    let status = output
        .status
        .code()
        .ok_or_else(|| anyhow::anyhow!("compile timed out or was interrupted"))?;
    let mut snapshot = format!(
        "exit_code: {} ({})\n",
        status,
        if output.status.success() {
            "success"
        } else {
            "failure"
        }
    );
    if !output.stdout.is_empty() {
        snapshot.push_str("----- stdout -----\n");
        snapshot.push_str(&String::from_utf8_lossy(&output.stdout));
        snapshot.push('\n');
    }
    if !output.stderr.is_empty() {
        snapshot.push_str("----- stderr -----\n");
        snapshot.push_str(&String::from_utf8_lossy(&output.stderr));
    }
    Ok(uv_test::apply_filters(snapshot, context.filters()))
}

/// A provider requiring an sdist currently being built cannot acquire its source lock again.
#[test]
fn variant_provider_source_build_cycle() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    write_source_build_cycle(&context)?;
    let snapshot = compile_source_build_cycle(&context, "source-package==1.0.0\n")?;
    insta::assert_snapshot!(snapshot, @r"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Failed to download and build `source-package==1.0.0`
      cause: Failed to resolve requirements from `build-system.requires`
      cause: No solution found when resolving: `build-dependency==1.0.0`
      cause: Failed to resolve requirements from `variant.providers.requires`
      cause: No solution found when resolving: `source-package==1.0.0`
      cause: Failed to download and build `source-package==1.0.0`
      cause: Cyclic build dependency detected for `source-package==1.0.0`
    ");
    Ok(())
}

/// A wheel can satisfy the provider while another source for the same package is being built.
#[test]
fn variant_provider_source_build_with_wheel() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    write_source_build_cycle(&context)?;
    let (filename, wheel) = generate_wheel_with_files(
        &"source-package".parse()?,
        &"1.0.0".parse()?,
        &[],
        &BTreeMap::new(),
        None,
        "py3-none-any",
        &[(
            "provider.py",
            "namespace = 'cpu'\ndef get_supported_configs(): return []\n",
        )],
    );
    context.temp_dir.child(filename).write_binary(&wheel)?;
    let archive = context.temp_dir.child("source_package-1.0.0.tar.gz");
    let url = Url::from_file_path(archive.path())
        .map_err(|()| anyhow::anyhow!("invalid source archive path"))?;
    let snapshot = compile_source_build_cycle(&context, &format!("source-package @ {url}\n"))?;
    insta::assert_snapshot!(snapshot, @r"
    exit_code: 0 (success)
    ----- stdout -----
    source-package @ file://[TEMP_DIR]/source_package-1.0.0.tar.gz

    ----- stderr -----
    Resolved 1 package in [TIME]
    ");
    Ok(())
}
