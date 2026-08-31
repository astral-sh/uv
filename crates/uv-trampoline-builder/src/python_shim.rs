//! Prebuilt Python launchers, regenerated separately from normal uv builds.

use std::io::{self, Write};
use std::path::Path;

#[cfg(unix)]
use fs_err::os::unix::fs::OpenOptionsExt;

/// Return the launcher for this platform.
///
/// Linux and Android use statically linked musl launchers where available, so the
/// launcher does not depend on the host's C library. `ARMv7` can use the `ARMv6` launcher.
pub fn binary() -> Option<&'static [u8]> {
    cfg_select! {
        all(any(target_os = "linux", target_os = "android"), target_arch = "aarch64") => {
            Some(include_bytes!("../python-shims/uv-python-aarch64-unknown-linux-musl"))
        },
        all(any(target_os = "linux", target_os = "android"), target_arch = "x86_64") => {
            Some(include_bytes!("../python-shims/uv-python-x86_64-unknown-linux-musl"))
        },
        all(any(target_os = "linux", target_os = "android"), target_arch = "x86") => {
            Some(include_bytes!("../python-shims/uv-python-i686-unknown-linux-musl"))
        },
        all(target_os = "linux", target_arch = "arm", target_abi = "eabihf") => {
            Some(include_bytes!("../python-shims/uv-python-arm-unknown-linux-musleabihf"))
        },
        all(target_os = "linux", target_arch = "powerpc64", target_endian = "little") => {
            Some(include_bytes!("../python-shims/uv-python-powerpc64le-unknown-linux-musl"))
        },
        all(target_os = "linux", target_arch = "riscv64") => {
            Some(include_bytes!("../python-shims/uv-python-riscv64gc-unknown-linux-musl"))
        },
        all(target_os = "linux", target_arch = "s390x") => {
            Some(include_bytes!("../python-shims/uv-python-s390x-unknown-linux-gnu"))
        },
        all(target_os = "macos", target_arch = "aarch64") => {
            Some(include_bytes!("../python-shims/uv-python-aarch64-apple-darwin"))
        },
        all(target_os = "macos", target_arch = "x86_64") => {
            Some(include_bytes!("../python-shims/uv-python-x86_64-apple-darwin"))
        },
        all(target_os = "freebsd", target_arch = "x86_64") => {
            Some(include_bytes!("../python-shims/uv-python-x86_64-unknown-freebsd"))
        },
        all(windows, target_arch = "aarch64") => {
            Some(include_bytes!("../python-shims/uv-python-aarch64-pc-windows-msvc.exe"))
        },
        all(windows, target_arch = "x86_64") => {
            Some(include_bytes!("../python-shims/uv-python-x86_64-pc-windows-msvc.exe"))
        },
        all(windows, target_arch = "x86") => {
            Some(include_bytes!("../python-shims/uv-python-i686-pc-windows-msvc.exe"))
        },
        _ => None,
    }
}

/// Write an embedded launcher without replacing an existing path.
pub fn write_to_path(path: &Path) -> io::Result<()> {
    let binary = binary().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::Unsupported,
            "Python shims are not supported on this platform",
        )
    })?;
    let mut options = fs_err::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o755);
    options.open(path)?.write_all(binary)
}

/// Return whether a path contains the currently embedded Python launcher.
pub fn is_python_shim(path: &Path) -> io::Result<bool> {
    let Some(binary) = binary() else {
        return Ok(false);
    };
    if fs_err::metadata(path)?.len() != binary.len() as u64 {
        return Ok(false);
    }
    Ok(fs_err::read(path)? == binary)
}
