// The `unreachable_pub` is to silence false positives in RustRover.
#![allow(dead_code, unreachable_pub)]

use std::path::{Path, PathBuf};
use std::process::Output;

use assert_cmd::assert::{Assert, OutputAssertExt};
use assert_cmd::Command;
use assert_fs::assert::PathAssert;
use assert_fs::fixture::PathChild;
use assert_fs::TempDir;

// Exclude any packages uploaded after this date.
pub static EXCLUDE_NEWER: &str = "2023-11-18T12:00:00Z";

pub const INSTA_FILTERS: &[(&str, &str)] = &[
    (r"--cache-dir [^\s]+", "--cache-dir [CACHE_DIR]"),
    // Operation times
    (r"(\d+m )?(\d+\.)?\d+(ms|s)", "[TIME]"),
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
        let mut cmd = std::process::Command::new(get_bin());
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

    /// Run the given python code and check whether it succeeds.
    pub fn assert_command(&self, command: &str) -> Assert {
        std::process::Command::new(venv_to_interpreter(&self.venv))
            // Our tests change files in <1s, so we must disable CPython bytecode caching or we'll get stale files
            // https://github.com/python/cpython/issues/75953
            .arg("-B")
            .arg("-c")
            .arg(command)
            .current_dir(&self.temp_dir)
            .assert()
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

/// If bootstrapped python build standalone pythons exists in `<project root>/bin`,
/// return the paths to the directories containing the python binaries (i.e. as paths that
/// `which::which_in` can use).
///
/// Use `scripts/bootstrap/install.py` to bootstrap.
///
/// Python versions are sorted from newest to oldest.
pub fn bootstrapped_pythons() -> Option<Vec<PathBuf>> {
    // Current dir is `<project root>/crates/puffin`.
    let bootstrapped_pythons = std::env::current_dir()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("bin")
        .join("versions");
    let Ok(bootstrapped_pythons) = fs_err::read_dir(bootstrapped_pythons) else {
        return None;
    };

    let mut bootstrapped_pythons: Vec<PathBuf> = bootstrapped_pythons
        .map(Result::unwrap)
        .filter(|entry| entry.metadata().unwrap().is_dir())
        .map(|entry| {
            if cfg!(unix) {
                entry.path().join("install").join("bin")
            } else if cfg!(windows) {
                entry.path().join("install")
            } else {
                unimplemented!("Only Windows and Unix are supported")
            }
        })
        .collect();
    bootstrapped_pythons.sort();
    // Prefer the most recent patch version.
    bootstrapped_pythons.reverse();
    Some(bootstrapped_pythons)
}

/// Create a virtual environment named `.venv` in a temporary directory with the given
/// Python version. Expected format for `python` is "python<version>".
pub fn create_venv(temp_dir: &TempDir, cache_dir: &TempDir, python: &str) -> PathBuf {
    let python = if let Some(bootstrapped_pythons) = bootstrapped_pythons() {
        bootstrapped_pythons
            .into_iter()
            // Good enough since we control the directory
            .find(|path| path.to_str().unwrap().contains(&format!("@{python}")))
            .expect("Missing python bootstrap version")
            .join(if cfg!(unix) {
                "python3"
            } else if cfg!(windows) {
                "python.exe"
            } else {
                unimplemented!("Only Windows and Unix are supported")
            })
    } else {
        PathBuf::from(python)
    };
    let venv = temp_dir.child(".venv");
    Command::new(get_bin())
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

/// Returns the puffin binary that cargo built before launching the tests.
///
/// <https://doc.rust-lang.org/cargo/reference/environment-variables.html#environment-variables-cargo-sets-for-crates>
pub fn get_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_puffin"))
}

/// Execute the command and format its output status, stdout and stderr into a snapshot string.
///
/// This function is derived from `insta_cmd`s `spawn_with_info`.
pub fn run_and_format(command: &mut std::process::Command) -> (String, Output) {
    let program = command.get_program().to_string_lossy().to_string();
    let output = command
        .output()
        .unwrap_or_else(|_| panic!("Failed to spawn {program}"));

    let snapshot = format!(
        "success: {:?}\nexit_code: {}\n----- stdout -----\n{}\n----- stderr -----\n{}",
        output.status.success(),
        output.status.code().unwrap_or(!0),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    (snapshot, output)
}

/// Run [`assert_cmd_snapshot!`], with default filters or with custom filters.
#[allow(unused_macros)]
macro_rules! puffin_snapshot {
    ($spawnable:expr, @$snapshot:literal) => {{
        puffin_snapshot!($crate::common::INSTA_FILTERS.to_vec(), $spawnable, @$snapshot)
    }};
    ($filters:expr, $spawnable:expr, @$snapshot:literal) => {{
        let (snapshot, output) = $crate::common::run_and_format($spawnable);
        ::insta::with_settings!({
            filters => $filters.to_vec()
        }, {
            ::insta::assert_snapshot!(snapshot, @$snapshot);
        });
        output
    }};
}

/// <https://stackoverflow.com/a/31749071/3549270>
#[allow(unused_imports)]
pub(crate) use puffin_snapshot;
