#![cfg(all(feature = "python", feature = "pypi"))]

use anyhow::Result;
use assert_cmd::prelude::*;
use assert_fs::prelude::*;
use insta::assert_snapshot;

use common::{uv_snapshot, TestContext};

mod common;

#[test]
fn sync() -> Result<()> {
    let context = TestContext::new("3.12");

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

    // Running `uv sync` should generate a lockfile.
    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + iniconfig==2.0.0
     + project==0.1.0 (from file://[TEMP_DIR]/)
    "###);

    assert!(context.temp_dir.child("uv.lock").exists());

    Ok(())
}

#[test]
fn locked() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0"]
        "#,
    )?;

    // Running with `--locked` should error, if no lockfile is present.
    uv_snapshot!(context.filters(), context.sync().arg("--locked"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Unable to find lockfile at `uv.lock`. To create a lockfile, run `uv lock` or `uv sync`.
    "###);

    // Lock the initial requirements.
    context.lock().assert().success();

    let existing = fs_err::read_to_string(context.temp_dir.child("uv.lock"))?;

    // Update the requirements.
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]
        "#,
    )?;

    // Running with `--locked` should error.
    uv_snapshot!(context.filters(), context.sync().arg("--locked"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided. To update the lockfile, run `uv lock`.
    "###);

    let updated = fs_err::read_to_string(context.temp_dir.child("uv.lock"))?;

    // And the lockfile should be unchanged.
    assert_eq!(existing, updated);

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
        dependencies = ["anyio==3.7.0"]
        "#,
    )?;

    // Running with `--frozen` should error, if no lockfile is present.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Unable to find lockfile at `uv.lock`. To create a lockfile, run `uv lock` or `uv sync`.
    "###);

    context.lock().assert().success();

    // Update the requirements.
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]
        "#,
    )?;

    // Running with `--frozen` should install the stale lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==3.7.0
     + idna==3.6
     + project==0.1.0 (from file://[TEMP_DIR]/)
     + sniffio==1.3.1
    "###);

    Ok(())
}

#[test]
fn empty() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r"
        [tool.uv.workspace]
        members = []
        ",
    )?;

    // Running `uv sync` should generate an empty lockfile.
    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 0 packages in [TIME]
    Audited 0 packages in [TIME]
    "###);

    assert!(context.temp_dir.child("uv.lock").exists());

    // Running `uv sync` again should succeed.
    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 0 packages in [TIME]
    Audited 0 packages in [TIME]
    "###);

    Ok(())
}

