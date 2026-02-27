use anyhow::Result;
use assert_cmd::prelude::*;
use assert_fs::prelude::*;
use indoc::indoc;
use uv_fs::copy_dir_all;
use uv_static::EnvVars;
use uv_test::{uv_snapshot, venv_bin_path};

#[test]
fn tool_run_args() {
    let context = uv_test::test_context!("3.12").with_filtered_counts();
    let context = context
        .with_filter((
            r"Usage: uv(\.exe)? tool run \[OPTIONS\] (?s).*",
            "[UV TOOL RUN HELP]",
        ))
        .with_filter((r"usage: pytest \[options\] (?s).*", "[PYTEST HELP]"));
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // We treat arguments before the command as uv tool run arguments
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--help")
        .arg("pytest")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Run a command provided by a Python package

    [UV TOOL RUN HELP]
    ");

    // We don't treat arguments after the command as uv tool run arguments
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("pytest")
        .arg("--help")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    [PYTEST HELP]
    ");

    // Can use `--` to separate uv arguments from the command arguments.
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--")
        .arg("pytest")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    pytest 8.1.1

    ----- stderr -----
    Resolved [N] packages in [TIME]
    ");
}

#[test]
fn tool_run_at_version() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("pytest@8.0.0")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    pytest 8.0.0

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + iniconfig==2.0.0
     + packaging==24.0
     + pluggy==1.4.0
     + pytest==8.0.0
    ");

    // Empty versions are just treated as package and command names
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("pytest@")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to parse: `pytest@`
      Caused by: Expected URL
    pytest@
           ^
    ");

    // Invalid versions are just treated as package and command names
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("pytest@invalid")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to resolve tool requirement
      ╰─▶ Distribution not found at: file://[TEMP_DIR]/invalid
    ");

    let filters = context
        .filters()
        .into_iter()
        .chain([(
            // The error message is different on Windows
            "Caused by: program not found",
            "Caused by: No such file or directory (os error 2)",
        )])
        .collect::<Vec<_>>();

    // When `--from` is used, `@` is not treated as a version request
    uv_snapshot!(filters, context.tool_run()
        .arg("--from")
        .arg("pytest")
        .arg("pytest@8.0.0")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 4 packages in [TIME]
     + iniconfig==2.0.0
     + packaging==24.0
     + pluggy==1.4.0
     + pytest==8.1.1
    An executable named `pytest@8.0.0` is not provided by package `pytest`.
    The following executables are available:
    - py.test
    - pytest
    ");
}

#[test]
fn tool_run_from_version() {
    let context = uv_test::test_context!("3.12");
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--from")
        .arg("pytest==8.0.0")
        .arg("pytest")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    pytest 8.0.0

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + iniconfig==2.0.0
     + packaging==24.0
     + pluggy==1.4.0
     + pytest==8.0.0
    ");
}

#[test]
fn tool_run_constraints() {
    let context = uv_test::test_context!("3.12");
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    let constraints_txt = context.temp_dir.child("constraints.txt");
    constraints_txt.write_str("pluggy<1.4.0").unwrap();

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--constraints")
        .arg("constraints.txt")
        .arg("pytest")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    pytest 8.0.2

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + iniconfig==2.0.0
     + packaging==24.0
     + pluggy==1.3.0
     + pytest==8.0.2
    ");
}

#[test]
fn tool_run_overrides() {
    let context = uv_test::test_context!("3.12");
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    let overrides_txt = context.temp_dir.child("overrides.txt");
    overrides_txt.write_str("pluggy<1.4.0").unwrap();

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--overrides")
        .arg("overrides.txt")
        .arg("pytest")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    pytest 8.1.1

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + iniconfig==2.0.0
     + packaging==24.0
     + pluggy==1.3.0
     + pytest==8.1.1
    ");
}

