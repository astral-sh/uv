use std::num::NonZeroUsize;

/// Concurrency limit settings.
#[derive(Copy, Clone, Debug)]
pub struct Concurrency {
    /// The maximum number of concurrent downloads.
    ///
    /// Note this value must be non-zero.
    pub downloads: usize,
    /// The maximum number of concurrent builds.
    ///
    /// Note this value must be non-zero.
    pub builds: usize,
}

impl Default for Concurrency {
    fn default() -> Self {
        Concurrency {
            downloads: Concurrency::DEFAULT_DOWNLOADS,
            builds: Concurrency::default_builds(),
        }
    }
}

impl Concurrency {
    // The default concurrent downloads limit.
    pub const DEFAULT_DOWNLOADS: usize = 50;

    // The default concurrent builds limit.
    pub fn default_builds() -> usize {
        std::thread::available_parallelism()
            .map(NonZeroUsize::get)
            .unwrap_or(1)
    }
}
