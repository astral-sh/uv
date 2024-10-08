#![cfg(feature = "python")]

use anyhow::Result;
use assert_cmd::assert::OutputAssertExt;
use common::{uv_snapshot, TestContext};
use std::env;
use std::path::Path;
use tempfile::TempDir;

mod common;

/// Test that build backend works if we invoke it directly.
///
/// We can't test end-to-end here including the PEP 517 bridge code since we don't have a uv wheel.
#[test]
fn uv_backend_direct() -> Result<()> {
    let context = TestContext::new("3.12");
    let uv_backend = Path::new("../../scripts/packages/uv_backend");

    let temp_dir = TempDir::new()?;

    uv_snapshot!(context
        .build_backend()
        .arg("build-wheel")
        .arg(temp_dir.path())
        .current_dir(uv_backend), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    uv_backend-0.1.0-py3-none-any.whl

    ----- stderr -----
    "###);

    context
        .pip_install()
        .arg(temp_dir.path().join("uv_backend-0.1.0-py3-none-any.whl"))
        .assert()
        .success();

    uv_snapshot!(context
        .run()
        .arg("python")
        .arg("-c")
        .arg("import uv_backend\nuv_backend.greet()")
        // Python on windows
        .env("PYTHONUTF8", "1"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello 👋

    ----- stderr -----
    "###);

    Ok(())
}
