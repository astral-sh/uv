use std::path::Path;

use anyhow::{Context as _, Result, anyhow};
use assert_fs::prelude::*;
use indoc::formatdoc;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use wiremock::matchers::{basic_auth, method, path};
use wiremock::{Match, Mock, MockServer, ResponseTemplate};

use uv_static::EnvVars;
use uv_test::{TestContext, uv_snapshot};

const WHEEL_FILENAME: &str = "basic_package-0.1.0-py3-none-any.whl";
const WHEEL_HASH: &str = "7b6229db79b5800e4e98a351b5628c1c8a944533a2d428aeeaa7275a30d4ea82";
const WHEEL_METADATA: &str = "Metadata-Version: 2.3\nName: basic-package\nVersion: 0.1.0\n";
const SOURCE_FILENAME: &str = "basic_package-0.1.0.tar.gz";

const SOURCE_ZIP_FILENAME: &str = "source_package-1.0.0.zip";

fn sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn fixture(context: &TestContext, filename: &str) -> Result<Vec<u8>> {
    fs_err::read(context.workspace_root.join("test/links").join(filename))
        .with_context(|| format!("failed to read package fixture `{filename}`"))
}

fn advertised_file(filename: &str, url: &str, hash: Option<&str>) -> Value {
    let hashes = hash.map_or_else(|| json!({}), |hash| json!({ "sha256": hash }));
    let mut file = json!({
        "filename": filename,
        "url": url,
        "hashes": hashes,
        "upload-time": "2024-03-24T00:00:00Z",
    });
    if Path::new(filename)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("whl"))
    {
        file["core-metadata"] = json!(true);
    }
    file
}

async fn mount_simple(server: &MockServer, package: &str, files: Vec<Value>, requests: u64) {
    Mock::given(method("GET"))
        .and(path(format!("/simple/{package}/")))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(
                json!({
                    "meta": { "api-version": "1.1" },
                    "name": package,
                    "files": files,
                })
                .to_string(),
                "application/vnd.pypi.simple.v1+json",
            ),
        )
        .expect(requests)
        .mount(server)
        .await;
}

async fn mount_metadata(server: &MockServer, filename: &str, metadata: &str, requests: u64) {
    Mock::given(method("GET"))
        .and(path(format!("/files/{filename}.metadata")))
        .respond_with(ResponseTemplate::new(200).set_body_string(metadata))
        .expect(requests)
        .mount(server)
        .await;
}

async fn mount_artifact(server: &MockServer, filename: &str, artifact: Vec<u8>, requests: u64) {
    Mock::given(method("GET"))
        .and(path(format!("/files/{filename}")))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(artifact))
        .expect(requests)
        .mount(server)
        .await;
}

async fn assert_no_requests(server: &MockServer, name: &str) -> Result<()> {
    let requests = server
        .received_requests()
        .await
        .ok_or_else(|| anyhow!("{name} should record requests"))?;
    assert!(
        requests.is_empty(),
        "{name} unexpectedly received requests: {requests:?}"
    );
    Ok(())
}

async fn assert_requested_once(server: &MockServer, expected: &str) -> Result<()> {
    let requests = server
        .received_requests()
        .await
        .ok_or_else(|| anyhow!("server should record requests"))?;
    let count = requests
        .iter()
        .filter(|request| request.url.path() == expected)
        .count();
    assert_eq!(
        count, 1,
        "expected exactly one request to `{expected}`, received: {requests:?}"
    );
    Ok(())
}

async fn assert_authenticated_requests(
    server: &MockServer,
    username: &str,
    password: &str,
) -> Result<()> {
    let requests = server
        .received_requests()
        .await
        .ok_or_else(|| anyhow!("server should record requests"))?;

    assert!(
        requests
            .iter()
            .all(|request| basic_auth(username, password).matches(request)),
        "requests did not use the expected credentials: {requests:?}"
    );
    Ok(())
}

fn write_frozen_lock(
    context: &TestContext,
    canonical_index: &MockServer,
    package: &str,
    version: &str,
    artifact: &str,
) -> Result<()> {
    let canonical_index = canonical_index.uri();

    context
        .temp_dir
        .child("uv.lock")
        .write_str(&formatdoc! {r#"
            version = 1
            revision = 3
            requires-python = ">=3.12"

            [[package]]
            name = "{package}"
            version = "{version}"
            source = {{ registry = "{canonical_index}/simple/" }}
            {artifact}

            [[package]]
            name = "project"
            version = "0.1.0"
            source = {{ virtual = "." }}
            dependencies = [
                {{ name = "{package}" }},
            ]

            [package.metadata]
            requires-dist = [{{ name = "{package}", specifier = "=={version}" }}]
            "#,
        })?;
    Ok(())
}

async fn assert_no_origin_requests(
    canonical_index: &MockServer,
    canonical_artifacts: &MockServer,
) -> Result<()> {
    assert_no_requests(canonical_index, "canonical Simple index").await?;
    assert_no_requests(canonical_artifacts, "canonical artifact origin").await
}

