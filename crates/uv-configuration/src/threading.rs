//! Configure rayon and determine thread stack sizes.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::LazyLock;
use uv_static::EnvVars;

/// The default minimum stack size for uv threads.
pub const UV_MIN_STACK_DEFAULT: usize = 4 * 1024 * 1024;

/// Running out of stack has been an issue for us. We box types and futures in various places
/// to mitigate this.
///
/// Main thread stack-size has a BIG variety here across platforms and it's harder to control
/// (which is why Rust doesn't by default). Notably  on macOS and Linux you will typically get 8MB
/// main thread, while on Windows you will typically get 1MB, which is *tiny*:
/// <https://learn.microsoft.com/en-us/cpp/build/reference/stack-stack-allocations?view=msvc-170>
///
/// To normalize this we just spawn a new thread called main2 with a size we can set
/// ourselves. 2MB is typically too small (especially for our debug builds), while 4MB
/// seems fine. Also we still try to respect `RUST_MIN_STACK` if it's set, in case useful,
/// but don't let it ask for a smaller stack to avoid messy misconfiguration since we
/// know we use quite a bit of main stack space.
///
/// Non-main threads should all have 2MB, as Rust forces platform consistency there,
/// but even then stack overflows can occur in release mode
/// (<https://github.com/astral-sh/uv/issues/12769>), so also configure a 4MB stack for tokio and
/// rayon threads.
pub fn min_stack_size() -> usize {
    std::env::var(EnvVars::RUST_MIN_STACK)
        .ok()
        .and_then(|var| var.parse::<usize>().ok())
        .unwrap_or(UV_MIN_STACK_DEFAULT)
}

/// The number of threads for the rayon threadpool.
///
/// The default of 0 makes rayon use its default.
pub static RAYON_PARALLELISM: AtomicUsize = AtomicUsize::new(0);

/// Initialize the threadpool lazily. Always call before using rayon the potentially first time.
///
/// The `uv` crate sets [`RAYON_PARALLELISM`] from the user settings, and the extract and install
/// code initialize the threadpool lazily only if they are actually used by calling
/// `LazyLock::force(&RAYON_INITIALIZE)`.
pub static RAYON_INITIALIZE: LazyLock<()> = LazyLock::new(|| {
    rayon::ThreadPoolBuilder::new()
        .num_threads(RAYON_PARALLELISM.load(Ordering::SeqCst))
        .stack_size(min_stack_size())
        .build_global()
        .expect("failed to initialize global rayon pool");
});
