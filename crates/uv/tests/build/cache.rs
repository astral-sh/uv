use std::fmt::Write;
use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result};
use assert_fs::prelude::*;
use async_zip::base::write::ZipFileWriter;
use async_zip::{Compression, ZipEntryBuilder};
use futures::executor::block_on;
use insta::{allow_duplicates, assert_snapshot};
use predicates::prelude::predicate;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

use uv_cache::CacheBucket;
use uv_fs::PortablePath;
#[cfg(unix)]
use uv_fs::create_symlink;
use uv_test::{TestContext, get_bin, uv_snapshot};

/// A custom cache directory must configure commands and snapshot filters together.
#[test]
fn cache_dir_uses_configured_test_context_path() {
    let context = uv_test::test_context!("3.12").with_cache_dir("project/cache");

    assert_eq!(
        context.cache_dir.path(),
        context.temp_dir.child("project").child("cache").path()
    );

    uv_snapshot!(context.filters(), context.command().arg("cache").arg("dir"), @"
    exit_code: 0 (success)
    ----- stdout -----
    [CACHE_DIR]/
    ");
}

/// When the active cache directory is inside an explicit build source, we should warn and continue
/// the build.
#[test]
fn build_warns_cache_inside_source() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_cache_dir("project/.uv-cache");
    let project = context.temp_dir.child("project");

    project.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["uv_build>=0.5.15,<10000"]
        build-backend = "uv_build"
        "#,
    )?;
    project.child("src/project/__init__.py").touch()?;

    uv_snapshot!(context.filters(), context.build().arg("--sdist").arg("project"), @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: The cache directory `project/.uv-cache` is inside the build source directory `project` and may be included in distributions
    Building source distribution...
    Successfully built project/dist/project-0.1.0.tar.gz
    ");

    project
        .child("dist/project-0.1.0.tar.gz")
        .assert(predicate::path::is_file());

    Ok(())
}

/// When the canonical cache directory is inside an explicit build source, we should warn even if
/// the configured cache path itself is outside the source.
#[test]
#[cfg(unix)]
fn build_warns_symlinked_cache_inside_source() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let project = context.temp_dir.child("project");

    project.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["uv_build>=0.5.15,<10000"]
        build-backend = "uv_build"
        "#,
    )?;
    project.child("src/project/__init__.py").touch()?;

    let cache_dir = project.child(".uv-cache");
    cache_dir.create_dir_all()?;
    let cache_link = context.temp_dir.child("cache-link");
    create_symlink(cache_dir.path(), cache_link.path())?;

    let mut command = Command::new(get_bin!());
    command
        .arg("build")
        .arg("--sdist")
        .arg("project")
        .arg("--cache-dir")
        .arg(cache_link.path());
    context.add_shared_env(&mut command, false);
    command.current_dir(context.temp_dir.path());

    uv_snapshot!(context.filters(), command, @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: The cache directory `cache-link` is inside the build source directory `project` and may be included in distributions
    Building source distribution...
    Successfully built project/dist/project-0.1.0.tar.gz
    ");

    project
        .child("dist/project-0.1.0.tar.gz")
        .assert(predicate::path::is_file());

    Ok(())
}

/// A cache in the workspace root is allowed when building a member that does not contain it.
#[test]
fn build_allows_cache_outside_selected_source() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_cache_dir("workspace/.uv-cache");
    let workspace = context.temp_dir.child("workspace");
    let member = workspace.child("member");

    workspace.child("pyproject.toml").write_str(
        r#"
        [tool.uv.workspace]
        members = ["member"]
        "#,
    )?;
    member.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "member"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["uv_build>=0.5.15,<10000"]
        build-backend = "uv_build"
        "#,
    )?;
    member.child("src/member/__init__.py").touch()?;

    uv_snapshot!(context.filters(), context.build()
        .arg("--sdist")
        .arg("--package")
        .arg("member")
        .current_dir(&workspace), @"
    exit_code: 0 (success)
    ----- stderr -----
    Building source distribution...
    Successfully built dist/member-0.1.0.tar.gz
    ");

    workspace
        .child("dist/member-0.1.0.tar.gz")
        .assert(predicate::path::is_file());

    Ok(())
}

