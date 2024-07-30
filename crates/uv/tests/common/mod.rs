// The `unreachable_pub` is to silence false positives in RustRover.
#![allow(dead_code, unreachable_pub)]

use std::borrow::BorrowMut;
use std::env;
use std::ffi::OsString;
use std::iter::Iterator;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::str::FromStr;

use assert_cmd::assert::{Assert, OutputAssertExt};
use assert_fs::assert::PathAssert;
use assert_fs::fixture::{ChildPath, PathChild, PathCreateDir, SymlinkToFile};
use indoc::formatdoc;
use predicates::prelude::predicate;
use regex::Regex;

use uv_cache::Cache;
use uv_fs::Simplified;
use uv_python::managed::ManagedPythonInstallations;
use uv_python::{
    EnvironmentPreference, PythonInstallation, PythonPreference, PythonRequest, PythonVersion,
};

// Exclude any packages uploaded after this date.
static EXCLUDE_NEWER: &str = "2024-03-25T00:00:00Z";

pub const PACKSE_VERSION: &str = "0.3.31";

/// Using a find links url allows using `--index-url` instead of `--extra-index-url` in tests
/// to prevent dependency confusion attacks against our test suite.
pub fn build_vendor_links_url() -> String {
    format!("https://raw.githubusercontent.com/astral-sh/packse/{PACKSE_VERSION}/vendor/links.html")
}

pub fn packse_index_url() -> String {
    format!("https://astral-sh.github.io/packse/{PACKSE_VERSION}/simple-html/")
}

#[doc(hidden)] // Macro and test context only, don't use directly.
pub const INSTA_FILTERS: &[(&str, &str)] = &[
    (r"--cache-dir [^\s]+", "--cache-dir [CACHE_DIR]"),
    // Operation times
    (r"(\s|\()(\d+m )?(\d+\.)?\d+(ms|s)", "$1[TIME]"),
    // File sizes
    (r"(\s|\()(\d+\.)?\d+([KM]i)?B", "$1[SIZE]"),
    // Timestamps
    (r"tv_sec: \d+", "tv_sec: [TIME]"),
    (r"tv_nsec: \d+", "tv_nsec: [TIME]"),
    // Rewrite Windows output to Unix output
    (r"\\([\w\d])", "/$1"),
    (r"uv.exe", "uv"),
    // uv version display
    (
        r"uv(-.*)? \d+\.\d+\.\d+( \(.*\))?",
        r"uv [VERSION] ([COMMIT] DATE)",
    ),
    // The exact message is host language dependent
    (
        r"Caused by: .* \(os error 2\)",
        "Caused by: No such file or directory (os error 2)",
    ),
];

/// Create a context for tests which simplifies shared behavior across tests.
///
/// * Set the current directory to a temporary directory (`temp_dir`).
/// * Set the cache dir to a different temporary directory (`cache_dir`).
/// * Set a cutoff for versions used in the resolution so the snapshots don't change after a new release.
/// * Set the venv to a fresh `.venv` in `temp_dir`
pub struct TestContext {
    pub temp_dir: assert_fs::TempDir,
    pub cache_dir: assert_fs::TempDir,
    pub python_dir: assert_fs::TempDir,
    pub home_dir: assert_fs::TempDir,
    pub venv: ChildPath,
    pub workspace_root: PathBuf,

    /// The Python version used for the virtual environment, if any.
    pub python_version: Option<PythonVersion>,

    /// All the Python versions available during this test context.
    pub python_versions: Vec<(PythonVersion, PathBuf)>,

    /// Standard filters for this test context.
    filters: Vec<(String, String)>,
}

impl TestContext {
    /// Create a new test context with a virtual environment.
    ///
    /// See [`TestContext::new_with_versions`] if multiple versions are needed or
    /// if creation of the virtual environment should be deferred.
    pub fn new(python_version: &str) -> Self {
        let new = Self::new_with_versions(&[python_version]);
        new.create_venv();
        new
    }

