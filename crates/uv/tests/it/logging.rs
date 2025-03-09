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
    uv_snapshot!(context.filters(), context.add().arg("iniconfig").arg("--log").arg("log_test"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Project is missing a `[project]` table; add a `[project]` table to use production dependencies, or run `uv add --dev` instead
    See [TEMP_DIR]/log_test.log for detailed logs
    ");

    // Invalid log path should give error
    uv_snapshot!(context.filters(), context.add().arg("iniconfig").arg("--log").arg("foo/test"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Error writing to log file: foo/test.log
      Caused by: No such file or directory (os error 2)
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
    uv_snapshot!(context.filters(), context.python_install().arg("--log").arg("log_test"), @r"
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
    for verbose_count in 0..3 {
        let mut cmd = context.python_install();
        // Added the quite flag to avoid the "Installed Python" message and already installed message
        cmd.arg("--log").arg(log_path.as_ref()).arg("-q");
        for _ in 0..verbose_count {
            cmd.arg("-l");
        }

        cmd.output()?;

        // Verify log content matches verbosity
        let log_content = fs_err::read_to_string(log_path.path().with_extension("log"))?;
        match verbose_count {
            0 => assert!(!log_content.contains("TRACE")),
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

    context
        .python_install()
        .arg("--log")
        .arg(log_path.as_ref())
        .output()?;

    let initial_content = fs_err::read_to_string(log_path.path().with_extension("log"))?;

    // Second command should append
    context
        .python_install()
        .arg("--log")
        .arg(log_path.as_ref())
        .output()?;

    let final_content = fs_err::read_to_string(log_path.path().with_extension("log"))?;
    assert!(final_content.len() > initial_content.len());
    Ok(())
}

// Other tests from the test suite can be used here to test file_logging behavior
#[test]
fn test_command_output_matches_log_content() -> Result<()> {
    let context = TestContext::new("3.12")
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs();
    let log_path = context.temp_dir.child("output_comparison.log");

    // Run command with verbose output and capture it
    let command_output = context
        .python_install()
        .arg("--log")
        .arg(log_path.as_ref())
        .arg("-v") // Use verbose output to get more detailed logs
        .output()?;

    // Get the stderr as string
    let stderr = String::from_utf8_lossy(&command_output.stderr).to_string();

    // Read the log file
    let log_content = fs_err::read_to_string(log_path.path().with_extension("log"))?;

    // Verify these messages exist in the log
    for msg in stderr.lines() {
        if ["DEBUG", "INFO", "WARN", "ERROR", "TRACE"]
            .iter()
            .any(|prefix| msg.starts_with(prefix))
        {
            assert!(
                log_content.contains(msg),
                "Log file missing message that was in command output: '{msg}'"
            );
        }
    }

    Ok(())
}
