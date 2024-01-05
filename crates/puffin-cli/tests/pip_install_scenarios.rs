#![cfg(all(feature = "python", feature = "pypi"))]

/// DO NOT EDIT
///
/// GENERATED WITH `./scripts/scenarios/update.py`
/// SCENARIOS FROM `https://github.com/zanieb/packse/tree/d899bfe2c3c33fcb9ba5eac0162236a8e8d8cbcf/scenarios`
use std::path::Path;
use std::process::Command;

use anyhow::Result;
use assert_cmd::assert::Assert;
use assert_cmd::prelude::*;
use insta_cmd::_macro_support::insta;
use insta_cmd::{assert_cmd_snapshot, get_cargo_bin};

use common::{create_venv, BIN_NAME, INSTA_FILTERS};

mod common;

fn assert_command(venv: &Path, command: &str, temp_dir: &Path) -> Assert {
    Command::new(venv.join("bin").join("python"))
        .arg("-c")
        .arg(command)
        .current_dir(temp_dir)
        .assert()
}

/// requires-package-only-prereleases
///
/// The user requires any version of package `a` which only has pre-release versions
/// available.
///
/// requires-package-only-prereleases-5829a64d
/// ├── environment
/// │   └── python3.7
/// ├── root
/// │   └── requires a
/// │       └── unsatisfied: no matching version
/// └── a
///     └── a-1.0.0a1
///         └── requires python>=3.7
#[test]
fn requires_package_only_prereleases() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"requires-package-only-prereleases-5829a64d-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-package-only-prereleases-5829a64d-a")
            .arg("--extra-index-url")
            .arg("https://test.pypi.org/simple")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .env("PUFFIN_NO_WRAP", "1")
            .current_dir(&temp_dir), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Resolved 1 package in [TIME]
        Downloaded 1 package in [TIME]
        Installed 1 package in [TIME]
         + a==1.0.0a1
        "###);
    });

    // Since there are only pre-release versions of `a` available, it should be
    // installed even though the user did not include a pre-release specifier.
    assert_command(
        &venv,
        "import requires_package_only_prereleases_5829a64d_a as package; print(package.__version__, end='')",
        &temp_dir
    )
    .success()
    .stdout("1.0.0a1");

    Ok(())
}

/// requires-package-only-prereleases-in-range
///
/// The user requires a version of package `a` which only matches pre-release
/// versions but they did not include a prerelease specifier.
///
/// requires-package-only-prereleases-in-range-2b0594c8
/// ├── environment
/// │   └── python3.7
/// ├── root
/// │   └── requires a>0.1.0
/// │       └── unsatisfied: no matching version
/// └── a
///     ├── a-0.1.0
///     │   └── requires python>=3.7
///     └── a-1.0.0a1
///         └── requires python>=3.7
#[test]
fn requires_package_only_prereleases_in_range() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"requires-package-only-prereleases-in-range-2b0594c8-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-package-only-prereleases-in-range-2b0594c8-a>0.1.0")
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
          ╰─▶ Because there is no version of a available matching >0.1.0 and root depends on a>0.1.0, version solving failed.
        "###);
    });

    // Since there are stable versions of `a` available, pre-release versions should
    // not be selected without explicit opt-in.
    assert_command(
        &venv,
        "import requires_package_only_prereleases_in_range_2b0594c8_a",
        &temp_dir,
    )
    .failure();

    Ok(())
}

/// requires-package-only-prereleases-in-range-global-opt-in
///
/// The user requires a version of package `a` which only matches pre-release
/// versions. They did not include a prerelease specifier for the package, but they
/// opted into pre-releases globally.
///
/// requires-package-only-prereleases-in-range-global-opt-in-51f94da2
/// ├── environment
/// │   └── python3.7
/// ├── root
/// │   └── requires a>0.1.0
/// │       └── unsatisfied: no matching version
/// └── a
///     ├── a-0.1.0
///     │   └── requires python>=3.7
///     └── a-1.0.0a1
///         └── requires python>=3.7
#[test]
fn requires_package_only_prereleases_in_range_global_opt_in() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((
        r"requires-package-only-prereleases-in-range-global-opt-in-51f94da2-",
        "",
    ));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-package-only-prereleases-in-range-global-opt-in-51f94da2-a>0.1.0")
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
          ╰─▶ Because there is no version of a available matching >0.1.0 and root depends on a>0.1.0, version solving failed.
        "###);
    });

    assert_command(
        &venv,
        "import requires_package_only_prereleases_in_range_global_opt_in_51f94da2_a as package; print(package.__version__, end='')",
        &temp_dir
    )
    .success()
    .stdout("1.0.0a1");

    Ok(())
}

/// requires-package-prerelease-and-final-any
///
/// The user requires any version of package `a` has a pre-release version available
/// and an older non-prerelease version.
///
/// requires-package-prerelease-and-final-any-66989e88
/// ├── environment
/// │   └── python3.7
/// ├── root
/// │   └── requires a
/// │       └── satisfied by a-0.1.0
/// └── a
///     ├── a-0.1.0
///     │   └── requires python>=3.7
///     └── a-1.0.0a1
///         └── requires python>=3.7
#[test]
fn requires_package_prerelease_and_final_any() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"requires-package-prerelease-and-final-any-66989e88-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-package-prerelease-and-final-any-66989e88-a")
            .arg("--extra-index-url")
            .arg("https://test.pypi.org/simple")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .env("PUFFIN_NO_WRAP", "1")
            .current_dir(&temp_dir), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Resolved 1 package in [TIME]
        Downloaded 1 package in [TIME]
        Installed 1 package in [TIME]
         + a==0.1.0
        "###);
    });

    // Since the user did not provide a pre-release specifier, the older stable version
    // should be selected.
    assert_command(
        &venv,
        "import requires_package_prerelease_and_final_any_66989e88_a as package; print(package.__version__, end='')",
        &temp_dir
    )
    .success()
    .stdout("0.1.0");

    Ok(())
}

