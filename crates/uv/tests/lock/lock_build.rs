use std::fmt::Write;
#[cfg(feature = "test-git")]
use std::process::Command;

use anyhow::Result;
use assert_cmd::assert::OutputAssertExt;
use assert_fs::fixture::ChildPath;
use assert_fs::prelude::*;
use async_zip::base::write::ZipFileWriter;
use async_zip::{Compression, ZipEntryBuilder};
use futures::executor::block_on;
use insta::assert_snapshot;
use serde_json::json;
use url::Url;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use uv_test::uv_snapshot;

fn package_section<'a>(lock: &'a str, name: &str) -> &'a str {
    let needle = format!("[[package]]\nname = \"{name}\"");
    let start = lock.find(&needle).expect("package section to exist");
    let rest = &lock[start..];
    let end = rest[1..]
        .find("\n[[package]]")
        .map(|offset| offset + 1)
        .unwrap_or(rest.len());
    &rest[..end]
}

fn resolution_sections(lock: &str) -> String {
    let start = lock
        .find("[[resolution]]")
        .expect("resolution section to exist");
    let rest = &lock[start..];
    let end = rest.find("\n[[package]]").unwrap_or(rest.len());
    rest[..end].trim_end().to_string()
}

fn write_wheel(path: &ChildPath, name: &str, version: &str) -> Result<()> {
    write_wheel_with_requires(path, name, version, &[])
}

fn write_wheel_with_requires(
    path: &ChildPath,
    name: &str,
    version: &str,
    requires_dist: &[&str],
) -> Result<()> {
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

/// Lock a project with a dependency that requires building from source
/// (due to dynamic metadata), and verify that build dependencies are captured
/// in the lock file.
#[test]
fn lock_build_dependencies() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    // Create a local dependency that uses setuptools with dynamic version,
    // forcing a build to extract metadata.
    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        dynamic = ["version"]
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.setuptools.dynamic]
        version = {attr = "dep.__version__"}
        "#,
    )?;
    dep_dir.child("dep").create_dir_all()?;
    dep_dir
        .child("dep/__init__.py")
        .write_str("__version__ = '0.1.0'")?;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
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

    uv_snapshot!(context.filters(), context.lock().arg("--preview-features").arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 2
        revision = 4
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[resolution]]
        id = "build:dep:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "dep"
        roots = [
            { name = "setuptools", version = "69.2.0" },
        ]

        [[resolution]]
        id = "build:dep:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "dep"
        roots = [
            { name = "setuptools", version = "69.2.0" },
            { name = "wheel", version = "0.43.0" },
        ]

        [[resolution]]
        id = "build:flit-core:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "flit-core"

        [[resolution]]
        id = "build:flit-core:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "flit-core"

        [[resolution]]
        id = "build:setuptools:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "setuptools"

        [[resolution]]
        id = "build:setuptools:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "setuptools"
        roots = [
            { name = "wheel", version = "0.43.0" },
        ]

        [[resolution]]
        id = "build:wheel:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "wheel"
        roots = [
            { name = "flit-core", version = "3.9.0" },
        ]

        [[resolution]]
        id = "build:wheel:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "wheel"
        roots = [
            { name = "flit-core", version = "3.9.0" },
        ]

        [[package]]
        name = "dep"
        source = { directory = "dep" }
        build-dependencies = [
            { name = "setuptools", version = "69.2.0" },
            { name = "wheel", version = "0.43.0" },
        ]

        [package.metadata]
        build-system = { build-backend = "setuptools.build_meta" }
        build-requires = [{ name = "setuptools", specifier = ">=42" }]

        [[package]]
        name = "flit-core"
        version = "3.9.0"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        build-dependencies = []
        sdist = { url = "https://files.pythonhosted.org/packages/c4/e6/c1ac50fe3eebb38a155155711e6e864e254ce4b6e17fe2429b4c4d5b9e80/flit_core-3.9.0.tar.gz", hash = "sha256:72ad266176c4a3fcfab5f2930d76896059851240570ce9a98733b658cb786eba", size = 41917, upload-time = "2023-05-14T14:48:51.809Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/38/45/618e84e49a6c51e5dd15565ec2fcd82ab273434f236b8f108f065ded517a/flit_core-3.9.0-py3-none-any.whl", hash = "sha256:7aada352fb0c7f5538c4fafeddf314d3a6a92ee8e2b1de70482329e42de70301", size = 63141, upload-time = "2023-05-14T14:48:49.24Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "dep" },
        ]

        [package.metadata]
        requires-dist = [{ name = "dep", directory = "dep" }]

        [[package]]
        name = "setuptools"
        version = "69.2.0"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        build-dependencies = [
            { name = "wheel", version = "0.43.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/4d/5b/dc575711b6b8f2f866131a40d053e30e962e633b332acf7cd2c24843d83d/setuptools-69.2.0.tar.gz", hash = "sha256:0ff4183f8f42cd8fa3acea16c45205521a4ef28f73c6391d8a25e92893134f2e", size = 2222950, upload-time = "2024-03-13T11:20:59.219Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/92/e1/1c8bb3420105e70bdf357d57dd5567202b4ef8d27f810e98bb962d950834/setuptools-69.2.0-py3-none-any.whl", hash = "sha256:c21c49fb1042386df081cb5d86759792ab89efca84cf114889191cd09aacc80c", size = 821485, upload-time = "2024-03-13T11:20:54.103Z" },
        ]

        [[package]]
        name = "wheel"
        version = "0.43.0"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        build-dependencies = [
            { name = "flit-core", version = "3.9.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/b8/d6/ac9cd92ea2ad502ff7c1ab683806a9deb34711a1e2bd8a59814e8fc27e69/wheel-0.43.0.tar.gz", hash = "sha256:465ef92c69fa5c5da2d1cf8ac40559a8c940886afcef87dcf14b9470862f1d85", size = 99109, upload-time = "2024-03-11T19:29:17.32Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/7d/cd/d7460c9a869b16c3dd4e1e403cce337df165368c71d6af229a74699622ce/wheel-0.43.0-py3-none-any.whl", hash = "sha256:55c570405f142630c6b9f72fe09d9b67cf1477fcf543ae5b8dcb1f5b7377da81", size = 65775, upload-time = "2024-03-11T19:29:15.522Z" },
        ]
        "#
        );
    });

    // Re-run with `--locked` to ensure the lock file with build-dependencies is valid.
    uv_snapshot!(context.filters(), context.lock().arg("--preview-features").arg("lock-build-dependencies").arg("--locked"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    Ok(())
}

/// Verify that dangling build dependency references invalidate a lockfile.
#[test]
fn lock_build_dependencies_rejects_dangling_build_dependency() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

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
        .assert()
        .success();

    let lock = context.read("uv.lock").replacen(
        r#"{ name = "setuptools", version = "69.2.0" }"#,
        r#"{ name = "setuptools", version = "69.2.0", source = { registry = "https://pypi.org/simple" } }"#,
        1,
    );
    let missing_package = package_section(&lock, "setuptools");
    context
        .temp_dir
        .child("uv.lock")
        .write_str(&lock.replacen(missing_package, "", 1))?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--locked"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to parse `uv.lock`
      Caused by: Dependency `setuptools` has missing `source` field but has more than one matching package
    ");

    Ok(())
}

/// Verify that a selectable workspace member's build lock is not restricted
/// to the marker of an incoming dependency edge.
#[test]
fn lock_build_dependencies_workspace_root_widens_marker_reachability() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let (conditional_marker, target_platform) = if cfg!(target_os = "windows") {
        ("sys_platform == 'win32'", "linux")
    } else {
        ("sys_platform == 'linux'", "windows")
    };

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    write_wheel(
        &links_dir.child("seed-0.1.0-py3-none-any.whl"),
        "seed",
        "0.1.0",
    )?;

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [tool.uv.workspace]
        members = ["a", "b"]
        "#,
    )?;

    let member_a = context.temp_dir.child("a");
    member_a.create_dir_all()?;
    member_a.child("pyproject.toml").write_str(&format!(
        r#"
        [project]
        name = "a"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["b ; {conditional_marker}"]

        [tool.uv.sources]
        b = {{ workspace = true }}
        "#
    ))?;

    let member_b = context.temp_dir.child("b");
    member_b.create_dir_all()?;
    member_b.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "b"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["seed==0.1.0"]
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
    )?;
    member_b.child("build_backend.py").write_str(
        r#"
from pathlib import Path
from zipfile import ZipFile

def get_requires_for_build_editable(config_settings=None):
    return []

def get_requires_for_build_wheel(config_settings=None):
    return []

def build_editable(wheel_directory, config_settings=None, metadata_directory=None):
    filename = "b-0.1.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("b/__init__.py", "")
        wheel.writestr(
            "b-0.1.0.dist-info/METADATA",
            "Metadata-Version: 2.3\nName: b\nVersion: 0.1.0\n",
        )
        wheel.writestr(
            "b-0.1.0.dist-info/WHEEL",
            "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
        )
        wheel.writestr("b-0.1.0.dist-info/RECORD", "")
    return filename

build_wheel = build_editable
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
    let resolutions = resolution_sections(&lock);
    let member_b_resolutions = resolutions
        .split("[[resolution]]")
        .filter(|resolution| resolution.contains("\nname = \"b\"\n"))
        .collect::<Vec<_>>();
    assert_eq!(member_b_resolutions.len(), 4, "{resolutions}");
    assert!(
        member_b_resolutions
            .iter()
            .all(|resolution| !resolution.contains("\ntarget = ")),
        "{resolutions}"
    );
    context
        .sync()
        .arg("--package")
        .arg("b")
        .arg("--python-platform")
        .arg(target_platform)
        .arg("--no-index")
        .arg("--frozen")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();

    Ok(())
}

/// Verify build locks retain every `python_full_version` branch for replay.
#[test]
fn lock_build_dependencies_preserve_python_full_version_branches() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    write_wheel(
        &links_dir.child("seed-0.1.0-py3-none-any.whl"),
        "seed",
        "0.1.0",
    )?;
    write_wheel(
        &links_dir.child("seed-0.2.0-py3-none-any.whl"),
        "seed",
        "0.2.0",
    )?;

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        version = "0.1.0"
        requires-python = ">=3.12,<3.13"

        [build-system]
        requires = [
            "seed==0.1.0 ; python_full_version < '3.12.5'",
            "seed==0.2.0 ; python_full_version >= '3.12.5'",
        ]
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
    )?;
    dep_dir.child("build_backend.py").write_str(
        r#"
from pathlib import Path
from zipfile import ZipFile

def get_requires_for_build_wheel(config_settings=None):
    return []

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    import seed
    filename = "dep-0.1.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("dep/__init__.py", "")
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

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12,<3.13"
        dependencies = ["dep"]

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
    let dep = package_section(&lock, "dep");
    assert!(
        dep.contains(r#"version = "0.1.0""#)
            && dep.contains(r#"marker = "python_full_version < '3.12.5'""#),
        "{dep}"
    );
    assert!(
        dep.contains(r#"version = "0.2.0""#)
            && dep.contains(r#"marker = "python_full_version >= '3.12.5'""#),
        "{dep}"
    );

    context
        .sync()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--frozen")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();

    Ok(())
}

/// Verify implementation markers are replayed without executor-specific records.
#[test]
fn lock_build_dependencies_preserve_implementation_branches_without_executor_records() -> Result<()>
{
    let context = uv_test::test_context!("3.12");

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    write_wheel(
        &links_dir.child("seed-0.1.0-py3-none-any.whl"),
        "seed",
        "0.1.0",
    )?;
    write_wheel(
        &links_dir.child("seed-0.2.0-py3-none-any.whl"),
        "seed",
        "0.2.0",
    )?;

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        version = "0.1.0"
        requires-python = ">=3.12,<3.13"

        [build-system]
        requires = [
            "seed==0.1.0 ; implementation_name == 'cpython'",
            "seed==0.2.0 ; implementation_name != 'cpython'",
        ]
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
    )?;
    dep_dir.child("build_backend.py").write_str(
        r#"
from pathlib import Path
from zipfile import ZipFile

def get_requires_for_build_wheel(config_settings=None):
    return []

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    import seed
    filename = "dep-0.1.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("dep/__init__.py", "")
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

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12,<3.13"
        dependencies = ["dep"]

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
    let dep = package_section(&lock, "dep");
    assert!(
        dep.contains(r#"version = "0.1.0""#)
            && dep.contains(r#"marker = "implementation_name == 'cpython'""#),
        "{dep}"
    );
    assert!(
        dep.contains(r#"version = "0.2.0""#)
            && dep.contains(r#"marker = "implementation_name != 'cpython'""#),
        "{dep}"
    );
    assert!(!lock.contains("\nexecutor = "), "{lock}");

    context
        .sync()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--frozen")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();

    Ok(())
}

/// Verify target environments do not prune executor markers and build artifacts
/// remain compatible with the executor.
#[test]
fn lock_build_dependencies_resolve_markers_against_executor() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let (executor_marker, target_marker, target_platform, target_wheel_tag) =
        if cfg!(target_os = "windows") {
            (
                "sys_platform == 'win32'",
                "sys_platform == 'linux'",
                "linux",
                "manylinux_2_17_x86_64",
            )
        } else if cfg!(target_os = "macos") {
            (
                "sys_platform == 'darwin'",
                "sys_platform == 'win32'",
                "windows",
                "win_amd64",
            )
        } else {
            (
                "sys_platform == 'linux'",
                "sys_platform == 'win32'",
                "windows",
                "win_amd64",
            )
        };

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    write_wheel(
        &links_dir.child("host_builder-1.0.0-py3-none-any.whl"),
        "host-builder",
        "1.0.0",
    )?;
    write_wheel(
        &links_dir.child("artifact_builder-1.0.0-py3-none-any.whl"),
        "artifact-builder",
        "1.0.0",
    )?;
    write_wheel(
        &links_dir.child(format!(
            "artifact_builder-2.0.0-py3-none-{target_wheel_tag}.whl"
        )),
        "artifact-builder",
        "2.0.0",
    )?;
    write_wheel(
        &links_dir.child("nested_seed-1.0.0-py3-none-any.whl"),
        "nested-seed",
        "1.0.0",
    )?;

    let nested_builder_dir = context.temp_dir.child("nested-builder");
    nested_builder_dir.create_dir_all()?;
    nested_builder_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "nested-builder"
        version = "1.0.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["nested-seed==1.0.0"]
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
    )?;
    nested_builder_dir.child("build_backend.py").write_str(
        r#"
from pathlib import Path
from zipfile import ZipFile

def get_requires_for_build_wheel(config_settings=None):
    return []

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    import nested_seed
    filename = "nested_builder-1.0.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("nested_builder.py", "")
        wheel.writestr(
            "nested_builder-1.0.0.dist-info/METADATA",
            "Metadata-Version: 2.3\nName: nested-builder\nVersion: 1.0.0\n",
        )
        wheel.writestr(
            "nested_builder-1.0.0.dist-info/WHEEL",
            "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
        )
        wheel.writestr("nested_builder-1.0.0.dist-info/RECORD", "")
    return filename
"#,
    )?;
    let nested_builder_url = Url::from_directory_path(nested_builder_dir.path()).unwrap();

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(&format!(
        r#"
        [project]
        name = "dep"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = [
            "artifact-builder",
            "host-builder ; {executor_marker}",
            "nested-builder @ {nested_builder_url}",
        ]
        backend-path = ["."]
        build-backend = "build_backend"
        "#
    ))?;
    dep_dir.child("build_backend.py").write_str(
        r#"
from pathlib import Path
from zipfile import ZipFile

def get_requires_for_build_wheel(config_settings=None):
    return []

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    import artifact_builder
    import host_builder
    import nested_builder
    filename = "dep-0.1.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("dep/__init__.py", "")
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

    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(&format!(
            r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep"]

        [tool.uv]
        environments = ["{target_marker}"]

        [tool.uv.sources]
        dep = {{ path = "dep" }}
        "#
        ))?;

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
    let dep = package_section(&lock, "dep");
    assert!(
        dep.contains(r#"{ name = "host-builder", version = "1.0.0""#),
        "{dep}"
    );
    assert!(
        dep.contains(r#"{ name = "artifact-builder", version = "1.0.0""#),
        "{dep}"
    );
    assert!(
        !dep.contains(r#"{ name = "artifact-builder", version = "2.0.0" }"#),
        "{dep}"
    );
    let resolutions = resolution_sections(&lock);
    let nested_builder_resolutions = resolutions
        .split("[[resolution]]")
        .filter(|resolution| resolution.contains("\nname = \"nested-builder\"\n"))
        .collect::<Vec<_>>();
    assert_eq!(nested_builder_resolutions.len(), 2, "{resolutions}");
    assert!(
        nested_builder_resolutions
            .iter()
            .all(|resolution| resolution
                .contains(&format!("target = {{ marker = \"{target_marker}\" }}"))),
        "{resolutions}"
    );

    context
        .sync()
        .arg("--python-platform")
        .arg(target_platform)
        .arg("--no-index")
        .arg("--frozen")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();

    Ok(())
}

/// Verify that editable workspace packages capture separate build requirements
/// for editable and regular wheel builds.
#[test]
fn lock_build_dependencies_distinguish_editable_and_wheel_builds() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    write_wheel(
        &links_dir.child("editable_helper-1.0.0-py3-none-any.whl"),
        "editable-helper",
        "1.0.0",
    )?;
    write_wheel(
        &links_dir.child("wheel_helper-1.0.0-py3-none-any.whl"),
        "wheel-helper",
        "1.0.0",
    )?;

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = []
        backend-path = ["."]
        build-backend = "build_backend"

        "#,
    )?;
    context.temp_dir.child("build_backend.py").write_str(
        r#"
from pathlib import Path
from zipfile import ZipFile

def get_requires_for_build_editable(config_settings=None):
    return ["editable-helper==1.0.0"]

def get_requires_for_build_wheel(config_settings=None):
    return ["wheel-helper==1.0.0"]

def build_editable(wheel_directory, config_settings=None, metadata_directory=None):
    import editable_helper
    return _build(wheel_directory)

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    import wheel_helper
    return _build(wheel_directory)

def _build(wheel_directory):
    filename = "project-0.1.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("project/__init__.py", "")
        wheel.writestr(
            "project-0.1.0.dist-info/METADATA",
            "Metadata-Version: 2.3\nName: project\nVersion: 0.1.0\n",
        )
        wheel.writestr(
            "project-0.1.0.dist-info/WHEEL",
            "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
        )
        wheel.writestr("project-0.1.0.dist-info/RECORD", "")
    return filename
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
    let resolutions = resolution_sections(&lock);
    let project_resolutions = resolutions
        .split("[[resolution]]")
        .filter(|resolution| resolution.contains("\nname = \"project\"\n"))
        .collect::<Vec<_>>();
    assert!(
        project_resolutions.iter().any(|resolution| {
            resolution.contains("operation = \"editable\"")
                && resolution.contains(r#"{ name = "editable-helper""#)
        }),
        "{resolutions}"
    );
    assert!(
        project_resolutions.iter().any(|resolution| {
            resolution.contains("operation = \"wheel\"")
                && resolution.contains(r#"{ name = "wheel-helper""#)
        }),
        "{resolutions}"
    );

    fs_err::remove_dir_all(&context.cache_dir)?;
    context
        .sync()
        .arg("--no-editable")
        .arg("--no-index")
        .arg("--frozen")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();

    Ok(())
}

/// Verify that stored build dependencies are used as preferences during
/// subsequent resolves, producing the same versions.
#[test]
fn lock_build_dependencies_preference() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    // Create a local dependency with dynamic version requiring setuptools.
    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        dynamic = ["version"]
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.setuptools.dynamic]
        version = {attr = "dep.__version__"}
        "#,
    )?;
    dep_dir.child("dep").create_dir_all()?;
    dep_dir
        .child("dep/__init__.py")
        .write_str("__version__ = '0.1.0'")?;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
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

    // First lock to capture build dependencies.
    uv_snapshot!(context.filters(), context.lock().arg("--preview-features").arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock_initial = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock_initial, @r#"
        version = 2
        revision = 4
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[resolution]]
        id = "build:dep:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "dep"
        roots = [
            { name = "setuptools", version = "69.2.0" },
        ]

        [[resolution]]
        id = "build:dep:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "dep"
        roots = [
            { name = "setuptools", version = "69.2.0" },
            { name = "wheel", version = "0.43.0" },
        ]

        [[resolution]]
        id = "build:flit-core:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "flit-core"

        [[resolution]]
        id = "build:flit-core:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "flit-core"

        [[resolution]]
        id = "build:setuptools:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "setuptools"

        [[resolution]]
        id = "build:setuptools:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "setuptools"
        roots = [
            { name = "wheel", version = "0.43.0" },
        ]

        [[resolution]]
        id = "build:wheel:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "wheel"
        roots = [
            { name = "flit-core", version = "3.9.0" },
        ]

        [[resolution]]
        id = "build:wheel:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "wheel"
        roots = [
            { name = "flit-core", version = "3.9.0" },
        ]

        [[package]]
        name = "dep"
        source = { directory = "dep" }
        build-dependencies = [
            { name = "setuptools", version = "69.2.0" },
            { name = "wheel", version = "0.43.0" },
        ]

        [package.metadata]
        build-system = { build-backend = "setuptools.build_meta" }
        build-requires = [{ name = "setuptools", specifier = ">=42" }]

        [[package]]
        name = "flit-core"
        version = "3.9.0"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        build-dependencies = []
        sdist = { url = "https://files.pythonhosted.org/packages/c4/e6/c1ac50fe3eebb38a155155711e6e864e254ce4b6e17fe2429b4c4d5b9e80/flit_core-3.9.0.tar.gz", hash = "sha256:72ad266176c4a3fcfab5f2930d76896059851240570ce9a98733b658cb786eba", size = 41917, upload-time = "2023-05-14T14:48:51.809Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/38/45/618e84e49a6c51e5dd15565ec2fcd82ab273434f236b8f108f065ded517a/flit_core-3.9.0-py3-none-any.whl", hash = "sha256:7aada352fb0c7f5538c4fafeddf314d3a6a92ee8e2b1de70482329e42de70301", size = 63141, upload-time = "2023-05-14T14:48:49.24Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "dep" },
        ]

        [package.metadata]
        requires-dist = [{ name = "dep", directory = "dep" }]

        [[package]]
        name = "setuptools"
        version = "69.2.0"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        build-dependencies = [
            { name = "wheel", version = "0.43.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/4d/5b/dc575711b6b8f2f866131a40d053e30e962e633b332acf7cd2c24843d83d/setuptools-69.2.0.tar.gz", hash = "sha256:0ff4183f8f42cd8fa3acea16c45205521a4ef28f73c6391d8a25e92893134f2e", size = 2222950, upload-time = "2024-03-13T11:20:59.219Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/92/e1/1c8bb3420105e70bdf357d57dd5567202b4ef8d27f810e98bb962d950834/setuptools-69.2.0-py3-none-any.whl", hash = "sha256:c21c49fb1042386df081cb5d86759792ab89efca84cf114889191cd09aacc80c", size = 821485, upload-time = "2024-03-13T11:20:54.103Z" },
        ]

        [[package]]
        name = "wheel"
        version = "0.43.0"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        build-dependencies = [
            { name = "flit-core", version = "3.9.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/b8/d6/ac9cd92ea2ad502ff7c1ab683806a9deb34711a1e2bd8a59814e8fc27e69/wheel-0.43.0.tar.gz", hash = "sha256:465ef92c69fa5c5da2d1cf8ac40559a8c940886afcef87dcf14b9470862f1d85", size = 99109, upload-time = "2024-03-11T19:29:17.32Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/7d/cd/d7460c9a869b16c3dd4e1e403cce337df165368c71d6af229a74699622ce/wheel-0.43.0-py3-none-any.whl", hash = "sha256:55c570405f142630c6b9f72fe09d9b67cf1477fcf543ae5b8dcb1f5b7377da81", size = 65775, upload-time = "2024-03-11T19:29:15.522Z" },
        ]
        "#
        );
    });

    // Re-lock without `--locked` to verify build dependencies are preserved.
    uv_snapshot!(context.filters(), context.lock().arg("--preview-features").arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock_second = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock_second, @r#"
        version = 2
        revision = 4
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[resolution]]
        id = "build:dep:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "dep"
        roots = [
            { name = "setuptools", version = "69.2.0" },
        ]

        [[resolution]]
        id = "build:dep:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "dep"
        roots = [
            { name = "setuptools", version = "69.2.0" },
            { name = "wheel", version = "0.43.0" },
        ]

        [[resolution]]
        id = "build:flit-core:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "flit-core"

        [[resolution]]
        id = "build:flit-core:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "flit-core"

        [[resolution]]
        id = "build:setuptools:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "setuptools"

        [[resolution]]
        id = "build:setuptools:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "setuptools"
        roots = [
            { name = "wheel", version = "0.43.0" },
        ]

        [[resolution]]
        id = "build:wheel:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "wheel"
        roots = [
            { name = "flit-core", version = "3.9.0" },
        ]

        [[resolution]]
        id = "build:wheel:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "wheel"
        roots = [
            { name = "flit-core", version = "3.9.0" },
        ]

        [[package]]
        name = "dep"
        source = { directory = "dep" }
        build-dependencies = [
            { name = "setuptools", version = "69.2.0" },
            { name = "wheel", version = "0.43.0" },
        ]

        [package.metadata]
        build-system = { build-backend = "setuptools.build_meta" }
        build-requires = [{ name = "setuptools", specifier = ">=42" }]

        [[package]]
        name = "flit-core"
        version = "3.9.0"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        build-dependencies = []
        sdist = { url = "https://files.pythonhosted.org/packages/c4/e6/c1ac50fe3eebb38a155155711e6e864e254ce4b6e17fe2429b4c4d5b9e80/flit_core-3.9.0.tar.gz", hash = "sha256:72ad266176c4a3fcfab5f2930d76896059851240570ce9a98733b658cb786eba", size = 41917, upload-time = "2023-05-14T14:48:51.809Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/38/45/618e84e49a6c51e5dd15565ec2fcd82ab273434f236b8f108f065ded517a/flit_core-3.9.0-py3-none-any.whl", hash = "sha256:7aada352fb0c7f5538c4fafeddf314d3a6a92ee8e2b1de70482329e42de70301", size = 63141, upload-time = "2023-05-14T14:48:49.24Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "dep" },
        ]

        [package.metadata]
        requires-dist = [{ name = "dep", directory = "dep" }]

        [[package]]
        name = "setuptools"
        version = "69.2.0"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        build-dependencies = [
            { name = "wheel", version = "0.43.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/4d/5b/dc575711b6b8f2f866131a40d053e30e962e633b332acf7cd2c24843d83d/setuptools-69.2.0.tar.gz", hash = "sha256:0ff4183f8f42cd8fa3acea16c45205521a4ef28f73c6391d8a25e92893134f2e", size = 2222950, upload-time = "2024-03-13T11:20:59.219Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/92/e1/1c8bb3420105e70bdf357d57dd5567202b4ef8d27f810e98bb962d950834/setuptools-69.2.0-py3-none-any.whl", hash = "sha256:c21c49fb1042386df081cb5d86759792ab89efca84cf114889191cd09aacc80c", size = 821485, upload-time = "2024-03-13T11:20:54.103Z" },
        ]

        [[package]]
        name = "wheel"
        version = "0.43.0"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        build-dependencies = [
            { name = "flit-core", version = "3.9.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/b8/d6/ac9cd92ea2ad502ff7c1ab683806a9deb34711a1e2bd8a59814e8fc27e69/wheel-0.43.0.tar.gz", hash = "sha256:465ef92c69fa5c5da2d1cf8ac40559a8c940886afcef87dcf14b9470862f1d85", size = 99109, upload-time = "2024-03-11T19:29:17.32Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/7d/cd/d7460c9a869b16c3dd4e1e403cce337df165368c71d6af229a74699622ce/wheel-0.43.0-py3-none-any.whl", hash = "sha256:55c570405f142630c6b9f72fe09d9b67cf1477fcf543ae5b8dcb1f5b7377da81", size = 65775, upload-time = "2024-03-11T19:29:15.522Z" },
        ]
        "#
        );
    });

    Ok(())
}

/// Verify that packages reached only through build dependencies do not become
/// runtime preferences, while packages shared with the runtime graph do.
#[test]
fn lock_build_dependencies_runtime_preferences_exclude_build_only_packages() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    write_wheel(
        &links_dir.child("build_only-1.0.0-py3-none-any.whl"),
        "build-only",
        "1.0.0",
    )?;
    write_wheel(
        &links_dir.child("shared-1.0.0-py3-none-any.whl"),
        "shared",
        "1.0.0",
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
        requires = ["build-only>=1", "shared>=1"]
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

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep", "shared>=1"]

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
    let build_only = package_section(&lock, "build-only");
    assert!(build_only.contains("build-only = true"), "{build_only}");

    write_wheel(
        &links_dir.child("build_only-2.0.0-py3-none-any.whl"),
        "build-only",
        "2.0.0",
    )?;
    write_wheel(
        &links_dir.child("shared-2.0.0-py3-none-any.whl"),
        "shared",
        "2.0.0",
    )?;
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["build-only>=1", "dep", "shared>=1"]

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
    let shared = package_section(&lock, "shared");
    assert!(shared.contains("version = \"1.0.0\""), "{shared}");
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(package_section(&lock, "project"), @r#"
        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "build-only", version = "2.0.0", source = { registry = "[TEMP_DIR]/links" } },
            { name = "dep" },
            { name = "shared" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "build-only", specifier = ">=1" },
            { name = "dep", directory = "dep" },
            { name = "shared", specifier = ">=1" },
        ]
        "#);
    });

    Ok(())
}

