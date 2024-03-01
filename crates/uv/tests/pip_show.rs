use std::process::Command;

use anyhow::Result;
use assert_fs::fixture::PathChild;
use assert_fs::fixture::{FileTouch, FileWriteStr};
use url::Url;

use common::uv_snapshot;
use indoc::indoc;

use crate::common::{get_bin, TestContext, EXCLUDE_NEWER, INSTA_FILTERS};

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
fn empty() {
    let context = TestContext::new("3.12");

    uv_snapshot!(Command::new(get_bin())
        .arg("pip")
        .arg("show")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .env("VIRTUAL_ENV", context.venv.as_os_str())
        .current_dir(&context.temp_dir), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Please provide a package name or names.
    "###
    );
}

#[test]
fn found_single_package() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("MarkupSafe==2.1.3")?;

    uv_snapshot!(command(&context)
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + markupsafe==2.1.3
    "###
    );

    context.assert_command("import markupsafe").success();

    // In addition to the standard filters, remove the temporary directory from the snapshot.
    let filters: Vec<_> = [(r"Location:.*/.venv", "Location: [VENV]")]
        .into_iter()
        .chain(INSTA_FILTERS.to_vec())
        .collect();

    uv_snapshot!(filters, Command::new(get_bin())
        .arg("pip")
        .arg("show")
        .arg("markupsafe")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .env("VIRTUAL_ENV", context.venv.as_os_str())
        .current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Name: markupsafe
    Version: 2.1.3
    Location: [VENV]/lib/python3.12/site-packages
    "###
    );

    Ok(())
}

#[test]
fn found_multiple_packages() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str(indoc! {r"
        MarkupSafe==2.1.3
        pip==21.3.1
    "
    })?;

    uv_snapshot!(command(&context)
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Downloaded 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + markupsafe==2.1.3
     + pip==21.3.1
    "###
    );

    context.assert_command("import markupsafe").success();

    // In addition to the standard filters, remove the temporary directory from the snapshot.
    let filters: Vec<_> = [(r"Location:.*/.venv", "Location: [VENV]")]
        .into_iter()
        .chain(INSTA_FILTERS.to_vec())
        .collect();

    uv_snapshot!(filters, Command::new(get_bin())
        .arg("pip")
        .arg("show")
        .arg("markupsafe")
        .arg("pip")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .env("VIRTUAL_ENV", context.venv.as_os_str())
        .current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Name: markupsafe
    Version: 2.1.3
    Location: [VENV]/lib/python3.12/site-packages
    Name: pip
    Version: 21.3.1
    Location: [VENV]/lib/python3.12/site-packages
    "###
    );

    Ok(())
}

#[test]
fn found_one_out_of_two() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str(indoc! {r"
        MarkupSafe==2.1.3
        pip==21.3.1
    "
    })?;

    uv_snapshot!(command(&context)
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Downloaded 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + markupsafe==2.1.3
     + pip==21.3.1
    "###
    );

    context.assert_command("import markupsafe").success();

    // In addition to the standard filters, remove the temporary directory from the snapshot.
    let filters: Vec<_> = [(r"Location:.*/.venv", "Location: [VENV]")]
        .into_iter()
        .chain(INSTA_FILTERS.to_vec())
        .collect();

    uv_snapshot!(filters, Command::new(get_bin())
        .arg("pip")
        .arg("show")
        .arg("markupsafe")
        .arg("flask")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .env("VIRTUAL_ENV", context.venv.as_os_str())
        .current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: Skipping flask as it is not installed.
    Name: markupsafe
    Version: 2.1.3
    Location: [VENV]/lib/python3.12/site-packages
    "###
    );

    Ok(())
}

#[test]
fn found_one_out_of_two_quiet() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str(indoc! {r"
        MarkupSafe==2.1.3
        pip==21.3.1
    "
    })?;

    uv_snapshot!(command(&context)
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Downloaded 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + markupsafe==2.1.3
     + pip==21.3.1
    "###
    );

    context.assert_command("import markupsafe").success();

    // In addition to the standard filters, remove the temporary directory from the snapshot.
    let filters: Vec<_> = [(r"Location:.*/.venv", "Location: [VENV]")]
        .into_iter()
        .chain(INSTA_FILTERS.to_vec())
        .collect();

    uv_snapshot!(filters, Command::new(get_bin())
        .arg("pip")
        .arg("show")
        .arg("markupsafe")
        .arg("flask")
        .arg("--quiet")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .env("VIRTUAL_ENV", context.venv.as_os_str())
        .current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    "###
    );

    Ok(())
}

#[test]
fn empty_quiet() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str(indoc! {r"
        MarkupSafe==2.1.3
        pip==21.3.1
    "
    })?;

    uv_snapshot!(command(&context)
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Downloaded 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + markupsafe==2.1.3
     + pip==21.3.1
    "###
    );

    context.assert_command("import markupsafe").success();

    // In addition to the standard filters, remove the temporary directory from the snapshot.
    let filters: Vec<_> = [(r"Location:.*/.venv", "Location: [VENV]")]
        .into_iter()
        .chain(INSTA_FILTERS.to_vec())
        .collect();

    uv_snapshot!(filters, Command::new(get_bin())
        .arg("pip")
        .arg("show")
        .arg("flask")
        .arg("--quiet")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .env("VIRTUAL_ENV", context.venv.as_os_str())
        .current_dir(&context.temp_dir), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    "###
    );

    Ok(())
}
