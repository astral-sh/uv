#![cfg(all(feature = "python", feature = "pypi"))]

use anyhow::Result;
use assert_fs::prelude::*;

use common::{uv_snapshot, TestContext};

mod common;

#[test]
fn nested_dependencies() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        # ...
        requires-python = ">=3.12"
        dependencies = [
            "scikit-learn==1.4.1.post1"
        ]
    "#,
    )?;

    uv_snapshot!(context.filters(), context.tree(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    project v0.1.0
    └── scikit-learn v1.4.1.post1
        ├── joblib v1.3.2
        ├── numpy v1.26.4
        ├── scipy v1.12.0
        │   └── numpy v1.26.4
        └── threadpoolctl v3.4.0

    ----- stderr -----
    warning: `uv tree` is experimental and may change without warning
    Resolved 6 packages in [TIME]
    "###
    );

    // `uv tree` should update the lockfile
    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;
    assert!(!lock.is_empty());

    Ok(())
}

#[test]
fn invert() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        # ...
        requires-python = ">=3.12"
        dependencies = [
            "scikit-learn==1.4.1.post1"
        ]
    "#,
    )?;

    uv_snapshot!(context.filters(), context.tree().arg("--invert"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    joblib v1.3.2
    └── scikit-learn v1.4.1.post1
        └── project v0.1.0
    numpy v1.26.4
    ├── scikit-learn v1.4.1.post1 (*)
    └── scipy v1.12.0
        └── scikit-learn v1.4.1.post1 (*)
    threadpoolctl v3.4.0
    └── scikit-learn v1.4.1.post1 (*)
    (*) Package tree already displayed

    ----- stderr -----
    warning: `uv tree` is experimental and may change without warning
    Resolved 6 packages in [TIME]
    "###
    );

    uv_snapshot!(context.filters(), context.tree().arg("--invert").arg("--no-dedupe"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    joblib v1.3.2
    └── scikit-learn v1.4.1.post1
        └── project v0.1.0
    numpy v1.26.4
    ├── scikit-learn v1.4.1.post1
    │   └── project v0.1.0
    └── scipy v1.12.0
        └── scikit-learn v1.4.1.post1
            └── project v0.1.0
    threadpoolctl v3.4.0
    └── scikit-learn v1.4.1.post1
        └── project v0.1.0

    ----- stderr -----
    warning: `uv tree` is experimental and may change without warning
    Resolved 6 packages in [TIME]
    "###
    );

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
        # ...
        requires-python = ">=3.12"
        dependencies = ["anyio"]
    "#,
    )?;

    uv_snapshot!(context.filters(), context.tree(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    project v0.1.0
    └── anyio v4.3.0
        ├── idna v3.6
        └── sniffio v1.3.1

    ----- stderr -----
    warning: `uv tree` is experimental and may change without warning
    Resolved 4 packages in [TIME]
    "###
    );

    // `uv tree` should update the lockfile
    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;
    assert!(!lock.is_empty());

    // Update the project dependencies.
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        # ...
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]
    "#,
    )?;

    // Running with `--frozen` should show the stale tree.
    uv_snapshot!(context.filters(), context.tree().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    project v0.1.0
    └── anyio v4.3.0
        ├── idna v3.6
        └── sniffio v1.3.1

    ----- stderr -----
    warning: `uv tree` is experimental and may change without warning
    "###
    );

    Ok(())
}
