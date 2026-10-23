use std::process::Command;
use std::sync::LazyLock;

use uv_distribution_types::{
    SupportedVariantFeature, VariantFeature, VariantNamespace, VariantValue,
};
use uv_normalize::PackageName;
use uv_pep440::Version;

use crate::variant::VendoredVariantProvider;

/// The Apple Silicon architecture version feature of `aarch64-variant-provider` 0.2.0.
///
/// See <https://github.com/wheelnext/provider-variant-aarch64/tree/v0.2.0>.
///
/// TODO(konsti): Report individual CPU extensions and support AArch64 hosts beyond Apple Silicon.
pub(super) struct Aarch64VariantProvider;

impl VendoredVariantProvider for Aarch64VariantProvider {
    fn namespace(&self) -> &VariantNamespace {
        static NAMESPACE: LazyLock<VariantNamespace> = LazyLock::new(|| {
            VariantNamespace::try_from("aarch64".to_owned()).expect("valid variant namespace")
        });
        &NAMESPACE
    }

    fn package_name(&self) -> &PackageName {
        static PACKAGE_NAME: LazyLock<PackageName> = LazyLock::new(|| {
            "aarch64-variant-provider"
                .parse()
                .expect("valid provider name")
        });
        &PACKAGE_NAME
    }

    fn package_version(&self) -> Version {
        Version::new([0, 2, 0])
    }

    fn properties(&self) -> Option<&[SupportedVariantFeature]> {
        static PROPERTIES: LazyLock<Option<Vec<SupportedVariantFeature>>> = LazyLock::new(|| {
            let output = Command::new("/usr/sbin/sysctl")
                .args(["-n", "machdep.cpu.brand_string"])
                .output()
                .ok()?;
            if !output.status.success() {
                return None;
            }
            let brand = std::str::from_utf8(&output.stdout).ok()?;
            Some(vec![
                version_feature(apple_architecture_minor(brand)?).ok()?,
            ])
        });
        PROPERTIES.as_deref()
    }
}

/// Map the Apple CPU model to its generic architecture in the provider's archspec database.
///
/// The database recognizes M1 through M4 and maps later models to the newest known model.
/// See <https://github.com/archspec/archspec-json/tree/90b453f4053b6b139d1d2cb78c463b02e71ef363>.
fn apple_architecture_minor(brand: &str) -> Option<u8> {
    let model = brand.split_once("Apple")?.1.trim_start();
    if model.trim() == "Processor" {
        return Some(4);
    }
    let generation = model
        .strip_prefix('M')
        .or_else(|| model.strip_prefix('m'))
        .and_then(|model| {
            model
                .split(|character: char| !character.is_ascii_digit())
                .next()
        })
        .and_then(|generation| generation.parse::<u32>().ok());
    Some(match generation {
        Some(1) => 4,
        Some(2) => 5,
        Some(3..) => 6,
        Some(0) | None => 0,
    })
}

fn version_feature(minor: u8) -> anyhow::Result<SupportedVariantFeature> {
    Ok((
        VariantFeature::try_from("version".to_owned())?,
        (1..=minor)
            .rev()
            .map(|minor| format!("8.{minor}a"))
            .chain(["8a".to_owned()])
            .map(VariantValue::try_from)
            .collect::<Result<_, _>>()?,
    ))
}

#[cfg(test)]
mod tests {
    use anyhow::Context;

    use super::*;

    #[test]
    fn apple_versions_follow_archspec_ancestors() -> anyhow::Result<()> {
        let minor = apple_architecture_minor("Apple M5").context("unrecognized CPU model")?;
        let (_, values) = version_feature(minor)?;
        assert_eq!(
            values.iter().map(VariantValue::as_str).collect::<Vec<_>>(),
            ["8.6a", "8.5a", "8.4a", "8.3a", "8.2a", "8.1a", "8a"]
        );
        Ok(())
    }
}
