use std::collections::BTreeMap;

use anyhow::Result;
use assert_cmd::assert::OutputAssertExt;
use assert_fs::fixture::{FileWriteBin, PathChild, PathCreateDir};
use sha2::{Digest, Sha256};

use uv_static::EnvVars;
use uv_test::packse::generate_wheel_with_files;

#[tokio::test]
async fn packaged_lock_preserves_tool_url_hash() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_tool_dirs();
    context.temp_dir.child("wheels").create_dir_all()?;
    let (filename, bytes) = generate_wheel_with_files(
        &"locked-tool".parse()?,
        &"1.0.0".parse()?,
        &[],
        &BTreeMap::new(),
        None,
        "py3-none-any",
        &[
            (
                "locked_tool-1.0.0.dist-info/pylock.toml",
                "lock-version = \"1.0\"\ncreated-by = \"test\"\npackages = []\n",
            ),
            (
                "locked_tool-1.0.0.dist-info/entry_points.txt",
                "[console_scripts]\nlocked-tool = locked_tool.cli:main\n",
            ),
            ("locked_tool/cli.py", "def main(): pass\n"),
        ],
    );
    context
        .temp_dir
        .child("wheels")
        .child(filename)
        .write_binary(&bytes)?;
    let wheel = context
        .temp_dir
        .child("wheels/locked_tool-1.0.0-py3-none-any.whl");
    let hash = hex::encode(Sha256::digest(fs_err::read(wheel.path())?));
    let wheel_url = url::Url::from_file_path(wheel.path())
        .map_err(|()| anyhow::anyhow!("Could not create a file URL"))?;
    let requirement = format!("locked-tool @ {wheel_url}#sha256={hash}&egg=locked-tool");
    context
        .tool_install()
        .args([
            "--locked",
            "--preview-features",
            "locked-tools",
            "--from",
            &requirement,
            "locked-tool",
        ])
        .env(EnvVars::PATH, context.temp_dir.child("bin").path())
        .assert()
        .success();
    let receipt = context.read("tools/locked-tool/uv-receipt.toml");
    assert!(receipt.contains(&hash));
    assert!(!receipt.contains("egg=locked-tool"));
    Ok(())
}
