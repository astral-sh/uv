use std::sync::LazyLock;

use tracing::debug;
use uv_distribution_types::{
    SupportedVariantFeature, VariantFeature, VariantNamespace, VariantValue,
};
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_platform_tags::{Arch, CpuInfo, Platform};

use super::{PluginApi, VendoredVariantProvider};

const NAMESPACE: &VariantNamespace = &VariantNamespace::from_static("aarch64");
static PACKAGE_NAME: LazyLock<PackageName> = LazyLock::new(|| {
    "aarch64-variant-provider"
        .parse()
        .expect("valid provider name")
});
/// The default version for cross-installs and interpreters without usable CPU information.
const PLATFORM_DEFAULT_VERSION: Aarch64Version = Aarch64Version::V8;
/// Cached properties indexed by the maximum supported [`Aarch64Version`].
static VERSION_PROPERTIES: LazyLock<[SupportedVariantFeature; 5]> = LazyLock::new(|| {
    let values = ["8a", "8.1a", "8.2a", "8.3a", "8.4a"].map(VariantValue::from_static);
    std::array::from_fn(|version| {
        (
            VariantFeature::from_static("version"),
            values[..=version].iter().rev().cloned().collect(),
        )
    })
});

/// The vendored implementation of `aarch64-variant-provider` 0.2.0.
///
/// # Variant properties
///
/// - `aarch64::version` lists the Arm architecture versions supported by every core, newest
///   first. For example, `8.4a` also supports `8.3a` through `8a`.
///
/// When CPU detection is unavailable, including for an explicit target (`--python-platform`),
/// AArch64 targets use `8a` as the platform default. Other architectures emit no properties.
///
/// # Known limitations
///
/// - On Linux, unrecognized core models use `8a`. Windows ARM64 also uses `8a`.
/// - On Linux, uv advertises at most `8.4a` because Linux does not expose [SPECRES] to user space.
/// - On macOS, uv reads the Apple chip name like [archspec] and advertises up to `8.4a` for
///   M1 through M5. The [generic Armv8.5-A target] enables SSBS and prediction-restriction
///   instructions such as `CFP RCTX`. For example, [macOS on M4 Max] reports neither feature.
///   [XNU] checks both hardware support and user-space access before reporting SPECRES.
///   Even a chip with BF16 and I8MM needs the `8.5a` instructions to run generic `8.6a` wheels.
/// - Unrecognized Apple chips use `8a`. The brand `Apple processor` was reported by both early
///   M1 systems and the [A12Z Developer Transition Kit], so it cannot identify an M1.
/// - The cached CPU information belongs to the interpreter's machine. Sharing its cache or
///   deploying its environment to a machine with a different CPU can select an incompatible wheel.
///
/// [archspec]: https://github.com/archspec/archspec/blob/ebd3eb502d9205f1033ffc028d661f2fbece7dfc/archspec/cpu/detect.py#L314-L334
/// [generic Armv8.5-A target]: https://gcc.gnu.org/onlinedocs/gcc/AArch64-Options.html
/// [macOS on M4 Max]:
///   https://github.com/archspec/archspec-json/blob/90b453f/tests/targets/darwin-sequoia-m4#L36-L68
/// [XNU]:
///   https://github.com/apple-oss-distributions/xnu/blob/f6217f8/osfmk/arm/commpage/commpage.c#L463-L475
/// [A12Z Developer Transition Kit]: https://stackoverflow.com/q/63214720
/// [SPECRES]: https://github.com/ARM-software/acle/pull/382
pub(super) struct Aarch64VariantProvider;

impl VendoredVariantProvider for Aarch64VariantProvider {
    fn namespace(&self) -> &VariantNamespace {
        NAMESPACE
    }

    fn plugin_api(&self) -> Option<&PluginApi> {
        None
    }

    fn package_name(&self) -> &PackageName {
        &PACKAGE_NAME
    }

    fn package_version(&self) -> Version {
        Version::new([0, 2, 0])
    }

    fn properties(&self, target: &Platform) -> Option<&[SupportedVariantFeature]> {
        if target.arch() != Arch::Aarch64 {
            return Some(&[]);
        }
        let version = target
            .cpu_info()
            .map_or(PLATFORM_DEFAULT_VERSION, Aarch64Version::detect);
        let property = &VERSION_PROPERTIES[version as usize];
        Some(std::slice::from_ref(property))
    }
}

/// An Arm A-profile architecture version recognized by the wheel variant provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Aarch64Version {
    V8,
    V8_1,
    V8_2,
    V8_3,
    V8_4,
}

impl Aarch64Version {
    fn detect(cpu_info: &CpuInfo) -> Self {
        match cpu_info {
            CpuInfo::AppleSilicon { brand } => {
                let Some(model) = brand.strip_prefix("Apple ") else {
                    return PLATFORM_DEFAULT_VERSION;
                };
                if let Some("M1" | "M2" | "M3" | "M4" | "M5") =
                    model.split_ascii_whitespace().next()
                {
                    Self::V8_4
                } else {
                    debug!("Unrecognized Apple chip `{brand}`, using the Armv8-A default");
                    PLATFORM_DEFAULT_VERSION
                }
            }
            CpuInfo::LinuxAarch64 { cores, hwcaps } => {
                // The kernel exposes process-wide capabilities. A wheel must also work on the
                // least capable core, including on systems with different kinds of cores.
                cores
                    .iter()
                    .map(|(implementer, part)| {
                        linux_core_ceiling(implementer, part).unwrap_or(PLATFORM_DEFAULT_VERSION)
                    })
                    .min()
                    .unwrap_or(PLATFORM_DEFAULT_VERSION)
                    .with_linux_hwcaps(hwcaps[0])
            }
            CpuInfo::Flags { .. } | CpuInfo::Cpuid { .. } => PLATFORM_DEFAULT_VERSION,
        }
    }

