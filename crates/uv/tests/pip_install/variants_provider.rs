//! Variant provider subprocess behavior.

use std::collections::BTreeMap;
use std::env;

use anyhow::Result;
use assert_fs::prelude::*;
use indoc::indoc;
use serde_json::{Value, json};

use uv_test::packse::generate_wheel_with_files;
use uv_test::{TestContext, uv_snapshot};
use uv_variants::variants_json::VARIANT_SCHEMA;

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
