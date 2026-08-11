use anyhow::{Result, anyhow};
use assert_cmd::prelude::*;
use assert_fs::fixture::ChildPath;
use assert_fs::prelude::*;
use url::Url;

use uv_test::uv_snapshot;

#[test]
fn freeze_many() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("MarkupSafe==2.1.3\ntomli==2.0.1")?;

    // Run `pip sync`.
    context
        .pip_sync()
        .arg(requirements_txt.path())
        .assert()
        .success();

    // Run `pip freeze`.
    uv_snapshot!(context.pip_freeze()
        .arg("--strict"), @"
    exit_code: 0 (success)
    ----- stdout -----
    markupsafe==2.1.3
    tomli==2.0.1
    "
    );

    Ok(())
}

/// List a package with multiple installed distributions in a virtual environment.
#[test]
#[cfg(unix)]
fn freeze_duplicate() -> Result<()> {
    use uv_fs::copy_dir_all;

    // Sync a version of `pip` into a virtual environment.
    let context1 = uv_test::test_context!("3.12");
    let requirements_txt = context1.temp_dir.child("requirements.txt");
    requirements_txt.write_str("pip==21.3.1")?;

    // Run `pip sync`.
    context1
        .pip_sync()
        .arg(requirements_txt.path())
        .assert()
        .success();

    // Sync a different version of `pip` into a virtual environment.
    let context2 = uv_test::test_context!("3.12");
    let requirements_txt = context2.temp_dir.child("requirements.txt");
    requirements_txt.write_str("pip==22.1.1")?;

    // Run `pip sync`.
    context2
        .pip_sync()
        .arg(requirements_txt.path())
        .assert()
        .success();

    // Copy the virtual environment to a new location.
    copy_dir_all(
        context2.site_packages().join("pip-22.1.1.dist-info"),
        context1.site_packages().join("pip-22.1.1.dist-info"),
    )?;

    // Run `pip freeze`.
    uv_snapshot!(context1.filters(), context1.pip_freeze().arg("--strict"), @"
    exit_code: 0 (success)
    ----- stdout -----
    pip==21.3.1
    pip==22.1.1

    ----- stderr -----
    warning: The package `pip` has multiple installed distributions:
      - [SITE_PACKAGES]/pip-21.3.1.dist-info
      - [SITE_PACKAGES]/pip-22.1.1.dist-info
    "
    );

    Ok(())
}

/// List a direct URL package in a virtual environment.
#[test]
fn freeze_url() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("anyio\niniconfig @ https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl")?;

    // Run `pip sync`.
    context
        .pip_sync()
        .arg(requirements_txt.path())
        .assert()
        .success();

    // Run `pip freeze`.
    uv_snapshot!(context.pip_freeze()
        .arg("--strict"), @"
    exit_code: 0 (success)
    ----- stdout -----
    anyio==4.3.0
    iniconfig @ https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl

    ----- stderr -----
    warning: The package `anyio` requires `idna>=2.8`, but it's not installed
    warning: The package `anyio` requires `sniffio>=1.1`, but it's not installed
    "
    );

    Ok(())
}

/// Preserve archive hashes recorded by another installer in `direct_url.json` so that frozen
/// requirements keep their artifact verification.
#[test]
fn freeze_direct_archive_hashes() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let site_packages = ChildPath::new(context.site_packages());

    let project = site_packages.child("project-1.0.0.dist-info");
    project.create_dir_all()?;
    project
        .child("METADATA")
        .write_str("Metadata-Version: 2.1\nName: project\nVersion: 1.0.0\n")?;
    project.child("direct_url.json").write_str(
        r#"{"url":"https://example.com/project-1.0.0.tar.gz","subdirectory":"src","archive_info":{"hashes":{"sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}}}"#,
    )?;

    let legacy = site_packages.child("legacy-1.0.0.dist-info");
    legacy.create_dir_all()?;
    legacy
        .child("METADATA")
        .write_str("Metadata-Version: 2.1\nName: legacy\nVersion: 1.0.0\n")?;
    legacy.child("direct_url.json").write_str(
        r#"{"url":"https://example.com/legacy-1.0.0.tar.gz","archive_info":{"hash":"sha256=bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}}"#,
    )?;

    uv_snapshot!(context.pip_freeze(), @"
    exit_code: 0 (success)
    ----- stdout -----
    legacy @ https://example.com/legacy-1.0.0.tar.gz#sha256=bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
    project @ https://example.com/project-1.0.0.tar.gz#subdirectory=src&sha256=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
    ");

    Ok(())
}