/// requires-package-prerelease-specified-only-final-available
///
/// The user requires a version of `a` with a pre-release specifier and only stable
/// releases are available.
///
/// requires-package-prerelease-specified-only-final-available-8c3e26d4
/// ├── environment
/// │   └── python3.7
/// ├── root
/// │   └── requires a>=0.1.0a1
/// │       ├── satisfied by a-0.1.0
/// │       ├── satisfied by a-0.2.0
/// │       └── satisfied by a-0.3.0
/// └── a
///     ├── a-0.1.0
///     │   └── requires python>=3.7
///     ├── a-0.2.0
///     │   └── requires python>=3.7
///     └── a-0.3.0
///         └── requires python>=3.7
#[test]
fn requires_package_prerelease_specified_only_final_available() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((
        r"requires-package-prerelease-specified-only-final-available-8c3e26d4-",
        "",
    ));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-package-prerelease-specified-only-final-available-8c3e26d4-a>=0.1.0a1")
            .arg("--extra-index-url")
            .arg("https://test.pypi.org/simple")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .env("PUFFIN_NO_WRAP", "1")
            .current_dir(&temp_dir), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Resolved 1 package in [TIME]
        Downloaded 1 package in [TIME]
        Installed 1 package in [TIME]
         + a==0.3.0
        "###);
    });

    // The latest stable version should be selected.
    assert_command(
        &venv,
        "import requires_package_prerelease_specified_only_final_available_8c3e26d4_a as package; print(package.__version__, end='')",
        &temp_dir
    )
    .success()
    .stdout("0.3.0");

    Ok(())
}

/// requires-package-prerelease-specified-only-prerelease-available
///
/// The user requires a version of `a` with a pre-release specifier and only pre-
/// release releases are available.
///
/// requires-package-prerelease-specified-only-prerelease-available-fa8a64e0
/// ├── environment
/// │   └── python3.7
/// ├── root
/// │   └── requires a>=0.1.0a1
/// │       ├── satisfied by a-0.1.0a1
/// │       ├── satisfied by a-0.2.0a1
/// │       └── satisfied by a-0.3.0a1
/// └── a
///     ├── a-0.1.0a1
///     │   └── requires python>=3.7
///     ├── a-0.2.0a1
///     │   └── requires python>=3.7
///     └── a-0.3.0a1
///         └── requires python>=3.7
#[test]
fn requires_package_prerelease_specified_only_prerelease_available() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((
        r"requires-package-prerelease-specified-only-prerelease-available-fa8a64e0-",
        "",
    ));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-package-prerelease-specified-only-prerelease-available-fa8a64e0-a>=0.1.0a1")
            .arg("--extra-index-url")
            .arg("https://test.pypi.org/simple")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .env("PUFFIN_NO_WRAP", "1")
            .current_dir(&temp_dir), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Resolved 1 package in [TIME]
        Downloaded 1 package in [TIME]
        Installed 1 package in [TIME]
         + a==0.3.0a1
        "###);
    });

    // The latest pre-release version should be selected.
    assert_command(
        &venv,
        "import requires_package_prerelease_specified_only_prerelease_available_fa8a64e0_a as package; print(package.__version__, end='')",
        &temp_dir
    )
    .success()
    .stdout("0.3.0a1");

    Ok(())
}

/// requires-package-prerelease-specified-mixed-available
///
/// The user requires a version of `a` with a pre-release specifier and both pre-
/// release and stable releases are available.
///
/// requires-package-prerelease-specified-mixed-available-caf5dd1a
/// ├── environment
/// │   └── python3.7
/// ├── root
/// │   └── requires a>=0.1.0a1
/// │       ├── satisfied by a-0.1.0
/// │       ├── satisfied by a-0.2.0a1
/// │       ├── satisfied by a-0.3.0
/// │       └── satisfied by a-1.0.0a1
/// └── a
///     ├── a-0.1.0
///     │   └── requires python>=3.7
///     ├── a-0.2.0a1
///     │   └── requires python>=3.7
///     ├── a-0.3.0
///     │   └── requires python>=3.7
///     └── a-1.0.0a1
///         └── requires python>=3.7
#[test]
fn requires_package_prerelease_specified_mixed_available() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((
        r"requires-package-prerelease-specified-mixed-available-caf5dd1a-",
        "",
    ));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-package-prerelease-specified-mixed-available-caf5dd1a-a>=0.1.0a1")
            .arg("--extra-index-url")
            .arg("https://test.pypi.org/simple")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .env("PUFFIN_NO_WRAP", "1")
            .current_dir(&temp_dir), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Resolved 1 package in [TIME]
        Downloaded 1 package in [TIME]
        Installed 1 package in [TIME]
         + a==1.0.0a1
        "###);
    });

    // Since the user provided a pre-release specifier, the latest pre-release version
    // should be selected.
    assert_command(
        &venv,
        "import requires_package_prerelease_specified_mixed_available_caf5dd1a_a as package; print(package.__version__, end='')",
        &temp_dir
    )
    .success()
    .stdout("1.0.0a1");

    Ok(())
}