    /// Add extra standard filtering for messages like "Resolved 10 packages" which
    /// can differ between platforms.
    ///
    /// In some cases, these counts are helpful for the snapshot and should not be filtered.
    #[must_use]
    pub fn with_filtered_counts(mut self) -> Self {
        for verb in &[
            "Resolved",
            "Prepared",
            "Installed",
            "Uninstalled",
            "Audited",
        ] {
            self.filters.push((
                format!("{verb} \\d+ packages?"),
                format!("{verb} [N] packages"),
            ));
        }
        self.filters.push((
            "Removed \\d+ files?".to_string(),
            "Removed [N] files".to_string(),
        ));
        self
    }

    /// Add extra standard filtering for executable suffixes on the current platform e.g.
    /// drops `.exe` on Windows.
    #[must_use]
    pub fn with_filtered_exe_suffix(mut self) -> Self {
        self.filters
            .push((regex::escape(env::consts::EXE_SUFFIX), String::new()));
        self
    }

    /// Add extra standard filtering for Python executable names.
    #[must_use]
    pub fn with_filtered_python_names(mut self) -> Self {
        if cfg!(windows) {
            self.filters
                .push(("python.exe".to_string(), "python".to_string()));
        } else {
            self.filters
                .push((r"python\d".to_string(), "python".to_string()));
            self.filters
                .push((r"python\d.\d\d".to_string(), "python".to_string()));
        }
        self
    }

    /// Add extra standard filtering for venv executable directories on the current platform e.g.
    /// `Scripts` on Windows and `bin` on Unix.
    #[must_use]
    pub fn with_filtered_virtualenv_bin(mut self) -> Self {
        self.filters.push((
            format!(r"[\\/]{}", venv_bin_path(PathBuf::new()).to_string_lossy()),
            "/[BIN]".to_string(),
        ));
        self
    }

