//! PEP 825 metadata in interoperable lockfiles.

use std::collections::BTreeMap;

use anyhow::Result;
use assert_fs::prelude::*;
use indoc::indoc;
use serde_json::json;
use sha2::{Digest, Sha256};
use wiremock::matchers::path;
use wiremock::{Mock, MockServer, ResponseTemplate};

use uv_test::packse::generate_wheel_with_files;
use uv_test::uv_snapshot;
use uv_variants::variants_json::VARIANT_SCHEMA;

use super::variants::{write_metadata, write_wheel};

#[tokio::test]
async fn pep825_pylock_metadata() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_filter((r"[a-f0-9]{64}", "[HASH]"));
    let properties = json!({"gpu": {"cuda": ["12.0", "13.0", "14.0"]}});
    // Provider extensions in the original wheel must not override the target already
    // selected from pylock's standard metadata and explicit properties.
    let metadata = json!({
        "$schema": VARIANT_SCHEMA,
        "default-priorities": {"namespace": ["gpu", "cpu"]},
        "variants": {"fast": properties},
        "providers": {"gpu": {
            "requires": ["gpu-provider==1.0.0"],
            "plugin-api": "gpu_provider.plugin"
        }},
    });
    let (_, wheel) = generate_wheel_with_files(
        &"example".parse()?,
        &"1.0.0".parse()?,
        &[],
        &BTreeMap::new(),
        None,
        "py3-none-any",
        &[
            (
                "example-1.0.0.dist-info/variant.json",
                &serde_json::to_string(&metadata)?,
            ),
            ("example/selected.txt", "fast build 0"),
        ],
    );
    context
        .temp_dir
        .child("example-1.0.0-py3-none-any-fast.whl")
        .write_binary(&wheel)?;
    write_wheel(&context, "example", Some("null"), json!({}), &[], None)?;
    write_wheel(&context, "example", None, json!({}), &[], None)?;
    write_metadata(&context, json!({"fast": properties, "null": {}}))?;
    context
        .temp_dir
        .child("requirements.in")
        .write_str("example")?;
    uv_snapshot!(context.filters(), context.pip_compile().arg("--preview-features").arg("wheel-variants")
        .arg("requirements.in").arg("--no-index").arg("--find-links").arg(context.temp_dir.path())
        .arg("--universal").arg("--no-header").arg("-o").arg("pylock.toml"), @r###"
    exit_code: 0 (success)
    ----- stdout -----
    lock-version = "1.0"
    created-by = "uv"
    requires-python = ">=3.12"

    [[packages]]
    name = "example"
    version = "1.0.0"
    wheels = [
        { url = "file://[TEMP_DIR]/example-1.0.0-py3-none-any.whl", hashes = { sha256 = "[HASH]" } },
        { url = "file://[TEMP_DIR]/example-1.0.0-py3-none-any-fast.whl", hashes = { sha256 = "[HASH]" } },
        { url = "file://[TEMP_DIR]/example-1.0.0-py3-none-any-null.whl", hashes = { sha256 = "[HASH]" } },
    ]

    [packages.variants-json]
    "$schema" = "https://variants-schema.wheelnext.dev/peps/825/v0.1.1.json"
    default-priorities = { namespace = ["gpu", "cpu"] }
    variants = { fast = { gpu = { cuda = ["12.0", "13.0", "14.0"] } }, null = {} }

    ----- stderr -----
    Resolved 1 package in [TIME]
    "###);
    // The exported file must remain usable when the mutable index metadata changes.
    context
        .temp_dir
        .child("example-1.0.0-variants.json")
        .write_str("{}")?;
    // The separate pylock preview does not enable variants or read the target property file.
    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("--preview-features").arg("pylock").arg("pylock.toml")
        .env("UV_VARIANT_LOCK", context.temp_dir.child("missing.toml").path()), @r###"
    exit_code: 2 (failure)
    ----- stderr -----
    error: This lockfile uses wheel variants; pass `--preview-features wheel-variants` to use it
    "###);
    uv_snapshot!(context.filters(), context.pip_sync().arg("--preview-features").arg("pylock,wheel-variants").arg("pylock.toml")
        .env("UV_VARIANT_LOCK", context.temp_dir.child("target.toml").path()), @r###"
    exit_code: 0 (success)
    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + example==1.0.0
    "###);
    uv_snapshot!(context.filters(), context.python_command().arg("-c").arg(indoc! {r#"
        import example
        import json
        from pathlib import Path
        from importlib.metadata import distribution
        print(Path(example.__file__).with_name("selected.txt").read_text())
        print(json.loads(distribution("example").read_text("uv_variant.json")))
    "#}), @r###"
    exit_code: 0 (success)
    ----- stdout -----
    fast build 0
    {'variant': {'gpu': {'cuda': ['12.0', '13.0']}}, 'label': 'fast'}
    "###);

    // Direct wheels carry the same complete metadata, with paths relative to the output file.
    context.temp_dir.child("exports").create_dir_all()?;
    context
        .temp_dir
        .child("direct.in")
        .write_str("./example-1.0.0-py3-none-any-fast.whl")?;
    uv_snapshot!(context.filters(), context.pip_compile()
        .arg("--preview-features").arg("wheel-variants")
        .arg("direct.in").arg("--universal").arg("--no-index").arg("--no-header")
        .arg("-o").arg("exports/pylock.toml"), @r###"
    exit_code: 0 (success)
    ----- stdout -----
    lock-version = "1.0"
    created-by = "uv"
    requires-python = ">=3.12"

    [[packages]]
    name = "example"
    version = "1.0.0"
    archive = { path = "../example-1.0.0-py3-none-any-fast.whl", hashes = { sha256 = "[HASH]" } }

    [packages.variants-json]
    "$schema" = "https://variants-schema.wheelnext.dev/peps/825/v0.1.1.json"
    default-priorities = { namespace = ["gpu", "cpu"] }
    variants = { fast = { gpu = { cuda = ["12.0", "13.0", "14.0"] } } }

    ----- stderr -----
    Resolved 1 package in [TIME]
    "###);
    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("--preview-features").arg("pylock,wheel-variants")
        .arg("exports/pylock.toml").arg("--reinstall")
        .env("UV_VARIANT_LOCK", context.temp_dir.child("target.toml").path()), @r###"
    exit_code: 0 (success)
    ----- stderr -----
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - example==1.0.0
     + example==1.0.0 (from file://[TEMP_DIR]/example-1.0.0-py3-none-any-fast.whl)
    "###);

    // uv.lock export must also read direct-wheel metadata without relying on an index sidecar.
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["example"]

        [tool.uv.sources]
        example = { path = "example-1.0.0-py3-none-any-fast.whl" }
    "#})?;
    uv_snapshot!(context.filters(), context.lock()
        .arg("--preview-features").arg("wheel-variants").arg("--no-index"), @r###"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);
    uv_snapshot!(context.filters(), context.export()
        .arg("--preview-features").arg("wheel-variants")
        .arg("--frozen").arg("--no-header").arg("--format").arg("pylock.toml"), @r###"
    exit_code: 0 (success)
    ----- stdout -----
    lock-version = "1.0"
    created-by = "uv"
    requires-python = ">=3.12"

    [[packages]]
    name = "example"
    version = "1.0.0"
    archive = { path = "example-1.0.0-py3-none-any-fast.whl", hashes = { sha256 = "[HASH]" } }

    [packages.variants-json]
    "$schema" = "https://variants-schema.wheelnext.dev/peps/825/v0.1.1.json"
    default-priorities = { namespace = ["gpu", "cpu"] }
    variants = { fast = { gpu = { cuda = ["12.0", "13.0", "14.0"] } } }
    "###);

    // A normal resolution already caches remote variant metadata for later offline exports.
    let server = MockServer::start().await;
    let digest = hex::encode(Sha256::digest(&wheel));
    Mock::given(path("/example-1.0.0-py3-none-any-fast.whl"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("cache-control", "max-age=3600")
                .set_body_bytes(wheel),
        )
        .mount(&server)
        .await;
    context.temp_dir.child("remote.in").write_str(&format!(
        "example @ {}/example-1.0.0-py3-none-any-fast.whl#sha256={digest}",
        server.uri()
    ))?;
    uv_snapshot!(context.filters(), context.pip_compile()
        .arg("--preview-features").arg("wheel-variants")
        .arg("remote.in").arg("--universal").arg("--no-index").arg("--no-header")
        .arg("--generate-hashes"), @r###"
    exit_code: 0 (success)
    ----- stdout -----
    example @ http://[LOCALHOST]/example-1.0.0-py3-none-any-fast.whl#sha256=[HASH] \
        --hash=sha256:[HASH]
        # via -r remote.in

    ----- stderr -----
    Resolved 1 package in [TIME]
    "###);
    uv_snapshot!(context.filters(), context.pip_compile()
        .arg("--preview-features").arg("wheel-variants")
        .arg("remote.in").arg("--universal").arg("--no-index").arg("--no-header")
        .arg("--offline").arg("-o").arg("exports/pylock.remote.toml"), @r###"
    exit_code: 0 (success)
    ----- stdout -----
    lock-version = "1.0"
    created-by = "uv"
    requires-python = ">=3.12"

    [[packages]]
    name = "example"
    version = "1.0.0"
    archive = { url = "http://[LOCALHOST]/example-1.0.0-py3-none-any-fast.whl", hashes = { sha256 = "[HASH]" } }

    [packages.variants-json]
    "$schema" = "https://variants-schema.wheelnext.dev/peps/825/v0.1.1.json"
    default-priorities = { namespace = ["gpu", "cpu"] }
    variants = { fast = { gpu = { cuda = ["12.0", "13.0", "14.0"] } } }

    ----- stderr -----
    Resolved 1 package in [TIME]
    "###);

    // Explicit archives still have to support the requested target before any reinstall.
    context.temp_dir.child("target.toml").write_str(indoc! {r#"
        [metadata]
        version = "0.1"
        created-by = "test"
        [[provider]]
        namespace = "gpu"
        resolved = []
        [provider.properties]
        cuda = ["15.0"]
    "#})?;
    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("--preview-features").arg("pylock,wheel-variants")
        .arg("exports/pylock.toml").arg("--reinstall")
        .env("UV_VARIANT_LOCK", context.temp_dir.child("target.toml").path()), @r###"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Package `example` can't be installed because the binary distribution is incompatible with the current platform
    "###);
    Ok(())
}
