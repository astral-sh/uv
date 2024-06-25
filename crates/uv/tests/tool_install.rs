#![cfg(all(feature = "python", feature = "pypi"))]

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
        .child("tools.toml")
        .assert(predicate::path::exists());

    let executable = bin_dir.child(format!("black{}", std::env::consts::EXE_SUFFIX));
    assert!(executable.exists());

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
        // We should have a tool entry
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("tools.toml")).unwrap(), @r###"
        [tools]
        black = { requirements = ["black"] }
        "###);
    });

    // Install another tool
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("pytest")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        // Check that we support `XDG_BIN_HOME`
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    Resolved 4 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 4 packages in [TIME]
     + iniconfig==2.0.0
     + packaging==24.0
     + pluggy==1.4.0
     + pytest==8.1.1
    "###);

    tool_dir.child("pytest").assert(predicate::path::is_dir());
    assert!(bin_dir
        .child(format!("pytest{}", std::env::consts::EXE_SUFFIX))
        .exists());

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(fs_err::read_to_string(bin_dir.join("pytest")).unwrap(), @r###"
        #![TEMP_DIR]/tools/pytest/bin/python
        # -*- coding: utf-8 -*-
        import re
        import sys
        from pytest import console_main
        if __name__ == "__main__":
            sys.argv[0] = re.sub(r"(-script\.pyw|\.exe)?$", "", sys.argv[0])
            sys.exit(console_main())
        "###);

    });

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have an additional tool entry
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("tools.toml")).unwrap(), @r###"
        [tools]
        black = { requirements = ["black"] }
        pytest = { requirements = ["pytest"] }
        "###);
    });
}

/// Test installing a tool twice with `uv tool install`
#[test]
fn tool_install_twice() {
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
        .child("tools.toml")
        .assert(predicate::path::exists());

    let executable = bin_dir.child(format!("black{}", std::env::consts::EXE_SUFFIX));
    assert!(executable.exists());

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
        // We should have a tool entry
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("tools.toml")).unwrap(), @r###"
        [tools]
        black = { requirements = ["black"] }
        "###);
    });

    // Install `black` again
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool install` is experimental and may change without warning.
    Tool `black` is already installed.
    "###);

    tool_dir.child("black").assert(predicate::path::is_dir());
    bin_dir
        .child(format!("black{}", std::env::consts::EXE_SUFFIX))
        .assert(predicate::path::exists());

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should not have an additional tool entry
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("tools.toml")).unwrap(), @r###"
        [tools]
        black = { requirements = ["black"] }
        "###);
    });
}

/// Test installing a tool when its entry pint already exists
#[test]
fn tool_install_entry_point_exists() {
    let context = TestContext::new("3.12");
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    let executable = bin_dir.child(format!("black{}", std::env::consts::EXE_SUFFIX));
    executable.touch().unwrap();

    // Install `black`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .env("UV_TOOL_DIR", tool_dir.as_os_str())
        .env("XDG_BIN_HOME", bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

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

    // TODO(zanieb): We happily overwrite entry points by default right now
    // https://github.com/astral-sh/uv/pull/4501 should resolve this

    // We should not create a virtual environment
    // assert!(tool_dir.child("black").exists());

    // // We should not write a tools entry
    // assert!(!tool_dir.join("tools.toml").exists());

    // insta::with_settings!({
    //     filters => context.filters(),
    // }, {
    //     // Nor should we change the `black` entry point that exists
    //     assert_snapshot!(fs_err::read_to_string(bin_dir.join("black")).unwrap(), @"");

    // });
}

/// Test `uv tool install` when the bin directory is inferred from `$HOME`
#[test]
fn tool_install_home() {
    let context = TestContext::new("3.12");
    let tool_dir = context.temp_dir.child("tools");

    // Install `black`
    uv_snapshot!(context.filters(), context.tool_install().arg("black").env("UV_TOOL_DIR", tool_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

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
        .child("data/bin/black")
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
