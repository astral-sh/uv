#![cfg(all(feature = "python", feature = "pypi"))]

use std::process::Command;

use anyhow::Result;
use assert_cmd::prelude::*;
use assert_fs::prelude::*;
use indoc::indoc;
use url::Url;

use common::{uv_snapshot, TestContext, EXCLUDE_NEWER, INSTA_FILTERS};

use crate::common::get_bin;

mod common;

/// Create a `pip install` command with options shared across scenarios.
fn command(context: &TestContext) -> Command {
    let mut command = Command::new(get_bin());
    command
        .arg("pip")
        .arg("install")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .arg("--exclude-newer")
        .arg(EXCLUDE_NEWER)
        .env("VIRTUAL_ENV", context.venv.as_os_str())
        .current_dir(&context.temp_dir);
    command
}

#[test]
fn missing_requirements_txt() {
    let context = TestContext::new("3.12");
    let requirements_txt = context.temp_dir.child("requirements.txt");

    uv_snapshot!(command(&context)
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: failed to open file `requirements.txt`
      Caused by: No such file or directory (os error 2)
    "###
    );

    requirements_txt.assert(predicates::path::missing());
}

#[test]
fn empty_requirements_txt() -> Result<()> {
    let context = TestContext::new("3.12");
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;

    uv_snapshot!(command(&context)
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: Requirements file requirements.txt does not contain any dependencies
    Audited 0 packages in [TIME]
    "###
    );

    Ok(())
}

