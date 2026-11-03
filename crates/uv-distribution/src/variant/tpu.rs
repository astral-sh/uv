// Ported from wheelnext/tpu-variant-provider, Copyright 2026 Google LLC (Apache-2.0).
use std::sync::LazyLock;

use uv_distribution_types::{
    SupportedVariantFeature, VariantFeature, VariantNamespace, VariantValue,
};
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_static::EnvVars;

use super::{PluginApi, VendoredVariantProvider};

static PLUGIN_API: LazyLock<PluginApi> = LazyLock::new(|| PluginApi {
    module: "tpu_variant_provider.plugin"
        .parse()
        .expect("valid Python module name"),
    object_path: vec!["TpuVariantPlugin".parse().expect("valid Python identifier")],
});
static PACKAGE_NAME: LazyLock<PackageName> =
    LazyLock::new(|| "tpu-variant-provider".parse().expect("valid package name"));
const NAMESPACE: &VariantNamespace = &VariantNamespace::from_static("tpu");

static PROPERTIES: LazyLock<[SupportedVariantFeature; 1]> = LazyLock::new(detect_properties);

/// The vendored implementation of `tpu-variant-provider` 0.1.0.
///
/// # Variant properties
///
/// - `tpu::present` is `true` if the host has a Google TPU and `false` otherwise.
///
/// Unless overridden, the provider checks PCI devices on Linux and reports `false` on
/// other platforms.
pub(super) struct TpuVariantProvider;

impl VendoredVariantProvider for TpuVariantProvider {
    fn namespace(&self) -> &VariantNamespace {
        NAMESPACE
    }

    fn plugin_api(&self) -> Option<&PluginApi> {
        Some(&PLUGIN_API)
    }

    fn package_name(&self) -> &PackageName {
        &PACKAGE_NAME
    }

    fn package_version(&self) -> Version {
        Version::new([0, 1, 0])
    }

    fn properties(&self) -> Option<&[SupportedVariantFeature]> {
        Some(PROPERTIES.as_slice())
    }
}

fn detect_properties() -> [SupportedVariantFeature; 1] {
    let force_present = std::env::var_os(EnvVars::TPU_VARIANT_PROVIDER_FORCE_PRESENT);
    let present = if let Some(value) = force_present {
        value == "1" || value.eq_ignore_ascii_case("true") || value.eq_ignore_ascii_case("yes")
    } else if cfg!(target_os = "linux") {
        has_tpu_pci_device()
    } else {
        false
    };
    let value = if present { "true" } else { "false" };
    [SupportedVariantFeature {
        name: VariantFeature::from_static("present"),
        values: vec![VariantValue::from_static(value)],
    }]
}

fn has_tpu_pci_device() -> bool {
    const GOOGLE_PCI_VENDOR_ID: &str = "0x1ae0";
    // TPU PCI device IDs and generations from JAX's `hardware_utils.py`.
    const TPU_DEVICE_IDS: &[&str] = &[
        "0x0027", // TPU v3
        "0x0056", // plc
        "0x005e", // TPU v4
        "0x0062", // TPU v5p
        "0x0063", // TPU v5e
        "0x006f", // TPU v6e
        "0x0076", // TPU7x
    ];

    let Ok(entries) = fs_err::read_dir("/sys/bus/pci/devices") else {
        return false;
    };
    // Skip devices whose sysfs attributes are unreadable.
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(vendor) = fs_err::read_to_string(path.join("vendor")) else {
            continue;
        };
        if vendor.trim() != GOOGLE_PCI_VENDOR_ID {
            continue;
        }
        let Ok(device) = fs_err::read_to_string(path.join("device")) else {
            continue;
        };
        if TPU_DEVICE_IDS.contains(&device.trim()) {
            return true;
        }
    }
    false
}
