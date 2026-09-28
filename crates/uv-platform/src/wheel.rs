//! Detect wheel compatibility without querying a Python interpreter.

use std::io;
use std::process::Command;
use std::str::FromStr;

use uv_platform_tags::{Arch, Os, Platform};

use crate::libc::{LibcVersion, detect_linux_libc};

/// Detect the native platform for installing executables distributed in wheels.
pub fn wheel_platform() -> Result<Platform, io::Error> {
    let architecture = crate::Arch::from_env().family().to_string();
    let architecture = match architecture.as_str() {
        "armv5te" => "armv5tel",
        "arm" | "armv6" => "armv6l",
        "armv7" => "armv7l",
        "powerpc64le" => "ppc64le",
        "powerpc64" => "ppc64",
        "powerpc" => "ppc",
        "riscv64gc" => "riscv64",
        architecture => architecture,
    };
    let arch = Arch::from_str(architecture).map_err(io::Error::other)?;
    let os = match std::env::consts::OS {
        "windows" => Os::Windows,
        "linux" => match detect_linux_libc().map_err(io::Error::other)? {
            LibcVersion::Manylinux { major, minor } => Os::Manylinux {
                major: major.try_into().map_err(io::Error::other)?,
                minor: minor.try_into().map_err(io::Error::other)?,
            },
            LibcVersion::Musllinux { major, minor } => Os::Musllinux {
                major: major.try_into().map_err(io::Error::other)?,
                minor: minor.try_into().map_err(io::Error::other)?,
            },
        },
        "macos" => {
            let output = Command::new("/usr/bin/sw_vers")
                .arg("-productVersion")
                .output()?;
            if !output.status.success() {
                return Err(io::Error::other("Failed to determine macOS version"));
            }
            let version = std::str::from_utf8(&output.stdout).map_err(io::Error::other)?;
            let mut components = version.trim().split('.');
            let mut component = || {
                components
                    .next()
                    .ok_or_else(|| io::Error::other("Invalid macOS version"))?
                    .parse()
                    .map_err(io::Error::other)
            };
            Os::Macos {
                major: component()?,
                minor: component()?,
            }
        }
        os => {
            return Err(io::Error::other(format!(
                "Native wheel installation is not supported on {os}"
            )));
        }
    };
    Ok(Platform::new(os, arch))
}