    /// Create a new test context with multiple Python versions.
    ///
    /// Does not create a virtual environment by default, but the first Python version
    /// can be used to create a virtual environment with [`TestContext::create_venv`].
    ///
    /// See [`TestContext::new`] if only a single version is desired.
    pub fn new_with_versions(python_versions: &[&str]) -> Self {
        let temp_dir = assert_fs::TempDir::new().expect("Failed to create test working directory");
        let cache_dir = assert_fs::TempDir::new().expect("Failed to create test cache directory");
        let python_dir = assert_fs::TempDir::new().expect("Failed to create test Python directory");
        let home_dir = assert_fs::TempDir::new().expect("Failed to create test home directory");

        // Canonicalize the temp dir for consistent snapshot behavior
        let canonical_temp_dir = temp_dir.canonicalize().unwrap();
        let venv = ChildPath::new(canonical_temp_dir.join(".venv"));

        let python_version = python_versions
            .first()
            .map(|version| PythonVersion::from_str(version).unwrap());

        let site_packages = python_version
            .as_ref()
            .map(|version| site_packages_path(&venv, &format!("python{version}")));

        // The workspace root directory is not available without walking up the tree
        // https://github.com/rust-lang/cargo/issues/3946
        let workspace_root = Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
            .parent()
            .expect("CARGO_MANIFEST_DIR should be nested in workspace")
            .parent()
            .expect("CARGO_MANIFEST_DIR should be doubly nested in workspace")
            .to_path_buf();

        let python_versions: Vec<_> = python_versions
            .iter()
            .map(|version| PythonVersion::from_str(version).unwrap())
            .zip(
                python_installations_for_versions(&temp_dir, python_versions)
                    .expect("Failed to find test Python versions"),
            )
            .collect();

        // Construct directories for each Python executable on Unix where the executable names
        // need to be normalized
        if cfg!(unix) {
            for (version, executable) in &python_versions {
                let parent = python_dir.child(version.to_string());
                parent.create_dir_all().unwrap();
                parent.child("python3").symlink_to_file(executable).unwrap();
            }
        }

        let mut filters = Vec::new();

        filters.extend(
            Self::path_patterns(&cache_dir)
                .into_iter()
                .map(|pattern| (pattern, "[CACHE_DIR]/".to_string())),
        );
        if let Some(ref site_packages) = site_packages {
            filters.extend(
                Self::path_patterns(site_packages)
                    .into_iter()
                    .map(|pattern| (pattern, "[SITE_PACKAGES]/".to_string())),
            );
        }
        filters.extend(
            Self::path_patterns(&venv)
                .into_iter()
                .map(|pattern| (pattern, "[VENV]/".to_string())),
        );
        for (version, executable) in &python_versions {
            // Add filtering for the interpreter path
            filters.extend(
                Self::path_patterns(executable)
                    .into_iter()
                    .map(|pattern| (pattern.to_string(), format!("[PYTHON-{version}]"))),
            );

            // And for the symlink we created in the test the Python path
            filters.extend(
                Self::path_patterns(python_dir.join(version.to_string()))
                    .into_iter()
                    .map(|pattern| {
                        (
                            format!("{pattern}[a-zA-Z0-9]*"),
                            format!("[PYTHON-{version}]"),
                        )
                    }),
            );

            // Add Python patch version filtering unless explicitly requested to ensure
            // snapshots are patch version agnostic when it is not a part of the test.
            if version.patch().is_none() {
                filters.push((
                    format!(r"({})\.\d+", regex::escape(version.to_string().as_str())),
                    "$1.[X]".to_string(),
                ));
            }
        }
        filters.extend(
            Self::path_patterns(&temp_dir)
                .into_iter()
                .map(|pattern| (pattern, "[TEMP_DIR]/".to_string())),
        );
        filters.extend(
            Self::path_patterns(&python_dir)
                .into_iter()
                .map(|pattern| (pattern, "[PYTHON_DIR]/".to_string())),
        );
        filters.extend(
            Self::path_patterns(&home_dir)
                .into_iter()
                .map(|pattern| (pattern, "[HOME]/".to_string())),
        );
        filters.extend(
            Self::path_patterns(&workspace_root)
                .into_iter()
                .map(|pattern| (pattern, "[WORKSPACE]/".to_string())),
        );

        // Make virtual environment activation cross-platform
        filters.push((
            r"Activate with: (?:.*)\\Scripts\\activate".to_string(),
            "Activate with: source .venv/bin/activate".to_string(),
        ));

        // Account for [`Simplified::user_display`] which is relative to the command working directory
        if let Some(site_packages) = site_packages {
            filters.push((
                Self::path_pattern(
                    site_packages
                        .strip_prefix(&canonical_temp_dir)
                        .expect("The test site-packages directory is always in the tempdir"),
                ),
                "[SITE_PACKAGES]/".to_string(),
            ));
        };

        // Filter non-deterministic temporary directory names
        // Note we apply this _after_ all the full paths to avoid breaking their matching
        filters.push((r"(\\|\/)\.tmp.*(\\|\/)".to_string(), "/[TMP]/".to_string()));

        // Account for platform prefix differences `file://` (Unix) vs `file:///` (Windows)
        filters.push((r"file:///".to_string(), "file://".to_string()));

        // Destroy any remaining UNC prefixes (Windows only)
        filters.push((r"\\\\\?\\".to_string(), String::new()));

        // Remove the version from the packse url in lockfile snapshots. This avoid having a huge
        // diff any time we upgrade packse
        filters.push((
            format!("https://astral-sh.github.io/packse/{PACKSE_VERSION}/"),
            "https://astral-sh.github.io/packse/PACKSE_VERSION/".to_string(),
        ));

        Self {
            temp_dir,
            cache_dir,
            python_dir,
            home_dir,
            venv,
            workspace_root,
            python_version,
            python_versions,
            filters,
        }
    }

    /// Create a uv command for testing.
    pub fn command(&self) -> Command {
        let mut command = Command::new(get_bin());
        self.add_shared_args(&mut command);
        command
    }

