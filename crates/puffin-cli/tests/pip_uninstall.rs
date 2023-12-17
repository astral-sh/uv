use std::process::Command;

use anyhow::Result;
use assert_cmd::prelude::*;
use assert_fs::prelude::*;
use insta_cmd::{assert_cmd_snapshot, get_cargo_bin};

use common::{BIN_NAME, INSTA_FILTERS};

mod common;

#[test]
fn no_arguments() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .arg("pip-uninstall")
        .current_dir(&temp_dir), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: the following required arguments were not provided:
      <PACKAGE|--requirement <REQUIREMENT>|--editable <EDITABLE>>

    Usage: puffin pip-uninstall <PACKAGE|--requirement <REQUIREMENT>|--editable <EDITABLE>>

    For more information, try '--help'.
    "###);

    Ok(())
}

#[test]
fn invalid_requirement() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .arg("pip-uninstall")
        .arg("flask==1.0.x")
        .current_dir(&temp_dir), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to parse `flask==1.0.x`
      Caused by: Version `1.0.x` doesn't match PEP 440 rules
    flask==1.0.x
         ^^^^^^^
    "###);

    Ok(())
}

#[test]
fn missing_requirements_txt() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .arg("pip-uninstall")
        .arg("-r")
        .arg("requirements.txt")
        .current_dir(&temp_dir), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: failed to open file `requirements.txt`
      Caused by: No such file or directory (os error 2)
    "###);

    Ok(())
}

#[test]
fn invalid_requirements_txt_requirement() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let requirements_txt = temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("flask==1.0.x")?;

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .arg("pip-uninstall")
        .arg("-r")
        .arg("requirements.txt")
        .current_dir(&temp_dir), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Couldn't parse requirement in requirements.txt position 0 to 12
      Caused by: Version `1.0.x` doesn't match PEP 440 rules
    flask==1.0.x
         ^^^^^^^
    "###);

    Ok(())
}

#[test]
fn missing_pyproject_toml() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .arg("pip-uninstall")
        .arg("-r")
        .arg("pyproject.toml")
        .current_dir(&temp_dir), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: failed to open file `pyproject.toml`
      Caused by: No such file or directory (os error 2)
    "###);

    Ok(())
}

#[test]
fn invalid_pyproject_toml_syntax() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let pyproject_toml = temp_dir.child("pyproject.toml");
    pyproject_toml.touch()?;
    pyproject_toml.write_str("123 - 456")?;

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .arg("pip-uninstall")
        .arg("-r")
        .arg("pyproject.toml")
        .current_dir(&temp_dir), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to read `pyproject.toml`
      Caused by: TOML parse error at line 1, column 5
      |
    1 | 123 - 456
      |     ^
    expected `.`, `=`

    "###);

    Ok(())
}

#[test]
fn invalid_pyproject_toml_schema() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let pyproject_toml = temp_dir.child("pyproject.toml");
    pyproject_toml.touch()?;
    pyproject_toml.write_str("[project]")?;

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .arg("pip-uninstall")
        .arg("-r")
        .arg("pyproject.toml")
        .current_dir(&temp_dir), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to read `pyproject.toml`
      Caused by: TOML parse error at line 1, column 1
      |
    1 | [project]
      | ^^^^^^^^^
    missing field `name`

    "###);

    Ok(())
}

#[test]
fn invalid_pyproject_toml_requirement() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let pyproject_toml = temp_dir.child("pyproject.toml");
    pyproject_toml.touch()?;
    pyproject_toml.write_str(
        r#"[project]
name = "project"
dependencies = ["flask==1.0.x"]
"#,
    )?;

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .arg("pip-uninstall")
        .arg("-r")
        .arg("pyproject.toml")
        .current_dir(&temp_dir), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to read `pyproject.toml`
      Caused by: TOML parse error at line 3, column 16
      |
    3 | dependencies = ["flask==1.0.x"]
      |                ^^^^^^^^^^^^^^^^
    Version `1.0.x` doesn't match PEP 440 rules
    flask==1.0.x
         ^^^^^^^

    "###);

    Ok(())
}

