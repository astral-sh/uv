#![cfg(all(feature = "python", feature = "pypi"))]

use assert_cmd::prelude::*;
use assert_fs::prelude::*;
use common::{copy_dir_all, uv_snapshot, TestContext};
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
    error: Not a valid package or extra name: "pytest@". Names must start and end with a letter or digit and may only contain -, _, ., and alphanumeric characters.
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
    error: Not a valid package or extra name: "pytest@invalid". Names must start and end with a letter or digit and may only contain -, _, ., and alphanumeric characters.
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
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + fastapi-cli==0.0.1
     + importlib-metadata==1.7.0
     + zipp==3.18.1
    warning: Package `fastapi-cli` does not provide any executables.
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
    "###);
}

#[test]
fn tool_run_from_install() {
    let context = TestContext::new("3.12").with_filtered_counts();
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
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
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
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
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
    Resolved [N] packages in [TIME]
    "###);

    // Verify that `--refresh` recreates everything.
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("-p")
        .arg("3.12")
        .arg("--refresh")
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

    // Verify that `--refresh-package` recreates everything. We may want to change this.
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("-p")
        .arg("3.12")
        .arg("--refresh-package")
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
    "###);
}

/// By default, omit resolver and installer output.
#[test]
fn tool_run_without_output() {
    let context = TestContext::new("3.12").with_filtered_counts();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // On the first run, only show the summary line.
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
    Installed [N] packages in [TIME]
    "###);

    // Subsequent runs are quiet.
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
    "###);
}

#[test]
fn tool_run_with_editable() -> anyhow::Result<()> {
    let context = TestContext::new("3.12").with_filtered_counts();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    let anyio_local = context.temp_dir.child("src").child("anyio_local");
    copy_dir_all(
        context.workspace_root.join("scripts/packages/anyio_local"),
        &anyio_local,
    )?;

    let black_editable = context.temp_dir.child("src").child("black_editable");
    copy_dir_all(
        context
            .workspace_root
            .join("scripts/packages/black_editable"),
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
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
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
    "###);

    // Requesting an editable requirement should install it in a layer, even if it satisfied
    uv_snapshot!(context.filters(), context.tool_run().arg("--with-editable").arg("./src/anyio_local").arg("flask").arg("--version").env("UV_TOOL_DIR", tool_dir.as_os_str()).env("XDG_BIN_HOME", bin_dir.as_os_str())
    
    , @r###"
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
    "###);

    // Requesting the project itself should use a new environment.
    uv_snapshot!(context.filters(), context.tool_run().arg("--with-editable").arg(".").arg("flask").arg("--version").env("UV_TOOL_DIR", tool_dir.as_os_str()).env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
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
    "###);

    // If invalid, we should reference `--with`.
    uv_snapshot!(context.filters(), context
        .tool_run()
        .arg("--with")
        .arg("./foo")
        .arg("flask")
        .arg("--version")
        .env("UV_TOOL_DIR", tool_dir
        .as_os_str()).env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Invalid `--with` requirement
      ╰─▶ Distribution not found at: file://[TEMP_DIR]/foo
    "###);

    Ok(())
}

#[test]
fn warn_no_executables_found() {
    let context = TestContext::new("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("requests")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    The executable `requests` was not found.

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Prepared 5 packages in [TIME]
    Installed 5 packages in [TIME]
     + certifi==2024.2.2
     + charset-normalizer==3.3.2
     + idna==3.6
     + requests==2.31.0
     + urllib3==2.2.1
    warning: Package `requests` does not provide any executables.
    "###);
}

/// Warn when a user passes `--upgrade` to `uv tool run`.
#[test]
fn tool_run_upgrade_warn() {
    let context = TestContext::new("3.12").with_filtered_counts();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--upgrade")
        .arg("pytest")
        .arg("--version")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
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
    "###);

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--upgrade")
        .arg("--with")
        .arg("typing-extensions")
        .arg("pytest")
        .arg("--version")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
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
    "###);
}

/// If we fail to resolve the tool, we should include "tool" in the error message.
#[test]
fn tool_run_resolution_error() {
    let context = TestContext::new("3.12").with_filtered_counts();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    uv_snapshot!(context.filters(), context.tool_run()
        .arg("add")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving tool dependencies:
      ╰─▶ Because there are no versions of add and you require add, we can conclude that your requirements are unsatisfiable.
    "###);
}

#[test]
fn tool_run_latest() {
    let context = TestContext::new("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `pytest` at a specific version.
    context
        .tool_install()
        .arg("pytest==7.0.0")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .assert()
        .success();

    // Run `pytest`, which should use the installed version.
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("pytest")
        .arg("--version")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    pytest 7.0.0

    ----- stderr -----
    "###);

    // Run `pytest@latest`, which should use the latest version.
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("pytest@latest")
        .arg("--version")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
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
    "###);

    // Run `pytest`, which should use the installed version.
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("pytest")
        .arg("--version")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    pytest 7.0.0

    ----- stderr -----
    "###);
}
