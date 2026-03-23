//! Fetches CPU information.

use std::io::Error;

#[cfg(target_os = "linux")]
use procfs::{CpuInfo, Current};

/// Detects whether the hardware supports floating-point operations using ARM's Vector Floating Point (VFP) hardware.
///
/// This function is relevant specifically for ARM architectures, where the presence of the `vfp` flag in `/proc/cpuinfo`
/// indicates that the CPU supports hardware floating-point operations.
/// This helps determine whether the system is using the `gnueabihf` (hard-float) ABI or `gnueabi` (soft-float) ABI.
///
/// More information on this can be found in the [Debian ARM Hard Float Port documentation](https://wiki.debian.org/ArmHardFloatPort#VFP).
#[cfg(target_os = "linux")]
pub(crate) fn detect_hardware_floating_point_support() -> Result<bool, Error> {
    let cpu_info = CpuInfo::current().map_err(Error::other)?;
    if let Some(features) = cpu_info.fields.get("Features") {
        if has_hardware_float_features(features) {
            return Ok(true);
        }
    }

    Ok(false) // Default to soft-float (gnueabi) if no hardware float feature is found
}

/// Check if a `/proc/cpuinfo` `Features` string indicates hardware floating-point support.
///
/// On native ARM systems, the `vfp` flag indicates hardware floating-point support.
///
/// On an AArch64 kernel running ARM userspace (e.g., `armv7l` containers on AArch64 hosts,
/// or 32-bit Raspberry Pi OS on 64-bit hardware), `/proc/cpuinfo` reports AArch64-style
/// feature flags instead of ARM flags. The `fp` feature is mandatory on all AArch64
/// CPUs and indicates hardware floating-point support.
///
/// See: <https://github.com/astral-sh/uv/issues/18509>
#[cfg(target_os = "linux")]
fn has_hardware_float_features(features: &str) -> bool {
    features
        .split_whitespace()
        .any(|feature| feature == "vfp" || feature == "fp")
}

/// For non-Linux systems or architectures, the function will return `false` as hardware floating-point detection
/// is not applicable outside of Linux ARM architectures.
#[cfg(not(target_os = "linux"))]
#[expect(clippy::unnecessary_wraps)]
pub(crate) fn detect_hardware_floating_point_support() -> Result<bool, Error> {
    Ok(false) // Non-Linux or non-ARM systems: hardware floating-point detection is not applicable
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "linux")]
    use super::has_hardware_float_features;

    /// Native arm32 (e.g., Raspberry Pi with 32-bit kernel) — `vfp` flag present.
    #[test]
    #[cfg(target_os = "linux")]
    fn arm32_native_hard_float() {
        let features = "half thumb fastmult vfp edsp neon vfpv3 tls vfpv4 idiva idivt vfpd32 lpae evtstrm crc32";
        assert!(has_hardware_float_features(features));
    }

    /// An aarch64 kernel running arm32 userspace — no `vfp` flag, but `fp` is present.
    /// This is the scenario from <https://github.com/astral-sh/uv/issues/18509>.
    #[test]
    #[cfg(target_os = "linux")]
    fn aarch64_kernel_with_arm32_userspace() {
        let features =
            "fp asimd evtstrm aes pmull sha1 sha2 crc32 atomics fphp asimdhp cpuid asimdrdm";
        assert!(has_hardware_float_features(features));
    }

    /// arm32 without any floating-point support — neither `vfp` nor `fp`.
    #[test]
    #[cfg(target_os = "linux")]
    fn arm32_soft_float() {
        let features = "swp half thumb fastmult edsp";
        assert!(!has_hardware_float_features(features));
    }

    /// "fp" must match as a discrete token, not as a substring of other features.
    #[test]
    #[cfg(target_os = "linux")]
    fn fp_only_matches_as_discrete_token() {
        // "fphp" contains "fp" as a prefix but should not match on its own
        let features = "asimd fphp asimdhp";
        assert!(!has_hardware_float_features(features));
    }
}
