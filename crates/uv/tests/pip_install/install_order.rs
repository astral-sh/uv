use std::fmt::Write;
use std::process::Command;

use anyhow::{Context, Result};
use assert_cmd::prelude::*;
use assert_fs::prelude::*;
use fs_err as fs;
use indoc::{formatdoc, indoc};

use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_static::EnvVars;
use uv_test::packse::PackseServer;
use uv_test::packse::scenario::Scenario;
use uv_test::{TestContext, uv_snapshot, venv_bin_path};

const OVERLAY_PACKAGE: &str = indoc! {r#"
    [packages.order-overlay.versions."1.0.0"]
    requires = ["order-base==1.0.0"]
    sdist = false
    entry_points = ["order-command"]
    wheel_files = { "order_shared.py" = "OWNER = 'order-overlay'\n" }
"#};

/// Serve packages that share a module and console script with a larger dependency.
fn install_order_index(packages: &str) -> Result<PackseServer> {
    let mut scenario = toml::from_str::<Scenario>(&formatdoc! {r#"
        name = "install-order"

        [root]

        [expected]
        satisfiable = true

        [packages.order-base.versions."1.0.0"]
        sdist = false
        entry_points = ["order-command"]
        wheel_files = {{ "order_shared.py" = "OWNER = 'order-base'\n" }}

        {packages}
    "#})?;
    let name: PackageName = "order-base".parse()?;
    let version: Version = "1.0.0".parse()?;
    let base = scenario
        .packages
        .get_mut(&name)
        .and_then(|package| package.versions.get_mut(&version))
        .context("scenario should contain order-base==1.0.0")?;

    // Console scripts are generated after linking the wheel's files. A larger dependency makes
    // the entry-point race visible when installs are only sorted before running in parallel.
    for index in 0..1024 {
        base.wheel_files.insert(
            format!("order_base/padding/file_{index:04}.txt"),
            "padding\n".to_string(),
        );
    }
    Ok(PackseServer::from_scenario(&scenario))
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
        format!("Hello from {expected}!\n")
    );
    Ok(())
}

#[test]
fn install_dependencies_before_dependents() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = install_order_index(OVERLAY_PACKAGE)?;

    uv_snapshot!(context.filters(), context.pip_install()
        .args(["--link-mode", "copy", "order-overlay"])
        .arg("--index-url").arg(server.index_url())
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
        .args(["--link-mode", "copy", "--reinstall", "order-overlay"])
        .arg("--index-url")
        .arg(server.index_url())
        .env(EnvVars::UV_CONCURRENT_INSTALLS, "4")
        .assert()
        .success();
    assert_winner(&context, "order-overlay")?;
    Ok(())
}

#[test]
fn install_dependency_cycles_in_name_order() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = install_order_index(indoc! {r#"
        [packages.cycle-alpha.versions."1.0.0"]
        requires = ["cycle-zebra==1.0.0"]
        sdist = false
        entry_points = ["order-command"]
        wheel_files = { "order_shared.py" = "OWNER = 'cycle-alpha'\n" }

        [packages.cycle-zebra.versions."1.0.0"]
        requires = ["cycle-alpha==1.0.0", "order-base==1.0.0"]
        sdist = false
        entry_points = ["order-command"]
        wheel_files = { "order_shared.py" = "OWNER = 'cycle-zebra'\n" }
    "#})?;

    uv_snapshot!(context.filters(), context.pip_install()
        .args(["--link-mode", "copy", "cycle-alpha"])
        .arg("--index-url").arg(server.index_url())
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
    let server = install_order_index(indoc! {r#"
        [packages.order-bridge.versions."1.0.0"]
        requires = ["order-base==1.0.0"]
        sdist = false

        [packages.order-overlay.versions."1.0.0"]
        requires = ["order-bridge==1.0.0"]
        sdist = false
        entry_points = ["order-command"]
        wheel_files = { "order_shared.py" = "OWNER = 'order-overlay'\n" }
    "#})?;
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
        .args(["--link-mode", "copy", "--no-install-package", "order-bridge"])
        .arg("--index-url").arg(server.index_url())
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

#[tokio::test]
async fn install_build_environment_dependencies_in_order() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = install_order_index(&formatdoc! {r#"
        {OVERLAY_PACKAGE}

        [packages.built-project.versions."1.0.0"]
        sdist = false
    "#})?;
    let filename = "built_project-1.0.0-py3-none-any.whl";
    let wheel = reqwest::get(server.file_url(filename))
        .await?
        .error_for_status()?
        .bytes()
        .await?;
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
        assert subprocess.check_output(["order-command"], text=True).strip() == "Hello from order-overlay!"

        def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
            wheel = Path(os.environ["BUILT_PROJECT_WHEEL"])
            shutil.copyfile(wheel, Path(wheel_directory) / wheel.name)
            return wheel.name
    "#})?;

    uv_snapshot!(context.filters(), context.pip_install()
        .args(["--link-mode", "copy", "./project"])
        .arg("--index-url").arg(server.index_url())
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
    let server = install_order_index(OVERLAY_PACKAGE)?;
    context
        .temp_dir
        .child("requirements.txt")
        .write_str("order-base==1.0.0\norder-overlay==1.0.0\n")?;

    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.txt")
        .arg("--index-url").arg(server.index_url())
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
        let hash = server
            .files()
            .find_map(|(name, hash)| (name == filename).then_some(hash))
            .context("scenario wheel should be indexed")?;
        let url = server.file_url(&filename);
        writeln!(
            pylock,
            "\n[[packages]]\nname = \"{name}\"\nversion = \"1.0.0\"\n\
             archive = {{ url = \"{url}\", hashes = {{ sha256 = \"{hash}\" }} }}"
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
