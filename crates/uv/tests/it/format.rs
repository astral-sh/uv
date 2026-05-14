use anyhow::Result;
use assert_fs::prelude::*;
use indoc::indoc;
use insta::assert_snapshot;

use uv_test::uv_snapshot;

#[test]
fn format_project() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    // Create an unformatted Python file
    let main_py = context.temp_dir.child("main.py");
    main_py.write_str(indoc! {r"
        x    = 1
    "})?;

    uv_snapshot!(context.filters(), context.format(), @"
    success: true
    exit_code: 0
    ----- stdout -----
    1 file reformatted

    ----- stderr -----
    warning: `uv format` is experimental and may change without warning. Pass `--preview-features format` to disable this warning.
    ");

    // Check that the file was formatted
    let formatted_content = fs_err::read_to_string(&main_py)?;
    assert_snapshot!(formatted_content, @"x = 1");

    Ok(())
}

#[test]
fn format_missing_pyproject_toml() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);

    // Create an unformatted Python file
    let main_py = context.temp_dir.child("main.py");
    main_py.write_str(indoc! {r"
        x    = 1
    "})?;

    uv_snapshot!(context.filters(), context.format(), @"
    success: true
    exit_code: 0
    ----- stdout -----
    1 file reformatted

    ----- stderr -----
    warning: `uv format` is experimental and may change without warning. Pass `--preview-features format` to disable this warning.
    ");

    // Check that the file was formatted
    let formatted_content = fs_err::read_to_string(&main_py)?;
    assert_snapshot!(formatted_content, @"x = 1");

    Ok(())
}

#[test]
fn format_missing_project_in_pyproject_toml() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);

    // Create an empty pyproject.toml with no [project] section
    context.temp_dir.child("pyproject.toml");

    // Create an unformatted Python file
    let main_py = context.temp_dir.child("main.py");
    main_py.write_str(indoc! {r"
        x    = 1
    "})?;

    uv_snapshot!(context.filters(), context.format(), @"
    success: true
    exit_code: 0
    ----- stdout -----
    1 file reformatted

    ----- stderr -----
    warning: `uv format` is experimental and may change without warning. Pass `--preview-features format` to disable this warning.
    ");

    // Check that the file was formatted
    let formatted_content = fs_err::read_to_string(&main_py)?;
    assert_snapshot!(formatted_content, @"x = 1");

    Ok(())
}

#[test]
fn format_unmanaged_project() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [tool.uv]
        managed = false
    "#})?;

    // Create an unformatted Python file
    let main_py = context.temp_dir.child("main.py");
    main_py.write_str(indoc! {r"
        x    = 1
    "})?;

    uv_snapshot!(context.filters(), context.format(), @"
    success: true
    exit_code: 0
    ----- stdout -----
    1 file reformatted

    ----- stderr -----
    warning: `uv format` is experimental and may change without warning. Pass `--preview-features format` to disable this warning.
    ");

    // Check that the file was formatted
    let formatted_content = fs_err::read_to_string(&main_py)?;
    assert_snapshot!(formatted_content, @"x = 1");

    Ok(())
}

#[test]
fn format_from_project_root() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    // Create an unformatted Python file
    let main_py = context.temp_dir.child("main.py");
    main_py.write_str(indoc! {r"
        x    = 1
    "})?;

    let subdir = context.temp_dir.child("subdir");
    fs_err::create_dir_all(&subdir)?;

    // Using format from a subdirectory should still run in the project root
    uv_snapshot!(context.filters(), context.format().current_dir(&subdir), @"
    success: true
    exit_code: 0
    ----- stdout -----
    1 file reformatted

    ----- stderr -----
    warning: `uv format` is experimental and may change without warning. Pass `--preview-features format` to disable this warning.
    ");

    // Check that the file was formatted
    let formatted_content = fs_err::read_to_string(&main_py)?;
    assert_snapshot!(formatted_content, @"x = 1");

    Ok(())
}

#[test]
fn format_no_project() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);

    let main_py = context.temp_dir.child("main.py");
    main_py.write_str(indoc! {r"
        x    = 1
    "})?;

    uv_snapshot!(context.filters(), context.format().arg("--no-project"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    1 file reformatted

    ----- stderr -----
    warning: `uv format` is experimental and may change without warning. Pass `--preview-features format` to disable this warning.
    ");

    // Check that the file was formatted
    let formatted_content = fs_err::read_to_string(&main_py)?;
    assert_snapshot!(formatted_content, @"x = 1");

    Ok(())
}

#[test]
fn format_relative_project() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);

    let pyproject_toml = context.temp_dir.child("project").child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    // Create an unformatted Python file in the relative project
    let relative_project_main_py = context.temp_dir.child("project").child("main.py");
    relative_project_main_py.write_str(indoc! {r"
        x    = 1
    "})?;

    // Create another unformatted Python file in the root directory
    let root_main_py = context.temp_dir.child("main.py");
    root_main_py.write_str(indoc! {r"
        x    = 1
    "})?;

    uv_snapshot!(context.filters(), context.format().arg("--project").arg("project"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    1 file reformatted

    ----- stderr -----
    warning: `uv format` is experimental and may change without warning. Pass `--preview-features format` to disable this warning.
    ");

    // Check that the relative project file was formatted
    let relative_project_content = fs_err::read_to_string(&relative_project_main_py)?;
    assert_snapshot!(relative_project_content, @"x = 1");

    // Check that the root file was not formatted
    let root_content = fs_err::read_to_string(&root_main_py)?;
    assert_snapshot!(root_content, @"x    = 1");

    Ok(())
}

#[test]
fn format_fails_malformed_pyproject() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str("malformed pyproject.toml")?;

    // Create an unformatted Python file
    let main_py = context.temp_dir.child("main.py");
    main_py.write_str(indoc! {r"
        x    = 1
    "})?;

    uv_snapshot!(context.filters(), context.format(), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: Failed to parse `pyproject.toml` during settings discovery:
      TOML parse error at line 1, column 11
        |
      1 | malformed pyproject.toml
        |           ^
      key with no value, expected `=`

    warning: `uv format` is experimental and may change without warning. Pass `--preview-features format` to disable this warning.
    error: Failed to parse: `pyproject.toml`
      Caused by: TOML parse error at line 1, column 11
      |
    1 | malformed pyproject.toml
      |           ^
    key with no value, expected `=`
    ");

    // Check that the file is not formatted
    let formatted_content = fs_err::read_to_string(&main_py)?;
    assert_snapshot!(formatted_content, @"x    = 1");

    Ok(())
}

#[test]
fn format_check() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    // Create an unformatted Python file
    let main_py = context.temp_dir.child("main.py");
    main_py.write_str(indoc! {r"
        x    = 1
    "})?;

    uv_snapshot!(context.filters(), context.format().arg("--check"), @"
    success: false
    exit_code: 1
    ----- stdout -----
    Would reformat: main.py
    1 file would be reformatted

    ----- stderr -----
    warning: `uv format` is experimental and may change without warning. Pass `--preview-features format` to disable this warning.
    ");

    // Verify the file wasn't modified
    let content = fs_err::read_to_string(&main_py)?;
    assert_snapshot!(content, @"x    = 1");

    Ok(())
}

#[test]
fn format_diff() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    // Create an unformatted Python file
    let main_py = context.temp_dir.child("main.py");
    main_py.write_str(indoc! {r"
        x    = 1
    "})?;

    uv_snapshot!(context.filters(), context.format().arg("--diff"), @"
    success: false
    exit_code: 1
    ----- stdout -----
    --- main.py
    +++ main.py
    @@ -1 +1 @@
    -x    = 1
    +x = 1


    ----- stderr -----
    warning: `uv format` is experimental and may change without warning. Pass `--preview-features format` to disable this warning.
    1 file would be reformatted
    ");

    // Verify the file wasn't modified
    let content = fs_err::read_to_string(&main_py)?;
    assert_snapshot!(content, @"x    = 1");

    Ok(())
}

#[test]
fn format_with_ruff_args() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    // Create a Python file with a long line
    let main_py = context.temp_dir.child("main.py");
    main_py.write_str(indoc! {r#"
        def hello():
            print("This is a very long line that should normally be wrapped by the formatter but we will configure it to have a longer line length")
    "#})?;

    // Run format with custom line length
    uv_snapshot!(context.filters(), context.format().arg("--").arg("main.py").arg("--line-length").arg("200"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    1 file left unchanged

    ----- stderr -----
    warning: `uv format` is experimental and may change without warning. Pass `--preview-features format` to disable this warning.
    ");

    // Check that the line wasn't wrapped (since we set a long line length)
    let formatted_content = fs_err::read_to_string(&main_py)?;
    assert_snapshot!(formatted_content, @r#"
    def hello():
        print("This is a very long line that should normally be wrapped by the formatter but we will configure it to have a longer line length")
    "#);

    Ok(())
}

#[test]
fn format_specific_files() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    // Create multiple unformatted Python files
    let main_py = context.temp_dir.child("main.py");
    main_py.write_str(indoc! {r"
        x    = 1
    "})?;

    let utils_py = context.temp_dir.child("utils.py");
    utils_py.write_str(indoc! {r"
        x    = 1
    "})?;

    uv_snapshot!(context.filters(), context.format().arg("--").arg("main.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    1 file reformatted

    ----- stderr -----
    warning: `uv format` is experimental and may change without warning. Pass `--preview-features format` to disable this warning.
    ");

    let main_content = fs_err::read_to_string(&main_py)?;
    assert_snapshot!(main_content, @"x = 1");

    // Unchanged
    let utils_content = fs_err::read_to_string(&utils_py)?;
    assert_snapshot!(utils_content, @"x    = 1");

    Ok(())
}

#[test]
fn format_version_option() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.11"
        dependencies = []
    "#})?;

    let main_py = context.temp_dir.child("main.py");
    main_py.write_str(indoc! {r"
        x    = 1
    "})?;

    // Run format with specific Ruff version
    // TODO(zanieb): It'd be nice to assert on the version used here somehow? Maybe we should emit
    // the version we're using to stderr? Alas there's not a way to get the Ruff version from the
    // format command :)
    uv_snapshot!(context.filters(), context.format().arg("--version").arg("0.8.2"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    1 file reformatted

    ----- stderr -----
    warning: `uv format` is experimental and may change without warning. Pass `--preview-features format` to disable this warning.
    ");

    Ok(())
}

#[test]
fn format_version_constraints() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.11"
        dependencies = []
    "#})?;

    let main_py = context.temp_dir.child("main.py");
    main_py.write_str(indoc! {r"
        x    = 1
    "})?;

    // Run format with version constraints - should find the latest version matching >=0.8.0
    uv_snapshot!(context.filters(), context.format().arg("--version").arg(">=0.8.0").arg("--show-version"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    1 file reformatted

    ----- stderr -----
    warning: `uv format` is experimental and may change without warning. Pass `--preview-features format` to disable this warning.
    ruff 0.15.1
    ");

    Ok(())
}

#[test]
fn format_version_latest() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.11"
        dependencies = []
    "#})?;

    let main_py = context.temp_dir.child("main.py");
    main_py.write_str(indoc! {r"
        x    = 1
    "})?;

    // Run format with --version latest - should fetch the latest version from the manifest
    uv_snapshot!(context.filters(), context.format().arg("--version").arg("latest").arg("--show-version"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    1 file reformatted

    ----- stderr -----
    warning: `uv format` is experimental and may change without warning. Pass `--preview-features format` to disable this warning.
    ruff 0.15.1
    ");

    Ok(())
}

#[test]
fn format_exclude_newer() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.11"
        dependencies = []
    "#})?;

    let main_py = context.temp_dir.child("main.py");
    main_py.write_str(indoc! {r"
        x    = 1
    "})?;

    // Run format with a different --exclude-newer value than the default (2025-01-01)
    // This verifies that the flag is respected for version resolution
    uv_snapshot!(context.filters(), context.format().arg("--exclude-newer").arg("2026-01-01").arg("--version").arg("latest").arg("--show-version"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    1 file reformatted

    ----- stderr -----
    warning: `uv format` is experimental and may change without warning. Pass `--preview-features format` to disable this warning.
    ruff 0.14.10
    ");

    Ok(())
}

#[test]
fn format_no_matching_version() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.11"
        dependencies = []
    "#})?;

    let main_py = context.temp_dir.child("main.py");
    main_py.write_str(indoc! {r"
        x    = 1
    "})?;

    // Run format with impossible version constraints - should fail
    let context = context.with_filter((
        r"\b[a-z0-9_]+-(?:apple|pc|unknown)-[a-z0-9_]+(?:-[a-z0-9_]+)?\b",
        "[PLATFORM]",
    ));
    uv_snapshot!(context.filters(), context.format().arg("--version").arg(">=999.0.0"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: `uv format` is experimental and may change without warning. Pass `--preview-features format` to disable this warning.
    error: Failed to find ruff version matching: >=999.0.0
      Caused by: No version of ruff found matching `>=999.0.0` for platform `[PLATFORM]`
    ");

    Ok(())
}
