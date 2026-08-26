#[cfg(windows)]
use std::path::PathBuf;

use std::{env, path::Path, process::Command};

use anyhow::Context;
use assert_cmd::assert::OutputAssertExt;
use assert_fs::{
    assert::PathAssert,
    fixture::ChildPath,
    prelude::{FileTouch, FileWriteStr, PathChild, PathCreateDir},
};
use indoc::indoc;
use insta::allow_duplicates;
use predicates::prelude::predicate;
use tracing::debug;
use uv_test::{LATEST_PYTHON_3_12, TestContext, uv_snapshot};

use uv_fs::{Simplified, copy_dir_all, remove_symlink};
use uv_python::PythonInstallationKey;
use uv_python::managed::{
    ManagedPythonInstallation, ManagedPythonInstallations, PythonMinorVersionLink,
    platform_key_from_env,
};
use uv_static::EnvVars;
use walkdir::WalkDir;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

#[test]
fn python_install() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_filtered_latest_python_versions()
        .with_managed_python_dirs()
        .with_empty_python_install_mirror();

    // Install the latest version
    uv_snapshot!(context.filters(), context.python_install(), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.14.[LATEST] in [TIME]
     + cpython-3.14.[LATEST]-[PLATFORM] (python3.14)
    ");

    let bin_python = context
        .bin_dir
        .child(format!("python3.14{}", std::env::consts::EXE_SUFFIX));

    // The executable should be installed in the bin directory
    bin_python.assert(predicate::path::exists());

    // On Unix, it should be a link
    #[cfg(unix)]
    bin_python.assert(predicate::path::is_symlink());

    // The link should be a path to the binary
    if cfg!(unix) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python), @"[TEMP_DIR]/managed/cpython-3.14-[PLATFORM]/bin/python3.14"
            );
        });
    } else if cfg!(windows) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python), @"[TEMP_DIR]/managed/cpython-3.14-[PLATFORM]/python"
            );
        });
    }

    // The executable should "work"
    uv_snapshot!(context.filters(), Command::new(bin_python.as_os_str())
        .arg("-c").arg("import subprocess; print('hello world')"), @"
    exit_code: 0 (success)
    ----- stdout -----
    hello world
    ");

    // Should be a no-op when already installed
    uv_snapshot!(context.filters(), context.python_install(), @"
    exit_code: 0 (success)
    ----- stderr -----
    Python is already installed. Use `uv python install <request>` to install another version.
    ");

    // Similarly, when a requested version is already installed
    uv_snapshot!(context.filters(), context.python_install().arg("3.14"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Python 3.14 is already installed
    ");

    // You can opt-in to a reinstall
    uv_snapshot!(context.filters(), context.python_install().arg("3.14").arg("--reinstall"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.14.[LATEST] in [TIME]
     ~ cpython-3.14.[LATEST]-[PLATFORM] (python3.14)
    ");

    // The executable should still be present in the bin directory
    bin_python.assert(predicate::path::exists());

    // Uninstallation requires an argument
    uv_snapshot!(context.filters(), context.python_uninstall(), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: the following required arguments were not provided:
      <TARGETS>...

    Usage: uv python uninstall --cache-dir [CACHE_DIR] --install-dir <INSTALL_DIR> <TARGETS>...

    For more information, try '--help'.
    ");

    uv_snapshot!(context.filters(), context.python_uninstall().arg("3.14"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Searching for Python versions matching: Python 3.14
    Uninstalled Python 3.14.[LATEST] in [TIME]
     - cpython-3.14.[LATEST]-[PLATFORM] (python3.14)
    ");

    // The executable should be removed
    bin_python.assert(predicate::path::missing());
}

#[tokio::test]
#[cfg(feature = "test-python-managed")]
async fn python_install_multiple_build_variants() -> anyhow::Result<()> {
    let source = uv_test::test_context_with_versions!(&[]).with_managed_python_dirs();
    source
        .python_install()
        .args(["3.13.7", "3.13.7t", "--no-bin"])
        .assert()
        .success();

    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs();
    let platform = platform_key_from_env()?;
    let key = format!("cpython-3.13.7-{platform}").parse::<PythonInstallationKey>()?;
    let managed_dir = context.temp_dir.child("managed");
    let mut downloads = serde_json::Map::new();

    // Copy the unpacked installations without their minor-version links or executable aliases.
    // Installing them together must recognize aliases created earlier in the same command.
    for runtime in [None, Some("freethreaded")] {
        let runtime_suffix = runtime.map_or(String::new(), |runtime| format!("+{runtime}"));
        let stock_name = format!("cpython-3.13.7{runtime_suffix}-{platform}");
        for build_variant in [None, Some("custom")] {
            let build_suffix = build_variant.map_or(String::new(), |variant| format!("+{variant}"));
            let name = format!("cpython-3.13.7{runtime_suffix}{build_suffix}-{platform}");
            copy_dir_all(
                source.temp_dir.child("managed").child(&stock_name),
                managed_dir.child(&name),
            )?;
            downloads.insert(
                name,
                serde_json::json!({
                    "name": "cpython",
                    "arch": { "family": key.arch().family().to_string(), "variant": null },
                    "os": key.os().to_string(),
                    "libc": key.libc().to_string(),
                    "major": 3,
                    "minor": 13,
                    "patch": 7,
                    "prerelease": "",
                    "variant": runtime,
                    "build_variant": build_variant,
                    "default": build_variant.is_none(),
                    "url": "https://custom.example/cpython.tar.gz",
                    "sha256": null,
                    "build": null
                }),
            );
        }
    }

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/metadata"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "version": 1,
            "downloads": downloads,
        })))
        .mount(&server)
        .await;

    uv_snapshot!(context.filters(), context.python_install()
        .args(["3.13", "3.13+custom", "3.13+freethreaded", "3.13+freethreaded+custom"])
        .arg("--bin")
        .arg("--python-downloads-json-url")
        .arg(format!("{}/metadata", server.uri())), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed 2 versions in [TIME]
     + cpython-3.13.7+freethreaded-[PLATFORM] (python3.13t)
     + cpython-3.13.7-[PLATFORM] (python3.13)
    ");

    let installations = ManagedPythonInstallations::from_settings(Some(managed_dir.to_path_buf()))?;
    for installation in installations.find_all()? {
        let minor_link = PythonMinorVersionLink::from_installation(&installation)
            .context("Missing minor-version link")?;
        assert!(minor_link.exists());
        if installation.key().build_variant().is_none() {
            let executable = context
                .bin_dir
                .child(installation.key().executable_name_minor());
            assert_eq!(
                canonicalize_link_path(&executable),
                installation
                    .executable(false)
                    .simplified_display()
                    .to_string(),
            );
        }
    }

    Ok(())
}

fn python_build_variant_context() -> anyhow::Result<(TestContext, ManagedPythonInstallation)> {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_python_sources()
        .with_filtered_python_install_bin()
        .with_filtered_python_names()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs();
    context.python_install().arg("3.13").assert().success();
    let managed_dir = context.temp_dir.child("managed");
    let installations = ManagedPythonInstallations::from_settings(Some(managed_dir.to_path_buf()))?;
    let stock = installations
        .find_all()?
        .find(|installation| installation.key().major() == 3 && installation.key().minor() == 13)
        .context("Missing stock installation")?;
    Ok((context, stock))
}

fn python_custom_build_variant_context()
-> anyhow::Result<(TestContext, ManagedPythonInstallation, ChildPath)> {
    let (context, stock) = python_build_variant_context()?;
    let stock_name = stock.key().to_string();
    let platform = platform_key_from_env()?;
    let version = stock_name
        .strip_suffix(&format!("-{platform}"))
        .context("Missing platform suffix")?;
    let custom_path = context
        .temp_dir
        .child("managed")
        .child(format!("{version}+custom-{platform}"));
    // Keep stock installed so environments using it remain healthy when switching builds.
    copy_dir_all(stock.path(), &custom_path)?;
    Ok((context, stock, custom_path))
}

fn python_build_variant_project(context: &TestContext) -> anyhow::Result<ChildPath> {
    let project = context.temp_dir.child("project");
    project.create_dir_all()?;
    project.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.13"
        dependencies = []
    "#})?;
    Ok(project)
}

fn python_build_variant_catalog_context() -> anyhow::Result<(
    TestContext,
    ManagedPythonInstallation,
    serde_json::Map<String, serde_json::Value>,
)> {
    let (context, stock) = python_build_variant_context()?;
    let context = context.with_filtered_latest_python_versions();
    let managed_dir = context.temp_dir.child("managed");
    let stock_path = stock.path();
    let key = stock.key();
    let stock_name = key.to_string();
    let platform = platform_key_from_env()?;
    let version = stock_name
        .strip_suffix(&format!("-{platform}"))
        .context("Missing platform suffix")?;
    let mut downloads = serde_json::Map::new();
    let arch = key.arch().to_string();
    let arch_family = key.arch().family().to_string();
    let arch_variant = arch.strip_prefix(&format!("{arch_family}_"));
    for (variant, default) in [
        ("pgo+lto", true),
        ("noopt", false),
        ("custom+pgo+lto", false),
    ] {
        let name = format!("{version}+{variant}-{platform}");
        copy_dir_all(stock_path, managed_dir.join(&name))?;
        downloads.insert(
            name,
            serde_json::json!({
                "name": "cpython",
                "arch": {"family": arch_family, "variant": arch_variant},
                "os": key.os().to_string(),
                "libc": key.libc().to_string(),
                "major": key.major(),
                "minor": key.minor(),
                "patch": key.version().patch().unwrap_or_default(),
                "prerelease": "",
                "variant": null,
                "build_variant": variant,
                "default": default,
                "url": "https://custom.example/cpython.tar.gz",
                "sha256": null,
                "build": null
            }),
        );
    }
    Ok((context, stock, downloads))
}

async fn mount_python_build_variant_catalog(
    server: &MockServer,
    downloads: serde_json::Map<String, serde_json::Value>,
    cache_control: &str,
) {
    Mock::given(method("GET"))
        .and(path("/metadata"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Cache-Control", cache_control)
                .set_body_json(serde_json::json!({"version": 1, "downloads": downloads})),
        )
        .mount(server)
        .await;
}

#[tokio::test]
#[cfg(feature = "test-python-managed")]
async fn python_install_build_variant() -> anyhow::Result<()> {
    let (context, stock, custom_path) = python_custom_build_variant_context()?;
    let custom_name = custom_path
        .file_name()
        .context("Missing custom installation name")?
        .to_string_lossy();
    let key = stock.key();
    let arch = key.arch().to_string();
    let arch_family = key.arch().family().to_string();
    let arch_variant = arch.strip_prefix(&format!("{arch_family}_"));

    let server = MockServer::start().await;
    let metadata = serde_json::json!({
        (custom_name): {
            "name": "cpython",
            "arch": {
                "family": arch_family,
                "variant": arch_variant
            },
            "os": key.os().to_string(),
            "libc": key.libc().to_string(),
            "major": key.major(),
            "minor": key.minor(),
            "patch": key.version().patch().unwrap_or_default(),
            "prerelease": "",
            "url": "https://custom.example/cpython.tar.gz",
            "sha256": "0000000000000000000000000000000000000000000000000000000000000000",
            "variant": null,
            "build_variant": "custom",
            "default": false,
            "build": null
        }
    });
    Mock::given(method("GET"))
        .and(path("/metadata"))
        .respond_with(ResponseTemplate::new(200).set_body_json(metadata))
        .mount(&server)
        .await;

    context
        .python_install()
        .arg("3.13+custom")
        .arg("--default")
        .arg("--force")
        .arg("--python-downloads-json-url")
        .arg(format!("{}/metadata", server.uri()))
        .assert()
        .success();

    context
        .bin_dir
        .child(format!("python3.13{}", std::env::consts::EXE_SUFFIX))
        .assert(predicate::path::exists());
    context
        .bin_dir
        .child(format!("python3.13+custom{}", std::env::consts::EXE_SUFFIX))
        .assert(predicate::path::missing());

    uv_snapshot!(context.filters(), context.python_find().arg("3.13+custom"), @"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13+custom-[PLATFORM]/[INSTALL-BIN]/[PYTHON]
    ");

    fs_err::write(custom_path.join("BUILD"), "custom-build")?;
    let find_dir = context.home_dir.child("find");
    find_dir.create_dir_all()?;
    context
        .python_find()
        .current_dir(find_dir.path())
        .arg("3.13+custom")
        .env(EnvVars::UV_PYTHON_BUILD, "custom-build")
        .assert()
        .success();
    context
        .python_find()
        .current_dir(find_dir.path())
        .arg("3.13+custom")
        .env(EnvVars::UV_PYTHON_BUILD, "missing-build")
        .assert()
        .failure();
    context
        .python_find()
        .current_dir(find_dir.path())
        .arg("3.13")
        .env(EnvVars::UV_PYTHON_BUILD, "missing-build")
        .assert()
        .success();
    context
        .python_find()
        .current_dir(find_dir.path())
        .arg("3.13+custom")
        .env(EnvVars::UV_PYTHON_CPYTHON_BUILD, "missing-build")
        .assert()
        .success();

    Ok(())
}

#[test]
#[cfg(feature = "test-python-managed")]
fn python_init_build_variant() -> anyhow::Result<()> {
    let (context, _stock, custom_path) = python_custom_build_variant_context()?;
    let custom_name = custom_path
        .file_name()
        .context("Missing custom installation name")?
        .to_string_lossy();
    for (directory, request) in [
        ("init-major", "3+custom"),
        ("init-implementation", "cpython@3.13+custom"),
        ("init-key", custom_name.as_ref()),
    ] {
        context
            .init()
            .arg(directory)
            .arg("--no-workspace")
            .arg("--python")
            .arg(request)
            .assert()
            .success();
        assert_eq!(
            context.read(format!("{directory}/.python-version")),
            "3.13+custom\n"
        );
    }

    Ok(())
}

#[test]
#[cfg(feature = "test-python-managed")]
fn python_project_build_variant_centralized() -> anyhow::Result<()> {
    let (context, _stock, custom_path) = python_custom_build_variant_context()?;
    // Use a minor-version link so the custom environment is upgradeable.
    let installations = ManagedPythonInstallations::from_settings(Some(
        context.temp_dir.child("managed").to_path_buf(),
    ))?;
    installations
        .find_all()?
        .find(|installation| installation.path() == custom_path.path())
        .context("Missing custom installation")?
        .ensure_minor_version_link()?;
    let context = context
        .with_filtered_latest_python_versions()
        .with_filtered_centralized_environment_hashes();

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.13"
        dependencies = []
        "#,
    )?;
    context
        .sync()
        .arg("--preview-features")
        .arg("centralized-project-envs")
        .arg("--python")
        .arg("3.13")
        .assert()
        .success();
    let stock_environment = fs_err::canonicalize(&context.venv)?;
    context.venv.child("stock-marker").touch()?;

    // The existing stock environment must still run before testing build compatibility.
    let base_prefix =
        "import sys; from pathlib import Path; print(Path(sys.base_prefix).resolve().as_posix())";
    assert!(context.interpreter().is_file());
    uv_snapshot!(context.filters(), context.python_command().arg("-c").arg(base_prefix), @"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13.[LATEST]-[PLATFORM]
    ");

    uv_snapshot!(context.filters(), context.sync()
        .arg("--preview-features")
        .arg("centralized-project-envs")
        .arg("--python")
        .arg("3.13+custom"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Using CPython 3.13.[LATEST]
    Creating virtual environment `project-cp3.13-[HASH]`
    Resolved 1 package in [TIME]
    Checked in [TIME]
    ");
    let custom_environment = fs_err::canonicalize(&context.venv)?;
    assert_ne!(stock_environment, custom_environment);
    assert!(stock_environment.join("stock-marker").is_file());
    context.venv.child("custom-marker").touch()?;

    uv_snapshot!(context.filters(), context.python_command().arg("-c").arg(base_prefix), @"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13.[LATEST]+custom-[PLATFORM]
    ");

    // Running with the same build reuses the custom environment.
    uv_snapshot!(context.filters(), context.run()
        .arg("--preview-features")
        .arg("centralized-project-envs")
        .arg("--python")
        .arg("3.13+custom")
        .arg("python")
        .arg("-c")
        .arg(base_prefix), @"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13.[LATEST]+custom-[PLATFORM]

    ----- stderr -----
    Resolved 1 package in [TIME]
    Checked in [TIME]
    ");
    assert_eq!(custom_environment, fs_err::canonicalize(&context.venv)?);
    context
        .venv
        .child("custom-marker")
        .assert(predicate::path::exists());

    // An unqualified request selects the stock catalog default instead of the custom environment.
    uv_snapshot!(context.filters(), context.run()
        .arg("--preview-features")
        .arg("centralized-project-envs")
        .arg("--python")
        .arg("3.13")
        .arg("python")
        .arg("-c")
        .arg(base_prefix), @"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13.[LATEST]-[PLATFORM]

    ----- stderr -----
    Resolved 1 package in [TIME]
    Checked in [TIME]
    ");
    assert_eq!(stock_environment, fs_err::canonicalize(&context.venv)?);
    context
        .venv
        .child("stock-marker")
        .assert(predicate::path::exists());
    assert!(custom_environment.join("custom-marker").is_file());

    // `uv run` must also switch a healthy stock environment to the requested custom build.
    uv_snapshot!(context.filters(), context.run()
        .arg("--preview-features")
        .arg("centralized-project-envs")
        .arg("--python")
        .arg("3.13+custom")
        .arg("python")
        .arg("-c")
        .arg(base_prefix), @"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13.[LATEST]+custom-[PLATFORM]

    ----- stderr -----
    Resolved 1 package in [TIME]
    Checked in [TIME]
    ");
    assert_eq!(custom_environment, fs_err::canonicalize(&context.venv)?);
    context
        .venv
        .child("custom-marker")
        .assert(predicate::path::exists());

    Ok(())
}

#[test]
#[cfg(feature = "test-python-managed")]
fn python_find_build_variant_tags() -> anyhow::Result<()> {
    let (context, stock) = python_build_variant_context()?;
    let context = context.with_filtered_latest_python_versions();
    let managed_dir = context.temp_dir.child("managed");
    let stock_name = stock.key().to_string();
    let platform = platform_key_from_env()?;
    let version = stock_name
        .strip_suffix(&format!("-{platform}"))
        .context("Missing platform suffix")?;
    // Discover a composite build with the same tags in any order.
    let optimized_name = format!("{version}+custom+pgo+lto-{platform}");
    let optimized_path = managed_dir.join(&optimized_name);
    fs_err::rename(stock.path(), &optimized_path)?;

    uv_snapshot!(context.filters(), context.python_find().arg("3.13+lto+pgo+custom"), @"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13.[LATEST]+custom+pgo+lto-[PLATFORM]/[INSTALL-BIN]/[PYTHON]
    ");

    uv_snapshot!(context.filters(), context.python_find().arg("3.13+custom+lto+pgo"), @"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13.[LATEST]+custom+pgo+lto-[PLATFORM]/[INSTALL-BIN]/[PYTHON]
    ");
    uv_snapshot!(context.filters(), context.python_find().arg("3.13+custom"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: No interpreter found for Python 3.13+custom in [PYTHON SOURCES]
    ");
    uv_snapshot!(context.filters(), context.python_find().arg("3.13+custom+lto"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: No interpreter found for Python 3.13+custom+lto in [PYTHON SOURCES]
    ");
    uv_snapshot!(context.filters(), context.python_find().arg("3.13+custom+pgo+lto+extra"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: No interpreter found for Python 3.13+custom+pgo+lto+extra in [PYTHON SOURCES]
    ");
    // A build with just the requested tag remains distinct from the composite build.
    let custom_path = managed_dir.join(format!("{version}+custom-{platform}"));
    copy_dir_all(&optimized_path, &custom_path)?;
    uv_snapshot!(context.filters(), context.python_find().arg("3.13+custom"), @"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13.[LATEST]+custom-[PLATFORM]/[INSTALL-BIN]/[PYTHON]
    ");
    Ok(())
}

#[tokio::test]
#[cfg(feature = "test-python-managed")]
async fn python_build_variant_catalog_selection() -> anyhow::Result<()> {
    let (context, stock, downloads) = python_build_variant_catalog_context()?;
    let server = MockServer::start().await;
    mount_python_build_variant_catalog(&server, downloads, "max-age=0").await;
    let managed_dir = context.temp_dir.child("managed");
    let stock_name = stock.key().to_string();
    let platform = platform_key_from_env()?;
    let version = stock_name
        .strip_suffix(&format!("-{platform}"))
        .context("Missing platform suffix")?;
    let metadata_url = format!("{}/metadata", server.uri());
    let find = |request| {
        let mut command = context.python_find();
        command
            .arg(request)
            .arg("--python-downloads-json-url")
            .arg(&metadata_url);
        command
    };

    for variant in ["pgo+lto", "custom+pgo+lto"] {
        let executable = managed_dir
            .join(format!("{version}+{variant}-{platform}"))
            .join(stock.executable(false).strip_prefix(stock.path())?);
        Command::new(executable)
            .arg("-c")
            .arg("import sys; assert sys.version_info[:2] == (3, 13)")
            .assert()
            .success();
    }

    // Unqualified requests select the default; explicit requests match every build tag.
    uv_snapshot!(context.filters(), find("3.13"), @"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13.[LATEST]+pgo+lto-[PLATFORM]/[INSTALL-BIN]/[PYTHON]
    ");
    assert_eq!(
        server
            .received_requests()
            .await
            .context("Missing request log")?
            .len(),
        1
    );
    uv_snapshot!(context.filters(), find("3.13+pgo+lto"), @"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13.[LATEST]+pgo+lto-[PLATFORM]/[INSTALL-BIN]/[PYTHON]
    ");
    uv_snapshot!(context.filters(), find("3.13+lto+pgo"), @"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13.[LATEST]+pgo+lto-[PLATFORM]/[INSTALL-BIN]/[PYTHON]
    ");
    // Provider and optimization tags can be reordered.
    uv_snapshot!(context.filters(), find("3.13+custom+pgo+lto"), @"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13.[LATEST]+custom+pgo+lto-[PLATFORM]/[INSTALL-BIN]/[PYTHON]
    ");
    uv_snapshot!(context.filters(), find("3.13+lto+custom+pgo"), @"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13.[LATEST]+custom+pgo+lto-[PLATFORM]/[INSTALL-BIN]/[PYTHON]
    ");
    // A non-default stock optimization variant is still selectable.
    uv_snapshot!(context.filters(), find("3.13+noopt"), @"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13.[LATEST]+noopt-[PLATFORM]/[INSTALL-BIN]/[PYTHON]
    ");

    assert_eq!(
        server
            .received_requests()
            .await
            .context("Missing request log")?
            .len(),
        1
    );

    Ok(())
}

#[tokio::test]
#[cfg(feature = "test-python-managed")]
async fn python_build_variant_catalog_cached() -> anyhow::Result<()> {
    let (context, _stock, downloads) = python_build_variant_catalog_context()?;
    let server = MockServer::start().await;
    mount_python_build_variant_catalog(&server, downloads, "max-age=0").await;
    let metadata_url = format!("{}/metadata", server.uri());
    context
        .python_find()
        .arg("3.13")
        .arg("--python-downloads-json-url")
        .arg(&metadata_url)
        .assert()
        .success();
    assert_eq!(
        server
            .received_requests()
            .await
            .context("Missing request log")?
            .len(),
        1
    );

    let base_prefix = "import os, sys; print(os.path.realpath(sys.base_prefix))";
    let run = || {
        let mut command = context.run();
        command
            .arg("--python")
            .arg("3.13")
            .env_remove(EnvVars::VIRTUAL_ENV)
            .env(EnvVars::UV_PYTHON_DOWNLOADS_JSON_URL, &metadata_url);
        command
    };

    // Installed candidates use the cached catalog even when HTTP metadata has expired.
    uv_snapshot!(context.filters(), run()
        .arg("python").arg("-c").arg(base_prefix), @"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13.[LATEST]+pgo+lto-[PLATFORM]
    ");
    context.temp_dir.child("requirements.in").write_str("")?;
    uv_snapshot!(context.filters(), context.pip_compile().arg("requirements.in")
        .arg("--python-version").arg("3.13").arg("--no-header")
        .env(EnvVars::UV_PYTHON_DOWNLOADS_JSON_URL, &metadata_url), @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: Requirements file `requirements.in` does not contain any dependencies
    Resolved in [TIME]
    ");
    assert_eq!(
        server
            .received_requests()
            .await
            .context("Missing request log")?
            .len(),
        1
    );

    Ok(())
}

#[tokio::test]
#[cfg(feature = "test-python-managed")]
async fn python_build_variant_catalog_missing_default() -> anyhow::Result<()> {
    let (context, stock, downloads) = python_build_variant_catalog_context()?;
    let server = MockServer::start().await;
    mount_python_build_variant_catalog(&server, downloads, "max-age=0").await;
    let managed_dir = context.temp_dir.child("managed");
    let stock_name = stock.key().to_string();
    let platform = platform_key_from_env()?;
    let version = stock_name
        .strip_suffix(&format!("-{platform}"))
        .context("Missing platform suffix")?;
    let metadata_url = format!("{}/metadata", server.uri());
    let find = |request| {
        let mut command = context.python_find();
        command
            .arg(request)
            .arg("--python-downloads-json-url")
            .arg(&metadata_url);
        command
    };

    // Warm the catalog before removing its default candidate.
    find("3.13").assert().success();

    // Removing stock must not turn the installed non-default custom build into a match.
    let optimized_path = managed_dir.join(format!("{version}+pgo+lto-{platform}"));
    let hidden_path = context.temp_dir.join("stock-optimized");
    fs_err::rename(&optimized_path, &hidden_path)?;
    uv_snapshot!(context.filters(), find("3.13"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: No interpreter found for Python 3.13 in [PYTHON SOURCES]
    ");
    assert_eq!(
        server
            .received_requests()
            .await
            .context("Missing request log")?
            .len(),
        2
    );
    uv_snapshot!(context.filters(), find("3.13+pgo+lto"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: No interpreter found for Python 3.13+pgo+lto in [PYTHON SOURCES]
    ");
    uv_snapshot!(context.filters(), context.python_install().arg("3.13")
        .env(EnvVars::UV_PYTHON_DOWNLOADS, "never")
        .arg("--python-downloads-json-url").arg(&metadata_url), @r#"
    exit_code: 1 (failure)
    ----- stderr -----
    Python downloads are not allowed (`python-downloads = "never"`). Change to `python-downloads = "manual"` to allow explicit installs.
    "#);
    uv_snapshot!(context.filters(), context.python_install().arg("3.13+pgo+lto")
        .env(EnvVars::UV_PYTHON_DOWNLOADS, "never")
        .arg("--python-downloads-json-url").arg(&metadata_url), @r#"
    exit_code: 1 (failure)
    ----- stderr -----
    Python downloads are not allowed (`python-downloads = "never"`). Change to `python-downloads = "manual"` to allow explicit installs.
    "#);
    fs_err::rename(&hidden_path, &optimized_path)?;

    Ok(())
}

#[tokio::test]
#[cfg(feature = "test-python-managed")]
async fn python_build_variant_catalog_custom_default() -> anyhow::Result<()> {
    let (context, stock, mut downloads) = python_build_variant_catalog_context()?;
    let server = MockServer::start().await;
    for download in downloads.values_mut() {
        download["default"] = serde_json::json!(download["build_variant"] == "custom+pgo+lto");
    }
    mount_python_build_variant_catalog(&server, downloads, "max-age=86400").await;
    let managed_dir = context.temp_dir.child("managed");
    let stock_name = stock.key().to_string();
    let platform = platform_key_from_env()?;
    let version = stock_name
        .strip_suffix(&format!("-{platform}"))
        .context("Missing platform suffix")?;
    let metadata_url = format!("{}/metadata", server.uri());
    let find = |request| {
        let mut command = context.python_find();
        command
            .arg(request)
            .arg("--python-downloads-json-url")
            .arg(&metadata_url);
        command
    };

    uv_snapshot!(context.filters(), find("3.13"), @"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13.[LATEST]+custom+pgo+lto-[PLATFORM]/[INSTALL-BIN]/[PYTHON]
    ");
    // Explicit tags select their matching build independently of the catalog default.
    uv_snapshot!(context.filters(), find("3.13+pgo+lto"), @"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13.[LATEST]+pgo+lto-[PLATFORM]/[INSTALL-BIN]/[PYTHON]
    ");
    uv_snapshot!(context.filters(), find("3.13+lto+custom+pgo"), @"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13.[LATEST]+custom+pgo+lto-[PLATFORM]/[INSTALL-BIN]/[PYTHON]
    ");
    uv_snapshot!(context.filters(), find("3.13+custom"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: No interpreter found for Python 3.13+custom in [PYTHON SOURCES]
    ");
    uv_snapshot!(context.filters(), find("3.13+custom+lto"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: No interpreter found for Python 3.13+custom+lto in [PYTHON SOURCES]
    ");

    // Missing required optimization tags still reject a default custom build.
    uv_snapshot!(context.filters(), find("3.13+custom+noopt"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: No interpreter found for Python 3.13+custom+noopt in [PYTHON SOURCES]
    ");

    // An installed stock build cannot satisfy an unqualified install when custom is the default.
    let custom_path = managed_dir.join(format!("{version}+custom+pgo+lto-{platform}"));
    let hidden_custom_path = context.temp_dir.join("custom-optimized");
    fs_err::rename(&custom_path, &hidden_custom_path)?;
    uv_snapshot!(context.filters(), context.python_install().arg("3.13")
        .env(EnvVars::UV_PYTHON_DOWNLOADS, "never")
        .arg("--python-downloads-json-url").arg(&metadata_url), @r#"
    exit_code: 1 (failure)
    ----- stderr -----
    Python downloads are not allowed (`python-downloads = "never"`). Change to `python-downloads = "manual"` to allow explicit installs.
    "#);
    fs_err::rename(&hidden_custom_path, &custom_path)?;

    Ok(())
}

#[tokio::test]
#[cfg(feature = "test-python-managed")]
async fn python_build_variant_catalog_fallback() -> anyhow::Result<()> {
    let (context, stock, mut downloads) = python_build_variant_catalog_context()?;
    for download in downloads.values_mut() {
        download["default"] = serde_json::json!(download["build_variant"] == "custom+pgo+lto");
    }
    let server = MockServer::start().await;
    mount_python_build_variant_catalog(&server, downloads, "max-age=86400").await;
    let metadata_url = format!("{}/metadata", server.uri());
    context.temp_dir.child("requirements.in").write_str("")?;
    let compile = || {
        let mut command = context.pip_compile();
        command
            .arg("requirements.in")
            .args(["--python-version", "3.8", "--no-python-downloads"])
            .env_remove(EnvVars::VIRTUAL_ENV)
            .env(EnvVars::UV_PYTHON_PREFERENCE, "only-managed")
            .env(EnvVars::UV_PYTHON_DOWNLOADS_JSON_URL, &metadata_url);
        command
    };

    // The installed catalog default is eligible when falling back to a different version.
    uv_snapshot!(context.filters(), compile(), @"
    exit_code: 0 (success)
    ----- stdout -----
    # This file was autogenerated by uv via the following command:
    #    uv pip compile --cache-dir [CACHE_DIR] requirements.in --python-version 3.8 --no-python-downloads

    ----- stderr -----
    warning: Requirements file `requirements.in` does not contain any dependencies
    warning: The requested Python version 3.8 is not available; 3.13.[LATEST] will be used to build dependencies instead.
    Resolved in [TIME]
    ");

    // Removing the default must not allow an installed non-default build to satisfy the fallback.
    let stock_name = stock.key().to_string();
    let platform = platform_key_from_env()?;
    let version = stock_name
        .strip_suffix(&format!("-{platform}"))
        .context("Missing platform suffix")?;
    fs_err::remove_dir_all(
        context
            .temp_dir
            .child("managed")
            .join(format!("{version}+custom+pgo+lto-{platform}")),
    )?;
    uv_snapshot!(context.filters(), compile(), @"
    exit_code: 2 (failure)
    ----- stderr -----
    warning: Requirements file `requirements.in` does not contain any dependencies
    error: No interpreter found for Python 3.8 in virtual environments or managed installations
    ");

    Ok(())
}

#[tokio::test]
#[cfg(feature = "test-python-managed")]
async fn python_project_build_variant_catalog() -> anyhow::Result<()> {
    let (context, _stock, downloads) = python_build_variant_catalog_context()?;
    let server = MockServer::start().await;
    mount_python_build_variant_catalog(&server, downloads, "max-age=0").await;
    let metadata_url = format!("{}/metadata", server.uri());
    let find = |request| {
        let mut command = context.python_find();
        command
            .arg(request)
            .arg("--python-downloads-json-url")
            .arg(&metadata_url);
        command
    };

    let base_prefix = "import os, sys; print(os.path.realpath(sys.base_prefix))";
    find("3.13").assert().success();

    let requests_before_project = server
        .received_requests()
        .await
        .context("Missing request log")?
        .len();
    let project = python_build_variant_project(&context)?;
    context
        .sync()
        .current_dir(&project)
        .arg("--python")
        .arg("3.13+custom+pgo+lto")
        .env(EnvVars::UV_PYTHON_DOWNLOADS_JSON_URL, &metadata_url)
        .assert()
        .success();
    let project_run = |request| {
        let mut command = context.run();
        command
            .current_dir(&project)
            .env_remove(EnvVars::VIRTUAL_ENV)
            .arg("--python")
            .arg(request)
            .env(EnvVars::UV_PYTHON_DOWNLOADS_JSON_URL, &metadata_url);
        command
    };

    // Reordered build tags reuse the environment even when it is not the default.
    project.child(".venv/custom-marker").touch()?;
    uv_snapshot!(context.filters(), project_run("3.13+lto+pgo+custom")
        .arg("python").arg("-c").arg(base_prefix), @"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13.[LATEST]+custom+pgo+lto-[PLATFORM]

    ----- stderr -----
    Resolved 1 package in [TIME]
    Checked in [TIME]
    ");
    project
        .child(".venv/custom-marker")
        .assert(predicate::path::exists());

    assert_eq!(
        server
            .received_requests()
            .await
            .context("Missing request log")?
            .len(),
        requests_before_project
    );

    // A request missing build tags must not reuse the composite build's environment.
    uv_snapshot!(context.filters(), project_run("3.13+custom")
        .env(EnvVars::UV_PYTHON_DOWNLOADS, "never")
        .arg("python").arg("-c").arg(base_prefix), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: No interpreter found for Python 3.13+custom in [PYTHON SOURCES]
    ");
    project
        .child(".venv/custom-marker")
        .assert(predicate::path::exists());

    // An unqualified request replaces the non-default custom environment with stock.
    uv_snapshot!(context.filters(), project_run("3.13")
        .arg("python").arg("-c").arg(base_prefix), @"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13.[LATEST]+pgo+lto-[PLATFORM]

    ----- stderr -----
    Using CPython 3.13.[LATEST]
    Removed virtual environment at: .venv
    Creating virtual environment at: .venv
    Resolved 1 package in [TIME]
    Checked in [TIME]
    ");
    project
        .child(".venv/custom-marker")
        .assert(predicate::path::missing());

    Ok(())
}

#[tokio::test]
#[cfg(feature = "test-python-managed")]
async fn python_project_build_variant_catalog_refresh() -> anyhow::Result<()> {
    let (context, _stock, mut downloads) = python_build_variant_catalog_context()?;
    let server = MockServer::start().await;
    mount_python_build_variant_catalog(&server, downloads.clone(), "max-age=0").await;
    let metadata_url = format!("{}/metadata", server.uri());
    let find = |request| {
        let mut command = context.python_find();
        command
            .arg(request)
            .arg("--python-downloads-json-url")
            .arg(&metadata_url);
        command
    };

    let base_prefix = "import os, sys; print(os.path.realpath(sys.base_prefix))";
    let project = python_build_variant_project(&context)?;
    context
        .sync()
        .current_dir(&project)
        .arg("--python")
        .arg("3.13")
        .env(EnvVars::UV_PYTHON_DOWNLOADS_JSON_URL, &metadata_url)
        .assert()
        .success();
    let project_run = |request| {
        let mut command = context.run();
        command
            .current_dir(&project)
            .env_remove(EnvVars::VIRTUAL_ENV)
            .arg("--python")
            .arg(request)
            .env(EnvVars::UV_PYTHON_DOWNLOADS_JSON_URL, &metadata_url);
        command
    };

    project.child(".venv/stock-marker").touch()?;

    // Change the catalog default with both installations present and healthy.
    for download in downloads.values_mut() {
        download["default"] = serde_json::json!(download["build_variant"] == "custom+pgo+lto");
    }
    server.reset().await;
    mount_python_build_variant_catalog(&server, downloads.clone(), "max-age=86400").await;
    // The cached catalog remains authoritative when reusing an existing environment.
    uv_snapshot!(context.filters(), project_run("3.13")
        .arg("python").arg("-c").arg(base_prefix), @"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13.[LATEST]+pgo+lto-[PLATFORM]

    ----- stderr -----
    Resolved 1 package in [TIME]
    Checked in [TIME]
    ");
    project
        .child(".venv/stock-marker")
        .assert(predicate::path::exists());
    // A changed remote default does not invalidate an installed candidate in the cached catalog.
    uv_snapshot!(context.filters(), find("3.13"), @"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13.[LATEST]+pgo+lto-[PLATFORM]/[INSTALL-BIN]/[PYTHON]
    ");
    assert!(
        server
            .received_requests()
            .await
            .context("Missing request log")?
            .is_empty()
    );
    // Explicit refresh observes the new default even though the cached candidate is installed.
    uv_snapshot!(context.filters(), project_run("3.13").arg("--refresh")
        .arg("python").arg("-c").arg(base_prefix), @"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13.[LATEST]+custom+pgo+lto-[PLATFORM]

    ----- stderr -----
    Using CPython 3.13.[LATEST]
    Removed virtual environment at: .venv
    Creating virtual environment at: .venv
    Resolved 1 package in [TIME]
    Checked in [TIME]
    ");
    assert_eq!(
        server
            .received_requests()
            .await
            .context("Missing request log")?
            .len(),
        1
    );
    project
        .child(".venv/stock-marker")
        .assert(predicate::path::missing());

    // An unqualified request reuses custom when the catalog marks it as the default.
    project.child(".venv/custom-marker").touch()?;
    uv_snapshot!(context.filters(), project_run("3.13")
        .arg("python").arg("-c").arg(base_prefix), @"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13.[LATEST]+custom+pgo+lto-[PLATFORM]

    ----- stderr -----
    Resolved 1 package in [TIME]
    Checked in [TIME]
    ");
    project
        .child(".venv/custom-marker")
        .assert(predicate::path::exists());

    Ok(())
}

#[test]
#[cfg(feature = "test-python-managed")]
fn python_project_build_variant_catalog_error() -> anyhow::Result<()> {
    let (context, _stock, _downloads) = python_build_variant_catalog_context()?;
    let project = python_build_variant_project(&context)?;
    context
        .sync()
        .current_dir(&project)
        .arg("--python")
        .arg("3.13+custom+pgo+lto")
        .assert()
        .success();
    let base_prefix = "import os, sys; print(os.path.realpath(sys.base_prefix))";
    let project_run = |request| {
        let mut command = context.run();
        command
            .current_dir(&project)
            .env_remove(EnvVars::VIRTUAL_ENV)
            .arg("--python")
            .arg(request);
        command
    };

    project.child(".venv/custom-marker").touch()?;

    // Catalog errors must not cause a healthy environment to be removed.
    project_run("3.13")
        .env(
            EnvVars::UV_PYTHON_DOWNLOADS_JSON_URL,
            "missing-catalog.json",
        )
        .arg("python")
        .arg("-c")
        .arg(base_prefix)
        .assert()
        .failure();
    // A system-only preference rejects the managed environment without reading the catalog.
    uv_snapshot!(context.filters(), project_run("3.13")
        .arg("--no-managed-python").arg("--no-sync")
        .env(EnvVars::UV_PYTHON_DOWNLOADS_JSON_URL, "missing-catalog.json")
        .arg("python").arg("-c").arg(base_prefix), @"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13.[LATEST]+custom+pgo+lto-[PLATFORM]

    ----- stderr -----
    warning: Using incompatible environment (`.venv`) due to `--no-sync` (The project environment's Python interpreter does not meet the Python preference: `only system`)
    ");
    project
        .child(".venv/custom-marker")
        .assert(predicate::path::exists());

    Ok(())
}

#[test]
#[cfg(feature = "test-python-managed")]
fn python_build_variant_catalog_explicit_path() -> anyhow::Result<()> {
    let (context, stock, _downloads) = python_build_variant_catalog_context()?;

    let project = python_build_variant_project(&context)?;
    context
        .sync()
        .current_dir(&project)
        .arg("--python")
        .arg("3.13+custom+pgo+lto")
        .assert()
        .success();
    let base_prefix = "import os, sys; print(os.path.realpath(sys.base_prefix))";

    // An explicitly requested path remains usable independently of catalog defaults.
    let executable = stock.executable(false);
    uv_snapshot!(context.filters(), context.python_find().arg(&executable)
        .arg("--python-downloads-json-url").arg("missing-catalog.json"), @"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13.[LATEST]-[PLATFORM]/[INSTALL-BIN]/[PYTHON]
    ");

    // An explicit executable request can switch environments without reading the catalog.
    context
        .run()
        .current_dir(&project)
        .env_remove(EnvVars::VIRTUAL_ENV)
        .arg("--python")
        .arg(&executable)
        .env(
            EnvVars::UV_PYTHON_DOWNLOADS_JSON_URL,
            "missing-catalog.json",
        )
        .arg("python")
        .arg("-c")
        .arg(base_prefix)
        .assert()
        .success();

    Ok(())
}

#[tokio::test]
#[cfg(feature = "test-python-managed")]
async fn python_build_variant_catalog_unavailable() -> anyhow::Result<()> {
    let (context, stock, mut downloads) = python_build_variant_catalog_context()?;
    for download in downloads.values_mut() {
        download["default"] = serde_json::json!(download["build_variant"] == "custom+pgo+lto");
    }
    let server = MockServer::start().await;
    mount_python_build_variant_catalog(&server, downloads, "max-age=86400").await;
    let metadata_url = format!("{}/metadata", server.uri());
    let find = |request| {
        let mut command = context.python_find();
        command
            .arg(request)
            .arg("--python-downloads-json-url")
            .arg(&metadata_url);
        command
    };

    let base_prefix = "import os, sys; print(os.path.realpath(sys.base_prefix))";
    let run = || {
        let mut command = context.run();
        command
            .arg("--python")
            .arg("3.13")
            .env_remove(EnvVars::VIRTUAL_ENV)
            .env(EnvVars::UV_PYTHON_DOWNLOADS_JSON_URL, &metadata_url);
        command
    };

    // Populate the cache with the custom default before the remote becomes unavailable.
    find("3.13").assert().success();

    let project = python_build_variant_project(&context)?;
    context
        .sync()
        .current_dir(&project)
        .arg("--python")
        .arg(stock.executable(false))
        .env(EnvVars::UV_PYTHON_DOWNLOADS_JSON_URL, &metadata_url)
        .assert()
        .success();
    let project_run = |request| {
        let mut command = context.run();
        command
            .current_dir(&project)
            .env_remove(EnvVars::VIRTUAL_ENV)
            .arg("--python")
            .arg(request)
            .env(EnvVars::UV_PYTHON_DOWNLOADS_JSON_URL, &metadata_url);
        command
    };

    // An unavailable remote is not contacted when the cached catalog has a matching installation.
    server.reset().await;
    Mock::given(method("GET"))
        .and(path("/metadata"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    uv_snapshot!(context.filters(), project_run("3.13").env(EnvVars::UV_HTTP_RETRIES, "0")
        .arg("python").arg("-c").arg(base_prefix), @"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13.[LATEST]+custom+pgo+lto-[PLATFORM]

    ----- stderr -----
    Using CPython 3.13.[LATEST]
    Removed virtual environment at: .venv
    Creating virtual environment at: .venv
    Resolved 1 package in [TIME]
    Checked in [TIME]
    ");
    project.child(".venv/custom-marker").touch()?;
    uv_snapshot!(context.filters(), project_run("3.13").arg("--offline")
        .arg("python").arg("-c").arg(base_prefix), @"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13.[LATEST]+custom+pgo+lto-[PLATFORM]

    ----- stderr -----
    Resolved 1 package in [TIME]
    Checked in [TIME]
    ");
    project
        .child(".venv/custom-marker")
        .assert(predicate::path::exists());
    uv_snapshot!(context.filters(), find("3.13").env(EnvVars::UV_HTTP_RETRIES, "0"), @"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13.[LATEST]+custom+pgo+lto-[PLATFORM]/[INSTALL-BIN]/[PYTHON]
    ");
    uv_snapshot!(context.filters(), find("3.13+lto+custom+pgo").env(EnvVars::UV_HTTP_RETRIES, "0"), @"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13.[LATEST]+custom+pgo+lto-[PLATFORM]/[INSTALL-BIN]/[PYTHON]
    ");
    assert!(
        server
            .received_requests()
            .await
            .context("Missing request log")?
            .is_empty()
    );
    uv_snapshot!(context.filters(), run().arg("--offline")
        .arg("python").arg("-c").arg(base_prefix), @"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13.[LATEST]+custom+pgo+lto-[PLATFORM]
    ");
    assert!(
        server
            .received_requests()
            .await
            .context("Missing request log")?
            .is_empty()
    );
    // A requested refresh still falls back to the cached catalog when the server is unavailable.
    uv_snapshot!(context.filters(), run().arg("--refresh").env(EnvVars::UV_HTTP_RETRIES, "0")
        .arg("python").arg("-c").arg(base_prefix), @"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13.[LATEST]+custom+pgo+lto-[PLATFORM]
    ");
    assert_eq!(
        server
            .received_requests()
            .await
            .context("Missing request log")?
            .len(),
        1
    );

    Ok(())
}

#[tokio::test]
#[cfg(feature = "test-python-managed")]
async fn python_build_variant_catalog_refresh_on_miss() -> anyhow::Result<()> {
    let (context, stock, mut downloads) = python_build_variant_catalog_context()?;
    for download in downloads.values_mut() {
        download["default"] = serde_json::json!(download["build_variant"] == "custom+pgo+lto");
    }
    let server = MockServer::start().await;
    mount_python_build_variant_catalog(&server, downloads.clone(), "max-age=86400").await;
    let metadata_url = format!("{}/metadata", server.uri());
    let find = |request| {
        let mut command = context.python_find();
        command
            .arg(request)
            .arg("--python-downloads-json-url")
            .arg(&metadata_url);
        command
    };

    find("3.13").assert().success();

    // A missing cached candidate refreshes the catalog and retries installed-interpreter discovery.
    server.reset().await;
    for download in downloads.values_mut() {
        download["default"] = serde_json::json!(download["build_variant"] == "pgo+lto");
    }
    mount_python_build_variant_catalog(&server, downloads, "max-age=86400").await;
    let managed_dir = context.temp_dir.child("managed");
    let stock_name = stock.key().to_string();
    let platform = platform_key_from_env()?;
    let version = stock_name
        .strip_suffix(&format!("-{platform}"))
        .context("Missing platform suffix")?;
    let custom_path = managed_dir.join(format!("{version}+custom+pgo+lto-{platform}"));
    let hidden_custom_path = context.temp_dir.join("custom-optimized");
    fs_err::rename(&custom_path, &hidden_custom_path)?;
    uv_snapshot!(context.filters(), find("3.13"), @"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13.[LATEST]+pgo+lto-[PLATFORM]/[INSTALL-BIN]/[PYTHON]
    ");
    assert_eq!(
        server
            .received_requests()
            .await
            .context("Missing request log")?
            .len(),
        1
    );

    Ok(())
}

#[test]
fn python_uninstall_build_variant() -> anyhow::Result<()> {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_managed_python_dirs();
    let platform = platform_key_from_env()?;
    let custom_key = format!("cpython-3.13.7+custom-{platform}");
    let optimized_key = format!("cpython-3.13.7+custom+pgo+lto-{platform}");
    let reordered_key = format!("cpython-3.13.7+lto+custom+pgo-{platform}");
    let managed_dir = context.temp_dir.child("managed");
    let custom = managed_dir.child(&custom_key);
    let optimized = managed_dir.child(&optimized_key);
    let reordered = managed_dir.child(&reordered_key);
    custom.create_dir_all()?;
    optimized.create_dir_all()?;
    reordered.create_dir_all()?;

    // A full key removes only that build, even when another build has additional tags.
    uv_snapshot!(context.filters(), context.python_uninstall().arg(&custom_key), @"
    exit_code: 0 (success)
    ----- stderr -----
    Searching for Python versions matching: cpython-3.13.7+custom-[PLATFORM]
    Uninstalled Python 3.13.7 in [TIME]
     - cpython-3.13.7+custom-[PLATFORM]
    ");
    custom.assert(predicate::path::missing());
    optimized.assert(predicate::path::exists());
    reordered.assert(predicate::path::exists());

    // Tag order remains part of the full installation identity.
    uv_snapshot!(context.filters(), context.python_uninstall().arg(&optimized_key), @"
    exit_code: 0 (success)
    ----- stderr -----
    Searching for Python versions matching: cpython-3.13.7+custom+pgo+lto-[PLATFORM]
    Uninstalled Python 3.13.7 in [TIME]
     - cpython-3.13.7+custom+pgo+lto-[PLATFORM]
    ");
    optimized.assert(predicate::path::missing());
    reordered.assert(predicate::path::exists());

    // A request missing build tags must leave the composite build installed.
    uv_snapshot!(context.filters(), context.python_uninstall().arg("3.13+custom"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Searching for Python versions matching: Python 3.13+custom
    No existing installations found for: Python 3.13+custom
    No Python installations found matching the requests
    ");
    reordered.assert(predicate::path::exists());

    // Version requests match the same build tags in any order.
    uv_snapshot!(context.filters(), context.python_uninstall().arg("3.13+custom+pgo+lto"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Searching for Python versions matching: Python 3.13+custom+pgo+lto
    Uninstalled Python 3.13.7 in [TIME]
     - cpython-3.13.7+lto+custom+pgo-[PLATFORM]
    ");
    reordered.assert(predicate::path::missing());

    Ok(())
}

#[test]
fn python_uninstall_prerelease_build_variant() -> anyhow::Result<()> {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_managed_python_dirs();
    let platform = platform_key_from_env()?;
    let stock_key = format!("cpython-3.14.0rc1-{platform}");
    let custom_key = format!("cpython-3.14.0rc1+custom-{platform}");
    let optimized_key = format!("cpython-3.14.0rc1+custom+pgo+lto-{platform}");
    let managed_dir = context.temp_dir.child("managed");
    let stock = managed_dir.child(&stock_key);
    let custom = managed_dir.child(&custom_key);
    let optimized = managed_dir.child(&optimized_key);
    stock.create_dir_all()?;
    custom.create_dir_all()?;
    optimized.create_dir_all()?;

    // Normalizing the zero patch must not broaden a full key to include other builds.
    uv_snapshot!(context.filters(), context.python_uninstall().arg(&stock_key), @"
    exit_code: 0 (success)
    ----- stderr -----
    Searching for Python versions matching: cpython-3.14rc1-[PLATFORM]
    Uninstalled Python 3.14.0rc1 in [TIME]
     - cpython-3.14.0rc1-[PLATFORM]
    ");
    stock.assert(predicate::path::missing());
    custom.assert(predicate::path::exists());
    optimized.assert(predicate::path::exists());

    // The normalized spelling also names an exact build when used in a full key.
    uv_snapshot!(context.filters(), context.python_uninstall().arg(format!("cpython-3.14rc1+custom-{platform}")), @"
    exit_code: 0 (success)
    ----- stderr -----
    Searching for Python versions matching: cpython-3.14rc1+custom-[PLATFORM]
    Uninstalled Python 3.14.0rc1 in [TIME]
     - cpython-3.14.0rc1+custom-[PLATFORM]
    ");
    custom.assert(predicate::path::missing());
    optimized.assert(predicate::path::exists());

    // Prerelease version requests also match the same build tags in any order.
    uv_snapshot!(context.filters(), context.python_uninstall().arg("3.14rc1+lto+pgo+custom"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Searching for Python versions matching: Python 3.14rc1+lto+pgo+custom
    Uninstalled Python 3.14.0rc1 in [TIME]
     - cpython-3.14.0rc1+custom+pgo+lto-[PLATFORM]
    ");
    optimized.assert(predicate::path::missing());

    Ok(())
}

#[tokio::test]
async fn python_reinstall_build_variant() -> anyhow::Result<()> {
    for target in [Some("3.13+custom+pgo"), Some("3.13+pgo+custom"), None] {
        let context = uv_test::test_context_with_versions!(&[])
            .with_managed_python_dirs()
            .with_http_retries("0");
        let platform = platform_key_from_env()?;
        let installed_key = format!("cpython-3.13.7+custom+pgo-{platform}");
        let key = installed_key.parse::<PythonInstallationKey>()?;
        context
            .temp_dir
            .child("managed")
            .child(&installed_key)
            .create_dir_all()?;

        let server = MockServer::start().await;
        let entry = |build_variant: &str, archive: &str| {
            serde_json::json!({
                "name": "cpython",
                "arch": { "family": key.arch().family().to_string(), "variant": null },
                "os": key.os().to_string(),
                "libc": key.libc().to_string(),
                "major": 3,
                "minor": 13,
                "patch": 7,
                "prerelease": "",
                "url": format!("{}/{archive}", server.uri()),
                "sha256": null,
                "variant": null,
                "build_variant": build_variant,
                "default": false,
                "build": null
            })
        };
        let metadata = serde_json::json!({
            "version": 1,
            "downloads": {
                (installed_key): entry("custom+pgo", "custom-pgo.tar.gz"),
                (format!("cpython-3.13.7+custom+lto+pgo-{platform}")):
                    entry("custom+lto+pgo", "custom-lto-pgo.tar.gz")
            }
        });
        Mock::given(method("GET"))
            .and(path("/metadata"))
            .respond_with(ResponseTemplate::new(200).set_body_json(metadata))
            .mount(&server)
            .await;

        // Archives return 404 so we can check which build was selected without downloading Python.
        context
            .python_install()
            .arg("--reinstall")
            .args(target)
            .arg("--python-downloads-json-url")
            .arg(format!("{}/metadata", server.uri()))
            .assert()
            .failure();

        let requests = server
            .received_requests()
            .await
            .expect("Request recording is enabled");
        allow_duplicates! {
            insta::assert_debug_snapshot!(
                requests.iter().map(|request| request.url.path()).collect::<Vec<_>>(), @r#"
            [
                "/metadata",
                "/custom-pgo.tar.gz",
            ]
            "#);
        }
    }

    Ok(())
}

#[tokio::test]
async fn python_reinstall_exact_key() -> anyhow::Result<()> {
    for exact in [true, false] {
        let context = uv_test::test_context_with_versions!(&[])
            .with_managed_python_dirs()
            .with_http_retries("0");
        let platform = platform_key_from_env()?;
        let stock_key = format!("cpython-3.13.7-{platform}");
        let custom_key = format!("cpython-3.13.7+custom-{platform}");
        let key = stock_key.parse::<PythonInstallationKey>()?;
        for installed_key in [&stock_key, &custom_key] {
            context
                .temp_dir
                .child("managed")
                .child(installed_key)
                .create_dir_all()?;
        }

        let server = MockServer::start().await;
        let entry = |build_variant: Option<&str>, archive: &str| {
            serde_json::json!({
                "name": "cpython",
                "arch": { "family": key.arch().family().to_string(), "variant": null },
                "os": key.os().to_string(),
                "libc": key.libc().to_string(),
                "major": 3,
                "minor": 13,
                "patch": 7,
                "prerelease": "",
                "url": format!("{}/{archive}", server.uri()),
                "sha256": null,
                "variant": null,
                "build_variant": build_variant,
                "default": build_variant.is_none(),
                "build": null
            })
        };
        let metadata = serde_json::json!({
            "version": 1,
            "downloads": {
                (stock_key.clone()): entry(None, "stock.tar.gz"),
                (custom_key): entry(Some("custom"), "custom.tar.gz")
            }
        });
        Mock::given(method("GET"))
            .and(path("/metadata"))
            .respond_with(ResponseTemplate::new(200).set_body_json(metadata))
            .mount(&server)
            .await;

        // Archives return 404 so the request log identifies every selected build.
        context
            .python_install()
            .arg("--reinstall")
            .arg(if exact { stock_key.as_str() } else { "3.13.7" })
            .arg("--python-downloads-json-url")
            .arg(format!("{}/metadata", server.uri()))
            .assert()
            .failure();

        let requests = server
            .received_requests()
            .await
            .context("Missing request log")?;
        let mut request_paths: Vec<_> = requests.iter().map(|request| request.url.path()).collect();
        request_paths.sort_unstable();
        if exact {
            // A full stock key must not select the custom installation.
            insta::assert_debug_snapshot!(request_paths, @r#"
            [
                "/metadata",
                "/stock.tar.gz",
            ]
            "#);
        } else {
            // Filling platform fields must not turn a version request into an exact key.
            insta::assert_debug_snapshot!(request_paths, @r#"
            [
                "/custom.tar.gz",
                "/metadata",
                "/stock.tar.gz",
            ]
            "#);
        }
    }

    Ok(())
}

#[test]
fn python_reinstall_missing_build_variant() -> anyhow::Result<()> {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs();

    context.python_install().arg("3.13.7").assert().success();

    let platform = platform_key_from_env()?;
    let stock_key = format!("cpython-3.13.7-{platform}");
    let stock = context.temp_dir.child("managed").child(&stock_key);
    let stock_marker = stock.child("marker");
    stock_marker.touch()?;

    // This installed build is absent from the download catalog.
    let custom = context
        .temp_dir
        .child("managed")
        .child(format!("cpython-3.13.7+custom-{platform}"));
    custom.create_dir_all()?;
    let custom_marker = custom.child("marker");
    custom_marker.touch()?;

    // Skip the unavailable build and reinstall the available build.
    uv_snapshot!(context.filters(), context.python_install().arg("--reinstall"), @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: Failed to create reinstall request for existing installation `cpython-3.13.7+custom-[PLATFORM]`: No download found for request: cpython-3.13.7+custom-[PLATFORM]
    Installed Python 3.13.7 in [TIME]
     ~ cpython-3.13.7-[PLATFORM] (python3.13)
    ");
    stock.assert(predicate::path::exists());
    stock_marker.assert(predicate::path::missing());
    custom_marker.assert(predicate::path::exists());

    // An explicit request for the unavailable build must still fail.
    uv_snapshot!(context.filters(), context.python_install().arg("--reinstall").arg("3.13.7+custom"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: No download found for request: cpython-3.13.7+custom-[PLATFORM]
    ");
    custom_marker.assert(predicate::path::exists());

    context
        .python_uninstall()
        .arg(&stock_key)
        .assert()
        .success();

    // If every installed build is unavailable, warn and leave them installed.
    uv_snapshot!(context.filters(), context.python_install().arg("--reinstall"), @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: Failed to create reinstall request for existing installation `cpython-3.13.7+custom-[PLATFORM]`: No download found for request: cpython-3.13.7+custom-[PLATFORM]
    ");
    custom_marker.assert(predicate::path::exists());

    Ok(())
}

#[test]
fn python_reinstall() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_filtered_latest_python_versions()
        .with_managed_python_dirs();

    // Install a couple versions
    uv_snapshot!(context.filters(), context.python_install().arg("3.12").arg("3.13"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed 2 versions in [TIME]
     + cpython-3.12.[LATEST]-[PLATFORM] (python3.12)
     + cpython-3.13.[LATEST]-[PLATFORM] (python3.13)
    ");

    // Reinstall a single version
    uv_snapshot!(context.filters(), context.python_install().arg("3.13").arg("--reinstall"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.13.[LATEST] in [TIME]
     ~ cpython-3.13.[LATEST]-[PLATFORM] (python3.13)
    ");

    // Reinstall multiple versions
    uv_snapshot!(context.filters(), context.python_install().arg("--reinstall"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed 2 versions in [TIME]
     ~ cpython-3.12.[LATEST]-[PLATFORM] (python3.12)
     ~ cpython-3.13.[LATEST]-[PLATFORM] (python3.13)
    ");

    // Reinstalling a version that is not installed should also work
    uv_snapshot!(context.filters(), context.python_install().arg("3.11").arg("--reinstall"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.11.[LATEST] in [TIME]
     + cpython-3.11.[LATEST]-[PLATFORM] (python3.11)
    ");
}

#[test]
fn python_reinstall_patch() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs()
        .with_filtered_latest_python_versions();

    // Install a couple patch versions
    uv_snapshot!(context.filters(), context.python_install().arg("3.12.6").arg("3.12.7"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed 2 versions in [TIME]
     + cpython-3.12.6-[PLATFORM]
     + cpython-3.12.7-[PLATFORM] (python3.12)
    ");

    // Reinstall all "3.12" versions.
    uv_snapshot!(context.filters(), context.python_install().arg("3.12").arg("--reinstall"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed 2 versions in [TIME]
     ~ cpython-3.12.6-[PLATFORM]
     ~ cpython-3.12.7-[PLATFORM] (python3.12)
    ");
}

#[test]
fn python_install_automatic() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_filtered_python_sources()
        .with_managed_python_dirs();

    // With downloads disabled, the automatic install should fail
    uv_snapshot!(context.filters(), context.run()
        .env_remove(EnvVars::VIRTUAL_ENV)
        .arg("--no-python-downloads")
        .arg("python").arg("-c").arg("import sys; print(sys.version_info[:2])"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: No interpreter found in [PYTHON SOURCES]

    hint: A managed Python download is available, but Python downloads are set to 'never'
    ");

    // Otherwise, we should fetch the latest Python version
    uv_snapshot!(context.filters(), context.run()
        .env_remove(EnvVars::VIRTUAL_ENV)
        .arg("python").arg("-c").arg("import sys; print(sys.version_info[:2])"), @"
    exit_code: 0 (success)
    ----- stdout -----
    (3, 14)
    ");

    // Subsequently, we can use the interpreter even with downloads disabled
    uv_snapshot!(context.filters(), context.run()
        .env_remove(EnvVars::VIRTUAL_ENV)
        .arg("--no-python-downloads")
        .arg("python").arg("-c").arg("import sys; print(sys.version_info[:2])"), @"
    exit_code: 0 (success)
    ----- stdout -----
    (3, 14)
    ");

    // We should respect the Python request
    uv_snapshot!(context.filters(), context.run()
    .env_remove(EnvVars::VIRTUAL_ENV)
    .arg("-p").arg("3.12")
    .arg("python").arg("-c").arg("import sys; print(sys.version_info[:2])"), @"
    exit_code: 0 (success)
    ----- stdout -----
    (3, 12)
    ");

    // But some requests cannot be mapped to a download
    uv_snapshot!(context.filters(), context.run()
       .env_remove(EnvVars::VIRTUAL_ENV)
       .arg("-p").arg("foobar")
       .arg("python").arg("-c").arg("import sys; print(sys.version_info[:2])"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: No interpreter found for executable name `foobar` in [PYTHON SOURCES]
    ");

    // Create a "broken" Python executable in the test context `bin`
    // (the snapshot is different on Windows so we just test on Unix)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let contents = r"#!/bin/sh
        echo 'error: intentionally broken python executable' >&2
        exit 1";
        let python = context
            .bin_dir
            .join(format!("python3{}", std::env::consts::EXE_SUFFIX));
        fs_err::write(&python, contents).unwrap();

        let mut perms = fs_err::metadata(&python).unwrap().permissions();
        perms.set_mode(0o755);
        fs_err::set_permissions(&python, perms).unwrap();

        // We should ignore the broken executable and download a version still
        uv_snapshot!(context.filters(), context.run()
            .env_remove(EnvVars::VIRTUAL_ENV)
            // In tests, we ignore `PATH` during Python discovery so we need to add the context `bin`
            .env(EnvVars::UV_PYTHON_SEARCH_PATH, context.bin_dir.as_os_str())
            .arg("-p").arg("3.11")
            .arg("python").arg("-c").arg("import sys; print(sys.version_info[:2])"), @"
        exit_code: 0 (success)
        ----- stdout -----
        (3, 11)
        ");
    }
}

/// Regression test for a bad cpython runtime
/// <https://github.com/astral-sh/uv/issues/13610>
#[test]
fn regression_cpython() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_filtered_python_sources()
        .with_managed_python_dirs();

    let init = context.temp_dir.child("mre.py");
    init.write_str(indoc! { r#"
        class Foo(str): ...

        a = []
        new_value = Foo("1")
        a += new_value
        "#
    })
    .unwrap();

    // We should respect the Python request
    uv_snapshot!(context.filters(), context.run()
        .env_remove(EnvVars::VIRTUAL_ENV)
        .arg("-p").arg("3.12")
        .arg("mre.py"), @"
    exit_code: 0 (success)
    ");
}

#[test]
fn python_install_force() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_filtered_latest_python_versions()
        .with_managed_python_dirs();

    // Install the latest version
    uv_snapshot!(context.filters(), context.python_install(), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.14.[LATEST] in [TIME]
     + cpython-3.14.[LATEST]-[PLATFORM] (python3.14)
    ");

    let bin_python = context
        .bin_dir
        .child(format!("python3.14{}", std::env::consts::EXE_SUFFIX));

    // You can force replacement of the executables
    uv_snapshot!(context.filters(), context.python_install().arg("--force"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.14.[LATEST] in [TIME]
     + cpython-3.14.[LATEST]-[PLATFORM] (python3.14)
    ");

    // The executable should still be present in the bin directory
    bin_python.assert(predicate::path::exists());

    // If an unmanaged executable is present, `--force` is required
    fs_err::remove_file(bin_python.path()).unwrap();
    bin_python.touch().unwrap();

    uv_snapshot!(context.filters(), context.python_install().arg("3.14"), @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: Failed to install executable for cpython-3.14.[LATEST]-[PLATFORM]
      cause: Executable already exists at `[BIN]/python3.14` but is not managed by uv; use `--force` to replace it
    ");

    uv_snapshot!(context.filters(), context.python_install().arg("--force").arg("3.14"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.14.[LATEST] in [TIME]
     + cpython-3.14.[LATEST]-[PLATFORM] (python3.14)
    ");

    bin_python.assert(predicate::path::exists());
}

#[test]
fn python_install_minor() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_filtered_latest_python_versions()
        .with_managed_python_dirs();

    // Install a minor version
    uv_snapshot!(context.filters(), context.python_install().arg("3.11"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.11.[LATEST] in [TIME]
     + cpython-3.11.[LATEST]-[PLATFORM] (python3.11)
    ");

    let bin_python = context
        .bin_dir
        .child(format!("python3.11{}", std::env::consts::EXE_SUFFIX));

    // The executable should be installed in the bin directory
    bin_python.assert(predicate::path::exists());

    // It should be a link to the minor version
    if cfg!(unix) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python), @"[TEMP_DIR]/managed/cpython-3.11-[PLATFORM]/bin/python3.11"
            );
        });
    } else if cfg!(windows) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python), @"[TEMP_DIR]/managed/cpython-3.11-[PLATFORM]/python"
            );
        });
    }

    uv_snapshot!(context.filters(), context.python_uninstall().arg("3.11"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Searching for Python versions matching: Python 3.11
    Uninstalled Python 3.11.[LATEST] in [TIME]
     - cpython-3.11.[LATEST]-[PLATFORM] (python3.11)
    ");

    // The executable should be removed
    bin_python.assert(predicate::path::missing());
}

#[test]
fn python_install_multiple_patch() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs();

    // Install multiple patch versions
    uv_snapshot!(context.filters(), context.python_install().arg("3.12.8").arg("3.12.6"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed 2 versions in [TIME]
     + cpython-3.12.6-[PLATFORM]
     + cpython-3.12.8-[PLATFORM] (python3.12)
    ");

    let bin_python = context
        .bin_dir
        .child(format!("python3.12{}", std::env::consts::EXE_SUFFIX));

    // The executable should be installed in the bin directory
    bin_python.assert(predicate::path::exists());

    // The link should resolve to the newer patch version
    if cfg!(unix) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python), @"[TEMP_DIR]/managed/cpython-3.12.8-[PLATFORM]/bin/python3.12"
            );
        });
    } else if cfg!(windows) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python), @"[TEMP_DIR]/managed/cpython-3.12.8-[PLATFORM]/python"
            );
        });
    }

    uv_snapshot!(context.filters(), context.python_uninstall().arg("3.12.8"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Searching for Python versions matching: Python 3.12.8
    Uninstalled Python 3.12.8 in [TIME]
     - cpython-3.12.8-[PLATFORM] (python3.12)
    ");

    // TODO(zanieb): This behavior is not implemented yet
    // // The executable should be installed in the bin directory
    // bin_python.assert(predicate::path::exists());

    // // When the version is removed, the link should point to the other patch version
    // if cfg!(unix) {
    //     insta::with_settings!({
    //         filters => context.filters(),
    //     }, {
    //         insta::assert_snapshot!(
    //             canonicalize_link_path(&bin_python), @"[TEMP_DIR]/managed/cpython-3.12.6-[PLATFORM]/bin/python3.12"
    //         );
    //     });
    // } else if cfg!(windows) {
    //     insta::with_settings!({
    //         filters => context.filters(),
    //     }, {
    //         insta::assert_snapshot!(
    //             canonicalize_link_path(&bin_python), @"[TEMP_DIR]/managed/cpython-3.12.6-[PLATFORM]/python"
    //         );
    //     });
    // }
}

#[test]
fn python_install_preview() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_filtered_latest_python_versions()
        .with_managed_python_dirs();

    // Install the latest version
    uv_snapshot!(context.filters(), context.python_install().arg("--preview"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.14.[LATEST] in [TIME]
     + cpython-3.14.[LATEST]-[PLATFORM] (python, python3, python3.14)
    ");

    let bin_python = context
        .bin_dir
        .child(format!("python3.14{}", std::env::consts::EXE_SUFFIX));

    // The executable should be installed in the bin directory
    bin_python.assert(predicate::path::exists());

    // On Unix, it should be a link
    #[cfg(unix)]
    bin_python.assert(predicate::path::is_symlink());

    // The link should be to a path containing a minor version symlink directory
    if cfg!(unix) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python), @"[TEMP_DIR]/managed/cpython-3.14-[PLATFORM]/bin/python3.14"
            );
        });
    } else if cfg!(windows) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python), @"[TEMP_DIR]/managed/cpython-3.14-[PLATFORM]/python"
            );
        });
    }

    // The executable should "work"
    uv_snapshot!(context.filters(), Command::new(bin_python.as_os_str())
        .arg("-c").arg("import subprocess; print('hello world')"), @"
    exit_code: 0 (success)
    ----- stdout -----
    hello world
    ");

    // Should be a no-op when already installed
    uv_snapshot!(context.filters(), context.python_install().arg("--preview"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Python is already installed. Use `uv python install <request>` to install another version.
    ");

    // You can opt-in to a reinstall
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("--reinstall"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.14.[LATEST] in [TIME]
     ~ cpython-3.14.[LATEST]-[PLATFORM] (python, python3, python3.14)
    ");

    // The executable should still be present in the bin directory
    bin_python.assert(predicate::path::exists());

    // You can also force replacement of the executables
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("--force"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.14.[LATEST] in [TIME]
     + cpython-3.14.[LATEST]-[PLATFORM] (python, python3, python3.14)
    ");

    // The executable should still be present in the bin directory
    bin_python.assert(predicate::path::exists());

    // If an unmanaged executable is present, `--force` is required
    fs_err::remove_file(bin_python.path()).unwrap();
    bin_python.touch().unwrap();

    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.14"), @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: Failed to install executable for cpython-3.14.[LATEST]-[PLATFORM]
      cause: Executable already exists at `[BIN]/python3.14` but is not managed by uv; use `--force` to replace it
    ");

    // With `--bin`, this should error instead of warn
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("--bin").arg("3.14"), @"
    exit_code: 1 (failure)
    ----- stderr -----
    error: Failed to install executable for cpython-3.14.[LATEST]-[PLATFORM]
      cause: Executable already exists at `[BIN]/python3.14` but is not managed by uv; use `--force` to replace it
    ");
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.14").env(EnvVars::UV_PYTHON_INSTALL_BIN, "1"), @"
    exit_code: 1 (failure)
    ----- stderr -----
    error: Failed to install executable for cpython-3.14.[LATEST]-[PLATFORM]
      cause: Executable already exists at `[BIN]/python3.14` but is not managed by uv; use `--force` to replace it
    ");

    // With `--no-bin`, this should be silent
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("--no-bin").arg("3.14"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Python 3.14 is already installed
    ");
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.14").env(EnvVars::UV_PYTHON_INSTALL_BIN, "0"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Python 3.14 is already installed
    ");

    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("--force").arg("3.14"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.14.[LATEST] in [TIME]
     + cpython-3.14.[LATEST]-[PLATFORM] (python3.14)
    ");

    bin_python.assert(predicate::path::exists());

    // On Unix, it should be a link
    #[cfg(unix)]
    bin_python.assert(predicate::path::is_symlink());

    // Uninstallation requires an argument
    uv_snapshot!(context.filters(), context.python_uninstall(), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: the following required arguments were not provided:
      <TARGETS>...

    Usage: uv python uninstall --cache-dir [CACHE_DIR] --install-dir <INSTALL_DIR> <TARGETS>...

    For more information, try '--help'.
    ");

    uv_snapshot!(context.filters(), context.python_uninstall().arg("3.14"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Searching for Python versions matching: Python 3.14
    Uninstalled Python 3.14.[LATEST] in [TIME]
     - cpython-3.14.[LATEST]-[PLATFORM] (python, python3, python3.14)
    ");

    // The executable should be removed
    bin_python.assert(predicate::path::missing());

    // Install a minor version
    uv_snapshot!(context.filters(), context.python_install().arg("3.11").arg("--preview"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.11.[LATEST] in [TIME]
     + cpython-3.11.[LATEST]-[PLATFORM] (python3.11)
    ");

    let bin_python = context
        .bin_dir
        .child(format!("python3.11{}", std::env::consts::EXE_SUFFIX));

    // The link should be to a path containing a minor version symlink directory
    if cfg!(unix) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python), @"[TEMP_DIR]/managed/cpython-3.11-[PLATFORM]/bin/python3.11"
            );
        });
    } else if cfg!(windows) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python), @"[TEMP_DIR]/managed/cpython-3.11-[PLATFORM]/python"
            );
        });
    }

    uv_snapshot!(context.filters(), context.python_uninstall().arg("3.11"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Searching for Python versions matching: Python 3.11
    Uninstalled Python 3.11.[LATEST] in [TIME]
     - cpython-3.11.[LATEST]-[PLATFORM] (python3.11)
    ");

    // Install multiple patch versions
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.12.8").arg("3.12.6"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed 2 versions in [TIME]
     + cpython-3.12.6-[PLATFORM]
     + cpython-3.12.8-[PLATFORM] (python3.12)
    ");

    let bin_python = context
        .bin_dir
        .child(format!("python3.12{}", std::env::consts::EXE_SUFFIX));

    // The link should resolve to the newer patch version
    if cfg!(unix) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python), @"[TEMP_DIR]/managed/cpython-3.12.8-[PLATFORM]/bin/python3.12"
            );
        });
    } else if cfg!(windows) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python), @"[TEMP_DIR]/managed/cpython-3.12.8-[PLATFORM]/python"
            );
        });
    }
}

/// Multiple pre-existing, unmanaged executables should be reported as a single grouped error,
/// rather than one near-identical error per executable.
///
/// See: <https://github.com/astral-sh/uv/issues/9707>
#[test]
fn python_install_multiple_unmanaged_executables() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_filtered_latest_python_versions()
        .with_managed_python_dirs();

    // Install a version with the default `python`, `python3`, and `python3.13` executables.
    uv_snapshot!(context.filters(), context.python_install().arg("--default").arg("--preview-features").arg("python-install-default").arg("3.13"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.13.[LATEST] in [TIME]
     + cpython-3.13.[LATEST]-[PLATFORM] (python, python3, python3.13)
    ");

    // Replace each managed executable with an empty, unmanaged file.
    for name in ["python", "python3", "python3.13"] {
        let executable = context
            .bin_dir
            .child(format!("{name}{}", std::env::consts::EXE_SUFFIX));
        fs_err::remove_file(executable.path()).unwrap();
        executable.touch().unwrap();
    }

    // Re-installing without `--force` should report all three conflicts in a single grouped error.
    uv_snapshot!(context.filters(), context.python_install().arg("--default").arg("--preview-features").arg("python-install-default").arg("3.13"), @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: Failed to install executable for cpython-3.13.[LATEST]-[PLATFORM]
      cause: Executables `python3.13`, `python3`, and `python` already exist in `[BIN]/` but are not managed by uv; use `--force` to replace them
    ");

    // The unmanaged executables should be left untouched (still empty).
    for name in ["python", "python3", "python3.13"] {
        let executable = context
            .bin_dir
            .child(format!("{name}{}", std::env::consts::EXE_SUFFIX));
        assert_eq!(fs_err::read_to_string(executable.path()).unwrap(), "");
    }
}

#[test]
fn python_install_preview_no_bin() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_filtered_latest_python_versions()
        .with_managed_python_dirs();

    // Install the latest version
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("--no-bin"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.14.[LATEST] in [TIME]
     + cpython-3.14.[LATEST]-[PLATFORM]
    ");

    let bin_python = context
        .bin_dir
        .child(format!("python3.14{}", std::env::consts::EXE_SUFFIX));

    // The executable should not be installed in the bin directory
    bin_python.assert(predicate::path::missing());

    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("--no-bin").arg("--default"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: the argument '--no-bin' cannot be used with '--default'

    Usage: uv python install --cache-dir [CACHE_DIR] --no-bin --install-dir <INSTALL_DIR> [TARGETS]...

    For more information, try '--help'.
    ");

    let bin_python = context
        .bin_dir
        .child(format!("python{}", std::env::consts::EXE_SUFFIX));

    // The executable should not be installed in the bin directory
    bin_python.assert(predicate::path::missing());
}

#[test]
fn python_install_preview_upgrade() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs();

    let bin_python = context
        .bin_dir
        .child(format!("python3.12{}", std::env::consts::EXE_SUFFIX));

    // Install 3.12.5
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.12.5"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.12.5 in [TIME]
     + cpython-3.12.5-[PLATFORM] (python3.12)
    ");

    // Installing with a patch version should cause the link to be to the patch installation.
    if cfg!(unix) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python), @"[TEMP_DIR]/managed/cpython-3.12.5-[PLATFORM]/bin/python3.12"
            );
        });
    } else if cfg!(windows) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python), @"[TEMP_DIR]/managed/cpython-3.12.5-[PLATFORM]/python"
            );
        });
    }

    // Installing 3.12.4 should not replace the executable, but also shouldn't fail
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.12.4"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.12.4 in [TIME]
     + cpython-3.12.4-[PLATFORM]
    ");

    if cfg!(unix) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python), @"[TEMP_DIR]/managed/cpython-3.12.5-[PLATFORM]/bin/python3.12"
            );
        });
    } else if cfg!(windows) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python), @"[TEMP_DIR]/managed/cpython-3.12.5-[PLATFORM]/python"
            );
        });
    }

    // Using `--reinstall` is not sufficient to replace it either
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.12.4").arg("--reinstall"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.12.4 in [TIME]
     ~ cpython-3.12.4-[PLATFORM]
    ");

    if cfg!(unix) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python), @"[TEMP_DIR]/managed/cpython-3.12.5-[PLATFORM]/bin/python3.12"
            );
        });
    } else if cfg!(windows) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python), @"[TEMP_DIR]/managed/cpython-3.12.5-[PLATFORM]/python"
            );
        });
    }

    // But `--force` is
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.12.4").arg("--force"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.12.4 in [TIME]
     + cpython-3.12.4-[PLATFORM] (python3.12)
    ");

    if cfg!(unix) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python), @"[TEMP_DIR]/managed/cpython-3.12.4-[PLATFORM]/bin/python3.12"
            );
        });
    } else if cfg!(windows) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python), @"[TEMP_DIR]/managed/cpython-3.12.4-[PLATFORM]/python"
            );
        });
    }

    // But installing 3.12.6 should upgrade automatically
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.12.6"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.12.6 in [TIME]
     + cpython-3.12.6-[PLATFORM] (python3.12)
    ");

    if cfg!(unix) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python), @"[TEMP_DIR]/managed/cpython-3.12.6-[PLATFORM]/bin/python3.12"
            );
        });
    } else if cfg!(windows) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python), @"[TEMP_DIR]/managed/cpython-3.12.6-[PLATFORM]/python"
            );
        });
    }
}

#[test]
fn python_install_patch_after_minor_alias() -> anyhow::Result<()> {
    for remove_minor_link in [false, true] {
        let context = uv_test::test_context_with_versions!(&[])
            .with_filtered_python_keys()
            .with_filtered_exe_suffix()
            .with_managed_python_dirs();

        // `--default` creates aliases through the minor-version link, even for a patch request.
        context
            .python_install()
            .args(["3.12.8", "--default", "--preview"])
            .assert()
            .success();

        if remove_minor_link {
            let minor_link = context
                .temp_dir
                .child("managed")
                .child(format!("cpython-3.12-{}", platform_key_from_env()?));
            remove_symlink(minor_link.path())?;
        }

        // An explicit patch upgrade replaces the minor alias with a direct link to that patch.
        context.python_install().arg("3.12.9").assert().success();

        allow_duplicates! {
            uv_snapshot!(context.filters(), context.python_install().arg("3.12.9"), @"
            exit_code: 0 (success)
            ----- stderr -----
            Python 3.12.9 is already installed
            ");

            // Moving the minor-version link again must not change the patch alias.
            uv_snapshot!(context.filters(), context.python_install().args(["3.12.11", "--no-bin"]), @"
            exit_code: 0 (success)
            ----- stderr -----
            Installed Python 3.12.11 in [TIME]
             + cpython-3.12.11-[PLATFORM]
            ");

            let bin_python = context
                .bin_dir
                .child(format!("python3.12{}", std::env::consts::EXE_SUFFIX));
            uv_snapshot!(context.filters(), Command::new(bin_python.as_os_str()).arg("--version"), @"
            exit_code: 0 (success)
            ----- stdout -----
            Python 3.12.9
            ");
        }
    }

    Ok(())
}

#[test]
fn python_install_freethreaded() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_latest_python_versions()
        .with_managed_python_dirs()
        .with_filtered_python_install_bin()
        .with_filtered_python_names()
        .with_filtered_exe_suffix();

    // Install the latest version
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.13t"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.13.[LATEST] in [TIME]
     + cpython-3.13.[LATEST]+freethreaded-[PLATFORM] (python3.13t)
    ");

    let bin_python = context
        .bin_dir
        .child(format!("python3.13t{}", std::env::consts::EXE_SUFFIX));

    // The executable should be installed in the bin directory
    bin_python.assert(predicate::path::exists());

    // On Unix, it should be a link
    #[cfg(unix)]
    bin_python.assert(predicate::path::is_symlink());

    // The executable should "work"
    uv_snapshot!(context.filters(), Command::new(bin_python.as_os_str())
        .arg("-c").arg("import subprocess; print('hello world')"), @"
    exit_code: 0 (success)
    ----- stdout -----
    hello world
    ");

    // We should find it with opt-in
    uv_snapshot!(context.filters(), context.python_find().arg("3.13t"), @"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13+freethreaded-[PLATFORM]/[INSTALL-BIN]/[PYTHON]
    ");

    // We should be able to select it with `+freethreaded`
    uv_snapshot!(context.filters(), context.python_find().arg("3.13+freethreaded"), @"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13+freethreaded-[PLATFORM]/[INSTALL-BIN]/[PYTHON]
    ");

    // Create a virtual environment with the freethreaded Python
    uv_snapshot!(context.filters(), context.venv().arg("--python").arg("3.13t"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Using CPython 3.13.[LATEST]+freethreaded
    Creating virtual environment at: .venv
    Activate with: source .venv/[BIN]/activate
    ");

    // `python`, `python3`, `python3.13`, and `python3.13t` should all be present
    let scripts = context
        .venv
        .join(if cfg!(windows) { "Scripts" } else { "bin" });
    assert!(
        scripts
            .join(format!("python{}", std::env::consts::EXE_SUFFIX))
            .exists()
    );

    #[cfg(windows)]
    {
        let pythonw = scripts.join(format!("pythonw{}", std::env::consts::EXE_SUFFIX));
        assert!(pythonw.exists());

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&pythonw), @"[TEMP_DIR]/managed/cpython-3.13+freethreaded-[PLATFORM]/pythonw"
            );
        });
    }

    #[cfg(unix)]
    assert!(
        scripts
            .join(format!("python3{}", std::env::consts::EXE_SUFFIX))
            .exists()
    );

    #[cfg(unix)]
    assert!(
        scripts
            .join(format!("python3.13{}", std::env::consts::EXE_SUFFIX))
            .exists()
    );

    assert!(
        scripts
            .join(format!("python3.13t{}", std::env::consts::EXE_SUFFIX))
            .exists()
    );

    #[cfg(windows)]
    {
        let pythonw = scripts.join(format!("pythonw3.13t{}", std::env::consts::EXE_SUFFIX));
        assert!(pythonw.exists());

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&pythonw), @"[TEMP_DIR]/managed/cpython-3.13+freethreaded-[PLATFORM]/pythonw"
            );
        });
    }

    // Remove the virtual environment
    fs_err::remove_dir_all(&context.venv).unwrap();

    // Should be distinct from 3.13
    uv_snapshot!(context.filters(), context.python_install().arg("3.13"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.13.[LATEST] in [TIME]
     + cpython-3.13.[LATEST]-[PLATFORM] (python3.13)
    ");

    // Should not work with older Python versions
    uv_snapshot!(context.filters(), context.python_install().arg("3.12t"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: No download found for request: cpython-3.12+freethreaded-[PLATFORM]
    ");

    uv_snapshot!(context.filters(), context.python_uninstall().arg("--all"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Searching for Python installations
    Uninstalled 2 versions in [TIME]
     - cpython-3.13.[LATEST]+freethreaded-[PLATFORM] (python3.13t)
     - cpython-3.13.[LATEST]-[PLATFORM] (python3.13)
    ");
}

/// Test that installing both GIL and free-threaded variants of the same Python version
/// doesn't cause managed installation entries to disappear from `uv python list`
/// on Windows when registry discovery is enabled.
///
/// Regression test for <https://github.com/astral-sh/uv/issues/18795>.
///
/// IMPORTANT: this test writes to the shared `HKCU` registry. The trailing uninstall is
/// best-effort cleanup; panics will leak entries. These registry tests still share global state,
/// so adding more will probably require isolation.
#[cfg(all(windows, feature = "test-windows-registry"))]
#[test]
fn python_install_freethreaded_and_gil_list() {
    use assert_cmd::assert::OutputAssertExt;

    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_latest_python_versions()
        .with_managed_python_dirs()
        .with_filtered_python_install_bin()
        .with_filtered_python_names()
        .with_filtered_exe_suffix()
        .with_collapsed_whitespace();

    // Install both the GIL and free-threaded versions, with registry enabled
    context
        .python_install()
        .arg("3.13")
        .env_remove(EnvVars::UV_PYTHON_NO_REGISTRY)
        .env(EnvVars::UV_PYTHON_INSTALL_REGISTRY, "1")
        .assert()
        .success();
    context
        .python_install()
        .arg("--preview")
        .arg("3.13t")
        .env_remove(EnvVars::UV_PYTHON_NO_REGISTRY)
        .env(EnvVars::UV_PYTHON_INSTALL_REGISTRY, "1")
        .assert()
        .success();

    // List installed versions with registry discovery enabled.
    // We remove `UV_PYTHON_NO_REGISTRY` to opt back into registry discovery, and remove
    // `UV_PYTHON_SEARCH_PATH` so the test can discover the installed bin trampolines.
    //
    // Both the GIL and freethreaded variants should show entries from:
    // - The registry (patch-versioned managed directory path)
    // - The search path (bin trampoline)
    // - Managed discovery (minor-version junction path)
    uv_snapshot!(context.filters(), context.python_list()
        .arg("3.13")
        .arg("--only-installed")
        .arg("--managed-python")
        .env_remove(EnvVars::UV_PYTHON_NO_REGISTRY)
        .env_remove(EnvVars::UV_PYTHON_SEARCH_PATH)
        .env(EnvVars::UV_PYTHON_INSTALL_REGISTRY, "1"), @"
    exit_code: 0 (success)
    ----- stdout -----
    cpython-3.13.[LATEST]-[PLATFORM] managed/cpython-3.13.[LATEST]-[PLATFORM]/[INSTALL-BIN]/[PYTHON]
    cpython-3.13.[LATEST]-[PLATFORM] [BIN]/[INSTALL-BIN]/[PYTHON]
    cpython-3.13.[LATEST]-[PLATFORM] managed/cpython-3.13-[PLATFORM]/[INSTALL-BIN]/[PYTHON]
    ");

    uv_snapshot!(context.filters(), context.python_list()
        .arg("3.13t")
        .arg("--only-installed")
        .arg("--managed-python")
        .env_remove(EnvVars::UV_PYTHON_NO_REGISTRY)
        .env_remove(EnvVars::UV_PYTHON_SEARCH_PATH)
        .env(EnvVars::UV_PYTHON_INSTALL_REGISTRY, "1"), @"
    exit_code: 0 (success)
    ----- stdout -----
    cpython-3.13.[LATEST]+freethreaded-[PLATFORM] managed/cpython-3.13.[LATEST]+freethreaded-[PLATFORM]/[INSTALL-BIN]/[PYTHON]
    cpython-3.13.[LATEST]+freethreaded-[PLATFORM] [BIN]/python3.13t
    cpython-3.13.[LATEST]+freethreaded-[PLATFORM] managed/cpython-3.13+freethreaded-[PLATFORM]/[INSTALL-BIN]/[PYTHON]
    ");

    // Clean up registry entries
    context
        .python_uninstall()
        .arg("--all")
        .env(EnvVars::UV_PYTHON_INSTALL_REGISTRY, "1")
        .assert()
        .success();
}

#[cfg(all(windows, feature = "test-windows-registry"))]
#[test]
fn python_install_registry_takes_precedence_over_no_registry() {
    use assert_cmd::assert::OutputAssertExt;

    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_latest_python_versions()
        .with_managed_python_dirs()
        .with_filtered_python_install_bin()
        .with_filtered_python_names()
        .with_filtered_exe_suffix()
        .with_collapsed_whitespace();

    context
        .python_install()
        .arg("3.13")
        .env(EnvVars::UV_PYTHON_INSTALL_REGISTRY, "1")
        .env(EnvVars::UV_PYTHON_NO_REGISTRY, "1")
        .assert()
        .success();

    // `UV_PYTHON_INSTALL_REGISTRY` should take precedence over `UV_PYTHON_NO_REGISTRY`.
    // When we re-enable registry discovery for this command and clear the search path, we should
    // see both the registry entry and the managed installation entry.
    uv_snapshot!(context.filters(), context.python_list()
        .arg("3.13")
        .arg("--only-installed")
        .arg("--managed-python")
        .env_remove(EnvVars::UV_PYTHON_NO_REGISTRY)
        .env(EnvVars::UV_PYTHON_SEARCH_PATH, "")
        .env(EnvVars::UV_PYTHON_INSTALL_REGISTRY, "1"), @"
    exit_code: 0 (success)
    ----- stdout -----
    cpython-3.13.[LATEST]-[PLATFORM] managed/cpython-3.13.[LATEST]-[PLATFORM]/[INSTALL-BIN]/[PYTHON]
    cpython-3.13.[LATEST]-[PLATFORM] managed/cpython-3.13-[PLATFORM]/[INSTALL-BIN]/[PYTHON]
    ");

    context
        .python_uninstall()
        .arg("--all")
        .env(EnvVars::UV_PYTHON_INSTALL_REGISTRY, "1")
        .assert()
        .success();
}

#[test]
fn python_upgrade_not_allowed() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs();

    // Request a patch upgrade
    uv_snapshot!(context.filters(), context.python_upgrade().arg("--preview").arg("3.13.0"), @"
    exit_code: 1 (failure)
    ----- stderr -----
    error: `uv python upgrade` only accepts minor versions, got: 3.13.0
    ");

    // Request a pre-release upgrade
    uv_snapshot!(context.filters(), context.python_upgrade().arg("--preview").arg("3.14rc3"), @"
    exit_code: 1 (failure)
    ----- stderr -----
    error: `uv python upgrade` only accepts minor versions, got: 3.14rc3
    ");
}

// We only support debug builds on Unix
#[cfg(unix)]
#[test]
fn python_install_debug() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_filtered_latest_python_versions()
        .with_managed_python_dirs();

    // Install the latest version
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.13+debug"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.13.[LATEST] in [TIME]
     + cpython-3.13.[LATEST]+debug-[PLATFORM] (python3.13d)
    ");

    let bin_python = context
        .bin_dir
        .child(format!("python3.13d{}", std::env::consts::EXE_SUFFIX));

    // The executable should be installed in the bin directory
    bin_python.assert(predicate::path::exists());

    // On Unix, it should be a link
    #[cfg(unix)]
    bin_python.assert(predicate::path::is_symlink());

    // The executable should "work"
    uv_snapshot!(context.filters(), Command::new(bin_python.as_os_str())
        .arg("-c").arg("import subprocess; print('hello world')"), @"
    exit_code: 0 (success)
    ----- stdout -----
    hello world
    ");

    // We should find it with opt-in
    uv_snapshot!(context.filters(), context.python_find().arg("3.13d"), @"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13+debug-[PLATFORM]/bin/python3.13d
    ");

    // We should find it without opt-in
    uv_snapshot!(context.filters(), context.python_find().arg("3.13"), @"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13+debug-[PLATFORM]/bin/python3.13d
    ");

    // Should be distinct from 3.13
    uv_snapshot!(context.filters(), context.python_install().arg("3.13"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.13.[LATEST] in [TIME]
     + cpython-3.13.[LATEST]-[PLATFORM] (python3.13)
    ");

    // Now we should prefer the non-debug version without opt-in
    uv_snapshot!(context.filters(), context.python_find().arg("3.13"), @"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13-[PLATFORM]/bin/python3.13
    ");

    // But still select it with opt-in
    uv_snapshot!(context.filters(), context.python_find().arg("3.13d"), @"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13+debug-[PLATFORM]/bin/python3.13d
    ");

    // We should allow selection with `+debug`
    uv_snapshot!(context.filters(), context.python_find().arg("3.13+debug"), @"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13+debug-[PLATFORM]/bin/python3.13d
    ");

    // Should work with older Python versions too
    uv_snapshot!(context.filters(), context.python_install().arg("3.12d"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.12.[LATEST] in [TIME]
     + cpython-3.12.[LATEST]+debug-[PLATFORM] (python3.12d)
    ");

    uv_snapshot!(context.filters(), context.python_uninstall().arg("--all"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Searching for Python installations
    Uninstalled 3 versions in [TIME]
     - cpython-3.12.[LATEST]+debug-[PLATFORM] (python3.12d)
     - cpython-3.13.[LATEST]+debug-[PLATFORM] (python3.13d)
     - cpython-3.13.[LATEST]-[PLATFORM] (python3.13)
    ");
}

// We only support debug builds on Unix
#[cfg(unix)]
#[test]
fn python_install_debug_freethreaded() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_filtered_latest_python_versions()
        .with_managed_python_dirs();

    // Install the latest version
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.13td"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.13.[LATEST] in [TIME]
     + cpython-3.13.[LATEST]+freethreaded+debug-[PLATFORM] (python3.13td)
    ");

    let bin_python = context
        .bin_dir
        .child(format!("python3.13td{}", std::env::consts::EXE_SUFFIX));

    // The executable should be installed in the bin directory
    bin_python.assert(predicate::path::exists());

    // On Unix, it should be a link
    #[cfg(unix)]
    bin_python.assert(predicate::path::is_symlink());

    // The executable should "work"
    uv_snapshot!(context.filters(), Command::new(bin_python.as_os_str())
        .arg("-c").arg("import subprocess; print('hello world')"), @"
    exit_code: 0 (success)
    ----- stdout -----
    hello world
    ");

    // We should find it with opt-in
    uv_snapshot!(context.filters(), context.python_find().arg("3.13td"), @"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13+freethreaded+debug-[PLATFORM]/bin/python3.13td
    ");

    // We should not find it without opt-in
    uv_snapshot!(context.filters(), context.python_find().arg("3.13"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: No interpreter found for Python 3.13 in virtual environments, managed installations, or search path
    ");

    // A `+freethreaded+debug` request should select the combined build.
    uv_snapshot!(context.filters(), context.python_find().arg("3.13+freethreaded+debug"), @"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13+freethreaded+debug-[PLATFORM]/bin/python3.13td
    ");

    // Should be distinct from 3.13
    uv_snapshot!(context.filters(), context.python_install().arg("3.13"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.13.[LATEST] in [TIME]
     + cpython-3.13.[LATEST]-[PLATFORM] (python3.13)
    ");

    // Should be distinct from 3.13t
    uv_snapshot!(context.filters(), context.python_install().arg("3.13t"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.13.[LATEST] in [TIME]
     + cpython-3.13.[LATEST]+freethreaded-[PLATFORM] (python3.13t)
    ");

    // Should be distinct from 3.13d
    uv_snapshot!(context.filters(), context.python_install().arg("3.13d"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.13.[LATEST] in [TIME]
     + cpython-3.13.[LATEST]+debug-[PLATFORM] (python3.13d)
    ");

    // Now we should prefer the non-debug version without opt-in
    uv_snapshot!(context.filters(), context.python_find().arg("3.13"), @"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13-[PLATFORM]/bin/python3.13
    ");

    uv_snapshot!(context.filters(), context.python_find().arg("3.13t"), @"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13+freethreaded-[PLATFORM]/bin/python3.13t
    ");

    // But still select it with opt-in
    uv_snapshot!(context.filters(), context.python_find().arg("3.13td"), @"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13+freethreaded+debug-[PLATFORM]/bin/python3.13td
    ");

    uv_snapshot!(context.filters(), context.python_uninstall().arg("--all"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Searching for Python installations
    Uninstalled 4 versions in [TIME]
     - cpython-3.13.[LATEST]+freethreaded+debug-[PLATFORM] (python3.13td)
     - cpython-3.13.[LATEST]+freethreaded-[PLATFORM] (python3.13t)
     - cpython-3.13.[LATEST]+debug-[PLATFORM] (python3.13d)
     - cpython-3.13.[LATEST]-[PLATFORM] (python3.13)
    ");
}

#[test]
fn python_install_invalid_request() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs();

    // Request something that is not a Python version
    uv_snapshot!(context.filters(), context.python_install().arg("foobar"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: `foobar` is not a valid Python download request; see `uv help python` for supported formats and `uv python list --only-downloads` for available versions
    ");

    // Request a version we don't have a download for
    uv_snapshot!(context.filters(), context.python_install().arg("3.8.0"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: No download found for request: cpython-3.8.0-[PLATFORM]
    ");

    // Request a version we don't have a download for mixed with one we do
    uv_snapshot!(context.filters(), context.python_install().arg("3.8.0").arg("3.12"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: No download found for request: cpython-3.8.0-[PLATFORM]
    ");
}

#[test]
fn python_install_default() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_filtered_latest_python_versions()
        .with_managed_python_dirs();

    let bin_python_minor_14 = context
        .bin_dir
        .child(format!("python3.14{}", std::env::consts::EXE_SUFFIX));

    let bin_python_major = context
        .bin_dir
        .child(format!("python3{}", std::env::consts::EXE_SUFFIX));

    let bin_python_default = context
        .bin_dir
        .child(format!("python{}", std::env::consts::EXE_SUFFIX));

    // Install a specific version
    uv_snapshot!(context.filters(), context.python_install().arg("3.14"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.14.[LATEST] in [TIME]
     + cpython-3.14.[LATEST]-[PLATFORM] (python3.14)
    ");

    // Only the minor versioned executable should be installed
    bin_python_minor_14.assert(predicate::path::exists());
    bin_python_major.assert(predicate::path::missing());
    bin_python_default.assert(predicate::path::missing());

    // Install again, with `--default`
    uv_snapshot!(context.filters(), context.python_install().arg("--default").arg("3.14"), @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: The `--default` option is experimental and may change without warning. Pass `--preview-features python-install-default` to disable this warning
    Installed Python 3.14.[LATEST] in [TIME]
     + cpython-3.14.[LATEST]-[PLATFORM] (python, python3)
    ");

    // Now all the executables should be installed
    bin_python_minor_14.assert(predicate::path::exists());
    bin_python_major.assert(predicate::path::exists());
    bin_python_default.assert(predicate::path::exists());

    // Uninstall
    uv_snapshot!(context.filters(), context.python_uninstall().arg("--all"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Searching for Python installations
    Uninstalled Python 3.14.[LATEST] in [TIME]
     - cpython-3.14.[LATEST]-[PLATFORM] (python, python3, python3.14)
    ");

    // The executables should be removed
    bin_python_minor_14.assert(predicate::path::missing());
    bin_python_major.assert(predicate::path::missing());
    bin_python_default.assert(predicate::path::missing());

    // Install the latest version, i.e., a "default install"
    uv_snapshot!(context.filters(), context.python_install().arg("--default"), @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: The `--default` option is experimental and may change without warning. Pass `--preview-features python-install-default` to disable this warning
    Installed Python 3.14.[LATEST] in [TIME]
     + cpython-3.14.[LATEST]-[PLATFORM] (python, python3, python3.14)
    ");

    // Since it's a default install, we should include all of the executables
    bin_python_minor_14.assert(predicate::path::exists());
    bin_python_major.assert(predicate::path::exists());
    bin_python_default.assert(predicate::path::exists());

    // And 3.14 should be the default
    if cfg!(unix) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_major), @"[TEMP_DIR]/managed/cpython-3.14-[PLATFORM]/bin/python3.14"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_minor_14), @"[TEMP_DIR]/managed/cpython-3.14-[PLATFORM]/bin/python3.14"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_default), @"[TEMP_DIR]/managed/cpython-3.14-[PLATFORM]/bin/python3.14"
            );
        });
    } else if cfg!(windows) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_major), @"[TEMP_DIR]/managed/cpython-3.14-[PLATFORM]/python"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_minor_14), @"[TEMP_DIR]/managed/cpython-3.14-[PLATFORM]/python"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_default), @"[TEMP_DIR]/managed/cpython-3.14-[PLATFORM]/python"
            );
        });
    }

    // Uninstall again
    uv_snapshot!(context.filters(), context.python_uninstall().arg("3.14"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Searching for Python versions matching: Python 3.14
    Uninstalled Python 3.14.[LATEST] in [TIME]
     - cpython-3.14.[LATEST]-[PLATFORM] (python, python3, python3.14)
    ");

    // We should remove all the executables
    bin_python_minor_14.assert(predicate::path::missing());
    bin_python_major.assert(predicate::path::missing());
    bin_python_default.assert(predicate::path::missing());

    // Install multiple versions, with the `--default` flag
    uv_snapshot!(context.filters(), context.python_install().arg("3.12").arg("3.14").arg("--default"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    warning: The `--default` option is experimental and may change without warning. Pass `--preview-features python-install-default` to disable this warning
    error: The `--default` flag cannot be used with multiple targets
    ");

    // Install 3.12 as a new default
    uv_snapshot!(context.filters(), context.python_install().arg("3.12").arg("--default"), @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: The `--default` option is experimental and may change without warning. Pass `--preview-features python-install-default` to disable this warning
    Installed Python 3.12.[LATEST] in [TIME]
     + cpython-3.12.[LATEST]-[PLATFORM] (python, python3, python3.12)
    ");

    let bin_python_minor_12 = context
        .bin_dir
        .child(format!("python3.12{}", std::env::consts::EXE_SUFFIX));

    // All the executables should exist
    bin_python_minor_12.assert(predicate::path::exists());
    bin_python_major.assert(predicate::path::exists());
    bin_python_default.assert(predicate::path::exists());

    // And 3.12 should be the default
    if cfg!(unix) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_major), @"[TEMP_DIR]/managed/cpython-3.12-[PLATFORM]/bin/python3.12"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_minor_12), @"[TEMP_DIR]/managed/cpython-3.12-[PLATFORM]/bin/python3.12"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_default), @"[TEMP_DIR]/managed/cpython-3.12-[PLATFORM]/bin/python3.12"
            );
        });
    } else {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_major), @"[TEMP_DIR]/managed/cpython-3.12-[PLATFORM]/python"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_minor_12), @"[TEMP_DIR]/managed/cpython-3.12-[PLATFORM]/python"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_default), @"[TEMP_DIR]/managed/cpython-3.12-[PLATFORM]/python"
            );
        });
    }
}

#[test]
fn python_install_default_preview() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_filtered_latest_python_versions()
        .with_managed_python_dirs();

    let bin_python_minor_14 = context
        .bin_dir
        .child(format!("python3.14{}", std::env::consts::EXE_SUFFIX));

    let bin_python_major = context
        .bin_dir
        .child(format!("python3{}", std::env::consts::EXE_SUFFIX));

    let bin_python_default = context
        .bin_dir
        .child(format!("python{}", std::env::consts::EXE_SUFFIX));

    // Install a specific version
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.14"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.14.[LATEST] in [TIME]
     + cpython-3.14.[LATEST]-[PLATFORM] (python3.14)
    ");

    // Only the minor versioned executable should be installed
    bin_python_minor_14.assert(predicate::path::exists());
    bin_python_major.assert(predicate::path::missing());
    bin_python_default.assert(predicate::path::missing());

    // Install again, with `--default`
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("--default").arg("3.14"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.14.[LATEST] in [TIME]
     + cpython-3.14.[LATEST]-[PLATFORM] (python, python3)
    ");

    // Now all the executables should be installed
    bin_python_minor_14.assert(predicate::path::exists());
    bin_python_major.assert(predicate::path::exists());
    bin_python_default.assert(predicate::path::exists());

    // Uninstall
    uv_snapshot!(context.filters(), context.python_uninstall().arg("--all"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Searching for Python installations
    Uninstalled Python 3.14.[LATEST] in [TIME]
     - cpython-3.14.[LATEST]-[PLATFORM] (python, python3, python3.14)
    ");

    // The executables should be removed
    bin_python_minor_14.assert(predicate::path::missing());
    bin_python_major.assert(predicate::path::missing());
    bin_python_default.assert(predicate::path::missing());

    // Install the latest version, i.e., a "default install"
    uv_snapshot!(context.filters(), context.python_install().arg("--default").arg("--preview"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.14.[LATEST] in [TIME]
     + cpython-3.14.[LATEST]-[PLATFORM] (python, python3, python3.14)
    ");

    // Since it's a default install, we should include all of the executables
    bin_python_minor_14.assert(predicate::path::exists());
    bin_python_major.assert(predicate::path::exists());
    bin_python_default.assert(predicate::path::exists());

    // And 3.14 should be the default
    if cfg!(unix) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_major), @"[TEMP_DIR]/managed/cpython-3.14-[PLATFORM]/bin/python3.14"
            );
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python_major), @"[TEMP_DIR]/managed/cpython-3.14.[LATEST]-[PLATFORM]/bin/python3.14"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_minor_14), @"[TEMP_DIR]/managed/cpython-3.14-[PLATFORM]/bin/python3.14"
            );
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python_minor_14), @"[TEMP_DIR]/managed/cpython-3.14.[LATEST]-[PLATFORM]/bin/python3.14"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_default), @"[TEMP_DIR]/managed/cpython-3.14-[PLATFORM]/bin/python3.14"
            );
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python_default), @"[TEMP_DIR]/managed/cpython-3.14.[LATEST]-[PLATFORM]/bin/python3.14"
            );
        });
    } else if cfg!(windows) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_major), @"[TEMP_DIR]/managed/cpython-3.14-[PLATFORM]/python"
            );
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python_major), @"[TEMP_DIR]/managed/cpython-3.14.[LATEST]-[PLATFORM]/python"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_minor_14), @"[TEMP_DIR]/managed/cpython-3.14-[PLATFORM]/python"
            );
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python_minor_14), @"[TEMP_DIR]/managed/cpython-3.14.[LATEST]-[PLATFORM]/python"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_default), @"[TEMP_DIR]/managed/cpython-3.14-[PLATFORM]/python"
            );
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python_default), @"[TEMP_DIR]/managed/cpython-3.14.[LATEST]-[PLATFORM]/python"
            );
        });
    }

    // Uninstall again
    uv_snapshot!(context.filters(), context.python_uninstall().arg("3.14"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Searching for Python versions matching: Python 3.14
    Uninstalled Python 3.14.[LATEST] in [TIME]
     - cpython-3.14.[LATEST]-[PLATFORM] (python, python3, python3.14)
    ");

    // We should remove all the executables
    bin_python_minor_14.assert(predicate::path::missing());
    bin_python_major.assert(predicate::path::missing());
    bin_python_default.assert(predicate::path::missing());

    // Install multiple versions, with the `--default` flag
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.12").arg("3.14").arg("--default"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: The `--default` flag cannot be used with multiple targets
    ");

    // Install 3.12 as a new default
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.12").arg("--default"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.12.[LATEST] in [TIME]
     + cpython-3.12.[LATEST]-[PLATFORM] (python, python3, python3.12)
    ");

    let bin_python_minor_12 = context
        .bin_dir
        .child(format!("python3.12{}", std::env::consts::EXE_SUFFIX));

    // All the executables should exist
    bin_python_minor_12.assert(predicate::path::exists());
    bin_python_major.assert(predicate::path::exists());
    bin_python_default.assert(predicate::path::exists());

    // And 3.12 should be the default
    if cfg!(unix) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_major), @"[TEMP_DIR]/managed/cpython-3.12-[PLATFORM]/bin/python3.12"
            );
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python_major), @"[TEMP_DIR]/managed/cpython-3.12.[LATEST]-[PLATFORM]/bin/python3.12"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_minor_12), @"[TEMP_DIR]/managed/cpython-3.12-[PLATFORM]/bin/python3.12"
            );
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python_minor_12), @"[TEMP_DIR]/managed/cpython-3.12.[LATEST]-[PLATFORM]/bin/python3.12"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_default), @"[TEMP_DIR]/managed/cpython-3.12-[PLATFORM]/bin/python3.12"
            );
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python_default), @"[TEMP_DIR]/managed/cpython-3.12.[LATEST]-[PLATFORM]/bin/python3.12"
            );
        });
    } else {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_major), @"[TEMP_DIR]/managed/cpython-3.12-[PLATFORM]/python"
            );
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python_major), @"[TEMP_DIR]/managed/cpython-3.12.[LATEST]-[PLATFORM]/python"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_minor_12), @"[TEMP_DIR]/managed/cpython-3.12-[PLATFORM]/python"
            );
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python_minor_12), @"[TEMP_DIR]/managed/cpython-3.12.[LATEST]-[PLATFORM]/python"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&bin_python_default), @"[TEMP_DIR]/managed/cpython-3.12-[PLATFORM]/python"
            );
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python_default), @"[TEMP_DIR]/managed/cpython-3.12.[LATEST]-[PLATFORM]/python"
            );
        });
    }

    // Change the default to 3.14
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.14").arg("--default"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.14.[LATEST] in [TIME]
     + cpython-3.14.[LATEST]-[PLATFORM] (python, python3, python3.14)
    ");

    // All the executables should exist
    bin_python_minor_14.assert(predicate::path::exists());
    bin_python_minor_12.assert(predicate::path::exists());
    bin_python_major.assert(predicate::path::exists());
    bin_python_default.assert(predicate::path::exists());

    // And 3.14 should be the default now
    if cfg!(unix) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python_major), @"[TEMP_DIR]/managed/cpython-3.14.[LATEST]-[PLATFORM]/bin/python3.14"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python_minor_14), @"[TEMP_DIR]/managed/cpython-3.14.[LATEST]-[PLATFORM]/bin/python3.14"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python_minor_12), @"[TEMP_DIR]/managed/cpython-3.12.[LATEST]-[PLATFORM]/bin/python3.12"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python_default), @"[TEMP_DIR]/managed/cpython-3.14.[LATEST]-[PLATFORM]/bin/python3.14"
            );
        });
    } else if cfg!(windows) {
        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python_major), @"[TEMP_DIR]/managed/cpython-3.14.[LATEST]-[PLATFORM]/python"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python_minor_14), @"[TEMP_DIR]/managed/cpython-3.14.[LATEST]-[PLATFORM]/python"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python_minor_12), @"[TEMP_DIR]/managed/cpython-3.12.[LATEST]-[PLATFORM]/python"
            );
        });

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                canonicalize_link_path(&bin_python_default), @"[TEMP_DIR]/managed/cpython-3.14.[LATEST]-[PLATFORM]/python"
            );
        });
    }
}

