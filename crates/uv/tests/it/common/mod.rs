// The `unreachable_pub` is to silence false positives in RustRover.
#![allow(dead_code, unreachable_pub)]

use std::borrow::BorrowMut;
use std::ffi::OsString;
use std::iter::Iterator;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Output};
use std::str::FromStr;
use std::{env, io};

use assert_cmd::assert::{Assert, OutputAssertExt};
use assert_fs::assert::PathAssert;
use assert_fs::fixture::{ChildPath, PathChild, PathCopy, PathCreateDir, SymlinkToFile};
use base64::{prelude::BASE64_STANDARD as base64, Engine};
use etcetera::BaseStrategy;
use futures::StreamExt;
use indoc::formatdoc;
use itertools::Itertools;
use predicates::prelude::predicate;
use regex::Regex;

use tokio::io::AsyncWriteExt;
use uv_cache::Cache;
use uv_fs::Simplified;
use uv_python::managed::ManagedPythonInstallations;
use uv_python::{
    EnvironmentPreference, PythonInstallation, PythonPreference, PythonRequest, PythonVersion,
};
use uv_static::EnvVars;

// Exclude any packages uploaded after this date.
static EXCLUDE_NEWER: &str = "2024-03-25T00:00:00Z";

pub const PACKSE_VERSION: &str = "0.3.46";

/// Using a find links url allows using `--index-url` instead of `--extra-index-url` in tests
/// to prevent dependency confusion attacks against our test suite.
pub fn build_vendor_links_url() -> String {
    env::var(EnvVars::UV_TEST_VENDOR_LINKS_URL)
        .ok()
        .unwrap_or(format!(
            "https://raw.githubusercontent.com/astral-sh/packse/{PACKSE_VERSION}/vendor/links.html"
        ))
}

pub fn packse_index_url() -> String {
    env::var(EnvVars::UV_TEST_INDEX_URL).ok().unwrap_or(format!(
        "https://astral-sh.github.io/packse/{PACKSE_VERSION}/simple-html/"
    ))
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
    (r"\\([\w\d]|\.)", "/$1"),
    (r"uv\.exe", "uv"),
    // uv version display
    (
        r"uv(-.*)? \d+\.\d+\.\d+(\+\d+)?( \(.*\))?",
        r"uv [VERSION] ([COMMIT] DATE)",
    ),
    // The exact message is host language dependent
    (
        r"Caused by: .* \(os error 2\)",
        "Caused by: No such file or directory (os error 2)",
    ),
    // Trim end-of-line whitespaces, to allow removing them on save.
    (r"([^\s])[ \t]+(\r?\n)", "$1$2"),
];

/// Create a context for tests which simplifies shared behavior across tests.
///
/// * Set the current directory to a temporary directory (`temp_dir`).
/// * Set the cache dir to a different temporary directory (`cache_dir`).
/// * Set a cutoff for versions used in the resolution so the snapshots don't change after a new release.
/// * Set the venv to a fresh `.venv` in `temp_dir`
pub struct TestContext {
    pub root: ChildPath,
    pub temp_dir: ChildPath,
    pub cache_dir: ChildPath,
    pub python_dir: ChildPath,
    pub home_dir: ChildPath,
    pub user_config_dir: ChildPath,
    pub bin_dir: ChildPath,
    pub venv: ChildPath,
    pub workspace_root: PathBuf,

    /// The Python version used for the virtual environment, if any.
    pub python_version: Option<PythonVersion>,

    /// All the Python versions available during this test context.
    pub python_versions: Vec<(PythonVersion, PathBuf)>,

    /// Standard filters for this test context.
    filters: Vec<(String, String)>,

    /// Extra environment variables to apply to all commands.
    extra_env: Vec<(OsString, OsString)>,

    #[allow(dead_code)]
    _root: tempfile::TempDir,
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

