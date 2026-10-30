use std::collections::HashSet;
use std::sync::LazyLock;

use uv_distribution_types::{
    SupportedVariantFeature, VariantFeature, VariantNamespace, VariantValue,
};
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_platform_tags::{Arch, CpuInfo, Platform};

use super::{PluginApi, VendoredVariantProvider};

const NAMESPACE: &VariantNamespace = &VariantNamespace::from_static("x86_64");
static PACKAGE_NAME: LazyLock<PackageName> = LazyLock::new(|| {
    "x86-64-variant-provider"
        .parse()
        .expect("valid provider name")
});
/// The default value for cross-installs or when the CPU detection API wasn't available.
///
/// CPUs that aren't v2 are over 10 - 15 years old.
const PLATFORM_DEFAULT_LEVEL: Level = Level::V2;
/// Cached properties indexed by the maximum supported [`Level`].
static LEVEL_PROPERTIES: LazyLock<[SupportedVariantFeature; 4]> = LazyLock::new(|| {
    let values = ["v4", "v3", "v2", "v1"].map(VariantValue::from_static);
    [&values[3..], &values[2..], &values[1..], &values[..]].map(|values| SupportedVariantFeature {
        name: VariantFeature::from_static("level"),
        values: values.to_vec(),
    })
});

/// The vendored implementation of `x86_64-variant-provider` 0.2.0.
///
/// # Variant properties
///
/// - `x86_64::level` lists the supported microarchitecture levels (`v4`, `v3`, `v2`, `v1`), which
///   group instruction set extensions such as SIMD instructions. Each level implies the previous
///   levels are also compatible, e.g., if `v3` is emitted, `v2` and `v1` are also emitted.
///
/// When CPU detection is unavailable, including for an explicit target (`--python-platform`),
/// x86-64 targets use `v2` as the platform default, while other architectures emit nothing.
///
/// There are two cases we are not handling:
/// * The cache is shared between machines with CPUs with different `x86_64` levels, but the same
///   Python interpreter.
/// * We are building the environment on one machine, and then deploying to another machine. This
///   case requires an override specifying the target machine. TODO(konsti): Add support for that as
///   part of the wheel variants stack.
pub(super) struct X86_64VariantProvider;

impl VendoredVariantProvider for X86_64VariantProvider {
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
        if target.arch() != Arch::X86_64 {
            return Some(&[]);
        }
        let level = target
            .cpu_info()
            .map_or(PLATFORM_DEFAULT_LEVEL, Level::detect);
        let property = &LEVEL_PROPERTIES[level as usize];
        Some(std::slice::from_ref(property))
    }
}

/// An x86-64 microarchitecture level supported by the Python interpreter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Level {
    V1,
    V2,
    V3,
    V4,
}

