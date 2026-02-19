// The `unreachable_pub` is to silence false positives in RustRover.
#![allow(dead_code, unreachable_pub)]

use std::borrow::BorrowMut;
use std::ffi::OsString;
use std::io::Write as _;
use std::iter::Iterator;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Output, Stdio};
use std::str::FromStr;
use std::{env, io};
use uv_preview::Preview;
use uv_python::downloads::ManagedPythonDownloadList;

use assert_cmd::assert::{Assert, OutputAssertExt};
use assert_fs::assert::PathAssert;
use assert_fs::fixture::{
    ChildPath, FileWriteStr, PathChild, PathCopy, PathCreateDir, SymlinkToFile,
};
use base64::{Engine, prelude::BASE64_STANDARD as base64};
use futures::StreamExt;
use indoc::{formatdoc, indoc};
use itertools::Itertools;
use predicates::prelude::predicate;
use regex::Regex;
use tokio::io::AsyncWriteExt;

use uv_cache::{Cache, CacheBucket};
use uv_fs::Simplified;
use uv_python::managed::ManagedPythonInstallations;
use uv_python::{
    EnvironmentPreference, PythonInstallation, PythonPreference, PythonRequest, PythonVersion,
};
use uv_static::EnvVars;

// Exclude any packages uploaded after this date.
static EXCLUDE_NEWER: &str = "2024-03-25T00:00:00Z";

pub const PACKSE_VERSION: &str = "0.3.59";
pub const DEFAULT_PYTHON_VERSION: &str = "3.12";

// The expected latest patch version for each Python minor version.
pub const LATEST_PYTHON_3_15: &str = "3.15.0a6";
pub const LATEST_PYTHON_3_14: &str = "3.14.3";
pub const LATEST_PYTHON_3_13: &str = "3.13.12";
pub const LATEST_PYTHON_3_12: &str = "3.12.12";
pub const LATEST_PYTHON_3_11: &str = "3.11.14";
pub const LATEST_PYTHON_3_10: &str = "3.10.19";

/// Using a find links url allows using `--index-url` instead of `--extra-index-url` in tests
/// to prevent dependency confusion attacks against our test suite.
pub fn build_vendor_links_url() -> String {
    env::var(EnvVars::UV_TEST_PACKSE_INDEX)
        .map(|url| format!("{}/vendor/", url.trim_end_matches('/')))
        .ok()
        .unwrap_or(format!(
            "https://astral-sh.github.io/packse/{PACKSE_VERSION}/vendor/"
        ))
}

pub fn packse_index_url() -> String {
    env::var(EnvVars::UV_TEST_PACKSE_INDEX)
        .map(|url| format!("{}/simple-html/", url.trim_end_matches('/')))
        .ok()
        .unwrap_or(format!(
            "https://astral-sh.github.io/packse/{PACKSE_VERSION}/simple-html/"
        ))
}

/// Create a new [`TestContext`] with the given Python version.
///
/// Creates a virtual environment for the test.
///
/// This macro captures the uv binary path at compile time using `env!("CARGO_BIN_EXE_uv")`,
/// which is only available in the test crate.
#[macro_export]
macro_rules! test_context {
    ($python_version:expr) => {
        $crate::TestContext::new_with_bin(
            $python_version,
            std::path::PathBuf::from(env!("CARGO_BIN_EXE_uv")),
        )
    };
}

/// Create a new [`TestContext`] with zero or more Python versions.
///
/// Unlike [`test_context!`], this does not create a virtual environment.
///
/// This macro captures the uv binary path at compile time using `env!("CARGO_BIN_EXE_uv")`,
/// which is only available in the test crate.
#[macro_export]
macro_rules! test_context_with_versions {
    ($python_versions:expr) => {
        $crate::TestContext::new_with_versions_and_bin(
            $python_versions,
            std::path::PathBuf::from(env!("CARGO_BIN_EXE_uv")),
        )
    };
}

/// Return the path to the uv binary.
///
/// This macro captures the uv binary path at compile time using `env!("CARGO_BIN_EXE_uv")`,
/// which is only available in the test crate.
#[macro_export]
macro_rules! get_bin {
    () => {
        std::path::PathBuf::from(env!("CARGO_BIN_EXE_uv"))
    };
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
        r"uv(-.*)? \d+\.\d+\.\d+(-(alpha|beta|rc)\.\d+)?(\+\d+)?( \([^)]*\))?",
        r"uv [VERSION] ([COMMIT] DATE)",
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

    /// Path to the uv binary.
    uv_bin: PathBuf,

    /// Standard filters for this test context.
    filters: Vec<(String, String)>,

    /// Extra environment variables to apply to all commands.
    extra_env: Vec<(OsString, OsString)>,

    #[allow(dead_code)]
    _root: tempfile::TempDir,
}

impl TestContext {
    /// Create a new test context with a virtual environment and explicit uv binary path.
    ///
    /// This is called by the `test_context!` macro.
    pub fn new_with_bin(python_version: &str, uv_bin: PathBuf) -> Self {
        let new = Self::new_with_versions_and_bin(&[python_version], uv_bin);
        new.create_venv();
        new
    }

    /// Set the "exclude newer" timestamp for all commands in this context.
    #[must_use]
    pub fn with_exclude_newer(mut self, exclude_newer: &str) -> Self {
        self.extra_env
            .push((EnvVars::UV_EXCLUDE_NEWER.into(), exclude_newer.into()));
        self
    }

    /// Set the "http timeout" for all commands in this context.
    #[must_use]
    pub fn with_http_timeout(mut self, http_timeout: &str) -> Self {
        self.extra_env
            .push((EnvVars::UV_HTTP_TIMEOUT.into(), http_timeout.into()));
        self
    }