#[test]
fn tool_run_suggest_valid_commands() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    uv_snapshot!(context.filters(), context.tool_run()
    .arg("--from")
    .arg("black")
    .arg("orange")
    .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
    .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 6 packages in [TIME]
    Installed 6 packages in [TIME]
     + black==24.3.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    An executable named `orange` is not provided by package `black`.
    The following executables are available:
    - black
    - blackd
    ");

    uv_snapshot!(context.filters(), context.tool_run()
    .arg("fastapi-cli")
    .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
    .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + fastapi-cli==0.0.1
     + importlib-metadata==1.7.0
     + zipp==3.18.1
    Package `fastapi-cli` does not provide any executables.
    ");
}

#[test]
fn tool_run_warn_executable_not_in_from() {
    // FastAPI 0.111 is only available from this date onwards.
    let context = uv_test::test_context!("3.12")
        .with_exclude_newer("2024-05-04T00:00:00Z")
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    let context = context
        .with_filter(("\\+ uvloop(.+)\n ", ""))
        // Strip off the `fastapi` command output.
        .with_filter(("(?s)fastapi` instead.*", "fastapi` instead."));

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--from")
        .arg("fastapi")
        .arg("fastapi")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 35 packages in [TIME]
    Prepared 35 packages in [TIME]
    Installed 35 packages in [TIME]
     + annotated-types==0.6.0
     + anyio==4.3.0
     + certifi==2024.2.2
     + click==8.1.7
     + dnspython==2.6.1
     + email-validator==2.1.1
     + fastapi==0.111.0
     + fastapi-cli==0.0.2
     + h11==0.14.0
     + httpcore==1.0.5
     + httptools==0.6.1
     + httpx==0.27.0
     + idna==3.7
     + jinja2==3.1.3
     + markdown-it-py==3.0.0
     + markupsafe==2.1.5
     + mdurl==0.1.2
     + orjson==3.10.3
     + pydantic==2.7.1
     + pydantic-core==2.18.2
     + pygments==2.17.2
     + python-dotenv==1.0.1
     + python-multipart==0.0.9
     + pyyaml==6.0.1
     + rich==13.7.1
     + shellingham==1.5.4
     + sniffio==1.3.1
     + starlette==0.37.2
     + typer==0.12.3
     + typing-extensions==4.11.0
     + ujson==5.9.0
     + uvicorn==0.29.0
     + watchfiles==0.21.0
     + websockets==12.0
    warning: An executable named `fastapi` is not provided by package `fastapi` but is available via the dependency `fastapi-cli`. Consider using `uv tool run --from fastapi-cli fastapi` instead.
    ");
}

#[test]
fn tool_run_from_install() {
    let context = uv_test::test_context!("3.12").with_filtered_counts();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black` at a specific version.
    context
        .tool_install()
        .arg("black==24.1.0")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .assert()
        .success();

    // Verify that `tool run black` uses the already-installed version.
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("black")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    black, 24.1.0 (compiled: yes)
    Python (CPython) 3.12.[X]

    ----- stderr -----
    ");

    // Verify that `--isolated` uses an isolated environment.
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--isolated")
        .arg("black")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    black, 24.3.0 (compiled: yes)
    Python (CPython) 3.12.[X]

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + black==24.3.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    ");

    // Verify that `tool run black` at a different version installs the new version.
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("black@24.1.1")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    black, 24.1.1 (compiled: yes)
    Python (CPython) 3.12.[X]

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + black==24.1.1
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    ");

    // Verify that `--with` installs a new version.
    // TODO(charlie): This could (in theory) layer the `--with` requirements on top of the existing
    // environment.
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--with")
        .arg("iniconfig")
        .arg("black")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    black, 24.3.0 (compiled: yes)
    Python (CPython) 3.12.[X]

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + black==24.3.0
     + click==8.1.7
     + iniconfig==2.0.0
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    ");

    // Verify that `tool run black` at a different version (via `--from`) installs the new version.
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--from")
        .arg("black==24.2.0")
        .arg("black")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    black, 24.2.0 (compiled: yes)
    Python (CPython) 3.12.[X]

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + black==24.2.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    ");
}

#[test]
fn tool_run_from_install_constraints() {
    let context = uv_test::test_context!("3.12").with_filtered_counts();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `flask` at a specific version.
    context
        .tool_install()
        .arg("flask==3.0.0")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .assert()
        .success();

    // Verify that `tool run flask` uses the already-installed version.
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("flask")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.[X]
    Flask 3.0.0
    Werkzeug 3.0.1

    ----- stderr -----
    ");

    // Verify that `tool run flask` with a compatible constraint uses the already-installed version.
    context
        .temp_dir
        .child("constraints.txt")
        .write_str("werkzeug<4.0.0")
        .unwrap();

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--constraints")
        .arg("constraints.txt")
        .arg("flask")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.[X]
    Flask 3.0.0
    Werkzeug 3.0.1

    ----- stderr -----
    ");

    // Verify that `tool run flask` with an incompatible constraint installs a new version.
    context
        .temp_dir
        .child("constraints.txt")
        .write_str("werkzeug<3.0.0")
        .unwrap();

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--constraints")
        .arg("constraints.txt")
        .arg("flask")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.[X]
    Flask 2.3.3
    Werkzeug 2.3.8

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + blinker==1.7.0
     + click==8.1.7
     + flask==2.3.3
     + itsdangerous==2.1.2
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + werkzeug==2.3.8
    ");

    // Verify that `tool run flask` with a compatible override uses the already-installed version.
    context
        .temp_dir
        .child("override.txt")
        .write_str("werkzeug==3.0.1")
        .unwrap();

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--override")
        .arg("override.txt")
        .arg("flask")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.[X]
    Flask 3.0.0
    Werkzeug 3.0.1

    ----- stderr -----
    ");

    // Verify that `tool run flask` with an incompatible override installs a new version.
    context
        .temp_dir
        .child("override.txt")
        .write_str("werkzeug==3.0.0")
        .unwrap();

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--override")
        .arg("override.txt")
        .arg("flask")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.[X]
    Flask 3.0.2
    Werkzeug 3.0.0

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + blinker==1.7.0
     + click==8.1.7
     + flask==3.0.2
     + itsdangerous==2.1.2
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + werkzeug==3.0.0
    ");

    // Verify that an override that enables a new extra also invalidates the environment.
    context
        .temp_dir
        .child("override.txt")
        .write_str("flask[dotenv]")
        .unwrap();

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--override")
        .arg("override.txt")
        .arg("flask")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.[X]
    Flask 3.0.2
    Werkzeug 3.0.1

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + blinker==1.7.0
     + click==8.1.7
     + flask==3.0.2
     + itsdangerous==2.1.2
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + python-dotenv==1.0.1
     + werkzeug==3.0.1
    ");
}

#[test]
fn tool_run_cache() {
    let context = uv_test::test_context_with_versions!(&["3.11", "3.12"]).with_filtered_counts();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Verify that `tool run black` installs the latest version.
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("-p")
        .arg("3.12")
        .arg("black")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    black, 24.3.0 (compiled: yes)
    Python (CPython) 3.12.[X]

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + black==24.3.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    ");

    // Verify that `tool run black` uses the cached version.
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("-p")
        .arg("3.12")
        .arg("black")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    black, 24.3.0 (compiled: yes)
    Python (CPython) 3.12.[X]

    ----- stderr -----
    Resolved [N] packages in [TIME]
    ");

    // Verify that `--refresh` allows cache reuse.
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("-p")
        .arg("3.12")
        .arg("--refresh")
        .arg("black")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    black, 24.3.0 (compiled: yes)
    Python (CPython) 3.12.[X]

    ----- stderr -----
    Resolved [N] packages in [TIME]
    ");

    // Verify that `--refresh-package` allows cache reuse.
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("-p")
        .arg("3.12")
        .arg("--refresh-package")
        .arg("packaging")
        .arg("black")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    black, 24.3.0 (compiled: yes)
    Python (CPython) 3.12.[X]

    ----- stderr -----
    Resolved [N] packages in [TIME]
    ");

    // Verify that varying the interpreter leads to a fresh environment.
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("-p")
        .arg("3.11")
        .arg("black")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    black, 24.3.0 (compiled: yes)
    Python (CPython) 3.11.[X]

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + black==24.3.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    ");

    // But that re-invoking with the previous interpreter retains the cached version.
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("-p")
        .arg("3.12")
        .arg("black")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    black, 24.3.0 (compiled: yes)
    Python (CPython) 3.12.[X]

    ----- stderr -----
    Resolved [N] packages in [TIME]
    ");

    // Verify that `--with` leads to a fresh environment.
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("-p")
        .arg("3.12")
        .arg("--with")
        .arg("iniconfig")
        .arg("black")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    black, 24.3.0 (compiled: yes)
    Python (CPython) 3.12.[X]

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + black==24.3.0
     + click==8.1.7
     + iniconfig==2.0.0
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    ");
}

#[test]
fn tool_run_url() {
    let context = uv_test::test_context!("3.12").with_filtered_counts();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--from")
        .arg("flask @ https://files.pythonhosted.org/packages/61/80/ffe1da13ad9300f87c93af113edd0638c75138c42a0994becfacac078c06/flask-3.0.3-py3-none-any.whl")
        .arg("flask")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.[X]
    Flask 3.0.3
    Werkzeug 3.0.1

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + blinker==1.7.0
     + click==8.1.7
     + flask==3.0.3 (from https://files.pythonhosted.org/packages/61/80/ffe1da13ad9300f87c93af113edd0638c75138c42a0994becfacac078c06/flask-3.0.3-py3-none-any.whl)
     + itsdangerous==2.1.2
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + werkzeug==3.0.1
    ");

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--from")
        .arg("https://files.pythonhosted.org/packages/61/80/ffe1da13ad9300f87c93af113edd0638c75138c42a0994becfacac078c06/flask-3.0.3-py3-none-any.whl")
        .arg("flask")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.[X]
    Flask 3.0.3
    Werkzeug 3.0.1

    ----- stderr -----
    Resolved [N] packages in [TIME]
    ");

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("flask @ https://files.pythonhosted.org/packages/61/80/ffe1da13ad9300f87c93af113edd0638c75138c42a0994becfacac078c06/flask-3.0.3-py3-none-any.whl")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.[X]
    Flask 3.0.3
    Werkzeug 3.0.1

    ----- stderr -----
    Resolved [N] packages in [TIME]
    ");

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("https://files.pythonhosted.org/packages/61/80/ffe1da13ad9300f87c93af113edd0638c75138c42a0994becfacac078c06/flask-3.0.3-py3-none-any.whl")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.[X]
    Flask 3.0.3
    Werkzeug 3.0.1

    ----- stderr -----
    Resolved [N] packages in [TIME]
    ");
}

/// Test running a tool with a Git requirement.
#[test]
#[cfg(feature = "test-git")]
fn tool_run_git() {
    let context = uv_test::test_context!("3.12").with_filtered_counts();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("git+https://github.com/psf/black@24.2.0")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    black, 24.2.0 (compiled: no)
    Python (CPython) 3.12.[X]

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + black==24.2.0 (from git+https://github.com/psf/black@6fdf8a4af28071ed1d079c01122b34c5d587207a)
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    ");

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("black @ git+https://github.com/psf/black@24.2.0")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    black, 24.2.0 (compiled: no)
    Python (CPython) 3.12.[X]

    ----- stderr -----
    Resolved [N] packages in [TIME]
    ");

    // Clear the cache.
    fs_err::remove_dir_all(&context.cache_dir).expect("Failed to remove cache dir.");

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--from")
        .arg("git+https://github.com/psf/black@24.2.0")
        .arg("black")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    black, 24.2.0 (compiled: no)
    Python (CPython) 3.12.[X]

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + black==24.2.0 (from git+https://github.com/psf/black@6fdf8a4af28071ed1d079c01122b34c5d587207a)
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    ");

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--from")
        .arg("black @ git+https://github.com/psf/black@24.2.0")
        .arg("black")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    black, 24.2.0 (compiled: no)
    Python (CPython) 3.12.[X]

    ----- stderr -----
    Resolved [N] packages in [TIME]
    ");
}

/// Test running a tool with a Git LFS enabled requirement.
#[test]
#[cfg(feature = "test-git-lfs")]
fn tool_run_git_lfs() {
    let context = uv_test::test_context!("3.13")
        .with_filtered_counts()
        .with_filtered_exe_suffix()
        .with_git_lfs_config();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--lfs")
        .arg("git+https://github.com/astral-sh/test-lfs-repo@54e5eebd3c6851b1353fc7b1e5b4eca11e27581c")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello from test-lfs-repo!

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + test-lfs-repo==0.1.0 (from git+https://github.com/astral-sh/test-lfs-repo@54e5eebd3c6851b1353fc7b1e5b4eca11e27581c#lfs=true)
    ");

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--lfs")
        .arg("test-lfs-repo @ git+https://github.com/astral-sh/test-lfs-repo@54e5eebd3c6851b1353fc7b1e5b4eca11e27581c")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello from test-lfs-repo!

    ----- stderr -----
    Resolved [N] packages in [TIME]
    ");

    // Clear the cache.
    fs_err::remove_dir_all(&context.cache_dir).expect("Failed to remove cache dir.");

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--from")
        .arg("git+https://github.com/astral-sh/test-lfs-repo@54e5eebd3c6851b1353fc7b1e5b4eca11e27581c")
        .arg("--lfs")
        .arg("test-lfs-repo-assets")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello from test-lfs-repo! LFS_TEST=True ANOTHER_LFS_TEST=True

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + test-lfs-repo==0.1.0 (from git+https://github.com/astral-sh/test-lfs-repo@54e5eebd3c6851b1353fc7b1e5b4eca11e27581c#lfs=true)
    ");

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--from")
        .arg("test-lfs-repo @ git+https://github.com/astral-sh/test-lfs-repo@54e5eebd3c6851b1353fc7b1e5b4eca11e27581c")
        .arg("--lfs")
        .arg("test-lfs-repo-assets")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello from test-lfs-repo! LFS_TEST=True ANOTHER_LFS_TEST=True

    ----- stderr -----
    Resolved [N] packages in [TIME]
    ");

    // Clear the cache.
    fs_err::remove_dir_all(&context.cache_dir).expect("Failed to remove cache dir.");

    // Attempt to run when LFS artifacts are missing and LFS is requested.

    // The filters below will remove any boilerplate before what we actually want to match.
    // They help handle slightly different output in uv-distribution/src/source/mod.rs between
    // calls to `git` and `git_metadata` functions which don't have guaranteed execution order.
    // In addition, we can get different error codes depending on where the failure occurs,
    // although we know the error code cannot be 0.
    let context = context
        .with_filter((r"exit_code: -?[1-9]\d*", "exit_code: [ERROR_CODE]"))
        .with_filter((
            "(?s)(----- stderr -----).*?The source distribution `[^`]+` is missing Git LFS artifacts.*",
            "$1\n[PREFIX]The source distribution `[DISTRIBUTION]` is missing Git LFS artifacts",
        ));

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--lfs")
        .arg("test-lfs-repo @ git+https://github.com/astral-sh/test-lfs-repo@54e5eebd3c6851b1353fc7b1e5b4eca11e27581c")
        .env(EnvVars::UV_INTERNAL__TEST_LFS_DISABLED, "1")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: false
    exit_code: [ERROR_CODE]
    ----- stdout -----

    ----- stderr -----
    [PREFIX]The source distribution `[DISTRIBUTION]` is missing Git LFS artifacts
    ");

    // Attempt to run when LFS artifacts are missing but LFS was not requested.
    #[cfg(not(windows))]
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--from")
        .arg("test-lfs-repo @ git+https://github.com/astral-sh/test-lfs-repo@54e5eebd3c6851b1353fc7b1e5b4eca11e27581c")
        .arg("test-lfs-repo-assets")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @r#"
    success: false
    exit_code: [ERROR_CODE]
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + test-lfs-repo==0.1.0 (from git+https://github.com/astral-sh/test-lfs-repo@54e5eebd3c6851b1353fc7b1e5b4eca11e27581c)
    Traceback (most recent call last):
      File "[CACHE_DIR]/archive-v0/[HASH]/bin/test-lfs-repo-assets", line 12, in <module>
        sys.exit(main_lfs())
                 ~~~~~~~~^^
      File "[CACHE_DIR]/archive-v0/[HASH]/[PYTHON-LIB]/site-packages/test_lfs_repo/__init__.py", line 5, in main_lfs
        from .lfs_module import LFS_TEST
      File "[CACHE_DIR]/archive-v0/[HASH]/[PYTHON-LIB]/site-packages/test_lfs_repo/lfs_module.py", line 1
        version https://git-lfs.github.com/spec/v1
                ^^^^^
    SyntaxError: invalid syntax
    "#);

    #[cfg(windows)]
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--from")
        .arg("test-lfs-repo @ git+https://github.com/astral-sh/test-lfs-repo@54e5eebd3c6851b1353fc7b1e5b4eca11e27581c")
        .arg("test-lfs-repo-assets")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @r#"
    success: false
    exit_code: [ERROR_CODE]
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + test-lfs-repo==0.1.0 (from git+https://github.com/astral-sh/test-lfs-repo@54e5eebd3c6851b1353fc7b1e5b4eca11e27581c)
    Traceback (most recent call last):
      File "<frozen runpy>", line 198, in _run_module_as_main
      File "<frozen runpy>", line 88, in _run_code
      File "[CACHE_DIR]/archive-v0/[HASH]/Scripts/test-lfs-repo-assets/__main__.py", line 10, in <module>
        sys.exit(main_lfs())
                 ~~~~~~~~^^
      File "[CACHE_DIR]/archive-v0/[HASH]/[PYTHON-LIB]/site-packages/test_lfs_repo/__init__.py", line 5, in main_lfs
        from .lfs_module import LFS_TEST
      File "[CACHE_DIR]/archive-v0/[HASH]/[PYTHON-LIB]/site-packages/test_lfs_repo/lfs_module.py", line 1
        version https://git-lfs.github.com/spec/v1
                ^^^^^
    SyntaxError: invalid syntax
    "#);
}

/// Read requirements from a `requirements.txt` file.
#[test]
fn tool_run_requirements_txt() {
    let context = uv_test::test_context!("3.12").with_filtered_counts();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("iniconfig").unwrap();

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--with-requirements")
        .arg("requirements.txt")
        .arg("--with")
        .arg("typing-extensions")
        .arg("flask")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.[X]
    Flask 3.0.2
    Werkzeug 3.0.1

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + blinker==1.7.0
     + click==8.1.7
     + flask==3.0.2
     + iniconfig==2.0.0
     + itsdangerous==2.1.2
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + typing-extensions==4.10.0
     + werkzeug==3.0.1
    ");
}

/// Ignore and warn when (e.g.) the `--index-url` argument is a provided `requirements.txt`.
#[test]
fn tool_run_requirements_txt_arguments() {
    let context = uv_test::test_context!("3.12").with_filtered_counts();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str(indoc! { r"
        --index-url https://test.pypi.org/simple
        idna
        "
        })
        .unwrap();

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--with-requirements")
        .arg("requirements.txt")
        .arg("flask")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.[X]
    Flask 3.0.2
    Werkzeug 3.0.1

    ----- stderr -----
    warning: Ignoring `--index-url` from requirements file: `https://test.pypi.org/simple`. Instead, use the `--index-url` command-line argument, or set `index-url` in a `uv.toml` or `pyproject.toml` file.
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + blinker==1.7.0
     + click==8.1.7
     + flask==3.0.2
     + idna==3.6
     + itsdangerous==2.1.2
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + werkzeug==3.0.1
    ");
}

/// List installed tools when no command arg is given (e.g. `uv tool run`).
#[test]
fn tool_run_list_installed() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // No tools installed.
    uv_snapshot!(context.filters(), context.tool_run()
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: false
    exit_code: 2
    ----- stdout -----
    Provide a command to run with `uv tool run <command>`.

    See `uv tool run --help` for more information.

    ----- stderr -----
    ");

    // Install `black`.
    context
        .tool_install()
        .arg("black==24.2.0")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .assert()
        .success();

    // List installed tools.
    uv_snapshot!(context.filters(), context.tool_run()
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: false
    exit_code: 2
    ----- stdout -----
    Provide a command to run with `uv tool run <command>`.

    The following tools are installed:

    - black v24.2.0

    See `uv tool run --help` for more information.

    ----- stderr -----
    ");
}

/// By default, omit resolver and installer output.
#[test]
fn tool_run_without_output() {
    let context = uv_test::test_context!("3.12").with_filtered_counts();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // On the first run, only show the summary line.
    uv_snapshot!(context.filters(), context.tool_run()
        .env_remove(EnvVars::UV_SHOW_RESOLUTION)
        .arg("--")
        .arg("pytest")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    pytest 8.1.1

    ----- stderr -----
    Installed [N] packages in [TIME]
    ");

    // Subsequent runs are quiet.
    uv_snapshot!(context.filters(), context.tool_run()
        .env_remove(EnvVars::UV_SHOW_RESOLUTION)
        .arg("--")
        .arg("pytest")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    pytest 8.1.1

    ----- stderr -----
    ");
}

#[test]
#[cfg(not(windows))]
fn tool_run_csv_with_shorthand() -> anyhow::Result<()> {
    let context = uv_test::test_context!("3.12").with_filtered_counts();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    let anyio_local = context.temp_dir.child("src").child("anyio_local");
    copy_dir_all(
        context.workspace_root.join("test/packages/anyio_local"),
        &anyio_local,
    )?;

    let black_editable = context.temp_dir.child("src").child("black_editable");
    copy_dir_all(
        context.workspace_root.join("test/packages/black_editable"),
        &black_editable,
    )?;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.8"
        dependencies = ["anyio", "sniffio==1.3.1"]
        "#
    })?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r"
        import sniffio
       "
    })?;

    // Performs a tool run with a comma-separated `--with` flag.
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("-w")
        .arg("iniconfig,typing-extensions")
        .arg("pytest")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    pytest 8.1.1

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + iniconfig==2.0.0
     + packaging==24.0
     + pluggy==1.4.0
     + pytest==8.1.1
     + typing-extensions==4.10.0
    ");

    Ok(())
}

#[test]
#[cfg(not(windows))]
fn tool_run_csv_with() -> anyhow::Result<()> {
    let context = uv_test::test_context!("3.12").with_filtered_counts();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    let anyio_local = context.temp_dir.child("src").child("anyio_local");
    copy_dir_all(
        context.workspace_root.join("test/packages/anyio_local"),
        &anyio_local,
    )?;

    let black_editable = context.temp_dir.child("src").child("black_editable");
    copy_dir_all(
        context.workspace_root.join("test/packages/black_editable"),
        &black_editable,
    )?;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.8"
        dependencies = ["anyio", "sniffio==1.3.1"]
        "#
    })?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r"
        import sniffio
       "
    })?;

    // Performs a tool run with a comma-separated `--with` flag.
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--with")
        .arg("iniconfig,typing-extensions")
        .arg("pytest")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    pytest 8.1.1

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + iniconfig==2.0.0
     + packaging==24.0
     + pluggy==1.4.0
     + pytest==8.1.1
     + typing-extensions==4.10.0
    ");

    Ok(())
}

