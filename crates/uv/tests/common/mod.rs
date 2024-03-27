// The `unreachable_pub` is to silence false positives in RustRover.
#![allow(dead_code, unreachable_pub)]

use assert_cmd::assert::{Assert, OutputAssertExt};
use assert_cmd::Command;
use assert_fs::assert::PathAssert;
use assert_fs::fixture::PathChild;
#[cfg(unix)]
use fs_err::os::unix::fs::symlink as symlink_file;
#[cfg(windows)]
use fs_err::os::windows::fs::symlink_file;
use regex::Regex;
use std::borrow::BorrowMut;
use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Output;
use uv_fs::Simplified;

use uv_cache::Cache;
use uv_interpreter::find_requested_python;

// Exclude any packages uploaded after this date.
pub static EXCLUDE_NEWER: &str = "2024-03-25T00:00:00Z";

pub const INSTA_FILTERS: &[(&str, &str)] = &[
    (r"--cache-dir [^\s]+", "--cache-dir [CACHE_DIR]"),
    // Operation times
    (r"(\s|\()(\d+m )?(\d+\.)?\d+(ms|s)", "$1[TIME]"),
    // File sizes
    (r"(\s|\()(\d+\.)?\d+([KM]i)?B", "$1[SIZE]"),
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
    pub python_version: String,
    pub workspace_root: PathBuf,

    // Standard filters for this test context
    filters: Vec<(String, String)>,
}

