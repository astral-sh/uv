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

use either::Either;
use url::Url;

use uv_distribution_types::IndexUrl;
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_platform_tags::Os;
use uv_static::EnvVars;

use crate::{Accelerator, AcceleratorError, AmdGpuArchitecture};

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
    },
    /// Select the appropriate PyTorch index based on the operating system and AMD GPU architecture (e.g., `gfx1100`).
    Amd {
        os: Os,
        gpu_architecture: AmdGpuArchitecture,
        source: TorchSource,
    },
    /// Select the appropriate PyTorch index based on the operating system and Intel GPU presence.
    Xpu { os: Os, source: TorchSource },
    /// Use the specified PyTorch index.
    Backend {
        backend: TorchBackend,
        source: TorchSource,
    },
}

impl TorchStrategy {
    /// Determine the [`TorchStrategy`] from the given [`TorchMode`], [`Os`], and [`Accelerator`].
    pub fn from_mode(
        mode: TorchMode,
        source: TorchSource,
        os: &Os,
    ) -> Result<Self, AcceleratorError> {
        let backend = match mode {
            TorchMode::Auto => match Accelerator::detect()? {
                Some(Accelerator::Cuda { driver_version }) => {
                    return Ok(Self::Cuda {
                        os: os.clone(),
                        driver_version: driver_version.clone(),
                        source,
                    });
                }
                Some(Accelerator::Amd { gpu_architecture }) => {
                    return Ok(Self::Amd {
                        os: os.clone(),
                        gpu_architecture,
                        source,
                    });
                }
                Some(Accelerator::Xpu) => {
                    return Ok(Self::Xpu {
                        os: os.clone(),
                        source,
                    });
                }
                None => TorchBackend::Cpu,
            },
            TorchMode::Cpu => TorchBackend::Cpu,
            TorchMode::Cu130 => TorchBackend::Cu130,
            TorchMode::Cu129 => TorchBackend::Cu129,
            TorchMode::Cu128 => TorchBackend::Cu128,
            TorchMode::Cu126 => TorchBackend::Cu126,
            TorchMode::Cu125 => TorchBackend::Cu125,
            TorchMode::Cu124 => TorchBackend::Cu124,
            TorchMode::Cu123 => TorchBackend::Cu123,
            TorchMode::Cu122 => TorchBackend::Cu122,
            TorchMode::Cu121 => TorchBackend::Cu121,
            TorchMode::Cu120 => TorchBackend::Cu120,
            TorchMode::Cu118 => TorchBackend::Cu118,
            TorchMode::Cu117 => TorchBackend::Cu117,
            TorchMode::Cu116 => TorchBackend::Cu116,
            TorchMode::Cu115 => TorchBackend::Cu115,
            TorchMode::Cu114 => TorchBackend::Cu114,
            TorchMode::Cu113 => TorchBackend::Cu113,
            TorchMode::Cu112 => TorchBackend::Cu112,
            TorchMode::Cu111 => TorchBackend::Cu111,
            TorchMode::Cu110 => TorchBackend::Cu110,
            TorchMode::Cu102 => TorchBackend::Cu102,
            TorchMode::Cu101 => TorchBackend::Cu101,
            TorchMode::Cu100 => TorchBackend::Cu100,
            TorchMode::Cu92 => TorchBackend::Cu92,
            TorchMode::Cu91 => TorchBackend::Cu91,
            TorchMode::Cu90 => TorchBackend::Cu90,
            TorchMode::Cu80 => TorchBackend::Cu80,
            TorchMode::Rocm64 => TorchBackend::Rocm64,
            TorchMode::Rocm63 => TorchBackend::Rocm63,
            TorchMode::Rocm624 => TorchBackend::Rocm624,
            TorchMode::Rocm62 => TorchBackend::Rocm62,
            TorchMode::Rocm61 => TorchBackend::Rocm61,
            TorchMode::Rocm60 => TorchBackend::Rocm60,
            TorchMode::Rocm57 => TorchBackend::Rocm57,
            TorchMode::Rocm56 => TorchBackend::Rocm56,
            TorchMode::Rocm55 => TorchBackend::Rocm55,
            TorchMode::Rocm542 => TorchBackend::Rocm542,
            TorchMode::Rocm54 => TorchBackend::Rocm54,
            TorchMode::Rocm53 => TorchBackend::Rocm53,
            TorchMode::Rocm52 => TorchBackend::Rocm52,
            TorchMode::Rocm511 => TorchBackend::Rocm511,
            TorchMode::Rocm42 => TorchBackend::Rocm42,
            TorchMode::Rocm41 => TorchBackend::Rocm41,
            TorchMode::Rocm401 => TorchBackend::Rocm401,
            TorchMode::Xpu => TorchBackend::Xpu,
        };
        Ok(Self::Backend { backend, source })
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
                    "pytorch-triton"
                        | "pytorch-triton-rocm"
                        | "pytorch-triton-xpu"
                        | "torch"
                        | "torch-tensorrt"
                        | "torchao"
                        | "torcharrow"
                        | "torchaudio"
                        | "torchcsprng"
                        | "torchdata"
                        | "torchdistx"
                        | "torchserve"
                        | "torchtext"
                        | "torchvision"
                        | "triton"
                )
            }
            TorchSource::Pyx => {
                matches!(
                    package_name.as_str(),
                    "deepspeed"
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
                        | "torchcsprng"
                        | "torchdata"
                        | "torchdistx"
                        | "torchserve"
                        | "torchtext"
                        | "torchvision"
                        | "triton"
                        | "vllm"
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
                | "flash-attn"
                | "flash-attn-3"
                | "megablocks"
                | "natten"
                | "torch"
                | "torch-tensorrt"
                | "torchao"
                | "torcharrow"
                | "torchaudio"
                | "torchcsprng"
                | "torchdata"
                | "torchdistx"
                | "torchtext"
                | "torchvision"
                | "vllm"
        )
    }

    /// Return the appropriate index URLs for the given [`TorchStrategy`].
    pub fn index_urls(&self) -> impl Iterator<Item = &IndexUrl> {
        match self {
            Self::Cuda {
                os,
                driver_version,
                source,
            } => {
                // If this is a GPU-enabled package, and CUDA drivers are installed, use PyTorch's CUDA
                // indexes.
                //
                // See: https://github.com/pmeier/light-the-torch/blob/33397cbe45d07b51ad8ee76b004571a4c236e37f/light_the_torch/_patch.py#L36-L49
                match os {
                    Os::Manylinux { .. } | Os::Musllinux { .. } => {
                        Either::Left(Either::Left(Either::Left(
                            LINUX_CUDA_DRIVERS
                                .iter()
                                .filter_map(move |(backend, version)| {
                                    if driver_version >= version {
                                        Some(backend.index_url(*source))
                                    } else {
                                        None
                                    }
                                })
                                .chain(std::iter::once(TorchBackend::Cpu.index_url(*source))),
                        )))
                    }
                    Os::Windows => Either::Left(Either::Left(Either::Right(
                        WINDOWS_CUDA_VERSIONS
                            .iter()
                            .filter_map(move |(backend, version)| {
                                if driver_version >= version {
                                    Some(backend.index_url(*source))
                                } else {
                                    None
                                }
                            })
                            .chain(std::iter::once(TorchBackend::Cpu.index_url(*source))),
                    ))),
                    Os::Macos { .. }
                    | Os::FreeBsd { .. }
                    | Os::NetBsd { .. }
                    | Os::OpenBsd { .. }
                    | Os::Dragonfly { .. }
                    | Os::Illumos { .. }
                    | Os::Haiku { .. }
                    | Os::Android { .. }
                    | Os::Pyodide { .. }
                    | Os::Ios { .. } => Either::Right(Either::Left(std::iter::once(
                        TorchBackend::Cpu.index_url(*source),
                    ))),
                }
            }
            Self::Amd {
                os,
                gpu_architecture,
                source,
            } => match os {
                Os::Manylinux { .. } | Os::Musllinux { .. } => Either::Left(Either::Right(
                    LINUX_AMD_GPU_DRIVERS
                        .iter()
                        .filter_map(move |(backend, architecture)| {
                            if gpu_architecture == architecture {
                                Some(backend.index_url(*source))
                            } else {
                                None
                            }
                        })
                        .chain(std::iter::once(TorchBackend::Cpu.index_url(*source))),
                )),
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
                | Os::Ios { .. } => Either::Right(Either::Left(std::iter::once(
                    TorchBackend::Cpu.index_url(*source),
                ))),
            },
            Self::Xpu { os, source } => match os {
                Os::Manylinux { .. } | Os::Windows => Either::Right(Either::Right(Either::Left(
                    std::iter::once(TorchBackend::Xpu.index_url(*source)),
                ))),
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
                | Os::Ios { .. } => Either::Right(Either::Left(std::iter::once(
                    TorchBackend::Cpu.index_url(*source),
                ))),
            },
            Self::Backend { backend, source } => Either::Right(Either::Right(Either::Right(
                std::iter::once(backend.index_url(*source)),
            ))),
        }
    }
}

