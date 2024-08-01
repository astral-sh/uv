use std::collections::BTreeMap;
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Result;
use assert_cmd::assert::OutputAssertExt;
use assert_fs::fixture::{FileWriteStr, PathChild};
use indoc::indoc;
use insta::assert_json_snapshot;
use serde::{Deserialize, Serialize};

use crate::common::{copy_dir_ignore, make_project, uv_snapshot, TestContext};

mod common;

/// `pip install --preview -e <current dir>`
fn install_workspace(context: &TestContext, current_dir: &Path) -> Command {
    let mut command = context.pip_install();
    command.arg("--preview").arg("-e").arg(current_dir);
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
        .arg("--preview")
        .arg("--package")
        .arg("bird-feeder")
        .arg("packages/bird-feeder/check_installed_bird_feeder.py")
        .current_dir(&work_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Success

    ----- stderr -----
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
        .arg("--preview")
        .arg("--package")
        .arg("albatross")
        .arg("packages/albatross/check_installed_albatross.py")
        .current_dir(&work_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Success

    ----- stderr -----
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
        .arg("--preview")
        .arg("packages/albatross/check_installed_albatross.py")
        .current_dir(&work_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Success

    ----- stderr -----
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
        .arg("--preview")
        .arg("--package")
        .arg("bird-feeder")
        .arg("packages/bird-feeder/check_installed_bird_feeder.py")
        .current_dir(&work_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Success

    ----- stderr -----
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
        .arg("--preview")
        .arg("--package")
        .arg("albatross")
        .arg("check_installed_albatross.py")
        .current_dir(&work_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Success

    ----- stderr -----
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
        .arg("--preview")
        .arg("--package")
        .arg("albatross")
        .arg("check_installed_albatross.py")
        .current_dir(&work_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Success

    ----- stderr -----
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
        .arg("--preview")
        .arg("--package")
        .arg("bird-feeder")
        .arg("check_installed_albatross.py")
        .current_dir(&work_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Success

    ----- stderr -----
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
        .arg("--preview")
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
    Prepared 3 packages in [TIME]
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
            .arg("--preview")
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
    distribution: Vec<Distribution>,
}

impl SourceLock {
    fn sources(self) -> BTreeMap<String, toml::Value> {
        self.distribution
            .into_iter()
            .map(|distribution| (distribution.name, distribution.source))
            .collect()
    }
}

#[derive(Deserialize, Serialize)]
struct Distribution {
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

    uv_snapshot!(context.filters(), context.lock().arg("--preview").current_dir(&main_workspace), @r###"
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

    uv_snapshot!(context.filters(), context.lock().arg("--preview").current_dir(&workspace), @r###"
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

    uv_snapshot!(context.filters(), context.lock().arg("--preview").current_dir(&workspace), @r###"
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

    uv_snapshot!(context.filters(), context.lock().arg("--preview").current_dir(&workspace), @r###"
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
