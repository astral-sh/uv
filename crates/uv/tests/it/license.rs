use anyhow::Result;
use assert_fs::prelude::*;
use indoc::formatdoc;
use url::Url;

use crate::common::{uv_snapshot, TestContext};

#[test]
fn project_with_no_license() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
    "#,
    )?;

    uv_snapshot!(context.filters(), context.license().arg("--universal"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    project: 0.1.0, Unknown License

    ----- stderr -----
    Resolved 1 package in [TIME]
    "###
    );

    // `uv tree` should update the lockfile
    let lock = context.read("uv.lock");
    assert!(!lock.is_empty());

    Ok(())
}

#[test]
fn project_with_trove_license() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        classifiers = [
            "License :: Other/Proprietary License"
        ]
    "#,
    )?;

    uv_snapshot!(context.filters(), context.license().arg("--universal"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    project: 0.1.0, Other/Proprietary License

    ----- stderr -----
    Resolved 1 package in [TIME]
    "###
    );

    // `uv tree` should update the lockfile
    let lock = context.read("uv.lock");
    assert!(!lock.is_empty());

    Ok(())
}

#[test]
fn project_with_trove_osi_license() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        classifiers = [
            "License :: OSI Approved"
        ]
    "#,
    )?;

    uv_snapshot!(context.filters(), context.license().arg("--universal"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    project: 0.1.0, OSI Approved

    ----- stderr -----
    Resolved 1 package in [TIME]
    "###
    );

    // `uv tree` should update the lockfile
    let lock = context.read("uv.lock");
    assert!(!lock.is_empty());

    Ok(())
}

#[test]
fn nested_dependencies() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "scikit-learn==1.4.1.post1"
        ]
        classifiers = [
            "License :: OSI Approved :: MIT License"
        ]
    "#,
    )?;

    uv_snapshot!(context.filters(), context.license().arg("--universal"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    project: 0.1.0, MIT License
    scikit-learn: 1.4.1.post1, BSD License
    joblib: 1.3.2, BSD License
    numpy: 1.26.4, BSD License
    scipy: 1.12.0, BSD License
    threadpoolctl: 3.4.0, BSD License

    ----- stderr -----
    Resolved 6 packages in [TIME]
    "###
    );

    // `uv tree` should update the lockfile
    let lock = context.read("uv.lock");
    assert!(!lock.is_empty());

    Ok(())
}

