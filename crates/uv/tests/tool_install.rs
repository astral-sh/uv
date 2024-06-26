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
    let context = TestContext::new("3.12");
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Installed: black, blackd

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    Resolved 6 packages in [TIME]
    Prepared 6 packages in [TIME]
    Installed 6 packages in [TIME]
     + black==24.3.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
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
        requirements = ["black"]
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
    uv_snapshot!(context.filters(), cached_windows_filters=true, context.tool_install()
        .arg("flask")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Installed: flask

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    Resolved 7 packages in [TIME]
    Prepared 6 packages in [TIME]
    Installed 7 packages in [TIME]
     + blinker==1.7.0
     + click==8.1.7
     + flask==3.0.2
     + itsdangerous==2.1.2
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + werkzeug==3.0.1
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
        "###);
    });
}

/// Test installing and reinstalling an already installed tool
#[test]
fn tool_install_already_installed() {
    let context = TestContext::new("3.12");
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Drop resolved counts, they differ on Windows and are not relevant here
    let filters = context
        .filters()
        .into_iter()
        .chain([("Resolved [0-9] packages", "Resolved [COUNT] packages")])
        .collect::<Vec<_>>();

    // Install `black`
    uv_snapshot!(filters, context.tool_install()
        .arg("black")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Installed: black, blackd

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    Resolved [COUNT] packages in [TIME]
    Prepared 6 packages in [TIME]
    Installed 6 packages in [TIME]
     + black==24.3.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
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
        requirements = ["black"]
        "###);
    });

    // Install `black` again
    uv_snapshot!(filters, context.tool_install()
        .arg("black")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    Tool `black` is already installed
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
        requirements = ["black"]
        "###);
    });

    // Install `black` again with the `--reinstall` flag
    // We should recreate the entire environment and reinstall the entry points
    uv_snapshot!(filters, context.tool_install()
        .arg("black")
        .arg("--reinstall")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Installed: black, blackd

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    Resolved [COUNT] packages in [TIME]
    Installed 6 packages in [TIME]
     + black==24.3.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    "###);

    // Install `black` again with `--reinstall-package` for `black`
    // We should reinstall `black` in the environment and reinstall the entry points
    uv_snapshot!(filters, context.tool_install()
        .arg("black")
        .arg("--reinstall-package")
        .arg("black")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Installed: black, blackd

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    Resolved [COUNT] packages in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - black==24.3.0
     + black==24.3.0
    "###);

    // Install `black` again with `--reinstall-package` for a dependency
    // We should reinstall `click` in the environment but not reinstall the entry points
    uv_snapshot!(filters, context.tool_install()
        .arg("black")
        .arg("--reinstall-package")
        .arg("click")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    Resolved [COUNT] packages in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - click==8.1.7
     + click==8.1.7
    Updated environment for tool `black`
    "###);
}

/// Test installing a tool when its entry point already exists
#[test]
fn tool_install_entry_point_exists() {
    let context = TestContext::new("3.12");
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    let executable = bin_dir.child(format!("black{}", std::env::consts::EXE_SUFFIX));
    executable.touch().unwrap();

    // Drop executable suffixes for cross-platform snapshots
    // Drop resolved counts, they differ on Windows and are not relevant here
    let filters = context
        .filters()
        .into_iter()
        .chain([
            (std::env::consts::EXE_SUFFIX, ""),
            ("Resolved [0-9] packages", "Resolved [COUNT] packages"),
        ])
        .collect::<Vec<_>>();

    // Attempt to install `black`
    uv_snapshot!(filters, context.tool_install()
        .arg("black")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    Resolved [COUNT] packages in [TIME]
    Prepared 6 packages in [TIME]
    Installed 6 packages in [TIME]
     + black==24.3.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    error: Entry point for tool already exists: black (use `--force` to overwrite)
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
    uv_snapshot!(filters, context.tool_install()
        .arg("black")
        .arg("--reinstall")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    Resolved [COUNT] packages in [TIME]
    Installed 6 packages in [TIME]
     + black==24.3.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    error: Entry point for tool already exists: black (use `--force` to overwrite)
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
    uv_snapshot!(filters, context.tool_install()
        .arg("black")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    Resolved [COUNT] packages in [TIME]
    Installed 6 packages in [TIME]
     + black==24.3.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    error: Entry points for tool already exist: black, blackd (use `--force` to overwrite)
    "###);

    // Install `black` with `--force`
    uv_snapshot!(filters, context.tool_install()
        .arg("black")
        .arg("--force")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Installed: black, blackd

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    Resolved [COUNT] packages in [TIME]
    Installed 6 packages in [TIME]
     + black==24.3.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    "###);

    tool_dir.child("black").assert(predicate::path::is_dir());

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We write a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r###"
        [tool]
        requirements = ["black"]
        "###);
    });

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
        requirements = ["black"]
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
    let context = TestContext::new("3.12");
    let tool_dir = context.temp_dir.child("tools");

    // Install `black`
    uv_snapshot!(context.filters(), context.tool_install().arg("black").env("UV_TOOL_DIR", tool_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Installed: black, blackd

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    Resolved 6 packages in [TIME]
    Prepared 6 packages in [TIME]
    Installed 6 packages in [TIME]
     + black==24.3.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    "###);

    context
        .home_dir
        .child(format!(".local/bin/black{}", std::env::consts::EXE_SUFFIX))
        .assert(predicate::path::exists());
}

/// Test `uv tool install` when the bin directory is inferred from `$XDG_DATA_HOME`
#[test]
fn tool_install_xdg_data_home() {
    let context = TestContext::new("3.12");
    let tool_dir = context.temp_dir.child("tools");
    let data_home = context.temp_dir.child("data/home");

    // Install `black`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_DATA_HOME", data_home.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Installed: black, blackd

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    Resolved 6 packages in [TIME]
    Prepared 6 packages in [TIME]
    Installed 6 packages in [TIME]
     + black==24.3.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    "###);

    context
        .temp_dir
        .child(format!("data/bin/black{}", std::env::consts::EXE_SUFFIX))
        .assert(predicate::path::exists());
}

/// Test `uv tool install` when the bin directory is set by `$XDG_BIN_HOME`
#[test]
fn tool_install_xdg_bin_home() {
    let context = TestContext::new("3.12");
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Installed: black, blackd

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    Resolved 6 packages in [TIME]
    Prepared 6 packages in [TIME]
    Installed 6 packages in [TIME]
     + black==24.3.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    "###);

    bin_dir
        .child(format!("black{}", std::env::consts::EXE_SUFFIX))
        .assert(predicate::path::exists());
}
