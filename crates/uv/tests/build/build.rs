use std::collections::BTreeMap;

use anyhow::{Result, anyhow};
use assert_cmd::assert::OutputAssertExt;
use assert_fs::prelude::*;
use async_zip::base::read::mem::ZipFileReader;
use futures::executor::block_on;
use indoc::{formatdoc, indoc};
use insta::assert_snapshot;
use predicates::prelude::predicate;
use rustc_hash::FxHashSet;
use sha2::{Digest, Sha256};
use std::env::current_dir;
use std::path::Path;
use std::process::Command;
use tokio::io::AsyncWriteExt;
use url::Url;
use uv_static::EnvVars;
use uv_test::package_server::PackageServer;
use uv_test::packse::generate_wheel;
use uv_test::{DEFAULT_PYTHON_VERSION, TestContext, apply_filters, get_bin, uv_snapshot};

fn zip_file_names(path: &Path) -> Result<Vec<String>> {
    block_on(async {
        let wheel = ZipFileReader::new(fs_err::read(path)?).await?;
        let mut files: Vec<_> = wheel
            .file()
            .entries()
            .iter()
            .map(|entry| Ok(entry.filename().as_str()?.to_string()))
            .collect::<async_zip::error::Result<_>>()?;
        files.sort();
        Ok(files)
    })
}

