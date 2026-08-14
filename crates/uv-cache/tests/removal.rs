#![cfg(unix)]

use std::fs::Metadata;
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use fs_err::{File, OpenOptions};
use uv_cache::Cache;

/// Return the file's metadata after syncing its allocated blocks.
fn synced_metadata(path: &Path) -> io::Result<Metadata> {
    let file = OpenOptions::new().write(true).open(path)?;
    // Sync the file so the reported block count is accurate.
    file.sync_all()?;
    file.metadata()
}

#[test]
fn remove_path_allocated_blocks() -> io::Result<()> {
    let cache = Cache::temp()?;
    let path = cache.root().join("cached.bin");
    fs_err::write(&path, [0])?;
    // Some filesystems store small files inline without allocating a block.
    if synced_metadata(&path)?.blocks() == 0 {
        return Ok(());
    }

    let summary = cache.remove_path(&path)?;
    assert_eq!(summary.num_files, 1);
    // File-length accounting would report one byte instead.
    assert!(summary.coarse_bytes >= 512);
    assert!(!path.exists());

    Ok(())
}

#[test]
fn prune_sparse_file() -> io::Result<()> {
    let cache = Cache::temp()?;
    // Prune treats unknown top-level directories as stale cache buckets.
    let stale = cache.root().join("stale-v0");
    let nested = stale.join("nested");
    fs_err::create_dir_all(&nested)?;

    let cached = stale.join("cached.bin");
    fs_err::write(&cached, [0])?;
    synced_metadata(&cached)?;

    let sparse = nested.join("sparse.bin");
    File::create(&sparse)?.set_len(1024 * 1024)?;
    let sparse_metadata = synced_metadata(&sparse)?;
    // Some filesystems, including HFS+, allocate the entire extended file.
    if sparse_metadata.blocks() >= sparse_metadata.len() / 512 {
        return Ok(());
    }

    // File-length accounting would include the hole and report 1 MiB plus one byte.
    let summary = cache.prune(false)?;
    assert_eq!(summary.num_files, 2);
    assert_eq!(summary.num_dirs, 2);
    assert!(summary.coarse_bytes < 1024 * 1024);
    assert!(!stale.exists());
    assert!(cache.root().is_dir());

    Ok(())
}
