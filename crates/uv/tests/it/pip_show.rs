use std::env::current_dir;

use anyhow::Result;
use assert_cmd::prelude::*;
use assert_fs::fixture::FileWriteStr;
use assert_fs::fixture::PathChild;
use indoc::indoc;

use uv_static::EnvVars;

use uv_test::uv_snapshot;

#[test]
fn show_empty() {
    let context = uv_test::test_context!("3.12");

    uv_snapshot!(context.pip_show(), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    warning: Please provide a package name or names.
    "
    );
}

#[test]
#[cfg(feature = "test-pypi")]
fn show_requires_multiple() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("requests==2.31.0")?;

    uv_snapshot!(context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @"
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
    "
    );

    context.assert_command("import requests").success();
    uv_snapshot!(context.filters(), context.pip_show()
        .arg("requests"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Name: requests
    Version: 2.31.0
    Location: [SITE_PACKAGES]/
    Requires: certifi, charset-normalizer, idna, urllib3
    Required-by:

    ----- stderr -----
    "
    );

    Ok(())
}

/// Asserts that the Python version marker in the metadata is correctly evaluated.
/// `click` v8.1.7 requires `importlib-metadata`, but only when `python_version < "3.8"`.
#[test]
#[cfg(feature = "test-pypi")]
fn show_python_version_marker() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("click==8.1.7")?;

    uv_snapshot!(context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + click==8.1.7
    "
    );

    context.assert_command("import click").success();

    let mut filters = context.filters();
    if cfg!(windows) {
        filters.push(("Requires: colorama", "Requires:"));
    }

    uv_snapshot!(filters, context.pip_show()
        .arg("click"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Name: click
    Version: 8.1.7
    Location: [SITE_PACKAGES]/
    Requires:
    Required-by:

    ----- stderr -----
    "
    );

    Ok(())
}

#[test]
#[cfg(feature = "test-pypi")]
fn show_found_single_package() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("MarkupSafe==2.1.3")?;

    uv_snapshot!(context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + markupsafe==2.1.3
    "
    );

    context.assert_command("import markupsafe").success();

    uv_snapshot!(context.filters(), context.pip_show()
        .arg("markupsafe"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Name: markupsafe
    Version: 2.1.3
    Location: [SITE_PACKAGES]/
    Requires:
    Required-by:

    ----- stderr -----
    "
    );

    Ok(())
}

#[test]
#[cfg(feature = "test-pypi")]
fn show_found_multiple_packages() -> Result<()> {
    let context = uv_test::test_context!("3.12");

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
        .arg("--strict"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + markupsafe==2.1.3
     + pip==21.3.1
    "
    );

    context.assert_command("import markupsafe").success();

    uv_snapshot!(context.filters(), context.pip_show()
        .arg("markupsafe")
        .arg("pip"), @"
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
    "
    );

    Ok(())
}

#[test]
#[cfg(feature = "test-pypi")]
fn show_found_one_out_of_three() -> Result<()> {
    let context = uv_test::test_context!("3.12");

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
        .arg("--strict"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + markupsafe==2.1.3
     + pip==21.3.1
    "
    );

    context.assert_command("import markupsafe").success();

    uv_snapshot!(context.filters(), context.pip_show()
        .arg("markupsafe")
        .arg("flask")
        .arg("django"), @"
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
    "
    );

    Ok(())
}

