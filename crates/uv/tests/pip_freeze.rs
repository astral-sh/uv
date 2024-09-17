#![cfg(all(feature = "python", feature = "pypi"))]

use anyhow::Result;
use assert_cmd::prelude::*;
use assert_fs::fixture::ChildPath;
use assert_fs::prelude::*;

use crate::common::{uv_snapshot, TestContext};

mod common;

/// List multiple installed packages in a virtual environment.
#[test]
fn freeze_many() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("MarkupSafe==2.1.3\ntomli==2.0.1")?;

    // Run `pip sync`.
    context
        .pip_sync()
        .arg(requirements_txt.path())
        .assert()
        .success();

    // Run `pip freeze`.
    uv_snapshot!(context.pip_freeze()
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    markupsafe==2.1.3
    tomli==2.0.1

    ----- stderr -----
    "###
    );

    Ok(())
}

/// List a package with multiple installed distributions in a virtual environment.
#[test]
#[cfg(unix)]
fn freeze_duplicate() -> Result<()> {
    use crate::common::copy_dir_all;

    // Sync a version of `pip` into a virtual environment.
    let context1 = TestContext::new("3.12");
    let requirements_txt = context1.temp_dir.child("requirements.txt");
    requirements_txt.write_str("pip==21.3.1")?;

    // Run `pip sync`.
    context1
        .pip_sync()
        .arg(requirements_txt.path())
        .assert()
        .success();

    // Sync a different version of `pip` into a virtual environment.
    let context2 = TestContext::new("3.12");
    let requirements_txt = context2.temp_dir.child("requirements.txt");
    requirements_txt.write_str("pip==22.1.1")?;

    // Run `pip sync`.
    context2
        .pip_sync()
        .arg(requirements_txt.path())
        .assert()
        .success();

    // Copy the virtual environment to a new location.
    copy_dir_all(
        context2.site_packages().join("pip-22.1.1.dist-info"),
        context1.site_packages().join("pip-22.1.1.dist-info"),
    )?;

    // Run `pip freeze`.
    uv_snapshot!(context1.filters(), context1.pip_freeze().arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    pip==21.3.1
    pip==22.1.1

    ----- stderr -----
    warning: The package `pip` has multiple installed distributions: 
      - [SITE_PACKAGES]/pip-21.3.1.dist-info
      - [SITE_PACKAGES]/pip-22.1.1.dist-info
    "###
    );

    Ok(())
}

/// List a direct URL package in a virtual environment.
#[test]
fn freeze_url() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("anyio\niniconfig @ https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl")?;

    // Run `pip sync`.
    context
        .pip_sync()
        .arg(requirements_txt.path())
        .assert()
        .success();

    // Run `pip freeze`.
    uv_snapshot!(context.pip_freeze()
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    anyio==4.3.0
    iniconfig @ https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl

    ----- stderr -----
    warning: The package `anyio` requires `idna>=2.8`, but it's not installed
    warning: The package `anyio` requires `sniffio>=1.1`, but it's not installed
    "###
    );

    Ok(())
}

#[test]
fn freeze_with_editable() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(&format!(
        "anyio\n-e {}",
        context
            .workspace_root
            .join("scripts/packages/poetry_editable")
            .display()
    ))?;

    // Run `pip sync`.
    context
        .pip_sync()
        .arg(requirements_txt.path())
        .assert()
        .success();

    // Run `pip freeze`.
    uv_snapshot!(context.filters(), context.pip_freeze()
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    anyio==4.3.0
    -e file://[WORKSPACE]/scripts/packages/poetry_editable

    ----- stderr -----
    warning: The package `anyio` requires `idna>=2.8`, but it's not installed
    warning: The package `anyio` requires `sniffio>=1.1`, but it's not installed
    "###
    );

    // Exclude editable package.
    uv_snapshot!(context.filters(), context.pip_freeze()
        .arg("--exclude-editable")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    anyio==4.3.0

    ----- stderr -----
    warning: The package `anyio` requires `idna>=2.8`, but it's not installed
    warning: The package `anyio` requires `sniffio>=1.1`, but it's not installed
    "###
    );

    Ok(())
}

