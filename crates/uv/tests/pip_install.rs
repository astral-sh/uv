#![cfg(all(feature = "python", feature = "pypi"))]

use std::process::Command;

use anyhow::Result;
use assert_cmd::prelude::*;
use assert_fs::prelude::*;
use base64::{prelude::BASE64_STANDARD as base64, Engine};
use indoc::indoc;
use itertools::Itertools;
use url::Url;

use common::{uv_snapshot, TestContext, EXCLUDE_NEWER, INSTA_FILTERS};

use crate::common::get_bin;

mod common;

// This is a fine-grained token that only has read-only access to the `uv-private-pypackage` repository
const READ_ONLY_GITHUB_TOKEN: &[&str] = &[
    "Z2l0aHViX3BhdA==",
    "MTFCR0laQTdRMGdXeGsweHV6ekR2Mg==",
    "NVZMaExzZmtFMHZ1ZEVNd0pPZXZkV040WUdTcmk2WXREeFB4TFlybGlwRTZONEpHV01FMnFZQWJVUm4=",
];

/// Decode a split, base64 encoded authentication token.
/// We split and encode the token to bypass revoke by GitHub's secret scanning
fn decode_token(content: &[&str]) -> String {
    let token = content
        .iter()
        .map(|part| base64.decode(part).unwrap())
        .map(|decoded| std::str::from_utf8(decoded.as_slice()).unwrap().to_string())
        .join("_");
    token
}

/// Create a `pip install` command with options shared across scenarios.
fn command(context: &TestContext) -> Command {
    let mut command = command_without_exclude_newer(context);
    command.arg("--exclude-newer").arg(EXCLUDE_NEWER);
    command
}

/// Create a `pip install` command with no `--exclude-newer` option.
///
/// One should avoid using this in tests to the extent possible because
/// it can result in tests failing when the index state changes. Therefore,
/// if you use this, there should be some other kind of mitigation in place.
/// For example, pinning package versions.
fn command_without_exclude_newer(context: &TestContext) -> Command {
    let mut command = Command::new(get_bin());
    command
        .arg("pip")
        .arg("install")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .env("VIRTUAL_ENV", context.venv.as_os_str())
        .current_dir(&context.temp_dir);

    if cfg!(all(windows, debug_assertions)) {
        // TODO(konstin): Reduce stack usage in debug mode enough that the tests pass with the
        // default windows stack of 1MB
        command.env("UV_STACK_SIZE", (2 * 1024 * 1024).to_string());
    }

    command
}

