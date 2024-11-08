use anyhow::Result;
use assert_cmd::assert::OutputAssertExt;
use assert_fs::prelude::*;
use indoc::formatdoc;
use url::Url;

use crate::common::{uv_snapshot, TestContext};

#[test]
fn nested_dependencies() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "scikit-learn==1.4.1.post1"
        ]
    "#,
    )?;

    uv_snapshot!(context.filters(), context.tree().arg("--universal"), @r###"
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
    Resolved 6 packages in [TIME]
    "###
    );

    // `uv tree` should update the lockfile
    let lock = context.read("uv.lock");
    assert!(!lock.is_empty());

    Ok(())
}

#[test]
fn nested_platform_dependencies() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "jupyter-client"
        ]
    "#,
    )?;

    uv_snapshot!(context.filters(), context.tree().arg("--python-platform").arg("linux"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    project v0.1.0
    └── jupyter-client v8.6.1
        ├── jupyter-core v5.7.2
        │   ├── platformdirs v4.2.0
        │   └── traitlets v5.14.2
        ├── python-dateutil v2.9.0.post0
        │   └── six v1.16.0
        ├── pyzmq v25.1.2
        ├── tornado v6.4
        └── traitlets v5.14.2

    ----- stderr -----
    Resolved 12 packages in [TIME]
    "###
    );

    uv_snapshot!(context.filters(), context.tree().arg("--universal"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    project v0.1.0
    └── jupyter-client v8.6.1
        ├── jupyter-core v5.7.2
        │   ├── platformdirs v4.2.0
        │   ├── pywin32 v306
        │   └── traitlets v5.14.2
        ├── python-dateutil v2.9.0.post0
        │   └── six v1.16.0
        ├── pyzmq v25.1.2
        │   └── cffi v1.16.0
        │       └── pycparser v2.21
        ├── tornado v6.4
        └── traitlets v5.14.2

    ----- stderr -----
    Resolved 12 packages in [TIME]
    "###
    );

    // `uv tree` should update the lockfile
    let lock = context.read("uv.lock");
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
        requires-python = ">=3.12"
        dependencies = ["anyio"]
    "#,
    )?;

    uv_snapshot!(context.filters(), context.tree().arg("--universal"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    project v0.1.0
    └── anyio v4.3.0
        ├── idna v3.6
        └── sniffio v1.3.1

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "###
    );

    // `uv tree` should update the lockfile
    let lock = context.read("uv.lock");
    assert!(!lock.is_empty());

    // Update the project dependencies.
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
    "###
    );

    Ok(())
}

#[test]
fn outdated() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.0.0"]
    "#,
    )?;

    uv_snapshot!(context.filters(), context.tree().arg("--outdated").arg("--universal"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    project v0.1.0
    └── anyio v3.0.0 (latest: v4.3.0)
        ├── idna v3.6
        └── sniffio v1.3.1

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "###
    );

    Ok(())
}

