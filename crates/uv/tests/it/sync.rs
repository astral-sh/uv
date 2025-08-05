use anyhow::Result;
use assert_cmd::prelude::*;
use assert_fs::{fixture::ChildPath, prelude::*};
use indoc::{formatdoc, indoc};
use insta::assert_snapshot;
use predicates::prelude::predicate;
use tempfile::tempdir_in;

use uv_fs::Simplified;
use uv_static::EnvVars;

use crate::common::{TestContext, download_to_disk, packse_index_url, uv_snapshot, venv_bin_path};

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
    uv_snapshot!(context.filters(), context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

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
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    The lockfile at `uv.lock` needs to be updated, but `--locked` was provided. To update the lockfile, run `uv lock`.
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
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==3.7.0
     + idna==3.6
     + sniffio==1.3.1
    ");

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
    uv_snapshot!(context.filters(), context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: No `requires-python` value found in the workspace. Defaulting to `>=3.12`.
    Resolved in [TIME]
    Audited in [TIME]
    ");

    assert!(context.temp_dir.child("uv.lock").exists());

    // Running `uv sync` again should succeed.
    uv_snapshot!(context.filters(), context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: No `requires-python` value found in the workspace. Defaulting to `>=3.12`.
    Resolved in [TIME]
    Audited in [TIME]
    ");

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

    uv_snapshot!(context.filters(), context.sync().arg("--package").arg("child"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + child==0.1.0 (from file://[TEMP_DIR]/child)
     + iniconfig==2.0.0
    ");

    Ok(())
}

/// Test json output
#[test]
fn sync_json() -> Result<()> {
    let context = TestContext::new("3.12")
        .with_filtered_python_names()
        .with_filtered_virtualenv_bin();

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

    uv_snapshot!(context.filters(), context.sync()
        .arg("--output-format").arg("json"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "schema": {
        "version": "preview"
      },
      "target": "project",
      "project": {
        "path": "[TEMP_DIR]/",
        "workspace": {
          "path": "[TEMP_DIR]/"
        }
      },
      "sync": {
        "environment": {
          "path": "[VENV]/",
          "python": {
            "path": "[VENV]/[BIN]/[PYTHON]",
            "version": "3.12.[X]",
            "implementation": "cpython"
          }
        },
        "action": "check"
      },
      "lock": {
        "path": "[TEMP_DIR]/uv.lock",
        "action": "create"
      },
      "dry_run": false
    }

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    "#);

    assert!(context.temp_dir.child("uv.lock").exists());

    uv_snapshot!(context.filters(), context.sync()
        .arg("--frozen")
        .arg("--output-format").arg("json"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "schema": {
        "version": "preview"
      },
      "target": "project",
      "project": {
        "path": "[TEMP_DIR]/",
        "workspace": {
          "path": "[TEMP_DIR]/"
        }
      },
      "sync": {
        "environment": {
          "path": "[VENV]/",
          "python": {
            "path": "[VENV]/[BIN]/[PYTHON]",
            "version": "3.12.[X]",
            "implementation": "cpython"
          }
        },
        "action": "check"
      },
      "lock": {
        "path": "[TEMP_DIR]/uv.lock",
        "action": "use"
      },
      "dry_run": false
    }

    ----- stderr -----
    Audited 1 package in [TIME]
    "#);

    uv_snapshot!(context.filters(), context.sync()
        .arg("--locked")
        .arg("--output-format").arg("json"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "schema": {
        "version": "preview"
      },
      "target": "project",
      "project": {
        "path": "[TEMP_DIR]/",
        "workspace": {
          "path": "[TEMP_DIR]/"
        }
      },
      "sync": {
        "environment": {
          "path": "[VENV]/",
          "python": {
            "path": "[VENV]/[BIN]/[PYTHON]",
            "version": "3.12.[X]",
            "implementation": "cpython"
          }
        },
        "action": "check"
      },
      "lock": {
        "path": "[TEMP_DIR]/uv.lock",
        "action": "check"
      },
      "dry_run": false
    }

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Audited 1 package in [TIME]
    "#);

    // Invalidate the lockfile by changing the requirements.
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig<2"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.sync()
        .arg("--locked")
        .arg("--output-format").arg("json"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    The lockfile at `uv.lock` needs to be updated, but `--locked` was provided. To update the lockfile, run `uv lock`.
    "###);

    // Test that JSON output is shown even with --quiet flag
    uv_snapshot!(context.filters(), context.sync()
        .arg("--quiet")
        .arg("--frozen")
        .arg("--output-format").arg("json"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "schema": {
        "version": "preview"
      },
      "target": "project",
      "project": {
        "path": "[TEMP_DIR]/",
        "workspace": {
          "path": "[TEMP_DIR]/"
        }
      },
      "sync": {
        "environment": {
          "path": "[VENV]/",
          "python": {
            "path": "[VENV]/[BIN]/[PYTHON]",
            "version": "3.12.[X]",
            "implementation": "cpython"
          }
        },
        "action": "check"
      },
      "lock": {
        "path": "[TEMP_DIR]/uv.lock",
        "action": "use"
      },
      "dry_run": false
    }

    ----- stderr -----
    "#);

    Ok(())
}

/// Test --dry json output
#[test]
fn sync_dry_json() -> Result<()> {
    let context = TestContext::new_with_versions(&["3.12"])
        .with_filtered_python_names()
        .with_filtered_virtualenv_bin();

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

    // Running `uv sync` should report intent to create the environment and lockfile
    uv_snapshot!(context.filters(), context.sync()
        .arg("--output-format").arg("json")
        .arg("--dry-run"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "schema": {
        "version": "preview"
      },
      "target": "project",
      "project": {
        "path": "[TEMP_DIR]/",
        "workspace": {
          "path": "[TEMP_DIR]/"
        }
      },
      "sync": {
        "environment": {
          "path": "[VENV]/",
          "python": {
            "path": "[VENV]/[BIN]/[PYTHON]",
            "version": "3.12.[X]",
            "implementation": "cpython"
          }
        },
        "action": "create"
      },
      "lock": {
        "path": "[TEMP_DIR]/uv.lock",
        "action": "create"
      },
      "dry_run": true
    }

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 2 packages in [TIME]
    Would download 1 package
    Would install 1 package
     + iniconfig==2.0.0
    "#);

    Ok(())
}

/// Ensure that we use the maximum Python version when a workspace contains mixed requirements.
#[test]
fn mixed_requires_python() -> Result<()> {
    let context = TestContext::new_with_versions(&["3.9", "3.12"]);

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

    // Create a child with a minimum Python requirement of Python 3.9.
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
        requires-python = ">=3.9"

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    // Running `uv sync` should succeed, locking for Python 3.12.
    uv_snapshot!(context.filters(), context.sync().arg("-p").arg("3.12"), @r"
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
    ");

    // Running `uv sync` again should fail.
    uv_snapshot!(context.filters(), context.sync().arg("-p").arg("3.9"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.9.[X] interpreter at: [PYTHON-3.9]
    error: The requested interpreter resolved to Python 3.9.[X], which is incompatible with the project's Python requirement: `>=3.12` (from workspace member `albatross`'s `project.requires-python`).
    ");

    Ok(())
}

/// Ensure that group requires-python solves an actual problem
#[test]
#[cfg(not(windows))]
#[cfg(feature = "python-eol")]
fn group_requires_python_useful_defaults() -> Result<()> {
    let context = TestContext::new_with_versions(&["3.8", "3.9"]);

    // Require 3.8 for our project, but have a dev-dependency on a version of sphinx that needs 3.9
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "pharaohs-tomp"
        version = "0.1.0"
        requires-python = ">=3.8"
        dependencies = ["anyio"]

        [dependency-groups]
        dev = ["sphinx>=7.2.6"]
        "#,
    )?;

    let src = context.temp_dir.child("src").child("albatross");
    src.create_dir_all()?;

    let init = src.child("__init__.py");
    init.touch()?;

    // Running `uv sync --no-dev` should ideally succeed, locking for Python 3.8.
    // ...but once we pick the 3.8 interpreter the lock freaks out because it sees
    // that the dependency-group containing sphinx will never successfully install,
    // even though it's not enabled!
    uv_snapshot!(context.filters(), context.sync()
        .arg("--no-dev"), @r"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.8.[X] interpreter at: [PYTHON-3.8]
    Creating virtual environment at: .venv
      × No solution found when resolving dependencies for split (python_full_version == '3.8.*'):
      ╰─▶ Because the requested Python version (>=3.8) does not satisfy Python>=3.9 and sphinx==7.2.6 depends on Python>=3.9, we can conclude that sphinx==7.2.6 cannot be used.
          And because only sphinx<=7.2.6 is available, we can conclude that sphinx>=7.2.6 cannot be used.
          And because pharaohs-tomp:dev depends on sphinx>=7.2.6 and your project requires pharaohs-tomp:dev, we can conclude that your project's requirements are unsatisfiable.

          hint: The `requires-python` value (>=3.8) includes Python versions that are not supported by your dependencies (e.g., sphinx==7.2.6 only supports >=3.9). Consider using a more restrictive `requires-python` value (like >=3.9).
    ");

    // Running `uv sync` should always fail, as now sphinx is involved
    uv_snapshot!(context.filters(), context.sync(), @r"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies for split (python_full_version == '3.8.*'):
      ╰─▶ Because the requested Python version (>=3.8) does not satisfy Python>=3.9 and sphinx==7.2.6 depends on Python>=3.9, we can conclude that sphinx==7.2.6 cannot be used.
          And because only sphinx<=7.2.6 is available, we can conclude that sphinx>=7.2.6 cannot be used.
          And because pharaohs-tomp:dev depends on sphinx>=7.2.6 and your project requires pharaohs-tomp:dev, we can conclude that your project's requirements are unsatisfiable.

          hint: The `requires-python` value (>=3.8) includes Python versions that are not supported by your dependencies (e.g., sphinx==7.2.6 only supports >=3.9). Consider using a more restrictive `requires-python` value (like >=3.9).
    ");

    // Adding group requires python should fix it
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "pharaohs-tomp"
        version = "0.1.0"
        requires-python = ">=3.8"
        dependencies = ["anyio"]

        [dependency-groups]
        dev = ["sphinx>=7.2.6"]

        [tool.uv.dependency-groups]
        dev = {requires-python = ">=3.9"}
        "#,
    )?;

    // Running `uv sync --no-dev` should succeed, still using the Python 3.8.
    uv_snapshot!(context.filters(), context.sync()
        .arg("--no-dev"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 29 packages in [TIME]
    Prepared 5 packages in [TIME]
    Installed 5 packages in [TIME]
     + anyio==4.3.0
     + exceptiongroup==1.2.0
     + idna==3.6
     + sniffio==1.3.1
     + typing-extensions==4.10.0
    ");

    // Running `uv sync` should succeed, bumping to Python 3.9 as sphinx is now involved.
    uv_snapshot!(context.filters(), context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.9.[X] interpreter at: [PYTHON-3.9]
    Removed virtual environment at: .venv
    Creating virtual environment at: .venv
    Resolved 29 packages in [TIME]
    Prepared 22 packages in [TIME]
    Installed 27 packages in [TIME]
     + alabaster==0.7.16
     + anyio==4.3.0
     + babel==2.14.0
     + certifi==2024.2.2
     + charset-normalizer==3.3.2
     + docutils==0.20.1
     + exceptiongroup==1.2.0
     + idna==3.6
     + imagesize==1.4.1
     + importlib-metadata==7.1.0
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + packaging==24.0
     + pygments==2.17.2
     + requests==2.31.0
     + sniffio==1.3.1
     + snowballstemmer==2.2.0
     + sphinx==7.2.6
     + sphinxcontrib-applehelp==1.0.8
     + sphinxcontrib-devhelp==1.0.6
     + sphinxcontrib-htmlhelp==2.0.5
     + sphinxcontrib-jsmath==1.0.1
     + sphinxcontrib-qthelp==1.0.7
     + sphinxcontrib-serializinghtml==1.1.10
     + typing-extensions==4.10.0
     + urllib3==2.2.1
     + zipp==3.18.1
    ");

    Ok(())
}

/// Ensure that group requires-python solves an actual problem
#[test]
#[cfg(not(windows))]
#[cfg(feature = "python-eol")]
fn group_requires_python_useful_non_defaults() -> Result<()> {
    let context = TestContext::new_with_versions(&["3.8", "3.9"]);

    // Require 3.8 for our project, but have a dev-dependency on a version of sphinx that needs 3.9
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "pharaohs-tomp"
        version = "0.1.0"
        requires-python = ">=3.8"
        dependencies = ["anyio"]

        [dependency-groups]
        mygroup = ["sphinx>=7.2.6"]
        "#,
    )?;

    let src = context.temp_dir.child("src").child("albatross");
    src.create_dir_all()?;

    let init = src.child("__init__.py");
    init.touch()?;

    // Running `uv sync` should ideally succeed, locking for Python 3.8.
    // ...but once we pick the 3.8 interpreter the lock freaks out because it sees
    // that the dependency-group containing sphinx will never successfully install,
    // even though it's not enabled, or even a default!
    uv_snapshot!(context.filters(), context.sync(), @r"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.8.[X] interpreter at: [PYTHON-3.8]
    Creating virtual environment at: .venv
      × No solution found when resolving dependencies for split (python_full_version == '3.8.*'):
      ╰─▶ Because the requested Python version (>=3.8) does not satisfy Python>=3.9 and sphinx==7.2.6 depends on Python>=3.9, we can conclude that sphinx==7.2.6 cannot be used.
          And because only sphinx<=7.2.6 is available, we can conclude that sphinx>=7.2.6 cannot be used.
          And because pharaohs-tomp:mygroup depends on sphinx>=7.2.6 and your project requires pharaohs-tomp:mygroup, we can conclude that your project's requirements are unsatisfiable.

          hint: The `requires-python` value (>=3.8) includes Python versions that are not supported by your dependencies (e.g., sphinx==7.2.6 only supports >=3.9). Consider using a more restrictive `requires-python` value (like >=3.9).
    ");

    // Running `uv sync --group mygroup` should definitely fail, as now sphinx is involved
    uv_snapshot!(context.filters(), context.sync()
        .arg("--group").arg("mygroup"), @r"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies for split (python_full_version == '3.8.*'):
      ╰─▶ Because the requested Python version (>=3.8) does not satisfy Python>=3.9 and sphinx==7.2.6 depends on Python>=3.9, we can conclude that sphinx==7.2.6 cannot be used.
          And because only sphinx<=7.2.6 is available, we can conclude that sphinx>=7.2.6 cannot be used.
          And because pharaohs-tomp:mygroup depends on sphinx>=7.2.6 and your project requires pharaohs-tomp:mygroup, we can conclude that your project's requirements are unsatisfiable.

          hint: The `requires-python` value (>=3.8) includes Python versions that are not supported by your dependencies (e.g., sphinx==7.2.6 only supports >=3.9). Consider using a more restrictive `requires-python` value (like >=3.9).
    ");

    // Adding group requires python should fix it
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "pharaohs-tomp"
        version = "0.1.0"
        requires-python = ">=3.8"
        dependencies = ["anyio"]

        [dependency-groups]
        mygroup = ["sphinx>=7.2.6"]

        [tool.uv.dependency-groups]
        mygroup = {requires-python = ">=3.9"}
        "#,
    )?;

    // Running `uv sync` should succeed, locking for the previous picked Python 3.8.
    uv_snapshot!(context.filters(), context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 29 packages in [TIME]
    Prepared 5 packages in [TIME]
    Installed 5 packages in [TIME]
     + anyio==4.3.0
     + exceptiongroup==1.2.0
     + idna==3.6
     + sniffio==1.3.1
     + typing-extensions==4.10.0
    ");

    // Running `uv sync --group mygroup` should pass, bumping the interpreter to 3.9,
    // as the group requires-python saves us
    uv_snapshot!(context.filters(), context.sync()
        .arg("--group").arg("mygroup"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.9.[X] interpreter at: [PYTHON-3.9]
    Removed virtual environment at: .venv
    Creating virtual environment at: .venv
    Resolved 29 packages in [TIME]
    Prepared 22 packages in [TIME]
    Installed 27 packages in [TIME]
     + alabaster==0.7.16
     + anyio==4.3.0
     + babel==2.14.0
     + certifi==2024.2.2
     + charset-normalizer==3.3.2
     + docutils==0.20.1
     + exceptiongroup==1.2.0
     + idna==3.6
     + imagesize==1.4.1
     + importlib-metadata==7.1.0
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + packaging==24.0
     + pygments==2.17.2
     + requests==2.31.0
     + sniffio==1.3.1
     + snowballstemmer==2.2.0
     + sphinx==7.2.6
     + sphinxcontrib-applehelp==1.0.8
     + sphinxcontrib-devhelp==1.0.6
     + sphinxcontrib-htmlhelp==2.0.5
     + sphinxcontrib-jsmath==1.0.1
     + sphinxcontrib-qthelp==1.0.7
     + sphinxcontrib-serializinghtml==1.1.10
     + typing-extensions==4.10.0
     + urllib3==2.2.1
     + zipp==3.18.1
    ");

    Ok(())
}

#[test]
fn check() -> Result<()> {
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

    // Running `uv sync --check` should fail.
    uv_snapshot!(context.filters(), context.sync().arg("--check"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Would use project environment at: .venv
    Resolved 2 packages in [TIME]
    Would create lockfile at: uv.lock
    Would download 1 package
    Would install 1 package
     + iniconfig==2.0.0
    The environment is outdated; run `uv sync` to update the environment
    "###);

    // Sync the environment.
    uv_snapshot!(context.filters(), context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    assert!(context.temp_dir.child("uv.lock").exists());

    // Running `uv sync --check` should pass now that the environment is up to date.
    uv_snapshot!(context.filters(), context.sync().arg("--check"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Would use project environment at: .venv
    Resolved 2 packages in [TIME]
    Found up-to-date lockfile at: uv.lock
    Audited 1 package in [TIME]
    Would make no changes
    ");
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
    context
        .temp_dir
        .child("src")
        .child("albatross")
        .child("__init__.py")
        .touch()?;

    let child = context.temp_dir.child("child");
    let pyproject_toml = child.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "child"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig>=1"]

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

    // Syncing with `--no-dev` should omit all dependencies except `iniconfig`.
    uv_snapshot!(context.filters(), context.sync().arg("--no-dev"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 11 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + child==0.1.0 (from file://[TEMP_DIR]/child)
     + iniconfig==2.0.0
    ");

    // Syncing without `--no-dev` should include `anyio`, `requests`, `pysocks`, and their
    // dependencies, but not `typing-extensions`.
    uv_snapshot!(context.filters(), context.sync(), @r"
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
    ");

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

    uv_snapshot!(context.filters(), context.sync().arg("--frozen").arg("--package").arg("foo"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + typing-extensions==4.10.0
    ");

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

    context
        .temp_dir
        .child("src")
        .child("albatross")
        .child("__init__.py")
        .touch()?;

    let child = context.temp_dir.child("child");
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
        requires = ["hatchling"]
        build-backend = "hatchling.build"
        "#,
    )?;
    child
        .child("src")
        .child("child")
        .child("__init__.py")
        .touch()?;

    uv_snapshot!(context.filters(), context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + child==0.1.0 (from file://[TEMP_DIR]/child)
     + iniconfig==2.0.0
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--group").arg("foo"), @r"
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
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--only-group").arg("bar"), @r"
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
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--group").arg("baz"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Installed 2 packages in [TIME]
     + child==0.1.0 (from file://[TEMP_DIR]/child)
     + iniconfig==2.0.0
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--group").arg("bop"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    error: Group `bop` is not defined in any project's `dependency-groups` table
    ");

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

    uv_snapshot!(context.filters(), context.sync().arg("--frozen").arg("--group").arg("async"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + sniffio==1.3.1
    ");

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
    uv_snapshot!(context.filters(), context.sync().arg("--frozen").arg("--group").arg("async"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 3 packages in [TIME]
    ");

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
    uv_snapshot!(context.filters(), context.sync().arg("--no-build-isolation"), @r"
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
    ");

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
    uv_snapshot!(context.filters(), context.sync().arg("--no-build-isolation-package").arg("source-distribution"), @r#"
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
    "#);

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
    uv_snapshot!(context.filters(), context.sync().arg("--no-build-isolation-package").arg("source-distribution"), @r"
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
    ");

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
    uv_snapshot!(context.filters(), context.sync().arg("--extra").arg("compile"), @r#"
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
    "#);

    // Running `uv sync` with `--all-extras` should also fail.
    uv_snapshot!(context.filters(), context.sync().arg("--all-extras"), @r#"
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
    "#);

    // Install the build dependencies.
    uv_snapshot!(context.filters(), context.sync().arg("--extra").arg("build"), @r"
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
    ");

    // Running `uv sync` for the `compile` extra should succeed, and remove the build dependencies.
    uv_snapshot!(context.filters(), context.sync().arg("--extra").arg("compile"), @r"
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
    ");

    assert!(context.temp_dir.child("uv.lock").exists());

    Ok(())
}

#[test]
fn sync_extra_build_dependencies() -> Result<()> {
    let context = TestContext::new("3.12").with_filtered_counts();

    // Write a test package that arbitrarily requires `anyio` at build time
    let child = context.temp_dir.child("child");
    child.create_dir_all()?;
    let child_pyproject_toml = child.child("pyproject.toml");
    child_pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "child"
        version = "0.1.0"
        requires-python = ">=3.9"

        [build-system]
        requires = ["hatchling"]
        backend-path = ["."]
        build-backend = "build_backend"
    "#})?;
    let build_backend = child.child("build_backend.py");
    build_backend.write_str(indoc! {r#"
        import sys

        from hatchling.build import *

        try:
            import anyio
        except ModuleNotFoundError:
            print("Missing `anyio` module", file=sys.stderr)
            sys.exit(1)
    "#})?;
    child.child("src/child/__init__.py").touch()?;

    let parent = &context.temp_dir;
    let pyproject_toml = parent.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "parent"
        version = "0.1.0"
        requires-python = ">=3.9"
        dependencies = ["child"]

        [tool.uv.sources]
        child = { path = "child" }
    "#})?;

    context.venv().arg("--clear").assert().success();
    // Running `uv sync` should fail due to missing build-dependencies
    uv_snapshot!(context.filters(), context.sync(), @r"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
      × Failed to build `child @ file://[TEMP_DIR]/child`
      ├─▶ The build backend returned an error
      ╰─▶ Call to `build_backend.build_wheel` failed (exit status: 1)

          [stderr]
          Missing `anyio` module

          hint: This usually indicates a problem with the package or the build environment.
      help: `child` was included because `parent` (v0.1.0) depends on `child`
    ");

    // Adding `extra-build-dependencies` should solve the issue
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "parent"
        version = "0.1.0"
        requires-python = ">=3.9"
        dependencies = ["child"]

        [tool.uv.sources]
        child = { path = "child" }

        [tool.uv.extra-build-dependencies]
        child = ["anyio"]
    "#})?;

    context.venv().arg("--clear").assert().success();
    uv_snapshot!(context.filters(), context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: The `extra-build-dependencies` option is experimental and may change without warning. Pass `--preview-features extra-build-dependencies` to disable this warning.
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + child==0.1.0 (from file://[TEMP_DIR]/child)
    ");

    context.venv().arg("--clear").assert().success();
    uv_snapshot!(context.filters(), context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: The `extra-build-dependencies` option is experimental and may change without warning. Pass `--preview-features extra-build-dependencies` to disable this warning.
    Resolved [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + child==0.1.0 (from file://[TEMP_DIR]/child)
    ");

    // Adding `extra-build-dependencies` with the wrong name should fail the build
    // (the cache is invalidated when extra build dependencies change)
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "parent"
        version = "0.1.0"
        requires-python = ">=3.9"
        dependencies = ["child"]

        [tool.uv.sources]
        child = { path = "child" }

        [tool.uv.extra-build-dependencies]
        wrong_name = ["anyio"]
    "#})?;

    context.venv().arg("--clear").assert().success();
    uv_snapshot!(context.filters(), context.sync(), @r"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    warning: The `extra-build-dependencies` option is experimental and may change without warning. Pass `--preview-features extra-build-dependencies` to disable this warning.
    Resolved [N] packages in [TIME]
      × Failed to build `child @ file://[TEMP_DIR]/child`
      ├─▶ The build backend returned an error
      ╰─▶ Call to `build_backend.build_wheel` failed (exit status: 1)

          [stderr]
          Missing `anyio` module

          hint: This usually indicates a problem with the package or the build environment.
      help: `child` was included because `parent` (v0.1.0) depends on `child`
    ");

    // Write a test package that arbitrarily bans `anyio` at build time
    let bad_child = context.temp_dir.child("bad_child");
    bad_child.create_dir_all()?;
    let bad_child_pyproject_toml = bad_child.child("pyproject.toml");
    bad_child_pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "bad_child"
        version = "0.1.0"
        requires-python = ">=3.9"

        [build-system]
        requires = ["hatchling"]
        backend-path = ["."]
        build-backend = "build_backend"
    "#})?;
    let build_backend = bad_child.child("build_backend.py");
    build_backend.write_str(indoc! {r#"
        import sys

        from hatchling.build import *

        try:
            import anyio
        except ModuleNotFoundError:
            pass
        else:
            print("Found `anyio` module", file=sys.stderr)
            sys.exit(1)
    "#})?;
    bad_child.child("src/bad_child/__init__.py").touch()?;

    // Depend on `bad_child` too
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "parent"
        version = "0.1.0"
        requires-python = ">=3.9"
        dependencies = ["child", "bad_child"]

        [tool.uv.sources]
        child = { path = "child" }
        bad_child = { path = "bad_child" }

        [tool.uv.extra-build-dependencies]
        child = ["anyio"]
        bad_child = ["anyio"]
    "#})?;

    // Confirm that `bad_child` fails if anyio is provided
    context.venv().arg("--clear").assert().success();
    uv_snapshot!(context.filters(), context.sync(), @r"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    warning: The `extra-build-dependencies` option is experimental and may change without warning. Pass `--preview-features extra-build-dependencies` to disable this warning.
    Resolved [N] packages in [TIME]
      × Failed to build `bad-child @ file://[TEMP_DIR]/bad_child`
      ├─▶ The build backend returned an error
      ╰─▶ Call to `build_backend.build_wheel` failed (exit status: 1)

          [stderr]
          Found `anyio` module

          hint: This usually indicates a problem with the package or the build environment.
      help: `bad-child` was included because `parent` (v0.1.0) depends on `bad-child`
    ");

    // But `anyio` is not provided to `bad_child` if scoped to `child`
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "parent"
        version = "0.1.0"
        requires-python = ">=3.9"
        dependencies = ["child", "bad_child"]

        [tool.uv.sources]
        child = { path = "child" }
        bad_child = { path = "bad_child" }

        [tool.uv.extra-build-dependencies]
        child = ["anyio"]
    "#})?;

    context.venv().arg("--clear").assert().success();
    uv_snapshot!(context.filters(), context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: The `extra-build-dependencies` option is experimental and may change without warning. Pass `--preview-features extra-build-dependencies` to disable this warning.
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + bad-child==0.1.0 (from file://[TEMP_DIR]/bad_child)
     + child==0.1.0 (from file://[TEMP_DIR]/child)
    ");

    Ok(())
}

#[test]
fn sync_extra_build_dependencies_setuptools_legacy() -> Result<()> {
    let context = TestContext::new("3.12").with_filtered_counts();

    // Write a test package that uses legacy setuptools (no pyproject.toml) and requires `anyio` at build time
    let child = context.temp_dir.child("child");
    child.create_dir_all()?;

    // Create a setup.py that checks for anyio during build
    let setup_py = child.child("setup.py");
    setup_py.write_str(indoc! {r#"
        import sys
        from setuptools import setup, find_packages

        try:
            import anyio
            print("anyio is available!", file=sys.stderr)
        except ModuleNotFoundError:
            print("Missing `anyio` module", file=sys.stderr)
            sys.exit(1)

        setup(
            name="child",
            version="0.1.0",
            packages=find_packages(),
        )
    "#})?;
    child.child("child").create_dir_all()?;
    child.child("child/__init__.py").touch()?;

    let parent = &context.temp_dir;
    let pyproject_toml = parent.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "parent"
        version = "0.1.0"
        requires-python = ">=3.9"
        dependencies = ["child"]

        [tool.uv.sources]
        child = { path = "child" }
    "#})?;

    // Running `uv sync` should fail due to missing build-dependencies
    uv_snapshot!(context.filters(), context.sync(), @r"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to build `child @ file://[TEMP_DIR]/child`
      ├─▶ The build backend returned an error
      ╰─▶ Call to `setuptools.build_meta:__legacy__.build_wheel` failed (exit status: 1)

          [stderr]
          Missing `anyio` module

          hint: This usually indicates a problem with the package or the build environment.
    ");

    // Adding `extra-build-dependencies` should solve the issue
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "parent"
        version = "0.1.0"
        requires-python = ">=3.9"
        dependencies = ["child"]

        [tool.uv.sources]
        child = { path = "child" }

        [tool.uv.extra-build-dependencies]
        child = ["anyio"]
    "#})?;

    context.venv().arg("--clear").assert().success();
    uv_snapshot!(context.filters(), context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: The `extra-build-dependencies` option is experimental and may change without warning. Pass `--preview-features extra-build-dependencies` to disable this warning.
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + child==0.1.0 (from file://[TEMP_DIR]/child)
    ");

    Ok(())
}

#[test]
fn sync_extra_build_dependencies_setuptools() -> Result<()> {
    let context = TestContext::new("3.12").with_filtered_counts();

    // Write a test package that uses setuptools with pyproject.toml and requires `anyio` at build time
    let child = context.temp_dir.child("child");
    child.create_dir_all()?;

    // Create a pyproject.toml that uses setuptools
    let child_pyproject_toml = child.child("pyproject.toml");
    child_pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "child"
        version = "0.1.0"
        requires-python = ">=3.9"

        [build-system]
        requires = ["setuptools"]
        build-backend = "setuptools.build_meta"
    "#})?;

    // Create a setup.py that checks for anyio during build
    let setup_py = child.child("setup.py");
    setup_py.write_str(indoc! {r#"
        import sys
        from setuptools import setup, find_packages

        try:
            import anyio
            print("anyio is available!", file=sys.stderr)
        except ModuleNotFoundError:
            print("Missing `anyio` module", file=sys.stderr)
            sys.exit(1)

        setup(
            name="child",
            version="0.1.0",
            packages=find_packages(),
        )
    "#})?;
    child.child("child").create_dir_all()?;
    child.child("child/__init__.py").touch()?;

    let parent = &context.temp_dir;
    let pyproject_toml = parent.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "parent"
        version = "0.1.0"
        requires-python = ">=3.9"
        dependencies = ["child"]

        [tool.uv.sources]
        child = { path = "child" }
    "#})?;

    // Running `uv sync` should fail due to missing build-dependencies
    uv_snapshot!(context.filters(), context.sync(), @r"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
      × Failed to build `child @ file://[TEMP_DIR]/child`
      ├─▶ The build backend returned an error
      ╰─▶ Call to `setuptools.build_meta.build_wheel` failed (exit status: 1)

          [stderr]
          Missing `anyio` module

          hint: This usually indicates a problem with the package or the build environment.
      help: `child` was included because `parent` (v0.1.0) depends on `child`
    ");

    // Adding `extra-build-dependencies` should solve the issue
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "parent"
        version = "0.1.0"
        requires-python = ">=3.9"
        dependencies = ["child"]

        [tool.uv.sources]
        child = { path = "child" }

        [tool.uv.extra-build-dependencies]
        child = ["anyio"]
    "#})?;

    context.venv().arg("--clear").assert().success();
    uv_snapshot!(context.filters(), context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: The `extra-build-dependencies` option is experimental and may change without warning. Pass `--preview-features extra-build-dependencies` to disable this warning.
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + child==0.1.0 (from file://[TEMP_DIR]/child)
    ");

    Ok(())
}

#[test]
fn sync_extra_build_dependencies_sources() -> Result<()> {
    let context = TestContext::new("3.12").with_filtered_counts();

    let anyio_local = context.workspace_root.join("scripts/packages/anyio_local");

    // Write a test package that arbitrarily requires `anyio` at a specific _path_ at build time
    let child = context.temp_dir.child("child");
    child.create_dir_all()?;
    let child_pyproject_toml = child.child("pyproject.toml");
    child_pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "child"
        version = "0.1.0"
        requires-python = ">=3.9"

        [build-system]
        requires = ["hatchling"]
        backend-path = ["."]
        build-backend = "build_backend"
    "#})?;
    let build_backend = child.child("build_backend.py");
    build_backend.write_str(&formatdoc! {r#"
        import sys

        from hatchling.build import *

        try:
            import anyio
        except ModuleNotFoundError:
            print("Missing `anyio` module", file=sys.stderr)
            sys.exit(1)

        # Check that we got the local version of anyio by checking for the marker
        if not hasattr(anyio, 'LOCAL_ANYIO_MARKER'):
            print("Found system anyio instead of local anyio", file=sys.stderr)
            sys.exit(1)
    "#})?;
    child.child("src/child/__init__.py").touch()?;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(&formatdoc! {r#"
            [project]
            name = "project"
            version = "0.1.0"
            requires-python = ">=3.12"
            dependencies = ["child"]

            [tool.uv.sources]
            anyio = {{ path = "{anyio_local}" }}
            child = {{ path = "child" }}

            [tool.uv.extra-build-dependencies]
            child = ["anyio"]
        "#,
        anyio_local = anyio_local.portable_display(),
    })?;

    // Running `uv sync` should succeed, as `anyio` is provided as a source
    uv_snapshot!(context.filters(), context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: The `extra-build-dependencies` option is experimental and may change without warning. Pass `--preview-features extra-build-dependencies` to disable this warning.
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + child==0.1.0 (from file://[TEMP_DIR]/child)
    ");

    // TODO(zanieb): We want to test with `--no-sources` too but unfortunately that's not easy
    // because it'll disable the `child` path source too!

    Ok(())
}

#[test]
fn sync_extra_build_dependencies_index() -> Result<()> {
    let context = TestContext::new("3.12").with_filtered_counts();

    // Write a test package that arbitrarily requires `anyio` at build time
    let child = context.temp_dir.child("child");
    child.create_dir_all()?;
    let child_pyproject_toml = child.child("pyproject.toml");
    child_pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "child"
        version = "0.1.0"
        requires-python = ">=3.9"

        [build-system]
        requires = ["hatchling", "anyio"]
        backend-path = ["."]
        build-backend = "build_backend"
    "#})?;

    // Create a build backend that checks for a specific version of anyio
    let build_backend = child.child("build_backend.py");
    build_backend.write_str(indoc! {r#"
        import os
        import sys
        from hatchling.build import *

        expected_version = os.environ.get("EXPECTED_ANYIO_VERSION", "")
        if not expected_version:
            print("`EXPECTED_ANYIO_VERSION` not set", file=sys.stderr)
            sys.exit(1)

        try:
            import anyio
        except ModuleNotFoundError:
            print("Missing `anyio` module", file=sys.stderr)
            sys.exit(1)

        from importlib.metadata import version
        anyio_version = version("anyio")

        if not anyio_version.startswith(expected_version):
            print(f"Expected `anyio` version {expected_version} but got {anyio_version}", file=sys.stderr)
            sys.exit(1)

        print(f"Found expected `anyio` version {anyio_version}", file=sys.stderr)
    "#})?;
    child.child("src/child/__init__.py").touch()?;

    let parent = &context.temp_dir;
    let pyproject_toml = parent.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "parent"
        version = "0.1.0"
        requires-python = ">=3.9"
        dependencies = ["child"]

        [tool.uv.sources]
        child = { path = "child" }
    "#})?;

    // Ensure our build backend is checking the version correctly
    uv_snapshot!(context.filters(), context.sync().env("EXPECTED_ANYIO_VERSION", "3.0"), @r"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
      × Failed to build `child @ file://[TEMP_DIR]/child`
      ├─▶ The build backend returned an error
      ╰─▶ Call to `build_backend.build_wheel` failed (exit status: 1)

          [stderr]
          Expected `anyio` version 3.0 but got 4.3.0

          hint: This usually indicates a problem with the package or the build environment.
      help: `child` was included because `parent` (v0.1.0) depends on `child`
    ");

    // Ensure that we're resolving to `4.3.0`, the "latest" on PyPI.
    uv_snapshot!(context.filters(), context.sync().env("EXPECTED_ANYIO_VERSION", "4.3"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + child==0.1.0 (from file://[TEMP_DIR]/child)
    ");

    // Pin `anyio` to the Test PyPI.
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "parent"
        version = "0.1.0"
        requires-python = ">=3.9"
        dependencies = ["child"]

        [tool.uv.sources]
        child = { path = "child" }
        anyio = { index = "test" }

        [tool.uv.extra-build-dependencies]
        child = ["anyio"]

        [[tool.uv.index]]
        url = "https://test.pypi.org/simple"
        name = "test"
        explicit = true
    "#})?;

    // The child should be rebuilt with `3.5` on reinstall, the "latest" on Test PyPI.
    uv_snapshot!(context.filters(), context.sync()
        .arg("--reinstall-package").arg("child").env("EXPECTED_ANYIO_VERSION", "4.3"), @r"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    warning: The `extra-build-dependencies` option is experimental and may change without warning. Pass `--preview-features extra-build-dependencies` to disable this warning.
    Resolved [N] packages in [TIME]
      × Failed to build `child @ file://[TEMP_DIR]/child`
      ├─▶ The build backend returned an error
      ╰─▶ Call to `build_backend.build_wheel` failed (exit status: 1)

          [stderr]
          Expected `anyio` version 4.3 but got 3.5.0

          hint: This usually indicates a problem with the package or the build environment.
      help: `child` was included because `parent` (v0.1.0) depends on `child`
    ");

    uv_snapshot!(context.filters(), context.sync()
        .arg("--reinstall-package").arg("child").env("EXPECTED_ANYIO_VERSION", "3.5"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: The `extra-build-dependencies` option is experimental and may change without warning. Pass `--preview-features extra-build-dependencies` to disable this warning.
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Uninstalled [N] packages in [TIME]
    Installed [N] packages in [TIME]
     ~ child==0.1.0 (from file://[TEMP_DIR]/child)
    ");

    Ok(())
}

#[test]
fn sync_extra_build_dependencies_sources_from_child() -> Result<()> {
    let context = TestContext::new("3.12").with_filtered_counts();

    let anyio_local = context.workspace_root.join("scripts/packages/anyio_local");

    // Write a test package that arbitrarily requires `anyio` at a specific _path_ at build time
    let child = context.temp_dir.child("child");
    child.create_dir_all()?;
    let child_pyproject_toml = child.child("pyproject.toml");
    child_pyproject_toml.write_str(&formatdoc! {r#"
        [project]
        name = "child"
        version = "0.1.0"
        requires-python = ">=3.9"

        [build-system]
        requires = ["hatchling"]
        backend-path = ["."]
        build-backend = "build_backend"

        [tool.uv.sources]
        anyio = {{ path = "{}" }}
    "#, anyio_local.portable_display()
    })?;
    let build_backend = child.child("build_backend.py");
    build_backend.write_str(&formatdoc! {r#"
        import sys

        from hatchling.build import *

        try:
            import anyio
        except ModuleNotFoundError:
            print("Missing `anyio` module", file=sys.stderr)
            sys.exit(1)

        # Check that we got the local version of anyio by checking for the marker
        if not hasattr(anyio, 'LOCAL_ANYIO_MARKER'):
            print("Found system anyio instead of local anyio", file=sys.stderr)
            sys.exit(1)
    "#})?;
    child.child("src/child/__init__.py").touch()?;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
            [project]
            name = "project"
            version = "0.1.0"
            requires-python = ">=3.12"
            dependencies = ["child"]

            [tool.uv.sources]
            child = { path = "child" }

            [tool.uv.extra-build-dependencies]
            child = ["anyio"]
        "#,
    })?;

    // Running `uv sync` should fail due to the unapplied source
    uv_snapshot!(context.filters(), context.sync().arg("--reinstall").arg("--refresh"), @r"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    warning: The `extra-build-dependencies` option is experimental and may change without warning. Pass `--preview-features extra-build-dependencies` to disable this warning.
    Resolved [N] packages in [TIME]
      × Failed to build `child @ file://[TEMP_DIR]/child`
      ├─▶ The build backend returned an error
      ╰─▶ Call to `build_backend.build_wheel` failed (exit status: 1)

          [stderr]
          Found system anyio instead of local anyio

          hint: This usually indicates a problem with the package or the build environment.
      help: `child` was included because `project` (v0.1.0) depends on `child`
    ");

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
    uv_snapshot!(context.filters(), context.sync(), @r"
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
    ");

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

    uv_snapshot!(context.filters(), context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 2 packages in [TIME]
     + ok==1.0.0 (from file://[TEMP_DIR]/wheels/ok-1.0.0-py3-none-any.whl)
     + relative-wheel==0.1.0 (from file://[TEMP_DIR]/)
    ");

    let lock = context.read("uv.lock");

    insta::with_settings!(
        {
            filters => context.filters(),
        },
        {
            assert_snapshot!(
                lock, @r#"
            version = 1
            revision = 3
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
            "#
            );
        }
    );

    // Check that we can re-read the lockfile.
    uv_snapshot!(context.filters(), context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Audited 2 packages in [TIME]
    ");

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

    uv_snapshot!(context.filters(), context.sync().arg("--only-dev"), @r"
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
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--no-dev"), @r"
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
    ");

    uv_snapshot!(context.filters(), context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + sniffio==1.3.1
    ");

    // Using `--no-default-groups` should remove dev dependencies
    uv_snapshot!(context.filters(), context.sync().arg("--no-default-groups"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Uninstalled 3 packages in [TIME]
     - anyio==4.3.0
     - idna==3.6
     - sniffio==1.3.1
    ");

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

    uv_snapshot!(context.filters(), context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 10 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + iniconfig==2.0.0
     + typing-extensions==4.10.0
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--group").arg("foo"), @r"
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
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--only-group").arg("bar"), @r"
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
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--group").arg("foo").arg("--group").arg("bar"), @r"
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
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--all-groups"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 10 packages in [TIME]
    Audited 9 packages in [TIME]
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--all-groups").arg("--no-group").arg("bar"), @r"
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
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--all-groups").arg("--no-dev"), @r"
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
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--dev"), @r"
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
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--dev").arg("--no-group").arg("dev"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 10 packages in [TIME]
    Uninstalled 1 package in [TIME]
     - iniconfig==2.0.0
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--group").arg("dev").arg("--no-dev"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 10 packages in [TIME]
    Audited 1 package in [TIME]
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--all-groups"), @r"
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
    ");

    // Using `--no-default-groups` should exclude all groups
    uv_snapshot!(context.filters(), context.sync().arg("--no-default-groups"), @r"
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
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--all-groups"), @r"
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
    ");

    // Using `--no-default-groups` with `--group foo` and `--group bar` should include those groups,
    // excluding the remaining `dev` group.
    uv_snapshot!(context.filters(), context.sync().arg("--no-default-groups").arg("--group").arg("foo").arg("--group").arg("bar"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 10 packages in [TIME]
    Uninstalled 1 package in [TIME]
     - iniconfig==2.0.0
    ");

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

    uv_snapshot!(context.filters(), context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + typing-extensions==4.10.0
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--group").arg("foo"), @r"
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
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--only-group").arg("bar"), @r"
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
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--group").arg("foo").arg("--group").arg("bar"), @r"
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
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--only-group").arg("foo"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Uninstalled 1 package in [TIME]
     - typing-extensions==4.10.0
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--all-groups"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Installed 1 package in [TIME]
     + typing-extensions==4.10.0
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--no-default-groups"), @r"
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
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--all-groups"), @r"
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
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--no-default-groups").arg("--group").arg("foo"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Audited 5 packages in [TIME]
    ");

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

    uv_snapshot!(context.filters(), context.sync().arg("--group").arg("foo"), @r"
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
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--group").arg("foo").arg("--no-group").arg("foo"), @r"
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
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--only-group").arg("bar"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
     - typing-extensions==4.10.0
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--only-group").arg("bar").arg("--no-group").arg("bar"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Uninstalled 1 package in [TIME]
     - iniconfig==2.0.0
    ");

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

    uv_snapshot!(context.filters(), context.sync(), @r"
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
    ");

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
    uv_snapshot!(context.filters(), context.sync().arg("--group").arg("baz"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
    error: Group `baz` is not defined in the project's `dependency-groups` table
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--no-group").arg("baz"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
    error: Group `baz` is not defined in the project's `dependency-groups` table
    ");

    // Requesting an empty group should succeed.
    uv_snapshot!(context.filters(), context.sync().arg("--group").arg("foo"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + typing-extensions==4.10.0
    ");

    // Requesting with `--frozen` should respect the groups in the lockfile, rather than the
    // `pyproject.toml`.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen").arg("--group").arg("bar"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 5 packages in [TIME]
    Installed 5 packages in [TIME]
     + certifi==2024.2.2
     + charset-normalizer==3.3.2
     + idna==3.6
     + requests==2.31.0
     + urllib3==2.2.1
    ");

    // Replace `bar` with `baz`.
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["typing-extensions"]

        [dependency-groups]
        baz = ["iniconfig"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.sync().arg("--frozen").arg("--group").arg("bar"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 6 packages in [TIME]
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--frozen").arg("--group").arg("baz"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Group `baz` is not defined in the project's `dependency-groups` table
    ");

    Ok(())
}

#[test]
fn sync_corner_groups() -> Result<()> {
    // Testing a bunch of random corner cases of flags so their behaviour is tracked.
    // It's fine if we decide we want to support these later!
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
        foo = ["sniffio"]
        bar = ["requests"]
        "#,
    )?;

    context.lock().assert().success();

    // --no-dev and --only-dev should error
    // (This one could be made to work with overloading)
    uv_snapshot!(context.filters(), context.sync()
        .arg("--no-dev")
        .arg("--only-dev"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: the argument '--no-dev' cannot be used with '--only-dev'

    Usage: uv sync --cache-dir [CACHE_DIR] --no-dev --exclude-newer <EXCLUDE_NEWER>

    For more information, try '--help'.
    ");

    // --dev and --only-group should error if they don't match
    uv_snapshot!(context.filters(), context.sync()
        .arg("--dev")
        .arg("--only-group").arg("bar"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: the argument '--dev' cannot be used with '--only-group <ONLY_GROUP>'

    Usage: uv sync --cache-dir [CACHE_DIR] --exclude-newer <EXCLUDE_NEWER>

    For more information, try '--help'.
    ");

    // --dev and --only-group should error even if it's dev still
    // (This one could be made to work the same as --dev --only-dev)
    uv_snapshot!(context.filters(), context.sync()
        .arg("--dev")
        .arg("--only-group").arg("dev"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: the argument '--dev' cannot be used with '--only-group <ONLY_GROUP>'

    Usage: uv sync --cache-dir [CACHE_DIR] --exclude-newer <EXCLUDE_NEWER>

    For more information, try '--help'.
    ");

    // --group and --only-dev should error if they don't match
    // (This one could be made to work the same as --dev --only-dev)
    uv_snapshot!(context.filters(), context.sync()
        .arg("--only-dev")
        .arg("--group").arg("bar"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: the argument '--only-dev' cannot be used with '--group <GROUP>'

    Usage: uv sync --cache-dir [CACHE_DIR] --only-dev --exclude-newer <EXCLUDE_NEWER>

    For more information, try '--help'.
    ");

    // --group and --only-dev should error even if it's dev still
    // (This one could be made to work the same as --dev --only-dev)
    uv_snapshot!(context.filters(), context.sync()
        .arg("--only-dev")
        .arg("--group").arg("dev"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: the argument '--only-dev' cannot be used with '--group <GROUP>'

    Usage: uv sync --cache-dir [CACHE_DIR] --only-dev --exclude-newer <EXCLUDE_NEWER>

    For more information, try '--help'.
    ");

    // --all-groups and --only-dev should error
    uv_snapshot!(context.filters(), context.sync()
        .arg("--all-groups")
        .arg("--only-dev"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: the argument '--all-groups' cannot be used with '--only-dev'

    Usage: uv sync --cache-dir [CACHE_DIR] --all-groups --exclude-newer <EXCLUDE_NEWER>

    For more information, try '--help'.
    ");

    // --all-groups and --only-group should error
    uv_snapshot!(context.filters(), context.sync()
        .arg("--all-groups")
        .arg("--only-group").arg("bar"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: the argument '--all-groups' cannot be used with '--only-group <ONLY_GROUP>'

    Usage: uv sync --cache-dir [CACHE_DIR] --all-groups --exclude-newer <EXCLUDE_NEWER>

    For more information, try '--help'.
    ");

    // --group and --only-group should error if they name disjoint things
    uv_snapshot!(context.filters(), context.sync()
        .arg("--group").arg("foo")
        .arg("--only-group").arg("bar"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: the argument '--group <GROUP>' cannot be used with '--only-group <ONLY_GROUP>'

    Usage: uv sync --cache-dir [CACHE_DIR] --group <GROUP> --exclude-newer <EXCLUDE_NEWER>

    For more information, try '--help'.
    ");

    // --group and --only-group should error if they name same things
    // (This one would be fair to allow, but... is it worth it?)
    uv_snapshot!(context.filters(), context.sync()
        .arg("--group").arg("foo")
        .arg("--only-group").arg("foo"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: the argument '--group <GROUP>' cannot be used with '--only-group <ONLY_GROUP>'

    Usage: uv sync --cache-dir [CACHE_DIR] --group <GROUP> --exclude-newer <EXCLUDE_NEWER>

    For more information, try '--help'.
    ");

    // --all-groups and --no-default-groups is redundant but should be --all-groups
    uv_snapshot!(context.filters(), context.sync()
        .arg("--all-groups")
        .arg("--no-default-groups"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 9 packages in [TIME]
    Prepared 8 packages in [TIME]
    Installed 8 packages in [TIME]
     + certifi==2024.2.2
     + charset-normalizer==3.3.2
     + idna==3.6
     + iniconfig==2.0.0
     + requests==2.31.0
     + sniffio==1.3.1
     + typing-extensions==4.10.0
     + urllib3==2.2.1
    ");

    // --dev --only-dev should saturate as --only-dev
    uv_snapshot!(context.filters(), context.sync()
        .arg("--dev")
        .arg("--only-dev"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 9 packages in [TIME]
    Uninstalled 7 packages in [TIME]
     - certifi==2024.2.2
     - charset-normalizer==3.3.2
     - idna==3.6
     - requests==2.31.0
     - sniffio==1.3.1
     - typing-extensions==4.10.0
     - urllib3==2.2.1
    ");
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
    error: Default group `bar` (from `tool.uv.default-groups`) is not defined in the project's `dependency-groups` table
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
    uv_snapshot!(context.filters(), context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 10 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + iniconfig==2.0.0
     + typing-extensions==4.10.0
    ");

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

    uv_snapshot!(context.filters(), context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 10 packages in [TIME]
    Uninstalled 1 package in [TIME]
     - iniconfig==2.0.0
    ");

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

    uv_snapshot!(context.filters(), context.sync(), @r"
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
    ");

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

    uv_snapshot!(context.filters(), context.sync().arg("--no-group").arg("foo"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 10 packages in [TIME]
    Uninstalled 3 packages in [TIME]
     - anyio==4.3.0
     - idna==3.6
     - sniffio==1.3.1
    ");

    // Using `--group` should include the defaults
    uv_snapshot!(context.filters(), context.sync().arg("--group").arg("dev"), @r"
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
    ");

    // Using `--all-groups` should include the defaults
    uv_snapshot!(context.filters(), context.sync().arg("--all-groups"), @r"
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
    ");

    // Using `--only-group` should exclude the defaults
    uv_snapshot!(context.filters(), context.sync().arg("--only-group").arg("dev"), @r"
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
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--all-groups"), @r"
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
    ");

    // Using `--no-default-groups` should exclude all groups
    uv_snapshot!(context.filters(), context.sync().arg("--no-default-groups"), @r"
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
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--all-groups"), @r"
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
    ");

    // Using `--no-default-groups` with `--group foo` and `--group bar` should include those groups,
    // excluding the remaining `dev` group.
    uv_snapshot!(context.filters(), context.sync().arg("--no-default-groups").arg("--group").arg("foo").arg("--group").arg("bar"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 10 packages in [TIME]
    Uninstalled 1 package in [TIME]
     - iniconfig==2.0.0
    ");

    Ok(())
}

/// default-groups = "all" sugar works
#[test]
fn sync_default_groups_all() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "myproject"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["typing-extensions"]

        [dependency-groups]
        dev = ["iniconfig"]
        foo = ["anyio"]
        bar = ["requests"]

        [tool.uv]
        default-groups = "all"
        "#,
    )?;

    context.lock().assert().success();

    // groups = "all" should behave like --all-groups in contexts where defaults exist
    uv_snapshot!(context.filters(), context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 10 packages in [TIME]
    Prepared 9 packages in [TIME]
    Installed 9 packages in [TIME]
     + anyio==4.3.0
     + certifi==2024.2.2
     + charset-normalizer==3.3.2
     + idna==3.6
     + iniconfig==2.0.0
     + requests==2.31.0
     + sniffio==1.3.1
     + typing-extensions==4.10.0
     + urllib3==2.2.1
    ");

    // Using `--no-default-groups` should still work
    uv_snapshot!(context.filters(), context.sync().arg("--no-default-groups"), @r"
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
    ");

    // Using `--all-groups` should be redundant and work fine
    uv_snapshot!(context.filters(), context.sync().arg("--all-groups"), @r"
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
    ");

    // Using `--no-dev` should exclude just the dev group
    uv_snapshot!(context.filters(), context.sync().arg("--no-dev"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 10 packages in [TIME]
    Uninstalled 1 package in [TIME]
     - iniconfig==2.0.0
    ");

    // Using `--group` should be redundant and still work fine
    uv_snapshot!(context.filters(), context.sync().arg("--group").arg("foo"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 10 packages in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    // Using `--only-group` should still disable defaults
    uv_snapshot!(context.filters(), context.sync().arg("--only-group").arg("foo"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 10 packages in [TIME]
    Uninstalled 6 packages in [TIME]
     - certifi==2024.2.2
     - charset-normalizer==3.3.2
     - iniconfig==2.0.0
     - requests==2.31.0
     - typing-extensions==4.10.0
     - urllib3==2.2.1
    ");

    Ok(())
}

/// default-groups = "gibberish" error
#[test]
fn sync_default_groups_gibberish() -> Result<()> {
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

        [tool.uv]
        default-groups = "gibberish"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.sync(), @r#"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to parse: `pyproject.toml`
      Caused by: TOML parse error at line 14, column 26
       |
    14 |         default-groups = "gibberish"
       |                          ^^^^^^^^^^^
    default-groups must be "all" or a ["list", "of", "groups"]
    "#);

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

    uv_snapshot!(context.filters(), context.sync().arg("--only-group").arg("foo"), @r"
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
    ");

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
            lock, @r#"
        version = 1
        revision = 3
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
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646, upload-time = "2023-01-07T11:08:11.254Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892, upload-time = "2023-01-07T11:08:09.864Z" },
        ]

        [[package]]
        name = "typing-extensions"
        version = "4.10.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/16/3a/0d26ce356c7465a19c9ea8814b960f8a36c3b0d07c323176620b7b483e44/typing_extensions-4.10.0.tar.gz", hash = "sha256:b0abd7c89e8fb96f98db18d86106ff1d90ab692004eb746cf6eda2682f91b3cb", size = 77558, upload-time = "2024-02-25T22:12:49.693Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/f9/de/dc04a3ea60b22624b51c703a84bbe0184abcd1d0b9bc8074b5d6b7ab90bb/typing_extensions-4.10.0-py3-none-any.whl", hash = "sha256:69b1a937c3a517342112fb4c6df7e72fc39a38e7891a5730ed4985b5214b5475", size = 33926, upload-time = "2024-02-25T22:12:47.72Z" },
        ]
        "#
        );
    });

    uv_snapshot!(context.filters(), context.sync().arg("--only-group").arg("foo"), @r"
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
    ");

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
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426, upload-time = "2023-11-25T15:40:54.902Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567, upload-time = "2023-11-25T15:40:52.604Z" },
        ]

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646, upload-time = "2023-01-07T11:08:11.254Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892, upload-time = "2023-01-07T11:08:09.864Z" },
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
        provides-extras = ["test"]

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
        sdist = { url = "https://files.pythonhosted.org/packages/16/3a/0d26ce356c7465a19c9ea8814b960f8a36c3b0d07c323176620b7b483e44/typing_extensions-4.10.0.tar.gz", hash = "sha256:b0abd7c89e8fb96f98db18d86106ff1d90ab692004eb746cf6eda2682f91b3cb", size = 77558, upload-time = "2024-02-25T22:12:49.693Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/f9/de/dc04a3ea60b22624b51c703a84bbe0184abcd1d0b9bc8074b5d6b7ab90bb/typing_extensions-4.10.0-py3-none-any.whl", hash = "sha256:69b1a937c3a517342112fb4c6df7e72fc39a38e7891a5730ed4985b5214b5475", size = 33926, upload-time = "2024-02-25T22:12:47.72Z" },
        ]
        "#
        );
    });

    uv_snapshot!(context.filters(), context.sync().arg("--only-group").arg("foo"), @r"
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
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--only-group").arg("bar"), @r"
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
    ");

    Ok(())
}

#[test]
fn sync_non_existent_extra() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        [project.optional-dependencies]
        types = ["sniffio>1"]
        async = ["anyio>3"]
        "#,
    )?;

    context.lock().assert().success();

    // Requesting a non-existent extra should fail.
    uv_snapshot!(context.filters(), context.sync().arg("--extra").arg("baz"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    error: Extra `baz` is not defined in the project's `optional-dependencies` table
    ");

    // Excluding a non-existing extra when requesting all extras should fail.
    uv_snapshot!(context.filters(), context.sync().arg("--all-extras").arg("--no-extra").arg("baz"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    error: Extra `baz` is not defined in the project's `optional-dependencies` table
    ");

    Ok(())
}

#[test]
fn sync_non_existent_extra_no_optional_dependencies() -> Result<()> {
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

    context.lock().assert().success();

    // Requesting a non-existent extra should fail.
    uv_snapshot!(context.filters(), context.sync().arg("--extra").arg("baz"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    error: Extra `baz` is not defined in the project's `optional-dependencies` table
    ");

    // Excluding a non-existing extra when requesting all extras should fail.
    uv_snapshot!(context.filters(), context.sync().arg("--all-extras").arg("--no-extra").arg("baz"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    error: Extra `baz` is not defined in the project's `optional-dependencies` table
    ");

    Ok(())
}

/// Ensures that we do not perform validation of extras against a lock file that was generated on a
/// version of uv that predates when `provides-extras` feature was added.
#[test]
fn sync_ignore_extras_check_when_no_provides_extras() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        [project.optional-dependencies]
        types = ["sniffio>1"]
        "#,
    )?;

    // Write a lockfile that does not have `provides-extra`, simulating a version that predates when
    // the feature was added.
    context.temp_dir.child("uv.lock").write_str(indoc! {r#"
        version = 1
        requires-python = ">=3.12"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }

        [package.optional-dependencies]
        types = [
            { name = "sniffio" },
        ]

        [package.metadata]
        requires-dist = [{ name = "sniffio", marker = "extra == 'types'", specifier = ">1" }]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 },
        ]
    "#})?;

    // Requesting a non-existent extra should not fail, as no validation should be performed.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen").arg("--extra").arg("baz"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited in [TIME]
    ");

    Ok(())
}

#[test]
fn sync_workspace_members_with_transitive_dependencies() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [tool.uv.workspace]
        members = [
            "packages/*",
        ]
        "#,
    )?;

    let packages = context.temp_dir.child("packages");
    packages.create_dir_all()?;

    // Create three workspace members with transitive dependency from
    // pkg-c -> pkg-b -> pkg-a
    let pkg_a = packages.child("pkg-a");
    pkg_a.create_dir_all()?;
    let pkg_a_pyproject_toml = pkg_a.child("pyproject.toml");
    pkg_a_pyproject_toml.write_str(
        r#"
        [project]
        name = "pkg-a"
        version = "0.0.1"
        requires-python = ">=3.12"
        dependencies = ["anyio"]
        "#,
    )?;

    let pkg_b = packages.child("pkg-b");
    pkg_b.create_dir_all()?;
    let pkg_b_pyproject_toml = pkg_b.child("pyproject.toml");
    pkg_b_pyproject_toml.write_str(
        r#"
        [project]
        name = "pkg-b"
        version = "0.0.1"
        requires-python = ">=3.12"
        dependencies = ["pkg-a"]

        [tool.uv.sources]
        pkg-a = { workspace = true }
        "#,
    )?;

    let pkg_c = packages.child("pkg-c");
    pkg_c.create_dir_all()?;
    let pkg_c_pyproject_toml = pkg_c.child("pyproject.toml");
    pkg_c_pyproject_toml.write_str(
        r#"
        [project]
        name = "pkg-c"
        version = "0.0.1"
        requires-python = ">=3.12"
        dependencies = ["pkg-b"]

        [tool.uv.sources]
        pkg-b = { workspace = true }
        "#,
    )?;

    // Syncing should build the two transitive dependencies pkg-a and pkg-b,
    // but not pkg-c, which is not a dependency.
    uv_snapshot!(context.filters(), context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 5 packages in [TIME]
    Installed 5 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + pkg-a==0.0.1 (from file://[TEMP_DIR]/packages/pkg-a)
     + pkg-b==0.0.1 (from file://[TEMP_DIR]/packages/pkg-b)
     + sniffio==1.3.1
    ");

    // The lockfile should be valid.
    uv_snapshot!(context.filters(), context.lock().arg("--check"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    ");

    Ok(())
}

#[test]
fn sync_non_existent_extra_workspace_member() -> Result<()> {
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

        [tool.uv.workspace]
        members = ["child"]

        [tool.uv.sources]
        child = { workspace = true }
        "#,
    )?;

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

        [project.optional-dependencies]
        async = ["anyio>3"]
        "#,
        )?;

    context.lock().assert().success();

    // Requesting an extra that only exists in the child should fail.
    uv_snapshot!(context.filters(), context.sync().arg("--extra").arg("async"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    error: Extra `async` is not defined in the project's `optional-dependencies` table
    ");

    // Unless we sync from the child directory.
    uv_snapshot!(context.filters(), context.sync().arg("--package").arg("child").arg("--extra").arg("async"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.3.0
     + child==0.1.0 (from file://[TEMP_DIR]/child)
     + idna==3.6
     + sniffio==1.3.1
    ");

    Ok(())
}

#[test]
fn sync_non_existent_extra_non_project_workspace() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [tool.uv.workspace]
        members = ["child", "other"]
        "#,
    )?;

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

        [project.optional-dependencies]
        async = ["anyio>3"]
        "#,
        )?;

    context
        .temp_dir
        .child("other")
        .child("pyproject.toml")
        .write_str(
            r#"
        [project]
        name = "other"
        version = "0.1.0"
        requires-python = ">=3.12"
        "#,
        )?;

    context.lock().assert().success();

    // Requesting an extra that only exists in the child should succeed, since we sync all members
    // by default.
    uv_snapshot!(context.filters(), context.sync().arg("--extra").arg("async"), @r"
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
    ");

    // Syncing from the child should also succeed.
    uv_snapshot!(context.filters(), context.sync().arg("--package").arg("child").arg("--extra").arg("async"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Audited 3 packages in [TIME]
    ");

    // Syncing from an unrelated child should fail.
    uv_snapshot!(context.filters(), context.sync().arg("--package").arg("other").arg("--extra").arg("async"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    error: Extra `async` is not defined in the project's `optional-dependencies` table
    ");

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
    uv_snapshot!(context.filters(), context.sync().arg("--no-install-project"), @r"
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
    ");

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
    uv_snapshot!(context.filters(), context.sync().arg("--no-install-workspace"), @r"
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
    ");

    // Remove the virtual environment.
    fs_err::remove_dir_all(&context.venv)?;

    // We don't require the `pyproject.toml` for non-root members, if `--frozen` is provided.
    fs_err::remove_file(child.join("pyproject.toml"))?;

    uv_snapshot!(context.filters(), context.sync().arg("--no-install-workspace").arg("--frozen"), @r"
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
    ");

    // Even if `--package` is used.
    uv_snapshot!(context.filters(), context.sync().arg("--package").arg("child").arg("--no-install-workspace").arg("--frozen"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Uninstalled 3 packages in [TIME]
     - anyio==3.7.0
     - idna==3.6
     - sniffio==1.3.1
    ");

    // Unless the package doesn't exist.
    uv_snapshot!(context.filters(), context.sync().arg("--package").arg("fake").arg("--no-install-workspace").arg("--frozen"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Could not find root package `fake`
    ");

    // Even if `--all-packages` is used.
    uv_snapshot!(context.filters(), context.sync().arg("--all-packages").arg("--no-install-workspace").arg("--frozen"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed 3 packages in [TIME]
     + anyio==3.7.0
     + idna==3.6
     + sniffio==1.3.1
    ");

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
    uv_snapshot!(context.filters(), context.sync().arg("--no-install-package").arg("anyio"), @r"
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
    ");

    // Running with `--no-install-package project` should skip the project itself (not as a special
    // case, that's just the name of the project)
    uv_snapshot!(context.filters(), context.sync().arg("--no-install-package").arg("project"), @r"
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
    ");

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
    uv_snapshot!(context.filters(), context.sync().arg("--no-build"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    error: Distribution `project==0.1.0 @ editable+.` can't be installed because it is marked as `--no-build` but has no binary distribution
    ");

    // But it's fine to combine `--no-install-project` with `--no-build`. We shouldn't error, since
    // we aren't building the project.
    uv_snapshot!(context.filters(), context.sync().arg("--no-install-project").arg("--no-build").arg("--locked"), @r"
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
    ");

    Ok(())
}

#[test]
fn sync_extra_build_dependencies_script() -> Result<()> {
    let context = TestContext::new("3.12").with_filtered_counts();

    // Write a test package that arbitrarily requires `anyio` at build time
    let child = context.temp_dir.child("child");
    child.create_dir_all()?;
    let child_pyproject_toml = child.child("pyproject.toml");
    child_pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "child"
        version = "0.1.0"
        requires-python = ">=3.9"
        [build-system]
        requires = ["hatchling"]
        backend-path = ["."]
        build-backend = "build_backend"
    "#})?;
    let build_backend = child.child("build_backend.py");
    build_backend.write_str(indoc! {r#"
        import sys
        from hatchling.build import *
        try:
            import anyio
        except ModuleNotFoundError:
            print("Missing `anyio` module", file=sys.stderr)
            sys.exit(1)
    "#})?;
    child.child("src/child/__init__.py").touch()?;

    // Create a script that depends on the child package
    let script = context.temp_dir.child("script.py");
    script.write_str(indoc! {r#"
        # /// script
        # requires-python = ">=3.12"
        # dependencies = ["child"]
        #
        # [tool.uv.sources]
        # child = { path = "child" }
        # ///
    "#})?;

    let filters = context
        .filters()
        .into_iter()
        .chain(vec![(
            r"environments-v2/script-[a-z0-9]+",
            "environments-v2/script-[HASH]",
        )])
        .collect::<Vec<_>>();

    // Running `uv sync` should fail due to missing build-dependencies
    uv_snapshot!(filters, context.sync().arg("--script").arg("script.py"), @r"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Creating script environment at: [CACHE_DIR]/environments-v2/script-[HASH]
    Resolved [N] packages in [TIME]
      × Failed to build `child @ file://[TEMP_DIR]/child`
      ├─▶ The build backend returned an error
      ╰─▶ Call to `build_backend.build_wheel` failed (exit status: 1)

          [stderr]
          Missing `anyio` module

          hint: This usually indicates a problem with the package or the build environment.
    ");

    // Add extra build dependencies to the script
    script.write_str(indoc! {r#"
        # /// script
        # requires-python = ">=3.12"
        # dependencies = ["child"]
        #
        # [tool.uv.sources]
        # child = { path = "child" }
        #
        # [tool.uv.extra-build-dependencies]
        # child = ["anyio"]
        # ///
    "#})?;

    // Running `uv sync` should now succeed due to extra build-dependencies
    context.venv().arg("--clear").assert().success();
    uv_snapshot!(filters, context.sync().arg("--script").arg("script.py"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using script environment at: [CACHE_DIR]/environments-v2/script-[HASH]
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + child==0.1.0 (from file://[TEMP_DIR]/child)
    ");

    Ok(())
}

#[test]
fn sync_extra_build_dependencies_script_sources() -> Result<()> {
    let context = TestContext::new("3.12").with_filtered_counts();
    let anyio_local = context.workspace_root.join("scripts/packages/anyio_local");

    // Write a test package that arbitrarily requires `anyio` at a specific _path_ at build time
    let child = context.temp_dir.child("child");
    child.create_dir_all()?;
    let child_pyproject_toml = child.child("pyproject.toml");
    child_pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "child"
        version = "0.1.0"
        requires-python = ">=3.9"
        [build-system]
        requires = ["hatchling"]
        backend-path = ["."]
        build-backend = "build_backend"
    "#})?;
    let build_backend = child.child("build_backend.py");
    build_backend.write_str(&formatdoc! {r#"
        import sys
        from hatchling.build import *
        try:
            import anyio
        except ModuleNotFoundError:
            print("Missing `anyio` module", file=sys.stderr)
            sys.exit(1)

        # Check that we got the local version of anyio by checking for the marker
        if not hasattr(anyio, 'LOCAL_ANYIO_MARKER'):
            print("Found system anyio instead of local anyio", file=sys.stderr)
            sys.exit(1)
    "#})?;
    child.child("src/child/__init__.py").touch()?;

    // Create a script that depends on the child package
    let script = context.temp_dir.child("script.py");
    script.write_str(&formatdoc! {r#"
        # /// script
        # requires-python = ">=3.12"
        # dependencies = ["child"]
        #
        # [tool.uv.sources]
        # anyio = {{ path = "{}" }}
        # child = {{ path = "child" }}
        #
        # [tool.uv.extra-build-dependencies]
        # child = ["anyio"]
        # ///
    "#, anyio_local.portable_display()
    })?;

    let filters = context
        .filters()
        .into_iter()
        .chain(vec![(
            r"environments-v2/script-[a-z0-9]+",
            "environments-v2/script-[HASH]",
        )])
        .collect::<Vec<_>>();

    // Running `uv sync` should succeed with the sources applied
    uv_snapshot!(filters, context.sync().arg("--script").arg("script.py"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Creating script environment at: [CACHE_DIR]/environments-v2/script-[HASH]
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + child==0.1.0 (from file://[TEMP_DIR]/child)
    ");

    Ok(())
}

#[test]
fn virtual_no_build() -> Result<()> {
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

    // Clear the cache.
    fs_err::remove_dir_all(&context.cache_dir)?;

    // `--no-build` should not raise an error, since we don't install virtual projects.
    uv_snapshot!(context.filters(), context.sync().arg("--no-build"), @r"
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
    ");

    Ok(())
}

#[test]
fn virtual_no_build_dynamic_cached() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dynamic = ["dependencies"]

        [tool.setuptools.dynamic]
        dependencies = {file = ["requirements.txt"]}
        "#,
    )?;

    context
        .temp_dir
        .child("requirements.txt")
        .write_str("anyio==3.7.0")?;

    // Generate a lockfile.
    context.lock().assert().success();

    // `--no-build` should not raise an error, since we don't build or install the project (given
    // that it's virtual and the metadata is cached).
    uv_snapshot!(context.filters(), context.sync().arg("--no-build"), @r"
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
    ");

    Ok(())
}

#[test]
fn virtual_no_build_dynamic_no_cache() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dynamic = ["dependencies"]

        [tool.setuptools.dynamic]
        dependencies = {file = ["requirements.txt"]}
        "#,
    )?;

    context
        .temp_dir
        .child("requirements.txt")
        .write_str("anyio==3.7.0")?;

    // Generate a lockfile.
    context.lock().assert().success();

    // Clear the cache.
    fs_err::remove_dir_all(&context.cache_dir)?;

    // `--no-build` should raise an error, since we need to build the project.
    uv_snapshot!(context.filters(), context.sync().arg("--no-build"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to generate package metadata for `project==0.1.0 @ virtual+.`
      Caused by: Building source distributions for `project` is disabled
    ");

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
    uv_snapshot!(context.filters(), context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + iniconfig==2.0.0
     + project==0.1.0 (from file://[TEMP_DIR]/)
    ");

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646, upload-time = "2023-01-07T11:08:11.254Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892, upload-time = "2023-01-07T11:08:09.864Z" },
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
        "#
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
    uv_snapshot!(context.filters(), context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Uninstalled 1 package in [TIME]
     - project==0.1.0 (from file://[TEMP_DIR]/)
    ");

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646, upload-time = "2023-01-07T11:08:11.254Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892, upload-time = "2023-01-07T11:08:09.864Z" },
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
        "#
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
    uv_snapshot!(context.filters(), context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646, upload-time = "2023-01-07T11:08:11.254Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892, upload-time = "2023-01-07T11:08:09.864Z" },
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
        "#
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
    uv_snapshot!(context.filters(), context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + project==0.1.0 (from file://[TEMP_DIR]/)
    ");

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646, upload-time = "2023-01-07T11:08:11.254Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892, upload-time = "2023-01-07T11:08:09.864Z" },
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
        "#
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
    uv_snapshot!(context.filters(), context.sync(), @r"
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
    ");

    context
        .temp_dir
        .child(".venv")
        .assert(predicate::path::is_dir());

    // Running `uv sync` should create `foo` in the project directory when customized
    uv_snapshot!(context.filters(), context.sync().env(EnvVars::UV_PROJECT_ENVIRONMENT, "foo"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: foo
    Resolved 2 packages in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

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
    uv_snapshot!(context.filters(), context.sync().env(EnvVars::UV_PROJECT_ENVIRONMENT, "foobar/.venv"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: foobar/.venv
    Resolved 2 packages in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

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
    uv_snapshot!(context.filters(), context.sync().env(EnvVars::UV_PROJECT_ENVIRONMENT, context.temp_dir.join("bar")), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: bar
    Resolved 2 packages in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    context
        .temp_dir
        .child("bar")
        .assert(predicate::path::is_dir());

    // And, it can be outside the project
    let tempdir = tempdir_in(TestContext::test_bucket_dir())?;
    context = context.with_filtered_path(tempdir.path(), "OTHER_TEMPDIR");
    uv_snapshot!(context.filters(), context.sync().env(EnvVars::UV_PROJECT_ENVIRONMENT, tempdir.path().join(".venv")), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: [OTHER_TEMPDIR]/.venv
    Resolved 2 packages in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

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
    uv_snapshot!(context.filters(), context.venv().arg("foo").arg("--python").arg("3.11"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.11.[X] interpreter at: [PYTHON-3.11]
    warning: The requested interpreter resolved to Python 3.11.[X], which is incompatible with the project's Python requirement: `>=3.12` (from `project.requires-python`)
    Creating virtual environment at: foo
    Activate with: source foo/[BIN]/activate
    ");

    // Even with some extraneous content...
    fs_err::write(context.temp_dir.join("foo").join("file"), b"")?;

    // We can delete and use it
    uv_snapshot!(context.filters(), context.sync().env(EnvVars::UV_PROJECT_ENVIRONMENT, "foo"), @r"
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
    ");

    Ok(())
}

#[test]
fn sync_active_project_environment() -> Result<()> {
    let context = TestContext::new_with_versions(&["3.11", "3.12"])
        .with_filtered_virtualenv_bin()
        .with_filtered_python_names();

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.11"
        dependencies = ["iniconfig"]
        "#,
    )?;

    // Running `uv sync` with `VIRTUAL_ENV` should warn
    uv_snapshot!(context.filters(), context.sync().env(EnvVars::VIRTUAL_ENV, "foo"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `VIRTUAL_ENV=foo` does not match the project environment path `.venv` and will be ignored; use `--active` to target the active environment instead
    Using CPython 3.11.[X] interpreter at: [PYTHON-3.11]
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

    context
        .temp_dir
        .child("foo")
        .assert(predicate::path::missing());

    // Using `--active` should create the environment
    uv_snapshot!(context.filters(), context.sync().env(EnvVars::VIRTUAL_ENV, "foo").arg("--active"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.11.[X] interpreter at: [PYTHON-3.11]
    Creating virtual environment at: foo
    Resolved 2 packages in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    context
        .temp_dir
        .child("foo")
        .assert(predicate::path::is_dir());

    // A subsequent sync will re-use the environment
    uv_snapshot!(context.filters(), context.sync().env(EnvVars::VIRTUAL_ENV, "foo").arg("--active"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Audited 1 package in [TIME]
    ");

    // Setting both the `VIRTUAL_ENV` and `UV_PROJECT_ENVIRONMENT` is fine if they agree
    uv_snapshot!(context.filters(), context.sync()
        .arg("--active")
        .env(EnvVars::VIRTUAL_ENV, "foo")
        .env(EnvVars::UV_PROJECT_ENVIRONMENT, "foo"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Audited 1 package in [TIME]
    ");

    // If they disagree, we use `VIRTUAL_ENV` because of `--active`
    uv_snapshot!(context.filters(), context.sync()
        .arg("--active")
        .env(EnvVars::VIRTUAL_ENV, "foo")
        .env(EnvVars::UV_PROJECT_ENVIRONMENT, "bar"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Audited 1 package in [TIME]
    ");

    context
        .temp_dir
        .child("bar")
        .assert(predicate::path::missing());

    // Requesting another Python version will invalidate the environment
    uv_snapshot!(context.filters(), context.sync()
        .env(EnvVars::VIRTUAL_ENV, "foo").arg("--active").arg("-p").arg("3.12"), @r"
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
    ");

    Ok(())
}

#[test]
fn sync_active_script_environment() -> Result<()> {
    let context = TestContext::new_with_versions(&["3.11", "3.12"])
        .with_filtered_virtualenv_bin()
        .with_filtered_python_names();

    let script = context.temp_dir.child("script.py");
    script.write_str(indoc! { r#"
        # /// script
        # requires-python = ">=3.11"
        # dependencies = [
        #   "anyio",
        # ]
        # ///

        import anyio
       "#
    })?;

    // Running `uv sync --script` with `VIRTUAL_ENV` should warn
    uv_snapshot!(context.filters(), context.sync().arg("--script").arg("script.py").env(EnvVars::VIRTUAL_ENV, "foo"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `VIRTUAL_ENV=foo` does not match the script environment path `[CACHE_DIR]/environments-v2/script-[HASH]` and will be ignored; use `--active` to target the active environment instead
    Creating script environment at: [CACHE_DIR]/environments-v2/script-[HASH]
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + sniffio==1.3.1
    ");

    context
        .temp_dir
        .child("foo")
        .assert(predicate::path::missing());

    // Using `--active` should create the environment
    uv_snapshot!(context.filters(), context.sync().arg("--script").arg("script.py").env(EnvVars::VIRTUAL_ENV, "foo").arg("--active"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Creating script environment at: foo
    Resolved 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + sniffio==1.3.1
    ");

    context
        .temp_dir
        .child("foo")
        .assert(predicate::path::is_dir());

    // A subsequent sync will re-use the environment
    uv_snapshot!(context.filters(), context.sync().arg("--script").arg("script.py").env(EnvVars::VIRTUAL_ENV, "foo").arg("--active"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using script environment at: foo
    Resolved 3 packages in [TIME]
    Audited 3 packages in [TIME]
    ");

    // Requesting another Python version will invalidate the environment
    uv_snapshot!(context.filters(), context.sync()
        .arg("--script")
        .arg("script.py")
        .env(EnvVars::VIRTUAL_ENV, "foo")
        .arg("--active")
        .arg("-p")
        .arg("3.12"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Updating script environment at: foo
    Resolved 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + sniffio==1.3.1
    ");

    Ok(())
}

#[test]
fn sync_active_script_environment_json() -> Result<()> {
    let context = TestContext::new_with_versions(&["3.11", "3.12"])
        .with_filtered_virtualenv_bin()
        .with_filtered_python_names();

    let script = context.temp_dir.child("script.py");
    script.write_str(indoc! { r#"
        # /// script
        # requires-python = ">=3.11"
        # dependencies = [
        #   "anyio",
        # ]
        # ///

        import anyio
       "#
    })?;

    // Running `uv sync --script` with `VIRTUAL_ENV` should warn
    uv_snapshot!(context.filters(), context.sync()
        .arg("--script").arg("script.py")
        .arg("--output-format").arg("json")
        .env(EnvVars::VIRTUAL_ENV, "foo"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "schema": {
        "version": "preview"
      },
      "target": "script",
      "script": {
        "path": "[TEMP_DIR]/script.py"
      },
      "sync": {
        "environment": {
          "path": "[CACHE_DIR]/environments-v2/script-[HASH]",
          "python": {
            "path": "[CACHE_DIR]/environments-v2/script-[HASH]/[BIN]/[PYTHON]",
            "version": "3.11.[X]",
            "implementation": "cpython"
          }
        },
        "action": "create"
      },
      "lock": null,
      "dry_run": false
    }

    ----- stderr -----
    warning: `VIRTUAL_ENV=foo` does not match the script environment path `[CACHE_DIR]/environments-v2/script-[HASH]` and will be ignored; use `--active` to target the active environment instead
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + sniffio==1.3.1
    "#);

    context
        .temp_dir
        .child("foo")
        .assert(predicate::path::missing());

    // Using `--active` should create the environment
    uv_snapshot!(context.filters(), context.sync()
        .arg("--script").arg("script.py")
        .arg("--output-format").arg("json")
        .env(EnvVars::VIRTUAL_ENV, "foo").arg("--active"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "schema": {
        "version": "preview"
      },
      "target": "script",
      "script": {
        "path": "[TEMP_DIR]/script.py"
      },
      "sync": {
        "environment": {
          "path": "[TEMP_DIR]/foo",
          "python": {
            "path": "[TEMP_DIR]/foo/[BIN]/[PYTHON]",
            "version": "3.11.[X]",
            "implementation": "cpython"
          }
        },
        "action": "create"
      },
      "lock": null,
      "dry_run": false
    }

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + sniffio==1.3.1
    "#);

    context
        .temp_dir
        .child("foo")
        .assert(predicate::path::is_dir());

    // A subsequent sync will re-use the environment
    uv_snapshot!(context.filters(), context.sync().arg("--script").arg("script.py").env(EnvVars::VIRTUAL_ENV, "foo").arg("--active"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using script environment at: foo
    Resolved 3 packages in [TIME]
    Audited 3 packages in [TIME]
    ");

    // Requesting another Python version will invalidate the environment
    uv_snapshot!(context.filters(), context.sync()
        .arg("--script").arg("script.py")
        .arg("--output-format").arg("json")
        .env(EnvVars::VIRTUAL_ENV, "foo")
        .arg("--active")
        .arg("-p")
        .arg("3.12"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "schema": {
        "version": "preview"
      },
      "target": "script",
      "script": {
        "path": "[TEMP_DIR]/script.py"
      },
      "sync": {
        "environment": {
          "path": "[TEMP_DIR]/foo",
          "python": {
            "path": "[TEMP_DIR]/foo/[BIN]/[PYTHON]",
            "version": "3.12.[X]",
            "implementation": "cpython"
          }
        },
        "action": "update"
      },
      "lock": null,
      "dry_run": false
    }

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + sniffio==1.3.1
    "#);

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
    uv_snapshot!(context.filters(), context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    context
        .temp_dir
        .child(".venv")
        .assert(predicate::path::is_dir());

    // Similarly, `uv sync` from the child project uses `.venv` in the workspace root
    uv_snapshot!(context.filters(), context.sync().current_dir(context.temp_dir.join("child")), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Uninstalled 1 package in [TIME]
     - iniconfig==2.0.0
    ");

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
    uv_snapshot!(context.filters(), context.sync().env(EnvVars::UV_PROJECT_ENVIRONMENT, "foo"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: foo
    Resolved 3 packages in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

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
    uv_snapshot!(context.filters(), context.sync().env(EnvVars::UV_PROJECT_ENVIRONMENT, "foo").current_dir(context.temp_dir.join("child")), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Uninstalled 1 package in [TIME]
     - iniconfig==2.0.0
    ");

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
    uv_snapshot!(context.filters(), context.sync().arg("--package").arg("child").env(EnvVars::UV_PROJECT_ENVIRONMENT, "foo"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Audited in [TIME]
    ");

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
    uv_snapshot!(context.filters(), context.sync(), @r"
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
    ");

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
    uv_snapshot!(context.filters(), context.sync().env(EnvVars::VIRTUAL_ENV, context.temp_dir.join(".venv")), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    // Including if it's a relative path that matches
    uv_snapshot!(context.filters(), context.sync().env(EnvVars::VIRTUAL_ENV, ".venv"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Audited 1 package in [TIME]
    ");

    // Or, if it's a link that resolves to the same path
    #[cfg(unix)]
    {
        use fs_err::os::unix::fs::symlink;

        let link = context.temp_dir.join("link");
        symlink(context.temp_dir.join(".venv"), &link)?;

        uv_snapshot!(context.filters(), context.sync().env(EnvVars::VIRTUAL_ENV, link), @r"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Resolved 2 packages in [TIME]
        Audited 1 package in [TIME]
        ");
    }

    // But we should warn if it's a different path
    uv_snapshot!(context.filters(), context.sync().env(EnvVars::VIRTUAL_ENV, "foo"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `VIRTUAL_ENV=foo` does not match the project environment path `.venv` and will be ignored; use `--active` to target the active environment instead
    Resolved 2 packages in [TIME]
    Audited 1 package in [TIME]
    ");

    // Including absolute paths
    uv_snapshot!(context.filters(), context.sync().env(EnvVars::VIRTUAL_ENV, context.temp_dir.join("foo")), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `VIRTUAL_ENV=foo` does not match the project environment path `.venv` and will be ignored; use `--active` to target the active environment instead
    Resolved 2 packages in [TIME]
    Audited 1 package in [TIME]
    ");

    // We should not warn if the project environment has been customized and matches
    uv_snapshot!(context.filters(), context.sync().env(EnvVars::VIRTUAL_ENV, "foo").env(EnvVars::UV_PROJECT_ENVIRONMENT, "foo"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: foo
    Resolved 2 packages in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    // But we should warn if they don't match still
    uv_snapshot!(context.filters(), context.sync().env(EnvVars::VIRTUAL_ENV, "foo").env(EnvVars::UV_PROJECT_ENVIRONMENT, "bar"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `VIRTUAL_ENV=foo` does not match the project environment path `bar` and will be ignored; use `--active` to target the active environment instead
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: bar
    Resolved 2 packages in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    let child = context.temp_dir.child("child");
    child.create_dir_all()?;

    // And `VIRTUAL_ENV` is resolved relative to the project root so with relative paths we should
    // warn from a child too
    uv_snapshot!(context.filters(), context.sync().env(EnvVars::VIRTUAL_ENV, "foo").env(EnvVars::UV_PROJECT_ENVIRONMENT, "foo").current_dir(&child), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `VIRTUAL_ENV=foo` does not match the project environment path `[TEMP_DIR]/foo` and will be ignored; use `--active` to target the active environment instead
    Resolved 2 packages in [TIME]
    Audited 1 package in [TIME]
    ");

    // But, a matching absolute path shouldn't warn
    uv_snapshot!(context.filters(), context.sync().env(EnvVars::VIRTUAL_ENV, context.temp_dir.join("foo")).env(EnvVars::UV_PROJECT_ENVIRONMENT, "foo").current_dir(&child), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Audited 1 package in [TIME]
    ");

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

    uv_snapshot!(context.filters(), context.sync(), @r"
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
    ");

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

    uv_snapshot!(context.filters(), context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + my-project==0.2.0 (from file://[TEMP_DIR]/)
    ");

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
    uv_snapshot!(context.filters(), context.sync(), @r"
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
    ");

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

    uv_snapshot!(context.filters(), context.sync().arg("--no-binary-package").arg("iniconfig"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    assert!(context.temp_dir.child("uv.lock").exists());

    uv_snapshot!(context.filters(), context.sync().arg("--reinstall").arg("--no-binary"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     ~ iniconfig==2.0.0
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--reinstall").env("UV_NO_BINARY_PACKAGE", "iniconfig"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     ~ iniconfig==2.0.0
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--reinstall").env("UV_NO_BINARY", "1"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     ~ iniconfig==2.0.0
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--reinstall").env("UV_NO_BINARY", "iniconfig"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: invalid value 'iniconfig' for '--no-binary': value was not a boolean

    For more information, try '--help'.
    "###);

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

    uv_snapshot!(context.filters(), context.sync().arg("--no-binary-package").arg("odrive"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 31 packages in [TIME]
    error: Distribution `odrive==0.6.8 @ registry+https://pypi.org/simple` can't be installed because it is marked as `--no-binary` but has no source distribution
    ");

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

    uv_snapshot!(context.filters(), context.sync().arg("--no-build-package").arg("iniconfig"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    assert!(context.temp_dir.child("uv.lock").exists());

    uv_snapshot!(context.filters(), context.sync().arg("--reinstall").env("UV_NO_BUILD_PACKAGE", "iniconfig"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     ~ iniconfig==2.0.0
    ");

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

    uv_snapshot!(context.filters(), context.sync().arg("--no-build-package").arg("django-allauth"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 19 packages in [TIME]
    error: Distribution `django-allauth==0.51.0 @ registry+https://pypi.org/simple` can't be installed because it is marked as `--no-build` but has no binary distribution
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--no-build"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 19 packages in [TIME]
    error: Distribution `django-allauth==0.51.0 @ registry+https://pypi.org/simple` can't be installed because it is marked as `--no-build` but has no binary distribution
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--reinstall").env("UV_NO_BUILD", "1"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 19 packages in [TIME]
    error: Distribution `django-allauth==0.51.0 @ registry+https://pypi.org/simple` can't be installed because it is marked as `--no-build` but has no binary distribution
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--reinstall").env("UV_NO_BUILD_PACKAGE", "django-allauth"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 19 packages in [TIME]
    error: Distribution `django-allauth==0.51.0 @ registry+https://pypi.org/simple` can't be installed because it is marked as `--no-build` but has no binary distribution
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--reinstall").env("UV_NO_BUILD", "django-allauth"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: invalid value 'django-allauth' for '--no-build': value was not a boolean

    For more information, try '--help'.
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

    uv_snapshot!(context.filters(), context.sync(), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    error: Distribution `cffi==1.17.1 @ direct+https://files.pythonhosted.org/packages/08/fd/cc2fedbd887223f9f5d170c96e57cbf655df9831a6546c1727ae13fa977a/cffi-1.17.1-cp310-cp310-macosx_11_0_arm64.whl` can't be installed because the binary distribution is incompatible with the current platform

    hint: You're using CPython 3.12 (`cp312`), but `cffi` (v1.17.1) only has wheels with the following Python ABI tag: `cp310`
    ");

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

    uv_snapshot!(context.filters(), context.sync(), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    error: Distribution `cffi==1.17.1 @ path+cffi-1.17.1-cp310-cp310-macosx_11_0_arm64.whl` can't be installed because the binary distribution is incompatible with the current platform

    hint: You're using CPython 3.12 (`cp312`), but `cffi` (v1.17.1) only has wheels with the following Python ABI tag: `cp310`
    ");

    Ok(())
}

#[test]
fn sync_override_package() -> Result<()> {
    let context = TestContext::new("3.12");

    // Create a dependency.
    let pyproject_toml = context.temp_dir.child("core").child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "core"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"

        [tool.uv]
        package = false
        "#,
    )?;

    context
        .temp_dir
        .child("core")
        .child("src")
        .child("core")
        .child("__init__.py")
        .touch()?;

    // Create a package that depends on it.
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.0.0"
        requires-python = ">=3.12"
        dependencies = ["core"]

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"

        [tool.uv.sources]
        core = { path = "./core" }
        "#,
    )?;

    context
        .temp_dir
        .child("src")
        .child("project")
        .child("__init__.py")
        .touch()?;

    // Syncing the project should _not_ install `core`.
    uv_snapshot!(context.filters(), context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + project==0.0.0 (from file://[TEMP_DIR]/)
    ");

    // Mark the source as `package = true`.
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.0.0"
        requires-python = ">=3.12"
        dependencies = ["core"]

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"

        [tool.uv.sources]
        core = { path = "./core", package = true }
        "#,
    )?;

    // Syncing the project _should_ install `core`.
    uv_snapshot!(context.filters(), context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 2 packages in [TIME]
     + core==0.1.0 (from file://[TEMP_DIR]/core)
     ~ project==0.0.0 (from file://[TEMP_DIR]/)
    ");

    // Remove `package = false`.
    let pyproject_toml = context.temp_dir.child("core").child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "core"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
        "#,
    )?;

    // Syncing the project _should_ install `core`.
    uv_snapshot!(context.filters(), context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     ~ core==0.1.0 (from file://[TEMP_DIR]/core)
    ");

    // Mark the source as `package = false`.
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.0.0"
        requires-python = ">=3.12"
        dependencies = ["core"]

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"

        [tool.uv.sources]
        core = { path = "./core", package = false }
        "#,
    )?;

    // Syncing the project should _not_ install `core`.
    uv_snapshot!(context.filters(), context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 2 packages in [TIME]
    Installed 1 package in [TIME]
     - core==0.1.0 (from file://[TEMP_DIR]/core)
     ~ project==0.0.0 (from file://[TEMP_DIR]/)
    ");

    // Update the source `tool.uv` to `package = true`
    let pyproject_toml = context.temp_dir.child("core").child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "core"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"

        [tool.uv]
        package = true
        "#,
    )?;

    // Mark the source as `package = false`.
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.0.0"
        requires-python = ">=3.12"
        dependencies = ["core"]

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"

        [tool.uv.sources]
        core = { path = "./core", package = false }
        "#,
    )?;

    // Syncing the project should _not_ install `core`.
    uv_snapshot!(context.filters(), context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     ~ project==0.0.0 (from file://[TEMP_DIR]/)
    ");

    // Remove the `package = false` mark.
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.0.0"
        requires-python = ">=3.12"
        dependencies = ["core"]

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"

        [tool.uv.sources]
        core = { path = "./core" }
        "#,
    )?;

    // Syncing the project _should_ install `core`.
    uv_snapshot!(context.filters(), context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 2 packages in [TIME]
     + core==0.1.0 (from file://[TEMP_DIR]/core)
     ~ project==0.0.0 (from file://[TEMP_DIR]/)
    ");

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

    uv_snapshot!(context.filters(), context.sync().arg("--dev"), @r"
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
    ");

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

    uv_snapshot!(context.filters(), context.sync().arg("--no-editable"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + child==0.1.0 (from file://[TEMP_DIR]/child)
     + root==0.1.0 (from file://[TEMP_DIR]/)
    ");

    uv_snapshot!(context.filters(), context.sync().env(EnvVars::UV_NO_EDITABLE, "1"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Audited 2 packages in [TIME]
    ");

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

    uv_snapshot!(context.filters(), context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: Skipping installation of entry points (`project.scripts`) because this project is not packaged; to install entry points, set `tool.uv.package = true` or define a `build-system`
    Resolved 1 package in [TIME]
    Audited in [TIME]
    ");

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

    uv_snapshot!(context.filters(), context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: Skipping installation of entry points (`project.scripts`) because this project is not packaged; to install entry points, set `tool.uv.package = true` or define a `build-system`
    Resolved 1 package in [TIME]
    Audited in [TIME]
    ");

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

    uv_snapshot!(context.filters(), context.sync().arg("--extra").arg("dev"), @r"
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
    ");

    let lock = context.read("uv.lock");

    insta::with_settings!(
        {
            filters => context.filters(),
        },
        {
            assert_snapshot!(
                lock, @r#"
            version = 1
            revision = 3
            requires-python = ">=3.12"

            [options]
            exclude-newer = "2024-03-25T00:00:00Z"

            [[package]]
            name = "iniconfig"
            version = "2.0.0"
            source = { registry = "https://pypi.org/simple" }
            sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646, upload-time = "2023-01-07T11:08:11.254Z" }
            wheels = [
                { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892, upload-time = "2023-01-07T11:08:09.864Z" },
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
            provides-extras = ["dev"]

            [[package]]
            name = "typing-extensions"
            version = "4.10.0"
            source = { registry = "https://pypi.org/simple" }
            sdist = { url = "https://files.pythonhosted.org/packages/16/3a/0d26ce356c7465a19c9ea8814b960f8a36c3b0d07c323176620b7b483e44/typing_extensions-4.10.0.tar.gz", hash = "sha256:b0abd7c89e8fb96f98db18d86106ff1d90ab692004eb746cf6eda2682f91b3cb", size = 77558, upload-time = "2024-02-25T22:12:49.693Z" }
            wheels = [
                { url = "https://files.pythonhosted.org/packages/f9/de/dc04a3ea60b22624b51c703a84bbe0184abcd1d0b9bc8074b5d6b7ab90bb/typing_extensions-4.10.0-py3-none-any.whl", hash = "sha256:69b1a937c3a517342112fb4c6df7e72fc39a38e7891a5730ed4985b5214b5475", size = 33926, upload-time = "2024-02-25T22:12:47.72Z" },
            ]
            "#
            );
        }
    );

    // Check that we can re-read the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--locked"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Uninstalled 1 package in [TIME]
     - typing-extensions==4.10.0
    ");

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

    uv_snapshot!(context.filters(), context.sync().current_dir(context.temp_dir.child("project")), @r"
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
    ");

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

    uv_snapshot!(context.filters(), context.sync().current_dir(context.temp_dir.child("project")), @r"
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
    ");

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
    uv_snapshot!(context.filters(), context.venv().arg("--python").arg("3.11"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.11.[X] interpreter at: [PYTHON-3.11]
    warning: The requested interpreter resolved to Python 3.11.[X], which is incompatible with the project's Python requirement: `>=3.12` (from `project.requires-python`)
    Creating virtual environment at: .venv
    Activate with: source .venv/[BIN]/activate
    ");

    // Even with some extraneous content...
    fs_err::write(context.temp_dir.join(".venv").join("file"), b"")?;

    // We can delete and use it
    uv_snapshot!(context.filters(), context.sync(), @r"
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
    ");

    let bin = venv_bin_path(context.temp_dir.join(".venv"));

    // If there's just a broken symlink, we should warn
    #[cfg(unix)]
    {
        fs_err::remove_file(bin.join("python"))?;
        fs_err::os::unix::fs::symlink(context.temp_dir.join("does-not-exist"), bin.join("python"))?;
        uv_snapshot!(context.filters(), context.sync(), @r"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        warning: Ignoring existing virtual environment linked to non-existent Python interpreter: .venv/[BIN]/[PYTHON] -> python
        Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
        Removed virtual environment at: .venv
        Creating virtual environment at: .venv
        Resolved 2 packages in [TIME]
        Installed 1 package in [TIME]
         + iniconfig==2.0.0
        ");
    }

    // If the Python executable is missing entirely, we'll delete and use it
    fs_err::remove_dir_all(&bin)?;
    uv_snapshot!(context.filters(), context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Removed virtual environment at: .venv
    Creating virtual environment at: .venv
    Resolved 2 packages in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    // But if it's not a virtual environment...
    fs_err::remove_dir_all(context.temp_dir.join(".venv"))?;
    uv_snapshot!(context.filters(), context.venv().arg("--python").arg("3.11"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.11.[X] interpreter at: [PYTHON-3.11]
    warning: The requested interpreter resolved to Python 3.11.[X], which is incompatible with the project's Python requirement: `>=3.12` (from `project.requires-python`)
    Creating virtual environment at: .venv
    Activate with: source .venv/[BIN]/activate
    ");

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

    // Even if there's no Python executable
    fs_err::remove_dir_all(&bin)?;
    uv_snapshot!(context.filters(), context.sync(), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Project virtual environment directory `[VENV]/` cannot be used because it is not a valid Python environment (no Python executable was found)
    ");

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

#[cfg(unix)]
#[test]
fn sync_partial_environment_delete() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let context = TestContext::new_with_versions(&["3.13", "3.12"]);

    context.init().arg("-p").arg("3.12").assert().success();
    uv_snapshot!(context.filters(), context.sync().arg("-p").arg("3.13"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.13.[X] interpreter at: [PYTHON-3.13]
    Creating virtual environment at: .venv
    Resolved 1 package in [TIME]
    Audited in [TIME]
    ");

    // Create a directory that's unreadable, erroring on trying to delete its children.
    // This relies on our implementation listing directory entries before deleting them — which is a
    // bit of a hack but accomplishes the goal here.
    let unreadable2 = context.temp_dir.child(".venv/z2.txt");
    fs_err::create_dir(&unreadable2)?;
    let perms = std::fs::Permissions::from_mode(0o000);
    fs_err::set_permissions(&unreadable2, perms)?;

    uv_snapshot!(context.filters(), context.sync().arg("-p").arg("3.12"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    error: failed to remove directory `[VENV]/z2.txt`: Permission denied (os error 13)
    ");

    uv_snapshot!(context.filters(), context.sync().arg("-p").arg("3.12"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    error: failed to remove directory `[VENV]/z2.txt`: Permission denied (os error 13)
    ");

    // Remove the unreadable directory
    fs_err::remove_dir(unreadable2)?;

    // We should be able to remove the venv now
    uv_snapshot!(context.filters(), context.sync().arg("-p").arg("3.12"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Removed virtual environment at: .venv
    Creating virtual environment at: .venv
    Resolved 1 package in [TIME]
    Audited in [TIME]
    ");

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

    uv_snapshot!(context.filters(), context.sync().arg("--no-sources"), @r"
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
    ");

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
    uv_snapshot!(context.filters(), context.sync(), @r"
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
    ");

    // Unless explicitly requested...
    uv_snapshot!(context.filters(), context.sync().arg("--python").arg("3.10"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.10.[X] interpreter at: [PYTHON-3.10]
    error: The requested interpreter resolved to Python 3.10.[X], which is incompatible with the project's Python requirement: `>=3.11` (from `project.requires-python`)
    ");

    // But a pin should take precedence
    uv_snapshot!(context.filters(), context.python_pin().arg("3.12"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Pinned `.python-version` to `3.12`

    ----- stderr -----
    "###);

    uv_snapshot!(context.filters(), context.sync(), @r"
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
    ");

    // Create a pin that's incompatible with the project
    uv_snapshot!(context.filters(), context.python_pin().arg("3.10").arg("--no-workspace"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Updated `.python-version` from `3.12` -> `3.10`

    ----- stderr -----
    "###);

    // We should warn on subsequent uses, but respect the pinned version?
    uv_snapshot!(context.filters(), context.sync(), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.10.[X] interpreter at: [PYTHON-3.10]
    error: The Python request from `.python-version` resolved to Python 3.10.[X], which is incompatible with the project's Python requirement: `>=3.11` (from `project.requires-python`)
    Use `uv python pin` to update the `.python-version` file to a compatible version
    ");

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

    uv_snapshot!(context.filters(), context.sync().current_dir(&child_dir), @r"
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
    ");

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

    uv_snapshot!(context.filters(), context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + idna==2.7
    ");

    // Clear the environment.
    fs_err::remove_dir_all(&context.venv)?;

    // The package should be drawn from the cache.
    uv_snapshot!(context.filters(), context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: .venv
    Resolved 2 packages in [TIME]
    Installed 1 package in [TIME]
     + idna==2.7
    ");

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
    uv_snapshot!(context.filters(), context.sync().arg("--all-packages"), @r"
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
    ");

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

    // Sync an extra that exists in both the parent and child.
    uv_snapshot!(context.filters(), context.sync().arg("--all-packages").arg("--extra").arg("types"), @r"
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
    ");

    // Sync an extra that only exists in the child.
    uv_snapshot!(context.filters(), context.sync().arg("--all-packages").arg("--extra").arg("testing"), @r"
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
    ");

    // Sync all extras.
    uv_snapshot!(context.filters(), context.sync().arg("--all-packages").arg("--all-extras"), @r"
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
    ");

    // Sync all extras excluding an extra that exists in both the parent and child.
    uv_snapshot!(context.filters(), context.sync().arg("--all-packages").arg("--all-extras").arg("--no-extra").arg("types"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 8 packages in [TIME]
    Uninstalled 1 package in [TIME]
     - typing-extensions==4.10.0
    ");

    // Sync an extra that doesn't exist.
    uv_snapshot!(context.filters(), context.sync().arg("--all-packages").arg("--extra").arg("foo"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 8 packages in [TIME]
    error: Extra `foo` is not defined in any project's `optional-dependencies` table
    ");

    // Sync all extras excluding an extra that doesn't exist.
    uv_snapshot!(context.filters(), context.sync().arg("--all-packages").arg("--all-extras").arg("--no-extra").arg("foo"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 8 packages in [TIME]
    error: Extra `foo` is not defined in any project's `optional-dependencies` table
    ");

    Ok(())
}

/// Sync all members in a workspace with dynamic extras.
#[test]
fn sync_all_extras_dynamic() -> Result<()> {
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

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"

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
        dynamic = ["optional-dependencies"]

        [tool.setuptools.dynamic.optional-dependencies]
        dev = { file = "requirements-dev.txt" }

        [tool.uv]
        cache-keys = ["pyproject.toml"]

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

    child
        .child("requirements-dev.txt")
        .write_str("typing-extensions==4.10.0")?;

    // Generate a lockfile.
    context.lock().assert().success();

    // Sync an extra that exists in the parent.
    uv_snapshot!(context.filters(), context.sync().arg("--all-packages").arg("--extra").arg("types"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + child==0.1.0 (from file://[TEMP_DIR]/child)
     + project==0.1.0 (from file://[TEMP_DIR]/)
     + sniffio==1.3.1
    ");

    // Sync a dynamic extra that exists in the child.
    uv_snapshot!(context.filters(), context.sync().arg("--all-packages").arg("--extra").arg("dev"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - sniffio==1.3.1
     + typing-extensions==4.10.0
    ");

    // Sync a dynamic extra that doesn't exist in the child.
    uv_snapshot!(context.filters(), context.sync().arg("--all-packages").arg("--extra").arg("foo"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    error: Extra `foo` is not defined in any project's `optional-dependencies` table
    ");

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
        empty = []

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

    // Sync a group that exists in both the parent and child.
    uv_snapshot!(context.filters(), context.sync().arg("--all-packages").arg("--group").arg("types"), @r"
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
    ");

    // Sync a group that only exists in the child.
    uv_snapshot!(context.filters(), context.sync().arg("--all-packages").arg("--group").arg("testing"), @r"
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
    ");

    // Sync a group that doesn't exist.
    uv_snapshot!(context.filters(), context.sync().arg("--all-packages").arg("--group").arg("foo"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 8 packages in [TIME]
    error: Group `foo` is not defined in any project's `dependency-groups` table
    ");

    // Sync an empty group.
    uv_snapshot!(context.filters(), context.sync().arg("--group").arg("empty"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 8 packages in [TIME]
    Uninstalled 1 package in [TIME]
     - packaging==24.0
    ");

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

    uv_snapshot!(context.filters(), context.sync().arg("--extra").arg("cu124"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + jinja2==3.1.3
     + markupsafe==2.1.5
    ");

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
        requires-dist = []
        "#,
    )?;

    let filters = context
        .filters()
        .into_iter()
        .chain([(r"/.*/src", "/[TMP]/src")])
        .collect::<Vec<_>>();

    uv_snapshot!(filters, context.sync(), @r#"
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
    "#);

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
        requires-dist = []
        "#,
    )?;

    let filters = context
        .filters()
        .into_iter()
        .chain([(r"/.*/src", "/[TMP]/src")])
        .collect::<Vec<_>>();

    uv_snapshot!(filters, context.sync().arg("--extra").arg("wsgi"), @r#"
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
    "#);

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
        requires-dist = []
        "#,
    )?;

    let filters = context
        .filters()
        .into_iter()
        .chain([(r"/.*/src", "/[TMP]/src")])
        .collect::<Vec<_>>();

    uv_snapshot!(filters, context.sync().arg("--group").arg("wsgi"), @r#"
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
    "#);

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
                lock, @r#"
            version = 1
            revision = 3
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
            sdist = { url = "https://files.pythonhosted.org/packages/4d/5b/dc575711b6b8f2f866131a40d053e30e962e633b332acf7cd2c24843d83d/setuptools-69.2.0.tar.gz", hash = "sha256:0ff4183f8f42cd8fa3acea16c45205521a4ef28f73c6391d8a25e92893134f2e", size = 2222950, upload-time = "2024-03-13T11:20:59.219Z" }
            wheels = [
                { url = "https://files.pythonhosted.org/packages/92/e1/1c8bb3420105e70bdf357d57dd5567202b4ef8d27f810e98bb962d950834/setuptools-69.2.0-py3-none-any.whl", hash = "sha256:c21c49fb1042386df081cb5d86759792ab89efca84cf114889191cd09aacc80c", size = 821485, upload-time = "2024-03-13T11:20:54.103Z" },
            ]
            "#
            );
        }
    );

    uv_snapshot!(context.filters(), context.sync(), @r"
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
    ");

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
                lock, @r#"
            version = 1
            revision = 3
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
            "#
            );
        }
    );

    uv_snapshot!(context.filters(), context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + uv-git-workspace-in-root==0.1.0 (from git+https://github.com/astral-sh/workspace-in-root-test.git@d3ab48d2338296d47e28dbb2fb327c5e2ac4ac68)
     + workspace-member-in-subdir==0.1.0 (from git+https://github.com/astral-sh/workspace-in-root-test.git@d3ab48d2338296d47e28dbb2fb327c5e2ac4ac68#subdirectory=workspace-member-in-subdir)
    ");

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
    Resolved 5 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!(
        {
            filters => context.filters(),
        },
        {
            assert_snapshot!(
                lock, @r#"
            version = 1
            revision = 3
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
            sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646, upload-time = "2023-01-07T11:08:11.254Z" }
            wheels = [
                { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892, upload-time = "2023-01-07T11:08:09.864Z" },
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
            sdist = { url = "https://files.pythonhosted.org/packages/16/3a/0d26ce356c7465a19c9ea8814b960f8a36c3b0d07c323176620b7b483e44/typing_extensions-4.10.0.tar.gz", hash = "sha256:b0abd7c89e8fb96f98db18d86106ff1d90ab692004eb746cf6eda2682f91b3cb", size = 77558, upload-time = "2024-02-25T22:12:49.693Z" }
            wheels = [
                { url = "https://files.pythonhosted.org/packages/f9/de/dc04a3ea60b22624b51c703a84bbe0184abcd1d0b9bc8074b5d6b7ab90bb/typing_extensions-4.10.0-py3-none-any.whl", hash = "sha256:69b1a937c3a517342112fb4c6df7e72fc39a38e7891a5730ed4985b5214b5475", size = 33926, upload-time = "2024-02-25T22:12:47.72Z" },
            ]
            "#
            );
        }
    );

    uv_snapshot!(context.filters(), context.sync(), @r"
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
    ");

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
                lock, @r#"
            version = 1
            revision = 3
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
            "#
            );
        }
    );

    uv_snapshot!(context.filters(), context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + dependency==0.1.0 (from git+https://github.com/astral-sh/uv-backwards-path-test@4bcc7fcd2e548c2ab7ba6b97b1c4e3ababccc7a9#subdirectory=dependency)
     + package==0.1.0 (from git+https://github.com/astral-sh/uv-backwards-path-test@4bcc7fcd2e548c2ab7ba6b97b1c4e3ababccc7a9#subdirectory=root)
    ");

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

    uv_snapshot!(context.filters(), context.sync(), @r"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
      × Failed to build `foo @ file://[TEMP_DIR]/`
      ╰─▶ Package metadata name `project` does not match given name `foo`
      help: `foo` was included because `project` (v0.1.0) depends on `foo`
    ");

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

    uv_snapshot!(context.filters(), context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0 (from https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz)
    ");

    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["foo @ https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.sync(), @r"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to download and build `foo @ https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz`
      ╰─▶ Package metadata name `iniconfig` does not match given name `foo`
    ");

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
                lock, @r#"
            version = 1
            revision = 3
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
            "#
            );
        }
    );

    uv_snapshot!(context.filters(), context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + package1==0.1.0 (from git+https://github.com/astral-sh/uv-path-dependency-test.git@28781b32cf1f260cdb2c8040628079eb265202bd#subdirectory=package1)
     + package2==0.1.0 (from git+https://github.com/astral-sh/uv-path-dependency-test.git@28781b32cf1f260cdb2c8040628079eb265202bd#subdirectory=package2)
    ");

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
            lock, @r#"
        version = 1
        revision = 3
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
        "#
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
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + build-tag==1.0.0
    ");

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
    uv_snapshot!(context.filters(), context.sync(), @r"
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
    ");

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
    uv_snapshot!(context.filters(), context.sync(), @r"
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
    ");

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
    uv_snapshot!(context.filters(), context.sync().current_dir(&subdir).arg("--offline"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + ok==1.0.0
    ");

    Ok(())
}

#[test]
fn sync_dry_run() -> Result<()> {
    let context = TestContext::new_with_versions(&["3.9", "3.12"]);

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

    // Perform a `--dry-run`.
    uv_snapshot!(context.filters(), context.sync().arg("--dry-run"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Would create project environment at: .venv
    Resolved 2 packages in [TIME]
    Would create lockfile at: uv.lock
    Would download 1 package
    Would install 1 package
     + iniconfig==2.0.0
    ");

    // Perform a full sync.
    uv_snapshot!(context.filters(), context.sync(), @r"
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
    ");

    // Update the requirements.
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["typing-extensions"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.sync().arg("--dry-run"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Would use project environment at: .venv
    Resolved 2 packages in [TIME]
    Would update lockfile at: uv.lock
    Would download 1 package
    Would uninstall 1 package
    Would install 1 package
     - iniconfig==2.0.0
     + typing-extensions==4.10.0
    ");

    // Update the `requires-python`.
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = "==3.9.*"
        dependencies = ["iniconfig"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.sync().arg("--dry-run"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.9.[X] interpreter at: [PYTHON-3.9]
    Would replace project environment at: .venv
    warning: Resolving despite existing lockfile due to fork markers being disjoint with `requires-python`: `python_full_version >= '3.12'` vs `python_full_version == '3.9.*'`
    Resolved 2 packages in [TIME]
    Would update lockfile at: uv.lock
    Would install 1 package
     + iniconfig==2.0.0
    ");

    // Perform a full sync.
    uv_snapshot!(context.filters(), context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.9.[X] interpreter at: [PYTHON-3.9]
    Removed virtual environment at: .venv
    Creating virtual environment at: .venv
    warning: Resolving despite existing lockfile due to fork markers being disjoint with `requires-python`: `python_full_version >= '3.12'` vs `python_full_version == '3.9.*'`
    Resolved 2 packages in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    // TMP: Attempt to catch this flake with verbose output
    // See https://github.com/astral-sh/uv/issues/13744
    let output = context.sync().arg("--dry-run").arg("-vv").output()?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("Would replace existing virtual environment"),
        "{}",
        stderr
    );

    uv_snapshot!(context.filters(), context.sync().arg("--dry-run"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Would use project environment at: .venv
    Resolved 2 packages in [TIME]
    Found up-to-date lockfile at: uv.lock
    Audited 1 package in [TIME]
    Would make no changes
    ");

    Ok(())
}

#[test]
fn sync_dry_run_and_locked() -> Result<()> {
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

    // Running with `--locked` and `--dry-run` should error.
    uv_snapshot!(context.filters(), context.sync().arg("--locked").arg("--dry-run"), @r"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Would use project environment at: .venv
    Resolved 2 packages in [TIME]
    Would download 1 package
    Would install 1 package
     + iniconfig==2.0.0
    The lockfile at `uv.lock` needs to be updated, but `--locked` was provided. To update the lockfile, run `uv lock`.
    ");

    let updated = context.read("uv.lock");

    // And the lockfile should be unchanged.
    assert_eq!(existing, updated);

    Ok(())
}

#[test]
fn sync_dry_run_and_frozen() -> Result<()> {
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

    // Lock the initial requirements.
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

    // Running with `--frozen` with `--dry-run` should preview dependencies to be installed.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen").arg("--dry-run"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Would use project environment at: .venv
    Would download 3 packages
    Would install 3 packages
     + anyio==3.7.0
     + idna==3.6
     + sniffio==1.3.1
    ");

    Ok(())
}

#[test]
fn sync_script() -> Result<()> {
    let context = TestContext::new_with_versions(&["3.9", "3.12"]);

    let script = context.temp_dir.child("script.py");
    script.write_str(indoc! { r#"
        # /// script
        # requires-python = ">=3.11"
        # dependencies = [
        #   "anyio",
        # ]
        # ///

        import anyio
       "#
    })?;

    uv_snapshot!(context.filters(), context.sync().arg("--script").arg("script.py"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Creating script environment at: [CACHE_DIR]/environments-v2/script-[HASH]
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + sniffio==1.3.1
    ");

    // If a lockfile didn't exist already, `uv sync --script` shouldn't create one.
    assert!(!context.temp_dir.child("uv.lock").exists());

    // Modify the script's dependencies.
    script.write_str(indoc! { r#"
        # /// script
        # requires-python = ">=3.11"
        # dependencies = [
        #   "anyio",
        #   "iniconfig",
        # ]
        # ///

        import anyio
       "#
    })?;

    uv_snapshot!(context.filters(), context.sync().arg("--script").arg("script.py"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using script environment at: [CACHE_DIR]/environments-v2/script-[HASH]
    Resolved 4 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    // Remove a dependency.
    script.write_str(indoc! { r#"
        # /// script
        # requires-python = ">=3.11"
        # dependencies = [
        #   "anyio",
        # ]
        # ///

        import anyio
       "#
    })?;

    uv_snapshot!(context.filters(), context.sync().arg("--script").arg("script.py"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using script environment at: [CACHE_DIR]/environments-v2/script-[HASH]
    Resolved 3 packages in [TIME]
    Uninstalled 1 package in [TIME]
     - iniconfig==2.0.0
    ");

    // Modify the `requires-python`.
    script.write_str(indoc! { r#"
        # /// script
        # requires-python = ">=3.8, <3.11"
        # dependencies = [
        #   "anyio",
        # ]
        # ///

        import anyio
       "#
    })?;

    uv_snapshot!(context.filters(), context.sync().arg("--script").arg("script.py"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Updating script environment at: [CACHE_DIR]/environments-v2/script-[HASH]
    Resolved 5 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 5 packages in [TIME]
     + anyio==4.3.0
     + exceptiongroup==1.2.0
     + idna==3.6
     + sniffio==1.3.1
     + typing-extensions==4.10.0
    ");

    // `--locked` and `--frozen` should fail with helpful error messages.
    uv_snapshot!(context.filters(), context.sync().arg("--script").arg("script.py").arg("--locked"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Using script environment at: [CACHE_DIR]/environments-v2/script-[HASH]
    error: `uv sync --locked` requires a script lockfile; run `uv lock --script script.py` to lock the script
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--script").arg("script.py").arg("--frozen"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Using script environment at: [CACHE_DIR]/environments-v2/script-[HASH]
    error: `uv sync --frozen` requires a script lockfile; run `uv lock --script script.py` to lock the script
    ");

    Ok(())
}

#[test]
fn sync_locked_script() -> Result<()> {
    let context = TestContext::new_with_versions(&["3.9", "3.12"]);

    let script = context.temp_dir.child("script.py");
    script.write_str(indoc! { r#"
        # /// script
        # requires-python = ">=3.11"
        # dependencies = [
        #   "anyio",
        # ]
        # ///

        import anyio
       "#
    })?;

    // Lock the script.
    uv_snapshot!(context.filters(), context.lock().arg("--script").arg("script.py"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###);

    let lock = context.read("script.py.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.11"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        requirements = [{ name = "anyio" }]

        [[package]]
        name = "anyio"
        version = "4.3.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "idna" },
            { name = "sniffio" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz", hash = "sha256:f75253795a87df48568485fd18cdd2a3fa5c4f7c5be8e5e36637733fce06fed6", size = 159642, upload-time = "2024-02-19T08:36:28.641Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl", hash = "sha256:048e05d0f6caeed70d731f3db756d35dcc1f35747c8c403364a8332c630441b8", size = 85584, upload-time = "2024-02-19T08:36:26.842Z" },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426, upload-time = "2023-11-25T15:40:54.902Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567, upload-time = "2023-11-25T15:40:52.604Z" },
        ]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372, upload-time = "2024-02-25T23:20:04.057Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235, upload-time = "2024-02-25T23:20:01.196Z" },
        ]
        "#
        );
    });

    uv_snapshot!(context.filters(), context.sync().arg("--script").arg("script.py"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Creating script environment at: [CACHE_DIR]/environments-v2/script-[HASH]
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + sniffio==1.3.1
    ");

    // Modify the script's dependencies.
    script.write_str(indoc! { r#"
        # /// script
        # requires-python = ">=3.11"
        # dependencies = [
        #   "anyio",
        #   "iniconfig",
        # ]
        # ///

        import anyio
       "#
    })?;

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.sync().arg("--script").arg("script.py").arg("--locked"), @r"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Using script environment at: [CACHE_DIR]/environments-v2/script-[HASH]
    Resolved 4 packages in [TIME]
    The lockfile at `uv.lock` needs to be updated, but `--locked` was provided. To update the lockfile, run `uv lock`.
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--script").arg("script.py"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using script environment at: [CACHE_DIR]/environments-v2/script-[HASH]
    Resolved 4 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    let lock = context.read("script.py.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.11"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        requirements = [
            { name = "anyio" },
            { name = "iniconfig" },
        ]

        [[package]]
        name = "anyio"
        version = "4.3.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "idna" },
            { name = "sniffio" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz", hash = "sha256:f75253795a87df48568485fd18cdd2a3fa5c4f7c5be8e5e36637733fce06fed6", size = 159642, upload-time = "2024-02-19T08:36:28.641Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl", hash = "sha256:048e05d0f6caeed70d731f3db756d35dcc1f35747c8c403364a8332c630441b8", size = 85584, upload-time = "2024-02-19T08:36:26.842Z" },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426, upload-time = "2023-11-25T15:40:54.902Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567, upload-time = "2023-11-25T15:40:52.604Z" },
        ]

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646, upload-time = "2023-01-07T11:08:11.254Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892, upload-time = "2023-01-07T11:08:09.864Z" },
        ]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372, upload-time = "2024-02-25T23:20:04.057Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235, upload-time = "2024-02-25T23:20:01.196Z" },
        ]
        "#
        );
    });

    // Modify the `requires-python`.
    script.write_str(indoc! { r#"
        # /// script
        # requires-python = ">=3.8, <3.11"
        # dependencies = [
        #   "anyio",
        #   "iniconfig",
        # ]
        # ///

        import anyio
       "#
    })?;

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.sync().arg("--script").arg("script.py").arg("--locked"), @r"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Updating script environment at: [CACHE_DIR]/environments-v2/script-[HASH]
    warning: Resolving despite existing lockfile due to fork markers being disjoint with `requires-python`: `python_full_version >= '3.11'` vs `python_full_version >= '3.8' and python_full_version < '3.11'`
    Resolved 6 packages in [TIME]
    The lockfile at `uv.lock` needs to be updated, but `--locked` was provided. To update the lockfile, run `uv lock`.
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--script").arg("script.py"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using script environment at: [CACHE_DIR]/environments-v2/script-[HASH]
    warning: Resolving despite existing lockfile due to fork markers being disjoint with `requires-python`: `python_full_version >= '3.11'` vs `python_full_version >= '3.8' and python_full_version < '3.11'`
    Resolved 6 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 6 packages in [TIME]
     + anyio==4.3.0
     + exceptiongroup==1.2.0
     + idna==3.6
     + iniconfig==2.0.0
     + sniffio==1.3.1
     + typing-extensions==4.10.0
    ");

    Ok(())
}

#[test]
fn sync_script_with_compatible_build_constraints() -> Result<()> {
    let context = TestContext::new("3.9");

    let test_script = context.temp_dir.child("script.py");

    // Compatible build constraints.
    test_script.write_str(indoc! { r#"
        # /// script
        # requires-python = ">=3.9"
        # dependencies = [
        #   "anyio>=3",
        #   "requests==1.2"
        # ]
        #
        # [tool.uv]
        # build-constraint-dependencies = ["setuptools>=40"]
        # ///

        import anyio
       "#
    })?;

    uv_snapshot!(context.filters(), context.sync().arg("--script").arg("script.py"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Creating script environment at: [CACHE_DIR]/environments-v2/script-[HASH]
    Resolved 6 packages in [TIME]
    Prepared 6 packages in [TIME]
    Installed 6 packages in [TIME]
     + anyio==4.3.0
     + exceptiongroup==1.2.0
     + idna==3.6
     + requests==1.2.0
     + sniffio==1.3.1
     + typing-extensions==4.10.0
    ");

    Ok(())
}

#[test]
fn sync_script_with_incompatible_build_constraints() -> Result<()> {
    let context = TestContext::new("3.9");

    let test_script = context.temp_dir.child("script.py");

    // Incompatible build constraints.
    test_script.write_str(indoc! { r#"
        # /// script
        # requires-python = ">=3.9"
        # dependencies = [
        #   "anyio>=3",
        #   "requests==1.2"
        # ]
        #
        # [tool.uv]
        # build-constraint-dependencies = ["setuptools==1"]
        # ///

        import anyio
       "#
    })?;

    uv_snapshot!(context.filters(), context.sync().arg("--script").arg("script.py"), @r"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Creating script environment at: [CACHE_DIR]/environments-v2/script-[HASH]
      × Failed to download and build `requests==1.2.0`
      ├─▶ Failed to resolve requirements from `setup.py` build
      ├─▶ No solution found when resolving: `setuptools>=40.8.0`
      ╰─▶ Because you require setuptools>=40.8.0 and setuptools==1, we can conclude that your requirements are unsatisfiable.
    ");

    Ok(())
}

#[test]
fn unsupported_git_scheme() -> Result<()> {
    let context = TestContext::new_with_versions(&["3.12"]);

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "foo"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["foo"]

        [tool.uv.sources]
        # `c:/...` looks like an absolute path, but this field requires a URL such as `file:///...`.
        foo = { git = "c:/home/ferris/projects/foo", rev = "7701ffcbae245819b828dc5f885a5201158897ef" }
        "#},
    )?;

    uv_snapshot!(context.filters(), context.sync(), @r"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: .venv
      × Failed to build `foo @ file://[TEMP_DIR]/`
      ├─▶ Failed to parse entry: `foo`
      ╰─▶ Unsupported Git URL scheme `c:` in `c:/home/ferris/projects/foo` (expected one of `https:`, `ssh:`, or `file:`)
    ");
    Ok(())
}

/// See: <https://github.com/astral-sh/uv/issues/11648>
#[test]
fn multiple_group_conflicts() -> Result<()> {
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
        foo = [
            "iniconfig>=2",
        ]
        bar = [
            "iniconfig<2",
        ]
        baz = [
            "iniconfig",
        ]

        [tool.uv]
        conflicts = [
          [
            { group = "foo" },
            { group = "bar" },
          ],
        ]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Audited in [TIME]
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--group").arg("baz"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--group").arg("foo").arg("--group").arg("baz"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Audited 1 package in [TIME]
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--group").arg("bar").arg("--group").arg("baz"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - iniconfig==2.0.0
     + iniconfig==1.1.1
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--group").arg("foo").arg("--group").arg("bar"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    error: Groups `bar` and `foo` are incompatible with the declared conflicts: {`project:bar`, `project:foo`}
    ");

    Ok(())
}

/// See: <https://github.com/astral-sh/uv/issues/11232>
#[test]
fn transitive_group_conflicts_shallow() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "example"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [dependency-groups]
        dev = [
            { include-group = "test" },
        ]
        test = ["anyio>4"]
        magic = ["anyio<4"]

        [tool.uv]
        conflicts = [
            [
                { group = "test" },
                { group = "magic" },
            ],
        ]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.sync(), @r"
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
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--group").arg("dev"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Audited 3 packages in [TIME]
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--group").arg("dev").arg("--group").arg("test"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Audited 3 packages in [TIME]
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--group").arg("test").arg("--group").arg("magic"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    error: Groups `magic` and `test` are incompatible with the declared conflicts: {`example:magic`, `example:test`}
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--group").arg("dev").arg("--group").arg("magic"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    error: Groups `dev` and `magic` are incompatible with the transitively inferred conflicts: {`example:dev`, `example:magic`}
    ");

    Ok(())
}

/// See: <https://github.com/astral-sh/uv/issues/11232>
#[test]
fn transitive_group_conflicts_deep() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "example"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [dependency-groups]
        dev = [
            { include-group = "intermediate" },
        ]
        intermediate = [
            { include-group = "test" },
            { include-group = "other" },
        ]
        test = ["iniconfig>=2"]
        magic = ["iniconfig<2", "anyio<4"]
        other = ["anyio>4"]

        [tool.uv]
        conflicts = [
            [
                { group = "test" },
                { group = "magic" },
            ],
            [
                { group = "other" },
                { group = "magic" },
            ],
        ]"#,
    )?;

    uv_snapshot!(context.filters(), context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + iniconfig==2.0.0
     + sniffio==1.3.1
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--group").arg("dev"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
    Audited 4 packages in [TIME]
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--group").arg("dev").arg("--group").arg("test"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
    Audited 4 packages in [TIME]
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--group").arg("dev").arg("--group").arg("magic"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
    error: Groups `dev` and `magic` are incompatible with the transitively inferred conflicts: {`example:dev`, `example:magic`}
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--no-dev").arg("--group").arg("intermediate").arg("--group").arg("magic"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
    error: Groups `intermediate` and `magic` are incompatible with the transitively inferred conflicts: {`example:intermediate`, `example:magic`}
    ");

    Ok(())
}

/// See: <https://github.com/astral-sh/uv/issues/11232>
#[test]
fn transitive_group_conflicts_siblings() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "example"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [dependency-groups]
        dev = [
            { include-group = "test" },
        ]
        dev2 = [
            { include-group = "magic" },
        ]
        test = ["anyio>4"]
        magic = ["anyio<4"]

        [tool.uv]
        conflicts = [
            [
                { group = "test" },
                { group = "magic" },
            ],
        ]"#,
    )?;

    uv_snapshot!(context.filters(), context.sync(), @r"
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
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--group").arg("dev"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Audited 3 packages in [TIME]
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--no-dev").arg("--group").arg("dev2"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - anyio==4.3.0
     + anyio==3.7.1
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--group").arg("dev2"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    error: Groups `dev` (enabled by default) and `dev2` are incompatible with the transitively inferred conflicts: {`example:dev`, `example:dev2`}
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--group").arg("dev").arg("--group").arg("dev2"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    error: Groups `dev` and `dev2` are incompatible with the transitively inferred conflicts: {`example:dev`, `example:dev2`}
    ");

    Ok(())
}

/// See: <https://github.com/astral-sh/uv/issues/11232>
#[test]
fn transitive_group_conflicts_cycle() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "example"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [dependency-groups]
        dev = [
            { include-group = "test" },
        ]
        test = [
            "anyio>4",
            { include-group = "dev" },
        ]
        magic = ["anyio<4"]

        [tool.uv]
        conflicts = [
            [
                { group = "test" },
                { group = "magic" },
            ],
        ]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.sync(), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Project `example` has malformed dependency groups
      Caused by: Detected a cycle in `dependency-groups`: `dev` -> `test` -> `dev`
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--group").arg("dev"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Project `example` has malformed dependency groups
      Caused by: Detected a cycle in `dependency-groups`: `dev` -> `test` -> `dev`
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--group").arg("dev").arg("--group").arg("test"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Project `example` has malformed dependency groups
      Caused by: Detected a cycle in `dependency-groups`: `dev` -> `test` -> `dev`
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--group").arg("test").arg("--group").arg("magic"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Project `example` has malformed dependency groups
      Caused by: Detected a cycle in `dependency-groups`: `dev` -> `test` -> `dev`
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--group").arg("dev").arg("--group").arg("magic"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Project `example` has malformed dependency groups
      Caused by: Detected a cycle in `dependency-groups`: `dev` -> `test` -> `dev`
    ");

    Ok(())
}

/// See: <https://github.com/astral-sh/uv/issues/11703>
#[test]
fn prune_cache_url_subdirectory() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "root",
        ]

        [tool.uv.sources]
        root = { url = "https://github.com/user-attachments/files/18216295/subdirectory-test.tar.gz", subdirectory = "packages/root" }
    "#})?;

    // Lock the project.
    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    "###);

    // Prune the cache.
    context.prune().arg("--ci").assert().success();

    // Install the project.
    uv_snapshot!(context.filters(), context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + root==0.0.1 (from https://github.com/user-attachments/files/18216295/subdirectory-test.tar.gz#subdirectory=packages/root)
     + sniffio==1.3.1
    ");

    Ok(())
}

/// Test that incoherence in the versions in a package entry of the lockfile versions is caught.
///
/// See <https://github.com/astral-sh/uv/issues/12164>
#[test]
fn locked_version_coherence() -> Result<()> {
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

    uv_snapshot!(context.filters(), context.lock(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646, upload-time = "2023-01-07T11:08:11.254Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892, upload-time = "2023-01-07T11:08:09.864Z" },
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
        "#);
    });

    // Write an inconsistent iniconfig entry
    context
        .temp_dir
        .child("uv.lock")
        .write_str(&lock.replace(r#"version = "2.0.0""#, r#"version = "1.0.0""#))?;

    // An inconsistent lockfile should fail with `--locked`
    uv_snapshot!(context.filters(), context.sync().arg("--locked"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to parse `uv.lock`
      Caused by: The entry for package `iniconfig` v1.0.0 has wheel `iniconfig-2.0.0-py3-none-any.whl` with inconsistent version: v2.0.0
    ");

    // Without `--locked`, we could fail or recreate the lockfile, currently, we fail.
    uv_snapshot!(context.filters(), context.lock(), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to parse `uv.lock`
      Caused by: The entry for package `iniconfig` v1.0.0 has wheel `iniconfig-2.0.0-py3-none-any.whl` with inconsistent version: v2.0.0
    ");

    Ok(())
}

/// `uv sync` should respect build constraints. In this case, `json-merge-patch` should _not_ fail
/// to build, despite the fact that `setuptools==78.0.1` is the most recent version and _does_ fail
/// to build that package.
///
/// See: <https://github.com/astral-sh/uv/issues/12434>
#[test]
fn sync_build_constraints() -> Result<()> {
    let context = TestContext::new("3.12").with_exclude_newer("2025-03-24T19:00:00Z");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["json-merge-patch"]

        [tool.uv]
        build-constraint-dependencies = ["setuptools<78"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.sync().arg("--no-binary-package").arg("json-merge-patch"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + json-merge-patch==0.2
    ");

    let lock = context.read("uv.lock");

    insta::with_settings!(
        {
            filters => context.filters(),
        },
        {
            assert_snapshot!(
                lock, @r#"
            version = 1
            revision = 3
            requires-python = ">=3.12"

            [options]
            exclude-newer = "2025-03-24T19:00:00Z"

            [manifest]
            build-constraints = [{ name = "setuptools", specifier = "<78" }]

            [[package]]
            name = "json-merge-patch"
            version = "0.2"
            source = { registry = "https://pypi.org/simple" }
            sdist = { url = "https://files.pythonhosted.org/packages/39/62/3b783faabac9a099877397d8f7a7cc862a03fbf9fb1b90d414ea7c6bb096/json-merge-patch-0.2.tar.gz", hash = "sha256:09898b6d427c08754e2a97c709cf2dfd7e28bd10c5683a538914975eab778d39", size = 3081, upload-time = "2017-11-09T11:38:15.773Z" }

            [[package]]
            name = "project"
            version = "0.1.0"
            source = { virtual = "." }
            dependencies = [
                { name = "json-merge-patch" },
            ]

            [package.metadata]
            requires-dist = [{ name = "json-merge-patch" }]
            "#
            );
        }
    );

    fs_err::remove_dir_all(&context.cache_dir)?;
    fs_err::remove_dir_all(&context.venv)?;

    // We should also be able to read from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--locked"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: .venv
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + json-merge-patch==0.2
    ");

    // Modify the build constraints.
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["json-merge-patch"]

        [tool.uv]
        build-constraint-dependencies = ["setuptools<77"]
        "#,
    )?;

    // This should fail, given that the build constraints have changed.
    uv_snapshot!(context.filters(), context.sync().arg("--locked"), @r"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    The lockfile at `uv.lock` needs to be updated, but `--locked` was provided. To update the lockfile, run `uv lock`.
    ");

    // Changing the build constraints should lead to a re-resolve.
    uv_snapshot!(context.filters(), context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Audited 1 package in [TIME]
    ");

    Ok(())
}

// Test that we recreate a virtual environment when `pyvenv.cfg` version
// is incompatible with the interpreter version.
#[test]
fn sync_when_virtual_environment_incompatible_with_interpreter() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.11"
        dependencies = []
        "#,
    )?;

    // Create a virtual environment at `.venv`.
    context
        .venv()
        .arg(context.venv.as_os_str())
        .arg("--clear")
        .arg("--python")
        .arg("3.12")
        .assert()
        .success();

    // Simulate an incompatible `pyvenv.cfg:version` value created
    // by the venv module.
    let pyvenv_cfg = context.venv.child("pyvenv.cfg");
    let contents = fs_err::read_to_string(&pyvenv_cfg)
        .unwrap()
        .lines()
        .map(|line| {
            if line.trim_start().starts_with("version") {
                "version = 3.11.0".to_string()
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    fs_err::write(&pyvenv_cfg, contents)?;

    // We should also be able to read from the lockfile.
    uv_snapshot!(context.filters(), context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Removed virtual environment at: .venv
    Creating virtual environment at: .venv
    Resolved 1 package in [TIME]
    Audited in [TIME]
    ");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        let contents = fs_err::read_to_string(&pyvenv_cfg).unwrap();
        let lines: Vec<&str> = contents.split('\n').collect();
        assert_snapshot!(lines[3], @"version_info = 3.12.[X]");
    });

    // Simulate an incompatible `pyvenv.cfg:version_info` value created
    // by uv or virtualenv.
    let pyvenv_cfg = context.venv.child("pyvenv.cfg");
    let contents = fs_err::read_to_string(&pyvenv_cfg)
        .unwrap()
        .lines()
        .map(|line| {
            if line.trim_start().starts_with("version") {
                "version_info = 3.11.0".to_string()
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    fs_err::write(&pyvenv_cfg, contents)?;

    // We should also be able to read from the lockfile.
    uv_snapshot!(context.filters(), context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Removed virtual environment at: .venv
    Creating virtual environment at: .venv
    Resolved 1 package in [TIME]
    Audited in [TIME]
    ");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        let contents = fs_err::read_to_string(&pyvenv_cfg).unwrap();
        let lines: Vec<&str> = contents.split('\n').collect();
        assert_snapshot!(lines[3], @"version_info = 3.12.[X]");
    });

    Ok(())
}

/// Ensure that existing `uv.lock` files can use `upload_time` or `upload-time` interchangeably.
#[test]
fn sync_upload_time() -> Result<()> {
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

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "###);

    let uv_lock = context.temp_dir.child("uv.lock");
    uv_lock.write_str(r#"
        version = 1
        revision = 2
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "anyio"
        version = "3.7.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "idna" },
            { name = "sniffio" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/c6/b3/fefbf7e78ab3b805dec67d698dc18dd505af7a18a8dd08868c9b4fa736b5/anyio-3.7.0.tar.gz", hash = "sha256:275d9973793619a5374e1c89a4f4ad3f4b0a5510a2b5b939444bee8f4c4d37ce", size = 142737, upload_time = "2023-05-27T11:12:46.688Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/68/fe/7ce1926952c8a403b35029e194555558514b365ad77d75125f521a2bec62/anyio-3.7.0-py3-none-any.whl", hash = "sha256:eddca883c4175f14df8aedce21054bfca3adb70ffe76a9f607aef9d7fa2ea7f0", size = 80873, upload_time = "2023-05-27T11:12:44.474Z" },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426, upload_time = "2023-11-25T15:40:54.902Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567, upload_time = "2023-11-25T15:40:52.604Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "anyio" },
        ]

        [package.metadata]
        requires-dist = [{ name = "anyio", specifier = "==3.7.0" }]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372, upload_time = "2024-02-25T23:20:04.057Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235, upload_time = "2024-02-25T23:20:01.196Z" },
        ]
    "#)?;

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==3.7.0
     + idna==3.6
     + sniffio==1.3.1
    ");

    // Re-install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 3 packages in [TIME]
    ");

    Ok(())
}

/// Ensure that workspace members that are also development dependencies are not duplicated with
/// `--all-packages`.
///
/// See: <https://github.com/astral-sh/uv/issues/13673#issuecomment-2912196406>
#[test]
fn repeated_dev_member_all_packages() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "first"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [dependency-groups]
        dev = ["second"]

        [tool.uv.sources]
        second = { workspace = true }

        [tool.uv.workspace]
        members = ["second"]

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
        "#,
    )?;

    let src = context.temp_dir.child("src").child("first");
    src.create_dir_all()?;

    let init = src.child("__init__.py");
    init.touch()?;

    let child = context.temp_dir.child("second");
    fs_err::create_dir_all(&child)?;

    let pyproject_toml = child.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "second"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
        "#,
    )?;

    let src = child.child("src").child("second");
    src.create_dir_all()?;

    let init = src.child("__init__.py");
    init.touch()?;

    uv_snapshot!(context.filters(), context.sync().arg("--all-packages"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + first==0.1.0 (from file://[TEMP_DIR]/)
     + iniconfig==2.0.0
     + second==0.1.0 (from file://[TEMP_DIR]/second)
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--all-packages"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Audited 3 packages in [TIME]
    ");

    Ok(())
}

/// Test that hash checking doesn't fail with dependency metadata.
#[test]
fn direct_url_dependency_metadata() -> Result<()> {
    let context = TestContext::new("3.12");
    context.temp_dir.child("pyproject.toml").write_str(r#"
        [project]
        name = "debug"
        version = "0.1.0"
        requires-python = ">=3.9"
        dependencies = [
            "tqdm",
        ]

        [tool.uv]
        dependency-metadata = [
          { name = "tqdm", version = "4.67.1", requires-dist = [] },
        ]

        [tool.uv.sources]
        tqdm = { url = "https://files.pythonhosted.org/packages/d0/30/dc54f88dd4a2b5dc8a0279bdd7270e735851848b762aeb1c1184ed1f6b14/tqdm-4.67.1-py3-none-any.whl" }
        "#
    )?;

    uv_snapshot!(context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Installed 1 package in [TIME]
     + tqdm==4.67.1 (from https://files.pythonhosted.org/packages/d0/30/dc54f88dd4a2b5dc8a0279bdd7270e735851848b762aeb1c1184ed1f6b14/tqdm-4.67.1-py3-none-any.whl)
    ");

    Ok(())
}

#[test]
fn sync_required_environment_hint() -> Result<()> {
    let context = TestContext::new("3.13");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(&formatdoc! {r#"
        [project]
        name = "example"
        version = "0.1.0"
        requires-python = ">=3.13"
        dependencies = ["no-sdist-no-wheels-with-matching-platform-a"]

        [[tool.uv.index]]
        name = "packse"
        url = "{}"
        default = true
        "#,
        packse_index_url()
    })?;

    uv_snapshot!(context.filters(), context.lock().env_remove("UV_EXCLUDE_NEWER"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let mut filters = context.filters();
    filters.push((
        r"You're on [^ ]+ \(`.*`\)",
        "You're on [PLATFORM] (`[TAG]`)",
    ));

    uv_snapshot!(filters, context.sync().env_remove("UV_EXCLUDE_NEWER"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    error: Distribution `no-sdist-no-wheels-with-matching-platform-a==1.0.0 @ registry+https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/` can't be installed because it doesn't have a source distribution or wheel for the current platform

    hint: You're on [PLATFORM] (`[TAG]`), but `no-sdist-no-wheels-with-matching-platform-a` (v1.0.0) only has wheels for the following platform: `macosx_10_0_ppc64`; consider adding your platform to `tool.uv.required-environments` to ensure uv resolves to a version with compatible wheels
    ");

    Ok(())
}

#[test]
fn sync_url_with_query_parameters() -> Result<()> {
    let context = TestContext::new("3.13").with_exclude_newer("2025-03-24T19:00:00Z");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(r#"
        [project]
        name = "example"
        version = "0.1.0"
        requires-python = ">=3.13"
        dependencies = ["source-distribution @ https://files.pythonhosted.org/packages/1f/e5/5b016c945d745f8b108e759d428341488a6aee8f51f07c6c4e33498bb91f/source_distribution-0.0.3.tar.gz?foo=bar"]
        "#
    )?;

    uv_snapshot!(context.filters(), context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + source-distribution==0.0.3 (from https://files.pythonhosted.org/packages/1f/e5/5b016c945d745f8b108e759d428341488a6aee8f51f07c6c4e33498bb91f/source_distribution-0.0.3.tar.gz?foo=bar)
    ");

    Ok(())
}

/// Test uv sync with --exclude-newer-package
#[test]
fn sync_exclude_newer_package() -> Result<()> {
    let context = TestContext::new("3.12").with_filtered_counts();

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
[project]
name = "project"
version = "0.1.0"
requires-python = ">=3.12"
dependencies = [
    "tqdm",
    "requests",
]
"#,
    )?;

    // First sync with only the global exclude-newer to show the baseline
    uv_snapshot!(context.filters(), context
        .sync()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--exclude-newer")
        .arg("2022-04-04T12:00:00Z"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + certifi==2021.10.8
     + charset-normalizer==2.0.12
     + idna==3.3
     + requests==2.27.1
     + tqdm==4.64.0
     + urllib3==1.26.9
    "
    );

    // Now sync with --exclude-newer-package to allow tqdm to use a newer version
    uv_snapshot!(context.filters(), context
        .sync()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--exclude-newer")
        .arg("2022-04-04T12:00:00Z")
        .arg("--exclude-newer-package")
        .arg("tqdm=2022-09-04T00:00:00Z"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Ignoring existing lockfile due to change in timestamp cutoff: `global: 2022-04-04T12:00:00Z` vs. `global: 2022-04-04T12:00:00Z, tqdm: 2022-09-04T00:00:00Z`
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Uninstalled [N] packages in [TIME]
    Installed [N] packages in [TIME]
     - tqdm==4.64.0
     + tqdm==4.64.1
    "
    );

    Ok(())
}

/// Test exclude-newer-package in pyproject.toml configuration
#[test]
fn sync_exclude_newer_package_config() -> Result<()> {
    let context = TestContext::new("3.12").with_filtered_counts();

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
[project]
name = "project"
version = "0.1.0"
requires-python = ">=3.12"
dependencies = [
    "tqdm",
    "requests",
]

[tool.uv]
exclude-newer = "2022-04-04T12:00:00Z"
"#,
    )?;

    // First sync with only the global exclude-newer from the config
    uv_snapshot!(context.filters(), context
        .sync()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + certifi==2021.10.8
     + charset-normalizer==2.0.12
     + idna==3.3
     + requests==2.27.1
     + tqdm==4.64.0
     + urllib3==1.26.9
    "
    );

    // Now add the package-specific exclude-newer to the config
    pyproject_toml.write_str(
        r#"
[project]
name = "project"
version = "0.1.0"
requires-python = ">=3.12"
dependencies = [
    "tqdm",
    "requests",
]

[tool.uv]
exclude-newer = "2022-04-04T12:00:00Z"
exclude-newer-package = { tqdm = "2022-09-04T00:00:00Z" }
"#,
    )?;

    // Sync again with the package-specific override
    uv_snapshot!(context.filters(), context
        .sync()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Ignoring existing lockfile due to change in timestamp cutoff: `global: 2022-04-04T12:00:00Z` vs. `global: 2022-04-04T12:00:00Z, tqdm: 2022-09-04T00:00:00Z`
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Uninstalled [N] packages in [TIME]
    Installed [N] packages in [TIME]
     - tqdm==4.64.0
     + tqdm==4.64.1
    "
    );

    Ok(())
}

#[test]
#[cfg(unix)]
fn read_only() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

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

    uv_snapshot!(context.filters(), context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    assert!(context.temp_dir.child("uv.lock").exists());

    // Remove the flock.
    fs_err::remove_file(context.venv.child(".lock"))?;

    // Make the virtual environment read and execute (but not write).
    fs_err::set_permissions(&context.venv, std::fs::Permissions::from_mode(0o555))?;

    uv_snapshot!(context.filters(), context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Audited 1 package in [TIME]
    ");

    Ok(())
}

#[test]
fn sync_python_platform() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["black"]
        "#,
    )?;

    // Lock the project
    context.lock().assert().success();

    // Sync with a specific platform should filter packages
    uv_snapshot!(context.filters(), context.sync().arg("--python-platform").arg("linux"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 8 packages in [TIME]
    Prepared 6 packages in [TIME]
    Installed 6 packages in [TIME]
     + black==24.3.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    ");

    Ok(())
}

/// See: <https://github.com/astral-sh/uv/issues/11648>
#[test]
#[cfg(not(windows))]
fn conflicting_editable() -> Result<()> {
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
        foo = [
            "child",
        ]
        bar = [
            "child",
        ]
        [tool.uv]
        conflicts = [
          [
            { group = "foo" },
            { group = "bar" },
          ],
        ]
        [tool.uv.sources]
        child = [
            { path = "./child", editable = true, group = "foo" },
            { path = "./child", editable = false, group = "bar" },
        ]
        "#,
    )?;

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
        dependencies = []
        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
        "#,
        )?;
    context
        .temp_dir
        .child("child")
        .child("src")
        .child("child")
        .child("__init__.py")
        .touch()?;

    uv_snapshot!(context.filters(), context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Audited in [TIME]
    ");

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"
        conflicts = [[
            { package = "project", group = "bar" },
            { package = "project", group = "foo" },
        ]]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "child"
        version = "0.1.0"
        source = { directory = "child" }

        [[package]]
        name = "child"
        version = "0.1.0"
        source = { editable = "child" }

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }

        [package.dev-dependencies]
        bar = [
            { name = "child", version = "0.1.0", source = { directory = "child" } },
        ]
        foo = [
            { name = "child", version = "0.1.0", source = { editable = "child" } },
        ]

        [package.metadata]

        [package.metadata.requires-dev]
        bar = [{ name = "child", directory = "child" }]
        foo = [{ name = "child", editable = "child" }]
        "#
        );
    });

    uv_snapshot!(context.filters(), context.sync().arg("--group").arg("foo"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + child==0.1.0 (from file://[TEMP_DIR]/child)
    ");

    uv_snapshot!(context.filters(), context.pip_list().arg("--format").arg("json"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    [{"name":"child","version":"0.1.0","editable_project_location":"[TEMP_DIR]/child"}]

    ----- stderr -----
    "#);

    uv_snapshot!(context.filters(), context.sync().arg("--group").arg("bar"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     ~ child==0.1.0 (from file://[TEMP_DIR]/child)
    ");

    uv_snapshot!(context.filters(), context.pip_list().arg("--format").arg("json"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    [{"name":"child","version":"0.1.0"}]

    ----- stderr -----
    "#);

    Ok(())
}

/// See: <https://github.com/astral-sh/uv/issues/11648>
#[test]
#[cfg(not(windows))]
fn undeclared_editable() -> Result<()> {
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
        foo = [
            "child",
        ]
        bar = [
            "child",
        ]
        [tool.uv]
        conflicts = [
          [
            { group = "foo" },
            { group = "bar" },
          ],
        ]
        [tool.uv.sources]
        child = [
            { path = "./child", editable = true, group = "foo" },
            { path = "./child", group = "bar" },
        ]
        "#,
    )?;

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
        dependencies = []
        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
        "#,
        )?;
    context
        .temp_dir
        .child("child")
        .child("src")
        .child("child")
        .child("__init__.py")
        .touch()?;

    uv_snapshot!(context.filters(), context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Audited in [TIME]
    ");

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"
        conflicts = [[
            { package = "project", group = "bar" },
            { package = "project", group = "foo" },
        ]]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "child"
        version = "0.1.0"
        source = { directory = "child" }

        [[package]]
        name = "child"
        version = "0.1.0"
        source = { editable = "child" }

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }

        [package.dev-dependencies]
        bar = [
            { name = "child", version = "0.1.0", source = { directory = "child" } },
        ]
        foo = [
            { name = "child", version = "0.1.0", source = { editable = "child" } },
        ]

        [package.metadata]

        [package.metadata.requires-dev]
        bar = [{ name = "child", directory = "child" }]
        foo = [{ name = "child", editable = "child" }]
        "#
        );
    });

    uv_snapshot!(context.filters(), context.sync().arg("--group").arg("foo"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + child==0.1.0 (from file://[TEMP_DIR]/child)
    ");

    uv_snapshot!(context.filters(), context.pip_list().arg("--format").arg("json"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    [{"name":"child","version":"0.1.0","editable_project_location":"[TEMP_DIR]/child"}]

    ----- stderr -----
    "#);

    uv_snapshot!(context.filters(), context.sync().arg("--group").arg("bar"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     ~ child==0.1.0 (from file://[TEMP_DIR]/child)
    ");

    uv_snapshot!(context.filters(), context.pip_list().arg("--format").arg("json"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    [{"name":"child","version":"0.1.0"}]

    ----- stderr -----
    "#);

    Ok(())
}

#[test]
fn sync_python_preference() -> Result<()> {
    let context = TestContext::new_with_versions(&["3.12", "3.11"]);

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.11"
        dependencies = []
        "#,
    )?;

    // Run an initial sync, with 3.12 as an "unmanaged" interpreter
    context.sync().assert().success();

    // Mark 3.12 as a managed interpreter for the rest of the tests
    let context = context.with_versions_as_managed(&["3.12"]);
    uv_snapshot!(context.filters(), context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Audited in [TIME]
    ");

    // We should invalidate the environment and switch to 3.11
    uv_snapshot!(context.filters(), context.sync().arg("--no-managed-python"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.11.[X] interpreter at: [PYTHON-3.11]
    Removed virtual environment at: .venv
    Creating virtual environment at: .venv
    Resolved 1 package in [TIME]
    Audited in [TIME]
    ");

    // We will use the environment if it exists
    uv_snapshot!(context.filters(), context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Audited in [TIME]
    ");

    // Unless the user requests a Python preference that is incompatible
    uv_snapshot!(context.filters(), context.sync().arg("--managed-python"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Removed virtual environment at: .venv
    Creating virtual environment at: .venv
    Resolved 1 package in [TIME]
    Audited in [TIME]
    ");

    // If a interpreter cannot be found, we'll fail
    uv_snapshot!(context.filters(), context.sync().arg("--managed-python").arg("-p").arg("3.11"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No interpreter found for Python 3.11 in managed installations

    hint: A managed Python download is available for Python 3.11, but Python downloads are set to 'never'
    ");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.11"
        dependencies = []

        [tool.uv]
        python-preference = "only-system"
        "#,
    )?;

    // We'll respect a `python-preference` in the `pyproject.toml` file
    uv_snapshot!(context.filters(), context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.11.[X] interpreter at: [PYTHON-3.11]
    Removed virtual environment at: .venv
    Creating virtual environment at: .venv
    Resolved 1 package in [TIME]
    Audited in [TIME]
    ");

    // But it can be overridden via the CLI
    uv_snapshot!(context.filters(), context.sync().arg("--managed-python"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Removed virtual environment at: .venv
    Creating virtual environment at: .venv
    Resolved 1 package in [TIME]
    Audited in [TIME]
    ");

    // `uv run` will invalidate the environment too
    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.11.[X]

    ----- stderr -----
    Using CPython 3.11.[X] interpreter at: [PYTHON-3.11]
    Removed virtual environment at: .venv
    Creating virtual environment at: .venv
    Resolved 1 package in [TIME]
    Audited in [TIME]
    ");

    Ok(())
}

#[test]
fn sync_config_settings_package() -> Result<()> {
    let context = TestContext::new("3.12").with_exclude_newer("2025-07-25T00:00:00Z");

    // Create a child project that uses `setuptools`.
    let dependency = context.temp_dir.child("dependency");
    dependency.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dependency"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;
    dependency
        .child("dependency")
        .child("__init__.py")
        .touch()?;

    // Install the `dependency` without `editable_mode=compat`.
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dependency"]

        [tool.uv.sources]
        dependency = { path = "dependency", editable = true }
        "#,
    )?;

    // Lock the project
    context.lock().assert().success();

    uv_snapshot!(context.filters(), context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + dependency==0.1.0 (from file://[TEMP_DIR]/dependency)
    ");

    // When installed without `editable_mode=compat`, the `finder.py` file should be present.
    let finder = context
        .site_packages()
        .join("__editable___dependency_0_1_0_finder.py");
    assert!(finder.exists());

    // Remove the virtual environment.
    fs_err::remove_dir_all(&context.venv)?;

    // Install the `dependency` with `editable_mode=compat` scoped to the package.
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dependency"]

        [tool.uv.sources]
        dependency = { path = "dependency", editable = true }

        [tool.uv.config-settings-package]
        dependency = { editable_mode = "compat" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: .venv
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + dependency==0.1.0 (from file://[TEMP_DIR]/dependency)
    ");

    // When installed with `editable_mode=compat`, the `finder.py` file should _not_ be present.
    let finder = context
        .site_packages()
        .join("__editable___dependency_0_1_0_finder.py");
    assert!(!finder.exists());

    // Remove the virtual environment.
    fs_err::remove_dir_all(&context.venv)?;

    // Install the `dependency` with `editable_mode=compat` scoped to another package.
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dependency"]

        [tool.uv.sources]
        dependency = { path = "dependency", editable = true }

        [tool.uv.config-settings-package]
        setuptools = { editable_mode = "compat" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: .venv
    Resolved 2 packages in [TIME]
    Installed 1 package in [TIME]
     + dependency==0.1.0 (from file://[TEMP_DIR]/dependency)
    ");

    // When installed without `editable_mode=compat`, the `finder.py` file should be present.
    let finder = context
        .site_packages()
        .join("__editable___dependency_0_1_0_finder.py");
    assert!(finder.exists());

    Ok(())
}

/// Ensure that when we sync to an empty virtual environment directory, we don't attempt to remove
/// it, which breaks Docker volume mounts.
#[test]
#[cfg(unix)]
fn sync_does_not_remove_empty_virtual_environment_directory() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let context = TestContext::new_with_versions(&["3.12"]);

    let project_dir = context.temp_dir.child("project");
    fs_err::create_dir(&project_dir)?;

    let pyproject_toml = project_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]
        "#,
    )?;

    let venv_dir = project_dir.child(".venv");
    fs_err::create_dir(&venv_dir)?;

    // Ensure the parent is read-only, to prevent deletion of the virtual environment
    fs_err::set_permissions(&project_dir, std::fs::Permissions::from_mode(0o555))?;

    // Note we do _not_ fail to create the virtual environment — we fail later when writing to the
    // project directory
    uv_snapshot!(context.filters(), context.sync().current_dir(&project_dir), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: .venv
    Resolved 2 packages in [TIME]
    error: failed to write to file `[TEMP_DIR]/project/uv.lock`: Permission denied (os error 13)
    ");

    Ok(())
}

/// Test that build dependencies respect locked versions from the lockfile.
#[test]
fn sync_build_dependencies_respect_locked_versions() -> Result<()> {
    let context = TestContext::new("3.12").with_filtered_counts();

    // Write a test package that arbitrarily requires `anyio` at build time
    let child = context.temp_dir.child("child");
    child.create_dir_all()?;
    let child_pyproject_toml = child.child("pyproject.toml");
    child_pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "child"
        version = "0.1.0"
        requires-python = ">=3.9"

        [build-system]
        requires = ["hatchling", "anyio"]
        backend-path = ["."]
        build-backend = "build_backend"
    "#})?;

    // Create a build backend that checks for a specific version of anyio
    let build_backend = child.child("build_backend.py");
    build_backend.write_str(indoc! {r#"
        import os
        import sys
        from hatchling.build import *

        expected_version = os.environ.get("EXPECTED_ANYIO_VERSION", "")
        if not expected_version:
            print("`EXPECTED_ANYIO_VERSION` not set", file=sys.stderr)
            sys.exit(1)

        try:
            import anyio
        except ModuleNotFoundError:
            print("Missing `anyio` module", file=sys.stderr)
            sys.exit(1)

        from importlib.metadata import version
        anyio_version = version("anyio")

        if not anyio_version.startswith(expected_version):
            print(f"Expected `anyio` version {expected_version} but got {anyio_version}", file=sys.stderr)
            sys.exit(1)

        print(f"Found expected `anyio` version {anyio_version}", file=sys.stderr)
    "#})?;
    child.child("src/child/__init__.py").touch()?;

    // Create a project that will resolve to a non-latest version of `anyio`
    let parent = &context.temp_dir;
    let pyproject_toml = parent.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "parent"
        version = "0.1.0"
        requires-python = ">=3.9"
        dependencies = ["anyio<4.1"]
    "#})?;

    uv_snapshot!(context.filters(), context.lock(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    ");

    // Now add the child dependency.
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "parent"
        version = "0.1.0"
        requires-python = ">=3.9"
        dependencies = ["anyio<4.1", "child"]

        [tool.uv.sources]
        child = { path = "child" }
    "#})?;

    // Ensure our build backend is checking the version correctly
    uv_snapshot!(context.filters(), context.sync().env("EXPECTED_ANYIO_VERSION", "3.0"), @r"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
      × Failed to build `child @ file://[TEMP_DIR]/child`
      ├─▶ The build backend returned an error
      ╰─▶ Call to `build_backend.build_wheel` failed (exit status: 1)

          [stderr]
          Expected `anyio` version 3.0 but got 4.3.0

          hint: This usually indicates a problem with the package or the build environment.
      help: `child` was included because `parent` (v0.1.0) depends on `child`
    ");

    // Now constrain the `anyio` build dependency to match the runtime
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "parent"
        version = "0.1.0"
        requires-python = ">=3.9"
        dependencies = ["anyio<4.1", "child"]

        [tool.uv.sources]
        child = { path = "child" }

        [tool.uv.extra-build-dependencies]
        child = [{ requirement = "anyio", match-runtime = true }]
    "#})?;

    // The child should be built with anyio 4.0
    uv_snapshot!(context.filters(), context.sync().env("EXPECTED_ANYIO_VERSION", "4.0"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: The `extra-build-dependencies` option is experimental and may change without warning. Pass `--preview-features extra-build-dependencies` to disable this warning.
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + anyio==4.0.0
     + child==0.1.0 (from file://[TEMP_DIR]/child)
     + idna==3.6
     + sniffio==1.3.1
    ");

    // Change the constraints on anyio
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "parent"
        version = "0.1.0"
        requires-python = ">=3.9"
        dependencies = ["anyio<3.8", "child"]

        [tool.uv.sources]
        child = { path = "child" }

        [tool.uv.extra-build-dependencies]
        child = [{ requirement = "anyio", match-runtime = true }]
    "#})?;

    // The child should be rebuilt with anyio 3.7, without `--reinstall`
    uv_snapshot!(context.filters(), context.sync()
        .arg("--reinstall-package").arg("child").env("EXPECTED_ANYIO_VERSION", "4.0"), @r"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    warning: The `extra-build-dependencies` option is experimental and may change without warning. Pass `--preview-features extra-build-dependencies` to disable this warning.
    Resolved [N] packages in [TIME]
      × Failed to build `child @ file://[TEMP_DIR]/child`
      ├─▶ The build backend returned an error
      ╰─▶ Call to `build_backend.build_wheel` failed (exit status: 1)

          [stderr]
          Expected `anyio` version 4.0 but got 3.7.1

          hint: This usually indicates a problem with the package or the build environment.
      help: `child` was included because `parent` (v0.1.0) depends on `child`
    ");

    uv_snapshot!(context.filters(), context.sync()
        .arg("--reinstall-package").arg("child").env("EXPECTED_ANYIO_VERSION", "3.7"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: The `extra-build-dependencies` option is experimental and may change without warning. Pass `--preview-features extra-build-dependencies` to disable this warning.
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Uninstalled [N] packages in [TIME]
    Installed [N] packages in [TIME]
     - anyio==4.0.0
     + anyio==3.7.1
     ~ child==0.1.0 (from file://[TEMP_DIR]/child)
    ");

    // With preview enabled, there's no warning
    uv_snapshot!(context.filters(), context.sync()
        .arg("--preview-features").arg("extra-build-dependencies")
        .arg("--reinstall-package").arg("child")
        .env("EXPECTED_ANYIO_VERSION", "3.7"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Uninstalled [N] packages in [TIME]
    Installed [N] packages in [TIME]
     ~ child==0.1.0 (from file://[TEMP_DIR]/child)
    ");

    // Now, we'll set a constraint in the parent project
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "parent"
        version = "0.1.0"
        requires-python = ">=3.9"
        dependencies = ["anyio<3.8", "child"]

        [tool.uv.sources]
        child = { path = "child" }

        [tool.uv.extra-build-dependencies]
        child = [{ requirement = "anyio", match-runtime = true }]
    "#})?;

    // And an incompatible constraint in the child project
    child_pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "child"
        version = "0.1.0"
        requires-python = ">=3.9"

        [build-system]
        requires = ["hatchling", "anyio>3.8,<4.2"]
        backend-path = ["."]
        build-backend = "build_backend"
    "#})?;

    // This should fail
    uv_snapshot!(context.filters(), context.sync()
        .arg("--reinstall-package").arg("child").env("EXPECTED_ANYIO_VERSION", "4.1"), @r"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    warning: The `extra-build-dependencies` option is experimental and may change without warning. Pass `--preview-features extra-build-dependencies` to disable this warning.
    Resolved [N] packages in [TIME]
      × Failed to build `child @ file://[TEMP_DIR]/child`
      ├─▶ Failed to resolve requirements from `build-system.requires` and `extra-build-dependencies`
      ├─▶ No solution found when resolving: `hatchling`, `anyio>3.8, <4.2`, `anyio==3.7.1 (index: https://pypi.org/simple)`
      ╰─▶ Because you require anyio>3.8,<4.2 and anyio==3.7.1, we can conclude that your requirements are unsatisfiable.
      help: `child` was included because `parent` (v0.1.0) depends on `child`
    ");

    // Adding a version specifier should also fail
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "parent"
        version = "0.1.0"
        requires-python = ">=3.9"
        dependencies = ["anyio<4.1", "child"]

        [tool.uv.sources]
        child = { path = "child" }

        [tool.uv.extra-build-dependencies]
        child = [{ requirement = "anyio>4", match-runtime = true }]
    "#})?;

    uv_snapshot!(context.filters(), context.sync(), @r#"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: Failed to parse `pyproject.toml` during settings discovery:
      TOML parse error at line 11, column 9
         |
      11 | child = [{ requirement = "anyio>4", match-runtime = true }]
         |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
      Dependencies marked with `match-runtime = true` cannot include version specifiers

    error: Failed to parse: `pyproject.toml`
      Caused by: TOML parse error at line 11, column 9
       |
    11 | child = [{ requirement = "anyio>4", match-runtime = true }]
       |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    Dependencies marked with `match-runtime = true` cannot include version specifiers
    "#);

    Ok(())
}
