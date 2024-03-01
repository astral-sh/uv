use std::process::Command;

use anyhow::Result;
use assert_fs::fixture::PathChild;
use assert_fs::fixture::{FileTouch, FileWriteStr};
use url::Url;

use common::uv_snapshot;

use crate::common::{get_bin, TestContext, EXCLUDE_NEWER, INSTA_FILTERS};

mod common;

/// Create a `pip install` command with options shared across scenarios.
fn command(context: &TestContext) -> Command {
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
    command
}
