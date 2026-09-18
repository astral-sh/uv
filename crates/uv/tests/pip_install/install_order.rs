use std::collections::BTreeMap;
use std::fmt::Write;
use std::path::Path;
use std::process::Command;

use anyhow::Result;
use assert_cmd::prelude::*;
use assert_fs::prelude::*;
use fs_err as fs;
use indoc::indoc;
use sha2::{Digest, Sha256};

use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_pep508::Requirement;
use uv_static::EnvVars;
use uv_test::packse::{generate_wheel, generate_wheel_with_files};
use uv_test::{TestContext, uv_snapshot, venv_bin_path};

/// Write a wheel that shares a module and console script with the other fixture wheels.
fn write_wheel(directory: &Path, name: &str, requirements: &[&str], slow: bool) -> Result<()> {
    let name: PackageName = name.parse()?;
    let version: Version = "1.0.0".parse()?;
    let normalized = name.as_dist_info_name();
    let requirements: Vec<Requirement> = requirements
        .iter()
        .map(|requirement| requirement.parse())
        .collect::<Result<Vec<_>, _>>()?;
    let mut files = vec![
        ("order_shared.py".to_string(), format!("OWNER = '{name}'\n")),
        (
            format!("{normalized}/cli.py"),
            format!("def main():\n    print('{name}')\n"),
        ),
        (
            format!("{normalized}-{version}.dist-info/entry_points.txt"),
            format!("[console_scripts]\norder-command = {normalized}.cli:main\n"),
        ),
    ];
    if slow {
        // Console scripts are generated after linking the wheel's files. A larger dependency
        // makes the entry-point race visible when installs are only sorted before running in parallel.
        for index in 0..1024 {
            files.push((
                format!("{normalized}/padding/file_{index:04}.txt"),
                "padding\n".to_string(),
            ));
        }
    }
    let (filename, wheel) = generate_wheel_with_files(
        &name,
        &version,
        &requirements,
        &BTreeMap::new(),
        None,
        "py3-none-any",
        &files
            .iter()
            .map(|(path, contents)| (path.as_str(), contents.as_str()))
            .collect::<Vec<_>>(),
    );
    fs::create_dir_all(directory)?;
    fs::write(directory.join(filename), wheel)?;
    Ok(())
}

fn assert_winner(context: &TestContext, expected: &str) -> Result<()> {
    assert_eq!(
        fs::read_to_string(context.site_packages().join("order_shared.py"))?,
        format!("OWNER = '{expected}'\n")
    );
    let script = if cfg!(windows) {
        "order-command.exe"
    } else {
        "order-command"
    };
    let output = Command::new(venv_bin_path(&context.venv).join(script)).output()?;
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8(output.stdout)?.replace("\r\n", "\n"),
        format!("{expected}\n")
    );
    Ok(())
}