#[cfg(windows)]
fn launcher_path(path: &Path) -> PathBuf {
    let launcher = uv_trampoline_builder::Launcher::try_from_path(path)
        .unwrap_or_else(|_| panic!("{} should be readable", path.display()))
        .unwrap_or_else(|| panic!("{} should be a valid launcher", path.display()));
    launcher.python_path
}

fn canonicalize_link_path(path: &Path) -> String {
    #[cfg(unix)]
    let canonical_path = fs_err::canonicalize(path);

    #[cfg(windows)]
    let canonical_path = dunce::canonicalize(launcher_path(path));

    canonical_path
        .unwrap_or_else(|_| panic!("{} should be readable", path.display()))
        .simplified_display()
        .to_string()
}

fn read_link(path: &Path) -> String {
    #[cfg(unix)]
    let linked_path =
        fs_err::read_link(path).unwrap_or_else(|_| panic!("{} should be readable", path.display()));

    #[cfg(windows)]
    let linked_path = launcher_path(path);

    linked_path.simplified_display().to_string()
}

#[test]
fn python_install_unknown() {
    let context = uv_test::test_context_with_versions!(&[]).with_managed_python_dirs();

    // An unknown request
    uv_snapshot!(context.filters(), context.python_install().arg("foobar"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: `foobar` is not a valid Python download request; see `uv help python` for supported formats and `uv python list --only-downloads` for available versions
    ");

    context.temp_dir.child("foo").create_dir_all().unwrap();

    // A directory
    uv_snapshot!(context.filters(), context.python_install().arg("./foo"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: `./foo` is not a valid Python download request; see `uv help python` for supported formats and `uv python list --only-downloads` for available versions
    ");
}

#[cfg(unix)]
#[test]
fn python_install_broken_link() {
    use assert_fs::prelude::PathCreateDir;
    use fs_err::os::unix::fs::symlink;

    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs()
        .with_filtered_latest_python_versions();

    let bin_python = context.bin_dir.child("python3.13");

    // Create a broken symlink
    context.bin_dir.create_dir_all().unwrap();
    symlink(context.temp_dir.join("does-not-exist"), &bin_python).unwrap();

    // Install
    uv_snapshot!(context.filters(), context.python_install().arg("3.13"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.13.[LATEST] in [TIME]
     + cpython-3.13.[LATEST]-[PLATFORM] (python3.13)
    ");

    // We should replace the broken symlink
    insta::with_settings!({
        filters => context.filters(),
    }, {
        insta::assert_snapshot!(
            canonicalize_link_path(&bin_python), @"[TEMP_DIR]/managed/cpython-3.13.[LATEST]-[PLATFORM]/bin/python3.13"
        );
    });
}

#[cfg(unix)]
#[test]
fn python_install_relative_unmanaged_link() -> anyhow::Result<()> {
    use fs_err::os::unix::fs::symlink;

    let context = uv_test::test_context!("3.12")
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs();
    context
        .python_install()
        .args(["--no-config", "--no-bin", "3.13.1"])
        .assert()
        .success();

    let bin_python = context.bin_dir.child("python3.13");
    let unmanaged = context.bin_dir.child("unmanaged-python");
    symlink(context.interpreter(), &unmanaged)?;
    symlink("unmanaged-python", &bin_python)?;
    assert!(bin_python.try_exists()?);
    assert!(!context.temp_dir.child("unmanaged-python").try_exists()?);

    uv_snapshot!(context.filters(), context.python_install()
        .args(["--no-config", "--offline", "3.13.1"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: Failed to install executable for cpython-3.13.1-[PLATFORM]
      cause: Executable already exists at `[BIN]/python3.13` but is not managed by uv; use `--force` to replace it
    ");
    assert_eq!(
        fs_err::read_link(&bin_python)?,
        Path::new("unmanaged-python")
    );
    uv_snapshot!(context.filters(), Command::new(bin_python.path())
        .args(["-I", "-c", "import sys; print('.'.join(map(str, sys.version_info[:2])))"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    3.12
    ");

    uv_snapshot!(context.filters(), context.python_install()
        .args(["--no-config", "--offline", "3.13.1", "--force"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.13.1 in [TIME]
     + cpython-3.13.1-[PLATFORM] (python3.13)
    ");
    insta::with_settings!({
        filters => context.filters(),
    }, {
        insta::assert_snapshot!(
            canonicalize_link_path(&bin_python), @"[TEMP_DIR]/managed/cpython-3.13.1-[PLATFORM]/bin/python3.13"
        );
    });
    uv_snapshot!(context.filters(), Command::new(bin_python.path())
        .args(["-I", "-c", "import sys; print(sys.version.split()[0])"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    3.13.1
    ");
    Ok(())
}

#[cfg(unix)]
#[test]
fn python_install_relative_broken_link() -> anyhow::Result<()> {
    use fs_err::os::unix::fs::symlink;

    let context = uv_test::test_context!("3.12")
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs();
    context
        .python_install()
        .args(["--no-config", "--no-bin", "3.13.1"])
        .assert()
        .success();

    // A target with the same name exists in uv's working directory, but the executable link is
    // relative to its own directory and is dangling.
    symlink(
        context.interpreter(),
        context.temp_dir.child("unmanaged-python"),
    )?;
    let bin_python = context.bin_dir.child("python3.13");
    symlink("unmanaged-python", &bin_python)?;
    assert!(!bin_python.try_exists()?);
    assert!(context.temp_dir.child("unmanaged-python").try_exists()?);

    uv_snapshot!(context.filters(), context.python_install()
        .args(["--no-config", "--offline", "3.13.1"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.13.1 in [TIME]
     + cpython-3.13.1-[PLATFORM] (python3.13)
    ");
    insta::with_settings!({
        filters => context.filters(),
    }, {
        insta::assert_snapshot!(
            canonicalize_link_path(&bin_python), @"[TEMP_DIR]/managed/cpython-3.13.1-[PLATFORM]/bin/python3.13"
        );
    });
    uv_snapshot!(context.filters(), Command::new(bin_python.path())
        .args(["-I", "-c", "import sys; print(sys.version.split()[0])"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    3.13.1
    ");
    Ok(())
}

/// Test that --default works with pre-release versions (e.g., 3.15.0a1).
/// This test verifies the fix for issue #16696 where --default didn't create
/// python.exe and python3.exe links for pre-release versions.
#[test]
fn python_install_default_prerelease() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs();

    // Install Python 3.15, which currently only exists as a pre-release.
    context
        .python_install()
        .arg("--default")
        .arg("--preview-features")
        .arg("python-install-default")
        .arg("3.15")
        .assert()
        .success();

    let bin_python_minor_15 = context
        .bin_dir
        .child(format!("python3.15{}", std::env::consts::EXE_SUFFIX));

    let bin_python_major = context
        .bin_dir
        .child(format!("python3{}", std::env::consts::EXE_SUFFIX));

    let bin_python_default = context
        .bin_dir
        .child(format!("python{}", std::env::consts::EXE_SUFFIX));

    // Verify that all three executables are created when --default is used with a pre-release version
    bin_python_minor_15.assert(predicate::path::exists());
    bin_python_major.assert(predicate::path::exists());
    bin_python_default.assert(predicate::path::exists());
}

#[test]
fn python_install_default_from_env() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_filtered_latest_python_versions()
        .with_managed_python_dirs();

    // Install the version specified by the `UV_PYTHON` environment variable by default
    uv_snapshot!(context.filters(), context.python_install().env(EnvVars::UV_PYTHON, "3.12"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.12.[LATEST] in [TIME]
     + cpython-3.12.[LATEST]-[PLATFORM] (python3.12)
    ");

    // But prefer explicit requests
    uv_snapshot!(context.filters(), context.python_install().arg("3.11").env(EnvVars::UV_PYTHON, "3.12"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.11.[LATEST] in [TIME]
     + cpython-3.11.[LATEST]-[PLATFORM] (python3.11)
    ");

    // We should ignore `UV_PYTHON` here and complain there is not a target
    uv_snapshot!(context.filters(), context.python_uninstall().env(EnvVars::UV_PYTHON, "3.12"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: the following required arguments were not provided:
      <TARGETS>...

    Usage: uv python uninstall --cache-dir [CACHE_DIR] --install-dir <INSTALL_DIR> <TARGETS>...

    For more information, try '--help'.
    ");

    // We should ignore `UV_PYTHON` here and respect `--all`
    uv_snapshot!(context.filters(), context.python_uninstall().arg("--all").env(EnvVars::UV_PYTHON, "3.11"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Searching for Python installations
    Uninstalled 2 versions in [TIME]
     - cpython-3.11.[LATEST]-[PLATFORM] (python3.11)
     - cpython-3.12.[LATEST]-[PLATFORM] (python3.12)
    ");

    // Uninstall with no targets should error
    uv_snapshot!(context.filters(), context.python_uninstall(), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: the following required arguments were not provided:
      <TARGETS>...

    Usage: uv python uninstall --cache-dir [CACHE_DIR] --install-dir <INSTALL_DIR> <TARGETS>...

    For more information, try '--help'.
    ");

    // Uninstall with conflicting options should error
    uv_snapshot!(context.filters(), context.python_uninstall().arg("--all").arg("3.12"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: the argument '--all' cannot be used with '<TARGETS>...'

    Usage: uv python uninstall --cache-dir [CACHE_DIR] --all --install-dir <INSTALL_DIR> <TARGETS>...

    For more information, try '--help'.
    ");
}

#[cfg(target_os = "macos")]
#[test]
fn python_install_patch_dylib() {
    use assert_cmd::assert::OutputAssertExt;
    use uv_python::managed::platform_key_from_env;

    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_managed_python_dirs();

    // Install the latest version
    context
        .python_install()
        .arg("--preview")
        .arg("3.13.1")
        .assert()
        .success();

    let dylib = context
        .temp_dir
        .child("managed")
        .child(format!(
            "cpython-3.13.1-{}",
            platform_key_from_env().unwrap()
        ))
        .child("lib")
        .child(format!(
            "{}python3.13{}",
            std::env::consts::DLL_PREFIX,
            std::env::consts::DLL_SUFFIX
        ));

    let mut cmd = std::process::Command::new("otool");
    cmd.arg("-D").arg(dylib.as_ref());

    uv_snapshot!(context.filters(), cmd, @r###"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13.1-[PLATFORM]/lib/libpython3.13.dylib:
    [TEMP_DIR]/managed/cpython-3.13.1-[PLATFORM]/lib/libpython3.13.dylib
    "###);
}

#[test]
fn python_install_prerelease() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_latest_python_versions()
        .with_managed_python_dirs()
        .with_filtered_python_install_bin()
        .with_filtered_python_names()
        .with_filtered_exe_suffix();

    // Install 3.15
    // For now, this provides test coverage of pre-release handling
    uv_snapshot!(context.filters(), context.python_install().arg("3.15"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.15.[LATEST] in [TIME]
     + cpython-3.15.[LATEST]-[PLATFORM] (python3.15)
    ");

    // Install a specific pre-release
    uv_snapshot!(context.filters(), context.python_install().arg("3.15.0a2"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.15.0a2 in [TIME]
     + cpython-3.15.0a2-[PLATFORM]
    ");

    // We should be able to find this version without opt-in, because there is no stable release
    // installed
    uv_snapshot!(context.filters(), context.python_find().arg("3.15"), @"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.15-[PLATFORM]/[INSTALL-BIN]/[PYTHON]
    ");

    // This also applies to `>=` requests, even though pre-releases aren't technically in the range
    uv_snapshot!(context.filters(), context.python_find().arg(">=3.15"), @"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.15-[PLATFORM]/[INSTALL-BIN]/[PYTHON]
    ");

    uv_snapshot!(context.filters(), context.python_find().arg("3"), @"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.15-[PLATFORM]/[INSTALL-BIN]/[PYTHON]
    ");

    // If we install a stable version, that should be preferred though
    uv_snapshot!(context.filters(), context.python_install().arg("3.13"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.13.[LATEST] in [TIME]
     + cpython-3.13.[LATEST]-[PLATFORM] (python3.13)
    ");

    uv_snapshot!(context.filters(), context.python_find().arg("3"), @"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13-[PLATFORM]/[INSTALL-BIN]/[PYTHON]
    ");

    // Install a release candidate for a non-zero patch version
    uv_snapshot!(context.filters(), context.python_install().arg("3.14.5rc1"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.14.5rc1 in [TIME]
     + cpython-3.14.5rc1-[PLATFORM] (python3.14)
    ");
}

/// A duplicate of [`python_install`] with an isolated `UV_PYTHON_CACHE_DIR`.
///
/// See also, [`python_install_no_cache`].
#[test]
fn python_install_cached() {
    // Skip this test if the developer has set `UV_PYTHON_CACHE_DIR` locally since it's slow
    if env::var_os(EnvVars::UV_PYTHON_CACHE_DIR).is_some() && env::var_os(EnvVars::CI).is_none() {
        debug!("Skipping test because `UV_PYTHON_CACHE_DIR` is set");
        return;
    }

    let context = uv_test::test_context_with_versions!(&[])
        .without_python_download_cache()
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs()
        .with_filtered_latest_python_versions();

    let python_cache = context.temp_dir.child("python-cache");

    // Install the latest version
    uv_snapshot!(context.filters(), context
        .python_install()
        .env(EnvVars::UV_PYTHON_CACHE_DIR, python_cache.as_ref()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.14.[LATEST] in [TIME]
     + cpython-3.14.[LATEST]-[PLATFORM] (python3.14)
    ");

    let bin_python = context
        .bin_dir
        .child(format!("python3.14{}", std::env::consts::EXE_SUFFIX));

    // The executable should be installed in the bin directory
    bin_python.assert(predicate::path::exists());

    // Should be a no-op when already installed
    uv_snapshot!(context.filters(), context
        .python_install()
        .env(EnvVars::UV_PYTHON_CACHE_DIR, python_cache.as_ref()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Python is already installed. Use `uv python install <request>` to install another version.
    ");

    uv_snapshot!(context.filters(), context.python_uninstall().arg("3.14"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Searching for Python versions matching: Python 3.14
    Uninstalled Python 3.14.[LATEST] in [TIME]
     - cpython-3.14.[LATEST]-[PLATFORM] (python3.14)
    ");

    // The cached archive can be installed offline
    uv_snapshot!(context.filters(), context
        .python_install()
        .arg("--offline")
        .env(EnvVars::UV_PYTHON_CACHE_DIR, python_cache.as_ref()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.14.[LATEST] in [TIME]
     + cpython-3.14.[LATEST]-[PLATFORM] (python3.14)
    ");

    // 3.12 isn't cached, so it can't be installed
    let context = context.with_filter((
        "cpython-3.12.*.tar.gz",
        "cpython-3.12.[PATCH]-[DATE]-[PLATFORM].tar.gz",
    ));
    uv_snapshot!(context.filters(), context
        .python_install()
        .arg("3.12")
        .arg("--offline")
        .env(EnvVars::UV_PYTHON_CACHE_DIR, python_cache.as_ref()), @"
    exit_code: 1 (failure)
    ----- stderr -----
    error: Failed to install cpython-3.12.[LATEST]-[PLATFORM]
      cause: An offline Python installation was requested, but cpython-3.12.[PATCH]-[DATE]-[PLATFORM].tar.gz) is missing in python-cache
    ");
}

/// Duplicate of [`python_install`] with the cache directory disabled.
#[test]
fn python_install_no_cache() {
    // Skip this test if the developer has set `UV_PYTHON_CACHE_DIR` locally since it's slow
    if env::var_os(EnvVars::UV_PYTHON_CACHE_DIR).is_some() && env::var_os(EnvVars::CI).is_none() {
        debug!("Skipping test because `UV_PYTHON_CACHE_DIR` is set");
        return;
    }

    let context = uv_test::test_context_with_versions!(&[])
        .without_python_download_cache()
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs()
        .with_filtered_latest_python_versions();

    // Install the latest version
    uv_snapshot!(context.filters(), context.python_install(), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.14.[LATEST] in [TIME]
     + cpython-3.14.[LATEST]-[PLATFORM] (python3.14)
    ");

    let bin_python = context
        .bin_dir
        .child(format!("python3.14{}", std::env::consts::EXE_SUFFIX));

    // The executable should not present in the bin directory
    bin_python.assert(predicate::path::exists());

    // Should be a no-op when already installed
    uv_snapshot!(context.filters(), context.python_install(), @"
    exit_code: 0 (success)
    ----- stderr -----
    Python is already installed. Use `uv python install <request>` to install another version.
    ");

    // Similarly, when a requested version is already installed
    uv_snapshot!(context.filters(), context.python_install().arg("3.14"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Python 3.14 is already installed
    ");

    // You can opt-in to a reinstall
    uv_snapshot!(context.filters(), context.python_install().arg("3.14").arg("--reinstall"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.14.[LATEST] in [TIME]
     ~ cpython-3.14.[LATEST]-[PLATFORM] (python3.14)
    ");

    // Uninstallation requires an argument
    uv_snapshot!(context.filters(), context.python_uninstall(), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: the following required arguments were not provided:
      <TARGETS>...

    Usage: uv python uninstall --cache-dir [CACHE_DIR] --install-dir <INSTALL_DIR> <TARGETS>...

    For more information, try '--help'.
    ");

    uv_snapshot!(context.filters(), context.python_uninstall().arg("3.14"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Searching for Python versions matching: Python 3.14
    Uninstalled Python 3.14.[LATEST] in [TIME]
     - cpython-3.14.[LATEST]-[PLATFORM] (python3.14)
    ");

    // 3.12 isn't cached, so it can't be installed
    let context = context
        .with_filter((
            "cpython-3.12.*.tar.gz",
            "cpython-3.12.[PATCH]-[DATE]-[PLATFORM].tar.gz",
        ))
        .with_filter((r"releases/download/\d{8}/", "releases/download/[DATE]/"));
    uv_snapshot!(context.filters(), context
        .python_install()
        .arg("3.12")
        .arg("--offline"), @"
    exit_code: 1 (failure)
    ----- stderr -----
    error: Failed to install cpython-3.12.[LATEST]-[PLATFORM]
      cause: Failed to download https://github.com/astral-sh/python-build-standalone/releases/download/[DATE]/cpython-3.12.[PATCH]-[DATE]-[PLATFORM].tar.gz
      cause: Network connectivity is disabled, but the requested data wasn't found in the cache for: `https://github.com/astral-sh/python-build-standalone/releases/download/[DATE]/cpython-3.12.[PATCH]-[DATE]-[PLATFORM].tar.gz`
    ");
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn python_install_emulated_macos() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_exe_suffix()
        .with_managed_python_dirs()
        .with_filtered_latest_python_versions();

    let arch_status = Command::new("/usr/bin/arch")
        .arg("-x86_64")
        .arg("true")
        .status();
    if !arch_status.is_ok_and(|x| x.success()) {
        // Rosetta is not available to run the x86_64 interpreter
        // fail the test in CI, otherwise skip it
        #[expect(clippy::manual_assert)]
        if env::var(EnvVars::CI).is_ok() {
            panic!("x86_64 emulation is not available on this CI runner");
        }
        debug!("Skipping test because x86_64 emulation is not available");
        return;
    }

    // Before installation, `uv python list` should not show the x86_64 download
    uv_snapshot!(context.filters(), context.python_list().arg("cpython@3.13"), @r"
    exit_code: 0 (success)
    ----- stdout -----
    cpython-3.13.[LATEST]-macos-aarch64-none    <download available>
    ");

    // Install an x86_64 version (assuming an aarch64 host)
    uv_snapshot!(context.filters(), context.python_install().arg("3.13-x86_64"), @r"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.13.[LATEST] in [TIME]
     + cpython-3.13.[LATEST]-macos-x86_64-none (python3.13)
    ");

    // It should be discoverable with `uv python find`
    uv_snapshot!(context.filters(), context.python_find().arg("3.13").arg("--resolve-links"), @r"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13.[LATEST]-macos-x86_64-none/bin/python3.13
    ");

    // And included in `uv python list`
    uv_snapshot!(context.filters(), context.python_list().arg("cpython@3.13"), @r"
    exit_code: 0 (success)
    ----- stdout -----
    cpython-3.13.[LATEST]-macos-aarch64-none    <download available>
    cpython-3.13.[LATEST]-macos-x86_64-none     managed/cpython-3.13-macos-x86_64-none/bin/python3.13
    ");

    uv_snapshot!(context.filters(), context.python_install().arg("3.13-aarch64"), @r"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.13.[LATEST] in [TIME]
     + cpython-3.13.[LATEST]-macos-aarch64-none
    ");

    // Once we've installed the native version, it should be preferred over x86_64
    uv_snapshot!(context.filters(), context.python_find().arg("3.13").arg("--resolve-links"), @r"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13.[LATEST]-macos-aarch64-none/bin/python3.13
    ");
}

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
#[test]
fn python_install_emulated_windows_x86_on_x64() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_exe_suffix()
        .with_managed_python_dirs()
        .with_filtered_latest_python_versions();

    // Before installation, `uv python list` should not show the x86_32 download
    uv_snapshot!(context.filters(), context.python_list().arg("cpython@3.13"), @r"
    exit_code: 0 (success)
    ----- stdout -----
    cpython-3.13.[LATEST]-windows-x86_64-none    <download available>
    ");

    // Install an x86_32 version (assuming an x64 host)
    uv_snapshot!(context.filters(), context.python_install().arg("3.13-x86"), @r"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.13.[LATEST] in [TIME]
     + cpython-3.13.[LATEST]-windows-x86-none (python3.13)
    ");

    // It should be discoverable with `uv python find`
    uv_snapshot!(context.filters(), context.python_find().arg("3.13"), @r"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13-windows-x86-none/python
    ");

    // And included in `uv python list`
    uv_snapshot!(context.filters(), context.python_list().arg("cpython@3.13"), @r"
    exit_code: 0 (success)
    ----- stdout -----
    cpython-3.13.[LATEST]-windows-x86_64-none    <download available>
    cpython-3.13.[LATEST]-windows-x86-none       managed/cpython-3.13-windows-x86-none/python
    ");

    uv_snapshot!(context.filters(), context.python_install().arg("3.13-x86_64"), @r"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.13.[LATEST] in [TIME]
     + cpython-3.13.[LATEST]-windows-x86_64-none
    ");

    // Once we've installed the native version, it should be preferred over x86_32
    uv_snapshot!(context.filters(), context.python_find().arg("3.13"), @r"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.13-windows-x86_64-none/python
    ");
}

// Creating a venv with `--allow-existing` over an existing managed venv should succeed.
//
// Regression test for <https://github.com/astral-sh/uv/issues/17963>.
#[test]
fn install_managed_venv_allow_existing() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_filtered_latest_python_versions()
        .with_managed_python_dirs()
        .with_filtered_python_install_bin();

    // Install a managed Python version.
    uv_snapshot!(context.filters(), context.python_install().arg("3.13"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.13.[LATEST] in [TIME]
     + cpython-3.13.[LATEST]-[PLATFORM] (python3.13)
    ");

    // Create a virtual environment using the managed installation.
    uv_snapshot!(context.filters(), context.venv().arg("-p").arg("3.13")
        .arg(context.venv.as_os_str()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Using CPython 3.13.[LATEST]
    Creating virtual environment at: .venv
    Activate with: source .venv/[BIN]/activate
    ");

    // Create the venv again with `--allow-existing` — this should not fail.
    uv_snapshot!(context.filters(), context.venv().arg("-p").arg("3.13")
        .arg("--allow-existing")
        .arg(context.venv.as_os_str()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Using CPython 3.13.[LATEST]
    Creating virtual environment at: .venv
    Activate with: source .venv/[BIN]/activate
    ");
}

// A virtual environment should track the latest patch version installed.
#[test]
fn install_transparent_patch_upgrade_uv_venv() {
    let context = uv_test::test_context_with_versions!(&["3.13"])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs()
        .with_filtered_python_install_bin();

    // Install a lower patch version.
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.12.9"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.12.9 in [TIME]
     + cpython-3.12.9-[PLATFORM] (python3.12)
    "
    );

    // Create a virtual environment.
    uv_snapshot!(context.filters(), context.venv().arg("-p").arg("3.12")
        .arg(context.venv.as_os_str()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Using CPython 3.12.9
    Creating virtual environment at: .venv
    Activate with: source .venv/[BIN]/activate
    "
    );

    #[cfg(windows)]
    {
        let scripts = context.venv.join("Scripts");

        insta::with_settings!({
            filters => context.filters(),
        }, {
            insta::assert_snapshot!(
                read_link(&scripts.join("python.exe")), @"[TEMP_DIR]/managed/cpython-3.12-[PLATFORM]/python"
            );
            insta::assert_snapshot!(
                read_link(&scripts.join("pythonw.exe")), @"[TEMP_DIR]/managed/cpython-3.12-[PLATFORM]/pythonw"
            );
        });
    }

    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version"), @"
    exit_code: 0 (success)
    ----- stdout -----
    Python 3.12.9
    "
    );

    // Install a higher patch version.
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.12.11"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.12.11 in [TIME]
     + cpython-3.12.11-[PLATFORM] (python3.12)
    "
    );

    // Virtual environment should reflect higher version.
    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version"), @"
    exit_code: 0 (success)
    ----- stdout -----
    Python 3.12.11
    "
    );

    // Install a lower patch version.
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.12.8"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.12.8 in [TIME]
     + cpython-3.12.8-[PLATFORM]
    "
    );

    // Virtual environment should reflect highest version.
    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version"), @"
    exit_code: 0 (success)
    ----- stdout -----
    Python 3.12.11
    "
    );
}

// When installing multiple patches simultaneously, a virtual environment on that
// minor version should point to the highest.
#[test]
fn install_multiple_patches() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs()
        .with_filtered_python_install_bin();

    // Install 3.12 patches in ascending order list
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.12.9").arg("3.12.11"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed 2 versions in [TIME]
     + cpython-3.12.9-[PLATFORM]
     + cpython-3.12.11-[PLATFORM] (python3.12)
    "
    );

    // Create a virtual environment.
    uv_snapshot!(context.filters(), context.venv().arg("-p").arg("3.12")
        .arg(context.venv.as_os_str()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Using CPython 3.12.11
    Creating virtual environment at: .venv
    Activate with: source .venv/[BIN]/activate
    "
    );

    // Virtual environment should be on highest installed patch.
    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version"), @"
    exit_code: 0 (success)
    ----- stdout -----
    Python 3.12.11
    "
    );

    // Remove the original virtual environment
    fs_err::remove_dir_all(&context.venv).unwrap();

    // Install 3.10 patches in descending order list
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.10.17").arg("3.10.16"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed 2 versions in [TIME]
     + cpython-3.10.16-[PLATFORM]
     + cpython-3.10.17-[PLATFORM] (python3.10)
    "
    );

    // Create a virtual environment on 3.10.
    uv_snapshot!(context.filters(), context.venv().arg("-p").arg("3.10")
        .arg(context.venv.as_os_str()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Using CPython 3.10.17
    Creating virtual environment at: .venv
    Activate with: source .venv/[BIN]/activate
    "
    );

    // Virtual environment should be on highest installed patch.
    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version"), @"
    exit_code: 0 (success)
    ----- stdout -----
    Python 3.10.17
    "
    );
}

// After uninstalling the highest patch, a virtual environment should point to the
// next highest.
#[test]
fn uninstall_highest_patch() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs()
        .with_filtered_python_install_bin();

    // Install patches in ascending order list
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.12.11").arg("3.12.9").arg("3.12.8"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed 3 versions in [TIME]
     + cpython-3.12.8-[PLATFORM]
     + cpython-3.12.9-[PLATFORM]
     + cpython-3.12.11-[PLATFORM] (python3.12)
    "
    );

    uv_snapshot!(context.filters(), context.venv().arg("-p").arg("3.12")
        .arg(context.venv.as_os_str()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Using CPython 3.12.11
    Creating virtual environment at: .venv
    Activate with: source .venv/[BIN]/activate
    "
    );

    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version"), @"
    exit_code: 0 (success)
    ----- stdout -----
    Python 3.12.11
    "
    );

    // Uninstall the highest patch version
    uv_snapshot!(context.filters(), context.python_uninstall().arg("--preview").arg("3.12.11"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Searching for Python versions matching: Python 3.12.11
    Uninstalled Python 3.12.11 in [TIME]
     - cpython-3.12.11-[PLATFORM] (python3.12)
    "
    );

    // Virtual environment should be on highest patch version remaining.
    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version"), @"
    exit_code: 0 (success)
    ----- stdout -----
    Python 3.12.9
    "
    );
}

// Virtual environments only record minor versions. `uv venv -p 3.x.y` will
// not prevent a virtual environment from tracking the latest patch version
// installed.
#[test]
fn install_no_transparent_upgrade_with_venv_patch_specification() {
    let context = uv_test::test_context_with_versions!(&["3.13"])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs()
        .with_filtered_python_install_bin();

    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.12.9"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.12.9 in [TIME]
     + cpython-3.12.9-[PLATFORM] (python3.12)
    "
    );

    // Create a virtual environment with a patch version
    uv_snapshot!(context.filters(), context.venv().arg("-p").arg("3.12.9")
        .arg(context.venv.as_os_str()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Using CPython 3.12.9
    Creating virtual environment at: .venv
    Activate with: source .venv/[BIN]/activate
    "
    );

    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version"), @"
    exit_code: 0 (success)
    ----- stdout -----
    Python 3.12.9
    "
    );

    // Install a higher patch version.
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.12.11"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.12.11 in [TIME]
     + cpython-3.12.11-[PLATFORM] (python3.12)
    "
    );

    // The virtual environment Python version is transparently upgraded.
    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version"), @"
    exit_code: 0 (success)
    ----- stdout -----
    Python 3.12.9
    "
    );
}

// A virtual environment created using the `venv` module should track
// the latest patch version installed.
#[test]
fn install_transparent_patch_upgrade_venv_module() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs()
        .with_filtered_python_install_bin();

    let bin_dir = context.temp_dir.child("bin");

    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.12.9"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.12.9 in [TIME]
     + cpython-3.12.9-[PLATFORM] (python3.12)
    "
    );

    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version"), @"
    exit_code: 0 (success)
    ----- stdout -----
    Python 3.12.9
    "
    );

    // Create a virtual environment using venv module.
    uv_snapshot!(context.filters(), context.run().arg("python").arg("-m").arg("venv").arg(context.venv.as_os_str()).arg("--without-pip")
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    exit_code: 0 (success)
    ");

    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version"), @"
    exit_code: 0 (success)
    ----- stdout -----
    Python 3.12.9
    "
    );

    // Install a higher patch version
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.12.11"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.12.11 in [TIME]
     + cpython-3.12.11-[PLATFORM] (python3.12)
    "
    );

    // Virtual environment should reflect highest patch version.
    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version"), @"
    exit_code: 0 (success)
    ----- stdout -----
    Python 3.12.11
    "
    );
}

// Automatically installing a lower patch version when running a command like
// `uv run` should not downgrade virtual environments.
#[test]
fn install_lower_patch_automatically() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs()
        .with_filtered_python_install_bin();

    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.12.11"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.12.11 in [TIME]
     + cpython-3.12.11-[PLATFORM] (python3.12)
    "
    );

    uv_snapshot!(context.filters(), context.venv().arg("-p").arg("3.12")
        .arg(context.venv.as_os_str()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Using CPython 3.12.11
    Creating virtual environment at: .venv
    Activate with: source .venv/[BIN]/activate
    "
    );

    uv_snapshot!(context.filters(), context.init().arg("-p").arg("3.12.9").arg("proj"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Initialized project `proj` at `[TEMP_DIR]/proj`
    "
    );

    // Create a new virtual environment to trigger automatic installation of
    // lower patch version
    uv_snapshot!(context.filters(), context.venv()
        .arg("--directory").arg("proj")
        .arg("-p").arg("3.12.9"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Using CPython 3.12.9
    Creating virtual environment at: .venv
    Activate with: source .venv/[BIN]/activate
    ");

    // Original virtual environment should still point to higher patch
    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version"), @"
    exit_code: 0 (success)
    ----- stdout -----
    Python 3.12.11
    "
    );
}

#[test]
fn uninstall_last_patch() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs()
        .with_filtered_virtualenv_bin();

    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.10.17"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.10.17 in [TIME]
     + cpython-3.10.17-[PLATFORM] (python3.10)
    "
    );

    uv_snapshot!(context.filters(), context.venv().arg("-p").arg("3.10"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Using CPython 3.10.17
    Creating virtual environment at: .venv
    Activate with: source .venv/[BIN]/activate
    "
    );

    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version"), @"
    exit_code: 0 (success)
    ----- stdout -----
    Python 3.10.17
    "
    );

    uv_snapshot!(context.filters(), context.python_uninstall().arg("--preview").arg("3.10.17"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Searching for Python versions matching: Python 3.10.17
    Uninstalled Python 3.10.17 in [TIME]
     - cpython-3.10.17-[PLATFORM] (python3.10)
    "
    );

    let context = context.with_filter(("python3", "python"));

    #[cfg(unix)]
    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Failed to inspect Python interpreter from active virtual environment at `.venv/[BIN]/python`
      cause: Broken symlink at `.venv/[BIN]/python`, was the underlying Python interpreter removed?

    hint: Consider recreating the environment (e.g., with `uv venv`)
    "
    );

    #[cfg(windows)]
    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version"), @r"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Failed to inspect Python interpreter from active virtual environment at `.venv/[BIN]/python`
      cause: Python interpreter not found at `[VENV]/[BIN]/python`
    "
    );
}

/// After uninstalling the last patch for a minor version, the minor version link
/// (symlink on Unix, junction on Windows) should be removed.
///
/// Regression test for <https://github.com/astral-sh/uv/issues/18793>.
/// This now backstops the upgrade to `junction` >=2, which can read dangling junctions.
#[test]
fn uninstall_last_patch_removes_minor_version_link() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs()
        .with_filtered_python_install_bin();

    let managed_dir = context.temp_dir.child("managed");
    let platform_key = platform_key_from_env().unwrap();

    let minor_version_link = managed_dir.child(format!("cpython-3.12-{platform_key}"));
    let patch_dir = managed_dir.child(format!("cpython-3.12.8-{platform_key}"));

    // Install a single patch version
    uv_snapshot!(context.filters(), context.python_install().arg("3.12.8"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.12.8 in [TIME]
     + cpython-3.12.8-[PLATFORM] (python3.12)
    ");

    // The patch directory and the minor version link should both exist
    patch_dir.assert(predicate::path::exists());
    minor_version_link.assert(predicate::path::exists());

    // Uninstall the only patch version for this minor
    uv_snapshot!(context.filters(), context.python_uninstall().arg("3.12.8"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Searching for Python versions matching: Python 3.12.8
    Uninstalled Python 3.12.8 in [TIME]
     - cpython-3.12.8-[PLATFORM] (python3.12)
    ");

    // The patch directory should be removed
    patch_dir.assert(predicate::path::missing());

    // The minor version link (symlink/junction) itself should be fully removed,
    // not just dangling. We use `symlink_metadata` because `Path::exists` follows
    // symlinks/junctions and would return false for a dangling link, hiding the bug.
    assert!(
        minor_version_link.path().symlink_metadata().is_err(),
        "minor version link should be removed after uninstalling the last patch, \
         but it still exists at: {}",
        minor_version_link.path().display()
    );
}

/// After uninstalling the highest patch but with other patches remaining,
/// the minor version link should be updated (not removed).
#[test]
fn uninstall_highest_patch_updates_minor_version_link() {
    use uv_python::managed::platform_key_from_env;

    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs()
        .with_filtered_python_install_bin();

    let managed_dir = context.temp_dir.child("managed");
    let platform_key = platform_key_from_env().unwrap();

    let minor_version_link = managed_dir.child(format!("cpython-3.12-{platform_key}"));
    let patch_dir_8 = managed_dir.child(format!("cpython-3.12.8-{platform_key}"));
    let patch_dir_9 = managed_dir.child(format!("cpython-3.12.9-{platform_key}"));

    // Install two patch versions
    uv_snapshot!(context.filters(), context.python_install().arg("3.12.9").arg("3.12.8"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed 2 versions in [TIME]
     + cpython-3.12.8-[PLATFORM]
     + cpython-3.12.9-[PLATFORM] (python3.12)
    ");

    // All directories should exist
    patch_dir_8.assert(predicate::path::exists());
    patch_dir_9.assert(predicate::path::exists());
    minor_version_link.assert(predicate::path::exists());

    // The minor version link should resolve to the highest patch (3.12.9).
    // Use `dunce::canonicalize` directly because `canonicalize_link_path` goes
    // through `launcher_path` on Windows, which only works for trampoline
    // executables, not junction directories.
    let link_target = dunce::canonicalize(minor_version_link.path())
        .unwrap()
        .simplified_display()
        .to_string();
    insta::with_settings!({
        filters => context.filters(),
    }, {
        insta::assert_snapshot!(
            link_target, @"[TEMP_DIR]/managed/cpython-3.12.9-[PLATFORM]"
        );
    });

    // Uninstall the highest patch version
    uv_snapshot!(context.filters(), context.python_uninstall().arg("3.12.9"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Searching for Python versions matching: Python 3.12.9
    Uninstalled Python 3.12.9 in [TIME]
     - cpython-3.12.9-[PLATFORM] (python3.12)
    ");

    // The highest patch dir should be removed
    patch_dir_9.assert(predicate::path::missing());

    // The lower patch dir should still exist
    patch_dir_8.assert(predicate::path::exists());

    // The minor version link should still exist, now pointing to the remaining patch
    minor_version_link.assert(predicate::path::exists());
    let link_target = dunce::canonicalize(minor_version_link.path())
        .unwrap()
        .simplified_display()
        .to_string();
    insta::with_settings!({
        filters => context.filters(),
    }, {
        insta::assert_snapshot!(
            link_target, @"[TEMP_DIR]/managed/cpython-3.12.8-[PLATFORM]"
        );
    });

    // Uninstall the last remaining patch
    uv_snapshot!(context.filters(), context.python_uninstall().arg("3.12.8"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Searching for Python versions matching: Python 3.12.8
    Uninstalled Python 3.12.8 in [TIME]
     - cpython-3.12.8-[PLATFORM]
    ");

    // The patch directory should be removed
    patch_dir_8.assert(predicate::path::missing());

    // The minor version link should be fully removed (see comment in
    // `uninstall_last_patch_removes_minor_version_link` for why we use
    // `symlink_metadata` instead of `predicate::path::missing`).
    assert!(
        minor_version_link.path().symlink_metadata().is_err(),
        "minor version link should be removed after uninstalling the last patch, \
         but it still exists at: {}",
        minor_version_link.path().display()
    );
}

#[cfg(unix)] // Pyodide cannot be used on Windows
#[test]
fn python_install_pyodide() {
    use assert_cmd::assert::OutputAssertExt;

    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_exe_suffix()
        .with_managed_python_dirs()
        .with_filtered_latest_python_versions();

    uv_snapshot!(context.filters(), context.python_install().arg("cpython-3.13.2-emscripten-wasm32-musl"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.13.2 in [TIME]
     + pyodide-3.13.2-emscripten-wasm32-musl (pyodide3.13)
    ");

    let bin_python = context
        .bin_dir
        .child(format!("pyodide3.13{}", std::env::consts::EXE_SUFFIX));

    // The executable should be installed in the bin directory
    bin_python.assert(predicate::path::exists());

    // It should be a link
    bin_python.assert(predicate::path::is_symlink());

    // The link should be a path to the binary
    insta::with_settings!({
        filters => context.filters(),
    }, {
        insta::assert_snapshot!(
            read_link(&bin_python), @"[TEMP_DIR]/managed/pyodide-3.13.2-emscripten-wasm32-musl/python"
        );
    });

    // The executable should "work"
    uv_snapshot!(context.filters(), Command::new(bin_python.as_os_str())
        .arg("-c").arg("import subprocess; print('hello world')"), @"
    exit_code: 0 (success)
    ----- stdout -----
    hello world
    ");

    // We should be able to find the Pyodide interpreter
    uv_snapshot!(context.filters(), context.python_find().arg("cpython-3.13.2-emscripten-wasm32-musl"), @"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/managed/pyodide-3.13.2-emscripten-wasm32-musl/python
    ");

    // We should be able to create a virtual environment with it
    uv_snapshot!(context.filters(), context.venv().arg("--python").arg("cpython-3.13.2-emscripten-wasm32-musl"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Using CPython 3.13.2
    Creating virtual environment at: .venv
    Activate with: source .venv/[BIN]/activate
    ");

    // We should be able to run the Python in the virtual environment
    uv_snapshot!(context.filters(), context.python_command().arg("-c").arg("import subprocess; print('hello world')"), @"
    exit_code: 0 (success)
    ----- stdout -----
    hello world
    ");

    context.python_uninstall().arg("--all").assert().success();
    fs_err::remove_dir_all(&context.venv).unwrap();

    // Install via `pyodide`
    uv_snapshot!(context.filters(), context.python_install().arg("pyodide"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.14.2 in [TIME]
     + pyodide-3.14.2-emscripten-wasm32-musl (pyodide3.14)
    ");

    context.python_uninstall().arg("--all").assert().success();

    // Install via `pyodide@<version>`
    uv_snapshot!(context.filters(), context.python_install().arg("pyodide@3.13"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.13.2 in [TIME]
     + pyodide-3.13.2-emscripten-wasm32-musl (pyodide3.13)
    ");

    // Find via `pyodide``
    uv_snapshot!(context.filters(), context.python_find().arg("pyodide"), @"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/managed/pyodide-3.13.2-emscripten-wasm32-musl/python
    ");

    // Find without a request should fail
    uv_snapshot!(context.filters(), context.python_find(), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: No interpreter found in virtual environments, managed installations, or search path
    ");
    // Find with "cpython" should also fail
    uv_snapshot!(context.filters(), context.python_find().arg("cpython"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: No interpreter found for CPython in virtual environments, managed installations, or search path
    ");

    // Install a CPython interpreter
    let context = context.with_filtered_python_keys();
    uv_snapshot!(context.filters(), context.python_install().arg("cpython"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.14.[LATEST] in [TIME]
     + cpython-3.14.[LATEST]-[PLATFORM] (python3.14)
    ");

    // Now, we should prefer that
    uv_snapshot!(context.filters(), context.python_find().arg("any"), @"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.14-[PLATFORM]/bin/python3.14
    ");

    // Unless we request pyodide
    uv_snapshot!(context.filters(), context.python_find().arg("pyodide"), @"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/managed/pyodide-3.13.2-emscripten-wasm32-musl/python
    ");
}

#[test]
fn python_install_build_version() {
    use uv_python::managed::platform_key_from_env;

    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_managed_python_dirs()
        .with_filtered_python_sources()
        .with_filtered_python_install_bin()
        .with_filtered_python_names()
        .with_filtered_exe_suffix();

    uv_snapshot!(context.filters(), context.python_install()
        .arg("3.12")
        .env(EnvVars::UV_PYTHON_CPYTHON_BUILD, "20240814"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.12.5 in [TIME]
     + cpython-3.12.5-[PLATFORM] (python3.12)
    ");

    // A BUILD file should be present with the version
    let cpython_dir = context.temp_dir.child("managed").child(format!(
        "cpython-3.12.5-{}",
        platform_key_from_env().unwrap()
    ));
    let build_file_path = cpython_dir.join("BUILD");
    let build_content = fs_err::read_to_string(&build_file_path).unwrap();
    assert_eq!(build_content, "20240814");

    // We should find the build
    uv_snapshot!(context.filters(), context.python_find()
        .arg("3.12")
        .env(EnvVars::UV_PYTHON_CPYTHON_BUILD, "20240814"), @"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/managed/cpython-3.12-[PLATFORM]/[INSTALL-BIN]/[PYTHON]
    ");

    // If the build number does not match, we should ignore the installation
    uv_snapshot!(context.filters(), context.python_find()
        .arg("3.12")
        .env(EnvVars::UV_PYTHON_CPYTHON_BUILD, "99999999"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: No interpreter found for Python 3.12 in [PYTHON SOURCES]
    ");

    // If there's no install for a build number, we should fail
    uv_snapshot!(context.filters(), context.python_install()
        .arg("3.12")
        .env(EnvVars::UV_PYTHON_CPYTHON_BUILD, "99999999"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: No download found for request: cpython-3.12-[PLATFORM]
    ");

    // Requesting a specific patch version without a matching build number should fail
    uv_snapshot!(context.filters(), context.python_install()
        .arg("3.12.10")
        .env(EnvVars::UV_PYTHON_CPYTHON_BUILD, "20250814"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: No download found for request: cpython-3.12.10-[PLATFORM]
    ");
}

#[test]
fn python_install_build_version_pypy() {
    use uv_python::managed::platform_key_from_env;

    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_python_sources()
        .with_managed_python_dirs()
        .with_filtered_python_install_bin()
        .with_filtered_python_names()
        .with_filtered_exe_suffix();

    uv_snapshot!(context.filters(), context.python_install()
        .arg("pypy3.10")
        .env(EnvVars::UV_PYTHON_PYPY_BUILD, "7.3.19"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.10.16 in [TIME]
     + pypy-3.10.16-[PLATFORM] (pypy3.10)
    ");

    // A BUILD file should be present with the version
    let pypy_dir = context
        .temp_dir
        .child("managed")
        .child(format!("pypy-3.10.16-{}", platform_key_from_env().unwrap()));
    let build_file_path = pypy_dir.join("BUILD");
    let build_content = fs_err::read_to_string(&build_file_path).unwrap();
    assert_eq!(build_content, "7.3.19");

    // We should find the build
    uv_snapshot!(context.filters(), context.python_find()
        .arg("pypy3.10")
        .env(EnvVars::UV_PYTHON_PYPY_BUILD, "7.3.19"), @"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/managed/pypy-3.10.16-[PLATFORM]/[INSTALL-BIN]/[PYPY]
    ");

    // If the build number does not match, we should ignore the installation
    uv_snapshot!(context.filters(), context.python_find()
        .arg("pypy3.10")
        .env(EnvVars::UV_PYTHON_PYPY_BUILD, "99.99.99"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: No interpreter found for PyPy 3.10 in [PYTHON SOURCES]
    ");

    // If there's no install for a build number, we should fail
    uv_snapshot!(context.filters(), context.python_install()
        .arg("pypy3.10")
        .env(EnvVars::UV_PYTHON_PYPY_BUILD, "99.99.99"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: No download found for request: pypy-3.10-[PLATFORM]
    ");
}

#[test]
fn python_install_upgrade() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs()
        .with_filtered_latest_python_versions();

    // Provide `--upgrade` as an `install` option without any versions
    uv_snapshot!(context.filters(), context.python_install().arg("--upgrade"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.14.[LATEST] in [TIME]
     + cpython-3.14.[LATEST]-[PLATFORM] (python3.14)
    ");

    // Provide `--upgrade` as an `install` option without any versions again!
    uv_snapshot!(context.filters(), context.python_install().arg("--upgrade"), @"
    exit_code: 0 (success)
    ----- stderr -----
    The default Python installation is already on the latest supported patch release. Use `uv python install <request>` to install another version.
    ");

    // Install an earlier patch version
    uv_snapshot!(context.filters(), context.python_install().arg("3.10.17"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.10.17 in [TIME]
     + cpython-3.10.17-[PLATFORM] (python3.10)
    ");

    // Upgrade an outdated patch even when a reinstall is requested.
    uv_snapshot!(context.filters(), context.python_install().arg("--upgrade").arg("--reinstall").arg("3.10"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.10.[LATEST] in [TIME]
     + cpython-3.10.[LATEST]-[PLATFORM] (python3.10)
    ");

    // Reinstall only the latest patch, leaving older installed patches untouched.
    uv_snapshot!(context.filters(), context.python_install().arg("--upgrade").arg("--reinstall").arg("3.10"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.10.[LATEST] in [TIME]
     ~ cpython-3.10.[LATEST]-[PLATFORM] (python3.10)
    ");

    // Request a patch version with `--upgrade`
    uv_snapshot!(context.filters(), context.python_install().arg("--upgrade").arg("3.11.4"), @"
    exit_code: 1 (failure)
    ----- stderr -----
    error: `uv python install --upgrade` only accepts minor versions, got: 3.11.4
    ");

    // Request a version that isn't installed yet
    uv_snapshot!(context.filters(), context.python_install().arg("--upgrade").arg("3.11"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.11.[LATEST] in [TIME]
     + cpython-3.11.[LATEST]-[PLATFORM] (python3.11)
    ");

    // Ask for it again
    uv_snapshot!(context.filters(), context.python_install().arg("--upgrade").arg("3.11"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Python 3.11 is already on the latest supported patch release
    ");

    // Install an outdated version
    uv_snapshot!(context.filters(), context.python_install().arg("3.9.5"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.9.5 in [TIME]
     + cpython-3.9.5-[PLATFORM] (python3.9)
    ");

    // We shouldn't update it when not relevant
    uv_snapshot!(context.filters(), context.python_install().arg("--upgrade").arg("3.11"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Python 3.11 is already on the latest supported patch release
    ");

    // Ask for multiple already satisfied versions
    uv_snapshot!(context.filters(), context.python_install().arg("--upgrade").arg("3.10").arg("3.11"), @"
    exit_code: 0 (success)
    ----- stderr -----
    All requested versions already on latest supported patch release
    ");

    // Mix in an unsatisfied version and a missing one
    uv_snapshot!(context.filters(), context.python_install().arg("--upgrade").arg("3.9").arg("3.10").arg("3.11").arg("3.12"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed 2 versions in [TIME]
     + cpython-3.9.25-[PLATFORM] (python3.9)
     + cpython-3.12.[LATEST]-[PLATFORM] (python3.12)
    ");
}

#[test]
fn python_install_upgrade_version_file() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs()
        .with_filtered_latest_python_versions();

    // Pin to a minor version
    context.python_pin().arg("3.13").assert().success();

    // Provide `--upgrade` as an `install` option without any versions
    uv_snapshot!(context.filters(), context.python_install().arg("--upgrade"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.13.[LATEST] in [TIME]
     + cpython-3.13.[LATEST]-[PLATFORM] (python3.13)
    ");

    // Provide `--upgrade` as an `install` option without any versions again!
    uv_snapshot!(context.filters(), context.python_install().arg("--upgrade"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Python 3.13 is already on the latest supported patch release
    ");

    // Pin to a patch version
    context.python_pin().arg("3.12.4").assert().success();

    // Provide `--upgrade` as an `install` option without any versions
    uv_snapshot!(context.filters(), context.python_install().arg("--upgrade"), @"
    exit_code: 1 (failure)
    ----- stderr -----
    error: `uv python install --upgrade` only accepts minor versions, got: 3.12.4

    hint: The version request came from a `.python-version` file; change the patch version in the file to upgrade instead
    ");
}

#[test]
fn python_install_armv7() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_managed_python_dirs()
        .with_filtered_python_sources()
        .with_filtered_python_install_bin()
        .with_filtered_python_names()
        .with_filtered_exe_suffix();

    // Explicitly request a musl build for armv7l
    uv_snapshot!(context.filters(), context.python_install().arg("cpython-3.12.12-linux-armv7-musl"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: uv does not yet provide musl Python distributions on armv7.
    ");

    // Explicitly request a gnuabi build for armv7l
    uv_snapshot!(context.filters(), context.python_install().arg("cpython-3.12.12-linux-armv7-gnueabi"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.12.12 in [TIME]
     + cpython-3.12.12-[PLATFORM] (python3.12)
    ");

    context.python_uninstall().arg("--all").assert().success();

    // Exercise the tar-codec symlink handling against the armv7 archive that contains duplicate
    // terminfo symlinks.
    uv_snapshot!(context.filters(), context
        .python_install()
        .arg("--preview-features")
        .arg("tar-codec")
        .arg("cpython-3.12.12-linux-armv7-gnueabi"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.12.12 in [TIME]
     + cpython-3.12.12-[PLATFORM] (python3.12)
    ");
}

#[test]
fn python_install_compile_bytecode() -> anyhow::Result<()> {
    fn count_files_by_ext(dir: &Path, extension: &str) -> anyhow::Result<usize> {
        let mut count = 0;
        let walker = WalkDir::new(dir).into_iter();
        for entry in walker {
            let entry = entry?;
            let path = entry.path();
            if entry.metadata()?.is_file() && path.extension().is_some_and(|ext| ext == extension) {
                count += 1;
            }
        }
        Ok(count)
    }

    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_filtered_compiled_file_count()
        .with_managed_python_dirs()
        .with_empty_python_install_mirror()
        .with_filtered_latest_python_versions();

    // Install 3.14 and compile its bytecode
    uv_snapshot!(context.filters(), context.python_install().arg("--compile-bytecode").arg("3.14"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.14.[LATEST] in [TIME]
     + cpython-3.14.[LATEST]-[PLATFORM] (python3.14)
    Bytecode compiled [COUNT] files in [TIME]
    ");

    // Find the stdlib path for cpython 3.14
    let bin_path = context
        .bin_dir
        .child(format!("python3.14{}", std::env::consts::EXE_SUFFIX));

    #[cfg(unix)]
    let stdlib = fs_err::read_link(bin_path)?
        .parent()
        .context("Python binary should be a child of `bin`")?
        .parent()
        .context("`bin` directory should be a child of the installation path")?
        .join("lib")
        .join("python3.14");
    #[cfg(windows)]
    let stdlib = launcher_path(&bin_path)
        .parent()
        .context("Python binary should be a child of the installation path")?
        .join("Lib");

    // And the count should match
    let pyc_count = count_files_by_ext(&stdlib, "pyc")?;
    let py_count = count_files_by_ext(&stdlib, "py")?;
    assert_eq!(pyc_count, py_count);

    // Attempting to install with --compile-bytecode should (currently)
    // unconditionally re-run the bytecode compiler
    uv_snapshot!(context.filters(), context.python_install().arg("--compile-bytecode").arg("3.14"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Python 3.14 is already installed
    Bytecode compiled [COUNT] files in [TIME]
    ");

    // Reinstalling with --compile-bytecode should compile bytecode.
    uv_snapshot!(context.filters(), context.python_install().arg("--reinstall").arg("--compile-bytecode").arg("3.14"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.14.[LATEST] in [TIME]
     ~ cpython-3.14.[LATEST]-[PLATFORM] (python3.14)
    Bytecode compiled [COUNT] files in [TIME]
    ");

    Ok(())
}

#[test]
fn python_install_compile_bytecode_existing() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_filtered_compiled_file_count()
        .with_managed_python_dirs()
        .with_empty_python_install_mirror()
        .with_filtered_latest_python_versions();

    // A fresh install should be able to be compiled later
    uv_snapshot!(context.filters(), context.python_install().arg("3.14"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.14.[LATEST] in [TIME]
     + cpython-3.14.[LATEST]-[PLATFORM] (python3.14)
    ");

    uv_snapshot!(context.filters(), context.python_install().arg("--compile-bytecode").arg("3.14"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Python 3.14 is already installed
    Bytecode compiled [COUNT] files in [TIME]
    ");
}

#[test]
fn python_install_compile_bytecode_upgrade() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_filtered_compiled_file_count()
        .with_managed_python_dirs()
        .with_empty_python_install_mirror()
        .with_filtered_latest_python_versions();

    // An upgrade should also compile bytecode
    uv_snapshot!(context.filters(), context.python_install().arg("3.14.0"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.14.0 in [TIME]
     + cpython-3.14.0-[PLATFORM] (python3.14)
    ");

    uv_snapshot!(context.filters(), context.python_install().arg("--upgrade").arg("--compile-bytecode").arg("3.14"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.14.[LATEST] in [TIME]
     + cpython-3.14.[LATEST]-[PLATFORM] (python3.14)
    Bytecode compiled [COUNT] files in [TIME]
    ");
}

#[test]
fn python_install_upgrade_build_version() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs()
        .with_filtered_latest_python_versions();

    // Install Python 3.12
    uv_snapshot!(context.filters(), context.python_install().arg("3.12"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.12.[LATEST] in [TIME]
     + cpython-3.12.[LATEST]-[PLATFORM] (python3.12)
    ");

    // Should be a no-op when already installed at latest version
    uv_snapshot!(context.filters(), context.python_install().arg("--upgrade").arg("3.12"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Python 3.12 is already on the latest supported patch release
    ");

    // Overwrite the BUILD file with an older build version
    let installation_dir = context.temp_dir.child("managed").child(format!(
        "cpython-{}-{}",
        LATEST_PYTHON_3_12,
        platform_key_from_env().unwrap()
    ));
    let build_file = installation_dir.join("BUILD");
    fs_err::write(&build_file, "19000101").unwrap();
    // A build upgrade retains the Python version and installation key, so set the interpreter's
    // modification time to a known value to detect whether the interpreter is replaced.
    let python_executable = if cfg!(windows) {
        installation_dir.join("python.exe")
    } else {
        installation_dir.join("bin").join("python3.12")
    };
    filetime::set_file_mtime(
        &python_executable,
        filetime::FileTime::from_unix_time(1_700_000_000, 0),
    )
    .unwrap();
    let previous_mtime = filetime::FileTime::from_last_modification_time(
        &fs_err::metadata(&python_executable).unwrap(),
    );

    // Now upgrade should detect the outdated build version and reinstall
    uv_snapshot!(context.filters(), context.python_install().arg("--upgrade").arg("3.12"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.12.[LATEST] in [TIME]
     ~ cpython-3.12.[LATEST]-[PLATFORM]
    ");

    assert_ne!(
        filetime::FileTime::from_last_modification_time(
            &fs_err::metadata(&python_executable).unwrap()
        ),
        previous_mtime
    );

    // Should be a no-op again after upgrade
    uv_snapshot!(context.filters(), context.python_install().arg("--upgrade").arg("3.12"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Python 3.12 is already on the latest supported patch release
    ");
}

#[test]
fn python_install_compile_bytecode_multiple() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_filtered_compiled_file_count()
        .with_managed_python_dirs()
        .with_empty_python_install_mirror()
        .with_filtered_latest_python_versions();

    // Should handle installing and compiling multiple versions correctly
    uv_snapshot!(context.filters(), context.python_install().arg("--compile-bytecode").arg("3.14").arg("3.12"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed 2 versions in [TIME]
     + cpython-3.12.[LATEST]-[PLATFORM] (python3.12)
     + cpython-3.14.[LATEST]-[PLATFORM] (python3.14)
    Bytecode compiled [COUNT] files in [TIME]
    ");
}

#[cfg(unix)] // Pyodide cannot be used on Windows
#[test]
fn python_install_compile_bytecode_pyodide() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_filtered_compiled_file_count()
        .with_managed_python_dirs()
        .with_empty_python_install_mirror();

    // Should warn on explicit pyodide installation
    uv_snapshot!(context.filters(), context.python_install().arg("--compile-bytecode").arg("cpython-3.13.2-emscripten-wasm32-musl"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.13.2 in [TIME]
     + pyodide-3.13.2-emscripten-wasm32-musl (pyodide3.13)
    No compatible versions to bytecode compile (skipped 1)
    ");

    // TODO(tk) There's a bug with python_upgrade when pyodide is installed which leads to
    // `error: No download found for request: pyodide-3.13-emscripten-wasm32-musl`
    //// Recompilation where pyodide isn't explicitly specified shouldn't warn
    //uv_snapshot!(context.filters(), context.python_upgrade().arg("--compile-bytecode"), @r"TODO");
}

#[test]
fn python_install_compile_bytecode_graalpy() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_filtered_compiled_file_count()
        .with_managed_python_dirs()
        .with_empty_python_install_mirror();

    // Should work for graalpy
    uv_snapshot!(context.filters(), context.python_install().arg("--compile-bytecode").arg("graalpy-3.12"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.12.0 in [TIME]
     + graalpy-3.12.0-[PLATFORM] (graalpy3.12)
    Bytecode compiled [COUNT] files in [TIME]
    ");
}

#[test]
fn python_install_compile_bytecode_pypy() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_filtered_compiled_file_count()
        .with_managed_python_dirs()
        .with_empty_python_install_mirror();

    // Should work for pypy
    uv_snapshot!(context.filters(), context.python_install().arg("--compile-bytecode").arg("pypy-3.11"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.11.15 in [TIME]
     + pypy-3.11.15-[PLATFORM] (pypy3.11)
    Bytecode compiled [COUNT] files in [TIME]
    ");
}
