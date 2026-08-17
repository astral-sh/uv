//! `uv-torch` is a library for determining the appropriate PyTorch index based on the operating
//! system and CUDA driver version.
//!
//! This library is derived from `light-the-torch` by Philipp Meier, which is available under the
//! following BSD-3 Clause license:
//!
//! ```text
//! BSD 3-Clause License
//!
//! Copyright (c) 2020, Philip Meier
//! All rights reserved.
//!
//! Redistribution and use in source and binary forms, with or without
//! modification, are permitted provided that the following conditions are met:
//!
//! 1. Redistributions of source code must retain the above copyright notice, this
//!    list of conditions and the following disclaimer.
//!
//! 2. Redistributions in binary form must reproduce the above copyright notice,
//!    this list of conditions and the following disclaimer in the documentation
//!    and/or other materials provided with the distribution.
//!
//! 3. Neither the name of the copyright holder nor the names of its
//!    contributors may be used to endorse or promote products derived from
//!    this software without specific prior written permission.
//!
//! THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
//! AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
//! IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//! DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
//! FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
//! DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//! SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
//! CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
//! OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
//! OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
//! ```

use std::borrow::Cow;
use std::str::FromStr;
use std::sync::LazyLock;

use url::Url;

use uv_distribution_types::{IndexUrl, IndexUrlError};
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_platform_tags::Os;
use uv_static::EnvVars;

use crate::accelerator::{Accelerator, AcceleratorError, AmdGpuArchitecture};

/// The strategy to use when determining the appropriate PyTorch index.
#[derive(Debug, Copy, Clone, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "kebab-case")]
pub enum TorchMode {
    /// Select the appropriate PyTorch index based on the operating system and CUDA driver version.
    Auto,
    /// Use the CPU-only PyTorch index.
    Cpu,
    /// Use the PyTorch index for CUDA 13.2.
    Cu132,
    /// Use the PyTorch index for CUDA 13.0.
    Cu130,
    /// Use the PyTorch index for CUDA 12.9.
    Cu129,
    /// Use the PyTorch index for CUDA 12.8.
    Cu128,
    /// Use the PyTorch index for CUDA 12.6.
    Cu126,
    /// Use the PyTorch index for CUDA 12.5.
    Cu125,
    /// Use the PyTorch index for CUDA 12.4.
    Cu124,
    /// Use the PyTorch index for CUDA 12.3.
    Cu123,
    /// Use the PyTorch index for CUDA 12.2.
    Cu122,
    /// Use the PyTorch index for CUDA 12.1.
    Cu121,
    /// Use the PyTorch index for CUDA 12.0.
    Cu120,
    /// Use the PyTorch index for CUDA 11.8.
    Cu118,
    /// Use the PyTorch index for CUDA 11.7.
    Cu117,
    /// Use the PyTorch index for CUDA 11.6.
    Cu116,
    /// Use the PyTorch index for CUDA 11.5.
    Cu115,
    /// Use the PyTorch index for CUDA 11.4.
    Cu114,
    /// Use the PyTorch index for CUDA 11.3.
    Cu113,
    /// Use the PyTorch index for CUDA 11.2.
    Cu112,
    /// Use the PyTorch index for CUDA 11.1.
    Cu111,
    /// Use the PyTorch index for CUDA 11.0.
    Cu110,
    /// Use the PyTorch index for CUDA 10.2.
    Cu102,
    /// Use the PyTorch index for CUDA 10.1.
    Cu101,
    /// Use the PyTorch index for CUDA 10.0.
    Cu100,
    /// Use the PyTorch index for CUDA 9.2.
    Cu92,
    /// Use the PyTorch index for CUDA 9.1.
    Cu91,
    /// Use the PyTorch index for CUDA 9.0.
    Cu90,
    /// Use the PyTorch index for CUDA 8.0.
    Cu80,
    /// Use the PyTorch index for ROCm 7.2.
    #[serde(rename = "rocm7.2")]
    #[cfg_attr(feature = "clap", clap(name = "rocm7.2"))]
    Rocm72,
    /// Use the PyTorch index for ROCm 7.1.
    #[serde(rename = "rocm7.1")]
    #[cfg_attr(feature = "clap", clap(name = "rocm7.1"))]
    Rocm71,
    /// Use the PyTorch index for ROCm 7.0.
    #[serde(rename = "rocm7.0")]
    #[cfg_attr(feature = "clap", clap(name = "rocm7.0"))]
    Rocm70,
    /// Use the PyTorch index for ROCm 6.4.
    #[serde(rename = "rocm6.4")]
    #[cfg_attr(feature = "clap", clap(name = "rocm6.4"))]
    Rocm64,
    /// Use the PyTorch index for ROCm 6.3.
    #[serde(rename = "rocm6.3")]
    #[cfg_attr(feature = "clap", clap(name = "rocm6.3"))]
    Rocm63,
    /// Use the PyTorch index for ROCm 6.2.4.
    #[serde(rename = "rocm6.2.4")]
    #[cfg_attr(feature = "clap", clap(name = "rocm6.2.4"))]
    Rocm624,
    /// Use the PyTorch index for ROCm 6.2.
    #[serde(rename = "rocm6.2")]
    #[cfg_attr(feature = "clap", clap(name = "rocm6.2"))]
    Rocm62,
    /// Use the PyTorch index for ROCm 6.1.
    #[serde(rename = "rocm6.1")]
    #[cfg_attr(feature = "clap", clap(name = "rocm6.1"))]
    Rocm61,
    /// Use the PyTorch index for ROCm 6.0.
    #[serde(rename = "rocm6.0")]
    #[cfg_attr(feature = "clap", clap(name = "rocm6.0"))]
    Rocm60,
    /// Use the PyTorch index for ROCm 5.7.
    #[serde(rename = "rocm5.7")]
    #[cfg_attr(feature = "clap", clap(name = "rocm5.7"))]
    Rocm57,
    /// Use the PyTorch index for ROCm 5.6.
    #[serde(rename = "rocm5.6")]
    #[cfg_attr(feature = "clap", clap(name = "rocm5.6"))]
    Rocm56,
    /// Use the PyTorch index for ROCm 5.5.
    #[serde(rename = "rocm5.5")]
    #[cfg_attr(feature = "clap", clap(name = "rocm5.5"))]
    Rocm55,
    /// Use the PyTorch index for ROCm 5.4.2.
    #[serde(rename = "rocm5.4.2")]
    #[cfg_attr(feature = "clap", clap(name = "rocm5.4.2"))]
    Rocm542,
    /// Use the PyTorch index for ROCm 5.4.
    #[serde(rename = "rocm5.4")]
    #[cfg_attr(feature = "clap", clap(name = "rocm5.4"))]
    Rocm54,
    /// Use the PyTorch index for ROCm 5.3.
    #[serde(rename = "rocm5.3")]
    #[cfg_attr(feature = "clap", clap(name = "rocm5.3"))]
    Rocm53,
    /// Use the PyTorch index for ROCm 5.2.
    #[serde(rename = "rocm5.2")]
    #[cfg_attr(feature = "clap", clap(name = "rocm5.2"))]
    Rocm52,
    /// Use the PyTorch index for ROCm 5.1.1.
    #[serde(rename = "rocm5.1.1")]
    #[cfg_attr(feature = "clap", clap(name = "rocm5.1.1"))]
    Rocm511,
    /// Use the PyTorch index for ROCm 4.2.
    #[serde(rename = "rocm4.2")]
    #[cfg_attr(feature = "clap", clap(name = "rocm4.2"))]
    Rocm42,
    /// Use the PyTorch index for ROCm 4.1.
    #[serde(rename = "rocm4.1")]
    #[cfg_attr(feature = "clap", clap(name = "rocm4.1"))]
    Rocm41,
    /// Use the PyTorch index for ROCm 4.0.1.
    #[serde(rename = "rocm4.0.1")]
    #[cfg_attr(feature = "clap", clap(name = "rocm4.0.1"))]
    Rocm401,
    /// Use the PyTorch index for Intel XPU.
    Xpu,
}

