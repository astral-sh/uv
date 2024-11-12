use std::env::current_dir;

use anyhow::Result;
use assert_cmd::prelude::*;
use assert_fs::fixture::FileWriteStr;
use assert_fs::fixture::PathChild;
use indoc::indoc;

use uv_static::EnvVars;

use crate::common::{uv_snapshot, TestContext};

#[test]
fn show_empty() {
    let context = TestContext::new("3.12");

    uv_snapshot!(context.pip_show(), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    warning: Please provide a package name or names.
    "###
    );
}

#[test]
fn show_requires_multiple() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("requests==2.31.0")?;

    uv_snapshot!(context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Prepared 5 packages in [TIME]
    Installed 5 packages in [TIME]
     + certifi==2024.2.2
     + charset-normalizer==3.3.2
     + idna==3.6
     + requests==2.31.0
     + urllib3==2.2.1
    "###
    );

    context.assert_command("import requests").success();
    uv_snapshot!(context.filters(), context.pip_show()
        .arg("requests"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Name: requests
    Version: 2.31.0
    Location: [SITE_PACKAGES]/
    Requires: certifi, charset-normalizer, idna, urllib3
    Required-by:

    ----- stderr -----
    "###
    );

    Ok(())
}

/// Asserts that the Python version marker in the metadata is correctly evaluated.
/// `click` v8.1.7 requires `importlib-metadata`, but only when `python_version < "3.8"`.
#[test]
fn show_python_version_marker() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("click==8.1.7")?;

    uv_snapshot!(context
        .pip_install()
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
     + click==8.1.7
    "###
    );

    context.assert_command("import click").success();

    let mut filters = context.filters();
    if cfg!(windows) {
        filters.push(("Requires: colorama", "Requires:"));
    }

    uv_snapshot!(filters, context.pip_show()
        .arg("click"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Name: click
    Version: 8.1.7
    Location: [SITE_PACKAGES]/
    Requires:
    Required-by:

    ----- stderr -----
    "###
    );

    Ok(())
}

#[test]
fn show_found_single_package() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("MarkupSafe==2.1.3")?;

    uv_snapshot!(context
        .pip_install()
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
     + markupsafe==2.1.3
    "###
    );

    context.assert_command("import markupsafe").success();

    uv_snapshot!(context.filters(), context.pip_show()
        .arg("markupsafe"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Name: markupsafe
    Version: 2.1.3
    Location: [SITE_PACKAGES]/
    Requires:
    Required-by:

    ----- stderr -----
    "###
    );

    Ok(())
}

#[test]
fn show_found_multiple_packages() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(indoc! {r"
        MarkupSafe==2.1.3
        pip==21.3.1
    "
    })?;

    uv_snapshot!(context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + markupsafe==2.1.3
     + pip==21.3.1
    "###
    );

    context.assert_command("import markupsafe").success();

    uv_snapshot!(context.filters(), context.pip_show()
        .arg("markupsafe")
        .arg("pip"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Name: markupsafe
    Version: 2.1.3
    Location: [SITE_PACKAGES]/
    Requires:
    Required-by:
    ---
    Name: pip
    Version: 21.3.1
    Location: [SITE_PACKAGES]/
    Requires:
    Required-by:

    ----- stderr -----
    "###
    );

    Ok(())
}

#[test]
fn show_found_one_out_of_three() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(indoc! {r"
        MarkupSafe==2.1.3
        pip==21.3.1
    "
    })?;

    uv_snapshot!(context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + markupsafe==2.1.3
     + pip==21.3.1
    "###
    );

    context.assert_command("import markupsafe").success();

    uv_snapshot!(context.filters(), context.pip_show()
        .arg("markupsafe")
        .arg("flask")
        .arg("django"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Name: markupsafe
    Version: 2.1.3
    Location: [SITE_PACKAGES]/
    Requires:
    Required-by:

    ----- stderr -----
    warning: Package(s) not found for: django, flask
    "###
    );

    Ok(())
}

