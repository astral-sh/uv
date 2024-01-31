#![cfg(all(feature = "python", feature = "pypi"))]

use std::iter;
use std::path::Path;
use std::process::Command;

use anyhow::Result;
use assert_cmd::assert::Assert;
use assert_cmd::prelude::*;
use assert_fs::prelude::*;
use indoc::indoc;
use insta_cmd::_macro_support::insta;
use insta_cmd::{assert_cmd_snapshot, get_cargo_bin};

use common::{create_venv, venv_to_interpreter, BIN_NAME, EXCLUDE_NEWER, INSTA_FILTERS};

mod common;

fn assert_command(venv: &Path, command: &str, temp_dir: &Path) -> Assert {
    Command::new(venv_to_interpreter(venv))
        // https://github.com/python/cpython/issues/75953
        .arg("-B")
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

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("-r")
            .arg("requirements.txt")
            .arg("--strict")
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
    });

    requirements_txt.assert(predicates::path::missing());

    Ok(())
}

#[test]
fn no_solution() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "3.12");

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .arg("pip")
        .arg("install")
        .arg("flask>=3.0.0")
        .arg("WerkZeug<1.0.0")
        .arg("--strict")
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
      ╰─▶ Because only flask<=3.0.0 is available and flask==3.0.0 depends
          on werkzeug>=3.0.0, we can conclude that flask>=3.0.0 depends on
          werkzeug>=3.0.0.
          And because you require flask>=3.0.0 and you require werkzeug<1.0.0, we
          can conclude that the requirements are unsatisfiable.
    "###);

    Ok(())
}

/// Install a package from the command line into a virtual environment.
#[test]
fn install_package() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "3.12");

    // Install Flask.
    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("Flask")
            .arg("--strict")
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
    let venv = create_venv(&temp_dir, &cache_dir, "3.12");

    // Install Flask.
    let requirements_txt = temp_dir.child("requirements.txt");
    requirements_txt.write_str("Flask")?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("-r")
            .arg("requirements.txt")
            .arg("--strict")
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
            .arg("pip")
            .arg("install")
            .arg("-r")
            .arg("requirements.txt")
            .arg("--strict")
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
    let venv = create_venv(&temp_dir, &cache_dir, "3.12");

    // Install Flask.
    let requirements_txt = temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("Flask==2.3.2")?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("-r")
            .arg("requirements.txt")
            .arg("--strict")
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
            .arg("pip")
            .arg("install")
            .arg("-r")
            .arg("requirements.txt")
            .arg("--strict")
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
            .arg("pip")
            .arg("install")
            .arg("-r")
            .arg("requirements.txt")
            .arg("--strict")
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
            .arg("pip")
            .arg("install")
            .arg("-r")
            .arg("requirements.txt")
            .arg("--reinstall-package")
            .arg("Flask")
            .arg("--strict")
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
    let venv = create_venv(&temp_dir, &cache_dir, "3.12");

    // Install Flask, which relies on `Werkzeug>=3.0.0`.
    let requirements_txt = temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("Flask")?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("-r")
            .arg("requirements.txt")
            .arg("--strict")
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
            .arg("pip")
            .arg("install")
            .arg("-r")
            .arg("requirements.txt")
            .arg("--strict")
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

