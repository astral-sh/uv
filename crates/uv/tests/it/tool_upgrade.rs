use std::process::Command;

use anyhow::{Context, Result};
use assert_cmd::assert::OutputAssertExt;
use assert_fs::prelude::*;
use indoc::indoc;
use insta::assert_snapshot;

use uv_static::EnvVars;

use uv_test::uv_snapshot;

#[test]
fn tool_upgrade_empty() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    uv_snapshot!(context.filters(), context.tool_upgrade()
        .arg("--all")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Nothing to upgrade
    ");

    uv_snapshot!(context.filters(), context.tool_upgrade()
        .arg("--all")
        .arg("-p")
        .arg("3.13")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Nothing to upgrade
    ");

    // Install the latest `babel`.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("babel")
        .arg("--index-url")
        .arg("https://pypi.org/simple/")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + babel==2.14.0
    Installed 1 executable: pybabel
    ");

    uv_snapshot!(context.filters(), context.tool_upgrade()
        .arg("--all")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Nothing to upgrade
    ");

    uv_snapshot!(context.filters(), context.tool_upgrade()
        .arg("--all")
        .arg("-p")
        .arg("3.12")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Nothing to upgrade
    ");
}

#[test]
fn tool_upgrade_preserves_workspace_member_editability() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    let root_pyproject = context.temp_dir.child("pyproject.toml");
    root_pyproject.write_str(indoc! {
        r#"
        [project]
        name = "root"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["child"]

        [project.scripts]
        root_cli = "root:main"

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"

        [tool.uv.sources]
        child = { workspace = true }

        [tool.uv.workspace]
        members = ["child"]
        "#
    })?;

    let root_src = context.temp_dir.child("src").child("root");
    root_src.create_dir_all()?;
    root_src.child("__init__.py").write_str(indoc! {
        r"
        def main():
            import child
            print(child.MESSAGE)
        "
    })?;

    let child = context.temp_dir.child("child");
    child.create_dir_all()?;
    child.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "child"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"
    "#})?;

    let child_src = child.child("src").child("child");
    child_src.create_dir_all()?;
    child_src
        .child("__init__.py")
        .write_str("MESSAGE = 'OK'\n")?;

    let status = context
        .tool_install()
        .arg(context.temp_dir.path())
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str())
        .status()
        .expect("failed to run uv tool install");
    assert!(status.success());

    uv_snapshot!(context.filters(), Command::new("root_cli").env(EnvVars::PATH, bin_dir.as_os_str()), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    OK

    ----- stderr -----
    ");

    child_src
        .child("__init__.py")
        .write_str("MESSAGE = 'PRE-UPGRADE'\n")?;

    uv_snapshot!(context.filters(), Command::new("root_cli").env(EnvVars::PATH, bin_dir.as_os_str()), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    OK

    ----- stderr -----
    ");

    let status = context
        .tool_upgrade()
        .arg("root")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str())
        .status()
        .expect("failed to run uv tool upgrade");
    assert!(status.success());

    uv_snapshot!(context.filters(), Command::new("root_cli").env(EnvVars::PATH, bin_dir.as_os_str()), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    OK

    ----- stderr -----
    ");

    child_src
        .child("__init__.py")
        .write_str("MESSAGE = 'POST-UPGRADE'\n")?;

    uv_snapshot!(context.filters(), Command::new("root_cli").env(EnvVars::PATH, bin_dir.as_os_str()), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    OK

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn tool_upgrade_preserves_mixed_workspace_member_editability() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    let tool_root = context.temp_dir.child("tool-root");
    tool_root.create_dir_all()?;
    tool_root.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "tool-root"
        version = "0.1.0"
        requires-python = ">=3.12"

        [project.scripts]
        root_cli = "tool_root:main"

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"
    "#})?;
    let tool_root_src = tool_root.child("src").child("tool_root");
    tool_root_src.create_dir_all()?;
    tool_root_src.child("__init__.py").write_str(indoc! {
        r#"
        def main():
            import importlib.metadata
            import other_child

            print(f"{importlib.metadata.version('tool-root')} {other_child.MESSAGE}")
        "#
    })?;

    let other_workspace = context.temp_dir.child("other-workspace");
    other_workspace.create_dir_all()?;
    other_workspace
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "other-workspace"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["other-child"]

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"

        [tool.uv.sources]
        other-child = { workspace = true }

        [tool.uv.workspace]
        members = ["packages/*"]
    "#})?;
    let other_workspace_src = other_workspace.child("src").child("other_workspace");
    other_workspace_src.create_dir_all()?;
    other_workspace_src.child("__init__.py").touch()?;

    let other_child = other_workspace.child("packages").child("other-child");
    other_child.create_dir_all()?;
    other_child.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "other-child"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"
    "#})?;
    let other_child_src = other_child.child("src").child("other_child");
    other_child_src.create_dir_all()?;
    other_child_src
        .child("__init__.py")
        .write_str("MESSAGE = 'OK'\n")?;

    let status = context
        .tool_install()
        .arg("--with-editable")
        .arg(other_workspace.path())
        .arg(tool_root.path())
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str())
        .status()
        .expect("failed to run uv tool install");
    assert!(status.success());

    uv_snapshot!(context.filters(), Command::new("root_cli").env(EnvVars::PATH, bin_dir.as_os_str()), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    0.1.0 OK

    ----- stderr -----
    ");

    tool_root.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "tool-root"
        version = "0.1.1"
        requires-python = ">=3.12"

        [project.scripts]
        root_cli = "tool_root:main"

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"
    "#})?;

    let status = context
        .tool_upgrade()
        .arg("tool-root")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str())
        .status()
        .expect("failed to run uv tool upgrade");
    assert!(status.success());

    uv_snapshot!(context.filters(), Command::new("root_cli").env(EnvVars::PATH, bin_dir.as_os_str()), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    0.1.1 OK

    ----- stderr -----
    ");

    other_child_src
        .child("__init__.py")
        .write_str("MESSAGE = 'POST-UPGRADE'\n")?;

    uv_snapshot!(context.filters(), Command::new("root_cli").env(EnvVars::PATH, bin_dir.as_os_str()), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    0.1.1 POST-UPGRADE

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn tool_upgrade_preserves_mixed_workspace_member_non_editability() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    let tool_root = context.temp_dir.child("tool-root");
    tool_root.create_dir_all()?;
    tool_root.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "tool-root"
        version = "0.1.0"
        requires-python = ">=3.12"

        [project.scripts]
        root_cli = "tool_root:main"

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"
    "#})?;
    let tool_root_src = tool_root.child("src").child("tool_root");
    tool_root_src.create_dir_all()?;
    tool_root_src.child("__init__.py").write_str(indoc! {
        r#"
        def main():
            import importlib.metadata
            import other_child

            print(f"{importlib.metadata.version('tool-root')} {other_child.MESSAGE}")
        "#
    })?;

    let other_workspace = context.temp_dir.child("other-workspace");
    other_workspace.create_dir_all()?;
    other_workspace
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "other-workspace"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["other-child"]

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"

        [tool.uv.sources]
        other-child = { workspace = true }

        [tool.uv.workspace]
        members = ["packages/*"]
    "#})?;
    let other_workspace_src = other_workspace.child("src").child("other_workspace");
    other_workspace_src.create_dir_all()?;
    other_workspace_src.child("__init__.py").touch()?;

    let other_child = other_workspace.child("packages").child("other-child");
    other_child.create_dir_all()?;
    other_child.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "other-child"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"
    "#})?;
    let other_child_src = other_child.child("src").child("other_child");
    other_child_src.create_dir_all()?;
    other_child_src
        .child("__init__.py")
        .write_str("MESSAGE = 'OK'\n")?;

    let status = context
        .tool_install()
        .arg("--editable")
        .arg("--with")
        .arg(other_workspace.path())
        .arg(tool_root.path())
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str())
        .status()
        .expect("failed to run uv tool install");
    assert!(status.success());

    uv_snapshot!(context.filters(), Command::new("root_cli").env(EnvVars::PATH, bin_dir.as_os_str()), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    0.1.0 OK

    ----- stderr -----
    ");

    tool_root.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "tool-root"
        version = "0.1.1"
        requires-python = ">=3.12"

        [project.scripts]
        root_cli = "tool_root:main"

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"
    "#})?;

    let status = context
        .tool_upgrade()
        .arg("tool-root")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str())
        .status()
        .expect("failed to run uv tool upgrade");
    assert!(status.success());

    uv_snapshot!(context.filters(), Command::new("root_cli").env(EnvVars::PATH, bin_dir.as_os_str()), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    0.1.1 OK

    ----- stderr -----
    ");

    other_child_src
        .child("__init__.py")
        .write_str("MESSAGE = 'POST-UPGRADE'\n")?;

    uv_snapshot!(context.filters(), Command::new("root_cli").env(EnvVars::PATH, bin_dir.as_os_str()), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    0.1.1 OK

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn tool_upgrade_name() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `babel` from Test PyPI, to get an outdated version.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("babel")
        .arg("--index-url")
        .arg("https://test.pypi.org/simple/")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + babel==2.6.0
     + pytz==2018.5
    Installed 1 executable: pybabel
    ");

    // Upgrade `babel` by installing from PyPI, which should upgrade to the latest version.
    uv_snapshot!(context.filters(), context.tool_upgrade()
        .arg("babel")
        .arg("--index-url")
        .arg("https://pypi.org/simple/")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Updated babel v2.6.0 -> v2.14.0
     - babel==2.6.0
     + babel==2.14.0
     - pytz==2018.5
    Installed 1 executable: pybabel
    ");
}

#[test]
fn tool_upgrade_recomputes_relative_exclude_newer() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    context
        .tool_install()
        .arg("black")
        .arg("--exclude-newer")
        .arg("3 weeks")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, "2024-03-22T00:00:00Z")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str())
        .assert()
        .success();

    uv_snapshot!(context.filters(), context.tool_upgrade()
        .arg("black")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, "2024-04-15T00:00:00Z")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Updated black v24.2.0 -> v24.3.0
     - black==24.2.0
     + black==24.3.0
     - packaging==23.2
     + packaging==24.0
    Installed 2 executables: black, blackd
    ");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12.[X]"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"
        exclude-newer-span = "P3W"

        [manifest]
        requirements = [{ name = "black" }]

        [[package]]
        name = "black"
        version = "24.3.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "click" },
            { name = "mypy-extensions" },
            { name = "packaging" },
            { name = "pathspec" },
            { name = "platformdirs" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/8f/5f/bac24a952668c7482cfdb4ebf91ba57a796c9da8829363a772040c1a3312/black-24.3.0.tar.gz", hash = "sha256:a0c9c4a0771afc6919578cec71ce82a3e31e054904e7197deacbc9382671c41f", size = 634292, upload-time = "2024-03-15T19:35:43.699Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b6/c6/1d174efa9ff02b22d0124c73fc5f4d4fb006d0d9a081aadc354d05754a13/black-24.3.0-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:2818cf72dfd5d289e48f37ccfa08b460bf469e67fb7c4abb07edc2e9f16fb63f", size = 1600822, upload-time = "2024-03-15T19:45:20.337Z" },
            { url = "https://files.pythonhosted.org/packages/d9/ed/704731afffe460b8ff0672623b40fce9fe569f2ee617c15857e4d4440a3a/black-24.3.0-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:4acf672def7eb1725f41f38bf6bf425c8237248bb0804faa3965c036f7672d11", size = 1429987, upload-time = "2024-03-15T19:45:00.637Z" },
            { url = "https://files.pythonhosted.org/packages/a8/05/8dd038e30caadab7120176d4bc109b7ca2f4457f12eef746b0560a583458/black-24.3.0-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:c7ed6668cbbfcd231fa0dc1b137d3e40c04c7f786e626b405c62bcd5db5857e4", size = 1755319, upload-time = "2024-03-15T19:38:24.009Z" },
            { url = "https://files.pythonhosted.org/packages/71/9d/e5fa1ff4ef1940be15a64883c0bb8d2fcf626efec996eab4ae5a8c691d2c/black-24.3.0-cp312-cp312-win_amd64.whl", hash = "sha256:56f52cfbd3dabe2798d76dbdd299faa046a901041faf2cf33288bc4e6dae57b5", size = 1385180, upload-time = "2024-03-15T19:39:37.014Z" },
            { url = "https://files.pythonhosted.org/packages/4d/ea/31770a7e49f3eedfd8cd7b35e78b3a3aaad860400f8673994bc988318135/black-24.3.0-py3-none-any.whl", hash = "sha256:41622020d7120e01d377f74249e677039d20e6344ff5851de8a10f11f513bf93", size = 201493, upload-time = "2024-03-15T19:35:41.572Z" },
        ]

        [[package]]
        name = "click"
        version = "8.1.7"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/d3/f04c7bfcf5c1862a2a5b845c6b2b360488cf47af55dfa79c98f6a6bf98b5/click-8.1.7.tar.gz", hash = "sha256:ca9853ad459e787e2192211578cc907e7594e294c7ccc834310722b41b9ca6de", size = 336121, upload-time = "2023-08-17T17:29:11.868Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/00/2e/d53fa4befbf2cfa713304affc7ca780ce4fc1fd8710527771b58311a3229/click-8.1.7-py3-none-any.whl", hash = "sha256:ae74fb96c20a0277a1d615f1e4d73c8414f5a98db8b799a7931d1582f3390c28", size = 97941, upload-time = "2023-08-17T17:29:10.08Z" },
        ]

        [[package]]
        name = "mypy-extensions"
        version = "1.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/98/a4/1ab47638b92648243faf97a5aeb6ea83059cc3624972ab6b8d2316078d3f/mypy_extensions-1.0.0.tar.gz", hash = "sha256:75dbf8955dc00442a438fc4d0666508a9a97b6bd41aa2f0ffe9d2f2725af0782", size = 4433, upload-time = "2023-02-04T12:11:27.157Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/2a/e2/5d3f6ada4297caebe1a2add3b126fe800c96f56dbe5d1988a2cbe0b267aa/mypy_extensions-1.0.0-py3-none-any.whl", hash = "sha256:4392f6c0eb8a5668a69e23d168ffa70f0be9ccfd32b5cc2d26a34ae5b844552d", size = 4695, upload-time = "2023-02-04T12:11:25.002Z" },
        ]

        [[package]]
        name = "packaging"
        version = "24.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ee/b5/b43a27ac7472e1818c4bafd44430e69605baefe1f34440593e0332ec8b4d/packaging-24.0.tar.gz", hash = "sha256:eb82c5e3e56209074766e6885bb04b8c38a0c015d0a30036ebe7ece34c9989e9", size = 147882, upload-time = "2024-03-10T09:39:28.33Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/49/df/1fceb2f8900f8639e278b056416d49134fb8d84c5942ffaa01ad34782422/packaging-24.0-py3-none-any.whl", hash = "sha256:2ddfb553fdf02fb784c234c7ba6ccc288296ceabec964ad2eae3777778130bc5", size = 53488, upload-time = "2024-03-10T09:39:25.947Z" },
        ]

        [[package]]
        name = "pathspec"
        version = "0.12.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ca/bc/f35b8446f4531a7cb215605d100cd88b7ac6f44ab3fc94870c120ab3adbf/pathspec-0.12.1.tar.gz", hash = "sha256:a482d51503a1ab33b1c67a6c3813a26953dbdc71c31dacaef9a838c4e29f5712", size = 51043, upload-time = "2023-12-10T22:30:45Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/cc/20/ff623b09d963f88bfde16306a54e12ee5ea43e9b597108672ff3a408aad6/pathspec-0.12.1-py3-none-any.whl", hash = "sha256:a0d503e138a4c123b27490a4f7beda6a01c6f288df0e4a8b79c7eb0dc7b4cc08", size = 31191, upload-time = "2023-12-10T22:30:43.14Z" },
        ]

        [[package]]
        name = "platformdirs"
        version = "4.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/dc/c1d911bf5bb0fdc58cc05010e9f3efe3b67970cef779ba7fbc3183b987a8/platformdirs-4.2.0.tar.gz", hash = "sha256:ef0cc731df711022c174543cb70a9b5bd22e5a9337c8624ef2c2ceb8ddad8768", size = 20055, upload-time = "2024-01-31T01:00:36.02Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/55/72/4898c44ee9ea6f43396fbc23d9bfaf3d06e01b83698bdf2e4c919deceb7c/platformdirs-4.2.0-py3-none-any.whl", hash = "sha256:0614df2a2f37e1a662acbd8e2b25b92ccf8632929bc6d43467e17fe89c75e068", size = 17717, upload-time = "2024-01-31T01:00:34.019Z" },
        ]

        [tool]
        requirements = [{ name = "black" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        exclude-newer-span = "P3W"
        "#);
    });
}

