use crate::common::{uv_snapshot, TestContext};
use anyhow::Result;
use assert_cmd::assert::OutputAssertExt;
use flate2::bufread::GzDecoder;
use fs_err::File;
use indoc::indoc;
use std::env;
use std::io::BufReader;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;
use uv_static::EnvVars;

const BUILT_BY_UV_TEST_SCRIPT: &str = indoc! {r#"
    from built_by_uv import greet
    from built_by_uv.arithmetic.circle import area

    print(greet())
    print(f"Area of a circle with r=2: {area(2)}")
"#};

/// Test that build backend works if we invoke it directly.
///
/// We can't test end-to-end here including the PEP 517 bridge code since we don't have a uv wheel.
#[test]
fn built_by_uv_direct_wheel() -> Result<()> {
    let context = TestContext::new("3.12");
    let built_by_uv = Path::new("../../scripts/packages/built-by-uv");

    let temp_dir = TempDir::new()?;

    uv_snapshot!(context
        .build_backend()
        .arg("build-wheel")
        .arg(temp_dir.path())
        .current_dir(built_by_uv), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    built_by_uv-0.1.0-py3-none-any.whl

    ----- stderr -----
    "###);

    context
        .pip_install()
        .arg(temp_dir.path().join("built_by_uv-0.1.0-py3-none-any.whl"))
        .assert()
        .success();

    uv_snapshot!(context
        .run()
        .arg("python")
        .arg("-c")
        .arg(BUILT_BY_UV_TEST_SCRIPT)
        // Python on windows
        .env(EnvVars::PYTHONUTF8, "1"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello 👋
    Area of a circle with r=2: 12.56636

    ----- stderr -----
    "###);

    Ok(())
}

/// Test that source tree -> source dist -> wheel works.
///
/// We can't test end-to-end here including the PEP 517 bridge code since we don't have a uv wheel,
/// so we call the build backend directly.
#[test]
fn built_by_uv_direct() -> Result<()> {
    let context = TestContext::new("3.12");
    let built_by_uv = Path::new("../../scripts/packages/built-by-uv");

    let sdist_dir = TempDir::new()?;

    uv_snapshot!(context
        .build_backend()
        .arg("build-sdist")
        .arg(sdist_dir.path())
        .current_dir(built_by_uv), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    built_by_uv-0.1.0.tar.gz

    ----- stderr -----
    "###);

    let sdist_tree = TempDir::new()?;

    let sdist_reader = BufReader::new(File::open(
        sdist_dir.path().join("built_by_uv-0.1.0.tar.gz"),
    )?);
    tar::Archive::new(GzDecoder::new(sdist_reader)).unpack(sdist_tree.path())?;

    drop(sdist_dir);

    let wheel_dir = TempDir::new()?;

    uv_snapshot!(context
        .build_backend()
        .arg("build-wheel")
        .arg(wheel_dir.path())
        .current_dir(sdist_tree.path().join("built_by_uv-0.1.0")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    built_by_uv-0.1.0-py3-none-any.whl

    ----- stderr -----
    "###);

    drop(sdist_tree);

    context
        .pip_install()
        .arg(wheel_dir.path().join("built_by_uv-0.1.0-py3-none-any.whl"))
        .assert()
        .success();

    drop(wheel_dir);

    uv_snapshot!(context
        .run()
        .arg("python")
        .arg("-c")
        .arg(BUILT_BY_UV_TEST_SCRIPT)
        // Python on windows
        .env(EnvVars::PYTHONUTF8, "1"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello 👋
    Area of a circle with r=2: 12.56636

    ----- stderr -----
    "###);

    Ok(())
}

/// Test that editables work.
///
/// We can't test end-to-end here including the PEP 517 bridge code since we don't have a uv wheel,
/// so we call the build backend directly.
#[test]
fn built_by_uv_editable() -> Result<()> {
    let context = TestContext::new("3.12");
    let built_by_uv = Path::new("../../scripts/packages/built-by-uv");

    // Without the editable, pytest fails.
    context.pip_install().arg("pytest").assert().success();
    Command::new(context.interpreter())
        .arg("-m")
        .arg("pytest")
        .current_dir(built_by_uv)
        .assert()
        .failure();

    // Build and install the editable. Normally, this should be one step with the editable never
    // been seen, but we have to split it for the test.
    let wheel_dir = TempDir::new()?;
    uv_snapshot!(context
        .build_backend()
        .arg("build-wheel")
        .arg(wheel_dir.path())
        .current_dir(built_by_uv), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    built_by_uv-0.1.0-py3-none-any.whl

    ----- stderr -----
    "###);
    context
        .pip_install()
        .arg(wheel_dir.path().join("built_by_uv-0.1.0-py3-none-any.whl"))
        .assert()
        .success();

    drop(wheel_dir);

    // Now, pytest passes.
    uv_snapshot!(Command::new(context.interpreter())
        .arg("-m")
        .arg("pytest")
        // Avoid showing absolute paths
        .arg("--no-header")
        // Otherwise, the header has a different length on windows
        .arg("--quiet")
        .current_dir(built_by_uv), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    ..                                                                       [100%]
    2 passed in [TIME]

    ----- stderr -----
    "###);

    Ok(())
}