/// requires-package-multiple-prereleases-kinds
///
/// The user requires `a` which has multiple prereleases available with different
/// labels.
///
/// requires-package-multiple-prereleases-kinds-08c2f99b
/// ├── environment
/// │   └── python3.7
/// ├── root
/// │   └── requires a>=1.0.0a1
/// │       ├── satisfied by a-1.0.0a1
/// │       ├── satisfied by a-1.0.0b1
/// │       └── satisfied by a-1.0.0rc1
/// └── a
///     ├── a-1.0.0a1
///     │   └── requires python>=3.7
///     ├── a-1.0.0b1
///     │   └── requires python>=3.7
///     └── a-1.0.0rc1
///         └── requires python>=3.7
#[test]
fn requires_package_multiple_prereleases_kinds() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"requires-package-multiple-prereleases-kinds-08c2f99b-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-package-multiple-prereleases-kinds-08c2f99b-a>=1.0.0a1")
            .arg("--extra-index-url")
            .arg("https://test.pypi.org/simple")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .env("PUFFIN_NO_WRAP", "1")
            .current_dir(&temp_dir), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Resolved 1 package in [TIME]
        Downloaded 1 package in [TIME]
        Installed 1 package in [TIME]
         + a==1.0.0rc1
        "###);
    });

    // Release candidates should be the highest precedence pre-release kind.
    assert_command(
        &venv,
        "import requires_package_multiple_prereleases_kinds_08c2f99b_a as package; print(package.__version__, end='')",
        &temp_dir
    )
    .success()
    .stdout("1.0.0rc1");

    Ok(())
}

/// requires-package-multiple-prereleases-numbers
///
/// The user requires `a` which has multiple alphas available.
///
/// requires-package-multiple-prereleases-numbers-4cf7acef
/// ├── environment
/// │   └── python3.7
/// ├── root
/// │   └── requires a>=1.0.0a1
/// │       ├── satisfied by a-1.0.0a1
/// │       ├── satisfied by a-1.0.0a2
/// │       └── satisfied by a-1.0.0a3
/// └── a
///     ├── a-1.0.0a1
///     │   └── requires python>=3.7
///     ├── a-1.0.0a2
///     │   └── requires python>=3.7
///     └── a-1.0.0a3
///         └── requires python>=3.7
#[test]
fn requires_package_multiple_prereleases_numbers() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((
        r"requires-package-multiple-prereleases-numbers-4cf7acef-",
        "",
    ));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-package-multiple-prereleases-numbers-4cf7acef-a>=1.0.0a1")
            .arg("--extra-index-url")
            .arg("https://test.pypi.org/simple")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .env("PUFFIN_NO_WRAP", "1")
            .current_dir(&temp_dir), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Resolved 1 package in [TIME]
        Downloaded 1 package in [TIME]
        Installed 1 package in [TIME]
         + a==1.0.0a3
        "###);
    });

    // The latest alpha version should be selected.
    assert_command(
        &venv,
        "import requires_package_multiple_prereleases_numbers_4cf7acef_a as package; print(package.__version__, end='')",
        &temp_dir
    )
    .success()
    .stdout("1.0.0a3");

    Ok(())
}

/// requires-transitive-package-only-prereleases
///
/// The user requires any version of package `a` which requires `b` which only has
/// pre-release versions available.
///
/// requires-transitive-package-only-prereleases-fa02005e
/// ├── environment
/// │   └── python3.7
/// ├── root
/// │   └── requires a
/// │       └── satisfied by a-0.1.0
/// ├── a
/// │   └── a-0.1.0
/// │       ├── requires b
/// │       │   └── unsatisfied: no matching version
/// │       └── requires python>=3.7
/// └── b
///     └── b-1.0.0a1
///         └── requires python>=3.7
#[test]
fn requires_transitive_package_only_prereleases() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((
        r"requires-transitive-package-only-prereleases-fa02005e-",
        "",
    ));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-transitive-package-only-prereleases-fa02005e-a")
            .arg("--extra-index-url")
            .arg("https://test.pypi.org/simple")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .env("PUFFIN_NO_WRAP", "1")
            .current_dir(&temp_dir), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Resolved 2 packages in [TIME]
        Downloaded 2 packages in [TIME]
        Installed 2 packages in [TIME]
         + a==0.1.0
         + b==1.0.0a1
        "###);
    });

    // Since there are only pre-release versions of `b` available, it should be
    // selected even though the user did not opt-in to pre-releases.
    assert_command(
        &venv,
        "import requires_transitive_package_only_prereleases_fa02005e_a as package; print(package.__version__, end='')",
        &temp_dir
    )
    .success()
    .stdout("0.1.0");
    assert_command(
        &venv,
        "import requires_transitive_package_only_prereleases_fa02005e_b as package; print(package.__version__, end='')",
        &temp_dir
    )
    .success()
    .stdout("1.0.0a1");

    Ok(())
}

