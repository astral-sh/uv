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
/// The Armv8-A baseline for cross-installs and interpreters without usable CPU information.
const PLATFORM_DEFAULT_VERSION: Aarch64Version = Aarch64Version::V8;
/// Cached properties indexed by the maximum supported [`Aarch64Version`].
static VERSION_PROPERTIES: LazyLock<[SupportedVariantFeature; 8]> = LazyLock::new(|| {
    let versions = [
        (Aarch64Version::V8, "8a"),
        (Aarch64Version::V8_1, "8.1a"),
        (Aarch64Version::V8_2, "8.2a"),
        (Aarch64Version::V8_3, "8.3a"),
        (Aarch64Version::V8_4, "8.4a"),
        (Aarch64Version::V8_5, "8.5a"),
        (Aarch64Version::V8_6, "8.6a"),
        (Aarch64Version::V9_0, "9.0a"),
    ];
    versions.map(|(version, _)| SupportedVariantFeature {
        name: VariantFeature::from_static("version"),
        values: versions
            .into_iter()
            .rev()
            .filter(|(candidate, _)| version.supports(*candidate))
            .map(|(_, value)| VariantValue::from_static(value))
            .collect(),
    })
});

/// The vendored implementation of `aarch64-variant-provider` 0.2.0.
///
/// # Variant properties
///
/// - `aarch64::version` lists the Arm architecture versions supported by every core, newest
///   first. For example, `9.0a` also supports `8.5a` and earlier versions, but not `8.6a`.
///
/// When CPU detection is unavailable, including for an explicit target (`--python-platform`),
/// AArch64 targets use `8a` as the platform default. Other architectures emit no properties.
///
/// # Known limitations
///
/// - On Linux, unrecognized core models use `8a`. Windows ARM64 also uses `8a`.
/// - Linux does not expose [SPECRES] to user space. Even an `8.5a` or newer Linux wheel
///   cannot rely on prediction-restriction instructions such as `CFP RCTX`.
/// - On macOS, uv reads the Apple chip name like [archspec]. M-series chips use `8.4a` because
///   macOS can omit instructions required by `8.5a`, including SPECRES and SSBS. Newer chips
///   can use higher-level wheels once the interpreter probe checks those capabilities.
/// - The [WheelNext provider] caps newer Armv9 targets at `9.0a` and lists its ancestors.
///   For example, the Armv9.2-A Neoverse V3AE supports `8.6a`, but the provider's list
///   goes directly from `9.0a` to `8.5a`.
/// - The cached CPU information belongs to the interpreter's machine. Sharing its cache or
///   deploying its environment to a machine with a different CPU can select an incompatible wheel.
///
/// [archspec]: https://github.com/archspec/archspec/blob/ebd3eb502d9205f1033ffc028d661f2fbece7dfc/archspec/cpu/detect.py#L314-L334
/// [SPECRES]: https://github.com/ARM-software/acle/pull/382
/// [WheelNext provider]: https://github.com/wheelnext/aarch64-variant-provider/blob/v0.2.0/aarch64_variant_provider/plugin.py#L106-L115
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Aarch64Version {
    V8,
    V8_1,
    V8_2,
    V8_3,
    V8_4,
    V8_5,
    V8_6,
    V9_0,
}

impl Aarch64Version {
    fn detect(cpu_info: &CpuInfo) -> Self {
        match cpu_info {
            CpuInfo::AppleSilicon { brand } => {
                let Some(model) = brand.strip_prefix("Apple ") else {
                    return PLATFORM_DEFAULT_VERSION;
                };
                // Early M1 systems report "Apple processor".
                // TODO(konsti): Check the features macOS exposes before advertising 8.5a or newer.
                match model.split_ascii_whitespace().next() {
                    Some("processor" | "M1" | "M2" | "M3" | "M4" | "M5") => Self::V8_4,
                    Some(model)
                        if model.starts_with('M')
                            && model.as_bytes().get(1).is_some_and(u8::is_ascii_digit) =>
                    {
                        debug!("Unknown Apple chip `{brand}`, assuming Armv8.4-A");
                        Self::V8_4
                    }
                    _ => PLATFORM_DEFAULT_VERSION,
                }
            }
            CpuInfo::LinuxAarch64 { cores, hwcaps } => {
                // The kernel exposes process-wide capabilities. A wheel must also work on the
                // least capable core, including on systems with different kinds of cores.
                cores
                    .iter()
                    .map(|(implementer, part)| {
                        linux_core_ceiling(implementer, part)
                            .unwrap_or(PLATFORM_DEFAULT_VERSION)
                            .with_linux_hwcaps(hwcaps[0], hwcaps[1])
                    })
                    .reduce(Self::common)
                    .unwrap_or(PLATFORM_DEFAULT_VERSION)
            }
            CpuInfo::Flags { .. } | CpuInfo::Cpuid { .. } => PLATFORM_DEFAULT_VERSION,
        }
    }

