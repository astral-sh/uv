use anyhow::Result;
use assert_cmd::assert::OutputAssertExt;
use assert_fs::fixture::ChildPath;
use assert_fs::prelude::*;
use async_zip::base::write::ZipFileWriter;
use async_zip::{Compression, ZipEntryBuilder};
use futures::executor::block_on;

use uv_test::uv_snapshot;

fn write_wheel(path: &ChildPath, name: &str, version: &str) -> Result<()> {
    let mut zip = ZipFileWriter::new(Vec::new());
    let dist_info = format!("{}-{version}.dist-info", name.replace('-', "_"));

    let entry = ZipEntryBuilder::new(
        format!("{}.py", name.replace('-', "_")).into(),
        Compression::Stored,
    );
    block_on(zip.write_entry_whole(entry, b""))?;
    let entry = ZipEntryBuilder::new(format!("{dist_info}/METADATA").into(), Compression::Stored);
    block_on(zip.write_entry_whole(
        entry,
        format!("Metadata-Version: 2.3\nName: {name}\nVersion: {version}\n").as_bytes(),
    ))?;
    let entry = ZipEntryBuilder::new(format!("{dist_info}/WHEEL").into(), Compression::Stored);
    block_on(zip.write_entry_whole(
        entry,
        b"Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
    ))?;
    let entry = ZipEntryBuilder::new(format!("{dist_info}/RECORD").into(), Compression::Stored);
    block_on(zip.write_entry_whole(entry, b""))?;
    fs_err::write(path.path(), block_on(zip.close())?)?;

    Ok(())
}

fn write_backend_package(
    path: &ChildPath,
    name: &str,
    hook_requirement: Option<&str>,
) -> Result<()> {
    path.create_dir_all()?;
    path.child("pyproject.toml").write_str(&format!(
        r#"
        [project]
        name = "{name}"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = []
        backend-path = ["."]
        build-backend = "build_backend"
        "#
    ))?;

    let hook_requirements = hook_requirement.map_or_else(
        || "[]".to_string(),
        |requirement| format!("[\"{requirement}\"]"),
    );
    path.child("build_backend.py").write_str(&format!(
        r#"
from pathlib import Path
from zipfile import ZipFile

def get_requires_for_build_wheel(config_settings=None):
    return {hook_requirements}

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    filename = "{name}-0.1.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("{name}/__init__.py", "")
        wheel.writestr(
            "{name}-0.1.0.dist-info/METADATA",
            "Metadata-Version: 2.3\nName: {name}\nVersion: 0.1.0\n",
        )
        wheel.writestr(
            "{name}-0.1.0.dist-info/WHEEL",
            "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
        )
        wheel.writestr("{name}-0.1.0.dist-info/RECORD", "")
    return filename
"#
    ))?;

    Ok(())
}

/// A lock created with global `--no-build` cannot replay a later source build under `--frozen`.
#[test]
fn frozen_build_rejects_lock_created_without_build_support() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    write_wheel(
        &links_dir.child("helper-0.1.0-py3-none-any.whl"),
        "helper",
        "0.1.0",
    )?;
    write_backend_package(&context.temp_dir.child("dep"), "dep", Some("helper==0.1.0"))?;
    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep"]

        [tool.uv.sources]
        dep = { path = "dep" }
        "#,
    )?;

    context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--no-index")
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-build")
        .assert()
        .success();

    let lock = context.read("uv.lock");
    assert!(lock.contains("revision = 3"), "{lock}");
    assert!(!lock.contains("build-dependencies = ["), "{lock}");

    uv_snapshot!(context.filters(), context
        .sync()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--no-index")
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--frozen"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: The lockfile does not contain build dependencies for `dep`; run `uv lock --preview-features lock-build-dependencies` without disabling builds for this package
    ");

    Ok(())
}

/// A build-capable lock may still omit a package captured under `--no-build-package`; frozen
/// replay must fail before resolving its backend requirements.
#[test]
fn frozen_build_rejects_uncaptured_package() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    write_wheel(
        &links_dir.child("helper-0.1.0-py3-none-any.whl"),
        "helper",
        "0.1.0",
    )?;
    write_backend_package(&context.temp_dir.child("dep"), "dep", Some("helper==0.1.0"))?;
    write_backend_package(&context.temp_dir.child("captured"), "captured", None)?;
    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["captured", "dep"]

        [tool.uv.sources]
        captured = { path = "captured" }
        dep = { path = "dep" }
        "#,
    )?;

    context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--no-index")
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-build-package")
        .arg("dep")
        .assert()
        .success();

    let lock = context.read("uv.lock");
    assert!(lock.contains("revision = 4"), "{lock}");
    assert!(lock.contains("name = \"captured\""), "{lock}");
    assert!(lock.contains("build-dependencies = []"), "{lock}");
    assert!(!lock.contains("name = \"helper\""), "{lock}");

    uv_snapshot!(context.filters(), context
        .sync()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--no-index")
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--frozen"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: The lockfile does not contain build dependencies for `dep`; run `uv lock --preview-features lock-build-dependencies` without disabling builds for this package
    ");

    uv_snapshot!(context.filters(), context
        .sync()
        .arg("--no-index")
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--frozen"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: The lockfile does not contain build dependencies for `dep`; run `uv lock --preview-features lock-build-dependencies` without disabling builds for this package
    ");

    Ok(())
}

/// Captured empty graphs and wheel selections under `--no-build` remain valid frozen replays.
#[test]
fn frozen_build_preserves_empty_graphs_and_no_build_wheels() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    write_backend_package(&context.temp_dir.child("dep"), "dep", None)?;
    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep"]

        [tool.uv.sources]
        dep = { path = "dep" }
        "#,
    )?;

    context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--no-index")
        .assert()
        .success();

    let lock = context.read("uv.lock");
    assert!(lock.contains("revision = 4"), "{lock}");
    assert!(lock.contains("build-dependencies = []"), "{lock}");

    uv_snapshot!(context.filters(), context
        .sync()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--no-index")
        .arg("--frozen"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + dep==0.1.0 (from file://[TEMP_DIR]/dep)
    ");

    let wheel_context = uv_test::test_context!("3.12");
    let links_dir = wheel_context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    write_wheel(
        &links_dir.child("runtime-0.1.0-py3-none-any.whl"),
        "runtime",
        "0.1.0",
    )?;
    wheel_context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["runtime==0.1.0"]
        "#,
    )?;

    wheel_context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--no-index")
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-build")
        .assert()
        .success();

    let lock = wheel_context.read("uv.lock");
    assert!(lock.contains("revision = 3"), "{lock}");

    uv_snapshot!(wheel_context.filters(), wheel_context
        .sync()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--no-index")
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-build")
        .arg("--frozen"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + runtime==0.1.0
    ");

    Ok(())
}