#[test]
fn tool_upgrade_migrates_lockless_receipt_with_installed_preferences() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    context
        .tool_install()
        .arg("black==24.2.0")
        .arg("--exclude-newer")
        .arg("3 weeks")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, "2024-03-22T00:00:00Z")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str())
        .assert()
        .success();

    let receipt_path = tool_dir.child("black").child("uv-receipt.toml");
    let receipt = fs_err::read_to_string(&receipt_path)?;
    let (_, tool_receipt) = receipt
        .split_once("\n[tool]\n")
        .context("expected the tool receipt to contain a tool section")?;
    receipt_path.write_str(&format!("[tool]\n{tool_receipt}"))?;

    uv_snapshot!(context.filters(), context.tool_upgrade()
        .arg("black")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, "2024-04-15T00:00:00Z")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Nothing to upgrade

    hint: `black` is pinned to `24.2.0` (installed with an exact version pin); reinstall with `uv tool install black@latest` to upgrade to a new version.
    ");

    Ok(())
}

#[test]
fn tool_upgrade_multiple_names() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `python-dotenv` from Test PyPI, to get an outdated version.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("python-dotenv")
        .arg("--index-url")
        .arg("https://test.pypi.org/simple/")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + python-dotenv==0.10.2.post2
    Installed 1 executable: dotenv
    ");

    // Install `babel` from Test PyPI, to get an outdated version.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("babel")
        .arg("--index-url")
        .arg("https://test.pypi.org/simple/")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + babel==2.6.0
     + pytz==2018.5
    Installed 1 executable: pybabel
    ");

    // Upgrade `babel` and `python-dotenv` from PyPI.
    uv_snapshot!(context.filters(), context.tool_upgrade()
        .arg("babel")
        .arg("python-dotenv")
        .arg("--index-url")
        .arg("https://pypi.org/simple/")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Updated babel v2.6.0 -> v2.14.0
     - babel==2.6.0
     + babel==2.14.0
     - pytz==2018.5
    Installed 1 executable: pybabel
    Updated python-dotenv v0.10.2.post2 -> v1.0.1
     - python-dotenv==0.10.2.post2
     + python-dotenv==1.0.1
    Installed 1 executable: dotenv
    ");
}

