use std::collections::BTreeMap;
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Result;
use assert_cmd::assert::OutputAssertExt;
use assert_fs::fixture::{FileWriteStr, PathChild};
use assert_fs::prelude::FileTouch;
use indoc::indoc;
use insta::{assert_json_snapshot, assert_snapshot};
use serde::{Deserialize, Serialize};

use crate::common::{copy_dir_ignore, make_project, uv_snapshot, TestContext};

mod common;

/// `pip install --preview -e <current dir>`
fn install_workspace(context: &TestContext, current_dir: &Path) -> Command {
    let mut command = context.pip_install();
    command.arg("-e").arg(current_dir);
    command
}

fn workspaces_dir() -> PathBuf {
    env::current_dir()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("scripts")
        .join("workspaces")
}

#[test]
fn test_albatross_in_examples_bird_feeder() {
    let context = TestContext::new("3.12");
    let current_dir = workspaces_dir()
        .join("albatross-in-example")
        .join("examples")
        .join("bird-feeder");

    uv_snapshot!(context.filters(), install_workspace(&context, &current_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.3.0
     + bird-feeder==1.0.0 (from file://[WORKSPACE]/scripts/workspaces/albatross-in-example/examples/bird-feeder)
     + idna==3.6
     + sniffio==1.3.1
    "###
    );

    context.assert_file(current_dir.join("check_installed_bird_feeder.py"));
    uv_snapshot!(context.filters(), install_workspace(&context, &current_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    "###
    );

    context.assert_file(current_dir.join("check_installed_bird_feeder.py"));
}

#[test]
fn test_albatross_in_examples() {
    let context = TestContext::new("3.12");
    let current_dir = workspaces_dir().join("albatross-in-example");

    uv_snapshot!(context.filters(), install_workspace(&context, &current_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + albatross==0.1.0 (from file://[WORKSPACE]/scripts/workspaces/albatross-in-example)
     + tqdm==4.66.2
    "###
    );

    context.assert_file(current_dir.join("check_installed_albatross.py"));
    uv_snapshot!(context.filters(), install_workspace(&context, &current_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    "###
    );

    context.assert_file(current_dir.join("check_installed_albatross.py"));
}

#[test]
fn test_albatross_just_project() {
    let context = TestContext::new("3.12");
    let current_dir = workspaces_dir().join("albatross-just-project");

    uv_snapshot!(context.filters(), install_workspace(&context, &current_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + albatross==0.1.0 (from file://[WORKSPACE]/scripts/workspaces/albatross-just-project)
     + tqdm==4.66.2
    "###
    );

    context.assert_file(current_dir.join("check_installed_albatross.py"));
    uv_snapshot!(context.filters(), install_workspace(&context, &current_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    "###
    );

    context.assert_file(current_dir.join("check_installed_albatross.py"));
}

#[test]
fn test_albatross_project_in_excluded() {
    let context = TestContext::new("3.12");
    let current_dir = workspaces_dir()
        .join("albatross-project-in-excluded")
        .join("excluded")
        .join("bird-feeder");

    uv_snapshot!(context.filters(), install_workspace(&context, &current_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.3.0
     + bird-feeder==1.0.0 (from file://[WORKSPACE]/scripts/workspaces/albatross-project-in-excluded/excluded/bird-feeder)
     + idna==3.6
     + sniffio==1.3.1
    "###
    );

    context.assert_file(current_dir.join("check_installed_bird_feeder.py"));
    uv_snapshot!(context.filters(), install_workspace(&context, &current_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    "###
    );

    context.assert_file(current_dir.join("check_installed_bird_feeder.py"));

    let current_dir = workspaces_dir()
        .join("albatross-project-in-excluded")
        .join("packages")
        .join("seeds");
    uv_snapshot!(context.filters(), install_workspace(&context, &current_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + seeds==1.0.0 (from file://[WORKSPACE]/scripts/workspaces/albatross-project-in-excluded/packages/seeds)
    "###
    );
}

#[test]
fn test_albatross_root_workspace() {
    let context = TestContext::new("3.12");
    let current_dir = workspaces_dir().join("albatross-root-workspace");

    uv_snapshot!(context.filters(), install_workspace(&context, &current_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
    Prepared 7 packages in [TIME]
    Installed 7 packages in [TIME]
     + albatross==0.1.0 (from file://[WORKSPACE]/scripts/workspaces/albatross-root-workspace)
     + anyio==4.3.0
     + bird-feeder==1.0.0 (from file://[WORKSPACE]/scripts/workspaces/albatross-root-workspace/packages/bird-feeder)
     + idna==3.6
     + seeds==1.0.0 (from file://[WORKSPACE]/scripts/workspaces/albatross-root-workspace/packages/seeds)
     + sniffio==1.3.1
     + tqdm==4.66.2
    "###
    );

    context.assert_file(current_dir.join("check_installed_albatross.py"));
    uv_snapshot!(context.filters(), install_workspace(&context, &current_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    "###
    );

    context.assert_file(current_dir.join("check_installed_albatross.py"));
}

#[test]
fn test_albatross_root_workspace_bird_feeder() {
    let context = TestContext::new("3.12");
    let current_dir = workspaces_dir()
        .join("albatross-root-workspace")
        .join("packages")
        .join("bird-feeder");

    uv_snapshot!(context.filters(), install_workspace(&context, &current_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Prepared 5 packages in [TIME]
    Installed 5 packages in [TIME]
     + anyio==4.3.0
     + bird-feeder==1.0.0 (from file://[WORKSPACE]/scripts/workspaces/albatross-root-workspace/packages/bird-feeder)
     + idna==3.6
     + seeds==1.0.0 (from file://[WORKSPACE]/scripts/workspaces/albatross-root-workspace/packages/seeds)
     + sniffio==1.3.1
    "###
    );

    context.assert_file(current_dir.join("check_installed_bird_feeder.py"));
    uv_snapshot!(context.filters(), install_workspace(&context, &current_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    "###
    );

    context.assert_file(current_dir.join("check_installed_bird_feeder.py"));
}

#[test]
fn test_albatross_root_workspace_albatross() {
    let context = TestContext::new("3.12");
    let current_dir = workspaces_dir()
        .join("albatross-root-workspace")
        .join("packages")
        .join("bird-feeder");

    uv_snapshot!(context.filters(), install_workspace(&context, &current_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Prepared 5 packages in [TIME]
    Installed 5 packages in [TIME]
     + anyio==4.3.0
     + bird-feeder==1.0.0 (from file://[WORKSPACE]/scripts/workspaces/albatross-root-workspace/packages/bird-feeder)
     + idna==3.6
     + seeds==1.0.0 (from file://[WORKSPACE]/scripts/workspaces/albatross-root-workspace/packages/seeds)
     + sniffio==1.3.1
    "###
    );

    context.assert_file(current_dir.join("check_installed_albatross.py"));
    uv_snapshot!(context.filters(), install_workspace(&context, &current_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    "###
    );

    context.assert_file(current_dir.join("check_installed_albatross.py"));
}

#[test]
fn test_albatross_virtual_workspace() {
    let context = TestContext::new("3.12");
    let current_dir = workspaces_dir()
        .join("albatross-virtual-workspace")
        .join("packages")
        .join("bird-feeder");

    uv_snapshot!(context.filters(), install_workspace(&context, &current_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Prepared 5 packages in [TIME]
    Installed 5 packages in [TIME]
     + anyio==4.3.0
     + bird-feeder==1.0.0 (from file://[WORKSPACE]/scripts/workspaces/albatross-virtual-workspace/packages/bird-feeder)
     + idna==3.6
     + seeds==1.0.0 (from file://[WORKSPACE]/scripts/workspaces/albatross-virtual-workspace/packages/seeds)
     + sniffio==1.3.1
    "###
    );

    context.assert_file(current_dir.join("check_installed_bird_feeder.py"));
    uv_snapshot!(context.filters(), install_workspace(&context, &current_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    "###
    );

    context.assert_file(current_dir.join("check_installed_bird_feeder.py"));
}

/// Check that `uv run --package` works in a virtual workspace.
#[test]
fn test_uv_run_with_package_virtual_workspace() -> Result<()> {
    let context = TestContext::new("3.12");
    let work_dir = context.temp_dir.join("albatross-virtual-workspace");

    copy_dir_ignore(
        workspaces_dir().join("albatross-virtual-workspace"),
        &work_dir,
    )?;

    let mut filters = context.filters();
    filters.push((
        r"Using Python 3.12.\[X\] interpreter at: .*",
        "Using Python 3.12.[X] interpreter at: [PYTHON]",
    ));

    // Run from the `bird-feeder` member.
    uv_snapshot!(filters, context
        .run()
        .arg("--package")
        .arg("bird-feeder")
        .arg("packages/bird-feeder/check_installed_bird_feeder.py")
        .current_dir(&work_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Success

    ----- stderr -----
    warning: `VIRTUAL_ENV=[VENV]/` does not match the project environment path `.venv` and will be ignored
    Using Python 3.12.[X] interpreter at: [PYTHON]
    Creating virtualenv at: .venv
    Resolved 8 packages in [TIME]
    Prepared 5 packages in [TIME]
    Installed 5 packages in [TIME]
     + anyio==4.3.0
     + bird-feeder==1.0.0 (from file://[TEMP_DIR]/albatross-virtual-workspace/packages/bird-feeder)
     + idna==3.6
     + seeds==1.0.0 (from file://[TEMP_DIR]/albatross-virtual-workspace/packages/seeds)
     + sniffio==1.3.1
    "###
    );

    uv_snapshot!(context.filters(), universal_windows_filters=true, context
        .run()
        .arg("--package")
        .arg("albatross")
        .arg("packages/albatross/check_installed_albatross.py")
        .current_dir(&work_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Success

    ----- stderr -----
    warning: `VIRTUAL_ENV=[VENV]/` does not match the project environment path `.venv` and will be ignored
    Resolved 8 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + albatross==0.1.0 (from file://[TEMP_DIR]/albatross-virtual-workspace/packages/albatross)
     + tqdm==4.66.2
    "###
    );

    Ok(())
}

/// Check that `uv run` works from a virtual workspace root, which should sync all packages in the
/// workspace.
#[test]
fn test_uv_run_virtual_workspace_root() -> Result<()> {
    let context = TestContext::new("3.12");
    let work_dir = context.temp_dir.join("albatross-virtual-workspace");

    copy_dir_ignore(
        workspaces_dir().join("albatross-virtual-workspace"),
        &work_dir,
    )?;

    uv_snapshot!(context.filters(), universal_windows_filters=true, context
        .run()
        .arg("packages/albatross/check_installed_albatross.py")
        .current_dir(&work_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Success

    ----- stderr -----
    warning: `VIRTUAL_ENV=[VENV]/` does not match the project environment path `.venv` and will be ignored
    Using Python 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtualenv at: .venv
    Resolved 8 packages in [TIME]
    Prepared 7 packages in [TIME]
    Installed 7 packages in [TIME]
     + albatross==0.1.0 (from file://[TEMP_DIR]/albatross-virtual-workspace/packages/albatross)
     + anyio==4.3.0
     + bird-feeder==1.0.0 (from file://[TEMP_DIR]/albatross-virtual-workspace/packages/bird-feeder)
     + idna==3.6
     + seeds==1.0.0 (from file://[TEMP_DIR]/albatross-virtual-workspace/packages/seeds)
     + sniffio==1.3.1
     + tqdm==4.66.2
    "###
    );

    Ok(())
}

/// Check that `uv run --package` works in a root workspace.
#[test]
fn test_uv_run_with_package_root_workspace() -> Result<()> {
    let context = TestContext::new("3.12");
    let work_dir = context.temp_dir.join("albatross-root-workspace");

    copy_dir_ignore(workspaces_dir().join("albatross-root-workspace"), &work_dir)?;

    let mut filters = context.filters();
    filters.push((
        r"Using Python 3.12.\[X\] interpreter at: .*",
        "Using Python 3.12.[X] interpreter at: [PYTHON]",
    ));

    uv_snapshot!(filters, context
        .run()
        .arg("--package")
        .arg("bird-feeder")
        .arg("packages/bird-feeder/check_installed_bird_feeder.py")
        .current_dir(&work_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Success

    ----- stderr -----
    warning: `VIRTUAL_ENV=[VENV]/` does not match the project environment path `.venv` and will be ignored
    Using Python 3.12.[X] interpreter at: [PYTHON]
    Creating virtualenv at: .venv
    Resolved 8 packages in [TIME]
    Prepared 5 packages in [TIME]
    Installed 5 packages in [TIME]
     + anyio==4.3.0
     + bird-feeder==1.0.0 (from file://[TEMP_DIR]/albatross-root-workspace/packages/bird-feeder)
     + idna==3.6
     + seeds==1.0.0 (from file://[TEMP_DIR]/albatross-root-workspace/packages/seeds)
     + sniffio==1.3.1
    "###
    );

    uv_snapshot!(context.filters(), universal_windows_filters=true, context
        .run()
        .arg("--package")
        .arg("albatross")
        .arg("check_installed_albatross.py")
        .current_dir(&work_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Success

    ----- stderr -----
    warning: `VIRTUAL_ENV=[VENV]/` does not match the project environment path `.venv` and will be ignored
    Resolved 8 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + albatross==0.1.0 (from file://[TEMP_DIR]/albatross-root-workspace)
     + tqdm==4.66.2
    "###
    );

    Ok(())
}

/// Check that `uv run --isolated` creates isolated virtual environments.
#[test]
fn test_uv_run_isolate() -> Result<()> {
    let context = TestContext::new("3.12");
    let work_dir = context.temp_dir.join("albatross-root-workspace");

    copy_dir_ignore(workspaces_dir().join("albatross-root-workspace"), &work_dir)?;

    let mut filters = context.filters();
    filters.push((
        r"Using Python 3.12.\[X\] interpreter at: .*",
        "Using Python 3.12.[X] interpreter at: [PYTHON]",
    ));

    // Install the root package.
    uv_snapshot!(context.filters(), universal_windows_filters=true, context
        .run()
        .arg("--package")
        .arg("albatross")
        .arg("check_installed_albatross.py")
        .current_dir(&work_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Success

    ----- stderr -----
    warning: `VIRTUAL_ENV=[VENV]/` does not match the project environment path `.venv` and will be ignored
    Using Python 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtualenv at: .venv
    Resolved 8 packages in [TIME]
    Prepared 7 packages in [TIME]
    Installed 7 packages in [TIME]
     + albatross==0.1.0 (from file://[TEMP_DIR]/albatross-root-workspace)
     + anyio==4.3.0
     + bird-feeder==1.0.0 (from file://[TEMP_DIR]/albatross-root-workspace/packages/bird-feeder)
     + idna==3.6
     + seeds==1.0.0 (from file://[TEMP_DIR]/albatross-root-workspace/packages/seeds)
     + sniffio==1.3.1
     + tqdm==4.66.2
    "###
    );

    // Run in `bird-feeder`. We shouldn't be able to import `albatross`, but we _can_ due to our
    // virtual environment semantics. Specifically, we only make the changes necessary to run a
    // given command, so we don't remove `albatross` from the environment.
    uv_snapshot!(filters, context
        .run()
        .arg("--package")
        .arg("bird-feeder")
        .arg("check_installed_albatross.py")
        .current_dir(&work_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Success

    ----- stderr -----
    warning: `VIRTUAL_ENV=[VENV]/` does not match the project environment path `.venv` and will be ignored
    Resolved 8 packages in [TIME]
    Audited 5 packages in [TIME]
    "###
    );

    // If we `--isolated`, though, we use an isolated virtual environment, so `albatross` is not
    // available.
    // TODO(charlie): This should show the resolution output, but `--isolated` is coupled to
    // `--no-project` right now.
    uv_snapshot!(filters, context
        .run()
        .arg("--isolated")
        .arg("--package")
        .arg("bird-feeder")
        .arg("check_installed_albatross.py")
        .current_dir(&work_dir), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 8 packages in [TIME]
    Installed 5 packages in [TIME]
     + anyio==4.3.0
     + bird-feeder==1.0.0 (from file://[TEMP_DIR]/albatross-root-workspace/packages/bird-feeder)
     + idna==3.6
     + seeds==1.0.0 (from file://[TEMP_DIR]/albatross-root-workspace/packages/seeds)
     + sniffio==1.3.1
    Traceback (most recent call last):
      File "[TEMP_DIR]/albatross-root-workspace/check_installed_albatross.py", line 1, in <module>
        from albatross import fly
    ModuleNotFoundError: No module named 'albatross'
    "###
    );

    Ok(())
}

/// Check that the resolution is the same no matter where in the workspace we are.
fn workspace_lock_idempotence(workspace: &str, subdirectories: &[&str]) -> Result<()> {
    let mut shared_lock = None;

    for dir in subdirectories {
        let context = TestContext::new("3.12");
        let work_dir = context.temp_dir.join(workspace);

        copy_dir_ignore(workspaces_dir().join(workspace), &work_dir)?;

        context
            .lock()
            .current_dir(work_dir.join(dir))
            .assert()
            .success();

        let lock = fs_err::read_to_string(work_dir.join("uv.lock"))?;
        // Check the lockfile is the same for all resolutions.
        if let Some(shared_lock) = &shared_lock {
            assert_eq!(shared_lock, &lock);
        } else {
            shared_lock = Some(lock);
        }
    }
    Ok(())
}

/// Check that the resolution is the same no matter where in the workspace we are.
#[test]
fn workspace_lock_idempotence_root_workspace() -> Result<()> {
    workspace_lock_idempotence(
        "albatross-root-workspace",
        &[".", "packages/bird-feeder", "packages/seeds"],
    )?;
    Ok(())
}

/// Check that the resolution is the same no matter where in the workspace we are, and that locking
/// works even if there is no root project.
#[test]
fn workspace_lock_idempotence_virtual_workspace() -> Result<()> {
    workspace_lock_idempotence(
        "albatross-virtual-workspace",
        &[
            ".",
            "packages/albatross",
            "packages/bird-feeder",
            "packages/seeds",
        ],
    )?;
    Ok(())
}

/// Extract just the sources from the lockfile, to test path resolution.
#[derive(Deserialize, Serialize)]
struct SourceLock {
    package: Vec<Package>,
}

impl SourceLock {
    fn sources(self) -> BTreeMap<String, toml::Value> {
        self.package
            .into_iter()
            .map(|package| (package.name, package.source))
            .collect()
    }
}

#[derive(Deserialize, Serialize)]
struct Package {
    name: String,
    source: toml::Value,
}

/// Test path dependencies from one workspace into another.
///
/// We have a main workspace with packages `a` and `b`, and a second workspace with `c`, `d` and
/// `e`. We have `a -> b`, `b -> c`, `c -> d`. `e` should not be installed.
#[test]
fn workspace_to_workspace_paths_dependencies() -> Result<()> {
    let context = TestContext::new("3.12");

    // Build the main workspace ...
    let main_workspace = context.temp_dir.child("main-workspace");
    main_workspace
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [tool.uv.workspace]
        members = ["packages/*"]
    "#})?;

    // ... with a  ...
    let deps = indoc! {r#"
        dependencies = ["b"]

        [tool.uv.sources]
        b = { workspace = true }
    "#};
    make_project(&main_workspace.join("packages").join("a"), "a", deps)?;

    // ... and b.
    let deps = indoc! {r#"
        dependencies = ["c"]

        [tool.uv.sources]
        c = { path = "../../../other-workspace/packages/c", editable = true }
    "#};
    make_project(&main_workspace.join("packages").join("b"), "b", deps)?;

    // Build the second workspace ...
    let other_workspace = context.temp_dir.child("other-workspace");
    other_workspace
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [tool.uv.workspace]
        members = ["packages/*"]
    "#})?;

    // ... with c  ...
    let deps = indoc! {r#"
        dependencies = ["d"]

        [tool.uv.sources]
        d = { workspace = true }
    "#};
    make_project(&other_workspace.join("packages").join("c"), "c", deps)?;

    // ... and d ...
    let deps = indoc! {r"
        dependencies = []
    "};
    make_project(&other_workspace.join("packages").join("d"), "d", deps)?;

    // ... and e.
    let deps = indoc! {r#"
        dependencies = ["numpy>=2.0.0,<3"]
    "#};
    make_project(&other_workspace.join("packages").join("e"), "e", deps)?;

    uv_snapshot!(context.filters(), context.lock().current_dir(&main_workspace), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using Python 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 4 packages in [TIME]
    "###
    );

    let lock: SourceLock =
        toml::from_str(&fs_err::read_to_string(main_workspace.join("uv.lock"))?)?;

    assert_json_snapshot!(lock.sources(), @r###"
    {
      "a": {
        "editable": "packages/a"
      },
      "b": {
        "editable": "packages/b"
      },
      "c": {
        "editable": "../other-workspace/packages/c"
      },
      "d": {
        "editable": "../other-workspace/packages/d"
      }
    }
    "###);

    Ok(())
}

/// Ensure that workspace discovery errors if a member is missing a `pyproject.toml`.
#[test]
fn workspace_empty_member() -> Result<()> {
    let context = TestContext::new("3.12");

    // Build the main workspace ...
    let workspace = context.temp_dir.child("workspace");
    workspace.child("pyproject.toml").write_str(indoc! {r#"
        [tool.uv.workspace]
        members = ["packages/*"]
    "#})?;

    // ... with a  ...
    let deps = indoc! {r#"
        dependencies = ["b"]

        [tool.uv.sources]
        b = { workspace = true }
    "#};
    make_project(&workspace.join("packages").join("a"), "a", deps)?;

    // ... and b.
    let deps = indoc! {r"
    "};
    make_project(&workspace.join("packages").join("b"), "b", deps)?;

    // ... and an empty c.
    fs_err::create_dir_all(workspace.join("packages").join("c"))?;

    uv_snapshot!(context.filters(), context.lock().current_dir(&workspace), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Workspace member `[TEMP_DIR]/workspace/packages/c` is missing a `pyproject.toml` (matches: `packages/*`)
    "###
    );

    Ok(())
}

/// Ensure that workspace discovery ignores hidden directories.
#[test]
fn workspace_hidden_files() -> Result<()> {
    let context = TestContext::new("3.12");

    // Build the main workspace ...
    let workspace = context.temp_dir.child("workspace");
    workspace.child("pyproject.toml").write_str(indoc! {r#"
        [tool.uv.workspace]
        members = ["packages/*"]
    "#})?;

    // ... with a  ...
    let deps = indoc! {r#"
        dependencies = ["b"]

        [tool.uv.sources]
        b = { workspace = true }
    "#};
    make_project(&workspace.join("packages").join("a"), "a", deps)?;

    // ... and b.
    let deps = indoc! {r"
    "};
    make_project(&workspace.join("packages").join("b"), "b", deps)?;

    // ... and a hidden c.
    fs_err::create_dir_all(workspace.join("packages").join(".c"))?;

    uv_snapshot!(context.filters(), context.lock().current_dir(&workspace), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using Python 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 2 packages in [TIME]
    "###
    );

    let lock: SourceLock = toml::from_str(&fs_err::read_to_string(workspace.join("uv.lock"))?)?;

    assert_json_snapshot!(lock.sources(), @r###"
    {
      "a": {
        "editable": "packages/a"
      },
      "b": {
        "editable": "packages/b"
      }
    }
    "###);

    Ok(())
}

/// Ensure that workspace discovery accepts valid hidden directories.
#[test]
fn workspace_hidden_member() -> Result<()> {
    let context = TestContext::new("3.12");

    // Build the main workspace ...
    let workspace = context.temp_dir.child("workspace");
    workspace.child("pyproject.toml").write_str(indoc! {r#"
        [tool.uv.workspace]
        members = ["packages/*"]
    "#})?;

    // ... with a  ...
    let deps = indoc! {r#"
        dependencies = ["b"]

        [tool.uv.sources]
        b = { workspace = true }
    "#};
    make_project(&workspace.join("packages").join("a"), "a", deps)?;

    // ... and b.
    let deps = indoc! {r#"
        dependencies = ["c"]

        [tool.uv.sources]
        c = { workspace = true }
    "#};
    make_project(&workspace.join("packages").join("b"), "b", deps)?;

    // ... and a hidden (but valid) .c.
    let deps = indoc! {r"
        dependencies = []
    "};
    make_project(&workspace.join("packages").join(".c"), "c", deps)?;

    uv_snapshot!(context.filters(), context.lock().current_dir(&workspace), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using Python 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 3 packages in [TIME]
    "###
    );

    let lock: SourceLock = toml::from_str(&fs_err::read_to_string(workspace.join("uv.lock"))?)?;

    assert_json_snapshot!(lock.sources(), @r###"
    {
      "a": {
        "editable": "packages/a"
      },
      "b": {
        "editable": "packages/b"
      },
      "c": {
        "editable": "packages/.c"
      }
    }
    "###);

    Ok(())
}

/// Ensure that workspace discovery accepts valid hidden directories.
#[test]
fn workspace_non_included_member() -> Result<()> {
    let context = TestContext::new("3.12");

    // Build the main workspace ...
    let workspace = context.temp_dir.child("workspace");
    workspace.child("pyproject.toml").write_str(indoc! {r#"
        [tool.uv.workspace]
        members = ["packages/*"]
    "#})?;

    // ... with a  ...
    let deps = indoc! {r#"
        dependencies = ["b"]

        [tool.uv.sources]
        b = { workspace = true }
    "#};
    make_project(&workspace.join("packages").join("a"), "a", deps)?;

    // ... and b.
    let deps = indoc! {r"
        dependencies = []
    "};
    make_project(&workspace.join("packages").join("b"), "b", deps)?;

    // ... and c, which is _not_ a member, but also isn't explicitly excluded.
    let deps = indoc! {r"
        dependencies = []
    "};
    make_project(&workspace.join("c"), "c", deps)?;

    // Locking from `c` should not include any workspace members.
    uv_snapshot!(context.filters(), context.lock().current_dir(workspace.join("c")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using Python 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 1 package in [TIME]
    "###
    );

    let lock: SourceLock = toml::from_str(&fs_err::read_to_string(
        workspace.join("c").join("uv.lock"),
    )?)?;

    assert_json_snapshot!(lock.sources(), @r###"
    {
      "c": {
        "editable": "."
      }
    }
    "###);

    Ok(())
}

/// Ensure workspace members inherit sources from the root, if not specified in the member.
///
/// In such cases, relative paths should be resolved relative to the workspace root, rather than
/// relative to the member.
#[test]
fn workspace_inherit_sources() -> Result<()> {
    let context = TestContext::new("3.12");

    // Create the workspace root.
    let workspace = context.temp_dir.child("workspace");
    workspace.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "workspace"
        version = "0.1.0"
        dependencies = []
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.workspace]
        members = ["packages/*"]
    "#})?;
    workspace.child("src/__init__.py").touch()?;

    // Create a package.
    let leaf = workspace.child("packages").child("leaf");
    leaf.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "leaf"
        version = "0.1.0"
        dependencies = ["library"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
    "#})?;
    leaf.child("src/__init__.py").touch()?;

    // Create a peripheral library.
    let library = context.temp_dir.child("library");
    library.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "library"
        version = "0.1.0"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
    "#})?;
    library.child("src/__init__.py").touch()?;

    // As-is, resolving should fail.
    uv_snapshot!(context.filters(), context.lock().arg("--offline").current_dir(&workspace), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Using Python 3.12.[X] interpreter at: [PYTHON-3.12]
      × No solution found when resolving dependencies:
      ╰─▶ Because library was not found in the cache and leaf depends on library, we can conclude that leaf's requirements are unsatisfiable.
          And because your workspace requires leaf, we can conclude that your workspace's requirements are unsatisfiable.

          hint: Packages were unavailable because the network was disabled. When the network is disabled, registry packages may only be read from the cache.
    "###
    );

    // Update the leaf to include the source.
    leaf.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "leaf"
        version = "0.1.0"
        dependencies = ["library"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sources]
        library = { path = "../../../library", editable = true }
    "#})?;
    leaf.child("src/__init__.py").touch()?;

    // Resolving should succeed.
    uv_snapshot!(context.filters(), context.lock().arg("--offline").current_dir(&workspace), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using Python 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 3 packages in [TIME]
    "###
    );

    // Revert that change.
    leaf.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "leaf"
        version = "0.1.0"
        dependencies = ["library"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
    "#})?;

    // Update the root to include the source.
    workspace.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "workspace"
        version = "0.1.0"
        dependencies = []
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sources]
        library = { path = "../library", editable = true }

        [tool.uv.workspace]
        members = ["packages/*"]
    "#})?;

    // Resolving should succeed.
    uv_snapshot!(context.filters(), context.lock().arg("--offline").current_dir(&workspace), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using Python 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 3 packages in [TIME]
    "###
    );

    let lock = fs_err::read_to_string(workspace.join("uv.lock")).unwrap();

    // The lockfile should use a path relative to the workspace root.
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
            "leaf",
            "workspace",
        ]

        [[package]]
        name = "leaf"
        version = "0.1.0"
        source = { editable = "packages/leaf" }
        dependencies = [
            { name = "library" },
        ]

        [package.metadata]
        requires-dist = [{ name = "library", editable = "../library" }]

        [[package]]
        name = "library"
        version = "0.1.0"
        source = { editable = "../library" }

        [[package]]
        name = "workspace"
        version = "0.1.0"
        source = { editable = "." }
        "###
        );
    });

    // Update the root to include the source again.
    workspace.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "workspace"
        version = "0.1.0"
        dependencies = []
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sources]
        library = { path = "../library", editable = true }

        [tool.uv.workspace]
        members = ["packages/*"]
    "#})?;

    // Update the member to include a _different_ source.
    leaf.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "leaf"
        version = "0.1.0"
        dependencies = ["library"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sources]
        application = { path = "../application", editable = true }
    "#})?;

    // Resolving should succeed; the member should still use the root's source, despite defining
    // some of its own
    uv_snapshot!(context.filters(), context.lock().arg("--offline").current_dir(&workspace), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using Python 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 3 packages in [TIME]
    "###
    );

    Ok(())
}

/// Tests error messages when a workspace member's dependencies cannot be resolved.
#[test]
#[cfg(feature = "pypi")]
fn workspace_unsatisfiable_member_dependencies() -> Result<()> {
    let context = TestContext::new("3.12");

    // Create the workspace root.
    let workspace = context.temp_dir.child("workspace");
    workspace.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "workspace"
        version = "0.1.0"
        dependencies = []
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.workspace]
        members = ["packages/*"]
    "#})?;
    workspace.child("src/__init__.py").touch()?;

    // Create a package that requires a dependency that does not exist.
    let leaf = workspace.child("packages").child("leaf");
    leaf.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "leaf"
        version = "0.1.0"
        dependencies = ["httpx>9999"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
    "#})?;
    leaf.child("src/__init__.py").touch()?;

    // Resolving should fail.
    uv_snapshot!(context.filters(), context.lock().current_dir(&workspace), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Using Python 3.12.[X] interpreter at: [PYTHON-3.12]
      × No solution found when resolving dependencies:
      ╰─▶ Because only httpx<=1.0.0b0 is available and leaf depends on httpx>9999, we can conclude that leaf's requirements are unsatisfiable.
          And because your workspace requires leaf, we can conclude that your workspace's requirements are unsatisfiable.
    "###
    );

    Ok(())
}

/// Tests error messages when a workspace member's dependencies conflict with
/// another member's.
#[test]
#[cfg(feature = "pypi")]
fn workspace_unsatisfiable_member_dependencies_conflicting() -> Result<()> {
    let context = TestContext::new("3.12");

    // Create the workspace root.
    let workspace = context.temp_dir.child("workspace");
    workspace.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "workspace"
        version = "0.1.0"
        dependencies = []
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.workspace]
        members = ["packages/*"]
    "#})?;
    workspace.child("src/__init__.py").touch()?;

    // Create two workspace members with incompatible pins
    let foo = workspace.child("packages").child("foo");
    foo.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "foo"
        version = "0.1.0"
        dependencies = ["anyio==4.1.0"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
    "#})?;
    foo.child("src/__init__.py").touch()?;
    let bar = workspace.child("packages").child("bar");
    bar.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "bar"
        version = "0.1.0"
        dependencies = ["anyio==4.2.0"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
    "#})?;
    bar.child("src/__init__.py").touch()?;

    // Resolving should fail.
    uv_snapshot!(context.filters(), context.lock().current_dir(&workspace), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Using Python 3.12.[X] interpreter at: [PYTHON-3.12]
      × No solution found when resolving dependencies:
      ╰─▶ Because bar depends on anyio==4.2.0 and foo depends on anyio==4.1.0, we can conclude that bar and foo are incompatible.
          And because your workspace requires bar and foo, we can conclude that your workspace's requirements are unsatisfiable.
    "###
    );

    Ok(())
}

/// Tests error messages when a workspace member's dependencies conflict with
/// two other member's.
#[test]
#[cfg(feature = "pypi")]
fn workspace_unsatisfiable_member_dependencies_conflicting_threeway() -> Result<()> {
    let context = TestContext::new("3.12");

    // Create the workspace root.
    let workspace = context.temp_dir.child("workspace");
    workspace.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "workspace"
        version = "0.1.0"
        dependencies = []
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.workspace]
        members = ["packages/*"]
    "#})?;
    workspace.child("src/__init__.py").touch()?;

    // Create three workspace members with incompatible pins.
    let red = workspace.child("packages").child("red");
    red.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "red"
        version = "0.1.0"
        dependencies = ["anyio==4.1.0"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
    "#})?;
    red.child("src/__init__.py").touch()?;
    let knot = workspace.child("packages").child("knot");
    knot.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "knot"
        version = "0.1.0"
        dependencies = ["anyio==4.2.0"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
    "#})?;
    knot.child("src/__init__.py").touch()?;

    // We'll raise the first conflict in the resolver, so `bird` shouldn't be
    // present in the error even though it also incompatible
    let bird = workspace.child("packages").child("bird");
    bird.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "bird"
        version = "0.1.0"
        dependencies = ["anyio==4.3.0"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
    "#})?;
    bird.child("src/__init__.py").touch()?;

    // Resolving should fail.
    uv_snapshot!(context.filters(), context.lock().current_dir(&workspace), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Using Python 3.12.[X] interpreter at: [PYTHON-3.12]
      × No solution found when resolving dependencies:
      ╰─▶ Because bird depends on anyio==4.3.0 and knot depends on anyio==4.2.0, we can conclude that bird and knot are incompatible.
          And because your workspace requires bird and knot, we can conclude that your workspace's requirements are unsatisfiable.
    "###
    );

    Ok(())
}

/// Tests error messages when a workspace member's dependencies conflict with
/// another member's optional dependencies.
#[test]
#[cfg(feature = "pypi")]
fn workspace_unsatisfiable_member_dependencies_conflicting_extra() -> Result<()> {
    let context = TestContext::new("3.12");

    // Create the workspace root.
    let workspace = context.temp_dir.child("workspace");
    workspace.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "workspace"
        version = "0.1.0"
        dependencies = []
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.workspace]
        members = ["packages/*"]
    "#})?;
    workspace.child("src/__init__.py").touch()?;

    // Create two workspace members with incompatible pins
    let foo = workspace.child("packages").child("foo");
    foo.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "foo"
        version = "0.1.0"
        dependencies = ["anyio==4.1.0"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
    "#})?;
    foo.child("src/__init__.py").touch()?;
    let bar = workspace.child("packages").child("bar");
    bar.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "bar"
        version = "0.1.0"

        [project.optional-dependencies]
        some_extra = ["anyio==4.2.0"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
    "#})?;
    bar.child("src/__init__.py").touch()?;

    // Resolving should fail.
    uv_snapshot!(context.filters(), context.lock().current_dir(&workspace), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Using Python 3.12.[X] interpreter at: [PYTHON-3.12]
      × No solution found when resolving dependencies:
      ╰─▶ Because bar[some-extra] depends on anyio==4.2.0 and foo depends on anyio==4.1.0, we can conclude that foo and bar[some-extra] are incompatible.
          And because your workspace requires bar[some-extra] and foo, we can conclude that your workspace's requirements are unsatisfiable.
    "###
    );

    Ok(())
}

/// Tests error messages when a workspace member's dependencies conflict with
/// another member's development dependencies.
#[test]
#[cfg(feature = "pypi")]
fn workspace_unsatisfiable_member_dependencies_conflicting_dev() -> Result<()> {
    let context = TestContext::new("3.12");

    // Create the workspace root.
    let workspace = context.temp_dir.child("workspace");
    workspace.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "workspace"
        version = "0.1.0"
        dependencies = []
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.workspace]
        members = ["packages/*"]
    "#})?;
    workspace.child("src/__init__.py").touch()?;

    // Create two workspace members with incompatible pins
    let foo = workspace.child("packages").child("foo");
    foo.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "foo"
        version = "0.1.0"
        dependencies = ["anyio==4.1.0"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
    "#})?;
    foo.child("src/__init__.py").touch()?;
    let bar = workspace.child("packages").child("bar");
    bar.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "bar"
        version = "0.1.0"

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv]
        dev-dependencies = ["anyio==4.2.0"]
    "#})?;
    bar.child("src/__init__.py").touch()?;

    // Resolving should fail.
    uv_snapshot!(context.filters(), context.lock().current_dir(&workspace), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Using Python 3.12.[X] interpreter at: [PYTHON-3.12]
      × No solution found when resolving dependencies:
      ╰─▶ Because bar depends on bar:dev and bar:dev depends on anyio==4.2.0, we can conclude that bar depends on anyio==4.2.0.
          And because foo depends on anyio==4.1.0, we can conclude that bar and foo are incompatible.
          And because your workspace requires bar and foo, we can conclude that your workspace's requirements are unsatisfiable.
    "###
    );

    Ok(())
}

/// Tests error messages when a workspace member's name shadows a dependency of
/// another member.
#[test]
#[cfg(feature = "pypi")]
fn workspace_member_name_shadows_dependencies() -> Result<()> {
    let context = TestContext::new("3.12");

    // Create the workspace root.
    let workspace = context.temp_dir.child("workspace");
    workspace.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "workspace"
        version = "0.1.0"
        dependencies = []
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.workspace]
        members = ["packages/*"]
    "#})?;
    workspace.child("src/__init__.py").touch()?;

    // Create a workspace member that depends on `anyio`
    let foo = workspace.child("packages").child("foo");
    foo.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "foo"
        version = "0.1.0"
        dependencies = ["anyio==4.1.0"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
    "#})?;
    foo.child("src/__init__.py").touch()?;

    // Then create an `anyio` workspace member
    let anyio = workspace.child("packages").child("anyio");
    anyio.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "anyio"
        version = "0.1.0"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
    "#})?;
    anyio.child("src/__init__.py").touch()?;

    // We should fail
    // TODO(zanieb): This error message is bad?
    uv_snapshot!(context.filters(), context.lock().current_dir(&workspace), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Using Python 3.12.[X] interpreter at: [PYTHON-3.12]
    error: Failed to build: `foo @ file://[TEMP_DIR]/workspace/packages/foo`
      Caused by: Failed to parse entry for: `anyio`
      Caused by: Package is not included as workspace package in `tool.uv.workspace`
    "###
    );

    Ok(())
}

/// Test that path dependencies with path dependencies resolve paths correctly across workspaces.
///
/// Each package is its own workspace. We put the other projects into a separate directory `libs` so
/// the paths don't line up by accident.
#[test]
fn test_path_hopping() -> Result<()> {
    let context = TestContext::new("3.12");

    // Build the main project ...
    let deps = indoc! {r#"
        dependencies = ["foo"]
        [tool.uv.sources]
        foo = { path = "../libs/foo", editable = true }
    "#};
    let main_project_dir = context.temp_dir.join("project");
    make_project(&main_project_dir, "project", deps)?;

    // ... that depends on foo ...
    let deps = indoc! {r#"
        dependencies = ["bar"]
        [tool.uv.sources]
        bar = { path = "../../libs/bar", editable = true }
    "#};
    make_project(&context.temp_dir.join("libs").join("foo"), "foo", deps)?;

    // ... that depends on bar, a stub project.
    make_project(&context.temp_dir.join("libs").join("bar"), "bar", "")?;

    uv_snapshot!(context.filters(), context.lock().arg("--preview").current_dir(&main_project_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using Python 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 3 packages in [TIME]
    "###
    );

    let lock: SourceLock =
        toml::from_str(&fs_err::read_to_string(main_project_dir.join("uv.lock"))?)?;
    assert_json_snapshot!(lock.sources(), @r###"
    {
      "bar": {
        "editable": "../libs/bar"
      },
      "foo": {
        "editable": "../libs/foo"
      },
      "project": {
        "editable": "."
      }
    }
    "###);

    Ok(())
}
