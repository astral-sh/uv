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

    // Install `flask`
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
        // Should run flask in the virtual environment
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
        requirements = ["flask"]
        entrypoints = [
            { name = "flask", install-path = "[TEMP_DIR]/bin/flask" },
        ]
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

    // Install another tool
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("pyflakes")
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
     + pyflakes==3.2.0
    Installed: pyflakes
    "###);

    tool_dir.child("pyflakes").assert(predicate::path::is_dir());
    assert!(bin_dir
        .child(format!("pyflakes{}", std::env::consts::EXE_SUFFIX))
        .exists());

    #[cfg(not(windows))]
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(fs_err::read_to_string(bin_dir.join("pyflakes")).unwrap(), @r###"
        #![TEMP_DIR]/tools/pyflakes/bin/python
        # -*- coding: utf-8 -*-
        import re
        import sys
        from pyflakes.api import main
        if __name__ == "__main__":
            sys.argv[0] = re.sub(r"(-script\.pyw|\.exe)?$", "", sys.argv[0])
            sys.exit(main())
        "###);

    });

    uv_snapshot!(context.filters(), Command::new("pyflakes").arg("--version").env("PATH", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    3.2.0 Python 3.12.[X] on Darwin

    ----- stderr -----
    "###);

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a new tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("pyflakes").join("uv-receipt.toml")).unwrap(), @r###"
        [tool]
        requirements = ["pyflakes"]
        entrypoints = [
            { name = "pyflakes", install-path = "[TEMP_DIR]/bin/pyflakes" },
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

    // Install `flask`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("flask==3.0.0")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    Resolved 7 packages in [TIME]
    Prepared 7 packages in [TIME]
    Installed 7 packages in [TIME]
     + blinker==1.7.0
     + click==8.1.7
     + flask==3.0.0
     + itsdangerous==2.1.2
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + werkzeug==3.0.1
    Installed: flask
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
        // Should run flask in the virtual environment
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
        requirements = ["flask==3.0.0"]
        entrypoints = [
            { name = "flask", install-path = "[TEMP_DIR]/bin/flask" },
        ]
        "###);
    });

    uv_snapshot!(context.filters(), Command::new("flask").arg("--version").env("PATH", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.[X]
    Flask 3.0.0
    Werkzeug 3.0.1

    ----- stderr -----
    "###);
}

/// Test installing a tool with `uv tool install --from`
#[test]
fn tool_install_from() {
    let context = TestContext::new("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `flask` using `--from` to specify the version
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("flask")
        .arg("--from")
        .arg("flask==3.0.0")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    Resolved 7 packages in [TIME]
    Prepared 7 packages in [TIME]
    Installed 7 packages in [TIME]
     + blinker==1.7.0
     + click==8.1.7
     + flask==3.0.0
     + itsdangerous==2.1.2
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + werkzeug==3.0.1
    Installed: flask
    "###);

    // Attempt to install `flask` using `--from` with a different package name
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("flask")
        .arg("--from")
        .arg("flask==3.0.0")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    Tool `flask==3.0.0` is already installed
    "###);

    // Attempt to install `flask` using `--from` with a different version
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("flask==1.0.1")
        .arg("--from")
        .arg("flask==3.0.0")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    error: Package requirement `flask==3.0.0` provided with `--from` conflicts with install request `flask==1.0.1`
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

    // Install `flask`
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
        // Should run flask in the virtual environment
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
        requirements = ["flask"]
        entrypoints = [
            { name = "flask", install-path = "[TEMP_DIR]/bin/flask" },
        ]
        "###);
    });

    // Install `flask` again
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("flask")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    Tool `flask` is already installed
    "###);

    tool_dir.child("flask").assert(predicate::path::is_dir());
    bin_dir
        .child(format!("flask{}", std::env::consts::EXE_SUFFIX))
        .assert(predicate::path::exists());

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should not have an additional tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("flask").join("uv-receipt.toml")).unwrap(), @r###"
        [tool]
        requirements = ["flask"]
        entrypoints = [
            { name = "flask", install-path = "[TEMP_DIR]/bin/flask" },
        ]
        "###);
    });

    // Install `flask` again with the `--reinstall` flag
    // We should recreate the entire environment and reinstall the entry points
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("flask")
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
     - blinker==1.7.0
     + blinker==1.7.0
     - click==8.1.7
     + click==8.1.7
     - flask==3.0.2
     + flask==3.0.2
     - itsdangerous==2.1.2
     + itsdangerous==2.1.2
     - jinja2==3.1.3
     + jinja2==3.1.3
     - markupsafe==2.1.5
     + markupsafe==2.1.5
     - werkzeug==3.0.1
     + werkzeug==3.0.1
    Installed: flask
    "###);

    // Install `flask` again with `--reinstall-package` for `flask`
    // We should reinstall `flask` in the environment and reinstall the entry points
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("flask")
        .arg("--reinstall-package")
        .arg("flask")
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
     - flask==3.0.2
     + flask==3.0.2
    Installed: flask
    "###);

    // Install `flask` again with `--reinstall-package` for a dependency
    // We should reinstall `click` in the environment but not reinstall `flask`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("flask")
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
    Installed: flask
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

    let executable = bin_dir.child(format!("flask{}", std::env::consts::EXE_SUFFIX));
    executable.touch().unwrap();

    // Attempt to install `flask`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("flask")
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
     + blinker==1.7.0
     + click==8.1.7
     + flask==3.0.2
     + itsdangerous==2.1.2
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + werkzeug==3.0.1
    error: Entry point for tool already exists: flask (use `--force` to overwrite)
    "###);

    // We should delete the virtual environment
    assert!(!tool_dir.child("flask").exists());

    // We should not write a tools entry
    assert!(!tool_dir.join("flask").join("uv-receipt.toml").exists());

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // Nor should we change the `flask` entry point that exists
        assert_snapshot!(fs_err::read_to_string(&executable).unwrap(), @"");

    });

    // Attempt to install `flask` with the `--reinstall` flag
    // Should have no effect
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("flask")
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
     + blinker==1.7.0
     + click==8.1.7
     + flask==3.0.2
     + itsdangerous==2.1.2
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + werkzeug==3.0.1
    error: Entry point for tool already exists: flask (use `--force` to overwrite)
    "###);

    // We should not create a virtual environment
    assert!(!tool_dir.child("flask").exists());

    // We should not write a tools entry
    assert!(!tool_dir.join("tools.toml").exists());

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // Nor should we change the `flask` entry point that exists
        assert_snapshot!(fs_err::read_to_string(&executable).unwrap(), @"");

    });

    // Install `flask` with `--force`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("flask")
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

    // Re-install `flask` with `--force`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("flask")
        .arg("--force")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    Installed: flask
    "###);

    tool_dir.child("flask").assert(predicate::path::is_dir());

    // Re-install `flask` without `--force`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("flask")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    Tool `flask` is already installed
    "###);

    tool_dir.child("flask").assert(predicate::path::is_dir());

    // Re-install `flask` with `--reinstall`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("flask")
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
     - blinker==1.7.0
     + blinker==1.7.0
     - click==8.1.7
     + click==8.1.7
     - flask==3.0.2
     + flask==3.0.2
     - itsdangerous==2.1.2
     + itsdangerous==2.1.2
     - jinja2==3.1.3
     + jinja2==3.1.3
     - markupsafe==2.1.5
     + markupsafe==2.1.5
     - werkzeug==3.0.1
     + werkzeug==3.0.1
    Installed: flask
    "###);

    tool_dir.child("flask").assert(predicate::path::is_dir());

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We write a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("flask").join("uv-receipt.toml")).unwrap(), @r###"
        [tool]
        requirements = ["flask"]
        entrypoints = [
            { name = "flask", install-path = "[TEMP_DIR]/bin/flask" },
        ]
        "###);
    });

    // On Windows, we can't snapshot an executable file.
    #[cfg(not(windows))]
    insta::with_settings!({
        filters => context.filters(),
    }, {
        // Should run flask in the virtual environment
        assert_snapshot!(fs_err::read_to_string(executable).unwrap(), @r###"
        #![TEMP_DIR]/tools/flask/bin/python3
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
        requirements = ["flask"]
        entrypoints = [
            { name = "flask", install-path = "[TEMP_DIR]/bin/flask" },
        ]
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
}

/// Test `uv tool install` when the bin directory is inferred from `$HOME`
///
/// Only tested on Linux right now because it's not clear how to change the %USERPROFILE% on Windows
#[cfg(unix)]
#[test]
fn tool_install_home() {
    let context = TestContext::new("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");

    // Install `flask`
    uv_snapshot!(context.filters(), context.tool_install().arg("flask").env("UV_TOOL_DIR", tool_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    Resolved 7 packages in [TIME]
    Prepared 7 packages in [TIME]
    Installed 7 packages in [TIME]
     + blinker==1.7.0
     + click==8.1.7
     + flask==3.0.2
     + itsdangerous==2.1.2
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + werkzeug==3.0.1
    Installed: flask
    "###);

    context
        .home_dir
        .child(format!(".local/bin/flask{}", std::env::consts::EXE_SUFFIX))
        .assert(predicate::path::exists());
}

/// Test `uv tool install` when the bin directory is inferred from `$XDG_DATA_HOME`
#[test]
fn tool_install_xdg_data_home() {
    let context = TestContext::new("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let data_home = context.temp_dir.child("data/home");

    // Install `flask`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("flask")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_DATA_HOME", data_home.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    Resolved 7 packages in [TIME]
    Prepared 7 packages in [TIME]
    Installed 7 packages in [TIME]
     + blinker==1.7.0
     + click==8.1.7
     + flask==3.0.2
     + itsdangerous==2.1.2
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + werkzeug==3.0.1
    Installed: flask
    "###);

    context
        .temp_dir
        .child(format!("data/bin/flask{}", std::env::consts::EXE_SUFFIX))
        .assert(predicate::path::exists());
}

/// Test `uv tool install` when the bin directory is set by `$XDG_BIN_HOME`
#[test]
fn tool_install_xdg_bin_home() {
    let context = TestContext::new("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `flask`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("flask")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    Resolved 7 packages in [TIME]
    Prepared 7 packages in [TIME]
    Installed 7 packages in [TIME]
     + blinker==1.7.0
     + click==8.1.7
     + flask==3.0.2
     + itsdangerous==2.1.2
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + werkzeug==3.0.1
    Installed: flask
    "###);

    bin_dir
        .child(format!("flask{}", std::env::consts::EXE_SUFFIX))
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

    // Install `flask`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("https://files.pythonhosted.org/packages/61/80/ffe1da13ad9300f87c93af113edd0638c75138c42a0994becfacac078c06/flask-3.0.3-py3-none-any.whl")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    Resolved 7 packages in [TIME]
    Prepared 7 packages in [TIME]
    Installed 7 packages in [TIME]
     + blinker==1.7.0
     + click==8.1.7
     + flask==3.0.3 (from https://files.pythonhosted.org/packages/61/80/ffe1da13ad9300f87c93af113edd0638c75138c42a0994becfacac078c06/flask-3.0.3-py3-none-any.whl)
     + itsdangerous==2.1.2
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + werkzeug==3.0.1
    Installed: flask
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
        // Should run flask in the virtual environment
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
        requirements = ["flask @ https://files.pythonhosted.org/packages/61/80/ffe1da13ad9300f87c93af113edd0638c75138c42a0994becfacac078c06/flask-3.0.3-py3-none-any.whl"]
        entrypoints = [
            { name = "flask", install-path = "[TEMP_DIR]/bin/flask" },
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

    // Install `flask`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("flask")
        .arg("--from")
        .arg("https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    error: Package name `iniconfig` provided with `--from` does not match install request `flask`
    "###);
}

/// Test installing a tool with a bare URL requirement using `--from`.
#[test]
fn tool_install_unnamed_from() {
    let context = TestContext::new("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `flask`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("flask")
        .arg("--from")
        .arg("https://files.pythonhosted.org/packages/61/80/ffe1da13ad9300f87c93af113edd0638c75138c42a0994becfacac078c06/flask-3.0.3-py3-none-any.whl")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    Resolved 7 packages in [TIME]
    Prepared 7 packages in [TIME]
    Installed 7 packages in [TIME]
     + blinker==1.7.0
     + click==8.1.7
     + flask==3.0.3 (from https://files.pythonhosted.org/packages/61/80/ffe1da13ad9300f87c93af113edd0638c75138c42a0994becfacac078c06/flask-3.0.3-py3-none-any.whl)
     + itsdangerous==2.1.2
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + werkzeug==3.0.1
    Installed: flask
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
        // Should run flask in the virtual environment
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
        requirements = ["flask @ https://files.pythonhosted.org/packages/61/80/ffe1da13ad9300f87c93af113edd0638c75138c42a0994becfacac078c06/flask-3.0.3-py3-none-any.whl"]
        entrypoints = [
            { name = "flask", install-path = "[TEMP_DIR]/bin/flask" },
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

    // Install `flask`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("flask")
        .arg("--with")
        .arg("https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    Resolved 8 packages in [TIME]
    Prepared 8 packages in [TIME]
    Installed 8 packages in [TIME]
     + blinker==1.7.0
     + click==8.1.7
     + flask==3.0.2
     + iniconfig==2.0.0 (from https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl)
     + itsdangerous==2.1.2
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + werkzeug==3.0.1
    Installed: flask
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
        // Should run flask in the virtual environment
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
        requirements = [
            "flask",
            "iniconfig @ https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl",
        ]
        entrypoints = [
            { name = "flask", install-path = "[TEMP_DIR]/bin/flask" },
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

    // Install `flask`.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("flask==3.0.0")
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
     + flask==3.0.0
     + itsdangerous==2.1.2
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + werkzeug==3.0.1
    Installed: flask
    "###);

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("flask").join("uv-receipt.toml")).unwrap(), @r###"
        [tool]
        requirements = ["flask==3.0.0"]
        entrypoints = [
            { name = "flask", install-path = "[TEMP_DIR]/bin/flask" },
        ]
        "###);
    });

    // Install without the constraint. It should be replaced, but the package shouldn't be installed
    // since it's already satisfied in the environment.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("flask")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    Installed: flask
    "###);

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("flask").join("uv-receipt.toml")).unwrap(), @r###"
        [tool]
        requirements = ["flask"]
        entrypoints = [
            { name = "flask", install-path = "[TEMP_DIR]/bin/flask" },
        ]
        "###);
    });

    // Install with a `with`. It should be added to the environment.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("flask")
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
    Installed: flask
    "###);

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("flask").join("uv-receipt.toml")).unwrap(), @r###"
        [tool]
        requirements = [
            "flask",
            "iniconfig @ https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl",
        ]
        entrypoints = [
            { name = "flask", install-path = "[TEMP_DIR]/bin/flask" },
        ]
        "###);
    });

    // Install with `--upgrade`. `flask` should be reinstalled with a more recent version, and
    // `iniconfig` should be removed.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("flask")
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
     - flask==3.0.0
     + flask==3.0.2
     - iniconfig==2.0.0 (from https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl)
    Installed: flask
    "###);

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("flask").join("uv-receipt.toml")).unwrap(), @r###"
        [tool]
        requirements = ["flask"]
        entrypoints = [
            { name = "flask", install-path = "[TEMP_DIR]/bin/flask" },
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

    // Install `flask`.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("-p")
        .arg("3.12")
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

    // Install with Python 3.12 (compatible).
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("-p")
        .arg("3.12")
        .arg("flask")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    Tool `flask` is already installed
    "###);

    // Install with Python 3.11 (incompatible).
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("-p")
        .arg("3.11")
        .arg("flask")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    Existing environment for `flask` does not satisfy the requested Python interpreter: `Python 3.11`
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
}
