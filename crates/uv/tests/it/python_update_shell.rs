use assert_cmd::assert::OutputAssertExt;
use assert_fs::fixture::PathChild;

use uv_static::EnvVars;

use crate::common::{TestContext, uv_snapshot};

#[cfg(not(windows))]
mod unix {
    use super::*;

    #[test]
    fn python_update_shell_not_in_path() {
        let context = TestContext::new("3.12");

        // Zsh uses .zshenv, not .zshrc
        let shell_config = context.home_dir.child(".zshenv");

        uv_snapshot!(context.filters(), context
            .python_update_shell()
            .env(EnvVars::HOME, context.home_dir.as_os_str())
            .env("SHELL", "/bin/zsh"), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Created configuration file: [HOME]/.zshenv
        Restart your shell to apply changes
        "###);

        // Verify the file was created with the correct content
        let contents = fs_err::read_to_string(shell_config.path()).unwrap();
        assert!(contents.contains("export PATH="));
        assert!(contents.contains("# uv"));
    }

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
        uv_snapshot!(context.filters(), context
            .python_update_shell()
            .env(EnvVars::HOME, context.home_dir.as_os_str())
            .env(EnvVars::UV_PYTHON_BIN_DIR, bin_dir.path())
            .env(EnvVars::PATH, path_with_bin)
            .env("SHELL", "/bin/zsh"), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Executable directory [HOME]/bin is already in PATH
        "###);
    }

    #[test]
    fn python_update_shell_force() {
        let context = TestContext::new("3.12");

        // Zsh uses .zshenv, not .zshrc
        let shell_config = context.home_dir.child(".zshenv");

        // First run - add to PATH
        context
            .python_update_shell()
            .env(EnvVars::HOME, context.home_dir.as_os_str())
            .env("SHELL", "/bin/zsh")
            .assert()
            .success();

        let first_contents = fs_err::read_to_string(shell_config.path()).unwrap();
        let first_count = first_contents.matches("export PATH=").count();

        // Second run with --force - should update
        uv_snapshot!(context.filters(), context
            .python_update_shell()
            .arg("--force")
            .env(EnvVars::HOME, context.home_dir.as_os_str())
            .env("SHELL", "/bin/zsh"), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Force updated configuration file: [HOME]/.zshenv
        Restart your shell to apply changes
        "###);

        // Verify only one PATH export exists (old one removed, new one added)
        let second_contents = fs_err::read_to_string(shell_config.path()).unwrap();
        let second_count = second_contents.matches("export PATH=").count();
        assert_eq!(
            first_count, second_count,
            "Should have same number of PATH exports"
        );

        // Verify the command is at the end
        assert!(
            second_contents.trim_end().ends_with("export PATH=")
                || second_contents.trim_end().ends_with('"')
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
    fn python_update_shell_not_in_path() {
        let context = TestContext::new("3.12");

        // On Windows, PATH is updated in the registry, not a config file
        uv_snapshot!(context.filters(), context
            .python_update_shell()
            .env(EnvVars::HOME, context.home_dir.as_os_str()), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Updated PATH to include executable directory [USER_CONFIG_DIR]/data/../bin
        Restart your shell to apply changes
        "###);
    }

    #[test]
    fn python_update_shell_already_in_path() {
        let context = TestContext::new("3.12");

        // Set a specific bin directory using UV_PYTHON_BIN_DIR
        let bin_dir = context.home_dir.child("bin");
        fs_err::create_dir_all(bin_dir.path()).unwrap();

        // On Windows, contains_path checks the registry, not env vars.
        // Since we can't easily set up the registry in tests, we just verify
        // the command succeeds.
        context
            .python_update_shell()
            .env(EnvVars::HOME, context.home_dir.as_os_str())
            .env(EnvVars::UV_PYTHON_BIN_DIR, bin_dir.path())
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
