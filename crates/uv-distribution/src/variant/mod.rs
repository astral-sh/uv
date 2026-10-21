#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
mod aarch64;
mod nvidia;
#[cfg(target_arch = "x86_64")]
mod x86_64;

use tracing::debug;
use uv_distribution_filename::VariantLabel;
use uv_distribution_types::{
    InstalledDist, SupportedVariantFeature, SupportedVariantProperties, VariantNamespace,
    VariantProvider, VariantProviderKind, VariantsJsonContent,
};
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_pep508::{Requirement, VersionOrUrl};
use uv_preview::PreviewFeature;
use uv_pypi_types::{Identifier, ModuleName};

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
use crate::variant::aarch64::Aarch64VariantProvider;
use crate::variant::nvidia::NvidiaVariantProvider;
#[cfg(target_arch = "x86_64")]
use crate::variant::x86_64::X86_64VariantProvider;

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
static AARCH64_PROVIDER: Aarch64VariantProvider = Aarch64VariantProvider;
static NVIDIA_PROVIDER: NvidiaVariantProvider = NvidiaVariantProvider::new();
#[cfg(target_arch = "x86_64")]
static X86_64_PROVIDER: X86_64VariantProvider = X86_64VariantProvider;
static VENDORED_PROVIDERS: &[&dyn VendoredVariantProvider] = &[
    #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
    &AARCH64_PROVIDER,
    &NVIDIA_PROVIDER,
    #[cfg(target_arch = "x86_64")]
    &X86_64_PROVIDER,
];

/// A Python module and optional path to an object within it, identifying a provider plugin.
struct PluginApi {
    module: ModuleName,
    object_path: Vec<Identifier>,
}

impl PluginApi {
    fn matches(&self, value: &str) -> bool {
        match value.split_once(':') {
            Some((module, object_path)) => {
                self.module.as_ref() == module
                    && self
                        .object_path
                        .iter()
                        .map(AsRef::as_ref)
                        .eq(object_path.split('.'))
            }
            None => self.module.as_ref() == value && self.object_path.is_empty(),
        }
    }
}

/// Whether an installed wheel's variant is compatible with the current system.
#[derive(Debug)]
pub enum InstalledWheelVariant {
    NonVariant,
    Compatible(VariantLabel),
    Incompatible,
}

impl InstalledWheelVariant {
    pub fn check(installed: &InstalledDist) -> Self {
        Self::check_with_target(installed, None)
    }

    /// Reselect an installed variant when resolving for an explicit target.
    pub fn check_with_target(
        installed: &InstalledDist,
        target: Option<&SupportedVariantProperties>,
    ) -> Self {
        let metadata = match installed.read_variant() {
            Ok(None) => return Self::NonVariant,
            Ok(Some(metadata)) => metadata,
            Err(err) => {
                debug!("Failed to read variant metadata for {installed}: {err}");
                return Self::Incompatible;
            }
        };
        if target.is_none()
            && uv_preview::is_enabled(PreviewFeature::WheelVariants)
            && let Some(label) =
                metadata.compatible_wheel_label(&supported_variant_properties(&metadata))
        {
            return Self::Compatible(label.clone());
        }
        Self::Incompatible
    }

    /// Whether the installed distribution is compatible with the current system.
    pub fn is_compatible(&self) -> bool {
        match self {
            // TODO(konsti): Consider higher-ranked variants even when this one is compatible.
            Self::NonVariant | Self::Compatible(_) => true,
            Self::Incompatible => false,
        }
    }

    /// Whether this wheel matches the variant selected for installation.
    pub fn matches(&self, expected: Option<&VariantLabel>) -> bool {
        match self {
            Self::NonVariant => expected.is_none(),
            Self::Compatible(label) => expected == Some(label),
            Self::Incompatible => false,
        }
    }
}

/// A vendored wheel variant provider.
trait VendoredVariantProvider: Sync {
    /// The API endpoint, or `None` to use the default derived from the package name.
    fn plugin_api(&self) -> Option<&PluginApi> {
        None
    }
    /// The name of the vendored provider package.
    fn package_name(&self) -> &PackageName;
    /// The version of the provider package that was vendored.
    fn package_version(&self) -> Version;
    /// The namespace of the emitted properties.
    fn namespace(&self) -> &VariantNamespace;
    /// Query the host for the compatible properties and their order.
    fn properties(&self) -> Option<&[SupportedVariantFeature]>;
}

/// Collect the variant properties supported by enabled static and vendored providers.
pub fn supported_variant_properties(metadata: &VariantsJsonContent) -> SupportedVariantProperties {
    trusted_variant_properties(
        metadata
            .providers()
            .filter(|(_, provider)| !provider.optional()),
    )
}

fn trusted_variant_properties<'a>(
    providers: impl Iterator<Item = (&'a VariantNamespace, &'a VariantProvider)>,
) -> SupportedVariantProperties {
    let mut supported = SupportedVariantProperties::default();
    for (namespace, provider) in providers {
        let features = provider.static_properties().or_else(|| {
            find_vendored_provider(namespace, provider.kind())
                .and_then(|provider| provider.properties())
                .map(<[_]>::to_vec)
        });
        if let Some(features) = features {
            supported.insert(namespace.clone(), features);
        }
    }
    supported
}

/// Find a vendored provider that satisfies a plugin request without installing other packages.
fn find_vendored_provider(
    namespace: &VariantNamespace,
    kind: &VariantProviderKind,
) -> Option<&'static dyn VendoredVariantProvider> {
    let (requires, plugin_api) = match kind {
        VariantProviderKind::Plugin {
            requires,
            plugin_api,
        } => (requires.as_slice(), plugin_api.as_deref()),
        VariantProviderKind::Static { .. } | VariantProviderKind::Builtin { .. } => return None,
    };
    let [requirement] = requires else {
        return None;
    };
    let requirement = requirement.parse::<Requirement>().ok()?;
    // TODO(konsti): Support markers?
    if !requirement.extras.is_empty() || !requirement.marker.is_true() {
        return None;
    }
    let specifiers = match requirement.version_or_url {
        Some(VersionOrUrl::VersionSpecifier(specifiers)) => Some(specifiers),
        Some(VersionOrUrl::Url(_)) => return None,
        None => None,
    };
    let default_plugin_api = requirement.name.as_dist_info_name();
    let plugin_api = plugin_api.unwrap_or(default_plugin_api.as_ref());
    VENDORED_PROVIDERS.iter().copied().find(|provider| {
        if provider.namespace() != namespace || provider.package_name() != &requirement.name {
            return false;
        }
        let matches_plugin_api = if let Some(provider_plugin_api) = provider.plugin_api() {
            provider_plugin_api.matches(plugin_api)
        } else {
            default_plugin_api.as_ref() == plugin_api
        };
        matches_plugin_api
            && specifiers
                .as_ref()
                .is_none_or(|specifiers| specifiers.contains(&provider.package_version()))
    })
}
