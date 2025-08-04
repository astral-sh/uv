use anyhow::Result;
use assert_fs::fixture::{FileWriteStr, PathChild};
use std::fmt::Write;

use crate::common::{TestContext, uv_snapshot};

// Helper function to create proper TOML lock file content
fn create_lock_file_toml(packages: &[(&str, &str)]) -> String {
    let mut content = String::from("version = 1\nrevision = 3\nrequires-python = \">=3.8\"\n\n");

    for (name, version) in packages {
        write!(content, "[[package]]\nname = \"{name}\"\nversion = \"{version}\"\nsource = {{ registry = \"https://pypi.org/simple\" }}\n\n").unwrap();
    }

    content
}

// =============================================================================
// BASIC FUNCTIONALITY TESTS
// =============================================================================

#[test]
fn test_audit_basic_workflow() -> Result<()> {
    let context = TestContext::new("3.12").with_filtered_audit_timestamps();

    // Create a simple project with no vulnerabilities
    context.temp_dir.child("pyproject.toml").write_str(
        r#"[project]
name = "safe-project"
version = "0.1.0"
description = "A project with no vulnerabilities"
requires-python = ">=3.8"
dependencies = ["requests==2.31.0"]
"#,
    )?;

    // Create a clean lock file
    let lock_content = create_lock_file_toml(&[("requests", "2.31.0")]);
    context.temp_dir.child("uv.lock").write_str(&lock_content)?;

    uv_snapshot!(context.filters(), context.audit(), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    🛡️  uv audit report
    ━━━━━━━━━━━━━━━━━━━━━━

    📊 Scan Summary
    ├─ Scanned: 1 packages
    ├─ Vulnerable: 0 packages
    └─ Vulnerabilities: 0

    ✅ No vulnerabilities found!
    Scan completed at [DATETIME]


    ----- stderr -----
    Auditing dependencies for vulnerabilities in ....
    Updating vulnerability database...
    Scanning project dependencies...
    Matching against vulnerability database...
    Audit complete: 0 vulnerabilities found in 0 packages
    ");

    Ok(())
}

#[test]
fn test_audit_no_vulnerabilities() -> Result<()> {
    let context = TestContext::new("3.12")
        .with_filtered_counts()
        .with_filtered_audit_timestamps();

    context.temp_dir.child("pyproject.toml").write_str(
        r#"[project]
name = "clean-project"
version = "0.1.0"
description = "Project with safe dependencies"
requires-python = ">=3.8"
dependencies = ["click==8.1.7", "colorama==0.4.6"]
"#,
    )?;

    // Create lock file with multiple safe packages
    let lock_content = create_lock_file_toml(&[("click", "8.1.7"), ("colorama", "0.4.6")]);
    context.temp_dir.child("uv.lock").write_str(&lock_content)?;

    uv_snapshot!(context.filters(), context.audit(), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    🛡️  uv audit report
    ━━━━━━━━━━━━━━━━━━━━━━

    📊 Scan Summary
    ├─ Scanned: 2 packages
    ├─ Vulnerable: 0 packages
    └─ Vulnerabilities: 0

    ✅ No vulnerabilities found!
    Scan completed at [DATETIME]


    ----- stderr -----
    Auditing dependencies for vulnerabilities in ....
    Updating vulnerability database...
    Scanning project dependencies...
    Matching against vulnerability database...
    Audit complete: 0 vulnerabilities found in 0 packages
    ");

    Ok(())
}

// =============================================================================
// CLI INTERFACE TESTS
// =============================================================================

#[test]
fn test_audit_help_and_usage() {
    let context = TestContext::new("3.12").with_filtered_audit_timestamps();

    uv_snapshot!(context.filters(), context.audit().arg("--help"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    Audit Python packages for known security vulnerabilities

    Usage: uv audit [OPTIONS] [PATH]

    Arguments:
      [PATH]  Path to the project directory

    Options:
          --format <FORMAT>        Output format [default: human] [possible values: human, json, sarif]
          --severity <SEVERITY>    Minimum severity level to report [default: low] [possible values:
                                   low, medium, high, critical]
          --ignore <IGNORE>        Vulnerability IDs to ignore
      -o, --output <OUTPUT>        Output file path
          --dev                    Include development dependencies
          --optional               Include optional dependencies
          --direct-only            Only check direct dependencies
          --no-cache               Disable vulnerability database caching
          --cache-dir [CACHE_DIR]  Custom cache directory

    Python options:
          --managed-python       Require use of uv-managed Python versions [env: UV_MANAGED_PYTHON=]
          --no-managed-python    Disable use of uv-managed Python versions [env: UV_NO_MANAGED_PYTHON=]
          --no-python-downloads  Disable automatic downloads of Python. [env:
                                 "UV_PYTHON_DOWNLOADS=never"]

    Global options:
      -q, --quiet...
              Use quiet output
      -v, --verbose...
              Use verbose output
          --color <COLOR_CHOICE>
              Control the use of color in output [possible values: auto, always, never]
          --native-tls
              Whether to load TLS certificates from the platform's native certificate store [env:
              UV_NATIVE_TLS=]
          --offline
              Disable network access [env: UV_OFFLINE=]
          --allow-insecure-host <ALLOW_INSECURE_HOST>
              Allow insecure connections to a host [env: UV_INSECURE_HOST=]
          --no-progress
              Hide all progress outputs [env: UV_NO_PROGRESS=]
          --directory <DIRECTORY>
              Change to the given directory prior to running the command
          --project <PROJECT>
              Run the command within the given project directory [env: UV_PROJECT=]
          --config-file <CONFIG_FILE>
              The path to a `uv.toml` file to use for configuration [env: UV_CONFIG_FILE=]
          --no-config
              Avoid discovering configuration files (`pyproject.toml`, `uv.toml`) [env: UV_NO_CONFIG=]
      -h, --help
              Display the concise help for this command

    Use `uv help audit` for more details.

    ----- stderr -----
    "#);
}

#[test]
fn test_audit_error_handling() {
    let context = TestContext::new("3.12").with_filtered_audit_timestamps();

    // Test with missing project files
    uv_snapshot!(context.filters(), context.audit(), @r"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Auditing dependencies for vulnerabilities in ....
    Updating vulnerability database...
    Scanning project dependencies...
    Error: Audit failed: No dependency information found. Run 'uv lock' to generate a lock file.
    This may be due to a missing or corrupted vulnerability database cache.
    Try running 'uv audit --no-cache' to force a fresh download.
    ");
}

#[test]
fn test_audit_exit_codes() -> Result<()> {
    let context = TestContext::new("3.12").with_filtered_audit_timestamps();

    // Create safe project - should exit 0
    context.temp_dir.child("pyproject.toml").write_str(
        r#"[project]
name = "safe-project"
version = "0.1.0"
dependencies = []
"#,
    )?;

    let lock_content = create_lock_file_toml(&[]); // Empty packages array
    context.temp_dir.child("uv.lock").write_str(&lock_content)?;

    uv_snapshot!(context.filters(), context.audit(), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    🛡️  uv audit report
    ━━━━━━━━━━━━━━━━━━━━━━

    📊 Scan Summary
    ├─ Scanned: 0 packages
    ├─ Vulnerable: 0 packages
    └─ Vulnerabilities: 0

    ⚠️  Warnings
    ├─ No dependencies found. This might indicate an issue with dependency resolution.

    ✅ No vulnerabilities found!
    Scan completed at [DATETIME]


    ----- stderr -----
    Auditing dependencies for vulnerabilities in ....
    Updating vulnerability database...
    Scanning project dependencies...
    Warning: No dependencies found. This might indicate an issue with dependency resolution.
    Matching against vulnerability database...
    Audit complete: 0 vulnerabilities found in 0 packages
    ");

    Ok(())
}

// =============================================================================
// DEPENDENCY SCANNING TESTS
// =============================================================================

#[test]
fn test_audit_lock_file_scanning() -> Result<()> {
    let context = TestContext::new("3.12")
        .with_filtered_counts()
        .with_filtered_audit_timestamps();

    // Create project file
    context.temp_dir.child("pyproject.toml").write_str(
        r#"[project]
name = "lock-test"
version = "0.1.0"
requires-python = ">=3.8"
dependencies = ["requests==2.31.0"]
"#,
    )?;

    // Create lock file with transitive dependencies (TOML format)
    let lock_content = r#"version = 1
revision = 3
requires-python = ">=3.8"

[[package]]
name = "requests"
version = "2.31.0"
source = { registry = "https://pypi.org/simple" }
dependencies = [
    { name = "urllib3", specifier = ">=1.21.1,<3" },
    { name = "certifi", specifier = ">=2017.4.17" },
]

[[package]]
name = "urllib3"
version = "2.2.1"
source = { registry = "https://pypi.org/simple" }

[[package]]
name = "certifi"
version = "2024.2.2"
source = { registry = "https://pypi.org/simple" }
"#;

    context.temp_dir.child("uv.lock").write_str(lock_content)?;

    uv_snapshot!(context.filters(), context.audit(), @r#"
    success: false
    exit_code: 1
    ----- stdout -----
    🛡️  uv audit report
    ━━━━━━━━━━━━━━━━━━━━━━

    📊 Scan Summary
    ├─ Scanned: 3 packages
    ├─ Vulnerable: 1 packages
    └─ Vulnerabilities: 1

    🚨 Severity Breakdown
    ├─ 🟢 Low: 1

    🔧 Fix Analysis
    ├─ Fixable: 1
    └─ Unfixable: 0

    🐛 Vulnerabilities Found
    ━━━━━━━━━━━━━━━━━━━━━━━━━

    1. 🟢 PYSEC-2024-230
       Package: certifi v2024.2.2
       Severity: Low
       Summary: Certifi is a curated collection of Root Certificates for validating the trustworthiness of SSL certificates while verifying the identity of TLS hosts. Certifi starting in 2021.05.30 and prior to 2024.07.4 recognized root certificates from `GLOBALTRUST`. Certifi 2024.07.04 removes root certificates from `GLOBALTRUST` from the root store. These are in the process of being removed from Mozilla's trust store. `GLOBALTRUST`'s root certificates are being removed pursuant to an investigation which identified "long-running and unresolved compliance issues."
       Description: Certifi is a curated collection of Root Certificates for validating the trustworthiness of SSL certificates while verifying the identity of TLS hosts. Certifi starting in 2021.05.30 and prior to 2024.07.4 recognized root certificates from `GLOBALTRUST`. Certifi 2024.07.04 removes root certificates from `GLOBALTRUST` from the root store. These are in the process of being removed from Mozilla's trust store. `GLOBALTRUST`'s root certificates are being removed pursuant to an investigation which identified "long-running and unresolved compliance issues."
       Fixed in: 2024.7.4
       References:
         - https://github.com/certifi/python-certifi/security/advisories/GHSA-248v-346w-9cwc
         - https://security.netapp.com/advisory/ntap-20241206-0001/
         - https://groups.google.com/a/mozilla.org/g/dev-security-policy/c/XpknYMPO8dI
         - https://github.com/certifi/python-certifi/commit/bd8153872e9c6fc98f4023df9c2deaffea2fa463

    💡 Fix Suggestions
    ━━━━━━━━━━━━━━━━━━━

    • certifi: 2024.2.2 → 2024.7.4 (fixes PYSEC-2024-230)

    Scan completed at [DATETIME]


    ----- stderr -----
    Auditing dependencies for vulnerabilities in ....
    Updating vulnerability database...
    Scanning project dependencies...
    Matching against vulnerability database...
    Audit complete: 1 vulnerabilities found in 1 packages
    "#);

    Ok(())
}

#[test]
fn test_audit_pyproject_scanning() -> Result<()> {
    let context = TestContext::new("3.12").with_filtered_audit_timestamps();

    // Only pyproject.toml, no lock file (fallback scenario)
    context.temp_dir.child("pyproject.toml").write_str(
        r#"[project]
name = "fallback-project"
version = "0.1.0"
dependencies = ["click==8.1.7", "requests==2.31.0"]

[project.optional-dependencies]
dev = ["pytest==7.4.0", "black==23.7.0"]
testing = ["coverage==7.2.7"]
"#,
    )?;

    uv_snapshot!(context.filters(), context.audit(), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    🛡️  uv audit report
    ━━━━━━━━━━━━━━━━━━━━━━

    📊 Scan Summary
    ├─ Scanned: 2 packages
    ├─ Vulnerable: 0 packages
    └─ Vulnerabilities: 0

    ✅ No vulnerabilities found!
    Scan completed at [DATETIME]


    ----- stderr -----
    Auditing dependencies for vulnerabilities in ....
    Updating vulnerability database...
    Scanning project dependencies...
    Matching against vulnerability database...
    Audit complete: 0 vulnerabilities found in 0 packages
    ");

    Ok(())
}

// =============================================================================
// FORMAT TESTS
// =============================================================================

#[test]
fn test_audit_json_format() -> Result<()> {
    let context = TestContext::new("3.12").with_filtered_audit_timestamps();

    context.temp_dir.child("pyproject.toml").write_str(
        r#"[project]
name = "json-test"
version = "0.1.0"
dependencies = []
"#,
    )?;

    let lock_content = create_lock_file_toml(&[]); // Empty packages for JSON format test
    context.temp_dir.child("uv.lock").write_str(&lock_content)?;

    uv_snapshot!(context.filters(), context.audit().arg("--format").arg("json"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "scan_time": "[DATETIME]",
      "total_packages": 0,
      "vulnerable_packages": 0,
      "total_vulnerabilities": 0,
      "vulnerabilities": [],
      "fix_suggestions": [],
      "warnings": [
        "No dependencies found. This might indicate an issue with dependency resolution."
      ]
    }

    ----- stderr -----
    Auditing dependencies for vulnerabilities in ....
    Updating vulnerability database...
    Scanning project dependencies...
    Warning: No dependencies found. This might indicate an issue with dependency resolution.
    Matching against vulnerability database...
    Audit complete: 0 vulnerabilities found in 0 packages
    "#);

    Ok(())
}

// =============================================================================
// CACHE TESTS
// =============================================================================

#[test]
fn test_audit_cache_behavior() -> Result<()> {
    let context = TestContext::new("3.12").with_filtered_audit_timestamps();

    context.temp_dir.child("pyproject.toml").write_str(
        r#"[project]
name = "cache-test"
version = "0.1.0"
dependencies = []
"#,
    )?;

    let lock_content = create_lock_file_toml(&[]); // Empty packages for cache test
    context.temp_dir.child("uv.lock").write_str(&lock_content)?;

    // First run should work
    uv_snapshot!(context.filters(), context.audit(), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    🛡️  uv audit report
    ━━━━━━━━━━━━━━━━━━━━━━

    📊 Scan Summary
    ├─ Scanned: 0 packages
    ├─ Vulnerable: 0 packages
    └─ Vulnerabilities: 0

    ⚠️  Warnings
    ├─ No dependencies found. This might indicate an issue with dependency resolution.

    ✅ No vulnerabilities found!
    Scan completed at [DATETIME]


    ----- stderr -----
    Auditing dependencies for vulnerabilities in ....
    Updating vulnerability database...
    Scanning project dependencies...
    Warning: No dependencies found. This might indicate an issue with dependency resolution.
    Matching against vulnerability database...
    Audit complete: 0 vulnerabilities found in 0 packages
    ");

    Ok(())
}

// =============================================================================
// INTEGRATION WITH UV ECOSYSTEM TESTS
// =============================================================================

#[test]
fn test_audit_after_lock_update() -> Result<()> {
    let context = TestContext::new("3.12").with_filtered_audit_timestamps();

    // Initial project setup
    context.temp_dir.child("pyproject.toml").write_str(
        r#"[project]
name = "integration-test"
version = "0.1.0"
dependencies = ["requests>=2.25.0"]
"#,
    )?;

    // First generate lock file with uv lock
    uv_snapshot!(context.lock(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: No `requires-python` value found in the workspace. Defaulting to `>=3.12`.
    Resolved 6 packages in [TIME]
    ");

    // Then audit the locked dependencies
    uv_snapshot!(context.filters(), context.audit(), @r#"
    success: false
    exit_code: 1
    ----- stdout -----
    🛡️  uv audit report
    ━━━━━━━━━━━━━━━━━━━━━━

    📊 Scan Summary
    ├─ Scanned: 6 packages
    ├─ Vulnerable: 2 packages
    └─ Vulnerabilities: 2

    🚨 Severity Breakdown
    ├─ 🟢 Low: 2

    🔧 Fix Analysis
    ├─ Fixable: 2
    └─ Unfixable: 0

    🐛 Vulnerabilities Found
    ━━━━━━━━━━━━━━━━━━━━━━━━━

    1. 🟢 PYSEC-2024-230
       Package: certifi v2024.2.2
       Severity: Low
       Summary: Certifi is a curated collection of Root Certificates for validating the trustworthiness of SSL certificates while verifying the identity of TLS hosts. Certifi starting in 2021.05.30 and prior to 2024.07.4 recognized root certificates from `GLOBALTRUST`. Certifi 2024.07.04 removes root certificates from `GLOBALTRUST` from the root store. These are in the process of being removed from Mozilla's trust store. `GLOBALTRUST`'s root certificates are being removed pursuant to an investigation which identified "long-running and unresolved compliance issues."
       Description: Certifi is a curated collection of Root Certificates for validating the trustworthiness of SSL certificates while verifying the identity of TLS hosts. Certifi starting in 2021.05.30 and prior to 2024.07.4 recognized root certificates from `GLOBALTRUST`. Certifi 2024.07.04 removes root certificates from `GLOBALTRUST` from the root store. These are in the process of being removed from Mozilla's trust store. `GLOBALTRUST`'s root certificates are being removed pursuant to an investigation which identified "long-running and unresolved compliance issues."
       Fixed in: 2024.7.4
       References:
         - https://github.com/certifi/python-certifi/security/advisories/GHSA-248v-346w-9cwc
         - https://security.netapp.com/advisory/ntap-20241206-0001/
         - https://groups.google.com/a/mozilla.org/g/dev-security-policy/c/XpknYMPO8dI
         - https://github.com/certifi/python-certifi/commit/bd8153872e9c6fc98f4023df9c2deaffea2fa463

    2. 🟢 PYSEC-2024-60
       Package: idna v3.6
       Severity: Low
       Summary: A vulnerability was identified in the kjd/idna library, specifically within the `idna.encode()` function, affecting version 3.6. The issue arises from the function's handling of crafted input strings, which can lead to quadratic complexity and consequently, a denial of service condition. This vulnerability is triggered by a crafted input that causes the `idna.encode()` function to process the input with considerable computational load, significantly increasing the processing time in a quadratic manner relative to the input size.
       Description: A vulnerability was identified in the kjd/idna library, specifically within the `idna.encode()` function, affecting version 3.6. The issue arises from the function's handling of crafted input strings, which can lead to quadratic complexity and consequently, a denial of service condition. This vulnerability is triggered by a crafted input that causes the `idna.encode()` function to process the input with considerable computational load, significantly increasing the processing time in a quadratic manner relative to the input size.
       Fixed in: 3.7
       References:
         - https://huntr.com/bounties/93d78d07-d791-4b39-a845-cbfabc44aadb
         - https://huntr.com/bounties/93d78d07-d791-4b39-a845-cbfabc44aadb
         - https://github.com/kjd/idna/commit/1d365e17e10d72d0b7876316fc7b9ca0eebdd38d

    💡 Fix Suggestions
    ━━━━━━━━━━━━━━━━━━━

    • certifi: 2024.2.2 → 2024.7.4 (fixes PYSEC-2024-230)
    • idna: 3.6 → 3.7 (fixes PYSEC-2024-60)

    Scan completed at [DATETIME]


    ----- stderr -----
    Auditing dependencies for vulnerabilities in ....
    Updating vulnerability database...
    Scanning project dependencies...
    Matching against vulnerability database...
    Audit complete: 2 vulnerabilities found in 2 packages
    "#);

    Ok(())
}

#[test]
fn test_audit_with_uv_sync_workflow() -> Result<()> {
    let context = TestContext::new("3.12").with_filtered_audit_timestamps();

    context.temp_dir.child("pyproject.toml").write_str(
        r#"[project]
name = "sync-audit-test"
version = "0.1.0"
dependencies = ["click==8.1.7"]
"#,
    )?;

    // Generate lock and sync
    uv_snapshot!(context.lock(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: No `requires-python` value found in the workspace. Defaulting to `>=3.12`.
    Resolved 3 packages in [TIME]
    ");

    uv_snapshot!(context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: No `requires-python` value found in the workspace. Defaulting to `>=3.12`.
    Resolved 3 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + click==8.1.7
    ");

    // Now audit the synced environment
    uv_snapshot!(context.filters(), context.audit(), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    🛡️  uv audit report
    ━━━━━━━━━━━━━━━━━━━━━━

    📊 Scan Summary
    ├─ Scanned: 3 packages
    ├─ Vulnerable: 0 packages
    └─ Vulnerabilities: 0

    ✅ No vulnerabilities found!
    Scan completed at [DATETIME]


    ----- stderr -----
    Auditing dependencies for vulnerabilities in ....
    Updating vulnerability database...
    Scanning project dependencies...
    Matching against vulnerability database...
    Audit complete: 0 vulnerabilities found in 0 packages
    ");

    Ok(())
}
