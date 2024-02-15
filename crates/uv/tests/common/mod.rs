// The `unreachable_pub` is to silence false positives in RustRover.
#![allow(dead_code, unreachable_pub)]

use std::borrow::BorrowMut;
use std::env;
use std::path::{Path, PathBuf};
use std::process::Output;

use assert_cmd::assert::{Assert, OutputAssertExt};
use assert_cmd::Command;
use assert_fs::assert::PathAssert;
use assert_fs::fixture::PathChild;
#[cfg(unix)]
use fs_err::os::unix::fs::symlink as symlink_file;
#[cfg(windows)]
use fs_err::os::windows::fs::symlink_file;
use platform_host::Platform;
use regex::Regex;
use uv_cache::Cache;

use uv_interpreter::find_requested_python;

// Exclude any packages uploaded after this date.
pub static EXCLUDE_NEWER: &str = "2023-11-18T12:00:00Z";

pub const INSTA_FILTERS: &[(&str, &str)] = &[
    (r"--cache-dir [^\s]+", "--cache-dir [CACHE_DIR]"),
    // Operation times
    (r"(\d+m )?(\d+\.)?\d+(ms|s)", "[TIME]"),
    // uv versions
    (r"v\d+\.\d+\.\d+-prerelease\.\d+", "v[VERSION]"),
    (r"v\d+\.\d+\.\d+", "v[VERSION]"),
    // File sizes
    (r"(\d+\.)?\d+([KM]i)?B", "[SIZE]"),
    // Rewrite Windows output to Unix output
    (r"\\([\w\d])", "/$1"),
    (r"uv.exe", "uv"),
    // The exact message is host language dependent
    (
        r"Caused by: .* \(os error 2\)",
        "Caused by: No such file or directory (os error 2)",
    ),
];

#[derive(Debug)]
pub struct TestContext {
    pub temp_dir: assert_fs::TempDir,
    pub cache_dir: assert_fs::TempDir,
    pub venv: PathBuf,
}

impl TestContext {
    pub fn new(python_version: &str) -> Self {
        let temp_dir = assert_fs::TempDir::new().expect("Failed to create temp dir");
        let cache_dir = assert_fs::TempDir::new().expect("Failed to create temp dir");
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
    // Current dir is `<project root>/crates/uv`.
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
pub fn create_venv(
    temp_dir: &assert_fs::TempDir,
    cache_dir: &assert_fs::TempDir,
    python: &str,
) -> PathBuf {
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

/// Returns the uv binary that cargo built before launching the tests.
///
/// <https://doc.rust-lang.org/cargo/reference/environment-variables.html#environment-variables-cargo-sets-for-crates>
pub fn get_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_uv"))
}

/// Create a directory with the requested Python binaries available.
pub fn create_bin_with_executables(
    temp_dir: &assert_fs::TempDir,
    python_versions: &[&str],
) -> anyhow::Result<PathBuf> {
    if let Some(bootstrapped_pythons) = bootstrapped_pythons() {
        let selected_pythons = bootstrapped_pythons.into_iter().filter(|path| {
            python_versions.iter().any(|python_version| {
                // Good enough since we control the directory
                path.to_str()
                    .unwrap()
                    .contains(&format!("@{python_version}"))
            })
        });
        return Ok(env::join_paths(selected_pythons)?.into());
    }

    let bin = temp_dir.child("bin");
    fs_err::create_dir(&bin)?;
    for &request in python_versions {
        let interpreter = find_requested_python(
            request,
            &Platform::current().unwrap(),
            &Cache::temp().unwrap(),
        )?
        .ok_or(uv_interpreter::Error::NoSuchPython(request.to_string()))?;
        let name = interpreter
            .sys_executable()
            .file_name()
            .expect("Discovered executable must have a filename");
        symlink_file(interpreter.sys_executable(), bin.child(name))?;
    }
    Ok(bin.canonicalize()?)
}

/// Execute the command and format its output status, stdout and stderr into a snapshot string.
///
/// This function is derived from `insta_cmd`s `spawn_with_info`.
pub fn run_and_format<'a>(
    mut command: impl BorrowMut<std::process::Command>,
    filters: impl AsRef<[(&'a str, &'a str)]>,
    windows_filters: bool,
) -> (String, Output) {
    let program = command
        .borrow_mut()
        .get_program()
        .to_string_lossy()
        .to_string();
    let output = command
        .borrow_mut()
        .output()
        .unwrap_or_else(|_| panic!("Failed to spawn {program}"));

    let mut snapshot = format!(
        "success: {:?}\nexit_code: {}\n----- stdout -----\n{}\n----- stderr -----\n{}",
        output.status.success(),
        output.status.code().unwrap_or(!0),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    for (matcher, replacement) in filters.as_ref() {
        // TODO(konstin): Cache regex compilation
        let re = Regex::new(matcher).expect("Do you need to regex::escape your filter?");
        if re.is_match(&snapshot) {
            snapshot = re.replace_all(&snapshot, *replacement).to_string();
        }
    }

    if cfg!(windows) && windows_filters {
        // The optional leading +/- is for install logs, the optional next line is for lock files
        let windows_only_deps = [
            ("( [+-] )?colorama==\\d+(\\.[\\d+])+\n(    # via .*\n)?"),
            ("( [+-] )?tzdata==\\d+(\\.[\\d+])+\n(    # via .*\n)?"),
        ];
        let mut removed_packages = 0;
        for windows_only_dep in windows_only_deps {
            // TODO(konstin): Cache regex compilation
            let re = Regex::new(windows_only_dep).unwrap();
            if re.is_match(&snapshot) {
                snapshot = re.replace(&snapshot, "").to_string();
                removed_packages += 1;
            }
        }
        if removed_packages > 0 {
            for i in 1..20 {
                snapshot = snapshot.replace(
                    &format!("{} packages", i + removed_packages),
                    &format!("{} package{}", i, if i > 1 { "s" } else { "" }),
                );
            }
        }
    }

    (snapshot, output)
}

/// Run [`assert_cmd_snapshot!`], with default filters or with custom filters.
///
/// By default, the filters will search for the generally windows-only deps colorama and tzdata,
/// filter them out and decrease the package counts by one for each match.
#[allow(unused_macros)]
macro_rules! uv_snapshot {
    ($spawnable:expr, @$snapshot:literal) => {{
        uv_snapshot!($crate::common::INSTA_FILTERS.to_vec(), $spawnable, @$snapshot)
    }};
    ($filters:expr, $spawnable:expr, @$snapshot:literal) => {{
        // Take a reference for backwards compatibility with the vec-expecting insta filters.
        let (snapshot, output) = $crate::common::run_and_format($spawnable, &$filters, true);
        ::insta::assert_snapshot!(snapshot, @$snapshot);
        output
    }};
    ($filters:expr, windows_filters=false, $spawnable:expr, @$snapshot:literal) => {{
        // Take a reference for backwards compatibility with the vec-expecting insta filters.
        let (snapshot, output) = $crate::common::run_and_format($spawnable, &$filters, false);
        ::insta::assert_snapshot!(snapshot, @$snapshot);
        output
    }};
}

/// <https://stackoverflow.com/a/31749071/3549270>
#[allow(unused_imports)]
pub(crate) use uv_snapshot;
