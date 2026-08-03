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
    create_overlong_directory(context.cache_dir.path())?;

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
    let context = uv_test::test_context_with_versions!(&[]);
    let stale_bucket = context.cache_dir.child("simple-v4");
    create_overlong_directory(stale_bucket.path())?;

    let filters: Vec<_> = context
        .filters()
        .into_iter()
        .chain(std::iter::once((
            r"Removed \d+ directories",
            "Removed [N] directories",
        )))
        .collect();

    uv_snapshot!(&filters, context.prune(), @"
    exit_code: 0 (success)
    ----- stderr -----
    Pruning cache at: [CACHE_DIR]/
    Removed [N] directories
    ");

    assert!(!stale_bucket.exists());

    Ok(())
}

/// Create a directory tree whose full path exceeds the macOS path length limit.
fn create_overlong_directory(path: &Path) -> io::Result<()> {
    fs_err::create_dir_all(path)?;

    let component = "x".repeat(100);
    let mut directory: OwnedFd = fs_err::File::open(path)?.into();
    let mut overlong_path = path.to_path_buf();
    for _ in 0..14 {
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

    let payload = openat(
        &directory,
        "payload.txt",
        OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_CLOEXEC,
        Mode::from_bits_truncate(0o644),
    )?;
    fs_err::File::from_parts(payload.into(), overlong_path.join("payload.txt"))
        .write_all(b"payload")?;

    let error = stat(&overlong_path).expect_err("Expected the path to be too long");
    assert_eq!(error, Errno::ENAMETOOLONG);

    Ok(())
}