#[test]
#[cfg(windows)]
fn tool_run_csv_with() -> anyhow::Result<()> {
    let context = uv_test::test_context!("3.12").with_filtered_counts();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    let anyio_local = context.temp_dir.child("src").child("anyio_local");
    copy_dir_all(
        context.workspace_root.join("test/packages/anyio_local"),
        &anyio_local,
    )?;

    let black_editable = context.temp_dir.child("src").child("black_editable");
    copy_dir_all(
        context.workspace_root.join("test/packages/black_editable"),
        &black_editable,
    )?;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.8"
        dependencies = ["anyio", "sniffio==1.3.1"]
        "#
    })?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r"
        import sniffio
       "
    })?;

    // Performs a tool run with a comma-separated `--with` flag.
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--with")
        .arg("iniconfig,typing-extensions")
        .arg("pytest")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    pytest 8.1.1

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + iniconfig==2.0.0
     + packaging==24.0
     + pluggy==1.4.0
     + pytest==8.1.1
     + typing-extensions==4.10.0
    "###);

    Ok(())
}

#[test]
#[cfg(not(windows))]
fn tool_run_repeated_with() -> anyhow::Result<()> {
    let context = uv_test::test_context!("3.12").with_filtered_counts();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    let anyio_local = context.temp_dir.child("src").child("anyio_local");
    copy_dir_all(
        context.workspace_root.join("test/packages/anyio_local"),
        &anyio_local,
    )?;

    let black_editable = context.temp_dir.child("src").child("black_editable");
    copy_dir_all(
        context.workspace_root.join("test/packages/black_editable"),
        &black_editable,
    )?;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.8"
        dependencies = ["anyio", "sniffio==1.3.1"]
        "#
    })?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r"
        import sniffio
       "
    })?;

    // Performs a tool run with a repeated `--with` flag.
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--with")
        .arg("iniconfig")
        .arg("--with")
        .arg("typing-extensions")
        .arg("pytest")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    pytest 8.1.1

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + iniconfig==2.0.0
     + packaging==24.0
     + pluggy==1.4.0
     + pytest==8.1.1
     + typing-extensions==4.10.0
    ");

    Ok(())
}

