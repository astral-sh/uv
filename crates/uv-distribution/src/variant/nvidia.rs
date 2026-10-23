use std::sync::LazyLock;

use tracing::debug;
use uv_distribution_types::{
    SupportedVariantFeature, VariantFeature, VariantMetadataError, VariantNamespace, VariantValue,
};
use uv_normalize::PackageName;
use uv_pep440::{Version, VersionParseError};
use uv_static::EnvVars;
use uv_torch::{AcceleratorError, NvidiaGpu};

use crate::variant::{PluginApi, ProviderDetection, VendoredVariantProvider};

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
///
/// # Variant properties
///
/// - `nvidia::cuda_version_lower_bound` lists bounds at or below the effective CUDA driver version.
/// - `nvidia::cuda_version_upper_bound` lists bounds above the effective CUDA driver version.
/// - `nvidia::sm_arch` lists compatible real and virtual GPU architectures for compute
///   capabilities of 5.0 or higher.
pub(super) struct NvidiaVariantProvider {
    /// Cache capabilities so every package and installation stage uses the same result.
    detection: LazyLock<ProviderDetection>,
}

#[derive(Debug, thiserror::Error)]
enum NvidiaProviderError {
    #[error("Failed to detect NVIDIA GPU capabilities: {0}")]
    Detection(#[from] AcceleratorError),
    #[error(
        "CUDA user-mode driver version {version} is outside the supported range \
         {MIN_CUDA_MAJOR}.{MIN_CUDA_MINOR} to {MAX_CUDA_MAJOR}.{MAX_CUDA_MINOR}"
    )]
    UnsupportedCudaVersion { version: Version },
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
            detection: LazyLock::new(|| {
                let detection = Self::detect_properties().unwrap_or_else(ProviderDetection::failed);
                if let Some(error) = &detection.error {
                    debug!("NVIDIA variant provider detection failed: {error}");
                }
                detection
            }),
        }
    }

    fn detect_properties() -> Result<ProviderDetection, NvidiaProviderError> {
        let forced_cuda_version =
            std::env::var(EnvVars::NV_VARIANT_PROVIDER_FORCE_CUDA_DRIVER_VERSION)
                .ok()
                .filter(|value| !value.is_empty())
                .map(|value| {
                    value
                        .parse::<Version>()
                        .map_err(|source| NvidiaProviderError::InvalidCudaVersion { value, source })
                })
                .transpose()?;
        let forced_compute_capability = std::env::var(EnvVars::NV_VARIANT_PROVIDER_FORCE_SM_ARCH)
            .ok()
            .filter(|value| !value.is_empty());
        let gpu = if forced_cuda_version.is_none() || forced_compute_capability.is_none() {
            NvidiaGpu::detect()
        } else {
            Ok(None)
        };
        let (gpu, mut detection) = match gpu {
            Ok(gpu) => (gpu, ProviderDetection::default()),
            Err(error) => (
                None,
                ProviderDetection::failed(NvidiaProviderError::Detection(error)),
            ),
        };
        let cuda_version = if let Some(version) = forced_cuda_version {
            version
        } else {
            let Some(gpu) = gpu.as_ref() else {
                return Ok(detection);
            };
            if gpu.cuda_version < Version::new([MIN_CUDA_MAJOR, MIN_CUDA_MINOR])
                || gpu.cuda_version > Version::new([MAX_CUDA_MAJOR, MAX_CUDA_MINOR])
            {
                return Err(NvidiaProviderError::UnsupportedCudaVersion {
                    version: gpu.cuda_version.clone(),
                });
            }
            gpu.cuda_version.clone()
        };
        let compute_capability = if let Some(value) = forced_compute_capability {
            Some(
                NvidiaGpu::parse_compute_capability(&value)
                    .ok_or(NvidiaProviderError::InvalidComputeCapability { value })?,
            )
        } else {
            gpu.as_ref().and_then(|gpu| gpu.compute_capability)
        };
        detection.properties =
            Self::properties_for_capabilities(&cuda_version, compute_capability)?;
        Ok(detection)
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
                name: VariantFeature::try_from("sm_arch".to_string())?,
                values: architectures,
            });
        }
        Ok(properties)
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

    fn detect(&self) -> &ProviderDetection {
        &self.detection
    }
}
