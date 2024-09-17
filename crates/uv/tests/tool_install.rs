#![cfg(all(feature = "python", feature = "pypi"))]

use std::process::Command;

use anyhow::Result;
use assert_fs::{
    assert::PathAssert,
    fixture::{FileTouch, FileWriteStr, PathChild},
};
use indoc::indoc;
use insta::assert_snapshot;
use predicates::prelude::predicate;

use common::{uv_snapshot, TestContext};

mod common;

/// Test installing a tool with `uv tool install`
#[test]
fn tool_install() {
    let context = TestContext::new("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
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
        assert_snapshot!(fs_err::read_to_string(executable).unwrap(), @r###"
        #![TEMP_DIR]/tools/black/bin/python
        # -*- coding: utf-8 -*-
        import re
        import sys
        from black import patched_main
        if __name__ == "__main__":
            sys.argv[0] = re.sub(r"(-script\.pyw|\.exe)?$", "", sys.argv[0])
            sys.exit(patched_main())
        "###);

    });

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r###"
        [tool]
        requirements = [{ name = "black" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "###);
    });

    uv_snapshot!(context.filters(), Command::new("black").arg("--version").env("PATH", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    black, 24.3.0 (compiled: yes)
    Python (CPython) 3.12.[X]

    ----- stderr -----
    "###);

    // Install another tool
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("flask")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
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
    "###);

    tool_dir.child("flask").assert(predicate::path::is_dir());
    assert!(bin_dir
        .child(format!("flask{}", std::env::consts::EXE_SUFFIX))
        .exists());

    #[cfg(not(windows))]
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(fs_err::read_to_string(bin_dir.join("flask")).unwrap(), @r###"
        #![TEMP_DIR]/tools/flask/bin/python
        # -*- coding: utf-8 -*-
        import re
        import sys
        from flask.cli import main
        if __name__ == "__main__":
            sys.argv[0] = re.sub(r"(-script\.pyw|\.exe)?$", "", sys.argv[0])
            sys.exit(main())
        "###);
    });

    uv_snapshot!(context.filters(), Command::new("flask").arg("--version").env("PATH", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.[X]
    Flask 3.0.2
    Werkzeug 3.0.1

    ----- stderr -----
    "###);

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("flask").join("uv-receipt.toml")).unwrap(), @r###"
        [tool]
        requirements = [{ name = "flask" }]
        entrypoints = [
            { name = "flask", install-path = "[TEMP_DIR]/bin/flask" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "###);
    });
}

#[test]
fn tool_install_suggest_other_packages_with_executable() {
    let context = TestContext::new("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");
    let mut filters = context.filters();
    filters.push(("\\+ uvloop(.+)\n ", ""));

    uv_snapshot!(filters, context.tool_install()
    .env_remove("UV_EXCLUDE_NEWER")
    .arg("fastapi==0.111.0")
    .env("UV_EXCLUDE_NEWER", "2024-05-04T00:00:00Z") // TODO: Remove this once EXCLUDE_NEWER is bumped past 2024-05-04
    // (FastAPI 0.111 is only available from this date onwards)
    .env("UV_TOOL_DIR", tool_dir.as_os_str())
    .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    No executables are provided by `fastapi`
    However, an executable with the name `fastapi` is available via dependency `fastapi-cli`.
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
    "###);
}

/// Test installing a tool at a version
#[test]
fn tool_install_version() {
    let context = TestContext::new("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black==24.2.0")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
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
    "###);

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
        assert_snapshot!(fs_err::read_to_string(executable).unwrap(), @r###"
        #![TEMP_DIR]/tools/black/bin/python
        # -*- coding: utf-8 -*-
        import re
        import sys
        from black import patched_main
        if __name__ == "__main__":
            sys.argv[0] = re.sub(r"(-script\.pyw|\.exe)?$", "", sys.argv[0])
            sys.exit(patched_main())
        "###);

    });

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r###"
        [tool]
        requirements = [{ name = "black", specifier = "==24.2.0" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "###);
    });

    uv_snapshot!(context.filters(), Command::new("black").arg("--version").env("PATH", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    black, 24.2.0 (compiled: yes)
    Python (CPython) 3.12.[X]

    ----- stderr -----
    "###);
}

/// Test an editable installation of a tool.
#[test]
fn tool_install_editable() {
    let context = TestContext::new("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black` as an editable package.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("-e")
        .arg(context.workspace_root.join("scripts/packages/black_editable"))
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + black==0.1.0 (from file://[WORKSPACE]/scripts/packages/black_editable)
    Installed 1 executable: black
    "###);

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
        assert_snapshot!(fs_err::read_to_string(&executable).unwrap(), @r###"
        #![TEMP_DIR]/tools/black/bin/python
        # -*- coding: utf-8 -*-
        import re
        import sys
        from black import main
        if __name__ == "__main__":
            sys.argv[0] = re.sub(r"(-script\.pyw|\.exe)?$", "", sys.argv[0])
            sys.exit(main())
        "###);

    });

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r###"
        [tool]
        requirements = [{ name = "black", editable = "[WORKSPACE]/scripts/packages/black_editable" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "###);
    });

    uv_snapshot!(context.filters(), Command::new("black").arg("--version").env("PATH", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello world!

    ----- stderr -----
    "###);

    // Request `black`. It should reinstall from the registry.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed 1 executable: black
    "###);

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r###"
        [tool]
        requirements = [{ name = "black" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "###);
    });

    // Request `black` at a different version. It should install a new version.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("--from")
        .arg("black==24.2.0")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 6 packages in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 6 packages in [TIME]
     - black==0.1.0 (from file://[WORKSPACE]/scripts/packages/black_editable)
     + black==24.2.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    Installed 2 executables: black, blackd
    "###);

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r###"
        [tool]
        requirements = [{ name = "black", specifier = "==24.2.0" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "###);
    });
}

/// Ensure that we remove any existing entrypoints upon error.
#[test]
fn tool_install_remove_on_empty() -> Result<()> {
    let context = TestContext::new("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Request `black`. It should reinstall from the registry.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
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
    "###);

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r###"
        [tool]
        requirements = [{ name = "black" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "###);
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
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    No executables are provided by `black`

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
    "###);

    // Re-request `black`. It should reinstall, without requiring `--force`.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
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
    "###);

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r###"
        [tool]
        requirements = [{ name = "black" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "###);
    });

    Ok(())
}

/// Test an editable installation of a tool using `--from`.
#[test]
fn tool_install_editable_from() {
    let context = TestContext::new("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black` as an editable package.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("-e")
        .arg("--from")
        .arg(context.workspace_root.join("scripts/packages/black_editable"))
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + black==0.1.0 (from file://[WORKSPACE]/scripts/packages/black_editable)
    Installed 1 executable: black
    "###);

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
        assert_snapshot!(fs_err::read_to_string(&executable).unwrap(), @r###"
        #![TEMP_DIR]/tools/black/bin/python
        # -*- coding: utf-8 -*-
        import re
        import sys
        from black import main
        if __name__ == "__main__":
            sys.argv[0] = re.sub(r"(-script\.pyw|\.exe)?$", "", sys.argv[0])
            sys.exit(main())
        "###);

    });

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r###"
        [tool]
        requirements = [{ name = "black", editable = "[WORKSPACE]/scripts/packages/black_editable" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "###);
    });

    uv_snapshot!(context.filters(), Command::new("black").arg("--version").env("PATH", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello world!

    ----- stderr -----
    "###);
}

/// Test installing a tool with `uv tool install --from`
#[test]
fn tool_install_from() {
    let context = TestContext::new("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black` using `--from` to specify the version
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("--from")
        .arg("black==24.2.0")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
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
    "###);

    // Attempt to install `black` using `--from` with a different package name
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("--from")
        .arg("flask==24.2.0")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Package name (`flask`) provided with `--from` does not match install request (`black`)
    "###);

    // Attempt to install `black` using `--from` with a different version
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black==24.2.0")
        .arg("--from")
        .arg("black==24.3.0")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Package requirement (`black==24.3.0`) provided with `--from` conflicts with install request (`black==24.2.0`)
    "###);
}

/// Test installing and reinstalling an already installed tool
#[test]
fn tool_install_already_installed() {
    let context = TestContext::new("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
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
        assert_snapshot!(fs_err::read_to_string(executable).unwrap(), @r###"
        #![TEMP_DIR]/tools/black/bin/python
        # -*- coding: utf-8 -*-
        import re
        import sys
        from black import patched_main
        if __name__ == "__main__":
            sys.argv[0] = re.sub(r"(-script\.pyw|\.exe)?$", "", sys.argv[0])
            sys.exit(patched_main())
        "###);
    });

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r###"
        [tool]
        requirements = [{ name = "black" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "###);
    });

    // Install `black` again
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    `black` is already installed
    "###);

    tool_dir.child("black").assert(predicate::path::is_dir());
    bin_dir
        .child(format!("black{}", std::env::consts::EXE_SUFFIX))
        .assert(predicate::path::exists());

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should not have an additional tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r###"
        [tool]
        requirements = [{ name = "black" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "###);
    });

    // Install `black` again with the `--reinstall` flag
    // We should recreate the entire environment and reinstall the entry points
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("--reinstall")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
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
    "###);

    // Install `black` again with `--reinstall-package` for `black`
    // We should reinstall `black` in the environment and reinstall the entry points
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("--reinstall-package")
        .arg("black")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
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
    "###);

    // Install `black` again with `--reinstall-package` for a dependency
    // We should reinstall `click` in the environment but not reinstall `black`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("--reinstall-package")
        .arg("click")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
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
    "###);
}

/// Test installing a tool when its entry point already exists
#[test]
fn tool_install_entry_point_exists() {
    let context = TestContext::new("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    let executable = bin_dir.child(format!("black{}", std::env::consts::EXE_SUFFIX));
    executable.touch().unwrap();

    // Attempt to install `black`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
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
    "###);

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
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
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
    "###);

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
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
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
    "###);

    // Install `black` with `--force`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("--force")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
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
    "###);

    tool_dir.child("black").assert(predicate::path::is_dir());

    // Re-install `black` with `--force`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("--force")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed 2 executables: black, blackd
    "###);

    tool_dir.child("black").assert(predicate::path::is_dir());

    // Re-install `black` without `--force`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    `black` is already installed
    "###);

    tool_dir.child("black").assert(predicate::path::is_dir());

    // Re-install `black` with `--reinstall`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("--reinstall")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
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
    "###);

    tool_dir.child("black").assert(predicate::path::is_dir());

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We write a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r###"
        [tool]
        requirements = [{ name = "black" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "###);
    });

    // On Windows, we can't snapshot an executable file.
    #[cfg(not(windows))]
    insta::with_settings!({
        filters => context.filters(),
    }, {
        // Should run black in the virtual environment
        assert_snapshot!(fs_err::read_to_string(executable).unwrap(), @r###"
        #![TEMP_DIR]/tools/black/bin/python3
        # -*- coding: utf-8 -*-
        import re
        import sys
        from black import patched_main
        if __name__ == "__main__":
            sys.argv[0] = re.sub(r"(-script\.pyw|\.exe)?$", "", sys.argv[0])
            sys.exit(patched_main())
        "###);

    });

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r###"
        [tool]
        requirements = [{ name = "black" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "###);
    });

    uv_snapshot!(context.filters(), Command::new("black").arg("--version").env("PATH", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    black, 24.3.0 (compiled: yes)
    Python (CPython) 3.12.[X]

    ----- stderr -----
    "###);
}

/// Test `uv tool install` when the bin directory is inferred from `$HOME`
///
/// Only tested on Linux right now because it's not clear how to change the %USERPROFILE% on Windows
#[cfg(unix)]
#[test]
fn tool_install_home() {
    let context = TestContext::new("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");

    // Install `black`
    let mut cmd = context.tool_install();
    cmd.arg("black")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env(
            "XDG_DATA_HOME",
            context.home_dir.child(".local").child("share").as_os_str(),
        )
        .env(
            "PATH",
            context.home_dir.child(".local").child("bin").as_os_str(),
        );
    uv_snapshot!(context.filters(), cmd, @r###"
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
    "###);

    context
        .home_dir
        .child(format!(".local/bin/black{}", std::env::consts::EXE_SUFFIX))
        .assert(predicate::path::exists());
}

/// Test `uv tool install` when the bin directory is inferred from `$XDG_DATA_HOME`
#[test]
fn tool_install_xdg_data_home() {
    let context = TestContext::new("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let data_home = context.temp_dir.child("data/home");
    let bin_dir = context.temp_dir.child("data/bin");

    // Install `black`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_DATA_HOME", data_home.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
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
    "###);

    context
        .temp_dir
        .child(format!("data/bin/black{}", std::env::consts::EXE_SUFFIX))
        .assert(predicate::path::exists());
}

/// Test `uv tool install` when the bin directory is set by `$XDG_BIN_HOME`
#[test]
fn tool_install_xdg_bin_home() {
    let context = TestContext::new("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
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
    "###);

    bin_dir
        .child(format!("black{}", std::env::consts::EXE_SUFFIX))
        .assert(predicate::path::exists());
}

/// Test `uv tool install` when the bin directory is set by `$UV_TOOL_BIN_DIR`
#[test]
fn tool_install_tool_bin_dir() {
    let context = TestContext::new("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("UV_TOOL_BIN_DIR", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
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
    "###);

    bin_dir
        .child(format!("black{}", std::env::consts::EXE_SUFFIX))
        .assert(predicate::path::exists());
}

/// Test installing a tool that lacks entrypoints
#[test]
fn tool_install_no_entrypoints() {
    let context = TestContext::new("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    uv_snapshot!(context.filters(), context.tool_install()
        .arg("iniconfig")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    No executables are provided by `iniconfig`

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    "###);
}

/// Test installing a tool with a bare URL requirement.
#[test]
fn tool_install_unnamed_package() {
    let context = TestContext::new("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("https://files.pythonhosted.org/packages/0f/89/294c9a6b6c75a08da55e9d05321d0707e9418735e3062b12ef0f54c33474/black-24.4.2-py3-none-any.whl")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
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
    "###);

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
        assert_snapshot!(fs_err::read_to_string(executable).unwrap(), @r###"
        #![TEMP_DIR]/tools/black/bin/python
        # -*- coding: utf-8 -*-
        import re
        import sys
        from black import patched_main
        if __name__ == "__main__":
            sys.argv[0] = re.sub(r"(-script\.pyw|\.exe)?$", "", sys.argv[0])
            sys.exit(patched_main())
        "###);

    });

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r###"
        [tool]
        requirements = [{ name = "black", url = "https://files.pythonhosted.org/packages/0f/89/294c9a6b6c75a08da55e9d05321d0707e9418735e3062b12ef0f54c33474/black-24.4.2-py3-none-any.whl" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "###);
    });

    uv_snapshot!(context.filters(), Command::new("black").arg("--version").env("PATH", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    black, 24.4.2 (compiled: no)
    Python (CPython) 3.12.[X]

    ----- stderr -----
    "###);
}

/// Test installing a tool with a bare URL requirement using `--from`, where the URL and the package
/// name conflict.
#[test]
fn tool_install_unnamed_conflict() {
    let context = TestContext::new("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("--from")
        .arg("https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Package name (`iniconfig`) provided with `--from` does not match install request (`black`)
    "###);
}

/// Test installing a tool with a bare URL requirement using `--from`.
#[test]
fn tool_install_unnamed_from() {
    let context = TestContext::new("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("--from")
        .arg("https://files.pythonhosted.org/packages/0f/89/294c9a6b6c75a08da55e9d05321d0707e9418735e3062b12ef0f54c33474/black-24.4.2-py3-none-any.whl")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
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
    "###);

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
        assert_snapshot!(fs_err::read_to_string(executable).unwrap(), @r###"
        #![TEMP_DIR]/tools/black/bin/python
        # -*- coding: utf-8 -*-
        import re
        import sys
        from black import patched_main
        if __name__ == "__main__":
            sys.argv[0] = re.sub(r"(-script\.pyw|\.exe)?$", "", sys.argv[0])
            sys.exit(patched_main())
        "###);

    });

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r###"
        [tool]
        requirements = [{ name = "black", url = "https://files.pythonhosted.org/packages/0f/89/294c9a6b6c75a08da55e9d05321d0707e9418735e3062b12ef0f54c33474/black-24.4.2-py3-none-any.whl" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "###);
    });

    uv_snapshot!(context.filters(), Command::new("black").arg("--version").env("PATH", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    black, 24.4.2 (compiled: no)
    Python (CPython) 3.12.[X]

    ----- stderr -----
    "###);
}

/// Test installing a tool with a bare URL requirement using `--with`.
#[test]
fn tool_install_unnamed_with() {
    let context = TestContext::new("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("--with")
        .arg("https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
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
    "###);

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
        assert_snapshot!(fs_err::read_to_string(executable).unwrap(), @r###"
        #![TEMP_DIR]/tools/black/bin/python
        # -*- coding: utf-8 -*-
        import re
        import sys
        from black import patched_main
        if __name__ == "__main__":
            sys.argv[0] = re.sub(r"(-script\.pyw|\.exe)?$", "", sys.argv[0])
            sys.exit(patched_main())
        "###);

    });

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r###"
        [tool]
        requirements = [
            { name = "black" },
            { name = "iniconfig", url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl" },
        ]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "###);
    });

    uv_snapshot!(context.filters(), Command::new("black").arg("--version").env("PATH", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    black, 24.3.0 (compiled: yes)
    Python (CPython) 3.12.[X]

    ----- stderr -----
    "###);
}

/// Test installing a tool with extra requirements from a `requirements.txt` file.
#[test]
fn tool_install_requirements_txt() {
    let context = TestContext::new("3.12")
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
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
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
    "###);

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r###"
        [tool]
        requirements = [
            { name = "black" },
            { name = "iniconfig" },
        ]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "###);
    });

    // Update the `requirements.txt` file.
    requirements_txt.write_str("idna").unwrap();

    // Install `black`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("--with-requirements")
        .arg("requirements.txt")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
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
    "###);

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r###"
        [tool]
        requirements = [
            { name = "black" },
            { name = "idna" },
        ]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "###);
    });
}

/// Ignore and warn when (e.g.) the `--index-url` argument is a provided `requirements.txt`.
#[test]
fn tool_install_requirements_txt_arguments() {
    let context = TestContext::new("3.12").with_filtered_exe_suffix();
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
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
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
    "###);

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r###"
        [tool]
        requirements = [
            { name = "black" },
            { name = "idna" },
        ]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "###);
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
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed 2 executables: black, blackd
    "###);

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
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
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
    "###);
}

/// Test upgrading an already installed tool.
#[test]
fn tool_install_upgrade() {
    let context = TestContext::new("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black==24.1.1")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
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
    "###);

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r###"
        [tool]
        requirements = [{ name = "black", specifier = "==24.1.1" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "###);
    });

    // Install without the constraint. It should be replaced, but the package shouldn't be installed
    // since it's already satisfied in the environment.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed 2 executables: black, blackd
    "###);

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r###"
        [tool]
        requirements = [{ name = "black" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "###);
    });

    // Install with a `with`. It should be added to the environment.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("--with")
        .arg("iniconfig @ https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + iniconfig==2.0.0 (from https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl)
    Installed 2 executables: black, blackd
    "###);

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r###"
        [tool]
        requirements = [
            { name = "black" },
            { name = "iniconfig", url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl" },
        ]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "###);
    });

    // Install with `--upgrade`. `black` should be reinstalled with a more recent version, and
    // `iniconfig` should be removed.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("--upgrade")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
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
    "###);

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r###"
        [tool]
        requirements = [{ name = "black" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "###);
    });
}

/// Test reinstalling tools with varying `--python` requests.
#[test]
fn tool_install_python_request() {
    let context = TestContext::new_with_versions(&["3.11", "3.12"])
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("-p")
        .arg("3.12")
        .arg("black")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
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
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    `black` is already installed
    "###);

    // Install with Python 3.11 (incompatible).
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("-p")
        .arg("3.11")
        .arg("black")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Existing environment for `black` does not satisfy the requested Python interpreter
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
}

/// Test preserving a tool environment when new but incompatible requirements are requested.
#[test]
fn tool_install_preserve_environment() {
    let context = TestContext::new("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black==24.1.1")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
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
    "###);

    // Install `black`, but with an incompatible requirement.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black==24.1.1")
        .arg("--with")
        .arg("packaging==0.0.1")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Because black==24.1.1 depends on packaging>=22.0 and you require black==24.1.1, we can conclude that you require packaging>=22.0.
    And because you require packaging==0.0.1, we can conclude that your requirements are unsatisfiable.
    "###);

    // Install `black`. The tool should already be installed, since we didn't remove the environment.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black==24.1.1")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    `black==24.1.1` is already installed
    "###);
}

/// Test warning when the binary directory is not on the user's PATH.
#[test]
#[cfg(unix)]
fn tool_install_warn_path() {
    let context = TestContext::new("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black==24.1.1")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env_remove("PATH"), @r###"
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
    "###);
}

/// Test installing and reinstalling with an invalid receipt.
#[test]
fn tool_install_bad_receipt() -> Result<()> {
    let context = TestContext::new("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
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
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
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
    "###);

    Ok(())
}

/// Test installing a tool with a malformed `.dist-info` directory (i.e., a `.dist-info` directory
/// that isn't properly normalized).
#[test]
fn tool_install_malformed_dist_info() {
    let context = TestContext::new("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `babel`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("babel")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + babel==2.14.0
    Installed 1 executable: pybabel
    "###);

    tool_dir.child("babel").assert(predicate::path::is_dir());
    tool_dir
        .child("babel")
        .child("uv-receipt.toml")
        .assert(predicate::path::exists());

    let executable = bin_dir.child(format!("pybabel{}", std::env::consts::EXE_SUFFIX));
    assert!(executable.exists());

    // On Windows, we can't snapshot an executable file.
    #[cfg(not(windows))]
    insta::with_settings!({
        filters => context.filters(),
    }, {
        // Should run black in the virtual environment
        assert_snapshot!(fs_err::read_to_string(executable).unwrap(), @r###"
        #![TEMP_DIR]/tools/babel/bin/python
        # -*- coding: utf-8 -*-
        import re
        import sys
        from babel.messages.frontend import main
        if __name__ == "__main__":
            sys.argv[0] = re.sub(r"(-script\.pyw|\.exe)?$", "", sys.argv[0])
            sys.exit(main())
        "###);

    });

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("babel").join("uv-receipt.toml")).unwrap(), @r###"
        [tool]
        requirements = [{ name = "babel" }]
        entrypoints = [
            { name = "pybabel", install-path = "[TEMP_DIR]/bin/pybabel" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "###);
    });
}

/// Test installing, then re-installing with different settings.
#[test]
fn tool_install_settings() {
    let context = TestContext::new("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("flask>=3")
        .arg("--resolution=lowest-direct")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
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
    "###);

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
        assert_snapshot!(fs_err::read_to_string(executable).unwrap(), @r###"
        #![TEMP_DIR]/tools/flask/bin/python
        # -*- coding: utf-8 -*-
        import re
        import sys
        from flask.cli import main
        if __name__ == "__main__":
            sys.argv[0] = re.sub(r"(-script\.pyw|\.exe)?$", "", sys.argv[0])
            sys.exit(main())
        "###);

    });

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("flask").join("uv-receipt.toml")).unwrap(), @r###"
        [tool]
        requirements = [{ name = "flask", specifier = ">=3" }]
        entrypoints = [
            { name = "flask", install-path = "[TEMP_DIR]/bin/flask" },
        ]

        [tool.options]
        resolution = "lowest-direct"
        exclude-newer = "2024-03-25T00:00:00Z"
        "###);
    });

    // Reinstall with `highest`. This is a no-op, since we _do_ have a compatible version installed.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("flask>=3")
        .arg("--resolution=highest")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    `flask>=3` is already installed
    "###);

    // It should update the receipt though.
    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("flask").join("uv-receipt.toml")).unwrap(), @r###"
        [tool]
        requirements = [{ name = "flask", specifier = ">=3" }]
        entrypoints = [
            { name = "flask", install-path = "[TEMP_DIR]/bin/flask" },
        ]

        [tool.options]
        resolution = "highest"
        exclude-newer = "2024-03-25T00:00:00Z"
        "###);
    });

    // Reinstall with `highest` and `--upgrade`. This should change the setting and install a higher
    // version.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("flask>=3")
        .arg("--resolution=highest")
        .arg("--upgrade")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
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
    "###);

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("flask").join("uv-receipt.toml")).unwrap(), @r###"
        [tool]
        requirements = [{ name = "flask", specifier = ">=3" }]
        entrypoints = [
            { name = "flask", install-path = "[TEMP_DIR]/bin/flask" },
        ]

        [tool.options]
        resolution = "highest"
        exclude-newer = "2024-03-25T00:00:00Z"
        "###);
    });
}

/// Test installing a tool with `uv tool install {package}@{version}`.
#[test]
fn tool_install_at_version() {
    let context = TestContext::new("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black` at `24.1.0`.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black@24.1.0")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
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
    "###);

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r###"
        [tool]
        requirements = [{ name = "black", specifier = "==24.1.0" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "###);
    });

    // Combining `{package}@{version}` with a `--from` should fail (even if they're ultimately
    // compatible).
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black@24.1.0")
        .arg("--from")
        .arg("black==24.1.0")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Package requirement (`black==24.1.0`) provided with `--from` conflicts with install request (`black@24.1.0`)
    "###);
}

/// Test installing a tool with `uv tool install {package}@latest`.
#[test]
fn tool_install_at_latest() {
    let context = TestContext::new("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black` at latest.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black@latest")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
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

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r###"
        [tool]
        requirements = [{ name = "black" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "###);
    });
}

/// Test installing a tool with `uv tool install {package} --from {package}@latest`.
#[test]
fn tool_install_from_at_latest() {
    let context = TestContext::new("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    uv_snapshot!(context.filters(), context.tool_install()
        .arg("pybabel")
        .arg("--from")
        .arg("babel@latest")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + babel==2.14.0
    Installed 1 executable: pybabel
    "###);

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("babel").join("uv-receipt.toml")).unwrap(), @r###"
        [tool]
        requirements = [{ name = "babel" }]
        entrypoints = [
            { name = "pybabel", install-path = "[TEMP_DIR]/bin/pybabel" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "###);
    });
}

/// Test installing a tool with `uv tool install {package} --from {package}@{version}`.
#[test]
fn tool_install_from_at_version() {
    let context = TestContext::new("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    uv_snapshot!(context.filters(), context.tool_install()
        .arg("pybabel")
        .arg("--from")
        .arg("babel@2.13.0")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + babel==2.13.0
    Installed 1 executable: pybabel
    "###);

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("babel").join("uv-receipt.toml")).unwrap(), @r###"
        [tool]
        requirements = [{ name = "babel", specifier = "==2.13.0" }]
        entrypoints = [
            { name = "pybabel", install-path = "[TEMP_DIR]/bin/pybabel" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "###);
    });
}

/// Test upgrading an already installed tool via `{package}@{latest}`.
#[test]
fn tool_install_at_latest_upgrade() {
    let context = TestContext::new("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black==24.1.1")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
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
    "###);

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r###"
        [tool]
        requirements = [{ name = "black", specifier = "==24.1.1" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "###);
    });

    // Install without the constraint. It should be replaced, but the package shouldn't be installed
    // since it's already satisfied in the environment.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed 2 executables: black, blackd
    "###);

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r###"
        [tool]
        requirements = [{ name = "black" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "###);
    });

    // Install with `{package}@{latest}`. `black` should be reinstalled with a more recent version.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black@latest")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str())
        .env("PATH", bin_dir.as_os_str()), @r###"
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
    "###);

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r###"
        [tool]
        requirements = [{ name = "black" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "###);
    });
}
