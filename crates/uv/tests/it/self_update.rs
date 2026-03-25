use std::{path::PathBuf, process::Command};

use anyhow::Result;
use assert_fs::prelude::*;
use axoupdater::{
    ReleaseSourceType,
    test::helpers::{RuntestArgs, perform_runtest},
};
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use uv_static::EnvVars;

use uv_test::{TestContext, get_bin, uv_snapshot};

#[test]
fn check_self_update() {
    // To maximally emulate behaviour in practice, this test actually modifies CARGO_HOME
    // and therefore should only be run in CI by default, where it can't hurt developers.
    // We use the "CI" env-var that CI machines tend to run
    if std::env::var(EnvVars::CI)
        .map(|s| s.is_empty())
        .unwrap_or(true)
    {
        return;
    }

    // Configure the runtest
    let args = RuntestArgs {
        app_name: "uv".to_owned(),
        package: "uv".to_owned(),
        owner: "astral-sh".to_owned(),
        bin: get_bin!(),
        binaries: vec!["uv".to_owned()],
        args: vec!["self".to_owned(), "update".to_owned()],
        release_type: ReleaseSourceType::GitHub,
    };

    // install and update the application
    let installed_bin = perform_runtest(&args);

    // check that the binary works like normal
    let status = Command::new(installed_bin)
        .arg("--version")
        .status()
        .expect("failed to run 'uv --version'");
    assert!(status.success(), "'uv --version' returned non-zero");
}

#[test]
fn self_update_offline_error() {
    let context = uv_test::test_context!("3.12");

    uv_snapshot!(context.self_update().arg("--offline"),
    @r"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    error: Self-update is not possible because network connectivity is disabled (i.e., with `--offline`)
    ");
}

#[test]
fn self_update_offline_quiet() {
    let context = uv_test::test_context!("3.12");

    uv_snapshot!(context.self_update().arg("--offline").arg("--quiet"),
    @r"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    error: Self-update is not possible because network connectivity is disabled (i.e., with `--offline`)
    ");
}

#[test]
fn self_update_offline_extra_quiet() {
    let context = uv_test::test_context!("3.12");

    uv_snapshot!(context.self_update().arg("--offline").arg("--quiet").arg("--quiet"),
    @r"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    ");
}

/// Set up a fake receipt and a mock update metadata endpoint to allow
/// simulating an update with `--dry-run`.
async fn setup_mock_update(
    context: &TestContext,
    target_version: &str,
) -> Result<(PathBuf, MockServer)> {
    let receipt_dir = context.temp_dir.child("receipt");
    receipt_dir.create_dir_all()?;

    let install_prefix = std::path::absolute(
        get_bin!()
            .parent()
            .expect("uv binary should have a parent directory"),
    )?;
    receipt_dir
        .child("uv-receipt.json")
        .write_str(&serde_json::to_string_pretty(&json!({
            "install_prefix": install_prefix,
            "binaries": ["uv"],
            "cdylibs": [],
            "source": {
                "release_type": "github",
                "owner": "astral-sh",
                "name": "uv",
                "app_name": "uv",
            },
            "version": env!("CARGO_PKG_VERSION"),
            "provider": {
                "source": "cargo-dist",
                "version": "0.31.0",
            },
            "modify_path": true,
        }))?)?;

    let server = MockServer::start().await;
    let installer_name = if cfg!(windows) {
        "uv-installer.ps1"
    } else {
        "uv-installer.sh"
    };
    Mock::given(method("GET"))
        .and(path(format!(
            "/api/v3/repos/astral-sh/uv/releases/tags/{target_version}"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "tag_name": target_version,
            "name": target_version,
            "url": format!("{}/repos/astral-sh/uv/releases/tags/{target_version}", server.uri()),
            "assets": [{
                "url": format!("{}/assets/{installer_name}", server.uri()),
                "browser_download_url": format!("{}/downloads/{installer_name}", server.uri()),
                "name": installer_name,
            }],
            "prerelease": false,
        })))
        .mount(&server)
        .await;

    Ok((receipt_dir.to_path_buf(), server))
}

#[tokio::test]
async fn test_self_update_uses_legacy_path_with_ghe_override() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_filtered_current_version();

    let target_version = "9.9.9";
    let (receipt_dir, server) = setup_mock_update(&context, target_version).await?;

    uv_snapshot!(context.filters(), context.self_update()
        .arg(target_version)
        .arg("--dry-run")
        .env("AXOUPDATER_CONFIG_PATH", receipt_dir.as_os_str())
        .env(EnvVars::UV_INSTALLER_GHE_BASE_URL, server.uri()), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    info: Checking for updates...
    Would update uv from v[CURRENT_VERSION] to v9.9.9
    ");

    Ok(())
}

#[tokio::test]
async fn self_update_dry_run_quiet() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_filtered_current_version();

    let target_version = "9.9.9";
    let (receipt_dir, server) = setup_mock_update(&context, target_version).await?;

    uv_snapshot!(context.filters(), context.self_update()
        .arg(target_version)
        .arg("--dry-run")
        .arg("--quiet")
        .env("AXOUPDATER_CONFIG_PATH", receipt_dir.as_os_str())
        .env(EnvVars::UV_INSTALLER_GHE_BASE_URL, server.uri()), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Would update uv from v[CURRENT_VERSION] to v9.9.9
    ");

    Ok(())
}

#[tokio::test]
async fn self_update_dry_run_extra_quiet() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let receipt_dir = context.temp_dir.child("receipt");
    receipt_dir.create_dir_all()?;

    let target_version = "9.9.9";
    let (receipt_dir, server) = setup_mock_update(&context, target_version).await?;

    uv_snapshot!(context.self_update()
        .arg(target_version)
        .arg("--dry-run")
        .arg("--quiet")
        .arg("--quiet")
        .env("AXOUPDATER_CONFIG_PATH", receipt_dir.as_os_str())
        .env(EnvVars::UV_INSTALLER_GHE_BASE_URL, server.uri()), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    ");

    Ok(())
}

#[tokio::test]
async fn self_update_noop_dry_run() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_filtered_current_version();

    let target_version = env!("CARGO_PKG_VERSION");
    let (receipt_dir, server) = setup_mock_update(&context, target_version).await?;

    uv_snapshot!(context.filters(), context.self_update()
        .arg(target_version)
        .arg("--dry-run")
        .env("AXOUPDATER_CONFIG_PATH", receipt_dir.as_os_str())
        .env(EnvVars::UV_INSTALLER_GHE_BASE_URL, server.uri()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    info: Checking for updates...
    You're on the latest version of uv (v[CURRENT_VERSION])
    ");

    Ok(())
}

#[tokio::test]
async fn self_update_noop_dry_run_quiet() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_filtered_current_version();

    let target_version = env!("CARGO_PKG_VERSION");
    let (receipt_dir, server) = setup_mock_update(&context, target_version).await?;

    uv_snapshot!(context.filters(), context.self_update()
        .arg(target_version)
        .arg("--dry-run")
        .arg("--quiet")
        .env("AXOUPDATER_CONFIG_PATH", receipt_dir.as_os_str())
        .env(EnvVars::UV_INSTALLER_GHE_BASE_URL, server.uri()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    ");

    Ok(())
}
