//! The only purpose of this crate is to configure the allocator used by uv.

#[cfg(target_os = "windows")]
mod mimalloc {
    use core::alloc::{GlobalAlloc, Layout};
    use core::ffi::c_void;
    #[cfg(test)]
    use core::ffi::{c_int, c_long};

    unsafe extern "C" {
        fn mi_malloc_aligned(size: usize, alignment: usize) -> *mut c_void;
        fn mi_zalloc_aligned(size: usize, alignment: usize) -> *mut c_void;
        fn mi_realloc_aligned(
            pointer: *mut c_void,
            new_size: usize,
            alignment: usize,
        ) -> *mut c_void;
        fn mi_free(pointer: *mut c_void);

        #[cfg(test)]
        fn uv_mimalloc_default_purge_delay() -> c_long;
        #[cfg(test)]
        fn uv_mimalloc_default_arena_purge_mult() -> c_long;
        #[cfg(test)]
        fn uv_mimalloc_large_pages_enabled() -> c_int;
    }

    pub(crate) struct Mimalloc;

    // SAFETY: Each method forwards to mimalloc with the size and alignment from
    // the caller-provided [`Layout`], matching the [`GlobalAlloc`] contract.
    unsafe impl GlobalAlloc for Mimalloc {
        #[inline]
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            // SAFETY: Mimalloc accepts any valid `Layout` size and alignment.
            unsafe { mi_malloc_aligned(layout.size(), layout.align()) }.cast()
        }

        #[inline]
        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            // SAFETY: Mimalloc accepts any valid `Layout` size and alignment.
            unsafe { mi_zalloc_aligned(layout.size(), layout.align()) }.cast()
        }

        #[inline]
        unsafe fn dealloc(&self, pointer: *mut u8, _layout: Layout) {
            // SAFETY: `pointer` was returned by this allocator.
            unsafe { mi_free(pointer.cast()) };
        }

        #[inline]
        unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            // SAFETY: `pointer` was returned by this allocator and `layout`
            // supplies its alignment.
            unsafe { mi_realloc_aligned(pointer.cast(), new_size, layout.align()) }.cast()
        }
    }

    #[cfg(test)]
    mod tests {
        use super::{
            GlobalAlloc, Layout, Mimalloc, uv_mimalloc_default_arena_purge_mult,
            uv_mimalloc_default_purge_delay, uv_mimalloc_large_pages_enabled,
        };

        #[test]
        fn compiled_configuration() {
            // SAFETY: These functions take no inputs and report compile-time constants.
            unsafe {
                assert_eq!(uv_mimalloc_default_purge_delay(), 10);
                assert_eq!(uv_mimalloc_default_arena_purge_mult(), 10);
                assert_eq!(uv_mimalloc_large_pages_enabled(), 0);
            }
        }

        #[test]
        fn allocates_with_mimalloc() {
            let allocator = Mimalloc;
            let layout = Layout::from_size_align(1024, 64).expect("valid allocation layout");
            let expanded_layout =
                Layout::from_size_align(2048, 64).expect("valid expanded allocation layout");

            // SAFETY: The allocation and deallocation use the same allocator and layout.
            unsafe {
                let pointer = allocator.alloc(layout);
                assert!(!pointer.is_null());
                allocator.dealloc(pointer, layout);

                let pointer = allocator.alloc_zeroed(layout);
                assert!(!pointer.is_null());
                assert_eq!(pointer.align_offset(layout.align()), 0);
                assert!(
                    core::slice::from_raw_parts(pointer, layout.size())
                        .iter()
                        .all(|byte| *byte == 0)
                );

                let pointer = allocator.realloc(pointer, layout, expanded_layout.size());
                assert!(!pointer.is_null());
                assert_eq!(pointer.align_offset(expanded_layout.align()), 0);
                allocator.dealloc(pointer, expanded_layout);
            }
        }
    }
}

#[cfg(target_os = "windows")]
#[global_allocator]
static GLOBAL: mimalloc::Mimalloc = mimalloc::Mimalloc;

#[cfg(all(
    not(target_os = "windows"),
    not(target_os = "openbsd"),
    not(target_os = "freebsd"),
    any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "powerpc64"
    )
))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;