/// requires-transitive-package-only-prereleases-in-range
///
/// The user requires package `a` which has a dependency on a package which only
/// matches pre-release versions but they did not include a pre-release specifier.
///
/// requires-transitive-package-only-prereleases-in-range-4800779d
/// ├── environment
/// │   └── python3.7
/// ├── root
/// │   └── requires a
/// │       └── satisfied by a-0.1.0
/// ├── a
/// │   └── a-0.1.0
/// │       ├── requires b>0.1
/// │       │   └── unsatisfied: no matching version
/// │       └── requires python>=3.7
/// └── b
///     ├── b-0.1.0
///     │   └── requires python>=3.7
///     └── b-1.0.0a1
///         └── requires python>=3.7
#[test]
fn requires_transitive_package_only_prereleases_in_range() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((
        r"requires-transitive-package-only-prereleases-in-range-4800779d-",
        "",
    ));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-transitive-package-only-prereleases-in-range-4800779d-a")
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
          ╰─▶ Because there is no version of b available matching >0.1 and a==0.1.0 depends on b>0.1, a==0.1.0 is forbidden.
              And because there is no version of a available matching <0.1.0 | >0.1.0 and root depends on a, version solving failed.
        "###);
    });

    // Since there are stable versions of `b` available, the pre-release version should
    // not be selected without explicit opt-in. The available version is excluded by
    // the range requested by the user.
    assert_command(
        &venv,
        "import requires_transitive_package_only_prereleases_in_range_4800779d_a",
        &temp_dir,
    )
    .failure();

    Ok(())
}

/// requires-transitive-package-only-prereleases-in-range-opt-in
///
/// The user requires package `a` which has a dependency on a package which only
/// matches pre-release versions; the user has opted into allowing prereleases in
/// `b` explicitly.
///
/// requires-transitive-package-only-prereleases-in-range-opt-in-4ca10c42
/// ├── environment
/// │   └── python3.7
/// ├── root
/// │   ├── requires a
/// │   │   └── satisfied by a-0.1.0
/// │   └── requires b>0.0.0a1
/// │       └── satisfied by b-0.1.0
/// ├── a
/// │   └── a-0.1.0
/// │       ├── requires b>0.1
/// │       │   └── unsatisfied: no matching version
/// │       └── requires python>=3.7
/// └── b
///     ├── b-0.1.0
///     │   └── requires python>=3.7
///     └── b-1.0.0a1
///         └── requires python>=3.7
#[test]
fn requires_transitive_package_only_prereleases_in_range_opt_in() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((
        r"requires-transitive-package-only-prereleases-in-range-opt-in-4ca10c42-",
        "",
    ));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-transitive-package-only-prereleases-in-range-opt-in-4ca10c42-a")
            .arg("requires-transitive-package-only-prereleases-in-range-opt-in-4ca10c42-b>0.0.0a1")
            .arg("--extra-index-url")
            .arg("https://test.pypi.org/simple")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .env("PUFFIN_NO_WRAP", "1")
            .current_dir(&temp_dir), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Resolved 2 packages in [TIME]
        Downloaded 2 packages in [TIME]
        Installed 2 packages in [TIME]
         + a==0.1.0
         + b==1.0.0a1
        "###);
    });

    // Since the user included a dependency on `b` with a pre-release specifier, a pre-
    // release version can be selected.
    assert_command(
        &venv,
        "import requires_transitive_package_only_prereleases_in_range_opt_in_4ca10c42_a as package; print(package.__version__, end='')",
        &temp_dir
    )
    .success()
    .stdout("0.1.0");
    assert_command(
        &venv,
        "import requires_transitive_package_only_prereleases_in_range_opt_in_4ca10c42_b as package; print(package.__version__, end='')",
        &temp_dir
    )
    .success()
    .stdout("1.0.0a1");

    Ok(())
}

/// requires-transitive-prerelease-and-stable-dependency
///
/// A transitive dependency has both a prerelease and a stable selector, but can
/// only be satisfied by a prerelease
///
/// requires-transitive-prerelease-and-stable-dependency-31b546ef
/// ├── environment
/// │   └── python3.7
/// ├── root
/// │   ├── requires a
/// │   │   └── satisfied by a-1.0.0
/// │   └── requires b
/// │       └── satisfied by b-1.0.0
/// ├── a
/// │   └── a-1.0.0
/// │       ├── requires c==2.0.0b1
/// │       │   └── satisfied by c-2.0.0b1
/// │       └── requires python>=3.7
/// ├── b
/// │   └── b-1.0.0
/// │       ├── requires c>=1.0.0,<=3.0.0
/// │       │   └── satisfied by c-1.0.0
/// │       └── requires python>=3.7
/// └── c
///     ├── c-1.0.0
///     │   └── requires python>=3.7
///     └── c-2.0.0b1
///         └── requires python>=3.7
#[test]
fn requires_transitive_prerelease_and_stable_dependency() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((
        r"requires-transitive-prerelease-and-stable-dependency-31b546ef-",
        "",
    ));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-transitive-prerelease-and-stable-dependency-31b546ef-a")
            .arg("requires-transitive-prerelease-and-stable-dependency-31b546ef-b")
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
          ╰─▶ Because there is no version of c available matching ==2.0.0b1 and a==1.0.0 depends on c==2.0.0b1, a==1.0.0 is forbidden.
              And because there is no version of a available matching <1.0.0 | >1.0.0 and root depends on a, version solving failed.

              hint: c was requested with a pre-release marker (e.g., ==2.0.0b1), but pre-releases weren't enabled (try: `--prerelease=allow`)
        "###);
    });

    // Since the user did not explicitly opt-in to a prerelease, it cannot be selected.
    assert_command(
        &venv,
        "import requires_transitive_prerelease_and_stable_dependency_31b546ef_a",
        &temp_dir,
    )
    .failure();
    assert_command(
        &venv,
        "import requires_transitive_prerelease_and_stable_dependency_31b546ef_b",
        &temp_dir,
    )
    .failure();

    Ok(())
}

