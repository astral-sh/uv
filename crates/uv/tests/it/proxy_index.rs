use anyhow::{Result, anyhow};
use assert_fs::prelude::*;
use async_zip::base::write::ZipFileWriter;
use async_zip::{Compression, ZipEntryBuilder};
use indoc::formatdoc;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate, Times};

use uv_static::EnvVars;
use uv_test::uv_snapshot;

const WHEEL_FILENAME: &str = "basic_package-0.1.0-py3-none-any.whl";
const WHEEL_HASH: &str = "7b6229db79b5800e4e98a351b5628c1c8a944533a2d428aeeaa7275a30d4ea82";
const WHEEL_METADATA: &str = "Metadata-Version: 2.3\nName: basic-package\nVersion: 0.1.0\n";
const SOURCE_FILENAME: &str = "basic_package-0.1.0.tar.gz";

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn wheel_file(filename: &str, artifact_url: &str, artifact_hash: Option<&str>) -> Value {
    json!({
        "filename": filename,
        "url": artifact_url,
        "hashes": artifact_hash.map_or_else(
            || json!({}),
            |hash| json!({ "sha256": hash }),
        ),
        "core-metadata": true
    })
}

fn source_file(filename: &str, artifact_url: &str, artifact_hash: Option<&str>) -> Value {
    json!({
        "filename": filename,
        "url": artifact_url,
        "hashes": artifact_hash.map_or_else(
            || json!({}),
            |hash| json!({ "sha256": hash }),
        ),
    })
}