    /// Limit the model's ISA to the user-space extensions enabled by the kernel.
    fn with_linux_hwcaps(self, hwcap: u64) -> Self {
        // Linux arm64 ELF HWCAP bit numbers and their architectural meanings:
        // https://docs.kernel.org/arch/arm64/elf_hwcaps.html
        // https://github.com/torvalds/linux/blob/v6.16/arch/arm64/include/uapi/asm/hwcap.h
        // GCC's AArch64 options and LLVM's AArch64Features.td define the generic -march targets.
        // https://gcc.gnu.org/onlinedocs/gcc/AArch64-Options.html
        // https://github.com/llvm/llvm-project/tree/ab7245d75cc9/llvm/lib/Target/AArch64
        let mut level = Self::V8;
        for (next, available) in [
            // FP, AdvSIMD, CRC32, atomics, RDM.
            (Self::V8_1, has_bits(hwcap, &[0, 1, 7, 8, 12])),
            // DC CVAP.
            (Self::V8_2, has_bits(hwcap, &[16])),
            // JSCVT, FCMA, LRCPC, address and generic pointer authentication.
            (Self::V8_3, has_bits(hwcap, &[13, 14, 15, 30, 31])),
            // FP16, AdvSIMD HP, dot product, FHM, DIT, LSE2, LRCPC2, FlagM.
            (
                Self::V8_4,
                has_bits(hwcap, &[9, 10, 20, 23, 24, 25, 26, 27]),
            ),
            // Stop at Armv8.4-A. Generic Armv8.5-A and newer wheels can use SPECRES
            // instructions such as CFP RCTX, which Linux does not expose to user space.
        ] {
            if next > self || !available {
                return level;
            }
            level = next;
        }
        level
    }
}

/// Whether a Linux hardware capability word contains every requested bit.
fn has_bits(word: u64, bits: &[u8]) -> bool {
    for bit in bits {
        if word & (1 << bit) == 0 {
            return false;
        }
    }
    true
}

/// Return the highest Linux wheel version for a known core before checking the process HWCAPs.
///
/// Linux and XNU publish the processor IDs. LLVM identifies their compiler targets.
/// <https://github.com/torvalds/linux/blob/v6.16/arch/arm64/include/asm/cputype.h>
/// <https://github.com/apple-oss-distributions/xnu/blob/xnu-12377.121.6/osfmk/arm/cpuid.h>
/// <https://github.com/llvm/llvm-project/blob/ab7245d75cc9/llvm/lib/TargetParser/Host.cpp>
/// <https://github.com/llvm/llvm-project/blob/ab7245d/llvm/lib/Target/AArch64/AArch64Processors.td>
fn linux_core_ceiling(implementer: &str, part: &str) -> Option<Aarch64Version> {
    let part = u16::from_str_radix(part.strip_prefix("0x")?, 16).ok()?;
    Some(match (implementer, part) {
        ("0x41", 0xd0c) => Aarch64Version::V8_2, // Neoverse N1
        ("0x41", 0xd40) => Aarch64Version::V8_4, // Neoverse V1
        ("0x41", 0xd49 | 0xd4f | 0xd83) => Aarch64Version::V8_4, // Neoverse N2, V2, V3AE
        ("0x6d", 0xd49) => Aarch64Version::V8_4, // Azure Cobalt 100 (Neoverse N2)
        ("0x42", 0x516) | ("0x43", 0x0af) => Aarch64Version::V8_1, // ThunderX2
        ("0x46", 0x001) => Aarch64Version::V8_2, // A64FX
        ("0xc0", 0xac3 | 0xac4) => Aarch64Version::V8_4, // AmpereOne, AmpereOneA
        ("0x61", 0x020..=0x025 | 0x028..=0x029) => Aarch64Version::V8_4, // Firestorm / Icestorm
        ("0x61", 0x030..=0x035 | 0x038..=0x039) => Aarch64Version::V8_4, // Avalanche / Blizzard
        // M3, M4, M5, including their efficiency and performance cores.
        (
            "0x61",
            0x042..=0x045
            | 0x048..=0x049
            | 0x052..=0x055
            | 0x058..=0x059
            | 0x062..=0x065
            | 0x068..=0x069,
        ) => Aarch64Version::V8_4,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use uv_platform_tags::CpuInfo;

    use super::Aarch64Version;

    /// Linux caps `AmpereOne` at Armv8.4-A even with all HWCAP bits set. A missing feature or a
    /// less capable core can lower the version further.
    #[test]
    fn linux_version_requires_usable_features_on_every_core() {
        let n1 = ("0x41".into(), "0xd0c".into());
        let cobalt = ("0x6d".into(), "0xd49".into());
        let ampere_one = CpuInfo::LinuxAarch64 {
            cores: vec![("0xc0".into(), "0xac3".into())],
            hwcaps: [u64::MAX; 4],
        };
        let masked = CpuInfo::LinuxAarch64 {
            cores: vec![cobalt.clone()],
            // HWCAP_ASIMDDP (dot product) is required for Armv8.4-A.
            hwcaps: [u64::MAX ^ (1 << 20), u64::MAX, 0, 0],
        };
        let mixed = CpuInfo::LinuxAarch64 {
            cores: vec![cobalt, n1],
            hwcaps: [u64::MAX, u64::MAX, 0, 0],
        };

        assert_eq!(Aarch64Version::detect(&ampere_one), Aarch64Version::V8_4);
        assert_eq!(Aarch64Version::detect(&masked), Aarch64Version::V8_3);
        assert_eq!(Aarch64Version::detect(&mixed), Aarch64Version::V8_2);
    }
}
