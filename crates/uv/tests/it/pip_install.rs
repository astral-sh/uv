use std::io::Cursor;
use std::process::Command;

use anyhow::Result;
use assert_cmd::prelude::*;
use assert_fs::prelude::*;
use flate2::write::GzEncoder;
use fs_err as fs;
use fs_err::File;
use indoc::indoc;
use predicates::prelude::predicate;
use url::Url;

#[cfg(feature = "git")]
use crate::common::{self, decode_token};

use crate::common::{
    build_vendor_links_url, get_bin, uv_snapshot, venv_bin_path, venv_to_interpreter, TestContext,
};
use uv_fs::Simplified;
use uv_static::EnvVars;

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
    warning: Requirements file `requirements.txt` does not contain any dependencies
    Audited in [TIME]
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
fn missing_find_links() -> Result<()> {
    let context = TestContext::new("3.12");
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("flask")?;

    let error = regex::escape("The system cannot find the path specified. (os error 3)");
    let filters = context
        .filters()
        .into_iter()
        .chain(std::iter::once((
            error.as_str(),
            "No such file or directory (os error 2)",
        )))
        .collect::<Vec<_>>();

    uv_snapshot!(filters, context.pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--find-links")
        .arg("./missing")
        .arg("--strict"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to read `--find-links` directory: [TEMP_DIR]/missing
      Caused by: No such file or directory (os error 2)
    "###
    );

    Ok(())
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
    warning: Failed to parse `pyproject.toml` during settings discovery:
      TOML parse error at line 1, column 5
        |
      1 | 123 - 456
        |     ^
      expected `.`, `=`

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
fn invalid_pyproject_toml_project_schema() -> Result<()> {
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
    `pyproject.toml` is using the `[project]` table, but the required `project.name` field is not set
    "###
    );

    Ok(())
}

#[test]
fn invalid_pyproject_toml_option_schema() -> Result<()> {
    let context = TestContext::new("3.12");
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r"
        [tool.uv]
        index-url = true
    "})?;

    uv_snapshot!(context.pip_install()
        .arg("iniconfig"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: Failed to parse `pyproject.toml` during settings discovery:
      TOML parse error at line 2, column 13
        |
      2 | index-url = true
        |             ^^^^
      invalid type: boolean `true`, expected a string

    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    "###
    );

    Ok(())
}

#[test]
fn invalid_pyproject_toml_option_unknown_field() -> Result<()> {
    let context = TestContext::new("3.12");
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [tool.uv]
        unknown = "field"

        [build-system]
        requires = ["setuptools"]
        build-backend = "setuptools.build_meta"
    "#})?;

    let mut filters = context.filters();
    filters.push((
        "expected one of `required-version`, `native-tls`, .*",
        "expected one of `required-version`, `native-tls`, [...]",
    ));

    uv_snapshot!(filters, context.pip_install()
        .arg("-r")
        .arg("pyproject.toml"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: Failed to parse `pyproject.toml` during settings discovery:
      TOML parse error at line 2, column 1
        |
      2 | unknown = "field"
        | ^^^^^^^
      unknown field `unknown`, expected one of `required-version`, `native-tls`, [...]

    Resolved in [TIME]
    Audited in [TIME]
    "###
    );

    Ok(())
}

#[test]
fn invalid_uv_toml_option_disallowed() -> Result<()> {
    let context = TestContext::new("3.12");
    let uv_toml = context.temp_dir.child("uv.toml");
    uv_toml.write_str(indoc! {r"
        managed = true
    "})?;

    uv_snapshot!(context.pip_install()
        .arg("iniconfig"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to parse: `uv.toml`. The `managed` field is not allowed in a `uv.toml` file. `managed` is only applicable in the context of a project, and should be placed in a `pyproject.toml` file instead.
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
version = "0.1.0"
dependencies = ["flask==1.0.x"]
"#,
    )?;
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("./path_dep")?;

    let filters = std::iter::once((r"exit code: 1", "exit status: 1"))
        .chain(context.filters())
        .collect::<Vec<_>>();
    uv_snapshot!(filters, context.pip_install()
        .arg("-r")
        .arg("requirements.txt"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to build `project @ file://[TEMP_DIR]/path_dep`
      ├─▶ The build backend returned an error
      ╰─▶ Call to `setuptools.build_meta:__legacy__.build_wheel` failed (exit status: 1)

          [stdout]
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

          [stderr]
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
            File "[CACHE_DIR]/builds-v0/[TMP]/_virtualenv.py", line 20, in parse_config_files
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

          hint: This usually indicates a problem with the package or the build environment.
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
          And because you require flask>=3.0.2 and werkzeug<1.0.0, we can conclude that your requirements are unsatisfiable.
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

/// Warn (but don't fail) when unsupported flags are set in the `requirements.txt`.
#[test]
fn install_unsupported_flag() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(indoc! {r"
        --pre
        --prefer-binary :all:
        iniconfig
    "})?;

    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: Ignoring unsupported option in `requirements.txt`: `--pre` (hint: pass `--pre` on the command line instead)
    warning: Ignoring unsupported option in `requirements.txt`: `--prefer-binary`
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    "###
    );

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
          And because you require flask==3.0.2, we can conclude that your requirements are unsatisfiable.
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
     ~ flask==3.0.2
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
    warning: Failed to uninstall package at [SITE_PACKAGES]/anyio-3.7.0.dist-info due to missing `RECORD` file. Installation may result in an incomplete environment.
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - anyio==3.7.0
     + anyio==4.0.0
    "###
    );

    Ok(())
}

#[test]
fn exact_install_removes_extraneous_packages() -> Result<()> {
    let context = TestContext::new("3.12").with_filtered_counts();
    // Install anyio
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("anyio==3.7.0")?;

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--exact")
        .arg("-r")
        .arg("requirements.txt"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + anyio==3.7.0
     + idna==3.6
     + sniffio==1.3.1
    "###
    );

    // Install flask
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("flask"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + blinker==1.7.0
     + click==8.1.7
     + flask==3.0.2
     + itsdangerous==2.1.2
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + werkzeug==3.0.1
    "###
    );

    // Install requirements file with exact flag removes flask and flask dependencies.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--exact")
        .arg("-r")
        .arg("requirements.txt"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Uninstalled [N] packages in [TIME]
     - blinker==1.7.0
     - click==8.1.7
     - flask==3.0.2
     - itsdangerous==2.1.2
     - jinja2==3.1.3
     - markupsafe==2.1.5
     - werkzeug==3.0.1
    "###
    );

    // Install flask again
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("flask"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + blinker==1.7.0
     + click==8.1.7
     + flask==3.0.2
     + itsdangerous==2.1.2
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + werkzeug==3.0.1
    "###
    );

    requirements_txt.write_str(indoc! {r"
        anyio==3.7.0
        flit_core<4.0.0
        "
    })?;

    // Install requirements file with exact flag installs flit_core and removes flask and flask dependencies.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--exact")
        .arg("-r")
        .arg("requirements.txt"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Uninstalled [N] packages in [TIME]
    Installed [N] packages in [TIME]
     - blinker==1.7.0
     - click==8.1.7
     - flask==3.0.2
     + flit-core==3.9.0
     - itsdangerous==2.1.2
     - jinja2==3.1.3
     - markupsafe==2.1.5
     - werkzeug==3.0.1
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
fn install_extras() -> Result<()> {
    let context = TestContext::new("3.12");

    // Request extras for an editable path
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--all-extras")
        .arg("-e")
        .arg(context.workspace_root.join("scripts/packages/poetry_editable")), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Requesting extras requires a `pyproject.toml`, `setup.cfg`, or `setup.py` file. Use `<dir>[extra]` syntax or `-r <file>` instead.
    "###
    );

    // Request extras for a source tree
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--all-extras")
        .arg(context.workspace_root.join("scripts/packages/poetry_editable")), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Requesting extras requires a `pyproject.toml`, `setup.cfg`, or `setup.py` file. Use `package[extra]` syntax instead.
    "###
    );

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("anyio==3.7.0")?;

    // Request extras for a requirements file
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--all-extras")
        .arg("-r").arg("requirements.txt"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Requesting extras requires a `pyproject.toml`, `setup.cfg`, or `setup.py` file. Use `package[extra]` syntax instead.
    "###
    );

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
[project]
name = "project"
version = "0.1.0"
dependencies = ["anyio==3.7.0"]
"#,
    )?;

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--all-extras")
        .arg("-r").arg("pyproject.toml"), @r###"
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

    // Install it again.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-e")
        .arg(context.workspace_root.join("scripts/packages/poetry_editable")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     ~ poetry-editable==0.1.0 (from file://[WORKSPACE]/scripts/packages/poetry_editable)
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
    Prepared 7 packages in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 7 packages in [TIME]
     + black==24.3.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
     ~ poetry-editable==0.1.0 (from file://[WORKSPACE]/scripts/packages/poetry_editable)
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
      ╰─▶ Because only black<=0.1.0 is available and you require black>0.1.0, we can conclude that your requirements are unsatisfiable.
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

    requirements_txt.write_str(&indoc::formatdoc! {r"
        --editable black[d] @ file://{workspace_root}/scripts/packages/black_editable
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
    Audited 1 package in [TIME]
    "###
    );

    requirements_txt.write_str(&indoc::formatdoc! {r"
        --editable=black[d] @ file://{workspace_root}/scripts/packages/black_editable
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
    Audited 1 package in [TIME]
    "###
    );

    requirements_txt.write_str(&indoc::formatdoc! {r"
        --editable= black[d] @ file://{workspace_root}/scripts/packages/black_editable
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
    Audited 1 package in [TIME]
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
    Using Python 3.12.[X] environment at: [VENV]/
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
    Using Python 3.12.[X] environment at: [VENV]/
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
      ╰─▶ Because flask was not found in the provided package locations and you require flask, we can conclude that your requirements are unsatisfiable.

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
      ╰─▶ Because flask was not found in the provided package locations and you require flask==3.0.0, we can conclude that your requirements are unsatisfiable.

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
    let context = TestContext::new("3.12").with_exclude_newer("2024-03-09T00:00:00Z");

    uv_snapshot!(context.pip_install()
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
        .arg("--no-deps"), @r###"
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

/// Install a package from a public GitHub repository, omitting the `git+` prefix
#[test]
#[cfg(feature = "git")]
fn install_implicit_git_public_https() {
    let context = TestContext::new("3.8");

    uv_snapshot!(
        context
        .pip_install()
        .arg("uv-public-pypackage @ https://github.com/astral-test/uv-public-pypackage.git"),
        @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + uv-public-pypackage==0.1.0 (from git+https://github.com/astral-test/uv-public-pypackage.git@b270df1a2fb5d012294e9aaf05e7e0bab1e6a389)
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
    filters.push(("`.*/git(.exe)? fetch .*`", "`git fetch [...]`"));
    filters.push(("exit status", "exit code"));

    uv_snapshot!(filters, context.pip_install()
        // 2.0.0 does not exist
        .arg("uv-public-pypackage @ git+https://github.com/astral-test/uv-public-pypackage@2.0.0"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to download and build `uv-public-pypackage @ git+https://github.com/astral-test/uv-public-pypackage@2.0.0`
      ├─▶ Git operation failed
      ├─▶ failed to clone into: [CACHE_DIR]/git-v0/db/8dab139913c4b566
      ├─▶ failed to fetch branch or tag `2.0.0`
      ╰─▶ process didn't exit successfully: `git fetch [...]` (exit code: 128)
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
    filters.push(("`.*/git(.exe)? rev-parse .*`", "`git rev-parse [...]`"));
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
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to download and build `uv-public-pypackage @ git+https://github.com/astral-test/uv-public-pypackage@79a935a7a1a0ad6d0bdf72dce0e16cb0a24a1b3b`
      ├─▶ Git operation failed
      ├─▶ failed to find branch, tag, or commit `79a935a7a1a0ad6d0bdf72dce0e16cb0a24a1b3b`
      ╰─▶ process didn't exit successfully: `git rev-parse [...]` (exit code: 128)
          --- stdout
          79a935a7a1a0ad6d0bdf72dce0e16cb0a24a1b3b^0

          --- stderr
          fatal: ambiguous argument '79a935a7a1a0ad6d0bdf72dce0e16cb0a24a1b3b^0': unknown revision or path not in the working tree.
          Use '--' to separate paths from revisions, like this:
          'git <command> [<revision>...] -- [<file>...]'
    "###);
}

/// Install a package from a private GitHub repository using a PAT
#[test]
#[cfg(all(not(windows), feature = "git"))]
fn install_git_private_https_pat() {
    use crate::common::decode_token;

    let context = TestContext::new("3.8");
    let token = decode_token(common::READ_ONLY_GITHUB_TOKEN);

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
    let token = decode_token(common::READ_ONLY_GITHUB_TOKEN);

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
    let token_1 = decode_token(common::READ_ONLY_GITHUB_TOKEN);
    let token_2 = decode_token(common::READ_ONLY_GITHUB_TOKEN_2);

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
    let token = decode_token(common::READ_ONLY_GITHUB_TOKEN);

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
    let token = decode_token(common::READ_ONLY_GITHUB_TOKEN);
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
    filters.push(("`.*/git fetch (.*)`", "`git fetch $1`"));

    // We provide a username otherwise (since the token is invalid), the git cli will prompt for a password
    // and hang the test
    uv_snapshot!(filters, context.pip_install()
        .arg(format!("uv-private-pypackage @ git+https://git:{token}@github.com/astral-test/uv-private-pypackage"))
        , @r"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to download and build `uv-private-pypackage @ git+https://git:***@github.com/astral-test/uv-private-pypackage`
      ├─▶ Git operation failed
      ├─▶ failed to clone into: [CACHE_DIR]/git-v0/db/8401f5508e3e612d
      ╰─▶ process didn't exit successfully: `git fetch --force --update-head-ok 'https://git:***@github.com/astral-test/uv-private-pypackage' '+HEAD:refs/remotes/origin/HEAD'` (exit status: 128)
          --- stderr
          remote: Support for password authentication was removed on August 13, 2021.
          remote: Please see https://docs.github.com/get-started/getting-started-with-git/about-remote-repositories#cloning-with-https-urls for information on currently recommended modes of authentication.
          fatal: Authentication failed for 'https://github.com/astral-test/uv-private-pypackage/'
    ");
}

/// Install a package from a private GitHub repository using a PAT
/// Does not use `git`, instead installs a distribution artifact.
/// Include a public GitHub repository too, to ensure that the authentication is not erroneously copied over.
#[test]
#[cfg(not(windows))]
fn install_github_artifact_private_https_pat_mixed_with_public() {
    let context = TestContext::new("3.8");
    let token = decode_token(common::READ_ONLY_GITHUB_TOKEN);

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
    let token_1 = decode_token(common::READ_ONLY_GITHUB_TOKEN);
    let token_2 = decode_token(common::READ_ONLY_GITHUB_TOKEN_2);

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

/// Fail to a package from a private GitHub repository using interactive authentication
/// It should fail gracefully, instead of silently hanging forever
/// Regression test for <https://github.com/astral-sh/uv/issues/5107>
#[test]
#[cfg(feature = "git")]
fn install_git_private_https_interactive() {
    let context = TestContext::new("3.8");

    let package = "uv-private-pypackage@ git+https://github.com/astral-test/uv-private-pypackage";

    // The path to a git binary may be arbitrary, filter and replace
    // The trailing space is load bearing, as to not match on false positives
    let filters: Vec<_> = [("\\/([[:alnum:]]*\\/)*git ", "/usr/bin/git ")]
        .into_iter()
        .chain(context.filters())
        .collect();

    uv_snapshot!(filters, context.pip_install().arg(package)
        , @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to download and build `uv-private-pypackage @ git+https://github.com/astral-test/uv-private-pypackage`
      ├─▶ Git operation failed
      ├─▶ failed to clone into: [CACHE_DIR]/git-v0/db/8401f5508e3e612d
      ╰─▶ process didn't exit successfully: `/usr/bin/git fetch --force --update-head-ok 'https://github.com/astral-test/uv-private-pypackage' '+HEAD:refs/remotes/origin/HEAD'` (exit status: 128)
          --- stderr
          fatal: could not read Username for 'https://github.com': terminal prompts disabled
    "###);
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
     ~ anyio==4.3.0
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

/// Disable binaries with an environment variable
/// TODO(zanieb): This is not yet implemented
#[test]
fn install_no_binary_env() {
    let context = TestContext::new("3.12");

    let mut command = context.pip_install();
    command.arg("anyio").env("UV_NO_BINARY", "1");
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

    let mut command = context.pip_install();
    command
        .arg("anyio")
        .arg("--reinstall")
        .env("UV_NO_BINARY", "anyio");
    uv_snapshot!(
        command,
        @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Uninstalled 3 packages in [TIME]
    Installed 3 packages in [TIME]
     ~ anyio==4.3.0
     ~ idna==3.6
     ~ sniffio==1.3.1
    "###
    );

    context.assert_command("import anyio").success();

    let mut command = context.pip_install();
    command
        .arg("anyio")
        .arg("--reinstall")
        .arg("idna")
        .env("UV_NO_BINARY_PACKAGE", "idna");
    uv_snapshot!(
        command,
        @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Uninstalled 3 packages in [TIME]
    Installed 3 packages in [TIME]
     ~ anyio==4.3.0
     ~ idna==3.6
     ~ sniffio==1.3.1
    "###
    );

    context.assert_command("import idna").success();
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
      ╰─▶ Because all versions of anyio have no usable wheels and you require anyio, we can conclude that your requirements are unsatisfiable.

          hint: Pre-releases are available for `anyio` in the requested range (e.g., 4.0.0rc1), but pre-releases weren't enabled (try: `--prerelease=allow`)

          hint: Wheels are required for `anyio` because building from source is disabled for all packages (i.e., with `--no-build`)
    "###
    );

    context.assert_command("import anyio").failure();
}

/// Binary dependencies in the cache should be reused when the user provides `--no-build`.
#[test]
fn install_no_binary_cache() {
    let context = TestContext::new("3.12");

    // Install a binary distribution.
    uv_snapshot!(
        context.pip_install().arg("idna"),
        @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + idna==3.6
    "###
    );

    // Re-create the virtual environment.
    context.venv().assert().success();

    // Re-install. The distribution should be installed from the cache.
    uv_snapshot!(
        context.pip_install().arg("idna"),
        @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Installed 1 package in [TIME]
     + idna==3.6
    "###
    );

    // Re-create the virtual environment.
    context.venv().assert().success();

    // Install with `--no-binary`. The distribution should be built from source, despite a binary
    // distribution being available in the cache.
    uv_snapshot!(
        context.pip_install().arg("idna").arg("--no-binary").arg(":all:"),
        @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + idna==3.6
    "###
    );
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
      ╰─▶ Because django-allauth==0.51.0 has no usable wheels and you require django-allauth==0.51.0, we can conclude that your requirements are unsatisfiable.

          hint: Wheels are required for `django-allauth` because building from source is disabled for `django-allauth` (i.e., with `--no-build-package django-allauth`)
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

/// We should not recommend `--prerelease=allow` in source distribution build failures, since we
/// don't propagate the `--prerelease` flag to the source distribution build regardless.
#[test]
fn no_prerelease_hint_source_builds() -> Result<()> {
    let context = TestContext::new("3.12").with_exclude_newer("2018-10-08");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["setuptools>=40.8.0"]
        build-backend = "setuptools.build_meta"
    "#})?;

    uv_snapshot!(context.filters(), context.pip_install().arg("."), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
      × Failed to build `project @ file://[TEMP_DIR]/`
      ├─▶ Failed to resolve requirements from `setup.py` build
      ├─▶ No solution found when resolving: `setuptools>=40.8.0`
      ╰─▶ Because only setuptools<40.8.0 is available and you require setuptools>=40.8.0, we can conclude that your requirements are unsatisfiable.
    "###
    );

    Ok(())
}

#[test]
fn cache_priority() {
    let context = TestContext::new("3.12");

    // Install a specific `idna` version.
    uv_snapshot!(
        context.pip_install().arg("idna==3.6"),
        @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + idna==3.6
    "###
    );

    // Install a lower `idna` version.
    uv_snapshot!(
        context.pip_install().arg("idna==3.0"),
        @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - idna==3.6
     + idna==3.0
    "###
    );

    // Re-create the virtual environment.
    context.venv().assert().success();

    // Install `idna` without a version specifier.
    uv_snapshot!(
        context.pip_install().arg("idna"),
        @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Installed 1 package in [TIME]
     + idna==3.6
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

/// Avoid downgrading already-installed packages when `--upgrade` is provided.
#[test]
fn install_no_downgrade() -> Result<()> {
    let context = TestContext::new("3.12");

    // Create a local package named `idna`.
    let idna = context.temp_dir.child("idna");
    idna.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "idna"
        version = "1000"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
    "#})?;

    // Install the local `idna`.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("./idna"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + idna==1000 (from file://[TEMP_DIR]/idna)
    "###
    );

    // Install `anyio`, which depends on `idna`.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("anyio"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + anyio==4.3.0
     + sniffio==1.3.1
    "###
    );

    // Install `anyio` with `--upgrade`, which should retain the local `idna`.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-U")
        .arg("anyio"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Audited 3 packages in [TIME]
    "###
    );

    // Install `anyio` with `--reinstall`, which should downgrade `idna`.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--reinstall")
        .arg("anyio"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Uninstalled 3 packages in [TIME]
    Installed 3 packages in [TIME]
     ~ anyio==4.3.0
     - idna==1000 (from file://[TEMP_DIR]/idna)
     + idna==3.6
     ~ sniffio==1.3.1
    "###
    );

    Ok(())
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

#[test]
#[cfg(feature = "git")]
fn install_git_source_respects_offline_mode() {
    let context = TestContext::new("3.12");

    uv_snapshot!(context.filters(), context.pip_install()
            .arg("--offline")
            .arg("uv-public-pypackage @ git+https://github.com/astral-test/uv-public-pypackage"), @r"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to download and build `uv-public-pypackage @ git+https://github.com/astral-test/uv-public-pypackage`
      ├─▶ Git operation failed
      ├─▶ failed to clone into: [CACHE_DIR]/git-v0/db/8dab139913c4b566
      ╰─▶ Remote Git fetches are not allowed because network connectivity is disabled (i.e., with `--offline`)
    "
    );
}

/// Test that constraint markers are respected when validating the current environment (i.e., we
/// skip resolution entirely).
#[test]
fn install_constraints_with_markers() -> Result<()> {
    let context = TestContext::new("3.12");
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("pytest")?;

    // Create a constraints file with a marker that is not relevant to the current environment.
    let constraints_txt = context.temp_dir.child("constraints.txt");
    constraints_txt.write_str("pytest==8.0.0; sys_platform == 'nonexistent-platform'")?;

    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--constraint")
        .arg("constraints.txt"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + iniconfig==2.0.0
     + packaging==24.0
     + pluggy==1.4.0
     + pytest==8.1.1
    "###
    );

    // We should only see "Audited" here; no need to resolve.
    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--constraint")
        .arg("constraints.txt"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
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
fn config_settings_registry() {
    let context = TestContext::new("3.12");

    // Install with a `-C` flag. In this case, the flag has no impact on the build, but uv should
    // respect it anyway.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("iniconfig")
        .arg("--no-binary")
        .arg("iniconfig")
        .arg("-C=global-option=build_ext"), @r###"
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

    // Uninstall the package.
    uv_snapshot!(context.filters(), context.pip_uninstall()
        .arg("iniconfig"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Uninstalled 1 package in [TIME]
     - iniconfig==2.0.0
    "###);

    // Re-install the package, with the same flag. We should read from the cache.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("iniconfig")
        .arg("--no-binary")
        .arg("iniconfig")
        .arg("-C=global-option=build_ext"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    "###
    );

    // Uninstall the package.
    uv_snapshot!(context.filters(), context.pip_uninstall()
        .arg("iniconfig"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Uninstalled 1 package in [TIME]
     - iniconfig==2.0.0
    "###);

    // Re-install the package, without the flag. We should build it from source.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("iniconfig")
        .arg("--no-binary")
        .arg("iniconfig"), @r###"
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
}

#[test]
fn config_settings_path() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(&format!(
        "-e {}",
        context
            .workspace_root
            .join("scripts/packages/setuptools_editable")
            .display()
    ))?;

    // Install the editable package.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-r")
        .arg("requirements.txt"), @r###"
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

    // Reinstalling with `--editable_mode=compat` should be a no-op; changes in build configuration
    // don't invalidate the environment.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("-C")
        .arg("editable_mode=compat")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    "###
    );

    // Uninstall the package.
    uv_snapshot!(context.filters(), context.pip_uninstall()
        .arg("setuptools-editable"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Uninstalled 1 package in [TIME]
     - setuptools-editable==0.1.0 (from file://[WORKSPACE]/scripts/packages/setuptools_editable)
    "###);

    // Install the editable package with `--editable_mode=compat`. We should ignore the cached
    // build configuration and rebuild.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("-C")
        .arg("editable_mode=compat")
        , @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + setuptools-editable==0.1.0 (from file://[WORKSPACE]/scripts/packages/setuptools_editable)
    "###
    );

    // When installed without `--editable_mode=compat`, the `finder.py` file should _not_ be present.
    let finder = context
        .site_packages()
        .join("__editable___setuptools_editable_0_1_0_finder.py");
    assert!(!finder.exists());

    Ok(())
}

/// Reinstall a duplicate package in a virtual environment.
#[test]
fn reinstall_duplicate() -> Result<()> {
    use uv_fs::copy_dir_all;

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

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("-e ./editable")?;

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
        .arg("-r")
        .arg("requirements.txt"), @r###"
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

    // Installing again should be a no-op.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-r")
        .arg("requirements.txt"), @r###"
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

    // Installing again should update the package.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-r")
        .arg("requirements.txt"), @r###"
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
     ~ example==0.0.0 (from file://[TEMP_DIR]/editable)
    "###
    );

    Ok(())
}

#[test]
fn editable_dynamic() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("-e ./editable")?;

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
dependencies = {file = ["dependencies.txt"]}
"#,
    )?;

    let dependencies_txt = editable_dir.child("dependencies.txt");
    dependencies_txt.write_str("anyio==4.0.0")?;

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-r")
        .arg("requirements.txt"), @r###"
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

    // Installing again should not re-install, as we don't special-case dynamic metadata.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-r")
        .arg("requirements.txt"), @r###"
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

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("example @ ./editable")?;

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
        .arg("-r")
        .arg("requirements.txt"), @r###"
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

    // Installing again should be a no-op.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-r")
        .arg("requirements.txt"), @r###"
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

    // Installing again should update the package.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-r")
        .arg("requirements.txt"), @r###"
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
     ~ example==0.0.0 (from file://[TEMP_DIR]/editable)
    "###
    );

    Ok(())
}

#[test]
fn invalidate_path_on_cache_key() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("example @ ./editable")?;

    // Create a local package.
    let editable_dir = context.temp_dir.child("editable");
    editable_dir.create_dir_all()?;
    let pyproject_toml = editable_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"[project]
        name = "example"
        version = "0.0.0"
        dependencies = ["anyio==4.0.0"]
        requires-python = ">=3.8"

        [tool.uv]
        cache-keys = ["constraints.txt", { file = "overrides.txt" }]
"#,
    )?;

    let overrides_txt = editable_dir.child("overrides.txt");
    overrides_txt.write_str("idna")?;

    let constraints_txt = editable_dir.child("constraints.txt");
    constraints_txt.write_str("idna<3.4")?;

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-r")
        .arg("requirements.txt"), @r###"
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

    // Installing again should be a no-op.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-r")
        .arg("requirements.txt"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    "###
    );

    // Modify the constraints file.
    constraints_txt.write_str("idna<3.5")?;

    // Installing again should update the package.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-r")
        .arg("requirements.txt"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     ~ example==0.0.0 (from file://[TEMP_DIR]/editable)
    "###
    );

    // Modify the requirements file.
    overrides_txt.write_str("flask")?;

    // Installing again should update the package.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-r")
        .arg("requirements.txt"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     ~ example==0.0.0 (from file://[TEMP_DIR]/editable)
    "###
    );

    // Modify the `pyproject.toml` file (but not in a meaningful way).
    pyproject_toml.write_str(
        r#"[project]
        name = "example"
        version = "0.0.0"
        dependencies = ["anyio==4.0.0"]
        requires-python = ">=3.8"

        [tool.uv]
        cache-keys = [{ file = "overrides.txt" }, "constraints.txt"]
"#,
    )?;

    // Installing again should be a no-op, since `pyproject.toml` was not included as a cache key.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-r")
        .arg("requirements.txt"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    "###
    );

    // Modify the `pyproject.toml` to use a glob.
    pyproject_toml.write_str(
        r#"[project]
        name = "example"
        version = "0.0.0"
        dependencies = ["anyio==4.0.0"]
        requires-python = ">=3.8"

        [tool.uv]
        cache-keys = [{ file = "**/*.txt" }]
"#,
    )?;

    // Write a new file.
    editable_dir
        .child("resources")
        .child("data.txt")
        .write_str("data")?;

    // Installing again should update the package.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-r")
        .arg("requirements.txt"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     ~ example==0.0.0 (from file://[TEMP_DIR]/editable)
    "###
    );

    // Write a new file in the current directory.
    editable_dir.child("data.txt").write_str("data")?;

    // Installing again should update the package.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-r")
        .arg("requirements.txt"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     ~ example==0.0.0 (from file://[TEMP_DIR]/editable)
    "###
    );

    Ok(())
}

#[test]
fn invalidate_path_on_commit() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("example @ ./editable")?;

    // Create a local package.
    let editable_dir = context.temp_dir.child("editable");
    editable_dir.create_dir_all()?;

    let pyproject_toml = editable_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "example"
        version = "0.0.0"
        dependencies = ["anyio==4.0.0"]
        requires-python = ">=3.8"

        [tool.uv]
        cache-keys = [{ git = true }]
        "#,
    )?;

    // Create a Git repository.
    context
        .temp_dir
        .child(".git")
        .child("HEAD")
        .write_str("ref: refs/heads/main")?;
    context
        .temp_dir
        .child(".git")
        .child("refs")
        .child("heads")
        .child("main")
        .write_str("1b6638fdb424e993d8354e75c55a3e524050c857")?;

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-r")
        .arg("requirements.txt"), @r###"
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

    // Installing again should be a no-op.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-r")
        .arg("requirements.txt"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    "###
    );

    // Change the current commit.
    context
        .temp_dir
        .child(".git")
        .child("refs")
        .child("heads")
        .child("main")
        .write_str("a1a42cbd10d83bafd8600ba81f72bbef6c579385")?;

    // Installing again should update the package.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-r")
        .arg("requirements.txt"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     ~ example==0.0.0 (from file://[TEMP_DIR]/editable)
    "###
    );

    Ok(())
}

#[test]
fn invalidate_path_on_env_var() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(".")?;

    // Create a local package.
    context.temp_dir.child("pyproject.toml").write_str(
        r#"[project]
        name = "example"
        version = "0.0.0"
        dependencies = ["anyio==4.0.0"]
        requires-python = ">=3.8"

        [tool.uv]
        cache-keys = [{ env = "FOO" }]
"#,
    )?;

    // Install the package.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .env_remove("FOO"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.0.0
     + example==0.0.0 (from file://[TEMP_DIR]/)
     + idna==3.6
     + sniffio==1.3.1
    "###
    );

    // Installing again should be a no-op.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .env_remove("FOO"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    "###
    );

    // Installing again should update the package.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .env("FOO", "BAR"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     ~ example==0.0.0 (from file://[TEMP_DIR]/)
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
requires-python = ">=3.13"
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
      ╰─▶ Because the current Python version (3.12.[X]) does not satisfy Python>=3.13 and example==0.0.0 depends on Python>=3.13, we can conclude that example==0.0.0 cannot be used.
          And because only example==0.0.0 is available and you require example, we can conclude that your requirements are unsatisfiable.
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
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to build `anyio @ https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz`
      ├─▶ The build backend returned an error
      ╰─▶ Call to `setuptools.build_meta.prepare_metadata_for_build_wheel` failed (exit status: 1)

          [stderr]
          Traceback (most recent call last):
            File "<string>", line 8, in <module>
          ModuleNotFoundError: No module named 'setuptools'

          hint: This usually indicates a problem with the package or the build environment.
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
        .env(EnvVars::UV_NO_BUILD_ISOLATION, "yes"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to build `anyio @ https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz`
      ├─▶ The build backend returned an error
      ╰─▶ Call to `setuptools.build_meta.prepare_metadata_for_build_wheel` failed (exit status: 1)

          [stderr]
          Traceback (most recent call last):
            File "<string>", line 8, in <module>
          ModuleNotFoundError: No module named 'setuptools'

          hint: This usually indicates a problem with the package or the build environment.
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
        .env(EnvVars::UV_NO_BUILD_ISOLATION, "yes"), @r###"
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
    let context = TestContext::new("3.12").with_exclude_newer("2025-01-01T00:00:00Z");
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_binary(&utf8_to_utf16_with_bom_le("tomli<=2.0.1"))?;

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
    let context = TestContext::new("3.12").with_exclude_newer("2025-01-01T00:00:00Z");
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_binary(&utf8_to_utf16_with_bom_be("tomli<=2.0.1"))?;

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
requires-python = ">=3.13"
"#,
    )?;

    uv_snapshot!(context.filters(), context.pip_install()
        .arg(format!("example @ {}", editable_dir.path().display())), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because the current Python version (3.12.[X]) does not satisfy Python>=3.13 and example==0.0.0 depends on Python>=3.13, we can conclude that example==0.0.0 cannot be used.
          And because only example==0.0.0 is available and you require example, we can conclude that your requirements are unsatisfiable.
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
        .env(EnvVars::NETRC, netrc.to_str().unwrap())
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
        .env(EnvVars::NETRC, netrc.to_str().unwrap())
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
        .env(EnvVars::NETRC, netrc.to_str().unwrap())
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
        .env(EnvVars::KEYRING_TEST_CREDENTIALS, r#"{"pypi-proxy.fly.dev": {"public": "heron"}}"#)
        .env(EnvVars::PATH, venv_bin_path(&context.venv)), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Keyring request for public@https://pypi-proxy.fly.dev/basic-auth/simple
    Keyring request for public@pypi-proxy.fly.dev
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + sniffio==1.3.1
    "
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
        .env(EnvVars::KEYRING_TEST_CREDENTIALS, r#"{"pypi-proxy.fly.dev": {"public": "foobar"}}"#)
        .env(EnvVars::PATH, venv_bin_path(&context.venv)), @r"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Keyring request for public@https://pypi-proxy.fly.dev/basic-auth/simple
    Keyring request for public@pypi-proxy.fly.dev
      × No solution found when resolving dependencies:
      ╰─▶ Because anyio was not found in the package registry and you require anyio, we can conclude that your requirements are unsatisfiable.

          hint: An index URL (https://pypi-proxy.fly.dev/basic-auth/simple) could not be queried due to a lack of valid authentication credentials (401 Unauthorized).
    "
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
        .env(EnvVars::KEYRING_TEST_CREDENTIALS, r#"{"pypi-proxy.fly.dev": {"other": "heron"}}"#)
        .env(EnvVars::PATH, venv_bin_path(&context.venv)), @r"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Keyring request for public@https://pypi-proxy.fly.dev/basic-auth/simple
    Keyring request for public@pypi-proxy.fly.dev
      × No solution found when resolving dependencies:
      ╰─▶ Because anyio was not found in the package registry and you require anyio, we can conclude that your requirements are unsatisfiable.

          hint: An index URL (https://pypi-proxy.fly.dev/basic-auth/simple) could not be queried due to a lack of valid authentication credentials (401 Unauthorized).
    "
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
    Using Python 3.12.[X] environment at: [VENV]/
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
      ╰─▶ Because anyio was not found in the provided package locations and you require anyio, we can conclude that your requirements are unsatisfiable.

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
    // We should reinstall the package because it was explicitly requested
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-e")
        .arg(root_path.join("first_local")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     ~ first-local==0.1.0 (from file://[WORKSPACE]/scripts/packages/dependent_locals/first_local)
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
          And because only second-local==0.1.0 is available and you require second-local, we can conclude that your requirements are unsatisfiable.
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
     ~ first-local==0.1.0 (from file://[WORKSPACE]/scripts/packages/dependent_locals/first_local)
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

    // Request install of the first local by full path again.
    // We should rebuild and reinstall it.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg(root_path.join("first_local")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     ~ first-local==0.1.0 (from file://[WORKSPACE]/scripts/packages/dependent_locals/first_local)
    "###
    );

    // Request install of the first local by full path again, along with its name.
    // We should rebuild and reinstall it.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg(format!("first-local @ {}", root_path.join("first_local").display())), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     ~ first-local==0.1.0 (from file://[WORKSPACE]/scripts/packages/dependent_locals/first_local)
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
          And because only second-local==0.1.0 is available and you require second-local, we can conclude that your requirements are unsatisfiable.
    "###
    );

    // Request reinstallation of the first package
    // We include it in the install command with a full path so we succeed
    uv_snapshot!(context.filters(), context.pip_install()
        .arg(root_path.join("second_local"))
        .arg(root_path.join("first_local"))
        .arg("--reinstall-package")
        .arg("first-local"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Uninstalled 2 packages in [TIME]
    Installed 2 packages in [TIME]
     ~ first-local==0.1.0 (from file://[WORKSPACE]/scripts/packages/dependent_locals/first_local)
     ~ second-local==0.1.0 (from file://[WORKSPACE]/scripts/packages/dependent_locals/second_local)
    "
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
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     ~ second-local==0.1.0 (from file://[WORKSPACE]/scripts/packages/dependent_locals/second_local)
    "###
    );

    // Request upgrade of the first package
    // A full path is specified and there's nothing to upgrade, but because it was passed
    // explicitly, we reinstall
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
    Prepared 2 packages in [TIME]
    Uninstalled 2 packages in [TIME]
    Installed 2 packages in [TIME]
     ~ first-local==0.1.0 (from file://[WORKSPACE]/scripts/packages/dependent_locals/first_local)
     ~ second-local==0.1.0 (from file://[WORKSPACE]/scripts/packages/dependent_locals/second_local)
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
      ╰─▶ Because anyio was not found in the provided package locations and you require anyio==4.2.0, we can conclude that your requirements are unsatisfiable.

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
      ╰─▶ Because there is no version of anyio==4.3.0+foo and you require anyio==4.3.0+foo, we can conclude that your requirements are unsatisfiable.
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
     ~ anyio==4.3.0+foo (from file://[WORKSPACE]/scripts/packages/anyio_local)
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
    Prepared 1 package in [TIME]
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
    Resolved 1 package in [TIME]
    Audited 1 package in [TIME]
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
        use uv_fs::copy_dir_all;

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
     ~ anyio==4.0.0
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
    Resolved 1 package in [TIME]
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
      ╰─▶ Because uv-public-pypackage was not found in the provided package locations and you require uv-public-pypackage, we can conclude that your requirements are unsatisfiable.

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
     ~ uv-public-pypackage==0.1.0 (from git+https://github.com/astral-test/uv-public-pypackage@b270df1a2fb5d012294e9aaf05e7e0bab1e6a389)
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
      ╰─▶ Because uv-public-pypackage was not found in the provided package locations and you require uv-public-pypackage==0.2.0, we can conclude that your requirements are unsatisfiable.

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
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
      × Failed to download `anyio==4.0.0`
      ╰─▶ Hash mismatch for `anyio==4.0.0`

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
    error: In `--require-hashes` mode, all requirements must have a hash, but none were provided for: file://[WORKSPACE]/scripts/packages/black_editable[d]
    "###
    );

    Ok(())
}

/// If a hash is only included as a constraint, that's good enough for `--require-hashes`.
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
        .arg("--no-deps")
        .arg("--require-hashes")
        .arg("-c")
        .arg(constraints_txt.path()), @r###"
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

    // Include the hash in the requirements file, but pin the version in the constraint file.
    let context = TestContext::new("3.12");

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
        .arg("--no-deps")
        .arg("--require-hashes")
        .arg("-c")
        .arg(constraints_txt.path()), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: In `--require-hashes` mode, all requirements must have their versions pinned with `==`, but found: anyio
    "###
    );

    // Include an empty intersection. This should fail.
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(
        "anyio==4.0.0 --hash=sha256:afdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f",
    )?;

    let constraints_txt = context.temp_dir.child("constraints.txt");
    constraints_txt.write_str("anyio==4.0.0 --hash=sha256:cfdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f")?;

    // Install the editable packages.
    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg(requirements_txt.path())
        .arg("--no-deps")
        .arg("--require-hashes")
        .arg("-c")
        .arg(constraints_txt.path()), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: In `--require-hashes` mode, all requirements must have a hash, but there were no overlapping hashes between the requirements and constraints for: anyio==4.0.0
    "###
    );

    // Include the right hash in both files.
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(
        "anyio==4.0.0 --hash=sha256:cfdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f",
    )?;

    let constraints_txt = context.temp_dir.child("constraints.txt");
    constraints_txt.write_str("anyio==4.0.0 --hash=sha256:cfdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f")?;

    // Install the editable packages.
    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg(requirements_txt.path())
        .arg("--no-deps")
        .arg("--require-hashes")
        .arg("-c")
        .arg(constraints_txt.path()), @r###"
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

    // Include the right hash in both files, along with an irrelevant, wrong hash.
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(
        "anyio==4.0.0 --hash=sha256:cfdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f",
    )?;

    let constraints_txt = context.temp_dir.child("constraints.txt");
    constraints_txt.write_str("anyio==4.0.0 --hash=sha256:cfdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f --hash=sha256:afdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f")?;

    // Install the editable packages.
    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg(requirements_txt.path())
        .arg("--no-deps")
        .arg("--require-hashes")
        .arg("-c")
        .arg(constraints_txt.path()), @r###"
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
    error: In `--require-hashes` mode, all requirements must have a hash, but none were provided for: anyio==4.0.0
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
    error: In `--require-hashes` mode, all requirements must have their versions pinned with `==`, but found: anyio
    "###
    );

    Ok(())
}

/// Provide valid hashes for all dependencies with `--require-hashes` with accompanying markers.
/// Critically, one package (`requests`) depends on another (`urllib3`).
#[test]
fn require_hashes_marker() -> Result<()> {
    let context = TestContext::new("3.12").with_exclude_newer("2025-01-01T00:00:00Z");

    // Write to a requirements file.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(indoc::indoc! {r"
        certifi==2024.12.14 ; python_version >= '3.8' \
            --hash=sha256:1275f7a45be9464efc1173084eaa30f866fe2e47d389406136d332ed4967ec56 \
            --hash=sha256:b650d30f370c2b724812bee08008be0c4163b163ddaec3f2546c1caf65f191db
        charset-normalizer==3.4.1 ; python_version >= '3.8' \
            --hash=sha256:0167ddc8ab6508fe81860a57dd472b2ef4060e8d378f0cc555707126830f2537 \
            --hash=sha256:01732659ba9b5b873fc117534143e4feefecf3b2078b0a6a2e925271bb6f4cfa \
            --hash=sha256:01ad647cdd609225c5350561d084b42ddf732f4eeefe6e678765636791e78b9a \
            --hash=sha256:04432ad9479fa40ec0f387795ddad4437a2b50417c69fa275e212933519ff294 \
            --hash=sha256:0907f11d019260cdc3f94fbdb23ff9125f6b5d1039b76003b5b0ac9d6a6c9d5b \
            --hash=sha256:0924e81d3d5e70f8126529951dac65c1010cdf117bb75eb02dd12339b57749dd \
            --hash=sha256:09b26ae6b1abf0d27570633b2b078a2a20419c99d66fb2823173d73f188ce601 \
            --hash=sha256:09b5e6733cbd160dcc09589227187e242a30a49ca5cefa5a7edd3f9d19ed53fd \
            --hash=sha256:0af291f4fe114be0280cdd29d533696a77b5b49cfde5467176ecab32353395c4 \
            --hash=sha256:0f55e69f030f7163dffe9fd0752b32f070566451afe180f99dbeeb81f511ad8d \
            --hash=sha256:1a2bc9f351a75ef49d664206d51f8e5ede9da246602dc2d2726837620ea034b2 \
            --hash=sha256:22e14b5d70560b8dd51ec22863f370d1e595ac3d024cb8ad7d308b4cd95f8313 \
            --hash=sha256:234ac59ea147c59ee4da87a0c0f098e9c8d169f4dc2a159ef720f1a61bbe27cd \
            --hash=sha256:2369eea1ee4a7610a860d88f268eb39b95cb588acd7235e02fd5a5601773d4fa \
            --hash=sha256:237bdbe6159cff53b4f24f397d43c6336c6b0b42affbe857970cefbb620911c8 \
            --hash=sha256:28bf57629c75e810b6ae989f03c0828d64d6b26a5e205535585f96093e405ed1 \
            --hash=sha256:2967f74ad52c3b98de4c3b32e1a44e32975e008a9cd2a8cc8966d6a5218c5cb2 \
            --hash=sha256:2a75d49014d118e4198bcee5ee0a6f25856b29b12dbf7cd012791f8a6cc5c496 \
            --hash=sha256:2bdfe3ac2e1bbe5b59a1a63721eb3b95fc9b6817ae4a46debbb4e11f6232428d \
            --hash=sha256:2d074908e1aecee37a7635990b2c6d504cd4766c7bc9fc86d63f9c09af3fa11b \
            --hash=sha256:2fb9bd477fdea8684f78791a6de97a953c51831ee2981f8e4f583ff3b9d9687e \
            --hash=sha256:311f30128d7d333eebd7896965bfcfbd0065f1716ec92bd5638d7748eb6f936a \
            --hash=sha256:329ce159e82018d646c7ac45b01a430369d526569ec08516081727a20e9e4af4 \
            --hash=sha256:345b0426edd4e18138d6528aed636de7a9ed169b4aaf9d61a8c19e39d26838ca \
            --hash=sha256:363e2f92b0f0174b2f8238240a1a30142e3db7b957a5dd5689b0e75fb717cc78 \
            --hash=sha256:3a3bd0dcd373514dcec91c411ddb9632c0d7d92aed7093b8c3bbb6d69ca74408 \
            --hash=sha256:3bed14e9c89dcb10e8f3a29f9ccac4955aebe93c71ae803af79265c9ca5644c5 \
            --hash=sha256:44251f18cd68a75b56585dd00dae26183e102cd5e0f9f1466e6df5da2ed64ea3 \
            --hash=sha256:44ecbf16649486d4aebafeaa7ec4c9fed8b88101f4dd612dcaf65d5e815f837f \
            --hash=sha256:4532bff1b8421fd0a320463030c7520f56a79c9024a4e88f01c537316019005a \
            --hash=sha256:49402233c892a461407c512a19435d1ce275543138294f7ef013f0b63d5d3765 \
            --hash=sha256:4c0907b1928a36d5a998d72d64d8eaa7244989f7aaaf947500d3a800c83a3fd6 \
            --hash=sha256:4d86f7aff21ee58f26dcf5ae81a9addbd914115cdebcbb2217e4f0ed8982e146 \
            --hash=sha256:5777ee0881f9499ed0f71cc82cf873d9a0ca8af166dfa0af8ec4e675b7df48e6 \
            --hash=sha256:5df196eb874dae23dcfb968c83d4f8fdccb333330fe1fc278ac5ceeb101003a9 \
            --hash=sha256:619a609aa74ae43d90ed2e89bdd784765de0a25ca761b93e196d938b8fd1dbbd \
            --hash=sha256:6e27f48bcd0957c6d4cb9d6fa6b61d192d0b13d5ef563e5f2ae35feafc0d179c \
            --hash=sha256:6ff8a4a60c227ad87030d76e99cd1698345d4491638dfa6673027c48b3cd395f \
            --hash=sha256:73d94b58ec7fecbc7366247d3b0b10a21681004153238750bb67bd9012414545 \
            --hash=sha256:7461baadb4dc00fd9e0acbe254e3d7d2112e7f92ced2adc96e54ef6501c5f176 \
            --hash=sha256:75832c08354f595c760a804588b9357d34ec00ba1c940c15e31e96d902093770 \
            --hash=sha256:7709f51f5f7c853f0fb938bcd3bc59cdfdc5203635ffd18bf354f6967ea0f824 \
            --hash=sha256:78baa6d91634dfb69ec52a463534bc0df05dbd546209b79a3880a34487f4b84f \
            --hash=sha256:7974a0b5ecd505609e3b19742b60cee7aa2aa2fb3151bc917e6e2646d7667dcf \
            --hash=sha256:7a4f97a081603d2050bfaffdefa5b02a9ec823f8348a572e39032caa8404a487 \
            --hash=sha256:7b1bef6280950ee6c177b326508f86cad7ad4dff12454483b51d8b7d673a2c5d \
            --hash=sha256:7d053096f67cd1241601111b698f5cad775f97ab25d81567d3f59219b5f1adbd \
            --hash=sha256:804a4d582ba6e5b747c625bf1255e6b1507465494a40a2130978bda7b932c90b \
            --hash=sha256:807f52c1f798eef6cf26beb819eeb8819b1622ddfeef9d0977a8502d4db6d534 \
            --hash=sha256:80ed5e856eb7f30115aaf94e4a08114ccc8813e6ed1b5efa74f9f82e8509858f \
            --hash=sha256:8417cb1f36cc0bc7eaba8ccb0e04d55f0ee52df06df3ad55259b9a323555fc8b \
            --hash=sha256:8436c508b408b82d87dc5f62496973a1805cd46727c34440b0d29d8a2f50a6c9 \
            --hash=sha256:89149166622f4db9b4b6a449256291dc87a99ee53151c74cbd82a53c8c2f6ccd \
            --hash=sha256:8bfa33f4f2672964266e940dd22a195989ba31669bd84629f05fab3ef4e2d125 \
            --hash=sha256:8c60ca7339acd497a55b0ea5d506b2a2612afb2826560416f6894e8b5770d4a9 \
            --hash=sha256:91b36a978b5ae0ee86c394f5a54d6ef44db1de0815eb43de826d41d21e4af3de \
            --hash=sha256:955f8851919303c92343d2f66165294848d57e9bba6cf6e3625485a70a038d11 \
            --hash=sha256:97f68b8d6831127e4787ad15e6757232e14e12060bec17091b85eb1486b91d8d \
            --hash=sha256:9b23ca7ef998bc739bf6ffc077c2116917eabcc901f88da1b9856b210ef63f35 \
            --hash=sha256:9f0b8b1c6d84c8034a44893aba5e767bf9c7a211e313a9605d9c617d7083829f \
            --hash=sha256:aabfa34badd18f1da5ec1bc2715cadc8dca465868a4e73a0173466b688f29dda \
            --hash=sha256:ab36c8eb7e454e34e60eb55ca5d241a5d18b2c6244f6827a30e451c42410b5f7 \
            --hash=sha256:b010a7a4fd316c3c484d482922d13044979e78d1861f0e0650423144c616a46a \
            --hash=sha256:b1ac5992a838106edb89654e0aebfc24f5848ae2547d22c2c3f66454daa11971 \
            --hash=sha256:b7b2d86dd06bfc2ade3312a83a5c364c7ec2e3498f8734282c6c3d4b07b346b8 \
            --hash=sha256:b97e690a2118911e39b4042088092771b4ae3fc3aa86518f84b8cf6888dbdb41 \
            --hash=sha256:bc2722592d8998c870fa4e290c2eec2c1569b87fe58618e67d38b4665dfa680d \
            --hash=sha256:c0429126cf75e16c4f0ad00ee0eae4242dc652290f940152ca8c75c3a4b6ee8f \
            --hash=sha256:c30197aa96e8eed02200a83fba2657b4c3acd0f0aa4bdc9f6c1af8e8962e0757 \
            --hash=sha256:c4c3e6da02df6fa1410a7680bd3f63d4f710232d3139089536310d027950696a \
            --hash=sha256:c75cb2a3e389853835e84a2d8fb2b81a10645b503eca9bcb98df6b5a43eb8886 \
            --hash=sha256:c96836c97b1238e9c9e3fe90844c947d5afbf4f4c92762679acfe19927d81d77 \
            --hash=sha256:d7f50a1f8c450f3925cb367d011448c39239bb3eb4117c36a6d354794de4ce76 \
            --hash=sha256:d973f03c0cb71c5ed99037b870f2be986c3c05e63622c017ea9816881d2dd247 \
            --hash=sha256:d98b1668f06378c6dbefec3b92299716b931cd4e6061f3c875a71ced1780ab85 \
            --hash=sha256:d9c3cdf5390dcd29aa8056d13e8e99526cda0305acc038b96b30352aff5ff2bb \
            --hash=sha256:dad3e487649f498dd991eeb901125411559b22e8d7ab25d3aeb1af367df5efd7 \
            --hash=sha256:dccbe65bd2f7f7ec22c4ff99ed56faa1e9f785482b9bbd7c717e26fd723a1d1e \
            --hash=sha256:dd78cfcda14a1ef52584dbb008f7ac81c1328c0f58184bf9a84c49c605002da6 \
            --hash=sha256:e218488cd232553829be0664c2292d3af2eeeb94b32bea483cf79ac6a694e037 \
            --hash=sha256:e358e64305fe12299a08e08978f51fc21fac060dcfcddd95453eabe5b93ed0e1 \
            --hash=sha256:ea0d8d539afa5eb2728aa1932a988a9a7af94f18582ffae4bc10b3fbdad0626e \
            --hash=sha256:eab677309cdb30d047996b36d34caeda1dc91149e4fdca0b1a039b3f79d9a807 \
            --hash=sha256:eb8178fe3dba6450a3e024e95ac49ed3400e506fd4e9e5c32d30adda88cbd407 \
            --hash=sha256:ecddf25bee22fe4fe3737a399d0d177d72bc22be6913acfab364b40bce1ba83c \
            --hash=sha256:eea6ee1db730b3483adf394ea72f808b6e18cf3cb6454b4d86e04fa8c4327a12 \
            --hash=sha256:f08ff5e948271dc7e18a35641d2f11a4cd8dfd5634f55228b691e62b37125eb3 \
            --hash=sha256:f30bf9fd9be89ecb2360c7d94a711f00c09b976258846efe40db3d05828e8089 \
            --hash=sha256:fa88b843d6e211393a37219e6a1c1df99d35e8fd90446f1118f4216e307e48cd \
            --hash=sha256:fc54db6c8593ef7d4b2a331b58653356cf04f67c960f584edb7c3d8c97e8f39e \
            --hash=sha256:fd4ec41f914fa74ad1b8304bbc634b3de73d2a0889bd32076342a573e0779e00 \
            --hash=sha256:ffc9202a29ab3920fa812879e95a9e78b2465fd10be7fcbd042899695d75e616
        idna==3.10 ; python_version >= '3.8' \
            --hash=sha256:12f65c9b470abda6dc35cf8e63cc574b1c52b11df2c86030af0ac09b01b13ea9 \
            --hash=sha256:946d195a0d259cbba61165e88e65941f16e9b36ea6ddb97f00452bae8b1287d3
        requests==2.32.3 ; python_version >= '3.8' \
            --hash=sha256:55365417734eb18255590a9ff9eb97e9e1da868d4ccd6402399eaf68af20a760 \
            --hash=sha256:70761cfe03c773ceb22aa2f671b4757976145175cdfca038c02654d061d6dcc6
        urllib3==2.2.3 ; python_version >= '3.8' \
            --hash=sha256:ca899ca043dcb1bafa3e262d73aa25c465bfb49e0bd9dd5d59f1d0acba2f8fac \
            --hash=sha256:e7d814a81dad81e6caf2ec9fdedb284ecc9c73076b62654547cc64ccdcae26e9
    "})?;

    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--require-hashes"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Prepared 5 packages in [TIME]
    Installed 5 packages in [TIME]
     + certifi==2024.12.14
     + charset-normalizer==3.4.1
     + idna==3.10
     + requests==2.32.3
     + urllib3==2.2.3
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
     + anyio==4.3.0
     + idna==3.6
     + sniffio==1.3.1
    "###
    );

    Ok(())
}

/// Provide the wrong hash with `--verify-hashes`.
#[test]
fn verify_hashes_mismatch() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(indoc::indoc! {r"
        idna==3.6 \
            --hash=sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2 \
            --hash=sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc
    "})?;

    // Raise an error.
    uv_snapshot!(context.pip_install()
        .arg("--no-deps")
        .arg("-r")
        .arg("requirements.txt")
        .arg("--verify-hashes"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
      × Failed to download `idna==3.6`
      ╰─▶ Hash mismatch for `idna==3.6`

          Expected:
            sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2
            sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc

          Computed:
            sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f
    "###
    );

    uv_snapshot!(context.pip_install()
        .arg("--no-deps")
        .arg("-r")
        .arg("requirements.txt")
        .arg("--no-verify-hashes"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Installed 1 package in [TIME]
     + idna==3.6
    "###
    );

    Ok(())
}

/// Provide the correct hash with `--verify-hashes`.
#[test]
fn verify_hashes_match() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(indoc::indoc! {r"
        idna==3.6 \
            --hash=sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca \
            --hash=sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f
    "})?;

    uv_snapshot!(context.pip_install()
        .arg("--no-deps")
        .arg("-r")
        .arg("requirements.txt"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + idna==3.6
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

/// Allow arguments within a `requirements.txt` file to be quoted or unquoted, as in the CLI.
#[test]
fn double_quoted_arguments() -> Result<()> {
    let context = TestContext::new("3.12");

    let constraints_in = context.temp_dir.child("constraints.in");
    constraints_in.write_str(indoc::indoc! {r"
        iniconfig==1.0.0
    "})?;

    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str(indoc::indoc! {r#"
       --constraint "./constraints.in"

        iniconfig
    "#})?;

    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg("requirements.in"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==1.0.0
    "###
    );

    Ok(())
}

/// Allow arguments within a `requirements.txt` file to be quoted or unquoted, as in the CLI.
#[test]
fn single_quoted_arguments() -> Result<()> {
    let context = TestContext::new("3.12");

    let constraints_in = context.temp_dir.child("constraints.in");
    constraints_in.write_str(indoc::indoc! {r"
        iniconfig==1.0.0
    "})?;

    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str(indoc::indoc! {r"
       --constraint './constraints.in'

        iniconfig
    "})?;

    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg("requirements.in"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==1.0.0
    "###
    );

    Ok(())
}

/// Allow arguments within a `requirements.txt` file to be quoted or unquoted, as in the CLI.
#[test]
fn unquoted_arguments() -> Result<()> {
    let context = TestContext::new("3.12");

    let constraints_in = context.temp_dir.child("constraints.in");
    constraints_in.write_str(indoc::indoc! {r"
        iniconfig==1.0.0
    "})?;

    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str(indoc::indoc! {r"
       --constraint ./constraints.in

        iniconfig
    "})?;

    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg("requirements.in"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==1.0.0
    "###
    );

    Ok(())
}

/// Allow arguments within a `requirements.txt` file to be quoted or unquoted, as in the CLI.
#[test]
fn concatenated_quoted_arguments() -> Result<()> {
    let context = TestContext::new("3.12");

    let constraints_in = context.temp_dir.child("constraints.in");
    constraints_in.write_str(indoc::indoc! {r"
        iniconfig==1.0.0
    "})?;

    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str(indoc::indoc! {r#"
       --constraint "./constr""aints.in"

        iniconfig
    "#})?;

    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg("requirements.in"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==1.0.0
    "###
    );

    Ok(())
}

#[test]
#[cfg(feature = "git")]
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
            "charset-normalizer==3.4.0"
        ]
        dont_install_me = [
            "broken @ https://example.org/does/not/exist.tar.gz"
        ]

        [tool.uv.sources]
        tqdm = { url = "https://files.pythonhosted.org/packages/a5/d6/502a859bac4ad5e274255576cd3e15ca273cdb91731bc39fb840dd422ee9/tqdm-4.66.0-py3-none-any.whl" }
        charset-normalizer = { git = "https://github.com/jawah/charset_normalizer", rev = "ffdf7f5f08beb0ceb92dc0637e97382ba27cecfa" }
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
     + charset-normalizer==3.4.1 (from git+https://github.com/jawah/charset_normalizer@ffdf7f5f08beb0ceb92dc0637e97382ba27cecfa)
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
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
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
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
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
    requirements_txt.write_str(&indoc::formatdoc! {r"
        --index-url {}
        tqdm
    ", Url::from_directory_path(root).unwrap().as_str()})?;

    uv_snapshot!(context.filters(), context.pip_install()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
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
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
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

    // Relocatable entrypoint should still be usable even if symlinked.
    // Only testable on POSIX since symlinks require elevated privilege on Windows
    #[cfg(unix)]
    {
        let script_symlink_path = context.temp_dir.join("black");
        std::os::unix::fs::symlink(script_path, script_symlink_path.clone())?;
        Command::new(script_symlink_path.as_os_str())
            .assert()
            .success()
            .stdout(predicate::str::contains("Hello world!"));
    }

    Ok(())
}

/// Install requesting Python 3.12 when the virtual environment uses 3.11
#[test]
fn install_incompatible_python_version() {
    let context = TestContext::new_with_versions(&["3.11", "3.12"]);

    // Initialize the virtual environment with 3.11
    context.reset_venv();

    // Request Python 3.12; which should fail
    uv_snapshot!(context.filters(), context.pip_install().arg("-p").arg("3.12")
        .arg("anyio"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No virtual environment found for Python 3.12; run `uv venv` to create an environment, or pass `--system` to install into a non-virtual environment
    "###
    );
}

/// Install requesting Python 3.12 when the virtual environment uses 3.11, but there's also
/// a broken interpreter in the PATH.
#[test]
#[cfg(unix)]
fn install_incompatible_python_version_interpreter_broken_in_path() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let context = TestContext::new_with_versions(&["3.11", "3.12"]);

    // Initialize the virtual environment with 3.11
    context.reset_venv();

    // Create a "broken" Python executable in the test context `bin`
    let contents = r"#!/bin/sh
    echo 'error: intentionally broken python executable' >&2
    exit 1";
    let python = context
        .bin_dir
        .join(format!("python3{}", std::env::consts::EXE_SUFFIX));
    fs_err::write(&python, contents)?;

    let mut perms = fs_err::metadata(&python)?.permissions();
    perms.set_mode(0o755);
    fs_err::set_permissions(&python, perms)?;

    // Put the broken interpreter _before_ the other interpreters in the PATH
    let path = std::env::join_paths(
        std::iter::once(context.bin_dir.to_path_buf())
            .chain(std::env::split_paths(&context.python_path())),
    )
    .unwrap();

    // Request Python 3.12, which should fail since the virtual environment does not have a matching
    // version.
    // Since the broken interpreter is at the front of the PATH, this query error should be raised
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-p").arg("3.12")
        .arg("anyio")
        // In tests, we ignore `PATH` during Python discovery so we need to add the context `bin`
        .env("UV_TEST_PYTHON_PATH", path.as_os_str()), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to inspect Python interpreter from first executable in the search path at `[BIN]/python3`
      Caused by: Querying Python at `[BIN]/python3` failed with exit status exit status: 1

    [stderr]
    error: intentionally broken python executable
    "###
    );

    // Put the broken interpreter _after_ the other interpreters in the PATH
    let path = std::env::join_paths(
        std::env::split_paths(&context.python_path())
            .chain(std::iter::once(context.bin_dir.to_path_buf())),
    )
    .unwrap();

    // Since the broken interpreter is not at the front of the PATH, the query error should not be
    // raised
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-p").arg("3.12")
        .arg("anyio")
        // In tests, we ignore `PATH` during Python discovery so we need to add the context `bin`
        .env("UV_TEST_PYTHON_PATH", path.as_os_str()), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No virtual environment found for Python 3.12; run `uv venv` to create an environment, or pass `--system` to install into a non-virtual environment
    "###
    );

    Ok(())
}

/// Include a `build_constraints.txt` file with an incompatible constraint.
#[test]
fn incompatible_build_constraint() -> Result<()> {
    let context = TestContext::new("3.8");

    let constraints_txt = context.temp_dir.child("build_constraints.txt");
    constraints_txt.write_str("setuptools==1")?;

    uv_snapshot!(context.pip_install()
        .arg("requests==1.2")
        .arg("--build-constraint")
        .arg("build_constraints.txt"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to download and build `requests==1.2.0`
      ├─▶ Failed to resolve requirements from `setup.py` build
      ├─▶ No solution found when resolving: `setuptools>=40.8.0`
      ╰─▶ Because you require setuptools>=40.8.0 and setuptools==1, we can conclude that your requirements are unsatisfiable.
    "###
    );

    Ok(())
}

/// Include a `build_constraints.txt` file with a compatible constraint.
#[test]
fn compatible_build_constraint() -> Result<()> {
    let context = TestContext::new("3.8");

    let constraints_txt = context.temp_dir.child("build_constraints.txt");
    constraints_txt.write_str("setuptools>=40")?;

    uv_snapshot!(context.pip_install()
        .arg("requests==1.2")
        .arg("--build-constraint")
        .arg("build_constraints.txt"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + requests==1.2.0
    "###
    );

    Ok(())
}

/// Include `build-constraint-dependencies` in pyproject.toml with an incompatible constraint.
#[test]
fn incompatible_build_constraint_in_pyproject_toml() -> Result<()> {
    let context = TestContext::new("3.8");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"[tool.uv]
build-constraint-dependencies = [
    "setuptools==1",
]
"#,
    )?;

    uv_snapshot!(context.pip_install()
        .arg("requests==1.2"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to download and build `requests==1.2.0`
      ├─▶ Failed to resolve requirements from `setup.py` build
      ├─▶ No solution found when resolving: `setuptools>=40.8.0`
      ╰─▶ Because you require setuptools>=40.8.0 and setuptools==1, we can conclude that your requirements are unsatisfiable.
    "###
    );

    Ok(())
}

/// Include a `build_constraints.txt` file with a compatible constraint.
#[test]
fn compatible_build_constraint_in_pyproject_toml() -> Result<()> {
    let context = TestContext::new("3.8");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"[tool.uv]
build-constraint-dependencies = [
    "setuptools==40.8.0",
]
"#,
    )?;

    uv_snapshot!(context.pip_install()
        .arg("requests==1.2"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + requests==1.2.0
    "###
    );

    Ok(())
}

/// Merge `build_constraints.txt` with `build-constraint-dependencies` in pyproject.toml with an incompatible constraint.
#[test]
fn incompatible_build_constraint_merged_with_pyproject_toml() -> Result<()> {
    let context = TestContext::new("3.8");

    // Incompatible setuptools version in pyproject.toml, compatible in build_constraints.txt.
    let constraints_txt = context.temp_dir.child("build_constraints.txt");
    constraints_txt.write_str("setuptools>=40")?;
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"[tool.uv]
build-constraint-dependencies = [
    "setuptools==1",
]
"#,
    )?;

    uv_snapshot!(context.pip_install()
        .arg("requests==1.2")
        .arg("--build-constraint")
        .arg("build_constraints.txt"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to download and build `requests==1.2.0`
      ├─▶ Failed to resolve requirements from `setup.py` build
      ├─▶ No solution found when resolving: `setuptools>=40.8.0`
      ╰─▶ Because you require setuptools>=40 and setuptools==1, we can conclude that your requirements are unsatisfiable.
    "###
    );

    // Compatible setuptools version in pyproject.toml, incompatible in build_constraints.txt.
    let constraints_txt = context.temp_dir.child("build_constraints.txt");
    constraints_txt.write_str("setuptools==1")?;
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"[tool.uv]
build-constraint-dependencies = [
    "setuptools>=40",
]
"#,
    )?;

    uv_snapshot!(context.pip_install()
        .arg("requests==1.2")
        .arg("--build-constraint")
        .arg("build_constraints.txt"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to download and build `requests==1.2.0`
      ├─▶ Failed to resolve requirements from `setup.py` build
      ├─▶ No solution found when resolving: `setuptools>=40.8.0`
      ╰─▶ Because you require setuptools==1 and setuptools>=40, we can conclude that your requirements are unsatisfiable.
    "###
    );

    Ok(())
}

/// Merge `build_constraints.txt` with `build-constraint-dependencies` in pyproject.toml with a compatible constraint.
#[test]
fn compatible_build_constraint_merged_with_pyproject_toml() -> Result<()> {
    let context = TestContext::new("3.8");

    let constraints_txt = context.temp_dir.child("build_constraints.txt");
    constraints_txt.write_str("setuptools>=40")?;
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"[tool.uv]
build-constraint-dependencies = [
    "setuptools>=1",
]
"#,
    )?;

    uv_snapshot!(context.pip_install()
        .arg("requests==1.2")
        .arg("--build-constraint")
        .arg("build_constraints.txt"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + requests==1.2.0
    "###
    );
    Ok(())
}

#[test]
fn install_build_isolation_package() -> Result<()> {
    let context = TestContext::new("3.12");

    // Create an package.
    let package = context.temp_dir.child("project");
    package.create_dir_all()?;
    let pyproject_toml = package.child("pyproject.toml");

    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "iniconfig @ https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz",
        ]
        [build-system]
        requires = [
          "setuptools >= 40.9.0",
        ]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    // Running `uv pip install` should fail for iniconfig.
    let filters = std::iter::once((r"exit code: 1", "exit status: 1"))
        .chain(context.filters())
        .collect::<Vec<_>>();
    uv_snapshot!(filters, context.pip_install()
        .arg("--no-build-isolation-package")
        .arg("iniconfig")
        .arg(package.path()), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to build `iniconfig @ https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz`
      ├─▶ The build backend returned an error
      ╰─▶ Call to `hatchling.build.prepare_metadata_for_build_wheel` failed (exit status: 1)

          [stderr]
          Traceback (most recent call last):
            File "<string>", line 8, in <module>
          ModuleNotFoundError: No module named 'hatchling'

          hint: This usually indicates a problem with the package or the build environment.
    "###
    );

    // Install `hatchinling`, `hatch-vs` for iniconfig
    uv_snapshot!(context.filters(), context.pip_install().arg("hatchling").arg("hatch-vcs"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 9 packages in [TIME]
    Prepared 9 packages in [TIME]
    Installed 9 packages in [TIME]
     + hatch-vcs==0.4.0
     + hatchling==1.22.4
     + packaging==24.0
     + pathspec==0.12.1
     + pluggy==1.4.0
     + setuptools==69.2.0
     + setuptools-scm==8.0.4
     + trove-classifiers==2024.3.3
     + typing-extensions==4.10.0
    "###);

    // Running `uv pip install` should succeed.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--no-build-isolation-package")
        .arg("iniconfig")
        .arg(package.path()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + iniconfig==2.0.0 (from https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz)
     + project==0.1.0 (from file://[TEMP_DIR]/project)
    "###);

    Ok(())
}

/// Install a package with an unsupported extension.
#[test]
fn invalid_extension() {
    let context = TestContext::new("3.8");

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("ruff @ https://files.pythonhosted.org/packages/f7/69/96766da2cdb5605e6a31ef2734aff0be17901cefb385b885c2ab88896d76/ruff-0.5.6.tar.baz")
        , @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to parse: `ruff @ https://files.pythonhosted.org/packages/f7/69/96766da2cdb5605e6a31ef2734aff0be17901cefb385b885c2ab88896d76/ruff-0.5.6.tar.baz`
      Caused by: Expected direct URL (`https://files.pythonhosted.org/packages/f7/69/96766da2cdb5605e6a31ef2734aff0be17901cefb385b885c2ab88896d76/ruff-0.5.6.tar.baz`) to end in a supported file extension: `.whl`, `.tar.gz`, `.zip`, `.tar.bz2`, `.tar.lz`, `.tar.lzma`, `.tar.xz`, `.tar.zst`, `.tar`, `.tbz`, `.tgz`, `.tlz`, or `.txz`
    ruff @ https://files.pythonhosted.org/packages/f7/69/96766da2cdb5605e6a31ef2734aff0be17901cefb385b885c2ab88896d76/ruff-0.5.6.tar.baz
           ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    "###);
}

/// Install a package without unsupported extension.
#[test]
fn no_extension() {
    let context = TestContext::new("3.8");

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("ruff @ https://files.pythonhosted.org/packages/f7/69/96766da2cdb5605e6a31ef2734aff0be17901cefb385b885c2ab88896d76/ruff-0.5.6")
        , @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to parse: `ruff @ https://files.pythonhosted.org/packages/f7/69/96766da2cdb5605e6a31ef2734aff0be17901cefb385b885c2ab88896d76/ruff-0.5.6`
      Caused by: Expected direct URL (`https://files.pythonhosted.org/packages/f7/69/96766da2cdb5605e6a31ef2734aff0be17901cefb385b885c2ab88896d76/ruff-0.5.6`) to end in a supported file extension: `.whl`, `.tar.gz`, `.zip`, `.tar.bz2`, `.tar.lz`, `.tar.lzma`, `.tar.xz`, `.tar.zst`, `.tar`, `.tbz`, `.tgz`, `.tlz`, or `.txz`
    ruff @ https://files.pythonhosted.org/packages/f7/69/96766da2cdb5605e6a31ef2734aff0be17901cefb385b885c2ab88896d76/ruff-0.5.6
           ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    "###);
}

/// Regression test for: <https://github.com/astral-sh/uv/pull/6646>
#[test]
fn switch_platform() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("iniconfig ; python_version == '3.12'")?;

    // Install `iniconfig`.
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
     + iniconfig==2.0.0
    "###);

    requirements_txt
        .write_str("iniconfig ; python_version == '3.12'\nanyio ; python_version < '3.12'")?;

    // Add `anyio`, though it's only installed because of `--python-version`.
    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--python-version")
        .arg("3.11"), @r###"
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
    "###);

    Ok(())
}

/// See: <https://github.com/astral-sh/uv/pull/6714>
#[test]
#[cfg(feature = "slow-tests")]
fn stale_egg_info() -> Result<()> {
    let context = TestContext::new("3.12");

    // Create a project with dynamic metadata (version).
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        dynamic = ["version"]

        dependencies = ["iniconfig"]
        "#
    })?;

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-e")
        .arg("."), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + iniconfig==2.0.0
     + project==0.0.0 (from file://[TEMP_DIR]/)
    "###
    );

    // Ensure that `.egg-info` exists.
    let egg_info = context.temp_dir.child("project.egg-info");
    egg_info.assert(predicates::path::is_dir());

    // Change the metadata.
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        dynamic = ["version"]

        dependencies = ["anyio"]
        "#
    })?;

    // Reinstall. Ensure that the metadata is updated.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-e")
        .arg("."), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 4 packages in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     ~ project==0.0.0 (from file://[TEMP_DIR]/)
     + sniffio==1.3.1
    "###
    );

    Ok(())
}

/// Avoid using a compatible, cached wheel if there's another, more compatible wheel returned by
/// the resolver.
///
/// See: <https://github.com/astral-sh/uv/issues/12273>
#[test]
fn avoid_cached_wheel() {
    let context = TestContext::new_with_versions(&["3.10", "3.11"]);

    // Create a Python 3.10 environment.
    context
        .venv()
        .arg("--python")
        .arg("3.10")
        .arg(".venv-3.10")
        .assert()
        .success();

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--python")
        .arg(".venv-3.10")
        .arg("multiprocess"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using Python 3.10.[X] environment at: .venv-3.10
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + dill==0.3.8
     + multiprocess==0.70.16
    "
    );

    // Create a Python 3.11 environment.
    context
        .venv()
        .arg("--python")
        .arg("3.11")
        .arg(".venv-3.11")
        .assert()
        .success();

    // `multiprocessing` should be re-downloaded (i.e., we should have a `Prepare` step here).
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--python")
        .arg(".venv-3.11")
        .arg("multiprocess"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using Python 3.11.[X] environment at: .venv-3.11
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 2 packages in [TIME]
     + dill==0.3.8
     + multiprocess==0.70.16
    "
    );
}

/// `suds-community` has an incorrect layout whereby the wheel includes `suds_community.egg-info` at
/// the top-level. We're then under the impression that `suds` is installed twice, but when we go to
/// uninstall the second "version", we can't find the `egg-info` directory.
#[test]
fn missing_top_level() {
    let context = TestContext::new("3.12");

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("suds-community==0.8.5"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + suds-community==0.8.5
    "###
    );

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("suds-community==0.8.5"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    warning: Failed to uninstall package at [SITE_PACKAGES]/suds_community.egg-info due to missing `top-level.txt` file. Installation may result in an incomplete environment.
    Uninstalled 2 packages in [TIME]
    Installed 1 package in [TIME]
     ~ suds-community==0.8.5
    "###
    );
}

/// Show a dedicated error when the user attempts to install `sklearn`.
#[test]
fn sklearn() {
    let context = TestContext::new("3.12");

    let filters = std::iter::once((r"exit code: 1", "exit status: 1"))
        .chain(context.filters())
        .collect::<Vec<_>>();
    uv_snapshot!(filters, context.pip_install().arg("sklearn"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to build `sklearn==0.0.post12`
      ├─▶ The build backend returned an error
      ╰─▶ Call to `setuptools.build_meta:__legacy__.build_wheel` failed (exit status: 1)

          [stderr]
          The 'sklearn' PyPI package is deprecated, use 'scikit-learn'
          rather than 'sklearn' for pip commands.

          Here is how to fix this error in the main use cases:
          - use 'pip install scikit-learn' rather than 'pip install sklearn'
          - replace 'sklearn' by 'scikit-learn' in your pip requirements files
            (requirements.txt, setup.py, setup.cfg, Pipfile, etc ...)
          - if the 'sklearn' package is used by one of your dependencies,
            it would be great if you take some time to track which package uses
            'sklearn' instead of 'scikit-learn' and report it to their issue tracker
          - as a last resort, set the environment variable
            SKLEARN_ALLOW_DEPRECATED_SKLEARN_PACKAGE_INSTALL=True to avoid this error

          More information is available at
          https://github.com/scikit-learn/sklearn-pypi-package

          hint: This usually indicates a problem with the package or the build environment.
      help: `sklearn` is often confused for `scikit-learn` Did you mean to install `scikit-learn` instead?
    "###
    );
}

#[test]
fn resolve_derivation_chain() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["wsgiref"]
        "#
    })?;

    let filters = context
        .filters()
        .into_iter()
        .chain([
            (r"exit code: 1", "exit status: 1"),
            (r"/.*/src", "/[TMP]/src"),
        ])
        .collect::<Vec<_>>();

    uv_snapshot!(filters, context.pip_install()
        .arg("-e")
        .arg("."), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to build `wsgiref==0.1.2`
      ├─▶ The build backend returned an error
      ╰─▶ Call to `setuptools.build_meta:__legacy__.build_wheel` failed (exit status: 1)

          [stderr]
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
            File "<string>", line 5, in <module>
            File "[CACHE_DIR]/[TMP]/src/ez_setup/__init__.py", line 170
              print "Setuptools version",version,"or greater has been installed."
              ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
          SyntaxError: Missing parentheses in call to 'print'. Did you mean print(...)?

          hint: This usually indicates a problem with the package or the build environment.
      help: `wsgiref` (v0.1.2) was included because `project` (v0.1.0) depends on `wsgiref`
    "###
    );

    Ok(())
}

/// Ensure that `UV_NO_INSTALLER_METADATA` env var is respected.
#[test]
fn respect_no_installer_metadata_env_var() {
    let context = TestContext::new("3.12");

    // Install urllib3.
    uv_snapshot!(context.pip_install()
        .arg("urllib3==2.2.1")
        .arg("--strict")
        .env(EnvVars::UV_NO_INSTALLER_METADATA, "1"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + urllib3==2.2.1
    "###
    );

    context.assert_command("import urllib3").success();

    // Assert INSTALLER file was _not_ created.
    let installer_file = context
        .site_packages()
        .join("urllib3-2.2.3.dist-info")
        .join("INSTALLER");
    assert!(!installer_file.exists());
}

/// Check that we error if a source dist lies about its built wheel version.
#[test]
fn test_dynamic_version_sdist_wrong_version() -> Result<()> {
    let context = TestContext::new("3.12");

    // Write a source dist that has a version in its name, a dynamic version in pyproject.toml,
    // but reports the wrong version when built.
    let pyproject_toml = r#"
    [project]
    name = "foo"
    requires-python = ">=3.9"
    dependencies = []
    dynamic = ["version"]
    "#;

    let setup_py = indoc! {r#"
    from setuptools import setup

    setup(name="foo", version="10.11.12")
    "#};

    let source_dist = context.temp_dir.child("foo-1.2.3.tar.gz");
    // Flush the file after we're done.
    {
        let file = File::create(source_dist.path())?;
        let enc = GzEncoder::new(file, flate2::Compression::default());
        let mut tar = tar::Builder::new(enc);

        for (path, contents) in [
            ("foo-1.2.3/pyproject.toml", pyproject_toml),
            ("foo-1.2.3/setup.py", setup_py),
        ] {
            let mut header = tar::Header::new_gnu();
            header.set_size(contents.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            tar.append_data(&mut header, path, Cursor::new(contents))?;
        }
        tar.finish()?;
    }

    uv_snapshot!(context.filters(), context
        .pip_install()
        .arg(source_dist.path()), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
      × Failed to build `foo @ file://[TEMP_DIR]/foo-1.2.3.tar.gz`
      ╰─▶ Package metadata version `10.11.12` does not match given version `1.2.3`
    "###
    );

    Ok(())
}

/// Install a package with multiple wheels at the same version, differing only in the build tag. We
/// should choose the wheel with the highest build tag.
#[test]
fn build_tag() {
    let context = TestContext::new("3.12");

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("build-tag")
        .arg("--find-links")
        .arg(context.workspace_root.join("scripts/links/")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + build-tag==1.0.0
    "###
    );

    // Ensure that we choose the highest build tag (5).
    uv_snapshot!(Command::new(venv_to_interpreter(&context.venv))
        .arg("-B")
        .arg("-c")
        .arg("import build_tag; build_tag.main()")
        .current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    5

    ----- stderr -----
    "###);
}

#[test]
fn missing_git_prefix() -> Result<()> {
    let context = TestContext::new("3.12");
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;

    uv_snapshot!(context.pip_install()
        .arg("workspace-in-root-test @ https://github.com/astral-sh/workspace-in-root-test"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to parse: `workspace-in-root-test @ https://github.com/astral-sh/workspace-in-root-test`
      Caused by: Direct URL (`https://github.com/astral-sh/workspace-in-root-test`) references a Git repository, but is missing the `git+` prefix (e.g., `git+https://github.com/astral-sh/workspace-in-root-test`)
    workspace-in-root-test @ https://github.com/astral-sh/workspace-in-root-test
                             ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    "###
    );

    Ok(())
}

#[test]
#[cfg(feature = "git")]
fn missing_subdirectory_git() -> Result<()> {
    let context = TestContext::new("3.12");
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;

    uv_snapshot!(context.pip_install()
        .arg("workspace-in-root-test @ git+https://github.com/astral-sh/workspace-in-root-test#subdirectory=missing"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to download and build `workspace-in-root-test @ git+https://github.com/astral-sh/workspace-in-root-test#subdirectory=missing`
      ╰─▶ The source distribution `git+https://github.com/astral-sh/workspace-in-root-test#subdirectory=missing` has no subdirectory `missing`
    "###
    );

    Ok(())
}

#[test]
fn missing_subdirectory_url() -> Result<()> {
    let context = TestContext::new("3.12");
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;

    uv_snapshot!(context.pip_install()
        .arg("source-distribution @ https://files.pythonhosted.org/packages/1f/e5/5b016c945d745f8b108e759d428341488a6aee8f51f07c6c4e33498bb91f/source_distribution-0.0.3.tar.gz#subdirectory=missing"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to download and build `source-distribution @ https://files.pythonhosted.org/packages/1f/e5/5b016c945d745f8b108e759d428341488a6aee8f51f07c6c4e33498bb91f/source_distribution-0.0.3.tar.gz#subdirectory=missing`
      ╰─▶ The source distribution `https://files.pythonhosted.org/packages/1f/e5/5b016c945d745f8b108e759d428341488a6aee8f51f07c6c4e33498bb91f/source_distribution-0.0.3.tar.gz#subdirectory=missing` has no subdirectory `missing`
    "###
    );

    Ok(())
}

// This wheel was uploaded with a bad crc32 and we weren't detecting that
// (Could be replaced with a checked-in hand-crafted corrupt wheel?)
#[test]
fn bad_crc32() -> Result<()> {
    let context = TestContext::new("3.11");
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;

    uv_snapshot!(context.pip_install()
        .arg("--python-platform").arg("linux")
        .arg("osqp @ https://files.pythonhosted.org/packages/00/04/5959347582ab970e9b922f27585d34f7c794ed01125dac26fb4e7dd80205/osqp-1.0.2-cp311-cp311-manylinux_2_17_x86_64.manylinux2014_x86_64.whl"), @r"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
      × Failed to download `osqp @ https://files.pythonhosted.org/packages/00/04/5959347582ab970e9b922f27585d34f7c794ed01125dac26fb4e7dd80205/osqp-1.0.2-cp311-cp311-manylinux_2_17_x86_64.manylinux2014_x86_64.whl`
      ├─▶ Failed to extract archive: osqp-1.0.2-cp311-cp311-manylinux_2_17_x86_64.manylinux2014_x86_64.whl
      ╰─▶ Bad CRC (got ca5f1131, expected d5c95dfa) for file: osqp/ext_builtin.cpython-311-x86_64-linux-gnu.so
    "
    );

    Ok(())
}

#[test]
fn static_metadata_pyproject_toml() -> Result<()> {
    let context = TestContext::new("3.12");

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "example"
        version = "0.0.0"
        dependencies = [
          "anyio==3.7.0"
        ]

        [[tool.uv.dependency-metadata]]
        name = "anyio"
        version = "3.7.0"
        requires-dist = ["typing-extensions"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-r")
        .arg("pyproject.toml"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + anyio==3.7.0
     + typing-extensions==4.10.0
    "###
    );

    Ok(())
}

#[test]
fn static_metadata_source_tree() -> Result<()> {
    let context = TestContext::new("3.12");

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "example"
        version = "0.0.0"
        dependencies = [
          "anyio==3.7.0"
        ]

        [[tool.uv.dependency-metadata]]
        name = "anyio"
        version = "3.7.0"
        requires-dist = ["typing-extensions"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-e")
        .arg("."), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==3.7.0
     + example==0.0.0 (from file://[TEMP_DIR]/)
     + typing-extensions==4.10.0
    "###
    );

    Ok(())
}

/// Regression test for: <https://github.com/astral-sh/uv/issues/10239#issuecomment-2565663046>
#[test]
fn static_metadata_already_installed() -> Result<()> {
    let context = TestContext::new("3.12");

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "example"
        version = "0.0.0"
        dependencies = [
          "anyio==3.7.0"
        ]

        [[tool.uv.dependency-metadata]]
        name = "anyio"
        version = "3.7.0"
        requires-dist = ["typing-extensions"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-r")
        .arg("pyproject.toml"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + anyio==3.7.0
     + typing-extensions==4.10.0
    "###
    );

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-e")
        .arg("."), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + example==0.0.0 (from file://[TEMP_DIR]/)
    "###
    );

    Ok(())
}

/// `circular-one` depends on `circular-two` as a build dependency, but `circular-two` depends on
/// `circular-one` was a runtime dependency.
#[test]
fn cyclic_build_dependency() {
    let context = TestContext::new("3.13").with_exclude_newer("2025-01-02T00:00:00Z");

    // Installing with `--no-binary circular-one` should fail, since we'll end up in a recursive
    // build.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("circular-one")
        .arg("--extra-index-url")
        .arg("https://test.pypi.org/simple")
        .arg("--index-strategy")
        .arg("unsafe-best-match")
        .arg("--no-binary")
        .arg("circular-one"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
      × Failed to download and build `circular-one==0.2.0`
      ├─▶ Failed to install requirements from `build-system.requires`
      ╰─▶ Cyclic build dependency detected for `circular-one`
    "###
    );

    // Installing without `--no-binary circular-one` should succeed, since we can use the wheel.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("circular-one")
        .arg("--extra-index-url")
        .arg("https://test.pypi.org/simple")
        .arg("--index-strategy")
        .arg("unsafe-best-match"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + circular-one==0.2.0
    "###
    );
}

#[test]
#[cfg(feature = "git")]
fn direct_url_json_git_default() -> Result<()> {
    let context = TestContext::new("3.12");
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(
        "uv-public-pypackage @ git+https://github.com/astral-test/uv-public-pypackage",
    )?;

    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + uv-public-pypackage==0.1.0 (from git+https://github.com/astral-test/uv-public-pypackage@b270df1a2fb5d012294e9aaf05e7e0bab1e6a389)
    "###
    );

    let direct_url = context.venv.child(if cfg!(windows) {
        "Lib\\site-packages\\uv_public_pypackage-0.1.0.dist-info\\direct_url.json"
    } else {
        "lib/python3.12/site-packages/uv_public_pypackage-0.1.0.dist-info/direct_url.json"
    });
    direct_url.assert(predicates::path::is_file());

    let direct_url_content = fs_err::read_to_string(direct_url.path())?;
    insta::assert_snapshot!(direct_url_content, @r###"{"url":"https://github.com/astral-test/uv-public-pypackage","vcs_info":{"vcs":"git","commit_id":"b270df1a2fb5d012294e9aaf05e7e0bab1e6a389"}}"###);

    Ok(())
}

#[test]
#[cfg(feature = "git")]
fn direct_url_json_git_tag() -> Result<()> {
    let context = TestContext::new("3.12");
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(
        "uv-public-pypackage @ git+https://github.com/astral-test/uv-public-pypackage@0.0.1",
    )?;

    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + uv-public-pypackage==0.1.0 (from git+https://github.com/astral-test/uv-public-pypackage@0dacfd662c64cb4ceb16e6cf65a157a8b715b979)
    "###
    );

    let direct_url = context.venv.child(if cfg!(windows) {
        "Lib\\site-packages\\uv_public_pypackage-0.1.0.dist-info\\direct_url.json"
    } else {
        "lib/python3.12/site-packages/uv_public_pypackage-0.1.0.dist-info/direct_url.json"
    });
    direct_url.assert(predicates::path::is_file());

    let direct_url_content = fs_err::read_to_string(direct_url.path())?;
    insta::assert_snapshot!(direct_url_content, @r###"{"url":"https://github.com/astral-test/uv-public-pypackage","vcs_info":{"vcs":"git","commit_id":"0dacfd662c64cb4ceb16e6cf65a157a8b715b979","requested_revision":"0.0.1"}}"###);

    Ok(())
}

#[test]
fn direct_url_json_direct_url() -> Result<()> {
    let context = TestContext::new("3.12");
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(
    "source-distribution @ https://files.pythonhosted.org/packages/1f/e5/5b016c945d745f8b108e759d428341488a6aee8f51f07c6c4e33498bb91f/source_distribution-0.0.3.tar.gz",
    )?;

    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + source-distribution==0.0.3 (from https://files.pythonhosted.org/packages/1f/e5/5b016c945d745f8b108e759d428341488a6aee8f51f07c6c4e33498bb91f/source_distribution-0.0.3.tar.gz)
    "###
    );

    let direct_url = context.venv.child(if cfg!(windows) {
        "Lib\\site-packages\\source_distribution-0.0.3.dist-info\\direct_url.json"
    } else {
        "lib/python3.12/site-packages/source_distribution-0.0.3.dist-info/direct_url.json"
    });
    direct_url.assert(predicates::path::is_file());

    let direct_url_content = fs_err::read_to_string(direct_url.path())?;
    insta::assert_snapshot!(direct_url_content, @r###"{"url":"https://files.pythonhosted.org/packages/1f/e5/5b016c945d745f8b108e759d428341488a6aee8f51f07c6c4e33498bb91f/source_distribution-0.0.3.tar.gz","archive_info":{}}"###);

    Ok(())
}

#[test]
fn dependency_group() -> Result<()> {
    // testing basic `uv pip install --group` functionality
    fn new_context() -> Result<TestContext> {
        let context = TestContext::new("3.12");

        let pyproject_toml = context.temp_dir.child("pyproject.toml");
        pyproject_toml.write_str(
            r#"
            [project]
            name = "myproject"
            version = "0.1.0"
            requires-python = ">=3.12"
            dependencies = ["typing-extensions"]
            [dependency-groups]
            foo = ["sortedcontainers"]
            bar = ["iniconfig"]
            dev = ["sniffio"]
            "#,
        )?;

        context.lock().assert().success();
        Ok(context)
    }

    let mut context;

    // 'bar' using path sugar
    context = new_context()?;
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--group").arg("bar"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    // 'bar' using path sugar
    // and also pulling in the same pyproject.toml with -r
    context = new_context()?;
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-r").arg("pyproject.toml")
        .arg("--group").arg("bar"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + iniconfig==2.0.0
     + typing-extensions==4.10.0
    ");

    // 'bar' with an explicit path
    context = new_context()?;
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--group").arg("pyproject.toml:bar"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    // 'bar' using explicit path
    // and also pulling in the same pyproject.toml with -r
    context = new_context()?;
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-r").arg("pyproject.toml")
        .arg("--group").arg("pyproject.toml:bar"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + iniconfig==2.0.0
     + typing-extensions==4.10.0
    ");

    // 'bar' using path sugar
    context = new_context()?;
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--group").arg("foo"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + sortedcontainers==2.4.0
    ");

    // 'foo' using path sugar
    // 'bar' using path sugar
    context = new_context()?;
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--group").arg("foo")
        .arg("--group").arg("bar"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + iniconfig==2.0.0
     + sortedcontainers==2.4.0
    ");

    // all together now!
    context = new_context()?;
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-r").arg("pyproject.toml")
        .arg("--group").arg("foo")
        .arg("--group").arg("bar"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + iniconfig==2.0.0
     + sortedcontainers==2.4.0
     + typing-extensions==4.10.0
    ");

    Ok(())
}

#[test]
fn many_pyproject_group() -> Result<()> {
    // `uv pip install --group` tests with multiple projects
    fn new_context() -> Result<TestContext> {
        let context = TestContext::new("3.12");

        let pyproject_toml = context.temp_dir.child("pyproject.toml");
        pyproject_toml.write_str(
            r#"
            [project]
            name = "myproject"
            version = "0.1.0"
            requires-python = ">=3.12"
            dependencies = ["typing-extensions"]
            [dependency-groups]
            foo = ["sortedcontainers"]
            "#,
        )?;

        let subdir = context.temp_dir.child("subdir");
        subdir.create_dir_all()?;
        let pyproject_toml2 = subdir.child("pyproject.toml");
        pyproject_toml2.write_str(
            r#"
            [project]
            name = "mysubproject"
            version = "0.1.0"
            requires-python = ">=3.12"
            [dependency-groups]
            foo = ["iniconfig"]
            bar = ["sniffio"]
            "#,
        )?;

        context.lock().assert().success();
        Ok(context)
    }

    let mut context;

    // 'foo' from main toml
    context = new_context()?;
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--group").arg("pyproject.toml:foo"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + sortedcontainers==2.4.0
    ");

    // 'foo' from subtoml
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--group").arg("subdir/pyproject.toml:foo"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    // 'foo' from main toml
    // 'foo' from sub toml
    context = new_context()?;
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--group").arg("pyproject.toml:foo")
        .arg("--group").arg("subdir/pyproject.toml:foo"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + iniconfig==2.0.0
     + sortedcontainers==2.4.0
    ");

    Ok(())
}

#[test]
fn other_sources_group() -> Result<()> {
    // `uv pip install --group` tests just slamming random other sources like -e and .
    fn new_context() -> Result<TestContext> {
        let context = TestContext::new("3.12");

        let pyproject_toml = context.temp_dir.child("pyproject.toml");
        pyproject_toml.write_str(
            r#"
            [project]
            name = "myproject"
            version = "0.1.0"
            requires-python = ">=3.12"
            dependencies = ["typing-extensions"]
            [dependency-groups]
            foo = ["sortedcontainers"]
            "#,
        )?;

        Ok(context)
    }

    let mut context;

    // 'foo' from main toml
    // and install '.'
    context = new_context()?;
    uv_snapshot!(context.filters(), context.pip_install()
        .arg(".")
        .arg("--group").arg("foo"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + myproject==0.1.0 (from file://[TEMP_DIR]/)
     + sortedcontainers==2.4.0
     + typing-extensions==4.10.0
    ");

    // 'foo' from main toml
    // and install an editable
    context = new_context()?;
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-e").arg(context.workspace_root.join("scripts/packages/poetry_editable"))
        .arg("--group").arg("foo"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Prepared 5 packages in [TIME]
    Installed 5 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + poetry-editable==0.1.0 (from file://[WORKSPACE]/scripts/packages/poetry_editable)
     + sniffio==1.3.1
     + sortedcontainers==2.4.0
    ");

    Ok(())
}

#[test]
fn suspicious_group() -> Result<()> {
    // uv pip compile --group tests, where the invocations are suspicious
    // and we might want to add warnings
    fn new_context() -> Result<TestContext> {
        let context = TestContext::new("3.12");

        let pyproject_toml = context.temp_dir.child("pyproject.toml");
        pyproject_toml.write_str(
            r#"
            [project]
            name = "myproject"
            version = "0.1.0"
            requires-python = ">=3.12"
            dependencies = ["typing-extensions"]
            [dependency-groups]
            foo = ["sortedcontainers"]
            "#,
        )?;

        let subdir = context.temp_dir.child("subdir");
        subdir.create_dir_all()?;
        let pyproject_toml2 = subdir.child("pyproject.toml");
        pyproject_toml2.write_str(
            r#"
            [project]
            name = "mysubproject"
            version = "0.1.0"
            requires-python = ">=3.12"
            dependencies = ["typing-extensions"]
            [dependency-groups]
            foo = ["iniconfig"]
            bar = ["sniffio"]
            "#,
        )?;

        Ok(context)
    }

    let mut context;

    // Another variant of "both" but with the path sugar applied to the one in cwd
    context = new_context()?;
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--group").arg("foo")
        .arg("--group").arg("subdir/pyproject.toml:foo"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + iniconfig==2.0.0
     + sortedcontainers==2.4.0
    ");

    // Using the path sugar for "foo" but requesting "bar" for the subtoml
    // Although you would be forgiven for thinking "foo" should be used from
    // the subtoml, that's not what should happen.
    context = new_context()?;
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--group").arg("foo")
        .arg("--group").arg("subdir/pyproject.toml:bar"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + sniffio==1.3.1
     + sortedcontainers==2.4.0
    ");

    // Using the path sugar to request pyproject.toml:foo
    // while also importing subdir/pyproject.toml's dependencies
    // Although you would be forgiven for thinking "foo" should be used from
    // the subtoml, that's not what should happen.
    context = new_context()?;
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-r").arg("subdir/pyproject.toml")
        .arg("--group").arg("foo"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + sortedcontainers==2.4.0
     + typing-extensions==4.10.0
    ");

    // An inversion of the previous -- this one isn't terribly ambiguous
    // but we should have it in the suite too in case it should be distinguished!
    context = new_context()?;
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-r").arg("pyproject.toml")
        .arg("--group").arg("subdir/pyproject.toml:foo"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + iniconfig==2.0.0
     + typing-extensions==4.10.0
    ");

    Ok(())
}

#[test]
fn invalid_group() -> Result<()> {
    // uv pip compile --group tests, where the invocations should fail
    fn new_context() -> Result<TestContext> {
        let context = TestContext::new("3.12");

        let pyproject_toml = context.temp_dir.child("pyproject.toml");
        pyproject_toml.write_str(
            r#"
            [project]
            name = "myproject"
            version = "0.1.0"
            requires-python = ">=3.12"
            dependencies = ["typing-extensions"]
            [dependency-groups]
            foo = ["sortedcontainers"]
            "#,
        )?;

        let subdir = context.temp_dir.child("subdir");
        subdir.create_dir_all()?;
        let pyproject_toml2 = subdir.child("pyproject.toml");
        pyproject_toml2.write_str(
            r#"
            [project]
            name = "mysubproject"
            version = "0.1.0"
            requires-python = ">=3.12"
            dependencies = ["typing-extensions"]
            [dependency-groups]
            foo = ["iniconfig"]
            bar = ["sniffio"]
            "#,
        )?;

        Ok(context)
    }

    let context = new_context()?;

    // Hey you passed a path and not a group!
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--group").arg("subdir/"), @r#"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: invalid value 'subdir/' for '--group <GROUP>': Not a valid package or extra name: "subdir/". Names must start and end with a letter or digit and may only contain -, _, ., and alphanumeric characters.

    For more information, try '--help'.
    "#);

    // Hey this path needs to end with "pyproject.toml"!
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--group").arg("./:foo"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: invalid value './:foo' for '--group <GROUP>': The `--group` path is required to end in 'pyproject.toml' for compatibility with pip; got: ./

    For more information, try '--help'.
    ");

    // Hey this path needs to end with "pyproject.toml"!
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--group").arg("subdir/:foo"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: invalid value 'subdir/:foo' for '--group <GROUP>': The `--group` path is required to end in 'pyproject.toml' for compatibility with pip; got: subdir/

    For more information, try '--help'.
    ");

    // Another invocation that Looks Weird but is asking for bar from two
    // different tomls. In this case the main one doesn't define it and
    // we should error!
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--group").arg("bar")
        .arg("--group").arg("subdir/pyproject.toml:bar"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: The dependency group 'bar' was not found in the project: pyproject.toml
    ");

    Ok(())
}

#[test]
fn project_and_group() -> Result<()> {
    // Checking that --project is handled properly with --group
    fn new_context() -> Result<TestContext> {
        let context = TestContext::new("3.12");

        let pyproject_toml = context.temp_dir.child("pyproject.toml");
        pyproject_toml.write_str(
            r#"
            [project]
            name = "myproject"
            version = "0.1.0"
            requires-python = ">=3.12"
            dependencies = ["typing-extensions"]
            [dependency-groups]
            foo = ["sortedcontainers"]
            "#,
        )?;

        let subdir = context.temp_dir.child("subdir");
        subdir.create_dir_all()?;
        let pyproject_toml2 = subdir.child("pyproject.toml");
        pyproject_toml2.write_str(
            r#"
            [project]
            name = "mysubproject"
            version = "0.1.0"
            requires-python = ">=3.12"
            [dependency-groups]
            foo = ["iniconfig"]
            bar = ["sniffio"]
            "#,
        )?;

        Ok(context)
    }

    let mut context;

    // 'foo' from subtoml, by implicit-sugar + --project
    context = new_context()?;
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--project").arg("subdir")
        .arg("--group").arg("foo"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    // 'foo' from subtoml, by implicit-sugar + --project
    // 'bar' from subtoml, by explicit relpath from cwd
    // (explicit relpaths are not affected by --project)
    context = new_context()?;
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--project").arg("subdir")
        .arg("--group").arg("subdir/pyproject.toml:bar")
        .arg("--group").arg("foo"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + iniconfig==2.0.0
     + sniffio==1.3.1
    ");

    // 'bar' from subtoml, by implicit-sugar + --project
    // 'foo' from main toml, by explicit relpath from cwd
    // (explicit relpaths are not affected by --project)
    context = new_context()?;
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--project").arg("subdir")
        .arg("--group").arg("bar")
        .arg("--group").arg("pyproject.toml:foo"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + sniffio==1.3.1
     + sortedcontainers==2.4.0
    ");

    // 'bar' from subtoml, by explicit relpath from cwd
    // 'foo' from main toml, by explicit relpath from cwd
    // (explicit relpaths are not affected by --project)
    context = new_context()?;
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--project").arg("subdir")
        .arg("--group").arg("subdir/pyproject.toml:bar")
        .arg("--group").arg("pyproject.toml:foo"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + sniffio==1.3.1
     + sortedcontainers==2.4.0
    ");

    Ok(())
}

#[test]
fn directory_and_group() -> Result<()> {
    // Checking that --directory is handled properly with --group
    fn new_context() -> Result<TestContext> {
        let context = TestContext::new("3.12");

        let pyproject_toml = context.temp_dir.child("pyproject.toml");
        pyproject_toml.write_str(
            r#"
            [project]
            name = "myproject"
            version = "0.1.0"
            requires-python = ">=3.12"
            dependencies = ["typing-extensions"]
            [dependency-groups]
            foo = ["sortedcontainers"]
            "#,
        )?;

        let subdir = context.temp_dir.child("subdir");
        subdir.create_dir_all()?;
        let pyproject_toml2 = subdir.child("pyproject.toml");
        pyproject_toml2.write_str(
            r#"
            [project]
            name = "mysubproject"
            version = "0.1.0"
            requires-python = ">=3.12"
            [dependency-groups]
            foo = ["iniconfig"]
            bar = ["sniffio"]
            "#,
        )?;

        Ok(context)
    }

    let mut context;

    // 'bar' from subtoml, by implicit-sugar + --directory
    // 'foo' from main toml, by explicit relpath from --directory
    // (explicit relpaths ARE affected by --directory)
    context = new_context()?;
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--directory").arg("subdir")
        .arg("--group").arg("bar")
        .arg("--group").arg("../pyproject.toml:foo"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using Python 3.12.[X] environment at: [VENV]/
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + sniffio==1.3.1
     + sortedcontainers==2.4.0
    ");

    // 'bar' from subtoml, by explicit relpath from --directory
    // 'foo' from main toml, by explicit relpath from --directory
    // (explicit relpaths ARE affected by --directory)
    context = new_context()?;
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--directory").arg("subdir")
        .arg("--group").arg("pyproject.toml:bar")
        .arg("--group").arg("../pyproject.toml:foo"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using Python 3.12.[X] environment at: [VENV]/
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + sniffio==1.3.1
     + sortedcontainers==2.4.0
    ");

    // 'bar' from subtoml, by explicit relpath from --directory
    // 'foo' from main toml, by implicit path + --project + --directory
    // (explicit relpaths ARE affected by --directory)
    context = new_context()?;
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--directory").arg("subdir")
        .arg("--project").arg("../")
        .arg("--group").arg("pyproject.toml:bar")
        .arg("--group").arg("foo"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using Python 3.12.[X] environment at: [VENV]/
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + sniffio==1.3.1
     + sortedcontainers==2.4.0
    ");

    Ok(())
}

/// Regression test that we don't discover workspaces with `--no-sources`.
///
/// We have a workspace dependency shadowing a PyPI package and using this package's version to
/// check that by default we respect workspace package, but with `--no-sources`, we ignore them.
#[test]
fn no_sources_workspace_discovery() -> Result<()> {
    let context = TestContext::new("3.12");
    context.temp_dir.child("pyproject.toml").write_str(indoc! {
        r#"
        [project]
        name = "foo"
        version = "1.0.0"
        dependencies = ["anyio"]

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"

        [tool.uv.sources]
        anyio = { workspace = true }

        [tool.uv.workspace]
        members = ["anyio"]
        "#
    })?;
    context
        .temp_dir
        .child("src")
        .child("foo")
        .child("__init__.py")
        .touch()?;

    let anyio = context.temp_dir.child("anyio");
    anyio.child("pyproject.toml").write_str(indoc! {
        r#"
        [project]
        name = "anyio"
        version = "2.0.0"

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
        "#
    })?;
    anyio
        .child("src")
        .child("anyio")
        .child("__init__.py")
        .touch()?;

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("."), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + anyio==2.0.0 (from file://[TEMP_DIR]/anyio)
     + foo==1.0.0 (from file://[TEMP_DIR]/)
    "###
    );

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--upgrade")
        .arg("--no-sources")
        .arg("."), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 4 packages in [TIME]
    Uninstalled 2 packages in [TIME]
    Installed 4 packages in [TIME]
     - anyio==2.0.0 (from file://[TEMP_DIR]/anyio)
     + anyio==4.3.0
     ~ foo==1.0.0 (from file://[TEMP_DIR]/)
     + idna==3.6
     + sniffio==1.3.1
    "###
    );

    // Reverse direction: Check that we switch back to the workspace package with `--upgrade`.
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--upgrade")
        .arg("."), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Uninstalled 2 packages in [TIME]
    Installed 2 packages in [TIME]
     - anyio==4.3.0
     + anyio==2.0.0 (from file://[TEMP_DIR]/anyio)
     ~ foo==1.0.0 (from file://[TEMP_DIR]/)
    "###
    );

    Ok(())
}

#[test]
fn unsupported_git_scheme() {
    let context = TestContext::new("3.12");
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("git+fantasy://foo"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to parse: `git+fantasy://foo`
      Caused by: Unsupported Git URL scheme `fantasy:` in `fantasy://foo` (expected one of `https:`, `ssh:`, or `file:`)
    git+fantasy://foo
    ^^^^^^^^^^^^^^^^^
    "###
    );
}

/// Modify a project to use a `src` layout.
#[test]
fn change_layout_src() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("-e .")?;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
        "#,
    )?;

    context
        .temp_dir
        .child("src")
        .child("project")
        .child("__init__.py")
        .touch()?;

    // Installing should build the package.
    uv_snapshot!(context.filters(), context.pip_install().arg("-r").arg("requirements.txt"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + iniconfig==2.0.0
     + project==0.1.0 (from file://[TEMP_DIR]/)
    "###
    );

    // Reinstalling should have no effect.
    uv_snapshot!(context.filters(), context.pip_install().arg("-r").arg("requirements.txt"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    "###
    );

    // Replace the `src` layout with a flat layout.
    fs_err::remove_dir_all(context.temp_dir.child("src").path())?;

    context
        .temp_dir
        .child("project")
        .child("__init__.py")
        .touch()?;

    // Installing should rebuild the package.
    uv_snapshot!(context.filters(), context.pip_install().arg("-r").arg("requirements.txt"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     ~ project==0.1.0 (from file://[TEMP_DIR]/)
    "###
    );

    // Reinstalling should have no effect.
    uv_snapshot!(context.filters(), context.pip_install().arg("-r").arg("requirements.txt"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    "###
    );

    Ok(())
}

/// Modify a custom directory in the cache keys.
#[test]
fn change_layout_custom_directory() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("-e .")?;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"

        [tool.uv]
        cache-keys = [{ dir = "build" }]
        "#,
    )?;

    context
        .temp_dir
        .child("src")
        .child("project")
        .child("__init__.py")
        .touch()?;

    // Installing should build the package.
    uv_snapshot!(context.filters(), context.pip_install().arg("-r").arg("requirements.txt"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + iniconfig==2.0.0
     + project==0.1.0 (from file://[TEMP_DIR]/)
    "###
    );

    // Reinstalling should have no effect.
    uv_snapshot!(context.filters(), context.pip_install().arg("-r").arg("requirements.txt"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    "
    );

    // Create the `build` directory.
    fs_err::create_dir(context.temp_dir.child("build"))?;

    // Installing should rebuild the package.
    uv_snapshot!(context.filters(), context.pip_install().arg("-r").arg("requirements.txt"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     ~ project==0.1.0 (from file://[TEMP_DIR]/)
    "###
    );

    // Reinstalling should have no effect.
    uv_snapshot!(context.filters(), context.pip_install().arg("-r").arg("requirements.txt"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    "
    );

    // Remove the `build` directory.
    fs_err::remove_dir(context.temp_dir.child("build"))?;

    // Installing should rebuild the package.
    uv_snapshot!(context.filters(), context.pip_install().arg("-r").arg("requirements.txt"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     ~ project==0.1.0 (from file://[TEMP_DIR]/)
    "###
    );

    // Reinstalling should have no effect.
    uv_snapshot!(context.filters(), context.pip_install().arg("-r").arg("requirements.txt"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    "
    );

    Ok(())
}
