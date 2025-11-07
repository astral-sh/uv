use std::env;
use std::path::PathBuf;

use anyhow::Result;
use assert_cmd::assert::OutputAssertExt;
use assert_fs::fixture::PathChild;

use crate::common::{TestContext, copy_dir_ignore, uv_snapshot};

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

/// Test basic metadata output for a simple workspace with one member.
#[test]
fn workspace_metadata_simple() {
    let context = TestContext::new("3.12");

    // Initialize a workspace with one member
    context.init().arg("foo").assert().success();

    let workspace = context.temp_dir.child("foo");

    uv_snapshot!(context.filters(), context.workspace_metadata().current_dir(&workspace), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "schema": {
        "version": "preview"
      },
      "workspace_root": "[TEMP_DIR]/foo",
      "members": [
        {
          "name": "foo",
          "path": "[TEMP_DIR]/foo",
          "dependencies": []
        }
      ]
    }

    ----- stderr -----
    "#
    );
}

/// Test metadata for a root workspace (workspace with a root package).
#[test]
fn workspace_metadata_root_workspace() -> Result<()> {
    let context = TestContext::new("3.12");
    let workspace = context.temp_dir.child("workspace");

    copy_dir_ignore(
        workspaces_dir().join("albatross-root-workspace"),
        &workspace,
    )?;

    uv_snapshot!(context.filters(), context.workspace_metadata().current_dir(&workspace), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "schema": {
        "version": "preview"
      },
      "workspace_root": "[TEMP_DIR]/workspace",
      "members": [
        {
          "name": "albatross",
          "path": "[TEMP_DIR]/workspace",
          "dependencies": [
            {
              "name": "bird-feeder",
              "workspace": true
            },
            {
              "name": "iniconfig",
              "workspace": false
            }
          ]
        },
        {
          "name": "bird-feeder",
          "path": "[TEMP_DIR]/workspace/packages/bird-feeder",
          "dependencies": [
            {
              "name": "iniconfig",
              "workspace": false
            },
            {
              "name": "seeds",
              "workspace": true
            }
          ]
        },
        {
          "name": "seeds",
          "path": "[TEMP_DIR]/workspace/packages/seeds",
          "dependencies": [
            {
              "name": "idna",
              "workspace": false
            }
          ]
        }
      ]
    }

    ----- stderr -----
    "#
    );

    Ok(())
}

/// Test metadata for a virtual workspace (no root package).
#[test]
fn workspace_metadata_virtual_workspace() -> Result<()> {
    let context = TestContext::new("3.12");
    let workspace = context.temp_dir.child("workspace");

    copy_dir_ignore(
        workspaces_dir().join("albatross-virtual-workspace"),
        &workspace,
    )?;

    uv_snapshot!(context.filters(), context.workspace_metadata().current_dir(&workspace), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "schema": {
        "version": "preview"
      },
      "workspace_root": "[TEMP_DIR]/workspace",
      "members": [
        {
          "name": "albatross",
          "path": "[TEMP_DIR]/workspace/packages/albatross",
          "dependencies": [
            {
              "name": "bird-feeder",
              "workspace": true
            },
            {
              "name": "iniconfig",
              "workspace": false
            }
          ]
        },
        {
          "name": "bird-feeder",
          "path": "[TEMP_DIR]/workspace/packages/bird-feeder",
          "dependencies": [
            {
              "name": "anyio",
              "workspace": false
            },
            {
              "name": "seeds",
              "workspace": true
            }
          ]
        },
        {
          "name": "seeds",
          "path": "[TEMP_DIR]/workspace/packages/seeds",
          "dependencies": [
            {
              "name": "idna",
              "workspace": false
            }
          ]
        }
      ]
    }

    ----- stderr -----
    "#
    );

    Ok(())
}

