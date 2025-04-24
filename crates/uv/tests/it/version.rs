use anyhow::{Ok, Result};
use assert_fs::prelude::*;
use insta::assert_snapshot;

use crate::common::uv_snapshot;
use crate::common::TestContext;

// Print the version
#[test]
fn version_get() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "myproject"
        version = "1.10.31"
        requires-python = ">=3.12"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.version(), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    myproject 1.10.31

    ----- stderr -----
    ");

    let pyproject = fs_err::read_to_string(&pyproject_toml)?;
    assert_snapshot!(
        pyproject,
    @r#"
    [project]
    name = "myproject"
    version = "1.10.31"
    requires-python = ">=3.12"
    "#
    );
    Ok(())
}

// Print the version (json format)
#[test]
fn version_get_json() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "myproject"
        version = "1.10.31"
        requires-python = ">=3.12"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.version()
        .arg("--output-format").arg("json"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "package_name": "myproject",
      "version": "1.10.31",
      "commit_info": null
    }

    ----- stderr -----
    "#);

    let pyproject = fs_err::read_to_string(&pyproject_toml)?;
    assert_snapshot!(
        pyproject,
    @r#"
    [project]
    name = "myproject"
    version = "1.10.31"
    requires-python = ">=3.12"
    "#
    );
    Ok(())
}

// Print the version (--short)
#[test]
fn version_get_short() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "myproject"
        version = "1.10.31"
        requires-python = ">=3.12"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.version()
        .arg("--short"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    1.10.31

    ----- stderr -----
    ");

    let pyproject = fs_err::read_to_string(&pyproject_toml)?;
    assert_snapshot!(
        pyproject,
    @r#"
    [project]
    name = "myproject"
    version = "1.10.31"
    requires-python = ">=3.12"
    "#
    );

    Ok(())
}

