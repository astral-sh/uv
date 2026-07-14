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
use sha2::{Digest, Sha256};
use url::Url;
#[cfg(feature = "test-git")]
use walkdir::WalkDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use uv_static::EnvVars;
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
    write_wheel_with_requires_and_tag(path, name, version, requires_dist, "py3-none-any")
}

fn write_wheel_with_requires_and_tag(
    path: &ChildPath,
    name: &str,
    version: &str,
    requires_dist: &[&str],
    tag: &str,
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
        format!("Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: {tag}\n").as_bytes(),
    ))?;
    let entry = ZipEntryBuilder::new(format!("{dist_info}/RECORD").into(), Compression::Stored);
    block_on(zip.write_entry_whole(entry, b""))?;
    fs_err::write(path.path(), block_on(zip.close())?)?;

    Ok(())
}

fn unsupported_executor_wheel_tag() -> &'static str {
    if cfg!(target_os = "macos") {
        "cp312-cp312-macosx_99_0_arm64"
    } else if cfg!(target_os = "windows") {
        if cfg!(target_arch = "aarch64") {
            "cp312-cp312-win_amd64"
        } else {
            "cp312-cp312-win_arm64"
        }
    } else if cfg!(target_arch = "aarch64") {
        "cp312-cp312-musllinux_99_0_aarch64"
    } else {
        "cp312-cp312-musllinux_99_0_x86_64"
    }
}

fn write_build_source(
    path: &ChildPath,
    name: &str,
    version: &str,
    hook_requirement: Option<(&str, &str)>,
) -> Result<()> {
    let mut zip = ZipFileWriter::new(Vec::new());
    let source = format!("{name}-{version}");
    let module = name.replace('-', "_");
    let (hook_requires, hook_import) = hook_requirement.map_or_else(
        || (String::new(), String::new()),
        |(requirement, module)| {
            (
                format!("\"{requirement}\""),
                format!("    import {module}\n"),
            )
        },
    );
    let entry = ZipEntryBuilder::new(
        format!("{source}/pyproject.toml").into(),
        Compression::Stored,
    );
    block_on(
        zip.write_entry_whole(
            entry,
            format!(
                r#"
        [project]
        name = "{name}"
        version = "{version}"
        requires-python = ">=3.12"

        [build-system]
        requires = []
        backend-path = ["."]
        build-backend = "build_backend"
        "#
            )
            .as_bytes(),
        ),
    )?;
    let entry = ZipEntryBuilder::new(
        format!("{source}/build_backend.py").into(),
        Compression::Stored,
    );
    block_on(
        zip.write_entry_whole(
            entry,
            format!(
                r#"
from pathlib import Path
from zipfile import ZipFile

def get_requires_for_build_wheel(config_settings=None):
    return [{hook_requires}]

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
{hook_import}    filename = "{module}-{version}-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("{module}.py", "")
        wheel.writestr(
            "{module}-{version}.dist-info/METADATA",
            "Metadata-Version: 2.3\nName: {name}\nVersion: {version}\n",
        )
        wheel.writestr(
            "{module}-{version}.dist-info/WHEEL",
            "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
        )
        wheel.writestr("{module}-{version}.dist-info/RECORD", "")
    return filename
"#
            )
            .as_bytes(),
        ),
    )?;
    fs_err::write(path.path(), block_on(zip.close())?)?;

    Ok(())
}

