use anyhow::Result;
use assert_fs::prelude::*;
use indoc::indoc;
use insta::assert_snapshot;

use crate::common::{TestContext, uv_snapshot};

#[test]
fn format_project() -> Result<()> {
    let context = TestContext::new("3.12");

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
    main_py.write_str(indoc! {r#"
        import sys
        def   hello():
            print(  "Hello, World!"  )
        if __name__=="__main__":
            hello(   )
    "#})?;

    // Snapshot the original content
    let original_content = std::fs::read_to_string(&main_py)?;
    assert_snapshot!(original_content, @r#"
    import sys
    def   hello():
        print(  "Hello, World!"  )
    if __name__=="__main__":
        hello(   )
    "#);

    // Run format
    uv_snapshot!(context.filters(), context.format().arg("main.py"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    1 file reformatted

    ----- stderr -----
    Installed 1 package in [TIME]
    ");

    // Check that the file was formatted
    let formatted_content = std::fs::read_to_string(&main_py)?;
    assert_snapshot!(formatted_content, @r#"
    import sys


    def hello():
        print("Hello, World!")


    if __name__ == "__main__":
        hello()
    "#);

    Ok(())
}

#[test]
fn format_check() -> Result<()> {
    let context = TestContext::new("3.12");

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
    main_py.write_str(indoc! {r#"
        def   hello():
            print(  "Hello, World!"  )
    "#})?;

    // Run format with --check
    uv_snapshot!(context.filters(), context.format().arg("--check").arg("main.py"), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    Would reformat: main.py
    1 file would be reformatted

    ----- stderr -----
    Installed 1 package in [TIME]
    ");

    // Verify the file wasn't modified
    let content = std::fs::read_to_string(&main_py)?;
    assert_snapshot!(content, @r#"
    def   hello():
        print(  "Hello, World!"  )
    "#);

    Ok(())
}

#[test]
fn format_diff() -> Result<()> {
    let context = TestContext::new("3.12");

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
    main_py.write_str(indoc! {r#"
        def   hello():
            print(  "Hello, World!"  )
    "#})?;

    // Run format with --diff
    uv_snapshot!(context.filters(), context.format().arg("--diff").arg("main.py"), @r#"
    success: false
    exit_code: 1
    ----- stdout -----
    --- main.py
    +++ main.py
    @@ -1,2 +1,2 @@
    -def   hello():
    -    print(  "Hello, World!"  )
    +def hello():
    +    print("Hello, World!")


    ----- stderr -----
    Installed 1 package in [TIME]
    1 file would be reformatted
    "#);

    // Verify the file wasn't modified
    let content = std::fs::read_to_string(&main_py)?;
    assert_snapshot!(content, @r#"
    def   hello():
        print(  "Hello, World!"  )
    "#);

    Ok(())
}

#[test]
fn format_with_args() -> Result<()> {
    let context = TestContext::new("3.12");

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
    uv_snapshot!(context.filters(), context.format().arg("main.py").arg("--").arg("--line-length").arg("200"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    1 file left unchanged

    ----- stderr -----
    Installed 1 package in [TIME]
    ");

    // Check that the line wasn't wrapped (because we set a high line length)
    let formatted_content = std::fs::read_to_string(&main_py)?;
    assert_snapshot!(formatted_content, @r#"
    def hello():
        print("This is a very long line that should normally be wrapped by the formatter but we will configure it to have a longer line length")
    "#);

    Ok(())
}

#[test]
fn format_multiple_files() -> Result<()> {
    let context = TestContext::new("3.12");

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
    main_py.write_str(indoc! {r#"
        def   main():
            print(  "Main"  )
    "#})?;

    let utils_py = context.temp_dir.child("utils.py");
    utils_py.write_str(indoc! {r#"
        def   util():
            return   42
    "#})?;

    // Run format on both files
    uv_snapshot!(context.filters(), context.format().arg("main.py").arg("utils.py"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    2 files reformatted

    ----- stderr -----
    Installed 1 package in [TIME]
    ");

    // Check that both files were formatted
    let main_content = std::fs::read_to_string(&main_py)?;
    assert_snapshot!(main_content, @r#"
    def main():
        print("Main")
    "#);

    let utils_content = std::fs::read_to_string(&utils_py)?;
    assert_snapshot!(utils_content, @r#"
    def util():
        return 42
    "#);

    Ok(())
}

#[test]
fn format_directory() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    // Create subdirectory with Python files
    let src_dir = context.temp_dir.child("src");
    src_dir.create_dir_all()?;

    let module_py = src_dir.child("module.py");
    module_py.write_str(indoc! {r#"
        def   func():
            pass
    "#})?;

    // Run format on directory
    uv_snapshot!(context.filters(), context.format().arg("src/"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    1 file reformatted

    ----- stderr -----
    Installed 1 package in [TIME]
    ");

    // Check that the file in the directory was formatted
    let module_content = std::fs::read_to_string(&module_py)?;
    assert_snapshot!(module_content, @r#"
    def func():
        pass
    "#);

    Ok(())
}

#[test]
fn format_no_files() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    // Create a Python file
    let main_py = context.temp_dir.child("main.py");
    main_py.write_str(indoc! {r#"
        def   hello():
            print(  "Hello"  )
    "#})?;

    // Run format without specifying files (should format current directory)
    uv_snapshot!(context.filters(), context.format(), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    1 file reformatted

    ----- stderr -----
    Installed 1 package in [TIME]
    ");

    // Check that the file was formatted
    let content = std::fs::read_to_string(&main_py)?;
    assert_snapshot!(content, @r#"
    def hello():
        print("Hello")
    "#);

    Ok(())
}

#[test]
fn format_cache_reuse() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    // Create Python file
    let main_py = context.temp_dir.child("main.py");
    main_py.write_str(indoc! {r#"
        def   hello():  pass
    "#})?;

    // First run - installs Ruff
    uv_snapshot!(context.filters(), context.format().arg("main.py"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    1 file reformatted

    ----- stderr -----
    Installed 1 package in [TIME]
    ");

    // Modify the file again
    main_py.write_str(indoc! {r#"
        def   goodbye():  pass
    "#})?;

    // Second run - should reuse cached Ruff
    uv_snapshot!(context.filters(), context.format().arg("main.py"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    1 file reformatted

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn format_python_option() -> Result<()> {
    let context = TestContext::new_with_versions(&["3.11", "3.12"]);

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.11"
        dependencies = []
    "#})?;

    let main_py = context.temp_dir.child("main.py");
    main_py.write_str(indoc! {r#"
        def   hello():  pass
    "#})?;

    // Run format with specific Python version
    uv_snapshot!(context.filters(), context.format().arg("--python").arg("3.11").arg("main.py"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    1 file reformatted

    ----- stderr -----
    Installed 1 package in [TIME]
    ");

    Ok(())
}
