#![allow(dead_code)]

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use assert_fs::assert::PathAssert;
use assert_fs::fixture::PathChild;
use assert_fs::TempDir;
use insta::internals::Filters;
use insta_cmd::get_cargo_bin;

pub(crate) const BIN_NAME: &str = "puffin";

pub(crate) const INSTA_FILTERS: &[(&str, &str)] = &[
    (r"--cache-dir .*", "--cache-dir [CACHE_DIR]"),
    (r"(\dm )?(\d+\.)?\d+(ms|s)", "[TIME]"),
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

/// See [`extra_filters`].
pub(crate) fn filters(windows_extra_count: usize) -> Filters {
    extra_filters(windows_extra_count, &[])
}

/// Shared filters for cargo insta.
///
/// Extra filters are run before the default filters.
///
/// The snapshots are for Unix. We try to remove the extra dependencies on windows and reduce the package counts by
/// usually one package to make the windows output match the Unix output.    
pub(crate) fn extra_filters(windows_extra_count: usize, extra_filters: &[(&str, &str)]) -> Filters {
    if cfg!(windows) {
        // Handles both install/remove messages and pip compile output
        let windows_only_deps = [
            ("( [+-] )?colorama==.*\n(    # via click\n)?", ""),
            ("( [+-] )?colorama==.*\n(    # via tqdm\n)?", ""),
            ("( [+-] )?tzdata==.*\n(    # via django\n)?", ""),
        ];
        // Usually, this reduces the package counts by one, since we're removing one windows-only dep.
        let reduce_package_counts: Vec<_> = (2..20)
            .map(|n| {
                let windows = format!(" {n} packages");
                let unix = format!(
                    " {} package{}",
                    n - windows_extra_count,
                    if n - windows_extra_count > 1 { "s" } else { "" }
                );
                (windows, unix)
            })
            .collect();
        extra_filters
            .into_iter()
            .cloned()
            .chain(windows_only_deps)
            .chain(
                reduce_package_counts
                    .iter()
                    .map(|(a, b)| (a.as_str(), b.as_str())),
            )
            .chain(INSTA_FILTERS.to_vec())
            .collect::<Vec<_>>()
            .into()
    } else if cfg!(unix) {
        extra_filters
            .into_iter()
            .cloned()
            .chain(INSTA_FILTERS.to_vec())
            .collect::<Vec<_>>()
            .into()
    } else {
        unimplemented!("Only Windows and Unix are supported")
    }
}

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