#[derive(Debug, Default, Copy, Clone, Eq, PartialEq)]
pub enum TorchSource {
    /// Download PyTorch builds from the official PyTorch index.
    #[default]
    PyTorch,
    /// Download PyTorch builds from the pyx index.
    Pyx,
}

/// The strategy to use when determining the appropriate PyTorch index.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum TorchStrategy {
    /// Select the appropriate PyTorch index based on the operating system and CUDA driver version (e.g., `550.144.03`).
    Cuda {
        os: Os,
        driver_version: Version,
        source: TorchSource,
        indexes: Box<[IndexUrl]>,
    },
    /// Select the appropriate PyTorch index based on the operating system and AMD GPU architecture (e.g., `gfx1100`).
    Amd {
        os: Os,
        gpu_architecture: AmdGpuArchitecture,
        source: TorchSource,
        indexes: Box<[IndexUrl]>,
    },
    /// Select the appropriate PyTorch index based on the operating system and Intel GPU presence.
    Xpu {
        os: Os,
        source: TorchSource,
        indexes: Box<[IndexUrl]>,
    },
    /// Use the specified PyTorch index.
    Backend {
        backend: TorchBackend,
        source: TorchSource,
        indexes: Box<[IndexUrl]>,
    },
}

/// An error that occurs when determining a [`TorchStrategy`].
#[derive(Debug, thiserror::Error)]
pub enum TorchStrategyError {
    #[error(transparent)]
    Accelerator(#[from] AcceleratorError),
    #[error("Invalid value for `UV_TORCH_BACKEND_INDEX`")]
    IndexUrl(#[source] IndexUrlError),
}

impl TorchStrategy {
    /// Determine the [`TorchStrategy`] from the given [`TorchMode`], [`Os`], and [`Accelerator`].
    ///
    /// The `cuda_driver_version` and `amd_gpu_architecture` overrides, if provided, take
    /// precedence over system detection and correspond to the `UV_CUDA_DRIVER_VERSION` and
    /// `UV_AMD_GPU_ARCHITECTURE` environment variables respectively. When
    /// `UV_TORCH_BACKEND_INDEX` is set, uv appends the selected backend to its base URL.
    pub fn from_mode(
        mode: TorchMode,
        source: TorchSource,
        os: &Os,
        cuda_driver_version: Option<Version>,
        amd_gpu_architecture: Option<AmdGpuArchitecture>,
        configured_index_base: Option<&str>,
    ) -> Result<Self, TorchStrategyError> {
        let index_base = configured_index_base.map(Cow::Borrowed).or_else(|| {
            std::env::var(EnvVars::UV_TORCH_BACKEND_INDEX)
                .ok()
                .map(Cow::Owned)
        });
        let index_base = index_base.as_deref();

        match mode {
            TorchMode::Auto => {
                match Accelerator::detect(cuda_driver_version, amd_gpu_architecture)? {
                    Some(Accelerator::Cuda { driver_version }) => {
                        let indexes = Self::cuda_indexes(os, &driver_version, source, index_base)?;
                        Ok(Self::Cuda {
                            os: os.clone(),
                            driver_version,
                            source,
                            indexes,
                        })
                    }
                    Some(Accelerator::Amd { gpu_architecture }) => {
                        let indexes = Self::amd_indexes(os, gpu_architecture, source, index_base)?;
                        Ok(Self::Amd {
                            os: os.clone(),
                            gpu_architecture,
                            source,
                            indexes,
                        })
                    }
                    Some(Accelerator::Xpu) => {
                        let indexes = Self::xpu_indexes(os, source, index_base)?;
                        Ok(Self::Xpu {
                            os: os.clone(),
                            source,
                            indexes,
                        })
                    }
                    None => Self::backend(TorchBackend::Cpu, source, index_base),
                }
            }
            TorchMode::Cpu => Self::backend(TorchBackend::Cpu, source, index_base),
            TorchMode::Cu132 => Self::backend(TorchBackend::Cu132, source, index_base),
            TorchMode::Cu130 => Self::backend(TorchBackend::Cu130, source, index_base),
            TorchMode::Cu129 => Self::backend(TorchBackend::Cu129, source, index_base),
            TorchMode::Cu128 => Self::backend(TorchBackend::Cu128, source, index_base),
            TorchMode::Cu126 => Self::backend(TorchBackend::Cu126, source, index_base),
            TorchMode::Cu125 => Self::backend(TorchBackend::Cu125, source, index_base),
            TorchMode::Cu124 => Self::backend(TorchBackend::Cu124, source, index_base),
            TorchMode::Cu123 => Self::backend(TorchBackend::Cu123, source, index_base),
            TorchMode::Cu122 => Self::backend(TorchBackend::Cu122, source, index_base),
            TorchMode::Cu121 => Self::backend(TorchBackend::Cu121, source, index_base),
            TorchMode::Cu120 => Self::backend(TorchBackend::Cu120, source, index_base),
            TorchMode::Cu118 => Self::backend(TorchBackend::Cu118, source, index_base),
            TorchMode::Cu117 => Self::backend(TorchBackend::Cu117, source, index_base),
            TorchMode::Cu116 => Self::backend(TorchBackend::Cu116, source, index_base),
            TorchMode::Cu115 => Self::backend(TorchBackend::Cu115, source, index_base),
            TorchMode::Cu114 => Self::backend(TorchBackend::Cu114, source, index_base),
            TorchMode::Cu113 => Self::backend(TorchBackend::Cu113, source, index_base),
            TorchMode::Cu112 => Self::backend(TorchBackend::Cu112, source, index_base),
            TorchMode::Cu111 => Self::backend(TorchBackend::Cu111, source, index_base),
            TorchMode::Cu110 => Self::backend(TorchBackend::Cu110, source, index_base),
            TorchMode::Cu102 => Self::backend(TorchBackend::Cu102, source, index_base),
            TorchMode::Cu101 => Self::backend(TorchBackend::Cu101, source, index_base),
            TorchMode::Cu100 => Self::backend(TorchBackend::Cu100, source, index_base),
            TorchMode::Cu92 => Self::backend(TorchBackend::Cu92, source, index_base),
            TorchMode::Cu91 => Self::backend(TorchBackend::Cu91, source, index_base),
            TorchMode::Cu90 => Self::backend(TorchBackend::Cu90, source, index_base),
            TorchMode::Cu80 => Self::backend(TorchBackend::Cu80, source, index_base),
            TorchMode::Rocm72 => Self::backend(TorchBackend::Rocm72, source, index_base),
            TorchMode::Rocm71 => Self::backend(TorchBackend::Rocm71, source, index_base),
            TorchMode::Rocm70 => Self::backend(TorchBackend::Rocm70, source, index_base),
            TorchMode::Rocm64 => Self::backend(TorchBackend::Rocm64, source, index_base),
            TorchMode::Rocm63 => Self::backend(TorchBackend::Rocm63, source, index_base),
            TorchMode::Rocm624 => Self::backend(TorchBackend::Rocm624, source, index_base),
            TorchMode::Rocm62 => Self::backend(TorchBackend::Rocm62, source, index_base),
            TorchMode::Rocm61 => Self::backend(TorchBackend::Rocm61, source, index_base),
            TorchMode::Rocm60 => Self::backend(TorchBackend::Rocm60, source, index_base),
            TorchMode::Rocm57 => Self::backend(TorchBackend::Rocm57, source, index_base),
            TorchMode::Rocm56 => Self::backend(TorchBackend::Rocm56, source, index_base),
            TorchMode::Rocm55 => Self::backend(TorchBackend::Rocm55, source, index_base),
            TorchMode::Rocm542 => Self::backend(TorchBackend::Rocm542, source, index_base),
            TorchMode::Rocm54 => Self::backend(TorchBackend::Rocm54, source, index_base),
            TorchMode::Rocm53 => Self::backend(TorchBackend::Rocm53, source, index_base),
            TorchMode::Rocm52 => Self::backend(TorchBackend::Rocm52, source, index_base),
            TorchMode::Rocm511 => Self::backend(TorchBackend::Rocm511, source, index_base),
            TorchMode::Rocm42 => Self::backend(TorchBackend::Rocm42, source, index_base),
            TorchMode::Rocm41 => Self::backend(TorchBackend::Rocm41, source, index_base),
            TorchMode::Rocm401 => Self::backend(TorchBackend::Rocm401, source, index_base),
            TorchMode::Xpu => Self::backend(TorchBackend::Xpu, source, index_base),
        }
    }

    fn backend(
        backend: TorchBackend,
        source: TorchSource,
        configured_index_base: Option<&str>,
    ) -> Result<Self, TorchStrategyError> {
        Ok(Self::Backend {
            backend,
            source,
            indexes: Self::indexes(std::iter::once(backend), source, configured_index_base)?,
        })
    }

    fn cuda_indexes(
        os: &Os,
        driver_version: &Version,
        source: TorchSource,
        configured_index_base: Option<&str>,
    ) -> Result<Box<[IndexUrl]>, TorchStrategyError> {
        match os {
            Os::Manylinux { .. } | Os::Musllinux { .. } => Self::indexes(
                LINUX_CUDA_DRIVERS
                    .iter()
                    .filter(|(_, version)| driver_version >= version)
                    .map(|(backend, _)| *backend)
                    .chain(std::iter::once(TorchBackend::Cpu)),
                source,
                configured_index_base,
            ),
            Os::Windows => Self::indexes(
                WINDOWS_CUDA_VERSIONS
                    .iter()
                    .filter(|(_, version)| driver_version >= version)
                    .map(|(backend, _)| *backend)
                    .chain(std::iter::once(TorchBackend::Cpu)),
                source,
                configured_index_base,
            ),
            Os::Macos { .. }
            | Os::FreeBsd { .. }
            | Os::NetBsd { .. }
            | Os::OpenBsd { .. }
            | Os::Dragonfly { .. }
            | Os::Illumos { .. }
            | Os::Haiku { .. }
            | Os::Android { .. }
            | Os::Pyodide { .. }
            | Os::PyEmscripten { .. }
            | Os::Ios { .. } => Self::indexes(
                std::iter::once(TorchBackend::Cpu),
                source,
                configured_index_base,
            ),
        }
    }

    fn amd_indexes(
        os: &Os,
        gpu_architecture: AmdGpuArchitecture,
        source: TorchSource,
        configured_index_base: Option<&str>,
    ) -> Result<Box<[IndexUrl]>, TorchStrategyError> {
        match os {
            Os::Manylinux { .. } | Os::Musllinux { .. } => Self::indexes(
                LINUX_AMD_GPU_DRIVERS
                    .iter()
                    .filter(|(_, architecture)| gpu_architecture == *architecture)
                    .map(|(backend, _)| *backend)
                    .chain(std::iter::once(TorchBackend::Cpu)),
                source,
                configured_index_base,
            ),
            Os::Windows
            | Os::Macos { .. }
            | Os::FreeBsd { .. }
            | Os::NetBsd { .. }
            | Os::OpenBsd { .. }
            | Os::Dragonfly { .. }
            | Os::Illumos { .. }
            | Os::Haiku { .. }
            | Os::Android { .. }
            | Os::Pyodide { .. }
            | Os::PyEmscripten { .. }
            | Os::Ios { .. } => Self::indexes(
                std::iter::once(TorchBackend::Cpu),
                source,
                configured_index_base,
            ),
        }
    }

    fn xpu_indexes(
        os: &Os,
        source: TorchSource,
        configured_index_base: Option<&str>,
    ) -> Result<Box<[IndexUrl]>, TorchStrategyError> {
        match os {
            Os::Manylinux { .. } | Os::Windows => Self::indexes(
                std::iter::once(TorchBackend::Xpu),
                source,
                configured_index_base,
            ),
            Os::Musllinux { .. }
            | Os::Macos { .. }
            | Os::FreeBsd { .. }
            | Os::NetBsd { .. }
            | Os::OpenBsd { .. }
            | Os::Dragonfly { .. }
            | Os::Illumos { .. }
            | Os::Haiku { .. }
            | Os::Android { .. }
            | Os::Pyodide { .. }
            | Os::PyEmscripten { .. }
            | Os::Ios { .. } => Self::indexes(
                std::iter::once(TorchBackend::Cpu),
                source,
                configured_index_base,
            ),
        }
    }

    fn indexes(
        backends: impl IntoIterator<Item = TorchBackend>,
        source: TorchSource,
        configured_index_base: Option<&str>,
    ) -> Result<Box<[IndexUrl]>, TorchStrategyError> {
        let index_base = Self::index_base(source, configured_index_base);
        backends
            .into_iter()
            .map(|backend| {
                backend
                    .index_url(&index_base)
                    .map_err(TorchStrategyError::IndexUrl)
            })
            .collect()
    }

    fn index_base(source: TorchSource, configured_index_base: Option<&str>) -> Cow<'_, str> {
        configured_index_base.map_or_else(
            || match source {
                TorchSource::PyTorch => Cow::Borrowed(PYTORCH_INDEX_BASE_URL),
                TorchSource::Pyx => Cow::Owned(format!("{}/simple/astral-sh", *PYX_API_BASE_URL)),
            },
            Cow::Borrowed,
        )
    }

    /// Returns `true` if the [`TorchStrategy`] applies to the given [`PackageName`].
    pub fn applies_to(&self, package_name: &PackageName) -> bool {
        let source = match self {
            Self::Cuda { source, .. } => *source,
            Self::Amd { source, .. } => *source,
            Self::Xpu { source, .. } => *source,
            Self::Backend { source, .. } => *source,
        };
        match source {
            TorchSource::PyTorch => {
                matches!(
                    package_name.as_str(),
                    "fbgemm-gpu"
                        | "fbgemm-gpu-genai"
                        | "pytorch-triton"
                        | "pytorch-triton-rocm"
                        | "pytorch-triton-xpu"
                        | "torch"
                        | "torch-tensorrt"
                        | "torchao"
                        | "torcharrow"
                        | "torchaudio"
                        | "torchcodec"
                        | "torchcsprng"
                        | "torchdistx"
                        | "torchrec"
                        | "torchserve"
                        | "torchtext"
                        | "torchtune"
                        | "torchvision"
                        | "triton"
                        | "triton-rocm"
                        | "triton-xpu"
                        | "xformers"
                )
            }
            TorchSource::Pyx => {
                matches!(
                    package_name.as_str(),
                    "deepspeed"
                        | "fbgemm-gpu"
                        | "fbgemm-gpu-genai"
                        | "flash-attn"
                        | "flash-attn-3"
                        | "megablocks"
                        | "natten"
                        | "pyg-lib"
                        | "pytorch-triton"
                        | "pytorch-triton-rocm"
                        | "pytorch-triton-xpu"
                        | "torch"
                        | "torch-cluster"
                        | "torch-scatter"
                        | "torch-sparse"
                        | "torch-spline-conv"
                        | "torch-tensorrt"
                        | "torchao"
                        | "torcharrow"
                        | "torchaudio"
                        | "torchcodec"
                        | "torchcsprng"
                        | "torchdistx"
                        | "torchrec"
                        | "torchserve"
                        | "torchtext"
                        | "torchtune"
                        | "torchvision"
                        | "triton"
                        | "triton-rocm"
                        | "triton-xpu"
                        | "vllm"
                        | "xformers"
                )
            }
        }
    }

    /// Returns `true` if the given [`PackageName`] has a system dependency (e.g., CUDA or ROCm).
    ///
    /// For example, `triton` is hosted on the PyTorch indexes, but does not have a system
    /// dependency on the associated CUDA version (i.e., the `triton` on the `cu128` index doesn't
    /// depend on CUDA 12.8).
    pub fn has_system_dependency(&self, package_name: &PackageName) -> bool {
        matches!(
            package_name.as_str(),
            "deepspeed"
                | "fbgemm-gpu"
                | "fbgemm-gpu-genai"
                | "flash-attn"
                | "flash-attn-3"
                | "megablocks"
                | "natten"
                | "torch"
                | "torch-tensorrt"
                | "torchao"
                | "torcharrow"
                | "torchaudio"
                | "torchcodec"
                | "torchcsprng"
                | "torchdistx"
                | "torchrec"
                | "torchtext"
                | "torchtune"
                | "torchvision"
                | "vllm"
        )
    }

    /// Return the appropriate index URLs for the given [`TorchStrategy`].
    pub fn index_urls(&self) -> impl Iterator<Item = &IndexUrl> {
        let indexes = match self {
            Self::Cuda { indexes, .. }
            | Self::Amd { indexes, .. }
            | Self::Xpu { indexes, .. }
            | Self::Backend { indexes, .. } => indexes,
        };
        indexes.iter()
    }
}

/// The available backends for PyTorch.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum TorchBackend {
    Cpu,
    Cu132,
    Cu130,
    Cu129,
    Cu128,
    Cu126,
    Cu125,
    Cu124,
    Cu123,
    Cu122,
    Cu121,
    Cu120,
    Cu118,
    Cu117,
    Cu116,
    Cu115,
    Cu114,
    Cu113,
    Cu112,
    Cu111,
    Cu110,
    Cu102,
    Cu101,
    Cu100,
    Cu92,
    Cu91,
    Cu90,
    Cu80,
    Rocm72,
    Rocm71,
    Rocm70,
    Rocm64,
    Rocm63,
    Rocm624,
    Rocm62,
    Rocm61,
    Rocm60,
    Rocm57,
    Rocm56,
    Rocm55,
    Rocm542,
    Rocm54,
    Rocm53,
    Rocm52,
    Rocm511,
    Rocm42,
    Rocm41,
    Rocm401,
    Xpu,
}

