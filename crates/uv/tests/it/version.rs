use anyhow::{Ok, Result};
use assert_cmd::assert::OutputAssertExt;
use assert_fs::prelude::*;
use indoc::indoc;
use insta::assert_snapshot;

use crate::common::TestContext;
use crate::common::uv_snapshot;

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
    Resolved 1 package in [TIME]
    Audited in [TIME]
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
    Resolved 1 package in [TIME]
    Audited in [TIME]
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
    Resolved 1 package in [TIME]
    Audited in [TIME]
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
    Resolved 1 package in [TIME]
    Audited in [TIME]
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
    Resolved 1 package in [TIME]
    Audited in [TIME]
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
    Resolved 1 package in [TIME]
    Audited in [TIME]
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
    Resolved 1 package in [TIME]
    Audited in [TIME]
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
    Resolved 1 package in [TIME]
    Audited in [TIME]
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
    Resolved 1 package in [TIME]
    Audited in [TIME]
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
    Resolved 1 package in [TIME]
    Audited in [TIME]
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
    Resolved 1 package in [TIME]
    Audited in [TIME]
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
    Resolved 1 package in [TIME]
    Audited in [TIME]
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
    warning: Failed to read project metadata (The project is marked as unmanaged: `[TEMP_DIR]/`). Running `uv self version` for compatibility. This fallback will be removed in the future; pass `--preview` to force an error.
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
    warning: Failed to read project metadata (The project is marked as unmanaged: `[TEMP_DIR]/`). Running `uv self version` for compatibility. This fallback will be removed in the future; pass `--preview` to force an error.
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