#[test]
fn platform_dependencies() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "black"
        ]
    "#,
    )?;

    // When `--universal` is _not_ provided, `colorama` should _not_ be included.
    #[cfg(not(windows))]
    uv_snapshot!(context.filters(), context.tree(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    project v0.1.0
    └── black v24.3.0
        ├── click v8.1.7
        ├── mypy-extensions v1.0.0
        ├── packaging v24.0
        ├── pathspec v0.12.1
        └── platformdirs v4.2.0

    ----- stderr -----
    Resolved 8 packages in [TIME]
    "###);

    // Unless `--python-platform` is set to `windows`, in which case it should be included.
    uv_snapshot!(context.filters(), context.tree().arg("--python-platform").arg("windows"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    project v0.1.0
    └── black v24.3.0
        ├── click v8.1.7
        │   └── colorama v0.4.6
        ├── mypy-extensions v1.0.0
        ├── packaging v24.0
        ├── pathspec v0.12.1
        └── platformdirs v4.2.0

    ----- stderr -----
    Resolved 8 packages in [TIME]
    "###);

    // When `--universal` is _not_ provided, should include `colorama`, even though it's only
    // included on Windows.
    uv_snapshot!(context.filters(), context.tree().arg("--universal"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    project v0.1.0
    └── black v24.3.0
        ├── click v8.1.7
        │   └── colorama v0.4.6
        ├── mypy-extensions v1.0.0
        ├── packaging v24.0
        ├── pathspec v0.12.1
        └── platformdirs v4.2.0

    ----- stderr -----
    Resolved 8 packages in [TIME]
    "###
    );

    // `uv tree` should update the lockfile
    let lock = context.read("uv.lock");
    assert!(!lock.is_empty());

    Ok(())
}

#[test]
fn platform_dependencies_inverted() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "click"
        ]
    "#,
    )?;

    // When `--universal` is _not_ provided, `colorama` should _not_ be included.
    uv_snapshot!(context.filters(), context.tree().arg("--invert").arg("--python-platform").arg("linux"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    click v8.1.7
    └── project v0.1.0

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###);

    // Unless `--python-platform` is set to `windows`, in which case it should be included.
    uv_snapshot!(context.filters(), context.tree().arg("--invert").arg("--python-platform").arg("windows"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    colorama v0.4.6
    └── click v8.1.7
        └── project v0.1.0

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "#);

    Ok(())
}

#[test]
fn repeated_dependencies() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "anyio < 2 ; sys_platform == 'win32'",
            "anyio > 2 ; sys_platform == 'linux'",
        ]
    "#,
    )?;

    // Should include both versions of `anyio`, which have different dependencies.
    uv_snapshot!(context.filters(), context.tree().arg("--universal"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    project v0.1.0
    ├── anyio v1.4.0
    │   ├── async-generator v1.10
    │   ├── idna v3.6
    │   └── sniffio v1.3.1
    └── anyio v4.3.0
        ├── idna v3.6
        └── sniffio v1.3.1

    ----- stderr -----
    Resolved 6 packages in [TIME]
    "###
    );

    // `uv tree` should update the lockfile
    let lock = context.read("uv.lock");
    assert!(!lock.is_empty());

    Ok(())
}

/// In this case, a package is included twice at the same version, but pointing to different direct
/// URLs.
#[test]
fn repeated_version() -> Result<()> {
    let context = TestContext::new("3.12");

    let v1 = context.temp_dir.child("v1");
    fs_err::create_dir_all(&v1)?;
    let pyproject_toml = v1.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "dependency"
        version = "0.0.1"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0"]
        "#,
    )?;

    let v2 = context.temp_dir.child("v2");
    fs_err::create_dir_all(&v2)?;
    let pyproject_toml = v2.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "dependency"
        version = "0.0.1"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.0.0"]
        "#,
    )?;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(&formatdoc! {
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
          "dependency @ {} ; sys_platform == 'darwin'",
          "dependency @ {} ; sys_platform != 'darwin'",
        ]
        "#,
        Url::from_file_path(context.temp_dir.join("v1")).unwrap(),
        Url::from_file_path(context.temp_dir.join("v2")).unwrap(),
    })?;

    uv_snapshot!(context.filters(), context.tree().arg("--universal"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    project v0.1.0
    ├── dependency v0.0.1
    │   └── anyio v3.7.0
    │       ├── idna v3.6
    │       └── sniffio v1.3.1
    └── dependency v0.0.1
        └── anyio v3.0.0
            ├── idna v3.6
            └── sniffio v1.3.1

    ----- stderr -----
    Resolved 7 packages in [TIME]
    "###
    );

    // `uv tree` should update the lockfile
    let lock = context.read("uv.lock");
    assert!(!lock.is_empty());

    Ok(())
}

#[test]
fn dev_dependencies() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]

        [tool.uv]
        dev-dependencies = ["anyio"]
    "#,
    )?;

    uv_snapshot!(context.filters(), context.tree(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    project v0.1.0
    ├── iniconfig v2.0.0
    └── anyio v4.3.0 (group: dev)
        ├── idna v3.6
        └── sniffio v1.3.1

    ----- stderr -----
    Resolved 5 packages in [TIME]
    "###
    );

    uv_snapshot!(context.filters(), context.tree().arg("--no-dev"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    project v0.1.0
    └── iniconfig v2.0.0

    ----- stderr -----
    Resolved 5 packages in [TIME]
    "###
    );

    // `uv tree` should update the lockfile
    let lock = context.read("uv.lock");
    assert!(!lock.is_empty());

    Ok(())
}

#[test]
fn dev_dependencies_inverted() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]
        [tool.uv]
        dev-dependencies = ["anyio"]
    "#,
    )?;

    uv_snapshot!(context.filters(), context.tree().arg("--universal").arg("--invert"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    idna v3.6
    └── anyio v4.3.0
        └── project v0.1.0 (group: dev)
    iniconfig v2.0.0
    └── project v0.1.0
    sniffio v1.3.1
    └── anyio v4.3.0 (*)
    (*) Package tree already displayed

    ----- stderr -----
    Resolved 5 packages in [TIME]
    "###
    );

    // `uv tree` should update the lockfile
    let lock = context.read("uv.lock");
    assert!(!lock.is_empty());

    Ok(())
}

