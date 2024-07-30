#![cfg(all(feature = "python", feature = "pypi"))]

use assert_cmd::prelude::*;
use assert_fs::prelude::*;
use common::{uv_snapshot, TestContext};
use indoc::indoc;

mod common;

#[test]
fn tool_run_args() {
    let context = TestContext::new("3.12").with_filtered_counts();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // We treat arguments before the command as uv arguments
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--version")
        .arg("pytest")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    uv [VERSION] ([COMMIT] DATE)

    ----- stderr -----
    "###);

    // We don't treat arguments after the command as uv arguments
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("pytest")
        .arg("--version")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    pytest 8.1.1

    ----- stderr -----
    warning: `uv tool run` is experimental and may change without warning
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + iniconfig==2.0.0
     + packaging==24.0
     + pluggy==1.4.0
     + pytest==8.1.1
    "###);

    // Can use `--` to separate uv arguments from the command arguments.
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--")
        .arg("pytest")
        .arg("--version")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    pytest 8.1.1

    ----- stderr -----
    warning: `uv tool run` is experimental and may change without warning
    Resolved [N] packages in [TIME]
    "###);
}

#[test]
fn tool_run_at_version() {
    let context = TestContext::new("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("pytest@8.0.0")
        .arg("--version")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    pytest 8.0.0

    ----- stderr -----
    warning: `uv tool run` is experimental and may change without warning
    Resolved 4 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + iniconfig==2.0.0
     + packaging==24.0
     + pluggy==1.4.0
     + pytest==8.0.0
    "###);

    // Empty versions are just treated as package and command names
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("pytest@")
        .arg("--version")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool run` is experimental and may change without warning
    error: Failed to parse: `pytest@`
      Caused by: Expected URL
    pytest@
           ^
    "###);

    // Invalid versions are just treated as package and command names
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("pytest@invalid")
        .arg("--version")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool run` is experimental and may change without warning
    error: Distribution not found at: file://[TEMP_DIR]/invalid
    "###);

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
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    The executable `pytest@8.0.0` was not found.
    The following executables are provided by `pytest`:
    - py.test
    - pytest

    ----- stderr -----
    warning: `uv tool run` is experimental and may change without warning
    Resolved 4 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 4 packages in [TIME]
     + iniconfig==2.0.0
     + packaging==24.0
     + pluggy==1.4.0
     + pytest==8.1.1
    warning: An executable named `pytest@8.0.0` is not provided by package `pytest`.
    "###);
}

#[test]
fn tool_run_from_version() {
    let context = TestContext::new("3.12");
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--from")
        .arg("pytest==8.0.0")
        .arg("pytest")
        .arg("--version")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    pytest 8.0.0

    ----- stderr -----
    warning: `uv tool run` is experimental and may change without warning
    Resolved 4 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + iniconfig==2.0.0
     + packaging==24.0
     + pluggy==1.4.0
     + pytest==8.0.0
    "###);
}

#[test]
fn tool_run_suggest_valid_commands() {
    let context = TestContext::new("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    uv_snapshot!(context.filters(), context.tool_run()
    .arg("--from")
    .arg("black")
    .arg("orange")
    .env("UV_TOOL_DIR", tool_dir.as_os_str())
    .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    The executable `orange` was not found.
    The following executables are provided by `black`:
    - black
    - blackd

    ----- stderr -----
    warning: `uv tool run` is experimental and may change without warning
    Resolved 6 packages in [TIME]
    Prepared 6 packages in [TIME]
    Installed 6 packages in [TIME]
     + black==24.3.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    warning: An executable named `orange` is not provided by package `black`.
    "###);

    uv_snapshot!(context.filters(), context.tool_run()
    .arg("fastapi-cli")
    .env("UV_TOOL_DIR", tool_dir.as_os_str())
    .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    The executable `fastapi-cli` was not found.

    ----- stderr -----
    warning: `uv tool run` is experimental and may change without warning
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + fastapi-cli==0.0.1
     + importlib-metadata==1.7.0
     + zipp==3.18.1
    warning: An executable named `fastapi-cli` is not provided by package `fastapi-cli`.
    "###);
}

#[test]
fn tool_run_warn_executable_not_in_from() {
    let context = TestContext::new("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");
    let mut filters = context.filters();
    filters.push(("\\+ uvloop(.+)\n ", ""));
    // Strip off the `fastapi` command output.
    filters.push(("(?s)fastapi` instead.*", "fastapi` instead."));

    uv_snapshot!(filters, context.tool_run()
        .arg("--from")
        .arg("fastapi")
        .arg("fastapi")
        .env("UV_EXCLUDE_NEWER", "2024-05-04T00:00:00Z") // TODO: Remove this once EXCLUDE_NEWER is bumped past 2024-05-04
        // (FastAPI 0.111 is only available from this date onwards)
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool run` is experimental and may change without warning
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
    "###);
}

#[test]
fn tool_run_from_install() {
    let context = TestContext::new("3.12");
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black` at a specific version.
    context
        .tool_install()
        .arg("black==24.1.0")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .assert()
        .success();

    // Verify that `tool run black` uses the already-installed version.
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("black")
        .arg("--version")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    black, 24.1.0 (compiled: yes)
    Python (CPython) 3.12.[X]

    ----- stderr -----
    warning: `uv tool run` is experimental and may change without warning
    "###);

    // Verify that `--isolated` uses an isolated environment.
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--isolated")
        .arg("black")
        .arg("--version")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    black, 24.3.0 (compiled: yes)
    Python (CPython) 3.12.[X]

    ----- stderr -----
    warning: `uv tool run` is experimental and may change without warning
    Resolved 6 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 6 packages in [TIME]
     + black==24.3.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    "###);

    // Verify that `tool run black` at a different version installs the new version.
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("black@24.1.1")
        .arg("--version")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    black, 24.1.1 (compiled: yes)
    Python (CPython) 3.12.[X]

    ----- stderr -----
    warning: `uv tool run` is experimental and may change without warning
    Resolved 6 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 6 packages in [TIME]
     + black==24.1.1
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    "###);

    // Verify that `--with` installs a new version.
    // TODO(charlie): This could (in theory) layer the `--with` requirements on top of the existing
    // environment.
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--with")
        .arg("iniconfig")
        .arg("black")
        .arg("--version")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    black, 24.3.0 (compiled: yes)
    Python (CPython) 3.12.[X]

    ----- stderr -----
    warning: `uv tool run` is experimental and may change without warning
    Resolved 7 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 7 packages in [TIME]
     + black==24.3.0
     + click==8.1.7
     + iniconfig==2.0.0
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    "###);

    // Verify that `tool run black` at a different version (via `--from`) installs the new version.
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--from")
        .arg("black==24.2.0")
        .arg("black")
        .arg("--version")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    black, 24.2.0 (compiled: yes)
    Python (CPython) 3.12.[X]

    ----- stderr -----
    warning: `uv tool run` is experimental and may change without warning
    Resolved 6 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 6 packages in [TIME]
     + black==24.2.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    "###);
}

#[test]
fn tool_run_cache() {
    let context = TestContext::new_with_versions(&["3.11", "3.12"]).with_filtered_counts();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Verify that `tool run black` installs the latest version.
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("-p")
        .arg("3.12")
        .arg("black")
        .arg("--version")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    black, 24.3.0 (compiled: yes)
    Python (CPython) 3.12.[X]

    ----- stderr -----
    warning: `uv tool run` is experimental and may change without warning
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + black==24.3.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    "###);

    // Verify that `tool run black` uses the cached version.
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("-p")
        .arg("3.12")
        .arg("black")
        .arg("--version")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    black, 24.3.0 (compiled: yes)
    Python (CPython) 3.12.[X]

    ----- stderr -----
    warning: `uv tool run` is experimental and may change without warning
    Resolved [N] packages in [TIME]
    "###);

    // Verify that `--reinstall` reinstalls everything.
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("-p")
        .arg("3.12")
        .arg("--reinstall")
        .arg("black")
        .arg("--version")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    black, 24.3.0 (compiled: yes)
    Python (CPython) 3.12.[X]

    ----- stderr -----
    warning: `uv tool run` is experimental and may change without warning
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + black==24.3.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    "###);

    // Verify that `--reinstall-package` reinstalls everything. We may want to change this.
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("-p")
        .arg("3.12")
        .arg("--reinstall-package")
        .arg("packaging")
        .arg("black")
        .arg("--version")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    black, 24.3.0 (compiled: yes)
    Python (CPython) 3.12.[X]

    ----- stderr -----
    warning: `uv tool run` is experimental and may change without warning
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + black==24.3.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    "###);

    // Verify that varying the interpreter leads to a fresh environment.
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("-p")
        .arg("3.11")
        .arg("black")
        .arg("--version")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    black, 24.3.0 (compiled: yes)
    Python (CPython) 3.11.[X]

    ----- stderr -----
    warning: `uv tool run` is experimental and may change without warning
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + black==24.3.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    "###);

    // But that re-invoking with the previous interpreter retains the cached version.
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("-p")
        .arg("3.12")
        .arg("black")
        .arg("--version")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    black, 24.3.0 (compiled: yes)
    Python (CPython) 3.12.[X]

    ----- stderr -----
    warning: `uv tool run` is experimental and may change without warning
    Resolved [N] packages in [TIME]
    "###);

    // Verify that `--with` leads to a fresh environment.
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("-p")
        .arg("3.12")
        .arg("--with")
        .arg("iniconfig")
        .arg("black")
        .arg("--version")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    black, 24.3.0 (compiled: yes)
    Python (CPython) 3.12.[X]

    ----- stderr -----
    warning: `uv tool run` is experimental and may change without warning
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
    "###);
}

#[test]
fn tool_run_url() {
    let context = TestContext::new("3.12").with_filtered_counts();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--from")
        .arg("flask @ https://files.pythonhosted.org/packages/61/80/ffe1da13ad9300f87c93af113edd0638c75138c42a0994becfacac078c06/flask-3.0.3-py3-none-any.whl")
        .arg("flask")
        .arg("--version")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.[X]
    Flask 3.0.3
    Werkzeug 3.0.1

    ----- stderr -----
    warning: `uv tool run` is experimental and may change without warning
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
    "###);
}

/// Read requirements from a `requirements.txt` file.
#[test]
fn tool_run_requirements_txt() {
    let context = TestContext::new("3.12").with_filtered_counts();
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
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.[X]
    Flask 3.0.2
    Werkzeug 3.0.1

    ----- stderr -----
    warning: `uv tool run` is experimental and may change without warning
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
    "###);
}

/// Ignore and warn when (e.g.) the `--index-url` argument is a provided `requirements.txt`.
#[test]
fn tool_run_requirements_txt_arguments() {
    let context = TestContext::new("3.12").with_filtered_counts();
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
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.[X]
    Flask 3.0.2
    Werkzeug 3.0.1

    ----- stderr -----
    warning: `uv tool run` is experimental and may change without warning
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
    "###);
}

/// List installed tools when no command arg is given (e.g. `uv tool run`).
#[test]
fn tool_run_list_installed() {
    let context = TestContext::new("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // No tools installed.
    uv_snapshot!(context.filters(), context.tool_run()
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool run` is experimental and may change without warning
    No tools installed
    "###);

    // Install `black`.
    context
        .tool_install()
        .arg("black==24.2.0")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .assert()
        .success();

    // List installed tools.
    uv_snapshot!(context.filters(), context.tool_run()
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    black v24.2.0
    - black
    - blackd

    ----- stderr -----
    warning: `uv tool run` is experimental and may change without warning
    "###);
}

/// By default, omit resolver and installer output.
#[test]
fn tool_run_without_output() {
    let context = TestContext::new("3.12").with_filtered_counts();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    uv_snapshot!(context.filters(), context.tool_run()
        .env_remove("UV_SHOW_RESOLUTION")
        .arg("--")
        .arg("pytest")
        .arg("--version")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    pytest 8.1.1

    ----- stderr -----
    warning: `uv tool run` is experimental and may change without warning
    "###);
}