/// In tarball builds of uv, git version info is missing (distros do this)
fn git_version_info_expected() -> bool {
    // This is setup to aggressively panic to make sure this is working at all
    // If you're a packager of uv and this does indeed blow up for you, we will
    // gladly change these expects into "just return false" or something.
    let manifest_dir = std::env::var(uv_static::EnvVars::CARGO_MANIFEST_DIR)
        .expect("CARGO_MANIFEST_DIR not defined");
    let git_dir = std::path::Path::new(&manifest_dir)
        .parent()
        .expect("parent of manifest dir missing")
        .parent()
        .expect("grandparent of manifest dir missing")
        .join(".git");
    git_dir.exists()
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
    if git_version_info_expected() {
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
      warning: Failed to read project metadata (The project is marked as unmanaged: `[TEMP_DIR]/`). Running `uv self version` for compatibility. This fallback will be removed in the future; pass `--preview` to force an error.
      "#);
    } else {
        uv_snapshot!(filters, context.version()
          .arg("--output-format").arg("json"), @r#"
      success: true
      exit_code: 0
      ----- stdout -----
      {
        "package_name": "uv",
        "version": "[VERSION]",
        "commit_info": null
      }

      ----- stderr -----
      warning: Failed to read project metadata (The project is marked as unmanaged: `[TEMP_DIR]/`). Running `uv self version` for compatibility. This fallback will be removed in the future; pass `--preview` to force an error.
      "#);
    }

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

    if git_version_info_expected() {
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
    } else {
        uv_snapshot!(filters, context.self_version()
          .arg("--output-format").arg("json"), @r#"
      success: true
      exit_code: 0
      ----- stdout -----
      {
        "package_name": "uv",
        "version": "[VERSION]",
        "commit_info": null
      }

      ----- stderr -----
      "#);
    }

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

// Ensure that the global `--project` option is respected.
#[test]
fn version_get_workspace() -> Result<()> {
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

    context
        .init()
        .arg("--lib")
        .arg("workspace-member")
        .assert()
        .success();

    uv_snapshot!(context.filters(), context.version().arg("--project").arg(context.temp_dir.as_ref()), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    myproject 1.10.31

    ----- stderr -----
    ");

    uv_snapshot!(context.filters(), context.version().arg("--project").arg(context.temp_dir.join("workspace-member")), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    workspace-member 0.1.0

    ----- stderr -----
    ");

    // Check that --directory also works
    uv_snapshot!(context.filters(), context.version().arg("--directory").arg(context.temp_dir.as_ref()), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    myproject 1.10.31

    ----- stderr -----
    ");

    uv_snapshot!(context.filters(), context.version().arg("--directory").arg(context.temp_dir.join("workspace-member")), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    workspace-member 0.1.0

    ----- stderr -----
    ");

    pyproject_toml.write_str(
        r#"
        [tool.uv.workspace]
        members = ["workspace-member"]
        "#,
    )?;

    // A virtual project root has a no version.
    // TODO(konsti): Show a dedicated error message for virtual workspace roots (generally, not
    // only for `uv version`)
    uv_snapshot!(context.filters(), context.version().arg("--project").arg(context.temp_dir.as_ref()), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Missing `project.name` field in: pyproject.toml
    ");

    Ok(())
}

/// Edit the version of a workspace member
///
/// Also check that --locked/--frozen/--no-sync do what they say
#[test]
#[cfg(feature = "pypi")]
fn version_set_workspace() -> Result<()> {
    let context = TestContext::new("3.12");

    let workspace = context.temp_dir.child("pyproject.toml");
    workspace.write_str(indoc! {r#"
        [tool.uv.workspace]
        members = ["child1", "child2"]
    "#})?;

    let pyproject_toml = context.temp_dir.child("child1/pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "child1"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "child2",
        ]

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"

        [tool.uv.sources]
        child2 = { workspace = true }
    "#})?;
    context
        .temp_dir
        .child("child1")
        .child("src")
        .child("child1")
        .child("__init__.py")
        .touch()?;

    let pyproject_toml = context.temp_dir.child("child2/pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "child2"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
    "#})?;
    context
        .temp_dir
        .child("child2")
        .child("src")
        .child("child2")
        .child("__init__.py")
        .touch()?;

    // Set one child's version, creating the lock and initial sync
    let mut version_cmd = context.version();
    version_cmd
        .arg("--package")
        .arg("child2")
        .arg("1.1.1")
        .current_dir(&context.temp_dir);

    uv_snapshot!(context.filters(), version_cmd, @r"
    success: true
    exit_code: 0
    ----- stdout -----
    child2 0.1.0 => 1.1.1

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + child2==1.1.1 (from file://[TEMP_DIR]/child2)
    ");

    // `uv version` implies a full lock and sync, including development dependencies.
    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 2
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        members = [
            "child1",
            "child2",
        ]

        [[package]]
        name = "child1"
        version = "0.1.0"
        source = { editable = "child1" }
        dependencies = [
            { name = "child2" },
        ]

        [package.metadata]
        requires-dist = [{ name = "child2", editable = "child2" }]

        [[package]]
        name = "child2"
        version = "1.1.1"
        source = { editable = "child2" }
        "#
        );
    });

    // Set the other child's version, refereshing the lock and sync
    let mut version_cmd = context.version();
    version_cmd
        .arg("--package")
        .arg("child1")
        .arg("1.2.3")
        .current_dir(&context.temp_dir);

    uv_snapshot!(context.filters(), version_cmd, @r"
    success: true
    exit_code: 0
    ----- stdout -----
    child1 0.1.0 => 1.2.3

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + child1==1.2.3 (from file://[TEMP_DIR]/child1)
    ");

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 2
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        members = [
            "child1",
            "child2",
        ]

        [[package]]
        name = "child1"
        version = "1.2.3"
        source = { editable = "child1" }
        dependencies = [
            { name = "child2" },
        ]

        [package.metadata]
        requires-dist = [{ name = "child2", editable = "child2" }]

        [[package]]
        name = "child2"
        version = "1.1.1"
        source = { editable = "child2" }
        "#
        );
    });

    // Confirm --locked get works fine
    uv_snapshot!(context.filters(), context.version()
        .arg("--package").arg("child1")
        .arg("--locked"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    child1 1.2.3

    ----- stderr -----
    ");

    // Confirm --frozen get works fine
    uv_snapshot!(context.filters(), context.version()
        .arg("--package").arg("child2")
        .arg("--frozen"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    child2 1.1.1

    ----- stderr -----
    ");

    // Confirm --no-sync get works fine
    uv_snapshot!(context.filters(), context.version()
        .arg("--package").arg("child1")
        .arg("--no-sync"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    child1 1.2.3

    ----- stderr -----
    ");

    // Confirm --frozen set works
    uv_snapshot!(context.filters(), context.version()
        .arg("--package").arg("child2")
        .arg("--frozen")
        .arg("2.0.0"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    child2 1.1.1 => 2.0.0

    ----- stderr -----
    ");

    // Confirm --frozen --bump works, sees the previous set
    uv_snapshot!(context.filters(), context.version()
        .arg("--package").arg("child2")
        .arg("--frozen")
        .arg("--bump").arg("patch"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    child2 2.0.0 => 2.0.1

    ----- stderr -----
    ");

    // Confirm --frozen get doesn't see the --frozen set or bump
    uv_snapshot!(context.filters(), context.version()
        .arg("--package").arg("child2")
        .arg("--frozen"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    child2 1.1.1

    ----- stderr -----
    ");

    // Confirm --no-sync set does a lock but no sync
    uv_snapshot!(context.filters(), context.version()
        .arg("--package").arg("child1")
        .arg("--no-sync")
        .arg("3.0.0"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    child1 1.2.3 => 3.0.0

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
        revision = 2
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        members = [
            "child1",
            "child2",
        ]

        [[package]]
        name = "child1"
        version = "3.0.0"
        source = { editable = "child1" }
        dependencies = [
            { name = "child2" },
        ]

        [package.metadata]
        requires-dist = [{ name = "child2", editable = "child2" }]

        [[package]]
        name = "child2"
        version = "2.0.1"
        source = { editable = "child2" }
        "#
        );
    });

    // Confirm --locked set works if it's a noop (can sync)
    uv_snapshot!(context.filters(), context.version()
        .arg("--package").arg("child1")
        .arg("--locked")
        .arg("3.0.0"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    child1 3.0.0 => 3.0.0

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Uninstalled 2 packages in [TIME]
    Installed 2 packages in [TIME]
     - child1==1.2.3 (from file://[TEMP_DIR]/child1)
     + child1==3.0.0 (from file://[TEMP_DIR]/child1)
     - child2==1.1.1 (from file://[TEMP_DIR]/child2)
     + child2==2.0.1 (from file://[TEMP_DIR]/child2)
    ");
    Ok(())
}

/// Edit the version of a workspace member in a way that breaks a version
/// constraint, forcing the lockfile to be updated non-trivially.
///
/// The idea here is that:
///
/// * "myproj" depends on the registry package "anyio"
/// * "anyio" depends on the registry package "idna"
/// * our workspace defines "idna", forcing "anyio" to use whatever version we have
///
/// The result is that `uv version --package idna x.y.z` can force the re-evaluation
/// of the version of "anyio" we select. In particular we *shrink* the version of "idna",
/// forcing "anyio" to massively revert all the way back to before it depended on "idna".
///
/// It would be nice to have a case where we still get a package dependency, but
/// this still demonstrates the non-trivial "hazard" of a version change.
#[test]
#[cfg(feature = "pypi")]
fn version_set_evil_constraints() -> Result<()> {
    let context = TestContext::new("3.12");

    let workspace = context.temp_dir.child("pyproject.toml");
    workspace.write_str(indoc! {r#"
        [tool.uv.workspace]
        members = ["idna", "myproj"]
    "#})?;

    let pyproject_toml = context.temp_dir.child("idna/pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "idna"
        version = "3.10.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
    "#})?;
    context
        .temp_dir
        .child("idna")
        .child("src")
        .child("idna")
        .child("__init__.py")
        .touch()?;

    let pyproject_toml = context.temp_dir.child("myproj/pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "myproj"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "anyio",
        ]

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
    "#})?;
    context
        .temp_dir
        .child("myproj")
        .child("src")
        .child("myproj")
        .child("__init__.py")
        .touch()?;

    // sync all, creating the lock and initial sync
    uv_snapshot!(context.filters(),  context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.3.0
     + idna==3.10.0 (from file://[TEMP_DIR]/idna)
     + myproj==0.1.0 (from file://[TEMP_DIR]/myproj)
     + sniffio==1.3.1
    ");

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 2
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        members = [
            "idna",
            "myproj",
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
        version = "3.10.0"
        source = { editable = "idna" }

        [[package]]
        name = "myproj"
        version = "0.1.0"
        source = { editable = "myproj" }
        dependencies = [
            { name = "anyio" },
        ]

        [package.metadata]
        requires-dist = [{ name = "anyio" }]

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

    // Reduce idna's version, forcing a downgrade of anyio (used by myproj)
    // This will not appear in the sync, but it will show up in the lock,
    // because we use "sufficient" sync semantics
    let mut version_cmd = context.version();
    version_cmd
        .arg("--project")
        .arg("idna")
        .arg("2.0.0")
        .current_dir(&context.temp_dir);

    uv_snapshot!(context.filters(), version_cmd, @r"
    success: true
    exit_code: 0
    ----- stdout -----
    idna 3.10.0 => 2.0.0

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - idna==3.10.0 (from file://[TEMP_DIR]/idna)
     + idna==2.0.0 (from file://[TEMP_DIR]/idna)
    ");

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 2
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        members = [
            "idna",
            "myproj",
        ]

        [[package]]
        name = "anyio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "async-generator" },
            { name = "sniffio" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/44/eb/c5f29a8c854cf454cb995dc791152c641eb8948b2d71cb30e233eb262c53/anyio-1.3.1.tar.gz", hash = "sha256:a46bb2b7743455434afd9adea848a3c4e0b7321aee3e9d08844b11d348d3b5a0", size = 56763, upload-time = "2020-05-31T11:50:54.61Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ab/c2/17b5c64a1a92c5dbd7ab3fd24c4db4332aa78ebe132e54136d1bc5eb1bd5/anyio-1.3.1-py3-none-any.whl", hash = "sha256:f21b4fafeec1b7db81e09a907e44e374a1e39718d782a488fdfcdcf949c8950c", size = 35056, upload-time = "2020-05-31T11:50:53.646Z" },
        ]

        [[package]]
        name = "async-generator"
        version = "1.10"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ce/b6/6fa6b3b598a03cba5e80f829e0dadbb49d7645f523d209b2fb7ea0bbb02a/async_generator-1.10.tar.gz", hash = "sha256:6ebb3d106c12920aaae42ccb6f787ef5eefdcdd166ea3d628fa8476abe712144", size = 29870, upload-time = "2018-08-01T03:36:21.69Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/71/52/39d20e03abd0ac9159c162ec24b93fbcaa111e8400308f2465432495ca2b/async_generator-1.10-py3-none-any.whl", hash = "sha256:01c7bf666359b4967d2cda0000cc2e4af16a0ae098cbffcb8472fb9e8ad6585b", size = 18857, upload-time = "2018-08-01T03:36:20.029Z" },
        ]

        [[package]]
        name = "idna"
        version = "2.0.0"
        source = { editable = "idna" }

        [[package]]
        name = "myproj"
        version = "0.1.0"
        source = { editable = "myproj" }
        dependencies = [
            { name = "anyio" },
        ]

        [package.metadata]
        requires-dist = [{ name = "anyio" }]

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

    // however once we explicitly sync the change will go into effect
    uv_snapshot!(context.filters(),  context.sync(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Prepared 2 packages in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 2 packages in [TIME]
     - anyio==4.3.0
     + anyio==1.3.1
     + async-generator==1.10
    ");

    Ok(())
}
