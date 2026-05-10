use anyhow::Result;
use assert_cmd::assert::OutputAssertExt;

use uv_test::uv_snapshot;

/// Test that `cache ls` returns 0 packages for an empty cache.
#[test]
fn cache_ls_empty() {
    let context = uv_test::test_context!("3.12");

    context.clean().assert().success();

    uv_snapshot!(context.cache_ls(), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    0 packages cached

    ----- stderr -----
    ");
}

/// Test that `cache ls --quiet` returns 0 for an empty cache.
#[test]
fn cache_ls_empty_quiet() {
    let context = uv_test::test_context!("3.12");

    context.clean().assert().success();

    uv_snapshot!(context.cache_ls().arg("--count"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    0

    ----- stderr -----
    ");
}

/// Test that `cache ls --format json` works for an empty cache.
#[test]
fn cache_ls_empty_json() {
    let context = uv_test::test_context!("3.12");

    context.clean().assert().success();

    uv_snapshot!(context.cache_ls().arg("--format").arg("json"), @r##"
    success: true
    exit_code: 0
    ----- stdout -----
    {"packages":[],"total":0}

    ----- stderr -----
    "##);
}

/// Test that `cache ls` lists packages with correct structure after install.
#[test]
fn cache_ls_with_packages() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    context.pip_install().arg("iniconfig").assert().success();

    let output = context.cache_ls().output()?;
    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout)?;
    assert!(stdout.contains("Package"));
    assert!(stdout.contains("Version"));
    assert!(stdout.contains("iniconfig"));
    assert!(stdout.contains("1 packages cached"));

    Ok(())
}

/// Test that `cache ls --quiet` returns the correct count.
#[test]
fn cache_ls_with_packages_quiet() {
    let context = uv_test::test_context!("3.12");

    context.pip_install().arg("iniconfig").assert().success();

    uv_snapshot!(context.cache_ls().arg("--count"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    1

    ----- stderr -----
    ");
}

/// Test that `cache ls --format json` works with packages.
#[test]
fn cache_ls_with_packages_json() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    context.pip_install().arg("iniconfig").assert().success();

    let output = context.cache_ls().arg("--format").arg("json").output()?;
    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout)?;
    let parsed: serde_json::Value = serde_json::from_str(&stdout)?;
    assert_eq!(parsed["total"], 1);
    assert_eq!(parsed["packages"][0]["name"], "iniconfig");
    assert!(parsed["packages"][0]["version"].is_string());

    Ok(())
}

/// Test that `cache ls` with a package filter works.
#[test]
fn cache_ls_filter() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    context.pip_install().arg("iniconfig").assert().success();
    context.pip_install().arg("tomli").assert().success();

    let output = context.cache_ls().arg("iniconfig").output()?;
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout)?;
    assert!(stdout.contains("iniconfig"));
    assert!(stdout.contains("1 packages cached"));

    Ok(())
}

/// Test that `cache ls` handles a non-matching filter gracefully.
#[test]
fn cache_ls_filter_no_match() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    context.pip_install().arg("iniconfig").assert().success();

    let output = context.cache_ls().arg("nonexistent-package").output()?;
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout)?;
    assert!(stdout.contains("Package"));
    assert!(stdout.contains("Version"));
    assert!(stdout.contains("0 packages cached"));

    Ok(())
}