/// Test metadata when run from a workspace member directory.
#[test]
fn workspace_metadata_from_member() -> Result<()> {
    let context = TestContext::new("3.12");
    let workspace = context.temp_dir.child("workspace");

    copy_dir_ignore(
        workspaces_dir().join("albatross-root-workspace"),
        &workspace,
    )?;

    let member_dir = workspace.join("packages").join("bird-feeder");

    uv_snapshot!(context.filters(), context.workspace_metadata().current_dir(&member_dir), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "schema": {
        "version": "preview"
      },
      "workspace_root": "[TEMP_DIR]/workspace",
      "members": [
        {
          "name": "albatross",
          "path": "[TEMP_DIR]/workspace",
          "dependencies": [
            {
              "name": "bird-feeder",
              "workspace": true
            },
            {
              "name": "iniconfig",
              "workspace": false
            }
          ]
        },
        {
          "name": "bird-feeder",
          "path": "[TEMP_DIR]/workspace/packages/bird-feeder",
          "dependencies": [
            {
              "name": "iniconfig",
              "workspace": false
            },
            {
              "name": "seeds",
              "workspace": true
            }
          ]
        },
        {
          "name": "seeds",
          "path": "[TEMP_DIR]/workspace/packages/seeds",
          "dependencies": [
            {
              "name": "idna",
              "workspace": false
            }
          ]
        }
      ]
    }

    ----- stderr -----
    "#
    );

    Ok(())
}

/// Test metadata for a workspace with multiple packages.
#[test]
fn workspace_metadata_multiple_members() {
    let context = TestContext::new("3.12");

    // Initialize workspace root
    context.init().arg("pkg-a").assert().success();

    let workspace_root = context.temp_dir.child("pkg-a");

    // Add more members
    context
        .init()
        .arg("pkg-b")
        .current_dir(&workspace_root)
        .assert()
        .success();

    context
        .init()
        .arg("pkg-c")
        .current_dir(&workspace_root)
        .assert()
        .success();

    uv_snapshot!(context.filters(), context.workspace_metadata().current_dir(&workspace_root), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "schema": {
        "version": "preview"
      },
      "workspace_root": "[TEMP_DIR]/pkg-a",
      "members": [
        {
          "name": "pkg-a",
          "path": "[TEMP_DIR]/pkg-a",
          "dependencies": []
        },
        {
          "name": "pkg-b",
          "path": "[TEMP_DIR]/pkg-a/pkg-b",
          "dependencies": []
        },
        {
          "name": "pkg-c",
          "path": "[TEMP_DIR]/pkg-a/pkg-c",
          "dependencies": []
        }
      ]
    }

    ----- stderr -----
    "#
    );
}

/// Test metadata for a single project (not a workspace).
#[test]
fn workspace_metadata_single_project() {
    let context = TestContext::new("3.12");

    context.init().arg("my-project").assert().success();

    let project = context.temp_dir.child("my-project");

    uv_snapshot!(context.filters(), context.workspace_metadata().current_dir(&project), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "schema": {
        "version": "preview"
      },
      "workspace_root": "[TEMP_DIR]/my-project",
      "members": [
        {
          "name": "my-project",
          "path": "[TEMP_DIR]/my-project",
          "dependencies": []
        }
      ]
    }

    ----- stderr -----
    "#
    );
}

/// Test metadata with excluded packages.
#[test]
fn workspace_metadata_with_excluded() -> Result<()> {
    let context = TestContext::new("3.12");
    let workspace = context.temp_dir.child("workspace");

    copy_dir_ignore(
        workspaces_dir().join("albatross-project-in-excluded"),
        &workspace,
    )?;

    uv_snapshot!(context.filters(), context.workspace_metadata().current_dir(&workspace), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "schema": {
        "version": "preview"
      },
      "workspace_root": "[TEMP_DIR]/workspace",
      "members": [
        {
          "name": "albatross",
          "path": "[TEMP_DIR]/workspace",
          "dependencies": [
            {
              "name": "iniconfig",
              "workspace": false
            }
          ]
        }
      ]
    }

    ----- stderr -----
    "#
    );

    Ok(())
}

/// Test metadata error when not in a project.
#[test]
fn workspace_metadata_no_project() {
    let context = TestContext::new("3.12");

    uv_snapshot!(context.filters(), context.workspace_metadata(), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No `pyproject.toml` found in current directory or any parent directory
    "###
    );
}

/// Test metadata with regular workspace dependencies.
#[test]
fn workspace_metadata_with_regular_dependencies() {
    let context = TestContext::new("3.12");

    // Create workspace with multiple members
    context.init().arg("workspace-root").assert().success();

    let workspace_root = context.temp_dir.child("workspace-root");

    // Create a library package
    context
        .init()
        .arg("lib-a")
        .current_dir(&workspace_root)
        .assert()
        .success();

    // Create another package that depends on lib-a
    context
        .init()
        .arg("app-b")
        .current_dir(&workspace_root)
        .assert()
        .success();

    // Add lib-a as a dependency to app-b
    context
        .add()
        .arg("lib-a")
        .current_dir(workspace_root.child("app-b"))
        .assert()
        .success();

    uv_snapshot!(context.filters(), context.workspace_metadata().current_dir(&workspace_root), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "schema": {
        "version": "preview"
      },
      "workspace_root": "[TEMP_DIR]/workspace-root",
      "members": [
        {
          "name": "app-b",
          "path": "[TEMP_DIR]/workspace-root/app-b",
          "dependencies": [
            {
              "name": "lib-a",
              "workspace": true
            }
          ]
        },
        {
          "name": "lib-a",
          "path": "[TEMP_DIR]/workspace-root/lib-a",
          "dependencies": []
        },
        {
          "name": "workspace-root",
          "path": "[TEMP_DIR]/workspace-root",
          "dependencies": []
        }
      ]
    }

    ----- stderr -----
    "#
    );
}

