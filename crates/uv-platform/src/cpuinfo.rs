//! Fetches CPU information.

use std::io::Error;

#[cfg(target_os = "linux")]
use procfs::{CpuInfo, Current};

#[cfg(target_os = "linux")]
use crate::ArchVariant;

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

/// Detects the POWER ISA generation of the current CPU by reading `/proc/cpuinfo`.
///
/// On IBM POWER machines the `cpu` field looks like:
/// - `POWER9 (architected), altivec supported`
/// - `POWER10 (architected), altivec supported`
/// - `Power11 (architected), altivec supported`
///
/// Returns the matching [`ArchVariant`] (Power9/Power10/Power11), or `None` if the
/// generation cannot be determined (e.g., older POWER, unknown string, or read error).
#[cfg(target_os = "linux")]
pub(crate) fn detect_power_cpu_generation() -> Option<ArchVariant> {
    let cpu_info = CpuInfo::current().ok()?;
    // get_info() returns Option<HashMap<&str,&str>>; fields is HashMap<String,String>.
    // Collect the cpu field value as an owned String to avoid lifetime issues.
    let cpu_str: String = cpu_info
        .get_info(0)
        .and_then(|info| info.get("cpu").map(|s| (*s).to_owned()))
        .or_else(|| cpu_info.fields.get("cpu").cloned())?;

    parse_power_generation(&cpu_str)
}

/// Parse a POWER generation out of a `/proc/cpuinfo` `cpu` field value.
///
/// Case-insensitive; matches `POWER9`, `POWER10`, `POWER11` (and mixed-case variants).
#[cfg(target_os = "linux")]
fn parse_power_generation(cpu: &str) -> Option<ArchVariant> {
    let lower = cpu.to_ascii_lowercase();
    if lower.contains("power11") {
        Some(ArchVariant::Power11)
    } else if lower.contains("power10") {
        Some(ArchVariant::Power10)
    } else if lower.contains("power9") {
        Some(ArchVariant::Power9)
    } else {
        None
    }
}

/// On non-Linux systems, Power generation detection is not applicable.
#[cfg(not(target_os = "linux"))]
pub(crate) fn detect_power_cpu_generation() -> Option<ArchVariant> {
    None
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "linux")]
    use super::{has_hardware_float_features, parse_power_generation};
    #[cfg(target_os = "linux")]
    use crate::ArchVariant;

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

    #[test]
    #[cfg(target_os = "linux")]
    fn power9_detected() {
        assert_eq!(
            parse_power_generation("POWER9 (architected), altivec supported"),
            Some(ArchVariant::Power9)
        );
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn power10_detected() {
        assert_eq!(
            parse_power_generation("POWER10 (architected), altivec supported"),
            Some(ArchVariant::Power10)
        );
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn power11_detected() {
        // PBS convention uses "Power11" (mixed case)
        assert_eq!(
            parse_power_generation("Power11 (architected), altivec supported"),
            Some(ArchVariant::Power11)
        );
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn power11_not_matched_as_power1() {
        // "power11" contains "power1" — make sure ordering is correct (check 11 before 9/10)
        assert_eq!(
            parse_power_generation("power11 foo"),
            Some(ArchVariant::Power11)
        );
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn unknown_power_returns_none() {
        assert_eq!(parse_power_generation("POWER8 SMT8 POWER8"), None);
        assert_eq!(parse_power_generation("unknown cpu"), None);
    }
}

/// Detects the POWER CPU generation on ppc64le Linux via `AT_PLATFORM` from the ELF auxiliary
/// vector. The kernel sets this to a string like `"power9"`, `"power10"`, or `"power11"`.
#[cfg(all(target_arch = "powerpc64", target_endian = "little", target_os = "linux"))]
pub(crate) fn detect_power_variant() -> Option<crate::arch::ArchVariant> {
    use std::ffi::c_char;

    let auxv = procfs::process::Process::myself()
        .and_then(|p| p.auxv())
        .ok()?;

    // AT_PLATFORM = 15: pointer to a null-terminated platform string in the process address space.
    let addr = *auxv.get(&15u64)? as *const c_char;
    if addr.is_null() {
        return None;
    }

    // SAFETY: AT_PLATFORM points to a valid null-terminated string for the lifetime of the process.
    #[allow(unsafe_code)]
    let platform = unsafe { std::ffi::CStr::from_ptr(addr) }.to_str().ok()?;

    match platform {
        "power9" => Some(crate::arch::ArchVariant::Power9),
        "power10" => Some(crate::arch::ArchVariant::Power10),
        "power11" => Some(crate::arch::ArchVariant::Power11),
        _ => None,
    }
}