impl TorchBackend {
    /// Return the appropriate index URL for the given [`TorchBackend`] and index base URL.
    fn index_url(self, index_base: &str) -> Result<IndexUrl, IndexUrlError> {
        let index_base = index_base.trim_end_matches('/');
        IndexUrl::from_str(&format!("{index_base}/{}", self.index_name()))
    }

    fn index_name(self) -> String {
        self.rocm_version().map_or_else(
            || format!("{self:?}").to_ascii_lowercase(),
            |version| format!("rocm{version}"),
        )
    }

    /// Extract a [`TorchBackend`] from an index URL.
    pub fn from_index(index: &Url) -> Option<Self> {
        let backend_identifier = if index.host_str() == Some("download.pytorch.org") {
            // E.g., `https://download.pytorch.org/whl/cu124`
            let mut path_segments = index.path_segments()?;
            if path_segments.next() != Some("whl") {
                return None;
            }
            path_segments.next()?
        // TODO(zanieb): We should consolidate this with `is_known_url` somehow
        } else if index.host_str() == PYX_API_BASE_URL.strip_prefix("https://") {
            // E.g., `https://api.pyx.dev/simple/astral-sh/cu124`
            let mut path_segments = index.path_segments()?;
            if path_segments.next() != Some("simple") {
                return None;
            }
            if path_segments.next() != Some("astral-sh") {
                return None;
            }
            path_segments.next()?
        } else {
            return None;
        };
        Self::from_str(backend_identifier).ok()
    }

