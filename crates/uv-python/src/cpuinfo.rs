//! Fetches CPU information.

use anyhow::Error;

#[cfg(target_os = "linux")]
use procfs::{CpuInfo, Current};

/// Detects whether the hardware supports floating-point operations using ARM's Vector Floating Point (VFP) hardware.
///
/// This function is relevant specifically for ARM architectures, where the presence of the "vfp" flag in `/proc/cpuinfo`
/// indicates that the CPU supports hardware floating-point operations.
/// This helps determine whether the system is using the `gnueabihf` (hard-float) ABI or `gnueabi` (soft-float) ABI.
///
/// More information on this can be found in the [Debian ARM Hard Float Port documentation](https://wiki.debian.org/ArmHardFloatPort#VFP).
#[cfg(target_os = "linux")]
pub(crate) fn detect_hardware_floating_point_support() -> Result<bool, Error> {
    let cpu_info = CpuInfo::current()?;
    if let Some(features) = cpu_info.fields.get("Features") {
        if features.contains("vfp") {
            return Ok(true); // "vfp" found: hard-float (gnueabihf) detected
        }
    }

    Ok(false) // Default to soft-float (gnueabi) if no "vfp" flag is found
}

/// For non-Linux systems or architectures, the function will return `false` as hardware floating-point detection
/// is not applicable outside of Linux ARM architectures.
#[cfg(not(target_os = "linux"))]
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn detect_hardware_floating_point_support() -> Result<bool, Error> {
    Ok(false) // Non-Linux or non-ARM systems: hardware floating-point detection is not applicable
}
