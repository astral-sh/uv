//! Reflink a file on macOS via `clonefile`.

use std::{
    ffi::CString,
    io,
    os::{
        raw::{c_char, c_int},
        unix::ffi::OsStrExt,
    },
    path::Path,
};

fn cstr(path: &Path) -> io::Result<CString> {
    Ok(CString::new(path.as_os_str().as_bytes())?)
}

// const CLONE_NOFOLLOW: c_int = 0x0001;
const CLONE_NOOWNERCOPY: c_int = 0x0002;

extern "C" {
    // http://www.manpagez.com/man/2/clonefileat/
    // https://github.com/apple/darwin-xnu/blob/0a798f6738bc1db01281fc08ae024145e84df927/bsd/sys/clonefile.h
    // TODO We need weak linkage here (OSX > 10.12, iOS > 10.0), otherwise compilation will fail on older versions
    fn clonefile(src: *const c_char, dest: *const c_char, flags: c_int) -> c_int;
}

pub(crate) fn reflink(from: &Path, to: &Path) -> io::Result<()> {
    let src = cstr(from)?;
    let dest = cstr(to)?;

    #[allow(unsafe_code)]
    let ret = unsafe { clonefile(src.as_ptr(), dest.as_ptr(), CLONE_NOOWNERCOPY) };

    if ret == -1 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
