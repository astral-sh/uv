// Don't optimize the alloc crate away due to it being otherwise unused.
// https://github.com/rust-lang/rust/issues/64402
#[cfg(feature = "performance-memory-allocator")]
extern crate uv_performance_memory_allocator;

use std::process::ExitCode;

use uv::main as uv_main;

fn main() -> ExitCode {
    uv_main(std::env::args_os())
}