    /// Returns the CUDA [`Version`] for the given [`TorchBackend`].
    pub fn cuda_version(&self) -> Option<Version> {
        let backend = self.index_name();
        let cuda = backend.strip_prefix("cu")?;
        let (major, minor) = cuda.split_at(cuda.len().checked_sub(1)?);
        Some(Version::new([
            major.parse::<u64>().ok()?,
            minor.parse::<u64>().ok()?,
        ]))
    }

    /// Returns the ROCM [`Version`] for the given [`TorchBackend`].
    pub fn rocm_version(&self) -> Option<Version> {
        match self {
            Self::Rocm72 => Some(Version::new([7, 2])),
            Self::Rocm71 => Some(Version::new([7, 1])),
            Self::Rocm70 => Some(Version::new([7, 0])),
            Self::Rocm64 => Some(Version::new([6, 4])),
            Self::Rocm63 => Some(Version::new([6, 3])),
            Self::Rocm624 => Some(Version::new([6, 2, 4])),
            Self::Rocm62 => Some(Version::new([6, 2])),
            Self::Rocm61 => Some(Version::new([6, 1])),
            Self::Rocm60 => Some(Version::new([6, 0])),
            Self::Rocm57 => Some(Version::new([5, 7])),
            Self::Rocm56 => Some(Version::new([5, 6])),
            Self::Rocm55 => Some(Version::new([5, 5])),
            Self::Rocm542 => Some(Version::new([5, 4, 2])),
            Self::Rocm54 => Some(Version::new([5, 4])),
            Self::Rocm53 => Some(Version::new([5, 3])),
            Self::Rocm52 => Some(Version::new([5, 2])),
            Self::Rocm511 => Some(Version::new([5, 1, 1])),
            Self::Rocm42 => Some(Version::new([4, 2])),
            Self::Rocm41 => Some(Version::new([4, 1])),
            Self::Rocm401 => Some(Version::new([4, 0, 1])),
            _ => None,
        }
    }
}

