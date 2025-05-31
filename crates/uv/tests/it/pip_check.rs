use anyhow::Result;
use assert_cmd::assert::OutputAssertExt;
use assert_fs::fixture::FileWriteStr;
use assert_fs::fixture::PathChild;

use crate::common::TestContext;
use crate::common::uv_snapshot;

#[test]
fn python_discovery_starts_at_project_root() -> Result<()> {
    let context = TestContext::new_with_versions(&["3.12"]);
    let filters = std::iter::once((r"Using Python.*", "[USING_PYTHON]"))
        .chain(context.filters())
        .collect::<Vec<_>>();

    // Create 2 separate projects, with separate virtual environments
    let project1 = context.temp_dir.child("project1");
    let requirements_txt1 = project1.child("requirements.txt");
    requirements_txt1.write_str("requests==2.30.0")?;
    context
        .venv()
        .arg("--directory")
        .arg("project1")
        .assert()
        .success();

    let project2 = context.temp_dir.child("project2");
    let requirements_txt1 = project2.child("requirements.txt");
    requirements_txt1.write_str("requests==2.31.0")?;
    context
        .venv()
        .arg("--directory")
        .arg("project2")
        .assert()
        .success();

    uv_snapshot!(filters, context
        .pip_install()
        .arg("--project")
        .arg("project1")
        .arg("-r")
        .arg("project1/requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    [USING_PYTHON]
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

    // We pip installed into project1, so expect this to pass
    uv_snapshot!(filters, context.pip_check().arg("--project").arg("project1"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    [USING_PYTHON]
    Checked 5 packages in [TIME]
    All installed packages are compatible
    "###
    );
    // We did not pip install in project2
    uv_snapshot!(filters, context.pip_check().arg("--project").arg("project2"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    [USING_PYTHON]
    Checked 0 packages in [TIME]
    All installed packages are compatible
    "###
    );

    Ok(())
}

#[test]
fn check_compatible_packages() -> Result<()> {
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

    uv_snapshot!(context.pip_check(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Checked 5 packages in [TIME]
    All installed packages are compatible
    "###
    );

    Ok(())
}

// requests 2.31.0 requires idna (<4,>=2.5)
// this test force-installs idna 2.4 to trigger a failure.
#[test]
fn check_incompatible_packages() -> Result<()> {
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

    let requirements_txt_idna = context.temp_dir.child("requirements_idna.txt");
    requirements_txt_idna.write_str("idna==2.4")?;

    uv_snapshot!(context
        .pip_install()
        .arg("-r")
        .arg("requirements_idna.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - idna==3.6
     + idna==2.4
    warning: The package `requests` requires `idna>=2.5,<4`, but `2.4` is installed
    "###
    );

    uv_snapshot!(context.pip_check(), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Checked 5 packages in [TIME]
    Found 1 incompatibility
    The package `requests` requires `idna>=2.5,<4`, but `2.4` is installed
    "###
    );

    Ok(())
}

// requests 2.31.0 requires idna (<4,>=2.5) and urllib3<3,>=1.21.1
// this test force-installs idna 2.4 and urllib3 1.20 to trigger a failure
// with multiple incompatible packages.
#[test]
fn check_multiple_incompatible_packages() -> Result<()> {
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

    let requirements_txt_two = context.temp_dir.child("requirements_two.txt");
    requirements_txt_two.write_str("idna==2.4\nurllib3==1.20")?;

    uv_snapshot!(context
        .pip_install()
        .arg("-r")
        .arg("requirements_two.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Uninstalled 2 packages in [TIME]
    Installed 2 packages in [TIME]
     - idna==3.6
     + idna==2.4
     - urllib3==2.2.1
     + urllib3==1.20
    warning: The package `requests` requires `idna>=2.5,<4`, but `2.4` is installed
    warning: The package `requests` requires `urllib3>=1.21.1,<3`, but `1.20` is installed
    "###
    );

    uv_snapshot!(context.pip_check(), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Checked 5 packages in [TIME]
    Found 2 incompatibilities
    The package `requests` requires `idna>=2.5,<4`, but `2.4` is installed
    The package `requests` requires `urllib3>=1.21.1,<3`, but `1.20` is installed
    "###
    );

    Ok(())
}
