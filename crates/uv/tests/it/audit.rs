use assert_cmd::assert::OutputAssertExt;
use assert_fs::prelude::*;
use indoc::{formatdoc, indoc};
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use uv_test::uv_snapshot;

fn write_audit_json_project(context: &uv_test::TestContext, index_url: &str) {
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml
        .write_str(&formatdoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig==2.0.0"]

        [[tool.uv.index]]
        url = "{index_url}"
        default = true
    "#})
        .unwrap();

    let lockfile = context.temp_dir.child("uv.lock");
    lockfile
        .write_str(&formatdoc! {r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = {{ registry = "{index_url}" }}
        sdist = {{ url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646, upload-time = "2023-01-07T11:08:11.254Z" }}
        wheels = [
            {{ url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892, upload-time = "2023-01-07T11:08:09.864Z" }},
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = {{ virtual = "." }}
        dependencies = [
            {{ name = "iniconfig" }},
        ]

        [package.metadata]
        requires-dist = [
            {{ name = "iniconfig", specifier = "==2.0.0" }},
        ]
    "#})
        .unwrap();
}

/// Audit a project with no vulnerabilities found.
#[tokio::test]
async fn audit_no_vulnerabilities() {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig==2.0.0"]
    "#})
        .unwrap();

    context.lock().assert().success();

    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/querybatch"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "results": [{"vulns": []}]
        })))
        .mount(&server)
        .await;

    uv_snapshot!(context.filters(), context
        .audit()
        .arg("--preview")
        .arg("--service-url")
        .arg(server.uri()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Found no known vulnerabilities and no adverse project statuses in 1 package
    ");
}

/// Audit a project with no vulnerabilities found, emitting JSON output.
#[tokio::test]
async fn audit_json_no_vulnerabilities() {
    let context = uv_test::test_context!("3.12");
    let proxy = crate::pypi_proxy::start().await;
    write_audit_json_project(&context, &proxy.url("/simple"));

    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/querybatch"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "results": [{"vulns": []}]
        })))
        .mount(&server)
        .await;

    uv_snapshot!(context.filters(), context
        .audit()
        .arg("--preview-features")
        .arg("audit,json-output")
        .arg("--output-format")
        .arg("json")
        .arg("--frozen")
        .arg("--service-url")
        .arg(server.uri()), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "schema": {
        "version": "preview"
      },
      "summary": {
        "audited_packages": 1,
        "vulnerabilities": 0,
        "adverse_statuses": 0
      },
      "vulnerabilities": [],
      "adverse_statuses": []
    }

    ----- stderr -----
    "#);
}

/// Requesting JSON output warns unless the JSON preview feature is enabled.
#[tokio::test]
async fn audit_json_preview_warning() {
    let context = uv_test::test_context!("3.12");
    let proxy = crate::pypi_proxy::start().await;
    write_audit_json_project(&context, &proxy.url("/simple"));

    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/querybatch"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "results": [{"vulns": []}]
        })))
        .mount(&server)
        .await;

    uv_snapshot!(context.filters(), context
        .audit()
        .arg("--preview-features")
        .arg("audit")
        .arg("--output-format")
        .arg("json")
        .arg("--frozen")
        .arg("--service-url")
        .arg(server.uri()), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "schema": {
        "version": "preview"
      },
      "summary": {
        "audited_packages": 1,
        "vulnerabilities": 0,
        "adverse_statuses": 0
      },
      "vulnerabilities": [],
      "adverse_statuses": []
    }

    ----- stderr -----
    warning: The `--output-format json` option is experimental and the schema may change without warning. Pass `--preview-features json-output` to disable this warning.
    "#);
}

/// Audit a project and find a single vulnerability with summary, fix version, and advisory link.
#[tokio::test]
async fn audit_vulnerability_found() {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig==2.0.0"]
    "#})
        .unwrap();

    context.lock().assert().success();

    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/querybatch"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "results": [{"vulns": [{"id": "PYSEC-2023-0001"}]}]
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/v1/vulns/PYSEC-2023-0001"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "PYSEC-2023-0001",
            "modified": "2026-01-01T00:00:00Z",
            "summary": "A test vulnerability in iniconfig",
            "affected": [{
                "ranges": [{
                    "type": "ECOSYSTEM",
                    "events": [
                        {"introduced": "0"},
                        {"fixed": "2.1.0"}
                    ]
                }]
            }],
            "references": [{
                "type": "ADVISORY",
                "url": "https://example.com/advisory/PYSEC-2023-0001"
            }]
        })))
        .mount(&server)
        .await;

    uv_snapshot!(context.filters(), context
        .audit()
        .arg("--preview")
        .arg("--service-url")
        .arg(server.uri()), @"
    success: false
    exit_code: 1
    ----- stdout -----

    Vulnerabilities:

    iniconfig 2.0.0 has 1 known vulnerability:

    - PYSEC-2023-0001: A test vulnerability in iniconfig

      Fixed in: 2.1.0

      Advisory information: https://example.com/advisory/PYSEC-2023-0001


    ----- stderr -----
    Resolved 2 packages in [TIME]
    Found 1 known vulnerability and no adverse project statuses in 1 package
    ");
}

