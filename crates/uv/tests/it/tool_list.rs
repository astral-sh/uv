use anyhow::Result;
use assert_cmd::assert::OutputAssertExt;
use assert_fs::fixture::PathChild;
use fs_err as fs;
use insta::assert_snapshot;
use uv_static::EnvVars;
use uv_test::uv_snapshot;

#[test]
fn tool_list() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`
    context
        .tool_install()
        .arg("black==24.2.0")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .assert()
        .success();

    uv_snapshot!(context.filters(), context.tool_list()
    .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
    .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    black v24.2.0
    - black
    - blackd

    ----- stderr -----
    ");
}

#[test]
fn tool_list_paths() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`
    context
        .tool_install()
        .arg("black==24.2.0")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .assert()
        .success();

    uv_snapshot!(context.filters(), context.tool_list().arg("--show-paths")
    .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
    .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    black v24.2.0 ([TEMP_DIR]/tools/black)
    - black ([TEMP_DIR]/bin/black)
    - blackd ([TEMP_DIR]/bin/blackd)

    ----- stderr -----
    ");
}

#[cfg(windows)]
#[test]
fn tool_list_paths_windows() {
    let context = uv_test::test_context!("3.12")
        .clear_filters()
        .with_filtered_windows_temp_dir();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`
    context
        .tool_install()
        .arg("black==24.2.0")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .assert()
        .success();

    uv_snapshot!(context.filters_without_standard_filters(), context.tool_list().arg("--show-paths")
    .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
    .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    black v24.2.0 ([TEMP_DIR]\tools\black)
    - black ([TEMP_DIR]\bin\black.exe)
    - blackd ([TEMP_DIR]\bin\blackd.exe)

    ----- stderr -----
    "###);
}

#[test]
fn tool_list_empty() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    uv_snapshot!(context.filters(), context.tool_list()
    .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
    .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    No tools installed
    ");
}

#[test]
fn tool_list_missing_receipt() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`
    context
        .tool_install()
        .arg("black==24.2.0")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .assert()
        .success();

    fs_err::remove_file(tool_dir.join("black").join("uv-receipt.toml")).unwrap();

    uv_snapshot!(context.filters(), context.tool_list()
    .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
    .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: Ignoring malformed tool `black` (run `uv tool uninstall black` to remove)
    ");
}

#[test]
fn tool_list_bad_environment() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_python_names()
        .with_filtered_virtualenv_bin()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`
    context
        .tool_install()
        .arg("black==24.2.0")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .assert()
        .success();

    // Install `ruff`
    context
        .tool_install()
        .arg("ruff==0.3.4")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .assert()
        .success();

    let venv_path = uv_test::venv_bin_path(tool_dir.path().join("black"));
    // Remove the python interpreter for black
    fs::remove_dir_all(venv_path.clone())?;

    uv_snapshot!(
        context.filters(),
        context
            .tool_list()
            .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
            .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    ruff v0.3.4
    - ruff

    ----- stderr -----
    warning: Invalid environment at `tools/black`: missing Python executable at `tools/black/[BIN]/[PYTHON]` (run `uv tool install black --reinstall` to reinstall)
    "
    );

    Ok(())
}

#[test]
fn tool_list_deprecated() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`
    context
        .tool_install()
        .arg("black==24.2.0")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .assert()
        .success();

    // Ensure that we have a modern tool receipt.
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        [tool]
        requirements = [{ name = "black", specifier = "==24.2.0" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    // Replace with a legacy receipt.
    fs::write(
        tool_dir.join("black").join("uv-receipt.toml"),
        r#"
        [tool]
        requirements = ["black==24.2.0"]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]
        "#,
    )?;

    // Ensure that we can still list the tool.
    uv_snapshot!(context.filters(), context.tool_list()
    .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
    .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    black v24.2.0
    - black
    - blackd

    ----- stderr -----
    ");

    // Replace with an invalid receipt.
    fs::write(
        tool_dir.join("black").join("uv-receipt.toml"),
        r#"
        [tool]
        requirements = ["black<>24.2.0"]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]
        "#,
    )?;

    // Ensure that listing fails.
    uv_snapshot!(context.filters(), context.tool_list()
    .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
    .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: Ignoring malformed tool `black` (run `uv tool uninstall black` to remove)
    ");

    Ok(())
}

#[test]
fn tool_list_show_version_specifiers() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black` with a version specifier
    context
        .tool_install()
        .arg("black<24.3.0")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .assert()
        .success();

    // Install `flask`
    context
        .tool_install()
        .arg("flask")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .assert()
        .success();

    uv_snapshot!(context.filters(), context.tool_list().arg("--show-version-specifiers")
    .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
    .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    black v24.2.0 [required: <24.3.0]
    - black
    - blackd
    flask v3.0.2
    - flask

    ----- stderr -----
    ");

    // with paths
    uv_snapshot!(context.filters(), context.tool_list().arg("--show-version-specifiers").arg("--show-paths")
    .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
    .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    black v24.2.0 [required: <24.3.0] ([TEMP_DIR]/tools/black)
    - black ([TEMP_DIR]/bin/black)
    - blackd ([TEMP_DIR]/bin/blackd)
    flask v3.0.2 ([TEMP_DIR]/tools/flask)
    - flask ([TEMP_DIR]/bin/flask)

    ----- stderr -----
    ");
}

#[test]
fn tool_list_show_with() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black` without additional requirements
    context
        .tool_install()
        .arg("black==24.2.0")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .assert()
        .success();

    // Install `flask` with additional requirements
    context
        .tool_install()
        .arg("flask")
        .arg("--with")
        .arg("requests")
        .arg("--with")
        .arg("black==24.2.0")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .assert()
        .success();

    // Install `ruff` with version specifier and additional requirements
    context
        .tool_install()
        .arg("ruff==0.3.4")
        .arg("--with")
        .arg("requests")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .assert()
        .success();

    // Test with --show-with
    uv_snapshot!(context.filters(), context.tool_list().arg("--show-with")
    .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
    .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    black v24.2.0
    - black
    - blackd
    flask v3.0.2 [with: requests, black==24.2.0]
    - flask
    ruff v0.3.4 [with: requests]
    - ruff

    ----- stderr -----
    ");

    // Test with both --show-with and --show-paths
    uv_snapshot!(context.filters(), context.tool_list().arg("--show-with").arg("--show-paths")
    .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
    .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    black v24.2.0 ([TEMP_DIR]/tools/black)
    - black ([TEMP_DIR]/bin/black)
    - blackd ([TEMP_DIR]/bin/blackd)
    flask v3.0.2 [with: requests, black==24.2.0] ([TEMP_DIR]/tools/flask)
    - flask ([TEMP_DIR]/bin/flask)
    ruff v0.3.4 [with: requests] ([TEMP_DIR]/tools/ruff)
    - ruff ([TEMP_DIR]/bin/ruff)

    ----- stderr -----
    ");

    // Test with both --show-with and --show-version-specifiers
    uv_snapshot!(context.filters(), context.tool_list().arg("--show-with").arg("--show-version-specifiers")
    .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
    .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    black v24.2.0 [required: ==24.2.0]
    - black
    - blackd
    flask v3.0.2 [with: requests, black==24.2.0]
    - flask
    ruff v0.3.4 [required: ==0.3.4] [with: requests]
    - ruff

    ----- stderr -----
    ");

    // Test with all flags
    uv_snapshot!(context.filters(), context.tool_list()
    .arg("--show-with")
    .arg("--show-version-specifiers")
    .arg("--show-paths")
    .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
    .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    black v24.2.0 [required: ==24.2.0] ([TEMP_DIR]/tools/black)
    - black ([TEMP_DIR]/bin/black)
    - blackd ([TEMP_DIR]/bin/blackd)
    flask v3.0.2 [with: requests, black==24.2.0] ([TEMP_DIR]/tools/flask)
    - flask ([TEMP_DIR]/bin/flask)
    ruff v0.3.4 [required: ==0.3.4] [with: requests] ([TEMP_DIR]/tools/ruff)
    - ruff ([TEMP_DIR]/bin/ruff)

    ----- stderr -----
    ");
}

#[test]
fn tool_list_show_extras() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black` without extras
    context
        .tool_install()
        .arg("black==24.2.0")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .assert()
        .success();

    // Install `flask` with extras and additional requirements
    context
        .tool_install()
        .arg("flask[async,dotenv]")
        .arg("--with")
        .arg("requests")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .assert()
        .success();

    // Test with --show-extras only
    uv_snapshot!(context.filters(), context.tool_list().arg("--show-extras")
    .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
    .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    black v24.2.0
    - black
    - blackd
    flask v3.0.2 [extras: async, dotenv]
    - flask

    ----- stderr -----
    ");

    // Test with both --show-extras and --show-with
    uv_snapshot!(context.filters(), context.tool_list().arg("--show-extras").arg("--show-with")
    .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
    .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    black v24.2.0
    - black
    - blackd
    flask v3.0.2 [extras: async, dotenv] [with: requests]
    - flask

    ----- stderr -----
    ");

    // Test with --show-extras and --show-paths
    uv_snapshot!(context.filters(), context.tool_list().arg("--show-extras").arg("--show-paths")
    .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
    .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    black v24.2.0 ([TEMP_DIR]/tools/black)
    - black ([TEMP_DIR]/bin/black)
    - blackd ([TEMP_DIR]/bin/blackd)
    flask v3.0.2 [extras: async, dotenv] ([TEMP_DIR]/tools/flask)
    - flask ([TEMP_DIR]/bin/flask)

    ----- stderr -----
    ");

    // Test with --show-extras and --show-version-specifiers
    uv_snapshot!(context.filters(), context.tool_list().arg("--show-extras").arg("--show-version-specifiers")
    .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
    .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    black v24.2.0 [required: ==24.2.0]
    - black
    - blackd
    flask v3.0.2 [extras: async, dotenv]
    - flask

    ----- stderr -----
    ");

    // Test with all flags including --show-extras
    uv_snapshot!(context.filters(), context.tool_list()
    .arg("--show-extras")
    .arg("--show-with")
    .arg("--show-version-specifiers")
    .arg("--show-paths")
    .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
    .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    black v24.2.0 [required: ==24.2.0] ([TEMP_DIR]/tools/black)
    - black ([TEMP_DIR]/bin/black)
    - blackd ([TEMP_DIR]/bin/blackd)
    flask v3.0.2 [extras: async, dotenv] [with: requests] ([TEMP_DIR]/tools/flask)
    - flask ([TEMP_DIR]/bin/flask)

    ----- stderr -----
    ");
}

#[test]
fn tool_list_show_python() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black` with python 3.12
    context
        .tool_install()
        .arg("black==24.2.0")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .assert()
        .success();

    // Test with --show-python
    uv_snapshot!(context.filters(), context.tool_list().arg("--show-python")
    .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
    .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    black v24.2.0 [CPython 3.12.[X]]
    - black
    - blackd

    ----- stderr -----
    ");
}

#[test]
fn tool_list_show_all() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black` without extras
    context
        .tool_install()
        .arg("black==24.2.0")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .assert()
        .success();

    // Install `flask` with extras and additional requirements
    context
        .tool_install()
        .arg("flask[async,dotenv]")
        .arg("--with")
        .arg("requests")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .assert()
        .success();

    // Test with all flags
    uv_snapshot!(context.filters(), context.tool_list()
    .arg("--show-extras")
    .arg("--show-with")
    .arg("--show-version-specifiers")
    .arg("--show-paths")
    .arg("--show-python")
    .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
    .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    black v24.2.0 [required: ==24.2.0] [CPython 3.12.[X]] ([TEMP_DIR]/tools/black)
    - black ([TEMP_DIR]/bin/black)
    - blackd ([TEMP_DIR]/bin/blackd)
    flask v3.0.2 [extras: async, dotenv] [with: requests] [CPython 3.12.[X]] ([TEMP_DIR]/tools/flask)
    - flask ([TEMP_DIR]/bin/flask)

    ----- stderr -----
    ");
}
