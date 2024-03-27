use std::process::Command;

use anyhow::Result;
use assert_fs::fixture::PathChild;
use assert_fs::fixture::{FileTouch, FileWriteStr};

use common::uv_snapshot;

use crate::common::{get_bin, TestContext, EXCLUDE_NEWER};

mod common;

/// Create a `pip install` command with options shared across scenarios.
fn install_command(context: &TestContext) -> Command {
    let mut command = Command::new(get_bin());
    command
        .arg("pip")
        .arg("install")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .arg("--exclude-newer")
        .arg(EXCLUDE_NEWER)
        .env("VIRTUAL_ENV", context.venv.as_os_str())
        .current_dir(&context.temp_dir);

    if cfg!(all(windows, debug_assertions)) {
        // TODO(konstin): Reduce stack usage in debug mode enough that the tests pass with the
        // default windows stack of 1MB
        command.env("UV_STACK_SIZE", (2 * 1024 * 1024).to_string());
    }

    command
}

#[test]
fn list_empty() {
    let context = TestContext::new("3.12");

    uv_snapshot!(Command::new(get_bin())
        .arg("pip")
        .arg("list")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .env("VIRTUAL_ENV", context.venv.as_os_str())
        .current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    "###
    );
}

#[test]
fn list_single_no_editable() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("MarkupSafe==2.1.3")?;

    uv_snapshot!(install_command(&context)
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + markupsafe==2.1.3
    "###
    );

    context.assert_command("import markupsafe").success();

    uv_snapshot!(Command::new(get_bin())
        .arg("pip")
        .arg("list")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .env("VIRTUAL_ENV", context.venv.as_os_str())
        .current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Package    Version
    ---------- -------
    markupsafe 2.1.3

    ----- stderr -----
    "###
    );

    Ok(())
}