    /// Set the "exclude newer" timestamp for all commands in this context.
    pub fn with_exclude_newer(mut self, exclude_newer: &str) -> Self {
        self.extra_env
            .push((EnvVars::UV_EXCLUDE_NEWER.into(), exclude_newer.into()));
        self
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

    /// Add extra standard filtering for Windows-compatible missing file errors.
    pub fn with_filtered_missing_file_error(mut self) -> Self {
        self.filters.push((
            regex::escape("The system cannot find the file specified. (os error 2)"),
            "[OS ERROR 2]".to_string(),
        ));
        self.filters.push((
            regex::escape("No such file or directory (os error 2)"),
            "[OS ERROR 2]".to_string(),
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

    /// Add extra standard filtering for Python interpreter sources
    #[must_use]
    pub fn with_filtered_python_sources(mut self) -> Self {
        self.filters.push((
            "virtual environments, managed installations, or search path".to_string(),
            "[PYTHON SOURCES]".to_string(),
        ));
        self.filters.push((
            "virtual environments, managed installations, search path, or registry".to_string(),
            "[PYTHON SOURCES]".to_string(),
        ));
        self.filters.push((
            "managed installations or search path".to_string(),
            "[PYTHON SOURCES]".to_string(),
        ));
        self.filters.push((
            "managed installations, search path, or registry".to_string(),
            "[PYTHON SOURCES]".to_string(),
        ));
        self
    }

    /// Add extra standard filtering for Python executable names, e.g., stripping version number
    /// and `.exe` suffixes.
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

    /// Add extra standard filtering for a given path.
    #[must_use]
    pub fn with_filtered_path(mut self, path: &Path, name: &str) -> Self {
        // Note this is sloppy, ideally we wouldn't push to the front of the `Vec` but we need
        // this to come in front of other filters or we can transform the path (e.g., with `[TMP]`)
        // before we reach this filter.
        for pattern in Self::path_patterns(path)
            .into_iter()
            .map(|pattern| (pattern, format!("[{name}]/")))
        {
            self.filters.insert(0, pattern);
        }
        self
    }

    /// Adds a filter that specifically ignores the link mode warning.
    ///
    /// This occurs in some cases and can be used on an ad hoc basis to squash
    /// the warning in the snapshots. This is useful because the warning does
    /// not consistently appear. It is dependent on the environment. (For
    /// example, sometimes it's dependent on whether `/tmp` and `~/.local` live
    /// on the same file system.)
    #[inline]
    pub fn with_filtered_link_mode_warning(mut self) -> Self {
        let pattern = "warning: Failed to hardlink files; .*\n.*\n.*\n";
        self.filters.push((pattern.to_string(), String::new()));
        self
    }

    /// Adds a filter that ignores platform information in a Python installation key.
    pub fn with_filtered_python_keys(mut self) -> Self {
        // Filter platform keys
        self.filters.push((
            r"((?:cpython|pypy)-\d+\.\d+(?:\.(?:\[X\]|\d+))?[a-z]?(?:\+[a-z]+)?)-[a-z0-9]+-[a-z0-9_]+-[a-z]+"
                .to_string(),
            "$1-[PLATFORM]".to_string(),
        ));
        self
    }

    /// Add a filter that ignores temporary directory in path.
    pub fn with_filtered_windows_temp_dir(mut self) -> Self {
        let pattern = regex::escape(
            &self
                .temp_dir
                .simplified_display()
                .to_string()
                .replace('/', "\\"),
        );
        self.filters.push((pattern, "[TEMP_DIR]".to_string()));
        self
    }

    /// Add extra directories and configuration for managed Python installations.
    #[must_use]
    pub fn with_managed_python_dirs(mut self) -> Self {
        let managed = self.temp_dir.join("managed");

        self.extra_env.push((
            EnvVars::UV_PYTHON_BIN_DIR.into(),
            self.bin_dir.as_os_str().to_owned(),
        ));
        self.extra_env
            .push((EnvVars::UV_PYTHON_INSTALL_DIR.into(), managed.into()));
        self.extra_env
            .push((EnvVars::UV_PYTHON_DOWNLOADS.into(), "automatic".into()));

        self
    }

    /// Clear filters on `TestContext`.
    pub fn clear_filters(mut self) -> Self {
        self.filters.clear();
        self
    }

    /// Discover the path to the XDG state directory. We use this, rather than the OS-specific
    /// temporary directory, because on macOS (and Windows on GitHub Actions), they involve
    /// symlinks. (On macOS, the temporary directory is, like `/var/...`, which resolves to
    /// `/private/var/...`.)
    ///
    /// It turns out that, at least on macOS, if we pass a symlink as `current_dir`, it gets
    /// _immediately_ resolved (such that if you call `current_dir` in the running `Command`, it
    /// returns resolved symlink). This is problematic, as we _don't_ want to resolve symlinks
    /// for user-provided paths.
    pub fn test_bucket_dir() -> PathBuf {
        env::var(EnvVars::UV_INTERNAL__TEST_DIR)
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                etcetera::base_strategy::choose_base_strategy()
                    .expect("Failed to find base strategy")
                    .data_dir()
                    .join("uv")
                    .join("tests")
            })
    }

    /// Create a new test context with multiple Python versions.
    ///
    /// Does not create a virtual environment by default, but the first Python version
    /// can be used to create a virtual environment with [`TestContext::create_venv`].
    ///
    /// See [`TestContext::new`] if only a single version is desired.
    pub fn new_with_versions(python_versions: &[&str]) -> Self {
        let bucket = Self::test_bucket_dir();
        fs_err::create_dir_all(&bucket).expect("Failed to create test bucket");

        let root = tempfile::TempDir::new_in(bucket).expect("Failed to create test root directory");

        // Create a `.git` directory to isolate tests that search for git boundaries from the state
        // of the file system
        fs_err::create_dir_all(root.path().join(".git"))
            .expect("Failed to create `.git` placeholder in test root directory");

        let temp_dir = ChildPath::new(root.path()).child("temp");
        fs_err::create_dir_all(&temp_dir).expect("Failed to create test working directory");

        let cache_dir = ChildPath::new(root.path()).child("cache");
        fs_err::create_dir_all(&cache_dir).expect("Failed to create test cache directory");

        let python_dir = ChildPath::new(root.path()).child("python");
        fs_err::create_dir_all(&python_dir).expect("Failed to create test Python directory");

        let bin_dir = ChildPath::new(root.path()).child("bin");
        fs_err::create_dir_all(&bin_dir).expect("Failed to create test bin directory");

        // When the `git` feature is disabled, enforce that the test suite does not use `git`
        if cfg!(not(feature = "git")) {
            Self::disallow_git_cli(&bin_dir).expect("Failed to setup disallowed `git` command");
        }

        let home_dir = ChildPath::new(root.path()).child("home");
        fs_err::create_dir_all(&home_dir).expect("Failed to create test home directory");

        let user_config_dir = if cfg!(windows) {
            ChildPath::new(home_dir.path())
        } else {
            ChildPath::new(home_dir.path()).child(".config")
        };

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
        let workspace_root = Path::new(&env::var(EnvVars::CARGO_MANIFEST_DIR).unwrap())
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
            Self::path_patterns(get_bin())
                .into_iter()
                .map(|pattern| (pattern, "[UV]".to_string())),
        );

        // Exclude `link-mode` on Windows since we set it in the remote test suite
        if cfg!(windows) {
            filters.push((" --link-mode <LINK_MODE>".to_string(), String::new()));
            filters.push((r#"link-mode = "copy"\n"#.to_string(), String::new()));
        }

        filters.extend(
            Self::path_patterns(&bin_dir)
                .into_iter()
                .map(|pattern| (pattern, "[BIN]/".to_string())),
        );
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
        let mut uv_user_config_dir = PathBuf::from(user_config_dir.path());
        uv_user_config_dir.push("uv");
        filters.extend(
            Self::path_patterns(&uv_user_config_dir)
                .into_iter()
                .map(|pattern| (pattern, "[UV_USER_CONFIG_DIR]/".to_string())),
        );
        filters.extend(
            Self::path_patterns(&user_config_dir)
                .into_iter()
                .map(|pattern| (pattern, "[USER_CONFIG_DIR]/".to_string())),
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

        // Make virtual environment activation cross-platform and shell-agnostic
        filters.push((
            r"Activate with: (.*)\\Scripts\\activate".to_string(),
            "Activate with: source $1/[BIN]/activate".to_string(),
        ));
        filters.push((
            r"Activate with: source (.*)/bin/activate(?:\.\w+)?".to_string(),
            "Activate with: source $1/[BIN]/activate".to_string(),
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

        // Remove the version from the packse url in lockfile snapshots. This avoids having a huge
        // diff any time we upgrade packse
        filters.push((
            format!("https://astral-sh.github.io/packse/{PACKSE_VERSION}/"),
            "https://astral-sh.github.io/packse/PACKSE_VERSION/".to_string(),
        ));
        filters.push((
            format!("https://raw.githubusercontent.com/astral-sh/packse/{PACKSE_VERSION}/"),
            "https://raw.githubusercontent.com/astral-sh/packse/PACKSE_VERSION/".to_string(),
        ));

        Self {
            root: ChildPath::new(root.path()),
            temp_dir,
            cache_dir,
            python_dir,
            home_dir,
            user_config_dir,
            bin_dir,
            venv,
            workspace_root,
            python_version,
            python_versions,
            filters,
            extra_env: vec![],
            _root: root,
        }
    }

    /// Create a uv command for testing.
    pub fn command(&self) -> Command {
        let mut command = self.new_command();
        self.add_shared_options(&mut command, true);
        command
    }

    fn disallow_git_cli(bin_dir: &Path) -> std::io::Result<()> {
        let contents = r"#!/bin/sh
    echo 'error: `git` operations are not allowed — are you missing a cfg for the `git` feature?' >&2
    exit 127";
        let git = bin_dir.join(format!("git{}", env::consts::EXE_SUFFIX));
        fs_err::write(&git, contents)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs_err::metadata(&git)?.permissions();
            perms.set_mode(0o755);
            fs_err::set_permissions(&git, perms)?;
        }

        Ok(())
    }

    /// Shared behaviour for almost all test commands.
    ///
    /// * Use a temporary cache directory
    /// * Use a temporary virtual environment with the Python version of [`Self`]
    /// * Don't wrap text output based on the terminal we're in, the test output doesn't get printed
    ///   but snapshotted to a string.
    /// * Use a fake `HOME` to avoid accidentally changing the developer's machine.
    /// * Hide other Pythons with `UV_PYTHON_INSTALL_DIR` and installed interpreters with
    ///   `UV_TEST_PYTHON_PATH` and an active venv (if applicable) by removing `VIRTUAL_ENV`.
    /// * Increase the stack size to avoid stack overflows on windows due to large async functions.
    pub fn add_shared_options(&self, command: &mut Command, activate_venv: bool) {
        self.add_shared_args(command);
        self.add_shared_env(command, activate_venv);
    }

    /// Only the arguments of [`TestContext::add_shared_options`].
    pub fn add_shared_args(&self, command: &mut Command) {
        command.arg("--cache-dir").arg(self.cache_dir.path());
    }

    /// Only the environment variables of [`TestContext::add_shared_options`].
    pub fn add_shared_env(&self, command: &mut Command, activate_venv: bool) {
        // Push the test context bin to the front of the PATH
        let path = env::join_paths(std::iter::once(self.bin_dir.to_path_buf()).chain(
            env::split_paths(&env::var(EnvVars::PATH).unwrap_or_default()),
        ))
        .unwrap();

        command
            // When running the tests in a venv, ignore that venv, otherwise we'll capture warnings.
            .env_remove(EnvVars::VIRTUAL_ENV)
            // Disable wrapping of uv output for readability / determinism in snapshots.
            .env(EnvVars::UV_NO_WRAP, "1")
            // While we disable wrapping in uv above, invoked tools may still wrap their output so
            // we set a fixed `COLUMNS` value for isolation from terminal width.
            .env(EnvVars::COLUMNS, "100")
            .env(EnvVars::PATH, path)
            .env(EnvVars::HOME, self.home_dir.as_os_str())
            .env(EnvVars::APPDATA, self.home_dir.as_os_str())
            .env(EnvVars::USERPROFILE, self.home_dir.as_os_str())
            .env(EnvVars::UV_PYTHON_INSTALL_DIR, "")
            // Installations are not allowed by default; see `Self::with_managed_python_dirs`
            .env(EnvVars::UV_PYTHON_DOWNLOADS, "never")
            .env(EnvVars::UV_TEST_PYTHON_PATH, self.python_path())
            .env(EnvVars::UV_EXCLUDE_NEWER, EXCLUDE_NEWER)
            // Since downloads, fetches and builds run in parallel, their message output order is
            // non-deterministic, so can't capture them in test output.
            .env(EnvVars::UV_TEST_NO_CLI_PROGRESS, "1")
            .env_remove(EnvVars::UV_CACHE_DIR)
            .env_remove(EnvVars::UV_TOOL_BIN_DIR)
            .env_remove(EnvVars::XDG_CONFIG_HOME)
            .current_dir(self.temp_dir.path());

        for (key, value) in &self.extra_env {
            command.env(key, value);
        }

        if activate_venv {
            command.env(EnvVars::VIRTUAL_ENV, self.venv.as_os_str());
        }

        if cfg!(unix) {
            // Avoid locale issues in tests
            command.env(EnvVars::LC_ALL, "C");
        }
    }

    /// Create a `pip compile` command for testing.
    pub fn pip_compile(&self) -> Command {
        let mut command = self.new_command();
        command.arg("pip").arg("compile");
        self.add_shared_options(&mut command, true);
        command
    }

    /// Create a `pip compile` command for testing.
    pub fn pip_sync(&self) -> Command {
        let mut command = self.new_command();
        command.arg("pip").arg("sync");
        self.add_shared_options(&mut command, true);
        command
    }

    pub fn pip_show(&self) -> Command {
        let mut command = self.new_command();
        command.arg("pip").arg("show");
        self.add_shared_options(&mut command, true);
        command
    }

    /// Create a `pip freeze` command with options shared across scenarios.
    pub fn pip_freeze(&self) -> Command {
        let mut command = self.new_command();
        command.arg("pip").arg("freeze");
        self.add_shared_options(&mut command, true);
        command
    }

    /// Create a `pip check` command with options shared across scenarios.
    pub fn pip_check(&self) -> Command {
        let mut command = self.new_command();
        command.arg("pip").arg("check");
        self.add_shared_options(&mut command, true);
        command
    }

    pub fn pip_list(&self) -> Command {
        let mut command = self.new_command();
        command.arg("pip").arg("list");
        self.add_shared_options(&mut command, true);
        command
    }

    /// Create a `uv venv` command
    pub fn venv(&self) -> Command {
        let mut command = self.new_command();
        command.arg("venv");
        self.add_shared_options(&mut command, false);
        command
    }

    /// Create a `pip install` command with options shared across scenarios.
    pub fn pip_install(&self) -> Command {
        let mut command = self.new_command();
        command.arg("pip").arg("install");
        self.add_shared_options(&mut command, true);
        command
    }

    /// Create a `pip uninstall` command with options shared across scenarios.
    pub fn pip_uninstall(&self) -> Command {
        let mut command = self.new_command();
        command.arg("pip").arg("uninstall");
        self.add_shared_options(&mut command, true);
        command
    }

    /// Create a `pip tree` command for testing.
    pub fn pip_tree(&self) -> Command {
        let mut command = self.new_command();
        command.arg("pip").arg("tree");
        self.add_shared_options(&mut command, true);
        command
    }

    /// Create a `uv help` command with options shared across scenarios.
    #[allow(clippy::unused_self)]
    pub fn help(&self) -> Command {
        let mut command = self.new_command();
        command.arg("help");
        command.env_remove(EnvVars::UV_CACHE_DIR);
        command
    }

    /// Create a `uv init` command with options shared across scenarios and
    /// isolated from any git repository that may exist in a parent directory.
    pub fn init(&self) -> Command {
        let mut command = self.new_command();
        command.arg("init");
        self.add_shared_options(&mut command, false);
        command
    }

    /// Create a `uv sync` command with options shared across scenarios.
    pub fn sync(&self) -> Command {
        let mut command = self.new_command();
        command.arg("sync");
        self.add_shared_options(&mut command, false);
        command
    }

    /// Create a `uv lock` command with options shared across scenarios.
    pub fn lock(&self) -> Command {
        let mut command = self.new_command();
        command.arg("lock");
        self.add_shared_options(&mut command, false);
        command
    }

    /// Create a `uv export` command with options shared across scenarios.
    pub fn export(&self) -> Command {
        let mut command = self.new_command();
        command.arg("export");
        self.add_shared_options(&mut command, false);
        command
    }

    /// Create a `uv build` command with options shared across scenarios.
    pub fn build(&self) -> Command {
        let mut command = self.new_command();
        command.arg("build");
        self.add_shared_options(&mut command, false);
        command
    }

    /// Create a `uv publish` command with options shared across scenarios.
    pub fn publish(&self) -> Command {
        let mut command = self.new_command();
        command.arg("publish");
        command
    }

    /// Create a `uv python find` command with options shared across scenarios.
    pub fn python_find(&self) -> Command {
        let mut command = self.new_command();
        command
            .arg("python")
            .arg("find")
            .env(EnvVars::UV_PREVIEW, "1")
            .env(EnvVars::UV_PYTHON_INSTALL_DIR, "")
            .current_dir(&self.temp_dir);
        self.add_shared_options(&mut command, false);
        command
    }

    /// Create a `uv python install` command with options shared across scenarios.
    pub fn python_install(&self) -> Command {
        let mut command = self.new_command();
        self.add_shared_options(&mut command, true);
        command
            .arg("python")
            .arg("install")
            .current_dir(&self.temp_dir);
        command
    }

    /// Create a `uv python uninstall` command with options shared across scenarios.
    pub fn python_uninstall(&self) -> Command {
        let mut command = self.new_command();
        self.add_shared_options(&mut command, true);
        command
            .arg("python")
            .arg("uninstall")
            .current_dir(&self.temp_dir);
        command
    }

    /// Create a `uv python pin` command with options shared across scenarios.
    pub fn python_pin(&self) -> Command {
        let mut command = self.new_command();
        command.arg("python").arg("pin");
        self.add_shared_options(&mut command, true);
        command
    }

    /// Create a `uv python dir` command with options shared across scenarios.
    pub fn python_dir(&self) -> Command {
        let mut command = self.new_command();
        command.arg("python").arg("dir");
        self.add_shared_options(&mut command, true);
        command
    }

    /// Create a `uv run` command with options shared across scenarios.
    pub fn run(&self) -> Command {
        let mut command = self.new_command();
        command.arg("run").env(EnvVars::UV_SHOW_RESOLUTION, "1");
        self.add_shared_options(&mut command, true);
        command
    }

    /// Create a `uv tool run` command with options shared across scenarios.
    pub fn tool_run(&self) -> Command {
        let mut command = self.new_command();
        command
            .arg("tool")
            .arg("run")
            .env(EnvVars::UV_SHOW_RESOLUTION, "1");
        self.add_shared_options(&mut command, false);
        command
    }

    /// Create a `uv upgrade run` command with options shared across scenarios.
    pub fn tool_upgrade(&self) -> Command {
        let mut command = self.new_command();
        command.arg("tool").arg("upgrade");
        self.add_shared_options(&mut command, false);
        command
    }

    /// Create a `uv tool install` command with options shared across scenarios.
    pub fn tool_install(&self) -> Command {
        let mut command = self.new_command();
        command.arg("tool").arg("install");
        self.add_shared_options(&mut command, false);
        command
    }

    /// Create a `uv tool list` command with options shared across scenarios.
    pub fn tool_list(&self) -> Command {
        let mut command = self.new_command();
        command.arg("tool").arg("list");
        self.add_shared_options(&mut command, false);
        command
    }

    /// Create a `uv tool dir` command with options shared across scenarios.
    pub fn tool_dir(&self) -> Command {
        let mut command = self.new_command();
        command.arg("tool").arg("dir");
        self.add_shared_options(&mut command, false);
        command
    }

    /// Create a `uv tool uninstall` command with options shared across scenarios.
    pub fn tool_uninstall(&self) -> Command {
        let mut command = self.new_command();
        command.arg("tool").arg("uninstall");
        self.add_shared_options(&mut command, false);
        command
    }

    /// Create a `uv add` command for the given requirements.
    pub fn add(&self) -> Command {
        let mut command = self.new_command();
        command.arg("add");
        self.add_shared_options(&mut command, false);
        command
    }

    /// Create a `uv remove` command for the given requirements.
    pub fn remove(&self) -> Command {
        let mut command = self.new_command();
        command.arg("remove");
        self.add_shared_options(&mut command, false);
        command
    }

    /// Create a `uv tree` command with options shared across scenarios.
    pub fn tree(&self) -> Command {
        let mut command = self.new_command();
        command.arg("tree");
        self.add_shared_options(&mut command, false);
        command
    }

    /// Create a `uv cache clean` command.
    pub fn clean(&self) -> Command {
        let mut command = self.new_command();
        command.arg("cache").arg("clean");
        self.add_shared_options(&mut command, false);
        command
    }

    /// Create a `uv cache prune` command.
    pub fn prune(&self) -> Command {
        let mut command = self.new_command();
        command.arg("cache").arg("prune");
        self.add_shared_options(&mut command, false);
        command
    }

    /// Create a `uv build_backend` command.
    ///
    /// Note that this command is hidden and only invoking it through a build frontend is supported.
    pub fn build_backend(&self) -> Command {
        let mut command = self.new_command();
        command.arg("build-backend");
        self.add_shared_options(&mut command, false);
        command
    }

    pub fn interpreter(&self) -> PathBuf {
        venv_to_interpreter(&self.venv)
    }

    /// Run the given python code and check whether it succeeds.
    pub fn assert_command(&self, command: &str) -> Assert {
        self.new_command_with(&venv_to_interpreter(&self.venv))
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
        self.new_command_with(&venv_to_interpreter(&self.venv))
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
    pub fn path_patterns(path: impl AsRef<Path>) -> Vec<String> {
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
            env::join_paths(
                self.python_versions
                    .iter()
                    .map(|(version, _)| self.python_dir.join(version.to_string())),
            )
            .unwrap()
        } else {
            // On Windows, just join the parent directories of the executables
            env::join_paths(
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

    /// Only the filters added to this test context.
    pub fn filters_without_standard_filters(&self) -> Vec<(&str, &str)> {
        self.filters
            .iter()
            .map(|(p, r)| (p.as_str(), r.as_str()))
            .collect()
    }

    /// For when we add pypy to the test suite.
    #[allow(clippy::unused_self)]
    pub fn python_kind(&self) -> &'static str {
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

    /// Copies the files from the ecosystem project given into this text
    /// context.
    ///
    /// This will almost always write at least a `pyproject.toml` into this
    /// test context.
    ///
    /// The given name should correspond to the name of a sub-directory (not a
    /// path to it) in the top-level `ecosystem` directory.
    ///
    /// This panics (fails the current test) for any failure.
    pub fn copy_ecosystem_project(&self, name: &str) {
        let project_dir = PathBuf::from(format!("../../ecosystem/{name}"));
        self.temp_dir.copy_from(project_dir, &["*"]).unwrap();
        // If there is a (gitignore) lockfile, remove it.
        if let Err(err) = fs_err::remove_file(self.temp_dir.join("uv.lock")) {
            assert_eq!(
                err.kind(),
                io::ErrorKind::NotFound,
                "Failed to remove uv.lock: {err}"
            );
        }
    }

    /// Creates a way to compare the changes made to a lock file.
    ///
    /// This routine starts by copying (not moves) the generated lock file to
    /// memory. It then calls the given closure with this test context to get a
    /// `Command` and runs the command. The diff between the old lock file and
    /// the new one is then returned.
    ///
    /// This assumes that a lock has already been performed.
    pub fn diff_lock(&self, change: impl Fn(&TestContext) -> Command) -> String {
        static TRIM_TRAILING_WHITESPACE: std::sync::LazyLock<Regex> =
            std::sync::LazyLock::new(|| Regex::new(r"(?m)^\s+$").unwrap());

        let lock_path = ChildPath::new(self.temp_dir.join("uv.lock"));
        let old_lock = fs_err::read_to_string(&lock_path).unwrap();
        let (snapshot, _, status) = run_and_format_with_status(
            change(self),
            self.filters(),
            "diff_lock",
            Some(WindowsFilters::Platform),
        );
        assert!(status.success(), "{snapshot}");
        let new_lock = fs_err::read_to_string(&lock_path).unwrap();
        diff_snapshot(&old_lock, &new_lock)
    }

    /// Read a file in the temporary directory
    pub fn read(&self, file: impl AsRef<Path>) -> String {
        fs_err::read_to_string(self.temp_dir.join(&file))
            .unwrap_or_else(|_| panic!("Missing file: `{}`", file.user_display()))
    }

    /// Creates a new `Command` that is intended to be suitable for use in
    /// all tests.
    fn new_command(&self) -> Command {
        self.new_command_with(&get_bin())
    }

    /// Creates a new `Command` that is intended to be suitable for use in
    /// all tests, but with the given binary.
    fn new_command_with(&self, bin: &Path) -> Command {
        let mut command = Command::new(bin);
        // I believe the intent of all tests is that they are run outside the
        // context of an existing git repository. And when they aren't, state
        // from the parent git repository can bleed into the behavior of `uv
        // init` in a way that makes it difficult to test consistently. By
        // setting GIT_CEILING_DIRECTORIES, we specifically prevent git from
        // climbing up past the root of our test directory to look for any
        // other git repos.
        //
        // If one wants to write a test specifically targeting uv within a
        // pre-existing git repository, then the test should make the parent
        // git repo explicitly. The GIT_CEILING_DIRECTORIES here shouldn't
        // impact it, since it only prevents git from discovering repositories
        // at or above the root.
        command.env(EnvVars::GIT_CEILING_DIRECTORIES, self.root.path());
        command
    }
}

/// Creates a "unified" diff between the two line-oriented strings suitable
/// for snapshotting.
pub fn diff_snapshot(old: &str, new: &str) -> String {
    static TRIM_TRAILING_WHITESPACE: std::sync::LazyLock<Regex> =
        std::sync::LazyLock::new(|| Regex::new(r"(?m)^\s+$").unwrap());

    let diff = similar::TextDiff::from_lines(old, new);
    let unified = diff
        .unified_diff()
        .context_radius(10)
        .header("old", "new")
        .to_string();
    // Not totally clear why, but some lines end up containing only
    // whitespace in the diff, even though they don't appear in the
    // original data. So just strip them here.
    TRIM_TRAILING_WHITESPACE
        .replace_all(&unified, "")
        .into_owned()
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
    ManagedPythonInstallations::from_settings(None)
        .map(|installed_pythons| {
            installed_pythons
                .find_version(version)
                .expect("Tests are run on a supported platform")
                .next()
                .as_ref()
                .map(|python| python.executable(false))
        })
        // We'll search for the request Python on the PATH if not found in the python versions
        // We hack this into a `PathBuf` to satisfy the compiler but it's just a string
        .unwrap_or_default()
        .unwrap_or(PathBuf::from(version.to_string()))
}

/// Create a virtual environment at the given path.
pub fn create_venv_from_executable<P: AsRef<Path>>(path: P, cache_dir: &ChildPath, python: &Path) {
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
    temp_dir: &ChildPath,
    python_versions: &[&str],
) -> anyhow::Result<OsString> {
    Ok(env::join_paths(
        python_installations_for_versions(temp_dir, python_versions)?
            .into_iter()
            .map(|path| path.parent().unwrap().to_path_buf()),
    )?)
}

/// Returns a list of Python executables for the given versions.
///
/// Generally this should be used with `UV_TEST_PYTHON_PATH`.
pub fn python_installations_for_versions(
    temp_dir: &ChildPath,
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

/// Helper method to apply filters to a string. Useful when `!uv_snapshot` cannot be used.
pub fn apply_filters<T: AsRef<str>>(mut snapshot: String, filters: impl AsRef<[(T, T)]>) -> String {
    for (matcher, replacement) in filters.as_ref() {
        // TODO(konstin): Cache regex compilation
        let re = Regex::new(matcher.as_ref()).expect("Do you need to regex::escape your filter?");
        if re.is_match(&snapshot) {
            snapshot = re.replace_all(&snapshot, replacement.as_ref()).to_string();
        }
    }
    snapshot
}

/// Execute the command and format its output status, stdout and stderr into a snapshot string.
///
/// This function is derived from `insta_cmd`s `spawn_with_info`.
pub fn run_and_format<T: AsRef<str>>(
    command: impl BorrowMut<Command>,
    filters: impl AsRef<[(T, T)]>,
    function_name: &str,
    windows_filters: Option<WindowsFilters>,
) -> (String, Output) {
    let (snapshot, output, _) =
        run_and_format_with_status(command, filters, function_name, windows_filters);
    (snapshot, output)
}

/// Execute the command and format its output status, stdout and stderr into a snapshot string.
///
/// This function is derived from `insta_cmd`s `spawn_with_info`.
#[allow(clippy::print_stderr)]
pub fn run_and_format_with_status<T: AsRef<str>>(
    mut command: impl BorrowMut<Command>,
    filters: impl AsRef<[(T, T)]>,
    function_name: &str,
    windows_filters: Option<WindowsFilters>,
) -> (String, Output, ExitStatus) {
    let program = command
        .borrow_mut()
        .get_program()
        .to_string_lossy()
        .to_string();

    // Support profiling test run commands with traces.
    if let Ok(root) = env::var(EnvVars::TRACING_DURATIONS_TEST_ROOT) {
        assert!(
            cfg!(feature = "tracing-durations-export"),
            "You need to enable the tracing-durations-export feature to use `TRACING_DURATIONS_TEST_ROOT`"
        );
        command.borrow_mut().env(
            EnvVars::TRACING_DURATIONS_FILE,
            Path::new(&root).join(function_name).with_extension("jsonl"),
        );
    }

    let output = command
        .borrow_mut()
        .output()
        .unwrap_or_else(|err| panic!("Failed to spawn {program}: {err}"));

    eprintln!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ Unfiltered output ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    eprintln!(
        "----- stdout -----\n{}\n----- stderr -----\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    eprintln!("────────────────────────────────────────────────────────────────────────────────\n");

    let mut snapshot = apply_filters(
        format!(
            "success: {:?}\nexit_code: {}\n----- stdout -----\n{}\n----- stderr -----\n{}",
            output.status.success(),
            output.status.code().unwrap_or(!0),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        ),
        filters,
    );

    // This is a heuristic filter meant to try and make *most* of our tests
    // pass whether it's on Windows or Unix. In particular, there are some very
    // common Windows-only dependencies that, when removed from a resolution,
    // cause the set of dependencies to be the same across platforms.
    if cfg!(windows) {
        if let Some(windows_filters) = windows_filters {
            // The optional leading +/-/~ is for install logs, the optional next line is for lockfiles
            let windows_only_deps = [
                (r"( ?[-+~] ?)?colorama==\d+(\.\d+)+( [\\]\n\s+--hash=.*)?\n(\s+# via .*\n)?"),
                (r"( ?[-+~] ?)?colorama==\d+(\.\d+)+(\s+[-+~]?\s+# via .*)?\n"),
                (r"( ?[-+~] ?)?tzdata==\d+(\.\d+)+( [\\]\n\s+--hash=.*)?\n(\s+# via .*\n)?"),
                (r"( ?[-+~] ?)?tzdata==\d+(\.\d+)+(\s+[-+~]?\s+# via .*)?\n"),
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
                        WindowsFilters::Platform => [
                            "Resolved",
                            "Prepared",
                            "Installed",
                            "Audited",
                            "Uninstalled",
                        ]
                        .iter(),
                        WindowsFilters::Universal => {
                            ["Prepared", "Installed", "Audited", "Uninstalled"].iter()
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

    let status = output.status;
    (snapshot, output, status)
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
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#
    };
    fs_err::create_dir_all(dir)?;
    fs_err::write(dir.join("pyproject.toml"), pyproject_toml)?;
    fs_err::create_dir(dir.join(name))?;
    fs_err::write(dir.join(name).join("__init__.py"), "")?;
    Ok(())
}

// This is a fine-grained token that only has read-only access to the `uv-private-pypackage` repository
pub const READ_ONLY_GITHUB_TOKEN: &[&str] = &[
    "Z2l0aHViX3BhdA==",
    "MTFCR0laQTdRMGdSQ0JRQVdRTklyQgo=",
    "cU5vakhySFV2a0ljNUVZY1pzd1k0bUFUWlBuU3VLVDV5eXR0WUxvcHh3UFI0NlpWTlRTblhvVHJHSXEK",
];

// This is a fine-grained token that only has read-only access to the `uv-private-pypackage-2` repository
#[cfg(not(windows))]
pub const READ_ONLY_GITHUB_TOKEN_2: &[&str] = &[
    "Z2l0aHViX3BhdA==",
    "MTFCR0laQTdRMGthWlY4dHppTDdQSwo=",
    "SHIzUG1tRVZRSHMzQTl2a3NiVnB4Tmk0eTR3R2JVYklLck1qY05naHhMSFVMTDZGVElIMXNYeFhYN2gK",
];

/// Decode a split, base64 encoded authentication token.
/// We split and encode the token to bypass revoke by GitHub's secret scanning
pub fn decode_token(content: &[&str]) -> String {
    let token = content
        .iter()
        .map(|part| base64.decode(part).unwrap())
        .map(|decoded| {
            std::str::from_utf8(decoded.as_slice())
                .unwrap()
                .trim_end()
                .to_string()
        })
        .join("_");
    token
}

/// Simulates `reqwest::blocking::get` but returns bytes directly, and disables
/// certificate verification, passing through the `BaseClient`
#[tokio::main(flavor = "current_thread")]
pub async fn download_to_disk(url: &str, path: &Path) {
    let trusted_hosts: Vec<_> = env::var(EnvVars::UV_INSECURE_HOST)
        .unwrap_or_default()
        .split(' ')
        .map(|h| uv_configuration::TrustedHost::from_str(h).unwrap())
        .collect();

    let client = uv_client::BaseClientBuilder::new()
        .allow_insecure_host(trusted_hosts)
        .build();
    let url: reqwest::Url = url.parse().unwrap();
    let client = client.for_host(&url);
    let response = client.request(http::Method::GET, url).send().await.unwrap();

    let mut file = tokio::fs::File::create(path).await.unwrap();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        file.write_all(&chunk.unwrap()).await.unwrap();
    }
    file.sync_all().await.unwrap();
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
        let (snapshot, output) = $crate::common::run_and_format($spawnable, &$filters, $crate::function_name!(), Some($crate::common::WindowsFilters::Platform));
        ::insta::assert_snapshot!(snapshot, @$snapshot);
        output
    }};
    ($filters:expr, windows_filters=false, $spawnable:expr, @$snapshot:literal) => {{
        // Take a reference for backwards compatibility with the vec-expecting insta filters.
        let (snapshot, output) = $crate::common::run_and_format($spawnable, &$filters, $crate::function_name!(), None);
        ::insta::assert_snapshot!(snapshot, @$snapshot);
        output
    }};
    ($filters:expr, universal_windows_filters=true, $spawnable:expr, @$snapshot:literal) => {{
        // Take a reference for backwards compatibility with the vec-expecting insta filters.
        let (snapshot, output) = $crate::common::run_and_format($spawnable, &$filters, $crate::function_name!(), Some($crate::common::WindowsFilters::Universal));
        ::insta::assert_snapshot!(snapshot, @$snapshot);
        output
    }};
}

/// <https://stackoverflow.com/a/31749071/3549270>
#[allow(unused_imports)]
pub(crate) use uv_snapshot;