/// When the project directory defaults to a current directory inside the cache directory, we should
/// error before using the cache.
#[test]
fn cache_current_dir_inside_cache() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    uv_snapshot!(context.filters(), context.command()
        .arg("cache")
        .arg("dir")
        .current_dir(context.cache_dir.path()), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: The project directory `.` is inside the cache directory `.`
    ");

    let child = context.cache_dir.child("child");
    child.create_dir_all()?;

    uv_snapshot!(context.filters(), context.command()
        .arg("cache")
        .arg("dir")
        .current_dir(child.path()), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: The project directory `.` is inside the cache directory `[CACHE_DIR]/`
    ");

    Ok(())
}

/// When the project directory is inside a symlinked cache directory, we should error before using
/// the cache.
#[test]
#[cfg(unix)]
fn cache_current_dir_inside_symlinked_cache() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let cache_link = context.temp_dir.child("cache-link");
    create_symlink(context.cache_dir.path(), cache_link.path())?;

    let child = context.cache_dir.child("child");
    child.create_dir_all()?;

    let mut command = Command::new(get_bin!());
    command
        .arg("cache")
        .arg("dir")
        .arg("--cache-dir")
        .arg(cache_link.path());
    context.add_shared_env(&mut command, false);
    command.current_dir(child.path());

    uv_snapshot!(context.filters(), command, @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: The project directory `.` is inside the cache directory `[CACHE_DIR]/`
    ");

    Ok(())
}

/// When a workspace is inside the cache directory, we should error before locking the workspace.
#[test]
fn cache_workspace_inside_cache() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let workspace = context.cache_dir.child("workspace");
    workspace.child("pyproject.toml").write_str(
        r#"
        [tool.uv.workspace]
        members = ["member"]
        "#,
    )?;
    workspace
        .child("member")
        .child("pyproject.toml")
        .write_str(
            r#"
        [project]
        name = "member"
        version = "0.1.0"
        requires-python = ">=3.12"
        "#,
        )?;

    uv_snapshot!(context.filters(), context.lock()
        .arg("--project")
        .arg(workspace.path()), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: The project directory `[CACHE_DIR]/workspace` is inside the cache directory `[CACHE_DIR]/`
    ");

    Ok(())
}

/// When the cache directory is a non-canonical parent of the project directory, we should still
/// detect that the project is inside the cache.
#[test]
fn cache_project_inside_relative_parent_cache() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let project = context.temp_dir.child("project");
    project.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        "#,
    )?;

    let mut command = Command::new(get_bin!());
    command.arg("lock").arg("--cache-dir").arg("..");
    context.add_shared_env(&mut command, false);
    command.current_dir(project.path());

    uv_snapshot!(context.filters(), command, @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: The project directory `.` is inside the cache directory `[TEMP_DIR]/`
    ");

    Ok(())
}

/// When `--no-cache` is enabled, running from a project inside the configured cache directory
/// should not trip the persistent cache guard.
#[test]
fn cache_project_inside_cache_no_cache() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let project = context.cache_dir.child("project");
    project.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock().arg("--no-cache").current_dir(&project), @"
    exit_code: 0 (success)
    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 1 package in [TIME]
    ");

    Ok(())
}

/// When the cache directory cannot be created (e.g., due to permissions), we should show a
/// chained error message that indicates we failed to initialize the cache.
#[test]
#[cfg(unix)]
fn cache_init_failure() -> Result<()> {
    use uv_test::ReadOnlyDirectoryGuard;

    let context = uv_test::test_context!("3.12").with_cache_dir("cache_parent/cache");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]
        "#,
    )?;

    // Create a read-only directory that will serve as the parent of the cache.
    // The guard sets it to read-only and restores original permissions on drop (including panic).
    let cache_parent = context.temp_dir.child("cache_parent");
    fs_err::create_dir(&cache_parent)?;
    let _guard = ReadOnlyDirectoryGuard::new(cache_parent.path())?;

    // Running a command should fail with a chained error about cache initialization
    uv_snapshot!(context.filters(), context.sync(), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Failed to initialize cache at `cache_parent/cache`
      Caused by: failed to create directory `[CACHE_DIR]/`: Permission denied (os error 13)
    ");

    Ok(())
}

