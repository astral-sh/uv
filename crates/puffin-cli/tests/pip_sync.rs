#![cfg(all(feature = "python", feature = "pypi"))]

use std::process::Command;

use anyhow::Result;
use assert_cmd::prelude::*;
use assert_fs::prelude::*;
use insta_cmd::_macro_support::insta;
use insta_cmd::{assert_cmd_snapshot, get_cargo_bin};

use common::{BIN_NAME, INSTA_FILTERS};

mod common;

#[test]
fn missing_requirements_txt() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let requirements_txt = temp_dir.child("requirements.txt");

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .arg("pip-sync")
        .arg("requirements.txt")
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .current_dir(&temp_dir));

    requirements_txt.assert(predicates::path::missing());

    Ok(())
}

#[test]
fn missing_venv() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .arg("pip-sync")
        .arg("requirements.txt")
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .env("VIRTUAL_ENV", venv.as_os_str())
        .current_dir(&temp_dir));

    venv.assert(predicates::path::missing());

    Ok(())
}

/// Install a package into a virtual environment using the default link semantics. (On macOS,
/// this using `clone` semantics.)
#[test]
fn install() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .current_dir(&temp_dir)
        .assert()
        .success();
    venv.assert(predicates::path::is_dir());

    let requirements_txt = temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("MarkupSafe==2.1.3")?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-sync")
            .arg("requirements.txt")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir));
    });

    Command::new(venv.join("bin").join("python"))
        .arg("-c")
        .arg("import markupsafe")
        .current_dir(&temp_dir)
        .assert()
        .success();

    Ok(())
}

/// Install a package into a virtual environment using copy semantics.
#[test]
fn install_copy() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .current_dir(&temp_dir)
        .assert()
        .success();
    venv.assert(predicates::path::is_dir());

    let requirements_txt = temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("MarkupSafe==2.1.3")?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-sync")
            .arg("requirements.txt")
            .arg("--link-mode")
            .arg("copy")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir));
    });

    Command::new(venv.join("bin").join("python"))
        .arg("-c")
        .arg("import markupsafe")
        .current_dir(&temp_dir)
        .assert()
        .success();

    Ok(())
}

/// Install a package into a virtual environment using hardlink semantics.
#[test]
fn install_hardlink() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .current_dir(&temp_dir)
        .assert()
        .success();
    venv.assert(predicates::path::is_dir());

    let requirements_txt = temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("MarkupSafe==2.1.3")?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-sync")
            .arg("requirements.txt")
            .arg("--link-mode")
            .arg("hardlink")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir));
    });

    Command::new(venv.join("bin").join("python"))
        .arg("-c")
        .arg("import markupsafe")
        .current_dir(&temp_dir)
        .assert()
        .success();

    Ok(())
}

/// Install multiple packages into a virtual environment.
#[test]
fn install_many() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .current_dir(&temp_dir)
        .assert()
        .success();
    venv.assert(predicates::path::is_dir());

    let requirements_txt = temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("MarkupSafe==2.1.3\ntomli==2.0.1")?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-sync")
            .arg("requirements.txt")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir));
    });

    Command::new(venv.join("bin").join("python"))
        .arg("-c")
        .arg("import markupsafe; import tomli")
        .current_dir(&temp_dir)
        .assert()
        .success();

    Ok(())
}

/// Attempt to install an already-installed package into a virtual environment.
#[test]
fn noop() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .current_dir(&temp_dir)
        .assert()
        .success();
    venv.assert(predicates::path::is_dir());

    let requirements_txt = temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("MarkupSafe==2.1.3")?;

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("pip-sync")
        .arg("requirements.txt")
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .env("VIRTUAL_ENV", venv.as_os_str())
        .current_dir(&temp_dir)
        .assert()
        .success();

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-sync")
            .arg("requirements.txt")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir));
    });

    Command::new(venv.join("bin").join("python"))
        .arg("-c")
        .arg("import markupsafe")
        .current_dir(&temp_dir)
        .assert()
        .success();

    Ok(())
}

/// Install a package into a virtual environment, then install the same package into a different
/// virtual environment.
#[test]
fn link() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .current_dir(&temp_dir)
        .assert()
        .success();
    venv.assert(predicates::path::is_dir());

    let requirements_txt = temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("MarkupSafe==2.1.3")?;

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("pip-sync")
        .arg("requirements.txt")
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .env("VIRTUAL_ENV", venv.as_os_str())
        .current_dir(&temp_dir)
        .assert()
        .success();

    let venv = temp_dir.child(".venv");
    Command::new(get_cargo_bin(BIN_NAME))
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .current_dir(&temp_dir)
        .assert()
        .success();
    venv.assert(predicates::path::is_dir());

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-sync")
            .arg("requirements.txt")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir));
    });

    Command::new(venv.join("bin").join("python"))
        .arg("-c")
        .arg("import markupsafe")
        .current_dir(&temp_dir)
        .assert()
        .success();

    Ok(())
}