// Set the version
#[test]
fn version_set_value() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
[project]
name = "myproject"
version = "1.10.31"
requires-python = ">=3.12"
"#,
    )?;

    uv_snapshot!(context.filters(), context.version()
        .arg("1.1.1"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    myproject 1.10.31 => 1.1.1

    ----- stderr -----
    ");

    let pyproject = fs_err::read_to_string(&pyproject_toml)?;
    assert_snapshot!(
        pyproject,
    @r###"
    [project]
    name = "myproject"
    version = "1.1.1"
    requires-python = ">=3.12"
    "###
    );

    Ok(())
}

// Set the version (--short)
#[test]
fn version_set_value_short() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
[project]
name = "myproject"
version = "1.10.31"
requires-python = ">=3.12"
"#,
    )?;

    uv_snapshot!(context.filters(), context.version()
        .arg("1.1.1")
        .arg("--short"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    1.1.1

    ----- stderr -----
    ");

    let pyproject = fs_err::read_to_string(&pyproject_toml)?;
    assert_snapshot!(
        pyproject,
    @r###"
    [project]
    name = "myproject"
    version = "1.1.1"
    requires-python = ">=3.12"
    "###
    );

    Ok(())
}

// Bump patch version
#[test]
fn version_bump_patch() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
[project]
name = "myproject"
version = "1.10.31"
requires-python = ">=3.12"
"#,
    )?;

    uv_snapshot!(context.filters(), context.version()
        .arg("--bump").arg("patch"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    myproject 1.10.31 => 1.10.32

    ----- stderr -----
    ");

    let pyproject = fs_err::read_to_string(&pyproject_toml)?;
    assert_snapshot!(
        pyproject,
    @r#"
    [project]
    name = "myproject"
    version = "1.10.32"
    requires-python = ">=3.12"
    "#
    );
    Ok(())
}

// Bump patch version (--short)
#[test]
fn version_bump_patch_short() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
[project]
name = "myproject"
version = "1.10.31"
requires-python = ">=3.12"
"#,
    )?;

    uv_snapshot!(context.filters(), context.version()
        .arg("--bump").arg("patch")
        .arg("--short"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    1.10.32

    ----- stderr -----
    ");

    let pyproject = fs_err::read_to_string(&pyproject_toml)?;
    assert_snapshot!(
        pyproject,
    @r#"
    [project]
    name = "myproject"
    version = "1.10.32"
    requires-python = ">=3.12"
    "#
    );
    Ok(())
}

// Bump minor version
#[test]
fn version_bump_minor() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
[project]
name = "myproject"
version = "1.10.31"
requires-python = ">=3.12"
"#,
    )?;

    uv_snapshot!(context.filters(), context.version()
        .arg("--bump").arg("minor"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    myproject 1.10.31 => 1.11.0

    ----- stderr -----
    ");

    let pyproject = fs_err::read_to_string(&pyproject_toml)?;
    assert_snapshot!(
        pyproject,
    @r#"
    [project]
    name = "myproject"
    version = "1.11.0"
    requires-python = ">=3.12"
    "#
    );
    Ok(())
}

// bump major version
#[test]
fn version_major_version() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
[project]
name = "myproject"
version = "1.10.31"
requires-python = ">=3.12"
"#,
    )?;

    uv_snapshot!(context.filters(), context.version()
        .arg("--bump").arg("major"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    myproject 1.10.31 => 2.0.0

    ----- stderr -----
    ");

    let pyproject = fs_err::read_to_string(&pyproject_toml)?;
    assert_snapshot!(
        pyproject,
    @r#"
    [project]
    name = "myproject"
    version = "2.0.0"
    requires-python = ">=3.12"
    "#
    );
    Ok(())
}

// Bump patch but the input version is missing a component
#[test]
fn version_patch_uncompleted() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
[project]
name = "myproject"
version = "0.1"
requires-python = ">=3.12"
"#,
    )?;

    uv_snapshot!(context.filters(), context.version()
        .arg("--bump").arg("patch"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    myproject 0.1 => 0.1.1

    ----- stderr -----
    ");

    let pyproject = fs_err::read_to_string(&pyproject_toml)?;
    assert_snapshot!(
        pyproject,
    @r#"
    [project]
    name = "myproject"
    version = "0.1.1"
    requires-python = ">=3.12"
    "#
    );
    Ok(())
}

// Bump minor but the input version is missing a component
#[test]
fn version_minor_uncompleted() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
[project]
name = "myproject"
version = "0.1"
requires-python = ">=3.12"
"#,
    )?;

    uv_snapshot!(context.filters(), context.version()
        .arg("--bump").arg("minor"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    myproject 0.1 => 0.2

    ----- stderr -----
    ");

    let pyproject = fs_err::read_to_string(&pyproject_toml)?;
    assert_snapshot!(
        pyproject,
    @r#"
    [project]
    name = "myproject"
    version = "0.2"
    requires-python = ">=3.12"
    "#
    );
    Ok(())
}

// Bump major but the input version is missing a component
#[test]
fn version_major_uncompleted() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
[project]
name = "myproject"
version = "0.1"
requires-python = ">=3.12"
"#,
    )?;

    uv_snapshot!(context.filters(), context.version()
        .arg("--bump").arg("major"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    myproject 0.1 => 1.0

    ----- stderr -----
    ");

    let pyproject = fs_err::read_to_string(&pyproject_toml)?;
    assert_snapshot!(
        pyproject,
    @r#"
    [project]
    name = "myproject"
    version = "1.0"
    requires-python = ">=3.12"
    "#
    );
    Ok(())
}

// Bump major but the input version is .dev
#[test]
fn version_major_dev() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
[project]
name = "myproject"
version = "1.10.31.dev10"
requires-python = ">=3.12"
"#,
    )?;

    uv_snapshot!(context.filters(), context.version()
        .arg("--bump").arg("major"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    myproject 1.10.31.dev10 => 2.0.0

    ----- stderr -----
    warning: prerelease information will be cleared as part of the version bump
    ");

    let pyproject = fs_err::read_to_string(&pyproject_toml)?;
    assert_snapshot!(
        pyproject,
    @r#"
    [project]
    name = "myproject"
    version = "2.0.0"
    requires-python = ">=3.12"
    "#
    );
    Ok(())
}

// Bump major but the input version is a complex mess
#[test]
fn version_major_complex_mess() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
[project]
name = "myproject"
version = "1!2a3.post4.dev5+deadbeef6"
requires-python = ">=3.12"
"#,
    )?;

    uv_snapshot!(context.filters(), context.version()
        .arg("--bump").arg("major"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    myproject 1!2a3.post4.dev5+deadbeef6 => 3

    ----- stderr -----
    warning: prerelease information will be cleared as part of the version bump
    ");

    let pyproject = fs_err::read_to_string(&pyproject_toml)?;
    assert_snapshot!(
        pyproject,
    @r#"
    [project]
    name = "myproject"
    version = "3"
    requires-python = ">=3.12"
    "#
    );
    Ok(())
}

// Bump major but the input version is .post
#[test]
fn version_major_post() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
[project]
name = "myproject"
version = "1.10.31.post10"
requires-python = ">=3.12"
"#,
    )?;

    uv_snapshot!(context.filters(), context.version()
        .arg("--bump").arg("major"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    myproject 1.10.31.post10 => 2.0.0

    ----- stderr -----
    warning: prerelease information will be cleared as part of the version bump
    ");

    let pyproject = fs_err::read_to_string(&pyproject_toml)?;
    assert_snapshot!(
        pyproject,
    @r#"
    [project]
    name = "myproject"
    version = "2.0.0"
    requires-python = ">=3.12"
    "#
    );
    Ok(())
}

// Set version --dry-run
#[test]
fn version_set_dry() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
[project]
name = "myproject"
version = "1.10.31"
requires-python = ">=3.12"
"#,
    )?;

    uv_snapshot!(context.filters(), context.version()
        .arg("1.2.3")
        .arg("--dry-run"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    myproject 1.10.31 => 1.2.3

    ----- stderr -----
    ");

    let pyproject = fs_err::read_to_string(&pyproject_toml)?;
    assert_snapshot!(
        pyproject,
    @r#"
    [project]
    name = "myproject"
    version = "1.10.31"
    requires-python = ">=3.12"
    "#
    );
    Ok(())
}

// Bump version --dry-run
#[test]
fn version_major_dry() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
[project]
name = "myproject"
version = "1.10.31"
requires-python = ">=3.12"
"#,
    )?;

    uv_snapshot!(context.filters(), context.version()
        .arg("--bump").arg("major")
        .arg("--dry-run"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    myproject 1.10.31 => 2.0.0

    ----- stderr -----
    ");

    let pyproject = fs_err::read_to_string(&pyproject_toml)?;
    assert_snapshot!(
        pyproject,
    @r#"
    [project]
    name = "myproject"
    version = "1.10.31"
    requires-python = ">=3.12"
    "#
    );
    Ok(())
}

// Set version invalid
#[test]
fn version_set_invalid() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
[project]
name = "myproject"
version = "1.10.31"
requires-python = ">=3.12"
"#,
    )?;

    uv_snapshot!(context.filters(), context.version()
        .arg("abcd"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: expected version to start with a number, but no leading ASCII digits were found
    ");

    let pyproject = fs_err::read_to_string(&pyproject_toml)?;
    assert_snapshot!(
        pyproject,
    @r#"
    [project]
    name = "myproject"
    version = "1.10.31"
    requires-python = ">=3.12"
    "#
    );
    Ok(())
}

// forget --bump but pass a valid bump name
#[test]
fn version_missing_bump() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
[project]
name = "myproject"
version = "1.10.31"
requires-python = ">=3.12"
"#,
    )?;

    uv_snapshot!(context.filters(), context.version()
        .arg("minor"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Invalid version `minor`, did you mean to pass `--bump minor`?
    ");

    let pyproject = fs_err::read_to_string(&pyproject_toml)?;
    assert_snapshot!(
        pyproject,
    @r#"
    [project]
    name = "myproject"
    version = "1.10.31"
    requires-python = ">=3.12"
    "#
    );
    Ok(())
}

// Dynamic version should error on read
#[test]
fn version_get_dynamic() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "myproject"
        dynamic = ["version"]
        requires-python = ">=3.12"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.version(), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: We cannot get or set dynamic project versions in: pyproject.toml
    ");

    let pyproject = fs_err::read_to_string(&pyproject_toml)?;
    assert_snapshot!(
        pyproject,
    @r#"
    [project]
    name = "myproject"
    dynamic = ["version"]
    requires-python = ">=3.12"
    "#
    );
    Ok(())
}