impl FromStr for TorchBackend {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "cpu" => Ok(Self::Cpu),
            "cu132" => Ok(Self::Cu132),
            "cu130" => Ok(Self::Cu130),
            "cu129" => Ok(Self::Cu129),
            "cu128" => Ok(Self::Cu128),
            "cu126" => Ok(Self::Cu126),
            "cu125" => Ok(Self::Cu125),
            "cu124" => Ok(Self::Cu124),
            "cu123" => Ok(Self::Cu123),
            "cu122" => Ok(Self::Cu122),
            "cu121" => Ok(Self::Cu121),
            "cu120" => Ok(Self::Cu120),
            "cu118" => Ok(Self::Cu118),
            "cu117" => Ok(Self::Cu117),
            "cu116" => Ok(Self::Cu116),
            "cu115" => Ok(Self::Cu115),
            "cu114" => Ok(Self::Cu114),
            "cu113" => Ok(Self::Cu113),
            "cu112" => Ok(Self::Cu112),
            "cu111" => Ok(Self::Cu111),
            "cu110" => Ok(Self::Cu110),
            "cu102" => Ok(Self::Cu102),
            "cu101" => Ok(Self::Cu101),
            "cu100" => Ok(Self::Cu100),
            "cu92" => Ok(Self::Cu92),
            "cu91" => Ok(Self::Cu91),
            "cu90" => Ok(Self::Cu90),
            "cu80" => Ok(Self::Cu80),
            "rocm7.2" => Ok(Self::Rocm72),
            "rocm7.1" => Ok(Self::Rocm71),
            "rocm7.0" => Ok(Self::Rocm70),
            "rocm6.4" => Ok(Self::Rocm64),
            "rocm6.3" => Ok(Self::Rocm63),
            "rocm6.2.4" => Ok(Self::Rocm624),
            "rocm6.2" => Ok(Self::Rocm62),
            "rocm6.1" => Ok(Self::Rocm61),
            "rocm6.0" => Ok(Self::Rocm60),
            "rocm5.7" => Ok(Self::Rocm57),
            "rocm5.6" => Ok(Self::Rocm56),
            "rocm5.5" => Ok(Self::Rocm55),
            "rocm5.4.2" => Ok(Self::Rocm542),
            "rocm5.4" => Ok(Self::Rocm54),
            "rocm5.3" => Ok(Self::Rocm53),
            "rocm5.2" => Ok(Self::Rocm52),
            "rocm5.1.1" => Ok(Self::Rocm511),
            "rocm4.2" => Ok(Self::Rocm42),
            "rocm4.1" => Ok(Self::Rocm41),
            "rocm4.0.1" => Ok(Self::Rocm401),
            "xpu" => Ok(Self::Xpu),
            _ => Err(format!("Unknown PyTorch backend: {s}")),
        }
    }
}

