use std::sync::LazyLock;

use uv_distribution_types::{
    SupportedVariantFeature, VariantFeature, VariantNamespace, VariantValue,
};
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_platform_tags::{Arch, CpuInfo, Platform};

use super::{PluginApi, VendoredVariantProvider};

const NAMESPACE: &VariantNamespace = &VariantNamespace::from_static("aarch64");
const VERSION_VALUES: [(Aarch64Version, &str); 8] = [
    (Aarch64Version::V8, "8a"),
    (Aarch64Version::V8_1, "8.1a"),
    (Aarch64Version::V8_2, "8.2a"),
    (Aarch64Version::V8_3, "8.3a"),
    (Aarch64Version::V8_4, "8.4a"),
    (Aarch64Version::V8_5, "8.5a"),
    (Aarch64Version::V8_6, "8.6a"),
    (Aarch64Version::V9_0, "9.0a"),
];
static PACKAGE_NAME: LazyLock<PackageName> = LazyLock::new(|| {
    "aarch64-variant-provider"
        .parse()
        .expect("valid provider name")
});
static VERSION_PROPERTIES: LazyLock<[SupportedVariantFeature; 8]> = LazyLock::new(|| {
    VERSION_VALUES.map(|(version, _)| SupportedVariantFeature {
        name: VariantFeature::from_static("version"),
        values: VERSION_VALUES
            .into_iter()
            .rev()
            .filter(|(candidate, _)| version.supports(*candidate))
            .map(|(_, value)| VariantValue::from_static(value))
            .collect(),
    })
});

/// The `aarch64::version` property of `aarch64-variant-provider` 0.2.0.
///
/// An explicit AArch64 target or an unrecognized CPU supports the Armv8-A baseline.
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
            .and_then(Aarch64Version::detect)
            .unwrap_or(Aarch64Version::V8);
        Some(std::slice::from_ref(&VERSION_PROPERTIES[version as usize]))
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
    fn detect(cpu_info: &CpuInfo) -> Option<Self> {
        match cpu_info {
            CpuInfo::AppleSilicon { brand } => {
                let model = brand
                    .strip_prefix("Apple ")?
                    .split_ascii_whitespace()
                    .next()?;
                // The provider caps the M2 at 8.5a. Early M1 systems use the generic brand.
                match model {
                    "processor" | "M1" => Some(Self::V8_4),
                    "M2" => Some(Self::V8_5),
                    "M3" | "M4" | "M5" => Some(Self::V8_6),
                    _ => None,
                }
            }
            CpuInfo::LinuxAarch64 { cores, hwcaps } => {
                // The kernel exposes process-wide capabilities. A wheel must also work on the
                // least capable core, including on systems with different kinds of cores.
                cores
                    .iter()
                    .map(|(implementer, part)| {
                        linux_core_ceiling(implementer, part)
                            .unwrap_or(Self::V8)
                            .with_linux_hwcaps(hwcaps[0], hwcaps[1])
                    })
                    .reduce(Self::common)
            }
            CpuInfo::WindowsArm64 { .. } => Some(Self::V8),
            CpuInfo::Flags { .. } | CpuInfo::Cpuid { .. } => None,
        }
    }

    /// Armv9.0-A descends from Armv8.5-A, so it does not imply Armv8.6-A.
    fn supports(self, version: Self) -> bool {
        version as usize <= self as usize && !(self == Self::V9_0 && version == Self::V8_6)
    }

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
        let has = |word: u64, bits: &[u8]| bits.iter().all(|bit| word & (1 << bit) != 0);
        let sve2 = has(hwcap, &[9, 10, 22]) && has(hwcap2, &[1]); // FP16, AdvSIMD HP, SVE, SVE2.
        if self == Self::V9_0 && !sve2 {
            // A VM can expose a v9 core ID while masking its vector ISA.
            return Self::V8;
        }
        let mut level = Self::V8;
        for (next, available) in [
            // FP, AdvSIMD, CRC32, atomics, RDM.
            (Self::V8_1, has(hwcap, &[0, 1, 7, 8, 12])),
            // DC CVAP.
            (Self::V8_2, has(hwcap, &[16])),
            // JSCVT, FCMA, LRCPC, address and generic pointer authentication.
            (Self::V8_3, has(hwcap, &[13, 14, 15, 30, 31])),
            // FP16, AdvSIMD HP, dot product, FHM, DIT, LSE2, LRCPC2, FlagM.
            (Self::V8_4, has(hwcap, &[9, 10, 20, 23, 24, 25, 26, 27])),
            // SSBS, SB, DC CVADP, FlagM2, FRINT, BTI.
            (
                Self::V8_5,
                has(hwcap, &[28, 29]) && has(hwcap2, &[0, 7, 8, 17]),
            ),
        ] {
            if !self.supports(next) || !available {
                return level;
            }
            level = next;
        }
        if self == Self::V9_0 {
            Self::V9_0
        } else if self == Self::V8_6 && has(hwcap2, &[13, 14, 19]) {
            // I8MM, BF16, ECV.
            Self::V8_6
        } else {
            level
        }
    }
}

/// Return the generic ISA ceiling for a Linux application processor with a known MIDR.
///
/// Unrecognized core models use the Armv8-A baseline. The ceiling is capped at the highest wheel
/// version (9.0a). M2 uses the provider's 8.5a ceiling on both macOS and Linux.
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
        ("0x43", 0x0af) => Aarch64Version::V8_1, // ThunderX2
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

    #[test]
    fn linux_version_requires_usable_features_on_every_core() {
        let detect = |cores, hwcap2| {
            Aarch64Version::detect(&CpuInfo::LinuxAarch64 {
                cores,
                hwcaps: [u64::MAX, hwcap2, 0, 0],
            })
            .expect("known core")
        };
        // A Neoverse N2 is Armv9.0-A. Its CPU ID survives when a VM masks SVE2.
        let n2 = ("0x41".into(), "0xd49".into());
        let version = detect(vec![n2.clone()], u64::MAX);
        assert_eq!(version, Aarch64Version::V9_0);
        assert!(!version.supports(Aarch64Version::V8_6));
        assert_eq!(
            detect(vec![n2.clone()], u64::MAX ^ (1 << 1)),
            Aarch64Version::V8
        );

        // An N1 in the same system limits every wheel to Armv8.2-A.
        let n1 = ("0x41".into(), "0xd0c".into());
        assert_eq!(detect(vec![n2, n1], u64::MAX), Aarch64Version::V8_2);
    }
}
