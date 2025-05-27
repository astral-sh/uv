// Don't optimize the alloc crate away due to it being otherwise unused.
// https://github.com/rust-lang/rust/issues/64402
#[cfg(feature = "performance-memory-allocator")]
extern crate uv_performance_memory_allocator;

use std::process::ExitCode;

use uv::main as uv_main;

#[allow(unsafe_code)]
fn main() -> ExitCode {
    // SAFETY: This is safe because we are running it early in `main` before spawning any threads.
    unsafe { uv_main(std::env::args_os()) }
}