#[test]
fn install_dependencies_before_dependents() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let wheels = context.temp_dir.join("wheels");
    write_wheel(&wheels, "order-base", &[], true)?;
    write_wheel(&wheels, "order-overlay", &["order-base==1.0.0"], false)?;

    uv_snapshot!(context.filters(), context.pip_install()
        .args(["--no-index", "--find-links", "wheels", "--link-mode", "copy", "order-overlay"])
        .env(EnvVars::UV_CONCURRENT_INSTALLS, "1"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + order-base==1.0.0
     + order-overlay==1.0.0
    ");
    assert_winner(&context, "order-overlay")?;

    // Exercise cached wheels and the parallel installer using the same environment.
    context
        .pip_install()
        .args([
            "--no-index",
            "--find-links",
            "wheels",
            "--link-mode",
            "copy",
            "--reinstall",
            "order-overlay",
        ])
        .env(EnvVars::UV_CONCURRENT_INSTALLS, "4")
        .assert()
        .success();
    assert_winner(&context, "order-overlay")?;
    Ok(())
}

#[test]
fn install_dependency_cycles_in_name_order() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let wheels = context.temp_dir.join("wheels");
    write_wheel(&wheels, "order-base", &[], true)?;
    write_wheel(&wheels, "cycle-alpha", &["cycle-zebra==1.0.0"], false)?;
    write_wheel(
        &wheels,
        "cycle-zebra",
        &["cycle-alpha==1.0.0", "order-base==1.0.0"],
        false,
    )?;

    uv_snapshot!(context.filters(), context.pip_install()
        .args(["--no-index", "--find-links", "wheels", "--link-mode", "copy", "cycle-alpha"])
        .env(EnvVars::UV_CONCURRENT_INSTALLS, "4"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + cycle-alpha==1.0.0
     + cycle-zebra==1.0.0
     + order-base==1.0.0
    ");
    assert_winner(&context, "cycle-zebra")?;
    Ok(())
}

#[test]
fn sync_keeps_dependencies_through_filtered_packages() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let wheels = context.temp_dir.join("wheels");
    write_wheel(&wheels, "order-base", &[], true)?;
    write_wheel(&wheels, "order-bridge", &["order-base==1.0.0"], false)?;
    write_wheel(&wheels, "order-overlay", &["order-bridge==1.0.0"], false)?;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = ["order-overlay==1.0.0"]
    "#})?;

    uv_snapshot!(context.filters(), context.sync()
        .args(["--no-index", "--find-links", "wheels", "--link-mode", "copy", "--no-install-package", "order-bridge"])
        .env(EnvVars::UV_CONCURRENT_INSTALLS, "4"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + order-base==1.0.0
     + order-overlay==1.0.0
    ");
    assert_winner(&context, "order-overlay")?;
    assert!(!context.site_packages().join("order_bridge").exists());
    Ok(())
}

#[test]
fn install_build_environment_dependencies_in_order() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let wheels = context.temp_dir.join("wheels");
    write_wheel(&wheels, "order-base", &[], true)?;
    write_wheel(&wheels, "order-overlay", &["order-base==1.0.0"], false)?;
    let (filename, wheel) = generate_wheel(
        &"built-project".parse()?,
        &"1.0.0".parse()?,
        &[],
        &BTreeMap::new(),
        None,
        "py3-none-any",
        &[],
    );
    let built_wheel = context.temp_dir.join(filename);
    fs::write(&built_wheel, wheel)?;
    context
        .temp_dir
        .child("project/pyproject.toml")
        .write_str(indoc! {r#"
        [build-system]
        requires = ["order-overlay==1.0.0"]
        build-backend = "backend"
        backend-path = ["."]
    "#})?;
    context
        .temp_dir
        .child("project/backend.py")
        .write_str(indoc! {r#"
        import os
        import shutil
        import subprocess
        from pathlib import Path
        import order_shared

        assert order_shared.OWNER == "order-overlay", order_shared.OWNER
        assert subprocess.check_output(["order-command"], text=True).strip() == "order-overlay"

        def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
            wheel = Path(os.environ["BUILT_PROJECT_WHEEL"])
            shutil.copyfile(wheel, Path(wheel_directory) / wheel.name)
            return wheel.name
    "#})?;

    uv_snapshot!(context.filters(), context.pip_install()
        .args(["--no-index", "--find-links", "wheels", "--link-mode", "copy", "./project"])
        .env("BUILT_PROJECT_WHEEL", built_wheel)
        .env(EnvVars::UV_CONCURRENT_INSTALLS, "4"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + built-project==1.0.0 (from file://[TEMP_DIR]/project)
    ");
    context.assert_command("import built_project").success();
    Ok(())
}

#[test]
fn flat_resolutions_install_all_selected_wheels() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let wheels = context.temp_dir.join("wheels");
    write_wheel(&wheels, "order-base", &[], false)?;
    write_wheel(&wheels, "order-overlay", &["order-base==1.0.0"], false)?;
    context
        .temp_dir
        .child("requirements.txt")
        .write_str("order-base==1.0.0\norder-overlay==1.0.0\n")?;

    uv_snapshot!(context.filters(), context.pip_sync()
        .args(["--no-index", "--find-links", "wheels", "requirements.txt"])
        .env(EnvVars::UV_CONCURRENT_INSTALLS, "4"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + order-base==1.0.0
     + order-overlay==1.0.0
    ");
    context
        .assert_command("import order_base, order_overlay")
        .success();

    let mut pylock = String::from("lock-version = \"1.0\"\ncreated-by = \"uv-test\"\n");
    for name in ["order-base", "order-overlay"] {
        let filename = format!("{}-1.0.0-py3-none-any.whl", name.replace('-', "_"));
        let hash = hex::encode(Sha256::digest(fs::read(wheels.join(&filename))?));
        writeln!(
            pylock,
            "\n[[packages]]\nname = \"{name}\"\nversion = \"1.0.0\"\n\
             archive = {{ path = \"wheels/{filename}\", hashes = {{ sha256 = \"{hash}\" }} }}"
        )?;
    }
    context.temp_dir.child("pylock.toml").write_str(&pylock)?;
    context
        .pip_install()
        .args([
            "--preview-features",
            "pylock",
            "--reinstall",
            "-r",
            "pylock.toml",
        ])
        .env(EnvVars::UV_CONCURRENT_INSTALLS, "4")
        .assert()
        .success();
    context
        .assert_command("import order_base, order_overlay")
        .success();
    Ok(())
}