/// Verify that packages reached only through build dependencies do not become
/// runtime roots for project-less workspaces.
#[tokio::test]
async fn lock_build_dependencies_runtime_consumers_exclude_build_only_packages() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    write_wheel(
        &links_dir.child("shared-1.0.0-py3-none-any.whl"),
        "shared",
        "1.0.0",
    )?;
    write_wheel(
        &links_dir.child("shared-2.0.0-py3-none-any.whl"),
        "shared",
        "2.0.0",
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
        requires = ["shared<2"]
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
        [dependency-groups]
        dev = ["dep", "shared>=2"]

        [tool.uv.workspace]
        members = []

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
    let shared = package_section(&lock, "shared");
    assert!(shared.contains("version = \"1.0.0\""), "{shared}");
    assert!(shared.contains("build-only = true"), "{shared}");
    assert!(
        lock.contains("name = \"shared\"\nversion = \"2.0.0\""),
        "{lock}"
    );

    uv_snapshot!(context.filters(), context
        .sync()
        .arg("--frozen")
        .arg("--no-install-package")
        .arg("dep")
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + shared==2.0.0
    ");

    uv_snapshot!(context.filters(), context
        .tree()
        .arg("--frozen")
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    shared v2.0.0 (group: dev)
    dep v0.1.0 (group: dev)

    ----- stderr -----
    ");

    uv_snapshot!(context.filters(), context
        .export()
        .arg("--frozen")
        .arg("--no-hashes")
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    # This file was autogenerated by uv via the following command:
    #    uv export --cache-dir [CACHE_DIR] --frozen --no-hashes --preview-features lock-build-dependencies
    ./dep
    shared==2.0.0

    ----- stderr -----
    ");

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/querybatch"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "results": [{}, {}]
        })))
        .mount(&server)
        .await;

    uv_snapshot!(context.filters(), context
        .audit()
        .arg("--frozen")
        .arg("--preview-features")
        .arg("audit,lock-build-dependencies")
        .arg("--service-url")
        .arg(server.uri()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Found no known vulnerabilities and no adverse project statuses in 2 packages
    ");

    Ok(())
}

/// Verify that `uv add` derives lower bounds from runtime packages rather than
/// same-name packages reached only through build dependencies.
#[test]
fn lock_build_dependencies_add_lower_bound_excludes_build_only_packages() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    write_wheel(
        &links_dir.child("foo-1.0.0-py3-none-any.whl"),
        "foo",
        "1.0.0",
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
        requires = ["foo>=1"]
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
        r#"[project]
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
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();

    let lock = context.read("uv.lock");
    let foo = package_section(&lock, "foo");
    assert!(foo.contains("version = \"1.0.0\""), "{foo}");
    assert!(foo.contains("build-only = true"), "{foo}");

    write_wheel(
        &links_dir.child("foo-2.0.0-py3-none-any.whl"),
        "foo",
        "2.0.0",
    )?;

    context
        .add()
        .arg("foo")
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--no-sync")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();

    let lock = context.read("uv.lock");
    assert!(
        lock.contains("name = \"foo\"\nversion = \"2.0.0\""),
        "{lock}"
    );

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(context.read("pyproject.toml"), @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "dep",
            "foo>=2.0.0",
        ]

        [tool.uv.sources]
        dep = { path = "dep" }
        "#);
    });

    Ok(())
}

