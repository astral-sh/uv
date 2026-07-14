use anyhow::Result;
use assert_cmd::assert::OutputAssertExt;
use assert_fs::fixture::ChildPath;
use assert_fs::prelude::*;
use async_zip::base::write::ZipFileWriter;
use async_zip::{Compression, ZipEntryBuilder};
use futures::executor::block_on;
use indoc::indoc;

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

/// `uv check --no-sync` installs a locked tool in a separate environment and must not resolve an
/// uncaptured build graph when the lockfile is frozen.
#[test]
fn check_no_sync_rejects_uncaptured_frozen_build() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let links = context.temp_dir.child("links");
    links.create_dir_all()?;
    write_wheel(
        &links.child("helper-0.1.0-py3-none-any.whl"),
        "helper",
        "0.1.0",
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

        [build-system]
        requires = []
        backend-path = ["."]
        build-backend = "project_backend"

        [dependency-groups]
        dev = ["ty"]

        [tool.uv.sources]
        ty = { workspace = true }

        [tool.uv.workspace]
        members = ["ty"]
    "#})?;
    context.temp_dir.child("main.py").write_str("x = 1")?;
    context
        .temp_dir
        .child("project_backend.py")
        .write_str("def get_requires_for_build_wheel(config_settings=None):\n    return []\n")?;

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
        from pathlib import Path
        from zipfile import ZipFile

        def get_requires_for_build_wheel(config_settings=None):
            return ["helper==0.1.0"]

        def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
            filename = "ty-1.2.3-py3-none-any.whl"
            with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
                wheel.writestr("ty/__init__.py", "def main():\n    print('All checks passed!')\n")
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
    "#})?;

    context
        .lock()
        .arg("--find-links")
        .arg(links.path())
        .arg("--no-index")
        .arg("--no-build-package")
        .arg("ty")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();

    let lock = context.read("uv.lock");
    assert!(lock.contains("revision = 4"), "{lock}");
    assert!(lock.contains("build-dependencies = []"), "{lock}");
    assert!(!lock.contains("name = \"helper\""), "{lock}");

    uv_snapshot!(context.filters(), context
        .check()
        .arg("--no-sync")
        .arg("--frozen")
        .arg("--no-index")
        .arg("--find-links")
        .arg(links.path())
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: `uv check` is experimental and may change without warning. Pass `--preview-features check-command` to disable this warning.
    error: The lockfile does not contain build dependencies for `ty`; run `uv lock --preview-features lock-build-dependencies` without disabling builds for this package
    ");

    uv_snapshot!(context.filters(), context
        .check()
        .arg("--no-sync")
        .arg("--frozen")
        .arg("--no-index")
        .arg("--find-links")
        .arg(links.path()), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: `uv check` is experimental and may change without warning. Pass `--preview-features check-command` to disable this warning.
    error: The lockfile does not contain build dependencies for `ty`; run `uv lock --preview-features lock-build-dependencies` without disabling builds for this package
    ");

    Ok(())
}

/// `uv check --no-sync` replays a compatible `uv_build` tool from its captured empty stages and
/// rejects later PEP 517 fallback requirements.
#[test]
fn check_no_sync_allows_frozen_direct_build() -> Result<()> {
    let context = uv_test::test_context!("3.12");

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

        [project.scripts]
        ty = "ty:main"

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"
    "#})?;
    let ty_package = ty.child("src").child("ty");
    ty_package.create_dir_all()?;
    ty_package
        .child("__init__.py")
        .write_str("def main():\n    print('All checks passed!')\n")?;

    context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--no-index")
        .assert()
        .success();

    let lock = context.read("uv.lock");
    assert!(lock.contains("revision = 4"), "{lock}");
    assert!(
        lock.contains(r#"{ name = "uv-build", specifier = ">=0.7,<10000" }"#),
        "{lock}"
    );
    assert!(lock.contains(r#"build-backend = "uv_build""#), "{lock}");
    assert!(lock.contains("build-dependencies = []"), "{lock}");
    assert!(!lock.contains("\nstage = "), "{lock}");

    uv_snapshot!(context.filters(), context
        .check()
        .arg("--no-sync")
        .arg("--frozen")
        .arg("--no-index")
        .arg("--no-cache")
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    warning: `uv check` is experimental and may change without warning. Pass `--preview-features check-command` to disable this warning.
    Installed 1 package in [TIME]
    ");
    assert!(!context.site_packages().join("ty").exists());

    ty.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "ty"
        version = "1.2.3"
        requires-python = ">=3.12"
        dependencies = []

        [project.scripts]
        ty = "ty:main"

        [build-system]
        requires = ["uv_build>=0.5,<0.6"]
        build-backend = "uv_build"
    "#})?;
    uv_snapshot!(context.filters(), context
        .check()
        .arg("--no-sync")
        .arg("--frozen")
        .arg("--no-index")
        .arg("--no-cache")
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    warning: `uv check` is experimental and may change without warning. Pass `--preview-features check-command` to disable this warning.
      × Failed to build `ty @ file://[TEMP_DIR]/ty`
      ├─▶ Failed to resolve requirements from `build-system.requires`
      ╰─▶ The initial build requirements for `ty` do not match the locked bootstrap environment
    ");

    ty.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "ty"
        version = "1.2.3"
        requires-python = ">=3.12"
        dependencies = []

        [project.scripts]
        ty = "ty:main"

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"
    "#})?;
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

        [tool.uv.extra-build-dependencies]
        ty = ["helper==0.1.0"]
    "#})?;
    uv_snapshot!(context.filters(), context
        .check()
        .arg("--no-sync")
        .arg("--frozen")
        .arg("--no-index")
        .arg("--no-cache")
        .arg("--preview-features")
        .arg("extra-build-dependencies,lock-build-dependencies"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    warning: `uv check` is experimental and may change without warning. Pass `--preview-features check-command` to disable this warning.
      × Failed to build `ty @ file://[TEMP_DIR]/ty`
      ├─▶ Failed to resolve requirements from `build-system.requires` and `extra-build-dependencies`
      ╰─▶ The initial build requirements for `ty` do not match the locked bootstrap environment
    ");

    Ok(())
}

/// `uv check --no-sync` cannot bypass a captured tool build environment when build isolation is
/// disabled globally or for the locked tool.
#[test]
fn check_no_sync_rejects_non_isolated_frozen_build() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let links = context.temp_dir.child("links");
    links.create_dir_all()?;
    write_wheel(
        &links.child("helper-0.1.0-py3-none-any.whl"),
        "helper",
        "0.1.0",
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
        from pathlib import Path
        from zipfile import ZipFile

        def get_requires_for_build_wheel(config_settings=None):
            return ["helper==0.1.0"]

        def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
            filename = "ty-1.2.3-py3-none-any.whl"
            with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
                wheel.writestr("ty/__init__.py", "def main():\n    print('All checks passed!')\n")
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
    "#})?;

    context
        .lock()
        .arg("--find-links")
        .arg(links.path())
        .arg("--no-index")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();

    let lock = context.read("uv.lock");
    assert!(lock.contains("revision = 4"), "{lock}");
    assert!(lock.contains("name = \"helper\""), "{lock}");

    uv_snapshot!(context.filters(), context
        .check()
        .arg("--no-sync")
        .arg("--frozen")
        .arg("--no-index")
        .arg("--no-cache")
        .arg("--find-links")
        .arg(links.path())
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--no-build-isolation"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    warning: `uv check` is experimental and may change without warning. Pass `--preview-features check-command` to disable this warning.
      × Failed to build `ty @ file://[TEMP_DIR]/ty`
      ╰─▶ Cannot replay locked build dependencies for `ty` without build isolation
    ");

    uv_snapshot!(context.filters(), context
        .check()
        .arg("--no-sync")
        .arg("--frozen")
        .arg("--no-index")
        .arg("--no-cache")
        .arg("--find-links")
        .arg(links.path())
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--no-build-isolation-package")
        .arg("ty"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    warning: `uv check` is experimental and may change without warning. Pass `--preview-features check-command` to disable this warning.
      × Failed to build `ty @ file://[TEMP_DIR]/ty`
      ╰─▶ Cannot replay locked build dependencies for `ty` without build isolation
    ");

    Ok(())
}
