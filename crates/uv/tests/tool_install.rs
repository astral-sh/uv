#![cfg(all(feature = "python", feature = "pypi"))]

use std::process::Command;

use assert_fs::{
    assert::PathAssert,
    fixture::{FileTouch, PathChild},
};
use common::{uv_snapshot, TestContext};
use insta::assert_snapshot;
use predicates::prelude::predicate;

mod common;

/// Test installing a tool with `uv tool install`
#[test]
fn tool_install() {
    let context = TestContext::new("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `python_dotenv`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("python-dotenv[cli]")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + click==8.1.7
     + python-dotenv==1.0.1
    Installed: dotenv
    "###);

    tool_dir
        .child("python-dotenv")
        .assert(predicate::path::is_dir());
    tool_dir
        .child("python-dotenv")
        .child("uv-receipt.toml")
        .assert(predicate::path::exists());

    let executable = bin_dir.child(format!("dotenv{}", std::env::consts::EXE_SUFFIX));
    assert!(executable.exists());

    // On Windows, we can't snapshot an executable file.
    #[cfg(not(windows))]
    insta::with_settings!({
        filters => context.filters(),
    }, {
        // Should run python_dotenv in the virtual environment
        assert_snapshot!(fs_err::read_to_string(executable).unwrap(), @r###"
        #![TEMP_DIR]/tools/python-dotenv/bin/python
        # -*- coding: utf-8 -*-
        import re
        import sys
        from dotenv.__main__ import cli
        if __name__ == "__main__":
            sys.argv[0] = re.sub(r"(-script\.pyw|\.exe)?$", "", sys.argv[0])
            sys.exit(cli())
        "###);
    });

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("python-dotenv").join("uv-receipt.toml")).unwrap(), @r###"
        [tool]
        requirements = ["python-dotenv[cli]"]
        entrypoints = [
            { name = "dotenv", install-path = "[TEMP_DIR]/bin/dotenv" },
        ]
        "###);
    });

    uv_snapshot!(context.filters(), Command::new("dotenv").arg("--version").env("PATH", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    dotenv, version 1.0.1

    ----- stderr -----
    "###);

    // Install another tool
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("flask")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
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
    Installed: flask
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
        // We should have a new tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("flask").join("uv-receipt.toml")).unwrap(), @r###"
        [tool]
        requirements = ["flask"]
        entrypoints = [
            { name = "flask", install-path = "[TEMP_DIR]/bin/flask" },
        ]
        "###);
    });
}

