#![cfg(all(feature = "python", feature = "pypi"))]

use std::path::Path;
use std::process::Command;

use anyhow::Result;
use assert_cmd::assert::Assert;
use assert_cmd::prelude::*;
use assert_fs::prelude::*;
use insta_cmd::_macro_support::insta;
use insta_cmd::{assert_cmd_snapshot, get_cargo_bin};

use common::{create_venv_py312, BIN_NAME, INSTA_FILTERS};

mod common;

// Exclude any packages uploaded after this date.
static EXCLUDE_NEWER: &str = "2023-11-18T12:00:00Z";

fn assert_command(venv: &Path, command: &str, temp_dir: &Path) -> Assert {
    Command::new(venv.join("bin").join("python"))
        .arg("-c")
        .arg(command)
        .current_dir(temp_dir)
        .assert()
}

#[test]
fn missing_requirements_txt() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let requirements_txt = temp_dir.child("requirements.txt");

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .arg("pip-install")
        .arg("-r")
        .arg("requirements.txt")
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .current_dir(&temp_dir), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: failed to open file `requirements.txt`
      Caused by: No such file or directory (os error 2)
    "###);

    requirements_txt.assert(predicates::path::missing());

    Ok(())
}

#[test]
fn no_solution() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv_py312(&temp_dir, &cache_dir);

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .arg("pip-install")
        .arg("flask>=3.0.0")
        .arg("WerkZeug<1.0.0")
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .arg("--exclude-newer")
            .arg(EXCLUDE_NEWER)
            .env("VIRTUAL_ENV", venv.as_os_str())
        .current_dir(&temp_dir), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because flask==3.0.0 depends on werkzeug>=3.0.0 and there is no
          version of flask available matching >3.0.0, flask>=3.0.0 depends on
          werkzeug>=3.0.0.
          And because root depends on werkzeug<1.0.0 and root depends on
          flask>=3.0.0, version solving failed.
    "###);

    Ok(())
}

/// Install a package from the command line into a virtual environment.
#[test]
fn install_package() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv_py312(&temp_dir, &cache_dir);

    // Install Flask.
    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("Flask")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .arg("--exclude-newer")
            .arg(EXCLUDE_NEWER)
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Resolved 7 packages in [TIME]
        Downloaded 7 packages in [TIME]
        Installed 7 packages in [TIME]
         + blinker==1.7.0
         + click==8.1.7
         + flask==3.0.0
         + itsdangerous==2.1.2
         + jinja2==3.1.2
         + markupsafe==2.1.3
         + werkzeug==3.0.1
        "###);
    });

    assert_command(&venv, "import flask", &temp_dir).success();

    Ok(())
}

/// Install a package from a `requirements.txt` into a virtual environment.
#[test]
fn install_requirements_txt() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv_py312(&temp_dir, &cache_dir);

    // Install Flask.
    let requirements_txt = temp_dir.child("requirements.txt");
    requirements_txt.write_str("Flask")?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("-r")
            .arg("requirements.txt")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .arg("--exclude-newer")
            .arg(EXCLUDE_NEWER)
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Resolved 7 packages in [TIME]
        Downloaded 7 packages in [TIME]
        Installed 7 packages in [TIME]
         + blinker==1.7.0
         + click==8.1.7
         + flask==3.0.0
         + itsdangerous==2.1.2
         + jinja2==3.1.2
         + markupsafe==2.1.3
         + werkzeug==3.0.1
        "###);
    });

    assert_command(&venv, "import flask", &temp_dir).success();

    // Install Jinja2 (which should already be installed, but shouldn't remove other packages).
    let requirements_txt = temp_dir.child("requirements.txt");
    requirements_txt.write_str("Jinja2")?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("-r")
            .arg("requirements.txt")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .arg("--exclude-newer")
            .arg(EXCLUDE_NEWER)
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Audited 1 package in [TIME]
        "###);
    });

    assert_command(&venv, "import flask", &temp_dir).success();

    Ok(())
}

/// Respect installed versions when resolving.
#[test]
fn respect_installed() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv_py312(&temp_dir, &cache_dir);

    // Install Flask.
    let requirements_txt = temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("Flask==2.3.2")?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("-r")
            .arg("requirements.txt")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .arg("--exclude-newer")
            .arg(EXCLUDE_NEWER)
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Resolved 7 packages in [TIME]
        Downloaded 7 packages in [TIME]
        Installed 7 packages in [TIME]
         + blinker==1.7.0
         + click==8.1.7
         + flask==2.3.2
         + itsdangerous==2.1.2
         + jinja2==3.1.2
         + markupsafe==2.1.3
         + werkzeug==3.0.1
        "###);
    });

    assert_command(&venv, "import flask", &temp_dir).success();

    // Re-install Flask. We should respect the existing version.
    let requirements_txt = temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("Flask")?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("-r")
            .arg("requirements.txt")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .arg("--exclude-newer")
            .arg(EXCLUDE_NEWER)
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Audited 1 package in [TIME]
        "###);
    });

    assert_command(&venv, "import flask", &temp_dir).success();

    // Install a newer version of Flask. We should upgrade it.
    let requirements_txt = temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("Flask==2.3.3")?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("-r")
            .arg("requirements.txt")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .arg("--exclude-newer")
            .arg(EXCLUDE_NEWER)
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Resolved 7 packages in [TIME]
        Downloaded 1 package in [TIME]
        Installed 1 package in [TIME]
         - flask==2.3.2
         + flask==2.3.3
        "###);
    });

    // Re-install Flask. We should upgrade it.
    let requirements_txt = temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("Flask")?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("-r")
            .arg("requirements.txt")
            .arg("--reinstall-package")
            .arg("Flask")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .arg("--exclude-newer")
            .arg(EXCLUDE_NEWER)
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Resolved 7 packages in [TIME]
        Downloaded 1 package in [TIME]
        Installed 1 package in [TIME]
         - flask==2.3.3
         + flask==3.0.0
        "###);
    });

    Ok(())
}

/// Like `pip`, we (unfortunately) allow incompatible environments.
#[test]
fn allow_incompatibilities() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv_py312(&temp_dir, &cache_dir);

    // Install Flask, which relies on `Werkzeug>=3.0.0`.
    let requirements_txt = temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("Flask")?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("-r")
            .arg("requirements.txt")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .arg("--exclude-newer")
            .arg(EXCLUDE_NEWER)
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Resolved 7 packages in [TIME]
        Downloaded 7 packages in [TIME]
        Installed 7 packages in [TIME]
         + blinker==1.7.0
         + click==8.1.7
         + flask==3.0.0
         + itsdangerous==2.1.2
         + jinja2==3.1.2
         + markupsafe==2.1.3
         + werkzeug==3.0.1
        "###);
    });

    assert_command(&venv, "import flask", &temp_dir).success();

    // Install an incompatible version of Jinja2.
    let requirements_txt = temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("jinja2==2.11.3")?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-install")
            .arg("-r")
            .arg("requirements.txt")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .arg("--exclude-newer")
            .arg(EXCLUDE_NEWER)
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Resolved 2 packages in [TIME]
        Downloaded 1 package in [TIME]
        Installed 1 package in [TIME]
         - jinja2==3.1.2
         + jinja2==2.11.3
        warning: The package `flask` requires `jinja2 >=3.1.2`, but `2.11.3` is installed.
        "###);
    });

    // This no longer works, since we have an incompatible version of Jinja2.
    assert_command(&venv, "import flask", &temp_dir).failure();

    Ok(())
}