fn write_executor_build_project(
    context: &uv_test::TestContext,
    requires: &str,
    expected_wheel: Option<&str>,
) -> Result<()> {
    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(&format!(
        r#"
        [project]
        name = "dep"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = [{requires}]
        backend-path = ["."]
        build-backend = "build_backend"
        "#
    ))?;
    let assertion = expected_wheel.map_or_else(String::new, |name| {
        format!("    assert 'Tag: py3-none-any' in distribution('{name}').read_text('WHEEL')\n")
    });
    dep_dir.child("build_backend.py").write_str(&format!(
        r#"
from importlib.metadata import distribution
from pathlib import Path
from zipfile import ZipFile

def get_requires_for_build_wheel(config_settings=None):
{assertion}    return []

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
{assertion}    filename = "dep-0.1.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("dep.py", "")
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
    ))?;

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
        id = "build:flit-core:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
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
        id = "build:wheel:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
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

/// Verify that build preferences from a newer lock invalidate metadata produced with older build
/// dependency versions in an existing cache.
#[tokio::test]
async fn lock_build_dependencies_invalidate_cached_metadata_for_preferences() -> Result<()> {
    fn write_backend_wheel(path: &ChildPath, version: &str, runtime: &str) -> Result<()> {
        let mut zip = ZipFileWriter::new(Vec::new());
        let entry = ZipEntryBuilder::new("helper_backend.py".into(), Compression::Stored);
        block_on(
            zip.write_entry_whole(
                entry,
                format!(
                    r#"
from pathlib import Path
from zipfile import ZipFile

RUNTIME = "{runtime}"

def get_requires_for_build_wheel(config_settings=None):
    return []

def prepare_metadata_for_build_wheel(metadata_directory, config_settings=None):
    dist_info = Path(metadata_directory) / "outer-0.1.0.dist-info"
    dist_info.mkdir(parents=True, exist_ok=True)
    (dist_info / "METADATA").write_text(
        "Metadata-Version: 2.3\nName: outer\nVersion: 0.1.0\nRequires-Dist: " + RUNTIME + "\n"
    )
    (dist_info / "WHEEL").write_text(
        "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n"
    )
    return dist_info.name

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    filename = "outer-0.1.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("outer.py", "")
        wheel.writestr(
            "outer-0.1.0.dist-info/METADATA",
            "Metadata-Version: 2.3\nName: outer\nVersion: 0.1.0\nRequires-Dist: " + RUNTIME + "\n",
        )
        wheel.writestr(
            "outer-0.1.0.dist-info/WHEEL",
            "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
        )
        wheel.writestr("outer-0.1.0.dist-info/RECORD", "")
    return filename
"#
                )
                .as_bytes(),
            ),
        )?;

        let dist_info = format!("helper_build-{version}.dist-info");
        let entry =
            ZipEntryBuilder::new(format!("{dist_info}/METADATA").into(), Compression::Stored);
        block_on(zip.write_entry_whole(
            entry,
            format!("Metadata-Version: 2.3\nName: helper-build\nVersion: {version}\n").as_bytes(),
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

    let context = uv_test::test_context!("3.12");
    let files = context.temp_dir.child("files");
    files.create_dir_all()?;
    let helper_one = files.child("helper_build-1.0.0-py3-none-any.whl");
    let helper_two = files.child("helper_build-2.0.0-py3-none-any.whl");
    let runtime_one = files.child("runtime_one-1.0.0-py3-none-any.whl");
    let runtime_two = files.child("runtime_two-1.0.0-py3-none-any.whl");
    write_backend_wheel(&helper_one, "1.0.0", "runtime-one")?;
    write_backend_wheel(&helper_two, "2.0.0", "runtime-two")?;
    write_wheel(&runtime_one, "runtime-one", "1.0.0")?;
    write_wheel(&runtime_two, "runtime-two", "1.0.0")?;

    let server = MockServer::start().await;
    let index_url = format!("{}/simple/", server.uri());
    Mock::given(method("GET"))
        .and(path("/simple/helper-build/"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            format!(
                r#"<a href="{}/files/helper_build-1.0.0-py3-none-any.whl" data-upload-time="2024-01-01T00:00:00Z">helper-build 1.0.0</a>"#,
                server.uri()
            ),
            "text/html",
        ))
        .mount(&server)
        .await;
    for (route, wheel) in [
        ("/files/helper_build-1.0.0-py3-none-any.whl", &helper_one),
        ("/files/runtime_one-1.0.0-py3-none-any.whl", &runtime_one),
        ("/files/runtime_two-1.0.0-py3-none-any.whl", &runtime_two),
    ] {
        Mock::given(method("GET"))
            .and(path(route))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(fs_err::read(wheel.path())?))
            .mount(&server)
            .await;
    }
    for (name, filename) in [
        ("runtime-one", "runtime_one-1.0.0-py3-none-any.whl"),
        ("runtime-two", "runtime_two-1.0.0-py3-none-any.whl"),
    ] {
        Mock::given(method("GET"))
            .and(path(format!("/simple/{name}/")))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                format!(
                    r#"<a href="{}/files/{filename}" data-upload-time="2024-01-01T00:00:00Z">{filename}</a>"#,
                    server.uri()
                ),
                "text/html",
            ))
            .mount(&server)
            .await;
    }

    let outer = context.temp_dir.child("outer");
    outer.create_dir_all()?;
    outer.child("pyproject.toml").write_str(
        r#"
        [build-system]
        requires = ["helper-build>=1"]
        build-backend = "helper_backend"
        "#,
    )?;
    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["outer"]

        [tool.uv.sources]
        outer = { path = "outer" }
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
    let initial = context.read("uv.lock");
    let outer = package_section(&initial, "outer");
    assert!(
        outer.contains(r#"{ name = "helper-build", version = "1.0.0" }"#),
        "{outer}"
    );
    assert!(
        outer.contains(r#"requires-dist = [{ name = "runtime-one" }]"#),
        "{outer}"
    );

    let old_cache = context.temp_dir.child("old-cache");
    fs_err::rename(context.cache_dir.path(), old_cache.path())?;

    server.reset().await;
    Mock::given(method("GET"))
        .and(path("/simple/helper-build/"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            format!(
                r#"<a href="{}/files/helper_build-1.0.0-py3-none-any.whl" data-upload-time="2024-01-01T00:00:00Z">helper-build 1.0.0</a><a href="{}/files/helper_build-2.0.0-py3-none-any.whl" data-upload-time="2024-02-01T00:00:00Z">helper-build 2.0.0</a>"#,
                server.uri(),
                server.uri()
            ),
            "text/html",
        ))
        .mount(&server)
        .await;
    for (route, wheel) in [
        ("/files/helper_build-1.0.0-py3-none-any.whl", &helper_one),
        ("/files/helper_build-2.0.0-py3-none-any.whl", &helper_two),
        ("/files/runtime_one-1.0.0-py3-none-any.whl", &runtime_one),
        ("/files/runtime_two-1.0.0-py3-none-any.whl", &runtime_two),
    ] {
        Mock::given(method("GET"))
            .and(path(route))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(fs_err::read(wheel.path())?))
            .mount(&server)
            .await;
    }
    for (name, filename) in [
        ("runtime-one", "runtime_one-1.0.0-py3-none-any.whl"),
        ("runtime-two", "runtime_two-1.0.0-py3-none-any.whl"),
    ] {
        Mock::given(method("GET"))
            .and(path(format!("/simple/{name}/")))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                format!(
                    r#"<a href="{}/files/{filename}" data-upload-time="2024-01-01T00:00:00Z">{filename}</a>"#,
                    server.uri()
                ),
                "text/html",
            ))
            .mount(&server)
            .await;
    }

    context
        .lock()
        .arg("--index-url")
        .arg(&index_url)
        .arg("--upgrade-package")
        .arg("helper-build")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();
    let upgraded = context.read("uv.lock");
    let outer = package_section(&upgraded, "outer");
    assert!(
        outer.contains(r#"{ name = "helper-build", version = "2.0.0" }"#),
        "{outer}"
    );
    assert!(
        outer.contains(r#"requires-dist = [{ name = "runtime-two" }]"#),
        "{outer}"
    );

    let new_cache = context.temp_dir.child("new-cache");
    fs_err::rename(context.cache_dir.path(), new_cache.path())?;
    fs_err::rename(old_cache.path(), context.cache_dir.path())?;
    let target = context.temp_dir.child("target");
    target.create_dir_all()?;
    context
        .pip_install()
        .arg("helper-build==2.0.0")
        .arg("--target")
        .arg(target.path())
        .arg("--index-url")
        .arg(&index_url)
        .arg("--refresh-package")
        .arg("helper-build")
        .assert()
        .success();

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["outer", "runtime-two"]

        [tool.uv.sources]
        outer = { path = "outer" }
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
    let relocked = context.read("uv.lock");
    let outer = package_section(&relocked, "outer");
    assert!(
        outer.contains(r#"{ name = "helper-build", version = "2.0.0" }"#),
        "{outer}"
    );
    assert!(
        outer.contains(r#"requires-dist = [{ name = "runtime-two" }]"#),
        "{outer}"
    );
    assert!(!outer.contains(r#"{ name = "runtime-one" }"#), "{outer}");

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
    write_wheel(
        &links_dir.child("hook_helper-0.1.0-py3-none-any.whl"),
        "hook-helper",
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
    return ["hook-helper==0.1.0"]

def get_requires_for_build_wheel(config_settings=None):
    return ["hook-helper==0.1.0"]

def build_editable(wheel_directory, config_settings=None, metadata_directory=None):
    import hook_helper
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
    for operation in ["operation = \"editable\"", "operation = \"wheel\""] {
        assert!(
            member_b_resolutions
                .iter()
                .any(|resolution| resolution.contains(operation)
                    && resolution.contains("hook-helper")),
            "{resolutions}"
        );
    }
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

/// Verify that a host-compatible wheel cannot hide a nested source fallback for a foreign Python
/// target when locking build dependencies.
#[test]
fn lock_build_dependencies_reject_partial_nested_wheel_coverage() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let simple = context.temp_dir.child("simple/helper");
    simple.create_dir_all()?;
    let files = context.temp_dir.child("files");
    files.create_dir_all()?;

    let wheel = "helper-1.0.0-py3-none-any.whl";
    let source = "helper-1.0.0.zip";
    write_wheel(&files.child(wheel), "helper", "1.0.0")?;

    let mut zip = ZipFileWriter::new(Vec::new());
    let entry = ZipEntryBuilder::new("helper-1.0.0/pyproject.toml".into(), Compression::Stored);
    block_on(zip.write_entry_whole(
        entry,
        br#"
        [project]
        name = "helper"
        version = "1.0.0"
        requires-python = ">=3.13,<3.14"

        [build-system]
        requires = []
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
    ))?;
    let entry = ZipEntryBuilder::new("helper-1.0.0/build_backend.py".into(), Compression::Stored);
    block_on(zip.write_entry_whole(
        entry,
        br#"
def get_requires_for_build_wheel(config_settings=None):
    raise RuntimeError("foreign source distribution hook was called")
"#,
    ))?;
    fs_err::write(files.child(source).path(), block_on(zip.close())?)?;

    simple.child("index.html").write_str(&format!(
        r#"
        <a href="../../files/{wheel}" data-upload-time="2024-03-01T00:00:00Z" data-requires-python=">=3.12,<3.13">{wheel}</a>
        <a href="../../files/{source}" data-upload-time="2024-03-01T00:00:00Z" data-requires-python=">=3.13,<3.14">{source}</a>
        "#
    ))?;
    let index =
        Url::from_directory_path(context.temp_dir.child("simple").path()).expect("valid index URL");

    write_executor_build_project(&context, r#""helper>=1""#, Some("helper"))?;
    for pyproject in [
        context.temp_dir.child("pyproject.toml"),
        context.temp_dir.child("dep/pyproject.toml"),
    ] {
        let contents = fs_err::read_to_string(pyproject.path())?;
        pyproject.write_str(&contents.replace(
            r#"requires-python = ">=3.12""#,
            r#"requires-python = ">=3.12,<3.14""#,
        ))?;
    }

    let output = context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index")
        .arg(index.as_str())
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .output()?;
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("helper==1.0.0"), "{stderr}");
    assert!(stderr.contains(">=3.13, <3.14"), "{stderr}");
    assert!(stderr.contains("Python 3.12"), "{stderr}");
    assert!(
        !stderr.contains("foreign source distribution hook was called"),
        "{stderr}"
    );

    Ok(())
}

/// Verify that an executor-ineligible nested source does not block build locking when multiple
/// retained wheels jointly cover every supported Python target.
#[test]
fn lock_build_dependencies_skip_jointly_covered_nested_sdist_hooks() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let simple = context.temp_dir.child("simple/helper");
    simple.create_dir_all()?;
    let files = context.temp_dir.child("files");
    files.create_dir_all()?;

    let host_wheel = "helper-1.0.0-2-py3-none-any.whl";
    let foreign_wheel = "helper-1.0.0-1-py3-none-any.whl";
    let source = "helper-1.0.0.zip";
    write_wheel(&files.child(host_wheel), "helper", "1.0.0")?;
    write_wheel(&files.child(foreign_wheel), "helper", "1.0.0")?;

    let mut zip = ZipFileWriter::new(Vec::new());
    let entry = ZipEntryBuilder::new("helper-1.0.0/pyproject.toml".into(), Compression::Stored);
    block_on(zip.write_entry_whole(
        entry,
        br#"
        [project]
        name = "helper"
        version = "1.0.0"
        requires-python = ">=3.13,<3.14"

        [build-system]
        requires = []
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
    ))?;
    let entry = ZipEntryBuilder::new("helper-1.0.0/build_backend.py".into(), Compression::Stored);
    block_on(zip.write_entry_whole(
        entry,
        br#"
def get_requires_for_build_wheel(config_settings=None):
    raise RuntimeError("jointly covered source distribution hook was called")
"#,
    ))?;
    fs_err::write(files.child(source).path(), block_on(zip.close())?)?;

    simple.child("index.html").write_str(&format!(
        r#"
        <a href="../../files/{host_wheel}" data-upload-time="2024-03-01T00:00:00Z" data-requires-python=">=3.12,<3.13">{host_wheel}</a>
        <a href="../../files/{foreign_wheel}" data-upload-time="2024-03-01T00:00:00Z" data-requires-python=">=3.13,<3.14">{foreign_wheel}</a>
        <a href="../../files/{source}" data-upload-time="2024-03-01T00:00:00Z" data-requires-python=">=3.13,<3.14">{source}</a>
        "#
    ))?;
    let index =
        Url::from_directory_path(context.temp_dir.child("simple").path()).expect("valid index URL");

    write_executor_build_project(&context, r#""helper>=1""#, Some("helper"))?;
    for pyproject in [
        context.temp_dir.child("pyproject.toml"),
        context.temp_dir.child("dep/pyproject.toml"),
    ] {
        let contents = fs_err::read_to_string(pyproject.path())?;
        pyproject.write_str(&contents.replace(
            r#"requires-python = ">=3.12""#,
            r#"requires-python = ">=3.12,<3.14""#,
        ))?;
    }

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
    let helper = package_section(&lock, "helper");
    assert!(helper.contains(host_wheel), "{helper}");
    assert!(helper.contains(foreign_wheel), "{helper}");

    context
        .sync()
        .arg("--index")
        .arg(index.as_str())
        .arg("--no-cache")
        .arg("--frozen")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();

    Ok(())
}

/// Verify that a same-version wheel from a different index cannot make an executor-ineligible
/// nested source distribution appear covered when that wheel will be omitted from the lock.
#[tokio::test]
async fn lock_build_dependencies_reject_cross_index_nested_wheel_coverage() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let simple = context.temp_dir.child("simple/helper");
    simple.create_dir_all()?;
    let files = context.temp_dir.child("files");
    files.create_dir_all()?;

    let host_wheel = "helper-1.0.0-2-py3-none-any.whl";
    let foreign_wheel = "helper-1.0.0-1-py3-none-any.whl";
    let source = "helper-1.0.0.zip";
    write_wheel(&files.child(host_wheel), "helper", "1.0.0")?;
    write_wheel(&files.child(foreign_wheel), "helper", "1.0.0")?;

    let mut zip = ZipFileWriter::new(Vec::new());
    let entry = ZipEntryBuilder::new("helper-1.0.0/pyproject.toml".into(), Compression::Stored);
    block_on(zip.write_entry_whole(
        entry,
        br#"
        [project]
        name = "helper"
        version = "1.0.0"
        requires-python = ">=3.13,<3.14"

        [build-system]
        requires = []
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
    ))?;
    let entry = ZipEntryBuilder::new("helper-1.0.0/build_backend.py".into(), Compression::Stored);
    block_on(zip.write_entry_whole(
        entry,
        br#"
def get_requires_for_build_wheel(config_settings=None):
    raise RuntimeError("cross-index source distribution hook was called")
"#,
    ))?;
    fs_err::write(files.child(source).path(), block_on(zip.close())?)?;

    simple.child("index.html").write_str(&format!(
        r#"
        <a href="../../files/{host_wheel}" data-upload-time="2024-03-01T00:00:00Z" data-requires-python=">=3.12,<3.13">{host_wheel}</a>
        <a href="../../files/{source}" data-upload-time="2024-03-01T00:00:00Z" data-requires-python=">=3.13,<3.14">{source}</a>
        "#
    ))?;
    let index =
        Url::from_directory_path(context.temp_dir.child("simple").path()).expect("valid index URL");

    let server = MockServer::start().await;
    let foreign_url =
        Url::from_file_path(files.child(foreign_wheel).path()).expect("valid file URL");
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            format!(
                r#"<a href="{foreign_url}" data-requires-python=">=3.13,<3.14">{foreign_wheel}</a>"#
            ),
            "text/html",
        ))
        .mount(&server)
        .await;

    write_executor_build_project(&context, r#""helper>=1""#, Some("helper"))?;
    for pyproject in [
        context.temp_dir.child("pyproject.toml"),
        context.temp_dir.child("dep/pyproject.toml"),
    ] {
        let contents = fs_err::read_to_string(pyproject.path())?;
        pyproject.write_str(&contents.replace(
            r#"requires-python = ">=3.12""#,
            r#"requires-python = ">=3.12,<3.14""#,
        ))?;
    }

    let output = context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index")
        .arg(index.as_str())
        .arg("--find-links")
        .arg(server.uri())
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .output()?;
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("helper==1.0.0"), "{stderr}");
    assert!(stderr.contains(">=3.13, <3.14"), "{stderr}");
    assert!(stderr.contains("Python 3.12"), "{stderr}");
    assert!(
        !stderr.contains("cross-index source distribution hook was called"),
        "{stderr}"
    );

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
    assert_eq!(nested_builder_resolutions.len(), 1, "{resolutions}");
    assert!(
        !nested_builder_resolutions[0].contains("stage = "),
        "{resolutions}"
    );
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

/// Verify universal build resolution retains foreign branches without selecting an
/// executor-incompatible wheel when capturing and replaying the active build environment.
#[test]
fn lock_build_dependencies_resolve_wheels_against_executor_python() -> Result<()> {
    let (host_platform, unsupported_platform, foreign_platform, foreign_marker) =
        if cfg!(target_os = "macos") {
            (
                "macosx_10_9_universal2",
                "macosx_99_0_arm64",
                "win_amd64",
                "sys_platform == 'win32'",
            )
        } else if cfg!(target_os = "windows") {
            if cfg!(target_arch = "aarch64") {
                (
                    "win_arm64",
                    "win_amd64",
                    "manylinux_2_17_aarch64.manylinux2014_aarch64",
                    "sys_platform == 'linux'",
                )
            } else {
                (
                    "win_amd64",
                    "win_arm64",
                    "manylinux_2_17_x86_64.manylinux2014_x86_64",
                    "sys_platform == 'linux'",
                )
            }
        } else if cfg!(target_arch = "aarch64") {
            (
                "manylinux_2_17_aarch64.manylinux2014_aarch64",
                "musllinux_99_0_aarch64",
                "win_arm64",
                "sys_platform == 'win32'",
            )
        } else {
            (
                "manylinux_2_17_x86_64.manylinux2014_x86_64",
                "musllinux_99_0_x86_64",
                "win_amd64",
                "sys_platform == 'win32'",
            )
        };

    for incompatible_tag in [
        format!("cp313-cp313-{host_platform}"),
        format!("cp312-cp312d-{host_platform}"),
        format!("cp312-cp312-{unsupported_platform}"),
    ] {
        let context = uv_test::test_context!("3.12");
        let links_dir = context.temp_dir.child("links");
        links_dir.create_dir_all()?;
        write_wheel(
            &links_dir.child("helper-1.0.0-py3-none-any.whl"),
            "helper",
            "1.0.0",
        )?;
        write_wheel_with_requires_and_tag(
            &links_dir.child(format!("helper-2.0.0-{incompatible_tag}.whl")),
            "helper",
            "2.0.0",
            &[],
            &incompatible_tag,
        )?;
        let foreign_tag = format!("py3-none-{foreign_platform}");
        write_wheel_with_requires_and_tag(
            &links_dir.child(format!("foreign_helper-1.0.0-{foreign_tag}.whl")),
            "foreign-helper",
            "1.0.0",
            &[],
            &foreign_tag,
        )?;
        let future_tag = format!("cp313-cp313-{host_platform}");
        write_wheel_with_requires_and_tag(
            &links_dir.child(format!("future_helper-1.0.0-{future_tag}.whl")),
            "future-helper",
            "1.0.0",
            &[],
            &future_tag,
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
        requires = ["helper>=1", "foreign-helper==1.0.0 ; {foreign_marker}", "future-helper==1.0.0 ; python_version >= '3.13'"]
        backend-path = ["."]
        build-backend = "build_backend"
        "#
        ))?;
        dep_dir.child("build_backend.py").write_str(
            r#"
from pathlib import Path
from zipfile import ZipFile

def get_requires_for_build_wheel(config_settings=None):
    import helper
    return []

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    import helper
    filename = "dep-0.1.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("dep.py", "")
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
        let dep = package_section(&lock, "dep");
        assert!(
            dep.contains(r#"{ name = "helper", version = "1.0.0""#),
            "{dep}"
        );
        assert!(
            dep.contains(r#"{ name = "foreign-helper", version = "1.0.0""#),
            "{dep}"
        );
        assert!(
            dep.contains(r#"{ name = "future-helper", version = "1.0.0""#),
            "{dep}"
        );
        assert!(
            lock.contains(&format!("helper-2.0.0-{incompatible_tag}.whl")),
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
            .assert()
            .success();
    }

    Ok(())
}

/// Verify inactive patch-level build requirements do not demand a compatible executor artifact.
#[test]
fn lock_build_dependencies_ignore_inactive_executor_markers() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    let incompatible_tag = unsupported_executor_wheel_tag();
    let incompatible_wheel = format!("future_patch_builder-1.0.0-{incompatible_tag}.whl");
    write_wheel_with_requires_and_tag(
        &links_dir.child(&incompatible_wheel),
        "future-patch-builder",
        "1.0.0",
        &[],
        incompatible_tag,
    )?;
    write_executor_build_project(
        &context,
        r#""future-patch-builder==1.0.0 ; python_full_version > '3.12.99'""#,
        None,
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
    assert!(lock.contains(&incompatible_wheel), "{lock}");
    assert!(lock.contains("python_full_version > '3.12.99'"), "{lock}");

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

/// Verify the active build environment selects a compatible wheel when a higher-priority,
/// same-version wheel only supports another executor.
#[test]
fn lock_build_dependencies_select_compatible_executor_wheel() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    let incompatible_tag = unsupported_executor_wheel_tag();
    let compatible_wheel = "helper-1.0.0-1-py3-none-any.whl";
    let incompatible_wheel = format!("helper-1.0.0-2-{incompatible_tag}.whl");
    write_wheel_with_requires_and_tag(
        &links_dir.child(compatible_wheel),
        "helper",
        "1.0.0",
        &[],
        "py3-none-any",
    )?;
    write_wheel_with_requires_and_tag(
        &links_dir.child(&incompatible_wheel),
        "helper",
        "1.0.0",
        &[],
        incompatible_tag,
    )?;
    write_executor_build_project(&context, r#""helper==1.0.0""#, Some("helper"))?;

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
    assert!(helper.contains(compatible_wheel), "{helper}");
    assert!(helper.contains(&incompatible_wheel), "{helper}");

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

/// Verify the active build environment falls back to a compatible source distribution when all
/// same-version wheels only support another executor.
#[test]
fn lock_build_dependencies_select_compatible_executor_sdist() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    let incompatible_tag = unsupported_executor_wheel_tag();
    let incompatible_wheel = format!("helper-1.0.0-{incompatible_tag}.whl");
    write_build_source(
        &links_dir.child("helper-1.0.0.zip"),
        "helper",
        "1.0.0",
        None,
    )?;
    write_wheel_with_requires_and_tag(
        &links_dir.child(&incompatible_wheel),
        "helper",
        "1.0.0",
        &[],
        incompatible_tag,
    )?;
    write_executor_build_project(&context, r#""helper==1.0.0""#, Some("helper"))?;

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
    assert!(helper.contains("helper-1.0.0.zip"), "{helper}");
    assert!(helper.contains(&incompatible_wheel), "{helper}");

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

/// Verify that selecting a source fallback for the active executor captures the source
/// distribution's nested build-hook requirements even when an ABI-none foreign wheel exists.
#[test]
fn lock_build_dependencies_capture_nested_executor_sdist_hook() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    let incompatible_tag =
        unsupported_executor_wheel_tag().replacen("cp312-cp312-", "py3-none-", 1);
    let incompatible_wheel = format!("helper-1.0.0-{incompatible_tag}.whl");
    write_wheel(
        &links_dir.child("nested_helper-1.0.0-py3-none-any.whl"),
        "nested-helper",
        "1.0.0",
    )?;
    write_build_source(
        &links_dir.child("helper-1.0.0.zip"),
        "helper",
        "1.0.0",
        Some(("nested-helper==1.0.0", "nested_helper")),
    )?;
    write_wheel_with_requires_and_tag(
        &links_dir.child(&incompatible_wheel),
        "helper",
        "1.0.0",
        &[],
        &incompatible_tag,
    )?;
    write_executor_build_project(&context, r#""helper==1.0.0""#, Some("helper"))?;

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
    assert!(helper.contains("helper-1.0.0.zip"), "{helper}");
    assert!(helper.contains(&incompatible_wheel), "{helper}");
    assert!(
        lock.contains("nested_helper-1.0.0-py3-none-any.whl"),
        "{lock}"
    );
    let resolutions = resolution_sections(&lock);
    assert!(
        resolutions.split("[[resolution]]").any(|resolution| {
            resolution.contains("\nname = \"helper\"\n")
                && resolution.contains(r#"{ name = "nested-helper", version = "1.0.0" }"#)
        }),
        "{resolutions}"
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

/// Verify that locking on Python 3.12 never executes the hook of a nested source distribution
/// whose artifact metadata restricts it to Python 3.13.
#[test]
fn lock_build_dependencies_reject_foreign_nested_sdist_hook() -> Result<()> {
    fn write_helper_source(
        path: &ChildPath,
        version: &str,
        requires_python: &str,
        guard: &str,
        value: &str,
    ) -> Result<()> {
        let mut zip = ZipFileWriter::new(Vec::new());
        let entry = ZipEntryBuilder::new(
            format!("helper-{version}/pyproject.toml").into(),
            Compression::Stored,
        );
        block_on(
            zip.write_entry_whole(
                entry,
                format!(
                    r#"
        [project]
        name = "helper"
        version = "{version}"
        requires-python = "{requires_python}"

        [build-system]
        requires = []
        backend-path = ["."]
        build-backend = "build_backend"
        "#
                )
                .as_bytes(),
            ),
        )?;
        let entry = ZipEntryBuilder::new(
            format!("helper-{version}/build_backend.py").into(),
            Compression::Stored,
        );
        block_on(
            zip.write_entry_whole(
                entry,
                format!(
                    r#"
import sys
from pathlib import Path
from zipfile import ZipFile

assert {guard}, "foreign source distribution hook was called"

def get_requires_for_build_wheel(config_settings=None):
    return []

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    filename = "helper-{version}-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("helper.py", "VALUE = '{value}'\n")
        wheel.writestr(
            "helper-{version}.dist-info/METADATA",
            "Metadata-Version: 2.3\nName: helper\nVersion: {version}\nRequires-Python: {requires_python}\n",
        )
        wheel.writestr(
            "helper-{version}.dist-info/WHEEL",
            "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
        )
        wheel.writestr("helper-{version}.dist-info/RECORD", "")
    return filename
"#
                )
                .as_bytes(),
            ),
        )?;
        fs_err::write(path.path(), block_on(zip.close())?)?;

        Ok(())
    }

    let context = uv_test::test_context!("3.12");
    let simple = context.temp_dir.child("simple/helper");
    simple.create_dir_all()?;
    let files = context.temp_dir.child("files");
    files.create_dir_all()?;
    write_helper_source(
        &files.child("helper-1.0.0.zip"),
        "1.0.0",
        ">=3.12,<3.13",
        "sys.version_info < (3, 13)",
        "one",
    )?;
    write_helper_source(
        &files.child("helper-1.1.0.zip"),
        "1.1.0",
        ">=3.13,<3.14",
        "sys.version_info >= (3, 13)",
        "two",
    )?;
    simple.child("index.html").write_str(
        r#"
        <a href="../../files/helper-1.0.0.zip" data-upload-time="2024-03-01T00:00:00Z" data-requires-python=">=3.12,<3.13">helper-1.0.0.zip</a>
        <a href="../../files/helper-1.1.0.zip" data-upload-time="2024-03-01T00:00:00Z" data-requires-python=">=3.13,<3.14">helper-1.1.0.zip</a>
        "#,
    )?;
    let index =
        Url::from_directory_path(context.temp_dir.child("simple").path()).expect("valid index URL");

    let dep = context.temp_dir.child("dep");
    dep.create_dir_all()?;
    dep.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        version = "0.1.0"
        requires-python = ">=3.12,<3.14"

        [build-system]
        requires = ["helper>=1"]
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
    )?;
    dep.child("build_backend.py").write_str(
        r#"
from helper import VALUE

assert VALUE == "one", VALUE

def get_requires_for_build_wheel(config_settings=None):
    return []
"#,
    )?;
    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12,<3.14"
        dependencies = ["dep"]

        [tool.uv.sources]
        dep = { path = "dep" }
        "#,
    )?;

    let output = context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index")
        .arg(index.as_str())
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .output()?;
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("helper==1.1.0"), "{stderr}");
    assert!(stderr.contains(">=3.13, <3.14"), "{stderr}");
    assert!(stderr.contains("Python 3.12"), "{stderr}");
    assert!(
        !stderr.contains("foreign source distribution hook was called"),
        "{stderr}"
    );

    Ok(())
}

/// Verify executor-incompatible direct path and file-URL wheels used as build requirements fail
/// during lock capture instead of being recorded as usable build artifacts.
#[test]
fn lock_build_dependencies_reject_incompatible_direct_executor_wheels() -> Result<()> {
    for source in ["path", "file-url"] {
        let context = uv_test::test_context!("3.12");
        let links_dir = context.temp_dir.child("links");
        links_dir.create_dir_all()?;
        let incompatible_tag = unsupported_executor_wheel_tag();
        let wheel_filename = format!("helper-1.0.0-{incompatible_tag}.whl");
        let helper_wheel = links_dir.child(&wheel_filename);
        write_wheel_with_requires_and_tag(&helper_wheel, "helper", "1.0.0", &[], incompatible_tag)?;
        let requirement = if source == "path" {
            format!(r"'helper @ {}'", helper_wheel.path().display())
        } else {
            let helper_url = Url::from_file_path(helper_wheel.path()).expect("valid file URL");
            format!(r#""helper @ {helper_url}""#)
        };
        write_executor_build_project(&context, &requirement, None)?;

        let output = context
            .lock()
            .arg("--no-index")
            .arg("--preview-features")
            .arg("lock-build-dependencies")
            .output()?;
        assert!(!output.status.success(), "{source}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("Failed to install requirements from `build-system.requires`"),
            "{source}: {stderr}"
        );
        assert!(
            stderr.contains("dependency is incompatible with the current platform"),
            "{source}: {stderr}"
        );
    }

    Ok(())
}

/// Verify a higher-build-tag wheel that is ineligible for the executor cannot hide a usable
/// same-version wheel during universal build resolution or concrete artifact projection.
#[tokio::test]
async fn lock_build_dependencies_select_eligible_executor_wheel() -> Result<()> {
    #[derive(Clone, Copy)]
    enum IndexKind {
        Simple,
        FindLinks,
        NamedFlat,
    }

    fn write_helper_wheel(path: &ChildPath, value: &str) -> Result<()> {
        let mut zip = ZipFileWriter::new(Vec::new());
        let entry = ZipEntryBuilder::new("helper.py".into(), Compression::Stored);
        block_on(zip.write_entry_whole(entry, value.as_bytes()))?;
        let entry = ZipEntryBuilder::new(
            "helper-1.0.0.dist-info/METADATA".into(),
            Compression::Stored,
        );
        block_on(zip.write_entry_whole(
            entry,
            b"Metadata-Version: 2.3\nName: helper\nVersion: 1.0.0\n",
        ))?;
        let entry =
            ZipEntryBuilder::new("helper-1.0.0.dist-info/WHEEL".into(), Compression::Stored);
        block_on(zip.write_entry_whole(
            entry,
            b"Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
        ))?;
        let entry =
            ZipEntryBuilder::new("helper-1.0.0.dist-info/RECORD".into(), Compression::Stored);
        block_on(zip.write_entry_whole(entry, b""))?;
        fs_err::write(path.path(), block_on(zip.close())?)?;

        Ok(())
    }

    for (ineligible_metadata, requirement, index_kind, bad_value, expected_value) in [
        (
            r#"data-requires-python=">=3.13""#,
            "helper==1.0.0",
            IndexKind::Simple,
            "raise RuntimeError('selected ineligible wheel')\n",
            "compatible",
        ),
        (
            r#"data-yanked="bad build""#,
            "helper>=1",
            IndexKind::Simple,
            "raise RuntimeError('selected ineligible wheel')\n",
            "compatible",
        ),
        (
            r#"data-requires-python=">=3.13""#,
            "helper==1.0.0",
            IndexKind::FindLinks,
            "raise RuntimeError('selected ineligible wheel')\n",
            "compatible",
        ),
        (
            r#"data-requires-python=">=3.13""#,
            "helper==1.0.0",
            IndexKind::NamedFlat,
            "raise RuntimeError('selected ineligible wheel')\n",
            "compatible",
        ),
        (
            r#"data-yanked="bad build""#,
            "helper>=1",
            IndexKind::FindLinks,
            "raise RuntimeError('selected ineligible wheel')\n",
            "compatible",
        ),
        (
            r#"data-yanked="bad build""#,
            "helper>=1",
            IndexKind::NamedFlat,
            "raise RuntimeError('selected ineligible wheel')\n",
            "compatible",
        ),
        (
            r#"data-yanked="allowed exact pin""#,
            "helper==1.0.0",
            IndexKind::FindLinks,
            "VALUE = 'yanked'\n",
            "yanked",
        ),
        (
            r#"data-yanked="allowed exact pin""#,
            "helper==1.0.0",
            IndexKind::NamedFlat,
            "VALUE = 'yanked'\n",
            "yanked",
        ),
    ] {
        let context = uv_test::test_context!("3.12");
        let simple = context.temp_dir.child("simple/helper");
        simple.create_dir_all()?;
        let files = context.temp_dir.child("files");
        files.create_dir_all()?;
        let good = "helper-1.0.0-1-py3-none-any.whl";
        let bad = "helper-1.0.0-2-py3-none-any.whl";
        write_helper_wheel(&files.child(good), "VALUE = 'compatible'\n")?;
        write_helper_wheel(&files.child(bad), bad_value)?;
        simple.child("index.html").write_str(&format!(
            r#"
            <a href="../../files/{good}" data-upload-time="2024-03-01T00:00:00Z" data-requires-python=">=3.12">{good}</a>
            <a href="../../files/{bad}" data-upload-time="2024-03-01T00:00:00Z" {ineligible_metadata}>{bad}</a>
            "#
        ))?;
        let index = Url::from_directory_path(context.temp_dir.child("simple").path())
            .expect("valid index URL");
        let server = MockServer::start().await;
        if !matches!(index_kind, IndexKind::Simple) {
            let good_url = Url::from_file_path(files.child(good).path()).expect("valid file URL");
            let bad_url = Url::from_file_path(files.child(bad).path()).expect("valid file URL");
            Mock::given(method("GET"))
                .and(path("/"))
                .respond_with(ResponseTemplate::new(200).set_body_raw(
                    format!(
                        r#"
                        <a href="{good_url}" data-requires-python=">=3.12">{good}</a>
                        <a href="{bad_url}" {ineligible_metadata}>{bad}</a>
                        "#
                    ),
                    "text/html",
                ))
                .mount(&server)
                .await;
        }

        let index_config = if matches!(index_kind, IndexKind::NamedFlat) {
            let index = server.uri().replacen("://", "://user:secret@", 1);
            format!(
                r#"
                [[tool.uv.index]]
                name = "local"
                url = "{index}"
                format = "flat"
                explicit = true
                "#
            )
        } else {
            String::new()
        };
        let helper_source = if matches!(index_kind, IndexKind::NamedFlat) {
            r#"helper = { index = "local" }"#
        } else {
            ""
        };

        let dep = context.temp_dir.child("dep");
        dep.create_dir_all()?;
        dep.child("pyproject.toml").write_str(&format!(
            r#"
            [project]
            name = "dep"
            version = "0.1.0"
            requires-python = ">=3.12"

            [build-system]
            requires = ["{requirement}"]
            backend-path = ["."]
            build-backend = "build_backend"

            {index_config}

            [tool.uv.sources]
            {helper_source}
            "#
        ))?;
        dep.child("build_backend.py").write_str(&format!(
            r#"
from pathlib import Path
from zipfile import ZipFile

def get_requires_for_build_wheel(config_settings=None):
    from helper import VALUE
    assert VALUE == "{expected_value}"
    return []

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    from helper import VALUE
    assert VALUE == "{expected_value}"
    filename = "dep-0.1.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("dep.py", "")
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
        let runtime_dependency = if matches!(index_kind, IndexKind::Simple)
            && ineligible_metadata.contains("requires-python")
        {
            r#", "helper==1.0.0; python_full_version >= '3.13'""#
        } else {
            ""
        };
        context
            .temp_dir
            .child("pyproject.toml")
            .write_str(&format!(
                r#"
            [project]
            name = "project"
            version = "0.1.0"
            requires-python = ">=3.12"
            dependencies = ["dep"{runtime_dependency}]

            {index_config}

            [tool.uv.sources]
            dep = {{ path = "dep" }}
            {helper_source}
            "#,
            ))?;

        let mut lock = context.lock();
        match index_kind {
            IndexKind::Simple => {
                lock.arg("--index").arg(index.as_str());
            }
            IndexKind::FindLinks => {
                lock.arg("--no-index").arg("--find-links").arg(server.uri());
            }
            IndexKind::NamedFlat => {
                lock.arg("--default-index")
                    .arg(format!("{}/simple", server.uri()));
            }
        }
        lock.arg("--preview-features")
            .arg("lock-build-dependencies")
            .assert()
            .success();

        let lock = context.read("uv.lock");
        if matches!(index_kind, IndexKind::NamedFlat) {
            assert!(!lock.contains("user:secret@"), "{lock}");
        }
        let helper = package_section(&lock, "helper");
        assert!(helper.contains(good), "{helper}");
        if ineligible_metadata.contains(r#"data-yanked="bad build""#) {
            assert!(!helper.contains(bad), "{helper}");
        } else {
            assert!(helper.contains(bad), "{helper}");
        }

        let mut sync = context.sync();
        match index_kind {
            IndexKind::Simple => {
                sync.arg("--index").arg(index.as_str());
            }
            IndexKind::FindLinks => {
                sync.arg("--no-index").arg("--find-links").arg(server.uri());
            }
            IndexKind::NamedFlat => {
                sync.arg("--default-index")
                    .arg(format!("{}/simple", server.uri()));
            }
        }
        sync.arg("--no-cache")
            .arg("--frozen")
            .arg("--preview-features")
            .arg("lock-build-dependencies")
            .assert()
            .success();
    }

    Ok(())
}

/// Verify artifact selection remains scoped when one isolated build allows a yanked wheel and
/// another build of the same version does not.
#[tokio::test]
async fn lock_build_dependencies_scope_yanked_executor_wheel() -> Result<()> {
    fn write_helper_wheel(path: &ChildPath, value: &str) -> Result<()> {
        let mut zip = ZipFileWriter::new(Vec::new());
        let entry = ZipEntryBuilder::new("helper.py".into(), Compression::Stored);
        block_on(zip.write_entry_whole(entry, value.as_bytes()))?;
        let entry = ZipEntryBuilder::new(
            "helper-1.0.0.dist-info/METADATA".into(),
            Compression::Stored,
        );
        block_on(zip.write_entry_whole(
            entry,
            b"Metadata-Version: 2.3\nName: helper\nVersion: 1.0.0\n",
        ))?;
        let entry =
            ZipEntryBuilder::new("helper-1.0.0.dist-info/WHEEL".into(), Compression::Stored);
        block_on(zip.write_entry_whole(
            entry,
            b"Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
        ))?;
        let entry =
            ZipEntryBuilder::new("helper-1.0.0.dist-info/RECORD".into(), Compression::Stored);
        block_on(zip.write_entry_whole(entry, b""))?;
        fs_err::write(path.path(), block_on(zip.close())?)?;

        Ok(())
    }

    let context = uv_test::test_context!("3.12");
    let simple = context.temp_dir.child("simple/helper");
    simple.create_dir_all()?;
    let files = context.temp_dir.child("files");
    files.create_dir_all()?;
    let unyanked = "helper-1.0.0-1-py3-none-any.whl";
    let yanked = "helper-1.0.0-2-py3-none-any.whl";
    write_helper_wheel(&files.child(unyanked), "VALUE = 'unyanked'\n")?;
    write_helper_wheel(&files.child(yanked), "VALUE = 'yanked'\n")?;
    simple.child("index.html").write_str(&format!(
        r#"
        <a href="../../files/{unyanked}" data-upload-time="2024-03-01T00:00:00Z" data-requires-python=">=3.12">{unyanked}</a>
        <a href="../../files/{yanked}" data-upload-time="2024-03-01T00:00:00Z" data-yanked="bad build">{yanked}</a>
        "#
    ))?;
    let index =
        Url::from_directory_path(context.temp_dir.child("simple").path()).expect("valid index URL");

    for (name, requirement, expected) in [
        ("dep-a", "helper>=1", "unyanked"),
        ("dep-b", "helper==1.0.0", "yanked"),
    ] {
        let dep = context.temp_dir.child(name);
        dep.create_dir_all()?;
        dep.child("pyproject.toml").write_str(&format!(
            r#"
            [project]
            name = "{name}"
            version = "0.1.0"
            requires-python = ">=3.12"

            [build-system]
            requires = ["{requirement}"]
            backend-path = ["."]
            build-backend = "build_backend"
            "#
        ))?;
        let module = name.replace('-', "_");
        dep.child("build_backend.py").write_str(&format!(
            r#"
from pathlib import Path
from zipfile import ZipFile

def get_requires_for_build_wheel(config_settings=None):
    from helper import VALUE
    assert VALUE == "{expected}"
    return []

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    from helper import VALUE
    assert VALUE == "{expected}"
    filename = "{module}-0.1.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("{module}.py", "")
        wheel.writestr(
            "{module}-0.1.0.dist-info/METADATA",
            "Metadata-Version: 2.3\nName: {name}\nVersion: 0.1.0\n",
        )
        wheel.writestr(
            "{module}-0.1.0.dist-info/WHEEL",
            "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
        )
        wheel.writestr("{module}-0.1.0.dist-info/RECORD", "")
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
        dependencies = ["dep-a", "dep-b"]

        [tool.uv.sources]
        dep-a = { path = "dep-a" }
        dep-b = { path = "dep-b" }
        "#,
    )?;

    context
        .lock()
        .arg("--index")
        .arg(index.as_str())
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();

    let lock = context.read("uv.lock");
    let helper = package_section(&lock, "helper");
    assert!(helper.contains(unyanked), "{helper}");
    assert!(helper.contains(yanked), "{helper}");

    context
        .sync()
        .arg("--index")
        .arg(index.as_str())
        .arg("--no-cache")
        .arg("--frozen")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();

    Ok(())
}

/// Verify an exact build requirement retains a yanked wheel for another executor even when the
/// active executor selects an unyanked wheel.
#[tokio::test]
async fn lock_build_dependencies_retain_yanked_foreign_executor_wheel() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let simple = context.temp_dir.child("simple/helper");
    simple.create_dir_all()?;
    let files = context.temp_dir.child("files");
    files.create_dir_all()?;

    let host_wheel = "helper-1.0.0-2-py3-none-any.whl";
    let foreign_tag = unsupported_executor_wheel_tag();
    let foreign_wheel = format!("helper-1.0.0-1-{foreign_tag}.whl");
    write_wheel(&files.child(host_wheel), "helper", "1.0.0")?;
    write_wheel_with_requires_and_tag(
        &files.child(&foreign_wheel),
        "helper",
        "1.0.0",
        &[],
        foreign_tag,
    )?;
    simple.child("index.html").write_str(&format!(
        r#"
        <a href="../../files/{host_wheel}" data-upload-time="2024-03-01T00:00:00Z" data-requires-python=">=3.12">{host_wheel}</a>
        <a href="../../files/{foreign_wheel}" data-upload-time="2024-03-01T00:00:00Z" data-yanked="allowed exact pin">{foreign_wheel}</a>
        "#
    ))?;
    let index =
        Url::from_directory_path(context.temp_dir.child("simple").path()).expect("valid index URL");

    write_executor_build_project(&context, r#""helper==1.0.0""#, Some("helper"))?;
    context
        .lock()
        .arg("--index")
        .arg(index.as_str())
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();

    let lock = context.read("uv.lock");
    let helpers = lock
        .split("\n[[package]]")
        .filter(|package| package.contains("\nname = \"helper\""))
        .collect::<Vec<_>>();
    assert!(!helpers.is_empty(), "{lock}");
    for helper in helpers {
        assert!(helper.contains(host_wheel), "{helper}");
        assert!(helper.contains(&foreign_wheel), "{helper}");
    }

    Ok(())
}

/// Verify a yanked or Python-ineligible source artifact is never executed when frozen replay is
/// forced to fall back from the captured build wheel.
#[tokio::test]
async fn lock_build_dependencies_reject_ineligible_sdist_fallback() -> Result<()> {
    for (sdist_metadata, requirement) in [
        (r#"data-yanked="bad source""#, "helper>=1"),
        (r#"data-requires-python=">=3.13""#, "helper==1.0.0"),
    ] {
        let context = uv_test::test_context!("3.12");
        let simple = context.temp_dir.child("simple/helper");
        simple.create_dir_all()?;
        let files = context.temp_dir.child("files");
        files.create_dir_all()?;

        let wheel = "helper-1.0.0-py3-none-any.whl";
        let source = "helper-1.0.0.zip";
        write_wheel(&files.child(wheel), "helper", "1.0.0")?;
        let source_dist = files.child(source);
        let mut zip = ZipFileWriter::new(Vec::new());
        let entry = ZipEntryBuilder::new("helper-1.0.0/pyproject.toml".into(), Compression::Stored);
        block_on(zip.write_entry_whole(
            entry,
            br#"
            [project]
            name = "helper"
            version = "1.0.0"
            requires-python = ">=3.12"

            [build-system]
            requires = []
            backend-path = ["."]
            build-backend = "build_backend"
            "#,
        ))?;
        let entry =
            ZipEntryBuilder::new("helper-1.0.0/build_backend.py".into(), Compression::Stored);
        block_on(zip.write_entry_whole(
            entry,
            br#"
from pathlib import Path
from zipfile import ZipFile

def get_requires_for_build_wheel(config_settings=None):
    return []

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    filename = "helper-1.0.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("helper.py", 'raise RuntimeError("selected ineligible source distribution")\n')
        wheel.writestr(
            "helper-1.0.0.dist-info/METADATA",
            "Metadata-Version: 2.3\nName: helper\nVersion: 1.0.0\n",
        )
        wheel.writestr(
            "helper-1.0.0.dist-info/WHEEL",
            "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
        )
        wheel.writestr("helper-1.0.0.dist-info/RECORD", "")
    return filename
"#,
        ))?;
        fs_err::write(source_dist.path(), block_on(zip.close())?)?;

        simple.child("index.html").write_str(&format!(
            r#"
            <a href="../../files/{wheel}" data-upload-time="2024-03-01T00:00:00Z" data-requires-python=">=3.12">{wheel}</a>
            <a href="../../files/{source}" data-upload-time="2024-03-01T00:00:00Z" {sdist_metadata}>{source}</a>
            "#
        ))?;
        let index = Url::from_directory_path(context.temp_dir.child("simple").path())
            .expect("valid index URL");

        write_executor_build_project(&context, &format!(r#""{requirement}""#), Some("helper"))?;
        for pyproject in [
            context.temp_dir.child("pyproject.toml"),
            context.temp_dir.child("dep/pyproject.toml"),
        ] {
            let contents = fs_err::read_to_string(pyproject.path())?;
            pyproject.write_str(&contents.replace(
                r#"requires-python = ">=3.12""#,
                r#"requires-python = ">=3.12,<4""#,
            ))?;
        }
        context
            .lock()
            .arg("--index")
            .arg(index.as_str())
            .arg("--preview-features")
            .arg("lock-build-dependencies")
            .assert()
            .success();

        let lock = context.read("uv.lock");
        let helper = package_section(&lock, "helper");
        assert!(helper.contains(wheel), "{helper}");

        let output = context
            .sync()
            .arg("--index")
            .arg(index.as_str())
            .arg("--no-binary-package")
            .arg("helper")
            .arg("--no-cache")
            .arg("--frozen")
            .arg("--preview-features")
            .arg("lock-build-dependencies")
            .output()?;
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("can't be installed because it is marked as `--no-binary` but has no source distribution"),
            "{stderr}"
        );
        assert!(
            !stderr.contains("selected ineligible source distribution"),
            "{stderr}"
        );
    }

    Ok(())
}

/// Verify frozen build replay cannot select a wheel excluded when the build was captured from an
/// eligible source distribution.
#[tokio::test]
async fn lock_build_dependencies_replay_ignores_excluded_executor_wheel() -> Result<()> {
    fn write_raising_helper_wheel(path: &ChildPath) -> Result<()> {
        let mut zip = ZipFileWriter::new(Vec::new());
        let entry = ZipEntryBuilder::new("helper.py".into(), Compression::Stored);
        block_on(zip.write_entry_whole(entry, b"raise RuntimeError('selected excluded wheel')\n"))?;
        let entry = ZipEntryBuilder::new(
            "helper-1.0.0.dist-info/METADATA".into(),
            Compression::Stored,
        );
        block_on(zip.write_entry_whole(
            entry,
            b"Metadata-Version: 2.3\nName: helper\nVersion: 1.0.0\n",
        ))?;
        let entry =
            ZipEntryBuilder::new("helper-1.0.0.dist-info/WHEEL".into(), Compression::Stored);
        block_on(zip.write_entry_whole(
            entry,
            b"Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
        ))?;
        let entry =
            ZipEntryBuilder::new("helper-1.0.0.dist-info/RECORD".into(), Compression::Stored);
        block_on(zip.write_entry_whole(entry, b""))?;
        fs_err::write(path.path(), block_on(zip.close())?)?;

        Ok(())
    }

    let context = uv_test::test_context!("3.12");
    let simple = context.temp_dir.child("simple/helper");
    simple.create_dir_all()?;
    let files = context.temp_dir.child("files");
    files.create_dir_all()?;
    write_build_source(&files.child("helper-1.0.0.zip"), "helper", "1.0.0", None)?;
    write_raising_helper_wheel(&files.child("helper-1.0.0-2-py3-none-any.whl"))?;
    simple.child("index.html").write_str(
        r#"
        <a href="../../files/helper-1.0.0.zip" data-upload-time="2024-03-01T00:00:00Z" data-requires-python=">=3.12">helper-1.0.0.zip</a>
        <a href="../../files/helper-1.0.0-2-py3-none-any.whl" data-upload-time="2025-03-01T00:00:00Z" data-requires-python=">=3.12">helper-1.0.0-2-py3-none-any.whl</a>
        "#,
    )?;
    let index =
        Url::from_directory_path(context.temp_dir.child("simple").path()).expect("valid index URL");

    let dep = context.temp_dir.child("dep");
    dep.create_dir_all()?;
    dep.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["helper==1.0.0"]
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
    )?;
    dep.child("build_backend.py").write_str(
        r#"
from pathlib import Path
from zipfile import ZipFile

def get_requires_for_build_wheel(config_settings=None):
    import helper
    return []

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    import helper
    filename = "dep-0.1.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("dep.py", "")
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
        .arg("--index")
        .arg(index.as_str())
        .arg("--exclude-newer")
        .arg("2024-03-25T00:00:00Z")
        .arg("--no-binary-package")
        .arg("helper")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();

    context
        .sync()
        .arg("--index")
        .arg(index.as_str())
        .arg("--no-cache")
        .arg("--frozen")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();

    Ok(())
}

/// Verify an unpinned, yanked higher-build-tag runtime wheel is not captured and selected during
/// cold frozen replay.
#[test]
fn lock_build_dependencies_replay_ignores_yanked_runtime_wheel() -> Result<()> {
    fn write_helper_wheel(path: &ChildPath, value: &str) -> Result<()> {
        let mut zip = ZipFileWriter::new(Vec::new());
        let entry = ZipEntryBuilder::new("helper.py".into(), Compression::Stored);
        block_on(zip.write_entry_whole(entry, value.as_bytes()))?;
        let entry = ZipEntryBuilder::new(
            "helper-1.0.0.dist-info/METADATA".into(),
            Compression::Stored,
        );
        block_on(zip.write_entry_whole(
            entry,
            b"Metadata-Version: 2.3\nName: helper\nVersion: 1.0.0\n",
        ))?;
        let entry =
            ZipEntryBuilder::new("helper-1.0.0.dist-info/WHEEL".into(), Compression::Stored);
        block_on(zip.write_entry_whole(
            entry,
            b"Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
        ))?;
        let entry =
            ZipEntryBuilder::new("helper-1.0.0.dist-info/RECORD".into(), Compression::Stored);
        block_on(zip.write_entry_whole(entry, b""))?;
        fs_err::write(path.path(), block_on(zip.close())?)?;

        Ok(())
    }

    let context = uv_test::test_context!("3.12");
    let simple = context.temp_dir.child("simple/helper");
    simple.create_dir_all()?;
    let files = context.temp_dir.child("files");
    files.create_dir_all()?;
    let good = "helper-1.0.0-1-py3-none-any.whl";
    let yanked = "helper-1.0.0-2-py3-none-any.whl";
    write_helper_wheel(&files.child(good), "VALUE = 'compatible'\n")?;
    write_helper_wheel(
        &files.child(yanked),
        "raise RuntimeError('selected yanked runtime wheel')\n",
    )?;
    simple.child("index.html").write_str(&format!(
        r#"
        <a href="../../files/{good}" data-upload-time="2024-03-01T00:00:00Z" data-requires-python=">=3.12">{good}</a>
        <a href="../../files/{yanked}" data-upload-time="2024-03-01T00:00:00Z" data-yanked="bad runtime wheel">{yanked}</a>
        "#
    ))?;
    let index =
        Url::from_directory_path(context.temp_dir.child("simple").path()).expect("valid index URL");

    write_executor_build_project(&context, "", None)?;
    let project = context.temp_dir.child("pyproject.toml");
    let contents = fs_err::read_to_string(project.path())?;
    project.write_str(&contents.replace(
        r#"dependencies = ["dep"]"#,
        r#"dependencies = ["dep", "helper>=1"]"#,
    ))?;

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
    let helper = package_section(&lock, "helper");
    assert!(helper.contains(good), "{helper}");
    assert!(!helper.contains(yanked), "{helper}");

    context
        .sync()
        .arg("--index")
        .arg(index.as_str())
        .arg("--no-cache")
        .arg("--frozen")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();
    uv_snapshot!(context.filters(), context
        .run()
        .arg("--no-sync")
        .arg("python")
        .arg("-c")
        .arg("import helper; print(helper.VALUE)"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    compatible

    ----- stderr -----
    ");

    Ok(())
}

/// Verify a higher-build-tag runtime wheel that is ineligible for the active Python cannot be
/// selected during cold frozen replay when a same-version compatible wheel is available.
#[test]
fn lock_build_dependencies_replay_selects_python_compatible_runtime_wheel() -> Result<()> {
    fn write_helper_wheel(path: &ChildPath, value: &str) -> Result<()> {
        let mut zip = ZipFileWriter::new(Vec::new());
        let entry = ZipEntryBuilder::new("helper.py".into(), Compression::Stored);
        block_on(zip.write_entry_whole(entry, value.as_bytes()))?;
        let entry = ZipEntryBuilder::new(
            "helper-1.0.0.dist-info/METADATA".into(),
            Compression::Stored,
        );
        block_on(zip.write_entry_whole(
            entry,
            b"Metadata-Version: 2.3\nName: helper\nVersion: 1.0.0\n",
        ))?;
        let entry =
            ZipEntryBuilder::new("helper-1.0.0.dist-info/WHEEL".into(), Compression::Stored);
        block_on(zip.write_entry_whole(
            entry,
            b"Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
        ))?;
        let entry =
            ZipEntryBuilder::new("helper-1.0.0.dist-info/RECORD".into(), Compression::Stored);
        block_on(zip.write_entry_whole(entry, b""))?;
        fs_err::write(path.path(), block_on(zip.close())?)?;

        Ok(())
    }

    let context = uv_test::test_context!("3.12");
    let simple = context.temp_dir.child("simple/helper");
    simple.create_dir_all()?;
    let files = context.temp_dir.child("files");
    files.create_dir_all()?;
    let compatible = "helper-1.0.0-1-py3-none-any.whl";
    let foreign = "helper-1.0.0-2-py3-none-any.whl";
    write_helper_wheel(&files.child(compatible), "VALUE = 'compatible'\n")?;
    write_helper_wheel(
        &files.child(foreign),
        "raise RuntimeError('selected Python-incompatible runtime wheel')\n",
    )?;
    simple.child("index.html").write_str(&format!(
        r#"
        <a href="../../files/{compatible}" data-upload-time="2024-03-01T00:00:00Z" data-requires-python=">=3.12,<3.14">{compatible}</a>
        <a href="../../files/{foreign}" data-upload-time="2024-03-01T00:00:00Z" data-requires-python=">=3.12,<3.14">{foreign}</a>
        "#
    ))?;
    let index =
        Url::from_directory_path(context.temp_dir.child("simple").path()).expect("valid index URL");

    write_executor_build_project(&context, "", None)?;
    for pyproject in [
        context.temp_dir.child("pyproject.toml"),
        context.temp_dir.child("dep/pyproject.toml"),
    ] {
        let contents = fs_err::read_to_string(pyproject.path())?;
        let contents = contents.replace(
            r#"requires-python = ">=3.12""#,
            r#"requires-python = ">=3.12,<3.14""#,
        );
        pyproject.write_str(&contents)?;
    }
    let project = context.temp_dir.child("pyproject.toml");
    let contents = fs_err::read_to_string(project.path())?;
    project.write_str(&contents.replace(
        r#"dependencies = ["dep"]"#,
        r#"dependencies = ["dep", "helper==1.0.0"]"#,
    ))?;

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
    let original_helper = package_section(&lock, "helper");
    let helper = original_helper
        .lines()
        .map(|line| {
            if line.contains(compatible) {
                line.replace(">=3.12, <3.14", ">=3.12, <3.13")
            } else if line.contains(foreign) {
                line.replace(">=3.12, <3.14", ">=3.13, <3.14")
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let lock = lock.replacen(original_helper, &helper, 1);
    context.temp_dir.child("uv.lock").write_str(&lock)?;
    let helper = package_section(&lock, "helper");
    assert!(helper.contains(compatible), "{helper}");
    assert!(helper.contains(foreign), "{helper}");
    assert!(helper.contains(">=3.12, <3.13"), "{helper}");
    assert!(helper.contains(">=3.13, <3.14"), "{helper}");

    context
        .sync()
        .arg("--index")
        .arg(index.as_str())
        .arg("--no-cache")
        .arg("--frozen")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();
    uv_snapshot!(context.filters(), context
        .run()
        .arg("--no-sync")
        .arg("python")
        .arg("-c")
        .arg("import helper; print(helper.VALUE)"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    compatible

    ----- stderr -----
    ");

    Ok(())
}

/// Verify executor-only wheel eligibility does not suppress normal runtime Python forks.
#[cfg(feature = "test-universal")]
#[test]
fn lock_runtime_requires_python_fork_is_preserved() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let simple = context.temp_dir.child("simple/helper");
    simple.create_dir_all()?;
    let files = context.temp_dir.child("files");
    files.create_dir_all()?;
    let older = "helper-1.0.0-py3-none-any.whl";
    let newer = "helper-2.0.0-py3-none-any.whl";
    write_wheel(&files.child(older), "helper", "1.0.0")?;
    write_wheel(&files.child(newer), "helper", "2.0.0")?;
    simple.child("index.html").write_str(&format!(
        r#"
        <a href="../../files/{older}" data-upload-time="2024-03-01T00:00:00Z" data-requires-python=">=3.12">{older}</a>
        <a href="../../files/{newer}" data-upload-time="2024-03-01T00:00:00Z" data-requires-python=">=3.13">{newer}</a>
        "#
    ))?;
    let index =
        Url::from_directory_path(context.temp_dir.child("simple").path()).expect("valid index URL");
    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["helper>=1"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock().arg("--index").arg(index.as_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    let runtime = lock
        .lines()
        .filter(|line| {
            line.starts_with("resolution-markers = [")
                || line.trim_start().starts_with("\"python_full_version")
                || line.starts_with("name = \"helper\"")
                || line.starts_with("version = \"1.0.0\"")
                || line.starts_with("version = \"2.0.0\"")
                || line.contains(older)
                || line.contains(newer)
                || line.trim_start().starts_with("{ name = \"helper\"")
        })
        .collect::<Vec<_>>()
        .join("\n");
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(runtime, @r#"
        resolution-markers = [
            "python_full_version >= '3.13'",
            "python_full_version < '3.13'",
        name = "helper"
        version = "1.0.0"
        resolution-markers = [
            "python_full_version < '3.13'",
            { path = "[TEMP_DIR]/files/helper-1.0.0-py3-none-any.whl", upload-time = "2024-03-01T00:00:00Z" },
        name = "helper"
        version = "2.0.0"
        resolution-markers = [
            "python_full_version >= '3.13'",
            { path = "[TEMP_DIR]/files/helper-2.0.0-py3-none-any.whl", upload-time = "2024-03-01T00:00:00Z" },
            { name = "helper", version = "1.0.0", source = { registry = "[TEMP_DIR]/simple/" }, marker = "python_full_version < '3.13'" },
            { name = "helper", version = "2.0.0", source = { registry = "[TEMP_DIR]/simple/" }, marker = "python_full_version >= '3.13'" },
        "#);
    });

    uv_snapshot!(context.filters(), context.lock().arg("--index").arg(index.as_str()).arg("--locked"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    ");

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
        .filter(|resolution| resolution.contains("\nname = \"project\""))
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

/// Verify that an editable source missing one operation is completed when relocking, even when
/// the remaining operation has the same empty build roots.
#[test]
fn lock_build_dependencies_captures_missing_editable_operation() -> Result<()> {
    let context = uv_test::test_context!("3.12");

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
    return []

def get_requires_for_build_wheel(config_settings=None):
    return []

def build_editable(wheel_directory, config_settings=None, metadata_directory=None):
    return _build(wheel_directory)

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
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
        .arg("--no-index")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();

    let lock = context.read("uv.lock");
    let resolutions = resolution_sections(&lock);
    let wheel_resolutions = resolutions
        .split("\n\n")
        .filter(|resolution| {
            resolution.contains("\nname = \"project\"")
                && resolution.contains("operation = \"wheel\"")
        })
        .collect::<Vec<_>>();
    assert!(!wheel_resolutions.is_empty(), "{lock}");
    let mut incomplete_lock = lock.clone();
    for wheel_resolution in wheel_resolutions {
        incomplete_lock = incomplete_lock.replacen(&format!("{wheel_resolution}\n\n"), "", 1);
    }
    assert_ne!(incomplete_lock, lock, "{lock}");
    fs_err::write(context.temp_dir.child("uv.lock"), incomplete_lock)?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--no-index")
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    ");

    let lock = context.read("uv.lock");
    let resolutions = resolution_sections(&lock);
    let project_resolutions = resolutions
        .split("[[resolution]]")
        .filter(|resolution| resolution.contains("\nname = \"project\""))
        .collect::<Vec<_>>();
    assert!(
        project_resolutions
            .iter()
            .any(|resolution| resolution.contains("operation = \"editable\"")),
        "{resolutions}"
    );
    assert!(
        project_resolutions
            .iter()
            .any(|resolution| resolution.contains("operation = \"wheel\"")),
        "{resolutions}"
    );

    context
        .sync()
        .arg("--no-editable")
        .arg("--no-index")
        .arg("--no-cache")
        .arg("--frozen")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();

    Ok(())
}

/// Verify that a captured build target that no longer covers an unconditional source is relocked.
#[test]
fn lock_build_dependencies_rejects_missing_target_coverage() -> Result<()> {
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
        .arg("--no-index")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();

    let lock = context.read("uv.lock");
    let resolutions = resolution_sections(&lock);
    let dep_resolutions = resolutions
        .split("\n\n")
        .filter(|resolution| {
            resolution.contains("\nname = \"dep\"") && resolution.contains("operation = \"wheel\"")
        })
        .collect::<Vec<_>>();
    assert!(!dep_resolutions.is_empty(), "{lock}");
    let mut incomplete_lock = lock.clone();
    for resolution in dep_resolutions {
        incomplete_lock = incomplete_lock.replacen(
            resolution,
            &format!("{resolution}\ntarget = {{ marker = \"sys_platform == 'linux'\" }}"),
            1,
        );
    }
    assert_ne!(incomplete_lock, lock, "{lock}");
    fs_err::write(context.temp_dir.child("uv.lock"), incomplete_lock)?;

    uv_snapshot!(context.filters(), context
        .lock()
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

    context
        .lock()
        .arg("--no-index")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();

    context
        .sync()
        .arg("--no-index")
        .arg("--no-cache")
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
        id = "build:flit-core:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
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
        id = "build:wheel:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
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
        sdist = { url = "https://files.pythonhosted.org/packages/c4/e6/c1ac50fe3eebb38a155155711e6e864e254ce4b6e17fe2429b4c4d5b9e80/flit_core-3.9.0.tar.gz", hash = "sha256:72ad266176c4a3fcfab5f2930d76896059851240570ce9a98733b658cb786eba", size = 41917, upload-time = "2023-05-14T14:48:51.809Z", requires-python = ">=3.6" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/38/45/618e84e49a6c51e5dd15565ec2fcd82ab273434f236b8f108f065ded517a/flit_core-3.9.0-py3-none-any.whl", hash = "sha256:7aada352fb0c7f5538c4fafeddf314d3a6a92ee8e2b1de70482329e42de70301", size = 63141, upload-time = "2023-05-14T14:48:49.24Z", requires-python = ">=3.6" },
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
        sdist = { url = "https://files.pythonhosted.org/packages/4d/5b/dc575711b6b8f2f866131a40d053e30e962e633b332acf7cd2c24843d83d/setuptools-69.2.0.tar.gz", hash = "sha256:0ff4183f8f42cd8fa3acea16c45205521a4ef28f73c6391d8a25e92893134f2e", size = 2222950, upload-time = "2024-03-13T11:20:59.219Z", requires-python = ">=3.8" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/92/e1/1c8bb3420105e70bdf357d57dd5567202b4ef8d27f810e98bb962d950834/setuptools-69.2.0-py3-none-any.whl", hash = "sha256:c21c49fb1042386df081cb5d86759792ab89efca84cf114889191cd09aacc80c", size = 821485, upload-time = "2024-03-13T11:20:54.103Z", requires-python = ">=3.8" },
        ]

        [[package]]
        name = "wheel"
        version = "0.43.0"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        build-dependencies = [
            { name = "flit-core", version = "3.9.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/b8/d6/ac9cd92ea2ad502ff7c1ab683806a9deb34711a1e2bd8a59814e8fc27e69/wheel-0.43.0.tar.gz", hash = "sha256:465ef92c69fa5c5da2d1cf8ac40559a8c940886afcef87dcf14b9470862f1d85", size = 99109, upload-time = "2024-03-11T19:29:17.32Z", requires-python = ">=3.8" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/7d/cd/d7460c9a869b16c3dd4e1e403cce337df165368c71d6af229a74699622ce/wheel-0.43.0-py3-none-any.whl", hash = "sha256:55c570405f142630c6b9f72fe09d9b67cf1477fcf543ae5b8dcb1f5b7377da81", size = 65775, upload-time = "2024-03-11T19:29:15.522Z", requires-python = ">=3.8" },
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
        id = "build:flit-core:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
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
        id = "build:wheel:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
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
        sdist = { url = "https://files.pythonhosted.org/packages/c4/e6/c1ac50fe3eebb38a155155711e6e864e254ce4b6e17fe2429b4c4d5b9e80/flit_core-3.9.0.tar.gz", hash = "sha256:72ad266176c4a3fcfab5f2930d76896059851240570ce9a98733b658cb786eba", size = 41917, upload-time = "2023-05-14T14:48:51.809Z", requires-python = ">=3.6" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/38/45/618e84e49a6c51e5dd15565ec2fcd82ab273434f236b8f108f065ded517a/flit_core-3.9.0-py3-none-any.whl", hash = "sha256:7aada352fb0c7f5538c4fafeddf314d3a6a92ee8e2b1de70482329e42de70301", size = 63141, upload-time = "2023-05-14T14:48:49.24Z", requires-python = ">=3.6" },
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
        sdist = { url = "https://files.pythonhosted.org/packages/4d/5b/dc575711b6b8f2f866131a40d053e30e962e633b332acf7cd2c24843d83d/setuptools-69.2.0.tar.gz", hash = "sha256:0ff4183f8f42cd8fa3acea16c45205521a4ef28f73c6391d8a25e92893134f2e", size = 2222950, upload-time = "2024-03-13T11:20:59.219Z", requires-python = ">=3.8" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/92/e1/1c8bb3420105e70bdf357d57dd5567202b4ef8d27f810e98bb962d950834/setuptools-69.2.0-py3-none-any.whl", hash = "sha256:c21c49fb1042386df081cb5d86759792ab89efca84cf114889191cd09aacc80c", size = 821485, upload-time = "2024-03-13T11:20:54.103Z", requires-python = ">=3.8" },
        ]

        [[package]]
        name = "wheel"
        version = "0.43.0"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        build-dependencies = [
            { name = "flit-core", version = "3.9.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/b8/d6/ac9cd92ea2ad502ff7c1ab683806a9deb34711a1e2bd8a59814e8fc27e69/wheel-0.43.0.tar.gz", hash = "sha256:465ef92c69fa5c5da2d1cf8ac40559a8c940886afcef87dcf14b9470862f1d85", size = 99109, upload-time = "2024-03-11T19:29:17.32Z", requires-python = ">=3.8" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/7d/cd/d7460c9a869b16c3dd4e1e403cce337df165368c71d6af229a74699622ce/wheel-0.43.0-py3-none-any.whl", hash = "sha256:55c570405f142630c6b9f72fe09d9b67cf1477fcf543ae5b8dcb1f5b7377da81", size = 65775, upload-time = "2024-03-11T19:29:15.522Z", requires-python = ">=3.8" },
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
        id = "build:flit-core:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
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
        id = "build:wheel:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
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
        sdist = { url = "https://files.pythonhosted.org/packages/c4/e6/c1ac50fe3eebb38a155155711e6e864e254ce4b6e17fe2429b4c4d5b9e80/flit_core-3.9.0.tar.gz", hash = "sha256:72ad266176c4a3fcfab5f2930d76896059851240570ce9a98733b658cb786eba", size = 41917, upload-time = "2023-05-14T14:48:51.809Z", requires-python = ">=3.6" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/38/45/618e84e49a6c51e5dd15565ec2fcd82ab273434f236b8f108f065ded517a/flit_core-3.9.0-py3-none-any.whl", hash = "sha256:7aada352fb0c7f5538c4fafeddf314d3a6a92ee8e2b1de70482329e42de70301", size = 63141, upload-time = "2023-05-14T14:48:49.24Z", requires-python = ">=3.6" },
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
        sdist = { url = "https://files.pythonhosted.org/packages/4d/5b/dc575711b6b8f2f866131a40d053e30e962e633b332acf7cd2c24843d83d/setuptools-69.2.0.tar.gz", hash = "sha256:0ff4183f8f42cd8fa3acea16c45205521a4ef28f73c6391d8a25e92893134f2e", size = 2222950, upload-time = "2024-03-13T11:20:59.219Z", requires-python = ">=3.8" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/92/e1/1c8bb3420105e70bdf357d57dd5567202b4ef8d27f810e98bb962d950834/setuptools-69.2.0-py3-none-any.whl", hash = "sha256:c21c49fb1042386df081cb5d86759792ab89efca84cf114889191cd09aacc80c", size = 821485, upload-time = "2024-03-13T11:20:54.103Z", requires-python = ">=3.8" },
        ]

        [[package]]
        name = "wheel"
        version = "0.43.0"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        build-dependencies = [
            { name = "flit-core", version = "3.9.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/b8/d6/ac9cd92ea2ad502ff7c1ab683806a9deb34711a1e2bd8a59814e8fc27e69/wheel-0.43.0.tar.gz", hash = "sha256:465ef92c69fa5c5da2d1cf8ac40559a8c940886afcef87dcf14b9470862f1d85", size = 99109, upload-time = "2024-03-11T19:29:17.32Z", requires-python = ">=3.8" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/7d/cd/d7460c9a869b16c3dd4e1e403cce337df165368c71d6af229a74699622ce/wheel-0.43.0-py3-none-any.whl", hash = "sha256:55c570405f142630c6b9f72fe09d9b67cf1477fcf543ae5b8dcb1f5b7377da81", size = 65775, upload-time = "2024-03-11T19:29:15.522Z", requires-python = ">=3.8" },
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
        id = "build:flit-core:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
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
        id = "build:wheel:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
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
        sdist = { url = "https://files.pythonhosted.org/packages/c4/e6/c1ac50fe3eebb38a155155711e6e864e254ce4b6e17fe2429b4c4d5b9e80/flit_core-3.9.0.tar.gz", hash = "sha256:72ad266176c4a3fcfab5f2930d76896059851240570ce9a98733b658cb786eba", size = 41917, upload-time = "2023-05-14T14:48:51.809Z", requires-python = ">=3.6" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/38/45/618e84e49a6c51e5dd15565ec2fcd82ab273434f236b8f108f065ded517a/flit_core-3.9.0-py3-none-any.whl", hash = "sha256:7aada352fb0c7f5538c4fafeddf314d3a6a92ee8e2b1de70482329e42de70301", size = 63141, upload-time = "2023-05-14T14:48:49.24Z", requires-python = ">=3.6" },
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
        sdist = { url = "https://files.pythonhosted.org/packages/4d/5b/dc575711b6b8f2f866131a40d053e30e962e633b332acf7cd2c24843d83d/setuptools-69.2.0.tar.gz", hash = "sha256:0ff4183f8f42cd8fa3acea16c45205521a4ef28f73c6391d8a25e92893134f2e", size = 2222950, upload-time = "2024-03-13T11:20:59.219Z", requires-python = ">=3.8" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/92/e1/1c8bb3420105e70bdf357d57dd5567202b4ef8d27f810e98bb962d950834/setuptools-69.2.0-py3-none-any.whl", hash = "sha256:c21c49fb1042386df081cb5d86759792ab89efca84cf114889191cd09aacc80c", size = 821485, upload-time = "2024-03-13T11:20:54.103Z", requires-python = ">=3.8" },
        ]

        [[package]]
        name = "wheel"
        version = "0.43.0"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        build-dependencies = [
            { name = "flit-core", version = "3.9.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/b8/d6/ac9cd92ea2ad502ff7c1ab683806a9deb34711a1e2bd8a59814e8fc27e69/wheel-0.43.0.tar.gz", hash = "sha256:465ef92c69fa5c5da2d1cf8ac40559a8c940886afcef87dcf14b9470862f1d85", size = 99109, upload-time = "2024-03-11T19:29:17.32Z", requires-python = ">=3.8" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/7d/cd/d7460c9a869b16c3dd4e1e403cce337df165368c71d6af229a74699622ce/wheel-0.43.0-py3-none-any.whl", hash = "sha256:55c570405f142630c6b9f72fe09d9b67cf1477fcf543ae5b8dcb1f5b7377da81", size = 65775, upload-time = "2024-03-11T19:29:15.522Z", requires-python = ">=3.8" },
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
        id = "build:flit-core:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
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
        id = "build:wheel:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
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
        sdist = { url = "https://files.pythonhosted.org/packages/c4/e6/c1ac50fe3eebb38a155155711e6e864e254ce4b6e17fe2429b4c4d5b9e80/flit_core-3.9.0.tar.gz", hash = "sha256:72ad266176c4a3fcfab5f2930d76896059851240570ce9a98733b658cb786eba", size = 41917, upload-time = "2023-05-14T14:48:51.809Z", requires-python = ">=3.6" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/38/45/618e84e49a6c51e5dd15565ec2fcd82ab273434f236b8f108f065ded517a/flit_core-3.9.0-py3-none-any.whl", hash = "sha256:7aada352fb0c7f5538c4fafeddf314d3a6a92ee8e2b1de70482329e42de70301", size = 63141, upload-time = "2023-05-14T14:48:49.24Z", requires-python = ">=3.6" },
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
        sdist = { url = "https://files.pythonhosted.org/packages/4d/5b/dc575711b6b8f2f866131a40d053e30e962e633b332acf7cd2c24843d83d/setuptools-69.2.0.tar.gz", hash = "sha256:0ff4183f8f42cd8fa3acea16c45205521a4ef28f73c6391d8a25e92893134f2e", size = 2222950, upload-time = "2024-03-13T11:20:59.219Z", requires-python = ">=3.8" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/92/e1/1c8bb3420105e70bdf357d57dd5567202b4ef8d27f810e98bb962d950834/setuptools-69.2.0-py3-none-any.whl", hash = "sha256:c21c49fb1042386df081cb5d86759792ab89efca84cf114889191cd09aacc80c", size = 821485, upload-time = "2024-03-13T11:20:54.103Z", requires-python = ">=3.8" },
        ]

        [[package]]
        name = "wheel"
        version = "0.43.0"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        build-dependencies = [
            { name = "flit-core", version = "3.9.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/b8/d6/ac9cd92ea2ad502ff7c1ab683806a9deb34711a1e2bd8a59814e8fc27e69/wheel-0.43.0.tar.gz", hash = "sha256:465ef92c69fa5c5da2d1cf8ac40559a8c940886afcef87dcf14b9470862f1d85", size = 99109, upload-time = "2024-03-11T19:29:17.32Z", requires-python = ">=3.8" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/7d/cd/d7460c9a869b16c3dd4e1e403cce337df165368c71d6af229a74699622ce/wheel-0.43.0-py3-none-any.whl", hash = "sha256:55c570405f142630c6b9f72fe09d9b67cf1477fcf543ae5b8dcb1f5b7377da81", size = 65775, upload-time = "2024-03-11T19:29:15.522Z", requires-python = ">=3.8" },
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

/// Verify a runtime package selected from `--find-links` remains available to a matched build.
#[test]
fn lock_build_dependencies_extra_match_runtime_find_links() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    write_wheel(
        &links_dir.child("runtime_builder-1.0.0-py3-none-any.whl"),
        "runtime-builder",
        "1.0.0",
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
    return []

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    assert version("runtime-builder") == "1.0.0"
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
        dependencies = ["child", "runtime-builder==1.0.0"]

        [tool.uv.sources]
        child = { path = "child" }

        [tool.uv.extra-build-dependencies]
        child = [{ requirement = "runtime-builder", match-runtime = true }]
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
    Resolved 3 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    let child = package_section(&lock, "child");
    assert!(
        child.contains(r#"{ name = "runtime-builder", version = "1.0.0", match-runtime = true }"#),
        "{child}"
    );

    uv_snapshot!(context.filters(), context
        .sync()
        .arg("--frozen")
        .arg("--no-index")
        .arg("--no-cache")
        .arg("--preview-features")
        .arg("extra-build-dependencies,lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + child==0.1.0 (from file://[TEMP_DIR]/child)
     + runtime-builder==1.0.0
    ");

    Ok(())
}

/// Verify an inactive `match-runtime` build root does not prevent frozen replay when the runtime
/// package is absent on the current platform.
#[test]
fn lock_build_dependencies_extra_match_runtime_inactive_find_links() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let links_dir = context.workspace_root.join("test/links");
    let foreign_marker = if cfg!(target_os = "windows") {
        "sys_platform == 'linux'"
    } else {
        "sys_platform == 'win32'"
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

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(&format!(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["child", "validation==1.0.0 ; {foreign_marker}"]

        [tool.uv.sources]
        child = {{ path = "child" }}

        [tool.uv.extra-build-dependencies]
        child = [{{ requirement = "validation ; {foreign_marker}", match-runtime = true }}]
        "#,
    ))?;

    context
        .lock()
        .arg("--find-links")
        .arg(&links_dir)
        .arg("--no-index")
        .arg("--preview-features")
        .arg("extra-build-dependencies,lock-build-dependencies")
        .assert()
        .success();

    let lock = context.read("uv.lock");
    let child = package_section(&lock, "child");
    assert!(child.contains(r#"name = "validation""#), "{child}");
    assert!(
        child.contains(&format!(
            r#"marker = "{foreign_marker}", match-runtime = true"#
        )),
        "{child}"
    );

    pyproject_toml.write_str(&format!(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["child", "validation==1.0.0 ; {foreign_marker}"]

        [tool.uv.sources]
        child = {{ path = "child" }}
        "#,
    ))?;

    context
        .sync()
        .arg("--no-index")
        .arg("--no-cache")
        .arg("--frozen")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();

    Ok(())
}

/// Verify a runtime package selected from a named flat index remains available to a matched build.
#[test]
fn lock_build_dependencies_extra_match_runtime_named_flat_index() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    write_wheel(
        &links_dir.child("runtime_builder-1.0.0-py3-none-any.whl"),
        "runtime-builder",
        "1.0.0",
    )?;
    let links_url = Url::from_directory_path(links_dir.path()).expect("valid links URL");

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
    assert version("runtime-builder") == "1.0.0"
    return []

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    assert version("runtime-builder") == "1.0.0"
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
        dependencies = ["child", "runtime-builder==1.0.0"]

        [[tool.uv.index]]
        name = "local"
        url = "{links_url}"
        format = "flat"
        default = true

        [tool.uv.sources]
        child = {{ path = "child" }}
        runtime-builder = {{ index = "local" }}

        [tool.uv.extra-build-dependencies]
        child = [{{ requirement = "runtime-builder", match-runtime = true }}]
        "#,
        ))?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("extra-build-dependencies,lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    let child = package_section(&lock, "child");
    assert!(
        child.contains(r#"{ name = "runtime-builder", version = "1.0.0", match-runtime = true }"#),
        "{child}"
    );

    context
        .sync()
        .arg("--frozen")
        .arg("--no-index")
        .arg("--no-cache")
        .arg("--preview-features")
        .arg("extra-build-dependencies,lock-build-dependencies")
        .assert()
        .success();

    Ok(())
}

/// Verify frozen matched builds replay named flat indexes whose configured URL is credentialed or
/// relative, even though the lockfile stores a normalized, credential-free source.
#[tokio::test]
async fn lock_build_dependencies_extra_match_runtime_replays_named_index_identity() -> Result<()> {
    for relative in [false, true] {
        let context = uv_test::test_context!("3.12");

        let links_dir = context.temp_dir.child("links");
        links_dir.create_dir_all()?;
        let wheel = links_dir.child("runtime_builder-1.0.0-py3-none-any.whl");
        write_wheel(&wheel, "runtime-builder", "1.0.0")?;

        let server = MockServer::start().await;
        if !relative {
            let wheel_url = Url::from_file_path(wheel.path()).expect("valid wheel URL");
            Mock::given(method("GET"))
                .and(path("/"))
                .respond_with(ResponseTemplate::new(200).set_body_raw(
                    format!(r#"<a href="{wheel_url}">runtime_builder-1.0.0-py3-none-any.whl</a>"#),
                    "text/html",
                ))
                .mount(&server)
                .await;
        }
        let index_url = if relative {
            "./links".to_string()
        } else {
            server
                .uri()
                .replacen("http://", "http://release-user:release-password@", 1)
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
from importlib.metadata import version
from pathlib import Path
from zipfile import ZipFile

def get_requires_for_build_wheel(config_settings=None):
    assert version("runtime-builder") == "1.0.0"
    return []

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    assert version("runtime-builder") == "1.0.0"
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
            dependencies = ["child", "runtime-builder==1.0.0"]

            [[tool.uv.index]]
            name = "local"
            url = "{index_url}"
            format = "flat"
            default = true

            [tool.uv.sources]
            child = {{ path = "child" }}
            runtime-builder = {{ index = "local" }}

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

        let lock = context.read("uv.lock");
        let child = package_section(&lock, "child");
        assert!(
            child.contains(
                r#"{ name = "runtime-builder", version = "1.0.0", match-runtime = true }"#
            ),
            "{child}"
        );
        assert!(!lock.contains("release-user"), "{lock}");
        assert!(!lock.contains("release-password"), "{lock}");

        context
            .sync()
            .arg("--frozen")
            .arg("--no-index")
            .arg("--no-cache")
            .arg("--preview-features")
            .arg("extra-build-dependencies,lock-build-dependencies")
            .assert()
            .success();
    }

    Ok(())
}

/// Verify a matched `--find-links` build cannot select a same-version wheel from another location.
#[test]
fn lock_build_dependencies_extra_match_runtime_find_links_keep_source_pin() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let selected_links = context.temp_dir.child("selected-links");
    selected_links.create_dir_all()?;
    write_wheel(
        &selected_links.child("runtime_builder-1.0.0-py3-none-any.whl"),
        "runtime-builder",
        "1.0.0",
    )?;

    let unrelated_links = context.temp_dir.child("unrelated-links");
    unrelated_links.create_dir_all()?;
    write_wheel_with_requires_and_tag(
        &unrelated_links.child("runtime_builder-1.0.0-cp312-none-any.whl"),
        "runtime-builder",
        "1.0.0",
        &[],
        "cp312-none-any",
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
from importlib.metadata import distribution
from pathlib import Path
from zipfile import ZipFile

def check_runtime_builder():
    wheel = distribution("runtime-builder").read_text("WHEEL")
    assert "Tag: py3-none-any" in wheel, wheel

def get_requires_for_build_wheel(config_settings=None):
    check_runtime_builder()
    return []

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    check_runtime_builder()
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
        dependencies = ["child", "runtime-builder==1.0.0"]

        [tool.uv.sources]
        child = { path = "child" }

        [tool.uv.extra-build-dependencies]
        child = [{ requirement = "runtime-builder", match-runtime = true }]
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--find-links")
        .arg(selected_links.path())
        .arg("--find-links")
        .arg(unrelated_links.path())
        .arg("--no-index")
        .arg("--preview-features")
        .arg("extra-build-dependencies,lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    let runtime_builder = package_section(&lock, "runtime-builder");
    assert!(
        runtime_builder.contains("selected-links/runtime_builder-1.0.0-py3-none-any.whl"),
        "{runtime_builder}"
    );

    context
        .sync()
        .arg("--frozen")
        .arg("--no-index")
        .arg("--no-cache")
        .arg("--preview-features")
        .arg("extra-build-dependencies,lock-build-dependencies")
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
    let builder_url = Url::from_directory_path(builder_dir.path()).expect("valid builder URL");

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

/// Verify a `match-runtime` build dependency provided by an absolute path wheel is recognized
/// when replaying a locked source build.
#[test]
fn lock_build_dependencies_extra_match_runtime_path_wheel() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let links = context.temp_dir.child("links");
    links.create_dir_all()?;
    let helper_wheel = links.child("helper-1.0.0-py3-none-any.whl");
    write_wheel(&helper_wheel, "helper", "1.0.0")?;
    let helper_url = Url::from_file_path(helper_wheel.path()).expect("valid file URL");

    let child = context.temp_dir.child("child");
    child.create_dir_all()?;
    child.child("pyproject.toml").write_str(
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
    child.child("build_backend.py").write_str(
        r#"
from importlib.metadata import version
from pathlib import Path
from zipfile import ZipFile

def get_requires_for_build_wheel(config_settings=None):
    return []

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    helper_version = version("helper")
    filename = "child-0.1.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("child.py", f'BUILD_HELPER = "{helper_version}"\n')
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
        dependencies = ["child", "helper @ {helper_url}"]

        [tool.uv.sources]
        child = {{ path = "child" }}

        [tool.uv.extra-build-dependencies]
        child = [{{ requirement = "helper", match-runtime = true }}]
        "#
        ))?;

    context
        .lock()
        .arg("--no-index")
        .arg("--preview-features")
        .arg("extra-build-dependencies,lock-build-dependencies")
        .assert()
        .success();

    context
        .sync()
        .arg("--frozen")
        .arg("--no-index")
        .arg("--preview-features")
        .arg("extra-build-dependencies,lock-build-dependencies")
        .assert()
        .success();

    uv_snapshot!(context.filters(), context
        .run()
        .arg("--no-sync")
        .arg("python")
        .arg("-c")
        .arg("import child; print(child.BUILD_HELPER)"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    1.0.0

    ----- stderr -----
    ");

    Ok(())
}

/// Verify that `uv run --with` does not reuse a source build from the locked base environment
/// when the overlay selects a different `match-runtime` build dependency.
#[test]
fn lock_build_dependencies_extra_match_runtime_run_overlay() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let helper_one = context.temp_dir.child("helper-one");
    let helper_two = context.temp_dir.child("helper-two");
    for (helper, version) in [(&helper_one, "1.0.0"), (&helper_two, "2.0.0")] {
        helper.create_dir_all()?;
        helper.child("pyproject.toml").write_str(&format!(
            r#"
            [project]
            name = "helper"
            version = "{version}"
            requires-python = ">=3.12"

            [build-system]
            requires = []
            backend-path = ["."]
            build-backend = "build_backend"
            "#
        ))?;
        helper.child("build_backend.py").write_str(&format!(
            r#"
from pathlib import Path
from zipfile import ZipFile

def get_requires_for_build_wheel(config_settings=None):
    return []

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    filename = "helper-{version}-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("helper.py", 'VERSION = "{version}"\n')
        wheel.writestr(
            "helper-{version}.dist-info/METADATA",
            "Metadata-Version: 2.3\nName: helper\nVersion: {version}\n",
        )
        wheel.writestr(
            "helper-{version}.dist-info/WHEEL",
            "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
        )
        wheel.writestr("helper-{version}.dist-info/RECORD", "")
    return filename
"#
        ))?;
    }
    let helper_one_url = Url::from_directory_path(helper_one.path()).expect("valid file URL");
    let helper_two_url = Url::from_directory_path(helper_two.path()).expect("valid file URL");

    let child = context.temp_dir.child("child");
    child.create_dir_all()?;
    child.child("pyproject.toml").write_str(
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
    child.child("build_backend.py").write_str(
        r#"
from importlib.metadata import version
from pathlib import Path
from zipfile import ZipFile

def get_requires_for_build_wheel(config_settings=None):
    return []

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    helper_version = version("helper")
    filename = "child-0.1.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("child.py", f'BUILD_HELPER = "{helper_version}"\n')
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
    let child_url = Url::from_directory_path(child.path()).expect("valid file URL");

    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(&format!(
            r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["child", "helper @ {helper_one_url}"]

        [tool.uv.sources]
        child = {{ path = "child" }}

        [tool.uv.extra-build-dependencies]
        child = [{{ requirement = "helper", match-runtime = true }}]
        "#
        ))?;

    uv_snapshot!(context.filters(), context
        .run()
        .arg("--no-index")
        .arg("--preview-features")
        .arg("extra-build-dependencies,lock-build-dependencies")
        .arg("--with")
        .arg(format!("child @ {child_url}"))
        .arg("--with")
        .arg(format!("helper @ {helper_two_url}"))
        .arg("python")
        .arg("-c")
        .arg("import child, helper; print(child.BUILD_HELPER); print(helper.VERSION)"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    2.0.0
    2.0.0

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + child==0.1.0 (from file://[TEMP_DIR]/child)
     + helper==1.0.0 (from file://[TEMP_DIR]/helper-one)
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + child==0.1.0 (from file://[TEMP_DIR]/child)
     + helper==2.0.0 (from file://[TEMP_DIR]/helper-two)
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

    // Removing the complete bootstrap record must not fall back to the final environment.
    let bootstrap_resolution = resolution_sections(&lock)
        .split("\n\n")
        .find(|resolution| {
            resolution.contains("stage = \"bootstrap\"") && resolution.contains("name = \"dep\"")
        })
        .expect("locked bootstrap resolution")
        .to_string();
    context.temp_dir.child("uv.lock").write_str(&lock.replacen(
        &format!("{bootstrap_resolution}\n\n"),
        "",
        1,
    ))?;

    uv_snapshot!(context.filters(), context
        .sync()
        .arg("--no-index")
        .arg("--no-cache")
        .arg("--frozen")
        .arg("--preview-features")
        .arg("extra-build-dependencies,lock-build-dependencies"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to parse `uv.lock`
      Caused by: Invalid resolution record `build:dep:wheel:build:[BUILD-ID]`: staged build resolution is missing a matching bootstrap resolution
    ");

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

    // A newly configured initial root must not be silently omitted from frozen bootstrap replay.
    context.temp_dir.child("uv.lock").write_str(&lock)?;
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
        dep = ["extra", "missing"]
        "#,
    )?;

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

/// Verify that source builds reachable only from the bootstrap environment replay their locked
/// nested build requirements without invoking the resolver.
#[test]
fn lock_build_dependencies_replay_bootstrap_only_nested_source() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    write_wheel(
        &links_dir.child("seed-1.0.0-py3-none-any.whl"),
        "seed",
        "1.0.0",
    )?;
    write_wheel(
        &links_dir.child("tool-1.0.0-py3-none-any.whl"),
        "tool",
        "1.0.0",
    )?;

    let seed_source = links_dir.child("seed-2.0.0.zip");
    let mut zip = ZipFileWriter::new(Vec::new());
    let entry = ZipEntryBuilder::new("seed-2.0.0/pyproject.toml".into(), Compression::Stored);
    block_on(zip.write_entry_whole(
        entry,
        br#"
        [project]
        name = "seed"
        version = "2.0.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["tool==1.0.0"]
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
    ))?;
    let entry = ZipEntryBuilder::new("seed-2.0.0/build_backend.py".into(), Compression::Stored);
    block_on(zip.write_entry_whole(
        entry,
        br#"
from pathlib import Path
from zipfile import ZipFile

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    filename = "seed-2.0.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("seed.py", "")
        wheel.writestr(
            "seed-2.0.0.dist-info/METADATA",
            "Metadata-Version: 2.3\nName: seed\nVersion: 2.0.0\n",
        )
        wheel.writestr(
            "seed-2.0.0.dist-info/WHEEL",
            "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
        )
        wheel.writestr("seed-2.0.0.dist-info/RECORD", "")
    return filename
"#,
    ))?;
    fs_err::write(seed_source.path(), block_on(zip.close())?)?;

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
from zipfile import ZipFile

def get_requires_for_build_wheel(config_settings=None):
    return ["seed<2"]

def prepare_metadata_for_build_wheel(metadata_directory, config_settings=None):
    dist_info = Path(metadata_directory) / "dep-0.1.0.dist-info"
    dist_info.mkdir()
    (dist_info / "METADATA").write_text(
        "Metadata-Version: 2.3\nName: dep\nVersion: 0.1.0\n"
    )
    return dist_info.name

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    filename = "dep-0.1.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("dep.py", "")
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
    let resolutions = resolution_sections(&lock);
    let dep_bootstrap_resolution = resolutions
        .split("\n\n")
        .find(|resolution| {
            resolution.contains("stage = \"bootstrap\"") && resolution.contains("name = \"dep\"")
        })
        .expect("locked bootstrap resolution");
    let dep_build_resolution = resolutions
        .split("\n\n")
        .find(|resolution| {
            resolution.contains("stage = \"build\"") && resolution.contains("name = \"dep\"")
        })
        .expect("locked build resolution");
    assert!(
        dep_bootstrap_resolution.contains(r#"{ name = "seed", version = "2.0.0""#),
        "{dep_bootstrap_resolution}"
    );
    assert!(
        dep_build_resolution.contains(r#"{ name = "seed", version = "1.0.0""#),
        "{dep_build_resolution}"
    );
    assert!(
        !dep_build_resolution.contains(r#"{ name = "seed", version = "2.0.0""#),
        "{dep_build_resolution}"
    );
    let seed_resolution = resolutions
        .split("\n\n")
        .find(|resolution| resolution.contains("\nname = \"seed\"\n"))
        .expect("locked nested resolution");
    assert!(!seed_resolution.contains("stage = "), "{seed_resolution}");
    assert!(
        seed_resolution.contains(r#"{ name = "tool", version = "1.0.0""#),
        "{seed_resolution}"
    );

    // With no index or find-links configured, a live nested resolution would be unsatisfiable.
    // The frozen build can only succeed by reconstructing the bootstrap-only source build.
    context
        .sync()
        .arg("--no-index")
        .arg("--no-cache")
        .arg("--frozen")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();

    let foreign_marker = if cfg!(target_os = "linux") {
        "sys_platform == 'darwin'"
    } else {
        "sys_platform == 'linux'"
    };
    let incomplete = lock.replacen(
        &format!("{seed_resolution}\n\n"),
        &format!("{seed_resolution}\ntarget = {{ marker = \"{foreign_marker}\" }}\n\n"),
        1,
    );
    context.temp_dir.child("uv.lock").write_str(&incomplete)?;

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

    context.temp_dir.child("uv.lock").write_str(&lock.replacen(
        &format!("{seed_resolution}\n\n"),
        "",
        1,
    ))?;

    uv_snapshot!(context.filters(), context
        .sync()
        .arg("--no-index")
        .arg("--no-cache")
        .arg("--frozen")
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to parse `uv.lock`
      Caused by: Invalid resolution record `build:seed:wheel:build:[BUILD-ID]`: package build-dependencies are missing a build resolution
    ");

    Ok(())
}

/// Verify changes to a mutable source reachable only from a locked bootstrap environment
/// invalidate the lock instead of reusing stale nested build metadata.
#[test]
fn lock_build_dependencies_stale_bootstrap_only_nested_source() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    write_wheel(
        &links_dir.child("seed-1.0.0-py3-none-any.whl"),
        "seed",
        "1.0.0",
    )?;
    write_wheel(
        &links_dir.child("tool-1.0.0-py3-none-any.whl"),
        "tool",
        "1.0.0",
    )?;
    write_wheel(
        &links_dir.child("tool-2.0.0-py3-none-any.whl"),
        "tool",
        "2.0.0",
    )?;

    let seed_source = links_dir.child("seed-2.0.0.zip");
    let write_seed_source = |tool_version: &str| -> Result<()> {
        let mut zip = ZipFileWriter::new(Vec::new());
        let pyproject_toml = format!(
            r#"
            [project]
            name = "seed"
            version = "2.0.0"
            requires-python = ">=3.12"

            [build-system]
            requires = ["tool=={tool_version}"]
            backend-path = ["."]
            build-backend = "build_backend"
            "#,
        );
        let entry = ZipEntryBuilder::new("seed-2.0.0/pyproject.toml".into(), Compression::Stored);
        block_on(zip.write_entry_whole(entry, pyproject_toml.as_bytes()))?;
        let entry = ZipEntryBuilder::new("seed-2.0.0/build_backend.py".into(), Compression::Stored);
        block_on(zip.write_entry_whole(
            entry,
            br#"
from pathlib import Path
from zipfile import ZipFile

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    filename = "seed-2.0.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("seed.py", "")
        wheel.writestr(
            "seed-2.0.0.dist-info/METADATA",
            "Metadata-Version: 2.3\nName: seed\nVersion: 2.0.0\n",
        )
        wheel.writestr(
            "seed-2.0.0.dist-info/WHEEL",
            "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
        )
        wheel.writestr("seed-2.0.0.dist-info/RECORD", "")
    return filename
"#,
        ))?;
        fs_err::write(seed_source.path(), block_on(zip.close())?)?;
        Ok(())
    };
    write_seed_source("1.0.0")?;

    let dep_source = links_dir.child("dep-0.1.0.zip");
    let mut zip = ZipFileWriter::new(Vec::new());
    let entry = ZipEntryBuilder::new("dep-0.1.0/pyproject.toml".into(), Compression::Stored);
    block_on(zip.write_entry_whole(
        entry,
        br#"
        [project]
        name = "dep"
        dynamic = ["version"]
        requires-python = ">=3.12"

        [build-system]
        requires = ["seed"]
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
    ))?;
    let entry = ZipEntryBuilder::new("dep-0.1.0/build_backend.py".into(), Compression::Stored);
    block_on(zip.write_entry_whole(
        entry,
        br#"
from pathlib import Path
from zipfile import ZipFile

def get_requires_for_build_wheel(config_settings=None):
    return ["seed<2"]

def prepare_metadata_for_build_wheel(metadata_directory, config_settings=None):
    dist_info = Path(metadata_directory) / "dep-0.1.0.dist-info"
    dist_info.mkdir()
    (dist_info / "METADATA").write_text(
        "Metadata-Version: 2.3\nName: dep\nVersion: 0.1.0\n"
    )
    return dist_info.name

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    filename = "dep-0.1.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("dep.py", "")
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
    fs_err::write(dep_source.path(), block_on(zip.close())?)?;

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep"]
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
    let dep_bootstrap_resolution = resolutions
        .split("\n\n")
        .find(|resolution| {
            resolution.contains("stage = \"bootstrap\"") && resolution.contains("name = \"dep\"")
        })
        .expect("locked bootstrap resolution");
    let dep_build_resolution = resolutions
        .split("\n\n")
        .find(|resolution| {
            resolution.contains("stage = \"build\"") && resolution.contains("name = \"dep\"")
        })
        .expect("locked build resolution");
    assert!(
        dep_bootstrap_resolution.contains(r#"{ name = "seed", version = "2.0.0""#),
        "{dep_bootstrap_resolution}"
    );
    assert!(
        dep_build_resolution.contains(r#"{ name = "seed", version = "1.0.0""#),
        "{dep_build_resolution}"
    );
    assert!(
        !dep_build_resolution.contains(r#"{ name = "seed", version = "2.0.0""#),
        "{dep_build_resolution}"
    );

    write_seed_source("2.0.0")?;

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
        requires-python = ">=3.12,<4"
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
    assert!(
        !dep.contains(r#"{ name = "helper", version = "0.1.0" }"#),
        "{dep}"
    );
    assert!(!dep.contains("build-dependencies = "), "{dep}");
    assert!(!lock.contains("[[resolution]]"), "{lock}");
    assert!(!lock.contains("build-settings = "), "{lock}");
    assert!(lock.starts_with("version = 1\nrevision = 3\n"), "{lock}");

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--no-build-isolation-package")
        .arg("dep")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--locked"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

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

/// Verify that metadata built while resolving an unnamed `uv add` requirement cannot leak an
/// unconstrained build environment into the locked sync.
#[test]
fn lock_build_dependencies_add_unnamed_uses_constrained_build_environment() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [tool.uv]
        build-constraint-dependencies = ["ok==1.0.0"]
        "#,
    )?;

    let source = context.temp_dir.child("source");
    source.create_dir_all()?;
    source.child("pyproject.toml").write_str(
        r#"
        [build-system]
        requires = ["ok"]
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
    )?;
    source.child("build_backend.py").write_str(
        r#"
from importlib.metadata import version
from pathlib import Path
from zipfile import ZipFile

def prepare_metadata_for_build_wheel(metadata_directory, config_settings=None):
    dist_info = Path(metadata_directory) / "unnamed_source-1.0.0.dist-info"
    dist_info.mkdir()
    (dist_info / "METADATA").write_text(
        "Metadata-Version: 2.3\nName: unnamed-source\nVersion: 1.0.0\n"
    )
    return dist_info.name

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    filename = "unnamed_source-1.0.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("unnamed_source/__init__.py", f"OK = {version('ok')!r}\n")
        wheel.writestr(
            "unnamed_source-1.0.0.dist-info/METADATA",
            "Metadata-Version: 2.3\nName: unnamed-source\nVersion: 1.0.0\n",
        )
        wheel.writestr(
            "unnamed_source-1.0.0.dist-info/WHEEL",
            "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
        )
        wheel.writestr("unnamed_source-1.0.0.dist-info/RECORD", "")
    return filename
"#,
    )?;

    context
        .add()
        .arg(source.path())
        .arg("--no-workspace")
        .arg("--no-index")
        .arg("--find-links")
        .arg(context.workspace_root.join("test/links"))
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();

    assert_eq!(
        fs_err::read_to_string(context.site_packages().join("unnamed_source/__init__.py"))?,
        "OK = '1.0.0'\n"
    );

    Ok(())
}

/// Verify that universal capture does not reuse a nested source wheel warmed by an ordinary
/// build before executing the parent's dependency hook.
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

def get_requires_for_build_wheel(config_settings=None):
    return [f"tool=={TOOL}"]

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
        .env("UV_TEST_TOOL_VERSION", "0.1.0")
        .assert()
        .success();
    context
        .sync()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--frozen")
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
    assert!(
        package_section(&lock, "dep").contains(r#"{ name = "tool", version = "0.2.0" }"#),
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
        block_on(zip.write_entry_whole(entry, format!("VALUE = '{value}'\n").as_bytes()))?;
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

/// Verify that a hashless runtime wheel in a local index is reinstalled when the file changes.
#[test]
fn lock_build_dependencies_reinstall_mutable_runtime_wheel() -> Result<()> {
    fn write_runtime_wheel(path: &ChildPath, value: &str) -> Result<()> {
        let mut zip = ZipFileWriter::new(Vec::new());
        let entry = ZipEntryBuilder::new("runtime.py".into(), Compression::Stored);
        block_on(zip.write_entry_whole(entry, format!("VALUE = '{value}'\n").as_bytes()))?;
        let entry = ZipEntryBuilder::new(
            "runtime-0.1.0.dist-info/METADATA".into(),
            Compression::Stored,
        );
        block_on(zip.write_entry_whole(
            entry,
            b"Metadata-Version: 2.3\nName: runtime\nVersion: 0.1.0\n",
        ))?;
        let entry =
            ZipEntryBuilder::new("runtime-0.1.0.dist-info/WHEEL".into(), Compression::Stored);
        block_on(zip.write_entry_whole(
            entry,
            b"Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
        ))?;
        let entry =
            ZipEntryBuilder::new("runtime-0.1.0.dist-info/RECORD".into(), Compression::Stored);
        block_on(zip.write_entry_whole(entry, b""))?;
        fs_err::write(path.path(), block_on(zip.close())?)?;

        Ok(())
    }

    let context = uv_test::test_context!("3.12");
    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    let runtime_wheel = links_dir.child("runtime-0.1.0-py3-none-any.whl");
    write_runtime_wheel(&runtime_wheel, "first")?;
    filetime::set_file_mtime(
        runtime_wheel.path(),
        filetime::FileTime::from_unix_time(1_700_000_000, 0),
    )?;

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
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();
    let lock = context.read("uv.lock");
    let runtime = package_section(&lock, "runtime");
    assert!(!runtime.contains("hash ="), "{runtime}");

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
        fs_err::read_to_string(context.site_packages().join("runtime.py"))?,
        "VALUE = 'first'\n"
    );

    write_runtime_wheel(&runtime_wheel, "second")?;
    filetime::set_file_mtime(
        runtime_wheel.path(),
        filetime::FileTime::from_unix_time(1_700_000_001, 0),
    )?;
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
        fs_err::read_to_string(context.site_packages().join("runtime.py"))?,
        "VALUE = 'second'\n"
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
    Checked 1 package in [TIME]
    ");

    Ok(())
}

/// Verify that a hashless runtime source archive in a local index is rebuilt when it changes.
#[test]
fn lock_build_dependencies_reinstall_mutable_runtime_sdist() -> Result<()> {
    fn write_runtime_sdist(path: &ChildPath, value: &str) -> Result<()> {
        let mut zip = ZipFileWriter::new(Vec::new());
        let entry =
            ZipEntryBuilder::new("runtime-0.1.0/pyproject.toml".into(), Compression::Stored);
        block_on(zip.write_entry_whole(
            entry,
            br#"
        [project]
        name = "runtime"
        version = "0.1.0"
        requires-python = ">=3.12"
        [build-system]
        requires = []
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
        ))?;
        let entry =
            ZipEntryBuilder::new("runtime-0.1.0/build_backend.py".into(), Compression::Stored);
        let backend = format!(
            r#"
from pathlib import Path
from zipfile import ZipFile

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    filename = "runtime-0.1.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("runtime.py", "VALUE = '{value}'\n")
        wheel.writestr(
            "runtime-0.1.0.dist-info/METADATA",
            "Metadata-Version: 2.3\nName: runtime\nVersion: 0.1.0\n",
        )
        wheel.writestr(
            "runtime-0.1.0.dist-info/WHEEL",
            "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
        )
        wheel.writestr("runtime-0.1.0.dist-info/RECORD", "")
    return filename
"#
        );
        block_on(zip.write_entry_whole(entry, backend.as_bytes()))?;
        fs_err::write(path.path(), block_on(zip.close())?)?;

        Ok(())
    }

    let context = uv_test::test_context!("3.12");
    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    let runtime_sdist = links_dir.child("runtime-0.1.0.zip");
    write_runtime_sdist(&runtime_sdist, "first")?;
    filetime::set_file_mtime(
        runtime_sdist.path(),
        filetime::FileTime::from_unix_time(1_700_000_000, 0),
    )?;

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
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();
    let lock = context.read("uv.lock");
    let runtime = package_section(&lock, "runtime");
    assert!(!runtime.contains("hash ="), "{runtime}");

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
        fs_err::read_to_string(context.site_packages().join("runtime.py"))?,
        "VALUE = 'first'\n"
    );

    write_runtime_sdist(&runtime_sdist, "second")?;
    filetime::set_file_mtime(
        runtime_sdist.path(),
        filetime::FileTime::from_unix_time(1_700_000_001, 0),
    )?;
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
        fs_err::read_to_string(context.site_packages().join("runtime.py"))?,
        "VALUE = 'second'\n"
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
    Checked 1 package in [TIME]
    ");

    Ok(())
}

/// Verify that a cold-cache local source archive used as a build requirement is hashed in the
/// captured build lock and can be replayed without resolving it again.
#[test]
fn lock_build_dependencies_hash_path_build_sdist() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let helper_source = context.temp_dir.child("helper-0.1.0.zip");
    let mut zip = ZipFileWriter::new(Vec::new());
    let entry = ZipEntryBuilder::new("helper-0.1.0/pyproject.toml".into(), Compression::Stored);
    block_on(zip.write_entry_whole(
        entry,
        br#"
        [project]
        name = "helper"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = []
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
    ))?;
    let entry = ZipEntryBuilder::new("helper-0.1.0/build_backend.py".into(), Compression::Stored);
    block_on(zip.write_entry_whole(
        entry,
        br#"
from pathlib import Path
from zipfile import ZipFile

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    filename = "helper-0.1.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("helper.py", "VALUE = 'locked'\n")
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
    ))?;
    let helper_archive = block_on(zip.close())?;
    let helper_digest = format!("{:x}", Sha256::digest(&helper_archive));
    let helper_hash = format!("sha256:{helper_digest}");
    fs_err::write(helper_source.path(), helper_archive)?;
    let helper_url = Url::from_file_path(helper_source.path()).expect("valid file URL");

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(&format!(
        r#"
        [project]
        name = "dep"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["helper @ {helper_url}#sha256={helper_digest}"]
        backend-path = ["."]
        build-backend = "build_backend"
        "#
    ))?;
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
        .arg("--no-index")
        .arg("--no-cache")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();
    let lock = context.read("uv.lock");
    let helper = package_section(&lock, "helper");
    assert!(
        helper.contains(&format!("hash = \"{helper_hash}\"")),
        "{helper}"
    );

    context
        .sync()
        .arg("--no-index")
        .arg("--no-cache")
        .arg("--frozen")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();
    assert_eq!(
        fs_err::read_to_string(context.site_packages().join("dep/__init__.py"))?,
        "VALUE = 'locked'\n"
    );

    // A conflicting digest on the unchanged local path must never be ignored during frozen replay.
    let pyproject = dep_dir.child("pyproject.toml");
    let contents = fs_err::read_to_string(pyproject.path())?;
    pyproject.write_str(&contents.replace(&helper_digest, &"0".repeat(64)))?;
    let output = context
        .sync()
        .arg("--no-index")
        .arg("--no-cache")
        .arg("--frozen")
        .arg("--reinstall")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .failure();
    let stderr = String::from_utf8_lossy(&output.get_output().stderr);
    assert!(
        stderr.contains("Conflicting archive URL hashes"),
        "{stderr}"
    );

    Ok(())
}

/// Verify that a cold-cache direct URL wheel used as a build requirement is hashed in the
/// captured build lock and can be replayed without resolving it again.
#[test]
fn lock_build_dependencies_hash_direct_build_wheel() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    let helper_wheel = links_dir.child("helper-0.1.0-py3-none-any.whl");
    let mut zip = ZipFileWriter::new(Vec::new());
    let entry = ZipEntryBuilder::new("helper.py".into(), Compression::Stored);
    block_on(zip.write_entry_whole(entry, b"VALUE = 'locked'\n"))?;
    let entry = ZipEntryBuilder::new(
        "helper-0.1.0.dist-info/METADATA".into(),
        Compression::Stored,
    );
    block_on(zip.write_entry_whole(
        entry,
        b"Metadata-Version: 2.3\nName: helper\nVersion: 0.1.0\n",
    ))?;
    let entry = ZipEntryBuilder::new("helper-0.1.0.dist-info/WHEEL".into(), Compression::Stored);
    block_on(zip.write_entry_whole(
        entry,
        b"Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
    ))?;
    let entry = ZipEntryBuilder::new("helper-0.1.0.dist-info/RECORD".into(), Compression::Stored);
    block_on(zip.write_entry_whole(entry, b""))?;
    let helper_archive = block_on(zip.close())?;
    let helper_digest = format!("{:x}", Sha256::digest(&helper_archive));
    let helper_hash = format!("sha256:{helper_digest}");
    fs_err::write(helper_wheel.path(), helper_archive)?;
    let server = uv_test::find_links::FindLinksServer::new(links_dir.path());
    let helper_url = format!("{}/helper-0.1.0-py3-none-any.whl", server.url());

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(&format!(
        r#"
        [project]
        name = "dep"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["helper @ {helper_url}#sha256={helper_digest}"]
        backend-path = ["."]
        build-backend = "build_backend"
        "#
    ))?;
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
        .arg("--no-index")
        .arg("--no-cache")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();
    let lock = context.read("uv.lock");
    let helper = package_section(&lock, "helper");
    let mut filters = context.filters();
    filters.push((r"sha256:[0-9a-f]{64}", "sha256:[HASH]"));
    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(helper, @r#"
        [[package]]
        name = "helper"
        version = "0.1.0"
        source = { url = "http://[LOCALHOST]/helper-0.1.0-py3-none-any.whl" }
        build-only = true
        wheels = [
            { url = "http://[LOCALHOST]/helper-0.1.0-py3-none-any.whl", hash = "sha256:[HASH]" },
        ]
        "#);
    });
    assert!(
        helper.contains(&format!("hash = \"{helper_hash}\"")),
        "{helper}"
    );

    context
        .sync()
        .arg("--no-index")
        .arg("--no-cache")
        .arg("--frozen")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();
    assert_eq!(
        fs_err::read_to_string(context.site_packages().join("dep/__init__.py"))?,
        "VALUE = 'locked'\n"
    );

    // A conflicting digest on the unchanged URL must never be ignored during frozen replay.
    let pyproject = dep_dir.child("pyproject.toml");
    let contents = fs_err::read_to_string(pyproject.path())?;
    pyproject.write_str(&contents.replace(&helper_digest, &"0".repeat(64)))?;
    let output = context
        .sync()
        .arg("--no-index")
        .arg("--no-cache")
        .arg("--frozen")
        .arg("--reinstall")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .failure();
    let stderr = String::from_utf8_lossy(&output.get_output().stderr);
    assert!(
        stderr.contains("Conflicting archive URL hashes"),
        "{stderr}"
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
        id = "build:builder:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        name = "builder"
        version = "1.0.0"
        source = { directory = "[TEMP_DIR]/builder" }
        roots = [
            { name = "nested-backend", version = "1.0.0" },
        ]

        [[resolution]]
        id = "build:dep-a:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        name = "dep-a"
        roots = [
            { name = "builder", version = "1.0.0", source = { directory = "[TEMP_DIR]/builder" } },
            { name = "helper", version = "1.0.0", source = { registry = "[TEMP_DIR]/links" } },
        ]

        [[resolution]]
        id = "build:dep-b:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        name = "dep-b"
        roots = [
            { name = "builder", version = "1.0.0", source = { directory = "[TEMP_DIR]/builder" }, resolution-id = "build:dep-b:wheel:build:[BUILD-ID]" },
            { name = "helper", version = "2.0.0", source = { registry = "[TEMP_DIR]/links" } },
        ]
        "#);
    });
    assert_eq!(lock.matches("[[package]]\nname = \"builder\"").count(), 2);
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
    assert_eq!(scoped_builders.len(), 1, "{lock}");
    assert!(
        scoped_builders
            .iter()
            .all(|package| package.contains(r#"{ name = "nested-backend""#)),
        "{lock}"
    );
    assert!(!lock.contains("build-dependency-packages"), "{lock}");

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

/// Verify a shared source is rebuilt for each capture context so hook-added requirements are not
/// omitted when an in-process build environment already exists.
#[test]
fn lock_build_dependencies_do_not_reuse_build_arena_across_contexts() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    for (name, version) in [
        ("nested-backend", "1.0.0"),
        ("hook-helper", "1.0.0"),
        ("helper", "1.0.0"),
        ("helper", "2.0.0"),
    ] {
        write_wheel(
            &links_dir.child(format!(
                "{}-{version}-py3-none-any.whl",
                name.replace('-', "_")
            )),
            name,
            version,
        )?;
    }
    let nested_backend_url = Url::from_file_path(
        links_dir
            .child("nested_backend-1.0.0-py3-none-any.whl")
            .path(),
    )
    .expect("valid file URL");

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
from importlib.metadata import version
from pathlib import Path
from zipfile import ZipFile

def get_requires_for_build_wheel(config_settings=None):
    return ["hook-helper==1.0.0"]

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    if version("hook-helper") != "1.0.0":
        raise RuntimeError("hook helper is unavailable")
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
            "#,
        ))?;
        dep_dir.child("build_backend.py").write_str(&format!(
            r#"
from pathlib import Path
from zipfile import ZipFile

def get_requires_for_build_wheel(config_settings=None):
    return []

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    filename = "{module_name}-0.1.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("{module_name}.py", "")
        wheel.writestr(
            "{module_name}-0.1.0.dist-info/METADATA",
            "Metadata-Version: 2.3\nName: {name}\nVersion: 0.1.0\n",
        )
        wheel.writestr(
            "{module_name}-0.1.0.dist-info/WHEEL",
            "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
        )
        wheel.writestr("{module_name}-0.1.0.dist-info/RECORD", "")
    return filename
"#,
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
    let resolutions = resolution_sections(&lock);
    let builder_resolutions = resolutions
        .split("[[resolution]]")
        .filter(|resolution| resolution.contains("\nname = \"builder\"\n"))
        .collect::<Vec<_>>();
    let bootstrap_resolutions = builder_resolutions
        .iter()
        .filter(|resolution| resolution.contains("stage = \"bootstrap\""))
        .collect::<Vec<_>>();
    let build_resolutions = builder_resolutions
        .iter()
        .filter(|resolution| resolution.contains("stage = \"build\""))
        .collect::<Vec<_>>();
    assert!(!bootstrap_resolutions.is_empty(), "{lock}");
    assert!(!build_resolutions.is_empty(), "{lock}");
    assert!(
        bootstrap_resolutions.iter().all(|resolution| {
            resolution.contains(r#"{ name = "nested-backend", version = "1.0.0" }"#)
                && !resolution.contains(r#"{ name = "hook-helper", version = "1.0.0" }"#)
        }),
        "{lock}"
    );
    assert!(
        build_resolutions.iter().all(|resolution| {
            resolution.contains(r#"{ name = "nested-backend", version = "1.0.0" }"#)
                && resolution.contains(r#"{ name = "hook-helper", version = "1.0.0" }"#)
        }),
        "{lock}"
    );

    for group in ["a", "b"] {
        if context.venv.exists() {
            fs_err::remove_dir_all(&context.venv)?;
        }
        fs_err::remove_dir_all(&context.cache_dir)?;
        context
            .sync()
            .arg("--find-links")
            .arg(links_dir.path())
            .arg("--no-index")
            .arg("--frozen")
            .arg("--no-default-groups")
            .arg("--group")
            .arg(group)
            .arg("--preview-features")
            .arg("lock-build-dependencies")
            .assert()
            .success();
    }

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
        id = "build:dep:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
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
        id = "build:dep-a:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        name = "dep-a"
        target = { marker = "sys_platform == 'linux'" }
        roots = [
            { name = "builder", version = "1.0.0", source = { registry = "[TEMP_DIR]/links" } },
            { name = "helper", version = "1.0.0", source = { registry = "[TEMP_DIR]/links" } },
        ]

        [[resolution]]
        id = "build:dep-b:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
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
        id = "build:dep:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
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

/// Verify that changing index precedence invalidates a locked build environment.
#[test]
fn lock_build_dependencies_index_strategy_invalidates() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let first_index = context.temp_dir.child("first-index");
    let default_index = context.temp_dir.child("default-index");
    for (index, version) in [(&first_index, "1.0.0"), (&default_index, "2.0.0")] {
        let builder = index.child("builder");
        builder.create_dir_all()?;
        let wheel = builder.child(format!("builder-{version}-py3-none-any.whl"));
        write_wheel(&wheel, "builder", version)?;
        builder.child("index.html").write_str(&format!(
            r#"<a href="builder-{version}-py3-none-any.whl" data-upload-time="2024-03-01T00:00:00Z">builder-{version}-py3-none-any.whl</a>"#
        ))?;
    }

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["builder>=1"]
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
    )?;
    context.temp_dir.child("build_backend.py").write_str("")?;

    context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--index")
        .arg(first_index.path())
        .arg("--default-index")
        .arg(default_index.path())
        .assert()
        .success();

    let lock = context.read("uv.lock");
    assert!(
        lock.contains(r#"name = "builder", version = "1.0.0""#),
        "{lock}"
    );

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--index")
        .arg(first_index.path())
        .arg("--default-index")
        .arg(default_index.path())
        .arg("--index-strategy")
        .arg("unsafe-best-match")
        .arg("--locked"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided.

    hint: To update the lockfile, run `uv lock`.
    ");

    fs_err::remove_file(context.temp_dir.child("uv.lock"))?;
    context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--index")
        .arg(first_index.path())
        .arg("--default-index")
        .arg(default_index.path())
        .arg("--index-strategy")
        .arg("unsafe-best-match")
        .assert()
        .success();

    let lock = context.read("uv.lock");
    assert!(
        lock.contains(r#"name = "builder", version = "2.0.0""#),
        "{lock}"
    );

    Ok(())
}

/// Verify that adding a higher-version flat index invalidates a locked build environment.
#[test]
fn lock_build_dependencies_find_links_invalidates() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let first_links = context.temp_dir.child("first-links");
    let additional_links = context.temp_dir.child("additional-links");
    first_links.create_dir_all()?;
    additional_links.create_dir_all()?;
    write_wheel(
        &first_links.child("builder-1.0.0-py3-none-any.whl"),
        "builder",
        "1.0.0",
    )?;
    write_wheel(
        &additional_links.child("builder-2.0.0-py3-none-any.whl"),
        "builder",
        "2.0.0",
    )?;

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["builder>=1"]
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
    )?;
    context.temp_dir.child("build_backend.py").write_str("")?;

    context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--find-links")
        .arg(first_links.path())
        .arg("--no-index")
        .assert()
        .success();

    let lock = context.read("uv.lock");
    assert!(
        lock.contains(r#"name = "builder", version = "1.0.0""#),
        "{lock}"
    );

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--find-links")
        .arg(first_links.path())
        .arg("--find-links")
        .arg(additional_links.path())
        .arg("--no-index")
        .arg("--locked"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided.

    hint: To update the lockfile, run `uv lock`.
    ");

    fs_err::remove_file(context.temp_dir.child("uv.lock"))?;
    context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--find-links")
        .arg(first_links.path())
        .arg("--find-links")
        .arg(additional_links.path())
        .arg("--no-index")
        .assert()
        .success();

    let lock = context.read("uv.lock");
    assert!(
        lock.contains(r#"name = "builder", version = "2.0.0""#),
        "{lock}"
    );

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
    let builder_url = Url::from_directory_path(builder_dir.path()).expect("valid builder URL");

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
    let builder_url = Url::from_directory_path(builder_dir.path()).expect("valid builder URL");

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
    let builder_url = Url::from_directory_path(builder_dir.path()).expect("valid builder URL");

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
    let leaf_url = Url::from_directory_path(leaf_dir.path()).expect("valid leaf URL");

    let carrier_dir = context.temp_dir.child("carrier");
    carrier_dir.create_dir_all()?;
    let carrier_url = Url::from_directory_path(carrier_dir.path()).expect("valid carrier URL");
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

/// Verify that a fragment-bearing direct URL source reached through build requirements is captured
/// and can replay its own locked build requirements during a frozen sync.
#[tokio::test]
async fn lock_build_dependencies_replay_fragmented_direct_url_source() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    write_wheel(
        &links_dir.child("helper-1.0.0-py3-none-any.whl"),
        "helper",
        "1.0.0",
    )?;

    let source_dist = context.temp_dir.child("backend-1.0.0.zip");
    write_build_source(
        &source_dist,
        "backend",
        "1.0.0",
        Some(("helper==1.0.0", "helper")),
    )?;
    let archive = fs_err::read(source_dist.path())?;
    let archive_hash = format!("{:x}", Sha256::digest(&archive));

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/backend-1.0.0.zip"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(archive))
        .mount(&server)
        .await;
    let authenticated_uri = server.uri().replacen("://", "://user:secret@", 1);

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(&format!(
        r#"
        [project]
        name = "dep"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["backend @ {authenticated_uri}/backend-1.0.0.zip#egg=backend&sha256={archive_hash}"]
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
    filename = "dep-0.1.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("dep.py", "")
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
    let backend = package_section(&lock, "backend");
    assert!(backend.contains("build-dependencies = ["), "{backend}");
    assert!(
        backend.contains(r#"{ name = "helper", version = "1.0.0" }"#),
        "{backend}"
    );

    uv_snapshot!(context.filters(), context
        .sync()
        .arg("--no-index")
        .arg("--no-cache")
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

    // A changed digest in a compound fragment must conflict with the captured artifact during
    // frozen replay, even though the normalized source identity is unchanged.
    let pyproject = dep_dir.child("pyproject.toml");
    let contents = fs_err::read_to_string(pyproject.path())?;
    pyproject.write_str(&contents.replace(&archive_hash, &"0".repeat(64)))?;
    let output = context
        .sync()
        .arg("--no-index")
        .arg("--no-cache")
        .arg("--frozen")
        .arg("--reinstall")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .failure();
    let stderr = String::from_utf8_lossy(&output.get_output().stderr);
    assert!(
        stderr.contains("Conflicting archive URL hashes"),
        "{stderr}"
    );

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

/// A direct-URL build requirement with a valid PEP 508 hash fragment must replay from a frozen
/// build resolution after the lock normalizes the verbatim URL.
#[tokio::test]
async fn lock_build_dependencies_replay_direct_url_fragment() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let helper = context.temp_dir.child("helper-0.1.0-py3-none-any.whl");
    write_wheel(&helper, "helper", "0.1.0")?;
    let bytes = fs_err::read(helper.path())?;
    let hash = format!("{:x}", Sha256::digest(&bytes));
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/helper-0.1.0-py3-none-any.whl"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(bytes))
        .mount(&server)
        .await;

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(&format!(
        r#"
        [project]
        name = "dep"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["helper @ {}/helper-0.1.0-py3-none-any.whl#sha256={hash}"]
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
        server.uri(),
    ))?;
    dep_dir.child("build_backend.py").write_str(
        r#"
from pathlib import Path
from zipfile import ZipFile

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    filename = "dep-0.1.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("dep.py", "VALUE = 'built'\n")
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
        .arg("--no-index")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();
    let lock = context.read("uv.lock");
    let dep = package_section(&lock, "dep");
    assert!(
        dep.contains(r#"{ name = "helper", version = "0.1.0" }"#),
        "{dep}"
    );

    context
        .sync()
        .arg("--no-index")
        .arg("--no-cache")
        .arg("--frozen")
        .arg("--reinstall")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();

    Ok(())
}

/// A credentialed Git source used only as a build dependency must capture its own build graph and
/// replay the locked precise commit when the original requirement selects the default branch.
#[tokio::test]
#[cfg(feature = "test-git")]
async fn lock_build_dependencies_replay_nested_credentialed_git_source() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    write_wheel(
        &links_dir.child("helper-0.1.0-py3-none-any.whl"),
        "helper",
        "0.1.0",
    )?;

    let nested_dir = context.temp_dir.child("nested-backend");
    nested_dir.create_dir_all()?;
    nested_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "nested-backend"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["helper==0.1.0"]
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
    )?;
    nested_dir.child("build_backend.py").write_str(
        r#"
from pathlib import Path
from zipfile import ZipFile

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    filename = "nested_backend-0.1.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("nested_backend.py", "VALUE = 'nested'\n")
        wheel.writestr(
            "nested_backend-0.1.0.dist-info/METADATA",
            "Metadata-Version: 2.3\nName: nested-backend\nVersion: 0.1.0\n",
        )
        wheel.writestr(
            "nested_backend-0.1.0.dist-info/WHEEL",
            "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
        )
        wheel.writestr("nested_backend-0.1.0.dist-info/RECORD", "")
    return filename
"#,
    )?;
    Command::new("git")
        .arg("init")
        .arg("--quiet")
        .arg(nested_dir.path())
        .assert()
        .success();
    Command::new("git")
        .args(["-C", nested_dir.path().to_str().expect("UTF-8 temp path")])
        .args(["config", "user.name", "uv-test"])
        .assert()
        .success();
    Command::new("git")
        .args(["-C", nested_dir.path().to_str().expect("UTF-8 temp path")])
        .args(["config", "user.email", "uv-test@example.com"])
        .assert()
        .success();
    Command::new("git")
        .args(["-C", nested_dir.path().to_str().expect("UTF-8 temp path")])
        .args(["add", "."])
        .assert()
        .success();
    Command::new("git")
        .args(["-C", nested_dir.path().to_str().expect("UTF-8 temp path")])
        .args(["commit", "--quiet", "-m", "initial"])
        .assert()
        .success();
    Command::new("git")
        .args(["-C", nested_dir.path().to_str().expect("UTF-8 temp path")])
        .arg("update-server-info")
        .assert()
        .success();

    let server = MockServer::start().await;
    let git_dir = nested_dir.child(".git");
    for entry in WalkDir::new(git_dir.path()).into_iter().flatten() {
        if !entry.file_type().is_file() {
            continue;
        }
        let relative = entry.path().strip_prefix(git_dir.path())?;
        let route = format!(
            "/nested.git/{}",
            relative.to_string_lossy().replace('\\', "/")
        );
        Mock::given(method("GET"))
            .and(path(route))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(fs_err::read(entry.path())?))
            .mount(&server)
            .await;
    }
    let nested_url = server.uri().replacen("http://", "http://user:secret@", 1);

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(&format!(
        r#"
        [project]
        name = "dep"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["nested-backend"]
        backend-path = ["."]
        build-backend = "build_backend"

        [tool.uv.sources]
        nested-backend = {{ git = "{nested_url}/nested.git" }}
        "#,
    ))?;
    dep_dir.child("build_backend.py").write_str(
        r#"
from pathlib import Path
from zipfile import ZipFile
import nested_backend

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    filename = "dep-0.1.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("dep.py", f"VALUE = {nested_backend.VALUE!r}\n")
        wheel.writestr("dep-0.1.0.dist-info/METADATA", "Metadata-Version: 2.3\nName: dep\nVersion: 0.1.0\n")
        wheel.writestr("dep-0.1.0.dist-info/WHEEL", "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n")
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
    let nested = package_section(&lock, "nested-backend");
    assert!(
        nested.contains(r#"{ name = "helper", version = "0.1.0" }"#),
        "{nested}"
    );
    assert!(!lock.contains("user:secret"), "{lock}");

    // Move the default branch after locking. Frozen replay must continue to build against the
    // precise commit captured in the lock, even with an empty cache.
    let backend = nested_dir.child("build_backend.py");
    let contents = fs_err::read_to_string(backend.path())?;
    backend.write_str(&contents.replace("VALUE = 'nested'", "VALUE = 'advanced'"))?;
    Command::new("git")
        .args(["-C", nested_dir.path().to_str().expect("UTF-8 temp path")])
        .args(["add", "."])
        .assert()
        .success();
    Command::new("git")
        .args(["-C", nested_dir.path().to_str().expect("UTF-8 temp path")])
        .args(["commit", "--quiet", "-m", "advance"])
        .assert()
        .success();
    Command::new("git")
        .args(["-C", nested_dir.path().to_str().expect("UTF-8 temp path")])
        .arg("update-server-info")
        .assert()
        .success();
    server.reset().await;
    for entry in WalkDir::new(git_dir.path()).into_iter().flatten() {
        if !entry.file_type().is_file() {
            continue;
        }
        let relative = entry.path().strip_prefix(git_dir.path())?;
        let route = format!(
            "/nested.git/{}",
            relative.to_string_lossy().replace('\\', "/")
        );
        Mock::given(method("GET"))
            .and(path(route))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(fs_err::read(entry.path())?))
            .mount(&server)
            .await;
    }

    context
        .sync()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--no-cache")
        .arg("--frozen")
        .arg("--reinstall")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();

    uv_snapshot!(context.filters(), context
        .run()
        .arg("--no-sync")
        .arg("python")
        .arg("-c")
        .arg("import dep; print(dep.VALUE)"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    nested

    ----- stderr -----
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
        id = "build:flit-core:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        name = "flit-core"

        [[resolution]]
        id = "build:hatch-vcs:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
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
        id = "build:iniconfig:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        name = "iniconfig"
        roots = [
            { name = "hatch-vcs", version = "0.4.0" },
            { name = "hatchling", version = "1.22.4" },
        ]

        [[resolution]]
        id = "build:packaging:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        name = "packaging"
        roots = [
            { name = "flit-core", version = "3.9.0" },
        ]

        [[resolution]]
        id = "build:pathspec:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
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
        id = "build:typing-extensions:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        name = "typing-extensions"
        roots = [
            { name = "flit-core", version = "3.9.0" },
        ]

        [[resolution]]
        id = "build:wheel:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
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
        sdist = { url = "https://files.pythonhosted.org/packages/b5/00/96cbed7c019c49ee04b8a08357a981983db7698ae6de402e57097cefc9ad/calver-2022.6.26.tar.gz", hash = "sha256:e05493a3b17517ef1748fbe610da11f10485faa7c416b9d33fd4a52d74894f8b", size = 6670, upload-time = "2022-06-26T23:25:10.382Z", requires-python = ">=3.5" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/f7/39/e421c06f42ca00fa9cf8929c2466e58a837e8e97b8ab3ff4f4ff9a15e33e/calver-2022.6.26-py3-none-any.whl", hash = "sha256:a1d7fcdd67797afc52ee36ffb8c8adf6643173864306547bfd1380cbce6310a0", size = 7049, upload-time = "2022-06-26T23:25:07.692Z", requires-python = ">=3.5" },
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
        sdist = { url = "https://files.pythonhosted.org/packages/c4/e6/c1ac50fe3eebb38a155155711e6e864e254ce4b6e17fe2429b4c4d5b9e80/flit_core-3.9.0.tar.gz", hash = "sha256:72ad266176c4a3fcfab5f2930d76896059851240570ce9a98733b658cb786eba", size = 41917, upload-time = "2023-05-14T14:48:51.809Z", requires-python = ">=3.6" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/38/45/618e84e49a6c51e5dd15565ec2fcd82ab273434f236b8f108f065ded517a/flit_core-3.9.0-py3-none-any.whl", hash = "sha256:7aada352fb0c7f5538c4fafeddf314d3a6a92ee8e2b1de70482329e42de70301", size = 63141, upload-time = "2023-05-14T14:48:49.24Z", requires-python = ">=3.6" },
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
        sdist = { url = "https://files.pythonhosted.org/packages/f5/c9/54bb4fa27b4e4a014ef3bb17710cdf692b3aa2cbc7953da885f1bf7e06ea/hatch_vcs-0.4.0.tar.gz", hash = "sha256:093810748fe01db0d451fabcf2c1ac2688caefd232d4ede967090b1c1b07d9f7", size = 10917, upload-time = "2023-11-06T06:24:57.228Z", requires-python = ">=3.8" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/82/0f/6cbd9976160bc334add63bc2e7a58b1433a31b34b7cda6c5de6dd983d9a7/hatch_vcs-0.4.0-py3-none-any.whl", hash = "sha256:b8a2b6bee54cf6f9fc93762db73890017ae59c9081d1038a41f16235ceaf8b2c", size = 8412, upload-time = "2023-11-06T06:24:55.389Z", requires-python = ">=3.8" },
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
        sdist = { url = "https://files.pythonhosted.org/packages/4f/2a/c34d71531d1e1c9a5029bb73eb3816285befd0fffd7c63ffa0544253dca8/hatchling-1.22.4.tar.gz", hash = "sha256:8a2dcec96d7fb848382ef5848e5ac43fdae641f35a08a3fab5116bd495f3416e", size = 62758, upload-time = "2024-03-24T02:00:59.122Z", requires-python = ">=3.8" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/49/63/2d56d6356f9f8b906aa68335cbf5b1b54c69873a2e271eda2ddba319c1ae/hatchling-1.22.4-py3-none-any.whl", hash = "sha256:f56da5bfc396af7b29daa3164851dd04991c994083f56cb054b5003675caecdc", size = 82032, upload-time = "2024-03-24T02:00:57.534Z", requires-python = ">=3.8" },
        ]

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        build-dependencies = [
            { name = "hatch-vcs", version = "0.4.0" },
            { name = "hatchling", version = "1.22.4" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646, upload-time = "2023-01-07T11:08:11.254Z", requires-python = ">=3.7" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892, upload-time = "2023-01-07T11:08:09.864Z", requires-python = ">=3.7" },
        ]

        [[package]]
        name = "packaging"
        version = "24.0"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        build-dependencies = [
            { name = "flit-core", version = "3.9.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/ee/b5/b43a27ac7472e1818c4bafd44430e69605baefe1f34440593e0332ec8b4d/packaging-24.0.tar.gz", hash = "sha256:eb82c5e3e56209074766e6885bb04b8c38a0c015d0a30036ebe7ece34c9989e9", size = 147882, upload-time = "2024-03-10T09:39:28.33Z", requires-python = ">=3.7" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/49/df/1fceb2f8900f8639e278b056416d49134fb8d84c5942ffaa01ad34782422/packaging-24.0-py3-none-any.whl", hash = "sha256:2ddfb553fdf02fb784c234c7ba6ccc288296ceabec964ad2eae3777778130bc5", size = 53488, upload-time = "2024-03-10T09:39:25.947Z", requires-python = ">=3.7" },
        ]

        [[package]]
        name = "pathspec"
        version = "0.12.1"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        build-dependencies = [
            { name = "flit-core", version = "3.9.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/ca/bc/f35b8446f4531a7cb215605d100cd88b7ac6f44ab3fc94870c120ab3adbf/pathspec-0.12.1.tar.gz", hash = "sha256:a482d51503a1ab33b1c67a6c3813a26953dbdc71c31dacaef9a838c4e29f5712", size = 51043, upload-time = "2023-12-10T22:30:45Z", requires-python = ">=3.8" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/cc/20/ff623b09d963f88bfde16306a54e12ee5ea43e9b597108672ff3a408aad6/pathspec-0.12.1-py3-none-any.whl", hash = "sha256:a0d503e138a4c123b27490a4f7beda6a01c6f288df0e4a8b79c7eb0dc7b4cc08", size = 31191, upload-time = "2023-12-10T22:30:43.14Z", requires-python = ">=3.8" },
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
        sdist = { url = "https://files.pythonhosted.org/packages/54/c6/43f9d44d92aed815e781ca25ba8c174257e27253a94630d21be8725a2b59/pluggy-1.4.0.tar.gz", hash = "sha256:8c85c2876142a764e5b7548e7d9a0e0ddb46f5185161049a79b7e974454223be", size = 65812, upload-time = "2024-01-24T13:45:15.875Z", requires-python = ">=3.8" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/a5/5b/0cc789b59e8cc1bf288b38111d002d8c5917123194d45b29dcdac64723cc/pluggy-1.4.0-py3-none-any.whl", hash = "sha256:7db9f7b503d67d1c5b95f59773ebb58a8c1c288129a88665838012cfb07b8981", size = 20120, upload-time = "2024-01-24T13:45:14.227Z", requires-python = ">=3.8" },
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
        sdist = { url = "https://files.pythonhosted.org/packages/4d/5b/dc575711b6b8f2f866131a40d053e30e962e633b332acf7cd2c24843d83d/setuptools-69.2.0.tar.gz", hash = "sha256:0ff4183f8f42cd8fa3acea16c45205521a4ef28f73c6391d8a25e92893134f2e", size = 2222950, upload-time = "2024-03-13T11:20:59.219Z", requires-python = ">=3.8" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/92/e1/1c8bb3420105e70bdf357d57dd5567202b4ef8d27f810e98bb962d950834/setuptools-69.2.0-py3-none-any.whl", hash = "sha256:c21c49fb1042386df081cb5d86759792ab89efca84cf114889191cd09aacc80c", size = 821485, upload-time = "2024-03-13T11:20:54.103Z", requires-python = ">=3.8" },
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
        sdist = { url = "https://files.pythonhosted.org/packages/eb/b1/0248705f10f6de5eefe7ff93e399f7192257b23df4d431d2f5680bb2778f/setuptools-scm-8.0.4.tar.gz", hash = "sha256:b5f43ff6800669595193fd09891564ee9d1d7dcb196cab4b2506d53a2e1c95c7", size = 74280, upload-time = "2023-10-02T15:14:32.996Z", requires-python = ">=3.8" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/0e/a3/b9a8b0adfe672bf0df5901707aa929d30a97ee390ba651910186776746d2/setuptools_scm-8.0.4-py3-none-any.whl", hash = "sha256:b47844cd2a84b83b3187a5782c71128c28b4c94cad8bfb871da2784a5cb54c4f", size = 42137, upload-time = "2023-10-02T15:14:31.281Z", requires-python = ">=3.8" },
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
        sdist = { url = "https://files.pythonhosted.org/packages/16/3a/0d26ce356c7465a19c9ea8814b960f8a36c3b0d07c323176620b7b483e44/typing_extensions-4.10.0.tar.gz", hash = "sha256:b0abd7c89e8fb96f98db18d86106ff1d90ab692004eb746cf6eda2682f91b3cb", size = 77558, upload-time = "2024-02-25T22:12:49.693Z", requires-python = ">=3.8" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/f9/de/dc04a3ea60b22624b51c703a84bbe0184abcd1d0b9bc8074b5d6b7ab90bb/typing_extensions-4.10.0-py3-none-any.whl", hash = "sha256:69b1a937c3a517342112fb4c6df7e72fc39a38e7891a5730ed4985b5214b5475", size = 33926, upload-time = "2024-02-25T22:12:47.72Z", requires-python = ">=3.8" },
        ]

        [[package]]
        name = "wheel"
        version = "0.43.0"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        build-dependencies = [
            { name = "flit-core", version = "3.9.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/b8/d6/ac9cd92ea2ad502ff7c1ab683806a9deb34711a1e2bd8a59814e8fc27e69/wheel-0.43.0.tar.gz", hash = "sha256:465ef92c69fa5c5da2d1cf8ac40559a8c940886afcef87dcf14b9470862f1d85", size = 99109, upload-time = "2024-03-11T19:29:17.32Z", requires-python = ">=3.8" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/7d/cd/d7460c9a869b16c3dd4e1e403cce337df165368c71d6af229a74699622ce/wheel-0.43.0-py3-none-any.whl", hash = "sha256:55c570405f142630c6b9f72fe09d9b67cf1477fcf543ae5b8dcb1f5b7377da81", size = 65775, upload-time = "2024-03-11T19:29:15.522Z", requires-python = ">=3.8" },
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
        id = "build:dep:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        name = "dep"
        roots = [
            { name = "iniconfig", version = "2.0.0" },
            { name = "setuptools", version = "69.2.0" },
            { name = "wheel", version = "0.43.0" },
        ]

        [[resolution]]
        id = "build:flit-core:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        name = "flit-core"

        [[resolution]]
        id = "build:hatch-vcs:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
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
        id = "build:iniconfig:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        name = "iniconfig"
        roots = [
            { name = "hatch-vcs", version = "0.4.0" },
            { name = "hatchling", version = "1.22.4" },
        ]

        [[resolution]]
        id = "build:packaging:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        name = "packaging"
        roots = [
            { name = "flit-core", version = "3.9.0" },
        ]

        [[resolution]]
        id = "build:pathspec:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
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
        id = "build:typing-extensions:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        name = "typing-extensions"
        roots = [
            { name = "flit-core", version = "3.9.0" },
        ]

        [[resolution]]
        id = "build:wheel:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
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
        sdist = { url = "https://files.pythonhosted.org/packages/b5/00/96cbed7c019c49ee04b8a08357a981983db7698ae6de402e57097cefc9ad/calver-2022.6.26.tar.gz", hash = "sha256:e05493a3b17517ef1748fbe610da11f10485faa7c416b9d33fd4a52d74894f8b", size = 6670, upload-time = "2022-06-26T23:25:10.382Z", requires-python = ">=3.5" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/f7/39/e421c06f42ca00fa9cf8929c2466e58a837e8e97b8ab3ff4f4ff9a15e33e/calver-2022.6.26-py3-none-any.whl", hash = "sha256:a1d7fcdd67797afc52ee36ffb8c8adf6643173864306547bfd1380cbce6310a0", size = 7049, upload-time = "2022-06-26T23:25:07.692Z", requires-python = ">=3.5" },
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
        sdist = { url = "https://files.pythonhosted.org/packages/c4/e6/c1ac50fe3eebb38a155155711e6e864e254ce4b6e17fe2429b4c4d5b9e80/flit_core-3.9.0.tar.gz", hash = "sha256:72ad266176c4a3fcfab5f2930d76896059851240570ce9a98733b658cb786eba", size = 41917, upload-time = "2023-05-14T14:48:51.809Z", requires-python = ">=3.6" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/38/45/618e84e49a6c51e5dd15565ec2fcd82ab273434f236b8f108f065ded517a/flit_core-3.9.0-py3-none-any.whl", hash = "sha256:7aada352fb0c7f5538c4fafeddf314d3a6a92ee8e2b1de70482329e42de70301", size = 63141, upload-time = "2023-05-14T14:48:49.24Z", requires-python = ">=3.6" },
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
        sdist = { url = "https://files.pythonhosted.org/packages/f5/c9/54bb4fa27b4e4a014ef3bb17710cdf692b3aa2cbc7953da885f1bf7e06ea/hatch_vcs-0.4.0.tar.gz", hash = "sha256:093810748fe01db0d451fabcf2c1ac2688caefd232d4ede967090b1c1b07d9f7", size = 10917, upload-time = "2023-11-06T06:24:57.228Z", requires-python = ">=3.8" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/82/0f/6cbd9976160bc334add63bc2e7a58b1433a31b34b7cda6c5de6dd983d9a7/hatch_vcs-0.4.0-py3-none-any.whl", hash = "sha256:b8a2b6bee54cf6f9fc93762db73890017ae59c9081d1038a41f16235ceaf8b2c", size = 8412, upload-time = "2023-11-06T06:24:55.389Z", requires-python = ">=3.8" },
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
        sdist = { url = "https://files.pythonhosted.org/packages/4f/2a/c34d71531d1e1c9a5029bb73eb3816285befd0fffd7c63ffa0544253dca8/hatchling-1.22.4.tar.gz", hash = "sha256:8a2dcec96d7fb848382ef5848e5ac43fdae641f35a08a3fab5116bd495f3416e", size = 62758, upload-time = "2024-03-24T02:00:59.122Z", requires-python = ">=3.8" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/49/63/2d56d6356f9f8b906aa68335cbf5b1b54c69873a2e271eda2ddba319c1ae/hatchling-1.22.4-py3-none-any.whl", hash = "sha256:f56da5bfc396af7b29daa3164851dd04991c994083f56cb054b5003675caecdc", size = 82032, upload-time = "2024-03-24T02:00:57.534Z", requires-python = ">=3.8" },
        ]

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        build-dependencies = [
            { name = "hatch-vcs", version = "0.4.0" },
            { name = "hatchling", version = "1.22.4" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646, upload-time = "2023-01-07T11:08:11.254Z", requires-python = ">=3.7" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892, upload-time = "2023-01-07T11:08:09.864Z", requires-python = ">=3.7" },
        ]

        [[package]]
        name = "packaging"
        version = "24.0"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        build-dependencies = [
            { name = "flit-core", version = "3.9.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/ee/b5/b43a27ac7472e1818c4bafd44430e69605baefe1f34440593e0332ec8b4d/packaging-24.0.tar.gz", hash = "sha256:eb82c5e3e56209074766e6885bb04b8c38a0c015d0a30036ebe7ece34c9989e9", size = 147882, upload-time = "2024-03-10T09:39:28.33Z", requires-python = ">=3.7" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/49/df/1fceb2f8900f8639e278b056416d49134fb8d84c5942ffaa01ad34782422/packaging-24.0-py3-none-any.whl", hash = "sha256:2ddfb553fdf02fb784c234c7ba6ccc288296ceabec964ad2eae3777778130bc5", size = 53488, upload-time = "2024-03-10T09:39:25.947Z", requires-python = ">=3.7" },
        ]

        [[package]]
        name = "pathspec"
        version = "0.12.1"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        build-dependencies = [
            { name = "flit-core", version = "3.9.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/ca/bc/f35b8446f4531a7cb215605d100cd88b7ac6f44ab3fc94870c120ab3adbf/pathspec-0.12.1.tar.gz", hash = "sha256:a482d51503a1ab33b1c67a6c3813a26953dbdc71c31dacaef9a838c4e29f5712", size = 51043, upload-time = "2023-12-10T22:30:45Z", requires-python = ">=3.8" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/cc/20/ff623b09d963f88bfde16306a54e12ee5ea43e9b597108672ff3a408aad6/pathspec-0.12.1-py3-none-any.whl", hash = "sha256:a0d503e138a4c123b27490a4f7beda6a01c6f288df0e4a8b79c7eb0dc7b4cc08", size = 31191, upload-time = "2023-12-10T22:30:43.14Z", requires-python = ">=3.8" },
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
        sdist = { url = "https://files.pythonhosted.org/packages/54/c6/43f9d44d92aed815e781ca25ba8c174257e27253a94630d21be8725a2b59/pluggy-1.4.0.tar.gz", hash = "sha256:8c85c2876142a764e5b7548e7d9a0e0ddb46f5185161049a79b7e974454223be", size = 65812, upload-time = "2024-01-24T13:45:15.875Z", requires-python = ">=3.8" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/a5/5b/0cc789b59e8cc1bf288b38111d002d8c5917123194d45b29dcdac64723cc/pluggy-1.4.0-py3-none-any.whl", hash = "sha256:7db9f7b503d67d1c5b95f59773ebb58a8c1c288129a88665838012cfb07b8981", size = 20120, upload-time = "2024-01-24T13:45:14.227Z", requires-python = ">=3.8" },
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
        sdist = { url = "https://files.pythonhosted.org/packages/4d/5b/dc575711b6b8f2f866131a40d053e30e962e633b332acf7cd2c24843d83d/setuptools-69.2.0.tar.gz", hash = "sha256:0ff4183f8f42cd8fa3acea16c45205521a4ef28f73c6391d8a25e92893134f2e", size = 2222950, upload-time = "2024-03-13T11:20:59.219Z", requires-python = ">=3.8" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/92/e1/1c8bb3420105e70bdf357d57dd5567202b4ef8d27f810e98bb962d950834/setuptools-69.2.0-py3-none-any.whl", hash = "sha256:c21c49fb1042386df081cb5d86759792ab89efca84cf114889191cd09aacc80c", size = 821485, upload-time = "2024-03-13T11:20:54.103Z", requires-python = ">=3.8" },
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
        sdist = { url = "https://files.pythonhosted.org/packages/eb/b1/0248705f10f6de5eefe7ff93e399f7192257b23df4d431d2f5680bb2778f/setuptools-scm-8.0.4.tar.gz", hash = "sha256:b5f43ff6800669595193fd09891564ee9d1d7dcb196cab4b2506d53a2e1c95c7", size = 74280, upload-time = "2023-10-02T15:14:32.996Z", requires-python = ">=3.8" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/0e/a3/b9a8b0adfe672bf0df5901707aa929d30a97ee390ba651910186776746d2/setuptools_scm-8.0.4-py3-none-any.whl", hash = "sha256:b47844cd2a84b83b3187a5782c71128c28b4c94cad8bfb871da2784a5cb54c4f", size = 42137, upload-time = "2023-10-02T15:14:31.281Z", requires-python = ">=3.8" },
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
        sdist = { url = "https://files.pythonhosted.org/packages/16/3a/0d26ce356c7465a19c9ea8814b960f8a36c3b0d07c323176620b7b483e44/typing_extensions-4.10.0.tar.gz", hash = "sha256:b0abd7c89e8fb96f98db18d86106ff1d90ab692004eb746cf6eda2682f91b3cb", size = 77558, upload-time = "2024-02-25T22:12:49.693Z", requires-python = ">=3.8" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/f9/de/dc04a3ea60b22624b51c703a84bbe0184abcd1d0b9bc8074b5d6b7ab90bb/typing_extensions-4.10.0-py3-none-any.whl", hash = "sha256:69b1a937c3a517342112fb4c6df7e72fc39a38e7891a5730ed4985b5214b5475", size = 33926, upload-time = "2024-02-25T22:12:47.72Z", requires-python = ">=3.8" },
        ]

        [[package]]
        name = "wheel"
        version = "0.43.0"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        build-dependencies = [
            { name = "flit-core", version = "3.9.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/b8/d6/ac9cd92ea2ad502ff7c1ab683806a9deb34711a1e2bd8a59814e8fc27e69/wheel-0.43.0.tar.gz", hash = "sha256:465ef92c69fa5c5da2d1cf8ac40559a8c940886afcef87dcf14b9470862f1d85", size = 99109, upload-time = "2024-03-11T19:29:17.32Z", requires-python = ">=3.8" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/7d/cd/d7460c9a869b16c3dd4e1e403cce337df165368c71d6af229a74699622ce/wheel-0.43.0-py3-none-any.whl", hash = "sha256:55c570405f142630c6b9f72fe09d9b67cf1477fcf543ae5b8dcb1f5b7377da81", size = 65775, upload-time = "2024-03-11T19:29:15.522Z", requires-python = ">=3.8" },
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
    assert!(lock.starts_with("version = 1\nrevision = 3\n"), "{lock}");
    assert!(project.contains(r#"source = { virtual = "." }"#));
    assert!(!project.contains("build-dependencies = ["));
    assert!(!lock.contains("[[resolution]]"), "{lock}");
    assert!(!lock.contains("build-requires"), "{lock}");
    assert!(!lock.contains("build-system"), "{lock}");
    assert!(!lock.contains("build-settings"), "{lock}");

    Ok(())
}

/// Verify a wheel-only project preserves the default lock schema, even when build settings are set.
#[test]
fn lock_build_dependencies_wheel_only_preserves_lock_schema() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    write_wheel(
        &links_dir.child("runtime-0.1.0-py3-none-any.whl"),
        "runtime",
        "0.1.0",
    )?;

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["runtime==0.1.0"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--no-index")
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--config-settings")
        .arg("choice=a"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    assert!(lock.starts_with("version = 1\nrevision = 3\n"), "{lock}");
    assert!(!lock.contains("[[resolution]]"), "{lock}");
    assert!(!lock.contains("build-dependencies"), "{lock}");
    assert!(!lock.contains("build-requires"), "{lock}");
    assert!(!lock.contains("build-system"), "{lock}");
    assert!(!lock.contains("build-settings"), "{lock}");

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--no-index")
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--config-settings")
        .arg("choice=a")
        .arg("--locked"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

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
        id = "build:dep:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        name = "dep"
        roots = [
            { name = "setuptools", version = "69.2.0" },
            { name = "wheel", version = "0.43.0" },
        ]

        [[resolution]]
        id = "build:flit-core:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
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
        id = "build:wheel:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
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
        sdist = { url = "https://files.pythonhosted.org/packages/c4/e6/c1ac50fe3eebb38a155155711e6e864e254ce4b6e17fe2429b4c4d5b9e80/flit_core-3.9.0.tar.gz", hash = "sha256:72ad266176c4a3fcfab5f2930d76896059851240570ce9a98733b658cb786eba", size = 41917, upload-time = "2023-05-14T14:48:51.809Z", requires-python = ">=3.6" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/38/45/618e84e49a6c51e5dd15565ec2fcd82ab273434f236b8f108f065ded517a/flit_core-3.9.0-py3-none-any.whl", hash = "sha256:7aada352fb0c7f5538c4fafeddf314d3a6a92ee8e2b1de70482329e42de70301", size = 63141, upload-time = "2023-05-14T14:48:49.24Z", requires-python = ">=3.6" },
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
        sdist = { url = "https://files.pythonhosted.org/packages/4d/5b/dc575711b6b8f2f866131a40d053e30e962e633b332acf7cd2c24843d83d/setuptools-69.2.0.tar.gz", hash = "sha256:0ff4183f8f42cd8fa3acea16c45205521a4ef28f73c6391d8a25e92893134f2e", size = 2222950, upload-time = "2024-03-13T11:20:59.219Z", requires-python = ">=3.8" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/92/e1/1c8bb3420105e70bdf357d57dd5567202b4ef8d27f810e98bb962d950834/setuptools-69.2.0-py3-none-any.whl", hash = "sha256:c21c49fb1042386df081cb5d86759792ab89efca84cf114889191cd09aacc80c", size = 821485, upload-time = "2024-03-13T11:20:54.103Z", requires-python = ">=3.8" },
        ]

        [[package]]
        name = "wheel"
        version = "0.43.0"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        build-dependencies = [
            { name = "flit-core", version = "3.9.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/b8/d6/ac9cd92ea2ad502ff7c1ab683806a9deb34711a1e2bd8a59814e8fc27e69/wheel-0.43.0.tar.gz", hash = "sha256:465ef92c69fa5c5da2d1cf8ac40559a8c940886afcef87dcf14b9470862f1d85", size = 99109, upload-time = "2024-03-11T19:29:17.32Z", requires-python = ">=3.8" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/7d/cd/d7460c9a869b16c3dd4e1e403cce337df165368c71d6af229a74699622ce/wheel-0.43.0-py3-none-any.whl", hash = "sha256:55c570405f142630c6b9f72fe09d9b67cf1477fcf543ae5b8dcb1f5b7377da81", size = 65775, upload-time = "2024-03-11T19:29:15.522Z", requires-python = ">=3.8" },
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
    let builder_url = Url::from_directory_path(builder_dir.path()).expect("valid builder URL");

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
        id = "build:dep:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        name = "dep"
        roots = [
            { name = "setuptools", version = "69.2.0" },
            { name = "wheel", version = "0.43.0" },
        ]

        [[resolution]]
        id = "build:flit-core:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
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
        id = "build:wheel:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
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
        sdist = { url = "https://files.pythonhosted.org/packages/c4/e6/c1ac50fe3eebb38a155155711e6e864e254ce4b6e17fe2429b4c4d5b9e80/flit_core-3.9.0.tar.gz", hash = "sha256:72ad266176c4a3fcfab5f2930d76896059851240570ce9a98733b658cb786eba", size = 41917, upload-time = "2023-05-14T14:48:51.809Z", requires-python = ">=3.6" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/38/45/618e84e49a6c51e5dd15565ec2fcd82ab273434f236b8f108f065ded517a/flit_core-3.9.0-py3-none-any.whl", hash = "sha256:7aada352fb0c7f5538c4fafeddf314d3a6a92ee8e2b1de70482329e42de70301", size = 63141, upload-time = "2023-05-14T14:48:49.24Z", requires-python = ">=3.6" },
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
        sdist = { url = "https://files.pythonhosted.org/packages/4d/5b/dc575711b6b8f2f866131a40d053e30e962e633b332acf7cd2c24843d83d/setuptools-69.2.0.tar.gz", hash = "sha256:0ff4183f8f42cd8fa3acea16c45205521a4ef28f73c6391d8a25e92893134f2e", size = 2222950, upload-time = "2024-03-13T11:20:59.219Z", requires-python = ">=3.8" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/92/e1/1c8bb3420105e70bdf357d57dd5567202b4ef8d27f810e98bb962d950834/setuptools-69.2.0-py3-none-any.whl", hash = "sha256:c21c49fb1042386df081cb5d86759792ab89efca84cf114889191cd09aacc80c", size = 821485, upload-time = "2024-03-13T11:20:54.103Z", requires-python = ">=3.8" },
        ]

        [[package]]
        name = "wheel"
        version = "0.43.0"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        build-dependencies = [
            { name = "flit-core", version = "3.9.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/b8/d6/ac9cd92ea2ad502ff7c1ab683806a9deb34711a1e2bd8a59814e8fc27e69/wheel-0.43.0.tar.gz", hash = "sha256:465ef92c69fa5c5da2d1cf8ac40559a8c940886afcef87dcf14b9470862f1d85", size = 99109, upload-time = "2024-03-11T19:29:17.32Z", requires-python = ">=3.8" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/7d/cd/d7460c9a869b16c3dd4e1e403cce337df165368c71d6af229a74699622ce/wheel-0.43.0-py3-none-any.whl", hash = "sha256:55c570405f142630c6b9f72fe09d9b67cf1477fcf543ae5b8dcb1f5b7377da81", size = 65775, upload-time = "2024-03-11T19:29:15.522Z", requires-python = ">=3.8" },
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

/// Verify that equivalent nested build graphs captured before and after resolving dynamic source
/// metadata share a single bootstrap and build pair.
#[test]
fn lock_build_dependencies_coalesce_dynamic_build_resolution() -> Result<()> {
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
    let helper_url = Url::from_directory_path(helper_dir.path()).expect("valid helper URL");

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
        "#,
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

    uv_snapshot!(context.filters(), context.lock().arg("--preview-features").arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    let helper_resolutions = resolution_sections(&lock)
        .split("\n\n")
        .filter(|resolution| resolution.contains("\nname = \"helper\""))
        .collect::<Vec<_>>()
        .join("\n\n");
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(helper_resolutions, @r#"
        [[resolution]]
        id = "build:helper:wheel:bootstrap:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "bootstrap"
        name = "helper"
        roots = [
            { name = "setuptools", version = "69.2.0" },
        ]

        [[resolution]]
        id = "build:helper:wheel:build:[BUILD-ID]:2"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
        stage = "build"
        name = "helper"
        roots = [
            { name = "setuptools", version = "69.2.0" },
            { name = "wheel", version = "0.43.0" },
        ]
        "#);
    });

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
    let entry = ZipEntryBuilder::new("dep-0.1.0/src/dep/__init__.py".into(), Compression::Stored);
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
    assert!(dep.contains("build-dependencies = []"), "{dep}");
    assert!(
        dep.contains(r#"{ name = "uv-build", specifier = ">=0.7,<10000" }"#),
        "{dep}"
    );
    assert!(dep.contains(r#"build-backend = "uv_build""#), "{dep}");

    let resolutions = resolution_sections(&lock);
    let dep_resolutions = resolutions
        .split("\n\n")
        .filter(|resolution| resolution.contains(r#"name = "dep""#))
        .collect::<Vec<_>>();
    assert_eq!(dep_resolutions.len(), 1, "{lock}");
    assert!(!resolutions.contains("\nstage = "), "{lock}");
    assert!(
        dep_resolutions
            .iter()
            .all(|resolution| !resolution.contains("roots =")),
        "{lock}"
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

    // A compatible direct build replays its captured empty stages without resolving `uv_build`.
    context
        .sync()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--no-index")
        .arg("--no-cache")
        .arg("--frozen")
        .assert()
        .success();
    assert!(context.site_packages().join("dep").exists());

    Ok(())
}

/// Verify that a conditional extra build dependency is captured for another supported executor
/// while the current executor can still use the in-process `uv_build` fast path.
#[test]
fn lock_build_dependencies_direct_build_captures_cross_executor_extra_dependency() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let links = context.temp_dir.child("links");
    links.create_dir_all()?;
    write_wheel(
        &links.child("helper-0.1.0-py3-none-any.whl"),
        "helper",
        "0.1.0",
    )?;

    let mut zip = ZipFileWriter::new(Vec::new());
    let entry = ZipEntryBuilder::new("uv_build.py".into(), Compression::Stored);
    block_on(zip.write_entry_whole(
        entry,
        b"def get_requires_for_build_wheel(config_settings=None):\n    return []\n\ndef get_requires_for_build_editable(config_settings=None):\n    return []\n",
    ))?;
    let entry = ZipEntryBuilder::new(
        "uv_build-0.10.12.dist-info/METADATA".into(),
        Compression::Stored,
    );
    block_on(zip.write_entry_whole(
        entry,
        b"Metadata-Version: 2.3\nName: uv-build\nVersion: 0.10.12\n",
    ))?;
    let entry = ZipEntryBuilder::new(
        "uv_build-0.10.12.dist-info/WHEEL".into(),
        Compression::Stored,
    );
    block_on(zip.write_entry_whole(
        entry,
        b"Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
    ))?;
    let entry = ZipEntryBuilder::new(
        "uv_build-0.10.12.dist-info/RECORD".into(),
        Compression::Stored,
    );
    block_on(zip.write_entry_whole(entry, b""))?;
    fs_err::write(
        links.child("uv_build-0.10.12-py3-none-any.whl").path(),
        block_on(zip.close())?,
    )?;

    let dep = context.temp_dir.child("dep");
    dep.create_dir_all()?;
    dep.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"
        "#,
    )?;
    let module = dep.child("src").child("dep");
    module.create_dir_all()?;
    module.child("__init__.py").touch()?;

    let other_executor_marker = if cfg!(windows) {
        "sys_platform != 'win32'"
    } else {
        "sys_platform == 'win32'"
    };
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
        environments = ["sys_platform == 'win32'", "sys_platform != 'win32'"]

        [tool.uv.sources]
        dep = {{ path = "dep" }}

        [tool.uv.extra-build-dependencies]
        dep = ["helper==0.1.0; {other_executor_marker}"]
        "#
        ))?;

    context
        .lock()
        .arg("--preview-features")
        .arg("extra-build-dependencies,lock-build-dependencies")
        .arg("--find-links")
        .arg(links.path())
        .arg("--no-index")
        .arg("--offline")
        .assert()
        .success();

    let lock = context.read("uv.lock");
    let dep = package_section(&lock, "dep");
    assert!(
        dep.contains(&format!(
            r#"{{ name = "helper", version = "0.1.0", marker = "{other_executor_marker}" }}"#
        )),
        "{dep}"
    );
    assert!(!lock.contains("\nstage = "), "{lock}");
    assert!(
        resolution_sections(&lock).contains(r#"name = "helper""#),
        "{lock}"
    );

    context
        .lock()
        .arg("--preview-features")
        .arg("extra-build-dependencies,lock-build-dependencies")
        .arg("--find-links")
        .arg(links.path())
        .arg("--no-index")
        .arg("--offline")
        .arg("--locked")
        .assert()
        .success();
    context
        .sync()
        .arg("--preview-features")
        .arg("extra-build-dependencies,lock-build-dependencies")
        .arg("--no-index")
        .arg("--offline")
        .arg("--no-cache")
        .arg("--frozen")
        .assert()
        .success();
    assert!(context.site_packages().join("dep").exists());

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

/// Verify that replacing a same-version `--find-links` source archive invalidates its build lock.
#[test]
fn lock_build_dependencies_find_links_sdist_build_requires_invalidate() -> Result<()> {
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
        let entry = ZipEntryBuilder::new("dep-0.1.0/dep/__init__.py".into(), Compression::Stored);
        block_on(zip.write_entry_whole(entry, b""))?;
        fs_err::write(source_dist.path(), block_on(zip.close())?)?;
        Ok(())
    };
    write_source_dist("helper-a")?;

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
        .arg("--no-cache")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();

    let lock = context.read("uv.lock");
    let dep = package_section(&lock, "dep");
    assert!(
        dep.contains(r#"build-requires = [{ name = "helper-a", specifier = "==0.1.0" }]"#),
        "{dep}"
    );

    write_source_dist("helper-b")?;

    uv_snapshot!(context.filters(), context
        .lock()
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

/// Verify that changing extra build dependencies invalidates a direct-URL build lock even when
/// the archive cannot be refreshed in offline mode.
#[tokio::test]
async fn lock_direct_url_extra_build_requires_invalidate_offline() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    write_wheel(
        &links_dir.child("helper-0.1.0-py3-none-any.whl"),
        "helper",
        "0.1.0",
    )?;
    write_wheel(
        &links_dir.child("extra-0.1.0-py3-none-any.whl"),
        "extra",
        "0.1.0",
    )?;

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
        requires = ["helper==0.1.0"]
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
    ))?;
    let entry = ZipEntryBuilder::new("dep-0.1.0/build_backend.py".into(), Compression::Stored);
    block_on(zip.write_entry_whole(entry, b""))?;
    let entry = ZipEntryBuilder::new("dep-0.1.0/dep/__init__.py".into(), Compression::Stored);
    block_on(zip.write_entry_whole(entry, b""))?;
    let archive = block_on(zip.close())?;

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/dep-0.1.0.zip"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(archive))
        .mount(&server)
        .await;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(&format!(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep @ {}/dep-0.1.0.zip"]

        [tool.uv.extra-build-dependencies]
        dep = ["extra==0.1.0"]
        "#,
        server.uri()
    ))?;

    context
        .lock()
        .arg("--preview-features")
        .arg("extra-build-dependencies,lock-build-dependencies")
        .arg("--no-index")
        .arg("--find-links")
        .arg(links_dir.path())
        .assert()
        .success();

    let lock = context.read("uv.lock");
    let dep = package_section(&lock, "dep");
    assert!(
        dep.contains(r#"{ name = "extra", specifier = "==0.1.0" }"#),
        "{dep}"
    );
    assert!(lock.contains("build-settings = "), "{lock}");

    pyproject_toml.write_str(&format!(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep @ {}/dep-0.1.0.zip"]
        "#,
        server.uri()
    ))?;
    server.reset().await;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("extra-build-dependencies,lock-build-dependencies")
        .arg("--no-index")
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--locked")
        .arg("--offline")
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
        id = "build:flit-core:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
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
        id = "build:wheel:wheel:build:[BUILD-ID]"
        kind = "build"
        operation = "wheel"
        mode = "isolated"
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
        sdist = { url = "https://files.pythonhosted.org/packages/c4/e6/c1ac50fe3eebb38a155155711e6e864e254ce4b6e17fe2429b4c4d5b9e80/flit_core-3.9.0.tar.gz", hash = "sha256:72ad266176c4a3fcfab5f2930d76896059851240570ce9a98733b658cb786eba", size = 41917, upload-time = "2023-05-14T14:48:51.809Z", requires-python = ">=3.6" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/38/45/618e84e49a6c51e5dd15565ec2fcd82ab273434f236b8f108f065ded517a/flit_core-3.9.0-py3-none-any.whl", hash = "sha256:7aada352fb0c7f5538c4fafeddf314d3a6a92ee8e2b1de70482329e42de70301", size = 63141, upload-time = "2023-05-14T14:48:49.24Z", requires-python = ">=3.6" },
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
        sdist = { url = "https://files.pythonhosted.org/packages/4d/5b/dc575711b6b8f2f866131a40d053e30e962e633b332acf7cd2c24843d83d/setuptools-69.2.0.tar.gz", hash = "sha256:0ff4183f8f42cd8fa3acea16c45205521a4ef28f73c6391d8a25e92893134f2e", size = 2222950, upload-time = "2024-03-13T11:20:59.219Z", requires-python = ">=3.8" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/92/e1/1c8bb3420105e70bdf357d57dd5567202b4ef8d27f810e98bb962d950834/setuptools-69.2.0-py3-none-any.whl", hash = "sha256:c21c49fb1042386df081cb5d86759792ab89efca84cf114889191cd09aacc80c", size = 821485, upload-time = "2024-03-13T11:20:54.103Z", requires-python = ">=3.8" },
        ]

        [[package]]
        name = "wheel"
        version = "0.43.0"
        source = { registry = "https://pypi.org/simple" }
        build-only = true
        build-dependencies = [
            { name = "flit-core", version = "3.9.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/b8/d6/ac9cd92ea2ad502ff7c1ab683806a9deb34711a1e2bd8a59814e8fc27e69/wheel-0.43.0.tar.gz", hash = "sha256:465ef92c69fa5c5da2d1cf8ac40559a8c940886afcef87dcf14b9470862f1d85", size = 99109, upload-time = "2024-03-11T19:29:17.32Z", requires-python = ">=3.8" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/7d/cd/d7460c9a869b16c3dd4e1e403cce337df165368c71d6af229a74699622ce/wheel-0.43.0-py3-none-any.whl", hash = "sha256:55c570405f142630c6b9f72fe09d9b67cf1477fcf543ae5b8dcb1f5b7377da81", size = 65775, upload-time = "2024-03-11T19:29:15.522Z", requires-python = ">=3.8" },
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
    let helper_url = Url::from_directory_path(helper_dir.path()).expect("valid helper URL");

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

/// Verify that an alternate registry sdist that cannot be selected never runs its backend hook
/// while a compatible wheel serves the active build executor.
#[tokio::test]
async fn lock_build_dependencies_skip_ineligible_alternate_sdist_hooks() -> Result<()> {
    for (sdist_metadata, requirement) in [
        (r#"data-yanked="bad source""#, r#""nested>=0.1.0""#),
        (r#"data-requires-python=">=3.13""#, r#""nested==0.1.0""#),
    ] {
        let context = uv_test::test_context!("3.12");
        let simple = context.temp_dir.child("simple/nested");
        simple.create_dir_all()?;
        let files = context.temp_dir.child("files");
        files.create_dir_all()?;

        write_wheel(
            &files.child("nested-0.1.0-py3-none-any.whl"),
            "nested",
            "0.1.0",
        )?;
        let source_dist = files.child("nested-0.1.0.zip");
        let mut zip = ZipFileWriter::new(Vec::new());
        let entry = ZipEntryBuilder::new("nested-0.1.0/pyproject.toml".into(), Compression::Stored);
        block_on(zip.write_entry_whole(
            entry,
            br#"
            [project]
            name = "nested"
            version = "0.1.0"
            requires-python = ">=3.12"

            [build-system]
            requires = []
            backend-path = ["."]
            build-backend = "build_backend"
            "#,
        ))?;
        let entry =
            ZipEntryBuilder::new("nested-0.1.0/build_backend.py".into(), Compression::Stored);
        block_on(zip.write_entry_whole(
            entry,
            br#"
def get_requires_for_build_wheel(config_settings=None):
    raise RuntimeError("ineligible alternate source distribution hook was called")
"#,
        ))?;
        fs_err::write(source_dist.path(), block_on(zip.close())?)?;

        simple.child("index.html").write_str(&format!(
            r#"
            <a href="../../files/nested-0.1.0-py3-none-any.whl" data-upload-time="2024-03-01T00:00:00Z" data-requires-python=">=3.12">nested-0.1.0-py3-none-any.whl</a>
            <a href="../../files/nested-0.1.0.zip" data-upload-time="2024-03-01T00:00:00Z" {sdist_metadata}>nested-0.1.0.zip</a>
            "#,
        ))?;
        let index = Url::from_directory_path(context.temp_dir.child("simple").path())
            .expect("valid index URL");

        write_executor_build_project(&context, requirement, None)?;
        for pyproject in [
            context.temp_dir.child("pyproject.toml"),
            context.temp_dir.child("dep/pyproject.toml"),
        ] {
            let contents = fs_err::read_to_string(pyproject.path())?;
            pyproject.write_str(&contents.replace(
                r#"requires-python = ">=3.12""#,
                r#"requires-python = ">=3.12,<4""#,
            ))?;
        }
        context
            .lock()
            .arg("--index")
            .arg(index.as_str())
            .arg("--preview-features")
            .arg("lock-build-dependencies")
            .assert()
            .success();
    }

    Ok(())
}

/// Verify that a source artifact selected for a foreign target fails clearly before its backend
/// hook is run when the source is ineligible, while a disallowed yanked source is pruned.
#[tokio::test]
async fn lock_build_dependencies_reject_ineligible_selected_sdist_hooks() -> Result<()> {
    for (sdist_metadata, requirement, expected_cause) in [
        (r#"data-yanked="bad source""#, r#""nested>=0.1.0""#, None),
        (
            r#"data-requires-python=">=3.13""#,
            r#""nested==0.1.0""#,
            Some("source distribution requires Python `>=3.13`"),
        ),
    ] {
        let context = uv_test::test_context!("3.12");
        let simple = context.temp_dir.child("simple/nested");
        simple.create_dir_all()?;
        let files = context.temp_dir.child("files");
        files.create_dir_all()?;

        write_wheel(
            &files.child("nested-0.1.0-py3-none-any.whl"),
            "nested",
            "0.1.0",
        )?;
        let source_dist = files.child("nested-0.1.0.zip");
        let mut zip = ZipFileWriter::new(Vec::new());
        let entry = ZipEntryBuilder::new("nested-0.1.0/pyproject.toml".into(), Compression::Stored);
        block_on(zip.write_entry_whole(
            entry,
            br#"
            [project]
            name = "nested"
            version = "0.1.0"
            requires-python = ">=3.12"

            [build-system]
            requires = []
            backend-path = ["."]
            build-backend = "build_backend"
            "#,
        ))?;
        let entry =
            ZipEntryBuilder::new("nested-0.1.0/build_backend.py".into(), Compression::Stored);
        block_on(zip.write_entry_whole(
            entry,
            br#"
def get_requires_for_build_wheel(config_settings=None):
    raise RuntimeError("ineligible selected source distribution hook was called")
"#,
        ))?;
        fs_err::write(source_dist.path(), block_on(zip.close())?)?;

        simple.child("index.html").write_str(&format!(
            r#"
            <a href="../../files/nested-0.1.0-py3-none-any.whl" data-upload-time="2024-03-01T00:00:00Z" data-requires-python=">=3.12,!=3.13.*">nested-0.1.0-py3-none-any.whl</a>
            <a href="../../files/nested-0.1.0.zip" data-upload-time="2024-03-01T00:00:00Z" {sdist_metadata}>nested-0.1.0.zip</a>
            "#,
        ))?;
        let index = Url::from_directory_path(context.temp_dir.child("simple").path())
            .expect("valid index URL");

        write_executor_build_project(&context, requirement, None)?;
        for pyproject in [
            context.temp_dir.child("pyproject.toml"),
            context.temp_dir.child("dep/pyproject.toml"),
        ] {
            let contents = fs_err::read_to_string(pyproject.path())?;
            pyproject.write_str(&contents.replace(
                r#"requires-python = ">=3.12""#,
                r#"requires-python = ">=3.12,<3.14""#,
            ))?;
        }
        let output = context
            .lock()
            .arg("--index")
            .arg(index.as_str())
            .arg("--preview-features")
            .arg("lock-build-dependencies")
            .output()?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        if let Some(expected_cause) = expected_cause {
            assert!(!output.status.success(), "{stderr}");
            assert!(
                stderr.contains("Cannot lock build dependencies for `nested==0.1.0`"),
                "{stderr}"
            );
            assert!(stderr.contains(expected_cause), "{stderr}");
        } else {
            assert!(output.status.success(), "{stderr}");
        }
        assert!(
            !stderr.contains("ineligible selected source distribution hook was called"),
            "{stderr}"
        );
    }

    Ok(())
}

/// Verify that a same-version yanked runtime source is omitted even when an unyanked wheel
/// leaves another supported Python target uncovered.
#[tokio::test]
async fn lock_build_dependencies_omit_yanked_runtime_sdist() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let simple = context.temp_dir.child("simple/dep");
    simple.create_dir_all()?;
    let files = context.temp_dir.child("files");
    files.create_dir_all()?;

    write_wheel(&files.child("dep-0.1.0-py3-none-any.whl"), "dep", "0.1.0")?;
    let source_dist = files.child("dep-0.1.0.zip");
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
    raise RuntimeError("yanked runtime source distribution hook was called")
"#,
    ))?;
    fs_err::write(source_dist.path(), block_on(zip.close())?)?;

    simple.child("index.html").write_str(
        r#"
        <a href="../../files/dep-0.1.0-py3-none-any.whl" data-upload-time="2024-03-01T00:00:00Z" data-requires-python=">=3.12,!=3.13.*">dep-0.1.0-py3-none-any.whl</a>
        <a href="../../files/dep-0.1.0.zip" data-upload-time="2024-03-01T00:00:00Z" data-yanked="bad source">dep-0.1.0.zip</a>
        "#,
    )?;
    let index =
        Url::from_directory_path(context.temp_dir.child("simple").path()).expect("valid index URL");

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12,<3.14"
        dependencies = ["dep>=0.1.0"]
        "#,
    )?;
    let output = context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index")
        .arg(index.as_str())
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .output()?;
    assert!(output.status.success(), "{output:?}");

    let lock = context.read("uv.lock");
    let dep = package_section(&lock, "dep");
    assert!(dep.contains("dep-0.1.0-py3-none-any.whl"), "{dep}");
    assert!(!dep.contains("dep-0.1.0.zip"), "{dep}");

    let output = context
        .sync()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index")
        .arg(index.as_str())
        .arg("--no-binary-package")
        .arg("dep")
        .arg("--frozen")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .output()?;
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("can't be installed because it is marked as `--no-binary` but has no source distribution"),
        "{stderr}"
    );
    assert!(
        !stderr.contains("yanked runtime source distribution hook was called"),
        "{stderr}"
    );

    Ok(())
}

/// Verify frozen runtime selection rejects a Python-ineligible source distribution before its
/// backend runs when `--no-binary-package` disables the only compatible wheel.
#[test]
fn lock_build_dependencies_reject_python_incompatible_runtime_sdist() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let simple = context.temp_dir.child("simple/helper");
    simple.create_dir_all()?;
    let files = context.temp_dir.child("files");
    files.create_dir_all()?;

    let wheel = "helper-1.0.0-py3-none-any.whl";
    let source = "helper-1.0.0.zip";
    write_wheel(&files.child(wheel), "helper", "1.0.0")?;

    let mut zip = ZipFileWriter::new(Vec::new());
    let entry = ZipEntryBuilder::new("helper-1.0.0/pyproject.toml".into(), Compression::Stored);
    block_on(zip.write_entry_whole(
        entry,
        br#"
        [project]
        name = "helper"
        version = "1.0.0"
        requires-python = ">=3.13,<3.14"

        [build-system]
        requires = []
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
    ))?;
    let entry = ZipEntryBuilder::new("helper-1.0.0/build_backend.py".into(), Compression::Stored);
    block_on(zip.write_entry_whole(
        entry,
        br#"
def get_requires_for_build_wheel(config_settings=None):
    raise RuntimeError("Python-incompatible runtime source hook was called")
"#,
    ))?;
    fs_err::write(files.child(source).path(), block_on(zip.close())?)?;

    simple.child("index.html").write_str(&format!(
        r#"
        <a href="../../files/{wheel}" data-upload-time="2024-03-01T00:00:00Z" data-requires-python=">=3.12,<3.13">{wheel}</a>
        <a href="../../files/{source}" data-upload-time="2024-03-01T00:00:00Z" data-requires-python=">=3.13,<3.14">{source}</a>
        "#
    ))?;
    let index =
        Url::from_directory_path(context.temp_dir.child("simple").path()).expect("valid index URL");

    // Keep one local source build so the generated lock uses the build-aware schema while the
    // runtime helper itself remains fully served by its compatible wheel.
    write_executor_build_project(&context, "", None)?;
    for pyproject in [
        context.temp_dir.child("pyproject.toml"),
        context.temp_dir.child("dep/pyproject.toml"),
    ] {
        let contents = fs_err::read_to_string(pyproject.path())?;
        let contents = contents.replace(
            r#"requires-python = ">=3.12""#,
            r#"requires-python = ">=3.12,<3.13""#,
        );
        pyproject.write_str(&contents)?;
    }
    let project = context.temp_dir.child("pyproject.toml");
    let contents = fs_err::read_to_string(project.path())?;
    project.write_str(&contents.replace(
        r#"dependencies = ["dep"]"#,
        r#"dependencies = ["dep", "helper==1.0.0"]"#,
    ))?;

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
    let helper = package_section(&lock, "helper");
    assert!(helper.contains(wheel), "{helper}");
    assert!(helper.contains(source), "{helper}");
    assert!(helper.contains(">=3.13, <3.14"), "{helper}");

    let output = context
        .sync()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index")
        .arg(index.as_str())
        .arg("--no-binary-package")
        .arg("helper")
        .arg("--no-cache")
        .arg("--frozen")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .output()?;
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("helper==1.0.0"), "{stderr}");
    assert!(
        !stderr.contains("Python-incompatible runtime source hook was called"),
        "{stderr}"
    );

    Ok(())
}

/// Verify that a disallowed yanked wheel for another Python target cannot hide an eligible
/// runtime source distribution whose backend hook requirements must be locked.
#[tokio::test]
async fn lock_build_dependencies_capture_yanked_runtime_sdist_hooks() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let simple = context.temp_dir.child("simple");
    let dep = simple.child("dep");
    dep.create_dir_all()?;
    let helper = simple.child("helper");
    helper.create_dir_all()?;
    let files = context.temp_dir.child("files");
    files.create_dir_all()?;

    write_wheel(&files.child("dep-0.1.0-py3-none-any.whl"), "dep", "0.1.0")?;
    write_wheel_with_requires_and_tag(
        &files.child("dep-0.1.0-cp313-none-any.whl"),
        "dep",
        "0.1.0",
        &[],
        "cp313-none-any",
    )?;
    write_build_source(
        &files.child("dep-0.1.0.zip"),
        "dep",
        "0.1.0",
        Some(("helper==0.1.0", "helper")),
    )?;
    write_wheel(
        &files.child("helper-0.1.0-py3-none-any.whl"),
        "helper",
        "0.1.0",
    )?;

    dep.child("index.html").write_str(
        r#"
        <a href="../../files/dep-0.1.0-py3-none-any.whl" data-upload-time="2024-03-01T00:00:00Z" data-requires-python=">=3.12,!=3.13.*">dep-0.1.0-py3-none-any.whl</a>
        <a href="../../files/dep-0.1.0-cp313-none-any.whl" data-upload-time="2024-03-01T00:00:00Z" data-requires-python=">=3.13,<3.14" data-yanked="bad build">dep-0.1.0-cp313-none-any.whl</a>
        <a href="../../files/dep-0.1.0.zip" data-upload-time="2024-03-01T00:00:00Z" data-requires-python=">=3.12">dep-0.1.0.zip</a>
        "#,
    )?;
    helper.child("index.html").write_str(
        r#"<a href="../../files/helper-0.1.0-py3-none-any.whl" data-upload-time="2024-03-01T00:00:00Z">helper-0.1.0-py3-none-any.whl</a>"#,
    )?;
    let index = Url::from_directory_path(simple.path()).expect("valid index URL");

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12,<3.14"
        dependencies = ["dep>=0.1.0"]
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
    let dep = package_section(&lock, "dep");
    assert!(dep.contains("dep-0.1.0-py3-none-any.whl"), "{dep}");
    assert!(!dep.contains("dep-0.1.0-cp313-none-any.whl"), "{dep}");
    assert!(dep.contains("dep-0.1.0.zip"), "{dep}");
    assert!(
        dep.contains(r#"{ name = "helper", version = "0.1.0" }"#),
        "{dep}"
    );

    Ok(())
}

/// Verify that a yanked foreign-target wheel cannot hide the hook requirements of an eligible
/// runtime source distribution.
#[tokio::test]
async fn lock_build_dependencies_capture_yanked_runtime_wheel_sdist_hooks() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let simple = context.temp_dir.child("simple");
    let dep = simple.child("dep");
    dep.create_dir_all()?;
    let helper = simple.child("helper");
    helper.create_dir_all()?;
    let files = context.temp_dir.child("files");
    files.create_dir_all()?;

    write_wheel(&files.child("dep-0.1.0-1-py3-none-any.whl"), "dep", "0.1.0")?;
    write_wheel(&files.child("dep-0.1.0-2-py3-none-any.whl"), "dep", "0.1.0")?;
    write_build_source(
        &files.child("dep-0.1.0.zip"),
        "dep",
        "0.1.0",
        Some(("helper==0.1.0", "helper")),
    )?;
    write_wheel(
        &files.child("helper-0.1.0-py3-none-any.whl"),
        "helper",
        "0.1.0",
    )?;

    dep.child("index.html").write_str(
        r#"
        <a href="../../files/dep-0.1.0-1-py3-none-any.whl" data-upload-time="2024-03-01T00:00:00Z" data-requires-python=">=3.12,<3.13">dep-0.1.0-1-py3-none-any.whl</a>
        <a href="../../files/dep-0.1.0-2-py3-none-any.whl" data-upload-time="2024-03-01T00:00:00Z" data-requires-python=">=3.13,<3.14" data-yanked="bad build">dep-0.1.0-2-py3-none-any.whl</a>
        <a href="../../files/dep-0.1.0.zip" data-upload-time="2024-03-01T00:00:00Z" data-requires-python=">=3.12">dep-0.1.0.zip</a>
        "#,
    )?;
    helper.child("index.html").write_str(
        r#"<a href="../../files/helper-0.1.0-py3-none-any.whl" data-upload-time="2024-03-01T00:00:00Z">helper-0.1.0-py3-none-any.whl</a>"#,
    )?;
    let index = Url::from_directory_path(simple.path()).expect("valid index URL");

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12,<3.14"
        dependencies = ["dep>=0.1.0"]
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
    let dep = package_section(&lock, "dep");
    assert!(
        dep.contains(r#"{ name = "helper", version = "0.1.0" }"#),
        "{dep}"
    );

    Ok(())
}

/// Verify that a yanked wheel allowed by an exact build requirement cannot leak into the
/// unscoped runtime package, where the same version is selected by an unpinned requirement.
#[tokio::test]
async fn lock_build_dependencies_scope_yanked_build_wheel_from_runtime() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let simple = context.temp_dir.child("simple");
    let dep = simple.child("dep");
    dep.create_dir_all()?;
    let helper = simple.child("helper");
    helper.create_dir_all()?;
    let files = context.temp_dir.child("files");
    files.create_dir_all()?;

    let runtime_wheel = "dep-0.1.0-1-py3-none-any.whl";
    let yanked_wheel = "dep-0.1.0-2-py3-none-any.whl";
    let mut zip = ZipFileWriter::new(Vec::new());
    let entry = ZipEntryBuilder::new("dep.py".into(), Compression::Stored);
    block_on(zip.write_entry_whole(entry, b"VALUE = 'runtime'\n"))?;
    let entry = ZipEntryBuilder::new("dep-0.1.0.dist-info/METADATA".into(), Compression::Stored);
    block_on(zip.write_entry_whole(entry, b"Metadata-Version: 2.3\nName: dep\nVersion: 0.1.0\n"))?;
    let entry = ZipEntryBuilder::new("dep-0.1.0.dist-info/WHEEL".into(), Compression::Stored);
    block_on(zip.write_entry_whole(
        entry,
        b"Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
    ))?;
    let entry = ZipEntryBuilder::new("dep-0.1.0.dist-info/RECORD".into(), Compression::Stored);
    block_on(zip.write_entry_whole(entry, b""))?;
    fs_err::write(files.child(runtime_wheel).path(), block_on(zip.close())?)?;

    let mut zip = ZipFileWriter::new(Vec::new());
    let entry = ZipEntryBuilder::new("dep.py".into(), Compression::Stored);
    block_on(zip.write_entry_whole(entry, b"VALUE = 'build'\n"))?;
    let entry = ZipEntryBuilder::new("dep-0.1.0.dist-info/METADATA".into(), Compression::Stored);
    block_on(zip.write_entry_whole(entry, b"Metadata-Version: 2.3\nName: dep\nVersion: 0.1.0\n"))?;
    let entry = ZipEntryBuilder::new("dep-0.1.0.dist-info/WHEEL".into(), Compression::Stored);
    block_on(zip.write_entry_whole(
        entry,
        b"Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
    ))?;
    let entry = ZipEntryBuilder::new("dep-0.1.0.dist-info/RECORD".into(), Compression::Stored);
    block_on(zip.write_entry_whole(entry, b""))?;
    fs_err::write(files.child(yanked_wheel).path(), block_on(zip.close())?)?;

    write_build_source(
        &files.child("dep-0.1.0.zip"),
        "dep",
        "0.1.0",
        Some(("helper==0.1.0", "helper")),
    )?;
    write_wheel(
        &files.child("helper-0.1.0-py3-none-any.whl"),
        "helper",
        "0.1.0",
    )?;

    dep.child("index.html").write_str(&format!(
        r#"
        <a href="../../files/{runtime_wheel}" data-upload-time="2024-03-01T00:00:00Z" data-requires-python=">=3.12,<3.13">{runtime_wheel}</a>
        <a href="../../files/{yanked_wheel}" data-upload-time="2024-03-01T00:00:00Z" data-requires-python=">=3.12,<3.14" data-yanked="allowed exact build pin">{yanked_wheel}</a>
        <a href="../../files/dep-0.1.0.zip" data-upload-time="2024-03-01T00:00:00Z" data-requires-python=">=3.12">dep-0.1.0.zip</a>
        "#
    ))?;
    helper.child("index.html").write_str(
        r#"<a href="../../files/helper-0.1.0-py3-none-any.whl" data-upload-time="2024-03-01T00:00:00Z">helper-0.1.0-py3-none-any.whl</a>"#,
    )?;
    let index = Url::from_directory_path(simple.path()).expect("valid index URL");

    let builder = context.temp_dir.child("builder");
    builder.create_dir_all()?;
    builder.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "builder"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["dep==0.1.0"]
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
    )?;
    builder.child("build_backend.py").write_str(
        r#"
import dep
from pathlib import Path
from zipfile import ZipFile

def get_requires_for_build_wheel(config_settings=None):
    return []

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    if dep.VALUE != "build":
        raise RuntimeError(f"selected runtime wheel for exact build pin: {dep.VALUE}")
    filename = "builder-0.1.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("builder.py", "")
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

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12,<3.14"
        dependencies = ["builder", "dep>=0.1.0"]

        [tool.uv.sources]
        builder = { path = "builder" }
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
    let dep_packages = lock
        .split("\n[[package]]")
        .skip(1)
        .filter(|package| package.contains("\nname = \"dep\""))
        .collect::<Vec<_>>();
    let runtime = dep_packages
        .iter()
        .find(|package| !package.contains("\nresolution-id = "))
        .expect("unscoped runtime dep package");
    assert!(runtime.contains(runtime_wheel), "{runtime}");
    assert!(!runtime.contains(yanked_wheel), "{runtime}");
    assert!(runtime.contains("dep-0.1.0.zip"), "{runtime}");
    assert!(
        runtime.contains(r#"{ name = "helper", version = "0.1.0" }"#),
        "{runtime}"
    );
    let scoped = dep_packages
        .iter()
        .find(|package| package.contains("\nresolution-id = "))
        .expect("build-scoped dep package");
    assert!(scoped.contains(yanked_wheel), "{scoped}");

    context
        .sync()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--index")
        .arg(index.as_str())
        .arg("--no-cache")
        .arg("--frozen")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();
    context
        .run()
        .arg("--no-sync")
        .arg("python")
        .arg("-c")
        .arg("import dep; assert dep.VALUE == 'runtime', dep.VALUE")
        .assert()
        .success();

    Ok(())
}

/// Verify that forcing a nested registry source build captures its hook requirements even when a
/// compatible wheel is available, and that the nested source can be replayed while frozen.
#[test]
fn lock_build_dependencies_replay_no_binary_nested_source_hooks() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let nested = context.temp_dir.child("simple/nested");
    nested.create_dir_all()?;
    let helper = context.temp_dir.child("simple/helper");
    helper.create_dir_all()?;
    let files = context.temp_dir.child("files");
    files.create_dir_all()?;

    write_wheel(
        &files.child("nested-0.1.0-py3-none-any.whl"),
        "nested",
        "0.1.0",
    )?;
    write_build_source(
        &files.child("nested-0.1.0.zip"),
        "nested",
        "0.1.0",
        Some(("helper==1.0.0", "helper")),
    )?;
    write_wheel(
        &files.child("helper-1.0.0-py3-none-any.whl"),
        "helper",
        "1.0.0",
    )?;

    nested.child("index.html").write_str(
        r#"
        <a href="../../files/nested-0.1.0-py3-none-any.whl" data-upload-time="2024-03-01T00:00:00Z" data-requires-python=">=3.12">nested-0.1.0-py3-none-any.whl</a>
        <a href="../../files/nested-0.1.0.zip" data-upload-time="2024-03-01T00:00:00Z" data-requires-python=">=3.12">nested-0.1.0.zip</a>
        "#,
    )?;
    helper.child("index.html").write_str(
        r#"
        <a href="../../files/helper-1.0.0-py3-none-any.whl" data-upload-time="2024-03-01T00:00:00Z" data-requires-python=">=3.12">helper-1.0.0-py3-none-any.whl</a>
        "#,
    )?;
    let index =
        Url::from_directory_path(context.temp_dir.child("simple").path()).expect("valid index URL");

    write_executor_build_project(&context, r#""nested==0.1.0""#, Some("nested"))?;
    context
        .lock()
        .arg("--index")
        .arg(index.as_str())
        .arg("--no-binary-package")
        .arg("nested")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();

    let lock = context.read("uv.lock");
    let nested = package_section(&lock, "nested");
    assert!(nested.contains("nested-0.1.0.zip"), "{nested}");
    let resolutions = resolution_sections(&lock);
    let nested_resolution = resolutions
        .split("\n\n")
        .find(|resolution| resolution.contains("\nname = \"nested\"\n"))
        .expect("locked nested resolution");
    assert!(
        nested_resolution.contains(r#"{ name = "helper", version = "1.0.0""#),
        "{nested_resolution}"
    );

    context
        .sync()
        .arg("--no-index")
        .arg("--no-cache")
        .arg("--no-binary-package")
        .arg("nested")
        .arg("--frozen")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();

    Ok(())
}

/// Verify that a retained wheel forbidden by `--no-binary-package` cannot hide a source
/// distribution that is ineligible for the active build executor.
#[tokio::test]
async fn lock_build_dependencies_reject_no_binary_ineligible_sdist_hooks() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let simple = context.temp_dir.child("simple/nested");
    simple.create_dir_all()?;
    let files = context.temp_dir.child("files");
    files.create_dir_all()?;

    write_wheel(
        &files.child("nested-0.1.0-py3-none-any.whl"),
        "nested",
        "0.1.0",
    )?;
    let source_dist = files.child("nested-0.1.0.zip");
    let mut zip = ZipFileWriter::new(Vec::new());
    let entry = ZipEntryBuilder::new("nested-0.1.0/pyproject.toml".into(), Compression::Stored);
    block_on(zip.write_entry_whole(
        entry,
        br#"
        [project]
        name = "nested"
        version = "0.1.0"
        requires-python = ">=3.12"

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
def get_requires_for_build_wheel(config_settings=None):
    raise RuntimeError("ineligible no-binary source distribution hook was called")
"#,
    ))?;
    fs_err::write(source_dist.path(), block_on(zip.close())?)?;

    simple.child("index.html").write_str(
        r#"
        <a href="../../files/nested-0.1.0-py3-none-any.whl" data-upload-time="2024-03-01T00:00:00Z" data-requires-python=">=3.12,!=3.13.*">nested-0.1.0-py3-none-any.whl</a>
        <a href="../../files/nested-0.1.0.zip" data-upload-time="2024-03-01T00:00:00Z" data-requires-python=">=3.13">nested-0.1.0.zip</a>
        "#,
    )?;
    let index =
        Url::from_directory_path(context.temp_dir.child("simple").path()).expect("valid index URL");

    write_executor_build_project(&context, r#""nested==0.1.0""#, None)?;
    let output = context
        .lock()
        .arg("--index")
        .arg(index.as_str())
        .arg("--no-binary-package")
        .arg("nested")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .output()?;
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("No solution found when resolving: `nested==0.1.0`"),
        "{stderr}"
    );
    assert!(stderr.contains("does not satisfy Python>=3.13"), "{stderr}");
    assert!(
        !stderr.contains("ineligible no-binary source distribution hook was called"),
        "{stderr}"
    );

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

/// Verify that a platform-specific wheel covering the only supported environment makes the
/// registry source distribution unreachable.
#[tokio::test]
async fn lock_build_dependencies_skip_unreachable_supported_platform_sdist_hooks() -> Result<()> {
    assert_supported_wheel_skips_registry_sdist_hook(
        "py3-none-win_amd64",
        ">=3.12,<4",
        "sys_platform == 'win32' and platform_machine == 'AMD64'",
        true,
    )
    .await
}

/// Verify that an implementation-specific wheel covering the only supported Python environment
/// makes the registry source distribution unreachable.
#[tokio::test]
async fn lock_build_dependencies_skip_unreachable_supported_python_sdist_hooks() -> Result<()> {
    assert_supported_wheel_skips_registry_sdist_hook(
        "cp312-none-any",
        ">=3.12,<4",
        "python_full_version == '3.12.*' and platform_python_implementation == 'CPython'",
        true,
    )
    .await
}

/// Verify that versioned platform wheels do not hide a potentially reachable registry sdist.
#[tokio::test]
async fn lock_build_dependencies_capture_versioned_platform_sdist_hooks() -> Result<()> {
    assert_supported_wheel_skips_registry_sdist_hook(
        "py3-none-manylinux_2_17_x86_64",
        ">=3.12,<4",
        "sys_platform == 'linux' and platform_machine == 'x86_64'",
        false,
    )
    .await?;
    assert_supported_wheel_skips_registry_sdist_hook(
        "py3-none-macosx_11_0_arm64",
        ">=3.12,<4",
        "sys_platform == 'darwin' and platform_machine == 'arm64'",
        false,
    )
    .await
}

async fn assert_supported_wheel_skips_registry_sdist_hook(
    wheel_tag: &str,
    requires_python: &str,
    supported_environment: &str,
    should_skip: bool,
) -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let artifacts = context.temp_dir.child("artifacts");
    artifacts.create_dir_all()?;
    let wheel_filename = format!("dep-0.1.0-{wheel_tag}.whl");
    write_wheel_with_requires_and_tag(
        &artifacts.child(&wheel_filename),
        "dep",
        "0.1.0",
        &[],
        wheel_tag,
    )?;

    let source_dist = artifacts.child("dep-0.1.0.zip");
    let mut zip = ZipFileWriter::new(Vec::new());
    let entry = ZipEntryBuilder::new("dep-0.1.0/pyproject.toml".into(), Compression::Stored);
    block_on(
        zip.write_entry_whole(
            entry,
            format!(
                r#"
        [project]
        name = "dep"
        dynamic = ["version"]
        requires-python = "{requires_python}"

        [build-system]
        requires = []
        backend-path = ["."]
        build-backend = "build_backend"
        "#
            )
            .as_bytes(),
        ),
    )?;
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
                <a href="{}/files/{wheel_filename}" data-upload-time="2024-03-01T00:00:00Z">{wheel_filename}</a>
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
        .and(path(format!("/files/{wheel_filename}")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(fs_err::read(artifacts.child(&wheel_filename).path())?),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/files/dep-0.1.0.zip"))
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
        requires-python = "{requires_python}"
        dependencies = ["dep==0.1.0"]

        [tool.uv]
        environments = ["{supported_environment}"]
        "#
        ))?;

    if !should_skip {
        let output = context
            .lock()
            .arg("--index-url")
            .arg(index_url)
            .arg("--preview-features")
            .arg("lock-build-dependencies")
            .output()?;
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains("unreachable source distribution hook was called")
        );
        return Ok(());
    }

    let mut filters = context.filters();
    filters.push((r"(?m)^WARN Range requests not supported[^\n]*\n", ""));
    insta::allow_duplicates! {
        uv_snapshot!(filters, context
            .lock()
            .arg("--index-url")
            .arg(index_url)
            .arg("--preview-features")
            .arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");
    }

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

/// Verify that nested source-build resolution IDs from a relative local index do not depend on
/// the absolute workspace path.
#[test]
fn lock_build_dependencies_nested_local_registry_resolution_ids_are_portable() -> Result<()> {
    let first = uv_test::test_context!("3.12");
    let second = uv_test::test_context!("3.12");

    for context in [&first, &second] {
        let links = context.temp_dir.child("links");
        links.create_dir_all()?;
        write_build_source(
            &links.child("nested-0.1.0.zip"),
            "nested",
            "0.1.0",
            Some(("helper==0.1.0", "helper")),
        )?;
        write_wheel(
            &links.child("helper-0.1.0-py3-none-any.whl"),
            "helper",
            "0.1.0",
        )?;
        write_executor_build_project(context, r#""nested==0.1.0""#, None)?;

        context
            .lock()
            .arg("--find-links")
            .arg("links")
            .arg("--no-index")
            .arg("--preview-features")
            .arg("lock-build-dependencies")
            .assert()
            .success();
    }

    let first_resolutions = resolution_sections(&first.read("uv.lock"));
    let second_resolutions = resolution_sections(&second.read("uv.lock"));
    assert_eq!(first_resolutions, second_resolutions);

    Ok(())
}

/// Verify that a Python-restricted universal wheel does not hide the hook requirements of a
/// fallback source distribution inside a locked build environment.
#[tokio::test]
async fn lock_build_dependencies_capture_requires_python_nested_sdist_hooks() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let simple = context.temp_dir.child("simple");
    let nested = simple.child("nested");
    nested.create_dir_all()?;
    let helper = simple.child("helper");
    helper.create_dir_all()?;
    let files = context.temp_dir.child("files");
    files.create_dir_all()?;

    write_wheel(
        &files.child("nested-0.1.0-py3-none-any.whl"),
        "nested",
        "0.1.0",
    )?;
    write_build_source(
        &files.child("nested-0.1.0.zip"),
        "nested",
        "0.1.0",
        Some(("helper==0.1.0", "helper")),
    )?;
    write_wheel(
        &files.child("helper-0.1.0-py3-none-any.whl"),
        "helper",
        "0.1.0",
    )?;

    nested.child("index.html").write_str(
        r#"
        <a href="../../files/nested-0.1.0-py3-none-any.whl" data-upload-time="2024-03-01T00:00:00Z" data-requires-python=">=3.12,!=3.13.*">nested-0.1.0-py3-none-any.whl</a>
        <a href="../../files/nested-0.1.0.zip" data-upload-time="2024-03-01T00:00:00Z" data-requires-python=">=3.12">nested-0.1.0.zip</a>
        "#,
    )?;
    helper.child("index.html").write_str(
        r#"<a href="../../files/helper-0.1.0-py3-none-any.whl" data-upload-time="2024-03-01T00:00:00Z">helper-0.1.0-py3-none-any.whl</a>"#,
    )?;
    let index = Url::from_directory_path(simple.path()).expect("valid index URL");

    write_executor_build_project(&context, r#""nested==0.1.0""#, None)?;

    context
        .lock()
        .arg("--index")
        .arg(index.as_str())
        .arg("--preview-features")
        .arg("lock-build-dependencies")
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

/// Verify that a yanked universal wheel that is not allowed by the build requirement does not
/// hide the hook requirements of an eligible source distribution in a locked build environment.
#[tokio::test]
async fn lock_build_dependencies_capture_yanked_nested_sdist_hooks() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let simple = context.temp_dir.child("simple");
    let nested = simple.child("nested");
    nested.create_dir_all()?;
    let helper = simple.child("helper");
    helper.create_dir_all()?;
    let files = context.temp_dir.child("files");
    files.create_dir_all()?;

    write_wheel(
        &files.child("nested-0.1.0-py3-none-any.whl"),
        "nested",
        "0.1.0",
    )?;
    write_build_source(
        &files.child("nested-0.1.0.zip"),
        "nested",
        "0.1.0",
        Some(("helper==0.1.0", "helper")),
    )?;
    write_wheel(
        &files.child("helper-0.1.0-py3-none-any.whl"),
        "helper",
        "0.1.0",
    )?;

    nested.child("index.html").write_str(
        r#"
        <a href="../../files/nested-0.1.0-py3-none-any.whl" data-upload-time="2024-03-01T00:00:00Z" data-yanked="bad build">nested-0.1.0-py3-none-any.whl</a>
        <a href="../../files/nested-0.1.0.zip" data-upload-time="2024-03-01T00:00:00Z" data-requires-python=">=3.12">nested-0.1.0.zip</a>
        "#,
    )?;
    helper.child("index.html").write_str(
        r#"<a href="../../files/helper-0.1.0-py3-none-any.whl" data-upload-time="2024-03-01T00:00:00Z">helper-0.1.0-py3-none-any.whl</a>"#,
    )?;
    let index = Url::from_directory_path(simple.path()).expect("valid index URL");

    write_executor_build_project(&context, r#""nested>=0.1.0""#, None)?;

    context
        .lock()
        .arg("--index")
        .arg(index.as_str())
        .arg("--preview-features")
        .arg("lock-build-dependencies")
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

/// Verify that an excluded universal wheel does not hide the hook requirements of a selected
/// runtime source distribution.
#[tokio::test]
async fn lock_build_dependencies_capture_excluded_runtime_sdist_hooks() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let artifacts = context.temp_dir.child("artifacts");
    artifacts.create_dir_all()?;
    write_wheel(
        &artifacts.child("dep-0.1.0-py3-none-any.whl"),
        "dep",
        "0.1.0",
    )?;
    write_wheel(
        &artifacts.child("helper-0.1.0-py3-none-any.whl"),
        "helper",
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
        version = "0.1.0"
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
    return ["helper==0.1.0"]
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
                <a href="{}/files/dep-0.1.0-py3-none-any.whl" data-upload-time="2024-04-01T00:00:00Z">dep-0.1.0-py3-none-any.whl</a>
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
    Mock::given(method("GET"))
        .and(path("/files/helper-0.1.0-py3-none-any.whl"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(fs_err::read(
            artifacts.child("helper-0.1.0-py3-none-any.whl").path(),
        )?))
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
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--index-url")
        .arg(index_url)
        .arg("--exclude-newer")
        .arg("2024-03-25T00:00:00Z")
        .assert()
        .success();

    let lock = context.read("uv.lock");
    let dep = package_section(&lock, "dep");
    assert!(
        dep.contains(r#"{ name = "helper", version = "0.1.0" }"#),
        "{dep}"
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

    context
        .sync()
        .arg("--no-cache")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--frozen")
        .assert()
        .success();

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
        .arg("--no-cache")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--frozen"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to build `dep @ file://[TEMP_DIR]/dep`
      ├─▶ Failed to install requirements from `build-system.requires`
      ├─▶ Failed to build `builder @ file://[TEMP_DIR]/builder`
      ├─▶ Failed to resolve requirements from `build-system.requires`
      ╰─▶ The initial build requirements for `builder` do not match the locked bootstrap environment

    hint: `dep` was included because `project` (v0.1.0) depends on `dep`
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