#[test]
fn build_basic() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_filter((r"\\\.", ""));

    let project = context.temp_dir.child("project");

    let pyproject_toml = project.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0"]

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
        "#,
    )?;

    project
        .child("src")
        .child("project")
        .child("__init__.py")
        .touch()?;
    project.child("README").touch()?;

    // Build the specified path.
    uv_snapshot!(context.filters(), context.build().arg("project"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Building source distribution...
    Building wheel from source distribution...
    Successfully built project/dist/project-0.1.0.tar.gz
    Successfully built project/dist/project-0.1.0-py3-none-any.whl
    ");

    project
        .child("dist")
        .child("project-0.1.0.tar.gz")
        .assert(predicate::path::is_file());
    project
        .child("dist")
        .child("project-0.1.0-py3-none-any.whl")
        .assert(predicate::path::is_file());

    fs_err::remove_dir_all(project.child("dist"))?;

    // Build the current working directory.
    uv_snapshot!(context.filters(), context.build().current_dir(project.path()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Building source distribution...
    Building wheel from source distribution...
    Successfully built dist/project-0.1.0.tar.gz
    Successfully built dist/project-0.1.0-py3-none-any.whl
    ");

    project
        .child("dist")
        .child("project-0.1.0.tar.gz")
        .assert(predicate::path::is_file());
    project
        .child("dist")
        .child("project-0.1.0-py3-none-any.whl")
        .assert(predicate::path::is_file());

    fs_err::remove_dir_all(project.child("dist"))?;

    // Error if there's nothing to build.
    uv_snapshot!(context.filters(), context.build(), @"
    exit_code: 2 (failure)
    ----- stderr -----
    Building source distribution...
    error: Failed to build `[TEMP_DIR]/`
      cause: `[TEMP_DIR]/` does not appear to be a Python project, as neither `pyproject.toml` nor `setup.py` are present in the directory
    ");

    // Build to a specified path, even if builds are disabled for the project by name.
    uv_snapshot!(context.filters(), context.build().arg("--out-dir").arg("out").arg("--no-build-package").arg("project").current_dir(project.path()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Building source distribution...
    Building wheel from source distribution...
    Successfully built out/project-0.1.0.tar.gz
    Successfully built out/project-0.1.0-py3-none-any.whl
    ");

    project
        .child("out")
        .child("project-0.1.0.tar.gz")
        .assert(predicate::path::is_file());
    project
        .child("out")
        .child("project-0.1.0-py3-none-any.whl")
        .assert(predicate::path::is_file());

    // A global build restriction still allows explicitly building the project and its sdist.
    project.child("uv.toml").write_str("no-build = true\n")?;

    uv_snapshot!(context.filters(), context.build().current_dir(project.path()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Building source distribution...
    Building wheel from source distribution...
    Successfully built dist/project-0.1.0.tar.gz
    Successfully built dist/project-0.1.0-py3-none-any.whl
    ");

    project
        .child("dist")
        .child("project-0.1.0.tar.gz")
        .assert(predicate::path::is_file());
    project
        .child("dist")
        .child("project-0.1.0-py3-none-any.whl")
        .assert(predicate::path::is_file());

    Ok(())
}

/// Global lazy imports are opt-in on supported build interpreters.
#[test]
fn build_lazy_imports() -> Result<()> {
    let context = uv_test::test_context!("3.15");
    let project = context.temp_dir.child("project");
    project.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"

        [build-system]
        requires = []
        build-backend = "backend"
        backend-path = ["."]
    "#})?;
    project.child("unused_module.py").write_str("VALUE = 1\n")?;
    project.child("backend.py").write_str(indoc! {r#"
        import sys
        import unused_module

        def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
            import pathlib
            import zipfile

            pathlib.Path("mode").write_text(
                "eager" if "unused_module" in sys.modules else "lazy"
            )
            name = "project-0.1.0-py3-none-any.whl"
            with zipfile.ZipFile(pathlib.Path(wheel_directory) / name, "w") as wheel:
                wheel.writestr("project-0.1.0.dist-info/METADATA", "Metadata-Version: 2.3\nName: project\nVersion: 0.1.0\n")
                wheel.writestr("project-0.1.0.dist-info/WHEEL", "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n")
                wheel.writestr("project-0.1.0.dist-info/RECORD", "")
            return name
    "#})?;

    uv_snapshot!(context.filters(), context.build().arg("--wheel").arg("project").env("PYTHON_LAZY_IMPORTS", "normal"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Building wheel...
    Successfully built project/dist/project-0.1.0-py3-none-any.whl
    ");
    project.child("mode").assert("eager");

    uv_snapshot!(context.filters(), context.build().arg("--wheel").arg("project").arg("--preview-features").arg("build-lazy-imports").env("PYTHON_LAZY_IMPORTS", "normal"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Building wheel...
    Successfully built project/dist/project-0.1.0-py3-none-any.whl
    ");
    project.child("mode").assert("lazy");

    context.temp_dir.child("uv.toml").write_str(indoc! {r#"
        preview-features = ["build-lazy-imports"]
    "#})?;
    uv_snapshot!(context.filters(), context.build().arg("--wheel").arg("project").env("PYTHON_LAZY_IMPORTS", "normal"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Building wheel...
    Successfully built project/dist/project-0.1.0-py3-none-any.whl
    ");
    project.child("mode").assert("lazy");

    uv_snapshot!(context.filters(), context.build().arg("--wheel").arg("project").arg("--no-preview").env("PYTHON_LAZY_IMPORTS", "normal"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Building wheel...
    Successfully built project/dist/project-0.1.0-py3-none-any.whl
    ");
    project.child("mode").assert("eager");

    Ok(())
}

/// Global lazy imports remain disabled on unsupported build interpreters.
#[test]
fn build_lazy_imports_unsupported_python() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let project = context.temp_dir.child("project");
    project.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"

        [build-system]
        requires = []
        build-backend = "backend"
        backend-path = ["."]
    "#})?;
    project.child("unused_module.py").write_str("VALUE = 1\n")?;
    project.child("backend.py").write_str(indoc! {r#"
        import sys
        import unused_module

        def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
            import pathlib
            import zipfile

            pathlib.Path("mode").write_text(
                "eager" if "unused_module" in sys.modules else "lazy"
            )
            name = "project-0.1.0-py3-none-any.whl"
            with zipfile.ZipFile(pathlib.Path(wheel_directory) / name, "w") as wheel:
                wheel.writestr("project-0.1.0.dist-info/METADATA", "Metadata-Version: 2.3\nName: project\nVersion: 0.1.0\n")
                wheel.writestr("project-0.1.0.dist-info/WHEEL", "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n")
                wheel.writestr("project-0.1.0.dist-info/RECORD", "")
            return name
    "#})?;

    uv_snapshot!(context.filters(), context.build().arg("--wheel").arg("project").env("PYTHON_LAZY_IMPORTS", "normal"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Building wheel...
    Successfully built project/dist/project-0.1.0-py3-none-any.whl
    ");
    project.child("mode").assert("eager");

    uv_snapshot!(context.filters(), context.build().arg("--wheel").arg("project").arg("--preview-features").arg("build-lazy-imports").env("PYTHON_LAZY_IMPORTS", "normal"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Building wheel...
    Successfully built project/dist/project-0.1.0-py3-none-any.whl
    ");
    project.child("mode").assert("eager");

    context.temp_dir.child("uv.toml").write_str(indoc! {r#"
        preview-features = ["build-lazy-imports"]
    "#})?;
    uv_snapshot!(context.filters(), context.build().arg("--wheel").arg("project").env("PYTHON_LAZY_IMPORTS", "normal"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Building wheel...
    Successfully built project/dist/project-0.1.0-py3-none-any.whl
    ");
    project.child("mode").assert("eager");

    uv_snapshot!(context.filters(), context.build().arg("--wheel").arg("project").arg("--no-preview").env("PYTHON_LAZY_IMPORTS", "normal"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Building wheel...
    Successfully built project/dist/project-0.1.0-py3-none-any.whl
    ");
    project.child("mode").assert("eager");

    Ok(())
}

/// Build hooks can invoke uv while building a wheel from an extracted source distribution.
/// Regression test for <https://github.com/astral-sh/uv/issues/19878>.
#[test]
fn build_hook_invokes_uv() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let project = context.temp_dir.child("project");

    project.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"

        [tool.hatch.build.hooks.custom]
        path = "hatch_build.py"
    "#})?;
    project.child("src/project/__init__.py").touch()?;
    project.child("hatch_build.py").write_str(&formatdoc! {r#"
        import subprocess

        from hatchling.builders.hooks.plugin.interface import BuildHookInterface


        class CustomBuildHook(BuildHookInterface):
            def initialize(self, version, build_data):
                subprocess.run(
                    [
                        {uv:?},
                        "export",
                        "--quiet",
                        "--no-dev",
                        "--no-editable",
                        "--no-emit-project",
                        "--output-file",
                        "requirements.txt",
                    ],
                    check=True,
                    cwd=self.root,
                )
    "#, uv = get_bin!().display() })?;

    uv_snapshot!(context.filters(), context.build().current_dir(&project), @"
    exit_code: 0 (success)
    ----- stderr -----
    Building source distribution...
    Building wheel from source distribution...
    Successfully built dist/project-0.1.0.tar.gz
    Successfully built dist/project-0.1.0-py3-none-any.whl
    ");

    Ok(())
}

/// A source distribution must include an in-tree build backend referenced by `backend-path`.
/// Regression test for <https://github.com/astral-sh/uv/issues/19771>.
#[test]
fn build_sdist_missing_backend_path() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let project = context.temp_dir.child("project");

    project.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        description = "A test project"

        [build-system]
        requires = ["flit_core >=3.4,<4"]
        build-backend = "my_backend:builder"
        backend-path = ["backend_dir"]
    "#})?;
    project.child("src/project/__init__.py").touch()?;
    project
        .child("backend_dir/my_backend.py")
        .write_str("from flit_core import buildapi\n\nbuilder = buildapi\n")?;

    uv_snapshot!(context.filters(), context.build().arg(project.path()), @r"
    exit_code: 2 (failure)
    ----- stderr -----
    Building source distribution...
    Building wheel from source distribution...
    error: Failed to build `[TEMP_DIR]/project`
      cause: `backend-path` entry `backend_dir` does not exist or is not a directory
    ");

    Ok(())
}

/// An in-tree build backend must not be able to escape the source tree via `backend-path`.
#[test]
fn build_backend_path_outside_source_tree() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let project = context.temp_dir.child("project");

    project.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = []
        build-backend = "backend"
        backend-path = ["../backend"]
    "#})?;
    context
        .temp_dir
        .child("backend/backend.py")
        .write_str("raise RuntimeError('outside backend was executed')\n")?;

    uv_snapshot!(context.filters(), context.build().arg("--wheel").arg(project.path()), @"
    exit_code: 2 (failure)
    ----- stderr -----
    Building wheel...
    error: Failed to build `[TEMP_DIR]/project`
      cause: `backend-path` entry `../backend` must be a relative path within the source tree
    ");

    Ok(())
}

/// PEP 517 requires `backend-path` entries to be relative, even when they point inside the tree.
#[cfg(unix)]
#[test]
fn build_backend_path_absolute_inside_source_tree() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let project = context.temp_dir.child("project");
    let backend = project.child("backend");

    project.child("pyproject.toml").write_str(&formatdoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = []
        build-backend = "backend"
        backend-path = ["{}"]
    "#, backend.path().display()})?;
    backend
        .child("backend.py")
        .write_str("raise RuntimeError('absolute backend was executed')\n")?;

    uv_snapshot!(context.filters(), context.build().arg("--wheel").arg(project.path()), @"
    exit_code: 2 (failure)
    ----- stderr -----
    Building wheel...
    error: Failed to build `[TEMP_DIR]/project`
      cause: `backend-path` entry `[TEMP_DIR]/project/backend` must be a relative path within the source tree
    ");

    Ok(())
}

/// Resolving an in-tree backend path must not follow a symlink outside the source tree.
#[cfg(unix)]
#[test]
fn build_backend_path_symlink_outside_source_tree() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let project = context.temp_dir.child("project");

    project.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = []
        build-backend = "backend"
        backend-path = ["backend"]
    "#})?;
    context
        .temp_dir
        .child("backend/backend.py")
        .write_str("raise RuntimeError('outside backend was executed')\n")?;
    fs_err::os::unix::fs::symlink(context.temp_dir.child("backend"), project.child("backend"))?;

    uv_snapshot!(context.filters(), context.build().arg("--wheel").arg(project.path()), @"
    exit_code: 2 (failure)
    ----- stderr -----
    Building wheel...
    error: Failed to build `[TEMP_DIR]/project`
      cause: `backend-path` entry `backend` must be a relative path within the source tree
    ");

    Ok(())
}

#[test]
fn build_sdist() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_filter((r"\\\.", ""));

    let project = context.temp_dir.child("project");

    let pyproject_toml = project.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0"]

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
        "#,
    )?;

    project
        .child("src")
        .child("project")
        .child("__init__.py")
        .touch()?;
    project.child("README").touch()?;

    // Build the specified path.
    uv_snapshot!(context.filters(), context.build().arg("--sdist").current_dir(&project), @"
    exit_code: 0 (success)
    ----- stderr -----
    Building source distribution...
    Successfully built dist/project-0.1.0.tar.gz
    ");

    project
        .child("dist")
        .child("project-0.1.0.tar.gz")
        .assert(predicate::path::is_file());
    project
        .child("dist")
        .child("project-0.1.0-py3-none-any.whl")
        .assert(predicate::path::missing());

    Ok(())
}

#[test]
fn build_wheel() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_filter((r"\\\.", ""));

    let project = context.temp_dir.child("project");

    let pyproject_toml = project.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0"]

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
        "#,
    )?;

    project
        .child("src")
        .child("project")
        .child("__init__.py")
        .touch()?;
    project.child("README").touch()?;

    // Explicit wheel builds are allowed even when dependency builds are disabled.
    uv_snapshot!(context.filters(), context.build().arg("--wheel").arg("--no-build").current_dir(&project), @"
    exit_code: 0 (success)
    ----- stderr -----
    Building wheel...
    Successfully built dist/project-0.1.0-py3-none-any.whl
    ");

    project
        .child("dist")
        .child("project-0.1.0.tar.gz")
        .assert(predicate::path::missing());
    project
        .child("dist")
        .child("project-0.1.0-py3-none-any.whl")
        .assert(predicate::path::is_file());

    Ok(())
}

#[test]
fn build_sdist_wheel() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_filter((r"\\\.", ""));

    let project = context.temp_dir.child("project");

    let pyproject_toml = project.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0"]

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
        "#,
    )?;

    project
        .child("src")
        .child("project")
        .child("__init__.py")
        .touch()?;
    project.child("README").touch()?;

    // Build the specified path.
    uv_snapshot!(context.filters(), context.build().arg("--sdist").arg("--wheel").current_dir(&project), @"
    exit_code: 0 (success)
    ----- stderr -----
    Building source distribution...
    Building wheel...
    Successfully built dist/project-0.1.0.tar.gz
    Successfully built dist/project-0.1.0-py3-none-any.whl
    ");

    project
        .child("dist")
        .child("project-0.1.0.tar.gz")
        .assert(predicate::path::is_file());
    project
        .child("dist")
        .child("project-0.1.0-py3-none-any.whl")
        .assert(predicate::path::is_file());

    Ok(())
}

#[test]
fn build_wheel_from_sdist() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_filter((r"\\\.", ""));

    let project = context.temp_dir.child("project");

    let pyproject_toml = project.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0"]

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
        "#,
    )?;

    project
        .child("src")
        .child("project")
        .child("__init__.py")
        .touch()?;
    project.child("README").touch()?;

    // Build the sdist.
    uv_snapshot!(context.filters(), context.build().arg("--sdist").current_dir(&project), @"
    exit_code: 0 (success)
    ----- stderr -----
    Building source distribution...
    Successfully built dist/project-0.1.0.tar.gz
    ");

    project
        .child("dist")
        .child("project-0.1.0.tar.gz")
        .assert(predicate::path::is_file());
    project
        .child("dist")
        .child("project-0.1.0-py3-none-any.whl")
        .assert(predicate::path::missing());

    // Error if `--wheel` is not specified.
    uv_snapshot!(context.filters(), context.build().arg("./dist/project-0.1.0.tar.gz").current_dir(&project), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Failed to build `[TEMP_DIR]/project/dist/project-0.1.0.tar.gz`
      cause: Pass `--wheel` explicitly to build a wheel from a source distribution
    ");

    // Error if `--sdist` is specified.
    uv_snapshot!(context.filters(), context.build().arg("./dist/project-0.1.0.tar.gz").arg("--sdist").current_dir(&project), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Failed to build `[TEMP_DIR]/project/dist/project-0.1.0.tar.gz`
      cause: Building an `--sdist` from a source distribution is not supported
    ");

    // Explicit wheel builds from an sdist are allowed even when dependency builds are disabled.
    uv_snapshot!(context.filters(), context.build().arg("./dist/project-0.1.0.tar.gz").arg("--wheel").arg("--no-build").current_dir(&project), @"
    exit_code: 0 (success)
    ----- stderr -----
    Building wheel from source distribution...
    Successfully built dist/project-0.1.0-py3-none-any.whl
    ");

    project
        .child("dist")
        .child("project-0.1.0.tar.gz")
        .assert(predicate::path::is_file());
    project
        .child("dist")
        .child("project-0.1.0-py3-none-any.whl")
        .assert(predicate::path::is_file());

    // Passing a wheel is an error.
    uv_snapshot!(context.filters(), context.build().arg("./dist/project-0.1.0-py3-none-any.whl").arg("--wheel").current_dir(&project), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Failed to build `[TEMP_DIR]/project/dist/project-0.1.0-py3-none-any.whl`
      cause: `dist/project-0.1.0-py3-none-any.whl` is not a valid build source. Expected to receive a source directory, or a source distribution ending in one of: `.tar.gz`, `.zip`, `.tar.bz2`, `.tar.lz`, `.tar.lzma`, `.tar.xz`, `.tar.zst`, `.tar`, `.tbz`, `.tgz`, `.tlz`, or `.txz`.
    ");

    Ok(())
}

#[test]
fn build_fail() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_filter((r"\\\.", ""));

    let project = context.temp_dir.child("project");

    let pyproject_toml = project.child("pyproject.toml");
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

    project
        .child("src")
        .child("project")
        .child("__init__.py")
        .touch()?;
    project.child("README").touch()?;

    project.child("setup.py").write_str(
        r#"
        from setuptools import setup

        setup(
            name="project",
            version="0.1.0",
            packages=["project"],
            install_requires=["foo==3.7.0"],
        )
        "#,
    )?;

    // Build the specified path.
    uv_snapshot!(context.filters(), context.build().arg("project"), @r#"
    exit_code: 2 (failure)
    ----- stderr -----
    Building source distribution...
    Traceback (most recent call last):
      File "<string>", line 14, in <module>
      File "[CACHE_DIR]/builds-v0/[TMP]/[PYTHON-LIB]/site-packages/setuptools/build_meta.py", line 328, in get_requires_for_build_sdist
        return self._get_build_requires(config_settings, requirements=[])
               ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
      File "[CACHE_DIR]/builds-v0/[TMP]/[PYTHON-LIB]/site-packages/setuptools/build_meta.py", line 295, in _get_build_requires
        self.run_setup()
      File "[CACHE_DIR]/builds-v0/[TMP]/[PYTHON-LIB]/site-packages/setuptools/build_meta.py", line 311, in run_setup
        exec(code, locals())
      File "<string>", line 2
        from setuptools import setup
    IndentationError: unexpected indent
    error: Failed to build `[TEMP_DIR]/project`
      cause: The build backend returned an error
      cause: Call to `setuptools.build_meta.get_requires_for_build_sdist` failed (exit status: 1)

    hint: Build failures usually indicate a problem with the package or the build environment
    "#);

    Ok(())
}

#[test]
fn build_workspace() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filter((r"\\\.", ""))
        .with_filter((r"\[project\]", "[PKG]"))
        .with_filter((r"\[member\]", "[PKG]"));

    let project = context.temp_dir.child("project");

    let pyproject_toml = project.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0"]

        [tool.uv.workspace]
        members = ["packages/*"]

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
        "#,
    )?;

    project
        .child("src")
        .child("project")
        .child("__init__.py")
        .touch()?;
    project.child("README").touch()?;

    let member = project.child("packages").child("member");
    fs_err::create_dir_all(member.path())?;

    member.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "member"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
        "#,
    )?;

    member
        .child("src")
        .child("member")
        .child("__init__.py")
        .touch()?;
    member.child("README").touch()?;

    let r#virtual = project.child("packages").child("virtual");
    fs_err::create_dir_all(r#virtual.path())?;

    r#virtual.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "virtual"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]
        "#,
    )?;

    r#virtual
        .child("src")
        .child("virtual")
        .child("__init__.py")
        .touch()?;
    r#virtual.child("README").touch()?;

    // Build the member.
    uv_snapshot!(context.filters(), context.build().arg("--package").arg("member").current_dir(&project), @"
    exit_code: 0 (success)
    ----- stderr -----
    Building source distribution...
    Building wheel from source distribution...
    Successfully built dist/member-0.1.0.tar.gz
    Successfully built dist/member-0.1.0-py3-none-any.whl
    ");

    project
        .child("dist")
        .child("member-0.1.0.tar.gz")
        .assert(predicate::path::is_file());
    project
        .child("dist")
        .child("member-0.1.0-py3-none-any.whl")
        .assert(predicate::path::is_file());

    // Build all packages.
    uv_snapshot!(context.filters(), context.build().arg("--all").arg("--no-build-logs").current_dir(&project), @"
    exit_code: 0 (success)
    ----- stderr -----
    [PKG] Building source distribution...
    [PKG] Building source distribution...
    [PKG] Building wheel from source distribution...
    [PKG] Building wheel from source distribution...
    Successfully built dist/member-0.1.0.tar.gz
    Successfully built dist/member-0.1.0-py3-none-any.whl
    Successfully built dist/project-0.1.0.tar.gz
    Successfully built dist/project-0.1.0-py3-none-any.whl
    ");

    project
        .child("dist")
        .child("member-0.1.0.tar.gz")
        .assert(predicate::path::is_file());
    project
        .child("dist")
        .child("member-0.1.0-py3-none-any.whl")
        .assert(predicate::path::is_file());
    project
        .child("dist")
        .child("project-0.1.0.tar.gz")
        .assert(predicate::path::is_file());
    project
        .child("dist")
        .child("project-0.1.0-py3-none-any.whl")
        .assert(predicate::path::is_file());

    // If a source is provided, discover the workspace from the source.
    uv_snapshot!(context.filters(), context.build().arg("./project").arg("--package").arg("member"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Building source distribution...
    Building wheel from source distribution...
    Successfully built project/dist/member-0.1.0.tar.gz
    Successfully built project/dist/member-0.1.0-py3-none-any.whl
    ");

    // If a source is provided, discover the workspace from the source.
    uv_snapshot!(context.filters(), context.build().arg("./project").arg("--all").arg("--no-build-logs"), @"
    exit_code: 0 (success)
    ----- stderr -----
    [PKG] Building source distribution...
    [PKG] Building source distribution...
    [PKG] Building wheel from source distribution...
    [PKG] Building wheel from source distribution...
    Successfully built project/dist/member-0.1.0.tar.gz
    Successfully built project/dist/member-0.1.0-py3-none-any.whl
    Successfully built project/dist/project-0.1.0.tar.gz
    Successfully built project/dist/project-0.1.0-py3-none-any.whl
    ");

    // Fail when `--package` is provided without a workspace.
    uv_snapshot!(context.filters(), context.build().arg("--package").arg("member"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: `--package` was provided, but no workspace was found
      cause: No `pyproject.toml` found in current directory or any parent directory
    ");

    // Fail when `--all` is provided without a workspace.
    uv_snapshot!(context.filters(), context.build().arg("--all"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: `--all-packages` was provided, but no workspace was found
      cause: No `pyproject.toml` found in current directory or any parent directory
    ");

    // Fail when `--package` is a non-existent member without a workspace.
    uv_snapshot!(context.filters(), context.build().arg("--package").arg("fail").current_dir(&project), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Package `fail` not found in workspace
    ");

    Ok(())
}

#[test]
fn build_all_with_failure() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filter((r"\\\.", ""))
        .with_filter((r"\[project\]", "[PKG]"))
        .with_filter((r"\[member-\w+\]", "[PKG]"));

    let project = context.temp_dir.child("project");

    let pyproject_toml = project.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0"]

        [tool.uv.workspace]
        members = ["packages/*"]

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
        "#,
    )?;

    project
        .child("src")
        .child("project")
        .child("__init__.py")
        .touch()?;
    project.child("README").touch()?;

    let member_a = project.child("packages").child("member_a");
    fs_err::create_dir_all(member_a.path())?;

    let member_b = project.child("packages").child("member_b");
    fs_err::create_dir_all(member_b.path())?;

    member_a.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "member_a"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
        "#,
    )?;

    member_a
        .child("src")
        .child("member_a")
        .child("__init__.py")
        .touch()?;
    member_a.child("README").touch()?;

    member_b.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "member_b"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    member_b
        .child("src")
        .child("member_b")
        .child("__init__.py")
        .touch()?;
    member_b.child("README").touch()?;

    // member_b build should fail
    member_b.child("setup.py").write_str(
        r#"
        from setuptools import setup

        setup(
            name="project",
            version="0.1.0",
            packages=["project"],
            install_requires=["foo==3.7.0"],
        )
        "#,
    )?;

    // Build all the packages
    uv_snapshot!(context.filters(), context.build().arg("--all").arg("--no-build-logs").current_dir(&project), @"
    exit_code: 2 (failure)
    ----- stderr -----
    [PKG] Building source distribution...
    [PKG] Building source distribution...
    [PKG] Building source distribution...
    [PKG] Building wheel from source distribution...
    [PKG] Building wheel from source distribution...
    Successfully built dist/member_a-0.1.0.tar.gz
    Successfully built dist/member_a-0.1.0-py3-none-any.whl
    error: Failed to build `member-b @ [TEMP_DIR]/project/packages/member_b`
      cause: The build backend returned an error
      cause: Call to `setuptools.build_meta.get_requires_for_build_sdist` failed (exit status: 1)

    hint: Build failures usually indicate a problem with the package or the build environment
    Successfully built dist/project-0.1.0.tar.gz
    Successfully built dist/project-0.1.0-py3-none-any.whl
    ");

    // project and member_a should be built, regardless of member_b build failure
    project
        .child("dist")
        .child("project-0.1.0.tar.gz")
        .assert(predicate::path::is_file());
    project
        .child("dist")
        .child("project-0.1.0-py3-none-any.whl")
        .assert(predicate::path::is_file());

    project
        .child("dist")
        .child("member_a-0.1.0.tar.gz")
        .assert(predicate::path::is_file());
    project
        .child("dist")
        .child("member_a-0.1.0-py3-none-any.whl")
        .assert(predicate::path::is_file());

    Ok(())
}

#[test]
fn build_constraints() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_filter((r"\\\.", ""));

    let project = context.temp_dir.child("project");

    let constraints = project.child("constraints.txt");
    constraints.write_str("hatchling==0.1.0")?;

    let pyproject_toml = project.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0"]

        [build-system]
        requires = ["hatchling>=1.0"]
        build-backend = "hatchling.build"
        "#,
    )?;

    project
        .child("src")
        .child("project")
        .child("__init__.py")
        .touch()?;
    project.child("README").touch()?;

    uv_snapshot!(context.filters(), context.build().arg("--build-constraint").arg("constraints.txt").current_dir(&project), @"
    exit_code: 2 (failure)
    ----- stderr -----
    Building source distribution...
    error: Failed to build `[TEMP_DIR]/project`
      cause: Failed to resolve requirements from `build-system.requires`
      cause: No solution found when resolving: `hatchling>=1.0`
      cause: Because you require hatchling>=1.0 and hatchling==0.1.0, we can conclude that your requirements are unsatisfiable.
    ");

    project
        .child("dist")
        .child("project-0.1.0.tar.gz")
        .assert(predicate::path::missing());
    project
        .child("dist")
        .child("project-0.1.0-py3-none-any.whl")
        .assert(predicate::path::missing());

    Ok(())
}

#[test]
fn build_dependency_check_missing_declared_requirement() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let project = context.temp_dir.child("project");
    project.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["missing-backend>=1"]
        build-backend = "backend"
        backend-path = ["."]
    "#})?;
    project.child("backend.py").write_str(indoc! {r#"
        raise RuntimeError("backend must not be imported")
    "#})?;
    uv_snapshot!(context.filters(), context.build().args([
        "--preview-features", "build-dependency-check", "--no-build-isolation",
    ]).arg("--offline").current_dir(&project), @"
    exit_code: 2 (failure)
    ----- stderr -----
    Building source distribution...
    error: Failed to build `[TEMP_DIR]/project`
      cause: Build requirement is not satisfied: `missing-backend>=1`
    ");
    project
        .child("dist/project-0.1.0-py3-none-any.whl")
        .assert(predicate::path::missing());
    Ok(())
}

#[test]
fn build_dependency_check_dynamic_requirements() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let project = context.temp_dir.child("project");
    project.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["idna>=3.3"]
        build-backend = "backend"
        backend-path = ["."]
    "#})?;
    project.child("backend.py").write_str(indoc! {r#"
        import os
        from pathlib import Path
        import tarfile
        import zipfile

        def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
            filename = "project-0.1.0-py3-none-any.whl"
            with zipfile.ZipFile(Path(wheel_directory) / filename, "w") as wheel:
                wheel.writestr("project-0.1.0.dist-info/METADATA", "Metadata-Version: 2.3\nName: project\nVersion: 0.1.0\n")
                wheel.writestr("project-0.1.0.dist-info/WHEEL", "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n")
                wheel.writestr("project-0.1.0.dist-info/RECORD", "")
            return filename

        def build_sdist(sdist_directory, config_settings=None):
            filename = "project-0.1.0.tar.gz"
            with tarfile.open(Path(sdist_directory) / filename, "w:gz") as sdist:
                for name in ["pyproject.toml", "backend.py"]:
                    sdist.add(name, arcname=f"project-0.1.0/{name}")
            return filename

        def get_requires_for_build_wheel(config_settings):
            assert os.environ["BUILD_CHECK_ENV"] == "preserved"
            assert config_settings == {"dependency": "idna>=3.6"}
            Path("wheel-hook-called").touch()
            return [config_settings["dependency"]]

        def get_requires_for_build_sdist(config_settings):
            Path("sdist-hook-called").touch()
            return []
    "#})?;
    uv_snapshot!(context.filters(), context.pip_install().arg("idna==3.3"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + idna==3.3
    ");
    uv_snapshot!(context.filters(), context.build().args([
        "--preview-features", "build-dependency-check", "--no-build-isolation",
    ]).arg("--wheel").arg("--offline")
        .arg("-Cdependency=idna>=3.6").env("BUILD_CHECK_ENV", "preserved").current_dir(&project), @"
    exit_code: 2 (failure)
    ----- stderr -----
    Building wheel...
    error: Failed to build `[TEMP_DIR]/project`
      cause: Build requirement is not satisfied: `idna>=3.6`
    ");
    project
        .child("wheel-hook-called")
        .assert(predicate::path::exists());
    project
        .child("sdist-hook-called")
        .assert(predicate::path::missing());
    // Build the dependency with settings that differ from the project's build settings.
    uv_snapshot!(context.filters(), context.pip_install().arg("idna==3.6")
        .arg("--no-binary=idna").arg("-Cdependency=installed"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - idna==3.3
     + idna==3.6
    ");
    uv_snapshot!(context.filters(), context.build().args([
        "--preview-features", "build-dependency-check", "--no-build-isolation",
    ]).arg("--offline")
        .arg("-Cdependency=idna>=3.6").env("BUILD_CHECK_ENV", "preserved").current_dir(&project), @"
    exit_code: 0 (success)
    ----- stderr -----
    Building source distribution...
    Building wheel from source distribution...
    Successfully built dist/project-0.1.0.tar.gz
    Successfully built dist/project-0.1.0-py3-none-any.whl
    ");
    project
        .child("sdist-hook-called")
        .assert(predicate::path::exists());
    project
        .child("dist/project-0.1.0-py3-none-any.whl")
        .assert(predicate::path::exists());
    Ok(())
}

#[test]
fn build_dependency_check_constraints_and_extra_build_dependencies() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let project = context.temp_dir.child("project");
    project.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["idna>=3.3"]
        build-backend = "backend"
        backend-path = ["."]
    "#})?;
    project.child("backend.py").write_str(indoc! {r#"
        raise RuntimeError("backend must not be imported")
    "#})?;
    uv_snapshot!(context.filters(), context.pip_install().arg("idna==3.3"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + idna==3.3
    ");
    let constraints = context.temp_dir.child("constraints.txt");
    constraints.write_str("idna>=3.6\n")?;
    uv_snapshot!(context.filters(), context.build().args([
        "--preview-features", "build-dependency-check", "--no-build-isolation",
    ]).arg("--offline")
        .arg("--build-constraint").arg(constraints.path()).current_dir(&project), @"
    exit_code: 2 (failure)
    ----- stderr -----
    Building source distribution...
    error: Failed to build `[TEMP_DIR]/project`
      cause: Build requirement is not satisfied: `idna>=3.6`
    ");
    project.child("pyproject.toml").write_str(&format!(
        "{}\n{}",
        fs_err::read_to_string(project.child("pyproject.toml"))?,
        "[tool.uv.extra-build-dependencies]\nproject = ['extra-build-dependency>=1']\n",
    ))?;
    uv_snapshot!(context.filters(), context.build().args([
        "--preview-features", "build-dependency-check", "--no-build-isolation",
    ]).arg("--offline").current_dir(&project), @"
    exit_code: 2 (failure)
    ----- stderr -----
    Building source distribution...
    error: Failed to build `[TEMP_DIR]/project`
      cause: Build requirement is not satisfied: `extra-build-dependency>=1`
    ");
    Ok(())
}

#[test]
fn build_dependency_check_bundled_backend() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let project = context.temp_dir.child("project");
    project.child("pyproject.toml").write_str(&formatdoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["uv_build=={}"]
        build-backend = "uv_build"
    "#, uv_version::version()})?;
    project.child("src/project/__init__.py").touch()?;
    uv_snapshot!(context.filters(), context.build().args([
        "--preview-features", "build-dependency-check", "--no-build-isolation",
    ]).arg("--offline").current_dir(&project), @"
    exit_code: 0 (success)
    ----- stderr -----
    Building source distribution...
    Building wheel from source distribution...
    Successfully built dist/project-0.1.0.tar.gz
    Successfully built dist/project-0.1.0-py3-none-any.whl
    ");
    project
        .child("dist/project-0.1.0.tar.gz")
        .assert(predicate::path::exists());
    project
        .child("dist/project-0.1.0-py3-none-any.whl")
        .assert(predicate::path::exists());
    Ok(())
}

#[test]
fn build_dependency_check_preview_and_skip_dependency_check() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let project = context.temp_dir.child("project");
    project.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["missing-backend>=1"]
        build-backend = "backend"
        backend-path = ["."]
    "#})?;
    project.child("backend.py").write_str(indoc! {r#"
        from pathlib import Path
        import zipfile

        def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
            filename = "project-0.1.0-py3-none-any.whl"
            with zipfile.ZipFile(Path(wheel_directory) / filename, "w") as wheel:
                wheel.writestr("project-0.1.0.dist-info/METADATA", "Metadata-Version: 2.3\nName: project\nVersion: 0.1.0\n")
                wheel.writestr("project-0.1.0.dist-info/WHEEL", "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n")
                wheel.writestr("project-0.1.0.dist-info/RECORD", "")
            return filename

        def get_requires_for_build_wheel(config_settings=None):
            raise RuntimeError("dependency hook must not be called")
    "#})?;
    // Dependency checks require the preview feature.
    uv_snapshot!(context.filters(), context.build().args(["--wheel", "--no-build-isolation", "--offline"])
        .current_dir(&project), @"
    exit_code: 0 (success)
    ----- stderr -----
    Building wheel...
    Successfully built dist/project-0.1.0-py3-none-any.whl
    ");
    // Skip static and dynamic checks even when the preview is enabled.
    uv_snapshot!(context.filters(), context.build().args([
        "--preview-features", "build-dependency-check", "--no-build-isolation",
    ]).args(["--wheel", "--offline", "--skip-dependency-check"])
        .current_dir(&project), @"
    exit_code: 0 (success)
    ----- stderr -----
    Building wheel...
    Successfully built dist/project-0.1.0-py3-none-any.whl
    ");
    Ok(())
}

#[test]
fn build_dependency_check_checks_extracted_sdist() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let project = context.temp_dir.child("project");
    project.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = []
        build-backend = "backend"
        backend-path = ["."]
    "#})?;
    project.child("backend.py").write_str(indoc! {r#"
        from pathlib import Path
        import tarfile

        def get_requires_for_build_sdist(config_settings=None):
            return []

        def get_requires_for_build_wheel(config_settings=None):
            # Only the extracted sdist has a backend that reports this requirement.
            return ["missing-in-sdist>=1"] if Path("PKG-INFO").exists() else []

        def build_sdist(sdist_directory, config_settings=None):
            import io
            filename = "project-0.1.0.tar.gz"
            with tarfile.open(Path(sdist_directory) / filename, "w:gz") as sdist:
                for name in ["pyproject.toml", "backend.py"]:
                    sdist.add(name, arcname=f"project-0.1.0/{name}")
                content = b"Metadata-Version: 2.3\nName: project\nVersion: 0.1.0\n"
                info = tarfile.TarInfo("project-0.1.0/PKG-INFO")
                info.size = len(content)
                sdist.addfile(info, io.BytesIO(content))
            return filename
    "#})?;
    uv_snapshot!(context.filters(), context.build().args([
        "--preview-features", "build-dependency-check", "--no-build-isolation",
    ]).arg("--offline").current_dir(&project), @"
    exit_code: 2 (failure)
    ----- stderr -----
    Building source distribution...
    Building wheel from source distribution...
    error: Failed to build `[TEMP_DIR]/project`
      cause: Build requirement is not satisfied: `missing-in-sdist>=1`
    ");
    project
        .child("dist/project-0.1.0.tar.gz")
        .assert(predicate::path::exists());
    project
        .child("dist/project-0.1.0-py3-none-any.whl")
        .assert(predicate::path::missing());
    Ok(())
}

#[test]
fn build_dependency_check_package_specific_isolation() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let project = context.temp_dir.child("project");
    project.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["missing-backend>=1"]
        build-backend = "backend"
        backend-path = ["."]
    "#})?;
    project.child("backend.py").write_str(indoc! {r#"
        raise RuntimeError("backend must not be imported")
    "#})?;
    uv_snapshot!(context.filters(), context.build().args([
        "--wheel", "--offline", "--preview-features", "build-dependency-check",
        "--no-build-isolation-package", "project",
    ]).current_dir(&project), @"
    exit_code: 2 (failure)
    ----- stderr -----
    Building wheel...
    error: Failed to build `[TEMP_DIR]/project`
      cause: Build requirement is not satisfied: `missing-backend>=1`
    ");
    Ok(())
}

#[test]
fn build_dependency_check_isolated_build_calls_dependency_hook_once() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let project = context.temp_dir.child("project");
    project.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = []
        build-backend = "backend"
        backend-path = ["."]
    "#})?;
    project.child("backend.py").write_str(indoc! {r#"
        from pathlib import Path
        import zipfile

        def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
            filename = "project-0.1.0-py3-none-any.whl"
            with zipfile.ZipFile(Path(wheel_directory) / filename, "w") as wheel:
                wheel.writestr("project-0.1.0.dist-info/METADATA", "Metadata-Version: 2.3\nName: project\nVersion: 0.1.0\n")
                wheel.writestr("project-0.1.0.dist-info/WHEEL", "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n")
                wheel.writestr("project-0.1.0.dist-info/RECORD", "")
            return filename

        def get_requires_for_build_wheel(config_settings=None):
            import sys
            assert sys.prefix != sys.base_prefix
            assert not Path("hook-called").exists()
            Path("hook-called").touch()
            return []
    "#})?;
    uv_snapshot!(context.filters(), context.build().args([
        "--wheel", "--offline", "--preview-features", "build-dependency-check",
    ]).current_dir(&project), @"
    exit_code: 0 (success)
    ----- stderr -----
    Building wheel...
    Successfully built dist/project-0.1.0-py3-none-any.whl
    ");
    // A package-specific exemption for another package must still use an isolated build.
    fs_err::remove_file(project.child("hook-called"))?;
    uv_snapshot!(context.filters(), context.build().args([
        "--wheel", "--offline", "--preview-features", "build-dependency-check",
        "--no-build-isolation-package", "other-project",
    ]).current_dir(&project), @"
    exit_code: 0 (success)
    ----- stderr -----
    Building wheel...
    Successfully built dist/project-0.1.0-py3-none-any.whl
    ");
    Ok(())
}

/// Regression test for <https://github.com/astral-sh/uv/issues/18283>.
///
/// `uv build --all` should respect `tool.uv.build-constraint-dependencies` declared at the
/// workspace root.
#[test]
fn build_all_respects_workspace_build_constraint_dependencies() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filter((r"\\\.", ""))
        .with_filter((r"\[member\]", "[PKG]"));

    let project = context.temp_dir.child("project");

    project.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [tool.uv]
        build-constraint-dependencies = ["hatchling==0.1.0"]

        [tool.uv.workspace]
        members = ["packages/*"]
        "#,
    )?;

    project
        .child("src")
        .child("project")
        .child("__init__.py")
        .touch()?;
    project.child("README").touch()?;

    let member = project.child("packages").child("member");
    fs_err::create_dir_all(member.path())?;

    member.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "member"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["hatchling>=1.0"]
        build-backend = "hatchling.build"
        "#,
    )?;

    member
        .child("src")
        .child("member")
        .child("__init__.py")
        .touch()?;
    member.child("README").touch()?;

    let output = context
        .build()
        .arg("--all")
        .arg("--no-build-logs")
        .current_dir(&project)
        .output()?;

    let stderr = apply_filters(
        String::from_utf8_lossy(&output.stderr).into_owned(),
        context.filters(),
    );

    assert!(
        !output.status.success(),
        "expected `uv build --all` to fail when workspace build constraints make `hatchling` \
         unsatisfiable, but it succeeded:\n{stderr}"
    );
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr.contains("Failed to resolve requirements from `build-system.requires`"),
        "expected build constraint failure in stderr:\n{stderr}"
    );
    assert!(
        stderr.contains(
            "Because you require hatchling>=1.0 and hatchling==0.1.0, we can conclude that your requirements are unsatisfiable."
        ),
        "expected incompatible build constraints in stderr:\n{stderr}"
    );

    project
        .child("dist")
        .child("member-0.1.0.tar.gz")
        .assert(predicate::path::missing());
    project
        .child("dist")
        .child("member-0.1.0-py3-none-any.whl")
        .assert(predicate::path::missing());

    Ok(())
}

/// Limitation: when `uv build` is invoked with an explicit source path, configuration is still
/// loaded from the invocation directory instead of the source workspace. As a result, workspace
/// `build-constraint-dependencies` are not applied here.
#[test]
fn build_source_path_ignores_workspace_build_constraint_dependencies() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filter((r"\\\.", ""))
        .with_filter((r"\[member\]", "[PKG]"));

    let project = context.temp_dir.child("project");

    project.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [tool.uv]
        build-constraint-dependencies = ["hatchling==0.1.0"]

        [tool.uv.workspace]
        members = ["packages/*"]
        "#,
    )?;

    project
        .child("src")
        .child("project")
        .child("__init__.py")
        .touch()?;
    project.child("README").touch()?;

    let member = project.child("packages").child("member");
    fs_err::create_dir_all(member.path())?;

    member.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "member"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["hatchling>=1.0"]
        build-backend = "hatchling.build"
        "#,
    )?;

    member
        .child("src")
        .child("member")
        .child("__init__.py")
        .touch()?;
    member.child("README").touch()?;

    uv_snapshot!(context.filters(), context.build().arg("./project").arg("--all").arg("--no-build-logs"), @"
    exit_code: 0 (success)
    ----- stderr -----
    [PKG] Building source distribution...
    [PKG] Building wheel from source distribution...
    Successfully built project/dist/member-0.1.0.tar.gz
    Successfully built project/dist/member-0.1.0-py3-none-any.whl
    ");

    project
        .child("dist")
        .child("member-0.1.0.tar.gz")
        .assert(predicate::path::is_file());
    project
        .child("dist")
        .child("member-0.1.0-py3-none-any.whl")
        .assert(predicate::path::is_file());

    Ok(())
}

/// A workspace member can use another workspace member as a PEP 517 build dependency, even when
/// that build dependency itself depends on a third workspace member. Regression test for
/// <https://github.com/astral-sh/uv/issues/19074>.
#[test]
fn build_workspace_transitive_build_dependency() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filter((r"\\\.", ""))
        .with_filter((r"\[my-util\]", "[PKG]"))
        .with_filter((r"\[my-backend\]", "[PKG]"))
        .with_filter((r"\[my-tool\]", "[PKG]"));

    let project = context.temp_dir.child("project");

    project.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "my-workspace"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [tool.uv]
        package = false

        [tool.uv.workspace]
        members = ["my-util", "my-backend", "my-tool"]

        [tool.uv.sources]
        my-backend = { workspace = true }
        my-util = { workspace = true }
        "#,
    )?;
    project.child("README.md").touch()?;

    let my_util = project.child("my-util");
    fs_err::create_dir_all(my_util.path())?;
    my_util.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "my-util"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
        "#,
    )?;
    my_util
        .child("src")
        .child("my_util")
        .child("__init__.py")
        .touch()?;
    my_util.child("README.md").touch()?;

    let my_backend = project.child("my-backend");
    fs_err::create_dir_all(my_backend.path())?;
    my_backend.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "my-backend"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["my-util"]

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
        "#,
    )?;
    my_backend
        .child("src")
        .child("my_backend")
        .child("__init__.py")
        .touch()?;
    my_backend.child("README.md").touch()?;

    let my_tool = project.child("my-tool");
    fs_err::create_dir_all(my_tool.path())?;
    my_tool.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "my-tool"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["my-backend", "hatchling"]
        build-backend = "hatchling.build"
        "#,
    )?;
    my_tool
        .child("src")
        .child("my_tool")
        .child("__init__.py")
        .touch()?;
    my_tool.child("README.md").touch()?;

    uv_snapshot!(
        &context.filters(),
        context
            .build()
            .arg("--wheel")
            .arg("--package")
            .arg("my-tool")
            .current_dir(&project),
        @r"
    exit_code: 0 (success)
    ----- stderr -----
    Building wheel...
    Successfully built dist/my_tool-0.1.0-py3-none-any.whl
    "
    );

    project
        .child("dist")
        .child("my_tool-0.1.0-py3-none-any.whl")
        .assert(predicate::path::is_file());

    Ok(())
}

#[test]
fn build_sha() -> Result<()> {
    let context = uv_test::test_context!(DEFAULT_PYTHON_VERSION).with_filter((r"\\\.", ""));

    let project = context.temp_dir.child("project");

    let pyproject_toml = project.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.8"
        dependencies = ["anyio==3.7.0"]

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
        "#,
    )?;

    project
        .child("src")
        .child("project")
        .child("__init__.py")
        .touch()?;
    project.child("README").touch()?;

    // Validate the original declaration, including extras, before dropping empty constraints.
    let constraints = project.child("constraints.txt");
    constraints.write_str("hatchling[foo]")?;

    uv_snapshot!(context.filters(), context.build().arg("--build-constraint").arg("constraints.txt").arg("--require-hashes").current_dir(&project), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Failed to build `[TEMP_DIR]/project`
      cause: In `--require-hashes` mode, all requirements must have their versions pinned with `==`, but found: hatchling[foo]
    ");

    // Reject an incorrect hash.
    constraints.write_str(indoc::indoc! {r"
        hatchling==1.22.4 \
            --hash=sha256:a248cb506794bececcddeddb1678bc722f9cfcacf02f98f7c0af6b9ed893caf2 \
            --hash=sha256:e16da5bfc396af7b29daa3164851dd04991c994083f56cb054b5003675caecdc
        packaging==24.0 \
            --hash=sha256:2ddfb553fdf02fb784c234c7ba6ccc288296ceabec964ad2eae3777778130bc5 \
            --hash=sha256:eb82c5e3e56209074766e6885bb04b8c38a0c015d0a30036ebe7ece34c9989e9
            # via hatchling
        pathspec==0.12.1 \
            --hash=sha256:a0d503e138a4c123b27490a4f7beda6a01c6f288df0e4a8b79c7eb0dc7b4cc08 \
            --hash=sha256:a482d51503a1ab33b1c67a6c3813a26953dbdc71c31dacaef9a838c4e29f5712
            # via hatchling
        pluggy==1.4.0 \
            --hash=sha256:7db9f7b503d67d1c5b95f59773ebb58a8c1c288129a88665838012cfb07b8981 \
            --hash=sha256:8c85c2876142a764e5b7548e7d9a0e0ddb46f5185161049a79b7e974454223be
            # via hatchling
        tomli==2.0.1 \
            --hash=sha256:939de3e7a6161af0c887ef91b7d41a53e7c5a1ca976325f429cb46ea9bc30ecc \
            --hash=sha256:de526c12914f0c550d15924c62d72abc48d6fe7364aa87328337a31007fe8a4f
            # via hatchling
        trove-classifiers==2024.3.3 \
            --hash=sha256:3a84096861b385ec422c79995d1f6435dde47a9b63adaa3c886e53232ba7e6e0 \
            --hash=sha256:df7edff9c67ff86b733628998330b180e81d125b1e096536d83ac0fd79673fdc
            # via hatchling
    "})?;

    uv_snapshot!(context.filters(), context.build().arg("--build-constraint").arg("constraints.txt").current_dir(&project), @"
    exit_code: 2 (failure)
    ----- stderr -----
    Building source distribution...
    error: Failed to build `[TEMP_DIR]/project`
      cause: Failed to install requirements from `build-system.requires`
      cause: Failed to download `hatchling==1.22.4`
      cause: Hash mismatch for `hatchling==1.22.4`

             Expected:
               sha256:a248cb506794bececcddeddb1678bc722f9cfcacf02f98f7c0af6b9ed893caf2
               sha256:e16da5bfc396af7b29daa3164851dd04991c994083f56cb054b5003675caecdc

             Computed:
               sha256:f56da5bfc396af7b29daa3164851dd04991c994083f56cb054b5003675caecdc
    ");

    project
        .child("dist")
        .child("project-0.1.0.tar.gz")
        .assert(predicate::path::missing());
    project
        .child("dist")
        .child("project-0.1.0-py3-none-any.whl")
        .assert(predicate::path::missing());

    fs_err::remove_dir_all(project.child("dist"))?;

    // Reject a missing hash with `--requires-hashes`.
    uv_snapshot!(context.filters(), context.build().arg("--build-constraint").arg("constraints.txt").arg("--require-hashes").current_dir(&project), @"
    exit_code: 2 (failure)
    ----- stderr -----
    Building source distribution...
    error: Failed to build `[TEMP_DIR]/project`
      cause: Failed to install requirements from `build-system.requires`
      cause: Failed to download `hatchling==1.22.4`
      cause: Hash mismatch for `hatchling==1.22.4`

             Expected:
               sha256:a248cb506794bececcddeddb1678bc722f9cfcacf02f98f7c0af6b9ed893caf2
               sha256:e16da5bfc396af7b29daa3164851dd04991c994083f56cb054b5003675caecdc

             Computed:
               sha256:f56da5bfc396af7b29daa3164851dd04991c994083f56cb054b5003675caecdc
    ");

    project
        .child("dist")
        .child("project-0.1.0.tar.gz")
        .assert(predicate::path::missing());
    project
        .child("dist")
        .child("project-0.1.0-py3-none-any.whl")
        .assert(predicate::path::missing());

    fs_err::remove_dir_all(project.child("dist"))?;

    // Reject a missing hash.
    let constraints = project.child("constraints.txt");
    constraints.write_str("hatchling==1.22.4")?;

    uv_snapshot!(context.filters(), context.build().arg("--build-constraint").arg("constraints.txt").arg("--require-hashes").current_dir(&project), @"
    exit_code: 2 (failure)
    ----- stderr -----
    Building source distribution...
    error: Failed to build `[TEMP_DIR]/project`
      cause: Failed to resolve requirements from `build-system.requires`
      cause: No solution found when resolving: `hatchling`
      cause: In `--require-hashes` mode, all requirements must be pinned upfront with `==`, but found: `hatchling`
    ");

    project
        .child("dist")
        .child("project-0.1.0.tar.gz")
        .assert(predicate::path::missing());
    project
        .child("dist")
        .child("project-0.1.0-py3-none-any.whl")
        .assert(predicate::path::missing());

    fs_err::remove_dir_all(project.child("dist"))?;

    // Accept a correct hash.
    let constraints = project.child("constraints.txt");
    constraints.write_str(indoc::indoc! {r"
        hatchling==1.22.4 \
            --hash=sha256:8a2dcec96d7fb848382ef5848e5ac43fdae641f35a08a3fab5116bd495f3416e \
            --hash=sha256:f56da5bfc396af7b29daa3164851dd04991c994083f56cb054b5003675caecdc
        packaging==24.0 \
            --hash=sha256:2ddfb553fdf02fb784c234c7ba6ccc288296ceabec964ad2eae3777778130bc5 \
            --hash=sha256:eb82c5e3e56209074766e6885bb04b8c38a0c015d0a30036ebe7ece34c9989e9
            # via hatchling
        pathspec==0.12.1 \
            --hash=sha256:a0d503e138a4c123b27490a4f7beda6a01c6f288df0e4a8b79c7eb0dc7b4cc08 \
            --hash=sha256:a482d51503a1ab33b1c67a6c3813a26953dbdc71c31dacaef9a838c4e29f5712
            # via hatchling
        pluggy==1.4.0 \
            --hash=sha256:7db9f7b503d67d1c5b95f59773ebb58a8c1c288129a88665838012cfb07b8981 \
            --hash=sha256:8c85c2876142a764e5b7548e7d9a0e0ddb46f5185161049a79b7e974454223be
            # via hatchling
        tomli==2.0.1 \
            --hash=sha256:939de3e7a6161af0c887ef91b7d41a53e7c5a1ca976325f429cb46ea9bc30ecc \
            --hash=sha256:de526c12914f0c550d15924c62d72abc48d6fe7364aa87328337a31007fe8a4f
            # via hatchling
        trove-classifiers==2024.3.3 \
            --hash=sha256:3a84096861b385ec422c79995d1f6435dde47a9b63adaa3c886e53232ba7e6e0 \
            --hash=sha256:df7edff9c67ff86b733628998330b180e81d125b1e096536d83ac0fd79673fdc
            # via hatchling
    "})?;

    uv_snapshot!(context.filters(), context.build().arg("--build-constraint").arg("constraints.txt").current_dir(&project), @"
    exit_code: 0 (success)
    ----- stderr -----
    Building source distribution...
    Building wheel from source distribution...
    Successfully built dist/project-0.1.0.tar.gz
    Successfully built dist/project-0.1.0-py3-none-any.whl
    ");

    project
        .child("dist")
        .child("project-0.1.0.tar.gz")
        .assert(predicate::path::is_file());
    project
        .child("dist")
        .child("project-0.1.0-py3-none-any.whl")
        .assert(predicate::path::is_file());

    Ok(())
}

#[tokio::test]
async fn build_transitive_url_build_requirement_hashes() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_filter((r"\\\.", ""));

    let links = context.workspace_root.join("test/links");
    let ok_filename = "ok-1.0.0-py3-none-any.whl";
    let validation_filename = "validation-1.0.0-py3-none-any.whl";
    let ok_server = PackageServer::new(&"ok".parse()?).await;
    let validation_server = PackageServer::new(&"validation".parse()?).await;
    let ok_wheel_url = ok_server.file_url(ok_filename);
    let validation_wheel_url = validation_server.file_url(validation_filename);

    ok_server
        .serve(ok_filename, &fs_err::read(links.join(ok_filename))?, None)
        .await;
    validation_server
        .serve(
            validation_filename,
            &fs_err::read(links.join(validation_filename))?,
            None,
        )
        .await;

    let project = context.temp_dir.child("project");

    project.child("pyproject.toml").write_str(&formatdoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["validation @ {validation_wheel_url}#sha256=23ee8bda94d44f5480dccca240b37a4de7c823bc4683d00fd8e5eb85cf056ce6"]
        build-backend = "backend"
        backend-path = ["."]

        [[tool.uv.dependency-metadata]]
        name = "validation"
        version = "1.0.0"
        requires-dist = ["ok @ {ok_wheel_url}#sha256=79f0b33e6ce1e09eaa1784c8eee275dfe84d215d9c65c652f07c18e85fdaac5f"]
    "#})?;
    project.child("backend.py").write_str(indoc! {r#"
        import pathlib
        import zipfile


        def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
            wheel_name = "project-0.1.0-py3-none-any.whl"
            wheel_path = pathlib.Path(wheel_directory, wheel_name)
            records = [
                ("project/__init__.py", b""),
                (
                    "project-0.1.0.dist-info/METADATA",
                    b"Metadata-Version: 2.1\nName: project\nVersion: 0.1.0\n",
                ),
                (
                    "project-0.1.0.dist-info/WHEEL",
                    b"Wheel-Version: 1.0\nGenerator: uv-test\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
                ),
            ]

            with zipfile.ZipFile(wheel_path, "w") as wheel:
                for path, contents in records:
                    wheel.writestr(path, contents)
                record = "\n".join(f"{path},," for path, _ in records)
                wheel.writestr(
                    "project-0.1.0.dist-info/RECORD",
                    record + "\nproject-0.1.0.dist-info/RECORD,,\n",
                )

            return wheel_name
    "#})?;
    uv_snapshot!(
        &context.filters(),
        context
            .build()
            .arg("--wheel")
            .arg("--require-hashes")
            .current_dir(&project),
        @"
    exit_code: 0 (success)
    ----- stderr -----
    Building wheel...
    Successfully built dist/project-0.1.0-py3-none-any.whl
    "
    );

    project
        .child("dist")
        .child("project-0.1.0-py3-none-any.whl")
        .assert(predicate::path::is_file());

    Ok(())
}

#[test]
fn build_quiet() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let project = context.temp_dir.child("project");

    let pyproject_toml = project.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0"]

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
        "#,
    )?;

    project
        .child("src")
        .child("project")
        .child("__init__.py")
        .touch()?;
    project.child("README").touch()?;

    uv_snapshot!(&context.filters(), context.build().arg("project").arg("-q"), @"
    exit_code: 0 (success)
    ");

    Ok(())
}

#[test]
fn build_no_build_logs() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let project = context.temp_dir.child("project");

    let pyproject_toml = project.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0"]

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
        "#,
    )?;

    project
        .child("src")
        .child("project")
        .child("__init__.py")
        .touch()?;
    project.child("README").touch()?;

    uv_snapshot!(&context.filters(), context.build().arg("project").arg("--no-build-logs"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Building source distribution...
    Building wheel from source distribution...
    Successfully built project/dist/project-0.1.0.tar.gz
    Successfully built project/dist/project-0.1.0-py3-none-any.whl
    ");

    Ok(())
}

/// Test that `UV_HIDE_BUILD_OUTPUT` suppresses build output.
#[test]
fn build_hide_build_output_env_var() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let project = context.temp_dir.child("project");

    let pyproject_toml = project.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0"]

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
        "#,
    )?;

    project
        .child("src")
        .child("project")
        .child("__init__.py")
        .touch()?;
    project.child("README").touch()?;

    uv_snapshot!(&context.filters(), context.build().arg("project").env(EnvVars::UV_HIDE_BUILD_OUTPUT, "1"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Building source distribution...
    Building wheel from source distribution...
    Successfully built project/dist/project-0.1.0.tar.gz
    Successfully built project/dist/project-0.1.0-py3-none-any.whl
    ");

    Ok(())
}

/// Test that `UV_HIDE_BUILD_OUTPUT` hides build output even on failure.
#[test]
fn build_hide_build_output_on_failure() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_filter((r"\\\.", ""));

    let project = context.temp_dir.child("project");

    let pyproject_toml = project.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    // Create a `setup.py` that prints an environment variable before failing.
    project.child("setup.py").write_str(indoc! {r#"
        import os
        import sys
        print("FOO=" + os.environ.get("FOO", "not-set"), file=sys.stderr)
        sys.stderr.flush()
        raise Exception("Build failed intentionally!")
        "#})?;

    // With `UV_HIDE_BUILD_OUTPUT`, the output is hidden even on failure.
    uv_snapshot!(context.filters(), context.build().arg("project").env(EnvVars::UV_HIDE_BUILD_OUTPUT, "1").env("FOO", "bar"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    Building source distribution...
    error: Failed to build `[TEMP_DIR]/project`
      cause: The build backend returned an error
      cause: Call to `setuptools.build_meta.get_requires_for_build_sdist` failed (exit status: 1)

    hint: Build failures usually indicate a problem with the package or the build environment
    ");

    Ok(())
}

#[test]
fn build_tool_uv_sources() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_filter((r"\\\.", ""));

    let build = context.temp_dir.child("backend");
    build.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "backend"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["typing-extensions>=3.10"]

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
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

    let project = context.temp_dir.child("project");

    project.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig>1"]

        [build-system]
        requires = ["hatchling", "backend==0.1.0"]
        build-backend = "hatchling.build"

        [tool.uv.sources]
        backend = { path = "../backend" }
        "#,
    )?;

    project.child("setup.py").write_str(indoc! {r"
        from setuptools import setup

        from backend import hello

        hello()

        setup()
        ",
    })?;

    project
        .child("src")
        .child("project")
        .child("__init__.py")
        .touch()?;
    project.child("README").touch()?;

    // Build restrictions still apply to dependencies. Check before building the backend so a
    // cached wheel cannot make these commands succeed.
    uv_snapshot!(context.filters(), context.build().arg("--no-build").current_dir(project.path()), @"
    exit_code: 2 (failure)
    ----- stderr -----
    Building source distribution...
    error: Failed to build `[TEMP_DIR]/project`
      cause: Failed to install requirements from `build-system.requires`
      cause: Building source distributions is disabled, but attempted to build `backend`
    ");

    uv_snapshot!(context.filters(), context.build().arg("--no-build-package").arg("backend").current_dir(project.path()), @"
    exit_code: 2 (failure)
    ----- stderr -----
    Building source distribution...
    error: Failed to build `[TEMP_DIR]/project`
      cause: Failed to install requirements from `build-system.requires`
      cause: Building source distributions is disabled, but attempted to build `backend`
    ");

    project
        .child("dist")
        .child("project-0.1.0.tar.gz")
        .assert(predicate::path::missing());
    project
        .child("dist")
        .child("project-0.1.0-py3-none-any.whl")
        .assert(predicate::path::missing());

    uv_snapshot!(context.filters(), context.build().current_dir(project.path()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Building source distribution...
    Building wheel from source distribution...
    Successfully built dist/project-0.1.0.tar.gz
    Successfully built dist/project-0.1.0-py3-none-any.whl
    ");

    project
        .child("dist")
        .child("project-0.1.0.tar.gz")
        .assert(predicate::path::is_file());
    project
        .child("dist")
        .child("project-0.1.0-py3-none-any.whl")
        .assert(predicate::path::is_file());

    Ok(())
}

#[test]
fn build_named_index_config_file_hint() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let project = context.temp_dir.child("project");
    project.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"

        [tool.uv.sources]
        hatchling = { index = "privindex" }
        "#,
    )?;

    project.child("uv.toml").write_str(
        r#"
        [[index]]
        name = "privindex"
        url = "https://example.com/simple"
        explicit = true
        "#,
    )?;

    uv_snapshot!(context.filters(), context.build().current_dir(project.path()), @"
    exit_code: 2 (failure)
    ----- stderr -----
    Building source distribution...
    error: Failed to build `[TEMP_DIR]/project`
      cause: Failed to parse entry: `hatchling`
      cause: Package `hatchling` references an undeclared index: `privindex`

    hint: Index `privindex` was found in a project-level `uv.toml`, but indexes referenced via `tool.uv.sources` must be defined in the project's `pyproject.toml`
    ");

    Ok(())
}

/// Check that we have a working git boundary for builds from source dist to wheel in `dist/`.
#[test]
fn build_git_boundary_in_dist_build() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let project = context.temp_dir.child("demo");
    project.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "demo"
        version = "0.1.0"
        requires-python = ">=3.11"

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
        "#,
    )?;
    project.child("src/demo/__init__.py").write_str(
        r#"
        def run():
            print("Running like the wind!")
        "#,
    )?;

    uv_snapshot!(&context.filters(), context.build().current_dir(project.path()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Building source distribution...
    Building wheel from source distribution...
    Successfully built dist/demo-0.1.0.tar.gz
    Successfully built dist/demo-0.1.0-py3-none-any.whl
    ");

    // Check that the source file is included
    let files = zip_file_names(&project.join("dist/demo-0.1.0-py3-none-any.whl"))?;
    assert_snapshot!(files.join("\n"), @"
    demo-0.1.0.dist-info/METADATA
    demo-0.1.0.dist-info/RECORD
    demo-0.1.0.dist-info/WHEEL
    demo/__init__.py
    ");

    Ok(())
}

#[test]
fn build_non_package() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filter((r"\\\.", ""))
        .with_filter((r"\[project\]", "[PKG]"))
        .with_filter((r"\[member\]", "[PKG]"));

    let project = context.temp_dir.child("project");

    let pyproject_toml = project.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0"]

        [tool.uv.workspace]
        members = ["packages/*"]
        "#,
    )?;

    project.child("src").child("__init__.py").touch()?;
    project.child("README").touch()?;

    let member = project.child("packages").child("member");
    fs_err::create_dir_all(member.path())?;

    member.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "member"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]
        "#,
    )?;

    member.child("src").child("__init__.py").touch()?;
    member.child("README").touch()?;

    // Build the member.
    uv_snapshot!(context.filters(), context.build().arg("--package").arg("member").current_dir(&project), @r#"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Package `member` is missing a `build-system`. For example, to build with `uv_build`, add the following to `packages/member/pyproject.toml`:
    ```toml
    [build-system]
    requires = ["uv_build>=[CURRENT_VERSION],<[NEXT_BREAKING]"]
    build-backend = "uv_build"
    ```
    "#);

    project
        .child("dist")
        .child("member-0.1.0.tar.gz")
        .assert(predicate::path::missing());
    project
        .child("dist")
        .child("member-0.1.0-py3-none-any.whl")
        .assert(predicate::path::missing());

    // Build all packages.
    uv_snapshot!(context.filters(), context.build().arg("--all").arg("--no-build-logs").current_dir(&project), @r#"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Workspace does not contain any buildable packages. For example, to build `member` with `uv_build`, add a `build-system` to `packages/member/pyproject.toml`:
    ```toml
    [build-system]
    requires = ["uv_build>=[CURRENT_VERSION],<[NEXT_BREAKING]"]
    build-backend = "uv_build"
    ```
    "#);

    project
        .child("dist")
        .child("member-0.1.0.tar.gz")
        .assert(predicate::path::missing());
    project
        .child("dist")
        .child("member-0.1.0-py3-none-any.whl")
        .assert(predicate::path::missing());

    Ok(())
}

/// Test the uv fast path. Tests all four possible build plans:
/// * Defaults
/// * `--sdist`
/// * `--wheel`
/// * `--sdist --wheel`
#[test]
fn build_fast_path() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let built_by_uv = current_dir()?.join("../../test/packages/built-by-uv");

    uv_snapshot!(context.build()
        .arg(&built_by_uv)
        .arg("--out-dir")
        .arg(context.temp_dir.join("output1")), @"
    exit_code: 0 (success)
    ----- stderr -----
    Building source distribution...
    Building wheel from source distribution...
    Successfully built output1/built_by_uv-0.1.0.tar.gz
    Successfully built output1/built_by_uv-0.1.0-py3-none-any.whl
    ");
    context
        .temp_dir
        .child("output1")
        .child("built_by_uv-0.1.0.tar.gz")
        .assert(predicate::path::is_file());
    context
        .temp_dir
        .child("output1")
        .child("built_by_uv-0.1.0-py3-none-any.whl")
        .assert(predicate::path::is_file());

    uv_snapshot!(context.build()
        .arg(&built_by_uv)
        .arg("--out-dir")
        .arg(context.temp_dir.join("output2"))
        .arg("--sdist"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Building source distribution...
    Successfully built output2/built_by_uv-0.1.0.tar.gz
    ");
    context
        .temp_dir
        .child("output2")
        .child("built_by_uv-0.1.0.tar.gz")
        .assert(predicate::path::is_file());

    uv_snapshot!(context.build()
        .arg(&built_by_uv)
        .arg("--out-dir")
        .arg(context.temp_dir.join("output3"))
        .arg("--wheel"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Building wheel...
    Successfully built output3/built_by_uv-0.1.0-py3-none-any.whl
    ");
    context
        .temp_dir
        .child("output3")
        .child("built_by_uv-0.1.0-py3-none-any.whl")
        .assert(predicate::path::is_file());

    uv_snapshot!(context.build()
        .arg(&built_by_uv)
        .arg("--out-dir")
        .arg(context.temp_dir.join("output4"))
        .arg("--sdist")
        .arg("--wheel"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Building source distribution...
    Building wheel...
    Successfully built output4/built_by_uv-0.1.0.tar.gz
    Successfully built output4/built_by_uv-0.1.0-py3-none-any.whl
    ");
    context
        .temp_dir
        .child("output4")
        .child("built_by_uv-0.1.0.tar.gz")
        .assert(predicate::path::is_file());
    context
        .temp_dir
        .child("output4")
        .child("built_by_uv-0.1.0-py3-none-any.whl")
        .assert(predicate::path::is_file());

    Ok(())
}

/// Warn about an unbounded build backend only when producing a source distribution.
#[test]
fn build_fast_path_unbounded_backend() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let filters = context
        .filters()
        .into_iter()
        .chain([(r"such as `<\d+\.\d+`", "such as `<[NEXT_BREAKING]`")])
        .collect::<Vec<_>>();
    let project = context.temp_dir.child("project");

    project.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["uv-build"]
        build-backend = "uv_build"
    "#})?;
    project.child("src/project/__init__.py").touch()?;

    uv_snapshot!(&filters, context.build().arg("project").arg("--wheel"), @r"
    exit_code: 0 (success)
    ----- stderr -----
    Building wheel...
    Successfully built project/dist/project-0.1.0-py3-none-any.whl
    ");

    uv_snapshot!(&filters, context.build().arg("project").arg("--sdist"), @r#"
    exit_code: 0 (success)
    ----- stderr -----
    Building source distribution...
    warning: `build_system.requires = ["uv-build"]` is missing an upper bound on the `uv_build` version such as `<[NEXT_BREAKING]`. Without bounding the `uv_build` version, the source distribution will break when a future, breaking version of `uv_build` is released.
    Successfully built project/dist/project-0.1.0.tar.gz
    "#);

    Ok(())
}

/// Only mention the bundled build backend when verbose logging is enabled.
#[test]
fn build_fast_path_verbose() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let project = context.temp_dir.child("project");

    project.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["uv_build>=0.5.15,<10000"]
        build-backend = "uv_build"
    "#})?;
    project.child("src/project/__init__.py").touch()?;

    let output = context
        .build()
        .arg("project")
        .arg("--sdist")
        .arg("--verbose")
        .output()?;

    let stderr = apply_filters(
        String::from_utf8_lossy(&output.stderr).into_owned(),
        context.filters(),
    );
    assert!(output.status.success(), "build failed:\n{stderr}");

    let messages = stderr
        .lines()
        .filter(|line| {
            line.starts_with("DEBUG Using bundled `uv_build` backend for")
                || line.starts_with("Building ")
                || line.starts_with("Successfully built ")
        })
        .collect::<Vec<_>>()
        .join("\n");

    assert_snapshot!(messages, @r"
    DEBUG Using bundled `uv_build` backend for `project`
    Building source distribution...
    Successfully built project/dist/project-0.1.0.tar.gz
    ");

    Ok(())
}

/// Exact backend pins must match the running uv version; compatible ranges can use the fast path.
#[test]
fn build_fast_path_exact_pin() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let project = context.temp_dir.child("project");
    let pyproject_toml = project.child("pyproject.toml");

    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["uv_build==0.11.33"]
        build-backend = "uv_build"
    "#})?;
    project.child("src/project/__init__.py").touch()?;

    uv_snapshot!(context.filters(), context.build()
        .arg("project")
        .arg("--wheel")
        .arg("--no-index"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    Building wheel...
    error: Failed to build `[TEMP_DIR]/project`
      cause: Failed to resolve requirements from `build-system.requires`
      cause: No solution found when resolving: `uv-build==0.11.33`
      cause: Because uv-build was not found in the provided package locations and you require uv-build==0.11.33, we can conclude that your requirements are unsatisfiable.

    hint: Packages were unavailable because index lookups were disabled and no additional package locations were provided (try: `--find-links <uri>`)
    ");

    for requirement in [
        format!("uv_build=={}", uv_version::version()),
        "uv_build>=0.11,<0.12".to_string(),
        "uv_build==0.11.*".to_string(),
        "uv_build==0.11.33 ; python_version < '0'".to_string(),
    ] {
        pyproject_toml.write_str(&formatdoc! {r#"
            [project]
            name = "project"
            version = "0.1.0"
            requires-python = ">=3.12"

            [build-system]
            requires = ["{requirement}"]
            build-backend = "uv_build"
        "#})?;

        context
            .build()
            .arg("project")
            .arg("--wheel")
            .arg("--no-index")
            .assert()
            .success();
    }

    Ok(())
}

/// Active exact build constraints must match the running uv version to use the fast path.
#[test]
fn build_fast_path_constraint_exact_pin() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let project = context.temp_dir.child("project");

    project.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["uv_build>=0.11,<10000"]
        build-backend = "uv_build"
    "#})?;
    project.child("src/project/__init__.py").touch()?;

    let constraints = context.temp_dir.child("constraints.txt");
    constraints.write_str("uv_build==0.11.33")?;

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("./project")
        .arg("--no-index")
        .arg("--build-constraint")
        .arg("constraints.txt"), @"
    exit_code: 1 (failure)
    ----- stderr -----
    Resolved 1 package in [TIME]
    error: Failed to build `project @ file://[TEMP_DIR]/project`
      cause: Failed to resolve requirements from `build-system.requires`
      cause: No solution found when resolving: `uv-build>=0.11, <10000`
      cause: Because uv-build was not found in the provided package locations and you require uv-build==0.11.33, we can conclude that your requirements are unsatisfiable.

    hint: Packages were unavailable because index lookups were disabled and no additional package locations were provided (try: `--find-links <uri>`)
    ");

    constraints.write_str("uv_build>=0.11,==0.11.33")?;

    uv_snapshot!(context.filters(), context.build()
        .arg("project")
        .arg("--wheel")
        .arg("--no-index")
        .arg("--build-constraint")
        .arg("constraints.txt"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    Building wheel...
    error: Failed to build `[TEMP_DIR]/project`
      cause: Failed to resolve requirements from `build-system.requires`
      cause: No solution found when resolving: `uv-build>=0.11, <10000`
      cause: Because uv-build was not found in the provided package locations and you require uv-build==0.11.33, we can conclude that your requirements are unsatisfiable.

    hint: Packages were unavailable because index lookups were disabled and no additional package locations were provided (try: `--find-links <uri>`)
    ");

    // Listing files requires the fast path and must reject the incompatible constraint.
    uv_snapshot!(context.filters(), context.build()
        .arg("project")
        .arg("--wheel")
        .arg("--list")
        .arg("--build-constraint")
        .arg("constraints.txt"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Failed to build `[TEMP_DIR]/project`
      cause: Can only use `--list` with a compatible uv build backend, but `project` is not compatible because `uv_build==0.11.33` does not match the running uv version
    ");

    for constraint in [
        format!("uv_build=={}", uv_version::version()),
        "uv_build==0.11.33 ; python_version < '0'".to_string(),
        "uv_build==0.11.*".to_string(),
        "other-package==0.11.33".to_string(),
        "uv_build>=0.11,<0.12".to_string(),
    ] {
        constraints.write_str(&constraint)?;

        context
            .build()
            .arg("project")
            .arg("--wheel")
            .arg("--no-index")
            .arg("--build-constraint")
            .arg("constraints.txt")
            .assert()
            .success();
    }

    Ok(())
}

/// Reject path-shaped script entry point names before writing wheel metadata.
#[test]
fn build_unsafe_script_entry_point_name() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    context.init().assert().success();

    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"

        [project.scripts]
        "../script" = "project:main"

        [build-system]
        requires = ["uv_build>=0.5.15,<10000"]
        build-backend = "uv_build"
    "#})?;
    context
        .temp_dir
        .child("src")
        .child("project")
        .child("__init__.py")
        .touch()?;

    uv_snapshot!(context.filters(), context.build().arg("--wheel"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    Building wheel...
    error: Failed to build `[TEMP_DIR]/`
      cause: Invalid project metadata
      cause: Script entry point name `../script` must include a non-dot character and consist only of letters, numbers, dots, underscores and dashes
    ");

    Ok(())
}

/// Reject dot-only script entry point names that do not resolve below the scripts directory.
#[test]
fn build_dot_script_entry_point_name() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    context.init().assert().success();

    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"

        [project.scripts]
        "." = "project:main"

        [build-system]
        requires = ["uv_build>=0.5.15,<10000"]
        build-backend = "uv_build"
    "#})?;
    context
        .temp_dir
        .child("src")
        .child("project")
        .child("__init__.py")
        .touch()?;

    uv_snapshot!(context.filters(), context.build().arg("--wheel"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    Building wheel...
    error: Failed to build `[TEMP_DIR]/`
      cause: Invalid project metadata
      cause: Script entry point name `.` must include a non-dot character and consist only of letters, numbers, dots, underscores and dashes
    ");

    Ok(())
}

/// Reject script entry point names that PyPI rejects in uploaded wheels.
#[test]
fn build_nested_script_entry_point_name() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    context.init().assert().success();

    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"

        [project.scripts]
        "nested/script" = "project:main"

        [build-system]
        requires = ["uv_build>=0.5.15,<10000"]
        build-backend = "uv_build"
    "#})?;
    context
        .temp_dir
        .child("src")
        .child("project")
        .child("__init__.py")
        .touch()?;

    uv_snapshot!(context.filters(), context.build().arg("--wheel"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    Building wheel...
    error: Failed to build `[TEMP_DIR]/`
      cause: Invalid project metadata
      cause: Script entry point name `nested/script` must include a non-dot character and consist only of letters, numbers, dots, underscores and dashes
    ");

    Ok(())
}

/// Test the `--list` option.
#[test]
fn build_list_files() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let built_by_uv = current_dir()?.join("../../test/packages/built-by-uv");

    // By default, we build the wheel from the source dist, which we need to do even for the list
    // task.
    uv_snapshot!(context.build()
        .arg(&built_by_uv)
        .arg("--out-dir")
        .arg(context.temp_dir.join("output1"))
        .arg("--list"), @"
    exit_code: 0 (success)
    ----- stdout -----
    Building built_by_uv-0.1.0.tar.gz will include the following files:
    built_by_uv-0.1.0/PKG-INFO (generated)
    built_by_uv-0.1.0/pyproject.toml (generated)
    built_by_uv-0.1.0/pyproject.toml.orig (pyproject.toml)
    built_by_uv-0.1.0/LICENSE-APACHE (LICENSE-APACHE)
    built_by_uv-0.1.0/LICENSE-MIT (LICENSE-MIT)
    built_by_uv-0.1.0/README.md (README.md)
    built_by_uv-0.1.0/assets/data.csv (assets/data.csv)
    built_by_uv-0.1.0/header/built_by_uv.h (header/built_by_uv.h)
    built_by_uv-0.1.0/scripts/whoami.sh (scripts/whoami.sh)
    built_by_uv-0.1.0/src/built_by_uv/__init__.py (src/built_by_uv/__init__.py)
    built_by_uv-0.1.0/src/built_by_uv/arithmetic/__init__.py (src/built_by_uv/arithmetic/__init__.py)
    built_by_uv-0.1.0/src/built_by_uv/arithmetic/circle.py (src/built_by_uv/arithmetic/circle.py)
    built_by_uv-0.1.0/src/built_by_uv/arithmetic/pi.txt (src/built_by_uv/arithmetic/pi.txt)
    built_by_uv-0.1.0/src/built_by_uv/build-only.h (src/built_by_uv/build-only.h)
    built_by_uv-0.1.0/src/built_by_uv/cli.py (src/built_by_uv/cli.py)
    built_by_uv-0.1.0/third-party-licenses/PEP-401.txt (third-party-licenses/PEP-401.txt)
    Building built_by_uv-0.1.0-py3-none-any.whl will include the following files:
    built_by_uv/__init__.py (src/built_by_uv/__init__.py)
    built_by_uv/arithmetic/__init__.py (src/built_by_uv/arithmetic/__init__.py)
    built_by_uv/arithmetic/circle.py (src/built_by_uv/arithmetic/circle.py)
    built_by_uv/arithmetic/pi.txt (src/built_by_uv/arithmetic/pi.txt)
    built_by_uv/cli.py (src/built_by_uv/cli.py)
    built_by_uv-0.1.0.dist-info/licenses/LICENSE-APACHE (LICENSE-APACHE)
    built_by_uv-0.1.0.dist-info/licenses/LICENSE-MIT (LICENSE-MIT)
    built_by_uv-0.1.0.dist-info/licenses/third-party-licenses/PEP-401.txt (third-party-licenses/PEP-401.txt)
    built_by_uv-0.1.0.data/headers/built_by_uv.h (header/built_by_uv.h)
    built_by_uv-0.1.0.data/scripts/whoami.sh (scripts/whoami.sh)
    built_by_uv-0.1.0.data/data/data.csv (assets/data.csv)
    built_by_uv-0.1.0.dist-info/WHEEL (generated)
    built_by_uv-0.1.0.dist-info/entry_points.txt (generated)
    built_by_uv-0.1.0.dist-info/METADATA (generated)

    ----- stderr -----
    Building source distribution...
    Successfully built output1/built_by_uv-0.1.0.tar.gz
    ");
    context
        .temp_dir
        .child("output1")
        .child("built_by_uv-0.1.0.tar.gz")
        .assert(predicate::path::is_file());
    context
        .temp_dir
        .child("output1")
        .child("built_by_uv-0.1.0-py3-none-any.whl")
        .assert(predicate::path::missing());

    uv_snapshot!(context.build()
        .arg(&built_by_uv)
        .arg("--out-dir")
        .arg(context.temp_dir.join("output2"))
        .arg("--list")
        .arg("--sdist")
        .arg("--wheel"), @"
    exit_code: 0 (success)
    ----- stdout -----
    Building built_by_uv-0.1.0.tar.gz will include the following files:
    built_by_uv-0.1.0/PKG-INFO (generated)
    built_by_uv-0.1.0/pyproject.toml (generated)
    built_by_uv-0.1.0/pyproject.toml.orig (pyproject.toml)
    built_by_uv-0.1.0/LICENSE-APACHE (LICENSE-APACHE)
    built_by_uv-0.1.0/LICENSE-MIT (LICENSE-MIT)
    built_by_uv-0.1.0/README.md (README.md)
    built_by_uv-0.1.0/assets/data.csv (assets/data.csv)
    built_by_uv-0.1.0/header/built_by_uv.h (header/built_by_uv.h)
    built_by_uv-0.1.0/scripts/whoami.sh (scripts/whoami.sh)
    built_by_uv-0.1.0/src/built_by_uv/__init__.py (src/built_by_uv/__init__.py)
    built_by_uv-0.1.0/src/built_by_uv/arithmetic/__init__.py (src/built_by_uv/arithmetic/__init__.py)
    built_by_uv-0.1.0/src/built_by_uv/arithmetic/circle.py (src/built_by_uv/arithmetic/circle.py)
    built_by_uv-0.1.0/src/built_by_uv/arithmetic/pi.txt (src/built_by_uv/arithmetic/pi.txt)
    built_by_uv-0.1.0/src/built_by_uv/build-only.h (src/built_by_uv/build-only.h)
    built_by_uv-0.1.0/src/built_by_uv/cli.py (src/built_by_uv/cli.py)
    built_by_uv-0.1.0/third-party-licenses/PEP-401.txt (third-party-licenses/PEP-401.txt)
    Building built_by_uv-0.1.0-py3-none-any.whl will include the following files:
    built_by_uv/__init__.py (src/built_by_uv/__init__.py)
    built_by_uv/arithmetic/__init__.py (src/built_by_uv/arithmetic/__init__.py)
    built_by_uv/arithmetic/circle.py (src/built_by_uv/arithmetic/circle.py)
    built_by_uv/arithmetic/pi.txt (src/built_by_uv/arithmetic/pi.txt)
    built_by_uv/cli.py (src/built_by_uv/cli.py)
    built_by_uv-0.1.0.dist-info/licenses/LICENSE-APACHE (LICENSE-APACHE)
    built_by_uv-0.1.0.dist-info/licenses/LICENSE-MIT (LICENSE-MIT)
    built_by_uv-0.1.0.dist-info/licenses/third-party-licenses/PEP-401.txt (third-party-licenses/PEP-401.txt)
    built_by_uv-0.1.0.data/headers/built_by_uv.h (header/built_by_uv.h)
    built_by_uv-0.1.0.data/scripts/whoami.sh (scripts/whoami.sh)
    built_by_uv-0.1.0.data/data/data.csv (assets/data.csv)
    built_by_uv-0.1.0.dist-info/WHEEL (generated)
    built_by_uv-0.1.0.dist-info/entry_points.txt (generated)
    built_by_uv-0.1.0.dist-info/METADATA (generated)
    ");
    context
        .temp_dir
        .child("output2")
        .child("built_by_uv-0.1.0.tar.gz")
        .assert(predicate::path::missing());
    context
        .temp_dir
        .child("output2")
        .child("built_by_uv-0.1.0-py3-none-any.whl")
        .assert(predicate::path::missing());

    Ok(())
}

/// Test `--list` option errors.
#[test]
fn build_list_files_errors() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        // Normalize Windows workspace paths.
        .with_filter(("/crates/uv/../../", "/"));

    let built_by_uv = current_dir()?.join("../../test/packages/built-by-uv");

    uv_snapshot!(context.filters(), context.build()
        .arg(&built_by_uv)
        .arg("--out-dir")
        .arg(context.temp_dir.join("output1"))
        .arg("--list")
        .arg("--force-pep517"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: the argument '--list' cannot be used with '--force-pep517'

    Usage: uv build --cache-dir [CACHE_DIR] --out-dir <OUT_DIR> --exclude-newer <EXCLUDE_NEWER> <SRC>

    For more information, try '--help'.
    ");

    // Not a uv build backend package, we can't list it.
    let anyio_local = current_dir()?.join("../../test/packages/anyio_local");
    uv_snapshot!(context.filters(), context.build()
        .arg(&anyio_local)
        .arg("--out-dir")
        .arg(context.temp_dir.join("output2"))
        .arg("--list"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Failed to build `[WORKSPACE]/test/packages/anyio_local`
      cause: Can only use `--list` with a compatible uv build backend, but `[WORKSPACE]/test/packages/anyio_local` is not compatible because `build_system.build-backend` is not `uv_build`, but `flit_core.buildapi`
    ");
    Ok(())
}

#[test]
fn build_version_mismatch() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let anyio_local = current_dir()?.join("../../test/packages/anyio_local");
    context
        .build()
        .arg("--sdist")
        .arg("--out-dir")
        .arg(context.temp_dir.path())
        .arg(anyio_local)
        .assert()
        .success();
    let wrong_source_dist = context.temp_dir.child("anyio-1.2.3.tar.gz");
    fs_err::rename(
        context.temp_dir.child("anyio-4.3.0+foo.tar.gz"),
        &wrong_source_dist,
    )?;
    uv_snapshot!(context.filters(), context.build()
        .arg(wrong_source_dist.path())
        .arg("--wheel")
        .arg("--out-dir")
        .arg(context.temp_dir.path()), @"
    exit_code: 2 (failure)
    ----- stderr -----
    Building wheel from source distribution...
    error: Failed to build `[TEMP_DIR]/anyio-1.2.3.tar.gz`
      cause: The source distribution declares version 1.2.3, but the wheel declares version 4.3.0+foo
    ");
    Ok(())
}

/// A backend must not return an sdist and wheel for different projects.
#[test]
fn build_name_mismatch() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let project = context.temp_dir.child("project");
    project.child("pyproject.toml").write_str(indoc! {r#"
        [build-system]
        requires = []
        build-backend = "backend"
        backend-path = ["."]
    "#})?;
    project.child("backend.py").write_str(indoc! {r#"
        from pathlib import Path

        def build_sdist(sdist_directory, config_settings=None):
            filename = "alpha-1.0.0.tar.gz"
            Path(sdist_directory, filename).touch()
            return filename

        def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
            filename = "beta-1.0.0-py3-none-any.whl"
            Path(wheel_directory, filename).touch()
            return filename
    "#})?;

    uv_snapshot!(context.filters(), context.build().arg("--sdist").arg("--wheel").current_dir(&project), @"
    exit_code: 2 (failure)
    ----- stderr -----
    Building source distribution...
    Building wheel...
    error: Failed to build `[TEMP_DIR]/project`
      cause: The source distribution declares name alpha, but the wheel declares name beta
    ");

    Ok(())
}

#[cfg(unix)] // Symlinks aren't universally available on windows.
#[test]
fn build_with_symlink() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    context
        .temp_dir
        .child("pyproject.toml.real")
        .write_str(indoc! {r#"
            [project]
            name = "softlinked"
            version = "0.1.0"
            requires-python = ">=3.12"

            [build-system]
            requires = ["hatchling"]
            build-backend = "hatchling.build"
    "#})?;
    fs_err::os::unix::fs::symlink(
        "pyproject.toml.real",
        context.temp_dir.child("pyproject.toml"),
    )?;
    context
        .temp_dir
        .child("src/softlinked/__init__.py")
        .touch()?;
    fs_err::remove_dir_all(&context.venv)?;
    uv_snapshot!(context.filters(), context.build(), @"
    exit_code: 0 (success)
    ----- stderr -----
    Building source distribution...
    Building wheel from source distribution...
    Successfully built dist/softlinked-0.1.0.tar.gz
    Successfully built dist/softlinked-0.1.0-py3-none-any.whl
    ");
    Ok(())
}

/// This is bad project layout that is allowed: A project that defines PEP 621 metadata, but no
/// PEP 517 build system not a setup.py, so we fallback to setuptools implicitly.
#[test]
fn build_unconfigured_setuptools() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
            [project]
            name = "greet"
            version = "0.1.0"
    "#})?;
    context
        .temp_dir
        .child("src/greet/__init__.py")
        .write_str("print('Greetings!')")?;

    // This is not technically a `uv build` test, we use it to contrast this passing case with the
    // failing cases later.
    uv_snapshot!(context.filters(), context.pip_install().arg("."), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + greet==0.1.0 (from file://[TEMP_DIR]/)
    ");

    uv_snapshot!(context.filters(), context.python_command().arg("-c").arg("import greet"), @"
    exit_code: 0 (success)
    ----- stdout -----
    Greetings!
    ");
    Ok(())
}

/// In a project layout with a virtual root, an easy mistake to make is running `uv pip install .`
/// in the root.
#[test]
fn build_workspace_virtual_root() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
            [tool.uv.workspace]
            members = ["packages/*"]
    "#})?;

    uv_snapshot!(context.filters(), context.build().arg("--no-build-logs"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    Building source distribution...
    warning: `[TEMP_DIR]/` appears to be a workspace root without a Python project; consider using `uv sync` to install the workspace, or add a `[build-system]` table to `pyproject.toml`
    Building wheel from source distribution...
    error: Failed to build `[TEMP_DIR]/`
      cause: The source distribution declares name cache, but the wheel declares name unknown
    ");
    Ok(())
}

/// There is a `pyproject.toml`, but it does not define any build information nor is there a
/// `setup.{py,cfg}`.
#[test]
fn build_pyproject_toml_not_a_project() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {"
            # Some other content we don't know about
            [tool.black]
            line-length = 88
    "})?;

    uv_snapshot!(context.filters(), context.build().arg("--no-build-logs"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    Building source distribution...
    warning: `[TEMP_DIR]/` does not appear to be a Python project, as the `pyproject.toml` does not include a `[build-system]` table, and neither `setup.py` nor `setup.cfg` are present in the directory
    Building wheel from source distribution...
    error: Failed to build `[TEMP_DIR]/`
      cause: The source distribution declares name cache, but the wheel declares name unknown
    ");
    Ok(())
}

#[test]
fn build_with_nonnormalized_name() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_filter((r"\\\.", ""));

    let project = context.temp_dir.child("project");

    let pyproject_toml = project.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "my.PROJECT"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0"]

        [build-system]
        requires = ["setuptools>=42,<69"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    project
        .child("src")
        .child("my.PROJECT")
        .child("__init__.py")
        .touch()?;
    project.child("README").touch()?;

    // Build the specified path.
    uv_snapshot!(context.filters(), context.build().arg("--no-build-logs").current_dir(&project), @"
    exit_code: 0 (success)
    ----- stderr -----
    Building source distribution...
    Building wheel from source distribution...
    Successfully built dist/my.PROJECT-0.1.0.tar.gz
    Successfully built dist/my.PROJECT-0.1.0-py3-none-any.whl
    ");

    project
        .child("dist")
        .child("my.PROJECT-0.1.0.tar.gz")
        .assert(predicate::path::is_file());
    project
        .child("dist")
        .child("my.PROJECT-0.1.0-py3-none-any.whl")
        .assert(predicate::path::is_file());

    Ok(())
}

/// Check that `--force-pep517` is respected.
///
/// The error messages for a broken project are different for direct builds vs. PEP 517.
#[test]
fn force_pep517() -> Result<()> {
    // We need to use a real `uv_build` package.
    let context = uv_test::test_context!("3.12").with_exclude_newer("2025-05-27T00:00:00Z");

    context.init().assert().success();

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "1.0.0"

        [tool.uv.build-backend]
        module-name = "does_not_exist"

        [build-system]
        requires = ["uv_build>=0.5.15,<10000"]
        build-backend = "uv_build"
    "#})?;

    uv_snapshot!(context.filters(), context.build().env(EnvVars::RUST_BACKTRACE, "0"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    Building source distribution...
    error: Failed to build `[TEMP_DIR]/`
      cause: Expected a Python module at: src/does_not_exist/__init__.py
    ");

    uv_snapshot!(context.filters(), context.build().arg("--force-pep517").env(EnvVars::RUST_BACKTRACE, "0"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    Building source distribution...
    Error: Missing module directory for `does_not_exist` in `src`. Found: `temp`
    error: Failed to build `[TEMP_DIR]/`
      cause: The build backend returned an error
      cause: Call to `uv_build.build_sdist` failed (exit status: 1)

    hint: Build failures usually indicate a problem with the package or the build environment
    ");

    Ok(())
}

/// Check that we show a hint when there's a venv in the source distribution.
///
/// <https://github.com/astral-sh/uv/issues/15096>
/// <https://github.com/astral-sh/uv/issues/21128>
// Windows uses trampolines instead of symlinks. You don't want those in your source distribution
// either, but that's for the build backend to catch, we're only checking for the unix error hint
// in uv here.
#[cfg(unix)]
#[test]
fn venv_included_in_sdist() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_filter((r"at byte \d+", "at byte [OFFSET]"));

    context
        .init()
        .arg("--name")
        .arg("project")
        .arg("--build-backend")
        .arg("hatchling")
        .assert()
        .success();

    let pyproject_toml = indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12.0"

        [tool.hatch.build.targets.sdist.force-include]
        ".venv" = ".venv"

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
    "#};

    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(pyproject_toml)?;

    context.venv().arg("--clear").assert().success();

    // The default astral-tokio-tar backend recognizes the external virtual-environment link.
    uv_snapshot!(context.filters(), context.build(), @"
    exit_code: 2 (failure)
    ----- stderr -----
    Building source distribution...
    error: Failed to build `[TEMP_DIR]/`
      cause: Invalid tar file
      cause: failed to unpack `[CACHE_DIR]/sdists-v9/[TMP]/project-0.1.0/.venv/bin/python`
      cause: symlink path `[PYTHON-3.12]` is absolute, but external symlinks are not allowed

    hint: The source distribution includes a virtual environment. Virtual environments must be excluded from source distributions.
    ");

    // Point the virtual environment at the test interpreter's `python/3.12/python3` shim to
    // exercise a base interpreter outside `bin`, as in Gentoo's test layout.
    let venv_python = context.venv.child("bin").child("python");
    fs_err::remove_file(&venv_python)?;
    venv_python.symlink_to_file(context.root.child("python").child("3.12").child("python3"))?;

    // The preview tar-codec backend reports a structured unsafe-link error and preserves the same
    // user-facing hint, regardless of the base interpreter's installation layout.
    uv_snapshot!(context.filters(), context
        .build()
        .arg("--preview-features")
        .arg("tar-codec"), @r#"
    exit_code: 2 (failure)
    ----- stderr -----
    Building source distribution...
    error: Failed to build `[TEMP_DIR]/`
      cause: Invalid tar file
      cause: at byte [OFFSET]: unsafe symbolic-link target "[PYTHON-3.12]": is absolute

    hint: The source distribution includes a virtual environment. Virtual environments must be excluded from source distributions.
    "#);

    uv_snapshot!(context.filters(), context.build().arg("-q"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Failed to build `[TEMP_DIR]/`
      cause: Invalid tar file
      cause: failed to unpack `[CACHE_DIR]/sdists-v9/[TMP]/project-0.1.0/.venv/bin/python`
      cause: symlink path `[PYTHON-3.12]` is absolute, but external symlinks are not allowed

    hint: The source distribution includes a virtual environment. Virtual environments must be excluded from source distributions.
    ");

    uv_snapshot!(context.filters(), context.build().arg("-qq"), @"
    exit_code: 2 (failure)
    ");

    Ok(())
}

/// Ensure that workspace discovery works with and without trailing slash.
///
/// <https://github.com/astral-sh/uv/issues/13914>
#[test]
fn test_workspace_trailing_slash() {
    let context = uv_test::test_context!("3.12");

    // Create a workspace with a root and a member.
    context.init().arg("--lib").assert().success();
    context.init().arg("--lib").arg("child").assert().success();

    uv_snapshot!(context.filters(), context.build().arg("child"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Building source distribution...
    Building wheel from source distribution...
    Successfully built dist/child-0.1.0.tar.gz
    Successfully built dist/child-0.1.0-py3-none-any.whl
    ");

    // Check that workspace discovery still works.
    uv_snapshot!(context.filters(), context.build().arg("child/"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Building source distribution...
    Building wheel from source distribution...
    Successfully built dist/child-0.1.0.tar.gz
    Successfully built dist/child-0.1.0-py3-none-any.whl
    ");

    // Check general normalization too.
    uv_snapshot!(context.filters(), context.build().arg("./child/"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Building source distribution...
    Building wheel from source distribution...
    Successfully built dist/child-0.1.0.tar.gz
    Successfully built dist/child-0.1.0-py3-none-any.whl
    ");

    uv_snapshot!(context.filters(), context.build().arg("./child/../child/"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Building source distribution...
    Building wheel from source distribution...
    Successfully built dist/child-0.1.0.tar.gz
    Successfully built dist/child-0.1.0-py3-none-any.whl
    ");
}

/// Test `uv build --clear`.
#[test]
fn build_clear() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let project = context.temp_dir.child("project");

    context.init().arg(project.path()).assert().success();

    // Regular build
    uv_snapshot!(&context.filters(), context.build().arg("project").arg("--no-build-logs"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Building source distribution...
    Building wheel from source distribution...
    Successfully built project/dist/project-0.1.0.tar.gz
    Successfully built project/dist/project-0.1.0-py3-none-any.whl
    ");

    project
        .child("dist")
        .child("project-0.1.0.tar.gz")
        .assert(predicate::path::is_file());
    project
        .child("dist")
        .child("project-0.1.0-py3-none-any.whl")
        .assert(predicate::path::is_file());

    // Add a marker file to verify `--clear` removes it
    fs_err::write(project.child("dist").child("marker.txt"), "marker")?;
    project
        .child("dist")
        .child("marker.txt")
        .assert(predicate::path::is_file());

    // Build with `--clear` to remove the marker file
    uv_snapshot!(&context.filters(), context.build().arg("project").arg("--clear").arg("--no-build-logs"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Building source distribution...
    Building wheel from source distribution...
    Successfully built project/dist/project-0.1.0.tar.gz
    Successfully built project/dist/project-0.1.0-py3-none-any.whl
    ");

    project
        .child("dist")
        .child("marker.txt")
        .assert(predicate::path::missing());
    project
        .child("dist")
        .child("project-0.1.0.tar.gz")
        .assert(predicate::path::is_file());
    project
        .child("dist")
        .child("project-0.1.0-py3-none-any.whl")
        .assert(predicate::path::is_file());

    Ok(())
}

/// Test `uv build --no-create-gitignore`.
#[test]
fn build_no_gitignore() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let project = context.temp_dir.child("project");

    context.init().arg(project.path()).assert().success();

    // Default build with `.gitignore`
    uv_snapshot!(&context.filters(), context.build().arg("project").arg("--no-build-logs"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Building source distribution...
    Building wheel from source distribution...
    Successfully built project/dist/project-0.1.0.tar.gz
    Successfully built project/dist/project-0.1.0-py3-none-any.whl
    ");

    project
        .child("dist")
        .child(".gitignore")
        .assert(predicate::path::is_file());

    fs_err::remove_dir_all(project.child("dist"))?;

    // Build with `--no-create-gitignore` that does not create `.gitignore`
    uv_snapshot!(&context.filters(), context.build().arg("project").arg("--no-create-gitignore").arg("--no-build-logs"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Building source distribution...
    Building wheel from source distribution...
    Successfully built project/dist/project-0.1.0.tar.gz
    Successfully built project/dist/project-0.1.0-py3-none-any.whl
    ");

    project
        .child("dist")
        .child(".gitignore")
        .assert(predicate::path::missing());

    Ok(())
}

#[test]
fn build_workspace_constraint_hashes() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let mut build_hash = String::new();
    for (name, version) in [("build-dependency", "1.0.0"), ("project", "0.1.0")] {
        let (filename, wheel) = generate_wheel(
            &name.parse()?,
            &version.parse()?,
            &[],
            &BTreeMap::new(),
            None,
            "py3-none-any",
            &[],
        );
        if name == "build-dependency" {
            build_hash = hex::encode(Sha256::digest(&wheel));
        }
        context
            .temp_dir
            .child("wheels")
            .child(filename)
            .write_binary(&wheel)?;
    }
    let context = context.with_filter((build_hash.clone(), "[BUILD_HASH]"));
    context.temp_dir.child("backend.py").write_str(indoc! {r#"
        import shutil
        from pathlib import Path

        import build_dependency

        Path(__file__).with_name("backend-executed").touch()

        def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
            source = Path(__file__).parent / "wheels" / "project-0.1.0-py3-none-any.whl"
            shutil.copyfile(source, Path(wheel_directory) / source.name)
            return source.name
    "#})?;
    let pyproject = context.temp_dir.child("pyproject.toml");
    let constraints = context.temp_dir.child("constraints.txt");
    let incorrect_hash = "0".repeat(64);
    pyproject.write_str(&formatdoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["build-dependency==1.0.0"]
        build-backend = "backend"
        backend-path = ["."]

        [tool.uv]
        no-index = true
        find-links = ["wheels"]
        build-constraint-dependencies = [
            {{ requirement = "build-dependency==1.0.0", hashes = ["sha256:{incorrect_hash}"] }},
        ]
    "#})?;
    constraints.write_str(&format!(
        "build-dependency==1.0.0 --hash=sha256:{build_hash}\n"
    ))?;

    // Workspace hashes are checked even without command-line constraints.
    uv_snapshot!(context.filters(), context.build()
        .arg("--wheel")
        .arg("--no-cache"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    Building wheel...
    error: Failed to build `[TEMP_DIR]/`
      cause: Failed to install requirements from `build-system.requires`
      cause: Failed to download `build-dependency==1.0.0`
      cause: Hash mismatch for `build-dependency==1.0.0`

             Expected:
               sha256:0000000000000000000000000000000000000000000000000000000000000000

             Computed:
               sha256:[BUILD_HASH]
    ");
    context
        .temp_dir
        .child("backend-executed")
        .assert(predicate::path::missing());

    // Workspace constraints follow command-line constraints, so their hashes take precedence.
    uv_snapshot!(context.filters(), context.build()
        .arg("--wheel")
        .arg("--no-cache")
        .args(["--build-constraint", "constraints.txt"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    Building wheel...
    error: Failed to build `[TEMP_DIR]/`
      cause: Failed to install requirements from `build-system.requires`
      cause: Failed to download `build-dependency==1.0.0`
      cause: Hash mismatch for `build-dependency==1.0.0`

             Expected:
               sha256:0000000000000000000000000000000000000000000000000000000000000000

             Computed:
               sha256:[BUILD_HASH]
    ");
    context
        .temp_dir
        .child("backend-executed")
        .assert(predicate::path::missing());

    // An explicit opt-out applies to hashes in both workspace and command-line constraints.
    uv_snapshot!(context.filters(), context.build()
        .arg("--wheel")
        .arg("--no-cache")
        .args(["--build-constraint", "constraints.txt", "--no-verify-hashes"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    Building wheel...
    Successfully built dist/project-0.1.0-py3-none-any.whl
    ");
    fs_err::remove_file(context.temp_dir.child("backend-executed"))?;

    let registry_pyproject = context.read("pyproject.toml");
    let wheel = context
        .temp_dir
        .child("wheels/build_dependency-1.0.0-py3-none-any.whl");
    let wheel_url =
        Url::from_file_path(wheel.path()).map_err(|()| anyhow!("invalid wheel path"))?;
    let requirement = format!("build-dependency @ {wheel_url}");
    pyproject.write_str(&registry_pyproject.replace(
        "requirement = \"build-dependency==1.0.0\"",
        &format!("requirement = \"{requirement}\""),
    ))?;
    constraints.write_str(&format!("{requirement} --hash=sha256:{build_hash}\n"))?;
    uv_snapshot!(context.filters(), context.build()
        .arg("--wheel")
        .arg("--no-cache")
        .args(["--build-constraint", "constraints.txt"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    Building wheel...
    error: Failed to build `[TEMP_DIR]/`
      cause: Failed to install requirements from `build-system.requires`
      cause: Failed to read `build-dependency @ file://[TEMP_DIR]/wheels/build_dependency-1.0.0-py3-none-any.whl`
      cause: Hash mismatch for `build-dependency @ file://[TEMP_DIR]/wheels/build_dependency-1.0.0-py3-none-any.whl`

             Expected:
               sha256:0000000000000000000000000000000000000000000000000000000000000000

             Computed:
               sha256:[BUILD_HASH]
    ");
    context
        .temp_dir
        .child("backend-executed")
        .assert(predicate::path::missing());

    // A correct workspace hash also takes precedence over an incorrect command-line hash.
    constraints.write_str(&format!(
        "build-dependency==1.0.0 --hash=sha256:{incorrect_hash}\n"
    ))?;
    pyproject.write_str(&registry_pyproject.replace(&incorrect_hash, &build_hash))?;
    uv_snapshot!(context.filters(), context.build()
        .arg("--wheel")
        .arg("--no-cache")
        .args(["--build-constraint", "constraints.txt"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    Building wheel...
    Successfully built dist/project-0.1.0-py3-none-any.whl
    ");
    context
        .temp_dir
        .child("backend-executed")
        .assert(predicate::path::exists());
    Ok(())
}

/// Lock validation must apply the build command's constraints before running a workspace backend.
#[tokio::test]
async fn build_packaged_lock_respects_build_hashes() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let (filename, wheel) = generate_wheel(
        &"build-dependency".parse()?,
        &"1.0.0".parse()?,
        &[],
        &BTreeMap::new(),
        None,
        "py3-none-any",
        &[],
    );
    let hash = hex::encode(Sha256::digest(&wheel));
    context
        .temp_dir
        .child("wheels")
        .child(&filename)
        .write_binary(&wheel)?;
    context.temp_dir.child("src/root/__init__.py").touch()?;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "root"
        version = "1.0.0"
        requires-python = ">=3.12"
        [build-system]
        requires = ["uv_build>=0.5.15,<2"]
        build-backend = "uv_build"
        [tool.uv]
        no-index = true
        find-links = ["wheels"]
        [tool.uv.workspace]
        members = ["sibling"]
    "#})?;
    let sibling = context.temp_dir.child("sibling");
    let pyproject = sibling.child("pyproject.toml");
    pyproject.write_str(indoc! {r#"
        [project]
        name = "sibling"
        version = "1.0.0"
        requires-python = ">=3.12"
        dynamic = ["dependencies"]
        [build-system]
        requires = ["build-dependency==1.0.0"]
        build-backend = "backend"
        backend-path = ["."]
    "#})?;
    let backend = sibling.child("backend.py");
    let backend_code = indoc! {r#"
        import build_dependency
        from pathlib import Path

        Path(__file__).with_name("backend-executed").touch()

        def prepare_metadata_for_build_wheel(metadata_directory, config_settings=None):
            target = Path(metadata_directory) / "sibling-1.0.0.dist-info"
            target.mkdir()
            (target / "METADATA").write_text(
                "Metadata-Version: 2.4\nName: sibling\nVersion: 1.0.0\nRequires-Python: >=3.12\n"
            )
            return target.name

        prepare_metadata_for_build_editable = prepare_metadata_for_build_wheel
    "#};
    backend.write_str(backend_code)?;
    context.lock().arg("--offline").assert().success();
    let marker = sibling.child("backend-executed");
    marker.assert(predicate::path::is_file());
    fs_err::remove_file(marker.path())?;
    pyproject.write_str(&format!(
        "{}\n# Refresh metadata\n",
        fs_err::read_to_string(pyproject.path())?
    ))?;
    context
        .temp_dir
        .child("constraints.txt")
        .write_str(&format!(
            "build-dependency==1.0.0 --hash=sha256:{}\n",
            "0".repeat(64)
        ))?;

    // Automatic export fails if the lock cannot be checked with the supplied hashes.
    context
        .build()
        .args([
            "--wheel",
            "--preview-features",
            "locked-tools",
            "--require-hashes",
            "--build-constraint",
            "constraints.txt",
        ])
        .assert()
        .failure();
    marker.assert(predicate::path::missing());

    // Explicit export fails instead of running a backend that violates the hash policy.
    context
        .build()
        .env(EnvVars::UV_EXPORT_LOCK, "true")
        .args([
            "--wheel",
            "--preview-features",
            "locked-tools",
            "--require-hashes",
            "--build-constraint",
            "constraints.txt",
        ])
        .assert()
        .failure();
    marker.assert(predicate::path::missing());

    // Version constraints also apply when hash verification is disabled.
    context
        .temp_dir
        .child("constraints.txt")
        .write_str("build-dependency==2.0.0\n")?;
    context
        .build()
        .env(EnvVars::UV_EXPORT_LOCK, "true")
        .args([
            "--wheel",
            "--preview-features",
            "locked-tools",
            "--no-verify-hashes",
            "--build-constraint",
            "constraints.txt",
        ])
        .assert()
        .failure();
    marker.assert(predicate::path::missing());

    // A hash from the runtime lock cannot authorize an unlisted build dependency.
    let server = PackageServer::new(&"build-dependency".parse()?).await;
    server.serve(&filename, &wheel, Some(&hash)).await;
    let root_pyproject = context.temp_dir.child("pyproject.toml");
    root_pyproject.write_str(&fs_err::read_to_string(root_pyproject.path())?.replace(
        "no-index = true\nfind-links = [\"wheels\"]",
        &format!("index-url = \"{}\"", server.index_url()),
    ))?;
    backend.write_str(&backend_code.replace(
        "Requires-Python: >=3.12\\n\"",
        "Requires-Python: >=3.12\\nRequires-Dist: build-dependency==1.0.0\\n\"",
    ))?;
    context.lock().assert().success();
    let lock =
        uv_lock::Lock::from_toml(&fs_err::read_to_string(context.temp_dir.child("uv.lock"))?)?;
    let hasher = lock.hash_strategy(context.temp_dir.path(), &FxHashSet::default())?;
    let uv_types::HashVerification::IfPresent(hashes) = hasher.verification() else {
        return Err(anyhow!("expected the runtime lock to contain hashes"));
    };
    assert!(
        hashes.contains_key(&uv_distribution_types::VersionId::from_registry(
            "build-dependency".parse()?,
            "1.0.0".parse()?,
        ))
    );
    fs_err::remove_file(marker.path())?;
    pyproject.write_str(&format!(
        "{}\n# Refresh again\n",
        fs_err::read_to_string(pyproject.path())?
    ))?;
    context.temp_dir.child("constraints.txt").write_str("")?;
    context
        .build()
        .env(EnvVars::UV_EXPORT_LOCK, "true")
        .args([
            "--wheel",
            "--preview-features",
            "locked-tools",
            "--require-hashes",
            "--build-constraint",
            "constraints.txt",
        ])
        .assert()
        .failure();
    marker.assert(predicate::path::missing());

    // The same metadata can be checked when the build dependency is explicitly authorized.
    context
        .temp_dir
        .child("constraints.txt")
        .write_str(&format!("build-dependency==1.0.0 --hash=sha256:{hash}\n"))?;
    context
        .build()
        .env(EnvVars::UV_EXPORT_LOCK, "true")
        .args([
            "--wheel",
            "--preview-features",
            "locked-tools",
            "--require-hashes",
            "--build-constraint",
            "constraints.txt",
        ])
        .assert()
        .success();
    marker.assert(predicate::path::is_file());
    assert!(
        zip_file_names(&context.temp_dir.child("dist/root-1.0.0-py3-none-any.whl"))?
            .contains(&"root-1.0.0.dist-info/pylock.toml".to_string())
    );
    Ok(())
}

fn write_uv_build_backend(context: &TestContext) -> Result<()> {
    context.temp_dir.child("backend/uv_build.py").write_str(indoc! {r#"
        import os, subprocess
        def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
            return subprocess.check_output([os.environ["TEST_UV_BIN"], "build-backend", "build-wheel", wheel_directory], text=True).strip()
        def build_sdist(sdist_directory, config_settings=None):
            return subprocess.check_output([os.environ["TEST_UV_BIN"], "build-backend", "build-sdist", sdist_directory], text=True).strip()
    "#})?;
    Ok(())
}

fn build_with_uv_build(context: &TestContext) -> Command {
    let mut command = context.build();
    command
        .env("PYTHONPATH", context.temp_dir.child("backend").path())
        .env("TEST_UV_BIN", get_bin!());
    command
}

/// The frontend packages the lock in both distributions and can use it when rebuilding a wheel.
#[test]
fn build_with_packaged_lock() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    context
        .temp_dir
        .child("src/locked_tool/__init__.py")
        .touch()?;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "locked-tool"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = ["ignored>=1,<2; python_version < '3'"]

        [project.optional-dependencies]
        z = []
        a = []

        [build-system]
        requires = ["uv_build>=0.5.15,<2"]
        build-backend = "uv_build"

        [tool.uv]
        resolution = "lowest"
    "#})?;
    context.lock().arg("--offline").assert().success();
    let pyproject = context.temp_dir.child("pyproject.toml");
    pyproject.write_str(
        &fs_err::read_to_string(pyproject.path())?.replace("z = []\na = []", "a = []\nz = []"),
    )?;

    context.build().arg("--wheel").assert().success();
    assert!(
        !zip_file_names(
            &context
                .temp_dir
                .child("dist/locked_tool-1.0.0-py3-none-any.whl")
        )?
        .contains(&"locked_tool-1.0.0.dist-info/pylock.toml".to_string())
    );
    context
        .build()
        .args(["--preview-features", "locked-tools"])
        .assert()
        .success();
    uv_snapshot!(context.filters(), context.build().args(["--preview-features", "locked-tools", "--wheel", "--list"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    Building locked_tool-1.0.0-py3-none-any.whl will include the following files:
    locked_tool/__init__.py (src/locked_tool/__init__.py)
    locked_tool-1.0.0.dist-info/WHEEL (generated)
    locked_tool-1.0.0.dist-info/METADATA (generated)
    locked_tool-1.0.0.dist-info/pylock.toml (generated)
    ");
    uv_snapshot!(context.python_command().arg("-c").arg(indoc! {r#"
        import base64, csv, hashlib, io, pathlib, tarfile, tomllib, zipfile
        sdist = pathlib.Path("dist/locked_tool-1.0.0.tar.gz")
        with tarfile.open(sdist) as archive:
            print("source lock:", "locked_tool-1.0.0/pylock.toml" in archive.getnames() and "locked_tool-1.0.0/uv.lock" not in archive.getnames())
            print("regular file:", archive.getmember("locked_tool-1.0.0/pylock.toml").type == tarfile.REGTYPE)
        wheel_path = pathlib.Path("dist/locked_tool-1.0.0-py3-none-any.whl")
        with zipfile.ZipFile(wheel_path) as wheel:
            lock_path = "locked_tool-1.0.0.dist-info/pylock.toml"
            lock = wheel.read(lock_path)
            record = list(csv.reader(io.StringIO(wheel.read("locked_tool-1.0.0.dist-info/RECORD").decode())))
            digest = base64.urlsafe_b64encode(hashlib.sha256(lock).digest()).rstrip(b"=").decode()
            print("record:", [lock_path, "sha256=" + digest, str(len(lock))] in record)
            print("valid wheel:", wheel.testzip() is None)
            parsed = tomllib.loads(lock.decode())
            print("standard lock:", parsed)
    "#}), @r#"
    exit_code: 0 (success)
    ----- stdout -----
    source lock: True
    regular file: True
    record: True
    valid wheel: True
    standard lock: {'lock-version': '1.0', 'created-by': 'uv', 'requires-python': '>=3.12', 'packages': []}
    "#);
    // Use the local backend shim so rebuilding the sdist does not require an index.
    write_uv_build_backend(&context)?;
    build_with_uv_build(&context)
        .args([
            "--preview-features",
            "locked-tools",
            "--no-build-isolation",
            "--wheel",
            "--out-dir",
            "rebuilt",
            "dist/locked_tool-1.0.0.tar.gz",
        ])
        .assert()
        .success();
    assert!(
        zip_file_names(
            &context
                .temp_dir
                .child("rebuilt/locked_tool-1.0.0-py3-none-any.whl")
        )?
        .contains(&"locked_tool-1.0.0.dist-info/pylock.toml".to_string())
    );
    build_with_uv_build(&context)
        .env(EnvVars::UV_EXPORT_LOCK, "false")
        .args([
            "--offline",
            "--preview-features",
            "locked-tools",
            "--no-build-isolation",
            "--wheel",
            "--out-dir",
            "disabled",
            "dist/locked_tool-1.0.0.tar.gz",
        ])
        .assert()
        .success();
    assert!(
        !zip_file_names(
            context
                .temp_dir
                .child("disabled/locked_tool-1.0.0-py3-none-any.whl")
                .path()
        )?
        .contains(&"locked_tool-1.0.0.dist-info/pylock.toml".to_string())
    );
    context.python_command().arg("-c").arg(indoc! {r#"
        import io, pathlib, stat, tarfile, zipfile
        source = pathlib.Path("dist/locked_tool-1.0.0.tar.gz")
        for variant in ("private", "comment", "duplicate", "missing", "index", "artifact", "pypi", "directory", "archive", "archive-index", "missing-index", "missing-url", "missing-hash"):
            pathlib.Path(variant).mkdir()
            with tarfile.open(source) as src, tarfile.open(f"{variant}/{source.name}", "w:gz") as dst:
                for entry in src:
                    if entry.name.endswith("/pylock.toml") and variant == "missing":
                        continue
                    data = src.extractfile(entry).read() if entry.isfile() else None
                    if entry.name.endswith("/pylock.toml") and variant == "private":
                        data += b'\nunknown = "https://private.example/secret"\n'
                    if entry.name.endswith("/pylock.toml") and variant == "comment":
                        data += b'\n# https://private.example/secret\n'
                    if entry.name.endswith("/pylock.toml") and variant in ("index", "artifact", "pypi"):
                        index = "https://pypi.org/other" if variant == "index" else "https://pypi.org/simple"
                        host = "pypi.org" if variant == "artifact" else "files.pythonhosted.org"
                        data = data.replace(b"packages = []", b"")
                        if variant == "pypi":
                            data = data.replace(b'requires-python = ">=3.12"\n', b'')
                        data += (f'\n[[packages]]\nname = "example"\nversion = "1.0.0"\nindex = "{index}"\n'
                                 f'wheels = [{{ url = "https://{host}/packages/example-1.0.0-py3-none-any.whl", hashes = {{ sha256 = "{"0" * 64}" }} }}]\n').encode()
                    if entry.name.endswith("/pylock.toml") and variant in ("directory", "archive", "archive-index", "missing-index", "missing-url", "missing-hash"):
                        data = data.replace(b"packages = []", b"")
                        if variant == "directory":
                            package_source = 'directory = { path = "example" }'
                        elif variant == "archive":
                            package_source = f'archive = {{ url = "https://private.example/example-1.0.0-py3-none-any.whl", hashes = {{ sha256 = "{"0" * 64}" }} }}'
                        elif variant == "archive-index":
                            package_source = f'index = "https://pypi.org/simple"\narchive = {{ url = "https://files.pythonhosted.org/packages/example-1.0.0-py3-none-any.whl", hashes = {{ sha256 = "{"0" * 64}" }} }}'
                        elif variant == "missing-index":
                            package_source = f'wheels = [{{ url = "https://files.pythonhosted.org/packages/example-1.0.0-py3-none-any.whl", hashes = {{ sha256 = "{"0" * 64}" }} }}]'
                        elif variant == "missing-url":
                            package_source = f'index = "https://pypi.org/simple"\nwheels = [{{ path = "example-1.0.0-py3-none-any.whl", hashes = {{ sha256 = "{"0" * 64}" }} }}]'
                        else:
                            package_source = 'index = "https://pypi.org/simple"\nwheels = [{ url = "https://files.pythonhosted.org/packages/example-1.0.0-py3-none-any.whl", hashes = {} }]'
                        data += f'\n[[packages]]\nname = "example"\nversion = "1.0.0"\n{package_source}\n'.encode()
                    if entry.name.endswith("/PKG-INFO") and variant == "comment":
                        assert b"python_full_version < '3'" in data
                        data = data.replace(b"python_full_version < '3'", b"python_full_version < '3.1'")
                    if data is not None:
                        entry.size = len(data)
                    dst.addfile(entry, io.BytesIO(data) if data is not None else None)
                    if entry.name.endswith("/pylock.toml") and variant == "duplicate":
                        dst.addfile(entry, io.BytesIO(data))
        pathlib.Path("formats").mkdir()
        for extension, mode in (("tar", "w"), ("tgz", "w:gz")):
            with tarfile.open("dist/locked_tool-1.0.0.tar.gz") as src, tarfile.open(f"formats/locked_tool-1.0.0.{extension}", mode) as dst:
                for entry in src:
                    data = src.extractfile(entry).read() if entry.isfile() else None
                    dst.addfile(entry, io.BytesIO(data) if data is not None else None)
        for variant in ("normal", "duplicate", "symlink"):
            pathlib.Path(f"formats/{variant}").mkdir()
            with tarfile.open("dist/locked_tool-1.0.0.tar.gz") as src, zipfile.ZipFile(f"formats/{variant}/locked_tool-1.0.0.zip", "w") as dst:
                for entry in src:
                    if not entry.isfile():
                        continue
                    data = src.extractfile(entry).read()
                    info = zipfile.ZipInfo(entry.name)
                    info.create_system = 3
                    info.external_attr = ((stat.S_IFLNK if variant == "symlink" and entry.name.endswith("/PKG-INFO") else stat.S_IFREG) | 0o644) << 16
                    dst.writestr(info, data)
                    if variant == "duplicate" and entry.name.endswith("/pylock.toml"):
                        duplicate = zipfile.ZipInfo(entry.name)
                        duplicate.create_system = info.create_system
                        duplicate.external_attr = info.external_attr
                        dst.writestr(duplicate, data)
    "#}).assert().success();
    build_with_uv_build(&context)
        .args([
            "--offline",
            "--preview-features",
            "locked-tools",
            "--no-build-isolation",
            "--wheel",
            "--out-dir",
            "private-wheel",
            "private/locked_tool-1.0.0.tar.gz",
        ])
        .assert()
        .success();
    assert!(
        !zip_file_names(
            context
                .temp_dir
                .child("private-wheel/locked_tool-1.0.0-py3-none-any.whl")
                .path()
        )?
        .contains(&"locked_tool-1.0.0.dist-info/pylock.toml".to_string())
    );
    build_with_uv_build(&context)
        .args([
            "--offline",
            "--preview-features",
            "locked-tools",
            "--no-build-isolation",
            "--wheel",
            "--out-dir",
            "comment-wheel",
            "comment/locked_tool-1.0.0.tar.gz",
        ])
        .assert()
        .success();
    context
        .python_command()
        .arg("-c")
        .arg(indoc! {r#"
        import zipfile
        with zipfile.ZipFile("comment-wheel/locked_tool-1.0.0-py3-none-any.whl") as wheel:
            contents = wheel.read("locked_tool-1.0.0.dist-info/pylock.toml")
            assert b"private.example" not in contents
    "#})
        .assert()
        .success();
    uv_snapshot!(context.filters(), build_with_uv_build(&context)
        .args(["--offline", "--preview-features", "locked-tools", "--no-build-isolation", "--wheel", "--out-dir", "missing-wheel", "missing/locked_tool-1.0.0.tar.gz"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Failed to build `[TEMP_DIR]/missing/locked_tool-1.0.0.tar.gz`
      cause: Failed to package the project lock
      cause: Cannot export a lock: the source distribution has no `pylock.toml`
    ");
    build_with_uv_build(&context)
        .env(EnvVars::UV_EXPORT_LOCK, "false")
        .args([
            "--offline",
            "--preview-features",
            "locked-tools",
            "--no-build-isolation",
            "--wheel",
            "--out-dir",
            "missing-disabled",
            "missing/locked_tool-1.0.0.tar.gz",
        ])
        .assert()
        .success();
    for variant in ["index", "artifact", "directory", "archive", "archive-index"] {
        let source = format!("{variant}/locked_tool-1.0.0.tar.gz");
        let output = format!("{variant}-wheel");
        build_with_uv_build(&context)
            .args([
                "--offline",
                "--preview-features",
                "locked-tools",
                "--no-build-isolation",
                "--wheel",
                "--out-dir",
                &output,
                &source,
            ])
            .assert()
            .success();
        let has_lock = zip_file_names(
            context
                .temp_dir
                .child(&output)
                .child("locked_tool-1.0.0-py3-none-any.whl")
                .path(),
        )?
        .contains(&"locked_tool-1.0.0.dist-info/pylock.toml".to_string());
        assert_eq!(has_lock, variant == "artifact");
    }
    for variant in [
        "directory",
        "archive",
        "archive-index",
        "missing-index",
        "missing-url",
        "missing-hash",
    ] {
        build_with_uv_build(&context)
            .env(EnvVars::UV_EXPORT_LOCK, "true")
            .args([
                "--offline",
                "--preview-features",
                "locked-tools",
                "--no-build-isolation",
                "--wheel",
                "--out-dir",
                &format!("{variant}-explicit"),
                &format!("{variant}/locked_tool-1.0.0.tar.gz"),
            ])
            .assert()
            .failure()
            .stderr(predicate::str::contains(
                "The source distribution has an unsupported `pylock.toml`",
            ));
    }
    for variant in ["missing-index", "missing-url", "missing-hash"] {
        build_with_uv_build(&context)
            .args([
                "--offline",
                "--preview-features",
                "locked-tools",
                "--no-build-isolation",
                "--wheel",
                "--out-dir",
                &format!("{variant}-automatic"),
                &format!("{variant}/locked_tool-1.0.0.tar.gz"),
            ])
            .assert()
            .failure()
            .stderr(predicate::str::contains(
                "The source distribution has an unsupported `pylock.toml`",
            ));
    }
    let tar = fs_err::read(context.temp_dir.child("formats/locked_tool-1.0.0.tar"))?;
    let compressed = block_on(async {
        let mut encoder = async_compression::tokio::write::ZstdEncoder::new(Vec::new());
        encoder.write_all(&tar).await?;
        encoder.shutdown().await?;
        Ok::<_, std::io::Error>(encoder.into_inner())
    })?;
    fs_err::write(
        context.temp_dir.child("formats/locked_tool-1.0.0.tar.zst"),
        compressed,
    )?;
    for source in [
        "formats/locked_tool-1.0.0.tar",
        "formats/locked_tool-1.0.0.tgz",
        "formats/locked_tool-1.0.0.tar.zst",
        "formats/normal/locked_tool-1.0.0.zip",
    ] {
        build_with_uv_build(&context)
            .args([
                "--offline",
                "--preview-features",
                "locked-tools",
                "--no-build-isolation",
                "--wheel",
                "--out-dir",
                "formats-wheel",
                source,
            ])
            .assert()
            .success();
        assert!(
            zip_file_names(
                context
                    .temp_dir
                    .child("formats-wheel/locked_tool-1.0.0-py3-none-any.whl")
                    .path()
            )?
            .contains(&"locked_tool-1.0.0.dist-info/pylock.toml".to_string())
        );
    }
    for (variant, error) in [
        ("duplicate", "invalid or duplicate pylock.toml file"),
        ("symlink", "invalid or duplicate PKG-INFO file"),
    ] {
        build_with_uv_build(&context)
            .args([
                "--offline",
                "--preview-features",
                "locked-tools",
                "--no-build-isolation",
                "--wheel",
                "--out-dir",
                "formats-wheel",
                &format!("formats/{variant}/locked_tool-1.0.0.zip"),
            ])
            .assert()
            .failure()
            .stderr(predicate::str::contains(error));
    }
    build_with_uv_build(&context)
        .env(EnvVars::UV_EXPORT_LOCK, "true")
        .args([
            "--offline",
            "--preview-features",
            "locked-tools",
            "--no-build-isolation",
            "--wheel",
            "--out-dir",
            "duplicate-wheel",
            "duplicate/locked_tool-1.0.0.tar.gz",
        ])
        .assert()
        .failure();
    assert!(
        !context
            .temp_dir
            .child("duplicate-wheel/locked_tool-1.0.0-py3-none-any.whl")
            .path()
            .is_file()
    );
    build_with_uv_build(&context)
        .args([
            "--offline",
            "--preview-features",
            "locked-tools",
            "--no-build-isolation",
            "--wheel",
            "--out-dir",
            "pypi-wheel",
            "pypi/locked_tool-1.0.0.tar.gz",
        ])
        .assert()
        .success();
    context
        .python_command()
        .arg("-c")
        .arg(indoc! {r#"
            import tomllib, zipfile
            with zipfile.ZipFile("pypi-wheel/locked_tool-1.0.0-py3-none-any.whl") as wheel:
                lock = tomllib.loads(wheel.read("locked_tool-1.0.0.dist-info/pylock.toml").decode())
                assert "tool" not in lock
                assert "requires-python" not in lock
                assert [package["name"] for package in lock["packages"]] == ["example"]
        "#})
        .assert()
        .success();
    fs_err::write(
        context.temp_dir.child("src/locked_tool/large.bin"),
        vec![0; 16 * 1024 * 1024 + 1],
    )?;
    build_with_uv_build(&context)
        .args([
            "--preview-features",
            "locked-tools",
            "--force-pep517",
            "--no-build-isolation",
            "--wheel",
            "--out-dir",
            "forced",
        ])
        .assert()
        .success();
    context
        .python_command()
        .arg("-c")
        .arg(indoc! {r#"
        import zipfile
        with zipfile.ZipFile("forced/locked_tool-1.0.0-py3-none-any.whl") as wheel:
            assert wheel.read("locked_tool/large.bin") == bytes(16 * 1024 * 1024 + 1)
            assert "locked_tool-1.0.0.dist-info/pylock.toml" in wheel.namelist()
    "#})
        .assert()
        .success();
    context
        .build()
        .current_dir(context.home_dir.path())
        .env(EnvVars::UV_EXPORT_LOCK, "true")
        .args(["--preview-features", "locked-tools", "--wheel"])
        .arg(context.temp_dir.join("../temp"))
        .assert()
        .success();
    assert!(
        zip_file_names(
            &context
                .temp_dir
                .child("dist/locked_tool-1.0.0-py3-none-any.whl")
        )?
        .contains(&"locked_tool-1.0.0.dist-info/pylock.toml".to_string())
    );

    Ok(())
}

/// Automatically exported locks must not include unchecked URLs.
#[test]
fn build_packaged_lock_unchecked_contents() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    context
        .temp_dir
        .child("src/locked_tool/__init__.py")
        .touch()?;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "locked-tool"
        version = "1.0.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["uv_build>=0.5.15,<2"]
        build-backend = "uv_build"

        [tool.uv.build-backend]
        source-include = ["uv.lock", "PYLOCK.TOML"]
    "#})?;
    context.lock().arg("--offline").assert().success();
    context
        .temp_dir
        .child("PYLOCK.TOML")
        .write_str("untrusted source lock")?;
    uv_snapshot!(context.filters(), context.build().args(["--offline", "--preview-features", "locked-tools", "--sdist", "--list"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    Building locked_tool-1.0.0.tar.gz will include the following files:
    locked_tool-1.0.0/PKG-INFO (generated)
    locked_tool-1.0.0/pyproject.toml (generated)
    locked_tool-1.0.0/pyproject.toml.orig (pyproject.toml)
    locked_tool-1.0.0/src/locked_tool/__init__.py (src/locked_tool/__init__.py)
    locked_tool-1.0.0/uv.lock (uv.lock)
    locked_tool-1.0.0/pylock.toml (generated)
    ");
    let path = context.temp_dir.child("uv.lock");
    let original = fs_err::read_to_string(path.path())?;
    path.write_str(&format!("unknown = \"https://private.example/field\"\n{original}\n# https://private.example/comment\n"))?;
    let output = context
        .build()
        .args([
            "--offline",
            "--preview-features",
            "locked-tools",
            "--sdist",
            "--wheel",
            "--verbose",
        ])
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8(output.stderr)?;
    let skipped = stderr
        .lines()
        .filter_map(|line| {
            line.split_once("Skipping included file ")
                .map(|(_, message)| message)
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert_snapshot!(skipped, @"
    `locked_tool-1.0.0/PYLOCK.TOML`: supplied by the build frontend
    ");
    uv_snapshot!(context.python_command().arg("-c").arg(indoc! {r#"
        import tarfile, zipfile
        with tarfile.open("dist/locked_tool-1.0.0.tar.gz") as sdist:
            names = sdist.getnames()
            contents = sdist.extractfile("locked_tool-1.0.0/pylock.toml").read()
            print("raw lock included:", "locked_tool-1.0.0/uv.lock" in names)
            print("one source lock:", names.count("locked_tool-1.0.0/pylock.toml") == 1)
            print("private data excluded:", b"private.example" not in contents)
            print("source lock replaced:", b"untrusted source lock" not in contents)
        with zipfile.ZipFile("dist/locked_tool-1.0.0-py3-none-any.whl") as wheel:
            print("locks match:", contents == wheel.read("locked_tool-1.0.0.dist-info/pylock.toml"))
    "#}), @"
    exit_code: 0 (success)
    ----- stdout -----
    raw lock included: True
    one source lock: True
    private data excluded: True
    source lock replaced: True
    locks match: True
    ");
    let pyproject = context.temp_dir.child("pyproject.toml");
    fs_err::remove_file(context.temp_dir.child("PYLOCK.TOML"))?;
    context.temp_dir.child("pylock.toml").create_dir_all()?;
    context
        .temp_dir
        .child("pylock.toml/child")
        .write_str("source")?;
    pyproject.write_str(
        &fs_err::read_to_string(pyproject.path())?.replace("PYLOCK.TOML", "pylock.toml/**"),
    )?;
    uv_snapshot!(context.filters(), context.build().args(["--offline", "--preview-features", "locked-tools", "--sdist", "--list"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    Building locked_tool-1.0.0.tar.gz will include the following files:
    locked_tool-1.0.0/PKG-INFO (generated)
    locked_tool-1.0.0/pyproject.toml (generated)
    locked_tool-1.0.0/pyproject.toml.orig (pyproject.toml)
    locked_tool-1.0.0/pylock.toml/child (pylock.toml/child)
    locked_tool-1.0.0/src/locked_tool/__init__.py (src/locked_tool/__init__.py)
    locked_tool-1.0.0/uv.lock (uv.lock)
    locked_tool-1.0.0/pylock.toml (generated)
    ");
    let contents = fs_err::read_to_string(pyproject.path())?.replace(
        "requires-python = \">=3.12\"",
        "requires-python = \">=3.12\"\ndependencies = [\"ignored; python_version < '3' and platform_release == 'https://private.example/secret'\"]",
    );
    pyproject.write_str(&contents)?;
    context.lock().arg("--offline").assert().success();
    context
        .build()
        .args(["--offline", "--preview-features", "locked-tools", "--wheel"])
        .assert()
        .success();
    uv_snapshot!(context.python_command().arg("-c").arg(indoc! {r#"
        import zipfile
        with zipfile.ZipFile("dist/locked_tool-1.0.0-py3-none-any.whl") as wheel:
            lock = wheel.read("locked_tool-1.0.0.dist-info/pylock.toml")
            print("private data excluded:", b"private.example" not in lock)
    "#}), @"
    exit_code: 0 (success)
    ----- stdout -----
    private data excluded: True
    ");
    Ok(())
}

/// Export is controlled by the preview feature and the environment takes precedence over the setting.
#[test]
fn build_packaged_lock_configuration() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    context
        .temp_dir
        .child("src/locked_tool/__init__.py")
        .touch()?;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "locked-tool"
        version = "1.0.0"

        [build-system]
        requires = ["uv_build>=0.5.15,<2"]
        build-backend = "uv_build"

        [tool.uv]
        export-lock = true
    "#})?;
    uv_snapshot!(context.filters(), context.build().arg("--wheel"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Failed to build `[TEMP_DIR]/`
      cause: Failed to package the project lock
      cause: Exporting locks requires the `locked-tools` preview feature
    ");
    uv_snapshot!(context.filters(), context.build().args(["--preview-features", "locked-tools", "--wheel"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Failed to build `[TEMP_DIR]/`
      cause: Failed to package the project lock
      cause: Cannot export a lock: `uv.lock` was not found; run `uv lock` before building
    ");
    context
        .build()
        .env(EnvVars::UV_EXPORT_LOCK, "false")
        .arg("--wheel")
        .assert()
        .success();
    assert!(
        !zip_file_names(
            &context
                .temp_dir
                .child("dist/locked_tool-1.0.0-py3-none-any.whl")
        )?
        .contains(&"locked_tool-1.0.0.dist-info/pylock.toml".to_string())
    );
    let pyproject = context.temp_dir.child("pyproject.toml");
    let original = fs_err::read_to_string(pyproject.path())?;
    pyproject.write_str(&original.replace("[tool.uv]\nexport-lock = true\n", ""))?;
    context.build().arg("--wheel").assert().success();
    context
        .build()
        .args(["--preview-features", "locked-tools", "--wheel"])
        .assert()
        .failure();
    context
        .build()
        .env(EnvVars::UV_EXPORT_LOCK, "false")
        .args(["--preview-features", "locked-tools", "--wheel"])
        .assert()
        .success();
    let contents = original.replace("export-lock = true", "export-lock = \"true\"");
    pyproject.write_str(&contents)?;
    uv_snapshot!(context.filters(), context.build().args(["--preview-features", "locked-tools", "--wheel"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Failed to build `[TEMP_DIR]/`
      cause: Failed to package the project lock
      cause: The project setting `tool.uv.export-lock` must be a boolean
    ");
    fs_err::remove_file(pyproject.path())?;
    context
        .temp_dir
        .child("setup.py")
        .write_str("from setuptools import setup\nsetup(name='locked-tool', version='1.0.0')\n")?;
    uv_snapshot!(context.filters(), context.build().env(EnvVars::UV_EXPORT_LOCK, "true").args(["--preview-features", "locked-tools", "--wheel"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Failed to build `[TEMP_DIR]/`
      cause: Failed to package the project lock
      cause: Cannot export a lock without project metadata
    ");
    Ok(())
}

/// Other build backends include a lock only when export is explicitly enabled.
#[test]
fn build_packaged_lock_other_backend() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "locked-tool"
        version = "1.0.0"
        requires-python = ">=3.12"

        [build-system]
        requires = []
        build-backend = "test_backend"
        backend-path = ["."]
    "#})?;
    context.temp_dir.child("test_backend.py").write_str(indoc! {r#"
        import io, pathlib, tarfile, zipfile
        def prepare_metadata_for_build_wheel(metadata_directory, config_settings=None):
            dist_info = pathlib.Path(metadata_directory) / "Locked_Tool-1.0.0.dist-info"
            dist_info.mkdir()
            (dist_info / "METADATA").write_text("Metadata-Version: 2.4\nName: locked-tool\nVersion: 1.0.0\nRequires-Python: >=3.12\n")
            return dist_info.name
        def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
            filename = "Locked_Tool-1.0.0-py3-none-any.whl"
            with zipfile.ZipFile(pathlib.Path(wheel_directory) / filename, "w") as wheel:
                wheel.writestr("Locked_Tool-1.0.0.dist-info/WHEEL", "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n")
                metadata = "Metadata-Version: 2.4\nName: locked-tool\nVersion: 1.0.0\nRequires-Python: >=3.12\n"
                if pathlib.Path("mismatch").exists():
                    metadata += "Requires-Dist: requests\n"
                wheel.writestr("Locked_Tool-1.0.0.dist-info/METADATA", metadata)
                wheel.writestr("Locked_Tool-1.0.0.dist-info/RECORD", "Locked_Tool-1.0.0.dist-info/RECORD,,\n")
                if pathlib.Path("signed").exists():
                    wheel.writestr("Locked_Tool-1.0.0.dist-info/RECORD.jws", "signature")
            return filename
        def build_sdist(sdist_directory, config_settings=None):
            filename = "locked-tool-1.0.0.tar.gz"
            with tarfile.open(pathlib.Path(sdist_directory) / filename, "w:gz") as archive:
                for path in ["pyproject.toml", "test_backend.py", "uv.lock"]:
                    archive.add(path, arcname=f"locked-tool-1.0.0/{path}")
                metadata = b"Metadata-Version: 2.4\nName: locked-tool\nVersion: 1.0.0\nRequires-Python: >=3.12\n"
                if pathlib.Path("mismatch_sdist").exists():
                    metadata += b"Requires-Dist: unexpected\n"
                info = tarfile.TarInfo("locked-tool-1.0.0/PKG-INFO")
                info.size = len(metadata)
                archive.addfile(info, io.BytesIO(metadata))
                link = tarfile.TarInfo("locked-tool-1.0.0/long-link")
                link.type = tarfile.SYMTYPE
                link.linkname = "a" * 140
                archive.addfile(link)
            return filename
        build_editable = build_wheel
    "#})?;
    context.lock().arg("--offline").assert().success();
    let wheel = context
        .temp_dir
        .child("dist/Locked_Tool-1.0.0-py3-none-any.whl");
    context
        .build()
        .args([
            "--preview-features",
            "locked-tools",
            "--wheel",
            "--no-build-isolation",
        ])
        .assert()
        .success();
    assert!(
        !zip_file_names(wheel.path())?
            .contains(&"Locked_Tool-1.0.0.dist-info/pylock.toml".to_string())
    );
    context
        .build()
        .env(EnvVars::UV_EXPORT_LOCK, "true")
        .args(["--preview-features", "locked-tools", "--no-build-isolation"])
        .assert()
        .success();
    assert!(
        zip_file_names(wheel.path())?
            .contains(&"Locked_Tool-1.0.0.dist-info/pylock.toml".to_string())
    );
    context
        .python_command()
        .args([
            "-c",
            indoc! {r#"
        import tarfile
        with tarfile.open("dist/locked-tool-1.0.0.tar.gz") as archive:
            assert "locked-tool-1.0.0/pylock.toml" in archive.getnames()
            assert "locked-tool-1.0.0/uv.lock" in archive.getnames()
            assert all(name.startswith("locked-tool-1.0.0/") for name in archive.getnames())
            assert archive.getmember("locked-tool-1.0.0/long-link").linkname == "a" * 140
    "#},
        ])
        .assert()
        .success();
    context.temp_dir.child("mismatch_sdist").touch()?;
    context
        .build()
        .env(EnvVars::UV_EXPORT_LOCK, "true")
        .args([
            "--preview-features",
            "locked-tools",
            "--no-build-isolation",
            "--sdist",
            "--out-dir",
            "bad-sdist",
        ])
        .assert()
        .failure();
    assert!(
        !context
            .temp_dir
            .child("bad-sdist/locked-tool-1.0.0.tar.gz")
            .path()
            .is_file()
    );
    fs_err::remove_file(context.temp_dir.child("mismatch_sdist"))?;
    let pyproject = context.temp_dir.child("pyproject.toml");
    let contents = fs_err::read_to_string(pyproject.path())?.replace(
        "requires-python = \">=3.12\"",
        "requires-python = \">=3.12\"\ndynamic = [\"dependencies\"]",
    );
    pyproject.write_str(&contents)?;
    context
        .lock()
        .args(["--offline", "--no-build-isolation"])
        .assert()
        .success();
    context
        .build()
        .env(EnvVars::UV_EXPORT_LOCK, "true")
        .args([
            "--preview-features",
            "locked-tools",
            "--wheel",
            "--no-build-isolation",
        ])
        .assert()
        .success();
    assert!(
        zip_file_names(wheel.path())?
            .contains(&"Locked_Tool-1.0.0.dist-info/pylock.toml".to_string())
    );
    context.temp_dir.child("mismatch").touch()?;
    context
        .build()
        .env(EnvVars::UV_EXPORT_LOCK, "true")
        .args([
            "--preview-features",
            "locked-tools",
            "--wheel",
            "--no-build-isolation",
        ])
        .assert()
        .success();
    fs_err::remove_file(context.temp_dir.child("mismatch"))?;
    context.temp_dir.child("signed").touch()?;
    uv_snapshot!(context.filters(), context.build().env(EnvVars::UV_EXPORT_LOCK, "true")
        .args(["--preview-features", "locked-tools", "--wheel", "--no-build-isolation"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    Building wheel...
    error: Failed to build `[TEMP_DIR]/`
      cause: Failed to package the project lock
      cause: Cannot add a lock to a signed wheel
    ");
    fs_err::remove_file(context.temp_dir.child("signed"))?;
    pyproject.write_str(&contents.replace(
        "requires-python = \">=3.12\"\ndynamic = [\"dependencies\"]",
        "dynamic = [\"requires-python\"]",
    ))?;
    context
        .lock()
        .args(["--offline", "--no-build-isolation"])
        .assert()
        .success();
    context
        .build()
        .env(EnvVars::UV_EXPORT_LOCK, "true")
        .args([
            "--preview-features",
            "locked-tools",
            "--wheel",
            "--no-build-isolation",
        ])
        .assert()
        .success();
    context.temp_dir.child("missing").create_dir_all()?;
    context
        .python_command()
        .arg("-c")
        .arg(indoc! {r#"
        import tarfile
        with tarfile.open("missing/locked-tool-1.0.0.tar.gz", "w:gz") as archive:
            archive.add("pyproject.toml", arcname="locked-tool-1.0.0/pyproject.toml")
    "#})
        .assert()
        .success();
    uv_snapshot!(context.filters(), context.build().env(EnvVars::UV_EXPORT_LOCK, "true")
        .args(["--preview-features", "locked-tools", "--wheel", "--no-build-isolation",
            "missing/locked-tool-1.0.0.tar.gz"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Failed to build `[TEMP_DIR]/missing/locked-tool-1.0.0.tar.gz`
      cause: Failed to package the project lock
      cause: Cannot export a lock: the source distribution has no `pylock.toml`
    ");
    Ok(())
}

/// Automatic export uses the registry source recorded in the lock.
#[test]
fn build_packaged_lock_pypi_sources() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    context
        .temp_dir
        .child("src/locked_tool/__init__.py")
        .touch()?;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "locked-tool"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = ["dependency"]

        [build-system]
        requires = ["uv_build>=0.5.15,<2"]
        build-backend = "uv_build"
    "#})?;
    let write_lock = |url: &str| -> Result<()> {
        context.temp_dir.child("uv.lock").write_str(&formatdoc! {r#"
            version = 1
            revision = 3
            requires-python = ">=3.12"
            [options]
            exclude-newer = "2024-03-25T00:00:00Z"
            [[package]]
            name = "locked-tool"
            version = "1.0.0"
            source = {{ editable = "." }}
            dependencies = [{{ name = "dependency" }}]
            [package.metadata]
            requires-dist = [{{ name = "dependency" }}]
            [[package]]
            name = "dependency"
            version = "1.0.0"
            source = {{ registry = "https://pypi.org/simple" }}
            wheels = [{{ url = "{url}", hash = "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef" }}]
        "#})?;
        Ok(())
    };
    let wheel = context
        .temp_dir
        .child("dist/locked_tool-1.0.0-py3-none-any.whl");
    let has_lock = || -> Result<bool> {
        Ok(zip_file_names(wheel.path())?
            .contains(&"locked_tool-1.0.0.dist-info/pylock.toml".to_string()))
    };
    write_lock("https://example.com/dependency-1.0.0-py3-none-any.whl")?;
    context
        .build()
        .args(["--preview-features", "locked-tools", "--wheel"])
        .assert()
        .success();
    assert!(has_lock()?);
    write_lock("http://example.com/dependency-1.0.0-py3-none-any.whl")?;
    context
        .build()
        .args(["--preview-features", "locked-tools", "--wheel"])
        .assert()
        .success();
    assert!(!has_lock()?);
    write_lock(
        "https://files.pythonhosted.org/packages/dependency-1.0.0-py3-none-any.whl?token=secret",
    )?;
    context
        .build()
        .args(["--preview-features", "locked-tools"])
        .assert()
        .success();
    assert!(!has_lock()?);
    context
        .python_command()
        .arg("-c")
        .arg(indoc! {r#"
        import tarfile
        with tarfile.open("dist/locked_tool-1.0.0.tar.gz") as sdist:
            assert "locked_tool-1.0.0/pylock.toml" not in sdist.getnames()
    "#})
        .assert()
        .success();
    write_lock("https://files.pythonhosted.org/packages/other-1.0.0-py3-none-any.whl")?;
    uv_snapshot!(context.filters(), context.build()
        .args(["--preview-features", "locked-tools", "--sdist"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Failed to build `[TEMP_DIR]/`
      cause: Failed to package the project lock
      cause: Cannot export an invalid lock
      cause: Wheel filename `other-1.0.0-py3-none-any.whl` does not match package name `dependency`
    ");
    write_lock("https://files.pythonhosted.org/packages/dependency-1.0.0-py3-none-any.whl")?;
    let pyproject = context.temp_dir.child("pyproject.toml");
    let original = fs_err::read_to_string(pyproject.path())?;
    pyproject.write_str(&format!(
        "{original}\n[tool.uv]\noverride-dependencies = [\"dependency==2.0.0\"]\n"
    ))?;
    uv_snapshot!(context.filters(), context.build()
        .args(["--preview-features", "locked-tools", "--wheel"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Failed to build `[TEMP_DIR]/`
      cause: Failed to package the project lock
      cause: Cannot verify that `uv.lock` is up to date offline; run `uv lock` before building
      cause: Because dependency was not found in the cache and your project depends on dependency==2.0.0, we can conclude that your project's requirements are unsatisfiable.

    hint: Packages were unavailable because the network was disabled. When the network is disabled, registry packages may only be read from the cache.
    ");
    pyproject.write_str(&original)?;
    let lock_path = context.temp_dir.child("uv.lock");
    let lock = fs_err::read_to_string(lock_path.path())?;
    lock_path.write_str(&lock.replace(
        ", hash = \"sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\"",
        "",
    ))?;
    uv_snapshot!(context.filters(), context.build()
        .args(["--preview-features", "locked-tools", "--wheel"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Failed to build `[TEMP_DIR]/`
      cause: Failed to package the project lock
      cause: Cannot export a lock with missing artifact hashes; regenerate `uv.lock` with artifact hashes before building
    ");
    lock_path.write_str(&lock.replace(
        "wheels = [{ url = \"https://files.pythonhosted.org/packages/dependency-1.0.0-py3-none-any.whl\", hash = \"sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\" }]\n",
        "",
    ))?;
    context
        .build()
        .args(["--preview-features", "locked-tools", "--wheel"])
        .assert()
        .failure();
    let wheel = "wheels = [{ url = \"https://files.pythonhosted.org/packages/dependency-1.0.0-py3-none-any.whl\", hash = \"sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\" }]";
    for artifact in [
        "wheels = [{ filename = \"dependency-1.0.0-py3-none-any.whl\", hash = \"sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\" }]",
        "wheels = [{ path = \"dependency-1.0.0-py3-none-any.whl\", hash = \"sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\" }]",
        "sdist = { path = \"dependency-1.0.0.tar.gz\", hash = \"sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\" }",
        "sdist = { hash = \"sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\" }",
    ] {
        lock_path.write_str(&lock.replace(wheel, artifact))?;
        insta::allow_duplicates! {
            uv_snapshot!(context.filters(), context.build()
                .args(["--preview-features", "locked-tools", "--wheel"]), @"
            exit_code: 2 (failure)
            ----- stderr -----
            error: Failed to build `[TEMP_DIR]/`
              cause: Failed to package the project lock
              cause: Cannot export a lock with registry artifacts without URLs; run `uv lock` before building
            ");
        }
    }
    // Non-PyPI sources disable automatic export, even if a PyPI artifact is missing its URL.
    pyproject.write_str(&original.replace(
        "dependencies = [\"dependency\"]",
        "dependencies = [\"dependency\", \"private\"]",
    ))?;
    lock_path.write_str(&format!(
        "{}\n[[package]]\nname = \"private\"\nversion = \"1.0.0\"\nsource = {{ registry = \"https://private.example/simple\" }}\n",
        lock.replace(wheel, "wheels = [{ filename = \"dependency-1.0.0-py3-none-any.whl\" }]")
            .replace("dependencies = [{ name = \"dependency\" }]", "dependencies = [{ name = \"dependency\" }, { name = \"private\" }]")
            .replace("requires-dist = [{ name = \"dependency\" }]", "requires-dist = [{ name = \"dependency\" }, { name = \"private\" }]"),
    ))?;
    context
        .build()
        .args(["--preview-features", "locked-tools", "--wheel"])
        .assert()
        .success();
    assert!(!has_lock()?);
    pyproject.write_str(&original)?;
    let restricted = lock.replacen(
        "version = 1\n",
        "version = 1\nsupported-markers = [\"sys_platform == 'linux'\"]\n",
        1,
    );
    lock_path.write_str(&restricted)?;
    uv_snapshot!(context.filters(), context.build()
        .args(["--preview-features", "locked-tools", "--wheel"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Failed to build `[TEMP_DIR]/`
      cause: Failed to package the project lock
      cause: Cannot export a lock restricted to specific environments
    ");
    Ok(())
}

/// Packaged locks contain base dependencies, but private optional sources still prevent export.
#[test]
fn build_packaged_lock_extras() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    context
        .temp_dir
        .child("src/locked_tool/__init__.py")
        .touch()?;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "locked-tool"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = ["dependency"]

        [project.optional-dependencies]
        fast = ["helper"]
        slow = ["helper"]
        empty = []

        [build-system]
        requires = ["uv_build>=0.5.15,<2"]
        build-backend = "uv_build"
    "#})?;
    let lock = context.temp_dir.child("uv.lock");
    lock.write_str(indoc! {r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "locked-tool"
        version = "1.0.0"
        source = { editable = "." }
        dependencies = [{ name = "dependency" }]

        [package.optional-dependencies]
        fast = [{ name = "helper" }]
        slow = [{ name = "helper" }]

        [package.metadata]
        requires-dist = [
            { name = "dependency" },
            { name = "helper", marker = "extra == 'fast'" },
            { name = "helper", marker = "extra == 'slow'" },
        ]
        provides-extras = ["empty", "fast", "slow"]

        [[package]]
        name = "dependency"
        version = "1.0.0"
        source = { registry = "https://pypi.org/simple" }
        wheels = [{ url = "https://files.pythonhosted.org/packages/dependency-1.0.0-py3-none-any.whl", hash = "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef" }]

        [package.optional-dependencies]
        feature = [{ name = "leaf" }]

        [[package]]
        name = "helper"
        version = "1.0.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [{ name = "dependency", extra = ["feature"] }]
        wheels = [{ url = "https://files.pythonhosted.org/packages/helper-1.0.0-py3-none-any.whl", hash = "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef" }]

        [[package]]
        name = "leaf"
        version = "1.0.0"
        source = { registry = "https://pypi.org/simple" }
        wheels = [{ url = "https://files.pythonhosted.org/packages/leaf-1.0.0-py3-none-any.whl", hash = "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef" }]
    "#})?;
    context
        .build()
        .args(["--offline", "--preview-features", "locked-tools", "--wheel"])
        .assert()
        .success();
    uv_snapshot!(context.python_command().arg("-c").arg(indoc! {r#"
        import tomllib, zipfile
        with zipfile.ZipFile("dist/locked_tool-1.0.0-py3-none-any.whl") as wheel:
            lock = tomllib.loads(wheel.read("locked_tool-1.0.0.dist-info/pylock.toml").decode())
        print("extras:", lock.get("extras"))
        for package in lock["packages"]:
            print(package["name"], package.get("marker"))
    "#}), @"
    exit_code: 0 (success)
    ----- stdout -----
    extras: None
    dependency None
    ");

    let contents = fs_err::read_to_string(lock.path())?;
    let pyproject = context.temp_dir.child("pyproject.toml");
    let original = fs_err::read_to_string(pyproject.path())?;
    pyproject.write_str(&format!(
        "{original}\n{}",
        indoc! {r#"
        [tool.uv.sources]
        helper = { index = "private" }

        [[tool.uv.index]]
        name = "private"
        url = "https://private.example/simple"
        explicit = true
    "#}
    ))?;
    let private_helper = contents.replace(
        "name = \"helper\"\nversion = \"1.0.0\"\nsource = { registry = \"https://pypi.org/simple\" }",
        "name = \"helper\"\nversion = \"1.0.0\"\nsource = { registry = \"https://private.example/simple\" }",
    ).replace(
        "{ name = \"helper\", marker =",
        "{ name = \"helper\", index = \"https://private.example/simple\", marker =",
    );
    lock.write_str(&private_helper)?;
    context
        .build()
        .args(["--offline", "--preview-features", "locked-tools", "--wheel"])
        .assert()
        .success();
    assert!(
        !zip_file_names(
            &context
                .temp_dir
                .child("dist/locked_tool-1.0.0-py3-none-any.whl")
        )?
        .contains(&"locked_tool-1.0.0.dist-info/pylock.toml".to_string())
    );
    // A private dependency group also prevents automatic export.
    pyproject.write_str(&format!(
        "{}\n{}",
        original,
        indoc! {r#"
            [dependency-groups]
            dev = ["private"]

            [tool.uv.sources]
            private = { index = "private" }

            [[tool.uv.index]]
            name = "private"
            url = "https://private.example/simple"
            explicit = true
        "#}
    ))?;
    lock.write_str(&format!(
        "{}\n{}",
        contents.replace(
            "[package.metadata]\nrequires-dist",
            "[package.dev-dependencies]\ndev = [{ name = \"private\" }]\n[package.metadata]\nrequires-dist",
        ),
        indoc! {r#"
            [[package]]
            name = "private"
            version = "1.0.0"
            source = { registry = "https://private.example/simple" }
            wheels = [{ url = "https://private.example/private-1.0.0-py3-none-any.whl", hash = "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef" }]
        "#}
    ))?;
    context
        .build()
        .args(["--offline", "--preview-features", "locked-tools", "--wheel"])
        .assert()
        .success();
    assert!(
        !zip_file_names(
            &context
                .temp_dir
                .child("dist/locked_tool-1.0.0-py3-none-any.whl")
        )?
        .contains(&"locked_tool-1.0.0.dist-info/pylock.toml".to_string())
    );
    Ok(())
}

/// A PyPI index ending in a slash is eligible for automatic lock export.
#[test]
fn build_packaged_lock_pypi_trailing_slash() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    context
        .temp_dir
        .child("src/locked_tool/__init__.py")
        .touch()?;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "locked-tool"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = ["dependency"]

        [build-system]
        requires = ["uv_build>=0.5.15,<2"]
        build-backend = "uv_build"

        [tool.uv.sources]
        dependency = { index = "pypi-explicit" }

        [[tool.uv.index]]
        name = "pypi-explicit"
        url = "https://pypi.org/simple/"
        explicit = true
    "#})?;
    context.temp_dir.child("uv.lock").write_str(indoc! {r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "locked-tool"
        version = "1.0.0"
        source = { editable = "." }
        dependencies = [{ name = "dependency" }]

        [package.metadata]
        requires-dist = [{ name = "dependency", index = "https://pypi.org/simple/" }]

        [[package]]
        name = "dependency"
        version = "1.0.0"
        source = { registry = "https://pypi.org/simple/" }
        wheels = [{ url = "https://files.pythonhosted.org/packages/dependency-1.0.0-py3-none-any.whl", hash = "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef" }]
    "#})?;
    context
        .build()
        .args(["--offline", "--preview-features", "locked-tools"])
        .assert()
        .success();
    assert!(
        zip_file_names(
            context
                .temp_dir
                .child("dist/locked_tool-1.0.0-py3-none-any.whl")
                .path()
        )?
        .contains(&"locked_tool-1.0.0.dist-info/pylock.toml".to_string())
    );
    context
        .python_command()
        .arg("-c")
        .arg(indoc! {r#"
        import tarfile
        with tarfile.open("dist/locked_tool-1.0.0.tar.gz") as sdist:
            assert "locked_tool-1.0.0/pylock.toml" in sdist.getnames()
    "#})
        .assert()
        .success();
    write_uv_build_backend(&context)?;
    build_with_uv_build(&context)
        .args([
            "--offline",
            "--preview-features",
            "locked-tools",
            "--no-build-isolation",
            "--wheel",
            "--out-dir",
            "rebuilt",
            "dist/locked_tool-1.0.0.tar.gz",
        ])
        .assert()
        .success();
    assert!(
        zip_file_names(
            context
                .temp_dir
                .child("rebuilt/locked_tool-1.0.0-py3-none-any.whl")
                .path()
        )?
        .contains(&"locked_tool-1.0.0.dist-info/pylock.toml".to_string())
    );
    Ok(())
}

/// Do not package a stale lock when project dependencies or supported Python versions change.
#[test]
fn build_packaged_lock_stale() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    context
        .temp_dir
        .child("src/locked_tool/__init__.py")
        .touch()?;
    let pyproject = context.temp_dir.child("pyproject.toml");
    pyproject.write_str(indoc! {r#"
        [project]
        name = "locked-tool"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = ["requests"]

        [build-system]
        requires = ["uv_build>=0.5.15,<2"]
        build-backend = "uv_build"
    "#})?;
    context.temp_dir.child("uv.lock").write_str(indoc! {r#"
        version = 1
        requires-python = ">=3.12"

        [[package]]
        name = "locked-tool"
        version = "1.0.0"
        source = { editable = "." }
    "#})?;
    uv_snapshot!(context.filters(), context.build()
        .args(["--preview-features", "locked-tools", "--wheel"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Failed to build `[TEMP_DIR]/`
      cause: Failed to package the project lock
      cause: `uv.lock` does not match the dependencies of `locked-tool`; run `uv lock` before building
    ");
    let original = fs_err::read_to_string(pyproject.path())?;
    pyproject.write_str(&format!(
        "{original}\n[tool.uv.sources]\nrequests = {{ index = \"missing\" }}\n"
    ))?;
    uv_snapshot!(context.filters(), context.build()
        .args(["--offline", "--preview-features", "locked-tools", "--wheel"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Failed to build `[TEMP_DIR]/`
      cause: Failed to package the project lock
      cause: Failed to parse entry: `requests`
      cause: Package `requests` references an undeclared index: `missing`
    ");
    pyproject.write_str(&original)?;
    pyproject.write_str(
        &fs_err::read_to_string(pyproject.path())?
            .replace("dependencies = [\"requests\"]", "dependencies = []")
            .replace(">=3.12", ">=3.8"),
    )?;
    uv_snapshot!(context.filters(), context.build()
        .args(["--preview-features", "locked-tools", "--wheel"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Failed to build `[TEMP_DIR]/`
      cause: Failed to package the project lock
      cause: Cannot export a lock that does not cover the Python versions supported by `locked-tool`
    ");
    pyproject.write_str(
        &fs_err::read_to_string(pyproject.path())?.replace("requires-python = \">=3.8\"\n", ""),
    )?;
    context
        .build()
        .args(["--preview-features", "locked-tools", "--wheel"])
        .assert()
        .failure();
    context.temp_dir.child("uv.lock").write_str(indoc! {r#"
        version = 1
        requires-python = ">=3.12"

        [[package]]
        name = "old-name"
        version = "1.0.0"
        source = { editable = "." }
    "#})?;
    uv_snapshot!(context.filters(), context.build()
        .args(["--preview-features", "locked-tools", "--wheel"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Failed to build `[TEMP_DIR]/`
      cause: Failed to package the project lock
      cause: `uv.lock` does not contain `locked-tool`
    ");
    context
        .temp_dir
        .child("uv.lock")
        .write_str("invalid lock")?;
    context
        .build()
        .args(["--preview-features", "locked-tools", "--wheel"])
        .assert()
        .failure();
    Ok(())
}

/// A workspace member can use the workspace lock unless its runtime dependencies include a member.
#[test]
fn build_packaged_lock_workspace() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [tool.uv.workspace]
        members = ["packages/*"]
    "#})?;
    context
        .temp_dir
        .child("packages/tool/src/locked_tool/__init__.py")
        .touch()?;
    context
        .temp_dir
        .child("packages/shared/src/shared/__init__.py")
        .touch()?;
    context
        .temp_dir
        .child("local-dependency/pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "local-dependency"
        version = "1.0.0"
        requires-python = ">=3.12"
    "#})?;
    let member = context.temp_dir.child("packages/tool/pyproject.toml");
    member.write_str(indoc! {r#"
        [project]
        name = "locked-tool"
        version = "1.0.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["uv_build>=0.5.15,<2"]
        build-backend = "uv_build"
    "#})?;
    context
        .temp_dir
        .child("packages/shared/pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "shared"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = ["local-dependency"]

        [build-system]
        requires = ["uv_build>=0.5.15,<2"]
        build-backend = "uv_build"

        [tool.uv.sources]
        local-dependency = { path = "../../local-dependency" }
    "#})?;
    context.lock().arg("--offline").assert().success();
    context
        .build()
        .args([
            "--offline",
            "--preview-features",
            "locked-tools",
            "--package",
            "locked-tool",
        ])
        .assert()
        .success();
    uv_snapshot!(context.python_command().arg("-c").arg(indoc! {r#"
        import pathlib, tarfile, tomllib, zipfile
        with tarfile.open("dist/locked_tool-1.0.0.tar.gz") as sdist:
            print("project lock:", "locked_tool-1.0.0/pylock.toml" in sdist.getnames() and "locked_tool-1.0.0/uv.lock" not in sdist.getnames())
        with zipfile.ZipFile("dist/locked_tool-1.0.0-py3-none-any.whl") as wheel:
            print("wheel lock:", "locked_tool-1.0.0.dist-info/pylock.toml" in wheel.namelist())
            lock = tomllib.loads(wheel.read("locked_tool-1.0.0.dist-info/pylock.toml").decode())
            print("dependencies:", lock["packages"])
    "#}), @"
    exit_code: 0 (success)
    ----- stdout -----
    project lock: True
    wheel lock: True
    dependencies: []
    ");
    context
        .build()
        .env(EnvVars::UV_EXPORT_LOCK, "true")
        .args(["--offline", "--preview-features", "locked-tools", "--wheel"])
        .arg(context.temp_dir.join("packages/tool/../../packages/tool"))
        .assert()
        .success();
    assert!(
        zip_file_names(
            &context
                .temp_dir
                .child("dist/locked_tool-1.0.0-py3-none-any.whl")
        )?
        .contains(&"locked_tool-1.0.0.dist-info/pylock.toml".to_string())
    );
    // A workspace member can be rebuilt without access to the original workspace.
    write_uv_build_backend(&context)?;
    build_with_uv_build(&context)
        .args([
            "--offline",
            "--preview-features",
            "locked-tools",
            "--no-build-isolation",
            "--wheel",
            "--out-dir",
            "rebuilt",
            "dist/locked_tool-1.0.0.tar.gz",
        ])
        .assert()
        .success();
    assert!(
        zip_file_names(
            &context
                .temp_dir
                .child("rebuilt/locked_tool-1.0.0-py3-none-any.whl")
        )?
        .contains(&"locked_tool-1.0.0.dist-info/pylock.toml".to_string())
    );
    let original = fs_err::read_to_string(member.path())?;
    let pyproject = original.replace(
        r#"requires-python = ">=3.12""#,
        "requires-python = \">=3.12\"\ndependencies = [\"shared\"]",
    ) + "\n[tool.uv.sources]\nshared = { workspace = true }\n";
    member.write_str(&pyproject)?;
    context.lock().arg("--offline").assert().success();
    // Export remains ineligible when another workspace member is unavailable.
    fs_err::remove_dir_all(context.temp_dir.child("packages/shared"))?;
    context
        .build()
        .args([
            "--offline",
            "--preview-features",
            "locked-tools",
            "--package",
            "locked-tool",
            "--wheel",
        ])
        .assert()
        .success();
    assert!(
        !zip_file_names(
            &context
                .temp_dir
                .child("dist/locked_tool-1.0.0-py3-none-any.whl")
        )?
        .contains(&"locked_tool-1.0.0.dist-info/pylock.toml".to_string())
    );
    uv_snapshot!(context.filters(), context.build().env(EnvVars::UV_EXPORT_LOCK, "true")
        .args(["--offline", "--preview-features", "locked-tools", "--package", "locked-tool", "--wheel"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Failed to build `[TEMP_DIR]/packages/tool`
      cause: Failed to package the project lock
      cause: Cannot export a lock with workspace dependency `shared`
    ");
    member.write_str(&original)?;
    uv_snapshot!(context.filters(), context.build()
        .args(["--offline", "--preview-features", "locked-tools", "--package", "locked-tool", "--wheel"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Failed to build `[TEMP_DIR]/packages/tool`
      cause: Failed to package the project lock
      cause: `uv.lock` does not match the dependencies of `locked-tool`; run `uv lock` before building
    ");
    Ok(())
}

/// Skip automatic export without a lock when a runtime dependency has an ineligible source.
#[test]
fn build_packaged_lock_missing_ineligible_source() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    context
        .temp_dir
        .child("src/locked_tool/__init__.py")
        .touch()?;
    let pyproject = context.temp_dir.child("pyproject.toml");
    let project = indoc! {r#"
        [project]
        name = "locked-tool"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = ["dependency"]

        [build-system]
        requires = ["uv_build>=0.5.15,<2"]
        build-backend = "uv_build"
    "#};
    let private_source = indoc! {r#"
        [[tool.uv.index]]
        name = "private"
        url = "https://private.example/simple"
        explicit = true

        [tool.uv.sources]
        dependency = { index = "private" }
    "#};
    pyproject.write_str(&format!("{project}\n{private_source}"))?;
    uv_snapshot!(context.filters(), context.build().args(["--offline", "--preview-features", "locked-tools", "--wheel"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    Building wheel...
    Successfully built dist/locked_tool-1.0.0-py3-none-any.whl
    ");
    assert!(
        !zip_file_names(
            &context
                .temp_dir
                .child("dist/locked_tool-1.0.0-py3-none-any.whl")
        )?
        .contains(&"locked_tool-1.0.0.dist-info/pylock.toml".to_string())
    );
    uv_snapshot!(context.filters(), context.build().env(EnvVars::UV_EXPORT_LOCK, "true").args(["--offline", "--preview-features", "locked-tools", "--wheel"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Failed to build `[TEMP_DIR]/`
      cause: Failed to package the project lock
      cause: Cannot export a lock: `uv.lock` was not found; run `uv lock` before building
    ");
    uv_snapshot!(context.filters(), context.build().args(["--offline", "--no-sources", "--preview-features", "locked-tools", "--wheel"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Failed to build `[TEMP_DIR]/`
      cause: Failed to package the project lock
      cause: Cannot export a lock: `uv.lock` was not found; run `uv lock` before building
    ");

    pyproject.write_str(&project.replace(
        "dependencies = [\"dependency\"]",
        "dependencies = [\"dependency @ https://example.com/dependency-1.0.0-py3-none-any.whl\"]",
    ))?;
    context
        .build()
        .args(["--offline", "--preview-features", "locked-tools", "--wheel"])
        .assert()
        .success();
    assert!(
        !zip_file_names(
            &context
                .temp_dir
                .child("dist/locked_tool-1.0.0-py3-none-any.whl")
        )?
        .contains(&"locked_tool-1.0.0.dist-info/pylock.toml".to_string())
    );

    // An optional or conditional dependency can make automatic export ineligible.
    for (declaration, source) in [
        (
            "dependencies = []\n[project.optional-dependencies]\nextra = [\"dependency\"]",
            private_source.to_string(),
        ),
        (
            "dependencies = [\"dependency\"]",
            private_source.replace(
                "dependency = { index = \"private\" }",
                "dependency = { index = \"private\", marker = \"sys_platform == 'linux'\" }",
            ),
        ),
    ] {
        pyproject.write_str(&format!(
            "{}\n{source}",
            project.replace("dependencies = [\"dependency\"]", declaration)
        ))?;
        context
            .build()
            .args(["--offline", "--preview-features", "locked-tools", "--wheel"])
            .assert()
            .success();
        assert!(
            !zip_file_names(
                &context
                    .temp_dir
                    .child("dist/locked_tool-1.0.0-py3-none-any.whl")
            )?
            .contains(&"locked_tool-1.0.0.dist-info/pylock.toml".to_string())
        );
    }

    // Unused sources do not establish the runtime source.
    for (declaration, source) in [
        ("dependencies = []", private_source.to_string()),
        ("dependencies = [\"other\"]", private_source.to_string()),
        (
            "dependencies = [\"dependency\"]",
            private_source.replace("https://private.example/simple", "https://pypi.org/simple"),
        ),
        (
            "dependencies = [\"dependency\"]",
            private_source.replace("https://private.example/simple", "https://pypi.org/simple/"),
        ),
        (
            "dependencies = [\"dependency\"]",
            format!("{private_source}\n[tool.uv]\noverride-dependencies = [\"dependency>=1\"]\n"),
        ),
        ("dynamic = [\"dependencies\"]", private_source.to_string()),
    ] {
        pyproject.write_str(&format!(
            "{}\n{source}",
            project.replace("dependencies = [\"dependency\"]", declaration)
        ))?;
        insta::allow_duplicates! {
            uv_snapshot!(context.filters(), context.build().args(["--offline", "--preview-features", "locked-tools", "--wheel"]), @"
            exit_code: 2 (failure)
            ----- stderr -----
            error: Failed to build `[TEMP_DIR]/`
              cause: Failed to package the project lock
              cause: Cannot export a lock: `uv.lock` was not found; run `uv lock` before building
            ");
        }
    }
    pyproject.write_str(&format!("{project}\n{private_source}"))?;
    let nested = context.temp_dir.child("nested");
    nested.child("src/locked_tool/__init__.py").touch()?;
    nested.child("pyproject.toml").write_str(&format!(
        "{project}\n{private_source}\n[tool.uv]\nno-sources-package = [\"dependency\"]\n"
    ))?;
    uv_snapshot!(context.filters(), context.build().args(["--offline", "--preview-features", "locked-tools", "--wheel", "nested"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Failed to build `[TEMP_DIR]/nested`
      cause: Failed to package the project lock
      cause: Cannot export a lock: `uv.lock` was not found; run `uv lock` before building
    ");
    context
        .user_config_dir
        .child("uv/uv.toml")
        .write_str("override-dependencies = [\"dependency>=1\"]\n")?;
    uv_snapshot!(context.filters(), context.build().args(["--offline", "--preview-features", "locked-tools", "--wheel"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Failed to build `[TEMP_DIR]/`
      cause: Failed to package the project lock
      cause: Cannot export a lock: `uv.lock` was not found; run `uv lock` before building
    ");
    Ok(())
}

/// A non-PyPI lock is skipped only after checking statically detectable changes.
#[test]
fn build_packaged_lock_skipped_stale() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    context
        .temp_dir
        .child("src/locked_tool/__init__.py")
        .touch()?;
    let pyproject = context.temp_dir.child("pyproject.toml");
    let project = indoc! {r#"
        [project]
        name = "locked-tool"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = ["dependency"]

        [build-system]
        requires = ["uv_build>=0.5.15,<2"]
        build-backend = "uv_build"
    "#};
    let named_private = indoc! {r#"
        [[tool.uv.index]]
        name = "private"
        url = "https://private.example/simple"
        explicit = true

        [tool.uv.sources]
        dependency = { index = "private" }
    "#};
    let private_default = indoc! {r#"
        [[tool.uv.index]]
        url = "https://private.example/simple"
        default = true
    "#};
    let pypi_default = indoc! {r#"
        [[tool.uv.index]]
        url = "https://pypi.org/simple"
        default = true
    "#};
    let lock = context.temp_dir.child("uv.lock");
    let lock_contents = |index: &str, pinned: bool| {
        let declaration = if pinned {
            ", index = \"https://private.example/simple\""
        } else {
            ""
        };
        let host = if index == "https://pypi.org/simple" {
            "files.pythonhosted.org"
        } else {
            "private.example"
        };
        formatdoc! {r#"
            version = 1
            revision = 3
            requires-python = ">=3.12"
            [[package]]
            name = "locked-tool"
            version = "1.0.0"
            source = {{ editable = "." }}
            dependencies = [{{ name = "dependency" }}]
            [package.metadata]
            requires-dist = [{{ name = "dependency"{declaration} }}]
            [[package]]
            name = "dependency"
            version = "1.0.0"
            source = {{ registry = "{index}" }}
            wheels = [{{ url = "https://{host}/packages/dependency-1.0.0-py3-none-any.whl", hash = "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef" }}]
        "#}
    };
    let private_lock = lock_contents("https://private.example/simple", true);
    lock.write_str(&private_lock)?;
    pyproject.write_str(&format!("{project}\n{named_private}"))?;
    context
        .build()
        .args(["--offline", "--preview-features", "locked-tools", "--wheel"])
        .assert()
        .success();

    // Revision 0 locks do not record the extras provided by a package.
    lock.write_str(&private_lock.replace("revision = 3\n", ""))?;
    pyproject.write_str(&format!(
        "{project}\n{named_private}\n[project.optional-dependencies]\nlegacy = []\n"
    ))?;
    context
        .build()
        .args(["--offline", "--preview-features", "locked-tools", "--wheel"])
        .assert()
        .success();

    lock.write_str(&private_lock)?;
    pyproject.write_str(project)?;
    uv_snapshot!(context.filters(), context.build().args(["--offline", "--preview-features", "locked-tools", "--wheel"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Failed to build `[TEMP_DIR]/`
      cause: Failed to package the project lock
      cause: `uv.lock` does not match the dependencies of `locked-tool`; run `uv lock` before building
    ");

    let unpinned_lock = lock_contents("https://private.example/simple", false);
    lock.write_str(&unpinned_lock)?;
    context
        .build()
        .args(["--offline", "--preview-features", "locked-tools", "--wheel"])
        .assert()
        .success();
    pyproject.write_str(&format!("{project}\n{private_default}"))?;
    context
        .build()
        .args(["--offline", "--preview-features", "locked-tools", "--wheel"])
        .assert()
        .success();
    pyproject.write_str(&format!("{project}\n{pypi_default}"))?;
    uv_snapshot!(context.filters(), context.build().args(["--offline", "--preview-features", "locked-tools", "--wheel"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Failed to build `[TEMP_DIR]/`
      cause: Failed to package the project lock
      cause: `uv.lock` references an index that is no longer configured; run `uv lock` before building
    ");

    // Omitted metadata does not establish whether the declarations changed.
    lock.write_str(&unpinned_lock.replace(
        "[package.metadata]\nrequires-dist = [{ name = \"dependency\" }]\n",
        "",
    ))?;
    context
        .build()
        .args(["--offline", "--preview-features", "locked-tools", "--wheel"])
        .assert()
        .success();

    lock.write_str(&lock_contents("https://pypi.org/simple", false))?;
    pyproject.write_str(&format!("{project}\n{named_private}"))?;
    context
        .build()
        .args(["--offline", "--preview-features", "locked-tools", "--wheel"])
        .assert()
        .failure();

    // A projectless workspace root can introduce an index through a dependency group.
    let workspace = context.temp_dir.child("workspace");
    workspace
        .child("packages/tool/src/locked_tool/__init__.py")
        .touch()?;
    workspace
        .child("packages/tool/pyproject.toml")
        .write_str(project)?;
    workspace.child("pyproject.toml").write_str(indoc! {r#"
        [tool.uv.workspace]
        members = ["packages/*"]

        [dependency-groups]
        dev = ["other"]

        [tool.uv.sources]
        other = { index = "private" }

        [[tool.uv.index]]
        name = "private"
        url = "https://private.example/simple"
        explicit = true
    "#})?;
    workspace.child("uv.lock").write_str(&formatdoc! {r#"
        {}
        [manifest]
        members = ["locked-tool"]
        [manifest.dependency-groups]
        dev = [{{ name = "other", index = "https://private.example/simple" }}]
        [[package]]
        name = "other"
        version = "1.0.0"
        source = {{ registry = "https://private.example/simple" }}
        wheels = [{{ url = "https://private.example/packages/other-1.0.0-py3-none-any.whl", hash = "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef" }}]
    "#, unpinned_lock.replace("editable = \".\"", "editable = \"packages/tool\"")})?;
    context
        .lock()
        .current_dir(workspace.path())
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .args(["--locked", "--offline", "--no-index"])
        .assert()
        .success();
    context
        .build()
        .current_dir(workspace.path())
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .args([
            "--offline",
            "--no-index",
            "--preview-features",
            "locked-tools",
            "--wheel",
            "--package",
            "locked-tool",
        ])
        .assert()
        .success();
    // A different workspace member does not make an unconfigured index valid.
    workspace
        .child("packages/other/pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "other"
        version = "1.0.0"
    "#})?;
    workspace.child("pyproject.toml").write_str(&format!(
        "[tool.uv.workspace]\nmembers = [\"packages/*\"]\n{private_default}"
    ))?;
    workspace.child("uv.lock").write_str(&formatdoc! {r#"
        {}
        [manifest]
        members = ["locked-tool", "other"]
        [[package]]
        name = "other"
        version = "1.0.0"
        source = {{ virtual = "packages/other" }}
    "#, unpinned_lock.replace("editable = \".\"", "editable = \"packages/tool\"")})?;
    context
        .build()
        .current_dir(workspace.path())
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .args([
            "--offline",
            "--preview-features",
            "locked-tools",
            "--wheel",
            "--package",
            "locked-tool",
        ])
        .assert()
        .success();
    context
        .build()
        .current_dir(workspace.path())
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .args([
            "--offline",
            "--no-index",
            "--preview-features",
            "locked-tools",
            "--wheel",
            "--package",
            "locked-tool",
        ])
        .assert()
        .success();
    workspace.child("pyproject.toml").write_str(&format!(
        "[tool.uv.workspace]\nmembers = [\"packages/*\"]\n{pypi_default}"
    ))?;
    uv_snapshot!(context.filters(), context.build().current_dir(workspace.path()).env_remove(EnvVars::UV_EXCLUDE_NEWER).args(["--offline", "--preview-features", "locked-tools", "--wheel", "--package", "locked-tool"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Failed to build `[TEMP_DIR]/workspace/packages/tool`
      cause: Failed to package the project lock
      cause: `uv.lock` references an index that is no longer configured; run `uv lock` before building
    ");
    Ok(())
}

/// A metadata-free lock can still reveal a removed unconditional dependency.
#[test]
fn build_packaged_lock_skipped_empty_dependencies() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    context
        .temp_dir
        .child("src/locked_tool/__init__.py")
        .touch()?;
    let pyproject = context.temp_dir.child("pyproject.toml");
    let project = indoc! {r#"
        [project]
        name = "locked-tool"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["uv_build>=0.5.15,<2"]
        build-backend = "uv_build"
    "#};
    pyproject.write_str(project)?;
    let lock = context.temp_dir.child("uv.lock");
    let lock_contents = indoc! {r#"
        version = 1
        revision = 5
        requires-python = ">=3.12"

        [[package]]
        name = "locked-tool"
        version = "1.0.0"
        source = { editable = "." }
        dependencies = [{ name = "dependency" }]

        [[package]]
        name = "dependency"
        version = "1.0.0"
        source = { registry = "https://private.example/simple" }
        wheels = [{ url = "https://private.example/packages/dependency-1.0.0-py3-none-any.whl", hash = "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef" }]
    "#};
    lock.write_str(lock_contents)?;
    uv_snapshot!(context.filters(), context.build().args(["--offline", "--preview-features", "locked-tools", "--wheel"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Failed to build `[TEMP_DIR]/`
      cause: Failed to package the project lock
      cause: `uv.lock` does not match the dependencies of `locked-tool`; run `uv lock` before building
    ");

    // Scoped overrides can introduce a dependency without a project declaration.
    pyproject.write_str(&format!("{project}\n[tool.uv]\noverride-dependencies = [{{ package = {{ name = \"locked-tool\", version = \"1.0.0\" }}, dependencies = [\"dependency\"] }}]\n"))?;
    lock.write_str(&format!(
        "{lock_contents}\n[manifest]\noverrides = [{{ package = {{ name = \"locked-tool\", version = \"1.0.0\" }}, dependencies = [{{ name = \"dependency\" }}] }}]\n"
    ))?;
    context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .args([
            "--offline",
            "--locked",
            "--preview-features",
            "lock-without-metadata",
        ])
        .assert()
        .success();
    context
        .build()
        .args(["--offline", "--preview-features", "locked-tools", "--wheel"])
        .assert()
        .success();
    Ok(())
}

/// An ineligible lock does not change the backend's source distribution.
#[test]
fn build_packaged_lock_skipped_sdist() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    context
        .temp_dir
        .child("src/locked_tool/__init__.py")
        .touch()?;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "locked-tool"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = ["dependency"]

        [build-system]
        requires = ["uv_build>=0.5.15,<2"]
        build-backend = "uv_build"

        [tool.uv.build-backend]
        source-include = ["uv.lock"]
    "#})?;
    context.temp_dir.child("uv.lock").write_str(indoc! {r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [[package]]
        name = "locked-tool"
        version = "1.0.0"
        source = { editable = "." }
        dependencies = [{ name = "dependency" }]

        [package.metadata]
        requires-dist = [{ name = "dependency" }]

        [[package]]
        name = "dependency"
        version = "1.0.0"
        source = { registry = "https://private.example/simple" }
    "#})?;
    context.temp_dir.child("backend/uv_build.py").write_str(indoc! {r#"
        import os, pathlib, shutil, subprocess
        def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
            return subprocess.check_output([os.environ["TEST_UV_BIN"], "build-backend", "build-wheel", wheel_directory], text=True).strip()
        def build_sdist(sdist_directory, config_settings=None):
            filename = subprocess.check_output([os.environ["TEST_UV_BIN"], "build-backend", "build-sdist", sdist_directory], text=True).strip()
            shutil.copyfile(pathlib.Path(sdist_directory) / filename, os.environ["TEST_BACKEND_SDIST"])
            return filename
    "#})?;
    let backend_sdist = context.temp_dir.child("backend-sdist.tar.gz");
    let built_sdist = context.temp_dir.child("dist/locked_tool-1.0.0.tar.gz");
    build_with_uv_build(&context)
        .env("TEST_BACKEND_SDIST", backend_sdist.path())
        .args([
            "--offline",
            "--preview-features",
            "locked-tools",
            "--no-build-isolation",
            "--force-pep517",
            "--sdist",
        ])
        .assert()
        .success();
    assert_eq!(
        fs_err::read(backend_sdist.path())?,
        fs_err::read(built_sdist.path())?
    );

    uv_snapshot!(context.filters(), build_with_uv_build(&context)
        .args(["--offline", "--preview-features", "locked-tools", "--no-build-isolation", "--wheel", "dist/locked_tool-1.0.0.tar.gz"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Failed to build `[TEMP_DIR]/dist/locked_tool-1.0.0.tar.gz`
      cause: Failed to package the project lock
      cause: Cannot export a lock: the source distribution has no `pylock.toml`
    ");
    build_with_uv_build(&context)
        .env(EnvVars::UV_EXPORT_LOCK, "false")
        .args([
            "--offline",
            "--preview-features",
            "locked-tools",
            "--no-build-isolation",
            "--wheel",
            "dist/locked_tool-1.0.0.tar.gz",
        ])
        .assert()
        .success();

    // Building the source distribution and wheel together retains the skip decision.
    build_with_uv_build(&context)
        .env("TEST_BACKEND_SDIST", backend_sdist.path())
        .args([
            "--offline",
            "--preview-features",
            "locked-tools",
            "--no-build-isolation",
            "--force-pep517",
        ])
        .assert()
        .success();
    assert!(
        !zip_file_names(
            context
                .temp_dir
                .child("dist/locked_tool-1.0.0-py3-none-any.whl")
                .path()
        )?
        .contains(&"locked_tool-1.0.0.dist-info/pylock.toml".to_string())
    );
    Ok(())
}

/// A packaged lock must have consistent artifact identities and non-overlapping package entries.
#[test]
fn build_sdist_with_invalid_packaged_lock() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    context.temp_dir.child("src/example/__init__.py").touch()?;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "example"
        version = "1.0.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["uv_build>=0.5.15,<2"]
        build-backend = "uv_build"
    "#})?;
    context.lock().arg("--offline").assert().success();
    context
        .build()
        .args(["--offline", "--preview-features", "locked-tools", "--sdist"])
        .assert()
        .success();
    write_uv_build_backend(&context)?;

    context.python_command().arg("-c").arg(indoc! {r#"
        import io, pathlib, tarfile
        source = pathlib.Path("dist/example-1.0.0.tar.gz")
        digest = "0" * 64
        def package(version, marker, artifact):
            return (f'\n[[packages]]\nname = "dependency"\nversion = "{version}"\n'
                    f'marker = "{marker}"\nindex = "https://pypi.org/simple"\n{artifact}\n')
        def wheel(filename):
            return f'wheels = [{{ url = "https://files.pythonhosted.org/{filename}", hashes = {{ sha256 = "{digest}" }} }}]'
        def sdist(filename):
            return f'sdist = {{ url = "https://files.pythonhosted.org/{filename}", hashes = {{ sha256 = "{digest}" }} }}'
        one = wheel("dependency-1.0.0-py3-none-any.whl")
        cases = {
            "missing-version": package("1.0.0", "sys_platform == 'win32'", one).replace('version = "1.0.0"\n', ''),
            "wheel-name": package("1.0.0", "sys_platform == 'win32'", wheel("other-1.0.0-py3-none-any.whl")),
            "wheel-version": package("1.0.0", "sys_platform == 'win32'", wheel("dependency-2.0.0-py3-none-any.whl")),
            "sdist-name": package("1.0.0", "sys_platform == 'win32'", sdist("other-1.0.0.tar.gz")),
            "sdist-version": package("1.0.0", "sys_platform == 'win32'", sdist("dependency-2.0.0.tar.gz")),
            "overlap": package("1.0.0", "sys_platform == 'win32'", one) + package("2.0.0", "python_version >= '3.13'", wheel("dependency-2.0.0-py3-none-any.whl")),

        }
        for variant, packages in cases.items():
            pathlib.Path(variant).mkdir()
            with tarfile.open(source) as src, tarfile.open(f"{variant}/{source.name}", "w:gz") as dst:
                for entry in src:
                    data = src.extractfile(entry).read() if entry.isfile() else None
                    if entry.name.endswith("/pylock.toml"):
                        data = data.replace(b"packages = []", packages.encode())
                    if data is not None:
                        entry.size = len(data)
                    dst.addfile(entry, io.BytesIO(data) if data is not None else None)
    "#}).assert().success();

    let mut errors = String::new();
    for variant in [
        "missing-version",
        "wheel-name",
        "wheel-version",
        "sdist-name",
        "sdist-version",
        "overlap",
    ] {
        let source = format!("{variant}/example-1.0.0.tar.gz");
        let output = build_with_uv_build(&context)
            .env(EnvVars::UV_EXPORT_LOCK, "true")
            .args([
                "--offline",
                "--preview-features",
                "locked-tools",
                "--no-build-isolation",
                "--wheel",
                &source,
            ])
            .output()?;
        assert_eq!(output.status.code(), Some(2));
        errors.push_str(&apply_filters(
            String::from_utf8_lossy(&output.stderr).into_owned(),
            context.filters(),
        ));
    }
    assert_snapshot!(errors, @"
    error: Failed to build `[TEMP_DIR]/missing-version/example-1.0.0.tar.gz`
      cause: Failed to package the project lock
      cause: The source distribution has an invalid `pylock.toml`
      cause: Registry package `dependency` has no version
    error: Failed to build `[TEMP_DIR]/wheel-name/example-1.0.0.tar.gz`
      cause: Failed to package the project lock
      cause: The source distribution has an invalid `pylock.toml`
      cause: Wheel filename `other-1.0.0-py3-none-any.whl` does not match package name `dependency`
    error: Failed to build `[TEMP_DIR]/wheel-version/example-1.0.0.tar.gz`
      cause: Failed to package the project lock
      cause: The source distribution has an invalid `pylock.toml`
      cause: Wheel filename `dependency-2.0.0-py3-none-any.whl` does not match package version `1.0.0`
    error: Failed to build `[TEMP_DIR]/sdist-name/example-1.0.0.tar.gz`
      cause: Failed to package the project lock
      cause: The source distribution has an invalid `pylock.toml`
      cause: Failed to parse source distribution filename `other-1.0.0.tar.gz`: Name doesn't start with package name dependency
    error: Failed to build `[TEMP_DIR]/sdist-version/example-1.0.0.tar.gz`
      cause: Failed to package the project lock
      cause: The source distribution has an invalid `pylock.toml`
      cause: Source distribution filename `dependency-2.0.0.tar.gz` does not match package version `1.0.0`
    error: Failed to build `[TEMP_DIR]/overlap/example-1.0.0.tar.gz`
      cause: Failed to package the project lock
      cause: The source distribution has an invalid `pylock.toml`
      cause: Package entries for `dependency` may be active in the same environment
    ");
    Ok(())
}

/// A lock with platform-specific versions remains usable when rebuilt from its source distribution.
#[test]
fn build_sdist_with_forked_packaged_lock() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    context.temp_dir.child("src/example/__init__.py").touch()?;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "example"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = ["dependency"]

        [build-system]
        requires = ["uv_build>=0.5.15,<2"]
        build-backend = "uv_build"
    "#})?;
    context.temp_dir.child("uv.lock").write_str(indoc! {r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"
        resolution-markers = [
            "sys_platform == 'win32'",
            "sys_platform != 'win32'",
        ]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "dependency"
        version = "1.0.0"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = ["sys_platform == 'win32'"]
        wheels = [{ url = "https://files.pythonhosted.org/packages/dependency-1.0.0-py3-none-any.whl", hash = "sha256:1111111111111111111111111111111111111111111111111111111111111111" }]

        [[package]]
        name = "dependency"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = ["sys_platform != 'win32'"]
        wheels = [{ url = "https://files.pythonhosted.org/packages/dependency-2.0.0-py3-none-any.whl", hash = "sha256:2222222222222222222222222222222222222222222222222222222222222222" }]

        [[package]]
        name = "example"
        version = "1.0.0"
        source = { editable = "." }
        dependencies = [
            { name = "dependency", version = "1.0.0", source = { registry = "https://pypi.org/simple" }, marker = "sys_platform == 'win32'" },
            { name = "dependency", version = "2.0.0", source = { registry = "https://pypi.org/simple" }, marker = "sys_platform != 'win32'" },
        ]

        [package.metadata]
        requires-dist = [{ name = "dependency" }]
    "#})?;
    context
        .build()
        .args(["--offline", "--preview-features", "locked-tools", "--sdist"])
        .assert()
        .success();
    write_uv_build_backend(&context)?;
    build_with_uv_build(&context)
        .args([
            "--offline",
            "--preview-features",
            "locked-tools",
            "--no-build-isolation",
            "--wheel",
            "dist/example-1.0.0.tar.gz",
        ])
        .assert()
        .success();
    uv_snapshot!(context.python_command().arg("-c").arg(indoc! {r#"
        import tomllib, zipfile
        with zipfile.ZipFile("dist/example-1.0.0-py3-none-any.whl") as wheel:
            lock = tomllib.loads(wheel.read("example-1.0.0.dist-info/pylock.toml").decode())
            print([(package["version"], package["marker"]) for package in lock["packages"]])
    "#}), @r#"
    exit_code: 0 (success)
    ----- stdout -----
    [('1.0.0', "sys_platform == 'win32'"), ('2.0.0', "sys_platform != 'win32'")]
    "#);
    Ok(())
}

/// Reject corrupt wheel entries when adding a packaged lock.
#[test]
fn build_packaged_lock_wheel_crc() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "locked-tool"
        version = "1.0.0"
        requires-python = ">=3.12"

        [build-system]
        requires = []
        build-backend = "uv_build"
        backend-path = ["."]
    "#})?;
    context.temp_dir.child("uv_build.py").write_str(indoc! {r#"
        import base64, hashlib, pathlib, struct, zipfile

        def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
            filename = "locked_tool-1.0.0-py3-none-any.whl"
            path = pathlib.Path(wheel_directory) / filename
            dist_info = "locked_tool-1.0.0.dist-info"
            files = {
                "locked_tool/__init__.py": b"original content\n",
                "locked_tool/large.bin": b"x" * (16 * 1024 * 1024 + 1),
                f"{dist_info}/METADATA": b"Metadata-Version: 2.4\nName: locked-tool\nVersion: 1.0.0\nRequires-Python: >=3.12\n",
                f"{dist_info}/WHEEL": b"Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
            }
            record = "".join(
                f"{name},sha256={base64.urlsafe_b64encode(hashlib.sha256(data).digest()).rstrip(b'=').decode()},{len(data)}\n"
                for name, data in files.items()
            ) + f"{dist_info}/RECORD,,\n"
            files[f"{dist_info}/RECORD"] = record.encode()
            target = pathlib.Path("corrupt").read_text()
            with zipfile.ZipFile(path, "w", compression=zipfile.ZIP_STORED) as wheel:
                for name, data in files.items():
                    wheel.writestr(name, data)
                info = wheel.getinfo(target)
            with path.open("r+b") as wheel:
                wheel.seek(info.header_offset)
                header = struct.unpack("<IHHHHHIIIHH", wheel.read(30))
                wheel.seek(info.header_offset + 30 + header[-2] + header[-1])
                first = wheel.read(1)
                wheel.seek(-1, 1)
                wheel.write(bytes([first[0] ^ 1]))
            return filename
    "#})?;
    context.lock().arg("--offline").assert().success();
    for entry in [
        "locked_tool/__init__.py",
        "locked_tool/large.bin",
        "locked_tool-1.0.0.dist-info/RECORD",
    ] {
        context.temp_dir.child("corrupt").write_str(entry)?;
        context
            .build()
            .args([
                "--offline",
                "--preview-features",
                "locked-tools",
                "--wheel",
                "--force-pep517",
                "--no-build-isolation",
            ])
            .assert()
            .failure()
            .stderr(predicate::str::contains(format!(
                "Wheel entry has an invalid CRC: `{entry}`"
            )));
        context
            .temp_dir
            .child("dist/locked_tool-1.0.0-py3-none-any.whl")
            .assert(predicate::path::missing());
    }
    Ok(())
}