/// Linux CUDA driver versions and the corresponding CUDA versions.
///
/// See: <https://github.com/pmeier/light-the-torch/blob/33397cbe45d07b51ad8ee76b004571a4c236e37f/light_the_torch/_cb.py#L150-L213>
static LINUX_CUDA_DRIVERS: LazyLock<[(TorchBackend, Version); 27]> = LazyLock::new(|| {
    [
        // Table 2 from
        // https://docs.nvidia.com/cuda/cuda-toolkit-release-notes/index.html
        (TorchBackend::Cu132, Version::new([580])),
        (TorchBackend::Cu130, Version::new([580])),
        (TorchBackend::Cu129, Version::new([525, 60, 13])),
        (TorchBackend::Cu128, Version::new([525, 60, 13])),
        (TorchBackend::Cu126, Version::new([525, 60, 13])),
        (TorchBackend::Cu125, Version::new([525, 60, 13])),
        (TorchBackend::Cu124, Version::new([525, 60, 13])),
        (TorchBackend::Cu123, Version::new([525, 60, 13])),
        (TorchBackend::Cu122, Version::new([525, 60, 13])),
        (TorchBackend::Cu121, Version::new([525, 60, 13])),
        (TorchBackend::Cu120, Version::new([525, 60, 13])),
        // Table 2 from
        // https://docs.nvidia.com/cuda/archive/11.8.0/cuda-toolkit-release-notes/index.html
        (TorchBackend::Cu118, Version::new([450, 80, 2])),
        (TorchBackend::Cu117, Version::new([450, 80, 2])),
        (TorchBackend::Cu116, Version::new([450, 80, 2])),
        (TorchBackend::Cu115, Version::new([450, 80, 2])),
        (TorchBackend::Cu114, Version::new([450, 80, 2])),
        (TorchBackend::Cu113, Version::new([450, 80, 2])),
        (TorchBackend::Cu112, Version::new([450, 80, 2])),
        (TorchBackend::Cu111, Version::new([450, 80, 2])),
        (TorchBackend::Cu110, Version::new([450, 36, 6])),
        // Table 1 from
        // https://docs.nvidia.com/cuda/archive/10.2/cuda-toolkit-release-notes/index.html
        (TorchBackend::Cu102, Version::new([440, 33])),
        (TorchBackend::Cu101, Version::new([418, 39])),
        (TorchBackend::Cu100, Version::new([410, 48])),
        (TorchBackend::Cu92, Version::new([396, 26])),
        (TorchBackend::Cu91, Version::new([390, 46])),
        (TorchBackend::Cu90, Version::new([384, 81])),
        (TorchBackend::Cu80, Version::new([375, 26])),
    ]
});

