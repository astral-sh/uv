mod nvidia;

use tracing::debug;
use uv_distribution_types::{
    SupportedVariantFeature, SupportedVariantProperties, VariantNamespace, VariantProviderRequest,
    VariantsJsonContent,
};
use uv_pep440::Version;
use uv_pep508::{Requirement, VersionOrUrl};

use crate::variant::nvidia::NvidiaVariantProvider;

static NVIDIA_PROVIDER: NvidiaVariantProvider = NvidiaVariantProvider::new();
static VENDORED_PROVIDERS: &[&dyn VendoredVariantProvider] = &[&NVIDIA_PROVIDER];

/// A vendored wheel variant provider.
trait VendoredVariantProvider: Sync {
    fn namespace(&self) -> &str;
    /// The API endpoint, or `None` to use the default derived from the package name.
    fn plugin_api(&self) -> Option<&str>;
    /// The normalized name of the provider package.
    fn package_name(&self) -> &str;
    fn package_version(&self) -> Version;
    fn properties(&self) -> Option<&[SupportedVariantFeature]>;
}

/// Collect the variant properties supported by enabled static and vendored providers.
pub fn supported_variant_properties(metadata: &VariantsJsonContent) -> SupportedVariantProperties {
    let detected = metadata
        .provider_requests()
        .filter_map(|(namespace, request)| {
            let Some(provider) = find_vendored_provider(namespace, request) else {
                // TODO(konsti): Support installing and running variant provider plugins.
                let provider_name = match request {
                    VariantProviderRequest::Plugin { plugin_api, .. } => plugin_api,
                    VariantProviderRequest::Builtin(name) => Some(name),
                }
                .unwrap_or("<default>");
                debug!(
                    "Skipping unsupported wheel variant provider `{provider_name}` for namespace \
                     `{namespace}`"
                );
                return None;
            };
            Some((namespace, provider.properties()?))
        });
    metadata.supported_properties(detected)
}

/// Find a vendored provider that satisfies a plugin request without installing other packages.
fn find_vendored_provider(
    namespace: &VariantNamespace,
    request: VariantProviderRequest<'_>,
) -> Option<&'static dyn VendoredVariantProvider> {
    let (requires, plugin_api) = match request {
        VariantProviderRequest::Plugin {
            requires,
            plugin_api,
        } => (requires, plugin_api),
        VariantProviderRequest::Builtin(_) => return None,
    };
    let [requirement] = requires else {
        return None;
    };
    // Reject unsupported markers before the requirement parser recurses into them.
    if requirement.contains(';') {
        return None;
    }
    let requirement = requirement.parse::<Requirement>().ok()?;
    if !requirement.extras.is_empty() {
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
        provider.namespace() == namespace.as_str()
            && provider.package_name() == requirement.name.as_str()
            && provider.plugin_api().unwrap_or(default_plugin_api.as_ref()) == plugin_api
            && specifiers
                .as_ref()
                .is_none_or(|specifiers| specifiers.contains(&provider.package_version()))
    })
}