#[test]
fn list_editable() {
    let context = TestContext::new("3.12");

    // Install the editable package.
    uv_snapshot!(context.filters(), install_command(&context)
        .arg("-e")
        .arg(context.workspace_root.join("scripts/packages/poetry_editable")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Built 1 editable in [TIME]
    Resolved 4 packages in [TIME]
    Downloaded 3 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + poetry-editable==0.1.0 (from file://[WORKSPACE]/scripts/packages/poetry_editable)
     + sniffio==1.3.1
    "###
    );

    let filters = context
        .filters()
        .into_iter()
        .chain(vec![(r"\-\-\-\-\-\-+.*", "[UNDERLINE]"), ("  +", " ")])
        .collect::<Vec<_>>();

    uv_snapshot!(filters, Command::new(get_bin())
        .arg("pip")
        .arg("list")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .env("VIRTUAL_ENV", context.venv.as_os_str())
        .current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Package Version Editable project location
    [UNDERLINE]
    anyio 4.3.0
    idna 3.6
    poetry-editable 0.1.0 [WORKSPACE]/scripts/packages/poetry_editable
    sniffio 1.3.1

    ----- stderr -----
    "###
    );
}

#[test]
fn list_editable_only() {
    let context = TestContext::new("3.12");

    // Install the editable package.
    uv_snapshot!(context.filters(), install_command(&context)
        .arg("-e")
        .arg(context.workspace_root.join("scripts/packages/poetry_editable")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Built 1 editable in [TIME]
    Resolved 4 packages in [TIME]
    Downloaded 3 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + poetry-editable==0.1.0 (from file://[WORKSPACE]/scripts/packages/poetry_editable)
     + sniffio==1.3.1
    "###
    );

    let filters = context
        .filters()
        .into_iter()
        .chain(vec![(r"\-\-\-\-\-\-+.*", "[UNDERLINE]"), ("  +", " ")])
        .collect::<Vec<_>>();

    uv_snapshot!(filters, Command::new(get_bin())
        .arg("pip")
        .arg("list")
        .arg("--editable")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .env("VIRTUAL_ENV", context.venv.as_os_str())
        .current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Package Version Editable project location
    [UNDERLINE]
    poetry-editable 0.1.0 [WORKSPACE]/scripts/packages/poetry_editable

    ----- stderr -----
    "###
    );

    uv_snapshot!(filters, Command::new(get_bin())
        .arg("pip")
        .arg("list")
        .arg("--exclude-editable")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .env("VIRTUAL_ENV", context.venv.as_os_str())
        .current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Package Version
    [UNDERLINE]
    anyio 4.3.0
    idna 3.6
    sniffio 1.3.1

    ----- stderr -----
    "###
    );

    uv_snapshot!(filters, Command::new(get_bin())
        .arg("pip")
        .arg("list")
        .arg("--editable")
        .arg("--exclude-editable")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .env("VIRTUAL_ENV", context.venv.as_os_str())
        .current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    "###
    );
}

#[test]
fn list_exclude() {
    let context = TestContext::new("3.12");

    // Install the editable package.
    uv_snapshot!(context.filters(), install_command(&context)
        .arg("-e")
        .arg(context.workspace_root.join("scripts/packages/poetry_editable")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Built 1 editable in [TIME]
    Resolved 4 packages in [TIME]
    Downloaded 3 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + poetry-editable==0.1.0 (from file://[WORKSPACE]/scripts/packages/poetry_editable)
     + sniffio==1.3.1
    "###
    );

    let filters = context
        .filters()
        .into_iter()
        .chain(vec![(r"\-\-\-\-\-\-+.*", "[UNDERLINE]"), ("  +", " ")])
        .collect::<Vec<_>>();

    uv_snapshot!(filters, Command::new(get_bin())
    .arg("pip")
    .arg("list")
    .arg("--exclude")
    .arg("numpy")
    .arg("--cache-dir")
    .arg(context.cache_dir.path())
    .env("VIRTUAL_ENV", context.venv.as_os_str())
    .current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Package Version Editable project location
    [UNDERLINE]
    anyio 4.3.0
    idna 3.6
    poetry-editable 0.1.0 [WORKSPACE]/scripts/packages/poetry_editable
    sniffio 1.3.1

    ----- stderr -----
    "###
    );

    uv_snapshot!(filters, Command::new(get_bin())
    .arg("pip")
    .arg("list")
    .arg("--exclude")
    .arg("poetry-editable")
    .arg("--cache-dir")
    .arg(context.cache_dir.path())
    .env("VIRTUAL_ENV", context.venv.as_os_str())
    .current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Package Version
    [UNDERLINE]
    anyio 4.3.0
    idna 3.6
    sniffio 1.3.1

    ----- stderr -----
    "###
    );

    uv_snapshot!(filters, Command::new(get_bin())
    .arg("pip")
    .arg("list")
    .arg("--exclude")
    .arg("numpy")
    .arg("--exclude")
    .arg("poetry-editable")
    .arg("--cache-dir")
    .arg(context.cache_dir.path())
    .env("VIRTUAL_ENV", context.venv.as_os_str())
    .current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Package Version
    [UNDERLINE]
    anyio 4.3.0
    idna 3.6
    sniffio 1.3.1

    ----- stderr -----
    "###
    );
}

#[test]
#[cfg(not(windows))]
fn list_format_json() {
    let context = TestContext::new("3.12");

    // Install the editable package.
    uv_snapshot!(context.filters(), install_command(&context)
        .arg("-e")
        .arg(context.workspace_root.join("scripts/packages/poetry_editable")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Built 1 editable in [TIME]
    Resolved 4 packages in [TIME]
    Downloaded 3 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + poetry-editable==0.1.0 (from file://[WORKSPACE]/scripts/packages/poetry_editable)
     + sniffio==1.3.1
    "###
    );

    let filters: Vec<_> = context
        .filters()
        .into_iter()
        .chain(vec![(r"\-\-\-\-\-\-+.*", "[UNDERLINE]"), ("  +", " ")])
        .collect();

    uv_snapshot!(filters, Command::new(get_bin())
    .arg("pip")
    .arg("list")
    .arg("--format=json")
    .arg("--cache-dir")
    .arg(context.cache_dir.path())
    .env("VIRTUAL_ENV", context.venv.as_os_str())
    .current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [{"name":"anyio","version":"4.3.0"},{"name":"idna","version":"3.6"},{"name":"poetry-editable","version":"0.1.0","editable_project_location":"[WORKSPACE]/scripts/packages/poetry_editable"},{"name":"sniffio","version":"1.3.1"}]

    ----- stderr -----
    "###
    );

    uv_snapshot!(filters, Command::new(get_bin())
    .arg("pip")
    .arg("list")
    .arg("--format=json")
    .arg("--editable")
    .arg("--cache-dir")
    .arg(context.cache_dir.path())
    .env("VIRTUAL_ENV", context.venv.as_os_str())
    .current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [{"name":"poetry-editable","version":"0.1.0","editable_project_location":"[WORKSPACE]/scripts/packages/poetry_editable"}]

    ----- stderr -----
    "###
    );

    uv_snapshot!(filters, Command::new(get_bin())
    .arg("pip")
    .arg("list")
    .arg("--format=json")
    .arg("--exclude-editable")
    .arg("--cache-dir")
    .arg(context.cache_dir.path())
    .env("VIRTUAL_ENV", context.venv.as_os_str())
    .current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    [{"name":"anyio","version":"4.3.0"},{"name":"idna","version":"3.6"},{"name":"sniffio","version":"1.3.1"}]

    ----- stderr -----
    "###
    );

    uv_snapshot!(filters, Command::new(get_bin())
    .arg("pip")
    .arg("list")
    .arg("--format=json")
    .arg("--editable")
    .arg("--exclude-editable")
    .arg("--cache-dir")
    .arg(context.cache_dir.path())
    .env("VIRTUAL_ENV", context.venv.as_os_str())
    .current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    "###
    );
}

#[test]
fn list_format_freeze() {
    let context = TestContext::new("3.12");

    // Install the editable package.
    uv_snapshot!(context.filters(), install_command(&context)
        .arg("-e")
        .arg(context.workspace_root.join("scripts/packages/poetry_editable")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Built 1 editable in [TIME]
    Resolved 4 packages in [TIME]
    Downloaded 3 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + poetry-editable==0.1.0 (from file://[WORKSPACE]/scripts/packages/poetry_editable)
     + sniffio==1.3.1
    "###
    );

    let filters = context
        .filters()
        .into_iter()
        .chain(vec![(r"\-\-\-\-\-\-+.*", "[UNDERLINE]"), ("  +", " ")])
        .collect::<Vec<_>>();

    uv_snapshot!(filters, Command::new(get_bin())
    .arg("pip")
    .arg("list")
    .arg("--format=freeze")
    .arg("--cache-dir")
    .arg(context.cache_dir.path())
    .env("VIRTUAL_ENV", context.venv.as_os_str())
    .current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    anyio==4.3.0
    idna==3.6
    poetry-editable==0.1.0
    sniffio==1.3.1

    ----- stderr -----
    "###
    );

    uv_snapshot!(filters, Command::new(get_bin())
    .arg("pip")
    .arg("list")
    .arg("--format=freeze")
    .arg("--editable")
    .arg("--cache-dir")
    .arg(context.cache_dir.path())
    .env("VIRTUAL_ENV", context.venv.as_os_str())
    .current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    poetry-editable==0.1.0

    ----- stderr -----
    "###
    );

    uv_snapshot!(filters, Command::new(get_bin())
    .arg("pip")
    .arg("list")
    .arg("--format=freeze")
    .arg("--exclude-editable")
    .arg("--cache-dir")
    .arg(context.cache_dir.path())
    .env("VIRTUAL_ENV", context.venv.as_os_str())
    .current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    anyio==4.3.0
    idna==3.6
    sniffio==1.3.1

    ----- stderr -----
    "###
    );

    uv_snapshot!(filters, Command::new(get_bin())
    .arg("pip")
    .arg("list")
    .arg("--format=freeze")
    .arg("--editable")
    .arg("--exclude-editable")
    .arg("--cache-dir")
    .arg(context.cache_dir.path())
    .env("VIRTUAL_ENV", context.venv.as_os_str())
    .current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    "###
    );
}