/// Windows CUDA driver versions and the corresponding CUDA versions.
///
/// See: <https://github.com/pmeier/light-the-torch/blob/33397cbe45d07b51ad8ee76b004571a4c236e37f/light_the_torch/_cb.py#L150-L213>
static WINDOWS_CUDA_VERSIONS: LazyLock<[(TorchBackend, Version); 27]> = LazyLock::new(|| {
    [
        // Table 2 from
        // https://docs.nvidia.com/cuda/cuda-toolkit-release-notes/index.html
        (TorchBackend::Cu132, Version::new([580])),
        (TorchBackend::Cu130, Version::new([580])),
        (TorchBackend::Cu129, Version::new([528, 33])),
        (TorchBackend::Cu128, Version::new([528, 33])),
        (TorchBackend::Cu126, Version::new([528, 33])),
        (TorchBackend::Cu125, Version::new([528, 33])),
        (TorchBackend::Cu124, Version::new([528, 33])),
        (TorchBackend::Cu123, Version::new([528, 33])),
        (TorchBackend::Cu122, Version::new([528, 33])),
        (TorchBackend::Cu121, Version::new([528, 33])),
        (TorchBackend::Cu120, Version::new([528, 33])),
        // Table 2 from
        // https://docs.nvidia.com/cuda/archive/11.8.0/cuda-toolkit-release-notes/index.html
        (TorchBackend::Cu118, Version::new([452, 39])),
        (TorchBackend::Cu117, Version::new([452, 39])),
        (TorchBackend::Cu116, Version::new([452, 39])),
        (TorchBackend::Cu115, Version::new([452, 39])),
        (TorchBackend::Cu114, Version::new([452, 39])),
        (TorchBackend::Cu113, Version::new([452, 39])),
        (TorchBackend::Cu112, Version::new([452, 39])),
        (TorchBackend::Cu111, Version::new([452, 39])),
        (TorchBackend::Cu110, Version::new([451, 22])),
        // Table 1 from
        // https://docs.nvidia.com/cuda/archive/10.2/cuda-toolkit-release-notes/index.html
        (TorchBackend::Cu102, Version::new([441, 22])),
        (TorchBackend::Cu101, Version::new([418, 96])),
        (TorchBackend::Cu100, Version::new([411, 31])),
        (TorchBackend::Cu92, Version::new([398, 26])),
        (TorchBackend::Cu91, Version::new([391, 29])),
        (TorchBackend::Cu90, Version::new([385, 54])),
        (TorchBackend::Cu80, Version::new([376, 51])),
    ]
});