// Dynamic version should error on write
#[test]
fn version_set_dynamic() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "myproject"
        dynamic = ["version"]
        requires-python = ">=3.12"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.version()
        .arg("0.1.2"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: We cannot get or set dynamic project versions in: pyproject.toml
    ");

    let pyproject = fs_err::read_to_string(&pyproject_toml)?;
    assert_snapshot!(
        pyproject,
    @r#"
    [project]
    name = "myproject"
    dynamic = ["version"]
    requires-python = ">=3.12"
    "#
    );
    Ok(())
}

// Should fallback to `uv --version` if this pyproject.toml isn't usable for whatever reason
// (In this case, because tool.uv.managed = false)
#[test]
fn version_get_fallback_unmanaged() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "myapp"
        version = "0.1.2"
        
        [tool.uv]
        managed = false
        "#,
    )?;

    uv_snapshot!(context.filters(), context.version(), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    uv [VERSION] ([COMMIT] DATE)

    ----- stderr -----
    warning: failed to read project: The project is marked as unmanaged: `[TEMP_DIR]/`
      running `uv self version` for compatibility with old `uv version` command.
      this fallback will be removed soon, pass `--preview` to make this an error.
    ");

    let pyproject = fs_err::read_to_string(&pyproject_toml)?;
    assert_snapshot!(
        pyproject,
    @r#"
    [project]
    name = "myapp"
    version = "0.1.2"

    [tool.uv]
    managed = false
    "#
    );
    Ok(())
}

