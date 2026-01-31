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
}

impl Default for Concurrency {
    fn default() -> Self {
        Self {
            downloads: Self::DEFAULT_DOWNLOADS,
            builds: Self::threads(),
            installs: Self::threads(),
            uploads: Self::DEFAULT_UPLOADS,
            pyx_wheel_validations: Self::DEFAULT_PYX_WHEEL_VALIDATIONS,
        }
    }
}

impl Concurrency {
    // The default concurrent downloads limit.
    pub const DEFAULT_DOWNLOADS: usize = 50;

    // The default concurrent uploads limit.
    pub const DEFAULT_UPLOADS: usize = 1;

    // The default concurrent pyx wheel validations limit.
    pub const DEFAULT_PYX_WHEEL_VALIDATIONS: usize = 32;

    // The default concurrent builds and install limit.
    pub fn threads() -> usize {
        std::thread::available_parallelism()
            .map(NonZeroUsize::get)
            .unwrap_or(1)
    }
}