#[tokio::test]
#[cfg_attr(
    windows,
    ignore = "Configuration tests are not yet supported on Windows"
)]
async fn proxy_index_rejects_duplicate_definitions_across_configuration_files() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let physical_index = MockServer::start().await;
    let physical_artifacts = MockServer::start().await;
    let pyproject = context.temp_dir.child("pyproject.toml");
    pyproject.write_str(&formatdoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["basic-package==0.1.0"]

        [tool.uv]
        preview-features = ["proxy-index"]

        [[tool.uv.index]]
        name = "pypi"
        url = "https://pypi.org/simple"
        default = true

        [[tool.uv.proxy-index]]
        name = "socket"
        url = "{physical_index_url}/simple/"
        artifact-base-url = "{physical_artifact_url}/files/"
        proxy-for = "pypi"
    "#,
        physical_index_url = physical_index.uri(),
        physical_artifact_url = physical_artifacts.uri(),
    })?;

    let user_config_dir = context.user_config_dir.child("uv");
    user_config_dir.create_dir_all()?;
    user_config_dir
        .child("uv.toml")
        .write_str(indoc::indoc! {r#"
            [[proxy-index]]
            name = "socket"
            url = "https://other-user:other-secret@other-proxy.example.com/simple/"
            artifact-base-url = "https://other-proxy.example.com/files/"
            proxy-for = "pypi"
        "#})?;

    let physical_index_url = physical_index.uri();
    let mut filters = context.filters();
    filters.insert(0, (physical_index_url.as_str(), "[PHYSICAL_INDEX]"));
    uv_snapshot!(filters, context.lock(), @r"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Each index can have only one proxy, but both `[PHYSICAL_INDEX]/simple/` and `https://other-user:****@other-proxy.example.com/simple/` are proxies for `https://pypi.org/simple`
    ");

    assert_no_requests(&physical_index, "physical proxy index").await?;
    assert_no_requests(&physical_artifacts, "physical artifact origin").await?;
    Ok(())
}

#[tokio::test]
#[cfg_attr(
    windows,
    ignore = "Configuration tests are not yet supported on Windows"
)]
async fn proxy_index_rejects_same_named_package_index_across_configuration_files() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let physical_index = MockServer::start().await;
    let physical_artifacts = MockServer::start().await;
    let pyproject = context.temp_dir.child("pyproject.toml");
    pyproject.write_str(&formatdoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["basic-package==0.1.0"]

        [tool.uv]
        preview-features = ["proxy-index"]

        [[tool.uv.index]]
        name = "pypi"
        url = "https://pypi.org/simple"
        default = true

        [[tool.uv.proxy-index]]
        name = "socket"
        url = "{physical_index_url}/simple/"
        artifact-base-url = "{physical_artifact_url}/files/"
        proxy-for = "pypi"
    "#,
        physical_index_url = physical_index.uri(),
        physical_artifact_url = physical_artifacts.uri(),
    })?;

    let user_config_dir = context.user_config_dir.child("uv");
    user_config_dir.create_dir_all()?;
    user_config_dir
        .child("uv.toml")
        .write_str(indoc::indoc! {r#"
            [[index]]
            name = "socket"
            url = "https://ordinary-user:ordinary-secret@ordinary.example.com/simple/"
        "#})?;

    let physical_index_url = physical_index.uri();
    let mut filters = context.filters();
    filters.insert(0, (physical_index_url.as_str(), "[PHYSICAL_INDEX]"));
    uv_snapshot!(filters, context.lock(), @r"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Duplicate index name `socket`: proxy index `[PHYSICAL_INDEX]/simple/` conflicts with index `https://ordinary-user:****@ordinary.example.com/simple/`
    ");

    assert_no_requests(&physical_index, "physical proxy index").await?;
    assert_no_requests(&physical_artifacts, "physical artifact origin").await?;
    Ok(())
}

#[test]
fn proxy_index_rejects_url_shared_by_two_proxies() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let pyproject = context.temp_dir.child("pyproject.toml");
    pyproject.write_str(indoc::indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [[tool.uv.index]]
        name = "pypi"
        url = "https://pypi.org/simple"
        default = true

        [[tool.uv.index]]
        name = "private"
        url = "https://private.example.com/simple/"
        artifact-base-url = "https://private.example.com/files/"

        [[tool.uv.proxy-index]]
        name = "pypi-proxy"
        url = "https://proxy.example.com/simple"
        artifact-base-url = "https://proxy.example.com/files/"
        proxy-for = "pypi"
        cache-control = { api = "max-age=3600" }

        [[tool.uv.proxy-index]]
        name = "private-proxy"
        url = "https://user:secret@proxy.example.com/simple/"
        artifact-base-url = "https://proxy.example.com/files/"
        proxy-for = "private"
        cache-control = { api = "no-cache" }
    "#})?;

    uv_snapshot!(context.filters(), context.lock().arg("--offline"), @r"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Proxy index URL `https://user:****@proxy.example.com/simple/` is ambiguous: it is configured as a proxy for both `https://pypi.org/simple` and `https://private.example.com/simple/`
    ");

    Ok(())
}

#[test]
fn proxy_index_rejects_url_shared_with_ordinary_index() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let pyproject = context.temp_dir.child("pyproject.toml");
    pyproject.write_str(indoc::indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [[tool.uv.index]]
        name = "pypi"
        url = "https://pypi.org/simple"
        default = true

        [[tool.uv.index]]
        name = "ordinary"
        url = "https://ordinary-user:ordinary-secret@proxy.example.com/simple"

        [[tool.uv.proxy-index]]
        name = "proxy"
        url = "https://proxy-user:proxy-secret@proxy.example.com/simple/"
        artifact-base-url = "https://proxy.example.com/files/"
        proxy-for = "pypi"
    "#})?;

    uv_snapshot!(context.filters(), context.lock().arg("--offline"), @r"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Proxy index `https://proxy-user:****@proxy.example.com/simple/` cannot use the same URL as ordinary index `https://ordinary-user:****@proxy.example.com/simple`
    ");

    Ok(())
}

#[test]
fn proxy_index_rejects_named_proxy_in_project_sources() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let pyproject = context.temp_dir.child("pyproject.toml");
    pyproject.write_str(indoc::indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["basic-package"]

        [tool.uv]
        preview-features = ["proxy-index"]

        [tool.uv.sources]
        basic-package = { index = "mirror" }

        [[tool.uv.index]]
        name = "upstream"
        url = "https://upstream.example.com/simple/"
        artifact-base-url = "https://upstream.example.com/files/"

        [[tool.uv.proxy-index]]
        name = "mirror"
        url = "https://mirror.example.com/simple/"
        artifact-base-url = "https://mirror.example.com/files/"
        proxy-for = "upstream"
    "#})?;

    uv_snapshot!(context.filters(), context.lock().arg("--offline"), @"
    exit_code: 1 (failure)
    ----- stderr -----
    error: Failed to build `project @ file://[TEMP_DIR]/`
      cause: Failed to parse entry: `basic-package`
      cause: Package `basic-package` references proxy index `mirror` in `tool.uv.sources`. Use its upstream index `upstream` instead
    ");

    Ok(())
}

#[tokio::test]
async fn proxy_index_routes_explicit_pypi_without_a_second_resolution_index() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let wheel = fixture(&context, WHEEL_FILENAME)?;
    let source = fixture(&context, SOURCE_FILENAME)?;
    let physical_index = MockServer::start().await;
    let physical_artifacts = MockServer::start().await;

    mount_simple(
        &physical_index,
        "basic-package",
        vec![
            advertised_file(
                WHEEL_FILENAME,
                &format!("{}/files/{WHEEL_FILENAME}", physical_artifacts.uri()),
                Some(WHEEL_HASH),
            ),
            advertised_file(
                SOURCE_FILENAME,
                &format!("{}/files/{SOURCE_FILENAME}", physical_artifacts.uri()),
                Some(&sha256(&source)),
            ),
        ],
        1,
    )
    .await;
    mount_metadata(&physical_artifacts, WHEEL_FILENAME, WHEEL_METADATA, 1).await;
    mount_artifact(&physical_artifacts, WHEEL_FILENAME, wheel, 1).await;
    let pyproject = context.temp_dir.child("pyproject.toml");
    pyproject.write_str(&formatdoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["basic-package==0.1.0"]

        [tool.uv]
        preview-features = ["proxy-index"]

        [[tool.uv.index]]
        name = "pypi"
        url = "https://pypi.org/simple"
        default = true

        [[tool.uv.proxy-index]]
        name = "socket"
        url = "{physical_index_url}/simple/"
        artifact-base-url = "{physical_artifact_url}/files/"
        proxy-for = "pypi"
    "#,
        physical_index_url = physical_index.uri(),
        physical_artifact_url = physical_artifacts.uri(),
    })?;

    uv_snapshot!(context.filters(), context.lock().arg("--no-preview"), @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: Proxy indexes are experimental and may change without warning. Pass `--preview-features proxy-index` to disable this warning.
    Resolved 2 packages in [TIME]
    ");

    insta::with_settings!({
        filters => [
            (physical_index.uri().as_str(), "[PHYSICAL_INDEX]"),
            (physical_artifacts.uri().as_str(), "[PHYSICAL_ARTIFACTS]"),
        ],
    }, {
        insta::assert_snapshot!(context.read("uv.lock"), @r#"
        version = 1
        revision = 5
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "basic-package"
        version = "0.1.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/basic_package-0.1.0.tar.gz", hash = "sha256:af478ff91ec60856c99a540b8df13d756513bebb65bc301fb27e0d1f974532b4", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/basic_package-0.1.0-py3-none-any.whl", hash = "sha256:7b6229db79b5800e4e98a351b5628c1c8a944533a2d428aeeaa7275a30d4ea82", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "basic-package" },
        ]

        [package.metadata]
        requires-dist = [{ name = "basic-package", specifier = "==0.1.0" }]
        "#);
    });

    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + basic-package==0.1.0
    ");

    assert_requested_once(&physical_index, "/simple/basic-package/").await?;
    assert_requested_once(&physical_artifacts, &format!("/files/{WHEEL_FILENAME}")).await?;
    Ok(())
}

#[test]
fn proxy_index_requires_explicit_upstream() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let pyproject = context.temp_dir.child("pyproject.toml");
    pyproject.write_str(indoc::indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [[tool.uv.proxy-index]]
        name = "proxy"
        url = "https://proxy.example.com/simple/"
        artifact-base-url = "https://proxy.example.com/files/"
        proxy-for = "pypi"
    "#})?;

    uv_snapshot!(context.filters(), context.lock().arg("--offline"), @r"
    exit_code: 2 (failure)
    ----- stderr -----
    error: The proxy index references package index `pypi`, which was not found in your configuration
    ");

    Ok(())
}

/// The proxy's cache policy overrides the cache header on PEP 658 metadata.
#[tokio::test]
async fn proxy_index_sidecar_uses_proxy_cache_policy() -> Result<()> {
    let context = uv_test::test_context!("3.13");
    let server = MockServer::start().await;
    let server_url = server.uri();

    let pyproject = context.temp_dir.child("pyproject.toml");
    pyproject.write_str(&formatdoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.13"
        dependencies = ["basic-package==0.1.0"]

        [tool.uv]
        preview-features = ["proxy-index"]

        [[tool.uv.index]]
        name = "pypi"
        url = "https://pypi.org/simple"
        default = true

        [[tool.uv.proxy-index]]
        name = "proxy"
        url = "{server_url}/simple/"
        artifact-base-url = "{server_url}/files/"
        proxy-for = "pypi"
        cache-control = {{ api = "max-age=3600", files = "max-age=3600" }}
    "#})?;
    let wheel = advertised_file(
        WHEEL_FILENAME,
        &format!("{server_url}/files/{WHEEL_FILENAME}"),
        Some(WHEEL_HASH),
    );
    mount_simple(&server, "basic-package", vec![wheel], 1).await;
    Mock::given(method("GET"))
        .and(path(format!("/files/{WHEEL_FILENAME}.metadata")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(WHEEL_METADATA)
                .insert_header("cache-control", "no-store"),
        )
        .expect(1)
        .mount(&server)
        .await;

    uv_snapshot!(context.filters(), context.lock(), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    // Resolve again without reusing the lockfile; the proxy's policy allows cached metadata.
    fs_err::remove_file(context.temp_dir.join("uv.lock"))?;
    uv_snapshot!(context.filters(), context.lock(), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    server.verify().await;
    Ok(())
}

/// The proxy's cache policy applies to variant metadata on another host.
#[tokio::test]
async fn proxy_index_variant_metadata_uses_advertised_url_and_proxy_cache_policy() -> Result<()> {
    let context = uv_test::test_context!("3.13");
    let server = MockServer::start().await;
    let metadata_server = MockServer::start().await;
    let server_url = server.uri();
    context
        .temp_dir
        .child("requirements.in")
        .write_str("basic-package==0.1.0")?;
    context
        .temp_dir
        .child("uv.toml")
        .write_str(&formatdoc! {r#"
        preview-features = ["proxy-index", "wheel-variants"]

        [[index]]
        name = "pypi"
        url = "https://pypi.org/simple"
        default = true

        [[proxy-index]]
        name = "proxy"
        url = "{server_url}/simple/"
        artifact-base-url = "{server_url}/files/"
        proxy-for = "pypi"
        cache-control = {{ api = "max-age=3600", files = "max-age=3600" }}
    "#})?;
    mount_simple(
        &server,
        "basic-package",
        vec![
            advertised_file(
                WHEEL_FILENAME,
                &format!("{server_url}/files/{WHEEL_FILENAME}"),
                Some(WHEEL_HASH),
            ),
            json!({
                "filename": "basic_package-0.1.0-variants.json",
                "url": format!("{}/metadata/basic_package-0.1.0-variants.json", metadata_server.uri()),
                "hashes": {},
            }),
        ],
        1,
    )
    .await;
    mount_metadata(&server, WHEEL_FILENAME, WHEEL_METADATA, 1).await;
    Mock::given(method("GET"))
        .and(path("/metadata/basic_package-0.1.0-variants.json"))
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
        .mount(&metadata_server)
        .await;

    uv_snapshot!(context.filters(), context.pip_compile()
        .arg("requirements.in")
        .arg("--no-header")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER), @"
    exit_code: 0 (success)
    ----- stdout -----
    basic-package==0.1.0
        # via -r requirements.in

    ----- stderr -----
    Resolved 1 package in [TIME]
    ");

    uv_snapshot!(context.filters(), context.pip_compile()
        .arg("requirements.in")
        .arg("--no-header")
        .arg("--offline")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER), @"
    exit_code: 0 (success)
    ----- stdout -----
    basic-package==0.1.0
        # via -r requirements.in

    ----- stderr -----
    Resolved 1 package in [TIME]
    ");

    server.verify().await;
    metadata_server.verify().await;
    Ok(())
}

#[tokio::test]
async fn proxy_index_locks_canonical_urls_without_origin_requests() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let wheel = fixture(&context, WHEEL_FILENAME)?;
    let source = fixture(&context, SOURCE_FILENAME)?;
    let canonical_index = MockServer::start().await;
    let canonical_artifacts = MockServer::start().await;
    let physical_index = MockServer::start().await;
    let physical_artifacts = MockServer::start().await;

    mount_simple(
        &physical_index,
        "basic-package",
        vec![
            advertised_file(
                WHEEL_FILENAME,
                &format!("{}/files/{WHEEL_FILENAME}", physical_artifacts.uri()),
                Some(WHEEL_HASH),
            ),
            advertised_file(
                SOURCE_FILENAME,
                &format!("{}/files/{SOURCE_FILENAME}", physical_artifacts.uri()),
                Some(&sha256(&source)),
            ),
        ],
        1,
    )
    .await;
    mount_metadata(&physical_artifacts, WHEEL_FILENAME, WHEEL_METADATA, 1).await;
    mount_artifact(&physical_artifacts, WHEEL_FILENAME, wheel, 1).await;
    let pyproject = context.temp_dir.child("pyproject.toml");
    pyproject.write_str(&formatdoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["basic-package==0.1.0"]

        [tool.uv]
        preview-features = ["proxy-index"]

        [[tool.uv.index]]
        name = "canonical"
        url = "{canonical_index_url}/simple/"
        artifact-base-url = "{canonical_artifact_url}/packages/"
        default = true

        [[tool.uv.proxy-index]]
        name = "socket"
        url = "{physical_index_url}/simple/"
        artifact-base-url = "{physical_artifact_url}/files/"
        proxy-for = "canonical"
    "#,
        canonical_index_url = canonical_index.uri(),
        canonical_artifact_url = canonical_artifacts.uri(),
        physical_index_url = format!("http://proxy-user:proxy-password@{}", physical_index.address()),
        physical_artifact_url = format!("http://artifact-user:artifact-password@{}", physical_artifacts.address()),
    })?;

    uv_snapshot!(context.filters(), context.lock(), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    insta::with_settings!({
        filters => [
            (canonical_index.uri().as_str(), "[UPSTREAM_INDEX]"),
            (canonical_artifacts.uri().as_str(), "[UPSTREAM_ARTIFACTS]"),
            (physical_index.uri().as_str(), "[PHYSICAL_INDEX]"),
            (physical_artifacts.uri().as_str(), "[PHYSICAL_ARTIFACTS]"),
        ],
    }, {
        insta::assert_snapshot!(context.read("uv.lock"), @r#"
        version = 1
        revision = 5
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "basic-package"
        version = "0.1.0"
        source = { registry = "[UPSTREAM_INDEX]/simple/" }
        sdist = { url = "[UPSTREAM_ARTIFACTS]/packages/basic_package-0.1.0.tar.gz", hash = "sha256:af478ff91ec60856c99a540b8df13d756513bebb65bc301fb27e0d1f974532b4", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "[UPSTREAM_ARTIFACTS]/packages/basic_package-0.1.0-py3-none-any.whl", hash = "sha256:7b6229db79b5800e4e98a351b5628c1c8a944533a2d428aeeaa7275a30d4ea82", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "basic-package" },
        ]

        [package.metadata]
        requires-dist = [{ name = "basic-package", specifier = "==0.1.0" }]
        "#);
    });

    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + basic-package==0.1.0
    ");

    assert_requested_once(&physical_index, "/simple/basic-package/").await?;
    assert_requested_once(&physical_artifacts, &format!("/files/{WHEEL_FILENAME}")).await?;
    assert_authenticated_requests(&physical_index, "proxy-user", "proxy-password").await?;
    assert_authenticated_requests(&physical_artifacts, "artifact-user", "artifact-password")
        .await?;
    assert_no_origin_requests(&canonical_index, &canonical_artifacts).await?;
    Ok(())
}

#[tokio::test]
async fn proxy_index_reports_unauthorized_proxy_without_origin_requests() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let canonical_index = MockServer::start().await;
    let canonical_artifacts = MockServer::start().await;
    let physical_index = MockServer::start().await;
    let physical_artifacts = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/simple/basic-package/"))
        .respond_with(ResponseTemplate::new(401))
        .expect(1)
        .mount(&physical_index)
        .await;
    let pyproject = context.temp_dir.child("pyproject.toml");
    pyproject.write_str(&formatdoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["basic-package==0.1.0"]

        [tool.uv]
        preview-features = ["proxy-index"]

        [[tool.uv.index]]
        name = "canonical"
        url = "{canonical_index_url}/simple/"
        artifact-base-url = "{canonical_artifact_url}/packages/"
        default = true

        [[tool.uv.proxy-index]]
        name = "socket"
        url = "{physical_index_url}/simple/"
        artifact-base-url = "{physical_artifact_url}/files/"
        proxy-for = "canonical"
    "#,
        canonical_index_url = canonical_index.uri(),
        canonical_artifact_url = canonical_artifacts.uri(),
        physical_index_url = physical_index.uri(),
        physical_artifact_url = physical_artifacts.uri(),
    })?;

    let physical_index_url = physical_index.uri();
    let mut filters = context.filters();
    filters.insert(0, (physical_index_url.as_str(), "[PHYSICAL_INDEX]"));

    let command = context.lock();
    uv_snapshot!(filters, command, @r"
    exit_code: 1 (failure)
    ----- stderr -----
    error: No solution found when resolving dependencies
      cause: Because basic-package was not found in the package registry and your project depends on basic-package==0.1.0, we can conclude that your project's requirements are unsatisfiable.

    hint: An index URL ([PHYSICAL_INDEX]/simple/) could not be queried due to a lack of valid authentication credentials (401 Unauthorized)
    ");

    assert!(!context.temp_dir.child("uv.lock").exists());
    assert_requested_once(&physical_index, "/simple/basic-package/").await?;
    assert_no_requests(&physical_artifacts, "physical artifact origin").await?;
    assert_no_origin_requests(&canonical_index, &canonical_artifacts).await?;
    Ok(())
}

#[tokio::test]
async fn proxy_index_reports_forbidden_proxy_without_origin_requests() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let canonical_index = MockServer::start().await;
    let canonical_artifacts = MockServer::start().await;
    let physical_index = MockServer::start().await;
    let physical_artifacts = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/simple/basic-package/"))
        .respond_with(ResponseTemplate::new(403))
        .expect(1)
        .mount(&physical_index)
        .await;
    let pyproject = context.temp_dir.child("pyproject.toml");
    pyproject.write_str(&formatdoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["basic-package==0.1.0"]

        [tool.uv]
        preview-features = ["proxy-index"]

        [[tool.uv.index]]
        name = "canonical"
        url = "{canonical_index_url}/simple/"
        artifact-base-url = "{canonical_artifact_url}/packages/"
        default = true

        [[tool.uv.proxy-index]]
        name = "socket"
        url = "{physical_index_url}/simple/"
        artifact-base-url = "{physical_artifact_url}/files/"
        proxy-for = "canonical"
    "#,
        canonical_index_url = canonical_index.uri(),
        canonical_artifact_url = canonical_artifacts.uri(),
        physical_index_url = physical_index.uri(),
        physical_artifact_url = physical_artifacts.uri(),
    })?;

    let physical_index_url = physical_index.uri();
    let mut filters = context.filters();
    filters.insert(0, (physical_index_url.as_str(), "[PHYSICAL_INDEX]"));

    let command = context.lock();
    uv_snapshot!(filters, command, @r"
    exit_code: 1 (failure)
    ----- stderr -----
    error: No solution found when resolving dependencies
      cause: Because basic-package was not found in the package registry and your project depends on basic-package==0.1.0, we can conclude that your project's requirements are unsatisfiable.

    hint: An index ([PHYSICAL_INDEX]/simple/) returned a 403 Forbidden error. Check that the index URL is correct and the credentials are valid.
    ");

    assert!(!context.temp_dir.child("uv.lock").exists());
    assert_requested_once(&physical_index, "/simple/basic-package/").await?;
    assert_no_requests(&physical_artifacts, "physical artifact origin").await?;
    assert_no_origin_requests(&canonical_index, &canonical_artifacts).await?;
    Ok(())
}

#[tokio::test]
async fn proxy_index_routes_source_through_proxy_and_reuses_offline_cache() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let filename = "source_package-1.0.0.zip";
    let source = fixture(&context, filename)?;
    let source_hash = sha256(&source);
    let canonical_index = MockServer::start().await;
    let canonical_artifacts = MockServer::start().await;
    let physical_index = MockServer::start().await;
    let physical_artifacts = MockServer::start().await;

    mount_simple(
        &physical_index,
        "source-package",
        vec![
            advertised_file(
                "source_package-1.0.0-cp39-cp39-win_amd64.whl",
                &format!(
                    "{}/packages/source_package-1.0.0-cp39-cp39-win_amd64.whl",
                    canonical_artifacts.uri()
                ),
                Some(&source_hash),
            ),
            advertised_file(
                filename,
                &format!("{}/files/{filename}", physical_artifacts.uri()),
                Some(&source_hash),
            ),
        ],
        1,
    )
    .await;
    mount_artifact(&physical_artifacts, filename, source, 1).await;
    let pyproject = context.temp_dir.child("pyproject.toml");
    pyproject.write_str(&formatdoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["source-package==1.0.0"]

        [tool.uv]
        preview-features = ["proxy-index"]

        [[tool.uv.index]]
        name = "canonical"
        url = "{canonical_index_url}/simple/"
        artifact-base-url = "{canonical_artifact_url}/packages/"
        default = true

        [[tool.uv.proxy-index]]
        name = "socket"
        url = "{physical_index_url}/simple/"
        artifact-base-url = "{physical_artifact_url}/files/"
        proxy-for = "canonical"

        [[tool.uv.dependency-metadata]]
        name = "source-package"
        version = "1.0.0"
        requires-dist = []
    "#,
        canonical_index_url = canonical_index.uri(),
        canonical_artifact_url = canonical_artifacts.uri(),
        physical_index_url = physical_index.uri(),
        physical_artifact_url = physical_artifacts.uri(),
    })?;

    uv_snapshot!(context.filters(), context.lock(), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let original_lock = context.read("uv.lock");
    assert!(
        original_lock.contains(&format!(
            "{}/packages/{filename}",
            canonical_artifacts.uri()
        )),
        "the source lock must preserve its canonical artifact base: {original_lock}",
    );
    assert!(!original_lock.contains(&physical_index.uri()));
    assert!(!original_lock.contains(&physical_artifacts.uri()));

    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + source-package==1.0.0
    ");

    uv_snapshot!(context.filters(), context
        .sync()
        .arg("--offline")
        .arg("--frozen")
        .arg("--reinstall"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     ~ source-package==1.0.0
    ");

    assert_eq!(context.read("uv.lock"), original_lock);
    assert_requested_once(&physical_index, "/simple/source-package/").await?;
    assert_requested_once(&physical_artifacts, &format!("/files/{filename}")).await?;
    assert_no_origin_requests(&canonical_index, &canonical_artifacts).await?;
    Ok(())
}

#[tokio::test]
async fn proxy_index_installs_existing_locked_source_without_simple_requests() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let artifact = fixture(&context, SOURCE_ZIP_FILENAME)?;
    let artifact_hash = sha256(&artifact);
    let canonical_index = MockServer::start().await;
    let canonical_artifacts = MockServer::start().await;
    let physical_index = MockServer::start().await;
    let physical_artifacts = MockServer::start().await;

    let dependency = "source-package==1.0.0";
    let pyproject = context.temp_dir.child("pyproject.toml");
    pyproject.write_str(&formatdoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["{dependency}"]

        [tool.uv]
        preview-features = ["proxy-index"]

        [[tool.uv.index]]
        name = "canonical"
        url = "{canonical_index_url}/simple/"
        artifact-base-url = "{canonical_artifact_url}/packages/"
        default = true

        [[tool.uv.proxy-index]]
        name = "socket"
        url = "{physical_index_url}/simple/"
        artifact-base-url = "{physical_artifact_url}/files/"
        proxy-for = "canonical"
    "#,
        canonical_index_url = canonical_index.uri(),
        canonical_artifact_url = canonical_artifacts.uri(),
        physical_index_url = physical_index.uri(),
        physical_artifact_url = physical_artifacts.uri(),
    })?;
    write_frozen_lock(
        &context,
        &canonical_index,
        "source-package",
        "1.0.0",
        &format!(
            r#"sdist = {{ url = "{}/packages/{SOURCE_ZIP_FILENAME}", hash = "sha256:{artifact_hash}" }}"#,
            canonical_artifacts.uri()
        ),
    )?;
    let original_lock = context.read("uv.lock");
    mount_artifact(&physical_artifacts, SOURCE_ZIP_FILENAME, artifact, 1).await;

    let mut command = context.sync();
    command.arg("--frozen");
    uv_snapshot!(context.filters(), command, @r"
    exit_code: 0 (success)
    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + source-package==1.0.0
    ");

    assert_eq!(context.read("uv.lock"), original_lock);
    assert_requested_once(
        &physical_artifacts,
        &format!("/files/{SOURCE_ZIP_FILENAME}"),
    )
    .await?;
    assert_no_requests(&physical_index, "physical Simple index").await?;
    assert_no_origin_requests(&canonical_index, &canonical_artifacts).await?;
    Ok(())
}

#[tokio::test]
async fn proxy_index_redownloads_artifacts_when_proxy_changes() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let wheel = fixture(&context, WHEEL_FILENAME)?;
    let canonical_index = MockServer::start().await;
    let canonical_artifacts = MockServer::start().await;
    let physical_index_a = MockServer::start().await;
    let physical_artifacts_a = MockServer::start().await;
    let physical_index_b = MockServer::start().await;
    let physical_artifacts_b = MockServer::start().await;

    let pyproject = context.temp_dir.child("pyproject.toml");
    let write_pyproject = |index: &MockServer, artifacts: &MockServer| {
        pyproject.write_str(&formatdoc! {r#"
            [project]
            name = "project"
            version = "0.1.0"
            requires-python = ">=3.12"
            dependencies = ["basic-package==0.1.0"]

            [tool.uv]
            preview-features = ["proxy-index"]

            [[tool.uv.index]]
            name = "canonical"
            url = "{canonical_index_url}/simple/"
            artifact-base-url = "{canonical_artifact_url}/packages/"
            default = true

            [[tool.uv.proxy-index]]
            name = "socket"
            url = "{physical_index_url}/simple/"
            artifact-base-url = "{physical_artifact_url}/files/"
            proxy-for = "canonical"
        "#,
            canonical_index_url = canonical_index.uri(),
            canonical_artifact_url = canonical_artifacts.uri(),
            physical_index_url = index.uri(),
            physical_artifact_url = artifacts.uri(),
        })
    };
    write_pyproject(&physical_index_a, &physical_artifacts_a)?;
    write_frozen_lock(
        &context,
        &canonical_index,
        "basic-package",
        "0.1.0",
        &format!(
            r#"wheels = [{{ url = "{}/packages/{WHEEL_FILENAME}", hash = "sha256:{WHEEL_HASH}" }}]"#,
            canonical_artifacts.uri()
        ),
    )?;
    let original_lock = context.read("uv.lock");
    mount_artifact(&physical_artifacts_a, WHEEL_FILENAME, wheel.clone(), 1).await;

    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + basic-package==0.1.0
    ");

    write_pyproject(&physical_index_b, &physical_artifacts_b)?;
    mount_artifact(&physical_artifacts_b, WHEEL_FILENAME, wheel, 1).await;

    // Recreate the environment while keeping the first proxy's artifacts in the cache.
    context.reset_venv();
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + basic-package==0.1.0
    ");

    assert_eq!(context.read("uv.lock"), original_lock);
    assert_requested_once(&physical_artifacts_a, &format!("/files/{WHEEL_FILENAME}")).await?;
    assert_requested_once(&physical_artifacts_b, &format!("/files/{WHEEL_FILENAME}")).await?;
    assert_no_requests(&physical_index_a, "original physical Simple index").await?;
    assert_no_requests(&physical_index_b, "replacement physical Simple index").await?;
    assert_no_origin_requests(&canonical_index, &canonical_artifacts).await?;
    Ok(())
}

/// A lock created without the proxy can contain a hashless artifact that export must reject.
#[tokio::test]
async fn proxy_index_export_rejects_missing_hash_in_frozen_lock() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let upstream = MockServer::start().await;
    let proxy = MockServer::start().await;

    let pyproject = context.temp_dir.child("pyproject.toml");
    pyproject.write_str(&formatdoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["basic-package==0.1.0"]

        [tool.uv]
        preview-features = ["proxy-index"]

        [[tool.uv.index]]
        name = "canonical"
        url = "{canonical_index_url}/simple/"
        artifact-base-url = "{canonical_artifact_url}/packages/"
        default = true

        [[tool.uv.proxy-index]]
        name = "socket"
        url = "{physical_index_url}/simple/"
        artifact-base-url = "{physical_artifact_url}/files/"
        proxy-for = "canonical"
    "#,
        canonical_index_url = upstream.uri(),
        canonical_artifact_url = upstream.uri(),
        physical_index_url = proxy.uri(),
        physical_artifact_url = proxy.uri(),
    })?;
    write_frozen_lock(
        &context,
        &upstream,
        "basic-package",
        "0.1.0",
        &format!(
            r#"sdist = {{ url = "{}/packages/{SOURCE_FILENAME}" }}"#,
            upstream.uri()
        ),
    )?;

    uv_snapshot!(context.filters(), context.export().args(["--frozen", "--format", "pylock.toml"]), @r"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Cannot lock `basic_package-0.1.0.tar.gz` for `basic-package` from proxy index `http://[LOCALHOST]/simple/` because it has no supported hash
    ");

    assert_no_requests(&upstream, "upstream index").await?;
    assert_no_requests(&proxy, "proxy index").await?;
    Ok(())
}

#[tokio::test]
async fn proxy_index_pip_compile_pylock_preserves_canonical_artifact_and_hash() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let artifact = fixture(&context, WHEEL_FILENAME)?;
    let canonical_index = MockServer::start().await;
    let canonical_artifacts = MockServer::start().await;
    let physical_index = MockServer::start().await;
    let physical_artifacts = MockServer::start().await;

    mount_simple(
        &physical_index,
        "basic-package",
        vec![advertised_file(
            WHEEL_FILENAME,
            &format!("{}/files/{WHEEL_FILENAME}", physical_artifacts.uri()),
            Some(WHEEL_HASH),
        )],
        1,
    )
    .await;
    let pyproject = context.temp_dir.child("pyproject.toml");
    pyproject.write_str(&formatdoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["basic-package==0.1.0"]

        [tool.uv]
        preview-features = ["proxy-index"]

        [[tool.uv.index]]
        name = "canonical"
        url = "{canonical_index_url}/simple/"
        artifact-base-url = "{canonical_artifact_url}/packages/"
        default = true

        [[tool.uv.proxy-index]]
        name = "socket"
        url = "{physical_index_url}/simple/"
        artifact-base-url = "{physical_artifact_url}/files/"
        proxy-for = "canonical"

        [[tool.uv.dependency-metadata]]
        name = "basic-package"
        version = "0.1.0"
        requires-dist = []
    "#,
        canonical_index_url = format!("http://canonical-user:canonical-password@{}", canonical_index.address()),
        canonical_artifact_url = canonical_artifacts.uri(),
        physical_index_url = physical_index.uri(),
        physical_artifact_url = physical_artifacts.uri(),
    })?;
    context
        .temp_dir
        .child("requirements.txt")
        .write_str("basic-package==0.1.0")?;

    let canonical_index_url = canonical_index.uri();
    let canonical_artifact_url = canonical_artifacts.uri();
    let mut filters = context.filters();
    filters.insert(0, (&canonical_index_url, "[UPSTREAM_INDEX]"));
    filters.insert(0, (&canonical_artifact_url, "[UPSTREAM_ARTIFACTS]"));
    let output = uv_snapshot!(filters, context
        .pip_compile()
        .arg("requirements.txt")
        .arg("--format")
        .arg("pylock.toml")
        .arg("--no-header")
        .arg("-o")
        .arg("pylock.toml"), @r#"
    exit_code: 0 (success)
    ----- stdout -----
    lock-version = "1.0"
    created-by = "uv"
    requires-python = ">=3.12.[X]"

    [[packages]]
    name = "basic-package"
    version = "0.1.0"
    index = "[UPSTREAM_INDEX]/simple/"
    wheels = [{ url = "[UPSTREAM_ARTIFACTS]/packages/basic_package-0.1.0-py3-none-any.whl", upload-time = 2024-03-24T00:00:00Z, hashes = { sha256 = "7b6229db79b5800e4e98a351b5628c1c8a944533a2d428aeeaa7275a30d4ea82" } }]

    ----- stderr -----
    Resolved 1 package in [TIME]
    "#);

    let lock = context.read("pylock.toml");
    assert_eq!(lock.as_bytes(), output.stdout);
    assert!(!lock.contains(&physical_index.uri()));
    assert!(!lock.contains(&physical_artifacts.uri()));
    assert_no_requests(&physical_artifacts, "physical artifact origin").await?;

    mount_artifact(&physical_artifacts, WHEEL_FILENAME, artifact, 1).await;
    let context = context.with_cache_dir("install-cache");
    uv_snapshot!(context.filters(), context
        .pip_sync()
        .arg("--preview-features")
        .arg("pylock,proxy-index")
        .arg("pylock.toml"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + basic-package==0.1.0
    ");

    assert_eq!(context.read("pylock.toml"), lock);
    assert_requested_once(&physical_index, "/simple/basic-package/").await?;
    assert_requested_once(&physical_artifacts, &format!("/files/{WHEEL_FILENAME}")).await?;
    assert_no_origin_requests(&canonical_index, &canonical_artifacts).await?;
    Ok(())
}

#[tokio::test]
async fn proxy_index_pip_installs_hashless_live_artifact() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let wheel = fixture(&context, WHEEL_FILENAME)?;
    let canonical_index = MockServer::start().await;
    let canonical_artifacts = MockServer::start().await;
    let physical_index = MockServer::start().await;
    let physical_artifacts = MockServer::start().await;

    mount_simple(
        &physical_index,
        "basic-package",
        vec![advertised_file(
            WHEEL_FILENAME,
            &format!("{}/files/{WHEEL_FILENAME}", physical_artifacts.uri()),
            None,
        )],
        1,
    )
    .await;
    mount_artifact(&physical_artifacts, WHEEL_FILENAME, wheel, 1).await;
    let pyproject = context.temp_dir.child("pyproject.toml");
    pyproject.write_str(&formatdoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["basic-package==0.1.0"]

        [tool.uv]
        preview-features = ["proxy-index"]

        [[tool.uv.index]]
        name = "canonical"
        url = "{canonical_index_url}/simple/"
        artifact-base-url = "{canonical_artifact_url}/packages/"
        default = true

        [[tool.uv.proxy-index]]
        name = "socket"
        url = "{physical_index_url}/simple/"
        artifact-base-url = "{physical_artifact_url}/files/"
        proxy-for = "canonical"
    "#,
        canonical_index_url = canonical_index.uri(),
        canonical_artifact_url = canonical_artifacts.uri(),
        physical_index_url = physical_index.uri(),
        physical_artifact_url = physical_artifacts.uri(),
    })?;
    context
        .temp_dir
        .child("requirements.txt")
        .write_str("basic-package==0.1.0")?;

    uv_snapshot!(context.filters(), context.pip_sync().arg("requirements.txt"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + basic-package==0.1.0
    ");

    assert_requested_once(&physical_index, "/simple/basic-package/").await?;
    assert_requested_once(&physical_artifacts, &format!("/files/{WHEEL_FILENAME}")).await?;
    assert_no_origin_requests(&canonical_index, &canonical_artifacts).await?;
    Ok(())
}

#[tokio::test]
async fn proxy_index_rejects_cached_wheel_when_hash_changes() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let artifact = fixture(&context, WHEEL_FILENAME)?;
    let actual_hash = sha256(&artifact);
    let changed_hash = "0000000000000000000000000000000000000000000000000000000000000000";
    let canonical_index = MockServer::start().await;
    let canonical_artifacts = MockServer::start().await;
    let physical_index = MockServer::start().await;
    let physical_artifacts = MockServer::start().await;
    let dependency = "basic-package==0.1.0";
    let artifact_url = format!("{}/files/{WHEEL_FILENAME}", physical_artifacts.uri());

    let pyproject = context.temp_dir.child("pyproject.toml");
    pyproject.write_str(&formatdoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["{dependency}"]

        [tool.uv]
        preview-features = ["proxy-index"]

        [[tool.uv.index]]
        name = "canonical"
        url = "{canonical_index_url}/simple/"
        artifact-base-url = "{canonical_artifact_url}/packages/"
        default = true

        [[tool.uv.proxy-index]]
        name = "socket"
        url = "{physical_index_url}/simple/"
        artifact-base-url = "{physical_artifact_url}/files/"
        proxy-for = "canonical"

        [[tool.uv.dependency-metadata]]
        name = "basic-package"
        version = "0.1.0"
        requires-dist = []
    "#,
        canonical_index_url = canonical_index.uri(),
        canonical_artifact_url = canonical_artifacts.uri(),
        physical_index_url = physical_index.uri(),
        physical_artifact_url = physical_artifacts.uri(),
    })?;
    context
        .temp_dir
        .child("requirements.txt")
        .write_str(dependency)?;

    Mock::given(method("GET"))
        .and(path("/simple/basic-package/"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("cache-control", "no-cache")
                .set_body_raw(
                    json!({
                        "meta": { "api-version": "1.1" },
                        "name": "basic-package",
                        "files": [advertised_file(WHEEL_FILENAME, &artifact_url, Some(&actual_hash))],
                    })
                    .to_string(),
                    "application/vnd.pypi.simple.v1+json",
                ),
        )
        .expect(1)
        .mount(&physical_index)
        .await;
    mount_artifact(&physical_artifacts, WHEEL_FILENAME, artifact.clone(), 1).await;

    uv_snapshot!(context.filters(), context.pip_sync().arg("requirements.txt"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + basic-package==0.1.0
    ");

    physical_index.verify().await;
    physical_artifacts.verify().await;
    physical_index.reset().await;
    physical_artifacts.reset().await;
    mount_simple(
        &physical_index,
        "basic-package",
        vec![advertised_file(
            WHEEL_FILENAME,
            &artifact_url,
            Some(changed_hash),
        )],
        1,
    )
    .await;
    mount_artifact(&physical_artifacts, WHEEL_FILENAME, artifact, 1).await;

    context.reset_venv();
    let mut filters = context.filters();
    filters.push((actual_hash.as_str(), "[COMPUTED_HASH]"));
    let mut command = context.pip_sync();
    command.arg("requirements.txt");

    uv_snapshot!(filters, command, @r"
    exit_code: 1 (failure)
    ----- stderr -----
    Resolved 1 package in [TIME]
    error: Failed to download `basic-package==0.1.0`
      cause: Hash mismatch for `basic-package==0.1.0`

             Expected:
               sha256:0000000000000000000000000000000000000000000000000000000000000000

             Computed:
               sha256:[COMPUTED_HASH]
    ");

    assert_requested_once(&physical_artifacts, &format!("/files/{WHEEL_FILENAME}")).await?;
    assert_no_origin_requests(&canonical_index, &canonical_artifacts).await?;
    Ok(())
}

#[tokio::test]
async fn proxy_index_rejects_cached_source_when_hash_changes() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let artifact = fixture(&context, SOURCE_ZIP_FILENAME)?;
    let actual_hash = sha256(&artifact);
    let changed_hash = "0000000000000000000000000000000000000000000000000000000000000000";
    let canonical_index = MockServer::start().await;
    let canonical_artifacts = MockServer::start().await;
    let physical_index = MockServer::start().await;
    let physical_artifacts = MockServer::start().await;
    let dependency = "source-package==1.0.0";
    let artifact_url = format!("{}/files/{SOURCE_ZIP_FILENAME}", physical_artifacts.uri());

    let pyproject = context.temp_dir.child("pyproject.toml");
    pyproject.write_str(&formatdoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["{dependency}"]

        [tool.uv]
        preview-features = ["proxy-index"]

        [[tool.uv.index]]
        name = "canonical"
        url = "{canonical_index_url}/simple/"
        artifact-base-url = "{canonical_artifact_url}/packages/"
        default = true

        [[tool.uv.proxy-index]]
        name = "socket"
        url = "{physical_index_url}/simple/"
        artifact-base-url = "{physical_artifact_url}/files/"
        proxy-for = "canonical"

        [[tool.uv.dependency-metadata]]
        name = "source-package"
        version = "1.0.0"
        requires-dist = []
    "#,
        canonical_index_url = canonical_index.uri(),
        canonical_artifact_url = canonical_artifacts.uri(),
        physical_index_url = physical_index.uri(),
        physical_artifact_url = physical_artifacts.uri(),
    })?;
    context
        .temp_dir
        .child("requirements.txt")
        .write_str(dependency)?;

    Mock::given(method("GET"))
        .and(path("/simple/source-package/"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("cache-control", "no-cache")
                .set_body_raw(
                    json!({
                        "meta": { "api-version": "1.1" },
                        "name": "source-package",
                        "files": [advertised_file(SOURCE_ZIP_FILENAME, &artifact_url, Some(&actual_hash))],
                    })
                    .to_string(),
                    "application/vnd.pypi.simple.v1+json",
                ),
        )
        .expect(1)
        .mount(&physical_index)
        .await;
    mount_artifact(
        &physical_artifacts,
        SOURCE_ZIP_FILENAME,
        artifact.clone(),
        1,
    )
    .await;

    uv_snapshot!(context.filters(), context.pip_sync().arg("requirements.txt"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + source-package==1.0.0
    ");

    physical_index.verify().await;
    physical_artifacts.verify().await;
    physical_index.reset().await;
    physical_artifacts.reset().await;
    mount_simple(
        &physical_index,
        "source-package",
        vec![advertised_file(
            SOURCE_ZIP_FILENAME,
            &artifact_url,
            Some(changed_hash),
        )],
        1,
    )
    .await;
    mount_artifact(&physical_artifacts, SOURCE_ZIP_FILENAME, artifact, 1).await;

    context.reset_venv();
    let mut filters = context.filters();
    filters.push((actual_hash.as_str(), "[COMPUTED_HASH]"));
    let mut command = context.pip_sync();
    command.arg("requirements.txt");

    uv_snapshot!(filters, command, @r"
    exit_code: 1 (failure)
    ----- stderr -----
    Resolved 1 package in [TIME]
    error: Failed to download and build `source-package==1.0.0`
      cause: Hash mismatch for `source-package==1.0.0`

             Expected:
               sha256:0000000000000000000000000000000000000000000000000000000000000000

             Computed:
               sha256:[COMPUTED_HASH]
    ");

    assert_requested_once(
        &physical_artifacts,
        &format!("/files/{SOURCE_ZIP_FILENAME}"),
    )
    .await?;
    assert_no_origin_requests(&canonical_index, &canonical_artifacts).await?;
    Ok(())
}

#[tokio::test]
async fn proxy_index_rejects_hashless_selected_wheel() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let canonical_index = MockServer::start().await;
    let canonical_artifacts = MockServer::start().await;
    let physical_index = MockServer::start().await;
    let physical_artifacts = MockServer::start().await;
    let artifact_url = format!("{}/files/{WHEEL_FILENAME}", physical_artifacts.uri());

    mount_simple(
        &physical_index,
        "basic-package",
        vec![advertised_file(WHEEL_FILENAME, &artifact_url, None)],
        1,
    )
    .await;
    let dependency = "basic-package==0.1.0";
    let pyproject = context.temp_dir.child("pyproject.toml");
    pyproject.write_str(&formatdoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["{dependency}"]

        [tool.uv]
        preview-features = ["proxy-index"]

        [[tool.uv.index]]
        name = "canonical"
        url = "{canonical_index_url}/simple/"
        artifact-base-url = "{canonical_artifact_url}/packages/"
        default = true

        [[tool.uv.proxy-index]]
        name = "socket"
        url = "{physical_index_url}/simple/"
        artifact-base-url = "{physical_artifact_url}/files/"
        proxy-for = "canonical"

        [[tool.uv.dependency-metadata]]
        name = "basic-package"
        version = "0.1.0"
        requires-dist = []
    "#,
        canonical_index_url = canonical_index.uri(),
        canonical_artifact_url = canonical_artifacts.uri(),
        physical_index_url = physical_index.uri(),
        physical_artifact_url = physical_artifacts.uri(),
    })?;

    let command = context.lock();
    uv_snapshot!(context.filters(), command, @r"
    exit_code: 2 (failure)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    error: Cannot lock `basic_package-0.1.0-py3-none-any.whl` for `basic-package` from proxy index `http://[LOCALHOST]/simple/` because it has no supported hash
    ");

    assert!(!context.temp_dir.child("uv.lock").exists());
    assert_requested_once(&physical_index, "/simple/basic-package/").await?;
    assert_no_requests(&physical_artifacts, "physical artifact origin").await?;
    assert_no_origin_requests(&canonical_index, &canonical_artifacts).await?;
    Ok(())
}

#[tokio::test]
async fn proxy_index_rejects_hashless_selected_source() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let canonical_index = MockServer::start().await;
    let canonical_artifacts = MockServer::start().await;
    let physical_index = MockServer::start().await;
    let physical_artifacts = MockServer::start().await;
    let artifact_url = format!("{}/files/{SOURCE_ZIP_FILENAME}", physical_artifacts.uri());

    mount_simple(
        &physical_index,
        "source-package",
        vec![advertised_file(SOURCE_ZIP_FILENAME, &artifact_url, None)],
        1,
    )
    .await;
    let dependency = "source-package==1.0.0";
    let pyproject = context.temp_dir.child("pyproject.toml");
    pyproject.write_str(&formatdoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["{dependency}"]

        [tool.uv]
        preview-features = ["proxy-index"]

        [[tool.uv.index]]
        name = "canonical"
        url = "{canonical_index_url}/simple/"
        artifact-base-url = "{canonical_artifact_url}/packages/"
        default = true

        [[tool.uv.proxy-index]]
        name = "socket"
        url = "{physical_index_url}/simple/"
        artifact-base-url = "{physical_artifact_url}/files/"
        proxy-for = "canonical"

        [[tool.uv.dependency-metadata]]
        name = "source-package"
        version = "1.0.0"
        requires-dist = []
    "#,
        canonical_index_url = canonical_index.uri(),
        canonical_artifact_url = canonical_artifacts.uri(),
        physical_index_url = physical_index.uri(),
        physical_artifact_url = physical_artifacts.uri(),
    })?;

    let command = context.lock();
    uv_snapshot!(context.filters(), command, @r"
    exit_code: 2 (failure)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    error: Cannot lock `source_package-1.0.0.zip` for `source-package` from proxy index `http://[LOCALHOST]/simple/` because it has no supported hash
    ");

    assert!(!context.temp_dir.child("uv.lock").exists());
    assert_requested_once(&physical_index, "/simple/source-package/").await?;
    assert_no_requests(&physical_artifacts, "physical artifact origin").await?;
    assert_no_origin_requests(&canonical_index, &canonical_artifacts).await?;
    Ok(())
}

#[tokio::test]
async fn proxy_index_rejects_unmapped_artifact_without_origin_fallback() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let canonical_index = MockServer::start().await;
    let canonical_artifacts = MockServer::start().await;
    let physical_index = MockServer::start().await;
    let physical_artifacts = MockServer::start().await;
    let unmapped_artifacts = MockServer::start().await;

    mount_simple(
        &physical_index,
        "basic-package",
        vec![advertised_file(
            WHEEL_FILENAME,
            &format!("{}/files/{WHEEL_FILENAME}", unmapped_artifacts.uri()),
            Some(WHEEL_HASH),
        )],
        1,
    )
    .await;
    let pyproject = context.temp_dir.child("pyproject.toml");
    pyproject.write_str(&formatdoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["basic-package==0.1.0"]

        [tool.uv]
        preview-features = ["proxy-index"]

        [[tool.uv.index]]
        name = "canonical"
        url = "{canonical_index_url}/simple/"
        artifact-base-url = "{canonical_artifact_url}/packages/"
        default = true

        [[tool.uv.proxy-index]]
        name = "socket"
        url = "{physical_index_url}/simple/"
        artifact-base-url = "{physical_artifact_url}/files/"
        proxy-for = "canonical"
    "#,
        canonical_index_url = canonical_index.uri(),
        canonical_artifact_url = canonical_artifacts.uri(),
        physical_index_url = physical_index.uri(),
        physical_artifact_url = physical_artifacts.uri(),
    })?;

    let physical_artifact_url = physical_artifacts.uri();
    let unmapped_artifact_url = unmapped_artifacts.uri();
    let mut filters = context.filters();
    filters.insert(0, (physical_artifact_url.as_str(), "[PHYSICAL_ARTIFACTS]"));
    filters.insert(0, (unmapped_artifact_url.as_str(), "[UNMAPPED_ARTIFACTS]"));

    uv_snapshot!(filters, context.lock(), @r"
    exit_code: 1 (failure)
    ----- stderr -----
    error: Artifact URL `[UNMAPPED_ARTIFACTS]/files/basic_package-0.1.0-py3-none-any.whl` does not match the `artifact-base-url` prefix `[PHYSICAL_ARTIFACTS]/files/`
    ");

    assert!(!context.temp_dir.child("uv.lock").exists());
    assert_requested_once(&physical_index, "/simple/basic-package/").await?;
    assert_no_origin_requests(&canonical_index, &canonical_artifacts).await?;
    assert_no_requests(&physical_artifacts, "configured physical artifact origin").await?;
    assert_no_requests(&unmapped_artifacts, "unmapped physical artifact origin").await?;
    Ok(())
}

#[tokio::test]
async fn proxy_index_does_not_cache_wheel_with_mismatched_locked_hash() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let artifact = fixture(&context, WHEEL_FILENAME)?;
    let expected_hash = "0000000000000000000000000000000000000000000000000000000000000000";
    let actual_hash = sha256(&artifact);
    let canonical_index = MockServer::start().await;
    let canonical_artifacts = MockServer::start().await;
    let physical_index = MockServer::start().await;
    let physical_artifacts = MockServer::start().await;
    let dependency = "basic-package==0.1.0";

    let pyproject = context.temp_dir.child("pyproject.toml");
    pyproject.write_str(&formatdoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["{dependency}"]

        [tool.uv]
        preview-features = ["proxy-index"]

        [[tool.uv.index]]
        name = "canonical"
        url = "{canonical_index_url}/simple/"
        artifact-base-url = "{canonical_artifact_url}/packages/"
        default = true

        [[tool.uv.proxy-index]]
        name = "socket"
        url = "{physical_index_url}/simple/"
        artifact-base-url = "{physical_artifact_url}/files/"
        proxy-for = "canonical"
    "#,
        canonical_index_url = canonical_index.uri(),
        canonical_artifact_url = canonical_artifacts.uri(),
        physical_index_url = physical_index.uri(),
        physical_artifact_url = physical_artifacts.uri(),
    })?;

    write_frozen_lock(
        &context,
        &canonical_index,
        "basic-package",
        "0.1.0",
        &format!(
            r#"wheels = [{{ url = "{}/packages/{WHEEL_FILENAME}", hash = "sha256:{expected_hash}" }}]"#,
            canonical_artifacts.uri()
        ),
    )?;
    let original_lock = context.read("uv.lock");
    mount_artifact(&physical_artifacts, WHEEL_FILENAME, artifact, 2).await;

    let mut filters = context.filters();
    filters.push((actual_hash.as_str(), "[COMPUTED_HASH]"));

    uv_snapshot!(filters, context.sync().arg("--frozen"), @r"
    exit_code: 1 (failure)
    ----- stderr -----
    error: Failed to download `basic-package==0.1.0`
      cause: Hash mismatch for `basic-package==0.1.0`

             Expected:
               sha256:0000000000000000000000000000000000000000000000000000000000000000

             Computed:
               sha256:[COMPUTED_HASH]

    hint: `basic-package` (v0.1.0) was included because `project` (v0.1.0) depends on `basic-package`
    ");

    uv_snapshot!(filters, context.sync().arg("--frozen"), @r"
    exit_code: 1 (failure)
    ----- stderr -----
    error: Failed to download `basic-package==0.1.0`
      cause: Hash mismatch for `basic-package==0.1.0`

             Expected:
               sha256:0000000000000000000000000000000000000000000000000000000000000000

             Computed:
               sha256:[COMPUTED_HASH]

    hint: `basic-package` (v0.1.0) was included because `project` (v0.1.0) depends on `basic-package`
    ");

    let requests = physical_artifacts
        .received_requests()
        .await
        .ok_or_else(|| anyhow!("physical artifact origin should record requests"))?;
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.url.path() == format!("/files/{WHEEL_FILENAME}"))
            .count(),
        2,
        "an artifact with an incorrect locked digest must not enter the cache",
    );
    assert_eq!(context.read("uv.lock"), original_lock);
    assert_no_requests(&physical_index, "physical Simple index").await?;
    assert_no_origin_requests(&canonical_index, &canonical_artifacts).await?;
    Ok(())
}

#[tokio::test]
async fn proxy_index_does_not_cache_source_with_mismatched_locked_hash() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let artifact = fixture(&context, SOURCE_ZIP_FILENAME)?;
    let expected_hash = "0000000000000000000000000000000000000000000000000000000000000000";
    let actual_hash = sha256(&artifact);
    let canonical_index = MockServer::start().await;
    let canonical_artifacts = MockServer::start().await;
    let physical_index = MockServer::start().await;
    let physical_artifacts = MockServer::start().await;
    let dependency = "source-package==1.0.0";

    let pyproject = context.temp_dir.child("pyproject.toml");
    pyproject.write_str(&formatdoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["{dependency}"]

        [tool.uv]
        preview-features = ["proxy-index"]

        [[tool.uv.index]]
        name = "canonical"
        url = "{canonical_index_url}/simple/"
        artifact-base-url = "{canonical_artifact_url}/packages/"
        default = true

        [[tool.uv.proxy-index]]
        name = "socket"
        url = "{physical_index_url}/simple/"
        artifact-base-url = "{physical_artifact_url}/files/"
        proxy-for = "canonical"
    "#,
        canonical_index_url = canonical_index.uri(),
        canonical_artifact_url = canonical_artifacts.uri(),
        physical_index_url = physical_index.uri(),
        physical_artifact_url = physical_artifacts.uri(),
    })?;

    write_frozen_lock(
        &context,
        &canonical_index,
        "source-package",
        "1.0.0",
        &format!(
            r#"sdist = {{ url = "{}/packages/{SOURCE_ZIP_FILENAME}", hash = "sha256:{expected_hash}" }}"#,
            canonical_artifacts.uri()
        ),
    )?;
    let original_lock = context.read("uv.lock");
    mount_artifact(&physical_artifacts, SOURCE_ZIP_FILENAME, artifact, 2).await;

    let mut filters = context.filters();
    filters.push((actual_hash.as_str(), "[COMPUTED_HASH]"));

    uv_snapshot!(filters, context.sync().arg("--frozen"), @r"
    exit_code: 1 (failure)
    ----- stderr -----
    error: Failed to download and build `source-package==1.0.0`
      cause: Hash mismatch for `source-package==1.0.0`

             Expected:
               sha256:0000000000000000000000000000000000000000000000000000000000000000

             Computed:
               sha256:[COMPUTED_HASH]

    hint: `source-package` (v1.0.0) was included because `project` (v0.1.0) depends on `source-package`
    ");

    uv_snapshot!(filters, context.sync().arg("--frozen"), @r"
    exit_code: 1 (failure)
    ----- stderr -----
    error: Failed to download and build `source-package==1.0.0`
      cause: Hash mismatch for `source-package==1.0.0`

             Expected:
               sha256:0000000000000000000000000000000000000000000000000000000000000000

             Computed:
               sha256:[COMPUTED_HASH]

    hint: `source-package` (v1.0.0) was included because `project` (v0.1.0) depends on `source-package`
    ");

    let requests = physical_artifacts
        .received_requests()
        .await
        .ok_or_else(|| anyhow!("physical artifact origin should record requests"))?;
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.url.path() == format!("/files/{SOURCE_ZIP_FILENAME}"))
            .count(),
        2,
        "an artifact with an incorrect locked digest must not enter the cache",
    );
    assert_eq!(context.read("uv.lock"), original_lock);
    assert_no_requests(&physical_index, "physical Simple index").await?;
    assert_no_origin_requests(&canonical_index, &canonical_artifacts).await?;
    Ok(())
}
