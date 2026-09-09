use anyhow::Result;
use assert_cmd::assert::OutputAssertExt;
use assert_fs::prelude::*;
use indoc::indoc;
use insta::assert_json_snapshot;
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use uv_static::EnvVars;
use uv_test::{TestContext, uv_snapshot};

fn install_tool(context: &TestContext, name: &str, locked: bool) {
    let links = context.workspace_root.join("test/links");

    let mut command = context.tool_install();
    command
        .arg(name)
        .arg("--no-index")
        .arg("--find-links")
        .arg(links);
    if locked {
        command.env(EnvVars::UV_PREVIEW_FEATURES, "tool-install-locks");
    }
    command.assert().success();
}

async fn mount_clean_service(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/v1/querybatch"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "results": [{ "vulns": [] }]
        })))
        .mount(server)
        .await;
}

async fn mount_vulnerable_service(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/v1/querybatch"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "results": [{ "vulns": [{ "id": "PYSEC-2026-0001" }] }]
        })))
        .mount(server)
        .await;

    Mock::given(method("GET"))
        .and(path("/v1/vulns/PYSEC-2026-0001"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "PYSEC-2026-0001",
            "modified": "2026-01-01T00:00:00Z",
            "summary": "A test vulnerability in simple-launcher",
            "affected": [{
                "package": {
                    "ecosystem": "PyPI",
                    "name": "simple-launcher"
                },
                "ranges": [{
                    "type": "ECOSYSTEM",
                    "events": [
                        { "introduced": "0" },
                        { "fixed": "0.2.0" }
                    ]
                }]
            }],
            "references": [{
                "type": "ADVISORY",
                "url": "https://example.com/advisory/PYSEC-2026-0001"
            }]
        })))
        .mount(server)
        .await;
}