impl Level {
    fn detect(cpu_info: &CpuInfo) -> Self {
        let ebx = 1;
        let ecx = 2;
        let extended = 0x8000_0001;
        // Each feature is a normalized OS flag and its CPUID leaf, register index, and bit.
        let levels: &[(Self, &[_])] = &[
            (
                Self::V2,
                &[
                    ("cx16", 1, ecx, 13),
                    ("lahf_lm", extended, ecx, 0),
                    ("popcnt", 1, ecx, 23),
                    ("sse3", 1, ecx, 0),
                    ("ssse3", 1, ecx, 9),
                    ("sse4_1", 1, ecx, 19),
                    ("sse4_2", 1, ecx, 20),
                ],
            ),
            (
                Self::V3,
                &[
                    ("avx", 1, ecx, 28),
                    ("avx2", 7, ebx, 5),
                    ("bmi1", 7, ebx, 3),
                    ("bmi2", 7, ebx, 8),
                    ("f16c", 1, ecx, 29),
                    ("fma", 1, ecx, 12),
                    ("lzcnt", extended, ecx, 5),
                    ("movbe", 1, ecx, 22),
                ],
            ),
            (
                Self::V4,
                &[
                    ("avx512f", 7, ebx, 16),
                    ("avx512bw", 7, ebx, 30),
                    ("avx512cd", 7, ebx, 28),
                    ("avx512dq", 7, ebx, 17),
                    ("avx512vl", 7, ebx, 31),
                ],
            ),
        ];

        match cpu_info {
            CpuInfo::Flags { flags } => {
                let flags = flags.to_ascii_lowercase();
                // The LAHF, SSE4.1, SSE4.2, and AVX1.0 macOS aliases follow archspec:
                // https://github.com/archspec/archspec-json/blob/90b453f4053b6b139d1d2cb78c463b02e71ef363/cpu/microarchitectures.json#L4586-L4594
                let flags: HashSet<_> = flags
                    .split_ascii_whitespace()
                    .map(|flag| match flag {
                        "pni" => "sse3",
                        "abm" => "lzcnt",
                        "lahf" => "lahf_lm",
                        "sse4.1" => "sse4_1",
                        "sse4.2" => "sse4_2",
                        "avx1.0" => "avx",
                        "avx2.0" => "avx2",
                        _ => flag,
                    })
                    .collect();
                let level = levels
                    .iter()
                    .take_while(|(_, features)| {
                        features.iter().all(|(name, ..)| flags.contains(name))
                    })
                    .last();
                level.map_or(Self::V1, |(level, _)| *level)
            }
            CpuInfo::Cpuid { leaves, xstate } => {
                // AVX requires XMM and YMM state (bits 1 and 2). AVX-512 additionally requires
                // opmask, ZMM_Hi256, and Hi16_ZMM state (bits 5, 6, and 7).
                let max_level = if xstate & 0b110 != 0b110 {
                    Self::V2
                } else if xstate & 0b1110_0000 != 0b1110_0000 {
                    Self::V3
                } else {
                    Self::V4
                };
                let mut detected = Self::V1;
                for &(level, features) in levels {
                    if level > max_level {
                        break;
                    }
                    for &(_, leaf, register, bit) in features {
                        let supported = leaves.iter().any(|entry| {
                            entry.leaf == leaf
                                && entry.subleaf == 0
                                && entry.registers[register] & (1 << bit) != 0
                        });
                        if !supported {
                            return detected;
                        }
                    }
                    detected = level;
                }
                detected
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use uv_platform_tags::{CpuInfo, CpuidLeaf};

    use super::Level;

    #[test]
    fn linux_flags() {
        let info = CpuInfo::Flags {
            flags: concat!(
                "cx16 lahf_lm popcnt pni ssse3 sse4_1 sse4_2 avx avx2 bmi1 bmi2 f16c ",
                "fma abm movbe"
            )
            .into(),
        };
        assert_eq!(Level::detect(&info), Level::V3);
    }

    #[test]
    fn macos_flags() {
        let info = CpuInfo::Flags {
            flags: concat!(
                "CX16 LAHF POPCNT SSE3 SSSE3 SSE4.1 SSE4.2 AVX1.0 ",
                "AVX2.0 BMI1 BMI2 F16C FMA LZCNT MOVBE"
            )
            .into(),
        };
        assert_eq!(Level::detect(&info), Level::V3);
    }

    /// CPUID support for x86-64-v4 is insufficient if the OS has not enabled AVX-512 register state.
    ///
    /// Use the same recorded v4 CPUID leaves with XCR0 masks for XMM/YMM only (0x6, v3) and
    /// with the additional AVX-512 state enabled (0xe6, v4).
    #[test]
    fn avx512_requires_enabled_register_state() {
        let leaves = vec![
            CpuidLeaf {
                leaf: 1,
                subleaf: 0,
                registers: [0x00a1_0f10, 0x0010_0800, 0xfffa_3203, 0x178b_fbff],
            },
            CpuidLeaf {
                leaf: 7,
                subleaf: 0,
                registers: [0x0000_0001, 0xf1bf_07ab, 0x0040_5f46, 0x0000_0010],
            },
            CpuidLeaf {
                leaf: 0x8000_0001,
                subleaf: 0,
                registers: [0x00a1_0f10, 0x0000_0000, 0x0040_03f3, 0x2fd3_fbff],
            },
        ];
        let limited = CpuInfo::Cpuid {
            leaves: leaves.clone(),
            xstate: 0x6,
        };
        let full = CpuInfo::Cpuid {
            leaves,
            xstate: 0xe6,
        };
        assert_eq!(Level::detect(&limited), Level::V3);
        assert_eq!(Level::detect(&full), Level::V4);
    }
}