#[test]
#[cfg(feature = "maturin")]
fn install_editable() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "3.12");

    let current_dir = std::env::current_dir()?;
    let workspace_dir = regex::escape(
        current_dir
            .join("..")
            .join("..")
            .canonicalize()?
            .to_str()
            .unwrap(),
    );

    let filters = iter::once((workspace_dir.as_str(), "[WORKSPACE_DIR]"))
        .chain(INSTA_FILTERS.to_vec())
        .collect::<Vec<_>>();

    // Install the editable package.
    insta::with_settings!({
        filters => filters.clone()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("-e")
            .arg("../../scripts/editable-installs/poetry_editable")
            .arg("--strict")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .arg("--exclude-newer")
            .arg(EXCLUDE_NEWER)
            .env("VIRTUAL_ENV", venv.as_os_str())
            .env("CARGO_TARGET_DIR", "../../../target/target_install_editable"), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Built 1 editable in [TIME]
        Resolved 2 packages in [TIME]
        Downloaded 1 package in [TIME]
        Installed 2 packages in [TIME]
         + numpy==1.26.2
         + poetry-editable==0.1.0 (from file://[WORKSPACE_DIR]/scripts/editable-installs/poetry_editable)
        "###);
    });

    // Install it again (no-op).
    insta::with_settings!({
        filters => filters.clone()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("-e")
            .arg("../../scripts/editable-installs/poetry_editable")
            .arg("--strict")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .arg("--exclude-newer")
            .arg(EXCLUDE_NEWER)
            .env("VIRTUAL_ENV", venv.as_os_str())
            .env("CARGO_TARGET_DIR", "../../../target/target_install_editable"), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Audited 1 package in [TIME]
        "###);
    });

    // Add another, non-editable dependency.
    insta::with_settings!({
        filters => filters.clone()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("-e")
            .arg("../../scripts/editable-installs/poetry_editable")
            .arg("black")
            .arg("--strict")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .arg("--exclude-newer")
            .arg(EXCLUDE_NEWER)
            .env("VIRTUAL_ENV", venv.as_os_str())
            .env("CARGO_TARGET_DIR", "../../../target/target_install_editable"), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Built 1 editable in [TIME]
        Resolved 8 packages in [TIME]
        Downloaded 6 packages in [TIME]
        Installed 7 packages in [TIME]
         + black==23.11.0
         + click==8.1.7
         + mypy-extensions==1.0.0
         + packaging==23.2
         + pathspec==0.11.2
         + platformdirs==4.0.0
         - poetry-editable==0.1.0 (from file://[WORKSPACE_DIR]/scripts/editable-installs/poetry_editable)
         + poetry-editable==0.1.0 (from file://[WORKSPACE_DIR]/scripts/editable-installs/poetry_editable)
        "###);
    });

    Ok(())
}

#[test]
fn install_editable_and_registry() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "3.12");

    let current_dir = std::env::current_dir()?;
    let workspace_dir = regex::escape(
        current_dir
            .join("..")
            .join("..")
            .canonicalize()?
            .to_str()
            .unwrap(),
    );

    let filters: Vec<_> = iter::once((workspace_dir.as_str(), "[WORKSPACE_DIR]"))
        .chain(INSTA_FILTERS.to_vec())
        .collect();

    // Install the registry-based version of Black.
    insta::with_settings!({
        filters => filters.clone()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("black")
            .arg("--strict")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .arg("--exclude-newer")
            .arg(EXCLUDE_NEWER)
            .env("VIRTUAL_ENV", venv.as_os_str())
            .env("CARGO_TARGET_DIR", "../../../target/target_install_editable"), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Resolved 6 packages in [TIME]
        Downloaded 6 packages in [TIME]
        Installed 6 packages in [TIME]
         + black==23.11.0
         + click==8.1.7
         + mypy-extensions==1.0.0
         + packaging==23.2
         + pathspec==0.11.2
         + platformdirs==4.0.0
        "###);
    });

    // Install the editable version of Black. This should remove the registry-based version.
    insta::with_settings!({
        filters => filters.clone()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("-e")
            .arg("../../scripts/editable-installs/black_editable")
            .arg("--strict")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .arg("--exclude-newer")
            .arg(EXCLUDE_NEWER)
            .env("VIRTUAL_ENV", venv.as_os_str())
            .env("CARGO_TARGET_DIR", "../../../target/target_install_editable"), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Built 1 editable in [TIME]
        Resolved 1 package in [TIME]
        Installed 1 package in [TIME]
         - black==23.11.0
         + black==0.1.0 (from file://[WORKSPACE_DIR]/scripts/editable-installs/black_editable)
        "###);
    });

    // Re-install the registry-based version of Black. This should be a no-op, since we have a
    // version of Black installed (the editable version) that satisfies the requirements.
    insta::with_settings!({
        filters => filters.clone()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("black")
            .arg("--strict")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .arg("--exclude-newer")
            .arg(EXCLUDE_NEWER)
            .env("VIRTUAL_ENV", venv.as_os_str())
            .env("CARGO_TARGET_DIR", "../../../target/target_install_editable"), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Audited 1 package in [TIME]
        "###);
    });

    // Re-install Black at a specific version. This should replace the editable version.
    insta::with_settings!({
        filters => filters.clone()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("black==23.10.0")
            .arg("--strict")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .arg("--exclude-newer")
            .arg(EXCLUDE_NEWER)
            .env("VIRTUAL_ENV", venv.as_os_str())
            .env("CARGO_TARGET_DIR", "../../../target/target_install_editable"), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Resolved 6 packages in [TIME]
        Downloaded 1 package in [TIME]
        Installed 1 package in [TIME]
         - black==0.1.0 (from file://[WORKSPACE_DIR]/scripts/editable-installs/black_editable)
         + black==23.10.0
        "###);
    });

    Ok(())
}

/// Install a source distribution that uses the `flit` build system, along with `flit`
/// at the top-level, along with `--reinstall` to force a re-download after resolution, to ensure
/// that the `flit` install and the source distribution build don't conflict.
#[test]
fn reinstall_build_system() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "3.12");

    // Install devpi.
    let requirements_txt = temp_dir.child("requirements.txt");
    requirements_txt.write_str(indoc! {r"
        flit_core<4.0.0
        flask @ https://files.pythonhosted.org/packages/d8/09/c1a7354d3925a3c6c8cfdebf4245bae67d633ffda1ba415add06ffc839c5/flask-3.0.0.tar.gz
        "
    })?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("--reinstall")
            .arg("-r")
            .arg("requirements.txt")
            .arg("--strict")
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
        Resolved 8 packages in [TIME]
        Downloaded 7 packages in [TIME]
        Installed 8 packages in [TIME]
         + blinker==1.7.0
         + click==8.1.7
         + flask==3.0.0 (from https://files.pythonhosted.org/packages/d8/09/c1a7354d3925a3c6c8cfdebf4245bae67d633ffda1ba415add06ffc839c5/flask-3.0.0.tar.gz)
         + flit-core==3.9.0
         + itsdangerous==2.1.2
         + jinja2==3.1.2
         + markupsafe==2.1.3
         + werkzeug==3.0.1
        "###);
    });

    Ok(())
}

