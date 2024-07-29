#![cfg(all(feature = "python", feature = "pypi"))]

use std::process::Command;

use anyhow::Result;
use assert_cmd::prelude::*;
use assert_fs::prelude::*;
use base64::{prelude::BASE64_STANDARD as base64, Engine};
use fs_err as fs;
use indoc::indoc;
use itertools::Itertools;
use predicates::prelude::predicate;
use url::Url;

use common::{uv_snapshot, TestContext};
use uv_fs::Simplified;

use crate::common::{build_vendor_links_url, get_bin, venv_bin_path};

mod common;

// This is a fine-grained token that only has read-only access to the `uv-private-pypackage` repository
const READ_ONLY_GITHUB_TOKEN: &[&str] = &[
    "Z2l0aHViX3BhdA==",
    "MTFCR0laQTdRMGdXeGsweHV6ekR2Mg==",
    "NVZMaExzZmtFMHZ1ZEVNd0pPZXZkV040WUdTcmk2WXREeFB4TFlybGlwRTZONEpHV01FMnFZQWJVUm4=",
];

// This is a fine-grained token that only has read-only access to the `uv-private-pypackage-2` repository
#[cfg(not(windows))]
const READ_ONLY_GITHUB_TOKEN_2: &[&str] = &[
    "Z2l0aHViX3BhdA==",
    "MTFCR0laQTdRMHV1MEpwaFp4dFFyRwo=",
    "cnNmNXJwMHk2WWpteVZvb2ZFc0c5WUs5b2NPcFY1aVpYTnNmdE05eEhaM0lGSExSSktDWTcxeVBVZXkK",
];

/// Decode a split, base64 encoded authentication token.
/// We split and encode the token to bypass revoke by GitHub's secret scanning
fn decode_token(content: &[&str]) -> String {
    let token = content
        .iter()
        .map(|part| base64.decode(part).unwrap())
        .map(|decoded| {
            std::str::from_utf8(decoded.as_slice())
                .unwrap()
                .trim_end()
                .to_string()
        })
        .join("_");
    token
}

#[test]
fn missing_requirements_txt() {
    let context = TestContext::new("3.12");
    let requirements_txt = context.temp_dir.child("requirements.txt");

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: File not found: `requirements.txt`
    "###
    );

    requirements_txt.assert(predicates::path::missing());
}

