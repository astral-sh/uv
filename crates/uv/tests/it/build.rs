use crate::common::{uv_snapshot, TestContext};
use anyhow::Result;
use assert_cmd::assert::OutputAssertExt;
use assert_fs::prelude::*;
use fs_err::File;
use indoc::indoc;
use insta::assert_snapshot;
use predicates::prelude::predicate;
use std::env::current_dir;
use std::process::Command;
use zip::ZipArchive;

#[test]
fn build_basic() -> Result<()> {
    let context = TestContext::new("3.12");
    let filters = context
        .filters()
        .into_iter()
        .chain([(r"exit code: 1", "exit status: 1"), (r"\\\.", "")])
        .collect::<Vec<_>>();

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
    uv_snapshot!(&filters, context.build().arg("project"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Building source distribution...
    Building wheel from source distribution...
    Successfully built project/dist/project-0.1.0.tar.gz
    Successfully built project/dist/project-0.1.0-py3-none-any.whl
    "###);

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
    uv_snapshot!(&filters, context.build().current_dir(project.path()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Building source distribution...
    Building wheel from source distribution...
    Successfully built dist/project-0.1.0.tar.gz
    Successfully built dist/project-0.1.0-py3-none-any.whl
    "###);

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
    uv_snapshot!(&filters, context.build(), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Building source distribution...
      × Failed to build `[TEMP_DIR]/`
      ╰─▶ [TEMP_DIR]/ does not appear to be a Python project, as neither `pyproject.toml` nor `setup.py` are present in the directory
    "###);

    // Build to a specified path.
    uv_snapshot!(&filters, context.build().arg("--out-dir").arg("out").current_dir(project.path()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Building source distribution...
    Building wheel from source distribution...
    Successfully built out/project-0.1.0.tar.gz
    Successfully built out/project-0.1.0-py3-none-any.whl
    "###);

    project
        .child("out")
        .child("project-0.1.0.tar.gz")
        .assert(predicate::path::is_file());
    project
        .child("out")
        .child("project-0.1.0-py3-none-any.whl")
        .assert(predicate::path::is_file());

    Ok(())
}

#[test]
fn build_sdist() -> Result<()> {
    let context = TestContext::new("3.12");
    let filters = context
        .filters()
        .into_iter()
        .chain([(r"exit code: 1", "exit status: 1"), (r"\\\.", "")])
        .collect::<Vec<_>>();

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
    uv_snapshot!(&filters, context.build().arg("--sdist").current_dir(&project), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Building source distribution...
    Successfully built dist/project-0.1.0.tar.gz
    "###);

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
    let context = TestContext::new("3.12");
    let filters = context
        .filters()
        .into_iter()
        .chain([(r"exit code: 1", "exit status: 1"), (r"\\\.", "")])
        .collect::<Vec<_>>();

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
    uv_snapshot!(&filters, context.build().arg("--wheel").current_dir(&project), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Building wheel...
    Successfully built dist/project-0.1.0-py3-none-any.whl
    "###);

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
    let context = TestContext::new("3.12");
    let filters = context
        .filters()
        .into_iter()
        .chain([(r"exit code: 1", "exit status: 1"), (r"\\\.", "")])
        .collect::<Vec<_>>();

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
    uv_snapshot!(&filters, context.build().arg("--sdist").arg("--wheel").current_dir(&project), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Building source distribution...
    Building wheel...
    Successfully built dist/project-0.1.0.tar.gz
    Successfully built dist/project-0.1.0-py3-none-any.whl
    "###);

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
    let context = TestContext::new("3.12");
    let filters = context
        .filters()
        .into_iter()
        .chain([(r"exit code: 1", "exit status: 1"), (r"\\\.", "")])
        .collect::<Vec<_>>();

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
    uv_snapshot!(&filters, context.build().arg("--sdist").current_dir(&project), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Building source distribution...
    Successfully built dist/project-0.1.0.tar.gz
    "###);

    project
        .child("dist")
        .child("project-0.1.0.tar.gz")
        .assert(predicate::path::is_file());
    project
        .child("dist")
        .child("project-0.1.0-py3-none-any.whl")
        .assert(predicate::path::missing());

    // Error if `--wheel` is not specified.
    uv_snapshot!(&filters, context.build().arg("./dist/project-0.1.0.tar.gz").current_dir(&project), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
      × Failed to build `[TEMP_DIR]/project/dist/project-0.1.0.tar.gz`
      ╰─▶ Pass `--wheel` explicitly to build a wheel from a source distribution
    "###);

    // Error if `--sdist` is specified.
    uv_snapshot!(&filters, context.build().arg("./dist/project-0.1.0.tar.gz").arg("--sdist").current_dir(&project), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
      × Failed to build `[TEMP_DIR]/project/dist/project-0.1.0.tar.gz`
      ╰─▶ Building an `--sdist` from a source distribution is not supported
    "###);

    // Build the wheel from the sdist.
    uv_snapshot!(&filters, context.build().arg("./dist/project-0.1.0.tar.gz").arg("--wheel").current_dir(&project), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Building wheel from source distribution...
    Successfully built dist/project-0.1.0-py3-none-any.whl
    "###);

    project
        .child("dist")
        .child("project-0.1.0.tar.gz")
        .assert(predicate::path::is_file());
    project
        .child("dist")
        .child("project-0.1.0-py3-none-any.whl")
        .assert(predicate::path::is_file());

    // Passing a wheel is an error.
    uv_snapshot!(&filters, context.build().arg("./dist/project-0.1.0-py3-none-any.whl").arg("--wheel").current_dir(&project), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
      × Failed to build `[TEMP_DIR]/project/dist/project-0.1.0-py3-none-any.whl`
      ╰─▶ `dist/project-0.1.0-py3-none-any.whl` is not a valid build source. Expected to receive a source directory, or a source distribution ending in one of: `.tar.gz`, `.zip`, `.tar.bz2`, `.tar.lz`, `.tar.lzma`, `.tar.xz`, `.tar.zst`, `.tar`, `.tbz`, `.tgz`, `.tlz`, or `.txz`.
    "###);

    Ok(())
}

#[test]
fn build_fail() -> Result<()> {
    let context = TestContext::new("3.12");
    let filters = context
        .filters()
        .into_iter()
        .chain([(r"exit code: 1", "exit status: 1"), (r"\\\.", "")])
        .collect::<Vec<_>>();

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
    uv_snapshot!(&filters, context.build().arg("project"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Building source distribution...
    Traceback (most recent call last):
      File "<string>", line 14, in <module>
      File "[CACHE_DIR]/builds-v0/[TMP]/build_meta.py", line 328, in get_requires_for_build_sdist
        return self._get_build_requires(config_settings, requirements=[])
               ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
      File "[CACHE_DIR]/builds-v0/[TMP]/build_meta.py", line 295, in _get_build_requires
        self.run_setup()
      File "[CACHE_DIR]/builds-v0/[TMP]/build_meta.py", line 311, in run_setup
        exec(code, locals())
      File "<string>", line 2
        from setuptools import setup
    IndentationError: unexpected indent
      × Failed to build `[TEMP_DIR]/project`
      ├─▶ The build backend returned an error
      ╰─▶ Call to `setuptools.build_meta.build_sdist` failed (exit status: 1)
          hint: This usually indicates a problem with the package or the build environment.
    "###);

    Ok(())
}

#[test]
fn build_workspace() -> Result<()> {
    let context = TestContext::new("3.12");
    let filters = context
        .filters()
        .into_iter()
        .chain([
            (r"exit code: 1", "exit status: 1"),
            (r"\\\.", ""),
            (r"\[project\]", "[PKG]"),
            (r"\[member\]", "[PKG]"),
        ])
        .collect::<Vec<_>>();

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
    uv_snapshot!(&filters, context.build().arg("--package").arg("member").current_dir(&project), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Building source distribution...
    Building wheel from source distribution...
    Successfully built dist/member-0.1.0.tar.gz
    Successfully built dist/member-0.1.0-py3-none-any.whl
    "###);

    project
        .child("dist")
        .child("member-0.1.0.tar.gz")
        .assert(predicate::path::is_file());
    project
        .child("dist")
        .child("member-0.1.0-py3-none-any.whl")
        .assert(predicate::path::is_file());

    // Build all packages.
    uv_snapshot!(&filters, context.build().arg("--all").arg("--no-build-logs").current_dir(&project), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    [PKG] Building source distribution...
    [PKG] Building source distribution...
    [PKG] Building wheel from source distribution...
    [PKG] Building wheel from source distribution...
    Successfully built dist/member-0.1.0.tar.gz
    Successfully built dist/member-0.1.0-py3-none-any.whl
    Successfully built dist/project-0.1.0.tar.gz
    Successfully built dist/project-0.1.0-py3-none-any.whl
    "###);

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
    uv_snapshot!(&filters, context.build().arg("./project").arg("--package").arg("member"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Building source distribution...
    Building wheel from source distribution...
    Successfully built project/dist/member-0.1.0.tar.gz
    Successfully built project/dist/member-0.1.0-py3-none-any.whl
    "###);

    // If a source is provided, discover the workspace from the source.
    uv_snapshot!(&filters, context.build().arg("./project").arg("--all").arg("--no-build-logs"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    [PKG] Building source distribution...
    [PKG] Building source distribution...
    [PKG] Building wheel from source distribution...
    [PKG] Building wheel from source distribution...
    Successfully built project/dist/member-0.1.0.tar.gz
    Successfully built project/dist/member-0.1.0-py3-none-any.whl
    Successfully built project/dist/project-0.1.0.tar.gz
    Successfully built project/dist/project-0.1.0-py3-none-any.whl
    "###);

    // Fail when `--package` is provided without a workspace.
    uv_snapshot!(&filters, context.build().arg("--package").arg("member"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: `--package` was provided, but no workspace was found
      Caused by: No `pyproject.toml` found in current directory or any parent directory
    "###);

    // Fail when `--all` is provided without a workspace.
    uv_snapshot!(&filters, context.build().arg("--all"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: `--all-packages` was provided, but no workspace was found
      Caused by: No `pyproject.toml` found in current directory or any parent directory
    "###);

    // Fail when `--package` is a non-existent member without a workspace.
    uv_snapshot!(&filters, context.build().arg("--package").arg("fail").current_dir(&project), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Package `fail` not found in workspace
    "###);

    Ok(())
}

#[test]
fn build_all_with_failure() -> Result<()> {
    let context = TestContext::new("3.12");
    let filters = context
        .filters()
        .into_iter()
        .chain([
            (r"exit code: 1", "exit status: 1"),
            (r"\\\.", ""),
            (r"\[project\]", "[PKG]"),
            (r"\[member-\w+\]", "[PKG]"),
        ])
        .collect::<Vec<_>>();

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
    uv_snapshot!(&filters, context.build().arg("--all").arg("--no-build-logs").current_dir(&project), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    [PKG] Building source distribution...
    [PKG] Building source distribution...
    [PKG] Building source distribution...
    [PKG] Building wheel from source distribution...
    [PKG] Building wheel from source distribution...
    Successfully built dist/member_a-0.1.0.tar.gz
    Successfully built dist/member_a-0.1.0-py3-none-any.whl
      × Failed to build `member-b @ [TEMP_DIR]/project/packages/member_b`
      ├─▶ The build backend returned an error
      ╰─▶ Call to `setuptools.build_meta.build_sdist` failed (exit status: 1)
          hint: This usually indicates a problem with the package or the build environment.
    Successfully built dist/project-0.1.0.tar.gz
    Successfully built dist/project-0.1.0-py3-none-any.whl
    "###);

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
    let context = TestContext::new("3.12");
    let filters = context
        .filters()
        .into_iter()
        .chain([(r"exit code: 1", "exit status: 1"), (r"\\\.", "")])
        .collect::<Vec<_>>();

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

    uv_snapshot!(&filters, context.build().arg("--build-constraint").arg("constraints.txt").current_dir(&project), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Building source distribution...
      × Failed to build `[TEMP_DIR]/project`
      ├─▶ Failed to resolve requirements from `build-system.requires`
      ├─▶ No solution found when resolving: `hatchling>=1.0`
      ╰─▶ Because you require hatchling>=1.0 and hatchling==0.1.0, we can conclude that your requirements are unsatisfiable.
    "###);

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
fn build_sha() -> Result<()> {
    let context = TestContext::new("3.8");
    let filters = context
        .filters()
        .into_iter()
        .chain([(r"exit code: 1", "exit status: 1"), (r"\\\.", "")])
        .collect::<Vec<_>>();

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

    // Reject an incorrect hash.
    let constraints = project.child("constraints.txt");
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

    uv_snapshot!(&filters, context.build().arg("--build-constraint").arg("constraints.txt").current_dir(&project), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Building source distribution...
      × Failed to build `[TEMP_DIR]/project`
      ├─▶ Failed to install requirements from `build-system.requires`
      ├─▶ Failed to download `hatchling==1.22.4`
      ╰─▶ Hash mismatch for `hatchling==1.22.4`

          Expected:
            sha256:a248cb506794bececcddeddb1678bc722f9cfcacf02f98f7c0af6b9ed893caf2
            sha256:e16da5bfc396af7b29daa3164851dd04991c994083f56cb054b5003675caecdc

          Computed:
            sha256:f56da5bfc396af7b29daa3164851dd04991c994083f56cb054b5003675caecdc
    "###);

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
    uv_snapshot!(&filters, context.build().arg("--build-constraint").arg("constraints.txt").arg("--require-hashes").current_dir(&project), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Building source distribution...
      × Failed to build `[TEMP_DIR]/project`
      ├─▶ Failed to install requirements from `build-system.requires`
      ├─▶ Failed to download `hatchling==1.22.4`
      ╰─▶ Hash mismatch for `hatchling==1.22.4`

          Expected:
            sha256:a248cb506794bececcddeddb1678bc722f9cfcacf02f98f7c0af6b9ed893caf2
            sha256:e16da5bfc396af7b29daa3164851dd04991c994083f56cb054b5003675caecdc

          Computed:
            sha256:f56da5bfc396af7b29daa3164851dd04991c994083f56cb054b5003675caecdc
    "###);

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

    uv_snapshot!(&filters, context.build().arg("--build-constraint").arg("constraints.txt").arg("--require-hashes").current_dir(&project), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Building source distribution...
      × Failed to build `[TEMP_DIR]/project`
      ├─▶ Failed to resolve requirements from `build-system.requires`
      ├─▶ No solution found when resolving: `hatchling`
      ╰─▶ In `--require-hashes` mode, all requirements must be pinned upfront with `==`, but found: `hatchling`
    "###);

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

    uv_snapshot!(&filters, context.build().arg("--build-constraint").arg("constraints.txt").current_dir(&project), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Building source distribution...
    Building wheel from source distribution...
    Successfully built dist/project-0.1.0.tar.gz
    Successfully built dist/project-0.1.0-py3-none-any.whl
    "###);

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
fn build_quiet() -> Result<()> {
    let context = TestContext::new("3.12");

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

    uv_snapshot!(&context.filters(), context.build().arg("project").arg("-q"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    "###);

    Ok(())
}

#[test]
fn build_no_build_logs() -> Result<()> {
    let context = TestContext::new("3.12");

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

    uv_snapshot!(&context.filters(), context.build().arg("project").arg("--no-build-logs"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Building source distribution...
    Building wheel from source distribution...
    Successfully built project/dist/project-0.1.0.tar.gz
    Successfully built project/dist/project-0.1.0-py3-none-any.whl
    "###);

    Ok(())
}

#[test]
fn build_tool_uv_sources() -> Result<()> {
    let context = TestContext::new("3.12");
    let filters = context
        .filters()
        .into_iter()
        .chain([(r"exit code: 1", "exit status: 1"), (r"\\\.", "")])
        .collect::<Vec<_>>();

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

    uv_snapshot!(filters, context.build().current_dir(project.path()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Building source distribution...
    Building wheel from source distribution...
    Successfully built dist/project-0.1.0.tar.gz
    Successfully built dist/project-0.1.0-py3-none-any.whl
    "###);

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

/// Check that we have a working git boundary for builds from source dist to wheel in `dist/`.
#[test]
fn build_git_boundary_in_dist_build() -> Result<()> {
    let context = TestContext::new("3.12");

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

    uv_snapshot!(&context.filters(), context.build().current_dir(project.path()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Building source distribution...
    Building wheel from source distribution...
    Successfully built dist/demo-0.1.0.tar.gz
    Successfully built dist/demo-0.1.0-py3-none-any.whl
    "###);

    // Check that the source file is included
    let reader = File::open(project.join("dist/demo-0.1.0-py3-none-any.whl"))?;
    let mut files: Vec<_> = ZipArchive::new(reader)?
        .file_names()
        .map(ToString::to_string)
        .collect();
    files.sort();
    assert_snapshot!(files.join("\n"), @r###"
    demo-0.1.0.dist-info/METADATA
    demo-0.1.0.dist-info/RECORD
    demo-0.1.0.dist-info/WHEEL
    demo/__init__.py
    "###);

    Ok(())
}

#[test]
fn build_non_package() -> Result<()> {
    let context = TestContext::new("3.12");
    let filters = context
        .filters()
        .into_iter()
        .chain([
            (r"exit code: 1", "exit status: 1"),
            (r"\\\.", ""),
            (r"\[project\]", "[PKG]"),
            (r"\[member\]", "[PKG]"),
        ])
        .collect::<Vec<_>>();

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
    uv_snapshot!(&filters, context.build().arg("--package").arg("member").current_dir(&project), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Package `member` is missing a `build-system`. For example, to build with `setuptools`, add the following to `packages/member/pyproject.toml`:
    ```toml
    [build-system]
    requires = ["setuptools"]
    build-backend = "setuptools.build_meta"
    ```
    "###);

    project
        .child("dist")
        .child("member-0.1.0.tar.gz")
        .assert(predicate::path::missing());
    project
        .child("dist")
        .child("member-0.1.0-py3-none-any.whl")
        .assert(predicate::path::missing());

    // Build all packages.
    uv_snapshot!(&filters, context.build().arg("--all").arg("--no-build-logs").current_dir(&project), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Workspace does not contain any buildable packages. For example, to build `member` with `setuptools`, add a `build-system` to `packages/member/pyproject.toml`:
    ```toml
    [build-system]
    requires = ["setuptools"]
    build-backend = "setuptools.build_meta"
    ```
    "###);

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
    let context = TestContext::new("3.12");

    let built_by_uv = current_dir()?.join("../../scripts/packages/built-by-uv");

    uv_snapshot!(context.build()
        .arg("--preview")
        .arg(&built_by_uv)
        .arg("--out-dir")
        .arg(context.temp_dir.join("output1")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Building source distribution (uv build backend)...
    Building wheel from source distribution (uv build backend)...
    Successfully built output1/built_by_uv-0.1.0.tar.gz
    Successfully built output1/built_by_uv-0.1.0-py3-none-any.whl
    "###);
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
        .arg("--preview")
        .arg(&built_by_uv)
        .arg("--out-dir")
        .arg(context.temp_dir.join("output2"))
        .arg("--sdist"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Building source distribution (uv build backend)...
    Successfully built output2/built_by_uv-0.1.0.tar.gz
    "###);
    context
        .temp_dir
        .child("output2")
        .child("built_by_uv-0.1.0.tar.gz")
        .assert(predicate::path::is_file());

    uv_snapshot!(context.build()
        .arg("--preview")
        .arg(&built_by_uv)
        .arg("--out-dir")
        .arg(context.temp_dir.join("output3"))
        .arg("--wheel"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Building wheel (uv build backend)...
    Successfully built output3/built_by_uv-0.1.0-py3-none-any.whl
    "###);
    context
        .temp_dir
        .child("output3")
        .child("built_by_uv-0.1.0-py3-none-any.whl")
        .assert(predicate::path::is_file());

    uv_snapshot!(context.build()
        .arg("--preview")
        .arg(&built_by_uv)
        .arg("--out-dir")
        .arg(context.temp_dir.join("output4"))
        .arg("--sdist")
        .arg("--wheel"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Building source distribution (uv build backend)...
    Building wheel (uv build backend)...
    Successfully built output4/built_by_uv-0.1.0.tar.gz
    Successfully built output4/built_by_uv-0.1.0-py3-none-any.whl
    "###);
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

/// Test the `--list` option.
#[test]
fn build_list_files() -> Result<()> {
    let context = TestContext::new("3.12");

    let built_by_uv = current_dir()?.join("../../scripts/packages/built-by-uv");

    // By default, we build the wheel from the source dist, which we need to do even for the list
    // task.
    uv_snapshot!(context.build()
        .arg("--preview")
        .arg(&built_by_uv)
        .arg("--out-dir")
        .arg(context.temp_dir.join("output1"))
        .arg("--list"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Building built_by_uv-0.1.0.tar.gz will include the following files:
    built_by_uv-0.1.0/LICENSE-APACHE (LICENSE-APACHE)
    built_by_uv-0.1.0/LICENSE-MIT (LICENSE-MIT)
    built_by_uv-0.1.0/PKG-INFO (generated)
    built_by_uv-0.1.0/README.md (README.md)
    built_by_uv-0.1.0/assets/data.csv (assets/data.csv)
    built_by_uv-0.1.0/header/built_by_uv.h (header/built_by_uv.h)
    built_by_uv-0.1.0/pyproject.toml (pyproject.toml)
    built_by_uv-0.1.0/scripts/whoami.sh (scripts/whoami.sh)
    built_by_uv-0.1.0/src/built_by_uv/__init__.py (src/built_by_uv/__init__.py)
    built_by_uv-0.1.0/src/built_by_uv/arithmetic/__init__.py (src/built_by_uv/arithmetic/__init__.py)
    built_by_uv-0.1.0/src/built_by_uv/arithmetic/circle.py (src/built_by_uv/arithmetic/circle.py)
    built_by_uv-0.1.0/src/built_by_uv/arithmetic/pi.txt (src/built_by_uv/arithmetic/pi.txt)
    built_by_uv-0.1.0/src/built_by_uv/build-only.h (src/built_by_uv/build-only.h)
    built_by_uv-0.1.0/src/built_by_uv/cli.py (src/built_by_uv/cli.py)
    built_by_uv-0.1.0/third-party-licenses/PEP-401.txt (third-party-licenses/PEP-401.txt)
    Building built_by_uv-0.1.0-py3-none-any.whl will include the following files:
    built_by_uv-0.1.0.data/data/data.csv (assets/data.csv)
    built_by_uv-0.1.0.data/headers/built_by_uv.h (header/built_by_uv.h)
    built_by_uv-0.1.0.data/scripts/whoami.sh (scripts/whoami.sh)
    built_by_uv-0.1.0.dist-info/METADATA (generated)
    built_by_uv-0.1.0.dist-info/WHEEL (generated)
    built_by_uv-0.1.0.dist-info/entry_points.txt (generated)
    built_by_uv-0.1.0.dist-info/licenses/LICENSE-APACHE (LICENSE-APACHE)
    built_by_uv-0.1.0.dist-info/licenses/LICENSE-MIT (LICENSE-MIT)
    built_by_uv-0.1.0.dist-info/licenses/third-party-licenses/PEP-401.txt (third-party-licenses/PEP-401.txt)
    built_by_uv/__init__.py (src/built_by_uv/__init__.py)
    built_by_uv/arithmetic/__init__.py (src/built_by_uv/arithmetic/__init__.py)
    built_by_uv/arithmetic/circle.py (src/built_by_uv/arithmetic/circle.py)
    built_by_uv/arithmetic/pi.txt (src/built_by_uv/arithmetic/pi.txt)
    built_by_uv/cli.py (src/built_by_uv/cli.py)

    ----- stderr -----
    Building source distribution (uv build backend)...
    Successfully built output1/built_by_uv-0.1.0.tar.gz
    "###);
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
        .arg("--preview")
        .arg(&built_by_uv)
        .arg("--out-dir")
        .arg(context.temp_dir.join("output2"))
        .arg("--list")
        .arg("--sdist")
        .arg("--wheel"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Building built_by_uv-0.1.0.tar.gz will include the following files:
    built_by_uv-0.1.0/LICENSE-APACHE (LICENSE-APACHE)
    built_by_uv-0.1.0/LICENSE-MIT (LICENSE-MIT)
    built_by_uv-0.1.0/PKG-INFO (generated)
    built_by_uv-0.1.0/README.md (README.md)
    built_by_uv-0.1.0/assets/data.csv (assets/data.csv)
    built_by_uv-0.1.0/header/built_by_uv.h (header/built_by_uv.h)
    built_by_uv-0.1.0/pyproject.toml (pyproject.toml)
    built_by_uv-0.1.0/scripts/whoami.sh (scripts/whoami.sh)
    built_by_uv-0.1.0/src/built_by_uv/__init__.py (src/built_by_uv/__init__.py)
    built_by_uv-0.1.0/src/built_by_uv/arithmetic/__init__.py (src/built_by_uv/arithmetic/__init__.py)
    built_by_uv-0.1.0/src/built_by_uv/arithmetic/circle.py (src/built_by_uv/arithmetic/circle.py)
    built_by_uv-0.1.0/src/built_by_uv/arithmetic/pi.txt (src/built_by_uv/arithmetic/pi.txt)
    built_by_uv-0.1.0/src/built_by_uv/build-only.h (src/built_by_uv/build-only.h)
    built_by_uv-0.1.0/src/built_by_uv/cli.py (src/built_by_uv/cli.py)
    built_by_uv-0.1.0/third-party-licenses/PEP-401.txt (third-party-licenses/PEP-401.txt)
    Building built_by_uv-0.1.0-py3-none-any.whl will include the following files:
    built_by_uv-0.1.0.data/data/data.csv (assets/data.csv)
    built_by_uv-0.1.0.data/headers/built_by_uv.h (header/built_by_uv.h)
    built_by_uv-0.1.0.data/scripts/whoami.sh (scripts/whoami.sh)
    built_by_uv-0.1.0.dist-info/METADATA (generated)
    built_by_uv-0.1.0.dist-info/WHEEL (generated)
    built_by_uv-0.1.0.dist-info/entry_points.txt (generated)
    built_by_uv-0.1.0.dist-info/licenses/LICENSE-APACHE (LICENSE-APACHE)
    built_by_uv-0.1.0.dist-info/licenses/LICENSE-MIT (LICENSE-MIT)
    built_by_uv-0.1.0.dist-info/licenses/third-party-licenses/PEP-401.txt (third-party-licenses/PEP-401.txt)
    built_by_uv/__init__.py (src/built_by_uv/__init__.py)
    built_by_uv/arithmetic/__init__.py (src/built_by_uv/arithmetic/__init__.py)
    built_by_uv/arithmetic/circle.py (src/built_by_uv/arithmetic/circle.py)
    built_by_uv/arithmetic/pi.txt (src/built_by_uv/arithmetic/pi.txt)
    built_by_uv/cli.py (src/built_by_uv/cli.py)

    ----- stderr -----
    "###);
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
    let context = TestContext::new("3.12");

    let built_by_uv = current_dir()?.join("../../scripts/packages/built-by-uv");

    let mut filters = context.filters();
    // In CI, we run with link mode settings.
    filters.push(("--link-mode <LINK_MODE> ", ""));
    uv_snapshot!(filters, context.build()
        .arg("--preview")
        .arg(&built_by_uv)
        .arg("--out-dir")
        .arg(context.temp_dir.join("output1"))
        .arg("--list")
        .arg("--force-pep517"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: the argument '--list' cannot be used with '--force-pep517'

    Usage: uv build --cache-dir [CACHE_DIR] --out-dir <OUT_DIR> --exclude-newer <EXCLUDE_NEWER> <SRC>

    For more information, try '--help'.
    "###);

    // Not a uv build backend package, we can't list it.
    let anyio_local = current_dir()?.join("../../scripts/packages/anyio_local");
    let mut filters = context.filters();
    // Windows normalization
    filters.push(("/crates/uv/../../", "/"));
    uv_snapshot!(filters, context.build()
        .arg("--preview")
        .arg(&anyio_local)
        .arg("--out-dir")
        .arg(context.temp_dir.join("output2"))
        .arg("--list"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
      × Failed to build `[WORKSPACE]/scripts/packages/anyio_local`
      ╰─▶ Can only use `--list` with the uv backend
    "###);
    Ok(())
}

#[test]
fn build_version_mismatch() -> Result<()> {
    let context = TestContext::new("3.12");
    let anyio_local = current_dir()?.join("../../scripts/packages/anyio_local");
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
        .arg(context.temp_dir.path()), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Building wheel from source distribution...
      × Failed to build `[TEMP_DIR]/anyio-1.2.3.tar.gz`
      ╰─▶ The source distribution declares version 1.2.3, but the wheel declares version 4.3.0+foo
    "###);
    Ok(())
}

#[cfg(unix)] // Symlinks aren't universally available on windows.
#[test]
fn build_with_symlink() -> Result<()> {
    let context = TestContext::new("3.12");
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
    std::os::unix::fs::symlink(
        context.temp_dir.child("pyproject.toml.real"),
        context.temp_dir.child("pyproject.toml"),
    )?;
    context
        .temp_dir
        .child("src/softlinked/__init__.py")
        .touch()?;
    uv_snapshot!(context.filters(), context.build(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Building source distribution...
    Building wheel from source distribution...
    Successfully built dist/softlinked-0.1.0.tar.gz
    Successfully built dist/softlinked-0.1.0-py3-none-any.whl
    "###);
    Ok(())
}

#[test]
fn build_with_hardlink() -> Result<()> {
    let context = TestContext::new("3.12");
    context
        .temp_dir
        .child("pyproject.toml.real")
        .write_str(indoc! {r#"
            [project]
            name = "hardlinked"
            version = "0.1.0"
            requires-python = ">=3.12"

            [build-system]
            requires = ["hatchling"]
            build-backend = "hatchling.build"
    "#})?;
    fs_err::hard_link(
        context.temp_dir.child("pyproject.toml.real"),
        context.temp_dir.child("pyproject.toml"),
    )?;
    context
        .temp_dir
        .child("src/hardlinked/__init__.py")
        .touch()?;
    uv_snapshot!(context.filters(), context.build(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Building source distribution...
    Building wheel from source distribution...
    Successfully built dist/hardlinked-0.1.0.tar.gz
    Successfully built dist/hardlinked-0.1.0-py3-none-any.whl
    "###);
    Ok(())
}

/// This is bad project layout that is allowed: A project that defines PEP 621 metadata, but no
/// PEP 517 build system not a setup.py, so we fallback to setuptools implicitly.
#[test]
fn build_unconfigured_setuptools() -> Result<()> {
    let context = TestContext::new("3.12");
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
    uv_snapshot!(context.filters(), context.pip_install().arg("."), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + greet==0.1.0 (from file://[TEMP_DIR]/)
    "###);

    uv_snapshot!(context.filters(), Command::new(context.interpreter()).arg("-c").arg("import greet"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Greetings!

    ----- stderr -----
    "###);
    Ok(())
}

/// In a project layout with a virtual root, an easy mistake to make is running `uv pip install .`
/// in the root.
#[test]
fn build_workspace_virtual_root() -> Result<()> {
    let context = TestContext::new("3.12");
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
            [tool.uv.workspace]
            members = ["packages/*"]
    "#})?;

    uv_snapshot!(context.filters(), context.build().arg("--no-build-logs"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Building source distribution...
    warning: `[TEMP_DIR]/` appears to be a workspace root without a Python project; consider using `uv sync` to install the workspace, or add a `[build-system]` table to `pyproject.toml`
    Building wheel from source distribution...
    Successfully built dist/cache-0.0.0.tar.gz
    Successfully built dist/UNKNOWN-0.0.0-py3-none-any.whl
    ");
    Ok(())
}

/// There is a `pyproject.toml`, but it does not define any build information nor is there a
/// `setup.{py,cfg}`.
#[test]
fn build_pyproject_toml_not_a_project() -> Result<()> {
    let context = TestContext::new("3.12");
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {"
            # Some other content we don't know about
            [tool.black]
            line-length = 88
    "})?;

    uv_snapshot!(context.filters(), context.build().arg("--no-build-logs"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Building source distribution...
    warning: `[TEMP_DIR]/` does not appear to be a Python project, as the `pyproject.toml` does not include a `[build-system]` table, and neither `setup.py` nor `setup.cfg` are present in the directory
    Building wheel from source distribution...
    Successfully built dist/cache-0.0.0.tar.gz
    Successfully built dist/UNKNOWN-0.0.0-py3-none-any.whl
    ");
    Ok(())
}

#[test]
fn build_with_nonnormalized_name() -> Result<()> {
    let context = TestContext::new("3.12");
    let filters = context
        .filters()
        .into_iter()
        .chain([(r"exit code: 1", "exit status: 1"), (r"\\\.", "")])
        .collect::<Vec<_>>();

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
    uv_snapshot!(&filters, context.build().arg("--no-build-logs").current_dir(&project), @r"
    success: true
    exit_code: 0
    ----- stdout -----

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
