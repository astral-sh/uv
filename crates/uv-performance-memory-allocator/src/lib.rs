//! The only purpose of this crate is to pull in `mimalloc` on windows and
//! `tikv-jemallocator` on most other platforms.
//! Haiku uses its system allocator because the bundled jemalloc cannot build there.

#[cfg(target_os = "windows")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[cfg(all(
    not(any(
        target_os = "windows",
        target_os = "openbsd",
        target_os = "freebsd",
        target_os = "haiku"
    )),
    any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "powerpc64"
    )
))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;