    /// Shared behaviour for almost all test commands.
    ///
    /// * Use a temporary cache directory
    /// * Use a temporary virtual environment with the Python version of [`Self`]
    /// * Don't wrap text output based on the terminal we're in, the test output doesn't get printed
    ///   but snapshotted to a string.
    /// * Use a fake `HOME` to avoid accidentally changing the developer's machine.
    /// * Hide other Python python with `UV_PYTHON_INSTALL_DIR` and installed interpreters with
    ///   `UV_TEST_PYTHON_PATH`.
    /// * Increase the stack size to avoid stack overflows on windows due to large async functions.
    pub fn add_shared_args(&self, command: &mut Command) {
        command
            .arg("--cache-dir")
            .arg(self.cache_dir.path())
            .env("VIRTUAL_ENV", self.venv.as_os_str())
            .env("UV_NO_WRAP", "1")
            .env("HOME", self.home_dir.as_os_str())
            .env("UV_PYTHON_INSTALL_DIR", "")
            .env("UV_TEST_PYTHON_PATH", self.python_path())
            .env("UV_EXCLUDE_NEWER", EXCLUDE_NEWER)
            .current_dir(self.temp_dir.path());

        if cfg!(all(windows, debug_assertions)) {
            // TODO(konstin): Reduce stack usage in debug mode enough that the tests pass with the
            // default windows stack of 1MB
            command.env("UV_STACK_SIZE", (2 * 1024 * 1024).to_string());
        }
    }

    /// Create a `pip compile` command for testing.
    pub fn pip_compile(&self) -> Command {
        let mut command = Command::new(get_bin());
        command.arg("pip").arg("compile");
        self.add_shared_args(&mut command);
        command
    }

    /// Create a `pip compile` command for testing.
    pub fn pip_sync(&self) -> Command {
        let mut command = Command::new(get_bin());
        command.arg("pip").arg("sync");
        self.add_shared_args(&mut command);
        command
    }

    /// Create a `uv venv` command
    pub fn venv(&self) -> Command {
        let mut command = Command::new(get_bin());
        command.arg("venv");
        self.add_shared_args(&mut command);
        command.env_remove("VIRTUAL_ENV");
        command
    }

    /// Create a `pip install` command with options shared across scenarios.
    pub fn pip_install(&self) -> Command {
        let mut command = Command::new(get_bin());
        command.arg("pip").arg("install");
        self.add_shared_args(&mut command);
        command
    }

    /// Create a `pip uninstall` command with options shared across scenarios.
    pub fn pip_uninstall(&self) -> Command {
        let mut command = Command::new(get_bin());
        command.arg("pip").arg("uninstall");
        self.add_shared_args(&mut command);
        command
    }

    /// Create a `pip tree` command for testing.
    pub fn pip_tree(&self) -> Command {
        let mut command = Command::new(get_bin());
        command.arg("pip").arg("tree");
        self.add_shared_args(&mut command);
        command
    }

    /// Create a `uv help` command with options shared across scenarios.
    #[allow(clippy::unused_self)]
    pub fn help(&self) -> Command {
        let mut command = Command::new(get_bin());
        command.arg("help");
        command
    }

    /// Create a `uv init` command with options shared across scenarios.
    pub fn init(&self) -> Command {
        let mut command = Command::new(get_bin());
        command.arg("init");
        self.add_shared_args(&mut command);
        command
    }

    /// Create a `uv sync` command with options shared across scenarios.
    pub fn sync(&self) -> Command {
        let mut command = Command::new(get_bin());
        command.arg("sync");
        self.add_shared_args(&mut command);
        command
    }

    /// Create a `uv lock` command with options shared across scenarios.
    pub fn lock(&self) -> Command {
        let mut command = Command::new(get_bin());
        command.arg("lock");
        self.add_shared_args(&mut command);
        command
    }