/// The available backends for PyTorch.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum TorchBackend {
    Cpu,
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
    /// Return the appropriate index URL for the given [`TorchBackend`].
    fn index_url(self, source: TorchSource) -> &'static IndexUrl {
        match self {
            Self::Cpu => match source {
                TorchSource::PyTorch => &PYTORCH_CPU_INDEX_URL,
                TorchSource::Pyx => &PYX_CPU_INDEX_URL,
            },
            Self::Cu130 => match source {
                TorchSource::PyTorch => &PYTORCH_CU130_INDEX_URL,
                TorchSource::Pyx => &PYX_CU130_INDEX_URL,
            },
            Self::Cu129 => match source {
                TorchSource::PyTorch => &PYTORCH_CU129_INDEX_URL,
                TorchSource::Pyx => &PYX_CU129_INDEX_URL,
            },
            Self::Cu128 => match source {
                TorchSource::PyTorch => &PYTORCH_CU128_INDEX_URL,
                TorchSource::Pyx => &PYX_CU128_INDEX_URL,
            },
            Self::Cu126 => match source {
                TorchSource::PyTorch => &PYTORCH_CU126_INDEX_URL,
                TorchSource::Pyx => &PYX_CU126_INDEX_URL,
            },
            Self::Cu125 => match source {
                TorchSource::PyTorch => &PYTORCH_CU125_INDEX_URL,
                TorchSource::Pyx => &PYX_CU125_INDEX_URL,
            },
            Self::Cu124 => match source {
                TorchSource::PyTorch => &PYTORCH_CU124_INDEX_URL,
                TorchSource::Pyx => &PYX_CU124_INDEX_URL,
            },
            Self::Cu123 => match source {
                TorchSource::PyTorch => &PYTORCH_CU123_INDEX_URL,
                TorchSource::Pyx => &PYX_CU123_INDEX_URL,
            },
            Self::Cu122 => match source {
                TorchSource::PyTorch => &PYTORCH_CU122_INDEX_URL,
                TorchSource::Pyx => &PYX_CU122_INDEX_URL,
            },
            Self::Cu121 => match source {
                TorchSource::PyTorch => &PYTORCH_CU121_INDEX_URL,
                TorchSource::Pyx => &PYX_CU121_INDEX_URL,
            },
            Self::Cu120 => match source {
                TorchSource::PyTorch => &PYTORCH_CU120_INDEX_URL,
                TorchSource::Pyx => &PYX_CU120_INDEX_URL,
            },
            Self::Cu118 => match source {
                TorchSource::PyTorch => &PYTORCH_CU118_INDEX_URL,
                TorchSource::Pyx => &PYX_CU118_INDEX_URL,
            },
            Self::Cu117 => match source {
                TorchSource::PyTorch => &PYTORCH_CU117_INDEX_URL,
                TorchSource::Pyx => &PYX_CU117_INDEX_URL,
            },
            Self::Cu116 => match source {
                TorchSource::PyTorch => &PYTORCH_CU116_INDEX_URL,
                TorchSource::Pyx => &PYX_CU116_INDEX_URL,
            },
            Self::Cu115 => match source {
                TorchSource::PyTorch => &PYTORCH_CU115_INDEX_URL,
                TorchSource::Pyx => &PYX_CU115_INDEX_URL,
            },
            Self::Cu114 => match source {
                TorchSource::PyTorch => &PYTORCH_CU114_INDEX_URL,
                TorchSource::Pyx => &PYX_CU114_INDEX_URL,
            },
            Self::Cu113 => match source {
                TorchSource::PyTorch => &PYTORCH_CU113_INDEX_URL,
                TorchSource::Pyx => &PYX_CU113_INDEX_URL,
            },
            Self::Cu112 => match source {
                TorchSource::PyTorch => &PYTORCH_CU112_INDEX_URL,
                TorchSource::Pyx => &PYX_CU112_INDEX_URL,
            },
            Self::Cu111 => match source {
                TorchSource::PyTorch => &PYTORCH_CU111_INDEX_URL,
                TorchSource::Pyx => &PYX_CU111_INDEX_URL,
            },
            Self::Cu110 => match source {
                TorchSource::PyTorch => &PYTORCH_CU110_INDEX_URL,
                TorchSource::Pyx => &PYX_CU110_INDEX_URL,
            },
            Self::Cu102 => match source {
                TorchSource::PyTorch => &PYTORCH_CU102_INDEX_URL,
                TorchSource::Pyx => &PYX_CU102_INDEX_URL,
            },
            Self::Cu101 => match source {
                TorchSource::PyTorch => &PYTORCH_CU101_INDEX_URL,
                TorchSource::Pyx => &PYX_CU101_INDEX_URL,
            },
            Self::Cu100 => match source {
                TorchSource::PyTorch => &PYTORCH_CU100_INDEX_URL,
                TorchSource::Pyx => &PYX_CU100_INDEX_URL,
            },
            Self::Cu92 => match source {
                TorchSource::PyTorch => &PYTORCH_CU92_INDEX_URL,
                TorchSource::Pyx => &PYX_CU92_INDEX_URL,
            },
            Self::Cu91 => match source {
                TorchSource::PyTorch => &PYTORCH_CU91_INDEX_URL,
                TorchSource::Pyx => &PYX_CU91_INDEX_URL,
            },
            Self::Cu90 => match source {
                TorchSource::PyTorch => &PYTORCH_CU90_INDEX_URL,
                TorchSource::Pyx => &PYX_CU90_INDEX_URL,
            },
            Self::Cu80 => match source {
                TorchSource::PyTorch => &PYTORCH_CU80_INDEX_URL,
                TorchSource::Pyx => &PYX_CU80_INDEX_URL,
            },
            Self::Rocm64 => match source {
                TorchSource::PyTorch => &PYTORCH_ROCM64_INDEX_URL,
                TorchSource::Pyx => &PYX_ROCM64_INDEX_URL,
            },
            Self::Rocm63 => match source {
                TorchSource::PyTorch => &PYTORCH_ROCM63_INDEX_URL,
                TorchSource::Pyx => &PYX_ROCM63_INDEX_URL,
            },
            Self::Rocm624 => match source {
                TorchSource::PyTorch => &PYTORCH_ROCM624_INDEX_URL,
                TorchSource::Pyx => &PYX_ROCM624_INDEX_URL,
            },
            Self::Rocm62 => match source {
                TorchSource::PyTorch => &PYTORCH_ROCM62_INDEX_URL,
                TorchSource::Pyx => &PYX_ROCM62_INDEX_URL,
            },
            Self::Rocm61 => match source {
                TorchSource::PyTorch => &PYTORCH_ROCM61_INDEX_URL,
                TorchSource::Pyx => &PYX_ROCM61_INDEX_URL,
            },
            Self::Rocm60 => match source {
                TorchSource::PyTorch => &PYTORCH_ROCM60_INDEX_URL,
                TorchSource::Pyx => &PYX_ROCM60_INDEX_URL,
            },
            Self::Rocm57 => match source {
                TorchSource::PyTorch => &PYTORCH_ROCM57_INDEX_URL,
                TorchSource::Pyx => &PYX_ROCM57_INDEX_URL,
            },
            Self::Rocm56 => match source {
                TorchSource::PyTorch => &PYTORCH_ROCM56_INDEX_URL,
                TorchSource::Pyx => &PYX_ROCM56_INDEX_URL,
            },
            Self::Rocm55 => match source {
                TorchSource::PyTorch => &PYTORCH_ROCM55_INDEX_URL,
                TorchSource::Pyx => &PYX_ROCM55_INDEX_URL,
            },
            Self::Rocm542 => match source {
                TorchSource::PyTorch => &PYTORCH_ROCM542_INDEX_URL,
                TorchSource::Pyx => &PYX_ROCM542_INDEX_URL,
            },
            Self::Rocm54 => match source {
                TorchSource::PyTorch => &PYTORCH_ROCM54_INDEX_URL,
                TorchSource::Pyx => &PYX_ROCM54_INDEX_URL,
            },
            Self::Rocm53 => match source {
                TorchSource::PyTorch => &PYTORCH_ROCM53_INDEX_URL,
                TorchSource::Pyx => &PYX_ROCM53_INDEX_URL,
            },
            Self::Rocm52 => match source {
                TorchSource::PyTorch => &PYTORCH_ROCM52_INDEX_URL,
                TorchSource::Pyx => &PYX_ROCM52_INDEX_URL,
            },
            Self::Rocm511 => match source {
                TorchSource::PyTorch => &PYTORCH_ROCM511_INDEX_URL,
                TorchSource::Pyx => &PYX_ROCM511_INDEX_URL,
            },
            Self::Rocm42 => match source {
                TorchSource::PyTorch => &PYTORCH_ROCM42_INDEX_URL,
                TorchSource::Pyx => &PYX_ROCM42_INDEX_URL,
            },
            Self::Rocm41 => match source {
                TorchSource::PyTorch => &PYTORCH_ROCM41_INDEX_URL,
                TorchSource::Pyx => &PYX_ROCM41_INDEX_URL,
            },
            Self::Rocm401 => match source {
                TorchSource::PyTorch => &PYTORCH_ROCM401_INDEX_URL,
                TorchSource::Pyx => &PYX_ROCM401_INDEX_URL,
            },
            Self::Xpu => match source {
                TorchSource::PyTorch => &PYTORCH_XPU_INDEX_URL,
                TorchSource::Pyx => &PYX_XPU_INDEX_URL,
            },
        }
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
        match self {
            Self::Cpu => None,
            Self::Cu130 => Some(Version::new([13, 0])),
            Self::Cu129 => Some(Version::new([12, 9])),
            Self::Cu128 => Some(Version::new([12, 8])),
            Self::Cu126 => Some(Version::new([12, 6])),
            Self::Cu125 => Some(Version::new([12, 5])),
            Self::Cu124 => Some(Version::new([12, 4])),
            Self::Cu123 => Some(Version::new([12, 3])),
            Self::Cu122 => Some(Version::new([12, 2])),
            Self::Cu121 => Some(Version::new([12, 1])),
            Self::Cu120 => Some(Version::new([12, 0])),
            Self::Cu118 => Some(Version::new([11, 8])),
            Self::Cu117 => Some(Version::new([11, 7])),
            Self::Cu116 => Some(Version::new([11, 6])),
            Self::Cu115 => Some(Version::new([11, 5])),
            Self::Cu114 => Some(Version::new([11, 4])),
            Self::Cu113 => Some(Version::new([11, 3])),
            Self::Cu112 => Some(Version::new([11, 2])),
            Self::Cu111 => Some(Version::new([11, 1])),
            Self::Cu110 => Some(Version::new([11, 0])),
            Self::Cu102 => Some(Version::new([10, 2])),
            Self::Cu101 => Some(Version::new([10, 1])),
            Self::Cu100 => Some(Version::new([10, 0])),
            Self::Cu92 => Some(Version::new([9, 2])),
            Self::Cu91 => Some(Version::new([9, 1])),
            Self::Cu90 => Some(Version::new([9, 0])),
            Self::Cu80 => Some(Version::new([8, 0])),
            Self::Rocm64 => None,
            Self::Rocm63 => None,
            Self::Rocm624 => None,
            Self::Rocm62 => None,
            Self::Rocm61 => None,
            Self::Rocm60 => None,
            Self::Rocm57 => None,
            Self::Rocm56 => None,
            Self::Rocm55 => None,
            Self::Rocm542 => None,
            Self::Rocm54 => None,
            Self::Rocm53 => None,
            Self::Rocm52 => None,
            Self::Rocm511 => None,
            Self::Rocm42 => None,
            Self::Rocm41 => None,
            Self::Rocm401 => None,
            Self::Xpu => None,
        }
    }

    /// Returns the ROCM [`Version`] for the given [`TorchBackend`].
    pub fn rocm_version(&self) -> Option<Version> {
        match self {
            Self::Cpu => None,
            Self::Cu130 => None,
            Self::Cu129 => None,
            Self::Cu128 => None,
            Self::Cu126 => None,
            Self::Cu125 => None,
            Self::Cu124 => None,
            Self::Cu123 => None,
            Self::Cu122 => None,
            Self::Cu121 => None,
            Self::Cu120 => None,
            Self::Cu118 => None,
            Self::Cu117 => None,
            Self::Cu116 => None,
            Self::Cu115 => None,
            Self::Cu114 => None,
            Self::Cu113 => None,
            Self::Cu112 => None,
            Self::Cu111 => None,
            Self::Cu110 => None,
            Self::Cu102 => None,
            Self::Cu101 => None,
            Self::Cu100 => None,
            Self::Cu92 => None,
            Self::Cu91 => None,
            Self::Cu90 => None,
            Self::Cu80 => None,
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
            Self::Xpu => None,
        }
    }
}

