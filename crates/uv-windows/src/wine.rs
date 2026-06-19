//! Wine detection for Windows builds running under the Wine compatibility layer.
//!
//! Wine implements the Win32 API on top of a Unix host. Most of the API is
//! transparent to applications, but a handful of low-level filesystem
//! operations (notably the reparse-point ioctl used to create NTFS junctions)
//! are unimplemented, so callers may want to take a different code path when
//! they detect Wine. See <https://github.com/astral-sh/uv/issues/19187>.
//!
//! Detection is based on the canonical method recommended by the Wine
//! project: probing `ntdll.dll` for the Wine-only `wine_get_version` export.

#![cfg(feature = "std")]

use std::ptr::{addr_of_mut, null_mut};
use std::sync::OnceLock;

use windows::Win32::Foundation::HMODULE;
use windows::Win32::System::LibraryLoader::{
    GET_MODULE_HANDLE_EX_FLAG_PIN, GetModuleHandleExA, GetProcAddress,
};
use windows::core::s;

/// Returns whether the current process is running under Wine.
///
/// The result is cached after the first call.
#[must_use]
pub fn is_wine() -> bool {
    static IS_WINE: OnceLock<bool> = OnceLock::new();
    *IS_WINE.get_or_init(detect_wine)
}

/// Detect Wine by probing `ntdll.dll` for the `wine_get_version` export, which
/// is present in Wine's `ntdll` but not in the genuine Windows DLL.
#[allow(unsafe_code)]
fn detect_wine() -> bool {
    let mut ntdll = HMODULE(null_mut());

    // SAFETY: `GetModuleHandleExA` is safe to call with a static,
    // NUL-terminated ASCII module name and a valid out-pointer. It returns an
    // error if the module isn't loaded; we treat that as "not Wine" and bail
    // out. `GET_MODULE_HANDLE_EX_FLAG_PIN` ensures the returned handle cannot be
    // invalidated by unloading the module later.
    if unsafe {
        GetModuleHandleExA(
            GET_MODULE_HANDLE_EX_FLAG_PIN,
            s!("ntdll.dll"),
            addr_of_mut!(ntdll),
        )
    }
    .is_err()
    {
        return false;
    }

    // SAFETY: `GetProcAddress` is safe to call with a valid module handle and
    // a static, NUL-terminated ASCII export name. It returns `None` if the
    // export isn't present, which is the case on real Windows.
    unsafe { GetProcAddress(ntdll, s!("wine_get_version")) }.is_some()
}
