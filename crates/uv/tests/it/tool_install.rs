#[cfg(any(feature = "test-git", feature = "test-git-lfs"))]
use std::collections::BTreeSet;
use std::process::Command;

use anyhow::Result;
use assert_cmd::assert::OutputAssertExt;
use assert_fs::{
    assert::PathAssert,
    fixture::{FileTouch, FileWriteStr, PathChild},
};
use indoc::indoc;
use insta::assert_snapshot;
use predicates::prelude::predicate;
use uv_fs::copy_dir_all;
use uv_static::EnvVars;

use uv_test::uv_snapshot;

#[test]
fn tool_install() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

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
    Installed 2 executables: black, blackd
    ");

    tool_dir.child("black").assert(predicate::path::is_dir());
    tool_dir
        .child("black")
        .child("uv-receipt.toml")
        .assert(predicate::path::exists());

    let executable = bin_dir.child(format!("black{}", std::env::consts::EXE_SUFFIX));
    assert!(executable.exists());

    // On Windows, we can't snapshot an executable file.
    #[cfg(not(windows))]
    insta::with_settings!({
        filters => context.filters(),
    }, {
        // Should run black in the virtual environment
        assert_snapshot!(fs_err::read_to_string(executable).unwrap(), @r#"
        #![TEMP_DIR]/tools/black/bin/python
        # -*- coding: utf-8 -*-
        import sys
        from black import patched_main
        if __name__ == "__main__":
            if sys.argv[0].endswith("-script.pyw"):
                sys.argv[0] = sys.argv[0][:-11]
            elif sys.argv[0].endswith(".exe"):
                sys.argv[0] = sys.argv[0][:-4]
            sys.exit(patched_main())
        "#);

    });

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        [tool]
        requirements = [{ name = "black" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    uv_snapshot!(context.filters(), Command::new("black").arg("--version").env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    black, 24.3.0 (compiled: yes)
    Python (CPython) 3.12.[X]

    ----- stderr -----
    ");

    // Install another tool
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("flask")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

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
     + werkzeug==3.0.1
    Installed 1 executable: flask
    ");

    tool_dir.child("flask").assert(predicate::path::is_dir());
    assert!(
        bin_dir
            .child(format!("flask{}", std::env::consts::EXE_SUFFIX))
            .exists()
    );

    #[cfg(not(windows))]
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(fs_err::read_to_string(bin_dir.join("flask")).unwrap(), @r#"
        #![TEMP_DIR]/tools/flask/bin/python
        # -*- coding: utf-8 -*-
        import sys
        from flask.cli import main
        if __name__ == "__main__":
            if sys.argv[0].endswith("-script.pyw"):
                sys.argv[0] = sys.argv[0][:-11]
            elif sys.argv[0].endswith(".exe"):
                sys.argv[0] = sys.argv[0][:-4]
            sys.exit(main())
        "#);
    });

    uv_snapshot!(context.filters(), Command::new("flask").arg("--version").env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.[X]
    Flask 3.0.2
    Werkzeug 3.0.1

    ----- stderr -----
    ");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("flask").join("uv-receipt.toml")).unwrap(), @r#"
        [tool]
        requirements = [{ name = "flask" }]
        entrypoints = [
            { name = "flask", install-path = "[TEMP_DIR]/bin/flask", from = "flask" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });
}

#[test]
fn tool_install_python_from_global_version_file() {
    let context = uv_test::test_context_with_versions!(&["3.11", "3.12", "3.13"])
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Pin to 3.12
    context
        .python_pin()
        .arg("3.12")
        .arg("--global")
        .assert()
        .success();

    // Install a tool
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("flask")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

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
     + werkzeug==3.0.1
    Installed 1 executable: flask
    ");

    // It should use the version from the global file
    uv_snapshot!(context.filters(), Command::new("flask").arg("--version").env(EnvVars::PATH, bin_dir.as_os_str()), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.[X]
    Flask 3.0.2
    Werkzeug 3.0.1

    ----- stderr -----
    ");

    // Change global version
    context
        .python_pin()
        .arg("3.13")
        .arg("--global")
        .assert()
        .success();

    // Installing flask again should be a no-op, even though the global pin changed
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("flask")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    `flask` is already installed
    ");

    uv_snapshot!(context.filters(), Command::new("flask").arg("--version").env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.[X]
    Flask 3.0.2
    Werkzeug 3.0.1

    ----- stderr -----
    ");

    // Using `--upgrade` forces us to check the environment
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("flask")
        .arg("--upgrade")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Audited [N] packages in [TIME]
    Installed 1 executable: flask
    ");

    // This will not change to the new global pin, since there was not a reinstall request
    uv_snapshot!(context.filters(), Command::new("flask").arg("--version").env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.[X]
    Flask 3.0.2
    Werkzeug 3.0.1

    ----- stderr -----
    ");

    // Using `--reinstall` forces us to install flask again
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("flask")
        .arg("--reinstall")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Ignoring existing environment for `flask`: the Python interpreter does not match the environment interpreter
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + blinker==1.7.0
     + click==8.1.7
     + flask==3.0.2
     + itsdangerous==2.1.2
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + werkzeug==3.0.1
    Installed 1 executable: flask
    ");

    // This will change to the new global pin, since there was not an explicit request recorded in
    // the receipt
    uv_snapshot!(context.filters(), Command::new("flask").arg("--version").env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.13.[X]
    Flask 3.0.2
    Werkzeug 3.0.1

    ----- stderr -----
    ");

    // If we request a specific Python version, it takes precedence over the pin
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("flask")
        .arg("--python")
        .arg("3.11")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Ignoring existing environment for `flask`: the requested Python interpreter does not match the environment interpreter
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + blinker==1.7.0
     + click==8.1.7
     + flask==3.0.2
     + itsdangerous==2.1.2
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + werkzeug==3.0.1
    Installed 1 executable: flask
    ");

    uv_snapshot!(context.filters(), Command::new("flask").arg("--version").env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.11.[X]
    Flask 3.0.2
    Werkzeug 3.0.1

    ----- stderr -----
    ");

    // Use `--reinstall` to install flask again
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("flask")
        .arg("--reinstall")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Uninstalled [N] packages in [TIME]
    Installed [N] packages in [TIME]
     ~ blinker==1.7.0
     ~ click==8.1.7
     ~ flask==3.0.2
     ~ itsdangerous==2.1.2
     ~ jinja2==3.1.3
     ~ markupsafe==2.1.5
     ~ werkzeug==3.0.1
    Installed 1 executable: flask
    ");

    // We should continue to use the version from the install, not the global pin
    uv_snapshot!(context.filters(), Command::new("flask").arg("--version").env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.11.[X]
    Flask 3.0.2
    Werkzeug 3.0.1

    ----- stderr -----
    ");
}

#[test]
fn tool_install_with_editable() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_exclude_newer("2025-01-18T00:00:00Z")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");
    let anyio_local = context.temp_dir.child("src").child("anyio_local");
    copy_dir_all(
        context.workspace_root.join("test/packages/anyio_local"),
        &anyio_local,
    )?;

    uv_snapshot!(context.filters(), context.tool_install()
        .arg("--with-editable")
        .arg("./src/anyio_local")
        .arg("--with")
        .arg("iniconfig")
        .arg("executable-application")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + anyio==4.3.0+foo (from file://[TEMP_DIR]/src/anyio_local)
     + executable-application==0.3.0
     + iniconfig==2.0.0
    Installed 1 executable: app
    ");

    Ok(())
}

#[test]
fn tool_install_with_compatible_build_constraints() -> Result<()> {
    let context = uv_test::test_context!("3.9")
        .with_exclude_newer("2024-05-04T00:00:00Z")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    let constraints_txt = context.temp_dir.child("build_constraints.txt");
    constraints_txt.write_str("setuptools>=40")?;

    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("--with")
        .arg("requests==1.2")
        .arg("--build-constraints")
        .arg("build_constraints.txt")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + black==24.4.2
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.1
     + requests==1.2.0
     + tomli==2.0.1
     + typing-extensions==4.11.0
    Installed 2 executables: black, blackd
    ");

    tool_dir
        .child("black")
        .child("uv-receipt.toml")
        .assert(predicate::path::exists());

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        [tool]
        requirements = [
            { name = "black" },
            { name = "requests", specifier = "==1.2" },
        ]
        build-constraint-dependencies = [{ name = "setuptools", specifier = ">=40" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-05-04T00:00:00Z"
        "#);
    });

    Ok(())
}

#[test]
fn tool_install_with_incompatible_build_constraints() -> Result<()> {
    let context = uv_test::test_context!("3.9")
        .with_exclude_newer("2024-05-04T00:00:00Z")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    let constraints_txt = context.temp_dir.child("build_constraints.txt");
    constraints_txt.write_str("setuptools==2")?;

    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("--with")
        .arg("requests==1.2")
        .arg("--build-constraints")
        .arg("build_constraints.txt")
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

    tool_dir
        .child("black")
        .child("uv-receipt.toml")
        .assert(predicate::path::missing());

    Ok(())
}

#[test]
fn tool_install_suggest_other_packages_with_executable() {
    // FastAPI 0.111 is only available from this date onwards.
    let context = uv_test::test_context!("3.12")
        .with_exclude_newer("2024-05-04T00:00:00Z")
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");
    let context = context.with_filter(("\\+ uvloop(.+)\n ", ""));

    uv_snapshot!(context.filters(), context.tool_install()
        .arg("fastapi==0.111.0")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: false
    exit_code: 2
    ----- stdout -----
    No executables are provided by package `fastapi`; removing tool
    hint: An executable with the name `fastapi` is available via dependency `fastapi-cli`.
          Did you mean `uv tool install fastapi-cli`?

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
    error: Failed to install entrypoints for `fastapi`
    ");
}

/// Test installing a tool at a version
#[test]
fn tool_install_version() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black==24.2.0")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 6 packages in [TIME]
    Installed 6 packages in [TIME]
     + black==24.2.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    Installed 2 executables: black, blackd
    ");

    tool_dir.child("black").assert(predicate::path::is_dir());
    tool_dir
        .child("black")
        .child("uv-receipt.toml")
        .assert(predicate::path::exists());

    let executable = bin_dir.child(format!("black{}", std::env::consts::EXE_SUFFIX));
    assert!(executable.exists());

    // On Windows, we can't snapshot an executable file.
    #[cfg(not(windows))]
    insta::with_settings!({
        filters => context.filters(),
    }, {
        // Should run black in the virtual environment
        assert_snapshot!(fs_err::read_to_string(executable).unwrap(), @r#"
        #![TEMP_DIR]/tools/black/bin/python
        # -*- coding: utf-8 -*-
        import sys
        from black import patched_main
        if __name__ == "__main__":
            if sys.argv[0].endswith("-script.pyw"):
                sys.argv[0] = sys.argv[0][:-11]
            elif sys.argv[0].endswith(".exe"):
                sys.argv[0] = sys.argv[0][:-4]
            sys.exit(patched_main())
        "#);

    });

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
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

    uv_snapshot!(context.filters(), Command::new("black").arg("--version").env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    black, 24.2.0 (compiled: yes)
    Python (CPython) 3.12.[X]

    ----- stderr -----
    ");
}

/// Test an editable installation of a tool.
#[test]
fn tool_install_editable() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black` as an editable package.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("-e")
        .arg(context.workspace_root.join("test/packages/black_editable"))
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + black==0.1.0 (from file://[WORKSPACE]/test/packages/black_editable)
    Installed 1 executable: black
    ");

    tool_dir.child("black").assert(predicate::path::is_dir());
    tool_dir
        .child("black")
        .child("uv-receipt.toml")
        .assert(predicate::path::exists());

    let executable = bin_dir.child(format!("black{}", std::env::consts::EXE_SUFFIX));
    assert!(executable.exists());

    // On Windows, we can't snapshot an executable file.
    #[cfg(not(windows))]
    insta::with_settings!({
        filters => context.filters(),
    }, {
        // Should run black in the virtual environment
        assert_snapshot!(fs_err::read_to_string(&executable).unwrap(), @r#"
        #![TEMP_DIR]/tools/black/bin/python
        # -*- coding: utf-8 -*-
        import sys
        from black import main
        if __name__ == "__main__":
            if sys.argv[0].endswith("-script.pyw"):
                sys.argv[0] = sys.argv[0][:-11]
            elif sys.argv[0].endswith(".exe"):
                sys.argv[0] = sys.argv[0][:-4]
            sys.exit(main())
        "#);

    });

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        [tool]
        requirements = [{ name = "black", editable = "[WORKSPACE]/test/packages/black_editable" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    uv_snapshot!(context.filters(), Command::new("black").arg("--version").env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello world!

    ----- stderr -----
    ");

    // Request `black`. It should reinstall from the registry.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Audited 1 package in [TIME]
    Installed 1 executable: black
    ");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        [tool]
        requirements = [{ name = "black" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    // Request `black` at a different version. It should install a new version.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("--from")
        .arg("black==24.2.0")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 6 packages in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 6 packages in [TIME]
     - black==0.1.0 (from file://[WORKSPACE]/test/packages/black_editable)
     + black==24.2.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    Installed 2 executables: black, blackd
    ");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
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
}

/// Ensure that we remove any existing entrypoints upon error.
#[test]
fn tool_install_remove_on_empty() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Request `black`. It should reinstall from the registry.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
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
    Installed 2 executables: black, blackd
    ");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        [tool]
        requirements = [{ name = "black" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    // Install `black` as an editable package, but without any entrypoints.
    let black = context.temp_dir.child("black");
    fs_err::create_dir_all(black.path())?;

    let pyproject_toml = black.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "black"
        version = "0.1.0"
        description = "Black without any entrypoints"
        authors = []
        dependencies = []
        requires-python = ">=3.11,<3.13"

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
        "#
    })?;

    let src = black.child("src").child("black");
    fs_err::create_dir_all(src.path())?;

    let init = src.child("__init__.py");
    init.touch()?;

    uv_snapshot!(context.filters(), context.tool_install()
        .arg("-e")
        .arg(black.path())
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: false
    exit_code: 2
    ----- stdout -----
    No executables are provided by package `black`; removing tool

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 6 packages in [TIME]
    Installed 1 package in [TIME]
     - black==24.3.0
     + black==0.1.0 (from file://[TEMP_DIR]/black)
     - click==8.1.7
     - mypy-extensions==1.0.0
     - packaging==24.0
     - pathspec==0.12.1
     - platformdirs==4.2.0
    error: Failed to install entrypoints for `black`
    ");

    // Re-request `black`. It should reinstall, without requiring `--force`.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Installed 6 packages in [TIME]
     + black==24.3.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    Installed 2 executables: black, blackd
    ");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        [tool]
        requirements = [{ name = "black" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    Ok(())
}

/// Test an editable installation of a tool using `--from`.
#[test]
fn tool_install_editable_from() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black` as an editable package.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("-e")
        .arg("--from")
        .arg(context.workspace_root.join("test/packages/black_editable"))
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + black==0.1.0 (from file://[WORKSPACE]/test/packages/black_editable)
    Installed 1 executable: black
    ");

    tool_dir.child("black").assert(predicate::path::is_dir());
    tool_dir
        .child("black")
        .child("uv-receipt.toml")
        .assert(predicate::path::exists());

    let executable = bin_dir.child(format!("black{}", std::env::consts::EXE_SUFFIX));
    assert!(executable.exists());

    // On Windows, we can't snapshot an executable file.
    #[cfg(not(windows))]
    insta::with_settings!({
        filters => context.filters(),
    }, {
        // Should run black in the virtual environment
        assert_snapshot!(fs_err::read_to_string(&executable).unwrap(), @r#"
        #![TEMP_DIR]/tools/black/bin/python
        # -*- coding: utf-8 -*-
        import sys
        from black import main
        if __name__ == "__main__":
            if sys.argv[0].endswith("-script.pyw"):
                sys.argv[0] = sys.argv[0][:-11]
            elif sys.argv[0].endswith(".exe"):
                sys.argv[0] = sys.argv[0][:-4]
            sys.exit(main())
        "#);

    });

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        [tool]
        requirements = [{ name = "black", editable = "[WORKSPACE]/test/packages/black_editable" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    uv_snapshot!(context.filters(), Command::new("black").arg("--version").env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello world!

    ----- stderr -----
    ");
}

/// Test installing a tool with `uv tool install --from`
#[test]
fn tool_install_from() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black` using `--from` to specify the version
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("--from")
        .arg("black==24.2.0")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 6 packages in [TIME]
    Installed 6 packages in [TIME]
     + black==24.2.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    Installed 2 executables: black, blackd
    ");

    // Attempt to install `black` using `--from` with a different package name
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("--from")
        .arg("flask==24.2.0")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Package name (`flask`) provided with `--from` does not match install request (`black`)
    ");

    // Attempt to install `black` using `--from` with a different version
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black==24.2.0")
        .arg("--from")
        .arg("black==24.3.0")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Package requirement (`black==24.3.0`) provided with `--from` conflicts with install request (`black==24.2.0`)
    ");
}

/// Test installing and reinstalling an already installed tool
#[test]
fn tool_install_already_installed() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

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
    Installed 2 executables: black, blackd
    ");

    tool_dir.child("black").assert(predicate::path::is_dir());
    tool_dir
        .child("black")
        .child("uv-receipt.toml")
        .assert(predicate::path::exists());

    let executable = bin_dir.child(format!("black{}", std::env::consts::EXE_SUFFIX));
    assert!(executable.exists());

    // On Windows, we can't snapshot an executable file.
    #[cfg(not(windows))]
    insta::with_settings!({
        filters => context.filters(),
    }, {
        // Should run black in the virtual environment
        assert_snapshot!(fs_err::read_to_string(executable).unwrap(), @r#"
        #![TEMP_DIR]/tools/black/bin/python
        # -*- coding: utf-8 -*-
        import sys
        from black import patched_main
        if __name__ == "__main__":
            if sys.argv[0].endswith("-script.pyw"):
                sys.argv[0] = sys.argv[0][:-11]
            elif sys.argv[0].endswith(".exe"):
                sys.argv[0] = sys.argv[0][:-4]
            sys.exit(patched_main())
        "#);
    });

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        [tool]
        requirements = [{ name = "black" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    // Install `black` again
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    `black` is already installed
    ");

    tool_dir.child("black").assert(predicate::path::is_dir());
    bin_dir
        .child(format!("black{}", std::env::consts::EXE_SUFFIX))
        .assert(predicate::path::exists());

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should not have an additional tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        [tool]
        requirements = [{ name = "black" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    // Install `black` again with the `--reinstall` flag
    // We should recreate the entire environment and reinstall the entry points
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("--reinstall")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Uninstalled [N] packages in [TIME]
    Installed [N] packages in [TIME]
     ~ black==24.3.0
     ~ click==8.1.7
     ~ mypy-extensions==1.0.0
     ~ packaging==24.0
     ~ pathspec==0.12.1
     ~ platformdirs==4.2.0
    Installed 2 executables: black, blackd
    ");

    // Install `black` again with `--reinstall-package` for `black`
    // We should reinstall `black` in the environment and reinstall the entry points
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("--reinstall-package")
        .arg("black")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Uninstalled [N] packages in [TIME]
    Installed [N] packages in [TIME]
     ~ black==24.3.0
    Installed 2 executables: black, blackd
    ");

    // Install `black` again with `--reinstall-package` for a dependency
    // We should reinstall `click` in the environment but not reinstall `black`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("--reinstall-package")
        .arg("click")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Uninstalled [N] packages in [TIME]
    Installed [N] packages in [TIME]
     ~ click==8.1.7
    Installed 2 executables: black, blackd
    ");
}

/// Test installing a tool when its entry point already exists
#[test]
fn tool_install_force() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    let executable = bin_dir.child(format!("black{}", std::env::consts::EXE_SUFFIX));
    executable.touch().unwrap();

    // Attempt to install `black`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: false
    exit_code: 2
    ----- stdout -----

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
    error: Executable already exists: black (use `--force` to overwrite)
    ");

    // We should delete the virtual environment
    assert!(!tool_dir.child("black").exists());

    // We should not write a tools entry
    assert!(!tool_dir.join("black").join("uv-receipt.toml").exists());

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // Nor should we change the `black` entry point that exists
        assert_snapshot!(fs_err::read_to_string(&executable).unwrap(), @"");

    });

    // Attempt to install `black` with the `--reinstall` flag
    // Should have no effect
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("--reinstall")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: false
    exit_code: 2
    ----- stdout -----

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
    error: Executable already exists: black (use `--force` to overwrite)
    ");

    // We should not create a virtual environment
    assert!(!tool_dir.child("black").exists());

    // We should not write a tools entry
    assert!(!tool_dir.join("tools.toml").exists());

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // Nor should we change the `black` entry point that exists
        assert_snapshot!(fs_err::read_to_string(&executable).unwrap(), @"");

    });

    // Test error message when multiple entry points exist
    bin_dir
        .child(format!("blackd{}", std::env::consts::EXE_SUFFIX))
        .touch()
        .unwrap();
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + black==24.3.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    error: Executables already exist: black, blackd (use `--force` to overwrite)
    ");

    // Install `black` with `--force`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("--force")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + black==24.3.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    Installed 2 executables: black, blackd
    ");

    tool_dir.child("black").assert(predicate::path::is_dir());

    // Re-install `black` with `--force`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("--force")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Uninstalled [N] packages in [TIME]
    Installed [N] packages in [TIME]
     ~ black==24.3.0
    Installed 2 executables: black, blackd
    ");

    tool_dir.child("black").assert(predicate::path::is_dir());

    // Re-install `black` without `--force`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    `black` is already installed
    ");

    tool_dir.child("black").assert(predicate::path::is_dir());

    // Re-install `black` with `--reinstall`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("--reinstall")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Uninstalled [N] packages in [TIME]
    Installed [N] packages in [TIME]
     ~ black==24.3.0
     ~ click==8.1.7
     ~ mypy-extensions==1.0.0
     ~ packaging==24.0
     ~ pathspec==0.12.1
     ~ platformdirs==4.2.0
    Installed 2 executables: black, blackd
    ");

    tool_dir.child("black").assert(predicate::path::is_dir());

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We write a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        [tool]
        requirements = [{ name = "black" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    // On Windows, we can't snapshot an executable file.
    #[cfg(not(windows))]
    insta::with_settings!({
        filters => context.filters(),
    }, {
        // Should run black in the virtual environment
        assert_snapshot!(fs_err::read_to_string(executable).unwrap(), @r#"
        #![TEMP_DIR]/tools/black/bin/python3
        # -*- coding: utf-8 -*-
        import sys
        from black import patched_main
        if __name__ == "__main__":
            if sys.argv[0].endswith("-script.pyw"):
                sys.argv[0] = sys.argv[0][:-11]
            elif sys.argv[0].endswith(".exe"):
                sys.argv[0] = sys.argv[0][:-4]
            sys.exit(patched_main())
        "#);

    });

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        [tool]
        requirements = [{ name = "black" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    uv_snapshot!(context.filters(), Command::new("black").arg("--version").env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    black, 24.3.0 (compiled: yes)
    Python (CPython) 3.12.[X]

    ----- stderr -----
    ");
}

/// Test `uv tool install` when the bin directory is inferred from `$HOME`
///
/// Only tested on Linux right now because it's not clear how to change the %USERPROFILE% on Windows
#[cfg(unix)]
#[test]
fn tool_install_home() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");

    // Install `black`
    let mut cmd = context.tool_install();
    cmd.arg("black")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(
            EnvVars::XDG_DATA_HOME,
            context.home_dir.child(".local").child("share").as_os_str(),
        )
        .env(
            EnvVars::PATH,
            context.home_dir.child(".local").child("bin").as_os_str(),
        );
    uv_snapshot!(context.filters(), cmd, @"
    success: true
    exit_code: 0
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
    Installed 2 executables: black, blackd
    ");

    context
        .home_dir
        .child(format!(".local/bin/black{}", std::env::consts::EXE_SUFFIX))
        .assert(predicate::path::exists());
}

/// Test `uv tool install` when the bin directory is inferred from `$XDG_DATA_HOME`
#[test]
fn tool_install_xdg_data_home() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let data_home = context.temp_dir.child("data/home");
    let bin_dir = context.temp_dir.child("data/bin");

    // Install `black`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_DATA_HOME, data_home.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
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
    Installed 2 executables: black, blackd
    ");

    context
        .temp_dir
        .child(format!("data/bin/black{}", std::env::consts::EXE_SUFFIX))
        .assert(predicate::path::exists());
}

/// Test `uv tool install` when the bin directory is set by `$XDG_BIN_HOME`
#[test]
fn tool_install_xdg_bin_home() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
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
    Installed 2 executables: black, blackd
    ");

    bin_dir
        .child(format!("black{}", std::env::consts::EXE_SUFFIX))
        .assert(predicate::path::exists());
}

/// Test `uv tool install` when the bin directory is set by `$UV_TOOL_BIN_DIR`
#[test]
fn tool_install_tool_bin_dir() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::UV_TOOL_BIN_DIR, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
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
    Installed 2 executables: black, blackd
    ");

    bin_dir
        .child(format!("black{}", std::env::consts::EXE_SUFFIX))
        .assert(predicate::path::exists());
}

/// Test installing a tool that lacks entrypoints
#[test]
fn tool_install_no_entrypoints() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    uv_snapshot!(context.filters(), context.tool_install()
        .arg("iniconfig")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: false
    exit_code: 2
    ----- stdout -----
    No executables are provided by package `iniconfig`; removing tool

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    error: Failed to install entrypoints for `iniconfig`
    ");

    // Ensure the tool environment is not created.
    tool_dir
        .child("iniconfig")
        .assert(predicate::path::missing());
    bin_dir
        .child("iniconfig")
        .assert(predicate::path::missing());
}

/// Test installing a package that can't be installed.
#[test]
fn tool_install_uninstallable() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    let filters = context
        .filters()
        .into_iter()
        .chain([
            (r"bdist\.[^/\\\s]+(-[^/\\\s]+)?", "bdist.linux-x86_64"),
            (r"\\\.", ""),
            (r"#+", "#"),
        ])
        .collect::<Vec<_>>();
    uv_snapshot!(filters, context.tool_install()
        .arg("pyenv")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
      × Failed to build `pyenv==0.0.1`
      ├─▶ The build backend returned an error
      ╰─▶ Call to `setuptools.build_meta:__legacy__.build_wheel` failed (exit status: 1)

          [stdout]
          running bdist_wheel
          running build
          installing to build/bdist.linux-x86_64/wheel
          running install

          [stderr]
          # NOTE #
          We are sorry, but this package is not installable with pip.

          Please read the installation instructions at:
     
          https://github.com/pyenv/pyenv#installation
          #


          hint: This usually indicates a problem with the package or the build environment.
    ");

    // Ensure the tool environment is not created.
    tool_dir.child("pyenv").assert(predicate::path::missing());
    bin_dir.child("pyenv").assert(predicate::path::missing());
}

/// Test installing a tool with a bare URL requirement.
#[test]
fn tool_install_unnamed_package() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("https://files.pythonhosted.org/packages/0f/89/294c9a6b6c75a08da55e9d05321d0707e9418735e3062b12ef0f54c33474/black-24.4.2-py3-none-any.whl")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 6 packages in [TIME]
    Installed 6 packages in [TIME]
     + black==24.4.2 (from https://files.pythonhosted.org/packages/0f/89/294c9a6b6c75a08da55e9d05321d0707e9418735e3062b12ef0f54c33474/black-24.4.2-py3-none-any.whl)
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    Installed 2 executables: black, blackd
    ");

    tool_dir.child("black").assert(predicate::path::is_dir());
    tool_dir
        .child("black")
        .child("uv-receipt.toml")
        .assert(predicate::path::exists());

    let executable = bin_dir.child(format!("black{}", std::env::consts::EXE_SUFFIX));
    assert!(executable.exists());

    // On Windows, we can't snapshot an executable file.
    #[cfg(not(windows))]
    insta::with_settings!({
        filters => context.filters(),
    }, {
        // Should run black in the virtual environment
        assert_snapshot!(fs_err::read_to_string(executable).unwrap(), @r#"
        #![TEMP_DIR]/tools/black/bin/python
        # -*- coding: utf-8 -*-
        import sys
        from black import patched_main
        if __name__ == "__main__":
            if sys.argv[0].endswith("-script.pyw"):
                sys.argv[0] = sys.argv[0][:-11]
            elif sys.argv[0].endswith(".exe"):
                sys.argv[0] = sys.argv[0][:-4]
            sys.exit(patched_main())
        "#);

    });

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        [tool]
        requirements = [{ name = "black", url = "https://files.pythonhosted.org/packages/0f/89/294c9a6b6c75a08da55e9d05321d0707e9418735e3062b12ef0f54c33474/black-24.4.2-py3-none-any.whl" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    uv_snapshot!(context.filters(), Command::new("black").arg("--version").env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    black, 24.4.2 (compiled: no)
    Python (CPython) 3.12.[X]

    ----- stderr -----
    ");
}

/// Test installing a tool with a Git requirement.
#[test]
#[cfg(feature = "test-git")]
fn tool_install_git() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");
    let mut paths = BTreeSet::new();

    // Avoid removing `git` from PATH
    let git_path = which::which("git")
        .expect("Failed to find `git` executable.")
        .parent()
        .expect("Failed to find `git` executable directory.")
        .to_path_buf();
    paths.insert(bin_dir.to_path_buf());
    paths.insert(git_path);
    // Git Submodule in macos seems to rely on `sed`.
    if cfg!(target_os = "macos") {
        let sed_path = which::which("sed")
            .expect("Failed to find `sed` executable.")
            .parent()
            .expect("Failed to find `sed` executable directory.")
            .to_path_buf();
        paths.insert(sed_path);
    }
    let path = std::env::join_paths(paths).unwrap();

    // Unnamed Git Install
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("git+https://github.com/psf/black@24.2.0")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, path.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 6 packages in [TIME]
     + black==24.2.0 (from git+https://github.com/psf/black@6fdf8a4af28071ed1d079c01122b34c5d587207a)
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    Installed 2 executables: black, blackd
    ");

    tool_dir.child("black").assert(predicate::path::is_dir());
    tool_dir
        .child("black")
        .child("uv-receipt.toml")
        .assert(predicate::path::exists());

    let executable = bin_dir.child(format!("black{}", std::env::consts::EXE_SUFFIX));
    assert!(executable.exists());

    fs_err::remove_dir_all(&bin_dir).expect("Failed to remove bin dir.");
    fs_err::remove_dir_all(&tool_dir).expect("Failed to remove tool dir.");

    // Named Git Install
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black @ git+https://github.com/psf/black@24.2.0")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, path.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Installed 6 packages in [TIME]
     + black==24.2.0 (from git+https://github.com/psf/black@6fdf8a4af28071ed1d079c01122b34c5d587207a)
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    Installed 2 executables: black, blackd
    ");

    tool_dir.child("black").assert(predicate::path::is_dir());
    tool_dir
        .child("black")
        .child("uv-receipt.toml")
        .assert(predicate::path::exists());

    let executable = bin_dir.child(format!("black{}", std::env::consts::EXE_SUFFIX));
    assert!(executable.exists());
}

/// Test installing a tool with a Git LFS enabled requirement.
#[test]
#[cfg(feature = "test-git-lfs")]
fn tool_install_git_lfs() {
    let context = uv_test::test_context!("3.13")
        .with_filtered_exe_suffix()
        .with_git_lfs_config();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");
    let mut paths = BTreeSet::new();

    // Avoid removing `git` or `git-lfs` from PATH
    let git_path = which::which("git")
        .expect("Failed to find `git` executable.")
        .parent()
        .expect("Failed to find `git` executable directory.")
        .to_path_buf();
    let git_lfs_path = which::which("git-lfs")
        .expect("Failed to find `git-lfs` executable.")
        .parent()
        .expect("Failed to find `git-lfs` executable directory.")
        .to_path_buf();
    paths.insert(bin_dir.to_path_buf());
    paths.insert(git_path);
    paths.insert(git_lfs_path);
    // Git LFS filter-process in macos seems to rely on `sh`.
    // Git Submodule in macos seems to rely on `sed`.
    if cfg!(target_os = "macos") {
        for bin_path in ["sh", "sed"].into_iter().map(|name| {
            which::which(name)
                .unwrap_or_else(|_| panic!("Failed to find `{name}` executable."))
                .parent()
                .unwrap_or_else(|| panic!("Failed to find `{name}` executable directory."))
                .to_path_buf()
        }) {
            paths.insert(bin_path);
        }
    }
    let path = std::env::join_paths(paths).unwrap();

    // Verify a successful LFS request
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("--lfs")
        .arg("test-lfs-repo @ git+https://github.com/astral-sh/test-lfs-repo@54e5eebd3c6851b1353fc7b1e5b4eca11e27581c")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, path.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + test-lfs-repo==0.1.0 (from git+https://github.com/astral-sh/test-lfs-repo@54e5eebd3c6851b1353fc7b1e5b4eca11e27581c#lfs=true)
    Installed 2 executables: test-lfs-repo, test-lfs-repo-assets
    ");

    tool_dir
        .child("test-lfs-repo")
        .assert(predicate::path::is_dir());
    tool_dir
        .child("test-lfs-repo")
        .child("uv-receipt.toml")
        .assert(predicate::path::exists());

    let executable = bin_dir.child(format!("test-lfs-repo{}", std::env::consts::EXE_SUFFIX));
    assert!(executable.exists());

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("test-lfs-repo").join("uv-receipt.toml")).unwrap(), @r#"
        [tool]
        requirements = [{ name = "test-lfs-repo", git = "https://github.com/astral-sh/test-lfs-repo?lfs=true&rev=54e5eebd3c6851b1353fc7b1e5b4eca11e27581c" }]
        entrypoints = [
            { name = "test-lfs-repo", install-path = "[TEMP_DIR]/bin/test-lfs-repo", from = "test-lfs-repo" },
            { name = "test-lfs-repo-assets", install-path = "[TEMP_DIR]/bin/test-lfs-repo-assets", from = "test-lfs-repo" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    uv_snapshot!(context.filters(), Command::new("test-lfs-repo").env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello from test-lfs-repo!

    ----- stderr -----
    ");

    uv_snapshot!(context.filters(), Command::new("test-lfs-repo-assets").env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello from test-lfs-repo! LFS_TEST=True ANOTHER_LFS_TEST=True

    ----- stderr -----
    ");

    // Attempt to install when LFS artifacts are missing and LFS is requested.

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

    uv_snapshot!(context.filters(), context.tool_install()
        .arg("--reinstall")
        .arg("--lfs")
        .arg("test-lfs-repo @ git+https://github.com/astral-sh/test-lfs-repo@54e5eebd3c6851b1353fc7b1e5b4eca11e27581c")
        .env(EnvVars::UV_INTERNAL__TEST_LFS_DISABLED, "1")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, path.as_os_str()), @"
    success: false
    exit_code: [ERROR_CODE]
    ----- stdout -----

    ----- stderr -----
    [PREFIX]The source distribution `[DISTRIBUTION]` is missing Git LFS artifacts
    ");

    // Attempt to install when LFS artifacts are missing but LFS was not requested.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("test-lfs-repo @ git+https://github.com/astral-sh/test-lfs-repo@54e5eebd3c6851b1353fc7b1e5b4eca11e27581c")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, path.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - test-lfs-repo==0.1.0 (from git+https://github.com/astral-sh/test-lfs-repo@54e5eebd3c6851b1353fc7b1e5b4eca11e27581c#lfs=true)
     + test-lfs-repo==0.1.0 (from git+https://github.com/astral-sh/test-lfs-repo@54e5eebd3c6851b1353fc7b1e5b4eca11e27581c)
    Installed 2 executables: test-lfs-repo, test-lfs-repo-assets
    ");

    #[cfg(not(windows))]
    uv_snapshot!(context.filters(), Command::new("test-lfs-repo-assets").env(EnvVars::PATH, bin_dir.as_os_str()), @r#"
    success: false
    exit_code: [ERROR_CODE]
    ----- stdout -----

    ----- stderr -----
    Traceback (most recent call last):
      File "[TEMP_DIR]/bin/test-lfs-repo-assets", line 10, in <module>
        sys.exit(main_lfs())
                 ~~~~~~~~^^
      File "[TEMP_DIR]/tools/test-lfs-repo/[PYTHON-LIB]/site-packages/test_lfs_repo/__init__.py", line 5, in main_lfs
        from .lfs_module import LFS_TEST
      File "[TEMP_DIR]/tools/test-lfs-repo/[PYTHON-LIB]/site-packages/test_lfs_repo/lfs_module.py", line 1
        version https://git-lfs.github.com/spec/v1
                ^^^^^
    SyntaxError: invalid syntax
    "#);

    #[cfg(windows)]
    uv_snapshot!(context.filters(), Command::new("test-lfs-repo-assets").env(EnvVars::PATH, bin_dir.as_os_str()), @r#"
    success: false
    exit_code: [ERROR_CODE]
    ----- stdout -----

    ----- stderr -----
    Traceback (most recent call last):
      File "<frozen runpy>", line 198, in _run_module_as_main
      File "<frozen runpy>", line 88, in _run_code
      File "[TEMP_DIR]/bin/test-lfs-repo-assets/__main__.py", line 10, in <module>
        sys.exit(main_lfs())
                 ~~~~~~~~^^
      File "[TEMP_DIR]/tools/test-lfs-repo/[PYTHON-LIB]/site-packages/test_lfs_repo/__init__.py", line 5, in main_lfs
        from .lfs_module import LFS_TEST
      File "[TEMP_DIR]/tools/test-lfs-repo/[PYTHON-LIB]/site-packages/test_lfs_repo/lfs_module.py", line 1
        version https://git-lfs.github.com/spec/v1
                ^^^^^
    SyntaxError: invalid syntax
    "#);
}

/// Test installing a tool with a bare URL requirement using `--from`, where the URL and the package
/// name conflict.
#[test]
fn tool_install_unnamed_conflict() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("--from")
        .arg("https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Package name (`iniconfig`) provided with `--from` does not match install request (`black`)
    ");
}

/// Test installing a tool with a bare URL requirement using `--from`.
#[test]
fn tool_install_unnamed_from() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("--from")
        .arg("https://files.pythonhosted.org/packages/0f/89/294c9a6b6c75a08da55e9d05321d0707e9418735e3062b12ef0f54c33474/black-24.4.2-py3-none-any.whl")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 6 packages in [TIME]
    Installed 6 packages in [TIME]
     + black==24.4.2 (from https://files.pythonhosted.org/packages/0f/89/294c9a6b6c75a08da55e9d05321d0707e9418735e3062b12ef0f54c33474/black-24.4.2-py3-none-any.whl)
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    Installed 2 executables: black, blackd
    ");

    tool_dir.child("black").assert(predicate::path::is_dir());
    tool_dir
        .child("black")
        .child("uv-receipt.toml")
        .assert(predicate::path::exists());

    let executable = bin_dir.child(format!("black{}", std::env::consts::EXE_SUFFIX));
    assert!(executable.exists());

    // On Windows, we can't snapshot an executable file.
    #[cfg(not(windows))]
    insta::with_settings!({
        filters => context.filters(),
    }, {
        // Should run black in the virtual environment
        assert_snapshot!(fs_err::read_to_string(executable).unwrap(), @r#"
        #![TEMP_DIR]/tools/black/bin/python
        # -*- coding: utf-8 -*-
        import sys
        from black import patched_main
        if __name__ == "__main__":
            if sys.argv[0].endswith("-script.pyw"):
                sys.argv[0] = sys.argv[0][:-11]
            elif sys.argv[0].endswith(".exe"):
                sys.argv[0] = sys.argv[0][:-4]
            sys.exit(patched_main())
        "#);

    });

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        [tool]
        requirements = [{ name = "black", url = "https://files.pythonhosted.org/packages/0f/89/294c9a6b6c75a08da55e9d05321d0707e9418735e3062b12ef0f54c33474/black-24.4.2-py3-none-any.whl" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    uv_snapshot!(context.filters(), Command::new("black").arg("--version").env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    black, 24.4.2 (compiled: no)
    Python (CPython) 3.12.[X]

    ----- stderr -----
    ");
}

/// Test installing a tool with a bare URL requirement using `--with`.
#[test]
fn tool_install_unnamed_with() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("--with")
        .arg("https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
    Prepared 7 packages in [TIME]
    Installed 7 packages in [TIME]
     + black==24.3.0
     + click==8.1.7
     + iniconfig==2.0.0 (from https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl)
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    Installed 2 executables: black, blackd
    ");

    tool_dir.child("black").assert(predicate::path::is_dir());
    tool_dir
        .child("black")
        .child("uv-receipt.toml")
        .assert(predicate::path::exists());

    let executable = bin_dir.child(format!("black{}", std::env::consts::EXE_SUFFIX));
    assert!(executable.exists());

    // On Windows, we can't snapshot an executable file.
    #[cfg(not(windows))]
    insta::with_settings!({
        filters => context.filters(),
    }, {
        // Should run black in the virtual environment
        assert_snapshot!(fs_err::read_to_string(executable).unwrap(), @r#"
        #![TEMP_DIR]/tools/black/bin/python
        # -*- coding: utf-8 -*-
        import sys
        from black import patched_main
        if __name__ == "__main__":
            if sys.argv[0].endswith("-script.pyw"):
                sys.argv[0] = sys.argv[0][:-11]
            elif sys.argv[0].endswith(".exe"):
                sys.argv[0] = sys.argv[0][:-4]
            sys.exit(patched_main())
        "#);

    });

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        [tool]
        requirements = [
            { name = "black" },
            { name = "iniconfig", url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl" },
        ]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    uv_snapshot!(context.filters(), Command::new("black").arg("--version").env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    black, 24.3.0 (compiled: yes)
    Python (CPython) 3.12.[X]

    ----- stderr -----
    ");
}

#[test]
fn tool_install_with_dependencies_from_script() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    let script = context.temp_dir.child("script.py");
    script.write_str(indoc! {r#"
        # /// script
        # requires-python = ">=3.11"
        # dependencies = [
        #   "anyio",
        # ]
        # ///

        import anyio
    "#})?;

    // script dependencies (anyio) are now installed.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("--with-requirements")
        .arg("script.py")
        .arg("black")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
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
    Installed 2 executables: black, blackd
    ");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        [tool]
        requirements = [
            { name = "black" },
            { name = "anyio" },
        ]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    // Update the script file.
    script.write_str(indoc! {r#"
        # /// script
        # requires-python = ">=3.11"
        # dependencies = [
        #   "anyio",
        #   "iniconfig",
        # ]
        # ///

        import anyio
    "#})?;

    // Install `black`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("--with-requirements")
        .arg("script.py")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + iniconfig==2.0.0
    Installed 2 executables: black, blackd
    ");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        [tool]
        requirements = [
            { name = "black" },
            { name = "anyio" },
            { name = "iniconfig" },
        ]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    Ok(())
}

/// Test installing a tool with additional requirements from a `requirements.txt` file.
#[test]
fn tool_install_requirements_txt() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("iniconfig").unwrap();

    // Install `black`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("--with-requirements")
        .arg("requirements.txt")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

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
    Installed 2 executables: black, blackd
    ");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        [tool]
        requirements = [
            { name = "black" },
            { name = "iniconfig" },
        ]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    // Update the `requirements.txt` file.
    requirements_txt.write_str("idna").unwrap();

    // Install `black`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("--with-requirements")
        .arg("requirements.txt")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Uninstalled [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + idna==3.6
     - iniconfig==2.0.0
    Installed 2 executables: black, blackd
    ");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        [tool]
        requirements = [
            { name = "black" },
            { name = "idna" },
        ]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });
}

/// Ignore and warn when (e.g.) the `--index-url` argument is a provided `requirements.txt`.
#[test]
fn tool_install_requirements_txt_arguments() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
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

    // Install `black`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("--with-requirements")
        .arg("requirements.txt")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: Ignoring `--index-url` from requirements file: `https://test.pypi.org/simple`. Instead, use the `--index-url` command-line argument, or set `index-url` in a `uv.toml` or `pyproject.toml` file.
    Resolved 7 packages in [TIME]
    Prepared 7 packages in [TIME]
    Installed 7 packages in [TIME]
     + black==24.3.0
     + click==8.1.7
     + idna==3.6
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    Installed 2 executables: black, blackd
    ");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        [tool]
        requirements = [
            { name = "black" },
            { name = "idna" },
        ]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    // Don't warn, though, if the index URL is the same as the default or as settings.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str(indoc! { r"
        --index-url https://pypi.org/simple
        idna
        "
        })
        .unwrap();

    // Install `black`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("--with-requirements")
        .arg("requirements.txt")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    `black` is already installed
    ");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str(indoc! { r"
        --index-url https://test.pypi.org/simple
        idna
        "
        })
        .unwrap();

    // Install `flask`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("flask")
        .arg("--with-requirements")
        .arg("requirements.txt")
        .arg("--index-url")
        .arg("https://test.pypi.org/simple")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 8 packages in [TIME]
    Prepared 8 packages in [TIME]
    Installed 8 packages in [TIME]
     + blinker==1.7.0
     + click==8.1.7
     + flask==3.0.2
     + idna==2.7
     + itsdangerous==2.1.2
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + werkzeug==3.0.1
    Installed 1 executable: flask
    ");
}

/// Test upgrading an already installed tool.
#[test]
fn tool_install_upgrade() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black==24.1.1")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

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
    Installed 2 executables: black, blackd
    ");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        [tool]
        requirements = [{ name = "black", specifier = "==24.1.1" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    // Install without the constraint. It should be replaced, but the package shouldn't be installed
    // since it's already satisfied in the environment.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Audited [N] packages in [TIME]
    Installed 2 executables: black, blackd
    ");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        [tool]
        requirements = [{ name = "black" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    // Install with a `with`. It should be added to the environment.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("--with")
        .arg("iniconfig @ https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + iniconfig==2.0.0 (from https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl)
    Installed 2 executables: black, blackd
    ");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        [tool]
        requirements = [
            { name = "black" },
            { name = "iniconfig", url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl" },
        ]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    // Install with `--upgrade`. `black` should be reinstalled with a more recent version, and
    // `iniconfig` should be removed.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("--upgrade")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Uninstalled [N] packages in [TIME]
    Installed [N] packages in [TIME]
     - black==24.1.1
     + black==24.3.0
     - iniconfig==2.0.0 (from https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl)
    Installed 2 executables: black, blackd
    ");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        [tool]
        requirements = [{ name = "black" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });
}

/// Test reinstalling tools with varying `--python` requests.
#[test]
fn tool_install_python_requests() {
    let context = uv_test::test_context_with_versions!(&["3.11", "3.12"])
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("-p")
        .arg("3.12")
        .arg("black")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

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
    Installed 2 executables: black, blackd
    ");

    // Install with Python 3.12 (compatible).
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("-p")
        .arg("3.12")
        .arg("black")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    `black` is already installed
    ");

    // // Install with Python 3.11 (incompatible).
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("-p")
        .arg("3.11")
        .arg("black")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Ignoring existing environment for `black`: the requested Python interpreter does not match the environment interpreter
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + black==24.3.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    Installed 2 executables: black, blackd
    ");
}

/// Test reinstalling tools with varying `--python` and
/// `--python-preference` parameters.
#[ignore = "https://github.com/astral-sh/uv/issues/7473"]
#[test]
fn tool_install_python_preference() {
    let context = uv_test::test_context_with_versions!(&["3.11", "3.12"])
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("-p")
        .arg("3.12")
        .arg("black")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

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
    Installed 2 executables: black, blackd
    "###);

    // Install with Python 3.12 (compatible).
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("-p")
        .arg("3.12")
        .arg("black")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    `black` is already installed
    "###);

    // Install with system Python 3.11 (different version, incompatible).
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("-p")
        .arg("3.11")
        .arg("--python-preference")
        .arg("only-system")
        .arg("black")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Ignoring existing environment for `black`: the requested Python interpreter does not match the environment interpreter
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + black==24.3.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    Installed 2 executables: black, blackd
    "###);

    // Install with system Python 3.11 (compatible).
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("-p")
        .arg("3.11")
        .arg("--python-preference")
        .arg("only-system")
        .arg("black")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    `black` is already installed
    "###);

    // Install with managed Python 3.11 (different source, incompatible).
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("-p")
        .arg("3.11")
        .arg("--python-preference")
        .arg("only-managed")
        .arg("black")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Ignoring existing environment for `black`: the requested Python interpreter does not match the environment interpreter
    Resolved [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + black==24.3.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    Installed 2 executables: black, blackd
    "###);

    // Install with managed Python 3.11 (compatible).
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("-p")
        .arg("3.11")
        .arg("--python-preference")
        .arg("only-managed")
        .arg("black")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    `black` is already installed
    "###);
}

/// Test preserving a tool environment when new but incompatible requirements are requested.
#[test]
fn tool_install_preserve_environment() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black==24.1.1")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

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
    Installed 2 executables: black, blackd
    ");

    // Install `black`, but with an incompatible requirement.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black==24.1.1")
        .arg("--with")
        .arg("packaging==0.0.1")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because black==24.1.1 depends on packaging>=22.0 and you require black==24.1.1, we can conclude that you require packaging>=22.0.
          And because you require packaging==0.0.1, we can conclude that your requirements are unsatisfiable.
    ");

    // Install `black`. The tool should already be installed, since we didn't remove the environment.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black==24.1.1")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    `black==24.1.1` is already installed
    ");
}

/// Test warning when the binary directory is not on the user's PATH.
#[test]
#[cfg(unix)]
fn tool_install_warn_path() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black==24.1.1")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env_remove(EnvVars::PATH), @r#"
    success: true
    exit_code: 0
    ----- stdout -----

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
    Installed 2 executables: black, blackd
    warning: `[TEMP_DIR]/bin` is not on your PATH. To use installed tools, run `export PATH="[TEMP_DIR]/bin:$PATH"` or `uv tool update-shell`.
    "#);
}

/// Test installing and reinstalling with an invalid receipt.
#[test]
fn tool_install_bad_receipt() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

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
    Installed 2 executables: black, blackd
    ");

    tool_dir
        .child("black")
        .child("uv-receipt.toml")
        .assert(predicate::path::exists());

    // Override the `uv-receipt.toml` file with an invalid receipt.
    tool_dir
        .child("black")
        .child("uv-receipt.toml")
        .write_str("invalid")?;

    // Reinstall `black`, which should remove the invalid receipt.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: Removed existing `black` with invalid receipt
    Resolved [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + black==24.3.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    Installed 2 executables: black, blackd
    ");

    Ok(())
}

/// Test installing a tool with a malformed `.dist-info` directory (i.e., a `.dist-info` directory
/// that isn't properly normalized).
#[test]
fn tool_install_malformed_dist_info() {
    let context = uv_test::test_context!("3.12")
        .with_exclude_newer("2025-01-18T00:00:00Z")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `executable-application`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("executable-application")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + executable-application==0.3.0
    Installed 1 executable: app
    ");

    tool_dir
        .child("executable-application")
        .assert(predicate::path::is_dir());
    tool_dir
        .child("executable-application")
        .child("uv-receipt.toml")
        .assert(predicate::path::exists());

    let executable = bin_dir.child(format!("app{}", std::env::consts::EXE_SUFFIX));
    assert!(executable.exists());

    // On Windows, we can't snapshot an executable file.
    #[cfg(not(windows))]
    insta::with_settings!({
        filters => context.filters(),
    }, {
        // Should run black in the virtual environment
        assert_snapshot!(fs_err::read_to_string(executable).unwrap(), @r#"
        #![TEMP_DIR]/tools/executable-application/bin/python
        # -*- coding: utf-8 -*-
        import sys
        from executable_application import main
        if __name__ == "__main__":
            if sys.argv[0].endswith("-script.pyw"):
                sys.argv[0] = sys.argv[0][:-11]
            elif sys.argv[0].endswith(".exe"):
                sys.argv[0] = sys.argv[0][:-4]
            sys.exit(main())
        "#);

    });

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("executable-application").join("uv-receipt.toml")).unwrap(), @r#"
        [tool]
        requirements = [{ name = "executable-application" }]
        entrypoints = [
            { name = "app", install-path = "[TEMP_DIR]/bin/app", from = "executable-application" },
        ]

        [tool.options]
        exclude-newer = "2025-01-18T00:00:00Z"
        "#);
    });
}

/// Test installing, then re-installing with different settings.
#[test]
fn tool_install_settings() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("flask>=3")
        .arg("--resolution=lowest-direct")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + blinker==1.7.0
     + click==8.1.7
     + flask==3.0.0
     + itsdangerous==2.1.2
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + werkzeug==3.0.1
    Installed 1 executable: flask
    ");

    tool_dir.child("flask").assert(predicate::path::is_dir());
    tool_dir
        .child("flask")
        .child("uv-receipt.toml")
        .assert(predicate::path::exists());

    let executable = bin_dir.child(format!("flask{}", std::env::consts::EXE_SUFFIX));
    assert!(executable.exists());

    // On Windows, we can't snapshot an executable file.
    #[cfg(not(windows))]
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(fs_err::read_to_string(executable).unwrap(), @r#"
        #![TEMP_DIR]/tools/flask/bin/python
        # -*- coding: utf-8 -*-
        import sys
        from flask.cli import main
        if __name__ == "__main__":
            if sys.argv[0].endswith("-script.pyw"):
                sys.argv[0] = sys.argv[0][:-11]
            elif sys.argv[0].endswith(".exe"):
                sys.argv[0] = sys.argv[0][:-4]
            sys.exit(main())
        "#);

    });

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("flask").join("uv-receipt.toml")).unwrap(), @r#"
        [tool]
        requirements = [{ name = "flask", specifier = ">=3" }]
        entrypoints = [
            { name = "flask", install-path = "[TEMP_DIR]/bin/flask", from = "flask" },
        ]

        [tool.options]
        resolution = "lowest-direct"
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    // Reinstall with `highest`. This is a no-op, since we _do_ have a compatible version installed.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("flask>=3")
        .arg("--resolution=highest")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    `flask>=3` is already installed
    ");

    // It should update the receipt though.
    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("flask").join("uv-receipt.toml")).unwrap(), @r#"
        [tool]
        requirements = [{ name = "flask", specifier = ">=3" }]
        entrypoints = [
            { name = "flask", install-path = "[TEMP_DIR]/bin/flask", from = "flask" },
        ]

        [tool.options]
        resolution = "highest"
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    // Reinstall with `highest` and `--upgrade`. This should change the setting and install a higher
    // version.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("flask>=3")
        .arg("--resolution=highest")
        .arg("--upgrade")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Uninstalled [N] packages in [TIME]
    Installed [N] packages in [TIME]
     - flask==3.0.0
     + flask==3.0.2
    Installed 1 executable: flask
    ");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("flask").join("uv-receipt.toml")).unwrap(), @r#"
        [tool]
        requirements = [{ name = "flask", specifier = ">=3" }]
        entrypoints = [
            { name = "flask", install-path = "[TEMP_DIR]/bin/flask", from = "flask" },
        ]

        [tool.options]
        resolution = "highest"
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });
}

/// Test installing a tool with `uv tool install {package}@{version}`.
#[test]
fn tool_install_at_version() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black` at `24.1.0`.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black@24.1.0")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + black==24.1.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    Installed 2 executables: black, blackd
    ");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        [tool]
        requirements = [{ name = "black", specifier = "==24.1.0" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    // Combining `{package}@{version}` with a `--from` should fail (even if they're ultimately
    // compatible).
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black@24.1.0")
        .arg("--from")
        .arg("black==24.1.0")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Package requirement (`black==24.1.0`) provided with `--from` conflicts with install request (`black@24.1.0`)
    ");
}

/// Test installing a tool with `uv tool install {package}@latest`.
#[test]
fn tool_install_at_latest() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black` at latest.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black@latest")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

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
    Installed 2 executables: black, blackd
    ");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        [tool]
        requirements = [{ name = "black" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });
}

/// Test installing a tool with `uv tool install {package} --from {package}@latest`.
#[test]
fn tool_install_from_at_latest() {
    let context = uv_test::test_context!("3.12")
        .with_exclude_newer("2025-01-18T00:00:00Z")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    uv_snapshot!(context.filters(), context.tool_install()
        .arg("app")
        .arg("--from")
        .arg("executable-application@latest")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + executable-application==0.3.0
    Installed 1 executable: app
    ");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("executable-application").join("uv-receipt.toml")).unwrap(), @r#"
        [tool]
        requirements = [{ name = "executable-application" }]
        entrypoints = [
            { name = "app", install-path = "[TEMP_DIR]/bin/app", from = "executable-application" },
        ]

        [tool.options]
        exclude-newer = "2025-01-18T00:00:00Z"
        "#);
    });
}

/// Test installing a tool with `uv tool install {package} --from {package}@{version}`.
#[test]
fn tool_install_from_at_version() {
    let context = uv_test::test_context!("3.12")
        .with_exclude_newer("2025-01-18T00:00:00Z")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    uv_snapshot!(context.filters(), context.tool_install()
        .arg("app")
        .arg("--from")
        .arg("executable-application@0.2.0")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + executable-application==0.2.0
    Installed 1 executable: app
    ");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("executable-application").join("uv-receipt.toml")).unwrap(), @r#"
        [tool]
        requirements = [{ name = "executable-application", specifier = "==0.2.0" }]
        entrypoints = [
            { name = "app", install-path = "[TEMP_DIR]/bin/app", from = "executable-application" },
        ]

        [tool.options]
        exclude-newer = "2025-01-18T00:00:00Z"
        "#);
    });
}

/// Test upgrading an already installed tool via `{package}@{latest}`.
#[test]
fn tool_install_at_latest_upgrade() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black==24.1.1")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

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
    Installed 2 executables: black, blackd
    ");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        [tool]
        requirements = [{ name = "black", specifier = "==24.1.1" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    // Install without the constraint. It should be replaced, but the package shouldn't be installed
    // since it's already satisfied in the environment.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Audited [N] packages in [TIME]
    Installed 2 executables: black, blackd
    ");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        [tool]
        requirements = [{ name = "black" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    // Install with `{package}@{latest}`. `black` should be reinstalled with a more recent version.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black@latest")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Uninstalled [N] packages in [TIME]
    Installed [N] packages in [TIME]
     - black==24.1.1
     + black==24.3.0
    Installed 2 executables: black, blackd
    ");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        [tool]
        requirements = [{ name = "black" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });
}

/// Install a tool with `--constraints`.
#[test]
fn tool_install_constraints() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    let constraints_txt = context.temp_dir.child("constraints.txt");
    constraints_txt.write_str(indoc::indoc! {r"
        mypy-extensions<1
        anyio>=3
    "})?;

    // Install `black`.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("--constraints")
        .arg(constraints_txt.as_os_str())
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + black==24.3.0
     + click==8.1.7
     + mypy-extensions==0.4.4
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    Installed 2 executables: black, blackd
    ");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        [tool]
        requirements = [{ name = "black" }]
        constraints = [
            { name = "mypy-extensions", specifier = "<1" },
            { name = "anyio", specifier = ">=3" },
        ]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    // Installing with the same constraints should be a no-op.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("--constraints")
        .arg(constraints_txt.as_os_str())
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    `black` is already installed
    ");

    let constraints_txt = context.temp_dir.child("constraints.txt");
    constraints_txt.write_str(indoc::indoc! {r"
        platformdirs<4
    "})?;

    // Installing with revised constraints should reinstall the tool.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("--constraints")
        .arg(constraints_txt.as_os_str())
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Uninstalled [N] packages in [TIME]
    Installed [N] packages in [TIME]
     - platformdirs==4.2.0
     + platformdirs==3.11.0
    Installed 2 executables: black, blackd
    ");

    Ok(())
}

/// Install a tool with `--overrides`.
#[test]
fn tool_install_overrides() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    let overrides_txt = context.temp_dir.child("overrides.txt");
    overrides_txt.write_str(indoc::indoc! {r"
        click<8
        anyio>=3
    "})?;

    // Install `black`.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("--overrides")
        .arg(overrides_txt.as_os_str())
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + black==24.3.0
     + click==7.1.2
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    Installed 2 executables: black, blackd
    ");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        [tool]
        requirements = [{ name = "black" }]
        overrides = [
            { name = "click", specifier = "<8" },
            { name = "anyio", specifier = ">=3" },
        ]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    Ok(())
}

/// `uv tool install python` is not allowed
#[test]
fn tool_install_python() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `python`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("python")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Cannot install Python with `uv tool install`. Did you mean to use `uv python install`?
    ");

    // Install `python@<version>`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("python@3.12")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Cannot install Python with `uv tool install`. Did you mean to use `uv python install`?
    ");
}

#[test]
fn tool_install_mismatched_name() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("--from")
        .arg("https://files.pythonhosted.org/packages/af/47/93213ee66ef8fae3b93b3e29206f6b251e65c97bd91d8e1c5596ef15af0a/flask-3.1.0-py3-none-any.whl")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Package name (`flask`) provided with `--from` does not match install request (`black`)
    ");

    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("--from")
        .arg("flask @ https://files.pythonhosted.org/packages/af/47/93213ee66ef8fae3b93b3e29206f6b251e65c97bd91d8e1c5596ef15af0a/flask-3.1.0-py3-none-any.whl")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Package name (`flask`) provided with `--from` does not match install request (`black`)
    ");

    uv_snapshot!(context.filters(), context.tool_install()
        .arg("flask")
        .arg("--from")
        .arg("black @ https://files.pythonhosted.org/packages/af/47/93213ee66ef8fae3b93b3e29206f6b251e65c97bd91d8e1c5596ef15af0a/flask-3.1.0-py3-none-any.whl")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Package name (`black`) provided with `--from` does not match install request (`flask`)
    ");
}

/// When installing from an authenticated index, the credentials should be omitted from the receipt.
#[tokio::test]
async fn tool_install_credentials() {
    let proxy = crate::pypi_proxy::start().await;
    let context = uv_test::test_context!("3.12")
        .with_exclude_newer("2025-01-18T00:00:00Z")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `executable-application`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("executable-application")
         .arg("--index")
        .arg(proxy.authenticated_url("public", "heron", "/basic-auth/simple"))
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + executable-application==0.3.0
    Installed 1 executable: app
    ");

    tool_dir
        .child("executable-application")
        .assert(predicate::path::is_dir());
    tool_dir
        .child("executable-application")
        .child("uv-receipt.toml")
        .assert(predicate::path::exists());

    let executable = bin_dir.child(format!("app{}", std::env::consts::EXE_SUFFIX));
    assert!(executable.exists());

    // On Windows, we can't snapshot an executable file.
    #[cfg(not(windows))]
    insta::with_settings!({
        filters => context.filters(),
    }, {
        // Should run black in the virtual environment
        assert_snapshot!(fs_err::read_to_string(executable).unwrap(), @r#"
        #![TEMP_DIR]/tools/executable-application/bin/python
        # -*- coding: utf-8 -*-
        import sys
        from executable_application import main
        if __name__ == "__main__":
            if sys.argv[0].endswith("-script.pyw"):
                sys.argv[0] = sys.argv[0][:-11]
            elif sys.argv[0].endswith(".exe"):
                sys.argv[0] = sys.argv[0][:-4]
            sys.exit(main())
        "#);

    });

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("executable-application").join("uv-receipt.toml")).unwrap(), @r#"
        [tool]
        requirements = [{ name = "executable-application" }]
        entrypoints = [
            { name = "app", install-path = "[TEMP_DIR]/bin/app", from = "executable-application" },
        ]

        [tool.options]
        index = [{ url = "http://[LOCALHOST]/basic-auth/simple", explicit = false, default = false, format = "simple", authenticate = "auto" }]
        exclude-newer = "2025-01-18T00:00:00Z"
        "#);
    });
}

/// When installing from an authenticated index, the credentials should be omitted from the receipt.
#[tokio::test]
async fn tool_install_default_credentials() -> Result<()> {
    let proxy = crate::pypi_proxy::start().await;
    let context = uv_test::test_context!("3.12")
        .with_exclude_newer("2025-01-18T00:00:00Z")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Write a `uv.toml` with a default index that has credentials.
    let uv_toml = context.temp_dir.child("uv.toml");
    uv_toml.write_str(&format!(
        indoc::indoc! {r#"
        [[index]]
        url = "{}"
        default = true
        authenticate = "always"
    "#},
        proxy.authenticated_url("public", "heron", "/basic-auth/simple")
    ))?;

    // Install `executable-application`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("executable-application")
        .arg("--config-file")
        .arg(uv_toml.as_os_str())
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + executable-application==0.3.0
    Installed 1 executable: app
    ");

    tool_dir
        .child("executable-application")
        .assert(predicate::path::is_dir());
    tool_dir
        .child("executable-application")
        .child("uv-receipt.toml")
        .assert(predicate::path::exists());

    let executable = bin_dir.child(format!("app{}", std::env::consts::EXE_SUFFIX));
    assert!(executable.exists());

    // On Windows, we can't snapshot an executable file.
    #[cfg(not(windows))]
    insta::with_settings!({
        filters => context.filters(),
    }, {
        // Should run black in the virtual environment
        assert_snapshot!(fs_err::read_to_string(executable).unwrap(), @r#"
        #![TEMP_DIR]/tools/executable-application/bin/python
        # -*- coding: utf-8 -*-
        import sys
        from executable_application import main
        if __name__ == "__main__":
            if sys.argv[0].endswith("-script.pyw"):
                sys.argv[0] = sys.argv[0][:-11]
            elif sys.argv[0].endswith(".exe"):
                sys.argv[0] = sys.argv[0][:-4]
            sys.exit(main())
        "#);
    });

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("executable-application").join("uv-receipt.toml")).unwrap(), @r#"
        [tool]
        requirements = [{ name = "executable-application" }]
        entrypoints = [
            { name = "app", install-path = "[TEMP_DIR]/bin/app", from = "executable-application" },
        ]

        [tool.options]
        index = [{ url = "http://[LOCALHOST]/basic-auth/simple", explicit = false, default = true, format = "simple", authenticate = "always" }]
        exclude-newer = "2025-01-18T00:00:00Z"
        "#);
    });

    // Attempt to upgrade without providing the credentials (from the config file).
    uv_snapshot!(context.filters(), context.tool_upgrade()
        .arg("executable-application")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    error: Failed to upgrade executable-application
      Caused by: Failed to fetch: `http://[LOCALHOST]/basic-auth/simple/executable-application/`
      Caused by: Missing credentials for http://[LOCALHOST]/basic-auth/simple/executable-application/
    ");

    // Attempt to upgrade.
    uv_snapshot!(context.filters(), context.tool_upgrade()
        .arg("executable-application")
        .arg("--config-file")
        .arg(uv_toml.as_os_str())
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Nothing to upgrade
    ");

    Ok(())
}

/// Test installing a tool with `--with-executables-from`.
#[test]
fn tool_install_with_executables_from() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    uv_snapshot!(context.filters(), context.tool_install()
        .arg("--with-executables-from")
        .arg("ansible-core,black")
        .arg("ansible==9.3.0")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + ansible==9.3.0
     + ansible-core==2.16.4
     + black==24.3.0
     + cffi==1.16.0
     + click==8.1.7
     + cryptography==42.0.5
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
     + pycparser==2.21
     + pyyaml==6.0.1
     + resolvelib==1.0.1
    Installed 11 executables from `ansible-core`: ansible, ansible-config, ansible-connection, ansible-console, ansible-doc, ansible-galaxy, ansible-inventory, ansible-playbook, ansible-pull, ansible-test, ansible-vault
    Installed 2 executables from `black`: black, blackd
    Installed 1 executable: ansible-community
    ");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("ansible").join("uv-receipt.toml")).unwrap(), @r#"
        [tool]
        requirements = [
            { name = "ansible", specifier = "==9.3.0" },
            { name = "ansible-core" },
            { name = "black" },
        ]
        entrypoints = [
            { name = "ansible", install-path = "[TEMP_DIR]/bin/ansible", from = "ansible-core" },
            { name = "ansible-community", install-path = "[TEMP_DIR]/bin/ansible-community", from = "ansible" },
            { name = "ansible-config", install-path = "[TEMP_DIR]/bin/ansible-config", from = "ansible-core" },
            { name = "ansible-connection", install-path = "[TEMP_DIR]/bin/ansible-connection", from = "ansible-core" },
            { name = "ansible-console", install-path = "[TEMP_DIR]/bin/ansible-console", from = "ansible-core" },
            { name = "ansible-doc", install-path = "[TEMP_DIR]/bin/ansible-doc", from = "ansible-core" },
            { name = "ansible-galaxy", install-path = "[TEMP_DIR]/bin/ansible-galaxy", from = "ansible-core" },
            { name = "ansible-inventory", install-path = "[TEMP_DIR]/bin/ansible-inventory", from = "ansible-core" },
            { name = "ansible-playbook", install-path = "[TEMP_DIR]/bin/ansible-playbook", from = "ansible-core" },
            { name = "ansible-pull", install-path = "[TEMP_DIR]/bin/ansible-pull", from = "ansible-core" },
            { name = "ansible-test", install-path = "[TEMP_DIR]/bin/ansible-test", from = "ansible-core" },
            { name = "ansible-vault", install-path = "[TEMP_DIR]/bin/ansible-vault", from = "ansible-core" },
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    uv_snapshot!(context.filters(), context.tool_uninstall()
        .arg("ansible")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Uninstalled 14 executables: ansible, ansible-community, ansible-config, ansible-connection, ansible-console, ansible-doc, ansible-galaxy, ansible-inventory, ansible-playbook, ansible-pull, ansible-test, ansible-vault, black, blackd
    ");
}

/// Test installing a tool with `--with-executables-from`, but the package has no entrypoints.
#[test]
fn tool_install_with_executables_from_no_entrypoints() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Try to install flask with executables from requests (which has no executables)
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("--with-executables-from")
        .arg("requests")
        .arg("flask")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    No executables are provided by package `requests`
    hint: Use `--with requests` to include `requests` as a dependency without installing its executables.

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + blinker==1.7.0
     + certifi==2024.2.2
     + charset-normalizer==3.3.2
     + click==8.1.7
     + flask==3.0.2
     + idna==3.6
     + itsdangerous==2.1.2
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + requests==2.31.0
     + urllib3==2.2.1
     + werkzeug==3.0.1
    Installed 1 executable: flask
    ");
}

#[test]
fn tool_install_find_links() {
    let context = uv_test::test_context!("3.13").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Run with `--find-links`.
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--find-links")
        .arg(context.workspace_root.join("test/links/"))
        .arg("basic-app")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello from basic-app!

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + basic-app==0.1.0
    ");

    // Install with `--find-links`.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("--find-links")
        .arg(context.workspace_root.join("test/links/"))
        .arg("basic-app")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Installed 1 package in [TIME]
     + basic-app==0.1.0
    Installed 1 executable: basic-app
    ");

    tool_dir
        .child("basic-app")
        .assert(predicate::path::is_dir());
    tool_dir
        .child("basic-app")
        .child("uv-receipt.toml")
        .assert(predicate::path::exists());

    let executable = bin_dir.child(format!("basic-app{}", std::env::consts::EXE_SUFFIX));
    assert!(executable.exists());

    // On Windows, we can't snapshot an executable file.
    #[cfg(not(windows))]
    insta::with_settings!({
        filters => context.filters(),
    }, {
        // Should run basic-app in the virtual environment
        assert_snapshot!(fs_err::read_to_string(executable).unwrap(), @r#"
        #![TEMP_DIR]/tools/basic-app/bin/python
        # -*- coding: utf-8 -*-
        import sys
        from basic_app import main
        if __name__ == "__main__":
            if sys.argv[0].endswith("-script.pyw"):
                sys.argv[0] = sys.argv[0][:-11]
            elif sys.argv[0].endswith(".exe"):
                sys.argv[0] = sys.argv[0][:-4]
            sys.exit(main())
        "#);
    });

    // Run the installed version with `--find-links` on the CLI again.
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--offline")
        .arg("--find-links")
        .arg(context.workspace_root.join("test/links/"))
        .arg("basic-app")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello from basic-app!

    ----- stderr -----
    ");

    // Run the installed version without `--find-links`.
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--offline")
        .arg("basic-app")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving tool dependencies:
      ╰─▶ Because only basic-app==0.1 is available and basic-app==0.1 needs to be downloaded from a registry, we can conclude that all versions of basic-app cannot be used.
          And because you require basic-app, we can conclude that your requirements are unsatisfiable.

          hint: Packages were unavailable because the network was disabled. When the network is disabled, registry packages may only be read from the cache.
    ");
}

#[test]
fn tool_install_python_platform() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black` for macos.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("--python-platform")
        .arg("macos")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

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
    Installed 2 executables: black, blackd
    ");

    // Install `black` for Linux.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("--python-platform")
        .arg("linux")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Uninstalled [N] packages in [TIME]
    Installed [N] packages in [TIME]
     ~ black==24.3.0
    Installed 2 executables: black, blackd
    ");
}
