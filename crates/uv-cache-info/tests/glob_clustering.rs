use anyhow::Result;
use uv_cache_info::CacheInfo;

#[test]
fn adding_unmatched_glob_preserves_recursive_basename_matches() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let root = directory.path();
    fs_err::create_dir_all(root.join("a/deep"))?;
    let source = root.join("a/deep/source.txt");
    fs_err::write(&source, "source")?;
    let expected = CacheInfo::from_file(&source)?;

    fs_err::write(
        root.join("pyproject.toml"),
        "[tool.uv]\ncache-keys = [{ file = 'a/*.txt' }]\n",
    )?;
    assert_eq!(CacheInfo::from_directory(root)?, expected);

    fs_err::write(
        root.join("pyproject.toml"),
        "[tool.uv]\ncache-keys = [{ file = 'a/*.txt' }, { file = '{a,b}/*.rs' }]\n",
    )?;
    assert_eq!(CacheInfo::from_directory(root)?, expected);
    Ok(())
}