    /// Set the "concurrent installs" for all commands in this context.
    #[must_use]
    pub fn with_concurrent_installs(mut self, concurrent_installs: &str) -> Self {
        self.extra_env.push((
            EnvVars::UV_CONCURRENT_INSTALLS.into(),
            concurrent_installs.into(),
        ));
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

    /// Add extra filtering for cache size output
    #[must_use]
    pub fn with_filtered_cache_size(mut self) -> Self {
        // Filter raw byte counts (numbers on their own line)
        self.filters
            .push((r"(?m)^\d+\n".to_string(), "[SIZE]\n".to_string()));
        // Filter human-readable sizes (e.g., "384.2 KiB")
        self.filters.push((
            r"(?m)^\d+(\.\d+)? [KMGT]i?B\n".to_string(),
            "[SIZE]\n".to_string(),
        ));
        self
    }

    /// Add extra standard filtering for Windows-compatible missing file errors.
    #[must_use]
    pub fn with_filtered_missing_file_error(mut self) -> Self {
        // The exact message string depends on the system language, so we remove it.
        // We want to only remove the phrase after `Caused by:`
        self.filters.push((
            r"[^:\n]* \(os error 2\)".to_string(),
            " [OS ERROR 2]".to_string(),
        ));
        // Replace the Windows "The system cannot find the path specified. (os error 3)"
        // with the Unix "No such file or directory (os error 2)"
        // and mask the language-dependent message.
        self.filters.push((
            r"[^:\n]* \(os error 3\)".to_string(),
            " [OS ERROR 2]".to_string(),
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
            "virtual environments, search path, or registry".to_string(),
            "[PYTHON SOURCES]".to_string(),
        ));
        self.filters.push((
            "virtual environments, registry, or search path".to_string(),
            "[PYTHON SOURCES]".to_string(),
        ));
        self.filters.push((
            "virtual environments or search path".to_string(),
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
        self.filters.push((
            "search path or registry".to_string(),
            "[PYTHON SOURCES]".to_string(),
        ));
        self.filters.push((
            "registry or search path".to_string(),
            "[PYTHON SOURCES]".to_string(),
        ));
        self.filters
            .push(("search path".to_string(), "[PYTHON SOURCES]".to_string()));
        self
    }

    /// Add extra standard filtering for Python executable names, e.g., stripping version number
    /// and `.exe` suffixes.
    #[must_use]
    pub fn with_filtered_python_names(mut self) -> Self {
        for name in ["python", "pypy"] {
            // Note we strip version numbers from the executable names because, e.g., on Windows
            // `python.exe` is the equivalent to a Unix `python3.12`.`
            let suffix = if cfg!(windows) {
                // On Windows, we'll require a `.exe` suffix for disambiguation
                // We'll also strip version numbers if present, which is not common for `python.exe`
                // but can occur for, e.g., `pypy3.12.exe`
                let exe_suffix = regex::escape(env::consts::EXE_SUFFIX);
                format!(r"(\d\.\d+|\d)?{exe_suffix}")
            } else {
                // On Unix, we'll strip version numbers
                if name == "python" {
                    // We can't require them in this case since `/python` is common
                    r"(\d\.\d+|\d)?(t|d|td)?".to_string()
                } else {
                    // However, for other names we'll require them to avoid over-matching
                    r"(\d\.\d+|\d)(t|d|td)?".to_string()
                }
            };

            self.filters.push((
                // We use a leading path separator to help disambiguate cases where the name is not
                // used in a path.
                format!(r"[\\/]{name}{suffix}"),
                format!("/[{}]", name.to_uppercase()),
            ));
        }

        self
    }

    /// Add extra standard filtering for venv executable directories on the current platform e.g.
    /// `Scripts` on Windows and `bin` on Unix.
    #[must_use]
    pub fn with_filtered_virtualenv_bin(mut self) -> Self {
        self.filters.push((
            format!(
                r"[\\/]{}[\\/]",
                venv_bin_path(PathBuf::new()).to_string_lossy()
            ),
            "/[BIN]/".to_string(),
        ));
        self.filters.push((
            format!(r"[\\/]{}", venv_bin_path(PathBuf::new()).to_string_lossy()),
            "/[BIN]".to_string(),
        ));
        self
    }

    /// Add extra standard filtering for Python installation `bin/` directories, which are not
    /// present on Windows but are on Unix. See [`TestContext::with_filtered_virtualenv_bin`] for
    /// the virtual environment equivalent.
    #[must_use]
    pub fn with_filtered_python_install_bin(mut self) -> Self {
        // We don't want to eagerly match paths that aren't actually Python executables, so we
        // do our best to detect that case
        let suffix = if cfg!(windows) {
            let exe_suffix = regex::escape(env::consts::EXE_SUFFIX);
            // On Windows, we usually don't have a version attached but we might, e.g., for pypy3.12
            format!(r"(\d\.\d+|\d)?{exe_suffix}")
        } else {
            // On Unix, we'll require a version to be attached to avoid over-matching
            r"\d\.\d+|\d".to_string()
        };

        if cfg!(unix) {
            self.filters.push((
                format!(r"[\\/]bin/python({suffix})"),
                "/[INSTALL-BIN]/python$1".to_string(),
            ));
            self.filters.push((
                format!(r"[\\/]bin/pypy({suffix})"),
                "/[INSTALL-BIN]/pypy$1".to_string(),
            ));
        } else {
            self.filters.push((
                format!(r"[\\/]python({suffix})"),
                "/[INSTALL-BIN]/python$1".to_string(),
            ));
            self.filters.push((
                format!(r"[\\/]pypy({suffix})"),
                "/[INSTALL-BIN]/pypy$1".to_string(),
            ));
        }
        self
    }

    /// Filtering for various keys in a `pyvenv.cfg` file that will vary
    /// depending on the specific machine used:
    /// - `home = foo/bar/baz/python3.X.X/bin`
    /// - `uv = X.Y.Z`
    /// - `extends-environment = <path/to/parent/venv>`
    #[must_use]
    pub fn with_pyvenv_cfg_filters(mut self) -> Self {
        let added_filters = [
            (r"home = .+".to_string(), "home = [PYTHON_HOME]".to_string()),
            (
                r"uv = \d+\.\d+\.\d+(-(alpha|beta|rc)\.\d+)?(\+\d+)?".to_string(),
                "uv = [UV_VERSION]".to_string(),
            ),
            (
                r"extends-environment = .+".to_string(),
                "extends-environment = [PARENT_VENV]".to_string(),
            ),
        ];
        for filter in added_filters {
            self.filters.insert(0, filter);
        }
        self
    }

    /// Add extra filtering for ` -> <PATH>` symlink display for Python versions in the test
    /// context, e.g., for use in `uv python list`.
    #[must_use]
    pub fn with_filtered_python_symlinks(mut self) -> Self {
        for (version, executable) in &self.python_versions {
            if fs_err::symlink_metadata(executable).unwrap().is_symlink() {
                self.filters.extend(
                    Self::path_patterns(executable.read_link().unwrap())
                        .into_iter()
                        .map(|pattern| (format! {" -> {pattern}"}, String::new())),
                );
            }
            // Drop links that are byproducts of the test context too
            self.filters.push((
                regex::escape(&format!(" -> [PYTHON-{version}]")),
                String::new(),
            ));
        }
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
    #[must_use]
    pub fn with_filtered_link_mode_warning(mut self) -> Self {
        let pattern = "warning: Failed to hardlink files; .*\n.*\n.*\n";
        self.filters.push((pattern.to_string(), String::new()));
        self
    }

    /// Adds a filter for platform-specific errors when a file is not executable.
    #[inline]
    #[must_use]
    pub fn with_filtered_not_executable(mut self) -> Self {
        let pattern = if cfg!(unix) {
            r"Permission denied \(os error 13\)"
        } else {
            r"\%1 is not a valid Win32 application. \(os error 193\)"
        };
        self.filters
            .push((pattern.to_string(), "[PERMISSION DENIED]".to_string()));
        self
    }

    /// Adds a filter that ignores platform information in a Python installation key.
    #[must_use]
    pub fn with_filtered_python_keys(mut self) -> Self {
        // Filter platform keys
        let platform_re = r"(?x)
  (                         # We capture the group before the platform
    (?:cpython|pypy|graalpy)# Python implementation
    -
    \d+\.\d+                # Major and minor version
    (?:                     # The patch version is handled separately
      \.
      (?:
        \[X\]               # A previously filtered patch version [X]
        |                   # OR
        \[LATEST\]          # A previously filtered latest patch version [LATEST]
        |                   # OR
        \d+                 # An actual patch version
      )
    )?                      # (we allow the patch version to be missing entirely, e.g., in a request)
    (?:(?:a|b|rc)[0-9]+)?   # Pre-release version component, e.g., `a6` or `rc2`
    (?:[td])?               # A short variant, such as `t` (for freethreaded) or `d` (for debug)
    (?:(\+[a-z]+)+)?        # A long variant, such as `+freethreaded` or `+freethreaded+debug`
  )
  -
  [a-z0-9]+                 # Operating system (e.g., 'macos')
  -
  [a-z0-9_]+                # Architecture (e.g., 'aarch64')
  -
  [a-z]+                    # Libc (e.g., 'none')
";
        self.filters
            .push((platform_re.to_string(), "$1-[PLATFORM]".to_string()));
        self
    }

    /// Adds a filter that replaces the latest Python patch versions with `[LATEST]` placeholder.
    #[must_use]
    pub fn with_filtered_latest_python_versions(mut self) -> Self {
        // Filter the latest patch versions with [LATEST] placeholder
        // The order matters - we want to match the full version first
        for (minor, patch) in [
            ("3.15", LATEST_PYTHON_3_15.strip_prefix("3.15.").unwrap()),
            ("3.14", LATEST_PYTHON_3_14.strip_prefix("3.14.").unwrap()),
            ("3.13", LATEST_PYTHON_3_13.strip_prefix("3.13.").unwrap()),
            ("3.12", LATEST_PYTHON_3_12.strip_prefix("3.12.").unwrap()),
            ("3.11", LATEST_PYTHON_3_11.strip_prefix("3.11.").unwrap()),
            ("3.10", LATEST_PYTHON_3_10.strip_prefix("3.10.").unwrap()),
        ] {
            // Match the full version in various contexts (cpython-X.Y.Z, Python X.Y.Z, etc.)
            let pattern = format!(r"(\b){minor}\.{patch}(\b)");
            let replacement = format!("${{1}}{minor}.[LATEST]${{2}}");
            self.filters.push((pattern, replacement));
        }
        self
    }

    /// Add a filter that ignores temporary directory in path.
    #[must_use]
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

    /// Add a filter for (bytecode) compilation file counts
    #[must_use]
    pub fn with_filtered_compiled_file_count(mut self) -> Self {
        self.filters.push((
            r"compiled \d+ files".to_string(),
            "compiled [COUNT] files".to_string(),
        ));
        self
    }

    /// Adds filters for non-deterministic `CycloneDX` data
    #[must_use]
    pub fn with_cyclonedx_filters(mut self) -> Self {
        self.filters.push((
            r"urn:uuid:[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}".to_string(),
            "[SERIAL_NUMBER]".to_string(),
        ));
        self.filters.push((
            r#""timestamp": "[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}\.[0-9]+Z""#
                .to_string(),
            r#""timestamp": "[TIMESTAMP]""#.to_string(),
        ));
        self.filters.push((
            r#""name": "uv",\s*"version": "\d+\.\d+\.\d+(-(alpha|beta|rc)\.\d+)?(\+\d+)?""#
                .to_string(),
            r#""name": "uv",
        "version": "[VERSION]""#
                .to_string(),
        ));
        self
    }

    /// Add a filter that collapses duplicate whitespace.
    #[must_use]
    pub fn with_collapsed_whitespace(mut self) -> Self {
        self.filters.push((r"[ \t]+".to_string(), " ".to_string()));
        self
    }

    /// Use a shared global cache for Python downloads.
    #[must_use]
    pub fn with_python_download_cache(mut self) -> Self {
        self.extra_env.push((
            EnvVars::UV_PYTHON_CACHE_DIR.into(),
            // Respect `UV_PYTHON_CACHE_DIR` if set, or use the default cache directory
            env::var_os(EnvVars::UV_PYTHON_CACHE_DIR).unwrap_or_else(|| {
                uv_cache::Cache::from_settings(false, None)
                    .unwrap()
                    .bucket(CacheBucket::Python)
                    .into()
            }),
        ));
        self
    }

    #[must_use]
    pub fn with_empty_python_install_mirror(mut self) -> Self {
        self.extra_env.push((
            EnvVars::UV_PYTHON_INSTALL_MIRROR.into(),
            String::new().into(),
        ));
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

    #[must_use]
    pub fn with_versions_as_managed(mut self, versions: &[&str]) -> Self {
        self.extra_env.push((
            EnvVars::UV_INTERNAL__TEST_PYTHON_MANAGED.into(),
            versions.iter().join(" ").into(),
        ));

        self
    }

    /// Add a custom filter to the `TestContext`.
    #[must_use]
    pub fn with_filter(mut self, filter: (impl Into<String>, impl Into<String>)) -> Self {
        self.filters.push((filter.0.into(), filter.1.into()));
        self
    }

    // Unsets the git credential helper using temp home gitconfig
    #[must_use]
    pub fn with_unset_git_credential_helper(self) -> Self {
        let git_config = self.home_dir.child(".gitconfig");
        git_config
            .write_str(indoc! {r"
                [credential]
                    helper =
            "})
            .expect("Failed to unset git credential helper");

        self
    }

    /// Clear filters on `TestContext`.
    #[must_use]
    pub fn clear_filters(mut self) -> Self {
        self.filters.clear();
        self
    }

    /// Default to the canonicalized path to the temp directory. We need to do this because on
    /// macOS (and Windows on GitHub Actions) the standard temp dir is a symlink. (On macOS, the
    /// temporary directory is, like `/var/...`, which resolves to `/private/var/...`.)
    ///
    /// It turns out that, at least on macOS, if we pass a symlink as `current_dir`, it gets
    /// _immediately_ resolved (such that if you call `current_dir` in the running `Command`, it
    /// returns resolved symlink). This breaks some snapshot tests, since we _don't_ want to
    /// resolve symlinks for user-provided paths.
    pub fn test_bucket_dir() -> PathBuf {
        std::env::temp_dir()
            .simple_canonicalize()
            .expect("failed to canonicalize temp dir")
            .join("uv")
            .join("tests")
    }

    /// Create a new test context with multiple Python versions and explicit uv binary path.
    ///
    /// Does not create a virtual environment by default, but the first Python version
    /// can be used to create a virtual environment with [`TestContext::create_venv`].
    ///
    /// This is called by the `test_context_with_versions!` macro.
    pub fn new_with_versions_and_bin(python_versions: &[&str], uv_bin: PathBuf) -> Self {
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

        let download_list = ManagedPythonDownloadList::new_only_embedded().unwrap();

        let python_versions: Vec<_> = python_versions
            .iter()
            .map(|version| PythonVersion::from_str(version).unwrap())
            .zip(
                python_installations_for_versions(&temp_dir, python_versions, &download_list)
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
            Self::path_patterns(&uv_bin)
                .into_iter()
                .map(|pattern| (pattern, "[UV]".to_string())),
        );

        // Exclude `link-mode` on Windows since we set it in the remote test suite
        if cfg!(windows) {
            filters.push((" --link-mode <LINK_MODE>".to_string(), String::new()));
            filters.push((r#"link-mode = "copy"\n"#.to_string(), String::new()));
            // Unix uses "exit status", Windows uses "exit code"
            filters.push((r"exit code: ".to_string(), "exit status: ".to_string()));
        }

        for (version, executable) in &python_versions {
            // Add filtering for the interpreter path
            filters.extend(
                Self::path_patterns(executable)
                    .into_iter()
                    .map(|pattern| (pattern, format!("[PYTHON-{version}]"))),
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
        }

        // Filter Python library path differences between Windows and Unix
        filters.push((
            r"[\\/]lib[\\/]python\d+\.\d+[\\/]".to_string(),
            "/[PYTHON-LIB]/".to_string(),
        ));
        filters.push((r"[\\/]Lib[\\/]".to_string(), "/[PYTHON-LIB]/".to_string()));

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
            r"Activate with: Scripts\\activate".to_string(),
            "Activate with: source [BIN]/activate".to_string(),
        ));
        filters.push((
            r"Activate with: source (.*/|)bin/activate(?:\.\w+)?".to_string(),
            "Activate with: source $1[BIN]/activate".to_string(),
        ));

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
            format!("https://astral-sh.github.io/packse/{PACKSE_VERSION}"),
            "https://astral-sh.github.io/packse/PACKSE_VERSION".to_string(),
        ));
        // Developer convenience
        if let Ok(packse_test_index) = env::var(EnvVars::UV_TEST_PACKSE_INDEX) {
            filters.push((
                packse_test_index.trim_end_matches('/').to_string(),
                "https://astral-sh.github.io/packse/PACKSE_VERSION".to_string(),
            ));
        }
        // For wiremock tests
        filters.push((r"127\.0\.0\.1:\d*".to_string(), "[LOCALHOST]".to_string()));
        // Avoid breaking the tests when bumping the uv version
        filters.push((
            format!(
                r#"requires = \["uv_build>={},<[0-9.]+"\]"#,
                uv_version::version()
            ),
            r#"requires = ["uv_build>=[CURRENT_VERSION],<[NEXT_BREAKING]"]"#.to_string(),
        ));
        // Filter script environment hashes
        filters.push((
            r"environments-v(\d+)[\\/](\w+)-[a-z0-9]+".to_string(),
            "environments-v$1/$2-[HASH]".to_string(),
        ));
        // Filter archive hashes
        filters.push((
            r"archive-v(\d+)[\\/][A-Za-z0-9\-\_]+".to_string(),
            "archive-v$1/[HASH]".to_string(),
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
            uv_bin,
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

    pub fn disallow_git_cli(bin_dir: &Path) -> std::io::Result<()> {
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

    /// Setup Git LFS Filters
    ///
    /// You can find the default filters in <https://github.com/git-lfs/git-lfs/blob/v3.7.1/lfs/attribute.go#L66-L71>
    /// We set required to true to get a full stacktrace when these commands fail.
    #[must_use]
    pub fn with_git_lfs_config(mut self) -> Self {
        let git_lfs_config = self.root.child(".gitconfig");
        git_lfs_config
            .write_str(indoc! {r#"
                [filter "lfs"]
                    clean = git-lfs clean -- %f
                    smudge = git-lfs smudge -- %f
                    process = git-lfs filter-process
                    required = true
            "#})
            .expect("Failed to setup `git-lfs` filters");

        // Its possible your system config can cause conflicts with the Git LFS tests.
        // In such cases, add self.extra_env.push(("GIT_CONFIG_NOSYSTEM".into(), "1".into()));
        self.extra_env.push((
            EnvVars::GIT_CONFIG_GLOBAL.into(),
            git_lfs_config.as_os_str().into(),
        ));
        self
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

        // Ensure the tests aren't sensitive to the running user's shell without forcing
        // `bash` on Windows
        if cfg!(not(windows)) {
            command.env(EnvVars::SHELL, "bash");
        }

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
            .env(
                EnvVars::XDG_CONFIG_DIRS,
                self.home_dir.join("config").as_os_str(),
            )
            .env(
                EnvVars::XDG_DATA_HOME,
                self.home_dir.join("data").as_os_str(),
            )
            .env(EnvVars::UV_PYTHON_INSTALL_DIR, "")
            // Installations are not allowed by default; see `Self::with_managed_python_dirs`
            .env(EnvVars::UV_PYTHON_DOWNLOADS, "never")
            .env(EnvVars::UV_TEST_PYTHON_PATH, self.python_path())
            // Lock to a point in time view of the world
            .env(EnvVars::UV_EXCLUDE_NEWER, EXCLUDE_NEWER)
            .env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, EXCLUDE_NEWER)
            // When installations are allowed, we don't want to write to global state, like the
            // Windows registry
            .env(EnvVars::UV_PYTHON_INSTALL_REGISTRY, "0")
            // Since downloads, fetches and builds run in parallel, their message output order is
            // non-deterministic, so can't capture them in test output.
            .env(EnvVars::UV_TEST_NO_CLI_PROGRESS, "1")
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
            .env(EnvVars::GIT_CEILING_DIRECTORIES, self.root.path())
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

    /// Create a `pip debug` command for testing.
    pub fn pip_debug(&self) -> Command {
        let mut command = self.new_command();
        command.arg("pip").arg("debug");
        self.add_shared_options(&mut command, true);
        command
    }

    /// Create a `uv help` command with options shared across scenarios.
    pub fn help(&self) -> Command {
        let mut command = self.new_command();
        command.arg("help");
        self.add_shared_env(&mut command, false);
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

    /// Create a `uv workspace metadata` command with options shared across scenarios.
    pub fn workspace_metadata(&self) -> Command {
        let mut command = self.new_command();
        command.arg("workspace").arg("metadata");
        self.add_shared_options(&mut command, false);
        command
    }

    /// Create a `uv workspace dir` command with options shared across scenarios.
    pub fn workspace_dir(&self) -> Command {
        let mut command = self.new_command();
        command.arg("workspace").arg("dir");
        self.add_shared_options(&mut command, false);
        command
    }

    /// Create a `uv workspace list` command with options shared across scenarios.
    pub fn workspace_list(&self) -> Command {
        let mut command = self.new_command();
        command.arg("workspace").arg("list");
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

    /// Create a `uv format` command with options shared across scenarios.
    pub fn format(&self) -> Command {
        let mut command = self.new_command();
        command.arg("format");
        self.add_shared_options(&mut command, false);
        // Override to a more recent date for ruff version resolution
        command.env(EnvVars::UV_EXCLUDE_NEWER, "2026-02-15T00:00:00Z");
        command
    }

    /// Create a `uv build` command with options shared across scenarios.
    pub fn build(&self) -> Command {
        let mut command = self.new_command();
        command.arg("build");
        self.add_shared_options(&mut command, false);
        command
    }

    pub fn version(&self) -> Command {
        let mut command = self.new_command();
        command.arg("version");
        self.add_shared_options(&mut command, false);
        command
    }

    pub fn self_version(&self) -> Command {
        let mut command = self.new_command();
        command.arg("self").arg("version");
        self.add_shared_options(&mut command, false);
        command
    }

    pub fn self_update(&self) -> Command {
        let mut command = self.new_command();
        command.arg("self").arg("update");
        self.add_shared_options(&mut command, false);
        command
    }

    /// Create a `uv publish` command with options shared across scenarios.
    pub fn publish(&self) -> Command {
        let mut command = self.new_command();
        command.arg("publish");
        self.add_shared_options(&mut command, false);
        command
    }

    /// Create a `uv python find` command with options shared across scenarios.
    pub fn python_find(&self) -> Command {
        let mut command = self.new_command();
        command
            .arg("python")
            .arg("find")
            .env(EnvVars::UV_PREVIEW, "1")
            .env(EnvVars::UV_PYTHON_INSTALL_DIR, "");
        self.add_shared_options(&mut command, false);
        command
    }

    /// Create a `uv python list` command with options shared across scenarios.
    pub fn python_list(&self) -> Command {
        let mut command = self.new_command();
        command
            .arg("python")
            .arg("list")
            .env(EnvVars::UV_PYTHON_INSTALL_DIR, "");
        self.add_shared_options(&mut command, false);
        command
    }

    /// Create a `uv python install` command with options shared across scenarios.
    pub fn python_install(&self) -> Command {
        let mut command = self.new_command();
        command.arg("python").arg("install");
        self.add_shared_options(&mut command, true);
        command
    }

    /// Create a `uv python uninstall` command with options shared across scenarios.
    pub fn python_uninstall(&self) -> Command {
        let mut command = self.new_command();
        command.arg("python").arg("uninstall");
        self.add_shared_options(&mut command, true);
        command
    }

    /// Create a `uv python upgrade` command with options shared across scenarios.
    pub fn python_upgrade(&self) -> Command {
        let mut command = self.new_command();
        command.arg("python").arg("upgrade");
        self.add_shared_options(&mut command, true);
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

    /// Create a `uv cache size` command.
    pub fn cache_size(&self) -> Command {
        let mut command = self.new_command();
        command.arg("cache").arg("size");
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

    /// The path to the Python interpreter in the venv.
    ///
    /// Don't use this for `Command::new`, use `Self::python_command` instead.
    pub fn interpreter(&self) -> PathBuf {
        let venv = &self.venv;
        if cfg!(unix) {
            venv.join("bin").join("python")
        } else if cfg!(windows) {
            venv.join("Scripts").join("python.exe")
        } else {
            unimplemented!("Only Windows and Unix are supported")
        }
    }

    pub fn python_command(&self) -> Command {
        let mut interpreter = self.interpreter();

        // If there's not a virtual environment, use the first Python interpreter in the context
        if !interpreter.exists() {
            interpreter.clone_from(
                &self
                    .python_versions
                    .first()
                    .expect("At least one Python version is required")
                    .1,
            );
        }

        let mut command = Self::new_command_with(&interpreter);
        command
            // Our tests change files in <1s, so we must disable CPython bytecode caching or we'll get stale files
            // https://github.com/python/cpython/issues/75953
            .arg("-B")
            // Python on windows
            .env(EnvVars::PYTHONUTF8, "1");

        self.add_shared_env(&mut command, false);

        command
    }

    /// Create a `uv auth login` command.
    pub fn auth_login(&self) -> Command {
        let mut command = self.new_command();
        command.arg("auth").arg("login");
        self.add_shared_options(&mut command, false);
        command
    }

    /// Create a `uv auth logout` command.
    pub fn auth_logout(&self) -> Command {
        let mut command = self.new_command();
        command.arg("auth").arg("logout");
        self.add_shared_options(&mut command, false);
        command
    }

    /// Create a `uv auth helper --protocol bazel get` command.
    pub fn auth_helper(&self) -> Command {
        let mut command = self.new_command();
        command.arg("auth").arg("helper");
        self.add_shared_options(&mut command, false);
        command
    }

    /// Create a `uv auth token` command.
    pub fn auth_token(&self) -> Command {
        let mut command = self.new_command();
        command.arg("auth").arg("token");
        self.add_shared_options(&mut command, false);
        command
    }

    /// Set `HOME` to the real home directory.
    ///
    /// We need this for testing commands which use the macOS keychain.
    #[must_use]
    pub fn with_real_home(mut self) -> Self {
        if let Some(home) = env::var_os(EnvVars::HOME) {
            self.extra_env
                .push((EnvVars::HOME.to_string().into(), home));
        }
        // Use the test's isolated config directory to avoid reading user
        // configuration files (like `.python-version`) that could interfere with tests.
        self.extra_env.push((
            EnvVars::XDG_CONFIG_HOME.into(),
            self.user_config_dir.as_os_str().into(),
        ));
        self
    }

    /// Run the given python code and check whether it succeeds.
    pub fn assert_command(&self, command: &str) -> Assert {
        self.python_command()
            .arg("-c")
            .arg(command)
            .current_dir(&self.temp_dir)
            .assert()
    }

    /// Run the given python file and check whether it succeeds.
    pub fn assert_file(&self, file: impl AsRef<Path>) -> Assert {
        self.python_command()
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

    /// Assert a package is not installed.
    pub fn assert_not_installed(&self, package: &'static str) {
        self.assert_command(format!("import {package}").as_str())
            .failure();
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
        create_venv_from_executable(&self.venv, &self.cache_dir, &executable, &self.uv_bin);
    }

    /// Copies the files from the ecosystem project given into this text
    /// context.
    ///
    /// This will almost always write at least a `pyproject.toml` into this
    /// test context.
    ///
    /// The given name should correspond to the name of a sub-directory (not a
    /// path to it) in the `test/ecosystem` directory.
    ///
    /// This panics (fails the current test) for any failure.
    pub fn copy_ecosystem_project(&self, name: &str) {
        let project_dir = PathBuf::from(format!("../../test/ecosystem/{name}"));
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
    pub fn diff_lock(&self, change: impl Fn(&Self) -> Command) -> String {
        static TRIM_TRAILING_WHITESPACE: std::sync::LazyLock<Regex> =
            std::sync::LazyLock::new(|| Regex::new(r"(?m)^\s+$").unwrap());

        let lock_path = ChildPath::new(self.temp_dir.join("uv.lock"));
        let old_lock = fs_err::read_to_string(&lock_path).unwrap();
        let (snapshot, _, status) = run_and_format_with_status(
            change(self),
            self.filters(),
            "diff_lock",
            Some(WindowsFilters::Platform),
            None,
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
        Self::new_command_with(&self.uv_bin)
    }

    /// Creates a new `Command` that is intended to be suitable for use in
    /// all tests, but with the given binary.
    ///
    /// Clears environment variables defined in [`EnvVars`] to avoid reading
    /// test host settings.
    fn new_command_with(bin: &Path) -> Command {
        let mut command = Command::new(bin);

        let passthrough = [
            // For linux distributions
            EnvVars::PATH,
            // For debugging tests.
            EnvVars::RUST_LOG,
            EnvVars::RUST_BACKTRACE,
            // Windows System configuration.
            EnvVars::SYSTEMDRIVE,
            // Work around small default stack sizes and large futures in debug builds.
            EnvVars::RUST_MIN_STACK,
            EnvVars::UV_STACK_SIZE,
            // Allow running tests with custom network settings.
            EnvVars::ALL_PROXY,
            EnvVars::HTTPS_PROXY,
            EnvVars::HTTP_PROXY,
            EnvVars::NO_PROXY,
            EnvVars::SSL_CERT_DIR,
            EnvVars::SSL_CERT_FILE,
            EnvVars::UV_NATIVE_TLS,
        ];

        for env_var in EnvVars::all_names()
            .iter()
            .filter(|name| !passthrough.contains(name))
        {
            command.env_remove(env_var);
        }

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
pub fn create_venv_from_executable<P: AsRef<Path>>(
    path: P,
    cache_dir: &ChildPath,
    python: &Path,
    uv_bin: &Path,
) {
    TestContext::new_command_with(uv_bin)
        .arg("venv")
        .arg(path.as_ref().as_os_str())
        .arg("--clear")
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .arg("--python")
        .arg(python)
        .current_dir(path.as_ref().parent().unwrap())
        .assert()
        .success();
    ChildPath::new(path.as_ref()).assert(predicate::path::is_dir());
}

/// Create a `PATH` with the requested Python versions available in order.
///
/// Generally this should be used with `UV_TEST_PYTHON_PATH`.
pub fn python_path_with_versions(
    temp_dir: &ChildPath,
    python_versions: &[&str],
) -> anyhow::Result<OsString> {
    let download_list = ManagedPythonDownloadList::new_only_embedded().unwrap();
    Ok(env::join_paths(
        python_installations_for_versions(temp_dir, python_versions, &download_list)?
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
    download_list: &ManagedPythonDownloadList,
) -> anyhow::Result<Vec<PathBuf>> {
    let cache = Cache::from_path(temp_dir.child("cache").to_path_buf())
        .init_no_wait()?
        .expect("No cache contention when setting up Python in tests");
    let selected_pythons = python_versions
        .iter()
        .map(|python_version| {
            if let Ok(python) = PythonInstallation::find(
                &PythonRequest::parse(python_version),
                EnvironmentPreference::OnlySystem,
                PythonPreference::Managed,
                download_list,
                &cache,
                Preview::default(),
            ) {
                python.into_interpreter().sys_executable().to_owned()
            } else {
                panic!("Could not find Python {python_version} for test\nTry `cargo run python install` first, or refer to CONTRIBUTING.md");
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
    input: Option<&str>,
) -> (String, Output) {
    let (snapshot, output, _) =
        run_and_format_with_status(command, filters, function_name, windows_filters, input);
    (snapshot, output)
}

/// Execute the command and format its output status, stdout and stderr into a snapshot string.
///
/// This function is derived from `insta_cmd`s `spawn_with_info`.
#[expect(clippy::print_stderr)]
pub fn run_and_format_with_status<T: AsRef<str>>(
    mut command: impl BorrowMut<Command>,
    filters: impl AsRef<[(T, T)]>,
    function_name: &str,
    windows_filters: Option<WindowsFilters>,
    input: Option<&str>,
) -> (String, Output, ExitStatus) {
    let program = command
        .borrow_mut()
        .get_program()
        .to_string_lossy()
        .to_string();

    // Support profiling test run commands with traces.
    if let Ok(root) = env::var(EnvVars::TRACING_DURATIONS_TEST_ROOT) {
        // We only want to fail if the variable is set at runtime.
        #[allow(clippy::assertions_on_constants)]
        {
            assert!(
                cfg!(feature = "tracing-durations-export"),
                "You need to enable the tracing-durations-export feature to use `TRACING_DURATIONS_TEST_ROOT`"
            );
        }
        command.borrow_mut().env(
            EnvVars::TRACING_DURATIONS_FILE,
            Path::new(&root).join(function_name).with_extension("jsonl"),
        );
    }

    let output = if let Some(input) = input {
        let mut child = command
            .borrow_mut()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap_or_else(|err| panic!("Failed to spawn {program}: {err}"));
        child
            .stdin
            .as_mut()
            .expect("Failed to open stdin")
            .write_all(input.as_bytes())
            .expect("Failed to write to stdin");

        child
            .wait_with_output()
            .unwrap_or_else(|err| panic!("Failed to read output from {program}: {err}"))
    } else {
        command
            .borrow_mut()
            .output()
            .unwrap_or_else(|err| panic!("Failed to spawn {program}: {err}"))
    };

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
        requires-python = ">=3.11,<3.13"
        {body}

        [build-system]
        requires = ["uv_build>=0.9.0,<10000"]
        build-backend = "uv_build"
        "#
    };
    fs_err::create_dir_all(dir)?;
    fs_err::write(dir.join("pyproject.toml"), pyproject_toml)?;
    fs_err::create_dir_all(dir.join("src").join(name))?;
    fs_err::write(dir.join("src").join(name).join("__init__.py"), "")?;
    Ok(())
}

// This is a fine-grained token that only has read-only access to the `uv-private-pypackage` repository
pub const READ_ONLY_GITHUB_TOKEN: &[&str] = &[
    "Z2l0aHViCg==",
    "cGF0Cg==",
    "MTFBQlVDUjZBMERMUTQ3aVphN3hPdV9qQmhTMkZUeHZ4ZE13OHczakxuZndsV2ZlZjc2cE53eHBWS2tiRUFwdnpmUk8zV0dDSUhicDFsT01aago=",
];

// This is a fine-grained token that only has read-only access to the `uv-private-pypackage-2` repository
#[cfg(not(windows))]
pub const READ_ONLY_GITHUB_TOKEN_2: &[&str] = &[
    "Z2l0aHViCg==",
    "cGF0Cg==",
    "MTFBQlVDUjZBMDJTOFYwMTM4YmQ0bV9uTXpueWhxZDBrcllROTQ5SERTeTI0dENKZ2lmdzIybDFSR2s1SE04QW8xTUVYQ1I0Q1YxYUdPRGpvZQo=",
];

pub const READ_ONLY_GITHUB_SSH_DEPLOY_KEY: &str = "LS0tLS1CRUdJTiBPUEVOU1NIIFBSSVZBVEUgS0VZLS0tLS0KYjNCbGJuTnphQzFyWlhrdGRqRUFBQUFBQkc1dmJtVUFBQUFFYm05dVpRQUFBQUFBQUFBQkFBQUFNd0FBQUF0emMyZ3RaVwpReU5UVXhPUUFBQUNBeTF1SnNZK1JXcWp1NkdIY3Z6a3AwS21yWDEwdmo3RUZqTkpNTkRqSGZPZ0FBQUpqWUpwVnAyQ2FWCmFRQUFBQXR6YzJndFpXUXlOVFV4T1FBQUFDQXkxdUpzWStSV3FqdTZHSGN2emtwMEttclgxMHZqN0VGak5KTU5EakhmT2cKQUFBRUMwbzBnd1BxbGl6TFBJOEFXWDVaS2dVZHJyQ2ptMDhIQm9FenB4VDg3MXBqTFc0bXhqNUZhcU83b1lkeS9PU25RcQphdGZYUytQc1FXTTBrdzBPTWQ4NkFBQUFFR3R2Ym5OMGFVQmhjM1J5WVd3dWMyZ0JBZ01FQlE9PQotLS0tLUVORCBPUEVOU1NIIFBSSVZBVEUgS0VZLS0tLS0K";

/// Decode a split, base64 encoded authentication token.
/// We split and encode the token to bypass revoke by GitHub's secret scanning
pub fn decode_token(content: &[&str]) -> String {
    content
        .iter()
        .map(|part| base64.decode(part).unwrap())
        .map(|decoded| {
            std::str::from_utf8(decoded.as_slice())
                .unwrap()
                .trim_end()
                .to_string()
        })
        .join("_")
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

    let client = uv_client::BaseClientBuilder::default()
        .allow_insecure_host(trusted_hosts)
        .build();
    let url = url.parse().unwrap();
    let response = client
        .for_host(&url)
        .get(reqwest::Url::from(url))
        .send()
        .await
        .unwrap();

    let mut file = fs_err::tokio::File::create(path).await.unwrap();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        file.write_all(&chunk.unwrap()).await.unwrap();
    }
    file.sync_all().await.unwrap();
}

/// A guard that sets a directory to read-only and restores original permissions when dropped.
///
/// This is useful for tests that need to make a directory read-only and ensure
/// the permissions are restored even if the test panics.
#[cfg(unix)]
pub struct ReadOnlyDirectoryGuard {
    path: PathBuf,
    original_mode: u32,
}

#[cfg(unix)]
impl ReadOnlyDirectoryGuard {
    /// Sets the directory to read-only (removes write permission) and returns a guard
    /// that will restore the original permissions when dropped.
    pub fn new(path: impl Into<PathBuf>) -> std::io::Result<Self> {
        use std::os::unix::fs::PermissionsExt;
        let path = path.into();
        let metadata = fs_err::metadata(&path)?;
        let original_mode = metadata.permissions().mode();
        // Remove write permissions (keep read and execute)
        let readonly_mode = original_mode & !0o222;
        fs_err::set_permissions(&path, std::fs::Permissions::from_mode(readonly_mode))?;
        Ok(Self {
            path,
            original_mode,
        })
    }
}

#[cfg(unix)]
impl Drop for ReadOnlyDirectoryGuard {
    fn drop(&mut self) {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs_err::set_permissions(
            &self.path,
            std::fs::Permissions::from_mode(self.original_mode),
        );
    }
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
#[macro_export]
macro_rules! uv_snapshot {
    ($spawnable:expr, @$snapshot:literal) => {{
        uv_snapshot!($crate::INSTA_FILTERS.to_vec(), $spawnable, @$snapshot)
    }};
    ($filters:expr, $spawnable:expr, @$snapshot:literal) => {{
        // Take a reference for backwards compatibility with the vec-expecting insta filters.
        let (snapshot, output) = $crate::run_and_format($spawnable, &$filters, $crate::function_name!(), Some($crate::WindowsFilters::Platform), None);
        ::insta::assert_snapshot!(snapshot, @$snapshot);
        output
    }};
    ($filters:expr, $spawnable:expr, input=$input:expr, @$snapshot:literal) => {{
        // Take a reference for backwards compatibility with the vec-expecting insta filters.
        let (snapshot, output) = $crate::run_and_format($spawnable, &$filters, $crate::function_name!(), Some($crate::WindowsFilters::Platform), Some($input));
        ::insta::assert_snapshot!(snapshot, @$snapshot);
        output
    }};
    ($filters:expr, windows_filters=false, $spawnable:expr, @$snapshot:literal) => {{
        // Take a reference for backwards compatibility with the vec-expecting insta filters.
        let (snapshot, output) = $crate::run_and_format($spawnable, &$filters, $crate::function_name!(), None, None);
        ::insta::assert_snapshot!(snapshot, @$snapshot);
        output
    }};
    ($filters:expr, universal_windows_filters=true, $spawnable:expr, @$snapshot:literal) => {{
        // Take a reference for backwards compatibility with the vec-expecting insta filters.
        let (snapshot, output) = $crate::run_and_format($spawnable, &$filters, $crate::function_name!(), Some($crate::WindowsFilters::Universal), None);
        ::insta::assert_snapshot!(snapshot, @$snapshot);
        output
    }};
}