#[test]
fn nested_platform_dependencies() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "jupyter-client"
        ]
    "#,
    )?;

    uv_snapshot!(context.filters(), context.license().arg("--python-platform").arg("linux"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    project: 0.1.0, Unknown License
    jupyter-client: 8.6.1, BSD License
    jupyter-core: 5.7.2, BSD License
    platformdirs: 4.2.0, MIT License
    traitlets: 5.14.2, BSD License
    python-dateutil: 2.9.0.post0, BSD License, Apache Software License
    six: 1.16.0, MIT License
    pyzmq: 25.1.2, GNU Library or Lesser General Public License (LGPL), BSD License
    tornado: 6.4, Apache Software License

    ----- stderr -----
    Resolved 12 packages in [TIME]
    "
    );

    uv_snapshot!(context.filters(), context.license().arg("--universal"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    project: 0.1.0, Unknown License
    jupyter-client: 8.6.1, BSD License
    jupyter-core: 5.7.2, BSD License
    platformdirs: 4.2.0, MIT License
    pywin32: 306, Python Software Foundation License
    traitlets: 5.14.2, BSD License
    python-dateutil: 2.9.0.post0, BSD License, Apache Software License
    six: 1.16.0, MIT License
    pyzmq: 25.1.2, GNU Library or Lesser General Public License (LGPL), BSD License
    cffi: 1.16.0, MIT License
    pycparser: 2.21, BSD License
    tornado: 6.4, Apache Software License

    ----- stderr -----
    Resolved 12 packages in [TIME]
    "
    );

    // `uv tree` should update the lockfile
    let lock = context.read("uv.lock");
    assert!(!lock.is_empty());

    Ok(())
}

#[test]
fn frozen() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio"]
    "#,
    )?;

    uv_snapshot!(context.filters(), context.license().arg("--universal"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    project: 0.1.0, Unknown License
    anyio: 4.3.0, MIT License
    idna: 3.6, BSD License
    sniffio: 1.3.1, MIT License, Apache Software License

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "
    );

    // `uv tree` should update the lockfile
    let lock = context.read("uv.lock");
    assert!(!lock.is_empty());

    // Update the project dependencies.
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

    // Running with `--frozen` should show the stale tree.
    uv_snapshot!(context.filters(), context.license().arg("--frozen"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    project: 0.1.0, Unknown License
    anyio: 4.3.0, MIT License
    idna: 3.6, BSD License
    sniffio: 1.3.1, MIT License, Apache Software License

    ----- stderr -----
    "
    );

    Ok(())
}

#[test]
fn platform_dependencies() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "black"
        ]
    "#,
    )?;

    // When `--universal` is _not_ provided, `colorama` should _not_ be included.
    #[cfg(not(windows))]
    uv_snapshot!(context.filters(), context.license(), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    project: 0.1.0, Unknown License
    black: 24.3.0, MIT License
    click: 8.1.7, BSD License
    mypy-extensions: 1.0.0, MIT License
    packaging: 24.0, Apache Software License, BSD License
    pathspec: 0.12.1, Mozilla Public License 2.0 (MPL 2.0)
    platformdirs: 4.2.0, MIT License

    ----- stderr -----
    Resolved 8 packages in [TIME]
    ");

    // Unless `--python-platform` is set to `windows`, in which case it should be included.
    uv_snapshot!(context.filters(), context.license().arg("--python-platform").arg("windows"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    project: 0.1.0, Unknown License
    black: 24.3.0, MIT License
    click: 8.1.7, BSD License
    colorama: 0.4.6, BSD License
    mypy-extensions: 1.0.0, MIT License
    packaging: 24.0, Apache Software License, BSD License
    pathspec: 0.12.1, Mozilla Public License 2.0 (MPL 2.0)
    platformdirs: 4.2.0, MIT License

    ----- stderr -----
    Resolved 8 packages in [TIME]
    ");

    // When `--universal` is _not_ provided, should include `colorama`, even though it's only
    // included on Windows.
    uv_snapshot!(context.filters(), context.license().arg("--universal"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    project: 0.1.0, Unknown License
    black: 24.3.0, MIT License
    click: 8.1.7, BSD License
    colorama: 0.4.6, BSD License
    mypy-extensions: 1.0.0, MIT License
    packaging: 24.0, Apache Software License, BSD License
    pathspec: 0.12.1, Mozilla Public License 2.0 (MPL 2.0)
    platformdirs: 4.2.0, MIT License

    ----- stderr -----
    Resolved 8 packages in [TIME]
    "
    );

    // `uv tree` should update the lockfile
    let lock = context.read("uv.lock");
    assert!(!lock.is_empty());

    Ok(())
}

#[test]
fn repeated_dependencies() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "anyio < 2 ; sys_platform == 'win32'",
            "anyio > 2 ; sys_platform == 'linux'",
        ]
    "#,
    )?;

    // Should include both versions of `anyio`, which have different dependencies.
    uv_snapshot!(context.filters(), context.license().arg("--universal"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    project: 0.1.0, Unknown License
    anyio: 1.4.0, MIT License
    async-generator: 1.10, MIT License, Apache Software License
    idna: 3.6, BSD License
    sniffio: 1.3.1, MIT License, Apache Software License
    anyio: 4.3.0, MIT License

    ----- stderr -----
    Resolved 6 packages in [TIME]
    "
    );

    // `uv tree` should update the lockfile
    let lock = context.read("uv.lock");
    assert!(!lock.is_empty());

    Ok(())
}

/// In this case, a package is included twice at the same version, but pointing to different direct
/// URLs.
#[test]
fn repeated_version() -> Result<()> {
    let context = TestContext::new("3.12");

    let v1 = context.temp_dir.child("v1");
    fs_err::create_dir_all(&v1)?;
    let pyproject_toml = v1.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "dependency"
        version = "0.0.1"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0"]
        "#,
    )?;

    let v2 = context.temp_dir.child("v2");
    fs_err::create_dir_all(&v2)?;
    let pyproject_toml = v2.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "dependency"
        version = "0.0.1"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.0.0"]
        "#,
    )?;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(&formatdoc! {
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
          "dependency @ {} ; sys_platform == 'darwin'",
          "dependency @ {} ; sys_platform != 'darwin'",
        ]
        "#,
        Url::from_file_path(context.temp_dir.join("v1")).unwrap(),
        Url::from_file_path(context.temp_dir.join("v2")).unwrap(),
    })?;

    uv_snapshot!(context.filters(), context.license().arg("--universal"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    project: 0.1.0, Unknown License
    dependency: 0.0.1, Unknown License
    anyio: 3.7.0, MIT License
    idna: 3.6, BSD License
    sniffio: 1.3.1, MIT License, Apache Software License
    dependency: 0.0.1, Unknown License
    anyio: 3.0.0, MIT License

    ----- stderr -----
    Resolved 7 packages in [TIME]
    "
    );

    // `uv tree` should update the lockfile
    let lock = context.read("uv.lock");
    assert!(!lock.is_empty());

    Ok(())
}

#[test]
fn workspace_dev() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio"]

        [dependency-groups]
        dev = ["child"]

        [tool.uv.workspace]
        members = ["child"]

        [tool.uv.sources]
        child = { workspace = true }
    "#,
    )?;

    let child = context.temp_dir.child("child");
    let pyproject_toml = child.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "child"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]
    "#,
    )?;

    uv_snapshot!(context.filters(), context.license().arg("--universal"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    project: 0.1.0, Unknown License
    anyio: 4.3.0, MIT License
    idna: 3.6, BSD License
    sniffio: 1.3.1, MIT License, Apache Software License
    child: 0.1.0, Unknown License (group: dev)
    iniconfig: 2.0.0, MIT License

    ----- stderr -----
    Resolved 6 packages in [TIME]
    "
    );

    // Under `--no-dev`, the member should still be included, since we show the entire workspace.
    // But it shouldn't be considered a dependency of the root.
    uv_snapshot!(context.filters(), context.license().arg("--universal").arg("--no-dev"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    child: 0.1.0, Unknown License
    iniconfig: 2.0.0, MIT License
    project: 0.1.0, Unknown License
    anyio: 4.3.0, MIT License
    idna: 3.6, BSD License
    sniffio: 1.3.1, MIT License, Apache Software License

    ----- stderr -----
    Resolved 6 packages in [TIME]
    "
    );

    // `uv tree` should update the lockfile
    let lock = context.read("uv.lock");
    assert!(!lock.is_empty());

    Ok(())
}
