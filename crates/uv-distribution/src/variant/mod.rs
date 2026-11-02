mod nvidia;

use tracing::debug;
use uv_distribution_types::{
    SupportedVariantFeature, SupportedVariantProperties, VariantNamespace, VariantProviderRequest,
    VariantsJsonContent,
};
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_pep508::{Requirement, VersionOrUrl};
use uv_pypi_types::{Identifier, ModuleName};

use crate::variant::nvidia::NvidiaVariantProvider;

static NVIDIA_PROVIDER: NvidiaVariantProvider = NvidiaVariantProvider::new();
static VENDORED_PROVIDERS: &[&dyn VendoredVariantProvider] = &[&NVIDIA_PROVIDER];

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

/// A vendored wheel variant provider.
trait VendoredVariantProvider: Sync {
    /// The API endpoint, or `None` to use the default derived from the package name.
    fn plugin_api(&self) -> Option<&PluginApi>;
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
    let detected = metadata
        .provider_requests()
        .filter_map(|(namespace, request)| {
            let Some(provider) = find_vendored_provider(namespace, request) else {
                // TODO(konsti): Support installing and running variant provider plugins.
                let provider_name = match request {
                    VariantProviderRequest::Plugin { plugin_api, .. } => plugin_api,
                    VariantProviderRequest::Builtin(name) => Some(name),
                };
                if let Some(provider_name) = provider_name {
                    debug!(
                        "Skipping unsupported wheel variant provider `{provider_name}` for \
                         namespace `{namespace}`"
                    );
                } else {
                    debug!(
                        "Skipping unsupported wheel variant provider for namespace `{namespace}`"
                    );
                }
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