#[test]
fn tool_upgrade_pinned_hint() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();

    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install a specific version of `babel` so the receipt records an exact pin.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("babel==2.6.0")
        .arg("--index-url")
        .arg("https://test.pypi.org/simple/")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + babel==2.6.0
     + pytz==2018.5
    Installed 1 executable: pybabel
    ");

    // Attempt to upgrade `babel`; it should remain pinned and emit a hint explaining why.
    uv_snapshot!(context.filters(), context.tool_upgrade()
        .arg("babel")
        .arg("--index-url")
        .arg("https://pypi.org/simple/")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Modified babel environment
     - pytz==2018.5
     + pytz==2024.1

    hint: `babel` is pinned to `2.6.0` (installed with an exact version pin); reinstall with `uv tool install babel@latest` to upgrade to a new version.
    ");
}

#[test]
fn tool_upgrade_pinned_hint_with_mixed_constraint() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();

    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install a specific version of `babel` with an additional constraint to ensure the requirement
    // contains multiple specifiers while still including an exact pin.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("babel>=2.0,==2.6.0")
        .arg("--index-url")
        .arg("https://test.pypi.org/simple/")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + babel==2.6.0
     + pytz==2018.5
    Installed 1 executable: pybabel
    ");

    // Attempt to upgrade `babel`; it should remain pinned and emit a hint explaining why.
    uv_snapshot!(context.filters(), context.tool_upgrade()
        .arg("babel")
        .arg("--index-url")
        .arg("https://pypi.org/simple/")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Modified babel environment
     - pytz==2018.5
     + pytz==2024.1

    hint: `babel` is pinned to `2.6.0` (installed with an exact version pin); reinstall with `uv tool install babel@latest` to upgrade to a new version.
    ");
}