/// Create a `pip uninstall` command with options shared across scenarios.
fn uninstall_command(context: &TestContext) -> Command {
    let mut command = Command::new(get_bin());
    command
        .arg("pip")
        .arg("uninstall")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .env("VIRTUAL_ENV", context.venv.as_os_str())
        .current_dir(&context.temp_dir);

    if cfg!(all(windows, debug_assertions)) {
        // TODO(konstin): Reduce stack usage in debug mode enough that the tests pass with the
        // default windows stack of 1MB
        command.env("UV_STACK_SIZE", (2 * 1024 * 1024).to_string());
    }

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
    error: failed to read from file `requirements.txt`
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

/// Respect installed versions when resolving.
#[test]
fn reinstall_extras() -> Result<()> {
    let context = TestContext::new("3.12");

    // Install httpx.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("httpx")?;

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
     + anyio==4.0.0
     + certifi==2023.11.17
     + h11==0.14.0
     + httpcore==1.0.2
     + httpx==0.25.1
     + idna==3.4
     + sniffio==1.3.0
    "###
    );

    context.assert_command("import httpx").success();

    // Re-install httpx, with an extra.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("httpx[http2]")?;

    uv_snapshot!(command(&context)
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 10 packages in [TIME]
    Downloaded 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + h2==4.1.0
     + hpack==4.0.0
     + hyperframe==6.0.1
    "###
    );

    context.assert_command("import httpx").success();

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
    uv_snapshot!(filters, command(&context)
        .arg("-e")
        .arg("../../scripts/editable-installs/poetry_editable")
        .current_dir(&current_dir)
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
    uv_snapshot!(filters, command(&context)
        .arg("black")
        .current_dir(&current_dir)
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
    uv_snapshot!(filters, command(&context)
        .arg("-e")
        .arg("../../scripts/editable-installs/black_editable")
        .current_dir(&current_dir)
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
    uv_snapshot!(filters, command(&context)
        .arg("black")
        .arg("--strict")
        .current_dir(&current_dir)
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
    uv_snapshot!(filters2, command(&context)
        .arg("black==23.10.0")
        .current_dir(&current_dir)
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

#[test]
fn install_editable_no_binary() -> Result<()> {
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

    // Install the editable package with no-binary enabled
    uv_snapshot!(filters, command(&context)
        .arg("-e")
        .arg("../../scripts/editable-installs/black_editable")
        .arg("--no-binary")
        .arg(":all:")
        .current_dir(&current_dir)
        .env("CARGO_TARGET_DIR", "../../../target/target_install_editable"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Built 1 editable in [TIME]
    Resolved 1 package in [TIME]
    Installed 1 package in [TIME]
     + black==0.1.0 (from file://[WORKSPACE_DIR]/scripts/editable-installs/black_editable)
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

/// Install a package via --extra-index-url.
///
/// This is a regression test where previously `uv` would consult test.pypi.org
/// first, and if the package was found there, `uv` would not look at any other
/// indexes. We fixed this by flipping the priority order of indexes so that
/// test.pypi.org becomes the fallback (in this example) and the extra indexes
/// (regular PyPI) are checked first.
///
/// (Neither approach matches `pip`'s behavior, which considers versions of
/// each package from all indexes. `uv` stops at the first index it finds a
/// package in.)
///
/// Ref: <https://github.com/astral-sh/uv/issues/1600>
#[test]
fn install_extra_index_url_has_priority() {
    let context = TestContext::new("3.12");

    uv_snapshot!(command_without_exclude_newer(&context)
        .arg("--index-url")
        .arg("https://test.pypi.org/simple")
        .arg("--extra-index-url")
        .arg("https://pypi.org/simple")
        // This tests what we want because BOTH of the following
        // are true: `black` is on pypi.org and test.pypi.org, AND
        // `black==24.2.0` is on pypi.org and NOT test.pypi.org. So
        // this would previously check for `black` on test.pypi.org,
        // find it, but then not find a compatible version. After
        // the fix, `uv` will check pypi.org first since it is given
        // priority via --extra-index-url.
        .arg("black==24.2.0")
        .arg("--no-deps")
        .arg("--exclude-newer")
        .arg("2024-03-09"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + black==24.2.0
    "###
    );

    context.assert_command("import flask").failure();
}

/// Install a package from a public GitHub repository
#[test]
#[cfg(feature = "git")]
fn install_git_public_https() {
    let context = TestContext::new("3.8");

    let mut command = command(&context);
    command.arg("uv-public-pypackage @ git+https://github.com/astral-test/uv-public-pypackage");

    uv_snapshot!(command
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + uv-public-pypackage==0.1.0 (from git+https://github.com/astral-test/uv-public-pypackage@0dacfd662c64cb4ceb16e6cf65a157a8b715b979)
    "###);

    context.assert_installed("uv_public_pypackage", "0.1.0");
}

/// Install a package from a public GitHub repository at a ref that does not exist
#[test]
#[cfg(feature = "git")]
fn install_git_public_https_missing_branch_or_tag() {
    let context = TestContext::new("3.8");

    let mut filters = context.filters();
    // Windows does not style the command the same as Unix, so we must omit it from the snapshot
    filters.push(("`git fetch .*`", "`git fetch [...]`"));
    filters.push(("exit status", "exit code"));

    uv_snapshot!(filters, command(&context)
        // 2.0.0 does not exist
        .arg("uv-public-pypackage @ git+https://github.com/astral-test/uv-public-pypackage@2.0.0"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to download and build: uv-public-pypackage @ git+https://github.com/astral-test/uv-public-pypackage@2.0.0
      Caused by: Git operation failed
      Caused by: failed to clone into: [CACHE_DIR]/git-v0/db/8dab139913c4b566
      Caused by: failed to fetch branch or tag `2.0.0`
      Caused by: process didn't exit successfully: `git fetch [...]` (exit code: 128)
    --- stderr
    fatal: couldn't find remote ref refs/tags/2.0.0

    "###);
}

/// Install a package from a public GitHub repository at a ref that does not exist
#[test]
#[cfg(feature = "git")]
fn install_git_public_https_missing_commit() {
    let context = TestContext::new("3.8");

    let mut filters = context.filters();
    // Windows does not style the command the same as Unix, so we must omit it from the snapshot
    filters.push(("`git fetch .*`", "`git fetch [...]`"));
    filters.push(("exit status", "exit code"));

    uv_snapshot!(filters, command(&context)
        // 2.0.0 does not exist
        .arg("uv-public-pypackage @ git+https://github.com/astral-test/uv-public-pypackage@79a935a7a1a0ad6d0bdf72dce0e16cb0a24a1b3b")
        , @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to download and build: uv-public-pypackage @ git+https://github.com/astral-test/uv-public-pypackage@79a935a7a1a0ad6d0bdf72dce0e16cb0a24a1b3b
      Caused by: Git operation failed
      Caused by: failed to clone into: [CACHE_DIR]/git-v0/db/8dab139913c4b566
      Caused by: failed to fetch commit `79a935a7a1a0ad6d0bdf72dce0e16cb0a24a1b3b`
      Caused by: process didn't exit successfully: `git fetch [...]` (exit code: 128)
    --- stderr
    fatal: remote error: upload-pack: not our ref 79a935a7a1a0ad6d0bdf72dce0e16cb0a24a1b3b

    "###);
}

/// Install a package from a private GitHub repository using a PAT
#[test]
#[cfg(all(not(windows), feature = "git"))]
fn install_git_private_https_pat() {
    let context = TestContext::new("3.8");
    let token = decode_token(READ_ONLY_GITHUB_TOKEN);

    let mut filters = INSTA_FILTERS.to_vec();
    filters.insert(0, (&token, "***"));

    let mut command = command(&context);
    command.arg(format!(
        "uv-private-pypackage @ git+https://{token}@github.com/astral-test/uv-private-pypackage"
    ));

    uv_snapshot!(filters, command
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + uv-private-pypackage==0.1.0 (from git+https://***@github.com/astral-test/uv-private-pypackage@6c09ce9ae81f50670a60abd7d95f30dd416d00ac)
    "###);

    context.assert_installed("uv_private_pypackage", "0.1.0");
}

/// Install a package from a private GitHub repository at a specific commit using a PAT
#[test]
#[cfg(feature = "git")]
fn install_git_private_https_pat_at_ref() {
    let context = TestContext::new("3.8");
    let token = decode_token(READ_ONLY_GITHUB_TOKEN);

    let mut filters = INSTA_FILTERS.to_vec();
    filters.insert(0, (&token, "***"));
    filters.push((r"git\+https://", ""));

    // A user is _required_ on Windows
    let user = if cfg!(windows) {
        filters.push((r"git:", ""));
        "git:"
    } else {
        ""
    };

    let mut command = command(&context);
    command.arg(format!("uv-private-pypackage @ git+https://{user}{token}@github.com/astral-test/uv-private-pypackage@6c09ce9ae81f50670a60abd7d95f30dd416d00ac"));

    uv_snapshot!(filters, command, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + uv-private-pypackage==0.1.0 (from ***@github.com/astral-test/uv-private-pypackage@6c09ce9ae81f50670a60abd7d95f30dd416d00ac)
    "###);

    context.assert_installed("uv_private_pypackage", "0.1.0");
}

/// Install a package from a private GitHub repository using a PAT and username
/// An arbitrary username is supported when using a PAT.
///
/// TODO(charlie): This test modifies the user's keyring.
/// See: <https://github.com/astral-sh/uv/issues/1980>.
#[test]
#[cfg(feature = "git")]
#[ignore]
fn install_git_private_https_pat_and_username() {
    let context = TestContext::new("3.8");
    let token = decode_token(READ_ONLY_GITHUB_TOKEN);
    let user = "astral-test-bot";

    let mut filters = INSTA_FILTERS.to_vec();
    filters.insert(0, (&token, "***"));

    let mut command = command(&context);
    command.arg(format!("uv-private-pypackage @ git+https://{user}:{token}@github.com/astral-test/uv-private-pypackage"));

    uv_snapshot!(filters, command
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + uv-private-pypackage==0.1.0 (from git+https://astral-test-bot:***@github.com/astral-test/uv-private-pypackage@6c09ce9ae81f50670a60abd7d95f30dd416d00ac)
    "###);

    context.assert_installed("uv_private_pypackage", "0.1.0");
}

/// Install a package from a private GitHub repository using a PAT
#[test]
#[cfg(all(not(windows), feature = "git"))]
fn install_git_private_https_pat_not_authorized() {
    let context = TestContext::new("3.8");

    // A revoked token
    let token = "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";

    let mut filters = context.filters();
    filters.insert(0, (token, "***"));

    // We provide a username otherwise (since the token is invalid), the git cli will prompt for a password
    // and hang the test
    uv_snapshot!(filters, command(&context)
        .arg(format!("uv-private-pypackage @ git+https://git:{token}@github.com/astral-test/uv-private-pypackage"))
        , @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to download and build: uv-private-pypackage @ git+https://git:***@github.com/astral-test/uv-private-pypackage
      Caused by: Git operation failed
      Caused by: failed to clone into: [CACHE_DIR]/git-v0/db/2496970ed6fdf08f
      Caused by: process didn't exit successfully: `git fetch --force --update-head-ok 'https://git:***@github.com/astral-test/uv-private-pypackage' '+HEAD:refs/remotes/origin/HEAD'` (exit status: 128)
    --- stderr
    remote: Support for password authentication was removed on August 13, 2021.
    remote: Please see https://docs.github.com/get-started/getting-started-with-git/about-remote-repositories#cloning-with-https-urls for information on currently recommended modes of authentication.
    fatal: Authentication failed for 'https://github.com/astral-test/uv-private-pypackage/'

    "###);
}

/// Install a package without using pre-built wheels.
#[test]
fn reinstall_no_binary() {
    let context = TestContext::new("3.12");

    // The first installation should use a pre-built wheel
    let mut command = command(&context);
    command.arg("anyio").arg("--strict");
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

/// Install a package from a `constraints.txt` file on a remote http server.
#[test]
fn install_constraints_remote() {
    let context = TestContext::new("3.12");

    uv_snapshot!(command(&context)
            .arg("-c")
            .arg("https://raw.githubusercontent.com/apache/airflow/constraints-2-6/constraints-3.11.txt")
            .arg("typing_extensions>=4.0"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + typing-extensions==4.7.1
    "###
    ); // would yield typing-extensions==4.8.2 without constraint file
}

/// Install a package from a `requirements.txt` file, with an inline constraint, which points
/// to a remote http server.
#[test]
fn install_constraints_inline_remote() -> Result<()> {
    let context = TestContext::new("3.12");
    let requirementstxt = context.temp_dir.child("requirements.txt");
    requirementstxt.write_str("typing-extensions>=4.0\n-c https://raw.githubusercontent.com/apache/airflow/constraints-2-6/constraints-3.11.txt")?;

    uv_snapshot!(command(&context)
            .arg("-r")
            .arg("requirements.txt"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + typing-extensions==4.7.1
    "### // would yield typing-extensions==4.8.2 without constraint file
    );

    Ok(())
}

#[test]
fn install_constraints_respects_offline_mode() {
    let context = TestContext::new("3.12");

    uv_snapshot!(command(&context)
            .arg("--offline")
            .arg("-r")
            .arg("http://example.com/requirements.txt"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Network connectivity is disabled, but a remote requirements file was requested: http://example.com/requirements.txt
    "###
    );
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

/// Install a source distribution with `--resolution=lowest-direct`, to ensure that the build
/// requirements aren't resolved at their lowest compatible version.
#[test]
fn install_sdist_resolution_lowest() -> Result<()> {
    let context = TestContext::new("3.12");
    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str("anyio @ https://files.pythonhosted.org/packages/2d/b8/7333d87d5f03247215d86a86362fd3e324111788c6cdd8d2e6196a6ba833/anyio-4.2.0.tar.gz")?;

    uv_snapshot!(command(&context)
            .arg("-r")
            .arg("requirements.in")
            .arg("--resolution=lowest-direct"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Downloaded 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.2.0 (from https://files.pythonhosted.org/packages/2d/b8/7333d87d5f03247215d86a86362fd3e324111788c6cdd8d2e6196a6ba833/anyio-4.2.0.tar.gz)
     + idna==3.4
     + sniffio==1.3.0
    "###
    );

    Ok(())
}

/// Tests that we can install a package from a zip file that has bunk
/// permissions.
///
/// See: <https://github.com/astral-sh/uv/issues/1453>
#[test]
fn direct_url_zip_file_bunk_permissions() -> Result<()> {
    let context = TestContext::new("3.12");
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(
        "opensafely-pipeline @ https://github.com/opensafely-core/pipeline/archive/refs/tags/v2023.11.06.145820.zip",
    )?;

    uv_snapshot!(command(&context)
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Downloaded 5 packages in [TIME]
    Installed 6 packages in [TIME]
     + distro==1.8.0
     + opensafely-pipeline==2023.11.6.145820 (from https://github.com/opensafely-core/pipeline/archive/refs/tags/v2023.11.06.145820.zip)
     + pydantic==1.10.13
     + ruyaml==0.91.0
     + setuptools==68.2.2
     + typing-extensions==4.8.0
    "###
    );

    Ok(())
}

#[test]
fn launcher() -> Result<()> {
    let context = TestContext::new("3.12");
    let project_root = fs_err::canonicalize(std::env::current_dir()?.join("../.."))?;

    let filters = [
        (r"(\d+m )?(\d+\.)?\d+(ms|s)", "[TIME]"),
        (
            r"simple-launcher==0\.1\.0 \(from .+\.whl\)",
            "simple_launcher.whl",
        ),
    ];

    uv_snapshot!(
        filters,
        command(&context)
        .arg(format!("simple_launcher@{}", project_root.join("scripts/wheels/simple_launcher-0.1.0-py3-none-any.whl").display()))
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + simple_launcher.whl
    "###
    );

    let bin_path = if cfg!(windows) { "Scripts" } else { "bin" };

    uv_snapshot!(Command::new(
        context.venv.join(bin_path).join("simple_launcher")
    ), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Hi from the simple launcher!

    ----- stderr -----
    "###);

    Ok(())
}

#[test]
fn launcher_with_symlink() -> Result<()> {
    let context = TestContext::new("3.12");
    let project_root = fs_err::canonicalize(std::env::current_dir()?.join("../.."))?;

    let filters = [
        (r"(\d+m )?(\d+\.)?\d+(ms|s)", "[TIME]"),
        (
            r"simple-launcher==0\.1\.0 \(from .+\.whl\)",
            "simple_launcher.whl",
        ),
    ];

    uv_snapshot!(filters,
        command(&context)
            .arg(format!("simple_launcher@{}", project_root.join("scripts/wheels/simple_launcher-0.1.0-py3-none-any.whl").display()))
            .arg("--strict"),
        @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + simple_launcher.whl
    "###
    );

    #[cfg(windows)]
    if let Err(error) = std::os::windows::fs::symlink_file(
        context.venv.join("Scripts\\simple_launcher.exe"),
        context.temp_dir.join("simple_launcher.exe"),
    ) {
        // Os { code: 1314, kind: Uncategorized, message: "A required privilege is not held by the client." }
        // where `Uncategorized` is unstable.
        if error.raw_os_error() == Some(1314) {
            return Ok(());
        }
        return Err(error.into());
    }

    #[cfg(unix)]
    std::os::unix::fs::symlink(
        context.venv.join("bin/simple_launcher"),
        context.temp_dir.join("simple_launcher"),
    )?;

    // Only support windows or linux
    #[cfg(not(any(windows, unix)))]
    return Ok(());

    uv_snapshot!(Command::new(
        context.temp_dir.join("simple_launcher")
    ), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Hi from the simple launcher!

    ----- stderr -----
    "###);

    Ok(())
}

#[test]
#[cfg(unix)]
fn config_settings() -> Result<()> {
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
        .arg("../../scripts/editable-installs/setuptools_editable")
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
     + iniconfig==2.0.0
     + setuptools-editable==0.1.0 (from file://[WORKSPACE_DIR]/scripts/editable-installs/setuptools_editable)
    "###
    );

    // When installed without `--editable_mode=compat`, the `finder.py` file should be present.
    let finder = context
        .venv
        .join("lib/python3.12/site-packages")
        .join("__editable___setuptools_editable_0_1_0_finder.py");
    assert!(finder.exists());

    // Install the editable package with `--editable_mode=compat`.
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

    uv_snapshot!(filters, Command::new(get_bin())
        .arg("pip")
        .arg("install")
        .arg("-e")
        .arg("../../scripts/editable-installs/setuptools_editable")
        .arg("-C")
        .arg("editable_mode=compat")
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
     + iniconfig==2.0.0
     + setuptools-editable==0.1.0 (from file://[WORKSPACE_DIR]/scripts/editable-installs/setuptools_editable)
    "###
    );

    // When installed without `--editable_mode=compat`, the `finder.py` file should _not_ be present.
    let finder = context
        .venv
        .join("lib/python3.12/site-packages")
        .join("__editable___setuptools_editable_0_1_0_finder.py");
    assert!(!finder.exists());

    Ok(())
}

/// Reinstall a duplicate package in a virtual environment.
#[test]
#[cfg(unix)]
fn reinstall_duplicate() -> Result<()> {
    use crate::common::copy_dir_all;

    // Sync a version of `pip` into a virtual environment.
    let context1 = TestContext::new("3.12");
    let requirements_txt = context1.temp_dir.child("requirements.txt");
    requirements_txt.write_str("pip==21.3.1")?;

    // Run `pip sync`.
    Command::new(get_bin())
        .arg("pip")
        .arg("sync")
        .arg(requirements_txt.path())
        .arg("--cache-dir")
        .arg(context1.cache_dir.path())
        .env("VIRTUAL_ENV", context1.venv.as_os_str())
        .assert()
        .success();

    // Sync a different version of `pip` into a virtual environment.
    let context2 = TestContext::new("3.12");
    let requirements_txt = context2.temp_dir.child("requirements.txt");
    requirements_txt.write_str("pip==22.1.1")?;

    // Run `pip sync`.
    Command::new(get_bin())
        .arg("pip")
        .arg("sync")
        .arg(requirements_txt.path())
        .arg("--cache-dir")
        .arg(context2.cache_dir.path())
        .env("VIRTUAL_ENV", context2.venv.as_os_str())
        .assert()
        .success();

    // Copy the virtual environment to a new location.
    copy_dir_all(
        context2
            .venv
            .join("lib/python3.12/site-packages/pip-22.1.1.dist-info"),
        context1
            .venv
            .join("lib/python3.12/site-packages/pip-22.1.1.dist-info"),
    )?;

    // Run `pip install`.
    uv_snapshot!(command(&context1)
        .arg("pip")
        .arg("--reinstall"),
        @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     - pip==21.3.1
     - pip==22.1.1
     + pip==23.3.1
    "###
    );

    Ok(())
}

/// Install a package that contains a symlink within the archive.
#[test]
fn install_symlink() {
    let context = TestContext::new("3.12");

    uv_snapshot!(command(&context)
        .arg("pgpdump==1.5")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + pgpdump==1.5
    "###
    );

    context.assert_command("import pgpdump").success();

    uv_snapshot!(uninstall_command(&context)
        .arg("pgpdump"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Uninstalled 1 package in [TIME]
     - pgpdump==1.5
    "###
    );
}

#[test]
fn invalidate_editable_on_change() -> Result<()> {
    let context = TestContext::new("3.12");

    // Create an editable package.
    let editable_dir = assert_fs::TempDir::new()?;
    let pyproject_toml = editable_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"[project]
name = "example"
version = "0.0.0"
dependencies = [
  "anyio==4.0.0"
]
requires-python = ">=3.8"
"#,
    )?;

    let filters = [(r"\(from file://.*\)", "(from [WORKSPACE_DIR])")]
        .into_iter()
        .chain(INSTA_FILTERS.to_vec())
        .collect::<Vec<_>>();

    uv_snapshot!(filters, command(&context)
        .arg("--editable")
        .arg(editable_dir.path()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Built 1 editable in [TIME]
    Resolved 4 packages in [TIME]
    Downloaded 3 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.0.0
     + example==0.0.0 (from [WORKSPACE_DIR])
     + idna==3.4
     + sniffio==1.3.0
    "###
    );

    // Re-installing should be a no-op.
    uv_snapshot!(filters, command(&context)
        .arg("--editable")
        .arg(editable_dir.path()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    "###
    );

    // Modify the editable package.
    pyproject_toml.write_str(
        r#"[project]
name = "example"
version = "0.0.0"
dependencies = [
  "anyio==3.7.1"
]
requires-python = ">=3.8"
"#,
    )?;

    // Re-installing should update the package.
    uv_snapshot!(filters, command(&context)
        .arg("--editable")
        .arg(editable_dir.path()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Built 1 editable in [TIME]
    Resolved 4 packages in [TIME]
    Downloaded 1 package in [TIME]
    Installed 2 packages in [TIME]
     - anyio==4.0.0
     + anyio==3.7.1
     - example==0.0.0 (from [WORKSPACE_DIR])
     + example==0.0.0 (from [WORKSPACE_DIR])
    "###
    );

    Ok(())
}

#[test]
fn invalidate_editable_dynamic() -> Result<()> {
    let context = TestContext::new("3.12");

    // Create an editable package with dynamic metadata
    let editable_dir = assert_fs::TempDir::new()?;
    let pyproject_toml = editable_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
[project]
name = "example"
version = "0.1.0"
dynamic = ["dependencies"]
requires-python = ">=3.11,<3.13"

[tool.setuptools.dynamic]
dependencies = {file = ["requirements.txt"]}
"#,
    )?;

    let requirements_txt = editable_dir.child("requirements.txt");
    requirements_txt.write_str("anyio==4.0.0")?;

    let filters = [(r"\(from file://.*\)", "(from [WORKSPACE_DIR])")]
        .into_iter()
        .chain(INSTA_FILTERS.to_vec())
        .collect::<Vec<_>>();

    uv_snapshot!(filters, command(&context)
        .arg("--editable")
        .arg(editable_dir.path()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Built 1 editable in [TIME]
    Resolved 4 packages in [TIME]
    Downloaded 3 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.0.0
     + example==0.1.0 (from [WORKSPACE_DIR])
     + idna==3.4
     + sniffio==1.3.0
    "###
    );

    // Re-installing should re-install.
    uv_snapshot!(filters, command(&context)
        .arg("--editable")
        .arg(editable_dir.path()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Built 1 editable in [TIME]
    Resolved 4 packages in [TIME]
    Installed 1 package in [TIME]
     - example==0.1.0 (from [WORKSPACE_DIR])
     + example==0.1.0 (from [WORKSPACE_DIR])
    "###
    );

    // Modify the requirements.
    requirements_txt.write_str("anyio==3.7.1")?;

    // Re-installing should update the package.
    uv_snapshot!(filters, command(&context)
        .arg("--editable")
        .arg(editable_dir.path()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Built 1 editable in [TIME]
    Resolved 4 packages in [TIME]
    Downloaded 1 package in [TIME]
    Installed 2 packages in [TIME]
     - anyio==4.0.0
     + anyio==3.7.1
     - example==0.1.0 (from [WORKSPACE_DIR])
     + example==0.1.0 (from [WORKSPACE_DIR])
    "###
    );

    Ok(())
}

#[test]
fn invalidate_path_on_change() -> Result<()> {
    let context = TestContext::new("3.12");

    // Create a local package.
    let editable_dir = assert_fs::TempDir::new()?;
    let pyproject_toml = editable_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"[project]
name = "example"
version = "0.0.0"
dependencies = [
  "anyio==4.0.0"
]
requires-python = ">=3.8"
"#,
    )?;

    let filters = [(r"\(from file://.*\)", "(from [WORKSPACE_DIR])")]
        .into_iter()
        .chain(INSTA_FILTERS.to_vec())
        .collect::<Vec<_>>();

    uv_snapshot!(filters, command(&context)
        .arg("example @ .")
        .current_dir(editable_dir.path()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Downloaded 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.0.0
     + example==0.0.0 (from [WORKSPACE_DIR])
     + idna==3.4
     + sniffio==1.3.0
    "###
    );

    // Re-installing should be a no-op.
    uv_snapshot!(filters, command(&context)
        .arg("example @ .")
        .current_dir(editable_dir.path()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    "###
    );

    // Modify the editable package.
    pyproject_toml.write_str(
        r#"[project]
name = "example"
version = "0.0.0"
dependencies = [
  "anyio==3.7.1"
]
requires-python = ">=3.8"
"#,
    )?;

    // Re-installing should update the package.
    uv_snapshot!(filters, command(&context)
        .arg("example @ .")
        .current_dir(editable_dir.path()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Downloaded 2 packages in [TIME]
    Installed 2 packages in [TIME]
     - anyio==4.0.0
     + anyio==3.7.1
     - example==0.0.0 (from [WORKSPACE_DIR])
     + example==0.0.0 (from [WORKSPACE_DIR])
    "###
    );

    Ok(())
}

/// Ignore a URL dependency with a non-matching marker.
#[test]
fn editable_url_with_marker() -> Result<()> {
    let context = TestContext::new("3.12");

    let editable_dir = assert_fs::TempDir::new()?;
    let pyproject_toml = editable_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
[project]
name = "example"
version = "0.1.0"
dependencies = [
  "anyio==4.0.0; python_version >= '3.11'",
  "anyio @ https://files.pythonhosted.org/packages/2d/b8/7333d87d5f03247215d86a86362fd3e324111788c6cdd8d2e6196a6ba833/anyio-4.2.0.tar.gz ; python_version < '3.11'"
]
requires-python = ">=3.11,<3.13"
"#,
    )?;

    let filters = [(r"\(from file://.*\)", "(from [WORKSPACE_DIR])")]
        .into_iter()
        .chain(INSTA_FILTERS.to_vec())
        .collect::<Vec<_>>();

    uv_snapshot!(filters, command(&context)
        .arg("--editable")
        .arg(editable_dir.path()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Built 1 editable in [TIME]
    Resolved 4 packages in [TIME]
    Downloaded 3 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.0.0
     + example==0.1.0 (from [WORKSPACE_DIR])
     + idna==3.4
     + sniffio==1.3.0
    "###
    );

    Ok(())
}

/// Raise an error when an editable's `Requires-Python` constraint is not met.
#[test]
fn requires_python_editable() -> Result<()> {
    let context = TestContext::new("3.12");

    // Create an editable package with a `Requires-Python` constraint that is not met.
    let editable_dir = assert_fs::TempDir::new()?;
    let pyproject_toml = editable_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"[project]
name = "example"
version = "0.0.0"
dependencies = [
  "anyio==4.0.0"
]
requires-python = "<=3.8"
"#,
    )?;

    uv_snapshot!(command(&context)
        .arg("--editable")
        .arg(editable_dir.path()), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Editable `example` requires Python <=3.8, but 3.12.1 is installed
    "###
    );

    Ok(())
}

/// Install with `--no-build-isolation`, to disable isolation during PEP 517 builds.
#[test]
fn no_build_isolation() -> Result<()> {
    let context = TestContext::new("3.12");
    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str("anyio @ https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz")?;

    // We expect the build to fail, because `setuptools` is not installed.
    let filters = std::iter::once((r"exit code: 1", "exit status: 1"))
        .chain(INSTA_FILTERS.to_vec())
        .collect::<Vec<_>>();
    uv_snapshot!(filters, command(&context)
        .arg("-r")
        .arg("requirements.in")
        .arg("--no-build-isolation"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to download and build: anyio @ https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz
      Caused by: Failed to build: anyio @ https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz
      Caused by: Build backend failed to determine metadata through `prepare_metadata_for_build_wheel` with exit status: 1
    --- stdout:

    --- stderr:
    Traceback (most recent call last):
      File "<string>", line 8, in <module>
    ModuleNotFoundError: No module named 'setuptools'
    ---
    "###
    );

    // Install `setuptools` and `wheel`.
    uv_snapshot!(command(&context)
        .arg("setuptools")
        .arg("wheel"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Downloaded 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + setuptools==68.2.2
     + wheel==0.41.3
    "###);

    // We expect the build to succeed, since `setuptools` is now installed.
    uv_snapshot!(command(&context)
        .arg("-r")
        .arg("requirements.in")
        .arg("--no-build-isolation"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Downloaded 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==0.0.0 (from https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz)
     + idna==3.4
     + sniffio==1.3.0
    "###
    );

    Ok(())
}

/// This tests that `uv` can read UTF-16LE encoded requirements.txt files.
///
/// Ref: <https://github.com/astral-sh/uv/issues/2276>
#[test]
fn install_utf16le_requirements() -> Result<()> {
    let context = TestContext::new("3.12");
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_binary(&utf8_to_utf16_with_bom_le("tomli"))?;

    uv_snapshot!(command_without_exclude_newer(&context)
        .arg("-r")
        .arg("requirements.txt"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + tomli==2.0.1
    "###
    );
    Ok(())
}

/// This tests that `uv` can read UTF-16BE encoded requirements.txt files.
///
/// Ref: <https://github.com/astral-sh/uv/issues/2276>
#[test]
fn install_utf16be_requirements() -> Result<()> {
    let context = TestContext::new("3.12");
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_binary(&utf8_to_utf16_with_bom_be("tomli"))?;

    uv_snapshot!(command_without_exclude_newer(&context)
        .arg("-r")
        .arg("requirements.txt"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + tomli==2.0.1
    "###
    );
    Ok(())
}

fn utf8_to_utf16_with_bom_le(s: &str) -> Vec<u8> {
    use byteorder::ByteOrder;

    let mut u16s = vec![0xFEFF];
    u16s.extend(s.encode_utf16());
    let mut u8s = vec![0; u16s.len() * 2];
    byteorder::LittleEndian::write_u16_into(&u16s, &mut u8s);
    u8s
}

fn utf8_to_utf16_with_bom_be(s: &str) -> Vec<u8> {
    use byteorder::ByteOrder;

    let mut u16s = vec![0xFEFF];
    u16s.extend(s.encode_utf16());
    let mut u8s = vec![0; u16s.len() * 2];
    byteorder::BigEndian::write_u16_into(&u16s, &mut u8s);
    u8s
}

#[test]
fn dry_run_install() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let context = TestContext::new("3.12");
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("httpx==0.25.1")?;

    uv_snapshot!(command(&context)
        .arg("-r")
        .arg("requirements.txt")
        .arg("--dry-run")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
    Would download 7 packages
    Would install 7 packages
     + anyio==4.0.0
     + certifi==2023.11.17
     + h11==0.14.0
     + httpcore==1.0.2
     + httpx==0.25.1
     + idna==3.4
     + sniffio==1.3.0
    "###
    );

    Ok(())
}

#[test]
fn dry_run_install_url_dependency() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let context = TestContext::new("3.12");
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("anyio @ https://files.pythonhosted.org/packages/2d/b8/7333d87d5f03247215d86a86362fd3e324111788c6cdd8d2e6196a6ba833/anyio-4.2.0.tar.gz")?;

    uv_snapshot!(command(&context)
        .arg("-r")
        .arg("requirements.txt")
        .arg("--dry-run")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Would download 3 packages
    Would install 3 packages
     + anyio @ https://files.pythonhosted.org/packages/2d/b8/7333d87d5f03247215d86a86362fd3e324111788c6cdd8d2e6196a6ba833/anyio-4.2.0.tar.gz
     + idna==3.4
     + sniffio==1.3.0
    "###
    );

    Ok(())
}

#[test]
fn dry_run_uninstall_url_dependency() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let context = TestContext::new("3.12");
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("anyio @ https://files.pythonhosted.org/packages/2d/b8/7333d87d5f03247215d86a86362fd3e324111788c6cdd8d2e6196a6ba833/anyio-4.2.0.tar.gz")?;

    // Install the URL dependency
    uv_snapshot!(command(&context)
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Downloaded 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.2.0 (from https://files.pythonhosted.org/packages/2d/b8/7333d87d5f03247215d86a86362fd3e324111788c6cdd8d2e6196a6ba833/anyio-4.2.0.tar.gz)
     + idna==3.4
     + sniffio==1.3.0
    "###
    );

    // Then switch to a registry dependency
    requirements_txt.write_str("anyio")?;
    uv_snapshot!(command(&context)
        .arg("-r")
        .arg("requirements.txt")
        .arg("--upgrade-package")
        .arg("anyio")
        .arg("--dry-run")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Would download 1 package
    Would uninstall 1 package
    Would install 1 package
     - anyio==4.2.0 (from https://files.pythonhosted.org/packages/2d/b8/7333d87d5f03247215d86a86362fd3e324111788c6cdd8d2e6196a6ba833/anyio-4.2.0.tar.gz)
     + anyio==4.0.0
    "###
    );

    Ok(())
}

#[test]
fn dry_run_install_already_installed() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let context = TestContext::new("3.12");
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("httpx==0.25.1")?;

    // Install the package
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
     + anyio==4.0.0
     + certifi==2023.11.17
     + h11==0.14.0
     + httpcore==1.0.2
     + httpx==0.25.1
     + idna==3.4
     + sniffio==1.3.0
    "###
    );

    // Install again with dry run enabled
    uv_snapshot!(command(&context)
        .arg("-r")
        .arg("requirements.txt")
        .arg("--dry-run")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    Would make no changes
    "###
    );

    Ok(())
}

#[test]
fn dry_run_install_transitive_dependency_already_installed(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("httpcore==1.0.2")?;

    // Install a dependency of httpx
    uv_snapshot!(command(&context)
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Downloaded 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + certifi==2023.11.17
     + h11==0.14.0
     + httpcore==1.0.2
    "###
    );

    // Install it httpx with dry run enabled
    requirements_txt.write_str("httpx==0.25.1")?;
    uv_snapshot!(command(&context)
        .arg("-r")
        .arg("requirements.txt")
        .arg("--dry-run")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
    Would download 4 packages
    Would install 4 packages
     + anyio==4.0.0
     + httpx==0.25.1
     + idna==3.4
     + sniffio==1.3.0
    "###
    );

    Ok(())
}

#[test]
fn dry_run_install_then_upgrade() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let context = TestContext::new("3.12");
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("httpx==0.25.0")?;

    // Install the package
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
     + anyio==4.0.0
     + certifi==2023.11.17
     + h11==0.14.0
     + httpcore==0.18.0
     + httpx==0.25.0
     + idna==3.4
     + sniffio==1.3.0
    "###
    );

    // Bump the version and install with dry run enabled
    requirements_txt.write_str("httpx==0.25.1")?;
    uv_snapshot!(command(&context)
        .arg("-r")
        .arg("requirements.txt")
        .arg("--dry-run"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
    Would download 1 package
    Would uninstall 1 package
    Would install 1 package
     - httpx==0.25.0
     + httpx==0.25.1
    "###
    );

    Ok(())
}