#[test]
fn optional_dependencies() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig", "flask[dotenv]"]

        [project.optional-dependencies]
        async = ["anyio"]
    "#,
    )?;

    uv_snapshot!(context.filters(), context.tree().arg("--universal"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    project v0.1.0
    ├── flask[dotenv] v3.0.2
    │   ├── blinker v1.7.0
    │   ├── click v8.1.7
    │   │   └── colorama v0.4.6
    │   ├── itsdangerous v2.1.2
    │   ├── jinja2 v3.1.3
    │   │   └── markupsafe v2.1.5
    │   ├── werkzeug v3.0.1
    │   │   └── markupsafe v2.1.5
    │   └── python-dotenv v1.0.1 (extra: dotenv)
    ├── iniconfig v2.0.0
    └── anyio v4.3.0 (extra: async)
        ├── idna v3.6
        └── sniffio v1.3.1

    ----- stderr -----
    Resolved 14 packages in [TIME]
    "###
    );

    // `uv tree` should update the lockfile
    let lock = context.read("uv.lock");
    assert!(!lock.is_empty());

    Ok(())
}

#[test]
fn optional_dependencies_inverted() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig", "flask[dotenv]"]

        [project.optional-dependencies]
        async = ["anyio"]
    "#,
    )?;

    uv_snapshot!(context.filters(), context.tree().arg("--universal").arg("--invert"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    blinker v1.7.0
    └── flask v3.0.2
        └── project[dotenv] v0.1.0
    colorama v0.4.6
    └── click v8.1.7
        └── flask v3.0.2 (*)
    idna v3.6
    └── anyio v4.3.0
        └── project v0.1.0 (extra: async)
    iniconfig v2.0.0
    └── project v0.1.0
    itsdangerous v2.1.2
    └── flask v3.0.2 (*)
    markupsafe v2.1.5
    ├── jinja2 v3.1.3
    │   └── flask v3.0.2 (*)
    └── werkzeug v3.0.1
        └── flask v3.0.2 (*)
    python-dotenv v1.0.1
    └── flask v3.0.2 (extra: dotenv) (*)
    sniffio v1.3.1
    └── anyio v4.3.0 (*)
    (*) Package tree already displayed

    ----- stderr -----
    Resolved 14 packages in [TIME]
    "###
    );

    // `uv tree` should update the lockfile
    let lock = context.read("uv.lock");
    assert!(!lock.is_empty());

    Ok(())
}