/// Lock a project with multiple local dependencies that each require building,
/// and verify each gets its own build-dependencies section.
#[test]
fn lock_build_dependencies_multiple_packages() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    // Create first local dependency.
    let dep_a_dir = context.temp_dir.child("dep-a");
    dep_a_dir.create_dir_all()?;
    dep_a_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep-a"
        dynamic = ["version"]
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.setuptools.dynamic]
        version = {attr = "dep_a.__version__"}
        "#,
    )?;
    dep_a_dir.child("dep_a").create_dir_all()?;
    dep_a_dir
        .child("dep_a/__init__.py")
        .write_str("__version__ = '0.1.0'")?;

    // Create second local dependency.
    let dep_b_dir = context.temp_dir.child("dep-b");
    dep_b_dir.create_dir_all()?;
    dep_b_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep-b"
        dynamic = ["version"]
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.setuptools.dynamic]
        version = {attr = "dep_b.__version__"}
        "#,
    )?;
    dep_b_dir.child("dep_b").create_dir_all()?;
    dep_b_dir
        .child("dep_b/__init__.py")
        .write_str("__version__ = '0.2.0'")?;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep-a", "dep-b"]

        [tool.uv.sources]
        dep-a = { path = "dep-a" }
        dep-b = { path = "dep-b" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock().arg("--preview-features").arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    ");

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 2
        revision = 4
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[resolution]]
        id = "build:dep-a:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "dep-a"
        roots = [
            { name = "setuptools", version = "69.2.0" },
        ]

        [[resolution]]
        id = "build:dep-a:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "dep-a"
        roots = [
            { name = "setuptools", version = "69.2.0" },
            { name = "wheel", version = "0.43.0" },
        ]

        [[resolution]]
        id = "build:dep-b:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "dep-b"
        roots = [
            { name = "setuptools", version = "69.2.0" },
        ]

        [[resolution]]
        id = "build:dep-b:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "dep-b"
        roots = [
            { name = "setuptools", version = "69.2.0" },
            { name = "wheel", version = "0.43.0" },
        ]

        [[resolution]]
        id = "build:flit-core:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "flit-core"

        [[resolution]]
        id = "build:flit-core:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "flit-core"

        [[resolution]]
        id = "build:setuptools:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "setuptools"

        [[resolution]]
        id = "build:setuptools:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "setuptools"
        roots = [
            { name = "wheel", version = "0.43.0" },
        ]

        [[resolution]]
        id = "build:wheel:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "wheel"
        roots = [
            { name = "flit-core", version = "3.9.0" },
        ]

        [[resolution]]
        id = "build:wheel:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "wheel"
        roots = [
            { name = "flit-core", version = "3.9.0" },
        ]

        [[package]]
        name = "dep-a"
        source = { directory = "dep-a" }
        build-dependencies = [
            { name = "setuptools", version = "69.2.0" },
            { name = "wheel", version = "0.43.0" },
        ]

        [package.metadata]
        build-system = { build-backend = "setuptools.build_meta" }
        build-requires = [{ name = "setuptools", specifier = ">=42" }]

        [[package]]
        name = "dep-b"
        source = { directory = "dep-b" }
        build-dependencies = [
            { name = "setuptools", version = "69.2.0" },
            { name = "wheel", version = "0.43.0" },
        ]

        [package.metadata]
        build-system = { build-backend = "setuptools.build_meta" }
        build-requires = [{ name = "setuptools", specifier = ">=42" }]

        [[package]]
        name = "flit-core"
        version = "3.9.0"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        build-dependencies = []
        sdist = { url = "https://files.pythonhosted.org/packages/c4/e6/c1ac50fe3eebb38a155155711e6e864e254ce4b6e17fe2429b4c4d5b9e80/flit_core-3.9.0.tar.gz", hash = "sha256:72ad266176c4a3fcfab5f2930d76896059851240570ce9a98733b658cb786eba", size = 41917, upload-time = "2023-05-14T14:48:51.809Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/38/45/618e84e49a6c51e5dd15565ec2fcd82ab273434f236b8f108f065ded517a/flit_core-3.9.0-py3-none-any.whl", hash = "sha256:7aada352fb0c7f5538c4fafeddf314d3a6a92ee8e2b1de70482329e42de70301", size = 63141, upload-time = "2023-05-14T14:48:49.24Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "dep-a" },
            { name = "dep-b" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "dep-a", directory = "dep-a" },
            { name = "dep-b", directory = "dep-b" },
        ]

        [[package]]
        name = "setuptools"
        version = "69.2.0"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        build-dependencies = [
            { name = "wheel", version = "0.43.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/4d/5b/dc575711b6b8f2f866131a40d053e30e962e633b332acf7cd2c24843d83d/setuptools-69.2.0.tar.gz", hash = "sha256:0ff4183f8f42cd8fa3acea16c45205521a4ef28f73c6391d8a25e92893134f2e", size = 2222950, upload-time = "2024-03-13T11:20:59.219Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/92/e1/1c8bb3420105e70bdf357d57dd5567202b4ef8d27f810e98bb962d950834/setuptools-69.2.0-py3-none-any.whl", hash = "sha256:c21c49fb1042386df081cb5d86759792ab89efca84cf114889191cd09aacc80c", size = 821485, upload-time = "2024-03-13T11:20:54.103Z" },
        ]

        [[package]]
        name = "wheel"
        version = "0.43.0"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        build-dependencies = [
            { name = "flit-core", version = "3.9.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/b8/d6/ac9cd92ea2ad502ff7c1ab683806a9deb34711a1e2bd8a59814e8fc27e69/wheel-0.43.0.tar.gz", hash = "sha256:465ef92c69fa5c5da2d1cf8ac40559a8c940886afcef87dcf14b9470862f1d85", size = 99109, upload-time = "2024-03-11T19:29:17.32Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/7d/cd/d7460c9a869b16c3dd4e1e403cce337df165368c71d6af229a74699622ce/wheel-0.43.0-py3-none-any.whl", hash = "sha256:55c570405f142630c6b9f72fe09d9b67cf1477fcf543ae5b8dcb1f5b7377da81", size = 65775, upload-time = "2024-03-11T19:29:15.522Z" },
        ]
        "#
        );
    });

    // Re-run with --locked to verify round-trip.
    uv_snapshot!(context.filters(), context.lock().arg("--preview-features").arg("lock-build-dependencies").arg("--locked"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    ");

    Ok(())
}

/// Verify that `--upgrade` clears build dependency preferences and re-resolves
/// build dependencies from scratch.
#[test]
fn lock_build_dependencies_upgrade() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        dynamic = ["version"]
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.setuptools.dynamic]
        version = {attr = "dep.__version__"}
        "#,
    )?;
    dep_dir.child("dep").create_dir_all()?;
    dep_dir
        .child("dep/__init__.py")
        .write_str("__version__ = '0.1.0'")?;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
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

    // Initial lock.
    uv_snapshot!(context.filters(), context.lock().arg("--preview-features").arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock_initial = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock_initial, @r#"
        version = 2
        revision = 4
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[resolution]]
        id = "build:dep:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "dep"
        roots = [
            { name = "setuptools", version = "69.2.0" },
        ]

        [[resolution]]
        id = "build:dep:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "dep"
        roots = [
            { name = "setuptools", version = "69.2.0" },
            { name = "wheel", version = "0.43.0" },
        ]

        [[resolution]]
        id = "build:flit-core:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "flit-core"

        [[resolution]]
        id = "build:flit-core:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "flit-core"

        [[resolution]]
        id = "build:setuptools:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "setuptools"

        [[resolution]]
        id = "build:setuptools:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "setuptools"
        roots = [
            { name = "wheel", version = "0.43.0" },
        ]

        [[resolution]]
        id = "build:wheel:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "wheel"
        roots = [
            { name = "flit-core", version = "3.9.0" },
        ]

        [[resolution]]
        id = "build:wheel:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "wheel"
        roots = [
            { name = "flit-core", version = "3.9.0" },
        ]

        [[package]]
        name = "dep"
        source = { directory = "dep" }
        build-dependencies = [
            { name = "setuptools", version = "69.2.0" },
            { name = "wheel", version = "0.43.0" },
        ]

        [package.metadata]
        build-system = { build-backend = "setuptools.build_meta" }
        build-requires = [{ name = "setuptools", specifier = ">=42" }]

        [[package]]
        name = "flit-core"
        version = "3.9.0"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        build-dependencies = []
        sdist = { url = "https://files.pythonhosted.org/packages/c4/e6/c1ac50fe3eebb38a155155711e6e864e254ce4b6e17fe2429b4c4d5b9e80/flit_core-3.9.0.tar.gz", hash = "sha256:72ad266176c4a3fcfab5f2930d76896059851240570ce9a98733b658cb786eba", size = 41917, upload-time = "2023-05-14T14:48:51.809Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/38/45/618e84e49a6c51e5dd15565ec2fcd82ab273434f236b8f108f065ded517a/flit_core-3.9.0-py3-none-any.whl", hash = "sha256:7aada352fb0c7f5538c4fafeddf314d3a6a92ee8e2b1de70482329e42de70301", size = 63141, upload-time = "2023-05-14T14:48:49.24Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "dep" },
        ]

        [package.metadata]
        requires-dist = [{ name = "dep", directory = "dep" }]

        [[package]]
        name = "setuptools"
        version = "69.2.0"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        build-dependencies = [
            { name = "wheel", version = "0.43.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/4d/5b/dc575711b6b8f2f866131a40d053e30e962e633b332acf7cd2c24843d83d/setuptools-69.2.0.tar.gz", hash = "sha256:0ff4183f8f42cd8fa3acea16c45205521a4ef28f73c6391d8a25e92893134f2e", size = 2222950, upload-time = "2024-03-13T11:20:59.219Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/92/e1/1c8bb3420105e70bdf357d57dd5567202b4ef8d27f810e98bb962d950834/setuptools-69.2.0-py3-none-any.whl", hash = "sha256:c21c49fb1042386df081cb5d86759792ab89efca84cf114889191cd09aacc80c", size = 821485, upload-time = "2024-03-13T11:20:54.103Z" },
        ]

        [[package]]
        name = "wheel"
        version = "0.43.0"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        build-dependencies = [
            { name = "flit-core", version = "3.9.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/b8/d6/ac9cd92ea2ad502ff7c1ab683806a9deb34711a1e2bd8a59814e8fc27e69/wheel-0.43.0.tar.gz", hash = "sha256:465ef92c69fa5c5da2d1cf8ac40559a8c940886afcef87dcf14b9470862f1d85", size = 99109, upload-time = "2024-03-11T19:29:17.32Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/7d/cd/d7460c9a869b16c3dd4e1e403cce337df165368c71d6af229a74699622ce/wheel-0.43.0-py3-none-any.whl", hash = "sha256:55c570405f142630c6b9f72fe09d9b67cf1477fcf543ae5b8dcb1f5b7377da81", size = 65775, upload-time = "2024-03-11T19:29:15.522Z" },
        ]
        "#
        );
    });

    // Re-lock with `--upgrade` and a newer cutoff. The build dependency
    // preferences from the original lock should not hold `setuptools` back.
    context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--upgrade")
        .arg("--exclude-newer")
        .arg("2025-01-30T00:00:00Z")
        .assert()
        .success();

    let lock_upgraded = context.read("uv.lock");
    let dep = package_section(&lock_upgraded, "dep");
    assert!(
        dep.contains(r#"{ name = "setuptools", version = "75.8.0" }"#),
        "{dep}"
    );
    assert!(
        !dep.contains(r#"{ name = "setuptools", version = "69.2.0" }"#),
        "{dep}"
    );

    // Verify the lock file is valid.
    uv_snapshot!(context.filters(), context.lock().arg("--preview-features").arg("lock-build-dependencies").arg("--locked").arg("--exclude-newer").arg("2025-01-30T00:00:00Z"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    Ok(())
}

/// Verify that `--exclude-newer` is respected for build dependency resolution.
#[test]
fn lock_build_dependencies_exclude_newer() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        dynamic = ["version"]
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.setuptools.dynamic]
        version = {attr = "dep.__version__"}
        "#,
    )?;
    dep_dir.child("dep").create_dir_all()?;
    dep_dir
        .child("dep/__init__.py")
        .write_str("__version__ = '0.1.0'")?;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
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

    // Lock with an exclude-newer date.
    uv_snapshot!(context.filters(), context.lock().arg("--preview-features").arg("lock-build-dependencies").arg("--exclude-newer").arg("2024-03-25T00:00:00Z"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 2
        revision = 4
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[resolution]]
        id = "build:dep:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "dep"
        roots = [
            { name = "setuptools", version = "69.2.0" },
        ]

        [[resolution]]
        id = "build:dep:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "dep"
        roots = [
            { name = "setuptools", version = "69.2.0" },
            { name = "wheel", version = "0.43.0" },
        ]

        [[resolution]]
        id = "build:flit-core:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "flit-core"

        [[resolution]]
        id = "build:flit-core:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "flit-core"

        [[resolution]]
        id = "build:setuptools:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "setuptools"

        [[resolution]]
        id = "build:setuptools:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "setuptools"
        roots = [
            { name = "wheel", version = "0.43.0" },
        ]

        [[resolution]]
        id = "build:wheel:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "wheel"
        roots = [
            { name = "flit-core", version = "3.9.0" },
        ]

        [[resolution]]
        id = "build:wheel:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "wheel"
        roots = [
            { name = "flit-core", version = "3.9.0" },
        ]

        [[package]]
        name = "dep"
        source = { directory = "dep" }
        build-dependencies = [
            { name = "setuptools", version = "69.2.0" },
            { name = "wheel", version = "0.43.0" },
        ]

        [package.metadata]
        build-system = { build-backend = "setuptools.build_meta" }
        build-requires = [{ name = "setuptools", specifier = ">=42" }]

        [[package]]
        name = "flit-core"
        version = "3.9.0"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        build-dependencies = []
        sdist = { url = "https://files.pythonhosted.org/packages/c4/e6/c1ac50fe3eebb38a155155711e6e864e254ce4b6e17fe2429b4c4d5b9e80/flit_core-3.9.0.tar.gz", hash = "sha256:72ad266176c4a3fcfab5f2930d76896059851240570ce9a98733b658cb786eba", size = 41917, upload-time = "2023-05-14T14:48:51.809Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/38/45/618e84e49a6c51e5dd15565ec2fcd82ab273434f236b8f108f065ded517a/flit_core-3.9.0-py3-none-any.whl", hash = "sha256:7aada352fb0c7f5538c4fafeddf314d3a6a92ee8e2b1de70482329e42de70301", size = 63141, upload-time = "2023-05-14T14:48:49.24Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "dep" },
        ]

        [package.metadata]
        requires-dist = [{ name = "dep", directory = "dep" }]

        [[package]]
        name = "setuptools"
        version = "69.2.0"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        build-dependencies = [
            { name = "wheel", version = "0.43.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/4d/5b/dc575711b6b8f2f866131a40d053e30e962e633b332acf7cd2c24843d83d/setuptools-69.2.0.tar.gz", hash = "sha256:0ff4183f8f42cd8fa3acea16c45205521a4ef28f73c6391d8a25e92893134f2e", size = 2222950, upload-time = "2024-03-13T11:20:59.219Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/92/e1/1c8bb3420105e70bdf357d57dd5567202b4ef8d27f810e98bb962d950834/setuptools-69.2.0-py3-none-any.whl", hash = "sha256:c21c49fb1042386df081cb5d86759792ab89efca84cf114889191cd09aacc80c", size = 821485, upload-time = "2024-03-13T11:20:54.103Z" },
        ]

        [[package]]
        name = "wheel"
        version = "0.43.0"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        build-dependencies = [
            { name = "flit-core", version = "3.9.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/b8/d6/ac9cd92ea2ad502ff7c1ab683806a9deb34711a1e2bd8a59814e8fc27e69/wheel-0.43.0.tar.gz", hash = "sha256:465ef92c69fa5c5da2d1cf8ac40559a8c940886afcef87dcf14b9470862f1d85", size = 99109, upload-time = "2024-03-11T19:29:17.32Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/7d/cd/d7460c9a869b16c3dd4e1e403cce337df165368c71d6af229a74699622ce/wheel-0.43.0-py3-none-any.whl", hash = "sha256:55c570405f142630c6b9f72fe09d9b67cf1477fcf543ae5b8dcb1f5b7377da81", size = 65775, upload-time = "2024-03-11T19:29:15.522Z" },
        ]
        "#
        );
    });

    // Verify the lock file is valid.
    uv_snapshot!(context.filters(), context.lock().arg("--preview-features").arg("lock-build-dependencies").arg("--locked"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    Ok(())
}

/// Verify that `extra-build-dependencies` are included in the build dependency
/// resolution and captured in the lock file.
#[test]
fn lock_build_dependencies_extra() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        dynamic = ["version"]
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.setuptools.dynamic]
        version = {attr = "dep.__version__"}
        "#,
    )?;
    dep_dir.child("dep").create_dir_all()?;
    dep_dir
        .child("dep/__init__.py")
        .write_str("__version__ = '0.1.0'")?;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep"]

        [tool.uv.sources]
        dep = { path = "dep" }

        [tool.uv.extra-build-dependencies]
        dep = ["iniconfig"]
        "#,
    )?;

    // Lock with extra-build-dependencies (requires preview).
    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("extra-build-dependencies,lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");

    let dep = package_section(&lock, "dep");
    assert!(
        dep.contains(r#"{ name = "iniconfig", version = "2.0.0" }"#),
        "{dep}"
    );
    assert!(dep.contains(r#"{ name = "setuptools", version = "69.2.0" }"#));
    assert!(lock.contains(r#"name = "iniconfig""#));

    Ok(())
}

/// Verify build locking honors extra build dependencies constrained to the runtime resolution.
#[test]
fn lock_build_dependencies_extra_match_runtime() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let child_dir = context.temp_dir.child("child");
    child_dir.create_dir_all()?;
    child_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "child"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = []
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
    )?;
    child_dir.child("build_backend.py").write_str(
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
        dependencies = ["anyio<4.1", "child"]

        [tool.uv.sources]
        child = { path = "child" }

        [tool.uv.extra-build-dependencies]
        child = [{ requirement = "anyio", match-runtime = true }]
        "#,
    )?;

    context
        .lock()
        .arg("--preview-features")
        .arg("extra-build-dependencies,lock-build-dependencies")
        .assert()
        .success();

    let lock = context.read("uv.lock");
    let child = package_section(&lock, "child");
    assert!(
        child.contains(r#"build-requires = [{ name = "anyio" }]"#),
        "{child}"
    );
    assert!(
        child.contains(r#"{ name = "anyio", version = "4.0.0", match-runtime = true }"#),
        "{child}"
    );

    context
        .lock()
        .arg("--preview-features")
        .arg("extra-build-dependencies,lock-build-dependencies")
        .arg("--locked")
        .assert()
        .success();

    Ok(())
}

/// Verify source-matched `match-runtime` requirements replay without resolving mutable metadata.
#[test]
fn lock_build_dependencies_extra_match_runtime_source_replay() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let builder_dir = context.temp_dir.child("runtime-builder");
    builder_dir.create_dir_all()?;
    let builder_pyproject = builder_dir.child("pyproject.toml");
    builder_pyproject.write_str(
        r#"
        [project]
        name = "runtime-builder"
        version = "1.0.0"
        requires-python = ">=3.12"

        [build-system]
        requires = []
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
    )?;
    builder_dir.child("build_backend.py").write_str(
        r#"
from pathlib import Path
from zipfile import ZipFile

def get_requires_for_build_wheel(config_settings=None):
    return []

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    filename = "runtime_builder-1.0.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("runtime_builder.py", "")
        wheel.writestr(
            "runtime_builder-1.0.0.dist-info/METADATA",
            "Metadata-Version: 2.3\nName: runtime-builder\nVersion: 1.0.0\n",
        )
        wheel.writestr(
            "runtime_builder-1.0.0.dist-info/WHEEL",
            "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
        )
        wheel.writestr("runtime_builder-1.0.0.dist-info/RECORD", "")
    return filename
"#,
    )?;
    let builder_url = Url::from_directory_path(builder_dir.path()).unwrap();

    let child_dir = context.temp_dir.child("child");
    child_dir.create_dir_all()?;
    child_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "child"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = []
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
    )?;
    child_dir.child("build_backend.py").write_str(
        r#"
from pathlib import Path
from zipfile import ZipFile

def get_requires_for_build_wheel(config_settings=None):
    return []

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    import runtime_builder
    filename = "child-0.1.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("child/__init__.py", "")
        wheel.writestr(
            "child-0.1.0.dist-info/METADATA",
            "Metadata-Version: 2.3\nName: child\nVersion: 0.1.0\n",
        )
        wheel.writestr(
            "child-0.1.0.dist-info/WHEEL",
            "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
        )
        wheel.writestr("child-0.1.0.dist-info/RECORD", "")
    return filename
"#,
    )?;

    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(&format!(
            r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["child", "runtime-builder @ {builder_url}"]

        [tool.uv.sources]
        child = {{ path = "child" }}

        [tool.uv.extra-build-dependencies]
        child = [{{ requirement = "runtime-builder", match-runtime = true }}]
        "#,
        ))?;

    context
        .lock()
        .arg("--preview-features")
        .arg("extra-build-dependencies,lock-build-dependencies")
        .assert()
        .success();

    let missing_url = Url::from_directory_path(context.temp_dir.child("missing").path()).unwrap();
    child_dir.child("pyproject.toml").write_str(&format!(
        r#"
        [project]
        name = "child"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["missing @ {missing_url}"]
        backend-path = ["."]
        build-backend = "build_backend"
        "#
    ))?;

    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(&format!(
            r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["child", "runtime-builder @ {builder_url}"]

        [tool.uv]
        build-constraint-dependencies = ["runtime-builder==999"]

        [tool.uv.sources]
        child = {{ path = "child" }}

        [tool.uv.extra-build-dependencies]
        child = [{{ requirement = "runtime-builder", match-runtime = true }}]
        "#,
        ))?;

    context
        .sync()
        .arg("--frozen")
        .arg("--no-index")
        .arg("--preview-features")
        .arg("extra-build-dependencies,lock-build-dependencies")
        .assert()
        .success();

    Ok(())
}

/// Verify `match-runtime` preserves mutually exclusive runtime branches.
#[test]
fn lock_build_dependencies_extra_match_runtime_conflicts() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let child_dir = context.temp_dir.child("child");
    child_dir.create_dir_all()?;
    child_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "child"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = []
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
    )?;
    child_dir.child("build_backend.py").write_str(
        r#"
from importlib.metadata import version
from pathlib import Path
from zipfile import ZipFile

def get_requires_for_build_wheel(config_settings=None):
    return []

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    anyio_version = version("anyio")
    filename = "child-0.1.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("child/__init__.py", f'BUILD_ANYIO = "{anyio_version}"\n')
        wheel.writestr(
            "child-0.1.0.dist-info/METADATA",
            "Metadata-Version: 2.3\nName: child\nVersion: 0.1.0\n",
        )
        wheel.writestr(
            "child-0.1.0.dist-info/WHEEL",
            "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
        )
        wheel.writestr("child-0.1.0.dist-info/RECORD", "")
    return filename
"#,
    )?;

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [project.optional-dependencies]
        extra1 = ["anyio==3.7.1", "child"]
        extra2 = ["anyio==4.0.0", "child"]

        [tool.uv]
        conflicts = [[
            { extra = "extra1" },
            { extra = "extra2" },
        ]]

        [tool.uv.sources]
        child = { path = "child" }

        [tool.uv.extra-build-dependencies]
        child = [{ requirement = "anyio", match-runtime = true }]
        "#,
    )?;

    context
        .lock()
        .arg("--preview-features")
        .arg("extra-build-dependencies,lock-build-dependencies")
        .assert()
        .success();

    let lock = context.read("uv.lock");
    let resolutions = resolution_sections(&lock);
    let child_resolutions = resolutions
        .split("[[resolution]]")
        .filter(|resolution| resolution.contains("\nname = \"child\"\n"))
        .collect::<Vec<_>>();
    assert!(
        child_resolutions.iter().any(|resolution| {
            resolution.contains(r#"extra = "extra1""#)
                && resolution.contains(r#"{ name = "anyio", version = "3.7.1""#)
        }),
        "{resolutions}"
    );
    assert!(
        child_resolutions.iter().any(|resolution| {
            resolution.contains(r#"extra = "extra2""#)
                && resolution.contains(r#"{ name = "anyio", version = "4.0.0""#)
        }),
        "{resolutions}"
    );

    fs_err::remove_dir_all(&context.cache_dir)?;
    context
        .sync()
        .arg("--extra")
        .arg("extra1")
        .arg("--no-index")
        .arg("--frozen")
        .arg("--preview-features")
        .arg("extra-build-dependencies,lock-build-dependencies")
        .assert()
        .success();
    uv_snapshot!(context.filters(), context
        .run()
        .arg("--no-sync")
        .arg("python")
        .arg("-c")
        .arg("import child; print(child.BUILD_ANYIO)"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    3.7.1

    ----- stderr -----
    ");

    fs_err::remove_dir_all(&context.venv)?;
    fs_err::remove_dir_all(&context.cache_dir)?;
    context
        .sync()
        .arg("--extra")
        .arg("extra2")
        .arg("--no-index")
        .arg("--frozen")
        .arg("--preview-features")
        .arg("extra-build-dependencies,lock-build-dependencies")
        .assert()
        .success();
    uv_snapshot!(context.filters(), context
        .run()
        .arg("--no-sync")
        .arg("python")
        .arg("-c")
        .arg("import child; print(child.BUILD_ANYIO)"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    4.0.0

    ----- stderr -----
    ");

    Ok(())
}

/// Verify `match-runtime` locked build dependencies replay for a foreign target.
#[test]
fn lock_build_dependencies_extra_match_runtime_cross_target() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let (target_platform, target_marker) = if cfg!(target_os = "windows") {
        ("linux", "sys_platform == 'linux'")
    } else {
        ("windows", "sys_platform == 'win32'")
    };

    let child_dir = context.temp_dir.child("child");
    child_dir.create_dir_all()?;
    child_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "child"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = []
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
    )?;
    child_dir.child("build_backend.py").write_str(
        r#"
from pathlib import Path
from zipfile import ZipFile

def get_requires_for_build_wheel(config_settings=None):
    return []

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    import anyio
    filename = "child-0.1.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("child/__init__.py", "")
        wheel.writestr(
            "child-0.1.0.dist-info/METADATA",
            "Metadata-Version: 2.3\nName: child\nVersion: 0.1.0\n",
        )
        wheel.writestr(
            "child-0.1.0.dist-info/WHEEL",
            "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
        )
        wheel.writestr("child-0.1.0.dist-info/RECORD", "")
    return filename
"#,
    )?;

    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(&format!(
            r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "child ; {target_marker}",
            "anyio<4.1 ; {target_marker}",
        ]

        [tool.uv.sources]
        child = {{ path = "child" }}

        [tool.uv.extra-build-dependencies]
        child = [{{ requirement = "anyio", match-runtime = true }}]
        "#
        ))?;

    context
        .lock()
        .arg("--preview-features")
        .arg("extra-build-dependencies,lock-build-dependencies")
        .assert()
        .success();

    let lock = context.read("uv.lock");
    let child = package_section(&lock, "child");
    assert!(child.contains("match-runtime = true"), "{child}");
    assert!(!lock.contains("\nexecutor = "), "{lock}");

    uv_snapshot!(context.filters(), context
        .sync()
        .arg("--python-platform")
        .arg(target_platform)
        .arg("--no-index")
        .arg("--frozen")
        .arg("--preview-features")
        .arg("extra-build-dependencies,lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.0.0
     + child==0.1.0 (from file://[TEMP_DIR]/child)
     + idna==3.6
     + sniffio==1.3.1
    ");

    Ok(())
}

/// Verify the hook-expanded build resolution replaces its initial stage without dropping extras.
#[test]
fn lock_build_dependencies_hook_resolution_replaces_initial_stage() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    for version in ["1.0.0", "2.0.0"] {
        write_wheel(
            &links_dir.child(format!("seed-{version}-py3-none-any.whl")),
            "seed",
            version,
        )?;
    }
    write_wheel(
        &links_dir.child("extra-0.1.0-py3-none-any.whl"),
        "extra",
        "0.1.0",
    )?;

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        dynamic = ["version"]
        requires-python = ">=3.12"

        [build-system]
        requires = ["seed"]
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
    )?;
    dep_dir.child("build_backend.py").write_str(
        r#"
from pathlib import Path

def get_requires_for_build_wheel(config_settings=None):
    return ["seed<2"]

def prepare_metadata_for_build_wheel(metadata_directory, config_settings=None):
    dist_info = Path(metadata_directory) / "dep-0.1.0.dist-info"
    dist_info.mkdir()
    (dist_info / "METADATA").write_text(
        "Metadata-Version: 2.3\nName: dep\nVersion: 0.1.0\n"
    )
    return dist_info.name
"#,
    )?;

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep"]

        [tool.uv.sources]
        dep = { path = "dep" }

        [tool.uv.extra-build-dependencies]
        dep = ["extra"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--preview-features")
        .arg("extra-build-dependencies,lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    let dep = package_section(&lock, "dep");
    assert!(
        dep.contains(r#"{ name = "extra", version = "0.1.0" }"#),
        "{dep}"
    );
    assert!(
        dep.contains(r#"{ name = "seed", version = "1.0.0""#),
        "{dep}"
    );
    assert!(
        !dep.contains(r#"{ name = "seed", version = "2.0.0" }"#),
        "{dep}"
    );

    // Removing the initial root must not allow a frozen build to fall back to live resolution.
    let bootstrap_root = lock
        .lines()
        .find(|line| line.starts_with("    { name = \"seed\", version = \"2.0.0\""))
        .expect("locked bootstrap root");
    let bootstrap_root = format!("{bootstrap_root}\n");
    context
        .temp_dir
        .child("uv.lock")
        .write_str(&lock.replacen(&bootstrap_root, "", 1))?;

    uv_snapshot!(context.filters(), context
        .sync()
        .arg("--no-index")
        .arg("--no-cache")
        .arg("--frozen")
        .arg("--preview-features")
        .arg("extra-build-dependencies,lock-build-dependencies"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to build `dep @ file://[TEMP_DIR]/dep`
      ├─▶ Failed to resolve requirements from `build-system.requires` and `extra-build-dependencies`
      ╰─▶ The initial build requirements for `dep` do not match the locked bootstrap environment

    hint: `dep` was included because `project` (v0.1.0) depends on `dep`
    ");

    Ok(())
}

/// Verify static metadata builds lock dependencies returned by the backend hook.
#[test]
fn lock_build_dependencies_static_metadata_captures_hook_requirements() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    write_wheel(
        &links_dir.child("helper-0.1.0-py3-none-any.whl"),
        "helper",
        "0.1.0",
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
from importlib.metadata import PackageNotFoundError, version
from pathlib import Path
from zipfile import ZipFile

def get_requires_for_build_wheel(config_settings=None):
    try:
        version("helper")
    except PackageNotFoundError:
        return ["helper"]
    raise RuntimeError("helper is installed before the backend hook runs")

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    if version("helper") != "0.1.0":
        raise RuntimeError("helper is unavailable")
    filename = "dep-0.1.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("dep/__init__.py", "")
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

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    let dep = package_section(&lock, "dep");
    assert!(
        dep.contains(r#"{ name = "helper", version = "0.1.0" }"#),
        "{dep}"
    );

    uv_snapshot!(context.filters(), context
        .sync()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--frozen")
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + dep==0.1.0 (from file://[TEMP_DIR]/dep)
    ");

    Ok(())
}

/// Verify that hook-affecting build configuration invalidates a locked build graph.
#[test]
fn lock_build_dependencies_config_settings_invalidate_hook_requirements() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    write_wheel(
        &links_dir.child("helper_a-0.1.0-py3-none-any.whl"),
        "helper-a",
        "0.1.0",
    )?;
    write_wheel(
        &links_dir.child("helper_b-0.1.0-py3-none-any.whl"),
        "helper-b",
        "0.1.0",
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
def get_requires_for_build_wheel(config_settings=None):
    choice = config_settings.get("choice", "a")
    return [f"helper-{choice}"]
"#,
    )?;

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

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--config-settings")
        .arg("choice=a")
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    let dep = package_section(&lock, "dep");
    assert!(
        dep.contains(r#"{ name = "helper-a", version = "0.1.0" }"#),
        "{dep}"
    );
    assert!(lock.contains("build-settings = "), "{lock}");
    assert!(!lock.contains("choice = \"a\""), "{lock}");

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--config-settings")
        .arg("choice=b")
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

/// Verify that source distribution selection policy changes invalidate a locked build graph.
#[test]
fn lock_build_dependencies_no_binary_invalidate_hook_requirements() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    write_wheel(
        &links_dir.child("dep-0.1.0-py3-none-any.whl"),
        "dep",
        "0.1.0",
    )?;

    let source_dist = links_dir.child("dep-0.1.0.zip");
    let mut zip = ZipFileWriter::new(Vec::new());
    let entry = ZipEntryBuilder::new("dep-0.1.0/pyproject.toml".into(), Compression::Stored);
    block_on(zip.write_entry_whole(
        entry,
        br#"
        [project]
        name = "dep"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = []
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
    ))?;
    let entry = ZipEntryBuilder::new("dep-0.1.0/build_backend.py".into(), Compression::Stored);
    block_on(zip.write_entry_whole(
        entry,
        br"
def get_requires_for_build_wheel(config_settings=None):
    return []
",
    ))?;
    fs_err::write(source_dist.path(), block_on(zip.close())?)?;

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep==0.1.0"]
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
    assert!(!lock.contains("build-settings = "), "{lock}");

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--no-binary-package")
        .arg("dep")
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

/// Verify that build isolation policy changes invalidate a locked build graph.
#[test]
fn lock_build_dependencies_build_isolation_invalidate_hook_requirements() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    write_wheel(
        &links_dir.child("helper-0.1.0-py3-none-any.whl"),
        "helper",
        "0.1.0",
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
def get_requires_for_build_wheel(config_settings=None):
    return ["helper"]
"#,
    )?;

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

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--no-build-isolation-package")
        .arg("dep")
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    let dep = package_section(&lock, "dep");
    assert!(!dep.contains(r#"{ name = "helper", version = "0.1.0" }"#));
    assert!(lock.contains("build-settings = "), "{lock}");

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
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

/// Verify that frozen builds reject changed backend hook requirements and replay removed ones.
#[test]
fn lock_build_dependencies_static_metadata_revalidates_hook_requirements() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    write_wheel(
        &links_dir.child("helper-0.1.0-py3-none-any.whl"),
        "helper",
        "0.1.0",
    )?;
    write_wheel(
        &links_dir.child("helper-0.2.0-py3-none-any.whl"),
        "helper",
        "0.2.0",
    )?;

    let source_dist = context.temp_dir.child("dep-0.1.0.zip");
    let write_source_dist = || {
        let mut zip = ZipFileWriter::new(Vec::new());
        let entry = ZipEntryBuilder::new("dep-0.1.0/pyproject.toml".into(), Compression::Stored);
        block_on(zip.write_entry_whole(
            entry,
            br#"
            [project]
            name = "dep"
            version = "0.1.0"
            requires-python = ">=3.12"

            [build-system]
            requires = []
            backend-path = ["."]
            build-backend = "build_backend"
            "#,
        ))?;
        let entry = ZipEntryBuilder::new("dep-0.1.0/build_backend.py".into(), Compression::Stored);
        block_on(
            zip.write_entry_whole(
                entry,
                r#"
import os
from importlib.metadata import version
from pathlib import Path
from zipfile import ZipFile

def get_requires_for_build_wheel(config_settings=None):
    helper_version = os.environ["UV_TEST_HELPER_VERSION"]
    return [f"helper=={helper_version}"] if helper_version else []

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    if version("helper") != (os.environ["UV_TEST_HELPER_VERSION"] or "0.1.0"):
        raise RuntimeError("unexpected helper version")
    filename = "dep-0.1.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("dep/__init__.py", "")
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
"#
                .as_bytes(),
            ),
        )?;
        let entry = ZipEntryBuilder::new("dep-0.1.0/dep/__init__.py".into(), Compression::Stored);
        block_on(zip.write_entry_whole(entry, b""))?;
        fs_err::write(source_dist.path(), block_on(zip.close())?)?;
        Ok::<_, anyhow::Error>(())
    };
    write_source_dist()?;

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep"]

        [tool.uv.sources]
        dep = { path = "dep-0.1.0.zip" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .env("UV_TEST_HELPER_VERSION", "0.1.0"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    let dep = package_section(&lock, "dep");
    assert!(
        dep.contains(r#"{ name = "helper", version = "0.1.0" }"#),
        "{dep}"
    );

    uv_snapshot!(context.filters(), context
        .sync()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--frozen")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .env("UV_TEST_HELPER_VERSION", "0.2.0"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to build `dep @ file://[TEMP_DIR]/dep-0.1.0.zip`
      ├─▶ Failed to resolve requirements from `build-system.requires`
      ╰─▶ The build requirements returned by the backend for `dep` do not match the locked build environment

    hint: `dep` was included because `project` (v0.1.0) depends on `dep`
    ");

    // The hook no longer reports `helper`, but the frozen build must still replay the captured
    // final environment after the dependency hook and before invoking the build hook.
    context
        .sync()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--frozen")
        .arg("--reinstall")
        .arg("--no-cache")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .env("UV_TEST_HELPER_VERSION", "")
        .assert()
        .success();

    Ok(())
}

/// Verify that enabling or changing a locked build environment invalidates stale built wheels.
#[test]
fn lock_build_dependencies_invalidate_built_wheel_cache() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    write_wheel(
        &links_dir.child("helper-0.1.0-py3-none-any.whl"),
        "helper",
        "0.1.0",
    )?;
    write_wheel(
        &links_dir.child("helper-0.2.0-py3-none-any.whl"),
        "helper",
        "0.2.0",
    )?;

    let source_dist = context.temp_dir.child("dep-0.1.0.zip");
    let mut zip = ZipFileWriter::new(Vec::new());
    let entry = ZipEntryBuilder::new("dep-0.1.0/pyproject.toml".into(), Compression::Stored);
    block_on(zip.write_entry_whole(
        entry,
        br#"
        [project]
        name = "dep"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = []
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
    ))?;
    let entry = ZipEntryBuilder::new("dep-0.1.0/build_backend.py".into(), Compression::Stored);
    block_on(
        zip.write_entry_whole(
            entry,
            r#"
import os
from importlib.metadata import version
from pathlib import Path
from zipfile import ZipFile

def get_requires_for_build_wheel(config_settings=None):
    return [f"helper=={os.environ['UV_TEST_HELPER_VERSION']}"]

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    filename = "dep-0.1.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("dep/__init__.py", f"HELPER = {version('helper')!r}\n")
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
"#
            .as_bytes(),
        ),
    )?;
    let entry = ZipEntryBuilder::new("dep-0.1.0/dep/__init__.py".into(), Compression::Stored);
    block_on(zip.write_entry_whole(entry, b""))?;
    fs_err::write(source_dist.path(), block_on(zip.close())?)?;

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep"]

        [tool.uv.sources]
        dep = { path = "dep-0.1.0.zip" }
        "#,
    )?;

    context
        .lock()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .env("UV_TEST_HELPER_VERSION", "0.2.0")
        .assert()
        .success();
    context
        .sync()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--frozen")
        .env("UV_TEST_HELPER_VERSION", "0.2.0")
        .assert()
        .success();
    assert_eq!(
        fs_err::read_to_string(context.site_packages().join("dep/__init__.py"))?,
        "HELPER = '0.2.0'\n"
    );

    fs_err::remove_file(context.temp_dir.join("uv.lock"))?;
    context
        .lock()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .env("UV_TEST_HELPER_VERSION", "0.1.0")
        .assert()
        .success();
    context
        .sync()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--frozen")
        .arg("--reinstall")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .env("UV_TEST_HELPER_VERSION", "0.1.0")
        .assert()
        .success();
    assert_eq!(
        fs_err::read_to_string(context.site_packages().join("dep/__init__.py"))?,
        "HELPER = '0.1.0'\n"
    );

    fs_err::remove_file(context.temp_dir.join("uv.lock"))?;
    context
        .lock()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .env("UV_TEST_HELPER_VERSION", "0.2.0")
        .assert()
        .success();
    let lock = context.read("uv.lock");
    assert!(
        package_section(&lock, "dep").contains(r#"{ name = "helper", version = "0.2.0" }"#),
        "{lock}"
    );
    context
        .sync()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--frozen")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .env("UV_TEST_HELPER_VERSION", "0.2.0")
        .assert()
        .success();
    assert_eq!(
        fs_err::read_to_string(context.site_packages().join("dep/__init__.py"))?,
        "HELPER = '0.2.0'\n"
    );

    Ok(())
}

/// Verify that changing a nested source build environment invalidates the outer built wheel.
#[test]
fn lock_build_dependencies_invalidate_nested_built_wheel_cache() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    write_wheel(
        &links_dir.child("tool-0.1.0-py3-none-any.whl"),
        "tool",
        "0.1.0",
    )?;
    write_wheel(
        &links_dir.child("tool-0.2.0-py3-none-any.whl"),
        "tool",
        "0.2.0",
    )?;

    let helper_dir = context.temp_dir.child("helper");
    helper_dir.create_dir_all()?;
    helper_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "helper"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = []
        backend-path = ["."]
        build-backend = "build_backend"

        [tool.uv]
        cache-keys = [{ file = "helper-value.txt" }]
        "#,
    )?;
    helper_dir.child("helper-value.txt").write_str("first")?;
    helper_dir.child("build_backend.py").write_str(
        r#"
import os
from importlib.metadata import version
from pathlib import Path
from zipfile import ZipFile

def get_requires_for_build_wheel(config_settings=None):
    return [f"tool=={os.environ['UV_TEST_TOOL_VERSION']}"]

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    filename = "helper-0.1.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        value = Path(__file__).with_name("helper-value.txt").read_text()
        wheel.writestr("helper/__init__.py", f"TOOL = {version('tool')!r}\nVALUE = {value!r}\n")
        wheel.writestr(
            "helper-0.1.0.dist-info/METADATA",
            "Metadata-Version: 2.3\nName: helper\nVersion: 0.1.0\n",
        )
        wheel.writestr(
            "helper-0.1.0.dist-info/WHEEL",
            "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
        )
        wheel.writestr("helper-0.1.0.dist-info/RECORD", "")
    return filename
"#,
    )?;

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(&format!(
        r#"
        [project]
        name = "dep"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["helper @ {}"]
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
        Url::from_directory_path(helper_dir.path()).expect("valid helper URL")
    ))?;
    dep_dir.child("build_backend.py").write_str(
        r#"
from pathlib import Path
from zipfile import ZipFile

from helper import TOOL, VALUE

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    filename = "dep-0.1.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("dep/__init__.py", f"TOOL = {TOOL!r}\nVALUE = {VALUE!r}\n")
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
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .env("UV_TEST_TOOL_VERSION", "0.1.0")
        .assert()
        .success();
    context
        .sync()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--frozen")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .env("UV_TEST_TOOL_VERSION", "0.1.0")
        .assert()
        .success();
    assert_eq!(
        fs_err::read_to_string(context.site_packages().join("dep/__init__.py"))?,
        "TOOL = '0.1.0'\nVALUE = 'first'\n"
    );

    fs_err::remove_file(context.temp_dir.join("uv.lock"))?;
    context
        .lock()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--no-cache")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .env("UV_TEST_TOOL_VERSION", "0.2.0")
        .assert()
        .success();
    let lock = context.read("uv.lock");
    assert!(
        package_section(&lock, "helper").contains(r#"{ name = "tool", version = "0.2.0" }"#),
        "{lock}"
    );
    context
        .sync()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--frozen")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .env("UV_TEST_TOOL_VERSION", "0.2.0")
        .assert()
        .success();
    assert_eq!(
        fs_err::read_to_string(context.site_packages().join("dep/__init__.py"))?,
        "TOOL = '0.2.0'\nVALUE = 'first'\n"
    );

    helper_dir.child("helper-value.txt").write_str("second")?;
    context
        .sync()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--frozen")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .env("UV_TEST_TOOL_VERSION", "0.2.0")
        .assert()
        .success();
    assert_eq!(
        fs_err::read_to_string(context.site_packages().join("dep/__init__.py"))?,
        "TOOL = '0.2.0'\nVALUE = 'second'\n"
    );

    Ok(())
}

/// Verify that changing a hashless build wheel in a local index invalidates the outer built wheel.
#[test]
fn lock_build_dependencies_invalidate_mutable_build_wheel_cache() -> Result<()> {
    fn write_helper_wheel(path: &ChildPath, value: &str) -> Result<()> {
        let mut zip = ZipFileWriter::new(Vec::new());
        let entry = ZipEntryBuilder::new("helper.py".into(), Compression::Stored);
        block_on(zip.write_entry_whole(entry, format!("VALUE = {value:?}\n").as_bytes()))?;
        let entry = ZipEntryBuilder::new(
            "helper-0.1.0.dist-info/METADATA".into(),
            Compression::Stored,
        );
        block_on(zip.write_entry_whole(
            entry,
            b"Metadata-Version: 2.3\nName: helper\nVersion: 0.1.0\n",
        ))?;
        let entry =
            ZipEntryBuilder::new("helper-0.1.0.dist-info/WHEEL".into(), Compression::Stored);
        block_on(zip.write_entry_whole(
            entry,
            b"Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
        ))?;
        let entry =
            ZipEntryBuilder::new("helper-0.1.0.dist-info/RECORD".into(), Compression::Stored);
        block_on(zip.write_entry_whole(entry, b""))?;
        fs_err::write(path.path(), block_on(zip.close())?)?;

        Ok(())
    }

    let context = uv_test::test_context!("3.12");

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    let helper_wheel = links_dir.child("helper-0.1.0-py3-none-any.whl");
    write_helper_wheel(&helper_wheel, "first")?;

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["helper==0.1.0"]
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
    )?;
    dep_dir.child("build_backend.py").write_str(
        r#"
from pathlib import Path
from zipfile import ZipFile

from helper import VALUE

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    filename = "dep-0.1.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("dep/__init__.py", f"VALUE = {VALUE!r}\n")
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
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();
    let lock = context.read("uv.lock");
    let helper = package_section(&lock, "helper");
    assert!(!helper.contains("hash ="), "{helper}");
    context
        .sync()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--frozen")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();
    assert_eq!(
        fs_err::read_to_string(context.site_packages().join("dep/__init__.py"))?,
        "VALUE = 'first'\n"
    );

    write_helper_wheel(&helper_wheel, "second")?;
    context
        .sync()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--frozen")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();
    assert_eq!(
        fs_err::read_to_string(context.site_packages().join("dep/__init__.py"))?,
        "VALUE = 'second'\n"
    );

    Ok(())
}

/// Verify that build-only dependency edges are retained for runtime-shared packages.
#[test]
fn lock_build_dependencies_merge_runtime_shared_build_edges() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let (build_marker, runtime_marker) = if cfg!(target_os = "macos") {
        ("sys_platform == 'darwin'", "sys_platform != 'darwin'")
    } else if cfg!(target_os = "windows") {
        ("sys_platform == 'win32'", "sys_platform != 'win32'")
    } else {
        ("sys_platform == 'linux'", "sys_platform != 'linux'")
    };

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    let leaf_requirement = format!("leaf==0.1.0 ; {build_marker}");
    write_wheel_with_requires(
        &links_dir.child("helper-0.1.0-py3-none-any.whl"),
        "helper",
        "0.1.0",
        &[&leaf_requirement],
    )?;
    write_wheel(
        &links_dir.child("leaf-0.1.0-py3-none-any.whl"),
        "leaf",
        "0.1.0",
    )?;

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(&format!(
        r#"
        [project]
        name = "dep"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["helper==0.1.0 ; {build_marker}"]
        backend-path = ["."]
        build-backend = "build_backend"
        "#
    ))?;
    dep_dir.child("build_backend.py").write_str(
        r#"
from importlib.metadata import version
from pathlib import Path
from zipfile import ZipFile

def get_requires_for_build_wheel(config_settings=None):
    return []

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    if version("leaf") != "0.1.0":
        raise RuntimeError("leaf is unavailable")
    filename = "dep-0.1.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("dep/__init__.py", "")
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

    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(&format!(
            r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "dep",
            "helper==0.1.0 ; {runtime_marker}",
        ]

        [tool.uv.sources]
        dep = {{ path = "dep" }}
        "#
        ))?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    ");

    uv_snapshot!(context.filters(), context
        .sync()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--frozen")
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + dep==0.1.0 (from file://[TEMP_DIR]/dep)
    ");

    Ok(())
}

/// Verify that build-required artifacts are retained for packages that are
/// selected by the runtime resolution only on another platform.
#[test]
fn lock_build_dependencies_merge_runtime_shared_build_artifacts() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let (host_wheel_tag, foreign_wheel_tag, foreign_marker) = if cfg!(target_os = "macos") {
        (
            "macosx_10_9_universal2",
            "win_amd64",
            "sys_platform == 'win32'",
        )
    } else if cfg!(target_os = "windows") {
        (
            if cfg!(target_arch = "aarch64") {
                "win_arm64"
            } else {
                "win_amd64"
            },
            "macosx_10_9_universal2",
            "sys_platform == 'darwin'",
        )
    } else {
        (
            if cfg!(target_arch = "aarch64") {
                "manylinux_2_17_aarch64.manylinux2014_aarch64"
            } else {
                "manylinux_2_17_x86_64.manylinux2014_x86_64"
            },
            "win_amd64",
            "sys_platform == 'win32'",
        )
    };

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    write_wheel(
        &links_dir.child(format!("shared-0.1.0-py3-none-{host_wheel_tag}.whl")),
        "shared",
        "0.1.0",
    )?;
    write_wheel(
        &links_dir.child(format!("shared-0.1.0-py3-none-{foreign_wheel_tag}.whl")),
        "shared",
        "0.1.0",
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
        requires = ["shared==0.1.0"]
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
    )?;
    dep_dir.child("build_backend.py").write_str(
        r#"
from pathlib import Path
from zipfile import ZipFile

def get_requires_for_build_wheel(config_settings=None):
    return []

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    filename = "dep-0.1.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("dep/__init__.py", "")
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

    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(&format!(
            r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "dep",
            "shared==0.1.0 ; {foreign_marker}",
        ]

        [tool.uv.sources]
        dep = {{ path = "dep" }}
        "#
        ))?;

    context
        .lock()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();

    uv_snapshot!(context.filters(), context
        .sync()
        .arg("--no-index")
        .arg("--frozen")
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + dep==0.1.0 (from file://[TEMP_DIR]/dep)
    ");

    Ok(())
}

/// Verify that source distributions selected for a cross-target sync retain
/// their locked build environment on the host performing the build.
#[test]
fn lock_build_dependencies_replay_cross_target_source_builds() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let (target_platform, target_marker) = if cfg!(target_os = "windows") {
        ("linux", "sys_platform == 'linux'")
    } else {
        ("windows", "sys_platform == 'win32'")
    };

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    write_wheel(
        &links_dir.child("builder-0.1.0-py3-none-any.whl"),
        "builder",
        "0.1.0",
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
        requires = ["builder==0.1.0"]
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
    )?;
    dep_dir.child("build_backend.py").write_str(
        r#"
import builder
from pathlib import Path
from zipfile import ZipFile

def get_requires_for_build_wheel(config_settings=None):
    return []

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    filename = "dep-0.1.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("dep/__init__.py", "")
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

    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(&format!(
            r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep ; {target_marker}"]

        [tool.uv.sources]
        dep = {{ path = "dep" }}
        "#
        ))?;

    context
        .lock()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();

    uv_snapshot!(context.filters(), context
        .sync()
        .arg("--python-platform")
        .arg(target_platform)
        .arg("--no-index")
        .arg("--frozen")
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + dep==0.1.0 (from file://[TEMP_DIR]/dep)
    ");

    Ok(())
}

/// Verify that build resolution records are selected for the target platform,
/// while their dependencies are evaluated for the build host.
#[test]
fn lock_build_dependencies_replay_separates_target_and_executor_markers() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let (host_marker, target_platform, target_marker) = if cfg!(target_os = "windows") {
        (
            "sys_platform == 'win32'",
            "linux",
            "sys_platform == 'linux'",
        )
    } else if cfg!(target_os = "macos") {
        (
            "sys_platform == 'darwin'",
            "windows",
            "sys_platform == 'win32'",
        )
    } else {
        (
            "sys_platform == 'linux'",
            "windows",
            "sys_platform == 'win32'",
        )
    };

    let child_dir = context.temp_dir.child("child");
    child_dir.create_dir_all()?;
    child_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "child"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = []
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
    )?;
    child_dir.child("build_backend.py").write_str(
        r#"
from pathlib import Path
from zipfile import ZipFile

def get_requires_for_build_wheel(config_settings=None):
    return []

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    filename = "child-0.1.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("child/__init__.py", "")
        wheel.writestr(
            "child-0.1.0.dist-info/METADATA",
            "Metadata-Version: 2.3\nName: child\nVersion: 0.1.0\n",
        )
        wheel.writestr(
            "child-0.1.0.dist-info/WHEEL",
            "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
        )
        wheel.writestr("child-0.1.0.dist-info/RECORD", "")
    return filename
"#,
    )?;

    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(&format!(
            r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "child",
            "anyio==3.7.1 ; {host_marker}",
            "anyio==4.0.0 ; {target_marker}",
        ]

        [tool.uv.sources]
        child = {{ path = "child" }}

        [tool.uv.extra-build-dependencies]
        child = [{{ requirement = "anyio", match-runtime = true }}]
        "#
        ))?;

    context
        .lock()
        .arg("--preview-features")
        .arg("extra-build-dependencies,lock-build-dependencies")
        .assert()
        .success();

    child_dir.child("build_backend.py").write_str(
        r#"
from importlib.metadata import version
from pathlib import Path
from zipfile import ZipFile

def get_requires_for_build_wheel(config_settings=None):
    return []

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    assert version("anyio") == "4.0.0"
    filename = "child-0.1.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("child/__init__.py", "")
        wheel.writestr(
            "child-0.1.0.dist-info/METADATA",
            "Metadata-Version: 2.3\nName: child\nVersion: 0.1.0\n",
        )
        wheel.writestr(
            "child-0.1.0.dist-info/WHEEL",
            "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
        )
        wheel.writestr("child-0.1.0.dist-info/RECORD", "")
    return filename
"#,
    )?;

    uv_snapshot!(context.filters(), context
        .sync()
        .arg("--python-platform")
        .arg(target_platform)
        .arg("--no-cache")
        .arg("--no-index")
        .arg("--frozen")
        .arg("--preview-features")
        .arg("extra-build-dependencies,lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.0.0
     + child==0.1.0 (from file://[TEMP_DIR]/child)
     + idna==3.6
     + sniffio==1.3.1
    ");

    Ok(())
}

/// Verify that build dependency edges are scoped to each isolated source build
/// when the same build package resolves a transitive dependency differently.
#[test]
fn lock_build_dependencies_isolates_shared_build_dependency_edges() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    write_wheel(
        &links_dir.child("nested_backend-1.0.0-py3-none-any.whl"),
        "nested-backend",
        "1.0.0",
    )?;
    let nested_backend_url = Url::from_file_path(
        links_dir
            .child("nested_backend-1.0.0-py3-none-any.whl")
            .path(),
    )
    .expect("valid file URL");
    write_wheel(
        &links_dir.child("helper-1.0.0-py3-none-any.whl"),
        "helper",
        "1.0.0",
    )?;
    write_wheel(
        &links_dir.child("helper-2.0.0-py3-none-any.whl"),
        "helper",
        "2.0.0",
    )?;

    let builder_dir = context.temp_dir.child("builder");
    builder_dir.create_dir_all()?;
    builder_dir.child("pyproject.toml").write_str(&format!(
        r#"
        [project]
        name = "builder"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = ["helper>=1"]

        [build-system]
        requires = ["nested-backend @ {nested_backend_url}"]
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
    ))?;
    builder_dir.child("build_backend.py").write_str(
        r#"
from pathlib import Path
from zipfile import ZipFile

def get_requires_for_build_wheel(config_settings=None):
    return []

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    filename = "builder-1.0.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("builder.py", "")
        wheel.writestr(
            "builder-1.0.0.dist-info/METADATA",
            "Metadata-Version: 2.3\nName: builder\nVersion: 1.0.0\nRequires-Dist: helper>=1\n",
        )
        wheel.writestr(
            "builder-1.0.0.dist-info/WHEEL",
            "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
        )
        wheel.writestr("builder-1.0.0.dist-info/RECORD", "")
    return filename
"#,
    )?;
    let builder_url = Url::from_directory_path(builder_dir.path()).expect("valid file URL");

    for (name, helper_version) in [("dep-a", "1.0.0"), ("dep-b", "2.0.0")] {
        let module_name = name.replace('-', "_");
        let dep_dir = context.temp_dir.child(name);
        dep_dir.create_dir_all()?;
        dep_dir.child("pyproject.toml").write_str(&format!(
            r#"
            [project]
            name = "{name}"
            version = "0.1.0"
            requires-python = ">=3.12"

            [build-system]
            requires = ["builder @ {builder_url}", "helper=={helper_version}"]
            backend-path = ["."]
            build-backend = "build_backend"
            "#
        ))?;
        dep_dir.child("build_backend.py").write_str(&format!(
            r#"
from importlib.metadata import version
from pathlib import Path
from zipfile import ZipFile

def get_requires_for_build_wheel(config_settings=None):
    return []

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    if version("helper") != "{helper_version}":
        raise RuntimeError("unexpected helper version: " + version("helper"))
    filename = "{module_name}-0.1.0-py3-none-any.whl"
    dist_info = "{module_name}-0.1.0.dist-info"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("{module_name}/__init__.py", "")
        wheel.writestr(
            dist_info + "/METADATA",
            "Metadata-Version: 2.3\nName: {name}\nVersion: 0.1.0\n",
        )
        wheel.writestr(
            dist_info + "/WHEEL",
            "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
        )
        wheel.writestr(dist_info + "/RECORD", "")
    return filename
"#
        ))?;
    }

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [dependency-groups]
        a = ["dep-a"]
        b = ["dep-b"]

        [tool.uv.sources]
        dep-a = { path = "dep-a" }
        dep-b = { path = "dep-b" }

        [tool.uv]
        package = false
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
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(resolution_sections(&lock), @r#"
        [[resolution]]
        id = "build:builder:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "builder"
        version = "1.0.0"
        source = { directory = "[TEMP_DIR]/builder" }
        roots = [
            { name = "nested-backend", version = "1.0.0" },
        ]

        [[resolution]]
        id = "build:builder:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "builder"
        version = "1.0.0"
        source = { directory = "[TEMP_DIR]/builder" }
        roots = [
            { name = "nested-backend", version = "1.0.0" },
        ]

        [[resolution]]
        id = "build:dep-a:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "dep-a"
        roots = [
            { name = "builder", version = "1.0.0", source = { directory = "[TEMP_DIR]/builder" } },
            { name = "helper", version = "1.0.0", source = { registry = "[TEMP_DIR]/links" } },
        ]

        [[resolution]]
        id = "build:dep-a:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "dep-a"
        roots = [
            { name = "builder", version = "1.0.0", source = { directory = "[TEMP_DIR]/builder" } },
            { name = "helper", version = "1.0.0", source = { registry = "[TEMP_DIR]/links" } },
        ]

        [[resolution]]
        id = "build:dep-b:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "dep-b"
        roots = [
            { name = "builder", version = "1.0.0", source = { directory = "[TEMP_DIR]/builder" }, resolution-id = "build:dep-b:wheel:bootstrap:[BUILD-ID]" },
            { name = "helper", version = "2.0.0", source = { registry = "[TEMP_DIR]/links" } },
        ]

        [[resolution]]
        id = "build:dep-b:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "dep-b"
        roots = [
            { name = "builder", version = "1.0.0", source = { directory = "[TEMP_DIR]/builder" }, resolution-id = "build:dep-b:wheel:build:[BUILD-ID]" },
            { name = "helper", version = "2.0.0", source = { registry = "[TEMP_DIR]/links" } },
        ]
        "#);
    });
    assert_eq!(lock.matches("[[package]]\nname = \"builder\"").count(), 3);
    assert!(
        lock.contains(r#"resolution-id = "build:dep-b:wheel:bootstrap:"#),
        "{lock}"
    );
    assert!(
        lock.contains(r#"resolution-id = "build:dep-b:wheel:build:"#),
        "{lock}"
    );
    let scoped_builders = lock
        .split("[[package]]")
        .filter(|package| {
            package.starts_with("\nname = \"builder\"") && package.contains("resolution-id = ")
        })
        .collect::<Vec<_>>();
    assert_eq!(scoped_builders.len(), 2, "{lock}");
    assert!(
        scoped_builders
            .iter()
            .all(|package| package.contains(r#"{ name = "nested-backend""#)),
        "{lock}"
    );
    assert!(!lock.contains("build-dependency-packages"), "{lock}");

    let missing_backend_url =
        Url::from_file_path(context.temp_dir.child("missing-backend.whl").path())
            .expect("valid file URL");
    builder_dir.child("pyproject.toml").write_str(&format!(
        r#"
        [project]
        name = "builder"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = ["helper>=1"]

        [build-system]
        requires = ["missing-backend @ {missing_backend_url}"]
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
    ))?;

    uv_snapshot!(context.filters(), context
        .sync()
        .arg("--no-cache")
        .arg("--no-index")
        .arg("--frozen")
        .arg("--no-default-groups")
        .arg("--group")
        .arg("b")
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + dep-b==0.1.0 (from file://[TEMP_DIR]/dep-b)
    ");

    Ok(())
}

/// Verify that every captured build graph gets a named resolution context,
/// even when its transitive edges do not conflict with another graph.
#[test]
fn lock_build_dependencies_records_simple_build_resolution_context() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    write_wheel_with_requires(
        &links_dir.child("builder-1.0.0-py3-none-any.whl"),
        "builder",
        "1.0.0",
        &["leaf==1.0.0"],
    )?;
    write_wheel(
        &links_dir.child("leaf-1.0.0-py3-none-any.whl"),
        "leaf",
        "1.0.0",
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
        requires = ["builder==1.0.0"]
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
    return []

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    if version("leaf") != "1.0.0":
        raise RuntimeError("unexpected build leaf version: " + version("leaf"))
    filename = "dep-0.1.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("dep/__init__.py", "")
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
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(resolution_sections(&lock), @r#"
        [[resolution]]
        id = "build:dep:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "dep"
        roots = [
            { name = "builder", version = "1.0.0" },
        ]

        [[resolution]]
        id = "build:dep:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "dep"
        roots = [
            { name = "builder", version = "1.0.0" },
        ]
        "#);
    });
    let builder = package_section(&lock, "builder");
    assert!(builder.contains(r"dependencies = ["), "{builder}");
    assert!(builder.contains(r#"{ name = "leaf" }"#), "{builder}");
    assert!(!builder.contains("contexts"), "{builder}");
    assert!(!lock.contains("build-dependency-packages"), "{lock}");

    uv_snapshot!(context.filters(), context
        .sync()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--frozen")
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + dep==0.1.0 (from file://[TEMP_DIR]/dep)
    ");

    Ok(())
}

/// Verify that build resolution records retain the target reachability of
/// the source package whose isolated build environment they replay.
#[test]
fn lock_build_dependencies_build_resolution_target_reachability() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    write_wheel_with_requires(
        &links_dir.child("builder-1.0.0-py3-none-any.whl"),
        "builder",
        "1.0.0",
        &["helper>=1"],
    )?;
    write_wheel(
        &links_dir.child("helper-1.0.0-py3-none-any.whl"),
        "helper",
        "1.0.0",
    )?;
    write_wheel(
        &links_dir.child("helper-2.0.0-py3-none-any.whl"),
        "helper",
        "2.0.0",
    )?;

    for (name, helper_version) in [("dep-a", "1.0.0"), ("dep-b", "2.0.0")] {
        let dep_dir = context.temp_dir.child(name);
        dep_dir.create_dir_all()?;
        dep_dir.child("pyproject.toml").write_str(&format!(
            r#"
            [project]
            name = "{name}"
            version = "0.1.0"
            requires-python = ">=3.12"

            [build-system]
            requires = ["builder==1.0.0", "helper=={helper_version}"]
            backend-path = ["."]
            build-backend = "build_backend"
            "#
        ))?;
        dep_dir.child("build_backend.py").write_str(
            r"
def get_requires_for_build_wheel(config_settings=None):
    return []
",
        )?;
    }

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "dep-a ; sys_platform == 'linux'",
            "dep-b",
        ]

        [tool.uv.sources]
        dep-a = { path = "dep-a" }
        dep-b = { path = "dep-b" }
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
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(resolution_sections(&lock), @r#"
        [[resolution]]
        id = "build:dep-a:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "dep-a"
        target = { marker = "sys_platform == 'linux'" }
        roots = [
            { name = "builder", version = "1.0.0", source = { registry = "[TEMP_DIR]/links" } },
            { name = "helper", version = "1.0.0", source = { registry = "[TEMP_DIR]/links" } },
        ]

        [[resolution]]
        id = "build:dep-a:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "dep-a"
        target = { marker = "sys_platform == 'linux'" }
        roots = [
            { name = "builder", version = "1.0.0", source = { registry = "[TEMP_DIR]/links" } },
            { name = "helper", version = "1.0.0", source = { registry = "[TEMP_DIR]/links" } },
        ]

        [[resolution]]
        id = "build:dep-b:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "dep-b"
        roots = [
            { name = "builder", version = "1.0.0", source = { registry = "[TEMP_DIR]/links" }, resolution-id = "build:dep-b:wheel:bootstrap:[BUILD-ID]" },
            { name = "helper", version = "2.0.0", source = { registry = "[TEMP_DIR]/links" } },
        ]

        [[resolution]]
        id = "build:dep-b:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "dep-b"
        roots = [
            { name = "builder", version = "1.0.0", source = { registry = "[TEMP_DIR]/links" }, resolution-id = "build:dep-b:wheel:build:[BUILD-ID]" },
            { name = "helper", version = "2.0.0", source = { registry = "[TEMP_DIR]/links" } },
        ]
        "#);
    });

    Ok(())
}

/// Verify that isolated build dependency edges do not affect the runtime
/// dependency closure when a package is shared between both environments.
#[test]
fn lock_build_dependencies_do_not_leak_edges_into_runtime_graph() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    write_wheel_with_requires(
        &links_dir.child("builder-1.0.0-py3-none-any.whl"),
        "builder",
        "1.0.0",
        &["leaf>=1"],
    )?;
    write_wheel(
        &links_dir.child("leaf-1.0.0-py3-none-any.whl"),
        "leaf",
        "1.0.0",
    )?;
    write_wheel(
        &links_dir.child("leaf-2.0.0-py3-none-any.whl"),
        "leaf",
        "2.0.0",
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
        r#"
from importlib.metadata import version
from pathlib import Path
from zipfile import ZipFile

def get_requires_for_build_wheel(config_settings=None):
    return []

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    if version("leaf") != "1.0.0":
        raise RuntimeError("unexpected build leaf version: " + version("leaf"))
    filename = "dep-0.1.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("dep/__init__.py", "")
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
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(resolution_sections(&lock), @r#"
        [[resolution]]
        id = "build:dep:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "dep"
        roots = [
            { name = "builder", version = "1.0.0", source = { registry = "[TEMP_DIR]/links" }, resolution-id = "build:dep:wheel:bootstrap:[BUILD-ID]" },
            { name = "leaf", version = "1.0.0", source = { registry = "[TEMP_DIR]/links" } },
        ]

        [[resolution]]
        id = "build:dep:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "dep"
        roots = [
            { name = "builder", version = "1.0.0", source = { registry = "[TEMP_DIR]/links" }, resolution-id = "build:dep:wheel:build:[BUILD-ID]" },
            { name = "leaf", version = "1.0.0", source = { registry = "[TEMP_DIR]/links" } },
        ]
        "#);
    });
    let builder = package_section(&lock, "builder");
    assert!(
        builder.contains(r#"{ name = "leaf", version = "2.0.0""#),
        "{builder}"
    );
    assert!(
        lock.contains(r#"resolution-id = "build:dep:wheel:build:"#),
        "{lock}"
    );
    assert!(!builder.contains("contexts"), "{builder}");
    assert!(!lock.contains("build-dependency-packages"), "{lock}");

    uv_snapshot!(context.filters(), context
        .workspace_metadata()
        .arg("--frozen")
        .arg("--preview-features")
        .arg("lock-build-dependencies,workspace-metadata"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "schema": {
        "version": "preview"
      },
      "workspace_root": "[TEMP_DIR]/",
      "workspace": {
        "path": "[TEMP_DIR]/",
        "id": "workspace+[TEMP_DIR]/"
      },
      "requires_python": ">=3.12",
      "conflicts": {
        "sets": []
      },
      "members": [
        {
          "name": "project",
          "path": "[TEMP_DIR]/",
          "id": "project==0.1.0@virtual+[TEMP_DIR]/"
        }
      ],
      "resolution": {
        "builder==1.0.0@registry+[TEMP_DIR]/links": {
          "name": "builder",
          "version": "1.0.0",
          "source": {
            "registry": {
              "path": "[TEMP_DIR]/links"
            }
          },
          "kind": "package",
          "dependencies": [
            {
              "id": "leaf==2.0.0@registry+[TEMP_DIR]/links"
            }
          ],
          "wheels": [
            {
              "path": "[TEMP_DIR]/links/builder-1.0.0-py3-none-any.whl",
              "filename": "builder-1.0.0-py3-none-any.whl"
            }
          ]
        },
        "dep==0.1.0@directory+[TEMP_DIR]/dep": {
          "name": "dep",
          "version": "0.1.0",
          "source": {
            "directory": "[TEMP_DIR]/dep"
          },
          "kind": "package",
          "dependencies": []
        },
        "leaf==2.0.0@registry+[TEMP_DIR]/links": {
          "name": "leaf",
          "version": "2.0.0",
          "source": {
            "registry": {
              "path": "[TEMP_DIR]/links"
            }
          },
          "kind": "package",
          "dependencies": [],
          "wheels": [
            {
              "path": "[TEMP_DIR]/links/leaf-2.0.0-py3-none-any.whl",
              "filename": "leaf-2.0.0-py3-none-any.whl"
            }
          ]
        },
        "project==0.1.0@virtual+[TEMP_DIR]/": {
          "name": "project",
          "version": "0.1.0",
          "source": {
            "virtual": "[TEMP_DIR]/"
          },
          "kind": "package",
          "dependencies": [
            {
              "id": "builder==1.0.0@registry+[TEMP_DIR]/links"
            },
            {
              "id": "dep==0.1.0@directory+[TEMP_DIR]/dep"
            }
          ]
        },
        "workspace+[TEMP_DIR]/": {
          "kind": "workspace",
          "path": "[TEMP_DIR]/",
          "dependencies": []
        }
      }
    }

    ----- stderr -----
    "#);

    uv_snapshot!(context.filters(), context
        .sync()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--frozen")
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + builder==1.0.0
     + dep==0.1.0 (from file://[TEMP_DIR]/dep)
     + leaf==2.0.0
    ");

    Ok(())
}

/// Verify that `extra-build-dependencies` participate in lock freshness checks.
#[test]
fn lock_build_dependencies_extra_build_dependencies_invalidate() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        dynamic = ["version"]
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.setuptools.dynamic]
        version = {attr = "dep.__version__"}
        "#,
    )?;
    dep_dir.child("dep").create_dir_all()?;
    dep_dir
        .child("dep/__init__.py")
        .write_str("__version__ = '0.1.0'")?;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
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

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("extra-build-dependencies,lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep"]

        [tool.uv.sources]
        dep = { path = "dep" }

        [tool.uv.extra-build-dependencies]
        dep = ["iniconfig"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("extra-build-dependencies,lock-build-dependencies")
        .arg("--locked"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided.

    hint: To update the lockfile, run `uv lock`.
    ");

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("extra-build-dependencies,lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Added calver v2022.6.26
    Added hatch-vcs v0.4.0
    Added hatchling v1.22.4
    Added iniconfig v2.0.0
    Added packaging v24.0
    Added pathspec v0.12.1
    Added pluggy v1.4.0
    Added setuptools-scm v8.0.4
    Added trove-classifiers v2024.3.3
    Added typing-extensions v4.10.0
    ");

    let lock = context.read("uv.lock");
    let dep = package_section(&lock, "dep");
    assert!(dep.contains(r#"{ name = "iniconfig" }"#), "{dep}");

    Ok(())
}

/// Verify that configured extra build dependencies invalidate immutable
/// source-distribution locks when the configuration is removed.
#[test]
fn lock_build_dependencies_extra_build_dependencies_invalidate_find_links() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    let source_dist = links_dir.child("locked_extra_dep-0.1.0.zip");
    let mut zip = ZipFileWriter::new(Vec::new());
    let entry = ZipEntryBuilder::new(
        "locked_extra_dep-0.1.0/pyproject.toml".into(),
        Compression::Stored,
    );
    block_on(zip.write_entry_whole(
        entry,
        br#"
        [project]
        name = "locked-extra-dep"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    ))?;
    fs_err::write(source_dist.path(), block_on(zip.close())?)?;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["locked-extra-dep==0.1.0"]

        [tool.uv.extra-build-dependencies]
        locked-extra-dep = ["wheel"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--preview-features")
        .arg("extra-build-dependencies,lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["locked-extra-dep==0.1.0"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--preview-features")
        .arg("extra-build-dependencies,lock-build-dependencies")
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

/// Verify that universal build dependency locks use the project's Python
/// range, not just the interpreter used to generate the lock.
#[test]
fn lock_build_dependencies_use_project_python_range() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let builder_dir = context.temp_dir.child("builder");
    builder_dir.create_dir_all()?;
    builder_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "builder"
        version = "0.1.0"
        requires-python = ">=3.12"
        "#,
    )?;
    let builder_url = Url::from_directory_path(builder_dir.path()).unwrap();

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(&format!(
        r#"
        [project]
        name = "dep"
        dynamic = ["version"]
        requires-python = ">=3.8"

        [build-system]
        requires = ["setuptools>=42", "builder @ {builder_url}"]
        build-backend = "setuptools.build_meta"

        [tool.setuptools.dynamic]
        version = {{attr = "dep.__version__"}}
        "#
    ))?;
    dep_dir.child("dep").create_dir_all()?;
    dep_dir
        .child("dep/__init__.py")
        .write_str("__version__ = '0.1.0'")?;

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.8"
        dependencies = ["dep"]

        [tool.uv.sources]
        dep = { path = "dep" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    error: Failed to resolve requirements from `build-system.requires`
      Caused by: No solution found when resolving: `setuptools>=42`, `builder @ file://[TEMP_DIR]/builder`
      Caused by: Because the requested Python version (>=3.8) does not satisfy Python>=3.12 and builder==0.1.0 depends on Python>=3.12, we can conclude that builder==0.1.0 cannot be used.
        And because only builder==0.1.0 is available and you require builder, we can conclude that your requirements are unsatisfiable.

    hint: The `requires-python` value (>=3.8) includes Python versions that are not supported by your dependencies (e.g., builder==0.1.0 only supports >=3.12). Consider using a more restrictive `requires-python` value (like >=3.12).
    ");

    Ok(())
}

/// Verify build dependency solves are restricted to a conditional source's Python region.
#[test]
fn lock_build_dependencies_use_conditional_source_python_range() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let builder_dir = context.temp_dir.child("builder");
    builder_dir.create_dir_all()?;
    builder_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "builder"
        version = "0.1.0"
        requires-python = ">=3.12"
        "#,
    )?;
    let builder_url = Url::from_directory_path(builder_dir.path()).unwrap();

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(&format!(
        r#"
        [project]
        name = "dep"
        dynamic = ["version"]
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42", "builder @ {builder_url}"]
        build-backend = "setuptools.build_meta"

        [tool.setuptools.dynamic]
        version = {{attr = "dep.__version__"}}
        "#
    ))?;
    dep_dir.child("dep").create_dir_all()?;
    dep_dir
        .child("dep/__init__.py")
        .write_str("__version__ = '0.1.0'")?;

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.8"
        dependencies = ["dep ; python_version >= '3.12'"]

        [tool.uv.sources]
        dep = { path = "dep" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    let resolutions = resolution_sections(&lock);
    assert!(
        resolutions.split("[[resolution]]").any(|resolution| {
            resolution.contains("\nname = \"dep\"\n")
                && resolution.contains(r#"target = { marker = "python_full_version >= '3.12'" }"#)
                && resolution.contains(
                    r#"{ name = "builder", version = "0.1.0", marker = "python_full_version >= '3.12'" }"#
                )
        }),
        "{lock}"
    );

    Ok(())
}

/// Verify that runtime supported environments do not prune executor marker
/// branches from universal build dependency locks.
#[test]
fn lock_build_dependencies_do_not_use_runtime_supported_environments_for_executor_markers()
-> Result<()> {
    let context = uv_test::test_context!("3.12");

    let builder_dir = context.temp_dir.child("builder");
    builder_dir.create_dir_all()?;
    builder_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "builder"
        version = "0.1.0"
        requires-python = ">=3.12"
        "#,
    )?;
    let builder_url = Url::from_directory_path(builder_dir.path()).unwrap();

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(&format!(
        r#"
        [project]
        name = "dep"
        dynamic = ["version"]
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42", "builder @ {builder_url}; sys_platform == 'win32'"]
        build-backend = "setuptools.build_meta"

        [tool.setuptools.dynamic]
        version = {{attr = "dep.__version__"}}
        "#
    ))?;
    dep_dir.child("dep").create_dir_all()?;
    dep_dir
        .child("dep/__init__.py")
        .write_str("__version__ = '0.1.0'")?;

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep"]

        [tool.uv]
        environments = ["sys_platform == 'linux'"]

        [tool.uv.sources]
        dep = { path = "dep" }
        "#,
    )?;

    context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();

    let lock = context.read("uv.lock");
    assert!(lock.contains("[[package]]\nname = \"builder\""), "{lock}");
    assert!(
        package_section(&lock, "dep").contains(
            r#"{ name = "builder", version = "0.1.0", marker = "sys_platform == 'win32'" }"#
        ),
        "{lock}"
    );

    Ok(())
}

/// Verify that PEP 508 extras in build requirements preserve their extra-only
/// dependency edges in the locked build environment.
#[test]
fn lock_build_dependencies_preserves_pep508_extras() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let leaf_dir = context.temp_dir.child("leaf");
    leaf_dir.create_dir_all()?;
    leaf_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "leaf"
        version = "0.1.0"
        requires-python = ">=3.12"
        "#,
    )?;
    let leaf_url = Url::from_directory_path(leaf_dir.path()).unwrap();

    let carrier_dir = context.temp_dir.child("carrier");
    carrier_dir.create_dir_all()?;
    let carrier_url = Url::from_directory_path(carrier_dir.path()).unwrap();
    carrier_dir.child("pyproject.toml").write_str(&format!(
        r#"
        [project]
        name = "carrier"
        version = "0.1.0"
        requires-python = ">=3.12"

        [project.optional-dependencies]
        extra = ["leaf @ {leaf_url}"]
        "#,
    ))?;

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(&format!(
        r#"
        [project]
        name = "dep"
        dynamic = ["version"]
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42", "carrier[extra] @ {carrier_url}"]
        build-backend = "setuptools.build_meta"

        [tool.setuptools.dynamic]
        version = {{attr = "dep.__version__"}}
        "#,
    ))?;
    dep_dir.child("dep").create_dir_all()?;
    dep_dir
        .child("dep/__init__.py")
        .write_str("__version__ = '0.1.0'")?;

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep", "carrier"]

        [tool.uv.sources]
        dep = { path = "dep" }
        carrier = { path = "carrier" }
        leaf = { path = "leaf" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock().arg("--preview-features").arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    let dep = package_section(&lock, "dep");
    assert!(dep.contains(r#"name = "carrier""#), "{dep}");
    assert!(dep.contains(r#"extra = ["extra"]"#), "{dep}");

    let carrier = package_section(&lock, "carrier");
    assert!(carrier.contains("[package.optional-dependencies]"));
    assert!(carrier.contains(r#"{ name = "leaf" }"#), "{carrier}");
    assert!(
        carrier.contains(r#"provides-extras = ["extra"]"#),
        "{carrier}"
    );
    assert!(lock.contains(r#"name = "leaf""#));

    Ok(())
}

/// Verify that enabling build-dependency locking upgrades an otherwise
/// up-to-date revision-3 lock instead of returning it unchanged.
#[test]
fn lock_build_dependencies_relocks_revision_3() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        dynamic = ["version"]
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.setuptools.dynamic]
        version = {attr = "dep.__version__"}
        "#,
    )?;
    dep_dir.child("dep").create_dir_all()?;
    dep_dir
        .child("dep/__init__.py")
        .write_str("__version__ = '0.1.0'")?;

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

    uv_snapshot!(context.filters(), context.lock(), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");
    assert!(context.read("uv.lock").contains("revision = 3"));

    uv_snapshot!(context.filters(), context.lock().arg("--preview-features").arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Added flit-core v3.9.0
    Added setuptools v69.2.0
    Added wheel v0.43.0
    ");

    let lock = context.read("uv.lock");
    assert!(lock.contains("revision = 4"));
    assert!(package_section(&lock, "dep").contains("build-dependencies = ["));

    Ok(())
}

/// Verify that the shared default-backend cache records the default build
/// resolution for every source package that reuses it.
#[test]
fn lock_build_dependencies_default_backend_cache_records_each_package() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let dep_a_dir = context.temp_dir.child("dep-a");
    dep_a_dir.create_dir_all()?;
    dep_a_dir
        .child("setup.py")
        .write_str("from setuptools import setup\nsetup(name='dep-a', version='0.1.0')\n")?;

    let dep_b_dir = context.temp_dir.child("dep-b");
    dep_b_dir.create_dir_all()?;
    dep_b_dir
        .child("setup.py")
        .write_str("from setuptools import setup\nsetup(name='dep-b', version='0.1.0')\n")?;

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep-a", "dep-b"]

        [tool.uv.sources]
        dep-a = { path = "dep-a" }
        dep-b = { path = "dep-b" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock().arg("--preview-features").arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    assert!(package_section(&lock, "dep-a").contains("build-dependencies = ["));
    assert!(package_section(&lock, "dep-b").contains("build-dependencies = ["));

    Ok(())
}

/// Verify that frozen default-backend builds replay each package's captured hook requirements.
#[test]
fn lock_build_dependencies_default_backend_cache_replays_each_package() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;

    for (name, helper) in [("dep-a", "helper-a"), ("dep-b", "helper-b")] {
        write_wheel(
            &links_dir.child(format!(
                "{}-0.1.0-py3-none-any.whl",
                helper.replace('-', "_")
            )),
            helper,
            "0.1.0",
        )?;

        let dep_dir = context.temp_dir.child(name);
        dep_dir.create_dir_all()?;
        dep_dir.child("setup.py").write_str(&format!(
            r#"
import os
import sys
from importlib.metadata import version
from setuptools import setup

if "bdist_wheel" in sys.argv and version("{helper}") != "0.1.0":
    raise RuntimeError("{name} expected {helper}")

setup(
    name="{name}",
    version="0.1.0",
    setup_requires=["{helper}==0.1.0"] if os.environ["UV_TEST_CAPTURE_SETUP_REQUIRES"] == "1" else [],
)
"#
        ))?;
    }

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep-a", "dep-b"]

        [tool.uv.sources]
        dep-a = { path = "dep-a" }
        dep-b = { path = "dep-b" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .env("UV_TEST_CAPTURE_SETUP_REQUIRES", "1"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    for (name, helper) in [("dep-a", "helper-a"), ("dep-b", "helper-b")] {
        let dep = package_section(&lock, name);
        assert!(
            dep.contains(&format!(r#"{{ name = "{helper}", version = "0.1.0" }}"#)),
            "{dep}"
        );
    }

    context
        .sync()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--frozen")
        .arg("--reinstall")
        .arg("--no-cache")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .env("UV_TEST_CAPTURE_SETUP_REQUIRES", "0")
        .assert()
        .success();

    Ok(())
}

/// Verify that target platform markers do not constrain cached implicit
/// backend build roots, which must be usable by the build host.
#[test]
fn lock_build_dependencies_default_backend_cache_ignores_target_platform() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let (host_marker, foreign_marker) = if cfg!(target_os = "macos") {
        ("sys_platform == 'darwin'", "sys_platform == 'linux'")
    } else if cfg!(target_os = "windows") {
        ("sys_platform == 'win32'", "sys_platform == 'linux'")
    } else {
        ("sys_platform == 'linux'", "sys_platform == 'darwin'")
    };

    for name in ["dep-a", "dep-b"] {
        let module_name = name.replace('-', "_");
        let archive_name = format!("{name}-0.1.0.zip");
        let source_dist = context.temp_dir.child(&archive_name);
        let mut zip = ZipFileWriter::new(Vec::new());
        let entry = ZipEntryBuilder::new(
            format!("{name}-0.1.0/pyproject.toml").into(),
            Compression::Stored,
        );
        let pyproject_toml = format!(
            r#"
                [project]
                name = "{name}"
                dynamic = ["version"]
                requires-python = ">=3.12"

                [tool.setuptools.dynamic]
                version = {{attr = "{module_name}.__version__"}}
                "#
        );
        block_on(zip.write_entry_whole(entry, pyproject_toml.as_bytes()))?;
        let entry = ZipEntryBuilder::new(
            format!("{name}-0.1.0/{module_name}/__init__.py").into(),
            Compression::Stored,
        );
        block_on(zip.write_entry_whole(entry, b"__version__ = '0.1.0'"))?;
        fs_err::write(source_dist.path(), block_on(zip.close())?)?;
    }

    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(&format!(
            r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "dep-a ; {host_marker}",
            "dep-b ; {foreign_marker}",
        ]

        [tool.uv.sources]
        dep-a = {{ path = "dep-a-0.1.0.zip" }}
        dep-b = {{ path = "dep-b-0.1.0.zip" }}
        "#
        ))?;

    context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();

    let lock = context.read("uv.lock");
    let dep_a = package_section(&lock, "dep-a");
    assert!(dep_a.contains(r#"{ name = "setuptools","#), "{dep_a}");
    assert!(!dep_a.contains("marker ="), "{dep_a}");
    let dep_b = package_section(&lock, "dep-b");
    assert!(dep_b.contains(r#"{ name = "setuptools","#), "{dep_b}");
    assert!(!dep_b.contains("marker ="), "{dep_b}");

    Ok(())
}

/// Verify that a static source package without an explicit build system still
/// records PEP 517's implicit setuptools build requirement.
#[test]
fn lock_build_dependencies_static_directory_implicit_default_backend() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        version = "0.1.0"
        requires-python = ">=3.12"
        "#,
    )?;
    dep_dir.child("dep").create_dir_all()?;
    dep_dir.child("dep/__init__.py").touch()?;

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

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    let dep = package_section(&lock, "dep");
    assert!(dep.contains("build-dependencies = ["), "{dep}");
    assert!(dep.contains(r#"{ name = "setuptools", version = "69.2.0" }"#));

    Ok(())
}

/// Verify that an explicit empty build-system requirement set is not treated
/// as the implicit default backend environment.
#[test]
fn lock_build_dependencies_explicit_empty_build_requires_invalidate() -> Result<()> {
    let context = uv_test::test_context!("3.12");

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
        dependencies = ["dep"]

        [tool.uv.sources]
        dep = { path = "dep" }
        "#,
    )?;

    context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();

    dep_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        version = "0.1.0"
        requires-python = ">=3.12"
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
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

    let lock = context.read("uv.lock");
    let dep = package_section(&lock, "dep");
    assert!(dep.contains("build-requires = []"), "{dep}");

    Ok(())
}

/// Verify that static directory dependencies lower build requirements through
/// their own source configuration before locking the build environment.
#[test]
fn lock_build_dependencies_static_directory_lowers_build_sources() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    write_wheel(
        &context
            .temp_dir
            .child("private_builder-0.1.0-py3-none-any.whl"),
        "private-builder",
        "0.1.0",
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
        requires = ["private-builder"]
        build-backend = "private_builder"

        [tool.uv.sources]
        private-builder = { path = "../private_builder-0.1.0-py3-none-any.whl" }
        "#,
    )?;

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

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--no-index"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    let dep = package_section(&lock, "dep");
    assert!(
        dep.contains(r#"{ name = "private-builder", version = "0.1.0" }"#),
        "{dep}"
    );
    let private_builder = package_section(&lock, "private-builder");
    assert!(
        private_builder.contains(r#"hash = "sha256:"#),
        "{private_builder}"
    );

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--no-index")
        .arg("--locked"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    Ok(())
}

/// Verify that direct URL source distributions used as build requirements are
/// serialized with a generated hash.
#[tokio::test]
async fn lock_build_dependencies_hashes_direct_url_source_build_requirements() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let source_dist = context.temp_dir.child("builder-0.1.0.zip");
    let mut zip = ZipFileWriter::new(Vec::new());
    let entry = ZipEntryBuilder::new("builder-0.1.0/pyproject.toml".into(), Compression::Stored);
    zip.write_entry_whole(
        entry,
        br#"
        [project]
        name = "builder"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = []
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
    )
    .await?;
    let entry = ZipEntryBuilder::new("builder-0.1.0/build_backend.py".into(), Compression::Stored);
    zip.write_entry_whole(
        entry,
        br#"
from pathlib import Path
from zipfile import ZipFile

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    filename = "builder-0.1.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("builder/__init__.py", "")
        wheel.writestr(
            "builder-0.1.0.dist-info/METADATA",
            "Metadata-Version: 2.3\nName: builder\nVersion: 0.1.0\n",
        )
        wheel.writestr(
            "builder-0.1.0.dist-info/WHEEL",
            "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
        )
        wheel.writestr("builder-0.1.0.dist-info/RECORD", "")
    return filename
"#,
    )
    .await?;
    fs_err::write(source_dist.path(), zip.close().await?)?;

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/builder-0.1.0.zip"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(fs_err::read(source_dist.path())?))
        .mount(&server)
        .await;

    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(&format!(
            r#"
            [project]
            name = "project"
            version = "0.1.0"
            requires-python = ">=3.12"

            [build-system]
            requires = ["builder @ {}/builder-0.1.0.zip"]
            backend-path = ["."]
            build-backend = "build_backend"
            "#,
            server.uri()
        ))?;
    context.temp_dir.child("build_backend.py").write_str("")?;

    context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--no-index")
        .assert()
        .success();

    let lock = context.read("uv.lock");
    let mut filters = context.filters();
    filters.push((r"sha256:[0-9a-f]{64}", "sha256:[HASH]"));
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(package_section(&lock, "builder"), @r#"
        [[package]]
        name = "builder"
        version = "0.1.0"
        source = { url = "http://[LOCALHOST]/builder-0.1.0.zip" }
        build-only = true
        build-dependencies = []
        sdist = { hash = "sha256:[HASH]" }

        [package.metadata]
        build-system = { build-backend = "build_backend", backend-path = ["."] }
        build-requires = []
        "#);
    });

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--no-index")
        .arg("--locked"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    ");

    Ok(())
}

/// Verify that an audit relock preserves explicitly requested and existing build locks.
#[tokio::test]
async fn lock_build_dependencies_audit_preserves_build_lock() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    write_wheel(
        &links_dir.child("builder-0.1.0-py3-none-any.whl"),
        "builder",
        "0.1.0",
    )?;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    let write_project = |version: &str| -> Result<()> {
        pyproject_toml.write_str(&format!(
            r#"
            [project]
            name = "project"
            version = "{version}"
            requires-python = ">=3.12"

            [build-system]
            requires = ["builder"]
            backend-path = ["."]
            build-backend = "build_backend"
            "#
        ))?;
        Ok(())
    };
    write_project("0.1.0")?;
    context.temp_dir.child("build_backend.py").write_str("")?;

    let server = MockServer::start().await;

    context
        .audit()
        .arg("--preview-features")
        .arg("audit,lock-build-dependencies")
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--service-url")
        .arg(server.uri())
        .assert()
        .success();

    let lock = context.read("uv.lock");
    assert!(lock.contains("revision = 4"), "{lock}");
    assert!(lock.contains("[[resolution]]"), "{lock}");
    assert!(lock.contains("name = \"builder\""), "{lock}");

    write_project("0.2.0")?;
    context
        .audit()
        .arg("--preview-features")
        .arg("audit")
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--service-url")
        .arg(server.uri())
        .assert()
        .success();

    let lock = context.read("uv.lock");
    assert!(lock.contains("revision = 4"), "{lock}");
    assert!(lock.contains("[[resolution]]"), "{lock}");
    assert!(lock.contains("name = \"builder\""), "{lock}");

    Ok(())
}

/// Verify that changing only a static dependency's lowered build source
/// invalidates its locked build environment.
#[test]
fn lock_build_dependencies_static_directory_lowered_sources_invalidate() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    for version in ["0.1.0", "0.2.0"] {
        write_wheel(
            &context
                .temp_dir
                .child(format!("private_builder-{version}-py3-none-any.whl")),
            "private-builder",
            version,
        )?;
    }

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    let write_dep = |version: &str| -> Result<()> {
        dep_dir.child("pyproject.toml").write_str(&format!(
            r#"
            [project]
            name = "dep"
            version = "0.1.0"
            requires-python = ">=3.12"

            [build-system]
            requires = ["private-builder"]
            build-backend = "private_builder"

            [tool.uv.sources]
            private-builder = {{ path = "../private_builder-{version}-py3-none-any.whl" }}
            "#
        ))?;
        Ok(())
    };
    write_dep("0.1.0")?;

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

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--no-index"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");
    assert!(
        package_section(&context.read("uv.lock"), "dep")
            .contains(r#"{ name = "private-builder", version = "0.1.0" }"#)
    );

    write_dep("0.2.0")?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--no-index")
        .arg("--locked"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided.

    hint: To update the lockfile, run `uv lock`.
    ");

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--no-index"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Updated private-builder v0.1.0 -> v0.2.0
    ");
    assert!(
        package_section(&context.read("uv.lock"), "dep")
            .contains(r#"{ name = "private-builder", version = "0.2.0" }"#)
    );

    Ok(())
}

/// Verify that static Git dependencies lower their declared build requirements
/// through their own source configuration before locking the build environment.
#[test]
#[cfg(feature = "test-git")]
fn lock_build_dependencies_static_git() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    write_wheel(
        &dep_dir.child("private_builder-0.1.0-py3-none-any.whl"),
        "private-builder",
        "0.1.0",
    )?;
    dep_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["private-builder"]
        build-backend = "private_builder"

        [tool.uv.sources]
        private-builder = { path = "private_builder-0.1.0-py3-none-any.whl" }
        "#,
    )?;
    Command::new("git")
        .arg("init")
        .arg("--quiet")
        .arg(dep_dir.path())
        .assert()
        .success();
    Command::new("git")
        .args(["-C", dep_dir.path().to_str().expect("UTF-8 temp path")])
        .args(["config", "user.name", "uv-test"])
        .assert()
        .success();
    Command::new("git")
        .args(["-C", dep_dir.path().to_str().expect("UTF-8 temp path")])
        .args(["config", "user.email", "uv-test@example.com"])
        .assert()
        .success();
    Command::new("git")
        .args(["-C", dep_dir.path().to_str().expect("UTF-8 temp path")])
        .args(["add", "."])
        .assert()
        .success();
    Command::new("git")
        .args(["-C", dep_dir.path().to_str().expect("UTF-8 temp path")])
        .args(["commit", "--quiet", "-m", "initial"])
        .assert()
        .success();

    let dep_url = Url::from_directory_path(dep_dir.path()).expect("valid file URL");
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(&format!(
            r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep @ git+{dep_url}"]
        "#
        ))?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--no-index"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    let dep = package_section(&lock, "dep");
    assert!(dep.contains("build-dependencies = ["), "{dep}");
    assert!(
        dep.contains(r#"{ name = "private-builder", version = "0.1.0" }"#),
        "{dep}"
    );

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--no-index")
        .arg("--no-sources")
        .arg("--locked"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    error: Failed to resolve requirements from `build-system.requires`
      Caused by: No solution found when resolving: `private-builder`
      Caused by: Because private-builder was not found in the provided package locations and you require private-builder, we can conclude that your requirements are unsatisfiable.

    hint: Packages were unavailable because index lookups were disabled and no additional package locations were provided (try: `--find-links <uri>`)
    ");

    Ok(())
}

/// Verify that static archives stored in Git capture dependencies returned by
/// their backend hook before the build resolution is locked.
#[test]
#[cfg(feature = "test-git")]
fn lock_build_dependencies_static_git_archive_captures_hook_requirements() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    write_wheel(
        &links_dir.child("helper-0.1.0-py3-none-any.whl"),
        "helper",
        "0.1.0",
    )?;

    let repo_dir = &context.temp_dir;
    repo_dir.child("archives").create_dir_all()?;
    let source_dist = repo_dir.child("archives/dep-0.1.0.zip");
    let mut zip = ZipFileWriter::new(Vec::new());
    let entry = ZipEntryBuilder::new("dep-0.1.0/pyproject.toml".into(), Compression::Stored);
    block_on(zip.write_entry_whole(
        entry,
        br#"
        [project]
        name = "dep"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = []
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
    ))?;
    let entry = ZipEntryBuilder::new("dep-0.1.0/build_backend.py".into(), Compression::Stored);
    block_on(zip.write_entry_whole(
        entry,
        br#"
def get_requires_for_build_wheel(config_settings=None):
    return ["helper==0.1.0"]
        "#,
    ))?;
    fs_err::write(source_dist.path(), block_on(zip.close())?)?;

    Command::new("git")
        .arg("init")
        .arg("--quiet")
        .arg(repo_dir.path())
        .assert()
        .success();
    Command::new("git")
        .args(["-C", repo_dir.path().to_str().expect("UTF-8 temp path")])
        .args(["config", "user.name", "uv-test"])
        .assert()
        .success();
    Command::new("git")
        .args(["-C", repo_dir.path().to_str().expect("UTF-8 temp path")])
        .args(["config", "user.email", "uv-test@example.com"])
        .assert()
        .success();
    Command::new("git")
        .args(["-C", repo_dir.path().to_str().expect("UTF-8 temp path")])
        .args(["add", "."])
        .assert()
        .success();
    Command::new("git")
        .args(["-C", repo_dir.path().to_str().expect("UTF-8 temp path")])
        .args(["commit", "--quiet", "-m", "initial"])
        .assert()
        .success();

    let repo_url = Url::from_directory_path(repo_dir.path()).expect("valid file URL");
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(&format!(
            r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep @ git+{repo_url}#path=archives/dep-0.1.0.zip"]
        "#
        ))?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    let dep = package_section(&lock, "dep");
    assert!(
        dep.contains(r#"{ name = "helper", version = "0.1.0" }"#),
        "{dep}"
    );

    Ok(())
}

/// Verify that build dependencies are captured correctly when the resolver forks
/// due to platform-specific dependencies.
#[test]
fn lock_build_dependencies_fork() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    // Create a local dependency with dynamic version, forcing a build.
    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        dynamic = ["version"]
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.setuptools.dynamic]
        version = {attr = "dep.__version__"}
        "#,
    )?;
    dep_dir.child("dep").create_dir_all()?;
    dep_dir
        .child("dep/__init__.py")
        .write_str("__version__ = '0.1.0'")?;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "dep",
            "iniconfig>=1 ; sys_platform == 'linux'",
            "iniconfig>=2 ; sys_platform == 'win32'",
        ]

        [tool.uv.sources]
        dep = { path = "dep" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock().arg("--preview-features").arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    ");

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 2
        revision = 4
        requires-python = ">=3.12"
        resolution-markers = [
            "sys_platform == 'linux'",
            "sys_platform == 'win32'",
            "sys_platform != 'linux' and sys_platform != 'win32'",
        ]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[resolution]]
        id = "build:calver:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "calver"
        roots = [
            { name = "setuptools", version = "69.2.0" },
        ]

        [[resolution]]
        id = "build:calver:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "calver"
        roots = [
            { name = "setuptools", version = "69.2.0" },
            { name = "wheel", version = "0.43.0" },
        ]

        [[resolution]]
        id = "build:dep:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "dep"
        roots = [
            { name = "setuptools", version = "69.2.0" },
        ]

        [[resolution]]
        id = "build:dep:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "dep"
        roots = [
            { name = "setuptools", version = "69.2.0" },
            { name = "wheel", version = "0.43.0" },
        ]

        [[resolution]]
        id = "build:flit-core:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "flit-core"

        [[resolution]]
        id = "build:flit-core:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "flit-core"

        [[resolution]]
        id = "build:hatch-vcs:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "hatch-vcs"
        roots = [
            { name = "hatchling", version = "1.22.4" },
        ]

        [[resolution]]
        id = "build:hatch-vcs:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "hatch-vcs"
        roots = [
            { name = "hatchling", version = "1.22.4" },
        ]

        [[resolution]]
        id = "build:hatchling:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "hatchling"

        [[resolution]]
        id = "build:hatchling:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "hatchling"
        roots = [
            { name = "packaging", version = "24.0" },
            { name = "pathspec", version = "0.12.1" },
            { name = "pluggy", version = "1.4.0" },
            { name = "trove-classifiers", version = "2024.3.3" },
        ]

        [[resolution]]
        id = "build:iniconfig:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "iniconfig"
        roots = [
            { name = "hatch-vcs", version = "0.4.0" },
            { name = "hatchling", version = "1.22.4" },
        ]

        [[resolution]]
        id = "build:iniconfig:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "iniconfig"
        roots = [
            { name = "hatch-vcs", version = "0.4.0" },
            { name = "hatchling", version = "1.22.4" },
        ]

        [[resolution]]
        id = "build:packaging:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "packaging"
        roots = [
            { name = "flit-core", version = "3.9.0" },
        ]

        [[resolution]]
        id = "build:packaging:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "packaging"
        roots = [
            { name = "flit-core", version = "3.9.0" },
        ]

        [[resolution]]
        id = "build:pathspec:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "pathspec"
        roots = [
            { name = "flit-core", version = "3.9.0" },
        ]

        [[resolution]]
        id = "build:pathspec:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "pathspec"
        roots = [
            { name = "flit-core", version = "3.9.0" },
        ]

        [[resolution]]
        id = "build:pluggy:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "pluggy"
        roots = [
            { name = "setuptools", version = "69.2.0" },
            { name = "setuptools-scm", version = "8.0.4" },
            { name = "setuptools-scm", version = "8.0.4", extra = ["toml"] },
        ]

        [[resolution]]
        id = "build:pluggy:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "pluggy"
        roots = [
            { name = "setuptools", version = "69.2.0" },
            { name = "setuptools-scm", version = "8.0.4" },
            { name = "setuptools-scm", version = "8.0.4", extra = ["toml"] },
            { name = "wheel", version = "0.43.0" },
        ]

        [[resolution]]
        id = "build:setuptools-scm:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "setuptools-scm"
        roots = [
            { name = "setuptools", version = "69.2.0" },
        ]

        [[resolution]]
        id = "build:setuptools-scm:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "setuptools-scm"
        roots = [
            { name = "setuptools", version = "69.2.0" },
            { name = "wheel", version = "0.43.0" },
        ]

        [[resolution]]
        id = "build:setuptools:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "setuptools"

        [[resolution]]
        id = "build:setuptools:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "setuptools"
        roots = [
            { name = "wheel", version = "0.43.0" },
        ]

        [[resolution]]
        id = "build:trove-classifiers:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "trove-classifiers"
        roots = [
            { name = "calver", version = "2022.6.26" },
            { name = "setuptools", version = "69.2.0" },
        ]

        [[resolution]]
        id = "build:trove-classifiers:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "trove-classifiers"
        roots = [
            { name = "calver", version = "2022.6.26" },
            { name = "setuptools", version = "69.2.0" },
            { name = "wheel", version = "0.43.0" },
        ]

        [[resolution]]
        id = "build:typing-extensions:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "typing-extensions"
        roots = [
            { name = "flit-core", version = "3.9.0" },
        ]

        [[resolution]]
        id = "build:typing-extensions:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "typing-extensions"
        roots = [
            { name = "flit-core", version = "3.9.0" },
        ]

        [[resolution]]
        id = "build:wheel:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "wheel"
        roots = [
            { name = "flit-core", version = "3.9.0" },
        ]

        [[resolution]]
        id = "build:wheel:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "wheel"
        roots = [
            { name = "flit-core", version = "3.9.0" },
        ]

        [[package]]
        name = "calver"
        version = "2022.6.26"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        build-dependencies = [
            { name = "setuptools", version = "69.2.0" },
            { name = "wheel", version = "0.43.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/b5/00/96cbed7c019c49ee04b8a08357a981983db7698ae6de402e57097cefc9ad/calver-2022.6.26.tar.gz", hash = "sha256:e05493a3b17517ef1748fbe610da11f10485faa7c416b9d33fd4a52d74894f8b", size = 6670, upload-time = "2022-06-26T23:25:10.382Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/f7/39/e421c06f42ca00fa9cf8929c2466e58a837e8e97b8ab3ff4f4ff9a15e33e/calver-2022.6.26-py3-none-any.whl", hash = "sha256:a1d7fcdd67797afc52ee36ffb8c8adf6643173864306547bfd1380cbce6310a0", size = 7049, upload-time = "2022-06-26T23:25:07.692Z" },
        ]

        [[package]]
        name = "dep"
        source = { directory = "dep" }
        build-dependencies = [
            { name = "setuptools", version = "69.2.0" },
            { name = "wheel", version = "0.43.0" },
        ]

        [package.metadata]
        build-system = { build-backend = "setuptools.build_meta" }
        build-requires = [{ name = "setuptools", specifier = ">=42" }]

        [[package]]
        name = "flit-core"
        version = "3.9.0"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        build-dependencies = []
        sdist = { url = "https://files.pythonhosted.org/packages/c4/e6/c1ac50fe3eebb38a155155711e6e864e254ce4b6e17fe2429b4c4d5b9e80/flit_core-3.9.0.tar.gz", hash = "sha256:72ad266176c4a3fcfab5f2930d76896059851240570ce9a98733b658cb786eba", size = 41917, upload-time = "2023-05-14T14:48:51.809Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/38/45/618e84e49a6c51e5dd15565ec2fcd82ab273434f236b8f108f065ded517a/flit_core-3.9.0-py3-none-any.whl", hash = "sha256:7aada352fb0c7f5538c4fafeddf314d3a6a92ee8e2b1de70482329e42de70301", size = 63141, upload-time = "2023-05-14T14:48:49.24Z" },
        ]

        [[package]]
        name = "hatch-vcs"
        version = "0.4.0"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        dependencies = [
            { name = "hatchling" },
            { name = "setuptools-scm" },
        ]
        build-dependencies = [
            { name = "hatchling", version = "1.22.4" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/f5/c9/54bb4fa27b4e4a014ef3bb17710cdf692b3aa2cbc7953da885f1bf7e06ea/hatch_vcs-0.4.0.tar.gz", hash = "sha256:093810748fe01db0d451fabcf2c1ac2688caefd232d4ede967090b1c1b07d9f7", size = 10917, upload-time = "2023-11-06T06:24:57.228Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/82/0f/6cbd9976160bc334add63bc2e7a58b1433a31b34b7cda6c5de6dd983d9a7/hatch_vcs-0.4.0-py3-none-any.whl", hash = "sha256:b8a2b6bee54cf6f9fc93762db73890017ae59c9081d1038a41f16235ceaf8b2c", size = 8412, upload-time = "2023-11-06T06:24:55.389Z" },
        ]

        [[package]]
        name = "hatchling"
        version = "1.22.4"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        dependencies = [
            { name = "packaging" },
            { name = "pathspec" },
            { name = "pluggy" },
            { name = "trove-classifiers" },
        ]
        build-dependencies = [
            { name = "packaging", version = "24.0" },
            { name = "pathspec", version = "0.12.1" },
            { name = "pluggy", version = "1.4.0" },
            { name = "trove-classifiers", version = "2024.3.3" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/4f/2a/c34d71531d1e1c9a5029bb73eb3816285befd0fffd7c63ffa0544253dca8/hatchling-1.22.4.tar.gz", hash = "sha256:8a2dcec96d7fb848382ef5848e5ac43fdae641f35a08a3fab5116bd495f3416e", size = 62758, upload-time = "2024-03-24T02:00:59.122Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/49/63/2d56d6356f9f8b906aa68335cbf5b1b54c69873a2e271eda2ddba319c1ae/hatchling-1.22.4-py3-none-any.whl", hash = "sha256:f56da5bfc396af7b29daa3164851dd04991c994083f56cb054b5003675caecdc", size = 82032, upload-time = "2024-03-24T02:00:57.534Z" },
        ]

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        build-dependencies = [
            { name = "hatch-vcs", version = "0.4.0" },
            { name = "hatchling", version = "1.22.4" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646, upload-time = "2023-01-07T11:08:11.254Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892, upload-time = "2023-01-07T11:08:09.864Z" },
        ]

        [[package]]
        name = "packaging"
        version = "24.0"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        build-dependencies = [
            { name = "flit-core", version = "3.9.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/ee/b5/b43a27ac7472e1818c4bafd44430e69605baefe1f34440593e0332ec8b4d/packaging-24.0.tar.gz", hash = "sha256:eb82c5e3e56209074766e6885bb04b8c38a0c015d0a30036ebe7ece34c9989e9", size = 147882, upload-time = "2024-03-10T09:39:28.33Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/49/df/1fceb2f8900f8639e278b056416d49134fb8d84c5942ffaa01ad34782422/packaging-24.0-py3-none-any.whl", hash = "sha256:2ddfb553fdf02fb784c234c7ba6ccc288296ceabec964ad2eae3777778130bc5", size = 53488, upload-time = "2024-03-10T09:39:25.947Z" },
        ]

        [[package]]
        name = "pathspec"
        version = "0.12.1"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        build-dependencies = [
            { name = "flit-core", version = "3.9.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/ca/bc/f35b8446f4531a7cb215605d100cd88b7ac6f44ab3fc94870c120ab3adbf/pathspec-0.12.1.tar.gz", hash = "sha256:a482d51503a1ab33b1c67a6c3813a26953dbdc71c31dacaef9a838c4e29f5712", size = 51043, upload-time = "2023-12-10T22:30:45Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/cc/20/ff623b09d963f88bfde16306a54e12ee5ea43e9b597108672ff3a408aad6/pathspec-0.12.1-py3-none-any.whl", hash = "sha256:a0d503e138a4c123b27490a4f7beda6a01c6f288df0e4a8b79c7eb0dc7b4cc08", size = 31191, upload-time = "2023-12-10T22:30:43.14Z" },
        ]

        [[package]]
        name = "pluggy"
        version = "1.4.0"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        build-dependencies = [
            { name = "setuptools", version = "69.2.0" },
            { name = "setuptools-scm", version = "8.0.4" },
            { name = "setuptools-scm", version = "8.0.4", extra = ["toml"] },
            { name = "wheel", version = "0.43.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/54/c6/43f9d44d92aed815e781ca25ba8c174257e27253a94630d21be8725a2b59/pluggy-1.4.0.tar.gz", hash = "sha256:8c85c2876142a764e5b7548e7d9a0e0ddb46f5185161049a79b7e974454223be", size = 65812, upload-time = "2024-01-24T13:45:15.875Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/a5/5b/0cc789b59e8cc1bf288b38111d002d8c5917123194d45b29dcdac64723cc/pluggy-1.4.0-py3-none-any.whl", hash = "sha256:7db9f7b503d67d1c5b95f59773ebb58a8c1c288129a88665838012cfb07b8981", size = 20120, upload-time = "2024-01-24T13:45:14.227Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "dep" },
            { name = "iniconfig", marker = "sys_platform == 'linux' or sys_platform == 'win32'" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "dep", directory = "dep" },
            { name = "iniconfig", marker = "sys_platform == 'linux'", specifier = ">=1" },
            { name = "iniconfig", marker = "sys_platform == 'win32'", specifier = ">=2" },
        ]

        [[package]]
        name = "setuptools"
        version = "69.2.0"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        build-dependencies = [
            { name = "wheel", version = "0.43.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/4d/5b/dc575711b6b8f2f866131a40d053e30e962e633b332acf7cd2c24843d83d/setuptools-69.2.0.tar.gz", hash = "sha256:0ff4183f8f42cd8fa3acea16c45205521a4ef28f73c6391d8a25e92893134f2e", size = 2222950, upload-time = "2024-03-13T11:20:59.219Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/92/e1/1c8bb3420105e70bdf357d57dd5567202b4ef8d27f810e98bb962d950834/setuptools-69.2.0-py3-none-any.whl", hash = "sha256:c21c49fb1042386df081cb5d86759792ab89efca84cf114889191cd09aacc80c", size = 821485, upload-time = "2024-03-13T11:20:54.103Z" },
        ]

        [[package]]
        name = "setuptools-scm"
        version = "8.0.4"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        dependencies = [
            { name = "packaging" },
            { name = "setuptools" },
            { name = "typing-extensions" },
        ]
        build-dependencies = [
            { name = "setuptools", version = "69.2.0" },
            { name = "wheel", version = "0.43.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/eb/b1/0248705f10f6de5eefe7ff93e399f7192257b23df4d431d2f5680bb2778f/setuptools-scm-8.0.4.tar.gz", hash = "sha256:b5f43ff6800669595193fd09891564ee9d1d7dcb196cab4b2506d53a2e1c95c7", size = 74280, upload-time = "2023-10-02T15:14:32.996Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/0e/a3/b9a8b0adfe672bf0df5901707aa929d30a97ee390ba651910186776746d2/setuptools_scm-8.0.4-py3-none-any.whl", hash = "sha256:b47844cd2a84b83b3187a5782c71128c28b4c94cad8bfb871da2784a5cb54c4f", size = 42137, upload-time = "2023-10-02T15:14:31.281Z" },
        ]

        [[package]]
        name = "trove-classifiers"
        version = "2024.3.3"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        build-dependencies = [
            { name = "calver", version = "2022.6.26" },
            { name = "setuptools", version = "69.2.0" },
            { name = "wheel", version = "0.43.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/13/11/e13906315b498cb8f5ce5a7ff39fc35941e8291e914158157937fd1c095d/trove-classifiers-2024.3.3.tar.gz", hash = "sha256:df7edff9c67ff86b733628998330b180e81d125b1e096536d83ac0fd79673fdc", size = 15982, upload-time = "2024-03-03T20:17:38.634Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/bb/81/a16cb58f719e68d0cce72fb9afd6f0f50c0e474d7b8dc267c8309c3e2793/trove_classifiers-2024.3.3-py3-none-any.whl", hash = "sha256:3a84096861b385ec422c79995d1f6435dde47a9b63adaa3c886e53232ba7e6e0", size = 13377, upload-time = "2024-03-03T20:17:34.101Z" },
        ]

        [[package]]
        name = "typing-extensions"
        version = "4.10.0"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        build-dependencies = [
            { name = "flit-core", version = "3.9.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/16/3a/0d26ce356c7465a19c9ea8814b960f8a36c3b0d07c323176620b7b483e44/typing_extensions-4.10.0.tar.gz", hash = "sha256:b0abd7c89e8fb96f98db18d86106ff1d90ab692004eb746cf6eda2682f91b3cb", size = 77558, upload-time = "2024-02-25T22:12:49.693Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/f9/de/dc04a3ea60b22624b51c703a84bbe0184abcd1d0b9bc8074b5d6b7ab90bb/typing_extensions-4.10.0-py3-none-any.whl", hash = "sha256:69b1a937c3a517342112fb4c6df7e72fc39a38e7891a5730ed4985b5214b5475", size = 33926, upload-time = "2024-02-25T22:12:47.72Z" },
        ]

        [[package]]
        name = "wheel"
        version = "0.43.0"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        build-dependencies = [
            { name = "flit-core", version = "3.9.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/b8/d6/ac9cd92ea2ad502ff7c1ab683806a9deb34711a1e2bd8a59814e8fc27e69/wheel-0.43.0.tar.gz", hash = "sha256:465ef92c69fa5c5da2d1cf8ac40559a8c940886afcef87dcf14b9470862f1d85", size = 99109, upload-time = "2024-03-11T19:29:17.32Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/7d/cd/d7460c9a869b16c3dd4e1e403cce337df165368c71d6af229a74699622ce/wheel-0.43.0-py3-none-any.whl", hash = "sha256:55c570405f142630c6b9f72fe09d9b67cf1477fcf543ae5b8dcb1f5b7377da81", size = 65775, upload-time = "2024-03-11T19:29:15.522Z" },
        ]
        "#
        );
    });

    // Verify the lock file is valid.
    uv_snapshot!(context.filters(), context.lock().arg("--preview-features").arg("lock-build-dependencies").arg("--locked"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    ");

    Ok(())
}

/// Verify that a package appearing in both the runtime dependency tree and the
/// build dependency tree is not duplicated, and its existing `dependencies`
/// from the runtime resolution are preserved.
#[test]
fn lock_build_dependencies_shared_package() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    // Create a local dependency with dynamic version that requires `iniconfig`
    // as a build dependency (via setuptools build-system.requires).
    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        dynamic = ["version"]
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42", "iniconfig", "wheel"]
        build-backend = "setuptools.build_meta"

        [tool.setuptools.dynamic]
        version = {attr = "dep.__version__"}
        "#,
    )?;
    dep_dir.child("dep").create_dir_all()?;
    dep_dir
        .child("dep/__init__.py")
        .write_str("__version__ = '0.1.0'")?;

    // The root project depends on both `dep` (which needs iniconfig to build)
    // and `iniconfig` directly as a runtime dependency.
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep", "iniconfig"]

        [tool.uv.sources]
        dep = { path = "dep" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock().arg("--preview-features").arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    ");

    let lock = context.read("uv.lock");

    // `iniconfig` should appear exactly once as a [[package]], used by both
    // the build-dependencies of `dep` and the runtime dependencies of `project`.
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 2
        revision = 4
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[resolution]]
        id = "build:calver:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "calver"
        roots = [
            { name = "setuptools", version = "69.2.0" },
        ]

        [[resolution]]
        id = "build:calver:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "calver"
        roots = [
            { name = "setuptools", version = "69.2.0" },
            { name = "wheel", version = "0.43.0" },
        ]

        [[resolution]]
        id = "build:dep:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "dep"
        roots = [
            { name = "iniconfig", version = "2.0.0" },
            { name = "setuptools", version = "69.2.0" },
            { name = "wheel", version = "0.43.0" },
        ]

        [[resolution]]
        id = "build:dep:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "dep"
        roots = [
            { name = "iniconfig", version = "2.0.0" },
            { name = "setuptools", version = "69.2.0" },
            { name = "wheel", version = "0.43.0" },
        ]

        [[resolution]]
        id = "build:flit-core:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "flit-core"

        [[resolution]]
        id = "build:flit-core:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "flit-core"

        [[resolution]]
        id = "build:hatch-vcs:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "hatch-vcs"
        roots = [
            { name = "hatchling", version = "1.22.4" },
        ]

        [[resolution]]
        id = "build:hatch-vcs:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "hatch-vcs"
        roots = [
            { name = "hatchling", version = "1.22.4" },
        ]

        [[resolution]]
        id = "build:hatchling:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "hatchling"

        [[resolution]]
        id = "build:hatchling:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "hatchling"
        roots = [
            { name = "packaging", version = "24.0" },
            { name = "pathspec", version = "0.12.1" },
            { name = "pluggy", version = "1.4.0" },
            { name = "trove-classifiers", version = "2024.3.3" },
        ]

        [[resolution]]
        id = "build:iniconfig:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "iniconfig"
        roots = [
            { name = "hatch-vcs", version = "0.4.0" },
            { name = "hatchling", version = "1.22.4" },
        ]

        [[resolution]]
        id = "build:iniconfig:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "iniconfig"
        roots = [
            { name = "hatch-vcs", version = "0.4.0" },
            { name = "hatchling", version = "1.22.4" },
        ]

        [[resolution]]
        id = "build:packaging:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "packaging"
        roots = [
            { name = "flit-core", version = "3.9.0" },
        ]

        [[resolution]]
        id = "build:packaging:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "packaging"
        roots = [
            { name = "flit-core", version = "3.9.0" },
        ]

        [[resolution]]
        id = "build:pathspec:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "pathspec"
        roots = [
            { name = "flit-core", version = "3.9.0" },
        ]

        [[resolution]]
        id = "build:pathspec:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "pathspec"
        roots = [
            { name = "flit-core", version = "3.9.0" },
        ]

        [[resolution]]
        id = "build:pluggy:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "pluggy"
        roots = [
            { name = "setuptools", version = "69.2.0" },
            { name = "setuptools-scm", version = "8.0.4" },
            { name = "setuptools-scm", version = "8.0.4", extra = ["toml"] },
        ]

        [[resolution]]
        id = "build:pluggy:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "pluggy"
        roots = [
            { name = "setuptools", version = "69.2.0" },
            { name = "setuptools-scm", version = "8.0.4" },
            { name = "setuptools-scm", version = "8.0.4", extra = ["toml"] },
            { name = "wheel", version = "0.43.0" },
        ]

        [[resolution]]
        id = "build:setuptools-scm:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "setuptools-scm"
        roots = [
            { name = "setuptools", version = "69.2.0" },
        ]

        [[resolution]]
        id = "build:setuptools-scm:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "setuptools-scm"
        roots = [
            { name = "setuptools", version = "69.2.0" },
            { name = "wheel", version = "0.43.0" },
        ]

        [[resolution]]
        id = "build:setuptools:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "setuptools"

        [[resolution]]
        id = "build:setuptools:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "setuptools"
        roots = [
            { name = "wheel", version = "0.43.0" },
        ]

        [[resolution]]
        id = "build:trove-classifiers:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "trove-classifiers"
        roots = [
            { name = "calver", version = "2022.6.26" },
            { name = "setuptools", version = "69.2.0" },
        ]

        [[resolution]]
        id = "build:trove-classifiers:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "trove-classifiers"
        roots = [
            { name = "calver", version = "2022.6.26" },
            { name = "setuptools", version = "69.2.0" },
            { name = "wheel", version = "0.43.0" },
        ]

        [[resolution]]
        id = "build:typing-extensions:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "typing-extensions"
        roots = [
            { name = "flit-core", version = "3.9.0" },
        ]

        [[resolution]]
        id = "build:typing-extensions:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "typing-extensions"
        roots = [
            { name = "flit-core", version = "3.9.0" },
        ]

        [[resolution]]
        id = "build:wheel:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "wheel"
        roots = [
            { name = "flit-core", version = "3.9.0" },
        ]

        [[resolution]]
        id = "build:wheel:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "wheel"
        roots = [
            { name = "flit-core", version = "3.9.0" },
        ]

        [[package]]
        name = "calver"
        version = "2022.6.26"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        build-dependencies = [
            { name = "setuptools", version = "69.2.0" },
            { name = "wheel", version = "0.43.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/b5/00/96cbed7c019c49ee04b8a08357a981983db7698ae6de402e57097cefc9ad/calver-2022.6.26.tar.gz", hash = "sha256:e05493a3b17517ef1748fbe610da11f10485faa7c416b9d33fd4a52d74894f8b", size = 6670, upload-time = "2022-06-26T23:25:10.382Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/f7/39/e421c06f42ca00fa9cf8929c2466e58a837e8e97b8ab3ff4f4ff9a15e33e/calver-2022.6.26-py3-none-any.whl", hash = "sha256:a1d7fcdd67797afc52ee36ffb8c8adf6643173864306547bfd1380cbce6310a0", size = 7049, upload-time = "2022-06-26T23:25:07.692Z" },
        ]

        [[package]]
        name = "dep"
        source = { directory = "dep" }
        build-dependencies = [
            { name = "iniconfig", version = "2.0.0" },
            { name = "setuptools", version = "69.2.0" },
            { name = "wheel", version = "0.43.0" },
        ]

        [package.metadata]
        build-system = { build-backend = "setuptools.build_meta" }
        build-requires = [
            { name = "iniconfig" },
            { name = "setuptools", specifier = ">=42" },
            { name = "wheel" },
        ]

        [[package]]
        name = "flit-core"
        version = "3.9.0"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        build-dependencies = []
        sdist = { url = "https://files.pythonhosted.org/packages/c4/e6/c1ac50fe3eebb38a155155711e6e864e254ce4b6e17fe2429b4c4d5b9e80/flit_core-3.9.0.tar.gz", hash = "sha256:72ad266176c4a3fcfab5f2930d76896059851240570ce9a98733b658cb786eba", size = 41917, upload-time = "2023-05-14T14:48:51.809Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/38/45/618e84e49a6c51e5dd15565ec2fcd82ab273434f236b8f108f065ded517a/flit_core-3.9.0-py3-none-any.whl", hash = "sha256:7aada352fb0c7f5538c4fafeddf314d3a6a92ee8e2b1de70482329e42de70301", size = 63141, upload-time = "2023-05-14T14:48:49.24Z" },
        ]

        [[package]]
        name = "hatch-vcs"
        version = "0.4.0"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        dependencies = [
            { name = "hatchling" },
            { name = "setuptools-scm" },
        ]
        build-dependencies = [
            { name = "hatchling", version = "1.22.4" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/f5/c9/54bb4fa27b4e4a014ef3bb17710cdf692b3aa2cbc7953da885f1bf7e06ea/hatch_vcs-0.4.0.tar.gz", hash = "sha256:093810748fe01db0d451fabcf2c1ac2688caefd232d4ede967090b1c1b07d9f7", size = 10917, upload-time = "2023-11-06T06:24:57.228Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/82/0f/6cbd9976160bc334add63bc2e7a58b1433a31b34b7cda6c5de6dd983d9a7/hatch_vcs-0.4.0-py3-none-any.whl", hash = "sha256:b8a2b6bee54cf6f9fc93762db73890017ae59c9081d1038a41f16235ceaf8b2c", size = 8412, upload-time = "2023-11-06T06:24:55.389Z" },
        ]

        [[package]]
        name = "hatchling"
        version = "1.22.4"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        dependencies = [
            { name = "packaging" },
            { name = "pathspec" },
            { name = "pluggy" },
            { name = "trove-classifiers" },
        ]
        build-dependencies = [
            { name = "packaging", version = "24.0" },
            { name = "pathspec", version = "0.12.1" },
            { name = "pluggy", version = "1.4.0" },
            { name = "trove-classifiers", version = "2024.3.3" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/4f/2a/c34d71531d1e1c9a5029bb73eb3816285befd0fffd7c63ffa0544253dca8/hatchling-1.22.4.tar.gz", hash = "sha256:8a2dcec96d7fb848382ef5848e5ac43fdae641f35a08a3fab5116bd495f3416e", size = 62758, upload-time = "2024-03-24T02:00:59.122Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/49/63/2d56d6356f9f8b906aa68335cbf5b1b54c69873a2e271eda2ddba319c1ae/hatchling-1.22.4-py3-none-any.whl", hash = "sha256:f56da5bfc396af7b29daa3164851dd04991c994083f56cb054b5003675caecdc", size = 82032, upload-time = "2024-03-24T02:00:57.534Z" },
        ]

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        build-dependencies = [
            { name = "hatch-vcs", version = "0.4.0" },
            { name = "hatchling", version = "1.22.4" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646, upload-time = "2023-01-07T11:08:11.254Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892, upload-time = "2023-01-07T11:08:09.864Z" },
        ]

        [[package]]
        name = "packaging"
        version = "24.0"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        build-dependencies = [
            { name = "flit-core", version = "3.9.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/ee/b5/b43a27ac7472e1818c4bafd44430e69605baefe1f34440593e0332ec8b4d/packaging-24.0.tar.gz", hash = "sha256:eb82c5e3e56209074766e6885bb04b8c38a0c015d0a30036ebe7ece34c9989e9", size = 147882, upload-time = "2024-03-10T09:39:28.33Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/49/df/1fceb2f8900f8639e278b056416d49134fb8d84c5942ffaa01ad34782422/packaging-24.0-py3-none-any.whl", hash = "sha256:2ddfb553fdf02fb784c234c7ba6ccc288296ceabec964ad2eae3777778130bc5", size = 53488, upload-time = "2024-03-10T09:39:25.947Z" },
        ]

        [[package]]
        name = "pathspec"
        version = "0.12.1"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        build-dependencies = [
            { name = "flit-core", version = "3.9.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/ca/bc/f35b8446f4531a7cb215605d100cd88b7ac6f44ab3fc94870c120ab3adbf/pathspec-0.12.1.tar.gz", hash = "sha256:a482d51503a1ab33b1c67a6c3813a26953dbdc71c31dacaef9a838c4e29f5712", size = 51043, upload-time = "2023-12-10T22:30:45Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/cc/20/ff623b09d963f88bfde16306a54e12ee5ea43e9b597108672ff3a408aad6/pathspec-0.12.1-py3-none-any.whl", hash = "sha256:a0d503e138a4c123b27490a4f7beda6a01c6f288df0e4a8b79c7eb0dc7b4cc08", size = 31191, upload-time = "2023-12-10T22:30:43.14Z" },
        ]

        [[package]]
        name = "pluggy"
        version = "1.4.0"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        build-dependencies = [
            { name = "setuptools", version = "69.2.0" },
            { name = "setuptools-scm", version = "8.0.4" },
            { name = "setuptools-scm", version = "8.0.4", extra = ["toml"] },
            { name = "wheel", version = "0.43.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/54/c6/43f9d44d92aed815e781ca25ba8c174257e27253a94630d21be8725a2b59/pluggy-1.4.0.tar.gz", hash = "sha256:8c85c2876142a764e5b7548e7d9a0e0ddb46f5185161049a79b7e974454223be", size = 65812, upload-time = "2024-01-24T13:45:15.875Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/a5/5b/0cc789b59e8cc1bf288b38111d002d8c5917123194d45b29dcdac64723cc/pluggy-1.4.0-py3-none-any.whl", hash = "sha256:7db9f7b503d67d1c5b95f59773ebb58a8c1c288129a88665838012cfb07b8981", size = 20120, upload-time = "2024-01-24T13:45:14.227Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "dep" },
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "dep", directory = "dep" },
            { name = "iniconfig" },
        ]

        [[package]]
        name = "setuptools"
        version = "69.2.0"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        build-dependencies = [
            { name = "wheel", version = "0.43.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/4d/5b/dc575711b6b8f2f866131a40d053e30e962e633b332acf7cd2c24843d83d/setuptools-69.2.0.tar.gz", hash = "sha256:0ff4183f8f42cd8fa3acea16c45205521a4ef28f73c6391d8a25e92893134f2e", size = 2222950, upload-time = "2024-03-13T11:20:59.219Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/92/e1/1c8bb3420105e70bdf357d57dd5567202b4ef8d27f810e98bb962d950834/setuptools-69.2.0-py3-none-any.whl", hash = "sha256:c21c49fb1042386df081cb5d86759792ab89efca84cf114889191cd09aacc80c", size = 821485, upload-time = "2024-03-13T11:20:54.103Z" },
        ]

        [[package]]
        name = "setuptools-scm"
        version = "8.0.4"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        dependencies = [
            { name = "packaging" },
            { name = "setuptools" },
            { name = "typing-extensions" },
        ]
        build-dependencies = [
            { name = "setuptools", version = "69.2.0" },
            { name = "wheel", version = "0.43.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/eb/b1/0248705f10f6de5eefe7ff93e399f7192257b23df4d431d2f5680bb2778f/setuptools-scm-8.0.4.tar.gz", hash = "sha256:b5f43ff6800669595193fd09891564ee9d1d7dcb196cab4b2506d53a2e1c95c7", size = 74280, upload-time = "2023-10-02T15:14:32.996Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/0e/a3/b9a8b0adfe672bf0df5901707aa929d30a97ee390ba651910186776746d2/setuptools_scm-8.0.4-py3-none-any.whl", hash = "sha256:b47844cd2a84b83b3187a5782c71128c28b4c94cad8bfb871da2784a5cb54c4f", size = 42137, upload-time = "2023-10-02T15:14:31.281Z" },
        ]

        [[package]]
        name = "trove-classifiers"
        version = "2024.3.3"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        build-dependencies = [
            { name = "calver", version = "2022.6.26" },
            { name = "setuptools", version = "69.2.0" },
            { name = "wheel", version = "0.43.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/13/11/e13906315b498cb8f5ce5a7ff39fc35941e8291e914158157937fd1c095d/trove-classifiers-2024.3.3.tar.gz", hash = "sha256:df7edff9c67ff86b733628998330b180e81d125b1e096536d83ac0fd79673fdc", size = 15982, upload-time = "2024-03-03T20:17:38.634Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/bb/81/a16cb58f719e68d0cce72fb9afd6f0f50c0e474d7b8dc267c8309c3e2793/trove_classifiers-2024.3.3-py3-none-any.whl", hash = "sha256:3a84096861b385ec422c79995d1f6435dde47a9b63adaa3c886e53232ba7e6e0", size = 13377, upload-time = "2024-03-03T20:17:34.101Z" },
        ]

        [[package]]
        name = "typing-extensions"
        version = "4.10.0"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        build-dependencies = [
            { name = "flit-core", version = "3.9.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/16/3a/0d26ce356c7465a19c9ea8814b960f8a36c3b0d07c323176620b7b483e44/typing_extensions-4.10.0.tar.gz", hash = "sha256:b0abd7c89e8fb96f98db18d86106ff1d90ab692004eb746cf6eda2682f91b3cb", size = 77558, upload-time = "2024-02-25T22:12:49.693Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/f9/de/dc04a3ea60b22624b51c703a84bbe0184abcd1d0b9bc8074b5d6b7ab90bb/typing_extensions-4.10.0-py3-none-any.whl", hash = "sha256:69b1a937c3a517342112fb4c6df7e72fc39a38e7891a5730ed4985b5214b5475", size = 33926, upload-time = "2024-02-25T22:12:47.72Z" },
        ]

        [[package]]
        name = "wheel"
        version = "0.43.0"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        build-dependencies = [
            { name = "flit-core", version = "3.9.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/b8/d6/ac9cd92ea2ad502ff7c1ab683806a9deb34711a1e2bd8a59814e8fc27e69/wheel-0.43.0.tar.gz", hash = "sha256:465ef92c69fa5c5da2d1cf8ac40559a8c940886afcef87dcf14b9470862f1d85", size = 99109, upload-time = "2024-03-11T19:29:17.32Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/7d/cd/d7460c9a869b16c3dd4e1e403cce337df165368c71d6af229a74699622ce/wheel-0.43.0-py3-none-any.whl", hash = "sha256:55c570405f142630c6b9f72fe09d9b67cf1477fcf543ae5b8dcb1f5b7377da81", size = 65775, upload-time = "2024-03-11T19:29:15.522Z" },
        ]
        "#
        );
    });

    // Verify the lock file is valid by re-locking with --locked.
    uv_snapshot!(context.filters(), context.lock().arg("--preview-features").arg("lock-build-dependencies").arg("--locked"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    ");

    // Verify sync works (the shared package should be installed once).
    uv_snapshot!(context.filters(), context.sync().arg("--preview-features").arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 2 packages in [TIME]
     + dep==0.1.0 (from file://[TEMP_DIR]/dep)
     + iniconfig==2.0.0
    ");

    Ok(())
}

/// Verify build dependencies are captured for the project itself.
#[test]
fn lock_build_dependencies_trivial_project() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42", "wheel"]
        build-backend = "setuptools.build_meta"

        [tool.setuptools]
        packages = ["project"]
        "#,
    )?;
    context.temp_dir.child("project").create_dir_all()?;
    context.temp_dir.child("project/__init__.py").touch()?;

    uv_snapshot!(context.filters(), context.lock().arg("--preview-features").arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    ");

    let lock = context.read("uv.lock");
    let project = package_section(&lock, "project");
    assert!(project.contains(r#"source = { editable = "." }"#));
    assert!(project.contains("build-dependencies = ["));
    assert!(project.contains(r#"{ name = "setuptools", version = "69.2.0" }"#));
    assert!(project.contains(r#"{ name = "wheel", version = "0.43.0" }"#));
    assert!(project.contains("[package.metadata]"));
    assert!(project.contains(r#"{ name = "setuptools", specifier = ">=42" }"#));
    assert!(project.contains(r#"{ name = "wheel" }"#));

    Ok(())
}

/// Verify virtual projects do not resolve declared build requirements, since they are not built.
#[test]
fn lock_build_dependencies_virtual_project_skips_build_requirements() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["unavailable-builder"]
        build-backend = "unavailable_builder"

        [tool.uv]
        package = false
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--no-index"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    ");

    let lock = context.read("uv.lock");
    let project = package_section(&lock, "project");
    assert!(project.contains(r#"source = { virtual = "." }"#));
    assert!(!project.contains("build-dependencies = ["));

    Ok(())
}

/// Verify build dependencies are captured for workspace members.
#[test]
fn lock_build_dependencies_workspace_member() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["member"]

        [tool.uv.sources]
        member = { workspace = true }

        [tool.uv.workspace]
        members = ["member"]
        "#,
    )?;

    let member_dir = context.temp_dir.child("member");
    member_dir.create_dir_all()?;
    member_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "member"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42", "wheel"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;
    member_dir.child("member").create_dir_all()?;
    member_dir.child("member/__init__.py").touch()?;

    uv_snapshot!(context.filters(), context.lock().arg("--preview-features").arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    let member = package_section(&lock, "member");
    assert!(member.contains(r#"source = { editable = "member" }"#));
    assert!(member.contains("build-dependencies = ["));
    assert!(member.contains(r#"{ name = "setuptools", version = "69.2.0" }"#));
    assert!(member.contains(r#"{ name = "wheel", version = "0.43.0" }"#));
    assert!(member.contains("[package.metadata]"));
    assert!(member.contains(r#"{ name = "setuptools", specifier = ">=42" }"#));
    assert!(member.contains(r#"{ name = "wheel" }"#));

    Ok(())
}

/// Verify that changing `build-system.requires` in a dependency's pyproject.toml
/// invalidates the lock file and triggers re-resolution.
#[test]
fn lock_build_dependencies_stale_build_requires() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        dynamic = ["version"]
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.setuptools.dynamic]
        version = {attr = "dep.__version__"}
        "#,
    )?;
    dep_dir.child("dep").create_dir_all()?;
    dep_dir
        .child("dep/__init__.py")
        .write_str("__version__ = '0.1.0'")?;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
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

    // Run the initial lock.
    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    // Verify `--locked` accepts the initial lock file.
    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--locked"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    // Change build-system.requires to add `wheel`.
    dep_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        dynamic = ["version"]
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42", "wheel"]
        build-backend = "setuptools.build_meta"

        [tool.setuptools.dynamic]
        version = {attr = "dep.__version__"}
        "#,
    )?;

    // Verify `--locked` rejects the changed `build-system.requires`.
    uv_snapshot!(context.filters(), context
        .lock()
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

    // Re-lock without `--locked` to pick up the new `build-system.requires`.
    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    // Verify `--locked` accepts the refreshed lock file.
    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--locked"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    // Verify the lock file records the updated `build-requires`.
    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 2
        revision = 4
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[resolution]]
        id = "build:dep:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "dep"
        roots = [
            { name = "setuptools", version = "69.2.0" },
            { name = "wheel", version = "0.43.0" },
        ]

        [[resolution]]
        id = "build:dep:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "dep"
        roots = [
            { name = "setuptools", version = "69.2.0" },
            { name = "wheel", version = "0.43.0" },
        ]

        [[resolution]]
        id = "build:flit-core:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "flit-core"

        [[resolution]]
        id = "build:flit-core:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "flit-core"

        [[resolution]]
        id = "build:setuptools:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "setuptools"

        [[resolution]]
        id = "build:setuptools:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "setuptools"
        roots = [
            { name = "wheel", version = "0.43.0" },
        ]

        [[resolution]]
        id = "build:wheel:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "wheel"
        roots = [
            { name = "flit-core", version = "3.9.0" },
        ]

        [[resolution]]
        id = "build:wheel:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "wheel"
        roots = [
            { name = "flit-core", version = "3.9.0" },
        ]

        [[package]]
        name = "dep"
        source = { directory = "dep" }
        build-dependencies = [
            { name = "setuptools", version = "69.2.0" },
            { name = "wheel", version = "0.43.0" },
        ]

        [package.metadata]
        build-system = { build-backend = "setuptools.build_meta" }
        build-requires = [
            { name = "setuptools", specifier = ">=42" },
            { name = "wheel" },
        ]

        [[package]]
        name = "flit-core"
        version = "3.9.0"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        build-dependencies = []
        sdist = { url = "https://files.pythonhosted.org/packages/c4/e6/c1ac50fe3eebb38a155155711e6e864e254ce4b6e17fe2429b4c4d5b9e80/flit_core-3.9.0.tar.gz", hash = "sha256:72ad266176c4a3fcfab5f2930d76896059851240570ce9a98733b658cb786eba", size = 41917, upload-time = "2023-05-14T14:48:51.809Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/38/45/618e84e49a6c51e5dd15565ec2fcd82ab273434f236b8f108f065ded517a/flit_core-3.9.0-py3-none-any.whl", hash = "sha256:7aada352fb0c7f5538c4fafeddf314d3a6a92ee8e2b1de70482329e42de70301", size = 63141, upload-time = "2023-05-14T14:48:49.24Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "dep" },
        ]

        [package.metadata]
        requires-dist = [{ name = "dep", directory = "dep" }]

        [[package]]
        name = "setuptools"
        version = "69.2.0"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        build-dependencies = [
            { name = "wheel", version = "0.43.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/4d/5b/dc575711b6b8f2f866131a40d053e30e962e633b332acf7cd2c24843d83d/setuptools-69.2.0.tar.gz", hash = "sha256:0ff4183f8f42cd8fa3acea16c45205521a4ef28f73c6391d8a25e92893134f2e", size = 2222950, upload-time = "2024-03-13T11:20:59.219Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/92/e1/1c8bb3420105e70bdf357d57dd5567202b4ef8d27f810e98bb962d950834/setuptools-69.2.0-py3-none-any.whl", hash = "sha256:c21c49fb1042386df081cb5d86759792ab89efca84cf114889191cd09aacc80c", size = 821485, upload-time = "2024-03-13T11:20:54.103Z" },
        ]

        [[package]]
        name = "wheel"
        version = "0.43.0"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        build-dependencies = [
            { name = "flit-core", version = "3.9.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/b8/d6/ac9cd92ea2ad502ff7c1ab683806a9deb34711a1e2bd8a59814e8fc27e69/wheel-0.43.0.tar.gz", hash = "sha256:465ef92c69fa5c5da2d1cf8ac40559a8c940886afcef87dcf14b9470862f1d85", size = 99109, upload-time = "2024-03-11T19:29:17.32Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/7d/cd/d7460c9a869b16c3dd4e1e403cce337df165368c71d6af229a74699622ce/wheel-0.43.0-py3-none-any.whl", hash = "sha256:55c570405f142630c6b9f72fe09d9b67cf1477fcf543ae5b8dcb1f5b7377da81", size = 65775, upload-time = "2024-03-11T19:29:15.522Z" },
        ]
        "#
        );
    });

    Ok(())
}

/// Verify that changing the PEP 517 backend identity invalidates the locked build graph.
#[test]
fn lock_build_dependencies_stale_build_backend() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    let backend = r#"
from pathlib import Path

def prepare_metadata_for_build_wheel(metadata_directory, config_settings=None):
    dist_info = Path(metadata_directory) / "dep-0.1.0.dist-info"
    dist_info.mkdir()
    (dist_info / "METADATA").write_text(
        "Metadata-Version: 2.3\nName: dep\nVersion: 0.1.0\n"
    )
    return dist_info.name
"#;
    dep_dir.child("backend_a.py").write_str(backend)?;
    dep_dir.child("backend_b.py").write_str(backend)?;
    for path in ["path-a", "path-b"] {
        dep_dir.child(path).create_dir_all()?;
        dep_dir.child(path).child("backend.py").write_str(backend)?;
    }

    dep_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        dynamic = ["version"]
        requires-python = ">=3.12"

        [build-system]
        requires = []
        backend-path = ["."]
        build-backend = "backend_a"
        "#,
    )?;

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
        .assert()
        .success();

    let lock = context.read("uv.lock");
    let dep = package_section(&lock, "dep");
    assert!(
        dep.contains(r#"build-system = { build-backend = "backend_a", backend-path = ["."] }"#),
        "{dep}"
    );

    dep_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        dynamic = ["version"]
        requires-python = ">=3.12"

        [build-system]
        requires = []
        backend-path = ["."]
        build-backend = "backend_b"
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
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

    dep_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        dynamic = ["version"]
        requires-python = ">=3.12"

        [build-system]
        requires = []
        backend-path = ["path-a"]
        build-backend = "backend"
        "#,
    )?;
    context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();

    dep_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        dynamic = ["version"]
        requires-python = ">=3.12"

        [build-system]
        requires = []
        backend-path = ["path-b"]
        build-backend = "backend"
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
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

/// Verify that stale build requirements for the executor invalidate the lock file.
#[test]
fn lock_build_dependencies_stale_build_requires_executor_platform() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let executor_marker = if cfg!(target_os = "windows") {
        "sys_platform == 'win32'"
    } else if cfg!(target_os = "macos") {
        "sys_platform == 'darwin'"
    } else {
        "sys_platform == 'linux'"
    };

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    write_wheel(
        &links_dir.child("seed-0.1.0-py3-none-any.whl"),
        "seed",
        "0.1.0",
    )?;
    write_wheel(
        &links_dir.child("seed-0.2.0-py3-none-any.whl"),
        "seed",
        "0.2.0",
    )?;

    let builder_dir = context.temp_dir.child("builder");
    builder_dir.create_dir_all()?;
    let builder_pyproject = builder_dir.child("pyproject.toml");
    let write_builder = |seed_version: &str| {
        builder_pyproject.write_str(&format!(
            r#"
        [project]
        name = "builder"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["seed=={seed_version}"]
        backend-path = ["."]
        build-backend = "build_backend"
        "#
        ))
    };
    write_builder("0.1.0")?;
    builder_dir.child("build_backend.py").write_str(
        r#"
from pathlib import Path
from zipfile import ZipFile

def get_requires_for_build_wheel(config_settings=None):
    return []

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    filename = "builder-0.1.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("builder/__init__.py", "")
        wheel.writestr(
            "builder-0.1.0.dist-info/METADATA",
            "Metadata-Version: 2.3\nName: builder\nVersion: 0.1.0\n",
        )
        wheel.writestr(
            "builder-0.1.0.dist-info/WHEEL",
            "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
        )
        wheel.writestr("builder-0.1.0.dist-info/RECORD", "")
    return filename
"#,
    )?;
    let builder_url = Url::from_directory_path(builder_dir.path()).unwrap();

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(&format!(
        r#"
        [project]
        name = "dep"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["builder @ {builder_url} ; {executor_marker}"]
        backend-path = ["."]
        build-backend = "build_backend"
        "#
    ))?;
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
        dependencies = ["dep"]

        [tool.uv.sources]
        dep = { path = "dep" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    assert!(
        package_section(&lock, "builder").contains(&format!(
            r#"{{ name = "seed", version = "0.1.0", marker = "{executor_marker}" }}"#
        )),
        "{lock}"
    );

    write_builder("0.2.0")?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
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

/// Verify that transitive build dependencies behind platform markers are
/// correctly excluded at sync time. When `anyio` (linux-only build dep)
/// is skipped on macOS/Windows, its transitive deps `idna` and `sniffio`
/// must also be excluded from the build environment.
#[test]
fn lock_build_dependencies_transitive_marker_filtering() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    // Create a local dependency with platform-specific build dependencies.
    // `anyio` is linux-only, `iniconfig` is darwin/windows-only.
    // `anyio` depends on `idna` and `sniffio`.
    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        dynamic = ["version"]
        requires-python = ">=3.12"

        [build-system]
        requires = [
            "setuptools>=42",
            "wheel",
            "anyio ; sys_platform == 'linux'",
            "iniconfig ; sys_platform == 'darwin' or sys_platform == 'win32'",
        ]
        build-backend = "setuptools.build_meta"

        [tool.setuptools.dynamic]
        version = {attr = "dep.__version__"}
        "#,
    )?;
    dep_dir.child("dep").create_dir_all()?;
    dep_dir
        .child("dep/__init__.py")
        .write_str("__version__ = '0.1.0'")?;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
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

    uv_snapshot!(context.filters(), context.lock().arg("--preview-features").arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");

    // Both marker branches are locked, while transitive dependencies remain
    // outside the direct build requirement list.
    let dep = package_section(&lock, "dep");
    assert!(
        dep.contains(
            r#"{ name = "anyio", version = "4.3.0", marker = "sys_platform == 'linux'" }"#
        ),
        "{lock}"
    );
    assert!(
        dep.contains(
            r#"{ name = "iniconfig", version = "2.0.0", marker = "sys_platform == 'darwin' or sys_platform == 'win32'" }"#
        ),
        "{lock}"
    );
    assert!(!dep.contains(r#"{ name = "idna","#), "{lock}");
    assert!(!dep.contains(r#"{ name = "sniffio","#), "{lock}");

    context
        .sync()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();

    Ok(())
}

/// Verify that build dependencies work for a directory dependency with a dynamic
/// version when syncing. During `uv lock`, the build resolution is stored with
/// the resolved version from the lock file. During `uv sync`, directory dists
/// return a URL (not a version) from `version_or_url()`, so `package_version` is
/// `None` at build time. The lookup must handle this version-key mismatch.
#[test]
fn lock_build_dependencies_dynamic_version_directory() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        dynamic = ["version"]
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42", "wheel"]
        build-backend = "setuptools.build_meta"

        [tool.setuptools.dynamic]
        version = {attr = "dep.__version__"}
        "#,
    )?;
    dep_dir.child("dep").create_dir_all()?;
    dep_dir
        .child("dep/__init__.py")
        .write_str("__version__ = '0.1.0'")?;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
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

    uv_snapshot!(context.filters(), context.lock().arg("--preview-features").arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 2
        revision = 4
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[resolution]]
        id = "build:dep:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "dep"
        roots = [
            { name = "setuptools", version = "69.2.0" },
            { name = "wheel", version = "0.43.0" },
        ]

        [[resolution]]
        id = "build:dep:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "dep"
        roots = [
            { name = "setuptools", version = "69.2.0" },
            { name = "wheel", version = "0.43.0" },
        ]

        [[resolution]]
        id = "build:flit-core:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "flit-core"

        [[resolution]]
        id = "build:flit-core:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "flit-core"

        [[resolution]]
        id = "build:setuptools:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "setuptools"

        [[resolution]]
        id = "build:setuptools:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "setuptools"
        roots = [
            { name = "wheel", version = "0.43.0" },
        ]

        [[resolution]]
        id = "build:wheel:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "wheel"
        roots = [
            { name = "flit-core", version = "3.9.0" },
        ]

        [[resolution]]
        id = "build:wheel:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "wheel"
        roots = [
            { name = "flit-core", version = "3.9.0" },
        ]

        [[package]]
        name = "dep"
        source = { directory = "dep" }
        build-dependencies = [
            { name = "setuptools", version = "69.2.0" },
            { name = "wheel", version = "0.43.0" },
        ]

        [package.metadata]
        build-system = { build-backend = "setuptools.build_meta" }
        build-requires = [
            { name = "setuptools", specifier = ">=42" },
            { name = "wheel" },
        ]

        [[package]]
        name = "flit-core"
        version = "3.9.0"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        build-dependencies = []
        sdist = { url = "https://files.pythonhosted.org/packages/c4/e6/c1ac50fe3eebb38a155155711e6e864e254ce4b6e17fe2429b4c4d5b9e80/flit_core-3.9.0.tar.gz", hash = "sha256:72ad266176c4a3fcfab5f2930d76896059851240570ce9a98733b658cb786eba", size = 41917, upload-time = "2023-05-14T14:48:51.809Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/38/45/618e84e49a6c51e5dd15565ec2fcd82ab273434f236b8f108f065ded517a/flit_core-3.9.0-py3-none-any.whl", hash = "sha256:7aada352fb0c7f5538c4fafeddf314d3a6a92ee8e2b1de70482329e42de70301", size = 63141, upload-time = "2023-05-14T14:48:49.24Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "dep" },
        ]

        [package.metadata]
        requires-dist = [{ name = "dep", directory = "dep" }]

        [[package]]
        name = "setuptools"
        version = "69.2.0"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        build-dependencies = [
            { name = "wheel", version = "0.43.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/4d/5b/dc575711b6b8f2f866131a40d053e30e962e633b332acf7cd2c24843d83d/setuptools-69.2.0.tar.gz", hash = "sha256:0ff4183f8f42cd8fa3acea16c45205521a4ef28f73c6391d8a25e92893134f2e", size = 2222950, upload-time = "2024-03-13T11:20:59.219Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/92/e1/1c8bb3420105e70bdf357d57dd5567202b4ef8d27f810e98bb962d950834/setuptools-69.2.0-py3-none-any.whl", hash = "sha256:c21c49fb1042386df081cb5d86759792ab89efca84cf114889191cd09aacc80c", size = 821485, upload-time = "2024-03-13T11:20:54.103Z" },
        ]

        [[package]]
        name = "wheel"
        version = "0.43.0"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        build-dependencies = [
            { name = "flit-core", version = "3.9.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/b8/d6/ac9cd92ea2ad502ff7c1ab683806a9deb34711a1e2bd8a59814e8fc27e69/wheel-0.43.0.tar.gz", hash = "sha256:465ef92c69fa5c5da2d1cf8ac40559a8c940886afcef87dcf14b9470862f1d85", size = 99109, upload-time = "2024-03-11T19:29:17.32Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/7d/cd/d7460c9a869b16c3dd4e1e403cce337df165368c71d6af229a74699622ce/wheel-0.43.0-py3-none-any.whl", hash = "sha256:55c570405f142630c6b9f72fe09d9b67cf1477fcf543ae5b8dcb1f5b7377da81", size = 65775, upload-time = "2024-03-11T19:29:15.522Z" },
        ]
        "#
        );
    });

    // Sync should succeed and use the locked build resolutions even though
    // `package_version` is `None` for the directory dep at build time.
    uv_snapshot!(context.filters(), context.sync().arg("--preview-features").arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + dep==0.1.0 (from file://[TEMP_DIR]/dep)
    ");

    Ok(())
}

/// Verify that `--no-build` disables build dependency locking.
#[test]
fn lock_build_dependencies_no_build_disables_locking() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--no-build"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646, upload-time = "2023-01-07T11:08:11.254Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892, upload-time = "2023-01-07T11:08:09.864Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [{ name = "iniconfig" }]
        "#);
    });

    Ok(())
}

/// Verify a revision-3 lock remains reusable when all builds are disabled.
#[tokio::test]
async fn lock_build_dependencies_no_build_reuses_revision_3() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let artifacts = context.temp_dir.child("artifacts");
    artifacts.create_dir_all()?;
    write_wheel(
        &artifacts.child("runtime-0.1.0-py3-none-any.whl"),
        "runtime",
        "0.1.0",
    )?;

    let server = MockServer::start().await;
    let index_url = format!("{}/simple/", server.uri());
    Mock::given(method("GET"))
        .and(path("/simple/runtime/"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            format!(
                r#"<a href="{}/files/runtime-0.1.0-py3-none-any.whl" data-upload-time="2024-03-01T00:00:00Z">runtime-0.1.0-py3-none-any.whl</a>"#,
                server.uri()
            ),
            "text/html",
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/files/runtime-0.1.0-py3-none-any.whl"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(fs_err::read(
            artifacts.child("runtime-0.1.0-py3-none-any.whl").path(),
        )?))
        .mount(&server)
        .await;

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["runtime==0.1.0"]
        "#,
    )?;

    context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--no-build")
        .arg("--index-url")
        .arg(&index_url)
        .assert()
        .success();

    let lock = context.read("uv.lock");
    assert!(lock.contains("revision = 3"), "{lock}");
    assert!(!lock.contains("build-dependencies = ["), "{lock}");

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--no-build")
        .arg("--index-url")
        .arg(&index_url)
        .arg("--locked")
        .arg("--offline")
        .arg("--no-cache"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    Ok(())
}

/// Verify that a lock created with `--no-build` is not reused when build
/// dependency locking is later requested without `--no-build`.
#[test]
fn lock_build_dependencies_no_build_relocks_without_no_build() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

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

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--no-build"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    assert!(!lock.contains("revision = 4"));
    assert!(!lock.contains("build-dependencies = ["));

    uv_snapshot!(context.filters(), context
        .lock()
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

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Added flit-core v3.9.0
    Added setuptools v69.2.0
    Added wheel v0.43.0
    ");

    let lock = context.read("uv.lock");
    assert!(lock.contains("revision = 4"));
    assert!(lock.contains("build-dependencies = ["));

    Ok(())
}

/// Verify that path source distributions with static metadata still have
/// their build requirements locked.
#[test]
fn lock_build_dependencies_static_sdist() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let source_dist = context.temp_dir.child("dep-0.1.0.zip");
    let mut zip = ZipFileWriter::new(Vec::new());
    let entry = ZipEntryBuilder::new("dep-0.1.0/pyproject.toml".into(), Compression::Stored);
    block_on(zip.write_entry_whole(
        entry,
        br#"
        [project]
        name = "dep"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    ))?;
    let entry = ZipEntryBuilder::new("dep-0.1.0/dep/__init__.py".into(), Compression::Stored);
    block_on(zip.write_entry_whole(entry, b""))?;
    fs_err::write(source_dist.path(), block_on(zip.close())?)?;

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep"]

        [tool.uv.sources]
        dep = { path = "dep-0.1.0.zip" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    let dep = package_section(&lock, "dep");
    assert!(dep.contains("build-dependencies = ["), "{dep}");
    assert!(dep.contains(r#"{ name = "setuptools", version = "69.2.0" }"#));

    Ok(())
}

/// Verify that compatible `uv_build` archives do not resolve an unused build environment.
#[test]
fn lock_build_dependencies_direct_build_static_sdist() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let source_dist = context.temp_dir.child("dep-0.1.0.zip");
    let mut zip = ZipFileWriter::new(Vec::new());
    let entry = ZipEntryBuilder::new("dep-0.1.0/pyproject.toml".into(), Compression::Stored);
    block_on(zip.write_entry_whole(
        entry,
        br#"
        [project]
        name = "dep"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"
        "#,
    ))?;
    let entry = ZipEntryBuilder::new("dep-0.1.0/dep/__init__.py".into(), Compression::Stored);
    block_on(zip.write_entry_whole(entry, b""))?;
    fs_err::write(source_dist.path(), block_on(zip.close())?)?;

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep"]

        [tool.uv.sources]
        dep = { path = "dep-0.1.0.zip" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--no-index"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    let dep = package_section(&lock, "dep");
    assert!(!dep.contains("build-dependencies = ["), "{dep}");

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--no-index")
        .arg("--locked"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    Ok(())
}

/// Verify that static source archives lower build requirements through source configuration.
#[test]
fn lock_build_dependencies_static_sdist_lowers_build_sources() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let private_builder = context
        .temp_dir
        .child("private_builder-0.1.0-py3-none-any.whl");
    write_wheel(&private_builder, "private-builder", "0.1.0")?;

    let source_dist = context.temp_dir.child("dep-0.1.0.zip");
    let mut zip = ZipFileWriter::new(Vec::new());
    let entry = ZipEntryBuilder::new("dep-0.1.0/pyproject.toml".into(), Compression::Stored);
    block_on(zip.write_entry_whole(
        entry,
        br#"
        [project]
        name = "dep"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["private-builder"]
        build-backend = "private_builder"

        [tool.uv.sources]
        private-builder = { path = "private_builder-0.1.0-py3-none-any.whl" }
        "#,
    ))?;
    let entry = ZipEntryBuilder::new(
        "dep-0.1.0/private_builder-0.1.0-py3-none-any.whl".into(),
        Compression::Stored,
    );
    block_on(zip.write_entry_whole(entry, &fs_err::read(private_builder.path())?))?;
    fs_err::write(source_dist.path(), block_on(zip.close())?)?;

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep"]

        [tool.uv.sources]
        dep = { path = "dep-0.1.0.zip" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--no-index"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    let dep = package_section(&lock, "dep");
    assert!(
        dep.contains(r#"{ name = "private-builder", version = "0.1.0" }"#),
        "{dep}"
    );

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--no-index")
        .arg("--locked"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    Ok(())
}

/// Verify that static source archives without an explicit build system lock
/// PEP 517's implicit setuptools build requirement.
#[test]
fn lock_build_dependencies_static_sdist_implicit_default_backend() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let source_dist = context.temp_dir.child("dep-0.1.0.zip");
    let mut zip = ZipFileWriter::new(Vec::new());
    let entry = ZipEntryBuilder::new("dep-0.1.0/pyproject.toml".into(), Compression::Stored);
    block_on(zip.write_entry_whole(
        entry,
        br#"
        [project]
        name = "dep"
        version = "0.1.0"
        requires-python = ">=3.12"
        "#,
    ))?;
    let entry = ZipEntryBuilder::new("dep-0.1.0/dep/__init__.py".into(), Compression::Stored);
    block_on(zip.write_entry_whole(entry, b""))?;
    fs_err::write(source_dist.path(), block_on(zip.close())?)?;

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep"]

        [tool.uv.sources]
        dep = { path = "dep-0.1.0.zip" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    let dep = package_section(&lock, "dep");
    assert!(dep.contains("build-dependencies = ["), "{dep}");
    assert!(dep.contains(r#"{ name = "setuptools", version = "69.2.0" }"#));

    Ok(())
}

/// Verify that changing build requirements in a mutable local source archive
/// invalidates the locked build environment.
#[test]
fn lock_build_dependencies_static_sdist_build_requires_invalidate() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let source_dist = context.temp_dir.child("dep-0.1.0.zip");
    let write_source_dist = |requires: &str| -> Result<()> {
        let mut zip = ZipFileWriter::new(Vec::new());
        let entry = ZipEntryBuilder::new("dep-0.1.0/pyproject.toml".into(), Compression::Stored);
        let pyproject_toml = format!(
            r#"
                [project]
                name = "dep"
                version = "0.1.0"
                requires-python = ">=3.12"

                [build-system]
                requires = [{requires}]
                build-backend = "setuptools.build_meta"
                "#
        );
        block_on(zip.write_entry_whole(entry, pyproject_toml.as_bytes()))?;
        let entry = ZipEntryBuilder::new("dep-0.1.0/dep/__init__.py".into(), Compression::Stored);
        block_on(zip.write_entry_whole(entry, b""))?;
        fs_err::write(source_dist.path(), block_on(zip.close())?)?;
        Ok(())
    };
    write_source_dist(r#""setuptools>=42""#)?;

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep"]

        [tool.uv.sources]
        dep = { path = "dep-0.1.0.zip" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    let dep = package_section(&lock, "dep");
    assert!(dep.contains(r#"build-requires = [{ name = "setuptools", specifier = ">=42" }]"#));

    write_source_dist(r#""setuptools>=42", "wheel""#)?;

    uv_snapshot!(context.filters(), context
        .lock()
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

/// Verify that changing build requirements in a mutable direct URL source archive invalidates
/// the locked build environment, while an offline check can still reuse the captured metadata.
#[tokio::test]
async fn lock_build_dependencies_direct_url_sdist_build_requires_invalidate() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    write_wheel(
        &links_dir.child("helper-0.1.0-py3-none-any.whl"),
        "helper",
        "0.1.0",
    )?;
    write_wheel(
        &links_dir.child("helper-0.2.0-py3-none-any.whl"),
        "helper",
        "0.2.0",
    )?;

    let source_dist = context.temp_dir.child("dep-0.1.0.zip");
    let write_source_dist = |helper_version: &str| -> Result<Vec<u8>> {
        let mut zip = ZipFileWriter::new(Vec::new());
        let entry = ZipEntryBuilder::new("dep-0.1.0/pyproject.toml".into(), Compression::Stored);
        let pyproject_toml = format!(
            r#"
                [project]
                name = "dep"
                version = "0.1.0"
                requires-python = ">=3.12"

                [build-system]
                requires = ["helper=={helper_version}"]
                backend-path = ["."]
                build-backend = "build_backend"
                "#
        );
        block_on(zip.write_entry_whole(entry, pyproject_toml.as_bytes()))?;
        let entry = ZipEntryBuilder::new("dep-0.1.0/build_backend.py".into(), Compression::Stored);
        block_on(zip.write_entry_whole(entry, b""))?;
        let entry = ZipEntryBuilder::new("dep-0.1.0/dep/__init__.py".into(), Compression::Stored);
        block_on(zip.write_entry_whole(entry, b""))?;
        let archive = block_on(zip.close())?;
        fs_err::write(source_dist.path(), &archive)?;
        Ok(archive)
    };

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/dep-0.1.0.zip"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(write_source_dist("0.1.0")?))
        .mount(&server)
        .await;

    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(&format!(
            r#"
            [project]
            name = "project"
            version = "0.1.0"
            requires-python = ">=3.12"
            dependencies = ["dep @ {}/dep-0.1.0.zip"]
            "#,
            server.uri()
        ))?;

    context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--no-index")
        .arg("--find-links")
        .arg(links_dir.path())
        .assert()
        .success();

    let lock = context.read("uv.lock");
    let dep = package_section(&lock, "dep");
    assert!(dep.contains(r#"source = { url = "http://"#), "{dep}");
    assert!(dep.contains(r#"build-requires = [{ name = "helper", specifier = "==0.1.0" }]"#));

    server.reset().await;
    Mock::given(method("GET"))
        .and(path("/dep-0.1.0.zip"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(write_source_dist("0.2.0")?))
        .mount(&server)
        .await;

    // An offline freshness check cannot inspect the updated archive and should keep using the
    // build-system metadata captured in the lockfile.
    context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--no-index")
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--locked")
        .arg("--offline")
        .arg("--no-cache")
        .assert()
        .success();

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--no-index")
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--locked")
        .arg("--no-cache"), @"
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

/// Verify that a complete empty build resolution is recorded and reused.
#[test]
fn lock_build_dependencies_empty_conditional_resolution() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    write_wheel(
        &links_dir.child("helper-0.1.0-py3-none-any.whl"),
        "helper",
        "0.1.0",
    )?;

    let source_dist = context.temp_dir.child("dep-0.1.0.zip");
    let mut zip = ZipFileWriter::new(Vec::new());
    let entry = ZipEntryBuilder::new("dep-0.1.0/pyproject.toml".into(), Compression::Stored);
    block_on(zip.write_entry_whole(
        entry,
        br#"
        [project]
        name = "dep"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["never ; python_version < '3.0'"]
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
    ))?;
    let entry = ZipEntryBuilder::new("dep-0.1.0/build_backend.py".into(), Compression::Stored);
    block_on(zip.write_entry_whole(
        entry,
        br#"
import os
from pathlib import Path
from zipfile import ZipFile

def get_requires_for_build_wheel(config_settings=None):
    requirement = os.environ.get("UV_TEST_BUILD_REQUIREMENT")
    return [requirement] if requirement else []

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    filename = "dep-0.1.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("dep/__init__.py", "")
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
    ))?;
    let entry = ZipEntryBuilder::new("dep-0.1.0/dep/__init__.py".into(), Compression::Stored);
    block_on(zip.write_entry_whole(entry, b""))?;
    fs_err::write(source_dist.path(), block_on(zip.close())?)?;

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep"]

        [tool.uv.sources]
        dep = { path = "dep-0.1.0.zip" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--no-index"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    let dep = package_section(&lock, "dep");
    assert!(dep.contains("build-dependencies = []"), "{dep}");

    uv_snapshot!(context.filters(), context
        .sync()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--no-index")
        .arg("--frozen")
        .env("UV_TEST_BUILD_REQUIREMENT", "helper==0.1.0"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to build `dep @ file://[TEMP_DIR]/dep-0.1.0.zip`
      ├─▶ Failed to resolve requirements from `build-system.requires`
      ╰─▶ The build requirements returned by the backend for `dep` do not match the locked build environment

    hint: `dep` was included because `project` (v0.1.0) depends on `dep`
    ");

    Ok(())
}

/// Verify that `--no-build-package` skips build dependency locking only for
/// the selected package.
#[test]
fn lock_build_dependencies_no_build_package_skips_selected() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    for dep_name in ["dep", "dep2"] {
        let dep_dir = context.temp_dir.child(dep_name);
        dep_dir.create_dir_all()?;
        dep_dir.child("pyproject.toml").write_str(&format!(
            r#"
            [project]
            name = "{dep_name}"
            version = "0.1.0"
            requires-python = ">=3.12"

            [build-system]
            requires = ["setuptools>=42"]
            build-backend = "setuptools.build_meta"
            "#
        ))?;
    }

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep", "dep2"]

        [tool.uv.sources]
        dep = { path = "dep" }
        dep2 = { path = "dep2" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--no-build-package")
        .arg("dep"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    ");

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(lock, @r#"
        version = 2
        revision = 4
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        build-settings = "aee6cdd85e59a835"

        [[resolution]]
        id = "build:dep2:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "dep2"
        roots = [
            { name = "setuptools", version = "69.2.0" },
        ]

        [[resolution]]
        id = "build:dep2:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "dep2"
        roots = [
            { name = "setuptools", version = "69.2.0" },
            { name = "wheel", version = "0.43.0" },
        ]

        [[resolution]]
        id = "build:flit-core:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "flit-core"

        [[resolution]]
        id = "build:flit-core:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "flit-core"

        [[resolution]]
        id = "build:setuptools:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "setuptools"

        [[resolution]]
        id = "build:setuptools:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "setuptools"
        roots = [
            { name = "wheel", version = "0.43.0" },
        ]

        [[resolution]]
        id = "build:wheel:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "wheel"
        roots = [
            { name = "flit-core", version = "3.9.0" },
        ]

        [[resolution]]
        id = "build:wheel:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "wheel"
        roots = [
            { name = "flit-core", version = "3.9.0" },
        ]

        [[package]]
        name = "dep"
        version = "0.1.0"
        source = { directory = "dep" }

        [package.metadata]
        build-system = { build-backend = "setuptools.build_meta" }
        build-requires = [{ name = "setuptools", specifier = ">=42" }]

        [[package]]
        name = "dep2"
        version = "0.1.0"
        source = { directory = "dep2" }
        build-dependencies = [
            { name = "setuptools", version = "69.2.0" },
            { name = "wheel", version = "0.43.0" },
        ]

        [package.metadata]
        build-system = { build-backend = "setuptools.build_meta" }
        build-requires = [{ name = "setuptools", specifier = ">=42" }]

        [[package]]
        name = "flit-core"
        version = "3.9.0"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        build-dependencies = []
        sdist = { url = "https://files.pythonhosted.org/packages/c4/e6/c1ac50fe3eebb38a155155711e6e864e254ce4b6e17fe2429b4c4d5b9e80/flit_core-3.9.0.tar.gz", hash = "sha256:72ad266176c4a3fcfab5f2930d76896059851240570ce9a98733b658cb786eba", size = 41917, upload-time = "2023-05-14T14:48:51.809Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/38/45/618e84e49a6c51e5dd15565ec2fcd82ab273434f236b8f108f065ded517a/flit_core-3.9.0-py3-none-any.whl", hash = "sha256:7aada352fb0c7f5538c4fafeddf314d3a6a92ee8e2b1de70482329e42de70301", size = 63141, upload-time = "2023-05-14T14:48:49.24Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "dep" },
            { name = "dep2" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "dep", directory = "dep" },
            { name = "dep2", directory = "dep2" },
        ]

        [[package]]
        name = "setuptools"
        version = "69.2.0"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        build-dependencies = [
            { name = "wheel", version = "0.43.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/4d/5b/dc575711b6b8f2f866131a40d053e30e962e633b332acf7cd2c24843d83d/setuptools-69.2.0.tar.gz", hash = "sha256:0ff4183f8f42cd8fa3acea16c45205521a4ef28f73c6391d8a25e92893134f2e", size = 2222950, upload-time = "2024-03-13T11:20:59.219Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/92/e1/1c8bb3420105e70bdf357d57dd5567202b4ef8d27f810e98bb962d950834/setuptools-69.2.0-py3-none-any.whl", hash = "sha256:c21c49fb1042386df081cb5d86759792ab89efca84cf114889191cd09aacc80c", size = 821485, upload-time = "2024-03-13T11:20:54.103Z" },
        ]

        [[package]]
        name = "wheel"
        version = "0.43.0"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        build-dependencies = [
            { name = "flit-core", version = "3.9.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/b8/d6/ac9cd92ea2ad502ff7c1ab683806a9deb34711a1e2bd8a59814e8fc27e69/wheel-0.43.0.tar.gz", hash = "sha256:465ef92c69fa5c5da2d1cf8ac40559a8c940886afcef87dcf14b9470862f1d85", size = 99109, upload-time = "2024-03-11T19:29:17.32Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/7d/cd/d7460c9a869b16c3dd4e1e403cce337df165368c71d6af229a74699622ce/wheel-0.43.0-py3-none-any.whl", hash = "sha256:55c570405f142630c6b9f72fe09d9b67cf1477fcf543ae5b8dcb1f5b7377da81", size = 65775, upload-time = "2024-03-11T19:29:15.522Z" },
        ]
        "#);
    });

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--locked"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided.

    hint: To update the lockfile, run `uv lock`.
    ");

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    let dep = package_section(&lock, "dep");
    assert!(dep.contains("build-dependencies = ["), "{dep}");

    Ok(())
}

/// Verify that a package-specific source requirement overrides global `--no-build`
/// during build dependency locking.
#[test]
fn lock_build_dependencies_no_binary_package_overrides_no_build() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    write_wheel(
        &links_dir.child("helper-0.1.0-py3-none-any.whl"),
        "helper",
        "0.1.0",
    )?;

    let child_dir = context.temp_dir.child("child");
    child_dir.create_dir_all()?;
    child_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "child"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = []
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
    )?;
    child_dir.child("build_backend.py").write_str(
        r#"
from importlib.metadata import version
from pathlib import Path
from zipfile import ZipFile

def get_requires_for_build_wheel(config_settings=None):
    return ["helper==0.1.0"]

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    if version("helper") != "0.1.0":
        raise RuntimeError("helper is unavailable")
    filename = "child-0.1.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("child/__init__.py", "")
        wheel.writestr(
            "child-0.1.0.dist-info/METADATA",
            "Metadata-Version: 2.3\nName: child\nVersion: 0.1.0\n",
        )
        wheel.writestr(
            "child-0.1.0.dist-info/WHEEL",
            "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
        )
        wheel.writestr("child-0.1.0.dist-info/RECORD", "")
    return filename
"#,
    )?;

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["child"]

        [tool.uv.sources]
        child = { path = "child" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--no-build")
        .arg("--no-binary-package")
        .arg("child")
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    let child = package_section(&lock, "child");
    assert!(
        child.contains(r#"{ name = "helper", version = "0.1.0" }"#),
        "{child}"
    );

    uv_snapshot!(context.filters(), context
        .sync()
        .arg("--no-index")
        .arg("--no-build")
        .arg("--no-binary-package")
        .arg("child")
        .arg("--frozen")
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + child==0.1.0 (from file://[TEMP_DIR]/child)
    ");

    Ok(())
}

/// Verify removing `--no-build-package` captures an implicit default backend.
#[test]
fn lock_build_dependencies_no_build_package_relocks_implicit_default_backend() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        version = "0.1.0"
        requires-python = ">=3.12"
        "#,
    )?;

    let dep2_dir = context.temp_dir.child("dep2");
    dep2_dir.create_dir_all()?;
    dep2_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep2"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep", "dep2"]

        [tool.uv.sources]
        dep = { path = "dep" }
        dep2 = { path = "dep2" }
        "#,
    )?;

    context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--no-build-package")
        .arg("dep")
        .assert()
        .success();

    let lock = context.read("uv.lock");
    let dep = package_section(&lock, "dep");
    assert!(!dep.contains("build-dependencies = ["), "{dep}");
    assert!(!dep.contains("build-requires = ["), "{dep}");

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--locked"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided.

    hint: To update the lockfile, run `uv lock`.
    ");

    context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();

    let lock = context.read("uv.lock");
    let dep = package_section(&lock, "dep");
    assert!(dep.contains("build-dependencies = ["), "{dep}");
    assert!(dep.contains(r#"{ name = "setuptools", version = "69.2.0" }"#));

    Ok(())
}

/// Verify removing `--no-build-package` captures a skipped registry source build graph.
#[test]
fn lock_build_dependencies_no_build_package_relocks_find_links_sdist() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let builder = context.temp_dir.child("builder-0.1.0-py3-none-any.whl");
    write_wheel(&builder, "builder", "0.1.0")?;
    let builder_url = Url::from_file_path(builder.path()).expect("valid file URL");

    let artifacts = context.temp_dir.child("artifacts");
    artifacts.create_dir_all()?;
    write_wheel(
        &artifacts.child("dep-0.1.0-py3-none-any.whl"),
        "dep",
        "0.1.0",
    )?;

    let source_dist = artifacts.child("dep-0.1.0.zip");
    let mut zip = ZipFileWriter::new(Vec::new());
    let entry = ZipEntryBuilder::new("dep-0.1.0/pyproject.toml".into(), Compression::Stored);
    let pyproject_toml = format!(
        r#"
            [project]
            name = "dep"
            version = "0.1.0"
            requires-python = ">=3.12"

            [build-system]
            requires = ["builder @ {builder_url}"]
            build-backend = "builder"
            "#
    );
    block_on(zip.write_entry_whole(entry, pyproject_toml.as_bytes()))?;
    fs_err::write(source_dist.path(), block_on(zip.close())?)?;

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep==0.1.0"]
        "#,
    )?;

    context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--find-links")
        .arg(artifacts.path())
        .arg("--no-index")
        .arg("--no-build-package")
        .arg("dep")
        .assert()
        .success();

    let lock = context.read("uv.lock");
    let dep = package_section(&lock, "dep");
    assert!(!dep.contains("build-dependencies = ["), "{dep}");

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--find-links")
        .arg(artifacts.path())
        .arg("--no-index")
        .arg("--locked"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided.

    hint: To update the lockfile, run `uv lock`.
    ");

    context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--find-links")
        .arg(artifacts.path())
        .arg("--no-index")
        .assert()
        .success();

    let lock = context.read("uv.lock");
    let dep = package_section(&lock, "dep");
    assert!(dep.contains("build-dependencies = ["), "{dep}");
    assert!(dep.contains(r#"{ name = "builder", version = "0.1.0""#));

    Ok(())
}

/// Verify that sync only reconstructs locked build resolutions for selected
/// packages, not unselected optional dependencies.
#[test]
fn sync_filters_locked_build_resolutions_to_selected_packages() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let helper_dir = context.temp_dir.child("helper");
    helper_dir.create_dir_all()?;
    helper_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "helper"
        dynamic = ["version"]
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.setuptools.dynamic]
        version = {attr = "helper.__version__"}
        "#,
    )?;
    helper_dir.child("helper").create_dir_all()?;
    helper_dir
        .child("helper/__init__.py")
        .write_str("__version__ = '0.1.0'")?;
    let helper_url = Url::from_directory_path(helper_dir.path()).unwrap();

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(&format!(
        r#"
        [project]
        name = "dep"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42", "helper @ {helper_url}"]
        build-backend = "setuptools.build_meta"
        "#
    ))?;

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [dependency-groups]
        dev = ["dep"]

        [tool.uv.sources]
        dep = { path = "dep" }
        helper = { path = "helper" }

        [tool.uv]
        package = false
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    uv_snapshot!(context.filters(), context
        .sync()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--frozen")
        .arg("--no-default-groups")
        .arg("--no-build"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Checked in [TIME]
    ");

    Ok(())
}

/// Verify that a selected wheel does not reconstruct its locked source build
/// environment during frozen sync.
#[tokio::test]
async fn sync_filters_locked_build_resolutions_to_selected_wheels() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let builder = context.temp_dir.child("builder-0.1.0-py3-none-any.whl");
    write_wheel(&builder, "builder", "0.1.0")?;
    let builder_url = Url::from_file_path(builder.path()).expect("valid file URL");

    let artifacts = context.temp_dir.child("artifacts");
    artifacts.create_dir_all()?;
    write_wheel(
        &artifacts.child("wheel_selected_dep-0.1.0-py3-none-any.whl"),
        "wheel-selected-dep",
        "0.1.0",
    )?;

    let source_dist = artifacts.child("wheel_selected_dep-0.1.0.zip");
    let mut zip = ZipFileWriter::new(Vec::new());
    let entry = ZipEntryBuilder::new(
        "wheel_selected_dep-0.1.0/pyproject.toml".into(),
        Compression::Stored,
    );
    let pyproject_toml = format!(
        r#"
            [project]
            name = "wheel-selected-dep"
            version = "0.1.0"
            requires-python = ">=3.12"

            [build-system]
            requires = ["builder @ {builder_url}"]
            build-backend = "builder"
            "#
    );
    block_on(zip.write_entry_whole(entry, pyproject_toml.as_bytes()))?;
    fs_err::write(source_dist.path(), block_on(zip.close())?)?;

    let server = MockServer::start().await;
    let index_url = format!("{}/simple/", server.uri());
    Mock::given(method("GET"))
        .and(path("/simple/wheel-selected-dep/"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            format!(
                r#"
                <a href="{}/files/wheel_selected_dep-0.1.0-py3-none-any.whl" data-upload-time="2024-03-01T00:00:00Z">wheel_selected_dep-0.1.0-py3-none-any.whl</a>
                <a href="{}/files/wheel_selected_dep-0.1.0.zip" data-upload-time="2024-03-01T00:00:00Z">wheel_selected_dep-0.1.0.zip</a>
                "#,
                server.uri(),
                server.uri()
            ),
            "text/html",
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/files/wheel_selected_dep-0.1.0-py3-none-any.whl"))
        .respond_with(
            ResponseTemplate::new(200).set_body_bytes(fs_err::read(
                artifacts
                    .child("wheel_selected_dep-0.1.0-py3-none-any.whl")
                    .path(),
            )?),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/files/wheel_selected_dep-0.1.0.zip"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(fs_err::read(
            artifacts.child("wheel_selected_dep-0.1.0.zip").path(),
        )?))
        .mount(&server)
        .await;

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["wheel-selected-dep"]
        "#,
    )?;

    let mut filters = context.filters();
    filters.push((r"(?m)^WARN Range requests not supported[^\n]*\n", ""));
    uv_snapshot!(filters, context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--index-url")
        .arg(&index_url), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    assert!(
        package_section(&lock, "wheel-selected-dep").contains("build-dependencies = ["),
        "{lock}"
    );

    uv_snapshot!(context.filters(), context
        .sync()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--frozen")
        .arg("--no-build")
        .arg("--index-url")
        .arg(index_url), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + wheel-selected-dep==0.1.0
    ");

    Ok(())
}

/// Verify that a wheel-selected registry package still locks hook requirements
/// needed when another target falls back to its source distribution.
#[test]
fn lock_build_dependencies_capture_fallback_sdist_hook_requirements() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let (host_wheel_tag, target_platform) = if cfg!(target_os = "macos") {
        ("macosx_10_9_universal2", "windows")
    } else if cfg!(target_os = "windows") {
        (
            if cfg!(target_arch = "aarch64") {
                "win_arm64"
            } else {
                "win_amd64"
            },
            "linux",
        )
    } else {
        (
            if cfg!(target_arch = "aarch64") {
                "manylinux_2_17_aarch64.manylinux2014_aarch64"
            } else {
                "manylinux_2_17_x86_64.manylinux2014_x86_64"
            },
            "windows",
        )
    };

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    write_wheel(
        &links_dir.child("helper-0.1.0-py3-none-any.whl"),
        "helper",
        "0.1.0",
    )?;
    write_wheel(
        &links_dir.child(format!("dep-0.1.0-py3-none-{host_wheel_tag}.whl")),
        "dep",
        "0.1.0",
    )?;

    let source_dist = links_dir.child("dep-0.1.0.zip");
    let mut zip = ZipFileWriter::new(Vec::new());
    let entry = ZipEntryBuilder::new("dep-0.1.0/pyproject.toml".into(), Compression::Stored);
    block_on(zip.write_entry_whole(
        entry,
        br#"
        [project]
        name = "dep"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = []
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
    ))?;
    let entry = ZipEntryBuilder::new("dep-0.1.0/build_backend.py".into(), Compression::Stored);
    block_on(zip.write_entry_whole(
        entry,
        br#"
from importlib.metadata import version
from pathlib import Path
from zipfile import ZipFile

def get_requires_for_build_wheel(config_settings=None):
    return ["helper==0.1.0"]

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    if version("helper") != "0.1.0":
        raise RuntimeError("helper is unavailable")
    filename = "dep-0.1.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("dep/__init__.py", "")
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
    ))?;
    fs_err::write(source_dist.path(), block_on(zip.close())?)?;

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep==0.1.0"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    let dep = package_section(&lock, "dep");
    assert!(
        dep.contains(r#"{ name = "helper", version = "0.1.0" }"#),
        "{dep}"
    );

    uv_snapshot!(context.filters(), context
        .sync()
        .arg("--python-platform")
        .arg(target_platform)
        .arg("--no-index")
        .arg("--frozen")
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + dep==0.1.0
    ");

    Ok(())
}

/// Verify that backend hooks are not called for a registry sdist when a
/// universal wheel makes it unreachable.
#[tokio::test]
async fn lock_build_dependencies_skip_unreachable_registry_sdist_hooks() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let artifacts = context.temp_dir.child("artifacts");
    artifacts.create_dir_all()?;
    write_wheel(
        &artifacts.child("dep-0.1.0-py3-none-any.whl"),
        "dep",
        "0.1.0",
    )?;

    let source_dist = artifacts.child("dep-0.1.0.zip");
    let mut zip = ZipFileWriter::new(Vec::new());
    let entry = ZipEntryBuilder::new("dep-0.1.0/pyproject.toml".into(), Compression::Stored);
    block_on(zip.write_entry_whole(
        entry,
        br#"
        [project]
        name = "dep"
        dynamic = ["version"]
        requires-python = ">=3.12,<4"

        [build-system]
        requires = []
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
    ))?;
    let entry = ZipEntryBuilder::new("dep-0.1.0/build_backend.py".into(), Compression::Stored);
    block_on(zip.write_entry_whole(
        entry,
        br#"
def get_requires_for_build_wheel(config_settings=None):
    raise RuntimeError("unreachable source distribution hook was called")
"#,
    ))?;
    fs_err::write(source_dist.path(), block_on(zip.close())?)?;

    let server = MockServer::start().await;
    let index_url = format!("{}/simple/", server.uri());
    Mock::given(method("GET"))
        .and(path("/simple/dep/"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            format!(
                r#"
                <a href="{}/files/dep-0.1.0-py3-none-any.whl" data-upload-time="2024-03-01T00:00:00Z">dep-0.1.0-py3-none-any.whl</a>
                <a href="{}/files/dep-0.1.0.zip" data-upload-time="2024-03-01T00:00:00Z">dep-0.1.0.zip</a>
                "#,
                server.uri(),
                server.uri()
            ),
            "text/html",
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/files/dep-0.1.0-py3-none-any.whl"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(fs_err::read(
            artifacts.child("dep-0.1.0-py3-none-any.whl").path(),
        )?))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/files/dep-0.1.0.zip"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(fs_err::read(source_dist.path())?))
        .mount(&server)
        .await;

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12,<4"
        dependencies = ["dep==0.1.0"]
        "#,
    )?;

    context
        .lock()
        .arg("--index-url")
        .arg(&index_url)
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();

    // Forcing source distributions makes the hook reachable even with a universal wheel.
    let output = context
        .lock()
        .arg("--index-url")
        .arg(index_url)
        .arg("--no-binary-package")
        .arg("dep")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .output()?;
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("unreachable source distribution hook was called")
    );

    Ok(())
}

/// Verify that a wheel selected inside a locked build environment does not
/// reconstruct the build environment of its fallback source distribution.
#[tokio::test]
async fn sync_filters_nested_locked_build_resolutions_to_selected_wheels() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let backend = |name: &str| {
        format!(
            r#"
from pathlib import Path
from zipfile import ZipFile

def get_requires_for_build_wheel(config_settings=None):
    return []

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
        )
    };

    let trouble_dir = context.temp_dir.child("trouble");
    trouble_dir.create_dir_all()?;
    trouble_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "trouble"
        version = "0.1.0"
        requires-python = ">=3.12,<4"

        [build-system]
        requires = []
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
    )?;
    trouble_dir
        .child("build_backend.py")
        .write_str(&backend("trouble"))?;
    let trouble_url = Url::from_directory_path(trouble_dir.path()).expect("valid file URL");

    let artifacts = context.temp_dir.child("artifacts");
    artifacts.create_dir_all()?;
    write_wheel(
        &artifacts.child("nested-0.1.0-py3-none-any.whl"),
        "nested",
        "0.1.0",
    )?;

    let nested_source_dist = artifacts.child("nested-0.1.0.zip");
    let mut zip = ZipFileWriter::new(Vec::new());
    let entry = ZipEntryBuilder::new("nested-0.1.0/pyproject.toml".into(), Compression::Stored);
    let pyproject_toml = format!(
        r#"
            [project]
            name = "nested"
            dynamic = ["version"]
            requires-python = ">=3.12,<4"

            [build-system]
            requires = ["trouble @ {trouble_url}"]
            backend-path = ["."]
            build-backend = "build_backend"
            "#
    );
    block_on(zip.write_entry_whole(entry, pyproject_toml.as_bytes()))?;
    let entry = ZipEntryBuilder::new("nested-0.1.0/build_backend.py".into(), Compression::Stored);
    let nested_backend = backend("nested").replace(
        "    return []",
        r#"    raise RuntimeError("unreachable nested source distribution hook was called")"#,
    );
    block_on(zip.write_entry_whole(entry, nested_backend.as_bytes()))?;
    fs_err::write(nested_source_dist.path(), block_on(zip.close())?)?;

    let server = MockServer::start().await;
    let index_url = format!("{}/simple/", server.uri());
    Mock::given(method("GET"))
        .and(path("/simple/nested/"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            format!(
                r#"
                <a href="{}/files/nested-0.1.0-py3-none-any.whl" data-upload-time="2024-03-01T00:00:00Z">nested-0.1.0-py3-none-any.whl</a>
                <a href="{}/files/nested-0.1.0.zip" data-upload-time="2024-03-01T00:00:00Z">nested-0.1.0.zip</a>
                "#,
                server.uri(),
                server.uri()
            ),
            "text/html",
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/files/nested-0.1.0-py3-none-any.whl"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(fs_err::read(
            artifacts.child("nested-0.1.0-py3-none-any.whl").path(),
        )?))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/files/nested-0.1.0.zip"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(fs_err::read(artifacts.child("nested-0.1.0.zip").path())?),
        )
        .mount(&server)
        .await;

    let parent_dir = context.temp_dir.child("parent");
    parent_dir.create_dir_all()?;
    parent_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "parent"
        version = "0.1.0"
        requires-python = ">=3.12,<4"

        [build-system]
        requires = ["nested==0.1.0"]
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
    )?;
    parent_dir
        .child("build_backend.py")
        .write_str(&backend("parent"))?;

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12,<4"
        dependencies = ["parent"]

        [tool.uv.sources]
        parent = { path = "parent" }
        "#,
    )?;

    let mut filters = context.filters();
    filters.push((r"(?m)^WARN Range requests not supported[^\n]*\n", ""));
    uv_snapshot!(filters, context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--index-url")
        .arg(&index_url), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    assert!(
        package_section(&lock, "nested").contains("build-dependencies = ["),
        "{lock}"
    );

    uv_snapshot!(context.filters(), context
        .sync()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--frozen")
        .arg("--no-build-package")
        .arg("trouble")
        .arg("--index-url")
        .arg(index_url), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + parent==0.1.0 (from file://[TEMP_DIR]/parent)
    ");

    Ok(())
}

/// Verify that an excluded universal wheel does not hide the hook requirements of a selected
/// source distribution inside a locked build environment.
#[tokio::test]
async fn lock_build_dependencies_capture_excluded_nested_sdist_hooks() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let artifacts = context.temp_dir.child("artifacts");
    artifacts.create_dir_all()?;
    write_wheel(
        &artifacts.child("nested-0.1.0-py3-none-any.whl"),
        "nested",
        "0.1.0",
    )?;
    write_wheel(
        &artifacts.child("helper-0.1.0-py3-none-any.whl"),
        "helper",
        "0.1.0",
    )?;

    let nested_source_dist = artifacts.child("nested-0.1.0.zip");
    let mut zip = ZipFileWriter::new(Vec::new());
    let entry = ZipEntryBuilder::new("nested-0.1.0/pyproject.toml".into(), Compression::Stored);
    block_on(zip.write_entry_whole(
        entry,
        br#"
        [project]
        name = "nested"
        version = "0.1.0"
        requires-python = ">=3.12,<4"

        [build-system]
        requires = []
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
    ))?;
    let entry = ZipEntryBuilder::new("nested-0.1.0/build_backend.py".into(), Compression::Stored);
    block_on(zip.write_entry_whole(
        entry,
        br#"
from importlib.metadata import version
from pathlib import Path
from zipfile import ZipFile

def get_requires_for_build_wheel(config_settings=None):
    return ["helper==0.1.0"]

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    if version("helper") != "0.1.0":
        raise RuntimeError("helper is unavailable")
    filename = "nested-0.1.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("nested/__init__.py", "")
        wheel.writestr(
            "nested-0.1.0.dist-info/METADATA",
            "Metadata-Version: 2.3\nName: nested\nVersion: 0.1.0\n",
        )
        wheel.writestr(
            "nested-0.1.0.dist-info/WHEEL",
            "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
        )
        wheel.writestr("nested-0.1.0.dist-info/RECORD", "")
    return filename
"#,
    ))?;
    fs_err::write(nested_source_dist.path(), block_on(zip.close())?)?;

    let server = MockServer::start().await;
    let index_url = format!("{}/simple/", server.uri());
    Mock::given(method("GET"))
        .and(path("/simple/nested/"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            format!(
                r#"
                <a href="{}/files/nested-0.1.0-py3-none-any.whl" data-upload-time="2024-04-01T00:00:00Z">nested-0.1.0-py3-none-any.whl</a>
                <a href="{}/files/nested-0.1.0.zip" data-upload-time="2024-03-01T00:00:00Z">nested-0.1.0.zip</a>
                "#,
                server.uri(),
                server.uri()
            ),
            "text/html",
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/simple/helper/"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            format!(
                r#"<a href="{}/files/helper-0.1.0-py3-none-any.whl" data-upload-time="2024-03-01T00:00:00Z">helper-0.1.0-py3-none-any.whl</a>"#,
                server.uri()
            ),
            "text/html",
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/files/nested-0.1.0-py3-none-any.whl"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(fs_err::read(
            artifacts.child("nested-0.1.0-py3-none-any.whl").path(),
        )?))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/files/nested-0.1.0.zip"))
        .respond_with(
            ResponseTemplate::new(200).set_body_bytes(fs_err::read(nested_source_dist.path())?),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/files/helper-0.1.0-py3-none-any.whl"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(fs_err::read(
            artifacts.child("helper-0.1.0-py3-none-any.whl").path(),
        )?))
        .mount(&server)
        .await;

    let parent_dir = context.temp_dir.child("parent");
    parent_dir.create_dir_all()?;
    parent_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "parent"
        version = "0.1.0"
        requires-python = ">=3.12,<4"

        [build-system]
        requires = ["nested==0.1.0"]
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
    )?;
    parent_dir.child("build_backend.py").write_str(
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
        requires-python = ">=3.12,<4"
        dependencies = ["parent"]

        [tool.uv.sources]
        parent = { path = "parent" }
        "#,
    )?;

    context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--index-url")
        .arg(index_url)
        .arg("--exclude-newer")
        .arg("2024-03-25T00:00:00Z")
        .assert()
        .success();

    let lock = context.read("uv.lock");
    let nested = package_section(&lock, "nested");
    assert!(
        nested.contains(r#"{ name = "helper", version = "0.1.0" }"#),
        "{nested}"
    );

    Ok(())
}

/// Verify that lock-time metadata builds use the build dependency branch for
/// the active marker environment instead of flattening every universal branch
/// into one concrete build environment.
#[test]
fn lock_build_dependencies_marker_selected_build_environment() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let other_builder_dir = context.temp_dir.child("builder-other");
    other_builder_dir.create_dir_all()?;
    other_builder_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "builder"
        version = "0.2.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;
    other_builder_dir.child("builder").create_dir_all()?;
    other_builder_dir
        .child("builder/__init__.py")
        .write_str("DEP_VERSION = '0.2.0'")?;
    let other_builder_url = Url::from_directory_path(other_builder_dir.path()).unwrap();

    let linux_builder_dir = context.temp_dir.child("builder-linux");
    linux_builder_dir.create_dir_all()?;
    linux_builder_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "builder"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;
    linux_builder_dir.child("builder").create_dir_all()?;
    linux_builder_dir
        .child("builder/__init__.py")
        .write_str("DEP_VERSION = '0.1.0'")?;
    let linux_builder_url = Url::from_directory_path(linux_builder_dir.path()).unwrap();

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(&format!(
        r#"
        [project]
        name = "dep"
        dynamic = ["version"]
        requires-python = ">=3.12"

        [build-system]
        requires = [
            "setuptools>=42",
            "builder @ {other_builder_url} ; sys_platform != 'linux'",
            "builder @ {linux_builder_url} ; sys_platform == 'linux'",
        ]
        build-backend = "setuptools.build_meta"

        [tool.setuptools.dynamic]
        version = {{attr = "dep.__version__"}}
        "#
    ))?;
    dep_dir.child("dep").create_dir_all()?;
    dep_dir
        .child("dep/__init__.py")
        .write_str("from builder import DEP_VERSION\n__version__ = DEP_VERSION")?;

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

    uv_snapshot!(context.filters(), context.lock().arg("--preview-features").arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    let dep = package_section(&lock, "dep");
    let expected_version = if cfg!(target_os = "linux") {
        "version = \"0.1.0\""
    } else {
        "version = \"0.2.0\""
    };
    assert!(dep.contains(expected_version), "{dep}");

    Ok(())
}

/// Verify that source packages reached only through build dependencies are
/// reconstructed for frozen syncs and validated by locked relocks.
#[test]
fn lock_build_dependencies_build_only_source_package() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let builder_dir = context.temp_dir.child("builder");
    builder_dir.create_dir_all()?;
    let builder_pyproject = builder_dir.child("pyproject.toml");
    builder_pyproject.write_str(
        r#"
        [project]
        name = "builder"
        dynamic = ["version"]
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42", "wheel"]
        build-backend = "setuptools.build_meta"

        [tool.setuptools.dynamic]
        version = {attr = "builder.__version__"}
        "#,
    )?;
    builder_dir.child("builder").create_dir_all()?;
    builder_dir
        .child("builder/__init__.py")
        .write_str("__version__ = '0.1.0'")?;
    let builder_url = Url::from_directory_path(builder_dir.path()).unwrap();

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(&format!(
        r#"
        [project]
        name = "dep"
        dynamic = ["version"]
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42", "builder @ {builder_url}"]
        build-backend = "setuptools.build_meta"

        [tool.setuptools.dynamic]
        version = {{attr = "dep.__version__"}}
        "#
    ))?;
    dep_dir.child("dep").create_dir_all()?;
    dep_dir
        .child("dep/__init__.py")
        .write_str("__version__ = '0.1.0'")?;

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

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    let builder = package_section(&lock, "builder");
    assert!(builder.contains("build-dependencies = ["), "{builder}");

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--locked"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let missing_url = Url::from_directory_path(context.temp_dir.child("missing").path()).unwrap();
    builder_pyproject.write_str(&format!(
        r#"
        [project]
        name = "builder"
        dynamic = ["version"]
        requires-python = ">=3.12"

        [build-system]
        requires = ["missing @ {missing_url}"]
        build-backend = "setuptools.build_meta"

        [tool.setuptools.dynamic]
        version = {{attr = "builder.__version__"}}
        "#
    ))?;

    uv_snapshot!(context.filters(), context
        .sync()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--frozen"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + dep==0.1.0 (from file://[TEMP_DIR]/dep)
    ");

    builder_pyproject.write_str(
        r#"
        [project]
        name = "builder"
        dynamic = ["version"]
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42", "wheel"]
        build-backend = "setuptools.build_meta"

        [tool.setuptools.dynamic]
        version = {attr = "builder.__version__"}
        "#,
    )?;
    builder_dir
        .child("builder/__init__.py")
        .write_str("__version__ = '0.2.0'")?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--locked"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    Ok(())
}

/// Verify that runtime metadata is preserved for source packages reached only
/// through build dependencies.
#[test]
fn lock_build_dependencies_build_only_source_package_metadata() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    write_wheel(
        &links_dir.child("helper-0.1.0-py3-none-any.whl"),
        "helper",
        "0.1.0",
    )?;

    let builder_dir = context.temp_dir.child("builder");
    builder_dir.create_dir_all()?;
    builder_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "builder"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["helper==0.1.0"]

        [build-system]
        requires = []
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
    )?;
    builder_dir.child("build_backend.py").write_str(
        r#"
from pathlib import Path
from zipfile import ZipFile

def get_requires_for_build_wheel(config_settings=None):
    return []

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    filename = "builder-0.1.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("builder/__init__.py", "")
        wheel.writestr(
            "builder-0.1.0.dist-info/METADATA",
            "Metadata-Version: 2.3\nName: builder\nVersion: 0.1.0\nRequires-Dist: helper==0.1.0\n",
        )
        wheel.writestr(
            "builder-0.1.0.dist-info/WHEEL",
            "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
        )
        wheel.writestr("builder-0.1.0.dist-info/RECORD", "")
    return filename
"#,
    )?;
    let builder_url = Url::from_directory_path(builder_dir.path()).unwrap();

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(&format!(
        r#"
        [project]
        name = "dep"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["builder @ {builder_url}"]
        backend-path = ["."]
        build-backend = "build_backend"
        "#
    ))?;
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
        dependencies = ["dep"]

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
    let builder = package_section(&lock, "builder");
    assert!(!builder.contains("contexts"), "{builder}");
    assert!(builder.contains(r#"{ name = "helper" }"#), "{builder}");
    assert!(
        builder.contains(r#"requires-dist = [{ name = "helper", specifier = "==0.1.0" }]"#),
        "{builder}"
    );

    context
        .lock()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--locked")
        .assert()
        .success();

    Ok(())
}

/// Verify that relocking without the preview feature preserves existing
/// locked build dependencies without churn.
#[test]
fn lock_build_dependencies_on_then_off_no_churn() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        dynamic = ["version"]
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.setuptools.dynamic]
        version = {attr = "dep.__version__"}
        "#,
    )?;
    dep_dir.child("dep").create_dir_all()?;
    dep_dir
        .child("dep/__init__.py")
        .write_str("__version__ = '0.1.0'")?;

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

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock_with_feature = context.read("uv.lock");
    assert!(lock_with_feature.contains("build-dependencies = ["));

    uv_snapshot!(context.filters(), context.lock(), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock_without_feature = context.read("uv.lock");
    assert_eq!(lock_without_feature, lock_with_feature);

    Ok(())
}

/// Verify that when relocking without the preview feature requires a rewrite,
/// build dependencies are not emitted by the non-preview path.
#[test]
fn lock_build_dependencies_on_then_off_forced_rewrite() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        dynamic = ["version"]
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.setuptools.dynamic]
        version = {attr = "dep.__version__"}
        "#,
    )?;
    dep_dir.child("dep").create_dir_all()?;
    dep_dir
        .child("dep/__init__.py")
        .write_str("__version__ = '0.1.0'")?;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
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

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    // Force a rewrite by changing project dependencies.
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep", "iniconfig"]

        [tool.uv.sources]
        dep = { path = "dep" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Removed flit-core v3.9.0
    Added iniconfig v2.0.0
    Removed setuptools v69.2.0
    Removed wheel v0.43.0
    ");

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "dep"
        source = { directory = "dep" }

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646, upload-time = "2023-01-07T11:08:11.254Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892, upload-time = "2023-01-07T11:08:09.864Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "dep" },
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "dep", directory = "dep" },
            { name = "iniconfig" },
        ]
        "#);
    });

    Ok(())
}