#[test]
fn uninstall() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .current_dir(&temp_dir)
        .assert()
        .success();
    venv.assert(predicates::path::is_dir());

    let requirements_txt = temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("MarkupSafe==2.1.3")?;

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("pip-sync")
        .arg("requirements.txt")
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .env("VIRTUAL_ENV", venv.as_os_str())
        .current_dir(&temp_dir)
        .assert()
        .success();

    Command::new(venv.join("bin").join("python"))
        .arg("-c")
        .arg("import markupsafe")
        .current_dir(&temp_dir)
        .assert()
        .success();

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-uninstall")
            .arg("MarkupSafe")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Uninstalled 1 package in [TIME]
         - markupsafe==2.1.3
        "###);
    });

    Command::new(venv.join("bin").join("python"))
        .arg("-c")
        .arg("import markupsafe")
        .current_dir(&temp_dir)
        .assert()
        .failure();

    Ok(())
}

#[test]
fn uninstall_editable_by_name() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .current_dir(&temp_dir)
        .assert()
        .success();
    venv.assert(predicates::path::is_dir());

    let requirements_txt = temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("-e ../../scripts/editable-installs/poetry_editable")?;

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("pip-sync")
        .arg(requirements_txt.path())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .env("VIRTUAL_ENV", venv.as_os_str())
        .assert()
        .success();

    Command::new(venv.join("bin").join("python"))
        .arg("-c")
        .arg("import poetry_editable")
        .assert()
        .success();

    // Uninstall the editable by name.
    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-uninstall")
            .arg("poetry-editable")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            , @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Uninstalled 1 package in [TIME]
         - poetry-editable==0.1.0
        "###);
    });

    Command::new(venv.join("bin").join("python"))
        .arg("-c")
        .arg("import poetry_editable")
        .assert()
        .failure();

    Ok(())
}

#[test]
fn uninstall_editable_by_path() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .assert()
        .success();
    venv.assert(predicates::path::is_dir());

    let requirements_txt = temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("-e ../../scripts/editable-installs/poetry_editable")?;

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("pip-sync")
        .arg(requirements_txt.path())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .env("VIRTUAL_ENV", venv.as_os_str())
        .assert()
        .success();

    Command::new(venv.join("bin").join("python"))
        .arg("-c")
        .arg("import poetry_editable")
        .assert()
        .success();

    // Uninstall the editable by path.
    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-uninstall")
            .arg("-e")
            .arg("../../scripts/editable-installs/poetry_editable")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str()), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Uninstalled 1 package in [TIME]
         - poetry-editable==0.1.0
        "###);
    });

    Command::new(venv.join("bin").join("python"))
        .arg("-c")
        .arg("import poetry_editable")
        .assert()
        .failure();

    Ok(())
}

#[test]
fn uninstall_duplicate_editable() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .assert()
        .success();
    venv.assert(predicates::path::is_dir());

    let requirements_txt = temp_dir.child("requirements.txt");
    requirements_txt.touch()?;
    requirements_txt.write_str("-e ../../scripts/editable-installs/poetry_editable")?;

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("pip-sync")
        .arg(requirements_txt.path())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .env("VIRTUAL_ENV", venv.as_os_str())
        .assert()
        .success();

    Command::new(venv.join("bin").join("python"))
        .arg("-c")
        .arg("import poetry_editable")
        .assert()
        .success();

    // Uninstall the editable by both path and name.
    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-uninstall")
            .arg("poetry-editable")
            .arg("-e")
            .arg("../../scripts/editable-installs/poetry_editable")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str()), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Uninstalled 1 package in [TIME]
         - poetry-editable==0.1.0
        "###);
    });

    Command::new(venv.join("bin").join("python"))
        .arg("-c")
        .arg("import poetry_editable")
        .assert()
        .failure();

    Ok(())
}
