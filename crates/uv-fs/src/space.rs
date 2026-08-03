use std::io;
use std::path::Path;

#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;

#[cfg(windows)]
use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
#[cfg(windows)]
use windows::core::PCWSTR;

/// Return the available space on the filesystem containing `path`, in bytes.
#[cfg_attr(windows, expect(unsafe_code))]
pub fn available_space(path: &Path) -> io::Result<u64> {
    #[cfg(unix)]
    {
        let statistics = rustix::fs::statvfs(path)?;
        Ok(statistics.f_bavail.saturating_mul(statistics.f_frsize))
    }

    #[cfg(windows)]
    {
        let path = path
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        let mut available = 0;

        // SAFETY: `path` is null-terminated and remains valid for the duration of the call, and
        // `available` is a valid, writable pointer to the requested output value.
        unsafe {
            GetDiskFreeSpaceExW(PCWSTR(path.as_ptr()), Some(&raw mut available), None, None)?;
        }

        Ok(available)
    }

    #[cfg(not(any(unix, windows)))]
    {
        let _ = path;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "filesystem space measurement is unsupported on this platform",
        ))
    }
}
