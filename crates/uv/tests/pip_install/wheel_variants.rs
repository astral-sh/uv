use std::collections::BTreeMap;

use anyhow::{Context, Result};
use assert_fs::prelude::*;
use indoc::{formatdoc, indoc};
use insta::assert_snapshot;
use predicates::prelude::predicate;
use serde_json::json;
use sha2::{Digest, Sha256};
use uv_cache::CacheBucket;
use uv_static::EnvVars;
use uv_test::archive::generate_source_archive;
use uv_test::packse::generate_wheel_with_files;
use uv_test::uv_snapshot;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// An empty-property variant takes precedence over an ordinary wheel when opted in.
#[test]
fn select_empty_property_variant() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let links = context.temp_dir.child("links");
    links.create_dir_all()?;
    let variant_metadata = json!({
        "$schema": "https://variants-schema.wheelnext.dev/peps/825/v0.1.1.json",
        "default-priorities": { "namespace": ["cuda"] },
        "variants": { "empty": { "cuda": {} } },
    })
    .to_string();
    fs_err::copy(
        context
            .workspace_root
            .join("test/links/ok-1.0.0-py3-none-any.whl"),
        links.child("ok-1.0.0-py3-none-any.whl"),
    )?;
    let (_, variant_wheel) = generate_wheel_with_files(
        &"ok".parse()?,
        &"1.0.0".parse()?,
        &[],
        &BTreeMap::default(),
        None,
        "py3-none-any",
        &[
            ("ok/variant.txt", "empty"),
            ("ok-1.0.0.dist-info/variant.json", &variant_metadata),
        ],
    );
    links
        .child("ok-1.0.0-py3-none-any-empty.whl")
        .write_binary(&variant_wheel)?;
    links
        .child("ok-1.0.0-9-py3-none-any-gpu.whl")
        .write_str("unsupported wheel")?;
    links
        .child("ok-1.0.0-10-py3-none-any-missing.whl")
        .write_str("unlisted wheel")?;
    links
        .child("ok-1.0.0-99-py3-none-any-null.whl")
        .write_str("lower-priority variant")?;
    links.child("ok-1.0.0-variants.json").write_str(
        &json!({
            "$schema": "https://variants-schema.wheelnext.dev/peps/825/v0.1.1.json",
            "default-priorities": { "namespace": ["cuda"] },
            "variants": { "empty": { "cuda": {} }, "gpu": { "cuda": { "version": ["12"] } }, "null": {} },
        })
        .to_string(),
    )?;

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
    assert_snapshot!(context.read(context.site_packages().join("ok/variant.txt")), @"empty");

    Ok(())
}

