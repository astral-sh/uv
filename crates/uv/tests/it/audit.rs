use assert_cmd::assert::OutputAssertExt;
use assert_fs::prelude::*;
use indoc::indoc;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use uv_test::uv_snapshot;

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
        .arg("--frozen")
        .arg("--preview")
        .arg("--service-url")
        .arg(server.uri()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Found no known vulnerabilities and no adverse project statuses in 1 package
    ");
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
        .arg("--frozen")
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
        .arg("--frozen")
        .arg("--preview")
        .arg("--service-url")
        .arg(server.uri()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
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
        .arg("--frozen")
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
        .arg("--frozen")
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
        .arg("--frozen")
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
        .arg("--frozen")
        .arg("--preview")
        .arg("--no-dev")
        .arg("--service-url")
        .arg(server.uri()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Found no known vulnerabilities and no adverse project statuses in 1 package
    ");

    // Without --no-dev, both packages should be audited.
    uv_snapshot!(context.filters(), context
        .audit()
        .arg("--frozen")
        .arg("--preview")
        .arg("--service-url")
        .arg(server.uri()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
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
        .arg("--frozen")
        .arg("--preview")
        .arg("--service-url")
        .arg(server.uri()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Found no known vulnerabilities and no adverse project statuses in 2 packages
    ");

    // With --no-extra web, only iniconfig should be audited.
    uv_snapshot!(context.filters(), context
        .audit()
        .arg("--frozen")
        .arg("--preview")
        .arg("--no-extra")
        .arg("web")
        .arg("--service-url")
        .arg(server.uri()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
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
        .arg("--frozen")
        .arg("--preview")
        .arg("--service-url")
        .arg(server.uri()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Found no known vulnerabilities and no adverse project statuses in 3 packages
    ");

    // --no-dev: excludes the dev group (iniconfig + sniffio = 2).
    uv_snapshot!(context.filters(), context
        .audit()
        .arg("--frozen")
        .arg("--preview")
        .arg("--no-dev")
        .arg("--service-url")
        .arg(server.uri()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Found no known vulnerabilities and no adverse project statuses in 2 packages
    ");

    // --no-group lint: excludes the lint group (iniconfig + typing-extensions = 2).
    uv_snapshot!(context.filters(), context
        .audit()
        .arg("--frozen")
        .arg("--preview")
        .arg("--no-group")
        .arg("lint")
        .arg("--service-url")
        .arg(server.uri()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Found no known vulnerabilities and no adverse project statuses in 2 packages
    ");

    // --only-group lint: only the "lint" group, project deps omitted (sniffio = 1).
    uv_snapshot!(context.filters(), context
        .audit()
        .arg("--frozen")
        .arg("--preview")
        .arg("--only-group")
        .arg("lint")
        .arg("--service-url")
        .arg(server.uri()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
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
        .arg("--frozen")
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
    Found 1 known vulnerability and no adverse project statuses in 1 package
    ");

    // With --ignore, the vulnerability is suppressed.
    uv_snapshot!(context.filters(), context
        .audit()
        .arg("--frozen")
        .arg("--preview")
        .arg("--ignore")
        .arg("PYSEC-2023-0001")
        .arg("--service-url")
        .arg(server.uri()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
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
        .arg("--frozen")
        .arg("--preview")
        .arg("--ignore")
        .arg("CVE-2023-9999")
        .arg("--service-url")
        .arg(server.uri()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
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
        .arg("--frozen")
        .arg("--preview")
        .arg("--ignore-until-fixed")
        .arg("VULN-NO-FIX")
        .arg("--service-url")
        .arg(server.uri()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
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
        .arg("--frozen")
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
        .arg("--frozen")
        .arg("--preview")
        .arg("--service-url")
        .arg(server.uri()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
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
        .arg("--frozen")
        .arg("--preview")
        .arg("--service-url")
        .arg(server.uri()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
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
        .arg("--frozen")
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
    Found 1 known vulnerability and no adverse project statuses in 1 package
    ");
}
