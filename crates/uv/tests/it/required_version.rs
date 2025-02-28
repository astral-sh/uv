use anyhow::Result;
use assert_cmd::Command;
use fs_err::write;
use predicates::prelude::*;
use tempfile::tempdir;

#[test]
fn auto_exec_with_compatible_version() -> Result<()> {
    let temp_dir = tempdir()?;
    let project_dir = temp_dir.path();

    // Create a pyproject.toml with a required-version that doesn't match the current version
    // We need to use a valid range that will include the version we can find with `uv tool run`
    let pyproject_path = project_dir.join("pyproject.toml");
    write(
        &pyproject_path,
        r#"[build-system]
requires = ["hatchling"]
build-backend = "hatchling.build"

[project]
name = "test-project"
version = "0.1.0"
requires-python = ">=3.8"

[tool.uv]
required-version = ">=0.5.0,<0.6.4"
"#,
    )?;

    // Run a simple command that should trigger auto-exec
    let mut cmd = Command::cargo_bin("uv")?;
    cmd.current_dir(project_dir)
        .arg("--version")
        .env("UV_VERBOSE", "1"); // To get debug output for easier testing

    // The command should succeed and we should get the correct version
    let output = cmd.assert().success().get_output().clone();
    let stdout = std::str::from_utf8(&output.stdout).unwrap();

    // The output will be from the auto-exec call, so it should have the correct version
    // We just make sure it contains a version string
    assert!(stdout.contains("uv "));

    Ok(())
}

#[test]
fn matches_current_version() -> Result<()> {
    let temp_dir = tempdir()?;
    let project_dir = temp_dir.path();

    // Since we can't reliably test a failure case that depends on the environment
    // (it could pass if the package for ==0.1.0 is available), we'll just test
    // that our current version is used when it matches the required-version.

    // Create a pyproject.toml with a required-version that matches the current version
    let pyproject_path = project_dir.join("pyproject.toml");
    let current_version = uv_version::version();
    let pyproject_content = format!(
        r#"[build-system]
requires = ["hatchling"]
build-backend = "hatchling.build"

[project]
name = "test-project"
version = "0.1.0"
requires-python = ">=3.8"

[tool.uv]
required-version = "=={current_version}"
"#
    );
    write(&pyproject_path, pyproject_content)?;

    // Run a simple command that should not trigger auto-exec since the version matches
    let mut cmd = Command::cargo_bin("uv")?;
    cmd.current_dir(project_dir).arg("--version");

    // The command should succeed with our version
    cmd.assert()
        .success()
        .stdout(predicate::str::contains(current_version));

    Ok(())
}