// version_get_fallback with `--short`
#[test]
fn version_get_fallback_unmanaged_short() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "myapp"
        version = "0.1.2"
        
        [tool.uv]
        managed = false
        "#,
    )?;

    let filters = context
        .filters()
        .into_iter()
        .chain([(
            r"\d+\.\d+\.\d+(\+\d+)?( \(.*\))?",
            r"[VERSION] ([COMMIT] DATE)",
        )])
        .collect::<Vec<_>>();
    uv_snapshot!(filters, context.version()
        .arg("--short"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    [VERSION] ([COMMIT] DATE)

    ----- stderr -----
    warning: failed to read project: The project is marked as unmanaged: `[TEMP_DIR]/`
      running `uv self version` for compatibility with old `uv version` command.
      this fallback will be removed soon, pass `--preview` to make this an error.
    ");

    let pyproject = fs_err::read_to_string(&pyproject_toml)?;
    assert_snapshot!(
        pyproject,
    @r#"
    [project]
    name = "myapp"
    version = "0.1.2"

    [tool.uv]
    managed = false
    "#
    );
    Ok(())
}

// version_get_fallback with `--json`
#[test]
fn version_get_fallback_unmanaged_json() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "myapp"
        version = "0.1.2"
        
        [tool.uv]
        managed = false
        "#,
    )?;

    let filters = context
        .filters()
        .into_iter()
        .chain([
            (r#"version": "\d+.\d+.\d+""#, r#"version": "[VERSION]""#),
            (
                r#"short_commit_hash": ".*""#,
                r#"short_commit_hash": "[HASH]""#,
            ),
            (r#"commit_hash": ".*""#, r#"commit_hash": "[LONGHASH]""#),
            (r#"commit_date": ".*""#, r#"commit_date": "[DATE]""#),
            (r#"last_tag": (".*"|null)"#, r#"last_tag": "[TAG]""#),
            (
                r#"commits_since_last_tag": .*"#,
                r#"commits_since_last_tag": [COUNT]"#,
            ),
        ])
        .collect::<Vec<_>>();
    uv_snapshot!(filters, context.version()
        .arg("--output-format").arg("json"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "package_name": "uv",
      "version": "[VERSION]",
      "commit_info": {
        "short_commit_hash": "[LONGHASH]",
        "commit_hash": "[LONGHASH]",
        "commit_date": "[DATE]",
        "last_tag": "[TAG]",
        "commits_since_last_tag": [COUNT]
      }
    }

    ----- stderr -----
    warning: failed to read project: The project is marked as unmanaged: `[TEMP_DIR]/`
      running `uv self version` for compatibility with old `uv version` command.
      this fallback will be removed soon, pass `--preview` to make this an error.
    "#);

    let pyproject = fs_err::read_to_string(&pyproject_toml)?;
    assert_snapshot!(
        pyproject,
    @r#"
    [project]
    name = "myapp"
    version = "0.1.2"

    [tool.uv]
    managed = false
    "#
    );
    Ok(())
}

// Should error if this pyproject.toml isn't usable for whatever reason
// and --project was passed explicitly.
#[test]
fn version_get_fallback_unmanaged_strict() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "myapp"
        version = "0.1.2"
        
        [tool.uv]
        managed = false
        "#,
    )?;

    uv_snapshot!(context.filters(), context.version()
        .arg("--project").arg("."), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: The project is marked as unmanaged: `[TEMP_DIR]/`
    ");

    let pyproject = fs_err::read_to_string(&pyproject_toml)?;
    assert_snapshot!(
        pyproject,
    @r#"
    [project]
    name = "myapp"
    version = "0.1.2"

    [tool.uv]
    managed = false
    "#
    );
    Ok(())
}

// Should error if this pyproject.toml is missing
// and --project was passed explicitly.
#[test]
fn version_get_fallback_missing_strict() -> Result<()> {
    let context = TestContext::new("3.12");

    uv_snapshot!(context.filters(), context.version()
        .arg("--project").arg("."), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No `pyproject.toml` found in current directory or any parent directory
    ");

    Ok(())
}

// Should error if this pyproject.toml is missing
// and --preview was passed explicitly.
#[test]
fn version_get_fallback_missing_strict_preview() -> Result<()> {
    let context = TestContext::new("3.12");

    uv_snapshot!(context.filters(), context.version()
        .arg("--preview"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No `pyproject.toml` found in current directory or any parent directory
    ");

    Ok(())
}

// `uv self version`
// (also setup a honeypot project and make sure it's not used)
#[test]
fn self_version() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "myapp"
        version = "0.1.2"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.self_version(), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    uv [VERSION] ([COMMIT] DATE)

    ----- stderr -----
    ");

    let pyproject = fs_err::read_to_string(&pyproject_toml)?;
    assert_snapshot!(
        pyproject,
    @r#"
    [project]
    name = "myapp"
    version = "0.1.2"
    "#
    );
    Ok(())
}

// `uv self version --short`
// (also setup a honeypot project and make sure it's not used)
#[test]
fn self_version_short() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "myapp"
        version = "0.1.2"
        "#,
    )?;

    let filters = context
        .filters()
        .into_iter()
        .chain([(
            r"\d+\.\d+\.\d+(\+\d+)?( \(.*\))?",
            r"[VERSION] ([COMMIT] DATE)",
        )])
        .collect::<Vec<_>>();
    uv_snapshot!(filters, context.self_version()
        .arg("--short"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    [VERSION] ([COMMIT] DATE)

    ----- stderr -----
    ");

    let pyproject = fs_err::read_to_string(&pyproject_toml)?;
    assert_snapshot!(
        pyproject,
    @r#"
    [project]
    name = "myapp"
    version = "0.1.2"
    "#
    );
    Ok(())
}

// `uv self version --output-format json`
// (also setup a honeypot project and make sure it's not used)
#[test]
fn self_version_json() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "myapp"
        version = "0.1.2"
        "#,
    )?;

    let filters = context
        .filters()
        .into_iter()
        .chain([
            (r#"version": "\d+.\d+.\d+""#, r#"version": "[VERSION]""#),
            (
                r#"short_commit_hash": ".*""#,
                r#"short_commit_hash": "[HASH]""#,
            ),
            (r#"commit_hash": ".*""#, r#"commit_hash": "[LONGHASH]""#),
            (r#"commit_date": ".*""#, r#"commit_date": "[DATE]""#),
            (r#"last_tag": (".*"|null)"#, r#"last_tag": "[TAG]""#),
            (
                r#"commits_since_last_tag": .*"#,
                r#"commits_since_last_tag": [COUNT]"#,
            ),
        ])
        .collect::<Vec<_>>();
    uv_snapshot!(filters, context.self_version()
        .arg("--output-format").arg("json"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "package_name": "uv",
      "version": "[VERSION]",
      "commit_info": {
        "short_commit_hash": "[LONGHASH]",
        "commit_hash": "[LONGHASH]",
        "commit_date": "[DATE]",
        "last_tag": "[TAG]",
        "commits_since_last_tag": [COUNT]
      }
    }

    ----- stderr -----
    "#);

    let pyproject = fs_err::read_to_string(&pyproject_toml)?;
    assert_snapshot!(
        pyproject,
    @r#"
    [project]
    name = "myapp"
    version = "0.1.2"
    "#
    );
    Ok(())
}
