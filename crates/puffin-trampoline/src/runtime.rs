// Nothing in this file is directly imported anywhere else; it just fills in
// some of the no_std gaps.

extern crate alloc;

use alloc::alloc::{GlobalAlloc, Layout};
use core::ffi::c_void;

use windows_sys::Win32::System::{
    Memory::{GetProcessHeap, HeapAlloc, HeapFree, HeapReAlloc, HEAP_ZERO_MEMORY},
    Threading::ExitProcess,
};

use crate::eprintln;

// Windows wants this symbol. It has something to do with floating point usage?
// idk, defining it gets rid of link errors.
#[no_mangle]
#[used]
static _fltused: i32 = 0;

struct SystemAlloc;

#[global_allocator]
static SYSTEM_ALLOC: SystemAlloc = SystemAlloc;

unsafe impl Sync for SystemAlloc {}
unsafe impl GlobalAlloc for SystemAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        HeapAlloc(GetProcessHeap(), 0, layout.size()) as *mut u8
    }
    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        HeapFree(GetProcessHeap(), 0, ptr as *const c_void);
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        HeapAlloc(GetProcessHeap(), HEAP_ZERO_MEMORY, layout.size()) as *mut u8
    }
    unsafe fn realloc(&self, ptr: *mut u8, _layout: Layout, new_size: usize) -> *mut u8 {
        HeapReAlloc(GetProcessHeap(), 0, ptr as *const c_void, new_size) as *mut u8
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    if let Some(location) = info.location() {
        let mut msg = "(couldn't format message)";
        if let Some(msg_args) = info.message() {
            if let Some(msg_str) = msg_args.as_str() {
                msg = msg_str;
            }
        }
        eprintln!(
            "panic at {}:{} (column {}): {}",
            location.file(),
            location.line(),
            location.column(),
            msg,
        );
    }
    unsafe {
        ExitProcess(128);
    }
}
