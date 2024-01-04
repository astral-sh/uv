#![cfg(all(feature = "python", feature = "pypi"))]

/// DO NOT EDIT
///
/// GENERATED WITH `./scripts/scenarios/update.py`
/// SCENARIOS FROM `https://github.com/zanieb/packse/tree/4ffd4ee25eb89fe078de15572bd609cf359a1997/scenarios`
use std::process::Command;

use anyhow::Result;
use insta_cmd::_macro_support::insta;
use insta_cmd::{assert_cmd_snapshot, get_cargo_bin};

use common::{create_venv, BIN_NAME, INSTA_FILTERS};

mod common;

/// requires-package-does-not-exist
///
/// The user requires any version of package `a` which does not exist.
///
/// requires-package-does-not-exist-bc7df012
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
    filters.push((r"requires-package-does-not-exist-bc7df012-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-package-does-not-exist-bc7df012-a")
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

    Ok(())
}

/// requires-exact-version-does-not-exist
///
/// The user requires an exact version of package `a` but only other versions exist
///
/// requires-exact-version-does-not-exist-c275ce96
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
    filters.push((r"requires-exact-version-does-not-exist-c275ce96-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-exact-version-does-not-exist-c275ce96-a==2.0.0")
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

    Ok(())
}

/// requires-greater-version-does-not-exist
///
/// The user requires a version of `a` greater than `1.0.0` but only smaller or
/// equal versions exist
///
/// requires-greater-version-does-not-exist-d34821ba
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
    filters.push((r"requires-greater-version-does-not-exist-d34821ba-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-greater-version-does-not-exist-d34821ba-a>1.0.0")
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

    Ok(())
}

/// requires-less-version-does-not-exist
///
/// The user requires a version of `a` less than `1.0.0` but only larger versions
/// exist
///
/// requires-less-version-does-not-exist-4088ec1b
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
    filters.push((r"requires-less-version-does-not-exist-4088ec1b-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-less-version-does-not-exist-4088ec1b-a<2.0.0")
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

    Ok(())
}

/// transitive-requires-package-does-not-exist
///
/// The user requires package `a` but `a` requires package `b` which does not exist
///
/// transitive-requires-package-does-not-exist-63ca5a54
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
    filters.push((r"transitive-requires-package-does-not-exist-63ca5a54-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("transitive-requires-package-does-not-exist-63ca5a54-a")
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

    Ok(())
}

/// requires-direct-incompatible-versions
///
/// The user requires two incompatible, existing versions of package `a`
///
/// requires-direct-incompatible-versions-1432ee4c
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
    filters.push((r"requires-direct-incompatible-versions-1432ee4c-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-direct-incompatible-versions-1432ee4c-a==1.0.0")
            .arg("requires-direct-incompatible-versions-1432ee4c-a==2.0.0")
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

    Ok(())
}

/// requires-transitive-incompatible-with-root-version
///
/// The user requires packages `a` and `b` but `a` requires a different version of
/// `b`
///
/// requires-transitive-incompatible-with-root-version-b3c83bbd
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
        r"requires-transitive-incompatible-with-root-version-b3c83bbd-",
        "",
    ));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-transitive-incompatible-with-root-version-b3c83bbd-a")
            .arg("requires-transitive-incompatible-with-root-version-b3c83bbd-b==1.0.0")
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

    Ok(())
}

/// requires-transitive-incompatible-with-transitive
///
/// The user requires package `a` and `b`; `a` and `b` require different versions of
/// `c`
///
/// requires-transitive-incompatible-with-transitive-a35362d1
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
        r"requires-transitive-incompatible-with-transitive-a35362d1-",
        "",
    ));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-transitive-incompatible-with-transitive-a35362d1-a")
            .arg("requires-transitive-incompatible-with-transitive-a35362d1-b")
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
          ╰─▶ Because there is no version of a available matching <1.0.0 | >1.0.0 and a==1.0.0 depends on c==1.0.0, a depends on c==1.0.0.
              And because b==1.0.0 depends on c==2.0.0 and there is no version of b available matching <1.0.0 | >1.0.0, a *, b * are incompatible.
              And because root depends on a and root depends on b, version solving failed.
        "###);
    });

    Ok(())
}

/// requires-python-version-does-not-exist
///
/// The user requires a package which requires a Python version that does not exist
///
/// requires-python-version-does-not-exist-d1fc625b
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
    filters.push((r"requires-python-version-does-not-exist-d1fc625b-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-python-version-does-not-exist-d1fc625b-a==1.0.0")
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

    Ok(())
}

/// requires-python-version-less-than-current
///
/// The user requires a package which requires a Python version less than the
/// current version
///
/// requires-python-version-less-than-current-48bada28
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
    filters.push((r"requires-python-version-less-than-current-48bada28-", ""));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-python-version-less-than-current-48bada28-a==1.0.0")
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

    Ok(())
}

/// requires-python-version-greater-than-current
///
/// The user requires a package which requires a Python version greater than the
/// current version
///
/// requires-python-version-greater-than-current-00f79f44
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
        r"requires-python-version-greater-than-current-00f79f44-",
        "",
    ));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-python-version-greater-than-current-00f79f44-a==1.0.0")
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

    Ok(())
}

/// requires-python-version-greater-than-current-many
///
/// The user requires a package which has many versions which all require a Python
/// version greater than the current version
///
/// requires-python-version-greater-than-current-many-b33dc0cb
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
        r"requires-python-version-greater-than-current-many-b33dc0cb-",
        "",
    ));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-python-version-greater-than-current-many-b33dc0cb-a==1.0.0")
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

    Ok(())
}

/// requires-python-version-greater-than-current-backtrack
///
/// The user requires a package where recent versions require a Python version
/// greater than the current version, but an older version is compatible.
///
/// requires-python-version-greater-than-current-backtrack-d756219a
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
        r"requires-python-version-greater-than-current-backtrack-d756219a-",
        "",
    ));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-python-version-greater-than-current-backtrack-d756219a-a")
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

    Ok(())
}

/// requires-python-version-greater-than-current-excluded
///
/// The user requires a package where recent versions require a Python version
/// greater than the current version, but an excluded older version is compatible.
///
/// requires-python-version-greater-than-current-excluded-7869d97e
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
        r"requires-python-version-greater-than-current-excluded-7869d97e-",
        "",
    ));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("requires-python-version-greater-than-current-excluded-7869d97e-a>=2.0.0")
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

    Ok(())
}
