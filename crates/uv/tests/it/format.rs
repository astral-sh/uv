use anyhow::Result;
use assert_fs::fixture::ChildPath;
use assert_fs::prelude::*;
use indoc::indoc;
use insta::assert_snapshot;
use std::path::Path;

use crate::common::{TestContext, uv_snapshot};

fn create_pyproject_toml<P: AsRef<Path>>(context: &TestContext, path: P) -> Result<()> {
    let pyproject_toml = context.temp_dir.child(path);
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.11"
        dependencies = []
    "#})?;

    Ok(())
}

fn create_unformatted_file(context: &TestContext, filename: &str) -> Result<ChildPath> {
    let py_file = context.temp_dir.child(filename);
    py_file.write_str(indoc! {r#"
        import sys
        def   hello():
            print(  "Hello, World!"  )
        if __name__=="__main__":
            hello(   )
    "#})?;

    Ok(py_file)
}

fn assert_file_formatted<P: AsRef<Path>>(path: P) -> Result<()> {
    let formatted_content = fs_err::read_to_string(path)?;
    assert_snapshot!(formatted_content, @r#"
    import sys


    def hello():
        print("Hello, World!")


    if __name__ == "__main__":
        hello()
    "#);

    Ok(())
}

fn assert_file_not_formatted<P: AsRef<Path>>(path: P) -> Result<()> {
    let formatted_content = fs_err::read_to_string(path)?;
    assert_snapshot!(formatted_content, @r#"
    import sys
    def   hello():
        print(  "Hello, World!"  )
    if __name__=="__main__":
        hello(   )
    "#);

    Ok(())
}

#[test]
fn format_project() -> Result<()> {
    let context = TestContext::new_with_versions(&[]);

    create_pyproject_toml(&context, "pyproject.toml")?;
    let main_py = create_unformatted_file(&context, "main.py")?;

    uv_snapshot!(context.filters(), context.format(), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    1 file reformatted

    ----- stderr -----
    warning: `uv format` is experimental and may change without warning. Pass `--preview-features format` to disable this warning.
    ");

    assert_file_formatted(&main_py)?;

    Ok(())
}

#[test]
fn format_relative_project() -> Result<()> {
    let context = TestContext::new_with_versions(&[]);

    create_pyproject_toml(&context, "project/pyproject.toml")?;
    let relative_project_main_py = create_unformatted_file(&context, "project/main.py")?;
    let root_main_py = create_unformatted_file(&context, "main.py")?;

    uv_snapshot!(context.filters(), context.format().arg("--project").arg("project"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    1 file reformatted

    ----- stderr -----
    warning: `uv format` is experimental and may change without warning. Pass `--preview-features format` to disable this warning.
    ");

    assert_file_formatted(&relative_project_main_py)?;
    assert_file_not_formatted(&root_main_py)?;

    Ok(())
}

#[test]
fn format_check() -> Result<()> {
    let context = TestContext::new_with_versions(&[]);

    create_pyproject_toml(&context, "pyproject.toml")?;
    let main_py = create_unformatted_file(&context, "main.py")?;

    uv_snapshot!(context.filters(), context.format().arg("--check"), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    Would reformat: main.py
    1 file would be reformatted

    ----- stderr -----
    warning: `uv format` is experimental and may change without warning. Pass `--preview-features format` to disable this warning.
    ");

    assert_file_not_formatted(&main_py)?;

    Ok(())
}

#[test]
fn format_diff() -> Result<()> {
    let context = TestContext::new_with_versions(&[]);

    create_pyproject_toml(&context, "pyproject.toml")?;
    let main_py = create_unformatted_file(&context, "main.py")?;

    uv_snapshot!(context.filters(), context.format().arg("--diff"), @r#"
    success: false
    exit_code: 1
    ----- stdout -----
    --- main.py
    +++ main.py
    @@ -1,5 +1,9 @@
     import sys
    -def   hello():
    -    print(  "Hello, World!"  )
    -if __name__=="__main__":
    -    hello(   )
    +
    +
    +def hello():
    +    print("Hello, World!")
    +
    +
    +if __name__ == "__main__":
    +    hello()


    ----- stderr -----
    warning: `uv format` is experimental and may change without warning. Pass `--preview-features format` to disable this warning.
    1 file would be reformatted
    "#);

    assert_file_not_formatted(&main_py)?;

    Ok(())
}

#[test]
fn format_with_ruff_args() -> Result<()> {
    let context = TestContext::new_with_versions(&[]);

    create_pyproject_toml(&context, "pyproject.toml")?;

    // Create a Python file with a long line
    let main_py = context.temp_dir.child("main.py");
    main_py.write_str(indoc! {r#"
        def hello():
            print("This is a very long line that should normally be wrapped by the formatter but we will configure it to have a longer line length")
    "#})?;

    // Run format with custom line length
    uv_snapshot!(context.filters(), context.format().arg("--").arg("main.py").arg("--line-length").arg("200"), @r"
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
    let context = TestContext::new_with_versions(&[]);

    create_pyproject_toml(&context, "pyproject.toml")?;
    let main_py = create_unformatted_file(&context, "main.py")?;
    let utils_py = create_unformatted_file(&context, "utils.py")?;

    uv_snapshot!(context.filters(), context.format().arg("--").arg("main.py"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    1 file reformatted

    ----- stderr -----
    warning: `uv format` is experimental and may change without warning. Pass `--preview-features format` to disable this warning.
    ");

    assert_file_formatted(&main_py)?;
    assert_file_not_formatted(&utils_py)?;

    Ok(())
}

#[test]
fn format_version_option() -> Result<()> {
    let context = TestContext::new_with_versions(&[]);

    create_pyproject_toml(&context, "pyproject.toml")?;
    create_unformatted_file(&context, "main.py")?;

    // Run format with specific Ruff version
    // TODO(zanieb): It'd be nice to assert on the version used here somehow? Maybe we should emit
    // the version we're using to stderr? Alas there's not a way to get the Ruff version from the
    // format command :)
    uv_snapshot!(context.filters(), context.format().arg("--version").arg("0.8.2"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    1 file reformatted

    ----- stderr -----
    warning: `uv format` is experimental and may change without warning. Pass `--preview-features format` to disable this warning.
    ");

    Ok(())
}
