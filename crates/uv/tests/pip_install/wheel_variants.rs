use anyhow::Result;
use assert_fs::prelude::*;
use indoc::{formatdoc, indoc};
use predicates::prelude::predicate;
use serde_json::json;
use sha2::{Digest, Sha256};
use uv_distribution_types::PEP_825_SCHEMA;
use uv_static::EnvVars;
use uv_test::archive::generate_source_archive;
use uv_test::uv_snapshot;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

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
    let no_variant_metadata = Mock::given(method("GET"))
        .and(path("/files/ok-1.0.0-variants.json"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount_as_scoped(&server)
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
    drop(no_variant_metadata);

    Mock::given(method("GET"))
        .and(path("/files/ok-1.0.0-variants.json"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({
                    "$schema": PEP_825_SCHEMA,
                    "default-priorities": { "namespace": ["x86_64"] },
                    "variants": { "null": {} },
                }))
                .insert_header("cache-control", "no-store"),
        )
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
        "$schema": PEP_825_SCHEMA,
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
            "$schema": PEP_825_SCHEMA,
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
    let fixtures = context.workspace_root.join("test/links");
    for wheel in ["ok-1.0.0-py3-none-any.whl", "ok-2.0.0-py3-none-any.whl"] {
        fs_err::copy(fixtures.join(wheel), links.child(wheel))?;
    }
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
fn variant_wheels_without_preview_or_index_metadata_are_rejected() -> Result<()> {
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
