#![cfg(all(feature = "python", feature = "pypi"))]

use common::{uv_snapshot, TestContext};

mod common;

#[test]
fn tool_run_args() {
    let context = TestContext::new("3.12");

    // We treat arguments before the command as uv arguments
    uv_snapshot!(context.filters(), context.tool_run().arg("--version").arg("pytest"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    uv [VERSION] ([COMMIT] DATE)

    ----- stderr -----
    "###);

    // We don't treat arguments after the command as uv arguments
    uv_snapshot!(context.filters(), context.tool_run().arg("pytest").arg("--version"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    pytest 8.1.1

    ----- stderr -----
    warning: `uv tool run` is experimental and may change without warning.
    Resolved 4 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + iniconfig==2.0.0
     + packaging==24.0
     + pluggy==1.4.0
     + pytest==8.1.1
    "###);

    // Can use `--` to separate uv arguments from the command arguments.
    uv_snapshot!(context.filters(), context.tool_run().arg("--").arg("pytest").arg("--version"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    pytest 8.1.1

    ----- stderr -----
    warning: `uv tool run` is experimental and may change without warning.
    Resolved 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + iniconfig==2.0.0
     + packaging==24.0
     + pluggy==1.4.0
     + pytest==8.1.1
    "###);
}

#[test]
fn tool_run_at_version() {
    let context = TestContext::new("3.12");

    uv_snapshot!(context.filters(), context.tool_run().arg("pytest@8.0.0").arg("--version"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    pytest 8.0.0

    ----- stderr -----
    warning: `uv tool run` is experimental and may change without warning.
    Resolved 4 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + iniconfig==2.0.0
     + packaging==24.0
     + pluggy==1.4.0
     + pytest==8.0.0
    "###);

    // Empty versions are just treated as package and command names
    uv_snapshot!(context.filters(), context.tool_run().arg("pytest@").arg("--version"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool run` is experimental and may change without warning.
    error: Failed to parse: `pytest@`
      Caused by: Expected URL
    pytest@
           ^
    "###);

    // Invalid versions are just treated as package and command names
    uv_snapshot!(context.filters(), context.tool_run().arg("pytest@invalid").arg("--version"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool run` is experimental and may change without warning.
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
    uv_snapshot!(filters, context.tool_run().arg("--from").arg("pytest").arg("pytest@8.0.0").arg("--version"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: `uv tool run` is experimental and may change without warning.
    Resolved 4 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 4 packages in [TIME]
     + iniconfig==2.0.0
     + packaging==24.0
     + pluggy==1.4.0
     + pytest==8.1.1
    error: Failed to spawn: `pytest@8.0.0`
      Caused by: No such file or directory (os error 2)
    "###);
}

#[test]
fn tool_run_from_version() {
    let context = TestContext::new("3.12");

    uv_snapshot!(context.filters(), context.tool_run().arg("--from").arg("pytest==8.0.0").arg("pytest").arg("--version"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    pytest 8.0.0

    ----- stderr -----
    warning: `uv tool run` is experimental and may change without warning.
    Resolved 4 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + iniconfig==2.0.0
     + packaging==24.0
     + pluggy==1.4.0
     + pytest==8.0.0
    "###);
}