    /// Create a `uv python find` command with options shared across scenarios.
    pub fn python_find(&self) -> Command {
        let mut command = Command::new(get_bin());
        command
            .arg("python")
            .arg("find")
            .env("UV_PREVIEW", "1")
            .env("UV_PYTHON_INSTALL_DIR", "")
            .current_dir(&self.temp_dir);
        self.add_shared_args(&mut command);
        command
    }

    /// Create a `uv python pin` command with options shared across scenarios.
    pub fn python_pin(&self) -> Command {
        let mut command = Command::new(get_bin());
        command
            .arg("python")
            .arg("pin")
            .env("UV_PREVIEW", "1")
            .env("UV_PYTHON_INSTALL_DIR", "")
            .current_dir(&self.temp_dir);
        self.add_shared_args(&mut command);
        command
    }

    /// Create a `uv python dir` command with options shared across scenarios.
    pub fn python_dir(&self) -> Command {
        let mut command = Command::new(get_bin());
        command.arg("python").arg("dir");
        self.add_shared_args(&mut command);
        command
    }

    /// Create a `uv run` command with options shared across scenarios.
    pub fn run(&self) -> Command {
        let mut command = Command::new(get_bin());
        command.arg("run").env("UV_SHOW_RESOLUTION", "1");
        self.add_shared_args(&mut command);
        command
    }

    /// Create a `uv tool run` command with options shared across scenarios.
    pub fn tool_run(&self) -> Command {
        let mut command = Command::new(get_bin());
        command
            .arg("tool")
            .arg("run")
            .env("UV_SHOW_RESOLUTION", "1");
        self.add_shared_args(&mut command);
        command
    }

    /// Create a `uv tool install` command with options shared across scenarios.
    pub fn tool_install(&self) -> Command {
        let mut command = self.tool_install_without_exclude_newer();
        command.arg("--exclude-newer").arg(EXCLUDE_NEWER);
        command
    }

    /// Create a `uv tool install` command with no `--exclude-newer` option.
    ///
    /// One should avoid using this in tests to the extent possible because
    /// it can result in tests failing when the index state changes. Therefore,
    /// if you use this, there should be some other kind of mitigation in place.
    /// For example, pinning package versions.
    pub fn tool_install_without_exclude_newer(&self) -> Command {
        let mut command = Command::new(get_bin());
        command.arg("tool").arg("install");
        self.add_shared_args(&mut command);
        command
    }

    /// Create a `uv tool list` command with options shared across scenarios.
    pub fn tool_list(&self) -> Command {
        let mut command = Command::new(get_bin());
        command.arg("tool").arg("list");
        self.add_shared_args(&mut command);
        command
    }

    /// Create a `uv tool dir` command with options shared across scenarios.
    pub fn tool_dir(&self) -> Command {
        let mut command = Command::new(get_bin());
        command.arg("tool").arg("dir");
        self.add_shared_args(&mut command);
        command
    }

    /// Create a `uv tool uninstall` command with options shared across scenarios.
    pub fn tool_uninstall(&self) -> Command {
        let mut command = Command::new(get_bin());
        command.arg("tool").arg("uninstall");
        self.add_shared_args(&mut command);
        command
    }

    /// Create a `uv add` command for the given requirements.
    pub fn add(&self, reqs: &[&str]) -> Command {
        let mut command = Command::new(get_bin());
        command.arg("add").args(reqs);
        self.add_shared_args(&mut command);
        command
    }

    /// Create a `uv remove` command for the given requirements.
    pub fn remove(&self, reqs: &[&str]) -> Command {
        let mut command = Command::new(get_bin());
        command.arg("remove").args(reqs);
        self.add_shared_args(&mut command);
        command
    }

    /// Create a `uv tree` command with options shared across scenarios.
    pub fn tree(&self) -> Command {
        let mut command = Command::new(get_bin());
        command.arg("tree");
        self.add_shared_args(&mut command);
        command
    }

    /// Create a `uv cache clean` command.
    pub fn clean(&self) -> Command {
        let mut command = Command::new(get_bin());
        command.arg("cache").arg("clean");
        self.add_shared_args(&mut command);
        command
    }

