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

#[test]
fn ancestor_glob_does_not_remove_matches_below_intermediate_symlink() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().join("project");
    let external = directory.path().join("external");
    fs_err::create_dir_all(&root)?;
    fs_err::create_dir_all(external.join("nested"))?;
    let target = external.join("nested/source.txt");
    fs_err::write(&target, "source")?;
    fs_err::os::unix::fs::symlink(&external, root.join("linked"))?;
    let expected = CacheInfo::from_file(&target)?;
    fs_err::write(
        root.join("pyproject.toml"),
        "[tool.uv]\ncache-keys = [{ file = 'linked/nested/**/*.txt' }, { file = '*.rs' }]\n",
    )?;
    assert_eq!(CacheInfo::from_directory(&root)?, expected);
    Ok(())
}

#[test]
fn protected_parent_glob_does_not_hide_nested_symlink_matches() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().join("project");
    let external = directory.path().join("external");
    let nested = directory.path().join("nested");
    fs_err::create_dir_all(&root)?;
    fs_err::create_dir_all(&external)?;
    fs_err::create_dir_all(&nested)?;
    let target = nested.join("source.txt");
    fs_err::write(&target, "source")?;
    fs_err::os::unix::fs::symlink(&external, root.join("linked"))?;
    fs_err::os::unix::fs::symlink(&nested, external.join("nested"))?;
    let expected = CacheInfo::from_file(&target)?;
    fs_err::write(
        root.join("pyproject.toml"),
        "[tool.uv]\ncache-keys = [{ file = 'linked/**/*.rs' }, { file = 'linked/nested/**/*.txt' }, { file = 'linked/nested/**/*.rs' }]\n",
    )?;
    assert_eq!(CacheInfo::from_directory(&root)?, expected);
    Ok(())
}