#[test]
fn tool_audit_requires_selection() {
    let context = uv_test::test_context!("3.12");

    uv_snapshot!(context.filters(), context.tool_audit(), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: the following required arguments were not provided:
      <NAME>...

    Usage: uv tool audit --cache-dir [CACHE_DIR] <NAME>...

    For more information, try '--help'.
    ");

    uv_snapshot!(context.filters(), context.tool_audit()
        .arg("--all")
        .arg("simple-launcher"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: the argument '--all' cannot be used with '<NAME>...'

    Usage: uv tool audit --cache-dir [CACHE_DIR] --all <NAME>...

    For more information, try '--help'.
    ");
}

#[test]
fn tool_audit_preview_features() {
    let context = uv_test::test_context!("3.12").with_tool_dirs();

    uv_snapshot!(context.filters(), context.tool_audit()
        .arg("--all")
        , @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: `uv tool audit` is experimental and may change without warning. Pass `--preview-features audit,tool-install-locks` to disable this warning.
    No tools installed
    ");

    uv_snapshot!(context.filters(), context.tool_audit()
        .arg("--all")
        .env(EnvVars::UV_PREVIEW_FEATURES, "audit")
        , @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: `uv tool audit` is experimental and may change without warning. Pass `--preview-features tool-install-locks` to disable this warning.
    No tools installed
    ");

    uv_snapshot!(context.filters(), context.tool_audit()
        .arg("--all")
        .env(EnvVars::UV_PREVIEW_FEATURES, "tool-install-locks")
        , @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: `uv tool audit` is experimental and may change without warning. Pass `--preview-features audit` to disable this warning.
    No tools installed
    ");

    uv_snapshot!(context.filters(), context.tool_audit()
        .arg("--all")
        .env(EnvVars::UV_PREVIEW_FEATURES, "audit,tool-install-locks")
        , @"
    exit_code: 0 (success)
    ----- stderr -----
    No tools installed
    ");
}

#[test]
fn tool_audit_unknown_tool() {
    let context = uv_test::test_context!("3.12").with_tool_dirs();

    uv_snapshot!(context.filters(), context.tool_audit()
        .arg("simple-launcher")
        .env(EnvVars::UV_PREVIEW_FEATURES, "audit,tool-install-locks")
        , @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: `simple-launcher` is not installed; run `uv tool install simple-launcher` to install
    ");
}

#[test]
fn tool_audit_missing_lockfile() {
    let context = uv_test::test_context!("3.12").with_tool_dirs();
    install_tool(&context, "simple-launcher", false);

    uv_snapshot!(context.filters(), context.tool_audit()
        .arg("--all")
        .env(EnvVars::UV_PREVIEW_FEATURES, "audit,tool-install-locks")
        , @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: Skipping tool `simple-launcher` because it does not have a lockfile; reinstall it with `--preview-features tool-install-locks` to audit it
    No auditable tools installed
    ");

    uv_snapshot!(context.filters(), context.tool_audit()
        .arg("simple-launcher")
        .env(EnvVars::UV_PREVIEW_FEATURES, "audit,tool-install-locks")
        , @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Tool `simple-launcher` does not have a lockfile; reinstall it with `--preview-features tool-install-locks` to audit it
    ");
}

#[test]
fn tool_audit_invalid_receipt() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_tool_dirs();
    let tool_dir = context.temp_dir.child("tools");
    install_tool(&context, "simple-launcher", true);
    fs_err::write(
        tool_dir.join("simple-launcher").join("uv-receipt.toml"),
        "not valid toml",
    )?;

    uv_snapshot!(context.filters(), context.tool_audit()
        .arg("--all")
        .env(EnvVars::UV_PREVIEW_FEATURES, "audit,tool-install-locks")
        , @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: Ignoring malformed tool `simple-launcher` (run `uv tool uninstall simple-launcher` to remove)
    No auditable tools installed
    ");

    uv_snapshot!(context.filters(), context.tool_audit()
        .arg("simple-launcher")
        .env(EnvVars::UV_PREVIEW_FEATURES, "audit,tool-install-locks")
        , @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Tool `simple-launcher` has an invalid receipt: Failed to read `uv-receipt.toml` at [TEMP_DIR]/tools/simple-launcher/uv-receipt.toml
    ");

    Ok(())
}

#[test]
fn tool_audit_invalid_lockfile() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_tool_dirs();
    let tool_dir = context.temp_dir.child("tools");
    install_tool(&context, "simple-launcher", true);
    fs_err::write(
        tool_dir.join("simple-launcher").join("uv.lock"),
        "not valid toml",
    )?;

    uv_snapshot!(context.filters(), context.tool_audit()
        .arg("--all")
        .env(EnvVars::UV_PREVIEW_FEATURES, "audit,tool-install-locks")
        , @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: Skipping tool `simple-launcher` because its lockfile at `tools/simple-launcher/uv.lock` is invalid: TOML parse error at line 1, column 5
      |
    1 | not valid toml
      |     ^
    key with no value, expected `=`

    No auditable tools installed
    ");

    uv_snapshot!(context.filters(), context.tool_audit()
        .arg("simple-launcher")
        .env(EnvVars::UV_PREVIEW_FEATURES, "audit,tool-install-locks")
        , @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Failed to parse the lockfile for tool `simple-launcher` at `tools/simple-launcher/uv.lock`: TOML parse error at line 1, column 5
      |
    1 | not valid toml
      |     ^
    key with no value, expected `=`
    ");

    Ok(())
}

#[test]
fn tool_audit_unsupported_lockfile_version() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_tool_dirs();
    let tool_dir = context.temp_dir.child("tools");
    install_tool(&context, "simple-launcher", true);

    let lock_path = tool_dir.join("simple-launcher").join("uv.lock");
    let contents = fs_err::read_to_string(&lock_path)?;
    fs_err::write(
        &lock_path,
        contents.replacen("version = 1\n", "version = 2\n", 1),
    )?;

    uv_snapshot!(context.filters(), context.tool_audit()
        .arg("--all")
        .env(EnvVars::UV_PREVIEW_FEATURES, "audit,tool-install-locks")
        , @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: Skipping tool `simple-launcher` because its lockfile at `tools/simple-launcher/uv.lock` uses an unsupported schema version (v2, but only v1 is supported)
    No auditable tools installed
    ");

    uv_snapshot!(context.filters(), context.tool_audit()
        .arg("simple-launcher")
        .env(EnvVars::UV_PREVIEW_FEATURES, "audit,tool-install-locks")
        , @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: The lockfile for tool `simple-launcher` at `tools/simple-launcher/uv.lock` uses an unsupported schema version (v2, but only v1 is supported)
    ");

    Ok(())
}

#[test]
fn tool_audit_unparsable_unsupported_lockfile_version() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_tool_dirs();
    let tool_dir = context.temp_dir.child("tools");
    install_tool(&context, "simple-launcher", true);

    let lock_path = tool_dir.join("simple-launcher").join("uv.lock");
    let contents = fs_err::read_to_string(&lock_path)?
        .replacen("version = 1\n", "version = 2\n", 1)
        .replacen("version = \"0.1.0\"\n", "version = false\n", 1);
    fs_err::write(&lock_path, contents)?;

    uv_snapshot!(context.filters(), context.tool_audit()
        .arg("simple-launcher")
        .env(EnvVars::UV_PREVIEW_FEATURES, "audit,tool-install-locks")
        , @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: The lockfile for tool `simple-launcher` at `tools/simple-launcher/uv.lock` uses an unsupported schema version (v2, but only v1 is supported)
    ");

    Ok(())
}

#[tokio::test]
async fn tool_audit_one_tool() {
    let context = uv_test::test_context!("3.12").with_tool_dirs();
    install_tool(&context, "simple-launcher", true);

    let server = MockServer::start().await;
    mount_clean_service(&server).await;

    uv_snapshot!(context.filters(), context.tool_audit()
        .arg("simple-launcher")
        .arg("--service-url")
        .arg(server.uri())
        .env(EnvVars::UV_PREVIEW_FEATURES, "audit,tool-install-locks")
        , @"
    exit_code: 0 (success)
    ----- stderr -----
    Auditing `simple-launcher`
    Found no known vulnerabilities and no adverse project statuses in 1 package
    ");
}

#[tokio::test]
async fn tool_audit_all_tools() {
    let context = uv_test::test_context!("3.13").with_tool_dirs();
    install_tool(&context, "simple-launcher", true);
    install_tool(&context, "basic-app", true);

    let server = MockServer::start().await;
    mount_clean_service(&server).await;

    uv_snapshot!(context.filters(), context.tool_audit()
        .arg("--all")
        .arg("--service-url")
        .arg(server.uri())
        .env(EnvVars::UV_PREVIEW_FEATURES, "audit,tool-install-locks")
        , @"
    exit_code: 0 (success)
    ----- stderr -----
    Auditing `basic-app`
    Found no known vulnerabilities and no adverse project statuses in 1 package
    Auditing `simple-launcher`
    Found no known vulnerabilities and no adverse project statuses in 1 package
    ");
}

#[tokio::test]
async fn tool_audit_multiple_tools() {
    let context = uv_test::test_context!("3.13").with_tool_dirs();
    install_tool(&context, "simple-launcher", true);
    install_tool(&context, "basic-app", true);

    let server = MockServer::start().await;
    mount_clean_service(&server).await;

    uv_snapshot!(context.filters(), context.tool_audit()
        .arg("simple-launcher")
        .arg("basic-app")
        .arg("--service-url")
        .arg(server.uri())
        .env(EnvVars::UV_PREVIEW_FEATURES, "audit,tool-install-locks")
        , @"
    exit_code: 0 (success)
    ----- stderr -----
    Auditing `basic-app`
    Found no known vulnerabilities and no adverse project statuses in 1 package
    Auditing `simple-launcher`
    Found no known vulnerabilities and no adverse project statuses in 1 package
    ");
}

#[tokio::test]
async fn tool_audit_mixed_lockfiles() {
    let context = uv_test::test_context!("3.13").with_tool_dirs();
    install_tool(&context, "simple-launcher", true);
    install_tool(&context, "basic-app", false);

    let server = MockServer::start().await;
    mount_clean_service(&server).await;

    uv_snapshot!(context.filters(), context.tool_audit()
        .arg("--all")
        .arg("--service-url")
        .arg(server.uri())
        .env(EnvVars::UV_PREVIEW_FEATURES, "audit,tool-install-locks")
        , @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: Skipping tool `basic-app` because it does not have a lockfile; reinstall it with `--preview-features tool-install-locks` to audit it
    Auditing `simple-launcher`
    Found no known vulnerabilities and no adverse project statuses in 1 package
    ");
}

#[tokio::test]
async fn tool_audit_shared_dependencies() {
    let context = uv_test::test_context!("3.13").with_tool_dirs();
    let links = context.workspace_root.join("test/links");

    context
        .tool_install()
        .arg("simple-launcher")
        .arg("--with")
        .arg("basic-app")
        .arg("--no-index")
        .arg("--find-links")
        .arg(links)
        .env(EnvVars::UV_PREVIEW_FEATURES, "tool-install-locks")
        .assert()
        .success();
    install_tool(&context, "basic-app", true);

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/querybatch"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "results": [{ "vulns": [] }, { "vulns": [] }]
        })))
        .mount(&server)
        .await;

    uv_snapshot!(context.filters(), context.tool_audit()
        .arg("--all")
        .arg("--service-url")
        .arg(server.uri())
        .env(EnvVars::UV_PREVIEW_FEATURES, "audit,tool-install-locks")
        , @"
    exit_code: 0 (success)
    ----- stderr -----
    Auditing `basic-app`
    Found no known vulnerabilities and no adverse project statuses in 1 package
    Auditing `simple-launcher`
    Found no known vulnerabilities and no adverse project statuses in 2 packages
    ");
}

#[tokio::test]
async fn tool_audit_vulnerability() {
    let context = uv_test::test_context!("3.12").with_tool_dirs();
    install_tool(&context, "simple-launcher", true);

    let server = MockServer::start().await;
    mount_vulnerable_service(&server).await;

    uv_snapshot!(context.filters(), context.tool_audit()
        .arg("simple-launcher")
        .arg("--service-url")
        .arg(server.uri())
        .env(EnvVars::UV_PREVIEW_FEATURES, "audit,tool-install-locks")
        , @"
    exit_code: 1 (failure)
    ----- stdout -----
    Tool `simple-launcher`:

    Vulnerabilities:

    simple-launcher 0.1.0 has 1 known vulnerability:

    - PYSEC-2026-0001: A test vulnerability in simple-launcher

      Fixed in: 0.2.0

      Advisory information: https://example.com/advisory/PYSEC-2026-0001


    ----- stderr -----
    Auditing `simple-launcher`
    Found 1 known vulnerability and no adverse project statuses in 1 package
    ");
}

#[tokio::test]
async fn tool_audit_ignore() {
    let context = uv_test::test_context!("3.12").with_tool_dirs();
    install_tool(&context, "simple-launcher", true);

    let server = MockServer::start().await;
    mount_vulnerable_service(&server).await;

    uv_snapshot!(context.filters(), context.tool_audit()
        .arg("--all")
        .arg("--ignore")
        .arg("PYSEC-2026-0001")
        .arg("--ignore")
        .arg("CVE-DOES-NOT-EXIST")
        .arg("--service-url")
        .arg(server.uri())
        .env(EnvVars::UV_PREVIEW_FEATURES, "audit,tool-install-locks")
        , @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: Ignored vulnerability `CVE-DOES-NOT-EXIST` does not match any vulnerability in the selected tools
    Auditing `simple-launcher`
    Found no known vulnerabilities and no adverse project statuses in 1 package
    ");
}

#[tokio::test]
async fn tool_audit_configured_ignore() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_tool_dirs();
    let config = context.temp_dir.child("uv.toml");
    install_tool(&context, "simple-launcher", true);
    config.write_str(indoc! {r#"
        [audit]
        ignore = ["PYSEC-2026-0001"]
    "#})?;

    let server = MockServer::start().await;
    mount_vulnerable_service(&server).await;

    uv_snapshot!(context.filters(), context.tool_audit()
        .arg("--all")
        .arg("--config-file")
        .arg(config.path())
        .arg("--service-url")
        .arg(server.uri())
        .env(EnvVars::UV_PREVIEW_FEATURES, "audit,tool-install-locks")
        , @"
    exit_code: 0 (success)
    ----- stderr -----
    Auditing `simple-launcher`
    Found no known vulnerabilities and no adverse project statuses in 1 package
    ");

    Ok(())
}

#[tokio::test]
async fn tool_audit_json() {
    let context = uv_test::test_context!("3.12").with_tool_dirs();
    install_tool(&context, "simple-launcher", true);

    let server = MockServer::start().await;
    mount_clean_service(&server).await;

    uv_snapshot!(context.filters(), context.tool_audit()
        .arg("--all")
        .arg("--output-format")
        .arg("json")
        .arg("--service-url")
        .arg(server.uri())
        .env(EnvVars::UV_PREVIEW_FEATURES, "audit,tool-install-locks,json-output")
        , @r#"
    exit_code: 0 (success)
    ----- stdout -----
    {
      "schema": {
        "version": "preview"
      },
      "tools": [
        {
          "name": "simple-launcher",
          "summary": {
            "audited_packages": 1,
            "vulnerabilities": 0,
            "adverse_statuses": 0
          },
          "vulnerabilities": [],
          "adverse_statuses": []
        }
      ]
    }
    "#);
}

#[tokio::test]
async fn tool_audit_json_preview_warning() {
    let context = uv_test::test_context!("3.12").with_tool_dirs();
    install_tool(&context, "simple-launcher", true);

    let server = MockServer::start().await;
    mount_clean_service(&server).await;

    uv_snapshot!(context.filters(), context.tool_audit()
        .arg("--all")
        .arg("--output-format")
        .arg("json")
        .arg("--service-url")
        .arg(server.uri())
        .env(EnvVars::UV_PREVIEW_FEATURES, "audit,tool-install-locks")
        , @r#"
    exit_code: 0 (success)
    ----- stdout -----
    {
      "schema": {
        "version": "preview"
      },
      "tools": [
        {
          "name": "simple-launcher",
          "summary": {
            "audited_packages": 1,
            "vulnerabilities": 0,
            "adverse_statuses": 0
          },
          "vulnerabilities": [],
          "adverse_statuses": []
        }
      ]
    }

    ----- stderr -----
    warning: The `--output-format json` option is experimental and the schema may change without warning. Pass `--preview-features json-output` to disable this warning.
    "#);
}

#[tokio::test]
async fn tool_audit_json_all_tools() {
    let context = uv_test::test_context!("3.13").with_tool_dirs();
    install_tool(&context, "simple-launcher", true);
    install_tool(&context, "basic-app", true);

    let server = MockServer::start().await;
    mount_clean_service(&server).await;

    uv_snapshot!(context.filters(), context.tool_audit()
        .arg("--all")
        .arg("--output-format")
        .arg("json")
        .arg("--service-url")
        .arg(server.uri())
        .env(EnvVars::UV_PREVIEW_FEATURES, "audit,tool-install-locks,json-output")
        , @r#"
    exit_code: 0 (success)
    ----- stdout -----
    {
      "schema": {
        "version": "preview"
      },
      "tools": [
        {
          "name": "basic-app",
          "summary": {
            "audited_packages": 1,
            "vulnerabilities": 0,
            "adverse_statuses": 0
          },
          "vulnerabilities": [],
          "adverse_statuses": []
        },
        {
          "name": "simple-launcher",
          "summary": {
            "audited_packages": 1,
            "vulnerabilities": 0,
            "adverse_statuses": 0
          },
          "vulnerabilities": [],
          "adverse_statuses": []
        }
      ]
    }
    "#);
}

#[tokio::test]
async fn tool_audit_sarif() {
    let context = uv_test::test_context!("3.12")
        .with_filter((uv_version::version(), "[VERSION]"))
        .with_tool_dirs();
    install_tool(&context, "simple-launcher", true);

    let server = MockServer::start().await;
    mount_clean_service(&server).await;

    uv_snapshot!(context.filters(), context.tool_audit()
        .arg("--all")
        .arg("--output-format")
        .arg("sarif")
        .arg("--service-url")
        .arg(server.uri())
        .env(EnvVars::UV_PREVIEW_FEATURES, "audit,tool-install-locks")
        , @r#"
    exit_code: 0 (success)
    ----- stdout -----
    {
      "$schema": "https://docs.oasis-open.org/sarif/sarif/v2.1.0/os/schemas/sarif-schema-2.1.0.json",
      "runs": [
        {
          "automationDetails": {
            "id": "uv/tool-audit/simple-launcher"
          },
          "invocations": [
            {
              "executionSuccessful": true
            }
          ],
          "results": [],
          "tool": {
            "driver": {
              "downloadUri": "https://github.com/astral-sh/uv",
              "informationUri": "https://pypi.org/project/uv/",
              "name": "uv",
              "semanticVersion": "[VERSION]",
              "version": "[VERSION]"
            }
          }
        }
      ],
      "version": "2.1.0"
    }
    "#);
}

#[test]
fn tool_audit_sarif_no_auditable_tools() {
    let context = uv_test::test_context!("3.12").with_tool_dirs();

    uv_snapshot!(context.filters(), context.tool_audit()
        .arg("--all")
        .arg("--output-format")
        .arg("sarif")
        .env(EnvVars::UV_PREVIEW_FEATURES, "audit,tool-install-locks")
        , @r#"
    exit_code: 0 (success)
    ----- stdout -----
    {
      "$schema": "https://docs.oasis-open.org/sarif/sarif/v2.1.0/os/schemas/sarif-schema-2.1.0.json",
      "runs": [],
      "version": "2.1.0"
    }
    "#);

    install_tool(&context, "simple-launcher", false);

    uv_snapshot!(context.filters(), context.tool_audit()
        .arg("--all")
        .arg("--output-format")
        .arg("sarif")
        .env(EnvVars::UV_PREVIEW_FEATURES, "audit,tool-install-locks")
        , @r#"
    exit_code: 0 (success)
    ----- stdout -----
    {
      "$schema": "https://docs.oasis-open.org/sarif/sarif/v2.1.0/os/schemas/sarif-schema-2.1.0.json",
      "runs": [],
      "version": "2.1.0"
    }

    ----- stderr -----
    warning: Skipping tool `simple-launcher` because it does not have a lockfile; reinstall it with `--preview-features tool-install-locks` to audit it
    "#);
}

#[tokio::test]
async fn tool_audit_sarif_all_tools() -> Result<()> {
    let context = uv_test::test_context!("3.13").with_tool_dirs();
    install_tool(&context, "simple-launcher", true);
    install_tool(&context, "basic-app", true);

    let server = MockServer::start().await;
    mount_clean_service(&server).await;

    let output = context
        .tool_audit()
        .arg("--all")
        .arg("--output-format")
        .arg("sarif")
        .arg("--service-url")
        .arg(server.uri())
        .env(EnvVars::UV_PREVIEW_FEATURES, "audit,tool-install-locks")
        .output()?;
    assert_eq!(output.status.code(), Some(0));

    let report: Value = serde_json::from_slice(&output.stdout)?;
    let runs = report["runs"].as_array().map(|runs| {
        runs.iter()
            .map(|run| {
                json!({
                    "automation_id": run["automationDetails"]["id"],
                    "results": run["results"],
                })
            })
            .collect::<Vec<_>>()
    });
    assert_json_snapshot!(runs, @r#"
    [
      {
        "automation_id": "uv/tool-audit/basic-app",
        "results": []
      },
      {
        "automation_id": "uv/tool-audit/simple-launcher",
        "results": []
      }
    ]
    "#);

    Ok(())
}

#[tokio::test]
async fn tool_audit_sarif_vulnerability_location() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_tool_dirs();
    install_tool(&context, "simple-launcher", true);

    let server = MockServer::start().await;
    mount_vulnerable_service(&server).await;

    let output = context
        .tool_audit()
        .arg("simple-launcher")
        .arg("--output-format")
        .arg("sarif")
        .arg("--service-url")
        .arg(server.uri())
        .env(EnvVars::UV_PREVIEW_FEATURES, "audit,tool-install-locks")
        .output()?;
    assert_eq!(output.status.code(), Some(1));

    let report: Value = serde_json::from_slice(&output.stdout)?;
    assert_json_snapshot!(json!({
        "automation_id": report["runs"][0]["automationDetails"]["id"],
        "artifact": report["runs"][0]["results"][0]["locations"][0]["physicalLocation"]
            ["artifactLocation"]["uri"],
        "rule": report["runs"][0]["results"][0]["ruleId"],
        "runs": report["runs"].as_array().map(Vec::len),
    }), @r#"
    {
      "artifact": "temp/tools/simple-launcher/uv.lock",
      "automation_id": "uv/tool-audit/simple-launcher",
      "rule": "PYSEC-2026-0001",
      "runs": 1
    }
    "#);

    Ok(())
}

#[tokio::test]
async fn tool_audit_persisted_index_and_project_status() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_tool_dirs();
    let server = MockServer::start().await;
    let wheel_filename = "simple_launcher-0.1.0-py3-none-any.whl";
    let wheel = fs_err::read(
        context
            .workspace_root
            .join("test/links")
            .join(wheel_filename),
    )?;

    let simple_index = json!({
        "meta": { "api-version": "1.1" },
        "name": "simple-launcher",
        "project-status": {
            "status": "archived",
            "reason": "no-longer-maintained"
        },
        "files": [{
            "filename": wheel_filename,
            "url": format!("{}/files/{wheel_filename}", server.uri()),
            "hashes": {
                "sha256": "5327e0bb67cdb46800999de6dcf034bf0a5335702883494af0d8b7f6ca48cee4"
            },
            "core-metadata": true,
            "upload-time": "2024-03-24T00:00:00Z"
        }]
    });
    Mock::given(method("GET"))
        .and(path("/simple/simple-launcher/"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            simple_index.to_string(),
            "application/vnd.pypi.simple.v1+json",
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/files/{wheel_filename}.metadata")))
        .respond_with(ResponseTemplate::new(200).set_body_string(indoc! {"
            Metadata-Version: 2.1
            Name: simple-launcher
            Version: 0.1.0
            Requires-Python: >=3.8
        "}))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/files/{wheel_filename}")))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(wheel))
        .mount(&server)
        .await;
    mount_clean_service(&server).await;

    context
        .tool_install()
        .arg("simple-launcher")
        .arg("--index-url")
        .arg(format!("{}/simple", server.uri()))
        .env(EnvVars::UV_PREVIEW_FEATURES, "tool-install-locks")
        .assert()
        .success();

    uv_snapshot!(context.filters(), context.tool_audit()
        .arg("simple-launcher")
        .arg("--service-url")
        .arg(server.uri())
        .env(EnvVars::UV_PREVIEW_FEATURES, "audit,tool-install-locks")
        , @"
    exit_code: 0 (success)
    ----- stdout -----
    Tool `simple-launcher`:

    Adverse statuses:

    - simple-launcher is archived: no-longer-maintained

    ----- stderr -----
    Auditing `simple-launcher`
    Found no known vulnerabilities and 1 adverse project status in 1 package
    ");

    Ok(())
}