/// Test installing a tool at a version
#[test]
fn tool_install_version() {
    let context = TestContext::new("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `python_dotenv`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("python_dotenv[cli]==1.0.0")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + click==8.1.7
     + python-dotenv==1.0.0
    Installed: dotenv
    "###);

    tool_dir
        .child("python-dotenv")
        .assert(predicate::path::is_dir());
    tool_dir
        .child("python-dotenv")
        .child("uv-receipt.toml")
        .assert(predicate::path::exists());

    let executable = bin_dir.child(format!("dotenv{}", std::env::consts::EXE_SUFFIX));
    assert!(executable.exists());

    // On Windows, we can't snapshot an executable file.
    #[cfg(not(windows))]
    insta::with_settings!({
        filters => context.filters(),
    }, {
        // Should run python_dotenv in the virtual environment
        assert_snapshot!(fs_err::read_to_string(executable).unwrap(), @r###"
        #![TEMP_DIR]/tools/python-dotenv/bin/python
        # -*- coding: utf-8 -*-
        import re
        import sys
        from dotenv.__main__ import cli
        if __name__ == "__main__":
            sys.argv[0] = re.sub(r"(-script\.pyw|\.exe)?$", "", sys.argv[0])
            sys.exit(cli())
        "###);

    });

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("python-dotenv").join("uv-receipt.toml")).unwrap(), @r###"
        [tool]
        requirements = ["python-dotenv[cli]==1.0.0"]
        entrypoints = [
            { name = "dotenv", install-path = "[TEMP_DIR]/bin/dotenv" },
        ]
        "###);
    });

    uv_snapshot!(context.filters(), Command::new("dotenv").arg("--version").env("PATH", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    dotenv, version 1.0.0

    ----- stderr -----
    "###);
}

/// Test installing a tool with `uv tool install --from`
#[test]
fn tool_install_from() {
    let context = TestContext::new("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `python_dotenv` using `--from` to specify the version
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("python-dotenv[cli]")
        .arg("--from")
        .arg("python_dotenv[cli]==1.0.0")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    error: Package requirement `python_dotenv[cli]==1.0.0` provided with `--from` conflicts with install request `python-dotenv[cli]`
    "###);

    // Attempt to install `python_dotenv` using `--from` with a different package name
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("python-dotenv[cli]")
        .arg("--from")
        .arg("python_dotenv[cli]==1.0.0")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    error: Package requirement `python_dotenv[cli]==1.0.0` provided with `--from` conflicts with install request `python-dotenv[cli]`
    "###);

    // Attempt to install `python_dotenv` using `--from` with a different version
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("python_dotenv==1.0.1")
        .arg("--from")
        .arg("python_dotenv[cli]==1.0.0")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    error: Package requirement `python_dotenv[cli]==1.0.0` provided with `--from` conflicts with install request `python_dotenv==1.0.1`
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

    // Install `python_dotenv`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("python-dotenv[cli]")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + click==8.1.7
     + python-dotenv==1.0.1
    Installed: dotenv
    "###);

    tool_dir
        .child("python-dotenv")
        .assert(predicate::path::is_dir());
    tool_dir
        .child("python-dotenv")
        .child("uv-receipt.toml")
        .assert(predicate::path::exists());

    let executable = bin_dir.child(format!("dotenv{}", std::env::consts::EXE_SUFFIX));
    assert!(executable.exists());

    // On Windows, we can't snapshot an executable file.
    #[cfg(not(windows))]
    insta::with_settings!({
        filters => context.filters(),
    }, {
        // Should run python_dotenv in the virtual environment
        assert_snapshot!(fs_err::read_to_string(executable).unwrap(), @r###"
        #![TEMP_DIR]/tools/python-dotenv/bin/python
        # -*- coding: utf-8 -*-
        import re
        import sys
        from dotenv.__main__ import cli
        if __name__ == "__main__":
            sys.argv[0] = re.sub(r"(-script\.pyw|\.exe)?$", "", sys.argv[0])
            sys.exit(cli())
        "###);

    });

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("python-dotenv").join("uv-receipt.toml")).unwrap(), @r###"
        [tool]
        requirements = ["python-dotenv[cli]"]
        entrypoints = [
            { name = "dotenv", install-path = "[TEMP_DIR]/bin/dotenv" },
        ]
        "###);
    });

    // Install `python_dotenv` again
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("python-dotenv[cli]")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    Tool `python-dotenv[cli]` is already installed
    "###);

    tool_dir
        .child("python-dotenv")
        .assert(predicate::path::is_dir());
    bin_dir
        .child(format!("dotenv{}", std::env::consts::EXE_SUFFIX))
        .assert(predicate::path::exists());

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should not have an additional tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("python-dotenv").join("uv-receipt.toml")).unwrap(), @r###"
        [tool]
        requirements = ["python-dotenv[cli]"]
        entrypoints = [
            { name = "dotenv", install-path = "[TEMP_DIR]/bin/dotenv" },
        ]
        "###);
    });

    // Install `python_dotenv` again with the `--reinstall` flag
    // We should recreate the entire environment and reinstall the entry points
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("python-dotenv[cli]")
        .arg("--reinstall")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    Resolved [N] packages in [TIME]
    Uninstalled [N] packages in [TIME]
    Installed [N] packages in [TIME]
     - click==8.1.7
     + click==8.1.7
     - python-dotenv==1.0.1
     + python-dotenv==1.0.1
    Installed: dotenv
    "###);

    // Install `python_dotenv` again with `--reinstall-package` for `python_dotenv`
    // We should reinstall `python_dotenv` in the environment and reinstall the entry points
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("python-dotenv[cli]")
        .arg("--reinstall-package")
        .arg("python-dotenv")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    Resolved [N] packages in [TIME]
    Uninstalled [N] packages in [TIME]
    Installed [N] packages in [TIME]
     - python-dotenv==1.0.1
     + python-dotenv==1.0.1
    Installed: dotenv
    "###);

    // Install `python_dotenv` again with `--reinstall-package` for a dependency
    // We should reinstall `click` in the environment but not reinstall `python_dotenv`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("python-dotenv[cli]")
        .arg("--reinstall-package")
        .arg("click")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    Resolved [N] packages in [TIME]
    Uninstalled [N] packages in [TIME]
    Installed [N] packages in [TIME]
     - click==8.1.7
     + click==8.1.7
    Installed: dotenv
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

    let executable = bin_dir.child(format!("dotenv{}", std::env::consts::EXE_SUFFIX));
    executable.touch().unwrap();

    // Attempt to install `python_dotenv`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("python-dotenv[cli]")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + click==8.1.7
     + python-dotenv==1.0.1
    error: Entry point for tool already exists: dotenv (use `--force` to overwrite)
    "###);

    // We should delete the virtual environment
    assert!(!tool_dir.child("python-dotenv").exists());

    // We should not write a tools entry
    assert!(!tool_dir
        .join("python-dotenv")
        .join("uv-receipt.toml")
        .exists());

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // Nor should we change the `python_dotenv` entry point that exists
        assert_snapshot!(fs_err::read_to_string(&executable).unwrap(), @"");

    });

    // Attempt to install `python_dotenv` with the `--reinstall` flag
    // Should have no effect
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("python-dotenv[cli]")
        .arg("--reinstall")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    Resolved [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + click==8.1.7
     + python-dotenv==1.0.1
    error: Entry point for tool already exists: dotenv (use `--force` to overwrite)
    "###);

    // We should not create a virtual environment
    assert!(!tool_dir.child("python-dotenv").exists());

    // We should not write a tools entry
    assert!(!tool_dir.join("tools.toml").exists());

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // Nor should we change the `python_dotenv` entry point that exists
        assert_snapshot!(fs_err::read_to_string(&executable).unwrap(), @"");

    });

    // Install `python_dotenv` with `--force`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("python-dotenv[cli]")
        .arg("--force")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    Resolved [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + click==8.1.7
     + python-dotenv==1.0.1
    Installed: dotenv
    "###);

    tool_dir
        .child("python-dotenv")
        .assert(predicate::path::is_dir());

    // Re-install `python_dotenv` with `--force`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("python-dotenv[cli]")
        .arg("--force")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    Installed: dotenv
    "###);

    tool_dir
        .child("python-dotenv")
        .assert(predicate::path::is_dir());

    // Re-install `python_dotenv` without `--force`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("python-dotenv[cli]")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    Tool `python-dotenv[cli]` is already installed
    "###);

    tool_dir
        .child("python-dotenv")
        .assert(predicate::path::is_dir());

    // Re-install `python_dotenv` with `--reinstall`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("python-dotenv[cli]")
        .arg("--reinstall")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    Resolved [N] packages in [TIME]
    Uninstalled [N] packages in [TIME]
    Installed [N] packages in [TIME]
     - click==8.1.7
     + click==8.1.7
     - python-dotenv==1.0.1
     + python-dotenv==1.0.1
    Installed: dotenv
    "###);

    tool_dir
        .child("python-dotenv")
        .assert(predicate::path::is_dir());

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We write a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("python-dotenv").join("uv-receipt.toml")).unwrap(), @r###"
        [tool]
        requirements = ["python-dotenv[cli]"]
        entrypoints = [
            { name = "dotenv", install-path = "[TEMP_DIR]/bin/dotenv" },
        ]
        "###);
    });

    // On Windows, we can't snapshot an executable file.
    #[cfg(not(windows))]
    insta::with_settings!({
        filters => context.filters(),
    }, {
        // Should run python_dotenv in the virtual environment
        assert_snapshot!(fs_err::read_to_string(executable).unwrap(), @r###"
        #![TEMP_DIR]/tools/python-dotenv/bin/python3
        # -*- coding: utf-8 -*-
        import re
        import sys
        from dotenv.__main__ import cli
        if __name__ == "__main__":
            sys.argv[0] = re.sub(r"(-script\.pyw|\.exe)?$", "", sys.argv[0])
            sys.exit(cli())
        "###);

    });

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("python-dotenv").join("uv-receipt.toml")).unwrap(), @r###"
        [tool]
        requirements = ["python-dotenv[cli]"]
        entrypoints = [
            { name = "dotenv", install-path = "[TEMP_DIR]/bin/dotenv" },
        ]
        "###);
    });

    uv_snapshot!(context.filters(), Command::new("dotenv").arg("--version").env("PATH", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    dotenv, version 1.0.1

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

    // Install `python_dotenv`
    uv_snapshot!(context.filters(), context.tool_install().arg("python-dotenv[cli]").env("UV_TOOL_DIR", tool_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + click==8.1.7
     + python-dotenv==1.0.1
    Installed: dotenv
    "###);

    context
        .home_dir
        .child(format!(".local/bin/dotenv{}", std::env::consts::EXE_SUFFIX))
        .assert(predicate::path::exists());
}

/// Test `uv tool install` when the bin directory is inferred from `$XDG_DATA_HOME`
#[test]
fn tool_install_xdg_data_home() {
    let context = TestContext::new("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let data_home = context.temp_dir.child("data/home");

    // Install `python_dotenv`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("python-dotenv[cli]")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_DATA_HOME", data_home.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + click==8.1.7
     + python-dotenv==1.0.1
    Installed: dotenv
    "###);

    context
        .temp_dir
        .child(format!("data/bin/dotenv{}", std::env::consts::EXE_SUFFIX))
        .assert(predicate::path::exists());
}

/// Test `uv tool install` when the bin directory is set by `$XDG_BIN_HOME`
#[test]
fn tool_install_xdg_bin_home() {
    let context = TestContext::new("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `python_dotenv`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("python-dotenv[cli]")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + click==8.1.7
     + python-dotenv==1.0.1
    Installed: dotenv
    "###);

    bin_dir
        .child(format!("dotenv{}", std::env::consts::EXE_SUFFIX))
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
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    error: No entry points found for tool `iniconfig`
    "###);
}

/// Test installing a tool with a bare URL requirement.
#[test]
fn tool_install_unnamed_package() {
    let context = TestContext::new("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `python_dotenv`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("https://files.pythonhosted.org/packages/6a/3e/b68c118422ec867fa7ab88444e1274aa40681c606d59ac27de5a5588f082/python_dotenv-1.0.1-py3-none-any.whl")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + python-dotenv==1.0.1 (from https://files.pythonhosted.org/packages/6a/3e/b68c118422ec867fa7ab88444e1274aa40681c606d59ac27de5a5588f082/python_dotenv-1.0.1-py3-none-any.whl)
    Installed: dotenv
    "###);

    tool_dir
        .child("python-dotenv")
        .assert(predicate::path::is_dir());
    tool_dir
        .child("python-dotenv")
        .child("uv-receipt.toml")
        .assert(predicate::path::exists());

    let executable = bin_dir.child(format!("dotenv{}", std::env::consts::EXE_SUFFIX));
    assert!(executable.exists());

    // On Windows, we can't snapshot an executable file.
    #[cfg(not(windows))]
    insta::with_settings!({
        filters => context.filters(),
    }, {
        // Should run python_dotenv in the virtual environment
        assert_snapshot!(fs_err::read_to_string(executable).unwrap(), @r###"
        #![TEMP_DIR]/tools/python-dotenv/bin/python
        # -*- coding: utf-8 -*-
        import re
        import sys
        from dotenv.__main__ import cli
        if __name__ == "__main__":
            sys.argv[0] = re.sub(r"(-script\.pyw|\.exe)?$", "", sys.argv[0])
            sys.exit(cli())
        "###);

    });

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("python-dotenv").join("uv-receipt.toml")).unwrap(), @r###"
        [tool]
        requirements = ["python-dotenv @ https://files.pythonhosted.org/packages/6a/3e/b68c118422ec867fa7ab88444e1274aa40681c606d59ac27de5a5588f082/python_dotenv-1.0.1-py3-none-any.whl"]
        entrypoints = [
            { name = "dotenv", install-path = "[TEMP_DIR]/bin/dotenv" },
        ]
        "###);
    });
}

/// Test installing a tool with a bare URL requirement using `--from`, where the URL and the package
/// name conflict.
#[test]
fn tool_install_unnamed_conflict() {
    let context = TestContext::new("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `python_dotenv`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("python-dotenv[cli]")
        .arg("--from")
        .arg("https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    error: Package requirement `https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl` provided with `--from` conflicts with install request `python-dotenv[cli]`
    "###);
}

/// Test installing a tool with a bare URL requirement using `--from`.
#[test]
fn tool_install_unnamed_from() {
    let context = TestContext::new("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `python_dotenv`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("python-dotenv")
        .arg("--from")
        .arg("https://files.pythonhosted.org/packages/6a/3e/b68c118422ec867fa7ab88444e1274aa40681c606d59ac27de5a5588f082/python_dotenv-1.0.1-py3-none-any.whl")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + python-dotenv==1.0.1 (from https://files.pythonhosted.org/packages/6a/3e/b68c118422ec867fa7ab88444e1274aa40681c606d59ac27de5a5588f082/python_dotenv-1.0.1-py3-none-any.whl)
    Installed: dotenv
    "###);

    tool_dir
        .child("python-dotenv")
        .assert(predicate::path::is_dir());
    tool_dir
        .child("python-dotenv")
        .child("uv-receipt.toml")
        .assert(predicate::path::exists());

    let executable = bin_dir.child(format!("dotenv{}", std::env::consts::EXE_SUFFIX));
    assert!(executable.exists());

    // On Windows, we can't snapshot an executable file.
    #[cfg(not(windows))]
    insta::with_settings!({
        filters => context.filters(),
    }, {
        // Should run python_dotenv in the virtual environment
        assert_snapshot!(fs_err::read_to_string(executable).unwrap(), @r###"
        #![TEMP_DIR]/tools/python-dotenv/bin/python
        # -*- coding: utf-8 -*-
        import re
        import sys
        from dotenv.__main__ import cli
        if __name__ == "__main__":
            sys.argv[0] = re.sub(r"(-script\.pyw|\.exe)?$", "", sys.argv[0])
            sys.exit(cli())
        "###);

    });

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("python-dotenv").join("uv-receipt.toml")).unwrap(), @r###"
        [tool]
        requirements = ["python-dotenv @ https://files.pythonhosted.org/packages/6a/3e/b68c118422ec867fa7ab88444e1274aa40681c606d59ac27de5a5588f082/python_dotenv-1.0.1-py3-none-any.whl"]
        entrypoints = [
            { name = "dotenv", install-path = "[TEMP_DIR]/bin/dotenv" },
        ]
        "###);
    });
}

/// Test installing a tool with a bare URL requirement using `--with`.
#[test]
fn tool_install_unnamed_with() {
    let context = TestContext::new("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `python_dotenv`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("python-dotenv[cli]")
        .arg("--with")
        .arg("https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + click==8.1.7
     + iniconfig==2.0.0 (from https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl)
     + python-dotenv==1.0.1
    Installed: dotenv
    "###);

    tool_dir
        .child("python-dotenv")
        .assert(predicate::path::is_dir());
    tool_dir
        .child("python-dotenv")
        .child("uv-receipt.toml")
        .assert(predicate::path::exists());

    let executable = bin_dir.child(format!("dotenv{}", std::env::consts::EXE_SUFFIX));
    assert!(executable.exists());

    // On Windows, we can't snapshot an executable file.
    #[cfg(not(windows))]
    insta::with_settings!({
        filters => context.filters(),
    }, {
        // Should run python_dotenv in the virtual environment
        assert_snapshot!(fs_err::read_to_string(executable).unwrap(), @r###"
        #![TEMP_DIR]/tools/python-dotenv/bin/python
        # -*- coding: utf-8 -*-
        import re
        import sys
        from dotenv.__main__ import cli
        if __name__ == "__main__":
            sys.argv[0] = re.sub(r"(-script\.pyw|\.exe)?$", "", sys.argv[0])
            sys.exit(cli())
        "###);

    });

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("python-dotenv").join("uv-receipt.toml")).unwrap(), @r###"
        [tool]
        requirements = [
            "python-dotenv[cli]",
            "iniconfig @ https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl",
        ]
        entrypoints = [
            { name = "dotenv", install-path = "[TEMP_DIR]/bin/dotenv" },
        ]
        "###);
    });
}

/// Test upgrading an already installed tool.
#[test]
fn tool_install_upgrade() {
    let context = TestContext::new("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `python_dotenv`.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("python_dotenv[cli]==1.0.0")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + click==8.1.7
     + python-dotenv==1.0.0
    Installed: dotenv
    "###);

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("python-dotenv").join("uv-receipt.toml")).unwrap(), @r###"
        [tool]
        requirements = ["python-dotenv[cli]==1.0.0"]
        entrypoints = [
            { name = "dotenv", install-path = "[TEMP_DIR]/bin/dotenv" },
        ]
        "###);
    });

    // Install without the constraint. It should be replaced, but the package shouldn't be installed
    // since it's already satisfied in the environment.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("python-dotenv[cli]")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    Installed: dotenv
    "###);

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("python-dotenv").join("uv-receipt.toml")).unwrap(), @r###"
        [tool]
        requirements = ["python-dotenv[cli]"]
        entrypoints = [
            { name = "dotenv", install-path = "[TEMP_DIR]/bin/dotenv" },
        ]
        "###);
    });

    // Install with a `with`. It should be added to the environment.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("python-dotenv[cli]")
        .arg("--with")
        .arg("iniconfig @ https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + iniconfig==2.0.0 (from https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl)
    Installed: dotenv
    "###);

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("python-dotenv").join("uv-receipt.toml")).unwrap(), @r###"
        [tool]
        requirements = [
            "python-dotenv[cli]",
            "iniconfig @ https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl",
        ]
        entrypoints = [
            { name = "dotenv", install-path = "[TEMP_DIR]/bin/dotenv" },
        ]
        "###);
    });

    // Install with `--upgrade`. `python_dotenv` should be reinstalled with a more recent version, and
    // `iniconfig` should be removed.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("python-dotenv[cli]")
        .arg("--upgrade")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Uninstalled [N] packages in [TIME]
    Installed [N] packages in [TIME]
     - iniconfig==2.0.0 (from https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl)
     - python-dotenv==1.0.0
     + python-dotenv==1.0.1
    Installed: dotenv
    "###);

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("python-dotenv").join("uv-receipt.toml")).unwrap(), @r###"
        [tool]
        requirements = ["python-dotenv[cli]"]
        entrypoints = [
            { name = "dotenv", install-path = "[TEMP_DIR]/bin/dotenv" },
        ]
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

    // Install `python_dotenv`.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("-p")
        .arg("3.12")
        .arg("python-dotenv[cli]")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + click==8.1.7
     + python-dotenv==1.0.1
    Installed: dotenv
    "###);

    // Install with Python 3.12 (compatible).
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("-p")
        .arg("3.12")
        .arg("python-dotenv[cli]")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    Tool `python-dotenv[cli]` is already installed
    "###);

    // Install with Python 3.11 (incompatible).
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("-p")
        .arg("3.11")
        .arg("python-dotenv[cli]")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    Existing environment for `python-dotenv` does not satisfy the requested Python interpreter: `Python 3.11`
    Resolved [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + click==8.1.7
     + python-dotenv==1.0.1
    Installed: dotenv
    "###);
}