    /// Create a `uv cache prune` command.
    pub fn prune(&self) -> Command {
        let mut command = Command::new(get_bin());
        command.arg("cache").arg("prune");
        self.add_shared_args(&mut command);
        command
    }

    pub fn interpreter(&self) -> PathBuf {
        venv_to_interpreter(&self.venv)
    }

    /// Run the given python code and check whether it succeeds.
    pub fn assert_command(&self, command: &str) -> Assert {
        Command::new(venv_to_interpreter(&self.venv))
            // Our tests change files in <1s, so we must disable CPython bytecode caching or we'll get stale files
            // https://github.com/python/cpython/issues/75953
            .arg("-B")
            .arg("-c")
            .arg(command)
            .current_dir(&self.temp_dir)
            .assert()
    }

    /// Run the given python file and check whether it succeeds.
    pub fn assert_file(&self, file: impl AsRef<Path>) -> Assert {
        Command::new(venv_to_interpreter(&self.venv))
            // Our tests change files in <1s, so we must disable CPython bytecode caching or we'll get stale files
            // https://github.com/python/cpython/issues/75953
            .arg("-B")
            .arg(file.as_ref())
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
    pub(crate) fn path_patterns(path: impl AsRef<Path>) -> Vec<String> {
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

    pub fn python_path(&self) -> OsString {
        if cfg!(unix) {
            // On Unix, we needed to normalize the Python executable names to `python3` for the tests
            std::env::join_paths(
                self.python_versions
                    .iter()
                    .map(|(version, _)| self.python_dir.join(version.to_string())),
            )
            .unwrap()
        } else {
            // On Windows, just join the parent directories of the executables
            std::env::join_paths(
                self.python_versions
                    .iter()
                    .map(|(_, executable)| executable.parent().unwrap().to_path_buf()),
            )
            .unwrap()
        }
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
            &format!(
                "{}{}",
                self.python_kind(),
                self.python_version.as_ref().expect(
                    "A Python version must be provided to retrieve the test site packages path"
                )
            ),
        )
    }

    /// Reset the virtual environment in the test context.
    pub fn reset_venv(&self) {
        self.create_venv();
    }

    /// Create a new virtual environment named `.venv` in the test context.
    fn create_venv(&self) {
        let executable = get_python(
            self.python_version
                .as_ref()
                .expect("A Python version must be provided to create a test virtual environment"),
        );
        create_venv_from_executable(&self.venv, &self.cache_dir, &executable);
    }
}

pub fn site_packages_path(venv: &Path, python: &str) -> PathBuf {
    if cfg!(unix) {
        venv.join("lib").join(python).join("site-packages")
    } else if cfg!(windows) {
        venv.join("Lib").join("site-packages")
    } else {
        unimplemented!("Only Windows and Unix are supported")
    }
}

pub fn venv_bin_path(venv: impl AsRef<Path>) -> PathBuf {
    if cfg!(unix) {
        venv.as_ref().join("bin")
    } else if cfg!(windows) {
        venv.as_ref().join("Scripts")
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

/// Get the path to the python interpreter for a specific python version.
pub fn get_python(version: &PythonVersion) -> PathBuf {
    ManagedPythonInstallations::from_settings()
        .map(|installed_pythons| {
            installed_pythons
                .find_version(version)
                .expect("Tests are run on a supported platform")
                .next()
                .as_ref()
                .map(uv_python::managed::ManagedPythonInstallation::executable)
        })
        // We'll search for the request Python on the PATH if not found in the python versions
        // We hack this into a `PathBuf` to satisfy the compiler but it's just a string
        .unwrap_or_default()
        .unwrap_or(PathBuf::from(version.to_string()))
}

/// Create a virtual environment at the given path.
pub fn create_venv_from_executable<P: AsRef<std::path::Path>>(
    path: P,
    cache_dir: &assert_fs::TempDir,
    python: &Path,
) {
    assert_cmd::Command::new(get_bin())
        .arg("venv")
        .arg(path.as_ref().as_os_str())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .arg("--python")
        .arg(python)
        .current_dir(path.as_ref().parent().unwrap())
        .assert()
        .success();
    ChildPath::new(path.as_ref()).assert(predicate::path::is_dir());
}

/// Returns the uv binary that cargo built before launching the tests.
///
/// <https://doc.rust-lang.org/cargo/reference/environment-variables.html#environment-variables-cargo-sets-for-crates>
pub fn get_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_uv"))
}

/// Create a `PATH` with the requested Python versions available in order.
///
/// Generally this should be used with `UV_TEST_PYTHON_PATH`.
pub fn python_path_with_versions(
    temp_dir: &assert_fs::TempDir,
    python_versions: &[&str],
) -> anyhow::Result<OsString> {
    Ok(std::env::join_paths(
        python_installations_for_versions(temp_dir, python_versions)?
            .into_iter()
            .map(|path| path.parent().unwrap().to_path_buf()),
    )?)
}

/// Returns a list of Python executables for the given versions.
///
/// Generally this should be used with `UV_TEST_PYTHON_PATH`.
pub fn python_installations_for_versions(
    temp_dir: &assert_fs::TempDir,
    python_versions: &[&str],
) -> anyhow::Result<Vec<PathBuf>> {
    let cache = Cache::from_path(temp_dir.child("cache").to_path_buf()).init()?;
    let selected_pythons = python_versions
        .iter()
        .map(|python_version| {
            if let Ok(python) = PythonInstallation::find(
                &PythonRequest::parse(python_version),
                EnvironmentPreference::OnlySystem,
                PythonPreference::Managed,
                &cache,
            ) {
                python.into_interpreter().sys_executable().to_owned()
            } else {
                panic!("Could not find Python {python_version} for test");
            }
        })
        .collect::<Vec<_>>();

    assert!(
        python_versions.is_empty() || !selected_pythons.is_empty(),
        "Failed to fulfill requested test Python versions: {selected_pythons:?}"
    );

    Ok(selected_pythons)
}

#[derive(Debug, Copy, Clone)]
pub enum WindowsFilters {
    Platform,
    Universal,
}

/// Execute the command and format its output status, stdout and stderr into a snapshot string.
///
/// This function is derived from `insta_cmd`s `spawn_with_info`.
pub fn run_and_format<T: AsRef<str>>(
    mut command: impl BorrowMut<Command>,
    filters: impl AsRef<[(T, T)]>,
    function_name: &str,
    windows_filters: Option<WindowsFilters>,
) -> (String, Output) {
    let program = command
        .borrow_mut()
        .get_program()
        .to_string_lossy()
        .to_string();

    // Support profiling test run commands with traces.
    if let Ok(root) = env::var("TRACING_DURATIONS_TEST_ROOT") {
        assert!(
            cfg!(feature = "tracing-durations-export"),
            "You need to enable the tracing-durations-export feature to use `TRACING_DURATIONS_TEST_ROOT`"
        );
        command.borrow_mut().env(
            "TRACING_DURATIONS_FILE",
            Path::new(&root).join(function_name).with_extension("jsonl"),
        );
    }

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
        let re = Regex::new(matcher.as_ref()).expect("Do you need to regex::escape your filter?");
        if re.is_match(&snapshot) {
            snapshot = re.replace_all(&snapshot, replacement.as_ref()).to_string();
        }
    }

    // This is a heuristic filter meant to try and make *most* of our tests
    // pass whether it's on Windows or Unix. In particular, there are some very
    // common Windows-only dependencies that, when removed from a resolution,
    // cause the set of dependencies to be the same across platforms.
    if cfg!(windows) {
        if let Some(windows_filters) = windows_filters {
            // The optional leading +/- is for install logs, the optional next line is for lockfiles
            let windows_only_deps = [
                ("( [+-] )?colorama==\\d+(\\.[\\d+])+( \\\\\n    --hash=.*)?\n(    # via .*\n)?"),
                ("( [+-] )?colorama==\\d+(\\.[\\d+])+(\\s+# via .*)?\n"),
                ("( [+-] )?tzdata==\\d+(\\.[\\d+])+( \\\\\n    --hash=.*)?\n(    # via .*\n)?"),
                ("( [+-] )?tzdata==\\d+(\\.[\\d+])+(\\s+# via .*)?\n"),
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
                    for verb in match windows_filters {
                        WindowsFilters::Platform => {
                            ["Resolved", "Prepared", "Installed", "Uninstalled"].iter()
                        }
                        WindowsFilters::Universal => {
                            ["Prepared", "Installed", "Uninstalled"].iter()
                        }
                    } {
                        snapshot = snapshot.replace(
                            &format!("{verb} {} packages", i + removed_packages),
                            &format!("{verb} {} package{}", i, if i > 1 { "s" } else { "" }),
                        );
                    }
                }
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

/// Recursively copy a directory and its contents, skipping gitignored files.
pub fn copy_dir_ignore(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> anyhow::Result<()> {
    for entry in ignore::Walk::new(&src) {
        let entry = entry?;
        let relative = entry.path().strip_prefix(&src)?;
        let ty = entry.file_type().unwrap();
        if ty.is_dir() {
            fs_err::create_dir(dst.as_ref().join(relative))?;
        } else {
            fs_err::copy(entry.path(), dst.as_ref().join(relative))?;
        }
    }
    Ok(())
}

/// Create a stub package `name` in `dir` with the given `pyproject.toml` body.
pub fn make_project(dir: &Path, name: &str, body: &str) -> anyhow::Result<()> {
    let pyproject_toml = formatdoc! {r#"
        [project]
        name = "{name}"
        version = "0.1.0"
        description = "Test package for direct URLs in branches"
        requires-python = ">=3.11,<3.13"
        {body}

        [build-system]
        requires = ["flit_core>=3.8,<4"]
        build-backend = "flit_core.buildapi"
        "#
    };
    fs_err::create_dir_all(dir)?;
    fs_err::write(dir.join("pyproject.toml"), pyproject_toml)?;
    fs_err::create_dir(dir.join(name))?;
    fs_err::write(dir.join(name).join("__init__.py"), "")?;
    Ok(())
}

/// Utility macro to return the name of the current function.
///
/// https://stackoverflow.com/a/40234666/3549270
#[doc(hidden)]
#[macro_export]
macro_rules! function_name {
    () => {{
        fn f() {}
        fn type_name_of_val<T>(_: T) -> &'static str {
            std::any::type_name::<T>()
        }
        let mut name = type_name_of_val(f).strip_suffix("::f").unwrap_or("");
        while let Some(rest) = name.strip_suffix("::{{closure}}") {
            name = rest;
        }
        name
    }};
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
        let (snapshot, output) = $crate::common::run_and_format($spawnable, &$filters, function_name!(), Some($crate::common::WindowsFilters::Platform));
        ::insta::assert_snapshot!(snapshot, @$snapshot);
        output
    }};
    ($filters:expr, windows_filters=false, $spawnable:expr, @$snapshot:literal) => {{
        // Take a reference for backwards compatibility with the vec-expecting insta filters.
        let (snapshot, output) = $crate::common::run_and_format($spawnable, &$filters, function_name!(), None);
        ::insta::assert_snapshot!(snapshot, @$snapshot);
        output
    }};
    ($filters:expr, universal_windows_filters=true, $spawnable:expr, @$snapshot:literal) => {{
        // Take a reference for backwards compatibility with the vec-expecting insta filters.
        let (snapshot, output) = $crate::common::run_and_format($spawnable, &$filters, function_name!(), Some($crate::common::WindowsFilters::Universal));
        ::insta::assert_snapshot!(snapshot, @$snapshot);
        output
    }};
}

/// <https://stackoverflow.com/a/31749071/3549270>
#[allow(unused_imports)]
pub(crate) use uv_snapshot;
