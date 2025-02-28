use crate::common::{uv_snapshot, TestContext};
use anyhow::Result;
use assert_fs::prelude::*;
use indoc::indoc;

// Picking a random test that gives error and add logging to it
#[test]
fn logging_on_fail() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r"
        [tool.uv.workspace]
        members = []
    "})?;

    // Adding `iniconfig` should fail, since virtual workspace roots don't support production
    // dependencies.
    uv_snapshot!(context.filters(), context.add().arg("iniconfig").arg("--log").arg("test"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Project is missing a `[project]` table; add a `[project]` table to use production dependencies, or run `uv add --dev` instead
    See [TEMP_DIR]/test.log for detailed logs
    ");

    // Should give clap error if `--log` used but no argument provided
    uv_snapshot!(context.filters(), context.add().arg("iniconfig").arg("--log"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: a value is required for '--log <PATH>' but none was supplied

    For more information, try '--help'.
    ");
    Ok(())
}

#[test]
fn logs_on_pass() {
    let context: TestContext = TestContext::new_with_versions(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs();

    // No see <path> for detailed logs on pass
    uv_snapshot!(context.filters(), context.python_install().arg("--log").arg("foo"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.2 in [TIME]
     + cpython-3.13.2-[PLATFORM]
    ");
}

#[test]
fn test_log_levels() -> Result<()> {
    let context: TestContext = TestContext::new("3.12")
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs();
    let log_path = context.temp_dir.child("test.log");

    // Test different verbosity levels
    for verbose_count in 0..=3 {
        let mut cmd = context.python_install();
        // Added the quite flag to avoid the "Installed Python" message and already installed message
        cmd.arg("--log").arg(log_path.as_ref()).arg("-q");
        for _ in 0..verbose_count {
            cmd.arg("-l");
        }

        insta::allow_duplicates!({
            uv_snapshot!(context.filters(), cmd, @r"
            success: true
            exit_code: 0
            ----- stdout -----

            ----- stderr -----
            ")
        });

        // Verify log content matches verbosity
        let log_content = fs_err::read_to_string(log_path.path().with_extension("log"))?;
        match verbose_count {
            0 | 1 => assert!(!log_content.contains("TRACE")),
            _ => assert!(log_content.contains("TRACE")),
        }
    }
    Ok(())
}

#[test]
fn test_log_file_append() -> Result<()> {
    let context = TestContext::new("3.12")
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs();
    let log_path = context.temp_dir.child("append_test.log");

    // First command
    uv_snapshot!(context
        .python_install()
        .arg("--log")
        .arg(log_path.as_ref())
        .arg("-q"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    ");

    let initial_content = fs_err::read_to_string(log_path.path().with_extension("log"))?;

    // Second command should append
    uv_snapshot!(context
        .python_install()
        .arg("--log")
        .arg(log_path.as_ref())
        .arg("-q"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    
    ----- stderr -----
    "###);

    let final_content = fs_err::read_to_string(log_path.path().with_extension("log"))?;
    assert!(final_content.len() > initial_content.len());
    Ok(())
}
