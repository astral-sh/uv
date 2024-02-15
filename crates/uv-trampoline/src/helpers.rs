use core::mem::size_of;

pub trait SizeOf {
    fn size_of(&self) -> u32;
}

impl<T: Sized> SizeOf for T {
    fn size_of(&self) -> u32 {
        size_of::<T>() as u32
    }
}

// Check result of win32 API call that returns BOOL
#[macro_export]
macro_rules! check {
    ($e:expr) => {
        if $e == 0 {
            use windows_sys::Win32::{
                Foundation::*,
                System::{
                    Diagnostics::Debug::{
                        FormatMessageA, FORMAT_MESSAGE_ALLOCATE_BUFFER, FORMAT_MESSAGE_FROM_SYSTEM,
                        FORMAT_MESSAGE_IGNORE_INSERTS,
                    },
                }
            };
            let err = GetLastError();
            let mut msg_ptr: *mut u8 = core::ptr::null_mut();
            let size = FormatMessageA(
                FORMAT_MESSAGE_ALLOCATE_BUFFER
                    | FORMAT_MESSAGE_FROM_SYSTEM
                    | FORMAT_MESSAGE_IGNORE_INSERTS,
                null(),
                err,
                0,
                // Weird calling convention: this argument is typed as *mut u16,
                // but if you pass FORMAT_MESSAGE_ALLOCATE_BUFFER then you have to
                // *actually* pass in a *mut *mut u16 and just lie about the type.
                // Getting Rust to do this requires some convincing.
                core::ptr::addr_of_mut!(msg_ptr) as *mut _ as _,
                0,
                core::ptr::null(),
            );
            let msg = core::slice::from_raw_parts(msg_ptr, size as usize);
            let msg = core::str::from_utf8_unchecked(msg);
            $crate::eprintln!("Error: {} (from {})", msg, stringify!($e));
            ExitProcess(1);
        }
    }
}

// CStr literal: c!("...")
#[macro_export]
macro_rules! c {
    ($s:literal) => {
        core::ffi::CStr::from_bytes_with_nul_unchecked(concat!($s, "\0").as_bytes())
    };
}
