use std::convert::Infallible;
use std::ffi::CString;
use std::io::Write;
use std::os::windows::io::AsRawHandle;
use std::string::String;

use ufmt_write::uWrite;
use windows::core::PCSTR;
use windows::Win32::UI::WindowsAndMessaging::{MessageBoxA, MESSAGEBOX_STYLE};

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
    let mut stderr = std::io::stderr();
    if !stderr.as_raw_handle().is_null() {
        let _ = stderr.write_all(message.as_bytes());
    } else {
        let nul_terminated = unsafe { CString::new(message.as_bytes()).unwrap_unchecked() };
        let pcstr_message = PCSTR::from_raw(nul_terminated.as_ptr() as *const _);
        unsafe { MessageBoxA(None, pcstr_message, None, MESSAGEBOX_STYLE(0)) };
    }
}
