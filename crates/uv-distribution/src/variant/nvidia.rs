use std::cell::LazyCell;
use std::sync::LazyLock;

use tracing::debug;
use uv_distribution_types::{
    SupportedVariantFeature, VariantFeature, VariantMetadataError, VariantNamespace, VariantValue,
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
const NAMESPACE: &VariantNamespace = &VariantNamespace::from_static("nvidia");
const CUDA_VERSION_LOWER_BOUND: VariantFeature =
    VariantFeature::from_static("cuda_version_lower_bound");
const CUDA_VERSION_UPPER_BOUND: VariantFeature =
    VariantFeature::from_static("cuda_version_upper_bound");
const SM_ARCH: VariantFeature = VariantFeature::from_static("sm_arch");

const MIN_CUDA_MAJOR: u64 = 11;
const MIN_CUDA_MINOR: u64 = 0;
const MAX_CUDA_MAJOR: u64 = 15;
const MAX_CUDA_MINOR: u64 = 20;
const MIN_COMPUTE_CAPABILITY: (u8, u8) = (5, 0);

/// Bound memory use when an override exceeds the automatic detection range.
const MAX_CUDA_VERSION_VALUES: usize = 1_024;

// TODO(konsti): Move the NVIDIA variant provider into its own crate.
/// The vendored implementation of `nvidia-variant-provider` 0.0.1.
///
/// # Variant properties
///
/// - `nvidia::cuda_version_lower_bound` lists bounds at or below the effective CUDA driver version.
/// - `nvidia::cuda_version_upper_bound` lists bounds above the effective CUDA driver version.
/// - `nvidia::sm_arch` lists compatible real and virtual GPU architectures for compute
///   capabilities of 5.0 or higher.
pub(super) struct NvidiaVariantProvider {
    /// Cache capabilities so every package and installation stage uses the same result.
    properties: LazyLock<Option<Vec<SupportedVariantFeature>>>,
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
            properties: LazyLock::new(|| match Self::detect_properties() {
                Ok(properties) => properties,
                Err(err) => {
                    debug!("Skipping NVIDIA variant provider: {err}");
                    None
                }
            }),
        }
    }

    fn detect_properties() -> Result<Option<Vec<SupportedVariantFeature>>, NvidiaProviderError> {
        let gpu = LazyCell::new(|| match NvidiaGpu::detect() {
            Ok(gpu) => gpu,
            Err(err) => {
                debug!("Failed to detect NVIDIA GPU capabilities: {err}");
                None
            }
        });
        let cuda_version = if let Ok(value) =
            std::env::var(EnvVars::NV_VARIANT_PROVIDER_FORCE_CUDA_DRIVER_VERSION)
            && !value.is_empty()
        {
            value
                .parse::<Version>()
                .map_err(|source| NvidiaProviderError::InvalidCudaVersion { value, source })?
        } else {
            let Some(gpu) = gpu.as_ref() else {
                return Ok(None);
            };
            if gpu.cuda_version < Version::new([MIN_CUDA_MAJOR, MIN_CUDA_MINOR])
                || gpu.cuda_version > Version::new([MAX_CUDA_MAJOR, MAX_CUDA_MINOR])
            {
                debug!(
                    "Skipping NVIDIA variant provider: CUDA user-mode driver version {} is \
                     outside the supported range {MIN_CUDA_MAJOR}.{MIN_CUDA_MINOR} to \
                     {MAX_CUDA_MAJOR}.{MAX_CUDA_MINOR}",
                    gpu.cuda_version
                );
                return Ok(None);
            }
            gpu.cuda_version.clone()
        };
        let compute_capability = if let Ok(value) =
            std::env::var(EnvVars::NV_VARIANT_PROVIDER_FORCE_SM_ARCH)
            && !value.is_empty()
        {
            Some(
                NvidiaGpu::parse_compute_capability(&value)
                    .ok_or(NvidiaProviderError::InvalidComputeCapability { value })?,
            )
        } else {
            gpu.as_ref().and_then(|gpu| gpu.compute_capability)
        };
        Ok(Some(Self::properties_for_capabilities(
            &cuda_version,
            compute_capability,
        )?))
    }

    /// Implement the ordered properties of `nvidia-variant-provider` 0.0.1.
    ///
    /// See <https://github.com/wheelnext/nvidia-variant-provider/blob/v0.0.1/nvidia_variant_provider/plugin.py>.
    fn properties_for_capabilities(
        cuda_version: &Version,
        compute_capability: Option<(u8, u8)>,
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
                name: CUDA_VERSION_LOWER_BOUND,
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
                    name: CUDA_VERSION_UPPER_BOUND,
                    values: upper_bound,
                });
            }
        }
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
            properties.push(SupportedVariantFeature {
                name: SM_ARCH,
                values: architectures,
            });
        }
        Ok(properties)
    }
}

impl VendoredVariantProvider for NvidiaVariantProvider {
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
        Version::new([0, 0, 1])
    }

    fn properties(&self) -> Option<&[SupportedVariantFeature]> {
        self.properties.as_deref()
    }
}