/// Install a package without using pre-built wheels.
#[test]
fn install_no_binary() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "3.12");

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("Flask")
            .arg("--no-binary")
            .arg("--strict")
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

/// Install a package without using pre-built wheels for a subset of packages.
#[test]
fn install_no_binary_subset() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "3.12");

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("Flask")
            .arg("--no-binary-package")
            .arg("click")
            .arg("--no-binary-package")
            .arg("flask")
            .arg("--strict")
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

/// Install a package without using pre-built wheels.
#[test]
fn reinstall_no_binary() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "3.12");

    // The first installation should use a pre-built wheel
    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("Flask")
            .arg("--strict")
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

    // Running installation again with `--no-binary` should be a no-op
    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("Flask")
            .arg("--no-binary")
            .arg("--strict")
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

    // With `--reinstall`, `--no-binary` should have an affect
    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("Flask")
            .arg("--no-binary")
            .arg("--reinstall-package")
            .arg("Flask")
            .arg("--strict")
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
        Installed 1 package in [TIME]
         - flask==3.0.0
         + flask==3.0.0
        "###);
    });

    assert_command(&venv, "import flask", &temp_dir).success();
    Ok(())
}

/// Install a package into a virtual environment, and ensuring that the executable permissions
/// are retained.
///
/// This test uses the default link semantics. (On macOS, this is `clone`.)
#[test]
fn install_executable() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "3.12");

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("pylint==3.0.0")
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
         + astroid==3.0.1
         + dill==0.3.7
         + isort==5.12.0
         + mccabe==0.7.0
         + platformdirs==4.0.0
         + pylint==3.0.0
         + tomlkit==0.12.3
        "###);
    });

    // Verify that `pylint` is executable.
    let executable = venv.join("bin/pylint");
    Command::new(executable).arg("--version").assert().success();

    Ok(())
}

/// Install a package into a virtual environment using copy semantics, and ensure that the
/// executable permissions are retained.
#[test]
fn install_executable_copy() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "3.12");

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("pylint==3.0.0")
            .arg("--link-mode")
            .arg("copy")
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
         + astroid==3.0.1
         + dill==0.3.7
         + isort==5.12.0
         + mccabe==0.7.0
         + platformdirs==4.0.0
         + pylint==3.0.0
         + tomlkit==0.12.3
        "###);
    });

    // Verify that `pylint` is executable.
    let executable = venv.join("bin/pylint");
    Command::new(executable).arg("--version").assert().success();

    Ok(())
}

/// Install a package into a virtual environment using hardlink semantics, and ensure that the
/// executable permissions are retained.
#[test]
fn install_executable_hardlink() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "3.12");

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("pylint==3.0.0")
            .arg("--link-mode")
            .arg("hardlink")
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
         + astroid==3.0.1
         + dill==0.3.7
         + isort==5.12.0
         + mccabe==0.7.0
         + platformdirs==4.0.0
         + pylint==3.0.0
         + tomlkit==0.12.3
        "###);
    });

    // Verify that `pylint` is executable.
    let executable = venv.join("bin/pylint");
    Command::new(executable).arg("--version").assert().success();

    Ok(())
}

/// Install a package from the command line into a virtual environment, ignoring its dependencies.
#[test]
fn no_deps() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = create_venv(&temp_dir, &cache_dir, "3.12");

    // Install Flask.
    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip")
            .arg("install")
            .arg("Flask")
            .arg("--no-deps")
            .arg("--strict")
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
        Resolved 1 package in [TIME]
        Downloaded 1 package in [TIME]
        Installed 1 package in [TIME]
         + flask==3.0.0
        warning: The package `flask` requires `werkzeug >=3.0.0`, but it's not installed.
        warning: The package `flask` requires `jinja2 >=3.1.2`, but it's not installed.
        warning: The package `flask` requires `itsdangerous >=2.1.2`, but it's not installed.
        warning: The package `flask` requires `click >=8.1.3`, but it's not installed.
        warning: The package `flask` requires `blinker >=1.6.2`, but it's not installed.
        "###);
    });

    assert_command(&venv, "import flask", &temp_dir).failure();

    Ok(())
}