/// Sync an individual package within a workspace.
#[test]
fn package() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "root"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["child", "anyio>3"]

        [tool.uv.sources]
        child = { workspace = true }

        [tool.uv.workspace]
        members = ["child"]
        "#,
    )?;

    let src = context.temp_dir.child("src").child("albatross");
    src.create_dir_all()?;

    let init = src.child("__init__.py");
    init.touch()?;

    let child = context.temp_dir.child("child");
    fs_err::create_dir_all(&child)?;

    let pyproject_toml = child.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "child"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig>1"]
        "#,
    )?;

    let src = child.child("src").child("albatross");
    src.create_dir_all()?;

    let init = src.child("__init__.py");
    init.touch()?;

    uv_snapshot!(context.filters(), context.sync().arg("--package").arg("child"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + child==0.1.0 (from file://[TEMP_DIR]/child)
     + iniconfig==2.0.0
    "###);

    Ok(())
}

/// Ensure that we use the maximum Python version when a workspace contains mixed requirements.
#[test]
fn mixed_requires_python() -> Result<()> {
    let context = TestContext::new_with_versions(&["3.8", "3.12"]);

    // Create a workspace root with a minimum Python requirement of Python 3.12.
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "albatross"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["bird-feeder", "anyio>3"]

        [tool.uv.sources]
        bird-feeder = { workspace = true }

        [tool.uv.workspace]
        members = ["packages/*"]

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
        "#,
    )?;

    let src = context.temp_dir.child("src").child("albatross");
    src.create_dir_all()?;

    let init = src.child("__init__.py");
    init.touch()?;

    // Create a child with a minimum Python requirement of Python 3.8.
    let child = context.temp_dir.child("packages").child("bird-feeder");
    child.create_dir_all()?;

    let src = context.temp_dir.child("src").child("bird_feeder");
    src.create_dir_all()?;

    let init = src.child("__init__.py");
    init.touch()?;

    let pyproject_toml = child.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "bird-feeder"
        version = "0.1.0"
        requires-python = ">=3.8"
        "#,
    )?;

    // Running `uv sync` should succeed, locking for Python 3.12.
    uv_snapshot!(context.filters(), context.sync().arg("-p").arg("3.12"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using Python 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtualenv at: .venv
    Resolved 5 packages in [TIME]
    Prepared 5 packages in [TIME]
    Installed 5 packages in [TIME]
     + albatross==0.1.0 (from file://[TEMP_DIR]/)
     + anyio==4.3.0
     + bird-feeder==0.1.0 (from file://[TEMP_DIR]/packages/bird-feeder)
     + idna==3.6
     + sniffio==1.3.1
    "###);

    // Running `uv sync` again should succeed.
    uv_snapshot!(context.filters(), context.sync().arg("-p").arg("3.8"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Using Python 3.8.[X] interpreter at: [PYTHON-3.8]
    error: The requested Python interpreter (3.8.[X]) is incompatible with the project Python requirement: `>=3.12`. However, a workspace member (`bird-feeder`) supports Python >=3.8. To install the workspace member on its own, navigate to `packages/bird-feeder`, then run `uv venv --python 3.8.[X]` followed by `uv pip install -e .`.
    "###);

    Ok(())
}

/// Sync development dependencies in a virtual workspace root.
#[test]
fn virtual_workspace_dev_dependencies() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [tool.uv]
        dev-dependencies = ["anyio>3", "requests[socks]", "typing-extensions ; sys_platform == ''"]

        [tool.uv.workspace]
        members = ["child"]
        "#,
    )?;

    let src = context.temp_dir.child("src").child("albatross");
    src.create_dir_all()?;

    let init = src.child("__init__.py");
    init.touch()?;

    let child = context.temp_dir.child("child");
    fs_err::create_dir_all(&child)?;

    let pyproject_toml = child.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "child"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig>1"]
        "#,
    )?;

    let src = child.child("src").child("albatross");
    src.create_dir_all()?;

    let init = src.child("__init__.py");
    init.touch()?;

    // Syncing with `--no-dev` should omit all dependencies except `iniconfig`.
    uv_snapshot!(context.filters(), context.sync().arg("--no-dev"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 11 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + child==0.1.0 (from file://[TEMP_DIR]/child)
     + iniconfig==2.0.0
    "###);

    // Syncing without `--no-dev` should include `anyio`, `requests`, `pysocks`, and their
    // dependencies, but not `typing-extensions`.
    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 11 packages in [TIME]
    Prepared 8 packages in [TIME]
    Installed 8 packages in [TIME]
     + anyio==4.3.0
     + certifi==2024.2.2
     + charset-normalizer==3.3.2
     + idna==3.6
     + pysocks==1.7.1
     + requests==2.31.0
     + sniffio==1.3.1
     + urllib3==2.2.1
    "###);

    Ok(())
}

/// Use a `pip install` step to pre-install build dependencies for `--no-build-isolation`.
#[test]
fn sync_build_isolation() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["source-distribution @ https://files.pythonhosted.org/packages/10/1f/57aa4cce1b1abf6b433106676e15f9fa2c92ed2bd4cf77c3b50a9e9ac773/source_distribution-0.0.1.tar.gz"]
        "#,
    )?;

    // Running `uv sync` should fail (but it could fail when building the root project, or when
    // building `source-distribution`).
    context
        .sync()
        .arg("--no-build-isolation")
        .assert()
        .failure();

    // Install `setuptools` (for the root project) plus `hatchling` (for `source-distribution`).
    uv_snapshot!(context.filters(), context.pip_install().arg("wheel").arg("setuptools").arg("hatchling"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
    Prepared 7 packages in [TIME]
    Installed 7 packages in [TIME]
     + hatchling==1.22.4
     + packaging==24.0
     + pathspec==0.12.1
     + pluggy==1.4.0
     + setuptools==69.2.0
     + trove-classifiers==2024.3.3
     + wheel==0.43.0
    "###);

    // Running `uv sync` should succeed.
    uv_snapshot!(context.filters(), context.sync().arg("--no-build-isolation"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Uninstalled 7 packages in [TIME]
    Installed 2 packages in [TIME]
     - hatchling==1.22.4
     - packaging==24.0
     - pathspec==0.12.1
     - pluggy==1.4.0
     + project==0.1.0 (from file://[TEMP_DIR]/)
     - setuptools==69.2.0
     + source-distribution==0.0.1 (from https://files.pythonhosted.org/packages/10/1f/57aa4cce1b1abf6b433106676e15f9fa2c92ed2bd4cf77c3b50a9e9ac773/source_distribution-0.0.1.tar.gz)
     - trove-classifiers==2024.3.3
     - wheel==0.43.0
    "###);

    assert!(context.temp_dir.child("uv.lock").exists());

    Ok(())
}

/// Use a `pip install` step to pre-install build dependencies for `--no-build-isolation-package`.
#[test]
fn sync_build_isolation_package() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "source-distribution @ https://files.pythonhosted.org/packages/10/1f/57aa4cce1b1abf6b433106676e15f9fa2c92ed2bd4cf77c3b50a9e9ac773/source_distribution-0.0.1.tar.gz",
        ]

        [build-system]
        requires = ["setuptools >= 40.9.0"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    // Running `uv sync` should fail for iniconfig.
    let filters = std::iter::once((r"exit code: 1", "exit status: 1"))
        .chain(context.filters())
        .collect::<Vec<_>>();
    uv_snapshot!(filters, context.sync().arg("--no-build-isolation-package").arg("source-distribution"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    error: Failed to prepare distributions
      Caused by: Failed to fetch wheel: source-distribution @ https://files.pythonhosted.org/packages/10/1f/57aa4cce1b1abf6b433106676e15f9fa2c92ed2bd4cf77c3b50a9e9ac773/source_distribution-0.0.1.tar.gz
      Caused by: Build backend failed to build wheel through `build_wheel()` with exit status: 1
    --- stdout:

    --- stderr:
    Traceback (most recent call last):
      File "<string>", line 8, in <module>
    ModuleNotFoundError: No module named 'hatchling'
    ---
    "###);

    // Install `hatchling` for `source-distribution`.
    uv_snapshot!(context.filters(), context.pip_install().arg("hatchling"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Prepared 5 packages in [TIME]
    Installed 5 packages in [TIME]
     + hatchling==1.22.4
     + packaging==24.0
     + pathspec==0.12.1
     + pluggy==1.4.0
     + trove-classifiers==2024.3.3
    "###);

    // Running `uv sync` should succeed.
    uv_snapshot!(context.filters(), context.sync().arg("--no-build-isolation-package").arg("source-distribution"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Uninstalled 5 packages in [TIME]
    Installed 2 packages in [TIME]
     - hatchling==1.22.4
     - packaging==24.0
     - pathspec==0.12.1
     - pluggy==1.4.0
     + project==0.1.0 (from file://[TEMP_DIR]/)
     + source-distribution==0.0.1 (from https://files.pythonhosted.org/packages/10/1f/57aa4cce1b1abf6b433106676e15f9fa2c92ed2bd4cf77c3b50a9e9ac773/source_distribution-0.0.1.tar.gz)
     - trove-classifiers==2024.3.3
    "###);

    assert!(context.temp_dir.child("uv.lock").exists());

    Ok(())
}

/// Use dedicated extra groups to install dependencies for `--no-build-isolation-package`.
#[test]
fn sync_build_isolation_extra() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [project.optional-dependencies]
        build = ["hatchling"]
        compile = ["source-distribution @ https://files.pythonhosted.org/packages/10/1f/57aa4cce1b1abf6b433106676e15f9fa2c92ed2bd4cf77c3b50a9e9ac773/source_distribution-0.0.1.tar.gz"]

        [build-system]
        requires = ["setuptools >= 40.9.0"]
        build-backend = "setuptools.build_meta"

        [tool.uv]
        no-build-isolation-package = ["source-distribution"]
        "#,
    )?;

    // Running `uv sync` should fail for the `compile` extra.
    let filters = std::iter::once((r"exit code: 1", "exit status: 1"))
        .chain(context.filters())
        .collect::<Vec<_>>();
    uv_snapshot!(&filters, context.sync().arg("--extra").arg("compile"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
    error: Failed to prepare distributions
      Caused by: Failed to fetch wheel: source-distribution @ https://files.pythonhosted.org/packages/10/1f/57aa4cce1b1abf6b433106676e15f9fa2c92ed2bd4cf77c3b50a9e9ac773/source_distribution-0.0.1.tar.gz
      Caused by: Build backend failed to build wheel through `build_wheel()` with exit status: 1
    --- stdout:

    --- stderr:
    Traceback (most recent call last):
      File "<string>", line 8, in <module>
    ModuleNotFoundError: No module named 'hatchling'
    ---
    "###);

    // Running `uv sync` with `--all-extras` should also fail.
    uv_snapshot!(&filters, context.sync().arg("--all-extras"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
    error: Failed to prepare distributions
      Caused by: Failed to fetch wheel: source-distribution @ https://files.pythonhosted.org/packages/10/1f/57aa4cce1b1abf6b433106676e15f9fa2c92ed2bd4cf77c3b50a9e9ac773/source_distribution-0.0.1.tar.gz
      Caused by: Build backend failed to build wheel through `build_wheel()` with exit status: 1
    --- stdout:

    --- stderr:
    Traceback (most recent call last):
      File "<string>", line 8, in <module>
    ModuleNotFoundError: No module named 'hatchling'
    ---
    "###);

    // Install the build dependencies.
    uv_snapshot!(context.filters(), context.sync().arg("--extra").arg("build"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
    Prepared 6 packages in [TIME]
    Installed 6 packages in [TIME]
     + hatchling==1.22.4
     + packaging==24.0
     + pathspec==0.12.1
     + pluggy==1.4.0
     + project==0.1.0 (from file://[TEMP_DIR]/)
     + trove-classifiers==2024.3.3
    "###);

    // Running `uv sync` for the `compile` extra should succeed, and remove the build dependencies.
    uv_snapshot!(context.filters(), context.sync().arg("--extra").arg("compile"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 5 packages in [TIME]
    Installed 1 package in [TIME]
     - hatchling==1.22.4
     - packaging==24.0
     - pathspec==0.12.1
     - pluggy==1.4.0
     + source-distribution==0.0.1 (from https://files.pythonhosted.org/packages/10/1f/57aa4cce1b1abf6b433106676e15f9fa2c92ed2bd4cf77c3b50a9e9ac773/source_distribution-0.0.1.tar.gz)
     - trove-classifiers==2024.3.3
    "###);

    assert!(context.temp_dir.child("uv.lock").exists());

    Ok(())
}

/// Avoid using incompatible versions for build dependencies that are also part of the resolved
/// environment. This is a very subtle issue, but: when locking, we don't enforce platform
/// compatibility. So, if we reuse the resolver state to install, and the install itself has to
/// preform a resolution (e.g., for the build dependencies of a source distribution), that
/// resolution may choose incompatible versions.
///
/// The key property here is that there's a shared package between the build dependencies and the
/// project dependencies.
#[test]
fn sync_reset_state() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["pydantic-core"]

        [build-system]
        requires = ["setuptools", "pydantic-core"]
        build-backend = "setuptools.build_meta:__legacy__"
        "#,
    )?;

    let setup_py = context.temp_dir.child("setup.py");
    setup_py.write_str(indoc::indoc! { r#"
        from setuptools import setup
        import pydantic_core

        setup(
            name="project",
            version="0.1.0",
            packages=["project"],
            install_requires=["pydantic-core"],
        )
    "# })?;

    let src = context.temp_dir.child("project");
    src.create_dir_all()?;

    let init = src.child("__init__.py");
    init.touch()?;

    // Running `uv sync` should succeed.
    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + project==0.1.0 (from file://[TEMP_DIR]/)
     + pydantic-core==2.17.0
     + typing-extensions==4.10.0
    "###);

    assert!(context.temp_dir.child("uv.lock").exists());

    Ok(())
}

/// Test that relative wheel paths are correctly preserved.
#[test]
fn sync_relative_wheel() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements = r#"[project]
    name = "relative_wheel"
    version = "0.1.0"
    requires-python = ">=3.12"
    dependencies = ["ok"]

    [tool.uv.sources]
    ok = { path = "wheels/ok-1.0.0-py3-none-any.whl" }

    [build-system]
    requires = ["hatchling"]
    build-backend = "hatchling.build"
    "#;

    context
        .temp_dir
        .child("src/relative_wheel/__init__.py")
        .touch()?;

    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(requirements)?;

    context.temp_dir.child("wheels").create_dir_all()?;
    fs_err::copy(
        "../../scripts/links/ok-1.0.0-py3-none-any.whl",
        context.temp_dir.join("wheels/ok-1.0.0-py3-none-any.whl"),
    )?;

    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 2 packages in [TIME]
     + ok==1.0.0 (from file://[TEMP_DIR]/wheels/ok-1.0.0-py3-none-any.whl)
     + relative-wheel==0.1.0 (from file://[TEMP_DIR]/)
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!(
        {
            filters => context.filters(),
        },
        {
            assert_snapshot!(
                lock, @r###"
            version = 1
            requires-python = ">=3.12"

            [options]
            exclude-newer = "2024-03-25T00:00:00Z"

            [[package]]
            name = "ok"
            version = "1.0.0"
            source = { path = "wheels/ok-1.0.0-py3-none-any.whl" }
            wheels = [
                { filename = "ok-1.0.0-py3-none-any.whl", hash = "sha256:79f0b33e6ce1e09eaa1784c8eee275dfe84d215d9c65c652f07c18e85fdaac5f" },
            ]

            [[package]]
            name = "relative-wheel"
            version = "0.1.0"
            source = { editable = "." }
            dependencies = [
                { name = "ok" },
            ]

            [package.metadata]
            requires-dist = [{ name = "ok", path = "wheels/ok-1.0.0-py3-none-any.whl" }]
            "###
            );
        }
    );

    // Check that we can re-read the lockfile.
    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Audited 2 packages in [TIME]
    "###);

    Ok(())
}

/// Syncing against an unstable environment should fail (but locking should succeed).
#[test]
fn sync_environment() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.10"
        dependencies = ["iniconfig"]

        [tool.uv]
        environments = ["python_version < '3.11'"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    error: The current Python platform is not compatible with the lockfile's supported environments: `python_full_version < '3.11'`
    "###);

    assert!(context.temp_dir.child("uv.lock").exists());

    Ok(())
}

/// Regression test for <https://github.com/astral-sh/uv/issues/6316>.
///
/// Previously, we would read metadata statically from pyproject.toml and write that to `uv.lock`. In
/// this sync pass, we had also built the project with hatchling, which sorts specifiers by python
/// string sort through packaging. On the second run, we read the cache that now has the hatchling
/// sorting, changing the lockfile.
#[test]
fn read_metadata_statically_over_the_cache() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        # Python string sorting is the other way round.
        dependencies = ["anyio>=4,<5"]
        "#,
    )?;

    context.sync().assert().success();
    let lock1 = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;
    // Assert we're reading static metadata.
    assert!(lock1.contains(">=4,<5"));
    assert!(!lock1.contains("<5,>=4"));
    context.sync().assert().success();
    let lock2 = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;
    // Assert stability.
    assert_eq!(lock1, lock2);

    Ok(())
}

/// Avoid syncing the project package when `--no-install-project` is provided.
#[test]
fn no_install_project() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0"]
        "#,
    )?;

    // Generate a lockfile.
    context.lock().assert().success();

    // Running with `--no-install-project` should install `anyio`, but not `project`.
    uv_snapshot!(context.filters(), context.sync().arg("--no-install-project"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==3.7.0
     + idna==3.6
     + sniffio==1.3.1
    "###);

    // However, we do require the `pyproject.toml`.
    fs_err::remove_file(pyproject_toml)?;

    uv_snapshot!(context.filters(), context.sync().arg("--no-install-project"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No `pyproject.toml` found in current directory or any parent directory
    "###);

    Ok(())
}

/// Avoid syncing workspace members and the project when `--no-install-workspace` is provided, but
/// include the all of the dependencies.
#[test]
fn no_install_workspace() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0", "child"]

        [tool.uv.workspace]
        members = ["child"]

        [tool.uv.sources]
        child = { workspace = true }
        "#,
    )?;

    // Add a workspace member.
    let child = context.temp_dir.child("child");
    child.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "child"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig>1"]

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
        "#,
    )?;
    child
        .child("src")
        .child("child")
        .child("__init__.py")
        .touch()?;

    // Generate a lockfile.
    context.lock().assert().success();

    // Running with `--no-install-workspace` should install `anyio` and `iniconfig`, but not
    // `project` or `child`.
    uv_snapshot!(context.filters(), context.sync().arg("--no-install-workspace"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==3.7.0
     + idna==3.6
     + iniconfig==2.0.0
     + sniffio==1.3.1
    "###);

    // However, we do require the `pyproject.toml`.
    fs_err::remove_file(child.join("pyproject.toml"))?;

    uv_snapshot!(context.filters(), context.sync().arg("--no-install-workspace"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Workspace member `[TEMP_DIR]/child` is missing a `pyproject.toml` (matches: `child`)
    "###);

    Ok(())
}

/// Avoid syncing the target package when `--no-install-package` is provided.
#[test]
fn no_install_package() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0"]
        "#,
    )?;

    // Generate a lockfile.
    context.lock().assert().success();

    // Running with `--no-install-package anyio` should skip anyio but include everything else
    uv_snapshot!(context.filters(), context.sync().arg("--no-install-package").arg("anyio"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + idna==3.6
     + project==0.1.0 (from file://[TEMP_DIR]/)
     + sniffio==1.3.1
    "###);

    // Running with `--no-install-package project` should skip the project itself (not as a special
    // case, that's just the name of the project)
    uv_snapshot!(context.filters(), context.sync().arg("--no-install-package").arg("project"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     + anyio==3.7.0
     - project==0.1.0 (from file://[TEMP_DIR]/)
    "###);

    Ok(())
}
