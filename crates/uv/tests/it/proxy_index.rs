use std::path::Path;

use anyhow::{Context as _, Result, anyhow};
use assert_fs::prelude::*;
use indoc::formatdoc;
use insta::allow_duplicates;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use wiremock::matchers::{basic_auth, method, path};
use wiremock::{Match, Mock, MockServer, ResponseTemplate};

use uv_test::{TestContext, uv_snapshot};

const WHEEL_FILENAME: &str = "basic_package-0.1.0-py3-none-any.whl";
const WHEEL_HASH: &str = "7b6229db79b5800e4e98a351b5628c1c8a944533a2d428aeeaa7275a30d4ea82";
const WHEEL_METADATA: &str = "Metadata-Version: 2.3\nName: basic-package\nVersion: 0.1.0\n";
const SOURCE_FILENAME: &str = "basic_package-0.1.0.tar.gz";

#[derive(Clone, Copy)]
enum ArtifactKind {
    Wheel,
    Source,
}

impl ArtifactKind {
    fn package(self) -> &'static str {
        match self {
            Self::Wheel => "basic-package",
            Self::Source => "source-package",
        }
    }

    fn version(self) -> &'static str {
        match self {
            Self::Wheel => "0.1.0",
            Self::Source => "1.0.0",
        }
    }

    fn filename(self) -> &'static str {
        match self {
            Self::Wheel => WHEEL_FILENAME,
            Self::Source => "source_package-1.0.0.zip",
        }
    }

    fn lock_key(self) -> &'static str {
        match self {
            Self::Wheel => "wheels",
            Self::Source => "sdist",
        }
    }

    fn advertised_file(self, url: &str, hash: Option<&str>) -> Value {
        advertised_file(self.filename(), url, hash)
    }

    fn bytes(self, context: &TestContext) -> Result<Vec<u8>> {
        fixture(context, self.filename())
    }
}

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

struct ProxyConfiguration<'a> {
    canonical_index_url: String,
    canonical_artifact_url: String,
    physical_index_url: String,
    physical_artifact_url: String,
    dependency: &'a str,
    dependency_metadata: Option<(&'a str, &'a str)>,
}

impl ProxyConfiguration<'_> {
    fn write(self, context: &TestContext) -> Result<()> {
        let Self {
            canonical_index_url,
            canonical_artifact_url,
            physical_index_url,
            physical_artifact_url,
            dependency,
            dependency_metadata,
        } = self;
        let dependency_metadata =
            dependency_metadata.map_or_else(String::new, |(name, version)| {
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

                [[tool.uv.index]]
                name = "canonical"
                url = "{canonical_index_url}/simple/"
                artifact-base-url = "{canonical_artifact_url}/packages/"
                default = true

                [[tool.uv.index]]
                name = "socket"
                url = "{physical_index_url}/simple/"
                artifact-base-url = "{physical_artifact_url}/files/"
                proxy-for = "canonical"

                {dependency_metadata}
                "#,
            })?;
        Ok(())
    }
}

fn write_implicit_pypi_configuration(
    context: &TestContext,
    physical_index: &MockServer,
    physical_artifacts: &MockServer,
    dependency: &str,
) -> Result<()> {
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(&formatdoc! {r#"
            [project]
            name = "project"
            version = "0.1.0"
            requires-python = ">=3.12"
            dependencies = ["{dependency}"]

            [[tool.uv.index]]
            name = "socket"
            url = "{}/simple/"
            artifact-base-url = "{}/files/"
            proxy-for = "pypi"
            "#,
            physical_index.uri(),
            physical_artifacts.uri(),
        })?;
    Ok(())
}

fn write_frozen_lock(
    context: &TestContext,
    canonical_index: &MockServer,
    package: &str,
    version: &str,
    artifact_key: &str,
    artifact_url: &str,
    artifact_hash: &str,
) -> Result<()> {
    write_frozen_lock_for_registry(
        context,
        &format!("{}/simple/", canonical_index.uri()),
        package,
        version,
        artifact_key,
        artifact_url,
        artifact_hash,
    )
}

