use std::arch::x86_64::__cpuid;
use std::sync::LazyLock;

use uv_distribution_types::{
    SupportedVariantFeature, VariantFeature, VariantNamespace, VariantValue,
};
use uv_normalize::PackageName;
use uv_pep440::Version;

use crate::variant::VendoredVariantProvider;

/// The microarchitecture level feature of `x86_64-variant-provider` 0.2.0.
///
/// See <https://github.com/wheelnext/x86_64-variant-provider/tree/v0.2.0>.
///
/// TODO(konsti): Report individual CPU extensions along with the microarchitecture level.
pub(super) struct X86_64VariantProvider;

impl VendoredVariantProvider for X86_64VariantProvider {
    fn namespace(&self) -> &VariantNamespace {
        static NAMESPACE: LazyLock<VariantNamespace> = LazyLock::new(|| {
            VariantNamespace::try_from("x86_64".to_owned()).expect("valid variant namespace")
        });
        &NAMESPACE
    }

    fn package_name(&self) -> &PackageName {
        static PACKAGE_NAME: LazyLock<PackageName> = LazyLock::new(|| {
            "x86-64-variant-provider"
                .parse()
                .expect("valid provider name")
        });
        &PACKAGE_NAME
    }

    fn package_version(&self) -> Version {
        Version::new([0, 2, 0])
    }

    fn properties(&self) -> Option<&[SupportedVariantFeature]> {
        static PROPERTIES: LazyLock<Option<Vec<SupportedVariantFeature>>> =
            LazyLock::new(|| Some(vec![level_feature(detect_level()?).ok()?]));
        PROPERTIES.as_deref()
    }
}

fn level_feature(level: u8) -> anyhow::Result<SupportedVariantFeature> {
    Ok((
        VariantFeature::try_from("level".to_owned())?,
        (1..=level)
            .rev()
            .map(|level| VariantValue::try_from(format!("v{level}")))
            .collect::<Result<_, _>>()?,
    ))
}

/// Detect the highest usable x86-64 microarchitecture level.
///
/// Rust's feature detection checks the operating system's extended register state support for
/// AVX and AVX-512 in addition to the processor's capabilities.
fn detect_level() -> Option<u8> {
    let has_lahf_sahf =
        __cpuid(0x8000_0000).eax >= 0x8000_0001 && (__cpuid(0x8000_0001).ecx & 1) != 0;
    if !(has_lahf_sahf
        && is_x86_feature_detected!("cmpxchg16b")
        && is_x86_feature_detected!("sse3")
        && is_x86_feature_detected!("ssse3")
        && is_x86_feature_detected!("sse4.1")
        && is_x86_feature_detected!("sse4.2")
        && is_x86_feature_detected!("popcnt"))
    {
        // The provider emits no properties when archspec reports the generic `x86_64` baseline.
        return None;
    }
    if !(is_x86_feature_detected!("avx")
        && is_x86_feature_detected!("avx2")
        && is_x86_feature_detected!("bmi1")
        && is_x86_feature_detected!("bmi2")
        && is_x86_feature_detected!("f16c")
        && is_x86_feature_detected!("fma")
        && is_x86_feature_detected!("lzcnt")
        && is_x86_feature_detected!("movbe")
        && is_x86_feature_detected!("xsave"))
    {
        return Some(2);
    }
    if !(is_x86_feature_detected!("avx512f")
        && is_x86_feature_detected!("avx512bw")
        && is_x86_feature_detected!("avx512cd")
        && is_x86_feature_detected!("avx512dq")
        && is_x86_feature_detected!("avx512vl"))
    {
        return Some(3);
    }
    Some(4)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supports_lower_levels_in_preference_order() -> anyhow::Result<()> {
        let (name, values) = level_feature(3)?;
        assert_eq!(name, VariantFeature::try_from("level".to_owned())?);
        assert_eq!(
            values.iter().map(VariantValue::as_str).collect::<Vec<_>>(),
            ["v3", "v2", "v1"]
        );
        Ok(())
    }
}