async fn mount_simple<T: Into<Times>>(
    server: &MockServer,
    package: &str,
    files: Vec<Value>,
    requests: T,
) {
    Mock::given(method("GET"))
        .and(path(format!("/simple/{package}/")))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(
                json!({
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

async fn mount_metadata<T: Into<Times>>(
    server: &MockServer,
    filename: &str,
    metadata: &str,
    requests: T,
) {
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
        "{name} received requests: {requests:?}"
    );
    Ok(())
}

async fn assert_requested_path(server: &MockServer, expected: &str) -> Result<()> {
    let requests = server
        .received_requests()
        .await
        .ok_or_else(|| anyhow!("server should record requests"))?;
    assert!(
        requests
            .iter()
            .any(|request| request.url.path() == expected),
        "expected request to {expected}, got {requests:?}"
    );
    Ok(())
}

async fn mount_find_links(
    server: &MockServer,
    artifact_url: &str,
    filename: &str,
    artifact_hash: &str,
) {
    Mock::given(method("GET"))
        .and(path("/links"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            format!(r#"<a href="{artifact_url}#sha256={artifact_hash}">{filename}</a>"#),
            "text/html",
        ))
        .expect(1)
        .mount(server)
        .await;
}

async fn self_contained_source() -> Result<Vec<u8>> {
    let files = [
        (
            "source_package-1.0.0/pyproject.toml",
            r#"
[project]
name = "source-package"
version = "1.0.0"

[build-system]
requires = []
build-backend = "backend"
backend-path = ["."]
"#,
        ),
        (
            "source_package-1.0.0/backend.py",
            r#"
import pathlib
import zipfile


def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    wheel_name = "source_package-1.0.0-py3-none-any.whl"
    wheel_path = pathlib.Path(wheel_directory, wheel_name)
    records = [
        ("source_package/__init__.py", b""),
        (
            "source_package-1.0.0.dist-info/METADATA",
            b"Metadata-Version: 2.1\nName: source-package\nVersion: 1.0.0\n",
        ),
        (
            "source_package-1.0.0.dist-info/WHEEL",
            b"Wheel-Version: 1.0\nGenerator: uv-test\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
        ),
    ]
    with zipfile.ZipFile(wheel_path, "w") as wheel:
        for path, contents in records:
            wheel.writestr(path, contents)
        record = "\n".join(f"{path},," for path, _ in records)
        wheel.writestr(
            "source_package-1.0.0.dist-info/RECORD",
            record + "\nsource_package-1.0.0.dist-info/RECORD,,\n",
        )
    return wheel_name
"#,
        ),
    ];
    let mut writer = ZipFileWriter::new(Vec::new());
    for (filename, contents) in files {
        writer
            .write_entry_whole(
                ZipEntryBuilder::new(filename.into(), Compression::Stored),
                contents.as_bytes(),
            )
            .await?;
    }
    Ok(writer.close().await?)
}

fn fixture(context: &uv_test::TestContext, filename: &str) -> Result<Vec<u8>> {
    Ok(fs_err::read(
        context.workspace_root.join("test/links").join(filename),
    )?)
}

#[derive(Clone)]
struct ProxyConfiguration {
    reference: String,
    index_url: String,
    physical_prefix: String,
    canonical_prefix: String,
}

fn write_project_configuration(
    context: &uv_test::TestContext,
    canonical_index: Option<&str>,
    proxy: Option<ProxyConfiguration>,
    dependency: &str,
    dependency_metadata: Option<(&str, &str)>,
    tool_settings: Option<&str>,
) -> Result<()> {
    let tool_settings = tool_settings.map_or_else(String::new, |settings| {
        formatdoc! {r"
            [tool.uv]
            {settings}
            "}
    });
    let canonical_index = canonical_index.map_or_else(String::new, |canonical_index| {
        formatdoc! {r#"
            [[tool.uv.index]]
            name = "canonical"
            url = "{canonical_index}"
            default = true
            "#}
    });
    let proxy = proxy.map_or_else(String::new, |proxy| {
        let ProxyConfiguration {
            reference,
            index_url,
            physical_prefix,
            canonical_prefix,
        } = proxy;
        formatdoc! {r#"
            [[tool.uv.proxy-index]]
            index = "{reference}"
            url = "{index_url}"
            artifact-url-map = {{ "{physical_prefix}" = "{canonical_prefix}" }}
            "#}
    });
    let dependency_metadata = dependency_metadata.map_or_else(String::new, |(name, version)| {
        formatdoc! {r#"
            [[tool.uv.dependency-metadata]]
            name = "{name}"
            version = "{version}"
            requires-dist = []
            "#}
    });

    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(&formatdoc! {r#"
            [project]
            name = "project"
            version = "0.1.0"
            requires-python = ">=3.12"
            dependencies = ["{dependency}"]

            {tool_settings}
            {canonical_index}
            {proxy}
            {dependency_metadata}
            "#})?;
    Ok(())
}

fn proxy_configuration(
    proxy: &MockServer,
    physical_artifacts: &MockServer,
    canonical_artifacts: &MockServer,
) -> ProxyConfiguration {
    ProxyConfiguration {
        reference: "canonical".to_string(),
        index_url: format!("{}/simple", proxy.uri()),
        physical_prefix: format!("{}/files", physical_artifacts.uri()),
        canonical_prefix: format!("{}/packages", canonical_artifacts.uri()),
    }
}

#[tokio::test]
async fn proxy_index_locks_mapped_wheel_and_source_without_origin_requests() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let wheel = fixture(&context, WHEEL_FILENAME)?;
    let source = fixture(&context, SOURCE_FILENAME)?;
    let canonical = MockServer::start().await;
    let canonical_metadata = MockServer::start().await;
    let canonical_artifacts = MockServer::start().await;
    let proxy = MockServer::start().await;
    let physical_artifacts = MockServer::start().await;

    let wheel_url = format!("{}/files/{WHEEL_FILENAME}", physical_artifacts.uri());
    let source_url = format!("{}/files/{SOURCE_FILENAME}", physical_artifacts.uri());
    let unreachable_filename = "basic_package-0.1.0-cp39-cp39-win_amd64.whl";
    mount_simple(
        &proxy,
        "basic-package",
        vec![
            wheel_file(WHEEL_FILENAME, &wheel_url, Some(WHEEL_HASH)),
            source_file(SOURCE_FILENAME, &source_url, Some(&sha256(&source))),
            source_file(
                unreachable_filename,
                "https://unmapped.example/basic_package-0.1.0-cp39-cp39-win_amd64.whl",
                None,
            ),
        ],
        2,
    )
    .await;
    mount_metadata(&physical_artifacts, WHEEL_FILENAME, WHEEL_METADATA, 1).await;
    mount_artifact(&physical_artifacts, WHEEL_FILENAME, wheel, 1).await;
    write_project_configuration(
        &context,
        Some(&format!("{}/simple", canonical.uri())),
        Some(proxy_configuration(
            &proxy,
            &physical_artifacts,
            &canonical_artifacts,
        )),
        "basic-package==0.1.0",
        None,
        None,
    )?;

    uv_snapshot!(context.filters(), context.lock().env_remove(EnvVars::UV_EXCLUDE_NEWER), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    assert!(lock.contains(&format!("{}/simple", canonical.uri())));
    assert!(lock.contains(&format!(
        "{}/packages/{WHEEL_FILENAME}",
        canonical_artifacts.uri()
    )));
    assert!(lock.contains(&format!(
        "{}/packages/{SOURCE_FILENAME}",
        canonical_artifacts.uri()
    )));
    assert!(!lock.contains(&proxy.uri()));
    assert!(!lock.contains(&physical_artifacts.uri()));
    assert!(!lock.contains(unreachable_filename));

    uv_snapshot!(context.filters(), context.sync().arg("--frozen").arg("--reinstall").env_remove(EnvVars::UV_EXCLUDE_NEWER), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + basic-package==0.1.0
    ");
    assert_requested_path(
        &physical_artifacts,
        &format!("/files/{WHEEL_FILENAME}.metadata"),
    )
    .await?;
    assert_requested_path(&physical_artifacts, &format!("/files/{WHEEL_FILENAME}")).await?;

    uv_snapshot!(context.filters(), context.sync().arg("--offline").arg("--frozen").arg("--reinstall").env_remove(EnvVars::UV_EXCLUDE_NEWER), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     ~ basic-package==0.1.0
    ");

    assert_no_requests(&canonical, "canonical Simple index").await?;
    assert_no_requests(&canonical_metadata, "canonical metadata host").await?;
    assert_no_requests(&canonical_artifacts, "canonical artifact host").await?;
    Ok(())
}

#[tokio::test]
async fn proxy_index_audit_routes_project_status_through_physical_index() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let canonical = MockServer::start().await;
    let canonical_artifacts = MockServer::start().await;
    let proxy = MockServer::start().await;
    let physical_artifacts = MockServer::start().await;
    let osv = MockServer::start().await;
    let canonical_index = format!("{}/simple", canonical.uri());

    write_project_configuration(
        &context,
        Some(&canonical_index),
        Some(proxy_configuration(
            &proxy,
            &physical_artifacts,
            &canonical_artifacts,
        )),
        "basic-package==0.1.0",
        None,
        None,
    )?;
    context
        .temp_dir
        .child("uv.lock")
        .write_str(&formatdoc! {r#"
            version = 1
            revision = 3
            requires-python = ">=3.12"

            [[package]]
            name = "basic-package"
            version = "0.1.0"
            source = {{ registry = "{canonical_index}" }}
            wheels = [
                {{ url = "{}/packages/{WHEEL_FILENAME}", hash = "sha256:{WHEEL_HASH}" }},
            ]

            [[package]]
            name = "project"
            version = "0.1.0"
            source = {{ virtual = "." }}
            dependencies = [
                {{ name = "basic-package" }},
            ]

            [package.metadata]
            requires-dist = [
                {{ name = "basic-package", specifier = "==0.1.0" }},
            ]
        "#, canonical_artifacts.uri()})?;

    Mock::given(method("GET"))
        .and(path("/simple/basic-package/"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(
                json!({
                    "name": "basic-package",
                    "files": [],
                    "versions": [],
                    "project-status": {
                        "status": "deprecated",
                        "reason": "routed-through-proxy"
                    }
                })
                .to_string(),
                "application/vnd.pypi.simple.v1+json",
            ),
        )
        .expect(1)
        .mount(&proxy)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/querybatch"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "results": [{ "vulns": [] }]
        })))
        .expect(1)
        .mount(&osv)
        .await;

    uv_snapshot!(context.filters(), context
        .audit()
        .arg("--preview-features")
        .arg("audit")
        .arg("--frozen")
        .arg("--service-url")
        .arg(osv.uri()), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    Adverse statuses:

    - basic-package is deprecated: routed-through-proxy

    ----- stderr -----
    Found no known vulnerabilities and 1 adverse project status in 1 package
    ");
    assert_requested_path(&proxy, "/simple/basic-package/").await?;
    assert_no_requests(&canonical, "canonical Simple index").await?;
    Ok(())
}

#[tokio::test]
async fn proxy_index_locks_mapped_source_and_reuses_offline_cache() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let source = self_contained_source().await?;
    let filename = "source_package-1.0.0.zip";
    let canonical = MockServer::start().await;
    let canonical_metadata = MockServer::start().await;
    let canonical_artifacts = MockServer::start().await;
    let proxy = MockServer::start().await;
    let physical_artifacts = MockServer::start().await;
    let source_url = format!("{}/files/{filename}", physical_artifacts.uri());

    mount_simple(
        &proxy,
        "source-package",
        vec![source_file(filename, &source_url, Some(&sha256(&source)))],
        1_u64..=2,
    )
    .await;
    mount_artifact(&physical_artifacts, filename, source, 1).await;
    write_project_configuration(
        &context,
        Some(&format!("{}/simple", canonical.uri())),
        Some(proxy_configuration(
            &proxy,
            &physical_artifacts,
            &canonical_artifacts,
        )),
        "source-package==1.0.0",
        Some(("source-package", "1.0.0")),
        None,
    )?;

    uv_snapshot!(context.filters(), context.lock().env_remove(EnvVars::UV_EXCLUDE_NEWER), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");
    let lock = context.read("uv.lock");
    assert!(lock.contains(&format!(
        "{}/packages/{filename}",
        canonical_artifacts.uri()
    )));
    assert!(!lock.contains(&proxy.uri()));
    assert!(!lock.contains(&physical_artifacts.uri()));

    uv_snapshot!(context.filters(), context.sync().arg("--frozen").env_remove(EnvVars::UV_EXCLUDE_NEWER), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + source-package==1.0.0
    ");
    assert_requested_path(&physical_artifacts, &format!("/files/{filename}")).await?;

    uv_snapshot!(context.filters(), context.sync().arg("--offline").arg("--frozen").arg("--reinstall").env_remove(EnvVars::UV_EXCLUDE_NEWER), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     ~ source-package==1.0.0
    ");

    assert_no_requests(&canonical, "canonical Simple index").await?;
    assert_no_requests(&canonical_metadata, "canonical metadata host").await?;
    assert_no_requests(&canonical_artifacts, "canonical artifact host").await?;
    Ok(())
}

#[tokio::test]
async fn proxy_index_keeps_incompatible_unmapped_wheel_source_bound() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let source = fixture(&context, "tqdm-999.0.0.tar.gz")?;
    let source_filename = "tqdm-999.0.0.tar.gz";
    let wheel_filename = "tqdm-999.0.0-cp39-cp39-win_amd64.whl";
    let canonical = MockServer::start().await;
    let canonical_artifacts = MockServer::start().await;
    let proxy = MockServer::start().await;
    let physical_artifacts = MockServer::start().await;
    let source_url = format!("{}/files/{source_filename}", physical_artifacts.uri());

    mount_simple(
        &proxy,
        "tqdm",
        vec![
            wheel_file(
                wheel_filename,
                "https://unmapped.example/tqdm-999.0.0-cp39-cp39-win_amd64.whl",
                None,
            ),
            source_file(source_filename, &source_url, Some(&sha256(&source))),
        ],
        1,
    )
    .await;
    write_project_configuration(
        &context,
        Some(&format!("{}/simple", canonical.uri())),
        Some(proxy_configuration(
            &proxy,
            &physical_artifacts,
            &canonical_artifacts,
        )),
        "tqdm==999.0.0",
        Some(("tqdm", "999.0.0")),
        None,
    )?;

    uv_snapshot!(context.filters(), context.lock().env_remove(EnvVars::UV_EXCLUDE_NEWER), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");
    let lock = context.read("uv.lock");
    assert!(lock.contains(&format!(
        "{}/packages/{source_filename}",
        canonical_artifacts.uri()
    )));
    assert!(!lock.contains(wheel_filename));
    assert_no_requests(&canonical, "canonical Simple index").await?;
    assert_no_requests(&canonical_artifacts, "canonical artifact host").await?;
    assert_no_requests(&physical_artifacts, "physical artifact host").await?;
    Ok(())
}

#[tokio::test]
async fn proxy_index_ignores_unselected_find_links_artifacts() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let canonical = MockServer::start().await;
    let canonical_artifacts = MockServer::start().await;
    let proxy = MockServer::start().await;
    let physical_artifacts = MockServer::start().await;
    let find_links = MockServer::start().await;
    let wheel_url = format!("{}/files/{WHEEL_FILENAME}", physical_artifacts.uri());
    let find_links_url = format!("{}/files/{SOURCE_FILENAME}", find_links.uri());

    mount_simple(
        &proxy,
        "basic-package",
        vec![wheel_file(WHEEL_FILENAME, &wheel_url, Some(WHEEL_HASH))],
        1,
    )
    .await;
    mount_find_links(
        &find_links,
        &find_links_url,
        SOURCE_FILENAME,
        "1111111111111111111111111111111111111111111111111111111111111111",
    )
    .await;
    write_project_configuration(
        &context,
        Some(&format!("{}/simple", canonical.uri())),
        Some(proxy_configuration(
            &proxy,
            &physical_artifacts,
            &canonical_artifacts,
        )),
        "basic-package==0.1.0",
        Some(("basic-package", "0.1.0")),
        Some(&format!("find-links = [\"{}/links\"]", find_links.uri())),
    )?;

    uv_snapshot!(context.filters(), context.lock().env_remove(EnvVars::UV_EXCLUDE_NEWER), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");
    let lock = context.read("uv.lock");
    assert!(lock.contains(&format!(
        "{}/packages/{WHEEL_FILENAME}",
        canonical_artifacts.uri()
    )));
    assert!(!lock.contains(&find_links_url));
    assert!(!lock.contains(SOURCE_FILENAME));
    assert_no_requests(&canonical, "canonical Simple index").await?;
    assert_no_requests(&canonical_artifacts, "canonical artifact host").await?;
    Ok(())
}

#[tokio::test]
async fn proxy_index_isolates_physical_cache_after_route_change() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let wheel = fixture(&context, WHEEL_FILENAME)?;
    let canonical = MockServer::start().await;
    let canonical_artifacts = MockServer::start().await;
    let proxy_a = MockServer::start().await;
    let physical_a = MockServer::start().await;
    let proxy_b = MockServer::start().await;
    let physical_b = MockServer::start().await;

    let wheel_a_url = format!("{}/files/{WHEEL_FILENAME}", physical_a.uri());
    mount_simple(
        &proxy_a,
        "basic-package",
        vec![wheel_file(WHEEL_FILENAME, &wheel_a_url, Some(WHEEL_HASH))],
        1_u64..=2,
    )
    .await;
    mount_metadata(&physical_a, WHEEL_FILENAME, WHEEL_METADATA, 1).await;
    mount_artifact(&physical_a, WHEEL_FILENAME, wheel.clone(), 1).await;
    write_project_configuration(
        &context,
        Some(&format!("{}/simple", canonical.uri())),
        Some(proxy_configuration(
            &proxy_a,
            &physical_a,
            &canonical_artifacts,
        )),
        "basic-package==0.1.0",
        None,
        None,
    )?;

    uv_snapshot!(context.filters(), context.lock().env_remove(EnvVars::UV_EXCLUDE_NEWER), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");
    uv_snapshot!(context.filters(), context.sync().arg("--frozen").env_remove(EnvVars::UV_EXCLUDE_NEWER), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + basic-package==0.1.0
    ");

    fs_err::remove_dir_all(&context.venv)?;
    fs_err::remove_file(context.temp_dir.child("uv.lock"))?;

    let wheel_b_url = format!("{}/files/{WHEEL_FILENAME}", physical_b.uri());
    mount_simple(
        &proxy_b,
        "basic-package",
        vec![wheel_file(WHEEL_FILENAME, &wheel_b_url, Some(WHEEL_HASH))],
        2,
    )
    .await;
    mount_metadata(&physical_b, WHEEL_FILENAME, WHEEL_METADATA, 1).await;
    mount_artifact(&physical_b, WHEEL_FILENAME, wheel, 1).await;
    write_project_configuration(
        &context,
        Some(&format!("{}/simple", canonical.uri())),
        Some(proxy_configuration(
            &proxy_b,
            &physical_b,
            &canonical_artifacts,
        )),
        "basic-package==0.1.0",
        None,
        None,
    )?;

    uv_snapshot!(context.filters(), context.lock().env_remove(EnvVars::UV_EXCLUDE_NEWER), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 2 packages in [TIME]
    ");
    uv_snapshot!(context.filters(), context.sync().arg("--frozen").env_remove(EnvVars::UV_EXCLUDE_NEWER), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: .venv
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + basic-package==0.1.0
    ");

    let lock = context.read("uv.lock");
    assert!(lock.contains(&format!(
        "{}/packages/{WHEEL_FILENAME}",
        canonical_artifacts.uri()
    )));
    assert_no_requests(&canonical, "canonical Simple index").await?;
    assert_no_requests(&canonical_artifacts, "canonical artifact host").await?;
    assert_requested_path(&physical_b, &format!("/files/{WHEEL_FILENAME}")).await?;
    Ok(())
}

#[tokio::test]
async fn proxy_index_existing_lock_uses_hashless_listing_and_ignores_changed_map() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let wheel = fixture(&context, WHEEL_FILENAME)?;
    let canonical = MockServer::start().await;
    let canonical_artifacts = MockServer::start().await;
    let proxy = MockServer::start().await;
    let physical_artifacts = MockServer::start().await;
    let changed_target = MockServer::start().await;
    let canonical_artifact_url = format!("{}/files/{WHEEL_FILENAME}", canonical_artifacts.uri());

    mount_simple(
        &canonical,
        "basic-package",
        vec![wheel_file(
            WHEEL_FILENAME,
            &canonical_artifact_url,
            Some(WHEEL_HASH),
        )],
        1,
    )
    .await;
    mount_metadata(&canonical_artifacts, WHEEL_FILENAME, WHEEL_METADATA, 1).await;
    write_project_configuration(
        &context,
        Some(&format!("{}/simple", canonical.uri())),
        None,
        "basic-package==0.1.0",
        None,
        None,
    )?;

    uv_snapshot!(context.filters(), context.lock().env_remove(EnvVars::UV_EXCLUDE_NEWER), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");
    let original_lock = context.read("uv.lock");
    let canonical_requests = canonical
        .received_requests()
        .await
        .ok_or_else(|| anyhow!("canonical index should record requests"))?
        .len();

    let canonical_index = format!("{}/simple", canonical.uri());
    write_project_configuration(
        &context,
        None,
        Some(ProxyConfiguration {
            reference: canonical_index.clone(),
            index_url: format!("{}/simple", proxy.uri()),
            physical_prefix: format!("{}/files", physical_artifacts.uri()),
            canonical_prefix: format!("{}/changed", changed_target.uri()),
        }),
        "basic-package==0.1.0",
        None,
        None,
    )?;
    uv_snapshot!(context.filters(), context.lock().arg("--locked").env_remove(EnvVars::UV_EXCLUDE_NEWER), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");
    assert_eq!(context.read("uv.lock"), original_lock);

    fs_err::remove_dir_all(&context.cache_dir)?;
    let proxy_artifact_url = format!("{}/files/{WHEEL_FILENAME}", physical_artifacts.uri());
    mount_simple(
        &proxy,
        "basic-package",
        vec![wheel_file(WHEEL_FILENAME, &proxy_artifact_url, None)],
        1,
    )
    .await;
    mount_artifact(&physical_artifacts, WHEEL_FILENAME, wheel, 1).await;

    uv_snapshot!(context.filters(), context.sync().arg("--frozen").env_remove(EnvVars::UV_EXCLUDE_NEWER), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + basic-package==0.1.0
    ");
    assert_eq!(context.read("uv.lock"), original_lock);
    assert_eq!(
        canonical
            .received_requests()
            .await
            .ok_or_else(|| anyhow!("canonical index should record requests"))?
            .len(),
        canonical_requests
    );
    assert_no_requests(&changed_target, "changed artifact map target").await?;
    assert_requested_path(&physical_artifacts, &format!("/files/{WHEEL_FILENAME}")).await?;
    Ok(())
}

#[tokio::test]
async fn proxy_index_routes_locked_source_and_reuses_physical_cache() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let source = self_contained_source().await?;
    let filename = "source_package-1.0.0.zip";
    let canonical = MockServer::start().await;
    let canonical_artifacts = MockServer::start().await;
    let proxy = MockServer::start().await;
    let physical_artifacts = MockServer::start().await;
    let changed_target = MockServer::start().await;
    let canonical_artifact_url = format!("{}/files/{filename}", canonical_artifacts.uri());

    mount_simple(
        &canonical,
        "source-package",
        vec![source_file(
            filename,
            &canonical_artifact_url,
            Some(&sha256(&source)),
        )],
        1,
    )
    .await;
    write_project_configuration(
        &context,
        Some(&format!("{}/simple", canonical.uri())),
        None,
        "source-package==1.0.0",
        Some(("source-package", "1.0.0")),
        None,
    )?;

    uv_snapshot!(context.filters(), context.lock().env_remove(EnvVars::UV_EXCLUDE_NEWER), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");
    let original_lock = context.read("uv.lock");
    let canonical_requests = canonical
        .received_requests()
        .await
        .ok_or_else(|| anyhow!("canonical index should record requests"))?
        .len();

    let canonical_index = format!("{}/simple", canonical.uri());
    write_project_configuration(
        &context,
        None,
        Some(ProxyConfiguration {
            reference: canonical_index,
            index_url: format!("{}/simple", proxy.uri()),
            physical_prefix: format!("{}/files", physical_artifacts.uri()),
            canonical_prefix: format!("{}/changed", changed_target.uri()),
        }),
        "source-package==1.0.0",
        Some(("source-package", "1.0.0")),
        None,
    )?;
    uv_snapshot!(context.filters(), context.lock().arg("--locked").env_remove(EnvVars::UV_EXCLUDE_NEWER), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");
    assert_eq!(context.read("uv.lock"), original_lock);

    fs_err::remove_dir_all(&context.cache_dir)?;
    let proxy_artifact_url = format!("{}/files/{filename}", physical_artifacts.uri());
    mount_simple(
        &proxy,
        "source-package",
        vec![source_file(
            filename,
            &proxy_artifact_url,
            Some(&sha256(&source)),
        )],
        1,
    )
    .await;
    mount_artifact(&physical_artifacts, filename, source, 1).await;

    uv_snapshot!(context.filters(), context.sync().arg("--frozen").env_remove(EnvVars::UV_EXCLUDE_NEWER), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + source-package==1.0.0
    ");
    uv_snapshot!(context.filters(), context.sync().arg("--offline").arg("--frozen").arg("--reinstall").env_remove(EnvVars::UV_EXCLUDE_NEWER), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     ~ source-package==1.0.0
    ");
    assert_eq!(context.read("uv.lock"), original_lock);
    assert_eq!(
        canonical
            .received_requests()
            .await
            .ok_or_else(|| anyhow!("canonical index should record requests"))?
            .len(),
        canonical_requests
    );
    assert_no_requests(&changed_target, "changed artifact map target").await?;
    assert_requested_path(&physical_artifacts, &format!("/files/{filename}")).await?;
    Ok(())
}

#[tokio::test]
async fn proxy_index_pip_ignores_artifact_url_map() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let wheel = fixture(&context, WHEEL_FILENAME)?;
    let canonical = MockServer::start().await;
    let mapped_target = MockServer::start().await;
    let proxy = MockServer::start().await;
    let physical_artifacts = MockServer::start().await;
    let proxy_artifact_url = format!("{}/files/{WHEEL_FILENAME}", physical_artifacts.uri());

    mount_simple(
        &proxy,
        "basic-package",
        vec![wheel_file(WHEEL_FILENAME, &proxy_artifact_url, None)],
        1,
    )
    .await;
    mount_artifact(&physical_artifacts, WHEEL_FILENAME, wheel, 1).await;
    write_project_configuration(
        &context,
        Some(&format!("{}/simple", canonical.uri())),
        Some(proxy_configuration(
            &proxy,
            &physical_artifacts,
            &mapped_target,
        )),
        "basic-package==0.1.0",
        None,
        None,
    )?;
    context
        .temp_dir
        .child("requirements.txt")
        .write_str("basic-package==0.1.0")?;

    uv_snapshot!(context.filters(), context.pip_sync().arg("requirements.txt").env_remove(EnvVars::UV_EXCLUDE_NEWER), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + basic-package==0.1.0
    ");
    assert_no_requests(&canonical, "canonical Simple index").await?;
    assert_no_requests(&mapped_target, "artifact map target").await?;
    assert_requested_path(&physical_artifacts, &format!("/files/{WHEEL_FILENAME}")).await?;
    Ok(())
}

#[tokio::test]
async fn proxy_index_rejects_wheel_bytes_with_wrong_advertised_hash() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let mut wheel = fixture(&context, WHEEL_FILENAME)?;
    wheel.push(0);
    let canonical = MockServer::start().await;
    let canonical_artifacts = MockServer::start().await;
    let proxy = MockServer::start().await;
    let physical_artifacts = MockServer::start().await;
    let proxy_artifact_url = format!("{}/files/{WHEEL_FILENAME}", physical_artifacts.uri());

    mount_simple(
        &proxy,
        "basic-package",
        vec![wheel_file(
            WHEEL_FILENAME,
            &proxy_artifact_url,
            Some(WHEEL_HASH),
        )],
        1_u64..=2,
    )
    .await;
    mount_metadata(&physical_artifacts, WHEEL_FILENAME, WHEEL_METADATA, 1).await;
    mount_artifact(&physical_artifacts, WHEEL_FILENAME, wheel, 1).await;
    write_project_configuration(
        &context,
        Some(&format!("{}/simple", canonical.uri())),
        Some(proxy_configuration(
            &proxy,
            &physical_artifacts,
            &canonical_artifacts,
        )),
        "basic-package==0.1.0",
        None,
        None,
    )?;

    uv_snapshot!(context.filters(), context.lock().env_remove(EnvVars::UV_EXCLUDE_NEWER), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");
    uv_snapshot!(context.filters(), context.sync().arg("--frozen").env_remove(EnvVars::UV_EXCLUDE_NEWER), @r#"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to download `basic-package==0.1.0`
      ╰─▶ Hash mismatch for `basic-package==0.1.0`

          Expected:
            sha256:7b6229db79b5800e4e98a351b5628c1c8a944533a2d428aeeaa7275a30d4ea82

          Computed:
            sha256:fd5f923df7d7a5b752ab08562109f5cd3ca55ab63e90c3993a9a37822a78c1cb

    hint: `basic-package` (v0.1.0) was included because `project` (v0.1.0) depends on `basic-package`
    "#);
    assert_no_requests(&canonical, "canonical Simple index").await?;
    assert_no_requests(&canonical_artifacts, "canonical artifact host").await?;
    Ok(())
}

#[tokio::test]
async fn proxy_index_rejects_source_bytes_with_wrong_advertised_hash() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let advertised_source = fixture(&context, "tqdm-999.0.0.tar.gz")?;
    let served_source = fixture(&context, "extras-0.0.2.tar.gz")?;
    let filename = "tqdm-999.0.0.tar.gz";
    let canonical = MockServer::start().await;
    let canonical_artifacts = MockServer::start().await;
    let proxy = MockServer::start().await;
    let physical_artifacts = MockServer::start().await;
    let proxy_artifact_url = format!("{}/files/{filename}", physical_artifacts.uri());

    mount_simple(
        &proxy,
        "tqdm",
        vec![source_file(
            filename,
            &proxy_artifact_url,
            Some(&sha256(&advertised_source)),
        )],
        1_u64..=2,
    )
    .await;
    mount_artifact(&physical_artifacts, filename, served_source, 1).await;
    write_project_configuration(
        &context,
        Some(&format!("{}/simple", canonical.uri())),
        Some(proxy_configuration(
            &proxy,
            &physical_artifacts,
            &canonical_artifacts,
        )),
        "tqdm==999.0.0",
        Some(("tqdm", "999.0.0")),
        None,
    )?;

    uv_snapshot!(context.filters(), context.lock().env_remove(EnvVars::UV_EXCLUDE_NEWER), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");
    uv_snapshot!(context.filters(), context.sync().arg("--frozen").env_remove(EnvVars::UV_EXCLUDE_NEWER), @r#"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to download and build `tqdm==999.0.0`
      ╰─▶ Hash mismatch for `tqdm==999.0.0`

          Expected:
            sha256:89fa05cffa7f457658373b85de302d24d0c205ceda2819a8739e324b75e9430b

          Computed:
            sha256:8c6bf0e0d2cb5dd1051ca06c7824917b6e00ab40e93097ca5bb8bb1598e08830

    hint: `tqdm` (v999.0.0) was included because `project` (v0.1.0) depends on `tqdm`
    "#);
    assert_no_requests(&canonical, "canonical Simple index").await?;
    assert_no_requests(&canonical_artifacts, "canonical artifact host").await?;
    Ok(())
}

#[tokio::test]
async fn proxy_index_requires_artifact_url_map() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let canonical = MockServer::start().await;
    let proxy = MockServer::start().await;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc::indoc! {r#"
            [project]
            name = "project"
            version = "0.1.0"
            requires-python = ">=3.12"
            dependencies = ["basic-package==0.1.0"]
            "#})?;
    context
        .temp_dir
        .child("uv.toml")
        .write_str(&formatdoc! {r#"
            [[index]]
            name = "canonical"
            url = "{canonical}/simple"
            default = true

            [[proxy-index]]
            index = "canonical"
            url = "{proxy}/simple"
            "#,
            canonical = canonical.uri(),
            proxy = proxy.uri(),
        })?;

    uv_snapshot!(context.filters(), context.lock().env_remove(EnvVars::UV_EXCLUDE_NEWER), @r#"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to parse: `uv.toml`
      Caused by: TOML parse error at line 6, column 1
          |
        6 | [[proxy-index]]
          | ^^^^^^^^^^^^^^^
        missing field `artifact-url-map`
    "#);
    assert!(!context.temp_dir.child("uv.lock").exists());
    assert_no_requests(&canonical, "canonical Simple index").await?;
    assert_no_requests(&proxy, "proxy Simple index").await?;
    Ok(())
}

#[tokio::test]
async fn proxy_index_rejects_empty_and_overlapping_maps_at_lock_boundary() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let canonical = MockServer::start().await;
    let proxy = MockServer::start().await;
    let pyproject = context.temp_dir.child("pyproject.toml");

    mount_simple(
        &proxy,
        "basic-package",
        vec![wheel_file(
            WHEEL_FILENAME,
            &format!("{}/files/{WHEEL_FILENAME}", proxy.uri()),
            Some(WHEEL_HASH),
        )],
        1_u64..,
    )
    .await;

    pyproject.write_str(&formatdoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["basic-package==0.1.0"]

        [[tool.uv.index]]
        name = "canonical"
        url = "{canonical}/simple"
        default = true

        [[tool.uv.proxy-index]]
        index = "canonical"
        url = "{proxy}/simple"
        artifact-url-map = {{}}

        [[tool.uv.dependency-metadata]]
        name = "basic-package"
        version = "0.1.0"
        requires-dist = []
        "#,
        canonical = canonical.uri(),
        proxy = proxy.uri(),
    })?;
    uv_snapshot!(context.filters(), context.lock().env_remove(EnvVars::UV_EXCLUDE_NEWER), @r#"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    error: Invalid artifact URL map for proxy index `http://[LOCALHOST]/simple`
      Caused by: Artifact URL map must contain at least one physical-to-canonical prefix mapping
    "#);

    pyproject.write_str(&formatdoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["basic-package==0.1.0"]

        [[tool.uv.index]]
        name = "canonical"
        url = "{canonical}/simple"
        default = true

        [[tool.uv.proxy-index]]
        index = "canonical"
        url = "{proxy}/simple"
        artifact-url-map = {{
            "{proxy}/files" = "https://canonical.example/packages",
            "{proxy}/files/" = "https://other.example/packages",
        }}

        [[tool.uv.dependency-metadata]]
        name = "basic-package"
        version = "0.1.0"
        requires-dist = []
        "#,
        canonical = canonical.uri(),
        proxy = proxy.uri(),
    })?;
    uv_snapshot!(context.filters(), context.lock().env_remove(EnvVars::UV_EXCLUDE_NEWER), @r#"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    error: Invalid artifact URL map for proxy index `http://[LOCALHOST]/simple`
      Caused by: Physical artifact URL prefixes `http://[LOCALHOST]/files` and `http://[LOCALHOST]/files` overlap
    "#);

    assert!(!context.temp_dir.child("uv.lock").exists());
    assert_no_requests(&canonical, "canonical Simple index").await?;
    assert_requested_path(&proxy, "/simple/basic-package/").await?;
    Ok(())
}

#[tokio::test]
async fn proxy_index_rejects_unmapped_selected_artifact() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let canonical = MockServer::start().await;
    let canonical_artifacts = MockServer::start().await;
    let proxy = MockServer::start().await;
    let physical_artifacts = MockServer::start().await;
    let proxy_artifact_url = format!("{}/files/{WHEEL_FILENAME}", physical_artifacts.uri());

    mount_simple(
        &proxy,
        "basic-package",
        vec![wheel_file(
            WHEEL_FILENAME,
            &proxy_artifact_url,
            Some(WHEEL_HASH),
        )],
        1,
    )
    .await;
    mount_metadata(&physical_artifacts, WHEEL_FILENAME, WHEEL_METADATA, 1).await;
    write_project_configuration(
        &context,
        Some(&format!("{}/simple", canonical.uri())),
        Some(ProxyConfiguration {
            reference: "canonical".to_string(),
            index_url: format!("{}/simple", proxy.uri()),
            physical_prefix: format!("{}/other", physical_artifacts.uri()),
            canonical_prefix: format!("{}/packages", canonical_artifacts.uri()),
        }),
        "basic-package==0.1.0",
        None,
        None,
    )?;

    uv_snapshot!(context.filters(), context.lock().env_remove(EnvVars::UV_EXCLUDE_NEWER), @r#"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    error: Failed to canonicalize `basic_package-0.1.0-py3-none-any.whl` for `basic-package` from proxy index `http://[LOCALHOST]/simple` against `http://[LOCALHOST]/simple`
      Caused by: Artifact URL `http://[LOCALHOST]/files/basic_package-0.1.0-py3-none-any.whl` does not match any configured physical prefix
    "#);
    assert!(!context.temp_dir.child("uv.lock").exists());
    assert_no_requests(&canonical, "canonical Simple index").await?;
    assert_no_requests(&canonical_artifacts, "canonical artifact host").await?;
    Ok(())
}

#[tokio::test]
async fn proxy_index_rejects_hashless_selected_artifact() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let canonical = MockServer::start().await;
    let canonical_artifacts = MockServer::start().await;
    let proxy = MockServer::start().await;
    let physical_artifacts = MockServer::start().await;
    let proxy_artifact_url = format!("{}/files/{WHEEL_FILENAME}", physical_artifacts.uri());

    mount_simple(
        &proxy,
        "basic-package",
        vec![wheel_file(WHEEL_FILENAME, &proxy_artifact_url, None)],
        1,
    )
    .await;
    mount_metadata(&physical_artifacts, WHEEL_FILENAME, WHEEL_METADATA, 1).await;
    write_project_configuration(
        &context,
        Some(&format!("{}/simple", canonical.uri())),
        Some(proxy_configuration(
            &proxy,
            &physical_artifacts,
            &canonical_artifacts,
        )),
        "basic-package==0.1.0",
        None,
        None,
    )?;

    uv_snapshot!(context.filters(), context.lock().env_remove(EnvVars::UV_EXCLUDE_NEWER), @r#"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    error: Cannot lock `basic_package-0.1.0-py3-none-any.whl` for `basic-package` from proxy index `http://[LOCALHOST]/simple` because it has no supported advertised digest
    "#);
    assert!(!context.temp_dir.child("uv.lock").exists());
    assert_no_requests(&canonical, "canonical Simple index").await?;
    assert_no_requests(&canonical_artifacts, "canonical artifact host").await?;
    Ok(())
}

#[tokio::test]
async fn proxy_index_map_failure_preserves_existing_lock() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let canonical = MockServer::start().await;
    let canonical_artifacts = MockServer::start().await;
    let proxy = MockServer::start().await;
    let physical_artifacts = MockServer::start().await;
    let proxy_artifact_url = format!("{}/files/{WHEEL_FILENAME}", physical_artifacts.uri());

    mount_simple(
        &proxy,
        "basic-package",
        vec![wheel_file(
            WHEEL_FILENAME,
            &proxy_artifact_url,
            Some(WHEEL_HASH),
        )],
        2,
    )
    .await;
    mount_metadata(
        &physical_artifacts,
        WHEEL_FILENAME,
        WHEEL_METADATA,
        1_u64..=2,
    )
    .await;
    write_project_configuration(
        &context,
        Some(&format!("{}/simple", canonical.uri())),
        Some(proxy_configuration(
            &proxy,
            &physical_artifacts,
            &canonical_artifacts,
        )),
        "basic-package==0.1.0",
        None,
        None,
    )?;

    uv_snapshot!(context.filters(), context.lock().env_remove(EnvVars::UV_EXCLUDE_NEWER), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");
    let original_lock = context.read("uv.lock");

    write_project_configuration(
        &context,
        Some(&format!("{}/simple", canonical.uri())),
        Some(ProxyConfiguration {
            reference: "canonical".to_string(),
            index_url: format!("{}/simple", proxy.uri()),
            physical_prefix: format!("{}/other", physical_artifacts.uri()),
            canonical_prefix: format!("{}/packages", canonical_artifacts.uri()),
        }),
        "basic-package==0.1.0",
        None,
        None,
    )?;
    uv_snapshot!(context.filters(), context.lock().arg("--upgrade").env_remove(EnvVars::UV_EXCLUDE_NEWER), @r#"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    error: Failed to canonicalize `basic_package-0.1.0-py3-none-any.whl` for `basic-package` from proxy index `http://[LOCALHOST]/simple` against `http://[LOCALHOST]/simple`
      Caused by: Artifact URL `http://[LOCALHOST]/files/basic_package-0.1.0-py3-none-any.whl` does not match any configured physical prefix
    "#);

    assert_eq!(context.read("uv.lock"), original_lock);
    assert_no_requests(&canonical, "canonical Simple index").await?;
    assert_no_requests(&canonical_artifacts, "canonical artifact host").await?;
    Ok(())
}
