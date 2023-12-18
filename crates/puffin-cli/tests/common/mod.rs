#![allow(dead_code)]

use assert_cmd::Command;
use assert_fs::assert::PathAssert;
use assert_fs::fixture::PathChild;
use assert_fs::TempDir;
use insta_cmd::get_cargo_bin;
use std::path::PathBuf;

pub(crate) const BIN_NAME: &str = "puffin";

pub(crate) const INSTA_FILTERS: &[(&str, &str)] = &[
    (r"(\d+\.)?\d+(ms|s)", "[TIME]"),
    (r"--cache-dir .*", "--cache-dir [CACHE_DIR]"),
];

/// Create a virtual environment named `.venv` in a temporary directory.
pub(crate) fn create_venv_py312(temp_dir: &TempDir, cache_dir: &TempDir) -> PathBuf {
    let venv = temp_dir.child(".venv");
    Command::new(get_cargo_bin(BIN_NAME))
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .arg("--python")
        .arg("python3.12")
        .current_dir(temp_dir)
        .assert()
        .success();
    venv.assert(predicates::path::is_dir());
    venv.to_path_buf()
}