#[test]
#[cfg(windows)]
fn tool_run_repeated_with() -> anyhow::Result<()> {
    let context = uv_test::test_context!("3.12").with_filtered_counts();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    let anyio_local = context.temp_dir.child("src").child("anyio_local");
    copy_dir_all(
        context.workspace_root.join("test/packages/anyio_local"),
        &anyio_local,
    )?;

    let black_editable = context.temp_dir.child("src").child("black_editable");
    copy_dir_all(
        context.workspace_root.join("test/packages/black_editable"),
        &black_editable,
    )?;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.8"
        dependencies = ["anyio", "sniffio==1.3.1"]
        "#
    })?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r"
        import sniffio
       "
    })?;

    // Performs a tool run with a repeated `--with` flag.
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--with")
        .arg("iniconfig")
        .arg("--with")
        .arg("typing-extensions")
        .arg("pytest")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    pytest 8.1.1

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + iniconfig==2.0.0
     + packaging==24.0
     + pluggy==1.4.0
     + pytest==8.1.1
     + typing-extensions==4.10.0
    "###);

    Ok(())
}

#[test]
fn tool_run_with_editable() -> anyhow::Result<()> {
    let context = uv_test::test_context!("3.12").with_filtered_counts();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    let anyio_local = context.temp_dir.child("src").child("anyio_local");
    copy_dir_all(
        context.workspace_root.join("test/packages/anyio_local"),
        &anyio_local,
    )?;

    let black_editable = context.temp_dir.child("src").child("black_editable");
    copy_dir_all(
        context.workspace_root.join("test/packages/black_editable"),
        &black_editable,
    )?;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.8"
        dependencies = ["anyio", "sniffio==1.3.1"]
        "#
    })?;

    let test_script = context.temp_dir.child("main.py");
    test_script.write_str(indoc! { r"
        import sniffio
       "
    })?;

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--with-editable")
        .arg("./src/black_editable")
        .arg("--with")
        .arg("iniconfig")
        .arg("flask")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.[X]
    Flask 3.0.2
    Werkzeug 3.0.1

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + black==0.1.0 (from file://[TEMP_DIR]/src/black_editable)
     + blinker==1.7.0
     + click==8.1.7
     + flask==3.0.2
     + iniconfig==2.0.0
     + itsdangerous==2.1.2
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + werkzeug==3.0.1
    ");

    // Requesting an editable requirement should install it in a layer, even if it satisfied
    uv_snapshot!(context.filters(), context.tool_run().arg("--with-editable").arg("./src/anyio_local").arg("flask").arg("--version").env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str()).env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()),
    @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.[X]
    Flask 3.0.2
    Werkzeug 3.0.1

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + anyio==4.3.0+foo (from file://[TEMP_DIR]/src/anyio_local)
     + blinker==1.7.0
     + click==8.1.7
     + flask==3.0.2
     + itsdangerous==2.1.2
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + werkzeug==3.0.1
    ");

    // Requesting the project itself should use a new environment.
    uv_snapshot!(context.filters(), context.tool_run().arg("--with-editable").arg(".").arg("flask").arg("--version").env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str()).env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.[X]
    Flask 3.0.2
    Werkzeug 3.0.1

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + anyio==4.3.0
     + blinker==1.7.0
     + click==8.1.7
     + flask==3.0.2
     + foo==1.0.0 (from file://[TEMP_DIR]/)
     + idna==3.6
     + itsdangerous==2.1.2
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + sniffio==1.3.1
     + werkzeug==3.0.1
    ");

    // If invalid, we should reference `--with`.
    uv_snapshot!(context.filters(), context
        .tool_run()
        .arg("--with")
        .arg("./foo")
        .arg("flask")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir
        .as_os_str()).env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to resolve `--with` requirement
      ╰─▶ Distribution not found at: file://[TEMP_DIR]/foo
    ");

    Ok(())
}

#[test]
fn warn_no_executables_found() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("requests")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Prepared 5 packages in [TIME]
    Installed 5 packages in [TIME]
     + certifi==2024.2.2
     + charset-normalizer==3.3.2
     + idna==3.6
     + requests==2.31.0
     + urllib3==2.2.1
    Package `requests` does not provide any executables.
    ");
}

/// Warn when a user passes `--upgrade` to `uv tool run`.
#[test]
fn tool_run_upgrade_warn() {
    let context = uv_test::test_context!("3.12").with_filtered_counts();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--upgrade")
        .arg("pytest")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    pytest 8.1.1

    ----- stderr -----
    warning: Tools cannot be upgraded via `uv tool run`; use `uv tool upgrade --all` to upgrade all installed tools, or `uv tool run package@latest` to run the latest version of a tool.
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + iniconfig==2.0.0
     + packaging==24.0
     + pluggy==1.4.0
     + pytest==8.1.1
    ");

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--upgrade")
        .arg("--with")
        .arg("typing-extensions")
        .arg("pytest")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    pytest 8.1.1

    ----- stderr -----
    warning: Tools cannot be upgraded via `uv tool run`; use `uv tool upgrade --all` to upgrade all installed tools, `uv tool run package@latest` to run the latest version of a tool, or `uv tool run --refresh package` to upgrade any `--with` dependencies.
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + iniconfig==2.0.0
     + packaging==24.0
     + pluggy==1.4.0
     + pytest==8.1.1
     + typing-extensions==4.10.0
    ");
}