impl TestContext {
    pub fn new(python_version: &str) -> Self {
        let temp_dir = assert_fs::TempDir::new().expect("Failed to create temp dir");
        let cache_dir = assert_fs::TempDir::new().expect("Failed to create cache dir");
        let venv = create_venv(&temp_dir, &cache_dir, python_version);

        // The workspace root directory is not available without walking up the tree
        // https://github.com/rust-lang/cargo/issues/3946
        let workspace_root = Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
            .parent()
            .expect("CARGO_MANIFEST_DIR should be nested in workspace")
            .parent()
            .expect("CARGO_MANIFEST_DIR should be doubly nested in workspace")
            .to_path_buf();

        let site_packages = site_packages_path(&venv, format!("python{python_version}"));

        let mut filters = Vec::new();
        filters.extend(
            Self::path_patterns(&cache_dir)
                .into_iter()
                .map(|pattern| (pattern, "[CACHE_DIR]/".to_string())),
        );
        filters.extend(
            Self::path_patterns(&site_packages)
                .into_iter()
                .map(|pattern| (pattern, "[SITE_PACKAGES]/".to_string())),
        );
        filters.extend(
            Self::path_patterns(&venv)
                .into_iter()
                .map(|pattern| (pattern, "[VENV]/".to_string())),
        );
        filters.extend(
            Self::path_patterns(&temp_dir)
                .into_iter()
                .map(|pattern| (pattern, "[TEMP_DIR]/".to_string())),
        );
        filters.extend(
            Self::path_patterns(&workspace_root)
                .into_iter()
                .map(|pattern| (pattern, "[WORKSPACE]/".to_string())),
        );

        // Account for [`Simplified::user_display`] which is relative to the command working directory
        filters.push((
            Self::path_pattern(
                site_packages
                    .strip_prefix(&temp_dir)
                    .expect("The test site-packages directory is always in the tempdir"),
            ),
            "[SITE_PACKAGES]/".to_string(),
        ));
        filters.push((
            Self::path_pattern(
                venv.strip_prefix(&temp_dir)
                    .expect("The test virtual environment directory is always in the tempdir"),
            ),
            "[VENV]/".to_string(),
        ));

        // Filter non-deterministic temporary directory names
        // Note we apply this _after_ all the full paths to avoid breaking their matching
        filters.push((r"(\\|\/)\.tmp.*(\\|\/)".to_string(), "/[TMP]/".to_string()));

        // Account for platform prefix differences `file://` (Unix) vs `file:///` (Windows)
        filters.push((r"file:///".to_string(), "file://".to_string()));

        // Destroy any remaining UNC prefixes (Windows only)
        filters.push((r"\\\\\?\\".to_string(), String::new()));

        Self {
            temp_dir,
            cache_dir,
            venv,
            python_version: python_version.to_string(),
            filters,
            workspace_root,
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

        if cfg!(all(windows, debug_assertions)) {
            // TODO(konstin): Reduce stack usage in debug mode enough that the tests pass with the
            // default windows stack of 1MB
            cmd.env("UV_STACK_SIZE", (8 * 1024 * 1024).to_string());
        }

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

    /// Assert a package is installed with the given version.
    pub fn assert_installed(&self, package: &'static str, version: &'static str) {
        self.assert_command(
            format!("import {package} as package; print(package.__version__, end='')").as_str(),
        )
        .success()
        .stdout(version);
    }

    /// Generate various escaped regex patterns for the given path.
    fn path_patterns(path: impl AsRef<Path>) -> Vec<String> {
        let mut patterns = Vec::new();

        // We can only canonicalize paths that exist already
        if path.as_ref().exists() {
            patterns.push(Self::path_pattern(
                path.as_ref()
                    .canonicalize()
                    .expect("Failed to create canonical path"),
            ));
        }

        // Include a non-canonicalized version
        patterns.push(Self::path_pattern(path));

        patterns
    }

    /// Generate an escaped regex pattern for the given path.
    fn path_pattern(path: impl AsRef<Path>) -> String {
        format!(
            // Trim the trailing separator for cross-platform directories filters
            r"{}\\?/?",
            regex::escape(&path.as_ref().simplified_display().to_string())
                // Make separators platform agnostic because on Windows we will display
                // paths with Unix-style separators sometimes
                .replace(r"\\", r"(\\|\/)")
        )
    }

    /// Standard snapshot filters _plus_ those for this test context.
    pub fn filters(&self) -> Vec<(&str, &str)> {
        // Put test context snapshots before the default filters
        // This ensures we don't replace other patterns inside paths from the test context first
        self.filters
            .iter()
            .map(|(p, r)| (p.as_str(), r.as_str()))
            .chain(INSTA_FILTERS.iter().copied())
            .collect()
    }

    /// For when we add pypy to the test suite.
    #[allow(clippy::unused_self)]
    pub fn python_kind(&self) -> &str {
        "python"
    }

    /// Returns the site-packages folder inside the venv.
    pub fn site_packages(&self) -> PathBuf {
        site_packages_path(
            &self.venv,
            format!("{}{}", self.python_kind(), self.python_version),
        )
    }
}

fn site_packages_path(venv: &Path, python: String) -> PathBuf {
    if cfg!(unix) {
        venv.join("lib").join(python).join("site-packages")
    } else if cfg!(windows) {
        venv.join("Lib").join("site-packages")
    } else {
        unimplemented!("Only Windows and Unix are supported")
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
    let project_root = std::env::current_dir()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let boostrap_dir = if let Some(boostrap_dir) = env::var_os("UV_BOOTSTRAP_DIR") {
        let boostrap_dir = PathBuf::from(boostrap_dir);
        if boostrap_dir.is_absolute() {
            boostrap_dir
        } else {
            // cargo test changes directory to the test crate, but doesn't tell us from where the user is running the
            // tests. We'll assume that it's the project root.
            project_root.join(boostrap_dir)
        }
    } else {
        project_root.join("bin")
    };
    let bootstrapped_pythons = boostrap_dir.join("versions");
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
pub fn create_venv<Parent: assert_fs::prelude::PathChild + AsRef<std::path::Path>>(
    temp_dir: &Parent,
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

/// Create a `PATH` with the requested Python versions available in order.
pub fn create_bin_with_executables(
    temp_dir: &assert_fs::TempDir,
    python_versions: &[&str],
) -> anyhow::Result<OsString> {
    if let Some(bootstrapped_pythons) = bootstrapped_pythons() {
        let selected_pythons = python_versions.iter().flat_map(|python_version| {
            bootstrapped_pythons.iter().filter(move |path| {
                // Good enough since we control the directory
                path.to_str()
                    .unwrap()
                    .contains(&format!("@{python_version}"))
            })
        });
        return Ok(env::join_paths(selected_pythons)?);
    }

    let bin = temp_dir.child("bin");
    fs_err::create_dir(&bin)?;
    for &request in python_versions {
        let interpreter = find_requested_python(request, &Cache::temp().unwrap())?
            .ok_or(uv_interpreter::Error::NoSuchPython(request.to_string()))?;
        let name = interpreter
            .sys_executable()
            .file_name()
            .expect("Discovered executable must have a filename");
        symlink_file(interpreter.sys_executable(), bin.child(name))?;
    }
    Ok(bin.canonicalize()?.into())
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
        .unwrap_or_else(|err| panic!("Failed to spawn {program}: {err}"));

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

    // This is a heuristic filter meant to try and make *most* of our tests
    // pass whether it's on Windows or Unix. In particular, there are some very
    // common Windows-only dependencies that, when removed from a resolution,
    // cause the set of dependencies to be the same across platforms.
    if cfg!(windows) && windows_filters {
        // The optional leading +/- is for install logs, the optional next line is for lock files
        let windows_only_deps = [
            ("( [+-] )?colorama==\\d+(\\.[\\d+])+\n(    # via .*\n)?"),
            ("( [+-] )?colorama==\\d+(\\.[\\d+])+\\s+(# via .*\n)?"),
            ("( [+-] )?tzdata==\\d+(\\.[\\d+])+\n(    # via .*\n)?"),
            ("( [+-] )?tzdata==\\d+(\\.[\\d+])+\\s+(# via .*\n)?"),
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

/// Recursively copy a directory and its contents.
pub fn copy_dir_all(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> std::io::Result<()> {
    fs_err::create_dir_all(&dst)?;
    for entry in fs_err::read_dir(src.as_ref())? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_all(entry.path(), dst.as_ref().join(entry.file_name()))?;
        } else {
            fs_err::copy(entry.path(), dst.as_ref().join(entry.file_name()))?;
        }
    }
    Ok(())
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
