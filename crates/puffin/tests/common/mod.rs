// The `unreachable_pub` is to silence false positives in RustRover.
#![allow(dead_code, unreachable_pub)]

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use assert_fs::assert::PathAssert;
use assert_fs::fixture::PathChild;
use assert_fs::TempDir;
use insta_cmd::get_cargo_bin;

pub const BIN_NAME: &str = "puffin";

// Exclude any packages uploaded after this date.
pub static EXCLUDE_NEWER: &str = "2023-11-18T12:00:00Z";

pub const INSTA_FILTERS: &[(&str, &str)] = &[
    (r"--cache-dir [^\s]+", "--cache-dir [CACHE_DIR]"),
    // Operation times
    (r"(\d+\.)?\d+(ms|s)", "[TIME]"),
    // Puffin versions
    (r"v\d+\.\d+\.\d+", "v[VERSION]"),
    // File sizes
    (r"(\d+\.)?\d+([KM]i)?B", "[SIZE]"),
    // Rewrite Windows output to Unix output
    (r"\\([\w\d])", "/$1"),
    (r"puffin.exe", "puffin"),
    // The exact message is host language dependent
    (
        r"Caused by: .* \(os error 2\)",
        "Caused by: No such file or directory (os error 2)",
    ),
];

#[derive(Debug)]
pub struct TestContext {
    pub temp_dir: TempDir,
    pub cache_dir: TempDir,
    pub venv: PathBuf,
}

impl TestContext {
    pub fn new(python_version: &str) -> Self {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let cache_dir = TempDir::new().expect("Failed to create temp dir");
        let venv = create_venv(&temp_dir, &cache_dir, python_version);
        Self {
            temp_dir,
            cache_dir,
            venv,
        }
    }

    /// Set shared defaults between tests:
    /// * Set the current directory to a temporary directory (`temp_dir`).
    /// * Set the cache dir to a different temporary directory (`cache_dir`).
    /// * Set a cutoff for versions used in the resolution so the snapshots don't change after a new release.
    /// * Set the venv to a fresh `.venv` in `temp_dir`.
    pub fn compile(&self) -> std::process::Command {
        let mut cmd = std::process::Command::new(get_cargo_bin(BIN_NAME));
        cmd.arg("pip")
            .arg("compile")
            .arg("--cache-dir")
            .arg(self.cache_dir.path())
            .arg("--exclude-newer")
            .arg(EXCLUDE_NEWER)
            .env("VIRTUAL_ENV", self.venv.as_os_str())
            .current_dir(self.temp_dir.path());
        cmd
    }
}

pub fn venv_to_interpreter(venv: &Path) -> PathBuf {
    if cfg!(unix) {
        venv.join("bin").join("python")
    } else if cfg!(windows) {
        venv.join("Scripts").join("python.exe")
    } else {
        unimplemented!("Only Windows and Unix are supported")
    }
}

/// Create a virtual environment named `.venv` in a temporary directory with the given
/// Python version. Expected format for `python` is "python<version>".
pub fn create_venv(temp_dir: &TempDir, cache_dir: &TempDir, python: &str) -> PathBuf {
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