/// A frozen direct URL with both a subdirectory and archive hash must keep enforcing that hash
/// when it is consumed as a requirement again.
#[test]
fn freeze_direct_archive_hash_roundtrip() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let site_packages = ChildPath::new(context.site_packages());
    let wheel_url = Url::from_file_path(
        context
            .workspace_root
            .join("test/links/ok-1.0.0-py3-none-any.whl"),
    )
    .map_err(|()| anyhow!("Failed to create wheel URL"))?;

    let ok = site_packages.child("ok-1.0.0.dist-info");
    ok.create_dir_all()?;
    ok.child("METADATA")
        .write_str("Metadata-Version: 2.1\nName: ok\nVersion: 1.0.0\n")?;
    ok.child("direct_url.json").write_str(&format!(
        r#"{{"url":"{wheel_url}","subdirectory":"src","archive_info":{{"hashes":{{"sha256":"79f0b33e6ce1e09eaa1784c8eee275dfe84d215d9c65c652f07c18e85fdaac5f"}}}}}}"#,
    ))?;

    let frozen = uv_snapshot!(context.filters(), context.pip_freeze(), @"
    exit_code: 0 (success)
    ----- stdout -----
    ok @ file://[WORKSPACE]/test/links/ok-1.0.0-py3-none-any.whl#subdirectory=src&sha256=79f0b33e6ce1e09eaa1784c8eee275dfe84d215d9c65c652f07c18e85fdaac5f
    ");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_binary(&frozen.stdout)?;
    fs_err::remove_dir_all(ok.path())?;

    context
        .pip_install()
        .arg("-r")
        .arg(requirements_txt.path())
        .arg("--no-deps")
        .arg("--require-hashes")
        .assert()
        .success();

    requirements_txt.write_str(&String::from_utf8(frozen.stdout)?.replace(
        "79f0b33e6ce1e09eaa1784c8eee275dfe84d215d9c65c652f07c18e85fdaac5f",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    ))?;

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-r")
        .arg(requirements_txt.path())
        .arg("--no-deps")
        .arg("--require-hashes")
        .arg("--reinstall"), @"
    exit_code: 1 (failure)
    ----- stderr -----
    Resolved 1 package in [TIME]
      × Failed to read `ok @ file://[WORKSPACE]/test/links/ok-1.0.0-py3-none-any.whl#subdirectory=src&sha256=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa`
      ╰─▶ Hash mismatch for `ok @ file://[WORKSPACE]/test/links/ok-1.0.0-py3-none-any.whl#subdirectory=src&sha256=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa`

          Expected:
            sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa

          Computed:
            sha256:79f0b33e6ce1e09eaa1784c8eee275dfe84d215d9c65c652f07c18e85fdaac5f
    ");

    Ok(())
}

#[test]
fn freeze_with_editable() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(&format!(
        "anyio\n-e {}",
        context
            .workspace_root
            .join("test/packages/poetry_editable")
            .display()
    ))?;

    // Run `pip sync`.
    context
        .pip_sync()
        .arg(requirements_txt.path())
        .assert()
        .success();

    // Run `pip freeze`.
    uv_snapshot!(context.filters(), context.pip_freeze()
        .arg("--strict"), @"
    exit_code: 0 (success)
    ----- stdout -----
    anyio==4.3.0
    -e file://[WORKSPACE]/test/packages/poetry_editable

    ----- stderr -----
    warning: The package `anyio` requires `idna>=2.8`, but it's not installed
    warning: The package `anyio` requires `sniffio>=1.1`, but it's not installed
    "
    );

    // Exclude editable package.
    uv_snapshot!(context.filters(), context.pip_freeze()
        .arg("--exclude-editable")
        .arg("--strict"), @"
    exit_code: 0 (success)
    ----- stdout -----
    anyio==4.3.0

    ----- stderr -----
    warning: The package `anyio` requires `idna>=2.8`, but it's not installed
    warning: The package `anyio` requires `sniffio>=1.1`, but it's not installed
    "
    );

    Ok(())
}

/// Show an `.egg-info` package in a virtual environment.
#[test]
fn freeze_with_egg_info() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let site_packages = ChildPath::new(context.site_packages());

    // Manually create an `.egg-info` directory.
    site_packages
        .child("zstandard-0.22.0-py3.12.egg-info")
        .create_dir_all()?;
    site_packages
        .child("zstandard-0.22.0-py3.12.egg-info")
        .child("top_level.txt")
        .write_str("zstd")?;
    site_packages
        .child("zstandard-0.22.0-py3.12.egg-info")
        .child("SOURCES.txt")
        .write_str("")?;
    site_packages
        .child("zstandard-0.22.0-py3.12.egg-info")
        .child("PKG-INFO")
        .write_str("")?;
    site_packages
        .child("zstandard-0.22.0-py3.12.egg-info")
        .child("dependency_links.txt")
        .write_str("")?;
    site_packages
        .child("zstandard-0.22.0-py3.12.egg-info")
        .child("entry_points.txt")
        .write_str("")?;

    // Manually create the package directory.
    site_packages.child("zstd").create_dir_all()?;
    site_packages
        .child("zstd")
        .child("__init__.py")
        .write_str("")?;

    // Run `pip freeze`.
    uv_snapshot!(context.filters(), context.pip_freeze(), @"
    exit_code: 0 (success)
    ----- stdout -----
    zstandard==0.22.0
    ");

    Ok(())
}