/// Linux AMD GPU architectures and the corresponding PyTorch backends.
///
/// These were inferred by running the following snippet for each ROCm version:
///
/// ```python
/// import torch
///
/// print(torch.cuda.get_arch_list())
/// ```
///
/// AMD also provides a compatibility matrix: <https://rocm.docs.amd.com/en/latest/compatibility/compatibility-matrix.html>;
/// however, this list includes a broader array of GPUs than those in the matrix.
static LINUX_AMD_GPU_DRIVERS: LazyLock<[(TorchBackend, AmdGpuArchitecture); 93]> =
    LazyLock::new(|| {
        [
            // ROCm 7.2
            (TorchBackend::Rocm72, AmdGpuArchitecture::Gfx900),
            (TorchBackend::Rocm72, AmdGpuArchitecture::Gfx906),
            (TorchBackend::Rocm72, AmdGpuArchitecture::Gfx908),
            (TorchBackend::Rocm72, AmdGpuArchitecture::Gfx90a),
            (TorchBackend::Rocm72, AmdGpuArchitecture::Gfx942),
            (TorchBackend::Rocm72, AmdGpuArchitecture::Gfx950),
            (TorchBackend::Rocm72, AmdGpuArchitecture::Gfx1030),
            (TorchBackend::Rocm72, AmdGpuArchitecture::Gfx1100),
            (TorchBackend::Rocm72, AmdGpuArchitecture::Gfx1101),
            (TorchBackend::Rocm72, AmdGpuArchitecture::Gfx1102),
            (TorchBackend::Rocm72, AmdGpuArchitecture::Gfx1150),
            (TorchBackend::Rocm72, AmdGpuArchitecture::Gfx1151),
            (TorchBackend::Rocm72, AmdGpuArchitecture::Gfx1200),
            (TorchBackend::Rocm72, AmdGpuArchitecture::Gfx1201),
            // ROCm 7.1
            (TorchBackend::Rocm71, AmdGpuArchitecture::Gfx900),
            (TorchBackend::Rocm71, AmdGpuArchitecture::Gfx906),
            (TorchBackend::Rocm71, AmdGpuArchitecture::Gfx908),
            (TorchBackend::Rocm71, AmdGpuArchitecture::Gfx90a),
            (TorchBackend::Rocm71, AmdGpuArchitecture::Gfx942),
            (TorchBackend::Rocm71, AmdGpuArchitecture::Gfx950),
            (TorchBackend::Rocm71, AmdGpuArchitecture::Gfx1030),
            (TorchBackend::Rocm71, AmdGpuArchitecture::Gfx1100),
            (TorchBackend::Rocm71, AmdGpuArchitecture::Gfx1101),
            (TorchBackend::Rocm71, AmdGpuArchitecture::Gfx1102),
            (TorchBackend::Rocm71, AmdGpuArchitecture::Gfx1200),
            (TorchBackend::Rocm71, AmdGpuArchitecture::Gfx1201),
            // ROCm 7.0
            (TorchBackend::Rocm70, AmdGpuArchitecture::Gfx900),
            (TorchBackend::Rocm70, AmdGpuArchitecture::Gfx906),
            (TorchBackend::Rocm70, AmdGpuArchitecture::Gfx908),
            (TorchBackend::Rocm70, AmdGpuArchitecture::Gfx90a),
            (TorchBackend::Rocm70, AmdGpuArchitecture::Gfx942),
            (TorchBackend::Rocm70, AmdGpuArchitecture::Gfx950),
            (TorchBackend::Rocm70, AmdGpuArchitecture::Gfx1030),
            (TorchBackend::Rocm70, AmdGpuArchitecture::Gfx1100),
            (TorchBackend::Rocm70, AmdGpuArchitecture::Gfx1101),
            (TorchBackend::Rocm70, AmdGpuArchitecture::Gfx1102),
            (TorchBackend::Rocm70, AmdGpuArchitecture::Gfx1200),
            (TorchBackend::Rocm70, AmdGpuArchitecture::Gfx1201),
            // ROCm 6.4
            (TorchBackend::Rocm64, AmdGpuArchitecture::Gfx900),
            (TorchBackend::Rocm64, AmdGpuArchitecture::Gfx906),
            (TorchBackend::Rocm64, AmdGpuArchitecture::Gfx908),
            (TorchBackend::Rocm64, AmdGpuArchitecture::Gfx90a),
            (TorchBackend::Rocm64, AmdGpuArchitecture::Gfx942),
            (TorchBackend::Rocm64, AmdGpuArchitecture::Gfx1030),
            (TorchBackend::Rocm64, AmdGpuArchitecture::Gfx1100),
            (TorchBackend::Rocm64, AmdGpuArchitecture::Gfx1101),
            (TorchBackend::Rocm64, AmdGpuArchitecture::Gfx1102),
            (TorchBackend::Rocm64, AmdGpuArchitecture::Gfx1200),
            (TorchBackend::Rocm64, AmdGpuArchitecture::Gfx1201),
            // ROCm 6.3
            (TorchBackend::Rocm63, AmdGpuArchitecture::Gfx900),
            (TorchBackend::Rocm63, AmdGpuArchitecture::Gfx906),
            (TorchBackend::Rocm63, AmdGpuArchitecture::Gfx908),
            (TorchBackend::Rocm63, AmdGpuArchitecture::Gfx90a),
            (TorchBackend::Rocm63, AmdGpuArchitecture::Gfx942),
            (TorchBackend::Rocm63, AmdGpuArchitecture::Gfx1030),
            (TorchBackend::Rocm63, AmdGpuArchitecture::Gfx1100),
            (TorchBackend::Rocm63, AmdGpuArchitecture::Gfx1101),
            (TorchBackend::Rocm63, AmdGpuArchitecture::Gfx1102),
            (TorchBackend::Rocm63, AmdGpuArchitecture::Gfx1200),
            (TorchBackend::Rocm63, AmdGpuArchitecture::Gfx1201),
            // ROCm 6.2.4
            (TorchBackend::Rocm624, AmdGpuArchitecture::Gfx900),
            (TorchBackend::Rocm624, AmdGpuArchitecture::Gfx906),
            (TorchBackend::Rocm624, AmdGpuArchitecture::Gfx908),
            (TorchBackend::Rocm624, AmdGpuArchitecture::Gfx90a),
            (TorchBackend::Rocm624, AmdGpuArchitecture::Gfx942),
            (TorchBackend::Rocm624, AmdGpuArchitecture::Gfx1030),
            (TorchBackend::Rocm624, AmdGpuArchitecture::Gfx1100),
            (TorchBackend::Rocm624, AmdGpuArchitecture::Gfx1101),
            (TorchBackend::Rocm624, AmdGpuArchitecture::Gfx1102),
            (TorchBackend::Rocm624, AmdGpuArchitecture::Gfx1200),
            (TorchBackend::Rocm624, AmdGpuArchitecture::Gfx1201),
            // ROCm 6.2
            (TorchBackend::Rocm62, AmdGpuArchitecture::Gfx900),
            (TorchBackend::Rocm62, AmdGpuArchitecture::Gfx906),
            (TorchBackend::Rocm62, AmdGpuArchitecture::Gfx908),
            (TorchBackend::Rocm62, AmdGpuArchitecture::Gfx90a),
            (TorchBackend::Rocm62, AmdGpuArchitecture::Gfx1030),
            (TorchBackend::Rocm62, AmdGpuArchitecture::Gfx1100),
            (TorchBackend::Rocm62, AmdGpuArchitecture::Gfx942),
            // ROCm 6.1
            (TorchBackend::Rocm61, AmdGpuArchitecture::Gfx900),
            (TorchBackend::Rocm61, AmdGpuArchitecture::Gfx906),
            (TorchBackend::Rocm61, AmdGpuArchitecture::Gfx908),
            (TorchBackend::Rocm61, AmdGpuArchitecture::Gfx90a),
            (TorchBackend::Rocm61, AmdGpuArchitecture::Gfx942),
            (TorchBackend::Rocm61, AmdGpuArchitecture::Gfx1030),
            (TorchBackend::Rocm61, AmdGpuArchitecture::Gfx1100),
            (TorchBackend::Rocm61, AmdGpuArchitecture::Gfx1101),
            // ROCm 6.0
            (TorchBackend::Rocm60, AmdGpuArchitecture::Gfx900),
            (TorchBackend::Rocm60, AmdGpuArchitecture::Gfx906),
            (TorchBackend::Rocm60, AmdGpuArchitecture::Gfx908),
            (TorchBackend::Rocm60, AmdGpuArchitecture::Gfx90a),
            (TorchBackend::Rocm60, AmdGpuArchitecture::Gfx1030),
            (TorchBackend::Rocm60, AmdGpuArchitecture::Gfx1100),
            (TorchBackend::Rocm60, AmdGpuArchitecture::Gfx942),
        ]
    });

const PYTORCH_INDEX_BASE_URL: &str = "https://download.pytorch.org/whl/";

static PYX_API_BASE_URL: LazyLock<Cow<'static, str>> = LazyLock::new(|| {
    std::env::var(EnvVars::PYX_API_URL)
        .map(Cow::Owned)
        .unwrap_or(Cow::Borrowed("https://api.pyx.dev"))
});
