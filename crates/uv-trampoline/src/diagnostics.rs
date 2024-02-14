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

pub struct DiagnosticBuffer {
    buffer: String,
}

impl DiagnosticBuffer {
    pub fn new() -> DiagnosticBuffer {
        DiagnosticBuffer {
            buffer: String::new(),
        }
    }

    pub fn display(self) {
        unsafe {
            let handle = GetStdHandle(STD_ERROR_HANDLE);
            let mut written: u32 = 0;
            let mut remaining = self.buffer.as_str();
            while !remaining.is_empty() {
                let ok = WriteFile(
                    handle,
                    remaining.as_ptr(),
                    remaining.len() as u32,
                    addr_of_mut!(written),
                    null_mut(),
                );
                if ok == 0 {
                    let nul_terminated = CString::new(self.buffer.as_bytes()).unwrap_unchecked();
                    MessageBoxA(0, nul_terminated.as_ptr() as *const _, null(), 0);
                    return;
                }
                remaining = &remaining.get_unchecked(written as usize..);
            }
        }
    }
}

impl uWrite for DiagnosticBuffer {
    type Error = Infallible;

    fn write_str(&mut self, s: &str) -> Result<(), Self::Error> {
        self.buffer.push_str(s);
        Ok(())
    }
}

#[macro_export]
macro_rules! eprintln {
    ($($tt:tt)*) => {{
        let mut d = $crate::diagnostics::DiagnosticBuffer::new();
        _ = ufmt::uwriteln!(&mut d, $($tt)*);
        d.display();
    }}
}