#[test]
fn tool_upgrade_all() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `python-dotenv` from Test PyPI, to get an outdated version.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("python-dotenv")
        .arg("--index-url")
        .arg("https://test.pypi.org/simple/")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + python-dotenv==0.10.2.post2
    Installed 1 executable: dotenv
    ");

    // Install `babel` from Test PyPI, to get an outdated version.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("babel")
        .arg("--index-url")
        .arg("https://test.pypi.org/simple/")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + babel==2.6.0
     + pytz==2018.5
    Installed 1 executable: pybabel
    ");

    // Upgrade all from PyPI.
    uv_snapshot!(context.filters(), context.tool_upgrade()
        .arg("--all")
        .arg("--index-url")
        .arg("https://pypi.org/simple/")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Updated babel v2.6.0 -> v2.14.0
     - babel==2.6.0
     + babel==2.14.0
     - pytz==2018.5
    Installed 1 executable: pybabel
    Updated python-dotenv v0.10.2.post2 -> v1.0.1
     - python-dotenv==0.10.2.post2
     + python-dotenv==1.0.1
    Installed 1 executable: dotenv
    ");
}

#[test]
fn tool_upgrade_non_existing_package() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Attempt to upgrade `black`.
    uv_snapshot!(context.filters(), context.tool_upgrade()
        .arg("black")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    error: Failed to upgrade black
      Caused by: `black` is not installed; run `uv tool install black` to install
    ");

    // Attempt to upgrade all.
    uv_snapshot!(context.filters(), context.tool_upgrade()
        .arg("--all")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Nothing to upgrade
    ");
}

#[test]
fn tool_upgrade_not_stop_if_upgrade_fails() -> anyhow::Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `python-dotenv` from Test PyPI, to get an outdated version.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("python-dotenv")
        .arg("--index-url")
        .arg("https://test.pypi.org/simple/")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + python-dotenv==0.10.2.post2
    Installed 1 executable: dotenv
    ");

    // Install `babel` from Test PyPI, to get an outdated version.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("babel")
        .arg("--index-url")
        .arg("https://test.pypi.org/simple/")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + babel==2.6.0
     + pytz==2018.5
    Installed 1 executable: pybabel
    ");

    // Break the receipt for python-dotenv
    tool_dir
        .child("python-dotenv")
        .child("uv-receipt.toml")
        .write_str("Invalid receipt")?;

    // Upgrade all from PyPI.
    uv_snapshot!(context.filters(), context.tool_upgrade()
        .arg("--all")
        .arg("--index-url")
        .arg("https://pypi.org/simple/")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Updated babel v2.6.0 -> v2.14.0
     - babel==2.6.0
     + babel==2.14.0
     - pytz==2018.5
    Installed 1 executable: pybabel
    error: Failed to upgrade python-dotenv
      Caused by: `python-dotenv` is missing a valid receipt; run `uv tool install --force python-dotenv` to reinstall
    ");

    Ok(())
}

#[test]
fn tool_upgrade_settings() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black` with `lowest-direct`.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black>=23")
        .arg("--resolution=lowest-direct")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + black==23.1.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    Installed 2 executables: black, blackd
    ");

    // Upgrade `black`. This should be a no-op, since the resolution is set to `lowest-direct`.
    uv_snapshot!(context.filters(), context.tool_upgrade()
        .arg("black")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Nothing to upgrade
    ");

    // Upgrade `black`, but override the resolution.
    uv_snapshot!(context.filters(), context.tool_upgrade()
        .arg("black")
        .arg("--resolution=highest")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Updated black v23.1.0 -> v24.3.0
     - black==23.1.0
     + black==24.3.0
    Installed 2 executables: black, blackd
    ");
}

#[test]
fn tool_upgrade_respect_constraints() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `babel` from Test PyPI, to get an outdated version.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("babel<2.10")
        .arg("--index-url")
        .arg("https://test.pypi.org/simple/")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + babel==2.6.0
     + pytz==2018.5
    Installed 1 executable: pybabel
    ");

    // Upgrade `babel` from PyPI. It should be updated, but not beyond the constraint.
    uv_snapshot!(context.filters(), context.tool_upgrade()
        .arg("babel")
        .arg("--index-url")
        .arg("https://pypi.org/simple/")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Updated babel v2.6.0 -> v2.9.1
     - babel==2.6.0
     + babel==2.9.1
     - pytz==2018.5
     + pytz==2024.1
    Installed 1 executable: pybabel
    ");
}

#[test]
fn tool_upgrade_constraint() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `babel` from Test PyPI, to get an outdated version.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("babel")
        .arg("--index-url")
        .arg("https://test.pypi.org/simple/")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + babel==2.6.0
     + pytz==2018.5
    Installed 1 executable: pybabel
    ");

    // Upgrade `babel`, but apply a constraint inline.
    uv_snapshot!(context.filters(), context.tool_upgrade()
        .arg("babel<2.12.0")
        .arg("--index-url")
        .arg("https://pypi.org/simple/")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Updated babel v2.6.0 -> v2.11.0
     - babel==2.6.0
     + babel==2.11.0
     - pytz==2018.5
     + pytz==2024.1
    Installed 1 executable: pybabel
    ");

    // Upgrade `babel`, but apply a constraint via `--upgrade-package`.
    uv_snapshot!(context.filters(), context.tool_upgrade()
        .arg("babel")
        .arg("--index-url")
        .arg("https://pypi.org/simple/")
        .arg("--upgrade-package")
        .arg("babel<2.14.0")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `--upgrade-package` is enabled by default on `uv tool upgrade`
    Updated babel v2.11.0 -> v2.13.1
     - babel==2.11.0
     + babel==2.13.1
     - pytz==2024.1
     + setuptools==69.2.0
    Installed 1 executable: pybabel
    ");

    // Upgrade `babel` without a constraint.
    uv_snapshot!(context.filters(), context.tool_upgrade()
        .arg("babel")
        .arg("--index-url")
        .arg("https://pypi.org/simple/")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Updated babel v2.13.1 -> v2.14.0
     - babel==2.13.1
     + babel==2.14.0
     - setuptools==69.2.0
    Installed 1 executable: pybabel
    ");

    // Passing `--upgrade` explicitly should warn.
    uv_snapshot!(context.filters(), context.tool_upgrade()
        .arg("babel")
        .arg("--index-url")
        .arg("https://pypi.org/simple/")
        .arg("--upgrade")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `--upgrade` is enabled by default on `uv tool upgrade`
    Nothing to upgrade
    ");
}

/// Upgrade a tool, but only by upgrading one of it's `--with` dependencies, and not the tool
/// itself.
#[test]
fn tool_upgrade_with() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `babel` from Test PyPI, to get an outdated version.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("babel==2.6.0")
        .arg("--index-url")
        .arg("https://test.pypi.org/simple/")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + babel==2.6.0
     + pytz==2018.5
    Installed 1 executable: pybabel
    ");

    // Upgrade `babel` from PyPI. It shouldn't be updated, but `pytz` should be.
    uv_snapshot!(context.filters(), context.tool_upgrade()
        .arg("babel")
        .arg("--index-url")
        .arg("https://pypi.org/simple/")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Modified babel environment
     - pytz==2018.5
     + pytz==2024.1

    hint: `babel` is pinned to `2.6.0` (installed with an exact version pin); reinstall with `uv tool install babel@latest` to upgrade to a new version.
    ");
}

#[test]
fn tool_upgrade_python() {
    let context = uv_test::test_context_with_versions!(&["3.11", "3.12"])
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    uv_snapshot!(context.filters(), context.tool_install()
    .arg("babel==2.6.0")
    .arg("--index-url")
    .arg("https://test.pypi.org/simple/")
    .arg("--python").arg("3.11")
    .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
    .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
    .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + babel==2.6.0
     + pytz==2018.5
    Installed 1 executable: pybabel
    ");

    uv_snapshot!(
        context.filters(),
        context.tool_upgrade().arg("babel")
        .arg("--python").arg("3.12")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + babel==2.6.0
     + pytz==2018.5
    Installed 1 executable: pybabel
    Upgraded tool environment for `babel` to Python 3.12
    "
    );

    insta::with_settings!({
        filters => context.filters(),
    }, {
        let content = fs_err::read_to_string(tool_dir.join("babel").join("pyvenv.cfg")).unwrap();
        let lines: Vec<&str> = content.split('\n').collect();
        assert_snapshot!(lines[lines.len() - 3], @"version_info = 3.12.[X]");
    });
}

#[test]
fn tool_upgrade_python_with_all() {
    let context = uv_test::test_context_with_versions!(&["3.11", "3.12"])
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    uv_snapshot!(context.filters(), context.tool_install()
    .arg("babel==2.6.0")
    .arg("--index-url")
    .arg("https://test.pypi.org/simple/")
    .arg("--python").arg("3.11")
    .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
    .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
    .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + babel==2.6.0
     + pytz==2018.5
    Installed 1 executable: pybabel
    ");

    uv_snapshot!(context.filters(), context.tool_install()
    .arg("python-dotenv")
    .arg("--index-url")
    .arg("https://test.pypi.org/simple/")
    .arg("--python").arg("3.11")
    .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
    .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
    .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + python-dotenv==0.10.2.post2
    Installed 1 executable: dotenv
    ");

    uv_snapshot!(
        context.filters(),
        context.tool_upgrade().arg("--all")
        .arg("--python").arg("3.12")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + babel==2.6.0
     + pytz==2018.5
    Installed 1 executable: pybabel
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + python-dotenv==0.10.2.post2
    Installed 1 executable: dotenv
    Upgraded tool environments for `babel` and `python-dotenv` to Python 3.12
    "
    );

    insta::with_settings!({
        filters => context.filters(),
    }, {
        let content = fs_err::read_to_string(tool_dir.join("babel").join("pyvenv.cfg")).unwrap();
        let lines: Vec<&str> = content.split('\n').collect();
        assert_snapshot!(lines[lines.len() - 3], @"version_info = 3.12.[X]");
    });

    insta::with_settings!({
        filters => context.filters(),
    }, {
        let content = fs_err::read_to_string(tool_dir.join("python-dotenv").join("pyvenv.cfg")).unwrap();
        let lines: Vec<&str> = content.split('\n').collect();
        assert_snapshot!(lines[lines.len() - 3], @"version_info = 3.12.[X]");
    });
}

/// Upgrade a tool together with any additional entrypoints from other
/// packages.
#[test]
fn test_tool_upgrade_additional_entrypoints() {
    let context = uv_test::test_context_with_versions!(&["3.11", "3.12"])
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `babel` entrypoint, and all additional ones from `black` too.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("--python")
        .arg("3.11")
        .arg("--with-executables-from")
        .arg("black")
        .arg("babel==2.14.0")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + babel==2.14.0
     + black==24.3.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    Installed 2 executables from `black`: black, blackd
    Installed 1 executable: pybabel
    ");

    // Upgrade python, and make sure that all the entrypoints above get
    // re-installed.
    uv_snapshot!(context.filters(), context.tool_upgrade()
        .arg("--python")
        .arg("3.12")
        .arg("babel")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + babel==2.14.0
     + black==24.3.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    Installed 2 executables from `black`: black, blackd
    Installed 1 executable: pybabel
    Upgraded tool environment for `babel` to Python 3.12
    ");
}

/// Upgrade a tool with an excluded dependency.
///
/// Compare with `tool_upgrade_respect_constraints`, which shows `pytz` being
/// upgraded alongside `babel`. Here, `pytz` is excluded, so it should remain
/// absent after the upgrade.
#[test]
fn tool_upgrade_excludes() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    let excludes_txt = context.temp_dir.child("excludes.txt");
    excludes_txt.write_str("pytz").unwrap();

    // Install `babel` from Test PyPI, to get an outdated version.
    // `pytz` is excluded, so it won't be installed despite being a dependency.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("babel<2.10")
        .arg("--excludes")
        .arg("excludes.txt")
        .arg("--index-url")
        .arg("https://test.pypi.org/simple/")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + babel==2.6.0
    Installed 1 executable: pybabel
    ");

    // Upgrade `babel` from PyPI. Babel should be updated (within the `<2.10`
    // constraint), but `pytz` should remain excluded.
    uv_snapshot!(context.filters(), context.tool_upgrade()
        .arg("babel")
        .arg("--index-url")
        .arg("https://pypi.org/simple/")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Updated babel v2.6.0 -> v2.9.1
     - babel==2.6.0
     + babel==2.9.1
    Installed 1 executable: pybabel
    ");
}

/// When upgrading a tool from an authenticated index with invalid credentials,
/// the command should fail with an auth error rather than silently reporting
/// "Nothing to upgrade".
///
/// See: <https://github.com/astral-sh/uv/issues/18120>
#[tokio::test]
async fn tool_upgrade_invalid_auth() -> Result<()> {
    let proxy = crate::pypi_proxy::start().await;
    let context = uv_test::test_context!("3.12")
        .with_exclude_newer("2025-01-18T00:00:00Z")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `executable-application` from an authenticated index using `--index`.
    // The receipt will store `authenticate = "auto"` (not "always").
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("executable-application")
        .arg("--index")
        .arg(proxy.authenticated_url("public", "heron", "/basic-auth/simple"))
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + executable-application==0.3.0
    Installed 1 executable: app
    ");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // Verify the receipt has `authenticate = "always"` (promoted from "auto" because the
        // original URL had embedded credentials).
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("executable-application").join("uv-receipt.toml")).unwrap(), @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12.[X]"

        [options]
        exclude-newer = "2025-01-18T00:00:00Z"

        [manifest]
        requirements = [{ name = "executable-application" }]

        [[package]]
        name = "executable-application"
        version = "0.3.0"
        source = { registry = "http://[LOCALHOST]/basic-auth/simple" }
        sdist = { url = "http://[LOCALHOST]/basic-auth/files/packages/9a/36/e803315469274d62f2dab543e3916c0b5b65730074d295f7d48711aa9e36/executable_application-0.3.0.tar.gz", hash = "sha256:0ef8c5ddd28649503c6e4a9f55be17e5b3bd0685df7b83ff7c260b481025f261", size = 914, upload-time = "2025-01-17T23:21:24.559Z" }
        wheels = [
            { url = "http://[LOCALHOST]/basic-auth/files/packages/32/97/8ab6fa1bbcb0a888f460c0a19c301f4cc4180573564ad7dd98b5ceca2ab6/executable_application-0.3.0-py3-none-any.whl", hash = "sha256:ca272aee7332e9d266663bc70037cd3ef1d74ffae40030eaf9ca46462dc8dcc6", size = 1719, upload-time = "2025-01-17T23:21:22.716Z" },
        ]

        [tool]
        requirements = [{ name = "executable-application" }]
        entrypoints = [
            { name = "app", install-path = "[TEMP_DIR]/bin/app", from = "executable-application" },
        ]

        [tool.options]
        index = [{ url = "http://[LOCALHOST]/basic-auth/simple", explicit = false, default = false, format = "simple", authenticate = "always" }]
        exclude-newer = "2025-01-18T00:00:00Z"
        "#);
    });

    // Attempt to upgrade without providing credentials.
    // Because the receipt now stores `authenticate = "always"`, the upgrade should fail
    // with a credentials error rather than silently reporting "Nothing to upgrade".
    uv_snapshot!(context.filters(), context.tool_upgrade()
        .arg("executable-application")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    error: Failed to upgrade executable-application
      Caused by: Failed to fetch: `http://[LOCALHOST]/basic-auth/simple/executable-application/`
      Caused by: Missing credentials for http://[LOCALHOST]/basic-auth/simple/executable-application/
    ");

    Ok(())
}
