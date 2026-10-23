use std::sync::LazyLock;

use tracing::debug;
use uv_distribution_types::{
    SupportedVariantFeature, VariantFeature, VariantMetadataError, VariantNamespace, VariantValue,
    VariantsJsonContent,
};
use uv_normalize::PackageName;
use uv_pep440::{Version, VersionParseError};
use uv_static::EnvVars;
use uv_torch::NvidiaGpu;

use crate::variant::{PluginApi, VendoredVariantProvider};

static PLUGIN_API: LazyLock<PluginApi> = LazyLock::new(|| PluginApi {
    module: "nvidia_variant_provider.plugin"
        .parse()
        .expect("valid Python module name"),
    object_path: vec![
        "NvidiaVariantPlugin"
            .parse()
            .expect("valid Python identifier"),
    ],
});
static PACKAGE_NAME: LazyLock<PackageName> = LazyLock::new(|| {
    "nvidia-variant-provider"
        .parse()
        .expect("valid package name")
});
static NAMESPACE: LazyLock<VariantNamespace> = LazyLock::new(|| {
    VariantNamespace::try_from("nvidia".to_string()).expect("valid variant namespace")
});

const MIN_CUDA_MAJOR: u64 = 11;
const MIN_CUDA_MINOR: u64 = 0;
const MAX_CUDA_MAJOR: u64 = 15;
const MAX_CUDA_MINOR: u64 = 20;
const MIN_COMPUTE_CAPABILITY: (u8, u8) = (5, 0);

/// Bound memory use when an override exceeds the automatic detection range.
const MAX_CUDA_VERSION_VALUES: usize = 1_024;

// TODO(konsti): Move the NVIDIA variant provider into its own crate.
/// The vendored implementation of `nvidia-variant-provider` 0.0.1.
pub(super) struct NvidiaVariantProvider {
    /// Cache each capability separately, including failures, for the lifetime of the process.
    cuda: LazyLock<Option<Vec<SupportedVariantFeature>>>,
    sm: LazyLock<Option<SupportedVariantFeature>>,
}

#[derive(Debug, thiserror::Error)]
enum NvidiaProviderError {
    #[error(
        "Invalid value for `{variable}`: `{value}`",
        variable = EnvVars::NV_VARIANT_PROVIDER_FORCE_CUDA_DRIVER_VERSION
    )]
    InvalidCudaVersion {
        value: String,
        #[source]
        source: VersionParseError,
    },
    #[error(
        "Invalid value for `{variable}`: `{value}`",
        variable = EnvVars::NV_VARIANT_PROVIDER_FORCE_SM_ARCH
    )]
    InvalidComputeCapability { value: String },
    #[error(
        "CUDA version `{version}` exceeds the limit of {limit} variant values",
        limit = MAX_CUDA_VERSION_VALUES
    )]
    TooManyCudaVersionValues { version: Version },
    #[error(transparent)]
    InvalidVariantProperty(#[from] VariantMetadataError),
}

impl NvidiaVariantProvider {
    pub(super) const fn new() -> Self {
        Self {
            cuda: LazyLock::new(|| match Self::detect_cuda_properties() {
                Ok(properties) => properties,
                Err(err) => {
                    debug!("Skipping NVIDIA CUDA properties: {err}");
                    None
                }
            }),
            sm: LazyLock::new(|| match Self::detect_sm_property() {
                Ok(property) => property,
                Err(err) => {
                    debug!("Skipping NVIDIA SM property: {err}");
                    None
                }
            }),
        }
    }

    fn detect_cuda_properties() -> Result<Option<Vec<SupportedVariantFeature>>, NvidiaProviderError>
    {
        let cuda_version = if let Ok(value) =
            std::env::var(EnvVars::NV_VARIANT_PROVIDER_FORCE_CUDA_DRIVER_VERSION)
            && !value.is_empty()
        {
            value
                .parse::<Version>()
                .map_err(|source| NvidiaProviderError::InvalidCudaVersion { value, source })?
        } else {
            let Some(cuda_version) = NvidiaGpu::detect_cuda_version() else {
                return Ok(None);
            };
            if cuda_version < Version::new([MIN_CUDA_MAJOR, MIN_CUDA_MINOR])
                || cuda_version > Version::new([MAX_CUDA_MAJOR, MAX_CUDA_MINOR])
            {
                debug!(
                    "Skipping NVIDIA variant provider: CUDA user-mode driver version {} is \
                     outside the supported range {MIN_CUDA_MAJOR}.{MIN_CUDA_MINOR} to \
                     {MAX_CUDA_MAJOR}.{MAX_CUDA_MINOR}",
                    cuda_version
                );
                return Ok(None);
            }
            cuda_version
        };
        Ok(Some(Self::properties_for_cuda(&cuda_version)?))
    }

