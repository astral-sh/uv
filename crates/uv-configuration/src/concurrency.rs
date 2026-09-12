use std::fmt;
use std::num::NonZeroUsize;
use std::sync::Arc;

use tokio::sync::Semaphore;

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
    /// The maximum number of concurrent cache reads.
    ///
    /// Note this value must be non-zero.
    pub cache_reads: usize,
    /// A global semaphore to limit the number of concurrent downloads.
    pub downloads_semaphore: Arc<Semaphore>,
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
            .field("cache_reads", &self.cache_reads)
            .finish()
    }
}

impl Default for Concurrency {
    fn default() -> Self {
        Self::new(
            Self::DEFAULT_DOWNLOADS,
            Self::threads(),
            Self::threads(),
            Self::DEFAULT_CACHE_READS,
        )
    }
}

impl Concurrency {
    // The default concurrent downloads limit.
    pub const DEFAULT_DOWNLOADS: usize = 50;

    // The default concurrent cache reads limit.
    pub const DEFAULT_CACHE_READS: usize = 4;

    /// Create a new [`Concurrency`] with the given limits.
    ///
    /// On Unix, the download, build, and install limits are capped to the number of concurrent
    /// operations that fit within the process's soft open file limit, so that uv doesn't exhaust
    /// the file descriptors available to it.
    ///
    /// See: <https://github.com/astral-sh/uv/issues/11296>
    pub fn new(downloads: usize, builds: usize, installs: usize, cache_reads: usize) -> Self {
        #[cfg(unix)]
        let (downloads, builds, installs) = cap_by_open_file_limit(downloads, builds, installs);
        Self {
            downloads,
            builds,
            installs,
            cache_reads,
            downloads_semaphore: Arc::new(Semaphore::new(downloads)),
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

/// The number of open file descriptors reserved for uv's own operation.
///
/// This covers the file descriptors that are always or transiently held, like the ones for
/// standard input, output, and error, the event loop, cache databases, and temporary files.
/// The reserve only needs to cover a small fraction of a typical open file limit, since uv
/// keeps most of its file descriptors open only briefly.
const RESERVED_OPEN_FILES: u64 = 32;

/// The number of open file descriptors held concurrently by each package moving through the
/// download, build, and install pipeline.
///
/// A build dominates the pipeline with about five open file descriptors for the build backend
/// process, its pipes, and the files written to the build cache. Downloads and installs add a
/// few more, e.g., for the download target file and the unpacked wheel, and we include
/// additional slack for file descriptors held only briefly.
const OPEN_FILES_PER_PIPELINE_PACKAGE: u64 = 12;

/// The highest number of concurrent downloads, builds, and installs to allow for the given
/// soft open file limit.
///
/// At least one concurrent operation is always allowed, even if the limit doesn't leave room
/// for it, since uv can't make progress otherwise.
fn open_file_limit_concurrency_cap(soft_limit: u64) -> usize {
    let concurrent_packages =
        soft_limit.saturating_sub(RESERVED_OPEN_FILES) / OPEN_FILES_PER_PIPELINE_PACKAGE;
    usize::try_from(concurrent_packages)
        .unwrap_or(usize::MAX)
        .max(1)
}

#[cfg(unix)]
fn cap_by_open_file_limit(
    downloads: usize,
    builds: usize,
    installs: usize,
) -> (usize, usize, usize) {
    let Some(soft_limit) = uv_unix::soft_open_file_limit() else {
        return (downloads, builds, installs);
    };
    let cap = open_file_limit_concurrency_cap(soft_limit);
    let capped = (downloads.min(cap), builds.min(cap), installs.min(cap));
    if capped != (downloads, builds, installs) {
        tracing::debug!(
            "Capping concurrency to at most {} concurrent downloads, builds, and installs \
             due to the open file limit of {}",
            cap,
            soft_limit
        );
    }
    capped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_file_limit_allows_single_concurrent_operation_at_minimum() {
        // There's no room for concurrent operations, but we allow one regardless.
        assert_eq!(open_file_limit_concurrency_cap(0), 1);
        assert_eq!(open_file_limit_concurrency_cap(RESERVED_OPEN_FILES), 1);
        assert_eq!(
            open_file_limit_concurrency_cap(
                RESERVED_OPEN_FILES + OPEN_FILES_PER_PIPELINE_PACKAGE - 1
            ),
            1
        );
    }

    #[test]
    fn open_file_limit_scales_with_budget() {
        assert_eq!(
            open_file_limit_concurrency_cap(RESERVED_OPEN_FILES + OPEN_FILES_PER_PIPELINE_PACKAGE),
            1
        );
        assert_eq!(
            open_file_limit_concurrency_cap(
                RESERVED_OPEN_FILES + 2 * OPEN_FILES_PER_PIPELINE_PACKAGE
            ),
            2
        );
        assert_eq!(
            open_file_limit_concurrency_cap(
                RESERVED_OPEN_FILES + 10 * OPEN_FILES_PER_PIPELINE_PACKAGE
            ),
            10
        );
    }

    #[test]
    fn open_file_limit_is_not_constraining_for_typical_limits() {
        // The Linux default soft limit is enough for uv's default limits.
        assert!(open_file_limit_concurrency_cap(1024) >= Concurrency::DEFAULT_DOWNLOADS);
        // The limit raised to the typical maximum, as done at startup on Unix.
        assert_eq!(open_file_limit_concurrency_cap(0x0010_0000), 87_378);
    }
}
