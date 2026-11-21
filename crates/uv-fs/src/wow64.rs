//! Filesystem views of Windows executables under WOW64.
//!
//! See the [WOW64 file system redirector](https://learn.microsoft.com/en-us/windows/win32/winprog64/file-system-redirector).

use std::ffi::OsString;
use std::io;
use std::os::windows::ffi::OsStringExt;
use std::path::{Path, PathBuf};

use windows::Win32::Foundation::MAX_PATH;
use windows::Win32::System::SystemInformation::GetSystemWindowsDirectoryW;
use windows::Win32::System::WindowsProgramming::{SCS_32BIT_BINARY, SCS_64BIT_BINARY};

/// The filesystem view of a Windows executable.
#[derive(Debug, Clone, Copy)]
pub enum FileSystemView {
    Bit32,
    Bit64,
    Unknown,
}

impl FileSystemView {
    /// Determine the view used by the executable at `path`.
    pub fn for_executable(path: &Path) -> Self {
        match crate::which::get_binary_type(path) {
            Ok(SCS_32BIT_BINARY) => Self::Bit32,
            Ok(SCS_64BIT_BINARY) => Self::Bit64,
            _ => Self::Unknown,
        }
    }

    /// Whether this view may resolve a path differently from the current process.
    pub fn may_redirect(self, path: &Path) -> io::Result<bool> {
        let shared = match self {
            Self::Bit32 => cfg!(target_pointer_width = "32"),
            Self::Bit64 => cfg!(target_pointer_width = "64"),
            Self::Unknown => false,
        };
        if shared {
            return Ok(false);
        }
        may_be_redirected_by_wow64(path)
    }
}

fn may_be_redirected_by_wow64(path: &Path) -> io::Result<bool> {
    let mut buffer = [0; MAX_PATH as usize];
    // SAFETY: The binding supplies the capacity of this writable slice to Windows.
    #[expect(unsafe_code)]
    let len = unsafe { GetSystemWindowsDirectoryW(Some(&mut buffer)) } as usize;
    if len == 0 {
        return Err(io::Error::last_os_error());
    }
    if len >= buffer.len() {
        // A larger return value is the required capacity.
        return Ok(true);
    }
    let windows = PathBuf::from(OsString::from_wide(&buffer[..len]));
    if !windows.is_absolute() {
        return Ok(true);
    }
    let path = std::path::absolute(path)?;
    for ancestor in path.ancestors() {
        let Some(name) = ancestor.file_name() else {
            continue;
        };
        if ["System32", "Sysnative", "LastGood", "regedit.exe"]
            .iter()
            .any(|location| name.eq_ignore_ascii_case(location))
            && let Some(parent) = ancestor.parent()
            && same_file::is_same_file(parent, &windows)?
        {
            let suffix = path.strip_prefix(ancestor).expect("ancestor of path");
            if name.eq_ignore_ascii_case("LastGood") {
                return Ok(suffix
                    .iter()
                    .next()
                    .is_some_and(|name| name.eq_ignore_ascii_case("System32")));
            }
            if name.eq_ignore_ascii_case("System32") {
                return Ok(!wow64_shared_subdirectory(suffix));
            }
            return Ok(true);
        }
    }
    Ok(false)
}

/// Subdirectories of `System32` shared by both architectures on supported Windows versions.
fn wow64_shared_subdirectory(path: &Path) -> bool {
    let mut parts = path.iter();
    let Some(name) = parts.next() else {
        return false;
    };
    if name.eq_ignore_ascii_case("drivers") {
        return parts
            .next()
            .is_some_and(|next| next.eq_ignore_ascii_case("etc"));
    }
    if name.eq_ignore_ascii_case("DriverStore") {
        // Wine 11 lists `driversstore` among its exceptions, so it redirects `DriverStore`.
        // https://github.com/wine-mirror/wine/blob/wine-11.0/dlls/wow64/file.c#L124
        return !uv_windows::is_wine();
    }
    ["catroot", "catroot2", "logfiles", "spool"]
        .iter()
        .any(|shared| name.eq_ignore_ascii_case(shared))
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::path::Path;

    use super::may_be_redirected_by_wow64;

    #[test]
    fn sysnative_outside_windows_is_an_ordinary_directory() -> io::Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join(r"Sysnative\python.exe");
        assert!(!may_be_redirected_by_wow64(&path)?);
        Ok(())
    }

    #[test]
    fn only_etc_is_shared_under_system32_drivers() -> io::Result<()> {
        let windows = std::env::var_os("SystemRoot").expect("Windows directory");
        let drivers = Path::new(&windows).join(r"System32\drivers");
        assert!(may_be_redirected_by_wow64(&drivers.join("python.exe"))?);
        assert!(!may_be_redirected_by_wow64(
            &drivers.join(r"etc\python.exe")
        )?);
        Ok(())
    }
}