/// Audit a project with no dependencies.
#[tokio::test]
async fn audit_no_dependencies() {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})
        .unwrap();

    context.lock().assert().success();

    let server = MockServer::start().await;

    // No querybatch call expected since there are no dependencies to audit.

    uv_snapshot!(context.filters(), context
        .audit()
        .arg("--preview")
        .arg("--service-url")
        .arg(server.uri()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Found no known vulnerabilities and no adverse project statuses in 0 packages
    ");
}

/// When a vulnerability has aliases, the best ID (PYSEC > GHSA > CVE) is displayed.
#[tokio::test]
async fn audit_best_id_selection() {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig==2.0.0"]
    "#})
        .unwrap();

    context.lock().assert().success();

    let server = MockServer::start().await;

    // The primary ID is an OSV ID, but aliases include a PYSEC ID which should be preferred.
    Mock::given(method("POST"))
        .and(path("/v1/querybatch"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "results": [{"vulns": [{"id": "OSV-2023-0001"}]}]
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/v1/vulns/OSV-2023-0001"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "OSV-2023-0001",
            "modified": "2026-01-01T00:00:00Z",
            "summary": "A vulnerability with many aliases",
            "aliases": ["PYSEC-2023-0042", "CVE-2023-9999", "GHSA-xxxx-yyyy-zzzz"]
        })))
        .mount(&server)
        .await;

    // The output should show PYSEC-2023-0042 as the display ID (PYSEC preferred over GHSA, CVE).
    uv_snapshot!(context.filters(), context
        .audit()
        .arg("--preview")
        .arg("--service-url")
        .arg(server.uri()), @"
    success: false
    exit_code: 1
    ----- stdout -----

    Vulnerabilities:

    iniconfig 2.0.0 has 1 known vulnerability:

    - PYSEC-2023-0042: A vulnerability with many aliases

      No fix versions available

      Advisory information: https://osv.dev/vulnerability/OSV-2023-0001


    ----- stderr -----
    Resolved 2 packages in [TIME]
    Found 1 known vulnerability and no adverse project statuses in 1 package
    ");
}

/// A vulnerability without fix versions shows "No fix versions available".
#[tokio::test]
async fn audit_no_fix_versions() {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig==2.0.0"]
    "#})
        .unwrap();

    context.lock().assert().success();

    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/querybatch"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "results": [{"vulns": [{"id": "VULN-NO-FIX"}]}]
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/v1/vulns/VULN-NO-FIX"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "VULN-NO-FIX",
            "modified": "2026-01-01T00:00:00Z",
            "summary": "A vulnerability with no fix available"
        })))
        .mount(&server)
        .await;

    uv_snapshot!(context.filters(), context
        .audit()
        .arg("--preview")
        .arg("--service-url")
        .arg(server.uri()), @"
    success: false
    exit_code: 1
    ----- stdout -----

    Vulnerabilities:

    iniconfig 2.0.0 has 1 known vulnerability:

    - VULN-NO-FIX: A vulnerability with no fix available

      No fix versions available

      Advisory information: https://osv.dev/vulnerability/VULN-NO-FIX


    ----- stderr -----
    Resolved 2 packages in [TIME]
    Found 1 known vulnerability and no adverse project statuses in 1 package
    ");
}

/// Multiple vulnerabilities on the same package are grouped together.
#[tokio::test]
async fn audit_multiple_vulnerabilities_same_package() {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig==2.0.0"]
    "#})
        .unwrap();

    context.lock().assert().success();

    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/querybatch"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "results": [{"vulns": [{"id": "VULN-A"}, {"id": "VULN-B"}]}]
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/v1/vulns/VULN-A"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "VULN-A",
            "modified": "2026-01-01T00:00:00Z",
            "summary": "First vulnerability",
            "affected": [{
                "ranges": [{
                    "type": "ECOSYSTEM",
                    "events": [
                        {"introduced": "0"},
                        {"fixed": "2.1.0"}
                    ]
                }]
            }],
            "references": [{
                "type": "ADVISORY",
                "url": "https://example.com/advisory/VULN-A"
            }]
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/v1/vulns/VULN-B"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "VULN-B",
            "modified": "2026-01-02T00:00:00Z",
            "summary": "Second vulnerability",
            "affected": [{
                "ranges": [{
                    "type": "ECOSYSTEM",
                    "events": [
                        {"introduced": "2.0.0"},
                        {"fixed": "2.0.1"}
                    ]
                }]
            }],
            "references": [{
                "type": "WEB",
                "url": "https://example.com/web/VULN-B"
            }]
        })))
        .mount(&server)
        .await;

    uv_snapshot!(context.filters(), context
        .audit()
        .arg("--preview")
        .arg("--service-url")
        .arg(server.uri()), @"
    success: false
    exit_code: 1
    ----- stdout -----

    Vulnerabilities:

    iniconfig 2.0.0 has 2 known vulnerabilities:

    - VULN-A: First vulnerability

      Fixed in: 2.1.0

      Advisory information: https://example.com/advisory/VULN-A

    - VULN-B: Second vulnerability

      Fixed in: 2.0.1

      Advisory information: https://example.com/web/VULN-B


    ----- stderr -----
    Resolved 2 packages in [TIME]
    Found 2 known vulnerabilities and no adverse project statuses in 1 package
    ");
}

