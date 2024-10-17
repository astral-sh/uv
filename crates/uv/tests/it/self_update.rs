use std::process::Command;

use axoupdater::{
    test::helpers::{perform_runtest, RuntestArgs},
    ReleaseSourceType,
};

use uv_static::EnvVars;

use crate::common::get_bin;

#[test]
fn check_self_update() {
    // To maximally emulate behaviour in practice, this test actually modifies CARGO_HOME
    // and therefore should only be run in CI by default, where it can't hurt developers.
    // We use the "CI" env-var that CI machines tend to run
    if std::env::var(EnvVars::CI)
        .map(|s| s.is_empty())
        .unwrap_or(true)
    {
        return;
    }

    // Configure the runtest
    let args = RuntestArgs {
        app_name: "uv".to_owned(),
        package: "uv".to_owned(),
        owner: "astral-sh".to_owned(),
        bin: get_bin(),
        binaries: vec!["uv".to_owned()],
        args: vec!["self".to_owned(), "update".to_owned()],
        release_type: ReleaseSourceType::GitHub,
    };

    // install and update the application
    let installed_bin = perform_runtest(&args);

    // check that the binary works like normal
    let status = Command::new(installed_bin)
        .arg("--version")
        .status()
        .expect("failed to run 'uv --version'");
    assert!(status.success(), "'uv --version' returned non-zero");
}