/// requires-transitive-prerelease-and-stable-dependency-opt-in
///
/// A transitive dependency has both a prerelease and a stable selector, but can
/// only be satisfied by a prerelease. The user includes an opt-in to prereleases of
/// the transitive dependency.
///
/// requires-transitive-prerelease-and-stable-dependency-opt-in-dd00a87f
/// ├── environment
/// │   └── python3.7
/// ├── root
/// │   ├── requires a
/// │   │   └── satisfied by a-1.0.0
/// │   ├── requires b
/// │   │   └── satisfied by b-1.0.0
/// │   └── requires c>=0.0.0a1
/// │       ├── satisfied by c-1.0.0
/// │       └── satisfied by c-2.0.0b1
/// ├── a
/// │   └── a-1.0.0
/// │       ├── requires c==2.0.0b1
/// │       │   └── satisfied by c-2.0.0b1
/// │       └── requires python>=3.7
/// ├── b
/// │   └── b-1.0.0
/// │       ├── requires c>=1.0.0,<=3.0.0
/// │       │   └── satisfied by c-1.0.0
/// │       └── requires python>=3.7
/// └── c
///     ├── c-1.0.0
///     │   └── requires python>=3.7
///     └── c-2.0.0b1
///         └── requires python>=3.7
#[test]
fn requires_transitive_prerelease_and_stable_dependency_opt_in() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((
        r"requires-transitive-prerelease-and-stable-dependency-opt-in-dd00a87f-",
        "",
    ));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-transitive-prerelease-and-stable-dependency-opt-in-dd00a87f-a")
            .arg("requires-transitive-prerelease-and-stable-dependency-opt-in-dd00a87f-b")
            .arg("requires-transitive-prerelease-and-stable-dependency-opt-in-dd00a87f-c>=0.0.0a1")
            .arg("--extra-index-url")
            .arg("https://test.pypi.org/simple")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .env("PUFFIN_NO_WRAP", "1")
            .current_dir(&temp_dir), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Resolved 3 packages in [TIME]
        Downloaded 3 packages in [TIME]
        Installed 3 packages in [TIME]
         + a==1.0.0
         + b==1.0.0
         + c==2.0.0b1
        "###);
    });

    // Since the user explicitly opted-in to a prerelease for `c`, it can be installed.
    assert_command(
        &venv,
        "import requires_transitive_prerelease_and_stable_dependency_opt_in_dd00a87f_a as package; print(package.__version__, end='')",
        &temp_dir
    )
    .success()
    .stdout("1.0.0");
    assert_command(
        &venv,
        "import requires_transitive_prerelease_and_stable_dependency_opt_in_dd00a87f_b as package; print(package.__version__, end='')",
        &temp_dir
    )
    .success()
    .stdout("1.0.0");
    assert_command(
        &venv,
        "import requires_transitive_prerelease_and_stable_dependency_opt_in_dd00a87f_c as package; print(package.__version__, end='')",
        &temp_dir
    )
    .success()
    .stdout("2.0.0b1");

    Ok(())
}

/// requires-package-does-not-exist
///
/// The user requires any version of package `a` which does not exist.
///
/// requires-package-does-not-exist-57cd4136
/// ├── environment
/// │   └── python3.7
/// └── root
///     └── requires a
///         └── unsatisfied: no versions for package
#[test]
fn requires_package_does_not_exist() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"requires-package-does-not-exist-57cd4136-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-package-does-not-exist-57cd4136-a")
            .arg("--extra-index-url")
            .arg("https://test.pypi.org/simple")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .env("PUFFIN_NO_WRAP", "1")
            .current_dir(&temp_dir), @r###"
        success: false
        exit_code: 2
        ----- stdout -----

        ----- stderr -----
        error: Package `a` was not found in the registry.
        "###);
    });

    assert_command(
        &venv,
        "import requires_package_does_not_exist_57cd4136_a",
        &temp_dir,
    )
    .failure();

    Ok(())
}

/// requires-exact-version-does-not-exist
///
/// The user requires an exact version of package `a` but only other versions exist
///
/// requires-exact-version-does-not-exist-eaa03067
/// ├── environment
/// │   └── python3.7
/// ├── root
/// │   └── requires a==2.0.0
/// │       └── unsatisfied: no matching version
/// └── a
///     └── a-1.0.0
///         └── requires python>=3.7
#[test]
fn requires_exact_version_does_not_exist() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"requires-exact-version-does-not-exist-eaa03067-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-exact-version-does-not-exist-eaa03067-a==2.0.0")
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
          ╰─▶ Because there is no version of a available matching ==2.0.0 and root depends on a==2.0.0, version solving failed.
        "###);
    });

    assert_command(
        &venv,
        "import requires_exact_version_does_not_exist_eaa03067_a",
        &temp_dir,
    )
    .failure();

    Ok(())
}

/// requires-greater-version-does-not-exist
///
/// The user requires a version of `a` greater than `1.0.0` but only smaller or
/// equal versions exist
///
/// requires-greater-version-does-not-exist-6e8e01df
/// ├── environment
/// │   └── python3.7
/// ├── root
/// │   └── requires a>1.0.0
/// │       └── unsatisfied: no matching version
/// └── a
///     ├── a-0.1.0
///     │   └── requires python>=3.7
///     └── a-1.0.0
///         └── requires python>=3.7
#[test]
fn requires_greater_version_does_not_exist() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"requires-greater-version-does-not-exist-6e8e01df-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-greater-version-does-not-exist-6e8e01df-a>1.0.0")
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
          ╰─▶ Because there is no version of a available matching >1.0.0 and root depends on a>1.0.0, version solving failed.
        "###);
    });

    assert_command(
        &venv,
        "import requires_greater_version_does_not_exist_6e8e01df_a",
        &temp_dir,
    )
    .failure();

    Ok(())
}