/// `--no-dev` excludes dev dependencies from the audit.
#[tokio::test]
async fn audit_no_dev() {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig==2.0.0"]

        [dependency-groups]
        dev = ["typing-extensions==4.10.0"]
    "#})
        .unwrap();

    context.lock().assert().success();

    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/querybatch"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "results": [{"vulns": []}]
        })))
        .mount(&server)
        .await;

    // With --no-dev, only "iniconfig" should be audited (not "typing-extensions").
    uv_snapshot!(context.filters(), context
        .audit()
        .arg("--preview")
        .arg("--no-dev")
        .arg("--service-url")
        .arg(server.uri()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Found no known vulnerabilities and no adverse project statuses in 1 package
    ");

    // Without --no-dev, both packages should be audited.
    uv_snapshot!(context.filters(), context
        .audit()
        .arg("--preview")
        .arg("--service-url")
        .arg(server.uri()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Found no known vulnerabilities and no adverse project statuses in 2 packages
    ");
}

/// Extras are included in the audit by default, and can be excluded.
#[tokio::test]
async fn audit_extras() {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig==2.0.0"]

        [project.optional-dependencies]
        web = ["typing-extensions==4.10.0"]
    "#})
        .unwrap();

    context.lock().assert().success();

    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/querybatch"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "results": [{"vulns": []}]
        })))
        .mount(&server)
        .await;

    // By default, extras are included: both iniconfig and typing-extensions are audited.
    uv_snapshot!(context.filters(), context
        .audit()
        .arg("--preview")
        .arg("--service-url")
        .arg(server.uri()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Found no known vulnerabilities and no adverse project statuses in 2 packages
    ");

    // With --no-extra web, only iniconfig should be audited.
    uv_snapshot!(context.filters(), context
        .audit()
        .arg("--preview")
        .arg("--no-extra")
        .arg("web")
        .arg("--service-url")
        .arg(server.uri()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Found no known vulnerabilities and no adverse project statuses in 1 package
    ");
}

/// Non-default dependency groups are included when explicitly requested.
#[tokio::test]
async fn audit_dependency_groups() {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig==2.0.0"]

        [dependency-groups]
        dev = ["typing-extensions==4.10.0"]
        lint = ["sniffio==1.3.1"]

    "#})
        .unwrap();

    context.lock().assert().success();

    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/querybatch"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "results": [{"vulns": []}]
        })))
        .mount(&server)
        .await;

    // Default: all groups are included (iniconfig + typing-extensions + sniffio = 3).
    uv_snapshot!(context.filters(), context
        .audit()
        .arg("--preview")
        .arg("--service-url")
        .arg(server.uri()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Found no known vulnerabilities and no adverse project statuses in 3 packages
    ");

    // --no-dev: excludes the dev group (iniconfig + sniffio = 2).
    uv_snapshot!(context.filters(), context
        .audit()
        .arg("--preview")
        .arg("--no-dev")
        .arg("--service-url")
        .arg(server.uri()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Found no known vulnerabilities and no adverse project statuses in 2 packages
    ");

    // --no-group lint: excludes the lint group (iniconfig + typing-extensions = 2).
    uv_snapshot!(context.filters(), context
        .audit()
        .arg("--preview")
        .arg("--no-group")
        .arg("lint")
        .arg("--service-url")
        .arg(server.uri()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Found no known vulnerabilities and no adverse project statuses in 2 packages
    ");

    // --only-group lint: only the "lint" group, project deps omitted (sniffio = 1).
    uv_snapshot!(context.filters(), context
        .audit()
        .arg("--preview")
        .arg("--only-group")
        .arg("lint")
        .arg("--service-url")
        .arg(server.uri()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Found no known vulnerabilities and no adverse project statuses in 1 package
    ");
}

/// `--ignore` excludes a vulnerability by its primary ID.
#[tokio::test]
async fn audit_ignore_by_id() {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig==2.0.0"]
    "#})
        .unwrap();

    context.lock().assert().success();

    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/querybatch"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "results": [{"vulns": [{"id": "PYSEC-2023-0001"}]}]
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/v1/vulns/PYSEC-2023-0001"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "PYSEC-2023-0001",
            "modified": "2026-01-01T00:00:00Z",
            "summary": "A test vulnerability in iniconfig",
            "affected": [{
                "ranges": [{
                    "type": "ECOSYSTEM",
                    "events": [
                        {"introduced": "0"},
                        {"fixed": "2.1.0"}
                    ]
                }]
            }]
        })))
        .mount(&server)
        .await;

    // Without --ignore, the vulnerability is reported.
    uv_snapshot!(context.filters(), context
        .audit()
        .arg("--preview")
        .arg("--service-url")
        .arg(server.uri()), @"
    success: false
    exit_code: 1
    ----- stdout -----

    Vulnerabilities:

    iniconfig 2.0.0 has 1 known vulnerability:

    - PYSEC-2023-0001: A test vulnerability in iniconfig

      Fixed in: 2.1.0

      Advisory information: https://osv.dev/vulnerability/PYSEC-2023-0001


    ----- stderr -----
    Resolved 2 packages in [TIME]
    Found 1 known vulnerability and no adverse project statuses in 1 package
    ");

    // With --ignore, the vulnerability is suppressed.
    uv_snapshot!(context.filters(), context
        .audit()
        .arg("--preview")
        .arg("--ignore")
        .arg("PYSEC-2023-0001")
        .arg("--service-url")
        .arg(server.uri()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Found no known vulnerabilities and no adverse project statuses in 1 package
    ");
}

/// `--ignore` matches against aliases, not just the primary ID.
#[tokio::test]
async fn audit_ignore_by_alias() {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig==2.0.0"]
    "#})
        .unwrap();

    context.lock().assert().success();

    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/querybatch"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "results": [{"vulns": [{"id": "OSV-2023-0001"}]}]
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/v1/vulns/OSV-2023-0001"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "OSV-2023-0001",
            "modified": "2026-01-01T00:00:00Z",
            "summary": "A vulnerability with aliases",
            "aliases": ["PYSEC-2023-0042", "CVE-2023-9999"]
        })))
        .mount(&server)
        .await;

    // Ignoring by alias (CVE-2023-9999) should suppress the vulnerability.
    uv_snapshot!(context.filters(), context
        .audit()
        .arg("--preview")
        .arg("--ignore")
        .arg("CVE-2023-9999")
        .arg("--service-url")
        .arg(server.uri()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Found no known vulnerabilities and no adverse project statuses in 1 package
    ");
}

/// `--ignore-until-fixed` suppresses a vulnerability only when no fix versions are available.
#[tokio::test]
async fn audit_ignore_until_fixed() {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig==2.0.0"]
    "#})
        .unwrap();

    context.lock().assert().success();

    let server = MockServer::start().await;

    // A vulnerability with no fix versions.
    Mock::given(method("POST"))
        .and(path("/v1/querybatch"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "results": [{"vulns": [{"id": "VULN-NO-FIX"}]}]
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/v1/vulns/VULN-NO-FIX"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "VULN-NO-FIX",
            "modified": "2026-01-01T00:00:00Z",
            "summary": "A vulnerability with no fix available"
        })))
        .mount(&server)
        .await;

    // With --ignore-until-fixed and no fix versions, the vulnerability is suppressed.
    uv_snapshot!(context.filters(), context
        .audit()
        .arg("--preview")
        .arg("--ignore-until-fixed")
        .arg("VULN-NO-FIX")
        .arg("--service-url")
        .arg(server.uri()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Found no known vulnerabilities and no adverse project statuses in 1 package
    ");
}

/// `--ignore-until-fixed` stops suppressing once a fix version is available.
#[tokio::test]
async fn audit_ignore_until_fixed_with_fix() {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig==2.0.0"]
    "#})
        .unwrap();

    context.lock().assert().success();

    let server = MockServer::start().await;

    // A vulnerability WITH fix versions.
    Mock::given(method("POST"))
        .and(path("/v1/querybatch"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "results": [{"vulns": [{"id": "PYSEC-2023-0001"}]}]
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/v1/vulns/PYSEC-2023-0001"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "PYSEC-2023-0001",
            "modified": "2026-01-01T00:00:00Z",
            "summary": "A test vulnerability in iniconfig",
            "affected": [{
                "ranges": [{
                    "type": "ECOSYSTEM",
                    "events": [
                        {"introduced": "0"},
                        {"fixed": "2.1.0"}
                    ]
                }]
            }]
        })))
        .mount(&server)
        .await;

    // With --ignore-until-fixed but a fix IS available, the vulnerability is still reported.
    uv_snapshot!(context.filters(), context
        .audit()
        .arg("--preview")
        .arg("--ignore-until-fixed")
        .arg("PYSEC-2023-0001")
        .arg("--service-url")
        .arg(server.uri()), @"
    success: false
    exit_code: 1
    ----- stdout -----

    Vulnerabilities:

    iniconfig 2.0.0 has 1 known vulnerability:

    - PYSEC-2023-0001: A test vulnerability in iniconfig

      Fixed in: 2.1.0

      Advisory information: https://osv.dev/vulnerability/PYSEC-2023-0001


    ----- stderr -----
    Resolved 2 packages in [TIME]
    Found 1 known vulnerability and no adverse project statuses in 1 package
    ");
}

/// `[tool.uv.audit]` config supports `ignore`.
#[tokio::test]
async fn audit_ignore_config() {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig==2.0.0"]

        [tool.uv.audit]
        ignore = ["PYSEC-2023-0001"]
    "#})
        .unwrap();

    context.lock().assert().success();

    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/querybatch"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "results": [{"vulns": [{"id": "PYSEC-2023-0001"}]}]
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/v1/vulns/PYSEC-2023-0001"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "PYSEC-2023-0001",
            "modified": "2026-01-01T00:00:00Z",
            "summary": "A test vulnerability in iniconfig",
            "affected": [{
                "ranges": [{
                    "type": "ECOSYSTEM",
                    "events": [
                        {"introduced": "0"},
                        {"fixed": "2.1.0"}
                    ]
                }]
            }]
        })))
        .mount(&server)
        .await;

    // The vulnerability is suppressed by the config.
    uv_snapshot!(context.filters(), context
        .audit()
        .arg("--preview")
        .arg("--service-url")
        .arg(server.uri()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Found no known vulnerabilities and no adverse project statuses in 1 package
    ");
}

/// `[tool.uv.audit]` config supports `ignore-until-fixed`.
#[tokio::test]
async fn audit_ignore_until_fixed_config() {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig==2.0.0"]

        [tool.uv.audit]
        ignore-until-fixed = ["VULN-NO-FIX"]
    "#})
        .unwrap();

    context.lock().assert().success();

    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/querybatch"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "results": [{"vulns": [{"id": "VULN-NO-FIX"}]}]
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/v1/vulns/VULN-NO-FIX"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "VULN-NO-FIX",
            "modified": "2026-01-01T00:00:00Z",
            "summary": "A vulnerability with no fix available"
        })))
        .mount(&server)
        .await;

    // The vulnerability is suppressed because no fix is available.
    uv_snapshot!(context.filters(), context
        .audit()
        .arg("--preview")
        .arg("--service-url")
        .arg(server.uri()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Found no known vulnerabilities and no adverse project statuses in 1 package
    ");
}

/// `--ignore` only suppresses the targeted vulnerability, not others.
#[tokio::test]
async fn audit_ignore_partial() {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig==2.0.0"]
    "#})
        .unwrap();

    context.lock().assert().success();

    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/querybatch"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "results": [{"vulns": [{"id": "VULN-A"}, {"id": "VULN-B"}]}]
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/v1/vulns/VULN-A"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "VULN-A",
            "modified": "2026-01-01T00:00:00Z",
            "summary": "First vulnerability",
            "affected": [{
                "ranges": [{
                    "type": "ECOSYSTEM",
                    "events": [
                        {"introduced": "0"},
                        {"fixed": "2.1.0"}
                    ]
                }]
            }]
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/v1/vulns/VULN-B"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "VULN-B",
            "modified": "2026-01-02T00:00:00Z",
            "summary": "Second vulnerability",
            "affected": [{
                "ranges": [{
                    "type": "ECOSYSTEM",
                    "events": [
                        {"introduced": "2.0.0"},
                        {"fixed": "2.0.1"}
                    ]
                }]
            }]
        })))
        .mount(&server)
        .await;

    // Ignoring VULN-A should still report VULN-B.
    uv_snapshot!(context.filters(), context
        .audit()
        .arg("--preview")
        .arg("--ignore")
        .arg("VULN-A")
        .arg("--service-url")
        .arg(server.uri()), @"
    success: false
    exit_code: 1
    ----- stdout -----

    Vulnerabilities:

    iniconfig 2.0.0 has 1 known vulnerability:

    - VULN-B: Second vulnerability

      Fixed in: 2.0.1

      Advisory information: https://osv.dev/vulnerability/VULN-B


    ----- stderr -----
    Resolved 2 packages in [TIME]
    Found 1 known vulnerability and no adverse project statuses in 1 package
    ");
}