/// Test metadata with optional dependencies (extras) on workspace members.
#[test]
fn workspace_metadata_with_extras() {
    let context = TestContext::new("3.12");

    // Create workspace with multiple members
    context.init().arg("workspace-root").assert().success();

    let workspace_root = context.temp_dir.child("workspace-root");

    // Create library packages
    context
        .init()
        .arg("lib-a")
        .current_dir(&workspace_root)
        .assert()
        .success();

    context
        .init()
        .arg("lib-b")
        .current_dir(&workspace_root)
        .assert()
        .success();

    // Create app with optional dependencies on lib-a and lib-b
    context
        .init()
        .arg("app")
        .current_dir(&workspace_root)
        .assert()
        .success();

    // Add optional dependencies
    let app_dir = workspace_root.child("app");
    context
        .add()
        .arg("lib-a")
        .arg("--optional")
        .arg("extra-a")
        .current_dir(&app_dir)
        .assert()
        .success();

    context
        .add()
        .arg("lib-b")
        .arg("--optional")
        .arg("extra-b")
        .current_dir(&app_dir)
        .assert()
        .success();

    uv_snapshot!(context.filters(), context.workspace_metadata().current_dir(&workspace_root), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "schema": {
        "version": "preview"
      },
      "workspace_root": "[TEMP_DIR]/workspace-root",
      "members": [
        {
          "name": "app",
          "path": "[TEMP_DIR]/workspace-root/app",
          "dependencies": [
            {
              "name": "lib-a",
              "workspace": true,
              "extra": "extra-a"
            },
            {
              "name": "lib-b",
              "workspace": true,
              "extra": "extra-b"
            }
          ]
        },
        {
          "name": "lib-a",
          "path": "[TEMP_DIR]/workspace-root/lib-a",
          "dependencies": []
        },
        {
          "name": "lib-b",
          "path": "[TEMP_DIR]/workspace-root/lib-b",
          "dependencies": []
        },
        {
          "name": "workspace-root",
          "path": "[TEMP_DIR]/workspace-root",
          "dependencies": []
        }
      ]
    }

    ----- stderr -----
    "#
    );
}

/// Test metadata with dependency groups containing workspace members.
#[test]
fn workspace_metadata_with_dependency_groups() {
    let context = TestContext::new("3.12");

    // Create workspace with multiple members
    context.init().arg("workspace-root").assert().success();

    let workspace_root = context.temp_dir.child("workspace-root");

    // Create library packages
    context
        .init()
        .arg("test-utils")
        .current_dir(&workspace_root)
        .assert()
        .success();

    context
        .init()
        .arg("dev-tools")
        .current_dir(&workspace_root)
        .assert()
        .success();

    // Create app with dependency groups
    context
        .init()
        .arg("app")
        .current_dir(&workspace_root)
        .assert()
        .success();

    // Add dependency groups
    let app_dir = workspace_root.child("app");
    context
        .add()
        .arg("test-utils")
        .arg("--group")
        .arg("test")
        .current_dir(&app_dir)
        .assert()
        .success();

    context
        .add()
        .arg("dev-tools")
        .arg("--group")
        .arg("dev")
        .current_dir(&app_dir)
        .assert()
        .success();

    uv_snapshot!(context.filters(), context.workspace_metadata().current_dir(&workspace_root), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "schema": {
        "version": "preview"
      },
      "workspace_root": "[TEMP_DIR]/workspace-root",
      "members": [
        {
          "name": "app",
          "path": "[TEMP_DIR]/workspace-root/app",
          "dependencies": [
            {
              "name": "dev-tools",
              "workspace": true,
              "group": "dev"
            },
            {
              "name": "test-utils",
              "workspace": true,
              "group": "test"
            }
          ]
        },
        {
          "name": "dev-tools",
          "path": "[TEMP_DIR]/workspace-root/dev-tools",
          "dependencies": []
        },
        {
          "name": "test-utils",
          "path": "[TEMP_DIR]/workspace-root/test-utils",
          "dependencies": []
        },
        {
          "name": "workspace-root",
          "path": "[TEMP_DIR]/workspace-root",
          "dependencies": []
        }
      ]
    }

    ----- stderr -----
    "#
    );
}