impl FromStr for TorchBackend {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "cpu" => Ok(Self::Cpu),
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
static LINUX_CUDA_DRIVERS: LazyLock<[(TorchBackend, Version); 26]> = LazyLock::new(|| {
    [
        // Table 2 from
        // https://docs.nvidia.com/cuda/cuda-toolkit-release-notes/index.html
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
static WINDOWS_CUDA_VERSIONS: LazyLock<[(TorchBackend, Version); 26]> = LazyLock::new(|| {
    [
        // Table 2 from
        // https://docs.nvidia.com/cuda/cuda-toolkit-release-notes/index.html
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
static LINUX_AMD_GPU_DRIVERS: LazyLock<[(TorchBackend, AmdGpuArchitecture); 55]> =
    LazyLock::new(|| {
        [
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

static PYTORCH_CPU_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/cpu").unwrap());
static PYTORCH_CU130_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/cu130").unwrap());
static PYTORCH_CU129_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/cu129").unwrap());
static PYTORCH_CU128_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/cu128").unwrap());
static PYTORCH_CU126_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/cu126").unwrap());
static PYTORCH_CU125_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/cu125").unwrap());
static PYTORCH_CU124_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/cu124").unwrap());
static PYTORCH_CU123_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/cu123").unwrap());
static PYTORCH_CU122_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/cu122").unwrap());
static PYTORCH_CU121_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/cu121").unwrap());
static PYTORCH_CU120_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/cu120").unwrap());
static PYTORCH_CU118_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/cu118").unwrap());
static PYTORCH_CU117_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/cu117").unwrap());
static PYTORCH_CU116_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/cu116").unwrap());
static PYTORCH_CU115_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/cu115").unwrap());
static PYTORCH_CU114_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/cu114").unwrap());
static PYTORCH_CU113_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/cu113").unwrap());
static PYTORCH_CU112_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/cu112").unwrap());
static PYTORCH_CU111_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/cu111").unwrap());
static PYTORCH_CU110_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/cu110").unwrap());
static PYTORCH_CU102_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/cu102").unwrap());
static PYTORCH_CU101_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/cu101").unwrap());
static PYTORCH_CU100_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/cu100").unwrap());
static PYTORCH_CU92_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/cu92").unwrap());
static PYTORCH_CU91_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/cu91").unwrap());
static PYTORCH_CU90_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/cu90").unwrap());
static PYTORCH_CU80_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/cu80").unwrap());
static PYTORCH_ROCM64_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/rocm6.4").unwrap());
static PYTORCH_ROCM63_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/rocm6.3").unwrap());
static PYTORCH_ROCM624_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/rocm6.2.4").unwrap());
static PYTORCH_ROCM62_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/rocm6.2").unwrap());
static PYTORCH_ROCM61_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/rocm6.1").unwrap());
static PYTORCH_ROCM60_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/rocm6.0").unwrap());
static PYTORCH_ROCM57_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/rocm5.7").unwrap());
static PYTORCH_ROCM56_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/rocm5.6").unwrap());
static PYTORCH_ROCM55_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/rocm5.5").unwrap());
static PYTORCH_ROCM542_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/rocm5.4.2").unwrap());
static PYTORCH_ROCM54_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/rocm5.4").unwrap());
static PYTORCH_ROCM53_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/rocm5.3").unwrap());
static PYTORCH_ROCM52_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/rocm5.2").unwrap());
static PYTORCH_ROCM511_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/rocm5.1.1").unwrap());
static PYTORCH_ROCM42_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/rocm4.2").unwrap());
static PYTORCH_ROCM41_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/rocm4.1").unwrap());
static PYTORCH_ROCM401_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/rocm4.0.1").unwrap());
static PYTORCH_XPU_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::from_str("https://download.pytorch.org/whl/xpu").unwrap());

static PYX_API_BASE_URL: LazyLock<Cow<'static, str>> = LazyLock::new(|| {
    std::env::var(EnvVars::PYX_API_URL)
        .map(Cow::Owned)
        .unwrap_or(Cow::Borrowed("https://api.pyx.dev"))
});
static PYX_CPU_INDEX_URL: LazyLock<IndexUrl> = LazyLock::new(|| {
    let api_base_url = &*PYX_API_BASE_URL;
    IndexUrl::from_str(&format!("{api_base_url}/simple/astral-sh/cpu")).unwrap()
});
static PYX_CU130_INDEX_URL: LazyLock<IndexUrl> = LazyLock::new(|| {
    let api_base_url = &*PYX_API_BASE_URL;
    IndexUrl::from_str(&format!("{api_base_url}/simple/astral-sh/cu130")).unwrap()
});
static PYX_CU129_INDEX_URL: LazyLock<IndexUrl> = LazyLock::new(|| {
    let api_base_url = &*PYX_API_BASE_URL;
    IndexUrl::from_str(&format!("{api_base_url}/simple/astral-sh/cu129")).unwrap()
});
static PYX_CU128_INDEX_URL: LazyLock<IndexUrl> = LazyLock::new(|| {
    let api_base_url = &*PYX_API_BASE_URL;
    IndexUrl::from_str(&format!("{api_base_url}/simple/astral-sh/cu128")).unwrap()
});
static PYX_CU126_INDEX_URL: LazyLock<IndexUrl> = LazyLock::new(|| {
    let api_base_url = &*PYX_API_BASE_URL;
    IndexUrl::from_str(&format!("{api_base_url}/simple/astral-sh/cu126")).unwrap()
});
static PYX_CU125_INDEX_URL: LazyLock<IndexUrl> = LazyLock::new(|| {
    let api_base_url = &*PYX_API_BASE_URL;
    IndexUrl::from_str(&format!("{api_base_url}/simple/astral-sh/cu125")).unwrap()
});
static PYX_CU124_INDEX_URL: LazyLock<IndexUrl> = LazyLock::new(|| {
    let api_base_url = &*PYX_API_BASE_URL;
    IndexUrl::from_str(&format!("{api_base_url}/simple/astral-sh/cu124")).unwrap()
});
static PYX_CU123_INDEX_URL: LazyLock<IndexUrl> = LazyLock::new(|| {
    let api_base_url = &*PYX_API_BASE_URL;
    IndexUrl::from_str(&format!("{api_base_url}/simple/astral-sh/cu123")).unwrap()
});
static PYX_CU122_INDEX_URL: LazyLock<IndexUrl> = LazyLock::new(|| {
    let api_base_url = &*PYX_API_BASE_URL;
    IndexUrl::from_str(&format!("{api_base_url}/simple/astral-sh/cu122")).unwrap()
});
static PYX_CU121_INDEX_URL: LazyLock<IndexUrl> = LazyLock::new(|| {
    let api_base_url = &*PYX_API_BASE_URL;
    IndexUrl::from_str(&format!("{api_base_url}/simple/astral-sh/cu121")).unwrap()
});
static PYX_CU120_INDEX_URL: LazyLock<IndexUrl> = LazyLock::new(|| {
    let api_base_url = &*PYX_API_BASE_URL;
    IndexUrl::from_str(&format!("{api_base_url}/simple/astral-sh/cu120")).unwrap()
});
static PYX_CU118_INDEX_URL: LazyLock<IndexUrl> = LazyLock::new(|| {
    let api_base_url = &*PYX_API_BASE_URL;
    IndexUrl::from_str(&format!("{api_base_url}/simple/astral-sh/cu118")).unwrap()
});
static PYX_CU117_INDEX_URL: LazyLock<IndexUrl> = LazyLock::new(|| {
    let api_base_url = &*PYX_API_BASE_URL;
    IndexUrl::from_str(&format!("{api_base_url}/simple/astral-sh/cu117")).unwrap()
});
static PYX_CU116_INDEX_URL: LazyLock<IndexUrl> = LazyLock::new(|| {
    let api_base_url = &*PYX_API_BASE_URL;
    IndexUrl::from_str(&format!("{api_base_url}/simple/astral-sh/cu116")).unwrap()
});
static PYX_CU115_INDEX_URL: LazyLock<IndexUrl> = LazyLock::new(|| {
    let api_base_url = &*PYX_API_BASE_URL;
    IndexUrl::from_str(&format!("{api_base_url}/simple/astral-sh/cu115")).unwrap()
});
static PYX_CU114_INDEX_URL: LazyLock<IndexUrl> = LazyLock::new(|| {
    let api_base_url = &*PYX_API_BASE_URL;
    IndexUrl::from_str(&format!("{api_base_url}/simple/astral-sh/cu114")).unwrap()
});
static PYX_CU113_INDEX_URL: LazyLock<IndexUrl> = LazyLock::new(|| {
    let api_base_url = &*PYX_API_BASE_URL;
    IndexUrl::from_str(&format!("{api_base_url}/simple/astral-sh/cu113")).unwrap()
});
static PYX_CU112_INDEX_URL: LazyLock<IndexUrl> = LazyLock::new(|| {
    let api_base_url = &*PYX_API_BASE_URL;
    IndexUrl::from_str(&format!("{api_base_url}/simple/astral-sh/cu112")).unwrap()
});
static PYX_CU111_INDEX_URL: LazyLock<IndexUrl> = LazyLock::new(|| {
    let api_base_url = &*PYX_API_BASE_URL;
    IndexUrl::from_str(&format!("{api_base_url}/simple/astral-sh/cu111")).unwrap()
});
static PYX_CU110_INDEX_URL: LazyLock<IndexUrl> = LazyLock::new(|| {
    let api_base_url = &*PYX_API_BASE_URL;
    IndexUrl::from_str(&format!("{api_base_url}/simple/astral-sh/cu110")).unwrap()
});
static PYX_CU102_INDEX_URL: LazyLock<IndexUrl> = LazyLock::new(|| {
    let api_base_url = &*PYX_API_BASE_URL;
    IndexUrl::from_str(&format!("{api_base_url}/simple/astral-sh/cu102")).unwrap()
});
static PYX_CU101_INDEX_URL: LazyLock<IndexUrl> = LazyLock::new(|| {
    let api_base_url = &*PYX_API_BASE_URL;
    IndexUrl::from_str(&format!("{api_base_url}/simple/astral-sh/cu101")).unwrap()
});
static PYX_CU100_INDEX_URL: LazyLock<IndexUrl> = LazyLock::new(|| {
    let api_base_url = &*PYX_API_BASE_URL;
    IndexUrl::from_str(&format!("{api_base_url}/simple/astral-sh/cu100")).unwrap()
});
static PYX_CU92_INDEX_URL: LazyLock<IndexUrl> = LazyLock::new(|| {
    let api_base_url = &*PYX_API_BASE_URL;
    IndexUrl::from_str(&format!("{api_base_url}/simple/astral-sh/cu92")).unwrap()
});
static PYX_CU91_INDEX_URL: LazyLock<IndexUrl> = LazyLock::new(|| {
    let api_base_url = &*PYX_API_BASE_URL;
    IndexUrl::from_str(&format!("{api_base_url}/simple/astral-sh/cu91")).unwrap()
});
static PYX_CU90_INDEX_URL: LazyLock<IndexUrl> = LazyLock::new(|| {
    let api_base_url = &*PYX_API_BASE_URL;
    IndexUrl::from_str(&format!("{api_base_url}/simple/astral-sh/cu90")).unwrap()
});
static PYX_CU80_INDEX_URL: LazyLock<IndexUrl> = LazyLock::new(|| {
    let api_base_url = &*PYX_API_BASE_URL;
    IndexUrl::from_str(&format!("{api_base_url}/simple/astral-sh/cu80")).unwrap()
});
static PYX_ROCM64_INDEX_URL: LazyLock<IndexUrl> = LazyLock::new(|| {
    let api_base_url = &*PYX_API_BASE_URL;
    IndexUrl::from_str(&format!("{api_base_url}/simple/astral-sh/rocm6.4")).unwrap()
});
static PYX_ROCM63_INDEX_URL: LazyLock<IndexUrl> = LazyLock::new(|| {
    let api_base_url = &*PYX_API_BASE_URL;
    IndexUrl::from_str(&format!("{api_base_url}/simple/astral-sh/rocm6.3")).unwrap()
});
static PYX_ROCM624_INDEX_URL: LazyLock<IndexUrl> = LazyLock::new(|| {
    let api_base_url = &*PYX_API_BASE_URL;
    IndexUrl::from_str(&format!("{api_base_url}/simple/astral-sh/rocm6.2.4")).unwrap()
});
static PYX_ROCM62_INDEX_URL: LazyLock<IndexUrl> = LazyLock::new(|| {
    let api_base_url = &*PYX_API_BASE_URL;
    IndexUrl::from_str(&format!("{api_base_url}/simple/astral-sh/rocm6.2")).unwrap()
});
static PYX_ROCM61_INDEX_URL: LazyLock<IndexUrl> = LazyLock::new(|| {
    let api_base_url = &*PYX_API_BASE_URL;
    IndexUrl::from_str(&format!("{api_base_url}/simple/astral-sh/rocm6.1")).unwrap()
});
static PYX_ROCM60_INDEX_URL: LazyLock<IndexUrl> = LazyLock::new(|| {
    let api_base_url = &*PYX_API_BASE_URL;
    IndexUrl::from_str(&format!("{api_base_url}/simple/astral-sh/rocm6.0")).unwrap()
});
static PYX_ROCM57_INDEX_URL: LazyLock<IndexUrl> = LazyLock::new(|| {
    let api_base_url = &*PYX_API_BASE_URL;
    IndexUrl::from_str(&format!("{api_base_url}/simple/astral-sh/rocm5.7")).unwrap()
});
static PYX_ROCM56_INDEX_URL: LazyLock<IndexUrl> = LazyLock::new(|| {
    let api_base_url = &*PYX_API_BASE_URL;
    IndexUrl::from_str(&format!("{api_base_url}/simple/astral-sh/rocm5.6")).unwrap()
});
static PYX_ROCM55_INDEX_URL: LazyLock<IndexUrl> = LazyLock::new(|| {
    let api_base_url = &*PYX_API_BASE_URL;
    IndexUrl::from_str(&format!("{api_base_url}/simple/astral-sh/rocm5.5")).unwrap()
});
static PYX_ROCM542_INDEX_URL: LazyLock<IndexUrl> = LazyLock::new(|| {
    let api_base_url = &*PYX_API_BASE_URL;
    IndexUrl::from_str(&format!("{api_base_url}/simple/astral-sh/rocm5.4.2")).unwrap()
});
static PYX_ROCM54_INDEX_URL: LazyLock<IndexUrl> = LazyLock::new(|| {
    let api_base_url = &*PYX_API_BASE_URL;
    IndexUrl::from_str(&format!("{api_base_url}/simple/astral-sh/rocm5.4")).unwrap()
});
static PYX_ROCM53_INDEX_URL: LazyLock<IndexUrl> = LazyLock::new(|| {
    let api_base_url = &*PYX_API_BASE_URL;
    IndexUrl::from_str(&format!("{api_base_url}/simple/astral-sh/rocm5.3")).unwrap()
});
static PYX_ROCM52_INDEX_URL: LazyLock<IndexUrl> = LazyLock::new(|| {
    let api_base_url = &*PYX_API_BASE_URL;
    IndexUrl::from_str(&format!("{api_base_url}/simple/astral-sh/rocm5.2")).unwrap()
});
static PYX_ROCM511_INDEX_URL: LazyLock<IndexUrl> = LazyLock::new(|| {
    let api_base_url = &*PYX_API_BASE_URL;
    IndexUrl::from_str(&format!("{api_base_url}/simple/astral-sh/rocm5.1.1")).unwrap()
});
static PYX_ROCM42_INDEX_URL: LazyLock<IndexUrl> = LazyLock::new(|| {
    let api_base_url = &*PYX_API_BASE_URL;
    IndexUrl::from_str(&format!("{api_base_url}/simple/astral-sh/rocm4.2")).unwrap()
});
static PYX_ROCM41_INDEX_URL: LazyLock<IndexUrl> = LazyLock::new(|| {
    let api_base_url = &*PYX_API_BASE_URL;
    IndexUrl::from_str(&format!("{api_base_url}/simple/astral-sh/rocm4.1")).unwrap()
});
static PYX_ROCM401_INDEX_URL: LazyLock<IndexUrl> = LazyLock::new(|| {
    let api_base_url = &*PYX_API_BASE_URL;
    IndexUrl::from_str(&format!("{api_base_url}/simple/astral-sh/rocm4.0.1")).unwrap()
});
static PYX_XPU_INDEX_URL: LazyLock<IndexUrl> = LazyLock::new(|| {
    let api_base_url = &*PYX_API_BASE_URL;
    IndexUrl::from_str(&format!("{api_base_url}/simple/astral-sh/xpu")).unwrap()
});
