use anyhow::Result;
use assert_fs::fixture::FileWriteStr;
use assert_fs::fixture::PathChild;

use crate::common::uv_snapshot;
use crate::common::TestContext;

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
