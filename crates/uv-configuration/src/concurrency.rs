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
    /// The maximum number of concurrent publish uploads.
    ///
    /// Note this value must be non-zero.
    pub uploads: usize,
    /// The maximum number of concurrent pyx wheel validations.
    ///
    /// Note this value must be non-zero.
    pub pyx_wheel_validations: usize,
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
            .field("uploads", &self.uploads)
            .field("pyx_wheel_validations", &self.pyx_wheel_validations)
            .finish()
    }
}

impl Default for Concurrency {
    fn default() -> Self {
        Self::new(
            Self::DEFAULT_DOWNLOADS,
            Self::threads(),
            Self::threads(),
            Self::DEFAULT_UPLOADS,
            Self::DEFAULT_PYX_WHEEL_VALIDATIONS,
        )
    }
}

impl Concurrency {
    // The default concurrent downloads limit.
    pub const DEFAULT_DOWNLOADS: usize = 50;

    // The default concurrent uploads limit.
    pub const DEFAULT_UPLOADS: usize = 1;

    // The default concurrent pyx wheel validations limit.
    pub const DEFAULT_PYX_WHEEL_VALIDATIONS: usize = 32;

    /// Create a new [`Concurrency`] with the given limits.
    pub fn new(
        downloads: usize,
        builds: usize,
        installs: usize,
        uploads: usize,
        pyx_wheel_validations: usize,
    ) -> Self {
        Self {
            downloads,
            builds,
            installs,
            uploads,
            pyx_wheel_validations,
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