/// requires-less-version-does-not-exist
///
/// The user requires a version of `a` less than `1.0.0` but only larger versions
/// exist
///
/// requires-less-version-does-not-exist-e45cec3c
/// ├── environment
/// │   └── python3.7
/// ├── root
/// │   └── requires a<2.0.0
/// │       └── unsatisfied: no matching version
/// └── a
///     ├── a-2.0.0
///     │   └── requires python>=3.7
///     ├── a-3.0.0
///     │   └── requires python>=3.7
///     └── a-4.0.0
///         └── requires python>=3.7
#[test]
fn requires_less_version_does_not_exist() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"requires-less-version-does-not-exist-e45cec3c-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-less-version-does-not-exist-e45cec3c-a<2.0.0")
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
          ╰─▶ Because there is no version of a available matching <2.0.0 and root depends on a<2.0.0, version solving failed.
        "###);
    });

    assert_command(
        &venv,
        "import requires_less_version_does_not_exist_e45cec3c_a",
        &temp_dir,
    )
    .failure();

    Ok(())
}

/// transitive-requires-package-does-not-exist
///
/// The user requires package `a` but `a` requires package `b` which does not exist
///
/// transitive-requires-package-does-not-exist-aca2796a
/// ├── environment
/// │   └── python3.7
/// ├── root
/// │   └── requires a
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
///         ├── requires b
///             └── unsatisfied: no versions for package
///         └── requires python>=3.7
#[test]
fn transitive_requires_package_does_not_exist() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"transitive-requires-package-does-not-exist-aca2796a-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("transitive-requires-package-does-not-exist-aca2796a-a")
            .arg("--extra-index-url")
            .arg("https://test.pypi.org/simple")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .env("PUFFIN_NO_WRAP", "1")
            .current_dir(&temp_dir), @r###"
        success: false
        exit_code: 2
        ----- stdout -----

        ----- stderr -----
        error: Package `b` was not found in the registry.
        "###);
    });

    assert_command(
        &venv,
        "import transitive_requires_package_does_not_exist_aca2796a_a",
        &temp_dir,
    )
    .failure();

    Ok(())
}

/// requires-direct-incompatible-versions
///
/// The user requires two incompatible, existing versions of package `a`
///
/// requires-direct-incompatible-versions-063ec9d3
/// ├── environment
/// │   └── python3.7
/// ├── root
/// │   ├── requires a==1.0.0
/// │   │   └── satisfied by a-1.0.0
/// │   └── requires a==2.0.0
/// │       └── satisfied by a-2.0.0
/// └── a
///     ├── a-1.0.0
///     │   └── requires python>=3.7
///     └── a-2.0.0
///         └── requires python>=3.7
#[test]
fn requires_direct_incompatible_versions() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"requires-direct-incompatible-versions-063ec9d3-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-direct-incompatible-versions-063ec9d3-a==1.0.0")
            .arg("requires-direct-incompatible-versions-063ec9d3-a==2.0.0")
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
          ╰─▶ root dependencies are unusable: Conflicting versions for `a`: `a==1.0.0` does not intersect with `a==2.0.0`
        "###);
    });

    assert_command(
        &venv,
        "import requires_direct_incompatible_versions_063ec9d3_a",
        &temp_dir,
    )
    .failure();
    assert_command(
        &venv,
        "import requires_direct_incompatible_versions_063ec9d3_a",
        &temp_dir,
    )
    .failure();

    Ok(())
}

/// requires-transitive-incompatible-with-root-version
///
/// The user requires packages `a` and `b` but `a` requires a different version of
/// `b`
///
/// requires-transitive-incompatible-with-root-version-638350f3
/// ├── environment
/// │   └── python3.7
/// ├── root
/// │   ├── requires a
/// │   │   └── satisfied by a-1.0.0
/// │   └── requires b==1.0.0
/// │       └── satisfied by b-1.0.0
/// ├── a
/// │   └── a-1.0.0
/// │       ├── requires b==2.0.0
/// │       │   └── satisfied by b-2.0.0
/// │       └── requires python>=3.7
/// └── b
///     ├── b-1.0.0
///     │   └── requires python>=3.7
///     └── b-2.0.0
///         └── requires python>=3.7
#[test]
fn requires_transitive_incompatible_with_root_version() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((
        r"requires-transitive-incompatible-with-root-version-638350f3-",
        "",
    ));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-transitive-incompatible-with-root-version-638350f3-a")
            .arg("requires-transitive-incompatible-with-root-version-638350f3-b==1.0.0")
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
          ╰─▶ Because there is no version of a available matching <1.0.0 | >1.0.0 and a==1.0.0 depends on b==2.0.0, a depends on b==2.0.0.
              And because root depends on a and root depends on b==1.0.0, version solving failed.
        "###);
    });

    assert_command(
        &venv,
        "import requires_transitive_incompatible_with_root_version_638350f3_a",
        &temp_dir,
    )
    .failure();
    assert_command(
        &venv,
        "import requires_transitive_incompatible_with_root_version_638350f3_b",
        &temp_dir,
    )
    .failure();

    Ok(())
}

