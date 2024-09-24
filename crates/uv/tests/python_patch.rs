use assert_fs::fixture::{FileWriteStr, PathChild};

use anyhow::Result;
use common::{uv_snapshot, TestContext};
use uv_python::PYTHON_VERSION_FILENAME;

mod common;

#[test]
fn python_patch() -> Result<()> {
    let context = TestContext::new_with_versions(&["3.8.12", "3.8.18"]);

    let python_version_file = context.temp_dir.child(PYTHON_VERSION_FILENAME);
    python_version_file.write_str(r"3.8.12")?;
    uv_snapshot!(context.filters(), context.python_patch(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Bumped patch for version 3.12.1 to 20

    ----- stderr -----
    "###);

    Ok(())
}
