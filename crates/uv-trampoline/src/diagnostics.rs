use std::convert::Infallible;
use std::ffi::CString;
use std::io::Write;
use std::os::windows::io::AsRawHandle;
use std::string::String;

use ufmt_write::uWrite;
use windows::Win32::UI::WindowsAndMessaging::{MESSAGEBOX_STYLE, MessageBoxA};
use windows::core::PCSTR;

#[macro_export]
macro_rules! error {
    ($($tt:tt)*) => {{
        $crate::diagnostics::write_diagnostic(&$crate::format!($($tt)*), true);
    }}
}

#[macro_export]
macro_rules! warn {
    ($($tt:tt)*) => {{
        $crate::diagnostics::write_diagnostic(&$crate::format!($($tt)*), false);
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
pub(crate) fn write_diagnostic(message: &str, is_error: bool) {
    let prefix = if is_error { "error" } else { "warning" };
    let mut stderr = std::io::stderr();
    if !stderr.as_raw_handle().is_null() {
        let _ = stderr.write_all(prefix.as_bytes());
        let _ = stderr.write_all(b": ");
        let _ = stderr.write_all(message.as_bytes());
    } else if is_error {
        let error = format!("{}: {}", prefix, message);
        let nul_terminated = unsafe { CString::new(error).unwrap_unchecked() };
        let pcstr_message = PCSTR::from_raw(nul_terminated.as_ptr() as *const _);
        unsafe { MessageBoxA(None, pcstr_message, None, MESSAGEBOX_STYLE(0)) };
    }
}