/// requires-transitive-incompatible-with-transitive
///
/// The user requires package `a` and `b`; `a` and `b` require different versions of
/// `c`
///
/// requires-transitive-incompatible-with-transitive-9b595175
/// ├── environment
/// │   └── python3.7
/// ├── root
/// │   ├── requires a
/// │   │   └── satisfied by a-1.0.0
/// │   └── requires b
/// │       └── satisfied by b-1.0.0
/// ├── a
/// │   └── a-1.0.0
/// │       ├── requires c==1.0.0
/// │       │   └── satisfied by c-1.0.0
/// │       └── requires python>=3.7
/// ├── b
/// │   └── b-1.0.0
/// │       ├── requires c==2.0.0
/// │       │   └── satisfied by c-2.0.0
/// │       └── requires python>=3.7
/// └── c
///     ├── c-1.0.0
///     │   └── requires python>=3.7
///     └── c-2.0.0
///         └── requires python>=3.7
#[test]
fn requires_transitive_incompatible_with_transitive() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((
        r"requires-transitive-incompatible-with-transitive-9b595175-",
        "",
    ));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-transitive-incompatible-with-transitive-9b595175-a")
            .arg("requires-transitive-incompatible-with-transitive-9b595175-b")
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
          ╰─▶ Because there is no version of b available matching <1.0.0 | >1.0.0 and b==1.0.0 depends on c==2.0.0, b depends on c==2.0.0.
              And because a==1.0.0 depends on c==1.0.0 and there is no version of a available matching <1.0.0 | >1.0.0, a *, b * are incompatible.
              And because root depends on b and root depends on a, version solving failed.
        "###);
    });

    assert_command(
        &venv,
        "import requires_transitive_incompatible_with_transitive_9b595175_a",
        &temp_dir,
    )
    .failure();
    assert_command(
        &venv,
        "import requires_transitive_incompatible_with_transitive_9b595175_b",
        &temp_dir,
    )
    .failure();

    Ok(())
}

/// requires-python-version-does-not-exist
///
/// The user requires a package which requires a Python version that does not exist
///
/// requires-python-version-does-not-exist-0825b69c
/// ├── environment
/// │   └── python3.7
/// ├── root
/// │   └── requires a==1.0.0
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
///         └── requires python>=4.0 (incompatible with environment)
#[test]
fn requires_python_version_does_not_exist() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.7");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"requires-python-version-does-not-exist-0825b69c-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-python-version-does-not-exist-0825b69c-a==1.0.0")
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
          ╰─▶ Because there is no version of Python available matching >=4.0 and a==1.0.0 depends on Python>=4.0, a==1.0.0 is forbidden.
              And because root depends on a==1.0.0, version solving failed.
        "###);
    });

    assert_command(
        &venv,
        "import requires_python_version_does_not_exist_0825b69c_a",
        &temp_dir,
    )
    .failure();

    Ok(())
}

/// requires-python-version-less-than-current
///
/// The user requires a package which requires a Python version less than the
/// current version
///
/// requires-python-version-less-than-current-f9296b84
/// ├── environment
/// │   └── python3.9
/// ├── root
/// │   └── requires a==1.0.0
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
///         └── requires python<=3.8 (incompatible with environment)
#[test]
fn requires_python_version_less_than_current() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.9");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"requires-python-version-less-than-current-f9296b84-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-python-version-less-than-current-f9296b84-a==1.0.0")
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
          ╰─▶ Because there is no version of Python available matching <=3.8 and a==1.0.0 depends on Python<=3.8, a==1.0.0 is forbidden.
              And because root depends on a==1.0.0, version solving failed.
        "###);
    });

    assert_command(
        &venv,
        "import requires_python_version_less_than_current_f9296b84_a",
        &temp_dir,
    )
    .failure();

    Ok(())
}

/// requires-python-version-greater-than-current
///
/// The user requires a package which requires a Python version greater than the
/// current version
///
/// requires-python-version-greater-than-current-a11d5394
/// ├── environment
/// │   └── python3.9
/// ├── root
/// │   └── requires a==1.0.0
/// │       └── satisfied by a-1.0.0
/// └── a
///     └── a-1.0.0
///         └── requires python>=3.10 (incompatible with environment)
#[test]
fn requires_python_version_greater_than_current() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.9");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((
        r"requires-python-version-greater-than-current-a11d5394-",
        "",
    ));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-python-version-greater-than-current-a11d5394-a==1.0.0")
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
          ╰─▶ Because there is no version of Python available matching >=3.10 and a==1.0.0 depends on Python>=3.10, a==1.0.0 is forbidden.
              And because root depends on a==1.0.0, version solving failed.
        "###);
    });

    assert_command(
        &venv,
        "import requires_python_version_greater_than_current_a11d5394_a",
        &temp_dir,
    )
    .failure();

    Ok(())
}

