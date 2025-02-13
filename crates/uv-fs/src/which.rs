use std::path::Path;

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
        if path.extension().is_none()
            && winsafe::GetBinaryType(&path.display().to_string()).is_err()
        {
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
