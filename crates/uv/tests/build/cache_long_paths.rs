use std::{
    io::{self, Write},
    os::fd::OwnedFd,
    path::Path,
};

use anyhow::Result;
use assert_fs::prelude::*;
use nix::{
    errno::Errno,
    fcntl::{OFlag, openat},
    sys::stat::{Mode, mkdirat, stat},
};

use uv_test::uv_snapshot;

/// `cache clean` should remove directory trees that exceed the macOS path length limit.
#[test]
fn clean_handles_overlong_paths() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);
    create_overlong_path(context.cache_dir.path(), OverlongPathKind::Directory)?;

    uv_snapshot!(context.filters(), context.clean(), @"
    exit_code: 0 (success)
    ----- stderr -----
    Clearing cache at: [CACHE_DIR]/
    Removed 1 file
    ");

    assert!(!context.cache_dir.exists());

    Ok(())
}

/// `cache clean` should remove files whose full paths exceed the macOS path length limit.
#[test]
fn clean_handles_overlong_file_paths() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);
    create_overlong_path(context.cache_dir.path(), OverlongPathKind::File)?;

    uv_snapshot!(context.filters(), context.clean(), @"
    exit_code: 0 (success)
    ----- stderr -----
    Clearing cache at: [CACHE_DIR]/
    Removed 1 file
    ");

    assert!(!context.cache_dir.exists());

    Ok(())
}

/// `cache prune` should remove stale directory trees that exceed the macOS path length limit.
#[test]
fn prune_handles_overlong_paths() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filter((r"Removed \d+ directories", "Removed [N] directories"));
    let stale_bucket = context.cache_dir.child("simple-v4");
    create_overlong_path(stale_bucket.path(), OverlongPathKind::Directory)?;

    uv_snapshot!(context.filters(), context.prune(), @"
    exit_code: 0 (success)
    ----- stderr -----
    Pruning cache at: [CACHE_DIR]/
    Removed [N] directories
    ");

    assert!(!stale_bucket.exists());

    Ok(())
}

#[derive(Clone, Copy)]
enum OverlongPathKind {
    Directory,
    File,
}

/// Create a file or directory whose full path exceeds the macOS path length limit.
fn create_overlong_path(path: &Path, kind: OverlongPathKind) -> io::Result<()> {
    fs_err::create_dir_all(path)?;

    let component = "x".repeat(100);
    let mut directory: OwnedFd = fs_err::File::open(path)?.into();
    let mut overlong_path = path.to_path_buf();
    for _ in 0..14 {
        if matches!(kind, OverlongPathKind::File)
            && matches!(
                stat(&overlong_path.join(&component)),
                Err(Errno::ENAMETOOLONG)
            )
        {
            break;
        }

        mkdirat(
            &directory,
            component.as_str(),
            Mode::from_bits_truncate(0o755),
        )?;
        directory = openat(
            &directory,
            component.as_str(),
            OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_CLOEXEC,
            Mode::empty(),
        )?;
        overlong_path.push(&component);
    }

    let payload_name = match kind {
        OverlongPathKind::Directory => "payload.txt".to_string(),
        OverlongPathKind::File => "y".repeat(255),
    };
    let payload = openat(
        &directory,
        payload_name.as_str(),
        OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_CLOEXEC,
        Mode::from_bits_truncate(0o644),
    )?;
    let payload_path = overlong_path.join(payload_name);
    fs_err::File::from_parts(payload.into(), &payload_path).write_all(b"payload")?;

    let overlong_path = match kind {
        OverlongPathKind::Directory => overlong_path,
        OverlongPathKind::File => payload_path,
    };
    let error = stat(&overlong_path).expect_err("Expected the path to be too long");
    assert_eq!(error, Errno::ENAMETOOLONG);

    Ok(())
}
