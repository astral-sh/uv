use assert_cmd::assert::OutputAssertExt;
use assert_fs::fixture::PathChild;

use uv_static::EnvVars;

use crate::common::TestContext;

#[cfg(not(windows))]
mod unix {
    use super::*;

    #[test]
    fn python_update_shell_already_in_path() {
        let context = TestContext::new("3.12");

        // Set a specific bin directory using UV_PYTHON_BIN_DIR
        let bin_dir = context.home_dir.child("bin");
        fs_err::create_dir_all(bin_dir.path()).unwrap();

        // Set PATH to include the bin directory so it's "already in PATH"
        let path_with_bin =
            std::env::join_paths(std::iter::once(bin_dir.path().to_path_buf()).chain(
                std::env::split_paths(&std::env::var(EnvVars::PATH).unwrap_or_default()),
            ))
            .unwrap();

        // Run without --force - should skip because it's already in PATH
        let output = context
            .python_update_shell()
            .env(EnvVars::HOME, context.home_dir.as_os_str())
            .env(EnvVars::UV_PYTHON_BIN_DIR, bin_dir.path())
            .env(EnvVars::PATH, path_with_bin)
            .env("SHELL", "/bin/zsh")
            .assert()
            .success();

        let stderr = String::from_utf8_lossy(&output.get_output().stderr);
        assert!(
            stderr.contains("is already in PATH"),
            "Expected 'is already in PATH' message, got: {}",
            stderr
        );
    }

    #[test]
    fn python_update_shell_force_with_existing_path() {
        let context = TestContext::new("3.12");

        // Set a specific bin directory using UV_PYTHON_BIN_DIR
        let bin_dir = context.home_dir.child("bin");
        fs_err::create_dir_all(bin_dir.path()).unwrap();

        // Set PATH to include the bin directory so it's "already in PATH"
        let path_with_bin =
            std::env::join_paths(std::iter::once(bin_dir.path().to_path_buf()).chain(
                std::env::split_paths(&std::env::var(EnvVars::PATH).unwrap_or_default()),
            ))
            .unwrap();

        // Run with --force - should update even though it's already in PATH
        let output = context
            .python_update_shell()
            .arg("--force")
            .env(EnvVars::HOME, context.home_dir.as_os_str())
            .env(EnvVars::UV_PYTHON_BIN_DIR, bin_dir.path())
            .env(EnvVars::PATH, path_with_bin)
            .env("SHELL", "/bin/zsh")
            .assert()
            .success();

        let stderr = String::from_utf8_lossy(&output.get_output().stderr);
        // With --force, it should update the config file (not skip)
        assert!(
            stderr.contains("Force updated") || stderr.contains("Updated"),
            "Expected update message, got: {}",
            stderr
        );
    }
}

#[cfg(windows)]
mod windows {
    use super::*;
    use std::process::Command;

    /// Query the user PATH from the Windows registry.
    fn get_registry_path() -> String {
        let output = Command::new("reg")
            .args(["query", "HKCU\\Environment", "/v", "Path"])
            .output()
            .expect("Failed to query registry");
        String::from_utf8_lossy(&output.stdout).to_string()
    }

    #[test]
    fn python_update_shell_succeeds() {
        let context = TestContext::new("3.12");

        // On Windows, PATH is updated in the registry
        context
            .python_update_shell()
            .env(EnvVars::HOME, context.home_dir.as_os_str())
            .assert()
            .success();
    }

    #[test]
    fn python_update_shell_force() {
        let context = TestContext::new("3.12");

        // Set a specific bin directory
        let bin_dir = context.home_dir.child("bin");
        fs_err::create_dir_all(bin_dir.path()).unwrap();

        // First run - add to PATH
        context
            .python_update_shell()
            .env(EnvVars::HOME, context.home_dir.as_os_str())
            .env(EnvVars::UV_PYTHON_BIN_DIR, bin_dir.path())
            .assert()
            .success();

        // Verify the path is in the registry
        let registry_path = get_registry_path();
        let bin_path_str = bin_dir.path().to_string_lossy();
        assert!(
            registry_path.contains(&*bin_path_str),
            "Path should be in registry after first run: {}",
            registry_path
        );

        // Second run with --force - should move to front
        context
            .python_update_shell()
            .arg("--force")
            .env(EnvVars::HOME, context.home_dir.as_os_str())
            .env(EnvVars::UV_PYTHON_BIN_DIR, bin_dir.path())
            .assert()
            .success();

        // Verify it's still in the registry after --force
        let registry_path_after = get_registry_path();
        assert!(
            registry_path_after.contains(&*bin_path_str),
            "Path should still be in registry after --force: {}",
            registry_path_after
        );
    }
}
