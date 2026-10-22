#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "linux")]
mod profiles;

use std::sync::LazyLock;

use uv_distribution_types::{
    SupportedVariantFeature, VariantFeature, VariantNamespace, VariantValue,
};
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_platform_tags::{Arch, Platform};

#[cfg(target_os = "linux")]
use self::linux::values;
#[cfg(target_os = "macos")]
use self::macos::values;
use super::{PluginApi, VendoredVariantProvider};

const FEATURE: &str = "version";
const BASELINE: &str = "8a";

const M1: &[&str] = &["8.4a", "8.3a", "8.2a", "8.1a", "8a"];
const M2: &[&str] = &["8.5a", "8.4a", "8.3a", "8.2a", "8.1a", "8a"];
const M3: &[&str] = &["8.6a", "8.5a", "8.4a", "8.3a", "8.2a", "8.1a", "8a"];

const NAMESPACE: &VariantNamespace = &VariantNamespace::from_static("aarch64");
static PACKAGE_NAME: LazyLock<PackageName> = LazyLock::new(|| {
    "aarch64-variant-provider"
        .parse()
        .expect("valid provider name")
});

/// The vendored implementation of `aarch64-variant-provider` 0.2.0.
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

    fn properties(&self, cross_platform: Option<&Platform>) -> Option<&[SupportedVariantFeature]> {
        static HOST: LazyLock<[SupportedVariantFeature; 1]> = LazyLock::new(|| feature(values()));
        static PLATFORM_DEFAULTS: LazyLock<[SupportedVariantFeature; 1]> =
            LazyLock::new(|| feature([BASELINE]));

        // An explicit target identifies the architecture, but we cannot probe its CPU features.
        match cross_platform {
            None => Some(HOST.as_slice()),
            Some(platform) if platform.arch() == Arch::Aarch64 => {
                Some(PLATFORM_DEFAULTS.as_slice())
            }
            Some(_) => None,
        }
    }
}

fn feature(values: impl IntoIterator<Item = &'static str>) -> [SupportedVariantFeature; 1] {
    [SupportedVariantFeature {
        name: VariantFeature::from_static(FEATURE),
        values: values.into_iter().map(VariantValue::from_static).collect(),
    }]
}