/// `--ignore` warns when the ignored ID doesn't match any vulnerability.
#[tokio::test]
async fn audit_ignore_unmatched() {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig==2.0.0"]
    "#})
        .unwrap();

    context.lock().assert().success();

    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/querybatch"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "results": [{"vulns": []}]
        })))
        .mount(&server)
        .await;

    // Ignoring a non-existent vulnerability should warn.
    uv_snapshot!(context.filters(), context
        .audit()
        .arg("--preview")
        .arg("--ignore")
        .arg("CVE-XXXX-YYYY")
        .arg("--service-url")
        .arg(server.uri()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    warning: Ignored vulnerability `CVE-XXXX-YYYY` does not match any vulnerability in the project
    Found no known vulnerabilities and no adverse project statuses in 1 package
    ");
}

/// `--ignore-until-fixed` warns when the ignored ID doesn't match any vulnerability.
#[tokio::test]
async fn audit_ignore_until_fixed_unmatched() {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig==2.0.0"]
    "#})
        .unwrap();

    context.lock().assert().success();

    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/querybatch"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "results": [{"vulns": []}]
        })))
        .mount(&server)
        .await;

    // Ignoring a non-existent vulnerability with --ignore-until-fixed should warn.
    uv_snapshot!(context.filters(), context
        .audit()
        .arg("--preview")
        .arg("--ignore-until-fixed")
        .arg("CVE-XXXX-YYYY")
        .arg("--service-url")
        .arg(server.uri()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    warning: Ignored vulnerability `CVE-XXXX-YYYY` does not match any vulnerability in the project
    Found no known vulnerabilities and no adverse project statuses in 1 package
    ");
}

/// `--ignore` with multiple IDs warns only about the unmatched ones.
#[tokio::test]
async fn audit_ignore_mixed_matched_unmatched() {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig==2.0.0"]
    "#})
        .unwrap();

    context.lock().assert().success();

    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/querybatch"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "results": [{"vulns": [{"id": "PYSEC-2023-0001"}]}]
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/v1/vulns/PYSEC-2023-0001"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "PYSEC-2023-0001",
            "modified": "2026-01-01T00:00:00Z",
            "summary": "A test vulnerability in iniconfig",
            "affected": [{
                "ranges": [{
                    "type": "ECOSYSTEM",
                    "events": [
                        {"introduced": "0"},
                        {"fixed": "2.1.0"}
                    ]
                }]
            }]
        })))
        .mount(&server)
        .await;

    // Ignore one real and one non-existent vulnerability: the real one is suppressed,
    // and the non-existent one triggers a warning.
    uv_snapshot!(context.filters(), context
        .audit()
        .arg("--preview")
        .arg("--ignore")
        .arg("PYSEC-2023-0001")
        .arg("--ignore")
        .arg("CVE-DOES-NOT-EXIST")
        .arg("--service-url")
        .arg(server.uri()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    warning: Ignored vulnerability `CVE-DOES-NOT-EXIST` does not match any vulnerability in the project
    Found no known vulnerabilities and no adverse project statuses in 1 package
    ");
}

/// Audit a PEP 723 script with a single dependency and no vulnerabilities.
#[tokio::test]
async fn audit_script_no_vulnerabilities() {
    let context = uv_test::test_context!("3.12");

    let script = context.temp_dir.child("script.py");
    script
        .write_str(indoc! {r#"
        # /// script
        # requires-python = ">=3.12"
        # dependencies = [
        #   "iniconfig==2.0.0",
        # ]
        # ///
        import iniconfig
    "#})
        .unwrap();

    let lockfile = context.temp_dir.child("script.py.lock");
    lockfile
        .write_str(indoc! {r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        requirements = [{ name = "iniconfig", specifier = "==2.0.0" }]

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646, upload-time = "2023-01-07T12:52:09.585Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beebd3ce04b132a8bf3491/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892, upload-time = "2023-01-07T12:52:07.538Z" },
        ]
    "#})
        .unwrap();

    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/querybatch"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "results": [{"vulns": []}]
        })))
        .mount(&server)
        .await;

    uv_snapshot!(context.filters(), context
        .audit()
        .arg("--preview")
        .arg("--script")
        .arg("script.py")
        .arg("--service-url")
        .arg(server.uri()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Found no known vulnerabilities and no adverse project statuses in 1 package
    ");
}

/// Audit a PEP 723 script and find a vulnerability.
#[tokio::test]
async fn audit_script_vulnerability_found() {
    let context = uv_test::test_context!("3.12");

    let script = context.temp_dir.child("script.py");
    script
        .write_str(indoc! {r#"
        # /// script
        # requires-python = ">=3.12"
        # dependencies = [
        #   "iniconfig==2.0.0",
        # ]
        # ///
        import iniconfig
    "#})
        .unwrap();

    let lockfile = context.temp_dir.child("script.py.lock");
    lockfile
        .write_str(indoc! {r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        requirements = [{ name = "iniconfig", specifier = "==2.0.0" }]

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646, upload-time = "2023-01-07T12:52:09.585Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beebd3ce04b132a8bf3491/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892, upload-time = "2023-01-07T12:52:07.538Z" },
        ]
    "#})
        .unwrap();

    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/querybatch"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "results": [{"vulns": [{"id": "PYSEC-2023-0001"}]}]
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/v1/vulns/PYSEC-2023-0001"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "PYSEC-2023-0001",
            "modified": "2026-01-01T00:00:00Z",
            "summary": "A test vulnerability in iniconfig",
            "affected": [{
                "ranges": [{
                    "type": "ECOSYSTEM",
                    "events": [
                        {"introduced": "0"},
                        {"fixed": "2.1.0"}
                    ]
                }]
            }],
            "references": [{
                "type": "ADVISORY",
                "url": "https://example.com/advisory/PYSEC-2023-0001"
            }]
        })))
        .mount(&server)
        .await;

    uv_snapshot!(context.filters(), context
        .audit()
        .arg("--preview")
        .arg("--script")
        .arg("script.py")
        .arg("--service-url")
        .arg(server.uri()), @"
    success: false
    exit_code: 1
    ----- stdout -----

    Vulnerabilities:

    iniconfig 2.0.0 has 1 known vulnerability:

    - PYSEC-2023-0001: A test vulnerability in iniconfig

      Fixed in: 2.1.0

      Advisory information: https://example.com/advisory/PYSEC-2023-0001


    ----- stderr -----
    Resolved 1 package in [TIME]
    Found 1 known vulnerability and no adverse project statuses in 1 package
    ");
}

/// Audit a PEP 723 script with no dependencies.
#[tokio::test]
async fn audit_script_no_dependencies() {
    let context = uv_test::test_context!("3.12");

    let script = context.temp_dir.child("script.py");
    script
        .write_str(indoc! {r#"
        # /// script
        # requires-python = ">=3.12"
        # dependencies = []
        # ///
        print("hello")
    "#})
        .unwrap();

    let lockfile = context.temp_dir.child("script.py.lock");
    lockfile
        .write_str(indoc! {r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        requirements = []
    "#})
        .unwrap();

    let server = MockServer::start().await;

    uv_snapshot!(context.filters(), context
        .audit()
        .arg("--preview")
        .arg("--script")
        .arg("script.py")
        .arg("--service-url")
        .arg(server.uri()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved in [TIME]
    Found no known vulnerabilities and no adverse project statuses in 0 packages
    ");
}

/// Audit a PEP 723 script with --frozen but no lockfile should error.
#[tokio::test]
async fn audit_script_frozen_missing_lockfile() {
    let context = uv_test::test_context!("3.12");

    let script = context.temp_dir.child("script.py");
    script
        .write_str(indoc! {r#"
        # /// script
        # requires-python = ">=3.12"
        # dependencies = [
        #   "iniconfig==2.0.0",
        # ]
        # ///
        import iniconfig
    "#})
        .unwrap();

    // No lockfile written — --frozen should fail.
    let server = MockServer::start().await;

    uv_snapshot!(context.filters(), context
        .audit()
        .arg("--frozen")
        .arg("--preview")
        .arg("--script")
        .arg("script.py")
        .arg("--service-url")
        .arg(server.uri()), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Unable to find lockfile at `script.py.lock`, but `--frozen` was provided. To create a lockfile, run `uv lock` or `uv sync` without the flag.
    ");
}

/// Audit a PEP 723 script with multiple dependencies.
#[tokio::test]
async fn audit_script_multiple_dependencies() {
    let context = uv_test::test_context!("3.12");

    let script = context.temp_dir.child("script.py");
    script
        .write_str(indoc! {r#"
        # /// script
        # requires-python = ">=3.12"
        # dependencies = [
        #   "iniconfig==2.0.0",
        #   "typing-extensions==4.10.0",
        # ]
        # ///
        import iniconfig
    "#})
        .unwrap();

    let lockfile = context.temp_dir.child("script.py.lock");
    lockfile
        .write_str(indoc! {r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        requirements = [
            { name = "iniconfig", specifier = "==2.0.0" },
            { name = "typing-extensions", specifier = "==4.10.0" },
        ]

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646, upload-time = "2023-01-07T12:52:09.585Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beebd3ce04b132a8bf3491/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892, upload-time = "2023-01-07T12:52:07.538Z" },
        ]

        [[package]]
        name = "typing-extensions"
        version = "4.10.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/16/3a/0d26ce356c7465a19c9ea8f4940a02f85f2b05e6e22cf5fd8a7c1d3b5e0b/typing_extensions-4.10.0.tar.gz", hash = "sha256:b0abd7c89e8fb96f98db18d86106ff1d90ab692004eb746cf6eda2682f91b3cb", size = 77558, upload-time = "2024-02-24T00:10:00.000Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/f9/de/dc04a3ea60b22624b51c703a84bbe0184abcd1d0b9bc8074b5d6b7ab90bb/typing_extensions-4.10.0-py3-none-any.whl", hash = "sha256:69b1a937c3a517342112fb4c6df7e72fc39a38e7891a5730ed4985b5214b5475", size = 33926, upload-time = "2024-02-24T00:09:57.000Z" },
        ]
    "#})
        .unwrap();

    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/querybatch"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "results": [{"vulns": []}, {"vulns": []}]
        })))
        .mount(&server)
        .await;

    uv_snapshot!(context.filters(), context
        .audit()
        .arg("--preview")
        .arg("--script")
        .arg("script.py")
        .arg("--service-url")
        .arg(server.uri()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Found no known vulnerabilities and no adverse project statuses in 2 packages
    ");
}

/// Audit a PEP 723 script where a requirement specifies extras. Dependencies reachable only
/// through those extras must be included in the audit.
#[tokio::test]
async fn audit_script_extras() {
    let context = uv_test::test_context!("3.12");

    // A PEP 723 script that depends on `iniconfig[test]`.
    let script = context.temp_dir.child("script.py");
    script
        .write_str(indoc! {r#"
        # /// script
        # requires-python = ">=3.12"
        # dependencies = [
        #   "iniconfig[test]",
        # ]
        # ///
        import iniconfig
    "#})
        .unwrap();

    // Write a synthetic lockfile where `iniconfig` has an optional dependency
    // `typing-extensions` under the `test` extra.
    let lockfile = context.temp_dir.child("script.py.lock");
    lockfile
        .write_str(indoc! {r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        requirements = [{ name = "iniconfig", extras = ["test"] }]

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646, upload-time = "2023-01-07T12:52:09.585Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beebd3ce04b132a8bf3491/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892, upload-time = "2023-01-07T12:52:07.538Z" },
        ]

        [package.optional-dependencies]
        test = [
            { name = "typing-extensions" },
        ]

        [[package]]
        name = "typing-extensions"
        version = "4.10.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/16/3a/0d26ce356c7465a19c9ea8f4940a02f85f2b05e6e22cf5fd8a7c1d3b5e0b/typing_extensions-4.10.0.tar.gz", hash = "sha256:b0abd7c89e8fb96f98db18d86106ff1d90ab692004eb746cf6eda2682f91b3cb", size = 77558, upload-time = "2024-02-24T00:10:00.000Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/f9/de/dc04a3ea60b22624b51c703a84bbe0184abcd1d0b9bc8074b5d6b7ab90bb/typing_extensions-4.10.0-py3-none-any.whl", hash = "sha256:69b1a937c3a517342112fb4c6df7e72fc39a38e7891a5730ed4985b5214b5475", size = 33926, upload-time = "2024-02-24T00:09:57.000Z" },
        ]
    "#})
        .unwrap();

    let server = MockServer::start().await;

    // Both iniconfig and typing-extensions should be queried (2 packages).
    Mock::given(method("POST"))
        .and(path("/v1/querybatch"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "results": [{"vulns": []}, {"vulns": []}]
        })))
        .mount(&server)
        .await;

    // typing-extensions (reachable only via the `test` extra) should be audited.
    uv_snapshot!(context.filters(), context
        .audit()
        .arg("--preview")
        .arg("--script")
        .arg("script.py")
        .arg("--service-url")
        .arg(server.uri()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Found no known vulnerabilities and no adverse project statuses in 2 packages
    ");
}

/// Audit a project whose index reports an adverse PEP 792 status (deprecated
/// with reason) for a lockfile dependency.
#[tokio::test]
async fn audit_project_status_deprecated_with_reason() {
    let context = uv_test::test_context!("3.12");
    let proxy = crate::pypi_proxy::start().await;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml
        .write_str(&formatdoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig==2.0.0"]

        [[tool.uv.index]]
        url = "{}"
        default = true
    "#, proxy.url("/status/deprecated/reason/no-longer-maintained/simple")})
        .unwrap();

    context.lock().assert().success();

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/querybatch"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "results": [{"vulns": []}]
        })))
        .mount(&server)
        .await;

    uv_snapshot!(context.filters(), context
        .audit()
        .arg("--preview")
        .arg("--service-url")
        .arg(server.uri()), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    Adverse statuses:

    - iniconfig is deprecated: no-longer-maintained

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Found no known vulnerabilities and 1 adverse project status in 1 package
    ");
}

/// Audit a project whose index reports an archived status without a reason.
#[tokio::test]
async fn audit_project_status_archived_no_reason() {
    let context = uv_test::test_context!("3.12");
    let proxy = crate::pypi_proxy::start().await;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml
        .write_str(&formatdoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig==2.0.0"]

        [[tool.uv.index]]
        url = "{}"
        default = true
    "#, proxy.url("/status/archived/simple")})
        .unwrap();

    context.lock().assert().success();

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/querybatch"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "results": [{"vulns": []}]
        })))
        .mount(&server)
        .await;

    uv_snapshot!(context.filters(), context
        .audit()
        .arg("--preview")
        .arg("--service-url")
        .arg(server.uri()), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    Adverse statuses:

    - iniconfig is archived

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Found no known vulnerabilities and 1 adverse project status in 1 package
    ");
}

/// Audit a project whose index reports a quarantined status.
#[tokio::test]
async fn audit_project_status_quarantined() {
    let context = uv_test::test_context!("3.12");
    let proxy = crate::pypi_proxy::start().await;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml
        .write_str(&formatdoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig==2.0.0"]

        [[tool.uv.index]]
        url = "{}"
        default = true
    "#, proxy.url("/status/quarantined/reason/suspected-malware/simple")})
        .unwrap();

    context.lock().assert().success();

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/querybatch"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "results": [{"vulns": []}]
        })))
        .mount(&server)
        .await;

    uv_snapshot!(context.filters(), context
        .audit()
        .arg("--preview")
        .arg("--service-url")
        .arg(server.uri()), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    Adverse statuses:

    - iniconfig is quarantined: suspected-malware

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Found no known vulnerabilities and 1 adverse project status in 1 package
    ");
}

/// An `active` status is not an adverse status and should not be reported.
#[tokio::test]
async fn audit_project_status_active_not_reported() {
    let context = uv_test::test_context!("3.12");
    let proxy = crate::pypi_proxy::start().await;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml
        .write_str(&formatdoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig==2.0.0"]

        [[tool.uv.index]]
        url = "{}"
        default = true
    "#, proxy.url("/status/active/simple")})
        .unwrap();

    context.lock().assert().success();

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/querybatch"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "results": [{"vulns": []}]
        })))
        .mount(&server)
        .await;

    uv_snapshot!(context.filters(), context
        .audit()
        .arg("--preview")
        .arg("--service-url")
        .arg(server.uri()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Found no known vulnerabilities and no adverse project statuses in 1 package
    ");
}

/// A vulnerable project that also has an adverse status should surface both
/// findings in the same audit run.
#[tokio::test]
async fn audit_vulnerability_and_project_status() {
    let context = uv_test::test_context!("3.12");
    let proxy = crate::pypi_proxy::start().await;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml
        .write_str(&formatdoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig==2.0.0"]

        [[tool.uv.index]]
        url = "{}"
        default = true
    "#, proxy.url("/status/archived/simple")})
        .unwrap();

    context.lock().assert().success();

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/querybatch"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "results": [{"vulns": [{"id": "PYSEC-2023-0001"}]}]
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/v1/vulns/PYSEC-2023-0001"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "PYSEC-2023-0001",
            "modified": "2026-01-01T00:00:00Z",
            "summary": "A test vulnerability in iniconfig",
            "affected": [{
                "ranges": [{
                    "type": "ECOSYSTEM",
                    "events": [
                        {"introduced": "0"},
                        {"fixed": "2.1.0"}
                    ]
                }]
            }]
        })))
        .mount(&server)
        .await;

    uv_snapshot!(context.filters(), context
        .audit()
        .arg("--preview")
        .arg("--service-url")
        .arg(server.uri()), @r"
    success: false
    exit_code: 1
    ----- stdout -----

    Vulnerabilities:

    iniconfig 2.0.0 has 1 known vulnerability:

    - PYSEC-2023-0001: A test vulnerability in iniconfig

      Fixed in: 2.1.0

      Advisory information: https://osv.dev/vulnerability/PYSEC-2023-0001


    Adverse statuses:

    - iniconfig is archived

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Found 1 known vulnerability and 1 adverse project status in 1 package
    ");
}

/// JSON output includes vulnerabilities and adverse project statuses in the
/// same audit report.
#[tokio::test]
async fn audit_json_vulnerability_and_project_status() {
    let context = uv_test::test_context!("3.12");
    let proxy = crate::pypi_proxy::start().await;
    write_audit_json_project(&context, &proxy.url("/status/archived/simple"));

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/querybatch"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "results": [{"vulns": [{"id": "PYSEC-2023-0001"}]}]
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/v1/vulns/PYSEC-2023-0001"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "PYSEC-2023-0001",
            "modified": "2026-01-01T00:00:00Z",
            "summary": "A test vulnerability in iniconfig",
            "affected": [{
                "ranges": [{
                    "type": "ECOSYSTEM",
                    "events": [
                        {"introduced": "0"},
                        {"fixed": "2.1.0"}
                    ]
                }]
            }],
            "references": [{
                "type": "ADVISORY",
                "url": "https://example.com/advisory/PYSEC-2023-0001"
            }]
        })))
        .mount(&server)
        .await;

    uv_snapshot!(context.filters(), context
        .audit()
        .arg("--preview-features")
        .arg("audit,json-output")
        .arg("--output-format")
        .arg("json")
        .arg("--frozen")
        .arg("--service-url")
        .arg(server.uri()), @r#"
    success: false
    exit_code: 1
    ----- stdout -----
    {
      "schema": {
        "version": "preview"
      },
      "summary": {
        "audited_packages": 1,
        "vulnerabilities": 1,
        "adverse_statuses": 1
      },
      "vulnerabilities": [
        {
          "dependency": {
            "name": "iniconfig",
            "version": "2.0.0"
          },
          "id": "PYSEC-2023-0001",
          "display_id": "PYSEC-2023-0001",
          "aliases": [],
          "summary": "A test vulnerability in iniconfig",
          "description": null,
          "link": "https://example.com/advisory/PYSEC-2023-0001",
          "fix_versions": [
            "2.1.0"
          ],
          "published": null,
          "modified": "2026-01-01T00:00:00Z"
        }
      ],
      "adverse_statuses": [
        {
          "name": "iniconfig",
          "status": "archived",
          "reason": null
        }
      ]
    }

    ----- stderr -----
    "#);
}