/// If we fail to resolve the tool, we should include "tool" in the error message.
#[test]
fn tool_run_resolution_error() {
    let context = uv_test::test_context!("3.12").with_filtered_counts();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("add")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving tool dependencies:
      ╰─▶ Because there are no versions of add and you require add, we can conclude that your requirements are unsatisfiable.
    ");
}

#[test]
fn tool_run_latest() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `pytest` at a specific version.
    context
        .tool_install()
        .arg("pytest==7.0.0")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .assert()
        .success();

    // Run `pytest`, which should use the installed version.
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("pytest")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    pytest 7.0.0

    ----- stderr -----
    ");

    // Run `pytest@latest`, which should use the latest version.
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("pytest@latest")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    pytest 8.1.1

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + iniconfig==2.0.0
     + packaging==24.0
     + pluggy==1.4.0
     + pytest==8.1.1
    ");

    // Run `pytest`, which should use the installed version.
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("pytest")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    pytest 7.0.0

    ----- stderr -----
    ");
}

#[test]
fn tool_run_latest_extra() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("flask[dotenv]@latest")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.[X]
    Flask 3.0.2
    Werkzeug 3.0.1

    ----- stderr -----
    Resolved 8 packages in [TIME]
    Prepared 8 packages in [TIME]
    Installed 8 packages in [TIME]
     + blinker==1.7.0
     + click==8.1.7
     + flask==3.0.2
     + itsdangerous==2.1.2
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + python-dotenv==1.0.1
     + werkzeug==3.0.1
    ");

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("flask[dotenv]@3.0.0")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.[X]
    Flask 3.0.0
    Werkzeug 3.0.1

    ----- stderr -----
    Resolved 8 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 8 packages in [TIME]
     + blinker==1.7.0
     + click==8.1.7
     + flask==3.0.0
     + itsdangerous==2.1.2
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + python-dotenv==1.0.1
     + werkzeug==3.0.1
    ");
}

#[test]
fn tool_run_extra() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("flask[dotenv]")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.[X]
    Flask 3.0.2
    Werkzeug 3.0.1

    ----- stderr -----
    Resolved 8 packages in [TIME]
    Prepared 8 packages in [TIME]
    Installed 8 packages in [TIME]
     + blinker==1.7.0
     + click==8.1.7
     + flask==3.0.2
     + itsdangerous==2.1.2
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + python-dotenv==1.0.1
     + werkzeug==3.0.1
    ");
}

#[test]
fn tool_run_specifier() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("flask<3.0.0")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.[X]
    Flask 2.3.3
    Werkzeug 3.0.1

    ----- stderr -----
    Resolved 7 packages in [TIME]
    Prepared 7 packages in [TIME]
    Installed 7 packages in [TIME]
     + blinker==1.7.0
     + click==8.1.7
     + flask==2.3.3
     + itsdangerous==2.1.2
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + werkzeug==3.0.1
    ");
}

