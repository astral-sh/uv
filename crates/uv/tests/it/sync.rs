use anyhow::Result;
use assert_cmd::prelude::*;
use assert_fs::{fixture::ChildPath, prelude::*};
use indoc::{formatdoc, indoc};
use insta::assert_snapshot;

use crate::common::{download_to_disk, uv_snapshot, venv_bin_path, TestContext};
use predicates::prelude::predicate;
use tempfile::tempdir_in;
use uv_fs::Simplified;
use uv_static::EnvVars;

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
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
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

    let existing = context.read("uv.lock");

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

    let updated = context.read("uv.lock");

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
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==3.7.0
     + idna==3.6
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
    warning: No `requires-python` value found in the workspace. Defaulting to `>=3.12`.
    Resolved in [TIME]
    Audited in [TIME]
    "###);

    assert!(context.temp_dir.child("uv.lock").exists());

    // Running `uv sync` again should succeed.
    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: No `requires-python` value found in the workspace. Defaulting to `>=3.12`.
    Resolved in [TIME]
    Audited in [TIME]
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
        dependencies = ["iniconfig>=1"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
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

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    // Running `uv sync` should succeed, locking for Python 3.12.
    uv_snapshot!(context.filters(), context.sync().arg("-p").arg("3.12"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: .venv
    Resolved 5 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.3.0
     + bird-feeder==0.1.0 (from file://[TEMP_DIR]/packages/bird-feeder)
     + idna==3.6
     + sniffio==1.3.1
    "###);

    // Running `uv sync` again should fail.
    uv_snapshot!(context.filters(), context.sync().arg("-p").arg("3.8"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.8.[X] interpreter at: [PYTHON-3.8]
    error: The requested interpreter resolved to Python 3.8.[X], which is incompatible with the project's Python requirement: `>=3.12`. However, a workspace member (`bird-feeder`) supports Python >=3.8. To install the workspace member on its own, navigate to `packages/bird-feeder`, then run `uv venv --python 3.8.[X]` followed by `uv pip install -e .`.
    "###);

    Ok(())
}

/// Sync development dependencies in a (legacy) non-project workspace root.
#[test]
fn sync_legacy_non_project_dev_dependencies() -> Result<()> {
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
        dependencies = ["iniconfig>=1"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
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

/// Sync development dependencies in a (legacy) non-project workspace root with `--frozen`.
#[test]
fn sync_legacy_non_project_frozen() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [tool.uv.workspace]
        members = ["foo", "bar"]
        "#,
    )?;

    context
        .temp_dir
        .child("foo")
        .child("pyproject.toml")
        .write_str(
            r#"
        [project]
        name = "foo"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig>=1"]
        "#,
        )?;

    context
        .temp_dir
        .child("bar")
        .child("pyproject.toml")
        .write_str(
            r#"
        [project]
        name = "bar"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["typing-extensions>=4"]
        "#,
        )?;

    context.lock().assert().success();

    uv_snapshot!(context.filters(), context.sync().arg("--frozen").arg("--package").arg("foo"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    "###);

    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + typing-extensions==4.10.0
    "###);

    Ok(())
}

/// Sync development dependencies in a (legacy) non-project workspace root.
#[test]
fn sync_legacy_non_project_group() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [dependency-groups]
        foo = ["anyio"]
        bar = ["typing-extensions"]

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
        dependencies = ["iniconfig>=1"]

        [dependency-groups]
        baz = ["typing-extensions"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    let src = child.child("src").child("albatross");
    src.create_dir_all()?;

    let init = src.child("__init__.py");
    init.touch()?;

    uv_snapshot!(context.filters(), context.sync(), @r###"
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

    uv_snapshot!(context.filters(), context.sync().arg("--group").arg("foo"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + sniffio==1.3.1
    "###);

    uv_snapshot!(context.filters(), context.sync().arg("--only-group").arg("bar"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 5 packages in [TIME]
    Installed 1 package in [TIME]
     - anyio==4.3.0
     - child==0.1.0 (from file://[TEMP_DIR]/child)
     - idna==3.6
     - iniconfig==2.0.0
     - sniffio==1.3.1
     + typing-extensions==4.10.0
    "###);

    uv_snapshot!(context.filters(), context.sync().arg("--group").arg("baz"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Installed 2 packages in [TIME]
     + child==0.1.0 (from file://[TEMP_DIR]/child)
     + iniconfig==2.0.0
    "###);

    uv_snapshot!(context.filters(), context.sync().arg("--group").arg("bop"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Group `bop` is not defined in any project's `dependency-group` table
    "###);

    Ok(())
}

/// Sync development dependencies in a (legacy) non-project workspace root with `--frozen`.
///
/// Modify the `pyproject.toml` after locking.
#[test]
fn sync_legacy_non_project_frozen_modification() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [tool.uv.workspace]
        members = []

        [dependency-groups]
        async = ["anyio"]
        "#,
    )?;

    context.lock().assert().success();

    uv_snapshot!(context.filters(), context.sync().arg("--frozen").arg("--group").arg("async"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + sniffio==1.3.1
    "###);

    // Modify the "live" dependency groups.
    pyproject_toml.write_str(
        r#"
        [tool.uv.workspace]
        members = []

        [dependency-groups]
        async = ["iniconfig"]
        "#,
    )?;

    // This should succeed.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen").arg("--group").arg("async"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 3 packages in [TIME]
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
    Prepared 1 package in [TIME]
    Uninstalled 7 packages in [TIME]
    Installed 1 package in [TIME]
     - hatchling==1.22.4
     - packaging==24.0
     - pathspec==0.12.1
     - pluggy==1.4.0
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
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
      × Failed to build `source-distribution @ https://files.pythonhosted.org/packages/10/1f/57aa4cce1b1abf6b433106676e15f9fa2c92ed2bd4cf77c3b50a9e9ac773/source_distribution-0.0.1.tar.gz`
      ├─▶ The build backend returned an error
      ╰─▶ Call to `hatchling.build.build_wheel` failed (exit status: 1)

          [stderr]
          Traceback (most recent call last):
            File "<string>", line 8, in <module>
          ModuleNotFoundError: No module named 'hatchling'

          hint: This usually indicates a problem with the package or the build environment.
      help: `source-distribution` was included because `project` (v0.1.0) depends on `source-distribution`
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
    let context = TestContext::new("3.12").with_filtered_counts();

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
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
      × Failed to build `source-distribution @ https://files.pythonhosted.org/packages/10/1f/57aa4cce1b1abf6b433106676e15f9fa2c92ed2bd4cf77c3b50a9e9ac773/source_distribution-0.0.1.tar.gz`
      ├─▶ The build backend returned an error
      ╰─▶ Call to `hatchling.build.build_wheel` failed (exit status: 1)

          [stderr]
          Traceback (most recent call last):
            File "<string>", line 8, in <module>
          ModuleNotFoundError: No module named 'hatchling'

          hint: This usually indicates a problem with the package or the build environment.
      help: `source-distribution` was included because `project[compile]` (v0.1.0) depends on `source-distribution`
    "###);

    // Running `uv sync` with `--all-extras` should also fail.
    uv_snapshot!(&filters, context.sync().arg("--all-extras"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
      × Failed to build `source-distribution @ https://files.pythonhosted.org/packages/10/1f/57aa4cce1b1abf6b433106676e15f9fa2c92ed2bd4cf77c3b50a9e9ac773/source_distribution-0.0.1.tar.gz`
      ├─▶ The build backend returned an error
      ╰─▶ Call to `hatchling.build.build_wheel` failed (exit status: 1)

          [stderr]
          Traceback (most recent call last):
            File "<string>", line 8, in <module>
          ModuleNotFoundError: No module named 'hatchling'

          hint: This usually indicates a problem with the package or the build environment.
      help: `source-distribution` was included because `project[compile]` (v0.1.0) depends on `source-distribution`
    "###);

    // Install the build dependencies.
    uv_snapshot!(context.filters(), context.sync().arg("--extra").arg("build"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
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
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Uninstalled [N] packages in [TIME]
    Installed [N] packages in [TIME]
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
/// perform a resolution (e.g., for the build dependencies of a source distribution), that
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

    let lock = context.read("uv.lock");

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

#[test]
fn sync_dev() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["typing-extensions"]

        [tool.uv]
        dev-dependencies = ["anyio"]
        "#,
    )?;

    context.lock().assert().success();

    uv_snapshot!(context.filters(), context.sync().arg("--only-dev"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + sniffio==1.3.1
    "###);

    uv_snapshot!(context.filters(), context.sync().arg("--no-dev"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 3 packages in [TIME]
    Installed 1 package in [TIME]
     - anyio==4.3.0
     - idna==3.6
     - sniffio==1.3.1
     + typing-extensions==4.10.0
    "###);

    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + sniffio==1.3.1
    "###);

    // Using `--no-default-groups` should remove dev dependencies
    uv_snapshot!(context.filters(), context.sync().arg("--no-default-groups"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Uninstalled 3 packages in [TIME]
     - anyio==4.3.0
     - idna==3.6
     - sniffio==1.3.1
    "###);

    Ok(())
}

#[test]
fn sync_group() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["typing-extensions"]

        [tool.uv]

        [dependency-groups]
        dev = ["iniconfig"]
        foo = ["anyio"]
        bar = ["requests"]
        "#,
    )?;

    context.lock().assert().success();

    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 10 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + iniconfig==2.0.0
     + typing-extensions==4.10.0
    "###);

    uv_snapshot!(context.filters(), context.sync().arg("--group").arg("foo"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 10 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + sniffio==1.3.1
    "###);

    uv_snapshot!(context.filters(), context.sync().arg("--only-group").arg("bar"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 10 packages in [TIME]
    Prepared 4 packages in [TIME]
    Uninstalled 4 packages in [TIME]
    Installed 4 packages in [TIME]
     - anyio==4.3.0
     + certifi==2024.2.2
     + charset-normalizer==3.3.2
     - iniconfig==2.0.0
     + requests==2.31.0
     - sniffio==1.3.1
     - typing-extensions==4.10.0
     + urllib3==2.2.1
    "###);

    uv_snapshot!(context.filters(), context.sync().arg("--group").arg("foo").arg("--group").arg("bar"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 10 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.3.0
     + iniconfig==2.0.0
     + sniffio==1.3.1
     + typing-extensions==4.10.0
    "###);

    uv_snapshot!(context.filters(), context.sync().arg("--all-groups"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 10 packages in [TIME]
    Audited 9 packages in [TIME]
    "###);

    uv_snapshot!(context.filters(), context.sync().arg("--all-groups").arg("--no-group").arg("bar"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 10 packages in [TIME]
    Uninstalled 4 packages in [TIME]
     - certifi==2024.2.2
     - charset-normalizer==3.3.2
     - requests==2.31.0
     - urllib3==2.2.1
    "###);

    uv_snapshot!(context.filters(), context.sync().arg("--all-groups").arg("--no-dev"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 10 packages in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 4 packages in [TIME]
     + certifi==2024.2.2
     + charset-normalizer==3.3.2
     - iniconfig==2.0.0
     + requests==2.31.0
     + urllib3==2.2.1
    "###);

    uv_snapshot!(context.filters(), context.sync().arg("--dev"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 10 packages in [TIME]
    Uninstalled 7 packages in [TIME]
    Installed 1 package in [TIME]
     - anyio==4.3.0
     - certifi==2024.2.2
     - charset-normalizer==3.3.2
     - idna==3.6
     + iniconfig==2.0.0
     - requests==2.31.0
     - sniffio==1.3.1
     - urllib3==2.2.1
    "###);

    uv_snapshot!(context.filters(), context.sync().arg("--dev").arg("--no-group").arg("dev"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 10 packages in [TIME]
    Uninstalled 1 package in [TIME]
     - iniconfig==2.0.0
    "###);

    uv_snapshot!(context.filters(), context.sync().arg("--group").arg("dev").arg("--no-dev"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 10 packages in [TIME]
    Audited 1 package in [TIME]
    "###);

    uv_snapshot!(context.filters(), context.sync().arg("--all-groups"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 10 packages in [TIME]
    Installed 8 packages in [TIME]
     + anyio==4.3.0
     + certifi==2024.2.2
     + charset-normalizer==3.3.2
     + idna==3.6
     + iniconfig==2.0.0
     + requests==2.31.0
     + sniffio==1.3.1
     + urllib3==2.2.1
    "###);

    // Using `--no-default-groups` should exclude all groups
    uv_snapshot!(context.filters(), context.sync().arg("--no-default-groups"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 10 packages in [TIME]
    Uninstalled 8 packages in [TIME]
     - anyio==4.3.0
     - certifi==2024.2.2
     - charset-normalizer==3.3.2
     - idna==3.6
     - iniconfig==2.0.0
     - requests==2.31.0
     - sniffio==1.3.1
     - urllib3==2.2.1
    "###);

    uv_snapshot!(context.filters(), context.sync().arg("--all-groups"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 10 packages in [TIME]
    Installed 8 packages in [TIME]
     + anyio==4.3.0
     + certifi==2024.2.2
     + charset-normalizer==3.3.2
     + idna==3.6
     + iniconfig==2.0.0
     + requests==2.31.0
     + sniffio==1.3.1
     + urllib3==2.2.1
    "###);

    // Using `--no-default-groups` with `--group foo` and `--group bar` should include those groups,
    // excluding the remaining `dev` group.
    uv_snapshot!(context.filters(), context.sync().arg("--no-default-groups").arg("--group").arg("foo").arg("--group").arg("bar"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 10 packages in [TIME]
    Uninstalled 1 package in [TIME]
     - iniconfig==2.0.0
    "###);

    Ok(())
}

#[test]
fn sync_include_group() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["typing-extensions"]

        [dependency-groups]
        foo = ["anyio", {include-group = "bar"}]
        bar = ["iniconfig"]
        "#,
    )?;

    context.lock().assert().success();

    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + typing-extensions==4.10.0
    "###);

    uv_snapshot!(context.filters(), context.sync().arg("--group").arg("foo"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + iniconfig==2.0.0
     + sniffio==1.3.1
    "###);

    uv_snapshot!(context.filters(), context.sync().arg("--only-group").arg("bar"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Uninstalled 4 packages in [TIME]
     - anyio==4.3.0
     - idna==3.6
     - sniffio==1.3.1
     - typing-extensions==4.10.0
    "###);

    uv_snapshot!(context.filters(), context.sync().arg("--group").arg("foo").arg("--group").arg("bar"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + sniffio==1.3.1
     + typing-extensions==4.10.0
    "###);

    uv_snapshot!(context.filters(), context.sync().arg("--only-group").arg("foo"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Uninstalled 1 package in [TIME]
     - typing-extensions==4.10.0
    "###);

    uv_snapshot!(context.filters(), context.sync().arg("--all-groups"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Installed 1 package in [TIME]
     + typing-extensions==4.10.0
    "###);

    uv_snapshot!(context.filters(), context.sync().arg("--no-default-groups"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Uninstalled 4 packages in [TIME]
     - anyio==4.3.0
     - idna==3.6
     - iniconfig==2.0.0
     - sniffio==1.3.1
    "###);

    uv_snapshot!(context.filters(), context.sync().arg("--all-groups"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + iniconfig==2.0.0
     + sniffio==1.3.1
    "###);

    uv_snapshot!(context.filters(), context.sync().arg("--no-default-groups").arg("--group").arg("foo"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Audited 5 packages in [TIME]
    "###);

    Ok(())
}

#[test]
fn sync_exclude_group() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["typing-extensions"]

        [dependency-groups]
        foo = ["anyio", {include-group = "bar"}]
        bar = ["iniconfig"]
        "#,
    )?;

    context.lock().assert().success();

    uv_snapshot!(context.filters(), context.sync().arg("--group").arg("foo"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 5 packages in [TIME]
    Installed 5 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + iniconfig==2.0.0
     + sniffio==1.3.1
     + typing-extensions==4.10.0
    "###);

    uv_snapshot!(context.filters(), context.sync().arg("--group").arg("foo").arg("--no-group").arg("foo"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Uninstalled 4 packages in [TIME]
     - anyio==4.3.0
     - idna==3.6
     - iniconfig==2.0.0
     - sniffio==1.3.1
    "###);

    uv_snapshot!(context.filters(), context.sync().arg("--only-group").arg("bar"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
     - typing-extensions==4.10.0
    "###);

    uv_snapshot!(context.filters(), context.sync().arg("--only-group").arg("bar").arg("--no-group").arg("bar"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Uninstalled 1 package in [TIME]
     - iniconfig==2.0.0
    "###);

    Ok(())
}

#[test]
fn sync_dev_group() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["typing-extensions"]

        [tool.uv]
        dev-dependencies = ["anyio"]

        [dependency-groups]
        dev = ["iniconfig"]
        "#,
    )?;

    context.lock().assert().success();

    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 5 packages in [TIME]
    Installed 5 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + iniconfig==2.0.0
     + sniffio==1.3.1
     + typing-extensions==4.10.0
    "###);

    Ok(())
}

#[test]
fn sync_non_existent_group() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["typing-extensions"]

        [dependency-groups]
        foo = []
        bar = ["requests"]
        "#,
    )?;

    context.lock().assert().success();

    // Requesting a non-existent group should fail.
    uv_snapshot!(context.filters(), context.sync().arg("--group").arg("baz"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Group `baz` is not defined in the project's `dependency-group` table
    "###);

    uv_snapshot!(context.filters(), context.sync().arg("--no-group").arg("baz"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Group `baz` is not defined in the project's `dependency-group` table
    "###);

    // Requesting an empty group should succeed.
    uv_snapshot!(context.filters(), context.sync().arg("--group").arg("foo"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + typing-extensions==4.10.0
    "###);

    Ok(())
}

#[test]
fn sync_non_existent_default_group() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["typing-extensions"]

        [dependency-groups]
        foo = []

        [tool.uv]
        default-groups = ["bar"]
        "#,
    )?;

    context.lock().assert().success();

    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Default group `bar` (from `tool.uv.default-groups`) is not defined in the project's `dependency-group` table
    "###);

    Ok(())
}

#[test]
fn sync_default_groups() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["typing-extensions"]

        [dependency-groups]
        dev = ["iniconfig"]
        foo = ["anyio"]
        bar = ["requests"]
        "#,
    )?;

    context.lock().assert().success();

    // The `dev` group should be synced by default.
    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 10 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + iniconfig==2.0.0
     + typing-extensions==4.10.0
    "###);

    // If we remove it from the `default-groups` list, it should be removed.
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["typing-extensions"]

        [dependency-groups]
        dev = ["iniconfig"]
        foo = ["anyio"]
        bar = ["requests"]

        [tool.uv]
        default-groups = []
        "#,
    )?;

    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 10 packages in [TIME]
    Uninstalled 1 package in [TIME]
     - iniconfig==2.0.0
    "###);

    // If we set a different default group, it should be synced instead.
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["typing-extensions"]

        [dependency-groups]
        dev = ["iniconfig"]
        foo = ["anyio"]
        bar = ["requests"]

        [tool.uv]
        default-groups = ["foo"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 10 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + sniffio==1.3.1
    "###);

    // `--no-group` should remove from the defaults.
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["typing-extensions"]

        [dependency-groups]
        dev = ["iniconfig"]
        foo = ["anyio"]
        bar = ["requests"]

        [tool.uv]
        default-groups = ["foo"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.sync().arg("--no-group").arg("foo"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 10 packages in [TIME]
    Uninstalled 3 packages in [TIME]
     - anyio==4.3.0
     - idna==3.6
     - sniffio==1.3.1
    "###);

    // Using `--group` should include the defaults
    uv_snapshot!(context.filters(), context.sync().arg("--group").arg("dev"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 10 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + iniconfig==2.0.0
     + sniffio==1.3.1
    "###);

    // Using `--all-groups` should include the defaults
    uv_snapshot!(context.filters(), context.sync().arg("--all-groups"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 10 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + certifi==2024.2.2
     + charset-normalizer==3.3.2
     + requests==2.31.0
     + urllib3==2.2.1
    "###);

    // Using `--only-group` should exclude the defaults
    uv_snapshot!(context.filters(), context.sync().arg("--only-group").arg("dev"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 10 packages in [TIME]
    Uninstalled 8 packages in [TIME]
     - anyio==4.3.0
     - certifi==2024.2.2
     - charset-normalizer==3.3.2
     - idna==3.6
     - requests==2.31.0
     - sniffio==1.3.1
     - typing-extensions==4.10.0
     - urllib3==2.2.1
    "###);

    uv_snapshot!(context.filters(), context.sync().arg("--all-groups"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 10 packages in [TIME]
    Installed 8 packages in [TIME]
     + anyio==4.3.0
     + certifi==2024.2.2
     + charset-normalizer==3.3.2
     + idna==3.6
     + requests==2.31.0
     + sniffio==1.3.1
     + typing-extensions==4.10.0
     + urllib3==2.2.1
    "###);

    // Using `--no-default-groups` should exclude all groups
    uv_snapshot!(context.filters(), context.sync().arg("--no-default-groups"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 10 packages in [TIME]
    Uninstalled 8 packages in [TIME]
     - anyio==4.3.0
     - certifi==2024.2.2
     - charset-normalizer==3.3.2
     - idna==3.6
     - iniconfig==2.0.0
     - requests==2.31.0
     - sniffio==1.3.1
     - urllib3==2.2.1
    "###);

    uv_snapshot!(context.filters(), context.sync().arg("--all-groups"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 10 packages in [TIME]
    Installed 8 packages in [TIME]
     + anyio==4.3.0
     + certifi==2024.2.2
     + charset-normalizer==3.3.2
     + idna==3.6
     + iniconfig==2.0.0
     + requests==2.31.0
     + sniffio==1.3.1
     + urllib3==2.2.1
    "###);

    // Using `--no-default-groups` with `--group foo` and `--group bar` should include those groups,
    // excluding the remaining `dev` group.
    uv_snapshot!(context.filters(), context.sync().arg("--no-default-groups").arg("--group").arg("foo").arg("--group").arg("bar"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 10 packages in [TIME]
    Uninstalled 1 package in [TIME]
     - iniconfig==2.0.0
    "###);

    Ok(())
}

/// Sync with `--only-group`, where the group includes a workspace member.
#[test]
fn sync_group_member() -> Result<()> {
    let context = TestContext::new("3.12");

    // Create a workspace.
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig>=2"]

        [dependency-groups]
        foo = ["child", "typing-extensions>=4"]

        [tool.uv.workspace]
        members = ["child"]

        [tool.uv.sources]
        child = { workspace = true }
        "#,
    )?;

    // Add a workspace member.
    context
        .temp_dir
        .child("child")
        .child("pyproject.toml")
        .write_str(
            r#"
        [project]
        name = "child"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig>=1"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
        )?;

    // Generate a lockfile.
    context.lock().assert().success();

    uv_snapshot!(context.filters(), context.sync().arg("--only-group").arg("foo"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + child==0.1.0 (from file://[TEMP_DIR]/child)
     + iniconfig==2.0.0
     + typing-extensions==4.10.0
    "###);

    Ok(())
}

/// Sync with `--only-group`, where the group includes a legacy non-`[project]` workspace member.
#[test]
fn sync_group_legacy_non_project_member() -> Result<()> {
    let context = TestContext::new("3.12");

    // Create a workspace.
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [dependency-groups]
        foo = ["child", "typing-extensions>=4"]

        [tool.uv.workspace]
        members = ["child"]

        [tool.uv.sources]
        child = { workspace = true }
        "#,
    )?;

    // Add a workspace member.
    context
        .temp_dir
        .child("child")
        .child("pyproject.toml")
        .write_str(
            r#"
        [project]
        name = "child"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig>=1"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
        )?;

    // Generate a lockfile.
    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        members = [
            "child",
        ]

        [manifest.dependency-groups]
        foo = [
            { name = "child", editable = "child" },
            { name = "typing-extensions", specifier = ">=4" },
        ]

        [[package]]
        name = "child"
        version = "0.1.0"
        source = { editable = "child" }
        dependencies = [
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [{ name = "iniconfig", specifier = ">=1" }]

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
        ]

        [[package]]
        name = "typing-extensions"
        version = "4.10.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/16/3a/0d26ce356c7465a19c9ea8814b960f8a36c3b0d07c323176620b7b483e44/typing_extensions-4.10.0.tar.gz", hash = "sha256:b0abd7c89e8fb96f98db18d86106ff1d90ab692004eb746cf6eda2682f91b3cb", size = 77558 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/f9/de/dc04a3ea60b22624b51c703a84bbe0184abcd1d0b9bc8074b5d6b7ab90bb/typing_extensions-4.10.0-py3-none-any.whl", hash = "sha256:69b1a937c3a517342112fb4c6df7e72fc39a38e7891a5730ed4985b5214b5475", size = 33926 },
        ]
        "###
        );
    });

    uv_snapshot!(context.filters(), context.sync().arg("--only-group").arg("foo"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + child==0.1.0 (from file://[TEMP_DIR]/child)
     + iniconfig==2.0.0
     + typing-extensions==4.10.0
    "###);

    Ok(())
}

/// Sync with `--only-group`, where the group includes the project itself.
#[test]
fn sync_group_self() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig>=2"]

        [project.optional-dependencies]
        test = ["idna>=3"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [dependency-groups]
        foo = ["project", "typing-extensions>=4"]
        bar = ["project[test]"]
        "#,
    )?;

    // Generate a lockfile.
    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 },
        ]

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "iniconfig" },
        ]

        [package.optional-dependencies]
        test = [
            { name = "idna" },
        ]

        [package.dev-dependencies]
        bar = [
            { name = "project", extra = ["test"] },
        ]
        foo = [
            { name = "project" },
            { name = "typing-extensions" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "idna", marker = "extra == 'test'", specifier = ">=3" },
            { name = "iniconfig", specifier = ">=2" },
        ]

        [package.metadata.requires-dev]
        bar = [{ name = "project", extras = ["test"] }]
        foo = [
            { name = "project" },
            { name = "typing-extensions", specifier = ">=4" },
        ]

        [[package]]
        name = "typing-extensions"
        version = "4.10.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/16/3a/0d26ce356c7465a19c9ea8814b960f8a36c3b0d07c323176620b7b483e44/typing_extensions-4.10.0.tar.gz", hash = "sha256:b0abd7c89e8fb96f98db18d86106ff1d90ab692004eb746cf6eda2682f91b3cb", size = 77558 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/f9/de/dc04a3ea60b22624b51c703a84bbe0184abcd1d0b9bc8074b5d6b7ab90bb/typing_extensions-4.10.0-py3-none-any.whl", hash = "sha256:69b1a937c3a517342112fb4c6df7e72fc39a38e7891a5730ed4985b5214b5475", size = 33926 },
        ]
        "###
        );
    });

    uv_snapshot!(context.filters(), context.sync().arg("--only-group").arg("foo"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + iniconfig==2.0.0
     + project==0.1.0 (from file://[TEMP_DIR]/)
     + typing-extensions==4.10.0
    "###);

    uv_snapshot!(context.filters(), context.sync().arg("--only-group").arg("bar"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     + idna==3.6
     - typing-extensions==4.10.0
    "###);

    Ok(())
}

/// Regression test for <https://github.com/astral-sh/uv/issues/6316>.
///
/// Previously, we would read metadata statically from pyproject.toml and write that to `uv.lock`. In
/// this sync pass, we had also built the project with setuptools, which sorts specifiers by python
/// string sort through packaging. On the second run, we read the cache that now has the setuptools
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

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    context.sync().assert().success();
    let lock1 = context.read("uv.lock");
    // Assert we're reading static metadata.
    assert!(lock1.contains(">=4,<5"));
    assert!(!lock1.contains("<5,>=4"));
    context.sync().assert().success();
    let lock2 = context.read("uv.lock");
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

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
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
/// include all dependencies.
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

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

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
        dependencies = ["iniconfig>=1"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
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

    // Remove the virtual environment.
    fs_err::remove_dir_all(&context.venv)?;

    // We don't require the `pyproject.toml` for non-root members, if `--frozen` is provided.
    fs_err::remove_file(child.join("pyproject.toml"))?;

    uv_snapshot!(context.filters(), context.sync().arg("--no-install-workspace").arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: .venv
    Installed 4 packages in [TIME]
     + anyio==3.7.0
     + idna==3.6
     + iniconfig==2.0.0
     + sniffio==1.3.1
    "###);

    // Even if `--package` is used.
    uv_snapshot!(context.filters(), context.sync().arg("--package").arg("child").arg("--no-install-workspace").arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Uninstalled 3 packages in [TIME]
     - anyio==3.7.0
     - idna==3.6
     - sniffio==1.3.1
    "###);

    // Unless the package doesn't exist.
    uv_snapshot!(context.filters(), context.sync().arg("--package").arg("fake").arg("--no-install-workspace").arg("--frozen"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Could not find root package `fake`
    "###);

    // Even if `--all-packages` is used.
    uv_snapshot!(context.filters(), context.sync().arg("--all-packages").arg("--no-install-workspace").arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed 3 packages in [TIME]
     + anyio==3.7.0
     + idna==3.6
     + sniffio==1.3.1
    "###);

    // But we do require the root `pyproject.toml`.
    fs_err::remove_file(context.temp_dir.join("pyproject.toml"))?;

    uv_snapshot!(context.filters(), context.sync().arg("--no-install-workspace").arg("--frozen"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No `pyproject.toml` found in current directory or any parent directory
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

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
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

/// Ensure that `--no-build` isn't enforced for projects that aren't installed in the first place.
#[test]
fn no_install_project_no_build() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    // Generate a lockfile.
    context.lock().assert().success();

    // `--no-build` should raise an error, since we try to install the project.
    uv_snapshot!(context.filters(), context.sync().arg("--no-build"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    error: Distribution `project==0.1.0 @ editable+.` can't be installed because it is marked as `--no-build` but has no binary distribution
    "###);

    // But it's fine to combine `--no-install-project` with `--no-build`. We shouldn't error, since
    // we aren't building the project.
    uv_snapshot!(context.filters(), context.sync().arg("--no-install-project").arg("--no-build").arg("--locked"), @r###"
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

    Ok(())
}

/// Convert from a package to a virtual project.
#[test]
fn convert_to_virtual() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    // Running `uv sync` should install the project itself.
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

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [{ name = "iniconfig" }]
        "###
        );
    });

    // Remove the build system.
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]
        "#,
    )?;

    // Running `uv sync` should remove the project itself.
    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Uninstalled 1 package in [TIME]
     - project==0.1.0 (from file://[TEMP_DIR]/)
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
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
        "###
        );
    });

    Ok(())
}

/// Convert from a virtual project to a package.
#[test]
fn convert_to_package() -> Result<()> {
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

    // Running `uv sync` should not install the project itself.
    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
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
        "###
        );
    });

    // Add the build system.
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    // Running `uv sync` should install the project itself.
    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + project==0.1.0 (from file://[TEMP_DIR]/)
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [{ name = "iniconfig" }]
        "###
        );
    });

    Ok(())
}

#[test]
fn sync_custom_environment_path() -> Result<()> {
    let mut context = TestContext::new_with_versions(&["3.11", "3.12"])
        .with_filtered_virtualenv_bin()
        .with_filtered_python_names();

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

    // Running `uv sync` should create `.venv` by default
    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: .venv
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    "###);

    context
        .temp_dir
        .child(".venv")
        .assert(predicate::path::is_dir());

    // Running `uv sync` should create `foo` in the project directory when customized
    uv_snapshot!(context.filters(), context.sync().env(EnvVars::UV_PROJECT_ENVIRONMENT, "foo"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: foo
    Resolved 2 packages in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    "###);

    context
        .temp_dir
        .child("foo")
        .assert(predicate::path::is_dir());

    // We don't delete `.venv`, though we arguably could
    context
        .temp_dir
        .child(".venv")
        .assert(predicate::path::is_dir());

    // An absolute path can be provided
    uv_snapshot!(context.filters(), context.sync().env(EnvVars::UV_PROJECT_ENVIRONMENT, "foobar/.venv"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: foobar/.venv
    Resolved 2 packages in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    "###);

    context
        .temp_dir
        .child("foobar")
        .assert(predicate::path::is_dir());

    context
        .temp_dir
        .child("foobar")
        .child(".venv")
        .assert(predicate::path::is_dir());

    // An absolute path can be provided
    uv_snapshot!(context.filters(), context.sync().env(EnvVars::UV_PROJECT_ENVIRONMENT, context.temp_dir.join("bar")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: bar
    Resolved 2 packages in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    "###);

    context
        .temp_dir
        .child("bar")
        .assert(predicate::path::is_dir());

    // And, it can be outside the project
    let tempdir = tempdir_in(TestContext::test_bucket_dir())?;
    context = context.with_filtered_path(tempdir.path(), "OTHER_TEMPDIR");
    uv_snapshot!(context.filters(), context.sync().env(EnvVars::UV_PROJECT_ENVIRONMENT, tempdir.path().join(".venv")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: [OTHER_TEMPDIR]/.venv
    Resolved 2 packages in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    "###);

    ChildPath::new(tempdir.path())
        .child(".venv")
        .assert(predicate::path::is_dir());

    // If the directory already exists and is not a virtual environment we should fail with an error
    fs_err::remove_dir_all(context.temp_dir.join("foo"))?;
    fs_err::create_dir(context.temp_dir.join("foo"))?;
    fs_err::write(context.temp_dir.join("foo").join("file"), b"")?;
    uv_snapshot!(context.filters(), context.sync().env(EnvVars::UV_PROJECT_ENVIRONMENT, "foo"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Project virtual environment directory `[TEMP_DIR]/foo` cannot be used because it is not a valid Python environment (no Python executable was found)
    "###);

    // But if it's just an incompatible virtual environment...
    fs_err::remove_dir_all(context.temp_dir.join("foo"))?;
    uv_snapshot!(context.filters(), context.venv().arg("foo").arg("--python").arg("3.11"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.11.[X] interpreter at: [PYTHON-3.11]
    warning: The requested interpreter resolved to Python 3.11.[X], which is incompatible with the project's Python requirement: `>=3.12`
    Creating virtual environment at: foo
    Activate with: source foo/[BIN]/activate
    "###);

    // Even with some extraneous content...
    fs_err::write(context.temp_dir.join("foo").join("file"), b"")?;

    // We can delete and use it
    uv_snapshot!(context.filters(), context.sync().env(EnvVars::UV_PROJECT_ENVIRONMENT, "foo"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Removed virtual environment at: foo
    Creating virtual environment at: foo
    Resolved 2 packages in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    "###);

    Ok(())
}

#[test]
#[cfg(feature = "git")]
fn sync_workspace_custom_environment_path() -> Result<()> {
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

    // Create a workspace member
    context.init().arg("child").assert().success();

    // Running `uv sync` should create `.venv` in the workspace root
    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    "###);

    context
        .temp_dir
        .child(".venv")
        .assert(predicate::path::is_dir());

    // Similarly, `uv sync` from the child project uses `.venv` in the workspace root
    uv_snapshot!(context.filters(), context.sync().current_dir(context.temp_dir.join("child")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Uninstalled 1 package in [TIME]
     - iniconfig==2.0.0
    "###);

    context
        .temp_dir
        .child(".venv")
        .assert(predicate::path::is_dir());

    context
        .temp_dir
        .child("child")
        .child(".venv")
        .assert(predicate::path::missing());

    // Running `uv sync` should create `foo` in the workspace root when customized
    uv_snapshot!(context.filters(), context.sync().env(EnvVars::UV_PROJECT_ENVIRONMENT, "foo"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: foo
    Resolved 3 packages in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    "###);

    context
        .temp_dir
        .child("foo")
        .assert(predicate::path::is_dir());

    // We don't delete `.venv`, though we arguably could
    context
        .temp_dir
        .child(".venv")
        .assert(predicate::path::is_dir());

    // Similarly, `uv sync` from the child project uses `foo` relative to  the workspace root
    uv_snapshot!(context.filters(), context.sync().env(EnvVars::UV_PROJECT_ENVIRONMENT, "foo").current_dir(context.temp_dir.join("child")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Uninstalled 1 package in [TIME]
     - iniconfig==2.0.0
    "###);

    context
        .temp_dir
        .child("foo")
        .assert(predicate::path::is_dir());

    context
        .temp_dir
        .child("child")
        .child("foo")
        .assert(predicate::path::missing());

    // And, `uv sync --package child` uses `foo` relative to  the workspace root
    uv_snapshot!(context.filters(), context.sync().arg("--package").arg("child").env(EnvVars::UV_PROJECT_ENVIRONMENT, "foo"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Audited in [TIME]
    "###);

    context
        .temp_dir
        .child("foo")
        .assert(predicate::path::is_dir());

    context
        .temp_dir
        .child("child")
        .child("foo")
        .assert(predicate::path::missing());

    Ok(())
}

#[test]
fn sync_empty_virtual_environment() -> Result<()> {
    let context = TestContext::new_with_versions(&["3.12"]);

    // Create an empty directory
    context.temp_dir.child(".venv").create_dir_all()?;

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

    // Running `uv sync` should work
    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: .venv
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    "###);

    Ok(())
}

/// Test for warnings when `VIRTUAL_ENV` is set but will not be respected.
#[test]
fn sync_legacy_non_project_warning() -> Result<()> {
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

    // We should not warn if it matches the project environment
    uv_snapshot!(context.filters(), context.sync().env(EnvVars::VIRTUAL_ENV, context.temp_dir.join(".venv")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    "###);

    // Including if it's a relative path that matches
    uv_snapshot!(context.filters(), context.sync().env(EnvVars::VIRTUAL_ENV, ".venv"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Audited 1 package in [TIME]
    "###);

    // Or, if it's a link that resolves to the same path
    #[cfg(unix)]
    {
        use fs_err::os::unix::fs::symlink;

        let link = context.temp_dir.join("link");
        symlink(context.temp_dir.join(".venv"), &link)?;

        uv_snapshot!(context.filters(), context.sync().env(EnvVars::VIRTUAL_ENV, link), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Resolved 2 packages in [TIME]
        Audited 1 package in [TIME]
        "###);
    }

    // But we should warn if it's a different path
    uv_snapshot!(context.filters(), context.sync().env(EnvVars::VIRTUAL_ENV, "foo"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `VIRTUAL_ENV=foo` does not match the project environment path `.venv` and will be ignored
    Resolved 2 packages in [TIME]
    Audited 1 package in [TIME]
    "###);

    // Including absolute paths
    uv_snapshot!(context.filters(), context.sync().env(EnvVars::VIRTUAL_ENV, context.temp_dir.join("foo")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `VIRTUAL_ENV=foo` does not match the project environment path `.venv` and will be ignored
    Resolved 2 packages in [TIME]
    Audited 1 package in [TIME]
    "###);

    // We should not warn if the project environment has been customized and matches
    uv_snapshot!(context.filters(), context.sync().env(EnvVars::VIRTUAL_ENV, "foo").env(EnvVars::UV_PROJECT_ENVIRONMENT, "foo"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: foo
    Resolved 2 packages in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    "###);

    // But we should warn if they don't match still
    uv_snapshot!(context.filters(), context.sync().env(EnvVars::VIRTUAL_ENV, "foo").env(EnvVars::UV_PROJECT_ENVIRONMENT, "bar"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `VIRTUAL_ENV=foo` does not match the project environment path `bar` and will be ignored
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: bar
    Resolved 2 packages in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    "###);

    let child = context.temp_dir.child("child");
    child.create_dir_all()?;

    // And `VIRTUAL_ENV` is resolved relative to the project root so with relative paths we should
    // warn from a child too
    uv_snapshot!(context.filters(), context.sync().env(EnvVars::VIRTUAL_ENV, "foo").env(EnvVars::UV_PROJECT_ENVIRONMENT, "foo").current_dir(&child), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `VIRTUAL_ENV=foo` does not match the project environment path `[TEMP_DIR]/foo` and will be ignored
    Resolved 2 packages in [TIME]
    Audited 1 package in [TIME]
    "###);

    // But, a matching absolute path shouldn't warn
    uv_snapshot!(context.filters(), context.sync().env(EnvVars::VIRTUAL_ENV, context.temp_dir.join("foo")).env(EnvVars::UV_PROJECT_ENVIRONMENT, "foo").current_dir(&child), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Audited 1 package in [TIME]
    "###);

    Ok(())
}

#[test]
fn sync_update_project() -> Result<()> {
    let context = TestContext::new_with_versions(&["3.12"]);

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "my-project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: .venv
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    "###);

    // Bump the project version.
    pyproject_toml.write_str(
        r#"
        [project]
        name = "my-project"
        version = "0.2.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + my-project==0.2.0 (from file://[TEMP_DIR]/)
    "###);

    Ok(())
}

#[test]
fn sync_environment_prompt() -> Result<()> {
    let context = TestContext::new_with_versions(&["3.12"]);

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "my-project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]
        "#,
    )?;

    // Running `uv sync` should create `.venv`
    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: .venv
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    "###);

    // The `pyvenv.cfg` should contain the prompt matching the project name
    let pyvenv_cfg = context.read(".venv/pyvenv.cfg");

    assert!(pyvenv_cfg.contains("prompt = my-project"));

    Ok(())
}

#[test]
fn no_binary() -> Result<()> {
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

    context.lock().assert().success();

    uv_snapshot!(context.filters(), context.sync().arg("--no-binary-package").arg("iniconfig"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    "###);

    assert!(context.temp_dir.child("uv.lock").exists());

    Ok(())
}

#[test]
fn no_binary_error() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["odrive"]
        "#,
    )?;

    context.lock().assert().success();

    uv_snapshot!(context.filters(), context.sync().arg("--no-binary-package").arg("odrive"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 31 packages in [TIME]
    error: Distribution `odrive==0.6.8 @ registry+https://pypi.org/simple` can't be installed because it is marked as `--no-binary` but has no source distribution
    "###);

    assert!(context.temp_dir.child("uv.lock").exists());

    Ok(())
}

#[test]
fn no_build() -> Result<()> {
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

    context.lock().assert().success();

    uv_snapshot!(context.filters(), context.sync().arg("--no-build-package").arg("iniconfig"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    "###);

    assert!(context.temp_dir.child("uv.lock").exists());

    Ok(())
}

#[test]
fn no_build_error() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["django_allauth==0.51.0"]
        "#,
    )?;

    context.lock().assert().success();

    uv_snapshot!(context.filters(), context.sync().arg("--no-build-package").arg("django-allauth"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 19 packages in [TIME]
    error: Distribution `django-allauth==0.51.0 @ registry+https://pypi.org/simple` can't be installed because it is marked as `--no-build` but has no binary distribution
    "###);

    assert!(context.temp_dir.child("uv.lock").exists());

    Ok(())
}

#[test]
fn sync_wheel_url_source_error() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "uv-test"
        version = "0.0.0"
        requires-python = ">=3.10"
        dependencies = [
            "cffi @ https://files.pythonhosted.org/packages/08/fd/cc2fedbd887223f9f5d170c96e57cbf655df9831a6546c1727ae13fa977a/cffi-1.17.1-cp310-cp310-macosx_11_0_arm64.whl",
        ]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###);

    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    error: Distribution `cffi==1.17.1 @ direct+https://files.pythonhosted.org/packages/08/fd/cc2fedbd887223f9f5d170c96e57cbf655df9831a6546c1727ae13fa977a/cffi-1.17.1-cp310-cp310-macosx_11_0_arm64.whl` can't be installed because the binary distribution is incompatible with the current platform

    hint: You're using CPython 3.12 (`cp312`), but  `cffi` (v1.17.1) only has wheels with the following Python ABI tag: `cp310`
    "###);

    Ok(())
}

#[test]
fn sync_wheel_path_source_error() -> Result<()> {
    let context = TestContext::new("3.12");

    // Download a wheel.
    let archive = context
        .temp_dir
        .child("cffi-1.17.1-cp310-cp310-macosx_11_0_arm64.whl");
    download_to_disk(
        "https://files.pythonhosted.org/packages/08/fd/cc2fedbd887223f9f5d170c96e57cbf655df9831a6546c1727ae13fa977a/cffi-1.17.1-cp310-cp310-macosx_11_0_arm64.whl",
        &archive,
    );

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "uv-test"
        version = "0.0.0"
        requires-python = ">=3.10"
        dependencies = ["cffi"]

        [tool.uv.sources]
        cffi = { path = "cffi-1.17.1-cp310-cp310-macosx_11_0_arm64.whl" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###);

    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    error: Distribution `cffi==1.17.1 @ path+cffi-1.17.1-cp310-cp310-macosx_11_0_arm64.whl` can't be installed because the binary distribution is incompatible with the current platform

    hint: You're using CPython 3.12 (`cp312`), but  `cffi` (v1.17.1) only has wheels with the following Python ABI tag: `cp310`
    "###);

    Ok(())
}

/// Avoid installing dev dependencies of transitive dependencies.
#[test]
fn transitive_dev() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "root"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["child"]

        [tool.uv]
        dev-dependencies = ["anyio>3"]

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

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv]
        dev-dependencies = ["iniconfig>=1"]
        "#,
    )?;

    let src = child.child("src").child("albatross");
    src.create_dir_all()?;

    let init = src.child("__init__.py");
    init.touch()?;

    uv_snapshot!(context.filters(), context.sync().arg("--dev"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.3.0
     + child==0.1.0 (from file://[TEMP_DIR]/child)
     + idna==3.6
     + sniffio==1.3.1
    "###);

    Ok(())
}

/// Avoid installing dev dependencies of transitive dependencies.
#[test]
fn sync_no_editable() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "root"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["child"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

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

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    let src = child.child("src").child("child");
    src.create_dir_all()?;

    let init = src.child("__init__.py");
    init.touch()?;

    uv_snapshot!(context.filters(), context.sync().arg("--no-editable"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + child==0.1.0 (from file://[TEMP_DIR]/child)
     + root==0.1.0 (from file://[TEMP_DIR]/)
    "###);

    // Remove the project.
    fs_err::remove_dir_all(&child)?;

    // Ensure that we can still import it.
    uv_snapshot!(context.filters(), context.run().arg("--no-sync").arg("python").arg("-c").arg("import child"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    "###);

    Ok(())
}

#[test]
/// Check warning message for <https://github.com/astral-sh/uv/issues/6998>
/// if no `build-system` section is defined.
fn sync_scripts_without_build_system() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "foo"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [project.scripts]
        entry = "foo:custom_entry"
        "#,
    )?;

    let test_script = context.temp_dir.child("src/__init__.py");
    test_script.write_str(
        r#"
        def custom_entry():
            print!("Hello")
       "#,
    )?;

    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: Skipping installation of entry points (`project.scripts`) because this project is not packaged; to install entry points, set `tool.uv.package = true` or define a `build-system`
    Resolved 1 package in [TIME]
    Audited in [TIME]
    "###);

    Ok(())
}

#[test]
/// Check warning message for <https://github.com/astral-sh/uv/issues/6998>
/// if the project is marked as `package = false`.
fn sync_scripts_project_not_packaged() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "foo"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [project.scripts]
        entry = "foo:custom_entry"

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"

        [tool.uv]
        package = false
        "#,
    )?;

    let test_script = context.temp_dir.child("src/__init__.py");
    test_script.write_str(
        r#"
        def custom_entry():
            print!("Hello")
       "#,
    )?;

    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: Skipping installation of entry points (`project.scripts`) because this project is not packaged; to install entry points, set `tool.uv.package = true` or define a `build-system`
    Resolved 1 package in [TIME]
    Audited in [TIME]
    "###);

    Ok(())
}

#[test]
fn sync_dynamic_extra() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]
        dynamic = ["optional-dependencies"]

        [tool.setuptools.dynamic.optional-dependencies]
        dev = { file = "requirements-dev.txt" }

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    context
        .temp_dir
        .child("requirements-dev.txt")
        .write_str("typing-extensions")?;

    uv_snapshot!(context.filters(), context.sync().arg("--extra").arg("dev"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + iniconfig==2.0.0
     + project==0.1.0 (from file://[TEMP_DIR]/)
     + typing-extensions==4.10.0
    "###);

    let lock = context.read("uv.lock");

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
            name = "iniconfig"
            version = "2.0.0"
            source = { registry = "https://pypi.org/simple" }
            sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
            wheels = [
                { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
            ]

            [[package]]
            name = "project"
            version = "0.1.0"
            source = { editable = "." }
            dependencies = [
                { name = "iniconfig" },
            ]

            [package.optional-dependencies]
            dev = [
                { name = "typing-extensions" },
            ]

            [package.metadata]
            requires-dist = [
                { name = "iniconfig" },
                { name = "typing-extensions", marker = "extra == 'dev'" },
            ]

            [[package]]
            name = "typing-extensions"
            version = "4.10.0"
            source = { registry = "https://pypi.org/simple" }
            sdist = { url = "https://files.pythonhosted.org/packages/16/3a/0d26ce356c7465a19c9ea8814b960f8a36c3b0d07c323176620b7b483e44/typing_extensions-4.10.0.tar.gz", hash = "sha256:b0abd7c89e8fb96f98db18d86106ff1d90ab692004eb746cf6eda2682f91b3cb", size = 77558 }
            wheels = [
                { url = "https://files.pythonhosted.org/packages/f9/de/dc04a3ea60b22624b51c703a84bbe0184abcd1d0b9bc8074b5d6b7ab90bb/typing_extensions-4.10.0-py3-none-any.whl", hash = "sha256:69b1a937c3a517342112fb4c6df7e72fc39a38e7891a5730ed4985b5214b5475", size = 33926 },
            ]
            "###
            );
        }
    );

    // Check that we can re-read the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Uninstalled 1 package in [TIME]
     - typing-extensions==4.10.0
    "###);

    Ok(())
}

#[test]
fn build_system_requires_workspace() -> Result<()> {
    let context = TestContext::new("3.12");

    let build = context.temp_dir.child("backend");
    build.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "backend"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["typing-extensions>=3.10"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    build
        .child("src")
        .child("backend")
        .child("__init__.py")
        .write_str(indoc! { r#"
            def hello() -> str:
                return "Hello, world!"
        "#})?;
    build.child("README.md").touch()?;

    let pyproject_toml = context.temp_dir.child("project").child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig>=1"]

        [build-system]
        requires = ["setuptools>=42", "backend==0.1.0"]
        build-backend = "setuptools.build_meta"

        [tool.uv.workspace]
        members = ["../backend"]

        [tool.uv.sources]
        backend = { workspace = true }
        "#,
    )?;

    context
        .temp_dir
        .child("project")
        .child("setup.py")
        .write_str(indoc! {r"
        from setuptools import setup

        from backend import hello

        hello()

        setup()
        ",
        })?;

    uv_snapshot!(context.filters(), context.sync().current_dir(context.temp_dir.child("project")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: .venv
    Resolved 4 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + iniconfig==2.0.0
     + project==0.1.0 (from file://[TEMP_DIR]/project)
    "###);

    Ok(())
}

#[test]
fn build_system_requires_path() -> Result<()> {
    let context = TestContext::new("3.12");

    let build = context.temp_dir.child("backend");
    build.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "backend"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["typing-extensions>=3.10"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    build
        .child("src")
        .child("backend")
        .child("__init__.py")
        .write_str(indoc! { r#"
            def hello() -> str:
                return "Hello, world!"
        "#})?;
    build.child("README.md").touch()?;

    let pyproject_toml = context.temp_dir.child("project").child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig>=1"]

        [build-system]
        requires = ["setuptools>=42", "backend==0.1.0"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sources]
        backend = { path = "../backend" }
        "#,
    )?;

    context
        .temp_dir
        .child("project")
        .child("setup.py")
        .write_str(indoc! {r"
        from setuptools import setup

        from backend import hello

        hello()

        setup()
        ",
        })?;

    uv_snapshot!(context.filters(), context.sync().current_dir(context.temp_dir.child("project")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: .venv
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + iniconfig==2.0.0
     + project==0.1.0 (from file://[TEMP_DIR]/project)
    "###);

    Ok(())
}

#[test]
fn sync_invalid_environment() -> Result<()> {
    let context = TestContext::new_with_versions(&["3.11", "3.12"])
        .with_filtered_virtualenv_bin()
        .with_filtered_python_names();

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

    // If the directory already exists and is not a virtual environment we should fail with an error
    fs_err::create_dir(context.temp_dir.join(".venv"))?;
    fs_err::write(context.temp_dir.join(".venv").join("file"), b"")?;
    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Project virtual environment directory `[VENV]/` cannot be used because it is not a valid Python environment (no Python executable was found)
    "###);

    // But if it's just an incompatible virtual environment...
    fs_err::remove_dir_all(context.temp_dir.join(".venv"))?;
    uv_snapshot!(context.filters(), context.venv().arg("--python").arg("3.11"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.11.[X] interpreter at: [PYTHON-3.11]
    warning: The requested interpreter resolved to Python 3.11.[X], which is incompatible with the project's Python requirement: `>=3.12`
    Creating virtual environment at: .venv
    Activate with: source .venv/[BIN]/activate
    "###);

    // Even with some extraneous content...
    fs_err::write(context.temp_dir.join(".venv").join("file"), b"")?;

    // We can delete and use it
    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Removed virtual environment at: .venv
    Creating virtual environment at: .venv
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    "###);

    let bin = venv_bin_path(context.temp_dir.join(".venv"));

    // If there's just a broken symlink, we should warn
    #[cfg(unix)]
    {
        fs_err::remove_file(bin.join("python"))?;
        fs_err::os::unix::fs::symlink(context.temp_dir.join("does-not-exist"), bin.join("python"))?;
        uv_snapshot!(context.filters(), context.sync(), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        warning: Ignoring existing virtual environment linked to non-existent Python interpreter: .venv/[BIN]/python -> python
        Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
        Removed virtual environment at: .venv
        Creating virtual environment at: .venv
        Resolved 2 packages in [TIME]
        Installed 1 package in [TIME]
         + iniconfig==2.0.0
        "###);
    }

    // But if the Python executable is missing entirely we should also fail
    fs_err::remove_dir_all(&bin)?;
    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Project virtual environment directory `[VENV]/` cannot be used because it is not a valid Python environment (no Python executable was found)
    "###);

    // But if it's not a virtual environment...
    fs_err::remove_dir_all(context.temp_dir.join(".venv"))?;
    uv_snapshot!(context.filters(), context.venv().arg("--python").arg("3.11"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.11.[X] interpreter at: [PYTHON-3.11]
    warning: The requested interpreter resolved to Python 3.11.[X], which is incompatible with the project's Python requirement: `>=3.12`
    Creating virtual environment at: .venv
    Activate with: source .venv/[BIN]/activate
    "###);

    // Which we detect by the presence of a `pyvenv.cfg` file
    fs_err::remove_file(context.temp_dir.join(".venv").join("pyvenv.cfg"))?;

    // Let's make sure some extraneous content isn't removed
    fs_err::write(context.temp_dir.join(".venv").join("file"), b"")?;

    // We should never delete it
    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    error: Project virtual environment directory `[VENV]/` cannot be used because it is not a compatible environment but cannot be recreated because it is not a virtual environment
    "###);

    context
        .temp_dir
        .child(".venv")
        .assert(predicate::path::is_dir());

    context
        .temp_dir
        .child(".venv")
        .child("file")
        .assert(predicate::path::is_file());

    Ok(())
}

/// Avoid validating workspace members when `--no-sources` is provided. Rather than reporting that
/// `./anyio` is missing, install `anyio` from the registry.
#[test]
fn sync_no_sources_missing_member() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "root"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio"]

        [tool.uv.sources]
        anyio = { workspace = true }

        [tool.uv.workspace]
        members = ["anyio"]
        "#,
    )?;

    let src = context.temp_dir.child("src").child("albatross");
    src.create_dir_all()?;

    let init = src.child("__init__.py");
    init.touch()?;

    uv_snapshot!(context.filters(), context.sync().arg("--no-sources"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + sniffio==1.3.1
    "###);

    Ok(())
}

#[test]
fn sync_python_version() -> Result<()> {
    let context: TestContext = TestContext::new_with_versions(&["3.10", "3.11", "3.12"]);

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc::indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.11"
        dependencies = ["anyio==3.7.0"]
    "#})?;

    // We should respect the project's required version, not the first on the path
    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.11.[X] interpreter at: [PYTHON-3.11]
    Creating virtual environment at: .venv
    Resolved 4 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==3.7.0
     + idna==3.6
     + sniffio==1.3.1
    "###);

    // Unless explicitly requested...
    uv_snapshot!(context.filters(), context.sync().arg("--python").arg("3.10"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.10.[X] interpreter at: [PYTHON-3.10]
    error: The requested interpreter resolved to Python 3.10.[X], which is incompatible with the project's Python requirement: `>=3.11`
    "###);

    // But a pin should take precedence
    uv_snapshot!(context.filters(), context.python_pin().arg("3.12"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Pinned `.python-version` to `3.12`

    ----- stderr -----
    "###);

    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Removed virtual environment at: .venv
    Creating virtual environment at: .venv
    Resolved 4 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==3.7.0
     + idna==3.6
     + sniffio==1.3.1
    "###);

    // Create a pin that's incompatible with the project
    uv_snapshot!(context.filters(), context.python_pin().arg("3.10").arg("--no-workspace"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Updated `.python-version` from `3.12` -> `3.10`

    ----- stderr -----
    "###);

    // We should warn on subsequent uses, but respect the pinned version?
    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.10.[X] interpreter at: [PYTHON-3.10]
    error: The Python request from `.python-version` resolved to Python 3.10.[X], which is incompatible with the project's Python requirement: `>=3.11`. Use `uv python pin` to update the `.python-version` file to a compatible version.
    "###);

    // Unless the pin file is outside the project, in which case we should just ignore it entirely
    let child_dir = context.temp_dir.child("child");
    child_dir.create_dir_all().unwrap();

    let pyproject_toml = child_dir.child("pyproject.toml");
    pyproject_toml
        .write_str(indoc::indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.11"
        dependencies = ["anyio==3.7.0"]
    "#})
        .unwrap();

    uv_snapshot!(context.filters(), context.sync().current_dir(&child_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.11.[X] interpreter at: [PYTHON-3.11]
    Creating virtual environment at: .venv
    Resolved 4 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==3.7.0
     + idna==3.6
     + sniffio==1.3.1
    "###);

    Ok(())
}

#[test]
fn sync_explicit() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "root"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "idna>2",
        ]

        [[tool.uv.index]]
        name = "test"
        url = "https://test.pypi.org/simple"
        explicit = true

        [tool.uv.sources]
        idna = { index = "test" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + idna==2.7
    "###);

    // Clear the environment.
    fs_err::remove_dir_all(&context.venv)?;

    // The package should be drawn from the cache.
    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: .venv
    Resolved 2 packages in [TIME]
    Installed 1 package in [TIME]
     + idna==2.7
    "###);

    Ok(())
}

/// Sync all members in a workspace.
#[test]
fn sync_all() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio>3", "child"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.workspace]
        members = ["child"]

        [tool.uv.sources]
        child = { workspace = true }
        "#,
    )?;
    context
        .temp_dir
        .child("src")
        .child("project")
        .child("__init__.py")
        .touch()?;

    // Add a workspace member.
    let child = context.temp_dir.child("child");
    child.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "child"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig>=1"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;
    child
        .child("src")
        .child("child")
        .child("__init__.py")
        .touch()?;

    // Generate a lockfile.
    context.lock().assert().success();

    // Sync all workspace members.
    uv_snapshot!(context.filters(), context.sync().arg("--all-packages"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 6 packages in [TIME]
    Installed 6 packages in [TIME]
     + anyio==4.3.0
     + child==0.1.0 (from file://[TEMP_DIR]/child)
     + idna==3.6
     + iniconfig==2.0.0
     + project==0.1.0 (from file://[TEMP_DIR]/)
     + sniffio==1.3.1
    "###);

    Ok(())
}

/// Sync all members in a workspace with extras attached.
#[test]
fn sync_all_extras() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["child"]

        [project.optional-dependencies]
        types = ["sniffio>1"]
        async = ["anyio>3"]

        [tool.uv.workspace]
        members = ["child"]

        [tool.uv.sources]
        child = { workspace = true }
        "#,
    )?;
    context
        .temp_dir
        .child("src")
        .child("project")
        .child("__init__.py")
        .touch()?;

    // Add a workspace member.
    let child = context.temp_dir.child("child");
    child.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "child"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig>=1"]

        [project.optional-dependencies]
        types = ["typing-extensions>=4"]
        testing = ["packaging>=24"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;
    child
        .child("src")
        .child("child")
        .child("__init__.py")
        .touch()?;

    // Generate a lockfile.
    context.lock().assert().success();

    // Sync an extra that exists in both the parent and child.
    uv_snapshot!(context.filters(), context.sync().arg("--all-packages").arg("--extra").arg("types"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 8 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + child==0.1.0 (from file://[TEMP_DIR]/child)
     + iniconfig==2.0.0
     + sniffio==1.3.1
     + typing-extensions==4.10.0
    "###);

    // Sync an extra that only exists in the child.
    uv_snapshot!(context.filters(), context.sync().arg("--all-packages").arg("--extra").arg("testing"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 8 packages in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 2 packages in [TIME]
    Installed 1 package in [TIME]
     + packaging==24.0
     - sniffio==1.3.1
     - typing-extensions==4.10.0
    "###);

    // Sync all extras.
    uv_snapshot!(context.filters(), context.sync().arg("--all-packages").arg("--all-extras"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 8 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + sniffio==1.3.1
     + typing-extensions==4.10.0
    "###);

    // Sync all extras.
    uv_snapshot!(context.filters(), context.sync().arg("--all-packages").arg("--all-extras").arg("--no-extra").arg("types"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 8 packages in [TIME]
    Uninstalled 1 package in [TIME]
     - typing-extensions==4.10.0
    "###);

    Ok(())
}

/// Sync all members in a workspace with dependency groups attached.
#[test]
fn sync_all_groups() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["child"]

        [dependency-groups]
        types = ["sniffio>=1"]
        async = ["anyio>=3"]

        [tool.uv.workspace]
        members = ["child"]

        [tool.uv.sources]
        child = { workspace = true }
        "#,
    )?;
    context
        .temp_dir
        .child("src")
        .child("project")
        .child("__init__.py")
        .touch()?;

    // Add a workspace member.
    let child = context.temp_dir.child("child");
    child.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "child"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig>=1"]

        [dependency-groups]
        types = ["typing-extensions>=4"]
        testing = ["packaging>=24"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;
    child
        .child("src")
        .child("child")
        .child("__init__.py")
        .touch()?;

    // Generate a lockfile.
    context.lock().assert().success();

    // Sync a group that exists in both the parent and child.
    uv_snapshot!(context.filters(), context.sync().arg("--all-packages").arg("--group").arg("types"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 8 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + child==0.1.0 (from file://[TEMP_DIR]/child)
     + iniconfig==2.0.0
     + sniffio==1.3.1
     + typing-extensions==4.10.0
    "###);

    // Sync a group that only exists in the child.
    uv_snapshot!(context.filters(), context.sync().arg("--all-packages").arg("--group").arg("testing"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 8 packages in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 2 packages in [TIME]
    Installed 1 package in [TIME]
     + packaging==24.0
     - sniffio==1.3.1
     - typing-extensions==4.10.0
    "###);

    // Sync a group that doesn't exist.
    uv_snapshot!(context.filters(), context.sync().arg("--all-packages").arg("--group").arg("foo"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Group `foo` is not defined in any project's `dependency-group` table
    "###);

    Ok(())
}

#[test]
fn sync_multiple_sources_index_disjoint_extras() -> Result<()> {
    let context = TestContext::new("3.12").with_exclude_newer("2025-01-30T00:00Z");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [project.optional-dependencies]
        cu118 = ["jinja2==3.1.2"]
        cu124 = ["jinja2==3.1.3"]

        [tool.uv]
        constraint-dependencies = ["markupsafe<3"]
        conflicts = [
            [
                { extra = "cu118" },
                { extra = "cu124" },
            ],
        ]

        [tool.uv.sources]
        jinja2 = [
            { index = "torch-cu118", extra = "cu118" },
            { index = "torch-cu124", extra = "cu124" },
        ]

        [[tool.uv.index]]
        name = "torch-cu118"
        url = "https://astral-sh.github.io/pytorch-mirror/whl/cu118"
        explicit = true

        [[tool.uv.index]]
        name = "torch-cu124"
        url = "https://astral-sh.github.io/pytorch-mirror/whl/cu124"
        explicit = true
        "#,
    )?;

    // Generate a lockfile.
    context.lock().assert().success();

    uv_snapshot!(context.filters(), context.sync().arg("--extra").arg("cu124"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + jinja2==3.1.3
     + markupsafe==2.1.5
    "###);

    Ok(())
}

#[test]
fn sync_derivation_chain() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["wsgiref"]

        [[tool.uv.dependency-metadata]]
        name = "wsgiref"
        version = "0.1.2"
        dependencies = []
        "#,
    )?;

    let filters = context
        .filters()
        .into_iter()
        .chain([
            (r"exit code: 1", "exit status: 1"),
            (r"/.*/src", "/[TMP]/src"),
        ])
        .collect::<Vec<_>>();

    uv_snapshot!(filters, context.sync(), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
      × Failed to build `wsgiref==0.1.2`
      ├─▶ The build backend returned an error
      ╰─▶ Call to `setuptools.build_meta:__legacy__.build_wheel` failed (exit status: 1)

          [stderr]
          Traceback (most recent call last):
            File "<string>", line 14, in <module>
            File "[CACHE_DIR]/builds-v0/[TMP]/build_meta.py", line 325, in get_requires_for_build_wheel
              return self._get_build_requires(config_settings, requirements=['wheel'])
                     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
            File "[CACHE_DIR]/builds-v0/[TMP]/build_meta.py", line 295, in _get_build_requires
              self.run_setup()
            File "[CACHE_DIR]/builds-v0/[TMP]/build_meta.py", line 487, in run_setup
              super().run_setup(setup_script=setup_script)
            File "[CACHE_DIR]/builds-v0/[TMP]/build_meta.py", line 311, in run_setup
              exec(code, locals())
            File "<string>", line 5, in <module>
            File "[CACHE_DIR]/[TMP]/src/ez_setup/__init__.py", line 170
              print "Setuptools version",version,"or greater has been installed."
              ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
          SyntaxError: Missing parentheses in call to 'print'. Did you mean print(...)?

          hint: This usually indicates a problem with the package or the build environment.
      help: `wsgiref` (v0.1.2) was included because `project` (v0.1.0) depends on `wsgiref`
    "###);

    Ok(())
}

#[test]
fn sync_derivation_chain_extra() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
        optional-dependencies = { wsgi = ["wsgiref"] }

        [[tool.uv.dependency-metadata]]
        name = "wsgiref"
        version = "0.1.2"
        dependencies = []
        "#,
    )?;

    let filters = context
        .filters()
        .into_iter()
        .chain([
            (r"exit code: 1", "exit status: 1"),
            (r"/.*/src", "/[TMP]/src"),
        ])
        .collect::<Vec<_>>();

    uv_snapshot!(filters, context.sync().arg("--extra").arg("wsgi"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
      × Failed to build `wsgiref==0.1.2`
      ├─▶ The build backend returned an error
      ╰─▶ Call to `setuptools.build_meta:__legacy__.build_wheel` failed (exit status: 1)

          [stderr]
          Traceback (most recent call last):
            File "<string>", line 14, in <module>
            File "[CACHE_DIR]/builds-v0/[TMP]/build_meta.py", line 325, in get_requires_for_build_wheel
              return self._get_build_requires(config_settings, requirements=['wheel'])
                     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
            File "[CACHE_DIR]/builds-v0/[TMP]/build_meta.py", line 295, in _get_build_requires
              self.run_setup()
            File "[CACHE_DIR]/builds-v0/[TMP]/build_meta.py", line 487, in run_setup
              super().run_setup(setup_script=setup_script)
            File "[CACHE_DIR]/builds-v0/[TMP]/build_meta.py", line 311, in run_setup
              exec(code, locals())
            File "<string>", line 5, in <module>
            File "[CACHE_DIR]/[TMP]/src/ez_setup/__init__.py", line 170
              print "Setuptools version",version,"or greater has been installed."
              ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
          SyntaxError: Missing parentheses in call to 'print'. Did you mean print(...)?

          hint: This usually indicates a problem with the package or the build environment.
      help: `wsgiref` (v0.1.2) was included because `project[wsgi]` (v0.1.0) depends on `wsgiref`
    "###);

    Ok(())
}

#[test]
fn sync_derivation_chain_group() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [dependency-groups]
        wsgi = ["wsgiref"]

        [[tool.uv.dependency-metadata]]
        name = "wsgiref"
        version = "0.1.2"
        dependencies = []
        "#,
    )?;

    let filters = context
        .filters()
        .into_iter()
        .chain([
            (r"exit code: 1", "exit status: 1"),
            (r"/.*/src", "/[TMP]/src"),
        ])
        .collect::<Vec<_>>();

    uv_snapshot!(filters, context.sync().arg("--group").arg("wsgi"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
      × Failed to build `wsgiref==0.1.2`
      ├─▶ The build backend returned an error
      ╰─▶ Call to `setuptools.build_meta:__legacy__.build_wheel` failed (exit status: 1)

          [stderr]
          Traceback (most recent call last):
            File "<string>", line 14, in <module>
            File "[CACHE_DIR]/builds-v0/[TMP]/build_meta.py", line 325, in get_requires_for_build_wheel
              return self._get_build_requires(config_settings, requirements=['wheel'])
                     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
            File "[CACHE_DIR]/builds-v0/[TMP]/build_meta.py", line 295, in _get_build_requires
              self.run_setup()
            File "[CACHE_DIR]/builds-v0/[TMP]/build_meta.py", line 487, in run_setup
              super().run_setup(setup_script=setup_script)
            File "[CACHE_DIR]/builds-v0/[TMP]/build_meta.py", line 311, in run_setup
              exec(code, locals())
            File "<string>", line 5, in <module>
            File "[CACHE_DIR]/[TMP]/src/ez_setup/__init__.py", line 170
              print "Setuptools version",version,"or greater has been installed."
              ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
          SyntaxError: Missing parentheses in call to 'print'. Did you mean print(...)?

          hint: This usually indicates a problem with the package or the build environment.
      help: `wsgiref` (v0.1.2) was included because `project:wsgi` (v0.1.0) depends on `wsgiref`
    "###);

    Ok(())
}

/// See: <https://github.com/astral-sh/uv/issues/9743>
#[test]
#[cfg(all(feature = "slow-tests", feature = "git"))]
fn sync_stale_egg_info() -> Result<()> {
    let context = TestContext::new("3.13");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "foo"
        version = "0.1.0"
        requires-python = ">=3.13"
        dependencies = [
            "member @ git+https://github.com/astral-sh/uv-stale-egg-info-test.git#subdirectory=member",
            "root @ git+https://github.com/astral-sh/uv-stale-egg-info-test.git",
        ]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!(
        {
            filters => context.filters(),
        },
        {
            assert_snapshot!(
                lock, @r###"
            version = 1
            requires-python = ">=3.13"

            [options]
            exclude-newer = "2024-03-25T00:00:00Z"

            [[package]]
            name = "foo"
            version = "0.1.0"
            source = { virtual = "." }
            dependencies = [
                { name = "member" },
                { name = "root" },
            ]

            [package.metadata]
            requires-dist = [
                { name = "member", git = "https://github.com/astral-sh/uv-stale-egg-info-test.git?subdirectory=member" },
                { name = "root", git = "https://github.com/astral-sh/uv-stale-egg-info-test.git" },
            ]

            [[package]]
            name = "member"
            version = "0.1.dev5+gfea1041"
            source = { git = "https://github.com/astral-sh/uv-stale-egg-info-test.git?subdirectory=member#fea10416b9c479ac88fb217e14e40249b63bfbee" }
            dependencies = [
                { name = "setuptools" },
            ]

            [[package]]
            name = "root"
            version = "0.1.dev5+gfea1041"
            source = { git = "https://github.com/astral-sh/uv-stale-egg-info-test.git#fea10416b9c479ac88fb217e14e40249b63bfbee" }
            dependencies = [
                { name = "member" },
            ]

            [[package]]
            name = "setuptools"
            version = "69.2.0"
            source = { registry = "https://pypi.org/simple" }
            sdist = { url = "https://files.pythonhosted.org/packages/4d/5b/dc575711b6b8f2f866131a40d053e30e962e633b332acf7cd2c24843d83d/setuptools-69.2.0.tar.gz", hash = "sha256:0ff4183f8f42cd8fa3acea16c45205521a4ef28f73c6391d8a25e92893134f2e", size = 2222950 }
            wheels = [
                { url = "https://files.pythonhosted.org/packages/92/e1/1c8bb3420105e70bdf357d57dd5567202b4ef8d27f810e98bb962d950834/setuptools-69.2.0-py3-none-any.whl", hash = "sha256:c21c49fb1042386df081cb5d86759792ab89efca84cf114889191cd09aacc80c", size = 821485 },
            ]
            "###
            );
        }
    );

    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + member==0.1.dev5+gfea1041 (from git+https://github.com/astral-sh/uv-stale-egg-info-test.git@fea10416b9c479ac88fb217e14e40249b63bfbee#subdirectory=member)
     + root==0.1.dev5+gfea1041 (from git+https://github.com/astral-sh/uv-stale-egg-info-test.git@fea10416b9c479ac88fb217e14e40249b63bfbee)
     + setuptools==69.2.0
    "###);

    Ok(())
}

/// See: <https://github.com/astral-sh/uv/issues/8887>
#[test]
#[cfg(feature = "git")]
fn sync_git_repeated_member_static_metadata() -> Result<()> {
    let context = TestContext::new("3.13");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "foo"
        version = "0.1.0"
        requires-python = ">=3.13"
        dependencies = ["uv-git-workspace-in-root", "workspace-member-in-subdir"]

        [tool.uv.sources]
        uv-git-workspace-in-root = { git = "https://github.com/astral-sh/workspace-in-root-test.git" }
        workspace-member-in-subdir = { git = "https://github.com/astral-sh/workspace-in-root-test.git", subdirectory = "workspace-member-in-subdir" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!(
        {
            filters => context.filters(),
        },
        {
            assert_snapshot!(
                lock, @r###"
            version = 1
            requires-python = ">=3.13"

            [options]
            exclude-newer = "2024-03-25T00:00:00Z"

            [[package]]
            name = "foo"
            version = "0.1.0"
            source = { virtual = "." }
            dependencies = [
                { name = "uv-git-workspace-in-root" },
                { name = "workspace-member-in-subdir" },
            ]

            [package.metadata]
            requires-dist = [
                { name = "uv-git-workspace-in-root", git = "https://github.com/astral-sh/workspace-in-root-test.git" },
                { name = "workspace-member-in-subdir", git = "https://github.com/astral-sh/workspace-in-root-test.git?subdirectory=workspace-member-in-subdir" },
            ]

            [[package]]
            name = "uv-git-workspace-in-root"
            version = "0.1.0"
            source = { git = "https://github.com/astral-sh/workspace-in-root-test.git#d3ab48d2338296d47e28dbb2fb327c5e2ac4ac68" }

            [[package]]
            name = "workspace-member-in-subdir"
            version = "0.1.0"
            source = { git = "https://github.com/astral-sh/workspace-in-root-test.git?subdirectory=workspace-member-in-subdir#d3ab48d2338296d47e28dbb2fb327c5e2ac4ac68" }
            dependencies = [
                { name = "uv-git-workspace-in-root" },
            ]
            "###
            );
        }
    );

    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + uv-git-workspace-in-root==0.1.0 (from git+https://github.com/astral-sh/workspace-in-root-test.git@d3ab48d2338296d47e28dbb2fb327c5e2ac4ac68)
     + workspace-member-in-subdir==0.1.0 (from git+https://github.com/astral-sh/workspace-in-root-test.git@d3ab48d2338296d47e28dbb2fb327c5e2ac4ac68#subdirectory=workspace-member-in-subdir)
    "###);

    Ok(())
}

/// See: <https://github.com/astral-sh/uv/issues/8887>
#[test]
#[cfg(feature = "git")]
fn sync_git_repeated_member_dynamic_metadata() -> Result<()> {
    let context = TestContext::new("3.13");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "foo"
        version = "0.1.0"
        requires-python = ">=3.13"
        dependencies = ["package", "dependency"]

        [tool.uv.sources]
        package = { git = "https://git@github.com/astral-sh/uv-dynamic-metadata-test.git" }
        dependency = { git = "https://git@github.com/astral-sh/uv-dynamic-metadata-test.git", subdirectory = "dependency" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: Missing version constraint (e.g., a lower bound) for `typing-extensions`
    Resolved 5 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!(
        {
            filters => context.filters(),
        },
        {
            assert_snapshot!(
                lock, @r###"
            version = 1
            requires-python = ">=3.13"

            [options]
            exclude-newer = "2024-03-25T00:00:00Z"

            [[package]]
            name = "dependency"
            version = "0.1.0"
            source = { git = "https://github.com/astral-sh/uv-dynamic-metadata-test.git?subdirectory=dependency#6c5aa0a65db737c9e7e2e60dc865bd8087012e64" }
            dependencies = [
                { name = "iniconfig" },
            ]

            [[package]]
            name = "foo"
            version = "0.1.0"
            source = { virtual = "." }
            dependencies = [
                { name = "dependency" },
                { name = "package" },
            ]

            [package.metadata]
            requires-dist = [
                { name = "dependency", git = "https://github.com/astral-sh/uv-dynamic-metadata-test.git?subdirectory=dependency" },
                { name = "package", git = "https://github.com/astral-sh/uv-dynamic-metadata-test.git" },
            ]

            [[package]]
            name = "iniconfig"
            version = "2.0.0"
            source = { registry = "https://pypi.org/simple" }
            sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
            wheels = [
                { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
            ]

            [[package]]
            name = "package"
            version = "0.1.0"
            source = { git = "https://github.com/astral-sh/uv-dynamic-metadata-test.git#6c5aa0a65db737c9e7e2e60dc865bd8087012e64" }
            dependencies = [
                { name = "dependency" },
                { name = "typing-extensions" },
            ]

            [[package]]
            name = "typing-extensions"
            version = "4.10.0"
            source = { registry = "https://pypi.org/simple" }
            sdist = { url = "https://files.pythonhosted.org/packages/16/3a/0d26ce356c7465a19c9ea8814b960f8a36c3b0d07c323176620b7b483e44/typing_extensions-4.10.0.tar.gz", hash = "sha256:b0abd7c89e8fb96f98db18d86106ff1d90ab692004eb746cf6eda2682f91b3cb", size = 77558 }
            wheels = [
                { url = "https://files.pythonhosted.org/packages/f9/de/dc04a3ea60b22624b51c703a84bbe0184abcd1d0b9bc8074b5d6b7ab90bb/typing_extensions-4.10.0-py3-none-any.whl", hash = "sha256:69b1a937c3a517342112fb4c6df7e72fc39a38e7891a5730ed4985b5214b5475", size = 33926 },
            ]
            "###
            );
        }
    );

    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + dependency==0.1.0 (from git+https://github.com/astral-sh/uv-dynamic-metadata-test.git@6c5aa0a65db737c9e7e2e60dc865bd8087012e64#subdirectory=dependency)
     + iniconfig==2.0.0
     + package==0.1.0 (from git+https://github.com/astral-sh/uv-dynamic-metadata-test.git@6c5aa0a65db737c9e7e2e60dc865bd8087012e64)
     + typing-extensions==4.10.0
    "###);

    Ok(())
}

/// See: <https://github.com/astral-sh/uv/issues/8887>
#[test]
#[cfg(feature = "git")]
fn sync_git_repeated_member_backwards_path() -> Result<()> {
    let context = TestContext::new("3.13");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "foo"
        version = "0.1.0"
        requires-python = ">=3.13"
        dependencies = ["package", "dependency"]

        [tool.uv.sources]
        package = { git = "https://github.com/astral-sh/uv-backwards-path-test", subdirectory = "root" }
        dependency = { git = "https://github.com/astral-sh/uv-backwards-path-test", subdirectory = "dependency" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!(
        {
            filters => context.filters(),
        },
        {
            assert_snapshot!(
                lock, @r###"
            version = 1
            requires-python = ">=3.13"

            [options]
            exclude-newer = "2024-03-25T00:00:00Z"

            [[package]]
            name = "dependency"
            version = "0.1.0"
            source = { git = "https://github.com/astral-sh/uv-backwards-path-test?subdirectory=dependency#4bcc7fcd2e548c2ab7ba6b97b1c4e3ababccc7a9" }

            [[package]]
            name = "foo"
            version = "0.1.0"
            source = { virtual = "." }
            dependencies = [
                { name = "dependency" },
                { name = "package" },
            ]

            [package.metadata]
            requires-dist = [
                { name = "dependency", git = "https://github.com/astral-sh/uv-backwards-path-test?subdirectory=dependency" },
                { name = "package", git = "https://github.com/astral-sh/uv-backwards-path-test?subdirectory=root" },
            ]

            [[package]]
            name = "package"
            version = "0.1.0"
            source = { git = "https://github.com/astral-sh/uv-backwards-path-test?subdirectory=root#4bcc7fcd2e548c2ab7ba6b97b1c4e3ababccc7a9" }
            dependencies = [
                { name = "dependency" },
            ]
            "###
            );
        }
    );

    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + dependency==0.1.0 (from git+https://github.com/astral-sh/uv-backwards-path-test@4bcc7fcd2e548c2ab7ba6b97b1c4e3ababccc7a9#subdirectory=dependency)
     + package==0.1.0 (from git+https://github.com/astral-sh/uv-backwards-path-test@4bcc7fcd2e548c2ab7ba6b97b1c4e3ababccc7a9#subdirectory=root)
    "###);

    Ok(())
}

/// The project itself is marked as an editable dependency, but under the wrong name. The project
/// is a package.
#[test]
fn mismatched_name_self_editable() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["foo"]

        [tool.uv.sources]
        foo = { path = ".", editable = true }
        "#,
    )?;

    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
      × Failed to build `foo @ file://[TEMP_DIR]/`
      ╰─▶ Package metadata name `project` does not match given name `foo`
      help: `foo` was included because `project` (v0.1.0) depends on `foo`
    "###);

    Ok(())
}

/// A wheel is available in the cache, but was requested under the wrong name.
#[test]
fn mismatched_name_cached_wheel() -> Result<()> {
    let context = TestContext::new("3.12");

    // Cache the `iniconfig` wheel.
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig @ https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0 (from https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz)
    "###);

    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["foo @ https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to download and build `foo @ https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz`
      ╰─▶ Package metadata name `iniconfig` does not match given name `foo`
    "###);

    Ok(())
}

/// Sync a Git repository that depends on a package within the same repository via a `path` source.
///
/// See: <https://github.com/astral-sh/uv/issues/9516>
#[test]
#[cfg(feature = "git")]
fn sync_git_path_dependency() -> Result<()> {
    let context = TestContext::new("3.13");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "foo"
        version = "0.1.0"
        requires-python = ">=3.13"
        dependencies = ["package2"]

        [tool.uv.sources]
        package2 = { git = "https://git@github.com/astral-sh/uv-path-dependency-test.git", subdirectory = "package2" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!(
        {
            filters => context.filters(),
        },
        {
            assert_snapshot!(
                lock, @r###"
            version = 1
            requires-python = ">=3.13"

            [options]
            exclude-newer = "2024-03-25T00:00:00Z"

            [[package]]
            name = "foo"
            version = "0.1.0"
            source = { virtual = "." }
            dependencies = [
                { name = "package2" },
            ]

            [package.metadata]
            requires-dist = [{ name = "package2", git = "https://github.com/astral-sh/uv-path-dependency-test.git?subdirectory=package2" }]

            [[package]]
            name = "package1"
            version = "0.1.0"
            source = { git = "https://github.com/astral-sh/uv-path-dependency-test.git?subdirectory=package1#28781b32cf1f260cdb2c8040628079eb265202bd" }

            [[package]]
            name = "package2"
            version = "0.1.0"
            source = { git = "https://github.com/astral-sh/uv-path-dependency-test.git?subdirectory=package2#28781b32cf1f260cdb2c8040628079eb265202bd" }
            dependencies = [
                { name = "package1" },
            ]
            "###
            );
        }
    );

    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + package1==0.1.0 (from git+https://github.com/astral-sh/uv-path-dependency-test.git@28781b32cf1f260cdb2c8040628079eb265202bd#subdirectory=package1)
     + package2==0.1.0 (from git+https://github.com/astral-sh/uv-path-dependency-test.git@28781b32cf1f260cdb2c8040628079eb265202bd#subdirectory=package2)
    "###);

    Ok(())
}

/// Sync a package with multiple wheels at the same version, differing only in the build tag. We
/// should choose the wheel with the highest build tag.
#[test]
fn sync_build_tag() -> Result<()> {
    let context = TestContext::new("3.12");

    // Populate the `--find-links` entries.
    fs_err::create_dir_all(context.temp_dir.join("links"))?;

    for entry in fs_err::read_dir(context.workspace_root.join("scripts/links"))? {
        let entry = entry?;
        let path = entry.path();
        if path
            .file_name()
            .and_then(|file_name| file_name.to_str())
            .is_some_and(|file_name| file_name.starts_with("build_tag-"))
        {
            let dest = context
                .temp_dir
                .join("links")
                .join(path.file_name().unwrap());
            fs_err::copy(&path, &dest)?;
        }
    }

    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(&formatdoc! { r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["build-tag"]

        [tool.uv]
        find-links = ["{}"]
        "#,
            context.temp_dir.join("links/").portable_display(),
        })?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.child("uv.lock")).unwrap();

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "build-tag"
        version = "1.0.0"
        source = { registry = "links" }
        wheels = [
            { path = "build_tag-1.0.0-1-py2.py3-none-any.whl" },
            { path = "build_tag-1.0.0-3-py2.py3-none-any.whl" },
            { path = "build_tag-1.0.0-5-py2.py3-none-any.whl" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "build-tag" },
        ]

        [package.metadata]
        requires-dist = [{ name = "build-tag" }]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + build-tag==1.0.0
    "###);

    // Ensure that we choose the highest build tag (5).
    uv_snapshot!(context.filters(), context.run().arg("--no-sync").arg("python").arg("-c").arg("import build_tag; build_tag.main()"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    5

    ----- stderr -----
    "###);

    Ok(())
}

#[test]
fn url_hash_mismatch() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]

        [tool.uv.sources]
        iniconfig = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz" }
        "#,
    )?;

    // Write a lockfile with an invalid hash.
    context.temp_dir.child("uv.lock").write_str(indoc! {r#"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz" }
        sdist = { hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b4" }

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [{ name = "iniconfig", url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz" }]
    "#})?;

    // Running `uv sync` should fail.
    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
      × Failed to download and build `iniconfig @ https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz`
      ╰─▶ Hash mismatch for `iniconfig @ https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz`

          Expected:
            sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b4

          Computed:
            sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3
      help: `iniconfig` was included because `project` (v0.1.0) depends on `iniconfig`
    "###);

    Ok(())
}

#[test]
fn path_hash_mismatch() -> Result<()> {
    let context = TestContext::new("3.12");

    // Download the source.
    let archive = context.temp_dir.child("iniconfig-2.0.0.tar.gz");
    download_to_disk(
        "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz",
        &archive,
    );

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]

        [tool.uv.sources]
        iniconfig = { path = "iniconfig-2.0.0.tar.gz" }
        "#,
    )?;

    // Write a lockfile with an invalid hash.
    context.temp_dir.child("uv.lock").write_str(indoc! {r#"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { path = "iniconfig-2.0.0.tar.gz" }
        sdist = { hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b4" }

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [{ name = "iniconfig", path = "iniconfig-2.0.0.tar.gz" }]
    "#})?;

    // Running `uv sync` should fail.
    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
      × Failed to build `iniconfig @ file://[TEMP_DIR]/iniconfig-2.0.0.tar.gz`
      ╰─▶ Hash mismatch for `iniconfig @ file://[TEMP_DIR]/iniconfig-2.0.0.tar.gz`

          Expected:
            sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b4

          Computed:
            sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3
      help: `iniconfig` was included because `project` (v0.1.0) depends on `iniconfig`
    "###);

    Ok(())
}

#[test]
fn find_links_relative_in_config_works_from_subdir() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "subdir_test"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["ok==1.0.0"]

        [tool.uv]
        find-links = ["packages/"]
    "#})?;

    // Create packages/ subdirectory and copy our "offline" tqdm wheel there
    let packages = context.temp_dir.child("packages");
    packages.create_dir_all()?;

    let wheel_src = context
        .workspace_root
        .join("scripts/links/ok-1.0.0-py3-none-any.whl");
    let wheel_dst = packages.child("ok-1.0.0-py3-none-any.whl");
    fs_err::copy(&wheel_src, &wheel_dst)?;

    // Create a separate subdir, which will become our working directory
    let subdir = context.temp_dir.child("subdir");
    subdir.create_dir_all()?;

    // Run `uv sync --offline` from subdir. We expect it to find the local wheel in ../packages/.
    uv_snapshot!(context.filters(), context.sync().current_dir(&subdir).arg("--offline"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + ok==1.0.0
    "###);

    Ok(())
}
