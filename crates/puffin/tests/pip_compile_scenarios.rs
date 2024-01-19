//! DO NOT EDIT
//!
//! Generated with ./scripts/scenarios/update.py
//! Scenarios from <https://github.com/zanieb/packse/tree/4ec7476dbf0bd0892dd879461bcb1ad2449d69d6/scenarios>
//!
#![cfg(all(feature = "python", feature = "pypi"))]

use std::process::Command;

use anyhow::Result;
use assert_fs::fixture::{FileWriteStr, PathChild};
use insta_cmd::_macro_support::insta;
use insta_cmd::{assert_cmd_snapshot, get_cargo_bin};

use common::{create_venv, BIN_NAME, INSTA_FILTERS};

mod common;

/// requires-python-version-greater-than-current-resolver-override
///
/// The user requires a package which requires a Python version greater than the
/// current version, but they use an alternative Python version for package
/// resolution.
///
/// ```text
/// b6505624
/// ├── environment
/// │   └── python3.9
/// ├── root
/// │   └── requires a==1.0.0
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
///         └── requires python>=3.10 (incompatible with environment)
/// ```
#[test]
fn requires_python_version_greater_than_current_resolver_override() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.9");

    // In addition to the standard filters, swap out package names for more realistic messages
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"a-b6505624", "albatross"));
    filters.push((r"-b6505624", ""));

    let requirements_in = temp_dir.child("requirements.in");
    requirements_in.write_str("a-b6505624==1.0.0")?;

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("compile")
            .arg("requirements.in")
            .arg("--python-version=3.11")
            .arg("--extra-index-url")
            .arg("https://test.pypi.org/simple")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .env("PUFFIN_NO_WRAP", "1")
            .current_dir(&temp_dir), @r###"
        success: false
        exit_code: 1
        ----- stdout -----

        ----- stderr -----
          × No solution found when resolving dependencies:
          ╰─▶ Because only Python<3.10 is available and albatross==1.0.0 depends on Python>=3.10, we can conclude that albatross==1.0.0 cannot be used.
              And because root depends on albatross==1.0.0 we can conclude that the requirements are unsatisfiable.
        "###);
    });

    Ok(())
}
