use std::fmt::Write;

use anyhow::Result;
use assert_cmd::assert::OutputAssertExt;
use assert_fs::fixture::ChildPath;
use assert_fs::prelude::*;
use async_zip::base::write::ZipFileWriter;
use async_zip::{Compression, ZipEntryBuilder};
use futures::executor::block_on;
use indoc::indoc;

use uv_test::uv_snapshot;

fn write_builder_wheel(path: &ChildPath, version: &str, value: &str) -> Result<()> {
    let mut zip = ZipFileWriter::new(Vec::new());
    let dist_info = format!("builder-{version}.dist-info");

    let entry = ZipEntryBuilder::new("builder.py".into(), Compression::Stored);
    block_on(zip.write_entry_whole(entry, format!("VALUE = {value:?}\n").as_bytes()))?;
    let entry = ZipEntryBuilder::new(format!("{dist_info}/METADATA").into(), Compression::Stored);
    let mut metadata = String::new();
    writeln!(metadata, "Metadata-Version: 2.3")?;
    writeln!(metadata, "Name: builder")?;
    writeln!(metadata, "Version: {version}")?;
    block_on(zip.write_entry_whole(entry, metadata.as_bytes()))?;
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

/// A locked tool installed outside the project environment must replay its build closure and must
/// not reuse an environment built with a different locked builder.
#[test]
fn check_no_sync_replays_locked_build_dependencies() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let links = context.temp_dir.child("links");
    links.create_dir_all()?;
    write_builder_wheel(
        &links.child("builder-0.1.0-py3-none-any.whl"),
        "0.1.0",
        "first",
    )?;
    write_builder_wheel(
        &links.child("builder-0.2.0-py3-none-any.whl"),
        "0.2.0",
        "second",
    )?;

    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [dependency-groups]
        dev = ["ty"]

        [tool.uv.sources]
        ty = { workspace = true }

        [tool.uv.workspace]
        members = ["ty"]
    "#})?;
    context.temp_dir.child("main.py").write_str("x = 1")?;

    let ty = context.temp_dir.child("ty");
    ty.create_dir_all()?;
    ty.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "ty"
        version = "1.2.3"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = []
        backend-path = ["."]
        build-backend = "build_backend"
    "#})?;
    ty.child("build_backend.py").write_str(indoc! {r#"
        import os
        from pathlib import Path
        from zipfile import ZipFile

        def get_requires_for_build_wheel(config_settings=None):
            return [f"builder=={os.environ['UV_TEST_BUILDER_VERSION']}"]

        def get_requires_for_build_editable(config_settings=None):
            return get_requires_for_build_wheel(config_settings)

        def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
            from builder import VALUE

            filename = "ty-1.2.3-py3-none-any.whl"
            with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
                wheel.writestr(
                    "ty/__init__.py",
                    f"def main():\n    print('All checks passed! (builder {VALUE})')\n",
                )
                wheel.writestr(
                    "ty-1.2.3.dist-info/METADATA",
                    "Metadata-Version: 2.3\nName: ty\nVersion: 1.2.3\n",
                )
                wheel.writestr(
                    "ty-1.2.3.dist-info/WHEEL",
                    "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
                )
                wheel.writestr(
                    "ty-1.2.3.dist-info/entry_points.txt",
                    "[console_scripts]\nty = ty:main\n",
                )
                wheel.writestr("ty-1.2.3.dist-info/RECORD", "")
            return filename

        def build_editable(wheel_directory, config_settings=None, metadata_directory=None):
            return build_wheel(wheel_directory, config_settings, metadata_directory)
    "#})?;

    context
        .lock()
        .arg("--find-links")
        .arg(links.path())
        .arg("--no-index")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .env("UV_TEST_BUILDER_VERSION", "0.1.0")
        .assert()
        .success();

    // Do not expose the find-links directory to `check`: the source tool can only be built by
    // replaying the builder distribution recorded in the lock.
    uv_snapshot!(
        context.filters(),
        context
            .check()
            .arg("--no-sync")
            .arg("--frozen")
            .arg("--offline")
            .arg("--no-index")
            .arg("--preview-features")
            .arg("lock-build-dependencies")
            .env("UV_TEST_BUILDER_VERSION", "0.1.0"),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed! (builder first)

    ----- stderr -----
    warning: `uv check` is experimental and may change without warning. Pass `--preview-features check-command` to disable this warning.
    Installed 1 package in [TIME]
    "
    );
    assert!(!context.site_packages().join("ty").exists());

    fs_err::remove_file(context.temp_dir.join("uv.lock"))?;
    context
        .lock()
        .arg("--find-links")
        .arg(links.path())
        .arg("--no-index")
        .arg("--no-cache")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .env("UV_TEST_BUILDER_VERSION", "0.2.0")
        .assert()
        .success();

    uv_snapshot!(
        context.filters(),
        context
            .check()
            .arg("--no-sync")
            .arg("--frozen")
            .arg("--offline")
            .arg("--no-index")
            .arg("--preview-features")
            .arg("lock-build-dependencies")
            .env("UV_TEST_BUILDER_VERSION", "0.2.0"),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed! (builder second)

    ----- stderr -----
    warning: `uv check` is experimental and may change without warning. Pass `--preview-features check-command` to disable this warning.
    Installed 1 package in [TIME]
    "
    );
    assert!(!context.site_packages().join("ty").exists());

    Ok(())
}