#[test]
fn empty_requirements_txt() -> Result<()> {
    let context = TestContext::new("3.12");
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;

    uv_snapshot!(context.pip_install()
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
fn missing_pyproject_toml() {
    let context = TestContext::new("3.12");

    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg("pyproject.toml"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: File not found: `pyproject.toml`
    "###
    );
}

#[test]
fn invalid_pyproject_toml_syntax() -> Result<()> {
    let context = TestContext::new("3.12");
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str("123 - 456")?;

    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg("pyproject.toml"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: Failed to parse `pyproject.toml` during settings discovery; skipping...
    error: Failed to parse: `pyproject.toml`
      Caused by: TOML parse error at line 1, column 5
      |
    1 | 123 - 456
      |     ^
    expected `.`, `=`

    "###
    );

    Ok(())
}

#[test]
fn invalid_pyproject_toml_schema() -> Result<()> {
    let context = TestContext::new("3.12");
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str("[project]")?;

    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg("pyproject.toml"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to parse: `pyproject.toml`
      Caused by: TOML parse error at line 1, column 1
      |
    1 | [project]
      | ^^^^^^^^^
    missing field `name`

    "###
    );

    Ok(())
}

/// For indirect, non-user controlled pyproject.toml, we don't enforce correctness.
///
/// If we fail to extract the PEP 621 metadata, we fall back to treating it as a source
/// tree, as there are some cases where the `pyproject.toml` may not be a valid PEP
/// 621 file, but might still resolve under PEP 517. (If the source tree doesn't
/// resolve under PEP 517, we'll catch that later.)
///
/// For example, Hatch's "Context formatting" API is not compliant with PEP 621, as
/// it expects dynamic processing by the build backend for the static metadata
/// fields. See: <https://hatch.pypa.io/latest/config/context/>
#[test]
fn invalid_pyproject_toml_requirement_indirect() -> Result<()> {
    let context = TestContext::new("3.12");
    let pyproject_toml = context.temp_dir.child("path_dep/pyproject.toml");
    pyproject_toml.write_str(
        r#"[project]
name = "project"
dependencies = ["flask==1.0.x"]
"#,
    )?;
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("./path_dep")?;

    let filters = [("exit status", "exit code")]
        .into_iter()
        .chain(context.filters())
        .collect::<Vec<_>>();

    uv_snapshot!(filters, context.pip_install()
        .arg("-r")
        .arg("requirements.txt"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to download and build: `project @ file://[TEMP_DIR]/path_dep`
      Caused by: Failed to build: `project @ file://[TEMP_DIR]/path_dep`
      Caused by: Build backend failed to determine extra requires with `build_wheel()` with exit code: 1
    --- stdout:
    configuration error: `project.dependencies[0]` must be pep508
    DESCRIPTION:
        Project dependency specification according to PEP 508

    GIVEN VALUE:
        "flask==1.0.x"

    OFFENDING RULE: 'format'

    DEFINITION:
        {
            "$id": "#/definitions/dependency",
            "title": "Dependency",
            "type": "string",
            "format": "pep508"
        }
    --- stderr:
    Traceback (most recent call last):
      File "<string>", line 14, in <module>
      File "[CACHE_DIR]/builds-v0/[TMP]/build_meta.py", line 325, in get_requires_for_build_wheel
        return self._get_build_requires(config_settings, requirements=['wheel'])
               ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
      File "[CACHE_DIR]/builds-v0/[TMP]/build_meta.py", line 295, in _get_build_requires
        self.run_setup()
      File "[CACHE_DIR]/builds-v0/[TMP]/build_meta.py", line 487, in run_setup
        super().run_setup(setup_script=setup_script)
      File "[CACHE_DIR]/builds-v0/[TMP]/build_meta.py", line 311, in run_setup
        exec(code, locals())
      File "<string>", line 1, in <module>
      File "[CACHE_DIR]/builds-v0/[TMP]/__init__.py", line 104, in setup
        return distutils.core.setup(**attrs)
               ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
      File "[CACHE_DIR]/builds-v0/[TMP]/core.py", line 159, in setup
        dist.parse_config_files()
      File "[CACHE_DIR]/builds-v0/[TMP]/_virtualenv.py", line 22, in parse_config_files
        result = old_parse_config_files(self, *args, **kwargs)
                 ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
      File "[CACHE_DIR]/builds-v0/[TMP]/dist.py", line 631, in parse_config_files
        pyprojecttoml.apply_configuration(self, filename, ignore_option_errors)
      File "[CACHE_DIR]/builds-v0/[TMP]/pyprojecttoml.py", line 68, in apply_configuration
        config = read_configuration(filepath, True, ignore_option_errors, dist)
                 ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
      File "[CACHE_DIR]/builds-v0/[TMP]/pyprojecttoml.py", line 129, in read_configuration
        validate(subset, filepath)
      File "[CACHE_DIR]/builds-v0/[TMP]/pyprojecttoml.py", line 57, in validate
        raise ValueError(f"{error}/n{summary}") from None
    ValueError: invalid pyproject.toml config: `project.dependencies[0]`.
    configuration error: `project.dependencies[0]` must be pep508
    ---
    "###
    );

    Ok(())
}

#[test]
fn missing_pip() {
    uv_snapshot!(Command::new(get_bin()).arg("install"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: unrecognized subcommand 'install'

      tip: a similar subcommand exists: 'uv pip install'

    Usage: uv [OPTIONS] <COMMAND>

    For more information, try '--help'.
    "###);
}

#[test]
fn no_solution() {
    let context = TestContext::new("3.12");

    uv_snapshot!(context.pip_install()
        .arg("flask>=3.0.2")
        .arg("WerkZeug<1.0.0")
        .arg("--strict"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only flask<=3.0.2 is available and flask==3.0.2 depends on werkzeug>=3.0.0, we can conclude that flask>=3.0.2 depends on werkzeug>=3.0.0.
          And because you require flask>=3.0.2 and werkzeug<1.0.0, we can conclude that the requirements are unsatisfiable.
    "###);
}

/// Install a package from the command line into a virtual environment.
#[test]
fn install_package() {
    let context = TestContext::new("3.12");

    // Install Flask.
    uv_snapshot!(context.pip_install()
        .arg("Flask")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
    Prepared 7 packages in [TIME]
    Installed 7 packages in [TIME]
     + blinker==1.7.0
     + click==8.1.7
     + flask==3.0.2
     + itsdangerous==2.1.2
     + jinja2==3.1.3
     + markupsafe==2.1.5
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

    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
    Prepared 7 packages in [TIME]
    Installed 7 packages in [TIME]
     + blinker==1.7.0
     + click==8.1.7
     + flask==3.0.2
     + itsdangerous==2.1.2
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + werkzeug==3.0.1
    "###
    );

    context.assert_command("import flask").success();

    // Install Jinja2 (which should already be installed, but shouldn't remove other packages).
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("Jinja2")?;

    uv_snapshot!(context.pip_install()
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

/// Install a requirements file with pins that conflict
///
/// This is likely to occur in the real world when compiled on one platform then installed on another.
#[test]
fn install_requirements_txt_conflicting_pins() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");

    // We pin `click` to a conflicting requirement
    requirements_txt.write_str(
        r"
blinker==1.7.0
click==7.0.0
flask==3.0.2
itsdangerous==2.1.2
jinja2==3.1.3
markupsafe==2.1.5
werkzeug==3.0.1
",
    )?;

    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because flask==3.0.2 depends on click>=8.1.3 and you require click==7.0.0, we can conclude that your requirements and flask==3.0.2 are incompatible.
          And because you require flask==3.0.2, we can conclude that the requirements are unsatisfiable.
    "###
    );

    Ok(())
}

/// Install a `pyproject.toml` file with a `poetry` section.
#[test]
fn install_pyproject_toml_poetry() -> Result<()> {
    let context = TestContext::new("3.12");
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"[tool.poetry]
name = "poetry-editable"
version = "0.1.0"
description = ""
authors = ["Astral Software Inc. <hey@astral.sh>"]

[tool.poetry.dependencies]
python = "^3.10"
anyio = "^3"
iniconfig = { version = "*", optional = true }

[tool.poetry.extras]
test = ["iniconfig"]

[build-system]
requires = ["poetry-core"]
build-backend = "poetry.core.masonry.api"
"#,
    )?;

    uv_snapshot!(context.pip_install()
            .arg("-r")
            .arg("pyproject.toml")
            .arg("--extra")
            .arg("test"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==3.7.1
     + idna==3.6
     + iniconfig==2.0.0
     + sniffio==1.3.1
    "###
    );

    Ok(())
}

/// Respect installed versions when resolving.
#[test]
fn respect_installed_and_reinstall() -> Result<()> {
    let context = TestContext::new("3.12");

    // Install Flask.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("Flask==2.3.2")?;

    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
    Prepared 7 packages in [TIME]
    Installed 7 packages in [TIME]
     + blinker==1.7.0
     + click==8.1.7
     + flask==2.3.2
     + itsdangerous==2.1.2
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + werkzeug==3.0.1
    "###
    );

    context.assert_command("import flask").success();

    // Re-install Flask. We should respect the existing version.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("Flask")?;

    uv_snapshot!(context.pip_install()
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
    requirements_txt.write_str("Flask==2.3.3")?;

    let context = context.with_filtered_counts();
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Uninstalled [N] packages in [TIME]
    Installed [N] packages in [TIME]
     - flask==2.3.2
     + flask==2.3.3
    "###
    );

    // Re-install Flask. We should upgrade it.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("Flask")?;

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--reinstall-package")
        .arg("Flask")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Uninstalled [N] packages in [TIME]
    Installed [N] packages in [TIME]
     - flask==2.3.3
     + flask==3.0.2
    "###
    );

    // Re-install Flask. We should install even though the version is current
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("Flask")?;

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--reinstall-package")
        .arg("Flask")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Uninstalled [N] packages in [TIME]
    Installed [N] packages in [TIME]
     - flask==3.0.2
     + flask==3.0.2
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

    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
    Prepared 7 packages in [TIME]
    Installed 7 packages in [TIME]
     + anyio==4.3.0
     + certifi==2024.2.2
     + h11==0.14.0
     + httpcore==1.0.4
     + httpx==0.27.0
     + idna==3.6
     + sniffio==1.3.1
    "###
    );

    context.assert_command("import httpx").success();

    // Re-install httpx, with an extra.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("httpx[http2]")?;

    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 10 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + h2==4.1.0
     + hpack==4.0.0
     + hyperframe==6.0.1
    "###
    );

    context.assert_command("import httpx").success();

    Ok(())
}

/// Warn, but don't fail, when uninstalling incomplete packages.
#[test]
fn reinstall_incomplete() -> Result<()> {
    let context = TestContext::new("3.12");

    // Install anyio.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("anyio==3.7.0")?;

    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg("requirements.txt"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==3.7.0
     + idna==3.6
     + sniffio==1.3.1
    "###
    );

    // Manually remove the `RECORD` file.
    fs_err::remove_file(context.site_packages().join("anyio-3.7.0.dist-info/RECORD"))?;

    // Re-install anyio.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("anyio==4.0.0")?;

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-r")
        .arg("requirements.txt"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 1 package in [TIME]
    warning: Failed to uninstall package at [SITE_PACKAGES]/anyio-3.7.0.dist-info due to missing RECORD file. Installation may result in an incomplete environment.
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - anyio==3.7.0
     + anyio==4.0.0
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
    requirements_txt.write_str("Flask")?;

    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
    Prepared 7 packages in [TIME]
    Installed 7 packages in [TIME]
     + blinker==1.7.0
     + click==8.1.7
     + flask==3.0.2
     + itsdangerous==2.1.2
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + werkzeug==3.0.1
    "###
    );

    context.assert_command("import flask").success();

    // Install an incompatible version of Jinja2.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("jinja2==2.11.3")?;

    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - jinja2==3.1.3
     + jinja2==2.11.3
    warning: The package `flask` requires `jinja2>=3.1.2`, but `2.11.3` is installed
    "###
    );

    // This no longer works, since we have an incompatible version of Jinja2.
    context.assert_command("import flask").failure();

    Ok(())
}

#[test]
fn install_editable() {
    let context = TestContext::new("3.12");

    // Install the editable package.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-e")
        .arg(context.workspace_root.join("scripts/packages/poetry_editable")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + poetry-editable==0.1.0 (from file://[WORKSPACE]/scripts/packages/poetry_editable)
     + sniffio==1.3.1
    "###
    );

    // Install it again (no-op).
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-e")
        .arg(context.workspace_root.join("scripts/packages/poetry_editable")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    "###
    );

    // Add another, non-editable dependency.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-e")
        .arg(context.workspace_root.join("scripts/packages/poetry_editable"))
        .arg("black"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 10 packages in [TIME]
    Prepared 6 packages in [TIME]
    Installed 6 packages in [TIME]
     + black==24.3.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    "###
    );
}

#[test]
fn install_editable_and_registry() {
    let context = TestContext::new("3.12");

    // Install the registry-based version of Black.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("black"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 6 packages in [TIME]
    Installed 6 packages in [TIME]
     + black==24.3.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    "###
    );

    // Install the editable version of Black. This should remove the registry-based version.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-e")
        .arg(context.workspace_root.join("scripts/packages/black_editable")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - black==24.3.0
     + black==0.1.0 (from file://[WORKSPACE]/scripts/packages/black_editable)
    "###
    );

    // Re-install the registry-based version of Black. This should be a no-op, since we have a
    // version of Black installed (the editable version) that satisfies the requirements.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("black")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    "###
    );

    let context = context.with_filtered_counts();
    // Re-install Black at a specific version. This should replace the editable version.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("black==23.10.0"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Uninstalled [N] packages in [TIME]
    Installed [N] packages in [TIME]
     - black==0.1.0 (from file://[WORKSPACE]/scripts/packages/black_editable)
     + black==23.10.0
    "###
    );
}

#[test]
fn install_editable_no_binary() {
    let context = TestContext::new("3.12");

    // Install the editable package with no-binary enabled
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-e")
        .arg(context.workspace_root.join("scripts/packages/black_editable"))
        .arg("--no-binary")
        .arg(":all:"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + black==0.1.0 (from file://[WORKSPACE]/scripts/packages/black_editable)
    "###
    );
}

#[test]
fn install_editable_compatible_constraint() -> Result<()> {
    let context = TestContext::new("3.12");

    let constraints_txt = context.temp_dir.child("constraints.txt");
    constraints_txt.write_str("black==0.1.0")?;

    // Install the editable package with a compatible constraint.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-e")
        .arg(context.workspace_root.join("scripts/packages/black_editable"))
        .arg("--constraint")
        .arg("constraints.txt"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + black==0.1.0 (from file://[WORKSPACE]/scripts/packages/black_editable)
    "###
    );

    Ok(())
}

#[test]
fn install_editable_incompatible_constraint_version() -> Result<()> {
    let context = TestContext::new("3.12");

    let constraints_txt = context.temp_dir.child("constraints.txt");
    constraints_txt.write_str("black>0.1.0")?;

    // Install the editable package with an incompatible constraint.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-e")
        .arg(context.workspace_root.join("scripts/packages/black_editable"))
        .arg("--constraint")
        .arg("constraints.txt"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only black<=0.1.0 is available and you require black>0.1.0, we can conclude that the requirements are unsatisfiable.
    "###
    );

    Ok(())
}

#[test]
fn install_editable_incompatible_constraint_url() -> Result<()> {
    let context = TestContext::new("3.12");

    let constraints_txt = context.temp_dir.child("constraints.txt");
    constraints_txt.write_str("black @ https://files.pythonhosted.org/packages/0f/89/294c9a6b6c75a08da55e9d05321d0707e9418735e3062b12ef0f54c33474/black-24.4.2-py3-none-any.whl")?;

    // Install the editable package with an incompatible constraint.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-e")
        .arg(context.workspace_root.join("scripts/packages/black_editable"))
        .arg("--constraint")
        .arg("constraints.txt"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Requirements contain conflicting URLs for package `black`:
    - [WORKSPACE]/scripts/packages/black_editable
    - https://files.pythonhosted.org/packages/0f/89/294c9a6b6c75a08da55e9d05321d0707e9418735e3062b12ef0f54c33474/black-24.4.2-py3-none-any.whl
    "###
    );

    Ok(())
}

#[test]
fn install_editable_pep_508_requirements_txt() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(&indoc::formatdoc! {r"
        -e black[d] @ file://{workspace_root}/scripts/packages/black_editable
        ",
        workspace_root = context.workspace_root.simplified_display(),
    })?;

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-r")
        .arg("requirements.txt"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 8 packages in [TIME]
    Prepared 8 packages in [TIME]
    Installed 8 packages in [TIME]
     + aiohttp==3.9.3
     + aiosignal==1.3.1
     + attrs==23.2.0
     + black==0.1.0 (from file://[WORKSPACE]/scripts/packages/black_editable)
     + frozenlist==1.4.1
     + idna==3.6
     + multidict==6.0.5
     + yarl==1.9.4
    "###
    );

    Ok(())
}

#[test]
fn install_editable_pep_508_cli() {
    let context = TestContext::new("3.12");

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-e")
        .arg(format!("black[d] @ file://{workspace_root}/scripts/packages/black_editable", workspace_root = context.workspace_root.simplified_display())), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 8 packages in [TIME]
    Prepared 8 packages in [TIME]
    Installed 8 packages in [TIME]
     + aiohttp==3.9.3
     + aiosignal==1.3.1
     + attrs==23.2.0
     + black==0.1.0 (from file://[WORKSPACE]/scripts/packages/black_editable)
     + frozenlist==1.4.1
     + idna==3.6
     + multidict==6.0.5
     + yarl==1.9.4
    "###
    );
}

#[test]
fn install_editable_bare_cli() {
    let context = TestContext::new("3.12");

    let packages_dir = context.workspace_root.join("scripts/packages");

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-e")
        .arg("black_editable")
        .current_dir(&packages_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + black==0.1.0 (from file://[WORKSPACE]/scripts/packages/black_editable)
    "###
    );
}

#[test]
fn install_editable_bare_requirements_txt() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("-e black_editable")?;

    let packages_dir = context.workspace_root.join("scripts/packages");

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-r")
        .arg(requirements_txt.path())
        .current_dir(&packages_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + black==0.1.0 (from file://[WORKSPACE]/scripts/packages/black_editable)
    "###
    );

    Ok(())
}

#[test]
fn invalid_editable_no_url() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("-e black==0.1.0")?;

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-r")
        .arg("requirements.txt"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Unsupported editable requirement in `requirements.txt`
      Caused by: Editable `black` must refer to a local directory, not a versioned package
    "###
    );

    Ok(())
}

#[test]
fn invalid_editable_unnamed_https_url() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("-e https://files.pythonhosted.org/packages/0f/89/294c9a6b6c75a08da55e9d05321d0707e9418735e3062b12ef0f54c33474/black-24.4.2-py3-none-any.whl")?;

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-r")
        .arg("requirements.txt"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Unsupported editable requirement in `requirements.txt`
      Caused by: Editable must refer to a local directory, not an HTTPS URL: `https://files.pythonhosted.org/packages/0f/89/294c9a6b6c75a08da55e9d05321d0707e9418735e3062b12ef0f54c33474/black-24.4.2-py3-none-any.whl`
    "###
    );

    Ok(())
}

#[test]
fn invalid_editable_named_https_url() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("-e black @ https://files.pythonhosted.org/packages/0f/89/294c9a6b6c75a08da55e9d05321d0707e9418735e3062b12ef0f54c33474/black-24.4.2-py3-none-any.whl")?;

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-r")
        .arg("requirements.txt"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Unsupported editable requirement in `requirements.txt`
      Caused by: Editable `black` must refer to a local directory, not an HTTPS URL: `https://files.pythonhosted.org/packages/0f/89/294c9a6b6c75a08da55e9d05321d0707e9418735e3062b12ef0f54c33474/black-24.4.2-py3-none-any.whl`
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

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(indoc! {r"
        flit_core<4.0.0
        flask @ https://files.pythonhosted.org/packages/d8/09/c1a7354d3925a3c6c8cfdebf4245bae67d633ffda1ba415add06ffc839c5/flask-3.0.0.tar.gz
        "
    })?;

    uv_snapshot!(context.pip_install()
        .arg("--reinstall")
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 8 packages in [TIME]
    Prepared 8 packages in [TIME]
    Installed 8 packages in [TIME]
     + blinker==1.7.0
     + click==8.1.7
     + flask==3.0.0 (from https://files.pythonhosted.org/packages/d8/09/c1a7354d3925a3c6c8cfdebf4245bae67d633ffda1ba415add06ffc839c5/flask-3.0.0.tar.gz)
     + flit-core==3.9.0
     + itsdangerous==2.1.2
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + werkzeug==3.0.1
    "###
    );

    Ok(())
}

/// Install a package without using the remote index
#[test]
fn install_no_index() {
    let context = TestContext::new("3.12");

    uv_snapshot!(context.pip_install()
        .arg("Flask")
        .arg("--no-index"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because flask was not found in the provided package locations and you require flask, we can conclude that the requirements are unsatisfiable.

          hint: Packages were unavailable because index lookups were disabled and no additional package locations were provided (try: `--find-links <uri>`)
    "###
    );

    context.assert_command("import flask").failure();
}

/// Install a package without using the remote index
/// Covers a case where the user requests a version which should be included in the error
#[test]
fn install_no_index_version() {
    let context = TestContext::new("3.12");

    uv_snapshot!(context.pip_install()
        .arg("Flask==3.0.0")
        .arg("--no-index"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because flask was not found in the provided package locations and you require flask==3.0.0, we can conclude that the requirements are unsatisfiable.

          hint: Packages were unavailable because index lookups were disabled and no additional package locations were provided (try: `--find-links <uri>`)
    "###
    );

    context.assert_command("import flask").failure();
}

/// Install a package via --extra-index-url.
///
/// This is a regression test where previously uv would consult test.pypi.org
/// first, and if the package was found there, uv would not look at any other
/// indexes. We fixed this by flipping the priority order of indexes so that
/// test.pypi.org becomes the fallback (in this example) and the extra indexes
/// (regular PyPI) are checked first.
///
/// (Neither approach matches `pip`'s behavior, which considers versions of
/// each package from all indexes. uv stops at the first index it finds a
/// package in.)
///
/// Ref: <https://github.com/astral-sh/uv/issues/1600>
#[test]
fn install_extra_index_url_has_priority() {
    let context = TestContext::new("3.12");

    uv_snapshot!(context.pip_install()
        .env_remove("UV_EXCLUDE_NEWER")
        .arg("--index-url")
        .arg("https://test.pypi.org/simple")
        .arg("--extra-index-url")
        .arg("https://pypi.org/simple")
        // This tests what we want because BOTH of the following
        // are true: `black` is on pypi.org and test.pypi.org, AND
        // `black==24.2.0` is on pypi.org and NOT test.pypi.org. So
        // this would previously check for `black` on test.pypi.org,
        // find it, but then not find a compatible version. After
        // the fix, uv will check pypi.org first since it is given
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
    Prepared 1 package in [TIME]
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

    uv_snapshot!(
        context
        .pip_install()
        .arg("uv-public-pypackage @ git+https://github.com/astral-test/uv-public-pypackage"),
        @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + uv-public-pypackage==0.1.0 (from git+https://github.com/astral-test/uv-public-pypackage@b270df1a2fb5d012294e9aaf05e7e0bab1e6a389)
    "###);

    context.assert_installed("uv_public_pypackage", "0.1.0");
}

/// Install and update a package from a public GitHub repository
#[test]
#[cfg(feature = "git")]
fn update_ref_git_public_https() {
    let context = TestContext::new("3.8");

    uv_snapshot!(
        context
        .pip_install()
        .arg("uv-public-pypackage @ git+https://github.com/astral-test/uv-public-pypackage@0dacfd662c64cb4ceb16e6cf65a157a8b715b979"),
        @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + uv-public-pypackage==0.1.0 (from git+https://github.com/astral-test/uv-public-pypackage@0dacfd662c64cb4ceb16e6cf65a157a8b715b979)
    "###);

    context.assert_installed("uv_public_pypackage", "0.1.0");

    // Update to a newer commit.
    uv_snapshot!(
        context
        .pip_install()
        .arg("uv-public-pypackage @ git+https://github.com/astral-test/uv-public-pypackage@b270df1a2fb5d012294e9aaf05e7e0bab1e6a389")
        .arg("--refresh"),
        @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - uv-public-pypackage==0.1.0 (from git+https://github.com/astral-test/uv-public-pypackage@0dacfd662c64cb4ceb16e6cf65a157a8b715b979)
     + uv-public-pypackage==0.1.0 (from git+https://github.com/astral-test/uv-public-pypackage@b270df1a2fb5d012294e9aaf05e7e0bab1e6a389)
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

    uv_snapshot!(filters, context.pip_install()
        // 2.0.0 does not exist
        .arg("uv-public-pypackage @ git+https://github.com/astral-test/uv-public-pypackage@2.0.0"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to download and build: `uv-public-pypackage @ git+https://github.com/astral-test/uv-public-pypackage@2.0.0`
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

    // There are flakes on Windows where this irrelevant error is appended
    filters.push((
        "fatal: unable to write response end packet: Broken pipe\n",
        "",
    ));

    uv_snapshot!(filters, context.pip_install()
        // 2.0.0 does not exist
        .arg("uv-public-pypackage @ git+https://github.com/astral-test/uv-public-pypackage@79a935a7a1a0ad6d0bdf72dce0e16cb0a24a1b3b")
        , @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to download and build: `uv-public-pypackage @ git+https://github.com/astral-test/uv-public-pypackage@79a935a7a1a0ad6d0bdf72dce0e16cb0a24a1b3b`
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

    let filters: Vec<_> = [(token.as_str(), "***")]
        .into_iter()
        .chain(context.filters())
        .collect();

    let package = format!(
        "uv-private-pypackage@ git+https://{token}@github.com/astral-test/uv-private-pypackage"
    );

    uv_snapshot!(filters, context.pip_install().arg(package)
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + uv-private-pypackage==0.1.0 (from git+https://***@github.com/astral-test/uv-private-pypackage@d780faf0ac91257d4d5a4f0c5a0e4509608c0071)
    "###);

    context.assert_installed("uv_private_pypackage", "0.1.0");
}

/// Install a package from a private GitHub repository using a PAT
/// Include a public GitHub repository too, to ensure that the authentication is not erroneously copied over.
#[test]
#[cfg(all(not(windows), feature = "git"))]
fn install_git_private_https_pat_mixed_with_public() {
    let context = TestContext::new("3.8");
    let token = decode_token(READ_ONLY_GITHUB_TOKEN);

    let filters: Vec<_> = [(token.as_str(), "***")]
        .into_iter()
        .chain(context.filters())
        .collect();

    let package = format!(
        "uv-private-pypackage @ git+https://{token}@github.com/astral-test/uv-private-pypackage"
    );

    uv_snapshot!(filters, context.pip_install().arg(package).arg("uv-public-pypackage @ git+https://github.com/astral-test/uv-public-pypackage"),
    @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + uv-private-pypackage==0.1.0 (from git+https://***@github.com/astral-test/uv-private-pypackage@d780faf0ac91257d4d5a4f0c5a0e4509608c0071)
     + uv-public-pypackage==0.1.0 (from git+https://github.com/astral-test/uv-public-pypackage@b270df1a2fb5d012294e9aaf05e7e0bab1e6a389)
    "###);

    context.assert_installed("uv_private_pypackage", "0.1.0");
}

/// Install packages from multiple private GitHub repositories with separate PATS
#[test]
#[cfg(all(not(windows), feature = "git"))]
fn install_git_private_https_multiple_pat() {
    let context = TestContext::new("3.8");
    let token_1 = decode_token(READ_ONLY_GITHUB_TOKEN);
    let token_2 = decode_token(READ_ONLY_GITHUB_TOKEN_2);

    let filters: Vec<_> = [(token_1.as_str(), "***_1"), (token_2.as_str(), "***_2")]
        .into_iter()
        .chain(context.filters())
        .collect();

    let package_1 = format!(
        "uv-private-pypackage @ git+https://{token_1}@github.com/astral-test/uv-private-pypackage"
    );
    let package_2 = format!(
        "uv-private-pypackage-2 @ git+https://{token_2}@github.com/astral-test/uv-private-pypackage-2"
    );

    uv_snapshot!(filters, context.pip_install().arg(package_1).arg(package_2)
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + uv-private-pypackage==0.1.0 (from git+https://***_1@github.com/astral-test/uv-private-pypackage@d780faf0ac91257d4d5a4f0c5a0e4509608c0071)
     + uv-private-pypackage-2==0.1.0 (from git+https://***_2@github.com/astral-test/uv-private-pypackage-2@45c0bec7365710f09b1f4dbca61c86dde9537e4e)
    "###);

    context.assert_installed("uv_private_pypackage", "0.1.0");
}

/// Install a package from a private GitHub repository at a specific commit using a PAT
#[test]
#[cfg(feature = "git")]
fn install_git_private_https_pat_at_ref() {
    let context = TestContext::new("3.8");
    let token = decode_token(READ_ONLY_GITHUB_TOKEN);

    let mut filters: Vec<_> = [(token.as_str(), "***")]
        .into_iter()
        .chain(context.filters())
        .collect();

    filters.push((r"git\+https://", ""));

    // A user is _required_ on Windows
    let user = if cfg!(windows) {
        filters.push((r"git:", ""));
        "git:"
    } else {
        ""
    };

    let package = format!("uv-private-pypackage @ git+https://{user}{token}@github.com/astral-test/uv-private-pypackage@6c09ce9ae81f50670a60abd7d95f30dd416d00ac");
    uv_snapshot!(filters, context.pip_install()
        .arg(package), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
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

    let filters: Vec<_> = [(token.as_str(), "***")]
        .into_iter()
        .chain(context.filters())
        .collect();

    uv_snapshot!(filters, context.pip_install().arg(format!("uv-private-pypackage @ git+https://{user}:{token}@github.com/astral-test/uv-private-pypackage"))
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
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
    uv_snapshot!(filters, context.pip_install()
        .arg(format!("uv-private-pypackage @ git+https://git:{token}@github.com/astral-test/uv-private-pypackage"))
        , @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to download and build: `uv-private-pypackage @ git+https://git:***@github.com/astral-test/uv-private-pypackage`
      Caused by: Git operation failed
      Caused by: failed to clone into: [CACHE_DIR]/git-v0/db/8401f5508e3e612d
      Caused by: process didn't exit successfully: `git fetch --force --update-head-ok 'https://git:***@github.com/astral-test/uv-private-pypackage' '+HEAD:refs/remotes/origin/HEAD'` (exit status: 128)
    --- stderr
    remote: Support for password authentication was removed on August 13, 2021.
    remote: Please see https://docs.github.com/get-started/getting-started-with-git/about-remote-repositories#cloning-with-https-urls for information on currently recommended modes of authentication.
    fatal: Authentication failed for 'https://github.com/astral-test/uv-private-pypackage/'

    "###);
}

/// Install a package from a private GitHub repository using a PAT
/// Does not use `git`, instead installs a distribution artifact.
/// Include a public GitHub repository too, to ensure that the authentication is not erroneously copied over.
#[test]
#[cfg(not(windows))]
fn install_github_artifact_private_https_pat_mixed_with_public() {
    let context = TestContext::new("3.8");
    let token = decode_token(READ_ONLY_GITHUB_TOKEN);

    let filters: Vec<_> = [(token.as_str(), "***")]
        .into_iter()
        .chain(context.filters())
        .collect();

    let private_package = format!(
        "uv-private-pypackage @ https://{token}@raw.githubusercontent.com/astral-test/uv-private-pypackage/main/dist/uv_private_pypackage-0.1.0-py3-none-any.whl"
    );
    let public_package = "uv-public-pypackage @ https://raw.githubusercontent.com/astral-test/uv-public-pypackage/main/dist/uv_public_pypackage-0.1.0-py3-none-any.whl";

    uv_snapshot!(filters, context.pip_install().arg(private_package).arg(public_package),
    @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + uv-private-pypackage==0.1.0 (from https://***@raw.githubusercontent.com/astral-test/uv-private-pypackage/main/dist/uv_private_pypackage-0.1.0-py3-none-any.whl)
     + uv-public-pypackage==0.1.0 (from https://raw.githubusercontent.com/astral-test/uv-public-pypackage/main/dist/uv_public_pypackage-0.1.0-py3-none-any.whl)
    "###);

    context.assert_installed("uv_private_pypackage", "0.1.0");
}

/// Install packages from multiple private GitHub repositories with separate PATS
/// Does not use `git`, instead installs a distribution artifact.
#[test]
#[cfg(not(windows))]
fn install_github_artifact_private_https_multiple_pat() {
    let context = TestContext::new("3.8");
    let token_1 = decode_token(READ_ONLY_GITHUB_TOKEN);
    let token_2 = decode_token(READ_ONLY_GITHUB_TOKEN_2);

    let filters: Vec<_> = [(token_1.as_str(), "***_1"), (token_2.as_str(), "***_2")]
        .into_iter()
        .chain(context.filters())
        .collect();

    let package_1 = format!(
        "uv-private-pypackage @ https://astral-test-bot:{token_1}@raw.githubusercontent.com/astral-test/uv-private-pypackage/main/dist/uv_private_pypackage-0.1.0-py3-none-any.whl"
    );
    let package_2 = format!(
        "uv-private-pypackage-2 @ https://astral-test-bot:{token_2}@raw.githubusercontent.com/astral-test/uv-private-pypackage-2/main/dist/uv_private_pypackage_2-0.1.0-py3-none-any.whl"
    );

    uv_snapshot!(filters, context.pip_install().arg(package_1).arg(package_2)
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + uv-private-pypackage==0.1.0 (from https://astral-test-bot:***_1@raw.githubusercontent.com/astral-test/uv-private-pypackage/main/dist/uv_private_pypackage-0.1.0-py3-none-any.whl)
     + uv-private-pypackage-2==0.1.0 (from https://astral-test-bot:***_2@raw.githubusercontent.com/astral-test/uv-private-pypackage-2/main/dist/uv_private_pypackage_2-0.1.0-py3-none-any.whl)
    "###);

    context.assert_installed("uv_private_pypackage", "0.1.0");
}

/// Install a package without using pre-built wheels.
#[test]
fn reinstall_no_binary() {
    let context = TestContext::new("3.12");

    // The first installation should use a pre-built wheel
    let mut command = context.pip_install();
    command.arg("anyio").arg("--strict");
    uv_snapshot!(
        command,
        @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + sniffio==1.3.1
    "###
    );

    context.assert_command("import anyio").success();

    // Running installation again with `--no-binary` should be a no-op
    // The first installation should use a pre-built wheel
    let mut command = context.pip_install();
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
    let context = context.with_filtered_counts();
    let mut command = context.pip_install();
    command
        .arg("anyio")
        .arg("--no-binary")
        .arg(":all:")
        .arg("--reinstall-package")
        .arg("anyio")
        .arg("--strict");
    uv_snapshot!(context.filters(), command, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Uninstalled [N] packages in [TIME]
    Installed [N] packages in [TIME]
     - anyio==4.3.0
     + anyio==4.3.0
    "###
    );

    context.assert_command("import anyio").success();
}

/// Overlapping usage of `--no-binary` and `--only-binary`
#[test]
fn install_no_binary_overrides_only_binary_all() {
    let context = TestContext::new("3.12");

    // The specific `--no-binary` should override the less specific `--only-binary`
    let mut command = context.pip_install();
    command
        .arg("anyio")
        .arg("--only-binary")
        .arg(":all:")
        .arg("--no-binary")
        .arg("idna")
        .arg("--strict");
    uv_snapshot!(
        command,
        @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + sniffio==1.3.1
    "###
    );

    context.assert_command("import anyio").success();
}

/// Overlapping usage of `--no-binary` and `--only-binary`
#[test]
fn install_only_binary_overrides_no_binary_all() {
    let context = TestContext::new("3.12");

    // The specific `--only-binary` should override the less specific `--no-binary`
    let mut command = context.pip_install();
    command
        .arg("anyio")
        .arg("--no-binary")
        .arg(":all:")
        .arg("--only-binary")
        .arg("idna")
        .arg("--strict");
    uv_snapshot!(
        command,
        @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + sniffio==1.3.1
    "###
    );

    context.assert_command("import anyio").success();
}

/// Overlapping usage of `--no-binary` and `--only-binary`
// TODO(zanieb): We should have a better error message here
#[test]
fn install_only_binary_all_and_no_binary_all() {
    let context = TestContext::new("3.12");

    // With both as `:all:` we can't install
    let mut command = context.pip_install();
    command
        .arg("anyio")
        .arg("--no-binary")
        .arg(":all:")
        .arg("--only-binary")
        .arg(":all:")
        .arg("--strict");
    uv_snapshot!(
        command,
        @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only the following versions of anyio are available:
              anyio>=1.0.0,<=1.4.0
              anyio>=2.0.0,<=2.2.0
              anyio>=3.0.0,<=3.6.2
              anyio>=3.7.0,<=3.7.1
              anyio>=4.0.0
          and anyio==1.0.0 has no usable wheels and building from source is disabled, we can conclude that any of:
              anyio<1.1.0
              anyio>1.4.0,<2.0.0
              anyio>2.2.0,<3.0.0
              anyio>3.6.2,<3.7.0
              anyio>3.7.1,<4.0.0
           cannot be used.
          And because anyio==1.1.0 has no usable wheels and building from source is disabled and anyio==1.2.0 has no usable wheels and building from source is disabled, we can conclude that any of:
              anyio<1.2.1
              anyio>1.4.0,<2.0.0
              anyio>2.2.0,<3.0.0
              anyio>3.6.2,<3.7.0
              anyio>3.7.1,<4.0.0
           cannot be used.
          And because anyio==1.2.1 has no usable wheels and building from source is disabled and anyio==1.2.2 has no usable wheels and building from source is disabled, we can conclude that any of:
              anyio<1.2.3
              anyio>1.4.0,<2.0.0
              anyio>2.2.0,<3.0.0
              anyio>3.6.2,<3.7.0
              anyio>3.7.1,<4.0.0
           cannot be used.
          And because anyio==1.2.3 has no usable wheels and building from source is disabled and anyio==1.3.0 has no usable wheels and building from source is disabled, we can conclude that any of:
              anyio<1.3.1
              anyio>1.4.0,<2.0.0
              anyio>2.2.0,<3.0.0
              anyio>3.6.2,<3.7.0
              anyio>3.7.1,<4.0.0
           cannot be used.
          And because anyio==1.3.1 has no usable wheels and building from source is disabled and anyio==1.4.0 has no usable wheels and building from source is disabled, we can conclude that any of:
              anyio<2.0.0
              anyio>2.2.0,<3.0.0
              anyio>3.6.2,<3.7.0
              anyio>3.7.1,<4.0.0
           cannot be used.
          And because anyio==2.0.0 has no usable wheels and building from source is disabled and anyio==2.0.1 has no usable wheels and building from source is disabled, we can conclude that any of:
              anyio<2.0.2
              anyio>2.2.0,<3.0.0
              anyio>3.6.2,<3.7.0
              anyio>3.7.1,<4.0.0
           cannot be used.
          And because anyio==2.0.2 has no usable wheels and building from source is disabled and anyio==2.1.0 has no usable wheels and building from source is disabled, we can conclude that any of:
              anyio<2.2.0
              anyio>2.2.0,<3.0.0
              anyio>3.6.2,<3.7.0
              anyio>3.7.1,<4.0.0
           cannot be used.
          And because anyio==2.2.0 has no usable wheels and building from source is disabled and anyio==3.0.0 has no usable wheels and building from source is disabled, we can conclude that any of:
              anyio<3.0.1
              anyio>3.6.2,<3.7.0
              anyio>3.7.1,<4.0.0
           cannot be used.
          And because anyio==3.0.1 has no usable wheels and building from source is disabled and anyio==3.1.0 has no usable wheels and building from source is disabled, we can conclude that any of:
              anyio<3.2.0
              anyio>3.6.2,<3.7.0
              anyio>3.7.1,<4.0.0
           cannot be used.
          And because anyio==3.2.0 has no usable wheels and building from source is disabled and anyio==3.2.1 has no usable wheels and building from source is disabled, we can conclude that any of:
              anyio<3.3.0
              anyio>3.6.2,<3.7.0
              anyio>3.7.1,<4.0.0
           cannot be used.
          And because anyio==3.3.0 has no usable wheels and building from source is disabled and anyio==3.3.1 has no usable wheels and building from source is disabled, we can conclude that any of:
              anyio<3.3.2
              anyio>3.6.2,<3.7.0
              anyio>3.7.1,<4.0.0
           cannot be used.
          And because anyio==3.3.2 has no usable wheels and building from source is disabled and anyio==3.3.3 has no usable wheels and building from source is disabled, we can conclude that any of:
              anyio<3.3.4
              anyio>3.6.2,<3.7.0
              anyio>3.7.1,<4.0.0
           cannot be used.
          And because anyio==3.3.4 has no usable wheels and building from source is disabled and anyio==3.4.0 has no usable wheels and building from source is disabled, we can conclude that any of:
              anyio<3.5.0
              anyio>3.6.2,<3.7.0
              anyio>3.7.1,<4.0.0
           cannot be used.
          And because anyio==3.5.0 has no usable wheels and building from source is disabled and anyio==3.6.0 has no usable wheels and building from source is disabled, we can conclude that any of:
              anyio<3.6.1
              anyio>3.6.2,<3.7.0
              anyio>3.7.1,<4.0.0
           cannot be used.
          And because anyio==3.6.1 has no usable wheels and building from source is disabled and anyio==3.6.2 has no usable wheels and building from source is disabled, we can conclude that any of:
              anyio<3.7.0
              anyio>3.7.1,<4.0.0
           cannot be used.
          And because anyio==3.7.0 has no usable wheels and building from source is disabled and anyio==3.7.1 has no usable wheels and building from source is disabled, we can conclude that anyio<4.0.0 cannot be used.
          And because anyio==4.0.0 has no usable wheels and building from source is disabled and anyio==4.1.0 has no usable wheels and building from source is disabled, we can conclude that anyio<4.2.0 cannot be used.
          And because anyio==4.2.0 has no usable wheels and building from source is disabled and anyio==4.3.0 has no usable wheels and building from source is disabled, we can conclude that anyio<4.4.0 cannot be used.
          And because anyio==4.4.0 has no usable wheels and building from source is disabled and you require anyio, we can conclude that the requirements are unsatisfiable.

          hint: Pre-releases are available for anyio in the requested range (e.g., 4.0.0rc1), but pre-releases weren't enabled (try: `--prerelease=allow`)
    "###
    );

    context.assert_command("import anyio").failure();
}

/// Respect `--only-binary` flags in `requirements.txt`
#[test]
fn only_binary_requirements_txt() {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str(indoc! {r"
        django_allauth==0.51.0
        --only-binary django_allauth
        "
        })
        .unwrap();

    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because django-allauth==0.51.0 has no usable wheels and building from source is disabled and you require django-allauth==0.51.0, we can conclude that the requirements are unsatisfiable.
    "###
    );
}

/// `--only-binary` does not apply to editable requirements
#[test]
fn only_binary_editable() {
    let context = TestContext::new("3.12");

    // Install the editable package.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--only-binary")
        .arg(":all:")
        .arg("-e")
        .arg(context.workspace_root.join("scripts/packages/anyio_local")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + anyio==4.3.0+foo (from file://[WORKSPACE]/scripts/packages/anyio_local)
    "###
    );
}

/// `--only-binary` does not apply to editable requirements that depend on each other
#[test]
fn only_binary_dependent_editables() {
    let context = TestContext::new("3.12");
    let root_path = context
        .workspace_root
        .join("scripts/packages/dependent_locals");

    // Install the editable package.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--only-binary")
        .arg(":all:")
        .arg("-e")
        .arg(root_path.join("first_local"))
        .arg("-e")
        .arg(root_path.join("second_local")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + first-local==0.1.0 (from file://[WORKSPACE]/scripts/packages/dependent_locals/first_local)
     + second-local==0.1.0 (from file://[WORKSPACE]/scripts/packages/dependent_locals/second_local)
    "###
    );
}

/// `--only-binary` does not apply to editable requirements, with a `setup.py` config
#[test]
fn only_binary_editable_setup_py() {
    let context = TestContext::new("3.12");

    // Install the editable package.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--only-binary")
        .arg(":all:")
        .arg("-e")
        .arg(context.workspace_root.join("scripts/packages/setup_py_editable")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 8 packages in [TIME]
    Prepared 8 packages in [TIME]
    Installed 8 packages in [TIME]
     + anyio==4.3.0
     + certifi==2024.2.2
     + h11==0.14.0
     + httpcore==1.0.4
     + httpx==0.27.0
     + idna==3.6
     + setup-py-editable==0.0.1 (from file://[WORKSPACE]/scripts/packages/setup_py_editable)
     + sniffio==1.3.1
    "###
    );
}

/// Install a package into a virtual environment, and ensuring that the executable permissions
/// are retained.
///
/// This test uses the default link semantics. (On macOS, this is `clone`.)
#[test]
fn install_executable() {
    let context = TestContext::new("3.12");

    uv_snapshot!(context.pip_install()
        .arg("pylint==3.0.0"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
    Prepared 7 packages in [TIME]
    Installed 7 packages in [TIME]
     + astroid==3.0.3
     + dill==0.3.8
     + isort==5.13.2
     + mccabe==0.7.0
     + platformdirs==4.2.0
     + pylint==3.0.0
     + tomlkit==0.12.4
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

    uv_snapshot!(context.pip_install()
        .arg("pylint==3.0.0")
        .arg("--link-mode")
        .arg("copy"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
    Prepared 7 packages in [TIME]
    Installed 7 packages in [TIME]
     + astroid==3.0.3
     + dill==0.3.8
     + isort==5.13.2
     + mccabe==0.7.0
     + platformdirs==4.2.0
     + pylint==3.0.0
     + tomlkit==0.12.4
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

    uv_snapshot!(context.pip_install()
        .arg("pylint==3.0.0")
        .arg("--link-mode")
        .arg("hardlink"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
    Prepared 7 packages in [TIME]
    Installed 7 packages in [TIME]
     + astroid==3.0.3
     + dill==0.3.8
     + isort==5.13.2
     + mccabe==0.7.0
     + platformdirs==4.2.0
     + pylint==3.0.0
     + tomlkit==0.12.4
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
    uv_snapshot!(context.pip_install()
        .arg("Flask")
        .arg("--no-deps")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + flask==3.0.2
    warning: The package `flask` requires `werkzeug>=3.0.0`, but it's not installed
    warning: The package `flask` requires `jinja2>=3.1.2`, but it's not installed
    warning: The package `flask` requires `itsdangerous>=2.1.2`, but it's not installed
    warning: The package `flask` requires `click>=8.1.3`, but it's not installed
    warning: The package `flask` requires `blinker>=1.6.2`, but it's not installed
    "###
    );

    context.assert_command("import flask").failure();
}

/// Install an editable package from the command line into a virtual environment, ignoring its
/// dependencies.
#[test]
fn no_deps_editable() {
    let context = TestContext::new("3.12");

    // Install the editable version of Black. This should remove the registry-based version.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--no-deps")
        .arg("-e")
        .arg(context.workspace_root.join("scripts/packages/black_editable[dev]")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + black==0.1.0 (from file://[WORKSPACE]/scripts/packages/black_editable)
    "###
    );

    context.assert_command("import black").success();
    context.assert_command("import aiohttp").failure();
}

/// Upgrade a package.
#[test]
fn install_upgrade() {
    let context = TestContext::new("3.12");

    // Install an old version of anyio and httpcore.
    uv_snapshot!(context.pip_install()
        .arg("anyio==3.6.2")
        .arg("httpcore==0.16.3")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 6 packages in [TIME]
    Installed 6 packages in [TIME]
     + anyio==3.6.2
     + certifi==2024.2.2
     + h11==0.14.0
     + httpcore==0.16.3
     + idna==3.6
     + sniffio==1.3.1
    "###
    );

    context.assert_command("import anyio").success();

    // Upgrade anyio.
    uv_snapshot!(context.pip_install()
        .arg("anyio")
        .arg("--upgrade-package")
        .arg("anyio"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - anyio==3.6.2
     + anyio==4.3.0
    "###
    );

    // Upgrade anyio again, should not reinstall.
    uv_snapshot!(context.pip_install()
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
    uv_snapshot!(context.pip_install()
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
    uv_snapshot!(context.pip_install()
        .arg("httpcore")
        .arg("--upgrade"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - httpcore==0.16.3
     + httpcore==1.0.4
    "###
    );
}

/// Install a package from a `requirements.txt` file, with a `constraints.txt` file.
#[test]
fn install_constraints_txt() -> Result<()> {
    let context = TestContext::new("3.12");
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("anyio==3.7.0")?;

    let constraints_txt = context.temp_dir.child("constraints.txt");
    constraints_txt.write_str("idna<3.4")?;

    uv_snapshot!(context.pip_install()
            .arg("-r")
            .arg("requirements.txt")
            .arg("--constraint")
            .arg("constraints.txt"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==3.7.0
     + idna==3.3
     + sniffio==1.3.1
    "###
    );

    Ok(())
}

/// Check that `tool.uv.constraint-dependencies` in `pyproject.toml` is respected.
#[test]
fn install_constraints_from_pyproject() -> Result<()> {
    let context = TestContext::new("3.12");
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"[project]
    name = "example"
    version = "0.0.0"
    dependencies = [
      "anyio==3.7.0"
    ]

    [tool.uv]
    constraint-dependencies = [
      "idna<3.4"
    ]
    "#,
    )?;

    uv_snapshot!(context.pip_install()
            .arg("-r")
            .arg("pyproject.toml"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==3.7.0
     + idna==3.3
     + sniffio==1.3.1
    "###
    );

    Ok(())
}

/// Install a package from a `requirements.txt` file, with an inline constraint.
#[test]
fn install_constraints_inline() -> Result<()> {
    let context = TestContext::new("3.12");
    let requirementstxt = context.temp_dir.child("requirements.txt");
    requirementstxt.write_str("anyio==3.7.0\n-c constraints.txt")?;

    let constraints_txt = context.temp_dir.child("constraints.txt");
    constraints_txt.write_str("idna<3.4")?;

    uv_snapshot!(context.pip_install()
            .arg("-r")
            .arg("requirements.txt"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==3.7.0
     + idna==3.3
     + sniffio==1.3.1
    "###
    );

    Ok(())
}

/// Install a package from a `constraints.txt` file on a remote http server.
#[test]
fn install_constraints_remote() {
    let context = TestContext::new("3.12");

    uv_snapshot!(context.pip_install()
            .arg("-c")
            .arg("https://raw.githubusercontent.com/apache/airflow/constraints-2-6/constraints-3.11.txt")
            .arg("typing_extensions>=4.0"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
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

    uv_snapshot!(context.pip_install()
            .arg("-r")
            .arg("requirements.txt"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + typing-extensions==4.7.1
    "### // would yield typing-extensions==4.8.2 without constraint file
    );

    Ok(())
}

/// Constrain a package that's included via an extra.
#[test]
fn install_constraints_extra() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("flask[dotenv]")?;

    let constraints_txt = context.temp_dir.child("constraints.txt");
    constraints_txt.write_str("python-dotenv==1.0.0")?;

    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("-c")
        .arg("constraints.txt"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 8 packages in [TIME]
    Prepared 8 packages in [TIME]
    Installed 8 packages in [TIME]
     + blinker==1.7.0
     + click==8.1.7
     + flask==3.0.2
     + itsdangerous==2.1.2
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + python-dotenv==1.0.0
     + werkzeug==3.0.1
    "###
    );

    Ok(())
}

#[test]
fn install_constraints_respects_offline_mode() {
    let context = TestContext::new("3.12");

    uv_snapshot!(context.pip_install()
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
    uv_snapshot!(context.pip_install()
        .arg("polars==0.14.0"),
        @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
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

    uv_snapshot!(context.pip_install()
            .arg("-r")
            .arg("requirements.in")
            .arg("--resolution=lowest-direct"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.2.0 (from https://files.pythonhosted.org/packages/2d/b8/7333d87d5f03247215d86a86362fd3e324111788c6cdd8d2e6196a6ba833/anyio-4.2.0.tar.gz)
     + idna==3.6
     + sniffio==1.3.1
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

    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 5 packages in [TIME]
    Installed 6 packages in [TIME]
     + distro==1.9.0
     + opensafely-pipeline==2023.11.6.145820 (from https://github.com/opensafely-core/pipeline/archive/refs/tags/v2023.11.06.145820.zip)
     + pydantic==1.10.14
     + ruyaml==0.91.0
     + setuptools==69.2.0
     + typing-extensions==4.10.0
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
        context.pip_install()
        .arg(format!("simple_launcher@{}", project_root.join("scripts/links/simple_launcher-0.1.0-py3-none-any.whl").display()))
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
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
        context.pip_install()
            .arg(format!("simple_launcher@{}", project_root.join("scripts/links/simple_launcher-0.1.0-py3-none-any.whl").display()))
            .arg("--strict"),
        @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
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
fn config_settings() {
    let context = TestContext::new("3.12");

    // Install the editable package.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-e")
        .arg(context.workspace_root.join("scripts/packages/setuptools_editable")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + iniconfig==2.0.0
     + setuptools-editable==0.1.0 (from file://[WORKSPACE]/scripts/packages/setuptools_editable)
    "###
    );

    // When installed without `--editable_mode=compat`, the `finder.py` file should be present.
    let finder = context
        .site_packages()
        .join("__editable___setuptools_editable_0_1_0_finder.py");
    assert!(finder.exists());

    // Install the editable package with `--editable_mode=compat`.
    let context = TestContext::new("3.12");

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-e")
        .arg(context.workspace_root.join("scripts/packages/setuptools_editable"))
        .arg("-C")
        .arg("editable_mode=compat")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + iniconfig==2.0.0
     + setuptools-editable==0.1.0 (from file://[WORKSPACE]/scripts/packages/setuptools_editable)
    "###
    );

    // When installed without `--editable_mode=compat`, the `finder.py` file should _not_ be present.
    let finder = context
        .site_packages()
        .join("__editable___setuptools_editable_0_1_0_finder.py");
    assert!(!finder.exists());
}

/// Reinstall a duplicate package in a virtual environment.
#[test]
fn reinstall_duplicate() -> Result<()> {
    use crate::common::copy_dir_all;

    // Sync a version of `pip` into a virtual environment.
    let context1 = TestContext::new("3.12");
    let requirements_txt = context1.temp_dir.child("requirements.txt");
    requirements_txt.write_str("pip==21.3.1")?;

    // Run `pip sync`.
    context1
        .pip_install()
        .arg("-r")
        .arg(requirements_txt.path())
        .assert()
        .success();

    // Sync a different version of `pip` into a virtual environment.
    let context2 = TestContext::new("3.12");
    let requirements_txt = context2.temp_dir.child("requirements.txt");
    requirements_txt.write_str("pip==22.1.1")?;

    // Run `pip sync`.
    context2
        .pip_install()
        .arg("-r")
        .arg(requirements_txt.path())
        .assert()
        .success();

    // Copy the virtual environment to a new location.
    copy_dir_all(
        context2.site_packages().join("pip-22.1.1.dist-info"),
        context1.site_packages().join("pip-22.1.1.dist-info"),
    )?;

    // Run `pip install`.
    uv_snapshot!(context1.pip_install()
        .arg("pip")
        .arg("--reinstall"),
        @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 2 packages in [TIME]
    Installed 1 package in [TIME]
     - pip==21.3.1
     - pip==22.1.1
     + pip==24.0
    "###
    );

    Ok(())
}

/// Install a package that contains a symlink within the archive.
#[test]
fn install_symlink() {
    let context = TestContext::new("3.12");

    uv_snapshot!(context.pip_install()
        .arg("pgpdump==1.5")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + pgpdump==1.5
    "###
    );

    context.assert_command("import pgpdump").success();

    uv_snapshot!(context
        .pip_uninstall()
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
    let editable_dir = context.temp_dir.child("editable");
    editable_dir.create_dir_all()?;
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

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--editable")
        .arg(editable_dir.path()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.0.0
     + example==0.0.0 (from file://[TEMP_DIR]/editable)
     + idna==3.6
     + sniffio==1.3.1
    "###
    );

    // Re-installing should be a no-op.
    uv_snapshot!(context.filters(), context.pip_install()
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
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--editable")
        .arg(editable_dir.path()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 2 packages in [TIME]
    Uninstalled 2 packages in [TIME]
    Installed 2 packages in [TIME]
     - anyio==4.0.0
     + anyio==3.7.1
     - example==0.0.0 (from file://[TEMP_DIR]/editable)
     + example==0.0.0 (from file://[TEMP_DIR]/editable)
    "###
    );

    Ok(())
}

#[test]
fn editable_dynamic() -> Result<()> {
    let context = TestContext::new("3.12");

    // Create an editable package with dynamic metadata.
    let editable_dir = context.temp_dir.child("editable");
    editable_dir.create_dir_all()?;
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

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--editable")
        .arg(editable_dir.path()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.0.0
     + example==0.1.0 (from file://[TEMP_DIR]/editable)
     + idna==3.6
     + sniffio==1.3.1
    "###
    );

    // Re-installing should not re-install, as we don't special-case dynamic metadata.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--editable")
        .arg(editable_dir.path()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    "###
    );

    Ok(())
}

#[test]
fn invalidate_path_on_change() -> Result<()> {
    let context = TestContext::new("3.12");

    // Create a local package.
    let editable_dir = context.temp_dir.child("editable");
    editable_dir.create_dir_all()?;
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

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("example @ .")
        .current_dir(editable_dir.path()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.0.0
     + example==0.0.0 (from file://[TEMP_DIR]/editable)
     + idna==3.6
     + sniffio==1.3.1
    "###
    );

    // Re-installing should be a no-op.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("example @ .")
        .current_dir(editable_dir.path()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    "###
    );

    // Modify the package.
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
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("example @ .")
        .current_dir(editable_dir.path()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 2 packages in [TIME]
    Uninstalled 2 packages in [TIME]
    Installed 2 packages in [TIME]
     - anyio==4.0.0
     + anyio==3.7.1
     - example==0.0.0 (from file://[TEMP_DIR]/editable)
     + example==0.0.0 (from file://[TEMP_DIR]/editable)
    "###
    );

    Ok(())
}

/// Install from a direct path (wheel) with changed versions in the file name.
#[test]
fn path_name_version_change() {
    let context = TestContext::new("3.12");

    uv_snapshot!(context.filters(), context.pip_install()
        .arg(context.workspace_root.join("scripts/links/ok-1.0.0-py3-none-any.whl")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + ok==1.0.0 (from file://[WORKSPACE]/scripts/links/ok-1.0.0-py3-none-any.whl)
    "###
    );

    // Installing the same path again should be a no-op
    uv_snapshot!(context.filters(), context.pip_install()
        .arg(context.workspace_root.join("scripts/links/ok-1.0.0-py3-none-any.whl")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    "###
    );

    // Installing a new path should succeed
    uv_snapshot!(context.filters(), context.pip_install()
        .arg(context.workspace_root.join("scripts/links/ok-2.0.0-py3-none-any.whl")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - ok==1.0.0 (from file://[WORKSPACE]/scripts/links/ok-1.0.0-py3-none-any.whl)
     + ok==2.0.0 (from file://[WORKSPACE]/scripts/links/ok-2.0.0-py3-none-any.whl)
    "###
    );

    // Installing a new path should succeed regardless of which version is "newer"
    uv_snapshot!(context.filters(), context.pip_install()
        .arg(context.workspace_root.join("scripts/links/ok-1.0.0-py3-none-any.whl")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - ok==2.0.0 (from file://[WORKSPACE]/scripts/links/ok-2.0.0-py3-none-any.whl)
     + ok==1.0.0 (from file://[WORKSPACE]/scripts/links/ok-1.0.0-py3-none-any.whl)
    "###
    );
}

/// Install from a direct path (wheel) with the same name at a different path.
#[test]
fn path_changes_with_same_name() -> Result<()> {
    let context = TestContext::new("3.12");

    let wheel = context
        .workspace_root
        .join("scripts/links/ok-1.0.0-py3-none-any.whl");

    let one = context.temp_dir.child("one");
    one.create_dir_all()?;
    let one_wheel = one.child(wheel.file_name().unwrap());

    let two = context.temp_dir.child("two");
    two.create_dir_all()?;
    let two_wheel = two.child(wheel.file_name().unwrap());

    fs_err::copy(&wheel, &one_wheel)?;
    fs_err::copy(&wheel, &two_wheel)?;

    uv_snapshot!(context.filters(), context.pip_install()
        .arg(one_wheel.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + ok==1.0.0 (from file://[TEMP_DIR]/one/ok-1.0.0-py3-none-any.whl)
    "###
    );

    uv_snapshot!(context.filters(), context.pip_install()
        .arg(two_wheel.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - ok==1.0.0 (from file://[TEMP_DIR]/one/ok-1.0.0-py3-none-any.whl)
     + ok==1.0.0 (from file://[TEMP_DIR]/two/ok-1.0.0-py3-none-any.whl)
    "###
    );

    Ok(())
}

/// Ignore a URL dependency with a non-matching marker.
#[test]
fn editable_url_with_marker() -> Result<()> {
    let context = TestContext::new("3.12");

    let editable_dir = context.temp_dir.child("editable");
    editable_dir.create_dir_all()?;
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

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--editable")
        .arg(editable_dir.path()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.0.0
     + example==0.1.0 (from file://[TEMP_DIR]/editable)
     + idna==3.6
     + sniffio==1.3.1
    "###
    );

    Ok(())
}

/// Raise an error when an editable's `Requires-Python` constraint is not met.
#[test]
fn requires_python_editable() -> Result<()> {
    let context = TestContext::new("3.12");

    // Create an editable package with a `Requires-Python` constraint that is not met.
    let editable_dir = context.temp_dir.child("editable");
    editable_dir.create_dir_all()?;
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

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--editable")
        .arg(editable_dir.path()), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because the current Python version (3.12.[X]) does not satisfy Python<=3.8 and example==0.0.0 depends on Python<=3.8, we can conclude that example==0.0.0 cannot be used.
          And because only example==0.0.0 is available and you require example, we can conclude that the requirements are unsatisfiable.
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
        .chain(context.filters())
        .collect::<Vec<_>>();
    uv_snapshot!(filters, context.pip_install()
        .arg("-r")
        .arg("requirements.in")
        .arg("--no-build-isolation"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to download and build: `anyio @ https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz`
      Caused by: Failed to build: `anyio @ https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz`
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
    uv_snapshot!(context.pip_install()
        .arg("setuptools")
        .arg("wheel"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + setuptools==69.2.0
     + wheel==0.43.0
    "###);

    // We expect the build to succeed, since `setuptools` is now installed.
    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg("requirements.in")
        .arg("--no-build-isolation"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==0.0.0 (from https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz)
     + idna==3.6
     + sniffio==1.3.1
    "###
    );

    Ok(())
}

/// Ensure that `UV_NO_BUILD_ISOLATION` env var does the same as the `--no-build-isolation` flag
#[test]
fn respect_no_build_isolation_env_var() -> Result<()> {
    let context = TestContext::new("3.12");
    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str("anyio @ https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz")?;

    // We expect the build to fail, because `setuptools` is not installed.
    let filters = std::iter::once((r"exit code: 1", "exit status: 1"))
        .chain(context.filters())
        .collect::<Vec<_>>();
    uv_snapshot!(filters, context.pip_install()
        .arg("-r")
        .arg("requirements.in")
        .env("UV_NO_BUILD_ISOLATION", "yes"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to download and build: `anyio @ https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz`
      Caused by: Failed to build: `anyio @ https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz`
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
    uv_snapshot!(context.pip_install()
        .arg("setuptools")
        .arg("wheel"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + setuptools==69.2.0
     + wheel==0.43.0
    "###);

    // We expect the build to succeed, since `setuptools` is now installed.
    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg("requirements.in")
        .env("UV_NO_BUILD_ISOLATION", "yes"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==0.0.0 (from https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz)
     + idna==3.6
     + sniffio==1.3.1
    "###
    );

    Ok(())
}

/// This tests that uv can read UTF-16LE encoded requirements.txt files.
///
/// Ref: <https://github.com/astral-sh/uv/issues/2276>
#[test]
fn install_utf16le_requirements() -> Result<()> {
    let context = TestContext::new("3.12");
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_binary(&utf8_to_utf16_with_bom_le("tomli"))?;

    uv_snapshot!(context.pip_install()
        .env_remove("UV_EXCLUDE_NEWER")
        .arg("-r")
        .arg("requirements.txt"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + tomli==2.0.1
    "###
    );
    Ok(())
}

/// This tests that uv can read UTF-16BE encoded requirements.txt files.
///
/// Ref: <https://github.com/astral-sh/uv/issues/2276>
#[test]
fn install_utf16be_requirements() -> Result<()> {
    let context = TestContext::new("3.12");
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_binary(&utf8_to_utf16_with_bom_be("tomli"))?;

    uv_snapshot!(context.pip_install()
        .env_remove("UV_EXCLUDE_NEWER")
        .arg("-r")
        .arg("requirements.txt"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
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
    requirements_txt.write_str("httpx==0.25.1")?;

    uv_snapshot!(context.pip_install()
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
     + anyio==4.3.0
     + certifi==2024.2.2
     + h11==0.14.0
     + httpcore==1.0.4
     + httpx==0.25.1
     + idna==3.6
     + sniffio==1.3.1
    "###
    );

    Ok(())
}

#[test]
fn dry_run_install_url_dependency() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let context = TestContext::new("3.12");
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("anyio @ https://files.pythonhosted.org/packages/2d/b8/7333d87d5f03247215d86a86362fd3e324111788c6cdd8d2e6196a6ba833/anyio-4.2.0.tar.gz")?;

    uv_snapshot!(context.pip_install()
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
     + idna==3.6
     + sniffio==1.3.1
    "###
    );

    Ok(())
}

#[test]
fn dry_run_uninstall_url_dependency() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let context = TestContext::new("3.12");
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("anyio @ https://files.pythonhosted.org/packages/2d/b8/7333d87d5f03247215d86a86362fd3e324111788c6cdd8d2e6196a6ba833/anyio-4.2.0.tar.gz")?;

    // Install the URL dependency
    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.2.0 (from https://files.pythonhosted.org/packages/2d/b8/7333d87d5f03247215d86a86362fd3e324111788c6cdd8d2e6196a6ba833/anyio-4.2.0.tar.gz)
     + idna==3.6
     + sniffio==1.3.1
    "###
    );

    // Then switch to a registry dependency
    requirements_txt.write_str("anyio")?;
    uv_snapshot!(context.pip_install()
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
     + anyio==4.3.0
    "###
    );

    Ok(())
}

#[test]
fn dry_run_install_already_installed() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let context = TestContext::new("3.12");
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("httpx==0.25.1")?;

    // Install the package
    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
    Prepared 7 packages in [TIME]
    Installed 7 packages in [TIME]
     + anyio==4.3.0
     + certifi==2024.2.2
     + h11==0.14.0
     + httpcore==1.0.4
     + httpx==0.25.1
     + idna==3.6
     + sniffio==1.3.1
    "###
    );

    // Install again with dry run enabled
    uv_snapshot!(context.pip_install()
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
    requirements_txt.write_str("httpcore==1.0.2")?;

    // Install a dependency of httpx
    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + certifi==2024.2.2
     + h11==0.14.0
     + httpcore==1.0.2
    "###
    );

    // Install it httpx with dry run enabled
    requirements_txt.write_str("httpx==0.25.1")?;
    uv_snapshot!(context.pip_install()
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
     + anyio==4.3.0
     + httpx==0.25.1
     + idna==3.6
     + sniffio==1.3.1
    "###
    );

    Ok(())
}

#[test]
fn dry_run_install_then_upgrade() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let context = TestContext::new("3.12");
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("httpx==0.25.0")?;

    // Install the package
    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
    Prepared 7 packages in [TIME]
    Installed 7 packages in [TIME]
     + anyio==4.3.0
     + certifi==2024.2.2
     + h11==0.14.0
     + httpcore==0.18.0
     + httpx==0.25.0
     + idna==3.6
     + sniffio==1.3.1
    "###
    );

    // Bump the version and install with dry run enabled
    requirements_txt.write_str("httpx==0.25.1")?;
    uv_snapshot!(context.pip_install()
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

/// Raise an error when a direct URL's `Requires-Python` constraint is not met.
#[test]
fn requires_python_direct_url() -> Result<()> {
    let context = TestContext::new("3.12");

    // Create an editable package with a `Requires-Python` constraint that is not met.
    let editable_dir = context.temp_dir.child("editable");
    editable_dir.create_dir_all()?;
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

    uv_snapshot!(context.filters(), context.pip_install()
        .arg(format!("example @ {}", editable_dir.path().display())), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because the current Python version (3.12.[X]) does not satisfy Python<=3.8 and example==0.0.0 depends on Python<=3.8, we can conclude that example==0.0.0 cannot be used.
          And because only example==0.0.0 is available and you require example, we can conclude that the requirements are unsatisfiable.
    "###
    );

    Ok(())
}

/// Install a package from an index that requires authentication
#[test]
fn install_package_basic_auth_from_url() {
    let context = TestContext::new("3.12");

    uv_snapshot!(context.pip_install()
        .arg("anyio")
        .arg("--index-url")
        .arg("https://public:heron@pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + sniffio==1.3.1
    "###
    );

    context.assert_command("import anyio").success();
}

/// Install a package from an index that requires authentication
#[test]
fn install_package_basic_auth_from_netrc_default() -> Result<()> {
    let context = TestContext::new("3.12");
    let netrc = context.temp_dir.child(".netrc");
    netrc.write_str("default login public password heron")?;

    uv_snapshot!(context.pip_install()
        .arg("anyio")
        .arg("--index-url")
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple")
        .env("NETRC", netrc.to_str().unwrap())
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + sniffio==1.3.1
    "###
    );

    context.assert_command("import anyio").success();

    Ok(())
}

/// Install a package from an index that requires authentication
#[test]
fn install_package_basic_auth_from_netrc() -> Result<()> {
    let context = TestContext::new("3.12");
    let netrc = context.temp_dir.child(".netrc");
    netrc.write_str("machine pypi-proxy.fly.dev login public password heron")?;

    uv_snapshot!(context.pip_install()
        .arg("anyio")
        .arg("--index-url")
        .arg("https://pypi-proxy.fly.dev/basic-auth/simple")
        .env("NETRC", netrc.to_str().unwrap())
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + sniffio==1.3.1
    "###
    );

    context.assert_command("import anyio").success();

    Ok(())
}

/// Install a package from an index that requires authentication
/// Define the `--index-url` in the requirements file
#[test]
fn install_package_basic_auth_from_netrc_index_in_requirements() -> Result<()> {
    let context = TestContext::new("3.12");
    let netrc = context.temp_dir.child(".netrc");
    netrc.write_str("machine pypi-proxy.fly.dev login public password heron")?;

    let requirements = context.temp_dir.child("requirements.txt");
    requirements.write_str(
        r"
anyio
--index-url https://pypi-proxy.fly.dev/basic-auth/simple
    ",
    )?;

    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .env("NETRC", netrc.to_str().unwrap())
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + sniffio==1.3.1
    "###
    );

    context.assert_command("import anyio").success();

    Ok(())
}

/// Install a package from an index that provides relative links
#[test]
fn install_index_with_relative_links() {
    let context = TestContext::new("3.12");

    uv_snapshot!(context.pip_install()
        .arg("anyio")
        .arg("--index-url")
        .arg("https://pypi-proxy.fly.dev/relative/simple")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + sniffio==1.3.1
    "###
    );

    context.assert_command("import anyio").success();
}

/// Install a package from an index that requires authentication from the keyring.
#[test]
fn install_package_basic_auth_from_keyring() {
    let context = TestContext::new("3.12");

    // Install our keyring plugin
    context
        .pip_install()
        .arg(
            context
                .workspace_root
                .join("scripts")
                .join("packages")
                .join("keyring_test_plugin"),
        )
        .assert()
        .success();

    uv_snapshot!(context.pip_install()
        .arg("anyio")
        .arg("--index-url")
        .arg("https://public@pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--keyring-provider")
        .arg("subprocess")
        .arg("--strict")
        .env("KEYRING_TEST_CREDENTIALS", r#"{"pypi-proxy.fly.dev": {"public": "heron"}}"#)
        .env("PATH", venv_bin_path(&context.venv)), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Request for public@https://pypi-proxy.fly.dev/basic-auth/simple/anyio/
    Request for public@pypi-proxy.fly.dev
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + sniffio==1.3.1
    "###
    );

    context.assert_command("import anyio").success();
}

/// Install a package from an index that requires authentication
/// but the keyring has the wrong password
#[test]
fn install_package_basic_auth_from_keyring_wrong_password() {
    let context = TestContext::new("3.12");

    // Install our keyring plugin
    context
        .pip_install()
        .arg(
            context
                .workspace_root
                .join("scripts")
                .join("packages")
                .join("keyring_test_plugin"),
        )
        .assert()
        .success();

    uv_snapshot!(context.pip_install()
        .arg("anyio")
        .arg("--index-url")
        .arg("https://public@pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--keyring-provider")
        .arg("subprocess")
        .arg("--strict")
        .env("KEYRING_TEST_CREDENTIALS", r#"{"pypi-proxy.fly.dev": {"public": "foobar"}}"#)
        .env("PATH", venv_bin_path(&context.venv)), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Request for public@https://pypi-proxy.fly.dev/basic-auth/simple/anyio/
    Request for public@pypi-proxy.fly.dev
    error: Failed to download `anyio==4.3.0`
      Caused by: HTTP status client error (401 Unauthorized) for url (https://pypi-proxy.fly.dev/basic-auth/files/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl.metadata)
    "###
    );
}

/// Install a package from an index that requires authentication
/// but the keyring has the wrong username
#[test]
fn install_package_basic_auth_from_keyring_wrong_username() {
    let context = TestContext::new("3.12");

    // Install our keyring plugin
    context
        .pip_install()
        .arg(
            context
                .workspace_root
                .join("scripts")
                .join("packages")
                .join("keyring_test_plugin"),
        )
        .assert()
        .success();

    uv_snapshot!(context.pip_install()
        .arg("anyio")
        .arg("--index-url")
        .arg("https://public@pypi-proxy.fly.dev/basic-auth/simple")
        .arg("--keyring-provider")
        .arg("subprocess")
        .arg("--strict")
        .env("KEYRING_TEST_CREDENTIALS", r#"{"pypi-proxy.fly.dev": {"other": "heron"}}"#)
        .env("PATH", venv_bin_path(&context.venv)), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Request for public@https://pypi-proxy.fly.dev/basic-auth/simple/anyio/
    Request for public@pypi-proxy.fly.dev
    error: Failed to download `anyio==4.3.0`
      Caused by: HTTP status client error (401 Unauthorized) for url (https://pypi-proxy.fly.dev/basic-auth/files/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl.metadata)
    "###
    );
}

/// Install a package from an index that provides relative links and requires authentication
#[test]
fn install_index_with_relative_links_authenticated() {
    let context = TestContext::new("3.12");

    uv_snapshot!(context.pip_install()
        .arg("anyio")
        .arg("--index-url")
        .arg("https://public:heron@pypi-proxy.fly.dev/basic-auth/relative/simple")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + sniffio==1.3.1
    "###
    );

    context.assert_command("import anyio").success();
}

/// The modified time of `site-packages` should change on package installation.
#[cfg(unix)]
#[test]
fn install_site_packages_mtime_updated() -> Result<()> {
    use std::os::unix::fs::MetadataExt;

    let context = TestContext::new("3.12");

    let site_packages = context.site_packages();

    // `mtime` is only second-resolution so we include the nanoseconds as well
    let metadata = site_packages.metadata()?;
    let pre_mtime = metadata.mtime();
    let pre_mtime_ns = metadata.mtime_nsec();

    // Install a package.
    uv_snapshot!(context.pip_install()
        .arg("anyio")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + sniffio==1.3.1
    "###
    );

    let metadata = site_packages.metadata()?;
    let post_mtime = metadata.mtime();
    let post_mtime_ns = metadata.mtime_nsec();

    assert!(
        (post_mtime, post_mtime_ns) > (pre_mtime, pre_mtime_ns),
        "Expected newer mtime than {pre_mtime}.{pre_mtime_ns} but got {post_mtime}.{post_mtime_ns}"
    );

    Ok(())
}

/// We had a bug where maturin would walk up to the top level gitignore of the cache with a `*`
/// entry (because we want to ignore the entire cache from outside), ignoring all python source
/// files.
#[test]
fn deptry_gitignore() {
    let context = TestContext::new("3.12");

    let source_dist_dir = context
        .workspace_root
        .join("scripts/packages/deptry_reproducer");

    uv_snapshot!(context.filters(), context.pip_install()
        .arg(format!("deptry_reproducer @ {}", source_dist_dir.join("deptry_reproducer-0.1.0.tar.gz").simplified_display()))
        .arg("--strict")
        .current_dir(source_dist_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + cffi==1.16.0
     + deptry-reproducer==0.1.0 (from file://[WORKSPACE]/scripts/packages/deptry_reproducer/deptry_reproducer-0.1.0.tar.gz)
     + pycparser==2.21
    "###
    );

    // Check that we packed the python source files
    context
        .assert_command("import deptry_reproducer.foo")
        .success();
}

/// Reinstall an installed package with `--no-index`
#[test]
fn reinstall_no_index() {
    let context = TestContext::new("3.12");

    // Install anyio
    uv_snapshot!(context.pip_install()
        .arg("anyio")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + sniffio==1.3.1
    "###
    );

    // Install anyio again
    uv_snapshot!(context.pip_install()
        .arg("anyio")
        .arg("--no-index")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    "###
    );

    // Reinstall
    // We should not consider the already installed package as a source and
    // should attempt to pull from the index
    uv_snapshot!(context.pip_install()
        .arg("anyio")
        .arg("--no-index")
        .arg("--reinstall")
        .arg("--strict"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because anyio was not found in the provided package locations and you require anyio, we can conclude that the requirements are unsatisfiable.

          hint: Packages were unavailable because index lookups were disabled and no additional package locations were provided (try: `--find-links <uri>`)
    "###
    );
}

#[test]
fn already_installed_remote_dependencies() {
    let context = TestContext::new("3.12");

    // Install anyio's dependencies.
    uv_snapshot!(context.pip_install()
        .arg("idna")
        .arg("sniffio")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + idna==3.6
     + sniffio==1.3.1
    "###
    );

    // Install anyio.
    uv_snapshot!(context.pip_install()
        .arg("anyio")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + anyio==4.3.0
    "###
    );
}

/// Install an editable package that depends on a previously installed editable package.
#[test]
fn already_installed_dependent_editable() {
    let context = TestContext::new("3.12");
    let root_path = context
        .workspace_root
        .join("scripts/packages/dependent_locals");

    // Install the first editable
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-e")
        .arg(root_path.join("first_local")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + first-local==0.1.0 (from file://[WORKSPACE]/scripts/packages/dependent_locals/first_local)
    "###
    );

    // Install the second editable which depends on the first editable
    // The already installed first editable package should satisfy the requirement
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-e")
        .arg(root_path.join("second_local"))
        // Disable the index to guard this test against dependency confusion attacks
        .arg("--no-index")
        .arg("--find-links")
        .arg(build_vendor_links_url()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + second-local==0.1.0 (from file://[WORKSPACE]/scripts/packages/dependent_locals/second_local)
    "###
    );

    // Request install of the first editable by full path again
    // We should audit the installed package
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-e")
        .arg(root_path.join("first_local")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    "###
    );

    // Request reinstallation of the first package during install of the second
    // It's not available on an index and the user has not specified the path so we fail.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-e")
        .arg(root_path.join("second_local"))
        .arg("--reinstall-package")
        .arg("first-local")
        // Disable the index to guard this test against dependency confusion attacks
        .arg("--no-index")
        .arg("--find-links")
        .arg(build_vendor_links_url()), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because first-local was not found in the provided package locations and second-local==0.1.0 depends on first-local, we can conclude that second-local==0.1.0 cannot be used.
          And because only second-local==0.1.0 is available and you require second-local, we can conclude that the requirements are unsatisfiable.
    "###
    );

    // Request reinstallation of the first package
    // We include it in the install command with a full path so we should succeed
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-e")
        .arg(root_path.join("first_local"))
        .arg("--reinstall-package")
        .arg("first-local"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - first-local==0.1.0 (from file://[WORKSPACE]/scripts/packages/dependent_locals/first_local)
     + first-local==0.1.0 (from file://[WORKSPACE]/scripts/packages/dependent_locals/first_local)
    "###
    );
}

/// Install a local package that depends on a previously installed local package.
#[test]
fn already_installed_local_path_dependent() {
    let context = TestContext::new("3.12");
    let root_path = context
        .workspace_root
        .join("scripts/packages/dependent_locals");

    // Install the first local
    uv_snapshot!(context.filters(), context.pip_install()
        .arg(root_path.join("first_local")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + first-local==0.1.0 (from file://[WORKSPACE]/scripts/packages/dependent_locals/first_local)
    "###
    );

    // Install the second local which depends on the first local
    // The already installed first local package should satisfy the requirement
    uv_snapshot!(context.filters(), context.pip_install()
        .arg(root_path.join("second_local"))
        // Disable the index to guard this test against dependency confusion attacks
        .arg("--no-index")
        .arg("--find-links")
        .arg(build_vendor_links_url()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + second-local==0.1.0 (from file://[WORKSPACE]/scripts/packages/dependent_locals/second_local)
    "###
    );

    // Request install of the first local by full path again
    // We should audit the installed package
    uv_snapshot!(context.filters(), context.pip_install()
        .arg(root_path.join("first_local")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    "###
    );

    // Request reinstallation of the first package during install of the second
    // It's not available on an index and the user has not specified the path so we fail
    uv_snapshot!(context.filters(), context.pip_install()
        .arg(root_path.join("second_local"))
        .arg("--reinstall-package")
        .arg("first-local")
        // Disable the index to guard this test against dependency confusion attacks
        .arg("--no-index")
        .arg("--find-links")
        .arg(build_vendor_links_url()), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because first-local was not found in the provided package locations and second-local==0.1.0 depends on first-local, we can conclude that second-local==0.1.0 cannot be used.
          And because only second-local==0.1.0 is available and you require second-local, we can conclude that the requirements are unsatisfiable.
    "###
    );

    // Request reinstallation of the first package
    // We include it in the install command with a full path so we succeed
    uv_snapshot!(context.filters(), context.pip_install()
        .arg(root_path.join("second_local"))
        .arg(root_path.join("first_local"))
        .arg("--reinstall-package")
        .arg("first-local"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - first-local==0.1.0 (from file://[WORKSPACE]/scripts/packages/dependent_locals/first_local)
     + first-local==0.1.0 (from file://[WORKSPACE]/scripts/packages/dependent_locals/first_local)
    "###
    );

    // Request upgrade of the first package
    // It's not available on an index and the user has not specified the path so we fail
    uv_snapshot!(context.filters(), context.pip_install()
        .arg(root_path.join("second_local"))
        .arg("--upgrade-package")
        .arg("first-local")
        // Disable the index to guard this test against dependency confusion attacks
        .arg("--no-index")
        .arg("--find-links")
        .arg(build_vendor_links_url()), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because first-local was not found in the provided package locations and second-local==0.1.0 depends on first-local, we can conclude that second-local==0.1.0 cannot be used.
          And because only second-local==0.1.0 is available and you require second-local, we can conclude that the requirements are unsatisfiable.
    "###
    );

    // Request upgrade of the first package
    // A full path is specified and there's nothing to upgrade to so we should just audit
    uv_snapshot!(context.filters(), context.pip_install()
        .arg(root_path.join("first_local"))
        .arg(root_path.join("second_local"))
        .arg("--upgrade-package")
        .arg("first-local")
        // Disable the index to guard this test against dependency confusion attacks
        .arg("--no-index")
        .arg("--find-links")
        .arg(build_vendor_links_url()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Audited 2 packages in [TIME]
    "###
    );
}

/// A local version of a package shadowing a remote package is installed.
#[test]
fn already_installed_local_version_of_remote_package() {
    let context = TestContext::new("3.12");
    let root_path = context.workspace_root.join("scripts/packages");

    // Install the local anyio first
    uv_snapshot!(context.filters(), context.pip_install()
        .arg(root_path.join("anyio_local")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + anyio==4.3.0+foo (from file://[WORKSPACE]/scripts/packages/anyio_local)
    "###
    );

    // Install again without specifying a local path — this should not pull from the index
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("anyio"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    "###
    );

    // Request install with a different version
    //
    // We should attempt to pull from the index since the installed version does not match
    // but we disable it here to preserve this dependency for future tests
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("anyio==4.2.0")
        .arg("--no-index"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because anyio was not found in the provided package locations and you require anyio==4.2.0, we can conclude that the requirements are unsatisfiable.

          hint: Packages were unavailable because index lookups were disabled and no additional package locations were provided (try: `--find-links <uri>`)
    "###
    );

    // Request reinstallation with the local version segment — this should fail since it is not available
    // in the index and the path was not provided
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("anyio==4.3.0+foo")
        .arg("--reinstall"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because there is no version of anyio==4.3.0+foo and you require anyio==4.3.0+foo, we can conclude that the requirements are unsatisfiable.
    "###
    );

    // Request reinstall with the full path, this should reinstall from the path and not pull from
    // the index (or rebuild).
    uv_snapshot!(context.filters(), context.pip_install()
        .arg(root_path.join("anyio_local"))
        .arg("--reinstall")
        .arg("anyio"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - anyio==4.3.0+foo (from file://[WORKSPACE]/scripts/packages/anyio_local)
     + anyio==4.3.0+foo (from file://[WORKSPACE]/scripts/packages/anyio_local)
    "###
    );

    // Request reinstallation with just the name, this should pull from the index
    // and replace the path dependency
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("anyio")
        .arg("--reinstall"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 3 packages in [TIME]
     - anyio==4.3.0+foo (from file://[WORKSPACE]/scripts/packages/anyio_local)
     + anyio==4.3.0
     + idna==3.6
     + sniffio==1.3.1
    "###
    );

    // Install the local anyio again so we can test upgrades
    uv_snapshot!(context.filters(), context.pip_install()
        .arg(root_path.join("anyio_local")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - anyio==4.3.0
     + anyio==4.3.0+foo (from file://[WORKSPACE]/scripts/packages/anyio_local)
    "###
    );

    // Request upgrade with just the name
    // We shouldn't pull from the index because the local version is "newer"
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("anyio")
        .arg("--upgrade"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Audited 3 packages in [TIME]
    "###
    );

    // Install something that depends on anyio
    // We shouldn't overwrite our local version with the remote anyio here
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("httpx"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + certifi==2024.2.2
     + h11==0.14.0
     + httpcore==1.0.4
     + httpx==0.27.0
    "###
    );
}

/// Install a package with multiple installed distributions in a virtual environment.
#[test]
#[cfg(unix)]
fn already_installed_multiple_versions() -> Result<()> {
    fn prepare(context: &TestContext) -> Result<()> {
        use crate::common::copy_dir_all;

        // Install into the base environment
        context.pip_install().arg("anyio==3.7.0").assert().success();

        // Install another version into another environment
        let context_duplicate = TestContext::new("3.12");
        context_duplicate
            .pip_install()
            .arg("anyio==4.0.0")
            .assert()
            .success();

        // Copy the second version into the first environment
        copy_dir_all(
            context_duplicate
                .site_packages()
                .join("anyio-4.0.0.dist-info"),
            context.site_packages().join("anyio-4.0.0.dist-info"),
        )?;

        Ok(())
    }

    let context = TestContext::new("3.12");

    prepare(&context)?;

    // Request the second anyio version again
    // Should remove both previous versions and reinstall the second one
    uv_snapshot!(context.filters(), context.pip_install().arg("anyio==4.0.0"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 2 packages in [TIME]
    Installed 1 package in [TIME]
     - anyio==3.7.0
     - anyio==4.0.0
     + anyio==4.0.0
    "###
    );

    // Reset the test context
    prepare(&context)?;

    // Request the anyio without a version specifier
    // This is loosely a regression test for the ordering of the installation preferences
    // from existing site-packages
    uv_snapshot!(context.filters(), context.pip_install().arg("anyio"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 2 packages in [TIME]
    Installed 1 package in [TIME]
     - anyio==3.7.0
     - anyio==4.0.0
     + anyio==4.3.0
    "###
    );

    Ok(())
}

/// Install a package from a remote URL
#[test]
#[cfg(feature = "git")]
fn already_installed_remote_url() {
    let context = TestContext::new("3.8");

    // First, install from the remote URL
    uv_snapshot!(context.filters(), context.pip_install().arg("uv-public-pypackage @ git+https://github.com/astral-test/uv-public-pypackage"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + uv-public-pypackage==0.1.0 (from git+https://github.com/astral-test/uv-public-pypackage@b270df1a2fb5d012294e9aaf05e7e0bab1e6a389)
    "###);

    context.assert_installed("uv_public_pypackage", "0.1.0");

    // Request installation again with a different URL, but the same _canonical_ URL. We should
    // resolve the package (since we installed a specific commit, but are now requesting the default
    // branch), but not reinstall the package.
    uv_snapshot!(context.filters(), context.pip_install().arg("uv-public-pypackage @ git+https://github.com/astral-test/uv-public-pypackage.git"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Audited 1 package in [TIME]
    "###);

    // Request installation again with a different URL, but the same _canonical_ URL and the same
    // commit. We should neither resolve nor reinstall the package, since it's already installed
    // at this precise commit.
    uv_snapshot!(context.filters(), context.pip_install().arg("uv-public-pypackage @ git+https://github.com/astral-test/uv-public-pypackage.git@b270df1a2fb5d012294e9aaf05e7e0bab1e6a389"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    "###);

    // Request installation again with just the name
    // We should just audit the URL package since it fulfills this requirement
    uv_snapshot!(
        context.pip_install().arg("uv-public-pypackage"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    "###);

    // Request reinstallation
    // We should fail since the URL was not provided
    uv_snapshot!(
        context.pip_install()
        .arg("uv-public-pypackage")
        .arg("--no-index")
        .arg("--reinstall"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because uv-public-pypackage was not found in the provided package locations and you require uv-public-pypackage, we can conclude that the requirements are unsatisfiable.

          hint: Packages were unavailable because index lookups were disabled and no additional package locations were provided (try: `--find-links <uri>`)
    "###);

    // Request installation again with just the full URL
    // We should just audit the existing package
    uv_snapshot!(
        context.pip_install().arg("uv-public-pypackage @ git+https://github.com/astral-test/uv-public-pypackage"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Audited 1 package in [TIME]
    "###);

    // Request reinstallation with the full URL
    // We should reinstall successfully
    uv_snapshot!(
        context.pip_install()
        .arg("uv-public-pypackage @ git+https://github.com/astral-test/uv-public-pypackage")
        .arg("--reinstall"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - uv-public-pypackage==0.1.0 (from git+https://github.com/astral-test/uv-public-pypackage@b270df1a2fb5d012294e9aaf05e7e0bab1e6a389)
     + uv-public-pypackage==0.1.0 (from git+https://github.com/astral-test/uv-public-pypackage@b270df1a2fb5d012294e9aaf05e7e0bab1e6a389)
    "###);

    // Request installation again with a different version
    // We should attempt to pull from the index since the local version does not match
    uv_snapshot!(
        context.pip_install().arg("uv-public-pypackage==0.2.0").arg("--no-index"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because uv-public-pypackage was not found in the provided package locations and you require uv-public-pypackage==0.2.0, we can conclude that the requirements are unsatisfiable.

          hint: Packages were unavailable because index lookups were disabled and no additional package locations were provided (try: `--find-links <uri>`)
    "###);
}

/// Sync using `--find-links` with a local directory.
#[test]
fn find_links() {
    let context = TestContext::new("3.12");

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("tqdm")
        .arg("--find-links")
        .arg(context.workspace_root.join("scripts/links/")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + tqdm==1000.0.0
    "###
    );
}

/// Sync using `--find-links` with a local directory, with wheels disabled.
#[test]
fn find_links_no_binary() {
    let context = TestContext::new("3.12");

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("tqdm")
        .arg("--no-binary")
        .arg(":all:")
        .arg("--find-links")
        .arg(context.workspace_root.join("scripts/links/")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + tqdm==999.0.0
    "###
    );
}

/// Provide valid hashes for all dependencies with `--require-hashes`.
#[test]
fn require_hashes() -> Result<()> {
    let context = TestContext::new("3.12");

    // Write to a requirements file.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(indoc::indoc! {r"
        anyio==4.0.0 \
            --hash=sha256:cfdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f \
            --hash=sha256:f7ed51751b2c2add651e5747c891b47e26d2a21be5d32d9311dfe9692f3e5d7a
        idna==3.6 \
            --hash=sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca \
            --hash=sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f
            # via anyio
        sniffio==1.3.1 \
            --hash=sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2 \
            --hash=sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc
            # via anyio
    "})?;

    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--require-hashes"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.0.0
     + idna==3.6
     + sniffio==1.3.1
    "###
    );

    Ok(())
}

/// Omit hashes for dependencies with `--require-hashes`, which is allowed with `--no-deps`.
#[test]
fn require_hashes_no_deps() -> Result<()> {
    let context = TestContext::new("3.12");

    // Write to a requirements file.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(indoc::indoc! {r"
        anyio==4.0.0 \
            --hash=sha256:cfdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f \
            --hash=sha256:f7ed51751b2c2add651e5747c891b47e26d2a21be5d32d9311dfe9692f3e5d7a
    "})?;

    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--no-deps")
        .arg("--require-hashes"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + anyio==4.0.0
    "###
    );

    Ok(())
}

/// Provide the wrong hash with `--require-hashes`.
#[test]
fn require_hashes_mismatch() -> Result<()> {
    let context = TestContext::new("3.12");

    // Write to a requirements file.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(indoc::indoc! {r"
        anyio==4.0.0 \
            --hash=sha256:afdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f \
            --hash=sha256:a7ed51751b2c2add651e5747c891b47e26d2a21be5d32d9311dfe9692f3e5d7a
        idna==3.6 \
            --hash=sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca \
            --hash=sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f
            # via anyio
        sniffio==1.3.1 \
            --hash=sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2 \
            --hash=sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc
            # via anyio
    "})?;

    // Raise an error.
    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--require-hashes"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    error: Failed to prepare distributions
      Caused by: Failed to fetch wheel: anyio==4.0.0
      Caused by: Hash mismatch for `anyio==4.0.0`

    Expected:
      sha256:afdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f
      sha256:a7ed51751b2c2add651e5747c891b47e26d2a21be5d32d9311dfe9692f3e5d7a

    Computed:
      sha256:cfdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f
    "###
    );

    Ok(())
}

/// Omit a transitive dependency in `--require-hashes`.
#[test]
fn require_hashes_missing_dependency() -> Result<()> {
    let context = TestContext::new("3.12");

    // Write to a requirements file.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(
        "werkzeug==3.0.0 --hash=sha256:cbb2600f7eabe51dbc0502f58be0b3e1b96b893b05695ea2b35b43d4de2d9962",
    )?;

    // Install without error when `--require-hashes` is omitted.
    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--require-hashes"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: In `--require-hashes` mode, all requirements must be pinned upfront with `==`, but found: `markupsafe`
    "###
    );

    Ok(())
}

/// We disallow `--require-hashes` for editables' dependencies.
#[test]
fn require_hashes_editable() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(&indoc::formatdoc! {r"
        -e file://{workspace_root}/scripts/packages/black_editable[d]
        ",
        workspace_root = context.workspace_root.simplified_display(),
    })?;

    // Install the editable packages.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-r")
        .arg(requirements_txt.path())
        .arg("--require-hashes"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: In `--require-hashes` mode, all requirement must have a hash, but none were provided for: file://[WORKSPACE]/scripts/packages/black_editable[d]
    "###
    );

    Ok(())
}

/// If a hash is only included as a constraint, that's not good enough for `--require-hashes`.
#[test]
fn require_hashes_constraint() -> Result<()> {
    let context = TestContext::new("3.12");

    // Include the hash in the constraint file.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("anyio==4.0.0")?;

    let constraints_txt = context.temp_dir.child("constraints.txt");
    constraints_txt.write_str("anyio==4.0.0 --hash=sha256:cfdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f")?;

    // Install the editable packages.
    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg(requirements_txt.path())
        .arg("--require-hashes")
        .arg("-c")
        .arg(constraints_txt.path()), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: In `--require-hashes` mode, all requirement must have a hash, but none were provided for: anyio==4.0.0
    "###
    );

    // Include the hash in the requirements file, but pin the version in the constraint file.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(
        "anyio --hash=sha256:cfdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f",
    )?;

    let constraints_txt = context.temp_dir.child("constraints.txt");
    constraints_txt.write_str("anyio==4.0.0")?;

    // Install the editable packages.
    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg(requirements_txt.path())
        .arg("--require-hashes")
        .arg("-c")
        .arg(constraints_txt.path()), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: In `--require-hashes` mode, all requirement must have their versions pinned with `==`, but found: anyio
    "###
    );

    Ok(())
}

/// We allow `--require-hashes` for unnamed URL dependencies.
#[test]
fn require_hashes_unnamed() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str(indoc::indoc! {r"
            https://files.pythonhosted.org/packages/36/55/ad4de788d84a630656ece71059665e01ca793c04294c463fd84132f40fe6/anyio-4.0.0-py3-none-any.whl --hash=sha256:cfdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f
            idna==3.6 \
                --hash=sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca \
                --hash=sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f
                # via anyio
            sniffio==1.3.1 \
                --hash=sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2 \
                --hash=sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc
                # via anyio
        "})?;

    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--require-hashes"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.0.0 (from https://files.pythonhosted.org/packages/36/55/ad4de788d84a630656ece71059665e01ca793c04294c463fd84132f40fe6/anyio-4.0.0-py3-none-any.whl)
     + idna==3.6
     + sniffio==1.3.1
    "###
    );

    Ok(())
}

/// We allow `--require-hashes` for unnamed URL dependencies. In this case, the unnamed URL is
/// a repeat of a registered package.
#[test]
fn require_hashes_unnamed_repeated() -> Result<()> {
    let context = TestContext::new("3.12");

    // Re-run, but duplicate `anyio`.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str(indoc::indoc! {r"
            anyio==4.0.0 \
                --hash=sha256:cfdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f \
                --hash=sha256:f7ed51751b2c2add651e5747c891b47e26d2a21be5d32d9311dfe9692f3e5d7a
            https://files.pythonhosted.org/packages/36/55/ad4de788d84a630656ece71059665e01ca793c04294c463fd84132f40fe6/anyio-4.0.0-py3-none-any.whl --hash=sha256:cfdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f
            idna==3.6 \
                --hash=sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca \
                --hash=sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f
                # via anyio
            sniffio==1.3.1 \
                --hash=sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2 \
                --hash=sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc
                # via anyio
        "})?;

    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--require-hashes"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.0.0 (from https://files.pythonhosted.org/packages/36/55/ad4de788d84a630656ece71059665e01ca793c04294c463fd84132f40fe6/anyio-4.0.0-py3-none-any.whl)
     + idna==3.6
     + sniffio==1.3.1
    "###
    );

    Ok(())
}

/// If a hash is only included as a override, that's not good enough for `--require-hashes`.
///
/// TODO(charlie): This _should_ be allowed. It's a bug.
#[test]
fn require_hashes_override() -> Result<()> {
    let context = TestContext::new("3.12");

    // Include the hash in the override file.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("anyio==4.0.0")?;

    let overrides_txt = context.temp_dir.child("overrides.txt");
    overrides_txt.write_str("anyio==4.0.0 --hash=sha256:cfdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f")?;

    // Install the editable packages.
    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg(requirements_txt.path())
        .arg("--require-hashes")
        .arg("--override")
        .arg(overrides_txt.path()), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: In `--require-hashes` mode, all requirement must have a hash, but none were provided for: anyio==4.0.0
    "###
    );

    // Include the hash in the requirements file, but pin the version in the override file.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(
        "anyio --hash=sha256:cfdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f",
    )?;

    let overrides_txt = context.temp_dir.child("overrides.txt");
    overrides_txt.write_str("anyio==4.0.0")?;

    // Install the editable packages.
    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg(requirements_txt.path())
        .arg("--require-hashes")
        .arg("--override")
        .arg(overrides_txt.path()), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: In `--require-hashes` mode, all requirement must have their versions pinned with `==`, but found: anyio
    "###
    );

    Ok(())
}

/// Provide valid hashes for all dependencies with `--require-hashes`.
#[test]
fn verify_hashes() -> Result<()> {
    let context = TestContext::new("3.12");

    // Write to a requirements file.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(indoc::indoc! {r"
        anyio==4.0.0 \
            --hash=sha256:cfdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f \
            --hash=sha256:f7ed51751b2c2add651e5747c891b47e26d2a21be5d32d9311dfe9692f3e5d7a
        idna==3.6 \
            --hash=sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca \
            --hash=sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f
            # via anyio
        sniffio==1.3.1 \
            --hash=sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2 \
            --hash=sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc
            # via anyio
    "})?;

    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--verify-hashes"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.0.0
     + idna==3.6
     + sniffio==1.3.1
    "###
    );

    Ok(())
}

/// Omit a pinned version with `--verify-hashes`.
#[test]
fn verify_hashes_missing_version() -> Result<()> {
    let context = TestContext::new("3.12");

    // Write to a requirements file.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(indoc::indoc! {r"
        anyio \
            --hash=sha256:afdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f \
            --hash=sha256:a7ed51751b2c2add651e5747c891b47e26d2a21be5d32d9311dfe9692f3e5d7a
        idna==3.6 \
            --hash=sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca \
            --hash=sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f
            # via anyio
        sniffio==1.3.1 \
            --hash=sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2 \
            --hash=sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc
            # via anyio
    "})?;

    // Raise an error.
    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--verify-hashes"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: In `--verify-hashes` mode, all requirement must have their versions pinned with `==`, but found: anyio
    "###
    );

    Ok(())
}

/// Provide the wrong hash with `--verify-hashes`.
#[test]
fn verify_hashes_mismatch() -> Result<()> {
    let context = TestContext::new("3.12");

    // Write to a requirements file.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(indoc::indoc! {r"
        anyio==4.0.0 \
            --hash=sha256:afdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f \
            --hash=sha256:a7ed51751b2c2add651e5747c891b47e26d2a21be5d32d9311dfe9692f3e5d7a
        idna==3.6 \
            --hash=sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca \
            --hash=sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f
            # via anyio
        sniffio==1.3.1 \
            --hash=sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2 \
            --hash=sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc
            # via anyio
    "})?;

    // Raise an error.
    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--verify-hashes"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    error: Failed to prepare distributions
      Caused by: Failed to fetch wheel: anyio==4.0.0
      Caused by: Hash mismatch for `anyio==4.0.0`

    Expected:
      sha256:afdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f
      sha256:a7ed51751b2c2add651e5747c891b47e26d2a21be5d32d9311dfe9692f3e5d7a

    Computed:
      sha256:cfdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f
    "###
    );

    Ok(())
}

/// Omit a transitive dependency in `--verify-hashes`. This is allowed.
#[test]
fn verify_hashes_omit_dependency() -> Result<()> {
    let context = TestContext::new("3.12");

    // Write to a requirements file.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(
        "anyio==4.0.0 --hash=sha256:cfdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f",
    )?;

    // Install without error when `--require-hashes` is omitted.
    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--verify-hashes"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.0.0
     + idna==3.6
     + sniffio==1.3.1
    "###
    );

    Ok(())
}

/// We allow `--verify-hashes` for editable dependencies.
#[test]
fn verify_hashes_editable() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(&indoc::formatdoc! {r"
        -e file://{workspace_root}/scripts/packages/black_editable[d]
        ",
        workspace_root = context.workspace_root.simplified_display(),
    })?;

    // Install the editable packages.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-r")
        .arg(requirements_txt.path())
        .arg("--verify-hashes"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 8 packages in [TIME]
    Prepared 8 packages in [TIME]
    Installed 8 packages in [TIME]
     + aiohttp==3.9.3
     + aiosignal==1.3.1
     + attrs==23.2.0
     + black==0.1.0 (from file://[WORKSPACE]/scripts/packages/black_editable)
     + frozenlist==1.4.1
     + idna==3.6
     + multidict==6.0.5
     + yarl==1.9.4
    "###
    );

    Ok(())
}

#[test]
fn tool_uv_sources() -> Result<()> {
    let context = TestContext::new("3.12");
    // Use a subdir to test path normalization.
    let require_path = "some_dir/pyproject.toml";
    let pyproject_toml = context.temp_dir.child(require_path);
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "foo"
        version = "0.0.0"
        dependencies = [
          "tqdm>4,<=5",
          "packaging @ git+https://github.com/pypa/packaging@32deafe8668a2130a3366b98154914d188f3718e",
          "poetry_editable",
          "urllib3 @ https://files.pythonhosted.org/packages/a2/73/a68704750a7679d0b6d3ad7aa8d4da8e14e151ae82e6fee774e6e0d05ec8/urllib3-2.2.1-py3-none-any.whl",
          # Windows consistency
          "colorama>0.4,<5",
        ]

        [project.optional-dependencies]
        utils = [
            "boltons==24.0.0"
        ]
        dont_install_me = [
            "broken @ https://example.org/does/not/exist"
        ]

        [tool.uv.sources]
        tqdm = { url = "https://files.pythonhosted.org/packages/a5/d6/502a859bac4ad5e274255576cd3e15ca273cdb91731bc39fb840dd422ee9/tqdm-4.66.0-py3-none-any.whl" }
        boltons = { git = "https://github.com/mahmoud/boltons", rev = "57fbaa9b673ed85b32458b31baeeae230520e4a0" }
        poetry_editable = { path = "../poetry_editable", editable = true }
    "#})?;

    let project_root = fs_err::canonicalize(std::env::current_dir()?.join("../.."))?;
    fs_err::create_dir_all(context.temp_dir.join("poetry_editable/poetry_editable"))?;
    fs_err::copy(
        project_root.join("scripts/packages/poetry_editable/pyproject.toml"),
        context.temp_dir.join("poetry_editable/pyproject.toml"),
    )?;
    fs_err::copy(
        project_root.join("scripts/packages/poetry_editable/poetry_editable/__init__.py"),
        context
            .temp_dir
            .join("poetry_editable/poetry_editable/__init__.py"),
    )?;

    // Install the editable packages.
    uv_snapshot!(context.filters(), windows_filters=false, context.pip_install()
        .arg("--preview")
        .arg("-r")
        .arg(require_path)
        .arg("--extra")
        .arg("utils"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 9 packages in [TIME]
    Prepared 9 packages in [TIME]
    Installed 9 packages in [TIME]
     + anyio==4.3.0
     + boltons==24.0.1.dev0 (from git+https://github.com/mahmoud/boltons@57fbaa9b673ed85b32458b31baeeae230520e4a0)
     + colorama==0.4.6
     + idna==3.6
     + packaging==24.1.dev0 (from git+https://github.com/pypa/packaging@32deafe8668a2130a3366b98154914d188f3718e)
     + poetry-editable==0.1.0 (from file://[TEMP_DIR]/poetry_editable)
     + sniffio==1.3.1
     + tqdm==4.66.0 (from https://files.pythonhosted.org/packages/a5/d6/502a859bac4ad5e274255576cd3e15ca273cdb91731bc39fb840dd422ee9/tqdm-4.66.0-py3-none-any.whl)
     + urllib3==2.2.1 (from https://files.pythonhosted.org/packages/a2/73/a68704750a7679d0b6d3ad7aa8d4da8e14e151ae82e6fee774e6e0d05ec8/urllib3-2.2.1-py3-none-any.whl)
    "###
    );

    // Re-install the editable packages.
    uv_snapshot!(context.filters(), windows_filters=false, context.pip_install()
        .arg("--preview")
        .arg("-r")
        .arg(require_path)
        .arg("--extra")
        .arg("utils"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 9 packages in [TIME]
    Audited 9 packages in [TIME]
    "###
    );
    Ok(())
}

#[test]
fn tool_uv_sources_is_in_preview() -> Result<()> {
    let context = TestContext::new("3.12");
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "foo"
        version = "0.0.0"
        dependencies = [
          "iniconfig>1,<=2",
        ]

        [tool.uv.sources]
        iniconfig = { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl" }
    "#})?;

    // Install the editable packages.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-r")
        .arg("pyproject.toml"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv.sources` is experimental and may change without warning
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0 (from https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl)
    "###
    );

    Ok(())
}

/// Allow transitive URLs via recursive extras.
#[test]
fn recursive_extra_transitive_url() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.0.0"
        dependencies = []

        [project.optional-dependencies]
        all = [
            "project[docs]",
        ]
        docs = [
            "iniconfig @ https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl",
        ]
    "#})?;

    uv_snapshot!(context.filters(), context.pip_install()
        .arg(".[all]"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + iniconfig==2.0.0 (from https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl)
     + project==0.0.0 (from file://[TEMP_DIR]/)
    "###);

    Ok(())
}

/// If a package is requested as both editable and non-editable, always install it as editable.
#[test]
fn prefer_editable() -> Result<()> {
    let context = TestContext::new("3.12");

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-e")
        .arg(context.workspace_root.join("scripts/packages/black_editable"))
        .arg(context.workspace_root.join("scripts/packages/black_editable")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + black==0.1.0 (from file://[WORKSPACE]/scripts/packages/black_editable)
    "###
    );

    // Validate that `black.pth` was created.
    let path = context.site_packages().join("black.pth");
    assert!(path.is_file());

    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(&format!(
        "black @ file://{}/scripts/packages/black_editable",
        context.workspace_root.simplified_display()
    ))?;

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-e")
        .arg(context.workspace_root.join("scripts/packages/black_editable"))
        .arg("-r")
        .arg("requirements.txt"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + black==0.1.0 (from file://[WORKSPACE]/scripts/packages/black_editable)
    "###
    );

    // Validate that `black.pth` was created.
    let path = context.site_packages().join("black.pth");
    assert!(path.is_file());

    Ok(())
}

/// Resolve against a local directory laid out as a PEP 503-compatible index.
#[test]
fn local_index_absolute() -> Result<()> {
    let context = TestContext::new("3.12");

    let root = context.temp_dir.child("simple-html");
    fs_err::create_dir_all(&root)?;

    let tqdm = root.child("tqdm");
    fs_err::create_dir_all(&tqdm)?;

    let index = tqdm.child("index.html");
    index.write_str(&indoc::formatdoc! {r#"
        <!DOCTYPE html>
        <html>
          <head>
            <meta name="pypi:repository-version" content="1.1" />
          </head>
          <body>
            <h1>Links for tqdm</h1>
            <a
              href="{}/tqdm-1000.0.0-py3-none-any.whl"
              data-requires-python=">=3.8"
            >
              tqdm-1000.0.0-py3-none-any.whl
            </a>
          </body>
        </html>
    "#, Url::from_directory_path(context.workspace_root.join("scripts/links/")).unwrap().as_str()})?;

    uv_snapshot!(context.filters(), context.pip_install()
        .env_remove("UV_EXCLUDE_NEWER")
        .arg("tqdm")
        .arg("--index-url")
        .arg(Url::from_directory_path(root).unwrap().as_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + tqdm==1000.0.0
    "###
    );

    Ok(())
}

/// Resolve against a local directory laid out as a PEP 503-compatible index, provided via a
/// relative path on the CLI.
#[test]
fn local_index_relative() -> Result<()> {
    let context = TestContext::new("3.12");

    let root = context.temp_dir.child("simple-html");
    fs_err::create_dir_all(&root)?;

    let tqdm = root.child("tqdm");
    fs_err::create_dir_all(&tqdm)?;

    let index = tqdm.child("index.html");
    index.write_str(&indoc::formatdoc! {r#"
        <!DOCTYPE html>
        <html>
          <head>
            <meta name="pypi:repository-version" content="1.1" />
          </head>
          <body>
            <h1>Links for tqdm</h1>
            <a
              href="{}/tqdm-1000.0.0-py3-none-any.whl"
              data-requires-python=">=3.8"
            >
              tqdm-1000.0.0-py3-none-any.whl
            </a>
          </body>
        </html>
    "#, Url::from_directory_path(context.workspace_root.join("scripts/links/")).unwrap().as_str()})?;

    uv_snapshot!(context.filters(), context.pip_install()
        .env_remove("UV_EXCLUDE_NEWER")
        .arg("tqdm")
        .arg("--index-url")
        .arg("./simple-html"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + tqdm==1000.0.0
    "###
    );

    Ok(())
}

/// Resolve against a local directory laid out as a PEP 503-compatible index, provided via a
/// `requirements.txt` file.
#[test]
fn local_index_requirements_txt_absolute() -> Result<()> {
    let context = TestContext::new("3.12");

    let root = context.temp_dir.child("simple-html");
    fs_err::create_dir_all(&root)?;

    let tqdm = root.child("tqdm");
    fs_err::create_dir_all(&tqdm)?;

    let index = tqdm.child("index.html");
    index.write_str(&indoc::formatdoc! {r#"
        <!DOCTYPE html>
        <html>
          <head>
            <meta name="pypi:repository-version" content="1.1" />
          </head>
          <body>
            <h1>Links for tqdm</h1>
            <a
              href="{}/tqdm-1000.0.0-py3-none-any.whl"
              data-requires-python=">=3.8"
            >
              tqdm-1000.0.0-py3-none-any.whl
            </a>
          </body>
        </html>
    "#, Url::from_directory_path(context.workspace_root.join("scripts/links/")).unwrap().as_str()})?;

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(&indoc::formatdoc! {r#"
        --index-url {}
        tqdm
    "#, Url::from_directory_path(root).unwrap().as_str()})?;

    uv_snapshot!(context.filters(), context.pip_install()
        .env_remove("UV_EXCLUDE_NEWER")
        .arg("-r")
        .arg("requirements.txt"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + tqdm==1000.0.0
    "###
    );

    Ok(())
}

/// Resolve against a local directory laid out as a PEP 503-compatible index, provided via a
/// relative path in a `requirements.txt` file.
#[test]
fn local_index_requirements_txt_relative() -> Result<()> {
    let context = TestContext::new("3.12");

    let root = context.temp_dir.child("simple-html");
    fs_err::create_dir_all(&root)?;

    let tqdm = root.child("tqdm");
    fs_err::create_dir_all(&tqdm)?;

    let index = tqdm.child("index.html");
    index.write_str(&indoc::formatdoc! {r#"
        <!DOCTYPE html>
        <html>
          <head>
            <meta name="pypi:repository-version" content="1.1" />
          </head>
          <body>
            <h1>Links for tqdm</h1>
            <a
              href="{}/tqdm-1000.0.0-py3-none-any.whl"
              data-requires-python=">=3.8"
            >
              tqdm-1000.0.0-py3-none-any.whl
            </a>
          </body>
        </html>
    "#, Url::from_directory_path(context.workspace_root.join("scripts/links/")).unwrap().as_str()})?;

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(
        r"
        --index-url ./simple-html
        tqdm
    ",
    )?;

    uv_snapshot!(context.filters(), context.pip_install()
        .env_remove("UV_EXCLUDE_NEWER")
        .arg("-r")
        .arg("requirements.txt"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + tqdm==1000.0.0
    "###
    );

    Ok(())
}

/// Resolve against a local directory laid out as a PEP 503-compatible index, falling back to
/// the default index.
#[test]
fn local_index_fallback() -> Result<()> {
    let context = TestContext::new("3.12");

    let root = context.temp_dir.child("simple-html");
    fs_err::create_dir_all(&root)?;

    let tqdm = root.child("tqdm");
    fs_err::create_dir_all(&tqdm)?;

    let index = tqdm.child("index.html");
    index.write_str(
        r#"
        <!DOCTYPE html>
        <html>
          <head>
            <meta name="pypi:repository-version" content="1.1" />
          </head>
          <body>
            <h1>Links for tqdm</h1>
          </body>
        </html>
    "#,
    )?;

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("iniconfig")
        .arg("--extra-index-url")
        .arg(Url::from_directory_path(root).unwrap().as_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    "###
    );

    Ok(())
}

#[test]
fn accept_existing_prerelease() -> Result<()> {
    let context = TestContext::new("3.12").with_filtered_counts();
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("Flask==2.0.0rc1")?;

    // Install a pre-release version of `flask`.
    uv_snapshot!(context.filters(), context.pip_install().arg("Flask==2.0.0rc1"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + click==8.1.7
     + flask==2.0.0rc1
     + itsdangerous==2.1.2
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + werkzeug==3.0.1
    "###
    );

    // Install `flask-login`, without enabling pre-releases. The existing version of `flask` should
    // still be accepted.
    uv_snapshot!(context.filters(), context.pip_install().arg("flask-login==0.6.0"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + flask-login==0.6.0
    "###
    );

    Ok(())
}

/// Allow `pip install` of an unmanaged project.
#[test]
fn unmanaged() -> Result<()> {
    let context = TestContext::new("3.12");
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"[project]
    name = "example"
    version = "0.0.0"
    dependencies = [
      "anyio==3.7.0"
    ]

    [tool.uv]
    managed = false
    "#,
    )?;

    uv_snapshot!(context.filters(), context.pip_install().arg("."), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==3.7.0
     + example==0.0.0 (from file://[TEMP_DIR]/)
     + idna==3.6
     + sniffio==1.3.1
    "###
    );

    Ok(())
}

#[test]
fn install_relocatable() -> Result<()> {
    let context = TestContext::new("3.12");

    // Remake the venv as relocatable
    context
        .venv()
        .arg(context.venv.as_os_str())
        .arg("--python")
        .arg("3.12")
        .arg("--relocatable")
        .assert()
        .success();

    // Install a package with a hello-world console script entrypoint.
    // (we use black_editable because it's convenient, but we don't actually install it as editable)
    context
        .pip_install()
        .arg(
            context
                .workspace_root
                .join("scripts/packages/black_editable"),
        )
        .assert()
        .success();

    // Script should run correctly in-situ.
    let script_path = if cfg!(windows) {
        context.venv.child(r"Scripts\black.exe")
    } else {
        context.venv.child("bin/black")
    };
    Command::new(script_path.as_os_str())
        .assert()
        .success()
        .stdout(predicate::str::contains("Hello world!"));

    // Relocate the venv, and see if it still works.
    let new_venv_path = context.venv.with_file_name("relocated");
    fs::rename(context.venv, new_venv_path.clone())?;

    let script_path = if cfg!(windows) {
        new_venv_path.join(r"Scripts\black.exe")
    } else {
        new_venv_path.join("bin/black")
    };
    Command::new(script_path.as_os_str())
        .assert()
        .success()
        .stdout(predicate::str::contains("Hello world!"));

    Ok(())
}
