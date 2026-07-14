use std::fmt::Write;

use anyhow::Result;
use assert_cmd::assert::OutputAssertExt;
use assert_fs::fixture::ChildPath;
use assert_fs::prelude::*;
use async_zip::base::write::ZipFileWriter;
use async_zip::{Compression, ZipEntryBuilder};
use futures::executor::block_on;
use url::Url;

use uv_static::EnvVars;
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

/// Verify that extra build dependencies on a foreign-only source root still participate in
/// PEP 723 lock freshness checks.
#[test]
fn script_foreign_source_extra_build_dependencies_invalidate() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    write_wheel(
        &links_dir.child("helper-0.1.0-py3-none-any.whl"),
        "helper",
        "0.1.0",
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
    dep_dir.child("build_backend.py").write_str("")?;

    let foreign_marker = if cfg!(target_os = "windows") {
        "sys_platform != 'win32'"
    } else {
        "sys_platform == 'win32'"
    };
    let script = context.temp_dir.child("script.py");
    script.write_str(&format!(
        r#"
# /// script
# requires-python = ">=3.12"
# dependencies = ["dep ; {foreign_marker}"]
#
# [tool.uv.sources]
# dep = {{ path = "dep" }}
#
# [tool.uv.extra-build-dependencies]
# dep = ["helper==0.1.0"]
# ///
"#
    ))?;

    context
        .lock()
        .arg("--script")
        .arg("script.py")
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--preview-features")
        .arg("extra-build-dependencies,lock-build-dependencies")
        .assert()
        .success();

    let lock = context.read("script.py.lock");
    assert!(lock.contains("build-settings = "), "{lock}");
    assert!(lock.contains("name = \"helper\""), "{lock}");

    script.write_str(&format!(
        r#"
# /// script
# requires-python = ">=3.12"
# dependencies = ["dep ; {foreign_marker}"]
#
# [tool.uv.sources]
# dep = {{ path = "dep" }}
# ///
"#
    ))?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--script")
        .arg("script.py")
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--preview-features")
        .arg("extra-build-dependencies,lock-build-dependencies")
        .arg("--locked"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided.

    hint: To update the lockfile, run `uv lock`.
    ");

    Ok(())
}

/// Verify that changing a registry source archive used directly by a PEP 723 script invalidates
/// its captured build environment.
#[test]
fn script_registry_source_build_dependencies_invalidate() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    write_wheel(
        &links_dir.child("helper_a-0.1.0-py3-none-any.whl"),
        "helper-a",
        "0.1.0",
        &[],
    )?;
    write_wheel(
        &links_dir.child("helper_b-0.1.0-py3-none-any.whl"),
        "helper-b",
        "0.1.0",
        &[],
    )?;

    let source_dist = links_dir.child("dep-0.1.0.zip");
    let write_source_dist = |helper: &str| -> Result<()> {
        let mut zip = ZipFileWriter::new(Vec::new());
        let entry = ZipEntryBuilder::new("dep-0.1.0/pyproject.toml".into(), Compression::Stored);
        let pyproject_toml = format!(
            r#"
            [project]
            name = "dep"
            version = "0.1.0"
            requires-python = ">=3.12"

            [build-system]
            requires = ["{helper}==0.1.0"]
            backend-path = ["."]
            build-backend = "build_backend"
            "#
        );
        block_on(zip.write_entry_whole(entry, pyproject_toml.as_bytes()))?;
        let entry = ZipEntryBuilder::new("dep-0.1.0/build_backend.py".into(), Compression::Stored);
        block_on(zip.write_entry_whole(
            entry,
            b"def get_requires_for_build_wheel(config_settings=None):\n    return []\n",
        ))?;
        fs_err::write(source_dist.path(), block_on(zip.close())?)?;
        Ok(())
    };
    write_source_dist("helper-a")?;

    context.temp_dir.child("script.py").write_str(
        r#"
# /// script
# requires-python = ">=3.12"
# dependencies = ["dep==0.1.0"]
# ///
"#,
    )?;

    context
        .lock()
        .arg("--script")
        .arg("script.py")
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--no-cache")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();

    let lock = context.read("script.py.lock");
    assert!(lock.contains("revision = 4"), "{lock}");
    assert!(lock.contains("name = \"helper-a\""), "{lock}");
    assert!(
        lock.contains(r#"build-requires = [{ name = "helper-a", specifier = "==0.1.0" }]"#),
        "{lock}"
    );

    write_source_dist("helper-b")?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--script")
        .arg("script.py")
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--no-cache")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--locked"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided.

    hint: To update the lockfile, run `uv lock`.
    ");

    Ok(())
}

/// Verify that a mutable registry source behind an immutable wheel root invalidates its captured
/// build environment for a PEP 723 script.
#[test]
fn script_transitive_registry_source_build_dependencies_invalidate() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    write_wheel(
        &links_dir.child("parent-0.1.0-py3-none-any.whl"),
        "parent",
        "0.1.0",
        &["dep==0.1.0"],
    )?;
    write_wheel(
        &links_dir.child("helper_a-0.1.0-py3-none-any.whl"),
        "helper-a",
        "0.1.0",
        &[],
    )?;
    write_wheel(
        &links_dir.child("helper_b-0.1.0-py3-none-any.whl"),
        "helper-b",
        "0.1.0",
        &[],
    )?;

    let source_dist = links_dir.child("dep-0.1.0.zip");
    let write_source_dist = |helper: &str| -> Result<()> {
        let mut zip = ZipFileWriter::new(Vec::new());
        let entry = ZipEntryBuilder::new("dep-0.1.0/pyproject.toml".into(), Compression::Stored);
        let pyproject_toml = format!(
            r#"
            [project]
            name = "dep"
            version = "0.1.0"
            requires-python = ">=3.12"

            [build-system]
            requires = ["{helper}==0.1.0"]
            backend-path = ["."]
            build-backend = "build_backend"
            "#
        );
        block_on(zip.write_entry_whole(entry, pyproject_toml.as_bytes()))?;
        let entry = ZipEntryBuilder::new("dep-0.1.0/build_backend.py".into(), Compression::Stored);
        block_on(zip.write_entry_whole(
            entry,
            b"def get_requires_for_build_wheel(config_settings=None):\n    return []\n",
        ))?;
        fs_err::write(source_dist.path(), block_on(zip.close())?)?;
        Ok(())
    };
    write_source_dist("helper-a")?;

    context.temp_dir.child("script.py").write_str(
        r#"
# /// script
# requires-python = ">=3.12"
# dependencies = ["parent==0.1.0"]
# ///
"#,
    )?;

    context
        .lock()
        .arg("--script")
        .arg("script.py")
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--no-cache")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();

    let lock = context.read("script.py.lock");
    assert!(lock.contains("revision = 4"), "{lock}");
    assert!(lock.contains("name = \"helper-a\""), "{lock}");
    assert!(
        lock.contains(r#"build-requires = [{ name = "helper-a", specifier = "==0.1.0" }]"#),
        "{lock}"
    );

    write_source_dist("helper-b")?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--script")
        .arg("script.py")
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--no-cache")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--locked"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided.

    hint: To update the lockfile, run `uv lock`.
    ");

    Ok(())
}

/// Verify that changing a foreign-only registry source archive used by a PEP 723 script
/// invalidates its captured build environment even when the root is inactive on the executor.
#[test]
fn script_foreign_registry_source_build_dependencies_invalidate() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    write_wheel(
        &links_dir.child("helper_a-0.1.0-py3-none-any.whl"),
        "helper-a",
        "0.1.0",
        &[],
    )?;
    write_wheel(
        &links_dir.child("helper_b-0.1.0-py3-none-any.whl"),
        "helper-b",
        "0.1.0",
        &[],
    )?;

    let source_dist = links_dir.child("dep-0.1.0.zip");
    let write_source_dist = |helper: &str| -> Result<()> {
        let mut zip = ZipFileWriter::new(Vec::new());
        let entry = ZipEntryBuilder::new("dep-0.1.0/pyproject.toml".into(), Compression::Stored);
        let pyproject_toml = format!(
            r#"
            [project]
            name = "dep"
            version = "0.1.0"
            requires-python = ">=3.12"

            [build-system]
            requires = ["{helper}==0.1.0"]
            backend-path = ["."]
            build-backend = "build_backend"
            "#
        );
        block_on(zip.write_entry_whole(entry, pyproject_toml.as_bytes()))?;
        let entry = ZipEntryBuilder::new("dep-0.1.0/build_backend.py".into(), Compression::Stored);
        block_on(zip.write_entry_whole(
            entry,
            b"def get_requires_for_build_wheel(config_settings=None):\n    return []\n",
        ))?;
        fs_err::write(source_dist.path(), block_on(zip.close())?)?;
        Ok(())
    };
    write_source_dist("helper-a")?;

    let foreign_marker = if cfg!(target_os = "windows") {
        "sys_platform != 'win32'"
    } else {
        "sys_platform == 'win32'"
    };
    context.temp_dir.child("script.py").write_str(&format!(
        r#"
# /// script
# requires-python = ">=3.12"
# dependencies = ["dep==0.1.0 ; {foreign_marker}"]
# ///
"#
    ))?;

    context
        .lock()
        .arg("--script")
        .arg("script.py")
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--no-cache")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();

    let lock = context.read("script.py.lock");
    assert!(lock.contains("revision = 4"), "{lock}");
    assert!(lock.contains("name = \"helper-a\""), "{lock}");
    assert!(
        lock.contains(r#"build-requires = [{ name = "helper-a", specifier = "==0.1.0" }]"#),
        "{lock}"
    );

    write_source_dist("helper-b")?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--script")
        .arg("script.py")
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--no-cache")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--locked"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided.

    hint: To update the lockfile, run `uv lock`.
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

/// Verify pylock.toml export rejects a build-enabled lock that would lose per-artifact Python
/// compatibility information.
#[test]
fn lock_build_dependencies_rejects_lossy_pylock_requires_python_export() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let simple = context.temp_dir.child("simple/helper");
    simple.create_dir_all()?;
    let files = context.temp_dir.child("files");
    files.create_dir_all()?;
    let older = "helper-1.0.0-1-py3-none-any.whl";
    let newer = "helper-1.0.0-2-py3-none-any.whl";
    write_wheel(&files.child(older), "helper", "1.0.0", &[])?;
    write_wheel(&files.child(newer), "helper", "1.0.0", &[])?;
    simple.child("index.html").write_str(&format!(
        r#"
        <a href="../../files/{older}" data-upload-time="2024-03-01T00:00:00Z" data-requires-python=">=3.12,<3.14">{older}</a>
        <a href="../../files/{newer}" data-upload-time="2024-03-01T00:00:00Z" data-requires-python=">=3.12,<3.14">{newer}</a>
        "#
    ))?;
    let index =
        Url::from_directory_path(context.temp_dir.child("simple").path()).expect("valid index URL");

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        version = "0.1.0"
        requires-python = ">=3.12,<3.14"

        [build-system]
        requires = []
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
        requires-python = ">=3.12,<3.14"
        dependencies = ["dep", "helper==1.0.0"]

        [tool.uv.sources]
        dep = { path = "dep" }
        "#,
    )?;

    context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index")
        .arg(index.as_str())
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();

    let lock = context.read("uv.lock");
    let lock = lock
        .lines()
        .map(|line| {
            if line.contains(older) {
                line.replace(">=3.12, <3.14", ">=3.12, <3.13")
            } else if line.contains(newer) {
                line.replace(">=3.12, <3.14", ">=3.13, <3.14")
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    context.temp_dir.child("uv.lock").write_str(&lock)?;
    assert!(lock.contains("revision = 4"), "{lock}");
    assert!(lock.contains(older), "{lock}");
    assert!(lock.contains(newer), "{lock}");
    assert!(lock.contains(">=3.12, <3.13"), "{lock}");
    assert!(lock.contains(">=3.13, <3.14"), "{lock}");

    uv_snapshot!(context.filters(), context
        .export()
        .arg("--frozen")
        .arg("--format")
        .arg("pylock.toml")
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Package `helper` has artifacts with different `requires-python` values, which uv cannot yet export to pylock.toml
    ");

    Ok(())
}

/// Verify pylock.toml export preserves per-artifact Python compatibility when all artifacts of a
/// package agree.
#[test]
fn lock_build_dependencies_preserves_uniform_pylock_requires_python_export() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let simple = context.temp_dir.child("simple/helper");
    simple.create_dir_all()?;
    let files = context.temp_dir.child("files");
    files.create_dir_all()?;
    let first = "helper-1.0.0-1-py3-none-any.whl";
    let second = "helper-1.0.0-2-py3-none-any.whl";
    write_wheel(&files.child(first), "helper", "1.0.0", &[])?;
    write_wheel(&files.child(second), "helper", "1.0.0", &[])?;
    simple.child("index.html").write_str(&format!(
        r#"
        <a href="../../files/{first}" data-requires-python=">=3.12,<3.13">{first}</a>
        <a href="../../files/{second}" data-requires-python=">=3.12,<3.13">{second}</a>
        "#
    ))?;
    let index =
        Url::from_directory_path(context.temp_dir.child("simple").path()).expect("valid index URL");

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        version = "0.1.0"
        requires-python = ">=3.12,<3.13"

        [build-system]
        requires = []
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
        requires-python = ">=3.12,<3.13"
        dependencies = ["dep", "helper==1.0.0"]

        [tool.uv.sources]
        dep = { path = "dep" }
        "#,
    )?;

    context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index")
        .arg(index.as_str())
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();

    let lock = context.read("uv.lock");
    assert!(lock.contains("revision = 4"), "{lock}");
    assert!(lock.contains(first), "{lock}");
    assert!(lock.contains(second), "{lock}");
    assert!(lock.contains(">=3.12, <3.13"), "{lock}");

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
    requires-python = "==3.12.*"

    [[packages]]
    name = "dep"
    directory = { path = "dep", editable = false }

    [[packages]]
    name = "helper"
    version = "1.0.0"
    requires-python = ">=3.12, <3.13"
    index = "file://[TEMP_DIR]/simple"
    wheels = [
        { url = "file://[TEMP_DIR]/files/helper-1.0.0-1-py3-none-any.whl", hashes = {} },
        { url = "file://[TEMP_DIR]/files/helper-1.0.0-2-py3-none-any.whl", hashes = {} },
    ]

    ----- stderr -----
    "#);

    Ok(())
}
