use std::path::Path;

#[cfg(windows)]
#[allow(unsafe_code)] // We need to do an FFI call through the windows-* crates.
fn get_binary_type(path: &Path) -> windows::core::Result<u32> {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Storage::FileSystem::GetBinaryTypeW;
    use windows_core::PCWSTR;

    // References:
    // https://github.com/denoland/deno/blob/01a6379505712be34ebf2cdc874fa7f54a6e9408/runtime/permissions/which.rs#L131-L154
    // https://github.com/conradkleinespel/rooster/blob/afa78dc9918535752c4af59d2f812197ad754e5a/src/quale.rs#L51-L77
    let mut binary_type = 0u32;
    let name = path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<u16>>();
    // SAFETY: winapi call
    unsafe { GetBinaryTypeW(PCWSTR(name.as_ptr()), &mut binary_type)? };
    Ok(binary_type)
}

/// Check whether a path in PATH is a valid executable.
///
/// Derived from `which`'s `Checker`.
pub fn is_executable(path: &Path) -> bool {
    #[cfg(any(unix, target_os = "wasi", target_os = "redox"))]
    {
        if rustix::fs::access(path, rustix::fs::Access::EXEC_OK).is_err() {
            return false;
        }
    }

    #[cfg(target_os = "windows")]
    {
        let Ok(file_type) = fs_err::symlink_metadata(path).map(|metadata| metadata.file_type())
        else {
            return false;
        };
        if !file_type.is_file() && !file_type.is_symlink() {
            return false;
        }
        if path.extension().is_none() && get_binary_type(path).is_err() {
            return false;
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        use std::os::unix::fs::PermissionsExt;

        if !fs_err::metadata(path)
            .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
        {
            return false;
        }
    }

    true
}
