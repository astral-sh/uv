use std::fmt;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use tokio::sync::{Notify, Semaphore, SemaphorePermit};

/// The priority of an operation using the shared download concurrency limit.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum DownloadPriority {
    /// Work required to make progress on the current operation.
    Active,
    /// Work performed in anticipation of a future request.
    Speculative,
}

/// A semaphore that prioritizes active downloads over speculative downloads.
///
/// This limiter does not preempt work that already holds a permit. When an active request is
/// waiting, speculative requests wait outside the semaphore queue or yield a newly acquired permit
/// until the active request can acquire it.
pub struct PrioritySemaphore {
    semaphore: Semaphore,
    active_waiters: AtomicUsize,
    active_waiters_done: Notify,
}

impl PrioritySemaphore {
    /// Create a new priority-aware semaphore with the given number of permits.
    pub fn new(permits: usize) -> Self {
        Self {
            semaphore: Semaphore::new(permits),
            active_waiters: AtomicUsize::new(0),
            active_waiters_done: Notify::new(),
        }
    }

    /// Acquire a permit at the given [`DownloadPriority`].
    pub async fn acquire(&self, priority: DownloadPriority) -> SemaphorePermit<'_> {
        match priority {
            DownloadPriority::Active => self.acquire_active().await,
            DownloadPriority::Speculative => self.acquire_speculative().await,
        }
    }

    async fn acquire_active(&self) -> SemaphorePermit<'_> {
        let waiter = ActiveWaiter::new(self);
        let permit = self
            .semaphore
            .acquire()
            .await
            .expect("download semaphore is never closed");
        drop(waiter);
        permit
    }

    async fn acquire_speculative(&self) -> SemaphorePermit<'_> {
        loop {
            self.wait_for_active_requests().await;

            let permit = self
                .semaphore
                .acquire()
                .await
                .expect("download semaphore is never closed");
            if self.active_waiters.load(Ordering::Acquire) == 0 {
                return permit;
            }
            drop(permit);
        }
    }

    async fn wait_for_active_requests(&self) {
        loop {
            let notified = self.active_waiters_done.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();

            if self.active_waiters.load(Ordering::Acquire) == 0 {
                return;
            }
            notified.await;
        }
    }
}

struct ActiveWaiter<'a> {
    semaphore: &'a PrioritySemaphore,
}

impl<'a> ActiveWaiter<'a> {
    fn new(semaphore: &'a PrioritySemaphore) -> Self {
        semaphore.active_waiters.fetch_add(1, Ordering::AcqRel);
        Self { semaphore }
    }
}

impl Drop for ActiveWaiter<'_> {
    fn drop(&mut self) {
        if self.semaphore.active_waiters.fetch_sub(1, Ordering::AcqRel) == 1 {
            self.semaphore.active_waiters_done.notify_waiters();
        }
    }
}

/// Concurrency limit settings.
// TODO(konsti): We should find a pattern that doesn't require having both semaphores and counts.
#[derive(Clone)]
pub struct Concurrency {
    /// The maximum number of concurrent downloads.
    ///
    /// Note this value must be non-zero.
    pub downloads: usize,
    /// The maximum number of concurrent builds.
    ///
    /// Note this value must be non-zero.
    pub builds: usize,
    /// The maximum number of concurrent installs.
    ///
    /// Note this value must be non-zero.
    pub installs: usize,
    /// A priority-aware global semaphore to limit the number of concurrent downloads.
    pub downloads_semaphore: Arc<PrioritySemaphore>,
    /// A global semaphore to limit the number of concurrent builds.
    pub builds_semaphore: Arc<Semaphore>,
}

/// Custom `Debug` to hide semaphore fields from `--show-settings` output.
#[expect(clippy::missing_fields_in_debug)]
impl fmt::Debug for Concurrency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Concurrency")
            .field("downloads", &self.downloads)
            .field("builds", &self.builds)
            .field("installs", &self.installs)
            .finish()
    }
}

impl Default for Concurrency {
    fn default() -> Self {
        Self::new(Self::DEFAULT_DOWNLOADS, Self::threads(), Self::threads())
    }
}

impl Concurrency {
    // The default concurrent downloads limit.
    pub const DEFAULT_DOWNLOADS: usize = 50;

    /// Create a new [`Concurrency`] with the given limits.
    pub fn new(downloads: usize, builds: usize, installs: usize) -> Self {
        Self {
            downloads,
            builds,
            installs,
            downloads_semaphore: Arc::new(PrioritySemaphore::new(downloads)),
            builds_semaphore: Arc::new(Semaphore::new(builds)),
        }
    }

    // The default concurrent builds and install limit.
    pub fn threads() -> usize {
        std::thread::available_parallelism()
            .map(NonZeroUsize::get)
            .unwrap_or(1)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use tokio::sync::{mpsc, oneshot};

    use super::{DownloadPriority, PrioritySemaphore};

    #[tokio::test(flavor = "current_thread")]
    async fn speculative_requests_use_all_available_permits() {
        let semaphore = PrioritySemaphore::new(2);
        let _first = semaphore.acquire(DownloadPriority::Speculative).await;
        let _second = tokio::time::timeout(
            Duration::from_secs(1),
            semaphore.acquire(DownloadPriority::Speculative),
        )
        .await
        .expect("speculative requests should use idle download capacity");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn active_request_overtakes_speculative_request() {
        let semaphore = Arc::new(PrioritySemaphore::new(1));
        let permit = semaphore.acquire(DownloadPriority::Active).await;
        let (order_tx, mut order_rx) = mpsc::unbounded_channel();

        let (speculative_started_tx, speculative_started_rx) = oneshot::channel();
        let speculative = tokio::spawn({
            let semaphore = Arc::clone(&semaphore);
            let order_tx = order_tx.clone();
            async move {
                speculative_started_tx.send(()).ok();
                let _permit = semaphore.acquire(DownloadPriority::Speculative).await;
                order_tx.send(DownloadPriority::Speculative).ok();
            }
        });
        speculative_started_rx
            .await
            .expect("speculative task should start");

        let (active_started_tx, active_started_rx) = oneshot::channel();
        let active = tokio::spawn({
            let semaphore = Arc::clone(&semaphore);
            async move {
                active_started_tx.send(()).ok();
                let _permit = semaphore.acquire(DownloadPriority::Active).await;
                order_tx.send(DownloadPriority::Active).ok();
            }
        });
        active_started_rx.await.expect("active task should start");

        drop(permit);

        assert_eq!(order_rx.recv().await, Some(DownloadPriority::Active));
        assert_eq!(order_rx.recv().await, Some(DownloadPriority::Speculative));
        active.await.expect("active task should complete");
        speculative.await.expect("speculative task should complete");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn cancelled_active_request_unblocks_speculative_request() {
        let semaphore = Arc::new(PrioritySemaphore::new(1));
        let permit = semaphore.acquire(DownloadPriority::Active).await;

        let (active_started_tx, active_started_rx) = oneshot::channel();
        let active = tokio::spawn({
            let semaphore = Arc::clone(&semaphore);
            async move {
                active_started_tx.send(()).ok();
                let _permit = semaphore.acquire(DownloadPriority::Active).await;
            }
        });
        active_started_rx.await.expect("active task should start");
        active.abort();
        active
            .await
            .expect_err("active task should be cancelled while waiting");

        drop(permit);
        let _permit = semaphore.acquire(DownloadPriority::Speculative).await;
    }
}