#[test]
#[cfg(feature = "test-pypi")]
fn show_found_one_out_of_two_quiet() -> Result<()> {
    let context = uv_test::test_context!("3.12");

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
        .arg("--strict"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + markupsafe==2.1.3
     + pip==21.3.1
    "
    );

    context.assert_command("import markupsafe").success();

    // Flask isn't installed, but markupsafe is, so the command should succeed.
    uv_snapshot!(context.pip_show()
        .arg("markupsafe")
        .arg("flask")
        .arg("--quiet"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    "
    );

    Ok(())
}

#[test]
#[cfg(feature = "test-pypi")]
fn show_empty_quiet() -> Result<()> {
    let context = uv_test::test_context!("3.12");

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
        .arg("--strict"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + markupsafe==2.1.3
     + pip==21.3.1
    "
    );

    context.assert_command("import markupsafe").success();

    // Flask isn't installed, so the command should fail.
    uv_snapshot!(context.pip_show()
        .arg("flask")
        .arg("--quiet"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    "
    );

    Ok(())
}

#[test]
#[cfg(feature = "test-pypi")]
fn show_editable() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    // Install the editable package.
    context
        .pip_install()
        .arg("-e")
        .arg("../../test/packages/poetry_editable")
        .current_dir(current_dir()?)
        .env(
            EnvVars::CARGO_TARGET_DIR,
            "../../../target/target_install_editable",
        )
        .assert()
        .success();

    uv_snapshot!(context.filters(), context.pip_show()
        .arg("poetry-editable"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Name: poetry-editable
    Version: 0.1.0
    Location: [SITE_PACKAGES]/
    Editable project location: [WORKSPACE]/test/packages/poetry_editable
    Requires: anyio
    Required-by:

    ----- stderr -----
    "
    );

    Ok(())
}

#[test]
#[cfg(feature = "test-pypi")]
fn show_required_by_multiple() -> Result<()> {
    let context = uv_test::test_context!("3.12");

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
        .arg("--strict"), @"
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
    "
    );

    context.assert_command("import requests").success();

    // idna is required by anyio and requests
    uv_snapshot!(context.filters(), context.pip_show()
        .arg("idna"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Name: idna
    Version: 3.6
    Location: [SITE_PACKAGES]/
    Requires:
    Required-by: anyio, requests

    ----- stderr -----
    "
    );

    Ok(())
}

#[test]
#[cfg(feature = "test-pypi")]
fn show_files() {
    let context = uv_test::test_context!("3.12");

    uv_snapshot!(context
        .pip_install()
        .arg("requests==2.31.0")
        .arg("--strict"), @"
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
    "
    );

    // Windows has a different files order.
    #[cfg(not(windows))]
    uv_snapshot!(context.filters(), context.pip_show().arg("requests").arg("--files"), @"
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
    ");
}

#[test]
#[cfg(feature = "test-pypi")]
fn show_target() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("MarkupSafe==2.1.3")?;

    let target = context.temp_dir.child("target");

    // Install packages to a target directory.
    context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--target")
        .arg(target.path())
        .assert()
        .success();

    // Show package in the target directory.
    uv_snapshot!(context.filters(), context.pip_show()
        .arg("markupsafe")
        .arg("--target")
        .arg(target.path()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Name: markupsafe
    Version: 2.1.3
    Location: [TEMP_DIR]/target
    Requires:
    Required-by:

    ----- stderr -----
    "
    );

    // Without --target, the package should not be found.
    uv_snapshot!(context.pip_show().arg("markupsafe"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    warning: Package(s) not found for: markupsafe
    "
    );

    Ok(())
}

#[test]
#[cfg(feature = "test-pypi")]
fn show_prefix() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("MarkupSafe==2.1.3")?;

    let prefix = context.temp_dir.child("prefix");

    // Install packages to a prefix directory.
    context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--prefix")
        .arg(prefix.path())
        .assert()
        .success();

    // Show package in the prefix directory.
    uv_snapshot!(context.filters(), context.pip_show()
        .arg("markupsafe")
        .arg("--prefix")
        .arg(prefix.path()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Name: markupsafe
    Version: 2.1.3
    Location: [TEMP_DIR]/prefix/[PYTHON-LIB]/site-packages
    Requires:
    Required-by:

    ----- stderr -----
    "
    );

    // Without --prefix, the package should not be found.
    uv_snapshot!(context.pip_show().arg("markupsafe"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    warning: Package(s) not found for: markupsafe
    "
    );

    Ok(())
}
