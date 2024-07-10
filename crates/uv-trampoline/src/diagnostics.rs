use std::convert::Infallible;
use std::ffi::CString;
use std::string::String;

use ufmt_write::uWrite;
use windows::core::PCSTR;
use windows::Win32::{
    Foundation::INVALID_HANDLE_VALUE,
    Storage::FileSystem::WriteFile,
    System::Console::{GetStdHandle, STD_ERROR_HANDLE},
    UI::WindowsAndMessaging::{MessageBoxA, MESSAGEBOX_STYLE},
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
    let handle = unsafe { GetStdHandle(STD_ERROR_HANDLE) }.unwrap_or(INVALID_HANDLE_VALUE);
    let mut written: u32 = 0;
    let mut remaining = message;
    while !remaining.is_empty() {
        // If we get an error, it means we tried to write to an invalid handle (GUI Application)
        // and we should try to write to a window instead
        if unsafe { WriteFile(handle, Some(remaining.as_bytes()), Some(&mut written), None) }
            .is_err()
        {
            let nul_terminated = unsafe { CString::new(message.as_bytes()).unwrap_unchecked() };
            let pcstr_message = PCSTR::from_raw(nul_terminated.as_ptr() as *const _);
            unsafe { MessageBoxA(None, pcstr_message, None, MESSAGEBOX_STYLE(0)) };
            return;
        }
        if let Some(out) = remaining.get(written as usize..) {
            remaining = out
        }
    }
}