#[tokio::test]
async fn binary_payloads_stay_in_archive_without_preview() -> Result<()> {
    let server = MockServer::start().await;
    for streaming in [false, true] {
        let context = uv_test::test_context!("3.12")
            .with_filter((r" \(from (?:file|http)://.*\)", " (from [WHEEL_URL])"));
        let wheel = binary_payload_wheel(&context)?;
        let mut command = context.pip_install();
        if streaming {
            Mock::given(method("GET"))
                .and(path("/binary_payload-0.1.0-py3-none-any.whl"))
                .respond_with(ResponseTemplate::new(200).set_body_bytes(fs_err::read(&wheel)?))
                .mount(&server)
                .await;
            command.arg(format!(
                "{}/binary_payload-0.1.0-py3-none-any.whl",
                server.uri()
            ));
        } else {
            command.arg(&wheel);
        }

        allow_duplicates! {
            uv_snapshot!(context.filters(), command, @"
            exit_code: 0 (success)
            ----- stderr -----
            Resolved 1 package in [TIME]
            Prepared 1 package in [TIME]
            Installed 1 package in [TIME]
             + binary-payload==0.1.0 (from [WHEEL_URL])
            ");
        }

        assert!(!context.cache_dir.child("files-v0").exists());
        let archive_files = context.cache_files(CacheBucket::Archive)?;
        let archive_binary = archive_files
            .iter()
            .find(|path| path.ends_with("binary_payload/native.so"))
            .context("binary payload is missing from the archive")?;
        assert_eq!(fs_err::read(archive_binary)?, BINARY_PAYLOAD_CONTENTS);
        assert_eq!(
            fs_err::read(context.site_packages().join("binary_payload/native.so"))?,
            BINARY_PAYLOAD_CONTENTS,
        );
    }
    Ok(())
}

#[tokio::test]
async fn all_files_except_record_use_archive_file_store() -> Result<()> {
    let server = MockServer::start().await;
    for (streaming, concurrent_installs) in [(false, "1"), (false, "4"), (true, "1"), (true, "4")] {
        let context = uv_test::test_context!("3.12")
            .with_concurrent_installs(concurrent_installs)
            .with_filter((r" \(from (?:file|http)://.*\)", " (from [WHEEL_URL])"));
        let wheel = binary_payload_wheel(&context)?;
        let mut command = context.pip_install();
        command.args(["--preview-features", "content-addressed-cache"]);
        if streaming {
            Mock::given(method("GET"))
                .and(path("/binary_payload-0.1.0-py3-none-any.whl"))
                .respond_with(ResponseTemplate::new(200).set_body_bytes(fs_err::read(&wheel)?))
                .mount(&server)
                .await;
            command.arg(format!(
                "{}/binary_payload-0.1.0-py3-none-any.whl",
                server.uri()
            ));
        } else {
            command.arg(&wheel);
        }
        allow_duplicates! {
            uv_snapshot!(context.filters(), command, @"
            exit_code: 0 (success)
            ----- stderr -----
            Resolved 1 package in [TIME]
            Prepared 1 package in [TIME]
            Installed 1 package in [TIME]
             + binary-payload==0.1.0 (from [WHEEL_URL])
            ");
        }

        let objects = context.cache_files(CacheBucket::Files)?;
        // Identical contents still need separate executable and non-executable objects.
        let mut snapshot = String::new();
        for object in &objects {
            writeln!(
                snapshot,
                "{}",
                PortablePath::from(object.strip_prefix(context.cache_dir.path())?)
            )?;
        }
        allow_duplicates! {
            assert_snapshot!(snapshot, @"
            files-v0/0c/0c8d68fa16e023b913e926be9b281c5a133c33291b3e60a54c310376f5602a45
            files-v0/2f/2f4d468b80be8a639ba0bdcfc738be8d912c35dd686a1eb15272e8f56096358c
            files-v0/4a/4a3f63865c29c673794f181932bc0e2c4779275f22e03a896d3d6ca3ac447332
            files-v0/80/8043c55c494befd8bb44cf59c112ae6a944da98d04b550ff4fd055e2ebfabeb8
            files-v0/92/920a0fbc7cd79a94ab2adabfaa8b93804bf6e3e858c454d45958cc9902554248
            files-v0/bf/bf13d7b1c373edcb8588b94aae048664a6684f665fa4a7e8cd814360e537a049
            files-v0/fa/fad1ac6fba02614a6ee120fbb397cd676f48c101afee95ab29d146c76df03596
            ");
        }
        let paths = context.cache_files(CacheBucket::Archive)?;
        let shared_paths = paths
            .iter()
            .filter(|path| {
                objects
                    .iter()
                    .any(|object| uv_fs::is_same_file_allow_missing(path, object) == Some(true))
            })
            .cloned()
            .collect::<Vec<_>>();
        let paths = paths
            .into_iter()
            .filter(|path| !path.ends_with("RECORD"))
            .collect::<Vec<_>>();
        assert_eq!(shared_paths, paths);
    }
    Ok(())
}

#[test]
fn binary_payloads_use_archive_file_store() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_file_counts()
        .with_filtered_sizes_and_units()
        .with_filtered_python_names()
        .with_filtered_virtualenv_bin();
    let wheel = binary_payload_wheel(&context)?;

    uv_snapshot!(context.filters(), context.pip_install()
        .args(["--preview-features", "content-addressed-cache", "--link-mode", "copy"])
        .arg(&wheel), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + binary-payload==0.1.0 (from file://[TEMP_DIR]/binary_payload-0.1.0-py3-none-any.whl)
    ");

    let objects = context.cache_files(CacheBucket::Files)?;
    let archive = fs_err::read_dir(context.cache_dir.child("archive-v0").path())?
        .next()
        .transpose()?
        .context("missing cached archive")?
        .path();
    let archive_binary = archive.join("binary_payload/native.so");
    let object = objects
        .iter()
        .find(|object| uv_fs::is_same_file_allow_missing(&archive_binary, object) == Some(true))
        .context("missing shared binary payload")?;

    uv_snapshot!(context.filters(), context.prune(), @"
    exit_code: 0 (success)
    ----- stderr -----
    Pruning cache at: [CACHE_DIR]/
    No unused entries found
    ");
    assert!(object.exists());

    // The archive remains complete even if its file-store entry is removed.
    fs_err::remove_file(object)?;
    let target = context.temp_dir.child("retained-target");
    uv_snapshot!(context.filters(), context.pip_install()
        .args(["--preview-features", "content-addressed-cache", "--link-mode", "copy"])
        .arg("--target")
        .arg(target.path())
        .arg(&wheel), @"
    exit_code: 0 (success)
    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: .venv/[BIN]/[PYTHON]
    Resolved 1 package in [TIME]
    Installed 1 package in [TIME]
     + binary-payload==0.1.0 (from file://[TEMP_DIR]/binary_payload-0.1.0-py3-none-any.whl)
    ");
    assert_eq!(
        fs_err::read(target.join("binary_payload/native.so"))?,
        BINARY_PAYLOAD_CONTENTS
    );
    fs_err::hard_link(&archive_binary, object)?;

    // Links outside the cache also keep objects alive after their archives are removed.
    let retained_binary = context.temp_dir.child("retained-native.so");
    fs_err::hard_link(object, &retained_binary)?;
    uv_snapshot!(context.filters(), context.clean().arg("binary-payload"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Removed [N] files ([SIZE])
    ");
    assert!(!archive.exists());
    assert_eq!(
        context.cache_files(CacheBucket::Files)?,
        vec![object.clone()]
    );
    assert_eq!(fs_err::read(&retained_binary)?, BINARY_PAYLOAD_CONTENTS);

    fs_err::remove_file(&retained_binary)?;
    uv_snapshot!(context.filters(), context.prune(), @"
    exit_code: 0 (success)
    ----- stderr -----
    Pruning cache at: [CACHE_DIR]/
    Removed [N] files ([SIZE])
    ");
    assert!(context.cache_files(CacheBucket::Files)?.is_empty());

    Ok(())
}

/// Requires `UV_INTERNAL__TEST_ALT_FS`.
#[test]
fn binary_payload_copy_fallback_uses_archive_file_store() -> Result<()> {
    let Some(context) = uv_test::test_context!("3.12")
        .with_filtered_python_names()
        .with_filtered_virtualenv_bin()
        .with_cache_on_alt_fs()?
    else {
        return Ok(());
    };
    let wheel = binary_payload_wheel(&context)?;

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--preview-features")
        .arg("content-addressed-cache")
        .arg(&wheel), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    warning: Failed to hardlink files; falling back to full copy. This may lead to degraded performance.
             If the cache and target directories are on different filesystems, hardlinking may not be supported.
             If this is intentional, set `export UV_LINK_MODE=copy` or use `--link-mode=copy` to suppress this warning.
    Installed 1 package in [TIME]
     + binary-payload==0.1.0 (from file://[TEMP_DIR]/binary_payload-0.1.0-py3-none-any.whl)
    ");

    let archive_files = context.cache_files(CacheBucket::Files)?;
    assert_eq!(archive_files.len(), 7);

    let target = context.temp_dir.child("fallback-target");
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("--preview-features")
        .arg("content-addressed-cache")
        .arg("--target")
        .arg(target.path())
        .arg(&wheel), @"
    exit_code: 0 (success)
    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: .venv/[BIN]/[PYTHON]
    Resolved 1 package in [TIME]
    warning: Failed to hardlink files; falling back to full copy. This may lead to degraded performance.
             If the cache and target directories are on different filesystems, hardlinking may not be supported.
             If this is intentional, set `export UV_LINK_MODE=copy` or use `--link-mode=copy` to suppress this warning.
    Installed 1 package in [TIME]
     + binary-payload==0.1.0 (from file://[TEMP_DIR]/binary_payload-0.1.0-py3-none-any.whl)
    ");

    assert_eq!(
        fs_err::read(target.path().join("binary_payload").join("native.so"))?,
        BINARY_PAYLOAD_CONTENTS,
    );

    Ok(())
}

const BINARY_PAYLOAD_CONTENTS: &[u8] = b"binary payload contents\n";

fn binary_payload_wheel(context: &TestContext) -> Result<PathBuf> {
    const METADATA: &[u8] = b"Metadata-Version: 2.1\nName: binary-payload\nVersion: 0.1.0\n";
    const WHEEL: &[u8] =
        b"Wheel-Version: 1.0\nGenerator: uv-test\nRoot-Is-Purelib: true\nTag: py3-none-any\n";
    const RECORD: &[u8] = b"binary_payload/__init__.py,,\n\
binary_payload/module.py,,\n\
binary_payload/native.so,,\n\
binary_payload/plain.so,,\n\
binary_payload/versioned.so.1,,\n\
binary_payload/versioned.so.1.2,,\n\
binary_payload/native.dylib,,\n\
binary_payload/native.DLL,,\n\
binary_payload/native.pyd,,\n\
binary_payload/tool,,\n\
binary_payload/large.dat,,\n\
binary_payload-0.1.0.dist-info/ignored.so,,\n\
binary_payload-0.1.0.dist-info/METADATA,,\n\
binary_payload-0.1.0.dist-info/WHEEL,,\n\
binary_payload-0.1.0.dist-info/RECORD,,\n";

    let wheel = context
        .temp_dir
        .join("binary_payload-0.1.0-py3-none-any.whl");
    let mut writer = ZipFileWriter::new(Vec::new());
    let large_file = vec![0; 2 * 1024 * 1024];
    for (name, contents) in [
        ("binary_payload/__init__.py", &[][..]),
        (
            "binary_payload/module.py",
            b"VALUE = 'not binary'\n" as &[u8],
        ),
        ("binary_payload/native.so", BINARY_PAYLOAD_CONTENTS),
        ("binary_payload/plain.so", BINARY_PAYLOAD_CONTENTS),
        ("binary_payload/versioned.so.1", BINARY_PAYLOAD_CONTENTS),
        ("binary_payload/versioned.so.1.2", BINARY_PAYLOAD_CONTENTS),
        ("binary_payload/native.dylib", BINARY_PAYLOAD_CONTENTS),
        ("binary_payload/native.DLL", BINARY_PAYLOAD_CONTENTS),
        ("binary_payload/native.pyd", BINARY_PAYLOAD_CONTENTS),
        ("binary_payload/tool", BINARY_PAYLOAD_CONTENTS),
        ("binary_payload/large.dat", large_file.as_slice()),
        (
            "binary_payload-0.1.0.dist-info/ignored.so",
            BINARY_PAYLOAD_CONTENTS,
        ),
        ("binary_payload-0.1.0.dist-info/METADATA", METADATA),
        ("binary_payload-0.1.0.dist-info/WHEEL", WHEEL),
        ("binary_payload-0.1.0.dist-info/RECORD", RECORD),
    ] {
        let entry = ZipEntryBuilder::new(name.into(), Compression::Stored).unix_permissions(
            if matches!(name, "binary_payload/native.so" | "binary_payload/tool") {
                0o755
            } else {
                0o644
            },
        );
        block_on(writer.write_entry_whole(entry, contents))?;
    }
    fs_err::write(&wheel, block_on(writer.close())?)?;

    Ok(wheel)
}
