#![cfg(feature = "tokio")]

use std::error::Error;
use std::future::{Future, poll_fn};
use std::pin::pin;
use std::sync::mpsc;
use std::task::Poll;
use std::time::Duration;

use uv_fs::{LockedFile, LockedFileMode};

/// A lock holder must be able to finish blocking I/O while another task waits for its lock.
#[test]
fn contended_lock_does_not_block_io() -> Result<(), Box<dyn Error>> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .max_blocking_threads(1)
        .enable_all()
        .build()?;
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("resource.lock");

    runtime.block_on(async {
        let lock = LockedFile::acquire(&path, LockedFileMode::Exclusive, "resource").await?;
        let mut waiter = pin!(LockedFile::acquire(
            &path,
            LockedFileMode::Exclusive,
            "resource"
        ));

        // Occupy the sole blocking thread so the first poll queues the nonblocking lock check
        // without advancing to the contended wait.
        let (release, blocked) = mpsc::channel();
        let blocking = tokio::task::spawn_blocking(move || blocked.recv());
        poll_fn(|context| {
            assert!(waiter.as_mut().poll(context).is_pending());
            Poll::Ready(())
        })
        .await;
        release.send(())?;
        blocking.await??;

        // Finish the queued nonblocking check before polling the contended wait.
        tokio::task::spawn_blocking(|| ()).await?;
        poll_fn(|context| {
            assert!(waiter.as_mut().poll(context).is_pending());
            Poll::Ready(())
        })
        .await;

        let progress =
            tokio::time::timeout(Duration::from_secs(5), tokio::task::spawn_blocking(|| ())).await;

        // Release the lock even if the blocking pool stalled, so a regression cannot hang
        // runtime shutdown. The waiter must also acquire the lock once it becomes available.
        drop(lock);
        let _lock = tokio::time::timeout(Duration::from_secs(5), waiter).await??;
        progress??;
        Ok(())
    })
}