/// Sync must replace an installed wheel when the selected variant changes its dependencies.
#[test]
fn sync_changes_installed_variant() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let links = context.temp_dir.child("links");
    links.create_dir_all()?;
    let metadata = json!({
        "$schema": "https://variants-schema.wheelnext.dev/peps/825/v0.1.1.json",
        "default-priorities": { "namespace": ["cuda"] },
        "variants": { "null": {} },
    })
    .to_string();
    let (filename, wheel) = generate_wheel_with_files(
        &"dep".parse()?,
        &"1.0.0".parse()?,
        &[],
        &BTreeMap::default(),
        None,
        "py3-none-any",
        &[],
    );
    links.child(filename).write_binary(&wheel)?;
    let (filename, wheel) = generate_wheel_with_files(
        &"ok".parse()?,
        &"1.0.0".parse()?,
        &["dep==1.0.0".parse()?],
        &BTreeMap::default(),
        None,
        "py3-none-any",
        &[("ok/variant.txt", "ordinary")],
    );
    links.child(filename).write_binary(&wheel)?;
    let (_, wheel) = generate_wheel_with_files(
        &"ok".parse()?,
        &"1.0.0".parse()?,
        &[],
        &BTreeMap::default(),
        None,
        "py3-none-any",
        &[
            ("ok/variant.txt", "null"),
            ("ok-1.0.0.dist-info/variant.json", &metadata),
        ],
    );
    links
        .child("ok-1.0.0-py3-none-any-null.whl")
        .write_binary(&wheel)?;
    links.child("ok-1.0.0-variants.json").write_str(&metadata)?;

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--no-index")
        .arg("--find-links")
        .arg(links.path())
        .arg("ok==1.0.0"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + dep==1.0.0
     + ok==1.0.0
    ");

    let requirements = context.temp_dir.child("requirements.txt");
    requirements.write_str("ok==1.0.0\n")?;
    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("--no-index")
        .arg("--find-links")
        .arg(links.path())
        .arg("--preview-features")
        .arg("wheel-variants")
        .arg(requirements.path()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 2 packages in [TIME]
    Installed 1 package in [TIME]
     - dep==1.0.0
     ~ ok==1.0.0
    ");
    assert_snapshot!(context.read(context.site_packages().join("ok/variant.txt")), @"null");

    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("--no-index")
        .arg("--find-links")
        .arg(links.path())
        .arg("--preview-features")
        .arg("wheel-variants")
        .arg(requirements.path()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Checked 1 package in [TIME]
    ");

    requirements.write_str("dep==1.0.0\nok==1.0.0\n")?;
    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("--no-index")
        .arg("--find-links")
        .arg(links.path())
        .arg(requirements.path()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 2 packages in [TIME]
     + dep==1.0.0
     ~ ok==1.0.0
    ");
    assert_snapshot!(context.read(context.site_packages().join("ok/variant.txt")), @"ordinary");
    Ok(())
}

/// A cached wheel with unsupported properties cannot be reused as a compatible variant.
#[test]
fn cached_variant_requires_compatible_properties() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let links = context.temp_dir.child("links");
    links.create_dir_all()?;
    let metadata = json!({
        "$schema": "https://variants-schema.wheelnext.dev/peps/825/v0.1.1.json",
        "default-priorities": { "namespace": ["cuda"] },
        "variants": { "empty": {} },
    })
    .to_string();
    let (_, wheel) = generate_wheel_with_files(
        &"ok".parse()?,
        &"1.0.0".parse()?,
        &[],
        &BTreeMap::default(),
        None,
        "py3-none-any",
        &[("ok-1.0.0.dist-info/variant.json", &metadata)],
    );
    links
        .child("ok-1.0.0-py3-none-any-empty.whl")
        .write_binary(&wheel)?;
    links.child("ok-1.0.0-variants.json").write_str(&metadata)?;

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

    uv_snapshot!(context.filters(), context.pip_uninstall()
        .arg("ok"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Uninstalled 1 package in [TIME]
     - ok==1.0.0
    ");

    let variant_metadata = context
        .cache_files(CacheBucket::Archive)?
        .into_iter()
        .find(|path| path.ends_with("ok-1.0.0.dist-info/variant.json"))
        .context("cached variant metadata not found")?;
    fs_err::write(
        variant_metadata,
        json!({
            "$schema": "https://variants-schema.wheelnext.dev/peps/825/v0.1.1.json",
            "default-priorities": { "namespace": ["cuda"] },
            "variants": { "empty": { "cuda": { "version": ["12"] } } },
        })
        .to_string(),
    )?;

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--no-index")
        .arg("--find-links")
        .arg(links.path())
        .arg("--preview-features")
        .arg("wheel-variants")
        .arg("ok==1.0.0"), @"
    exit_code: 1 (failure)
    ----- stderr -----
    Resolved 1 package in [TIME]
    error: Failed to download `ok==1.0.0`
      cause: Wheel variant has unsupported or invalid properties: `ok-1.0.0-py3-none-any-empty.whl`
    ");
    Ok(())
}

/// A source resolution must not reuse a cached variant that the resolver did not select.
#[test]
fn source_resolution_does_not_reuse_cached_variant() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let links = context.temp_dir.child("links");
    links.create_dir_all()?;
    let metadata = json!({
        "$schema": "https://variants-schema.wheelnext.dev/peps/825/v0.1.1.json",
        "default-priorities": { "namespace": ["cuda"] },
        "variants": { "empty": {} },
    })
    .to_string();
    let (_, wheel) = generate_wheel_with_files(
        &"ok".parse()?,
        &"1.0.0".parse()?,
        &[],
        &BTreeMap::default(),
        None,
        "py3-none-any",
        &[
            ("ok/variant.txt", "variant"),
            ("ok-1.0.0.dist-info/variant.json", &metadata),
        ],
    );
    links
        .child("ok-1.0.0-py3-none-any-empty.whl")
        .write_binary(&wheel)?;
    links.child("ok-1.0.0-variants.json").write_str(&metadata)?;
    let sdist = generate_source_archive(&"ok".parse()?, &"1.0.0".parse()?, "", None)?;
    links.child("ok-1.0.0.tar.gz").write_binary(&sdist)?;

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
    assert_snapshot!(context.read(context.site_packages().join("ok/variant.txt")), @"variant");

    uv_snapshot!(context.filters(), context.pip_uninstall().arg("ok"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Uninstalled 1 package in [TIME]
     - ok==1.0.0
    ");
    fs_err::remove_file(links.child("ok-1.0.0-variants.json"))?;

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
    assert!(!context.site_packages().join("ok/variant.txt").exists());
    Ok(())
}

/// A wheel's variant metadata must describe only its own variant.
#[test]
fn variant_wheel_with_multiple_labels() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let links = context.temp_dir.child("links");
    links.create_dir_all()?;
    let metadata = json!({
        "$schema": "https://variants-schema.wheelnext.dev/peps/825/v0.1.1.json",
        "default-priorities": { "namespace": ["cuda"] },
        "variants": { "null": {}, "other": {} },
    })
    .to_string();
    let (_, wheel) = generate_wheel_with_files(
        &"ok".parse()?,
        &"1.0.0".parse()?,
        &[],
        &BTreeMap::default(),
        None,
        "py3-none-any",
        &[("ok-1.0.0.dist-info/variant.json", &metadata)],
    );
    links
        .child("ok-1.0.0-py3-none-any-null.whl")
        .write_binary(&wheel)?;
    links.child("ok-1.0.0-variants.json").write_str(&metadata)?;

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--no-index")
        .arg("--find-links")
        .arg(links.path())
        .arg("--preview-features")
        .arg("wheel-variants")
        .arg("ok==1.0.0"), @"
    exit_code: 1 (failure)
    ----- stderr -----
    Resolved 1 package in [TIME]
    error: Failed to download `ok==1.0.0`
      cause: Wheel variant has unsupported or invalid properties: `ok-1.0.0-py3-none-any-null.whl`
    ");
    Ok(())
}

/// An unsupported variant on the first index must not hide a compatible variant on the second,
/// even when an ordinary wheel is available on the third.
#[tokio::test]
async fn select_later_variant_with_unsafe_best_match() -> Result<()> {
    let server = MockServer::start().await;
    let context = uv_test::test_context!("3.12").with_filter((server.uri(), "http://[LOCALHOST]"));
    let metadata = json!({
        "$schema": "https://variants-schema.wheelnext.dev/peps/825/v0.1.1.json",
        "default-priorities": { "namespace": ["cuda"] },
        "variants": { "null": {} },
    })
    .to_string();
    let (_, wheel) = generate_wheel_with_files(
        &"ok".parse()?,
        &"1.0.0".parse()?,
        &[],
        &BTreeMap::default(),
        None,
        "py3-none-any",
        &[
            ("ok/variant.txt", "null"),
            ("ok-1.0.0.dist-info/variant.json", &metadata),
        ],
    );
    Mock::given(method("GET"))
        .and(path("/first/ok/"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            format!(
                r#"<a href="{}/first/files/ok-1.0.0-py3-none-any-gpu.whl">wheel</a>
                <a href="{}/first/files/ok-1.0.0-variants.json">metadata</a>"#,
                server.uri(),
                server.uri(),
            ),
            "text/html",
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/first/files/ok-1.0.0-variants.json"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(
                json!({
                    "$schema": "https://variants-schema.wheelnext.dev/peps/825/v0.1.1.json",
                    "default-priorities": { "namespace": ["cuda"] },
                    "variants": { "gpu": { "cuda": { "version": ["12"] } } },
                })
                .to_string(),
            ),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/second/ok/"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            format!(
                r#"<a href="{}/second/files/ok-1.0.0-py3-none-any-null.whl">wheel</a>
                <a href="{}/second/files/ok-1.0.0-variants.json">metadata</a>"#,
                server.uri(),
                server.uri(),
            ),
            "text/html",
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/second/files/ok-1.0.0-variants.json"))
        .respond_with(ResponseTemplate::new(200).set_body_string(metadata))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/second/files/ok-1.0.0-py3-none-any-null.whl"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(wheel))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/third/ok/"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"<a href="/third/files/ok-1.0.0-py3-none-any.whl">wheel</a>"#,
            "text/html",
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/third/files/ok-1.0.0-py3-none-any.whl"))
        .respond_with(
            ResponseTemplate::new(200).set_body_bytes(fs_err::read(
                context
                    .workspace_root
                    .join("test/links/ok-1.0.0-py3-none-any.whl"),
            )?),
        )
        .mount(&server)
        .await;

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--extra-index-url")
        .arg(format!("{}/first/", server.uri()))
        .arg("--extra-index-url")
        .arg(format!("{}/second/", server.uri()))
        .arg("--index-url")
        .arg(format!("{}/third/", server.uri()))
        .arg("--index-strategy")
        .arg("unsafe-best-match")
        .arg("--preview-features")
        .arg("wheel-variants")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("ok==1.0.0"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + ok==1.0.0
    ");
    assert_snapshot!(context.read(context.site_packages().join("ok/variant.txt")), @"null");
    Ok(())
}

/// Invalid metadata for a variant-only version also permits backtracking.
#[test]
fn invalid_variant_only_metadata_backtracks() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let links = context.temp_dir.child("links");
    links.create_dir_all()?;
    fs_err::copy(
        context
            .workspace_root
            .join("test/links/ok-1.0.0-py3-none-any.whl"),
        links.child("ok-1.0.0-py3-none-any.whl"),
    )?;
    links
        .child("ok-2.0.0-py3-none-any-null.whl")
        .write_str("unverified wheel")?;
    links.child("ok-2.0.0-variants.json").write_str("{}")?;

    uv_snapshot!(context.filters(), context.pip_install()
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

/// A sidecar from another source cannot make a variant wheel eligible.
#[test]
fn variant_metadata_is_scoped_to_its_source() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let wheels = context.temp_dir.child("wheels");
    let metadata = context.temp_dir.child("metadata");
    wheels.create_dir_all()?;
    metadata.create_dir_all()?;
    fs_err::copy(
        context
            .workspace_root
            .join("test/links/ok-1.0.0-py3-none-any.whl"),
        wheels.child("ok-1.0.0-py3-none-any.whl"),
    )?;
    wheels
        .child("ok-2.0.0-py3-none-any-null.whl")
        .write_str("unverified wheel")?;
    metadata.child("ok-2.0.0-variants.json").write_str(
        &json!({
            "$schema": "https://variants-schema.wheelnext.dev/peps/825/v0.1.1.json",
            "default-priorities": { "namespace": ["cuda"] },
            "variants": { "null": {} },
        })
        .to_string(),
    )?;

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--no-index")
        .arg("--find-links")
        .arg(wheels.path())
        .arg("--find-links")
        .arg(metadata.path())
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

/// Only the selected version's variant metadata is fetched, and configured caching works offline.
#[tokio::test]
async fn selected_variant_metadata_is_cached() -> Result<()> {
    let server = MockServer::start().await;
    let context = uv_test::test_context!("3.12").with_filter((server.uri(), "http://[LOCALHOST]"));
    context
        .temp_dir
        .child("requirements.in")
        .write_str("ok==1.0.0")?;
    context
        .temp_dir
        .child("uv.toml")
        .write_str(&formatdoc! {r#"
        [[index]]
        url = "{}/simple/"
        default = true
        cache-control = {{ files = "max-age=3600" }}
    "#, server.uri()})?;

    let simple_index = json!({
        "meta": { "api-version": "1.0" },
        "name": "ok",
        "files": [
            {
                "filename": "ok-1.0.0-py3-none-any.whl",
                "url": format!("{}/files/ok-1.0.0-py3-none-any.whl", server.uri()),
                "hashes": {},
                "core-metadata": true,
            },
            {
                "filename": "ok-1.0.0-variants.json",
                "url": format!("{}/files/ok-1.0.0-variants.json", server.uri()),
                "hashes": {},
            },
            {
                "filename": "ok-2.0.0-py3-none-any.whl",
                "url": format!("{}/files/ok-2.0.0-py3-none-any.whl", server.uri()),
                "hashes": {},
            },
            {
                "filename": "ok-2.0.0-variants.json",
                "url": format!("{}/files/ok-2.0.0-variants.json", server.uri()),
                "hashes": {},
            },
        ],
    });
    Mock::given(method("GET"))
        .and(path("/simple/ok/"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(
                    simple_index.to_string(),
                    "application/vnd.pypi.simple.v1+json",
                )
                .insert_header("cache-control", "max-age=3600"),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/files/ok-1.0.0-py3-none-any.whl.metadata"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("Metadata-Version: 2.3\nName: ok\nVersion: 1.0.0\n")
                .insert_header("cache-control", "max-age=3600"),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/files/ok-1.0.0-variants.json"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({
                    "$schema": "https://variants-schema.wheelnext.dev/peps/825/v0.1.1.json",
                    "default-priorities": { "namespace": ["x86_64"] },
                    "variants": { "null": {} },
                }))
                .insert_header("cache-control", "no-store"),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/files/ok-2.0.0-variants.json"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;

    uv_snapshot!(context.filters(), context.pip_compile()
        .arg("requirements.in")
        .arg("--no-header")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER), @"
    exit_code: 0 (success)
    ----- stdout -----
    ok==1.0.0
        # via -r requirements.in

    ----- stderr -----
    Resolved 1 package in [TIME]
    ");
    let requests = server
        .received_requests()
        .await
        .ok_or_else(|| anyhow::anyhow!("Request recording is disabled"))?;
    assert!(
        requests
            .iter()
            .all(|request| request.url.path() != "/files/ok-1.0.0-variants.json")
    );

    uv_snapshot!(context.filters(), context.pip_compile()
        .arg("requirements.in")
        .arg("--no-header")
        .arg("--preview-features")
        .arg("wheel-variants")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER), @"
    exit_code: 0 (success)
    ----- stdout -----
    ok==1.0.0
        # via -r requirements.in

    ----- stderr -----
    Resolved 1 package in [TIME]
    ");

    uv_snapshot!(context.filters(), context.pip_compile()
        .arg("requirements.in")
        .arg("--no-header")
        .arg("--preview-features")
        .arg("wheel-variants")
        .arg("--offline")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER), @"
    exit_code: 0 (success)
    ----- stdout -----
    ok==1.0.0
        # via -r requirements.in

    ----- stderr -----
    Resolved 1 package in [TIME]
    ");
    Ok(())
}

/// A changed advertised hash invalidates cached metadata, and mismatched bytes are not cached.
#[tokio::test]
async fn variant_metadata_hashes_are_verified() -> Result<()> {
    let server = MockServer::start().await;
    let context = uv_test::test_context!("3.12").with_filter((server.uri(), "http://[LOCALHOST]"));
    context
        .temp_dir
        .child("requirements.in")
        .write_str("ok==1.0.0")?;
    let first_sidecar = json!({
        "$schema": "https://variants-schema.wheelnext.dev/peps/825/v0.1.1.json",
        "default-priorities": { "namespace": ["x86_64"] },
        "variants": { "null": {} },
    })
    .to_string();
    let second_sidecar = format!("{first_sidecar}\n");
    let first_hash = hex::encode(Sha256::digest(first_sidecar.as_bytes()));
    let second_hash = hex::encode(Sha256::digest(second_sidecar.as_bytes()));
    let mut simple_index = json!({
        "meta": { "api-version": "1.0" },
        "name": "ok",
        "files": [
            {
                "filename": "ok-1.0.0-py3-none-any.whl",
                "url": format!("{}/files/ok-1.0.0-py3-none-any.whl", server.uri()),
                "hashes": {},
                "core-metadata": true,
            },
            {
                "filename": "ok-1.0.0-variants.json",
                "url": format!("{}/files/ok-1.0.0-variants.json", server.uri()),
                "hashes": { "sha256": first_hash },
            },
        ],
    });
    Mock::given(method("GET"))
        .and(path("/files/ok-1.0.0-py3-none-any.whl.metadata"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("Metadata-Version: 2.3\nName: ok\nVersion: 1.0.0\n")
                .insert_header("cache-control", "max-age=3600"),
        )
        .mount(&server)
        .await;
    let index_mock = Mock::given(method("GET"))
        .and(path("/simple/ok/"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(
                    simple_index.to_string(),
                    "application/vnd.pypi.simple.v1+json",
                )
                .insert_header("cache-control", "no-store"),
        )
        .expect(1)
        .mount_as_scoped(&server)
        .await;
    let sidecar_mock = Mock::given(method("GET"))
        .and(path("/files/ok-1.0.0-variants.json"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(first_sidecar.clone())
                .insert_header("cache-control", "max-age=3600"),
        )
        .expect(1)
        .mount_as_scoped(&server)
        .await;
    uv_snapshot!(context.filters(), context.pip_compile()
        .arg("requirements.in")
        .arg("--no-header")
        .arg("--index-url")
        .arg(format!("{}/simple/", server.uri()))
        .arg("--preview-features")
        .arg("wheel-variants")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER), @"
    exit_code: 0 (success)
    ----- stdout -----
    ok==1.0.0
        # via -r requirements.in

    ----- stderr -----
    Resolved 1 package in [TIME]
    ");
    drop(index_mock);
    drop(sidecar_mock);

    simple_index["files"][1]["hashes"]["sha256"] = second_hash.into();
    let index_mock = Mock::given(method("GET"))
        .and(path("/simple/ok/"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(
                    simple_index.to_string(),
                    "application/vnd.pypi.simple.v1+json",
                )
                .insert_header("cache-control", "no-store"),
        )
        .expect(1)
        .mount_as_scoped(&server)
        .await;
    let sidecar_mock = Mock::given(method("GET"))
        .and(path("/files/ok-1.0.0-variants.json"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(second_sidecar)
                .insert_header("cache-control", "max-age=3600"),
        )
        .expect(1)
        .mount_as_scoped(&server)
        .await;
    uv_snapshot!(context.filters(), context.pip_compile()
        .arg("requirements.in")
        .arg("--no-header")
        .arg("--index-url")
        .arg(format!("{}/simple/", server.uri()))
        .arg("--preview-features")
        .arg("wheel-variants")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER), @"
    exit_code: 0 (success)
    ----- stdout -----
    ok==1.0.0
        # via -r requirements.in

    ----- stderr -----
    Resolved 1 package in [TIME]
    ");
    drop(index_mock);
    drop(sidecar_mock);

    simple_index["files"][1]["hashes"]["sha256"] = "0".repeat(64).into();
    Mock::given(method("GET"))
        .and(path("/simple/ok/"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(
                    simple_index.to_string(),
                    "application/vnd.pypi.simple.v1+json",
                )
                .insert_header("cache-control", "no-store"),
        )
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/files/ok-1.0.0-variants.json"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(first_sidecar)
                .insert_header("cache-control", "max-age=3600"),
        )
        .expect(2)
        .mount(&server)
        .await;
    uv_snapshot!(context.filters(), context.pip_compile()
        .arg("requirements.in")
        .arg("--no-header")
        .arg("--index-url")
        .arg(format!("{}/simple/", server.uri()))
        .arg("--preview-features")
        .arg("wheel-variants")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER), @"
    exit_code: 1 (failure)
    ----- stderr -----
    error: Hash mismatch for package metadata at `http://[LOCALHOST]/files/ok-1.0.0-variants.json`

    Expected:
      sha256:0000000000000000000000000000000000000000000000000000000000000000

    Computed:
      sha256:d5dd2ecf5be76b72a82f0e739b73a0fd5b26f137c91d9a1074181f0749fa4bba
    ");
    uv_snapshot!(context.filters(), context.pip_compile()
        .arg("requirements.in")
        .arg("--no-header")
        .arg("--index-url")
        .arg(format!("{}/simple/", server.uri()))
        .arg("--preview-features")
        .arg("wheel-variants")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER), @"
    exit_code: 1 (failure)
    ----- stderr -----
    error: Hash mismatch for package metadata at `http://[LOCALHOST]/files/ok-1.0.0-variants.json`

    Expected:
      sha256:0000000000000000000000000000000000000000000000000000000000000000

    Computed:
      sha256:d5dd2ecf5be76b72a82f0e739b73a0fd5b26f137c91d9a1074181f0749fa4bba
    ");
    Ok(())
}

/// A retryable HTTP error while fetching variant metadata is reported to the user.
#[tokio::test]
async fn unavailable_variant_metadata_is_an_error() -> Result<()> {
    let server = MockServer::start().await;
    let context = uv_test::test_context!("3.12").with_filter((server.uri(), "http://[LOCALHOST]"));
    let simple_index = json!({
        "meta": { "api-version": "1.0" },
        "name": "ok",
        "files": [
            {
                "filename": "ok-1.0.0-py3-none-any.whl",
                "url": format!("{}/files/ok-1.0.0-py3-none-any.whl", server.uri()),
                "hashes": {},
                "core-metadata": true,
            },
            {
                "filename": "ok-1.0.0-variants.json",
                "url": format!("{}/files/ok-1.0.0-variants.json", server.uri()),
                "hashes": {},
            },
        ],
    });
    Mock::given(method("GET"))
        .and(path("/simple/ok/"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            simple_index.to_string(),
            "application/vnd.pypi.simple.v1+json",
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/files/ok-1.0.0-py3-none-any.whl.metadata"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("Metadata-Version: 2.3\nName: ok\nVersion: 1.0.0\n"),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/files/ok-1.0.0-variants.json"))
        .respond_with(ResponseTemplate::new(500))
        .expect(4)
        .mount(&server)
        .await;

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--index-url")
        .arg(format!("{}/simple/", server.uri()))
        .arg("--preview-features")
        .arg("wheel-variants")
        .arg("ok==1.0.0")
        .env(EnvVars::UV_INTERNAL__TEST_NO_HTTP_RETRY_DELAY, "true")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Request failed after 3 retries in [TIME]
      cause: Failed to fetch: http://[LOCALHOST]/files/ok-1.0.0-variants.json
      cause: HTTP status server error (500 Internal Server Error) for url (http://[LOCALHOST]/files/ok-1.0.0-variants.json)
    ");
    Ok(())
}

/// An ignored HTTP error for a version's variant metadata makes that version unavailable.
#[tokio::test]
async fn unavailable_variant_metadata_backtracks() -> Result<()> {
    let server = MockServer::start().await;
    let context = uv_test::test_context!("3.12").with_filter((server.uri(), "http://[LOCALHOST]"));
    context.temp_dir.child("requirements.in").write_str("ok")?;
    context
        .temp_dir
        .child("uv.toml")
        .write_str(&formatdoc! {r#"
        [[index]]
        url = "{}/simple/"
        default = true
        ignore-error-codes = [403]
        "#, server.uri()})?;
    let simple_index = json!({
        "meta": { "api-version": "1.0" },
        "name": "ok",
        "files": [
            {
                "filename": "ok-1.0.0-py3-none-any.whl",
                "url": format!("{}/files/ok-1.0.0-py3-none-any.whl", server.uri()),
                "hashes": {},
                "core-metadata": true,
            },
            {
                "filename": "ok-2.0.0-py3-none-any.whl",
                "url": format!("{}/files/ok-2.0.0-py3-none-any.whl", server.uri()),
                "hashes": {},
                "core-metadata": true,
            },
            {
                "filename": "ok-2.0.0-variants.json",
                "url": format!("{}/files/ok-2.0.0-variants.json", server.uri()),
                "hashes": {},
            },
        ],
    });
    Mock::given(method("GET"))
        .and(path("/simple/ok/"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            simple_index.to_string(),
            "application/vnd.pypi.simple.v1+json",
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/files/ok-1.0.0-py3-none-any.whl.metadata"))
        .respond_with(ResponseTemplate::new(200).set_body_string(indoc! {"
            Metadata-Version: 2.3
            Name: ok
            Version: 1.0.0
        "}))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/files/ok-2.0.0-py3-none-any.whl.metadata"))
        .respond_with(ResponseTemplate::new(200).set_body_string(indoc! {"
            Metadata-Version: 2.3
            Name: ok
            Version: 2.0.0
        "}))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/files/ok-2.0.0-variants.json"))
        .respond_with(ResponseTemplate::new(403))
        .expect(1)
        .mount(&server)
        .await;

    uv_snapshot!(context.filters(), context.pip_compile()
        .arg("requirements.in")
        .arg("--no-header")
        .arg("--preview-features")
        .arg("wheel-variants")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER), @"
    exit_code: 0 (success)
    ----- stdout -----
    ok==1.0.0
        # via -r requirements.in

    ----- stderr -----
    Resolved 1 package in [TIME]
    ");
    Ok(())
}

/// A pinned version with invalid variant metadata cannot be used.
#[test]
fn invalid_variant_metadata() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let links = context.temp_dir.child("links");
    links.create_dir_all()?;
    fs_err::copy(
        context
            .workspace_root
            .join("test/links/ok-1.0.0-py3-none-any.whl"),
        links.child("ok-1.0.0-py3-none-any.whl"),
    )?;
    links.child("ok-1.0.0-variants.json").write_str(
        &json!({
            "$schema": "https://variants-schema.wheelnext.dev/peps/825/v0.1.1.json",
            "default-priorities": { "namespace": ["cuda"] },
            "variants": { "null": { "cuda": { "version": ["12"] } } },
        })
        .to_string(),
    )?;

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--no-index")
        .arg("--find-links")
        .arg(links.path())
        .arg("--preview-features")
        .arg("wheel-variants")
        .arg("ok==1.0.0"), @"
    exit_code: 1 (failure)
    ----- stderr -----
    error: No solution found when resolving dependencies
      cause: Because ok==1.0.0 has invalid metadata and you require ok==1.0.0, we can conclude that your requirements are unsatisfiable.

    hint: Metadata for `ok` (v1.0.0) could not be parsed:
      Failed to parse variant metadata `ok-1.0.0-variants.json`: The null variant must not have properties
    ");

    Ok(())
}

/// The resolver backtracks to an older version when the latest has invalid variant metadata.
#[test]
fn invalid_variant_metadata_backtrack() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let links = context.temp_dir.child("links");
    links.create_dir_all()?;
    fs_err::copy(
        context
            .workspace_root
            .join("test/links/ok-1.0.0-py3-none-any.whl"),
        links.child("ok-1.0.0-py3-none-any.whl"),
    )?;
    fs_err::copy(
        context
            .workspace_root
            .join("test/links/ok-2.0.0-py3-none-any.whl"),
        links.child("ok-2.0.0-py3-none-any.whl"),
    )?;
    links.child("ok-2.0.0-variants.json").write_str("{}")?;

    uv_snapshot!(context.filters(), context.pip_install()
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

/// An unreadable unknown schema is reported alongside the parsing error.
#[test]
fn invalid_variant_metadata_unknown_schema() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    fs_err::copy(
        context
            .workspace_root
            .join("test/links/ok-1.0.0-py3-none-any.whl"),
        context.temp_dir.child("ok-1.0.0-py3-none-any.whl"),
    )?;
    context.temp_dir.child("ok-1.0.0-variants.json").write_str(
        &json!({
            "$schema": "https://variants-schema.wheelnext.dev/peps/825/v0.2.0.json",
            "default-priorities": { "namespace": 42 },
            "variants": { "null": {} },
        })
        .to_string(),
    )?;

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--no-index")
        .arg("--find-links")
        .arg(context.temp_dir.path())
        .arg("--preview-features")
        .arg("wheel-variants")
        .arg("ok==1.0.0"), @"
    exit_code: 1 (failure)
    ----- stderr -----
    error: No solution found when resolving dependencies
      cause: Because ok==1.0.0 has invalid metadata and you require ok==1.0.0, we can conclude that your requirements are unsatisfiable.

    hint: Metadata for `ok` (v1.0.0) could not be parsed:
      Failed to parse variant metadata `ok-1.0.0-variants.json`: Unknown variant metadata schema:
        `https://variants-schema.wheelnext.dev/peps/825/v0.2.0.json`:
          invalid type: integer `42`, expected a sequence at line 1 column 108
    ");
    Ok(())
}

#[test]
fn variant_wheels_cannot_be_installed() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let links = context.temp_dir.child("links");
    links.create_dir_all()?;
    let wheel = links.child("ok-1.0.0-9-py3-none-any-null.whl");
    fs_err::copy(
        context
            .workspace_root
            .join("test/links/ok-1.0.0-py3-none-any.whl"),
        wheel.path(),
    )?;

    uv_snapshot!(
        context.filters(),
        context
            .pip_install()
            .arg("--no-index")
            .arg("--find-links")
            .arg(links.path())
            .arg("ok==1.0.0"),
        @"
    exit_code: 1 (failure)
    ----- stderr -----
    error: No solution found when resolving dependencies
      cause: Because ok==1.0.0 has only unsupported wheel variants and you require ok==1.0.0, we can conclude that your requirements are unsatisfiable.
    "
    );

    uv_snapshot!(
        context.filters(),
        context
            .pip_install()
            .arg("--no-index")
            .arg("--find-links")
            .arg(links.path())
            .arg("--preview-features")
            .arg("wheel-variants")
            .arg("ok==1.0.0"),
        @"
    exit_code: 1 (failure)
    ----- stderr -----
    error: No solution found when resolving dependencies
      cause: Because ok==1.0.0 has only unsupported wheel variants and you require ok==1.0.0, we can conclude that your requirements are unsatisfiable.
    "
    );

    uv_snapshot!(context.filters(), context.pip_install().arg(wheel.path()), @"
    exit_code: 1 (failure)
    ----- stderr -----
    error: Failed to read `ok @ file://[TEMP_DIR]/links/ok-1.0.0-9-py3-none-any-null.whl`
      cause: Wheel variants are not supported yet: `ok-1.0.0-9-py3-none-any-null.whl`
    ");

    uv_snapshot!(
        context.filters(),
        context
            .pip_install()
            .arg("--preview-features")
            .arg("wheel-variants")
            .arg(wheel.path()),
        @"
    exit_code: 1 (failure)
    ----- stderr -----
    error: Failed to read `ok @ file://[TEMP_DIR]/links/ok-1.0.0-9-py3-none-any-null.whl`
      cause: Wheel variants are not supported yet: `ok-1.0.0-9-py3-none-any-null.whl`
    "
    );
    Ok(())
}

#[test]
fn ordinary_wheels_are_selected_over_variants() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let links = context.temp_dir.child("links");
    links.create_dir_all()?;
    fs_err::copy(
        context
            .workspace_root
            .join("test/links/ok-1.0.0-py3-none-any.whl"),
        links.child("ok-1.0.0-py3-none-any.whl"),
    )?;
    // A higher build tag and a newer version must not make a variant selectable.
    links
        .child("ok-1.0.0-9-py3-none-any-cpu.whl")
        .write_str("invalid wheel")?;
    links
        .child("ok-2.0.0-py3-none-any-null.whl")
        .write_str("invalid wheel")?;

    uv_snapshot!(
        context.filters(),
        context
            .pip_install()
            .arg("--no-index")
            .arg("--find-links")
            .arg(links.path())
            .arg("--preview-features")
            .arg("wheel-variants")
            .arg("ok"),
        @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + ok==1.0.0
    "
    );
    Ok(())
}

/// With `--no-binary`, ordinary wheels can still provide metadata without running the sdist's
/// backend. A variant wheel must not displace them from metadata selection.
#[test]
fn no_binary_metadata_ignores_variant_wheels() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let links = context.temp_dir.child("links");
    links.create_dir_all()?;
    fs_err::copy(
        context
            .workspace_root
            .join("test/links/ok-1.0.0-py3-none-any.whl"),
        links.child("ok-1.0.0-py3-none-any.whl"),
    )?;
    links
        .child("ok-1.0.0-9-py3-none-any-cpu.whl")
        .write_str("invalid wheel")?;
    let backend_marker = context.temp_dir.child("backend-executed");
    let sdist = generate_source_archive(
        &"ok".parse()?,
        &"1.0.0".parse()?,
        "",
        Some(backend_marker.path()),
    )?;
    links.child("ok-1.0.0.tar.gz").write_binary(&sdist)?;
    context
        .temp_dir
        .child("requirements.in")
        .write_str("ok==1.0.0")?;

    uv_snapshot!(context.filters(), context.pip_compile()
        .arg("requirements.in")
        .arg("--no-index")
        .arg("--find-links")
        .arg(links.path())
        .arg("--no-binary")
        .arg("ok"), @"
    exit_code: 0 (success)
    ----- stdout -----
    # This file was autogenerated by uv via the following command:
    #    uv pip compile --cache-dir [CACHE_DIR] requirements.in --no-index --no-binary ok
    ok==1.0.0
        # via -r requirements.in

    ----- stderr -----
    Resolved 1 package in [TIME]
    ");
    backend_marker.assert(predicate::path::missing());
    Ok(())
}

/// A variant wheel cannot provide metadata for an sdist, even when it is the only wheel.
#[tokio::test]
async fn sdist_metadata_ignores_variant_wheels() -> Result<()> {
    let server = MockServer::start().await;
    let context = uv_test::test_context!("3.12").with_filter((server.uri(), "http://[LOCALHOST]"));
    let sdist = generate_source_archive(&"ok".parse()?, &"1.0.0".parse()?, "", None)?;

    let simple_index = json!({
        "meta": { "api-version": "1.0" },
        "name": "ok",
        "files": [
            {
                "filename": "ok-1.0.0.tar.gz",
                "url": format!("{}/files/ok-1.0.0.tar.gz", server.uri()),
                "hashes": {},
            },
            {
                "filename": "ok-1.0.0-py3-none-any-cpu.whl",
                "url": format!("{}/files/ok-1.0.0-py3-none-any-cpu.whl", server.uri()),
                "hashes": {},
            },
        ],
    });
    Mock::given(method("GET"))
        .and(path("/simple/ok/"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            simple_index.to_string(),
            "application/vnd.pypi.simple.v1+json",
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/files/ok-1.0.0.tar.gz"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(sdist))
        .mount(&server)
        .await;

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--index-url")
        .arg(format!("{}/simple/", server.uri()))
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("ok==1.0.0"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + ok==1.0.0
    ");
    Ok(())
}

#[test]
fn universal_lock_excludes_variant_wheels() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let links = context.temp_dir.child("links");
    links.create_dir_all()?;
    fs_err::copy(
        context
            .workspace_root
            .join("test/links/ok-1.0.0-py3-none-any.whl"),
        links.child("ok-1.0.0-py3-none-any.whl"),
    )?;
    links
        .child("ok-1.0.0-9-py3-none-any-cpu.whl")
        .write_str("invalid wheel")?;
    links
        .child("ok-2.0.0-py3-none-any-null.whl")
        .write_str("invalid wheel")?;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "example"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = ["ok"]
    "#})?;

    uv_snapshot!(
        context.filters(),
        context
            .lock()
            .arg("--no-index")
            .arg("--find-links")
            .arg(links.path())
            .arg("--preview-features")
            .arg("wheel-variants"),
        @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    "
    );
    insta::with_settings!({filters => context.filters()}, {
        insta::assert_snapshot!(context.read("uv.lock"), @r#"
        version = 1
        revision = 5
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "example"
        version = "1.0.0"
        source = { virtual = "." }
        dependencies = [
            { name = "ok" },
        ]

        [package.metadata]
        requires-dist = [{ name = "ok" }]

        [[package]]
        name = "ok"
        version = "1.0.0"
        source = { registry = "[TEMP_DIR]/links" }
        wheels = [
            { path = "[TEMP_DIR]/links/ok-1.0.0-py3-none-any.whl" },
        ]
        "#);
    });

    // A lockfile containing a variant wheel must not make it installable.
    let lock = context.read("uv.lock").replace(
        "ok-1.0.0-py3-none-any.whl",
        "ok-1.0.0-9-py3-none-any-cpu.whl",
    );
    context.temp_dir.child("uv.lock").write_str(&lock)?;
    uv_snapshot!(context.filters(), context.sync()
        .arg("--frozen")
        .arg("--preview-features")
        .arg("wheel-variants"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Distribution `ok==1.0.0 @ registry+[TEMP_DIR]/links` can't be installed because it doesn't have a source distribution or wheel for the current platform
    ");
    Ok(())
}

#[tokio::test]
async fn registry_variant_wheels_are_not_selectable() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/simple/ok/"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"<a href="../../files/ok-2.0.0-py3-none-any-null.whl">ok-2.0.0-py3-none-any-null.whl</a>"#,
            "text/html",
        ))
        .mount(&server)
        .await;
    let context = uv_test::test_context!("3.12").with_filter((server.uri(), "http://[LOCALHOST]"));

    uv_snapshot!(
        context.filters(),
        context
            .pip_install()
            .arg("--index-url")
            .arg(format!("{}/simple/", server.uri()))
            .env_remove(EnvVars::UV_EXCLUDE_NEWER)
            .arg("--preview-features")
            .arg("wheel-variants")
            .arg("ok"),
        @"
    exit_code: 1 (failure)
    ----- stderr -----
    error: No solution found when resolving dependencies
      cause: Because ok==2.0.0 has only unsupported wheel variants and only ok==2.0.0 is available, we can conclude that all versions of ok cannot be used.
             And because you require ok, we can conclude that your requirements are unsatisfiable.
    "
    );
    uv_snapshot!(context.filters(), context.pip_install()
        .arg(context.workspace_root.join("test/links/ok-1.0.0-py3-none-any.whl")), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + ok==1.0.0 (from file://[WORKSPACE]/test/links/ok-1.0.0-py3-none-any.whl)
    ");
    uv_snapshot!(context.filters(), context.pip_list()
        .arg("--outdated")
        .arg("--format")
        .arg("json")
        .arg("--index-url")
        .arg(format!("{}/simple/", server.uri()))
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--preview-features")
        .arg("wheel-variants"), @"
    exit_code: 0 (success)
    ----- stdout -----
    []
    ");
    Ok(())
}