fn write_frozen_lock_for_registry(
    context: &TestContext,
    canonical_index: &str,
    package: &str,
    version: &str,
    artifact_key: &str,
    artifact_url: &str,
    artifact_hash: &str,
) -> Result<()> {
    let artifact = match artifact_key {
        "wheels" => formatdoc! {r#"
            wheels = [
                {{ url = "{artifact_url}", hash = "sha256:{artifact_hash}" }},
            ]
            "#},
        "sdist" => {
            format!(r#"sdist = {{ url = "{artifact_url}", hash = "sha256:{artifact_hash}" }}"#)
        }
        key => anyhow::bail!("unsupported locked artifact kind `{key}`"),
    };

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
            source = {{ registry = "{canonical_index}" }}
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

fn assert_canonical_lock(
    lock: &str,
    canonical_index: &str,
    canonical_artifacts: &str,
) -> Result<()> {
    let lock_document: toml::Value = toml::from_str(lock)?;
    let package = lock_document
        .get("package")
        .and_then(toml::Value::as_array)
        .and_then(|packages| {
            packages.iter().find(|package| {
                package.get("name").and_then(toml::Value::as_str) == Some("basic-package")
            })
        })
        .ok_or_else(|| anyhow!("canonical lock does not contain `basic-package`"))?;

    assert_eq!(
        package
            .get("source")
            .and_then(|source| source.get("registry"))
            .and_then(toml::Value::as_str)
            .map(|registry| registry.trim_end_matches('/')),
        Some(canonical_index.trim_end_matches('/')),
        "the canonical index, not the physical index, must own the locked package",
    );
    assert_eq!(
        package
            .get("wheels")
            .and_then(toml::Value::as_array)
            .and_then(|wheels| wheels.first())
            .and_then(|wheel| wheel.get("url"))
            .and_then(toml::Value::as_str),
        Some(
            format!(
                "{}/{WHEEL_FILENAME}",
                canonical_artifacts.trim_end_matches('/')
            )
            .as_str()
        ),
        "wheel URLs must be canonical before lock serialization",
    );
    assert_eq!(
        package
            .get("sdist")
            .and_then(|source| source.get("url"))
            .and_then(toml::Value::as_str),
        Some(
            format!(
                "{}/{SOURCE_FILENAME}",
                canonical_artifacts.trim_end_matches('/')
            )
            .as_str()
        ),
        "source URLs must be canonical before lock serialization",
    );

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
    write_implicit_pypi_configuration(
        &context,
        &physical_index,
        &physical_artifacts,
        "basic-package==0.1.0",
    )?;

    let user_config_dir = context.user_config_dir.child("uv");
    user_config_dir.create_dir_all()?;
    user_config_dir
        .child("uv.toml")
        .write_str(indoc::indoc! {r#"
            [[index]]
            name = "socket"
            url = "https://other-proxy.example.com/simple/"
            artifact-base-url = "https://other-proxy.example.com/files/"
            proxy-for = "pypi"
        "#})?;

    uv_snapshot!(context.filters(), context.lock(), @r"
    exit_code: 2 (failure)
    ----- stderr -----
    error: More than one proxy index references the package index `https://pypi.org/simple`
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
    write_implicit_pypi_configuration(
        &context,
        &physical_index,
        &physical_artifacts,
        "basic-package==0.1.0",
    )?;

    let user_config_dir = context.user_config_dir.child("uv");
    user_config_dir.create_dir_all()?;
    user_config_dir
        .child("uv.toml")
        .write_str(indoc::indoc! {r#"
            [[index]]
            name = "socket"
            url = "https://ordinary.example.com/simple/"
        "#})?;

    uv_snapshot!(context.filters(), context.lock(), @r"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Proxy index `socket` shares its name with another index
    ");

    assert_no_requests(&physical_index, "physical proxy index").await?;
    assert_no_requests(&physical_artifacts, "physical artifact origin").await?;
    Ok(())
}

#[tokio::test]
async fn proxy_index_implicitly_routes_pypi_without_a_second_resolution_index() -> Result<()> {
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
    write_implicit_pypi_configuration(
        &context,
        &physical_index,
        &physical_artifacts,
        "basic-package==0.1.0",
    )?;

    uv_snapshot!(context.filters(), context.lock(), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    assert_canonical_lock(
        &lock,
        "https://pypi.org/simple/",
        "https://files.pythonhosted.org/packages/",
    )?;
    assert!(!lock.contains(&physical_index.uri()));
    assert!(!lock.contains(&physical_artifacts.uri()));

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
    ProxyConfiguration {
        canonical_index_url: canonical_index.uri(),
        canonical_artifact_url: canonical_artifacts.uri(),
        physical_index_url: format!(
            "http://proxy-user:proxy-password@{}",
            physical_index.address()
        ),
        physical_artifact_url: format!(
            "http://artifact-user:artifact-password@{}",
            physical_artifacts.address()
        ),
        dependency: "basic-package==0.1.0",
        dependency_metadata: None,
    }
    .write(&context)?;

    uv_snapshot!(context.filters(), context.lock(), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    assert_canonical_lock(
        &lock,
        &format!("{}/simple/", canonical_index.uri()),
        &format!("{}/packages/", canonical_artifacts.uri()),
    )?;
    assert!(!lock.contains(&physical_index.uri()));
    assert!(!lock.contains(&physical_artifacts.uri()));
    for credential in [
        "proxy-user",
        "proxy-password",
        "artifact-user",
        "artifact-password",
    ] {
        assert!(
            !lock.contains(credential),
            "physical proxy credential persisted in the lockfile: {credential}"
        );
    }

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
async fn proxy_index_reports_physical_authentication_errors_without_origin_requests() -> Result<()>
{
    for status in [401, 403] {
        let context = uv_test::test_context!("3.12");
        let canonical_index = MockServer::start().await;
        let canonical_artifacts = MockServer::start().await;
        let physical_index = MockServer::start().await;
        let physical_artifacts = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/simple/basic-package/"))
            .respond_with(ResponseTemplate::new(status))
            .expect(1)
            .mount(&physical_index)
            .await;
        ProxyConfiguration {
            canonical_index_url: canonical_index.uri(),
            canonical_artifact_url: canonical_artifacts.uri(),
            physical_index_url: physical_index.uri(),
            physical_artifact_url: physical_artifacts.uri(),
            dependency: "basic-package==0.1.0",
            dependency_metadata: None,
        }
        .write(&context)?;

        let physical_index_url = physical_index.uri();
        let mut filters = context.filters();
        filters.insert(0, (physical_index_url.as_str(), "[PHYSICAL_INDEX]"));

        let command = context.lock();
        match status {
            401 => {
                uv_snapshot!(filters, command, @r"
                exit_code: 1 (failure)
                ----- stderr -----
                  × No solution found when resolving dependencies:
                  ╰─▶ Because basic-package was not found in the package registry and your project depends on basic-package==0.1.0, we can conclude that your project's requirements are unsatisfiable.

                hint: An index URL ([PHYSICAL_INDEX]/simple/) could not be queried due to a lack of valid authentication credentials (401 Unauthorized)
                ");
            }
            403 => {
                uv_snapshot!(filters, command, @r"
                exit_code: 1 (failure)
                ----- stderr -----
                  × No solution found when resolving dependencies:
                  ╰─▶ Because basic-package was not found in the package registry and your project depends on basic-package==0.1.0, we can conclude that your project's requirements are unsatisfiable.

                hint: An index ([PHYSICAL_INDEX]/simple/) returned a 403 Forbidden error. Check that the index URL is correct and the credentials are valid.
                ");
            }
            _ => anyhow::bail!("unsupported proxy authentication status `{status}`"),
        }

        assert!(!context.temp_dir.child("uv.lock").exists());
        assert_requested_once(&physical_index, "/simple/basic-package/").await?;
        assert_no_requests(&physical_artifacts, "physical artifact origin").await?;
        assert_no_origin_requests(&canonical_index, &canonical_artifacts).await?;
    }

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
    ProxyConfiguration {
        canonical_index_url: canonical_index.uri(),
        canonical_artifact_url: canonical_artifacts.uri(),
        physical_index_url: physical_index.uri(),
        physical_artifact_url: physical_artifacts.uri(),
        dependency: "source-package==1.0.0",
        dependency_metadata: Some(("source-package", "1.0.0")),
    }
    .write(&context)?;

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
async fn proxy_index_installs_existing_locked_artifacts_without_simple_requests() -> Result<()> {
    for kind in [ArtifactKind::Wheel, ArtifactKind::Source] {
        let context = uv_test::test_context!("3.12");
        let artifact = kind.bytes(&context)?;
        let artifact_hash = sha256(&artifact);
        let canonical_index = MockServer::start().await;
        let canonical_artifacts = MockServer::start().await;
        let physical_index = MockServer::start().await;
        let physical_artifacts = MockServer::start().await;

        let dependency = format!("{}=={}", kind.package(), kind.version());
        ProxyConfiguration {
            canonical_index_url: canonical_index.uri(),
            canonical_artifact_url: canonical_artifacts.uri(),
            physical_index_url: physical_index.uri(),
            physical_artifact_url: physical_artifacts.uri(),
            dependency: &dependency,
            dependency_metadata: None,
        }
        .write(&context)?;
        write_frozen_lock(
            &context,
            &canonical_index,
            kind.package(),
            kind.version(),
            kind.lock_key(),
            &format!("{}/packages/{}", canonical_artifacts.uri(), kind.filename()),
            &artifact_hash,
        )?;
        let original_lock = context.read("uv.lock");
        mount_artifact(&physical_artifacts, kind.filename(), artifact, 1).await;

        let mut command = context.sync();
        command.arg("--frozen");
        match kind {
            ArtifactKind::Wheel => {
                uv_snapshot!(context.filters(), command, @r"
                exit_code: 0 (success)
                ----- stderr -----
                Prepared 1 package in [TIME]
                Installed 1 package in [TIME]
                 + basic-package==0.1.0
                ");
            }
            ArtifactKind::Source => {
                uv_snapshot!(context.filters(), command, @r"
                exit_code: 0 (success)
                ----- stderr -----
                Prepared 1 package in [TIME]
                Installed 1 package in [TIME]
                 + source-package==1.0.0
                ");
            }
        }

        assert_eq!(context.read("uv.lock"), original_lock);
        assert_requested_once(&physical_artifacts, &format!("/files/{}", kind.filename())).await?;
        assert_no_requests(&physical_index, "physical Simple index").await?;
        assert_no_origin_requests(&canonical_index, &canonical_artifacts).await?;
    }

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

    ProxyConfiguration {
        canonical_index_url: canonical_index.uri(),
        canonical_artifact_url: canonical_artifacts.uri(),
        physical_index_url: physical_index_a.uri(),
        physical_artifact_url: physical_artifacts_a.uri(),
        dependency: "basic-package==0.1.0",
        dependency_metadata: None,
    }
    .write(&context)?;
    write_frozen_lock(
        &context,
        &canonical_index,
        "basic-package",
        "0.1.0",
        "wheels",
        &format!("{}/packages/{WHEEL_FILENAME}", canonical_artifacts.uri()),
        WHEEL_HASH,
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

    ProxyConfiguration {
        canonical_index_url: canonical_index.uri(),
        canonical_artifact_url: canonical_artifacts.uri(),
        physical_index_url: physical_index_b.uri(),
        physical_artifact_url: physical_artifacts_b.uri(),
        dependency: "basic-package==0.1.0",
        dependency_metadata: None,
    }
    .write(&context)?;
    mount_artifact(&physical_artifacts_b, WHEEL_FILENAME, wheel, 1).await;

    uv_snapshot!(context.filters(), context.sync().arg("--frozen").arg("--reinstall"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     ~ basic-package==0.1.0
    ");

    assert_eq!(context.read("uv.lock"), original_lock);
    assert_requested_once(&physical_artifacts_a, &format!("/files/{WHEEL_FILENAME}")).await?;
    assert_requested_once(&physical_artifacts_b, &format!("/files/{WHEEL_FILENAME}")).await?;
    assert_no_requests(&physical_index_a, "original physical Simple index").await?;
    assert_no_requests(&physical_index_b, "replacement physical Simple index").await?;
    assert_no_origin_requests(&canonical_index, &canonical_artifacts).await?;
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
    ProxyConfiguration {
        canonical_index_url: format!(
            "http://canonical-user:canonical-password@{}",
            canonical_index.address()
        ),
        canonical_artifact_url: canonical_artifacts.uri(),
        physical_index_url: physical_index.uri(),
        physical_artifact_url: physical_artifacts.uri(),
        dependency: "basic-package==0.1.0",
        dependency_metadata: Some(("basic-package", "0.1.0")),
    }
    .write(&context)?;
    context
        .temp_dir
        .child("requirements.txt")
        .write_str("basic-package==0.1.0")?;

    uv_snapshot!(context.filters(), context
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
    index = "http://[LOCALHOST]/simple/"
    wheels = [{ url = "http://[LOCALHOST]/packages/basic_package-0.1.0-py3-none-any.whl", upload-time = 2024-03-24T00:00:00Z, hashes = { sha256 = "7b6229db79b5800e4e98a351b5628c1c8a944533a2d428aeeaa7275a30d4ea82" } }]

    ----- stderr -----
    Resolved 1 package in [TIME]
    "#);

    let lock = context.read("pylock.toml");
    let lock_document: toml::Value = toml::from_str(&lock)?;
    let package = lock_document
        .get("packages")
        .and_then(toml::Value::as_array)
        .and_then(|packages| {
            packages.iter().find(|package| {
                package.get("name").and_then(toml::Value::as_str) == Some("basic-package")
            })
        })
        .ok_or_else(|| anyhow!("canonical pylock does not contain `basic-package`"))?;
    assert_eq!(
        package.get("index").and_then(toml::Value::as_str),
        Some(format!("{}/simple/", canonical_index.uri()).as_str()),
        "pylock packages must retain their credential-free canonical index",
    );
    let wheel = package
        .get("wheels")
        .and_then(toml::Value::as_array)
        .and_then(|wheels| wheels.first())
        .ok_or_else(|| anyhow!("canonical pylock does not contain the selected wheel"))?;
    assert_eq!(
        wheel.get("url").and_then(toml::Value::as_str),
        Some(format!("{}/packages/{WHEEL_FILENAME}", canonical_artifacts.uri()).as_str()),
        "pylock artifacts must use their canonical artifact base",
    );
    assert_eq!(
        wheel
            .get("hashes")
            .and_then(|hashes| hashes.get("sha256"))
            .and_then(toml::Value::as_str),
        Some(WHEEL_HASH),
        "pylock artifacts must retain the supported hash",
    );
    assert!(!lock.contains(&physical_index.uri()));
    assert!(!lock.contains(&physical_artifacts.uri()));
    assert_no_requests(&physical_artifacts, "physical artifact origin").await?;

    mount_artifact(&physical_artifacts, WHEEL_FILENAME, artifact, 1).await;
    let context = context.with_cache_dir("install-cache");
    uv_snapshot!(context.filters(), context
        .pip_sync()
        .arg("--preview-features")
        .arg("pylock")
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
    ProxyConfiguration {
        canonical_index_url: canonical_index.uri(),
        canonical_artifact_url: canonical_artifacts.uri(),
        physical_index_url: physical_index.uri(),
        physical_artifact_url: physical_artifacts.uri(),
        dependency: "basic-package==0.1.0",
        dependency_metadata: None,
    }
    .write(&context)?;
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
async fn proxy_index_rejects_cached_artifacts_when_advertised_hash_changes() -> Result<()> {
    for kind in [ArtifactKind::Wheel, ArtifactKind::Source] {
        let context = uv_test::test_context!("3.12");
        let artifact = kind.bytes(&context)?;
        let actual_hash = sha256(&artifact);
        let changed_hash = "0000000000000000000000000000000000000000000000000000000000000000";
        let canonical_index = MockServer::start().await;
        let canonical_artifacts = MockServer::start().await;
        let physical_index = MockServer::start().await;
        let physical_artifacts = MockServer::start().await;
        let dependency = format!("{}=={}", kind.package(), kind.version());
        let artifact_url = format!("{}/files/{}", physical_artifacts.uri(), kind.filename());

        ProxyConfiguration {
            canonical_index_url: canonical_index.uri(),
            canonical_artifact_url: canonical_artifacts.uri(),
            physical_index_url: physical_index.uri(),
            physical_artifact_url: physical_artifacts.uri(),
            dependency: &dependency,
            dependency_metadata: Some((kind.package(), kind.version())),
        }
        .write(&context)?;
        context
            .temp_dir
            .child("requirements.txt")
            .write_str(&dependency)?;

        Mock::given(method("GET"))
            .and(path(format!("/simple/{}/", kind.package())))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("cache-control", "no-cache")
                    .set_body_raw(
                        json!({
                            "meta": { "api-version": "1.1" },
                            "name": kind.package(),
                            "files": [kind.advertised_file(&artifact_url, Some(&actual_hash))],
                        })
                        .to_string(),
                        "application/vnd.pypi.simple.v1+json",
                    ),
            )
            .expect(1)
            .mount(&physical_index)
            .await;
        mount_artifact(&physical_artifacts, kind.filename(), artifact.clone(), 1).await;

        match kind {
            ArtifactKind::Wheel => {
                uv_snapshot!(context.filters(), context.pip_sync().arg("requirements.txt"), @"
                exit_code: 0 (success)
                ----- stderr -----
                Resolved 1 package in [TIME]
                Prepared 1 package in [TIME]
                Installed 1 package in [TIME]
                 + basic-package==0.1.0
                ");
            }
            ArtifactKind::Source => {
                uv_snapshot!(context.filters(), context.pip_sync().arg("requirements.txt"), @"
                exit_code: 0 (success)
                ----- stderr -----
                Resolved 1 package in [TIME]
                Prepared 1 package in [TIME]
                Installed 1 package in [TIME]
                 + source-package==1.0.0
                ");
            }
        }

        physical_index.reset().await;
        physical_artifacts.reset().await;
        mount_simple(
            &physical_index,
            kind.package(),
            vec![kind.advertised_file(&artifact_url, Some(changed_hash))],
            1,
        )
        .await;
        mount_artifact(&physical_artifacts, kind.filename(), artifact, 1).await;

        let mut filters = context.filters();
        filters.push((actual_hash.as_str(), "[COMPUTED_HASH]"));
        let mut command = context.pip_sync();
        command.arg("requirements.txt").arg("--reinstall");

        match kind {
            ArtifactKind::Wheel => {
                uv_snapshot!(filters, command, @r"
                exit_code: 1 (failure)
                ----- stderr -----
                Resolved 1 package in [TIME]
                  × Failed to download `basic-package==0.1.0`
                  ╰─▶ Hash mismatch for `basic-package==0.1.0`

                      Expected:
                        sha256:0000000000000000000000000000000000000000000000000000000000000000

                      Computed:
                        sha256:[COMPUTED_HASH]
                ");
            }
            ArtifactKind::Source => {
                uv_snapshot!(filters, command, @r"
                exit_code: 1 (failure)
                ----- stderr -----
                Resolved 1 package in [TIME]
                  × Failed to download and build `source-package==1.0.0`
                  ╰─▶ Hash mismatch for `source-package==1.0.0`

                      Expected:
                        sha256:0000000000000000000000000000000000000000000000000000000000000000

                      Computed:
                        sha256:[COMPUTED_HASH]
                ");
            }
        }

        assert_requested_once(&physical_artifacts, &format!("/files/{}", kind.filename())).await?;
        assert_no_origin_requests(&canonical_index, &canonical_artifacts).await?;
    }

    Ok(())
}

#[tokio::test]
async fn proxy_index_rejects_hashless_selected_artifacts() -> Result<()> {
    for kind in [ArtifactKind::Wheel, ArtifactKind::Source] {
        let context = uv_test::test_context!("3.12");
        let canonical_index = MockServer::start().await;
        let canonical_artifacts = MockServer::start().await;
        let physical_index = MockServer::start().await;
        let physical_artifacts = MockServer::start().await;
        let artifact_url = format!("{}/files/{}", physical_artifacts.uri(), kind.filename());

        mount_simple(
            &physical_index,
            kind.package(),
            vec![kind.advertised_file(&artifact_url, None)],
            1,
        )
        .await;
        let dependency = format!("{}=={}", kind.package(), kind.version());
        ProxyConfiguration {
            canonical_index_url: canonical_index.uri(),
            canonical_artifact_url: canonical_artifacts.uri(),
            physical_index_url: physical_index.uri(),
            physical_artifact_url: physical_artifacts.uri(),
            dependency: &dependency,
            dependency_metadata: Some((kind.package(), kind.version())),
        }
        .write(&context)?;

        let command = context.lock();
        match kind {
            ArtifactKind::Wheel => {
                uv_snapshot!(context.filters(), command, @r"
                exit_code: 2 (failure)
                ----- stderr -----
                Resolved 2 packages in [TIME]
                error: Cannot lock `basic_package-0.1.0-py3-none-any.whl` for `basic-package` from proxy index `http://[LOCALHOST]/simple/` because it has no supported hash
                ");
            }
            ArtifactKind::Source => {
                uv_snapshot!(context.filters(), command, @r"
                exit_code: 2 (failure)
                ----- stderr -----
                Resolved 2 packages in [TIME]
                error: Cannot lock `source_package-1.0.0.zip` for `source-package` from proxy index `http://[LOCALHOST]/simple/` because it has no supported hash
                ");
            }
        }

        assert!(!context.temp_dir.child("uv.lock").exists());
        assert_requested_once(&physical_index, &format!("/simple/{}/", kind.package())).await?;
        assert_no_requests(&physical_artifacts, "physical artifact origin").await?;
        assert_no_origin_requests(&canonical_index, &canonical_artifacts).await?;
    }

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
    ProxyConfiguration {
        canonical_index_url: canonical_index.uri(),
        canonical_artifact_url: canonical_artifacts.uri(),
        physical_index_url: physical_index.uri(),
        physical_artifact_url: physical_artifacts.uri(),
        dependency: "basic-package==0.1.0",
        dependency_metadata: None,
    }
    .write(&context)?;

    uv_snapshot!(context.filters(), context
        .lock(), @r"
    exit_code: 2 (failure)
    ----- stderr -----
    error: No proxy artifact URL mapping matches `http://[LOCALHOST]/files/basic_package-0.1.0-py3-none-any.whl`
    ");

    assert!(!context.temp_dir.child("uv.lock").exists());
    assert_requested_once(&physical_index, "/simple/basic-package/").await?;
    assert_no_origin_requests(&canonical_index, &canonical_artifacts).await?;
    assert_no_requests(&physical_artifacts, "configured physical artifact origin").await?;
    assert_no_requests(&unmapped_artifacts, "unmapped physical artifact origin").await?;
    Ok(())
}

#[tokio::test]
async fn proxy_index_rejects_and_does_not_cache_locked_artifacts_with_mismatched_hash() -> Result<()>
{
    for kind in [ArtifactKind::Wheel, ArtifactKind::Source] {
        let context = uv_test::test_context!("3.12");
        let artifact = kind.bytes(&context)?;
        let expected_hash = "0000000000000000000000000000000000000000000000000000000000000000";
        let actual_hash = sha256(&artifact);
        let canonical_index = MockServer::start().await;
        let canonical_artifacts = MockServer::start().await;
        let physical_index = MockServer::start().await;
        let physical_artifacts = MockServer::start().await;
        let dependency = format!("{}=={}", kind.package(), kind.version());

        ProxyConfiguration {
            canonical_index_url: canonical_index.uri(),
            canonical_artifact_url: canonical_artifacts.uri(),
            physical_index_url: physical_index.uri(),
            physical_artifact_url: physical_artifacts.uri(),
            dependency: &dependency,
            dependency_metadata: None,
        }
        .write(&context)?;

        write_frozen_lock(
            &context,
            &canonical_index,
            kind.package(),
            kind.version(),
            kind.lock_key(),
            &format!("{}/packages/{}", canonical_artifacts.uri(), kind.filename()),
            expected_hash,
        )?;
        let original_lock = context.read("uv.lock");
        mount_artifact(&physical_artifacts, kind.filename(), artifact, 2).await;

        let mut filters = context.filters();
        filters.push((actual_hash.as_str(), "[COMPUTED_HASH]"));

        for _ in 0..2 {
            let mut command = context.sync();
            command.arg("--frozen");

            allow_duplicates! {
                match kind {
                    ArtifactKind::Wheel => {
                        uv_snapshot!(filters, command, @r"
                        exit_code: 1 (failure)
                        ----- stderr -----
                          × Failed to download `basic-package==0.1.0`
                          ╰─▶ Hash mismatch for `basic-package==0.1.0`

                              Expected:
                                sha256:0000000000000000000000000000000000000000000000000000000000000000

                              Computed:
                                sha256:[COMPUTED_HASH]

                        hint: `basic-package` (v0.1.0) was included because `project` (v0.1.0) depends on `basic-package`
                        ");
                    }
                    ArtifactKind::Source => {
                        uv_snapshot!(filters, command, @r"
                        exit_code: 1 (failure)
                        ----- stderr -----
                          × Failed to download and build `source-package==1.0.0`
                          ╰─▶ Hash mismatch for `source-package==1.0.0`

                              Expected:
                                sha256:0000000000000000000000000000000000000000000000000000000000000000

                              Computed:
                                sha256:[COMPUTED_HASH]

                        hint: `source-package` (v1.0.0) was included because `project` (v0.1.0) depends on `source-package`
                        ");
                    }
                }
            }
        }

        let requests = physical_artifacts
            .received_requests()
            .await
            .ok_or_else(|| anyhow!("physical artifact origin should record requests"))?;
        assert_eq!(
            requests
                .iter()
                .filter(|request| request.url.path() == format!("/files/{}", kind.filename()))
                .count(),
            2,
            "an artifact with an incorrect locked digest must not enter the cache",
        );
        assert_eq!(context.read("uv.lock"), original_lock);
        assert_no_requests(&physical_index, "physical Simple index").await?;
        assert_no_origin_requests(&canonical_index, &canonical_artifacts).await?;
    }

    Ok(())
}