#[test]
fn no_solution() {
    let context = TestContext::new("3.12");

    uv_snapshot!(command(&context)
        .arg("flask>=3.0.0")
        .arg("WerkZeug<1.0.0")
        .arg("--strict"), @r###"
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
}

/// Install a package from the command line into a virtual environment.
#[test]
fn install_package() {
    let context = TestContext::new("3.12");

    // Install Flask.
    uv_snapshot!(command(&context)
        .arg("Flask")
        .arg("--strict"), @r###"
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
    "###
    );

    context.assert_command("import flask").success();
}

/// Install a package from a `requirements.txt` into a virtual environment.
#[test]
fn install_requirements_txt() -> Result<()> {
    let context = TestContext::new("3.12");

    // Install Flask.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("Flask")?;

    uv_snapshot!(command(&context)
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
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
    "###
    );

    context.assert_command("import flask").success();

    // Install Jinja2 (which should already be installed, but shouldn't remove other packages).
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("Jinja2")?;

    uv_snapshot!(command(&context)
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    "###
    );

    context.assert_command("import flask").success();

    Ok(())
}

/// Respect installed versions when resolving.
#[test]
fn respect_installed_and_reinstall() -> Result<()> {
    let context = TestContext::new("3.12");

    // Install Flask.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("Flask==2.3.2")?;

    uv_snapshot!(command(&context)
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
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
    "###
    );

    context.assert_command("import flask").success();

    // Re-install Flask. We should respect the existing version.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("Flask")?;

    uv_snapshot!(command(&context)
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    "###
    );

    context.assert_command("import flask").success();

    // Install a newer version of Flask. We should upgrade it.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("Flask==2.3.3")?;

    let filters = if cfg!(windows) {
        // Remove the colorama count on windows
        INSTA_FILTERS
            .iter()
            .copied()
            .chain([("Resolved 8 packages", "Resolved 7 packages")])
            .collect()
    } else {
        INSTA_FILTERS.to_vec()
    };
    uv_snapshot!(filters, command(&context)
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     - flask==2.3.2
     + flask==2.3.3
    "###
    );

    // Re-install Flask. We should upgrade it.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("Flask")?;

    uv_snapshot!(filters, command(&context)
        .arg("-r")
        .arg("requirements.txt")
        .arg("--reinstall-package")
        .arg("Flask")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     - flask==2.3.3
     + flask==3.0.0
    "###
    );

    // Re-install Flask. We should install even though the version is current
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("Flask")?;

    uv_snapshot!(filters, command(&context)
        .arg("-r")
        .arg("requirements.txt")
        .arg("--reinstall-package")
        .arg("Flask")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
    Installed 1 package in [TIME]
     - flask==3.0.0
     + flask==3.0.0
    "###
    );

    Ok(())
}

/// Like `pip`, we (unfortunately) allow incompatible environments.
#[test]
fn allow_incompatibilities() -> Result<()> {
    let context = TestContext::new("3.12");

    // Install Flask, which relies on `Werkzeug>=3.0.0`.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("Flask")?;

    uv_snapshot!(command(&context)
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
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
    "###
    );

    context.assert_command("import flask").success();

    // Install an incompatible version of Jinja2.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("jinja2==2.11.3")?;

    uv_snapshot!(command(&context)
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
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
    "###
    );

    // This no longer works, since we have an incompatible version of Jinja2.
    context.assert_command("import flask").failure();

    Ok(())
}

#[test]
#[cfg(feature = "maturin")]
fn install_editable() -> Result<()> {
    let context = TestContext::new("3.12");

    let current_dir = std::env::current_dir()?;
    let workspace_dir = regex::escape(
        Url::from_directory_path(current_dir.join("..").join("..").canonicalize()?)
            .unwrap()
            .as_str(),
    );

    let filters = [(workspace_dir.as_str(), "file://[WORKSPACE_DIR]/")]
        .into_iter()
        .chain(INSTA_FILTERS.to_vec())
        .collect::<Vec<_>>();

    // Install the editable package.
    uv_snapshot!(filters, Command::new(get_bin())
        .arg("pip")
        .arg("install")
        .arg("-e")
        .arg("../../scripts/editable-installs/poetry_editable")
        .arg("--strict")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .arg("--exclude-newer")
        .arg(EXCLUDE_NEWER)
        .env("VIRTUAL_ENV", context.venv.as_os_str())
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
    "###
    );

    // Install it again (no-op).
    uv_snapshot!(filters, Command::new(get_bin())
        .arg("pip")
        .arg("install")
        .arg("-e")
        .arg("../../scripts/editable-installs/poetry_editable")
        .arg("--strict")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .arg("--exclude-newer")
        .arg(EXCLUDE_NEWER)
        .env("VIRTUAL_ENV", context.venv.as_os_str())
        .env("CARGO_TARGET_DIR", "../../../target/target_install_editable"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    "###
    );

    // Add another, non-editable dependency.
    uv_snapshot!(filters, Command::new(get_bin())
        .arg("pip")
        .arg("install")
        .arg("-e")
        .arg("../../scripts/editable-installs/poetry_editable")
        .arg("black")
        .arg("--strict")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .arg("--exclude-newer")
        .arg(EXCLUDE_NEWER)
        .env("VIRTUAL_ENV", context.venv.as_os_str())
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
    "###
    );

    Ok(())
}

#[test]
fn install_editable_and_registry() -> Result<()> {
    let context = TestContext::new("3.12");

    let current_dir = std::env::current_dir()?;
    let workspace_dir = regex::escape(
        Url::from_directory_path(current_dir.join("..").join("..").canonicalize()?)
            .unwrap()
            .as_str(),
    );

    let filters: Vec<_> = [(workspace_dir.as_str(), "file://[WORKSPACE_DIR]/")]
        .into_iter()
        .chain(INSTA_FILTERS.to_vec())
        .collect();

    // Install the registry-based version of Black.
    uv_snapshot!(filters, Command::new(get_bin())
        .arg("pip")
        .arg("install")
        .arg("black")
        .arg("--strict")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .arg("--exclude-newer")
        .arg(EXCLUDE_NEWER)
        .env("VIRTUAL_ENV", context.venv.as_os_str())
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
    "###
    );

    // Install the editable version of Black. This should remove the registry-based version.
    uv_snapshot!(filters, Command::new(get_bin())
        .arg("pip")
        .arg("install")
        .arg("-e")
        .arg("../../scripts/editable-installs/black_editable")
        .arg("--strict")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .arg("--exclude-newer")
        .arg(EXCLUDE_NEWER)
        .env("VIRTUAL_ENV", context.venv.as_os_str())
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
    "###
    );

    // Re-install the registry-based version of Black. This should be a no-op, since we have a
    // version of Black installed (the editable version) that satisfies the requirements.
    uv_snapshot!(filters, Command::new(get_bin())
        .arg("pip")
        .arg("install")
        .arg("black")
        .arg("--strict")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .arg("--exclude-newer")
        .arg(EXCLUDE_NEWER)
        .env("VIRTUAL_ENV", context.venv.as_os_str())
        .env("CARGO_TARGET_DIR", "../../../target/target_install_editable"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    "###
    );

    let filters2: Vec<_> = filters
        .into_iter()
        .chain([
            // Remove colorama
            ("Resolved 7 packages", "Resolved 6 packages"),
        ])
        .collect();

    // Re-install Black at a specific version. This should replace the editable version.
    uv_snapshot!(filters2, Command::new(get_bin())
        .arg("pip")
        .arg("install")
        .arg("black==23.10.0")
        .arg("--strict")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .arg("--exclude-newer")
        .arg(EXCLUDE_NEWER)
        .env("VIRTUAL_ENV", context.venv.as_os_str())
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
    "###
    );

    Ok(())
}

/// Install a source distribution that uses the `flit` build system, along with `flit`
/// at the top-level, along with `--reinstall` to force a re-download after resolution, to ensure
/// that the `flit` install and the source distribution build don't conflict.
#[test]
fn reinstall_build_system() -> Result<()> {
    let context = TestContext::new("3.12");

    // Install devpi.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(indoc! {r"
        flit_core<4.0.0
        flask @ https://files.pythonhosted.org/packages/d8/09/c1a7354d3925a3c6c8cfdebf4245bae67d633ffda1ba415add06ffc839c5/flask-3.0.0.tar.gz
        "
    })?;

    uv_snapshot!(command(&context)
        .arg("--reinstall")
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
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
    "###
    );

    Ok(())
}

/// Install a package without using the remote index
#[test]
fn install_no_index() {
    let context = TestContext::new("3.12");

    uv_snapshot!(command(&context)
        .arg("Flask")
        .arg("--no-index"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because flask was not found in the provided package locations and you
          require flask, we can conclude that the requirements are unsatisfiable.

          hint: Packages were unavailable because index lookups were disabled
          and no additional package locations were provided (try: `--find-links
          <uri>`)
    "###
    );

    context.assert_command("import flask").failure();
}

/// Install a package without using the remote index
/// Covers a case where the user requests a version which should be included in the error
#[test]
fn install_no_index_version() {
    let context = TestContext::new("3.12");

    uv_snapshot!(command(&context)
        .arg("Flask==3.0.0")
        .arg("--no-index"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because flask==3.0.0 was not found in the provided package locations
          and you require flask==3.0.0, we can conclude that the requirements
          are unsatisfiable.

          hint: Packages were unavailable because index lookups were disabled
          and no additional package locations were provided (try: `--find-links
          <uri>`)
    "###
    );

    context.assert_command("import flask").failure();
}

/// Install a package without using pre-built wheels.
#[test]
fn reinstall_no_binary() {
    let context = TestContext::new("3.12");

    // The first installation should use a pre-built wheel
    let mut command = command(&context);
    command.arg("anyio").arg("--strict");
    if cfg!(all(windows, debug_assertions)) {
        // TODO(konstin): Reduce stack usage in debug mode enough that the tests pass with the
        // default windows stack of 1MB
        command.env("UV_STACK_SIZE", (2 * 1024 * 1024).to_string());
    }
    uv_snapshot!(command, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Downloaded 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.0.0
     + idna==3.4
     + sniffio==1.3.0
    "###
    );

    context.assert_command("import anyio").success();

    // Running installation again with `--no-binary` should be a no-op
    // The first installation should use a pre-built wheel
    let mut command = crate::command(&context);
    command
        .arg("anyio")
        .arg("--no-binary")
        .arg(":all:")
        .arg("--strict");
    if cfg!(all(windows, debug_assertions)) {
        // TODO(konstin): Reduce stack usage in debug mode enough that the tests pass with the
        // default windows stack of 1MB
        command.env("UV_STACK_SIZE", (2 * 1024 * 1024).to_string());
    }
    uv_snapshot!(command, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    "###
    );

    context.assert_command("import anyio").success();

    // With `--reinstall`, `--no-binary` should have an affect
    let filters = if cfg!(windows) {
        // Remove the colorama count on windows
        INSTA_FILTERS
            .iter()
            .copied()
            .chain([("Resolved 8 packages", "Resolved 7 packages")])
            .collect()
    } else {
        INSTA_FILTERS.to_vec()
    };
    let mut command = crate::command(&context);
    command
        .arg("anyio")
        .arg("--no-binary")
        .arg(":all:")
        .arg("--reinstall-package")
        .arg("anyio")
        .arg("--strict");
    if cfg!(all(windows, debug_assertions)) {
        // TODO(konstin): Reduce stack usage in debug mode enough that the tests pass with the
        // default windows stack of 1MB
        command.env("UV_STACK_SIZE", (2 * 1024 * 1024).to_string());
    }
    uv_snapshot!(filters, command, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Installed 1 package in [TIME]
     - anyio==4.0.0
     + anyio==4.0.0
    "###
    );

    context.assert_command("import anyio").success();
}

/// Install a package into a virtual environment, and ensuring that the executable permissions
/// are retained.
///
/// This test uses the default link semantics. (On macOS, this is `clone`.)
#[test]
fn install_executable() {
    let context = TestContext::new("3.12");

    uv_snapshot!(command(&context)
        .arg("pylint==3.0.0"), @r###"
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
    "###
    );

    // Verify that `pylint` is executable.
    let executable = context
        .venv
        .join(if cfg!(windows) { "Scripts" } else { "bin" })
        .join(format!("pylint{}", std::env::consts::EXE_SUFFIX));
    Command::new(executable).arg("--version").assert().success();
}

/// Install a package into a virtual environment using copy semantics, and ensure that the
/// executable permissions are retained.
#[test]
fn install_executable_copy() {
    let context = TestContext::new("3.12");

    uv_snapshot!(command(&context)
        .arg("pylint==3.0.0")
        .arg("--link-mode")
        .arg("copy"), @r###"
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
    "###
    );

    // Verify that `pylint` is executable.
    let executable = context
        .venv
        .join(if cfg!(windows) { "Scripts" } else { "bin" })
        .join(format!("pylint{}", std::env::consts::EXE_SUFFIX));
    Command::new(executable).arg("--version").assert().success();
}

/// Install a package into a virtual environment using hardlink semantics, and ensure that the
/// executable permissions are retained.
#[test]
fn install_executable_hardlink() {
    let context = TestContext::new("3.12");

    uv_snapshot!(command(&context)
        .arg("pylint==3.0.0")
        .arg("--link-mode")
        .arg("hardlink"), @r###"
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
    "###
    );

    // Verify that `pylint` is executable.
    let executable = context
        .venv
        .join(if cfg!(windows) { "Scripts" } else { "bin" })
        .join(format!("pylint{}", std::env::consts::EXE_SUFFIX));
    Command::new(executable).arg("--version").assert().success();
}

/// Install a package from the command line into a virtual environment, ignoring its dependencies.
#[test]
fn no_deps() {
    let context = TestContext::new("3.12");

    // Install Flask.
    uv_snapshot!(command(&context)
        .arg("Flask")
        .arg("--no-deps")
        .arg("--strict"), @r###"
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
    "###
    );

    context.assert_command("import flask").failure();
}

/// Upgrade a package.
#[test]
fn install_upgrade() {
    let context = TestContext::new("3.12");

    // Install an old version of anyio and httpcore.
    uv_snapshot!(command(&context)
        .arg("anyio==3.6.2")
        .arg("httpcore==0.16.3")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Downloaded 6 packages in [TIME]
    Installed 6 packages in [TIME]
     + anyio==3.6.2
     + certifi==2023.11.17
     + h11==0.14.0
     + httpcore==0.16.3
     + idna==3.4
     + sniffio==1.3.0
    "###
    );

    context.assert_command("import anyio").success();

    // Upgrade anyio.
    uv_snapshot!(command(&context)
        .arg("anyio")
        .arg("--upgrade-package")
        .arg("anyio"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     - anyio==3.6.2
     + anyio==4.0.0
    "###
    );

    // Upgrade anyio again, should not reinstall.
    uv_snapshot!(command(&context)
        .arg("anyio")
        .arg("--upgrade-package")
        .arg("anyio"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Audited 3 packages in [TIME]
    "###
    );

    // Install httpcore, request anyio upgrade should not reinstall
    uv_snapshot!(command(&context)
        .arg("httpcore")
        .arg("--upgrade-package")
        .arg("anyio"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Audited 6 packages in [TIME]
    "###
    );

    // Upgrade httpcore with global flag
    uv_snapshot!(command(&context)
        .arg("httpcore")
        .arg("--upgrade"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     - httpcore==0.16.3
     + httpcore==1.0.2
    "###
    );
}

/// Install a package from a `requirements.txt` file, with a `constraints.txt` file.
#[test]
fn install_constraints_txt() -> Result<()> {
    let context = TestContext::new("3.12");
    let requirementstxt = context.temp_dir.child("requirements.txt");
    requirementstxt.write_str("django==5.0b1")?;

    let constraints_txt = context.temp_dir.child("constraints.txt");
    constraints_txt.write_str("sqlparse<0.4.4")?;

    uv_snapshot!(command(&context)
            .arg("-r")
            .arg("requirements.txt")
            .arg("--constraint")
            .arg("constraints.txt"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Downloaded 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + asgiref==3.7.2
     + django==5.0b1
     + sqlparse==0.4.3
    "###
    );

    Ok(())
}

/// Install a package from a `requirements.txt` file, with an inline constraint.
#[test]
fn install_constraints_inline() -> Result<()> {
    let context = TestContext::new("3.12");
    let requirementstxt = context.temp_dir.child("requirements.txt");
    requirementstxt.write_str("django==5.0b1\n-c constraints.txt")?;

    let constraints_txt = context.temp_dir.child("constraints.txt");
    constraints_txt.write_str("sqlparse<0.4.4")?;

    uv_snapshot!(command(&context)
            .arg("-r")
            .arg("requirements.txt"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Downloaded 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + asgiref==3.7.2
     + django==5.0b1
     + sqlparse==0.4.3
    "###
    );

    Ok(())
}

/// Tests that we can install `polars==0.14.0`, which has this odd dependency
/// requirement in its wheel metadata: `pyarrow>=4.0.*; extra == 'pyarrow'`.
///
/// The `>=4.0.*` is invalid, but is something we "fix" because it is out
/// of the control of the end user. However, our fix for this case ends up
/// stripping the quotes around `pyarrow` and thus produces an irrevocably
/// invalid dependency requirement.
///
/// See: <https://github.com/astral-sh/uv/issues/1477>
#[test]
fn install_pinned_polars_invalid_metadata() {
    let context = TestContext::new("3.12");

    // Install Flask.
    uv_snapshot!(command(&context)
        .arg("polars==0.14.0"),
        @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + polars==0.14.0
    "###
    );

    context.assert_command("import polars").success();
}
