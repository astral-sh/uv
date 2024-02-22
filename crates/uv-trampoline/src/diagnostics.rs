extern crate alloc;

use alloc::{ffi::CString, string::String};
use core::{
    convert::Infallible,
    ptr::{addr_of_mut, null, null_mut},
};

use ufmt_write::uWrite;
use windows_sys::Win32::{
    Storage::FileSystem::WriteFile,
    System::Console::{GetStdHandle, STD_ERROR_HANDLE},
    UI::WindowsAndMessaging::MessageBoxA,
};

#[macro_export]
macro_rules! eprintln {
    ($($tt:tt)*) => {{
        $crate::diagnostics::write_diagnostic(&$crate::format!($($tt)*));
    }}
}

#[macro_export]
macro_rules! format {
    ($($tt:tt)*) => {{
        let mut buffer = $crate::diagnostics::StringBuffer::default();
        _ = ufmt::uwriteln!(&mut buffer, $($tt)*);
        buffer.0
    }}
}

#[derive(Default)]
pub(crate) struct StringBuffer(pub(crate) String);

impl uWrite for StringBuffer {
    type Error = Infallible;

    fn write_str(&mut self, s: &str) -> Result<(), Self::Error> {
        self.0.push_str(s);
        Ok(())
    }
}

#[cold]
pub(crate) fn write_diagnostic(message: &str) {
    unsafe {
        let handle = GetStdHandle(STD_ERROR_HANDLE);
        let mut written: u32 = 0;
        let mut remaining = message;
        while !remaining.is_empty() {
            let ok = WriteFile(
                handle,
                remaining.as_ptr(),
                remaining.len() as u32,
                addr_of_mut!(written),
                null_mut(),
            );
            if ok == 0 {
                let nul_terminated = CString::new(message.as_bytes()).unwrap_unchecked();
                MessageBoxA(0, nul_terminated.as_ptr() as *const _, null(), 0);
                return;
            }
            remaining = &remaining.get_unchecked(written as usize..);
        }
    }
}