    /// Armv9.0-A descends from Armv8.5-A, so it does not imply Armv8.6-A.
    fn supports(self, version: Self) -> bool {
        version as usize <= self as usize && !(self == Self::V9_0 && version == Self::V8_6)
    }

    /// Return the newest architecture version supported by both cores.
    ///
    /// Armv8.6-A and Armv9.0-A share Armv8.5-A as their newest common version.
    fn common(self, other: Self) -> Self {
        if self.supports(other) {
            other
        } else if other.supports(self) {
            self
        } else {
            Self::V8_5
        }
    }

    /// Limit the model's ISA to the user-space extensions enabled by the kernel.
    fn with_linux_hwcaps(self, hwcap: u64, hwcap2: u64) -> Self {
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
            // SSBS, SB, DC CVADP, FlagM2, FRINT. BTI instructions are NOPs without enforcement.
            (
                Self::V8_5,
                has_bits(hwcap, &[28, 29]) && has_bits(hwcap2, &[0, 7, 8]),
            ),
        ] {
            if !self.supports(next) || !available {
                return level;
            }
            level = next;
        }
        if self == Self::V9_0 && has_bits(hwcap, &[9, 10, 22]) && has_bits(hwcap2, &[1]) {
            // FP16, AdvSIMD HP, SVE, SVE2. A VM can mask SVE2 while still supporting Armv8.5-A.
            Self::V9_0
        } else if self == Self::V8_6 && has_bits(hwcap2, &[13, 14, 19]) {
            // I8MM, BF16, ECV.
            Self::V8_6
        } else {
            level
        }
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

/// Return the generic ISA ceiling for a Linux application processor with a known MIDR.
///
/// Unrecognized core models use the Armv8-A baseline. The ceiling is capped at the highest wheel
/// version (9.0a). M2 uses the upstream provider's 8.5a ceiling on Linux.
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
        ("0x41", 0xd49 | 0xd4f | 0xd83) => Aarch64Version::V9_0, // Neoverse N2, V2, V3AE
        ("0x6d", 0xd49) => Aarch64Version::V9_0, // Azure Cobalt 100 (Neoverse N2)
        ("0x42", 0x516) | ("0x43", 0x0af) => Aarch64Version::V8_1, // ThunderX2
        ("0x46", 0x001) => Aarch64Version::V8_2, // A64FX
        ("0xc0", 0xac3 | 0xac4) => Aarch64Version::V8_6, // AmpereOne, AmpereOneA
        ("0x61", 0x020..=0x025 | 0x028..=0x029) => Aarch64Version::V8_4, // Firestorm / Icestorm
        ("0x61", 0x030..=0x035 | 0x038..=0x039) => Aarch64Version::V8_5, // Avalanche / Blizzard
        // M3, M4, M5, including their efficiency and performance cores.
        (
            "0x61",
            0x042..=0x045
            | 0x048..=0x049
            | 0x052..=0x055
            | 0x058..=0x059
            | 0x062..=0x065
            | 0x068..=0x069,
        ) => Aarch64Version::V8_6,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use uv_platform_tags::CpuInfo;

    use super::Aarch64Version;

    /// Disabling BTI enforcement still lets a Neoverse N2 run Armv9.0-A wheels. Masking SVE2
    /// or sharing the workload with an Armv8.2-A Neoverse N1 limits the usable version.
    #[test]
    fn linux_version_requires_usable_features_on_every_core() {
        let n2 = ("0x41".into(), "0xd49".into());
        let n1 = ("0x41".into(), "0xd0c".into());
        let cobalt_without_bti = CpuInfo::LinuxAarch64 {
            cores: vec![("0x6d".into(), "0xd49".into())],
            hwcaps: [u64::MAX, u64::MAX ^ (1 << 17), 0, 0],
        };
        let masked = CpuInfo::LinuxAarch64 {
            cores: vec![n2.clone()],
            hwcaps: [u64::MAX, u64::MAX ^ (1 << 1), 0, 0],
        };
        let mixed = CpuInfo::LinuxAarch64 {
            cores: vec![n2, n1],
            hwcaps: [u64::MAX, u64::MAX, 0, 0],
        };

        assert_eq!(
            Aarch64Version::detect(&cobalt_without_bti),
            Aarch64Version::V9_0
        );
        assert!(!Aarch64Version::V9_0.supports(Aarch64Version::V8_6));
        assert_eq!(Aarch64Version::detect(&masked), Aarch64Version::V8_5);
        assert_eq!(Aarch64Version::detect(&mixed), Aarch64Version::V8_2);
    }
}