/// Test metadata with workspace members that have no dependencies on each other.
#[test]
fn workspace_metadata_no_workspace_dependencies() {
    let context = TestContext::new("3.12");

    // Create workspace with multiple members that don't depend on each other
    context.init().arg("workspace-root").assert().success();

    let workspace_root = context.temp_dir.child("workspace-root");

    // Create independent packages
    context
        .init()
        .arg("package-a")
        .current_dir(&workspace_root)
        .assert()
        .success();

    context
        .init()
        .arg("package-b")
        .current_dir(&workspace_root)
        .assert()
        .success();

    context
        .init()
        .arg("package-c")
        .current_dir(&workspace_root)
        .assert()
        .success();

    uv_snapshot!(context.filters(), context.workspace_metadata().current_dir(&workspace_root), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "schema": {
        "version": "preview"
      },
      "workspace_root": "[TEMP_DIR]/workspace-root",
      "members": [
        {
          "name": "package-a",
          "path": "[TEMP_DIR]/workspace-root/package-a",
          "dependencies": []
        },
        {
          "name": "package-b",
          "path": "[TEMP_DIR]/workspace-root/package-b",
          "dependencies": []
        },
        {
          "name": "package-c",
          "path": "[TEMP_DIR]/workspace-root/package-c",
          "dependencies": []
        },
        {
          "name": "workspace-root",
          "path": "[TEMP_DIR]/workspace-root",
          "dependencies": []
        }
      ]
    }

    ----- stderr -----
    "#
    );
}

/// Test metadata with mixed workspace dependencies (regular, extras, and groups).
#[test]
fn workspace_metadata_mixed_dependencies() {
    let context = TestContext::new("3.12");

    // Create workspace with multiple members
    context.init().arg("workspace-root").assert().success();

    let workspace_root = context.temp_dir.child("workspace-root");

    // Create library packages
    context
        .init()
        .arg("core")
        .current_dir(&workspace_root)
        .assert()
        .success();

    context
        .init()
        .arg("utils")
        .current_dir(&workspace_root)
        .assert()
        .success();

    context
        .init()
        .arg("testing")
        .current_dir(&workspace_root)
        .assert()
        .success();

    // Create app with all types of dependencies
    context
        .init()
        .arg("app")
        .current_dir(&workspace_root)
        .assert()
        .success();

    // Add regular dependency, optional dependency, and dependency group
    let app_dir = workspace_root.child("app");

    // Add regular dependency
    context
        .add()
        .arg("core")
        .current_dir(&app_dir)
        .assert()
        .success();

    // Add optional dependency
    context
        .add()
        .arg("utils")
        .arg("--optional")
        .arg("utils")
        .current_dir(&app_dir)
        .assert()
        .success();

    // Add dependency group
    context
        .add()
        .arg("testing")
        .arg("--group")
        .arg("test")
        .current_dir(&app_dir)
        .assert()
        .success();

    uv_snapshot!(context.filters(), context.workspace_metadata().current_dir(&workspace_root), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "schema": {
        "version": "preview"
      },
      "workspace_root": "[TEMP_DIR]/workspace-root",
      "members": [
        {
          "name": "app",
          "path": "[TEMP_DIR]/workspace-root/app",
          "dependencies": [
            {
              "name": "core",
              "workspace": true
            },
            {
              "name": "utils",
              "workspace": true,
              "extra": "utils"
            },
            {
              "name": "testing",
              "workspace": true,
              "group": "test"
            }
          ]
        },
        {
          "name": "core",
          "path": "[TEMP_DIR]/workspace-root/core",
          "dependencies": []
        },
        {
          "name": "testing",
          "path": "[TEMP_DIR]/workspace-root/testing",
          "dependencies": []
        },
        {
          "name": "utils",
          "path": "[TEMP_DIR]/workspace-root/utils",
          "dependencies": []
        },
        {
          "name": "workspace-root",
          "path": "[TEMP_DIR]/workspace-root",
          "dependencies": []
        }
      ]
    }

    ----- stderr -----
    "#
    );
}