#[test]
fn show_found_one_out_of_two_quiet() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(indoc! {r"
        MarkupSafe==2.1.3
        pip==21.3.1
    "
    })?;

    uv_snapshot!(context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + markupsafe==2.1.3
     + pip==21.3.1
    "###
    );

    context.assert_command("import markupsafe").success();

    // Flask isn't installed, but markupsafe is, so the command should succeed.
    uv_snapshot!(context.pip_show()
        .arg("markupsafe")
        .arg("flask")
        .arg("--quiet"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    "###
    );

    Ok(())
}

#[test]
fn show_empty_quiet() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(indoc! {r"
        MarkupSafe==2.1.3
        pip==21.3.1
    "
    })?;

    uv_snapshot!(context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + markupsafe==2.1.3
     + pip==21.3.1
    "###
    );

    context.assert_command("import markupsafe").success();

    // Flask isn't installed, so the command should fail.
    uv_snapshot!(context.pip_show()
        .arg("flask")
        .arg("--quiet"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    "###
    );

    Ok(())
}

#[test]
fn show_editable() -> Result<()> {
    let context = TestContext::new("3.12");

    // Install the editable package.
    context
        .pip_install()
        .arg("-e")
        .arg("../../scripts/packages/poetry_editable")
        .current_dir(current_dir()?)
        .env(
            EnvVars::CARGO_TARGET_DIR,
            "../../../target/target_install_editable",
        )
        .assert()
        .success();

    uv_snapshot!(context.filters(), context.pip_show()
        .arg("poetry-editable"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Name: poetry-editable
    Version: 0.1.0
    Location: [SITE_PACKAGES]/
    Editable project location: [WORKSPACE]/scripts/packages/poetry_editable
    Requires: anyio
    Required-by:

    ----- stderr -----
    "###
    );

    Ok(())
}

#[test]
fn show_required_by_multiple() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(indoc! {r"
        anyio==4.0.0
        requests==2.31.0
    "
    })?;

    uv_snapshot!(context
        .pip_install()
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
     + anyio==4.0.0
     + certifi==2024.2.2
     + charset-normalizer==3.3.2
     + idna==3.6
     + requests==2.31.0
     + sniffio==1.3.1
     + urllib3==2.2.1
    "###
    );

    context.assert_command("import requests").success();

    // idna is required by anyio and requests
    uv_snapshot!(context.filters(), context.pip_show()
        .arg("idna"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Name: idna
    Version: 3.6
    Location: [SITE_PACKAGES]/
    Requires:
    Required-by: anyio, requests

    ----- stderr -----
    "###
    );

    Ok(())
}

#[test]
fn show_files() {
    let context = TestContext::new("3.12");

    uv_snapshot!(context
        .pip_install()
        .arg("requests==2.31.0")
        .arg("--strict"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Prepared 5 packages in [TIME]
    Installed 5 packages in [TIME]
     + certifi==2024.2.2
     + charset-normalizer==3.3.2
     + idna==3.6
     + requests==2.31.0
     + urllib3==2.2.1
    "#
    );

    // Windows has a different files order.
    #[cfg(not(windows))]
    uv_snapshot!(context.filters(), context.pip_show().arg("requests").arg("--files"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    Name: requests
    Version: 2.31.0
    Location: [SITE_PACKAGES]/
    Requires: certifi, charset-normalizer, idna, urllib3
    Required-by:
    Files:
      requests-2.31.0.dist-info/INSTALLER
      requests-2.31.0.dist-info/LICENSE
      requests-2.31.0.dist-info/METADATA
      requests-2.31.0.dist-info/RECORD
      requests-2.31.0.dist-info/REQUESTED
      requests-2.31.0.dist-info/WHEEL
      requests-2.31.0.dist-info/top_level.txt
      requests/__init__.py
      requests/__version__.py
      requests/_internal_utils.py
      requests/adapters.py
      requests/api.py
      requests/auth.py
      requests/certs.py
      requests/compat.py
      requests/cookies.py
      requests/exceptions.py
      requests/help.py
      requests/hooks.py
      requests/models.py
      requests/packages.py
      requests/sessions.py
      requests/status_codes.py
      requests/structures.py
      requests/utils.py

    ----- stderr -----
    "#);
}