/// Show an `.egg-info` package in a virtual environment.
#[test]
fn freeze_with_egg_info() -> Result<()> {
    let context = TestContext::new("3.12");

    let site_packages = ChildPath::new(context.site_packages());

    // Manually create an `.egg-info` directory.
    site_packages
        .child("zstandard-0.22.0-py3.12.egg-info")
        .create_dir_all()?;
    site_packages
        .child("zstandard-0.22.0-py3.12.egg-info")
        .child("top_level.txt")
        .write_str("zstd")?;
    site_packages
        .child("zstandard-0.22.0-py3.12.egg-info")
        .child("SOURCES.txt")
        .write_str("")?;
    site_packages
        .child("zstandard-0.22.0-py3.12.egg-info")
        .child("PKG-INFO")
        .write_str("")?;
    site_packages
        .child("zstandard-0.22.0-py3.12.egg-info")
        .child("dependency_links.txt")
        .write_str("")?;
    site_packages
        .child("zstandard-0.22.0-py3.12.egg-info")
        .child("entry_points.txt")
        .write_str("")?;

    // Manually create the package directory.
    site_packages.child("zstd").create_dir_all()?;
    site_packages
        .child("zstd")
        .child("__init__.py")
        .write_str("")?;

    // Run `pip freeze`.
    uv_snapshot!(context.filters(), context.pip_freeze(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    zstandard==0.22.0

    ----- stderr -----
    "###);

    Ok(())
}

/// Show an `.egg-info` package in a virtual environment. In this case, the filename omits the
/// Python version.
#[test]
fn freeze_with_egg_info_no_py() -> Result<()> {
    let context = TestContext::new("3.12");

    let site_packages = ChildPath::new(context.site_packages());

    // Manually create an `.egg-info` directory.
    site_packages
        .child("zstandard-0.22.0.egg-info")
        .create_dir_all()?;
    site_packages
        .child("zstandard-0.22.0.egg-info")
        .child("top_level.txt")
        .write_str("zstd")?;
    site_packages
        .child("zstandard-0.22.0.egg-info")
        .child("SOURCES.txt")
        .write_str("")?;
    site_packages
        .child("zstandard-0.22.0.egg-info")
        .child("PKG-INFO")
        .write_str("")?;
    site_packages
        .child("zstandard-0.22.0.egg-info")
        .child("dependency_links.txt")
        .write_str("")?;
    site_packages
        .child("zstandard-0.22.0.egg-info")
        .child("entry_points.txt")
        .write_str("")?;

    // Manually create the package directory.
    site_packages.child("zstd").create_dir_all()?;
    site_packages
        .child("zstd")
        .child("__init__.py")
        .write_str("")?;

    // Run `pip freeze`.
    uv_snapshot!(context.filters(), context.pip_freeze(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    zstandard==0.22.0

    ----- stderr -----
    "###);

    Ok(())
}

/// Show a set of `.egg-info` files in a virtual environment.
#[test]
fn freeze_with_egg_info_file() -> Result<()> {
    let context = TestContext::new("3.11");
    let site_packages = ChildPath::new(context.site_packages());

    // Manually create a `.egg-info` file with python version.
    site_packages
        .child("pycurl-7.45.1-py3.11.egg-info")
        .write_str(indoc::indoc! {"
            Metadata-Version: 1.1
            Name: pycurl
            Version: 7.45.1
        "})?;

    // Manually create another `.egg-info` file with no python version.
    site_packages
        .child("vtk-9.2.6.egg-info")
        .write_str(indoc::indoc! {"
            Metadata-Version: 1.1
            Name: vtk
            Version: 9.2.6
        "})?;

    // Run `pip freeze`.
    uv_snapshot!(context.filters(), context.pip_freeze(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    pycurl==7.45.1
    vtk==9.2.6

    ----- stderr -----
    "###);
    Ok(())
}

#[test]
fn freeze_with_legacy_editable() -> Result<()> {
    let context = TestContext::new("3.12");

    let site_packages = ChildPath::new(context.site_packages());

    let target = context.temp_dir.child("zstandard_project");
    target.child("zstd").create_dir_all()?;
    target.child("zstd").child("__init__.py").write_str("")?;

    target.child("zstandard.egg-info").create_dir_all()?;
    target
        .child("zstandard.egg-info")
        .child("PKG-INFO")
        .write_str(
            "Metadata-Version: 2.1
Name: zstandard
Version: 0.22.0
",
        )?;

    site_packages
        .child("zstandard.egg-link")
        .write_str(target.path().to_str().unwrap())?;

    // Run `pip freeze`.
    uv_snapshot!(context.filters(), context.pip_freeze(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    -e [TEMP_DIR]/zstandard_project

    ----- stderr -----
    "###);

    Ok(())
}