/// requires-python-version-greater-than-current-many
///
/// The user requires a package which has many versions which all require a Python
/// version greater than the current version
///
/// requires-python-version-greater-than-current-many-02dc550c
/// ├── environment
/// │   └── python3.9
/// ├── root
/// │   └── requires a==1.0.0
/// │       └── unsatisfied: no matching version
/// └── a
///     ├── a-2.0.0
///     │   └── requires python>=3.10 (incompatible with environment)
///     ├── a-2.1.0
///     │   └── requires python>=3.10 (incompatible with environment)
///     ├── a-2.2.0
///     │   └── requires python>=3.10 (incompatible with environment)
///     ├── a-2.3.0
///     │   └── requires python>=3.10 (incompatible with environment)
///     ├── a-2.4.0
///     │   └── requires python>=3.10 (incompatible with environment)
///     ├── a-2.5.0
///     │   └── requires python>=3.10 (incompatible with environment)
///     ├── a-3.0.0
///     │   └── requires python>=3.11 (incompatible with environment)
///     ├── a-3.1.0
///     │   └── requires python>=3.11 (incompatible with environment)
///     ├── a-3.2.0
///     │   └── requires python>=3.11 (incompatible with environment)
///     ├── a-3.3.0
///     │   └── requires python>=3.11 (incompatible with environment)
///     ├── a-3.4.0
///     │   └── requires python>=3.11 (incompatible with environment)
///     └── a-3.5.0
///         └── requires python>=3.11 (incompatible with environment)
#[test]
fn requires_python_version_greater_than_current_many() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.9");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((
        r"requires-python-version-greater-than-current-many-02dc550c-",
        "",
    ));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-python-version-greater-than-current-many-02dc550c-a==1.0.0")
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
          ╰─▶ Because there is no version of a available matching ==1.0.0 and root depends on a==1.0.0, version solving failed.
        "###);
    });

    assert_command(
        &venv,
        "import requires_python_version_greater_than_current_many_02dc550c_a",
        &temp_dir,
    )
    .failure();

    Ok(())
}

/// requires-python-version-greater-than-current-backtrack
///
/// The user requires a package where recent versions require a Python version
/// greater than the current version, but an older version is compatible.
///
/// requires-python-version-greater-than-current-backtrack-ef060cef
/// ├── environment
/// │   └── python3.9
/// ├── root
/// │   └── requires a
/// │       ├── satisfied by a-1.0.0
/// │       ├── satisfied by a-2.0.0
/// │       ├── satisfied by a-3.0.0
/// │       └── satisfied by a-4.0.0
/// └── a
///     ├── a-1.0.0
///     │   └── requires python>=3.9
///     ├── a-2.0.0
///     │   └── requires python>=3.10 (incompatible with environment)
///     ├── a-3.0.0
///     │   └── requires python>=3.11 (incompatible with environment)
///     └── a-4.0.0
///         └── requires python>=3.12 (incompatible with environment)
#[test]
fn requires_python_version_greater_than_current_backtrack() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.9");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((
        r"requires-python-version-greater-than-current-backtrack-ef060cef-",
        "",
    ));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-python-version-greater-than-current-backtrack-ef060cef-a")
            .arg("--extra-index-url")
            .arg("https://test.pypi.org/simple")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .env("PUFFIN_NO_WRAP", "1")
            .current_dir(&temp_dir), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Resolved 1 package in [TIME]
        Downloaded 1 package in [TIME]
        Installed 1 package in [TIME]
         + a==1.0.0
        "###);
    });

    assert_command(
        &venv,
        "import requires_python_version_greater_than_current_backtrack_ef060cef_a as package; print(package.__version__, end='')",
        &temp_dir
    )
    .success()
    .stdout("1.0.0");

    Ok(())
}

/// requires-python-version-greater-than-current-excluded
///
/// The user requires a package where recent versions require a Python version
/// greater than the current version, but an excluded older version is compatible.
///
/// requires-python-version-greater-than-current-excluded-1bde0c18
/// ├── environment
/// │   └── python3.9
/// ├── root
/// │   └── requires a>=2.0.0
/// │       ├── satisfied by a-2.0.0
/// │       ├── satisfied by a-3.0.0
/// │       └── satisfied by a-4.0.0
/// └── a
///     ├── a-1.0.0
///     │   └── requires python>=3.9
///     ├── a-2.0.0
///     │   └── requires python>=3.10 (incompatible with environment)
///     ├── a-3.0.0
///     │   └── requires python>=3.11 (incompatible with environment)
///     └── a-4.0.0
///         └── requires python>=3.12 (incompatible with environment)
#[test]
fn requires_python_version_greater_than_current_excluded() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "python3.9");

    // In addition to the standard filters, remove the scenario prefix
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((
        r"requires-python-version-greater-than-current-excluded-1bde0c18-",
        "",
    ));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-python-version-greater-than-current-excluded-1bde0c18-a>=2.0.0")
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
          ╰─▶ Because there is no version of Python available matching >=3.10, <3.11 and there is no version of Python available matching >=3.12, Python >=3.10, <3.11 | >=3.12 are incompatible.
              And because there is no version of Python available matching >=3.11, <3.12, Python >=3.10 are incompatible.
              And because a==2.0.0 depends on Python>=3.10 and there is no version of a available matching >2.0.0, <3.0.0 | >3.0.0, <4.0.0 | >4.0.0, a>=2.0.0, <3.0.0 is forbidden. (1)

              Because there is no version of Python available matching >=3.11, <3.12 and there is no version of Python available matching >=3.12, Python >=3.11 are incompatible.
              And because a==3.0.0 depends on Python>=3.11, a==3.0.0 is forbidden.
              And because a>=2.0.0, <3.0.0 is forbidden (1), a>=2.0.0, <4.0.0 is forbidden. (2)

              Because there is no version of Python available matching >=3.12 and a==4.0.0 depends on Python>=3.12, a==4.0.0 is forbidden.
              And because a>=2.0.0, <4.0.0 is forbidden (2), a>=2.0.0 is forbidden.
              And because root depends on a>=2.0.0, version solving failed.
        "###);
    });

    assert_command(
        &venv,
        "import requires_python_version_greater_than_current_excluded_1bde0c18_a",
        &temp_dir,
    )
    .failure();

    Ok(())
}
