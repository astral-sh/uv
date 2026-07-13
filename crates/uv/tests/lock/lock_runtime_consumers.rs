use std::fmt::Write;

use anyhow::Result;
use assert_cmd::assert::OutputAssertExt;
use assert_fs::fixture::ChildPath;
use assert_fs::prelude::*;
use async_zip::base::write::ZipFileWriter;
use async_zip::{Compression, ZipEntryBuilder};
use futures::executor::block_on;

use uv_test::uv_snapshot;

fn write_wheel(path: &ChildPath, name: &str, version: &str, requires_dist: &[&str]) -> Result<()> {
    let mut zip = ZipFileWriter::new(Vec::new());
    let dist_info = format!("{}-{version}.dist-info", name.replace('-', "_"));

    let entry = ZipEntryBuilder::new(
        format!("{}.py", name.replace('-', "_")).into(),
        Compression::Stored,
    );
    block_on(zip.write_entry_whole(entry, b""))?;

    let entry = ZipEntryBuilder::new(format!("{dist_info}/METADATA").into(), Compression::Stored);
    let mut metadata = format!("Metadata-Version: 2.3\nName: {name}\nVersion: {version}\n");
    for requirement in requires_dist {
        writeln!(metadata, "Requires-Dist: {requirement}")?;
    }
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

/// Verify that a PEP 723 script captures hook requirements and can replay the locked build
/// environment with both frozen sync and run.
#[test]
fn script_replays_locked_build_dependencies() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    write_wheel(
        &links_dir.child("builder-1.0.0-py3-none-any.whl"),
        "builder",
        "1.0.0",
        &[],
    )?;

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = []
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
    )?;
    dep_dir.child("build_backend.py").write_str(
        r#"
from importlib.metadata import version
from pathlib import Path
from zipfile import ZipFile

def get_requires_for_build_wheel(config_settings=None):
    return ["builder==1.0.0"]

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    filename = "dep-0.1.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("dep/__init__.py", f"BUILDER = {version('builder')!r}\n")
        wheel.writestr(
            "dep-0.1.0.dist-info/METADATA",
            "Metadata-Version: 2.3\nName: dep\nVersion: 0.1.0\n",
        )
        wheel.writestr(
            "dep-0.1.0.dist-info/WHEEL",
            "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
        )
        wheel.writestr("dep-0.1.0.dist-info/RECORD", "")
    return filename
"#,
    )?;

    context.temp_dir.child("script.py").write_str(
        r#"
# /// script
# requires-python = ">=3.12"
# dependencies = ["dep"]
#
# [tool.uv.sources]
# dep = { path = "dep" }
# ///

import dep

print(f"builder={dep.BUILDER}")
"#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--script")
        .arg("script.py")
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    ");

    let lock = context.read("script.py.lock");
    assert!(lock.contains("revision = 4"), "{lock}");
    assert!(lock.contains("build-only = true"), "{lock}");
    assert!(lock.contains("name = \"builder\""), "{lock}");
    assert!(lock.contains("build-dependencies = ["), "{lock}");

    fs_err::remove_dir_all(&context.cache_dir)?;
    uv_snapshot!(context.filters(), context
        .sync()
        .arg("--script")
        .arg("script.py")
        .arg("--frozen")
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Creating script environment at: [CACHE_DIR]/environments-v2/script-[HASH]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + dep==0.1.0 (from file://[TEMP_DIR]/dep)
    ");

    fs_err::remove_dir_all(&context.cache_dir)?;
    uv_snapshot!(context.filters(), context
        .run()
        .arg("--frozen")
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("script.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    builder=1.0.0

    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + dep==0.1.0 (from file://[TEMP_DIR]/dep)
    ");

    Ok(())
}

/// Verify that runtime exports omit both build-only packages and build-scoped dependency edges
/// when a package is shared between the runtime and build graphs.
#[cfg(feature = "test-universal")]
#[test]
fn exports_exclude_locked_build_dependencies() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_cyclonedx_filters();

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    write_wheel(
        &links_dir.child("builder-1.0.0-py3-none-any.whl"),
        "builder",
        "1.0.0",
        &["leaf>=1"],
    )?;
    write_wheel(
        &links_dir.child("leaf-1.0.0-py3-none-any.whl"),
        "leaf",
        "1.0.0",
        &[],
    )?;
    write_wheel(
        &links_dir.child("leaf-2.0.0-py3-none-any.whl"),
        "leaf",
        "2.0.0",
        &[],
    )?;

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["builder==1.0.0", "leaf==1.0.0"]
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
    )?;
    dep_dir.child("build_backend.py").write_str(
        r"
def get_requires_for_build_wheel(config_settings=None):
    return []
",
    )?;

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["builder==1.0.0", "dep"]

        [tool.uv.sources]
        dep = { path = "dep" }
        "#,
    )?;

    context
        .lock()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();

    let lock = context.read("uv.lock");
    assert!(lock.contains("build-only = true"), "{lock}");
    assert!(
        lock.contains("resolution-id = \"build:dep:wheel:build:"),
        "{lock}"
    );
    assert!(lock.contains("version = \"1.0.0\""), "{lock}");
    assert!(lock.contains("version = \"2.0.0\""), "{lock}");

    uv_snapshot!(context.filters(), context
        .export()
        .arg("--frozen")
        .arg("--format")
        .arg("cyclonedx1.5")
        .arg("--preview-features")
        .arg("lock-build-dependencies,sbom-export"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "bomFormat": "CycloneDX",
      "specVersion": "1.5",
      "version": 1,
      "serialNumber": "[SERIAL_NUMBER]",
      "metadata": {
        "timestamp": "[TIMESTAMP]",
        "tools": [
          {
            "vendor": "Astral Software Inc.",
            "name": "uv",
            "version": "[VERSION]"
          }
        ],
        "component": {
          "type": "library",
          "bom-ref": "project-1@0.1.0",
          "name": "project",
          "version": "0.1.0",
          "properties": [
            {
              "name": "uv:package:is_project_root",
              "value": "true"
            }
          ]
        }
      },
      "components": [
        {
          "type": "library",
          "bom-ref": "builder-2@1.0.0",
          "name": "builder",
          "version": "1.0.0",
          "purl": "pkg:pypi/builder@1.0.0"
        },
        {
          "type": "library",
          "bom-ref": "dep-3@0.1.0",
          "name": "dep",
          "version": "0.1.0"
        },
        {
          "type": "library",
          "bom-ref": "leaf-4@2.0.0",
          "name": "leaf",
          "version": "2.0.0",
          "purl": "pkg:pypi/leaf@2.0.0"
        }
      ],
      "dependencies": [
        {
          "ref": "builder-2@1.0.0",
          "dependsOn": [
            "leaf-4@2.0.0"
          ]
        },
        {
          "ref": "dep-3@0.1.0"
        },
        {
          "ref": "leaf-4@2.0.0"
        },
        {
          "ref": "project-1@0.1.0",
          "dependsOn": [
            "builder-2@1.0.0",
            "dep-3@0.1.0"
          ]
        }
      ]
    }
    ----- stderr -----
    "#);

    uv_snapshot!(context.filters(), context
        .export()
        .arg("--frozen")
        .arg("--format")
        .arg("pylock.toml")
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    # This file was autogenerated by uv via the following command:
    #    uv export --cache-dir [CACHE_DIR] --frozen --format pylock.toml --preview-features lock-build-dependencies
    lock-version = "1.0"
    created-by = "uv"
    requires-python = ">=3.12"

    [[packages]]
    name = "builder"
    version = "1.0.0"
    index = "file://[TEMP_DIR]/links"
    wheels = [{ url = "file://[TEMP_DIR]/links/builder-1.0.0-py3-none-any.whl", hashes = {} }]

    [[packages]]
    name = "dep"
    directory = { path = "dep", editable = false }

    [[packages]]
    name = "leaf"
    version = "2.0.0"
    index = "file://[TEMP_DIR]/links"
    wheels = [{ url = "file://[TEMP_DIR]/links/leaf-2.0.0-py3-none-any.whl", hashes = {} }]

    ----- stderr -----
    "#);

    Ok(())
}