/// Show an `.egg-info` package in a virtual environment. In this case, the filename omits the
/// Python version.
#[test]
fn freeze_with_egg_info_no_py() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let site_packages = ChildPath::new(context.site_packages());

    // Manually create an `.egg-info` directory.
    site_packages
        .child("zstandard-0.22.0.egg-info")
        .create_dir_all()?;
    site_packages
        .child("zstandard-0.22.0.egg-info")
        .child("top_level.txt")
        .write_str("zstd")?;
    site_packages
        .child("zstandard-0.22.0.egg-info")
        .child("SOURCES.txt")
        .write_str("")?;
    site_packages
        .child("zstandard-0.22.0.egg-info")
        .child("PKG-INFO")
        .write_str("")?;
    site_packages
        .child("zstandard-0.22.0.egg-info")
        .child("dependency_links.txt")
        .write_str("")?;
    site_packages
        .child("zstandard-0.22.0.egg-info")
        .child("entry_points.txt")
        .write_str("")?;

    // Manually create the package directory.
    site_packages.child("zstd").create_dir_all()?;
    site_packages
        .child("zstd")
        .child("__init__.py")
        .write_str("")?;

    // Run `pip freeze`.
    uv_snapshot!(context.filters(), context.pip_freeze(), @"
    exit_code: 0 (success)
    ----- stdout -----
    zstandard==0.22.0
    ");

    Ok(())
}

