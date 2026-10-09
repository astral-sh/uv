#![cfg(unix)]

use anyhow::Result;
use uv_cache_info::CacheInfo;

#[test]
fn ancestor_glob_does_not_remove_literal_symlink_base_matches() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().join("project");
    let external = directory.path().join("external");
    fs_err::create_dir_all(&root)?;
    fs_err::create_dir_all(&external)?;
    let target = external.join("source.txt");
    fs_err::write(&target, "source")?;
    fs_err::os::unix::fs::symlink(&external, root.join("linked"))?;
    let expected = CacheInfo::from_file(&target)?;
    fs_err::write(
        root.join("pyproject.toml"),
        "[tool.uv]\ncache-keys = [{ file = 'linked/**/*.txt' }]\n",
    )?;
    assert_eq!(CacheInfo::from_directory(&root)?, expected);
    fs_err::write(
        root.join("pyproject.toml"),
        "[tool.uv]\ncache-keys = [{ file = 'linked/**/*.txt' }, { file = '*.rs' }]\n",
    )?;
    assert_eq!(CacheInfo::from_directory(&root)?, expected);
    Ok(())
}