#[test]
fn package() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["scikit-learn==1.4.1.post1", "pandas"]
    "#,
    )?;

    uv_snapshot!(context.filters(), context.tree(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    project v0.1.0
    ├── pandas v2.2.1
    │   ├── numpy v1.26.4
    │   ├── python-dateutil v2.9.0.post0
    │   │   └── six v1.16.0
    │   ├── pytz v2024.1
    │   └── tzdata v2024.1
    └── scikit-learn v1.4.1.post1
        ├── joblib v1.3.2
        ├── numpy v1.26.4
        ├── scipy v1.12.0
        │   └── numpy v1.26.4
        └── threadpoolctl v3.4.0

    ----- stderr -----
    Resolved 11 packages in [TIME]
    "###
    );

    uv_snapshot!(context.filters(), context.tree().arg("--package").arg("scipy"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    scipy v1.12.0
    └── numpy v1.26.4

    ----- stderr -----
    Resolved 11 packages in [TIME]
    "###
    );

    uv_snapshot!(context.filters(), context.tree().arg("--package").arg("numpy").arg("--invert"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    numpy v1.26.4
    ├── pandas v2.2.1
    │   └── project v0.1.0
    ├── scikit-learn v1.4.1.post1
    │   └── project v0.1.0
    └── scipy v1.12.0
        └── scikit-learn v1.4.1.post1 (*)
    (*) Package tree already displayed

    ----- stderr -----
    Resolved 11 packages in [TIME]
    "###
    );

    // `uv tree` should update the lockfile
    let lock = context.read("uv.lock");
    assert!(!lock.is_empty());

    Ok(())
}

#[test]
fn group() -> Result<()> {
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
        foo = ["anyio"]
        bar = ["iniconfig"]
        dev = ["sniffio"]
        "#,
    )?;

    context.lock().assert().success();

    uv_snapshot!(context.filters(), context.tree(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    project v0.1.0
    ├── typing-extensions v4.10.0
    └── sniffio v1.3.1 (group: dev)

    ----- stderr -----
    Resolved 6 packages in [TIME]
    "###);

    uv_snapshot!(context.filters(), context.tree().arg("--only-group").arg("bar"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    project v0.1.0
    └── iniconfig v2.0.0 (group: bar)

    ----- stderr -----
    Resolved 6 packages in [TIME]
    "###);

    uv_snapshot!(context.filters(), context.tree().arg("--group").arg("foo"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    project v0.1.0
    ├── typing-extensions v4.10.0
    ├── sniffio v1.3.1 (group: dev)
    └── anyio v4.3.0 (group: foo)
        ├── idna v3.6
        └── sniffio v1.3.1

    ----- stderr -----
    Resolved 6 packages in [TIME]
    "###);

    uv_snapshot!(context.filters(), context.tree().arg("--group").arg("foo").arg("--group").arg("bar"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    project v0.1.0
    ├── typing-extensions v4.10.0
    ├── iniconfig v2.0.0 (group: bar)
    ├── sniffio v1.3.1 (group: dev)
    └── anyio v4.3.0 (group: foo)
        ├── idna v3.6
        └── sniffio v1.3.1

    ----- stderr -----
    Resolved 6 packages in [TIME]
    "###);

    uv_snapshot!(context.filters(), context.tree().arg("--all-groups"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    project v0.1.0
    ├── typing-extensions v4.10.0
    ├── iniconfig v2.0.0 (group: bar)
    ├── sniffio v1.3.1 (group: dev)
    └── anyio v4.3.0 (group: foo)
        ├── idna v3.6
        └── sniffio v1.3.1

    ----- stderr -----
    Resolved 6 packages in [TIME]
    "###);

    uv_snapshot!(context.filters(), context.tree().arg("--all-groups").arg("--no-group").arg("bar"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    project v0.1.0
    ├── typing-extensions v4.10.0
    ├── sniffio v1.3.1 (group: dev)
    └── anyio v4.3.0 (group: foo)
        ├── idna v3.6
        └── sniffio v1.3.1

    ----- stderr -----
    Resolved 6 packages in [TIME]
    "###);

    Ok(())
}

#[test]
fn cycle() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["testtools==2.3.0", "fixtures==3.0.0"]
    "#,
    )?;

    uv_snapshot!(context.filters(), context.tree().arg("--universal"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    project v0.1.0
    ├── fixtures v3.0.0
    │   ├── pbr v6.0.0
    │   ├── six v1.16.0
    │   └── testtools v2.3.0
    │       ├── extras v1.0.0
    │       ├── fixtures v3.0.0 (*)
    │       ├── pbr v6.0.0
    │       ├── python-mimeparse v1.6.0
    │       ├── six v1.16.0
    │       ├── traceback2 v1.4.0
    │       │   └── linecache2 v1.0.0
    │       └── unittest2 v1.1.0
    │           ├── argparse v1.4.0
    │           ├── six v1.16.0
    │           └── traceback2 v1.4.0 (*)
    └── testtools v2.3.0 (*)
    (*) Package tree already displayed

    ----- stderr -----
    Resolved 11 packages in [TIME]
    "###
    );

    uv_snapshot!(context.filters(), context.tree().arg("--package").arg("traceback2").arg("--package").arg("six"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    six v1.16.0
    traceback2 v1.4.0
    └── linecache2 v1.0.0

    ----- stderr -----
    Resolved 11 packages in [TIME]
    "###
    );

    uv_snapshot!(context.filters(), context.tree().arg("--package").arg("traceback2").arg("--package").arg("six").arg("--invert"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    six v1.16.0
    ├── fixtures v3.0.0
    │   ├── project v0.1.0
    │   └── testtools v2.3.0
    │       ├── fixtures v3.0.0 (*)
    │       └── project v0.1.0
    ├── testtools v2.3.0 (*)
    └── unittest2 v1.1.0
        └── testtools v2.3.0 (*)
    traceback2 v1.4.0
    ├── testtools v2.3.0 (*)
    └── unittest2 v1.1.0 (*)
    (*) Package tree already displayed

    ----- stderr -----
    Resolved 11 packages in [TIME]
    "###
    );

    // `uv tree` should update the lockfile
    let lock = context.read("uv.lock");
    assert!(!lock.is_empty());

    Ok(())
}

#[test]
fn workspace_dev() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio"]

        [dependency-groups]
        dev = ["child"]

        [tool.uv.workspace]
        members = ["child"]

        [tool.uv.sources]
        child = { workspace = true }
    "#,
    )?;

    let child = context.temp_dir.child("child");
    let pyproject_toml = child.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "child"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]
    "#,
    )?;

    uv_snapshot!(context.filters(), context.tree().arg("--universal"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    project v0.1.0
    ├── anyio v4.3.0
    │   ├── idna v3.6
    │   └── sniffio v1.3.1
    └── child v0.1.0 (group: dev)
        └── iniconfig v2.0.0

    ----- stderr -----
    Resolved 6 packages in [TIME]
    "###
    );

    // Under `--no-dev`, the member should still be included, since we show the entire workspace.
    // But it shouldn't be considered a dependency of the root.
    uv_snapshot!(context.filters(), context.tree().arg("--universal").arg("--no-dev"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    child v0.1.0
    └── iniconfig v2.0.0
    project v0.1.0
    └── anyio v4.3.0
        ├── idna v3.6
        └── sniffio v1.3.1

    ----- stderr -----
    Resolved 6 packages in [TIME]
    "###
    );

    // `uv tree` should update the lockfile
    let lock = context.read("uv.lock");
    assert!(!lock.is_empty());

    Ok(())
}