/// Show a set of `.egg-info` files in a virtual environment.
#[test]
fn freeze_with_egg_info_file() -> Result<()> {
    let context = uv_test::test_context!("3.11");
    let site_packages = ChildPath::new(context.site_packages());

    // Manually create a `.egg-info` file with python version.
    site_packages
        .child("pycurl-7.45.1-py3.11.egg-info")
        .write_str(indoc::indoc! {"
            Metadata-Version: 1.1
            Name: pycurl
            Version: 7.45.1
        "})?;

    // Manually create another `.egg-info` file with no python version.
    site_packages
        .child("vtk-9.2.6.egg-info")
        .write_str(indoc::indoc! {"
            Metadata-Version: 1.1
            Name: vtk
            Version: 9.2.6
        "})?;

    // Run `pip freeze`.
    uv_snapshot!(context.filters(), context.pip_freeze(), @"
    exit_code: 0 (success)
    ----- stdout -----
    pycurl==7.45.1
    vtk==9.2.6
    ");
    Ok(())
}

#[test]
fn freeze_with_legacy_editable() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let site_packages = ChildPath::new(context.site_packages());

    let target = context.temp_dir.child("zstandard_project");
    target.child("zstd").create_dir_all()?;
    target.child("zstd").child("__init__.py").write_str("")?;

    target.child("zstandard.egg-info").create_dir_all()?;
    target
        .child("zstandard.egg-info")
        .child("PKG-INFO")
        .write_str(
            "Metadata-Version: 2.1
Name: zstandard
Version: 0.22.0
",
        )?;

    site_packages
        .child("zstandard.egg-link")
        .write_str(target.path().to_str().unwrap())?;

    // Run `pip freeze`.
    uv_snapshot!(context.filters(), context.pip_freeze(), @"
    exit_code: 0 (success)
    ----- stdout -----
    -e [TEMP_DIR]/zstandard_project
    ");

    Ok(())
}

#[test]
fn freeze_path() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("MarkupSafe==2.1.3\ntomli==2.0.1")?;

    let target = context.temp_dir.child("install-path");

    // Run `pip sync`.
    context
        .pip_sync()
        .arg(requirements_txt.path())
        .arg("--target")
        .arg(target.path())
        .assert()
        .success();

    // Run `pip freeze`.
    uv_snapshot!(context.filters(), context.pip_freeze()
        .arg("--path")
        .arg(target.path()), @"
    exit_code: 0 (success)
    ----- stdout -----
    markupsafe==2.1.3
    tomli==2.0.1
    ");

    Ok(())
}

#[test]
fn freeze_multiple_paths() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let requirements_txt1 = context.temp_dir.child("requirements1.txt");
    requirements_txt1.write_str("MarkupSafe==2.1.3\ntomli==2.0.1")?;

    let requirements_txt2 = context.temp_dir.child("requirements2.txt");
    requirements_txt2.write_str("MarkupSafe==2.1.3\nrequests==2.31.0")?;

    let target1 = context.temp_dir.child("install-path1");
    let target2 = context.temp_dir.child("install-path2");

    // Run `pip sync`.
    for (target, requirements_txt) in [
        (target1.path(), requirements_txt1),
        (target2.path(), requirements_txt2),
    ] {
        context
            .pip_sync()
            .arg(requirements_txt.path())
            .arg("--target")
            .arg(target)
            .assert()
            .success();
    }

    // Run `pip freeze`.
    uv_snapshot!(context.filters(), context.pip_freeze().arg("--path").arg(target1.path()).arg("--path").arg(target2.path()), @"
    exit_code: 0 (success)
    ----- stdout -----
    markupsafe==2.1.3
    requests==2.31.0
    tomli==2.0.1
    ");

    Ok(())
}

// We follow pip in just ignoring nonexistent paths
#[test]
fn freeze_nonexistent_path() {
    let context = uv_test::test_context!("3.12");

    let nonexistent_dir = {
        let dir = context.temp_dir.child("blahblah");
        assert!(!dir.exists());
        dir
    };

    // Run `pip freeze`.
    uv_snapshot!(context.filters(), context.pip_freeze()
        .arg("--path")
        .arg(nonexistent_dir.path()), @"
    exit_code: 0 (success)
    ");
}

#[test]
fn freeze_with_quiet_flag() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("MarkupSafe==2.1.3\ntomli==2.0.1")?;

    // Run `pip sync`.
    context
        .pip_sync()
        .arg(requirements_txt.path())
        .assert()
        .success();

    // Run `pip freeze` with `--quiet` flag.
    uv_snapshot!(context.pip_freeze().arg("--quiet"), @"
    exit_code: 0 (success)
    ----- stdout -----
    markupsafe==2.1.3
    tomli==2.0.1
    "
    );

    Ok(())
}

#[test]
fn freeze_target() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("MarkupSafe==2.1.3\ntomli==2.0.1")?;

    let target = context.temp_dir.child("target");

    // Install packages to a target directory.
    context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--target")
        .arg(target.path())
        .assert()
        .success();

    // Freeze packages in the target directory.
    uv_snapshot!(context.filters(), context.pip_freeze()
        .arg("--target")
        .arg(target.path()), @"
    exit_code: 0 (success)
    ----- stdout -----
    markupsafe==2.1.3
    tomli==2.0.1
    "
    );

    // Without --target, the packages should not be visible.
    uv_snapshot!(context.pip_freeze(), @"
    exit_code: 0 (success)
    "
    );

    Ok(())
}

#[test]
fn freeze_prefix() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("MarkupSafe==2.1.3\ntomli==2.0.1")?;

    let prefix = context.temp_dir.child("prefix");

    // Install packages to a prefix directory.
    context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--prefix")
        .arg(prefix.path())
        .assert()
        .success();

    // Freeze packages in the prefix directory.
    uv_snapshot!(context.filters(), context.pip_freeze()
        .arg("--prefix")
        .arg(prefix.path()), @"
    exit_code: 0 (success)
    ----- stdout -----
    markupsafe==2.1.3
    tomli==2.0.1
    "
    );

    // Without --prefix, the packages should not be visible.
    uv_snapshot!(context.pip_freeze(), @"
    exit_code: 0 (success)
    "
    );

    Ok(())
}

#[test]
fn freeze_exclude() {
    let context = uv_test::test_context!("3.12");

    let prefix = context.temp_dir.child("prefix");

    // Install packages to a prefix directory.
    context
        .pip_install()
        .arg("MarkupSafe")
        .arg("tomli")
        .arg("--prefix")
        .arg(prefix.path())
        .assert()
        .success();

    // Run `pip freeze --exclude MarkupSafe`.
    uv_snapshot!(context.filters(), context.pip_freeze().arg("--exclude").arg("MarkupSafe").arg("--prefix").arg(prefix.path()), @"
    exit_code: 0 (success)
    ----- stdout -----
    tomli==2.0.1
    "
    );

    // Run `pip freeze --exclude MarkupSafe --exclude tomli`.
    uv_snapshot!(context.filters(), context.pip_freeze().arg("--exclude").arg("MarkupSafe").arg("--exclude").arg("tomli").arg("--prefix").arg(prefix.path()), @"
    exit_code: 0 (success)
    "
    );
}