/// Install a package into a virtual environment, then sync the virtual environment with a
/// different requirements file.
#[test]
fn add_remove() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .current_dir(&temp_dir)
        .assert()
        .success();
    venv.assert(predicates::path::is_dir());

    let requirements_txt = temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("MarkupSafe==2.1.3")?;

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("pip-sync")
        .arg("requirements.txt")
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .env("VIRTUAL_ENV", venv.as_os_str())
        .current_dir(&temp_dir)
        .assert()
        .success();

    let requirements_txt = temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("tomli==2.0.1")?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-sync")
            .arg("requirements.txt")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir));
    });

    Command::new(venv.join("bin").join("python"))
        .arg("-c")
        .arg("import tomli")
        .current_dir(&temp_dir)
        .assert()
        .success();

    Command::new(venv.join("bin").join("python"))
        .arg("-c")
        .arg("import markupsafe")
        .current_dir(&temp_dir)
        .assert()
        .failure();

    Ok(())
}

/// Install a package into a virtual environment, then install a second package into the same
/// virtual environment.
#[test]
fn install_sequential() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .current_dir(&temp_dir)
        .assert()
        .success();
    venv.assert(predicates::path::is_dir());

    let requirements_txt = temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("MarkupSafe==2.1.3")?;

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("pip-sync")
        .arg("requirements.txt")
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .env("VIRTUAL_ENV", venv.as_os_str())
        .current_dir(&temp_dir)
        .assert()
        .success();

    let requirements_txt = temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("MarkupSafe==2.1.3\ntomli==2.0.1")?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-sync")
            .arg("requirements.txt")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir));
    });

    Command::new(venv.join("bin").join("python"))
        .arg("-c")
        .arg("import markupsafe; import tomli")
        .current_dir(&temp_dir)
        .assert()
        .success();

    Ok(())
}

/// Install a package into a virtual environment, then install a second package into the same
/// virtual environment.
#[test]
fn upgrade() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .current_dir(&temp_dir)
        .assert()
        .success();
    venv.assert(predicates::path::is_dir());

    let requirements_txt = temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("tomli==2.0.0")?;

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("pip-sync")
        .arg("requirements.txt")
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .env("VIRTUAL_ENV", venv.as_os_str())
        .current_dir(&temp_dir)
        .assert()
        .success();

    let requirements_txt = temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("tomli==2.0.1")?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-sync")
            .arg("requirements.txt")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir));
    });

    Command::new(venv.join("bin").join("python"))
        .arg("-c")
        .arg("import tomli")
        .current_dir(&temp_dir)
        .assert()
        .success();

    Ok(())
}

/// Install a package into a virtual environment from a URL.
#[test]
fn install_url() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .current_dir(&temp_dir)
        .assert()
        .success();
    venv.assert(predicates::path::is_dir());

    let requirements_txt = temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("werkzeug @ https://files.pythonhosted.org/packages/ff/1d/960bb4017c68674a1cb099534840f18d3def3ce44aed12b5ed8b78e0153e/Werkzeug-2.0.0-py3-none-any.whl")?;

    insta::with_settings!({
        filters => vec![
            (r"(\d|\.)+(ms|s)", "[TIME]"),
        ]
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-sync")
            .arg("requirements.txt")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir));
    });

    Command::new(venv.join("bin").join("python"))
        .arg("-c")
        .arg("import werkzeug")
        .current_dir(&temp_dir)
        .assert()
        .success();

    Ok(())
}

/// Install a package into a virtual environment from a Git repository.
#[test]
#[cfg(feature = "git")]
fn install_git() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .current_dir(&temp_dir)
        .assert()
        .success();
    venv.assert(predicates::path::is_dir());

    let requirements_txt = temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("werkzeug @ git+https://github.com/pallets/werkzeug.git@2.0.0")?;

    insta::with_settings!({
        filters => vec![
            (r"(\d|\.)+(ms|s)", "[TIME]"),
        ]
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-sync")
            .arg("requirements.txt")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir));
    });

    Command::new(venv.join("bin").join("python"))
        .arg("-c")
        .arg("import werkzeug")
        .current_dir(&temp_dir)
        .assert()
        .success();

    Ok(())
}

/// Install two packages from the same Git repository.
#[test]
#[cfg(feature = "git")]
fn install_git_subdirectories() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .current_dir(&temp_dir)
        .assert()
        .success();
    venv.assert(predicates::path::is_dir());

    let requirements_txt = temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("example-pkg-a @ git+https://github.com/pypa/sample-namespace-packages.git@df7530eeb8fa0cb7dbb8ecb28363e8e36bfa2f45#subdirectory=pkg_resources/pkg_a\nexample-pkg-b @ git+https://github.com/pypa/sample-namespace-packages.git@df7530eeb8fa0cb7dbb8ecb28363e8e36bfa2f45#subdirectory=pkg_resources/pkg_b")?;

    insta::with_settings!({
        filters => vec![
            (r"(\d|\.)+(ms|s)", "[TIME]"),
        ]
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-sync")
            .arg("requirements.txt")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir));
    });

    Command::new(venv.join("bin").join("python"))
        .arg("-c")
        .arg("import example_pkg")
        .current_dir(&temp_dir)
        .assert()
        .success();

    Ok(())
}

/// Install a source distribution into a virtual environment.
#[test]
fn install_sdist() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .current_dir(&temp_dir)
        .assert()
        .success();
    venv.assert(predicates::path::is_dir());

    let requirements_txt = temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("Werkzeug==0.9.6")?;

    insta::with_settings!({
        filters => vec![
            (r"(\d|\.)+(ms|s)", "[TIME]"),
        ]
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-sync")
            .arg("requirements.txt")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir));
    });

    Command::new(venv.join("bin").join("python"))
        .arg("-c")
        .arg("import werkzeug")
        .current_dir(&temp_dir)
        .assert()
        .success();

    Ok(())
}
