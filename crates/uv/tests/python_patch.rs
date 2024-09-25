use assert_fs::fixture::{FileWriteStr, PathChild};

use anyhow::Result;
use common::{uv_snapshot, TestContext};
use insta::assert_snapshot;
use uv_python::{PYTHON_VERSIONS_FILENAME, PYTHON_VERSION_FILENAME};

mod common;

#[test]
fn python_patch_version() -> Result<()> {
    let context = TestContext::new("3.8.12");

    let python_version_file = context.temp_dir.child(PYTHON_VERSION_FILENAME);
    python_version_file.write_str(r"3.8.12")?;
    uv_snapshot!(context.filters(), context.python_patch(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Bumped patch for version 3.8.12 to 20

    ----- stderr -----
    "###);

    Ok(())
}

#[test]
fn python_patch_versions() -> Result<()> {
    let context = TestContext::new_with_versions(&["3.8.12", "3.9.18", "3.10.13"]);

    let python_version_file = context.temp_dir.child(PYTHON_VERSIONS_FILENAME);
    python_version_file.write_str(
        r"3.8.12
          3.8.18
          3.10.13
          3.9.18
    ",
    )?;
    uv_snapshot!(context.filters(), context.python_patch(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Bumped patch for version 3.8.18 to 20
    Bumped patch for version 3.9.18 to 20
    Bumped patch for version 3.10.13 to 15

    ----- stderr -----
    "###);

    let contents = fs_err::read(python_version_file)?;
    assert_snapshot!(String::from_utf8(contents)?, @r###"
    3.8.12
    3.8.20
    3.10.15
    3.9.20
    "###);

    Ok(())
}
