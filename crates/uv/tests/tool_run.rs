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
