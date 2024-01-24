#![allow(dead_code)]

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use assert_fs::assert::PathAssert;
use assert_fs::fixture::PathChild;
use assert_fs::TempDir;
use insta_cmd::get_cargo_bin;

pub(crate) const BIN_NAME: &str = "puffin";

pub(crate) const INSTA_FILTERS: &[(&str, &str)] = &[
    (r"--cache-dir .*", "--cache-dir [CACHE_DIR]"),
    (r"(\d+\.)?\d+(ms|s)", "[TIME]"),
    (r"v\d+\.\d+\.\d+", "v[VERSION]"),
    // Rewrite Windows output to Unix output
    (r"\\([\w\d])", "/$1"),
    (r"puffin.exe", "puffin"),
    // The exact message is host language dependent
    (
        r"Caused by: .* \(os error 2\)",
        "Caused by: No such file or directory (os error 2)",
    ),
];

pub(crate) fn venv_to_interpreter(venv: &Path) -> PathBuf {
    if cfg!(unix) {
        venv.join("bin").join("python")
    } else if cfg!(windows) {
        venv.join("Scripts").join("python.exe")
    } else {
        unimplemented!("Only Windows and Unix are supported")
    }
}

/// Create a virtual environment named `.venv` in a temporary directory.
pub(crate) fn create_venv_py312(temp_dir: &TempDir, cache_dir: &TempDir) -> PathBuf {
    create_venv(temp_dir, cache_dir, "3.12")
}

/// Create a virtual environment named `.venv` in a temporary directory with the given
/// Python version. Expected format for `python` is "python<version>".
pub(crate) fn create_venv(temp_dir: &TempDir, cache_dir: &TempDir, python: &str) -> PathBuf {
    let venv = temp_dir.child(".venv");
    Command::new(get_cargo_bin(BIN_NAME))
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .arg("--python")
        .arg(python)
        .current_dir(temp_dir)
        .assert()
        .success();
    venv.assert(predicates::path::is_dir());
    venv.to_path_buf()
}