    fn detect_sm_property() -> Result<Option<SupportedVariantFeature>, NvidiaProviderError> {
        let compute_capability = if let Ok(value) =
            std::env::var(EnvVars::NV_VARIANT_PROVIDER_FORCE_SM_ARCH)
            && !value.is_empty()
        {
            Some(
                NvidiaGpu::parse_compute_capability(&value)
                    .ok_or(NvidiaProviderError::InvalidComputeCapability { value })?,
            )
        } else {
            NvidiaGpu::detect_compute_capability()
        };
        Self::property_for_sm(compute_capability)
    }

    /// Implement the ordered properties of `nvidia-variant-provider` 0.0.1.
    ///
    /// See <https://github.com/wheelnext/nvidia-variant-provider/blob/v0.0.1/nvidia_variant_provider/plugin.py>.
    fn properties_for_cuda(
        cuda_version: &Version,
    ) -> Result<Vec<SupportedVariantFeature>, NvidiaProviderError> {
        let release = cuda_version.release();
        let detected_major = release[0];
        // A major-only CUDA version denotes the start of a release series: `12` means `12.0`.
        let detected_minor = release.get(1).copied().unwrap_or(0);
        let versions_for_major = |major: u64, max_minor: u64| {
            (0..=max_minor)
                .rev()
                .map(move |minor| format!("{major}.{minor}"))
                .chain([major.to_string()])
        };
        let mut properties = Vec::new();
        // Add `cuda_version_lower_bound` with values at or below the effective CUDA version, which
        // may be forced outside the automatic detection range.
        if detected_major >= MIN_CUDA_MAJOR {
            let lower_bound = versions_for_major(detected_major, detected_minor)
                .chain(
                    (MIN_CUDA_MAJOR..detected_major)
                        .rev()
                        .flat_map(|major| versions_for_major(major, MAX_CUDA_MINOR)),
                )
                .take(MAX_CUDA_VERSION_VALUES + 1)
                .map(VariantValue::try_from)
                .collect::<Result<Vec<_>, _>>()?;
            if lower_bound.len() > MAX_CUDA_VERSION_VALUES {
                return Err(NvidiaProviderError::TooManyCudaVersionValues {
                    version: cuda_version.clone(),
                });
            }
            properties.push(SupportedVariantFeature {
                name: VariantFeature::try_from("cuda_version_lower_bound".to_string())?,
                values: lower_bound,
            });
        }
        // Add `cuda_version_upper_bound` with values above the effective CUDA version.
        if detected_major <= MAX_CUDA_MAJOR {
            let upper_bound = (detected_major + 1..=MAX_CUDA_MAJOR)
                .rev()
                .flat_map(|major| versions_for_major(major, MAX_CUDA_MINOR))
                .chain(
                    (0..=MAX_CUDA_MINOR)
                        .rev()
                        .take_while(|minor| *minor > detected_minor)
                        .map(|minor| format!("{detected_major}.{minor}")),
                )
                .map(VariantValue::try_from)
                .collect::<Result<Vec<_>, _>>()?;
            if !upper_bound.is_empty() {
                properties.push(SupportedVariantFeature {
                    name: VariantFeature::try_from("cuda_version_upper_bound".to_string())?,
                    values: upper_bound,
                });
            }
        }
        Ok(properties)
    }

    fn property_for_sm(
        compute_capability: Option<(u8, u8)>,
    ) -> Result<Option<SupportedVariantFeature>, NvidiaProviderError> {
        if let Some((major, minor)) = compute_capability
            && (major, minor) >= MIN_COMPUTE_CAPABILITY
        {
            let architectures = (0..=minor)
                .rev()
                .map(|minor| format!("{major}{minor}_real"))
                // TODO(konsti): Ask NVIDIA about `_virtual` values; use upstream values for now.
                .chain([format!("{major}0_virtual")])
                .map(VariantValue::try_from)
                .collect::<Result<_, _>>()?;
            return Ok(Some(SupportedVariantFeature {
                name: VariantFeature::try_from("sm_arch".to_string())?,
                values: architectures,
            }));
        }
        Ok(None)
    }
}

impl VendoredVariantProvider for NvidiaVariantProvider {
    fn namespace(&self) -> &VariantNamespace {
        &NAMESPACE
    }

    fn plugin_api(&self) -> Option<&PluginApi> {
        Some(&PLUGIN_API)
    }

    fn package_name(&self) -> &PackageName {
        &PACKAGE_NAME
    }

    fn package_version(&self) -> Version {
        Version::new([0, 0, 1])
    }

    fn properties(&self, metadata: &VariantsJsonContent) -> Vec<SupportedVariantFeature> {
        let mut properties = Vec::new();
        if (metadata.uses_feature(&NAMESPACE, "cuda_version_lower_bound")
            || metadata.uses_feature(&NAMESPACE, "cuda_version_upper_bound"))
            && let Some(cuda) = self.cuda.as_ref()
        {
            properties.extend(cuda.iter().cloned());
        }
        if metadata.uses_feature(&NAMESPACE, "sm_arch")
            && let Some(sm) = self.sm.as_ref()
        {
            properties.push(sm.clone());
        }
        properties
    }
}
