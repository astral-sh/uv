#[cfg(target_arch = "x86_64")]
use std::arch::{is_x86_feature_detected, x86_64::__cpuid};
use std::sync::LazyLock;

use uv_distribution_types::{
    SupportedVariantFeature, VariantFeature, VariantNamespace, VariantValue,
};
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_platform_tags::{Arch, Platform};

use super::{PluginApi, VendoredVariantProvider};

const NAMESPACE: &VariantNamespace = &VariantNamespace::from_static("x86_64");
const LEVEL: VariantFeature = VariantFeature::from_static("level");
const LEVEL_VALUES: [VariantValue; 4] = [
    VariantValue::from_static("v1"),
    VariantValue::from_static("v2"),
    VariantValue::from_static("v3"),
    VariantValue::from_static("v4"),
];
static PACKAGE_NAME: LazyLock<PackageName> = LazyLock::new(|| {
    "x86-64-variant-provider"
        .parse()
        .expect("valid provider name")
});
// When we can't probe the target CPU, assume v2. Mainstream x86-64 CPUs that lack v2
// are more than a decade old.
static PLATFORM_DEFAULTS: LazyLock<[SupportedVariantFeature; 1]> = LazyLock::new(|| [levels(2)]);
static HOST_LEVELS: LazyLock<Option<SupportedVariantFeature>> = LazyLock::new(|| {
    cfg_select! {
        target_arch = "x86_64" => Some(levels(host_level())),
        _ => None,
    }
});

/// The vendored implementation of `x86_64-variant-provider` 0.2.0.
///
/// # Variant properties
///
/// - `x86_64::level` lists the microarchitecture levels the target supports (`v1` through `v4`),
///   highest first, so uv prefers the most specialized compatible wheel.
///
/// An explicit x86-64 target (`--python-platform`) uses the platform defaults, `v2` then `v1`.
/// Other target architectures have no x86-64 levels.
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

    fn properties(&self, cross_platform: Option<&Platform>) -> Option<&[SupportedVariantFeature]> {
        let properties = match cross_platform {
            None => HOST_LEVELS.as_slice(),
            Some(platform) if platform.arch() == Arch::X86_64 => PLATFORM_DEFAULTS.as_slice(),
            Some(_) => &[],
        };
        Some(properties)
    }
}

fn levels(max_level: u8) -> SupportedVariantFeature {
    SupportedVariantFeature {
        name: LEVEL,
        values: LEVEL_VALUES
            .into_iter()
            .take(usize::from(max_level))
            .rev()
            .collect(),
    }
}

/// Returns the highest x86-64 microarchitecture level the host supports.
///
/// The x86-64 target guarantees v1. Each higher level includes all features of the preceding
/// levels, so a missing feature rules out that level and every level above it.
/// Rust's AVX and AVX-512 probes also check OS support for vector registers.
/// On Intel macOS this can skip v4 because the kernel enables AVX-512 state on first use.
#[cfg(target_arch = "x86_64")]
fn host_level() -> u8 {
    let v2 = is_x86_feature_detected!("cmpxchg16b")
        && is_x86_feature_detected!("popcnt")
        && is_x86_feature_detected!("sse3")
        && is_x86_feature_detected!("ssse3")
        && is_x86_feature_detected!("sse4.1")
        && is_x86_feature_detected!("sse4.2")
        // Rust has no named probe for LAHF/SAHF in 64-bit mode (CPUID.80000001H:ECX[0]).
        && __cpuid(0x8000_0000).eax >= 0x8000_0001
        && __cpuid(0x8000_0001).ecx & 1 != 0;
    if !v2 {
        return 1;
    }

    let v3 = is_x86_feature_detected!("avx")
        && is_x86_feature_detected!("avx2")
        && is_x86_feature_detected!("bmi1")
        && is_x86_feature_detected!("bmi2")
        && is_x86_feature_detected!("f16c")
        && is_x86_feature_detected!("fma")
        && is_x86_feature_detected!("lzcnt")
        && is_x86_feature_detected!("movbe");
    if !v3 {
        return 2;
    }

    let v4 = is_x86_feature_detected!("avx512f")
        && is_x86_feature_detected!("avx512bw")
        && is_x86_feature_detected!("avx512cd")
        && is_x86_feature_detected!("avx512dq")
        && is_x86_feature_detected!("avx512vl");
    if v4 { 4 } else { 3 }
}
