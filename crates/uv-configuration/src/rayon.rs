//! Initialize the rayon threadpool once, before we need it.
//!
//! The `uv` crate sets [`RAYON_PARALLELISM`] from the user settings, and the extract and install
//! code initialize the threadpool lazily only if they are actually used by calling
//! `LazyLock::force(&RAYON_INITIALIZE)`.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::LazyLock;

/// The number of threads for the rayon threadpool.
///
/// The default of 0 makes rayon use its default.
pub static RAYON_PARALLELISM: AtomicUsize = AtomicUsize::new(0);

/// Initialize the threadpool lazily. Always call before using rayon the potentially first time.
pub static RAYON_INITIALIZE: LazyLock<()> = LazyLock::new(|| {
    rayon::ThreadPoolBuilder::new()
        .num_threads(RAYON_PARALLELISM.load(Ordering::SeqCst))
        .build_global()
        .expect("failed to initialize global rayon pool");
});
