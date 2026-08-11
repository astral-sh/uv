use anyhow::Result;
use assert_fs::fixture::PathChild;
use insta::assert_snapshot;

use uv_static::EnvVars;
use uv_test::uv_snapshot;

#[test]
fn update_shell_tool_and_python() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);
    let tool_directory = context.temp_dir.child("tool-bin");
    let python_directory = context.temp_dir.child("python-bin");

    // The tool updater creates both Bash startup files when neither already exists.
    uv_snapshot!(
        context.filters(),
        context
            .command()
            .arg("tool")
            .arg("update-shell")
            .env(EnvVars::UV_TOOL_BIN_DIR, tool_directory.as_os_str()),
        @"
        exit_code: 0 (success)
        ----- stderr -----
        Created configuration file: [HOME]/.bash_profile
        Created configuration file: [HOME]/.bashrc
        Restart your shell to apply changes
        "
    );

    // The Python updater modifies those existing files instead of replacing them.
    uv_snapshot!(
        context.filters(),
        context
            .command()
            .arg("python")
            .arg("update-shell")
            .env(EnvVars::UV_PYTHON_BIN_DIR, python_directory.as_os_str()),
        @"
        exit_code: 0 (success)
        ----- stderr -----
        Updated configuration file: [HOME]/.bash_profile
        Updated configuration file: [HOME]/.bashrc
        Restart your shell to apply changes
        "
    );

    let bash_profile = fs_err::read_to_string(context.home_dir.child(".bash_profile"))?;
    let bashrc = fs_err::read_to_string(context.home_dir.child(".bashrc"))?;
    assert_eq!(bash_profile, bashrc);

    // Both startup files retain the separate tool and Python executable directories.
    insta::with_settings!({ filters => context.filters() }, {
        assert_snapshot!(bash_profile, @r#"
        # uv
        export PATH="[TEMP_DIR]/tool-bin:$PATH"

        # uv
        export PATH="[TEMP_DIR]/python-bin:$PATH"
        "#);
    });

    // Repeating the tool updater before restarting reports that the files are already up to date.
    uv_snapshot!(
        context.filters(),
        context
            .command()
            .arg("tool")
            .arg("update-shell")
            .env(EnvVars::UV_TOOL_BIN_DIR, tool_directory.as_os_str()),
        @"
        exit_code: 2 (failure)
        ----- stderr -----
        error: The executable directory [TEMP_DIR]/tool-bin is not in PATH, but the Bash configuration files are already up-to-date
        "
    );

    Ok(())
}
