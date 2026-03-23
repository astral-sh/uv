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
        dependencies = ["requests==2.31.0"]
    "#})
        .unwrap();

    context
        .temp_dir
        .child("uv.lock")
        .write_str(indoc! {r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "requests" },
        ]

        [package.metadata]
        requires-dist = [{ name = "requests", specifier = "==2.31.0" }]

        [[package]]
        name = "requests"
        version = "2.31.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/9d/be/10918a2eac4ae9f02f6cfe6ef0b/requests-2.31.0.tar.gz", hash = "sha256:942c5a758f98d790eaed1a29cb6eefc7f0edf3fcb0fce8aea3fbd5951d bdf708" }
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
        .arg("--frozen")
        .arg("--preview")
        .arg("--service-url")
        .arg(server.uri()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Found no known vulnerabilities and no adverse project statuses in 1 packages
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
        dependencies = ["requests==2.31.0"]
    "#})
        .unwrap();

    context
        .temp_dir
        .child("uv.lock")
        .write_str(indoc! {r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "requests" },
        ]

        [package.metadata]
        requires-dist = [{ name = "requests", specifier = "==2.31.0" }]

        [[package]]
        name = "requests"
        version = "2.31.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/9d/be/10918a2eac4ae9f02f6cfe6ef0b/requests-2.31.0.tar.gz", hash = "sha256:942c5a758f98d790eaed1a29cb6eefc7f0edf3fcb0fce8aea3fbd5951d bdf708" }
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
            "summary": "A test vulnerability in requests",
            "affected": [{
                "ranges": [{
                    "type": "ECOSYSTEM",
                    "events": [
                        {"introduced": "0"},
                        {"fixed": "2.32.0"}
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

    requests 2.31.0 has 1 known vulnerability:

    - PYSEC-2023-0001: A test vulnerability in requests

      Fixed in: 2.32.0

      Advisory information: https://example.com/advisory/PYSEC-2023-0001



    ----- stderr -----
    Found 1 known vulnerability and no adverse project statuses in 1 packages
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

    context
        .temp_dir
        .child("uv.lock")
        .write_str(indoc! {r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
    "#})
        .unwrap();

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
        dependencies = ["requests==2.31.0"]
    "#})
        .unwrap();

    context
        .temp_dir
        .child("uv.lock")
        .write_str(indoc! {r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "requests" },
        ]

        [package.metadata]
        requires-dist = [{ name = "requests", specifier = "==2.31.0" }]

        [[package]]
        name = "requests"
        version = "2.31.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/9d/be/10918a2eac4ae9f02f6cfe6ef0b/requests-2.31.0.tar.gz", hash = "sha256:942c5a758f98d790eaed1a29cb6eefc7f0edf3fcb0fce8aea3fbd5951d bdf708" }
    "#})
        .unwrap();

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

    requests 2.31.0 has 1 known vulnerability:

    - PYSEC-2023-0042: A vulnerability with many aliases

      No fix versions available

      Advisory information: https://osv.dev/vulnerability/OSV-2023-0001



    ----- stderr -----
    Found 1 known vulnerability and no adverse project statuses in 1 packages
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
        dependencies = ["requests==2.31.0"]
    "#})
        .unwrap();

    context
        .temp_dir
        .child("uv.lock")
        .write_str(indoc! {r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "requests" },
        ]

        [package.metadata]
        requires-dist = [{ name = "requests", specifier = "==2.31.0" }]

        [[package]]
        name = "requests"
        version = "2.31.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/9d/be/10918a2eac4ae9f02f6cfe6ef0b/requests-2.31.0.tar.gz", hash = "sha256:942c5a758f98d790eaed1a29cb6eefc7f0edf3fcb0fce8aea3fbd5951d bdf708" }
    "#})
        .unwrap();

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

    requests 2.31.0 has 1 known vulnerability:

    - VULN-NO-FIX: A vulnerability with no fix available

      No fix versions available

      Advisory information: https://osv.dev/vulnerability/VULN-NO-FIX



    ----- stderr -----
    Found 1 known vulnerability and no adverse project statuses in 1 packages
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
        dependencies = ["requests==2.31.0"]
    "#})
        .unwrap();

    context
        .temp_dir
        .child("uv.lock")
        .write_str(indoc! {r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "requests" },
        ]

        [package.metadata]
        requires-dist = [{ name = "requests", specifier = "==2.31.0" }]

        [[package]]
        name = "requests"
        version = "2.31.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/9d/be/10918a2eac4ae9f02f6cfe6ef0b/requests-2.31.0.tar.gz", hash = "sha256:942c5a758f98d790eaed1a29cb6eefc7f0edf3fcb0fce8aea3fbd5951d bdf708" }
    "#})
        .unwrap();

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
                        {"fixed": "2.32.0"}
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
                        {"fixed": "2.31.1"}
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

    requests 2.31.0 has 2 known vulnerabilities:

    - VULN-A: First vulnerability

      Fixed in: 2.32.0

      Advisory information: https://example.com/advisory/VULN-A

    - VULN-B: Second vulnerability

      Fixed in: 2.31.1

      Advisory information: https://example.com/web/VULN-B



    ----- stderr -----
    Found 2 known vulnerabilities and no adverse project statuses in 1 packages
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
        dependencies = ["requests==2.31.0"]

        [dependency-groups]
        dev = ["flask==3.0.0"]
    "#})
        .unwrap();

    context
        .temp_dir
        .child("uv.lock")
        .write_str(indoc! {r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "requests" },
        ]

        [package.dev-dependencies]
        dev = [
            { name = "flask" },
        ]

        [package.metadata]
        requires-dist = [{ name = "requests", specifier = "==2.31.0" }]

        [package.metadata.requires-dev]
        dev = [{ name = "flask", specifier = "==3.0.0" }]

        [[package]]
        name = "requests"
        version = "2.31.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/9d/be/10918a2eac4ae9f02f6cfe6ef0b/requests-2.31.0.tar.gz", hash = "sha256:942c5a758f98d790eaed1a29cb6eefc7f0edf3fcb0fce8aea3fbd5951d bdf708" }

        [[package]]
        name = "flask"
        version = "3.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d8/09/c1a7354d3925a3c6c228e02bc/flask-3.0.0.tar.gz", hash = "sha256:cfadcdb39eb12ae77f3a5d66e7e40de8cf4e0e38e3a52a81ae2530794783f86a" }
    "#})
        .unwrap();

    let server = MockServer::start().await;

    // With --no-dev, only "requests" should be audited (not "flask").
    // The querybatch should only contain requests.
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
        .arg("--no-dev")
        .arg("--service-url")
        .arg(server.uri()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Found no known vulnerabilities and no adverse project statuses in 1 packages
    ");

    // Without --no-dev, both "requests" and "flask" should be audited (2 packages).
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
