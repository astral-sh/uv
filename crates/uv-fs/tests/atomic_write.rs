use std::io;

use uv_fs::write_atomic_sync;

#[test]
fn create_and_replace() -> io::Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("entry");

    for data in [b"original".as_slice(), b"replacement", b""] {
        write_atomic_sync(&path, data)?;
        assert_eq!(fs_err::read(&path)?, data);
        assert_eq!(fs_err::read_dir(directory.path())?.count(), 1);
    }

    Ok(())
}

#[cfg(unix)]
#[test]
fn replace_symlink() -> io::Result<()> {
    let directory = tempfile::tempdir()?;
    let target = directory.path().join("target");
    let path = directory.path().join("entry");

    // Both dangling symlinks and symlinks to existing files must be replaced, not followed.
    for target_exists in [false, true] {
        if target_exists {
            fs_err::write(&target, b"target")?;
        }
        fs_err::os::unix::fs::symlink(&target, &path)?;
        write_atomic_sync(&path, b"replacement")?;
        assert!(fs_err::symlink_metadata(&path)?.is_file());
        assert_eq!(fs_err::read(&path)?, b"replacement");
        if target_exists {
            assert_eq!(fs_err::read(&target)?, b"target");
        } else {
            assert!(!target.exists());
        }
        fs_err::remove_file(&path)?;
    }

    Ok(())
}

#[test]
fn failed_replacement_cleans_up() -> io::Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("entry");
    fs_err::create_dir(&path)?;
    let child = path.join("child");
    fs_err::write(&child, b"original")?;

    assert!(write_atomic_sync(&path, b"replacement").is_err());
    assert_eq!(fs_err::read(&child)?, b"original");
    assert_eq!(fs_err::read_dir(directory.path())?.count(), 1);

    Ok(())
}