#[test]
fn tool_run_python() {
    let context = uv_test::test_context!("3.12").with_filtered_counts();
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("python")
        .arg("--version"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.[X]

    ----- stderr -----
    Resolved in [TIME]
    Audited in [TIME]
    ");

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("python")
        .arg("-c")
        .arg("print('Hello, world!')"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello, world!

    ----- stderr -----
    Resolved in [TIME]
    ");
}

#[test]
fn tool_run_python_at_version() {
    let context = uv_test::test_context_with_versions!(&["3.12", "3.11"])
        .with_filtered_counts()
        .with_filtered_python_sources();

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("python")
        .arg("--version"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.[X]

    ----- stderr -----
    Resolved in [TIME]
    Audited in [TIME]
    ");

    uv_snapshot!(context.filters(), context.tool_run()
            .arg("python@3.12")
            .arg("--version"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.[X]

    ----- stderr -----
    Resolved in [TIME]
    ");

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("python@3.11")
        .arg("--version"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.11.[X]

    ----- stderr -----
    Resolved in [TIME]
    Audited in [TIME]
    ");

    // The @ is optional.
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("python3.11")
        .arg("--version"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.11.[X]

    ----- stderr -----
    Resolved in [TIME]
    ");

    // Dotless syntax also works.
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("python311")
        .arg("--version"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.11.[X]

    ----- stderr -----
    Resolved in [TIME]
    ");

    // Other implementations like PyPy also work. PyPy isn't currently in the test suite, so
    // specify CPython and rely on the fact that they go through the same codepath.
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("cpython311")
        .arg("--version"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.11.[X]

    ----- stderr -----
    Resolved in [TIME]
    ");

    // But short names don't work in the executable position (as opposed to with -p/--python). We
    // interpret those as package names.
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("cp311")
        .arg("--version"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving tool dependencies:
      ╰─▶ Because cp311 was not found in the package registry and you require cp311, we can conclude that your requirements are unsatisfiable.
    ");

    // Bare versions don't work either. Again we interpret them as package names.
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("311")
        .arg("--version"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving tool dependencies:
      ╰─▶ Because 311 was not found in the package registry and you require 311, we can conclude that your requirements are unsatisfiable.
    ");

    // Request a version via `-p`
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("-p")
        .arg("3.11")
        .arg("python")
        .arg("--version"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.11.[X]

    ----- stderr -----
    Resolved in [TIME]
    ");

    // @ syntax is also allowed here.
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("-p")
        .arg("python@311")
        .arg("python")
        .arg("--version"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.11.[X]

    ----- stderr -----
    Resolved in [TIME]
    ");

    // But @ with nothing in front of it is not.
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("-p")
        .arg("@311")
        .arg("python")
        .arg("--version"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No interpreter found for executable name `@311` in [PYTHON SOURCES]
    ");

    // Request a version in the tool and `-p`
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("-p")
        .arg("3.12")
        .arg("python@3.11")
        .arg("--version"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Received multiple Python version requests: `3.12` and `3.11`
    ");

    // Request a version that does not exist
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("python@3.12.99"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No interpreter found for Python 3.12.[X] in [PYTHON SOURCES]
    ");

    // Request an invalid version
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("python@3.300"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Invalid version request: 3.300
    ");

    // Request `@latest` (not yet supported)
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("python@latest")
        .arg("--version"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Requesting the 'latest' Python version is not yet supported
    ");
}

#[test]
fn tool_run_hint_version_not_available() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_counts()
        .with_filtered_python_sources();

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("python@3.12")
        .env(EnvVars::UV_PYTHON_DOWNLOADS, "never"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No interpreter found for Python 3.12 in [PYTHON SOURCES]

    hint: A managed Python download is available for Python 3.12, but Python downloads are set to 'never'
    ");

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("python@3.12")
        .env(EnvVars::UV_PYTHON_DOWNLOADS, "auto")
        .env(EnvVars::UV_OFFLINE, "true"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No interpreter found for Python 3.12 in [PYTHON SOURCES]

    hint: A managed Python download is available for Python 3.12, but uv is set to offline mode
    ");

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("python@3.12")
        .env(EnvVars::UV_PYTHON_DOWNLOADS, "auto")
        .env(EnvVars::UV_NO_MANAGED_PYTHON, "true"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No interpreter found for Python 3.12 in [PYTHON SOURCES]

    hint: A managed Python download is available for Python 3.12, but the Python preference is set to 'only system'
    ");
}

#[test]
fn tool_run_python_from_global_version_file() {
    let context = uv_test::test_context_with_versions!(&["3.12", "3.11"])
        .with_filtered_counts()
        .with_filtered_python_sources();

    context
        .python_pin()
        .arg("3.11")
        .arg("--global")
        .assert()
        .success();

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("python")
        .arg("--version"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.11.[X]

    ----- stderr -----
    Resolved in [TIME]
    Audited in [TIME]
    "###);
}

#[test]
fn tool_run_python_version_overrides_global_pin() {
    let context = uv_test::test_context_with_versions!(&["3.12", "3.11"])
        .with_filtered_counts()
        .with_filtered_python_sources();

    // Set global pin to 3.11
    context
        .python_pin()
        .arg("3.11")
        .arg("--global")
        .assert()
        .success();

    // Explicitly request python3.12, should override global pin
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("python3.12")
        .arg("--version"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.[X]

    ----- stderr -----
    Resolved in [TIME]
    Audited in [TIME]
    "###);
}

#[test]
fn tool_run_python_with_explicit_default_bypasses_global_pin() {
    let context = uv_test::test_context_with_versions!(&["3.12", "3.11"])
        .with_filtered_counts()
        .with_filtered_python_sources();

    // Set global pin to 3.11
    context
        .python_pin()
        .arg("3.11")
        .arg("--global")
        .assert()
        .success();

    // Explicitly request --python default, should bypass global pin and use system default (3.12)
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--python")
        .arg("default")
        .arg("python")
        .arg("--version"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.[X]

    ----- stderr -----
    Resolved in [TIME]
    Audited in [TIME]
    "###);
}

#[test]
fn tool_run_python_from() {
    let context = uv_test::test_context_with_versions!(&["3.12", "3.11"])
        .with_filtered_counts()
        .with_filtered_python_sources();

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--from")
        .arg("python")
        .arg("python")
        .arg("--version"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.[X]

    ----- stderr -----
    Resolved in [TIME]
    Audited in [TIME]
    ");

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--from")
        .arg("python@3.11")
        .arg("python")
        .arg("--version"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.11.[X]

    ----- stderr -----
    Resolved in [TIME]
    Audited in [TIME]
    ");

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--from")
        .arg("python311")
        .arg("python")
        .arg("--version"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.11.[X]

    ----- stderr -----
    Resolved in [TIME]
    ");

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--from")
        .arg("python>3.11,<3.13")
        .arg("python")
        .arg("--version"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.[X]

    ----- stderr -----
    Resolved in [TIME]
    ");

    // The executed command isn't necessarily Python, but Python is in the PATH.
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--from")
        .arg("python@3.11")
        .arg("bash")
        .arg("-c")
        .arg("python --version"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.11.[X]

    ----- stderr -----
    Resolved in [TIME]
    ");
}

#[test]
fn run_with_env_file() -> anyhow::Result<()> {
    let context = uv_test::test_context!("3.12").with_filtered_counts();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Create a project with a custom script.
    let foo_dir = context.temp_dir.child("foo");
    let foo_pyproject_toml = foo_dir.child("pyproject.toml");

    foo_pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.8"
        dependencies = []

        [project.scripts]
        script = "foo.main:run"

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"
        "#
    })?;

    // Create the `foo` module.
    let foo_project_src = foo_dir.child("src");
    let foo_module = foo_project_src.child("foo");
    foo_module.child("__init__.py").touch()?;
    let foo_main_py = foo_module.child("main.py");
    foo_main_py.write_str(indoc! { r#"
        def run():
            import os

            print(os.environ.get('THE_EMPIRE_VARIABLE'))
            print(os.environ.get('REBEL_1'))
            print(os.environ.get('REBEL_2'))
            print(os.environ.get('REBEL_3'))

        __name__ == "__main__" and run()
       "#
    })?;

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--from")
        .arg("./foo")
        .arg("script")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    None
    None
    None
    None

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/foo)
    ");

    context.temp_dir.child(".file").write_str(indoc! { "
        THE_EMPIRE_VARIABLE=palpatine
        REBEL_1=leia_organa
        REBEL_2=obi_wan_kenobi
        REBEL_3=C3PO
       "
    })?;

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--env-file").arg(".file")
        .arg("--from")
        .arg("./foo")
        .arg("script")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    palpatine
    leia_organa
    obi_wan_kenobi
    C3PO

    ----- stderr -----
    Resolved [N] packages in [TIME]
    ");

    Ok(())
}

#[test]
fn tool_run_from_at() {
    let context = uv_test::test_context!("3.12")
        .with_exclude_newer("2025-01-18T00:00:00Z")
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--from")
        .arg("executable-application@latest")
        .arg("app")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    app 0.3.0

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + executable-application==0.3.0
    ");

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--from")
        .arg("executable-application@0.2.0")
        .arg("app")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    app 0.2.0

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + executable-application==0.2.0
    ");
}

#[test]
fn tool_run_verbatim_name() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // The normalized package name is `change-wheel-version`, but the executable is `change_wheel_version`.
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("change_wheel_version")
        .arg("--help")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    usage: change_wheel_version [-h] [--local-version LOCAL_VERSION] [--version VERSION]
                                [--delete-old-wheel] [--allow-same-version]
                                wheel

    positional arguments:
      wheel

    options:
      -h, --help            show this help message and exit
      --local-version LOCAL_VERSION
      --version VERSION
      --delete-old-wheel
      --allow-same-version

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + change-wheel-version==0.5.0
     + installer==0.7.0
     + packaging==24.0
     + wheel==0.43.0
    ");

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("change-wheel-version")
        .arg("--help")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    An executable named `change-wheel-version` is not provided by package `change-wheel-version`.
    The following executables are available:
    - change_wheel_version

    Use `uv tool run --from change-wheel-version change_wheel_version` instead.
    ");

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--from")
        .arg("change-wheel-version")
        .arg("change_wheel_version")
        .arg("--help")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    usage: change_wheel_version [-h] [--local-version LOCAL_VERSION] [--version VERSION]
                                [--delete-old-wheel] [--allow-same-version]
                                wheel

    positional arguments:
      wheel

    options:
      -h, --help            show this help message and exit
      --local-version LOCAL_VERSION
      --version VERSION
      --delete-old-wheel
      --allow-same-version

    ----- stderr -----
    Resolved [N] packages in [TIME]
    ");
}

#[test]
fn tool_run_with_existing_py_script() -> anyhow::Result<()> {
    let context = uv_test::test_context!("3.12").with_filtered_counts();
    context.temp_dir.child("script.py").touch()?;

    uv_snapshot!(context.filters(), context.tool_run().arg("script.py"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: It looks like you tried to run a Python script at `script.py`, which is not supported by `uv tool run`

    hint: Use `uv run script.py` instead
    ");
    Ok(())
}

#[test]
fn tool_run_with_existing_pyw_script() -> anyhow::Result<()> {
    let context = uv_test::test_context!("3.12").with_filtered_counts();
    context.temp_dir.child("script.pyw").touch()?;

    // We treat arguments before the command as uv arguments
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("script.pyw"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: It looks like you tried to run a Python script at `script.pyw`, which is not supported by `uv tool run`

    hint: Use `uv run script.pyw` instead
    ");
    Ok(())
}

#[test]
fn tool_run_with_nonexistent_py_script() {
    let context = uv_test::test_context!("3.12").with_filtered_counts();

    // We treat arguments before the command as uv arguments
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("script.py"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: It looks like you provided a Python script to run, which is not supported supported by `uv tool run`

    hint: We did not find a script at the requested path. If you meant to run a command from the `script-py` package, pass the normalized package name to `--from` to disambiguate, e.g., `uv tool run --from script-py script.py`
    ");
}

#[test]
fn tool_run_with_nonexistent_pyw_script() {
    let context = uv_test::test_context!("3.12").with_filtered_counts();

    // We treat arguments before the command as uv arguments
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("script.pyw"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: It looks like you provided a Python script to run, which is not supported supported by `uv tool run`

    hint: We did not find a script at the requested path. If you meant to run a command from the `script-pyw` package, pass the normalized package name to `--from` to disambiguate, e.g., `uv tool run --from script-pyw script.pyw`
    ");
}

#[test]
fn tool_run_with_from_script() {
    let context = uv_test::test_context!("3.12").with_filtered_counts();

    // We treat arguments before the command as uv arguments
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--from")
        .arg("script.py")
        .arg("ruff"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: It looks like you provided a Python script to `--from`, which is not supported

    hint: If you meant to run a command from the `script-py` package, use the normalized package name instead to disambiguate, e.g., `uv tool run --from script-py ruff`
    ");
}

#[test]
fn tool_run_with_script_and_from_script() {
    let context = uv_test::test_context!("3.12").with_filtered_counts();

    // We treat arguments before the command as uv arguments
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--from")
        .arg("script.py")
        .arg("other-script.py"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: It looks like you provided a Python script to `--from`, which is not supported

    hint: If you meant to run a command from the `script-py` package, use the normalized package name instead to disambiguate, e.g., `uv tool run --from script-py other-script.py`
    ");
}

/// Test that when a user provides `--verbose` to the subcommand,
/// we show a helpful hint.
#[test]
fn tool_run_verbose_hint() {
    let context = uv_test::test_context!("3.12").with_filtered_counts();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Test with --verbose flag
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("nonexistent-package-foo")
        .arg("--verbose")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because nonexistent-package-foo was not found in the package registry and you require nonexistent-package-foo, we can conclude that your requirements are unsatisfiable.
      help: You provided `--verbose` to `nonexistent-package-foo`. Did you mean to provide it to `uv tool run`? e.g., `uv tool run --verbose nonexistent-package-foo`
    ");

    // Test with -v flag
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("nonexistent-package-bar")
        .arg("-v")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because nonexistent-package-bar was not found in the package registry and you require nonexistent-package-bar, we can conclude that your requirements are unsatisfiable.
      help: You provided `-v` to `nonexistent-package-bar`. Did you mean to provide it to `uv tool run`? e.g., `uv tool run -v nonexistent-package-bar`
    ");

    // Test with -vv flag
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("nonexistent-package-baz")
        .arg("-vv")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because nonexistent-package-baz was not found in the package registry and you require nonexistent-package-baz, we can conclude that your requirements are unsatisfiable.
      help: You provided `-vv` to `nonexistent-package-baz`. Did you mean to provide it to `uv tool run`? e.g., `uv tool run -vv nonexistent-package-baz`
    ");

    // Test for false positives
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("nonexistent-package-quux")
        .arg("-version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving tool dependencies:
      ╰─▶ Because nonexistent-package-quux was not found in the package registry and you require nonexistent-package-quux, we can conclude that your requirements are unsatisfiable.
    ");
}

#[test]
fn tool_run_with_compatible_build_constraints() -> Result<()> {
    let context = uv_test::test_context!("3.9")
        .with_exclude_newer("2024-05-04T00:00:00Z")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let constraints_txt = context.temp_dir.child("build_constraints.txt");
    constraints_txt.write_str("setuptools>=40")?;

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--with")
        .arg("requests==1.2")
        .arg("--build-constraints")
        .arg("build_constraints.txt")
        .arg("pytest")
        .arg("--version"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    pytest 8.2.0

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + exceptiongroup==1.2.1
     + iniconfig==2.0.0
     + packaging==24.0
     + pluggy==1.5.0
     + pytest==8.2.0
     + requests==1.2.0
     + tomli==2.0.1
    ");

    Ok(())
}

#[test]
fn tool_run_with_incompatible_build_constraints() -> Result<()> {
    let context = uv_test::test_context!("3.9")
        .with_exclude_newer("2024-05-04T00:00:00Z")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    let constraints_txt = context.temp_dir.child("build_constraints.txt");
    constraints_txt.write_str("setuptools==2")?;

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--with")
        .arg("requests==1.2")
        .arg("--build-constraints")
        .arg("build_constraints.txt")
        .arg("pytest")
        .arg("--version")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to download and build `requests==1.2.0`
      ├─▶ Failed to resolve requirements from `setup.py` build
      ├─▶ No solution found when resolving: `setuptools>=40.8.0`
      ╰─▶ Because you require setuptools>=40.8.0 and setuptools==2, we can conclude that your requirements are unsatisfiable.
    ");

    Ok(())
}

#[test]
fn tool_run_with_dependencies_from_script() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_missing_file_error();

    let script_contents = indoc! {r#"
        # /// script
        # requires-python = ">=3.11"
        # dependencies = [
        #   "anyio",
        # ]
        # ///

        import anyio
    "#};

    let script = context.temp_dir.child("script.py");
    script.write_str(script_contents)?;

    let script_without_extension = context.temp_dir.child("script-no-ext");
    script_without_extension.write_str(script_contents)?;

    // script dependencies (anyio) are now installed.
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--with-requirements")
        .arg("script.py")
        .arg("black")
        .arg("script.py")
        .arg("-q"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + anyio==4.3.0
     + black==24.3.0
     + click==8.1.7
     + idna==3.6
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
     + sniffio==1.3.1
    ");

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--with-requirements")
        .arg("script-no-ext")
        .arg("black")
        .arg("script-no-ext")
        .arg("-q"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    ");

    // Error when the script is not a valid PEP723 script.
    let script = context.temp_dir.child("not_pep723_script.py");
    script.write_str("import anyio")?;

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--with-requirements")
        .arg("not_pep723_script.py")
        .arg("black"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: `not_pep723_script.py` does not contain inline script metadata
    ");

    // Error when the script doesn't exist.
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--with-requirements")
        .arg("missing_file.py")
        .arg("black"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: failed to read from file `missing_file.py`: [OS ERROR 2]
    ");

    Ok(())
}

/// Test windows runnable types, namely console scripts and legacy setuptools scripts.
/// Console Scripts <https://packaging.python.org/en/latest/guides/writing-pyproject-toml/#console-scripts>
/// Legacy Scripts <https://packaging.python.org/en/latest/guides/distributing-packages-using-setuptools/#scripts>.
///
/// This tests for uv tool run of windows runnable types defined by [`WindowsRunnable`].
#[cfg(windows)]
#[test]
fn tool_run_windows_runnable_types() -> anyhow::Result<()> {
    let context = uv_test::test_context!("3.12").with_filtered_counts();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    let foo_dir = context.temp_dir.child("foo");
    let foo_pyproject_toml = foo_dir.child("pyproject.toml");

    // Use `script-files` which enables legacy scripts packaging.
    foo_pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.8"
        dependencies = []

        [project.scripts]
        custom_pydoc = "foo.main:run"

        [tool.setuptools]
        script-files = [
            "misc/custom_pydoc.bat",
            "misc/custom_pydoc.cmd",
            "misc/custom_pydoc.ps1"
        ]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#
    })?;

    // Create the legacy scripts
    let custom_pydoc_bat = foo_dir.child("misc").child("custom_pydoc.bat");
    let custom_pydoc_cmd = foo_dir.child("misc").child("custom_pydoc.cmd");
    let custom_pydoc_ps1 = foo_dir.child("misc").child("custom_pydoc.ps1");

    custom_pydoc_bat.write_str("python.exe -m pydoc %*")?;
    custom_pydoc_cmd.write_str("python.exe -m pydoc %*")?;
    custom_pydoc_ps1.write_str("python.exe -m pydoc $args")?;

    // Create the foo module
    let foo_project_src = foo_dir.child("src");
    let foo_module = foo_project_src.child("foo");
    let foo_main_py = foo_module.child("main.py");
    foo_main_py.write_str(indoc! { r#"
        import pydoc, sys

        def run():
            sys.argv[0] = "pydoc"
            pydoc.cli()

        __name__ == "__main__" and run()
       "#
    })?;

    // Install `foo` tool.
    context
        .tool_install()
        .arg(foo_dir.as_os_str())
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str())
        .assert()
        .success();

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--from")
        .arg("foo")
        .arg("does_not_exist")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    An executable named `does_not_exist` is not provided by package `foo`.
    The following executables are available:
    - custom_pydoc.bat
    - custom_pydoc.cmd
    - custom_pydoc.exe
    - custom_pydoc.ps1
    "###);

    // Test with explicit .bat extension
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--from")
        .arg("foo")
        .arg("custom_pydoc.bat")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    pydoc - the Python documentation tool

    pydoc <name> ...
        Show text documentation on something.  <name> may be the name of a
        Python keyword, topic, function, module, or package, or a dotted
        reference to a class or function within a module or module in a
        package.  If <name> contains a '\', it is used as the path to a
        Python source file to document. If name is 'keywords', 'topics',
        or 'modules', a listing of these things is displayed.

    pydoc -k <keyword>
        Search for a keyword in the synopsis lines of all available modules.

    pydoc -n <hostname>
        Start an HTTP server with the given hostname (default: localhost).

    pydoc -p <port>
        Start an HTTP server on the given port on the local machine.  Port
        number 0 can be used to get an arbitrary unused port.

    pydoc -b
        Start an HTTP server on an arbitrary unused port and open a web browser
        to interactively browse documentation.  This option can be used in
        combination with -n and/or -p.

    pydoc -w <name> ...
        Write out the HTML documentation for a module to a file in the current
        directory.  If <name> contains a '\', it is treated as a filename; if
        it names a directory, documentation is written for all the contents.


    ----- stderr -----
    "###);

    // Test with explicit .cmd extension
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--from")
        .arg("foo")
        .arg("custom_pydoc.cmd")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    pydoc - the Python documentation tool

    pydoc <name> ...
        Show text documentation on something.  <name> may be the name of a
        Python keyword, topic, function, module, or package, or a dotted
        reference to a class or function within a module or module in a
        package.  If <name> contains a '\', it is used as the path to a
        Python source file to document. If name is 'keywords', 'topics',
        or 'modules', a listing of these things is displayed.

    pydoc -k <keyword>
        Search for a keyword in the synopsis lines of all available modules.

    pydoc -n <hostname>
        Start an HTTP server with the given hostname (default: localhost).

    pydoc -p <port>
        Start an HTTP server on the given port on the local machine.  Port
        number 0 can be used to get an arbitrary unused port.

    pydoc -b
        Start an HTTP server on an arbitrary unused port and open a web browser
        to interactively browse documentation.  This option can be used in
        combination with -n and/or -p.

    pydoc -w <name> ...
        Write out the HTML documentation for a module to a file in the current
        directory.  If <name> contains a '\', it is treated as a filename; if
        it names a directory, documentation is written for all the contents.


    ----- stderr -----
    "###);

    // Test with explicit .ps1 extension
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--from")
        .arg("foo")
        .arg("custom_pydoc.ps1")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    pydoc - the Python documentation tool

    pydoc <name> ...
        Show text documentation on something.  <name> may be the name of a
        Python keyword, topic, function, module, or package, or a dotted
        reference to a class or function within a module or module in a
        package.  If <name> contains a '\', it is used as the path to a
        Python source file to document. If name is 'keywords', 'topics',
        or 'modules', a listing of these things is displayed.

    pydoc -k <keyword>
        Search for a keyword in the synopsis lines of all available modules.

    pydoc -n <hostname>
        Start an HTTP server with the given hostname (default: localhost).

    pydoc -p <port>
        Start an HTTP server on the given port on the local machine.  Port
        number 0 can be used to get an arbitrary unused port.

    pydoc -b
        Start an HTTP server on an arbitrary unused port and open a web browser
        to interactively browse documentation.  This option can be used in
        combination with -n and/or -p.

    pydoc -w <name> ...
        Write out the HTML documentation for a module to a file in the current
        directory.  If <name> contains a '\', it is treated as a filename; if
        it names a directory, documentation is written for all the contents.


    ----- stderr -----
    "###);

    // Test with explicit .exe extension
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--from")
        .arg("foo")
        .arg("custom_pydoc")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    pydoc - the Python documentation tool

    pydoc <name> ...
        Show text documentation on something.  <name> may be the name of a
        Python keyword, topic, function, module, or package, or a dotted
        reference to a class or function within a module or module in a
        package.  If <name> contains a '\', it is used as the path to a
        Python source file to document. If name is 'keywords', 'topics',
        or 'modules', a listing of these things is displayed.

    pydoc -k <keyword>
        Search for a keyword in the synopsis lines of all available modules.

    pydoc -n <hostname>
        Start an HTTP server with the given hostname (default: localhost).

    pydoc -p <port>
        Start an HTTP server on the given port on the local machine.  Port
        number 0 can be used to get an arbitrary unused port.

    pydoc -b
        Start an HTTP server on an arbitrary unused port and open a web browser
        to interactively browse documentation.  This option can be used in
        combination with -n and/or -p.

    pydoc -w <name> ...
        Write out the HTML documentation for a module to a file in the current
        directory.  If <name> contains a '\', it is treated as a filename; if
        it names a directory, documentation is written for all the contents.


    ----- stderr -----
    "###);

    // Test without explicit extension (.exe should be used)
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--from")
        .arg("foo")
        .arg("custom_pydoc")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    pydoc - the Python documentation tool

    pydoc <name> ...
        Show text documentation on something.  <name> may be the name of a
        Python keyword, topic, function, module, or package, or a dotted
        reference to a class or function within a module or module in a
        package.  If <name> contains a '\', it is used as the path to a
        Python source file to document. If name is 'keywords', 'topics',
        or 'modules', a listing of these things is displayed.

    pydoc -k <keyword>
        Search for a keyword in the synopsis lines of all available modules.

    pydoc -n <hostname>
        Start an HTTP server with the given hostname (default: localhost).

    pydoc -p <port>
        Start an HTTP server on the given port on the local machine.  Port
        number 0 can be used to get an arbitrary unused port.

    pydoc -b
        Start an HTTP server on an arbitrary unused port and open a web browser
        to interactively browse documentation.  This option can be used in
        combination with -n and/or -p.

    pydoc -w <name> ...
        Write out the HTML documentation for a module to a file in the current
        directory.  If <name> contains a '\', it is treated as a filename; if
        it names a directory, documentation is written for all the contents.


    ----- stderr -----
    "###);

    Ok(())
}

#[test]
fn tool_run_reresolve_python() -> anyhow::Result<()> {
    let context = uv_test::test_context_with_versions!(&["3.11", "3.12"]).with_filtered_counts();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");
    let foo_dir = context.temp_dir.child("foo");
    let foo_pyproject_toml = foo_dir.child("pyproject.toml");

    foo_pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = []

        [project.scripts]
        foo = "foo:run"
        "#
    })?;
    let foo_project_src = foo_dir.child("src");
    let foo_module = foo_project_src.child("foo");
    let foo_init = foo_module.child("__init__.py");
    foo_init.write_str(indoc! { r#"
        import sys

        def run():
            print(".".join(str(key) for key in sys.version_info[:2]))
       "#
    })?;

    // Although 3.11 is first on the path, we'll re-resolve with 3.12 because the `requires-python`
    // is not compatible with 3.11.
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--from")
        .arg("./foo")
        .arg("foo")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    3.12

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + foo==1.0.0 (from file://[TEMP_DIR]/foo)
    ");

    // When an incompatible Python version is explicitly requested, we should not re-resolve
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--from")
        .arg("./foo")
        .arg("--python")
        .arg("3.11")
        .arg("foo")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving tool dependencies:
      ╰─▶ Because the current Python version (3.11.[X]) does not satisfy Python>=3.12 and foo==1.0.0 depends on Python>=3.12, we can conclude that foo==1.0.0 cannot be used.
          And because only foo==1.0.0 is available and you require foo, we can conclude that your requirements are unsatisfiable.
    ");

    // Unless the discovered interpreter is compatible with the request
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--from")
        .arg("./foo")
        .arg("--python")
        .arg(">=3.11")
        .arg("foo")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    3.12

    ----- stderr -----
    Resolved [N] packages in [TIME]
    ");

    Ok(())
}

/// Test that Windows executable resolution works correctly for package names with dots.
/// This test verifies the fix for the bug where package names containing dots were
/// incorrectly handled when adding Windows executable extensions.
#[cfg(windows)]
#[test]
fn tool_run_windows_dotted_package_name() -> anyhow::Result<()> {
    let context = uv_test::test_context!("3.12").with_filtered_counts();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Copy the test package to a temporary location
    let workspace_packages = context.workspace_root.join("test").join("packages");
    let test_package_source = workspace_packages.join("package.name.with.dots");
    let test_package_dest = context.temp_dir.child("package.name.with.dots");

    copy_dir_all(&test_package_source, &test_package_dest)?;

    // Test that uv tool run can find and execute the dotted package name
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--from")
        .arg(test_package_dest.path())
        .arg("package.name.with.dots")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    package.name.with.dots version 0.1.0

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + package-name-with-dots==0.1.0 (from file://[TEMP_DIR]/package.name.with.dots)
    "###);

    Ok(())
}

/// Regression test for <https://github.com/astral-sh/uv/issues/17436>
#[tokio::test]
async fn tool_run_latest_keyring_auth() {
    let keyring_context = uv_test::test_context!("3.12");

    // Install our keyring plugin
    keyring_context
        .pip_install()
        .arg(
            keyring_context
                .workspace_root
                .join("test")
                .join("packages")
                .join("keyring_test_plugin"),
        )
        .assert()
        .success();

    let proxy = crate::pypi_proxy::start().await;

    let context = uv_test::test_context!("3.12")
        .with_exclude_newer("2025-01-18T00:00:00Z")
        .with_filtered_counts();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Combine keyring venv bin with tool bin directory to avoid PATH warnings.
    let path = std::env::join_paths([venv_bin_path(&keyring_context.venv), bin_dir.to_path_buf()])
        .unwrap();

    // Test that the keyring is consulted during the @latest version lookup.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("--index")
        .arg(proxy.username_url("public", "/basic-auth/simple"))
        .arg("--keyring-provider")
        .arg("subprocess")
        .arg("executable-application@latest")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::KEYRING_TEST_CREDENTIALS, format!(r#"{{"{host}": {{"public": "heron"}}}}"#, host = proxy.host_port()))
        .env(EnvVars::PATH, path), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Keyring request for public@http://[LOCALHOST]/basic-auth/simple
    Keyring request for public@[LOCALHOST]
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + executable-application==0.3.0
    Installed 1 executable: app
    ");
}
